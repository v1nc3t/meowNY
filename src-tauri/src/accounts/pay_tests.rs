//! Paying a liability (`docs/loans-deposits-design.md`, section 5.4): one payment is a transfer of the principal and an expense of the interest, in
//! one database transaction. The edge cases: all of it interest, none of it, and every refusal, each of which must leave nothing written.

use super::fixtures::*;
use super::pay::{pay, Paid, Payment};
use super::{create, get, Account, Kind, LiabilityState, NewAccount, MAIN_ID};
use crate::categories::{self, CategoryType};
use crate::transactions;
use crate::vault::AppError;

const NOT_FOUND: AppError = AppError::NotFound;

/// An account that holds `opening` since the start of the year. Returns its id.
fn funded(conn: &rusqlite::Connection, name: &str, opening: &str) -> i64 {
    create(conn, &NewAccount { opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), ..new_account(name) }, TODAY).unwrap().id
}

/// A liability that was owed `owed` since the start of the year, in the currency `code` (none: the app's, a rate of 5.00 for another).
fn debt(conn: &rusqlite::Connection, name: &str, kind: Kind, owed: &str, code: Option<&str>) -> i64 {
    let (currency, rate) = (code.map(str::to_string), code.map(|_| "5.00".to_string()));
    create(conn, &NewAccount { kind, opening_balance: Some(format!("-{owed}")), opening_date: Some("2026-01-01".into()), currency, rate, ..new_account(name) }, TODAY).unwrap().id
}

fn payment(liability: i64, from: i64, amount: &str) -> Payment {
    Payment { liability_id: liability.to_string(), from_account_id: from.to_string(), amount: amount.into(), interest: None, category_id: None, date: None, note: None }
}

fn with_interest(liability: i64, from: i64, amount: &str, interest: &str, category: i64) -> Payment {
    Payment { interest: Some(interest.into()), category_id: Some(category.to_string()), ..payment(liability, from, amount) }
}

fn interest_category(conn: &rusqlite::Connection) -> i64 {
    categories::create(conn, "Interest", CategoryType::Expense).unwrap().id
}

fn now(conn: &rusqlite::Connection, id: i64) -> Account {
    get(conn, id, TODAY).unwrap()
}

#[test]
fn a_payment_is_a_transfer_of_the_principal_and_an_expense_of_the_interest_with_the_same_date_and_note() {
    let w = world();
    let (current, flat, interest) = (funded(&w.conn, "Current", "2000.00"), debt(&w.conn, "Flat", Kind::Mortgage, "180000.00", None), interest_category(&w.conn));
    let made = pay(&w.conn, &Payment { date: Some("2026-11-10".into()), note: Some(" November ".into()), ..with_interest(flat, current, "950.00", "585.00", interest) }, TODAY).unwrap();
    let (transfer, expense) = (made.transfer.clone().unwrap(), made.transaction.clone().unwrap());
    assert_eq!(
        (transfer.from_account_id, transfer.to_account_id, transfer.amount.as_str(), transfer.to_amount.as_deref(), transfer.date.as_str(), transfer.note.as_deref()),
        (current, flat, "365.00", None, "2026-11-10", Some("November"))
    );
    assert_eq!(
        (expense.name.as_str(), expense.amount.as_str(), expense.native_amount.as_deref(), expense.account_id, expense.category_id, expense.payment_date.as_str(), expense.description.as_deref()),
        ("Flat", "585.00", None, current, interest, "2026-11-10", Some("November")),
        "the name of the expense is the name of the liability"
    );
    assert_eq!(serde_json::to_value(&made).unwrap()["transfer"]["amount"], "365.00");
    assert_eq!((now(&w.conn, current).balance.as_str(), now(&w.conn, flat).balance.as_str()), ("1050.00", "-179635.00"), "the whole 950.00 left the account, and only the principal fell off the debt");
    let owed = now(&w.conn, flat).liability.unwrap();
    assert_eq!((owed.owed.as_str(), owed.state), ("179635.00", LiabilityState::Owing));
    // the two rows are ordinary rows afterwards: one is deleted without the other
    transactions::delete(&w.conn, expense.id).unwrap();
    assert_eq!(super::transfers::get(&w.conn, transfer.id).unwrap(), transfer);
    let again = pay(&w.conn, &with_interest(flat, current, "10.00", "4.00", interest), TODAY).unwrap();
    super::transfers::delete(&w.conn, again.transfer.unwrap().id).unwrap();
    assert_eq!(transactions::get(&w.conn, again.transaction.unwrap().id).unwrap().amount, "4.00");
    // left out, the date is today
    let today = pay(&w.conn, &with_interest(flat, current, "10.00", "4.00", interest), TODAY).unwrap();
    assert_eq!((today.transfer.unwrap().date.as_str(), today.transaction.unwrap().payment_date.as_str()), (TODAY, TODAY));
}

#[test]
fn a_payment_that_is_all_interest_makes_an_expense_and_no_transfer() {
    let w = world();
    let (current, card, interest) = (funded(&w.conn, "Current", "500.00"), debt(&w.conn, "Card", Kind::Credit, "300.00", None), interest_category(&w.conn));
    let made = pay(&w.conn, &with_interest(card, current, "12.00", "12.00", interest), TODAY).unwrap();
    assert_eq!(made.transfer, None);
    assert_eq!(made.transaction.unwrap().amount, "12.00");
    assert_eq!((now(&w.conn, current).balance.as_str(), now(&w.conn, card).balance.as_str()), ("488.00", "-300.00"), "the debt is as it was");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 0);
}

#[test]
fn a_payment_with_no_interest_makes_a_transfer_and_no_expense_and_asks_for_no_category() {
    let w = world();
    let (current, car) = (funded(&w.conn, "Current", "500.00"), debt(&w.conn, "Car", Kind::Loan, "300.00", None));
    for (what, input) in [
        ("interest left out", payment(car, current, "100.00")),
        ("interest 0.00", Payment { interest: Some("0.00".into()), ..payment(car, current, "100.00") }),
        // the category counts only when there is interest to book
        ("interest 0.00 with an income category", Payment { interest: Some("0.00".into()), category_id: Some(w.salary.to_string()), ..payment(car, current, "100.00") }),
    ] {
        let made = pay(&w.conn, &input, TODAY).unwrap();
        assert_eq!((made.transfer.map(|t| t.amount), made.transaction), (Some("100.00".to_string()), None), "{what}");
    }
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transactions"), 0);
    assert_eq!((now(&w.conn, current).balance.as_str(), now(&w.conn, car).balance.as_str()), ("200.00", "0.00"));
    // the same input as the page sends it
    let sent: Payment = serde_json::from_value(serde_json::json!({ "liabilityId": car.to_string(), "fromAccountId": current.to_string(), "amount": "1.00" })).unwrap();
    assert_eq!(pay(&w.conn, &sent, TODAY).unwrap().transfer.unwrap().amount, "1.00");
}

#[test]
fn a_payment_may_pay_the_debt_off_and_pay_more_than_is_owed_and_the_state_follows_the_balance() {
    let w = world();
    let (current, car) = (funded(&w.conn, "Current", "500.00"), debt(&w.conn, "Car", Kind::Loan, "100.00", None));
    let state = |conn: &rusqlite::Connection| {
        let owed = now(conn, car).liability.unwrap();
        (owed.owed, owed.state, owed.payoff)
    };
    assert_eq!(state(&w.conn).1, LiabilityState::Owing);
    pay(&w.conn, &payment(car, current, "100.00"), TODAY).unwrap();
    assert_eq!(state(&w.conn), ("0.00".to_string(), LiabilityState::PaidOff, None));
    pay(&w.conn, &payment(car, current, "50.00"), TODAY).unwrap();
    assert_eq!(state(&w.conn), ("0.00".to_string(), LiabilityState::Overpaid, None), "paying a debt that is paid off is allowed, and nothing is owed");
    assert_eq!(now(&w.conn, car).balance, "50.00");
}

#[test]
fn a_payment_from_an_account_in_the_currency_of_a_liability_in_another_one_is_in_that_currency() {
    let w = world();
    let interest = interest_category(&w.conn);
    let (lei, loan) = (create(&w.conn, &NewAccount { currency: Some("RON".into()), rate: Some("5.00".into()), opening_balance: Some("1000.00".into()), opening_date: Some("2026-01-01".into()), ..new_account("Lei") }, TODAY).unwrap().id, debt(&w.conn, "Lei loan", Kind::Loan, "500.00", Some("RON")));
    let made = pay(&w.conn, &with_interest(loan, lei, "100.00", "40.00", interest), TODAY).unwrap();
    let (transfer, expense) = (made.transfer.unwrap(), made.transaction.unwrap());
    assert_eq!((transfer.amount.as_str(), transfer.to_amount), ("60.00", None), "the same currency on both sides: one amount");
    assert_eq!(
        (expense.amount.as_str(), expense.native_amount.as_deref()),
        ("8.00", Some("40.00")),
        "what left the account in its currency, and what it is worth in the app's at the account's rate"
    );
    assert_eq!((now(&w.conn, lei).balance.as_str(), now(&w.conn, loan).balance.as_str()), ("900.00", "-440.00"), "in lei, as both accounts are");
    // the smallest interest is worth less than a cent in the app's currency and still counts one cent there
    let tiny = pay(&w.conn, &with_interest(loan, lei, "0.01", "0.01", interest), TODAY).unwrap().transaction.unwrap();
    assert_eq!((tiny.amount.as_str(), tiny.native_amount.as_deref()), ("0.01", Some("0.01")));
}

#[test]
fn every_payment_that_is_refused_leaves_no_transfer_and_no_transaction() {
    let mut w = world();
    let (current, flat, interest) = (funded(&w.conn, "Current", "2000.00"), debt(&w.conn, "Flat", Kind::Mortgage, "180000.00", None), interest_category(&w.conn));
    let (lei, lei_loan) = (
        create(&w.conn, &NewAccount { currency: Some("RON".into()), rate: Some("5.00".into()), opening_balance: Some("100.00".into()), opening_date: Some("2026-01-01".into()), ..new_account("Lei") }, TODAY).unwrap().id,
        debt(&w.conn, "Lei loan", Kind::Loan, "500.00", Some("RON")),
    );
    let dollars = create(&w.conn, &NewAccount { currency: Some("USD".into()), rate: Some("1.10".into()), opening_balance: Some("100.00".into()), opening_date: Some("2026-01-01".into()), ..new_account("Dollars") }, TODAY).unwrap().id;
    let old_card = debt(&w.conn, "Old card", Kind::Credit, "10.00", None);
    super::archive(&w.conn, old_card, TODAY).unwrap();
    let archived = funded(&w.conn, "Archived", "5.00");
    super::archive(&w.conn, archived, TODAY).unwrap();
    let gone = categories::create(&w.conn, "Gone", CategoryType::Expense).unwrap().id;
    categories::delete(&mut w.conn, gone, chrono::Utc::now()).unwrap();
    let ok = |amount: &str| with_interest(flat, current, amount, "1.00", interest);
    let bad = [
        ("interest above the amount", with_interest(flat, current, "100.00", "100.01", interest)),
        ("interest with no category", Payment { interest: Some("5.00".into()), ..payment(flat, current, "100.00") }),
        ("interest with an income category", with_interest(flat, current, "100.00", "5.00", w.salary)),
        ("interest with a category that is not there", with_interest(flat, current, "100.00", "5.00", 999)),
        ("interest with a category that is not a number", Payment { category_id: Some("abc".into()), ..with_interest(flat, current, "100.00", "5.00", interest) }),
        // the expense is refused after the transfer was made: both must go
        ("interest with an expense category that was deleted", with_interest(flat, current, "100.00", "5.00", gone)),
        ("from the liability itself", payment(flat, flat, "100.00")),
        ("from an archived account", payment(flat, archived, "5.00")),
        ("from an account that is not there", payment(flat, 999, "5.00")),
        ("from an id that is not an id", Payment { from_account_id: "x".into(), ..payment(flat, current, "5.00") }),
        ("an account in another currency to a liability in the app's", payment(flat, lei, "5.00")),
        ("an account in the app's currency to a liability in another", payment(lei_loan, current, "5.00")),
        ("two other currencies", payment(lei_loan, dollars, "5.00")),
        ("an account that is not a liability", payment(current, lei, "5.00")),
        ("the main account", payment(MAIN_ID, current, "5.00")),
        ("an archived liability", payment(old_card, current, "5.00")),
        ("a liability id that is not an id", Payment { liability_id: "x".into(), ..payment(flat, current, "5.00") }),
        ("an amount of 0.00", payment(flat, current, "0.00")),
        ("an amount of 0", payment(flat, current, "0")),
        ("a negative amount", payment(flat, current, "-5.00")),
        ("an amount with three decimals", payment(flat, current, "5.001")),
        ("an amount past the largest", payment(flat, current, "10000000000.00")),
        ("an amount that is no number", payment(flat, current, "abc")),
        ("interest that is no amount", Payment { interest: Some("x".into()), ..ok("5.00") }),
        ("negative interest", Payment { interest: Some("-1.00".into()), ..ok("5.00") }),
        ("interest of one decimal", Payment { interest: Some("1.5".into()), ..ok("5.00") }),
        ("a date that is not real", Payment { date: Some("2026-02-30".into()), ..ok("5.00") }),
        ("a date that is not padded", Payment { date: Some("2026-2-3".into()), ..ok("5.00") }),
        ("a note of 201 characters", Payment { note: Some("n".repeat(201)), ..ok("5.00") }),
        ("a note with a control character", Payment { note: Some("a\tb".into()), ..ok("5.00") }),
    ];
    for (what, input) in bad {
        assert_refused(&w.conn, INVALID, what, || pay(&w.conn, &input, TODAY));
    }
    assert_refused(&w.conn, NOT_FOUND, "a liability that is not there", || pay(&w.conn, &payment(999, current, "5.00"), TODAY));
    assert_refused(&w.conn, INVALID, "a today that is no date", || pay(&w.conn, &ok("5.00"), "today"));
    // and what is accepted next to them is paid
    let paid: Paid = pay(&w.conn, &ok("5.00"), TODAY).unwrap();
    assert_eq!((paid.transfer.unwrap().amount.as_str(), paid.transaction.unwrap().amount.as_str()), ("4.00", "1.00"));
}

#[test]
fn a_payment_as_the_page_sends_it_has_these_names_and_no_others() {
    let full = serde_json::json!({ "liabilityId": "3", "fromAccountId": "1", "amount": "950.00", "interest": "585.00", "categoryId": "7", "date": "2026-11-20", "note": "November" });
    let sent: Payment = serde_json::from_value(full.clone()).unwrap();
    assert_eq!((sent.liability_id.as_str(), sent.from_account_id.as_str(), sent.interest.as_deref(), sent.category_id.as_deref(), sent.date.as_deref(), sent.note.as_deref()), ("3", "1", Some("585.00"), Some("7"), Some("2026-11-20"), Some("November")));
    for key in ["liabilityId", "fromAccountId", "amount"] {
        let mut without = full.clone();
        without.as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<Payment>(without).is_err(), "{key} is needed");
    }
    let mut extra = full;
    extra["currency"] = "EUR".into();
    assert!(serde_json::from_value::<Payment>(extra).is_err(), "a name the core does not know is refused, not ignored");
}
