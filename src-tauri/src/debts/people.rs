//! The friends of the ledger. A friend is a name, and optionally a reminder; nothing else about a person is stored (no phone number, email or
//! address, no import of contacts). A friend is archived, never deleted: the balance and the history stay, and the name is free to use again.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Deserializer, Serialize};

use crate::categories::{clean_name, serialize_id};
use crate::money::format_amount;
use crate::transactions::clean_date;
use crate::vault::AppError;

const MAX_REPEAT_DAYS: u32 = 365;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    pub name: String,
    pub archived: bool,
    /// The day of the reminder, or the first one when it repeats.
    pub remind_on: Option<String>,
    pub remind_every_days: Option<u32>,
    /// What they owe you, in the app's currency: positive when they owe you, negative when you owe them.
    pub balance: String,
    /// The same in minor units, for the code around (it does not leave the app).
    #[serde(skip)]
    pub balance_minor: i64,
}

const COLUMNS: &str = "p.id, p.name, p.archived, p.remind_on, p.remind_every_days, b.balance_minor";

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Person> {
    Ok(Person {
        id: row.get(0)?,
        name: row.get(1)?,
        archived: row.get::<_, i64>(2)? == 1,
        remind_on: row.get(3)?,
        remind_every_days: row.get(4)?,
        balance: format_amount(row.get(5)?),
        balance_minor: row.get(5)?,
    })
}

fn is_name_taken(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(e, _) if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE)
}

fn write_error(error: rusqlite::Error) -> AppError {
    if is_name_taken(&error) {
        AppError::NameExists
    } else {
        AppError::from(error)
    }
}

/// A friend who was merged into another (two devices each added the same friend, and the sync made them one) is not a friend any more:
/// everything that was theirs belongs to the other, and they are neither listed nor found.
pub fn find(conn: &Connection, id: i64) -> Result<Option<Person>, AppError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM people p JOIN person_balances b ON b.person_id = p.id WHERE p.id = ?1 AND p.merged_into IS NULL"),
            [id],
            from_row,
        )
        .optional()?)
}

pub fn get(conn: &Connection, id: i64) -> Result<Person, AppError> {
    find(conn, id)?.ok_or(AppError::NotFound)
}

/// The friends, the ones who are not archived first, then the largest balance (in either direction) first. Archived friends only on request.
pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Person>, AppError> {
    let mut statement = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM people p JOIN person_balances b ON b.person_id = p.id
         WHERE (?1 OR p.archived = 0) AND p.merged_into IS NULL ORDER BY p.archived, abs(b.balance_minor) DESC, p.name COLLATE NOCASE, p.id"
    ))?;
    let rows = statement.query_map([include_archived], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Fields left out stay as they are. `remindOn: null` clears the reminder (and its repeat with it); `remindEveryDays: null` makes it once.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub remind_on: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub remind_every_days: Option<Option<u32>>,
}

fn present_or_null<'de, D: Deserializer<'de>, T: Deserialize<'de>>(deserializer: D) -> Result<Option<Option<T>>, D::Error> {
    Ok(Some(Option::deserialize(deserializer)?))
}

/// Renames a friend and/or changes the reminder, all or nothing.
pub fn update(conn: &Connection, id: i64, patch: &PersonPatch) -> Result<Person, AppError> {
    let tx = conn.unchecked_transaction()?;
    let current = get(&tx, id)?;
    if let Some(name) = &patch.name {
        rename(&tx, id, name)?;
    }
    if patch.remind_on.is_some() || patch.remind_every_days.is_some() {
        let on = patch.remind_on.clone().unwrap_or(current.remind_on);
        // clearing the date clears the repeat with it; a repeat given for a friend with no date is refused
        let every = if patch.remind_on == Some(None) && patch.remind_every_days.is_none() { None } else { patch.remind_every_days.unwrap_or(current.remind_every_days) };
        set_reminder(&tx, id, on.as_deref(), every)?;
    }
    let person = get(&tx, id)?;
    tx.commit()?;
    Ok(person)
}

pub fn create(conn: &Connection, name: &str) -> Result<Person, AppError> {
    let name = clean_name(name)?;
    conn.execute("INSERT INTO people (name) VALUES (?1)", [&name]).map_err(write_error)?;
    get(conn, conn.last_insert_rowid())
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<Person, AppError> {
    let name = clean_name(name)?;
    get(conn, id)?;
    conn.execute("UPDATE people SET name = ?1 WHERE id = ?2", params![name, id]).map_err(write_error)?;
    get(conn, id)
}

/// A reminder is a date and, optionally, a number of days after which it repeats. `None` for the date clears both.
pub fn set_reminder(conn: &Connection, id: i64, on: Option<&str>, every_days: Option<u32>) -> Result<Person, AppError> {
    get(conn, id)?;
    let on = on.map(clean_date).transpose()?;
    if every_days.is_some_and(|n| n == 0 || n > MAX_REPEAT_DAYS) || (on.is_none() && every_days.is_some()) {
        return Err(AppError::InvalidInput);
    }
    conn.execute("UPDATE people SET remind_on = ?1, remind_every_days = ?2 WHERE id = ?3", params![on, every_days, id])?;
    get(conn, id)
}

/// Hides a friend from the lists and pickers. Their balance, their history and their reminder stay.
pub fn archive(conn: &Connection, id: i64) -> Result<Person, AppError> {
    get(conn, id)?;
    conn.execute("UPDATE people SET archived = 1 WHERE id = ?1", [id])?;
    get(conn, id)
}

/// Brings an archived friend back. The name must not have been taken by another friend in the meantime.
pub fn restore(conn: &Connection, id: i64) -> Result<Person, AppError> {
    get(conn, id)?;
    conn.execute("UPDATE people SET archived = 0 WHERE id = ?1", [id]).map_err(write_error)?;
    get(conn, id)
}
