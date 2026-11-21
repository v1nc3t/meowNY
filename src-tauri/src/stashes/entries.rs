//! The entries of a stash: money put in (an add, above 0) and taken out (a take, below 0) (`docs/stash-design.md`, section 5.2). What a stash
//! holds is the sum of its entries. Every local write that changes it is checked by the delta rule (section 3.1), in the one database
//! transaction that makes it, so a refusal after a partial write (the transfer that came with an add) leaves nothing.
//!
//! A deposit has more to say about money (`docs/loans-deposits-design.md`, section 5.3), on top of the delta rule: money goes in only while it runs; while it
//! runs it is emptied whole by a take, which breaks it; once it is broken only the notes of its entries change.

use std::cmp::Ordering;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{delta_allowed, facts, free, DepositState, Detail, Facts};
use crate::accounts::transfers::{self, NewTransfer};
use crate::categories::{parse_id, present_or_null, serialize_id};
use crate::debts::ledger::clean_note;
use crate::money::{format_amount, parse_positive_amount};
use crate::transactions::clean_date;
use crate::vault::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Kind {
    Add,
    Take,
}

fn serialize_optional_id<S: serde::Serializer>(id: &Option<i64>, serializer: S) -> Result<S::Ok, S::Error> {
    match id {
        Some(id) => serializer.serialize_str(&id.to_string()),
        None => serializer.serialize_none(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    #[serde(serialize_with = "serialize_id")]
    pub stash_id: i64,
    pub kind: Kind,
    /// Decimal string, always above 0: the kind says which way it went.
    pub amount: String,
    pub date: String,
    pub note: Option<String>,
    /// The transfer that came with the entry. It is cleared when the transfer is deleted.
    #[serde(serialize_with = "serialize_optional_id")]
    pub transfer_id: Option<i64>,
    /// The other side of the transfer: the account the money came from for an add, the one it went to for a take.
    #[serde(serialize_with = "serialize_optional_id")]
    pub other_account_id: Option<i64>,
    pub other_account: Option<String>,
    /// What the stash held after this entry, counting the entries dated before it and on its day with a smaller id.
    pub saved_after: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewEntry {
    /// "0.01" to "9999999999.99".
    pub amount: String,
    /// Left out means today. Not after today.
    #[serde(default)]
    pub date: Option<String>,
    /// At most 200 characters, line breaks allowed. Empty means none.
    #[serde(default)]
    pub note: Option<String>,
    /// Bring the money from this account: a transfer into the stash's account is made with the entry.
    #[serde(default)]
    pub from_account_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewTake {
    pub amount: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// Send the money on to this account: a transfer out of the stash's account is made with the entry.
    #[serde(default)]
    pub to_account_id: Option<String>,
}

/// Fields left out stay as they are. `note: null` clears it. The amount is a size: an add stays an add and a take stays a take.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryPatch {
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub note: Option<Option<String>>,
}

/// The entries of a stash, newest first (by date, then id). `saved_after` is what the stash held after each, running oldest first.
pub fn list(conn: &Connection, stash_id: i64) -> Result<Vec<Entry>, AppError> {
    let mut statement = conn.prepare_cached(
        "SELECT e.id, e.stash_id, e.amount_minor, e.entry_date, e.note, e.transfer_id, o.id, o.name
         FROM stash_entries e
         LEFT JOIN transfers x ON x.id = e.transfer_id
         LEFT JOIN accounts o ON o.id = CASE WHEN e.amount_minor > 0 THEN x.from_account_id ELSE x.to_account_id END
         WHERE e.stash_id = ?1 ORDER BY e.entry_date, e.id",
    )?;
    let rows = statement.query_map([stash_id], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
    })?;
    let mut saved = 0i64;
    let mut entries = Vec::new();
    for row in rows {
        let (id, stash_id, minor, date, note, transfer_id, other_account_id, other_account) = row?;
        saved += minor;
        entries.push(Entry {
            id,
            stash_id,
            kind: if minor > 0 { Kind::Add } else { Kind::Take },
            amount: format_amount(minor.abs()),
            date,
            note,
            transfer_id,
            other_account_id,
            other_account,
            saved_after: format_amount(saved),
        });
    }
    entries.reverse();
    Ok(entries)
}

/// An entry as it is stored: its stash, what it moved (signed), its date and note, and the transfer that came with it.
struct Stored {
    stash_id: i64,
    minor: i64,
    date: String,
    note: Option<String>,
    transfer_id: Option<i64>,
}

fn stored(conn: &Connection, id: i64) -> Result<Stored, AppError> {
    conn.query_row("SELECT stash_id, amount_minor, entry_date, note, transfer_id FROM stash_entries WHERE id = ?1", [id], |r| {
        Ok(Stored { stash_id: r.get(0)?, minor: r.get(1)?, date: r.get(2)?, note: r.get(3)?, transfer_id: r.get(4)? })
    })
    .optional()?
    .ok_or(AppError::NotFound)
}

/// The date of an entry: not after today (section 5.2). Left out, it is today.
fn entry_date(date: Option<&str>, today: &str) -> Result<String, AppError> {
    let date = match date {
        Some(date) => clean_date(date)?,
        None => today.to_string(),
    };
    if date.as_str() > today {
        return Err(AppError::InvalidInput);
    }
    Ok(date)
}

/// What a deposit lets happen to what it holds (section 5.3): money goes in only while it runs, and an edit or a delete takes money out of it only
/// when it does not run (while it runs, money comes out by a take of everything, which `take` checks itself). A goal is not asked, and a write
/// that changes nothing is always fine.
fn deposit_allows(stash: &Facts, after: i64, today: &str) -> bool {
    match (stash.state(today), after.cmp(&stash.saved)) {
        (None, _) | (_, Ordering::Equal) => true,
        (Some(state), Ordering::Greater) => state == DepositState::Running,
        (Some(state), Ordering::Less) => state != DepositState::Running,
    }
}

/// The delta rule (section 3.1) for a write that takes the stash from what it holds now to `after`. The free money of its account is read now,
/// so an add that comes with a transfer is checked after the transfer is made.
fn check_delta(conn: &Connection, stash: &Facts, after: i64, today: &str) -> Result<(), AppError> {
    let free = if after > stash.saved { free(conn, stash.account_id, today)? } else { 0 };
    if delta_allowed(stash.saved, after, free, stash.target) {
        Ok(())
    } else {
        Err(AppError::InvalidInput)
    }
}

fn insert(conn: &Connection, stash_id: i64, minor: i64, date: &str, note: Option<&str>, transfer_id: Option<i64>) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO stash_entries (stash_id, amount_minor, entry_date, note, transfer_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![stash_id, minor, date, note, transfer_id],
    )?;
    Ok(())
}

/// Adds money to a stash that is not archived, in an account that is not. With `from_account_id`, a transfer from that account into the stash's
/// is made first, in the same transaction, and the entry remembers it; the free money is then counted with the transfer in it. A deposit takes
/// money only while it runs.
pub fn add(conn: &Connection, stash_id: i64, input: &NewEntry, today: &str) -> Result<Detail, AppError> {
    let tx = conn.unchecked_transaction()?;
    add_in(&tx, stash_id, input, today, false)?;
    let detail = super::get(&tx, stash_id, today)?;
    tx.commit()?;
    Ok(detail)
}

/// `add` inside a database transaction that the caller owns. `returning` skips the one rule that a deposit takes money only while it runs: the
/// return of a matured deposit is put in it (`deposits::book_return`); every other rule is asked.
pub(super) fn add_in(tx: &Connection, stash_id: i64, input: &NewEntry, today: &str, returning: bool) -> Result<(), AppError> {
    let minor = parse_positive_amount(&input.amount)?;
    let today = clean_date(today)?;
    let date = entry_date(input.date.as_deref(), &today)?;
    let note = clean_note(input.note.as_deref())?;
    let from = input.from_account_id.as_deref().map(parse_id).transpose()?;
    let stash = facts(tx, stash_id)?;
    if stash.archived || stash.account_archived || !(returning || deposit_allows(&stash, stash.saved + minor, &today)) {
        return Err(AppError::InvalidInput);
    }
    let transfer_id = match from {
        Some(from) => {
            let input = NewTransfer {
                from_account_id: from.to_string(),
                to_account_id: stash.account_id.to_string(),
                amount: input.amount.clone(),
                date: date.clone(),
                note: note.clone(),
                to_amount: None,
            };
            Some(transfers::create(tx, &input)?.id)
        }
        None => None,
    };
    check_delta(tx, &stash, stash.saved + minor, &today)?;
    insert(tx, stash_id, minor, &date, note.as_deref(), transfer_id)
}

/// Takes money out of a stash, archived or not, in an account that may be archived: no more than it holds. With `to_account_id`, a transfer
/// from the stash's account to that one is made with it, in the same transaction. A deposit that runs is emptied whole or not at all, and that
/// breaks it: `broken_on` is the day of the entry.
pub fn take(conn: &Connection, stash_id: i64, input: &NewTake, today: &str) -> Result<Detail, AppError> {
    let minor = parse_positive_amount(&input.amount)?;
    let today = clean_date(today)?;
    let date = entry_date(input.date.as_deref(), &today)?;
    let note = clean_note(input.note.as_deref())?;
    let to = input.to_account_id.as_deref().map(parse_id).transpose()?;
    let tx = conn.unchecked_transaction()?;
    let stash = facts(&tx, stash_id)?;
    let breaking = stash.state(&today) == Some(DepositState::Running);
    if breaking && minor != stash.saved {
        return Err(AppError::InvalidInput);
    }
    check_delta(&tx, &stash, stash.saved - minor, &today)?;
    let transfer_id = match to {
        Some(to) => {
            let input = NewTransfer {
                from_account_id: stash.account_id.to_string(),
                to_account_id: to.to_string(),
                amount: input.amount.clone(),
                date: date.clone(),
                note: note.clone(),
                to_amount: None,
            };
            // the account the money leaves may be archived: it can still be emptied
            Some(transfers::create_with(&tx, &input, true)?.id)
        }
        None => None,
    };
    insert(&tx, stash_id, -minor, &date, note.as_deref(), transfer_id)?;
    if breaking {
        tx.execute("UPDATE stashes SET broken_on = ?1 WHERE id = ?2", params![date, stash_id])?;
    }
    let detail = super::get(&tx, stash_id, &today)?;
    tx.commit()?;
    Ok(detail)
}

/// Changes the amount, the date or the note of an entry. The amount is a size: the kind never changes. The stamp moves only when something
/// changed. The transfer that came with the entry is not changed. Money going in (`after > before`) is refused for a stash that is archived
/// and for one whose account is. Of a deposit that is broken only the note changes; while it runs an edit may not take money out, and once it
/// has matured it may not put money in.
pub fn update(conn: &Connection, entry_id: i64, patch: &EntryPatch, today: &str) -> Result<Detail, AppError> {
    let today = clean_date(today)?;
    let tx = conn.unchecked_transaction()?;
    let current = stored(&tx, entry_id)?;
    let size = match patch.amount.as_deref() {
        Some(text) => parse_positive_amount(text)?,
        None => current.minor.abs(),
    };
    let date = match patch.date.as_deref() {
        Some(date) => entry_date(Some(date), &today)?,
        None => current.date.clone(),
    };
    let note = match &patch.note {
        Some(note) => clean_note(note.as_deref())?,
        None => current.note.clone(),
    };
    let stash = facts(&tx, current.stash_id)?;
    let new = size * current.minor.signum();
    let after = stash.saved - current.minor + new;
    if after > stash.saved && (stash.archived || stash.account_archived) {
        return Err(AppError::InvalidInput);
    }
    let broken = stash.state(&today) == Some(DepositState::Broken);
    if (broken && (size != current.minor.abs() || date != current.date)) || !deposit_allows(&stash, after, &today) {
        return Err(AppError::InvalidInput);
    }
    check_delta(&tx, &stash, after, &today)?;
    tx.execute("UPDATE stash_entries SET amount_minor = ?1, entry_date = ?2, note = ?3 WHERE id = ?4", params![new, date, note, entry_id])?;
    let detail = super::get(&tx, current.stash_id, &today)?;
    tx.commit()?;
    Ok(detail)
}

/// Deletes an entry. With `delete_transfer` and a transfer that came with it, that transfer is deleted too, in the same transaction (it leaves
/// its own marker); without, it stays. Allowed for a stash that is archived or in an archived account, within the delta rule. Refused for a
/// deposit that is broken; for one that runs when it takes money out (an add), and for one that has matured when it puts money in (a take).
pub fn delete(conn: &Connection, entry_id: i64, delete_transfer: bool, today: &str) -> Result<Detail, AppError> {
    let today = clean_date(today)?;
    let tx = conn.unchecked_transaction()?;
    let current = stored(&tx, entry_id)?;
    let stash = facts(&tx, current.stash_id)?;
    if stash.state(&today) == Some(DepositState::Broken) || !deposit_allows(&stash, stash.saved - current.minor, &today) {
        return Err(AppError::InvalidInput);
    }
    check_delta(&tx, &stash, stash.saved - current.minor, &today)?;
    tx.execute("DELETE FROM stash_entries WHERE id = ?1", [entry_id])?;
    if let (true, Some(transfer_id)) = (delete_transfer, current.transfer_id) {
        transfers::delete(&tx, transfer_id)?;
    }
    let detail = super::get(&tx, current.stash_id, &today)?;
    tx.commit()?;
    Ok(detail)
}
