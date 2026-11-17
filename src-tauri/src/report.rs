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
//!
//! A report of every category also has a short table of the accounts, once accounts are in use: what each held on the last day, what it
//! changed over the months, and the net worth on that day. An account that net worth leaves out is listed and marked, and is not in the
//! net worth (`docs/accounts-design.md`, sections 10 and 20.3). An account in a currency of its own is listed in that currency, with its code
//! in front of the numbers, and is turned into the app's currency at its rate for the net worth, which is the only total (section 21.6).

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

use crate::accounts;
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
    /// The table of accounts: its title, its two columns of numbers and its last line. A caller from before accounts may leave them out, and
    /// they are then in English, as the command line writes them.
    #[serde(default = "words::accounts")]
    pub accounts: String,
    #[serde(default = "words::balance")]
    pub balance: String,
    #[serde(default = "words::change")]
    pub change: String,
    #[serde(default = "words::net_worth")]
    pub net_worth: String,
    /// The mark of an account that net worth leaves out. A caller from before the account details may leave it out too.
    #[serde(default = "words::not_in_net_worth")]
    pub not_in_net_worth: String,
}

/// The English words of the table of accounts.
mod words {
    pub(super) fn accounts() -> String {
        "Accounts".to_string()
    }

    pub(super) fn balance() -> String {
        "Balance".to_string()
    }

    pub(super) fn change() -> String {
        "Change".to_string()
    }

    pub(super) fn net_worth() -> String {
        "Net worth".to_string()
    }

    pub(super) fn not_in_net_worth() -> String {
        "(not in net worth)".to_string()
    }
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
            accounts: words::accounts(),
            balance: words::balance(),
            change: words::change(),
            net_worth: words::net_worth(),
            not_in_net_worth: words::not_in_net_worth(),
        })
    }

    fn all(&self) -> [&String; 23] {
        [
            &self.title, &self.period, &self.income, &self.expenses, &self.savings, &self.savings_rate, &self.by_month,
            &self.spending, &self.other, &self.budget_vs_actual, &self.no_budget, &self.transactions, &self.date,
            &self.category, &self.name, &self.amount, &self.page, &self.no_data, &self.accounts, &self.balance, &self.change, &self.net_worth,
            &self.not_in_net_worth,
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
    /// The recurring payments that have come due and are not paid or skipped.
    #[serde(default)]
    pub include_due_payments: bool,
    /// Every line of the debts of every friend in the months, with the balance after it, and the recurring payments paid or due in them.
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
    /// The accounts, in a report of every category once accounts are in use.
    pub accounts: Option<AccountsPart>,
}

/// What a report says about accounts, as of the last day of its months.
pub(crate) struct AccountsPart {
    /// Each account, the archived ones last: its name, what it held on the last day, what it changed over the months, whether net worth counts
    /// it, and the code of its currency when it has one of its own (the two numbers are then in that currency). An account that net worth leaves
    /// out is listed all the same, with its balance.
    pub accounts: Vec<(String, i64, i64, bool, Option<String>)>,
    /// What every account that counts held on the last day plus what friends owed you then, as the dashboard counts it (`accounts::summary`).
    pub net_worth: i64,
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

fn load(conn: &Connection, options: &ReportOptions, now: chrono::DateTime<chrono::Utc>, accounts_in_use: bool) -> Result<ReportData, AppError> {
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

    let debts = if options.include_debts || options.include_due_payments || options.include_debt_history {
        let wanted = (options.include_debts, options.include_due_payments, options.include_debt_history);
        Some(export::debts::report_part(conn, &sel, &first_day.to_string(), &last_day.to_string(), wanted, &export::today_of(conn, now)?)?)
    } else {
        None
    };

    // the accounts belong to a report of everything: a limit to some categories would leave out what moved them
    let accounts = if accounts_in_use && sel.categories.is_none() { Some(accounts_part(conn, &first_day.to_string(), &last_day.to_string())?) } else { None };

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
        accounts,
    })
}

/// The accounts of a report: what each held on `last_day`, what it changed from `first_day` to `last_day` (both included), and the net worth on
/// `last_day`, which has the accounts that count in it and not the others (`docs/accounts-design.md`, sections 3 and 20.3). The numbers are
/// sums of the movements of the accounts, as the account screens have them; what friends owed you is read from their statements, as in the
/// debts of a report. A virtual balance is in none of it. The numbers of an account in a currency of its own are in that currency: the net
/// worth turns each balance into the app's currency at the account's rate, one account at a time, as `accounts::summary` does, and adds up
/// nothing in two currencies.
fn accounts_part(conn: &Connection, first_day: &str, last_day: &str) -> Result<AccountsPart, AppError> {
    let mut statement = conn.prepare(
        "SELECT a.name,
                COALESCE((SELECT SUM(m.delta_minor) FROM account_movements m WHERE m.account_id = a.id AND m.day <= ?1), 0),
                COALESCE((SELECT SUM(m.delta_minor) FROM account_movements m WHERE m.account_id = a.id AND m.day < ?2), 0),
                a.include_net_worth, a.currency, a.rate
         FROM accounts a ORDER BY a.archived, a.position, a.id",
    )?;
    // (name, held on the last day, held before the first, counts in net worth, currency of its own, rate)
    type Read = (String, i64, i64, bool, Option<String>, Option<String>);
    let rows: Vec<Read> = statement
        .query_map([last_day, first_day], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? == 1, r.get(4)?, r.get(5)?)))?
        .collect::<Result<_, _>>()?;
    // what the accounts that count hold, in the app's currency; one that does not fit shows the bound it passed
    let in_accounts: i64 = rows
        .iter()
        .filter(|row| row.3)
        .map(|(_, after, _, _, _, rate)| match rate {
            None => *after,
            Some(rate) => crate::accounts::to_main(*after, rate).unwrap_or(crate::money::MAX_MINOR * after.signum()),
        })
        .sum();
    let accounts: Vec<(String, i64, i64, bool, Option<String>)> = rows.into_iter().map(|(name, after, before, counts, currency, _)| (name, after, after - before, counts, currency)).collect();
    let mut friends = 0;
    for person in crate::debts::people::list(conn, true)? {
        let statement = crate::debts::ledger::statement_in(conn, person.id)?;
        friends += statement.lines.iter().rev().find(|l| l.date.as_str() <= last_day).map_or(0, |l| l.balance_minor);
    }
    Ok(AccountsPart { net_worth: in_accounts + friends, accounts })
}

/// The numbers of a report, read from the database. Laying out and writing the PDF (`Report::pdf`)
/// needs no database, so a caller can release the connection first.
pub struct Report(ReportData);

/// Reads the numbers for a report in one read transaction, so they agree with each other.
pub fn read(conn: &Connection, options: &ReportOptions) -> Result<Report, AppError> {
    read_at(conn, options, chrono::Utc::now())
}

/// The same, as of `now`: the recurring payments that are overdue are the ones whose day is before that day.
pub(crate) fn read_at(conn: &Connection, options: &ReportOptions, now: chrono::DateTime<chrono::Utc>) -> Result<Report, AppError> {
    options.format.validate()?;
    if options.labels.all().iter().any(|l| l.chars().count() > LABEL_MAX_CHARS || l.chars().any(char::is_control)) {
        return Err(AppError::InvalidInput);
    }
    // the words of the debts parts are needed for them and checked like the others
    if let Some(labels) = &options.debts_labels {
        labels.validate()?;
    }
    if (options.include_debts || options.include_due_payments || options.include_debt_history) && options.debts_labels.is_none() {
        return Err(AppError::InvalidInput);
    }
    // whether accounts are in use is asked first: `summary` has a transaction of its own, which cannot start inside the one below
    let accounts_in_use = accounts::summary(conn, &export::today_of(conn, now)?)?.in_use;
    let tx = conn.unchecked_transaction()?;
    Ok(Report(load(&tx, options, now, accounts_in_use)?))
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
    use super::layout::{label_stride, Metrics, Op};
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
            accounts: s("Accounts"),
            balance: s("Balance"),
            change: s("Change"),
            net_worth: s("Net worth"),
            not_in_net_worth: s("(not in net worth)"),
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
            include_due_payments: false,
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
                account_id: None,
                native_amount: None,
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
    fn a_line_break_in_a_description_is_a_space_in_the_pdf() {
        assert_eq!(Metrics::new().unwrap().clean("taxi\nhome\u{7}"), "taxi home");
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
    fn labels_along_an_axis_are_thinned_out_to_fit() {
        assert_eq!(label_stride(12, 40.0, 30.0), 1);
        assert_eq!(label_stride(36, 12.0, 30.0), 3);
        assert_eq!(label_stride(120, 3.0, 30.0), 12);
        assert_eq!(label_stride(1, 400.0, 30.0), 1);
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
            pages.push(PdfPage { text });
        }
        let title = objects.values().find_map(|(d, _)| d.split("/Title (").nth(1).map(|t| t.split(')').next().unwrap().to_string()));
        PdfFile { pages, title }
    }

    /// Every piece of text a viewer would let you copy, in drawing order.
    fn pdf_text(bytes: &[u8]) -> Vec<String> {
        read_pdf(bytes).pages.into_iter().flat_map(|p| p.text).collect()
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
            ledger::add(&w.conn, &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: date.into(), note: None, transaction_id: None, account_id: None }).unwrap();
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
        ledger::add(&w2.conn, &NewEntry { person_id: cara.to_string(), kind: Kind::Borrowed, amount: "12.00".into(), date: "2026-01-09".into(), note: None, transaction_id: None, account_id: None }).unwrap();
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
                ledger::add(&w.conn, &NewEntry { person_id: anna.to_string(), kind: Kind::Lent, amount: "1.00".into(), date: format!("2026-03-{day:02}"), note: Some("a note".into()), transaction_id: None, account_id: None }).unwrap();
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
    fn the_recurring_payments_that_have_come_due_are_a_list_of_their_own_with_every_occurrence_and_what_they_come_to() {
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
                    account_id: None,
                },
            )
            .unwrap()
        };
        // two that have come due by the 15th of March (the rent twice: the 1st of February and the 1st of March), one that has not, and one
        // that is paused
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
        let due_only = |from: &str, to: &str| {
            let mut o = options(from, to);
            o.include_due_payments = true;
            o.debts_labels = Some(crate::debts::summary::Labels::english());
            o
        };

        let text = say(&due_only("2026-01", "2026-02"));
        assert!(has(&text, "Due payments") && has(&text, "Flat rent") && has(&text, "Phone bill"), "{text:?}");
        assert_eq!(text.iter().filter(|t| *t == "Flat rent").count(), 2, "one for each month that came due: {text:?}");
        assert!(has(&text, "2026-02-01") && has(&text, "2026-03-01"), "the day each fell due: {text:?}");
        assert!(!has(&text, "Gym") && !has(&text, "Old paper"), "not due yet, and paused: {text:?}");
        // the total is of the payments that say what they are for; the phone bill says only that it is due
        assert!(has(&text, "Total due") && text.iter().any(|t| t == "1,800.00"), "{text:?}");
        assert!(text.iter().any(|t| t == "\u{2013}"), "a bill with no fixed amount has a dash: {text:?}");
        // a list of its own: the friends are not listed unless they are asked for
        assert!(!has(&text, "Who owes what"), "{text:?}");

        // asked for the friends and the history only: no due payments
        let others = say(&with_debts("2026-01", "2026-02", true, true));
        assert!(!has(&others, "Due payments") && !has(&others, "Total due"), "{others:?}");
        // the history, though, says what is overdue, with the others
        assert!(has(&others, "recurring payment overdue: Flat rent"), "{others:?}");

        // some categories: only the payments of those
        let mut food = due_only("2026-01", "2026-02");
        food.category_ids = Some(vec![w.food.to_string()]);
        let limited = say(&food);
        assert!(has(&limited, "Phone bill") && !has(&limited, "Flat rent"), "{limited:?}");
        // before the first one fell due there is nothing to list, and the report says so
        let early: chrono::DateTime<chrono::Utc> = "2026-01-15T12:00:00Z".parse().unwrap();
        let o = due_only("2026-01", "2026-02");
        let none: Vec<String> = layout::build(&read_at(&w.conn, &o, early).unwrap().0, &o).unwrap().texts().iter().map(|t| t.to_string()).collect();
        assert!(has(&none, "Due payments") && has(&none, "Nobody owes anything") && !has(&none, "Flat rent"), "{none:?}");
        // a month of nothing but debts, with payments due, shows them and not the empty message
        let report = read_at(&w.conn, &due_only("2026-06", "2026-06"), now).unwrap();
        let page: Vec<String> = layout::build(&report.0, &due_only("2026-06", "2026-06")).unwrap().texts().iter().map(|t| t.to_string()).collect();
        assert!(!has(&page, "No transactions in this period.") && has(&page, "Flat rent"), "{page:?}");
    }

    #[test]
    fn a_recurring_payment_that_was_paid_is_in_the_history_with_the_day_it_fell_due_and_no_balance() {
        let w = indebted();
        let rent = crate::recurring::create(
            &w.conn,
            &crate::recurring::NewRecurring {
                name: "Flat rent".into(),
                amount: Some("900.00".into()),
                category_id: w.rent.to_string(),
                start_date: "2026-01-01".into(),
                interval_unit: crate::recurring::Unit::Month,
                interval_count: 1,
                end_date: Some("2026-02-15".into()),
                description: None,
                account_id: None,
            },
        )
        .unwrap();
        let now: chrono::DateTime<chrono::Utc> = "2026-03-15T12:00:00Z".parse().unwrap();
        crate::recurring::pay(&w.conn, rent.id, None, Some("2026-01-04"), "2026-03-01", None, None).unwrap();
        let o = with_debts("2026-01", "2026-02", false, true);
        let report = read_at(&w.conn, &o, now).unwrap();
        let text: Vec<String> = layout::build(&report.0, &o).unwrap().texts().iter().map(|t| t.to_string()).collect();
        let has = |needle: &str| text.iter().any(|t| t.contains(needle));
        assert!(has("recurring payment paid late: Flat rent") && has("recurring payment overdue: Flat rent"), "{text:?}");
        // the day it was paid, and what was paid
        assert!(has("2026-01-04") && text.iter().filter(|t| *t == "900.00").count() == 2, "{text:?}");
        // a friend's lines still have the balance, and the payments do not
        assert!(has("Balance") && has("65.00"), "{text:?}");
    }

    // ---- accounts in the report (docs/accounts-design.md, section 10) -------------------------------------------------

    use crate::accounts::fixtures::{new_account, transfer};
    use crate::accounts::{Kind as AccountKind, NewAccount, MAIN_ID};

    fn at(text: &str) -> chrono::DateTime<chrono::Utc> {
        text.parse().unwrap()
    }

    fn open_account(conn: &Connection, name: &str, kind: AccountKind, opening: &str, date: &str) -> i64 {
        accounts::create(conn, &NewAccount { kind, opening_balance: Some(opening.into()), opening_date: Some(date.into()), ..new_account(name) }, "2026-04-15").unwrap().id
    }

    /// The world (the main account holds 5,713.80 on the last day of March: 9,600 of pay, 2,700 of rent, 886.20 of other things, and the 300 that
    /// left it in two transfers) with three accounts more. Savings opened with 1,000 and took 200 on the 2nd of February, 50 from the old account
    /// on the 10th of January and 50 on the 1st of April, after the months below. The credit card opened with -240 and took 100 in March. The
    /// old account opened with 50, gave it all to Savings and is archived.
    fn with_accounts() -> World {
        let w = world();
        let savings = open_account(&w.conn, "Savings", AccountKind::Savings, "1000.00", "2025-12-01");
        let card = open_account(&w.conn, "Credit card", AccountKind::CreditCard, "-240.00", "2025-12-01");
        let old = open_account(&w.conn, "Old", AccountKind::Savings, "50.00", "2025-12-01");
        transfer(&w.conn, MAIN_ID, savings, "200.00", "2026-02-02");
        transfer(&w.conn, MAIN_ID, card, "100.00", "2026-03-15");
        transfer(&w.conn, MAIN_ID, savings, "50.00", "2026-04-01");
        transfer(&w.conn, old, savings, "50.00", "2026-01-10");
        accounts::archive(&w.conn, old, "2026-04-15").unwrap();
        w
    }

    const APRIL: &str = "2026-04-15T12:00:00Z";

    fn accounts_of(o: &ReportOptions, w: &World, now: &str) -> Option<AccountsPart> {
        read_at(&w.conn, o, at(now)).unwrap().0.accounts
    }

    #[test]
    fn the_accounts_table_has_what_each_held_on_the_last_day_what_it_changed_and_the_net_worth() {
        let w = with_accounts();
        let o = options("2026-01", "2026-03");
        let part = accounts_of(&o, &w, APRIL).expect("accounts are in use");
        assert_eq!(
            part.accounts,
            [("Main account".to_string(), 571_380, 571_380, true, None), ("Savings".into(), 125_000, 25_000, true, None), ("Credit card".into(), -14_000, 10_000, true, None), ("Old".into(), 0, -5_000, true, None)],
            "the person's order, the archived one last; the change leaves out what the account held before the first day"
        );
        assert_eq!(part.net_worth, 571_380 + 125_000 - 14_000);
        // the same numbers as the account screens and the dashboard: the transfers add up to nothing, so the 50 of April changes no sum
        let summary = accounts::summary(&w.conn, "2026-04-15").unwrap();
        assert_eq!(part.accounts.iter().map(|a| a.1).sum::<i64>().to_string(), summary.in_accounts.replace('.', ""));
        assert_eq!(part.net_worth.to_string(), summary.net_worth.replace('.', ""));
        for (name, balance, _, _, _) in &part.accounts {
            let id: i64 = w.conn.query_row("SELECT id FROM accounts WHERE name = ?1", [name], |r| r.get(0)).unwrap();
            let held = accounts::history(&w.conn, id, "2026", "2026-04-15").unwrap();
            assert_eq!(format!("{}.{:02}", balance / 100, (balance % 100).abs()), held[2].balance, "{name}: the chart has the balance of the end of March");
        }
        // one month: the change is that of the month, and the rest is what was held before it. Main account: two pays, two rents, 356.20 of food
        // in January, 450.00 and 80.00 in February and the 200.00 to Savings; and of those, in February: one pay, one rent, 450.00, 80.00 and 200.00
        let february = accounts_of(&options("2026-02", "2026-02"), &w, APRIL).unwrap();
        assert_eq!(february.accounts[0], ("Main account".to_string(), 351_380, 157_000, true, None));
        assert_eq!(february.accounts[1], ("Savings".to_string(), 125_000, 20_000, true, None), "1,000.00 and 50.00 from the old account, and the 200.00 of the month");
    }

    #[test]
    fn the_accounts_table_is_on_the_page_with_the_words_of_the_labels_and_the_net_worth_last() {
        let w = with_accounts();
        let o = options("2026-01", "2026-03");
        let page = texts(&o, &w);
        let place = |needle: &str| page.iter().position(|t| t == needle).unwrap_or_else(|| panic!("{needle:?} is not on the page: {page:?}"));
        for word in ["Accounts", "Name", "Change", "Balance", "Net worth", "Main account", "Savings", "Credit card", "Old"] {
            place(word);
        }
        assert!(page.iter().any(|t| t == "5,713.80") && page.iter().any(|t| t == "+5,713.80"), "{page:?}");
        assert!(page.iter().any(|t| t == "-140.00") && page.iter().any(|t| t == "+100.00") && page.iter().any(|t| t == "-50.00"), "a negative balance and a change are signed: {page:?}");
        assert!(page.iter().any(|t| t == "6,823.80"), "the net worth: {page:?}");
        // (the first "Savings" is a tile of the page: the account is after the title of the table)
        let in_table = |needle: &str| place("Accounts") + page[place("Accounts")..].iter().position(|t| t == needle).unwrap();
        assert!(place("Accounts") < in_table("Main account") && in_table("Main account") < in_table("Savings") && in_table("Savings") < in_table("Credit card"));
        assert!(in_table("Credit card") < in_table("Old") && in_table("Old") < place("Net worth"));
        assert!(place("Where the money went") < place("Accounts"), "after the budgets, before the list of transactions");
        let mut listed = options("2026-01", "2026-03");
        listed.include_transactions = true;
        let with_list = texts(&listed, &w);
        assert!(with_list.iter().position(|t| t == "Net worth") < with_list.iter().position(|t| t == "Transactions"), "the table comes before the long list");
        // the words are the labels'
        let mut french = options("2026-01", "2026-03");
        french.labels.accounts = "Comptes".into();
        french.labels.balance = "Solde".into();
        french.labels.change = "Variation".into();
        french.labels.net_worth = "Patrimoine".into();
        let page = texts(&french, &w);
        assert!(["Comptes", "Solde", "Variation", "Patrimoine"].iter().all(|word| page.iter().any(|t| t == word)));
        assert!(!page.iter().any(|t| t == "Net worth"));
        // and it is in the file
        let bytes = read(&w.conn, &o).unwrap().pdf(&o).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn the_accounts_table_is_absent_with_one_account_and_with_a_limit_to_categories() {
        // nobody who has not chosen to use accounts is shown any
        let w = world();
        assert!(accounts_of(&options("2026-01", "2026-03"), &w, APRIL).is_none());
        let page = texts(&options("2026-01", "2026-03"), &w);
        for absent in ["Accounts", "Net worth", "Balance", "Change"] {
            assert!(!page.iter().any(|t| t == absent), "{absent} is in a report of someone with one account");
        }
        // an opening balance on the main account is using accounts
        accounts::update(&w.conn, MAIN_ID, &accounts::AccountPatch { opening_balance: Some("100.00".into()), opening_date: Some("2025-12-01".into()), ..Default::default() }, "2026-04-15").unwrap();
        let part = accounts_of(&options("2026-01", "2026-03"), &w, APRIL).unwrap();
        assert_eq!(part.accounts.len(), 1);
        assert_eq!((part.accounts[0].1, part.net_worth), (571_380 + 10_000 + 30_000, 571_380 + 10_000 + 30_000), "the opening 100.00 and the 300.00 that never left, as there is no other account");
        // more than one account, but a limit to categories: what moved an account is not all in it
        let w = with_accounts();
        let mut limited = options("2026-01", "2026-03");
        limited.category_ids = Some(vec![w.food.to_string()]);
        assert!(accounts_of(&limited, &w, APRIL).is_none());
        let page = texts(&limited, &w);
        assert!(!page.iter().any(|t| t == "Accounts" || t == "Net worth" || t == "Credit card"), "{page:?}");
        // every category named is still a limit
        let mut every: ReportOptions = options("2026-01", "2026-03");
        every.category_ids = Some(w.conn.prepare("SELECT id FROM categories").unwrap().query_map([], |r| Ok(r.get::<_, i64>(0)?.to_string())).unwrap().map(Result::unwrap).collect());
        assert!(accounts_of(&every, &w, APRIL).is_none());
    }

    #[test]
    fn the_net_worth_has_what_friends_owed_you_on_the_last_day() {
        let w = with_accounts();
        let anna = friend(&w.conn, "Anna");
        for (kind, amount, date) in [(Kind::Lent, "25.00", "2026-01-02"), (Kind::RepaidToMe, "10.00", "2026-01-28"), (Kind::Lent, "5.00", "2026-04-03")] {
            ledger::add(&w.conn, &NewEntry { person_id: anna.to_string(), kind, amount: amount.into(), date: date.into(), note: None, transaction_id: None, account_id: None }).unwrap();
        }
        // on the 31st of January: Anna owes 15.00, and the loan and the repayment are in the main account
        let january = accounts_of(&options("2026-01", "2026-01"), &w, APRIL).unwrap();
        let in_accounts: i64 = january.accounts.iter().map(|a| a.1).sum();
        assert_eq!(january.net_worth, in_accounts + 1_500);
        // on the 31st of March the same 15.00 (the loan of April is after it); the dashboard of today counts all 20.00
        let march = accounts_of(&options("2026-01", "2026-03"), &w, APRIL).unwrap();
        assert_eq!(march.net_worth, march.accounts.iter().map(|a| a.1).sum::<i64>() + 1_500);
        let summary = accounts::summary(&w.conn, "2026-04-15").unwrap();
        assert_eq!(summary.friends, "20.00");
        let to_april = accounts_of(&options("2026-01", "2026-04"), &w, APRIL).unwrap();
        assert_eq!(to_april.net_worth.to_string(), summary.net_worth.replace('.', ""), "to the end of April it is the dashboard's number");
    }

    #[test]
    fn a_month_with_nothing_but_accounts_shows_them_and_not_the_empty_message() {
        let w = with_accounts();
        let o = options("2026-06", "2026-06");
        let page = texts(&o, &w);
        assert!(!page.iter().any(|t| t == "No transactions in this period."), "{page:?}");
        assert!(page.iter().any(|t| t == "Accounts") && page.iter().any(|t| t == "5,663.80"), "the main account, after the 50.00 of April: {page:?}");
        let part = accounts_of(&o, &w, APRIL).unwrap();
        assert!(part.accounts.iter().all(|a| a.2 == 0), "nothing changed in June: {:?}", part.accounts);
        // an account that opens after the last day holds nothing on it, and has not changed
        open_account(&w.conn, "Later", AccountKind::Cash, "70.00", "2026-09-01");
        let part = accounts_of(&o, &w, APRIL).unwrap();
        assert_eq!(part.accounts.iter().find(|a| a.0 == "Later").map(|a| (a.1, a.2)), Some((0, 0)));
        assert_eq!(part.net_worth, part.accounts.iter().map(|a| a.1).sum::<i64>());
    }

    #[test]
    fn a_long_odd_name_is_cut_to_fit_and_a_long_table_runs_onto_more_pages() {
        let w = with_accounts();
        open_account(&w.conn, &"W".repeat(48), AccountKind::Cash, "1.00", "2026-01-01");
        open_account(&w.conn, "寿司 account", AccountKind::Cash, "2.00", "2026-01-01");
        for i in 0..70 {
            open_account(&w.conn, &format!("Pot {i}"), AccountKind::Savings, "1.00", "2026-01-01");
        }
        let o = options("2026-01", "2026-03");
        let doc = layout::build(&read_at(&w.conn, &o, at(APRIL)).unwrap().0, &o).unwrap();
        assert!(doc.pages.len() >= 2, "{} pages", doc.pages.len());
        let metrics = Metrics::new().unwrap();
        for op in doc.pages.iter().flatten() {
            if let Op::Text { x, y, size, bold, text, .. } = op {
                assert!(x + metrics.width(text, *size, *bold) <= doc.width - 44.0 + 0.5, "{text:?} runs past the margin");
                assert!(*y < doc.height - 30.0 || *y > doc.height - 40.0, "text over the footer area: {y}");
            }
        }
        let texts = doc.texts();
        assert!(texts.iter().any(|t| t.starts_with("WWW") && t.ends_with('\u{2026}')), "the name is cut with an ellipsis");
        assert!(texts.iter().any(|t| t.starts_with("?? account")), "what the font lacks is a question mark, as everywhere else");
        assert_eq!(texts.iter().filter(|t| **t == "Net worth").count(), 1);
        let last = doc.pages.last().unwrap();
        assert!(last.iter().any(|op| matches!(op, Op::Text { text, .. } if text == "Net worth")), "the net worth ends the table");
        assert!(read(&w.conn, &o).unwrap().pdf(&o).unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn the_words_of_the_table_default_to_english_and_are_checked_like_the_others() {
        // a caller from before accounts (the command line builds its own labels) leaves them out
        let mut sent = serde_json::json!({
            "title": "t", "period": "p", "income": "i", "expenses": "e", "savings": "s", "savingsRate": "sr", "byMonth": "b", "spending": "w", "other": "o",
            "budgetVsActual": "v", "noBudget": "n", "transactions": "x", "date": "d", "category": "c", "name": "m", "amount": "a", "page": "g", "noData": "z",
        });
        let old: ReportLabels = serde_json::from_value(sent.clone()).unwrap();
        assert_eq!((old.accounts.as_str(), old.balance.as_str(), old.change.as_str(), old.net_worth.as_str()), ("Accounts", "Balance", "Change", "Net worth"));
        let english = ReportLabels::english("2026-01", "2026-01").unwrap();
        assert_eq!((english.accounts, english.balance, english.change, english.net_worth), (old.accounts, old.balance, old.change, old.net_worth));
        // sent, they are used; an unknown one is still refused
        sent["accounts"] = "Comptes".into();
        sent["netWorth"] = "Patrimoine".into();
        let french: ReportLabels = serde_json::from_value(sent.clone()).unwrap();
        assert_eq!((french.accounts.as_str(), french.balance.as_str(), french.net_worth.as_str()), ("Comptes", "Balance", "Patrimoine"));
        sent["colour"] = "red".into();
        assert!(serde_json::from_value::<ReportLabels>(sent).is_err());
        // and they are held to the limits of the rest
        let w = world();
        for set in [
            (|l: &mut ReportLabels| l.accounts = "x".repeat(LABEL_MAX_CHARS + 1)) as fn(&mut ReportLabels),
            |l| l.balance = "a\nb".into(),
            |l| l.change = "x".repeat(LABEL_MAX_CHARS + 1),
            |l| l.net_worth = "\u{7}".into(),
        ] {
            let mut o = options("2026-01", "2026-01");
            set(&mut o.labels);
            assert_eq!(read(&w.conn, &o).err(), Some(AppError::InvalidInput));
        }
    }

    #[test]
    fn reading_the_report_with_accounts_changes_nothing() {
        let w = with_accounts();
        let before = crate::accounts::fixtures::snapshot(&w.conn);
        let o = options("2026-01", "2026-03");
        read_at(&w.conn, &o, at(APRIL)).unwrap().pdf(&o).unwrap();
        assert_eq!(crate::accounts::fixtures::snapshot(&w.conn), before);
    }

    // ---- an account that net worth leaves out (docs/accounts-design.md, section 20.3) ----------------------------------

    const NOT_COUNTED: &str = "(not in net worth)";

    /// Leaves these accounts out of net worth, by SQL so that the archived one can be among them.
    fn leave_out(conn: &Connection, names: &[&str]) {
        for name in names {
            assert_eq!(conn.execute("UPDATE accounts SET include_net_worth = 0 WHERE name = ?1", [name]).unwrap(), 1, "{name}");
        }
    }

    /// Anna owes 15.00 on the 31st of January and 20.00 from the 3rd of April (as in `the_net_worth_has_what_friends_owed_you_on_the_last_day`).
    fn lend_to_anna(w: &World) {
        let anna = friend(&w.conn, "Anna");
        for (kind, amount, date) in [(Kind::Lent, "25.00", "2026-01-02"), (Kind::RepaidToMe, "10.00", "2026-01-28"), (Kind::Lent, "5.00", "2026-04-03")] {
            ledger::add(&w.conn, &NewEntry { person_id: anna.to_string(), kind, amount: amount.into(), date: date.into(), note: None, transaction_id: None, account_id: None }).unwrap();
        }
    }

    /// The text of the page and where each piece is: (x, y, text), in drawing order.
    fn placed(o: &ReportOptions, w: &World) -> Vec<(f32, f32, String)> {
        let doc = layout::build(&read(&w.conn, o).unwrap().0, o).unwrap();
        doc.pages.iter().flatten().filter_map(|op| if let Op::Text { x, y, text, .. } = op { Some((*x, *y, text.clone())) } else { None }).collect()
    }

    #[test]
    fn an_account_left_out_of_net_worth_is_listed_with_its_balance_marked_and_not_in_the_net_worth_line() {
        let w = with_accounts();
        leave_out(&w.conn, &["Savings"]);
        let o = options("2026-01", "2026-03");
        let part = accounts_of(&o, &w, APRIL).unwrap();
        assert_eq!(
            part.accounts.iter().map(|a| (a.0.as_str(), a.1, a.3)).collect::<Vec<_>>(),
            [("Main account", 571_380, true), ("Savings", 125_000, false), ("Credit card", -14_000, true), ("Old", 0, true)],
            "every account is listed, with its balance, and says whether net worth counts it"
        );
        assert_eq!(part.net_worth, 571_380 - 14_000, "the 1,250.00 of Savings is not in it");
        let page = texts(&o, &w);
        assert_eq!(page.iter().filter(|t| *t == NOT_COUNTED).count(), 1, "one account is marked: {page:?}");
        let place = |needle: &str| page.iter().position(|t| t == needle).unwrap_or_else(|| panic!("{needle:?} is not on the page: {page:?}"));
        let in_table = |needle: &str| place("Accounts") + page[place("Accounts")..].iter().position(|t| t == needle).unwrap();
        assert!(in_table("Savings") < place(NOT_COUNTED) && place(NOT_COUNTED) < in_table("Credit card"), "the mark is on the row of the account that is left out");
        assert!(page.iter().any(|t| t == "1,250.00"), "its balance is listed all the same: {page:?}");
        assert!(page.iter().any(|t| t == "5,573.80"), "the net worth leaves it out: {page:?}");
        assert!(!page.iter().any(|t| t == "6,823.80"), "and is not the number with it");
        // the mark is drawn after the name, on its row
        let ops = placed(&o, &w);
        let savings = ops.iter().rev().find(|t| t.2 == "Savings").unwrap();
        let mark = ops.iter().find(|t| t.2 == NOT_COUNTED).unwrap();
        assert!(mark.0 > savings.0 && (mark.1 - savings.1).abs() < 0.01, "{savings:?} {mark:?}");
        // and the same thing is what the dashboard counts
        let to_april = accounts_of(&options("2026-01", "2026-04"), &w, APRIL).unwrap();
        assert_eq!(to_april.net_worth, crate::money::parse_signed_amount(&accounts::summary(&w.conn, "2026-04-15").unwrap().net_worth).unwrap());
    }

    #[test]
    fn with_every_account_left_out_all_are_listed_marked_and_the_net_worth_is_what_friends_owed() {
        let w = with_accounts();
        lend_to_anna(&w);
        leave_out(&w.conn, &["Main account", "Savings", "Credit card", "Old"]);
        let january = options("2026-01", "2026-01");
        let part = accounts_of(&january, &w, APRIL).unwrap();
        assert_eq!(part.accounts.len(), 4);
        assert!(part.accounts.iter().all(|a| !a.3));
        assert_eq!(part.net_worth, 1_500, "nothing of the accounts, and the 15.00 that Anna owed on the 31st");
        let page = texts(&january, &w);
        assert_eq!(page.iter().filter(|t| *t == NOT_COUNTED).count(), 4, "{page:?}");
        let net_worth = page.iter().position(|t| t == "Net worth").unwrap();
        assert_eq!(page[net_worth + 1], "15.00", "the line of the net worth is the friends' part alone");
        // to the end of April it is the number of the dashboard, which also counts nothing of the accounts
        let summary = accounts::summary(&w.conn, "2026-04-15").unwrap();
        assert_eq!((summary.in_accounts.as_str(), summary.accounts, summary.in_use), ("0.00", 4, true), "every account is still counted as an account, and accounts are in use");
        let to_april = accounts_of(&options("2026-01", "2026-04"), &w, APRIL).unwrap();
        assert_eq!(to_april.net_worth, 2_000);
        assert_eq!(to_april.net_worth, crate::money::parse_signed_amount(&summary.net_worth).unwrap());
        // with no friend either, it is nothing
        let w = with_accounts();
        leave_out(&w.conn, &["Main account", "Savings", "Credit card", "Old"]);
        assert_eq!(accounts_of(&january, &w, APRIL).unwrap().net_worth, 0);
        let page = texts(&january, &w);
        let net_worth = page.iter().position(|t| t == "Net worth").unwrap();
        assert_eq!(page[net_worth + 1], "0.00");
    }

    #[test]
    fn with_no_account_left_out_the_page_has_no_mark_and_the_words_of_it_change_nothing() {
        let w = with_accounts();
        let o = options("2026-01", "2026-03");
        let part = accounts_of(&o, &w, APRIL).unwrap();
        assert!(part.accounts.iter().all(|a| a.3), "the archived account counts too");
        assert_eq!(part.net_worth, part.accounts.iter().map(|a| a.1).sum::<i64>());
        let page = texts(&o, &w);
        assert!(!page.iter().any(|t| t == NOT_COUNTED));
        // a different mark is never drawn, and the page is the same down to the last piece
        let mut other = options("2026-01", "2026-03");
        other.labels.not_in_net_worth = "NOT COUNTED".into();
        assert!(!texts(&other, &w).iter().any(|t| t == "NOT COUNTED"));
        let (a, b) = (layout::build(&read(&w.conn, &o).unwrap().0, &o).unwrap(), layout::build(&read(&w.conn, &other).unwrap().0, &other).unwrap());
        assert_eq!(format!("{:?}", a.pages), format!("{:?}", b.pages));
    }

    #[test]
    fn the_net_worth_line_is_the_accounts_that_count_plus_friends_whichever_accounts_are_left_out() {
        let names = ["Main account", "Savings", "Credit card", "Old"];
        for subset in 0..16u32 {
            let w = with_accounts();
            lend_to_anna(&w);
            let out: Vec<&str> = names.iter().enumerate().filter(|(i, _)| subset >> i & 1 == 1).map(|(_, n)| *n).collect();
            leave_out(&w.conn, &out);
            // to the end of April: the dashboard's day, so the report and `summary` must say the same
            let part = accounts_of(&options("2026-01", "2026-04"), &w, APRIL).unwrap();
            let counted: i64 = part.accounts.iter().filter(|a| a.3).map(|a| a.1).sum();
            assert_eq!(part.net_worth, counted + 2_000, "without {out:?}");
            assert_eq!(part.accounts.iter().filter(|a| !a.3).map(|a| a.0.as_str()).collect::<Vec<_>>(), out, "the ones that are marked");
            let summary = accounts::summary(&w.conn, "2026-04-15").unwrap();
            assert_eq!(part.net_worth, crate::money::parse_signed_amount(&summary.net_worth).unwrap(), "without {out:?}");
            assert_eq!(counted, crate::money::parse_signed_amount(&summary.in_accounts).unwrap(), "without {out:?}");
            // the page says the same number, and the marks are on the accounts that are left out
            let o = options("2026-01", "2026-04");
            let page = texts(&o, &w);
            assert_eq!(page.iter().filter(|t| *t == NOT_COUNTED).count(), out.len(), "without {out:?}");
            let net_worth = page.iter().position(|t| t == "Net worth").unwrap();
            assert_eq!(page[net_worth + 1], layout_money(part.net_worth), "without {out:?}");
        }
    }

    /// An amount as the page writes it in the default format.
    fn layout_money(minor: i64) -> String {
        let (sign, size) = (if minor < 0 { "-" } else { "" }, minor.abs());
        let whole = (size / 100).to_string();
        let grouped: Vec<String> = whole.as_bytes().rchunks(3).rev().map(|c| String::from_utf8_lossy(c).to_string()).collect();
        format!("{sign}{}.{:02}", grouped.join(","), size % 100)
    }

    #[test]
    fn a_long_name_is_cut_before_the_mark_and_a_very_long_mark_is_cut_too() {
        let w = with_accounts();
        open_account(&w.conn, &"W".repeat(48), AccountKind::Cash, "1.00", "2026-01-01");
        open_account(&w.conn, "寿司 account", AccountKind::Cash, "2.00", "2026-01-01");
        leave_out(&w.conn, &[&"W".repeat(48), "寿司 account", "Savings"]);
        let long_mark = "not counted in the net worth because ".repeat(3);
        for mark in [NOT_COUNTED.to_string(), long_mark.chars().take(LABEL_MAX_CHARS).collect::<String>()] {
            let mut o = options("2026-01", "2026-03");
            o.labels.not_in_net_worth = mark.clone();
            let doc = layout::build(&read(&w.conn, &o).unwrap().0, &o).unwrap();
            let metrics = Metrics::new().unwrap();
            for op in doc.pages.iter().flatten() {
                if let Op::Text { x, size, bold, text, .. } = op {
                    assert!(x + metrics.width(text, *size, *bold) <= doc.width - 44.0 + 0.5, "{text:?} runs past the margin with the mark {mark:?}");
                }
            }
            let texts = doc.texts();
            assert!(texts.iter().any(|t| t.starts_with("WWW") && t.ends_with('\u{2026}')), "the name is cut with an ellipsis: {texts:?}");
            if mark == NOT_COUNTED {
                assert_eq!(texts.iter().filter(|t| **t == NOT_COUNTED).count(), 3, "the mark is whole, on each of the three rows");
            } else {
                assert_eq!(texts.iter().filter(|t| t.starts_with("not counted")).count(), 3, "a long mark is cut, not left out");
            }
            // the marked names and the balances do not touch: the change column starts after the name and its mark
            let ops = placed(&o, &w);
            let row_of = |name: &str| ops.iter().rev().find(|t| t.2.starts_with(name)).unwrap().clone();
            let wide = row_of("WWW");
            let mark_at = ops.iter().find(|t| (t.1 - wide.1).abs() < 0.01 && t.0 > wide.0 && t.2.starts_with(if mark == NOT_COUNTED { "(not" } else { "not counted" })).unwrap();
            let change_at = ops.iter().find(|t| (t.1 - wide.1).abs() < 0.01 && t.2.starts_with(['+', '-']) && t.0 > mark_at.0).map(|t| t.0);
            let end_of_mark = mark_at.0 + metrics.width(&mark_at.2, 8.0, false);
            assert!(change_at.is_none_or(|x| end_of_mark <= x + 0.5), "the mark runs into the change: {mark_at:?} ends at {end_of_mark}, the change starts at {change_at:?}");
        }
        assert!(read(&w.conn, &options("2026-01", "2026-03")).unwrap().pdf(&options("2026-01", "2026-03")).unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn the_mark_defaults_to_english_is_used_when_sent_and_is_checked_like_the_others() {
        // a caller from before the details (the app does not send it yet, and the command line builds its own labels) leaves it out
        let mut sent = serde_json::json!({
            "title": "t", "period": "p", "income": "i", "expenses": "e", "savings": "s", "savingsRate": "sr", "byMonth": "b", "spending": "w", "other": "o",
            "budgetVsActual": "v", "noBudget": "n", "transactions": "x", "date": "d", "category": "c", "name": "m", "amount": "a", "page": "g", "noData": "z",
        });
        let old: ReportLabels = serde_json::from_value(sent.clone()).unwrap();
        assert_eq!(old.not_in_net_worth, NOT_COUNTED);
        let four_of_round_one: ReportLabels = {
            let mut with_four = sent.clone();
            for (key, word) in [("accounts", "A"), ("balance", "B"), ("change", "C"), ("netWorth", "N")] {
                with_four[key] = word.into();
            }
            serde_json::from_value(with_four).unwrap()
        };
        assert_eq!(four_of_round_one.not_in_net_worth, NOT_COUNTED, "a caller that sends the four of round one gets the English mark");
        let english = ReportLabels::english("2026-01", "2026-01").unwrap();
        assert_eq!(english.not_in_net_worth, old.not_in_net_worth, "the command line writes it as the default does");
        // sent, it is used, and an unknown label is still refused
        sent["notInNetWorth"] = "hors patrimoine".into();
        let french: ReportLabels = serde_json::from_value(sent.clone()).unwrap();
        assert_eq!(french.not_in_net_worth, "hors patrimoine");
        sent["colour"] = "red".into();
        assert!(serde_json::from_value::<ReportLabels>(sent).is_err());
        // it is held to the limits of the rest: a report with a bad one is refused and reads nothing
        let w = with_accounts();
        for bad in ["x".repeat(LABEL_MAX_CHARS + 1), "a\nb".to_string(), "\u{7}".to_string()] {
            let mut o = options("2026-01", "2026-03");
            o.labels.not_in_net_worth = bad;
            let before = crate::accounts::fixtures::snapshot(&w.conn);
            assert_eq!(read(&w.conn, &o).err(), Some(AppError::InvalidInput));
            assert_eq!(crate::accounts::fixtures::snapshot(&w.conn), before);
        }
        let mut longest = options("2026-01", "2026-03");
        longest.labels.not_in_net_worth = "x".repeat(LABEL_MAX_CHARS);
        assert!(read(&w.conn, &longest).is_ok(), "the limit itself is allowed");
    }

    #[test]
    fn a_virtual_balance_is_nowhere_in_the_report() {
        let w = with_accounts();
        let o = options("2026-01", "2026-03");
        let before = texts(&o, &w);
        let card: i64 = w.conn.query_row("SELECT id FROM accounts WHERE name = 'Credit card'", [], |r| r.get(0)).unwrap();
        accounts::update(&w.conn, card, &accounts::AccountPatch { virtual_balance: Some("1000.00".into()), bic: Some(Some("DEUTDEFF".into())), notes: Some(Some("PRIVATE-NOTE".into())), group: Some(Some("Cards".into())), ..Default::default() }, "2026-04-15").unwrap();
        let after = texts(&o, &w);
        assert_eq!(after, before, "the page is the same, down to the net worth: no virtual balance, BIC, note or group");
        let part = accounts_of(&o, &w, APRIL).unwrap();
        assert_eq!(part.accounts[2], ("Credit card".to_string(), -14_000, 10_000, true, None));
        assert!(!after.iter().any(|t| t.contains("PRIVATE-NOTE") || t.contains("DEUTDEFF") || t.contains("1,000.00") || t == "Cards"));
    }

    // ---- accounts in a currency of their own (docs/accounts-design.md, section 21.6) ------------------------------------

    /// An account in another currency, at `rate` of its units to the euro, opened on the first of December 2025.
    fn open_foreign(conn: &Connection, name: &str, code: &str, rate: &str, opening: &str) -> i64 {
        accounts::create(
            conn,
            &NewAccount { opening_balance: Some(opening.into()), opening_date: Some("2025-12-01".into()), currency: Some(code.into()), rate: Some(rate.into()), ..new_account(name) },
            "2026-04-15",
        )
        .unwrap()
        .id
    }

    /// A transfer between two currencies: `amount` leaves the first in its currency and `arrives` reaches the second in its own.
    fn move_between(conn: &Connection, from: i64, to: i64, amount: &str, arrives: &str, date: &str) {
        let new = crate::accounts::transfers::NewTransfer { to_amount: Some(arrives.into()), ..crate::accounts::fixtures::new_transfer(from, to, amount, date) };
        crate::accounts::transfers::create(conn, &new).unwrap();
    }

    /// `with_accounts()` and an account in lei at 5.00 to the euro: 1,000.00 lei on the first of December, 100.00 lei that came from the main
    /// account (20.00 euro) on the 10th of February, and 50.00 lei that went to Savings (10.00 euro) on the 20th of March. By the end of March it
    /// holds 1,050.00 lei, which is 210.00 euro, and it changed by +50.00 lei over the months; the main account holds 5,693.80, Savings 1,260.00.
    fn with_lei() -> (World, i64) {
        let w = with_accounts();
        let lei = open_foreign(&w.conn, "Lei account", "RON", "5.00", "1000.00");
        let id = |name: &str| -> i64 { w.conn.query_row("SELECT id FROM accounts WHERE name = ?1", [name], |r| r.get(0)).unwrap() };
        move_between(&w.conn, MAIN_ID, lei, "20.00", "100.00", "2026-02-10");
        move_between(&w.conn, lei, id("Savings"), "50.00", "10.00", "2026-03-20");
        (w, lei)
    }

    /// The net worth of the dashboard on the last day of the months of the report.
    fn dashboard_net_worth(w: &World, day: &str) -> i64 {
        // (a rate of 0.00000001 shows the bound a balance passed, so the sum may be larger than an amount a screen takes)
        accounts::summary(&w.conn, day).unwrap().net_worth.replace('.', "").parse().unwrap()
    }

    #[test]
    fn a_foreign_account_is_listed_in_its_own_currency_with_its_code_and_is_turned_into_the_apps_in_the_net_worth() {
        let (w, _) = with_lei();
        let o = options("2026-01", "2026-03");
        let part = accounts_of(&o, &w, APRIL).unwrap();
        assert_eq!(
            part.accounts.iter().map(|a| (a.0.as_str(), a.1, a.2, a.4.as_deref())).collect::<Vec<_>>(),
            [("Main account", 569_380, 569_380, None), ("Savings", 126_000, 26_000, None), ("Credit card", -14_000, 10_000, None), ("Lei account", 105_000, 5_000, Some("RON")), ("Old", 0, -5_000, None)],
            "the balance and the change are in lei, the archived account is last"
        );
        // 5,693.80 + 1,260.00 - 140.00 + 0.00 and the lei account at its rate: 1,050.00 / 5.00 = 210.00
        assert_eq!(part.net_worth, 569_380 + 126_000 - 14_000 + 21_000);
        assert_ne!(part.net_worth, part.accounts.iter().map(|a| a.1).sum::<i64>(), "the lei are not added as if they were euro");
        assert_eq!(part.net_worth, dashboard_net_worth(&w, "2026-03-31"), "the dashboard's number for the last day");
        let page = texts(&o, &w);
        assert!(page.iter().any(|t| t == "RON 1,050.00"), "the balance, with the code in front and the digits as the report writes them: {page:?}");
        assert!(page.iter().any(|t| t == "RON +50.00"), "and the change");
        assert!(!page.iter().any(|t| t == "1,050.00" || t == "+50.00"), "never as if it were the app's currency");
        let net_worth = page.iter().position(|t| t == "Net worth").unwrap();
        assert_eq!(page[net_worth + 1], "7,023.80");
        // the accounts in the app's currency are written as before, with no code
        for shown in ["5,693.80", "1,260.00", "-140.00"] {
            assert!(page.iter().any(|t| t == shown), "{shown}: {page:?}");
        }
        assert_eq!(page.iter().filter(|t| t.starts_with("RON ")).count(), 2, "only the balance and the change of the one account in lei: {page:?}");
        // the code is drawn on its row, in the columns of the numbers
        let ops = placed(&o, &w);
        let row_of = |name: &str| ops.iter().rev().find(|t| t.2 == name).unwrap().clone();
        let (name, balance, change) = (row_of("Lei account"), ops.iter().find(|t| t.2 == "RON 1,050.00").unwrap(), ops.iter().find(|t| t.2 == "RON +50.00").unwrap());
        assert!((name.1 - balance.1).abs() < 0.01 && (name.1 - change.1).abs() < 0.01, "{name:?} {balance:?} {change:?}");
        // a report of one month is in lei too: 100.00 came in on the 10th of February
        let february = accounts_of(&options("2026-02", "2026-02"), &w, APRIL).unwrap();
        assert_eq!(february.accounts.iter().find(|a| a.0 == "Lei account").map(|a| (a.1, a.2)), Some((110_000, 10_000)));
        assert_eq!(february.net_worth, dashboard_net_worth(&w, "2026-02-28"));
    }

    #[test]
    fn a_foreign_account_has_the_code_where_the_person_puts_the_symbol_and_the_digits_in_the_persons_format() {
        let (w, _) = with_lei();
        let mut o = options("2026-01", "2026-03");
        o.format = DisplayFormat { currency_symbol: "€".into(), symbol_before: false, symbol_space: false, decimal_mark: ",".into(), group_separator: ".".into(), ..Default::default() };
        let page = texts(&o, &w);
        assert!(page.iter().any(|t| t == "RON 1.050,00") && page.iter().any(|t| t == "RON +50,00"), "{page:?}");
        assert!(page.iter().any(|t| t == "5.693,80€"), "the others have the symbol of the app's currency: {page:?}");
        assert!(!page.iter().any(|t| t.starts_with("RON") && t.contains('€')), "no symbol of the app's on an amount in lei");
        o.format = DisplayFormat { currency_symbol: "$".into(), symbol_before: true, symbol_space: true, ..Default::default() };
        let page = texts(&o, &w);
        assert!(page.iter().any(|t| t == "RON 1,050.00"), "{page:?}");
        let net_worth = page.iter().position(|t| t == "Net worth").unwrap();
        assert_eq!(page[net_worth + 1], "$\u{a0}7,023.80", "the net worth is in the app's currency, with its symbol");
    }

    #[test]
    fn a_foreign_account_left_out_of_net_worth_is_listed_in_its_currency_marked_and_not_in_the_net_worth() {
        let (w, _) = with_lei();
        w.conn.execute("UPDATE accounts SET include_net_worth = 0 WHERE name = 'Lei account'", []).unwrap();
        let o = options("2026-01", "2026-03");
        let part = accounts_of(&o, &w, APRIL).unwrap();
        assert_eq!(part.net_worth, 569_380 + 126_000 - 14_000, "the 210.00 of the lei account are not in it");
        assert_eq!(part.net_worth, dashboard_net_worth(&w, "2026-03-31"));
        let page = texts(&o, &w);
        assert_eq!(page.iter().filter(|t| *t == NOT_COUNTED).count(), 1, "{page:?}");
        assert!(page.iter().any(|t| t == "RON 1,050.00"), "it is listed all the same");
        let net_worth = page.iter().position(|t| t == "Net worth").unwrap();
        assert_eq!(page[net_worth + 1], "6,813.80");
        let place = |needle: &str| page.iter().position(|t| t == needle).unwrap();
        assert!(place("Lei account") < place(NOT_COUNTED) && place(NOT_COUNTED) < place("RON +50.00"), "the mark is on its row: {page:?}");
    }

    #[test]
    fn a_negative_foreign_balance_is_written_with_its_sign_and_turned_into_the_apps_currency_half_away_from_zero() {
        let w = with_accounts();
        open_foreign(&w.conn, "Lei card", "RON", "3.00", "-200.00");
        let o = options("2026-01", "2026-03");
        let part = accounts_of(&o, &w, APRIL).unwrap();
        let card = part.accounts.iter().find(|a| a.0 == "Lei card").unwrap();
        assert_eq!((card.1, card.2, card.4.as_deref()), (-20_000, 0, Some("RON")));
        // -200.00 lei at 3.00 is -66.666... euro, which is -66.67
        assert_eq!(part.net_worth, 571_380 + 125_000 - 14_000 - 6_667);
        assert_eq!(part.net_worth, dashboard_net_worth(&w, "2026-03-31"));
        let page = texts(&o, &w);
        assert!(page.iter().any(|t| t == "RON -200.00") && page.iter().any(|t| t == "RON +0.00"), "{page:?}");
        let net_worth = page.iter().position(|t| t == "Net worth").unwrap();
        assert_eq!(page[net_worth + 1], "6,757.13");
        // and in red, as a negative balance of the app's currency is
        let ops = layout::build(&read(&w.conn, &o).unwrap().0, &o).unwrap();
        let colour_of = |text: &str| ops.pages.iter().flatten().find_map(|op| if let Op::Text { text: t, color, .. } = op { (t == text).then_some(*color) } else { None }).unwrap();
        assert_eq!(colour_of("RON -200.00"), colour_of("-140.00"));
        assert_ne!(colour_of("RON -200.00"), colour_of("5,713.80"));
    }

    #[test]
    fn two_foreign_currencies_are_each_listed_in_their_own_and_added_up_only_in_the_apps() {
        let (w, _) = with_lei();
        open_foreign(&w.conn, "Dollars", "USD", "1.10", "220.00");
        let o = options("2026-01", "2026-03");
        let part = accounts_of(&o, &w, APRIL).unwrap();
        assert_eq!(part.accounts.iter().filter_map(|a| a.4.as_deref()).collect::<Vec<_>>(), ["RON", "USD"], "in the person's order, the archived one last");
        // 220.00 dollars at 1.10 are 200.00 euro
        assert_eq!(part.net_worth, 569_380 + 126_000 - 14_000 + 21_000 + 20_000);
        assert_eq!(part.net_worth, dashboard_net_worth(&w, "2026-03-31"));
        let page = texts(&o, &w);
        assert!(page.iter().any(|t| t == "RON 1,050.00") && page.iter().any(|t| t == "USD 220.00") && page.iter().any(|t| t == "USD +0.00"), "{page:?}");
        let net_worth = page.iter().position(|t| t == "Net worth").unwrap();
        assert_eq!(page[net_worth + 1], "7,223.80");
        // there is no row that adds the balances up: the only total is the net worth
        assert_eq!(page.iter().filter(|t| *t == "Net worth").count(), 1);
    }

    #[test]
    fn the_net_worth_of_the_report_is_the_dashboards_for_any_rate_inclusion_and_archiving_of_a_foreign_account() {
        for rate in ["5.00", "0.5", "160", "1", "0.00000001", "999999999.99999999", "3.33333333"] {
            for (counts, archived) in [(true, false), (false, false), (true, true), (false, true)] {
                let w = with_accounts();
                let lei = open_foreign(&w.conn, "Lei account", "RON", rate, "1234.56");
                move_between(&w.conn, MAIN_ID, lei, "20.00", "99.99", "2026-02-10");
                w.conn.execute("UPDATE accounts SET include_net_worth = ?2, archived = ?3 WHERE id = ?1", rusqlite::params![lei, counts, archived]).unwrap();
                for (from, to, last) in [("2026-01", "2026-01", "2026-01-31"), ("2026-01", "2026-03", "2026-03-31"), ("2026-03", "2026-03", "2026-03-31")] {
                    let part = accounts_of(&options(from, to), &w, APRIL).unwrap();
                    assert_eq!(part.net_worth, dashboard_net_worth(&w, last), "rate {rate}, counts {counts}, archived {archived}, {from} to {to}");
                    let page = texts(&options(from, to), &w);
                    let net_worth = page.iter().position(|t| t == "Net worth").unwrap();
                    assert_eq!(page[net_worth + 1], layout_money(part.net_worth), "rate {rate}");
                }
            }
        }
    }

    #[test]
    fn with_no_foreign_account_the_page_has_no_code_and_is_what_it_was() {
        let w = with_accounts();
        let o = options("2026-01", "2026-03");
        let part = accounts_of(&o, &w, APRIL).unwrap();
        assert!(part.accounts.iter().all(|a| a.4.is_none()));
        let page = texts(&o, &w);
        for shown in ["5,713.80", "1,250.00", "-140.00", "0.00", "+5,713.80", "+250.00", "+100.00", "-50.00"] {
            assert!(page.iter().any(|t| t == shown), "{shown}: {page:?}");
        }
        assert!(!page.iter().any(|t| t.len() > 4 && t.chars().take(3).all(|c| c.is_ascii_uppercase()) && t.chars().nth(3) == Some(' ')), "no text starts with a code: {page:?}");
        // an account that was foreign and is deleted leaves the page as it was
        let (with, lei) = with_lei();
        let before = texts(&o, &with);
        assert!(before.iter().any(|t| t.starts_with("RON ")));
        with.conn.execute("DELETE FROM transfers", []).unwrap();
        with.conn.execute("DELETE FROM accounts WHERE id = ?1", [lei]).unwrap();
        let after = texts(&o, &with);
        assert!(!after.iter().any(|t| t.starts_with("RON ")));
        assert_eq!(accounts_of(&o, &with, APRIL).unwrap().net_worth, 571_380 + 125_000 - 14_000, "a world with transfers gone: the three accounts as they are");
    }

    #[test]
    fn a_long_name_next_to_a_code_and_a_big_foreign_number_stays_inside_the_margins() {
        let (w, lei) = with_lei();
        w.conn.execute("UPDATE accounts SET name = ?2, opening_balance_minor = 99999999999 WHERE id = ?1", rusqlite::params![lei, "W".repeat(48)]).unwrap();
        w.conn.execute("UPDATE accounts SET include_net_worth = 0 WHERE id = ?1", [lei]).unwrap();
        let o = options("2026-01", "2026-03");
        let doc = layout::build(&read(&w.conn, &o).unwrap().0, &o).unwrap();
        let metrics = Metrics::new().unwrap();
        for op in doc.pages.iter().flatten() {
            if let Op::Text { x, size, bold, text, .. } = op {
                assert!(x + metrics.width(text, *size, *bold) <= doc.width - 44.0 + 0.5, "{text:?} runs past the margin");
            }
        }
        let texts = doc.texts();
        assert!(texts.iter().any(|t| t.starts_with("RON ") && t.contains("1,000,000,049.99")), "{texts:?}");
        assert!(read(&w.conn, &o).unwrap().pdf(&o).unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn reading_a_report_with_foreign_accounts_changes_nothing() {
        let (w, _) = with_lei();
        let before = crate::accounts::fixtures::snapshot(&w.conn);
        let o = options("2026-01", "2026-03");
        read_at(&w.conn, &o, at(APRIL)).unwrap().pdf(&o).unwrap();
        assert_eq!(crate::accounts::fixtures::snapshot(&w.conn), before);
    }
}
