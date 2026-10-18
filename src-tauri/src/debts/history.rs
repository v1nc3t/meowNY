//! The history of debts: everything that changed what you owe or are owed, in one list.
//!
//! * the lines of every friend (money lent, borrowed and paid back, and their parts of shared bills), with the balance after each;
//! * the recurring payments that were paid **late** (a transaction made from a recurring item and paid after the day it fell due, on the day
//!   it was paid, with the day it fell due); one paid on its day or before it was never a debt, so it is not here;
//! * the recurring payments that are **overdue** now (`dues`), on the day each fell due.
//!
//! Nothing here is stored: the lines are read from the statements of the friends, from the transactions and from the recurring items, so
//! the history cannot disagree with them.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::{dues, ledger, people};
use crate::categories::parse_id;
use crate::money::format_amount;
use crate::transactions::clean_date;
use crate::vault::AppError;

pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 200;

/// The kinds of line besides those of a friend's statement (`LENT`, `BORROWED`, `REPAID_TO_ME`, `REPAID_BY_ME`, `BILL`): a recurring payment
/// that was paid late, and one that is overdue.
pub const PAYMENT_PAID: &str = "PAYMENT_PAID";
pub const PAYMENT_DUE: &str = "PAYMENT_DUE";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryLine {
    /// The day it happened: for a recurring payment that was paid, the day it was paid; for one that is due, the day it fell due.
    pub date: String,
    pub kind: String,
    /// The friend, for the lines of a friend.
    pub friend: Option<String>,
    pub person_id: Option<String>,
    /// The name of the bill, the note of the entry, or the name of the recurring payment.
    pub what: String,
    /// Always positive: what was lent, borrowed, paid back, shared or paid.
    pub amount: String,
    /// What it did to what the friend owes you (positive: they owe you more). Only for the lines of a friend.
    pub effect: Option<String>,
    /// What the friend owed you after the line. Only for the lines of a friend.
    pub balance: Option<String>,
    /// The day a recurring payment fell due. Only for the lines of a recurring payment.
    pub due_date: Option<String>,
    pub entry_id: Option<String>,
    pub transaction_id: Option<String>,
    pub recurring_id: Option<String>,
    /// The same in minor units, and the category of a recurring payment, for the code around (they do not leave the app).
    #[serde(skip)]
    pub amount_minor: i64,
    #[serde(skip)]
    pub effect_minor: Option<i64>,
    #[serde(skip)]
    pub balance_minor: Option<i64>,
    #[serde(skip)]
    pub category_id: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Filter {
    /// Inclusive, "YYYY-MM-DD".
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// Only the lines of this friend (and none of the recurring payments).
    #[serde(default)]
    pub person_id: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    /// Newest first.
    pub lines: Vec<HistoryLine>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

/// What `lines` reads.
#[derive(Default)]
pub struct Scope<'a> {
    pub from: Option<&'a str>,
    pub to: Option<&'a str>,
    pub person: Option<i64>,
}

/// Every line in the dates, oldest first (the order lines come in a file; the screen turns it round). Lines of friends come from
/// `ledger::statement_in`, so their balances are the debts screens' own. A friend's balance counts every line, whatever the dates.
pub fn lines(conn: &Connection, today: &str, scope: &Scope) -> Result<Vec<HistoryLine>, AppError> {
    let inside = |date: &str| scope.from.is_none_or(|from| date >= from) && scope.to.is_none_or(|to| date <= to);
    let mut out: Vec<(String, u8, usize, HistoryLine)> = Vec::new();
    let mut push = |source: u8, line: HistoryLine| {
        let at = out.len();
        out.push((line.date.clone(), source, at, line));
    };
    for person in people::list(conn, true)?.into_iter().filter(|p| scope.person.is_none_or(|id| id == p.id)) {
        for line in ledger::statement_in(conn, person.id)?.lines.into_iter().filter(|l| inside(&l.date)) {
            push(
                0,
                HistoryLine {
                    date: line.date,
                    kind: line.kind,
                    friend: Some(person.name.clone()),
                    person_id: Some(person.id.to_string()),
                    what: line.what,
                    amount: line.amount,
                    amount_minor: line.effect_minor.abs(),
                    effect: Some(line.effect),
                    effect_minor: Some(line.effect_minor),
                    balance: Some(line.balance),
                    balance_minor: Some(line.balance_minor),
                    due_date: None,
                    entry_id: line.entry_id,
                    transaction_id: line.transaction_id,
                    recurring_id: None,
                    category_id: None,
                },
            );
        }
    }
    if scope.person.is_none() {
        let mut statement = conn.prepare(
            "SELECT payment_date, name, amount_minor, recurring_due_date, id, recurring_transaction_id, category_id
             FROM transactions WHERE recurring_transaction_id IS NOT NULL",
        )?;
        let paid = statement
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?, r.get::<_, i64>(6)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        // only those paid after their day: one paid on time was never overdue, and one with no day recorded cannot be told to have been
        for (date, name, amount, due, transaction, recurring, category) in paid.into_iter().filter(|p| inside(&p.0) && p.3.as_deref().is_some_and(|day| p.0.as_str() > day)) {
            push(
                1,
                HistoryLine {
                    date,
                    kind: PAYMENT_PAID.to_string(),
                    friend: None,
                    person_id: None,
                    what: name,
                    amount: format_amount(amount),
                    amount_minor: amount,
                    effect: None,
                    effect_minor: None,
                    balance: None,
                    balance_minor: None,
                    due_date: due,
                    entry_id: None,
                    transaction_id: Some(transaction.to_string()),
                    recurring_id: Some(recurring.to_string()),
                    category_id: Some(category),
                },
            );
        }
        for due in dues::due_payments(conn, today)?.into_iter().filter(|d| inside(&d.due_date)) {
            push(
                2,
                HistoryLine {
                    date: due.due_date.clone(),
                    kind: PAYMENT_DUE.to_string(),
                    friend: None,
                    person_id: None,
                    what: due.name,
                    // a bill with no fixed amount has nothing to say: 0.00, and the screen shows it as unknown
                    amount: format_amount(due.amount_minor.unwrap_or(0)),
                    amount_minor: due.amount_minor.unwrap_or(0),
                    effect: None,
                    effect_minor: None,
                    balance: None,
                    balance_minor: None,
                    due_date: Some(due.due_date),
                    entry_id: None,
                    transaction_id: None,
                    recurring_id: Some(due.recurring_id.to_string()),
                    category_id: Some(due.category_id),
                },
            );
        }
    }
    out.sort_by(|a, b| (&a.0, a.1, a.2).cmp(&(&b.0, b.1, b.2)));
    Ok(out.into_iter().map(|l| l.3).collect())
}

/// The history for a screen: newest first, a page at a time.
pub fn history(conn: &Connection, today: &str, filter: &Filter) -> Result<History, AppError> {
    let from = filter.from.as_deref().map(clean_date).transpose()?;
    let to = filter.to.as_deref().map(clean_date).transpose()?;
    if matches!((&from, &to), (Some(f), Some(t)) if f > t) {
        return Err(AppError::InvalidInput);
    }
    let person = filter.person_id.as_deref().map(parse_id).transpose()?;
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(AppError::InvalidInput);
    }
    let tx = conn.unchecked_transaction()?;
    let mut all = lines(&tx, today, &Scope { from: from.as_deref(), to: to.as_deref(), person })?;
    all.reverse();
    let total = all.len() as u32;
    let offset = filter.offset.unwrap_or(0);
    let lines = all.into_iter().skip(offset as usize).take(limit as usize).collect();
    Ok(History { lines, total, limit, offset })
}
