//! Schema migrations. Plain SQL files in `migrations/`, embedded in the binary and applied in
//! order on startup. The applied version is stored in SQLite's `user_version` pragma.

use rusqlite::Connection;

/// Ordered by version: migration N is `MIGRATIONS[N - 1]`. Append only, never edit a released one.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_tables.sql"),
    include_str!("../migrations/0002_triggers.sql"),
    include_str!("../migrations/0003_language.sql"),
    include_str!("../migrations/0004_indexes.sql"),
    include_str!("../migrations/0005_backup_schedule.sql"),
    include_str!("../migrations/0006_recurring_amount_optional.sql"),
    include_str!("../migrations/0007_api_tokens.sql"),
    include_str!("../migrations/0008_sync.sql"),
    include_str!("../migrations/0009_updated_at_and_sync.sql"),
    include_str!("../migrations/0010_documents.sql"),
];

#[derive(Debug)]
pub enum MigrateError {
    /// The database was written by a newer app. Opening it could corrupt data we do not understand.
    DatabaseTooNew { database: i64, app: usize },
    /// A migration left rows pointing at rows that are not there. Nothing from it was kept.
    ForeignKeys,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for MigrateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrateError::DatabaseTooNew { database, app } => write!(
                f,
                "this database is schema version {database} but this app only supports up to {app}; update the app"
            ),
            MigrateError::ForeignKeys => write!(f, "migration failed: it broke a foreign key"),
            MigrateError::Sqlite(e) => write!(f, "migration failed: {e}"),
        }
    }
}

impl std::error::Error for MigrateError {}

impl From<rusqlite::Error> for MigrateError {
    fn from(e: rusqlite::Error) -> Self {
        MigrateError::Sqlite(e)
    }
}

/// The schema version this app writes and understands.
pub fn latest_version() -> u32 {
    MIGRATIONS.len() as u32
}

/// Brings the database up to the latest schema. Each migration runs in its own transaction,
/// so a failure leaves the database at the last good version.
pub fn run(conn: &mut Connection) -> Result<(), MigrateError> {
    apply(conn, MIGRATIONS)
}

/// Builds the schema as it was at `version`. Restoring an older backup loads its rows into a
/// database of that shape first, then runs the remaining migrations over them.
pub fn run_up_to(conn: &mut Connection, version: u32) -> Result<(), MigrateError> {
    apply(conn, &MIGRATIONS[..(version as usize).min(MIGRATIONS.len())])
}

fn apply(conn: &mut Connection, migrations: &[&str]) -> Result<(), MigrateError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > migrations.len() as i64 {
        return Err(MigrateError::DatabaseTooNew { database: current, app: migrations.len() });
    }
    // Rebuilding a table (SQLite cannot change some things in place) must not trigger the foreign key actions
    // of the table it drops, so foreign keys are off while a migration runs, as SQLite's own guide says. The
    // switch only works outside a transaction. Before the commit they are checked, so a migration that
    // leaves a dangling row is rolled back.
    let foreign_keys: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    for (index, sql) in migrations.iter().enumerate().skip(current as usize) {
        if foreign_keys {
            conn.execute_batch("PRAGMA foreign_keys = OFF")?;
        }
        let applied = apply_one(conn, sql, index as i64 + 1);
        if foreign_keys {
            conn.execute_batch("PRAGMA foreign_keys = ON")?;
        }
        applied?;
    }
    Ok(())
}

fn apply_one(conn: &mut Connection, sql: &str, version: i64) -> Result<(), MigrateError> {
    let tx = conn.transaction()?;
    tx.execute_batch(sql)?;
    if tx.prepare("PRAGMA foreign_key_check")?.exists([])? {
        return Err(MigrateError::ForeignKeys);
    }
    tx.pragma_update(None, "user_version", version)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
    }

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row("SELECT count(*) FROM sqlite_master WHERE name = ?1", [name], |r| r.get::<_, i64>(0))
            .unwrap()
            == 1
    }

    #[test]
    fn fresh_database_is_built_from_migrations_only() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
        for table in ["settings", "categories", "budgets", "recurring_transactions", "transactions", "audit_log"] {
            assert!(table_exists(&conn, table), "missing table {table}");
        }
    }

    #[test]
    fn running_twice_changes_nothing() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        conn.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')", []).unwrap();
        run(&mut conn).unwrap();
        let rows: i64 = conn.query_row("SELECT count(*) FROM categories", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn only_missing_migrations_are_applied() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn, &["CREATE TABLE a (id INTEGER);"]).unwrap();
        apply(&mut conn, &["CREATE TABLE a (id INTEGER);", "CREATE TABLE b (id INTEGER);"]).unwrap();
        assert_eq!(version(&conn), 2);
        assert!(table_exists(&conn, "b"));
    }

    #[test]
    fn making_the_recurring_amount_optional_keeps_everything_that_pointed_at_it() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run_up_to(&mut conn, 5).unwrap();
        conn.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Rent')", []).unwrap();
        conn.execute(
            "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date)
             VALUES (1, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-02-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date)
             VALUES (1, 1, '2026-01-01', 'Rent', 90000, 90000, '2026-01-01')",
            [],
        )
        .unwrap();
        let audit_before: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();

        run(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
        assert!(conn.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, bool>(0)).unwrap(), "foreign keys are back on");

        // The row and the link from the paid transaction survived the rebuild.
        let (name, amount): (String, i64) =
            conn.query_row("SELECT name, amount_minor FROM recurring_transactions WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((name.as_str(), amount), ("Rent", 90000));
        let linked: Option<i64> = conn.query_row("SELECT recurring_transaction_id FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(linked, Some(1), "paying history is not detached");
        assert_eq!(conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_, i64>(0)).unwrap(), audit_before, "the copy is not history");

        // The new rule: no amount is fine, zero and negative are not.
        conn.execute("INSERT INTO recurring_transactions (category_id, name, start_date, interval_unit, next_due_date) VALUES (1, 'Electricity', '2026-01-20', 'MONTH', '2026-01-20')", []).unwrap();
        for bad in ["0", "-5", "1000000000000"] {
            let sql = format!("INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date) VALUES (1, 'x', {bad}, '2026-01-01', 'MONTH', '2026-01-01')");
            assert!(conn.execute(&sql, []).is_err(), "amount {bad} should be refused");
        }

        // Triggers and indexes were made again, and the link still clears when the item is deleted.
        let audited: i64 = conn.query_row("SELECT count(*) FROM audit_log WHERE entity_type = 'recurring_transactions' AND action = 'INSERT'", [], |r| r.get(0)).unwrap();
        assert!(audited >= 2, "the audit trigger works again");
        conn.execute("UPDATE recurring_transactions SET name = 'Rent flat' WHERE id = 1", []).unwrap();
        let updated_at_moved: bool = conn
            .query_row("SELECT updated_at > created_at OR updated_at = created_at FROM recurring_transactions WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(updated_at_moved);
        for index in ["idx_recurring_due", "idx_recurring_category"] {
            assert_eq!(conn.query_row("SELECT count(*) FROM sqlite_master WHERE name = ?1", [index], |r| r.get::<_, i64>(0)).unwrap(), 1, "{index}");
        }
        conn.execute("DELETE FROM recurring_transactions WHERE id = 1", []).unwrap();
        let after_delete: Option<i64> = conn.query_row("SELECT recurring_transaction_id FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(after_delete, None, "the foreign key action still works once the keys are on");
    }

    #[test]
    fn a_migration_that_leaves_a_dangling_row_is_rolled_back() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        let result = apply(
            &mut conn,
            &[
                "CREATE TABLE parent (id INTEGER PRIMARY KEY); CREATE TABLE child (parent_id INTEGER REFERENCES parent (id));",
                "INSERT INTO child VALUES (42);",
            ],
        );
        assert!(matches!(result, Err(MigrateError::ForeignKeys)));
        assert_eq!(version(&conn), 1, "the bad migration was not kept");
        assert_eq!(conn.query_row("SELECT count(*) FROM child", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert!(conn.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, bool>(0)).unwrap(), "foreign keys are on again after a failure");
    }

    #[test]
    fn newer_database_is_refused_and_left_untouched() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", MIGRATIONS.len() as i64 + 1).unwrap();
        let err = run(&mut conn).unwrap_err();
        assert!(matches!(err, MigrateError::DatabaseTooNew { .. }));
        assert_eq!(version(&conn), MIGRATIONS.len() as i64 + 1);
        assert!(!table_exists(&conn, "categories"));
    }

    #[test]
    fn failed_migration_rolls_back_and_keeps_earlier_ones() {
        let mut conn = Connection::open_in_memory().unwrap();
        let result = apply(
            &mut conn,
            &["CREATE TABLE a (id INTEGER);", "CREATE TABLE b (id INTEGER); THIS IS NOT SQL;"],
        );
        assert!(matches!(result, Err(MigrateError::Sqlite(_))));
        assert_eq!(version(&conn), 1);
        assert!(table_exists(&conn, "a"));
        assert!(!table_exists(&conn, "b"));
    }
}
