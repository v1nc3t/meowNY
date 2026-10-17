//! The debts in an export: the friends, the ledger of money lent and borrowed, and the parts of every shared bill.
//!
//! With no limit (no dates, no categories) everything is in. With a limit the file reveals nothing about the rest:
//!
//! * the parts of the bills that are in the export, and only those;
//! * the friends and guests that those parts name, by name, with no balance and no reminder (a balance would add up things the export left out);
//! * the ledger entries only when the person asked for them (`include_debts`), limited by the dates if there are any. A category does not
//!   limit them: a loan has none.

use std::collections::{BTreeSet, HashMap};

use rusqlite::{params, Connection};

use super::{transaction_filter, Parsed};
use crate::vault::AppError;

pub(crate) struct PersonRow {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) archived: bool,
    /// Only in an export with no limit.
    pub(crate) balance: Option<i64>,
    pub(crate) remind_on: Option<String>,
    pub(crate) remind_every_days: Option<u32>,
}

pub(crate) struct EntryRow {
    pub(crate) id: i64,
    pub(crate) date: String,
    pub(crate) person_id: i64,
    pub(crate) person: String,
    pub(crate) kind: String,
    pub(crate) amount: i64,
    pub(crate) note: Option<String>,
    pub(crate) transaction_id: Option<i64>,
}

pub(crate) struct LineRow {
    pub(crate) transaction_id: i64,
    pub(crate) date: String,
    pub(crate) bill: String,
    pub(crate) person_id: Option<i64>,
    pub(crate) person: Option<String>,
    pub(crate) guest: Option<String>,
    pub(crate) amount: i64,
}

/// The parts of the bills the export contains, oldest bill first.
pub(crate) fn lines(conn: &Connection, sel: &Parsed) -> Result<Vec<LineRow>, AppError> {
    let mut statement = conn.prepare(&format!(
        "SELECT l.transaction_id, t.payment_date, t.name, l.person_id, p.name, l.guest_name, l.amount_minor
         FROM transaction_split_lines l
         JOIN transactions t ON t.id = l.transaction_id
         LEFT JOIN people p ON p.id = l.person_id
         WHERE {} ORDER BY t.payment_date, t.id, l.id",
        transaction_filter(sel)
    ))?;
    let rows = statement.query_map(params![sel.from, sel.to], |r| {
        Ok(LineRow {
            transaction_id: r.get(0)?,
            date: r.get(1)?,
            bill: r.get(2)?,
            person_id: r.get(3)?,
            person: r.get(4)?,
            guest: r.get(5)?,
            amount: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The parts of each bill, by transaction.
pub(crate) fn lines_by_transaction(lines: Vec<LineRow>) -> HashMap<i64, Vec<LineRow>> {
    let mut out: HashMap<i64, Vec<LineRow>> = HashMap::new();
    for line in lines {
        out.entry(line.transaction_id).or_default().push(line);
    }
    out
}

/// The ledger entries the export contains: all of them with no limit, otherwise none unless the person asked, then those inside the dates.
pub(crate) fn entries(conn: &Connection, sel: &Parsed) -> Result<Vec<EntryRow>, AppError> {
    if !(sel.everything() || sel.debts) {
        return Ok(Vec::new());
    }
    let mut statement = conn.prepare(
        "SELECT e.id, e.entry_date, e.person_id, p.name, e.kind, e.amount_minor, e.note, e.transaction_id
         FROM ledger_entries e JOIN people p ON p.id = e.person_id
         WHERE (?1 IS NULL OR e.entry_date >= ?1) AND (?2 IS NULL OR e.entry_date <= ?2)
         ORDER BY e.entry_date, e.id",
    )?;
    let rows = statement.query_map(params![sel.from, sel.to], |r| {
        Ok(EntryRow {
            id: r.get(0)?,
            date: r.get(1)?,
            person_id: r.get(2)?,
            person: r.get(3)?,
            kind: r.get(4)?,
            amount: r.get(5)?,
            note: r.get(6)?,
            transaction_id: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The friends the export contains: everyone with no limit, otherwise those named by the parts and entries it holds.
pub(crate) fn people(conn: &Connection, sel: &Parsed, lines: &[LineRow], entries: &[EntryRow]) -> Result<Vec<PersonRow>, AppError> {
    let everything = sel.everything();
    let wanted: BTreeSet<i64> = lines.iter().filter_map(|l| l.person_id).chain(entries.iter().map(|e| e.person_id)).collect();
    let mut statement = conn.prepare(
        "SELECT p.id, p.name, p.archived, b.balance_minor, p.remind_on, p.remind_every_days
         FROM people p JOIN person_balances b ON b.person_id = p.id WHERE p.merged_into IS NULL ORDER BY p.id",
    )?;
    let rows = statement.query_map([], |r| {
        Ok(PersonRow {
            id: r.get(0)?,
            name: r.get(1)?,
            archived: r.get::<_, i64>(2)? == 1,
            balance: everything.then(|| r.get::<_, i64>(3)).transpose()?,
            remind_on: if everything { r.get(4)? } else { None },
            remind_every_days: if everything { r.get(5)? } else { None },
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        let row = row?;
        if everything || wanted.contains(&row.id) {
            out.push(row);
        }
    }
    Ok(out)
}
