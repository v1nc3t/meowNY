//! A row and its account (`docs/accounts-design.md`, section 5.2) and the life of a transfer (section 5.3), row by row of the state
//! machines, for what happens on this device. Every refusal is checked to have changed nothing.

use rusqlite::Connection;
use serde_json::json;

use super::fixtures::*;
use super::transfers::{self, Filter, NewTransfer, Transfer, TransferPatch};
use super::{archive, get, restore, MAIN_ID};
use crate::debts::codes::{self, Accept};
use crate::debts::ledger::{self, EntryPatch, Expense, NewEntry};
use crate::recurring::{self, NewRecurring, RecurringPatch, Unit};
use crate::transactions::{self, NewTransaction, TransactionPatch};
use crate::vault::AppError;

const NOT_FOUND: AppError = AppError::NotFound;

fn new_tx(category: i64, account: Option<&str>) -> NewTransaction {
    NewTransaction { name: "Shop".into(), amount: "12.50".into(), payment_date: "2026-11-02".into(), category_id: category.to_string(), description: None, client_uuid: None, account_id: account.map(str::to_string) }
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
    assert_eq!(made, Transfer { id: 1, from_account_id: MAIN_ID, to_account_id: cash, amount: "200.00".into(), date: "2026-11-03".into(), note: Some("for the\nweekend".into()) });
    assert_eq!(
        serde_json::to_value(&made).unwrap(),
        json!({ "id": "1", "fromAccountId": "1", "toAccountId": cash.to_string(), "amount": "200.00", "date": "2026-11-03", "note": "for the\nweekend" })
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
    let patch = TransferPatch { from_account_id: Some(card.to_string()), to_account_id: Some(MAIN_ID.to_string()), amount: Some("150.00".into()), date: Some("2026-11-04".into()), note: Some(Some("card bill".into())) };
    let changed = transfers::update(&w.conn, id, &patch).unwrap();
    assert_eq!(changed, Transfer { id, from_account_id: card, to_account_id: MAIN_ID, amount: "150.00".into(), date: "2026-11-04".into(), note: Some("card bill".into()) });
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
