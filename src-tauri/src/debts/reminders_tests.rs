use super::fixtures::*;
use super::ledger::{self, Kind, NewEntry};
use super::people;
use super::reminders::{days_from, latest_until, reminders, Day, SCHEDULED_DAYS};
use crate::vault::AppError;

fn lend(conn: &rusqlite::Connection, person: i64, kind: Kind, amount: &str) {
    ledger::add(conn, &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: "2026-10-01".into(), note: None, transaction_id: None }).unwrap();
}

#[test]
fn the_day_a_reminder_last_came_is_its_first_day_or_the_last_step_after_it() {
    // once only
    assert_eq!(latest_until("2026-10-20", None, "2026-10-19"), None, "not yet");
    assert_eq!(latest_until("2026-10-20", None, "2026-10-20").as_deref(), Some("2026-10-20"));
    assert_eq!(latest_until("2026-10-20", None, "2027-01-01").as_deref(), Some("2026-10-20"), "a missed one stays due");
    // every 7 days
    for (today, latest) in [("2026-10-20", "2026-10-20"), ("2026-10-26", "2026-10-20"), ("2026-10-27", "2026-10-27"), ("2026-11-02", "2026-10-27"), ("2026-11-03", "2026-11-03")] {
        assert_eq!(latest_until("2026-10-20", Some(7), today).as_deref(), Some(latest), "today {today}");
    }
    // across a leap day and a year end
    assert_eq!(latest_until("2028-02-27", Some(2), "2028-03-02").as_deref(), Some("2028-03-02"));
    assert_eq!(latest_until("2026-12-30", Some(3), "2027-01-02").as_deref(), Some("2027-01-02"));
    // damaged input is nothing, not a panic
    for (on, today) in [("", "2026-10-20"), ("2026-13-01", "2026-10-20"), ("2026-10-20", ""), ("x", "y")] {
        assert_eq!(latest_until(on, Some(7), today), None, "{on:?} {today:?}");
    }
    assert_eq!(latest_until("2026-10-20", Some(0), "2026-10-30").as_deref(), Some("2026-10-20"), "a repeat of 0 days is no repeat");
}

#[test]
fn the_next_days_of_a_reminder_start_today_and_a_once_only_one_has_at_most_one() {
    assert_eq!(days_from("2026-10-20", None, "2026-10-17", 4), ["2026-10-20"]);
    assert_eq!(days_from("2026-10-20", None, "2026-10-20", 4), ["2026-10-20"], "today counts");
    assert!(days_from("2026-10-20", None, "2026-10-21", 4).is_empty(), "a once-only reminder that has passed has no more days");
    assert_eq!(days_from("2026-10-20", Some(7), "2026-10-17", 3), ["2026-10-20", "2026-10-27", "2026-11-03"]);
    assert_eq!(days_from("2026-10-20", Some(7), "2026-10-27", 2), ["2026-10-27", "2026-11-03"], "today counts when it is a day of the reminder");
    assert_eq!(days_from("2026-10-20", Some(7), "2026-10-28", 2), ["2026-11-03", "2026-11-10"], "otherwise the next one");
    assert_eq!(days_from("2026-10-20", Some(7), "2030-01-01", 1).len(), 1);
    assert!(days_from("2026-10-20", Some(7), "2026-10-17", 0).is_empty());
    assert!(days_from("bad", Some(7), "2026-10-17", 3).is_empty() && days_from("2026-10-20", Some(7), "bad", 3).is_empty());
    for day in days_from("2026-01-31", Some(30), "2026-01-01", 10) {
        assert!(chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d").is_ok(), "{day}");
    }
}

#[test]
fn a_reminder_is_live_only_while_somebody_owes_something() {
    let w = world();
    let (anna, bob, cleo, dave) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"), friend(&w.conn, "Cleo"), friend(&w.conn, "Dave"));
    for (id, on, every) in [(anna, "2026-10-10", None), (bob, "2026-10-20", Some(7)), (cleo, "2026-10-17", None), (dave, "2026-10-12", None)] {
        people::set_reminder(&w.conn, id, Some(on), every).unwrap();
    }
    lend(&w.conn, anna, Kind::Lent, "10.00");
    lend(&w.conn, bob, Kind::Borrowed, "5.00");
    lend(&w.conn, cleo, Kind::Lent, "1.00");
    // Dave has a reminder and no debt
    let on_17 = reminders(&w.conn, "2026-10-17").unwrap();
    assert_eq!(on_17.today, "2026-10-17");
    let due: Vec<(&str, &str)> = on_17.due.iter().map(|d| (d.name.as_str(), d.due_on.as_str())).collect();
    assert_eq!(due, [("Anna", "2026-10-10"), ("Cleo", "2026-10-17")], "Bob's has not come, Dave owes nothing");
    assert_eq!(on_17.upcoming, [Day { date: "2026-10-17".into(), count: 1 }, Day { date: "2026-10-20".into(), count: 1 }, Day { date: "2026-10-27".into(), count: 1 }, Day { date: "2026-11-03".into(), count: 1 }]);
    // a debt paid back by hand makes the reminder quiet, and a new one makes it live again
    lend(&w.conn, anna, Kind::RepaidToMe, "10.00");
    assert!(!reminders(&w.conn, "2026-10-17").unwrap().due.iter().any(|d| d.name == "Anna"));
    lend(&w.conn, anna, Kind::Lent, "3.00");
    assert!(reminders(&w.conn, "2026-10-17").unwrap().due.iter().any(|d| d.name == "Anna"));
    // marking as paid in the app clears the reminder itself
    ledger::settle(&w.conn, cleo, None, None, None, "2026-10-17").unwrap();
    assert!(people::get(&w.conn, cleo).unwrap().remind_on.is_none());
    assert!(!reminders(&w.conn, "2026-10-17").unwrap().due.iter().any(|d| d.name == "Cleo"));
}

#[test]
fn a_friend_who_is_off_the_list_is_still_reminded_and_days_are_counted_by_friends() {
    let w = world();
    let (anna, bob, cleo) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"), friend(&w.conn, "Cleo"));
    for id in [anna, bob] {
        lend(&w.conn, id, Kind::Lent, "10.00");
        people::set_reminder(&w.conn, id, Some("2026-10-20"), None).unwrap();
    }
    lend(&w.conn, cleo, Kind::Lent, "10.00");
    people::set_reminder(&w.conn, cleo, Some("2026-10-21"), Some(1)).unwrap();
    people::archive(&w.conn, anna).unwrap();
    let r = reminders(&w.conn, "2026-10-19").unwrap();
    assert!(r.due.is_empty());
    assert_eq!(r.upcoming.len(), SCHEDULED_DAYS, "at most four days");
    assert_eq!((r.upcoming[0].date.as_str(), r.upcoming[0].count), ("2026-10-20", 2), "an archived friend counts: the money is still owed");
    assert_eq!((r.upcoming[1].date.as_str(), r.upcoming[1].count), ("2026-10-21", 1));
    assert!(r.upcoming.windows(2).all(|w| w[0].date < w[1].date), "the days are in order and distinct");
}

#[test]
fn a_day_that_is_not_a_day_is_refused() {
    let w = world();
    for today in ["", "2026-13-01", "tomorrow", "2026-10-17T10:00"] {
        assert_eq!(reminders(&w.conn, today), Err(AppError::InvalidInput), "{today:?}");
    }
}
