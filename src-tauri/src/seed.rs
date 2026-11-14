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

/// Loads the demo categories, budgets, recurring items, transactions, friends and accounts into a migrated database that has
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
            assert_eq!(count(&conn, "accounts"), 5, "{today}");
            assert!(count(&conn, "transfers") >= 16, "{today}: {} transfers", count(&conn, "transfers"));
            assert!(crate::accounts::check(&conn).unwrap(), "{today}: the main account is as it must be");
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

    fn money(text: &str) -> i64 {
        crate::money::parse_signed_amount(text).unwrap()
    }

    #[test]
    fn the_accounts_of_the_seed_show_every_state_and_each_has_what_it_should_on_any_day() {
        use crate::accounts::{self, Kind};
        for today in DAYS {
            let conn = seeded(today);
            let list = accounts::list(&conn, today).unwrap();
            let names: Vec<&str> = list.iter().map(|a| a.name.as_str()).collect();
            assert_eq!(names, ["Current account", "Savings", "Cash", "Credit card", "Old savings"], "{today}: the person's order, the archived one last");
            let kinds: Vec<Kind> = list.iter().map(|a| a.kind).collect();
            assert_eq!(kinds, [Kind::Checking, Kind::Savings, Kind::Cash, Kind::CreditCard, Kind::Savings], "{today}");
            let (current, savings, cash, card, old) = (&list[0], &list[1], &list[2], &list[3], &list[4]);
            // the main account is the current account: an IBAN and an opening balance sixteen months back
            assert!(current.is_main && !current.archived && current.number.as_deref().is_some_and(|n| n.starts_with("IE")), "{today}");
            assert!(money(&current.opening_balance) > 0, "{today}");
            assert_eq!(current.opening_date, sixteen_months_back(&conn, today), "{today}");
            // one that is archived, with history, and that holds nothing: what was left went to the savings
            assert!(old.archived && old.entries > 1 && money(&old.balance) == 0, "{today}: {old:?}");
            assert_eq!(list.iter().filter(|a| a.archived).count(), 1, "{today}");
            // the card is in the red, and the others are not
            assert!(money(&card.balance) < 0 && money(&card.opening_balance) < 0, "{today}: {card:?}");
            for held in [current, savings, cash] {
                assert!(money(&held.balance) > 0, "{today}: {} holds {}", held.name, held.balance);
            }
            // and none of them was below zero at the end of a month since they opened (the card never above)
            let year = &today[..4];
            for year in [year.parse::<i32>().unwrap() - 1, year.parse().unwrap()] {
                for account in &list {
                    for point in accounts::history(&conn, account.id, &year.to_string(), today).unwrap().iter().filter(|p| p.month[..] >= account.opening_date[..7]) {
                        let (held, in_the_red) = (money(&point.balance), account.kind == Kind::CreditCard);
                        assert!(in_the_red && held < 0 || !in_the_red && held >= 0, "{today}: {} on {}: {}", account.name, point.month, point.balance);
                    }
                }
            }
            assert!(accounts::check(&conn).unwrap(), "{today}");
        }
    }

    /// The first day of the month sixteen months before `today`, which is when the history of the demo starts.
    fn sixteen_months_back(conn: &Connection, today: &str) -> String {
        conn.query_row("SELECT date(?1, 'start of month', '-16 months')", [today], |r| r.get(0)).unwrap()
    }

    #[test]
    fn what_the_seed_put_in_each_account_is_there_and_the_balances_add_up() {
        use crate::accounts::{self, MAIN_ID};
        for today in DAYS {
            let conn = seeded(today);
            let in_account = |sql: &str, id: i64| -> i64 { conn.query_row(sql, [id], |r| r.get(0)).unwrap() };
            let (cash, card, savings) = (3, 4, 2);
            // some transactions in cash and on the card, the interest in the savings, a recurring item on the card with all its payments, and
            // a friend who paid back in cash
            assert!(in_account("SELECT count(*) FROM transactions WHERE account_id = ?1", cash) > 20, "{today}: cash");
            assert!(in_account("SELECT count(*) FROM transactions WHERE account_id = ?1", card) > 15, "{today}: the card");
            assert!(in_account("SELECT count(*) FROM transactions WHERE account_id = ?1", savings) >= 4, "{today}: interest");
            assert_eq!(in_account("SELECT count(*) FROM recurring_transactions WHERE account_id = ?1", card), 1, "{today}");
            assert_eq!(in_account("SELECT count(*) FROM transactions WHERE recurring_transaction_id = 2 AND account_id <> ?1", card), 0, "{today}: its payments are on the card");
            assert_eq!(in_account("SELECT count(*) FROM ledger_entries WHERE account_id = ?1 AND kind = 'REPAID_TO_ME'", cash), 1, "{today}: the friend paid back in cash");
            assert!(in_account("SELECT count(*) FROM transactions WHERE account_id = ?1", MAIN_ID) > 150, "{today}: the rest is in the current account");
            // the transfers: a monthly one to the savings and one that pays off the card, cash from the bank, and the old account paid into and emptied
            let transfers = |note: &str| -> i64 { conn.query_row("SELECT count(*) FROM transfers WHERE note = ?1", [note], |r| r.get(0)).unwrap() };
            assert!(transfers("Monthly saving") >= 15 && transfers("Credit card payment") >= 14, "{today}");
            assert!(transfers("Cash from the bank") >= 5 && transfers("Rainy day") == 6 && transfers("Closing the old account") == 1, "{today}");
            assert_eq!(in_account("SELECT count(*) FROM transfers WHERE from_account_id = ?1 AND to_account_id = 2 AND amount_minor = 180000", 5), 1, "{today}");
            let future: i64 = conn.query_row("SELECT count(*) FROM transfers WHERE transfer_date > ?1", [today], |r| r.get(0)).unwrap();
            assert_eq!(future, 0, "{today}: none is in the future");
            // the balances add up. What every account holds together is what the person had, what came in, less what they paid, and the ledger:
            // the transfers move money and add nothing
            let by_hand: i64 = conn
                .query_row(
                    "SELECT (SELECT COALESCE(SUM(opening_balance_minor), 0) FROM accounts)
                          + (SELECT COALESCE(SUM(CASE c.type WHEN 'INCOME' THEN t.amount_minor ELSE -t.amount_minor END), 0)
                               FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.paid_by IS NULL AND t.payment_date <= ?1)
                          + (SELECT COALESCE(SUM(CASE WHEN kind IN ('BORROWED', 'REPAID_TO_ME') THEN amount_minor ELSE -amount_minor END), 0)
                               FROM ledger_entries WHERE entry_date <= ?1)",
                    [today],
                    |r| r.get(0),
                )
                .unwrap();
            let summary = accounts::summary(&conn, today).unwrap();
            assert_eq!(money(&summary.in_accounts), by_hand, "{today}");
            let list = accounts::list(&conn, today).unwrap();
            assert_eq!(list.iter().map(|a| money(&a.balance)).sum::<i64>(), by_hand, "{today}: the accounts add up to what is in them");
            let moved: i64 = conn.query_row("SELECT COALESCE(SUM(delta_minor), 0) FROM account_movements WHERE source IN ('TRANSFER_IN', 'TRANSFER_OUT')", [], |r| r.get(0)).unwrap();
            assert_eq!(moved, 0, "{today}: what left one account reached another");
            // each account's statement over the whole history ends at its balance
            for account in &list {
                let statement = accounts::statement(&conn, account.id, "2000-01-01", today, today).unwrap();
                assert_eq!((statement.balance_before.as_str(), statement.balance_after.as_str()), ("0.00", account.balance.as_str()), "{today}: {}", account.name);
            }
            // and it is what the dashboard would say
            assert_eq!(money(&summary.net_worth), money(&summary.in_accounts) + money(&summary.friends), "{today}");
            assert!(summary.in_use && summary.accounts == 5, "{today}");
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
