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

/// Loads the demo categories, budgets, recurring items, transactions, friends, accounts and stashes into a migrated database that has
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
            assert_eq!(count(&conn, "accounts"), 6, "{today}");
            assert!(count(&conn, "transfers") >= 18, "{today}: {} transfers", count(&conn, "transfers"));
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
            assert_eq!(names, ["Current account", "Savings", "Cash", "Credit card", "Lei account", "Old savings"], "{today}: the person's order, the archived one last");
            let kinds: Vec<Kind> = list.iter().map(|a| a.kind).collect();
            assert_eq!(kinds, [Kind::Checking, Kind::Savings, Kind::Cash, Kind::CreditCard, Kind::Checking, Kind::Savings], "{today}");
            let (current, savings, cash, card, lei, old) = (&list[0], &list[1], &list[2], &list[3], &list[4], &list[5]);
            // the main account is the current account: an IBAN and an opening balance sixteen months back
            assert!(current.is_main && !current.archived && current.number.as_deref().is_some_and(|n| n.starts_with("IE")), "{today}");
            assert!(money(&current.opening_balance) > 0, "{today}");
            assert_eq!(current.opening_date, sixteen_months_back(&conn, today), "{today}");
            // one that is archived, with history, and that holds nothing: what was left went to the savings
            assert!(old.archived && old.entries > 1 && money(&old.balance) == 0, "{today}: {old:?}");
            assert_eq!(list.iter().filter(|a| a.archived).count(), 1, "{today}");
            // the card is in the red, and the others are not
            assert!(money(&card.balance) < 0 && money(&card.opening_balance) < 0, "{today}: {card:?}");
            for held in [current, savings, cash, lei] {
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
    fn the_accounts_of_the_seed_have_their_details_and_net_worth_leaves_the_archived_one_out() {
        use crate::accounts::{self, Account};
        for today in DAYS {
            let conn = seeded(today);
            let list = accounts::list(&conn, today).unwrap();
            let [current, savings, cash, card, lei, old] = &list[..] else { panic!("{today}: {} accounts", list.len()) };
            let details = |a: &Account| (a.bic.clone(), a.notes.clone(), a.group.clone(), a.include_in_net_worth, a.virtual_balance.clone());
            // the credit card has a limit of 1000.00 as its virtual balance, and a BIC: what it can still pay is its balance and the limit
            assert_eq!(details(card), (Some("COBADEFFXXX".into()), None, None, true, "1000.00".into()), "{today}");
            assert_eq!(money(&card.available), money(&card.balance) + 100_000, "{today}: {card:?}");
            assert!(money(&card.balance) < 0 && money(&card.available) < 100_000, "{today}: it is in the red, so less than the whole limit is left");
            // the savings have a group and a note of two lines; the current account a group
            assert_eq!(details(savings), (None, Some("Emergency fund\nAim: six months of rent".into()), Some("Savings".into()), true, "0.00".into()), "{today}");
            assert_eq!(savings.notes.as_deref().unwrap().lines().count(), 2, "{today}");
            assert_eq!(details(current), (None, None, Some("Everyday".into()), true, "0.00".into()), "{today}");
            // cash has none, and the archived account is left out of net worth
            assert_eq!(details(cash), (None, None, None, true, "0.00".into()), "{today}");
            assert_eq!(details(old), (None, None, None, false, "0.00".into()), "{today}");
            assert_eq!(details(lei), (None, None, Some("Everyday".into()), true, "0.00".into()), "{today}");
            assert!(old.archived && list.iter().filter(|a| !a.include_in_net_worth).count() == 1, "{today}: only the archived one");
            for a in [current, savings, cash, lei, old] {
                assert_eq!(a.available, a.balance, "{today}: {} has no virtual balance, so what is available is what it holds", a.name);
            }
            assert!(accounts::check(&conn).unwrap(), "{today}");
            // net worth counts the others and not it, and `accounts` still counts all five
            let summary = accounts::summary(&conn, today).unwrap();
            let counted: i64 = list.iter().filter(|a| a.include_in_net_worth).map(|a| money(&a.balance_in_main)).sum();
            assert_eq!((money(&summary.in_accounts), summary.accounts, summary.in_use), (counted, 6, true), "{today}");
            // it holds nothing, so prove it by putting something in it: net worth does not move, until the account is counted
            conn.execute("INSERT INTO transactions (category_id, name, amount_minor, payment_date, account_id) VALUES (1, 'Found money', 10000, ?1, 5)", [today]).unwrap();
            assert_eq!(accounts::get(&conn, 5, today).unwrap().balance, "100.00", "{today}");
            assert_eq!(accounts::summary(&conn, today).unwrap(), summary, "{today}: the archived account is not in net worth");
            conn.execute("UPDATE accounts SET include_net_worth = 1 WHERE id = 5", []).unwrap();
            assert_eq!(money(&accounts::summary(&conn, today).unwrap().in_accounts), counted + 10_000, "{today}: and when it is counted it is");
        }
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
            // (the account in lei is in lei: its opening balance is turned into euro at its rate of 5.00; its payments have their amount in euro)
            let by_hand: i64 = conn
                .query_row(
                    "SELECT (SELECT COALESCE(SUM(CASE WHEN currency IS NULL THEN opening_balance_minor ELSE opening_balance_minor / 5 END), 0) FROM accounts)
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
            assert_eq!(list.iter().map(|a| money(&a.balance_in_main)).sum::<i64>(), by_hand, "{today}: the accounts add up to what is in them, the lei at their rate");
            // what left one account reached another: in one currency to the cent, and between the euro and the lei at 5.00 lei to the euro
            let moved: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(m.delta_minor), 0) FROM account_movements m JOIN transfers x ON x.id = m.source_id JOIN accounts a ON a.id = m.account_id
                      WHERE m.source IN ('TRANSFER_IN', 'TRANSFER_OUT') AND a.currency IS NULL
                        AND (SELECT currency FROM accounts WHERE id = x.from_account_id) IS NULL AND (SELECT currency FROM accounts WHERE id = x.to_account_id) IS NULL",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(moved, 0, "{today}: what left one account in euro reached another");
            let across: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(CASE WHEN f.currency IS NULL THEN -x.amount_minor ELSE -x.amount_minor / 5 END
                                       + CASE WHEN t.currency IS NULL THEN COALESCE(x.to_amount_minor, x.amount_minor) ELSE COALESCE(x.to_amount_minor, x.amount_minor) / 5 END), 0)
                       FROM transfers x JOIN accounts f ON f.id = x.from_account_id JOIN accounts t ON t.id = x.to_account_id",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(across, 0, "{today}: nor does a transfer between the euro and the lei make or lose any value at the rate");
            // each account's statement over the whole history ends at its balance
            for account in &list {
                let statement = accounts::statement(&conn, account.id, "2000-01-01", today, today).unwrap();
                assert_eq!((statement.balance_before.as_str(), statement.balance_after.as_str()), ("0.00", account.balance.as_str()), "{today}: {}", account.name);
            }
            // and it is what the dashboard would say
            assert_eq!(money(&summary.net_worth), money(&summary.in_accounts) + money(&summary.friends), "{today}");
            assert!(summary.in_use && summary.accounts == 6, "{today}");
        }
    }

    #[test]
    fn the_lei_account_of_the_seed_is_a_foreign_account_that_follows_every_rule_and_holds_what_its_rows_say_on_any_day() {
        use crate::accounts;
        for today in DAYS {
            let mut conn = seeded(today);
            assert!(accounts::check(&conn).unwrap(), "{today}");
            let lei = accounts::get(&conn, 6, today).unwrap();
            assert_eq!((lei.name.as_str(), lei.currency.as_str(), lei.foreign, lei.rate.as_deref()), ("Lei account", "RON", true, Some("5.00")), "{today}");
            assert!(!lei.archived && lei.include_in_net_worth && lei.group.as_deref() == Some("Everyday"), "{today}");
            // opened months back, with ten expenses and two incomes that carry what the account really paid, and the amount in euro is a fifth of it
            let (expenses, incomes): (i64, i64) = conn
                .query_row(
                    "SELECT SUM(c.type = 'EXPENSE'), SUM(c.type = 'INCOME') FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.account_id = 6",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!((expenses, incomes), (10, 2), "{today}");
            let odd: i64 = conn.query_row("SELECT count(*) FROM transactions WHERE account_id = 6 AND (native_amount_minor IS NULL OR native_amount_minor <> amount_minor * 5)", [], |r| r.get(0)).unwrap();
            assert_eq!(odd, 0, "{today}: every row in it has its native amount, five times its amount in euro");
            let elsewhere: i64 = conn.query_row("SELECT count(*) FROM transactions WHERE account_id <> 6 AND native_amount_minor IS NOT NULL", [], |r| r.get(0)).unwrap();
            assert_eq!(elsewhere, 0, "{today}: no other row has one");
            // its balance is its opening balance and the sum of what its rows say in lei, which is what the statement ends with
            let native: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(CASE c.type WHEN 'INCOME' THEN t.native_amount_minor ELSE -t.native_amount_minor END), 0)
                       FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.account_id = 6 AND t.payment_date <= ?1",
                    [today],
                    |r| r.get(0),
                )
                .unwrap();
            // one transfer in lei (euro out of the current account) and one out (euro in): what arrives and what leaves, each in lei
            let moved: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(CASE WHEN to_account_id = 6 THEN to_amount_minor ELSE -amount_minor END), 0) FROM transfers WHERE (to_account_id = 6 OR from_account_id = 6) AND transfer_date <= ?1",
                    [today],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(money(&lei.balance), 150_000 + native + moved, "{today}: {lei:?}");
            assert_eq!(money(&lei.balance_in_main), money(&lei.balance) / 5, "{today}: at its rate, exactly");
            let statement = accounts::statement(&conn, 6, "2000-01-01", today, today).unwrap();
            assert_eq!((statement.currency.as_str(), statement.balance_after.as_str()), ("RON", lei.balance.as_str()), "{today}");
            // the transfers between it and the current account, one each way, both amounts
            let both: Vec<(i64, i64, i64, i64)> = conn
                .prepare("SELECT from_account_id, to_account_id, amount_minor, to_amount_minor FROM transfers WHERE to_account_id = 6 OR from_account_id = 6 ORDER BY transfer_date")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert_eq!(both, [(1, 6, 20_000, 100_000), (6, 1, 50_000, 10_000)], "{today}");
            // the dashboard counts it in euro: the sum of what each account that counts is worth in the app's currency
            let list = accounts::list(&conn, today).unwrap();
            let worth: i64 = list.iter().filter(|a| a.include_in_net_worth).map(|a| money(&a.balance_in_main)).sum();
            assert_eq!(money(&accounts::summary(&conn, today).unwrap().in_accounts), worth, "{today}");
            // no recurring item and no entry of the ledger is in it
            for table in ["recurring_transactions", "ledger_entries"] {
                assert_eq!(conn.query_row(&format!("SELECT count(*) FROM {table} WHERE account_id = 6"), [], |r| r.get::<_, i64>(0)).unwrap(), 0, "{today}: {table}");
            }
            // the app's currency cannot be changed while it is there, a relabel included, and nothing is written by trying
            let before = crate::accounts::fixtures::snapshot(&conn);
            assert_eq!(crate::currency::change(&mut conn, "USD", Some("1.10"), today), Err(crate::vault::AppError::InvalidInput), "{today}");
            assert_eq!(crate::currency::change(&mut conn, "USD", None, today), Err(crate::vault::AppError::InvalidInput), "{today}");
            assert_eq!(crate::accounts::fixtures::snapshot(&conn), before, "{today}");
        }
    }

    #[test]
    fn the_stashes_of_the_seed_show_every_state_and_no_account_is_over_booked_on_any_day() {
        use crate::accounts::{self, Kind};
        for today in DAYS {
            let conn = seeded(today);
            assert_eq!(count(&conn, "stashes"), 5, "{today}");
            assert!(count(&conn, "stash_entries") >= 15, "{today}: {} entries", count(&conn, "stash_entries"));
            // none of them is over-booked: what the stashes of an account hold fits in what it has. (The credit card is in the red by design, so its
            // free money is below 0 as its balance is; no stash is in it.)
            for a in accounts::list(&conn, today).unwrap() {
                if a.kind == Kind::CreditCard {
                    assert!(money(&a.free) < 0 && a.free == a.balance && money(&a.set_aside) == 0, "{today}: {a:?}");
                } else {
                    assert!(money(&a.free) >= 0, "{today}: {} has {} free", a.name, a.free);
                }
                assert_eq!(money(&a.free), money(&a.balance) - money(&a.set_aside), "{today}: {}", a.name);
            }
            // each stash holds what the comment of the seed says, in its account's currency, and is within 0..=target (the rainy day has none)
            let stashes = crate::stashes::list(&conn, None, today).unwrap();
            type Shown<'a> = (&'a str, &'a str, &'a str, bool, &'a str, Option<&'a str>);
            let shown: Vec<Shown> =
                stashes.iter().map(|s| (s.name.as_str(), s.account.as_str(), s.currency.as_str(), s.archived, s.saved.as_str(), s.target.as_deref())).collect();
            assert_eq!(
                shown,
                [
                    ("Holiday in Lisbon", "Savings", "EUR", false, "560.00", Some("1200.00")),
                    ("New laptop", "Current account", "EUR", false, "900.00", Some("900.00")),
                    ("Rainy day", "Savings", "EUR", false, "500.00", None),
                    ("Wedding gift", "Lei account", "RON", false, "550.00", Some("1000.00")),
                    ("Bike", "Savings", "EUR", true, "0.00", Some("450.00")),
                ],
                "{today}: the active ones by position, then the archived one"
            );
            for s in &stashes {
                let (saved, target) = (money(&s.saved), s.target.as_deref().map(money));
                assert!(saved >= 0 && target.is_none_or(|t| saved <= t), "{today}: {} holds {} of {:?}", s.name, s.saved, s.target);
            }
            let by_name = |name: &str| stashes.iter().find(|s| s.name == name).unwrap();
            assert!(by_name("New laptop").reached && by_name("New laptop").percent == Some(100), "{today}: reached");
            let bike = by_name("Bike");
            assert!(bike.archived && bike.saved == "0.00" && bike.entries == 4 && !bike.reached, "{today}: reached once, then spent: {bike:?}");
            assert!(by_name("Rainy day").percent.is_none() && by_name("Rainy day").left.is_none(), "{today}: no target");
            let holiday = by_name("Holiday in Lisbon");
            assert_eq!((holiday.entries, holiday.left.as_deref(), holiday.percent, holiday.overdue), (7, Some("640.00"), Some(46), false), "{today}");
            assert!(holiday.per_month.is_some(), "{today}: a date seven months ahead, so a sum to save each month");
            // the stashes are numbered from 0, as the lists sort them
            let positions: Vec<i64> = conn.prepare("SELECT position FROM stashes ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
            assert_eq!(positions, [0, 1, 2, 3, 4], "{today}");
            let days_ahead: i64 = conn.query_row("SELECT CAST(julianday(target_date) - julianday(?1) AS INTEGER) FROM stashes WHERE id = 1", [today], |r| r.get(0)).unwrap();
            assert!((190..=240).contains(&days_ahead), "{today}: the holiday is {days_ahead} days ahead");
            let dated: i64 = conn.query_row("SELECT count(*) FROM stashes WHERE target_date IS NOT NULL", [], |r| r.get(0)).unwrap();
            assert_eq!(dated, 1, "{today}: only the holiday has a date");

            // what each account sets aside is the sum of its stashes
            let set_aside = |id: i64| accounts::get(&conn, id, today).unwrap().set_aside;
            assert_eq!([1, 2, 3, 4, 6].map(set_aside), ["900.00", "1060.00", "0.00", "0.00", "550.00"], "{today}: the archived bike holds nothing");

            // the entries: notes on some, the one take of the holiday and the one that spent the bike, and none is dated after today
            let note_of = |stash: i64, minor: i64| -> Option<String> {
                conn.query_row("SELECT note FROM stash_entries WHERE stash_id = ?1 AND amount_minor = ?2", [stash, minor], |r| r.get(0)).unwrap()
            };
            assert_eq!(note_of(1, -4_000).as_deref(), Some("deposit for the flat"), "{today}");
            assert_eq!(note_of(4, -45_000).as_deref(), Some("bought it"), "{today}");
            let (noted, plain): (i64, i64) = conn.query_row("SELECT count(note), count(*) - count(note) FROM stash_entries", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            assert!(noted >= 8 && plain >= 5, "{today}: {noted} entries with a note and {plain} without");
            let takes: i64 = conn.query_row("SELECT count(*) FROM stash_entries WHERE amount_minor < 0", [], |r| r.get(0)).unwrap();
            assert_eq!(takes, 2, "{today}");
            let future: i64 = conn.query_row("SELECT count(*) FROM stash_entries WHERE entry_date > ?1", [today], |r| r.get(0)).unwrap();
            assert_eq!(future, 0, "{today}: none is in the future");

            // five adds of the holiday came with a transfer of the same amount and day from the current account to the savings, and the sixth did not
            let linked: Vec<(i64, i64, i64, i64, String)> = conn
                .prepare(
                    "SELECT e.amount_minor, x.amount_minor, x.from_account_id, x.to_account_id, x.note
                     FROM stash_entries e JOIN transfers x ON x.id = e.transfer_id WHERE e.entry_date = x.transfer_date ORDER BY e.entry_date",
                )
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert_eq!(linked, vec![(10_000, 10_000, 1, 2, "Holiday in Lisbon".to_string()); 5], "{today}");
            let unlinked: i64 = conn.query_row("SELECT count(*) FROM stash_entries WHERE stash_id = 1 AND amount_minor > 0 AND transfer_id IS NULL", [], |r| r.get(0)).unwrap();
            assert_eq!(unlinked, 1, "{today}");
            let others: i64 = conn.query_row("SELECT count(*) FROM stash_entries WHERE stash_id <> 1 AND transfer_id IS NOT NULL", [], |r| r.get(0)).unwrap();
            assert_eq!(others, 0, "{today}");

            // the rules held on every day an entry was made, not only today: the account had the room for what was set aside by then
            let over: i64 = conn
                .query_row(
                    "SELECT count(*) FROM (SELECT DISTINCT t.account_id, e.entry_date FROM stash_entries e JOIN stashes t ON t.id = e.stash_id) d
                      WHERE (SELECT COALESCE(SUM(m.delta_minor), 0) FROM account_movements m WHERE m.account_id = d.account_id AND m.day <= d.entry_date)
                          < (SELECT COALESCE(SUM(MAX(held.saved, 0)), 0)
                               FROM (SELECT (SELECT COALESCE(SUM(x.amount_minor), 0) FROM stash_entries x WHERE x.stash_id = s.id AND x.entry_date <= d.entry_date) AS saved
                                       FROM stashes s WHERE s.account_id = d.account_id) held)",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(over, 0, "{today}: no account was over-booked on the day of an entry");

            // the demo rows are written through the triggers: stamped, and no history
            let unstamped: i64 = conn.query_row("SELECT (SELECT count(*) FROM stashes WHERE stamp IS NULL) + (SELECT count(*) FROM stash_entries WHERE stamp IS NULL)", [], |r| r.get(0)).unwrap();
            assert_eq!(unstamped, 0, "{today}");
            assert_eq!(count(&conn, "audit_log"), 0, "{today}");
            assert!(accounts::check(&conn).unwrap(), "{today}");
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
