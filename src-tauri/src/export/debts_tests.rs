//! Friends, the ledger and the parts of shared bills in the exports: all of it with no limit, and with a limit nothing about the rest.

use serde_json::Value;

use super::*;
use crate::categories;
use crate::debts::fixtures::*;
use crate::debts::ledger::{self, Kind, NewEntry};
use crate::debts::splits::set_split;
use crate::debts::people;

struct Case {
    conn: Connection,
    food: i64,
}

/// A shared dinner (Food) and a shared therapy bill (a category that must not leak), a loan to Anna, a loan to Dina who is on no bill.
fn case() -> Case {
    let w = world();
    let conn = w.conn;
    let secret = categories::create(&conn, "Therapy", categories::CategoryType::Expense).unwrap().id;
    let (anna, bob, eve, dina) = (friend(&conn, "Anna"), friend(&conn, "Bob"), friend(&conn, "Evelyn"), friend(&conn, "Dina"));
    let dinner = bill(&conn, w.food, "Dinner", "90.00", "2026-03-05");
    set_split(&conn, dinner, &equal(vec![who(anna), who(bob), guest("Carl")])).unwrap();
    let session = bill(&conn, secret, "Session", "80.00", "2026-03-06");
    set_split(&conn, session, &equal(vec![who(eve), guest("Hidden Guest")])).unwrap();
    bill(&conn, w.food, "Bread", "2.00", "2026-03-07");
    let entry = |person: i64, kind: Kind, amount: &str, date: &str, note: Option<&str>| {
        ledger::add(&conn, &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: date.into(), note: note.map(str::to_string), transaction_id: None }).unwrap();
    };
    entry(anna, Kind::Lent, "10.00", "2026-03-01", Some("taxi"));
    entry(dina, Kind::Borrowed, "7.00", "2026-04-01", Some("lunch"));
    people::set_reminder(&conn, bob, Some("2026-05-01"), Some(7)).unwrap();
    Case { conn, food: w.food }
}

fn json_of(conn: &Connection, selection: &Selection) -> Value {
    serde_json::from_str(&json(conn, selection, "2026-10-17T00:00:00Z".parse().unwrap()).unwrap()).unwrap()
}

fn names(list: &Value) -> Vec<String> {
    list.as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap().to_string()).collect()
}

#[test]
fn a_limit_keeps_to_what_is_in_it() {
    let c = case();
    let selection = Selection { category_ids: Some(vec![c.food.to_string()]), ..Default::default() };
    let file = json_of(&c.conn, &selection);
    assert_eq!(names(&file["people"]), ["Anna", "Bob"], "only the friends on the bills that are in it");
    assert!(file["ledger"].as_array().unwrap().is_empty(), "and no ledger unless asked");
    assert!(file["people"][0]["balance"].is_null() && file["people"][0]["remindOn"].is_null() && file["people"][1]["remindOn"].is_null(), "no balance and no reminder: they would add up what the file leaves out");
    let text = serde_json::to_string(&file).unwrap();
    for secret in ["Evelyn", "Hidden Guest", "Session", "Therapy", "Dina", "lunch"] {
        assert!(!text.contains(secret), "{secret} leaked into a limited export: {text}");
    }
    assert!(text.contains("Carl") && text.contains("Anna"));
    assert!(!serde_json::to_string(&json_of(&c.conn, &Selection { category_ids: Some(vec![]), ..Default::default() })).unwrap().contains("Anna"));
}

#[test]
fn the_ledger_comes_with_a_limit_only_when_asked_and_by_the_dates() {
    let c = case();
    let asked = Selection { category_ids: Some(vec![c.food.to_string()]), include_debts: true, ..Default::default() };
    let file = json_of(&c.conn, &asked);
    assert_eq!(file["ledger"].as_array().unwrap().len(), 2, "a loan has no category, so the category does not limit it");
    assert_eq!(names(&file["people"]), ["Anna", "Bob", "Dina"], "and the friends in it");
    let march = Selection { from: Some("2026-03-01".into()), to: Some("2026-03-31".into()), include_debts: true, ..Default::default() };
    let file = json_of(&c.conn, &march);
    assert_eq!(file["ledger"].as_array().unwrap().len(), 1, "dates limit it");
    assert_eq!(file["ledger"][0]["note"], "taxi");
    assert!(!serde_json::to_string(&file).unwrap().contains("lunch"));
    // asking for the ledger when there is no limit changes nothing
    assert_eq!(json_of(&c.conn, &Selection { include_debts: true, ..Default::default() }), json_of(&c.conn, &Selection::default()));
}

#[test]
fn the_monthly_totals_of_an_export_are_what_the_bills_cost_you() {
    let c = case();
    let parsed = parse(&Selection::default()).unwrap();
    let sums = monthly_sums(&c.conn, &parsed).unwrap();
    // Dinner 22.50 + Session 26.66 + Bread 2.00, and the loans are in no sum
    assert_eq!(sums["2026-03"], (0, 2250 + 2667 + 200));
    assert_eq!(sums.len(), 1, "the loan of April is not a month of spending");
}

#[test]
fn a_name_that_looks_like_a_formula_is_text_in_the_exports() {
    let w = world();
    let evil = friend(&w.conn, "=HYPERLINK(\"evil\")");
    let id = bill(&w.conn, w.food, "Lunch", "10.00", "2026-03-01");
    set_split(&w.conn, id, &equal(vec![who(evil), guest("+cmd|' /C calc'!A0")])).unwrap();
    let file = serde_json::to_string(&json_of(&w.conn, &Selection::default())).unwrap();
    assert!(file.contains("HYPERLINK"), "JSON holds the name as it is");
    let bytes = workbook(&w.conn, &Selection::default(), &Default::default()).unwrap();
    assert_eq!(&bytes[..2], b"PK");
}
