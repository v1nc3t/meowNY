//! The two tables and their triggers: the database itself must refuse bad rows and keep stamps, markers and history, whatever
//! the code above it does.

use rusqlite::{params, Connection};

use super::names::MediaType;
use crate::sync::stamp;

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::migrate::run(&mut conn).unwrap();
    conn
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn text(conn: &Connection, sql: &str) -> String {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn category(conn: &Connection) -> i64 {
    conn.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')", []).unwrap();
    conn.last_insert_rowid()
}

fn transaction(conn: &Connection, category: i64) -> i64 {
    conn.execute(
        "INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (?1, 'Coffee', 350, '2026-01-10')",
        [category],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn recurring(conn: &Connection, category: i64) -> i64 {
    conn.execute(
        "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date)
         VALUES (?1, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-02-01')",
        [category],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn document(conn: &Connection, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date) VALUES (?1, 'application/pdf', 100, ?2, '2026-03-01')",
        params![name, SHA],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn insert_document(conn: &Connection, name: &str, media: &str, size: i64, sha: &str, date: &str, keep: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date, keep_on_phone) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![name, media, size, sha, date, keep],
    )
}

fn link_transaction(conn: &Connection, document: i64, transaction: i64) -> rusqlite::Result<usize> {
    conn.execute("INSERT INTO document_links (document_id, transaction_id) VALUES (?1, ?2)", params![document, transaction])
}

fn link_recurring(conn: &Connection, document: i64, recurring: i64) -> rusqlite::Result<usize> {
    conn.execute("INSERT INTO document_links (document_id, recurring_transaction_id) VALUES (?1, ?2)", params![document, recurring])
}

fn stamp_of(conn: &Connection, table: &str, id: i64) -> String {
    text(conn, &format!("SELECT stamp FROM {table} WHERE id = {id}"))
}

fn applying(conn: &Connection, on: bool) {
    stamp::set_applying(conn, on).unwrap();
}

// -- documents ----------------------------------------------------------------------------------------------------------

#[test]
fn a_document_gets_a_random_sync_id_and_a_stamp() {
    let conn = db();
    let a = document(&conn, "One");
    let b = document(&conn, "Two");
    let ids: Vec<String> = [a, b].iter().map(|id| text(&conn, &format!("SELECT sync_id FROM documents WHERE id = {id}"))).collect();
    for id in &ids {
        assert_eq!(id.len(), 32);
        assert!(id.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)), "{id}");
    }
    assert_ne!(ids[0], ids[1]);
    let (first, second) = (stamp_of(&conn, "documents", a), stamp_of(&conn, "documents", b));
    assert_eq!(first.len(), stamp::STAMP_LEN);
    assert!(second > first, "the clock only goes forward: {first} then {second}");
}

#[test]
fn a_row_that_comes_with_its_id_and_stamp_keeps_them() {
    let conn = db();
    let id = "0123456789abcdef0123456789abcdef";
    let stamp = "1700000000000-0003-abcdef01";
    conn.execute(
        "INSERT INTO documents (sync_id, stamp, name, media_type, size_bytes, sha256, document_date) VALUES (?1, ?2, 'Sent', 'text/plain', 5, ?3, '2026-03-01')",
        params![id, stamp, SHA],
    )
    .unwrap();
    assert_eq!(text(&conn, "SELECT sync_id || ' ' || stamp FROM documents"), format!("{id} {stamp}"));
}

#[test]
fn the_database_refuses_bad_documents() {
    let conn = db();
    let ok = |name: &str, media: &str, size: i64, sha: &str, date: &str, keep: i64| insert_document(&conn, name, media, size, sha, date, keep);
    assert!(ok("Fine", "application/pdf", 100, SHA, "2026-03-01", 0).is_ok());

    assert!(ok("", "application/pdf", 100, SHA, "2026-03-01", 0).is_err(), "no name");
    assert!(ok(&"a".repeat(120), "application/pdf", 100, SHA, "2026-03-01", 0).is_ok());
    assert!(ok(&"a".repeat(121), "application/pdf", 100, SHA, "2026-03-01", 0).is_err(), "a name of 121 characters");
    assert!(ok(&"é".repeat(120), "application/pdf", 100, SHA, "2026-03-01", 0).is_ok(), "120 characters, not 120 bytes");

    for bad in ["text/xml", "image/svg+xml", "IMAGE/PNG", "", "application/x-msdownload"] {
        assert!(ok("Odd", bad, 100, SHA, "2026-03-01", 0).is_err(), "media type {bad:?}");
    }
    for media in MediaType::ALL {
        assert!(ok("Every kind", media.as_str(), 100, SHA, "2026-03-01", 0).is_ok(), "{media:?} must be allowed by the table");
    }

    assert!(ok("Size", "application/pdf", 0, SHA, "2026-03-01", 0).is_err(), "an empty file");
    assert!(ok("Size", "application/pdf", -5, SHA, "2026-03-01", 0).is_err());
    assert!(ok("Size", "application/pdf", 1, SHA, "2026-03-01", 0).is_ok());
    assert!(ok("Size", "application/pdf", 26_214_400, SHA, "2026-03-01", 0).is_ok(), "25 MiB");
    assert!(ok("Size", "application/pdf", 26_214_401, SHA, "2026-03-01", 0).is_err());

    for bad in ["", "abc", &SHA[..63], &format!("{SHA}a"), &SHA.to_uppercase(), &format!("g{}", &SHA[1..])] {
        assert!(ok("Hash", "application/pdf", 100, bad, "2026-03-01", 0).is_err(), "hash {bad:?}");
    }
    for bad in ["", "2026-02-30", "2026-3-1", "2026-13-01", "01/03/2026", "2026-03-01T10:00", "tomorrow"] {
        assert!(ok("Date", "application/pdf", 100, SHA, bad, 0).is_err(), "date {bad:?}");
    }
    assert!(ok("Leap day", "application/pdf", 100, SHA, "2028-02-29", 0).is_ok());
    assert!(ok("Switch", "application/pdf", 100, SHA, "2026-03-01", 1).is_ok());
    assert!(ok("Switch", "application/pdf", 100, SHA, "2026-03-01", 2).is_err());
    assert!(ok("Switch", "application/pdf", 100, SHA, "2026-03-01", -1).is_err());

    assert!(conn.execute("INSERT INTO documents (sync_id, name, media_type, size_bytes, sha256, document_date) VALUES ('short', 'x', 'text/plain', 1, ?1, '2026-03-01')", [SHA]).is_err());
    assert!(conn
        .execute(
            "INSERT INTO documents (sync_id, name, media_type, size_bytes, sha256, document_date) VALUES ('0123456789ABCDEF0123456789ABCDEF', 'x', 'text/plain', 1, ?1, '2026-03-01')",
            [SHA]
        )
        .is_err());
}

#[test]
fn two_documents_cannot_share_a_sync_id() {
    let conn = db();
    let id = "0123456789abcdef0123456789abcdef";
    let add = || {
        conn.execute(
            "INSERT INTO documents (sync_id, name, media_type, size_bytes, sha256, document_date) VALUES (?1, 'x', 'text/plain', 1, ?2, '2026-03-01')",
            params![id, SHA],
        )
    };
    add().unwrap();
    assert!(add().is_err());
}

#[test]
fn the_contents_of_a_document_never_change() {
    let conn = db();
    let id = document(&conn, "Contract");
    for sql in [
        "UPDATE documents SET media_type = 'text/plain'",
        "UPDATE documents SET size_bytes = 101",
        "UPDATE documents SET sha256 = 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'",
        "UPDATE documents SET sync_id = '0123456789abcdef0123456789abcdef'",
    ] {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    // setting a column to the value it has is not a change
    conn.execute("UPDATE documents SET media_type = media_type, size_bytes = size_bytes, sha256 = sha256", []).unwrap();
    conn.execute("UPDATE documents SET name = 'Renamed', document_date = '2026-04-01', keep_on_phone = 1 WHERE id = ?1", [id]).unwrap();
}

#[test]
fn what_a_person_edits_gives_the_row_a_new_stamp() {
    let conn = db();
    let id = document(&conn, "Contract");
    let mut last = stamp_of(&conn, "documents", id);
    for sql in [
        "UPDATE documents SET name = 'Renamed'",
        "UPDATE documents SET document_date = '2026-04-01'",
        "UPDATE documents SET keep_on_phone = 1",
    ] {
        conn.execute(sql, []).unwrap();
        let now = stamp_of(&conn, "documents", id);
        assert!(now > last, "{sql}: {last} then {now}");
        last = now;
    }
    // updated_at alone, or the same value again, is not a change
    conn.execute("UPDATE documents SET updated_at = '2030-01-01T00:00:00.000Z'", []).unwrap();
    conn.execute("UPDATE documents SET name = 'Renamed', keep_on_phone = 1", []).unwrap();
    assert_eq!(stamp_of(&conn, "documents", id), last);
}

#[test]
fn a_sync_or_a_restore_writing_a_row_keeps_the_stamp_it_brought() {
    let conn = db();
    let id = document(&conn, "Contract");
    let before = stamp_of(&conn, "documents", id);
    applying(&conn, true);
    conn.execute("UPDATE documents SET name = 'From the phone', stamp = '1700000000000-0000-abcdef01'", []).unwrap();
    conn.execute("UPDATE documents SET name = 'Again'", []).unwrap();
    applying(&conn, false);
    assert_eq!(stamp_of(&conn, "documents", id), "1700000000000-0000-abcdef01");
    assert_ne!(before, "1700000000000-0000-abcdef01");
}

#[test]
fn updated_at_follows_an_edit_and_not_a_sync() {
    let conn = db();
    let id = document(&conn, "Contract");
    // a value written to updated_at itself is kept
    conn.execute("UPDATE documents SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = ?1", [id]).unwrap();
    assert_eq!(text(&conn, "SELECT updated_at FROM documents"), "2000-01-01T00:00:00.000Z");
    conn.execute("UPDATE documents SET name = 'Renamed'", []).unwrap();
    assert_ne!(text(&conn, "SELECT updated_at FROM documents"), "2000-01-01T00:00:00.000Z");

    conn.execute("UPDATE documents SET updated_at = '2000-01-01T00:00:00.000Z'", []).unwrap();
    applying(&conn, true);
    conn.execute("UPDATE documents SET name = 'Synced'", []).unwrap();
    applying(&conn, false);
    assert_eq!(text(&conn, "SELECT updated_at FROM documents"), "2000-01-01T00:00:00.000Z", "a sync writes the updated_at it brings");
}

#[test]
fn deleting_a_document_leaves_a_marker_that_a_sync_does_not() {
    let conn = db();
    let id = document(&conn, "Contract");
    let sync_id = text(&conn, "SELECT sync_id FROM documents");
    let stamp = stamp_of(&conn, "documents", id);
    conn.execute("DELETE FROM documents", []).unwrap();
    let (kind, marker): (String, String) =
        conn.query_row("SELECT kind, stamp FROM sync_deleted WHERE sync_id = ?1", [&sync_id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(kind, "documents");
    assert!(marker > stamp, "the delete is a newer change than the row: {stamp} then {marker}");

    document(&conn, "Other");
    applying(&conn, true);
    conn.execute("DELETE FROM documents", []).unwrap();
    applying(&conn, false);
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted"), 1, "applying a delete from the other device leaves no marker of its own");
}

#[test]
fn the_markers_accept_the_new_kinds_and_nothing_else() {
    let conn = db();
    let id = |n: u8| format!("{n:02x}").repeat(16);
    for (n, kind) in ["budgets", "recurring_transactions", "transactions", "documents", "document_links"].iter().enumerate() {
        conn.execute("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES (?1, ?2, 'x')", params![id(n as u8), kind]).unwrap();
    }
    for (n, kind) in ["categories", "settings", "document", ""].iter().enumerate() {
        assert!(conn.execute("INSERT INTO sync_deleted (sync_id, kind, stamp) VALUES (?1, ?2, 'x')", params![id(100 + n as u8), kind]).is_err(), "{kind:?}");
    }
}

// -- links --------------------------------------------------------------------------------------------------------------

#[test]
fn a_link_has_exactly_one_target() {
    let conn = db();
    let cat = category(&conn);
    let (doc, tx, rec) = (document(&conn, "Receipt"), transaction(&conn, cat), recurring(&conn, cat));
    assert!(conn.execute("INSERT INTO document_links (document_id) VALUES (?1)", [doc]).is_err(), "no target");
    assert!(conn
        .execute("INSERT INTO document_links (document_id, transaction_id, recurring_transaction_id) VALUES (?1, ?2, ?3)", params![doc, tx, rec])
        .is_err());
    assert!(link_transaction(&conn, doc, tx).is_ok());
    assert!(link_recurring(&conn, doc, rec).is_ok());
}

#[test]
fn a_link_is_made_once_and_to_things_that_exist() {
    let conn = db();
    let cat = category(&conn);
    let (doc, other) = (document(&conn, "Receipt"), document(&conn, "Invoice"));
    let (tx, tx2, rec) = (transaction(&conn, cat), transaction(&conn, cat), recurring(&conn, cat));
    link_transaction(&conn, doc, tx).unwrap();
    assert!(link_transaction(&conn, doc, tx).is_err(), "the same link twice");
    assert!(link_transaction(&conn, doc, tx2).is_ok(), "one document, two transactions");
    assert!(link_transaction(&conn, other, tx).is_ok(), "one transaction, two documents");
    link_recurring(&conn, doc, rec).unwrap();
    assert!(link_recurring(&conn, doc, rec).is_err());
    assert!(link_transaction(&conn, 999, tx).is_err(), "no such document");
    assert!(link_transaction(&conn, doc, 999).is_err(), "no such transaction");
    assert!(link_recurring(&conn, doc, 999).is_err(), "no such recurring item");
}

#[test]
fn a_link_is_made_or_removed_never_changed() {
    let conn = db();
    let cat = category(&conn);
    let (doc, other) = (document(&conn, "Receipt"), document(&conn, "Invoice"));
    let (tx, tx2) = (transaction(&conn, cat), transaction(&conn, cat));
    link_transaction(&conn, doc, tx).unwrap();
    assert!(conn.execute("UPDATE document_links SET transaction_id = ?1", [tx2]).is_err());
    assert!(conn.execute("UPDATE document_links SET document_id = ?1", [other]).is_err());
    assert!(conn.execute("UPDATE document_links SET sync_id = '0123456789abcdef0123456789abcdef'", []).is_err());
    assert!(conn.execute("UPDATE document_links SET recurring_transaction_id = NULL, transaction_id = NULL", []).is_err());
}

#[test]
fn deleting_what_a_document_is_linked_to_removes_the_link_and_keeps_the_document() {
    let conn = db();
    let cat = category(&conn);
    let (doc, tx, rec) = (document(&conn, "Receipt"), transaction(&conn, cat), recurring(&conn, cat));
    link_transaction(&conn, doc, tx).unwrap();
    link_recurring(&conn, doc, rec).unwrap();
    conn.execute("DELETE FROM transactions WHERE id = ?1", [tx]).unwrap();
    assert_eq!((count(&conn, "SELECT count(*) FROM document_links"), count(&conn, "SELECT count(*) FROM documents")), (1, 1));
    conn.execute("DELETE FROM recurring_transactions WHERE id = ?1", [rec]).unwrap();
    assert_eq!((count(&conn, "SELECT count(*) FROM document_links"), count(&conn, "SELECT count(*) FROM documents")), (0, 1));
    // both links left a marker
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'document_links'"), 2);
}

#[test]
fn deleting_a_document_removes_its_links_and_nothing_it_was_linked_to() {
    let conn = db();
    let cat = category(&conn);
    let (doc, tx, rec) = (document(&conn, "Receipt"), transaction(&conn, cat), recurring(&conn, cat));
    link_transaction(&conn, doc, tx).unwrap();
    link_recurring(&conn, doc, rec).unwrap();
    conn.execute("DELETE FROM documents WHERE id = ?1", [doc]).unwrap();
    assert_eq!(count(&conn, "SELECT count(*) FROM document_links"), 0);
    assert_eq!((count(&conn, "SELECT count(*) FROM transactions"), count(&conn, "SELECT count(*) FROM recurring_transactions")), (1, 1));
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'documents'"), 1);
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'document_links'"), 2);
}

#[test]
fn a_link_has_a_stamp_and_a_removed_link_a_marker() {
    let conn = db();
    let cat = category(&conn);
    let (doc, tx) = (document(&conn, "Receipt"), transaction(&conn, cat));
    link_transaction(&conn, doc, tx).unwrap();
    let id: i64 = conn.query_row("SELECT id FROM document_links", [], |r| r.get(0)).unwrap();
    let stamp = stamp_of(&conn, "document_links", id);
    assert_eq!(stamp.len(), stamp::STAMP_LEN);
    let sync_id = text(&conn, "SELECT sync_id FROM document_links");
    conn.execute("DELETE FROM document_links", []).unwrap();
    let marker = text(&conn, &format!("SELECT stamp FROM sync_deleted WHERE sync_id = '{sync_id}' AND kind = 'document_links'"));
    assert!(marker > stamp);
    // linking again is a new link, with a new id
    link_transaction(&conn, doc, tx).unwrap();
    assert_ne!(text(&conn, "SELECT sync_id FROM document_links"), sync_id);
}

#[test]
fn the_clock_catches_up_with_the_stamps_of_documents_and_links() {
    let conn = db();
    let cat = category(&conn);
    let tx = transaction(&conn, cat);
    // a document and a link that came from a device whose clock is far ahead
    conn.execute(
        "INSERT INTO documents (stamp, name, media_type, size_bytes, sha256, document_date) VALUES ('4102444800000-0002-abcdef01', 'x', 'text/plain', 1, ?1, '2026-03-01')",
        [SHA],
    )
    .unwrap();
    let doc = conn.last_insert_rowid();
    conn.execute("INSERT INTO document_links (stamp, document_id, transaction_id) VALUES ('4102444900000-0000-abcdef01', ?1, ?2)", params![doc, tx]).unwrap();
    stamp::catch_up(&conn).unwrap();
    let ms: i64 = conn.query_row("SELECT last_ms FROM sync_clock", [], |r| r.get(0)).unwrap();
    assert!(ms >= 4_102_444_900_000, "{ms}");
    // so the next change is newer than both
    conn.execute("UPDATE documents SET name = 'y'", []).unwrap();
    assert!(stamp_of(&conn, "documents", doc).as_str() > "4102444900000-0000-abcdef01");
}

// -- the history --------------------------------------------------------------------------------------------------------

fn history(conn: &Connection, entity: &str, action: &str) -> i64 {
    conn.query_row("SELECT count(*) FROM audit_log WHERE entity_type = ?1 AND action = ?2", params![entity, action], |r| r.get(0)).unwrap()
}

#[test]
fn adding_changing_and_deleting_are_in_the_history_with_the_name_and_not_the_hash() {
    let conn = db();
    let cat = category(&conn);
    let (doc, tx) = (document(&conn, "Boiler warranty"), transaction(&conn, cat));
    link_transaction(&conn, doc, tx).unwrap();
    conn.execute("UPDATE documents SET name = 'Boiler warranty 2026'", []).unwrap();
    conn.execute("UPDATE documents SET keep_on_phone = 1", []).unwrap();
    conn.execute("UPDATE documents SET updated_at = '2030-01-01T00:00:00.000Z'", []).unwrap();
    conn.execute("DELETE FROM documents", []).unwrap();

    assert_eq!(history(&conn, "documents", "INSERT"), 1);
    assert_eq!(history(&conn, "documents", "UPDATE"), 2, "a rename and a switch; the updated_at alone is not a change");
    assert_eq!(history(&conn, "documents", "DELETE"), 1);
    assert_eq!((history(&conn, "document_links", "INSERT"), history(&conn, "document_links", "DELETE")), (1, 1));

    let all: String = conn
        .query_row("SELECT group_concat(coalesce(old_values, '') || coalesce(new_values, ''), ' ') FROM audit_log WHERE entity_type LIKE 'document%'", [], |r| r.get(0))
        .unwrap();
    assert!(all.contains("Boiler warranty 2026"));
    assert!(!all.contains(SHA), "the hash of the contents is not history");
}

// -- the backup schedule ------------------------------------------------------------------------------------------------

#[test]
fn a_scheduled_backup_remembers_whether_to_take_the_documents() {
    let conn = db();
    conn.execute("INSERT INTO backup_schedule (id, folder, every_days, passphrase) VALUES (1, '/tmp/b', 7, 'secret words here')", []).unwrap();
    assert_eq!(count(&conn, "SELECT include_documents FROM backup_schedule"), 0, "off until the person says so");
    conn.execute("UPDATE backup_schedule SET include_documents = 1", []).unwrap();
    assert!(conn.execute("UPDATE backup_schedule SET include_documents = 2", []).is_err());
}

// -- the migration ------------------------------------------------------------------------------------------------------

#[test]
fn the_markers_of_the_old_schema_survive_and_the_old_triggers_still_write_them() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::migrate::run_up_to(&mut conn, 9).unwrap();
    let cat = category(&conn);
    let old_tx = transaction(&conn, cat);
    let old_rec = recurring(&conn, cat);
    conn.execute("INSERT INTO budgets (type, scope, effective_from, amount_limit_minor) VALUES ('EXPENSE', 'GLOBAL', '2026-01-01', 100)", []).unwrap();
    conn.execute("DELETE FROM transactions WHERE id = ?1", [old_tx]).unwrap();
    conn.execute("DELETE FROM recurring_transactions WHERE id = ?1", [old_rec]).unwrap();
    conn.execute("DELETE FROM budgets", []).unwrap();
    let before: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare("SELECT sync_id, kind, stamp FROM sync_deleted ORDER BY id").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(before.len(), 3);

    crate::migrate::run(&mut conn).unwrap();
    let after: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare("SELECT sync_id, kind, stamp FROM sync_deleted ORDER BY id").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(after, before, "the markers kept their ids, kinds and stamps");

    // the three triggers that write markers were made again, and still do
    let tx = transaction(&conn, cat);
    let rec = recurring(&conn, cat);
    conn.execute("INSERT INTO budgets (type, scope, effective_from, amount_limit_minor) VALUES ('EXPENSE', 'GLOBAL', '2026-02-01', 100)", []).unwrap();
    conn.execute("DELETE FROM transactions WHERE id = ?1", [tx]).unwrap();
    conn.execute("DELETE FROM recurring_transactions WHERE id = ?1", [rec]).unwrap();
    conn.execute("DELETE FROM budgets", []).unwrap();
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted"), 6);
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'transactions'"), 2);
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'recurring_transactions'"), 2);
    assert_eq!(count(&conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'budgets'"), 2);
    assert_eq!(conn.query_row::<i64, _, _>("PRAGMA user_version", [], |r| r.get(0)).unwrap(), 10);
    assert_eq!(count(&conn, "SELECT count(*) FROM pragma_foreign_key_check"), 0);
}
