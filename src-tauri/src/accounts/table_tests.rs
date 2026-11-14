//! The database itself must keep the rules of accounts and transfers, whatever the code around it does (migration 0018,
//! `docs/accounts-design.md`, section 4).

use rusqlite::{params, Connection};

use super::fixtures::*;
use super::{MAIN_ID, MAIN_NAME, MAIN_SYNC_ID};
use crate::sync::engine::UNSET_STAMP;

fn exec(conn: &Connection, sql: &str) -> rusqlite::Result<usize> {
    conn.execute(sql, [])
}

fn refused(conn: &Connection, sql: &str) -> String {
    conn.execute(sql, []).expect_err(sql).to_string()
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

/// A plain account written straight into the table. Returns its id.
fn raw_account(conn: &Connection, name: &str) -> i64 {
    conn.execute("INSERT INTO accounts (name, kind, opening_date) VALUES (?1, 'CASH', '2026-01-01')", [name]).unwrap();
    conn.last_insert_rowid()
}

fn raw_transfer(conn: &Connection, from: i64, to: i64) -> i64 {
    conn.execute("INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date) VALUES (?1, ?2, 500, '2026-03-01')", [from, to]).unwrap();
    conn.last_insert_rowid()
}

// ---- the migration ---------------------------------------------------------------------------

#[test]
fn a_new_database_has_the_main_account_with_its_fixed_values_and_no_history_of_it() {
    let w = world();
    let row = text(
        &w.conn,
        "SELECT id || '|' || sync_id || '|' || stamp || '|' || name || '|' || kind || '|' || COALESCE(number, 'none') || '|' || opening_balance_minor || '|' || opening_date
                || '|' || archived || '|' || position || '|' || created_at || '|' || updated_at FROM accounts",
    );
    assert_eq!(row, format!("{MAIN_ID}|{MAIN_SYNC_ID}|{UNSET_STAMP}|{MAIN_NAME}|CHECKING|none|0|1970-01-01|0|0|1970-01-01T00:00:00.000Z|1970-01-01T00:00:00.000Z"));
    assert_eq!((MAIN_ID, MAIN_SYNC_ID, MAIN_NAME), (1, "00000000000000000000000000000001", "Main account"));
    assert_eq!(one(&w.conn, "SELECT count(*) FROM accounts"), 1);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'accounts'"), 0, "the migration's row is not history");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM account_movements"), 0, "an opening balance of 0 is no movement");
    assert!(super::check(&w.conn).unwrap());
}

#[test]
fn the_migration_puts_every_old_row_in_the_main_account_without_writing_one() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::migrate::run_up_to(&mut conn, 17).unwrap();
    conn.execute_batch(
        "INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food'), ('INCOME', 'Salary');
         INSERT INTO people (name) VALUES ('Anna');
         INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date) VALUES (1, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-02-01');
         INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (1, 'Shop', 1250, '2026-01-10'), (2, 'Pay', 200000, '2026-01-31'), (1, 'Gone', 100, '2026-01-11');
         INSERT INTO transactions (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date) VALUES (1, 1, '2026-01-01', 'Rent', 90000, 90000, '2026-01-02');
         INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date) VALUES (1, 'LENT', 2000, '2026-01-12'), (1, 'REPAID_TO_ME', 500, '2026-01-20');
         INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from) VALUES ('EXPENSE', 'GLOBAL', NULL, 100000, '2026-01-01');
         DELETE FROM transactions WHERE name = 'Gone';
         DELETE FROM ledger_entries WHERE id = 2;
         DELETE FROM budgets;",
    )
    .unwrap();
    // everything a write would move: the stamps and `updated_at` of the rows, the markers, the history and the clock
    let state = |conn: &Connection| -> Vec<String> {
        let mut all = Vec::new();
        for sql in [
            "SELECT id || ' ' || sync_id || ' ' || stamp || ' ' || updated_at || ' ' || name || ' ' || amount_minor FROM transactions ORDER BY id",
            "SELECT id || ' ' || sync_id || ' ' || stamp || ' ' || updated_at || ' ' || name FROM recurring_transactions ORDER BY id",
            "SELECT id || ' ' || sync_id || ' ' || stamp || ' ' || updated_at || ' ' || kind FROM ledger_entries ORDER BY id",
            "SELECT id || ' ' || sync_id || ' ' || kind || ' ' || stamp FROM sync_deleted ORDER BY id",
            "SELECT count(*) || ' rows of history' FROM audit_log",
            "SELECT last_ms || ' ' || last_counter FROM sync_clock",
        ] {
            all.extend(conn.prepare(sql).unwrap().query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap));
        }
        all
    };
    let before = state(&conn);
    assert_eq!(one(&conn, "SELECT count(*) FROM sync_deleted"), 3, "a marker of each kind that existed: {before:?}");

    crate::migrate::run(&mut conn).unwrap();

    assert_eq!(state(&conn), before, "no stamp, no updated_at, no marker and no history row moved");
    assert!(conn.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, bool>(0)).unwrap(), "foreign keys are back on");
    assert_eq!(one(&conn, "SELECT count(*) FROM accounts"), 1);
    assert_eq!(text(&conn, "SELECT sync_id || ' ' || stamp || ' ' || name FROM accounts WHERE id = 1"), format!("{MAIN_SYNC_ID} {UNSET_STAMP} {MAIN_NAME}"));
    for (table, rows) in [("transactions", 3), ("recurring_transactions", 1), ("ledger_entries", 1)] {
        assert_eq!(one(&conn, &format!("SELECT count(*) FROM {table} WHERE account_id = 1")), rows, "{table}");
        assert_eq!(one(&conn, &format!("SELECT count(*) FROM {table}")), rows, "{table}");
    }
    assert_eq!(one(&conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'accounts'"), 0);
    // the main account holds what the rows say: -12.50 + 2000.00 - 900.00 - 20.00
    assert_eq!(balance(&conn, MAIN_ID, "2026-12-31"), -1250 + 200_000 - 90_000 - 2000);
    // and the triggers that were made again still work on the old rows
    conn.execute("DELETE FROM transactions WHERE name = 'Shop'", []).unwrap();
    conn.execute("DELETE FROM recurring_transactions", []).unwrap();
    conn.execute("DELETE FROM ledger_entries", []).unwrap();
    assert_eq!(one(&conn, "SELECT count(*) FROM sync_deleted"), 6);
}

#[test]
fn the_migration_makes_every_index_trigger_and_the_view() {
    let w = world();
    for name in [
        "uq_accounts_sync_id", "idx_transactions_account", "idx_recurring_account", "idx_ledger_account", "uq_transfers_sync_id", "idx_transfers_from",
        "idx_transfers_to", "uq_sync_deleted_sync_id", "account_movements", "trg_accounts_keep_main", "trg_accounts_updated_at", "trg_accounts_sync_insert",
        "trg_accounts_sync_update", "trg_accounts_sync_delete", "trg_accounts_audit_insert", "trg_accounts_audit_update", "trg_accounts_audit_delete",
        "trg_transfers_updated_at", "trg_transfers_sync_insert", "trg_transfers_sync_update", "trg_transfers_sync_delete", "trg_transfers_audit_insert",
        "trg_transfers_audit_update", "trg_transfers_audit_delete", "trg_budgets_sync_delete", "trg_recurring_transactions_sync_delete",
        "trg_transactions_sync_delete", "trg_documents_sync_delete", "trg_document_links_sync_delete", "trg_ledger_entries_sync_delete",
        "trg_transactions_updated_at", "trg_transactions_sync_update", "trg_recurring_transactions_updated_at", "trg_recurring_transactions_sync_update",
        "trg_ledger_entries_updated_at", "trg_ledger_entries_sync_update",
    ] {
        assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM sqlite_master WHERE name = '{name}'")), 1, "{name}");
    }
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sqlite_master WHERE name = 'sync_deleted_new'"), 0);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = 'accounts' AND sql LIKE '%name%'"), 0, "no unique index on the name");
}

// ---- sync_deleted ----------------------------------------------------------------------------

#[test]
fn markers_can_be_of_an_account_and_of_a_transfer_and_of_nothing_unknown() {
    let w = world();
    for (n, kind) in ["budgets", "recurring_transactions", "transactions", "documents", "document_links", "ledger_entries", "accounts", "transfers"].iter().enumerate() {
        w.conn.execute("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES (?1, ?2, 'x')", params![format!("{n:032x}"), kind]).unwrap();
    }
    for kind in ["people", "categories", "account", "ACCOUNTS", ""] {
        assert!(exec(&w.conn, &format!("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES ('{}', '{kind}', 'x')", "f".repeat(32))).is_err(), "{kind}");
    }
    assert!(exec(&w.conn, &format!("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES ('{:032x}', 'accounts', 'x')", 0)).is_err(), "one marker for a sync id");
    assert!(exec(&w.conn, "INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES ('short', 'accounts', 'x')").is_err());
}

#[test]
fn the_triggers_that_were_dropped_for_the_rebuild_still_leave_their_markers() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    w.conn
        .execute_batch(&format!(
            "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from) VALUES ('EXPENSE', 'GLOBAL', NULL, 100000, '2026-01-01');
             INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date) VALUES ({food}, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-02-01');
             INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES ({food}, 'Shop', 1250, '2026-01-10');
             INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date) VALUES ({anna}, 'LENT', 2000, '2026-01-12');
             INSERT INTO documents (name, media_type, size_bytes, sha256, document_date) VALUES ('a.pdf', 'application/pdf', 10, '{sha}', '2026-01-01');
             INSERT INTO document_links (document_id, transaction_id) VALUES (1, 1);",
            food = w.food,
            sha = "a".repeat(64),
        ))
        .unwrap();
    for (table, kind) in [
        ("document_links", "document_links"),
        ("documents", "documents"),
        ("ledger_entries", "ledger_entries"),
        ("transactions", "transactions"),
        ("recurring_transactions", "recurring_transactions"),
        ("budgets", "budgets"),
    ] {
        let sync_id = text(&w.conn, &format!("SELECT sync_id FROM {table}"));
        exec(&w.conn, &format!("DELETE FROM {table}")).unwrap();
        let stamp = text(&w.conn, &format!("SELECT stamp FROM sync_deleted WHERE sync_id = '{sync_id}' AND kind = '{kind}'"));
        assert_eq!(stamp.len(), 27, "{table}: {stamp}");
    }
    // and they stand aside while a sync or a restore is writing
    applying(&w.conn, true);
    exec(&w.conn, &format!("INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES ({}, 'Shop', 1250, '2026-01-10')", w.food)).unwrap();
    exec(&w.conn, "DELETE FROM transactions").unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 6);
}

// ---- accounts --------------------------------------------------------------------------------

#[test]
fn an_account_has_a_name_of_one_to_fifty_characters_and_two_may_share_one() {
    let w = world();
    let insert = |name: &str| w.conn.execute("INSERT INTO accounts (name, kind, opening_date) VALUES (?1, 'CASH', '2026-01-01')", [name]);
    assert!(insert("").is_err());
    assert!(insert(&"x".repeat(51)).is_err());
    assert!(insert(&"x".repeat(50)).is_ok());
    assert!(insert("Zoë 日本語").is_ok());
    assert!(insert("Savings").is_ok());
    assert!(insert("Savings").is_ok(), "the name is checked by the code on this device, not by an index");
    assert!(insert("savings").is_ok());
    assert!(exec(&w.conn, "INSERT INTO accounts (kind, opening_date) VALUES ('CASH', '2026-01-01')").is_err(), "a name is needed");
}

#[test]
fn an_account_is_of_one_of_four_kinds() {
    let w = world();
    for kind in ["CHECKING", "SAVINGS", "CASH", "CREDIT_CARD"] {
        assert!(exec(&w.conn, &format!("INSERT INTO accounts (name, kind, opening_date) VALUES ('A', '{kind}', '2026-01-01')")).is_ok(), "{kind}");
    }
    for kind in ["checking", "LOAN", "CREDIT CARD", ""] {
        assert!(exec(&w.conn, &format!("INSERT INTO accounts (name, kind, opening_date) VALUES ('A', '{kind}', '2026-01-01')")).is_err(), "{kind}");
    }
    assert!(exec(&w.conn, "INSERT INTO accounts (name, opening_date) VALUES ('A', '2026-01-01')").is_err(), "a kind is needed");
}

#[test]
fn the_number_of_an_account_is_absent_or_one_to_fifty_characters() {
    let w = world();
    let id = raw_account(&w.conn, "A");
    assert_eq!(one(&w.conn, &format!("SELECT number IS NULL FROM accounts WHERE id = {id}")), 1);
    assert!(exec(&w.conn, &format!("UPDATE accounts SET number = '' WHERE id = {id}")).is_err());
    assert!(exec(&w.conn, &format!("UPDATE accounts SET number = '{}' WHERE id = {id}", "1".repeat(51))).is_err());
    assert!(exec(&w.conn, &format!("UPDATE accounts SET number = '{}' WHERE id = {id}", "1".repeat(50))).is_ok());
    assert!(exec(&w.conn, &format!("UPDATE accounts SET number = 'IE29 AIBK 9311 5212 3456 78' WHERE id = {id}")).is_ok());
    assert!(exec(&w.conn, &format!("UPDATE accounts SET number = NULL WHERE id = {id}")).is_ok());
}

#[test]
fn the_opening_balance_is_a_whole_number_inside_the_range_on_a_real_date() {
    let w = world();
    let id = raw_account(&w.conn, "A");
    assert_eq!(one(&w.conn, &format!("SELECT opening_balance_minor FROM accounts WHERE id = {id}")), 0, "0 when not given");
    for ok in ["-999999999999", "999999999999", "0", "-1"] {
        assert!(exec(&w.conn, &format!("UPDATE accounts SET opening_balance_minor = {ok} WHERE id = {id}")).is_ok(), "{ok}");
    }
    for bad in ["-1000000000000", "1000000000000", "1.5", "'twelve'", "NULL"] {
        assert!(exec(&w.conn, &format!("UPDATE accounts SET opening_balance_minor = {bad} WHERE id = {id}")).is_err(), "{bad}");
    }
    for bad in ["2026-02-30", "2026-2-3", "today", "20260101", "2026-13-01", ""] {
        assert!(exec(&w.conn, &format!("UPDATE accounts SET opening_date = '{bad}' WHERE id = {id}")).is_err(), "{bad}");
    }
    assert!(exec(&w.conn, &format!("UPDATE accounts SET opening_date = NULL WHERE id = {id}")).is_err());
    assert!(exec(&w.conn, &format!("UPDATE accounts SET opening_date = '2024-02-29' WHERE id = {id}")).is_ok());
    assert!(exec(&w.conn, "INSERT INTO accounts (name, kind) VALUES ('A', 'CASH')").is_err(), "an opening date is needed");
}

#[test]
fn archived_is_yes_or_no_and_the_main_account_is_never_archived_whoever_writes() {
    let w = world();
    let id = raw_account(&w.conn, "A");
    assert!(exec(&w.conn, &format!("UPDATE accounts SET archived = 2 WHERE id = {id}")).is_err());
    assert!(exec(&w.conn, &format!("UPDATE accounts SET archived = 1 WHERE id = {id}")).is_ok());
    let error = refused(&w.conn, "UPDATE accounts SET archived = 1 WHERE id = 1");
    assert!(error.contains("ck_accounts_main_active"), "{error}");
    // a sync and a restore turn `applying` on, and the check holds for them too
    applying(&w.conn, true);
    assert!(exec(&w.conn, "UPDATE accounts SET archived = 1 WHERE id = 1").is_err());
    exec(&w.conn, "DELETE FROM accounts WHERE id = 1").unwrap();
    assert!(exec(&w.conn, &format!("INSERT INTO accounts (id, sync_id, name, kind, opening_date, archived) VALUES (1, '{MAIN_SYNC_ID}', 'Main', 'CASH', '2026-01-01', 1)")).is_err());
    assert_eq!(one(&w.conn, "SELECT archived FROM accounts WHERE id = 2"), 1, "the other account was not touched");
}

#[test]
fn the_sync_id_of_an_account_and_of_a_transfer_is_32_hex_characters_and_unique() {
    let w = world();
    let a = raw_account(&w.conn, "A");
    let b = raw_account(&w.conn, "B");
    let x = raw_transfer(&w.conn, a, b);
    let y = raw_transfer(&w.conn, b, a);
    for (table, first, second) in [("accounts", a, b), ("transfers", x, y)] {
        let made = text(&w.conn, &format!("SELECT sync_id FROM {table} WHERE id = {first}"));
        assert!(made.len() == 32 && made.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{made}");
        for bad in ["abc".to_string(), "G".repeat(32), "A".repeat(32), "a".repeat(33)] {
            assert!(exec(&w.conn, &format!("UPDATE {table} SET sync_id = '{bad}' WHERE id = {first}")).is_err(), "{table} {bad}");
        }
        assert!(exec(&w.conn, &format!("UPDATE {table} SET sync_id = '{made}' WHERE id = {second}")).is_err(), "{table}: two rows with one sync id");
    }
}

#[test]
fn the_main_account_is_never_deleted_except_by_a_restore_that_empties_the_tables() {
    let w = world();
    let error = refused(&w.conn, "DELETE FROM accounts WHERE id = 1");
    assert!(error.contains("the main account is never deleted"), "{error}");
    assert!(refused(&w.conn, "DELETE FROM accounts").contains("the main account is never deleted"));
    assert_eq!(one(&w.conn, "SELECT count(*) FROM accounts"), 1);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 0);
    applying(&w.conn, true);
    assert_eq!(exec(&w.conn, "DELETE FROM accounts WHERE id = 1").unwrap(), 1);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 0, "and that leaves no marker");
    assert!(!super::check(&w.conn).unwrap(), "a database without it does not pass the check");
}

#[test]
fn an_account_with_a_row_of_any_of_the_four_kinds_cannot_be_deleted() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let rows = [
        format!("INSERT INTO transactions (category_id, name, amount_minor, payment_date, account_id) VALUES ({}, 'Shop', 1250, '2026-01-10', ?1)", w.food),
        format!("INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date, account_id) VALUES ({}, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-02-01', ?1)", w.food),
        format!("INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date, account_id) VALUES ({anna}, 'LENT', 2000, '2026-01-12', ?1)"),
        "INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date) VALUES (?1, 1, 500, '2026-03-01')".to_string(),
        "INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date) VALUES (1, ?1, 500, '2026-03-01')".to_string(),
    ];
    for (n, sql) in rows.iter().enumerate() {
        let id = raw_account(&w.conn, &format!("Account {n}"));
        w.conn.execute(sql, [id]).unwrap();
        let error = refused(&w.conn, &format!("DELETE FROM accounts WHERE id = {id}"));
        assert!(error.contains("FOREIGN KEY"), "{sql}: {error}");
        assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM accounts WHERE id = {id}")), 1);
    }
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 0, "a refused deletion leaves no marker");
}

// ---- the account of a row --------------------------------------------------------------------

#[test]
fn a_row_that_names_no_account_is_in_the_main_account_and_an_unknown_account_is_refused() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let inserts = [
        ("transactions", format!("INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES ({}, 'Shop', 1250, '2026-01-10')", w.food)),
        ("recurring_transactions", format!("INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date) VALUES ({}, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-02-01')", w.food)),
        ("ledger_entries", format!("INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date) VALUES ({anna}, 'LENT', 2000, '2026-01-12')")),
    ];
    for (table, sql) in &inserts {
        exec(&w.conn, sql).unwrap();
        assert_eq!(one(&w.conn, &format!("SELECT account_id FROM {table}")), MAIN_ID, "{table}");
        assert!(refused(&w.conn, &format!("UPDATE {table} SET account_id = 99")).contains("FOREIGN KEY"), "{table}");
        assert!(exec(&w.conn, &format!("UPDATE {table} SET account_id = NULL")).is_err(), "{table}: the column is never empty");
        assert_eq!(one(&w.conn, &format!("SELECT account_id FROM {table}")), MAIN_ID, "{table}");
    }
}

#[test]
fn moving_a_row_to_another_account_is_a_change_of_the_row() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let cash = raw_account(&w.conn, "Cash");
    w.conn
        .execute_batch(&format!(
            "INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES ({food}, 'Shop', 1250, '2026-01-10');
             INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date) VALUES ({food}, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-02-01');
             INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date) VALUES ({anna}, 'LENT', 2000, '2026-01-12');",
            food = w.food
        ))
        .unwrap();
    for table in ["transactions", "recurring_transactions", "ledger_entries"] {
        age(&w.conn, table, 1);
        let before = stamp_of(&w.conn, table, 1);
        exec(&w.conn, &format!("UPDATE {table} SET account_id = {cash} WHERE id = 1")).unwrap();
        let moved = stamp_of(&w.conn, table, 1);
        assert!(moved > before, "{table}: a move makes a newer stamp");
        assert_ne!(updated_at_of(&w.conn, table, 1), OLD, "{table}: and moves updated_at");

        // while a sync or a restore is writing, neither moves
        applying(&w.conn, true);
        age(&w.conn, table, 1);
        exec(&w.conn, &format!("UPDATE {table} SET account_id = 1 WHERE id = 1")).unwrap();
        assert_eq!((stamp_of(&w.conn, table, 1), updated_at_of(&w.conn, table, 1)), (moved, OLD.to_string()), "{table}");
        applying(&w.conn, false);
    }
    // a ledger entry gets a stamp on a real change only (its triggers compare the values): the same account again is no change
    let before = (stamp_of(&w.conn, "ledger_entries", 1), updated_at_of(&w.conn, "ledger_entries", 1));
    exec(&w.conn, "UPDATE ledger_entries SET account_id = account_id, amount_minor = amount_minor WHERE id = 1").unwrap();
    assert_eq!((stamp_of(&w.conn, "ledger_entries", 1), updated_at_of(&w.conn, "ledger_entries", 1)), before);
}

// ---- transfers -------------------------------------------------------------------------------

#[test]
fn a_transfer_is_between_two_different_accounts_that_exist() {
    let w = world();
    let cash = raw_account(&w.conn, "Cash");
    let insert = |from: i64, to: i64| w.conn.execute("INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date) VALUES (?1, ?2, 500, '2026-03-01')", [from, to]);
    assert!(insert(1, cash).is_ok());
    assert!(insert(cash, 1).is_ok());
    assert!(insert(cash, cash).unwrap_err().to_string().contains("ck_transfers_two_accounts"));
    assert!(insert(1, 1).is_err());
    assert!(insert(1, 99).unwrap_err().to_string().contains("FOREIGN KEY"));
    assert!(insert(99, 1).unwrap_err().to_string().contains("FOREIGN KEY"));
    assert!(exec(&w.conn, "INSERT INTO transfers (to_account_id, amount_minor, transfer_date) VALUES (1, 500, '2026-03-01')").is_err(), "no default account for a side");
    assert!(exec(&w.conn, "UPDATE transfers SET to_account_id = 1 WHERE from_account_id = 1").is_err(), "nor by an edit");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 2);
}

#[test]
fn a_transfer_has_an_amount_of_at_least_one_cent_a_real_date_and_a_short_note() {
    let w = world();
    let cash = raw_account(&w.conn, "Cash");
    let id = raw_transfer(&w.conn, 1, cash);
    for bad in ["0", "-1", "1000000000000", "1.5", "NULL"] {
        assert!(exec(&w.conn, &format!("UPDATE transfers SET amount_minor = {bad} WHERE id = {id}")).is_err(), "{bad}");
    }
    for ok in ["1", "999999999999"] {
        assert!(exec(&w.conn, &format!("UPDATE transfers SET amount_minor = {ok} WHERE id = {id}")).is_ok(), "{ok}");
    }
    for bad in ["2026-02-30", "2026-2-3", "today", "20260101", ""] {
        assert!(exec(&w.conn, &format!("UPDATE transfers SET transfer_date = '{bad}' WHERE id = {id}")).is_err(), "{bad}");
    }
    assert!(exec(&w.conn, &format!("UPDATE transfers SET note = '' WHERE id = {id}")).is_err());
    assert!(exec(&w.conn, &format!("UPDATE transfers SET note = '{}' WHERE id = {id}", "x".repeat(201))).is_err());
    assert!(exec(&w.conn, &format!("UPDATE transfers SET note = '{}' WHERE id = {id}", "x".repeat(200))).is_ok());
    assert!(exec(&w.conn, &format!("UPDATE transfers SET note = NULL WHERE id = {id}")).is_ok());
}

// ---- stamps, updated_at, markers and history of the two tables --------------------------------

/// The same story for both tables: `change` is a real change of a row with id `id`, `same` writes the values it has.
fn stamps_follow_real_changes(conn: &Connection, table: &str, id: i64, change: &str, same: &str) {
    let made = stamp_of(conn, table, id);
    assert_eq!(made.len(), 27, "{table}: a new row gets a stamp: {made}");
    assert_eq!(audit(conn, table, "INSERT"), one(conn, &format!("SELECT count(*) FROM {table} WHERE stamp <> '{UNSET_STAMP}'")), "{table}: each new row is in the history");

    age(conn, table, id);
    let history = audit(conn, table, "UPDATE");
    exec(conn, &format!("UPDATE {table} SET {same} WHERE id = {id}")).unwrap();
    assert_eq!((stamp_of(conn, table, id), updated_at_of(conn, table, id)), (made.clone(), OLD.to_string()), "{table}: the same values again are no change");
    assert_eq!(audit(conn, table, "UPDATE"), history, "{table}");

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
    exec(conn, &format!("UPDATE {table} SET {same} WHERE id = {id}")).unwrap();
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
fn an_account_gets_a_stamp_when_it_is_made_and_on_each_real_change_and_leaves_a_marker() {
    let w = world();
    let id = raw_account(&w.conn, "Cash");
    stamps_follow_real_changes(
        &w.conn,
        "accounts",
        id,
        "name = 'Wallet'",
        "name = name, kind = kind, number = number, opening_balance_minor = opening_balance_minor, opening_date = opening_date, archived = archived, position = position",
    );
    // each column the stamp watches is a change by itself
    let id = raw_account(&w.conn, "Cash");
    for change in ["name = 'W'", "kind = 'SAVINGS'", "number = '12'", "opening_balance_minor = 5", "opening_date = '2026-02-02'", "archived = 1", "position = 7"] {
        let before = stamp_of(&w.conn, "accounts", id);
        exec(&w.conn, &format!("UPDATE accounts SET {change} WHERE id = {id}")).unwrap();
        assert!(stamp_of(&w.conn, "accounts", id) > before, "{change}");
    }
    // the first real edit of the main account gives it a real stamp
    exec(&w.conn, "UPDATE accounts SET name = 'Current account' WHERE id = 1").unwrap();
    assert!(stamp_of(&w.conn, "accounts", 1).as_str() > UNSET_STAMP);
}

#[test]
fn a_transfer_gets_a_stamp_when_it_is_made_and_on_each_real_change_and_leaves_a_marker() {
    let w = world();
    let cash = raw_account(&w.conn, "Cash");
    let savings = raw_account(&w.conn, "Savings");
    let id = raw_transfer(&w.conn, 1, cash);
    stamps_follow_real_changes(
        &w.conn,
        "transfers",
        id,
        "amount_minor = 700",
        "from_account_id = from_account_id, to_account_id = to_account_id, amount_minor = amount_minor, transfer_date = transfer_date, note = note",
    );
    let id = raw_transfer(&w.conn, 1, cash);
    for change in [format!("from_account_id = {savings}"), "to_account_id = 1".to_string(), "amount_minor = 9".to_string(), "transfer_date = '2026-02-02'".to_string(), "note = 'rent'".to_string()] {
        let before = stamp_of(&w.conn, "transfers", id);
        exec(&w.conn, &format!("UPDATE transfers SET {change} WHERE id = {id}")).unwrap();
        assert!(stamp_of(&w.conn, "transfers", id) > before, "{change}");
    }
}

#[test]
fn a_sync_or_a_restore_writing_the_two_tables_keeps_the_stamps_it_brings_and_leaves_no_marker() {
    let w = world();
    applying(&w.conn, true);
    let clock = text(&w.conn, "SELECT last_ms || '-' || last_counter FROM sync_clock");
    let theirs = "1760000000000-0001-aaaaaaaa";
    w.conn.execute("INSERT INTO accounts (name, kind, opening_date, stamp, archived) VALUES ('Cash', 'CASH', '2026-01-01', ?1, 1)", [theirs]).unwrap();
    w.conn.execute("INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date, stamp) VALUES (1, 2, 500, '2026-03-01', ?1)", [theirs]).unwrap();
    assert_eq!((stamp_of(&w.conn, "accounts", 2).as_str(), stamp_of(&w.conn, "transfers", 1).as_str()), (theirs, theirs), "a side of a synced transfer may be archived");
    exec(&w.conn, "DELETE FROM transfers").unwrap();
    exec(&w.conn, "DELETE FROM accounts WHERE id = 2").unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 0);
    assert_eq!(text(&w.conn, "SELECT last_ms || '-' || last_counter FROM sync_clock"), clock, "and the clock that makes stamps did not move");
}

// ---- the view --------------------------------------------------------------------------------

#[test]
fn the_view_has_one_row_for_each_thing_that_moved_an_account_with_its_sign() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    w.conn
        .execute_batch(&format!(
            "INSERT INTO accounts (name, kind, opening_balance_minor, opening_date) VALUES ('Cash', 'CASH', -700, '2026-01-05'), ('Empty', 'CASH', 0, '2026-01-05');
             INSERT INTO transactions (category_id, name, amount_minor, payment_date, account_id) VALUES ({food}, 'Shop', 1250, '2026-01-10', 2), ({salary}, 'Pay', 200000, '2026-01-31', 2);
             INSERT INTO transactions (category_id, name, amount_minor, payment_date, account_id, paid_by) VALUES ({food}, 'Dinner', 4000, '2026-02-01', 2, {anna});
             INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date) VALUES (2, 1, 300, '2026-02-02');
             INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date, account_id) VALUES
               ({anna}, 'LENT', 11, '2026-02-03', 2), ({anna}, 'BORROWED', 13, '2026-02-04', 2), ({anna}, 'REPAID_TO_ME', 17, '2026-02-05', 2), ({anna}, 'REPAID_BY_ME', 19, '2026-02-06', 2);",
            food = w.food,
            salary = w.salary
        ))
        .unwrap();
    let rows: Vec<(i64, String, i64, String, i64)> = w
        .conn
        .prepare("SELECT account_id, day, delta_minor, source, source_id FROM account_movements ORDER BY day, source, account_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let expected = [
        (2, "2026-01-05", -700, "OPENING", 2),
        (2, "2026-01-10", -1250, "TRANSACTION", 1),
        (2, "2026-01-31", 200_000, "TRANSACTION", 2),
        // the bill Anna paid moved nothing
        (1, "2026-02-02", 300, "TRANSFER_IN", 1),
        (2, "2026-02-02", -300, "TRANSFER_OUT", 1),
        (2, "2026-02-03", -11, "LEDGER", 1),
        (2, "2026-02-04", 13, "LEDGER", 2),
        (2, "2026-02-05", 17, "LEDGER", 3),
        (2, "2026-02-06", -19, "LEDGER", 4),
    ]
    .map(|(a, day, delta, source, id): (i64, &str, i64, &str, i64)| (a, day.to_string(), delta, source.to_string(), id));
    assert_eq!(rows, expected);
}
