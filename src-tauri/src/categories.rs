//! Categories. Deleting is soft: the row stays so old transactions keep their category, and a
//! budget row with a NULL limit is written from the current month so the category stops being
//! budgeted without losing its budget history (schema design decision 3).

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize, Serializer};

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
    fn create_and_list_in_order_with_type_filter() {
        let conn = db();
        create(&conn, "Salary", CategoryType::Income).unwrap();
        create(&conn, "Food", CategoryType::Expense).unwrap();
        create(&conn, "Rent", CategoryType::Expense).unwrap();
        assert_eq!(names(&conn, None), ["Salary", "Food", "Rent"]);
        assert_eq!(names(&conn, Some(CategoryType::Expense)), ["Food", "Rent"]);
        assert_eq!(names(&conn, Some(CategoryType::Income)), ["Salary"]);
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
    fn rename_changes_the_name_and_reports_conflicts_and_missing() {
        let conn = db();
        let food = create(&conn, "Food", CategoryType::Expense).unwrap();
        create(&conn, "Rent", CategoryType::Expense).unwrap();
        assert_eq!(rename(&conn, food.id, "Groceries").unwrap().name, "Groceries");
        assert_eq!(rename(&conn, food.id, "Rent"), Err(AppError::NameExists));
        assert_eq!(rename(&conn, 999, "Nope"), Err(AppError::NotFound));
        assert_eq!(rename(&conn, food.id, " "), Err(AppError::InvalidInput));
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

    #[test]
    fn ids_are_strings_on_the_wire_and_parsed_strictly() {
        let conn = db();
        let food = create(&conn, "Food", CategoryType::Expense).unwrap();
        let json = serde_json::to_string(&food).unwrap();
        assert_eq!(json, format!(r#"{{"id":"{}","name":"Food","type":"EXPENSE"}}"#, food.id));
        assert_eq!(parse_id("12"), Ok(12));
        for bad in ["0", "-1", "1.5", "abc", "", "+3", " 4"] {
            assert_eq!(parse_id(bad), Err(AppError::InvalidInput), "{bad:?}");
        }
    }
}
