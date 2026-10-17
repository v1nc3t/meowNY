use super::fixtures::*;
use super::ledger::{self, Kind, NewEntry};
use super::people;
use super::splits::set_split;
use super::summary::{ledger_csv, statement_csv, statement_text, summary, summary_csv, summary_text, Labels};
use crate::vault::AppError;

fn entry(conn: &rusqlite::Connection, person: i64, kind: Kind, amount: &str, date: &str, note: Option<&str>) {
    ledger::add(conn, &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: date.into(), note: note.map(str::to_string), transaction_id: None }).unwrap();
}

#[test]
fn the_totals_are_what_friends_owe_you_and_what_you_owe_them() {
    let w = world();
    let (anna, bob, carl, dina) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"), friend(&w.conn, "Carl"), friend(&w.conn, "Dina"));
    entry(&w.conn, anna, Kind::Lent, "25.00", "2026-01-01", None);
    entry(&w.conn, bob, Kind::Lent, "10.00", "2026-01-01", None);
    entry(&w.conn, carl, Kind::Borrowed, "7.50", "2026-01-01", None);
    entry(&w.conn, dina, Kind::Lent, "5.00", "2026-01-01", None);
    entry(&w.conn, dina, Kind::RepaidToMe, "5.00", "2026-01-02", None);
    let s = summary(&w.conn).unwrap();
    assert_eq!((s.owed_to_you.as_str(), s.you_owe.as_str(), s.net.as_str()), ("35.00", "7.50", "27.50"));
    assert_eq!(s.people.iter().map(|p| (p.name.as_str(), p.balance.as_str())).collect::<Vec<_>>(), [("Anna", "25.00"), ("Bob", "10.00"), ("Carl", "-7.50")], "settled friends are not listed, the largest first");
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["owedToYou"], "35.00");
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
fn the_text_summary_lists_each_friend_and_the_direction() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bartholomew"));
    entry(&w.conn, anna, Kind::Lent, "25.00", "2026-01-01", None);
    entry(&w.conn, bob, Kind::Borrowed, "7.50", "2026-01-01", None);
    let text = summary_text(&w.conn, "2026-10-17", &Labels::english()).unwrap();
    assert_eq!(
        text,
        "Who owes what\n2026-10-17 (EUR)\n\nOwed to you: 25.00\nYou owe: 7.50\nNet: 17.50\n\nAnna         owes you  25.00\nBartholomew  you owe    7.50\n"
    );
}

#[test]
fn nobody_owing_anything_says_so() {
    let w = world();
    friend(&w.conn, "Anna");
    let text = summary_text(&w.conn, "2026-10-17", &Labels::english()).unwrap();
    assert_eq!(text, "Who owes what\n2026-10-17 (EUR)\n\nNobody owes anything.\n");
    assert_eq!(summary_csv(&w.conn).unwrap(), "\u{feff}name,balance,currency\r\n");
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

#[test]
fn a_statement_is_the_story_of_one_friend() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-05");
    set_split(&w.conn, dinner, &equal(vec![who(anna)])).unwrap();
    entry(&w.conn, anna, Kind::RepaidToMe, "20.00", "2026-01-20", Some("cash"));
    let text = statement_text(&w.conn, anna, "2026-10-17", &Labels::english()).unwrap();
    assert_eq!(
        text,
        "Statement: Anna\n2026-10-17 (EUR)\n\n2026-01-05  share of a bill: Dinner  +45.00  45.00\n2026-01-20  paid back to me: cash    -20.00  25.00\n\nAnna owes you 25.00\n"
    );
    let csv = statement_csv(&w.conn, anna).unwrap();
    assert_eq!(
        csv,
        "\u{feff}date,kind,what,amount,effect,balance,currency\r\n2026-01-05,BILL,Dinner,45.00,45.00,45.00,EUR\r\n2026-01-20,REPAID_TO_ME,cash,20.00,-20.00,25.00,EUR\r\n"
    );
    ledger::settle(&w.conn, anna, None, None, None, "2026-02-01").unwrap();
    assert!(statement_text(&w.conn, anna, "2026-10-17", &Labels::english()).unwrap().ends_with("\nAll settled.\n"));
    let carl = friend(&w.conn, "Carl");
    entry(&w.conn, carl, Kind::Borrowed, "3.00", "2026-01-01", None);
    assert!(statement_text(&w.conn, carl, "2026-10-17", &Labels::english()).unwrap().ends_with("\nCarl you owe 3.00\n"));
    assert_eq!(statement_text(&w.conn, 99, "2026-10-17", &Labels::english()), Err(AppError::NotFound));
    assert_eq!(statement_csv(&w.conn, 99), Err(AppError::NotFound));
}

#[test]
fn the_ledger_csv_has_every_entry_and_nothing_from_the_bills() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-05");
    set_split(&w.conn, dinner, &equal(vec![who(anna)])).unwrap();
    entry(&w.conn, bob, Kind::Borrowed, "3.00", "2026-01-09", Some("=1+1"));
    entry(&w.conn, anna, Kind::Lent, "5.00", "2026-01-02", None);
    let csv = ledger_csv(&w.conn).unwrap();
    assert_eq!(csv, "\u{feff}date,friend,kind,amount,currency,note\r\n2026-01-02,Anna,LENT,5.00,EUR,\r\n2026-01-09,Bob,BORROWED,3.00,EUR,'=1+1\r\n");
}
