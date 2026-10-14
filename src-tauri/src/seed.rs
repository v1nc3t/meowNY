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
    use crate::categories::CategoryType;
    use crate::recurring::{self, Unit};
    use crate::stats;

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

    fn months_ago(today: &str, months: i32) -> String {
        let d = NaiveDate::parse_from_str(today, "%Y-%m-%d").unwrap();
        let total = d.year_ce().1 as i32 * 12 + d.month0() as i32 - months;
        NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap().to_string()
    }

    use chrono::Datelike;

    #[test]
    fn the_seed_loads_for_any_day_and_passes_every_constraint() {
        for today in DAYS {
            let conn = seeded(today);
            assert_eq!(count(&conn, "settings"), 1, "{today}");
            assert_eq!(count(&conn, "categories"), 15, "{today}");
            assert!(count(&conn, "budgets") >= 12, "{today}");
            assert_eq!(count(&conn, "recurring_transactions"), 9, "{today}");
            assert!(count(&conn, "transactions") > 250, "{today}: {} transactions", count(&conn, "transactions"));
            assert_eq!(count(&conn, "audit_log"), 0, "{today}: demo rows are not user history");
        }
    }

    #[test]
    fn the_history_runs_sixteen_months_and_never_into_the_future() {
        for today in DAYS {
            let conn = seeded(today);
            let (first, last): (String, String) =
                conn.query_row("SELECT MIN(payment_date), MAX(payment_date) FROM transactions", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            assert!(last.as_str() <= today, "{today}: a transaction on {last}");
            assert!(first >= months_ago(today, 15), "{today}: history starts {first}");
            assert!(first <= months_ago(today, 14), "{today}: history starts only {first}");
        }
    }

    #[test]
    fn text_follows_the_rules_the_app_enforces() {
        for today in ["2026-10-14", "2024-02-29"] {
            let conn = seeded(today);
            let mut stmt = conn.prepare("SELECT name, description FROM transactions").unwrap();
            for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))).unwrap() {
                let (name, description) = row.unwrap();
                assert!(!name.is_empty() && name.chars().count() <= 50, "{name:?}");
                assert!(!name.chars().any(char::is_control), "{name:?}");
                if let Some(d) = description {
                    assert!(!d.is_empty() && d.chars().count() <= 255 && !d.chars().any(char::is_control), "{d:?}");
                    assert_eq!(d.trim(), d, "{d:?}");
                }
            }
        }
    }

    #[test]
    fn recurring_items_are_consistent_with_what_was_paid() {
        for today in DAYS {
            let conn = seeded(today);
            for item in recurring::list(&conn).unwrap() {
                let (id, unit, count) = (item.id, item.interval_unit, item.interval_count);
                let start = NaiveDate::parse_from_str(&item.start_date, "%Y-%m-%d").unwrap();
                let mut stmt = conn.prepare("SELECT recurring_due_date, payment_date FROM transactions WHERE recurring_transaction_id = ?1 ORDER BY recurring_due_date").unwrap();
                let paid: Vec<(String, String)> = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();

                // The paid occurrences are the first ones, in order, and the next due date is the one after them.
                for (k, (due, paid_on)) in paid.iter().enumerate() {
                    assert_eq!(*due, recurring::due_date(start, unit, count, k as u32).unwrap().to_string(), "{today}: {} #{k}", item.name);
                    assert!(paid_on.as_str() <= today && paid_on >= due, "{today}: {} paid {paid_on} for {due}", item.name);
                }
                assert_eq!(
                    item.next_due_date,
                    recurring::due_date(start, unit, count, paid.len() as u32).unwrap().to_string(),
                    "{today}: {} has {} paid",
                    item.name,
                    paid.len()
                );
            }
        }
    }

    #[test]
    fn recurring_items_show_every_state_on_any_day() {
        for today in DAYS {
            let conn = seeded(today);
            let items = recurring::list(&conn).unwrap();
            let by_name = |name: &str| items.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no {name}"));
            assert!(!by_name("Language course").is_active, "paused");
            let magazine = by_name("Magazine");
            assert!(!magazine.is_active && magazine.end_date.is_some(), "ended");
            assert!(magazine.next_due_date > magazine.end_date.clone().unwrap(), "ended items are past their end date");
            assert_eq!(by_name("Home insurance").interval_unit, Unit::Year);
            for active in ["Rent", "Netflix", "Spotify", "Electricity", "Home insurance", "Cleaning service", "Salary"] {
                assert!(by_name(active).is_active, "{active}");
            }
            let dues = recurring::dues(&conn, today, None).unwrap();
            let overdue: Vec<&str> = dues.overdue.iter().map(|r| r.name.as_str()).collect();
            assert!(overdue.contains(&"Cleaning service"), "{today}: overdue {overdue:?}");
            assert!(dues.upcoming.iter().chain(&dues.overdue).all(|r| r.is_active), "paused and ended items are not due");
        }
    }

    #[test]
    fn electricity_is_a_bill_with_no_fixed_amount() {
        let conn = seeded("2026-10-14");
        let item = recurring::list(&conn).unwrap().into_iter().find(|r| r.name == "Electricity").unwrap();
        assert_eq!(item.amount, None);
        let (planned, distinct, total): (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(planned_amount_minor), COUNT(DISTINCT amount_minor), COUNT(*) FROM transactions WHERE name = 'Electricity'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!(total >= 14, "{total} payments");
        assert_eq!(planned, 0, "nothing was planned");
        assert!(distinct * 2 > total, "the amounts differ from month to month: {distinct} of {total}");
    }

    #[test]
    fn the_cleaner_is_sometimes_paid_more_than_planned() {
        let conn = seeded("2026-10-14");
        let (more, same): (i64, i64) = conn
            .query_row(
                "SELECT SUM(amount_minor > planned_amount_minor), SUM(amount_minor = planned_amount_minor)
                 FROM transactions WHERE name = 'Cleaning service'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(more >= 2 && same >= 6, "{more} paid more, {same} as planned");
    }

    #[test]
    fn the_budget_screens_have_something_to_show_in_every_month() {
        let today = "2026-10-14";
        let conn = seeded(today);
        let (mut over_budget, mut without_budget, mut unbudgeted_entertainment) = (false, false, false);
        for ago in 0..=15 {
            let month = months_ago(today, ago)[..7].to_string();
            let view = stats::budget_vs_actual(&conn, &month).unwrap();
            for line in &view.categories {
                over_budget |= line.pct_used.is_some_and(|p| p > 100.0);
                without_budget |= line.budgeted.is_none() && line.actual != "0.00" && matches!(line.name.as_str(), "Health" | "Travel" | "Gifts");
                unbudgeted_entertainment |= ago < 4 && line.name == "Entertainment" && line.budgeted.is_none() && line.actual != "0.00";
            }
        }
        assert!(over_budget, "some category should go over its budget in some month");
        assert!(without_budget, "Health, Travel or Gifts should have spending and no budget");
        assert!(unbudgeted_entertainment, "Entertainment keeps being spent on after its budget was removed");

        // Rent went up and the global budget was raised: the limits differ between the first and the last month.
        let first = stats::budget_vs_actual(&conn, &months_ago(today, 15)[..7]).unwrap();
        let last = stats::budget_vs_actual(&conn, &months_ago(today, 0)[..7]).unwrap();
        let rent = |v: &stats::BudgetVsActual| v.categories.iter().find(|c| c.name == "Rent").unwrap().budgeted.clone();
        assert_eq!((rent(&first).as_deref(), rent(&last).as_deref()), (Some("900.00"), Some("950.00")));
    }

    #[test]
    fn a_deleted_category_keeps_its_history() {
        let conn = seeded("2026-10-14");
        let (deleted, history): (i64, i64) = conn
            .query_row(
                "SELECT (SELECT count(*) FROM categories WHERE name = 'Gym' AND deleted_at IS NOT NULL),
                        (SELECT count(*) FROM transactions t JOIN categories c ON c.id = t.category_id WHERE c.name = 'Gym')",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(deleted, 1);
        assert!(history >= 12);
    }

    #[test]
    fn income_and_spending_look_like_a_household() {
        let today = "2026-10-14";
        let conn = seeded(today);
        let year = stats::year_trend(&conn, "2026").unwrap();
        // The months that are complete: income comes in, most of it is kept, and the odd month goes into the red.
        for point in &year[..9] {
            let (income, expense) = (point.income.parse::<f64>().unwrap(), point.expense.parse::<f64>().unwrap());
            assert!(income > 3000.0 && expense > 1200.0 && expense < 4000.0, "{point:?}");
        }
        assert!(year.iter().take(9).any(|p| p.savings_rate.is_some_and(|r| r > 20.0)));
        let kinds: Vec<(String, i64)> = {
            let mut stmt = conn.prepare("SELECT c.type, count(*) FROM transactions t JOIN categories c ON c.id = t.category_id GROUP BY 1").unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
        };
        assert_eq!(kinds.len(), 2, "both income and expenses");
        let _ = CategoryType::Income;
    }

    #[test]
    fn seeding_twice_is_refused_and_changes_nothing() {
        let mut conn = seeded("2026-10-14");
        assert!(matches!(run_for(&mut conn, "2026-10-14"), Err(SeedError::NotEmpty)));
        assert_eq!(count(&conn, "categories"), 15);
        assert!(matches!(run_for(&mut db(), "tomorrow"), Err(SeedError::BadDate)));
    }

    #[test]
    fn an_existing_settings_row_is_kept() {
        let mut conn = db();
        conn.execute("INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, 'RON', 'ro-RO', 'Europe/Bucharest', 'ro')", []).unwrap();
        run_for(&mut conn, "2026-10-14").unwrap();
        let (currency, language): (String, String) =
            conn.query_row("SELECT currency, language FROM settings", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((currency.as_str(), language.as_str()), ("RON", "ro"));
        assert_eq!(count(&conn, "settings"), 1);
    }

    #[test]
    fn changes_after_seeding_are_audited_again() {
        let conn = seeded("2026-10-14");
        conn.execute("UPDATE categories SET name = 'Food' WHERE id = 5", []).unwrap();
        assert_eq!(count(&conn, "audit_log"), 1);
    }

    #[test]
    fn the_seed_goes_through_a_real_vault_like_the_dev_command_does() {
        use crate::keys::memory::MemoryStore;
        use crate::vault::{Settings, Vault};
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::new(dir.path().to_path_buf(), Box::new(MemoryStore::default()));
        let settings = Settings { currency: "RON".into(), locale: "ro-RO".into(), timezone: "Europe/Bucharest".into(), language: "ro".into() };
        vault.create(&settings, None).unwrap();
        vault.with_conn(|c| run(c).map_err(|_| crate::vault::AppError::Internal)).unwrap();
        assert_eq!(vault.settings().unwrap(), settings, "the vault's own settings win");
        assert!(vault.with_conn(|c| Ok(count(c, "transactions"))).unwrap() > 250);
        vault.lock();
        vault.unlock(None).unwrap();
        assert_eq!(vault.with_conn(|c| Ok(count(c, "categories"))).unwrap(), 15);
    }

    /// The files this app writes handle the awkward rows: accents, Romanian and Japanese letters, commas, quotes
    /// and text that looks like a formula.
    #[test]
    fn exports_cope_with_the_demo_data() {
        use crate::display::DisplayFormat;
        use crate::export::{self, Selection};
        let conn = seeded("2026-10-14");

        let csv = export::csv(&conn, &Selection::default()).unwrap();
        assert!(csv.contains("'=SUM(1+1)") && csv.contains("'-5% voucher (used)") && csv.contains("'@home delivery"), "formula-like cells are written as text");
        assert!(csv.contains("\"Lunch, \"\"team\"\" day\"") && csv.contains("Café Müller") && csv.contains("寿司"));
        assert!(!csv.lines().skip(1).any(|l| l.starts_with(['=', '+', '-', '@'])));

        let json: serde_json::Value = serde_json::from_str(&export::json(&conn, &Selection::default(), chrono::Utc::now()).unwrap()).unwrap();
        assert_eq!(json["categories"].as_array().unwrap().len(), 15);
        assert!(json["transactions"].as_array().unwrap().len() > 250);

        let book = export::workbook(&conn, &Selection::default(), &DisplayFormat::default()).unwrap();
        assert!(book.starts_with(b"PK"));

        let options: crate::report::ReportOptions = serde_json::from_value(serde_json::json!({
            "fromMonth": &months_ago("2026-10-14", 11)[..7], "toMonth": "2026-10", "includeTransactions": true, "showDescriptions": true,
            "labels": { "title": "meowNY report", "period": "last twelve months", "income": "Income", "expenses": "Expenses",
                "savings": "Savings", "savingsRate": "Savings rate", "byMonth": "By month", "spending": "Spending", "other": "Other",
                "budgetVsActual": "Budget vs actual", "noBudget": "no budget", "transactions": "Transactions", "date": "Date",
                "category": "Category", "name": "Name", "amount": "Amount", "page": "Page", "noData": "Nothing" }
        }))
        .unwrap();
        let pdf = crate::report::read(&conn, &options).unwrap().pdf(&options).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
    }
}
