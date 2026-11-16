//! A row and its account (`docs/accounts-design.md`, section 5.2) and the life of a transfer (section 5.3), row by row of the state
//! machines, for what happens on this device. Every refusal is checked to have changed nothing.

use rusqlite::Connection;
use serde_json::json;

use super::fixtures::*;
use super::transfers::{self, Filter, NewTransfer, Transfer, TransferPatch};
use super::{archive, get, restore, MAIN_ID};
use crate::categories::CategoryType;
use crate::debts::codes::{self, Accept};
use crate::debts::fixtures::{equal, who};
use crate::debts::ledger::{self, EntryPatch, Expense, NewEntry};
use crate::debts::splits::{set_split, SplitInput};
use crate::recurring::{self, NewRecurring, RecurringPatch, Unit};
use crate::transactions::{self, NewTransaction, TransactionPatch};
use crate::vault::AppError;

const NOT_FOUND: AppError = AppError::NotFound;

fn new_tx(category: i64, account: Option<&str>) -> NewTransaction {
    NewTransaction { name: "Shop".into(), amount: "12.50".into(), payment_date: "2026-11-02".into(), category_id: category.to_string(), description: None, client_uuid: None, account_id: account.map(str::to_string), native_amount: None }
}

fn new_item(category: i64, account: Option<&str>) -> NewRecurring {
    NewRecurring {
        name: "Rent".into(),
        amount: Some("900.00".into()),
        category_id: category.to_string(),
        start_date: "2026-01-01".into(),
        interval_unit: Unit::Month,
        interval_count: 1,
        end_date: None,
        description: None,
        account_id: account.map(str::to_string),
    }
}

fn new_entry(person: i64, kind: ledger::Kind, account: Option<&str>) -> NewEntry {
    NewEntry { person_id: person.to_string(), kind, amount: "5.00".into(), date: "2026-11-02".into(), note: None, transaction_id: None, account_id: account.map(str::to_string) }
}

/// The text of a bill code for 25.00 of a dinner, as a friend would send it.
fn code(id: u8) -> String {
    let wire = json!({ "v": 1, "id": format!("{id:032x}"), "from": "Vincent", "what": "Dinner", "date": "2026-03-04", "amt": 2500, "total": 10000, "cur": "EUR" });
    format!("{}{}", codes::PREFIX, crate::sync::qr::base64url_encode(wire.to_string().as_bytes()))
}

fn accepting(text: String, expense: Option<i64>, account: Option<&str>) -> Accept {
    Accept { text, person_id: None, new_friend: Some("Vincent".into()), add_expense: expense.is_some(), category_id: expense.map(|c| c.to_string()), rate: None, amount: None, account_id: account.map(str::to_string) }
}

fn account_of(conn: &Connection, table: &str, id: i64) -> i64 {
    one(conn, &format!("SELECT account_id FROM {table} WHERE id = {id}"))
}

/// The accounts that a new row or a moved row cannot go to: one that is archived, one that is not there, one that was deleted, and text
/// that is no id.
fn closed(conn: &Connection) -> Vec<(&'static str, String)> {
    let old = archived_account(conn, "Old");
    let gone = account(conn, "Gone");
    super::delete(conn, gone).unwrap();
    vec![("an archived account", old.to_string()), ("an account that is not there", "999".into()), ("a deleted account", gone.to_string()), ("no id", "cash".into()), ("id 0", "0".into()), ("an empty id", String::new())]
}

// ---- 5.2: create -----------------------------------------------------------------------------

#[test]
fn a_new_row_that_names_no_account_is_in_the_main_account() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    account(&w.conn, "Cash");
    assert_eq!(transactions::create(&w.conn, &new_tx(w.food, None)).unwrap().account_id, MAIN_ID);
    assert_eq!(recurring::create(&w.conn, &new_item(w.rent, None)).unwrap().account_id, MAIN_ID);
    assert_eq!(ledger::add(&w.conn, &new_entry(anna, ledger::Kind::Lent, None)).unwrap().account_id, MAIN_ID);
    assert_eq!(ledger::settle(&w.conn, anna, None, None, None, None, None, TODAY).unwrap().entry.account_id, MAIN_ID);
    let borrowed = ledger::borrow_with_expense(&w.conn, &new_entry(anna, ledger::Kind::Borrowed, None), &Expense { name: "Pizza".into(), category_id: w.food.to_string() }).unwrap();
    assert_eq!(borrowed.account_id, MAIN_ID);
    assert_eq!(transactions::get(&w.conn, borrowed.transaction_id.unwrap().parse().unwrap()).unwrap().account_id, MAIN_ID);
    assert_eq!(codes::accept(&w.conn, &accepting(code(1), None, None)).unwrap().account_id, MAIN_ID);
    // the fields are optional in what the screens send
    assert!(serde_json::from_value::<NewTransaction>(json!({ "name": "A", "amount": "1.00", "paymentDate": "2026-01-01", "categoryId": "1" })).unwrap().account_id.is_none());
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transactions WHERE account_id <> 1") + one(&w.conn, "SELECT count(*) FROM ledger_entries WHERE account_id <> 1"), 0);
}

#[test]
fn a_new_row_goes_to_the_active_account_it_names() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let cash = account(&w.conn, "Cash");
    let named = cash.to_string();
    let named = Some(named.as_str());
    let shop = transactions::create(&w.conn, &new_tx(w.food, named)).unwrap();
    assert_eq!((shop.account_id, account_of(&w.conn, "transactions", shop.id)), (cash, cash));
    assert_eq!(serde_json::to_value(&shop).unwrap()["accountId"], json!(cash.to_string()), "the id crosses as text");
    let rent = recurring::create(&w.conn, &new_item(w.rent, named)).unwrap();
    assert_eq!((rent.account_id, serde_json::to_value(&rent).unwrap()["accountId"].clone()), (cash, json!(cash.to_string())));
    let loan = ledger::add(&w.conn, &new_entry(anna, ledger::Kind::Lent, named)).unwrap();
    assert_eq!((loan.account_id, serde_json::to_value(&loan).unwrap()["accountId"].clone()), (cash, json!(cash.to_string())));
    // "mark as paid" in cash
    let paid = ledger::settle(&w.conn, anna, None, None, None, None, named, TODAY).unwrap();
    assert_eq!(paid.entry.account_id, cash);
    // "my share of a bill": the expense and the entry are in the same account
    let borrowed = ledger::borrow_with_expense(&w.conn, &new_entry(anna, ledger::Kind::Borrowed, named), &Expense { name: "Pizza".into(), category_id: w.food.to_string() }).unwrap();
    assert_eq!(borrowed.account_id, cash);
    assert_eq!(account_of(&w.conn, "transactions", borrowed.transaction_id.unwrap().parse().unwrap()), cash);
    // a bill code, with and without the expense
    assert_eq!(codes::accept(&w.conn, &accepting(code(1), None, named)).unwrap().account_id, cash);
    let with_expense = codes::accept(&w.conn, &Accept { new_friend: Some("Other".into()), ..accepting(code(2), Some(w.food), named) }).unwrap();
    assert_eq!((with_expense.account_id, account_of(&w.conn, "transactions", with_expense.transaction_id.unwrap().parse().unwrap())), (cash, cash));
    // naming the main account is the same as naming none
    assert_eq!(transactions::create(&w.conn, &new_tx(w.food, Some("1"))).unwrap().account_id, MAIN_ID);
    // and the list of transactions can be asked for one account
    let in_cash = transactions::list(&w.conn, &transactions::Filter { account_id: Some(cash.to_string()), ..Default::default() }).unwrap();
    assert_eq!(in_cash.total, 3);
    assert!(in_cash.transactions.iter().all(|t| t.account_id == cash));
    assert_eq!(transactions::list(&w.conn, &transactions::Filter { account_id: Some("1".into()), ..Default::default() }).unwrap().total, 1);
    assert_eq!(transactions::list(&w.conn, &transactions::Filter { account_id: Some("x".into()), ..Default::default() }).err(), Some(INVALID));
}

/// 5.5 item 6, the creations.
#[test]
fn a_new_row_in_an_account_that_is_archived_or_not_there_is_refused_and_nothing_is_made() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let cash = account(&w.conn, "Cash");
    ledger::add(&w.conn, &new_entry(anna, ledger::Kind::Lent, None)).unwrap();
    for (what, id) in closed(&w.conn) {
        let named = Some(id.as_str());
        assert_refused(&w.conn, INVALID, &format!("a transaction in {what}"), || transactions::create(&w.conn, &new_tx(w.food, named)));
        assert_refused(&w.conn, INVALID, &format!("a recurring item in {what}"), || recurring::create(&w.conn, &new_item(w.rent, named)));
        assert_refused(&w.conn, INVALID, &format!("a ledger entry in {what}"), || ledger::add(&w.conn, &new_entry(anna, ledger::Kind::Lent, named)));
        // neither the expense nor the entry of "my share of a bill"
        assert_refused(&w.conn, INVALID, &format!("a borrowing with its expense in {what}"), || {
            ledger::borrow_with_expense(&w.conn, &new_entry(anna, ledger::Kind::Borrowed, named), &Expense { name: "Pizza".into(), category_id: w.food.to_string() })
        });
        assert_refused(&w.conn, INVALID, &format!("a repayment in {what}"), || ledger::settle(&w.conn, anna, None, None, None, None, named, TODAY));
        // not the friend, the expense or the entry of a bill code
        assert_refused(&w.conn, INVALID, &format!("a bill code in {what}"), || codes::accept(&w.conn, &accepting(code(1), None, named)));
        assert_refused(&w.conn, INVALID, &format!("a bill code with its expense in {what}"), || codes::accept(&w.conn, &accepting(code(1), Some(w.food), named)));
        assert_refused(&w.conn, INVALID, &format!("a transfer from {what}"), || {
            transfers::create(&w.conn, &NewTransfer { from_account_id: id.clone(), ..new_transfer(MAIN_ID, cash, "5.00", "2026-11-02") })
        });
        assert_refused(&w.conn, INVALID, &format!("a transfer to {what}"), || {
            transfers::create(&w.conn, &NewTransfer { to_account_id: id.clone(), ..new_transfer(cash, MAIN_ID, "5.00", "2026-11-02") })
        });
    }
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transactions") + one(&w.conn, "SELECT count(*) FROM recurring_transactions") + one(&w.conn, "SELECT count(*) FROM transfers"), 0);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM ledger_entries"), 1);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM people"), 1, "the friend of a refused code was not made");
}

#[test]
fn a_replayed_transaction_is_answered_as_it_is_whatever_account_the_retry_names() {
    let w = world();
    let cash = account(&w.conn, "Cash");
    let input = NewTransaction { client_uuid: Some("0b8f6c1e-5c52-4d6a-9d50-3f7a8a1b2c3d".into()), ..new_tx(w.food, Some(&cash.to_string())) };
    let first = transactions::create(&w.conn, &input).unwrap();
    archive(&w.conn, cash, TODAY).unwrap();
    let again = assert_nothing_written(&w.conn, "the same request again", || transactions::create(&w.conn, &input));
    assert_eq!((again.id, again.account_id), (first.id, cash));
}

// ---- 5.2: move, and edits of a row in an archived account ---------------------------------------

struct Rows {
    cash: i64,
    card: i64,
    shop: i64,
    rent: i64,
    loan: i64,
}

/// A transaction, a recurring item and a ledger entry in the account "Cash", and an empty account "Card".
fn rows(w: &World) -> Rows {
    let anna = friend(&w.conn, "Anna");
    let (cash, card) = (account(&w.conn, "Cash"), account(&w.conn, "Card"));
    let named = cash.to_string();
    let shop = transactions::create(&w.conn, &new_tx(w.food, Some(&named))).unwrap().id;
    let rent = recurring::create(&w.conn, &new_item(w.rent, Some(&named))).unwrap().id;
    let loan = ledger::add(&w.conn, &new_entry(anna, ledger::Kind::Lent, Some(&named))).unwrap().id;
    Rows { cash, card, shop, rent, loan }
}

fn move_tx(to: &str) -> TransactionPatch {
    TransactionPatch { account_id: Some(to.into()), ..Default::default() }
}

fn move_item(to: &str) -> RecurringPatch {
    RecurringPatch { account_id: Some(to.into()), ..Default::default() }
}

fn move_entry(to: &str) -> EntryPatch {
    EntryPatch { account_id: Some(to.into()), ..Default::default() }
}

#[test]
fn a_row_moves_to_another_active_account_with_a_new_stamp() {
    let w = world();
    let r = rows(&w);
    let to = r.card.to_string();
    let stamps = || [stamp_of(&w.conn, "transactions", r.shop), stamp_of(&w.conn, "recurring_transactions", r.rent), stamp_of(&w.conn, "ledger_entries", r.loan)];
    let before = stamps();
    assert_eq!(transactions::update(&w.conn, r.shop, &move_tx(&to)).unwrap().account_id, r.card);
    assert_eq!(recurring::update(&w.conn, r.rent, &move_item(&to)).unwrap().account_id, r.card);
    assert_eq!(ledger::update(&w.conn, r.loan, &move_entry(&to)).unwrap().account_id, r.card);
    let after = stamps();
    assert!(after.iter().zip(&before).all(|(new, old)| new > old), "{before:?} {after:?}");
    assert_eq!((get(&w.conn, r.cash, TODAY).unwrap().entries, get(&w.conn, r.card, TODAY).unwrap().entries), (0, 3));
    // nothing else about the rows changed
    let shop = transactions::get(&w.conn, r.shop).unwrap();
    assert_eq!((shop.name.as_str(), shop.amount.as_str(), shop.payment_date.as_str(), shop.category_id), ("Shop", "12.50", "2026-11-02", w.food));
    let rent = recurring::get(&w.conn, r.rent).unwrap();
    assert_eq!((rent.amount.as_deref(), rent.next_due_date.as_str(), rent.is_active), (Some("900.00"), "2026-01-01", true));
    // an update that names no account leaves it where it is
    assert_eq!(transactions::update(&w.conn, r.shop, &TransactionPatch { name: Some("Market".into()), ..Default::default() }).unwrap().account_id, r.card);
    assert_eq!(recurring::update(&w.conn, r.rent, &RecurringPatch { name: Some("Flat".into()), ..Default::default() }).unwrap().account_id, r.card);
    assert_eq!(ledger::update(&w.conn, r.loan, &EntryPatch { amount: Some("6.00".into()), ..Default::default() }).unwrap().account_id, r.card);
    // a ledger entry gets a stamp on a real change only: naming the account it has is none
    assert_nothing_written(&w.conn, "an entry updated with the account it has", || ledger::update(&w.conn, r.loan, &move_entry(&to)));
}

/// 5.5 item 6, the moves.
#[test]
fn a_row_is_not_moved_into_an_account_that_is_archived_or_not_there() {
    let w = world();
    let r = rows(&w);
    for (what, id) in closed(&w.conn) {
        assert_refused(&w.conn, INVALID, &format!("move a transaction to {what}"), || transactions::update(&w.conn, r.shop, &move_tx(&id)));
        assert_refused(&w.conn, INVALID, &format!("move a recurring item to {what}"), || recurring::update(&w.conn, r.rent, &move_item(&id)));
        assert_refused(&w.conn, INVALID, &format!("move a ledger entry to {what}"), || ledger::update(&w.conn, r.loan, &move_entry(&id)));
        // a good field with it is refused too
        assert_refused(&w.conn, INVALID, &format!("rename and move a transaction to {what}"), || {
            transactions::update(&w.conn, r.shop, &TransactionPatch { name: Some("Market".into()), ..move_tx(&id) })
        });
    }
    for (table, id) in [("transactions", r.shop), ("recurring_transactions", r.rent), ("ledger_entries", r.loan)] {
        assert_eq!(account_of(&w.conn, table, id), r.cash, "{table}");
    }
}

/// 5.5 item 6, the rows already in an archived account.
#[test]
fn a_row_in_an_archived_account_can_be_edited_named_again_moved_out_and_deleted_but_not_moved_back() {
    let w = world();
    let r = rows(&w);
    let stamps = || [stamp_of(&w.conn, "transactions", r.shop), stamp_of(&w.conn, "recurring_transactions", r.rent), stamp_of(&w.conn, "ledger_entries", r.loan)];
    let before = stamps();
    let archived = archive(&w.conn, r.cash, TODAY).unwrap();
    assert_eq!(stamps(), before, "archiving the account writes none of its rows");
    assert_eq!(archived.entries, 3);
    let own = r.cash.to_string();

    // edit other fields: allowed, the row stays in the archived account
    let shop = transactions::update(&w.conn, r.shop, &TransactionPatch { amount: Some("20.00".into()), ..Default::default() }).unwrap();
    assert_eq!((shop.amount.as_str(), shop.account_id), ("20.00", r.cash));
    assert_eq!(recurring::update(&w.conn, r.rent, &RecurringPatch { amount: Some(Some("950.00".into())), ..Default::default() }).unwrap().account_id, r.cash);
    assert_eq!(ledger::update(&w.conn, r.loan, &EntryPatch { note: Some(Some("taxi".into())), ..Default::default() }).unwrap().account_id, r.cash);
    assert_eq!(get(&w.conn, r.cash, TODAY).unwrap().balance, "-25.00", "and its balance follows: -20.00 - 5.00");

    // an update that names the archived account it already has is not a move: allowed
    let named = transactions::update(&w.conn, r.shop, &TransactionPatch { name: Some("Market".into()), ..move_tx(&own) }).unwrap();
    assert_eq!((named.name.as_str(), named.account_id), ("Market", r.cash));
    assert_eq!(recurring::update(&w.conn, r.rent, &move_item(&own)).unwrap().account_id, r.cash);
    assert_eq!(ledger::update(&w.conn, r.loan, &EntryPatch { amount: Some("7.00".into()), ..move_entry(&own) }).unwrap().account_id, r.cash);

    // move out to an active account: allowed; back into the archived one: refused
    let card = r.card.to_string();
    assert_eq!(transactions::update(&w.conn, r.shop, &move_tx(&card)).unwrap().account_id, r.card);
    assert_refused(&w.conn, INVALID, "move a transaction back into the archived account", || transactions::update(&w.conn, r.shop, &move_tx(&own)));
    assert_eq!(ledger::update(&w.conn, r.loan, &move_entry("1")).unwrap().account_id, MAIN_ID);
    assert_refused(&w.conn, INVALID, "move an entry back into the archived account", || ledger::update(&w.conn, r.loan, &move_entry(&own)));

    // delete a row that is in the archived account: allowed
    recurring::delete(&w.conn, r.rent).unwrap();
    assert_eq!(get(&w.conn, r.cash, TODAY).unwrap().entries, 0);

    // once the account is restored, rows go into it again, and restoring wrote none
    let before = stamp_of(&w.conn, "transactions", r.shop);
    restore(&w.conn, r.cash, TODAY).unwrap();
    assert_eq!(stamp_of(&w.conn, "transactions", r.shop), before);
    assert_eq!(transactions::update(&w.conn, r.shop, &move_tx(&own)).unwrap().account_id, r.cash);
}

// ---- 5.2: paying a recurring item ---------------------------------------------------------------

/// 5.5 item 8.
#[test]
fn a_payment_is_made_in_the_account_of_its_item_or_in_the_main_account_when_that_one_is_archived() {
    let w = world();
    let r = rows(&w);
    let first = recurring::pay(&w.conn, r.rent, None, None, TODAY, None, None).unwrap();
    assert_eq!((first.account_id, first.amount.as_str()), (r.cash, "900.00"), "the payment copies the item's account");
    assert_eq!(get(&w.conn, r.cash, TODAY).unwrap().balance, "-917.50");

    archive(&w.conn, r.cash, TODAY).unwrap();
    let second = recurring::pay(&w.conn, r.rent, Some("910.00"), None, TODAY, None, None).unwrap();
    assert_eq!(second.account_id, MAIN_ID, "the account is archived: the payment is in the main account");
    // a later occurrence paid on its own goes the same way
    let later = recurring::pay(&w.conn, r.rent, None, None, TODAY, Some("2026-05-01"), None).unwrap();
    assert_eq!(later.account_id, MAIN_ID);
    assert_eq!((get(&w.conn, r.cash, TODAY).unwrap().balance.as_str(), get(&w.conn, MAIN_ID, TODAY).unwrap().balance.as_str()), ("-917.50", "-1810.00"));
    let item = recurring::get(&w.conn, r.rent).unwrap();
    assert_eq!((item.account_id, item.next_due_date.as_str()), (r.cash, "2026-03-01"), "the item keeps its account and only moves on");

    // restored, the next payment is in the item's account again
    restore(&w.conn, r.cash, TODAY).unwrap();
    assert_eq!(recurring::pay(&w.conn, r.rent, None, None, TODAY, None, None).unwrap().account_id, r.cash);
    // and a payment that is refused makes nothing
    assert_refused(&w.conn, INVALID, "an occurrence that is paid already", || recurring::pay(&w.conn, r.rent, None, None, TODAY, Some("2026-05-01"), None));
}

// ---- 5.3: a transfer ---------------------------------------------------------------------------

#[test]
fn a_transfer_is_made_between_two_active_accounts_and_moves_both_balances() {
    let w = world();
    let cash = account(&w.conn, "Cash");
    let made = transfers::create(&w.conn, &NewTransfer { note: Some("  for the\r\nweekend  ".into()), ..new_transfer(MAIN_ID, cash, "200.00", "2026-11-03") }).unwrap();
    assert_eq!(made, Transfer { id: 1, from_account_id: MAIN_ID, to_account_id: cash, amount: "200.00".into(), to_amount: None, date: "2026-11-03".into(), note: Some("for the\nweekend".into()) });
    assert_eq!(
        serde_json::to_value(&made).unwrap(),
        json!({ "id": "1", "fromAccountId": "1", "toAccountId": cash.to_string(), "amount": "200.00", "toAmount": null, "date": "2026-11-03", "note": "for the\nweekend" })
    );
    assert_eq!(transfers::get(&w.conn, made.id).unwrap(), made);
    assert_eq!((get(&w.conn, MAIN_ID, TODAY).unwrap().balance.as_str(), get(&w.conn, cash, TODAY).unwrap().balance.as_str()), ("-200.00", "200.00"));
    assert_eq!(super::summary(&w.conn, TODAY).unwrap().in_accounts, "0.00", "a transfer changes no total");
    // an empty note is none; the smallest and the largest amount
    assert_eq!(transfers::create(&w.conn, &NewTransfer { note: Some("   ".into()), ..new_transfer(cash, MAIN_ID, "0.01", "2026-11-03") }).unwrap().note, None);
    assert_eq!(transfers::create(&w.conn, &new_transfer(cash, MAIN_ID, "9999999999.99", "2026-11-03")).unwrap().amount, "9999999999.99");
    assert!(transfers::create(&w.conn, &NewTransfer { note: Some("x".repeat(200)), ..new_transfer(cash, MAIN_ID, "1.00", "2026-11-03") }).is_ok());
    // it is no income and no expense: nothing that counts spending sees it
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transactions") + one(&w.conn, "SELECT count(*) FROM transaction_costs"), 0);
}

/// 5.5 item 7, the creations.
#[test]
fn a_transfer_to_itself_of_nothing_or_with_a_bad_field_is_not_made() {
    let w = world();
    let cash = account(&w.conn, "Cash");
    let good = || new_transfer(MAIN_ID, cash, "5.00", "2026-11-03");
    let bad = [
        ("to itself", new_transfer(cash, cash, "5.00", "2026-11-03")),
        ("the main account to itself", new_transfer(MAIN_ID, MAIN_ID, "5.00", "2026-11-03")),
        ("amount 0.00", NewTransfer { amount: "0.00".into(), ..good() }),
        ("amount -1.00", NewTransfer { amount: "-1.00".into(), ..good() }),
        ("three decimals", NewTransfer { amount: "1.000".into(), ..good() }),
        ("no decimals", NewTransfer { amount: "1".into(), ..good() }),
        ("too large", NewTransfer { amount: "10000000000.00".into(), ..good() }),
        ("an unknown account on the from side", NewTransfer { from_account_id: "999".into(), ..good() }),
        ("an unknown account on the to side", NewTransfer { to_account_id: "999".into(), ..good() }),
        ("an unknown account on both sides", new_transfer(999, 999, "5.00", "2026-11-03")),
        ("a date that is not real", NewTransfer { date: "2026-02-30".into(), ..good() }),
        ("a date not padded", NewTransfer { date: "2026-2-3".into(), ..good() }),
        ("a long note", NewTransfer { note: Some("x".repeat(201)), ..good() }),
        ("a control character in the note", NewTransfer { note: Some("a\tb".into()), ..good() }),
    ];
    for (what, input) in bad {
        assert_refused(&w.conn, INVALID, what, || transfers::create(&w.conn, &input));
    }
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 0);
    assert!(serde_json::from_value::<NewTransfer>(json!({ "fromAccountId": "1", "toAccountId": "2", "amount": "1.00", "date": "2026-01-01", "kind": "x" })).is_err(), "an unknown field is refused");
    assert!(serde_json::from_value::<NewTransfer>(json!({ "fromAccountId": "1", "amount": "1.00", "date": "2026-01-01" })).is_err(), "both sides are needed");
}

#[test]
fn an_update_of_a_transfer_writes_the_fields_given_and_makes_a_stamp_only_on_a_change() {
    let w = world();
    let (cash, card) = (account(&w.conn, "Cash"), account(&w.conn, "Card"));
    let id = transfer(&w.conn, MAIN_ID, cash, "200.00", "2026-11-03");
    let made = stamp_of(&w.conn, "transfers", id);
    let patch = TransferPatch { from_account_id: Some(card.to_string()), to_account_id: Some(MAIN_ID.to_string()), amount: Some("150.00".into()), date: Some("2026-11-04".into()), note: Some(Some("card bill".into())), to_amount: None };
    let changed = transfers::update(&w.conn, id, &patch).unwrap();
    assert_eq!(changed, Transfer { id, from_account_id: card, to_account_id: MAIN_ID, amount: "150.00".into(), to_amount: None, date: "2026-11-04".into(), note: Some("card bill".into()) });
    let stamp = stamp_of(&w.conn, "transfers", id);
    assert!(stamp > made);
    assert_eq!([MAIN_ID, cash, card].map(|a| balance(&w.conn, a, TODAY)), [15000, 0, -15000], "the old sides hold nothing of it any more");

    assert_eq!(assert_nothing_written(&w.conn, "the same values", || transfers::update(&w.conn, id, &patch)), changed);
    assert_eq!(assert_nothing_written(&w.conn, "an empty patch", || transfers::update(&w.conn, id, &TransferPatch::default())), changed);
    // `note: null` clears it; a field left out stays
    let cleared = transfers::update(&w.conn, id, &serde_json::from_value::<TransferPatch>(json!({ "note": null })).unwrap()).unwrap();
    assert_eq!((cleared.note, cleared.amount.as_str()), (None, "150.00"));
    assert!(stamp_of(&w.conn, "transfers", id) > stamp);
    // the two sides can change places in one update
    let swapped = transfers::update(&w.conn, id, &TransferPatch { from_account_id: Some(MAIN_ID.to_string()), to_account_id: Some(card.to_string()), ..Default::default() }).unwrap();
    assert_eq!((swapped.from_account_id, swapped.to_account_id), (MAIN_ID, card));
}

/// 5.5 item 7, the edits.
#[test]
fn an_update_that_would_make_both_sides_equal_or_has_a_bad_field_changes_nothing() {
    let w = world();
    let cash = account(&w.conn, "Cash");
    let id = transfer(&w.conn, MAIN_ID, cash, "200.00", "2026-11-03");
    let side = |from: Option<i64>, to: Option<i64>| TransferPatch { from_account_id: from.map(|a| a.to_string()), to_account_id: to.map(|a| a.to_string()), ..Default::default() };
    let bad = [
        ("the to side becomes the from side", side(None, Some(MAIN_ID))),
        ("the from side becomes the to side", side(Some(cash), None)),
        ("both sides the same new account", side(Some(cash), Some(cash))),
        ("an unknown from side", side(Some(999), None)),
        ("an unknown to side", side(None, Some(999))),
        ("amount 0.00", TransferPatch { amount: Some("0.00".into()), ..Default::default() }),
        ("amount -1.00", TransferPatch { amount: Some("-1.00".into()), ..Default::default() }),
        ("a bad date", TransferPatch { date: Some("2026-13-01".into()), ..Default::default() }),
        ("a long note", TransferPatch { note: Some(Some("x".repeat(201))), ..Default::default() }),
        ("a good amount with a bad side", TransferPatch { amount: Some("1.00".into()), ..side(None, Some(MAIN_ID)) }),
    ];
    for (what, patch) in bad {
        assert_refused(&w.conn, INVALID, what, || transfers::update(&w.conn, id, &patch));
    }
    assert_refused(&w.conn, INVALID, "a side that is no id", || transfers::update(&w.conn, id, &TransferPatch { to_account_id: Some("cash".into()), ..Default::default() }));
    assert_eq!(transfers::get(&w.conn, id).unwrap().amount, "200.00");
}

/// 5.5 item 6, for the sides of a transfer.
#[test]
fn a_side_that_does_not_change_may_be_archived_and_a_side_that_changes_must_be_active() {
    let w = world();
    let (cash, card, other) = (account(&w.conn, "Cash"), account(&w.conn, "Card"), account(&w.conn, "Other"));
    let id = transfer(&w.conn, cash, card, "200.00", "2026-11-03");
    let before = (stamp_of(&w.conn, "transfers", id), text(&w.conn, "SELECT updated_at FROM transfers"));
    archive(&w.conn, cash, TODAY).unwrap();
    archive(&w.conn, card, TODAY).unwrap();
    assert_eq!((stamp_of(&w.conn, "transfers", id), text(&w.conn, "SELECT updated_at FROM transfers")), before, "archiving a side writes nothing to the transfer");
    assert_eq!([cash, card].map(|a| balance(&w.conn, a, TODAY)), [-20000, 20000], "and it still counts in both balances");

    // both sides archived: the amount, the date and the note can still change, and the sides can be named as they are
    let own = TransferPatch { from_account_id: Some(cash.to_string()), to_account_id: Some(card.to_string()), ..Default::default() };
    let edited = transfers::update(&w.conn, id, &TransferPatch { amount: Some("180.00".into()), date: Some("2026-11-05".into()), note: Some(Some("late".into())), ..own.clone() }).unwrap();
    assert_eq!((edited.amount.as_str(), edited.from_account_id, edited.to_account_id), ("180.00", cash, card));
    assert_nothing_written(&w.conn, "both archived sides named again", || transfers::update(&w.conn, id, &own));

    // a side that changes must be active: not the other archived account, in either place
    assert_refused(&w.conn, INVALID, "the sides change places while both are archived", || {
        transfers::update(&w.conn, id, &TransferPatch { from_account_id: Some(card.to_string()), to_account_id: Some(cash.to_string()), ..Default::default() })
    });
    let third = archived_account(&w.conn, "Third");
    assert_refused(&w.conn, INVALID, "a side changes to an archived account", || transfers::update(&w.conn, id, &TransferPatch { to_account_id: Some(third.to_string()), ..Default::default() }));
    // one side moves to an active account, the other stays archived
    let moved = transfers::update(&w.conn, id, &TransferPatch { to_account_id: Some(other.to_string()), ..Default::default() }).unwrap();
    assert_eq!((moved.from_account_id, moved.to_account_id), (cash, other));
    assert_refused(&w.conn, INVALID, "and not back", || transfers::update(&w.conn, id, &TransferPatch { to_account_id: Some(card.to_string()), ..Default::default() }));

    // restoring a side writes nothing to the transfer either
    let before = stamp_of(&w.conn, "transfers", id);
    restore(&w.conn, cash, TODAY).unwrap();
    assert_eq!(stamp_of(&w.conn, "transfers", id), before);
    // a transfer with an archived side can be deleted
    archive(&w.conn, cash, TODAY).unwrap();
    transfers::delete(&w.conn, id).unwrap();
}

#[test]
fn a_deleted_transfer_leaves_a_marker_and_is_not_found_again() {
    let w = world();
    let cash = account(&w.conn, "Cash");
    let id = transfer(&w.conn, MAIN_ID, cash, "200.00", "2026-11-03");
    let (sync_id, stamp) = (text(&w.conn, "SELECT sync_id FROM transfers"), stamp_of(&w.conn, "transfers", id));
    transfers::delete(&w.conn, id).unwrap();
    assert!(text(&w.conn, &format!("SELECT stamp FROM sync_deleted WHERE sync_id = '{sync_id}' AND kind = 'transfers'")) > stamp);
    assert_eq!([MAIN_ID, cash].map(|a| balance(&w.conn, a, TODAY)), [0, 0]);
    for gone in [id, 999] {
        assert_refused(&w.conn, NOT_FOUND, "get", || transfers::get(&w.conn, gone));
        assert_refused(&w.conn, NOT_FOUND, "update", || transfers::update(&w.conn, gone, &TransferPatch { amount: Some("1.00".into()), ..Default::default() }));
        assert_refused(&w.conn, NOT_FOUND, "delete", || transfers::delete(&w.conn, gone));
    }
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 1);
}

#[test]
fn the_list_of_transfers_is_newest_first_by_account_and_dates_a_page_at_a_time() {
    let w = world();
    let (cash, card) = (account(&w.conn, "Cash"), account(&w.conn, "Card"));
    let a = transfer(&w.conn, MAIN_ID, cash, "1.00", "2026-10-01");
    let b = transfer(&w.conn, cash, card, "2.00", "2026-11-01");
    let c = transfer(&w.conn, card, MAIN_ID, "3.00", "2026-11-01");
    let d = transfer(&w.conn, MAIN_ID, card, "4.00", "2026-12-01");
    let ids = |filter: Filter| -> Vec<i64> { transfers::list(&w.conn, &filter).unwrap().transfers.into_iter().map(|t| t.id).collect() };
    assert_eq!(ids(Filter::default()), [d, c, b, a]);
    assert_eq!(ids(Filter { account_id: Some(cash.to_string()), ..Default::default() }), [b, a], "either side");
    assert_eq!(ids(Filter { account_id: Some(card.to_string()), from: Some("2026-11-01".into()), to: Some("2026-11-30".into()), ..Default::default() }), [c, b]);
    assert_eq!(ids(Filter { from: Some("2026-11-01".into()), ..Default::default() }), [d, c, b]);
    assert_eq!(ids(Filter { to: Some("2026-11-01".into()), ..Default::default() }), [c, b, a]);
    assert_eq!(ids(Filter { account_id: Some("999".into()), ..Default::default() }), Vec::<i64>::new());
    let page = transfers::list(&w.conn, &Filter { limit: Some(2), offset: Some(1), ..Default::default() }).unwrap();
    assert_eq!((page.transfers.iter().map(|t| t.id).collect::<Vec<_>>(), page.total, page.limit, page.offset), (vec![c, b], 4, 2, 1));
    assert_eq!(transfers::list(&w.conn, &Filter::default()).unwrap().limit, 50);
    for bad in [
        Filter { limit: Some(0), ..Default::default() },
        Filter { limit: Some(101), ..Default::default() },
        Filter { from: Some("November".into()), ..Default::default() },
        Filter { to: Some("2026-11-31".into()), ..Default::default() },
        Filter { account_id: Some("x".into()), ..Default::default() },
    ] {
        assert_eq!(transfers::list(&w.conn, &bad).err(), Some(INVALID), "{bad:?}");
    }
    assert!(serde_json::from_value::<Filter>(json!({ "q": "x" })).is_err(), "an unknown field is refused");
}

// ---- 21.3: a currency of its own -----------------------------------------------------------------

/// An account in another currency. Returns its id.
fn foreign(conn: &Connection, name: &str, code: &str, rate: &str) -> i64 {
    super::create(conn, &super::NewAccount { currency: Some(code.into()), rate: Some(rate.into()), ..new_account(name) }, TODAY).unwrap().id
}

/// The accounts of these tests: two in lei, one in dollars, one that names the main currency (and is stored with none).
struct Currencies {
    lei: i64,
    lei2: i64,
    usd: i64,
    euros: i64,
}

fn currencies(w: &World) -> Currencies {
    Currencies {
        lei: foreign(&w.conn, "Lei", "RON", "5.00"),
        lei2: foreign(&w.conn, "More lei", "ron", "4.00"),
        usd: foreign(&w.conn, "Dollars", "USD", "1.10"),
        euros: super::create(&w.conn, &super::NewAccount { currency: Some("eur".into()), ..new_account("Euros") }, TODAY).unwrap().id,
    }
}

fn spent(category: i64, account: Option<i64>, amount: &str, native: Option<&str>) -> NewTransaction {
    NewTransaction { native_amount: native.map(str::to_string), amount: amount.into(), ..new_tx(category, account.map(|a| a.to_string()).as_deref()) }
}

fn native_patch(native: &str) -> TransactionPatch {
    TransactionPatch { native_amount: Some(native.into()), ..Default::default() }
}

fn native_column(conn: &Connection, id: i64) -> Option<i64> {
    conn.query_row("SELECT native_amount_minor FROM transactions WHERE id = ?1", [id], |r| r.get(0)).unwrap()
}

fn arrives_column(conn: &Connection, id: i64) -> Option<i64> {
    conn.query_row("SELECT to_amount_minor FROM transfers WHERE id = ?1", [id], |r| r.get(0)).unwrap()
}

#[test]
fn a_transaction_in_a_foreign_account_needs_its_native_amount_and_moves_the_account_by_it() {
    let w = world();
    let c = currencies(&w);
    let shop = transactions::create(&w.conn, &spent(w.food, Some(c.lei), "22.75", Some("113.74"))).unwrap();
    assert_eq!((shop.amount.as_str(), shop.native_amount.as_deref(), shop.my_share.as_str(), shop.account_id), ("22.75", Some("113.74"), "22.75", c.lei));
    assert_eq!(transactions::get(&w.conn, shop.id).unwrap(), shop);
    assert_eq!(serde_json::to_value(&shop).unwrap()["nativeAmount"], json!("113.74"));
    assert_eq!((balance(&w.conn, c.lei, TODAY), balance(&w.conn, MAIN_ID, TODAY)), (-11_374, 0), "the account moved by what it really paid");
    // income is the other way
    let pay = transactions::create(&w.conn, &spent(w.salary, Some(c.lei), "100.00", Some("500.50"))).unwrap();
    assert_eq!(balance(&w.conn, c.lei, TODAY), -11_374 + 50_050);
    assert_eq!(transactions::list(&w.conn, &transactions::Filter { account_id: Some(c.lei.to_string()), ..Default::default() }).unwrap().transactions.iter().map(|t| t.native_amount.clone()).collect::<Vec<_>>(), [Some("500.50".to_string()), Some("113.74".to_string())]);
    // no native amount for a foreign account: refused, and nothing is made
    assert_refused(&w.conn, INVALID, "a foreign account and no native amount", || transactions::create(&w.conn, &spent(w.food, Some(c.lei), "22.75", None)));
    assert_refused(&w.conn, INVALID, "the same, in dollars", || transactions::create(&w.conn, &spent(w.food, Some(c.usd), "22.75", None)));
    // and one for an account in the main currency: refused, whether it is named or the default, and whether it is stored with none or named so
    for (what, account) in [("the main account by default", None), ("the main account named", Some(MAIN_ID)), ("an account that names the main currency", Some(c.euros))] {
        assert_refused(&w.conn, INVALID, &format!("a native amount for {what}"), || transactions::create(&w.conn, &spent(w.food, account, "22.75", Some("113.74"))));
        let plain = transactions::create(&w.conn, &spent(w.food, account, "22.75", None)).unwrap();
        assert_eq!((plain.native_amount.clone(), serde_json::to_value(&plain).unwrap()["nativeAmount"].clone()), (None, json!(null)), "{what}");
    }
    assert_eq!(native_column(&w.conn, shop.id), Some(11_374));
    assert_eq!(native_column(&w.conn, pay.id), Some(50_050));
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transactions WHERE native_amount_minor IS NOT NULL"), 2);
    // the fields are optional in what the screens send, and an unknown one is refused
    assert!(serde_json::from_value::<NewTransaction>(json!({ "name": "A", "amount": "1.00", "paymentDate": "2026-01-01", "categoryId": "1" })).unwrap().native_amount.is_none());
    assert_eq!(serde_json::from_value::<NewTransaction>(json!({ "name": "A", "amount": "1.00", "paymentDate": "2026-01-01", "categoryId": "1", "nativeAmount": "5.00" })).unwrap().native_amount.as_deref(), Some("5.00"));
    assert!(serde_json::from_value::<TransactionPatch>(json!({ "nativeAmounts": "5.00" })).is_err());
    assert_eq!(serde_json::from_value::<TransactionPatch>(json!({ "nativeAmount": "5.00" })).unwrap().native_amount.as_deref(), Some("5.00"));
}

#[test]
fn a_native_amount_is_a_positive_amount_to_the_largest_and_anything_else_is_refused_whichever_account() {
    let w = world();
    let c = currencies(&w);
    for ok in ["0.01", "1.00", "9999999999.99"] {
        let made = transactions::create(&w.conn, &spent(w.food, Some(c.lei), "1.00", Some(ok))).unwrap();
        assert_eq!(made.native_amount.as_deref(), Some(ok));
        assert_eq!(transactions::update(&w.conn, made.id, &native_patch("2.00")).unwrap().native_amount.as_deref(), Some("2.00"));
        assert_eq!(transactions::update(&w.conn, made.id, &native_patch(ok)).unwrap().native_amount.as_deref(), Some(ok));
    }
    let shop = transactions::create(&w.conn, &spent(w.food, Some(c.lei), "1.00", Some("5.00"))).unwrap().id;
    let plain = transactions::create(&w.conn, &spent(w.food, None, "1.00", None)).unwrap().id;
    for bad in ["0.00", "-1.00", "1.5", "1", "", " ", "10000000000.00", "1,00", "+1.00", "abc", "1.00 ", " 1.00", "-0.00", "0.001"] {
        assert_refused(&w.conn, INVALID, &format!("create with a native amount of {bad:?}"), || transactions::create(&w.conn, &spent(w.food, Some(c.lei), "1.00", Some(bad))));
        assert_refused(&w.conn, INVALID, &format!("create in the main account with a native amount of {bad:?}"), || transactions::create(&w.conn, &spent(w.food, None, "1.00", Some(bad))));
        assert_refused(&w.conn, INVALID, &format!("update with a native amount of {bad:?}"), || transactions::update(&w.conn, shop, &native_patch(bad)));
        assert_refused(&w.conn, INVALID, &format!("update of a row in the main account with {bad:?}"), || transactions::update(&w.conn, plain, &native_patch(bad)));
        assert_refused(&w.conn, INVALID, &format!("move into a foreign account with {bad:?}"), || {
            transactions::update(&w.conn, plain, &TransactionPatch { account_id: Some(c.lei.to_string()), native_amount: Some(bad.into()), ..Default::default() })
        });
    }
}

#[test]
fn a_native_amount_changes_in_place_and_is_kept_when_other_fields_change() {
    let w = world();
    let c = currencies(&w);
    let shop = transactions::create(&w.conn, &spent(w.food, Some(c.lei), "22.75", Some("113.74"))).unwrap();
    let made = stamp_of(&w.conn, "transactions", shop.id);
    let changed = transactions::update(&w.conn, shop.id, &native_patch("120.00")).unwrap();
    assert_eq!((changed.amount.as_str(), changed.native_amount.as_deref(), changed.account_id), ("22.75", Some("120.00"), c.lei));
    assert!(stamp_of(&w.conn, "transactions", shop.id) > made, "a native amount that changes is a change of the row");
    assert_eq!(balance(&w.conn, c.lei, TODAY), -12_000);
    // another field, or the amount in the main currency, leaves it alone
    let renamed = transactions::update(&w.conn, shop.id, &TransactionPatch { name: Some("Market".into()), amount: Some("30.00".into()), ..Default::default() }).unwrap();
    assert_eq!((renamed.name.as_str(), renamed.amount.as_str(), renamed.native_amount.as_deref()), ("Market", "30.00", Some("120.00")));
    assert_eq!(balance(&w.conn, c.lei, TODAY), -12_000, "the account is moved by the native amount, not by the amount");
    // both at once, and naming the account it has
    let both = transactions::update(&w.conn, shop.id, &TransactionPatch { amount: Some("40.00".into()), native_amount: Some("199.99".into()), account_id: Some(c.lei.to_string()), ..Default::default() }).unwrap();
    assert_eq!((both.amount.as_str(), both.native_amount.as_deref()), ("40.00", Some("199.99")));
    let named = transactions::update(&w.conn, shop.id, &TransactionPatch { description: Some(Some("noted".into())), account_id: Some(c.lei.to_string()), ..Default::default() }).unwrap();
    assert_eq!((named.description.as_deref(), named.native_amount.as_deref()), (Some("noted"), Some("199.99")), "naming the account it has is no move");
    // a bad field with a good native amount changes nothing
    assert_refused(&w.conn, INVALID, "a good native amount with a bad amount", || transactions::update(&w.conn, shop.id, &TransactionPatch { amount: Some("0.00".into()), native_amount: Some("5.00".into()), ..Default::default() }));
    assert_eq!(transactions::get(&w.conn, shop.id).unwrap().native_amount.as_deref(), Some("199.99"));
}

/// Every row of the table of section 21.3 for a transaction that moves.
#[test]
fn a_transaction_moved_between_accounts_follows_the_table_of_native_amounts() {
    let w = world();
    let c = currencies(&w);
    let move_to = |account: i64, native: Option<&str>| TransactionPatch { account_id: Some(account.to_string()), native_amount: native.map(str::to_string), ..Default::default() };
    let in_main = transactions::create(&w.conn, &spent(w.food, None, "20.00", None)).unwrap().id;

    // main -> foreign: the native amount is needed in the same patch
    assert_refused(&w.conn, INVALID, "main to foreign without a native amount", || transactions::update(&w.conn, in_main, &move_to(c.lei, None)));
    assert_eq!((account_of(&w.conn, "transactions", in_main), native_column(&w.conn, in_main)), (MAIN_ID, None));
    let moved = transactions::update(&w.conn, in_main, &move_to(c.lei, Some("100.00"))).unwrap();
    assert_eq!((moved.account_id, moved.native_amount.as_deref(), moved.amount.as_str()), (c.lei, Some("100.00"), "20.00"));
    assert_eq!([MAIN_ID, c.lei].map(|a| balance(&w.conn, a, TODAY)), [0, -10_000]);
    // foreign -> main: cleared, and one in that patch is refused
    assert_refused(&w.conn, INVALID, "foreign to main with a native amount", || transactions::update(&w.conn, in_main, &move_to(MAIN_ID, Some("100.00"))));
    assert_refused(&w.conn, INVALID, "foreign to an account in the main currency with one", || transactions::update(&w.conn, in_main, &move_to(c.euros, Some("100.00"))));
    assert_eq!((account_of(&w.conn, "transactions", in_main), native_column(&w.conn, in_main)), (c.lei, Some(10_000)), "refused: it is where it was, with what it had");
    let back = transactions::update(&w.conn, in_main, &move_to(MAIN_ID, None)).unwrap();
    assert_eq!((back.account_id, back.native_amount), (MAIN_ID, None));
    assert_eq!(native_column(&w.conn, in_main), None, "the stored one is cleared, not only hidden");
    assert_eq!([MAIN_ID, c.lei].map(|a| balance(&w.conn, a, TODAY)), [-2000, 0]);
    // foreign -> foreign: kept unless given, whatever their currencies
    transactions::update(&w.conn, in_main, &move_to(c.lei, Some("100.00"))).unwrap();
    for (what, to) in [("another account in the same currency", c.lei2), ("an account in another currency", c.usd), ("and back to the first", c.lei)] {
        let kept = transactions::update(&w.conn, in_main, &move_to(to, None)).unwrap();
        assert_eq!((kept.account_id, kept.native_amount.as_deref()), (to, Some("100.00")), "{what}: kept");
    }
    let given = transactions::update(&w.conn, in_main, &move_to(c.usd, Some("22.00"))).unwrap();
    assert_eq!((given.account_id, given.native_amount.as_deref()), (c.usd, Some("22.00")), "given: replaced");
    assert_eq!([c.lei, c.usd].map(|a| balance(&w.conn, a, TODAY)), [0, -2200]);
    // foreign -> an account in the main currency that is not the main account, and back
    let euro = transactions::update(&w.conn, in_main, &move_to(c.euros, None)).unwrap();
    assert_eq!((euro.account_id, euro.native_amount), (c.euros, None));
    assert_eq!(native_column(&w.conn, in_main), None);
    assert_eq!(balance(&w.conn, c.euros, TODAY), -2000);
    // main -> main: a native amount is refused, and a row that is and stays in the main currency takes none
    let other_main = transactions::update(&w.conn, in_main, &move_to(MAIN_ID, None)).unwrap();
    assert_eq!(other_main.account_id, MAIN_ID);
    assert_refused(&w.conn, INVALID, "a native amount for a row that stays in the main account", || transactions::update(&w.conn, in_main, &native_patch("5.00")));
    assert_refused(&w.conn, INVALID, "a native amount with a move between accounts in the main currency", || transactions::update(&w.conn, in_main, &move_to(c.euros, Some("5.00"))));
    assert_refused(&w.conn, INVALID, "a native amount, and the account it has named", || transactions::update(&w.conn, in_main, &TransactionPatch { account_id: Some("1".into()), native_amount: Some("5.00".into()), ..Default::default() }));
    assert_eq!(transactions::update(&w.conn, in_main, &move_to(c.euros, None)).unwrap().account_id, c.euros);
    // a rename of a row in a foreign account that names no native amount keeps it
    let lei_row = transactions::create(&w.conn, &spent(w.food, Some(c.lei), "10.00", Some("50.00"))).unwrap().id;
    assert_eq!(transactions::update(&w.conn, lei_row, &TransactionPatch { name: Some("Renamed".into()), ..Default::default() }).unwrap().native_amount.as_deref(), Some("50.00"));
    // a move into an account that is archived is refused as before, native amount or not
    let old = archived_account(&w.conn, "Old");
    assert_refused(&w.conn, INVALID, "into an archived account", || transactions::update(&w.conn, lei_row, &move_to(old, None)));
    let old_lei = foreign(&w.conn, "Old lei", "RON", "5.00");
    archive(&w.conn, old_lei, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "into an archived foreign account", || transactions::update(&w.conn, lei_row, &move_to(old_lei, Some("5.00"))));
}

#[test]
fn a_native_amount_left_on_a_row_in_the_main_currency_is_reported_as_none_counts_for_nothing_and_goes_on_the_next_write() {
    let w = world();
    let c = currencies(&w);
    // what a sync leaves behind when the account of a row was deleted on the other device: the row came back to the main account, native amount and all
    let row = transactions::create(&w.conn, &spent(w.food, Some(c.lei), "20.00", Some("100.00"))).unwrap().id;
    applying(&w.conn, true);
    w.conn.execute("UPDATE transactions SET account_id = 1 WHERE id = ?1", [row]).unwrap();
    applying(&w.conn, false);
    assert_eq!(native_column(&w.conn, row), Some(10_000), "it is still stored");
    let shown = transactions::get(&w.conn, row).unwrap();
    assert_eq!((shown.native_amount.clone(), shown.account_id, shown.amount.as_str()), (None, MAIN_ID, "20.00"));
    assert_eq!(serde_json::to_value(&shown).unwrap()["nativeAmount"], json!(null));
    assert_eq!(transactions::list(&w.conn, &Default::default()).unwrap().transactions[0].native_amount, None);
    assert_eq!(balance(&w.conn, MAIN_ID, TODAY), -2000, "the account moved by the amount");
    // one for the row is refused as for any row of the main account; any other write drops what is left
    assert_refused(&w.conn, INVALID, "a native amount for it", || transactions::update(&w.conn, row, &native_patch("100.00")));
    let renamed = transactions::update(&w.conn, row, &TransactionPatch { name: Some("Renamed".into()), ..Default::default() }).unwrap();
    assert_eq!(renamed.native_amount, None);
    assert_eq!(native_column(&w.conn, row), None, "and the next write cleared it");
    // moving it into a foreign account needs a native amount in the patch, as for any row from the main account
    applying(&w.conn, true);
    w.conn.execute("UPDATE transactions SET native_amount_minor = 777 WHERE id = ?1", [row]).unwrap();
    applying(&w.conn, false);
    assert_refused(&w.conn, INVALID, "a move to a foreign account with only the stale one", || transactions::update(&w.conn, row, &TransactionPatch { account_id: Some(c.lei.to_string()), ..Default::default() }));
    let moved = transactions::update(&w.conn, row, &TransactionPatch { account_id: Some(c.lei.to_string()), native_amount: Some("88.00".into()), ..Default::default() }).unwrap();
    assert_eq!(moved.native_amount.as_deref(), Some("88.00"));
    // a foreign row that has no native amount (written by hand) still moves the account by its amount, is reported with none, and a rename keeps it so
    let bare = transactions::create(&w.conn, &spent(w.food, Some(c.lei), "7.00", Some("35.00"))).unwrap().id;
    w.conn.execute("UPDATE transactions SET native_amount_minor = NULL WHERE id = ?1", [bare]).unwrap();
    assert_eq!(transactions::get(&w.conn, bare).unwrap().native_amount, None);
    assert_eq!(balance(&w.conn, c.lei, TODAY), -8800 - 700);
    assert_eq!(transactions::update(&w.conn, bare, &TransactionPatch { name: Some("Bare".into()), ..Default::default() }).unwrap().native_amount, None);
    // and a stored zero is a number: reported, kept
    w.conn.execute("UPDATE transactions SET native_amount_minor = 0 WHERE id = ?1", [bare]).unwrap();
    assert_eq!(transactions::get(&w.conn, bare).unwrap().native_amount.as_deref(), Some("0.00"));
    assert_eq!(transactions::update(&w.conn, bare, &TransactionPatch { name: Some("Zero".into()), ..Default::default() }).unwrap().native_amount.as_deref(), Some("0.00"));
    assert_eq!(balance(&w.conn, c.lei, TODAY), -8800);
}

#[test]
fn a_replayed_transaction_in_a_foreign_account_is_answered_as_it_is() {
    let w = world();
    let c = currencies(&w);
    let input = NewTransaction { client_uuid: Some("0b8f6c1e-5c52-4d6a-9d50-3f7a8a1b2c3d".into()), ..spent(w.food, Some(c.lei), "22.75", Some("113.74")) };
    let first = transactions::create(&w.conn, &input).unwrap();
    // a retry that leaves the native amount out, or names another account, is answered with the first row and is not refused
    for retry in [NewTransaction { native_amount: None, ..input.clone() }, NewTransaction { account_id: None, ..input.clone() }, NewTransaction { native_amount: Some("1.00".into()), ..input.clone() }] {
        let again = assert_nothing_written(&w.conn, "the same request again", || transactions::create(&w.conn, &retry));
        assert_eq!(again, first);
    }
}

#[test]
fn a_split_bill_in_a_foreign_account_moves_it_by_the_native_amount_when_you_paid_and_by_nothing_when_a_friend_did() {
    let w = world();
    let c = currencies(&w);
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    // 90.00 shared in three: the split works on the amount in the main currency
    let dinner = transactions::create(&w.conn, &spent(w.food, Some(c.lei), "90.00", Some("450.00"))).unwrap().id;
    set_split(&w.conn, dinner, &equal(vec![who(anna), who(bob)])).unwrap();
    let shown = transactions::get(&w.conn, dinner).unwrap();
    assert_eq!((shown.amount.as_str(), shown.my_share.as_str(), shown.native_amount.as_deref(), shown.split.len()), ("90.00", "30.00", Some("450.00"), 2));
    assert_eq!(balance(&w.conn, c.lei, TODAY), -45_000, "the whole bill, as the account paid it");
    // a bill that a friend paid moved nothing, whatever the account and its native amount
    let lunch = transactions::create(&w.conn, &spent(w.food, Some(c.lei), "40.00", Some("200.00"))).unwrap().id;
    assert_eq!(balance(&w.conn, c.lei, TODAY), -65_000, "until it is known that Bob paid");
    set_split(&w.conn, lunch, &SplitInput { paid_by: Some(bob.to_string()), ..equal(vec![who(bob)]) }).unwrap();
    assert_eq!(balance(&w.conn, c.lei, TODAY), -45_000);
    let shown = transactions::get(&w.conn, lunch).unwrap();
    assert_eq!((shown.native_amount.as_deref(), shown.account_id), (Some("200.00"), c.lei), "it still has its native amount and its account");
    // a new amount rescales the parts of the bill and leaves the native amount where it was
    let changed = transactions::update(&w.conn, dinner, &TransactionPatch { amount: Some("60.00".into()), ..Default::default() }).unwrap();
    assert_eq!((changed.amount.as_str(), changed.my_share.as_str(), changed.native_amount.as_deref()), ("60.00", "20.00", Some("450.00")));
    assert_eq!(balance(&w.conn, c.lei, TODAY), -45_000);
    // statistics count your parts of the amounts in the main currency, 20.00 of each bill, and never what the account paid
    let top = crate::stats::top_categories(&w.conn, "2026-11", CategoryType::Expense, 5).unwrap();
    assert_eq!(top[0].actual, "40.00");
}

#[test]
fn budgets_and_statistics_count_the_amount_in_the_main_currency_and_never_the_native_amount() {
    let w = world();
    let c = currencies(&w);
    let figures = |conn: &Connection| {
        let top = crate::stats::top_categories(conn, "2026-11", CategoryType::Expense, 5).unwrap();
        let overview = crate::budgets::overview(conn, "2026-11").unwrap();
        let vs = crate::stats::budget_vs_actual(conn, "2026-11").unwrap();
        let spent = vs.categories.iter().map(|l| (l.name.clone(), l.actual.clone())).collect::<Vec<_>>();
        (top.iter().map(|t| (t.name.clone(), t.actual.clone())).collect::<Vec<_>>(), overview.iter().map(|o| (o.actual.clone(), o.spent.len())).collect::<Vec<_>>(), spent)
    };
    transactions::create(&w.conn, &spent(w.food, None, "20.00", None)).unwrap();
    let plain = figures(&w.conn);
    assert_eq!(plain.0, [("Food".to_string(), "20.00".to_string())]);
    // a row of 20.00 in a foreign account that really cost 100.00 lei or 9999999999.99 of them: the same numbers, one row more
    for native in ["100.00", "9999999999.99", "0.01"] {
        let conn = &w.conn;
        let id = transactions::create(conn, &spent(w.rent, Some(c.lei), "20.00", Some(native))).unwrap().id;
        let with = figures(conn);
        assert_eq!(with.0, [("Food".to_string(), "20.00".to_string()), ("Rent".to_string(), "20.00".to_string())], "native {native}");
        assert_eq!(with.1[1].0, "40.00", "native {native}: the expense side adds 20.00 each, not what the account paid");
        transactions::delete(conn, id).unwrap();
        assert_eq!(figures(conn), plain);
    }
    // and an income row in a foreign account
    transactions::create(&w.conn, &spent(w.salary, Some(c.usd), "1000.00", Some("1100.00"))).unwrap();
    assert_eq!(figures(&w.conn).1[0].0, "1000.00", "the income side");
}

fn in_account(conn: &Connection, account: i64) -> i64 {
    balance(conn, account, TODAY)
}

#[test]
fn a_transfer_between_accounts_of_different_currencies_needs_the_amount_that_arrives_and_each_side_moves_by_its_own() {
    let w = world();
    let c = currencies(&w);
    let to_lei = NewTransfer { to_amount: Some("249.00".into()), ..new_transfer(MAIN_ID, c.lei, "50.00", "2026-11-03") };
    let made = transfers::create(&w.conn, &to_lei).unwrap();
    assert_eq!((made.amount.as_str(), made.to_amount.as_deref()), ("50.00", Some("249.00")));
    assert_eq!(serde_json::to_value(&made).unwrap()["toAmount"], json!("249.00"));
    assert_eq!(transfers::get(&w.conn, made.id).unwrap(), made);
    assert_eq!((in_account(&w.conn, MAIN_ID), in_account(&w.conn, c.lei)), (-5000, 24_900), "each side by its own amount");
    // lei to euro, and lei to dollars, need it too
    let back = transfers::create(&w.conn, &NewTransfer { to_amount: Some("5.90".into()), ..new_transfer(c.lei, MAIN_ID, "30.00", "2026-11-04") }).unwrap();
    assert_eq!(back.to_amount.as_deref(), Some("5.90"));
    assert_eq!((in_account(&w.conn, MAIN_ID), in_account(&w.conn, c.lei)), (-5000 + 590, 24_900 - 3000));
    let across = transfers::create(&w.conn, &NewTransfer { to_amount: Some("6.00".into()), ..new_transfer(c.lei, c.usd, "33.00", "2026-11-05") }).unwrap();
    assert_eq!((across.amount.as_str(), across.to_amount.as_deref()), ("33.00", Some("6.00")));
    assert_eq!((in_account(&w.conn, c.lei), in_account(&w.conn, c.usd)), (24_900 - 3000 - 3300, 600));
    for (what, from, to) in [("euro to lei", MAIN_ID, c.lei), ("lei to euro", c.lei, MAIN_ID), ("lei to dollars", c.lei, c.usd), ("dollars to euro", c.usd, c.euros), ("an account that names the main currency to lei", c.euros, c.lei)] {
        assert_refused(&w.conn, INVALID, &format!("{what}, with no amount that arrives"), || transfers::create(&w.conn, &new_transfer(from, to, "5.00", "2026-11-03")));
    }
    // the same currency on both sides takes none: the main account and one that names the main currency are the same currency, and so are two in lei
    for (what, from, to) in [("euro to euro", MAIN_ID, c.euros), ("euro to euro the other way", c.euros, MAIN_ID), ("lei to lei", c.lei, c.lei2), ("lei to lei the other way", c.lei2, c.lei)] {
        assert_refused(&w.conn, INVALID, &format!("{what}, with an amount that arrives"), || transfers::create(&w.conn, &NewTransfer { to_amount: Some("5.00".into()), ..new_transfer(from, to, "5.00", "2026-11-03") }));
        let plain = transfers::create(&w.conn, &new_transfer(from, to, "5.00", "2026-11-03")).unwrap();
        assert_eq!((plain.to_amount, arrives_column(&w.conn, plain.id)), (None, None), "{what}");
    }
    assert_eq!(in_account(&w.conn, c.euros), 0, "the two transfers between euro accounts cancel");
    assert_eq!(in_account(&w.conn, c.lei2), 0, "and so do the two between lei accounts");
    // a transfer is no income or expense: no statistic saw it
    assert_eq!(crate::stats::top_categories(&w.conn, "2026-11", crate::categories::CategoryType::Expense, 5).unwrap().len(), 0);
    // sums of all the accounts are not zero between currencies, and that is right
    let total: i64 = [MAIN_ID, c.lei, c.lei2, c.usd, c.euros].iter().map(|a| in_account(&w.conn, *a)).sum();
    assert_ne!(total, 0);
}

#[test]
fn the_amount_that_arrives_is_from_one_cent_to_the_largest_amount_and_bad_forms_are_refused() {
    let w = world();
    let c = currencies(&w);
    for ok in ["0.01", "1.00", "9999999999.99"] {
        let made = transfers::create(&w.conn, &NewTransfer { to_amount: Some(ok.into()), ..new_transfer(MAIN_ID, c.lei, "5.00", "2026-11-03") }).unwrap();
        assert_eq!(made.to_amount.as_deref(), Some(ok));
        assert_eq!(transfers::update(&w.conn, made.id, &TransferPatch { to_amount: Some("7.00".into()), ..Default::default() }).unwrap().to_amount.as_deref(), Some("7.00"));
        assert_eq!(transfers::update(&w.conn, made.id, &TransferPatch { to_amount: Some(ok.into()), ..Default::default() }).unwrap().to_amount.as_deref(), Some(ok));
    }
    let made = transfers::create(&w.conn, &NewTransfer { to_amount: Some("5.00".into()), ..new_transfer(MAIN_ID, c.lei, "5.00", "2026-11-03") }).unwrap().id;
    for bad in ["0.00", "-1.00", "1.5", "1", "", " ", "10000000000.00", "1,00", "+1.00", "abc", "1.00 ", "-0.00", "0.001"] {
        assert_refused(&w.conn, INVALID, &format!("create with {bad:?} arriving"), || transfers::create(&w.conn, &NewTransfer { to_amount: Some(bad.into()), ..new_transfer(MAIN_ID, c.lei, "5.00", "2026-11-03") }));
        assert_refused(&w.conn, INVALID, &format!("update with {bad:?} arriving"), || transfers::update(&w.conn, made, &TransferPatch { to_amount: Some(bad.into()), ..Default::default() }));
        assert_refused(&w.conn, INVALID, &format!("create between euro accounts with {bad:?} arriving"), || transfers::create(&w.conn, &NewTransfer { to_amount: Some(bad.into()), ..new_transfer(MAIN_ID, c.euros, "5.00", "2026-11-03") }));
    }
    assert_refused(&w.conn, INVALID, "a good amount that arrives with a bad amount", || transfers::create(&w.conn, &NewTransfer { to_amount: Some("5.00".into()), ..new_transfer(MAIN_ID, c.lei, "0.00", "2026-11-03") }));
    assert!(serde_json::from_value::<TransferPatch>(json!({ "toAmount": "5.00" })).is_ok());
    assert!(serde_json::from_value::<TransferPatch>(json!({ "to_amount": "5.00" })).is_err());
    assert!(serde_json::from_value::<NewTransfer>(json!({ "fromAccountId": "1", "toAccountId": "2", "amount": "1.00", "date": "2026-01-01" })).unwrap().to_amount.is_none());
}

#[test]
fn a_transfer_whose_side_changes_ends_with_the_second_amount_its_currencies_ask_for() {
    let w = world();
    let c = currencies(&w);
    // lei to euro: it arrives in euro
    let id = transfers::create(&w.conn, &NewTransfer { to_amount: Some("5.90".into()), ..new_transfer(c.lei, MAIN_ID, "30.00", "2026-11-03") }).unwrap().id;
    let to = |account: i64, arrives: Option<&str>| TransferPatch { to_account_id: Some(account.to_string()), to_amount: arrives.map(str::to_string), ..Default::default() };
    let from = |account: i64, arrives: Option<&str>| TransferPatch { from_account_id: Some(account.to_string()), to_amount: arrives.map(str::to_string), ..Default::default() };

    // a side changes so that both are in lei: the second amount is cleared, and one in the patch is refused
    assert_refused(&w.conn, INVALID, "both sides in lei, with a second amount", || transfers::update(&w.conn, id, &to(c.lei2, Some("5.90"))));
    assert_eq!(arrives_column(&w.conn, id), Some(590));
    let equal = transfers::update(&w.conn, id, &to(c.lei2, None)).unwrap();
    assert_eq!((equal.to_account_id, equal.to_amount), (c.lei2, None));
    assert_eq!(arrives_column(&w.conn, id), None, "cleared in the table, not only hidden");
    assert_eq!([c.lei, c.lei2].map(|a| in_account(&w.conn, a)), [-3000, 3000], "each side by the amount now");
    // and the other way, to a side with another currency: it is needed in the same patch
    assert_refused(&w.conn, INVALID, "the sides differ again, with no amount that arrives", || transfers::update(&w.conn, id, &to(MAIN_ID, None)));
    assert_refused(&w.conn, INVALID, "the sides differ again, to an account that names the main currency", || transfers::update(&w.conn, id, &to(c.euros, None)));
    assert_refused(&w.conn, INVALID, "the sides differ again, with a bad amount", || transfers::update(&w.conn, id, &to(MAIN_ID, Some("0.00"))));
    assert_eq!((account_of_side(&w.conn, id), arrives_column(&w.conn, id)), (c.lei2, None));
    let again = transfers::update(&w.conn, id, &to(c.euros, Some("5.90"))).unwrap();
    assert_eq!((again.to_account_id, again.to_amount.as_deref()), (c.euros, Some("5.90")));
    // from now on: kept unless given, and changed when given, also when the currencies still differ after the move
    let kept = transfers::update(&w.conn, id, &to(MAIN_ID, None)).unwrap();
    assert_eq!((kept.to_account_id, kept.to_amount.as_deref()), (MAIN_ID, Some("5.90")), "to the main account: the sides still differ, kept");
    let kept = transfers::update(&w.conn, id, &from(c.usd, None)).unwrap();
    assert_eq!((kept.from_account_id, kept.to_amount.as_deref()), (c.usd, Some("5.90")), "dollars to euro: kept");
    let given = transfers::update(&w.conn, id, &from(c.lei, Some("6.10"))).unwrap();
    assert_eq!((given.from_account_id, given.to_amount.as_deref()), (c.lei, Some("6.10")));
    // both sides at once: from lei to lei2, equal; from main to lei, different
    let (same_currency, differ) = (
        TransferPatch { from_account_id: Some(c.lei.to_string()), to_account_id: Some(c.lei2.to_string()), ..Default::default() },
        TransferPatch { from_account_id: Some(MAIN_ID.to_string()), to_account_id: Some(c.lei.to_string()), ..Default::default() },
    );
    assert_eq!(transfers::update(&w.conn, id, &same_currency).unwrap().to_amount, None);
    assert_refused(&w.conn, INVALID, "both sides at once, to different currencies with no amount", || transfers::update(&w.conn, id, &differ));
    assert_eq!(transfers::update(&w.conn, id, &TransferPatch { to_amount: Some("249.00".into()), ..differ }).unwrap().to_amount.as_deref(), Some("249.00"));
}

fn account_of_side(conn: &Connection, transfer: i64) -> i64 {
    one(conn, &format!("SELECT to_account_id FROM transfers WHERE id = {transfer}"))
}

#[test]
fn only_the_amount_that_arrives_can_change_and_a_transfer_that_changes_nothing_writes_nothing() {
    let w = world();
    let c = currencies(&w);
    let id = transfers::create(&w.conn, &NewTransfer { to_amount: Some("249.00".into()), ..new_transfer(MAIN_ID, c.lei, "50.00", "2026-11-03") }).unwrap().id;
    let made = stamp_of(&w.conn, "transfers", id);
    let only = |arrives: &str| TransferPatch { to_amount: Some(arrives.into()), ..Default::default() };
    // only the second amount
    let changed = transfers::update(&w.conn, id, &only("250.00")).unwrap();
    assert_eq!((changed.amount.as_str(), changed.to_amount.as_deref()), ("50.00", Some("250.00")));
    assert!(stamp_of(&w.conn, "transfers", id) > made);
    assert_eq!([MAIN_ID, c.lei].map(|a| in_account(&w.conn, a)), [-5000, 25_000]);
    // the amount that leaves, or the day, with no second amount: kept
    let kept = transfers::update(&w.conn, id, &TransferPatch { amount: Some("51.00".into()), date: Some("2026-11-04".into()), ..Default::default() }).unwrap();
    assert_eq!((kept.amount.as_str(), kept.to_amount.as_deref(), kept.date.as_str()), ("51.00", Some("250.00"), "2026-11-04"));
    assert_eq!([MAIN_ID, c.lei].map(|a| in_account(&w.conn, a)), [-5100, 25_000]);
    // nothing new: nothing written
    assert_eq!(assert_nothing_written(&w.conn, "an empty patch", || transfers::update(&w.conn, id, &TransferPatch::default())), kept);
    assert_eq!(assert_nothing_written(&w.conn, "the same second amount", || transfers::update(&w.conn, id, &only("250.00"))), kept);
    assert_eq!(assert_nothing_written(&w.conn, "everything as it is", || {
        transfers::update(&w.conn, id, &TransferPatch { from_account_id: Some("1".into()), to_account_id: Some(c.lei.to_string()), amount: Some("51.00".into()), date: Some("2026-11-04".into()), to_amount: Some("250.00".into()), note: None })
    }), kept);
    // a second amount for a transfer between the same currency is refused, whatever the amount
    let same = transfers::create(&w.conn, &new_transfer(c.lei, c.lei2, "10.00", "2026-11-03")).unwrap().id;
    assert_refused(&w.conn, INVALID, "a second amount for lei to lei", || transfers::update(&w.conn, same, &only("10.00")));
    let euro_to_euro = transfers::create(&w.conn, &new_transfer(MAIN_ID, c.euros, "1.00", "2026-11-03")).unwrap().id;
    assert_refused(&w.conn, INVALID, "a second amount for euro to euro", || transfers::update(&w.conn, euro_to_euro, &only("1.00")));
    assert_nothing_written(&w.conn, "a patch with only what lei to lei has", || transfers::update(&w.conn, same, &TransferPatch { amount: Some("10.00".into()), ..Default::default() }));
    // a transfer one of whose sides is archived: the second amount may change, a side may not move to an account that is
    archive(&w.conn, c.lei, TODAY).unwrap();
    assert_eq!(transfers::update(&w.conn, id, &only("251.00")).unwrap().to_amount.as_deref(), Some("251.00"), "the side does not change, so it may be archived");
    let old_lei = foreign(&w.conn, "Old lei", "RON", "5.00");
    archive(&w.conn, old_lei, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "a move to an archived account of the same currency", || transfers::update(&w.conn, id, &TransferPatch { to_account_id: Some(old_lei.to_string()), ..Default::default() }));
    // a transfer that is not there
    assert_refused(&w.conn, NOT_FOUND, "a second amount for a transfer that is not there", || transfers::update(&w.conn, 999, &only("1.00")));
    // the list and the filter answer the new field
    let page = transfers::list(&w.conn, &Filter { account_id: Some(c.lei.to_string()), ..Default::default() }).unwrap();
    assert_eq!(page.transfers.iter().map(|t| t.to_amount.clone()).collect::<Vec<_>>(), [Some("251.00".to_string()), None], "newest first");
}

#[test]
fn a_recurring_item_a_ledger_entry_a_repayment_and_a_bill_code_are_refused_for_a_foreign_account_and_nothing_changes() {
    let w = world();
    let c = currencies(&w);
    let anna = friend(&w.conn, "Anna");
    let lei = c.lei.to_string();
    let named = Some(lei.as_str());
    let owed = ledger::add(&w.conn, &new_entry(anna, ledger::Kind::Lent, None)).unwrap().id;
    let item = recurring::create(&w.conn, &new_item(w.rent, None)).unwrap().id;
    assert_refused(&w.conn, INVALID, "a recurring item in a foreign account", || recurring::create(&w.conn, &new_item(w.rent, named)));
    assert_refused(&w.conn, INVALID, "a recurring item moved to a foreign account", || recurring::update(&w.conn, item, &move_item(&lei)));
    assert_refused(&w.conn, INVALID, "a recurring item moved to a foreign account with other fields", || {
        recurring::update(&w.conn, item, &RecurringPatch { name: Some("Flat".into()), ..move_item(&lei) })
    });
    assert_refused(&w.conn, INVALID, "a ledger entry in a foreign account", || ledger::add(&w.conn, &new_entry(anna, ledger::Kind::Lent, named)));
    assert_refused(&w.conn, INVALID, "a ledger entry moved to a foreign account", || ledger::update(&w.conn, owed, &move_entry(&lei)));
    assert_refused(&w.conn, INVALID, "a repayment in a foreign account", || ledger::settle(&w.conn, anna, None, None, None, None, named, TODAY));
    assert_refused(&w.conn, INVALID, "a borrowing with its expense in a foreign account", || {
        ledger::borrow_with_expense(&w.conn, &new_entry(anna, ledger::Kind::Borrowed, named), &Expense { name: "Pizza".into(), category_id: w.food.to_string() })
    });
    assert_refused(&w.conn, INVALID, "a bill code in a foreign account", || codes::accept(&w.conn, &accepting(code(1), None, named)));
    assert_refused(&w.conn, INVALID, "a bill code with its expense in a foreign account", || codes::accept(&w.conn, &accepting(code(1), Some(w.food), named)));
    // the dollars too, and the same for an archived one that is foreign (refused for either reason)
    let usd = c.usd.to_string();
    assert_refused(&w.conn, INVALID, "a recurring item in dollars", || recurring::create(&w.conn, &new_item(w.rent, Some(&usd))));
    assert_eq!((account_of(&w.conn, "recurring_transactions", item), account_of(&w.conn, "ledger_entries", owed)), (MAIN_ID, MAIN_ID));
    assert_eq!(one(&w.conn, "SELECT count(*) FROM people"), 1, "the friend of a refused code was not made");
    // an account that names the main currency is one of them: all of it is allowed
    let euros = c.euros.to_string();
    let named = Some(euros.as_str());
    assert_eq!(recurring::create(&w.conn, &new_item(w.rent, named)).unwrap().account_id, c.euros);
    assert_eq!(recurring::update(&w.conn, item, &move_item(&euros)).unwrap().account_id, c.euros);
    assert_eq!(ledger::update(&w.conn, owed, &move_entry(&euros)).unwrap().account_id, c.euros);
    assert_eq!(ledger::add(&w.conn, &new_entry(anna, ledger::Kind::Lent, named)).unwrap().account_id, c.euros);
    assert_eq!(ledger::settle(&w.conn, anna, None, None, None, None, named, TODAY).unwrap().entry.account_id, c.euros);
    assert_eq!(ledger::borrow_with_expense(&w.conn, &new_entry(anna, ledger::Kind::Borrowed, named), &Expense { name: "Pizza".into(), category_id: w.food.to_string() }).unwrap().account_id, c.euros);
    assert_eq!(codes::accept(&w.conn, &accepting(code(1), Some(w.food), named)).unwrap().account_id, c.euros);
    // an item that moves out of the main account and back keeps being paid in the main one: paying an item whose account is archived is as before
    assert_eq!(recurring::update(&w.conn, item, &move_item("1")).unwrap().account_id, MAIN_ID);
}
