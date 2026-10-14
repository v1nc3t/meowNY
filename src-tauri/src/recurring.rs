//! Recurring items: templates for income or expenses that repeat. Paying one copies it into a
//! normal transaction (see `pay`). Due dates always come from `start_date`, never from the
//! previous due date, so "monthly from 31 Jan" gives 28 Feb, 31 Mar and does not drift.

use chrono::{Days, Months, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Deserializer, Serialize};

use crate::categories::{parse_id, serialize_id};
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
    /// Decimal string, e.g. "12.50".
    pub amount: String,
    pub start_date: String,
    pub interval_unit: Unit,
    pub interval_count: u32,
    pub next_due_date: String,
    pub end_date: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRecurring {
    pub name: String,
    pub amount: String,
    pub category_id: String,
    pub start_date: String,
    pub interval_unit: Unit,
    pub interval_count: u32,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Fields left out stay as they are. `description: null` and `endDate: null` clear them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecurringPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub amount: Option<String>,
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
        amount: format_amount(row.get(4)?),
        start_date: row.get(5)?,
        interval_unit: Unit::parse(&unit).map_err(|_| rusqlite::Error::InvalidQuery)?,
        interval_count: row.get(7)?,
        next_due_date: row.get(8)?,
        end_date: row.get(9)?,
        is_active: row.get::<_, i64>(10)? == 1,
    })
}

pub fn find(conn: &Connection, id: i64) -> Result<Option<Recurring>, AppError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM recurring_transactions WHERE id = ?1"), [id], from_row).optional()?)
}

pub fn get(conn: &Connection, id: i64) -> Result<Recurring, AppError> {
    find(conn, id)?.ok_or(AppError::NotFound)
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
    let minor = parse_positive_amount(&input.amount)?;
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
    let minor = parse_positive_amount(patch.amount.as_deref().unwrap_or(&current.amount))?;
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
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
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
            amount: "900.00".into(),
            category_id: category.to_string(),
            start_date: start.into(),
            interval_unit: Unit::Month,
            interval_count: 1,
            end_date: None,
            description: None,
        }
    }

    #[test]
    fn create_starts_due_on_the_start_date() {
        let (conn, rent, _) = db();
        let item = create(&conn, &new(rent, "2026-01-31")).unwrap();
        assert_eq!(item.next_due_date, "2026-01-31");
        assert_eq!(item.amount, "900.00");
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
        bad(|i| i.amount = "0.00".into());
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
        let patch = RecurringPatch { amount: Some("950.00".into()), end_date: Some(Some("2026-12-31".into())), ..Default::default() };
        let edited = update(&conn, item.id, &patch).unwrap();
        assert_eq!((edited.amount.as_str(), edited.end_date.as_deref()), ("950.00", Some("2026-12-31")));
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
}
