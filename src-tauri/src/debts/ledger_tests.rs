use rusqlite::Connection;

use super::fixtures::*;
use super::ledger::{self, Expense, Kind, NewEntry, EntryPatch};
use super::people;
use super::splits::set_split;
use crate::transactions;
use crate::vault::AppError;

fn new(person: i64, kind: Kind, amount: &str, date: &str) -> NewEntry {
    NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: date.into(), note: None, transaction_id: None }
}

fn balance(conn: &Connection, person: i64) -> i64 {
    ledger::balance_minor(conn, person).unwrap()
}

#[test]
fn the_four_kinds_move_the_balance_the_way_the_design_says() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    for (kind, expected) in [(Kind::Lent, 1000), (Kind::RepaidToMe, 0), (Kind::Borrowed, -1000), (Kind::RepaidByMe, 0)] {
        ledger::add(&w.conn, &new(anna, kind, "10.00", "2026-01-01")).unwrap();
        assert_eq!(balance(&w.conn, anna), expected, "{kind:?}");
    }
    assert_eq!([Kind::Lent.sign(), Kind::RepaidByMe.sign(), Kind::Borrowed.sign(), Kind::RepaidToMe.sign()], [1, 1, -1, -1]);
}

#[test]
fn an_entry_is_checked_before_it_is_written() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "10.00", "2026-01-01");
    for bad in [
        new(anna, Kind::Lent, "0.00", "2026-01-01"),
        new(anna, Kind::Lent, "1", "2026-01-01"),
        new(anna, Kind::Lent, "-5.00", "2026-01-01"),
        new(anna, Kind::Lent, "10000000000.00", "2026-01-01"),
        new(anna, Kind::Lent, "5.00", "2026-02-30"),
        new(anna, Kind::Lent, "5.00", "01/02/2026"),
        new(99, Kind::Lent, "5.00", "2026-01-01"),
        NewEntry { person_id: "abc".into(), ..new(anna, Kind::Lent, "5.00", "2026-01-01") },
        NewEntry { note: Some("x".repeat(201)), ..new(anna, Kind::Lent, "5.00", "2026-01-01") },
        NewEntry { note: Some("bad\u{7}".into()), ..new(anna, Kind::Lent, "5.00", "2026-01-01") },
        NewEntry { transaction_id: Some("999".into()), ..new(anna, Kind::Lent, "5.00", "2026-01-01") },
    ] {
        assert_eq!(ledger::add(&w.conn, &bad), Err(AppError::InvalidInput), "{bad:?}");
    }
    assert_eq!(one(&w.conn, "SELECT count(*) FROM ledger_entries"), 0);
    let ok = ledger::add(&w.conn, &NewEntry { note: Some("  taxi  ".into()), transaction_id: Some(dinner.to_string()), ..new(anna, Kind::Lent, "5.00", "2026-01-01") }).unwrap();
    assert_eq!((ok.note.as_deref(), ok.transaction_id, ok.amount.as_str()), (Some("taxi"), Some(dinner.to_string()), "5.00"));
    let empty_note = ledger::add(&w.conn, &NewEntry { note: Some("   ".into()), ..new(anna, Kind::Lent, "5.00", "2026-01-01") }).unwrap();
    assert_eq!(empty_note.note, None, "an empty note is none");
}

#[test]
fn an_entry_can_be_added_for_an_archived_friend_because_a_repayment_can_come_late() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    ledger::add(&w.conn, &new(anna, Kind::Lent, "5.00", "2026-01-01")).unwrap();
    people::archive(&w.conn, anna).unwrap();
    assert!(ledger::add(&w.conn, &new(anna, Kind::RepaidToMe, "5.00", "2026-02-01")).is_ok());
    assert_eq!(balance(&w.conn, anna), 0);
}

#[test]
fn an_entry_is_changed_field_by_field_and_a_null_clears() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let dinner = bill(&w.conn, w.food, "Dinner", "10.00", "2026-01-01");
    let made = ledger::add(&w.conn, &NewEntry { note: Some("taxi".into()), transaction_id: Some(dinner.to_string()), ..new(anna, Kind::Lent, "5.00", "2026-01-01") }).unwrap();
    let same = ledger::update(&w.conn, made.id, &EntryPatch::default()).unwrap();
    assert_eq!(same, made, "nothing given, nothing changed");
    let changed = ledger::update(&w.conn, made.id, &EntryPatch { amount: Some("7.50".into()), kind: Some(Kind::Borrowed), ..Default::default() }).unwrap();
    assert_eq!((changed.amount.as_str(), changed.kind, changed.note.as_deref()), ("7.50", Kind::Borrowed, Some("taxi")));
    let cleared = ledger::update(&w.conn, made.id, &EntryPatch { note: Some(None), transaction_id: Some(None), ..Default::default() }).unwrap();
    assert_eq!((cleared.note, cleared.transaction_id), (None, None));
    let moved = ledger::update(&w.conn, made.id, &EntryPatch { person_id: Some(bob.to_string()), date: Some("2026-02-02".into()), ..Default::default() }).unwrap();
    assert_eq!((moved.person_id, moved.date.as_str()), (bob, "2026-02-02"));
    assert_eq!((balance(&w.conn, anna), balance(&w.conn, bob)), (0, -750));
    for bad in [
        EntryPatch { amount: Some("0.00".into()), ..Default::default() },
        EntryPatch { date: Some("x".into()), ..Default::default() },
        EntryPatch { person_id: Some("99".into()), ..Default::default() },
        EntryPatch { transaction_id: Some(Some("999".into())), ..Default::default() },
    ] {
        assert_eq!(ledger::update(&w.conn, made.id, &bad), Err(AppError::InvalidInput), "{bad:?}");
    }
    assert_eq!(ledger::update(&w.conn, 999, &EntryPatch::default()), Err(AppError::NotFound));
    let patch: EntryPatch = serde_json::from_str(r#"{"note": null}"#).unwrap();
    assert_eq!(patch.note, Some(None));
    assert!(serde_json::from_str::<EntryPatch>(r#"{"stamp": "x"}"#).is_err());
}

#[test]
fn deleting_an_entry_changes_the_balance_and_reports_a_missing_one() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let made = ledger::add(&w.conn, &new(anna, Kind::Lent, "5.00", "2026-01-01")).unwrap();
    ledger::delete(&w.conn, made.id).unwrap();
    assert_eq!(balance(&w.conn, anna), 0);
    assert_eq!(ledger::delete(&w.conn, made.id), Err(AppError::NotFound));
    assert_eq!(ledger::get(&w.conn, made.id), Err(AppError::NotFound));
}

// ---- mark as paid ----------------------------------------------------------------------------

#[test]
fn marking_paid_with_no_amount_settles_the_whole_balance_in_the_right_direction() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    ledger::add(&w.conn, &new(anna, Kind::Lent, "25.00", "2026-01-01")).unwrap();
    ledger::add(&w.conn, &new(bob, Kind::Borrowed, "12.00", "2026-01-01")).unwrap();
    let a = ledger::settle(&w.conn, anna, None, None, None, "2026-02-01").unwrap();
    assert_eq!((a.entry.kind, a.entry.amount.as_str(), a.entry.date.as_str(), a.before.as_str(), a.after.as_str(), a.over.as_str()), (Kind::RepaidToMe, "25.00", "2026-02-01", "25.00", "0.00", "0.00"));
    let b = ledger::settle(&w.conn, bob, None, Some("2026-01-15"), Some("cash"), "2026-02-01").unwrap();
    assert_eq!((b.entry.kind, b.entry.amount.as_str(), b.entry.date.as_str(), b.entry.note.as_deref(), b.before.as_str(), b.after.as_str()), (Kind::RepaidByMe, "12.00", "2026-01-15", Some("cash"), "-12.00", "0.00"));
    assert_eq!((balance(&w.conn, anna), balance(&w.conn, bob)), (0, 0));
}

#[test]
fn a_part_payment_leaves_the_rest() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    ledger::add(&w.conn, &new(anna, Kind::Lent, "25.00", "2026-01-01")).unwrap();
    let paid = ledger::settle(&w.conn, anna, Some("10.00"), None, None, "2026-02-01").unwrap();
    assert_eq!((paid.after.as_str(), paid.over.as_str()), ("15.00", "0.00"));
    assert_eq!(balance(&w.conn, anna), 1500);
}

#[test]
fn an_amount_larger_than_the_debt_is_accepted_and_the_balance_turns_around() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    ledger::add(&w.conn, &new(anna, Kind::Lent, "25.00", "2026-01-01")).unwrap();
    let paid = ledger::settle(&w.conn, anna, Some("40.00"), None, None, "2026-02-01").unwrap();
    assert_eq!((paid.before.as_str(), paid.after.as_str(), paid.over.as_str()), ("25.00", "-15.00", "15.00"), "you now owe Anna the rest");
    // and the next settlement goes the other way
    let back = ledger::settle(&w.conn, anna, None, None, None, "2026-02-02").unwrap();
    assert_eq!((back.entry.kind, back.entry.amount.as_str(), back.after.as_str()), (Kind::RepaidByMe, "15.00", "0.00"));
}

#[test]
fn nothing_to_settle_or_a_bad_amount_is_refused_and_writes_nothing() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    assert_eq!(ledger::settle(&w.conn, anna, None, None, None, "2026-02-01"), Err(AppError::InvalidInput), "a balance of 0 has nothing to settle");
    assert_eq!(ledger::settle(&w.conn, anna, Some("5.00"), None, None, "2026-02-01"), Err(AppError::InvalidInput), "and no direction");
    ledger::add(&w.conn, &new(anna, Kind::Lent, "25.00", "2026-01-01")).unwrap();
    for bad in [Some("0.00"), Some("-1.00"), Some("1"), Some("x")] {
        assert_eq!(ledger::settle(&w.conn, anna, bad, None, None, "2026-02-01"), Err(AppError::InvalidInput), "{bad:?}");
    }
    assert_eq!(ledger::settle(&w.conn, anna, None, Some("nope"), None, "2026-02-01"), Err(AppError::InvalidInput));
    assert_eq!(ledger::settle(&w.conn, 99, None, None, None, "2026-02-01"), Err(AppError::NotFound));
    assert_eq!(one(&w.conn, "SELECT count(*) FROM ledger_entries"), 1);
}

#[test]
fn settling_to_zero_clears_the_reminder_and_a_part_payment_does_not() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    ledger::add(&w.conn, &new(anna, Kind::Lent, "25.00", "2026-01-01")).unwrap();
    people::set_reminder(&w.conn, anna, Some("2026-03-01"), Some(7)).unwrap();
    ledger::settle(&w.conn, anna, Some("10.00"), None, None, "2026-02-01").unwrap();
    assert_eq!(people::get(&w.conn, anna).unwrap().remind_on.as_deref(), Some("2026-03-01"));
    ledger::settle(&w.conn, anna, None, None, None, "2026-02-01").unwrap();
    let after = people::get(&w.conn, anna).unwrap();
    assert_eq!((after.remind_on, after.remind_every_days), (None, None));
}

#[test]
fn a_friend_can_be_settled_after_a_shared_bill_without_any_ledger_entry() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-01");
    set_split(&w.conn, dinner, &equal(vec![who(anna)])).unwrap();
    let paid = ledger::settle(&w.conn, anna, None, None, None, "2026-02-01").unwrap();
    assert_eq!((paid.before.as_str(), paid.after.as_str()), ("45.00", "0.00"));
    assert_eq!(cost(&w.conn, dinner), 4500, "paying you back changes what the dinner cost you not at all");
}

// ---- I borrowed, with an expense -------------------------------------------------------------

#[test]
fn a_borrowing_can_add_your_share_to_your_expenses_in_one_step() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let borrowed = ledger::borrow_with_expense(&w.conn, &new(anna, Kind::Borrowed, "25.00", "2026-01-10"), &Expense { name: "Dinner at Luigi's".into(), category_id: w.food.to_string() }).unwrap();
    assert_eq!(borrowed.kind, Kind::Borrowed);
    let spent = transactions::get(&w.conn, borrowed.transaction_id.as_deref().unwrap().parse().unwrap()).unwrap();
    assert_eq!((spent.name.as_str(), spent.amount.as_str(), spent.payment_date.as_str(), spent.category_id), ("Dinner at Luigi's", "25.00", "2026-01-10", w.food));
    assert_eq!(balance(&w.conn, anna), -2500, "you owe her");
    // paying her back later is an entry, and never a second expense
    ledger::settle(&w.conn, anna, None, None, None, "2026-02-01").unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transactions"), 1);
    assert_eq!(balance(&w.conn, anna), 0);
}

#[test]
fn a_borrowing_with_an_expense_is_all_or_nothing() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let entry = new(anna, Kind::Borrowed, "25.00", "2026-01-10");
    for (bad_entry, expense) in [
        (entry.clone(), Expense { name: "X".into(), category_id: w.salary.to_string() }),
        (entry.clone(), Expense { name: "X".into(), category_id: "999".into() }),
        (entry.clone(), Expense { name: "X".into(), category_id: "abc".into() }),
        (entry.clone(), Expense { name: "".into(), category_id: w.food.to_string() }),
        (new(99, Kind::Borrowed, "25.00", "2026-01-10"), Expense { name: "X".into(), category_id: w.food.to_string() }),
        (new(anna, Kind::Lent, "25.00", "2026-01-10"), Expense { name: "X".into(), category_id: w.food.to_string() }),
        (new(anna, Kind::Borrowed, "0.00", "2026-01-10"), Expense { name: "X".into(), category_id: w.food.to_string() }),
    ] {
        assert!(ledger::borrow_with_expense(&w.conn, &bad_entry, &expense).is_err(), "{bad_entry:?} {expense:?}");
        assert_eq!((one(&w.conn, "SELECT count(*) FROM transactions"), one(&w.conn, "SELECT count(*) FROM ledger_entries")), (0, 0), "nothing was left behind");
    }
}

// ---- the statement ---------------------------------------------------------------------------

#[test]
fn the_statement_tells_the_story_by_date_with_the_balance_after_each_line() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-05");
    set_split(&w.conn, dinner, &equal(vec![who(anna)])).unwrap();
    ledger::add(&w.conn, &NewEntry { note: Some("taxi".into()), ..new(anna, Kind::Lent, "5.00", "2026-01-05") }).unwrap();
    ledger::add(&w.conn, &new(anna, Kind::RepaidToMe, "20.00", "2026-01-20")).unwrap();
    ledger::add(&w.conn, &new(anna, Kind::Borrowed, "40.00", "2026-01-01")).unwrap();
    let statement = ledger::statement(&w.conn, anna).unwrap();
    let rows: Vec<(String, String, String, String, String)> = statement.lines.iter().map(|l| (l.date.clone(), l.kind.clone(), l.what.clone(), l.effect.clone(), l.balance.clone())).collect();
    assert_eq!(
        rows,
        [
            ("2026-01-01".into(), "BORROWED".into(), "".into(), "-40.00".into(), "-40.00".into()),
            ("2026-01-05".into(), "BILL".into(), "Dinner".into(), "45.00".into(), "5.00".into()),
            ("2026-01-05".into(), "LENT".into(), "taxi".into(), "5.00".into(), "10.00".into()),
            ("2026-01-20".into(), "REPAID_TO_ME".into(), "".into(), "-20.00".into(), "-10.00".into()),
        ],
        "a bill comes before the entries of the same day"
    );
    assert_eq!(statement.person.balance, "-10.00");
    assert_eq!(statement.lines.last().unwrap().balance, statement.person.balance, "the last line is where the balance is");
    assert_eq!(statement.lines[1].amount, "45.00");
    assert_eq!((statement.lines[1].entry_id.clone(), statement.lines[1].transaction_id.clone()), (None, Some(dinner.to_string())));
    assert!(statement.lines[0].entry_id.is_some());
    assert_eq!(ledger::statement(&w.conn, 99), Err(AppError::NotFound));
}

#[test]
fn money_lent_or_paid_back_is_never_income_or_expense_and_touches_no_budget() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    bill(&w.conn, w.food, "Bread", "2.00", "2026-01-05");
    let before = (
        crate::budgets::overview(&w.conn, "2026-01").unwrap(),
        crate::stats::year_trend(&w.conn, "2026").unwrap(),
        crate::export::preview(&w.conn, &Default::default()).unwrap().income,
        crate::export::preview(&w.conn, &Default::default()).unwrap().expense,
    );
    for kind in [Kind::Lent, Kind::Borrowed, Kind::RepaidToMe, Kind::RepaidByMe] {
        ledger::add(&w.conn, &new(anna, kind, "500.00", "2026-01-05")).unwrap();
    }
    ledger::borrow_with_expense(&w.conn, &new(anna, Kind::Borrowed, "7.00", "2026-01-06"), &Expense { name: "Lunch".into(), category_id: w.food.to_string() }).unwrap();
    let after = (
        crate::budgets::overview(&w.conn, "2026-01").unwrap(),
        crate::stats::year_trend(&w.conn, "2026").unwrap(),
        crate::export::preview(&w.conn, &Default::default()).unwrap().income,
        crate::export::preview(&w.conn, &Default::default()).unwrap().expense,
    );
    assert_eq!(after.2, before.2, "no income");
    assert_eq!(after.3, "9.00", "only the 2.00 and the 7.00 that was made an expense on purpose");
    assert_eq!(after.0.iter().find(|o| o.kind == crate::categories::CategoryType::Expense).unwrap().actual, "9.00");
    assert_eq!(before.1.len(), after.1.len());
}
