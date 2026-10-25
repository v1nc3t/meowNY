//! The recurring payments that have come due: money you owe by now, kept with the debts.
//!
//! A recurring item has one **next** occurrence, the oldest one that has not been paid or skipped. Every occurrence from that one up to
//! **yesterday** is overdue unless it was paid or skipped on its own, and each is a payment of its own: a rent that was not paid for three
//! months is three. A payment that falls due today is not yet a debt: it becomes one the day after (the recurring screen shows it
//! as due today, in red, from its day). Nothing is stored for them: they are worked out from the items, so they cannot be out of step with the
//! recurring screen. Paying or skipping one removes it here; any occurrence can be paid or skipped on its own. Only payments (items of an expense category) are listed; a recurring income is
//! not something you owe.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use serde::Serialize;

use crate::categories::serialize_id;
use crate::money::{format_amount, parse_amount};
use crate::recurring;
use crate::transactions::clean_date;
use crate::vault::AppError;

/// The most occurrences of one item that are listed. An item that has been forgotten for years does not fill the screen.
pub const MAX_PER_ITEM: usize = 24;

/// One occurrence of a recurring payment that has come due and is not paid or skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuePayment {
    /// The recurring item it belongs to.
    #[serde(serialize_with = "serialize_id")]
    pub recurring_id: i64,
    #[serde(serialize_with = "serialize_id")]
    pub category_id: i64,
    pub name: String,
    pub category: String,
    /// The day it fell due.
    pub due_date: String,
    /// What it is for; none for a bill whose amount changes every time.
    pub amount: Option<String>,
    /// Whole days since the day it fell due; at least 1, since a payment is a debt only after its day.
    pub days_overdue: u32,
    /// The same in minor units, for the code around (it does not leave the app).
    #[serde(skip)]
    pub amount_minor: Option<i64>,
}

/// Every occurrence whose day is **before** `today` (a date in the person's timezone), the oldest first, each item at most `MAX_PER_ITEM`
/// times (not counting those paid or skipped). Items that are paused or ended are not due, and neither are recurring incomes.
pub fn due_payments(conn: &rusqlite::Connection, today: &str) -> Result<Vec<DuePayment>, AppError> {
    let today_date: NaiveDate = clean_date(today)?.parse().map_err(|_| AppError::Internal)?;
    let mut statement = conn.prepare("SELECT id, name, type FROM categories")?;
    let categories: HashMap<i64, (String, bool)> = statement
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get::<_, String>(1)?, r.get::<_, String>(2)? == "EXPENSE"))))?
        .collect::<Result<_, _>>()?;
    let mut paid: HashSet<(i64, String)> = HashSet::new();
    let mut statement = conn.prepare("SELECT recurring_transaction_id, recurring_due_date FROM transactions WHERE recurring_due_date IS NOT NULL AND recurring_transaction_id IS NOT NULL")?;
    for row in statement.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        paid.insert(row?);
    }
    let skipped = recurring::skipped_by_item(conn)?;
    let mut out = Vec::new();
    for item in recurring::list(conn)? {
        let Some((category, true)) = categories.get(&item.category_id).cloned() else { continue };
        if !item.is_active {
            continue;
        }
        let start: NaiveDate = item.start_date.parse().map_err(|_| AppError::Internal)?;
        let mut due: NaiveDate = item.next_due_date.parse().map_err(|_| AppError::Internal)?;
        let amount_minor = item.amount.as_deref().map(parse_amount).transpose()?;
        let mut listed = 0;
        while listed < MAX_PER_ITEM {
            if due >= today_date || item.end_date.as_deref().is_some_and(|end| due.to_string().as_str() > end) {
                break;
            }
            let text = due.to_string();
            let open = !paid.contains(&(item.id, text.clone())) && !skipped.get(&item.id).is_some_and(|dates| dates.contains(&text));
            if open {
                listed += 1;
                out.push(DuePayment {
                    recurring_id: item.id,
                    category_id: item.category_id,
                    name: item.name.clone(),
                    category: category.clone(),
                    due_date: text,
                    amount: amount_minor.map(format_amount),
                    days_overdue: (today_date - due).num_days() as u32,
                    amount_minor,
                });
            }
            match recurring::next_after(start, item.interval_unit, item.interval_count, due) {
                Some(next) => due = next,
                None => break,
            }
        }
    }
    out.sort_by(|a, b| (&a.due_date, a.recurring_id).cmp(&(&b.due_date, b.recurring_id)));
    Ok(out)
}

/// What the due payments come to, for those that say what they are for, and how many do not.
pub fn total(payments: &[DuePayment]) -> (i64, usize) {
    (payments.iter().filter_map(|p| p.amount_minor).sum(), payments.iter().filter(|p| p.amount_minor.is_none()).count())
}

/// The due payments for a screen: the list and what they come to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Due {
    pub payments: Vec<DuePayment>,
    /// What the payments that say what they are for come to.
    pub total: String,
    /// How many of the payments do not say (a bill whose amount changes every time).
    pub unknown: u32,
}

pub fn due(conn: &rusqlite::Connection, today: &str) -> Result<Due, AppError> {
    let payments = due_payments(conn, today)?;
    let (total, unknown) = total(&payments);
    Ok(Due { payments, total: format_amount(total), unknown: unknown as u32 })
}
