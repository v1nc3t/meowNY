//! Constraint tests: the database itself must refuse bad data, whatever the app code does.

use rusqlite::{params, Connection};

fn db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::migrate::run(&mut conn).unwrap();
    conn
}

fn category(conn: &Connection, kind: &str, name: &str) -> i64 {
    conn.execute("INSERT INTO categories (type, name) VALUES (?1, ?2)", params![kind, name]).unwrap();
    conn.last_insert_rowid()
}

fn transaction(conn: &Connection, category_id: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO transactions (category_id, name, amount_minor, payment_date)
         VALUES (?1, 'Coffee', 350, '2026-01-10')",
        [category_id],
    )
}

fn budget(conn: &Connection, kind: &str, scope: &str, category_id: Option<i64>, month: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from)
         VALUES (?1, ?2, ?3, 100000, ?4)",
        params![kind, scope, category_id, month],
    )
}

fn audit_count(conn: &Connection, entity: &str, action: &str) -> i64 {
    conn.query_row(
        "SELECT count(*) FROM audit_log WHERE entity_type = ?1 AND action = ?2",
        params![entity, action],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn transaction_cannot_use_a_missing_category() {
    let conn = db();
    assert!(transaction(&conn, 999).is_err());
    let food = category(&conn, "EXPENSE", "Food");
    assert!(transaction(&conn, food).is_ok());
}

#[test]
fn category_with_transactions_cannot_be_hard_deleted() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    transaction(&conn, food).unwrap();
    assert!(conn.execute("DELETE FROM categories WHERE id = ?1", [food]).is_err());
}

#[test]
fn budget_type_must_match_category_type() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    assert!(budget(&conn, "INCOME", "CATEGORY", Some(food), "2026-01-01").is_err());
    assert!(budget(&conn, "EXPENSE", "CATEGORY", Some(food), "2026-01-01").is_ok());
}

#[test]
fn budget_scope_and_category_must_agree() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    assert!(budget(&conn, "EXPENSE", "GLOBAL", Some(food), "2026-01-01").is_err());
    assert!(budget(&conn, "EXPENSE", "CATEGORY", None, "2026-01-01").is_err());
}

#[test]
fn budget_must_start_on_the_first_of_a_month() {
    let conn = db();
    assert!(budget(&conn, "EXPENSE", "GLOBAL", None, "2026-01-15").is_err());
    assert!(budget(&conn, "EXPENSE", "GLOBAL", None, "2026-13-01").is_err());
    assert!(budget(&conn, "EXPENSE", "GLOBAL", None, "2026-01-01").is_ok());
}

#[test]
fn only_one_global_budget_per_type_and_month() {
    let conn = db();
    budget(&conn, "EXPENSE", "GLOBAL", None, "2026-01-01").unwrap();
    assert!(budget(&conn, "EXPENSE", "GLOBAL", None, "2026-01-01").is_err());
    // Other type and other month are fine.
    assert!(budget(&conn, "INCOME", "GLOBAL", None, "2026-01-01").is_ok());
    assert!(budget(&conn, "EXPENSE", "GLOBAL", None, "2026-02-01").is_ok());
}

#[test]
fn only_one_budget_per_category_and_month() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    budget(&conn, "EXPENSE", "CATEGORY", Some(food), "2026-01-01").unwrap();
    assert!(budget(&conn, "EXPENSE", "CATEGORY", Some(food), "2026-01-01").is_err());
}

#[test]
fn category_type_cannot_change_after_creation() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    assert!(conn.execute("UPDATE categories SET type = 'INCOME' WHERE id = ?1", [food]).is_err());
    // Other columns still update, and setting the same type is not a change.
    assert!(conn.execute("UPDATE categories SET name = 'Groceries', type = 'EXPENSE' WHERE id = ?1", [food]).is_ok());
}

#[test]
fn active_category_names_are_unique_per_type_but_deleted_ones_free_the_name() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    assert!(conn.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')", []).is_err());
    assert!(conn.execute("INSERT INTO categories (type, name) VALUES ('INCOME', 'Food')", []).is_ok());
    conn.execute("UPDATE categories SET deleted_at = '2026-01-01T00:00:00.000Z' WHERE id = ?1", [food]).unwrap();
    assert!(conn.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')", []).is_ok());
}

#[test]
fn the_same_recurring_occurrence_cannot_be_paid_twice() {
    let conn = db();
    let rent = category(&conn, "EXPENSE", "Rent");
    conn.execute(
        "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date)
         VALUES (?1, 'Rent', 80000, '2026-01-01', 'MONTH', '2026-01-01')",
        [rent],
    )
    .unwrap();
    let recurring = conn.last_insert_rowid();
    let pay = |due: &str| {
        conn.execute(
            "INSERT INTO transactions (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, payment_date)
             VALUES (?1, ?2, ?3, 'Rent', 80000, '2026-01-02')",
            params![rent, recurring, due],
        )
    };
    assert!(pay("2026-01-01").is_ok());
    assert!(pay("2026-01-01").is_err());
    assert!(pay("2026-02-01").is_ok());
}

#[test]
fn recurring_link_requires_a_due_date() {
    let conn = db();
    let rent = category(&conn, "EXPENSE", "Rent");
    conn.execute(
        "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date)
         VALUES (?1, 'Rent', 80000, '2026-01-01', 'MONTH', '2026-01-01')",
        [rent],
    )
    .unwrap();
    let recurring = conn.last_insert_rowid();
    let result = conn.execute(
        "INSERT INTO transactions (category_id, recurring_transaction_id, name, amount_minor, payment_date)
         VALUES (?1, ?2, 'Rent', 80000, '2026-01-02')",
        params![rent, recurring],
    );
    assert!(result.is_err());
}

#[test]
fn the_same_client_uuid_cannot_create_two_transactions() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    let add = |uuid: Option<&str>| {
        conn.execute(
            "INSERT INTO transactions (category_id, client_uuid, name, amount_minor, payment_date)
             VALUES (?1, ?2, 'Coffee', 350, '2026-01-10')",
            params![food, uuid],
        )
    };
    let id = "0b8f6c1e-5c52-4d6a-9d50-3f7a8a1b2c3d";
    assert!(add(Some(id)).is_ok());
    assert!(add(Some(id)).is_err());
    // Transactions without a client_uuid never collide.
    assert!(add(None).is_ok());
    assert!(add(None).is_ok());
}

#[test]
fn audit_rows_appear_for_insert_update_and_delete() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    assert_eq!(audit_count(&conn, "categories", "INSERT"), 1);

    conn.execute("UPDATE categories SET name = 'Groceries' WHERE id = ?1", [food]).unwrap();
    assert_eq!(audit_count(&conn, "categories", "UPDATE"), 1);
    let (old, new): (String, String) = conn
        .query_row(
            "SELECT old_values, new_values FROM audit_log WHERE action = 'UPDATE'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(old.contains("\"Food\"") && new.contains("\"Groceries\""));

    conn.execute("DELETE FROM categories WHERE id = ?1", [food]).unwrap();
    assert_eq!(audit_count(&conn, "categories", "DELETE"), 1);
}

#[test]
fn audit_rows_cover_transactions_and_budgets_too() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    transaction(&conn, food).unwrap();
    budget(&conn, "EXPENSE", "GLOBAL", None, "2026-01-01").unwrap();
    assert_eq!(audit_count(&conn, "transactions", "INSERT"), 1);
    assert_eq!(audit_count(&conn, "budgets", "INSERT"), 1);
}

#[test]
fn audit_is_skipped_when_only_updated_at_changed() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    let before: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
    conn.execute("UPDATE categories SET updated_at = '2030-01-01T00:00:00.000Z' WHERE id = ?1", [food]).unwrap();
    // A no-op update (same values) is also not a change.
    conn.execute("UPDATE categories SET name = name WHERE id = ?1", [food]).unwrap();
    let after: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
    assert_eq!(before, after);
}

#[test]
fn updated_at_moves_forward_on_update() {
    let conn = db();
    let food = category(&conn, "EXPENSE", "Food");
    conn.execute("UPDATE categories SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = ?1", [food]).unwrap();
    conn.execute("UPDATE categories SET name = 'Groceries' WHERE id = ?1", [food]).unwrap();
    let updated: String = conn.query_row("SELECT updated_at FROM categories WHERE id = ?1", [food], |r| r.get(0)).unwrap();
    assert!(updated.as_str() > "2020-01-01", "updated_at was {updated}");
}

#[test]
fn settings_have_a_language_that_defaults_to_english_and_is_audited() {
    let conn = db();
    conn.execute("INSERT INTO settings (id, currency, timezone) VALUES (1, 'EUR', 'UTC')", []).unwrap();
    let language: String = conn.query_row("SELECT language FROM settings", [], |r| r.get(0)).unwrap();
    assert_eq!(language, "en");

    conn.execute("UPDATE settings SET language = 'ro'", []).unwrap();
    let (old, new): (String, String) = conn
        .query_row(
            "SELECT old_values, new_values FROM audit_log WHERE entity_type = 'settings' AND action = 'UPDATE'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(old.contains("\"language\":\"en\"") && new.contains("\"language\":\"ro\""));
}

#[test]
fn a_language_that_is_too_short_or_too_long_is_refused() {
    let conn = db();
    conn.execute("INSERT INTO settings (id, currency, timezone) VALUES (1, 'EUR', 'UTC')", []).unwrap();
    assert!(conn.execute("UPDATE settings SET language = 'e'", []).is_err());
    assert!(conn.execute(&format!("UPDATE settings SET language = '{}'", "x".repeat(36)), []).is_err());
}

fn token(conn: &Connection, name: &str, scope: &str, hash: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO api_tokens (name, scope, token_hash, created_at, expires_at)
         VALUES (?1, ?2, ?3, '2026-10-14T09:00:00Z', '2026-11-13T09:00:00Z')",
        params![name, scope, hash],
    )
}

#[test]
fn a_token_has_a_name_a_known_scope_and_a_sha256_shaped_hash() {
    let conn = db();
    let hash = "a".repeat(64);
    assert!(token(&conn, "agent", "READ", &hash).is_ok());
    assert!(token(&conn, "agent", "WRITE", &"b".repeat(64)).is_ok());
    assert!(token(&conn, "", "READ", &"c".repeat(64)).is_err(), "empty name");
    assert!(token(&conn, &"x".repeat(51), "READ", &"c".repeat(64)).is_err(), "long name");
    assert!(token(&conn, "agent", "ADMIN", &"c".repeat(64)).is_err(), "unknown scope");
    assert!(token(&conn, "agent", "read", &"c".repeat(64)).is_err(), "scope is upper case in the table");
    assert!(token(&conn, "agent", "READ", &"c".repeat(63)).is_err(), "short hash");
    assert!(token(&conn, "agent", "READ", &"C".repeat(64)).is_err(), "upper case hex");
    assert!(token(&conn, "agent", "READ", &format!("{}g", "c".repeat(63))).is_err(), "not hex");
}

#[test]
fn two_tokens_cannot_share_a_hash() {
    let conn = db();
    token(&conn, "one", "READ", &"a".repeat(64)).unwrap();
    assert!(token(&conn, "two", "WRITE", &"a".repeat(64)).is_err());
}

#[test]
fn tokens_leave_no_trace_in_the_history() {
    let conn = db();
    token(&conn, "agent", "READ", &"a".repeat(64)).unwrap();
    conn.execute("UPDATE api_tokens SET last_used_at = '2026-10-14T10:00:00Z'", []).unwrap();
    conn.execute("DELETE FROM api_tokens", []).unwrap();
    let rows: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 0);
}
