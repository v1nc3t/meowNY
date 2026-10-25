//! Categories. Deleting is soft: the row stays so old transactions keep their category, and a
//! budget row with a NULL limit is written from the current month so the category stops being
//! budgeted without losing its budget history (schema design decision 3).
//!
//! `deleted_at` marks the month a category is gone from: a deleted category still counts in month S
//! (its budget in `allocated`, its spending in `used`) when `deleted_at` is at or after the start of
//! the month after S. The plain `delete` sets it to now, so the category counts in the months before
//! the current one. `delete_span` can set it to any month, or leave the category alive for one month.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize, Serializer};

use crate::budgets::{self, Span};
use crate::clock;
use crate::vault::AppError;

const NAME_MAX_CHARS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CategoryType {
    Income,
    Expense,
}

impl CategoryType {
    fn as_sql(self) -> &'static str {
        match self {
            CategoryType::Income => "INCOME",
            CategoryType::Expense => "EXPENSE",
        }
    }

    fn from_sql(text: &str) -> Result<Self, AppError> {
        match text {
            "INCOME" => Ok(CategoryType::Income),
            "EXPENSE" => Ok(CategoryType::Expense),
            _ => Err(AppError::Internal),
        }
    }
}

/// Ids cross the UI boundary as strings, as the old API did, so they never lose precision.
pub fn serialize_id<S: Serializer>(id: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&id.to_string())
}

pub fn parse_id(text: &str) -> Result<i64, AppError> {
    match text.parse::<i64>() {
        Ok(id) if id > 0 && text.bytes().all(|b| b.is_ascii_digit()) => Ok(id),
        _ => Err(AppError::InvalidInput),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Category {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: CategoryType,
}

/// A category as the budget page of one month shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoryInMonth {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: CategoryType,
    /// Deleted from a later month on: it still counts in this one, and can only be looked at.
    pub deleted: bool,
}

pub(crate) fn clean_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    let length = name.chars().count();
    if length == 0 || length > NAME_MAX_CHARS || name.chars().any(char::is_control) {
        return Err(AppError::InvalidInput);
    }
    Ok(name.to_string())
}

fn is_unique_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(e, _) if e.code == rusqlite::ErrorCode::ConstraintViolation
            && e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

fn write_error(error: rusqlite::Error) -> AppError {
    if is_unique_violation(&error) {
        AppError::NameExists
    } else {
        AppError::from(error)
    }
}

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<(i64, String, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

fn build(row: (i64, String, String)) -> Result<Category, AppError> {
    Ok(Category { id: row.0, name: row.1, kind: CategoryType::from_sql(&row.2)? })
}

/// Active categories, oldest first, optionally of one type.
pub fn list(conn: &Connection, kind: Option<CategoryType>) -> Result<Vec<Category>, AppError> {
    let mut statement = conn.prepare(
        "SELECT id, name, type FROM categories
         WHERE deleted_at IS NULL AND (?1 IS NULL OR type = ?1)
         ORDER BY id",
    )?;
    let rows = statement.query_map([kind.map(CategoryType::as_sql)], from_row)?;
    rows.map(|row| build(row?)).collect()
}

/// The categories that count in `month` ("YYYY-MM"), oldest first: the active ones, and the ones deleted from a later month on
/// that still have a budget or a transaction in it.
pub fn list_in_month(conn: &Connection, kind: Option<CategoryType>, month: &str) -> Result<Vec<CategoryInMonth>, AppError> {
    let first = budgets::month_start(month)?;
    let start = first.to_string();
    let after = budgets::next_month(first)?.to_string();
    let mut statement = conn.prepare(
        "SELECT c.id, c.name, c.type, c.deleted_at IS NOT NULL FROM categories c
         WHERE (?1 IS NULL OR c.type = ?1)
           AND (c.deleted_at IS NULL OR (c.deleted_at >= ?2 AND (
                 EXISTS (SELECT 1 FROM transaction_costs t WHERE t.category_id = c.id AND t.payment_date >= ?3 AND t.payment_date < ?2)
                 OR (SELECT b.amount_limit_minor FROM budgets b WHERE b.category_id = c.id AND b.effective_from <= ?3
                     ORDER BY b.effective_from DESC LIMIT 1) IS NOT NULL)))
         ORDER BY c.id",
    )?;
    let rows = statement.query_map(params![kind.map(CategoryType::as_sql), after, start], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, bool>(3)?))
    })?;
    rows.map(|row| {
        let (id, name, kind, deleted) = row?;
        Ok(CategoryInMonth { id, name, kind: CategoryType::from_sql(&kind)?, deleted })
    })
    .collect()
}

pub fn create(conn: &Connection, name: &str, kind: CategoryType) -> Result<Category, AppError> {
    let name = clean_name(name)?;
    conn.execute("INSERT INTO categories (name, type) VALUES (?1, ?2)", params![name, kind.as_sql()])
        .map_err(write_error)?;
    Ok(Category { id: conn.last_insert_rowid(), name, kind })
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<Category, AppError> {
    let name = clean_name(name)?;
    let changed = conn
        .execute("UPDATE categories SET name = ?1 WHERE id = ?2 AND deleted_at IS NULL", params![name, id])
        .map_err(write_error)?;
    if changed == 0 {
        return Err(AppError::NotFound);
    }
    let row = conn
        .query_row("SELECT id, name, type FROM categories WHERE id = ?1", [id], from_row)
        .optional()?
        .ok_or(AppError::NotFound)?;
    build(row)
}

/// Soft delete plus the NULL budget row, in one transaction. `now` is passed in so tests control the month.
pub fn delete(conn: &mut Connection, id: i64, now: DateTime<Utc>) -> Result<(), AppError> {
    let tx = conn.transaction()?;
    let timezone: String = tx.query_row("SELECT timezone FROM settings WHERE id = 1", [], |r| r.get(0))?;
    let month = clock::month_start(now, &timezone)?.to_string();
    let stamp = now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

    let kind: Option<String> = tx
        .query_row(
            "UPDATE categories SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL RETURNING type",
            params![stamp, id],
            |r| r.get(0),
        )
        .optional()?;
    let kind = kind.ok_or(AppError::NotFound)?;

    tx.execute(
        "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
         VALUES (?1, 'CATEGORY', ?2, NULL, ?3)
         ON CONFLICT (category_id, effective_from) DO UPDATE SET amount_limit_minor = NULL",
        params![kind, id, month],
    )?;
    tx.commit()?;
    Ok(())
}

/// Deletes a category for a period, seen from `month` ("YYYY-MM"), in one transaction. Only: the category stays and just its
/// budget for that month goes. Forward: it is gone from that month on (months before keep its budget and spending); a month
/// that has not started yet only loses its budget, as the category is still alive. All: it is gone from every month.
pub fn delete_span(conn: &mut Connection, id: i64, month: &str, span: Span, now: DateTime<Utc>) -> Result<(), AppError> {
    let first = budgets::month_start(month)?;
    let start = first.to_string();
    let tx = conn.transaction()?;
    let timezone: String = tx.query_row("SELECT timezone FROM settings WHERE id = 1", [], |r| r.get(0))?;
    let current = clock::month_start(now, &timezone)?;
    let kind: String = tx
        .query_row("SELECT type FROM categories WHERE id = ?1 AND deleted_at IS NULL", [id], |r| r.get(0))
        .optional()?
        .ok_or(AppError::NotFound)?;

    let null_row = |month: &str| -> Result<(), AppError> {
        tx.execute(
            "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
             VALUES (?1, 'CATEGORY', ?2, NULL, ?3)
             ON CONFLICT (category_id, effective_from) DO UPDATE SET amount_limit_minor = NULL",
            params![kind, id, month],
        )?;
        Ok(())
    };

    match span {
        Span::Only => {
            // The month after keeps what it showed, even when it has not started: "only this month" must not leak forward.
            let next = budgets::next_month(first)?.to_string();
            if !budgets::has_row(&tx, &kind, Some(id), &next)? {
                if let Some(limit) = budgets::series_limit(&tx, &kind, Some(id), &next)? {
                    tx.execute(
                        "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
                         VALUES (?1, 'CATEGORY', ?2, ?3, ?4)",
                        params![kind, id, limit, next],
                    )?;
                }
            }
            null_row(&start)?;
        }
        Span::Forward if first > current => {
            null_row(&start)?;
            tx.execute("DELETE FROM budgets WHERE category_id = ?1 AND effective_from > ?2", params![id, start])?;
        }
        Span::Forward => {
            tx.execute("UPDATE categories SET deleted_at = ?1 WHERE id = ?2", params![format!("{start}T00:00:00.000Z"), id])?;
            null_row(&start)?;
        }
        Span::All => {
            // The earliest month that has anything of the category, so that no month counts it. Never later than this month
            // (the category is gone now, whichever month the question was asked in).
            let earliest_budget: Option<String> = tx.query_row(
                "SELECT MIN(effective_from) FROM budgets WHERE category_id = ?1 AND amount_limit_minor IS NOT NULL",
                [id],
                |r| r.get(0),
            )?;
            let earliest_payment: Option<String> =
                tx.query_row("SELECT MIN(payment_date) FROM transaction_costs WHERE category_id = ?1", [id], |r| r.get(0))?;
            let gone_from = [Some(start.clone()), Some(current.to_string()), earliest_budget, earliest_payment.map(|d| format!("{}-01", &d[..7]))]
                .into_iter()
                .flatten()
                .min()
                .unwrap_or(start);
            tx.execute("UPDATE categories SET deleted_at = ?1 WHERE id = ?2", params![format!("{gone_from}T00:00:00.000Z"), id])?;
            tx.execute("DELETE FROM budgets WHERE category_id = ?1", [id])?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn.execute("INSERT INTO settings (id, currency, timezone) VALUES (1, 'EUR', 'Europe/Dublin')", []).unwrap();
        conn
    }

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    fn names(conn: &Connection, kind: Option<CategoryType>) -> Vec<String> {
        list(conn, kind).unwrap().into_iter().map(|c| c.name).collect()
    }

    #[test]
    fn names_are_trimmed_and_validated() {
        let conn = db();
        assert_eq!(create(&conn, "  Food  ", CategoryType::Expense).unwrap().name, "Food");
        for bad in ["", "   ", &"x".repeat(51), "bad\nname"] {
            assert_eq!(create(&conn, bad, CategoryType::Expense), Err(AppError::InvalidInput), "{bad:?}");
        }
        assert!(create(&conn, &"é".repeat(50), CategoryType::Expense).is_ok(), "50 characters, not bytes");
    }

    #[test]
    fn duplicate_active_name_is_a_name_conflict_but_other_type_is_fine() {
        let conn = db();
        create(&conn, "Food", CategoryType::Expense).unwrap();
        assert_eq!(create(&conn, "Food", CategoryType::Expense), Err(AppError::NameExists));
        assert!(create(&conn, "Food", CategoryType::Income).is_ok());
    }

    #[test]
    fn delete_is_soft_and_writes_a_null_budget_row_for_the_current_month() {
        let mut conn = db();
        let food = create(&conn, "Food", CategoryType::Expense).unwrap();
        conn.execute(
            "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
             VALUES ('EXPENSE', 'CATEGORY', ?1, 45000, '2026-01-01')",
            [food.id],
        )
        .unwrap();

        delete(&mut conn, food.id, at("2026-06-15T10:00:00Z")).unwrap();

        assert!(names(&conn, None).is_empty());
        let (still_there, deleted_at): (i64, Option<String>) = conn
            .query_row("SELECT count(*), max(deleted_at) FROM categories WHERE id = ?1", [food.id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(still_there, 1);
        assert!(deleted_at.is_some());

        let rows: Vec<(String, Option<i64>)> = conn
            .prepare("SELECT effective_from, amount_limit_minor FROM budgets WHERE category_id = ?1 ORDER BY effective_from")
            .unwrap()
            .query_map([food.id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, [("2026-01-01".to_string(), Some(45000)), ("2026-06-01".to_string(), None)]);
    }

    #[test]
    fn delete_uses_the_timezone_for_the_month() {
        let mut conn = db();
        let food = create(&conn, "Food", CategoryType::Expense).unwrap();
        // 23:30 UTC on 30 September is already October in Dublin.
        delete(&mut conn, food.id, at("2026-09-30T23:30:00Z")).unwrap();
        let month: String = conn
            .query_row("SELECT effective_from FROM budgets WHERE category_id = ?1", [food.id], |r| r.get(0))
            .unwrap();
        assert_eq!(month, "2026-10-01");
    }

    #[test]
    fn deleting_twice_in_a_month_keeps_one_null_row() {
        let mut conn = db();
        let food = create(&conn, "Food", CategoryType::Expense).unwrap();
        // A budget set later in the same month, after the delete, is replaced by the delete only if it exists first.
        conn.execute(
            "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
             VALUES ('EXPENSE', 'CATEGORY', ?1, 100, '2026-06-01')",
            [food.id],
        )
        .unwrap();
        delete(&mut conn, food.id, at("2026-06-20T10:00:00Z")).unwrap();
        let limit: Option<i64> = conn
            .query_row("SELECT amount_limit_minor FROM budgets WHERE category_id = ?1", [food.id], |r| r.get(0))
            .unwrap();
        assert_eq!(limit, None, "an existing row for the month is overwritten with NULL");
        assert_eq!(delete(&mut conn, food.id, at("2026-06-21T10:00:00Z")), Err(AppError::NotFound));
    }

    #[test]
    fn deleted_categories_cannot_be_renamed_and_free_their_name() {
        let mut conn = db();
        let food = create(&conn, "Food", CategoryType::Expense).unwrap();
        delete(&mut conn, food.id, at("2026-06-15T10:00:00Z")).unwrap();
        assert_eq!(rename(&conn, food.id, "Other"), Err(AppError::NotFound));
        assert!(create(&conn, "Food", CategoryType::Expense).is_ok());
    }

    #[test]
    fn delete_of_a_missing_category_changes_nothing() {
        let mut conn = db();
        assert_eq!(delete(&mut conn, 42, at("2026-06-15T10:00:00Z")), Err(AppError::NotFound));
        let budgets: i64 = conn.query_row("SELECT count(*) FROM budgets", [], |r| r.get(0)).unwrap();
        assert_eq!(budgets, 0);
    }

}
