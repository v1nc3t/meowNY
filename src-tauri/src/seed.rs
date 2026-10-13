//! Development demo data. Not used by the app itself; see `examples/seed.rs`.

use rusqlite::Connection;

const DEV_SEED: &str = include_str!("../seeds/dev.sql");

#[derive(Debug)]
pub enum SeedError {
    /// Seeding twice would duplicate or collide with existing rows.
    NotEmpty,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeedError::NotEmpty => write!(f, "database already has data; seed a fresh database"),
            SeedError::Sqlite(e) => write!(f, "seed failed: {e}"),
        }
    }
}

impl std::error::Error for SeedError {}

impl From<rusqlite::Error> for SeedError {
    fn from(e: rusqlite::Error) -> Self {
        SeedError::Sqlite(e)
    }
}

/// Loads the demo categories, budgets, recurring items and transactions into a migrated, empty database.
pub fn run(conn: &mut Connection) -> Result<(), SeedError> {
    let tx = conn.transaction()?;
    let used: i64 = tx.query_row(
        "SELECT (SELECT count(*) FROM settings) + (SELECT count(*) FROM categories) + (SELECT count(*) FROM transactions)",
        [],
        |r| r.get(0),
    )?;
    if used > 0 {
        return Err(SeedError::NotEmpty);
    }
    tx.execute_batch(DEV_SEED)?;
    // Demo rows are not user history.
    tx.execute("DELETE FROM audit_log", [])?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        run(&mut conn).unwrap();
        conn
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn seed_loads_demo_data_that_passes_every_constraint() {
        let conn = seeded();
        assert_eq!(count(&conn, "settings"), 1);
        assert_eq!(count(&conn, "categories"), 9);
        assert!(count(&conn, "budgets") >= 6);
        assert_eq!(count(&conn, "recurring_transactions"), 2);
        assert!(count(&conn, "transactions") > 20);
        assert_eq!(count(&conn, "audit_log"), 0);
    }

    #[test]
    fn seed_refuses_a_database_that_already_has_data() {
        let mut conn = seeded();
        assert!(matches!(run(&mut conn), Err(SeedError::NotEmpty)));
        assert_eq!(count(&conn, "categories"), 9);
    }

    #[test]
    fn changes_after_seeding_are_audited_again() {
        let conn = seeded();
        conn.execute("UPDATE categories SET name = 'Food' WHERE id = 4", []).unwrap();
        assert_eq!(count(&conn, "audit_log"), 1);
    }
}
