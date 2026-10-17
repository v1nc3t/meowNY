//! Reminders about debts (`docs/debts-design.md`, section 11). One setting for all of them: **every so many days**. Every friend whose balance
//! is not 0 is reminded that many days after the day of the latest line of their statement (the last time the debt changed), and again
//! after the same number of days until the debt is settled or changes (a new line starts the count again). Nothing is stored about a
//! reminder and nothing is written when one comes due, so two devices never fight over one: each device has its own period, keeps for
//! itself which day it has shown, and rings at its own time of day. This module only says which days those are.
//!
//! (The `remind_on` and `remind_every_days` of a friend are not read here any more: they stay in the data, the sync, the backups and the
//! command API so that nothing written before is lost, and nothing rings from them.)

use chrono::{Duration, NaiveDate};
use serde::Serialize;
use std::collections::BTreeMap;

use rusqlite::Connection;

use super::ledger;
use crate::vault::AppError;

/// The longest period a device may ask for.
pub const MAX_EVERY_DAYS: u32 = 365;

/// How many days a phone schedules ahead: the next few distinct days with a reminder.
pub const SCHEDULED_DAYS: usize = 4;

fn day(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

fn text(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

/// The last day of the reminder that is not after `today`: its first day, then every `every` days. `None` before the first day.
pub fn latest_until(remind_on: &str, every: Option<u32>, today: &str) -> Option<String> {
    let (first, today) = (day(remind_on)?, day(today)?);
    if first > today {
        return None;
    }
    let Some(every) = every.filter(|n| *n > 0) else { return Some(text(first)) };
    let passed = (today - first).num_days() / i64::from(every);
    Some(text(first + Duration::days(passed * i64::from(every))))
}

/// The days of the reminder from `from` on (that day included), at most `count`.
pub fn days_from(remind_on: &str, every: Option<u32>, from: &str, count: usize) -> Vec<String> {
    let (Some(first), Some(from)) = (day(remind_on), day(from)) else { return Vec::new() };
    let every = every.filter(|n| *n > 0).map(i64::from);
    let mut at = if first >= from {
        first
    } else {
        // a once-only reminder that has passed has no more days; a repeating one goes on from the next day not before `from`
        let Some(every) = every else { return Vec::new() };
        let behind = (from - first).num_days();
        first + Duration::days((behind + every - 1) / every * every)
    };
    let mut out = Vec::new();
    while out.len() < count {
        out.push(text(at));
        let Some(every) = every else { break };
        at += Duration::days(every);
    }
    out
}

/// A friend whose reminder has come and whose balance is not 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Due {
    pub person_id: String,
    pub name: String,
    /// The day of the reminder that came last, which is the day it rang or should have.
    pub due_on: String,
}

/// A day with reminders, and how many friends have one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Day {
    pub date: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reminders {
    pub today: String,
    /// The friends whose reminder day has come, the oldest first.
    pub due: Vec<Due>,
    /// The next days with a reminder, today included, with how many friends have one. For the phone to hand to the system.
    pub upcoming: Vec<Day>,
}

/// Which friends are due and which days come next, for `today` (a day in the person's time zone) and a period of `every_days` days (0 is no
/// reminders). Friends who are not on the list any more still count: the money is still owed.
pub fn reminders(conn: &Connection, today: &str, every_days: u32) -> Result<Reminders, AppError> {
    if day(today).is_none() || every_days > MAX_EVERY_DAYS {
        return Err(AppError::InvalidInput);
    }
    let mut out = Reminders { today: today.to_string(), due: Vec::new(), upcoming: Vec::new() };
    if every_days == 0 {
        return Ok(out);
    }
    let mut statement = conn.prepare(
        "SELECT p.id FROM people p JOIN person_balances b ON b.person_id = p.id WHERE p.merged_into IS NULL AND b.balance_minor != 0 ORDER BY p.id",
    )?;
    let ids: Vec<i64> = statement.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut days: BTreeMap<String, u32> = BTreeMap::new();
    for id in ids {
        let statement = ledger::statement_in(conn, id)?;
        // the debt last changed on the day of the latest line; the first reminder comes a period after it
        let Some(changed) = statement.lines.iter().map(|l| l.date.as_str()).max().and_then(day) else { continue };
        let first = text(changed + Duration::days(i64::from(every_days)));
        if let Some(due_on) = latest_until(&first, Some(every_days), today) {
            out.due.push(Due { person_id: id.to_string(), name: statement.person.name.clone(), due_on });
        }
        for date in days_from(&first, Some(every_days), today, SCHEDULED_DAYS) {
            *days.entry(date).or_default() += 1;
        }
    }
    out.due.sort_by(|a, b| a.due_on.cmp(&b.due_on).then(a.person_id.len().cmp(&b.person_id.len())).then(a.person_id.cmp(&b.person_id)));
    out.upcoming = days.into_iter().take(SCHEDULED_DAYS).map(|(date, count)| Day { date, count }).collect();
    Ok(out)
}
