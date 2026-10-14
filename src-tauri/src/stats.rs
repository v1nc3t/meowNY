//! Numbers for the charts. Budget lookup is the same code the budget screens use
//! (`budgets::effective`, `budgets::series_limit`), so figures never disagree between screens.
//! Money leaves as two-decimal strings.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::budgets::{self, kind_sql, next_month, month_start, Overview, Scope};
use crate::categories::{serialize_id, CategoryType};
use crate::money::format_amount;
use crate::vault::AppError;

const MAX_TREND_MONTHS: usize = 120;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryLine {
    #[serde(serialize_with = "serialize_id")]
    pub category_id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: CategoryType,
    /// The category's budget for the month; null when it has none.
    pub budgeted: Option<String>,
    pub actual: String,
    /// Budget minus actual; null without a budget. Negative means over budget.
    pub remaining: Option<String>,
    /// Share of the budget used, with one decimal (89.1); null without a budget.
    pub pct_used: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BudgetVsActual {
    /// "YYYY-MM".
    pub month: String,
    pub categories: Vec<CategoryLine>,
    /// Income and expense sides: global budget, allocated, unallocated, actual, remaining.
    pub totals: Vec<Overview>,
}

/// Percent with one decimal, rounded half up, from integer amounts.
fn pct_tenths(actual: i64, budget: i64) -> Option<f64> {
    if budget <= 0 {
        return None;
    }
    let tenths = (i128::from(actual) * 2000 + i128::from(budget)) / (2 * i128::from(budget));
    Some(tenths as f64 / 10.0)
}

/// Every category for one month, income and expense. A category that was deleted is listed only
/// if it still has transactions that month. Sorted by type, then name.
pub fn budget_vs_actual(conn: &Connection, month: &str) -> Result<BudgetVsActual, AppError> {
    let first = month_start(month)?;
    let start = first.to_string();
    let end = next_month(first)?.to_string();
    let limits: std::collections::HashMap<i64, i64> = budgets::effective(conn, &start)?
        .into_iter()
        .filter(|b| b.scope == Scope::Category)
        .filter_map(|b| b.category_id.map(|id| (id, b.limit_minor)))
        .collect();

    let mut statement = conn.prepare(
        "SELECT c.id, c.name, c.type, COALESCE(SUM(t.amount_minor), 0) AS actual, c.deleted_at IS NULL
         FROM categories c
         LEFT JOIN transactions t ON t.category_id = c.id AND t.payment_date >= ?1 AND t.payment_date < ?2
         GROUP BY c.id
         HAVING c.deleted_at IS NULL OR COUNT(t.id) > 0
         ORDER BY c.type, c.name COLLATE NOCASE, c.id",
    )?;
    let rows = statement.query_map([&start, &end], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
    })?;
    let mut categories = Vec::new();
    for row in rows {
        let (id, name, kind, actual) = row?;
        let budget = limits.get(&id).copied();
        categories.push(CategoryLine {
            category_id: id,
            name,
            kind: if kind == "INCOME" { CategoryType::Income } else { CategoryType::Expense },
            budgeted: budget.map(format_amount),
            actual: format_amount(actual),
            remaining: budget.map(|b| format_amount(b - actual)),
            pct_used: budget.and_then(|b| pct_tenths(actual, b)),
        });
    }
    Ok(BudgetVsActual { month: month.to_string(), categories, totals: budgets::overview(conn, month)? })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    /// "YYYY-MM".
    pub month: String,
    /// Null for months where the category has no budget.
    pub budgeted: Option<String>,
    pub actual: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTrend {
    #[serde(serialize_with = "serialize_id")]
    pub category_id: i64,
    pub points: Vec<TrendPoint>,
}

/// One category, month by month from `from` to `to` (both "YYYY-MM", inclusive, at most 120 months).
pub fn category_trend(conn: &Connection, category_id: i64, from: &str, to: &str) -> Result<CategoryTrend, AppError> {
    let first = month_start(from)?;
    let last = month_start(to)?;
    if first > last {
        return Err(AppError::InvalidInput);
    }
    let kind: String = conn
        .query_row("SELECT type FROM categories WHERE id = ?1", [category_id], |r| r.get(0))
        .optional()?
        .ok_or(AppError::NotFound)?;
    let kind = if kind == "INCOME" { CategoryType::Income } else { CategoryType::Expense };

    let after_last = next_month(last)?.to_string();
    let mut actuals = std::collections::HashMap::new();
    let mut statement = conn.prepare(
        "SELECT substr(payment_date, 1, 7), SUM(amount_minor) FROM transactions
         WHERE category_id = ?1 AND payment_date >= ?2 AND payment_date < ?3 GROUP BY 1",
    )?;
    for row in statement.query_map(params![category_id, first.to_string(), after_last], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (month, sum) = row?;
        actuals.insert(month, sum);
    }

    let mut points = Vec::new();
    let mut month: NaiveDate = first;
    while month <= last {
        if points.len() >= MAX_TREND_MONTHS {
            return Err(AppError::InvalidInput);
        }
        let label = month.format("%Y-%m").to_string();
        let budget = budgets::series_limit(conn, kind_sql(kind), Some(category_id), &month.to_string())?;
        points.push(TrendPoint {
            budgeted: budget.map(format_amount),
            actual: format_amount(actuals.get(&label).copied().unwrap_or(0)),
            month: label,
        });
        month = next_month(month)?;
    }
    Ok(CategoryTrend { category_id, points })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budgets::SetBudget;
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

    fn budget(conn: &Connection, kind: CategoryType, scope: Scope, category: Option<i64>, month: &str, limit: &str) {
        let input = SetBudget {
            kind,
            scope,
            month: month.into(),
            amount_limit: Some(limit.into()),
            category_id: category.map(|c| c.to_string()),
        };
        budgets::set(conn, &input, "2026-01-15T12:00:00Z".parse().unwrap()).unwrap();
    }

    #[test]
    fn the_food_example_matches_hand_calculation() {
        let (conn, food, rent, _) = db();
        budget(&conn, CategoryType::Expense, Scope::Category, Some(food), "2026-10", "400.00");
        budget(&conn, CategoryType::Expense, Scope::Global, None, "2026-10", "2000.00");
        for (amount, date) in [("120.40", "2026-10-02"), ("85.30", "2026-10-11"), ("150.50", "2026-10-31")] {
            spend(&conn, food, amount, date);
        }
        spend(&conn, food, "99.00", "2026-09-30");
        spend(&conn, food, "99.00", "2026-11-01");
        spend(&conn, rent, "900.00", "2026-10-01");

        let result = budget_vs_actual(&conn, "2026-10").unwrap();
        let line = result.categories.iter().find(|c| c.category_id == food).unwrap();
        assert_eq!(line.budgeted.as_deref(), Some("400.00"));
        assert_eq!(line.actual, "356.20");
        assert_eq!(line.remaining.as_deref(), Some("43.80"));
        assert_eq!(line.pct_used, Some(89.1));

        let rent_line = result.categories.iter().find(|c| c.category_id == rent).unwrap();
        assert_eq!((rent_line.budgeted.as_deref(), rent_line.pct_used), (None, None));
        assert_eq!(rent_line.actual, "900.00");

        let expense = result.totals.iter().find(|o| o.kind == CategoryType::Expense).unwrap();
        assert_eq!(expense.actual, "1256.20");
        assert_eq!(expense.budgeted.as_deref(), Some("2000.00"));
        assert_eq!(expense.allocated, "400.00");
        assert_eq!(expense.remaining.as_deref(), Some("743.80"));
    }

    #[test]
    fn over_budget_is_negative_remaining_and_over_a_hundred_percent() {
        let (conn, food, _, _) = db();
        budget(&conn, CategoryType::Expense, Scope::Category, Some(food), "2026-10", "100.00");
        spend(&conn, food, "150.00", "2026-10-05");
        let line = &budget_vs_actual(&conn, "2026-10").unwrap().categories[0];
        assert_eq!((line.remaining.as_deref(), line.pct_used), (Some("-50.00"), Some(150.0)));
    }

    #[test]
    fn a_month_without_budgets_has_null_budgeted() {
        let (conn, food, _, _) = db();
        spend(&conn, food, "10.00", "2026-10-05");
        let result = budget_vs_actual(&conn, "2026-10").unwrap();
        assert!(result.categories.iter().all(|c| c.budgeted.is_none() && c.remaining.is_none() && c.pct_used.is_none()));
        assert!(result.totals.iter().all(|o| o.budgeted.is_none() && o.remaining.is_none()));
        assert_eq!(budget_vs_actual(&conn, "2026-13").unwrap_err(), AppError::InvalidInput);
    }

    #[test]
    fn a_deleted_category_is_listed_only_for_months_it_has_transactions() {
        let (mut conn, food, rent, _) = db();
        spend(&conn, rent, "50.00", "2026-10-05");
        categories::delete(&mut conn, rent, "2026-10-20T12:00:00Z".parse().unwrap()).unwrap();
        let october = budget_vs_actual(&conn, "2026-10").unwrap();
        assert!(october.categories.iter().any(|c| c.category_id == rent));
        let november = budget_vs_actual(&conn, "2026-11").unwrap();
        assert!(november.categories.iter().all(|c| c.category_id != rent));
        assert!(november.categories.iter().any(|c| c.category_id == food));
    }

    #[test]
    fn trend_follows_budget_changes_and_gaps() {
        let (conn, food, _, _) = db();
        budget(&conn, CategoryType::Expense, Scope::Category, Some(food), "2026-02", "400.00");
        budget(&conn, CategoryType::Expense, Scope::Category, Some(food), "2026-04", "450.00");
        spend(&conn, food, "380.10", "2026-02-10");
        spend(&conn, food, "455.00", "2026-03-10");
        spend(&conn, food, "20.00", "2026-03-11");
        let trend = category_trend(&conn, food, "2026-01", "2026-04").unwrap();
        let rows: Vec<(&str, Option<&str>, &str)> =
            trend.points.iter().map(|p| (p.month.as_str(), p.budgeted.as_deref(), p.actual.as_str())).collect();
        assert_eq!(
            rows,
            vec![
                ("2026-01", None, "0.00"),
                ("2026-02", Some("400.00"), "380.10"),
                ("2026-03", Some("400.00"), "475.00"),
                ("2026-04", Some("450.00"), "0.00"),
            ]
        );
    }

    #[test]
    fn trend_rejects_bad_ranges_and_unknown_categories() {
        let (conn, food, _, _) = db();
        assert_eq!(category_trend(&conn, food, "2026-05", "2026-04").unwrap_err(), AppError::InvalidInput);
        assert_eq!(category_trend(&conn, food, "2000-01", "2026-04").unwrap_err(), AppError::InvalidInput);
        assert_eq!(category_trend(&conn, 999, "2026-01", "2026-02").unwrap_err(), AppError::NotFound);
        assert_eq!(category_trend(&conn, food, "2026-01", "2026-01").unwrap().points.len(), 1);
    }

    #[test]
    fn percent_rounds_half_up_to_one_decimal() {
        assert_eq!(pct_tenths(35620, 40000), Some(89.1));
        assert_eq!(pct_tenths(1, 3), Some(33.3));
        assert_eq!(pct_tenths(2, 3), Some(66.7));
        assert_eq!(pct_tenths(5, 0), None);
    }
}
