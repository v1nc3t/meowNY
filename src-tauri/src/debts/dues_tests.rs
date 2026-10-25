use super::dues::{due, due_payments, total, MAX_PER_ITEM};
use super::fixtures::world;
use crate::money::format_amount;
use crate::recurring::{self, NewRecurring, Unit};

fn item(conn: &rusqlite::Connection, name: &str, amount: Option<&str>, category: i64, start: &str, every: (Unit, u32), end: Option<&str>) -> i64 {
    recurring::create(
        conn,
        &NewRecurring {
            name: name.into(),
            amount: amount.map(str::to_string),
            category_id: category.to_string(),
            start_date: start.into(),
            interval_unit: every.0,
            interval_count: every.1,
            end_date: end.map(str::to_string),
            description: None,
        },
    )
    .unwrap()
    .id
}

fn days(conn: &rusqlite::Connection, today: &str) -> Vec<(String, String)> {
    due_payments(conn, today).unwrap().into_iter().map(|d| (d.name, d.due_date)).collect()
}

#[test]
fn every_occurrence_before_today_that_was_not_paid_is_a_payment_and_the_day_itself_is_not() {
    let w = world();
    item(&w.conn, "Rent", Some("900.00"), w.rent, "2026-01-01", (Unit::Month, 1), None);
    assert_eq!(days(&w.conn, "2025-12-31"), Vec::<(String, String)>::new(), "the day before the first one");
    assert_eq!(days(&w.conn, "2026-01-01"), Vec::<(String, String)>::new(), "the day it falls due is not yet a debt");
    assert_eq!(days(&w.conn, "2026-01-02"), [("Rent".to_string(), "2026-01-01".to_string())], "the day after");
    assert_eq!(due_payments(&w.conn, "2026-01-02").unwrap()[0].days_overdue, 1);
    assert_eq!(days(&w.conn, "2026-03-01").len(), 2, "January and February; March falls due today");
    assert_eq!(days(&w.conn, "2026-03-02").len(), 3);
    assert_eq!(days(&w.conn, "2026-03-15").len(), 3, "January, February and March, none of them paid");
    // the days since it fell due
    let list = due_payments(&w.conn, "2026-03-15").unwrap();
    assert_eq!(list.iter().map(|d| d.days_overdue).collect::<Vec<_>>(), [73, 42, 14]);
    // the same item in the oldest-first order of every day
    let dates: Vec<&str> = list.iter().map(|d| d.due_date.as_str()).collect();
    assert_eq!(dates, ["2026-01-01", "2026-02-01", "2026-03-01"]);
}

#[test]
fn paying_or_skipping_the_oldest_takes_it_off_the_list() {
    let w = world();
    let id = item(&w.conn, "Rent", Some("900.00"), w.rent, "2026-01-01", (Unit::Month, 1), None);
    assert_eq!(days(&w.conn, "2026-03-15").len(), 3);
    recurring::pay(&w.conn, id, None, Some("2026-01-03"), "2026-03-15", None).unwrap();
    assert_eq!(days(&w.conn, "2026-03-15").iter().map(|d| d.1.as_str()).collect::<Vec<_>>(), ["2026-02-01", "2026-03-01"]);
    recurring::skip(&w.conn, id, None).unwrap();
    assert_eq!(days(&w.conn, "2026-03-15").iter().map(|d| d.1.as_str()).collect::<Vec<_>>(), ["2026-03-01"]);
    recurring::pay(&w.conn, id, None, Some("2026-03-15"), "2026-03-15", None).unwrap();
    assert!(days(&w.conn, "2026-03-15").is_empty());
}

#[test]
fn a_payment_paid_or_skipped_on_its_own_is_left_out_and_does_not_count_toward_the_most_per_item() {
    let w = world();
    let id = item(&w.conn, "Rent", Some("900.00"), w.rent, "2026-01-01", (Unit::Month, 1), None);
    recurring::skip(&w.conn, id, Some("2026-02-01")).unwrap();
    recurring::pay(&w.conn, id, None, Some("2026-03-15"), "2026-03-15", Some("2026-03-01")).unwrap();
    assert_eq!(days(&w.conn, "2026-03-15").iter().map(|d| d.1.as_str()).collect::<Vec<_>>(), ["2026-01-01"]);
    let daily = item(&w.conn, "Daily", Some("1.00"), w.food, "2020-01-01", (Unit::Day, 1), None);
    recurring::skip(&w.conn, daily, Some("2020-01-03")).unwrap();
    let list: Vec<String> = due_payments(&w.conn, "2026-03-15").unwrap().into_iter().filter(|d| d.name == "Daily").map(|d| d.due_date).collect();
    assert_eq!(list.len(), MAX_PER_ITEM);
    assert!(!list.contains(&"2020-01-03".to_string()));
    assert_eq!(list[MAX_PER_ITEM - 1], "2020-01-25");
}

#[test]
fn what_is_not_a_payment_you_owe_is_not_listed() {
    let w = world();
    // a recurring income, a paused item, an item that has ended, and one that is not due yet
    item(&w.conn, "Wages", Some("3000.00"), w.salary, "2026-01-01", (Unit::Month, 1), None);
    let paused = item(&w.conn, "Paused", Some("5.00"), w.food, "2026-01-01", (Unit::Month, 1), None);
    recurring::set_active(&w.conn, paused, false).unwrap();
    item(&w.conn, "Later", Some("5.00"), w.food, "2026-06-01", (Unit::Month, 1), None);
    item(&w.conn, "Ended", Some("10.00"), w.food, "2026-01-01", (Unit::Month, 1), Some("2026-02-10"));
    // the item that ended stops at its end date: January and February, not March
    assert_eq!(days(&w.conn, "2026-03-15"), [("Ended".to_string(), "2026-01-01".to_string()), ("Ended".to_string(), "2026-02-01".to_string())]);
}

#[test]
fn an_item_forgotten_for_years_is_cut_at_the_most_per_item() {
    let w = world();
    item(&w.conn, "Daily", Some("1.00"), w.food, "2020-01-01", (Unit::Day, 1), None);
    let list = due_payments(&w.conn, "2026-03-15").unwrap();
    assert_eq!(list.len(), MAX_PER_ITEM);
    assert_eq!(list[0].due_date, "2020-01-01", "the oldest are the ones kept: they are the ones that have been due the longest");
    assert_eq!(list[MAX_PER_ITEM - 1].due_date, "2020-01-24");
}

#[test]
fn a_month_that_is_short_lands_on_its_last_day_the_way_the_recurring_screen_does() {
    let w = world();
    item(&w.conn, "End of month", Some("10.00"), w.food, "2026-01-31", (Unit::Month, 1), None);
    let dates: Vec<String> = days(&w.conn, "2026-05-01").into_iter().map(|d| d.1).collect();
    assert_eq!(dates, ["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]);
    assert_eq!(days(&w.conn, "2026-04-30").len(), 3, "the 30th itself is not yet a debt");
}

#[test]
fn the_total_counts_what_is_known_and_says_how_many_are_not() {
    let w = world();
    item(&w.conn, "Rent", Some("900.00"), w.rent, "2026-01-01", (Unit::Month, 1), Some("2026-02-15"));
    item(&w.conn, "Phone", None, w.food, "2026-02-01", (Unit::Month, 1), Some("2026-02-15"));
    let payments = due_payments(&w.conn, "2026-03-15").unwrap();
    assert_eq!(total(&payments), (180_000, 1));
    let screen = due(&w.conn, "2026-03-15").unwrap();
    assert_eq!((screen.total.as_str(), screen.unknown, screen.payments.len()), ("1800.00", 1, 3));
    assert_eq!(format_amount(180_000), screen.total);
    let json = serde_json::to_value(&screen.payments[0]).unwrap();
    assert!(json["recurringId"].is_string() && json["categoryId"].is_string() && json["amountMinor"].is_null(), "the minor units stay inside: {json}");
}

#[test]
fn a_day_that_is_not_a_date_is_refused() {
    let w = world();
    assert!(due_payments(&w.conn, "2026-02-30").is_err());
    assert!(due_payments(&w.conn, "today").is_err());
}
