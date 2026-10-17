//! Recurring items: templates for income or expenses that repeat. Paying one copies it into a
//! normal transaction (see `pay`). Due dates always come from `start_date`, never from the
//! previous due date, so "monthly from 31 Jan" gives 28 Feb, 31 Mar and does not drift.

use chrono::{Days, Months, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Deserializer, Serialize};

use crate::categories::{parse_id, serialize_id};
use crate::debts::splits::{self, Template};
use crate::money::{format_amount, parse_positive_amount};
use crate::transactions::{category_is_active, clean_date, clean_description, clean_name};
use crate::vault::AppError;

const MAX_INTERVAL_COUNT: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Unit {
    Day,
    Week,
    Month,
    Year,
}

impl Unit {
    fn as_str(self) -> &'static str {
        match self {
            Unit::Day => "DAY",
            Unit::Week => "WEEK",
            Unit::Month => "MONTH",
            Unit::Year => "YEAR",
        }
    }

    fn parse(text: &str) -> Result<Unit, AppError> {
        match text {
            "DAY" => Ok(Unit::Day),
            "WEEK" => Ok(Unit::Week),
            "MONTH" => Ok(Unit::Month),
            "YEAR" => Ok(Unit::Year),
            _ => Err(AppError::Internal),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recurring {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    #[serde(serialize_with = "serialize_id")]
    pub category_id: i64,
    pub name: String,
    pub description: Option<String>,
    /// Decimal string, e.g. "12.50". Null for a bill with no fixed amount: each payment says what was paid.
    pub amount: Option<String>,
    pub start_date: String,
    pub interval_unit: Unit,
    pub interval_count: u32,
    pub next_due_date: String,
    pub end_date: Option<String>,
    pub is_active: bool,
    /// How each payment is shared, as weights (yours and one for each friend or guest), or null when the payments are not split.
    pub split: Option<Template>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRecurring {
    pub name: String,
    /// Leave out (or null) when the amount changes every time.
    #[serde(default)]
    pub amount: Option<String>,
    pub category_id: String,
    pub start_date: String,
    pub interval_unit: Unit,
    pub interval_count: u32,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Fields left out stay as they are. `description: null` and `endDate: null` clear them, and `amount: null`
/// makes it a bill with no fixed amount.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecurringPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub amount: Option<Option<String>>,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub interval_unit: Option<Unit>,
    #[serde(default)]
    pub interval_count: Option<u32>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub end_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub description: Option<Option<String>>,
}

fn present_or_null<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::deserialize(deserializer)?))
}

fn parse_date(text: &str) -> Result<NaiveDate, AppError> {
    clean_date(text)?.parse().map_err(|_| AppError::Internal)
}

/// The `k`-th occurrence (0 is `start`). `None` when the date would leave the calendar.
/// Months and years are added to `start` itself, so a day that does not exist in the target
/// month (31st, 29 Feb) lands on the last day of that month.
pub fn due_date(start: NaiveDate, unit: Unit, count: u32, k: u32) -> Option<NaiveDate> {
    let steps = u64::from(count) * u64::from(k);
    match unit {
        Unit::Day => start.checked_add_days(Days::new(steps)),
        Unit::Week => start.checked_add_days(Days::new(steps * 7)),
        Unit::Month => start.checked_add_months(Months::new(u32::try_from(steps).ok()?)),
        Unit::Year => start.checked_add_months(Months::new(u32::try_from(steps * 12).ok()?)),
    }
}

/// The first occurrence on or after `date`.
pub fn first_on_or_after(start: NaiveDate, unit: Unit, count: u32, date: NaiveDate) -> Option<NaiveDate> {
    (0..).map_while(|k| due_date(start, unit, count, k)).find(|due| *due >= date)
}

/// The first occurrence after `date`.
pub fn next_after(start: NaiveDate, unit: Unit, count: u32, date: NaiveDate) -> Option<NaiveDate> {
    (0..).map_while(|k| due_date(start, unit, count, k)).find(|due| *due > date)
}

const COLUMNS: &str = "id, category_id, name, description, amount_minor, start_date, interval_unit, \
                       interval_count, next_due_date, end_date, is_active";

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Recurring> {
    let unit: String = row.get(6)?;
    Ok(Recurring {
        id: row.get(0)?,
        category_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        amount: row.get::<_, Option<i64>>(4)?.map(format_amount),
        start_date: row.get(5)?,
        interval_unit: Unit::parse(&unit).map_err(|_| rusqlite::Error::InvalidQuery)?,
        interval_count: row.get(7)?,
        next_due_date: row.get(8)?,
        end_date: row.get(9)?,
        is_active: row.get::<_, i64>(10)? == 1,
        split: None,
    })
}

/// Fills in the split of each item, with one query for all of them.
fn hydrate(conn: &Connection, items: &mut [Recurring]) -> Result<(), AppError> {
    let ids: Vec<i64> = items.iter().map(|r| r.id).collect();
    let mut templates = splits::templates_of(conn, &ids)?;
    for item in items {
        item.split = templates.remove(&item.id);
    }
    Ok(())
}

pub fn find(conn: &Connection, id: i64) -> Result<Option<Recurring>, AppError> {
    let found = conn.query_row(&format!("SELECT {COLUMNS} FROM recurring_transactions WHERE id = ?1"), [id], from_row).optional()?;
    found
        .map(|item| {
            let mut one = [item];
            hydrate(conn, &mut one)?;
            let [item] = one;
            Ok(item)
        })
        .transpose()
}

pub fn get(conn: &Connection, id: i64) -> Result<Recurring, AppError> {
    find(conn, id)?.ok_or(AppError::NotFound)
}

/// No amount, or an empty one, means the bill varies. Otherwise it is a positive amount in minor units.
fn optional_amount(text: Option<&str>) -> Result<Option<i64>, AppError> {
    match text.map(str::trim).filter(|t| !t.is_empty()) {
        None => Ok(None),
        Some(text) => Ok(Some(parse_positive_amount(text)?)),
    }
}

fn category_is_expense(conn: &Connection, id: i64) -> Result<bool, AppError> {
    Ok(conn.query_row("SELECT 1 FROM categories WHERE id = ?1 AND type = 'EXPENSE'", [id], |_| Ok(())).optional()?.is_some())
}

fn clean_count(count: u32) -> Result<u32, AppError> {
    if count == 0 || count > MAX_INTERVAL_COUNT {
        return Err(AppError::InvalidInput);
    }
    Ok(count)
}

/// An end date may not come before the start. An unreachable first occurrence is rejected.
fn clean_end(start: NaiveDate, end: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(end) = end else { return Ok(None) };
    if parse_date(end)? < start {
        return Err(AppError::InvalidInput);
    }
    Ok(Some(end.to_string()))
}

pub fn create(conn: &Connection, input: &NewRecurring) -> Result<Recurring, AppError> {
    let name = clean_name(&input.name)?;
    let minor = optional_amount(input.amount.as_deref())?;
    let category_id = parse_id(&input.category_id)?;
    let start = parse_date(&input.start_date)?;
    let count = clean_count(input.interval_count)?;
    let end = clean_end(start, input.end_date.as_deref())?;
    let description = clean_description(input.description.as_deref())?;
    if !category_is_active(conn, category_id)? {
        return Err(AppError::InvalidInput);
    }

    conn.execute(
        "INSERT INTO recurring_transactions
           (category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, end_date)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?5, ?8)",
        params![category_id, name, description, minor, input.start_date, input.interval_unit.as_str(), count, end],
    )?;
    get(conn, conn.last_insert_rowid())
}

/// Editing never touches transactions that were already paid; they are copies.
/// When the schedule changes, the next due date moves to the first new occurrence on or after
/// the old one, so an edit does not turn an upcoming item into an overdue one or the reverse.
pub fn update(conn: &Connection, id: i64, patch: &RecurringPatch) -> Result<Recurring, AppError> {
    let current = get(conn, id)?;

    let name = match &patch.name {
        Some(name) => clean_name(name)?,
        None => current.name.clone(),
    };
    let minor = match &patch.amount {
        Some(amount) => optional_amount(amount.as_deref())?,
        None => optional_amount(current.amount.as_deref())?,
    };
    let category_id = match &patch.category_id {
        Some(category) => {
            let category_id = parse_id(category)?;
            if category_id != current.category_id && !category_is_active(conn, category_id)? {
                return Err(AppError::InvalidInput);
            }
            category_id
        }
        None => current.category_id,
    };
    let description = match &patch.description {
        Some(description) => clean_description(description.as_deref())?,
        None => current.description.clone(),
    };
    // A split item stays an expense: take the split off first to move it to an income category.
    if current.split.is_some() && category_id != current.category_id && !category_is_expense(conn, category_id)? {
        return Err(AppError::InvalidInput);
    }
    let start_text = patch.start_date.clone().unwrap_or_else(|| current.start_date.clone());
    let start = parse_date(&start_text)?;
    let unit = patch.interval_unit.unwrap_or(current.interval_unit);
    let count = clean_count(patch.interval_count.unwrap_or(current.interval_count))?;
    let end_text = match &patch.end_date {
        Some(end) => end.clone(),
        None => current.end_date.clone(),
    };
    let end = clean_end(start, end_text.as_deref())?;

    let schedule_changed =
        start_text != current.start_date || unit != current.interval_unit || count != current.interval_count;
    let next_due = if schedule_changed {
        let old = parse_date(&current.next_due_date)?;
        first_on_or_after(start, unit, count, old).ok_or(AppError::InvalidInput)?.to_string()
    } else {
        current.next_due_date.clone()
    };

    conn.execute(
        "UPDATE recurring_transactions
         SET name = ?1, description = ?2, amount_minor = ?3, category_id = ?4, start_date = ?5,
             interval_unit = ?6, interval_count = ?7, next_due_date = ?8, end_date = ?9
         WHERE id = ?10",
        params![name, description, minor, category_id, start_text, unit.as_str(), count, next_due, end, id],
    )?;
    get(conn, id)
}

/// Transactions already paid from it keep their data; their link to the item is cleared.
pub fn delete(conn: &Connection, id: i64) -> Result<(), AppError> {
    match conn.execute("DELETE FROM recurring_transactions WHERE id = ?1", [id])? {
        0 => Err(AppError::NotFound),
        _ => Ok(()),
    }
}

/// Every item, soonest due first.
pub fn list(conn: &Connection) -> Result<Vec<Recurring>, AppError> {
    let mut statement = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM recurring_transactions ORDER BY is_active DESC, next_due_date, id"
    ))?;
    let rows = statement.query_map([], from_row)?;
    let mut items = rows.collect::<Result<Vec<_>, _>>()?;
    hydrate(conn, &mut items)?;
    Ok(items)
}

#[derive(Debug, Serialize)]
pub struct Dues {
    /// Active and due before today, oldest first.
    pub overdue: Vec<Recurring>,
    /// Active and due from today up to `within_days` ahead, soonest first.
    pub upcoming: Vec<Recurring>,
}

const DEFAULT_WITHIN_DAYS: u32 = 30;
const MAX_WITHIN_DAYS: u32 = 366;

/// `today` is the date in the user's timezone setting, so "overdue" follows it too.
pub fn dues(conn: &Connection, today: &str, within_days: Option<u32>) -> Result<Dues, AppError> {
    let today_date = parse_date(today)?;
    let days = within_days.unwrap_or(DEFAULT_WITHIN_DAYS);
    if days > MAX_WITHIN_DAYS {
        return Err(AppError::InvalidInput);
    }
    let horizon = today_date.checked_add_days(Days::new(days.into())).ok_or(AppError::InvalidInput)?.to_string();
    let mut statement = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM recurring_transactions
         WHERE is_active = 1 AND next_due_date <= ?1 ORDER BY next_due_date, id"
    ))?;
    let mut items = statement.query_map([&horizon], from_row)?.collect::<Result<Vec<_>, _>>()?;
    hydrate(conn, &mut items)?;
    let (overdue, upcoming) = items.into_iter().partition(|r| r.next_due_date.as_str() < today);
    Ok(Dues { overdue, upcoming })
}

/// Moves the item past the occurrence it is on. Stops it when the next one is after the end date
/// or off the calendar. The caller runs this inside its own database transaction.
fn advance(conn: &Connection, item: &Recurring) -> Result<(), AppError> {
    let start = parse_date(&item.start_date)?;
    let due = parse_date(&item.next_due_date)?;
    let next = next_after(start, item.interval_unit, item.interval_count, due);
    let (next_text, active) = match next {
        Some(next) => {
            let within_end = item.end_date.as_deref().is_none_or(|end| next.to_string().as_str() <= end);
            (next.to_string(), within_end)
        }
        None => (item.next_due_date.clone(), false),
    };
    conn.execute(
        "UPDATE recurring_transactions SET next_due_date = ?1, is_active = ?2 WHERE id = ?3",
        params![next_text, active, item.id],
    )?;
    Ok(())
}

fn find_active(conn: &Connection, id: i64) -> Result<Recurring, AppError> {
    let item = get(conn, id)?;
    if item.is_active {
        Ok(item)
    } else {
        Err(AppError::InvalidInput)
    }
}

fn is_unique_conflict(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(e, _) if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

/// Turns the current occurrence into a normal transaction and moves to the next one, all or
/// nothing. `amount` defaults to the planned amount and `paid_on` to `today`; the planned amount
/// is kept on the transaction. An occurrence that already has a transaction is refused.
pub fn pay(
    conn: &Connection,
    id: i64,
    amount: Option<&str>,
    paid_on: Option<&str>,
    today: &str,
) -> Result<crate::transactions::Transaction, AppError> {
    let paid_on = clean_date(paid_on.unwrap_or(today))?;
    let actual = amount.map(parse_positive_amount).transpose()?;

    let tx = conn.unchecked_transaction()?;
    let item = find_active(&tx, id)?;
    // A bill with no fixed amount has nothing planned, so the payment must say what was paid.
    let planned = optional_amount(item.amount.as_deref())?;
    let paid = actual.or(planned).ok_or(AppError::InvalidInput)?;
    let inserted = tx.execute(
        "INSERT INTO transactions
           (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor,
            planned_amount_minor, payment_date, description)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![item.category_id, id, item.next_due_date, item.name, paid, planned, paid_on, item.description],
    );
    match inserted {
        Ok(_) => {}
        Err(e) if is_unique_conflict(&e) => return Err(AppError::InvalidInput),
        Err(e) => return Err(e.into()),
    }
    let created = tx.last_insert_rowid();
    // An item that is split makes a payment that is split by the same proportions, whatever was paid.
    splits::apply_template(&tx, created, id, paid)?;
    advance(&tx, &item)?;
    tx.commit()?;
    crate::transactions::get(conn, created)
}

/// Passes over the current occurrence without a transaction. The audit log records it.
pub fn skip(conn: &Connection, id: i64) -> Result<Recurring, AppError> {
    let tx = conn.unchecked_transaction()?;
    let item = find_active(&tx, id)?;
    advance(&tx, &item)?;
    tx.commit()?;
    get(conn, id)
}

/// Pause (`false`) or resume (`true`). An item whose next occurrence is past its end date
/// cannot be resumed.
pub fn set_active(conn: &Connection, id: i64, active: bool) -> Result<Recurring, AppError> {
    let item = get(conn, id)?;
    if active && item.end_date.as_deref().is_some_and(|end| item.next_due_date.as_str() > end) {
        return Err(AppError::InvalidInput);
    }
    conn.execute("UPDATE recurring_transactions SET is_active = ?1 WHERE id = ?2", params![active, id])?;
    get(conn, id)
}

/// Stops the item for good: inactive, and the end date is today, or the start date if that is
/// later, unless it already ends sooner. Nothing already paid changes.
pub fn end(conn: &Connection, id: i64, today: &str) -> Result<Recurring, AppError> {
    let item = get(conn, id)?;
    let today = parse_date(today)?;
    let start = parse_date(&item.start_date)?;
    let mut end = today.max(start);
    if let Some(existing) = item.end_date.as_deref() {
        end = end.min(parse_date(existing)?);
    }
    conn.execute(
        "UPDATE recurring_transactions SET is_active = 0, end_date = ?1 WHERE id = ?2",
        params![end.to_string(), id],
    )?;
    get(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::{self, CategoryType};

    fn d(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    fn due(start: &str, unit: Unit, count: u32, k: u32) -> String {
        due_date(d(start), unit, count, k).unwrap().to_string()
    }

    #[test]
    fn monthly_from_the_31st_clamps_without_drifting() {
        assert_eq!(due("2026-01-31", Unit::Month, 1, 0), "2026-01-31");
        assert_eq!(due("2026-01-31", Unit::Month, 1, 1), "2026-02-28");
        assert_eq!(due("2026-01-31", Unit::Month, 1, 2), "2026-03-31");
        assert_eq!(due("2026-01-31", Unit::Month, 1, 3), "2026-04-30");
        assert_eq!(due("2026-01-31", Unit::Month, 1, 13), "2027-02-28");
    }

    #[test]
    fn leap_year_february() {
        assert_eq!(due("2028-01-31", Unit::Month, 1, 1), "2028-02-29");
        assert_eq!(due("2028-01-29", Unit::Month, 1, 13), "2029-02-28");
    }

    #[test]
    fn yearly_on_feb_29() {
        assert_eq!(due("2024-02-29", Unit::Year, 1, 1), "2025-02-28");
        assert_eq!(due("2024-02-29", Unit::Year, 1, 3), "2027-02-28");
        assert_eq!(due("2024-02-29", Unit::Year, 1, 4), "2028-02-29");
        assert_eq!(due("2024-02-29", Unit::Year, 4, 1), "2028-02-29");
    }

    #[test]
    fn days_weeks_and_multi_month_steps() {
        assert_eq!(due("2026-12-30", Unit::Day, 3, 1), "2027-01-02");
        assert_eq!(due("2026-12-30", Unit::Week, 2, 1), "2027-01-13");
        assert_eq!(due("2026-11-30", Unit::Month, 3, 1), "2027-02-28");
    }

    #[test]
    fn next_and_first_on_or_after() {
        let start = d("2026-01-31");
        assert_eq!(next_after(start, Unit::Month, 1, d("2026-01-31")).unwrap().to_string(), "2026-02-28");
        assert_eq!(next_after(start, Unit::Month, 1, d("2026-02-10")).unwrap().to_string(), "2026-02-28");
        assert_eq!(first_on_or_after(start, Unit::Month, 1, d("2026-02-28")).unwrap().to_string(), "2026-02-28");
        assert_eq!(first_on_or_after(start, Unit::Month, 1, d("2026-03-01")).unwrap().to_string(), "2026-03-31");
        assert_eq!(first_on_or_after(start, Unit::Month, 1, d("2025-01-01")).unwrap().to_string(), "2026-01-31");
    }

    fn db() -> (Connection, i64, i64) {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn.execute("INSERT INTO settings (id, currency, timezone) VALUES (1, 'EUR', 'Europe/Dublin')", []).unwrap();
        let rent = categories::create(&conn, "Rent", CategoryType::Expense).unwrap().id;
        let salary = categories::create(&conn, "Salary", CategoryType::Income).unwrap().id;
        (conn, rent, salary)
    }

    fn new(category: i64, start: &str) -> NewRecurring {
        NewRecurring {
            name: "Rent".into(),
            amount: Some("900.00".into()),
            category_id: category.to_string(),
            start_date: start.into(),
            interval_unit: Unit::Month,
            interval_count: 1,
            end_date: None,
            description: None,
        }
    }

    fn varying(category: i64, start: &str) -> NewRecurring {
        NewRecurring { name: "Electricity".into(), amount: None, ..new(category, start) }
    }

    #[test]
    fn a_bill_can_have_no_fixed_amount() {
        let (conn, rent, _) = db();
        let item = create(&conn, &varying(rent, "2026-01-20")).unwrap();
        assert_eq!(item.amount, None);
        assert_eq!(list(&conn).unwrap()[0].amount, None);
        // An empty amount means the same, and a stray space does not matter.
        for blank in ["", "  "] {
            let input = NewRecurring { amount: Some(blank.into()), ..varying(rent, "2026-01-20") };
            assert_eq!(create(&conn, &input).unwrap().amount, None, "{blank:?}");
        }
        // What is written must still be a real amount.
        for bad in ["0.00", "-5.00", "abc", "12", "12.345"] {
            let input = NewRecurring { amount: Some(bad.into()), ..varying(rent, "2026-01-20") };
            assert_eq!(create(&conn, &input).unwrap_err(), AppError::InvalidInput, "{bad}");
        }
    }

    #[test]
    fn editing_can_add_or_remove_the_fixed_amount() {
        let (conn, rent, _) = db();
        let fixed = create(&conn, &new(rent, "2026-01-31")).unwrap();
        // Left out: unchanged. Null: no fixed amount any more. A value: fixed again.
        let renamed = update(&conn, fixed.id, &RecurringPatch { name: Some("Flat".into()), ..Default::default() }).unwrap();
        assert_eq!(renamed.amount.as_deref(), Some("900.00"));
        let varies = update(&conn, fixed.id, &RecurringPatch { amount: Some(None), ..Default::default() }).unwrap();
        assert_eq!(varies.amount, None);
        let renamed_again = update(&conn, fixed.id, &RecurringPatch { name: Some("Flat 4".into()), ..Default::default() }).unwrap();
        assert_eq!(renamed_again.amount, None, "an edit that does not mention the amount leaves a varying bill varying");
        let fixed_again = update(&conn, fixed.id, &RecurringPatch { amount: Some(Some("950.00".into())), ..Default::default() }).unwrap();
        assert_eq!(fixed_again.amount.as_deref(), Some("950.00"));
        assert_eq!(update(&conn, fixed.id, &RecurringPatch { amount: Some(Some("0.00".into())), ..Default::default() }).unwrap_err(), AppError::InvalidInput);
    }

    #[test]
    fn a_varying_bill_is_paid_with_the_amount_of_the_day() {
        let (conn, rent, _) = db();
        let item = create(&conn, &varying(rent, "2026-01-20")).unwrap();

        // It cannot be paid without saying how much, and a failed attempt changes nothing.
        assert_eq!(pay(&conn, item.id, None, Some("2026-01-21"), TODAY).unwrap_err(), AppError::InvalidInput);
        assert_eq!(pay(&conn, item.id, Some("0.00"), Some("2026-01-21"), TODAY).unwrap_err(), AppError::InvalidInput);
        assert_eq!(get(&conn, item.id).unwrap().next_due_date, "2026-01-20");
        assert_eq!(conn.query_row("SELECT count(*) FROM transactions", [], |r| r.get::<_, i64>(0)).unwrap(), 0);

        let january = pay(&conn, item.id, Some("78.40"), Some("2026-01-21"), TODAY).unwrap();
        let february = pay(&conn, item.id, Some("91.05"), Some("2026-02-19"), TODAY).unwrap();
        assert_eq!((january.amount.as_str(), february.amount.as_str()), ("78.40", "91.05"));
        assert_eq!(get(&conn, item.id).unwrap().next_due_date, "2026-03-20", "it moves on like any other item");
        // Nothing was planned, so nothing is stored as planned.
        let planned: Vec<Option<i64>> = conn
            .prepare("SELECT planned_amount_minor FROM transactions ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(planned, [None, None]);
    }

    #[test]
    fn the_amount_in_a_request_may_be_left_out_or_null() {
        let make = |amount: &str| -> NewRecurring {
            serde_json::from_str(&format!(r#"{{"name":"Gas","categoryId":"1","startDate":"2026-01-01","intervalUnit":"MONTH","intervalCount":1{amount}}}"#)).unwrap()
        };
        assert_eq!(make("").amount, None);
        assert_eq!(make(r#","amount":null"#).amount, None);
        assert_eq!(make(r#","amount":"12.00""#).amount.as_deref(), Some("12.00"));

        let patch = |json: &str| -> RecurringPatch { serde_json::from_str(json).unwrap() };
        assert_eq!(patch("{}").amount, None, "left out: unchanged");
        assert_eq!(patch(r#"{"amount":null}"#).amount, Some(None), "null: no fixed amount");
        assert_eq!(patch(r#"{"amount":"5.00"}"#).amount, Some(Some("5.00".into())));
    }

    #[test]
    fn create_starts_due_on_the_start_date() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        assert_eq!(item.next_due_date, "2026-01-31");
        assert_eq!(item.amount.as_deref(), Some("900.00"));
        assert!(item.is_active);
        assert_eq!(list(&conn).unwrap(), vec![item]);
    }

    #[test]
    fn create_rejects_bad_input() {
        let (conn, rent, _) = db();
        let bad = |f: fn(&mut NewRecurring)| {
            let mut input = new(rent, "2026-01-31");
            f(&mut input);
            assert_eq!(create(&conn, &input), Err(AppError::InvalidInput));
        };
        bad(|i| i.name = " ".into());
        bad(|i| i.amount = Some("0.00".into()));
        bad(|i| i.interval_count = 0);
        bad(|i| i.interval_count = MAX_INTERVAL_COUNT + 1);
        bad(|i| i.start_date = "2026-1-31".into());
        bad(|i| i.end_date = Some("2026-01-30".into()));
        bad(|i| i.category_id = "999".into());
    }

    #[test]
    fn schedule_edit_keeps_the_item_on_its_upcoming_date() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        conn.execute("UPDATE recurring_transactions SET next_due_date = '2026-03-31'", []).unwrap();
        let patch = RecurringPatch { interval_unit: Some(Unit::Week), interval_count: Some(2), ..Default::default() };
        let edited = update(&conn, item.id, &patch).unwrap();
        // 31 Jan + 2 weeks * k: ..., 28 Mar, 11 Apr. First on or after 31 Mar is 11 Apr.
        assert_eq!(edited.next_due_date, "2026-04-11");
    }

    #[test]
    fn plain_edit_changes_only_what_was_sent() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        let patch = RecurringPatch { amount: Some(Some("950.00".into())), end_date: Some(Some("2026-12-31".into())), ..Default::default() };
        let edited = update(&conn, item.id, &patch).unwrap();
        assert_eq!((edited.amount.as_deref(), edited.end_date.as_deref()), (Some("950.00"), Some("2026-12-31")));
        assert_eq!(edited.next_due_date, "2026-01-31");
        let cleared = update(&conn, item.id, &RecurringPatch { end_date: Some(None), ..Default::default() }).unwrap();
        assert_eq!(cleared.end_date, None);
        assert_eq!(update(&conn, 999, &patch), Err(AppError::NotFound));
    }

    #[test]
    fn delete_keeps_paid_transactions() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        conn.execute(
            "INSERT INTO transactions (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, payment_date)
             VALUES (?1, ?2, '2026-01-31', 'Rent', 90000, '2026-01-31')",
            params![rent, item.id],
        )
        .unwrap();
        delete(&conn, item.id).unwrap();
        let left: i64 = conn.query_row("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 1);
        assert_eq!(delete(&conn, item.id), Err(AppError::NotFound));
    }

    const TODAY: &str = "2026-03-15";

    #[test]
    fn pay_makes_a_transaction_and_moves_on() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        let paid = pay(&conn, item.id, None, Some("2026-01-30"), TODAY).unwrap();
        assert_eq!((paid.amount.as_str(), paid.payment_date.as_str(), paid.name.as_str()), ("900.00", "2026-01-30", "Rent"));
        assert_eq!(get(&conn, item.id).unwrap().next_due_date, "2026-02-28");
        let next = pay(&conn, item.id, None, None, TODAY).unwrap();
        assert_eq!(next.payment_date, TODAY);
        assert_eq!(get(&conn, item.id).unwrap().next_due_date, "2026-03-31");
    }

    #[test]
    fn pay_keeps_planned_and_actual_amounts() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        let paid = pay(&conn, item.id, Some("87.35"), None, TODAY).unwrap();
        assert_eq!(paid.amount, "87.35");
        let (planned, due, link): (i64, String, i64) = conn
            .query_row(
                "SELECT planned_amount_minor, recurring_due_date, recurring_transaction_id FROM transactions WHERE id = ?1",
                [paid.id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((planned, due.as_str(), link), (90000, "2026-01-31", item.id));
    }

    #[test]
    fn paid_transaction_counts_in_its_category() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-03-01")).unwrap();
        pay(&conn, item.id, None, Some("2026-03-01"), TODAY).unwrap();
        let spent: i64 = conn
            .query_row("SELECT sum(amount_minor) FROM transactions WHERE category_id = ?1 AND payment_date LIKE '2026-03%'", [rent], |r| r.get(0))
            .unwrap();
        assert_eq!(spent, 90000);
    }

    #[test]
    fn an_occurrence_cannot_be_paid_twice() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        pay(&conn, item.id, None, None, TODAY).unwrap();
        // Move the item back onto the paid occurrence, as a stale edit could.
        conn.execute("UPDATE recurring_transactions SET next_due_date = '2026-01-31'", []).unwrap();
        assert_eq!(pay(&conn, item.id, None, None, TODAY).unwrap_err(), AppError::InvalidInput);
        // The failed attempt changed nothing.
        assert_eq!(get(&conn, item.id).unwrap().next_due_date, "2026-01-31");
        let count: i64 = conn.query_row("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn pay_rejects_bad_amount_and_inactive_items() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        assert_eq!(pay(&conn, item.id, Some("0.00"), None, TODAY).unwrap_err(), AppError::InvalidInput);
        assert_eq!(pay(&conn, item.id, None, Some("nope"), TODAY).unwrap_err(), AppError::InvalidInput);
        set_active(&conn, item.id, false).unwrap();
        assert_eq!(pay(&conn, item.id, None, None, TODAY).unwrap_err(), AppError::InvalidInput);
        assert_eq!(pay(&conn, 999, None, None, TODAY).unwrap_err(), AppError::NotFound);
    }

    #[test]
    fn the_last_payment_before_the_end_date_finishes_the_item() {
        let (conn, rent, _) = db();
        let mut input = new(rent, "2026-01-31");
        input.end_date = Some("2026-02-28".into());
        let item = create(&conn, &input).unwrap();
        pay(&conn, item.id, None, None, TODAY).unwrap();
        assert!(get(&conn, item.id).unwrap().is_active);
        pay(&conn, item.id, None, None, TODAY).unwrap();
        let done = get(&conn, item.id).unwrap();
        assert!(!done.is_active);
        assert_eq!(set_active(&conn, item.id, true).unwrap_err(), AppError::InvalidInput);
    }

    #[test]
    fn skip_moves_on_without_a_transaction() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        assert_eq!(skip(&conn, item.id).unwrap().next_due_date, "2026-02-28");
        let count: i64 = conn.query_row("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
        let logged: i64 = conn
            .query_row("SELECT count(*) FROM audit_log WHERE entity_type = 'recurring_transactions' AND action = 'UPDATE'", [], |r| r.get(0))
            .unwrap();
        assert!(logged >= 1);
    }

    #[test]
    fn pause_resume_and_end() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        assert!(!set_active(&conn, item.id, false).unwrap().is_active);
        assert!(set_active(&conn, item.id, true).unwrap().is_active);
        let ended = end(&conn, item.id, TODAY).unwrap();
        assert!(!ended.is_active);
        assert_eq!(ended.end_date.as_deref(), Some(TODAY));
        // Ending before the start date cannot put the end date before it.
        let future = create(&conn, &new(rent, "2026-09-01")).unwrap();
        assert_eq!(end(&conn, future.id, TODAY).unwrap().end_date.as_deref(), Some("2026-09-01"));
    }

    #[test]
    fn dues_split_overdue_and_upcoming_by_the_given_today() {
        let (conn, rent, salary) = db();
        let late = create(&conn, &new(rent, "2026-03-14")).unwrap();
        let today = create(&conn, &new(rent, "2026-03-15")).unwrap();
        let soon = create(&conn, &new(salary, "2026-04-14")).unwrap();
        let _far = create(&conn, &new(salary, "2026-04-15")).unwrap();
        let paused = create(&conn, &new(rent, "2026-03-01")).unwrap();
        set_active(&conn, paused.id, false).unwrap();
        let d = dues(&conn, TODAY, None).unwrap();
        assert_eq!(d.overdue.iter().map(|r| r.id).collect::<Vec<_>>(), vec![late.id]);
        assert_eq!(d.upcoming.iter().map(|r| r.id).collect::<Vec<_>>(), vec![today.id, soon.id]);
        // One day earlier in a timezone behind, the same item is not overdue yet.
        let behind = dues(&conn, "2026-03-14", Some(0)).unwrap();
        assert!(behind.overdue.is_empty());
        assert_eq!(behind.upcoming.iter().map(|r| r.id).collect::<Vec<_>>(), vec![late.id]);
        assert_eq!(dues(&conn, TODAY, Some(MAX_WITHIN_DAYS + 1)).unwrap_err(), AppError::InvalidInput);
    }
}
