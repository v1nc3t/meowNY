//! Stashes: money set aside inside one account for a goal (`docs/stash-design.md`). A holiday, a laptop, a rainy-day fund.
//!
//! * A stash lives in one account, chosen when it is made. The money never leaves the account: a stash only says that part of the balance is
//!   set aside, so no balance, statement, chart, net worth or budget moves when money goes into or out of one.
//! * Money goes in and out by entries (`entries`): an add above 0, a take below 0. What a stash holds (saved) is their sum. Nothing is stored:
//!   saved, set aside, free, left and per month are worked out when they are asked for (section 3).
//! * What an account holds that no stash has set aside is free. A local write that raises what a stash holds is checked against it, and against
//!   the target; one that lowers it must keep it at 0 or more (the delta rule, section 3.1). A sync never checks it (section 7.6).
//! * An archived stash still holds what it holds, and it can be emptied: it takes no more money, but takes and edits that lower what it holds are allowed.
//! * A stash is a goal or a term deposit (`docs/loans-deposits-design.md`, sections 3.3 and 5.3). A deposit has a rate, a start date and a maturity (its
//!   target date), and a state worked out from its row and today: running, matured, booked or broken. Its rules for money are in `entries` and `deposits`.

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::accounts;
use crate::budgets;
use crate::categories::{clean_name, parse_id, present_or_null, serialize_id};
use crate::money::{format_amount, parse_positive_amount};
use crate::rates::{self, Period};
use crate::transactions::clean_date;
use crate::vault::AppError;

pub mod deposits;
pub mod entries;

#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod deposits_tests;
#[cfg(test)]
mod entries_tests;
#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod numbers_tests;
#[cfg(test)]
mod table_tests;

/// What an account holds today, in its own currency (`?1` is today): everything that moved it up to that day (`accounts-design.md`, section 3).
/// The table of accounts is `a`.
pub(crate) const BALANCE: &str = "COALESCE((SELECT SUM(m.delta_minor) FROM account_movements m WHERE m.account_id = a.id AND m.day <= ?1), 0)";

/// What the stashes of an account hold: the sum of `held` (saved, or 0 when it is below 0) over every stash in it, the archived ones too
/// (section 3). The table of accounts is `a`.
pub(crate) const SET_ASIDE: &str =
    "COALESCE((SELECT SUM(MAX(COALESCE((SELECT SUM(e.amount_minor) FROM stash_entries e WHERE e.stash_id = t.id), 0), 0)) FROM stashes t WHERE t.account_id = a.id), 0)";

/// What a stash is for, fixed when it is made: a goal to save towards, or a term deposit (`docs/loans-deposits-design.md`, section 5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StashKind {
    Goal,
    Deposit,
}

impl StashKind {
    pub fn as_sql(self) -> &'static str {
        match self {
            StashKind::Goal => "GOAL",
            StashKind::Deposit => "DEPOSIT",
        }
    }

    pub fn from_sql(text: &str) -> Result<StashKind, AppError> {
        match text {
            "GOAL" => Ok(StashKind::Goal),
            "DEPOSIT" => Ok(StashKind::Deposit),
            _ => Err(AppError::Internal),
        }
    }
}

/// Where a deposit stands, worked out from its row and today and stored nowhere (section 5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DepositState {
    /// Not broken and before its maturity: money may be added.
    Running,
    /// Not broken, at or past its maturity, and its return is not booked yet.
    Matured,
    /// Not broken, at or past its maturity, and its return is booked.
    Booked,
    /// Emptied before its maturity. It stays as it is; the only way back is to delete the stash.
    Broken,
}

/// The numbers and dates of a term deposit (section 3.3). The maturity is the stash's `targetDate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deposit {
    /// The rate as a percent without trailing zeros: "3.1", "4", "0.5".
    pub interest_rate: String,
    /// What the rate is a percent of: a month or a year.
    pub interest_period: Period,
    /// The day the term begins. The maturity is after it.
    pub start_date: String,
    /// The day it was emptied before its maturity. None unless it is broken.
    pub broken_on: Option<String>,
    /// The day its return was booked (`deposits::book_return`). None until then, and again after a roll over.
    pub returned_on: Option<String>,
    /// Worked out from the dates and today (section 5.3).
    pub state: DepositState,
    /// What the whole term returns on what the deposit holds now: simple interest on the days from start to maturity, with a year of 365 days.
    /// Worked out on `held`, not on `saved`. In the stash's currency.
    pub expected_return: String,
    /// What it holds now plus `expected_return`.
    pub value_at_maturity: String,
    /// The days from today to the maturity; 0 once it is today or past.
    pub days_left: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stash {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    pub name: String,
    #[serde(serialize_with = "serialize_id")]
    pub account_id: i64,
    /// The name of the account.
    pub account: String,
    /// The currency of every amount of the stash: the account's, or the app's for an account in the main currency.
    pub currency: String,
    pub archived: bool,
    pub account_archived: bool,
    /// A goal to save towards, or a term deposit. Fixed when the stash is made.
    pub kind: StashKind,
    /// The rate, dates, state and numbers of a deposit. None for a goal.
    pub deposit: Option<Deposit>,
    /// Decimal string, e.g. "1200.00". None when the stash has no target.
    pub target: Option<String>,
    /// The day a goal is to be reached by; for a deposit, its maturity.
    pub target_date: Option<String>,
    pub notes: Option<String>,
    /// What the entries add up to. It may be below 0 or above the target only after a sync joined edits that were each fine alone (section 7.6).
    pub saved: String,
    /// What is still missing to the target, never below 0. None without a target.
    pub left: Option<String>,
    /// `floor(saved * 100 / target)`, 0 when saved is not above 0, and it may be above 100. None without a target.
    pub percent: Option<i64>,
    /// There is a target and saved has reached it.
    pub reached: bool,
    /// What to save each month to reach the target by its date, rounded up to the cent. None without a target or a date, when the target is
    /// reached, and when the date is past. Always None for a deposit.
    pub per_month: Option<String>,
    /// The target date is before today and the target is not reached.
    pub overdue: bool,
    /// What the account holds that no stash has set aside, in the account's currency. It is below 0 when the stashes hold more than the account.
    pub free: String,
    /// How many entries the stash has.
    pub entries: u32,
    /// The date of the latest entry.
    pub last_entry: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Detail {
    pub stash: Stash,
    /// Newest first. `saved_after` of each runs oldest first by `(date, id)`.
    pub entries: Vec<entries::Entry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewStash {
    pub name: String,
    /// The account it lives in. It must exist, not be archived and not be a liability.
    pub account_id: String,
    /// Left out means a goal. A deposit needs `interest_rate`, `interest_period` and `target_date` (its maturity, after the start date); a goal
    /// takes none of the deposit fields.
    #[serde(default)]
    pub kind: Option<StashKind>,
    /// "0.01" to "9999999999.99".
    #[serde(default)]
    pub target: Option<String>,
    /// Any real date; a past one makes the stash overdue. For a deposit it is the maturity and is required.
    #[serde(default)]
    pub target_date: Option<String>,
    /// Line breaks allowed, at most 1000 characters. Empty means none.
    #[serde(default)]
    pub notes: Option<String>,
    /// A deposit's rate as a percent, "0" to "100" with at most 3 decimals ("3.1", "4.125").
    #[serde(default)]
    pub interest_rate: Option<String>,
    /// What the rate is a percent of. Needed with the rate.
    #[serde(default)]
    pub interest_period: Option<Period>,
    /// The day a deposit's term begins. Left out means today.
    #[serde(default)]
    pub start_date: Option<String>,
}

/// Fields left out stay as they are. `target`, `target_date` and `notes` clear with `null`, but a deposit's `target_date` (its maturity) does not.
/// A new `account_id` moves the stash. The deposit fields are refused for a goal, and for a deposit that is broken (section 5.3).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StashPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub target: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub target_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub notes: Option<Option<String>>,
    /// Refused whatever it says: the kind is fixed when the stash is made. The field is here so that sending it answers `InvalidInput`, as every
    /// other refusal does, rather than a failure to read the patch.
    #[serde(default)]
    pub kind: Option<StashKind>,
    /// A deposit's new rate as a percent, as `NewStash.interest_rate`.
    #[serde(default)]
    pub interest_rate: Option<String>,
    /// A deposit's new period.
    #[serde(default)]
    pub interest_period: Option<Period>,
    /// A new start date. Left out while the maturity is moved from matured or booked to a day after today, the next term starts where the last one
    /// ended (the roll over of section 5.3).
    #[serde(default)]
    pub start_date: Option<String>,
}

/// What section 3 works out from what a stash holds, its target and its target date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Progress {
    pub left: Option<i64>,
    pub percent: Option<i64>,
    pub reached: bool,
    pub per_month: Option<i64>,
    pub overdue: bool,
}

/// The numbers of one stash on the day `today`. The months to go count the calendar months from today's to the target date's, whatever the
/// day; a target this month asks for all that is left. `per_month` is none without a target, without a date, with nothing left, and for a
/// date that is past: the stash is then `overdue`, which it is not when its target is reached.
pub(crate) fn progress(saved: i64, target: Option<i64>, target_date: Option<&str>, today: &str) -> Result<Progress, AppError> {
    let Some(target) = target else {
        return Ok(Progress { left: None, percent: None, reached: false, per_month: None, overdue: false });
    };
    let left = (target - saved).max(0);
    let percent = if saved > 0 { i64::try_from(i128::from(saved) * 100 / i128::from(target)).unwrap_or(i64::MAX) } else { 0 };
    let (mut per_month, mut overdue) = (None, false);
    if let (Some(date), true) = (target_date, left > 0) {
        if date < today {
            overdue = true;
        } else {
            let months = (month_number(date)? - month_number(today)?).max(1);
            per_month = Some((left + months - 1) / months);
        }
    }
    Ok(Progress { left: Some(left), percent: Some(percent), reached: saved >= target, per_month, overdue })
}

/// Months counted from year 0: `year * 12 + month`.
fn month_number(date: &str) -> Result<i64, AppError> {
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| AppError::Internal)?;
    Ok(i64::from(date.year()) * 12 + i64::from(date.month()))
}

/// The delta rule of section 3.1, on numbers. `before` is what the stash holds and `after` what it would hold; `free` is what the account has
/// free before the write; `target` is the stash's. Money going in must fit in what is free and under the target; money coming out must leave
/// 0 or more; a write that changes nothing is always allowed.
pub(crate) fn delta_allowed(before: i64, after: i64, free: i64, target: Option<i64>) -> bool {
    if after > before {
        after - before <= free && target.is_none_or(|t| after <= t)
    } else {
        after == before || after >= 0
    }
}

/// What a deposit's row stores, with its maturity (the stash's target date; a deposit made by the core always has one, and the start date stands in
/// for a row a sync left without).
#[derive(Debug, Clone)]
pub(crate) struct DepositFacts {
    pub rate_milli: i64,
    pub period: Period,
    pub start_date: String,
    pub maturity: String,
    pub broken_on: Option<String>,
    pub returned_on: Option<String>,
}

impl DepositFacts {
    /// Reads `kind, rate_milli, rate_period, start_date, broken_on, returned_on` of a row from column `at`. None for a goal.
    fn read(row: &rusqlite::Row, at: usize, target_date: Option<&str>) -> rusqlite::Result<Option<DepositFacts>> {
        let unreadable = |_| rusqlite::Error::InvalidQuery;
        if StashKind::from_sql(&row.get::<_, String>(at)?).map_err(unreadable)? == StashKind::Goal {
            return Ok(None);
        }
        let start_date: String = row.get(at + 3)?;
        Ok(Some(DepositFacts {
            rate_milli: row.get(at + 1)?,
            period: Period::from_sql(&row.get::<_, String>(at + 2)?).map_err(unreadable)?,
            maturity: target_date.map_or_else(|| start_date.clone(), str::to_string),
            start_date,
            broken_on: row.get(at + 4)?,
            returned_on: row.get(at + 5)?,
        }))
    }

    /// Section 5.3, first paragraph.
    pub(crate) fn state(&self, today: &str) -> DepositState {
        if self.broken_on.is_some() {
            DepositState::Broken
        } else if today < self.maturity.as_str() {
            DepositState::Running
        } else if self.returned_on.is_some() {
            DepositState::Booked
        } else {
            DepositState::Matured
        }
    }

    /// The deposit as the screens show it, for a stash that holds `held` (section 3.3).
    fn shown(&self, held: i64, today: &str) -> Deposit {
        let expected = rates::deposit_return(held, self.rate_milli, self.period, &self.start_date, &self.maturity);
        Deposit {
            interest_rate: rates::format_rate(self.rate_milli),
            interest_period: self.period,
            start_date: self.start_date.clone(),
            broken_on: self.broken_on.clone(),
            returned_on: self.returned_on.clone(),
            state: self.state(today),
            expected_return: format_amount(expected),
            value_at_maturity: format_amount(held.saturating_add(expected)),
            days_left: rates::days(today, &self.maturity),
        }
    }
}

/// What the delta rule is checked against: what is stored, not worked out for the screens.
#[derive(Debug, Clone)]
pub(crate) struct Facts {
    pub name: String,
    pub account_id: i64,
    pub archived: bool,
    pub account_archived: bool,
    pub target: Option<i64>,
    pub target_date: Option<String>,
    pub notes: Option<String>,
    pub saved: i64,
    /// None for a goal.
    pub deposit: Option<DepositFacts>,
}

impl Facts {
    /// What the stash takes away from its account (section 3): saved, or 0 when it is below 0.
    pub(crate) fn held(&self) -> i64 {
        self.saved.max(0)
    }

    /// The state of a deposit today; None for a goal.
    pub(crate) fn state(&self, today: &str) -> Option<DepositState> {
        self.deposit.as_ref().map(|d| d.state(today))
    }
}

pub(crate) fn facts(conn: &Connection, id: i64) -> Result<Facts, AppError> {
    conn.query_row(
        "SELECT s.name, s.account_id, s.archived, a.archived, s.target_minor, s.target_date, s.notes,
                COALESCE((SELECT SUM(e.amount_minor) FROM stash_entries e WHERE e.stash_id = s.id), 0),
                s.kind, s.rate_milli, s.rate_period, s.start_date, s.broken_on, s.returned_on
         FROM stashes s JOIN accounts a ON a.id = s.account_id WHERE s.id = ?1",
        [id],
        |r| {
            let target_date: Option<String> = r.get(5)?;
            Ok(Facts {
                name: r.get(0)?,
                account_id: r.get(1)?,
                archived: r.get::<_, i64>(2)? == 1,
                account_archived: r.get::<_, i64>(3)? == 1,
                target: r.get(4)?,
                notes: r.get(6)?,
                saved: r.get(7)?,
                deposit: DepositFacts::read(r, 8, target_date.as_deref())?,
                target_date,
            })
        },
    )
    .optional()?
    .ok_or(AppError::NotFound)
}

/// What an account has free today (section 3): its balance minus what the stashes in it hold, the archived ones too. It is below 0 when the
/// stashes hold more than the account has. This is the one place that works out free.
pub(crate) fn free(conn: &Connection, account_id: i64, today: &str) -> Result<i64, AppError> {
    let today = clean_date(today)?;
    conn.query_row(&format!("SELECT {BALANCE} - {SET_ASIDE} FROM accounts a WHERE a.id = ?2"), params![today, account_id], |r| r.get(0))
        .optional()?
        .ok_or(AppError::NotFound)
}

/// `?1` is today. Free is worked out from the same two pieces as `free`.
const SELECT: &str = "SELECT s.id, s.name, s.account_id, a.name, COALESCE(a.currency, (SELECT currency FROM settings WHERE id = 1)), s.archived, a.archived,
       s.target_minor, s.target_date, s.notes,
       COALESCE((SELECT SUM(e.amount_minor) FROM stash_entries e WHERE e.stash_id = s.id), 0),
       (SELECT count(*) FROM stash_entries e WHERE e.stash_id = s.id), (SELECT MAX(e.entry_date) FROM stash_entries e WHERE e.stash_id = s.id),
       @BALANCE - @SET_ASIDE, s.kind, s.rate_milli, s.rate_period, s.start_date, s.broken_on, s.returned_on
     FROM stashes s JOIN accounts a ON a.id = s.account_id";

fn select() -> String {
    SELECT.replace("@BALANCE", BALANCE).replace("@SET_ASIDE", SET_ASIDE)
}

fn from_row(row: &rusqlite::Row, today: &str) -> rusqlite::Result<Stash> {
    let (saved, target): (i64, Option<i64>) = (row.get(10)?, row.get(7)?);
    let target_date: Option<String> = row.get(8)?;
    let numbers = progress(saved, target, target_date.as_deref(), today).map_err(|_| rusqlite::Error::InvalidQuery)?;
    let deposit = DepositFacts::read(row, 14, target_date.as_deref())?;
    Ok(Stash {
        id: row.get(0)?,
        name: row.get(1)?,
        account_id: row.get(2)?,
        account: row.get(3)?,
        currency: row.get(4)?,
        archived: row.get::<_, i64>(5)? == 1,
        account_archived: row.get::<_, i64>(6)? == 1,
        kind: if deposit.is_some() { StashKind::Deposit } else { StashKind::Goal },
        deposit: deposit.as_ref().map(|d| d.shown(saved.max(0), today)),
        target: target.map(format_amount),
        target_date,
        notes: row.get(9)?,
        saved: format_amount(saved),
        left: numbers.left.map(format_amount),
        percent: numbers.percent,
        reached: numbers.reached,
        per_month: numbers.per_month.filter(|_| deposit.is_none()).map(format_amount),
        // a deposit's maturity is no deadline to save by
        overdue: numbers.overdue && deposit.is_none(),
        free: format_amount(row.get(13)?),
        entries: row.get(11)?,
        last_entry: row.get(12)?,
    })
}

/// The stashes in the person's order, the archived ones after the others; with `account_id`, only those of that account.
pub fn list(conn: &Connection, account_id: Option<i64>, today: &str) -> Result<Vec<Stash>, AppError> {
    let today = clean_date(today)?;
    let mut statement = conn.prepare_cached(&format!("{} WHERE ?2 IS NULL OR s.account_id = ?2 ORDER BY s.archived, s.position, s.id", select()))?;
    let rows = statement.query_map(params![today, account_id], |r| from_row(r, &today))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// One stash as the screens show it, without its entries.
pub(crate) fn stash(conn: &Connection, id: i64, today: &str) -> Result<Stash, AppError> {
    let today = clean_date(today)?;
    conn.query_row(&format!("{} WHERE s.id = ?2", select()), params![today, id], |r| from_row(r, &today)).optional()?.ok_or(AppError::NotFound)
}

/// A stash with its entries, newest first.
pub fn get(conn: &Connection, id: i64, today: &str) -> Result<Detail, AppError> {
    Ok(Detail { stash: stash(conn, id, today)?, entries: entries::list(conn, id)? })
}

/// A name is checked on this device only, among the stashes that are not archived, ignoring case (SQLite folds ASCII letters only). There is
/// no unique index: two devices that pick the same name keep two stashes (as accounts, `accounts-design.md`, section 16, choice 4).
fn name_is_taken(conn: &Connection, name: &str, except: Option<i64>) -> Result<bool, AppError> {
    Ok(conn
        .query_row("SELECT 1 FROM stashes WHERE name = ?1 COLLATE NOCASE AND archived = 0 AND id IS NOT ?2", params![name, except], |_| Ok(()))
        .optional()?
        .is_some())
}

/// A new stash goes to the end of the list, in an account that exists and is not archived and is not a liability: there is no money in a debt
/// to set aside (`docs/loans-deposits-design.md`, section 5.2). A deposit (section 5.3) needs a rate, a period and a maturity (`target_date`) after
/// its start date, which is today when left out; a goal takes none of the deposit fields.
pub fn create(conn: &Connection, input: &NewStash, today: &str) -> Result<Stash, AppError> {
    let today = clean_date(today)?;
    let name = clean_name(&input.name)?;
    let account = parse_id(&input.account_id)?;
    let target = input.target.as_deref().map(parse_positive_amount).transpose()?;
    let target_date = input.target_date.as_deref().map(clean_date).transpose()?;
    let notes = accounts::clean_notes(input.notes.as_deref())?;
    let kind = input.kind.unwrap_or(StashKind::Goal);
    let (rate_milli, rate_period, start_date) = match kind {
        StashKind::Goal if input.interest_rate.is_some() || input.interest_period.is_some() || input.start_date.is_some() => return Err(AppError::InvalidInput),
        StashKind::Goal => (None, None, None),
        StashKind::Deposit => {
            let rate = rates::parse_rate(input.interest_rate.as_deref().ok_or(AppError::InvalidInput)?)?;
            let period = input.interest_period.ok_or(AppError::InvalidInput)?;
            let start = input.start_date.as_deref().map(clean_date).transpose()?.unwrap_or_else(|| today.clone());
            if target_date.as_deref().is_none_or(|maturity| maturity <= start.as_str()) {
                return Err(AppError::InvalidInput);
            }
            (Some(rate), Some(period.as_sql()), Some(start))
        }
    };
    let tx = conn.unchecked_transaction()?;
    if !accounts::is_active(&tx, account)? || accounts::is_liability(&tx, account)? {
        return Err(AppError::InvalidInput);
    }
    if name_is_taken(&tx, &name, None)? {
        return Err(AppError::NameExists);
    }
    tx.execute(
        "INSERT INTO stashes (name, account_id, target_minor, target_date, notes, position, kind, rate_milli, rate_period, start_date)
         VALUES (?1, ?2, ?3, ?4, ?5, COALESCE((SELECT MAX(position) + 1 FROM stashes), 0), ?6, ?7, ?8, ?9)",
        params![name, account, target, target_date, notes, kind.as_sql(), rate_milli, rate_period, start_date],
    )?;
    let stash = stash(&tx, tx.last_insert_rowid(), &today)?;
    tx.commit()?;
    Ok(stash)
}

/// What an update writes of a deposit's terms: rate, period, start date and what is left of `returned_on` (section 5.3).
type Terms = (Option<i64>, Option<&'static str>, Option<String>, Option<String>);

/// The terms a deposit has after `patch`, given the maturity it will have (`maturity`, already the patch's). A broken deposit takes no change of
/// them, a deposit always has a maturity, and the maturity stays after the start date. **Roll over**: from matured or booked, a maturity after
/// today starts the next term (where the last ended, unless the patch gives a start date), and its return is not booked yet.
fn deposit_terms(current: &DepositFacts, patch: &StashPatch, maturity: Option<&str>, today: &str) -> Result<Terms, AppError> {
    let state = current.state(today);
    let sent = patch.interest_rate.is_some() || patch.interest_period.is_some() || patch.start_date.is_some() || patch.target_date.is_some();
    if sent && state == DepositState::Broken {
        return Err(AppError::InvalidInput);
    }
    let maturity = maturity.ok_or(AppError::InvalidInput)?;
    let rate = patch.interest_rate.as_deref().map(rates::parse_rate).transpose()?.unwrap_or(current.rate_milli);
    let period = patch.interest_period.unwrap_or(current.period);
    // a new term, with or without its own start date: its return is not booked yet
    let rolled = patch.target_date.is_some() && maturity > today && matches!(state, DepositState::Matured | DepositState::Booked);
    let start = match (&patch.start_date, rolled) {
        (Some(date), _) => clean_date(date)?,
        (None, true) => current.maturity.clone(),
        (None, false) => current.start_date.clone(),
    };
    if maturity <= start.as_str() {
        return Err(AppError::InvalidInput);
    }
    let returned_on = if rolled { None } else { current.returned_on.clone() };
    Ok((Some(rate), Some(period.as_sql()), Some(start), returned_on))
}

/// Changes a stash that is not archived (restore it first). The stamp moves only when something changed. A new target must be at least what
/// is saved. A new account moves the stash (section 5.1): the account must be active, not a liability, and in the same currency, and what the
/// stash holds must fit in what is free there; one that holds nothing always fits (section 5.4). Naming the account it has is not a move.
/// The kind is fixed: sending it is refused. The deposit fields are refused for a goal; for a deposit see `deposit_terms`.
pub fn update(conn: &Connection, id: i64, patch: &StashPatch, today: &str) -> Result<Stash, AppError> {
    let today = clean_date(today)?;
    if patch.kind.is_some() {
        return Err(AppError::InvalidInput);
    }
    let tx = conn.unchecked_transaction()?;
    let current = facts(&tx, id)?;
    if current.archived {
        return Err(AppError::InvalidInput);
    }
    let name = match &patch.name {
        Some(name) => clean_name(name)?,
        None => current.name.clone(),
    };
    let target = match &patch.target {
        Some(Some(text)) => Some(parse_positive_amount(text)?),
        Some(None) => None,
        None => current.target,
    };
    if target != current.target && target.is_some_and(|t| t < current.saved) {
        return Err(AppError::InvalidInput);
    }
    let target_date = match &patch.target_date {
        Some(date) => date.as_deref().map(clean_date).transpose()?,
        None => current.target_date.clone(),
    };
    let (rate_milli, rate_period, start_date, returned_on) = match &current.deposit {
        Some(deposit) => deposit_terms(deposit, patch, target_date.as_deref(), &today)?,
        None if patch.interest_rate.is_some() || patch.interest_period.is_some() || patch.start_date.is_some() => return Err(AppError::InvalidInput),
        None => (None, None, None, None),
    };
    let notes = match &patch.notes {
        Some(notes) => accounts::clean_notes(notes.as_deref())?,
        None => current.notes.clone(),
    };
    let account = match patch.account_id.as_deref().map(parse_id).transpose()? {
        Some(account) if account != current.account_id => {
            if !accounts::is_active(&tx, account)?
                || accounts::is_liability(&tx, account)?
                || accounts::currencies_differ(&tx, current.account_id, account)?
                || (current.held() > 0 && current.held() > free(&tx, account, &today)?)
            {
                return Err(AppError::InvalidInput);
            }
            account
        }
        _ => current.account_id,
    };
    if name_is_taken(&tx, &name, Some(id))? {
        return Err(AppError::NameExists);
    }
    tx.execute(
        "UPDATE stashes SET name = ?1, account_id = ?2, target_minor = ?3, target_date = ?4, notes = ?5,
                rate_milli = ?6, rate_period = ?7, start_date = ?8, returned_on = ?9 WHERE id = ?10",
        params![name, account, target, target_date, notes, rate_milli, rate_period, start_date, returned_on, id],
    )?;
    let stash = stash(&tx, id, &today)?;
    tx.commit()?;
    Ok(stash)
}

/// Sets the order of the stashes that are not archived: `ids` is all of them, in the new order. Only the rows that move are written, so the
/// others keep their sync stamp.
pub fn reorder(conn: &Connection, ids: &[i64]) -> Result<(), AppError> {
    if ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len() {
        return Err(AppError::InvalidInput);
    }
    let tx = conn.unchecked_transaction()?;
    for id in ids {
        if tx.query_row("SELECT 1 FROM stashes WHERE id = ?1 AND archived = 0", [id], |_| Ok(())).optional()?.is_none() {
            return Err(AppError::InvalidInput);
        }
    }
    let active: i64 = tx.query_row("SELECT count(*) FROM stashes WHERE archived = 0", [], |r| r.get(0))?;
    if active != ids.len() as i64 {
        return Err(AppError::InvalidInput);
    }
    for (position, id) in ids.iter().enumerate() {
        tx.execute("UPDATE stashes SET position = ?1 WHERE id = ?2 AND position != ?1", params![position as i64, id])?;
    }
    tx.commit()?;
    Ok(())
}

/// Takes a stash out of the lists. Its entries stay, and what it holds stays set aside. A stash that is archived already is answered as it is.
pub fn archive(conn: &Connection, id: i64, today: &str) -> Result<Stash, AppError> {
    let current = stash(conn, id, today)?;
    if current.archived {
        return Ok(current);
    }
    conn.execute("UPDATE stashes SET archived = 1 WHERE id = ?1", [id])?;
    stash(conn, id, today)
}

/// Brings an archived stash back. The name must not have been taken by another stash in the meantime. A stash that is not archived is
/// answered as it is.
pub fn restore(conn: &Connection, id: i64, today: &str) -> Result<Stash, AppError> {
    let tx = conn.unchecked_transaction()?;
    let current = stash(&tx, id, today)?;
    if !current.archived {
        return Ok(current);
    }
    if name_is_taken(&tx, &current.name, Some(id))? {
        return Err(AppError::NameExists);
    }
    tx.execute("UPDATE stashes SET archived = 0 WHERE id = ?1", [id])?;
    let stash = stash(&tx, id, today)?;
    tx.commit()?;
    Ok(stash)
}

/// Deletes a stash, archived or not, with its entries (the foreign key cascades; the stash leaves one marker and the entries none). The
/// transfers that came with entries stay, and what the stash held is free again.
pub fn delete(conn: &Connection, id: i64) -> Result<(), AppError> {
    match conn.execute("DELETE FROM stashes WHERE id = ?1", [id])? {
        0 => Err(AppError::NotFound),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryPoint {
    /// "YYYY-MM".
    pub month: String,
    /// What the stash held at the end of that month.
    pub saved: String,
}

/// What a stash held at the end of each month of `year` ("YYYY"), for the chart: on the last day of a month that is over, on `today` for the
/// current month, and no point for a month that has not begun. What it held on a day is the sum of the entries dated on or before it.
pub fn history(conn: &Connection, id: i64, year: &str, today: &str) -> Result<Vec<HistoryPoint>, AppError> {
    let today = clean_date(today)?;
    budgets::month_start(&format!("{year}-01"))?;
    let tx = conn.unchecked_transaction()?;
    facts(&tx, id)?;
    let mut points = Vec::new();
    for m in 1..=12 {
        let month = format!("{year}-{m:02}");
        let first = budgets::month_start(&month)?;
        if first.to_string() > today {
            break;
        }
        let last = budgets::next_month(first)?.pred_opt().ok_or(AppError::InvalidInput)?.to_string();
        let saved: i64 = tx.query_row(
            "SELECT COALESCE(SUM(amount_minor), 0) FROM stash_entries WHERE stash_id = ?1 AND entry_date <= ?2",
            params![id, last.as_str().min(today.as_str())],
            |r| r.get(0),
        )?;
        points.push(HistoryPoint { month, saved: format_amount(saved) });
    }
    Ok(points)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// How many stashes are not archived.
    pub stashes: u32,
    /// What the stashes that are not archived hold (the sum of `held`, section 3), in the app's currency: a stash of a foreign account at the
    /// account's rate. The archived ones are left out here as they are from `stashes` and `target`; what they hold still counts as set aside
    /// in the account (`Account.set_aside`).
    pub saved: String,
    /// The targets of the stashes that are not archived and have one, in the app's currency.
    pub target: String,
    /// How many of the stashes that are not archived have reached their target.
    pub reached: u32,
}

pub fn summary(conn: &Connection, today: &str) -> Result<Summary, AppError> {
    clean_date(today)?;
    let mut statement = conn.prepare(
        "SELECT a.rate, s.target_minor, COALESCE((SELECT SUM(e.amount_minor) FROM stash_entries e WHERE e.stash_id = s.id), 0)
         FROM stashes s JOIN accounts a ON a.id = s.account_id WHERE s.archived = 0",
    )?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, i64>(2)?)))?;
    let (mut stashes, mut saved, mut target, mut reached) = (0u32, 0i64, 0i64, 0u32);
    for row in rows {
        let (rate, aim, held) = row?;
        stashes += 1;
        saved += accounts::in_main(held.max(0), rate.as_deref());
        if let Some(aim) = aim {
            target += accounts::in_main(aim, rate.as_deref());
            reached += u32::from(held >= aim);
        }
    }
    Ok(Summary { stashes, saved: format_amount(saved), target: format_amount(target), reached })
}
