use super::fixtures::{bill, equal, friend, who, world};
use super::history::{history, lines, Filter, Scope, PAYMENT_DUE, PAYMENT_PAID};
use super::ledger::{self, Kind, NewEntry};
use super::splits::set_split;
use crate::recurring::{self, NewRecurring, Unit};
use crate::vault::AppError;

const TODAY: &str = "2026-03-15";

fn entry(conn: &rusqlite::Connection, person: i64, kind: Kind, amount: &str, date: &str) {
    ledger::add(conn, &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: date.into(), note: None, transaction_id: None }).unwrap();
}

fn item(conn: &rusqlite::Connection, name: &str, category: i64, start: &str, end: &str) -> i64 {
    recurring::create(
        conn,
        &NewRecurring {
            name: name.into(),
            amount: Some("100.00".into()),
            category_id: category.to_string(),
            start_date: start.into(),
            interval_unit: Unit::Month,
            interval_count: 1,
            end_date: Some(end.into()),
            description: None,
        },
    )
    .unwrap()
    .id
}

/// (date, kind) of every line, oldest first.
fn all(conn: &rusqlite::Connection, scope: &Scope) -> Vec<(String, String)> {
    lines(conn, TODAY, scope).unwrap().into_iter().map(|l| (l.date, l.kind)).collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(d, k)| (d.to_string(), k.to_string())).collect()
}

#[test]
fn the_history_has_the_lines_of_the_friends_the_payments_that_were_paid_late_and_those_that_are_overdue() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    entry(&w.conn, anna, Kind::Lent, "25.00", "2026-01-02");
    entry(&w.conn, bob, Kind::Borrowed, "7.50", "2026-02-02");
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-20");
    set_split(&w.conn, dinner, &equal(vec![who(anna)])).unwrap();
    let rent = item(&w.conn, "Rent", w.rent, "2026-01-01", "2026-04-15");
    recurring::pay(&w.conn, rent, None, Some("2026-01-04"), TODAY, None, None).unwrap();
    // the payment on the day it was paid late (the 4th; it fell due on the 1st), the ones overdue now on the days they fell due
    assert_eq!(
        all(&w.conn, &Scope::default()),
        pairs(&[
            ("2026-01-02", "LENT"),
            ("2026-01-04", PAYMENT_PAID),
            ("2026-01-20", "BILL"),
            ("2026-02-01", PAYMENT_DUE),
            ("2026-02-02", "BORROWED"),
            ("2026-03-01", PAYMENT_DUE),
        ])
    );
    let every = lines(&w.conn, TODAY, &Scope::default()).unwrap();
    let paid = every.iter().find(|l| l.kind == PAYMENT_PAID).unwrap();
    assert_eq!((paid.what.as_str(), paid.amount.as_str(), paid.due_date.as_deref(), paid.friend.as_deref()), ("Rent", "100.00", Some("2026-01-01"), None));
    assert!(paid.transaction_id.is_some() && paid.recurring_id.is_some() && paid.balance.is_none() && paid.effect.is_none());
    let due = every.iter().find(|l| l.kind == PAYMENT_DUE && l.date == "2026-03-01").unwrap();
    assert_eq!((due.amount.as_str(), due.due_date.as_deref(), due.transaction_id.as_deref()), ("100.00", Some("2026-03-01"), None));
    // a friend's line says what it did and what the friend owed after it: the debts screens' own numbers
    let dinner_line = every.iter().find(|l| l.kind == "BILL").unwrap();
    assert_eq!((dinner_line.friend.as_deref(), dinner_line.effect.as_deref(), dinner_line.balance.as_deref()), (Some("Anna"), Some("45.00"), Some("70.00")));
}

#[test]
fn a_payment_made_on_time_is_not_in_the_history_and_one_made_late_is() {
    let w = world();
    let rent = item(&w.conn, "Rent", w.rent, "2026-01-01", "2026-06-15");
    // January paid the day before it fell due, February on its day, March a day late; April (the 1st) is not overdue on the 15th of March
    recurring::pay(&w.conn, rent, None, Some("2025-12-31"), TODAY, None, None).unwrap();
    recurring::pay(&w.conn, rent, None, Some("2026-02-01"), TODAY, None, None).unwrap();
    assert_eq!(all(&w.conn, &Scope::default()), pairs(&[("2026-03-01", PAYMENT_DUE)]), "paid in time: never overdue, so no line; March is overdue now");
    recurring::pay(&w.conn, rent, None, Some("2026-03-02"), TODAY, None, None).unwrap();
    assert_eq!(all(&w.conn, &Scope::default()), pairs(&[("2026-03-02", PAYMENT_PAID)]));
    let line = lines(&w.conn, TODAY, &Scope::default()).unwrap().remove(0);
    assert_eq!(line.due_date.as_deref(), Some("2026-03-01"), "it says which day it was late for");
    // the same count on the screen
    assert_eq!(history(&w.conn, TODAY, &Filter::default()).unwrap().total, 1);
}

#[test]
fn a_range_of_dates_keeps_the_lines_inside_it_both_ends_included_and_the_balances_stay_true() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    entry(&w.conn, anna, Kind::Lent, "25.00", "2026-01-02");
    entry(&w.conn, anna, Kind::Lent, "5.00", "2026-02-02");
    entry(&w.conn, anna, Kind::RepaidToMe, "10.00", "2026-03-02");
    let scope = Scope { from: Some("2026-02-02"), to: Some("2026-03-02"), person: None };
    assert_eq!(all(&w.conn, &scope), pairs(&[("2026-02-02", "LENT"), ("2026-03-02", "REPAID_TO_ME")]));
    let balances: Vec<String> = lines(&w.conn, TODAY, &scope).unwrap().into_iter().filter_map(|l| l.balance).collect();
    assert_eq!(balances, ["30.00", "20.00"], "the 25.00 before the range is in them");
    assert!(all(&w.conn, &Scope { from: Some("2027-01-01"), ..Default::default() }).is_empty());
}

#[test]
fn what_cannot_be_asked_is_refused() {
    let w = world();
    let ask = |filter: Filter| history(&w.conn, TODAY, &filter).err();
    assert_eq!(ask(Filter { limit: Some(0), ..Default::default() }), Some(AppError::InvalidInput));
    assert_eq!(ask(Filter { limit: Some(201), ..Default::default() }), Some(AppError::InvalidInput));
    assert_eq!(ask(Filter { from: Some("2026-03-01".into()), to: Some("2026-02-01".into()), ..Default::default() }), Some(AppError::InvalidInput));
    assert_eq!(ask(Filter { from: Some("2026-02-30".into()), ..Default::default() }), Some(AppError::InvalidInput));
    assert_eq!(ask(Filter { person_id: Some("0".into()), ..Default::default() }), Some(AppError::InvalidInput));
    assert_eq!(ask(Filter { person_id: Some("abc".into()), ..Default::default() }), Some(AppError::InvalidInput));
}
