//! The life of an account, row by row of the state machine (`docs/accounts-design.md`, section 5.1), for what happens on this device:
//! made, changed, reordered, archived, restored, deleted. Every refusal is checked to have changed nothing.

use super::fixtures::*;
use super::transfers;
use super::{archive, create, delete, get, list, reorder, restore, update, Account, AccountPatch, Kind, NewAccount, MAIN_ID, MAIN_NAME};
use crate::debts::ledger::{self, NewEntry};
use crate::recurring::{self, NewRecurring, Unit};
use crate::transactions::{self, TransactionPatch};
use crate::vault::AppError;

const NOT_FOUND: AppError = AppError::NotFound;
const NAME_EXISTS: AppError = AppError::NameExists;

fn names(conn: &rusqlite::Connection) -> Vec<String> {
    list(conn, TODAY).unwrap().into_iter().map(|a| a.name).collect()
}

fn rename(name: &str) -> AccountPatch {
    AccountPatch { name: Some(name.into()), ..Default::default() }
}

fn recurring_in(conn: &rusqlite::Connection, category: i64, account: i64) -> i64 {
    recurring::create(
        conn,
        &NewRecurring {
            name: "Rent".into(),
            amount: Some("900.00".into()),
            category_id: category.to_string(),
            start_date: "2026-01-01".into(),
            interval_unit: Unit::Month,
            interval_count: 1,
            end_date: None,
            description: None,
            account_id: Some(account.to_string()),
        },
    )
    .unwrap()
    .id
}

fn entry_in(conn: &rusqlite::Connection, person: i64, account: i64) -> i64 {
    ledger::add(
        conn,
        &NewEntry { person_id: person.to_string(), kind: ledger::Kind::Lent, amount: "5.00".into(), date: "2026-01-02".into(), note: None, transaction_id: None, account_id: Some(account.to_string()) },
    )
    .unwrap()
    .id
}

// ---- NONE: create ----------------------------------------------------------------------------

#[test]
fn a_new_account_goes_to_the_end_of_the_list_with_what_was_given_and_the_defaults_for_the_rest() {
    let w = world();
    let plain = create(&w.conn, &new_account("  Savings  "), TODAY).unwrap();
    assert_eq!(
        plain,
        Account {
            id: 2,
            name: "Savings".into(),
            kind: Kind::Checking,
            number: None,
            opening_balance: "0.00".into(),
            opening_date: TODAY.into(),
            archived: false,
            is_main: false,
            balance: "0.00".into(),
            change_this_month: "0.00".into(),
            last_activity: None,
            entries: 0,
        }
    );
    let card = create(
        &w.conn,
        &NewAccount { name: "Card".into(), kind: Kind::CreditCard, number: Some(" 1234 5678 ".into()), opening_balance: Some("-120.00".into()), opening_date: Some("2026-01-01".into()) },
        TODAY,
    )
    .unwrap();
    assert_eq!((card.kind, card.number.as_deref(), card.opening_balance.as_str(), card.opening_date.as_str(), card.balance.as_str()), (Kind::CreditCard, Some("1234 5678"), "-120.00", "2026-01-01", "-120.00"));
    assert_eq!(card.change_this_month, "0.00", "the opening balance moved the account in January, not this month");
    assert_eq!(card.last_activity, None, "and it is not activity");
    assert_eq!(create(&w.conn, &NewAccount { number: Some("   ".into()), ..new_account("Cash") }, TODAY).unwrap().number, None, "an empty number is none");
    assert_eq!(names(&w.conn), [MAIN_NAME, "Savings", "Card", "Cash"]);
    assert_eq!(text(&w.conn, "SELECT group_concat(position) FROM (SELECT position FROM accounts ORDER BY id)"), "0,1,2,3");
    let main = get(&w.conn, MAIN_ID, TODAY).unwrap();
    assert!(main.is_main && !main.archived && main.name == MAIN_NAME);
}

#[test]
fn an_account_with_a_bad_name_kind_number_balance_or_date_is_not_made() {
    let w = world();
    let bad = [
        ("empty name", new_account("")),
        ("blank name", new_account("   ")),
        ("long name", new_account(&"x".repeat(51))),
        ("control character in the name", new_account("Sav\ning")),
        ("long number", NewAccount { number: Some("1".repeat(51)), ..new_account("A") }),
        ("control character in the number", NewAccount { number: Some("12\t34".into()), ..new_account("A") }),
        ("date that is not real", NewAccount { opening_date: Some("2026-02-30".into()), ..new_account("A") }),
        ("date not padded", NewAccount { opening_date: Some("2026-2-3".into()), ..new_account("A") }),
        ("balance without decimals", NewAccount { opening_balance: Some("12".into()), ..new_account("A") }),
    ];
    for (what, input) in bad {
        assert_refused(&w.conn, INVALID, what, || create(&w.conn, &input, TODAY));
    }
    assert_refused(&w.conn, INVALID, "a today that is not a date", || create(&w.conn, &new_account("A"), "today"));
    assert_eq!(names(&w.conn), [MAIN_NAME]);
    assert!(create(&w.conn, &new_account(&"x".repeat(50)), TODAY).is_ok());
    // the kind is one of four: anything else does not even reach the code
    for kind in ["\"CHECKING\"", "\"SAVINGS\"", "\"CASH\"", "\"CREDIT_CARD\""] {
        assert!(serde_json::from_str::<Kind>(kind).is_ok(), "{kind}");
    }
    for kind in ["\"checking\"", "\"LOAN\"", "\"CreditCard\"", "\"\"", "null", "1"] {
        assert!(serde_json::from_str::<Kind>(kind).is_err(), "{kind}");
    }
    assert!(serde_json::from_str::<NewAccount>(r#"{"name":"A","kind":"CASH","color":"red"}"#).is_err(), "an unknown field is refused");
    assert!(serde_json::from_str::<NewAccount>(r#"{"name":"A"}"#).is_err(), "the kind is needed");
}

/// 5.5 item 16.
#[test]
fn the_opening_balance_may_be_anything_from_minus_to_plus_the_largest_amount_written_one_way() {
    let w = world();
    for (n, ok) in ["-9999999999.99", "9999999999.99", "0.00", "-0.01", "0.01"].iter().enumerate() {
        let made = create(&w.conn, &NewAccount { opening_balance: Some(ok.to_string()), ..new_account(&format!("Account {n}")) }, TODAY).unwrap();
        assert_eq!((made.opening_balance.as_str(), made.balance.as_str()), (*ok, *ok));
    }
    let first = 2;
    for bad in ["-10000000000.00", "10000000000.00", "-0.00", "1.5", "+1.00", "", "1,00", "--1.00", "1e3", " 1.00"] {
        assert_refused(&w.conn, INVALID, bad, || create(&w.conn, &NewAccount { opening_balance: Some(bad.to_string()), ..new_account("Other") }, TODAY));
        assert_refused(&w.conn, INVALID, bad, || update(&w.conn, first, &AccountPatch { opening_balance: Some(bad.to_string()), ..Default::default() }, TODAY));
    }
    assert_eq!(get(&w.conn, first, TODAY).unwrap().opening_balance, "-9999999999.99");
}

/// 5.5 item 12, the part on this device.
#[test]
fn a_name_that_an_active_account_has_is_refused_ignoring_case_and_an_archived_one_frees_it() {
    let w = world();
    let savings = account(&w.conn, "Savings");
    for taken in ["Savings", "savings", "  SAVINGS ", MAIN_NAME, "main ACCOUNT"] {
        assert_refused(&w.conn, NAME_EXISTS, taken, || create(&w.conn, &new_account(taken), TODAY));
    }
    let cash = account(&w.conn, "Cash");
    assert_refused(&w.conn, NAME_EXISTS, "rename to a taken name", || update(&w.conn, cash, &rename("savings"), TODAY));
    assert_refused(&w.conn, NAME_EXISTS, "rename to the main account's name", || update(&w.conn, cash, &rename(MAIN_NAME), TODAY));
    assert_refused(&w.conn, NAME_EXISTS, "rename the main account to a taken name", || update(&w.conn, MAIN_ID, &rename("Cash"), TODAY));
    // its own name, in another case, is not taken
    assert_eq!(update(&w.conn, cash, &rename("CASH"), TODAY).unwrap().name, "CASH");

    archive(&w.conn, savings, TODAY).unwrap();
    let second = create(&w.conn, &new_account("Savings"), TODAY).unwrap();
    assert_refused(&w.conn, NAME_EXISTS, "restore while another has the name", || restore(&w.conn, savings, TODAY));
    assert!(get(&w.conn, savings, TODAY).unwrap().archived);
    // once the name is free again, it comes back
    update(&w.conn, second.id, &rename("Rainy day"), TODAY).unwrap();
    assert!(!restore(&w.conn, savings, TODAY).unwrap().archived);
    // two archived accounts may share a name, and only one of them can come back
    archive(&w.conn, savings, TODAY).unwrap();
    update(&w.conn, second.id, &rename("Savings"), TODAY).unwrap();
    archive(&w.conn, second.id, TODAY).unwrap();
    restore(&w.conn, second.id, TODAY).unwrap();
    assert_refused(&w.conn, NAME_EXISTS, "the second of two archived accounts with one name", || restore(&w.conn, savings, TODAY));
}

// ---- ACTIVE: update --------------------------------------------------------------------------

#[test]
fn an_update_writes_the_fields_given_and_makes_a_stamp_only_when_something_changed() {
    let w = world();
    let id = account(&w.conn, "Savings");
    let made = stamp_of(&w.conn, "accounts", id);
    let patch = AccountPatch {
        name: Some(" Rainy day ".into()),
        kind: Some(Kind::Savings),
        number: Some(Some("IE29 AIBK".into())),
        opening_balance: Some("1500.00".into()),
        opening_date: Some("2026-01-01".into()),
    };
    let changed = update(&w.conn, id, &patch, TODAY).unwrap();
    assert_eq!(
        (changed.name.as_str(), changed.kind, changed.number.as_deref(), changed.opening_balance.as_str(), changed.opening_date.as_str(), changed.balance.as_str()),
        ("Rainy day", Kind::Savings, Some("IE29 AIBK"), "1500.00", "2026-01-01", "1500.00")
    );
    let stamp = stamp_of(&w.conn, "accounts", id);
    assert!(stamp > made);

    // the same values again, and an empty patch: allowed, and nothing is written
    assert_eq!(assert_nothing_written(&w.conn, "the same values", || update(&w.conn, id, &patch, TODAY)), changed);
    assert_eq!(assert_nothing_written(&w.conn, "an empty patch", || update(&w.conn, id, &AccountPatch::default(), TODAY)), changed);

    // one field at a time; `number: null` clears it, a field left out stays
    let cleared = update(&w.conn, id, &serde_json::from_str::<AccountPatch>(r#"{"number":null}"#).unwrap(), TODAY).unwrap();
    assert_eq!((cleared.number, cleared.name.as_str(), cleared.opening_balance.as_str()), (None, "Rainy day", "1500.00"));
    assert!(stamp_of(&w.conn, "accounts", id) > stamp);
    assert!(serde_json::from_str::<AccountPatch>(r#"{"archived":true}"#).is_err(), "archiving is not an update");
    assert!(serde_json::from_str::<AccountPatch>(r#"{"position":3}"#).is_err());

    // the main account can be changed like any other, and gets its first real stamp
    let main = update(&w.conn, MAIN_ID, &AccountPatch { name: Some("Current account".into()), opening_balance: Some("250.00".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!((main.name.as_str(), main.balance.as_str(), main.is_main), ("Current account", "250.00", true));
    assert!(stamp_of(&w.conn, "accounts", MAIN_ID).as_str() > crate::sync::engine::UNSET_STAMP);
}

#[test]
fn an_update_with_a_bad_field_changes_nothing() {
    let w = world();
    let id = account(&w.conn, "Savings");
    let bad = [
        ("blank name", rename(" ")),
        ("long name", rename(&"x".repeat(51))),
        ("control character", rename("a\u{7}b")),
        ("long number", AccountPatch { number: Some(Some("1".repeat(51))), ..Default::default() }),
        ("bad date", AccountPatch { opening_date: Some("31/01/2026".into()), ..Default::default() }),
        ("bad balance", AccountPatch { opening_balance: Some("12.5".into()), ..Default::default() }),
        // one bad field refuses the good ones with it
        ("a good name with a bad date", AccountPatch { name: Some("Fine".into()), opening_date: Some("2026-13-01".into()), ..Default::default() }),
    ];
    for (what, patch) in bad {
        assert_refused(&w.conn, INVALID, what, || update(&w.conn, id, &patch, TODAY));
    }
    assert_eq!(get(&w.conn, id, TODAY).unwrap().name, "Savings");
}

// ---- ACTIVE: reorder -------------------------------------------------------------------------

#[test]
fn a_reorder_sets_the_order_and_writes_only_the_rows_that_move() {
    let w = world();
    let (a, b, c) = (account(&w.conn, "A"), account(&w.conn, "B"), account(&w.conn, "C"));
    let old = archived_account(&w.conn, "Old");
    let stamps = |ids: &[i64]| -> Vec<String> { ids.iter().map(|id| stamp_of(&w.conn, "accounts", *id)).collect() };
    let before = stamps(&[MAIN_ID, a, b, c, old]);

    reorder(&w.conn, &[MAIN_ID, a, c, b]).unwrap();
    assert_eq!(names(&w.conn), [MAIN_NAME, "A", "C", "B", "Old"], "the archived one stays after the others");
    let after = stamps(&[MAIN_ID, a, b, c, old]);
    assert_eq!((&after[0], &after[1], &after[4]), (&before[0], &before[1], &before[4]), "the main account, A and the archived one did not move");
    assert!(after[2] > before[2] && after[3] > before[3], "B and C did");

    assert_nothing_written(&w.conn, "the order it has", || reorder(&w.conn, &[MAIN_ID, a, c, b]));
    // the main account can be anywhere in the list
    reorder(&w.conn, &[c, b, MAIN_ID, a]).unwrap();
    assert_eq!(names(&w.conn), ["C", "B", MAIN_NAME, "A", "Old"]);
    // a new account still goes to the end
    account(&w.conn, "D");
    assert_eq!(names(&w.conn), ["C", "B", MAIN_NAME, "A", "D", "Old"]);
}

/// 5.5 item 20.
#[test]
fn a_reorder_that_is_not_exactly_the_active_accounts_each_once_changes_nothing() {
    let w = world();
    let (a, b) = (account(&w.conn, "A"), account(&w.conn, "B"));
    let old = archived_account(&w.conn, "Old");
    let food = w.food; // the id of a category: an id of another list
    assert!(food != a && food != b);
    let bad: [(&str, Vec<i64>); 9] = [
        ("a missing id", vec![MAIN_ID, a]),
        ("without the main account", vec![a, b]),
        ("a repeated id", vec![MAIN_ID, a, b, a]),
        ("a repeated id in place of another", vec![MAIN_ID, a, a]),
        ("an archived id", vec![MAIN_ID, a, b, old]),
        ("an archived id in place of an active one", vec![MAIN_ID, a, old]),
        ("an id that is no account", vec![MAIN_ID, a, b, 999]),
        ("an id that is no account in place of one", vec![MAIN_ID, a, 999]),
        ("nothing", vec![]),
    ];
    for (what, ids) in bad {
        assert_refused(&w.conn, INVALID, what, || reorder(&w.conn, &ids));
    }
    assert_eq!(names(&w.conn), [MAIN_NAME, "A", "B", "Old"]);
}

// ---- archive and restore ---------------------------------------------------------------------

/// 5.5 items 1 and 2.
#[test]
fn archiving_twice_and_restoring_what_was_never_archived_answer_the_account_and_write_nothing() {
    let w = world();
    let id = account(&w.conn, "Savings");
    let untouched = assert_nothing_written(&w.conn, "restore an active account", || restore(&w.conn, id, TODAY));
    assert!(!untouched.archived);
    assert_nothing_written(&w.conn, "restore the main account", || restore(&w.conn, MAIN_ID, TODAY));

    let made = stamp_of(&w.conn, "accounts", id);
    let archived = archive(&w.conn, id, TODAY).unwrap();
    assert!(archived.archived);
    let stamp = stamp_of(&w.conn, "accounts", id);
    assert!(stamp > made, "archiving is a change of the account");
    let again = assert_nothing_written(&w.conn, "archive again", || archive(&w.conn, id, TODAY));
    assert_eq!(again, archived);

    let back = restore(&w.conn, id, TODAY).unwrap();
    assert!(!back.archived);
    assert!(stamp_of(&w.conn, "accounts", id) > stamp);
    assert_nothing_written(&w.conn, "restore again", || restore(&w.conn, id, TODAY));
    assert_eq!(names(&w.conn), [MAIN_NAME, "Savings"]);
}

#[test]
fn archiving_changes_no_number_and_writes_none_of_the_rows_of_the_account() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let cash = create(&w.conn, &NewAccount { opening_balance: Some("100.00".into()), opening_date: Some("2026-01-01".into()), ..new_account("Cash") }, TODAY).unwrap().id;
    let shop = transactions::create(&w.conn, &crate::transactions::NewTransaction { account_id: Some(cash.to_string()), ..tx(w.food, "Shop", "12.50", "2026-11-02") }).unwrap().id;
    let rent = recurring_in(&w.conn, w.rent, cash);
    let loan = entry_in(&w.conn, anna, cash);
    let moved = transfer(&w.conn, cash, MAIN_ID, "20.00", "2026-11-03");
    let rows = [("transactions", shop), ("recurring_transactions", rent), ("ledger_entries", loan), ("transfers", moved)];
    let stamps = || rows.map(|(table, id)| (stamp_of(&w.conn, table, id), text(&w.conn, &format!("SELECT updated_at FROM {table} WHERE id = {id}"))));
    let before = (stamps(), get(&w.conn, cash, TODAY).unwrap(), super::summary(&w.conn, TODAY).unwrap());
    assert_eq!((before.1.balance.as_str(), before.1.entries, before.1.last_activity.as_deref()), ("62.50", 4, Some("2026-11-03")));

    let archived = archive(&w.conn, cash, TODAY).unwrap();
    assert_eq!(stamps(), before.0, "no row of the account was written");
    assert_eq!(Account { archived: false, ..archived.clone() }, before.1, "its numbers are the same");
    assert_eq!(super::summary(&w.conn, TODAY).unwrap(), before.2, "and net worth still counts it");
    assert_eq!(names(&w.conn), [MAIN_NAME, "Cash"]);
    assert_eq!(list(&w.conn, TODAY).unwrap()[1], archived);

    restore(&w.conn, cash, TODAY).unwrap();
    assert_eq!(stamps(), before.0, "restoring writes none of them either");
    assert_eq!(get(&w.conn, cash, TODAY).unwrap(), before.1);
}

fn tx(category: i64, name: &str, amount: &str, date: &str) -> crate::transactions::NewTransaction {
    crate::transactions::NewTransaction { name: name.into(), amount: amount.into(), payment_date: date.into(), category_id: category.to_string(), description: None, client_uuid: None, account_id: None }
}

/// 5.5 items 3 (the part on this device) and 4.
#[test]
fn the_main_account_is_never_archived_or_deleted_so_one_active_account_always_stays() {
    let w = world();
    assert_refused(&w.conn, INVALID, "archive the main account", || archive(&w.conn, MAIN_ID, TODAY));
    assert_refused(&w.conn, INVALID, "delete the main account", || delete(&w.conn, MAIN_ID));
    let other = account(&w.conn, "Savings");
    // with a second account it is still the main one that stays
    assert_refused(&w.conn, INVALID, "archive the main account when there is another", || archive(&w.conn, MAIN_ID, TODAY));
    assert_refused(&w.conn, INVALID, "delete the main account when there is another", || delete(&w.conn, MAIN_ID));
    // the last active account besides the main one goes, then the main one is asked: refused
    archive(&w.conn, other, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "archive the last active account", || archive(&w.conn, MAIN_ID, TODAY));
    assert_refused(&w.conn, INVALID, "delete the last active account", || delete(&w.conn, MAIN_ID));
    assert_eq!(one(&w.conn, "SELECT count(*) FROM accounts WHERE archived = 0"), 1);
    assert!(super::check(&w.conn).unwrap());
    // renamed, it is still the main account
    update(&w.conn, MAIN_ID, &rename("Everyday"), TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "archive the renamed main account", || archive(&w.conn, MAIN_ID, TODAY));
    assert_refused(&w.conn, INVALID, "delete the renamed main account", || delete(&w.conn, MAIN_ID));
    assert!(super::check(&w.conn).unwrap());
}

#[test]
fn an_archived_account_is_not_updated_or_reordered_until_it_is_restored() {
    let w = world();
    let (a, old) = (account(&w.conn, "A"), archived_account(&w.conn, "Old"));
    assert_refused(&w.conn, INVALID, "rename an archived account", || update(&w.conn, old, &rename("New"), TODAY));
    assert_refused(&w.conn, INVALID, "an empty patch on an archived account", || update(&w.conn, old, &AccountPatch::default(), TODAY));
    assert_refused(&w.conn, INVALID, "change its opening balance", || update(&w.conn, old, &AccountPatch { opening_balance: Some("5.00".into()), ..Default::default() }, TODAY));
    assert_refused(&w.conn, INVALID, "reorder with it", || reorder(&w.conn, &[old, MAIN_ID, a]));
    restore(&w.conn, old, TODAY).unwrap();
    assert_eq!(update(&w.conn, old, &rename("New"), TODAY).unwrap().name, "New");
    reorder(&w.conn, &[old, MAIN_ID, a]).unwrap();
    assert_eq!(names(&w.conn), ["New", MAIN_NAME, "A"]);
}

// ---- delete ----------------------------------------------------------------------------------

#[test]
fn an_empty_account_is_deleted_active_or_archived_and_leaves_a_marker() {
    let w = world();
    for archived in [false, true] {
        let id = account(&w.conn, "Savings");
        if archived {
            archive(&w.conn, id, TODAY).unwrap();
        }
        let (sync_id, stamp) = (text(&w.conn, &format!("SELECT sync_id FROM accounts WHERE id = {id}")), stamp_of(&w.conn, "accounts", id));
        delete(&w.conn, id).unwrap();
        assert_eq!(get(&w.conn, id, TODAY), Err(NOT_FOUND));
        let marker = text(&w.conn, &format!("SELECT stamp FROM sync_deleted WHERE sync_id = '{sync_id}' AND kind = 'accounts'"));
        assert!(marker > stamp, "archived: {archived}");
    }
    assert_eq!(names(&w.conn), [MAIN_NAME]);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 2);
    // the name is free again
    account(&w.conn, "Savings");
}

/// 5.5 item 5.
#[test]
fn an_account_with_one_row_of_any_kind_is_not_deleted_until_the_row_is_moved_away_or_gone() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let other = account(&w.conn, "Other");

    // one transaction
    let a = account(&w.conn, "With a transaction");
    let shop = transactions::create(&w.conn, &crate::transactions::NewTransaction { account_id: Some(a.to_string()), ..tx(w.food, "Shop", "12.50", "2026-11-02") }).unwrap().id;
    assert_eq!(get(&w.conn, a, TODAY).unwrap().entries, 1);
    assert_refused(&w.conn, INVALID, "delete with one transaction", || delete(&w.conn, a));
    transactions::update(&w.conn, shop, &TransactionPatch { account_id: Some(other.to_string()), ..Default::default() }).unwrap();
    delete(&w.conn, a).unwrap();

    // only a recurring item
    let b = account(&w.conn, "With a recurring item");
    let rent = recurring_in(&w.conn, w.rent, b);
    assert_refused(&w.conn, INVALID, "delete with only a recurring item", || delete(&w.conn, b));
    recurring::update(&w.conn, rent, &recurring::RecurringPatch { account_id: Some(MAIN_ID.to_string()), ..Default::default() }).unwrap();
    delete(&w.conn, b).unwrap();

    // only a ledger entry
    let c = account(&w.conn, "With a ledger entry");
    let loan = entry_in(&w.conn, anna, c);
    assert_refused(&w.conn, INVALID, "delete with only a ledger entry", || delete(&w.conn, c));
    ledger::delete(&w.conn, loan).unwrap();
    delete(&w.conn, c).unwrap();

    // only a transfer, on its `to` side, and then on its `from` side
    let d = account(&w.conn, "With a transfer in");
    let moved = transfer(&w.conn, other, d, "5.00", "2026-11-03");
    assert_eq!(get(&w.conn, d, TODAY).unwrap().entries, 1);
    assert_refused(&w.conn, INVALID, "delete with only a transfer to it", || delete(&w.conn, d));
    transfers::update(&w.conn, moved, &transfers::TransferPatch { from_account_id: Some(d.to_string()), to_account_id: Some(other.to_string()), ..Default::default() }).unwrap();
    assert_refused(&w.conn, INVALID, "delete with only a transfer from it", || delete(&w.conn, d));
    // archived, with rows: still refused
    archive(&w.conn, d, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "delete an archived account with rows", || delete(&w.conn, d));
    transfers::update(&w.conn, moved, &transfers::TransferPatch { from_account_id: Some(MAIN_ID.to_string()), ..Default::default() }).unwrap();
    assert_eq!(get(&w.conn, d, TODAY).unwrap().entries, 0);
    delete(&w.conn, d).unwrap();

    assert_eq!(names(&w.conn), [MAIN_NAME, "Other"]);
    // the account the rows went to holds them, and is not deleted either
    assert_eq!(get(&w.conn, other, TODAY).unwrap().entries, 2);
    assert_refused(&w.conn, INVALID, "delete the account they moved to", || delete(&w.conn, other));
}

// ---- NONE, DELETED: any event by id -----------------------------------------------------------

#[test]
fn an_account_that_is_not_there_or_was_deleted_is_not_found_by_any_event() {
    let w = world();
    let gone = account(&w.conn, "Gone");
    account(&w.conn, "Stays");
    delete(&w.conn, gone).unwrap();
    for id in [gone, 999] {
        assert_refused(&w.conn, NOT_FOUND, "get", || get(&w.conn, id, TODAY));
        assert_refused(&w.conn, NOT_FOUND, "update", || update(&w.conn, id, &rename("X"), TODAY));
        assert_refused(&w.conn, NOT_FOUND, "archive", || archive(&w.conn, id, TODAY));
        assert_refused(&w.conn, NOT_FOUND, "restore", || restore(&w.conn, id, TODAY));
        assert_refused(&w.conn, NOT_FOUND, "delete", || delete(&w.conn, id));
        assert_refused(&w.conn, NOT_FOUND, "statement", || super::statement(&w.conn, id, "2026-11-01", "2026-11-30", TODAY));
        assert_refused(&w.conn, NOT_FOUND, "history", || super::history(&w.conn, id, "2026", TODAY));
        assert_refused(&w.conn, INVALID, "reorder", || reorder(&w.conn, &[MAIN_ID, 3, id]));
    }
    assert_eq!(names(&w.conn), [MAIN_NAME, "Stays"]);
}

#[test]
fn the_check_of_the_main_account_fails_when_it_is_missing_or_is_another_row() {
    let w = world();
    assert!(super::check(&w.conn).unwrap());
    account(&w.conn, "Savings");
    assert!(super::check(&w.conn).unwrap());
    // what a damaged backup or a hostile sync could leave behind
    applying(&w.conn, true);
    w.conn.execute("UPDATE accounts SET sync_id = ?1 WHERE id = 1", ["a".repeat(32)]).unwrap();
    assert!(!super::check(&w.conn).unwrap(), "id 1 with another sync id is not the main account");
    w.conn.execute("DELETE FROM accounts WHERE id = 1", []).unwrap();
    assert!(!super::check(&w.conn).unwrap(), "no row with id 1");
    w.conn.execute("UPDATE accounts SET sync_id = ?1 WHERE id = 2", [super::MAIN_SYNC_ID]).unwrap();
    assert!(!super::check(&w.conn).unwrap(), "the main sync id on another id");
}
