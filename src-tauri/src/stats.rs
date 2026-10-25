//! Numbers for the charts. Budget lookup is the same code the budget screens use
//! (`budgets::effective`, `budgets::series_limit`), so figures never disagree between screens.
//! Money leaves as two-decimal strings.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::budgets::{self, kind_sql, next_month, month_start, Overview, Scope};
use crate::categories::{serialize_id, CategoryType};
use crate::debts::splits;
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
pub(crate) fn pct_tenths(actual: i64, budget: i64) -> Option<f64> {
    if budget <= 0 {
        return None;
    }
    let tenths = (i128::from(actual) * 2000 + i128::from(budget)) / (2 * i128::from(budget));
    Some(tenths as f64 / 10.0)
}

/// Every category for one month, income and expense. A category that was deleted is listed only
/// if it still counts in that month (deleted from a later month) or has transactions in it. Sorted by type, then name.
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
        "SELECT c.id, c.name, c.type, COALESCE(SUM(t.cost_minor), 0) AS actual, c.deleted_at IS NULL
         FROM categories c
         LEFT JOIN transaction_costs t ON t.category_id = c.id AND t.payment_date >= ?1 AND t.payment_date < ?2
         GROUP BY c.id
         HAVING c.deleted_at IS NULL OR c.deleted_at >= ?2 OR COUNT(t.transaction_id) > 0
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
        "SELECT substr(payment_date, 1, 7), SUM(cost_minor) FROM transaction_costs
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

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YearPoint {
    /// "YYYY-MM".
    pub month: String,
    pub income: String,
    pub expense: String,
    /// Income minus expense; negative when more was spent than earned.
    pub savings: String,
    /// Savings as a share of income, one decimal (12.5); null in months without income.
    pub savings_rate: Option<f64>,
}

/// Income, expense and savings for each of the 12 months of `year` ("YYYY"), for the year trend.
pub fn year_trend(conn: &Connection, year: &str) -> Result<Vec<YearPoint>, AppError> {
    month_start(&format!("{year}-01"))?;
    let mut sums: std::collections::HashMap<(String, String), i64> = std::collections::HashMap::new();
    let mut statement = conn.prepare(
        "SELECT substr(t.payment_date, 1, 7), c.type, SUM(t.cost_minor)
         FROM transaction_costs t JOIN categories c ON c.id = t.category_id
         WHERE t.payment_date >= ?1 AND t.payment_date < ?2 GROUP BY 1, 2",
    )?;
    let from = format!("{year}-01-01");
    let to = format!("{}-01-01", year.parse::<i32>().map_err(|_| AppError::InvalidInput)? + 1);
    for row in statement.query_map([&from, &to], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))? {
        let (month, kind, sum) = row?;
        sums.insert((month, kind), sum);
    }
    Ok((1..=12)
        .map(|m| {
            let month = format!("{year}-{m:02}");
            let get = |kind: &str| sums.get(&(month.clone(), kind.to_string())).copied().unwrap_or(0);
            let (income, expense) = (get("INCOME"), get("EXPENSE"));
            YearPoint {
                income: format_amount(income),
                expense: format_amount(expense),
                savings: format_amount(income - expense),
                savings_rate: pct_tenths_signed(income - expense, income),
                month,
            }
        })
        .collect())
}

/// Like `pct_tenths`, but the part may be negative (spending above income).
pub(crate) fn pct_tenths_signed(part: i64, whole: i64) -> Option<f64> {
    let sign = if part < 0 { -1.0 } else { 1.0 };
    pct_tenths(part.abs(), whole).map(|p| p * sign)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopCategory {
    #[serde(serialize_with = "serialize_id")]
    pub category_id: i64,
    pub name: String,
    pub actual: String,
    /// Share of all spending (or income) of that month, one decimal.
    pub share: Option<f64>,
}

/// The `limit` categories with the most spending (or income) in the month, biggest first.
pub fn top_categories(conn: &Connection, month: &str, kind: CategoryType, limit: u32) -> Result<Vec<TopCategory>, AppError> {
    if limit == 0 || limit > 50 {
        return Err(AppError::InvalidInput);
    }
    let first = month_start(month)?;
    let (start, end) = (first.to_string(), next_month(first)?.to_string());
    let mut statement = conn.prepare(
        "SELECT c.id, c.name, SUM(t.cost_minor) AS total
         FROM transaction_costs t JOIN categories c ON c.id = t.category_id
         WHERE c.type = ?1 AND t.payment_date >= ?2 AND t.payment_date < ?3
         GROUP BY c.id ORDER BY total DESC, c.name COLLATE NOCASE, c.id",
    )?;
    let rows = statement
        .query_map(params![kind_sql(kind), start, end], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let whole: i64 = rows.iter().map(|r| r.2).sum();
    Ok(rows
        .into_iter()
        .take(limit as usize)
        .map(|(category_id, name, total)| TopCategory { category_id, name, actual: format_amount(total), share: pct_tenths(total, whole) })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commitment {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: CategoryType,
    /// What the item costs (or brings in) in an average month. Null for a bill with no fixed amount,
    /// which is left out of the totals.
    pub per_month: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Commitments {
    pub expense: String,
    pub income: String,
    pub items: Vec<Commitment>,
}

/// An item's amount spread over an average month, in minor units, rounded half up.
/// A day is 365/12 of a month and a week 52/12, so "weekly" is not simply four a month.
fn per_month_minor(amount: i64, unit: &str, count: i64) -> i64 {
    let (numerator, denominator): (i128, i64) = match unit {
        "DAY" => (365, 12 * count),
        "WEEK" => (52, 12 * count),
        "MONTH" => (1, count),
        _ => (1, 12 * count),
    };
    ((i128::from(amount) * numerator * 2 + i128::from(denominator)) / (2 * i128::from(denominator))) as i64
}

/// What the active recurring items add up to in an average month, expense and income apart.
pub fn commitments(conn: &Connection) -> Result<Commitments, AppError> {
    let mut statement = conn.prepare(
        "SELECT r.id, r.name, c.type, r.amount_minor, r.interval_unit, r.interval_count
         FROM recurring_transactions r JOIN categories c ON c.id = r.category_id
         WHERE r.is_active = 1 ORDER BY c.type, r.name COLLATE NOCASE, r.id",
    )?;
    let rows = statement.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, String>(4)?, r.get::<_, i64>(5)?))
    })?;
    // An item that is split costs you only your part of each payment.
    let templates = splits::all_template_weights(conn)?;
    let (mut expense, mut income) = (0, 0);
    let mut items = Vec::new();
    for row in rows {
        let (id, name, kind, amount, unit, count) = row?;
        let mine = match (amount, templates.get(&id)) {
            (Some(amount), Some(weights)) => Some(splits::my_part(amount, weights)?),
            (amount, _) => amount,
        };
        let monthly = mine.map(|amount| per_month_minor(amount, &unit, count));
        let kind = if kind == "INCOME" { CategoryType::Income } else { CategoryType::Expense };
        match kind {
            CategoryType::Income => income += monthly.unwrap_or(0),
            CategoryType::Expense => expense += monthly.unwrap_or(0),
        }
        items.push(Commitment { id, name, kind, per_month: monthly.map(format_amount) });
    }
    Ok(Commitments { expense: format_amount(expense), income: format_amount(income), items })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedVsPaid {
    #[serde(serialize_with = "serialize_id")]
    pub recurring_id: i64,
    pub name: String,
    /// How many occurrences were paid in the month.
    pub paid_count: u32,
    /// Null when none of the payments had a plan: a bill with no fixed amount.
    pub planned: Option<String>,
    pub paid: String,
    /// Paid minus planned; positive means it cost more than planned. Null when there was no plan.
    pub difference: Option<String>,
}

/// For each recurring item paid in the month: what was planned and what was actually paid.
pub fn planned_vs_paid(conn: &Connection, month: &str) -> Result<Vec<PlannedVsPaid>, AppError> {
    let first = month_start(month)?;
    let (start, end) = (first.to_string(), next_month(first)?.to_string());
    let mut statement = conn.prepare(
        "SELECT r.id, r.name, COUNT(*), COUNT(t.planned_amount_minor), SUM(COALESCE(t.planned_amount_minor, t.amount_minor)), SUM(t.amount_minor)
         FROM transactions t JOIN recurring_transactions r ON r.id = t.recurring_transaction_id
         WHERE t.payment_date >= ?1 AND t.payment_date < ?2
         GROUP BY r.id ORDER BY r.name COLLATE NOCASE, r.id",
    )?;
    let rows = statement.query_map([&start, &end], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, u32>(2)?, r.get::<_, u32>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?))
    })?;
    rows.map(|row| {
        let (recurring_id, name, paid_count, planned_count, planned, paid) = row?;
        // Payments without a plan are not compared with one.
        let has_plan = planned_count > 0;
        Ok(PlannedVsPaid {
            recurring_id,
            name,
            paid_count,
            planned: has_plan.then(|| format_amount(planned)),
            paid: format_amount(paid),
            difference: has_plan.then(|| format_amount(paid - planned)),
        })
    })
    .collect()
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

    #[test]
    fn a_bill_with_no_fixed_amount_is_listed_but_not_counted() {
        use crate::recurring::{self, NewRecurring, Unit};
        let (conn, _, rent, _) = db();
        let make = |name: &str, amount: Option<&str>| {
            recurring::create(
                &conn,
                &NewRecurring {
                    name: name.into(),
                    amount: amount.map(str::to_string),
                    category_id: rent.to_string(),
                    start_date: "2026-01-01".into(),
                    interval_unit: Unit::Month,
                    interval_count: 1,
                    end_date: None,
                    description: None,
                },
            )
            .unwrap()
        };
        make("Rent", Some("900.00"));
        let electricity = make("Electricity", None);
        let c = commitments(&conn).unwrap();
        assert_eq!(c.expense, "900.00", "only what is known is added up");
        let listed = c.items.iter().find(|i| i.id == electricity.id).unwrap();
        assert_eq!(listed.per_month, None);
        assert_eq!(c.items.iter().find(|i| i.name == "Rent").unwrap().per_month.as_deref(), Some("900.00"));

        // Paid with whatever the bill was: there was no plan to compare it with.
        recurring::pay(&conn, electricity.id, Some("78.40"), Some("2026-09-02"), "2026-09-02").unwrap();
        recurring::pay(&conn, electricity.id, Some("81.10"), Some("2026-09-28"), "2026-09-28").unwrap();
        let row = planned_vs_paid(&conn, "2026-09").unwrap().into_iter().find(|r| r.name == "Electricity").unwrap();
        assert_eq!((row.planned, row.difference), (None, None));
        assert_eq!((row.paid.as_str(), row.paid_count), ("159.50", 2));
    }

}
