//! Budgets are effective-dated. A row says "from this month on, the limit is X". The limit for
//! any month is the latest row at or before it, so a March budget still applies in June until
//! someone changes it, and old months keep their old limits. A row with a NULL limit means
//! "no budget from this month on" and keeps the history.
//!
//! Two levels, for both income and expense: GLOBAL is the monthly total, CATEGORY is the part
//! of it given to one category. Allocated and unallocated amounts are computed, not stored.

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::categories::{parse_id, serialize_id, CategoryType};
use crate::money::{format_amount, parse_amount};
use crate::vault::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Scope {
    Global,
    Category,
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

fn next_month(first: NaiveDate) -> Result<NaiveDate, AppError> {
    let (year, month) = if first.month() == 12 { (first.year() + 1, 1) } else { (first.year(), first.month() + 1) };
    NaiveDate::from_ymd_opt(year, month, 1).ok_or(AppError::InvalidInput)
}

fn kind_sql(kind: CategoryType) -> &'static str {
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

/// Sets, changes or removes (null limit) a budget from `month` on. Setting the same month twice replaces it.
pub fn set(conn: &Connection, input: &SetBudget) -> Result<Budget, AppError> {
    let start = month_start(&input.month)?.to_string();
    let limit = input.amount_limit.as_deref().map(parse_amount).transpose()?;
    let kind = kind_sql(input.kind);

    let id: i64 = match input.scope {
        Scope::Global => {
            if input.category_id.is_some() {
                return Err(AppError::InvalidInput);
            }
            conn.query_row(
                "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
                 VALUES (?1, 'GLOBAL', NULL, ?2, ?3)
                 ON CONFLICT (type, effective_from) WHERE scope = 'GLOBAL'
                 DO UPDATE SET amount_limit_minor = excluded.amount_limit_minor
                 RETURNING id",
                params![kind, limit, start],
                |r| r.get(0),
            )?
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
            conn.query_row(
                "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
                 VALUES (?1, 'CATEGORY', ?2, ?3, ?4)
                 ON CONFLICT (category_id, effective_from)
                 DO UPDATE SET amount_limit_minor = excluded.amount_limit_minor
                 RETURNING id",
                params![kind, category_id, limit, start],
                |r| r.get(0),
            )?
        }
    };

    Ok(Budget {
        id,
        kind: input.kind,
        scope: input.scope,
        category_id: input.category_id.clone(),
        amount_limit: limit.map(format_amount),
        effective_from: start,
    })
}

struct Effective {
    kind: CategoryType,
    scope: Scope,
    category_id: Option<i64>,
    limit_minor: i64,
}

/// The budgets in force in the month: latest row at or before it per (type, scope, category),
/// with removed (NULL) budgets dropped.
fn effective(conn: &Connection, start: &str) -> Result<Vec<Effective>, AppError> {
    let mut statement = conn.prepare(
        "SELECT type, scope, category_id, amount_limit_minor FROM (
           SELECT type, scope, category_id, amount_limit_minor,
                  ROW_NUMBER() OVER (
                    PARTITION BY type, scope, COALESCE(category_id, 0)
                    ORDER BY effective_from DESC
                  ) AS rank
           FROM budgets
           WHERE effective_from <= ?1
         )
         WHERE rank = 1 AND amount_limit_minor IS NOT NULL
         ORDER BY category_id",
    )?;
    let rows = statement.query_map([start], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, i64>(3)?))
    })?;
    rows.map(|row| {
        let (kind, scope, category_id, limit_minor) = row?;
        let scope = if scope == "GLOBAL" { Scope::Global } else { Scope::Category };
        Ok(Effective { kind: kind_from_sql(&kind)?, scope, category_id, limit_minor })
    })
    .collect()
}

fn actual_minor(conn: &Connection, kind: CategoryType, start: &str, end: &str) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(t.amount_minor), 0)
         FROM transactions t JOIN categories c ON c.id = t.category_id
         WHERE c.type = ?1 AND t.payment_date >= ?2 AND t.payment_date < ?3",
        params![kind_sql(kind), start, end],
        |r| r.get(0),
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
            let actual = actual_minor(conn, kind, &start, &end)?;
            let unallocated = budgeted.map(|b| b - allocated);
            Ok(Overview {
                kind,
                budgeted: budgeted.map(format_amount),
                allocated: format_amount(allocated),
                unallocated: unallocated.map(format_amount),
                actual: format_amount(actual),
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
        set(&conn, &global(CategoryType::Expense, "2026-03", Some("2000.00"))).unwrap();
        for month in ["2026-03", "2026-04", "2026-06", "2027-01"] {
            assert_eq!(side(&conn, month, CategoryType::Expense).budgeted.as_deref(), Some("2000.00"), "{month}");
        }
        assert_eq!(side(&conn, "2026-02", CategoryType::Expense).budgeted, None, "nothing before it starts");

        set(&conn, &global(CategoryType::Expense, "2026-07", Some("2500.00"))).unwrap();
        assert_eq!(side(&conn, "2026-06", CategoryType::Expense).budgeted.as_deref(), Some("2000.00"), "old months keep their limit");
        assert_eq!(side(&conn, "2026-07", CategoryType::Expense).budgeted.as_deref(), Some("2500.00"));
        assert_eq!(side(&conn, "2026-12", CategoryType::Expense).budgeted.as_deref(), Some("2500.00"));
    }

    #[test]
    fn category_budgets_carry_over_and_change_the_same_way() {
        let (conn, food, _, _) = db();
        set(&conn, &category(CategoryType::Expense, food, "2026-03", Some("400.00"))).unwrap();
        set(&conn, &category(CategoryType::Expense, food, "2026-05", Some("450.00"))).unwrap();
        let limit = |month: &str| side(&conn, month, CategoryType::Expense).categories.first().map(|c| c.amount_limit.clone());
        assert_eq!(limit("2026-04").as_deref(), Some("400.00"));
        assert_eq!(limit("2026-05").as_deref(), Some("450.00"));
        assert_eq!(limit("2026-09").as_deref(), Some("450.00"));
        assert_eq!(limit("2026-02"), None);
    }

    #[test]
    fn a_null_limit_removes_the_budget_from_that_month_and_keeps_history() {
        let (conn, food, _, _) = db();
        set(&conn, &global(CategoryType::Expense, "2026-03", Some("2000.00"))).unwrap();
        set(&conn, &category(CategoryType::Expense, food, "2026-03", Some("400.00"))).unwrap();

        let removed = set(&conn, &global(CategoryType::Expense, "2026-06", None)).unwrap();
        assert_eq!(removed.amount_limit, None);
        set(&conn, &category(CategoryType::Expense, food, "2026-06", None)).unwrap();

        let may = side(&conn, "2026-05", CategoryType::Expense);
        assert_eq!((may.budgeted.as_deref(), may.categories.len()), (Some("2000.00"), 1), "earlier months unchanged");
        let june = side(&conn, "2026-06", CategoryType::Expense);
        assert_eq!((june.budgeted, june.categories.len(), june.allocated.as_str()), (None, 0, "0.00"));

        // A later budget starts fresh, and the history rows are all still there.
        set(&conn, &global(CategoryType::Expense, "2026-09", Some("1800.00"))).unwrap();
        assert_eq!(side(&conn, "2026-10", CategoryType::Expense).budgeted.as_deref(), Some("1800.00"));
        let rows: i64 = conn.query_row("SELECT count(*) FROM budgets WHERE scope = 'GLOBAL'", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 3);
    }

    #[test]
    fn income_and_expense_budgets_are_independent() {
        let (conn, _, _, salary) = db();
        set(&conn, &global(CategoryType::Income, "2026-03", Some("3200.00"))).unwrap();
        set(&conn, &global(CategoryType::Expense, "2026-03", Some("2000.00"))).unwrap();
        set(&conn, &category(CategoryType::Income, salary, "2026-03", Some("3000.00"))).unwrap();
        let income = side(&conn, "2026-04", CategoryType::Income);
        assert_eq!((income.budgeted.as_deref(), income.allocated.as_str(), income.unallocated.as_deref()), (Some("3200.00"), "3000.00", Some("200.00")));
        let expense = side(&conn, "2026-04", CategoryType::Expense);
        assert_eq!((expense.budgeted.as_deref(), expense.allocated.as_str()), (Some("2000.00"), "0.00"));
    }

    #[test]
    fn setting_the_same_month_again_replaces_it() {
        let (conn, food, _, _) = db();
        let first = set(&conn, &global(CategoryType::Expense, "2026-03", Some("2000.00"))).unwrap();
        let second = set(&conn, &global(CategoryType::Expense, "2026-03", Some("2100.00"))).unwrap();
        assert_eq!(first.id, second.id);
        set(&conn, &category(CategoryType::Expense, food, "2026-03", Some("1.00"))).unwrap();
        set(&conn, &category(CategoryType::Expense, food, "2026-03", Some("2.00"))).unwrap();
        let rows: i64 = conn.query_row("SELECT count(*) FROM budgets", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 2);
        assert_eq!(side(&conn, "2026-03", CategoryType::Expense).budgeted.as_deref(), Some("2100.00"));
    }

    #[test]
    fn allocation_overview_warns_when_categories_exceed_the_global_budget() {
        let (conn, food, rent, _) = db();
        set(&conn, &global(CategoryType::Expense, "2026-06", Some("1000.00"))).unwrap();
        set(&conn, &category(CategoryType::Expense, rent, "2026-06", Some("700.00"))).unwrap();

        let within = side(&conn, "2026-06", CategoryType::Expense);
        assert_eq!((within.unallocated.as_deref(), within.over_allocated), (Some("300.00"), false));

        set(&conn, &category(CategoryType::Expense, food, "2026-06", Some("400.00"))).unwrap();
        let over = side(&conn, "2026-06", CategoryType::Expense);
        assert_eq!((over.allocated.as_str(), over.unallocated.as_deref(), over.over_allocated), ("1100.00", Some("-100.00"), true));
        assert_eq!(over.categories.len(), 2);

        // Exactly fully allocated is not a warning.
        set(&conn, &category(CategoryType::Expense, food, "2026-06", Some("300.00"))).unwrap();
        assert!(!side(&conn, "2026-06", CategoryType::Expense).over_allocated);
    }

    #[test]
    fn without_a_global_budget_unallocated_and_remaining_are_unknown() {
        let (conn, food, _, _) = db();
        set(&conn, &category(CategoryType::Expense, food, "2026-06", Some("400.00"))).unwrap();
        let o = side(&conn, "2026-06", CategoryType::Expense);
        assert_eq!((o.budgeted, o.unallocated, o.remaining, o.over_allocated), (None, None, None, false));
        assert_eq!(o.allocated, "400.00");
    }

    #[test]
    fn actual_and_remaining_count_only_that_month_and_type() {
        let (conn, food, rent, salary) = db();
        set(&conn, &global(CategoryType::Expense, "2026-06", Some("1000.00"))).unwrap();
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
            assert_eq!(set(&conn, &input).err(), Some(AppError::InvalidInput), "{input:?}");
        }
        assert_eq!(overview(&conn, "2026-6").err(), Some(AppError::InvalidInput));
        let rows: i64 = conn.query_row("SELECT count(*) FROM budgets", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn zero_is_a_valid_budget_limit() {
        let (conn, _, _, _) = db();
        set(&conn, &global(CategoryType::Expense, "2026-06", Some("0.00"))).unwrap();
        assert_eq!(side(&conn, "2026-06", CategoryType::Expense).budgeted.as_deref(), Some("0.00"));
    }

    #[test]
    fn a_deleted_category_drops_out_of_later_months_but_stays_in_earlier_ones() {
        let (mut conn, food, _, _) = db();
        set(&conn, &category(CategoryType::Expense, food, "2026-03", Some("400.00"))).unwrap();
        categories::delete(&mut conn, food, "2026-06-15T10:00:00Z".parse().unwrap()).unwrap();
        assert_eq!(side(&conn, "2026-05", CategoryType::Expense).categories.len(), 1);
        assert_eq!(side(&conn, "2026-06", CategoryType::Expense).categories.len(), 0);
        assert_eq!(side(&conn, "2026-08", CategoryType::Expense).allocated, "0.00");
        assert_eq!(set(&conn, &category(CategoryType::Expense, food, "2026-07", Some("1.00"))).err(), Some(AppError::InvalidInput));
    }

    #[test]
    fn wire_format_matches_the_client() {
        let (conn, _, _, _) = db();
        let budget = set(&conn, &global(CategoryType::Expense, "2026-06", Some("12.00"))).unwrap();
        let json = serde_json::to_value(&budget).unwrap();
        assert_eq!(json["type"], "EXPENSE");
        assert_eq!(json["scope"], "GLOBAL");
        assert_eq!(json["amountLimit"], "12.00");
        assert_eq!(json["effectiveFrom"], "2026-06-01");
        assert!(json["categoryId"].is_null());
        let overview = serde_json::to_value(side(&conn, "2026-06", CategoryType::Expense)).unwrap();
        assert_eq!(overview["overAllocated"], false);
        assert!(overview["categories"].is_array());
        assert!(serde_json::from_str::<SetBudget>(r#"{"type":"EXPENSE","scope":"GLOBAL","month":"2026-06","amountLimit":null,"userId":1}"#).is_err());
    }
}
