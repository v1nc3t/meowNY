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
         JOIN transaction_costs k ON k.transaction_id = t.id
         LEFT JOIN people p ON p.id = l.person_id
         WHERE {} ORDER BY t.payment_date, t.id, l.id",
        transaction_filter(sel)
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(sel.args()), |r| {
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

/// The history of debts an export holds, oldest first: the lines of the friends and the recurring payments that were paid or are due, in the
/// dates. With a limit, as in the ledger above: the entries only when they were asked for (`entries`), and the parts of bills and the
/// recurring payments only those of what the export holds (a part of a bill belongs to a transaction, a payment is a transaction or a
/// recurring item of a category). A balance is only kept when no transaction is left out of what it adds up.
pub(crate) fn history_lines(conn: &Connection, sel: &Parsed, today: &str, entries: bool) -> Result<Vec<crate::debts::history::HistoryLine>, AppError> {
    use crate::debts::history::{self, PAYMENT_DUE, PAYMENT_PAID};
    let held: std::collections::HashSet<i64> = super::transactions(conn, sel)?.iter().map(|t| t.id).collect();
    let scope = history::Scope { from: sel.from.as_deref(), to: sel.to.as_deref(), person: None };
    let keep_balance = !sel.narrows_transactions();
    let mut out = Vec::new();
    for mut line in history::lines(conn, today, &scope)? {
        let transaction = line.transaction_id.as_deref().and_then(|id| id.parse::<i64>().ok());
        let keep = match line.kind.as_str() {
            "BILL" => transaction.is_some_and(|id| held.contains(&id)),
            PAYMENT_PAID => transaction.is_some_and(|id| held.contains(&id)),
            PAYMENT_DUE => sel.kind != Some(super::Kind::Income) && sel.categories.as_ref().is_none_or(|ids| line.category_id.is_some_and(|id| ids.contains(&id))),
            _ => entries,
        };
        if !keep {
            continue;
        }
        if !keep_balance {
            line.balance = None;
            line.balance_minor = None;
        }
        out.push(line);
        if out.len() > MAX_HISTORY_LINES {
            return Err(AppError::InvalidInput);
        }
    }
    Ok(out)
}

/// A recurring payment that has come due and is not paid or skipped, as a report lists it.
pub(crate) struct DueLine {
    pub(crate) name: String,
    /// The day it fell due.
    pub(crate) date: String,
    /// What it is for; none for a bill whose amount changes every time.
    pub(crate) amount: Option<i64>,
}

/// What a report says about debts, for the months it covers.
pub(crate) struct DebtsPart {
    /// Each friend who owed or was owed something on the last day, the biggest first: what they owed you (negative: you owed them).
    pub(crate) friends: Option<Vec<(String, i64)>>,
    /// The recurring payments that have come due as of today (not as of the months: what is due is a matter of the day).
    pub(crate) due: Option<Vec<DueLine>>,
    /// Every line of the history inside the months, by date.
    pub(crate) history: Option<Vec<crate::debts::history::HistoryLine>>,
    /// Whether the balance after each line may be shown (see `history_lines`).
    pub(crate) show_balance: bool,
}

/// The most lines of the history of debts an export or a report holds.
pub(crate) const MAX_HISTORY_LINES: usize = 20_000;

/// The debts of a report: who owed what on the last day of its months (`friends`), the payments that are due (`due`), and what happened in
/// those months (`history`). All come from the same functions as the debts screens, so the numbers are theirs. The ledger has no category,
/// so the categories of the report do not narrow its entries; they narrow the parts of bills and the payments.
pub(crate) fn report_part(
    conn: &Connection,
    sel: &Parsed,
    first_day: &str,
    last_day: &str,
    wanted: (bool, bool, bool),
    today: &str,
) -> Result<DebtsPart, AppError> {
    let (friends, due, history) = wanted;
    let owing = if friends {
        let mut owing: Vec<(String, i64)> = Vec::new();
        for person in crate::debts::people::list(conn, true)? {
            let statement = crate::debts::ledger::statement_in(conn, person.id)?;
            let balance = statement.lines.iter().rev().find(|l| l.date.as_str() <= last_day).map_or(0, |l| l.balance_minor);
            if balance != 0 {
                owing.push((person.name.clone(), balance));
            }
        }
        owing.sort_by(|a, b| (b.1.abs(), a.0.to_lowercase()).cmp(&(a.1.abs(), b.0.to_lowercase())));
        Some(owing)
    } else {
        None
    };
    let due = if due {
        let items = super::due_payments(conn, sel, today)?;
        Some(items.into_iter().map(|d| DueLine { name: d.name, date: d.due_date, amount: d.amount_minor }).collect())
    } else {
        None
    };
    let history = if history {
        // the dates of the report are its months, not those of `sel` (which has them too, as whole months)
        let _ = (first_day, last_day);
        Some(history_lines(conn, sel, today, true)?)
    } else {
        None
    };
    Ok(DebtsPart { friends: owing, due, history, show_balance: !sel.narrows_transactions() })
}
