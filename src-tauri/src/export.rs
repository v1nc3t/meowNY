//! Plain-file exports: a JSON file with the data and a CSV of transactions. These files are not
//! encrypted (the encrypted copy is the backup), so the export screen says so before saving.
//!
//! A `Selection` narrows what goes in: a date range for transactions, and a set of categories for
//! everything that belongs to a category. With categories chosen, other category names, global
//! budgets and the change history are left out, so a limited export reveals nothing about the rest.
//!
//! Documents are listed by name, date, type, size, hash and what they are attached to, and a file is never in an export (the
//! encrypted backup is where the files go). A limited export lists only the documents attached to something that is in it, and for
//! each only the attachments that are in it, so the name of a document attached to something left out is not revealed.
//! The device-specific scheduled-backup setup, which holds a passphrase, and the access tokens of the
//! command-line client are never exported.

#[cfg(desktop)]
mod xlsx;

use std::borrow::Cow;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection};
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
}

pub(crate) struct Parsed {
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) categories: Option<Vec<i64>>,
}

impl Parsed {
    fn everything(&self) -> bool {
        self.from.is_none() && self.to.is_none() && self.categories.is_none()
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
    Ok(Parsed { from, to, categories })
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
    pub(crate) amount: i64,
    pub(crate) planned: Option<i64>,
    pub(crate) description: Option<String>,
    pub(crate) recurring_id: Option<i64>,
}

/// The `WHERE` of every transaction query: `?1` is the first date, `?2` the last, both optional.
pub(crate) fn transaction_filter(sel: &Parsed) -> String {
    format!(
        "(?1 IS NULL OR t.payment_date >= ?1) AND (?2 IS NULL OR t.payment_date <= ?2) AND {}",
        sel.category_filter("t.category_id")
    )
}

pub(crate) fn transactions(conn: &Connection, sel: &Parsed) -> Result<Vec<Tx>, AppError> {
    let mut statement = conn.prepare(&format!(
        "SELECT t.id, t.payment_date, c.type, t.category_id, c.name, t.name, t.amount_minor,
                t.planned_amount_minor, t.description, t.recurring_transaction_id
         FROM transactions t JOIN categories c ON c.id = t.category_id
         WHERE {}
         ORDER BY t.payment_date, t.id",
        transaction_filter(sel)
    ))?;
    let rows = statement.query_map(params![sel.from, sel.to], |r| {
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
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// (income, expense) in minor units for every month that has transactions, "YYYY-MM" keys.
pub(crate) fn monthly_sums(conn: &Connection, sel: &Parsed) -> Result<std::collections::BTreeMap<String, (i64, i64)>, AppError> {
    let mut sums: std::collections::BTreeMap<String, (i64, i64)> = Default::default();
    let mut statement = conn.prepare(&format!(
        "SELECT substr(t.payment_date, 1, 7), c.type, SUM(t.amount_minor)
         FROM transactions t JOIN categories c ON c.id = t.category_id
         WHERE {} GROUP BY 1, 2",
        transaction_filter(sel)
    ))?;
    let rows = statement.query_map(params![sel.from, sel.to], |r| {
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

fn csv_field(cell: &str) -> String {
    let cell = neutralise(cell);
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.into_owned()
    }
}

/// Transactions as CSV: a header row, then one row per transaction, oldest first. Amounts are
/// positive; `type` says whether it was income or an expense. The last column names the documents attached to the transaction. The file starts with a byte order
/// mark so Excel reads the UTF-8 text correctly, and rows end with CRLF as the format says.
pub fn csv(conn: &Connection, selection: &Selection) -> Result<String, AppError> {
    let sel = parse(selection)?;
    let tx = conn.unchecked_transaction()?;
    let currency = currency(&tx)?;
    let attached = document_names_by_transaction(&tx)?;
    let mut out = String::from("\u{feff}date,type,category,name,amount,currency,description,documents\r\n");
    for t in transactions(&tx, &sel)? {
        let documents = attached.get(&t.id).map(|names| join_names(names)).unwrap_or_default();
        let cells = [
            t.date.as_str(),
            t.kind.as_str(),
            t.category.as_str(),
            t.name.as_str(),
            &format_amount(t.amount),
            currency.as_str(),
            t.description.as_deref().unwrap_or(""),
            documents.as_str(),
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
        "SELECT id, type, name, deleted_at IS NOT NULL FROM categories WHERE {} ORDER BY id",
        sel.category_filter("id")
    ))?;
    let rows = statement.query_map([], |r| {
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

/// Every budget row, including the ones that remove a budget from a month on.
fn budget_rows(conn: &Connection, sel: &Parsed) -> Result<Vec<BudgetRow>, AppError> {
    // Global budgets belong to no category, so they only go out with an export of every category.
    let filter = if sel.categories.is_some() { sel.category_filter("category_id") } else { "1".to_string() };
    let mut statement = conn.prepare(&format!(
        "SELECT type, scope, category_id, amount_limit_minor, effective_from
         FROM budgets WHERE {filter} ORDER BY effective_from, id"
    ))?;
    let rows = statement.query_map([], |r| {
        Ok(BudgetRow { kind: r.get(0)?, scope: r.get(1)?, category_id: r.get(2)?, limit: r.get(3)?, effective_from: r.get(4)? })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn recurring_rows(conn: &Connection, sel: &Parsed) -> Result<Vec<recurring::Recurring>, AppError> {
    Ok(recurring::list(conn)?
        .into_iter()
        .filter(|item| sel.categories.as_ref().is_none_or(|ids| ids.contains(&item.category_id)))
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
    /// "transaction" or "recurring".
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

impl Parsed {
    fn has_transaction(&self, date: &str, category_id: i64) -> bool {
        self.from.as_deref().is_none_or(|from| date >= from)
            && self.to.as_deref().is_none_or(|to| date <= to)
            && self.categories.as_ref().is_none_or(|ids| ids.contains(&category_id))
    }

    fn has_recurring(&self, category_id: i64) -> bool {
        self.categories.as_ref().is_none_or(|ids| ids.contains(&category_id))
    }
}

/// The documents an export with this selection lists. With no selection every document, attached or not. With one, a document only if
/// it is attached to a transaction or a recurring item that the export contains, and then only those attachments.
pub(crate) fn documents(conn: &Connection, sel: &Parsed) -> Result<Vec<DocRow>, AppError> {
    let mut statement = conn.prepare(
        "SELECT d.id, d.name, d.media_type, d.size_bytes, d.sha256, d.document_date,
                l.transaction_id, t.name, t.payment_date, t.category_id,
                l.recurring_transaction_id, r.name, r.category_id
         FROM documents d
         LEFT JOIN document_links l ON l.document_id = d.id
         LEFT JOIN transactions t ON t.id = l.transaction_id
         LEFT JOIN recurring_transactions r ON r.id = l.recurring_transaction_id
         ORDER BY d.document_date, d.id, l.transaction_id IS NULL, t.payment_date, l.transaction_id, r.name, l.recurring_transaction_id",
    )?;
    let mut rows = statement.query([])?;
    let everything = sel.everything();
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
            let (name, date, category): (String, String, i64) = (r.get(7)?, r.get(8)?, r.get(9)?);
            if sel.has_transaction(&date, category) {
                doc.links.push(DocLink { kind: "transaction", id: transaction, name, date: Some(date) });
            }
        } else if let Some(item) = r.get::<_, Option<i64>>(10)? {
            let (name, category): (String, i64) = (r.get(11)?, r.get(12)?);
            if sel.has_recurring(category) {
                doc.links.push(DocLink { kind: "recurring", id: item, name, date: None });
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
                    COALESCE(SUM(CASE WHEN c.type = 'INCOME' THEN t.amount_minor END), 0),
                    COALESCE(SUM(CASE WHEN c.type = 'EXPENSE' THEN t.amount_minor END), 0)
             FROM transactions t JOIN categories c ON c.id = t.category_id WHERE {}",
            transaction_filter(&sel)
        ),
        params![sel.from, sel.to],
        |r| Ok((r.get::<_, u32>(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?)),
    )?;
    let history = if sel.everything() { tx.query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))? } else { 0 };
    Ok(Preview {
        transactions,
        first_date,
        last_date,
        income: format_amount(income),
        expense: format_amount(expense),
        categories: category_rows(&tx, &sel)?.len() as u32,
        budgets: budget_rows(&tx, &sel)?.len() as u32,
        recurring: recurring_rows(&tx, &sel)?.len() as u32,
        documents: documents(&tx, &sel)?.len() as u32,
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
    let transactions: Vec<Value> = transactions(&tx, &sel)?
        .into_iter()
        .map(|t| {
            json!({
                "id": t.id.to_string(),
                "date": t.date,
                "type": t.kind,
                "categoryId": t.category_id.to_string(),
                "category": t.category,
                "name": t.name,
                "amount": format_amount(t.amount),
                "plannedAmount": t.planned.map(format_amount),
                "description": t.description,
                "recurringId": t.recurring_id.map(|id| id.to_string()),
            })
        })
        .collect();

    let mut file = json!({
        "format": "meowny-export",
        "version": 1,
        "exportedAt": now.to_rfc3339_opts(SecondsFormat::Secs, true),
        "scope": { "from": sel.from, "to": sel.to, "categoryIds": sel.categories.as_ref().map(|ids| ids.iter().map(|i| i.to_string()).collect::<Vec<_>>()) },
        "settings": { "currency": currency, "locale": locale, "timezone": timezone, "language": language },
        "categories": categories_json(&tx, &sel)?,
        "budgets": budgets_json(&tx, &sel)?,
        "recurring": recurring_json(&tx, &sel)?,
        "transactions": transactions,
        "documents": documents_json(&tx, &sel)?,
    });
    if sel.everything() {
        file["history"] = Value::Array(history_json(&tx)?);
    }
    serde_json::to_string_pretty(&file).map_err(|_| AppError::Internal)
}

#[cfg(test)]
mod documents_tests;

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
    fn the_csv_has_one_row_per_transaction_oldest_first() {
        let (conn, ..) = lived_in();
        let text = csv(&conn, &Selection::default()).unwrap();
        assert!(text.starts_with("\u{feff}date,type,category,name,amount,currency,description,documents\r\n"));
        let rows = read_csv(&text);
        assert_eq!(rows.len(), 5, "header and four transactions");
        assert_eq!(rows[1], ["2026-01-25", "INCOME", "Salary", "Pay", "3200.00", "EUR", "Monthly pay", ""]);
        assert_eq!(rows[2], ["2026-02-01", "EXPENSE", "Rent", "Rent", "900.00", "EUR", "", ""]);
        assert_eq!(rows[3][3..5], ["Session", "80.00"]);
        assert_eq!(rows[4][4], "905.50");
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
    fn a_selection_narrows_the_transactions() {
        let (conn, salary, rent, _) = lived_in();
        let names = |sel: &Selection| -> Vec<String> {
            read_csv(&csv(&conn, sel).unwrap())[1..].iter().map(|r| format!("{} {}", r[0], r[3])).collect()
        };
        let range = Selection { from: Some("2026-02-01".into()), to: Some("2026-02-10".into()), ..Default::default() };
        assert_eq!(names(&range), ["2026-02-01 Rent", "2026-02-10 Session"], "both ends are inclusive");

        let categories = Selection { category_ids: Some(vec![rent.to_string(), salary.to_string()]), ..Default::default() };
        assert_eq!(names(&categories).len(), 3);

        let both = Selection { from: Some("2026-03-01".into()), category_ids: Some(vec![rent.to_string()]), ..Default::default() };
        assert_eq!(names(&both), ["2026-03-01 Rent"]);

        let none = Selection { category_ids: Some(vec![]), ..Default::default() };
        assert!(names(&none).is_empty());
        let empty_range = Selection { from: Some("2030-01-01".into()), ..Default::default() };
        assert!(names(&empty_range).is_empty());
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
    fn the_full_json_holds_all_of_the_data() {
        let (conn, ..) = lived_in();
        let file = parsed(&conn, &Selection::default());
        assert_eq!((file["format"].as_str(), file["version"].as_i64()), (Some("meowny-export"), Some(1)));
        assert_eq!(file["settings"]["currency"], "EUR");
        assert_eq!(file["categories"].as_array().unwrap().len(), 3);
        let stored: i64 = conn.query_row("SELECT count(*) FROM budgets", [], |r| r.get(0)).unwrap();
        assert_eq!(file["budgets"].as_array().unwrap().len() as i64, stored, "every budget row, carry-forward rows included");
        assert_eq!(file["recurring"].as_array().unwrap().len(), 2);
        assert_eq!(file["transactions"].as_array().unwrap().len(), 4);
        assert!(!file["history"].as_array().unwrap().is_empty(), "the change history is part of all the data");

        let pay = &file["transactions"][0];
        assert_eq!((pay["amount"].as_str(), pay["date"].as_str(), pay["type"].as_str()), (Some("3200.00"), Some("2026-01-25"), Some("INCOME")));
        assert!(pay["id"].is_string() && pay["categoryId"].is_string(), "ids are strings, as in the app");
        let removed = file["budgets"].as_array().unwrap().iter().find(|b| b["month"] == "2026-03").unwrap();
        assert!(removed["limit"].is_null(), "a removed budget stays visible");
        assert!(file["budgets"].as_array().unwrap().iter().any(|b| b["scope"] == "GLOBAL" && b["limit"] == "2200.00"));
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
    fn the_preview_counts_what_the_files_contain() {
        let (conn, _, rent, _) = lived_in();
        for selection in [
            Selection::default(),
            Selection { from: Some("2026-02-01".into()), ..Default::default() },
            Selection { category_ids: Some(vec![rent.to_string()]), ..Default::default() },
            Selection { from: Some("2026-02-01".into()), to: Some("2026-02-28".into()), category_ids: Some(vec![rent.to_string()]) },
            Selection { category_ids: Some(vec![]), ..Default::default() },
        ] {
            let shown = preview(&conn, &selection).unwrap();
            let file = parsed(&conn, &selection);
            let count = |section: &str| file[section].as_array().unwrap().len() as u32;
            assert_eq!(shown.transactions, count("transactions"), "{selection:?}");
            assert_eq!((shown.categories, shown.budgets, shown.recurring), (count("categories"), count("budgets"), count("recurring")), "{selection:?}");
            assert_eq!(shown.history, file.get("history").map_or(0, |h| h.as_array().unwrap().len() as u32), "{selection:?}");
            assert_eq!(read_csv(&csv(&conn, &selection).unwrap()).len() as u32 - 1, shown.transactions, "{selection:?}");
        }
    }

    #[test]
    fn the_preview_gives_the_span_and_the_totals() {
        let (conn, ..) = lived_in();
        let all = preview(&conn, &Selection::default()).unwrap();
        assert_eq!((all.first_date.as_deref(), all.last_date.as_deref()), (Some("2026-01-25"), Some("2026-03-01")));
        assert_eq!((all.income.as_str(), all.expense.as_str()), ("3200.00", "1885.50"), "900.00 + 905.50 rent and 80.00 of therapy");
        assert!(all.history > 0);

        let narrow = preview(&conn, &Selection { from: Some("2026-03-01".into()), ..Default::default() }).unwrap();
        assert_eq!((narrow.transactions, narrow.first_date.as_deref(), narrow.expense.as_str()), (1, Some("2026-03-01"), "905.50"));
        assert_eq!(narrow.history, 0, "a range is not all the data");
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
