//! The PDF report for a month or a range of months: totals, savings rate, income and expenses by
//! month, where the money went, budget against actual per category, and optionally the list of
//! transactions. It is made on this computer and nothing is sent anywhere.
//!
//! `load` gathers the numbers (whole months, in minor units), `layout` places them on pages as a list
//! of drawing operations, and `render` turns that list into a PDF. The text in headings comes from the
//! UI in the chosen language (`ReportLabels`), the way the due notification does, and numbers and
//! dates are written with the person's `DisplayFormat`. Text is set in DejaVu Sans, which is embedded:
//! it covers Latin, Greek and Cyrillic. A character it lacks, such as Chinese, is shown as `?`.
//!
//! Budget against actual for a range compares each category over the months in which it had a budget;
//! for a single month that is exactly what the budget screen shows.

// What a report reads is only used by the PDF, which the phone's build leaves out.
#![cfg_attr(mobile, allow(dead_code))]

// The PDF is made on the desktop only: the phone's build has neither the PDF writer nor the fonts (see `Cargo.toml`).
#[cfg(desktop)]
mod debts;
#[cfg(desktop)]
mod layout;
#[cfg(desktop)]
mod render;

use std::collections::HashMap;

use chrono::Duration;
use rusqlite::Connection;
use serde::Deserialize;

use crate::budgets::{month_start, next_month};
use crate::categories::CategoryType;
use crate::display::DisplayFormat;
use crate::export::{self, Selection, Tx};
use crate::money::parse_amount;
use crate::stats;
use crate::vault::AppError;

const MAX_MONTHS: usize = 120;
/// About 400 pages. More than this is not a report anyone reads.
const MAX_TRANSACTIONS: usize = 20_000;
const LABEL_MAX_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum PageSize {
    #[default]
    #[serde(rename = "A4")]
    A4,
    #[serde(rename = "LETTER")]
    Letter,
}

impl PageSize {
    /// Width and height in points.
    fn points(self) -> (f32, f32) {
        match self {
            PageSize::A4 => (595.28, 841.89),
            PageSize::Letter => (612.0, 792.0),
        }
    }
}

/// Every piece of fixed text on the page, in the person's language.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportLabels {
    pub title: String,
    /// The months the report covers, written out ("October 2026").
    pub period: String,
    pub income: String,
    pub expenses: String,
    pub savings: String,
    pub savings_rate: String,
    pub by_month: String,
    pub spending: String,
    pub other: String,
    pub budget_vs_actual: String,
    pub no_budget: String,
    pub transactions: String,
    pub date: String,
    pub category: String,
    pub name: String,
    pub amount: String,
    pub page: String,
    pub no_data: String,
}

const MONTH_NAMES: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// "October 2026" from "2026-10".
fn english_month(month: &str) -> Result<String, AppError> {
    let (year, number) = month.split_once('-').ok_or(AppError::InvalidInput)?;
    let number: usize = number.parse().map_err(|_| AppError::InvalidInput)?;
    if year.len() != 4 || !(1..=12).contains(&number) {
        return Err(AppError::InvalidInput);
    }
    Ok(format!("{} {year}", MONTH_NAMES[number - 1]))
}

impl ReportLabels {
    /// The fixed text in English, for a report asked for without the app's own window (from the command
    /// line). A test keeps these equal to the English texts the app's screens use.
    pub fn english(from_month: &str, to_month: &str) -> Result<Self, AppError> {
        let (from, to) = (english_month(from_month)?, english_month(to_month)?);
        let s = |t: &str| t.to_string();
        Ok(Self {
            title: s("meowNY report"),
            period: if from_month == to_month { from } else { format!("{from} – {to}") },
            income: s("Income"),
            expenses: s("Expenses"),
            savings: s("Savings"),
            savings_rate: s("Savings rate"),
            by_month: s("Income and expenses by month"),
            spending: s("Where the money went"),
            other: s("Other"),
            budget_vs_actual: s("Budget vs actual"),
            no_budget: s("no budget"),
            transactions: s("Transactions"),
            date: s("Date"),
            category: s("Category"),
            name: s("Name"),
            amount: s("Amount"),
            page: s("Page"),
            no_data: s("No transactions in this period."),
        })
    }

    fn all(&self) -> [&String; 18] {
        [
            &self.title, &self.period, &self.income, &self.expenses, &self.savings, &self.savings_rate, &self.by_month,
            &self.spending, &self.other, &self.budget_vs_actual, &self.no_budget, &self.transactions, &self.date,
            &self.category, &self.name, &self.amount, &self.page, &self.no_data,
        ]
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportOptions {
    /// "YYYY-MM". The report covers whole months, `from_month` to `to_month` inclusive.
    pub from_month: String,
    pub to_month: String,
    /// Null means every category.
    #[serde(default)]
    pub category_ids: Option<Vec<String>>,
    #[serde(default)]
    pub include_transactions: bool,
    /// Whether the transaction list shows each transaction's description. Off, the report can be
    /// shared without private details.
    #[serde(default)]
    pub show_descriptions: bool,
    /// Who owed what on the last day of the months: one line for each friend.
    #[serde(default)]
    pub include_debts: bool,
    /// Every line of the debts of every friend in the months, with the balance after it.
    #[serde(default)]
    pub include_debt_history: bool,
    /// The words of the two parts above, in the person's language. Needed when either is asked for.
    #[serde(default)]
    pub debts_labels: Option<crate::debts::summary::Labels>,
    #[serde(default)]
    pub page: PageSize,
    #[serde(default)]
    pub format: DisplayFormat,
    pub labels: ReportLabels,
}

pub(crate) struct MonthFigures {
    /// "YYYY-MM".
    pub month: String,
    pub income: i64,
    pub expense: i64,
}

pub(crate) struct BudgetLine {
    pub name: String,
    pub kind: CategoryType,
    /// Sum of the monthly limits over the months with a budget; None if it never had one.
    pub budgeted: Option<i64>,
    /// Over the months with a budget, or the whole period for a category without one.
    pub actual: i64,
}

pub(crate) struct GlobalBudget {
    pub kind: CategoryType,
    pub budgeted: i64,
    pub actual: i64,
}

pub(crate) struct ReportData {
    pub currency: String,
    pub months: Vec<MonthFigures>,
    pub income: i64,
    pub expense: i64,
    /// Share of income that was saved, one decimal; None without income.
    pub savings_rate: Option<f64>,
    /// Expenses by category, biggest first.
    pub spending: Vec<(String, i64)>,
    pub budget_lines: Vec<BudgetLine>,
    pub global_budgets: Vec<GlobalBudget>,
    pub transactions: Vec<Tx>,
    /// Debts and their history, when the report was asked for them.
    pub debts: Option<export::debts::DebtsPart>,
}

fn months_between(from: &str, to: &str) -> Result<Vec<String>, AppError> {
    let (mut month, last) = (month_start(from)?, month_start(to)?);
    let mut months = Vec::new();
    while month <= last {
        months.push(month.format("%Y-%m").to_string());
        if months.len() > MAX_MONTHS {
            return Err(AppError::InvalidInput);
        }
        month = next_month(month)?;
    }
    if months.is_empty() {
        return Err(AppError::InvalidInput);
    }
    Ok(months)
}

fn load(conn: &Connection, options: &ReportOptions, now: chrono::DateTime<chrono::Utc>) -> Result<ReportData, AppError> {
    let months = months_between(&options.from_month, &options.to_month)?;
    let first_day = month_start(&options.from_month)?;
    let last_day = next_month(month_start(&options.to_month)?)? - Duration::days(1);
    let sel = export::parse(&Selection {
        from: Some(first_day.to_string()),
        to: Some(last_day.to_string()),
        category_ids: options.category_ids.clone(),
        ..Selection::default()
    })?;

    let sums = export::monthly_sums(conn, &sel)?;
    let figures: Vec<MonthFigures> = months
        .iter()
        .map(|m| {
            let (income, expense) = sums.get(m).copied().unwrap_or_default();
            MonthFigures { month: m.clone(), income, expense }
        })
        .collect();
    let income: i64 = figures.iter().map(|m| m.income).sum();
    let expense: i64 = figures.iter().map(|m| m.expense).sum();

    let mut statement = conn.prepare(&format!(
        "SELECT c.name, SUM(k.cost_minor) FROM transactions t JOIN transaction_costs k ON k.transaction_id = t.id JOIN categories c ON c.id = t.category_id
         WHERE {} AND c.type = 'EXPENSE' GROUP BY c.id ORDER BY 2 DESC, c.name",
        export::transaction_filter(&sel)
    ))?;
    let spending = statement
        .query_map(rusqlite::params_from_iter(sel.args()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    // Budget against actual, month by month with the same code the budget screens use.
    struct Sum {
        name: String,
        kind: CategoryType,
        budgeted: Option<i64>,
        actual_budgeted: i64,
        actual_unbudgeted: i64,
    }
    let mut lines: HashMap<i64, Sum> = HashMap::new();
    // (budgeted, actual) of the global budget per side, summed over the months that had one: expense, income.
    let mut globals: [Option<(i64, i64)>; 2] = [None, None];
    for month in &months {
        let view = stats::budget_vs_actual(conn, month)?;
        for line in view.categories {
            if sel.categories.as_ref().is_some_and(|ids| !ids.contains(&line.category_id)) {
                continue;
            }
            let actual = parse_amount(&line.actual)?;
            let entry = lines.entry(line.category_id).or_insert_with(|| Sum {
                name: line.name.clone(),
                kind: line.kind,
                budgeted: None,
                actual_budgeted: 0,
                actual_unbudgeted: 0,
            });
            // The latest name wins, in case the category was renamed during the period.
            entry.name = line.name;
            match line.budgeted {
                Some(limit) => {
                    entry.budgeted = Some(entry.budgeted.unwrap_or(0) + parse_amount(&limit)?);
                    entry.actual_budgeted += actual;
                }
                None => entry.actual_unbudgeted += actual,
            }
        }
        // A global budget is for everything, so it only belongs in a report of every category.
        if sel.categories.is_none() {
            for side in view.totals {
                if let Some(limit) = side.budgeted {
                    let slot = globals[usize::from(side.kind == CategoryType::Income)].get_or_insert((0, 0));
                    slot.0 += parse_amount(&limit)?;
                    slot.1 += parse_amount(&side.actual)?;
                }
            }
        }
    }
    let mut budget_lines: Vec<BudgetLine> = lines
        .into_values()
        .map(|s| BudgetLine {
            name: s.name,
            kind: s.kind,
            actual: if s.budgeted.is_some() { s.actual_budgeted } else { s.actual_unbudgeted },
            budgeted: s.budgeted,
        })
        .filter(|l| l.budgeted.is_some() || l.actual > 0)
        .collect();
    budget_lines.sort_by(|a, b| {
        (a.kind == CategoryType::Income, a.name.to_lowercase()).cmp(&(b.kind == CategoryType::Income, b.name.to_lowercase()))
    });
    let global_budgets: Vec<GlobalBudget> = [CategoryType::Expense, CategoryType::Income]
        .into_iter()
        .zip(globals)
        .filter_map(|(kind, sums)| sums.map(|(budgeted, actual)| GlobalBudget { kind, budgeted, actual }))
        .collect();

    let transactions = if options.include_transactions { export::transactions(conn, &sel)? } else { Vec::new() };
    if transactions.len() > MAX_TRANSACTIONS {
        return Err(AppError::InvalidInput);
    }

    let debts = if options.include_debts || options.include_debt_history {
        Some(export::debts::report_part(conn, &sel, &first_day.to_string(), &last_day.to_string(), options.include_debts, options.include_debt_history, &export::today_of(conn, now)?)?)
    } else {
        None
    };

    Ok(ReportData {
        currency: export::currency(conn)?,
        months: figures,
        income,
        expense,
        savings_rate: stats::pct_tenths_signed(income - expense, income),
        spending,
        budget_lines,
        global_budgets,
        transactions,
        debts,
    })
}

/// The numbers of a report, read from the database. Laying out and writing the PDF (`Report::pdf`)
/// needs no database, so a caller can release the connection first.
pub struct Report(ReportData);

/// Reads the numbers for a report in one read transaction, so they agree with each other.
pub fn read(conn: &Connection, options: &ReportOptions) -> Result<Report, AppError> {
    read_at(conn, options, chrono::Utc::now())
}

/// The same, as of `now`: the recurring payments that have come due are the ones due by that day.
pub(crate) fn read_at(conn: &Connection, options: &ReportOptions, now: chrono::DateTime<chrono::Utc>) -> Result<Report, AppError> {
    options.format.validate()?;
    if options.labels.all().iter().any(|l| l.chars().count() > LABEL_MAX_CHARS || l.chars().any(char::is_control)) {
        return Err(AppError::InvalidInput);
    }
    // the words of the debts parts are needed for them and checked like the others
    if let Some(labels) = &options.debts_labels {
        labels.validate()?;
    }
    if (options.include_debts || options.include_debt_history) && options.debts_labels.is_none() {
        return Err(AppError::InvalidInput);
    }
    let tx = conn.unchecked_transaction()?;
    Ok(Report(load(&tx, options, now)?))
}

/// The summary of who owes what as a PDF, with the words in the person's language. The phone's build has no PDF writer.
#[cfg(desktop)]
pub fn debts_summary_pdf(conn: &Connection, today: &str, labels: &crate::debts::summary::Labels, format: &DisplayFormat, page: PageSize) -> Result<Vec<u8>, AppError> {
    labels.validate()?;
    format.validate()?;
    let tx = conn.unchecked_transaction()?;
    let currency = export::currency(&tx)?;
    let summary = crate::debts::summary::summary(&tx)?;
    let doc = debts::summary_doc(&summary, &currency, today, labels, format, page)?;
    debts::pdf(&doc, &labels.summary_title, true)
}

/// The statement of one friend as a PDF.
#[cfg(desktop)]
pub fn debts_statement_pdf(
    conn: &Connection,
    person_id: i64,
    today: &str,
    labels: &crate::debts::summary::Labels,
    format: &DisplayFormat,
    page: PageSize,
) -> Result<Vec<u8>, AppError> {
    labels.validate()?;
    format.validate()?;
    let tx = conn.unchecked_transaction()?;
    let currency = export::currency(&tx)?;
    let statement = crate::debts::ledger::statement_in(&tx, person_id)?;
    let doc = debts::statement_doc(&statement, &currency, today, labels, format, page)?;
    debts::pdf(&doc, &labels.statement_title, true)
}

#[cfg(mobile)]
pub fn debts_summary_pdf(_: &Connection, _: &str, _: &crate::debts::summary::Labels, _: &DisplayFormat, _: PageSize) -> Result<Vec<u8>, AppError> {
    Err(AppError::InvalidInput)
}

#[cfg(mobile)]
pub fn debts_statement_pdf(_: &Connection, _: i64, _: &str, _: &crate::debts::summary::Labels, _: &DisplayFormat, _: PageSize) -> Result<Vec<u8>, AppError> {
    Err(AppError::InvalidInput)
}

#[cfg(test)]
impl Report {
    /// What the report adds up, for the tests: income, expenses, the sum of "where the money went", and the sum of the listed transactions.
    pub(crate) fn figures(&self) -> (i64, i64, i64, i64) {
        let data = &self.0;
        (data.income, data.expense, data.spending.iter().map(|s| s.1).sum(), data.transactions.iter().map(|t| t.my_share).sum())
    }
}

impl Report {
    /// The report as the bytes of a PDF file.
    pub fn pdf(&self, options: &ReportOptions) -> Result<Vec<u8>, AppError> {
        self.pdf_with(options, true)
    }

    #[cfg(desktop)]
    fn pdf_with(&self, options: &ReportOptions, compress: bool) -> Result<Vec<u8>, AppError> {
        let doc = layout::build(&self.0, options)?;
        render::pdf(&doc, &options.labels.title, compress)
    }

    #[cfg(mobile)]
    fn pdf_with(&self, _: &ReportOptions, _: bool) -> Result<Vec<u8>, AppError> {
        Err(AppError::InvalidInput)
    }
}

#[cfg(all(test, desktop))]
mod tests {
    use super::layout::{axis_top, donut_slices, label_stride, Metrics, Op};
    use super::*;
    use crate::budgets::{self, Scope, SetBudget};
    use crate::categories::{self, CategoryType};
    use crate::display::{DateOrder, DateSeparator};
    use crate::transactions::{self, NewTransaction};

    fn labels() -> ReportLabels {
        let s = |t: &str| t.to_string();
        ReportLabels {
            title: s("meowNY report"),
            period: s("January 2026"),
            income: s("Income"),
            expenses: s("Expenses"),
            savings: s("Savings"),
            savings_rate: s("Savings rate"),
            by_month: s("Income and expenses by month"),
            spending: s("Where the money went"),
            other: s("Other"),
            budget_vs_actual: s("Budget vs actual"),
            no_budget: s("no budget"),
            transactions: s("Transactions"),
            date: s("Date"),
            category: s("Category"),
            name: s("Name"),
            amount: s("Amount"),
            page: s("Page"),
            no_data: s("No transactions in this period."),
        }
    }

    fn options(from: &str, to: &str) -> ReportOptions {
        ReportOptions {
            from_month: from.into(),
            to_month: to.into(),
            category_ids: None,
            include_transactions: false,
            show_descriptions: false,
            include_debts: false,
            include_debt_history: false,
            debts_labels: None,
            page: PageSize::A4,
            format: DisplayFormat::default(),
            labels: labels(),
        }
    }

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, 'EUR', 'en-IE', 'UTC', 'en')",
            [],
        )
        .unwrap();
        conn
    }

    fn spend(conn: &Connection, category: i64, name: &str, amount: &str, date: &str, description: Option<&str>) {
        transactions::create(
            conn,
            &NewTransaction {
                name: name.into(),
                amount: amount.into(),
                payment_date: date.into(),
                category_id: category.to_string(),
                description: description.map(str::to_string),
                client_uuid: None,
            },
        )
        .unwrap();
    }

    fn budget(conn: &Connection, kind: CategoryType, scope: Scope, category: Option<i64>, month: &str, limit: &str) {
        budgets::set(
            conn,
            &SetBudget {
                kind,
                scope,
                month: month.into(),
                amount_limit: Some(limit.into()),
                category_id: category.map(|c| c.to_string()),
            },
            chrono::Utc::now(),
        )
        .unwrap();
    }

    struct World {
        conn: Connection,
        salary: i64,
        rent: i64,
        food: i64,
        therapy: i64,
    }

    /// January to March 2026. January is the Food example: a budget of 400 and 356.20 spent.
    fn world() -> World {
        let conn = db();
        let salary = categories::create(&conn, "Salary", CategoryType::Income).unwrap().id;
        let rent = categories::create(&conn, "Rent", CategoryType::Expense).unwrap().id;
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        let therapy = categories::create(&conn, "Therapy", CategoryType::Expense).unwrap().id;
        budget(&conn, CategoryType::Expense, Scope::Global, None, "2026-01", "2200.00");
        budget(&conn, CategoryType::Income, Scope::Global, None, "2026-01", "3500.00");
        budget(&conn, CategoryType::Expense, Scope::Category, Some(food), "2026-01", "400.00");
        budget(&conn, CategoryType::Expense, Scope::Category, Some(rent), "2026-01", "900.00");
        for month in ["2026-01", "2026-02", "2026-03"] {
            spend(&conn, salary, "Pay", "3200.00", &format!("{month}-25"), Some("PRIVATE-PAY-NOTE"));
            spend(&conn, rent, "Rent", "900.00", &format!("{month}-01"), None);
        }
        spend(&conn, food, "Market", "200.00", "2026-01-05", Some("PRIVATE-FOOD-NOTE"));
        spend(&conn, food, "Bakery", "56.20", "2026-01-12", None);
        spend(&conn, food, "Dinner", "100.00", "2026-01-20", None);
        spend(&conn, food, "Market", "450.00", "2026-02-07", None);
        spend(&conn, therapy, "Session", "80.00", "2026-02-10", Some("PRIVATE-THERAPY-NOTE"));
        World { conn, salary, rent, food, therapy }
    }

    fn money(text: &str) -> i64 {
        parse_amount(text).unwrap()
    }

    #[test]
    fn the_figures_match_the_dashboard_for_the_same_month() {
        let w = world();
        let report = read(&w.conn, &options("2026-01", "2026-01")).unwrap().0;
        let view = stats::budget_vs_actual(&w.conn, "2026-01").unwrap();
        let trend = &stats::year_trend(&w.conn, "2026").unwrap()[0];

        assert_eq!((report.income, report.expense), (money(&trend.income), money(&trend.expense)));
        assert_eq!(report.income - report.expense, money(&trend.savings));
        assert_eq!(report.savings_rate, trend.savings_rate);
        assert_eq!(report.expense, 4 * 90_000 / 4 + 35_620, "900.00 rent and the 356.20 of food");
        for side in &view.totals {
            assert_eq!(money(&side.actual), if side.kind == CategoryType::Income { report.income } else { report.expense });
        }
        // The Food example: a budget of 400, 356.20 spent, 89.1 percent used.
        let food = report.budget_lines.iter().find(|l| l.name == "Food").unwrap();
        assert_eq!((food.budgeted, food.actual), (Some(40_000), 35_620));
        assert_eq!(stats::pct_tenths(food.actual, 40_000), Some(89.1));
        for line in &report.budget_lines {
            let dash = view.categories.iter().find(|c| c.name == line.name).unwrap();
            assert_eq!(line.actual, money(&dash.actual), "{}", line.name);
            assert_eq!(line.budgeted, dash.budgeted.as_deref().map(money), "{}", line.name);
        }
        for global in &report.global_budgets {
            let side = view.totals.iter().find(|s| s.kind == global.kind).unwrap();
            assert_eq!(Some(global.budgeted), side.budgeted.as_deref().map(money));
            assert_eq!(global.actual, money(&side.actual));
        }
    }

    #[test]
    fn a_range_adds_up_the_months() {
        let w = world();
        let report = read(&w.conn, &options("2026-01", "2026-03")).unwrap().0;
        let trend = stats::year_trend(&w.conn, "2026").unwrap();
        assert_eq!(report.months.len(), 3);
        for (figures, point) in report.months.iter().zip(&trend) {
            assert_eq!((figures.income, figures.expense), (money(&point.income), money(&point.expense)), "{}", point.month);
        }
        assert_eq!(report.income, 3 * 320_000);
        assert_eq!(report.expense, 3 * 90_000 + 35_620 + 45_000 + 8_000);
        let spent: i64 = report.spending.iter().map(|(_, m)| m).sum();
        assert_eq!(spent, report.expense, "the donut covers every expense");
        assert_eq!(report.spending[0].0, "Rent", "biggest first");
    }

    #[test]
    fn a_range_compares_only_the_months_that_had_a_budget() {
        let w = world();
        // A budget set for a month that has already started changes only that month, so each is set here.
        for month in ["2026-02", "2026-03"] {
            budget(&w.conn, CategoryType::Expense, Scope::Category, Some(w.food), month, "400.00");
            budget(&w.conn, CategoryType::Expense, Scope::Global, None, month, "2200.00");
        }
        let report = read(&w.conn, &options("2026-01", "2026-03")).unwrap().0;
        let food = report.budget_lines.iter().find(|l| l.name == "Food").unwrap();
        assert_eq!((food.budgeted, food.actual), (Some(120_000), 35_620 + 45_000));
        // Therapy never had a budget: the whole period counts, and it is flagged as without one.
        let therapy = report.budget_lines.iter().find(|l| l.name == "Therapy").unwrap();
        assert_eq!((therapy.budgeted, therapy.actual), (None, 8_000));
        let g = report.global_budgets.iter().find(|g| g.kind == CategoryType::Expense).unwrap();
        assert_eq!(g.budgeted, 3 * 220_000);
    }

    #[test]
    fn a_category_selection_limits_everything_in_the_report() {
        let w = world();
        let mut o = options("2026-01", "2026-03");
        o.category_ids = Some(vec![w.food.to_string(), w.salary.to_string()]);
        o.include_transactions = true;
        o.show_descriptions = true;
        let report = read(&w.conn, &o).unwrap().0;
        assert_eq!(report.expense, 35_620 + 45_000);
        assert!(report.global_budgets.is_empty(), "a global budget is for everything");
        assert!(report.budget_lines.iter().all(|l| l.name == "Food" || l.name == "Salary"));
        assert!(report.spending.iter().all(|(n, _)| n == "Food"));
        let doc = layout::build(&report, &o).unwrap();
        for hidden in ["Rent", "Therapy", "PRIVATE-THERAPY-NOTE", "Session"] {
            assert!(!doc.texts().iter().any(|t| t.contains(hidden)), "{hidden} reached the page");
        }
        let _ = (w.rent, w.therapy);
    }

    #[test]
    fn a_bad_period_is_refused() {
        let w = world();
        for (from, to) in [("2026-03", "2026-01"), ("2026-13", "2026-13"), ("26-1", "26-2"), ("", ""), ("2026-01", "2036-02")] {
            assert_eq!(read(&w.conn, &options(from, to)).err(), Some(AppError::InvalidInput), "{from} {to}");
        }
        assert!(read(&w.conn, &options("2026-01", "2035-12")).is_ok(), "ten years is the longest");
        let mut o = options("2026-01", "2026-01");
        o.category_ids = Some(vec!["1; DROP TABLE x".into()]);
        assert_eq!(read(&w.conn, &o).err(), Some(AppError::InvalidInput));
    }

    #[test]
    fn labels_and_format_are_checked() {
        let w = world();
        let mut long = options("2026-01", "2026-01");
        long.labels.title = "x".repeat(LABEL_MAX_CHARS + 1);
        assert_eq!(read(&w.conn, &long).err(), Some(AppError::InvalidInput));
        let mut control = options("2026-01", "2026-01");
        control.labels.period = "January\n2026".into();
        assert_eq!(read(&w.conn, &control).err(), Some(AppError::InvalidInput));
        let mut format = options("2026-01", "2026-01");
        format.format.decimal_mark = "ab".into();
        assert_eq!(read(&w.conn, &format).err(), Some(AppError::InvalidInput));
        let unknown = serde_json::json!({ "fromMonth": "2026-01", "toMonth": "2026-01", "labels": {}, "extra": 1 });
        assert!(serde_json::from_value::<ReportOptions>(unknown).is_err());
    }

    fn texts(o: &ReportOptions, w: &World) -> Vec<String> {
        let report = read(&w.conn, o).unwrap().0;
        layout::build(&report, o).unwrap().texts().iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn the_page_shows_the_figures_in_the_chosen_format() {
        let w = world();
        let mut o = options("2026-01", "2026-01");
        o.format = DisplayFormat {
            date_order: DateOrder::DayMonthYear,
            date_separator: DateSeparator::Dot,
            currency_symbol: "€".into(),
            symbol_before: false,
            symbol_space: true,
            decimal_mark: ",".into(),
            group_separator: ".".into(),
        };
        let text = texts(&o, &w);
        let has = |needle: &str| text.iter().any(|t| t.contains(needle));
        assert!(has("meowNY report") && has("January 2026") && has("Where the money went") && has("Budget vs actual"));
        assert!(has("3.200,00\u{a0}€"), "income: {text:?}");
        assert!(has("1.256,20\u{a0}€"), "expenses: 900 rent and 356.20 food");
        assert!(has("1.943,80\u{a0}€"), "savings");
        assert!(has("60,7%"), "savings rate, with the decimal mark of the format");
        assert!(has("356,20\u{a0}€ / 400,00\u{a0}€") && has("89,1%"), "the Food example");
        assert!(has("01.2026"), "months on the axis follow the date format");
        assert!(has("Page 1 / 1"));
    }

    #[test]
    fn the_transaction_list_is_optional_and_descriptions_can_be_hidden() {
        let w = world();
        let mut o = options("2026-01", "2026-01");
        let without = texts(&o, &w);
        assert!(!without.iter().any(|t| t == "Transactions" || t.contains("Market")));

        o.include_transactions = true;
        let hidden = texts(&o, &w);
        assert!(hidden.iter().any(|t| t == "Transactions"));
        assert!(hidden.iter().any(|t| t == "Market") && hidden.iter().any(|t| t == "2026-01-05"));
        assert!(!hidden.iter().any(|t| t.contains("PRIVATE")), "no description reaches the page: {hidden:?}");

        o.show_descriptions = true;
        let shown = texts(&o, &w);
        assert!(shown.iter().any(|t| t == "PRIVATE-FOOD-NOTE") && shown.iter().any(|t| t == "PRIVATE-PAY-NOTE"));
        assert!(!shown.iter().any(|t| t.contains("PRIVATE-THERAPY")), "February is not in this report");
    }

    #[test]
    fn a_long_list_runs_onto_more_pages_and_repeats_its_heading() {
        let w = world();
        for i in 0..200 {
            spend(&w.conn, w.food, &format!("Shop {i}"), "1.00", "2026-01-15", Some("note"));
        }
        let mut o = options("2026-01", "2026-01");
        o.include_transactions = true;
        o.show_descriptions = true;
        let report = read(&w.conn, &o).unwrap().0;
        let doc = layout::build(&report, &o).unwrap();
        let pages = doc.pages.len();
        assert!(pages >= 5, "{pages} pages");
        let headings = doc.pages.iter().filter(|p| {
            p.iter().any(|op| matches!(op, Op::Text { text, bold: true, .. } if text == "Amount"))
        }).count();
        assert_eq!(headings, pages - 1, "every page of the list has the column headings; the first page has none");
        let last = format!("Page {pages} / {pages}");
        assert!(doc.texts().contains(&last.as_str()));
        // Nothing runs past the margins.
        for op in doc.pages.iter().flatten() {
            if let Op::Text { y, .. } = op {
                assert!(*y < doc.height - 30.0 || *y > doc.height - 40.0, "text over the footer area: {y}");
            }
        }
    }

    #[test]
    fn long_text_is_cut_to_its_column() {
        let w = world();
        spend(&w.conn, w.food, &"W".repeat(48), "1.00", "2026-01-16", Some(&"long note ".repeat(25)));
        let mut o = options("2026-01", "2026-01");
        o.include_transactions = true;
        o.show_descriptions = true;
        let doc = layout::build(&read(&w.conn, &o).unwrap().0, &o).unwrap();
        let metrics = Metrics::new().unwrap();
        for op in doc.pages.iter().flatten() {
            if let Op::Text { x, size, bold, text, .. } = op {
                assert!(x + metrics.width(text, *size, *bold) <= doc.width - 44.0 + 0.5, "{text:?} runs past the margin");
            }
        }
        assert!(doc.texts().iter().any(|t| t.starts_with("WWW") && t.ends_with('\u{2026}')), "the name is cut with an ellipsis");
        assert!(doc.texts().iter().any(|t| t.starts_with("long note") && t.ends_with('\u{2026}')));
    }

    #[test]
    fn characters_the_font_lacks_become_question_marks_and_the_rest_survive() {
        let w = world();
        let rare = categories::create(&w.conn, "Școală Țară Ωμέγα Дом", CategoryType::Expense).unwrap().id;
        let cjk = categories::create(&w.conn, "食費", CategoryType::Expense).unwrap().id;
        spend(&w.conn, rare, "Café – niño", "5.00", "2026-01-10", None);
        spend(&w.conn, cjk, "ラーメン", "9.00", "2026-01-11", None);
        let mut o = options("2026-01", "2026-01");
        o.include_transactions = true;
        let text = texts(&o, &w);
        assert!(text.iter().any(|t| t == "Școală Țară Ωμέγα Дом"), "Latin, Greek and Cyrillic are drawn");
        assert!(text.iter().any(|t| t == "Café – niño"));
        assert!(text.iter().any(|t| t == "??"), "Chinese is not in the font");
        assert!(text.iter().any(|t| t == "????"));
    }

    #[test]
    fn an_empty_period_still_makes_a_report() {
        let w = world();
        let o = options("2030-01", "2030-02");
        let text = texts(&o, &w);
        assert!(text.iter().any(|t| t == "No transactions in this period."));
        assert!(!text.iter().any(|t| t == "Where the money went"));
        let bytes = read(&w.conn, &o).unwrap().pdf(&o).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn axis_labels_use_round_numbers() {
        assert_eq!(axis_top(35_620), (40_000, 10_000), "the Food example: 0 to 400 in steps of 100");
        assert_eq!(axis_top(0), (10_000, 2_500), "an empty chart still has an axis");
        assert_eq!(axis_top(320_000), (400_000, 100_000));
        assert_eq!(axis_top(100_000), (100_000, 25_000), "1000.00 fits four steps of 250");
        assert_eq!(axis_top(1_234_567), (2_000_000, 500_000));
        assert_eq!(axis_top(999_999_999_999), (1_000_000_000_000, 250_000_000_000));
        for max in [1, 99, 100, 101, 12_345, 9_999_999, 123_456_789_012] {
            let (top, step) = axis_top(max);
            assert!(top >= max && top == step * 4, "{max}: {top} {step}");
        }
    }

    #[test]
    fn labels_along_an_axis_are_thinned_out_to_fit() {
        assert_eq!(label_stride(12, 40.0, 30.0), 1);
        assert_eq!(label_stride(36, 12.0, 30.0), 3);
        assert_eq!(label_stride(120, 3.0, 30.0), 12);
        assert_eq!(label_stride(1, 400.0, 30.0), 1);
    }

    #[test]
    fn the_donut_shows_seven_slices_and_the_rest_together() {
        let spending: Vec<(String, i64)> = (1..=10).map(|i| (format!("c{i}"), 1000 - i * 10)).collect();
        let slices = donut_slices(&spending, "Other");
        assert_eq!(slices.len(), 8);
        assert_eq!(slices[7], ("Other".to_string(), (8..=10).map(|i| 1000 - i * 10).sum()));
        let total: i64 = slices.iter().map(|(_, m)| m).sum();
        assert_eq!(total, spending.iter().map(|(_, m)| m).sum::<i64>());
        assert_eq!(donut_slices(&spending[..3], "Other").len(), 3);
        assert!(donut_slices(&[], "Other").is_empty());
        assert_eq!(donut_slices(&[("a".into(), 0), ("b".into(), 5)], "Other"), [("b".to_string(), 5)]);
    }

    #[test]
    fn the_text_of_the_pdf_is_what_the_layout_says() {
        let w = world();
        let mut o = options("2026-01", "2026-01");
        o.include_transactions = true;
        o.show_descriptions = true;
        let report = read(&w.conn, &o).unwrap();
        let in_file = pdf_text(&report.pdf_with(&o, false).unwrap()).concat();
        // The reader works: everything the layout drew can be read back from the file.
        for text in layout::build(&report.0, &o).unwrap().texts() {
            assert!(in_file.contains(text), "{text:?} is not in the PDF");
        }
        assert!(in_file.contains("meowNY report") && in_file.contains("3,200.00") && in_file.contains("PRIVATE-FOOD-NOTE"));
    }

    #[test]
    fn a_pdf_made_without_descriptions_contains_none() {
        let w = world();
        let mut o = options("2026-01", "2026-03");
        o.include_transactions = true;
        o.show_descriptions = false;
        let in_file = pdf_text(&read(&w.conn, &o).unwrap().pdf_with(&o, false).unwrap()).concat();
        assert!(in_file.contains("Market") && in_file.contains("Session"), "the transactions are listed");
        assert!(!in_file.contains("PRIVATE"), "a description is in the file");
        // Nor anywhere else in the raw bytes, where a font name or a stray string could carry one.
        let report = read(&w.conn, &o).unwrap();
        for raw in [report.pdf(&o).unwrap(), report.pdf_with(&o, false).unwrap()] {
            assert!(!raw.windows(7).any(|window| window == b"PRIVATE"));
        }

        o.show_descriptions = true;
        let shown = pdf_text(&read(&w.conn, &o).unwrap().pdf_with(&o, false).unwrap()).concat();
        for note in ["PRIVATE-PAY-NOTE", "PRIVATE-FOOD-NOTE", "PRIVATE-THERAPY-NOTE"] {
            assert!(shown.contains(note), "{note}");
        }
    }

    #[test]
    fn a_pdf_limited_to_some_categories_contains_nothing_about_the_others() {
        let w = world();
        let mut o = options("2026-01", "2026-03");
        o.category_ids = Some(vec![w.food.to_string()]);
        o.include_transactions = true;
        o.show_descriptions = true;
        let in_file = pdf_text(&read(&w.conn, &o).unwrap().pdf_with(&o, false).unwrap()).concat();
        assert!(in_file.contains("Food") && in_file.contains("Market"));
        for hidden in ["Rent", "Therapy", "Session", "Salary", "PRIVATE-THERAPY-NOTE", "PRIVATE-PAY-NOTE"] {
            assert!(!in_file.contains(hidden), "{hidden} is in the PDF");
        }
    }

    #[test]
    fn the_pdf_totals_are_the_dashboard_totals() {
        let w = world();
        let o = options("2026-01", "2026-01");
        let in_file = pdf_text(&read(&w.conn, &o).unwrap().pdf_with(&o, false).unwrap());
        let trend = &stats::year_trend(&w.conn, "2026").unwrap()[0];
        let f = DisplayFormat::default();
        for (label, minor) in [("income", money(&trend.income)), ("expense", money(&trend.expense)), ("savings", money(&trend.savings))] {
            assert!(in_file.contains(&f.money(minor)), "the {label} total {} is not in the PDF: {in_file:?}", f.money(minor));
        }
        assert!(in_file.contains(&f.percent(trend.savings_rate.unwrap())));
        let view = stats::budget_vs_actual(&w.conn, "2026-01").unwrap();
        for line in view.categories.iter().filter(|c| c.budgeted.is_some()) {
            let shown = format!("{} / {}", f.money(money(&line.actual)), f.money(money(line.budgeted.as_deref().unwrap())));
            assert!(in_file.contains(&shown), "{} {shown}", line.name);
            assert!(in_file.contains(&f.percent(line.pct_used.unwrap())), "{}", line.name);
        }
    }

    /// A PDF taken apart, for the tests. It understands what the report writer produces with compression
    /// off: plain objects, one text operation per line, and fonts that map the codes drawn on the page back
    /// to characters through a ToUnicode table. It is not a general PDF reader.
    struct PdfFile {
        pages: Vec<PdfPage>,
        title: Option<String>,
    }

    struct PdfPage {
        media_box: Vec<f32>,
        /// One string per text-showing operation, in drawing order.
        text: Vec<String>,
    }

    /// The numbers written as "N 0 R" in `text`, in order.
    fn references(text: &str) -> Vec<u32> {
        let words: Vec<&str> = text.split_whitespace().collect();
        words.windows(3).filter(|w| w[1] == "0" && w[2].trim_end_matches(']') == "R").filter_map(|w| w[0].trim_start_matches('[').parse().ok()).collect()
    }

    fn reference_after(dict: &str, key: &str) -> Option<u32> {
        let rest = &dict[dict.find(key)? + key.len()..];
        references(rest.split('/').next()?).first().copied()
    }

    fn read_cmap(cmap: &str) -> HashMap<u16, String> {
        let hex = |t: &str| u32::from_str_radix(t.trim_matches(|c| c == '<' || c == '>'), 16).unwrap();
        let utf16 = |t: &str| {
            let digits = t.trim_matches(|c| c == '<' || c == '>');
            let units: Vec<u16> = (0..digits.len() / 4).map(|i| u16::from_str_radix(&digits[i * 4..i * 4 + 4], 16).unwrap()).collect();
            String::from_utf16(&units).unwrap()
        };
        let mut map = HashMap::new();
        let mut mode = "";
        for line in cmap.lines().map(str::trim) {
            // A section starts with its size in front: "47 beginbfchar".
            if line.ends_with("beginbfchar") {
                mode = "char";
            } else if line.ends_with("beginbfrange") {
                mode = "range";
            } else if line == "endbfchar" || line == "endbfrange" {
                mode = "";
            } else if mode == "char" {
                let t: Vec<&str> = line.split_whitespace().collect();
                map.insert(hex(t[0]) as u16, utf16(t[1]));
            } else if mode == "range" {
                let t: Vec<&str> = line.split_whitespace().collect();
                for (i, code) in (hex(t[0])..=hex(t[1])).enumerate() {
                    let mut text = utf16(t[2]);
                    let last = text.pop().unwrap();
                    text.push(char::from_u32(last as u32 + i as u32).unwrap());
                    map.insert(code as u16, text);
                }
            }
        }
        map
    }

    /// The bytes of every `(...)` string in a line of page content, with the escapes undone.
    fn literal_strings(line: &str) -> Vec<u8> {
        let (mut out, mut depth, mut chars) = (Vec::new(), 0, line.chars().peekable());
        while let Some(c) = chars.next() {
            match (depth, c) {
                (_, '\\') if depth > 0 => {
                    let next = chars.next().unwrap();
                    match next {
                        '0'..='7' => {
                            let mut value = next.to_digit(8).unwrap();
                            for _ in 0..2 {
                                match chars.peek().and_then(|d| d.to_digit(8)) {
                                    Some(d) => {
                                        value = value * 8 + d;
                                        chars.next();
                                    }
                                    None => break,
                                }
                            }
                            out.push(value as u8);
                        }
                        'n' => out.push(b'\n'),
                        'r' => out.push(b'\r'),
                        't' => out.push(b'\t'),
                        'b' => out.push(8),
                        'f' => out.push(12),
                        other => out.push(other as u8),
                    }
                }
                (0, '(') => depth = 1,
                (_, '(') => {
                    depth += 1;
                    out.push(b'(');
                }
                (1, ')') => depth = 0,
                (_, ')') if depth > 1 => {
                    depth -= 1;
                    out.push(b')');
                }
                (d, c) if d > 0 => out.push(c as u8),
                _ => {}
            }
        }
        out
    }

    fn read_pdf(bytes: &[u8]) -> PdfFile {
        // One character per byte, so binary parts survive and the text parts can be searched.
        let raw: String = bytes.iter().map(|&b| b as char).collect();
        assert!(raw.starts_with("%PDF-"), "not a PDF");
        let mut objects: HashMap<u32, (String, Option<String>)> = HashMap::new();
        for chunk in raw.split("\nendobj") {
            let Some(at) = chunk.find(" 0 obj\n") else { continue };
            let number: u32 = chunk[..at].rsplit(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap();
            let body = &chunk[at + 7..];
            match body.split_once("\nstream\n") {
                Some((dict, stream)) => {
                    objects.insert(number, (dict.to_string(), Some(stream.split("\nendstream").next().unwrap().to_string())));
                }
                None => {
                    objects.insert(number, (body.to_string(), None));
                }
            }
        }
        let dict = |n: u32| objects[&n].0.clone();
        let tree = objects.values().find(|(d, _)| d.contains("/Type /Pages")).expect("a page tree");
        let kids = tree.0.split("/Kids").nth(1).unwrap().split(']').next().unwrap();
        let mut pages = Vec::new();
        for page in references(kids) {
            let d = dict(page);
            let media_box: Vec<f32> = d.split("/MediaBox [").nth(1).unwrap().split(']').next().unwrap().split_whitespace().map(|n| n.parse().unwrap()).collect();
            let fonts_text = d.split("/Font <<").nth(1).unwrap().split(">>").next().unwrap();
            let words: Vec<&str> = fonts_text.split_whitespace().collect();
            let mut maps: HashMap<String, HashMap<u16, String>> = HashMap::new();
            for pair in words.chunks(4) {
                let font = dict(pair[1].parse().unwrap());
                let cmap = reference_after(&font, "/ToUnicode").unwrap();
                maps.insert(pair[0].trim_start_matches('/').to_string(), read_cmap(objects[&cmap].1.as_ref().unwrap()));
            }
            let content = objects[&reference_after(&d, "/Contents").unwrap()].1.clone().unwrap();
            let (mut current, mut text) = (None, Vec::new());
            for line in content.lines() {
                if line.ends_with(" Tf") {
                    current = Some(line.split_whitespace().next().unwrap().trim_start_matches('/').to_string());
                } else if line.ends_with("TJ") || line.ends_with("Tj") {
                    let map = &maps[current.as_ref().expect("text before any font")];
                    let codes = literal_strings(line);
                    text.push(codes.chunks(2).map(|c| map.get(&u16::from_be_bytes([c[0], c[1]])).cloned().unwrap_or_else(|| "\u{fffd}".into())).collect());
                }
            }
            pages.push(PdfPage { media_box, text });
        }
        let title = objects.values().find_map(|(d, _)| d.split("/Title (").nth(1).map(|t| t.split(')').next().unwrap().to_string()));
        PdfFile { pages, title }
    }

    /// Every piece of text a viewer would let you copy, in drawing order.
    fn pdf_text(bytes: &[u8]) -> Vec<String> {
        read_pdf(bytes).pages.into_iter().flat_map(|p| p.text).collect()
    }

    #[test]
    fn the_pdf_is_a_valid_file_with_the_pages_of_the_layout() {
        let w = world();
        for page in [PageSize::A4, PageSize::Letter] {
            let mut o = options("2026-01", "2026-03");
            o.page = page;
            o.include_transactions = true;
            let report = read(&w.conn, &o).unwrap();
            let expected_pages = layout::build(&report.0, &o).unwrap().pages.len();
            let (width, height) = page.points();
            // The real file, compressed, and the readable one have the same pages and page size.
            for bytes in [report.pdf(&o).unwrap(), report.pdf_with(&o, false).unwrap()] {
                assert!(bytes.starts_with(b"%PDF-") && String::from_utf8_lossy(&bytes).trim_end().ends_with("%%EOF"));
                let file = read_pdf_pages_only(&bytes);
                assert_eq!(file.len(), expected_pages);
                for size in file {
                    assert!((size[2] - width).abs() < 0.1 && (size[3] - height).abs() < 0.1, "{size:?}");
                }
            }
            let readable = read_pdf(&report.pdf_with(&o, false).unwrap());
            assert_eq!(readable.pages.len(), expected_pages);
            assert!(readable.pages.iter().all(|p| p.media_box.len() == 4 && !p.text.is_empty()));
        }
    }

    /// The page sizes of a PDF, without reading the content (which a compressed file keeps packed).
    fn read_pdf_pages_only(bytes: &[u8]) -> Vec<Vec<f32>> {
        let raw: String = bytes.iter().map(|&b| b as char).collect();
        raw.split("/MediaBox [").skip(1).map(|t| t.split(']').next().unwrap().split_whitespace().map(|n| n.parse().unwrap()).collect()).collect()
    }

    #[test]
    fn the_pdf_carries_a_title_and_no_author_or_dates() {
        let w = world();
        let o = options("2026-01", "2026-01");
        let bytes = read(&w.conn, &o).unwrap().pdf(&o).unwrap();
        assert_eq!(read_pdf(&read(&w.conn, &o).unwrap().pdf_with(&o, false).unwrap()).title.as_deref(), Some("meowNY report"));
        let raw = String::from_utf8_lossy(&bytes).to_string();
        for absent in ["CreationDate", "ModDate", "/Author", "CreateDate", "ModifyDate", "dc:creator"] {
            assert!(!raw.contains(absent), "{absent} is in the file");
        }
    }
    /// Writes a sample report where a person can look at it: `MEOWNY_REPORT_DIR=/some/dir cargo test sample`.
    #[test]
    fn sample() {
        let Ok(dir) = std::env::var("MEOWNY_REPORT_DIR") else { return };
        let w = world();
        let food = w.food;
        for i in 0..70 {
            spend(&w.conn, food, &format!("Corner shop number {i}"), "7.30", "2026-02-20", Some("a little note about this purchase"));
        }
        let mut o = options("2026-01", "2026-03");
        o.format = DisplayFormat {
            date_order: DateOrder::DayMonthYear,
            date_separator: DateSeparator::Dot,
            currency_symbol: "€".into(),
            symbol_space: true,
            decimal_mark: ",".into(),
            group_separator: ".".into(),
            ..Default::default()
        };
        o.labels.period = "January – March 2026".into();
        o.include_transactions = true;
        o.show_descriptions = true;
        std::fs::write(format!("{dir}/report.pdf"), read(&w.conn, &o).unwrap().pdf(&o).unwrap()).unwrap();
        let single = options("2026-01", "2026-01");
        std::fs::write(format!("{dir}/month.pdf"), read(&w.conn, &single).unwrap().pdf(&single).unwrap()).unwrap();
    }

    // ---- the English texts the command line asks for ----

    const UI_ENGLISH: &str = include_str!("../../ui/src/i18n/en.ts");
    const UI_LABELS: &str = include_str!("../../ui/src/api/reportLabels.ts");

    /// The English text of a screen text, read from the UI's dictionary.
    fn ui_text(key: &str) -> String {
        // On Windows, git may have checked the file out with CRLF line ends.
        let english = UI_ENGLISH.replace("\r\n", "\n");
        let marker = format!("'{key}': '");
        let start = english.find(&marker).unwrap_or_else(|| panic!("no text {key} in the UI")) + marker.len();
        let end = english[start..].find("',\n").expect("the text ends");
        english[start..start + end].to_string()
    }

    fn capital(text: &str) -> String {
        let mut chars = text.chars();
        chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
    }

    fn field<'a>(labels: &'a ReportLabels, name: &str) -> &'a str {
        match name {
            "title" => &labels.title,
            "period" => &labels.period,
            "income" => &labels.income,
            "expenses" => &labels.expenses,
            "savings" => &labels.savings,
            "savingsRate" => &labels.savings_rate,
            "byMonth" => &labels.by_month,
            "spending" => &labels.spending,
            "other" => &labels.other,
            "budgetVsActual" => &labels.budget_vs_actual,
            "noBudget" => &labels.no_budget,
            "transactions" => &labels.transactions,
            "date" => &labels.date,
            "category" => &labels.category,
            "name" => &labels.name,
            "amount" => &labels.amount,
            "page" => &labels.page,
            "noData" => &labels.no_data,
            other => panic!("the UI has a label the app does not know: {other}"),
        }
    }

    /// The command line writes a report in English without the window's help. Its texts are the ones the
    /// window would give in English: this reads the UI's own recipe for each label and compares.
    #[test]
    fn the_english_labels_are_the_texts_the_ui_builds_in_english() {
        let labels = ReportLabels::english("2026-01", "2026-03").unwrap();
        let mut seen = Vec::new();
        for line in UI_LABELS.lines().map(str::trim) {
            let Some((name, recipe)) = line.split_once(": ") else { continue };
            let recipe = recipe.trim_end_matches(',');
            let (capitalised, key) = if let Some(inner) = recipe.strip_prefix("capital(t('").and_then(|r| r.strip_suffix("'))")) {
                (true, inner)
            } else if let Some(inner) = recipe.strip_prefix("t('").and_then(|r| r.strip_suffix("')")) {
                (false, inner)
            } else {
                continue;
            };
            let english = ui_text(key);
            let expected = if capitalised { capital(&english) } else { english };
            assert_eq!(field(&labels, name), expected, "the label {name} (UI text {key})");
            seen.push(name.to_string());
        }
        // Every label but the period comes from a text; none was skipped by the reader above.
        assert_eq!(seen.len(), 17, "{seen:?}");
        assert!(!seen.contains(&"period".to_string()));
    }

    #[test]
    fn the_period_is_written_out_in_english() {
        assert_eq!(ReportLabels::english("2026-10", "2026-10").unwrap().period, "October 2026");
        assert_eq!(ReportLabels::english("2026-10", "2027-02").unwrap().period, "October 2026 – February 2027");
        assert_eq!(ReportLabels::english("2026-01", "2026-12").unwrap().period, "January 2026 – December 2026");
        for bad in ["2026-13", "2026-00", "2026", "x-1", "26-10", "2026-1x", ""] {
            assert_eq!(ReportLabels::english(bad, "2026-10").unwrap_err(), AppError::InvalidInput, "{bad:?}");
            assert_eq!(ReportLabels::english("2026-10", bad).unwrap_err(), AppError::InvalidInput, "{bad:?}");
        }
    }

    #[test]
    fn english_labels_pass_the_reports_own_limits() {
        let labels = ReportLabels::english("2026-01", "2026-01").unwrap();
        assert!(labels.all().iter().all(|l| !l.is_empty() && l.chars().count() <= LABEL_MAX_CHARS && !l.chars().any(char::is_control)));
    }

    // ---- debts and their history in the report ---------------------------------------------------------------

    use crate::debts::fixtures::{equal, friend, who};
    use crate::debts::ledger::{self, Kind, NewEntry};
    use crate::debts::splits::set_split;

    /// The world with two friends: Anna has half of the Dinner (Food) and Bob half of the Session (Therapy); Anna was lent 25.00 and paid
    /// back 10.00 in January, Bob lent 7.50 in February, and Anna was lent 5.00 in April, after the months of the reports below.
    fn indebted() -> World {
        let w = world();
        let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
        let id = |name: &str, date: &str| -> i64 { w.conn.query_row("SELECT id FROM transactions WHERE name = ?1 AND payment_date = ?2", [name, date], |r| r.get(0)).unwrap() };
        set_split(&w.conn, id("Dinner", "2026-01-20"), &equal(vec![who(anna)])).unwrap();
        set_split(&w.conn, id("Session", "2026-02-10"), &equal(vec![who(bob)])).unwrap();
        for (person, kind, amount, date) in
            [(anna, Kind::Lent, "25.00", "2026-01-02"), (anna, Kind::RepaidToMe, "10.00", "2026-01-28"), (bob, Kind::Borrowed, "7.50", "2026-02-02"), (anna, Kind::Lent, "5.00", "2026-04-03")]
        {
            ledger::add(&w.conn, &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: date.into(), note: None, transaction_id: None }).unwrap();
        }
        w
    }

    fn with_debts(from: &str, to: &str, friends: bool, history: bool) -> ReportOptions {
        let mut o = options(from, to);
        o.include_debts = friends;
        o.include_debt_history = history;
        o.debts_labels = Some(crate::debts::summary::Labels::english());
        o
    }

    #[test]
    fn debts_and_their_history_are_in_the_report_only_when_asked_for() {
        let w = indebted();
        let plain = texts(&options("2026-01", "2026-01"), &w);
        for absent in ["Who owes what", "History of debts", "Anna", "Owed to you"] {
            assert!(!plain.iter().any(|t| t.contains(absent)), "{absent:?} is in a report that did not ask for debts");
        }
        let only_friends = texts(&with_debts("2026-01", "2026-01", true, false), &w);
        assert!(only_friends.iter().any(|t| t == "Who owes what") && !only_friends.iter().any(|t| t == "History of debts"));
        let only_history = texts(&with_debts("2026-01", "2026-01", false, true), &w);
        assert!(only_history.iter().any(|t| t == "History of debts") && !only_history.iter().any(|t| t == "Who owes what"));
    }

    #[test]
    fn who_owes_what_is_what_was_owed_on_the_last_day_of_the_months() {
        let w = indebted();
        let say = |from: &str, to: &str| texts(&with_debts(from, to, true, false), &w);
        let has = |text: &[String], needle: &str| text.iter().any(|t| t.contains(needle));
        // January: Anna owes 50.00 for the dinner and 25.00 lent, less the 10.00 she paid back; Bob owes and is owed nothing yet
        let january = say("2026-01", "2026-01");
        assert!(has(&january, "Anna") && has(&january, "owes you 65.00"), "{january:?}");
        assert!(!has(&january, "Bob"), "Bob had no debt on the 31st of January: {january:?}");
        assert!(has(&january, "Owed to you") && has(&january, "You owe") && has(&january, "Net"));
        // January and February: Bob lent 7.50 and has half of a 80.00 session, so he owes 32.50, and Anna's April loan is not counted
        let two = say("2026-01", "2026-02");
        assert!(has(&two, "Bob") && has(&two, "owes you 32.50") && has(&two, "owes you 65.00"), "{two:?}");
        assert!(!has(&two, "70.00"), "the loan of April is after the months: {two:?}");
        // a friend who is owed (a loan from the friend) says so the other way round
        let w2 = indebted();
        let (cara,) = (friend(&w2.conn, "Cara"),);
        ledger::add(&w2.conn, &NewEntry { person_id: cara.to_string(), kind: Kind::Borrowed, amount: "12.00".into(), date: "2026-01-09".into(), note: None, transaction_id: None }).unwrap();
        let owed = texts(&with_debts("2026-01", "2026-01", true, false), &w2);
        assert!(owed.iter().any(|t| t.contains("you owe 12.00")), "{owed:?}");
    }

    #[test]
    fn the_history_has_every_line_of_the_months_with_the_balance_after_each() {
        let w = indebted();
        let text = texts(&with_debts("2026-01", "2026-01", false, true), &w);
        let has = |needle: &str| text.iter().any(|t| t.contains(needle));
        // Anna in January, by date: lent 25.00 (balance 25.00), her half of the dinner (75.00), paid back 10.00 (65.00)
        for needle in ["lent", "share of a bill: Dinner", "paid back to me", "+25.00", "+50.00", "-10.00", "25.00", "75.00", "65.00", "Date", "Friend", "What", "Change", "Balance"] {
            assert!(has(needle), "{needle:?} is not in the history: {text:?}");
        }
        // the lines of other months are not
        assert!(!has("Session") && !has("7.50") && !text.iter().any(|t| t == "+5.00"), "{text:?}");
        let order: Vec<&String> = text.iter().filter(|t| t.starts_with("lent") || t.starts_with("share of a bill") || t.starts_with("paid back")).collect();
        assert_eq!(order.len(), 3, "{order:?}");
        let at = |prefix: &str| order.iter().position(|t| t.starts_with(prefix)).unwrap();
        assert!(at("lent") < at("share of a bill") && at("share of a bill") < at("paid back"), "by date: {order:?}");
    }

    #[test]
    fn with_some_categories_the_history_leaves_out_the_bills_of_the_others_and_the_balance_that_would_add_them_up() {
        let w = indebted();
        let mut o = with_debts("2026-01", "2026-02", true, true);
        o.category_ids = Some(vec![w.food.to_string()]);
        let text = texts(&o, &w);
        let has = |needle: &str| text.iter().any(|t| t.contains(needle));
        // Anna's half of the dinner (Food) is there; Bob's half of the session (Therapy) is not, and neither is its name
        assert!(has("share of a bill: Dinner") && !has("Session"), "{text:?}");
        // Bob's own loan is: a ledger entry has no category
        assert!(has("borrowed") && has("-7.50"), "{text:?}");
        // the history has no column of balances, and so does not add up what it leaves out; who owes what still says what is owed
        assert!(!text.iter().any(|t| t == "Balance"), "{text:?}");
        assert!(has("owes you 32.50"), "the friends are as the debts screens say: {text:?}");
        // every category: the column is there
        let all = texts(&with_debts("2026-01", "2026-02", false, true), &w);
        assert!(all.iter().any(|t| t == "Balance"));
    }

    #[test]
    fn debts_with_no_words_for_them_or_with_bad_ones_are_refused() {
        let w = indebted();
        let mut none = with_debts("2026-01", "2026-01", true, false);
        none.debts_labels = None;
        assert_eq!(read(&w.conn, &none).err(), Some(AppError::InvalidInput));
        let mut long = with_debts("2026-01", "2026-01", false, true);
        long.debts_labels.as_mut().unwrap().history_title = "x".repeat(121);
        assert_eq!(read(&w.conn, &long).err(), Some(AppError::InvalidInput));
        let mut control = with_debts("2026-01", "2026-01", false, true);
        control.debts_labels.as_mut().unwrap().balance = "Bal\nance".into();
        assert_eq!(read(&w.conn, &control).err(), Some(AppError::InvalidInput));
        // words that are given but not needed are checked all the same
        let mut unneeded = options("2026-01", "2026-01");
        unneeded.debts_labels = Some(crate::debts::summary::Labels::english());
        unneeded.debts_labels.as_mut().unwrap().nobody = "x".repeat(121);
        assert_eq!(read(&w.conn, &unneeded).err(), Some(AppError::InvalidInput));
    }

    #[test]
    fn a_month_with_debts_and_no_money_shows_the_debts_and_not_the_empty_message() {
        let w = indebted();
        let text = texts(&with_debts("2026-04", "2026-04", true, true), &w);
        let has = |needle: &str| text.iter().any(|t| t.contains(needle));
        assert!(!has("No transactions in this period."), "{text:?}");
        assert!(has("Who owes what") && has("History of debts") && has("+5.00"), "{text:?}");
        // and a month with neither says so
        let empty = texts(&with_debts("2025-01", "2025-01", true, true), &w);
        assert!(empty.iter().any(|t| t.contains("Nobody owes anything")), "{empty:?}");
    }

    #[test]
    fn a_long_history_goes_on_to_more_pages_and_its_heads_come_with_it() {
        let w = indebted();
        let anna: i64 = w.conn.query_row("SELECT id FROM people WHERE name = 'Anna'", [], |r| r.get(0)).unwrap();
        for day in 1..=28 {
            for _ in 0..4 {
                ledger::add(&w.conn, &NewEntry { person_id: anna.to_string(), kind: Kind::Lent, amount: "1.00".into(), date: format!("2026-03-{day:02}"), note: Some("a note".into()), transaction_id: None }).unwrap();
            }
        }
        let o = with_debts("2026-03", "2026-03", false, true);
        let report = read(&w.conn, &o).unwrap();
        let doc = layout::build(&report.0, &o).unwrap();
        assert!(doc.pages.len() >= 3, "{} pages", doc.pages.len());
        let heads = doc.texts().iter().filter(|t| **t == "Change").count();
        assert!(heads >= 2, "the heads of the columns are on every page the table is on: {heads}");
        assert_eq!(doc.texts().iter().filter(|t| t.starts_with("lent: a note")).count(), 112);
        // the pdf itself is written
        assert!(report.pdf(&o).unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn the_recurring_payments_that_have_come_due_are_listed_under_the_debts_with_what_they_come_to() {
        let w = indebted();
        let item = |name: &str, amount: Option<&str>, category: i64, start: &str| {
            crate::recurring::create(
                &w.conn,
                &crate::recurring::NewRecurring {
                    name: name.into(),
                    amount: amount.map(str::to_string),
                    category_id: category.to_string(),
                    start_date: start.into(),
                    interval_unit: crate::recurring::Unit::Month,
                    interval_count: 1,
                    end_date: None,
                    description: None,
                },
            )
            .unwrap()
        };
        // two that have come due by the 15th of March, one that has not, and one that is paused
        item("Flat rent", Some("900.00"), w.rent, "2026-02-01");
        item("Phone bill", None, w.food, "2026-03-01");
        item("Gym", Some("30.00"), w.rent, "2026-04-01");
        let paused = item("Old paper", Some("5.00"), w.food, "2026-01-01");
        crate::recurring::set_active(&w.conn, paused.id, false).unwrap();
        let now: chrono::DateTime<chrono::Utc> = "2026-03-15T12:00:00Z".parse().unwrap();
        let say = |o: &ReportOptions| -> Vec<String> {
            let report = read_at(&w.conn, o, now).unwrap();
            layout::build(&report.0, o).unwrap().texts().iter().map(|t| t.to_string()).collect()
        };
        let has = |text: &[String], needle: &str| text.iter().any(|t| t.contains(needle));

        let text = say(&with_debts("2026-01", "2026-02", true, false));
        assert!(has(&text, "DUE PAYMENTS") && has(&text, "Flat rent") && has(&text, "Phone bill"), "{text:?}");
        assert!(has(&text, "2026-02-01") && has(&text, "2026-03-01"), "the day each fell due: {text:?}");
        assert!(!has(&text, "Gym") && !has(&text, "Old paper"), "not due yet, and paused: {text:?}");
        // the total is of the payments that say what they are for; the phone bill says only that it is due
        assert!(has(&text, "Total due") && text.iter().any(|t| t == "900.00"), "{text:?}");
        assert!(text.iter().any(|t| t == "\u{2013}"), "a bill with no fixed amount has a dash: {text:?}");
        // with no friend owing anything the due payments are still there
        assert!(has(&text, "Nobody owes anything") || has(&text, "owes you"), "{text:?}");

        // asked for the history only: no list of debts, so no due payments
        let history = say(&with_debts("2026-01", "2026-02", false, true));
        assert!(!has(&history, "DUE PAYMENTS") && !has(&history, "Flat rent"), "{history:?}");

        // some categories: only the payments of those
        let mut food = with_debts("2026-01", "2026-02", true, false);
        food.category_ids = Some(vec![w.food.to_string()]);
        let limited = say(&food);
        assert!(has(&limited, "Phone bill") && !has(&limited, "Flat rent"), "{limited:?}");
        // a day before the first one fell due there is nothing to list
        let early: chrono::DateTime<chrono::Utc> = "2026-01-15T12:00:00Z".parse().unwrap();
        let o = with_debts("2026-01", "2026-02", true, false);
        let none: Vec<String> = layout::build(&read_at(&w.conn, &o, early).unwrap().0, &o).unwrap().texts().iter().map(|t| t.to_string()).collect();
        assert!(!has(&none, "DUE PAYMENTS") && !has(&none, "Flat rent"), "{none:?}");
        // a month of nothing but debts, with a payment due, shows both
        let quiet = with_debts("2026-06", "2026-06", true, false);
        let report = read_at(&w.conn, &quiet, now).unwrap();
        let page: Vec<String> = layout::build(&report.0, &quiet).unwrap().texts().iter().map(|t| t.to_string()).collect();
        assert!(!has(&page, "No transactions in this period.") && has(&page, "Flat rent"), "{page:?}");
    }
}
