//! Backups with stashes: they come back exactly, a backup from before stashes restores and is migrated with none, and a file that breaks the
//! rules of the stashes or their foreign keys is refused whole and leaves the database as it was (`docs/stash-design.md`, section 9).

use serde_json::{json, Value};

use super::tests::{db, db_at, forged, lived_in, main_account};
use super::*;
use crate::accounts::fixtures::{account, new_account, snapshot, transfer, TODAY};
use crate::accounts::{self, AccountPatch, Kind, NewAccount, MAIN_ID};
use crate::keys::memory::MemoryStore;
use crate::rates::Period;
use crate::stashes::entries::{self, NewEntry, NewTake};
use crate::stashes::{self, NewStash, StashKind};
use crate::vault::{Settings, Vault};

const PASSPHRASE: &str = "correct horse";

fn restore(target: &mut Connection, file: &[u8]) -> Result<(), AppError> {
    restore_snapshot(target, &open(file, PASSPHRASE)?)
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// `lived_in()` (Holiday, in Savings, with one add that came with the first transfer) with three stashes more, so that every kind of row is in the
/// file: Bike (Savings, archived: an add and a take that says "bought it"), Wedding gift (the account in lei, target 1000.00 and notes of two lines,
/// an add of 300.00 lei and one of 50.00 that came with the second transfer) and Rainy day (the main account, no target, and a take written
/// straight into the table as a sync brings it, so the stash holds less than 0).
fn with_stashes() -> Connection {
    let conn = lived_in();
    let make = |name: &str, account: i64, target: Option<&str>, notes: Option<&str>| {
        stashes::create(&conn, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: name.into(), account_id: account.to_string(), target: target.map(str::to_string), target_date: None, notes: notes.map(str::to_string) }, "2026-02-04").unwrap().id
    };
    let bike = make("Bike", 2, Some("450.00"), None);
    let wedding = make("Wedding gift", 3, Some("1000.00"), Some("for the wedding\nin Cluj, \"June\""));
    let rainy = make("Rainy day", MAIN_ID, None, None);
    for (stash, minor, date, note, transfer) in [
        (bike, 45_000, "2026-01-10", Some("birthday money"), None),
        (bike, -45_000, "2026-01-20", Some("bought it"), None),
        (wedding, 30_000, "2026-02-01", None, None),
        (wedding, 5_000, "2026-02-04", Some("came with the lei"), Some(2)),
        (rainy, -500, "2026-02-02", None, None),
    ] {
        conn.execute("INSERT INTO stash_entries (stash_id, amount_minor, entry_date, note, transfer_id) VALUES (?1, ?2, ?3, ?4, ?5)", rusqlite::params![stash, minor, date, note, transfer]).unwrap();
    }
    stashes::archive(&conn, bike, "2026-02-04").unwrap();
    conn
}

#[test]
fn stashes_and_their_entries_come_back_exactly() {
    let source = with_stashes();
    let before = dump(&source).unwrap();
    assert_eq!((before.rows("stashes").len(), before.rows("stash_entries").len()), (4, 6), "the stash of lived_in and the three more, and the entries");
    assert!(before.rows("stash_entries").iter().any(|r| r["transfer_id"] == json!(1)) && before.rows("stash_entries").iter().any(|r| r["transfer_id"].is_null()), "linked and not");
    assert!(before.rows("stash_entries").iter().any(|r| r["amount_minor"].as_i64().is_some_and(|n| n < 0)), "an add and a take");
    assert!(before.rows("stashes").iter().any(|r| r["archived"] == json!(1)) && before.rows("stashes").iter().any(|r| r["target_minor"].is_null()), "archived or not, with a target or without");

    let file = seal(&before, PASSPHRASE).unwrap();
    let mut target = db();
    restore(&mut target, &file).unwrap();

    assert_eq!(dump(&target).unwrap(), before, "ids, stamps, sync ids, notes with line breaks, the links to the transfers and the whole history");
    // and they are alive: the same lists, the same numbers, and the same account totals
    assert_eq!(stashes::list(&target, None, TODAY).unwrap(), stashes::list(&source, None, TODAY).unwrap());
    assert_eq!(accounts::list(&target, TODAY).unwrap(), accounts::list(&source, TODAY).unwrap(), "what each account has set aside, and so what is free");
    let wedding = stashes::list(&target, None, TODAY).unwrap().into_iter().find(|s| s.name == "Wedding gift").unwrap();
    assert_eq!((wedding.currency.as_str(), wedding.saved.as_str(), wedding.notes.as_deref()), ("RON", "350.00", Some("for the wedding\nin Cluj, \"June\"")));
    assert!(accounts::check(&target).unwrap());
    assert_eq!(count(&target, "SELECT applying FROM sync_clock"), 0, "the triggers that make stamps are back");

    // a row made after the restore carries on from the ids of the file
    let next = account(&target, "Cash");
    let made = stashes::create(&target, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: "After".into(), account_id: next.to_string(), target: None, target_date: None, notes: None }, TODAY).unwrap();
    assert_eq!(made.id, 5);
    target.execute("INSERT INTO stash_entries (stash_id, amount_minor, entry_date) VALUES (5, 100, '2026-03-01')", []).unwrap();
    assert_eq!(count(&target, "SELECT MAX(id) FROM stash_entries"), 7);
    // the link to a transfer is a live one: deleting the transfer clears it and gives the entry a new stamp, as in any other database
    let stamp = |conn: &Connection| -> String { conn.query_row("SELECT stamp FROM stash_entries WHERE transfer_id = 1 OR id = 1", [], |r| r.get(0)).unwrap() };
    let was = stamp(&target);
    accounts::transfers::delete(&target, 1).unwrap();
    assert_eq!(count(&target, "SELECT count(*) FROM stash_entries WHERE transfer_id IS NULL AND id = 1"), 1, "the entry stays, with no transfer");
    assert_ne!(stamp(&target), was);
    // an account with a stash is not deleted, as before
    assert_eq!(accounts::delete(&target, 2), Err(AppError::InvalidInput));
}

/// `lived_in()` with a mortgage and a loan (a rate and an installment each), a rate on Savings, and two deposits in Savings, one running and one
/// broken on 2026-02-10 (an add of 5.00 and the take of all of it): every new column of `accounts` and `stashes` has a value in the file.
fn with_loans_and_deposits() -> Connection {
    let conn = lived_in();
    let debt = |name: &str, kind: Kind, owed: &str, rate: &str, period: Period, installment: &str| {
        let new = NewAccount {
            kind,
            opening_balance: Some(format!("-{owed}")),
            opening_date: Some("2026-01-01".into()),
            interest_rate: Some(rate.into()),
            interest_period: Some(period),
            installment: Some(installment.into()),
            ..new_account(name)
        };
        accounts::create(&conn, &new, "2026-02-04").unwrap().id
    };
    debt("Flat", Kind::Mortgage, "180000.00", "3.9", Period::Yearly, "950.00");
    debt("Car", Kind::Loan, "9000.00", "0.5", Period::Monthly, "300.00");
    accounts::update(&conn, 2, &AccountPatch { interest_rate: Some(Some("2.5".into())), interest_period: Some(Period::Yearly), ..Default::default() }, "2026-02-04").unwrap();
    // Savings holds less than nothing in this world, so it is given what the two deposits are made of
    transfer(&conn, MAIN_ID, 2, "10000.00", "2026-02-02");
    let deposit = |name: &str, rate: &str| {
        let new = NewStash {
            kind: Some(StashKind::Deposit),
            interest_rate: Some(rate.into()),
            interest_period: Some(Period::Yearly),
            start_date: Some("2026-01-01".into()),
            target_date: Some("2027-01-01".into()),
            ..NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: name.into(), account_id: "2".into(), target: None, target_date: None, notes: Some("two\nlines".into()) }
        };
        let id = stashes::create(&conn, &new, "2026-02-04").unwrap().id;
        entries::add(&conn, id, &NewEntry { amount: "5.00".into(), date: Some("2026-02-01".into()), note: Some("in".into()), from_account_id: None }, "2026-02-04").unwrap();
        id
    };
    deposit("Running", "3.1");
    let broken = deposit("Broken", "4.125");
    entries::take(&conn, broken, &NewTake { amount: "5.00".into(), date: Some("2026-02-10".into()), note: Some("out".into()), to_account_id: None }, "2026-02-10").unwrap();
    conn
}

#[test]
fn a_mortgage_with_its_rate_and_installment_and_a_broken_deposit_come_back_with_every_field() {
    let source = with_loans_and_deposits();
    let before = dump(&source).unwrap();
    let row = |snapshot: &Snapshot, table: &str, name: &str| snapshot.rows(table).iter().find(|r| r["name"] == json!(name)).unwrap().clone();
    let flat = row(&before, "accounts", "Flat");
    assert_eq!((&flat["kind"], &flat["rate_milli"], &flat["rate_period"], &flat["installment_minor"]), (&json!("MORTGAGE"), &json!(3900), &json!("YEARLY"), &json!(95_000)));
    let broken = row(&before, "stashes", "Broken");
    assert_eq!((&broken["kind"], &broken["rate_milli"], &broken["start_date"], &broken["broken_on"], &broken["returned_on"]), (&json!("DEPOSIT"), &json!(4125), &json!("2026-01-01"), &json!("2026-02-10"), &Value::Null));

    let file = seal(&before, PASSPHRASE).unwrap();
    let mut target = db();
    restore(&mut target, &file).unwrap();

    assert_eq!(dump(&target).unwrap(), before, "every column of every row, the new ones too");
    let after = dump(&target).unwrap();
    assert_eq!(row(&after, "accounts", "Car")["installment_minor"], json!(30_000));
    assert_eq!(row(&after, "accounts", "Savings")["rate_milli"], json!(2500));
    assert_eq!((row(&after, "stashes", "Running")["broken_on"].clone(), row(&after, "stashes", "Running")["rate_period"].clone()), (Value::Null, json!("YEARLY")));
    // and they are alive: the same lists, with what is worked out from the columns, and the main account is as the rules want it
    assert_eq!(accounts::list(&target, TODAY).unwrap(), accounts::list(&source, TODAY).unwrap());
    assert_eq!(stashes::list(&target, None, TODAY).unwrap(), stashes::list(&source, None, TODAY).unwrap());
    let list = stashes::list(&target, None, TODAY).unwrap();
    let broken = list.iter().find(|s| s.name == "Broken").and_then(|s| s.deposit.clone()).unwrap();
    assert_eq!((broken.state, broken.broken_on.as_deref(), broken.interest_rate.as_str()), (crate::stashes::DepositState::Broken, Some("2026-02-10"), "4.125"));
    let flat = accounts::list(&target, TODAY).unwrap().into_iter().find(|a| a.name == "Flat").unwrap();
    assert_eq!((flat.interest_rate.as_deref(), flat.installment.as_deref(), flat.balance.as_str()), (Some("3.9"), Some("950.00"), "-180000.00"));
    assert!(accounts::check(&target).unwrap());
    assert_eq!(count(&target, "SELECT applying FROM sync_clock"), 0);
    // the rules of the columns still hold in the restored database: a goal cannot be given a rate by hand
    assert!(target.execute("UPDATE stashes SET rate_milli = 100 WHERE name = 'Holiday'", []).is_err());
}

#[test]
fn a_restore_replaces_the_stashes_that_were_there_and_a_file_with_none_leaves_none() {
    // the target has stashes and entries that the file knows nothing about
    let mut target = with_stashes();
    let cash = account(&target, "Only here");
    let only_here = stashes::create(&target, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: "Only here".into(), account_id: cash.to_string(), target: None, target_date: None, notes: None }, TODAY).unwrap().id;
    target.execute("INSERT INTO stash_entries (stash_id, amount_minor, entry_date) VALUES (?1, 700, '2026-02-02')", [only_here]).unwrap();
    transfer(&target, MAIN_ID, cash, "9.00", "2026-02-03");

    let source = lived_in();
    let file = seal(&dump(&source).unwrap(), PASSPHRASE).unwrap();
    restore(&mut target, &file).unwrap();
    assert_eq!(dump(&target).unwrap(), dump(&source).unwrap());
    assert_eq!((count(&target, "SELECT count(*) FROM stashes"), count(&target, "SELECT count(*) FROM stash_entries")), (1, 1), "only the one of the file");
    assert_eq!(count(&target, "SELECT name = 'Holiday' FROM stashes"), 1);

    // a file with no stash at all empties both tables
    let empty = forged(json!({ "accounts": main_account() }));
    restore(&mut target, &empty).unwrap();
    assert_eq!((count(&target, "SELECT count(*) FROM stashes"), count(&target, "SELECT count(*) FROM stash_entries")), (0, 0));
    assert_eq!(dump(&target).unwrap().tables, dump(&db()).unwrap().tables, "nothing but the main account every database has");
    // the ids start over (the file had no settings, which a new account needs for its currency)
    target.execute("INSERT INTO settings (id, currency) VALUES (1, 'EUR')", []).unwrap();
    let account = account(&target, "Cash");
    assert_eq!(stashes::create(&target, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: "First".into(), account_id: account.to_string(), target: None, target_date: None, notes: None }, TODAY).unwrap().id, 1);
}

fn table<'a>(snapshot: &'a mut Snapshot, name: &str) -> &'a mut Vec<Row> {
    snapshot.tables.get_mut(name).unwrap_or_else(|| panic!("no rows for {name}"))
}

fn set(snapshot: &mut Snapshot, name: &str, index: usize, column: &str, value: Value) {
    table(snapshot, name)[index].insert(column.into(), value);
}

/// A database with data of its own, as it is before a restore that must be refused.
fn with_its_own_data() -> Connection {
    let target = with_stashes();
    let cash = account(&target, "Only here");
    transfer(&target, MAIN_ID, cash, "9.00", "2026-02-03");
    target.execute("INSERT INTO stash_entries (stash_id, amount_minor, entry_date, note) VALUES (1, 100, '2026-02-03', 'mine')", []).unwrap();
    target
}

/// Each change makes a file the app could not have written. The foreign keys refuse the first group, the checks of the columns the rest. Every one
/// is refused whole, and nothing changes: the stashes, the entries and every other table are as they were.
#[test]
fn a_file_that_breaks_the_foreign_keys_or_the_rules_of_the_stashes_is_refused_whole_and_changes_nothing() {
    let base = dump(&with_stashes()).unwrap();
    type Change = Box<dyn Fn(&mut Snapshot)>;
    let cases: Vec<(&str, Change)> = vec![
        // the foreign keys
        ("an entry in a stash that is not there", Box::new(|s| set(s, "stash_entries", 0, "stash_id", json!(99)))),
        ("an entry in the stash after the last one", Box::new(|s| set(s, "stash_entries", 3, "stash_id", json!(5)))),
        ("an entry of no stash", Box::new(|s| set(s, "stash_entries", 0, "stash_id", Value::Null))),
        ("an entry that came with a transfer that is not there", Box::new(|s| set(s, "stash_entries", 0, "transfer_id", json!(99)))),
        ("an entry with a transfer that is a number with decimals", Box::new(|s| set(s, "stash_entries", 0, "transfer_id", json!(1.5)))),
        ("a stash in an account that is not there", Box::new(|s| set(s, "stashes", 0, "account_id", json!(99)))),
        ("a stash of no account", Box::new(|s| set(s, "stashes", 0, "account_id", Value::Null))),
        ("a stash in an account that is a text", Box::new(|s| set(s, "stashes", 1, "account_id", json!("two")))),
        ("entries and no stashes", Box::new(|s| {
            s.tables.insert("stashes".into(), Vec::new());
        })),
        ("a link to a transfer when the file has no transfers", Box::new(|s| {
            s.tables.insert("transfers".into(), Vec::new());
        })),
        ("stashes in accounts when the file has only the main account", Box::new(|s| {
            let main = table(s, "accounts")[0].clone();
            s.tables.insert("accounts".into(), vec![main]);
        })),
        // the checks of the columns
        ("a stash with no name", Box::new(|s| set(s, "stashes", 0, "name", json!("")))),
        ("a name that is too long", Box::new(|s| set(s, "stashes", 0, "name", json!("x".repeat(51))))),
        ("a target of nothing", Box::new(|s| set(s, "stashes", 0, "target_minor", json!(0)))),
        ("a target that is too big", Box::new(|s| set(s, "stashes", 0, "target_minor", json!(1_000_000_000_000_i64)))),
        ("a negative target", Box::new(|s| set(s, "stashes", 0, "target_minor", json!(-1)))),
        ("a target date that is not one", Box::new(|s| set(s, "stashes", 0, "target_date", json!("2026-13-01")))),
        ("a target date in the wrong shape", Box::new(|s| set(s, "stashes", 0, "target_date", json!("1/2/2027")))),
        ("empty notes", Box::new(|s| set(s, "stashes", 0, "notes", json!("")))),
        ("notes that are too long", Box::new(|s| set(s, "stashes", 0, "notes", json!("x".repeat(1001))))),
        ("a stash that is archived two", Box::new(|s| set(s, "stashes", 0, "archived", json!(2)))),
        ("a stash with a sync id that is not one", Box::new(|s| set(s, "stashes", 0, "sync_id", json!("AB".repeat(16))))),
        ("two stashes with one sync id", Box::new(|s| {
            let id = table(s, "stashes")[0]["sync_id"].clone();
            set(s, "stashes", 1, "sync_id", id);
        })),
        ("two stashes with one id", Box::new(|s| set(s, "stashes", 1, "id", json!(1)))),
        ("an entry of nothing", Box::new(|s| set(s, "stash_entries", 0, "amount_minor", json!(0)))),
        ("an entry that is too big", Box::new(|s| set(s, "stash_entries", 0, "amount_minor", json!(1_000_000_000_000_i64)))),
        ("a take that is too big", Box::new(|s| set(s, "stash_entries", 1, "amount_minor", json!(-1_000_000_000_000_i64)))),
        ("an entry of text", Box::new(|s| set(s, "stash_entries", 0, "amount_minor", json!("many")))),
        ("an entry with decimals", Box::new(|s| set(s, "stash_entries", 0, "amount_minor", json!(1.5)))),
        ("an entry that is not dated", Box::new(|s| set(s, "stash_entries", 0, "entry_date", json!("2026-02-30")))),
        ("an entry with no date", Box::new(|s| set(s, "stash_entries", 0, "entry_date", Value::Null))),
        ("an entry with an empty note", Box::new(|s| set(s, "stash_entries", 0, "note", json!("")))),
        ("an entry with a long note", Box::new(|s| set(s, "stash_entries", 0, "note", json!("x".repeat(201))))),
        ("an entry with a sync id that is not one", Box::new(|s| set(s, "stash_entries", 0, "sync_id", json!("not a sync id")))),
        ("two entries with one sync id", Box::new(|s| {
            let id = table(s, "stash_entries")[0]["sync_id"].clone();
            set(s, "stash_entries", 1, "sync_id", id);
        })),
        ("a column that does not exist", Box::new(|s| set(s, "stashes", 0, "colour", json!("red")))),
    ];
    for (label, change) in cases {
        let mut snapshot = base.clone();
        change(&mut snapshot);
        let mut target = with_its_own_data();
        let before = dump(&target).unwrap();
        let marks = snapshot_of(&target);
        let file = seal(&snapshot, PASSPHRASE).unwrap();
        assert_eq!(restore(&mut target, &file), Err(AppError::InvalidBackup), "{label}");
        assert_eq!(dump(&target).unwrap(), before, "{label}: nothing changed");
        assert_eq!(snapshot_of(&target), marks, "{label}: no stamp, marker or history line moved");
        assert!(accounts::check(&target).unwrap(), "{label}");
    }
}

fn snapshot_of(conn: &Connection) -> String {
    snapshot(conn)
}

/// A file is read in a scratch database first; `commit` is what writes to the real one. A file that got past that look is still refused by the
/// foreign keys, in the one transaction that empties the tables and fills them again: all of it is undone and the checks stand again.
#[test]
fn the_foreign_keys_refuse_the_file_in_the_one_transaction_that_writes_it_to_the_real_database() {
    let base = dump(&with_stashes()).unwrap();
    type Change = Box<dyn Fn(&mut Snapshot)>;
    let cases: Vec<(&str, Change)> = vec![
        ("an entry in a stash that is not there", Box::new(|s| set(s, "stash_entries", 2, "stash_id", json!(99)))),
        ("an entry that came with a transfer that is not there", Box::new(|s| set(s, "stash_entries", 2, "transfer_id", json!(99)))),
        ("a stash in an account that is not there", Box::new(|s| set(s, "stashes", 1, "account_id", json!(99)))),
    ];
    for (label, change) in cases {
        let mut snapshot = base.clone();
        change(&mut snapshot);
        let mut target = with_its_own_data();
        let before = dump(&target).unwrap();
        assert_eq!(replace_all(&mut target, &snapshot.tables), Err(AppError::InvalidBackup), "{label}");
        assert_eq!(dump(&target).unwrap(), before, "{label}: the transaction was dropped, so the tables are as they were");
        assert_eq!(count(&target, "SELECT applying FROM sync_clock"), 0, "{label}: the checks stand again");
        assert!(accounts::check(&target).unwrap(), "{label}");
    }
}

#[test]
fn the_stash_tables_come_after_the_tables_they_point_at_and_a_restore_empties_them_before_their_parents() {
    let at = |name: &str| TABLES.iter().position(|t| *t == name).unwrap_or_else(|| panic!("{name} is not listed"));
    for (first, then) in [("accounts", "stashes"), ("transfers", "stash_entries"), ("stashes", "stash_entries"), ("transfers", "stashes")] {
        assert!(at(first) < at(then), "{first} before {then}");
    }
    // a restore deletes in the opposite order, so the entries go before their stashes and the stashes before the accounts and transfers they point at
    let reversed: Vec<&str> = TABLES.iter().rev().copied().collect();
    let place = |name: &str| reversed.iter().position(|t| *t == name).unwrap();
    assert!(place("stash_entries") < place("stashes") && place("stashes") < place("accounts") && place("stash_entries") < place("transfers"));
    // and the tables are the ones of the schema, with their rows in the file
    let dumped = dump(&with_stashes()).unwrap();
    for name in ["stashes", "stash_entries"] {
        assert!(dumped.tables.contains_key(name) && !dumped.rows(name).is_empty(), "{name}");
    }
}

/// A database as it was before stashes, at schema version `version`: a category and a transaction, and from version 18 on (accounts) a savings
/// account and two transfers, the second of which was deleted, so that its marker is in the file.
fn old_database(version: u32) -> Connection {
    let old = db_at(version);
    for sql in [
        "INSERT INTO settings (id, currency) VALUES (1, 'EUR')",
        "INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')",
        "INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (1, 'Dinner', 4000, '2026-01-01')",
    ] {
        old.execute(sql, []).unwrap();
    }
    if version >= 18 {
        for sql in [
            "INSERT INTO accounts (name, kind, opening_balance_minor, opening_date) VALUES ('Savings', 'SAVINGS', 10000, '2026-01-01')",
            "INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date, note) VALUES (1, 2, 500, '2026-02-01', 'kept')",
            "INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date, note) VALUES (1, 2, 700, '2026-02-02', 'gone')",
            "DELETE FROM transfers WHERE note = 'gone'",
        ] {
            old.execute(sql, []).unwrap();
        }
    }
    old
}

#[test]
fn a_backup_made_before_stashes_restores_and_is_migrated_with_no_stashes() {
    // 20 is the version just before stashes; 17 is before accounts, 11 before the debts
    for version in [11, 17, 20] {
        let snapshot = dump(&old_database(version)).unwrap();
        assert_eq!(snapshot.schema_version, version);
        assert!(!snapshot.tables.contains_key("stashes") && !snapshot.tables.contains_key("stash_entries"), "version {version} has neither table");

        // the person had stashes of their own on this device, which the restore replaces
        let mut target = with_stashes();
        restore(&mut target, &seal(&snapshot, PASSPHRASE).unwrap()).unwrap_or_else(|e| panic!("version {version}: {e:?}"));

        let after = dump(&target).unwrap();
        assert_eq!(after.schema_version, migrate::latest_version(), "version {version}: the database is a current one");
        assert!(after.rows("stashes").is_empty() && after.rows("stash_entries").is_empty(), "version {version}: no stashes");
        assert_eq!(after.rows("transactions").len(), 1, "version {version}");
        assert_eq!(after.rows("transactions")[0]["name"], "Dinner");
        assert!(accounts::check(&target).unwrap());
        if version >= 18 {
            assert_eq!((after.rows("accounts").len(), after.rows("transfers").len()), (2, 1), "version {version}");
            let markers: Vec<&Row> = after.rows("sync_deleted").iter().filter(|r| r["kind"] == json!("transfers")).collect();
            assert_eq!(markers.len(), 1, "version {version}: the marker of the transfer that was deleted came through the rebuilt table");
        }
        // the newer tables work in what was restored: the kinds of marker they bring, a stash, an entry that points at a transfer, and the delta rule
        let a_stamp = "0000000000001-0000-abcdef01";
        for kind in ["stashes", "stash_entries"] {
            target.execute("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES (lower(hex(randomblob(16))), ?1, ?2)", [kind, a_stamp]).unwrap_or_else(|e| panic!("version {version}: {kind}: {e}"));
        }
        let main = accounts::get(&target, MAIN_ID, TODAY).unwrap();
        assert_eq!((main.set_aside.as_str(), main.free.as_str()), ("0.00", main.balance.as_str()), "version {version}: nothing is set aside");
        let made = stashes::create(&target, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: "New".into(), account_id: MAIN_ID.to_string(), target: None, target_date: None, notes: None }, TODAY).unwrap();
        assert_eq!(made.id, 1, "version {version}");
        target.execute("INSERT INTO stash_entries (stash_id, amount_minor, entry_date) VALUES (1, 100, '2026-03-01')", []).unwrap();
        assert!(stashes::list(&target, None, TODAY).unwrap().iter().all(|s| s.saved == "1.00"));
    }
}

#[test]
fn a_backup_from_before_stashes_that_never_had_rows_restores_to_a_fresh_database() {
    let snapshot = dump(&db_at(20)).unwrap();
    let mut target = with_stashes();
    restore_snapshot(&mut target, &snapshot).unwrap();
    let after = dump(&target).unwrap();
    for table in TABLES.iter().filter(|table| **table != "accounts") {
        assert!(after.rows(table).is_empty(), "{table}");
    }
    assert_eq!(after.rows("accounts"), dump(&db()).unwrap().rows("accounts"));
}

fn settings() -> Settings {
    Settings { currency: "eur".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() }
}

fn vault(dir: &Path) -> Vault {
    Vault::new(dir.to_path_buf(), Box::new(MemoryStore::default()))
}

/// Stashes in the database of a vault: one with an add that came with a transfer, and one that is archived.
fn fill(v: &Vault) {
    v.with_conn(|conn| {
        let savings = account(conn, "Savings");
        transfer(conn, MAIN_ID, savings, "50.00", "2026-02-02");
        let holiday = stashes::create(conn, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: "Holiday".into(), account_id: savings.to_string(), target: Some("200.00".into()), target_date: None, notes: Some("two\nlines".into()) }, TODAY)?.id;
        conn.execute("INSERT INTO stash_entries (stash_id, amount_minor, entry_date, note, transfer_id) VALUES (?1, 2000, '2026-02-02', 'first', 1)", [holiday])?;
        let bike = stashes::create(conn, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: "Bike".into(), account_id: savings.to_string(), target: None, target_date: None, notes: None }, TODAY)?.id;
        stashes::archive(conn, bike, TODAY)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_all_my_data_takes_the_stashes_and_their_entries_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let v = vault(dir.path());
    v.create(&settings(), None).unwrap();
    fill(&v);
    assert_eq!(v.with_conn(|c| Ok((count(c, "SELECT count(*) FROM stashes"), count(c, "SELECT count(*) FROM stash_entries")))).unwrap(), (2, 1));

    assert_eq!(v.wipe("delete"), Err(AppError::InvalidInput));
    assert_eq!(v.with_conn(|c| Ok(count(c, "SELECT count(*) FROM stashes"))).unwrap(), 2, "a refused wipe changes nothing");

    // the wipe removes the database file and its key, so no row is left to list in a table: a new vault has none
    v.wipe("DELETE").unwrap();
    v.create(&settings(), None).unwrap();
    let now = v.with_conn(|c| dump(c)).unwrap();
    for table in ["stashes", "stash_entries", "transfers"] {
        assert!(now.rows(table).is_empty(), "{table}");
    }
    assert_eq!(now.rows("accounts"), dump(&db()).unwrap().rows("accounts"), "the main account of the migration, and no other");
    v.with_conn(|c| {
        // and the next stash made is the first, as in any new database
        let first = stashes::create(c, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: "First".into(), account_id: MAIN_ID.to_string(), target: None, target_date: None, notes: None }, TODAY)?;
        assert_eq!(first.id, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_restore_through_the_vault_brings_the_stashes_and_a_file_that_breaks_a_foreign_key_is_refused_and_the_data_stays() {
    let dir = tempfile::tempdir().unwrap();
    let v = vault(dir.path());
    v.create(&settings(), None).unwrap();
    fill(&v);
    let before = v.with_conn(|c| dump(c)).unwrap();
    let marks = v.with_conn(|c| Ok(snapshot(c))).unwrap();

    let entry = json!({ "id": 1, "stash_id": 99, "amount_minor": 100, "entry_date": "2026-01-01" });
    let stash = json!({ "id": 1, "name": "Lost", "account_id": 99 });
    for tables in [
        json!({ "accounts": main_account(), "stash_entries": [entry] }),
        json!({ "accounts": main_account(), "stashes": [stash] }),
        json!({ "accounts": main_account(), "stashes": [{ "id": 1, "name": "Held", "account_id": 1 }], "stash_entries": [{ "id": 1, "stash_id": 1, "amount_minor": 100, "entry_date": "2026-01-01", "transfer_id": 7 }] }),
    ] {
        assert_eq!(v.restore_backup(&forged(tables.clone()), PASSPHRASE, "RESTORE"), Err(AppError::InvalidBackup), "{tables}");
        assert_eq!(v.with_conn(|c| dump(c)).unwrap(), before, "{tables}: the data stays");
        assert_eq!(v.with_conn(|c| Ok(snapshot(c))).unwrap(), marks, "{tables}: and no stamp moved");
    }
    // a good one is taken, and the stashes are what the file held
    let source = with_stashes();
    let file = seal(&dump(&source).unwrap(), PASSPHRASE).unwrap();
    v.restore_backup(&file, PASSPHRASE, "RESTORE").unwrap();
    let after = v.with_conn(|c| dump(c)).unwrap();
    let held = dump(&source).unwrap();
    assert_eq!((after.rows("stashes"), after.rows("stash_entries")), (held.rows("stashes"), held.rows("stash_entries")));
    assert_eq!(v.with_conn(|c| Ok(stashes::list(c, None, TODAY)?.len())).unwrap(), 4);
}
