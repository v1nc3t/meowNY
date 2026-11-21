//! A term deposit's state machine (`docs/loans-deposits-design.md`, section 5.3 and 5.5), the edge cases of item 8 of section 9: what each state lets
//! happen to its money, what the return books, how a term is rolled over, and what the terms of a deposit may be. Every refusal is checked to have changed
//! nothing: no row, no stamp, no marker, no history line. What a goal stash does is proved by the other tests of this module and is not repeated.
//!
//! Today is 2026-11-13. A running deposit starts on 2026-10-01 and matures on 2027-04-01; a matured one is a running deposit whose maturity was moved to
//! 2026-10-01 after money went in (nothing else can put money in a deposit that has matured).

use super::deposits::{book_return, ReturnInput};
use super::entries::{self, EntryPatch, NewTake};
use super::fixtures::*;
use super::{create, DepositState, NewStash, StashKind, StashPatch};
use crate::accounts;
use crate::categories::{self, CategoryType};
use crate::debts::fixtures::World;
use crate::rates::{self, Period};
use crate::transactions;
use rusqlite::Connection;

const RUNNING: DepositState = DepositState::Running;
const MATURED: DepositState = DepositState::Matured;
const BOOKED: DepositState = DepositState::Booked;
const BROKEN: DepositState = DepositState::Broken;

/// The terms of a deposit at 3.65 % a year.
fn terms(name: &str, account: i64, start: &str, maturity: &str) -> NewStash {
    NewStash {
        kind: Some(StashKind::Deposit),
        interest_rate: Some("3.65".into()),
        interest_period: Some(Period::Yearly),
        start_date: Some(start.into()),
        target_date: Some(maturity.into()),
        ..new_stash(name, account)
    }
}

/// A deposit that is running: 2026-10-01 to 2027-04-01.
fn running(conn: &Connection, name: &str, account: i64) -> i64 {
    create(conn, &terms(name, account, "2026-10-01", "2027-04-01"), TODAY).unwrap().id
}

fn maturity(date: &str) -> StashPatch {
    StashPatch { target_date: Some(Some(date.into())), ..Default::default() }
}

fn state(conn: &Connection, id: i64) -> DepositState {
    shown(conn, id).deposit.unwrap().state
}

/// A deposit that has matured on 2026-10-01 holding `held`, put in on 2026-02-01 while it ran (2026-01-01 to 2027-01-01). Returns it and the entry.
fn matured(conn: &Connection, name: &str, account: i64, held: &str) -> (i64, i64) {
    let id = create(conn, &terms(name, account, "2026-01-01", "2027-01-01"), TODAY).unwrap().id;
    let entry = put_on(conn, id, held, "2026-02-01");
    super::update(conn, id, &maturity("2026-10-01"), TODAY).unwrap();
    assert_eq!(state(conn, id), MATURED);
    (id, entry)
}

fn ret(amount: &str, category: i64) -> ReturnInput {
    ReturnInput { amount: amount.into(), category_id: category.to_string(), date: None, note: None }
}

fn on(date: &str, input: ReturnInput) -> ReturnInput {
    ReturnInput { date: Some(date.into()), ..input }
}

/// A matured deposit whose return of 31.00 is booked on 2026-10-05. Returns it and the entry of the return.
fn booked(w: &World, name: &str, account: i64) -> (i64, i64) {
    let (id, _) = matured(&w.conn, name, account, "1000.00");
    book_return(&w.conn, id, &on("2026-10-05", ret("31.00", w.salary)), TODAY).unwrap();
    assert_eq!(state(&w.conn, id), BOOKED);
    (id, one(&w.conn, "SELECT MAX(id) FROM stash_entries"))
}

/// A running deposit that holds 1000.00 put in on 2026-10-15 and was emptied on 2026-11-10. Returns it, the add and the take.
fn broken(conn: &Connection, name: &str, account: i64) -> (i64, i64, i64) {
    let id = running(conn, name, account);
    let add = put_on(conn, id, "1000.00", "2026-10-15");
    entries::take(conn, id, &NewTake { date: Some("2026-11-10".into()), ..new_take("1000.00") }, TODAY).unwrap();
    assert_eq!(state(conn, id), BROKEN);
    (id, add, one(conn, "SELECT MAX(id) FROM stash_entries"))
}

fn amount(text: &str) -> EntryPatch {
    EntryPatch { amount: Some(text.into()), ..Default::default() }
}

fn income_of(conn: &Connection) -> transactions::Transaction {
    transactions::get(conn, one(conn, "SELECT MAX(id) FROM transactions")).unwrap()
}

// ---- the numbers and the state, from the row and today ------------------------------------------------

#[test]
fn a_deposit_shows_its_return_over_the_whole_term_on_what_it_holds_for_both_periods() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "20000.00");
    // 10000.00 at 3.65 % a year for 365 days is 365.00
    let yearly = create(&w.conn, &terms("Yearly", acc, "2026-10-01", "2027-10-01"), TODAY).unwrap().id;
    assert_eq!(shown(&w.conn, yearly).deposit.unwrap().expected_return, "0.00", "it holds nothing yet: held, not target");
    put(&w.conn, yearly, "10000.00");
    let stash = shown(&w.conn, yearly);
    let deposit = stash.deposit.clone().unwrap();
    assert_eq!(
        (deposit.interest_rate.as_str(), deposit.interest_period, deposit.start_date.as_str(), stash.target_date.as_deref(), deposit.expected_return.as_str(), deposit.value_at_maturity.as_str()),
        ("3.65", Period::Yearly, "2026-10-01", Some("2027-10-01"), "365.00", "10365.00")
    );
    assert_eq!((deposit.state, deposit.days_left, deposit.broken_on, deposit.returned_on), (RUNNING, 322, None, None));
    assert_eq!((stash.kind, stash.per_month), (StashKind::Deposit, None), "a deposit has no amount to save each month");
    // 1 % a month for 365 days is 12 % a year: 1200.00
    let monthly = create(&w.conn, &NewStash { interest_rate: Some("1".into()), interest_period: Some(Period::Monthly), ..terms("Monthly", acc, "2026-10-01", "2027-10-01") }, TODAY).unwrap().id;
    put(&w.conn, monthly, "10000.00");
    let deposit = shown(&w.conn, monthly).deposit.unwrap();
    assert_eq!((deposit.interest_rate.as_str(), deposit.interest_period, deposit.expected_return.as_str(), deposit.value_at_maturity.as_str()), ("1", Period::Monthly, "1200.00", "11200.00"));
    assert_eq!(rates::deposit_return(1_000_000, 3650, Period::Yearly, "2026-10-01", "2027-10-01"), 36_500);
    // the same target and date make a goal save each month and a deposit not
    let aimed = |input: NewStash| shown(&w.conn, create(&w.conn, &NewStash { target: Some("5000.00".into()), ..input }, TODAY).unwrap().id);
    let goal = aimed(NewStash { target_date: Some("2027-04-01".into()), ..new_stash("Goal", acc) });
    assert_eq!((goal.kind, goal.deposit, goal.per_month.is_some()), (StashKind::Goal, None, true));
    let with_target = aimed(terms("With target", acc, "2026-10-01", "2027-04-01"));
    assert_eq!((with_target.kind, with_target.per_month), (StashKind::Deposit, None));
}

#[test]
fn the_state_follows_the_maturity_day_and_the_days_left_stop_at_zero() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "100.00");
    let tomorrow = create(&w.conn, &terms("Tomorrow", acc, "2026-10-01", "2026-11-14"), TODAY).unwrap().id;
    let day = create(&w.conn, &terms("Day", acc, "2026-10-01", TODAY), TODAY).unwrap().id;
    let past = create(&w.conn, &terms("Past", acc, "2026-01-01", "2026-02-01"), TODAY).unwrap().id;
    let seen: Vec<(DepositState, i64)> = [tomorrow, day, past].iter().map(|id| shown(&w.conn, *id).deposit.unwrap()).map(|d| (d.state, d.days_left)).collect();
    assert_eq!(seen, [(RUNNING, 1), (MATURED, 0), (MATURED, 0)], "on its day a deposit has matured; a term in the past has matured when it is made");
    // a start left out is today, and the maturity is after it
    let tomorrow_start = create(&w.conn, &NewStash { start_date: None, ..terms("Fresh", acc, "", "2026-11-14") }, TODAY).unwrap().id;
    assert_eq!(shown(&w.conn, tomorrow_start).deposit.unwrap().start_date, TODAY);
}

// ---- add ---------------------------------------------------------------------------------------------

#[test]
fn money_goes_into_a_deposit_only_while_it_is_running() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "9000.00");
    let id = running(&w.conn, "Term", acc);
    let detail = entries::add(&w.conn, id, &new_entry("100.00"), TODAY).unwrap();
    assert_eq!((detail.stash.saved.as_str(), detail.stash.deposit.unwrap().state), ("100.00", RUNNING));

    let (matured_id, _) = matured(&w.conn, "Matured", acc, "100.00");
    let (booked_id, _) = booked(&w, "Booked", acc);
    let (broken_id, _, _) = broken(&w.conn, "Broken", acc);
    for (id, what) in [(matured_id, "matured"), (booked_id, "booked"), (broken_id, "broken")] {
        assert_refused(&w.conn, INVALID, what, || entries::add(&w.conn, id, &new_entry("1.00"), TODAY));
        assert_refused(&w.conn, INVALID, what, || entries::add(&w.conn, id, &entries::NewEntry { from_account_id: Some("1".into()), ..new_entry("1.00") }, TODAY));
    }
}

// ---- take --------------------------------------------------------------------------------------------

#[test]
fn a_take_of_less_than_everything_is_refused_while_it_runs_and_a_take_of_everything_breaks_the_deposit() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let id = running(&w.conn, "Term", acc);
    put_on(&w.conn, id, "1000.00", "2026-10-15");
    assert_refused(&w.conn, INVALID, "a cent less than everything", || entries::take(&w.conn, id, &new_take("999.99"), TODAY));
    assert_refused(&w.conn, INVALID, "a cent less, to an account", || entries::take(&w.conn, id, &NewTake { to_account_id: Some("1".into()), ..new_take("999.99") }, TODAY));
    assert_refused(&w.conn, INVALID, "a cent more than everything", || entries::take(&w.conn, id, &new_take("1000.01"), TODAY));
    assert_eq!(state(&w.conn, id), RUNNING);

    let stamp = stamp_of(&w.conn, "stashes", id);
    let detail = entries::take(&w.conn, id, &NewTake { date: Some("2026-11-10".into()), note: Some("early".into()), ..new_take("1000.00") }, TODAY).unwrap();
    let deposit = detail.stash.deposit.unwrap();
    assert_eq!((deposit.state, deposit.broken_on.as_deref(), detail.stash.saved.as_str(), deposit.returned_on), (BROKEN, Some("2026-11-10"), "0.00", None), "broken on the day of the entry, not today");
    assert_ne!(stamp_of(&w.conn, "stashes", id), stamp, "broken_on is a change of the row");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 0);

    // with a transfer to another account: the transfer is made and the deposit is broken, in one step
    let current = funded(&w.conn, "Current", "10.00");
    let second = running(&w.conn, "Second", acc);
    put(&w.conn, second, "500.00");
    assert_refused(&w.conn, INVALID, "a part of it, to an account", || entries::take(&w.conn, second, &NewTake { to_account_id: Some(current.to_string()), ..new_take("100.00") }, TODAY));
    let detail = entries::take(&w.conn, second, &NewTake { to_account_id: Some(current.to_string()), ..new_take("500.00") }, TODAY).unwrap();
    assert_eq!((detail.stash.deposit.unwrap().broken_on.as_deref(), detail.entries[0].other_account_id), (Some(TODAY), Some(current)));
    assert_eq!((one(&w.conn, "SELECT count(*) FROM transfers"), balance(&w.conn, current, TODAY), balance(&w.conn, acc, TODAY)), (1, 51_000, 450_000));
}

// ---- edit and delete an entry --------------------------------------------------------------------------

#[test]
fn a_broken_deposit_changes_only_the_notes_of_its_entries() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let (id, add, take) = broken(&w.conn, "Term", acc);
    assert_refused(&w.conn, INVALID, "an amount", || entries::update(&w.conn, add, &amount("900.00"), TODAY));
    assert_refused(&w.conn, INVALID, "an amount of a take", || entries::update(&w.conn, take, &amount("900.00"), TODAY));
    assert_refused(&w.conn, INVALID, "a date", || entries::update(&w.conn, add, &EntryPatch { date: Some("2026-10-14".into()), ..Default::default() }, TODAY));
    assert_refused(&w.conn, INVALID, "a date, with the amount as it is", || entries::update(&w.conn, add, &EntryPatch { amount: Some("1000.00".into()), date: Some("2026-10-16".into()), ..Default::default() }, TODAY));
    assert_refused(&w.conn, INVALID, "delete the add", || entries::delete(&w.conn, add, false, TODAY));
    assert_refused(&w.conn, INVALID, "delete the take", || entries::delete(&w.conn, take, false, TODAY));
    assert_refused(&w.conn, INVALID, "delete the take and its transfer", || entries::delete(&w.conn, take, true, TODAY));

    let kept = entries::update(&w.conn, add, &EntryPatch { note: Some(Some("the bank".into())), ..Default::default() }, TODAY).unwrap();
    assert_eq!(kept.entries.iter().find(|e| e.id == add).unwrap().note.as_deref(), Some("the bank"));
    let same = entries::update(&w.conn, add, &EntryPatch { amount: Some("1000.00".into()), date: Some("2026-10-15".into()), note: Some(None) }, TODAY).unwrap();
    assert_eq!((same.entries.iter().find(|e| e.id == add).unwrap().note.clone(), same.stash.deposit.unwrap().state), (None, BROKEN), "the size and the day as they are, with the note cleared");
    assert_eq!(state(&w.conn, id), BROKEN);
}

#[test]
fn while_it_runs_no_edit_or_delete_takes_money_out_of_a_deposit_but_one_that_puts_money_in_is_fine() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let id = running(&w.conn, "Term", acc);
    let add = put_on(&w.conn, id, "1000.00", "2026-10-15");
    assert_refused(&w.conn, INVALID, "an add edited down", || entries::update(&w.conn, add, &amount("999.99"), TODAY));
    assert_refused(&w.conn, INVALID, "the only add deleted: that would be a take of everything with no take", || entries::delete(&w.conn, add, false, TODAY));
    let up = entries::update(&w.conn, add, &amount("1500.00"), TODAY).unwrap();
    assert_eq!(up.stash.saved, "1500.00");
    let moved = entries::update(&w.conn, add, &EntryPatch { date: Some("2026-10-20".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!((moved.stash.saved.as_str(), moved.entries[0].date.as_str()), ("1500.00", "2026-10-20"));
    assert_eq!(state(&w.conn, id), RUNNING);
}

#[test]
fn once_a_deposit_has_matured_no_edit_or_delete_puts_money_in_but_money_still_comes_out() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let (id, add) = matured(&w.conn, "Term", acc, "1000.00");
    assert_refused(&w.conn, INVALID, "an add edited up", || entries::update(&w.conn, add, &amount("1000.01"), TODAY));
    let out = entries::take(&w.conn, id, &new_take("100.00"), TODAY).unwrap();
    let take = out.entries[0].id;
    assert_eq!((out.stash.saved.as_str(), out.stash.deposit.unwrap().state), ("900.00", MATURED), "taking some of it leaves the state as it is");
    assert_refused(&w.conn, INVALID, "a take edited down: money goes back in", || entries::update(&w.conn, take, &amount("50.00"), TODAY));
    assert_refused(&w.conn, INVALID, "a take deleted: money goes back in", || entries::delete(&w.conn, take, false, TODAY));
    assert_eq!(entries::update(&w.conn, take, &amount("200.00"), TODAY).unwrap().stash.saved, "800.00", "a take edited up: more comes out");
    assert_eq!(entries::update(&w.conn, add, &amount("900.00"), TODAY).unwrap().stash.saved, "700.00", "an add edited down");

    // booked: the same
    let (booked_id, returned) = booked(&w, "Booked", acc);
    assert_refused(&w.conn, INVALID, "the return edited up", || entries::update(&w.conn, returned, &amount("31.01"), TODAY));
    assert_eq!(entries::update(&w.conn, returned, &amount("30.00"), TODAY).unwrap().stash.deposit.unwrap().state, BOOKED);
    assert_eq!(state(&w.conn, booked_id), BOOKED);
}

// ---- book the return ------------------------------------------------------------------------------------

#[test]
fn the_return_of_a_matured_deposit_is_an_income_an_add_and_the_day_it_was_booked() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let (id, _) = matured(&w.conn, "Term deposit", acc, "1000.00");
    let stamp = stamp_of(&w.conn, "stashes", id);
    let got = book_return(&w.conn, id, &ReturnInput { note: Some(" interest ".into()), ..on("2026-10-05", ret("31.00", w.salary)) }, TODAY).unwrap();
    let deposit = got.deposit.clone().unwrap();
    assert_eq!((deposit.state, deposit.returned_on.as_deref(), deposit.broken_on, got.saved.as_str(), got.entries), (BOOKED, Some("2026-10-05"), None, "1031.00", 2));
    assert_ne!(stamp_of(&w.conn, "stashes", id), stamp);
    // the income: named as the stash, in its account, on the day, with the note
    let income = income_of(&w.conn);
    assert_eq!(
        (income.name.as_str(), income.amount.as_str(), income.native_amount.as_deref(), income.account_id, income.category_id, income.payment_date.as_str(), income.description.as_deref()),
        ("Term deposit", "31.00", None, acc, w.salary, "2026-10-05", Some("interest"))
    );
    // the add: the same amount, day and note, with no transfer
    let entry = entries::list(&w.conn, id).unwrap().into_iter().find(|e| e.amount == "31.00").unwrap();
    assert_eq!((entry.kind, entry.date.as_str(), entry.note.as_deref(), entry.transfer_id), (entries::Kind::Add, "2026-10-05", Some("interest"), None));
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 0);
    // the delta rule saw the income: the account has the 31.00 and the deposit holds it
    assert_eq!((accounts::get(&w.conn, acc, TODAY).unwrap().balance.as_str(), got.free.as_str()), ("5031.00", "4000.00"));
    // once it is booked it is not booked again
    assert_refused(&w.conn, INVALID, "booked again", || book_return(&w.conn, id, &ret("1.00", w.salary), TODAY));

    // a date left out is today
    let (other, _) = matured(&w.conn, "Other", acc, "100.00");
    let got = book_return(&w.conn, other, &ret("1.00", w.salary), TODAY).unwrap();
    assert_eq!((got.deposit.unwrap().returned_on.as_deref(), income_of(&w.conn).payment_date.as_str()), (Some(TODAY), TODAY));
}

#[test]
fn the_return_of_a_deposit_in_a_foreign_account_is_an_income_in_its_currency_and_the_same_amount_added() {
    let w = world();
    let lei = foreign(&w.conn, "Lei", "RON", "9000.00");
    let (id, _) = matured(&w.conn, "Term", lei, "1000.00");
    let got = book_return(&w.conn, id, &on("2026-10-05", ret("50.00", w.salary)), TODAY).unwrap();
    let income = income_of(&w.conn);
    assert_eq!((income.amount.as_str(), income.native_amount.as_deref(), income.account_id), ("10.00", Some("50.00"), lei), "worth 10.00 at a rate of 5.00, and what moved the account is 50.00");
    assert_eq!((got.saved.as_str(), got.currency.as_str(), got.deposit.unwrap().state), ("1050.00", "RON", BOOKED), "the deposit holds the 50.00 in the account's currency");
}

#[test]
fn the_return_is_refused_unless_the_deposit_has_matured_and_everything_else_is_right_and_nothing_is_written() {
    let mut w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let (id, _) = matured(&w.conn, "Term", acc, "1000.00");
    let running_id = running(&w.conn, "Running", acc);
    let (booked_id, _) = booked(&w, "Booked", acc);
    let (broken_id, _, _) = broken(&w.conn, "Broken", acc);
    let goal = stash(&w.conn, "Goal", acc);
    for (stash, what) in [(running_id, "running"), (booked_id, "booked"), (broken_id, "broken"), (goal, "a goal")] {
        assert_refused(&w.conn, INVALID, what, || book_return(&w.conn, stash, &ret("1.00", w.salary), TODAY));
    }
    assert_refused(&w.conn, NOT_FOUND, "no such stash", || book_return(&w.conn, 99, &ret("1.00", w.salary), TODAY));
    let gone = categories::create(&w.conn, "Gone", CategoryType::Income).unwrap().id;
    categories::delete(&mut w.conn, gone, chrono::Utc::now()).unwrap();
    let bad = [
        ("an expense category", ret("1.00", w.food)),
        ("a category that is gone", ret("1.00", gone)),
        ("a category that is not there", ret("1.00", 999)),
        ("a category that is not an id", ReturnInput { category_id: "x".into(), ..ret("1.00", w.salary) }),
        ("zero", ret("0.00", w.salary)),
        ("negative", ret("-1.00", w.salary)),
        ("one decimal", ret("1.5", w.salary)),
        ("beyond the largest", ret("10000000000.00", w.salary)),
        ("a day before the maturity", on("2026-09-30", ret("1.00", w.salary))),
        ("a day after today", on("2026-11-14", ret("1.00", w.salary))),
        ("a date that is not real", on("2026-02-30", ret("1.00", w.salary))),
        ("a long note", ReturnInput { note: Some("x".repeat(201)), ..ret("1.00", w.salary) }),
    ];
    for (what, input) in bad {
        assert_refused(&w.conn, INVALID, what, || book_return(&w.conn, id, &input, TODAY));
    }
    assert_refused(&w.conn, INVALID, "a today that is not a date", || book_return(&w.conn, id, &ret("1.00", w.salary), "today"));
    // the maturity day itself is allowed
    assert!(book_return(&w.conn, id, &on("2026-10-01", ret("1.00", w.salary)), TODAY).is_ok());

    // an archived stash, and one in an archived account
    let (archived, _) = matured(&w.conn, "Archived", acc, "100.00");
    super::archive(&w.conn, archived, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "an archived stash", || book_return(&w.conn, archived, &ret("1.00", w.salary), TODAY));
    let old = funded(&w.conn, "Old", "100.00");
    let (in_old, _) = matured(&w.conn, "In old", old, "100.00");
    accounts::archive(&w.conn, old, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "an archived account", || book_return(&w.conn, in_old, &ret("1.00", w.salary), TODAY));
}

#[test]
fn a_return_that_the_delta_rule_refuses_leaves_no_income_behind() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let (id, _) = matured(&w.conn, "Term", acc, "1000.00");
    super::update(&w.conn, id, &StashPatch { target: Some(Some("1000.00".into())), ..Default::default() }, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "the add would go over the target, after the income was made", || book_return(&w.conn, id, &ret("1.00", w.salary), TODAY));
    assert_eq!((one(&w.conn, "SELECT count(*) FROM transactions"), state(&w.conn, id)), (0, MATURED));
}

// ---- terms: create and update ---------------------------------------------------------------------------

#[test]
fn a_maturity_that_is_not_after_the_start_is_refused_on_create_and_on_update() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "100.00");
    let bad = [
        ("maturity = start", terms("A", acc, "2026-10-01", "2026-10-01")),
        ("maturity before start", terms("A", acc, "2026-10-01", "2026-09-30")),
        ("no maturity", NewStash { target_date: None, ..terms("A", acc, "2026-10-01", "") }),
        ("no start, maturity today", NewStash { start_date: None, ..terms("A", acc, "", TODAY) }),
        ("no start, maturity before today", NewStash { start_date: None, ..terms("A", acc, "", "2026-11-12") }),
        ("no rate", NewStash { interest_rate: None, ..terms("A", acc, "2026-10-01", "2027-04-01") }),
        ("no period", NewStash { interest_period: None, ..terms("A", acc, "2026-10-01", "2027-04-01") }),
        ("a rate over 100", NewStash { interest_rate: Some("100.001".into()), ..terms("A", acc, "2026-10-01", "2027-04-01") }),
        ("a rate with four decimals", NewStash { interest_rate: Some("3.1415".into()), ..terms("A", acc, "2026-10-01", "2027-04-01") }),
        ("a start that is not a date", terms("A", acc, "2026-02-30", "2027-04-01")),
    ];
    for (what, input) in bad {
        assert_refused(&w.conn, INVALID, what, || create(&w.conn, &input, TODAY));
    }
    assert!(create(&w.conn, &NewStash { interest_rate: Some("0".into()), ..terms("Zero", acc, "2026-10-01", "2026-10-02") }, TODAY).is_ok(), "a rate of 0 and a term of one day are allowed");

    let id = running(&w.conn, "Term", acc);
    assert_refused(&w.conn, INVALID, "a maturity that is the start", || super::update(&w.conn, id, &maturity("2026-10-01"), TODAY));
    assert_refused(&w.conn, INVALID, "a maturity before the start", || super::update(&w.conn, id, &maturity("2026-01-01"), TODAY));
    for (what, start) in [("a start that is the maturity", "2027-04-01"), ("a start after the maturity", "2027-05-01")] {
        assert_refused(&w.conn, INVALID, what, || super::update(&w.conn, id, &StashPatch { start_date: Some(start.into()), ..Default::default() }, TODAY));
    }
    assert_refused(&w.conn, INVALID, "both at once", || super::update(&w.conn, id, &StashPatch { start_date: Some("2027-06-01".into()), target_date: Some(Some("2027-06-01".into())), ..Default::default() }, TODAY));
    assert_refused(&w.conn, INVALID, "a maturity that is not a date", || super::update(&w.conn, id, &maturity("2027-02-30"), TODAY));
    let both = super::update(&w.conn, id, &StashPatch { start_date: Some("2027-05-01".into()), target_date: Some(Some("2027-06-01".into())), ..Default::default() }, TODAY).unwrap();
    assert_eq!((both.deposit.unwrap().start_date.as_str(), both.target_date.as_deref()), ("2027-05-01", Some("2027-06-01")), "a start and a maturity that are fine together");
}

#[test]
fn a_deposit_always_has_a_maturity_and_its_rate_and_period_change_together_or_alone() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let id = running(&w.conn, "Term", acc);
    put(&w.conn, id, "1000.00");
    assert_refused(&w.conn, INVALID, "a cleared maturity", || super::update(&w.conn, id, &StashPatch { target_date: Some(None), ..Default::default() }, TODAY));
    assert_refused(&w.conn, INVALID, "a cleared maturity with a rename", || super::update(&w.conn, id, &StashPatch { name: Some("New".into()), target_date: Some(None), ..Default::default() }, TODAY));
    for bad in ["100.001", "-1", "", "abc", "3.", ".5"] {
        assert_refused(&w.conn, INVALID, bad, || super::update(&w.conn, id, &StashPatch { interest_rate: Some(bad.into()), ..Default::default() }, TODAY));
    }
    let rate = super::update(&w.conn, id, &StashPatch { interest_rate: Some("4.125".into()), ..Default::default() }, TODAY).unwrap().deposit.unwrap();
    assert_eq!((rate.interest_rate.as_str(), rate.interest_period), ("4.125", Period::Yearly));
    let period = super::update(&w.conn, id, &StashPatch { interest_period: Some(Period::Monthly), ..Default::default() }, TODAY).unwrap().deposit.unwrap();
    assert_eq!((period.interest_rate.as_str(), period.interest_period), ("4.125", Period::Monthly), "the rate as it was, the period new");
    // a rename keeps the terms
    let renamed = super::update(&w.conn, id, &StashPatch { name: Some("Renamed".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!((renamed.deposit.unwrap().interest_rate.as_str(), renamed.target_date.as_deref()), ("4.125", Some("2027-04-01")));
}

#[test]
fn a_broken_deposit_takes_no_change_of_its_terms_but_its_name_notes_and_target_change() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let (id, _, _) = broken(&w.conn, "Term", acc);
    let sends = [
        ("a rate", StashPatch { interest_rate: Some("5".into()), ..Default::default() }),
        ("a period", StashPatch { interest_period: Some(Period::Monthly), ..Default::default() }),
        ("a start", StashPatch { start_date: Some("2026-09-01".into()), ..Default::default() }),
        ("a maturity", maturity("2027-05-01")),
        ("a cleared maturity", StashPatch { target_date: Some(None), ..Default::default() }),
        ("a rate with a name", StashPatch { name: Some("New".into()), interest_rate: Some("5".into()), ..Default::default() }),
    ];
    for (what, patch) in sends {
        assert_refused(&w.conn, INVALID, what, || super::update(&w.conn, id, &patch, TODAY));
    }
    let changed = super::update(&w.conn, id, &StashPatch { name: Some("Renamed".into()), notes: Some(Some("it broke".into())), target: Some(Some("1000.00".into())), ..Default::default() }, TODAY).unwrap();
    assert_eq!((changed.name.as_str(), changed.notes.as_deref(), changed.target.as_deref(), changed.deposit.unwrap().state), ("Renamed", Some("it broke"), Some("1000.00"), BROKEN));
}

#[test]
fn a_new_maturity_after_today_rolls_a_matured_or_booked_deposit_over_to_start_where_the_last_term_ended() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "9000.00");
    // booked: the next term starts on the old maturity, and its return is not booked
    let (id, _) = booked(&w, "Booked", acc);
    let before = shown(&w.conn, id).deposit.unwrap();
    assert_eq!((before.start_date.as_str(), before.returned_on.as_deref()), ("2026-01-01", Some("2026-10-05")));
    let rolled = super::update(&w.conn, id, &maturity("2027-10-01"), TODAY).unwrap();
    let deposit = rolled.deposit.unwrap();
    assert_eq!((deposit.state, deposit.start_date.as_str(), deposit.returned_on, rolled.target_date.as_deref()), (RUNNING, "2026-10-01", None, Some("2027-10-01")));
    assert_eq!(rolled.saved, "1031.00", "what it holds goes into the next term");
    assert_eq!(deposit.days_left, 322);
    // and it takes money again
    assert!(entries::add(&w.conn, id, &new_entry("10.00"), TODAY).is_ok());

    // matured and not booked: the same
    let (matured_id, _) = matured(&w.conn, "Matured", acc, "100.00");
    let deposit = super::update(&w.conn, matured_id, &maturity("2028-01-01"), TODAY).unwrap().deposit.unwrap();
    assert_eq!((deposit.state, deposit.start_date.as_str()), (RUNNING, "2026-10-01"));

    // a start in the patch: rolled from that start; not rolled: a maturity that is not after today, a deposit that runs
    let (a, _) = booked(&w, "A", acc);
    let with_start = super::update(&w.conn, a, &StashPatch { start_date: Some("2026-11-01".into()), target_date: Some(Some("2027-11-01".into())), ..Default::default() }, TODAY).unwrap().deposit.unwrap();
    assert_eq!((with_start.start_date.as_str(), with_start.returned_on.as_deref(), with_start.state), ("2026-11-01", None, RUNNING), "the start that was given is the start; the new term's return is not booked");
    let (b, _) = booked(&w, "B", acc);
    let past = super::update(&w.conn, b, &maturity("2026-11-13"), TODAY).unwrap().deposit.unwrap();
    assert_eq!((past.state, past.start_date.as_str(), past.returned_on.as_deref()), (BOOKED, "2026-01-01", Some("2026-10-05")), "a maturity that is today is not after today");
    let ahead = running(&w.conn, "Ahead", acc);
    let moved = super::update(&w.conn, ahead, &maturity("2027-09-01"), TODAY).unwrap().deposit.unwrap();
    assert_eq!((moved.state, moved.start_date.as_str()), (RUNNING, "2026-10-01"), "a deposit that runs keeps its start");
}

// ---- the kind ---------------------------------------------------------------------------------------

#[test]
fn the_kind_is_fixed_and_a_goal_takes_no_deposit_field() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "100.00");
    let goal = stash(&w.conn, "Goal", acc);
    let deposit = running(&w.conn, "Term", acc);
    for (id, what) in [(goal, "a goal"), (deposit, "a deposit")] {
        for kind in [StashKind::Goal, StashKind::Deposit] {
            assert_refused(&w.conn, INVALID, what, || super::update(&w.conn, id, &StashPatch { kind: Some(kind), ..Default::default() }, TODAY));
            assert_refused(&w.conn, INVALID, what, || super::update(&w.conn, id, &StashPatch { kind: Some(kind), name: Some("New".into()), ..Default::default() }, TODAY));
        }
    }
    // a goal made with a deposit field, or updated with one
    let fields = [
        ("a rate", NewStash { interest_rate: Some("3".into()), ..new_stash("G", acc) }),
        ("a period", NewStash { interest_period: Some(Period::Yearly), ..new_stash("G", acc) }),
        ("a start", NewStash { start_date: Some("2026-10-01".into()), ..new_stash("G", acc) }),
        ("a rate, as a goal said so", NewStash { kind: Some(StashKind::Goal), interest_rate: Some("3".into()), ..new_stash("G", acc) }),
        ("every field", NewStash { interest_rate: Some("3".into()), interest_period: Some(Period::Yearly), start_date: Some("2026-10-01".into()), ..new_stash("G", acc) }),
    ];
    for (what, input) in fields {
        assert_refused(&w.conn, INVALID, what, || create(&w.conn, &input, TODAY));
    }
    for (what, patch) in [
        ("a rate", StashPatch { interest_rate: Some("3".into()), ..Default::default() }),
        ("a period", StashPatch { interest_period: Some(Period::Monthly), ..Default::default() }),
        ("a start", StashPatch { start_date: Some("2026-10-01".into()), ..Default::default() }),
    ] {
        assert_refused(&w.conn, INVALID, what, || super::update(&w.conn, goal, &patch, TODAY));
    }
    assert_eq!(shown(&w.conn, goal).kind, StashKind::Goal);
    // a goal made as a goal, with a target date in the past and no rate, is as it always was
    assert!(create(&w.conn, &NewStash { kind: Some(StashKind::Goal), target_date: Some("2026-01-01".into()), ..new_stash("Old goal", acc) }, TODAY).is_ok());
}
