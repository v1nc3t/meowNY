//! Budgets are effective-dated. A row says "from this month on, the limit is X". The limit for
//! any month is the latest row at or before it, so a March budget still applies in June until
//! someone changes it, and old months keep their old limits. A row with a NULL limit means
//! "no budget from this month on" and keeps the history.
//!
//! An edit reaches as far as its span says. Only (the default) changes just that month. A month
//! that has already started keeps what it showed: when you edit month M and the month after it
//! has no value of its own and is not in the future, that month gets its own row holding what it
//! used to show. Future months follow the newest earlier value, so a budget set this month
//! carries forward until you change it. Forward changes that month and every month after it:
//! the later rows of that budget are dropped. All changes every month, before and after: the
//! budget becomes one row at the first month that ever had a budget (or the edited month, if
//! that is earlier).
//!
//! Two levels, for both income and expense: GLOBAL is the monthly total, CATEGORY is the part
//! of it given to one category. Allocated and unallocated amounts are computed, not stored.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::categories::{parse_id, serialize_id, CategoryType};
use crate::clock;
use crate::money::{format_amount, parse_amount};
use crate::vault::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Scope {
    Global,
    Category,
}

/// How far a budget change reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Span {
    /// Only that month.
    #[default]
    Only,
    /// That month and every month after it.
    Forward,
    /// Every month, before and after.
    All,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetBudget {
    #[serde(rename = "type")]
    pub kind: CategoryType,
    pub scope: Scope,
    /// "YYYY-MM": the budget applies from this month on.
    pub month: String,
    /// Decimal string, or null to remove the budget from this month on.
    pub amount_limit: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: CategoryType,
    pub scope: Scope,
    pub category_id: Option<String>,
    pub amount_limit: Option<String>,
    pub effective_from: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryLimit {
    pub category_id: String,
    pub amount_limit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    #[serde(rename = "type")]
    pub kind: CategoryType,
    /// The global budget for the month, null when none is set.
    pub budgeted: Option<String>,
    /// Sum of the category budgets.
    pub allocated: String,
    /// Global budget minus allocated; null without a global budget. Negative means over-allocated.
    pub unallocated: Option<String>,
    /// What the month's transactions of this type add up to.
    pub actual: String,
    /// What the month's transactions add up to in categories that still count in the month.
    pub used: String,
    /// Allocated minus used; negative when the categories spent more than they were given.
    pub not_used: String,
    /// Global budget minus actual; null without a global budget.
    pub remaining: Option<String>,
    pub over_allocated: bool,
    pub categories: Vec<CategoryLimit>,
}

/// "2026-06" to the first day of that month.
pub fn month_start(month: &str) -> Result<NaiveDate, AppError> {
    let date = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").map_err(|_| AppError::InvalidInput)?;
    if date.format("%Y-%m").to_string() != month {
        return Err(AppError::InvalidInput);
    }
    Ok(date)
}

pub(crate) fn next_month(first: NaiveDate) -> Result<NaiveDate, AppError> {
    let (year, month) = if first.month() == 12 { (first.year() + 1, 1) } else { (first.year(), first.month() + 1) };
    NaiveDate::from_ymd_opt(year, month, 1).ok_or(AppError::InvalidInput)
}

pub(crate) fn kind_sql(kind: CategoryType) -> &'static str {
    match kind {
        CategoryType::Income => "INCOME",
        CategoryType::Expense => "EXPENSE",
    }
}

fn kind_from_sql(text: &str) -> Result<CategoryType, AppError> {
    match text {
        "INCOME" => Ok(CategoryType::Income),
        "EXPENSE" => Ok(CategoryType::Expense),
        _ => Err(AppError::Internal),
    }
}

/// The limit a budget series shows in the month starting `start`: its latest row at or before it.
pub(crate) fn series_limit(conn: &Connection, kind: &str, category_id: Option<i64>, start: &str) -> Result<Option<i64>, AppError> {
    Ok(conn
        .query_row(
            "SELECT amount_limit_minor FROM budgets
             WHERE type = ?1 AND COALESCE(category_id, 0) = COALESCE(?2, 0) AND effective_from <= ?3
               AND (scope = 'GLOBAL') = (?2 IS NULL)
             ORDER BY effective_from DESC LIMIT 1",
            params![kind, category_id, start],
            |r| r.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten())
}

pub(crate) fn has_row(conn: &Connection, kind: &str, category_id: Option<i64>, start: &str) -> Result<bool, AppError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM budgets
             WHERE type = ?1 AND COALESCE(category_id, 0) = COALESCE(?2, 0) AND effective_from = ?3
               AND (scope = 'GLOBAL') = (?2 IS NULL)",
            params![kind, category_id, start],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Sets, changes or removes (null limit) a budget for `month`. Setting the same month twice replaces it.
/// `now` decides which later month counts as already started (see the module notes).
pub fn set(conn: &Connection, input: &SetBudget, now: DateTime<Utc>) -> Result<Budget, AppError> {
    set_span(conn, input, now, Span::Only)
}

/// Like `set`, but `span` says how far the change reaches (see the module notes).
pub fn set_span(conn: &Connection, input: &SetBudget, now: DateTime<Utc>, span: Span) -> Result<Budget, AppError> {
    let first = month_start(&input.month)?;
    let start = first.to_string();
    let limit = input.amount_limit.as_deref().map(parse_amount).transpose()?;
    let kind = kind_sql(input.kind);

    let timezone: String = conn.query_row("SELECT timezone FROM settings WHERE id = 1", [], |r| r.get(0))?;
    let current = clock::month_start(now, &timezone)?;
    let next = next_month(first)?;
    let next_start = next.to_string();

    // Keeps the month after this one as it was, if it has already started and would otherwise
    // start following the new value.
    let pin = |category_id: Option<i64>| -> Result<(), AppError> {
        if next <= current && !has_row(conn, kind, category_id, &next_start)? {
            let before = series_limit(conn, kind, category_id, &next_start)?;
            if before != limit {
                conn.execute(
                    "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![kind, if category_id.is_some() { "CATEGORY" } else { "GLOBAL" }, category_id, before, next_start],
                )?;
            }
        }
        Ok(())
    };

    // Checked before anything is deleted or written.
    let category_id: Option<i64> = match input.scope {
        Scope::Global => {
            if input.category_id.is_some() {
                return Err(AppError::InvalidInput);
            }
            None
        }
        Scope::Category => {
            let category_id = parse_id(input.category_id.as_deref().ok_or(AppError::InvalidInput)?)?;
            let matches: bool = conn
                .query_row(
                    "SELECT 1 FROM categories WHERE id = ?1 AND type = ?2 AND deleted_at IS NULL",
                    params![category_id, kind],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !matches {
                return Err(AppError::InvalidInput);
            }
            Some(category_id)
        }
    };

    let write = || -> Result<(i64, String), AppError> {
        let mut start = start.clone();
        match span {
            Span::Only => pin(category_id)?,
            Span::Forward => {
                conn.execute(
                    "DELETE FROM budgets
                     WHERE type = ?1 AND COALESCE(category_id, 0) = COALESCE(?2, 0) AND (scope = 'GLOBAL') = (?2 IS NULL)
                       AND effective_from > ?3",
                    params![kind, category_id, start],
                )?;
            }
            Span::All => {
                let earliest: Option<String> =
                    conn.query_row("SELECT MIN(effective_from) FROM budgets WHERE amount_limit_minor IS NOT NULL", [], |r| r.get(0))?;
                conn.execute(
                    "DELETE FROM budgets
                     WHERE type = ?1 AND COALESCE(category_id, 0) = COALESCE(?2, 0) AND (scope = 'GLOBAL') = (?2 IS NULL)",
                    params![kind, category_id],
                )?;
                if let Some(earliest) = earliest.filter(|e| *e < start) {
                    start = earliest;
                }
            }
        }

        let id: i64 = match category_id {
            None => conn.query_row(
                "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
                 VALUES (?1, 'GLOBAL', NULL, ?2, ?3)
                 ON CONFLICT (type, effective_from) WHERE scope = 'GLOBAL'
                 DO UPDATE SET amount_limit_minor = excluded.amount_limit_minor
                 RETURNING id",
                params![kind, limit, start],
                |r| r.get(0),
            )?,
            Some(category_id) => conn.query_row(
                "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
                 VALUES (?1, 'CATEGORY', ?2, ?3, ?4)
                 ON CONFLICT (category_id, effective_from)
                 DO UPDATE SET amount_limit_minor = excluded.amount_limit_minor
                 RETURNING id",
                params![kind, category_id, limit, start],
                |r| r.get(0),
            )?,
        };
        Ok((id, start))
    };

    // One savepoint, so a failed insert never leaves the rows above deleted. It also nests inside a caller's transaction.
    conn.execute_batch("SAVEPOINT set_budget")?;
    let written = write();
    conn.execute_batch(if written.is_ok() { "RELEASE set_budget" } else { "ROLLBACK TO set_budget; RELEASE set_budget" })?;
    let (id, start) = written?;

    Ok(Budget {
        id,
        kind: input.kind,
        scope: input.scope,
        category_id: input.category_id.clone(),
        amount_limit: limit.map(format_amount),
        effective_from: start,
    })
}

pub(crate) struct Effective {
    pub(crate) kind: CategoryType,
    pub(crate) scope: Scope,
    pub(crate) category_id: Option<i64>,
    pub(crate) limit_minor: i64,
}

/// The budgets in force in the month: latest row at or before it per (type, scope, category),
/// with removed (NULL) budgets dropped. A deleted category still counts in the months before the one its
/// `deleted_at` falls in (see `categories::delete_span`), and is left out from that month on.
pub(crate) fn effective(conn: &Connection, start: &str) -> Result<Vec<Effective>, AppError> {
    let after = next_month(NaiveDate::parse_from_str(start, "%Y-%m-%d").map_err(|_| AppError::Internal)?)?.to_string();
    let mut statement = conn.prepare(
        "SELECT type, scope, category_id, amount_limit_minor FROM (
           SELECT type, scope, category_id, amount_limit_minor,
                  ROW_NUMBER() OVER (
                    PARTITION BY type, scope, COALESCE(category_id, 0)
                    ORDER BY effective_from DESC
                  ) AS rank
           FROM budgets
           WHERE effective_from <= ?1
             AND (category_id IS NULL OR category_id IN (SELECT id FROM categories WHERE deleted_at IS NULL OR deleted_at >= ?2))
         )
         WHERE rank = 1 AND amount_limit_minor IS NOT NULL
         ORDER BY category_id",
    )?;
    let rows = statement.query_map([start, after.as_str()], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, i64>(3)?))
    })?;
    rows.map(|row| {
        let (kind, scope, category_id, limit_minor) = row?;
        let scope = if scope == "GLOBAL" { Scope::Global } else { Scope::Category };
        Ok(Effective { kind: kind_from_sql(&kind)?, scope, category_id, limit_minor })
    })
    .collect()
}

/// What the month's transactions of this type add up to, and the part of it in categories that still count in the month.
fn actual_minor(conn: &Connection, kind: CategoryType, start: &str, end: &str) -> Result<(i64, i64), AppError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(t.cost_minor), 0), COALESCE(SUM(CASE WHEN c.deleted_at IS NULL OR c.deleted_at >= ?3 THEN t.cost_minor END), 0)
         FROM transaction_costs t JOIN categories c ON c.id = t.category_id
         WHERE c.type = ?1 AND t.payment_date >= ?2 AND t.payment_date < ?3",
        params![kind_sql(kind), start, end],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

/// Income and expense side for one month. Categories removed from the budget are not listed.
pub fn overview(conn: &Connection, month: &str) -> Result<Vec<Overview>, AppError> {
    let first = month_start(month)?;
    let start = first.to_string();
    let end = next_month(first)?.to_string();
    let budgets = effective(conn, &start)?;

    [CategoryType::Income, CategoryType::Expense]
        .into_iter()
        .map(|kind| {
            let side: Vec<&Effective> = budgets.iter().filter(|b| b.kind == kind).collect();
            let budgeted = side.iter().find(|b| b.scope == Scope::Global).map(|b| b.limit_minor);
            let lines: Vec<&&Effective> = side.iter().filter(|b| b.scope == Scope::Category).collect();
            let allocated: i64 = lines.iter().map(|b| b.limit_minor).sum();
            let (actual, used) = actual_minor(conn, kind, &start, &end)?;
            let unallocated = budgeted.map(|b| b - allocated);
            Ok(Overview {
                kind,
                budgeted: budgeted.map(format_amount),
                allocated: format_amount(allocated),
                unallocated: unallocated.map(format_amount),
                actual: format_amount(actual),
                used: format_amount(used),
                not_used: format_amount(allocated - used),
                remaining: budgeted.map(|b| format_amount(b - actual)),
                over_allocated: unallocated.is_some_and(|u| u < 0),
                categories: lines
                    .iter()
                    .filter_map(|b| {
                        b.category_id.map(|id| CategoryLimit { category_id: id.to_string(), amount_limit: format_amount(b.limit_minor) })
                    })
                    .collect(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthHistory {
    /// "YYYY-MM".
    pub month: String,
    pub expense_budget: Option<String>,
    pub expense_actual: String,
    pub income_budget: Option<String>,
    pub income_actual: String,
}

const MAX_HISTORY_MONTHS: usize = 600;

/// One row per month from the first month that ever had a budget up to the current month, newest first.
/// Budgets are stored as dated rows, and past months never change unless edited, so this is the record.
pub fn history(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<MonthHistory>, AppError> {
    let first: Option<String> = conn.query_row(
        "SELECT MIN(effective_from) FROM budgets WHERE amount_limit_minor IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    let Some(first) = first else { return Ok(Vec::new()) };
    let first = NaiveDate::parse_from_str(&first, "%Y-%m-%d").map_err(|_| AppError::Internal)?;
    let timezone: String = conn.query_row("SELECT timezone FROM settings WHERE id = 1", [], |r| r.get(0))?;
    let current = clock::month_start(now, &timezone)?;
    if first > current {
        return Ok(Vec::new());
    }
    let after_current = next_month(current)?;

    // The global budget rows of each side, oldest first. A month shows the latest row at or before it.
    let mut limits: std::collections::HashMap<&'static str, Vec<(String, Option<i64>)>> = std::collections::HashMap::new();
    let mut statement = conn.prepare(
        "SELECT type, effective_from, amount_limit_minor FROM budgets WHERE scope = 'GLOBAL' ORDER BY effective_from",
    )?;
    for row in statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?)))? {
        let (kind, from, limit) = row?;
        limits.entry(kind_sql(kind_from_sql(&kind)?)).or_default().push((from, limit));
    }

    // What was spent and earned in every month, in one pass over the transactions.
    let mut actuals: std::collections::HashMap<(String, &'static str), i64> = std::collections::HashMap::new();
    let mut statement = conn.prepare(
        "SELECT substr(t.payment_date, 1, 7), c.type, SUM(t.cost_minor)
         FROM transaction_costs t JOIN categories c ON c.id = t.category_id
         WHERE t.payment_date >= ?1 AND t.payment_date < ?2
         GROUP BY 1, 2",
    )?;
    let range = (first.to_string(), after_current.to_string());
    for row in statement.query_map([&range.0, &range.1], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))? {
        let (month, kind, sum) = row?;
        actuals.insert((month, kind_sql(kind_from_sql(&kind)?)), sum);
    }

    let budgeted = |kind: CategoryType, start: &str| -> Option<String> {
        let rows = limits.get(kind_sql(kind))?;
        let upto = rows.partition_point(|(from, _)| from.as_str() <= start);
        rows[..upto].last().and_then(|(_, limit)| *limit).map(format_amount)
    };
    let actual = |month: &str, kind: CategoryType| format_amount(actuals.get(&(month.to_string(), kind_sql(kind))).copied().unwrap_or(0));

    let mut rows = Vec::new();
    let mut month = first;
    while month <= current && rows.len() < MAX_HISTORY_MONTHS {
        let label = month.format("%Y-%m").to_string();
        let start = month.to_string();
        rows.push(MonthHistory {
            expense_budget: budgeted(CategoryType::Expense, &start),
            expense_actual: actual(&label, CategoryType::Expense),
            income_budget: budgeted(CategoryType::Income, &start),
            income_actual: actual(&label, CategoryType::Income),
            month: label,
        });
        month = next_month(month)?;
    }
    rows.reverse();
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories;
    use crate::transactions::{self, NewTransaction};

    fn db() -> (Connection, i64, i64, i64) {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn.execute("INSERT INTO settings (id, currency, timezone) VALUES (1, 'EUR', 'UTC')", []).unwrap();
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        let rent = categories::create(&conn, "Rent", CategoryType::Expense).unwrap().id;
        let salary = categories::create(&conn, "Salary", CategoryType::Income).unwrap().id;
        (conn, food, rent, salary)
    }

    /// Early enough that every month the older tests use is still in the future: nothing is pinned.
    fn put(conn: &Connection, input: &SetBudget) -> Result<Budget, AppError> {
        set(conn, input, at("2026-01-15T12:00:00Z"))
    }

    fn put_at(conn: &Connection, input: &SetBudget, now: &str) -> Result<Budget, AppError> {
        set(conn, input, at(now))
    }

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    fn global(kind: CategoryType, month: &str, limit: Option<&str>) -> SetBudget {
        SetBudget { kind, scope: Scope::Global, month: month.into(), amount_limit: limit.map(String::from), category_id: None }
    }

    fn category(kind: CategoryType, id: i64, month: &str, limit: Option<&str>) -> SetBudget {
        SetBudget { kind, scope: Scope::Category, month: month.into(), amount_limit: limit.map(String::from), category_id: Some(id.to_string()) }
    }

    fn side(conn: &Connection, month: &str, kind: CategoryType) -> Overview {
        overview(conn, month).unwrap().into_iter().find(|o| o.kind == kind).unwrap()
    }

    fn spend(conn: &Connection, category: i64, amount: &str, date: &str) {
        transactions::create(
            conn,
            &NewTransaction {
                name: "x".into(),
                amount: amount.into(),
                payment_date: date.into(),
                category_id: category.to_string(),
                description: None,
                client_uuid: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn a_march_budget_applies_in_june_until_it_is_changed() {
        let (conn, _, _, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-03", Some("2000.00"))).unwrap();
        for month in ["2026-03", "2026-04", "2026-06", "2027-01"] {
            assert_eq!(side(&conn, month, CategoryType::Expense).budgeted.as_deref(), Some("2000.00"), "{month}");
        }
        assert_eq!(side(&conn, "2026-02", CategoryType::Expense).budgeted, None, "nothing before it starts");

        put(&conn, &global(CategoryType::Expense, "2026-07", Some("2500.00"))).unwrap();
        assert_eq!(side(&conn, "2026-06", CategoryType::Expense).budgeted.as_deref(), Some("2000.00"), "old months keep their limit");
        assert_eq!(side(&conn, "2026-07", CategoryType::Expense).budgeted.as_deref(), Some("2500.00"));
        assert_eq!(side(&conn, "2026-12", CategoryType::Expense).budgeted.as_deref(), Some("2500.00"));
    }

    #[test]
    fn category_budgets_carry_over_and_change_the_same_way() {
        let (conn, food, _, _) = db();
        put(&conn, &category(CategoryType::Expense, food, "2026-03", Some("400.00"))).unwrap();
        put(&conn, &category(CategoryType::Expense, food, "2026-05", Some("450.00"))).unwrap();
        let limit = |month: &str| side(&conn, month, CategoryType::Expense).categories.first().map(|c| c.amount_limit.clone());
        assert_eq!(limit("2026-04").as_deref(), Some("400.00"));
        assert_eq!(limit("2026-05").as_deref(), Some("450.00"));
        assert_eq!(limit("2026-09").as_deref(), Some("450.00"));
        assert_eq!(limit("2026-02"), None);
    }

    #[test]
    fn a_null_limit_removes_the_budget_from_that_month_and_keeps_history() {
        let (conn, food, _, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-03", Some("2000.00"))).unwrap();
        put(&conn, &category(CategoryType::Expense, food, "2026-03", Some("400.00"))).unwrap();

        let removed = put(&conn, &global(CategoryType::Expense, "2026-06", None)).unwrap();
        assert_eq!(removed.amount_limit, None);
        put(&conn, &category(CategoryType::Expense, food, "2026-06", None)).unwrap();

        let may = side(&conn, "2026-05", CategoryType::Expense);
        assert_eq!((may.budgeted.as_deref(), may.categories.len()), (Some("2000.00"), 1), "earlier months unchanged");
        let june = side(&conn, "2026-06", CategoryType::Expense);
        assert_eq!((june.budgeted, june.categories.len(), june.allocated.as_str()), (None, 0, "0.00"));

        // A later budget starts fresh, and the history rows are all still there.
        put(&conn, &global(CategoryType::Expense, "2026-09", Some("1800.00"))).unwrap();
        assert_eq!(side(&conn, "2026-10", CategoryType::Expense).budgeted.as_deref(), Some("1800.00"));
        let rows: i64 = conn.query_row("SELECT count(*) FROM budgets WHERE scope = 'GLOBAL'", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 3);
    }

    #[test]
    fn allocation_overview_warns_when_categories_exceed_the_global_budget() {
        let (conn, food, rent, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-06", Some("1000.00"))).unwrap();
        put(&conn, &category(CategoryType::Expense, rent, "2026-06", Some("700.00"))).unwrap();

        let within = side(&conn, "2026-06", CategoryType::Expense);
        assert_eq!((within.unallocated.as_deref(), within.over_allocated), (Some("300.00"), false));

        put(&conn, &category(CategoryType::Expense, food, "2026-06", Some("400.00"))).unwrap();
        let over = side(&conn, "2026-06", CategoryType::Expense);
        assert_eq!((over.allocated.as_str(), over.unallocated.as_deref(), over.over_allocated), ("1100.00", Some("-100.00"), true));
        assert_eq!(over.categories.len(), 2);

        // Exactly fully allocated is not a warning.
        put(&conn, &category(CategoryType::Expense, food, "2026-06", Some("300.00"))).unwrap();
        assert!(!side(&conn, "2026-06", CategoryType::Expense).over_allocated);
    }

    #[test]
    fn without_a_global_budget_unallocated_and_remaining_are_unknown() {
        let (conn, food, _, _) = db();
        put(&conn, &category(CategoryType::Expense, food, "2026-06", Some("400.00"))).unwrap();
        let o = side(&conn, "2026-06", CategoryType::Expense);
        assert_eq!((o.budgeted, o.unallocated, o.remaining, o.over_allocated), (None, None, None, false));
        assert_eq!(o.allocated, "400.00");
    }

    #[test]
    fn actual_and_remaining_count_only_that_month_and_type() {
        let (conn, food, rent, salary) = db();
        put(&conn, &global(CategoryType::Expense, "2026-06", Some("1000.00"))).unwrap();
        spend(&conn, food, "100.50", "2026-06-01");
        spend(&conn, rent, "700.00", "2026-06-30");
        spend(&conn, food, "55.00", "2026-05-31");
        spend(&conn, food, "55.00", "2026-07-01");
        spend(&conn, salary, "3000.00", "2026-06-15");

        let expense = side(&conn, "2026-06", CategoryType::Expense);
        assert_eq!((expense.actual.as_str(), expense.remaining.as_deref()), ("800.50", Some("199.50")));
        let income = side(&conn, "2026-06", CategoryType::Income);
        assert_eq!((income.actual.as_str(), income.remaining), ("3000.00", None));

        spend(&conn, food, "400.00", "2026-06-10");
        assert_eq!(side(&conn, "2026-06", CategoryType::Expense).remaining.as_deref(), Some("-200.50"), "overspent is negative");
    }

    #[test]
    fn december_rolls_into_january_for_the_month_range() {
        let (conn, food, _, _) = db();
        spend(&conn, food, "10.00", "2026-12-31");
        spend(&conn, food, "20.00", "2027-01-01");
        assert_eq!(side(&conn, "2026-12", CategoryType::Expense).actual, "10.00");
        assert_eq!(side(&conn, "2027-01", CategoryType::Expense).actual, "20.00");
    }

    #[test]
    fn invalid_budgets_are_rejected() {
        let (conn, food, _, salary) = db();
        let bad = [
            global(CategoryType::Expense, "2026-13", Some("1.00")),
            global(CategoryType::Expense, "2026-6", Some("1.00")),
            global(CategoryType::Expense, "June", Some("1.00")),
            global(CategoryType::Expense, "2026-06", Some("-1.00")),
            global(CategoryType::Expense, "2026-06", Some("5")),
            SetBudget { category_id: Some(food.to_string()), ..global(CategoryType::Expense, "2026-06", Some("1.00")) },
            category(CategoryType::Expense, 999, "2026-06", Some("1.00")),
            category(CategoryType::Income, food, "2026-06", Some("1.00")),
            category(CategoryType::Expense, salary, "2026-06", Some("1.00")),
            SetBudget { category_id: None, ..category(CategoryType::Expense, food, "2026-06", Some("1.00")) },
        ];
        for input in bad {
            assert_eq!(put(&conn, &input).err(), Some(AppError::InvalidInput), "{input:?}");
        }
        assert_eq!(overview(&conn, "2026-6").err(), Some(AppError::InvalidInput));
        let rows: i64 = conn.query_row("SELECT count(*) FROM budgets", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn zero_is_a_valid_budget_limit() {
        let (conn, _, _, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-06", Some("0.00"))).unwrap();
        assert_eq!(side(&conn, "2026-06", CategoryType::Expense).budgeted.as_deref(), Some("0.00"));
    }

    #[test]
    fn a_deleted_category_counts_in_the_months_before_the_deletion_and_not_from_it() {
        let (mut conn, food, _, _) = db();
        put(&conn, &category(CategoryType::Expense, food, "2026-03", Some("400.00"))).unwrap();
        spend(&conn, food, "30.00", "2026-05-10");
        categories::delete(&mut conn, food, "2026-06-15T10:00:00Z".parse().unwrap()).unwrap();
        let may = side(&conn, "2026-05", CategoryType::Expense);
        assert_eq!((may.categories.len(), may.allocated.as_str(), may.used.as_str()), (1, "400.00", "30.00"));
        assert_eq!(side(&conn, "2026-06", CategoryType::Expense).categories.len(), 0);
        assert_eq!(side(&conn, "2026-08", CategoryType::Expense).allocated, "0.00");
        assert_eq!(put(&conn, &category(CategoryType::Expense, food, "2026-07", Some("1.00"))).err(), Some(AppError::InvalidInput));
    }

    const LATE: &str = "2026-10-15T12:00:00Z";

    fn exp(conn: &Connection, month: &str) -> Option<String> {
        side(conn, month, CategoryType::Expense).budgeted
    }

    fn row_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT count(*) FROM budgets", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn setting_a_past_month_changes_only_that_month() {
        let (conn, _, _, _) = db();
        put_at(&conn, &global(CategoryType::Expense, "2026-03", Some("2000.00")), LATE).unwrap();
        assert_eq!(exp(&conn, "2026-03").as_deref(), Some("2000.00"));
        assert_eq!(exp(&conn, "2026-04"), None, "the months after it that already happened stay empty");
        assert_eq!(exp(&conn, "2026-10"), None);
        assert_eq!(exp(&conn, "2026-02"), None, "and nothing carries back");
    }

    #[test]
    fn editing_an_earlier_month_leaves_the_months_after_it_as_they_were() {
        let (conn, _, _, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-01", Some("2000.00"))).unwrap();
        put_at(&conn, &global(CategoryType::Expense, "2026-03", Some("2500.00")), LATE).unwrap();
        assert_eq!(exp(&conn, "2026-02").as_deref(), Some("2000.00"));
        assert_eq!(exp(&conn, "2026-03").as_deref(), Some("2500.00"));
        for month in ["2026-04", "2026-07", "2026-10", "2026-12", "2027-02"] {
            assert_eq!(exp(&conn, month).as_deref(), Some("2000.00"), "{month} is unchanged");
        }
    }

    #[test]
    fn editing_the_first_month_does_not_rewrite_the_rest_of_the_history() {
        let (conn, _, _, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-01", Some("2000.00"))).unwrap();
        put_at(&conn, &global(CategoryType::Expense, "2026-01", Some("2200.00")), LATE).unwrap();
        assert_eq!(exp(&conn, "2026-01").as_deref(), Some("2200.00"));
        assert_eq!(exp(&conn, "2026-02").as_deref(), Some("2000.00"));
        assert_eq!(exp(&conn, "2026-10").as_deref(), Some("2000.00"));
    }

    #[test]
    fn a_month_that_already_has_its_own_value_is_not_pinned_again() {
        let (conn, _, _, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-01", Some("2000.00"))).unwrap();
        put(&conn, &global(CategoryType::Expense, "2026-03", Some("2300.00"))).unwrap();
        let before = row_count(&conn);
        put_at(&conn, &global(CategoryType::Expense, "2026-02", Some("2100.00")), LATE).unwrap();
        assert_eq!(row_count(&conn), before + 1, "only the edited month gains a row");
        assert_eq!(exp(&conn, "2026-03").as_deref(), Some("2300.00"));
        assert_eq!(exp(&conn, "2026-04").as_deref(), Some("2300.00"));
    }

    #[test]
    fn no_extra_row_when_the_next_month_would_not_change() {
        let (conn, _, _, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-01", Some("2000.00"))).unwrap();
        put_at(&conn, &global(CategoryType::Expense, "2026-03", Some("2000.00")), LATE).unwrap();
        assert_eq!(row_count(&conn), 2, "the value is the same, so nothing needs pinning");
    }

    #[test]
    fn clearing_a_past_month_keeps_the_months_after_it() {
        let (conn, _, _, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-01", Some("2000.00"))).unwrap();
        put_at(&conn, &global(CategoryType::Expense, "2026-03", None), LATE).unwrap();
        assert_eq!(exp(&conn, "2026-02").as_deref(), Some("2000.00"));
        assert_eq!(exp(&conn, "2026-03"), None);
        assert_eq!(exp(&conn, "2026-04").as_deref(), Some("2000.00"));
    }

    #[test]
    fn category_budgets_are_pinned_per_category_and_independent_of_the_global_one() {
        let (conn, food, rent, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-01", Some("2000.00"))).unwrap();
        put(&conn, &category(CategoryType::Expense, food, "2026-01", Some("400.00"))).unwrap();
        put(&conn, &category(CategoryType::Expense, rent, "2026-01", Some("900.00"))).unwrap();

        put_at(&conn, &category(CategoryType::Expense, food, "2026-03", Some("450.00")), LATE).unwrap();

        let limits = |month: &str| -> Vec<(String, String)> {
            side(&conn, month, CategoryType::Expense)
                .categories
                .into_iter()
                .map(|c| (c.category_id, c.amount_limit))
                .collect()
        };
        let (f, r) = (food.to_string(), rent.to_string());
        assert_eq!(limits("2026-03"), [(f.clone(), "450.00".into()), (r.clone(), "900.00".into())]);
        assert_eq!(limits("2026-04"), [(f, "400.00".into()), (r, "900.00".into())], "food is back to its old value, rent never moved");
        assert_eq!(exp(&conn, "2026-04").as_deref(), Some("2000.00"), "the global budget is untouched");
    }

    #[test]
    fn history_lists_every_month_from_the_first_budget_to_now_newest_first() {
        let (conn, food, _, salary) = db();
        put(&conn, &global(CategoryType::Expense, "2026-08", Some("1000.00"))).unwrap();
        put(&conn, &global(CategoryType::Income, "2026-08", Some("3000.00"))).unwrap();
        put_at(&conn, &global(CategoryType::Expense, "2026-10", Some("1200.00")), LATE).unwrap();
        spend(&conn, food, "400.00", "2026-08-10");
        spend(&conn, food, "1300.00", "2026-10-02");
        spend(&conn, salary, "3000.00", "2026-09-01");

        let rows = history(&conn, at(LATE)).unwrap();
        let months: Vec<&str> = rows.iter().map(|r| r.month.as_str()).collect();
        assert_eq!(months, ["2026-10", "2026-09", "2026-08"]);

        let october = &rows[0];
        assert_eq!((october.expense_budget.as_deref(), october.expense_actual.as_str()), (Some("1200.00"), "1300.00"));
        assert_eq!(october.income_budget.as_deref(), Some("3000.00"), "income carried forward");
        let september = &rows[1];
        assert_eq!((september.expense_budget.as_deref(), september.expense_actual.as_str(), september.income_actual.as_str()), (Some("1000.00"), "0.00", "3000.00"));
        assert_eq!(rows[2].expense_actual, "400.00");
    }

    #[test]
    fn history_does_not_change_when_a_later_month_is_edited() {
        let (conn, _, _, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-08", Some("1000.00"))).unwrap();
        let before: Vec<_> = history(&conn, at(LATE)).unwrap().into_iter().filter(|r| r.month.as_str() < "2026-10").collect();
        put_at(&conn, &global(CategoryType::Expense, "2026-10", Some("5000.00")), LATE).unwrap();
        let after: Vec<_> = history(&conn, at(LATE)).unwrap().into_iter().filter(|r| r.month.as_str() < "2026-10").collect();
        assert_eq!(before, after);
    }

    #[test]
    fn history_is_empty_without_any_budget_and_ignores_removed_ones() {
        let (conn, _, _, _) = db();
        assert!(history(&conn, at(LATE)).unwrap().is_empty());
        put(&conn, &global(CategoryType::Expense, "2026-03", None)).unwrap();
        assert!(history(&conn, at(LATE)).unwrap().is_empty());
    }

    #[test]
    fn history_follows_the_timezone_for_the_current_month() {
        let (conn, _, _, _) = db();
        conn.execute("UPDATE settings SET timezone = 'Europe/Dublin'", []).unwrap();
        put(&conn, &global(CategoryType::Expense, "2026-09", Some("100.00"))).unwrap();
        let utc_evening = at("2026-09-30T23:30:00Z");
        assert_eq!(history(&conn, utc_evening).unwrap().len(), 2, "already October in Dublin");
    }

    #[test]
    fn history_matches_the_single_month_overview_for_every_month() {
        let (conn, food, rent, salary) = db();
        put(&conn, &global(CategoryType::Expense, "2026-01", Some("2000.00"))).unwrap();
        put(&conn, &global(CategoryType::Income, "2026-02", Some("3000.00"))).unwrap();
        put_at(&conn, &global(CategoryType::Expense, "2026-04", Some("2500.00")), LATE).unwrap();
        put_at(&conn, &global(CategoryType::Expense, "2026-06", None), LATE).unwrap();
        put(&conn, &global(CategoryType::Expense, "2026-08", Some("1800.00"))).unwrap();
        for (cat, amount, date) in [
            (food, "10.00", "2026-01-05"), (rent, "700.00", "2026-01-31"), (salary, "3000.00", "2026-02-01"),
            (food, "55.50", "2026-03-31"), (food, "9.99", "2026-04-01"), (rent, "700.00", "2026-05-15"),
            (salary, "3100.00", "2026-07-30"), (food, "1.00", "2026-09-30"), (food, "2.00", "2026-10-01"),
        ] {
            spend(&conn, cat, amount, date);
        }
        let rows = history(&conn, at(LATE)).unwrap();
        assert_eq!(rows.len(), 10);
        for row in &rows {
            let expense = side(&conn, &row.month, CategoryType::Expense);
            let income = side(&conn, &row.month, CategoryType::Income);
            assert_eq!(row.expense_budget, expense.budgeted, "{} expense budget", row.month);
            assert_eq!(row.expense_actual, expense.actual, "{} expense actual", row.month);
            assert_eq!(row.income_budget, income.budgeted, "{} income budget", row.month);
            assert_eq!(row.income_actual, income.actual, "{} income actual", row.month);
        }
    }

    fn rent_limit(conn: &Connection, rent: i64, month: &str) -> Option<String> {
        side(conn, month, CategoryType::Expense).categories.into_iter().find(|c| c.category_id == rent.to_string()).map(|c| c.amount_limit)
    }

    #[test]
    fn a_category_deleted_for_all_months_leaves_every_month_total() {
        let (mut conn, food, _, _) = db();
        put(&conn, &category(CategoryType::Expense, food, "2026-03", Some("100.00"))).unwrap();
        put(&conn, &category(CategoryType::Expense, food, "2026-12", Some("150.00"))).unwrap();
        spend(&conn, food, "30.00", "2026-05-10");
        spend(&conn, food, "7.00", "2026-04-02");
        categories::delete_span(&mut conn, food, "2026-10", Span::All, at(LATE)).unwrap();
        for month in ["2026-02", "2026-03", "2026-04", "2026-05", "2026-10", "2026-12"] {
            let o = side(&conn, month, CategoryType::Expense);
            assert_eq!((o.allocated.as_str(), o.used.as_str(), o.not_used.as_str(), o.categories.len()), ("0.00", "0.00", "0.00", 0), "{month}");
        }
        assert_eq!(side(&conn, "2026-05", CategoryType::Expense).actual, "30.00");
        assert_eq!(side(&conn, "2026-04", CategoryType::Expense).actual, "7.00");
        assert_eq!(listed(&conn, food, "2026-04"), None, "not listed in the month of its first payment");
        assert_eq!(listed(&conn, food, "2026-02"), None);
    }

    #[test]
    fn used_and_not_used_leave_out_deleted_categories_but_actual_keeps_them() {
        let (mut conn, food, rent, _) = db();
        put(&conn, &category(CategoryType::Expense, food, "2026-10", Some("500.00"))).unwrap();
        put(&conn, &category(CategoryType::Expense, rent, "2026-10", Some("200.00"))).unwrap();
        spend(&conn, food, "300.00", "2026-10-05");
        let o = side(&conn, "2026-10", CategoryType::Expense);
        assert_eq!((o.used.as_str(), o.not_used.as_str(), o.actual.as_str()), ("300.00", "400.00", "300.00"));

        spend(&conn, rent, "50.00", "2026-10-06");
        categories::delete(&mut conn, rent, at(LATE)).unwrap();
        let o = side(&conn, "2026-10", CategoryType::Expense);
        assert_eq!((o.allocated.as_str(), o.used.as_str(), o.not_used.as_str(), o.actual.as_str()), ("500.00", "300.00", "200.00", "350.00"));
    }

    #[test]
    fn forward_changes_the_month_and_every_month_after_it() {
        let (conn, _, rent, _) = db();
        put(&conn, &category(CategoryType::Expense, rent, "2026-09", Some("50.00"))).unwrap();
        let input = category(CategoryType::Expense, rent, "2026-08", Some("100.00"));
        set_span(&conn, &input, at(LATE), Span::Forward).unwrap();
        for month in ["2026-08", "2026-09", "2026-10", "2026-11"] {
            assert_eq!(rent_limit(&conn, rent, month).as_deref(), Some("100.00"), "{month}");
        }
        assert_eq!(rent_limit(&conn, rent, "2026-07"), None);
        assert_eq!(row_count(&conn), 1, "the 2026-09 row is gone");
    }

    #[test]
    fn all_changes_every_month_from_the_first_budget() {
        let (conn, _, rent, _) = db();
        put(&conn, &global(CategoryType::Expense, "2026-03", Some("2000.00"))).unwrap();
        put(&conn, &category(CategoryType::Expense, rent, "2026-05", Some("50.00"))).unwrap();
        put(&conn, &category(CategoryType::Expense, rent, "2026-11", Some("70.00"))).unwrap();
        let input = category(CategoryType::Expense, rent, "2026-09", Some("100.00"));
        let saved = set_span(&conn, &input, at(LATE), Span::All).unwrap();
        assert_eq!(saved.effective_from, "2026-03-01");
        for month in ["2026-03", "2026-05", "2026-09", "2026-10", "2026-12"] {
            assert_eq!(rent_limit(&conn, rent, month).as_deref(), Some("100.00"), "{month}");
        }
        assert_eq!(rent_limit(&conn, rent, "2026-02"), None);
        assert_eq!(exp(&conn, "2026-06").as_deref(), Some("2000.00"), "the global budget is untouched");
    }

    #[test]
    fn a_span_that_is_not_valid_changes_nothing() {
        let (conn, food, _, salary) = db();
        put(&conn, &category(CategoryType::Expense, food, "2026-05", Some("50.00"))).unwrap();
        for span in [Span::Forward, Span::All] {
            let bad = category(CategoryType::Expense, salary, "2026-04", Some("1.00"));
            assert_eq!(set_span(&conn, &bad, at(LATE), span).err(), Some(AppError::InvalidInput));
        }
        assert_eq!(row_count(&conn), 1);
    }

    fn limits(conn: &Connection, category: i64, months: &[&str]) -> Vec<Option<String>> {
        months.iter().map(|m| rent_limit(conn, category, m)).collect()
    }

    /// None when the category is not listed in the month, otherwise whether it shows as deleted.
    fn listed(conn: &Connection, category: i64, month: &str) -> Option<bool> {
        categories::list_in_month(conn, None, month).unwrap().into_iter().find(|c| c.id == category).map(|c| c.deleted)
    }

    #[test]
    fn deleting_only_one_month_removes_just_its_budget() {
        let (mut conn, food, _, _) = db();
        put(&conn, &category(CategoryType::Expense, food, "2026-03", Some("400.00"))).unwrap();
        // a past month, the current one and a future one
        for month in ["2026-06", "2026-10", "2026-12"] {
            categories::delete_span(&mut conn, food, month, Span::Only, at(LATE)).unwrap();
            let previous = shift(month, -1);
            let following = shift(month, 1);
            assert_eq!(limits(&conn, food, &[month]), [None], "{month} has no budget");
            assert_eq!(limits(&conn, food, &[&previous, &following]), [Some("400.00".into()), Some("400.00".into())], "{month} neighbours");
            assert_eq!(listed(&conn, food, month), Some(false), "{month}: the category stays");
        }
        assert_eq!(limits(&conn, food, &["2026-07", "2027-03"]), [Some("400.00".into()), Some("400.00".into())]);
        assert_eq!(limits(&conn, food, &["2026-06", "2026-10", "2026-12"]), [None, None, None]);
    }

    fn shift(month: &str, by: i32) -> String {
        let first = month_start(month).unwrap();
        let moved = if by > 0 { next_month(first).unwrap() } else { first - chrono::Months::new(1) };
        moved.format("%Y-%m").to_string()
    }

    #[test]
    fn deleting_from_a_past_month_keeps_the_months_before_with_budget_and_spending() {
        let (mut conn, food, _, _) = db();
        put(&conn, &category(CategoryType::Expense, food, "2026-06", Some("400.00"))).unwrap();
        spend(&conn, food, "30.00", "2026-07-10");
        spend(&conn, food, "20.00", "2026-09-10");
        categories::delete_span(&mut conn, food, "2026-08", Span::Forward, at(LATE)).unwrap();

        let july = side(&conn, "2026-07", CategoryType::Expense);
        assert_eq!((july.allocated.as_str(), july.used.as_str(), july.categories.len()), ("400.00", "30.00", 1));
        for month in ["2026-08", "2026-09", "2026-10"] {
            let o = side(&conn, month, CategoryType::Expense);
            assert_eq!((o.allocated.as_str(), o.used.as_str(), o.categories.len()), ("0.00", "0.00", 0), "{month}");
        }
        assert_eq!(side(&conn, "2026-09", CategoryType::Expense).actual, "20.00", "actual keeps everything");
        assert_eq!(listed(&conn, food, "2026-07"), Some(true));
        assert_eq!(listed(&conn, food, "2026-09"), None);
        assert!(categories::list(&conn, None).unwrap().iter().all(|c| c.id != food), "gone from the active list");
    }

    #[test]
    fn deleting_from_a_future_month_keeps_the_category_and_ends_its_budget_there() {
        let (mut conn, food, _, _) = db();
        put(&conn, &category(CategoryType::Expense, food, "2026-03", Some("400.00"))).unwrap();
        put(&conn, &category(CategoryType::Expense, food, "2027-02", Some("500.00"))).unwrap();
        categories::delete_span(&mut conn, food, "2026-12", Span::Forward, at(LATE)).unwrap();
        assert!(categories::list(&conn, None).unwrap().iter().any(|c| c.id == food), "still alive");
        assert_eq!(limits(&conn, food, &["2026-10", "2026-11"]), [Some("400.00".into()), Some("400.00".into())]);
        assert_eq!(limits(&conn, food, &["2026-12", "2027-02", "2027-06"]), [None, None, None]);
        assert_eq!(listed(&conn, food, "2026-11"), Some(false));
    }

    #[test]
    fn deleting_for_all_months_removes_the_budget_and_the_spending_everywhere() {
        let (mut conn, food, _, _) = db();
        put(&conn, &category(CategoryType::Expense, food, "2026-06", Some("400.00"))).unwrap();
        spend(&conn, food, "30.00", "2026-04-20");
        categories::delete_span(&mut conn, food, "2026-09", Span::All, at(LATE)).unwrap();
        for month in ["2026-03", "2026-04", "2026-05", "2026-06", "2026-09", "2026-12"] {
            let o = side(&conn, month, CategoryType::Expense);
            assert_eq!((o.allocated.as_str(), o.used.as_str()), ("0.00", "0.00"), "{month}");
        }
        assert_eq!(side(&conn, "2026-04", CategoryType::Expense).actual, "30.00");
        let rows: i64 = conn.query_row("SELECT count(*) FROM budgets WHERE category_id = ?1", [food], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn deleting_for_a_period_checks_the_category_and_the_month_first() {
        let (mut conn, food, _, _) = db();
        assert_eq!(categories::delete_span(&mut conn, 999, "2026-06", Span::All, at(LATE)), Err(AppError::NotFound));
        assert_eq!(categories::delete_span(&mut conn, food, "2026-6", Span::Only, at(LATE)), Err(AppError::InvalidInput));
        categories::delete(&mut conn, food, at(LATE)).unwrap();
        assert_eq!(categories::delete_span(&mut conn, food, "2026-06", Span::Only, at(LATE)), Err(AppError::NotFound));
    }
}
