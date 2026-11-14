//! Backups with accounts and transfers: they come back exactly, a backup from before accounts gets the main account, and a file that
//! breaks the rules of the accounts is refused whole and leaves the database as it was (`docs/accounts-design.md`, sections 9 and 5.5).

use serde_json::{json, Value};

use super::tests::{db, db_at, forged, lived_in, main_account};
use super::*;
use crate::accounts::fixtures::{account, snapshot, transfer, TODAY};
use crate::accounts::{self, AccountPatch, MAIN_ID, MAIN_SYNC_ID};
use crate::keys::memory::MemoryStore;
use crate::vault::{Settings, Vault};

const PASSPHRASE: &str = "correct horse";

fn restore(target: &mut Connection, file: &[u8]) -> Result<(), AppError> {
    restore_snapshot(target, &open(file, PASSPHRASE)?)
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn accounts_transfers_and_the_account_of_every_row_come_back_exactly() {
    let source = lived_in();
    let before = dump(&source).unwrap();
    for table in ["transactions", "recurring_transactions", "ledger_entries"] {
        assert!(before.rows(table).iter().any(|r| r["account_id"] == json!(2)), "the test data has a row of {table} in the second account");
    }
    assert!(before.rows("transactions").iter().any(|r| r["account_id"] == json!(1)), "and one in the main account");
    assert_eq!(before.rows("accounts").len(), 2);
    assert_eq!(before.rows("transfers").len(), 1);

    let file = seal(&before, PASSPHRASE).unwrap();
    let mut target = db();
    restore(&mut target, &file).unwrap();

    assert_eq!(dump(&target).unwrap(), before, "ids, stamps, sync ids and the account of each row");
    // and they are alive: the same numbers, and the main account is the one the rules know
    assert!(accounts::check(&target).unwrap());
    assert_eq!(accounts::list(&target, TODAY).unwrap(), accounts::list(&source, TODAY).unwrap());
    assert_eq!(accounts::summary(&target, TODAY).unwrap(), accounts::summary(&source, TODAY).unwrap());
    for id in [1, 2] {
        assert_eq!(accounts::statement(&target, id, "2026-01-01", "2026-12-31", TODAY).unwrap(), accounts::statement(&source, id, "2026-01-01", "2026-12-31", TODAY).unwrap(), "account {id}");
    }
    // a row made after the restore works: the ids carry on from the file, and the new account is the next one
    let cash = account(&target, "Cash");
    assert_eq!(cash, 3);
    transfer(&target, 2, cash, "1.00", "2026-03-01");
    assert_eq!(accounts::delete(&target, 2), Err(AppError::InvalidInput), "an account with rows is still not deleted");
}

#[test]
fn the_main_account_keeps_its_own_name_number_and_opening_balance_and_an_archived_account_stays_archived() {
    let source = lived_in();
    let patch = AccountPatch {
        name: Some("Current account".into()),
        number: Some(Some("IE29 AIBK 9311 5212 3456 78".into())),
        opening_balance: Some("-120.00".into()),
        opening_date: Some("2025-12-01".into()),
        ..Default::default()
    };
    accounts::update(&source, MAIN_ID, &patch, TODAY).unwrap();
    accounts::archive(&source, 2, TODAY).unwrap();
    let before = dump(&source).unwrap();

    let mut target = db();
    restore(&mut target, &seal(&before, PASSPHRASE).unwrap()).unwrap();
    assert_eq!(dump(&target).unwrap(), before);

    let main = dump(&target).unwrap().rows("accounts")[0].clone();
    assert_eq!((main["id"].as_i64(), main["sync_id"].as_str(), main["archived"].as_i64()), (Some(MAIN_ID), Some(MAIN_SYNC_ID), Some(0)), "the constants of the main account");
    assert_eq!((main["name"].as_str(), main["opening_balance_minor"].as_i64()), (Some("Current account"), Some(-12_000)));
    let list = accounts::list(&target, TODAY).unwrap();
    assert_eq!((list[0].name.as_str(), list[0].is_main, list[0].archived), ("Current account", true, false));
    assert_eq!((list[1].name.as_str(), list[1].archived, list[1].entries > 0), ("Savings", true, true), "it is archived, with its rows and its balance");
    assert_eq!(list, accounts::list(&source, TODAY).unwrap());
}

/// A database as it was before accounts, at schema version `version`: a category, a transaction and a recurring item, and from version 12 on
/// (the debts) a friend and an entry of the ledger.
fn old_database(version: u32) -> Connection {
    let old = db_at(version);
    for sql in [
        "INSERT INTO settings (id, currency) VALUES (1, 'EUR')",
        "INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')",
        "INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (1, 'Dinner', 4000, '2026-01-01')",
        "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date)
         VALUES (1, 'Box', 2400, '2026-01-01', 'WEEK', '2026-01-08')",
    ] {
        old.execute(sql, []).unwrap();
    }
    if version >= 12 {
        old.execute("INSERT INTO people (name) VALUES ('Anna')", []).unwrap();
        old.execute("INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date) VALUES (1, 'LENT', 1500, '2026-01-05')", []).unwrap();
    }
    old
}

#[test]
fn a_backup_made_before_accounts_restores_with_one_main_account_holding_everything() {
    let fresh = dump(&db()).unwrap();
    // 17 is the version just before accounts; 11 is before the debts, so it has no ledger
    for version in [11, 17] {
        let snapshot = dump(&old_database(version)).unwrap();
        assert_eq!(snapshot.schema_version, version);
        assert!(!snapshot.tables.contains_key("accounts") && !snapshot.tables.contains_key("transfers"), "version {version} has neither table");

        // the person had accounts of their own on this device, which the restore replaces
        let mut target = lived_in();
        restore(&mut target, &seal(&snapshot, PASSPHRASE).unwrap()).unwrap_or_else(|e| panic!("version {version}: {e:?}"));

        let after = dump(&target).unwrap();
        assert_eq!(after.rows("accounts"), fresh.rows("accounts"), "version {version}: the main account as the migration makes it, and no other");
        assert!(after.rows("transfers").is_empty(), "version {version}");
        let (rows, balance) = if version >= 12 { (3, "-55.00") } else { (2, "-40.00") };
        for table in ["transactions", "recurring_transactions", "ledger_entries"].iter().take(rows) {
            assert_eq!(after.rows(table).len(), 1, "{table}, version {version}");
            assert_eq!(after.rows(table)[0]["account_id"], json!(1), "{table}, version {version}: in the main account");
        }
        assert_eq!(after.rows("transactions")[0]["name"], "Dinner");
        assert!(accounts::check(&target).unwrap());
        let main = accounts::get(&target, MAIN_ID, TODAY).unwrap();
        assert_eq!((main.entries as usize, main.balance.as_str(), main.opening_balance.as_str()), (rows, balance, "0.00"), "version {version}: it holds all of it");
        assert!(!accounts::summary(&target, TODAY).unwrap().in_use, "and nobody is bothered with accounts");
    }
}

#[test]
fn a_backup_from_a_database_that_never_had_accounts_and_never_had_rows_restores_to_a_fresh_one() {
    let snapshot = dump(&db_at(17)).unwrap();
    let mut target = lived_in();
    restore_snapshot(&mut target, &snapshot).unwrap();
    let after = dump(&target).unwrap();
    for table in TABLES.iter().filter(|table| **table != "accounts") {
        assert!(after.rows(table).is_empty(), "{table}");
    }
    assert_eq!(after.rows("accounts"), dump(&db()).unwrap().rows("accounts"));
}

#[test]
fn a_restore_removes_the_accounts_and_transfers_that_were_there() {
    // the target has accounts, transfers and rows in them that the file knows nothing about
    let mut target = lived_in();
    let cash = account(&target, "Cash");
    transfer(&target, 2, cash, "5.00", "2026-02-02");
    accounts::archive(&target, cash, TODAY).unwrap();
    target.execute("INSERT INTO transactions (category_id, name, amount_minor, payment_date, account_id) VALUES (1, 'In cash', 100, '2026-02-02', ?1)", [cash]).unwrap();

    let source = db();
    accounts::update(&source, MAIN_ID, &AccountPatch { name: Some("Renamed".into()), ..Default::default() }, TODAY).unwrap();
    let file = seal(&dump(&source).unwrap(), PASSPHRASE).unwrap();
    restore(&mut target, &file).unwrap();

    assert_eq!(dump(&target).unwrap(), dump(&source).unwrap());
    assert_eq!(count(&target, "SELECT count(*) FROM accounts"), 1);
    assert_eq!(count(&target, "SELECT count(*) FROM transfers"), 0);
    assert_eq!(accounts::get(&target, MAIN_ID, TODAY).unwrap().name, "Renamed", "the main account is the file's, not a new one");
    assert!(accounts::check(&target).unwrap());
    // the ids start over: the next account is the second
    assert_eq!(account(&target, "Cash"), 2);
}

fn table<'a>(snapshot: &'a mut Snapshot, name: &str) -> &'a mut Vec<Row> {
    snapshot.tables.get_mut(name).unwrap_or_else(|| panic!("no rows for {name}"))
}

fn set(snapshot: &mut Snapshot, name: &str, index: usize, column: &str, value: Value) {
    table(snapshot, name)[index].insert(column.into(), value);
}

/// A database with data of its own, as it is before a restore that must be refused.
fn with_its_own_data() -> Connection {
    let target = lived_in();
    let cash = account(&target, "Only here");
    transfer(&target, MAIN_ID, cash, "9.00", "2026-02-03");
    target
}

/// Each change to the rows below makes a file the app could not have written. Every one is refused, and nothing changes.
#[test]
fn a_file_that_breaks_the_rules_of_the_accounts_is_refused_whole_and_changes_nothing() {
    let base = dump(&lived_in()).unwrap();
    type Change = Box<dyn Fn(&mut Snapshot)>;
    let cases: Vec<(&str, Change)> = vec![
        ("a transaction in an account that is not there", Box::new(|s| set(s, "transactions", 0, "account_id", json!(99)))),
        ("a recurring item in an account that is not there", Box::new(|s| set(s, "recurring_transactions", 0, "account_id", json!(99)))),
        ("a ledger entry in an account that is not there", Box::new(|s| set(s, "ledger_entries", 0, "account_id", json!(99)))),
        ("a transfer from an account that is not there", Box::new(|s| set(s, "transfers", 0, "from_account_id", json!(99)))),
        ("a transfer to an account that is not there", Box::new(|s| set(s, "transfers", 0, "to_account_id", json!(99)))),
        ("a row with an account of nothing", Box::new(|s| set(s, "transactions", 0, "account_id", Value::Null))),
        ("a transfer of an account to itself", Box::new(|s| set(s, "transfers", 0, "to_account_id", json!(1)))),
        ("a transfer of nothing", Box::new(|s| set(s, "transfers", 0, "amount_minor", json!(0)))),
        ("a transfer of too much", Box::new(|s| set(s, "transfers", 0, "amount_minor", json!(1_000_000_000_000_i64)))),
        ("a transfer that is not dated", Box::new(|s| set(s, "transfers", 0, "transfer_date", json!("2026-02-30")))),
        ("a transfer with an empty note", Box::new(|s| set(s, "transfers", 0, "note", json!("")))),
        ("a transfer with a long note", Box::new(|s| set(s, "transfers", 0, "note", json!("x".repeat(201))))),
        ("the main account archived", Box::new(|s| set(s, "accounts", 0, "archived", json!(1)))),
        ("an account that is archived two", Box::new(|s| set(s, "accounts", 1, "archived", json!(2)))),
        ("the main account under another sync id", Box::new(|s| set(s, "accounts", 0, "sync_id", json!("ab".repeat(16))))),
        ("an account with a sync id that is not one", Box::new(|s| set(s, "accounts", 1, "sync_id", json!("AB".repeat(16))))),
        ("two accounts with one sync id", Box::new(|s| set(s, "accounts", 1, "sync_id", json!(MAIN_SYNC_ID)))),
        ("a kind that does not exist", Box::new(|s| set(s, "accounts", 1, "kind", json!("PIGGY_BANK")))),
        ("an account with no name", Box::new(|s| set(s, "accounts", 1, "name", json!("")))),
        ("a name that is too long", Box::new(|s| set(s, "accounts", 1, "name", json!("x".repeat(51))))),
        ("an empty number", Box::new(|s| set(s, "accounts", 1, "number", json!("")))),
        ("an opening balance out of range", Box::new(|s| set(s, "accounts", 1, "opening_balance_minor", json!(1_000_000_000_000_i64)))),
        ("an opening balance out of range, negative", Box::new(|s| set(s, "accounts", 1, "opening_balance_minor", json!(-1_000_000_000_000_i64)))),
        ("an opening date that is not one", Box::new(|s| set(s, "accounts", 1, "opening_date", json!("2026-13-01")))),
        ("an opening balance of text", Box::new(|s| set(s, "accounts", 1, "opening_balance_minor", json!("many")))),
        ("an account that is a number with decimals", Box::new(|s| set(s, "accounts", 1, "opening_balance_minor", json!(1.5)))),
    ];
    for (label, change) in cases {
        let mut snapshot = base.clone();
        change(&mut snapshot);
        let mut target = with_its_own_data();
        let before = dump(&target).unwrap();
        let file = seal(&snapshot, PASSPHRASE).unwrap();
        assert_eq!(restore(&mut target, &file), Err(AppError::InvalidBackup), "{label}");
        assert_eq!(dump(&target).unwrap(), before, "{label}: nothing changed");
        assert!(accounts::check(&target).unwrap(), "{label}");
    }
}

/// A file is only read in a scratch database first; `commit` is what writes to the real one. If a file got past the first look, the single
/// transaction still leaves the database as it was: its rows are all written before the main account is looked at.
#[test]
fn a_file_without_a_valid_main_account_is_refused_even_when_it_reaches_the_real_database() {
    let main = main_account().as_array().unwrap()[0].clone();
    let other = |sync_id: &str, id: i64| {
        let mut row = main.as_object().unwrap().clone();
        row.insert("sync_id".into(), json!(sync_id));
        row.insert("id".into(), json!(id));
        row.insert("name".into(), json!("Not the main account"));
        row
    };
    let cases: Vec<(&str, Value)> = vec![
        ("no accounts, and nothing in them", json!({ "accounts": [] })),
        ("no accounts key at all", json!({})),
        ("only another account", json!({ "accounts": [other("cd".repeat(16).as_str(), 2)] })),
        ("the main sync id under another id", json!({ "accounts": [other(MAIN_SYNC_ID, 2)] })),
        ("the main id with another sync id", json!({ "accounts": [other("cd".repeat(16).as_str(), 1)] })),
        ("the main account only as a second row", json!({ "accounts": [other("cd".repeat(16).as_str(), 1), other(MAIN_SYNC_ID, 2)] })),
    ];
    for (label, tables) in cases {
        let file = forged(tables);
        let mut target = with_its_own_data();
        let before = dump(&target).unwrap();
        // through the whole restore: `prepare` finds it
        assert_eq!(restore(&mut target, &file), Err(AppError::InvalidBackup), "{label}");
        assert_eq!(dump(&target).unwrap(), before, "{label}: nothing changed");
        // straight into the real database: it is emptied, filled and then found out, and all of it is undone
        let rows = open(&file, PASSPHRASE).unwrap().tables;
        assert_eq!(replace_all(&mut target, &rows), Err(AppError::InvalidBackup), "{label}");
        assert_eq!(dump(&target).unwrap(), before, "{label}: the transaction was dropped");
        assert_eq!(count(&target, "SELECT applying FROM sync_clock"), 0, "{label}: the checks stand again");
    }
}

#[test]
fn rows_that_name_no_account_go_to_the_main_account() {
    let file = forged(json!({
        "accounts": main_account(),
        "categories": [{ "id": 1, "type": "EXPENSE", "name": "Food" }],
        "transactions": [{ "id": 1, "category_id": 1, "name": "x", "amount_minor": 500, "payment_date": "2026-01-01" }],
    }));
    let mut target = lived_in();
    restore(&mut target, &file).unwrap();
    assert_eq!(count(&target, "SELECT account_id FROM transactions"), 1);
    assert_eq!(accounts::get(&target, MAIN_ID, TODAY).unwrap().balance, "-5.00");
}

#[test]
fn the_tables_come_after_the_tables_they_point_at() {
    let conn = db();
    let at = |name: &str| TABLES.iter().position(|t| *t == name).unwrap_or_else(|| panic!("{name} is not listed"));
    for (child, index) in TABLES.iter().map(|t| (*t, at(t))) {
        let mut statement = conn.prepare("SELECT DISTINCT \"table\" FROM pragma_foreign_key_list(?1)").unwrap();
        for parent in statement.query_map([child], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap) {
            assert!(parent == child || at(&parent) < index, "{child} points at {parent}, which comes after it");
        }
    }
    for (first, then) in [("accounts", "transfers"), ("accounts", "transactions"), ("accounts", "recurring_transactions"), ("accounts", "ledger_entries")] {
        assert!(at(first) < at(then), "{first} before {then}");
    }
}

fn settings() -> Settings {
    Settings { currency: "eur".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() }
}

fn vault(dir: &Path) -> Vault {
    Vault::new(dir.to_path_buf(), Box::new(MemoryStore::default()))
}

/// Accounts, a transfer and a row in each account, in the database of a vault.
fn fill(v: &Vault) {
    v.with_conn(|conn| {
        let cash = account(conn, "Cash");
        transfer(conn, MAIN_ID, cash, "20.00", "2026-02-02");
        accounts::update(conn, MAIN_ID, &AccountPatch { name: Some("Mine".into()), opening_balance: Some("100.00".into()), ..Default::default() }, TODAY)?;
        accounts::archive(conn, cash, TODAY)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_all_my_data_leaves_exactly_the_main_account_in_its_fresh_state() {
    let dir = tempfile::tempdir().unwrap();
    let v = vault(dir.path());
    v.create(&settings(), None).unwrap();
    fill(&v);
    assert_eq!(v.with_conn(|c| Ok(count(c, "SELECT count(*) FROM accounts"))).unwrap(), 2);

    assert_eq!(v.wipe("delete"), Err(AppError::InvalidInput));
    assert_eq!(v.with_conn(|c| Ok(count(c, "SELECT count(*) FROM transfers"))).unwrap(), 1, "a refused wipe changes nothing");

    v.wipe("DELETE").unwrap();
    v.create(&settings(), None).unwrap();
    let fresh = dump(&db()).unwrap();
    let now = v.with_conn(|c| dump(c)).unwrap();
    assert_eq!(now.rows("accounts"), fresh.rows("accounts"), "the main account of the migration, with its constants, name, dates and stamp");
    assert_eq!(now.rows("accounts").len(), 1);
    for table in ["transfers", "transactions", "recurring_transactions", "ledger_entries", "people"] {
        assert!(now.rows(table).is_empty(), "{table}");
    }
    v.with_conn(|c| {
        assert!(accounts::check(c).unwrap());
        let main = accounts::get(c, MAIN_ID, TODAY)?;
        assert_eq!((main.name.as_str(), main.opening_balance.as_str(), main.balance.as_str(), main.archived, main.entries), ("Main account", "0.00", "0.00", false, 0));
        assert!(!accounts::summary(c, TODAY)?.in_use);
        // and the next account made is the second, as in any new database
        assert_eq!(account(c, "Savings"), 2);
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_restore_through_the_vault_of_a_file_without_the_main_account_is_refused_and_the_data_stays() {
    let dir = tempfile::tempdir().unwrap();
    let v = vault(dir.path());
    v.create(&settings(), None).unwrap();
    fill(&v);
    let before = v.with_conn(|c| dump(c)).unwrap();
    let marks = v.with_conn(|c| Ok(snapshot(c))).unwrap();

    for tables in [json!({ "accounts": [] }), json!({ "transfers": [] })] {
        assert_eq!(v.restore_backup(&forged(tables), PASSPHRASE, "RESTORE"), Err(AppError::InvalidBackup));
        assert_eq!(v.with_conn(|c| dump(c)).unwrap(), before, "the data stays");
        assert_eq!(v.with_conn(|c| Ok(snapshot(c))).unwrap(), marks, "and no stamp moved");
    }
    // a good one, with the main account in it, is taken
    v.restore_backup(&forged(json!({ "accounts": main_account() })), PASSPHRASE, "RESTORE").unwrap();
    assert_eq!(v.with_conn(|c| Ok(count(c, "SELECT count(*) FROM accounts"))).unwrap(), 1);
}
