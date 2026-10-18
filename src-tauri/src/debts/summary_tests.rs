use super::fixtures::*;
use super::ledger::{self, Kind, NewEntry};
use super::people;
use super::splits::set_split;
use super::summary::{summary, summary_csv};

fn entry(conn: &rusqlite::Connection, person: i64, kind: Kind, amount: &str, date: &str, note: Option<&str>) {
    ledger::add(conn, &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: date.into(), note: note.map(str::to_string), transaction_id: None }).unwrap();
}

#[test]
fn an_archived_friend_who_still_owes_is_in_the_totals() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    entry(&w.conn, anna, Kind::Lent, "25.00", "2026-01-01", None);
    people::archive(&w.conn, anna).unwrap();
    assert_eq!(summary(&w.conn).unwrap().owed_to_you, "25.00", "the money is still owed");
}

#[test]
fn the_totals_include_the_parts_of_shared_bills() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-01");
    set_split(&w.conn, dinner, &equal(vec![who(anna), who(bob)])).unwrap();
    let s = summary(&w.conn).unwrap();
    assert_eq!((s.owed_to_you.as_str(), s.you_owe.as_str()), ("60.00", "0.00"));
}

#[test]
fn the_csv_summary_has_a_signed_balance_and_cannot_run_as_a_formula() {
    let w = world();
    let (evil, quote) = (friend(&w.conn, "=HYPERLINK(\"evil\")"), friend(&w.conn, "Smith, \"Jo\""));
    entry(&w.conn, evil, Kind::Borrowed, "7.50", "2026-01-01", None);
    entry(&w.conn, quote, Kind::Lent, "1.00", "2026-01-01", None);
    let csv = summary_csv(&w.conn).unwrap();
    assert!(csv.starts_with("\u{feff}name,balance,currency\r\n"));
    let rows: Vec<&str> = csv.trim_end().split("\r\n").skip(1).collect();
    assert_eq!(rows[0], "\"'=HYPERLINK(\"\"evil\"\")\",-7.50,EUR", "{csv}");
    assert_eq!(rows[1], "\"Smith, \"\"Jo\"\"\",1.00,EUR");
    assert_eq!(rows.len(), 2);
}
