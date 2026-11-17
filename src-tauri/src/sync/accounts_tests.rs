//! Accounts and transfers between two devices: what `docs/accounts-design.md` (section 7, and the odd transitions of 5.5) asks of the
//! sync, on two in-memory databases. After every two-device scenario the two are equal, a second sync finds nothing to send, and the
//! main account is as it must be on both (`settle`).

use rusqlite::Connection;

use super::engine::{self, snapshot, Incoming, RowsCollector, SyncError, UNSET_STAMP};
use super::engine_tests::{assert_same, category, count, device, id_of, new_phone, now, pause, paired, rows_to, spend, sync_ok, text, Rng, TODAY};
use super::peers;
use super::run::{sync, sync_cut_after, Outcome};
use super::stamp::Stamp;
use super::wire::{self, Marker, Message, Row, Table};
use crate::accounts::transfers::{self, NewTransfer, TransferPatch};
use crate::accounts::{self, AccountPatch, Kind, NewAccount, MAIN_ID, MAIN_SYNC_ID};
use crate::categories::CategoryType;
use crate::currency;
use crate::debts::{ledger, people};
use crate::recurring::{self, NewRecurring, Unit};
use crate::transactions::{self, NewTransaction, TransactionPatch};

// --- a world to work in ------------------------------------------------------------------------------------------------

/// `sync_ok`, and on both devices the main account is as it must be and every reference holds.
fn settle(a: &mut Connection, b: &mut Connection) -> Outcome {
    let outcome = sync_ok(a, b);
    for conn in [&*a, &*b] {
        assert!(accounts::check(conn).unwrap(), "the main account is not as it must be");
        assert_eq!(count(conn, "SELECT count(*) FROM pragma_foreign_key_check"), 0);
        assert_eq!(count(conn, "SELECT applying FROM sync_clock"), 0);
        assert_eq!(count(conn, "SELECT count(*) FROM transfers WHERE from_account_id = to_account_id"), 0);
    }
    outcome
}

/// An account with nothing in it and an opening date of its own. Returns its local id.
fn open(conn: &Connection, name: &str) -> i64 {
    accounts::create(conn, &NewAccount { name: name.into(), kind: Kind::Checking, number: None, opening_balance: None, opening_date: Some("2026-01-01".into()), bic: None, notes: None, group: None, include_in_net_worth: None, virtual_balance: None, currency: None, rate: None }, TODAY).unwrap().id
}

fn rename(conn: &Connection, id: i64, name: &str) {
    accounts::update(conn, id, &AccountPatch { name: Some(name.into()), ..Default::default() }, TODAY).unwrap();
}

fn account_id(conn: &Connection, name: &str) -> i64 {
    id_of(conn, "accounts", name)
}

fn transfer_named(conn: &Connection, note: &str) -> i64 {
    conn.query_row("SELECT id FROM transfers WHERE note = ?1", [note], |r| r.get(0)).unwrap()
}

fn account_sync_id(conn: &Connection, name: &str) -> String {
    text(conn, &format!("SELECT sync_id FROM accounts WHERE name = '{name}'"))
}

/// A payment, a recurring item, an entry of the ledger and a transfer, each named so it can be found again.
fn spend_in(conn: &Connection, name: &str, account: i64) -> i64 {
    let food = id_of(conn, "categories", "Food");
    transactions::create(
        conn,
        &NewTransaction { name: name.into(), amount: "5.00".into(), payment_date: "2026-03-02".into(), category_id: food.to_string(), description: None, client_uuid: None, account_id: Some(account.to_string()), native_amount: None },
    )
    .unwrap()
    .id
}

fn bill_in(conn: &Connection, name: &str, account: i64) -> i64 {
    let food = id_of(conn, "categories", "Food");
    recurring::create(
        conn,
        &NewRecurring { name: name.into(), amount: Some("10.00".into()), category_id: food.to_string(), start_date: "2026-03-01".into(), interval_unit: Unit::Month, interval_count: 1, end_date: None, description: None, account_id: Some(account.to_string()) },
    )
    .unwrap()
    .id
}

/// An entry of the ledger with the friend Anna; `note` is its name.
fn lend_in(conn: &Connection, note: &str, account: i64) -> i64 {
    let anna = id_of(conn, "people", "Anna");
    ledger::add(
        conn,
        &ledger::NewEntry { person_id: anna.to_string(), kind: ledger::Kind::Lent, amount: "7.00".into(), date: "2026-03-05".into(), note: Some(note.into()), transaction_id: None, account_id: Some(account.to_string()) },
    )
    .unwrap()
    .id
}

/// A transfer; `note` is its name.
fn move_money(conn: &Connection, from: i64, to: i64, note: &str) -> i64 {
    transfers::create(conn, &NewTransfer { from_account_id: from.to_string(), to_account_id: to.to_string(), amount: "10.00".into(), date: "2026-03-03".into(), note: Some(note.into()), to_amount: None }).unwrap().id
}

/// A desktop and a phone that have met, with the category Food, the friend Anna and the accounts Cash and Savings on both, and a moment
/// after it. (The accounts have other local ids on the phone: look them up by name.)
fn met_with_accounts() -> (Connection, Connection) {
    let mut desktop = device("EUR");
    let mut phone = new_phone(&desktop);
    category(&desktop, "Food", CategoryType::Expense);
    people::create(&desktop, "Anna").unwrap();
    open(&desktop, "Cash");
    open(&desktop, "Savings");
    settle(&mut desktop, &mut phone);
    pause();
    (desktop, phone)
}

/// What a person puts into the account Cash on one device: one of each of the rows that name an account, and four transfers.
fn put_rows_in_cash(conn: &Connection) {
    let (cash, savings) = (account_id(conn, "Cash"), account_id(conn, "Savings"));
    spend_in(conn, "R payment", cash);
    bill_in(conn, "R item", cash);
    lend_in(conn, "R entry", cash);
    move_money(conn, cash, savings, "T cash to savings");
    move_money(conn, savings, cash, "T savings to cash");
    move_money(conn, MAIN_ID, cash, "T main to cash");
    move_money(conn, cash, MAIN_ID, "T cash to main");
}

fn stamp_and_updated_at(conn: &Connection, table: &str, column: &str, value: &str) -> (String, String) {
    conn.query_row(&format!("SELECT stamp, updated_at FROM {table} WHERE {column} = ?1"), [value], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
}

fn marker_of(conn: &Connection, sync_id: &str) -> Option<String> {
    conn.query_row("SELECT stamp FROM sync_deleted WHERE sync_id = ?1", [sync_id], |r| r.get(0)).ok()
}

/// The accounts a device shows, in its order: name, kind, balance, archived.
fn accounts_shown(conn: &Connection) -> Vec<(String, String, String, bool)> {
    accounts::list(conn, TODAY).unwrap().into_iter().map(|a| (a.name, a.kind.as_sql().to_string(), a.balance, a.archived)).collect()
}

/// The version of Stamp's order the sync works by.
fn later_of(a: &str, b: &str) -> String {
    std::cmp::max(Stamp::parse(a).unwrap(), Stamp::parse(b).unwrap()).format()
}

fn after(stamp: &str) -> String {
    engine::after(&Stamp::parse(stamp).unwrap()).format()
}

// --- the first copy and the main account -------------------------------------------------------------------------------

#[test]
fn a_device_with_accounts_transfers_and_rows_in_several_accounts_gives_a_new_phone_a_full_copy() {
    let mut desktop = device("EUR");
    let mut phone = new_phone(&desktop);
    category(&desktop, "Food", CategoryType::Expense);
    people::create(&desktop, "Anna").unwrap();
    rename(&desktop, MAIN_ID, "Current");
    let savings = accounts::create(
        &desktop,
        &NewAccount { name: "Savings".into(), kind: Kind::Savings, number: Some("IE29 AIBK 9311 5212 3456 78".into()), opening_balance: Some("1000.00".into()), opening_date: Some("2026-01-01".into()), bic: None, notes: None, group: None, include_in_net_worth: None, virtual_balance: None, currency: None, rate: None },
        TODAY,
    )
    .unwrap()
    .id;
    let card = accounts::create(&desktop, &NewAccount { name: "Card".into(), kind: Kind::CreditCard, number: None, opening_balance: Some("-250.00".into()), opening_date: Some("2026-02-01".into()), bic: None, notes: None, group: None, include_in_net_worth: None, virtual_balance: None, currency: None, rate: None }, TODAY).unwrap().id;
    let old = open(&desktop, "Old");
    spend_in(&desktop, "On the card", card);
    spend_in(&desktop, "Long ago", old);
    bill_in(&desktop, "Card bill", card);
    lend_in(&desktop, "Cash back", savings);
    move_money(&desktop, MAIN_ID, savings, "Monthly saving");
    move_money(&desktop, MAIN_ID, card, "Pay the card");
    accounts::archive(&desktop, old, TODAY).unwrap();
    accounts::reorder(&desktop, &[card, savings, MAIN_ID]).unwrap();
    let outcome = settle(&mut desktop, &mut phone);
    assert!(outcome.compared.contains(&Table::Accounts) && outcome.compared.contains(&Table::Transfers), "{:?}", outcome.compared);
    assert_eq!(count(&phone, "SELECT count(*) FROM accounts"), 4);
    assert_eq!(count(&phone, "SELECT count(*) FROM transfers"), 2);
    assert_eq!(text(&phone, "SELECT name FROM accounts WHERE id = 1"), "Current", "the rename of the main account arrived");
    assert_eq!(text(&phone, "SELECT number FROM accounts WHERE name = 'Savings'"), "IE29 AIBK 9311 5212 3456 78");
    assert_eq!(accounts_shown(&phone), accounts_shown(&desktop), "the same accounts, in the same order, with the same balances");
    assert_eq!(accounts::list(&phone, TODAY).unwrap().iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Card", "Savings", "Current", "Old"]);
    assert_eq!(text(&phone, "SELECT a.name FROM transactions t JOIN accounts a ON a.id = t.account_id WHERE t.name = 'On the card'"), "Card");
    assert_eq!(text(&phone, "SELECT a.name FROM recurring_transactions t JOIN accounts a ON a.id = t.account_id WHERE t.name = 'Card bill'"), "Card");
    assert_eq!(text(&phone, "SELECT a.name FROM ledger_entries t JOIN accounts a ON a.id = t.account_id WHERE t.note = 'Cash back'"), "Savings");
    assert_eq!(text(&phone, "SELECT a.name FROM transfers t JOIN accounts a ON a.id = t.to_account_id WHERE t.note = 'Pay the card'"), "Card");
    assert_eq!(count(&phone, "SELECT archived FROM accounts WHERE name = 'Old'"), 1);
}

#[test]
fn devices_that_only_ran_the_migration_send_no_account_and_have_one_main_account_each() {
    let mut desktop = device("EUR");
    let mut phone = device("EUR");
    for conn in [&desktop, &phone] {
        assert_eq!(count(conn, "SELECT count(*) FROM accounts"), 1);
        assert_eq!((count(conn, "SELECT id FROM accounts"), text(conn, "SELECT sync_id FROM accounts"), text(conn, "SELECT stamp FROM accounts")), (1, MAIN_SYNC_ID.to_string(), UNSET_STAMP.to_string()));
    }
    // the same row on both, so the lists agree before anything is said
    let (on_desktop, on_phone) = (engine::digest(&desktop).unwrap(), engine::digest(&phone).unwrap());
    assert_eq!(on_desktop.0[&Table::Accounts], on_phone.0[&Table::Accounts]);
    assert_eq!(on_desktop.0[&Table::Transfers], on_phone.0[&Table::Transfers]);
    assert!(engine::conflicts(&on_desktop, &on_phone, &[Table::Accounts, Table::Transfers], Some(0)).0.is_empty(), "no conflict over the main account");
    let outcome = settle(&mut desktop, &mut phone);
    assert!(!outcome.compared.contains(&Table::Accounts) && !outcome.compared.contains(&Table::Transfers), "not even compared: {:?}", outcome.compared);
    for conn in [&desktop, &phone] {
        assert_eq!(count(conn, "SELECT count(*) FROM accounts"), 1, "one main account, not two");
        assert_eq!(text(conn, "SELECT stamp FROM accounts"), UNSET_STAMP, "nobody touched it");
        assert_eq!(count(conn, "SELECT count(*) FROM sync_deleted"), 0);
    }
}

#[test]
fn a_rename_or_a_new_opening_balance_of_the_main_account_on_one_device_arrives_on_the_other() {
    for on_desktop in [true, false] {
        let (mut desktop, mut phone) = paired();
        let (changed, other) = if on_desktop { (&desktop, &phone) } else { (&phone, &desktop) };
        pause();
        accounts::update(changed, MAIN_ID, &AccountPatch { name: Some("Wallet".into()), opening_balance: Some("-30.00".into()), opening_date: Some("2026-02-01".into()), ..Default::default() }, TODAY).unwrap();
        assert!(text(changed, "SELECT stamp FROM accounts").as_str() > UNSET_STAMP, "an edit gets a real stamp");
        assert_eq!(text(other, "SELECT stamp FROM accounts"), UNSET_STAMP);
        let outcome = settle(&mut desktop, &mut phone);
        assert_eq!((outcome.compared.as_slice(), count(&desktop, "SELECT count(*) FROM accounts")), ([Table::Accounts].as_slice(), 1));
        for conn in [&desktop, &phone] {
            assert_eq!((text(conn, "SELECT name FROM accounts"), count(conn, "SELECT opening_balance_minor FROM accounts"), text(conn, "SELECT opening_date FROM accounts")), ("Wallet".into(), -3000, "2026-02-01".into()));
        }
    }
}

#[test]
fn both_devices_renaming_the_main_account_differently_is_an_ordinary_conflict_with_the_name_to_choose_from() {
    for (choice, expected) in [(engine::Choice::Theirs, "Phone wallet"), (engine::Choice::Mine, "Desktop wallet")] {
        let (mut desktop, mut phone) = paired();
        let met = now();
        pause();
        rename(&desktop, MAIN_ID, "Desktop wallet");
        rename(&phone, MAIN_ID, "Phone wallet");
        let (conflicts, views) = conflicts_between(&desktop, &phone, met);
        assert_eq!(conflicts.len(), 1);
        assert_eq!((conflicts[0].table, conflicts[0].id.as_str()), (Table::Accounts, MAIN_SYNC_ID));
        assert_eq!((views[0].title.as_str(), fields(&views, Table::Accounts)), ("Desktop wallet", vec![("name".to_string(), "Desktop wallet".to_string(), "Phone wallet".to_string())]));
        engine::decide(&mut desktop, &conflicts, &[choice]).unwrap();
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(text(conn, "SELECT name FROM accounts"), expected, "{choice:?}");
            assert_eq!(count(conn, "SELECT count(*) FROM accounts"), 1);
        }
    }
}

/// What a sync would put in front of the person at `desktop`: the conflicts since they met, and what differs in each.
fn conflicts_between(desktop: &Connection, phone: &Connection, met: u64) -> (Vec<engine::Conflict>, Vec<engine::ConflictView>) {
    let (mine, theirs) = (engine::digest(desktop).unwrap(), engine::digest(phone).unwrap());
    let (conflicts, more) = engine::conflicts(&mine, &theirs, &Table::ALL, Some(met));
    assert_eq!(more, 0);
    let mut collector = RowsCollector::new(engine::conflict_tables(&conflicts));
    for message in engine::conflict_rows(phone, &conflicts).unwrap() {
        if let Message::Rows(rows) = message {
            collector.push(rows).unwrap();
        }
    }
    let views = engine::describe(desktop, &conflicts, &collector.finish().unwrap()).unwrap();
    (conflicts, views)
}

fn fields(views: &[engine::ConflictView], table: Table) -> Vec<(String, String, String)> {
    views.iter().find(|v| v.table == table).unwrap().fields.iter().map(|f| (f.field.to_string(), f.mine.clone(), f.theirs.clone())).collect()
}

fn texts(list: &[(&str, &str, &str)]) -> Vec<(String, String, String)> {
    list.iter().map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string())).collect()
}

#[test]
fn a_conflict_over_an_account_a_transfer_or_the_account_of_a_payment_lists_what_differs() {
    let (mut desktop, mut phone) = met_with_accounts();
    let (cash, savings) = (account_id(&desktop, "Cash"), account_id(&desktop, "Savings"));
    let cash_sync_id = account_sync_id(&desktop, "Cash");
    let rent = move_money(&desktop, cash, savings, "rent");
    let coffee = spend_in(&desktop, "Coffee", cash);
    settle(&mut desktop, &mut phone);
    let met = now();
    pause();
    // the desktop changes everything about Cash, the transfer and the payment ...
    accounts::update(
        &desktop,
        cash,
        &AccountPatch { name: Some("Rainy day".into()), kind: Some(Kind::Savings), number: Some(Some("IE00".into())), opening_balance: Some("50.00".into()), opening_date: Some("2026-02-01".into()), ..Default::default() },
        TODAY,
    )
    .unwrap();
    transfers::update(&desktop, rent, &TransferPatch { from_account_id: Some(MAIN_ID.to_string()), amount: Some("12.00".into()), date: Some("2026-03-04".into()), note: Some(Some("moved".into())), ..Default::default() }).unwrap();
    transactions::update(&desktop, coffee, &TransactionPatch { account_id: Some(savings.to_string()), ..Default::default() }).unwrap();
    // ... and so does the phone, differently
    let (cash_p, rent_p, coffee_p) = (account_id(&phone, "Cash"), transfer_named(&phone, "rent") , id_of(&phone, "transactions", "Coffee"));
    accounts::archive(&phone, cash_p, TODAY).unwrap();
    transfers::update(&phone, rent_p, &TransferPatch { to_account_id: Some(MAIN_ID.to_string()), amount: Some("13.00".into()), date: Some("2026-03-05".into()), note: Some(None), ..Default::default() }).unwrap();
    transactions::update(&phone, coffee_p, &TransactionPatch { account_id: Some(MAIN_ID.to_string()), ..Default::default() }).unwrap();

    let (conflicts, views) = conflicts_between(&desktop, &phone, met);
    assert_eq!(conflicts.iter().map(|c| c.table).collect::<Vec<_>>(), [Table::Accounts, Table::Transactions, Table::Transfers]);
    assert_eq!(
        fields(&views, Table::Accounts),
        texts(&[
            ("name", "Rainy day", "Cash"),
            ("type", "SAVINGS", "CHECKING"),
            ("number", "IE00", ""),
            ("openingBalance", "50.00", "0.00"),
            ("openingDate", "2026-02-01", "2026-01-01"),
            ("archived", "no", "yes"),
        ])
    );
    // the sides of a transfer and the account of a payment are the names of the accounts on this device
    assert_eq!(
        fields(&views, Table::Transfers),
        texts(&[("from", "Main account", "Rainy day"), ("to", "Savings", "Main account"), ("amount", "12.00", "13.00"), ("date", "2026-03-04", "2026-03-05"), ("note", "moved", "")])
    );
    assert_eq!(fields(&views, Table::Transactions), texts(&[("account", "Savings", "Main account")]));
    assert_eq!(views.iter().find(|v| v.table == Table::Accounts).unwrap().title, "Rainy day");

    // the choices are only stamps: the account and the payment as the phone had them, the transfer as the desktop had it
    engine::decide(&mut desktop, &conflicts, &[engine::Choice::Theirs, engine::Choice::Theirs, engine::Choice::Mine]).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(text(conn, &format!("SELECT name || archived FROM accounts WHERE sync_id = '{cash_sync_id}'")), "Cash1");
        assert_eq!(text(conn, "SELECT a.name FROM transactions t JOIN accounts a ON a.id = t.account_id WHERE t.name = 'Coffee'"), "Main account");
        assert_eq!(text(conn, "SELECT note || amount_minor FROM transfers"), "moved1200");
    }
}

#[test]
fn a_fresh_phone_is_empty_for_pairing_until_anything_about_accounts_is_changed() {
    let fresh = || device("EUR");
    assert!(engine::is_empty(&fresh()).unwrap(), "only the untouched main account");
    type Edit = fn(&Connection);
    let edits: [(&str, Edit); 11] = [
        ("a rename of the main account", |c| rename(c, MAIN_ID, "Wallet")),
        ("its opening balance", |c| {
            accounts::update(c, MAIN_ID, &AccountPatch { opening_balance: Some("10.00".into()), ..Default::default() }, TODAY).unwrap();
        }),
        ("its number", |c| {
            accounts::update(c, MAIN_ID, &AccountPatch { number: Some(Some("IE29".into())), ..Default::default() }, TODAY).unwrap();
        }),
        ("a second account", |c| {
            open(c, "Cash");
        }),
        ("a transfer", |c| {
            let cash = open(c, "Cash");
            move_money(c, MAIN_ID, cash, "t");
        }),
        ("an account that was made and deleted again (it leaves a marker)", |c| {
            let cash = open(c, "Cash");
            accounts::delete(c, cash).unwrap();
        }),
        ("a BIC of the main account", |c| {
            accounts::update(c, MAIN_ID, &AccountPatch { bic: Some(Some("DEUTDEFF".into())), ..Default::default() }, TODAY).unwrap();
        }),
        ("its notes", |c| {
            accounts::update(c, MAIN_ID, &AccountPatch { notes: Some(Some("mine".into())), ..Default::default() }, TODAY).unwrap();
        }),
        ("its group", |c| {
            accounts::update(c, MAIN_ID, &AccountPatch { group: Some(Some("Everyday".into())), ..Default::default() }, TODAY).unwrap();
        }),
        ("leaving it out of net worth", |c| {
            accounts::update(c, MAIN_ID, &AccountPatch { include_in_net_worth: Some(false), ..Default::default() }, TODAY).unwrap();
        }),
        ("a virtual balance of the main account", |c| {
            accounts::update(c, MAIN_ID, &AccountPatch { virtual_balance: Some("1.00".into()), ..Default::default() }, TODAY).unwrap();
        }),
    ];
    for (what, edit) in edits {
        let phone = fresh();
        edit(&phone);
        assert!(!engine::is_empty(&phone).unwrap(), "{what}: a phone like this holds data and is not paired");
        let mut phone = phone;
        assert!(matches!(engine::adopt_currency(&mut phone, "USD", 3, &[]), Err(SyncError::Currency)), "{what}: and it does not take another currency");
    }
    // a transfer on a phone whose accounts are all untouched cannot be: it needs a second account; the count of transfers is checked as well
    assert!(engine::is_empty(&fresh()).unwrap());
}

// --- the main account is never deleted or archived ----------------------------------------------------------------------

fn newer_than_everything() -> String {
    Stamp::new(now() + 10_000, 0, "abcdef01").format()
}

#[test]
fn a_marker_for_the_main_account_stops_the_sync_and_changes_nothing() {
    let (mut desktop, mut phone) = paired();
    open(&desktop, "Fine");
    let before = snapshot(&phone).unwrap();
    for stamp in [newer_than_everything(), Stamp::new(1, 0, "abcdef01").format()] {
        // alone, and in a batch that is otherwise good (a new account, with its row)
        for with_a_good_row in [false, true] {
            let mut incoming = if with_a_good_row { rows_to(&desktop, &phone) } else { Incoming::default() };
            assert_eq!(incoming.rows.contains_key(&Table::Accounts), with_a_good_row);
            incoming.markers.insert(Table::Accounts, vec![Marker { sync_id: MAIN_SYNC_ID.into(), stamp: stamp.clone() }]);
            let result = engine::apply(&mut phone, &incoming, now(), None);
            assert!(matches!(result, Err(SyncError::Invalid("account"))), "{stamp} {with_a_good_row}: {result:?}");
            assert_eq!(snapshot(&phone).unwrap(), before, "nothing of the batch was kept");
            assert_eq!(count(&phone, "SELECT applying FROM sync_clock"), 0);
            assert_eq!(count(&phone, "SELECT count(*) FROM sync_deleted WHERE sync_id = '00000000000000000000000000000001'"), 0);
            assert!(accounts::check(&phone).unwrap());
        }
    }
    // the same through a whole sync: a device that somehow holds such a marker is not obeyed
    desktop.execute("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES (?1, 'accounts', ?2)", [MAIN_SYNC_ID, &newer_than_everything()]).unwrap();
    let result = sync(&mut desktop, &mut phone, TODAY, now(), None);
    assert!(matches!(result, Err(SyncError::Invalid("account"))), "{result:?}");
    assert_eq!(snapshot(&phone).unwrap(), before);
    assert!(accounts::check(&phone).unwrap());
}

#[test]
fn a_main_account_row_that_is_archived_is_refused_even_when_it_gets_past_the_wire() {
    let (_desktop, mut phone) = paired();
    let before = snapshot(&phone).unwrap();
    let row = wire::AccountRow {
        sync_id: MAIN_SYNC_ID.into(),
        stamp: newer_than_everything(),
        name: "Main account".into(),
        kind: "CHECKING".into(),
        number: None,
        opening_balance_minor: 0,
        opening_date: "1970-01-01".into(),
        archived: 1,
        position: 0,
        bic: None,
        notes: None,
        group: None,
        include_net_worth: 1,
        virtual_balance_minor: 0,
        currency: None,
        rate: None,
        created_at: "1970-01-01T00:00:00.000Z".into(),
        updated_at: "1970-01-01T00:00:00.000Z".into(),
    };
    // the wire refuses it ...
    assert!(wire::parse_row(Table::Accounts, &wire::row_to_value(&Row::Account(row.clone()))).is_err());
    // ... and so does the applier when it is put in by hand, with the rest of its batch
    let mut incoming = Incoming::default();
    incoming.rows.insert(Table::Accounts, vec![Row::Account(row)]);
    let result = engine::apply(&mut phone, &incoming, now(), None);
    assert!(matches!(result, Err(SyncError::Invalid("account"))), "{result:?}");
    assert_eq!(snapshot(&phone).unwrap(), before);
    assert_eq!(count(&phone, "SELECT archived FROM accounts WHERE id = 1"), 0);
    assert!(accounts::check(&phone).unwrap());
}

// --- a row whose account is nowhere ---------------------------------------------------------------------------------------

/// A desktop and a phone that have met, and on the desktop a new account, Wallet, with one of each kind of row in it, all unsent.
fn rows_naming_an_account_the_phone_lacks() -> (Connection, Connection) {
    let (desktop, phone) = met_with_accounts();
    let wallet = open(&desktop, "Wallet");
    spend_in(&desktop, "W payment", wallet);
    bill_in(&desktop, "W item", wallet);
    lend_in(&desktop, "W entry", wallet);
    move_money(&desktop, MAIN_ID, wallet, "W transfer");
    (desktop, phone)
}

fn only(mut incoming: Incoming, tables: &[Table]) -> Incoming {
    incoming.rows.retain(|table, _| tables.contains(table));
    incoming.markers.clear();
    incoming
}

fn refused_with_a_broken_reference(phone: &mut Connection, incoming: &Incoming, what: &str) {
    let before = snapshot(phone).unwrap();
    let result = engine::apply(phone, incoming, now(), None);
    assert!(matches!(result, Err(SyncError::Invalid("reference"))), "{what}: {result:?}");
    assert_eq!(snapshot(phone).unwrap(), before, "{what}: nothing was applied");
    assert_eq!(count(phone, "SELECT applying FROM sync_clock"), 0);
}

#[test]
fn a_row_whose_account_is_nowhere_stops_the_sync_and_changes_nothing_and_comes_in_with_its_account() {
    for (table, what) in [(Table::Transactions, "a payment"), (Table::RecurringTransactions, "a recurring item"), (Table::LedgerEntries, "an entry"), (Table::Transfers, "a transfer")] {
        let (desktop, mut phone) = rows_naming_an_account_the_phone_lacks();
        let incoming = only(rows_to(&desktop, &phone), &[table]);
        assert!(!incoming.rows[&table].is_empty(), "{what}");
        refused_with_a_broken_reference(&mut phone, &incoming, what);
        // the same rows with the row of their account apply
        let incoming = only(rows_to(&desktop, &phone), &[Table::Accounts, table]);
        engine::apply(&mut phone, &incoming, now(), None).unwrap();
        assert_eq!(count(&phone, "SELECT count(*) FROM accounts WHERE name = 'Wallet'"), 1, "{what}");
        assert!(accounts::check(&phone).unwrap());
    }
}

#[test]
fn a_transfer_with_a_side_nobody_heard_of_is_refused_on_either_side() {
    for side in ["from", "to", "both"] {
        let (desktop, mut phone) = rows_naming_an_account_the_phone_lacks();
        let mut incoming = only(rows_to(&desktop, &phone), &[Table::Accounts, Table::Transfers]);
        for row in incoming.rows.get_mut(&Table::Transfers).unwrap() {
            let Row::Transfer(t) = row else { unreachable!() };
            if side != "to" {
                t.from = "ab".repeat(16);
            }
            if side != "from" {
                t.to = "cd".repeat(16);
            }
        }
        refused_with_a_broken_reference(&mut phone, &incoming, side);
    }
}

#[test]
fn a_batch_with_the_row_of_its_account_applies_whatever_order_the_rows_come_in() {
    let (desktop, _) = rows_naming_an_account_the_phone_lacks();
    let mut rng = Rng::new(7);
    for round in 0..12 {
        let mut phone = new_phone(&desktop);
        let mut incoming = rows_to(&desktop, &phone);
        // the rows of every table in another order each time, the account rows among them, and the tables put in at another time
        let tables: Vec<Table> = incoming.rows.keys().copied().collect();
        let mut shuffled = Incoming::default();
        let mut order = tables.clone();
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i as u64 + 1) as usize);
        }
        for table in order {
            let mut rows = incoming.rows.remove(&table).unwrap();
            for i in (1..rows.len()).rev() {
                rows.swap(i, rng.below(i as u64 + 1) as usize);
            }
            shuffled.rows.insert(table, rows);
        }
        engine::apply(&mut phone, &shuffled, now(), None).unwrap_or_else(|e| panic!("round {round}: {e}"));
        assert_same(&desktop, &phone, &format!("round {round}"));
        assert!(accounts::check(&phone).unwrap());
    }
}

#[test]
fn a_peer_that_sends_the_rows_that_name_an_account_before_the_accounts_is_not_followed() {
    let (desktop, phone) = rows_naming_an_account_the_phone_lacks();
    let (mine, theirs) = (engine::digest(&desktop).unwrap(), engine::digest(&phone).unwrap());
    let tables = [Table::Accounts, Table::Transactions];
    let messages = engine::outgoing(&desktop, &mine, &theirs, &tables).unwrap();
    let mut collector = RowsCollector::new(tables.to_vec());
    let Some(Message::Rows(last)) = messages.last().cloned() else { panic!() };
    assert_eq!(last.table, Table::Transactions);
    assert!(collector.push(last).is_err(), "the payments cannot come before the accounts");
    let mut collector = RowsCollector::new(tables.to_vec());
    for message in messages {
        let Message::Rows(rows) = message else { panic!() };
        collector.push(rows).unwrap();
    }
    assert!(collector.is_complete());
}

// --- an account deleted on one device while the other puts rows in it ---------------------------------------------------

#[test]
fn an_empty_account_deleted_on_one_device_while_the_other_puts_rows_in_it_ends_equal_with_the_rows_in_the_main_account() {
    // who deletes, which of the two the sync is started from, and whether the rows are older or newer than the deletion
    for (deleter_is_desktop, deleter_first, rows_are_older) in [(true, true, true), (true, true, false), (true, false, true), (true, false, false), (false, true, true), (false, true, false), (false, false, true), (false, false, false)] {
        let label = format!("deleter on the desktop: {deleter_is_desktop}, sync started by the deleter: {deleter_first}, rows older: {rows_are_older}");
        let (desktop, phone) = met_with_accounts();
        let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
        let cash = account_sync_id(&a, "Cash");
        let delete_cash = |a: &Connection| accounts::delete(a, account_id(a, "Cash")).unwrap();
        if rows_are_older {
            put_rows_in_cash(&b);
            pause();
            delete_cash(&a);
        } else {
            delete_cash(&a);
            pause();
            put_rows_in_cash(&b);
        }
        let deleted_at = marker_of(&a, &cash).unwrap();
        // what the rows were when they were made
        let names = [("transactions", "name", "R payment"), ("recurring_transactions", "name", "R item"), ("ledger_entries", "note", "R entry")];
        let before: Vec<(String, String)> = names.iter().map(|(table, column, name)| stamp_and_updated_at(&b, table, column, name)).collect();
        let transfer_names = ["T cash to savings", "T savings to cash", "T main to cash", "T cash to main"];
        let transfers_before: Vec<(String, String, String)> = transfer_names
            .iter()
            .map(|name| {
                let (stamp, updated_at) = stamp_and_updated_at(&b, "transfers", "note", name);
                (text(&b, &format!("SELECT sync_id FROM transfers WHERE note = '{name}'")), stamp, updated_at)
            })
            .collect();

        if deleter_first {
            settle(&mut a, &mut b);
        } else {
            settle(&mut b, &mut a);
        }

        for (who, conn) in [("deleter", &a), ("adder", &b)] {
            let label = format!("{label}, on the {who}");
            assert_eq!(count(conn, "SELECT count(*) FROM accounts WHERE name = 'Cash'"), 0, "{label}");
            assert_eq!(marker_of(conn, &cash).as_deref(), Some(deleted_at.as_str()), "{label}: the same marker");
            // one payment, one item, one entry: in the main account, with the later of their stamp and the deletion's; updated_at as it was
            for ((table, column, name), (stamp, updated_at)) in names.iter().zip(&before) {
                let here = stamp_and_updated_at(conn, table, column, name);
                assert_eq!(count(conn, &format!("SELECT account_id FROM {table} WHERE {column} = '{name}'")), MAIN_ID, "{label}: {name}");
                assert_eq!(here.0, later_of(stamp, &deleted_at), "{label}: the stamp of {name}");
                assert_eq!(&here.1, updated_at, "{label}: {name} keeps the time it was written");
                assert_eq!(here.0 > *stamp, rows_are_older, "{label}: {name} was raised exactly when the deletion is the later");
            }
            // the transfers with one side in Cash: that side is the main account now, and the ones between Cash and the main account are gone
            let sides = |note: &str| -> (String, String, String) {
                conn.query_row(
                    "SELECT f.sync_id, t.sync_id, x.stamp FROM transfers x JOIN accounts f ON f.id = x.from_account_id JOIN accounts t ON t.id = x.to_account_id WHERE x.note = ?1",
                    [note],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap()
            };
            let savings = account_sync_id(conn, "Savings");
            let (from, to, stamp) = sides("T cash to savings");
            assert_eq!((from.as_str(), to.as_str(), stamp.as_str()), (MAIN_SYNC_ID, savings.as_str(), later_of(&transfers_before[0].1, &deleted_at).as_str()), "{label}");
            let (from, to, stamp) = sides("T savings to cash");
            assert_eq!((from.as_str(), to.as_str(), stamp.as_str()), (savings.as_str(), MAIN_SYNC_ID, later_of(&transfers_before[1].1, &deleted_at).as_str()), "{label}");
            for (sync_id, stamp, _) in &transfers_before[2..] {
                assert_eq!(count(conn, &format!("SELECT count(*) FROM transfers WHERE sync_id = '{sync_id}'")), 0, "{label}: a transfer with the main account on both sides is gone");
                assert_eq!(marker_of(conn, sync_id), Some(after(&later_of(stamp, &deleted_at))), "{label}: its marker is the one both devices work out");
            }
            assert_eq!(count(conn, "SELECT count(*) FROM transfers"), 2, "{label}");
        }
    }
}

#[test]
fn rows_that_were_moved_to_the_main_account_by_a_deletion_can_be_moved_on_and_synced_again() {
    let (mut desktop, mut phone) = met_with_accounts();
    put_rows_in_cash(&phone);
    pause();
    accounts::delete(&desktop, account_id(&desktop, "Cash")).unwrap();
    settle(&mut desktop, &mut phone);
    pause();
    // the person moves what arrived in the main account on to Savings, on the phone; then Savings is edited on the desktop
    let savings_p = account_id(&phone, "Savings");
    transactions::update(&phone, id_of(&phone, "transactions", "R payment"), &TransactionPatch { account_id: Some(savings_p.to_string()), ..Default::default() }).unwrap();
    pause();
    rename(&desktop, account_id(&desktop, "Savings"), "Rainy day");
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(text(conn, "SELECT a.name FROM transactions t JOIN accounts a ON a.id = t.account_id WHERE t.name = 'R payment'"), "Rainy day");
    }
}

#[test]
fn an_account_deleted_on_both_devices_at_once_is_one_deletion() {
    let (mut desktop, mut phone) = met_with_accounts();
    let cash = account_sync_id(&desktop, "Cash");
    accounts::delete(&desktop, account_id(&desktop, "Cash")).unwrap();
    pause();
    accounts::delete(&phone, account_id(&phone, "Cash")).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(count(conn, "SELECT count(*) FROM accounts"), 2);
        assert_eq!(count(conn, &format!("SELECT count(*) FROM sync_deleted WHERE sync_id = '{cash}'")), 1);
    }
}

#[test]
fn a_transfer_between_two_accounts_that_are_both_deleted_elsewhere_is_dropped_as_the_other_end_drops_it() {
    // the desktop deletes Cash and then Savings; the phone has a transfer between them, and another from Cash to the main account
    for desktop_first in [true, false] {
        let (mut desktop, mut phone) = met_with_accounts();
        let (cash, savings) = (account_id(&phone, "Cash"), account_id(&phone, "Savings"));
        let (cash_sync_id, savings_sync_id) = (account_sync_id(&phone, "Cash"), account_sync_id(&phone, "Savings"));
        let between = move_money(&phone, cash, savings, "between");
        let to_main = move_money(&phone, cash, MAIN_ID, "to main");
        let (between_id, between_stamp, _) = stamp_row(&phone, between);
        let (to_main_id, to_main_stamp, _) = stamp_row(&phone, to_main);
        pause();
        accounts::delete(&desktop, account_id(&desktop, "Cash")).unwrap();
        pause();
        accounts::delete(&desktop, account_id(&desktop, "Savings")).unwrap();
        let (cash_deleted, savings_deleted) = (marker_of(&desktop, &cash_sync_id).unwrap(), marker_of(&desktop, &savings_sync_id).unwrap());
        assert!(savings_deleted > cash_deleted);
        if desktop_first {
            settle(&mut desktop, &mut phone);
        } else {
            settle(&mut phone, &mut desktop);
        }
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT count(*) FROM transfers"), 0);
            assert_eq!(marker_of(conn, &between_id), Some(after(&later_of(&later_of(&between_stamp, &cash_deleted), &savings_deleted))), "the later of the two deletions counts");
            assert_eq!(marker_of(conn, &to_main_id), Some(after(&later_of(&to_main_stamp, &cash_deleted))));
        }
    }
}

/// `sync_id`, stamp and `updated_at` of a row of `transfers`.
fn stamp_row(conn: &Connection, id: i64) -> (String, String, String) {
    conn.query_row("SELECT sync_id, stamp, updated_at FROM transfers WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap()
}

// --- a deletion against an edit, an archive against a rename ---------------------------------------------------------------

#[test]
fn an_edit_of_an_account_newer_than_its_deletion_brings_the_account_back_on_both() {
    for deleter_is_desktop in [true, false] {
        for deleter_first in [true, false] {
            let (desktop, phone) = met_with_accounts();
            let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
            let cash = account_sync_id(&a, "Cash");
            accounts::delete(&a, account_id(&a, "Cash")).unwrap();
            pause();
            rename(&b, account_id(&b, "Cash"), "Cash box");
            if deleter_first {
                settle(&mut a, &mut b);
            } else {
                settle(&mut b, &mut a);
            }
            for conn in [&a, &b] {
                assert_eq!(text(conn, &format!("SELECT name FROM accounts WHERE sync_id = '{cash}'")), "Cash box");
                assert_eq!(marker_of(conn, &cash), None, "the marker goes with the deletion it was");
                assert_eq!(count(conn, "SELECT count(*) FROM accounts"), 3);
            }
        }
    }
}

#[test]
fn an_edit_of_an_account_older_than_its_deletion_leaves_it_deleted_on_both() {
    for deleter_is_desktop in [true, false] {
        for deleter_first in [true, false] {
            let (desktop, phone) = met_with_accounts();
            let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
            let cash = account_sync_id(&a, "Cash");
            rename(&b, account_id(&b, "Cash"), "Cash box");
            pause();
            accounts::delete(&a, account_id(&a, "Cash")).unwrap();
            if deleter_first {
                settle(&mut a, &mut b);
            } else {
                settle(&mut b, &mut a);
            }
            for conn in [&a, &b] {
                assert_eq!(count(conn, &format!("SELECT count(*) FROM accounts WHERE sync_id = '{cash}'")), 0);
                assert!(marker_of(conn, &cash).is_some());
            }
        }
    }
}

#[test]
fn an_account_deleted_here_stays_deleted_when_an_older_copy_of_its_row_comes_back_and_returns_for_a_newer_one() {
    let (desktop, mut phone) = met_with_accounts();
    let cash = account_sync_id(&phone, "Cash");
    // the desktop's row, as it is now: older than the deletion that follows
    let older = {
        let mut rows = rows_to(&desktop, &device("EUR")).rows;
        rows.remove(&Table::Accounts).unwrap().into_iter().find(|r| r.sync_id() == cash).unwrap()
    };
    accounts::delete(&phone, account_id(&phone, "Cash")).unwrap();
    let before = snapshot(&phone).unwrap();
    let mut incoming = Incoming::default();
    incoming.rows.insert(Table::Accounts, vec![older.clone()]);
    let summary = engine::apply(&mut phone, &incoming, now(), None).unwrap();
    assert_eq!((summary.added, summary.kept_yours), (0, 1), "dropped: {summary:?}");
    assert_eq!(snapshot(&phone).unwrap(), before);
    // a newer one removes the marker and comes in as it is
    let Row::Account(mut newer) = older else { unreachable!() };
    newer.stamp = Stamp::new(now() + 5_000, 0, "abcdef01").format();
    newer.name = "Cash again".into();
    let mut incoming = Incoming::default();
    incoming.rows.insert(Table::Accounts, vec![Row::Account(newer)]);
    engine::apply(&mut phone, &incoming, now(), None).unwrap();
    assert_eq!(marker_of(&phone, &cash), None);
    assert_eq!(text(&phone, &format!("SELECT name FROM accounts WHERE sync_id = '{cash}'")), "Cash again");
}

#[test]
fn archiving_on_one_device_while_the_other_renames_the_same_account_the_newer_whole_row_wins() {
    for archive_first in [true, false] {
        for archiver_is_desktop in [true, false] {
            let (desktop, phone) = met_with_accounts();
            let (mut archiver, mut renamer) = if archiver_is_desktop { (desktop, phone) } else { (phone, desktop) };
            let cash = account_sync_id(&archiver, "Cash");
            if archive_first {
                accounts::archive(&archiver, account_id(&archiver, "Cash"), TODAY).unwrap();
                pause();
                rename(&renamer, account_id(&renamer, "Cash"), "Cash box");
            } else {
                rename(&renamer, account_id(&renamer, "Cash"), "Cash box");
                pause();
                accounts::archive(&archiver, account_id(&archiver, "Cash"), TODAY).unwrap();
            }
            settle(&mut archiver, &mut renamer);
            // the later change decides everything about the row: the other one is not mixed in
            let (name, archived) = if archive_first { ("Cash box", 0) } else { ("Cash", 1) };
            for conn in [&archiver, &renamer] {
                assert_eq!(text(conn, &format!("SELECT name || archived FROM accounts WHERE sync_id = '{cash}'")), format!("{name}{archived}"), "archive first: {archive_first}");
            }
        }
    }
}

// --- the same name twice ----------------------------------------------------------------------------------------------------

#[test]
fn two_devices_that_rename_different_accounts_to_one_name_keep_both_and_merge_nothing() {
    let (mut desktop, mut phone) = met_with_accounts();
    let (cash_d, savings_p) = (account_id(&desktop, "Cash"), account_id(&phone, "Savings"));
    spend_in(&desktop, "Rent", cash_d);
    spend_in(&phone, "Coffee", savings_p);
    settle(&mut desktop, &mut phone);
    pause();
    rename(&desktop, account_id(&desktop, "Cash"), "Wallet");
    rename(&phone, account_id(&phone, "Savings"), "Wallet");
    let outcome = settle(&mut desktop, &mut phone);
    for summary in [outcome.first, outcome.second] {
        assert_eq!((summary.merged, summary.dropped, summary.deleted), (0, 0, 0), "no merge, no deletion: {summary:?}");
    }
    for conn in [&desktop, &phone] {
        assert_eq!(count(conn, "SELECT count(*) FROM accounts WHERE name = 'Wallet'"), 2, "both stay");
        assert_eq!(count(conn, "SELECT count(*) FROM accounts"), 3);
        assert_eq!(count(conn, "SELECT count(*) FROM sync_deleted"), 0);
        // each keeps what it held: the two payments are in two accounts, and each account has its own balance
        assert_eq!(count(conn, "SELECT count(DISTINCT account_id) FROM transactions"), 2);
        let wallets: Vec<String> = accounts_shown(conn).into_iter().filter(|a| a.0 == "Wallet").map(|a| a.2).collect();
        assert_eq!(wallets, ["-5.00", "-5.00"], "not one account holding both payments");
    }
}

#[test]
fn two_devices_that_make_an_account_of_one_name_keep_both_and_do_not_add_up_their_balances() {
    let (mut desktop, mut phone) = met_with_accounts();
    for (conn, opening) in [(&desktop, "100.00"), (&phone, "40.00")] {
        accounts::create(conn, &NewAccount { name: "Rainy day".into(), kind: Kind::Savings, number: None, opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), bic: None, notes: None, group: None, include_in_net_worth: None, virtual_balance: None, currency: None, rate: None }, TODAY).unwrap();
    }
    let outcome = settle(&mut desktop, &mut phone);
    assert_eq!((outcome.first.merged, outcome.second.merged, outcome.first.dropped, outcome.second.dropped), (0, 0, 0, 0));
    for conn in [&desktop, &phone] {
        let mut balances: Vec<String> = accounts::list(conn, TODAY).unwrap().into_iter().filter(|a| a.name == "Rainy day").map(|a| a.balance).collect();
        balances.sort();
        assert_eq!(balances, ["100.00", "40.00"], "two accounts, each with its own balance");
        assert_eq!(accounts::summary(conn, TODAY).unwrap().in_accounts, "140.00");
    }
    // and a second round of the same changes keeps on being two
    pause();
    for conn in [&desktop, &phone] {
        accounts::create(conn, &NewAccount { name: "Rainy day 2".into(), kind: Kind::Cash, number: None, opening_balance: None, opening_date: Some("2026-01-01".into()), bic: None, notes: None, group: None, include_in_net_worth: None, virtual_balance: None, currency: None, rate: None }, TODAY).unwrap();
    }
    settle(&mut desktop, &mut phone);
    assert_eq!(count(&desktop, "SELECT count(*) FROM accounts WHERE name = 'Rainy day 2'"), 2);
}

// --- rows in an archived account -------------------------------------------------------------------------------------------

#[test]
fn a_row_that_arrives_in_an_archived_account_stays_in_it() {
    type Add = fn(&Connection, i64);
    let kinds: [(&str, Add); 4] = [
        ("transactions", |c, x| {
            spend_in(c, "Arrived", x);
        }),
        ("recurring_transactions", |c, x| {
            bill_in(c, "Arrived", x);
        }),
        ("ledger_entries", |c, x| {
            lend_in(c, "Arrived", x);
        }),
        ("transfers", |c, x| {
            move_money(c, x, MAIN_ID, "Arrived");
        }),
    ];
    for (table, add) in kinds {
        for archiver_is_desktop in [true, false] {
            let (desktop, phone) = met_with_accounts();
            let (mut archiver, mut other) = if archiver_is_desktop { (desktop, phone) } else { (phone, desktop) };
            let cash = account_sync_id(&archiver, "Cash");
            accounts::archive(&archiver, account_id(&archiver, "Cash"), TODAY).unwrap();
            pause();
            // the other device does not know yet: for it the account is open
            add(&other, account_id(&other, "Cash"));
            settle(&mut archiver, &mut other);
            for conn in [&archiver, &other] {
                assert_eq!(count(conn, &format!("SELECT archived FROM accounts WHERE sync_id = '{cash}'")), 1, "{table}: still archived");
                let sql = match table {
                    "transfers" => format!("SELECT count(*) FROM transfers x JOIN accounts a ON a.id = x.from_account_id WHERE a.sync_id = '{cash}' AND x.note = 'Arrived'"),
                    _ => format!("SELECT count(*) FROM {table} x JOIN accounts a ON a.id = x.account_id WHERE a.sync_id = '{cash}'"),
                };
                assert_eq!(count(conn, &sql), 1, "{table}: it stays in the archived account");
            }
            // and the balance of the archived account counts it
            assert_eq!(accounts_shown(&archiver), accounts_shown(&other));
        }
    }
}

// --- transfers: the ordinary rules --------------------------------------------------------------------------------------------

/// A device pair with a transfer of 10.00 from Cash to Savings, named `rent`, synced; and the ids of it on each.
fn met_with_a_transfer() -> (Connection, Connection, i64, i64) {
    let (mut desktop, mut phone) = met_with_accounts();
    let (cash, savings) = (account_id(&desktop, "Cash"), account_id(&desktop, "Savings"));
    let on_desktop = move_money(&desktop, cash, savings, "rent");
    settle(&mut desktop, &mut phone);
    pause();
    let on_phone = transfer_named(&phone, "rent");
    (desktop, phone, on_desktop, on_phone)
}

fn amount_patch(amount: &str) -> TransferPatch {
    TransferPatch { amount: Some(amount.into()), ..Default::default() }
}

#[test]
fn a_newer_transfer_replaces_an_older_one_whole_and_an_older_one_loses() {
    for newer_is_desktop in [true, false] {
        let (mut desktop, mut phone, on_desktop, on_phone) = met_with_a_transfer();
        let (older, newer) = if newer_is_desktop { (&phone, &desktop) } else { (&desktop, &phone) };
        let (older_id, newer_id) = if newer_is_desktop { (on_phone, on_desktop) } else { (on_desktop, on_phone) };
        // both edit it after they met; the second edit is the newer, and it is the whole row that wins, not the fields it changed
        transfers::update(older, older_id, &TransferPatch { amount: Some("30.00".into()), date: Some("2026-03-09".into()), ..Default::default() }).unwrap();
        pause();
        transfers::update(newer, newer_id, &TransferPatch { note: Some(Some("newer".into())), ..Default::default() }).unwrap();
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(text(conn, "SELECT note || '/' || amount_minor || '/' || transfer_date FROM transfers"), "newer/1000/2026-03-03");
        }
    }
}

#[test]
fn a_transfer_deleted_after_an_edit_is_gone_on_both_and_edited_after_a_deletion_comes_back() {
    for deleter_is_desktop in [true, false] {
        // the edit is older
        let (mut desktop, mut phone, on_desktop, on_phone) = met_with_a_transfer();
        let (deleter, editor, deleted_id, edited_id) = if deleter_is_desktop { (&desktop, &phone, on_desktop, on_phone) } else { (&phone, &desktop, on_phone, on_desktop) };
        let sync_id = text(deleter, "SELECT sync_id FROM transfers");
        transfers::update(editor, edited_id, &amount_patch("30.00")).unwrap();
        pause();
        transfers::delete(deleter, deleted_id).unwrap();
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT count(*) FROM transfers"), 0);
            assert!(marker_of(conn, &sync_id).is_some());
        }

        // the edit is newer
        let (mut desktop, mut phone, on_desktop, on_phone) = met_with_a_transfer();
        let (deleter, editor, deleted_id, edited_id) = if deleter_is_desktop { (&desktop, &phone, on_desktop, on_phone) } else { (&phone, &desktop, on_phone, on_desktop) };
        let sync_id = text(deleter, "SELECT sync_id FROM transfers");
        transfers::delete(deleter, deleted_id).unwrap();
        pause();
        transfers::update(editor, edited_id, &amount_patch("30.00")).unwrap();
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT amount_minor FROM transfers"), 3000, "back with the edit");
            assert_eq!(marker_of(conn, &sync_id), None);
        }
    }
}

#[test]
fn a_transfer_that_changes_a_side_to_another_account_arrives_with_the_new_side() {
    let (mut desktop, mut phone, on_desktop, _) = met_with_a_transfer();
    let wallet = open(&desktop, "Wallet");
    transfers::update(&desktop, on_desktop, &TransferPatch { to_account_id: Some(wallet.to_string()), ..Default::default() }).unwrap();
    settle(&mut desktop, &mut phone);
    assert_eq!(text(&phone, "SELECT a.name FROM transfers x JOIN accounts a ON a.id = x.to_account_id"), "Wallet");
    assert_eq!(text(&phone, "SELECT a.name FROM transfers x JOIN accounts a ON a.id = x.from_account_id"), "Cash");
}

// --- a sync that is cut, or that one side finished ------------------------------------------------------------------------

/// Cash is deleted on the desktop while the phone puts rows in it: everything a sync has to move.
fn deletion_meets_rows() -> (Connection, Connection) {
    let (desktop, phone) = met_with_accounts();
    accounts::delete(&desktop, account_id(&desktop, "Cash")).unwrap();
    pause();
    put_rows_in_cash(&phone);
    (desktop, phone)
}

#[test]
fn a_sync_cut_after_any_message_changes_nothing_before_the_rows_are_in_and_the_next_one_finishes_it() {
    let total = {
        let (mut desktop, mut phone) = deletion_meets_rows();
        sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap().messages
    };
    assert!(total > 10, "{total} messages");
    for cut in 0..total {
        let (mut desktop, mut phone) = deletion_meets_rows();
        let before = (snapshot(&desktop).unwrap(), snapshot(&phone).unwrap());
        let result = sync_cut_after(&mut desktop, &mut phone, TODAY, now(), cut);
        assert!(result.is_err(), "cut after {cut} of {total} messages");
        // the rows are applied after the last of them is in: only the two closing messages come after
        if cut < total - 2 {
            assert_eq!((snapshot(&desktop).unwrap(), snapshot(&phone).unwrap()), before, "cut after {cut} of {total} messages: nothing was applied");
        } else {
            assert_same(&desktop, &phone, &format!("cut after {cut} of {total}: both had applied it"));
        }
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT applying FROM sync_clock"), 0, "cut after {cut}");
        }
        settle(&mut desktop, &mut phone);
        assert_eq!(count(&desktop, "SELECT count(*) FROM transactions WHERE account_id = 1 AND name = 'R payment'"), 1, "cut after {cut}");
    }
}

#[test]
fn a_sync_that_only_one_side_finished_is_finished_by_the_next() {
    for desktop_applied in [true, false] {
        let (mut desktop, mut phone) = deletion_meets_rows();
        // one device got everything the other sent and applied it; the other never did
        if desktop_applied {
            let incoming = rows_to(&phone, &desktop);
            engine::apply(&mut desktop, &incoming, now(), None).unwrap();
        } else {
            let incoming = rows_to(&desktop, &phone);
            engine::apply(&mut phone, &incoming, now(), None).unwrap();
        }
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT count(*) FROM accounts WHERE name = 'Cash'"), 0);
            assert_eq!(count(conn, "SELECT count(*) FROM transfers"), 2);
        }
    }
}

// --- a phone of the version before accounts ---------------------------------------------------------------------------------

#[test]
fn a_device_that_speaks_protocol_8_is_told_to_update_whichever_of_the_two_it_is() {
    let (a, b) = (device("EUR"), device("EUR"));
    let new = engine::hello(&a, "desktop", "0.1.0", now()).unwrap();
    let mut old = engine::hello(&b, "phone", "0.1.0", now()).unwrap();
    old.protocol = 8;
    assert!(matches!(engine::check_hello(&new, &old, now()), Err(SyncError::ProtocolMismatch)));
    assert!(matches!(engine::check_hello(&old, &new, now()), Err(SyncError::ProtocolMismatch)));
    // the version of the schema is its own check, and comes second: accounts are a migration too
    assert_eq!(new.schema, crate::migrate::latest_version());
}

#[test]
fn the_unsent_main_account_is_never_in_what_a_first_copy_sends() {
    // a first copy to a phone that is empty: its main account is the same row, so it is not sent, and the sentinel stamp stays
    let mut desktop = device("EUR");
    let mut phone = new_phone(&desktop);
    category(&desktop, "Food", CategoryType::Expense);
    spend(&desktop, id_of(&desktop, "categories", "Food"), "Coffee", "3.50");
    let outcome = settle(&mut desktop, &mut phone);
    assert!(!outcome.compared.contains(&Table::Accounts));
    assert_eq!(text(&phone, "SELECT stamp FROM accounts"), UNSET_STAMP);
    assert_eq!(count(&phone, "SELECT count(*) FROM transactions WHERE account_id = 1"), 1, "in the main account on both");
}

// --- a row deleted before its account -----------------------------------------------------------------------------------

/// The row named R in Cash that the three tests below delete and edit, one kind at a time.
fn make_r(conn: &Connection, kind: &str) {
    let cash = account_id(conn, "Cash");
    match kind {
        "transactions" => {
            spend_in(conn, "R", cash);
        }
        "recurring_transactions" => {
            bill_in(conn, "R", cash);
        }
        "ledger_entries" => {
            lend_in(conn, "R", cash);
        }
        _ => {
            move_money(conn, cash, account_id(conn, "Savings"), "R");
        }
    }
}

fn r_id(conn: &Connection, kind: &str) -> i64 {
    let column = if kind == "transactions" || kind == "recurring_transactions" { "name" } else { "note" };
    conn.query_row(&format!("SELECT id FROM {kind} WHERE {column} = 'R'"), [], |r| r.get(0)).unwrap()
}

fn delete_r(conn: &Connection, kind: &str) {
    let id = r_id(conn, kind);
    match kind {
        "transactions" => transactions::delete(conn, id).unwrap(),
        "recurring_transactions" => recurring::delete(conn, id).unwrap(),
        "ledger_entries" => ledger::delete(conn, id).unwrap(),
        _ => transfers::delete(conn, id).unwrap(),
    }
}

/// Another amount, so the edited row differs from the one that was synced.
fn edit_r(conn: &Connection, kind: &str) {
    let id = r_id(conn, kind);
    match kind {
        "transactions" => drop(transactions::update(conn, id, &TransactionPatch { amount: Some("6.00".into()), ..Default::default() }).unwrap()),
        "recurring_transactions" => drop(recurring::update(conn, id, &recurring::RecurringPatch { amount: Some(Some("11.00".into())), ..Default::default() }).unwrap()),
        "ledger_entries" => drop(ledger::update(conn, id, &ledger::EntryPatch { amount: Some("8.00".into()), ..Default::default() }).unwrap()),
        _ => drop(transfers::update(conn, id, &amount_patch("11.00")).unwrap()),
    }
}

#[test]
fn a_row_deleted_on_one_device_before_its_account_is_deleted_stays_deleted_on_the_other_device() {
    // The other device holds the row in the account. Its own deletion and the account's arrive together: the row must go first, or the
    // account's deletion would give it a stamp newer than its own deletion and it would live on, on one device only.
    for kind in ["transactions", "recurring_transactions", "ledger_entries", "transfers"] {
        for deleter_is_desktop in [true, false] {
            let (mut desktop, mut phone) = met_with_accounts();
            make_r(&desktop, kind);
            settle(&mut desktop, &mut phone);
            pause();
            let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
            let r = text(&a, &format!("SELECT sync_id FROM {kind}"));
            delete_r(&a, kind);
            pause();
            accounts::delete(&a, account_id(&a, "Cash")).unwrap();
            settle(&mut a, &mut b);
            for conn in [&a, &b] {
                assert_eq!(count(conn, &format!("SELECT count(*) FROM {kind}")), 0, "{kind}: gone on both");
                assert!(marker_of(conn, &r).is_some(), "{kind}");
                assert_eq!(count(conn, "SELECT count(*) FROM accounts WHERE name = 'Cash'"), 0);
            }
        }
    }
}

#[test]
fn a_row_edited_on_the_other_device_after_it_was_deleted_survives_in_the_main_account_whatever_it_did_to_the_account() {
    for kind in ["transactions", "recurring_transactions", "ledger_entries", "transfers"] {
        for edit_before_the_account_goes in [true, false] {
            let (mut desktop, mut phone) = met_with_accounts();
            make_r(&desktop, kind);
            settle(&mut desktop, &mut phone);
            pause();
            let r = text(&desktop, &format!("SELECT sync_id FROM {kind}"));
            let cash = account_sync_id(&desktop, "Cash");
            delete_r(&desktop, kind);
            pause();
            if edit_before_the_account_goes {
                edit_r(&phone, kind);
                pause();
                accounts::delete(&desktop, account_id(&desktop, "Cash")).unwrap();
            } else {
                accounts::delete(&desktop, account_id(&desktop, "Cash")).unwrap();
                pause();
                edit_r(&phone, kind);
            }
            let (edited_at, account_deleted_at) = (text(&phone, &format!("SELECT stamp FROM {kind}")), marker_of(&desktop, &cash).unwrap());
            settle(&mut desktop, &mut phone);
            for conn in [&desktop, &phone] {
                assert_eq!(count(conn, &format!("SELECT count(*) FROM {kind}")), 1, "{kind}: the edit is newer than the deletion of the row");
                assert_eq!(marker_of(conn, &r), None);
                assert_eq!(text(conn, &format!("SELECT stamp FROM {kind}")), later_of(&edited_at, &account_deleted_at), "{kind}: the later of the edit and the deletion of the account");
                let in_main = match kind {
                    "transfers" => "SELECT count(*) FROM transfers WHERE from_account_id = 1".to_string(),
                    _ => format!("SELECT count(*) FROM {kind} WHERE account_id = 1"),
                };
                assert_eq!(count(conn, &in_main), 1, "{kind}: in the main account");
            }
        }
    }
}

// --- a change of currency ---------------------------------------------------------------------------------------------------

#[test]
fn a_change_of_currency_converts_opening_balances_and_transfers_on_both_devices_without_sending_them() {
    let (mut desktop, mut phone) = met_with_accounts();
    let (cash, savings) = (account_id(&desktop, "Cash"), account_id(&desktop, "Savings"));
    accounts::update(&desktop, cash, &AccountPatch { opening_balance: Some("-100.50".into()), ..Default::default() }, TODAY).unwrap();
    accounts::update(&desktop, savings, &AccountPatch { opening_balance: Some("1000.00".into()), ..Default::default() }, TODAY).unwrap();
    move_money(&desktop, cash, savings, "ten");
    transfers::create(&desktop, &NewTransfer { from_account_id: savings.to_string(), to_account_id: cash.to_string(), amount: "0.01".into(), date: "2026-03-03".into(), note: Some("one cent".into()), to_amount: None }).unwrap();
    settle(&mut desktop, &mut phone);
    pause();
    currency::change(&mut desktop, "USD", Some("0.001"), TODAY).unwrap();
    let outcome = settle(&mut desktop, &mut phone);
    assert!(!outcome.compared.contains(&Table::Accounts) && !outcome.compared.contains(&Table::Transfers), "both devices converted them alike: {:?}", outcome.compared);
    assert!(outcome.first.is_empty() && outcome.second.is_empty(), "no row of either table travelled: {outcome:?}");
    for conn in [&desktop, &phone] {
        assert_eq!(text(conn, "SELECT currency FROM settings"), "USD");
        assert_eq!(count(conn, "SELECT opening_balance_minor FROM accounts WHERE name = 'Cash'"), -10, "half away from zero");
        assert_eq!(count(conn, "SELECT opening_balance_minor FROM accounts WHERE name = 'Savings'"), 100);
        assert_eq!(count(conn, "SELECT amount_minor FROM transfers WHERE note = 'ten'"), 1);
        assert_eq!(count(conn, "SELECT amount_minor FROM transfers WHERE note = 'one cent'"), 1, "a transfer stays at least one cent");
    }
}

#[test]
fn a_change_of_currency_converts_virtual_balances_on_both_devices_without_sending_them() {
    // (rate, what -100.50 on Cash, 1000.00 on Savings and 250.00 on the main account become, in minor units)
    for (rate, expected) in [("0.001", [-10, 100, 25]), ("1.08", [-10_854, 108_000, 27_000]), ("160", [-1_608_000, 16_000_000, 4_000_000])] {
        let (mut desktop, mut phone) = met_with_accounts();
        let (cash, savings) = (account_id(&desktop, "Cash"), account_id(&desktop, "Savings"));
        for (id, virtual_balance) in [(cash, "-100.50"), (savings, "1000.00"), (MAIN_ID, "250.00")] {
            accounts::update(&desktop, id, &AccountPatch { virtual_balance: Some(virtual_balance.into()), ..Default::default() }, TODAY).unwrap();
        }
        settle(&mut desktop, &mut phone);
        pause();
        currency::change(&mut desktop, "USD", Some(rate), TODAY).unwrap();
        let outcome = settle(&mut desktop, &mut phone);
        assert!(!outcome.compared.contains(&Table::Accounts), "both devices converted them alike: {:?}", outcome.compared);
        assert!(outcome.first.is_empty() && outcome.second.is_empty(), "no row travelled: {outcome:?}");
        for conn in [&desktop, &phone] {
            let held = [
                count(conn, "SELECT virtual_balance_minor FROM accounts WHERE name = 'Cash'"),
                count(conn, "SELECT virtual_balance_minor FROM accounts WHERE name = 'Savings'"),
                count(conn, "SELECT virtual_balance_minor FROM accounts WHERE id = 1"),
            ];
            assert_eq!(held, expected, "rate {rate}: half away from zero, on both devices");
        }
        // (the local ids differ between the devices, so the accounts are compared by name)
        let shown = |conn: &Connection| {
            let mut list: Vec<_> = accounts::list(conn, TODAY).unwrap().into_iter().map(|a| (a.name, a.balance, a.virtual_balance, a.available)).collect();
            list.sort();
            list
        };
        assert_eq!(shown(&phone), shown(&desktop), "rate {rate}: `available` too");
        assert_eq!(accounts::summary(&phone, TODAY).unwrap(), accounts::summary(&desktop, TODAY).unwrap(), "rate {rate}");
    }
}

#[test]
fn what_a_device_changed_in_accounts_and_transfers_counts_as_waiting_for_the_next_sync() {
    let conn = device("EUR");
    let before = peers::changes_since(&conn, 0).unwrap();
    let cash = open(&conn, "Cash");
    move_money(&conn, MAIN_ID, cash, "t");
    assert_eq!(peers::changes_since(&conn, 0).unwrap(), before + 2, "an account and a transfer");
    rename(&conn, MAIN_ID, "Wallet");
    assert_eq!(peers::changes_since(&conn, 0).unwrap(), before + 3, "the main account, once it is edited");
    transfers::delete(&conn, transfer_named(&conn, "t")).unwrap();
    accounts::delete(&conn, cash).unwrap();
    assert_eq!(peers::changes_since(&conn, 0).unwrap(), before + 3, "the two rows are gone, their markers are changes in their place");
}

// --- the details of an account: protocol 10 (docs/accounts-design.md, section 20) ------------------------------------------

/// What a person sees of the five details of an account: BIC, notes, group, whether net worth counts it, virtual balance.
type Details = (Option<String>, Option<String>, Option<String>, bool, String);

fn plain_details() -> Details {
    (None, None, None, true, "0.00".into())
}

fn details_of(conn: &Connection, sync_id: &str) -> Details {
    let id = count(conn, &format!("SELECT id FROM accounts WHERE sync_id = '{sync_id}'"));
    let a = accounts::get(conn, id, TODAY).unwrap();
    (a.bic, a.notes, a.group, a.include_in_net_worth, a.virtual_balance)
}

/// Each detail on its own: its name on the wire, the patch that sets it, what that makes the details, and the patch that clears it.
fn one_detail_at_a_time() -> Vec<(&'static str, AccountPatch, Details, AccountPatch)> {
    vec![
        (
            "bic",
            AccountPatch { bic: Some(Some("DEUTDEFF500".into())), ..Default::default() },
            (Some("DEUTDEFF500".into()), None, None, true, "0.00".into()),
            AccountPatch { bic: Some(None), ..Default::default() },
        ),
        (
            "notes",
            AccountPatch { notes: Some(Some("first line\nsecond line".into())), ..Default::default() },
            (None, Some("first line\nsecond line".into()), None, true, "0.00".into()),
            AccountPatch { notes: Some(None), ..Default::default() },
        ),
        (
            "group",
            AccountPatch { group: Some(Some("Everyday".into())), ..Default::default() },
            (None, None, Some("Everyday".into()), true, "0.00".into()),
            AccountPatch { group: Some(None), ..Default::default() },
        ),
        (
            "includeNetWorth",
            AccountPatch { include_in_net_worth: Some(false), ..Default::default() },
            (None, None, None, false, "0.00".into()),
            AccountPatch { include_in_net_worth: Some(true), ..Default::default() },
        ),
        (
            "virtualBalance",
            AccountPatch { virtual_balance: Some("-250.00".into()), ..Default::default() },
            (None, None, None, true, "-250.00".into()),
            AccountPatch { virtual_balance: Some("0.00".into()), ..Default::default() },
        ),
    ]
}

#[test]
fn each_detail_set_on_one_device_arrives_on_the_other_and_clearing_it_there_arrives_back() {
    for on_desktop in [true, false] {
        for (field, set, expected, clear) in one_detail_at_a_time() {
            let (mut desktop, mut phone) = met_with_accounts();
            let cash = account_sync_id(&desktop, "Cash");
            let (changed, other) = if on_desktop { (&desktop, &phone) } else { (&phone, &desktop) };
            let before = text(other, &format!("SELECT stamp FROM accounts WHERE sync_id = '{cash}'"));
            accounts::update(changed, account_id(changed, "Cash"), &set, TODAY).unwrap();
            let outcome = settle(&mut desktop, &mut phone);
            assert_eq!((outcome.compared.as_slice(), outcome.differences), ([Table::Accounts].as_slice(), false), "{field}: only the accounts were compared");
            for conn in [&desktop, &phone] {
                assert_eq!(details_of(conn, &cash), expected, "{field} set on the {}", if on_desktop { "desktop" } else { "phone" });
                assert_eq!(text(conn, &format!("SELECT name FROM accounts WHERE sync_id = '{cash}'")), "Cash", "{field}: the rest of the row is as it was");
            }
            let other = if on_desktop { &phone } else { &desktop };
            assert!(text(other, &format!("SELECT stamp FROM accounts WHERE sync_id = '{cash}'")) > before, "{field}: the stamp came with it");

            // the other device clears it again, and that arrives where it was set
            pause();
            accounts::update(other, account_id(other, "Cash"), &clear, TODAY).unwrap();
            settle(&mut desktop, &mut phone);
            for conn in [&desktop, &phone] {
                assert_eq!(details_of(conn, &cash), plain_details(), "{field} cleared on the other device");
                assert_eq!(count(conn, &format!("SELECT count(*) FROM accounts WHERE sync_id = '{cash}' AND bic IS NULL AND notes IS NULL AND group_name IS NULL")), 1, "cleared is null, not an empty text");
            }
        }
    }
}

#[test]
fn an_account_made_with_every_detail_comes_to_a_new_phone_whole_even_at_the_bounds() {
    for (virtual_balance, notes) in [("9999999999.99", "n".repeat(1000)), ("-9999999999.99", "é\nx\n".repeat(250)), ("0.00", "a".into())] {
        let mut desktop = device("EUR");
        let mut phone = new_phone(&desktop);
        let new = NewAccount {
            name: "Card".into(),
            kind: Kind::CreditCard,
            number: None,
            opening_balance: Some("-10.00".into()),
            opening_date: Some("2026-01-01".into()),
            bic: Some("B".repeat(20)),
            notes: Some(notes.clone()),
            group: Some("é".repeat(50)),
            include_in_net_worth: Some(false),
            virtual_balance: Some(virtual_balance.into()),
            currency: None, rate: None,
        };
        let made = accounts::create(&desktop, &new, TODAY).unwrap();
        assert_eq!(made.notes.as_deref(), Some(notes.trim()));
        settle(&mut desktop, &mut phone);
        let sent = details_of(&desktop, &account_sync_id(&desktop, "Card"));
        assert_eq!(sent, (Some("B".repeat(20)), Some(notes.trim().to_string()), Some("é".repeat(50)), false, virtual_balance.to_string()));
        assert_eq!(details_of(&phone, &account_sync_id(&phone, "Card")), sent, "virtual balance {virtual_balance}");
        assert_eq!(accounts::list(&phone, TODAY).unwrap(), accounts::list(&desktop, TODAY).unwrap(), "even `available` is the same");
    }
}

#[test]
fn a_newer_edit_of_only_a_detail_wins_over_an_older_rename_of_the_same_account_as_a_whole_row() {
    for (field, set, expected, _) in one_detail_at_a_time() {
        for detail_first in [true, false] {
            for detail_on_desktop in [true, false] {
                let (desktop, phone) = met_with_accounts();
                let (mut detailer, mut renamer) = if detail_on_desktop { (desktop, phone) } else { (phone, desktop) };
                let cash = account_sync_id(&detailer, "Cash");
                if detail_first {
                    accounts::update(&detailer, account_id(&detailer, "Cash"), &set, TODAY).unwrap();
                    pause();
                    rename(&renamer, account_id(&renamer, "Cash"), "Cash box");
                } else {
                    rename(&renamer, account_id(&renamer, "Cash"), "Cash box");
                    pause();
                    accounts::update(&detailer, account_id(&detailer, "Cash"), &set, TODAY).unwrap();
                }
                settle(&mut detailer, &mut renamer);
                // the later change decides the whole row, so the earlier one is gone from both: the rename is lost to the detail,
                // and the detail to the rename
                let (name, details) = if detail_first { ("Cash box", plain_details()) } else { ("Cash", expected.clone()) };
                for conn in [&detailer, &renamer] {
                    assert_eq!(text(conn, &format!("SELECT name FROM accounts WHERE sync_id = '{cash}'")), name, "{field}, detail first: {detail_first}");
                    assert_eq!(details_of(conn, &cash), details, "{field}, detail first: {detail_first}");
                }
            }
        }
    }
}

#[test]
fn both_devices_changing_the_details_of_one_account_is_an_ordinary_conflict_with_the_facts_to_choose_from() {
    for (choice, want_desktop) in [(engine::Choice::Mine, true), (engine::Choice::Theirs, false)] {
        let (mut desktop, mut phone) = met_with_accounts();
        let cash = account_sync_id(&desktop, "Cash");
        let met = now();
        pause();
        accounts::update(
            &desktop,
            account_id(&desktop, "Cash"),
            &AccountPatch {
                bic: Some(Some("DESKTOP".into())),
                notes: Some(Some("from the desktop".into())),
                group: Some(Some("Everyday".into())),
                include_in_net_worth: Some(false),
                virtual_balance: Some("1000.00".into()),
                ..Default::default()
            },
            TODAY,
        )
        .unwrap();
        accounts::update(
            &phone,
            account_id(&phone, "Cash"),
            &AccountPatch {
                bic: Some(Some("PHONE".into())),
                notes: Some(Some("from the phone\nin two lines".into())),
                group: Some(Some("Savings".into())),
                virtual_balance: Some("-5.50".into()),
                ..Default::default()
            },
            TODAY,
        )
        .unwrap();
        let (conflicts, views) = conflicts_between(&desktop, &phone, met);
        assert_eq!(conflicts.len(), 1);
        assert_eq!((conflicts[0].table, conflicts[0].id.as_str()), (Table::Accounts, cash.as_str()));
        assert_eq!(views[0].title, "Cash");
        assert_eq!(
            fields(&views, Table::Accounts),
            texts(&[
                ("bic", "DESKTOP", "PHONE"),
                ("notes", "from the desktop", "from the phone\nin two lines"),
                ("group", "Everyday", "Savings"),
                ("includeNetWorth", "no", "yes"),
                ("virtualBalance", "1000.00", "-5.50"),
            ]),
            "the five facts, in this order, and nothing else"
        );
        engine::decide(&mut desktop, &conflicts, &[choice]).unwrap();
        settle(&mut desktop, &mut phone);
        let expected: Details = if want_desktop {
            (Some("DESKTOP".into()), Some("from the desktop".into()), Some("Everyday".into()), false, "1000.00".into())
        } else {
            (Some("PHONE".into()), Some("from the phone\nin two lines".into()), Some("Savings".into()), true, "-5.50".into())
        };
        for conn in [&desktop, &phone] {
            assert_eq!(details_of(conn, &cash), expected, "{choice:?}");
        }
    }
}

#[test]
fn a_conflict_lists_only_the_details_that_differ_and_a_cleared_one_as_nothing() {
    let (desktop, phone) = met_with_accounts();
    let cash = account_id(&desktop, "Cash");
    // the same on both ...
    for conn in [&desktop, &phone] {
        accounts::update(conn, account_id(conn, "Cash"), &AccountPatch { notes: Some(Some("shared".into())), group: Some(Some("Bank".into())), ..Default::default() }, TODAY).unwrap();
    }
    let (mut desktop, mut phone) = (desktop, phone);
    settle(&mut desktop, &mut phone);
    let met = now();
    pause();
    // ... then each clears or changes one thing, and the facts are those two only
    accounts::update(&desktop, cash, &AccountPatch { group: Some(None), include_in_net_worth: Some(false), ..Default::default() }, TODAY).unwrap();
    accounts::update(&phone, account_id(&phone, "Cash"), &AccountPatch { bic: Some(Some("PHONE".into())), virtual_balance: Some("0.01".into()), ..Default::default() }, TODAY).unwrap();
    let (conflicts, views) = conflicts_between(&desktop, &phone, met);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(
        fields(&views, Table::Accounts),
        texts(&[("bic", "", "PHONE"), ("group", "", "Bank"), ("includeNetWorth", "no", "yes"), ("virtualBalance", "0.00", "0.01")]),
        "nothing is shown for a cleared text, and the notes that are the same are not listed"
    );
    engine::decide(&mut desktop, &conflicts, &[engine::Choice::Mine]).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(details_of(conn, &account_sync_id(conn, "Cash")), (None, Some("shared".into()), None, false, "0.00".into()));
    }
}

#[test]
fn the_main_account_with_details_arrives_on_the_other_device_and_both_stay_one_account() {
    for on_desktop in [true, false] {
        let (mut desktop, mut phone) = paired();
        let (changed, other) = if on_desktop { (&desktop, &phone) } else { (&phone, &desktop) };
        assert_eq!(text(other, "SELECT stamp FROM accounts"), UNSET_STAMP);
        pause();
        accounts::update(
            changed,
            MAIN_ID,
            &AccountPatch {
                bic: Some(Some("COBADEFFXXX".into())),
                notes: Some(Some("where the salary lands\nmain bank".into())),
                group: Some(Some("Everyday".into())),
                include_in_net_worth: Some(false),
                virtual_balance: Some("500.00".into()),
                ..Default::default()
            },
            TODAY,
        )
        .unwrap();
        assert!(text(changed, "SELECT stamp FROM accounts").as_str() > UNSET_STAMP, "a change of the details alone gives it a real stamp");
        let outcome = settle(&mut desktop, &mut phone);
        assert_eq!(outcome.compared, [Table::Accounts]);
        let expected: Details = (Some("COBADEFFXXX".into()), Some("where the salary lands\nmain bank".into()), Some("Everyday".into()), false, "500.00".into());
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT count(*) FROM accounts"), 1, "one main account, not two");
            assert_eq!((count(conn, "SELECT id FROM accounts"), text(conn, "SELECT sync_id FROM accounts")), (1, MAIN_SYNC_ID.to_string()));
            assert_eq!(details_of(conn, MAIN_SYNC_ID), expected);
            assert_eq!(text(conn, "SELECT name FROM accounts"), "Main account", "the rest of the row is as it was");
            assert_eq!(count(conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'accounts'"), 0, "no deletion of an account");
        }
        // the other device puts the main account back to its defaults, and that arrives too
        pause();
        let other = if on_desktop { &phone } else { &desktop };
        accounts::update(
            other,
            MAIN_ID,
            &AccountPatch { bic: Some(None), notes: Some(None), group: Some(None), include_in_net_worth: Some(true), virtual_balance: Some("0.00".into()), ..Default::default() },
            TODAY,
        )
        .unwrap();
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT count(*) FROM accounts"), 1);
            assert_eq!(details_of(conn, MAIN_SYNC_ID), plain_details());
        }
    }
}

#[test]
fn the_same_details_set_on_both_devices_before_they_meet_make_no_conflict_and_one_account() {
    let (mut desktop, mut phone) = paired();
    let met = now();
    pause();
    for conn in [&desktop, &phone] {
        accounts::update(conn, MAIN_ID, &AccountPatch { group: Some(Some("Everyday".into())), virtual_balance: Some("10.00".into()), ..Default::default() }, TODAY).unwrap();
    }
    // two edits of one row that came out the same are still two edits: the review shows nothing to choose between
    let (conflicts, views) = conflicts_between(&desktop, &phone, met);
    assert_eq!(conflicts.len(), 1);
    assert!(fields(&views, Table::Accounts).is_empty(), "no fact differs");
    engine::decide(&mut desktop, &conflicts, &[engine::Choice::Theirs]).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(details_of(conn, MAIN_SYNC_ID), (None, None, Some("Everyday".into()), true, "10.00".into()));
    }
}

#[test]
fn leaving_an_account_out_of_net_worth_on_one_device_gives_the_same_summary_on_the_other() {
    let (mut desktop, mut phone) = met_with_accounts();
    for (name, opening) in [("Cash", "50.00"), ("Savings", "200.00")] {
        accounts::update(&desktop, account_id(&desktop, name), &AccountPatch { opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), ..Default::default() }, TODAY).unwrap();
    }
    settle(&mut desktop, &mut phone);
    let summary = |conn: &Connection| accounts::summary(conn, TODAY).unwrap();
    assert_eq!((summary(&desktop).in_accounts.as_str(), summary(&desktop).accounts), ("250.00", 3));
    assert_eq!(summary(&phone), summary(&desktop));

    // the phone leaves Savings out: both count 50.00, and both still count three accounts
    pause();
    accounts::update(&phone, account_id(&phone, "Savings"), &AccountPatch { include_in_net_worth: Some(false), ..Default::default() }, TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        let s = summary(conn);
        assert_eq!((s.in_accounts.as_str(), s.net_worth.as_str(), s.accounts), ("50.00", "50.00", 3));
        assert_eq!(accounts::get(conn, account_id(conn, "Savings"), TODAY).unwrap().balance, "200.00", "its balance did not change");
    }
    assert_eq!(summary(&phone), summary(&desktop));

    // an archived account that was left out stays out on both
    pause();
    accounts::update(&desktop, account_id(&desktop, "Cash"), &AccountPatch { include_in_net_worth: Some(false), ..Default::default() }, TODAY).unwrap();
    accounts::archive(&desktop, account_id(&desktop, "Cash"), TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    assert_eq!(summary(&phone), summary(&desktop));
    assert_eq!(summary(&phone).in_accounts, "0.00");

    // with every account left out, net worth is what friends owe and nothing else; the main account too
    pause();
    accounts::update(&desktop, MAIN_ID, &AccountPatch { include_in_net_worth: Some(false), ..Default::default() }, TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    assert_eq!((summary(&phone).in_accounts.as_str(), summary(&phone).accounts), ("0.00", 3));
    assert_eq!(summary(&phone), summary(&desktop));

    // and counting one again arrives as well
    pause();
    accounts::update(&phone, account_id(&phone, "Savings"), &AccountPatch { include_in_net_worth: Some(true), ..Default::default() }, TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    assert_eq!(summary(&desktop).in_accounts, "200.00");
    assert_eq!(summary(&phone), summary(&desktop));
}

#[test]
fn a_virtual_balance_travels_without_changing_any_balance_or_the_net_worth() {
    let (mut desktop, mut phone) = met_with_accounts();
    accounts::update(&desktop, account_id(&desktop, "Cash"), &AccountPatch { opening_balance: Some("-240.00".into()), opening_date: Some("2026-01-01".into()), ..Default::default() }, TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    let before = accounts::summary(&phone, TODAY).unwrap();
    pause();
    accounts::update(&desktop, account_id(&desktop, "Cash"), &AccountPatch { virtual_balance: Some("1000.00".into()), ..Default::default() }, TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    let cash = accounts::get(&phone, account_id(&phone, "Cash"), TODAY).unwrap();
    assert_eq!((cash.balance.as_str(), cash.virtual_balance.as_str(), cash.available.as_str()), ("-240.00", "1000.00", "760.00"));
    assert_eq!(accounts::summary(&phone, TODAY).unwrap(), before);
    assert_eq!(accounts_shown(&phone), accounts_shown(&desktop));
}

/// The row of an account as the wire carries it, read back out of what a device would send.
fn wire_row_of(conn: &Connection, name: &str) -> serde_json::Value {
    let id = account_sync_id(conn, name);
    let line = snapshot(conn).unwrap().into_iter().find(|l| l.starts_with("accounts:") && l.contains(&id)).unwrap();
    serde_json::from_str(line.trim_start_matches("accounts:")).unwrap()
}

#[test]
fn what_the_screens_accept_as_a_detail_is_what_the_wire_accepts_and_the_other_way_round() {
    let long = |c: &str, n: usize| c.repeat(n);
    let candidates = [
        "".to_string(), " ".into(), "a".into(), " a".into(), "a ".into(), "a b".into(), long("x", 20), long("x", 21), long("é", 20), long("é", 21), long("x", 50), long("x", 51),
        long("x", 1000), long("x", 1001), "a\tb".into(), "a\nb".into(), "a\r\nb".into(), "a\rb".into(), "a\n".into(), "\na".into(), "a\u{7}b".into(), "a\u{7f}b".into(),
        "a\u{85}b".into(), "\u{85}a".into(), "a\u{a0}".into(), "\u{a0}a".into(), "a\u{200b}b".into(), "=1+1".into(), "line one\n\nline three".into(),
    ];
    // an account as a device sends it, to put each candidate into
    let base = {
        let device = device("EUR");
        open(&device, "A");
        wire_row_of(&device, "A")
    };
    let mut kept_by_the_screens = 0;
    for candidate in &candidates {
        for field in ["bic", "notes", "group"] {
            let device = device("EUR");
            let mut new = NewAccount { name: "A".into(), kind: Kind::Cash, number: None, opening_balance: None, opening_date: Some("2026-01-01".into()), bic: None, notes: None, group: None, include_in_net_worth: None, virtual_balance: None, currency: None, rate: None };
            match field {
                "bic" => new.bic = Some(candidate.clone()),
                "notes" => new.notes = Some(candidate.clone()),
                _ => new.group = Some(candidate.clone()),
            }
            let mut row = base.clone();
            row[field] = serde_json::json!(candidate);
            let wire_accepts = wire::parse_row(Table::Accounts, &row).is_ok();
            match accounts::create(&device, &new, TODAY) {
                Ok(made) => {
                    // what the screens keep is a valid row: what they trimmed or normalised is already done
                    let kept = match field {
                        "bic" => made.bic,
                        "notes" => made.notes,
                        _ => made.group,
                    };
                    let sent = wire_row_of(&device, "A");
                    assert_eq!(sent[field], serde_json::json!(kept), "{field} = {candidate:?}");
                    assert!(wire::parse_row(Table::Accounts, &sent).is_ok(), "{field} = {candidate:?} is kept by the screens and refused by the wire");
                    // and what the wire accepts is kept as it is, not trimmed or repaired
                    if wire_accepts {
                        assert_eq!(kept.as_deref(), Some(candidate.as_str()), "{field} = {candidate:?}");
                    }
                    kept_by_the_screens += 1;
                }
                Err(e) => {
                    assert_eq!(e, crate::vault::AppError::InvalidInput, "{field} = {candidate:?}");
                    assert!(!wire_accepts, "{field} = {candidate:?} is accepted by the wire and refused by the screens");
                    assert_eq!(count(&device, "SELECT count(*) FROM accounts"), 1, "a refused account is not made");
                }
            }
        }
    }
    assert!(kept_by_the_screens > 40, "the list is not all refusals: {kept_by_the_screens}");
}

#[test]
fn a_row_in_the_shape_of_protocol_9_is_refused_by_the_collector_and_nothing_is_applied() {
    let (_desktop, phone) = paired();
    let before = snapshot(&phone).unwrap();
    let mut old = wire_row_of(&phone, "Main account");
    for key in ["bic", "notes", "group", "includeNetWorth", "virtualBalanceMinor"] {
        old.as_object_mut().unwrap().remove(key);
    }
    old["name"] = serde_json::json!("Renamed by an old phone");
    old["stamp"] = serde_json::json!(newer_than_everything());
    let message = wire::RowsMessage { table: Table::Accounts, rows: vec![old], deleted: vec![], last: true };
    assert!(RowsCollector::new(vec![Table::Accounts]).push(message).is_err(), "the two required details are missing");
    assert_eq!(snapshot(&phone).unwrap(), before);
}

#[test]
fn a_device_that_speaks_protocol_9_is_told_to_update_whichever_of_the_two_it_is() {
    let (a, b) = (device("EUR"), device("EUR"));
    let new = engine::hello(&a, "desktop", "0.1.0", now()).unwrap();
    let mut old = engine::hello(&b, "phone", "0.1.0", now()).unwrap();
    assert_eq!(new.protocol, 11);
    old.protocol = 9;
    assert!(matches!(engine::check_hello(&new, &old, now()), Err(SyncError::ProtocolMismatch)));
    assert!(matches!(engine::check_hello(&old, &new, now()), Err(SyncError::ProtocolMismatch)));
    // the schema is the other check: the details are a migration too, and a device at schema 18 is told to update by that one
    assert_eq!(new.schema, crate::migrate::latest_version());
}

/// A desktop that changed all five details of Cash and a phone that has not heard of it.
fn details_changed_on_the_desktop() -> (Connection, Connection) {
    let (desktop, phone) = met_with_accounts();
    accounts::update(
        &desktop,
        account_id(&desktop, "Cash"),
        &AccountPatch { bic: Some(Some("DESKTOP".into())), notes: Some(Some("a\nb".into())), group: Some(Some("G".into())), include_in_net_worth: Some(false), virtual_balance: Some("9.99".into()), ..Default::default() },
        TODAY,
    )
    .unwrap();
    (desktop, phone)
}

#[test]
fn a_sync_cut_after_any_message_leaves_the_details_of_an_account_as_they_were_and_the_next_one_finishes_it() {
    let total = {
        let (mut desktop, mut phone) = details_changed_on_the_desktop();
        sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap().messages
    };
    assert!(total > 6, "{total} messages");
    for cut in 0..total {
        let (mut desktop, mut phone) = details_changed_on_the_desktop();
        let cash = account_sync_id(&desktop, "Cash");
        let before = (snapshot(&desktop).unwrap(), snapshot(&phone).unwrap());
        assert!(sync_cut_after(&mut desktop, &mut phone, TODAY, now(), cut).is_err(), "cut after {cut} of {total} messages");
        // never half of the details: the phone has the row it had, or the whole new one
        if cut < total - 2 {
            assert_eq!((snapshot(&desktop).unwrap(), snapshot(&phone).unwrap()), before, "cut after {cut} of {total} messages: nothing was applied");
            assert_eq!(details_of(&phone, &cash), plain_details());
        } else {
            assert_same(&desktop, &phone, &format!("cut after {cut} of {total}: both had applied it"));
        }
        settle(&mut desktop, &mut phone);
        assert_eq!(details_of(&phone, &cash), (Some("DESKTOP".into()), Some("a\nb".into()), Some("G".into()), false, "9.99".into()), "cut after {cut}");
    }
}

// --- a currency per account: protocol 11 (docs/accounts-design.md, section 21) -------------------------------------------------

/// An account in another currency, at `rate` of its units to one euro, with an opening balance of its own in that currency (dated the first
/// of January). Returns its local id.
fn open_foreign(conn: &Connection, name: &str, code: &str, rate: &str, opening: &str) -> i64 {
    accounts::create(
        conn,
        &NewAccount {
            name: name.into(),
            kind: Kind::Checking,
            number: None,
            opening_balance: Some(opening.into()),
            opening_date: Some("2026-01-01".into()),
            bic: None,
            notes: None,
            group: None,
            include_in_net_worth: None,
            virtual_balance: None,
            currency: Some(code.into()),
            rate: Some(rate.into()),
        },
        TODAY,
    )
    .unwrap()
    .id
}

/// A payment of 4.00 in the app's currency; in an account of another currency, `native` is what that account really paid.
fn spend_native(conn: &Connection, name: &str, account: i64, native: Option<&str>) -> i64 {
    let food = id_of(conn, "categories", "Food");
    transactions::create(
        conn,
        &NewTransaction {
            name: name.into(),
            amount: "4.00".into(),
            payment_date: "2026-03-02".into(),
            category_id: food.to_string(),
            description: None,
            client_uuid: None,
            account_id: Some(account.to_string()),
            native_amount: native.map(str::to_string),
        },
    )
    .unwrap()
    .id
}

/// A transfer with what arrives, when the two accounts are in different currencies; `note` is its name.
fn move_between_currencies(conn: &Connection, from: i64, to: i64, amount: &str, arrives: Option<&str>, note: &str) -> i64 {
    transfers::create(
        conn,
        &NewTransfer { from_account_id: from.to_string(), to_account_id: to.to_string(), amount: amount.into(), date: "2026-03-03".into(), note: Some(note.into()), to_amount: arrives.map(str::to_string) },
    )
    .unwrap()
    .id
}

type InCurrencies = (String, String, String, String, Option<String>);

/// What a device shows of its accounts, by name (the local ids differ): balance, what it is worth in the app's currency, currency, rate.
fn accounts_in_currencies(conn: &Connection) -> Vec<InCurrencies> {
    let mut list: Vec<_> = accounts::list(conn, TODAY).unwrap().into_iter().map(|a| (a.name, a.balance, a.balance_in_main, a.currency, a.rate)).collect();
    list.sort();
    list
}

fn in_currencies(conn: &Connection, name: &str) -> InCurrencies {
    accounts_in_currencies(conn).into_iter().find(|a| a.0 == name).unwrap_or_else(|| panic!("no account {name}"))
}

/// What a payment really moved its account by, in the account's currency: nothing for an account in the app's currency.
fn native_of(conn: &Connection, name: &str) -> Option<i64> {
    conn.query_row("SELECT native_amount_minor FROM transactions WHERE name = ?1", [name], |r| r.get(0)).unwrap()
}

/// What a transfer brought to its second account, when the two were in different currencies.
fn arrives_of(conn: &Connection, note: &str) -> Option<i64> {
    conn.query_row("SELECT to_amount_minor FROM transfers WHERE note = ?1", [note], |r| r.get(0)).unwrap()
}

/// What a patch does, as the person says it, the patch (made where it is made, for the local ids), what it leaves the row in and the amount there.
type Step<P> = (&'static str, Box<dyn Fn(&Connection) -> P>, &'static str, Option<i64>);

fn who(on_desktop: bool) -> &'static str {
    if on_desktop {
        "desktop"
    } else {
        "phone"
    }
}

#[test]
fn a_foreign_account_with_its_payments_and_transfers_between_currencies_arrives_with_the_same_balances_and_net_worth() {
    for on_desktop in [true, false] {
        let (mut desktop, mut phone) = met_with_accounts();
        let made = if on_desktop { &desktop } else { &phone };
        category(made, "Salary", CategoryType::Income);
        let (cash, savings) = (account_id(made, "Cash"), account_id(made, "Savings"));
        let lei = open_foreign(made, "Lei account", "RON", "5.00", "100.00");
        spend_native(made, "Lei lunch", lei, Some("20.00"));
        spend_native(made, "Cash lunch", cash, None);
        let salary = id_of(made, "categories", "Salary");
        transactions::create(
            made,
            &NewTransaction { name: "Pay in lei".into(), amount: "100.00".into(), payment_date: "2026-03-01".into(), category_id: salary.to_string(), description: None, client_uuid: None, account_id: Some(lei.to_string()), native_amount: Some("500.00".into()) },
        )
        .unwrap();
        // a transfer in each direction between the euro and the lei, one between two euro accounts that has no second amount
        move_between_currencies(made, cash, lei, "10.00", Some("50.50"), "T cash to lei");
        move_between_currencies(made, lei, savings, "100.00", Some("19.00"), "T lei to savings");
        move_between_currencies(made, MAIN_ID, lei, "20.00", Some("100.00"), "T main to lei");
        move_between_currencies(made, lei, MAIN_ID, "50.00", Some("10.00"), "T lei to main");
        move_between_currencies(made, cash, savings, "5.00", None, "T cash to savings");
        let outcome = settle(&mut desktop, &mut phone);
        assert!(outcome.compared.contains(&Table::Accounts) && outcome.compared.contains(&Table::Transactions) && outcome.compared.contains(&Table::Transfers), "{:?}", outcome.compared);
        for conn in [&desktop, &phone] {
            let label = format!("made on the {}", who(on_desktop));
            // 100.00 + 500.00 - 20.00 + 50.50 - 100.00 + 100.00 - 50.00 lei, and at 5.00 that is 116.10 euro
            assert_eq!(in_currencies(conn, "Lei account"), ("Lei account".into(), "580.50".into(), "116.10".into(), "RON".into(), Some("5.00".into())), "{label}");
            assert_eq!(in_currencies(conn, "Cash").1, "-19.00", "{label}");
            assert_eq!(in_currencies(conn, "Savings").1, "24.00", "{label}: 19.00 from the lei and 5.00 from Cash");
            assert_eq!(in_currencies(conn, "Main account").1, "-10.00", "{label}");
            let summary = accounts::summary(conn, TODAY).unwrap();
            assert_eq!((summary.in_accounts.as_str(), summary.accounts), ("111.10", 4), "{label}: -10.00 - 19.00 + 24.00 + 116.10");
            assert_eq!([native_of(conn, "Lei lunch"), native_of(conn, "Pay in lei"), native_of(conn, "Cash lunch")], [Some(2000), Some(50_000), None], "{label}");
            assert_eq!(count(conn, "SELECT amount_minor FROM transactions WHERE name = 'Lei lunch'"), 400, "{label}: the amount in the app's currency is what budgets count");
            assert_eq!(count(conn, "SELECT count(*) FROM transactions WHERE name = 'Cash lunch' AND native_amount_minor IS NULL"), 1, "{label}: null, not zero");
            assert_eq!(
                [arrives_of(conn, "T cash to lei"), arrives_of(conn, "T lei to savings"), arrives_of(conn, "T main to lei"), arrives_of(conn, "T lei to main"), arrives_of(conn, "T cash to savings")],
                [Some(5050), Some(1900), Some(10_000), Some(1000), None],
                "{label}"
            );
            assert_eq!([count(conn, "SELECT amount_minor FROM transfers WHERE note = 'T lei to savings'"), count(conn, "SELECT amount_minor FROM transfers WHERE note = 'T main to lei'")], [10_000, 2000], "{label}");
        }
        assert_eq!(accounts_in_currencies(&desktop), accounts_in_currencies(&phone));
        assert_eq!(accounts::summary(&desktop, TODAY).unwrap(), accounts::summary(&phone, TODAY).unwrap());
    }
}

#[test]
fn a_foreign_account_made_with_the_largest_amounts_and_the_smallest_and_largest_rate_comes_to_a_new_phone_whole() {
    for (rate, opening, native, arrives) in [("0.00000001", "9999999999.99", "9999999999.99", "9999999999.99"), ("999999999.99999999", "-9999999999.99", "0.01", "0.01"), ("1", "0.00", "0.01", "5.00")] {
        let mut desktop = device("EUR");
        let mut phone = new_phone(&desktop);
        category(&desktop, "Food", CategoryType::Expense);
        let lei = open_foreign(&desktop, "Lei account", "RON", rate, opening);
        let food = id_of(&desktop, "categories", "Food");
        // a payment that really paid more than the amount in the app's currency, and one that paid the least a screen takes: both are in the rules
        transactions::create(
            &desktop,
            &NewTransaction { name: "Native".into(), amount: "0.01".into(), payment_date: "2026-03-02".into(), category_id: food.to_string(), description: None, client_uuid: None, account_id: Some(lei.to_string()), native_amount: Some(native.into()) },
        )
        .unwrap();
        move_between_currencies(&desktop, MAIN_ID, lei, "9999999999.99", Some(arrives), "T big");
        settle(&mut desktop, &mut phone);
        assert_eq!(in_currencies(&phone, "Lei account"), in_currencies(&desktop, "Lei account"), "rate {rate}");
        assert_eq!(text(&phone, "SELECT rate FROM accounts WHERE name = 'Lei account'"), rate);
        assert_eq!(count(&phone, "SELECT native_amount_minor FROM transactions WHERE name = 'Native'"), (native.replace('.', "")).parse::<i64>().unwrap(), "rate {rate}");
        assert_eq!(count(&phone, "SELECT to_amount_minor FROM transfers WHERE note = 'T big'"), arrives.replace('.', "").parse::<i64>().unwrap(), "rate {rate}");
        assert_eq!(accounts::summary(&phone, TODAY).unwrap(), accounts::summary(&desktop, TODAY).unwrap(), "rate {rate}");
    }
}

#[test]
fn a_new_rate_arrives_and_changes_what_a_foreign_account_is_worth_and_no_stored_amount() {
    for on_desktop in [true, false] {
        let (mut desktop, mut phone) = met_with_accounts();
        let lei = open_foreign(&desktop, "Lei account", "RON", "5.00", "100.00");
        spend_native(&desktop, "Lei lunch", lei, Some("20.00"));
        settle(&mut desktop, &mut phone);
        assert_eq!(in_currencies(&phone, "Lei account"), ("Lei account".into(), "80.00".into(), "16.00".into(), "RON".into(), Some("5.00".into())));
        pause();
        let (changed, other) = if on_desktop { (&desktop, &phone) } else { (&phone, &desktop) };
        let before = text(other, "SELECT stamp FROM accounts WHERE name = 'Lei account'");
        accounts::update(changed, account_id(changed, "Lei account"), &AccountPatch { rate: Some("4.00".into()), ..Default::default() }, TODAY).unwrap();
        let outcome = settle(&mut desktop, &mut phone);
        assert_eq!((outcome.compared.as_slice(), outcome.differences), ([Table::Accounts].as_slice(), false), "only the account travelled");
        for conn in [&desktop, &phone] {
            assert_eq!(in_currencies(conn, "Lei account"), ("Lei account".into(), "80.00".into(), "20.00".into(), "RON".into(), Some("4.00".into())), "changed on the {}", who(on_desktop));
            assert_eq!((native_of(conn, "Lei lunch"), count(conn, "SELECT amount_minor FROM transactions WHERE name = 'Lei lunch'")), (Some(2000), 400), "no stored amount moves with a rate");
            assert_eq!(accounts::summary(conn, TODAY).unwrap().in_accounts, "20.00");
        }
        let other = if on_desktop { &phone } else { &desktop };
        assert!(text(other, "SELECT stamp FROM accounts WHERE name = 'Lei account'") > before, "the stamp came with it");
        // and the other device sets it again, to the far end of what a rate can be
        pause();
        accounts::update(other, account_id(other, "Lei account"), &AccountPatch { rate: Some("160".into()), ..Default::default() }, TODAY).unwrap();
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(in_currencies(conn, "Lei account").2, "0.50", "80.00 at 160");
        }
    }
}

#[test]
fn a_newer_rename_wins_over_an_older_new_rate_as_a_whole_row_and_the_older_rate_is_not_merged_in() {
    let (mut desktop, mut phone) = met_with_accounts();
    open_foreign(&desktop, "Lei account", "RON", "5.00", "100.00");
    settle(&mut desktop, &mut phone);
    pause();
    accounts::update(&desktop, account_id(&desktop, "Lei account"), &AccountPatch { rate: Some("4.00".into()), ..Default::default() }, TODAY).unwrap();
    pause();
    rename(&phone, account_id(&phone, "Lei account"), "Lei savings");
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(in_currencies(conn, "Lei savings"), ("Lei savings".into(), "100.00".into(), "20.00".into(), "RON".into(), Some("5.00".into())), "the newer whole row, with the rate it had");
    }
}

#[test]
fn the_currency_of_an_empty_account_changes_on_either_device_and_arrives_with_its_rate_and_back_to_the_app_s_currency() {
    for first_on_desktop in [true, false] {
        let (mut desktop, mut phone) = met_with_accounts();
        open_foreign(&desktop, "Money", "RON", "5.00", "0.00");
        settle(&mut desktop, &mut phone);
        let steps = [
            (AccountPatch { currency: Some("usd".into()), rate: Some("1.10".into()), ..Default::default() }, "USD", Some("1.10")),
            // the app's own currency, in any case: no currency of its own, so no rate
            (AccountPatch { currency: Some("eur".into()), ..Default::default() }, "EUR", None),
            (AccountPatch { currency: Some("GBP".into()), rate: Some("0.85".into()), ..Default::default() }, "GBP", Some("0.85")),
            // a new code keeps the rate that is there
            (AccountPatch { currency: Some("RON".into()), ..Default::default() }, "RON", Some("0.85")),
        ];
        for (step, (patch, code, rate)) in steps.into_iter().enumerate() {
            pause();
            let on_desktop = first_on_desktop == (step % 2 == 0);
            let changed = if on_desktop { &desktop } else { &phone };
            accounts::update(changed, account_id(changed, "Money"), &patch, TODAY).unwrap();
            let outcome = settle(&mut desktop, &mut phone);
            assert_eq!(outcome.compared, [Table::Accounts], "step {step}");
            for conn in [&desktop, &phone] {
                let shown = in_currencies(conn, "Money");
                assert_eq!((shown.3.as_str(), shown.4.as_deref()), (code, rate), "step {step} made on the {}", who(on_desktop));
                // none for the app's currency: null in the table, not the code of it
                assert_eq!(count(conn, "SELECT count(*) FROM accounts WHERE name = 'Money' AND currency IS NULL AND rate IS NULL"), (code == "EUR") as i64, "step {step}");
            }
        }
    }
}

#[test]
fn a_payment_moved_between_a_main_and_a_foreign_account_on_one_device_arrives_with_its_native_amount_set_kept_changed_and_cleared() {
    for on_desktop in [true, false] {
        let (mut desktop, mut phone) = met_with_accounts();
        open_foreign(&desktop, "Lei account", "RON", "5.00", "0.00");
        open_foreign(&desktop, "Dollars", "USD", "1.10", "0.00");
        let cash = account_id(&desktop, "Cash");
        spend_in(&desktop, "Lunch", cash);
        settle(&mut desktop, &mut phone);
        // (what the patch does, the account the payment is then in, what it really paid there)
        let steps: Vec<Step<TransactionPatch>> = vec![
            ("main to foreign, with its native amount", Box::new(|c| TransactionPatch { account_id: Some(account_id(c, "Lei account").to_string()), native_amount: Some("20.00".into()), ..Default::default() }), "Lei account", Some(2000)),
            ("a new native amount, staying", Box::new(|_| TransactionPatch { native_amount: Some("21.50".into()), ..Default::default() }), "Lei account", Some(2150)),
            ("foreign to main: cleared", Box::new(|c| TransactionPatch { account_id: Some(account_id(c, "Cash").to_string()), ..Default::default() }), "Cash", None),
            ("main to foreign again", Box::new(|c| TransactionPatch { account_id: Some(account_id(c, "Lei account").to_string()), native_amount: Some("30.00".into()), ..Default::default() }), "Lei account", Some(3000)),
            ("foreign to foreign: kept", Box::new(|c| TransactionPatch { account_id: Some(account_id(c, "Dollars").to_string()), ..Default::default() }), "Dollars", Some(3000)),
            ("foreign to foreign, with a new one", Box::new(|c| TransactionPatch { account_id: Some(account_id(c, "Lei account").to_string()), native_amount: Some("1.00".into()), ..Default::default() }), "Lei account", Some(100)),
            ("foreign to the main account", Box::new(|_| TransactionPatch { account_id: Some(MAIN_ID.to_string()), ..Default::default() }), "Main account", None),
        ];
        for (step, (what, patch, account, native)) in steps.into_iter().enumerate() {
            pause();
            // the two devices take turns, so each end meets the other's change to a row it made itself
            let changed = if (step % 2 == 0) == on_desktop { &desktop } else { &phone };
            transactions::update(changed, id_of(changed, "transactions", "Lunch"), &patch(changed)).unwrap();
            let outcome = settle(&mut desktop, &mut phone);
            assert_eq!((outcome.compared.as_slice(), outcome.differences), ([Table::Transactions].as_slice(), false), "{what}");
            for conn in [&desktop, &phone] {
                assert_eq!(text(conn, "SELECT a.name FROM transactions t JOIN accounts a ON a.id = t.account_id WHERE t.name = 'Lunch'"), account, "{what}");
                assert_eq!(native_of(conn, "Lunch"), native, "{what}");
                assert_eq!(count(conn, "SELECT amount_minor FROM transactions WHERE name = 'Lunch'"), 500, "{what}: the amount in the app's currency never moves");
            }
            assert_eq!(accounts_in_currencies(&desktop), accounts_in_currencies(&phone), "{what}");
        }
    }
}

#[test]
fn a_transfer_whose_sides_change_currency_arrives_with_its_second_amount_set_and_cleared() {
    let (mut desktop, mut phone) = met_with_accounts();
    open_foreign(&desktop, "Lei account", "RON", "5.00", "0.00");
    let (cash, lei) = (account_id(&desktop, "Cash"), account_id(&desktop, "Lei account"));
    move_between_currencies(&desktop, cash, lei, "10.00", Some("50.00"), "T");
    settle(&mut desktop, &mut phone);
    // (the patch on the phone, then on the desktop, and what arrives afterwards: the second side and the amount that arrives there)
    let steps: Vec<Step<TransferPatch>> = vec![
        ("a new amount that arrives", Box::new(|_| TransferPatch { to_amount: Some("52.50".into()), ..Default::default() }), "Lei account", Some(5250)),
        ("the second side is now in the app's currency: cleared", Box::new(|c| TransferPatch { to_account_id: Some(account_id(c, "Savings").to_string()), ..Default::default() }), "Savings", None),
        ("a side in a currency again: required", Box::new(|c| TransferPatch { to_account_id: Some(account_id(c, "Lei account").to_string()), to_amount: Some("49.00".into()), ..Default::default() }), "Lei account", Some(4900)),
        ("only the first side changes: kept", Box::new(|_| TransferPatch { from_account_id: Some(MAIN_ID.to_string()), ..Default::default() }), "Lei account", Some(4900)),
    ];
    for (step, (what, patch, to, arrives)) in steps.into_iter().enumerate() {
        pause();
        let on_phone = step % 2 == 0;
        let changed = if on_phone { &phone } else { &desktop };
        transfers::update(changed, transfer_named(changed, "T"), &patch(changed)).unwrap();
        let outcome = settle(&mut desktop, &mut phone);
        assert_eq!((outcome.compared.as_slice(), outcome.differences), ([Table::Transfers].as_slice(), false), "{what}");
        for conn in [&desktop, &phone] {
            assert_eq!(text(conn, "SELECT t.name FROM transfers x JOIN accounts t ON t.id = x.to_account_id WHERE x.note = 'T'"), to, "{what}");
            assert_eq!(arrives_of(conn, "T"), arrives, "{what}");
        }
        assert_eq!(accounts_in_currencies(&desktop), accounts_in_currencies(&phone), "{what}");
    }
}

#[test]
fn both_devices_changing_the_currency_and_the_rate_of_an_empty_account_is_a_conflict_that_lists_both_facts() {
    for (choice, expected) in [(engine::Choice::Mine, ("GBP", Some("0.85"))), (engine::Choice::Theirs, ("USD", Some("1.20")))] {
        let (mut desktop, mut phone) = met_with_accounts();
        open_foreign(&desktop, "Money", "USD", "1.10", "0.00");
        settle(&mut desktop, &mut phone);
        let met = now();
        pause();
        accounts::update(&desktop, account_id(&desktop, "Money"), &AccountPatch { currency: Some("GBP".into()), rate: Some("0.85".into()), ..Default::default() }, TODAY).unwrap();
        accounts::update(&phone, account_id(&phone, "Money"), &AccountPatch { rate: Some("1.20".into()), ..Default::default() }, TODAY).unwrap();
        let (conflicts, views) = conflicts_between(&desktop, &phone, met);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(views[0].title, "Money");
        assert_eq!(fields(&views, Table::Accounts), texts(&[("currency", "GBP", "USD"), ("rate", "0.85", "1.20")]), "the currency and the rate, and nothing else");
        engine::decide(&mut desktop, &conflicts, &[choice]).unwrap();
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            let shown = in_currencies(conn, "Money");
            assert_eq!((shown.3.as_str(), shown.4.as_deref()), expected, "{choice:?}");
        }
    }
    // one device sets the app's currency, which clears both: they show as nothing
    let (mut desktop, mut phone) = met_with_accounts();
    open_foreign(&desktop, "Money", "USD", "1.10", "0.00");
    settle(&mut desktop, &mut phone);
    let met = now();
    pause();
    accounts::update(&desktop, account_id(&desktop, "Money"), &AccountPatch { currency: Some("EUR".into()), ..Default::default() }, TODAY).unwrap();
    accounts::update(&phone, account_id(&phone, "Money"), &AccountPatch { rate: Some("1.20".into()), ..Default::default() }, TODAY).unwrap();
    let (_, views) = conflicts_between(&desktop, &phone, met);
    assert_eq!(fields(&views, Table::Accounts), texts(&[("currency", "", "USD"), ("rate", "", "1.20")]));
}

#[test]
fn both_devices_changing_the_native_amount_of_a_payment_and_what_arrives_of_a_transfer_is_a_conflict_that_lists_them() {
    let world = || {
        let (mut desktop, mut phone) = met_with_accounts();
        let lei = open_foreign(&desktop, "Lei account", "RON", "5.00", "0.00");
        let cash = account_id(&desktop, "Cash");
        spend_native(&desktop, "Lei lunch", lei, Some("20.00"));
        move_between_currencies(&desktop, cash, lei, "10.00", Some("50.00"), "T cash to lei");
        settle(&mut desktop, &mut phone);
        (desktop, phone)
    };
    let (mut desktop, mut phone) = world();
    let met = now();
    pause();
    transactions::update(&desktop, id_of(&desktop, "transactions", "Lei lunch"), &TransactionPatch { native_amount: Some("21.00".into()), ..Default::default() }).unwrap();
    transfers::update(&desktop, transfer_named(&desktop, "T cash to lei"), &TransferPatch { to_amount: Some("51.00".into()), ..Default::default() }).unwrap();
    transactions::update(&phone, id_of(&phone, "transactions", "Lei lunch"), &TransactionPatch { native_amount: Some("22.00".into()), ..Default::default() }).unwrap();
    transfers::update(&phone, transfer_named(&phone, "T cash to lei"), &TransferPatch { to_amount: Some("52.00".into()), ..Default::default() }).unwrap();
    let (conflicts, views) = conflicts_between(&desktop, &phone, met);
    assert_eq!(conflicts.iter().map(|c| c.table).collect::<Vec<_>>(), [Table::Transactions, Table::Transfers]);
    assert_eq!(fields(&views, Table::Transactions), texts(&[("nativeAmount", "21.00", "22.00")]), "only what differs");
    assert_eq!(fields(&views, Table::Transfers), texts(&[("toAmount", "51.00", "52.00")]));
    engine::decide(&mut desktop, &conflicts, &[engine::Choice::Theirs, engine::Choice::Mine]).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!((native_of(conn, "Lei lunch"), arrives_of(conn, "T cash to lei")), (Some(2200), Some(5100)));
    }

    // a native amount that one device cleared by moving the payment to the main account is nothing in the review, beside the account
    let (desktop, phone) = world();
    let met = now();
    pause();
    transactions::update(&desktop, id_of(&desktop, "transactions", "Lei lunch"), &TransactionPatch { account_id: Some(account_id(&desktop, "Cash").to_string()), ..Default::default() }).unwrap();
    transactions::update(&phone, id_of(&phone, "transactions", "Lei lunch"), &TransactionPatch { native_amount: Some("23.00".into()), ..Default::default() }).unwrap();
    let (_, views) = conflicts_between(&desktop, &phone, met);
    assert_eq!(fields(&views, Table::Transactions), texts(&[("nativeAmount", "", "23.00"), ("account", "Cash", "Lei account")]));
    // and a transfer made between two euro accounts has no second amount to show
    let (desktop, phone) = world();
    let met = now();
    pause();
    transfers::update(&desktop, transfer_named(&desktop, "T cash to lei"), &TransferPatch { to_account_id: Some(account_id(&desktop, "Savings").to_string()), ..Default::default() }).unwrap();
    transfers::update(&phone, transfer_named(&phone, "T cash to lei"), &TransferPatch { to_amount: Some("60.00".into()), ..Default::default() }).unwrap();
    let (_, views) = conflicts_between(&desktop, &phone, met);
    assert_eq!(fields(&views, Table::Transfers), texts(&[("to", "Savings", "Lei account"), ("toAmount", "", "60.00")]));
}

#[test]
fn a_foreign_account_deleted_on_one_device_while_the_other_puts_rows_with_second_amounts_in_it_ends_equal_by_the_rule_for_the_second_amount() {
    // who deletes, which of the two the sync is started from, and whether the rows are older or newer than the deletion
    for (deleter_is_desktop, deleter_first, rows_are_older) in [(true, true, true), (true, true, false), (true, false, true), (true, false, false), (false, true, true), (false, true, false), (false, false, true), (false, false, false)] {
        let label = format!("deleter on the desktop: {deleter_is_desktop}, sync started by the deleter: {deleter_first}, rows older: {rows_are_older}");
        let (mut desktop, mut phone) = met_with_accounts();
        open_foreign(&desktop, "Lei account", "RON", "5.00", "0.00");
        open_foreign(&desktop, "Lei two", "RON", "5.00", "0.00");
        open_foreign(&desktop, "Dollars", "USD", "1.10", "0.00");
        settle(&mut desktop, &mut phone);
        pause();
        let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
        let lei = account_sync_id(&a, "Lei account");
        // what the adder puts in the account: a payment, and a transfer from and to each kind of account, between currencies or not
        let put_rows = |b: &Connection| {
            let (lei, lei_two, dollars, cash) = (account_id(b, "Lei account"), account_id(b, "Lei two"), account_id(b, "Dollars"), account_id(b, "Cash"));
            spend_native(b, "R payment", lei, Some("20.00"));
            move_between_currencies(b, cash, lei, "10.00", Some("50.00"), "T cash to lei");
            move_between_currencies(b, lei, cash, "100.00", Some("19.00"), "T lei to cash");
            move_between_currencies(b, MAIN_ID, lei, "20.00", Some("100.00"), "T main to lei");
            move_between_currencies(b, lei, MAIN_ID, "50.00", Some("10.00"), "T lei to main");
            move_between_currencies(b, lei, lei_two, "5.00", None, "T lei to lei two");
            move_between_currencies(b, dollars, lei, "10.00", Some("55.00"), "T dollars to lei");
        };
        let delete_lei = |a: &Connection| accounts::delete(a, account_id(a, "Lei account")).unwrap();
        if rows_are_older {
            put_rows(&b);
            pause();
            delete_lei(&a);
        } else {
            delete_lei(&a);
            pause();
            put_rows(&b);
        }
        let deleted_at = marker_of(&a, &lei).unwrap();
        let transfer_sync_ids: Vec<String> = ["T main to lei", "T lei to main"].iter().map(|n| text(&b, &format!("SELECT sync_id FROM transfers WHERE note = '{n}'"))).collect();
        if deleter_first {
            settle(&mut a, &mut b);
        } else {
            settle(&mut b, &mut a);
        }
        for (who, conn) in [("deleter", &a), ("adder", &b)] {
            let label = format!("{label}, on the {who}");
            assert_eq!(count(conn, "SELECT count(*) FROM accounts WHERE name = 'Lei account'"), 0, "{label}");
            assert_eq!(marker_of(conn, &lei).as_deref(), Some(deleted_at.as_str()), "{label}: the same marker");
            // the payment is in the main account and keeps what it really paid, which the view ignores there
            assert_eq!(text(conn, "SELECT a.name FROM transactions t JOIN accounts a ON a.id = t.account_id WHERE t.name = 'R payment'"), "Main account", "{label}");
            assert_eq!((native_of(conn, "R payment"), count(conn, "SELECT amount_minor FROM transactions WHERE name = 'R payment'")), (Some(2000), 400), "{label}");
            // (from, to, what arrives): a side in the main account and the other in the app's currency too loses its second amount; one that
            // is still between two currencies keeps it
            let sides = |note: &str| -> (String, String, Option<i64>) {
                conn.query_row(
                    "SELECT f.name, t.name, x.to_amount_minor FROM transfers x JOIN accounts f ON f.id = x.from_account_id JOIN accounts t ON t.id = x.to_account_id WHERE x.note = ?1",
                    [note],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap()
            };
            assert_eq!(sides("T cash to lei"), ("Cash".into(), "Main account".into(), None), "{label}");
            assert_eq!(sides("T lei to cash"), ("Main account".into(), "Cash".into(), None), "{label}");
            assert_eq!(sides("T lei to lei two"), ("Main account".into(), "Lei two".into(), None), "{label}: it had none, and still has none");
            assert_eq!(sides("T dollars to lei"), ("Dollars".into(), "Main account".into(), Some(5500)), "{label}: dollars to the euro is still between two currencies");
            for sync_id in &transfer_sync_ids {
                assert_eq!(count(conn, &format!("SELECT count(*) FROM transfers WHERE sync_id = '{sync_id}'")), 0, "{label}: a transfer with the main account on both sides is gone");
                assert!(marker_of(conn, sync_id).is_some(), "{label}");
            }
            assert_eq!(count(conn, "SELECT count(*) FROM transfers"), 4, "{label}");
            // the amounts of the rescued transfers are in mixed units, and both devices add them up the same way: the 100.00 that left the lei
            // account counts as 100.00 of euro going out of the main account and into Cash
            assert_eq!([in_currencies(conn, "Main account").1, in_currencies(conn, "Cash").1], ["-44.00", "90.00"], "{label}");
        }
        assert_eq!(accounts_in_currencies(&a), accounts_in_currencies(&b), "{label}");
        assert_eq!(accounts::summary(&a, TODAY).unwrap(), accounts::summary(&b, TODAY).unwrap(), "{label}");
    }
}

#[test]
fn a_transfer_that_keeps_its_second_amount_after_a_deletion_is_one_the_core_can_correct_and_the_correction_arrives() {
    let (mut desktop, mut phone) = met_with_accounts();
    open_foreign(&desktop, "Lei account", "RON", "5.00", "0.00");
    open_foreign(&desktop, "Dollars", "USD", "1.10", "0.00");
    settle(&mut desktop, &mut phone);
    pause();
    move_between_currencies(&phone, account_id(&phone, "Dollars"), account_id(&phone, "Lei account"), "10.00", Some("55.00"), "T dollars to lei");
    pause();
    accounts::delete(&desktop, account_id(&desktop, "Lei account")).unwrap();
    settle(&mut desktop, &mut phone);
    assert_eq!(arrives_of(&desktop, "T dollars to lei"), Some(5500), "the lei figure now arrives in the main account");
    pause();
    // the person corrects it: 10.00 dollars are about 9.00 euro
    transfers::update(&desktop, transfer_named(&desktop, "T dollars to lei"), &TransferPatch { to_amount: Some("9.00".into()), ..Default::default() }).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!((arrives_of(conn, "T dollars to lei"), in_currencies(conn, "Main account").1), (Some(900), "9.00".into()));
    }
}

#[test]
fn a_change_of_currency_made_where_there_is_no_foreign_account_runs_on_a_device_with_one_and_leaves_what_is_in_its_own_currency_alone() {
    for rate in [Some("1.10"), None] {
        let (mut desktop, mut phone) = met_with_accounts();
        // the phone makes the account after they met, so the desktop, which never heard of it, may change its currency
        let (lei, cash, savings) = (open_foreign(&phone, "Lei account", "RON", "5.00", "100.00"), account_id(&phone, "Cash"), account_id(&phone, "Savings"));
        spend_native(&phone, "Lei lunch", lei, Some("20.00"));
        spend_in(&phone, "Cash lunch", cash);
        move_between_currencies(&phone, cash, lei, "10.00", Some("50.00"), "T cash to lei");
        move_between_currencies(&phone, lei, MAIN_ID, "100.00", Some("20.00"), "T lei to main");
        move_between_currencies(&phone, cash, savings, "10.00", None, "T cash to savings");
        let balance_before = in_currencies(&phone, "Lei account").1;
        assert_eq!(balance_before, "30.00", "100.00 - 20.00 + 50.00 - 100.00");
        currency::change(&mut desktop, "USD", rate, TODAY).unwrap();
        settle(&mut desktop, &mut phone);
        // (rate 1.10) what is in the app's currency is converted; what is in lei is not
        let converted = |minor: i64| if rate.is_some() { minor * 11 / 10 } else { minor };
        for conn in [&desktop, &phone] {
            let label = format!("rate {rate:?}");
            assert_eq!(text(conn, "SELECT currency FROM settings"), "USD", "{label}");
            assert_eq!(count(conn, "SELECT amount_minor FROM transactions WHERE name = 'Cash lunch'"), converted(500), "{label}: the app's currency");
            assert_eq!(count(conn, "SELECT amount_minor FROM transactions WHERE name = 'Lei lunch'"), converted(400), "{label}: its amount in the app's currency");
            assert_eq!(native_of(conn, "Lei lunch"), Some(2000), "{label}: what the lei account really paid");
            assert_eq!(count(conn, "SELECT amount_minor FROM transfers WHERE note = 'T cash to lei'"), converted(1000), "{label}: leaves Cash, which is in the app's currency");
            assert_eq!(arrives_of(conn, "T cash to lei"), Some(5000), "{label}: arrives in lei");
            assert_eq!(count(conn, "SELECT amount_minor FROM transfers WHERE note = 'T lei to main'"), 10_000, "{label}: leaves the lei account");
            assert_eq!(arrives_of(conn, "T lei to main"), Some(converted(2000)), "{label}: arrives in the main account, which is in the app's currency");
            assert_eq!(count(conn, "SELECT amount_minor FROM transfers WHERE note = 'T cash to savings'"), converted(1000), "{label}");
            assert_eq!(arrives_of(conn, "T cash to savings"), None, "{label}");
            assert_eq!(count(conn, "SELECT opening_balance_minor FROM accounts WHERE name = 'Lei account'"), 10_000, "{label}: the opening balance is in lei");
            assert_eq!(in_currencies(conn, "Lei account").1, balance_before, "{label}: the balance of the lei account does not move");
            assert_eq!(in_currencies(conn, "Lei account").4.as_deref(), Some("5.00"), "{label}: nor does its rate");
            assert_eq!(in_currencies(conn, "Cash").1, format!("{}.{:02}", -(converted(500) + converted(1000) + converted(1000)) / 100, (converted(500) + converted(1000) + converted(1000)) % 100), "{label}");
        }
        assert_eq!(accounts_in_currencies(&desktop), accounts_in_currencies(&phone), "rate {rate:?}");
        assert_eq!(accounts::summary(&desktop, TODAY).unwrap(), accounts::summary(&phone, TODAY).unwrap());
        assert_eq!(count(&phone, "SELECT currency_generation FROM settings"), 1);
    }
}

#[test]
fn a_change_of_currency_that_the_other_device_made_is_not_refused_by_a_device_with_a_foreign_account_even_when_the_core_refuses_to_make_one_there() {
    let (mut desktop, mut phone) = met_with_accounts();
    open_foreign(&phone, "Lei account", "RON", "5.00", "0.00");
    // the person cannot change the currency where a foreign account is ...
    let before = snapshot(&phone).unwrap();
    assert!(currency::change(&mut phone, "USD", Some("1.10"), TODAY).is_err());
    assert_eq!(snapshot(&phone).unwrap(), before, "refused, nothing written");
    // ... but the one made on the desktop runs on the phone, or the two would never agree again
    currency::change(&mut desktop, "USD", Some("1.10"), TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    assert_eq!(text(&phone, "SELECT currency FROM settings"), "USD");
    assert_eq!(in_currencies(&phone, "Lei account").3, "RON");
    // and now neither may change it again while the account is there
    assert!(currency::change(&mut desktop, "GBP", Some("0.80"), TODAY).is_err());
    accounts::delete(&desktop, account_id(&desktop, "Lei account")).unwrap();
    settle(&mut desktop, &mut phone);
    currency::change(&mut desktop, "GBP", Some("0.80"), TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    assert_eq!(text(&phone, "SELECT currency FROM settings"), "GBP");
}

#[test]
fn a_batch_that_would_leave_the_main_account_in_a_currency_or_a_currency_with_no_rate_is_refused_whole() {
    let (_desktop, mut phone) = paired();
    let before = snapshot(&phone).unwrap();
    let main = wire_row_of(&phone, "Main account");
    let account_row = |sync_id: &str, name: &str, position: i64| {
        let mut row = main.clone();
        row["syncId"] = serde_json::json!(sync_id);
        row["name"] = serde_json::json!(name);
        row["position"] = serde_json::json!(position);
        row["stamp"] = serde_json::json!(newer_than_everything());
        row
    };
    let good = account_row(&"ab".repeat(16), "Lei account", 1);
    let cases = [
        ("the main account in lei, with a rate", MAIN_SYNC_ID.to_string(), [("currency", serde_json::json!("RON")), ("rate", serde_json::json!("5.00"))]),
        ("the main account in lei, with no rate", MAIN_SYNC_ID.to_string(), [("currency", serde_json::json!("RON")), ("rate", serde_json::json!(null))]),
        ("the main account with a rate", MAIN_SYNC_ID.to_string(), [("currency", serde_json::json!(null)), ("rate", serde_json::json!("5.00"))]),
        ("an account in lei with no rate", "cd".repeat(16), [("currency", serde_json::json!("RON")), ("rate", serde_json::json!(null))]),
        ("an account with a rate and no currency", "cd".repeat(16), [("currency", serde_json::json!(null)), ("rate", serde_json::json!("5.00"))]),
    ];
    for (what, sync_id, changes) in cases {
        let mut bad = account_row(&sync_id, "Renamed", 0);
        for (key, value) in changes {
            bad[key] = value;
        }
        // the wire refuses it ...
        assert!(wire::parse_row(Table::Accounts, &bad).is_err(), "{what}");
        // ... and so does the applier when it is put in by hand with a good account in the same batch, which then does not stay either
        let rows = [good.clone(), bad].into_iter().map(|v| Row::Account(serde_json::from_value(v).unwrap())).collect();
        let mut incoming = Incoming::default();
        incoming.rows.insert(Table::Accounts, rows);
        let result = engine::apply(&mut phone, &incoming, now(), None);
        assert!(matches!(result, Err(SyncError::Invalid("account"))), "{what}: {result:?}");
        assert_eq!(snapshot(&phone).unwrap(), before, "{what}: nothing at all was applied, the good account neither");
        assert!(accounts::check(&phone).unwrap(), "{what}");
        assert_eq!(count(&phone, "SELECT applying FROM sync_clock"), 0, "{what}");
    }
    // the same two rows without the mistake go in
    let mut incoming = Incoming::default();
    incoming.rows.insert(Table::Accounts, vec![Row::Account(serde_json::from_value(good).unwrap())]);
    engine::apply(&mut phone, &incoming, now(), None).unwrap();
    assert_eq!(count(&phone, "SELECT count(*) FROM accounts"), 2);
}

#[test]
fn rows_of_a_foreign_account_with_a_native_amount_or_a_second_amount_that_break_a_rule_of_the_wire_stop_the_collector_and_change_nothing() {
    let (desktop, phone) = met_with_accounts();
    let lei = open_foreign(&desktop, "Lei account", "RON", "5.00", "0.00");
    spend_native(&desktop, "Lei lunch", lei, Some("20.00"));
    move_between_currencies(&desktop, account_id(&desktop, "Cash"), lei, "10.00", Some("50.00"), "T cash to lei");
    let before = snapshot(&phone).unwrap();
    let row = |table: Table, name: &str| -> serde_json::Value {
        let prefix = format!("{}:", table.name());
        let line = snapshot(&desktop).unwrap().into_iter().find(|l| l.starts_with(&prefix) && l.contains(name)).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(line.trim_start_matches(&prefix)).unwrap();
        value["stamp"] = serde_json::json!(newer_than_everything());
        value
    };
    let (account, payment, transfer) = (row(Table::Accounts, "Lei account"), row(Table::Transactions, "Lei lunch"), row(Table::Transfers, "T cash to lei"));
    let with = |mut v: serde_json::Value, key: &str, to: serde_json::Value| {
        v[key] = to;
        v
    };
    let bad = [
        (Table::Accounts, with(account.clone(), "currency", serde_json::json!("ron"))),
        (Table::Accounts, with(account.clone(), "rate", serde_json::json!("5,00"))),
        (Table::Accounts, with(account, "rate", serde_json::json!(null))),
        (Table::Transactions, with(payment.clone(), "nativeAmountMinor", serde_json::json!(-1))),
        (Table::Transactions, with(payment, "nativeAmountMinor", serde_json::json!(crate::money::MAX_MINOR + 1))),
        (Table::Transfers, with(transfer.clone(), "toAmountMinor", serde_json::json!(0))),
        (Table::Transfers, with(transfer, "toAmountMinor", serde_json::json!(crate::money::MAX_MINOR + 1))),
    ];
    for (table, value) in bad {
        let message = wire::RowsMessage { table, rows: vec![value.clone()], deleted: vec![], last: true };
        assert!(RowsCollector::new(vec![table]).push(message).is_err(), "{table:?}: {value}");
    }
    assert_eq!(snapshot(&phone).unwrap(), before);
}

#[test]
fn the_snapshot_sees_each_new_column_and_the_digest_and_the_hashes_go_by_the_stamps() {
    let world = || {
        let (mut desktop, mut phone) = met_with_accounts();
        let lei = open_foreign(&desktop, "Lei account", "RON", "5.00", "100.00");
        spend_native(&desktop, "Lei lunch", lei, Some("20.00"));
        move_between_currencies(&desktop, account_id(&desktop, "Cash"), lei, "10.00", Some("50.00"), "T cash to lei");
        settle(&mut desktop, &mut phone);
        (desktop, phone)
    };
    // a column written the way a sync writes it, with no stamp: the snapshot shows it, and the digest, which is the stamps, does not
    for (what, sql) in [
        ("the currency", "UPDATE accounts SET currency = 'USD' WHERE name = 'Lei account'"),
        ("the rate", "UPDATE accounts SET rate = '4.00' WHERE name = 'Lei account'"),
        ("the native amount", "UPDATE transactions SET native_amount_minor = 1 WHERE name = 'Lei lunch'"),
        ("what arrives", "UPDATE transfers SET to_amount_minor = 1 WHERE note = 'T cash to lei'"),
    ] {
        let (desktop, phone) = world();
        assert_same(&desktop, &phone, "before");
        let hashes = engine::hashes(&engine::digest(&desktop).unwrap());
        crate::accounts::fixtures::applying(&desktop, true);
        desktop.execute(sql, []).unwrap();
        crate::accounts::fixtures::applying(&desktop, false);
        assert_ne!(snapshot(&desktop).unwrap(), snapshot(&phone).unwrap(), "{what} is in the snapshot");
        assert_eq!(engine::hashes(&engine::digest(&desktop).unwrap()), hashes, "{what}: the digest is the stamps");
    }
    // the same changes made as a person makes them get a stamp, so the hash of the table changes and the other device is sent the row
    let (mut desktop, mut phone) = world();
    let hashes = engine::hashes(&engine::digest(&desktop).unwrap());
    pause();
    accounts::update(&desktop, account_id(&desktop, "Lei account"), &AccountPatch { rate: Some("4.00".into()), ..Default::default() }, TODAY).unwrap();
    transactions::update(&desktop, id_of(&desktop, "transactions", "Lei lunch"), &TransactionPatch { native_amount: Some("21.00".into()), ..Default::default() }).unwrap();
    transfers::update(&desktop, transfer_named(&desktop, "T cash to lei"), &TransferPatch { to_amount: Some("51.00".into()), ..Default::default() }).unwrap();
    let now_hashes = engine::hashes(&engine::digest(&desktop).unwrap());
    for table in [Table::Accounts, Table::Transactions, Table::Transfers] {
        let of = |hashes: &[(Table, String)]| hashes.iter().find(|(t, _)| *t == table).unwrap().1.clone();
        assert_ne!(of(&now_hashes), of(&hashes), "{table:?}");
    }
    let outcome = settle(&mut desktop, &mut phone);
    assert_eq!(outcome.compared, [Table::Accounts, Table::Transactions, Table::Transfers]);
    assert_eq!((native_of(&phone, "Lei lunch"), arrives_of(&phone, "T cash to lei"), in_currencies(&phone, "Lei account").4), (Some(2100), Some(5100), Some("4.00".into())));
}

#[test]
fn what_the_screens_accept_as_a_currency_and_a_rate_is_what_the_wire_accepts_and_the_other_way_round() {
    let currencies = ["RON", "ron", "Ron", " ron ", "RO", "RONN", "R0N", "R N", "", " ", "ÄÖÜ", "EUR", "eur", "XXX", "AAA"];
    let rates = [
        "5", "5.00", "0.5", "0.00000001", "0.000000001", "999999999.99999999", "999999999", "1000000000", "0", "0.00", "00.5", "05", "-1", "+1", "5.", ".5", "5,00", "1e3", "abc", "", " ", " 5.00",
        "5.00 ", "5 .00", "٥", "1.123456789",
    ];
    // an account as a device sends it, to put each candidate into
    let base = {
        let device = device("EUR");
        open_foreign(&device, "A", "RON", "5.00", "0.00");
        wire_row_of(&device, "A")
    };
    let make = |currency: &str, rate: Option<&str>| {
        let device = device("EUR");
        let new = NewAccount { name: "A".into(), kind: Kind::Cash, number: None, opening_balance: None, opening_date: Some("2026-01-01".into()), bic: None, notes: None, group: None, include_in_net_worth: None, virtual_balance: None, currency: Some(currency.into()), rate: rate.map(str::to_string) };
        let made = accounts::create(&device, &new, TODAY);
        (device, made)
    };
    let (mut kept_currencies, mut kept_rates) = (0, 0);
    for candidate in currencies {
        let mut row = base.clone();
        row["currency"] = serde_json::json!(candidate);
        let wire_accepts = wire::parse_row(Table::Accounts, &row).is_ok();
        // the app's own currency takes no rate on a screen (it is no currency of its own), the wire takes it as a code like another
        match make(candidate, (!candidate.trim().eq_ignore_ascii_case("EUR")).then_some("5.00")) {
            (device, Ok(made)) => {
                // what the screens keep is a valid row: capitals, no spaces, and the app's own currency is no currency at all
                let sent = wire_row_of(&device, "A");
                assert!(wire::parse_row(Table::Accounts, &sent).is_ok(), "{candidate:?} is kept by the screens and refused by the wire");
                assert_eq!(sent["currency"], if made.foreign { serde_json::json!(made.currency) } else { serde_json::json!(null) }, "{candidate:?}");
                assert_eq!(sent["rate"], if made.foreign { serde_json::json!("5.00") } else { serde_json::json!(null) }, "{candidate:?}");
                // what the wire accepts as it is, the screens keep as it is (the app's own code is the one they turn into none)
                if wire_accepts && candidate != "EUR" {
                    assert_eq!(made.currency, candidate, "{candidate:?}");
                }
                kept_currencies += 1;
            }
            (device, Err(e)) => {
                assert_eq!(e, crate::vault::AppError::InvalidInput, "{candidate:?}");
                assert!(!wire_accepts, "{candidate:?} is accepted by the wire and refused by the screens");
                assert_eq!(count(&device, "SELECT count(*) FROM accounts"), 1, "a refused account is not made");
            }
        }
    }
    for candidate in rates {
        let mut row = base.clone();
        row["rate"] = serde_json::json!(candidate);
        let wire_accepts = wire::parse_row(Table::Accounts, &row).is_ok();
        match make("RON", Some(candidate)) {
            (device, Ok(made)) => {
                let sent = wire_row_of(&device, "A");
                assert!(wire::parse_row(Table::Accounts, &sent).is_ok(), "{candidate:?} is kept by the screens and refused by the wire");
                assert_eq!(sent["rate"], serde_json::json!(candidate.trim()), "{candidate:?}: kept as typed, outer spaces apart");
                if wire_accepts {
                    assert_eq!(made.rate.as_deref(), Some(candidate), "{candidate:?}");
                }
                kept_rates += 1;
            }
            (device, Err(e)) => {
                assert_eq!(e, crate::vault::AppError::InvalidInput, "{candidate:?}");
                assert!(!wire_accepts, "{candidate:?} is accepted by the wire and refused by the screens");
                assert_eq!(count(&device, "SELECT count(*) FROM accounts"), 1, "a refused account is not made");
            }
        }
    }
    assert!(kept_currencies >= 6 && kept_rates >= 6, "the lists are not all refusals: {kept_currencies} and {kept_rates}");
}

/// A desktop that made a foreign account, a payment and a transfer in it after the two met, and a phone that has not heard of them.
fn foreign_made_on_the_desktop() -> (Connection, Connection) {
    let (desktop, phone) = met_with_accounts();
    let lei = open_foreign(&desktop, "Lei account", "RON", "5.00", "100.00");
    spend_native(&desktop, "Lei lunch", lei, Some("20.00"));
    move_between_currencies(&desktop, account_id(&desktop, "Cash"), lei, "10.00", Some("50.50"), "T cash to lei");
    (desktop, phone)
}

#[test]
fn a_sync_cut_after_any_message_leaves_a_foreign_account_with_its_rows_as_they_were_and_the_next_one_finishes_it() {
    let total = {
        let (mut desktop, mut phone) = foreign_made_on_the_desktop();
        sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap().messages
    };
    assert!(total > 6, "{total} messages");
    for cut in 0..total {
        let (mut desktop, mut phone) = foreign_made_on_the_desktop();
        let before = (snapshot(&desktop).unwrap(), snapshot(&phone).unwrap());
        assert!(sync_cut_after(&mut desktop, &mut phone, TODAY, now(), cut).is_err(), "cut after {cut} of {total} messages");
        // never half of it: the phone has nothing of the account, or all of it (the account, the payment and the transfer are applied together)
        if cut < total - 2 {
            assert_eq!((snapshot(&desktop).unwrap(), snapshot(&phone).unwrap()), before, "cut after {cut} of {total} messages: nothing was applied");
            assert_eq!(count(&phone, "SELECT count(*) FROM accounts WHERE currency IS NOT NULL"), 0);
        } else {
            assert_same(&desktop, &phone, &format!("cut after {cut} of {total}: both had applied it"));
        }
        settle(&mut desktop, &mut phone);
        assert_eq!(in_currencies(&phone, "Lei account"), ("Lei account".into(), "130.50".into(), "26.10".into(), "RON".into(), Some("5.00".into())), "cut after {cut}");
        assert_eq!((native_of(&phone, "Lei lunch"), arrives_of(&phone, "T cash to lei")), (Some(2000), Some(5050)), "cut after {cut}");
    }
}

#[test]
fn a_device_that_speaks_protocol_10_is_told_to_update_whichever_of_the_two_it_is_and_nothing_is_exchanged() {
    let (a, b) = (device("EUR"), device("EUR"));
    let new = engine::hello(&a, "desktop", "0.1.0", now()).unwrap();
    let mut old = engine::hello(&b, "phone", "0.1.0", now()).unwrap();
    assert_eq!(new.protocol, 11);
    old.protocol = 10;
    assert!(matches!(engine::check_hello(&new, &old, now()), Err(SyncError::ProtocolMismatch)));
    assert!(matches!(engine::check_hello(&old, &new, now()), Err(SyncError::ProtocolMismatch)));
    // a row in the shape of protocol 10 is a good row of protocol 11 (the new fields are optional), so the hello is what stops the old device
    let (_desktop, phone) = paired();
    let mut old_row = wire_row_of(&phone, "Main account");
    for key in ["currency", "rate"] {
        old_row.as_object_mut().unwrap().remove(key);
    }
    assert!(wire::parse_row(Table::Accounts, &old_row).is_ok());
}
