//! The database itself must keep the rules of stashes and their entries, whatever the code around it does (migration 0021,
//! `docs/stash-design.md`, section 4).

use std::collections::BTreeMap;

use rusqlite::{params, Connection};

use super::fixtures::*;
use crate::sync::engine::UNSET_STAMP;

fn exec(conn: &Connection, sql: &str) -> rusqlite::Result<usize> {
    conn.execute(sql, [])
}

fn audit(conn: &Connection, entity: &str, action: &str) -> i64 {
    conn.query_row("SELECT count(*) FROM audit_log WHERE entity_type = ?1 AND action = ?2", params![entity, action], |r| r.get(0)).unwrap()
}

fn updated_at_of(conn: &Connection, table: &str, id: i64) -> String {
    text(conn, &format!("SELECT updated_at FROM {table} WHERE id = {id}"))
}

/// Sets `updated_at` to a day long ago (a write the triggers leave alone), so that a later move of it can be seen.
fn age(conn: &Connection, table: &str, id: i64) {
    conn.execute(&format!("UPDATE {table} SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = ?1"), [id]).unwrap();
}

const OLD: &str = "2000-01-01T00:00:00.000Z";

fn clock(conn: &Connection) -> String {
    text(conn, "SELECT last_ms || '-' || last_counter FROM sync_clock WHERE id = 1")
}

/// A stash written straight into the table, in the main account. Returns its id.
fn raw_stash(conn: &Connection, name: &str) -> i64 {
    conn.execute("INSERT INTO stashes (name, account_id) VALUES (?1, 1)", [name]).unwrap();
    conn.last_insert_rowid()
}

fn raw_entry(conn: &Connection, stash: i64) -> i64 {
    conn.execute("INSERT INTO stash_entries (stash_id, amount_minor, entry_date) VALUES (?1, 500, '2026-03-01')", [stash]).unwrap();
    conn.last_insert_rowid()
}

fn raw_account(conn: &Connection, name: &str) -> i64 {
    conn.execute("INSERT INTO accounts (name, kind, opening_date) VALUES (?1, 'CASH', '2026-01-01')", [name]).unwrap();
    conn.last_insert_rowid()
}

fn raw_transfer(conn: &Connection, from: i64, to: i64) -> i64 {
    conn.execute("INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date) VALUES (?1, ?2, 500, '2026-03-01')", [from, to]).unwrap();
    conn.last_insert_rowid()
}

/// Every trigger of the database and its text.
fn triggers(conn: &Connection) -> BTreeMap<String, String> {
    conn.prepare("SELECT name, sql FROM sqlite_master WHERE type = 'trigger'")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn at_version_20() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::migrate::run_up_to(&mut conn, 20).unwrap();
    conn
}

// ---- the migration ---------------------------------------------------------------------------

#[test]
fn a_new_database_has_both_tables_every_index_every_trigger_and_no_row() {
    let w = world();
    for name in [
        "stashes", "stash_entries", "uq_stashes_sync_id", "idx_stashes_account", "uq_stash_entries_sync_id", "idx_stash_entries_stash", "idx_stash_entries_transfer",
        "trg_stashes_updated_at", "trg_stashes_sync_insert", "trg_stashes_sync_update", "trg_stashes_sync_delete", "trg_stashes_audit_insert", "trg_stashes_audit_update",
        "trg_stashes_audit_delete", "trg_stash_entries_updated_at", "trg_stash_entries_sync_insert", "trg_stash_entries_sync_update", "trg_stash_entries_sync_delete",
        "trg_stash_entries_audit_insert", "trg_stash_entries_audit_update", "trg_stash_entries_audit_delete", "trg_transfers_sync_unlink", "uq_sync_deleted_sync_id",
    ] {
        assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM sqlite_master WHERE name = '{name}'")), 1, "{name}");
    }
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sqlite_master WHERE name = 'sync_deleted_new'"), 0);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM stashes"), 0);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM stash_entries"), 0);
    assert_eq!(one(&w.conn, "PRAGMA user_version"), 21);
    assert_eq!(i64::from(crate::migrate::latest_version()), 21);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM audit_log WHERE entity_type IN ('stashes', 'stash_entries')"), 0);
    // the name has no unique index (the code checks it on this device only), and the index that looks entries up by transfer holds only links
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = 'stashes' AND sql LIKE '%name%'"), 0);
    let index = text(&w.conn, "SELECT sql FROM sqlite_master WHERE name = 'idx_stash_entries_transfer'");
    assert!(index.contains("WHERE transfer_id IS NOT NULL"), "{index}");
    assert!(text(&w.conn, "SELECT sql FROM sqlite_master WHERE name = 'idx_stash_entries_stash'").contains("(stash_id, entry_date, id)"));
}

#[test]
fn the_triggers_that_write_markers_at_version_20_are_the_eight_of_the_design_and_come_back_as_they_were() {
    let mut conn = at_version_20();
    let before = triggers(&conn);
    let writers: Vec<&str> = before.iter().filter(|(_, sql)| sql.contains("sync_deleted")).map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        writers,
        [
            "trg_accounts_sync_delete", "trg_budgets_sync_delete", "trg_document_links_sync_delete", "trg_documents_sync_delete", "trg_ledger_entries_sync_delete",
            "trg_recurring_transactions_sync_delete", "trg_transactions_sync_delete", "trg_transfers_sync_delete",
        ],
        "section 4.1 lists every one of them"
    );

    crate::migrate::run(&mut conn).unwrap();

    let after = triggers(&conn);
    for (name, sql) in &before {
        if name == "trg_transfers_sync_delete" {
            continue;
        }
        assert_eq!(after.get(name), Some(sql), "{name} is as it was, byte for byte");
    }
    // the one that is not: the marker of a transfer takes the tick of `trg_transfers_sync_unlink` instead of making one of its own
    let transfers_delete = &after["trg_transfers_sync_delete"];
    assert!(!transfers_delete.contains("UPDATE sync_clock") && transfers_delete.contains("'transfers'"), "{transfers_delete}");
    assert!(after["trg_transfers_sync_unlink"].contains("UPDATE sync_clock") && after["trg_transfers_sync_unlink"].contains("BEFORE DELETE"));
    let new: Vec<&String> = after.keys().filter(|name| !before.contains_key(*name)).collect();
    assert_eq!(new.len(), 14 + 1, "fourteen triggers of the two tables and the unlink: {new:?}");
}

#[test]
fn the_migration_on_a_database_with_rows_moves_nothing_and_every_marker_trigger_still_works() {
    let mut conn = at_version_20();
    conn.execute_batch(
        "INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food'), ('INCOME', 'Salary');
         INSERT INTO people (name) VALUES ('Anna');
         INSERT INTO accounts (name, kind, opening_balance_minor, opening_date) VALUES ('Cash', 'CASH', 5000, '2026-01-01'), ('Savings', 'SAVINGS', 0, '2026-01-01');
         INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date) VALUES (1, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-02-01');
         INSERT INTO transactions (category_id, name, amount_minor, payment_date, account_id) VALUES (1, 'Shop', 1250, '2026-01-10', 2), (2, 'Pay', 200000, '2026-01-31', 1), (1, 'Gone', 100, '2026-01-11', 1);
         INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date) VALUES (1, 'LENT', 2000, '2026-01-12'), (1, 'REPAID_TO_ME', 500, '2026-01-20');
         INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date) VALUES (1, 2, 700, '2026-02-01'), (2, 3, 50, '2026-02-02'), (1, 3, 9, '2026-02-03');
         INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from) VALUES ('EXPENSE', 'GLOBAL', NULL, 100000, '2026-01-01');
         DELETE FROM transactions WHERE name = 'Gone';
         DELETE FROM ledger_entries WHERE id = 2;
         DELETE FROM transfers WHERE id = 3;
         DELETE FROM budgets;
         INSERT INTO accounts (name, kind, opening_date) VALUES ('Gone', 'CASH', '2026-01-01');
         DELETE FROM accounts WHERE name = 'Gone';",
    )
    .unwrap();
    // everything a write would move: the stamps and `updated_at` of the rows, the markers, the history and the clock
    let state = |conn: &Connection| -> Vec<String> {
        let mut all = Vec::new();
        for sql in [
            "SELECT id || ' ' || sync_id || ' ' || stamp || ' ' || updated_at || ' ' || name || ' ' || amount_minor FROM transactions ORDER BY id",
            "SELECT id || ' ' || sync_id || ' ' || stamp || ' ' || updated_at || ' ' || name FROM recurring_transactions ORDER BY id",
            "SELECT id || ' ' || sync_id || ' ' || stamp || ' ' || updated_at || ' ' || kind FROM ledger_entries ORDER BY id",
            "SELECT id || ' ' || sync_id || ' ' || stamp || ' ' || updated_at || ' ' || name || ' ' || opening_balance_minor FROM accounts ORDER BY id",
            "SELECT id || ' ' || sync_id || ' ' || stamp || ' ' || updated_at || ' ' || amount_minor FROM transfers ORDER BY id",
            "SELECT id || ' ' || sync_id || ' ' || kind || ' ' || stamp FROM sync_deleted ORDER BY id",
            "SELECT count(*) || ' rows of history' FROM audit_log",
            "SELECT last_ms || ' ' || last_counter FROM sync_clock",
        ] {
            all.extend(conn.prepare(sql).unwrap().query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap));
        }
        all
    };
    let before = state(&conn);
    assert_eq!(one(&conn, "SELECT count(*) FROM sync_deleted"), 5, "a marker of each kind that existed, and some rows left: {before:?}");

    crate::migrate::run(&mut conn).unwrap();

    assert_eq!(state(&conn), before, "no stamp, no updated_at, no marker and no history row moved");
    assert!(conn.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, bool>(0)).unwrap(), "foreign keys are back on");
    assert_eq!(one(&conn, "PRAGMA user_version"), 21);
    assert_eq!(one(&conn, "SELECT count(*) FROM stashes") + one(&conn, "SELECT count(*) FROM stash_entries"), 0, "no stashes");
    assert_eq!(one(&conn, "SELECT count(*) FROM audit_log WHERE entity_type IN ('stashes', 'stash_entries')"), 0);
    assert!(conn.prepare("PRAGMA foreign_key_check").unwrap().query([]).unwrap().next().unwrap().is_none());
    // the account that was there is as it was and its balance is the same
    assert_eq!(balance(&conn, 2, "2026-12-31"), 5000 - 1250 + 700 - 50);

    // the triggers that were made again still leave their markers, kind by kind, with a stamp of the clock
    let anna = 1;
    let before_markers = one(&conn, "SELECT count(*) FROM sync_deleted");
    conn.execute_batch(&format!(
        "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from) VALUES ('EXPENSE', 'GLOBAL', NULL, 1000, '2026-01-01');
         INSERT INTO documents (name, media_type, size_bytes, sha256, document_date) VALUES ('a.pdf', 'application/pdf', 10, '{sha}', '2026-01-01');
         INSERT INTO document_links (document_id, transaction_id) VALUES (1, 1);
         INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date) VALUES ({anna}, 'LENT', 1, '2026-01-12');
         INSERT INTO accounts (name, kind, opening_date) VALUES ('Spare', 'CASH', '2026-01-01');",
        sha = "a".repeat(64)
    ))
    .unwrap();
    for table in ["document_links", "documents", "ledger_entries", "transactions", "recurring_transactions", "budgets", "transfers", "accounts"] {
        // the oldest row of the table, except for accounts, where the oldest is the main account that is never deleted
        let which = if table == "accounts" { "name = 'Spare'" } else { "id = (SELECT MIN(id) FROM TABLE)" }.replace("TABLE", table);
        let sync_id = text(&conn, &format!("SELECT sync_id FROM {table} WHERE {which}"));
        exec(&conn, &format!("DELETE FROM {table} WHERE {which}")).unwrap();
        let stamp = text(&conn, &format!("SELECT stamp FROM sync_deleted WHERE sync_id = '{sync_id}' AND kind = '{table}'"));
        assert_eq!(stamp.len(), 27, "{table}: {stamp}");
    }
    assert_eq!(one(&conn, "SELECT count(*) FROM sync_deleted"), before_markers + 8);
}

#[test]
fn markers_can_be_of_a_stash_and_of_an_entry_and_of_nothing_unknown() {
    let w = world();
    for (n, kind) in ["budgets", "recurring_transactions", "transactions", "documents", "document_links", "ledger_entries", "accounts", "transfers", "stashes", "stash_entries"].iter().enumerate() {
        w.conn.execute("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES (?1, ?2, 'x')", params![format!("{n:032x}"), kind]).unwrap();
    }
    for kind in ["people", "categories", "stash", "STASHES", "stash_entry", ""] {
        assert!(exec(&w.conn, &format!("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES ('{}', '{kind}', 'x')", "f".repeat(32))).is_err(), "{kind}");
    }
    assert!(exec(&w.conn, &format!("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES ('{:032x}', 'stashes', 'x')", 0)).is_err(), "one marker for a sync id");
    assert!(exec(&w.conn, "INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES ('short', 'stashes', 'x')").is_err());
}

// ---- stashes ---------------------------------------------------------------------------------

#[test]
fn a_stash_has_a_name_of_one_to_fifty_characters_and_two_may_share_one() {
    let w = world();
    let insert = |name: &str| w.conn.execute("INSERT INTO stashes (name, account_id) VALUES (?1, 1)", [name]);
    assert!(insert("").is_err());
    assert!(insert(&"x".repeat(51)).is_err());
    assert!(insert(&"x".repeat(50)).is_ok());
    assert!(insert("Zoë 日本語").is_ok());
    assert!(insert("Holiday").is_ok());
    assert!(insert("Holiday").is_ok(), "the name is checked by the code on this device, not by an index");
    assert!(insert("holiday").is_ok());
    assert!(exec(&w.conn, "INSERT INTO stashes (account_id) VALUES (1)").is_err(), "a name is needed");
}

#[test]
fn a_stash_is_in_an_account_that_exists_and_that_account_is_not_deleted_while_it_is_there() {
    let w = world();
    let cash = raw_account(&w.conn, "Cash");
    assert!(exec(&w.conn, "INSERT INTO stashes (name, account_id) VALUES ('A', 99)").unwrap_err().to_string().contains("FOREIGN KEY"));
    assert!(exec(&w.conn, "INSERT INTO stashes (name) VALUES ('A')").is_err(), "no default account");
    exec(&w.conn, &format!("INSERT INTO stashes (name, account_id) VALUES ('A', {cash})")).unwrap();
    assert!(exec(&w.conn, &format!("DELETE FROM accounts WHERE id = {cash}")).unwrap_err().to_string().contains("FOREIGN KEY"), "NO ACTION: the account stays");
    assert!(exec(&w.conn, "UPDATE stashes SET account_id = 99").is_err(), "nor can the stash be moved to one that is not there");
    assert!(exec(&w.conn, "UPDATE stashes SET account_id = NULL").is_err());
    exec(&w.conn, &format!("UPDATE stashes SET account_id = 1 WHERE account_id = {cash}")).unwrap();
    assert_eq!(exec(&w.conn, &format!("DELETE FROM accounts WHERE id = {cash}")).unwrap(), 1, "empty again, so it can go");
}

#[test]
fn a_target_is_absent_or_between_one_cent_and_the_largest_amount() {
    let w = world();
    let id = raw_stash(&w.conn, "A");
    assert_eq!(one(&w.conn, &format!("SELECT target_minor IS NULL FROM stashes WHERE id = {id}")), 1);
    for ok in ["1", "999999999999", "120000", "NULL"] {
        assert!(exec(&w.conn, &format!("UPDATE stashes SET target_minor = {ok} WHERE id = {id}")).is_ok(), "{ok}");
    }
    for bad in ["0", "-1", "1000000000000", "1.5", "'twelve'"] {
        assert!(exec(&w.conn, &format!("UPDATE stashes SET target_minor = {bad} WHERE id = {id}")).is_err(), "{bad}");
    }
}

#[test]
fn a_target_date_is_absent_or_a_real_padded_date_in_any_year() {
    let w = world();
    let id = raw_stash(&w.conn, "A");
    for ok in ["2027-06-30", "2020-02-29", "1999-01-01", "2026-11-13", "NULL"] {
        let value = if ok == "NULL" { ok.to_string() } else { format!("'{ok}'") };
        assert!(exec(&w.conn, &format!("UPDATE stashes SET target_date = {value} WHERE id = {id}")).is_ok(), "{ok}");
    }
    for bad in ["2026-02-30", "2026-2-3", "today", "20260101", "2026-13-01", "", "2026-01-01 ", "2027-02-29"] {
        assert!(exec(&w.conn, &format!("UPDATE stashes SET target_date = '{bad}' WHERE id = {id}")).is_err(), "{bad:?}");
    }
}

#[test]
fn stash_notes_are_absent_or_one_to_a_thousand_characters_and_archived_is_yes_or_no() {
    let w = world();
    let id = raw_stash(&w.conn, "A");
    assert!(exec(&w.conn, &format!("UPDATE stashes SET notes = '' WHERE id = {id}")).is_err());
    assert!(exec(&w.conn, &format!("UPDATE stashes SET notes = '{}' WHERE id = {id}", "x".repeat(1001))).is_err());
    assert!(exec(&w.conn, &format!("UPDATE stashes SET notes = '{}' WHERE id = {id}", "x".repeat(1000))).is_ok());
    assert!(exec(&w.conn, &format!("UPDATE stashes SET notes = 'two\nlines' WHERE id = {id}")).is_ok());
    assert!(exec(&w.conn, &format!("UPDATE stashes SET notes = NULL WHERE id = {id}")).is_ok());
    assert_eq!(one(&w.conn, &format!("SELECT archived FROM stashes WHERE id = {id}")), 0, "not archived when not said");
    assert_eq!(one(&w.conn, &format!("SELECT position FROM stashes WHERE id = {id}")), 0, "position 0 when not said");
    for bad in ["2", "-1", "NULL", "'yes'"] {
        assert!(exec(&w.conn, &format!("UPDATE stashes SET archived = {bad} WHERE id = {id}")).is_err(), "{bad}");
    }
    assert!(exec(&w.conn, &format!("UPDATE stashes SET archived = 1 WHERE id = {id}")).is_ok());
    assert!(exec(&w.conn, &format!("UPDATE stashes SET position = 7 WHERE id = {id}")).is_ok());
    assert!(exec(&w.conn, &format!("UPDATE stashes SET position = NULL WHERE id = {id}")).is_err());
    let made = text(&w.conn, &format!("SELECT created_at || ' ' || updated_at FROM stashes WHERE id = {id}"));
    assert!(made.len() == 49 && made.starts_with("20"), "created_at and updated_at have the clock's text: {made}");
}

#[test]
fn the_sync_id_of_a_stash_and_of_an_entry_is_32_hex_characters_and_unique() {
    let w = world();
    let a = raw_stash(&w.conn, "A");
    let b = raw_stash(&w.conn, "B");
    let x = raw_entry(&w.conn, a);
    let y = raw_entry(&w.conn, a);
    for (table, first, second) in [("stashes", a, b), ("stash_entries", x, y)] {
        let made = text(&w.conn, &format!("SELECT sync_id FROM {table} WHERE id = {first}"));
        assert!(made.len() == 32 && made.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{made}");
        for bad in ["abc".to_string(), "G".repeat(32), "A".repeat(32), "a".repeat(33)] {
            assert!(exec(&w.conn, &format!("UPDATE {table} SET sync_id = '{bad}' WHERE id = {first}")).is_err(), "{table} {bad}");
        }
        assert!(exec(&w.conn, &format!("UPDATE {table} SET sync_id = '{made}' WHERE id = {second}")).is_err(), "{table}: two rows with one sync id");
    }
}

// ---- entries ---------------------------------------------------------------------------------

#[test]
fn an_entry_is_an_amount_other_than_zero_inside_the_range_on_a_real_date_with_a_short_note() {
    let w = world();
    let stash = raw_stash(&w.conn, "A");
    let id = raw_entry(&w.conn, stash);
    for ok in ["1", "-1", "999999999999", "-999999999999"] {
        assert!(exec(&w.conn, &format!("UPDATE stash_entries SET amount_minor = {ok} WHERE id = {id}")).is_ok(), "{ok}");
    }
    for bad in ["0", "1000000000000", "-1000000000000", "1.5", "'twelve'", "NULL"] {
        assert!(exec(&w.conn, &format!("UPDATE stash_entries SET amount_minor = {bad} WHERE id = {id}")).is_err(), "{bad}");
    }
    for bad in ["2026-02-30", "2026-2-3", "today", "20260101", ""] {
        assert!(exec(&w.conn, &format!("UPDATE stash_entries SET entry_date = '{bad}' WHERE id = {id}")).is_err(), "{bad}");
    }
    assert!(exec(&w.conn, &format!("UPDATE stash_entries SET entry_date = NULL WHERE id = {id}")).is_err());
    assert!(exec(&w.conn, &format!("UPDATE stash_entries SET entry_date = '2030-05-05' WHERE id = {id}")).is_ok(), "a date in the future is for the code to refuse, a sync may bring it");
    assert!(exec(&w.conn, &format!("UPDATE stash_entries SET note = '' WHERE id = {id}")).is_err());
    assert!(exec(&w.conn, &format!("UPDATE stash_entries SET note = '{}' WHERE id = {id}", "x".repeat(201))).is_err());
    assert!(exec(&w.conn, &format!("UPDATE stash_entries SET note = '{}' WHERE id = {id}", "x".repeat(200))).is_ok());
    assert!(exec(&w.conn, &format!("UPDATE stash_entries SET note = NULL WHERE id = {id}")).is_ok());
    assert!(exec(&w.conn, "INSERT INTO stash_entries (stash_id, entry_date) VALUES (1, '2026-01-01')").is_err(), "an amount is needed");
    assert!(exec(&w.conn, &format!("INSERT INTO stash_entries (stash_id, amount_minor) VALUES ({stash}, 5)")).is_err(), "a date is needed");
}

#[test]
fn an_entry_is_in_a_stash_that_exists_and_goes_with_it() {
    let w = world();
    let stash = raw_stash(&w.conn, "A");
    let other = raw_stash(&w.conn, "B");
    assert!(exec(&w.conn, "INSERT INTO stash_entries (stash_id, amount_minor, entry_date) VALUES (99, 5, '2026-01-01')").unwrap_err().to_string().contains("FOREIGN KEY"));
    assert!(exec(&w.conn, "INSERT INTO stash_entries (amount_minor, entry_date) VALUES (5, '2026-01-01')").is_err());
    let kept = raw_entry(&w.conn, other);
    raw_entry(&w.conn, stash);
    raw_entry(&w.conn, stash);
    assert!(exec(&w.conn, "UPDATE stash_entries SET stash_id = 99").is_err());
    assert_eq!(exec(&w.conn, &format!("DELETE FROM stashes WHERE id = {stash}")).unwrap(), 1);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM stash_entries"), 1, "the entries of a stash that is deleted go with it (ON DELETE CASCADE)");
    assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM stash_entries WHERE id = {kept}")), 1);
}

#[test]
fn the_transfer_of_an_entry_exists_and_the_link_clears_when_the_transfer_is_deleted() {
    let w = world();
    let cash = raw_account(&w.conn, "Cash");
    let stash = raw_stash(&w.conn, "A");
    let transfer = raw_transfer(&w.conn, cash, 1);
    assert!(exec(&w.conn, &format!("INSERT INTO stash_entries (stash_id, amount_minor, entry_date, transfer_id) VALUES ({stash}, 5, '2026-01-01', 99)")).unwrap_err().to_string().contains("FOREIGN KEY"));
    exec(&w.conn, &format!("INSERT INTO stash_entries (stash_id, amount_minor, entry_date, transfer_id) VALUES ({stash}, 500, '2026-01-01', {transfer})")).unwrap();
    assert!(exec(&w.conn, "UPDATE stash_entries SET transfer_id = 99").is_err());
    assert!(exec(&w.conn, &format!("DELETE FROM accounts WHERE id = {cash}")).is_err(), "the transfer keeps its accounts");
    exec(&w.conn, &format!("DELETE FROM transfers WHERE id = {transfer}")).unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM stash_entries"), 1, "the entry stays: the money stays set aside (ON DELETE SET NULL)");
    assert_eq!(one(&w.conn, "SELECT transfer_id IS NULL FROM stash_entries"), 1);
}

// ---- stamps, updated_at, markers and history of the two tables --------------------------------

/// The same story for both tables: `change` is a real change of a row with id `id`, `same` writes the values it has.
fn stamps_follow_real_changes(conn: &Connection, table: &str, id: i64, change: &str, same: &str) {
    let made = stamp_of(conn, table, id);
    assert_eq!(made.len(), 27, "{table}: a new row gets a stamp: {made}");
    assert!(made.as_str() > UNSET_STAMP, "{table}");
    assert!(audit(conn, table, "INSERT") >= 1, "{table}: each new row is in the history");

    age(conn, table, id);
    let history = audit(conn, table, "UPDATE");
    let ticked = clock(conn);
    exec(conn, &format!("UPDATE {table} SET {same} WHERE id = {id}")).unwrap();
    assert_eq!((stamp_of(conn, table, id), updated_at_of(conn, table, id)), (made.clone(), OLD.to_string()), "{table}: the same values again are no change");
    assert_eq!(audit(conn, table, "UPDATE"), history, "{table}");
    assert_eq!(clock(conn), ticked, "{table}: and the clock did not tick");

    exec(conn, &format!("UPDATE {table} SET {change} WHERE id = {id}")).unwrap();
    let changed = stamp_of(conn, table, id);
    assert!(changed > made, "{table}: a real change makes a newer stamp");
    assert_ne!(updated_at_of(conn, table, id), OLD, "{table}: and moves updated_at");
    assert_eq!(audit(conn, table, "UPDATE"), history + 1, "{table}");
    let logged = text(conn, &format!("SELECT new_values FROM audit_log WHERE entity_type = '{table}' AND action = 'UPDATE' ORDER BY id DESC LIMIT 1"));
    assert!(!logged.contains("sync_id") && !logged.contains("stamp"), "{table}: the history keeps no sync column: {logged}");

    // a sync or a restore brings its own stamp and dates
    applying(conn, true);
    age(conn, table, id);
    exec(conn, &format!("UPDATE {table} SET {change} WHERE id = {id}")).unwrap();
    exec(conn, &format!("UPDATE {table} SET stamp = '1760000000000-0001-aaaaaaaa' WHERE id = {id}")).unwrap();
    assert_eq!((stamp_of(conn, table, id), updated_at_of(conn, table, id)), ("1760000000000-0001-aaaaaaaa".to_string(), OLD.to_string()), "{table}: nothing moves while applying");
    applying(conn, false);

    let sync_id = text(conn, &format!("SELECT sync_id FROM {table} WHERE id = {id}"));
    exec(conn, &format!("DELETE FROM {table} WHERE id = {id}")).unwrap();
    let marker = text(conn, &format!("SELECT stamp FROM sync_deleted WHERE sync_id = '{sync_id}' AND kind = '{table}'"));
    assert!(marker > changed, "{table}: the marker is newer than the row was");
    assert_eq!(audit(conn, table, "DELETE"), 1, "{table}");
}

#[test]
fn a_stash_gets_a_stamp_when_it_is_made_and_on_each_real_change_and_leaves_a_marker() {
    let w = world();
    let id = raw_stash(&w.conn, "Holiday");
    stamps_follow_real_changes(
        &w.conn,
        "stashes",
        id,
        "name = 'Trip'",
        "name = name, account_id = account_id, target_minor = target_minor, target_date = target_date, notes = notes, archived = archived, position = position",
    );
    // each column the stamp watches is a change by itself, and a move to another account is one
    let cash = raw_account(&w.conn, "Cash");
    let id = raw_stash(&w.conn, "Holiday");
    for change in [
        "name = 'W'".to_string(),
        format!("account_id = {cash}"),
        "target_minor = 5".to_string(),
        "target_date = '2026-12-12'".to_string(),
        "notes = 'n'".to_string(),
        "archived = 1".to_string(),
        "position = 7".to_string(),
    ] {
        let (before, history) = (stamp_of(&w.conn, "stashes", id), audit(&w.conn, "stashes", "UPDATE"));
        exec(&w.conn, &format!("UPDATE stashes SET {change} WHERE id = {id}")).unwrap();
        assert!(stamp_of(&w.conn, "stashes", id) > before, "{change}");
        assert_eq!(audit(&w.conn, "stashes", "UPDATE"), history + 1, "{change}: one row of history");
    }
    // the clock alone is no change: created_at and updated_at make no stamp, and no history
    let before = (stamp_of(&w.conn, "stashes", id), audit(&w.conn, "stashes", "UPDATE"));
    exec(&w.conn, &format!("UPDATE stashes SET updated_at = '2001-01-01T00:00:00.000Z' WHERE id = {id}")).unwrap();
    assert_eq!((stamp_of(&w.conn, "stashes", id), audit(&w.conn, "stashes", "UPDATE")), before);
}

#[test]
fn an_entry_gets_a_stamp_when_it_is_made_and_on_each_real_change_and_leaves_a_marker() {
    let w = world();
    let stash = raw_stash(&w.conn, "Holiday");
    let id = raw_entry(&w.conn, stash);
    stamps_follow_real_changes(&w.conn, "stash_entries", id, "amount_minor = 700", "stash_id = stash_id, amount_minor = amount_minor, entry_date = entry_date, note = note, transfer_id = transfer_id");
    let other = raw_stash(&w.conn, "Other");
    let id = raw_entry(&w.conn, stash);
    for change in ["stash_id = ".to_string() + &other.to_string(), "amount_minor = -5".to_string(), "entry_date = '2026-05-05'".to_string(), "note = 'n'".to_string()] {
        let (before, history) = (stamp_of(&w.conn, "stash_entries", id), audit(&w.conn, "stash_entries", "UPDATE"));
        exec(&w.conn, &format!("UPDATE stash_entries SET {change} WHERE id = {id}")).unwrap();
        assert!(stamp_of(&w.conn, "stash_entries", id) > before, "{change}");
        assert_eq!(audit(&w.conn, "stash_entries", "UPDATE"), history + 1, "{change}: one row of history");
    }
}

#[test]
fn deleting_an_entry_alone_leaves_a_marker_and_deleting_its_stash_leaves_one_for_the_stash_only() {
    let w = world();
    let stash = raw_stash(&w.conn, "Holiday");
    let (first, second, third) = (raw_entry(&w.conn, stash), raw_entry(&w.conn, stash), raw_entry(&w.conn, stash));
    let ids = text(&w.conn, &format!("SELECT group_concat(sync_id) FROM stash_entries WHERE id IN ({first}, {second}, {third})"));
    let lone = text(&w.conn, &format!("SELECT sync_id FROM stash_entries WHERE id = {first}"));
    exec(&w.conn, &format!("DELETE FROM stash_entries WHERE id = {first}")).unwrap();
    assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM sync_deleted WHERE kind = 'stash_entries' AND sync_id = '{lone}'")), 1, "the stash is there, so the delete is a delete of the entry");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 1);

    let stash_sync_id = text(&w.conn, &format!("SELECT sync_id FROM stashes WHERE id = {stash}"));
    exec(&w.conn, &format!("DELETE FROM stashes WHERE id = {stash}")).unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM stash_entries"), 0, "the cascade removed them");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'stash_entries'"), 1, "and left no marker of their own: only the one of the entry deleted before");
    assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM sync_deleted WHERE kind = 'stashes' AND sync_id = '{stash_sync_id}'")), 1, "the stash's marker covers them");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 2);
    for sync_id in ids.split(',').filter(|id| *id != lone) {
        assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM sync_deleted WHERE sync_id = '{sync_id}'")), 0, "{sync_id}");
    }
    assert_eq!(audit(&w.conn, "stash_entries", "DELETE"), 3, "the history keeps every row that went");
    assert_eq!(audit(&w.conn, "stashes", "DELETE"), 1);
}

#[test]
fn the_triggers_of_stashes_and_entries_stand_aside_while_a_sync_or_a_restore_is_writing() {
    let w = world();
    let stash = raw_stash(&w.conn, "Holiday");
    let entry = raw_entry(&w.conn, stash);
    let other = raw_stash(&w.conn, "Other");
    let sync_ids = (text(&w.conn, &format!("SELECT sync_id FROM stash_entries WHERE id = {entry}")), text(&w.conn, &format!("SELECT sync_id FROM stashes WHERE id = {stash}")));
    let ticked = clock(&w.conn);
    applying(&w.conn, true);
    exec(&w.conn, &format!("DELETE FROM stash_entries WHERE id = {entry}")).unwrap();
    exec(&w.conn, &format!("DELETE FROM stashes WHERE id = {stash}")).unwrap();
    exec(&w.conn, &format!("DELETE FROM stashes WHERE id = {other}")).unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 0, "no marker: {sync_ids:?}");
    assert_eq!(clock(&w.conn), ticked, "and no tick of the clock");
}

// ---- the link to a transfer --------------------------------------------------------------------

#[test]
fn deleting_a_linked_transfer_leaves_the_entry_with_no_link_a_newer_stamp_and_one_row_of_history() {
    let w = world();
    let cash = raw_account(&w.conn, "Cash");
    let stash = raw_stash(&w.conn, "Holiday");
    let transfer = raw_transfer(&w.conn, cash, 1);
    let other_transfer = raw_transfer(&w.conn, cash, 1);
    exec(&w.conn, &format!("INSERT INTO stash_entries (stash_id, amount_minor, entry_date, transfer_id) VALUES ({stash}, 500, '2026-01-01', {transfer})")).unwrap();
    let linked = w.conn.last_insert_rowid();
    exec(&w.conn, &format!("INSERT INTO stash_entries (stash_id, amount_minor, entry_date, transfer_id) VALUES ({stash}, 500, '2026-01-02', {other_transfer})")).unwrap();
    let untouched = w.conn.last_insert_rowid();
    let alone = raw_entry(&w.conn, stash);
    age(&w.conn, "stash_entries", linked);
    let (stamp_before, history) = (stamp_of(&w.conn, "stash_entries", linked), audit(&w.conn, "stash_entries", "UPDATE"));
    let others = (stamp_of(&w.conn, "stash_entries", untouched), stamp_of(&w.conn, "stash_entries", alone));
    let transfer_sync_id = text(&w.conn, &format!("SELECT sync_id FROM transfers WHERE id = {transfer}"));

    exec(&w.conn, &format!("DELETE FROM transfers WHERE id = {transfer}")).unwrap();

    assert_eq!(one(&w.conn, &format!("SELECT transfer_id IS NULL FROM stash_entries WHERE id = {linked}")), 1, "the link is clear");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM stash_entries"), 3, "no entry went");
    let stamp_after = stamp_of(&w.conn, "stash_entries", linked);
    assert!(stamp_after > stamp_before, "a newer stamp, so the other device learns that the link is gone: {stamp_before} {stamp_after}");
    assert_eq!(audit(&w.conn, "stash_entries", "UPDATE"), history + 1, "one row of history for the change");
    let logged = text(&w.conn, "SELECT old_values || ' ' || new_values FROM audit_log WHERE entity_type = 'stash_entries' AND action = 'UPDATE' ORDER BY id DESC LIMIT 1");
    assert!(logged.contains(&format!("\"transfer_id\":{transfer}")) && logged.contains("\"transfer_id\":null"), "{logged}");
    assert_eq!(updated_at_of(&w.conn, "stash_entries", linked), OLD, "the link is not an edit: updated_at stays, as the ledger's does");
    assert_eq!((stamp_of(&w.conn, "stash_entries", untouched), stamp_of(&w.conn, "stash_entries", alone)), others, "other entries are not touched");
    assert_eq!(one(&w.conn, &format!("SELECT transfer_id FROM stash_entries WHERE id = {untouched}")), other_transfer);
    // one tick for the entry and the marker: the same stamp, so two devices end equal (section 7.4)
    assert_eq!(text(&w.conn, &format!("SELECT stamp FROM sync_deleted WHERE sync_id = '{transfer_sync_id}' AND kind = 'transfers'")), stamp_after);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 1);
}

#[test]
fn deleting_a_transfer_that_no_entry_came_with_makes_its_marker_and_touches_no_entry() {
    let w = world();
    let cash = raw_account(&w.conn, "Cash");
    let stash = raw_stash(&w.conn, "Holiday");
    let entry = raw_entry(&w.conn, stash);
    let transfer = raw_transfer(&w.conn, cash, 1);
    let before = (stamp_of(&w.conn, "stash_entries", entry), audit(&w.conn, "stash_entries", "UPDATE"));
    let sync_id = text(&w.conn, &format!("SELECT sync_id FROM transfers WHERE id = {transfer}"));
    exec(&w.conn, &format!("DELETE FROM transfers WHERE id = {transfer}")).unwrap();
    assert_eq!((stamp_of(&w.conn, "stash_entries", entry), audit(&w.conn, "stash_entries", "UPDATE")), before);
    assert_eq!(text(&w.conn, &format!("SELECT stamp FROM sync_deleted WHERE sync_id = '{sync_id}'")).len(), 27);
}

#[test]
fn the_unlink_stands_aside_while_applying() {
    let w = world();
    let cash = raw_account(&w.conn, "Cash");
    let stash = raw_stash(&w.conn, "Holiday");
    let transfer = raw_transfer(&w.conn, cash, 1);
    exec(&w.conn, &format!("INSERT INTO stash_entries (stash_id, amount_minor, entry_date, transfer_id) VALUES ({stash}, 500, '2026-01-01', {transfer})")).unwrap();
    let entry = w.conn.last_insert_rowid();
    age(&w.conn, "stash_entries", entry);
    let before = (stamp_of(&w.conn, "stash_entries", entry), clock(&w.conn));
    applying(&w.conn, true);
    exec(&w.conn, &format!("DELETE FROM transfers WHERE id = {transfer}")).unwrap();
    assert_eq!(one(&w.conn, &format!("SELECT transfer_id IS NULL FROM stash_entries WHERE id = {entry}")), 1, "the foreign key clears the link all the same");
    assert_eq!((stamp_of(&w.conn, "stash_entries", entry), clock(&w.conn)), before, "but no stamp is made and the clock does not tick: the stamp comes from the sync");
    assert_eq!(updated_at_of(&w.conn, "stash_entries", entry), OLD);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 0, "and no marker");
}

#[test]
fn a_link_that_is_cleared_by_hand_is_history_but_not_an_edit_as_for_the_ledger() {
    let w = world();
    let cash = raw_account(&w.conn, "Cash");
    let stash = raw_stash(&w.conn, "Holiday");
    let transfer = raw_transfer(&w.conn, cash, 1);
    exec(&w.conn, &format!("INSERT INTO stash_entries (stash_id, amount_minor, entry_date, transfer_id) VALUES ({stash}, 500, '2026-01-01', {transfer})")).unwrap();
    let entry = w.conn.last_insert_rowid();
    age(&w.conn, "stash_entries", entry);
    let before = (stamp_of(&w.conn, "stash_entries", entry), audit(&w.conn, "stash_entries", "UPDATE"));
    // the stamp triggers do not watch `transfer_id` (0012 did the same for `ledger_entries.transaction_id`): the code that edits an entry
    // writes the amount, the date and the note, which they do
    exec(&w.conn, &format!("UPDATE stash_entries SET transfer_id = NULL WHERE id = {entry}")).unwrap();
    assert_eq!(stamp_of(&w.conn, "stash_entries", entry), before.0);
    assert_eq!(audit(&w.conn, "stash_entries", "UPDATE"), before.1 + 1);
    // but a write that also changes something they watch stamps the whole change, the link too
    exec(&w.conn, &format!("UPDATE stash_entries SET transfer_id = {transfer}, note = 'again' WHERE id = {entry}")).unwrap();
    assert!(stamp_of(&w.conn, "stash_entries", entry) > before.0);
}
