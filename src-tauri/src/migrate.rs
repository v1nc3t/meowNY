//! Schema migrations. Plain SQL files in `migrations/`, embedded in the binary and applied in
//! order on startup. The applied version is stored in SQLite's `user_version` pragma.

use rusqlite::Connection;

/// Ordered by version: migration N is `MIGRATIONS[N - 1]`. Append only, never edit a released one.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_tables.sql"),
    include_str!("../migrations/0002_triggers.sql"),
];

#[derive(Debug)]
pub enum MigrateError {
    /// The database was written by a newer app. Opening it could corrupt data we do not understand.
    DatabaseTooNew { database: i64, app: usize },
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for MigrateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrateError::DatabaseTooNew { database, app } => write!(
                f,
                "this database is schema version {database} but this app only supports up to {app}; update the app"
            ),
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

/// Brings the database up to the latest schema. Each migration runs in its own transaction,
/// so a failure leaves the database at the last good version.
pub fn run(conn: &mut Connection) -> Result<(), MigrateError> {
    apply(conn, MIGRATIONS)
}

fn apply(conn: &mut Connection, migrations: &[&str]) -> Result<(), MigrateError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > migrations.len() as i64 {
        return Err(MigrateError::DatabaseTooNew { database: current, app: migrations.len() });
    }
    for (index, sql) in migrations.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
    }
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
