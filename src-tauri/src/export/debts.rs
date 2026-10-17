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

/// A line of the history of debts in a report: a part of a bill or an entry of the ledger, of one friend.
pub(crate) struct HistoryLine {
    pub(crate) date: String,
    pub(crate) friend: String,
    /// `BILL` for a part of a shared bill, otherwise the kind of the entry.
    pub(crate) kind: String,
    /// The name of the bill, or the note of the entry.
    pub(crate) what: String,
    /// What it did to what the friend owes you: positive when they owe you more.
    pub(crate) effect: i64,
    /// What the friend owed you after the line, counting every line there is.
    pub(crate) balance: i64,
}

/// A recurring payment that has come due and is not paid or skipped, as a report lists it with the debts.
pub(crate) struct DueLine {
    pub(crate) name: String,
    /// The day it fell due.
    pub(crate) date: String,
    /// What it is for; none for a bill whose amount changes every time.
    pub(crate) amount: Option<i64>,
}

/// What a report says about debts, for the months it covers.
pub(crate) struct DebtsPart {
    /// The recurring payments that have come due as of today (not as of the months: what is due is a matter of the day), listed with the
    /// friends.
    pub(crate) due: Option<Vec<DueLine>>,
    /// Each friend who owed or was owed something on the last day, the biggest first: what they owed you (negative: you owed them).
    pub(crate) friends: Option<Vec<(String, i64)>>,
    /// Every line of every friend inside the months, by date.
    pub(crate) history: Option<Vec<HistoryLine>>,
    /// Whether the balance after each line may be shown. With some categories chosen it is not: the balance adds up bills that the report
    /// leaves out, and the parts of those bills are not in the history either.
    pub(crate) show_balance: bool,
}

/// The most lines of the history of debts a report holds.
pub(crate) const MAX_HISTORY_LINES: usize = 20_000;

/// The debts of a report: who owed what on the last day of its months (`friends`), and what happened in those months (`history`). Both come
/// from the statements of the friends, so they are the numbers of the debts screens. The ledger has no category, so the categories of the
/// report do not narrow its entries; they narrow the parts of bills, which belong to a transaction of a category.
pub(crate) fn report_part(
    conn: &Connection,
    sel: &Parsed,
    first_day: &str,
    last_day: &str,
    friends: bool,
    history: bool,
    today: &str,
) -> Result<DebtsPart, AppError> {
    let held: std::collections::HashSet<i64> = super::transactions(conn, sel)?.iter().map(|t| t.id).collect();
    let mut owing: Vec<(String, i64)> = Vec::new();
    let mut lines: Vec<(String, usize, HistoryLine)> = Vec::new();
    for person in crate::debts::people::list(conn, true)? {
        let statement = crate::debts::ledger::statement_in(conn, person.id)?;
        if friends {
            let balance = statement.lines.iter().rev().find(|l| l.date.as_str() <= last_day).map_or(0, |l| l.balance_minor);
            if balance != 0 {
                owing.push((person.name.clone(), balance));
            }
        }
        if history {
            for line in statement.lines.iter().filter(|l| l.date.as_str() >= first_day && l.date.as_str() <= last_day) {
                let in_report = line.kind != "BILL" || line.transaction_id.as_deref().and_then(|id| id.parse::<i64>().ok()).is_some_and(|id| held.contains(&id));
                if !in_report {
                    continue;
                }
                let at = lines.len();
                lines.push((
                    person.name.clone(),
                    at,
                    HistoryLine { date: line.date.clone(), friend: person.name.clone(), kind: line.kind.clone(), what: line.what.clone(), effect: line.effect_minor, balance: line.balance_minor },
                ));
                if lines.len() > MAX_HISTORY_LINES {
                    return Err(AppError::InvalidInput);
                }
            }
        }
    }
    owing.sort_by(|a, b| (b.1.abs(), a.0.to_lowercase()).cmp(&(a.1.abs(), b.0.to_lowercase())));
    lines.sort_by(|a, b| (&a.2.date, a.0.to_lowercase(), a.1).cmp(&(&b.2.date, b.0.to_lowercase(), b.1)));
    let due = if friends {
        let items = super::due_payments(conn, sel, today)?;
        Some(items.into_iter().map(|item| DueLine { name: item.name, date: item.next_due_date, amount: item.amount.as_deref().and_then(|a| crate::money::parse_amount(a).ok()) }).collect())
    } else {
        None
    };
    Ok(DebtsPart { due, friends: friends.then_some(owing), history: history.then(|| lines.into_iter().map(|l| l.2).collect()), show_balance: sel.categories.is_none() })
}
