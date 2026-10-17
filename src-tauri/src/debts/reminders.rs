//! Reminders about debts (`docs/debts-design.md`, section 11). A reminder belongs to a friend: a day and, optionally, a number of days
//! after which it comes again. It is **live while the balance is not 0** (settling to 0 clears it, and a balance that reaches 0 any
//! other way makes it quiet). Nothing is written when a reminder comes due, so two devices never fight over one: each device keeps for
//! itself which day it has shown, and rings at its own time of day. This module only says which days those are.

use chrono::{Duration, NaiveDate};
use serde::Serialize;
use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::vault::AppError;

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

/// Which friends are due and which days come next, for `today` (a day in the person's time zone). Friends who are not on the list any more
/// still count: the money is still owed.
pub fn reminders(conn: &Connection, today: &str) -> Result<Reminders, AppError> {
    if day(today).is_none() {
        return Err(AppError::InvalidInput);
    }
    let mut statement = conn.prepare(
        "SELECT p.id, p.name, p.remind_on, p.remind_every_days FROM people p JOIN person_balances b ON b.person_id = p.id
         WHERE p.remind_on IS NOT NULL AND p.merged_into IS NULL AND b.balance_minor != 0 ORDER BY p.remind_on, p.id",
    )?;
    let rows: Vec<(i64, String, String, Option<u32>)> = statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
    let mut due = Vec::new();
    let mut days: BTreeMap<String, u32> = BTreeMap::new();
    for (id, name, remind_on, every) in rows {
        if let Some(due_on) = latest_until(&remind_on, every, today) {
            due.push(Due { person_id: id.to_string(), name, due_on });
        }
        for date in days_from(&remind_on, every, today, SCHEDULED_DAYS) {
            *days.entry(date).or_default() += 1;
        }
    }
    due.sort_by(|a, b| a.due_on.cmp(&b.due_on).then(a.person_id.len().cmp(&b.person_id.len())).then(a.person_id.cmp(&b.person_id)));
    let upcoming = days.into_iter().take(SCHEDULED_DAYS).map(|(date, count)| Day { date, count }).collect();
    Ok(Reminders { today: today.to_string(), due, upcoming })
}
