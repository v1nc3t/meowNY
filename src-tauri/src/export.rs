//! Plain-file exports: a JSON file with the data and a CSV of transactions. These files are not
//! encrypted (the encrypted copy is the backup); the guides say so.
//!
//! A `Selection` narrows what goes in: a date range for transactions, and a set of categories for
//! everything that belongs to a category. With categories chosen, other category names, global
//! budgets and the change history are left out, so a limited export reveals nothing about the rest.
//!
//! Documents are listed by name, date, type, size, hash and what they are attached to, and a file is never in an export (the
//! encrypted backup is where the files go). A limited export lists only the documents attached to something that is in it, and for
//! each only the attachments that are in it, so the name of a document attached to something left out is not revealed.
//! Debts are listed too: the friends, the ledger of money lent and borrowed, and the parts of every shared bill; each transaction has what was
//! paid (`amount`) and what it costs you (`my_share`). A limited export keeps to what it contains (see `debts.rs`).
//! The device-specific scheduled-backup setup, which holds a passphrase, and the access tokens of the
//! command-line client are never exported.

pub(crate) mod debts;
#[cfg(desktop)]
mod xlsx;

use std::borrow::Cow;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::Connection;
#[cfg(test)]
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::categories::parse_id;
use crate::money::format_amount;
use crate::recurring;
use crate::transactions::clean_date;
use crate::vault::AppError;

#[cfg(desktop)]
pub use xlsx::workbook;

/// The phone has no Excel export (see `Cargo.toml`): the writer is not part of its build, so asking for one is refused.
#[cfg(mobile)]
pub fn workbook(_: &Connection, _: &Selection, _: &crate::display::DisplayFormat) -> Result<Vec<u8>, AppError> {
    Err(AppError::InvalidInput)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    /// Inclusive, "YYYY-MM-DD". Narrows transactions.
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// Null means every category. An empty list means none.
    #[serde(default)]
    pub category_ids: Option<Vec<String>>,
    /// With a limit (dates or categories) the ledger of money lent and borrowed is left out, unless this is set. Without a limit it is all in.
    #[serde(default)]
    pub include_debts: bool,
    /// Only income or only expenses. Null means both.
    #[serde(default)]
    pub kind: Option<Kind>,
    /// What a transaction cost you (the amount minus the parts of the others) is at least this and at most that, as decimal strings
    /// ("12.50"). Null means no bound.
    #[serde(default)]
    pub min_amount: Option<String>,
    #[serde(default)]
    pub max_amount: Option<String>,
    /// Only transactions whose name or description has this text in it, whatever the case (as the search of the transactions page).
    #[serde(default)]
    pub text: Option<String>,
    /// Only the bills that are shared with friends, or only those that are not. Null means both.
    #[serde(default)]
    pub split: Option<SplitFilter>,
    /// What goes into a JSON or an Excel file. Null means every part. The change history only ever goes into a file with no limit.
    #[serde(default)]
    pub sections: Option<Vec<Section>>,
}

/// Income or expense, to choose one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Kind {
    Income,
    Expense,
}

impl Kind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Kind::Income => "INCOME",
            Kind::Expense => "EXPENSE",
        }
    }
}

/// Whether a transaction is a bill shared with friends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitFilter {
    Split,
    NotSplit,
}

/// The parts of a JSON or Excel file that can be left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Section {
    Transactions,
    Categories,
    Budgets,
    Recurring,
    Documents,
    /// The friends and what each owes or is owed.
    People,
    /// The recurring payments that have come due and are not paid or skipped: what you owe by now.
    Due,
    /// The history of debts: the ledger of money lent, borrowed and paid back, the parts of the shared bills, and the recurring payments that
    /// were paid or are due.
    Ledger,
    History,
}

impl Section {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Section::Transactions => "transactions",
            Section::Categories => "categories",
            Section::Budgets => "budgets",
            Section::Recurring => "recurring",
            Section::Documents => "documents",
            Section::People => "people",
            Section::Due => "due",
            Section::Ledger => "ledger",
            Section::History => "history",
        }
    }
}

/// The longest text a transaction search may be, in characters.
pub const TEXT_MAX_CHARS: usize = 100;

#[derive(Default)]
pub(crate) struct Parsed {
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) categories: Option<Vec<i64>>,
    pub(crate) debts: bool,
    pub(crate) kind: Option<Kind>,
    /// In minor units: what a transaction cost you.
    pub(crate) min: Option<i64>,
    pub(crate) max: Option<i64>,
    /// The text asked for, and the `LIKE` pattern made of it (`%text%`, with the wild characters of the text escaped).
    pub(crate) text: Option<String>,
    pub(crate) pattern: Option<String>,
    pub(crate) split: Option<SplitFilter>,
    pub(crate) sections: Option<Vec<Section>>,
}

impl Parsed {
    /// No limit of any kind: not dates, not categories, not a kind, an amount or a text.
    pub(crate) fn everything(&self) -> bool {
        self.from.is_none()
            && self.to.is_none()
            && self.categories.is_none()
            && self.kind.is_none()
            && self.min.is_none()
            && self.max.is_none()
            && self.pattern.is_none()
            && self.split.is_none()
    }

    /// Whether anything but the dates and the parts leaves some transactions out. A running balance that counts them would add up what the
    /// file leaves out, so it is not shown then.
    pub(crate) fn narrows_transactions(&self) -> bool {
        self.categories.is_some() || self.kind.is_some() || self.min.is_some() || self.max.is_some() || self.pattern.is_some() || self.split.is_some()
    }

    /// Whether a part of the file is asked for. (The history is also left out of any file that has a limit.)
    pub(crate) fn wants(&self, section: Section) -> bool {
        self.sections.as_ref().is_none_or(|list| list.contains(&section))
    }

    /// The seven values `transaction_filter` names, in order: first day, last day, kind, least and most it cost, text pattern, shared or not.
    pub(crate) fn args(&self) -> [rusqlite::types::Value; 7] {
        use rusqlite::types::Value;
        let text = |t: &Option<String>| t.clone().map_or(Value::Null, Value::Text);
        let number = |n: Option<i64>| n.map_or(Value::Null, Value::Integer);
        [
            text(&self.from),
            text(&self.to),
            self.kind.map_or(Value::Null, |k| Value::Text(k.as_str().to_string())),
            number(self.min),
            number(self.max),
            text(&self.pattern),
            self.split.map_or(Value::Null, |split| Value::Integer(i64::from(split == SplitFilter::Split))),
        ]
    }

    /// `AND`-ready SQL for a category column; ids are integers, so they are written in directly.
    fn category_filter(&self, column: &str) -> String {
        match &self.categories {
            None => "1".to_string(),
            Some(ids) => {
                let list: Vec<String> = ids.iter().map(i64::to_string).collect();
                format!("{column} IN ({})", list.join(","))
            }
        }
    }
}

pub(crate) fn parse(selection: &Selection) -> Result<Parsed, AppError> {
    let date = |d: &Option<String>| d.as_deref().map(clean_date).transpose();
    let (from, to) = (date(&selection.from)?, date(&selection.to)?);
    if matches!((&from, &to), (Some(f), Some(t)) if f > t) {
        return Err(AppError::InvalidInput);
    }
    let categories = selection
        .category_ids
        .as_ref()
        .map(|ids| ids.iter().map(|id| parse_id(id)).collect::<Result<Vec<_>, _>>())
        .transpose()?;
    let amount = |a: &Option<String>| a.as_deref().map(crate::money::parse_amount).transpose();
    let (min, max) = (amount(&selection.min_amount)?, amount(&selection.max_amount)?);
    if matches!((min, max), (Some(low), Some(high)) if low > high) {
        return Err(AppError::InvalidInput);
    }
    let text = match selection.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        None => None,
        Some(text) if text.chars().count() > TEXT_MAX_CHARS || text.chars().any(char::is_control) => return Err(AppError::InvalidInput),
        Some(text) => Some(text.to_string()),
    };
    let pattern = text.as_deref().map(crate::transactions::like_pattern);
    let sections = match &selection.sections {
        None => None,
        Some(list) if list.is_empty() => return Err(AppError::InvalidInput),
        Some(list) => Some(list.clone()),
    };
    Ok(Parsed { from, to, categories, debts: selection.include_debts, kind: selection.kind, min, max, text, pattern, split: selection.split, sections })
}

/// Spreadsheet programs run a cell that starts with one of these as a formula, so such a cell is
/// written with a `'` in front, which turns it into text. Tab and carriage return are included
/// because some programs skip them before reading the formula. Shared by CSV and Excel.
pub fn neutralise(cell: &str) -> Cow<'_, str> {
    if cell.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        Cow::Owned(format!("'{cell}"))
    } else {
        Cow::Borrowed(cell)
    }
}

pub(crate) struct Tx {
    pub(crate) id: i64,
    pub(crate) date: String,
    pub(crate) kind: String,
    pub(crate) category_id: i64,
    pub(crate) category: String,
    pub(crate) name: String,
    /// What was paid.
    pub(crate) amount: i64,
    /// What it costs you (`transaction_costs`): the amount minus the parts of the others.
    pub(crate) my_share: i64,
    pub(crate) planned: Option<i64>,
    pub(crate) description: Option<String>,
    pub(crate) recurring_id: Option<i64>,
}

/// The `WHERE` of every transaction query, written for `transactions t JOIN transaction_costs k ON k.transaction_id = t.id`, with the
/// values of `Parsed::args` bound in order: `?1` the first day, `?2` the last, `?3` income or expense, `?4` the least it cost, `?5` the most,
/// `?6` a text pattern, `?7` 1 for the bills that are shared and 0 for those that are not. Each is optional, and the categories are written in
/// (they are integers).
pub(crate) fn transaction_filter(sel: &Parsed) -> String {
    format!(
        "(?1 IS NULL OR t.payment_date >= ?1) AND (?2 IS NULL OR t.payment_date <= ?2) AND {}
         AND (?3 IS NULL OR (SELECT kind.type FROM categories kind WHERE kind.id = t.category_id) = ?3)
         AND (?4 IS NULL OR k.cost_minor >= ?4) AND (?5 IS NULL OR k.cost_minor <= ?5)
         AND (?6 IS NULL OR t.name LIKE ?6 ESCAPE '\\' OR t.description LIKE ?6 ESCAPE '\\')
         AND (?7 IS NULL OR EXISTS (SELECT 1 FROM transaction_split_lines sl WHERE sl.transaction_id = t.id) = ?7)",
        sel.category_filter("t.category_id")
    )
}

pub(crate) fn transactions(conn: &Connection, sel: &Parsed) -> Result<Vec<Tx>, AppError> {
    let mut statement = conn.prepare(&format!(
        "SELECT t.id, t.payment_date, c.type, t.category_id, c.name, t.name, t.amount_minor,
                t.planned_amount_minor, t.description, t.recurring_transaction_id, k.cost_minor
         FROM transactions t JOIN categories c ON c.id = t.category_id JOIN transaction_costs k ON k.transaction_id = t.id
         WHERE {}
         ORDER BY t.payment_date, t.id",
        transaction_filter(sel)
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(sel.args()), |r| {
        Ok(Tx {
            id: r.get(0)?,
            date: r.get(1)?,
            kind: r.get(2)?,
            category_id: r.get(3)?,
            category: r.get(4)?,
            name: r.get(5)?,
            amount: r.get(6)?,
            planned: r.get(7)?,
            description: r.get(8)?,
            recurring_id: r.get(9)?,
            my_share: r.get(10)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// (income, expense) in minor units for every month that has transactions, "YYYY-MM" keys.
pub(crate) fn monthly_sums(conn: &Connection, sel: &Parsed) -> Result<std::collections::BTreeMap<String, (i64, i64)>, AppError> {
    let mut sums: std::collections::BTreeMap<String, (i64, i64)> = Default::default();
    let mut statement = conn.prepare(&format!(
        "SELECT substr(t.payment_date, 1, 7), c.type, SUM(k.cost_minor)
         FROM transactions t JOIN transaction_costs k ON k.transaction_id = t.id JOIN categories c ON c.id = t.category_id
         WHERE {} GROUP BY 1, 2",
        transaction_filter(sel)
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(sel.args()), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
    })?;
    for row in rows {
        let (month, kind, sum) = row?;
        let entry = sums.entry(month).or_default();
        if kind == "INCOME" {
            entry.0 = sum;
        } else {
            entry.1 = sum;
        }
    }
    Ok(sums)
}

pub(crate) fn currency(conn: &Connection) -> Result<String, AppError> {
    Ok(conn.query_row("SELECT currency FROM settings WHERE id = 1", [], |r| r.get(0))?)
}

/// A cell that is a number as `format_amount` writes it ("12.50", "-7.50"). It is not neutralised: a minus sign in front of a number is not
/// a formula, and an apostrophe in front of it would make a spreadsheet read the number as text. Anything that is not such a number is
/// treated like any other text.
pub(crate) fn csv_number(cell: &str) -> String {
    let digits = cell.strip_prefix('-').unwrap_or(cell);
    let plain = digits.split_once('.').is_some_and(|(whole, fraction)| {
        !whole.is_empty() && !fraction.is_empty() && whole.bytes().all(|b| b.is_ascii_digit()) && fraction.bytes().all(|b| b.is_ascii_digit())
    });
    if plain {
        cell.to_string()
    } else {
        csv_field(cell)
    }
}

pub(crate) fn csv_field(cell: &str) -> String {
    let cell = neutralise(cell);
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.into_owned()
    }
}

/// Transactions as CSV: a header row, then one row per transaction, oldest first. Amounts are
/// positive; `type` says whether it was income or an expense. `documents` names the documents attached to the transaction and `my_share` is
/// what it costs you (the same as `amount` unless the bill is shared). The file starts with a byte order
/// mark so Excel reads the UTF-8 text correctly, and rows end with CRLF as the format says.
pub fn csv(conn: &Connection, selection: &Selection) -> Result<String, AppError> {
    let sel = parse(selection)?;
    let tx = conn.unchecked_transaction()?;
    let currency = currency(&tx)?;
    let attached = document_names_by_transaction(&tx)?;
    let mut out = String::from("\u{feff}date,type,category,name,amount,currency,description,documents,my_share\r\n");
    for t in transactions(&tx, &sel)? {
        let documents = attached.get(&t.id).map(|names| join_names(names)).unwrap_or_default();
        let share = format_amount(t.my_share);
        let cells = [
            t.date.as_str(),
            t.kind.as_str(),
            t.category.as_str(),
            t.name.as_str(),
            &format_amount(t.amount),
            currency.as_str(),
            t.description.as_deref().unwrap_or(""),
            documents.as_str(),
            share.as_str(),
        ];
        let row: Vec<String> = cells.iter().map(|c| csv_field(c)).collect();
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }
    Ok(out)
}

struct CategoryRow {
    id: i64,
    kind: String,
    name: String,
    deleted: bool,
}

fn category_rows(conn: &Connection, sel: &Parsed) -> Result<Vec<CategoryRow>, AppError> {
    let mut statement = conn.prepare(&format!(
        "SELECT id, type, name, deleted_at IS NOT NULL FROM categories WHERE {} AND (?1 IS NULL OR type = ?1) ORDER BY id",
        sel.category_filter("id")
    ))?;
    let rows = statement.query_map([sel.kind.map(Kind::as_str)], |r| {
        Ok(CategoryRow { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)?, deleted: r.get(3)? })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

struct BudgetRow {
    kind: String,
    scope: String,
    category_id: Option<i64>,
    /// "YYYY-MM-01".
    effective_from: String,
    /// Null on rows that remove a budget from that month on.
    limit: Option<i64>,
}

/// Every budget row, including the ones that remove a budget from a month on. A budget holds from its month until a later row of the
/// same budget, so with dates the file keeps the rows of the months in the range and, for each budget, the row in force when the range
/// starts (unless that row removed the budget): the budgets that applied in the range, and no other.
fn budget_rows(conn: &Connection, sel: &Parsed) -> Result<Vec<BudgetRow>, AppError> {
    // Global budgets belong to no category, so they only go out with an export of every category.
    let filter = if sel.categories.is_some() { sel.category_filter("category_id") } else { "1".to_string() };
    let mut statement = conn.prepare(&format!(
        "SELECT type, scope, category_id, amount_limit_minor, effective_from
         FROM budgets WHERE {filter} AND (?1 IS NULL OR type = ?1) ORDER BY effective_from, id"
    ))?;
    let rows = statement.query_map([sel.kind.map(Kind::as_str)], |r| {
        Ok(BudgetRow { kind: r.get(0)?, scope: r.get(1)?, category_id: r.get(2)?, limit: r.get(3)?, effective_from: r.get(4)? })
    })?;
    Ok(budgets_in_range(rows.collect::<Result<Vec<_>, _>>()?, sel))
}

fn budgets_in_range(rows: Vec<BudgetRow>, sel: &Parsed) -> Vec<BudgetRow> {
    if sel.from.is_none() && sel.to.is_none() {
        return rows;
    }
    // "YYYY-MM-01" of the months the dates are in; the dates were checked, so they are ten characters long
    let first = sel.from.as_deref().map(|day| format!("{}-01", &day[..7]));
    let last = sel.to.as_deref().map(|day| format!("{}-01", &day[..7]));
    let mut in_force: std::collections::HashMap<(&str, &str, Option<i64>), usize> = Default::default();
    if let Some(first) = &first {
        // the rows are in the order of their months, so the last one before the range wins
        for (at, row) in rows.iter().enumerate().filter(|(_, row)| row.effective_from < *first) {
            in_force.insert((row.kind.as_str(), row.scope.as_str(), row.category_id), at);
        }
    }
    // a budget that has a row of its own in the first month is in force by that row, which is inside the range
    let starting_here: std::collections::HashSet<(&str, &str, Option<i64>)> = match &first {
        Some(first) => rows.iter().filter(|row| row.effective_from == *first).map(|row| (row.kind.as_str(), row.scope.as_str(), row.category_id)).collect(),
        None => Default::default(),
    };
    let carried: std::collections::HashSet<usize> =
        in_force.into_iter().filter(|(key, at)| rows[*at].limit.is_some() && !starting_here.contains(key)).map(|(_, at)| at).collect();
    rows.into_iter()
        .enumerate()
        .filter(|(at, row)| {
            let inside = first.as_ref().is_none_or(|first| row.effective_from >= *first) && last.as_ref().is_none_or(|last| row.effective_from <= *last);
            inside || carried.contains(at)
        })
        .map(|(_, row)| row)
        .collect()
}

/// The recurring items an export contains: those in the chosen categories (and of the chosen kind) that were running at some time in the
/// range of dates, that is, that started on or before its last day and did not end before its first.
/// Today's date in the person's timezone, as `now` says it.
pub(crate) fn today_of(conn: &Connection, now: DateTime<Utc>) -> Result<String, AppError> {
    let timezone: String = conn.query_row("SELECT timezone FROM settings WHERE id = 1", [], |r| r.get(0))?;
    Ok(crate::clock::today(now, &timezone)?.to_string())
}

/// The recurring payments that have come due and are not paid or skipped yet (every occurrence, the oldest first): what you owe by `today`,
/// as the debts screen lists it. Those of the chosen categories; the dates of an export do not narrow them, because what is due is a matter
/// of today, and a payment is an expense, so choosing income leaves none.
pub(crate) fn due_payments(conn: &Connection, sel: &Parsed, today: &str) -> Result<Vec<crate::debts::dues::DuePayment>, AppError> {
    if sel.kind == Some(Kind::Income) {
        return Ok(Vec::new());
    }
    Ok(crate::debts::dues::due_payments(conn, today)?
        .into_iter()
        .filter(|payment| sel.categories.as_ref().is_none_or(|ids| ids.contains(&payment.category_id)))
        .collect())
}

fn recurring_rows(conn: &Connection, sel: &Parsed) -> Result<Vec<recurring::Recurring>, AppError> {
    let mut statement = conn.prepare("SELECT id, type FROM categories")?;
    let kinds: std::collections::HashMap<i64, String> =
        statement.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<_, _>>()?;
    Ok(recurring::list(conn)?
        .into_iter()
        .filter(|item| {
            sel.categories.as_ref().is_none_or(|ids| ids.contains(&item.category_id))
                && sel.kind.is_none_or(|kind| kinds.get(&item.category_id).is_some_and(|k| k == kind.as_str()))
                && sel.from.as_deref().is_none_or(|from| item.end_date.as_deref().is_none_or(|end| end >= from))
                && sel.to.as_deref().is_none_or(|to| item.start_date.as_str() <= to)
        })
        .collect())
}

fn categories_json(conn: &Connection, sel: &Parsed) -> Result<Vec<Value>, AppError> {
    Ok(category_rows(conn, sel)?
        .into_iter()
        .map(|c| json!({ "id": c.id.to_string(), "type": c.kind, "name": c.name, "deleted": c.deleted }))
        .collect())
}

fn budgets_json(conn: &Connection, sel: &Parsed) -> Result<Vec<Value>, AppError> {
    Ok(budget_rows(conn, sel)?
        .into_iter()
        .map(|b| {
            json!({
                "type": b.kind,
                "scope": b.scope,
                "categoryId": b.category_id.map(|id| id.to_string()),
                "month": b.effective_from[..7],
                "limit": b.limit.map(format_amount),
            })
        })
        .collect())
}

fn recurring_json(conn: &Connection, sel: &Parsed) -> Result<Vec<Value>, AppError> {
    recurring_rows(conn, sel)?
        .into_iter()
        .map(|item| serde_json::to_value(item).map_err(|_| AppError::Internal))
        .collect()
}

/// The change history as recorded; the old and new values are embedded as objects, not as text.
fn history_json(conn: &Connection) -> Result<Vec<Value>, AppError> {
    let embed = |text: Option<String>| text.and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or(Value::Null);
    let mut statement = conn.prepare(
        "SELECT entity_type, entity_id, action, old_values, new_values, changed_at FROM audit_log ORDER BY id",
    )?;
    let rows = statement.query_map([], |r| {
        Ok(json!({
            "entity": r.get::<_, String>(0)?,
            "entityId": r.get::<_, String>(1)?,
            "action": r.get::<_, String>(2)?,
            "oldValues": embed(r.get(3)?),
            "newValues": embed(r.get(4)?),
            "changedAt": r.get::<_, String>(5)?,
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// What a document is attached to, as an export names it.
pub(crate) struct DocLink {
    /// "transaction", "recurring" or "ledger" (the name is then the friend's and the date the entry's).
    pub(crate) kind: &'static str,
    pub(crate) id: i64,
    pub(crate) name: String,
    /// The payment date of a transaction; none for a recurring item.
    pub(crate) date: Option<String>,
}

/// A document as an export lists it. Never the file.
pub(crate) struct DocRow {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) media_type: String,
    pub(crate) size: i64,
    pub(crate) sha256: String,
    /// The day of the paper.
    pub(crate) date: String,
    pub(crate) links: Vec<DocLink>,
}

/// The documents an export with this selection lists. With no selection every document, attached or not. With one, a document only if
/// it is attached to a transaction or a recurring item that the export contains, and then only those attachments.
pub(crate) fn documents(conn: &Connection, sel: &Parsed) -> Result<Vec<DocRow>, AppError> {
    let mut statement = conn.prepare(
        "SELECT d.id, d.name, d.media_type, d.size_bytes, d.sha256, d.document_date,
                l.transaction_id, t.name, t.payment_date,
                l.recurring_transaction_id, r.name,
                l.ledger_entry_id, pe.name, e.entry_date
         FROM documents d
         LEFT JOIN document_links l ON l.document_id = d.id
         LEFT JOIN transactions t ON t.id = l.transaction_id
         LEFT JOIN recurring_transactions r ON r.id = l.recurring_transaction_id
         LEFT JOIN ledger_entries e ON e.id = l.ledger_entry_id
         LEFT JOIN people pe ON pe.id = e.person_id
         ORDER BY d.document_date, d.id, l.transaction_id IS NULL, t.payment_date, l.transaction_id, r.name, l.recurring_transaction_id, e.entry_date, l.ledger_entry_id",
    )?;
    let mut rows = statement.query([])?;
    let everything = sel.everything();
    // an attachment counts when what it is attached to is in the export
    let held: std::collections::HashSet<i64> = transactions(conn, sel)?.iter().map(|t| t.id).collect();
    let running: std::collections::HashSet<i64> = recurring_rows(conn, sel)?.iter().map(|r| r.id).collect();
    let mut out: Vec<DocRow> = Vec::new();
    // the document being read, kept only once it is known to have an attachment in the export (or the export is of everything)
    let mut current: Option<DocRow> = None;
    let keep = |doc: Option<DocRow>, out: &mut Vec<DocRow>| {
        if let Some(doc) = doc {
            if everything || !doc.links.is_empty() {
                out.push(doc);
            }
        }
    };
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        if current.as_ref().is_none_or(|d| d.id != id) {
            keep(current.take(), &mut out);
            current = Some(DocRow {
                id,
                name: r.get(1)?,
                media_type: r.get(2)?,
                size: r.get(3)?,
                sha256: r.get(4)?,
                date: r.get(5)?,
                links: Vec::new(),
            });
        }
        let doc = current.as_mut().expect("a document is being read");
        if let Some(transaction) = r.get::<_, Option<i64>>(6)? {
            let (name, date): (String, String) = (r.get(7)?, r.get(8)?);
            if held.contains(&transaction) {
                doc.links.push(DocLink { kind: "transaction", id: transaction, name, date: Some(date) });
            }
        } else if let Some(item) = r.get::<_, Option<i64>>(9)? {
            let name: String = r.get(10)?;
            if running.contains(&item) {
                doc.links.push(DocLink { kind: "recurring", id: item, name, date: None });
            }
        } else if let Some(entry) = r.get::<_, Option<i64>>(11)? {
            // an entry of the ledger is in the export only when the ledger is (see `debts.rs`)
            let (friend, date): (String, String) = (r.get(12)?, r.get(13)?);
            if (everything || sel.debts) && sel.from.as_deref().is_none_or(|from| date.as_str() >= from) && sel.to.as_deref().is_none_or(|to| date.as_str() <= to) {
                doc.links.push(DocLink { kind: "ledger", id: entry, name: friend, date: Some(date) });
            }
        }
    }
    keep(current.take(), &mut out);
    Ok(out)
}

/// For each transaction, the names of the documents attached to it, for the column of the CSV and of the Excel sheet. The names are
/// in the order of the names.
pub(crate) fn document_names_by_transaction(conn: &Connection) -> Result<std::collections::HashMap<i64, Vec<String>>, AppError> {
    let mut statement = conn.prepare(
        "SELECT l.transaction_id, d.name FROM document_links l JOIN documents d ON d.id = l.document_id
         WHERE l.transaction_id IS NOT NULL ORDER BY l.transaction_id, d.name, d.id",
    )?;
    let mut names: std::collections::HashMap<i64, Vec<String>> = Default::default();
    for row in statement.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (transaction, name) = row?;
        names.entry(transaction).or_default().push(name);
    }
    Ok(names)
}

/// Names in one cell. A semicolon and a space between them; a name that has one itself cannot be told apart, which is the price of a
/// single cell (the JSON file lists them one by one).
pub(crate) fn join_names(names: &[String]) -> String {
    names.join("; ")
}

fn documents_json(conn: &Connection, sel: &Parsed) -> Result<Vec<Value>, AppError> {
    Ok(documents(conn, sel)?
        .into_iter()
        .map(|d| {
            let links: Vec<Value> = d
                .links
                .into_iter()
                .map(|l| json!({ "kind": l.kind, "id": l.id.to_string(), "name": l.name, "date": l.date }))
                .collect();
            json!({
                "id": d.id.to_string(),
                "name": d.name,
                "date": d.date,
                "type": d.media_type,
                "sizeBytes": d.size,
                "sha256": d.sha256,
                "links": links,
            })
        })
        .collect())
}

/// What an export with this selection would contain, so the screen can say so before anything is saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub transactions: u32,
    /// Dates of the oldest and the newest transaction in the selection.
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    /// Decimal strings, like every amount that reaches the UI.
    pub income: String,
    pub expense: String,
    pub categories: u32,
    pub budgets: u32,
    pub recurring: u32,
    /// Documents listed by name (never their files).
    pub documents: u32,
    /// Friends, ledger entries and shared bills (with their parts) in the export.
    pub people: u32,
    pub ledger: u32,
    pub split_bills: u32,
    /// Recurring payments that have come due and are listed with the debts.
    pub due_payments: u32,
    /// Lines of the history of debts: those of the friends and the recurring payments paid or due.
    pub debt_history: u32,
    /// Entries of the change history. Only a JSON file with no selection carries them; with any
    /// selection this is 0.
    pub history: u32,
}

pub fn preview(conn: &Connection, selection: &Selection) -> Result<Preview, AppError> {
    let sel = parse(selection)?;
    let tx = conn.unchecked_transaction()?;
    let (transactions, first_date, last_date, income, expense) = tx.query_row(
        &format!(
            "SELECT COUNT(*), MIN(t.payment_date), MAX(t.payment_date),
                    COALESCE(SUM(CASE WHEN c.type = 'INCOME' THEN k.cost_minor END), 0),
                    COALESCE(SUM(CASE WHEN c.type = 'EXPENSE' THEN k.cost_minor END), 0)
             FROM transactions t JOIN transaction_costs k ON k.transaction_id = t.id JOIN categories c ON c.id = t.category_id WHERE {}",
            transaction_filter(&sel)
        ),
        rusqlite::params_from_iter(sel.args()),
        |r| Ok((r.get::<_, u32>(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?)),
    )?;
    let history = if sel.everything() && sel.wants(Section::History) { tx.query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))? } else { 0 };
    let bill_lines = debts::lines(&tx, &sel)?;
    let ledger = if sel.wants(Section::Ledger) { debts::entries(&tx, &sel)? } else { Vec::new() };
    let friends = if sel.wants(Section::People) { debts::people(&tx, &sel, &bill_lines, &ledger)? } else { Vec::new() };
    let split_bills = bill_lines.iter().map(|l| l.transaction_id).collect::<std::collections::BTreeSet<_>>().len() as u32;
    let today = today_of(&tx, Utc::now())?;
    let due = if sel.wants(Section::Due) { due_payments(&tx, &sel, &today)?.len() } else { 0 };
    let timeline = if sel.wants(Section::Ledger) { debts::history_lines(&tx, &sel, &today, sel.everything() || sel.debts)?.len() } else { 0 };
    // what a file would hold: a part that is left out counts for nothing
    let count = |wanted: bool, n: usize| if wanted { n as u32 } else { 0 };
    let with_transactions = sel.wants(Section::Transactions);
    Ok(Preview {
        people: friends.len() as u32,
        due_payments: due as u32,
        debt_history: timeline as u32,
        ledger: ledger.len() as u32,
        split_bills: count(sel.wants(Section::Transactions) || sel.wants(Section::Ledger), split_bills as usize),
        transactions: count(with_transactions, transactions as usize),
        first_date: if with_transactions { first_date } else { None },
        last_date: if with_transactions { last_date } else { None },
        income: format_amount(if with_transactions { income } else { 0 }),
        expense: format_amount(if with_transactions { expense } else { 0 }),
        categories: count(sel.wants(Section::Categories), category_rows(&tx, &sel)?.len()),
        budgets: count(sel.wants(Section::Budgets), budget_rows(&tx, &sel)?.len()),
        recurring: count(sel.wants(Section::Recurring), recurring_rows(&tx, &sel)?.len()),
        documents: count(sel.wants(Section::Documents), documents(&tx, &sel)?.len()),
        history,
    })
}

/// The data as one JSON file. Amounts are decimal strings and ids are strings, as in the app's own
/// commands. With no selection it is everything, including the change history; with one it is only
/// what the selection reaches.
pub fn json(conn: &Connection, selection: &Selection, now: DateTime<Utc>) -> Result<String, AppError> {
    let sel = parse(selection)?;
    let tx = conn.unchecked_transaction()?;
    let (currency, locale, timezone, language): (String, String, String, String) = tx.query_row(
        "SELECT currency, locale, timezone, language FROM settings WHERE id = 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    // the parts of shared bills go with the transactions they are parts of; the friends and the ledger are a part of their own
    let bill_lines = debts::lines(&tx, &sel)?;
    let ledger = if sel.wants(Section::Ledger) { debts::entries(&tx, &sel)? } else { Vec::new() };
    let friends = if sel.wants(Section::People) { debts::people(&tx, &sel, &bill_lines, &ledger)? } else { Vec::new() };
    let mut parts = debts::lines_by_transaction(bill_lines);
    let transactions: Vec<Value> = transactions(&tx, &sel)?
        .into_iter()
        .map(|t| {
            let split: Vec<Value> = parts
                .remove(&t.id)
                .unwrap_or_default()
                .into_iter()
                .map(|l| json!({ "personId": l.person_id.map(|id| id.to_string()), "person": l.person, "guest": l.guest, "amount": format_amount(l.amount) }))
                .collect();
            json!({
                "id": t.id.to_string(),
                "date": t.date,
                "type": t.kind,
                "categoryId": t.category_id.to_string(),
                "category": t.category,
                "name": t.name,
                "amount": format_amount(t.amount),
                "myShare": format_amount(t.my_share),
                "split": split,
                "plannedAmount": t.planned.map(format_amount),
                "description": t.description,
                "recurringId": t.recurring_id.map(|id| id.to_string()),
            })
        })
        .collect();
    let people: Vec<Value> = friends
        .into_iter()
        .map(|p| {
            json!({
                "id": p.id.to_string(),
                "name": p.name,
                "archived": p.archived,
                "balance": p.balance.map(format_amount),
                "remindOn": p.remind_on,
                "remindEveryDays": p.remind_every_days,
            })
        })
        .collect();
    let ledger: Vec<Value> = ledger
        .into_iter()
        .map(|e| {
            json!({
                "id": e.id.to_string(),
                "date": e.date,
                "personId": e.person_id.to_string(),
                "person": e.person,
                "kind": e.kind,
                "amount": format_amount(e.amount),
                "note": e.note,
                "transactionId": e.transaction_id.map(|id| id.to_string()),
            })
        })
        .collect();

    let mut file = json!({
        "format": "meowny-export",
        "version": 1,
        "exportedAt": now.to_rfc3339_opts(SecondsFormat::Secs, true),
        "scope": {
            "from": sel.from,
            "to": sel.to,
            "categoryIds": sel.categories.as_ref().map(|ids| ids.iter().map(|i| i.to_string()).collect::<Vec<_>>()),
            "kind": sel.kind.map(Kind::as_str),
            "minAmount": sel.min.map(format_amount),
            "maxAmount": sel.max.map(format_amount),
            "text": sel.text,
            "split": selection.split.map(|s| if s == SplitFilter::Split { "split" } else { "not_split" }),
            "sections": sel.sections.as_ref().map(|list| list.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
        },
        "settings": { "currency": currency, "locale": locale, "timezone": timezone, "language": language },
    });
    // each part only when it is asked for: a file does not say what it was told to leave out
    if sel.wants(Section::Categories) {
        file["categories"] = Value::Array(categories_json(&tx, &sel)?);
    }
    if sel.wants(Section::Budgets) {
        file["budgets"] = Value::Array(budgets_json(&tx, &sel)?);
    }
    if sel.wants(Section::Recurring) {
        file["recurring"] = Value::Array(recurring_json(&tx, &sel)?);
    }
    if sel.wants(Section::Transactions) {
        file["transactions"] = Value::Array(transactions);
    }
    if sel.wants(Section::People) {
        file["people"] = Value::Array(people);
    }
    let today = today_of(&tx, now)?;
    if sel.wants(Section::Due) {
        // the payments that have come due are owed too
        let due = due_payments(&tx, &sel, &today)?;
        file["duePayments"] = Value::Array(due.into_iter().map(|item| serde_json::to_value(item).map_err(|_| AppError::Internal)).collect::<Result<_, _>>()?);
    }
    if sel.wants(Section::Ledger) {
        file["ledger"] = Value::Array(ledger);
        // the same story as the debts screen tells, oldest first
        let timeline = debts::history_lines(&tx, &sel, &today, sel.everything() || sel.debts)?;
        file["debtHistory"] = Value::Array(timeline.into_iter().map(|line| serde_json::to_value(line).map_err(|_| AppError::Internal)).collect::<Result<_, _>>()?);
    }
    if sel.wants(Section::Documents) {
        file["documents"] = Value::Array(documents_json(&tx, &sel)?);
    }
    if sel.everything() && sel.wants(Section::History) {
        file["history"] = Value::Array(history_json(&tx)?);
    }
    serde_json::to_string_pretty(&file).map_err(|_| AppError::Internal)
}

/// The kinds of file the export screen can show a preview of, as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextFormat {
    Json,
    Csv,
}

/// The most of a file that the preview carries: 64 KiB, so a big export does not cross to the screen whole.
pub const PREVIEW_TEXT_BYTES: usize = 64 * 1024;

/// The start of the file an export would write, for the preview on the export screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPreview {
    /// The text as written (without the byte order mark of a CSV file), cut after a whole line when the file is longer than
    /// `PREVIEW_TEXT_BYTES`.
    pub text: String,
    /// How long the whole file would be, in bytes.
    pub bytes: u64,
    /// Whether `text` is only the start of the file.
    pub cut: bool,
}

/// The file the export of this selection would be, as the preview shows it: the same text `json` or `csv` would save, so what is
/// seen is what is written. Nothing is written anywhere.
pub fn preview_text(conn: &Connection, selection: &Selection, format: TextFormat, now: DateTime<Utc>) -> Result<TextPreview, AppError> {
    let whole = match format {
        TextFormat::Json => json(conn, selection, now)?,
        TextFormat::Csv => csv(conn, selection)?,
    };
    let bytes = whole.len() as u64;
    let text = whole.strip_prefix('\u{feff}').unwrap_or(&whole);
    if text.len() <= PREVIEW_TEXT_BYTES {
        return Ok(TextPreview { text: text.to_string(), bytes, cut: false });
    }
    let mut end = PREVIEW_TEXT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    // after a whole line, unless there is no line break at all
    let end = text[..end].rfind('\n').map_or(end, |at| at + 1);
    Ok(TextPreview { text: text[..end].to_string(), bytes, cut: true })
}

/// The most of an Excel or PDF file that the preview carries: 16 MiB. A bigger file is saved whole all the same, it just is not looked at.
pub const PREVIEW_FILE_BYTES: usize = 16 * 1024 * 1024;

/// A whole file for the preview on the export screen, which reads it itself: the bytes as base64url text, and how long the file is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewFile {
    pub data: String,
    pub bytes: u64,
}

/// A file that was just made, handed to the preview. `InvalidInput` when it is longer than `PREVIEW_FILE_BYTES`.
pub fn preview_file(bytes: &[u8]) -> Result<PreviewFile, AppError> {
    if bytes.len() > PREVIEW_FILE_BYTES {
        return Err(AppError::InvalidInput);
    }
    Ok(PreviewFile { data: crate::sync::qr::base64url_encode(bytes), bytes: bytes.len() as u64 })
}

#[cfg(test)]
mod documents_tests;
#[cfg(test)]
pub(crate) mod filters_tests;
#[cfg(test)]
mod debts_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budgets::{self, Scope, SetBudget};
    use crate::categories::{self, CategoryType};
    use crate::transactions::{self, NewTransaction};

    pub(super) fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, 'EUR', 'en-IE', 'Europe/Dublin', 'en')",
            [],
        )
        .unwrap();
        conn
    }

    pub(super) fn spend(conn: &Connection, category: i64, name: &str, amount: &str, date: &str, description: Option<&str>) {
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

    pub(super) fn set_budget(conn: &Connection, kind: CategoryType, scope: Scope, category: Option<i64>, month: &str, limit: Option<&str>) {
        budgets::set(
            conn,
            &SetBudget {
                kind,
                scope,
                month: month.into(),
                amount_limit: limit.map(str::to_string),
                category_id: category.map(|c| c.to_string()),
            },
            Utc::now(),
        )
        .unwrap();
    }

    /// Salary and rent (with a budget and a recurring item) plus a private category for the filter tests.
    pub(super) fn lived_in() -> (Connection, i64, i64, i64) {
        let conn = db();
        let salary = categories::create(&conn, "Salary", CategoryType::Income).unwrap().id;
        let rent = categories::create(&conn, "Rent", CategoryType::Expense).unwrap().id;
        let secret = categories::create(&conn, "Therapy", CategoryType::Expense).unwrap().id;
        set_budget(&conn, CategoryType::Expense, Scope::Global, None, "2026-01", Some("2200.00"));
        set_budget(&conn, CategoryType::Expense, Scope::Category, Some(rent), "2026-01", Some("900.00"));
        set_budget(&conn, CategoryType::Expense, Scope::Category, Some(secret), "2026-01", Some("300.00"));
        set_budget(&conn, CategoryType::Expense, Scope::Category, Some(secret), "2026-03", None);
        for (category, name) in [(rent, "Rent"), (secret, "Therapy plan")] {
            recurring::create(
                &conn,
                &recurring::NewRecurring {
                    name: name.into(),
                    amount: Some("100.00".into()),
                    category_id: category.to_string(),
                    start_date: "2026-01-01".into(),
                    interval_unit: recurring::Unit::Month,
                    interval_count: 1,
                    end_date: None,
                    description: None,
                },
            )
            .unwrap();
        }
        spend(&conn, salary, "Pay", "3200.00", "2026-01-25", Some("Monthly pay"));
        spend(&conn, rent, "Rent", "900.00", "2026-02-01", None);
        spend(&conn, rent, "Rent", "905.50", "2026-03-01", None);
        spend(&conn, secret, "Session", "80.00", "2026-02-10", Some("Dr. Private"));
        (conn, salary, rent, secret)
    }

    /// A small reader for the CSV the app writes, to check the quoting by reading it back.
    pub(super) fn read_csv(text: &str) -> Vec<Vec<String>> {
        let text = text.strip_prefix('\u{feff}').unwrap();
        let (mut rows, mut row, mut cell, mut quoted) = (Vec::new(), Vec::new(), String::new(), false);
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match (c, quoted) {
                ('"', true) if chars.peek() == Some(&'"') => {
                    cell.push('"');
                    chars.next();
                }
                ('"', _) => quoted = !quoted,
                (',', false) => row.push(std::mem::take(&mut cell)),
                ('\r', false) if chars.peek() == Some(&'\n') => {
                    chars.next();
                    row.push(std::mem::take(&mut cell));
                    rows.push(std::mem::take(&mut row));
                }
                (c, _) => cell.push(c),
            }
        }
        assert!(cell.is_empty() && row.is_empty(), "the file ends with a line break");
        rows
    }

    #[test]
    fn cells_that_start_like_a_formula_are_written_as_text() {
        for dangerous in ["=1+1", "+1", "-1", "@SUM(A1)", "\t=1", "\r=1", "=HYPERLINK(\"x\",\"y\")", "-5% promo"] {
            assert_eq!(neutralise(dangerous), format!("'{dangerous}"), "{dangerous:?}");
        }
        for fine in ["", "Rent", "a=b", "1+1", "x-y", "'=already text", " leading space", "12.50", "email@example"] {
            assert_eq!(neutralise(fine), fine, "{fine:?}");
        }
    }

    #[test]
    fn a_csv_cannot_carry_a_formula_into_a_spreadsheet() {
        let (conn, _, rent, _) = lived_in();
        spend(&conn, rent, "=cmd|' /C calc'!A0", "1.00", "2026-04-01", Some("+1+1"));
        spend(&conn, rent, "@SUM(1)", "2.00", "2026-04-02", Some("-5% off, \"really\""));
        let text = csv(&conn, &Selection::default()).unwrap();
        let rows = read_csv(&text);
        for row in &rows[1..] {
            for cell in row {
                assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{cell:?} would run as a formula");
            }
        }
        let evil = rows.iter().find(|r| r[3].contains("cmd")).unwrap();
        assert_eq!((evil[3].as_str(), evil[6].as_str()), ("'=cmd|' /C calc'!A0", "'+1+1"));
        let quoted = rows.iter().find(|r| r[3] == "'@SUM(1)").unwrap();
        assert_eq!(quoted[6], "'-5% off, \"really\"", "commas and quotes survive the quoting");
        // The amounts are plain numbers: positive, so never mistaken for a formula and never prefixed.
        assert!(rows[1..].iter().all(|r| r[4].chars().next().unwrap().is_ascii_digit()));
    }

    #[test]
    fn awkward_text_survives_the_quoting() {
        let (conn, _, rent, _) = lived_in();
        spend(&conn, rent, "Café, \"the\" best", "3.00", "2026-04-01", Some("日本語 and 'quotes', commas"));
        let rows = read_csv(&csv(&conn, &Selection::default()).unwrap());
        let row = rows.last().unwrap();
        assert_eq!((row[3].as_str(), row[6].as_str()), ("Café, \"the\" best", "日本語 and 'quotes', commas"));
    }

    #[test]
    fn a_bad_selection_is_refused() {
        let (conn, ..) = lived_in();
        for bad in [
            Selection { from: Some("2026-1-1".into()), ..Default::default() },
            Selection { to: Some("tomorrow".into()), ..Default::default() },
            Selection { from: Some("2026-03-01".into()), to: Some("2026-02-01".into()), ..Default::default() },
            Selection { category_ids: Some(vec!["1; DROP TABLE transactions".into()]), ..Default::default() },
            Selection { category_ids: Some(vec!["-1".into()]), ..Default::default() },
            Selection { category_ids: Some(vec!["x".into()]), ..Default::default() },
        ] {
            assert_eq!(csv(&conn, &bad), Err(AppError::InvalidInput));
            assert_eq!(json(&conn, &bad, Utc::now()), Err(AppError::InvalidInput));
        }
        let count: i64 = conn.query_row("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 4);
    }

    pub(super) fn parsed(conn: &Connection, selection: &Selection) -> Value {
        serde_json::from_str(&json(conn, selection, Utc::now()).unwrap()).unwrap()
    }

    #[test]
    fn money_in_the_json_is_always_a_decimal_string() {
        let (conn, ..) = lived_in();
        let text = json(&conn, &Selection::default(), Utc::now()).unwrap();
        let file: Value = serde_json::from_str(&text).unwrap();
        for section in ["budgets", "recurring", "transactions"] {
            for item in file[section].as_array().unwrap() {
                for key in ["amount", "limit", "plannedAmount"] {
                    if let Some(value) = item.get(key).filter(|v| !v.is_null()) {
                        assert!(value.as_str().is_some_and(|s| s.contains('.')), "{section}.{key} = {value}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_limited_json_reveals_nothing_about_the_rest() {
        let (conn, _, rent, _) = lived_in();
        let text = json(&conn, &Selection { category_ids: Some(vec![rent.to_string()]), ..Default::default() }, Utc::now()).unwrap();
        for hidden in ["Therapy", "Dr. Private", "Session", "Monthly pay", "Salary", "300.00", "2200.00"] {
            assert!(!text.contains(hidden), "{hidden:?} leaked into a Rent-only export");
        }
        let file: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(file["categories"].as_array().unwrap().len(), 1);
        let budgets = file["budgets"].as_array().unwrap();
        assert!(!budgets.is_empty());
        let rent_id = rent.to_string();
        assert!(budgets.iter().all(|b| b["categoryId"] == rent_id.as_str()), "no global budget, no other category's budget");
        assert_eq!(file["recurring"].as_array().unwrap().len(), 1);
        assert_eq!(file["transactions"].as_array().unwrap().len(), 2);
        assert!(file.get("history").is_none(), "history mentions every category");
        assert_eq!(file["scope"]["categoryIds"][0], rent.to_string());
    }

    #[test]
    fn a_date_range_alone_is_not_the_whole_data() {
        let (conn, ..) = lived_in();
        let file = parsed(&conn, &Selection { from: Some("2026-03-01".into()), ..Default::default() });
        assert_eq!(file["transactions"].as_array().unwrap().len(), 1);
        assert!(file.get("history").is_none());
        assert_eq!(file["categories"].as_array().unwrap().len(), 3, "a range does not hide categories");
    }

    #[test]
    fn deleted_categories_are_marked_and_their_transactions_keep_the_name() {
        let (mut conn, _, _, secret) = lived_in();
        categories::delete(&mut conn, secret, Utc::now()).unwrap();
        let file = parsed(&conn, &Selection::default());
        let gone = file["categories"].as_array().unwrap().iter().find(|c| c["name"] == "Therapy").unwrap();
        assert_eq!(gone["deleted"], true);
        let rows = read_csv(&csv(&conn, &Selection::default()).unwrap());
        assert!(rows.iter().any(|r| r[2] == "Therapy"));
    }

    #[test]
    fn the_scheduled_backup_passphrase_is_never_exported() {
        let (conn, ..) = lived_in();
        conn.execute(
            "INSERT INTO backup_schedule (id, folder, every_days, passphrase) VALUES (1, '/somewhere/private', 7, 'unmistakable passphrase')",
            [],
        )
        .unwrap();
        let text = json(&conn, &Selection::default(), Utc::now()).unwrap();
        assert!(!text.contains("unmistakable") && !text.contains("/somewhere/private"));
        assert!(!csv(&conn, &Selection::default()).unwrap().contains("unmistakable"));
    }

    #[test]
    fn access_tokens_are_never_exported() {
        let (conn, ..) = lived_in();
        let input = crate::tokens::NewToken { name: "unmistakable agent".into(), scope: crate::tokens::Scope::Write, expires_in_days: 30 };
        crate::tokens::create(&conn, &input, Utc::now()).unwrap();
        let hash: String = conn.query_row("SELECT token_hash FROM api_tokens", [], |r| r.get(0)).unwrap();
        for text in [json(&conn, &Selection::default(), Utc::now()).unwrap(), csv(&conn, &Selection::default()).unwrap()] {
            assert!(!text.contains("unmistakable") && !text.contains(&hash) && !text.contains("api_tokens"));
        }
    }

    #[test]
    fn the_preview_of_nothing_is_empty_not_an_error() {
        let conn = db();
        let empty = preview(&conn, &Selection::default()).unwrap();
        assert_eq!((empty.transactions, empty.first_date, empty.last_date), (0, None, None));
        assert_eq!((empty.income.as_str(), empty.expense.as_str()), ("0.00", "0.00"));
        assert_eq!(preview(&conn, &Selection { from: Some("bad".into()), ..Default::default() }), Err(AppError::InvalidInput));
    }

    #[test]
    fn a_bill_with_no_fixed_amount_has_a_null_amount_in_the_json() {
        let (conn, _, rent, _) = lived_in();
        recurring::create(
            &conn,
            &recurring::NewRecurring {
                name: "Electricity".into(),
                amount: None,
                category_id: rent.to_string(),
                start_date: "2026-01-20".into(),
                interval_unit: recurring::Unit::Month,
                interval_count: 1,
                end_date: None,
                description: None,
            },
        )
        .unwrap();
        let file = parsed(&conn, &Selection::default());
        let items = file["recurring"].as_array().unwrap();
        let electricity = items.iter().find(|i| i["name"] == "Electricity").unwrap();
        assert!(electricity["amount"].is_null());
        assert!(items.iter().filter(|i| i["name"] != "Electricity").all(|i| i["amount"].is_string()));
    }

    #[test]
    fn exporting_changes_nothing() {
        let (conn, ..) = lived_in();
        let before: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
        csv(&conn, &Selection::default()).unwrap();
        json(&conn, &Selection::default(), Utc::now()).unwrap();
        let after: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
    }
}

#[cfg(test)]
mod preview_text_tests {
    use super::tests::{db, spend};
    use super::*;
    use crate::categories::{self, CategoryType};

    fn now() -> DateTime<Utc> {
        "2026-04-01T09:00:00Z".parse().unwrap()
    }

    fn everything() -> Selection {
        Selection::default()
    }

    #[test]
    fn a_long_file_is_cut_after_a_whole_line_and_says_how_long_it_is() {
        let conn = db();
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        for at in 0..1500 {
            spend(&conn, food, &format!("Shop number {at}"), "12.50", "2026-01-15", Some("a description to make the line longer"));
        }
        let file = csv(&conn, &everything()).unwrap();
        let shown = preview_text(&conn, &everything(), TextFormat::Csv, now()).unwrap();
        assert!(file.len() > PREVIEW_TEXT_BYTES * 2);
        assert!(shown.cut);
        assert_eq!(shown.bytes, file.len() as u64);
        assert!(shown.text.len() <= PREVIEW_TEXT_BYTES);
        assert!(shown.text.ends_with("\r\n"), "a whole line, not half of one");
        assert!(file.strip_prefix('\u{feff}').unwrap().starts_with(&shown.text), "it is the start of the file, as it is");
        assert!(shown.text.len() > PREVIEW_TEXT_BYTES - 400, "and nearly as much as fits");
    }

    #[test]
    fn a_cut_never_lands_inside_a_character() {
        let conn = db();
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        // 3-byte characters, so the limit falls inside one for some lengths of name
        for at in 0..400 {
            spend(&conn, food, &format!("{}{}", "\u{20ac}".repeat(40 + at % 3), at), "1.00", "2026-01-15", Some(&"\u{20ac}".repeat(60)));
        }
        let shown = preview_text(&conn, &everything(), TextFormat::Csv, now()).unwrap();
        assert!(shown.cut && shown.text.len() <= PREVIEW_TEXT_BYTES);
    }
}

#[cfg(test)]
mod preview_file_tests {
    
    use super::*;

    #[test]
    fn a_file_over_the_limit_is_not_given() {
        assert!(preview_file(&vec![7u8; PREVIEW_FILE_BYTES]).is_ok());
        assert_eq!(preview_file(&vec![7u8; PREVIEW_FILE_BYTES + 1]), Err(AppError::InvalidInput));
        assert_eq!(preview_file(&[]).map(|f| f.bytes), Ok(0));
    }
}
