//! The ledger of a friend: money lent, borrowed and paid back, kept apart from the transactions (`docs/debts-design.md`, section 6).
//!
//! * A loan or a repayment is **never income or expense**: it lives only here and no sum of spending or earning reads it, so it cannot
//!   inflate your income, hide a real cost or touch a budget.
//! * The balance of a friend is the sum of their parts of split bills and of the signed entries: who owes whom, and how much. It is a
//!   running total; a repayment is not matched to one bill.
//! * "Mark as paid" takes an amount that may be larger than the debt: the balance then turns around and you owe the rest.
//! * "I borrowed" can add your share to your expenses at the same time, in one step: someone else paid, you owe them, and the month shows
//!   your part. Paying them back later is another entry and never a second expense.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Deserializer, Serialize};

use super::people::{self, Person};
use crate::categories::{parse_id, serialize_id};
use crate::money::{format_amount, parse_positive_amount};
use crate::transactions::{self, clean_date, NewTransaction};
use crate::vault::AppError;

const NOTE_MAX_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Kind {
    /// I lent them money.
    Lent,
    /// I borrowed from them.
    Borrowed,
    /// They paid me back.
    RepaidToMe,
    /// I paid them back.
    RepaidByMe,
}

impl Kind {
    pub fn as_sql(self) -> &'static str {
        match self {
            Kind::Lent => "LENT",
            Kind::Borrowed => "BORROWED",
            Kind::RepaidToMe => "REPAID_TO_ME",
            Kind::RepaidByMe => "REPAID_BY_ME",
        }
    }

    pub fn from_sql(text: &str) -> Result<Kind, AppError> {
        match text {
            "LENT" => Ok(Kind::Lent),
            "BORROWED" => Ok(Kind::Borrowed),
            "REPAID_TO_ME" => Ok(Kind::RepaidToMe),
            "REPAID_BY_ME" => Ok(Kind::RepaidByMe),
            _ => Err(AppError::Internal),
        }
    }

    /// +1 when the entry makes the friend owe you more, -1 when less.
    pub fn sign(self) -> i64 {
        match self {
            Kind::Lent | Kind::RepaidByMe => 1,
            Kind::Borrowed | Kind::RepaidToMe => -1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    #[serde(serialize_with = "serialize_id")]
    pub person_id: i64,
    pub kind: Kind,
    /// Decimal string, e.g. "12.50". Always positive: the kind says which way it goes.
    pub amount: String,
    pub date: String,
    pub note: Option<String>,
    /// The expense this entry came with, if it is still there.
    pub transaction_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewEntry {
    pub person_id: String,
    pub kind: Kind,
    pub amount: String,
    pub date: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub transaction_id: Option<String>,
}

/// Fields left out stay as they are. `note: null` and `transactionId: null` clear them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryPatch {
    #[serde(default)]
    pub person_id: Option<String>,
    #[serde(default)]
    pub kind: Option<Kind>,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub note: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub transaction_id: Option<Option<String>>,
}

fn present_or_null<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::deserialize(deserializer)?))
}

/// The expense that "I borrowed" adds to your expenses along with the entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Expense {
    pub name: String,
    pub category_id: String,
}

/// Empty means "no note".
fn clean_note(text: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    if text.chars().count() > NOTE_MAX_CHARS || text.chars().any(char::is_control) {
        return Err(AppError::InvalidInput);
    }
    Ok(Some(text.to_string()))
}

const COLUMNS: &str = "id, person_id, kind, amount_minor, entry_date, note, transaction_id";

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Entry> {
    let kind: String = row.get(2)?;
    Ok(Entry {
        id: row.get(0)?,
        person_id: row.get(1)?,
        kind: Kind::from_sql(&kind).map_err(|_| rusqlite::Error::InvalidQuery)?,
        amount: format_amount(row.get(3)?),
        date: row.get(4)?,
        note: row.get(5)?,
        transaction_id: row.get::<_, Option<i64>>(6)?.map(|id| id.to_string()),
    })
}

pub fn find(conn: &Connection, id: i64) -> Result<Option<Entry>, AppError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM ledger_entries WHERE id = ?1"), [id], from_row).optional()?)
}

pub fn get(conn: &Connection, id: i64) -> Result<Entry, AppError> {
    find(conn, id)?.ok_or(AppError::NotFound)
}

fn person_exists(conn: &Connection, id: i64) -> Result<bool, AppError> {
    Ok(conn.query_row("SELECT 1 FROM people WHERE id = ?1 AND merged_into IS NULL", [id], |_| Ok(())).optional()?.is_some())
}

fn transaction_exists(conn: &Connection, id: i64) -> Result<bool, AppError> {
    Ok(conn.query_row("SELECT 1 FROM transactions WHERE id = ?1", [id], |_| Ok(())).optional()?.is_some())
}

/// Adds an entry. The friend may be archived: a repayment can come after the friend was hidden.
pub fn add(conn: &Connection, input: &NewEntry) -> Result<Entry, AppError> {
    add_from(conn, input, None)
}

/// Adds an entry that came from a bill code: `origin` is the code's id, so the same code is never accepted twice.
pub(crate) fn add_from(conn: &Connection, input: &NewEntry, origin: Option<&str>) -> Result<Entry, AppError> {
    let person_id = parse_id(&input.person_id)?;
    let minor = parse_positive_amount(&input.amount)?;
    let date = clean_date(&input.date)?;
    let note = clean_note(input.note.as_deref())?;
    let transaction_id = input.transaction_id.as_deref().map(parse_id).transpose()?;
    if !person_exists(conn, person_id)? {
        return Err(AppError::InvalidInput);
    }
    if let Some(id) = transaction_id {
        if !transaction_exists(conn, id)? {
            return Err(AppError::InvalidInput);
        }
    }
    conn.execute(
        "INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date, note, transaction_id, origin_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![person_id, input.kind.as_sql(), minor, date, note, transaction_id, origin],
    )?;
    get(conn, conn.last_insert_rowid())
}

/// "I borrowed", with your share added to your expenses: the expense (the given name, your amount, the date, the category, which must be an
/// expense category) and the `BORROWED` entry linked to it, in one step or not at all.
pub fn borrow_with_expense(conn: &Connection, input: &NewEntry, expense: &Expense) -> Result<Entry, AppError> {
    let tx = conn.unchecked_transaction()?;
    let entry = borrow_in(&tx, input, expense, None)?;
    tx.commit()?;
    Ok(entry)
}

/// The same, for a caller that has its own database transaction (taking in a bill code). `origin` is the id of the code, if there was one.
pub(crate) fn borrow_in(conn: &Connection, input: &NewEntry, expense: &Expense, origin: Option<&str>) -> Result<Entry, AppError> {
    if input.kind != Kind::Borrowed {
        return Err(AppError::InvalidInput);
    }
    let category = parse_id(&expense.category_id)?;
    let is_expense: bool = conn
        .query_row("SELECT type = 'EXPENSE' AND deleted_at IS NULL FROM categories WHERE id = ?1", [category], |r| r.get(0))
        .optional()?
        .unwrap_or(false);
    if !is_expense {
        return Err(AppError::InvalidInput);
    }
    let spent = transactions::create(
        conn,
        &NewTransaction {
            name: expense.name.clone(),
            amount: input.amount.clone(),
            payment_date: input.date.clone(),
            category_id: expense.category_id.clone(),
            description: None,
            client_uuid: None,
        },
    )?;
    add_from(conn, &NewEntry { transaction_id: Some(spent.id.to_string()), ..input.clone() }, origin)
}

pub fn update(conn: &Connection, id: i64, patch: &EntryPatch) -> Result<Entry, AppError> {
    let current = get(conn, id)?;
    let person_id = match &patch.person_id {
        Some(text) => {
            let person_id = parse_id(text)?;
            if !person_exists(conn, person_id)? {
                return Err(AppError::InvalidInput);
            }
            person_id
        }
        None => current.person_id,
    };
    let kind = patch.kind.unwrap_or(current.kind);
    let minor = match &patch.amount {
        Some(amount) => parse_positive_amount(amount)?,
        None => parse_positive_amount(&current.amount)?,
    };
    let date = match &patch.date {
        Some(date) => clean_date(date)?,
        None => current.date.clone(),
    };
    let note = match &patch.note {
        Some(note) => clean_note(note.as_deref())?,
        None => current.note.clone(),
    };
    let transaction_id = match &patch.transaction_id {
        Some(Some(text)) => {
            let transaction_id = parse_id(text)?;
            if !transaction_exists(conn, transaction_id)? {
                return Err(AppError::InvalidInput);
            }
            Some(transaction_id)
        }
        Some(None) => None,
        None => current.transaction_id.as_deref().map(parse_id).transpose()?,
    };
    // Every column is written, so that a change of the link alone is an edit too (the stamp triggers do not watch it by itself).
    conn.execute(
        "UPDATE ledger_entries SET person_id = ?1, kind = ?2, amount_minor = ?3, entry_date = ?4, note = ?5, transaction_id = ?6 WHERE id = ?7",
        params![person_id, kind.as_sql(), minor, date, note, transaction_id, id],
    )?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: i64) -> Result<(), AppError> {
    match conn.execute("DELETE FROM ledger_entries WHERE id = ?1", [id])? {
        0 => Err(AppError::NotFound),
        _ => Ok(()),
    }
}

/// What "mark as paid" did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settled {
    pub entry: Entry,
    /// The balance before and after: positive when they owe you.
    pub before: String,
    pub after: String,
    /// How much of the payment was more than the debt. When it is above 0.00 the balance has turned around: the rest counts as a debt the
    /// other way.
    pub over: String,
}

/// Marks a payment between you and a friend. It is a repayment in the direction that reduces the debt: they paid you back when they owe you,
/// you paid them back when you owe them. With no `amount` it is the whole balance. An amount larger than the debt is accepted. When the
/// balance reaches 0 the friend's reminder is cleared.
pub fn settle(conn: &Connection, person_id: i64, amount: Option<&str>, date: Option<&str>, note: Option<&str>, today: &str) -> Result<Settled, AppError> {
    let tx = conn.unchecked_transaction()?;
    let before = balance_minor(&tx, person_id)?;
    if before == 0 {
        return Err(AppError::InvalidInput);
    }
    let minor = match amount {
        Some(text) => parse_positive_amount(text)?,
        None => before.abs(),
    };
    let kind = if before > 0 { Kind::RepaidToMe } else { Kind::RepaidByMe };
    let entry = add(
        &tx,
        &NewEntry {
            person_id: person_id.to_string(),
            kind,
            amount: format_amount(minor),
            date: date.unwrap_or(today).to_string(),
            note: note.map(str::to_string),
            transaction_id: None,
        },
    )?;
    let after = balance_minor(&tx, person_id)?;
    if after == 0 {
        tx.execute("UPDATE people SET remind_on = NULL, remind_every_days = NULL WHERE id = ?1", [person_id])?;
    }
    tx.commit()?;
    Ok(Settled { entry, before: format_amount(before), after: format_amount(after), over: format_amount((minor - before.abs()).max(0)) })
}

/// What a friend owes you in minor units; negative when you owe them. The one definition is the view `person_balances`.
pub fn balance_minor(conn: &Connection, person_id: i64) -> Result<i64, AppError> {
    conn.query_row("SELECT balance_minor FROM person_balances WHERE person_id = ?1", [person_id], |r| r.get(0)).optional()?.ok_or(AppError::NotFound)
}

/// One line of a friend's ledger: a part of a bill or an entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub date: String,
    /// `BILL` for their part of a split bill, otherwise the kind of the entry.
    pub kind: String,
    /// The name of the bill, or the note of the entry; empty when there is none.
    pub what: String,
    /// Always positive.
    pub amount: String,
    /// What it did to the balance: positive when they owe you more.
    pub effect: String,
    /// The balance after this line.
    pub balance: String,
    /// The same in minor units, for the code around (they do not leave the app).
    #[serde(skip)]
    pub effect_minor: i64,
    #[serde(skip)]
    pub balance_minor: i64,
    pub entry_id: Option<String>,
    pub transaction_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Statement {
    pub person: Person,
    /// Oldest first.
    pub lines: Vec<Line>,
}

/// The story of a friend's balance: their parts of bills and the entries together, by date, with the balance after each line.
pub fn statement(conn: &Connection, person_id: i64) -> Result<Statement, AppError> {
    let tx = conn.unchecked_transaction()?;
    statement_in(&tx, person_id)
}

/// Without a transaction of its own, for the callers that have one.
pub(crate) fn statement_in(tx: &Connection, person_id: i64) -> Result<Statement, AppError> {
    let person = people::get(tx, person_id)?;
    // (date, source, id, kind, what, signed minor, entry id, transaction id): bills before entries on the same day
    struct Raw {
        date: String,
        source: u8,
        id: i64,
        kind: String,
        what: String,
        signed: i64,
        entry: Option<i64>,
        transaction: Option<i64>,
    }
    let mut raw: Vec<Raw> = Vec::new();
    let mut statement = tx.prepare(
        "SELECT t.payment_date, l.id, t.name, l.amount_minor, t.id
         FROM transaction_split_lines l JOIN transactions t ON t.id = l.transaction_id WHERE l.person_id = ?1",
    )?;
    for row in statement.query_map([person_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?)))? {
        let (date, id, what, amount, transaction) = row?;
        raw.push(Raw { date, source: 0, id, kind: "BILL".into(), what, signed: amount, entry: None, transaction: Some(transaction) });
    }
    let mut statement = tx.prepare("SELECT entry_date, id, kind, note, amount_minor, transaction_id FROM ledger_entries WHERE person_id = ?1")?;
    for row in statement.query_map([person_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, i64>(4)?, r.get::<_, Option<i64>>(5)?))
    })? {
        let (date, id, kind, note, amount, transaction) = row?;
        let sign = Kind::from_sql(&kind)?.sign();
        raw.push(Raw { date, source: 1, id, kind, what: note.unwrap_or_default(), signed: sign * amount, entry: Some(id), transaction });
    }
    raw.sort_by(|a, b| (&a.date, a.source, a.id).cmp(&(&b.date, b.source, b.id)));
    let mut running = 0i64;
    let lines = raw
        .into_iter()
        .map(|r| {
            running += r.signed;
            Line {
                date: r.date,
                kind: r.kind,
                what: r.what,
                amount: format_amount(r.signed.abs()),
                effect: format_amount(r.signed),
                balance: format_amount(running),
                effect_minor: r.signed,
                balance_minor: running,
                entry_id: r.entry.map(|id| id.to_string()),
                transaction_id: r.transaction.map(|id| id.to_string()),
            }
        })
        .collect();
    Ok(Statement { person, lines })
}
