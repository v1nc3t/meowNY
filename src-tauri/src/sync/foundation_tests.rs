//! The database side of sync: sync ids, stamps, deletion markers, the clock, and how a backup, a restore and a currency
//! change treat them (migration 0008, `docs/sync-design.md` sections 9 to 11).

use rusqlite::{params, Connection};

use super::stamp::{self, Stamp, STAMP_LEN};
use crate::backup;
use crate::currency;
use crate::migrate;

fn db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrate::run(&mut conn).unwrap();
    conn
}

fn db_at(version: u32) -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrate::run_up_to(&mut conn, version).unwrap();
    conn
}

fn settings(conn: &Connection) {
    conn.execute("INSERT INTO settings (id, currency, locale, timezone) VALUES (1, 'EUR', 'en-IE', 'Europe/Dublin')", []).unwrap();
}

fn category(conn: &Connection, name: &str) -> i64 {
    conn.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', ?1)", [name]).unwrap();
    conn.last_insert_rowid()
}

fn transaction(conn: &Connection, category_id: i64, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (?1, ?2, 1250, '2026-01-10')",
        params![category_id, name],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn recurring(conn: &Connection, category_id: i64, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date)
         VALUES (?1, ?2, 90000, '2026-01-01', 'MONTH', '2026-01-01')",
        params![category_id, name],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn budget(conn: &Connection, category_id: i64) -> i64 {
    conn.execute(
        "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
         VALUES ('EXPENSE', 'CATEGORY', ?1, 50000, '2026-01-01')",
        [category_id],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn text(conn: &Connection, sql: &str, id: i64) -> String {
    conn.query_row(sql, [id], |r| r.get(0)).unwrap()
}

fn sync_id(conn: &Connection, table: &str, id: i64) -> String {
    text(conn, &format!("SELECT sync_id FROM {table} WHERE id = ?1"), id)
}

fn stamp_of(conn: &Connection, table: &str, id: i64) -> String {
    text(conn, &format!("SELECT stamp FROM {table} WHERE id = ?1"), id)
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn is_hex32(text: &str) -> bool {
    text.len() == 32 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[test]
fn a_new_row_gets_a_sync_id_and_a_stamp_in_every_shared_table() {
    let conn = db();
    settings(&conn);
    let c = category(&conn, "Food");
    let ids = [
        ("categories", c),
        ("budgets", budget(&conn, c)),
        ("recurring_transactions", recurring(&conn, c, "Rent")),
        ("transactions", transaction(&conn, c, "Coffee")),
    ];
    let device = stamp::device_id(&conn).unwrap();
    assert!(is_hex32(&device));
    for (table, id) in ids {
        let sid = sync_id(&conn, table, id);
        assert!(is_hex32(&sid), "{table}: {sid}");
        let stamp = Stamp::parse(&stamp_of(&conn, table, id)).unwrap_or_else(|| panic!("{table} has no valid stamp"));
        assert_eq!(stamp.device, device[..8]);
    }
    let settings_stamp: String = conn.query_row("SELECT stamp FROM settings", [], |r| r.get(0)).unwrap();
    assert!(Stamp::is_valid(&settings_stamp));
    assert_eq!(settings_stamp.len(), STAMP_LEN);
}

#[test]
fn no_two_rows_share_a_sync_id_and_a_duplicate_is_refused() {
    let conn = db();
    let c = category(&conn, "Food");
    let mut seen = std::collections::HashSet::new();
    for i in 0..200 {
        let t = transaction(&conn, c, &format!("t{i}"));
        assert!(seen.insert(sync_id(&conn, "transactions", t)));
    }
    let first = sync_id(&conn, "transactions", 1);
    assert!(conn.execute("UPDATE transactions SET sync_id = ?1 WHERE id = 2", [&first]).is_err());
}

#[test]
fn stamps_only_go_up_and_sort_as_text() {
    let conn = db();
    let c = category(&conn, "Food");
    let mut stamps = vec![stamp_of(&conn, "categories", c)];
    for i in 0..300 {
        let t = transaction(&conn, c, &format!("t{i}"));
        stamps.push(stamp_of(&conn, "transactions", t));
    }
    for pair in stamps.windows(2) {
        assert!(pair[0] < pair[1], "{} then {}", pair[0], pair[1]);
        assert!(Stamp::parse(&pair[0]).unwrap() < Stamp::parse(&pair[1]).unwrap());
    }
}

#[test]
fn the_clock_never_goes_backwards_even_when_the_wall_clock_does() {
    let conn = db();
    let c = category(&conn, "Food");
    let ahead = stamp::now_ms() as i64 + 3_600_000;
    conn.execute("UPDATE sync_clock SET last_ms = ?1, last_counter = 5", [ahead]).unwrap();
    let t = transaction(&conn, c, "a");
    let s = Stamp::parse(&stamp_of(&conn, "transactions", t)).unwrap();
    assert_eq!((s.ms as i64, s.counter), (ahead, 6));

    conn.execute("UPDATE sync_clock SET last_counter = 65535", []).unwrap();
    let t = transaction(&conn, c, "b");
    let s = Stamp::parse(&stamp_of(&conn, "transactions", t)).unwrap();
    assert_eq!((s.ms as i64, s.counter), (ahead + 1, 0), "a full counter moves the milliseconds on by one");
}

#[test]
fn the_clock_in_rust_and_the_triggers_make_one_ordered_sequence() {
    let conn = db();
    let c = category(&conn, "Food");
    let mut all = vec![];
    for i in 0..50 {
        all.push(Stamp::parse(&stamp_of(&conn, "transactions", transaction(&conn, c, &format!("t{i}")))).unwrap());
        all.push(stamp::tick(&conn).unwrap());
    }
    assert!(all.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn changing_what_a_row_holds_gives_it_a_new_stamp() {
    let conn = db();
    let c = category(&conn, "Food");
    let t = transaction(&conn, c, "Coffee");
    for (what, sql) in [
        ("name", "UPDATE transactions SET name = 'Tea' WHERE id = ?1"),
        ("amount", "UPDATE transactions SET amount_minor = 99 WHERE id = ?1"),
        ("date", "UPDATE transactions SET payment_date = '2026-02-01' WHERE id = ?1"),
        ("description", "UPDATE transactions SET description = 'x' WHERE id = ?1"),
        ("planned", "UPDATE transactions SET planned_amount_minor = 100 WHERE id = ?1"),
    ] {
        let before = stamp_of(&conn, "transactions", t);
        conn.execute(sql, [t]).unwrap();
        assert!(stamp_of(&conn, "transactions", t) > before, "{what}");
    }
    let sid = sync_id(&conn, "transactions", t);
    conn.execute("UPDATE transactions SET name = 'Again' WHERE id = ?1", [t]).unwrap();
    assert_eq!(sync_id(&conn, "transactions", t), sid, "an update never changes the sync id");

    let before = stamp_of(&conn, "categories", c);
    conn.execute("UPDATE categories SET name = 'Groceries' WHERE id = ?1", [c]).unwrap();
    assert!(stamp_of(&conn, "categories", c) > before);
    let before = stamp_of(&conn, "categories", c);
    conn.execute("UPDATE categories SET deleted_at = '2026-02-01T00:00:00.000Z' WHERE id = ?1", [c]).unwrap();
    assert!(stamp_of(&conn, "categories", c) > before, "a soft delete is a change");
}

#[test]
fn what_is_not_content_does_not_make_a_stamp() {
    let conn = db();
    let c = category(&conn, "Food");
    let t = transaction(&conn, c, "Coffee");
    let before = stamp_of(&conn, "transactions", t);
    conn.execute("UPDATE transactions SET updated_at = '2030-01-01T00:00:00.000Z' WHERE id = ?1", [t]).unwrap();
    conn.execute("UPDATE transactions SET created_at = '2020-01-01T00:00:00.000Z' WHERE id = ?1", [t]).unwrap();
    assert_eq!(stamp_of(&conn, "transactions", t), before);
}

#[test]
fn the_sync_columns_write_no_history_and_leave_updated_at_alone() {
    let conn = db();
    let c = category(&conn, "Food");
    assert_eq!(count(&conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'categories' AND action = 'INSERT'"), 1);
    assert_eq!(count(&conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'categories' AND action = 'UPDATE'"), 0);
    let (created, updated): (String, String) =
        conn.query_row("SELECT created_at, updated_at FROM categories WHERE id = ?1", [c], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(created, updated, "setting the sync id and stamp is not an edit");

    std::thread::sleep(std::time::Duration::from_millis(5));
    conn.execute("UPDATE categories SET name = 'Groceries' WHERE id = ?1", [c]).unwrap();
    let after: String = conn.query_row("SELECT updated_at FROM categories WHERE id = ?1", [c], |r| r.get(0)).unwrap();
    assert!(after > updated, "an edit still moves updated_at, as before");
    assert_eq!(count(&conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'categories' AND action = 'UPDATE'"), 1);
}

#[test]
fn deleting_a_transaction_leaves_a_marker_with_a_newer_stamp() {
    let conn = db();
    let c = category(&conn, "Food");
    let t = transaction(&conn, c, "Coffee");
    let (sid, old) = (sync_id(&conn, "transactions", t), stamp_of(&conn, "transactions", t));
    conn.execute("DELETE FROM transactions WHERE id = ?1", [t]).unwrap();
    let (kind, stamp): (String, String) =
        conn.query_row("SELECT kind, stamp FROM sync_deleted WHERE sync_id = ?1", [&sid], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(kind, "transactions");
    assert!(stamp > old);
}

#[test]
fn deleting_a_recurring_item_leaves_a_marker_and_clears_the_links_without_new_stamps() {
    let conn = db();
    let c = category(&conn, "Rent");
    let r = recurring(&conn, c, "Rent");
    conn.execute(
        "INSERT INTO transactions (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, payment_date)
         VALUES (?1, ?2, '2026-01-01', 'Rent', 90000, '2026-01-02')",
        params![c, r],
    )
    .unwrap();
    let t = conn.last_insert_rowid();
    let (sid, tx_stamp) = (sync_id(&conn, "recurring_transactions", r), stamp_of(&conn, "transactions", t));

    conn.execute("DELETE FROM recurring_transactions WHERE id = ?1", [r]).unwrap();

    assert_eq!(count(&conn, "SELECT count(*) FROM transactions WHERE recurring_transaction_id IS NULL"), 1, "the link is cleared");
    assert_eq!(stamp_of(&conn, "transactions", t), tx_stamp, "clearing the link is not an edit: the other device does it by itself");
    let kind: String = conn.query_row("SELECT kind FROM sync_deleted WHERE sync_id = ?1", [&sid], |r| r.get(0)).unwrap();
    assert_eq!(kind, "recurring_transactions");
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted"), 1, "the transaction was not deleted");
}

#[test]
fn while_applying_nothing_is_stamped_and_a_delete_leaves_no_marker() {
    let conn = db();
    let c = category(&conn, "Food");
    stamp::set_applying(&conn, true).unwrap();
    let given = "0000000000001-0000-deadbeef";
    let sid = "0123456789abcdef0123456789abcdef";
    conn.execute(
        "INSERT INTO transactions (sync_id, stamp, category_id, name, amount_minor, payment_date) VALUES (?1, ?2, ?3, 'In', 100, '2026-01-01')",
        params![sid, given, c],
    )
    .unwrap();
    let t = conn.last_insert_rowid();
    assert_eq!((sync_id(&conn, "transactions", t), stamp_of(&conn, "transactions", t)), (sid.to_string(), given.to_string()));
    conn.execute("UPDATE transactions SET name = 'Changed' WHERE id = ?1", [t]).unwrap();
    assert_eq!(stamp_of(&conn, "transactions", t), given);
    conn.execute("DELETE FROM transactions WHERE id = ?1", [t]).unwrap();
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted"), 0);
    stamp::set_applying(&conn, false).unwrap();

    let t = transaction(&conn, c, "Out");
    conn.execute("DELETE FROM transactions WHERE id = ?1", [t]).unwrap();
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted"), 1, "back to normal once it is off");
}

#[test]
fn only_the_shared_settings_make_a_stamp() {
    let conn = db();
    settings(&conn);
    let stamp = |conn: &Connection| -> String { conn.query_row("SELECT stamp FROM settings", [], |r| r.get(0)).unwrap() };
    let start = stamp(&conn);
    conn.execute("UPDATE settings SET language = 'ro'", []).unwrap();
    conn.execute("UPDATE settings SET currency = 'USD'", []).unwrap();
    conn.execute("UPDATE settings SET locale = 'en-IE', timezone = 'Europe/Dublin'", []).unwrap();
    assert_eq!(stamp(&conn), start, "language, currency and a write of the same values are not shared changes");

    conn.execute("UPDATE settings SET locale = 'de-DE'", []).unwrap();
    let after_locale = stamp(&conn);
    assert!(after_locale > start);
    conn.execute("UPDATE settings SET timezone = 'UTC'", []).unwrap();
    assert!(stamp(&conn) > after_locale);
}

#[test]
fn rows_from_before_the_migration_get_ids_and_stamps_from_their_last_update() {
    let mut conn = db_at(7);
    conn.execute("INSERT INTO settings (id, currency, locale, timezone, updated_at) VALUES (1, 'EUR', 'en', 'UTC', '2026-03-01T10:00:00.250Z')", []).unwrap();
    conn.execute("INSERT INTO categories (type, name, updated_at) VALUES ('EXPENSE', 'Food', '2026-03-01T10:00:00.250Z')", []).unwrap();
    conn.execute("INSERT INTO categories (type, name, updated_at) VALUES ('INCOME', 'Pay', '2026-03-02T08:30:00.000Z')", []).unwrap();
    conn.execute(
        "INSERT INTO transactions (category_id, name, amount_minor, payment_date, updated_at) VALUES (1, 'Coffee', 350, '2026-03-01', '2026-03-05T12:00:00.500Z')",
        [],
    )
    .unwrap();
    let audit_before = count(&conn, "SELECT count(*) FROM audit_log");
    let updated_before: Vec<String> = conn
        .prepare("SELECT updated_at FROM categories ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    migrate::run(&mut conn).unwrap();

    let ms = |iso: &str| chrono::DateTime::parse_from_rfc3339(iso).unwrap().timestamp_millis();
    let device = stamp::device_id(&conn).unwrap();
    let first = Stamp::parse(&stamp_of(&conn, "categories", 1)).unwrap();
    assert_eq!((first.ms as i64, first.counter, first.device.as_str()), (ms("2026-03-01T10:00:00.250Z"), 0, &device[..8]));
    assert_eq!(Stamp::parse(&stamp_of(&conn, "transactions", 1)).unwrap().ms as i64, ms("2026-03-05T12:00:00.500Z"));
    assert_eq!(Stamp::parse(&stamp_of(&conn, "settings", 1)).unwrap().ms as i64, ms("2026-03-01T10:00:00.250Z"));
    assert_ne!(sync_id(&conn, "categories", 1), sync_id(&conn, "categories", 2));
    assert!(is_hex32(&sync_id(&conn, "transactions", 1)));

    let updated_after: Vec<String> = conn
        .prepare("SELECT updated_at FROM categories ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(updated_after, updated_before, "the migration does not move any updated_at");
    assert_eq!(count(&conn, "SELECT count(*) FROM audit_log"), audit_before, "and writes no history");

    // the clock starts at or after every stamp the data has, so the next edit is newer than all of them
    let c = category(&conn, "New");
    let newest = Stamp::parse(&stamp_of(&conn, "transactions", 1)).unwrap();
    assert!(Stamp::parse(&stamp_of(&conn, "categories", c)).unwrap() > newest);
}

#[test]
fn a_clock_that_is_behind_the_data_catches_up() {
    let conn = db();
    let c = category(&conn, "Food");
    let future = Stamp::new(stamp::now_ms() + 86_400_000, 7, "abcdef01");
    stamp::set_applying(&conn, true).unwrap();
    conn.execute("UPDATE categories SET stamp = ?1 WHERE id = ?2", params![future.format(), c]).unwrap();
    stamp::set_applying(&conn, false).unwrap();
    conn.execute("UPDATE sync_clock SET last_ms = 1, last_counter = 0", []).unwrap();

    stamp::catch_up(&conn).unwrap();

    let t = transaction(&conn, c, "after");
    assert!(Stamp::parse(&stamp_of(&conn, "transactions", t)).unwrap() > future, "the next change is newer than everything seen");
    assert_eq!(stamp::max_stamp(&conn).unwrap().unwrap().ms, future.ms + 0);
}

#[test]
fn observing_a_stamp_moves_the_clock_up_and_never_down() {
    let conn = db();
    let now = stamp::now_ms();
    stamp::observe(&conn, &Stamp::new(now + 10_000, 3, "11111111")).unwrap();
    stamp::observe(&conn, &Stamp::new(now + 5_000, 99, "11111111")).unwrap();
    let (ms, counter): (i64, i64) = conn.query_row("SELECT last_ms, last_counter FROM sync_clock", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((ms as u64, counter), (now + 10_000, 3));
    stamp::observe(&conn, &Stamp::new(now + 10_000, 4, "11111111")).unwrap();
    let counter: i64 = conn.query_row("SELECT last_counter FROM sync_clock", [], |r| r.get(0)).unwrap();
    assert_eq!(counter, 4);
}

#[test]
fn the_new_tables_refuse_bad_values() {
    let conn = db();
    let id = "0123456789abcdef0123456789abcdef";
    let secret = "ab".repeat(32);
    assert!(conn.execute("INSERT INTO sync_clock (id, device_id, last_ms) VALUES (2, ?1, 0)", [id]).is_err(), "one clock only");
    assert!(conn.execute("UPDATE sync_clock SET device_id = 'short'", []).is_err());
    assert!(conn.execute("UPDATE sync_clock SET device_id = upper(?1)", [id]).is_err(), "device ids are lower case hex");
    assert!(conn.execute("UPDATE sync_clock SET last_counter = 65536", []).is_err());
    assert!(conn.execute("UPDATE sync_clock SET applying = 2", []).is_err());

    assert!(conn.execute("INSERT INTO sync_peers (device_id, name, secret, paired_at) VALUES (?1, 'Phone', ?2, 'now')", params![id, secret]).is_ok());
    assert!(conn.execute("INSERT INTO sync_peers (device_id, name, secret, paired_at) VALUES (?1, 'Again', ?2, 'now')", params![id, secret]).is_err(), "a device is paired once");
    for (device, name, secret) in [("short", "Phone", secret.clone()), ("ffffffffffffffffffffffffffffffff", "", secret.clone()), ("ffffffffffffffffffffffffffffffff", &"x".repeat(51), secret.clone()), ("ffffffffffffffffffffffffffffffff", "Phone", "abcd".to_string()), ("ffffffffffffffffffffffffffffffff", "Phone", "g".repeat(64))] {
        assert!(conn.execute("INSERT INTO sync_peers (device_id, name, secret, paired_at) VALUES (?1, ?2, ?3, 'now')", params![device, name, secret]).is_err(), "{device} {name}");
    }

    assert!(conn.execute("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES (?1, 'categories', 'x')", [id]).is_err(), "categories are never hard deleted");
    assert!(conn.execute("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES ('nope', 'transactions', 'x')", []).is_err());
    assert!(conn.execute("INSERT INTO currency_changes (id, from_currency, to_currency, stamp) VALUES (1, 'EUR', 'usd', 'x')", []).is_err());
    assert!(conn.execute("INSERT INTO currency_changes (id, from_currency, to_currency, rate, stamp) VALUES (1, 'EUR', 'USD', NULL, 'x')", []).is_ok(), "a relabel has no rate");
}

#[test]
fn a_currency_change_converts_without_new_stamps_and_is_recorded() {
    let mut conn = db();
    settings(&conn);
    let c = category(&conn, "Food");
    let t = transaction(&conn, c, "Coffee");
    let r = recurring(&conn, c, "Rent");
    let b = budget(&conn, c);
    let before = (stamp_of(&conn, "transactions", t), stamp_of(&conn, "recurring_transactions", r), stamp_of(&conn, "budgets", b), stamp_of(&conn, "categories", c));
    let settings_stamp: String = conn.query_row("SELECT stamp FROM settings", [], |r| r.get(0)).unwrap();

    currency::change(&mut conn, "USD", Some("2"), "2026-02-01").unwrap();

    let after = (stamp_of(&conn, "transactions", t), stamp_of(&conn, "recurring_transactions", r), stamp_of(&conn, "budgets", b), stamp_of(&conn, "categories", c));
    assert_eq!(after, before, "a conversion is not an edit");
    assert_eq!(count(&conn, "SELECT amount_minor FROM transactions"), 2500, "but the amounts really changed");
    assert_eq!(count(&conn, "SELECT amount_limit_minor FROM budgets"), 100000);
    assert_eq!(count(&conn, "SELECT applying FROM sync_clock"), 0);
    assert_eq!(conn.query_row::<String, _, _>("SELECT stamp FROM settings", [], |r| r.get(0)).unwrap(), settings_stamp);

    fn row(conn: &Connection, id: i64) -> (i64, String, String, Option<String>, String) {
        conn.query_row("SELECT id, from_currency, to_currency, rate, stamp FROM currency_changes WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
    }
    let first = row(&conn, 1);
    assert_eq!((first.1.as_str(), first.2.as_str(), first.3.as_deref()), ("EUR", "USD", Some("2")));
    assert!(Stamp::is_valid(&first.4));
    assert_eq!(count(&conn, "SELECT currency_generation FROM settings"), 1);

    currency::change(&mut conn, "GBP", None, "2026-03-01").unwrap();
    let second = row(&conn, 2);
    assert_eq!((second.1.as_str(), second.2.as_str(), second.3), ("USD", "GBP", None), "relabelling records no rate");
    assert!(second.4 > first.4);
    assert_eq!(count(&conn, "SELECT currency_generation FROM settings"), 2);
    assert_eq!(count(&conn, "SELECT amount_minor FROM transactions"), 2500, "a relabel leaves amounts alone");
}

#[test]
fn a_failed_currency_change_records_nothing_and_leaves_stamping_on() {
    let mut conn = db();
    settings(&conn);
    let c = category(&conn, "Food");
    conn.execute("INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (?1, 'Big', 999999999999, '2026-01-01')", [c]).unwrap();
    assert!(currency::change(&mut conn, "USD", Some("9"), "2026-02-01").is_err());
    assert_eq!(count(&conn, "SELECT count(*) FROM currency_changes"), 0);
    assert_eq!(count(&conn, "SELECT currency_generation FROM settings"), 0);
    assert_eq!(count(&conn, "SELECT applying FROM sync_clock"), 0, "the rollback turned it off");
}

/// A database with every kind of shared row, one deletion and one currency change.
fn lived_in() -> Connection {
    let mut conn = db();
    settings(&conn);
    let food = category(&conn, "Food");
    let rent = category(&conn, "Rent");
    budget(&conn, food);
    let r = recurring(&conn, rent, "Rent");
    let t = transaction(&conn, food, "Coffee");
    transaction(&conn, food, "Tea");
    conn.execute(
        "INSERT INTO transactions (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, payment_date)
         VALUES (?1, ?2, '2026-01-01', 'Rent', 90000, '2026-01-02')",
        params![rent, r],
    )
    .unwrap();
    conn.execute("DELETE FROM transactions WHERE id = ?1", [t]).unwrap();
    currency::change(&mut conn, "USD", Some("1.5"), "2026-02-01").unwrap();
    conn
}

fn rows_of(conn: &Connection, table: &str) -> Vec<String> {
    let columns: Vec<String> =
        conn.prepare("SELECT name FROM pragma_table_info(?1)").unwrap().query_map([table], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
    let select = columns.iter().map(|c| format!("quote({c})")).collect::<Vec<_>>().join(" || ',' || ");
    conn.prepare(&format!("SELECT {select} FROM {table} ORDER BY id")).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

#[test]
fn a_restore_brings_back_every_sync_id_stamp_marker_and_currency_change_exactly() {
    let source = lived_in();
    let snapshot = backup::dump(&source).unwrap();
    let mut target = db();
    settings(&target);
    category(&target, "Something else");

    backup::restore_snapshot(&mut target, &snapshot).unwrap();

    for table in ["settings", "categories", "budgets", "recurring_transactions", "transactions", "sync_deleted", "currency_changes"] {
        assert_eq!(rows_of(&target, table), rows_of(&source, table), "{table}");
    }
    assert_eq!(count(&target, "SELECT count(*) FROM sync_deleted"), 1, "the restore's own deletes left no markers");
    assert_eq!(count(&target, "SELECT applying FROM sync_clock"), 0);
}

#[test]
fn a_restore_makes_the_device_a_new_one_and_forgets_its_pairings() {
    let source = lived_in();
    let snapshot = backup::dump(&source).unwrap();
    let mut target = db();
    settings(&target);
    let before = stamp::device_id(&target).unwrap();
    target
        .execute("INSERT INTO sync_peers (device_id, name, secret, paired_at) VALUES (?1, 'Phone', ?2, 'now')", params!["ab".repeat(16), "cd".repeat(32)])
        .unwrap();

    backup::restore_snapshot(&mut target, &snapshot).unwrap();

    let after = stamp::device_id(&target).unwrap();
    assert!(is_hex32(&after));
    assert_ne!(after, before);
    assert_ne!(after, stamp::device_id(&source).unwrap());
    assert_eq!(count(&target, "SELECT count(*) FROM sync_peers"), 0);
    let c = category(&target, "After the restore");
    let newest = stamp::max_stamp(&source).unwrap().unwrap();
    assert!(Stamp::parse(&stamp_of(&target, "categories", c)).unwrap() > newest, "new changes are newer than everything restored");
}

#[test]
fn the_backup_file_carries_the_ids_but_not_the_pairings_or_the_device() {
    let source = lived_in();
    source
        .execute("INSERT INTO sync_peers (device_id, name, secret, paired_at) VALUES (?1, 'Phone', ?2, 'now')", params!["ab".repeat(16), "cd".repeat(32)])
        .unwrap();
    let snapshot = backup::dump(&source).unwrap();
    assert!(snapshot.tables.get("sync_peers").is_none() && snapshot.tables.get("sync_clock").is_none());
    let json = serde_json::to_string(&snapshot.tables).unwrap();
    assert!(!json.contains(&"cd".repeat(32)), "the pairing secret is not in the file");
    assert!(!json.contains(&stamp::device_id(&source).unwrap()), "nor is the device id");
    assert!(json.contains("sync_id"));
}

#[test]
fn a_backup_from_before_sync_is_given_ids_and_stamps_when_it_is_restored() {
    let old = db_at(7);
    old.execute("INSERT INTO settings (id, currency, locale, timezone) VALUES (1, 'EUR', 'en', 'UTC')", []).unwrap();
    old.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')", []).unwrap();
    old.execute("INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (1, 'Coffee', 350, '2026-03-01')", []).unwrap();
    let snapshot = backup::dump(&old).unwrap();
    assert_eq!(snapshot.schema_version, 7);

    let mut target = db();
    backup::restore_snapshot(&mut target, &snapshot).unwrap();

    assert!(is_hex32(&sync_id(&target, "categories", 1)));
    assert!(Stamp::is_valid(&stamp_of(&target, "transactions", 1)));
    assert_eq!(count(&target, "SELECT count(*) FROM transactions"), 1);
}
