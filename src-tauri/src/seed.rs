//! Development demo data. Not used by the app itself; see `examples/dev_vault.rs` and `examples/seed.rs`.

use chrono::NaiveDate;
use rusqlite::Connection;

const DEV_SEED: &str = include_str!("../seeds/dev.sql");

#[derive(Debug)]
pub enum SeedError {
    /// Seeding twice would duplicate or collide with existing rows.
    NotEmpty,
    /// The date for "today" is not a date.
    BadDate,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeedError::NotEmpty => write!(f, "database already has data; seed a fresh database"),
            SeedError::BadDate => write!(f, "today must be a date such as 2026-10-14"),
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

/// Loads the demo categories, budgets, recurring items and transactions into a migrated database that has
/// none yet, with sixteen months of history ending today. An existing settings row is left as it is.
pub fn run(conn: &mut Connection) -> Result<(), SeedError> {
    run_for(conn, &chrono::Local::now().date_naive().to_string())
}

/// Like `run`, with the day that counts as today. The tests use it to try month ends and leap days.
pub fn run_for(conn: &mut Connection, today: &str) -> Result<(), SeedError> {
    let date = NaiveDate::parse_from_str(today, "%Y-%m-%d").map_err(|_| SeedError::BadDate)?;
    let tx = conn.transaction()?;
    let used: i64 = tx.query_row("SELECT (SELECT count(*) FROM categories) + (SELECT count(*) FROM transactions)", [], |r| r.get(0))?;
    if used > 0 {
        return Err(SeedError::NotEmpty);
    }
    tx.execute_batch(&DEV_SEED.replace("@today", &format!("'{date}'")))?;
    // Demo rows are not user history.
    tx.execute("DELETE FROM audit_log", [])?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAYS: [&str; 10] = [
        "2026-10-14", "2026-01-01", "2026-03-31", "2026-12-31", "2024-02-29", "2025-02-28", "2026-05-31", "2027-01-02",
        "2026-02-01", "2026-06-15",
    ];

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn
    }

    fn seeded(today: &str) -> Connection {
        let mut conn = db();
        run_for(&mut conn, today).unwrap();
        conn
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn the_seed_loads_for_any_day_and_passes_every_constraint() {
        for today in DAYS {
            let conn = seeded(today);
            assert_eq!(count(&conn, "settings"), 1, "{today}");
            assert_eq!(count(&conn, "categories"), 15, "{today}");
            assert!(count(&conn, "budgets") >= 12, "{today}");
            assert_eq!(count(&conn, "recurring_transactions"), 12, "{today}");
            assert_eq!(count(&conn, "people"), 4, "{today}");
            assert!(count(&conn, "ledger_entries") >= 9 && count(&conn, "transaction_split_lines") >= 16, "{today}: debts");
            assert!(count(&conn, "transactions") > 250, "{today}: {} transactions", count(&conn, "transactions"));
            assert_eq!(count(&conn, "audit_log"), 0, "{today}: demo rows are not user history");
        }
    }

    #[test]
    fn the_debts_of_the_seed_follow_the_rules_and_show_every_state_on_any_day() {
        for today in DAYS {
            let conn = seeded(today);
            assert!(crate::debts::splits::check(&conn).unwrap(), "{today}: the parts and templates follow the rules");
            // overdue: the yoga class (two or three occurrences) and the top-up with no fixed amount; due soon: the veg box
            let due = crate::debts::dues::due_payments(&conn, today).unwrap();
            for name in ["Yoga class", "Mobile top-up"] {
                assert!(due.iter().any(|d| d.name == name), "{today}: {name} is overdue");
            }
            assert!(due.iter().any(|d| d.name == "Mobile top-up" && d.amount.is_none()), "{today}: a bill with no fixed amount");
            assert!(crate::recurring::dues(&conn, today, Some(7)).unwrap().upcoming.iter().any(|r| r.name == "Veg box"), "{today}: due in five days");
            // who owes what: Marta and Sorin owe, Tomás is owed, Priya has paid back everything
            let summary = crate::debts::summary::summary(&conn).unwrap();
            let owes: Vec<(&str, bool)> = summary.people.iter().map(|p| (p.name.as_str(), p.balance.starts_with('-'))).collect();
            assert_eq!(owes.len(), 3, "{today}: {owes:?}");
            assert!(owes.contains(&("Tomás", true)) && owes.contains(&("Marta", false)) && owes.contains(&("Sorin", false)), "{today}: {owes:?}");
            // the history has the friends' lines and the payments that were late or are overdue, and nothing that was paid on time
            let lines = crate::debts::history::lines(&conn, today, &Default::default()).unwrap();
            assert!(lines.iter().any(|l| l.kind == "PAYMENT_PAID") && lines.iter().any(|l| l.kind == "PAYMENT_DUE") && lines.iter().any(|l| l.kind == "BILL"), "{today}");
            assert!(lines.iter().filter(|l| l.kind == "PAYMENT_PAID").all(|l| l.due_date.as_deref().is_some_and(|due| due < l.date.as_str())), "{today}: only the late ones");
        }
    }

    #[test]
    fn seeding_twice_is_refused_and_changes_nothing() {
        let mut conn = seeded("2026-10-14");
        assert!(matches!(run_for(&mut conn, "2026-10-14"), Err(SeedError::NotEmpty)));
        assert_eq!(count(&conn, "categories"), 15);
        assert!(matches!(run_for(&mut db(), "tomorrow"), Err(SeedError::BadDate)));
    }
}
