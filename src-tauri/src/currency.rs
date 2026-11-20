//! Changing the currency. The currency is a label on plain numbers, so changing it on its own
//! would silently misstate every amount. Changing it therefore asks for a choice:
//!
//! * convert: every stored amount is multiplied by the exchange rate the person typed, in one
//!   database transaction, rounded to the cent (half up);
//! * relabel only: amounts stay as they are (fixing a mistaken choice).
//!
//! No rate is ever fetched; the person supplies it. The rate and date are written to the history.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::debts::splits;
use crate::money::MAX_MINOR;
use crate::sync;
use crate::vault::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct Conversion {
    pub transactions: u32,
    pub recurring: u32,
    pub budgets: u32,
    /// Entries of the ledger of money lent and borrowed.
    pub ledger: u32,
    /// Accounts with an opening balance or a virtual balance that is not 0 (one row counts once, whichever of the two it has or both), and
    /// transfers between them (`docs/accounts-design.md`, sections 8 and 20.3).
    pub accounts: u32,
    pub transfers: u32,
    /// Stashes in an account of the app's currency that had a target or at least one entry converted (a stash with neither is not counted), and
    /// the entries converted in them (`docs/stash-design.md`, section 8). A stash in an account with a currency of its own is left as it is.
    pub stashes: u32,
    pub stash_entries: u32,
}

/// An exchange rate as an exact fraction, so no floating point touches money.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rate {
    pub(crate) numerator: i128,
    pub(crate) denominator: i128,
}

/// "1.08", "0.0063", "160": 1 to 9 whole digits (no leading zeros), up to 8 decimals, above zero.
pub(crate) fn parse_rate(text: &str) -> Result<Rate, AppError> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let well_formed = !whole.is_empty()
        && whole.len() <= 9
        && digits(whole)
        && !(whole.len() > 1 && whole.starts_with('0'))
        && digits(fraction)
        && fraction.len() <= 8
        && !(text.contains('.') && fraction.is_empty());
    if !well_formed {
        return Err(AppError::InvalidInput);
    }
    let numerator: i128 = format!("{whole}{fraction}").parse().map_err(|_| AppError::InvalidInput)?;
    if numerator == 0 {
        return Err(AppError::InvalidInput);
    }
    Ok(Rate { numerator, denominator: 10i128.pow(fraction.len() as u32) })
}

/// Rounds half up. Amounts are never negative.
pub(crate) fn convert(minor: i64, rate: Rate) -> Result<i64, AppError> {
    let scaled = i128::from(minor) * rate.numerator;
    let rounded = (scaled * 2 + rate.denominator) / (2 * rate.denominator);
    i64::try_from(rounded).ok().filter(|v| *v <= MAX_MINOR).ok_or(AppError::InvalidInput)
}

/// One amount converted at a rate the person typed, with the same rules and rounding as a change of currency (half up, to the cent).
pub(crate) fn convert_at(minor: i64, rate: &str) -> Result<i64, AppError> {
    convert(minor, parse_rate(rate)?)
}

pub(crate) fn normalize_code(code: &str) -> Result<String, AppError> {
    let code = code.trim().to_ascii_uppercase();
    if code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(code)
    } else {
        Err(AppError::InvalidInput)
    }
}

/// Changes the currency. With `rate`, every stored amount is converted; without it, only the label changes.
/// Either everything changes or nothing does. It is refused while any account, archived or not, has a currency of its own: what such an
/// account holds is in that currency, which neither a conversion nor a new label would leave right (section 21.1).
pub fn change(conn: &mut Connection, new_currency: &str, rate: Option<&str>, today: &str) -> Result<Conversion, AppError> {
    // shortcut: refuse, do not convert. A foreign account would need a rate of its own to the new currency; ask for one when people ask.
    if conn.query_row("SELECT EXISTS (SELECT 1 FROM accounts WHERE currency IS NOT NULL)", [], |r| r.get(0))? {
        return Err(AppError::InvalidInput);
    }
    let new_currency = normalize_code(new_currency)?;
    let rate_text = rate.map(str::to_string);
    let rate = rate.map(parse_rate).transpose()?;

    let tx = conn.transaction()?;
    let old_currency: String = tx.query_row("SELECT currency FROM settings WHERE id = 1", [], |r| r.get(0))?;
    if old_currency == new_currency {
        return Err(AppError::InvalidInput);
    }

    // A conversion is not an edit: no row gets a new stamp, and the phone converts its own rows the same way when it
    // learns of the change (docs/sync-design.md, section 11).
    sync::stamp::set_applying(&tx, true)?;
    let mut done = Conversion::default();
    if let Some(rate) = rate {
        done.transactions = convert_transactions(&tx, rate)?;
        done.recurring = convert_recurring(&tx, rate)?;
        done.budgets = convert_budgets(&tx, rate)?;
        done.ledger = convert_ledger(&tx, rate)?;
        done.accounts = convert_accounts(&tx, rate)?;
        done.transfers = convert_transfers(&tx, rate)?;
        (done.stashes, done.stash_entries) = convert_stashes(&tx, rate)?;
    }
    // The parts of the shared bills were written while the checks stood aside: they must add up to no more than their bills.
    if !splits::check(&tx)? {
        return Err(AppError::Internal);
    }

    tx.execute("UPDATE settings SET currency = ?1, currency_generation = currency_generation + 1 WHERE id = 1", [&new_currency])?;
    let generation: i64 = tx.query_row("SELECT currency_generation FROM settings WHERE id = 1", [], |r| r.get(0))?;
    let stamp = sync::stamp::tick(&tx)?.format();
    tx.execute(
        "INSERT INTO currency_changes (id, from_currency, to_currency, rate, stamp) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![generation, old_currency, new_currency, rate_text, stamp],
    )?;
    sync::stamp::set_applying(&tx, false)?;
    tx.execute(
        "INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
         VALUES ('currency_change', '1', 'UPDATE', json_object('currency', ?1),
                 json_object('currency', ?2, 'rate', ?3, 'date', ?4, 'converted', ?5))",
        params![old_currency, new_currency, rate_text, today, rate_text.is_some()],
    )?;
    tx.commit()?;
    Ok(done)
}

/// A change of currency made on the desktop, as it reaches another device (docs/sync-design.md, section 11).
pub(crate) struct Recorded<'a> {
    pub generation: i64,
    pub from: &'a str,
    pub to: &'a str,
    pub rate: Option<&'a str>,
    pub stamp: &'a str,
}

/// Applies a change that another device made: the same conversion with the same rate and rounding, the same record, and
/// the generation moves up by one. It must be the next generation, and from the currency this device has now. The caller
/// has turned `applying` on, so no row gets a new stamp. Either everything changes or nothing does.
///
/// It is not refused when this device has a foreign account: the other device has already changed its currency, and refusing would stop the
/// sync for good. What is in a foreign account's own currency is left as it is: its opening balance and virtual balance, every native
/// amount of a transaction, the amount of a transfer that leaves it, and the amount that arrives in it. What is in the app's currency is
/// converted, a transfer's second amount too when it arrives in an account of the app's currency (section 21.6), and so are the target and the
/// entries of a stash in an account of the app's currency (`docs/stash-design.md`, section 8); a stash in a foreign account is in its currency.
pub(crate) fn apply_recorded(tx: &rusqlite::Transaction, change: &Recorded, today: &str) -> Result<Conversion, AppError> {
    let (current, generation): (String, i64) =
        tx.query_row("SELECT currency, currency_generation FROM settings WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if change.generation != generation + 1 || current != change.from || change.from == change.to || normalize_code(change.to)? != change.to {
        return Err(AppError::InvalidInput);
    }
    let rate = change.rate.map(parse_rate).transpose()?;
    let mut done = Conversion::default();
    if let Some(rate) = rate {
        done.transactions = convert_transactions(tx, rate)?;
        done.recurring = convert_recurring(tx, rate)?;
        done.budgets = convert_budgets(tx, rate)?;
        done.ledger = convert_ledger(tx, rate)?;
        done.accounts = convert_accounts(tx, rate)?;
        done.transfers = convert_transfers(tx, rate)?;
        (done.stashes, done.stash_entries) = convert_stashes(tx, rate)?;
    }
    if !splits::check(tx)? {
        return Err(AppError::Internal);
    }
    tx.execute("UPDATE settings SET currency = ?1, currency_generation = ?2 WHERE id = 1", params![change.to, change.generation])?;
    tx.execute(
        "INSERT INTO currency_changes (id, from_currency, to_currency, rate, stamp) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![change.generation, change.from, change.to, change.rate, change.stamp],
    )?;
    tx.execute(
        "INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
         VALUES ('currency_change', '1', 'UPDATE', json_object('currency', ?1),
                 json_object('currency', ?2, 'rate', ?3, 'date', ?4, 'converted', ?5, 'from_sync', 1))",
        params![change.from, change.to, change.rate, today, change.rate.is_some()],
    )?;
    Ok(done)
}

/// An amount that must stay at least one cent, whatever the rate.
fn at_least_one(minor: i64) -> i64 {
    minor.max(1)
}

/// `amount_minor` and `planned_amount_minor` only: `native_amount_minor` is in the currency of a foreign account and is never converted.
fn convert_transactions(tx: &rusqlite::Transaction, rate: Rate) -> Result<u32, AppError> {
    let rows: Vec<(i64, i64, Option<i64>)> = tx
        .prepare("SELECT id, amount_minor, planned_amount_minor FROM transactions")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let mut update = tx.prepare("UPDATE transactions SET amount_minor = ?1, planned_amount_minor = ?2 WHERE id = ?3")?;
    for (id, old, planned) in &rows {
        let amount = at_least_one(convert(*old, rate)?);
        let planned = planned.map(|p| convert(p, rate)).transpose()?;
        update.execute(params![amount, planned, id])?;
        // A shared bill: every part, yours included, is scaled to the new amount with the old parts as the weights, so the parts still add
        // up to the cent and none of them is negative. Rounding each part on its own could make them add up to more than the bill.
        splits::rescale(tx, *id, *old, amount)?;
    }
    Ok(rows.len() as u32)
}

/// Each entry of the ledger, at least one cent. The balance of a friend is a sum of converted pieces, so it can differ from the converted old
/// balance by a few cents; every device ends with the same number.
fn convert_ledger(tx: &rusqlite::Transaction, rate: Rate) -> Result<u32, AppError> {
    let rows: Vec<(i64, i64)> = tx
        .prepare("SELECT id, amount_minor FROM ledger_entries")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut update = tx.prepare("UPDATE ledger_entries SET amount_minor = ?1 WHERE id = ?2")?;
    for (id, amount) in &rows {
        update.execute(params![at_least_one(convert(*amount, rate)?), id])?;
    }
    Ok(rows.len() as u32)
}

/// A signed amount: its size is converted and the sign put back, so it rounds half away from zero, and it may become zero.
fn convert_signed(minor: i64, rate: Rate) -> Result<i64, AppError> {
    Ok(convert(minor.abs(), rate)? * minor.signum())
}

/// The opening balance and the virtual balance of each account that has one, the archived ones too. Both may be negative, so both are
/// converted by `convert_signed`. The balance of an account is a sum of converted pieces, so it can differ from the converted old balance
/// by a few cents, as a friend's does. An account with a currency of its own is left out: both are in that currency.
fn convert_accounts(tx: &rusqlite::Transaction, rate: Rate) -> Result<u32, AppError> {
    let rows: Vec<(i64, i64, i64)> = tx
        .prepare("SELECT id, opening_balance_minor, virtual_balance_minor FROM accounts WHERE (opening_balance_minor <> 0 OR virtual_balance_minor <> 0) AND currency IS NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let mut update = tx.prepare("UPDATE accounts SET opening_balance_minor = ?1, virtual_balance_minor = ?2 WHERE id = ?3")?;
    for (id, opening, virtual_balance) in &rows {
        update.execute(params![convert_signed(*opening, rate)?, convert_signed(*virtual_balance, rate)?, id])?;
    }
    Ok(rows.len() as u32)
}

/// Each transfer between accounts, at least one cent. `amount_minor` leaves the `from` account and `to_amount_minor` (when there is one)
/// arrives in the `to` account, each in the currency of its own account: one that is in an account's own currency is left as it is, so a
/// transfer between two accounts with a currency of their own is not touched. A transfer with nothing to convert is not counted.
fn convert_transfers(tx: &rusqlite::Transaction, rate: Rate) -> Result<u32, AppError> {
    let rows: Vec<(i64, i64, Option<i64>, bool, bool)> = tx
        .prepare(
            "SELECT x.id, x.amount_minor, x.to_amount_minor, f.currency IS NOT NULL, t.currency IS NOT NULL
             FROM transfers x JOIN accounts f ON f.id = x.from_account_id JOIN accounts t ON t.id = x.to_account_id",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<Result<_, _>>()?;
    let mut update = tx.prepare("UPDATE transfers SET amount_minor = ?1, to_amount_minor = ?2 WHERE id = ?3")?;
    let mut converted = 0;
    for (id, amount, arrives, from_foreign, to_foreign) in rows {
        let leaves_in_main = !from_foreign;
        let arrives_in_main = arrives.is_some() && !to_foreign;
        if !leaves_in_main && !arrives_in_main {
            continue;
        }
        let amount = if leaves_in_main { at_least_one(convert(amount, rate)?) } else { amount };
        let arrives = match arrives {
            Some(minor) if arrives_in_main => Some(at_least_one(convert(minor, rate)?)),
            other => other,
        };
        update.execute(params![amount, arrives, id])?;
        converted += 1;
    }
    Ok(converted)
}

/// The target and every entry of each stash that lives in an account of the app's currency. A stash is in its account's currency, so one in an
/// account with a currency of its own is left as it is (a conversion that arrives by sync can meet one: `apply_recorded`). The target is at
/// least one cent, as the check of the column asks. An entry keeps its sign and is at least one cent in size, so an add never becomes a take
/// and an entry never becomes the 0 that the check of the column refuses. What a stash holds is then a sum of converted pieces and may be a cent
/// above its target: it is reached all the same, so nothing is repaired (section 8). Counts the stashes that had a target or an entry converted,
/// and the entries.
fn convert_stashes(tx: &rusqlite::Transaction, rate: Rate) -> Result<(u32, u32), AppError> {
    let stashes: Vec<(i64, Option<i64>)> = tx
        .prepare("SELECT s.id, s.target_minor FROM stashes s JOIN accounts a ON a.id = s.account_id WHERE a.currency IS NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut entries = tx.prepare("SELECT id, amount_minor FROM stash_entries WHERE stash_id = ?1")?;
    let mut update_target = tx.prepare("UPDATE stashes SET target_minor = ?1 WHERE id = ?2")?;
    let mut update_entry = tx.prepare("UPDATE stash_entries SET amount_minor = ?1 WHERE id = ?2")?;
    let (mut converted_stashes, mut converted_entries) = (0, 0);
    for (id, target) in stashes {
        let rows: Vec<(i64, i64)> = entries.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        if target.is_none() && rows.is_empty() {
            continue;
        }
        if let Some(minor) = target {
            update_target.execute(params![at_least_one(convert(minor, rate)?), id])?;
        }
        for (entry, minor) in &rows {
            let size = convert_signed(*minor, rate)?.abs().max(1);
            update_entry.execute(params![size * minor.signum(), entry])?;
        }
        converted_stashes += 1;
        converted_entries += rows.len() as u32;
    }
    Ok((converted_stashes, converted_entries))
}

fn convert_recurring(tx: &rusqlite::Transaction, rate: Rate) -> Result<u32, AppError> {
    let rows: Vec<(i64, i64)> = tx
        .prepare("SELECT id, amount_minor FROM recurring_transactions WHERE amount_minor IS NOT NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut update = tx.prepare("UPDATE recurring_transactions SET amount_minor = ?1 WHERE id = ?2")?;
    for (id, amount) in &rows {
        update.execute(params![at_least_one(convert(*amount, rate)?), id])?;
    }
    Ok(rows.len() as u32)
}

/// Budget limits may become zero. Removed budgets (NULL) stay removed.
fn convert_budgets(tx: &rusqlite::Transaction, rate: Rate) -> Result<u32, AppError> {
    let rows: Vec<(i64, i64)> = tx
        .prepare("SELECT id, amount_limit_minor FROM budgets WHERE amount_limit_minor IS NOT NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut update = tx.prepare("UPDATE budgets SET amount_limit_minor = ?1 WHERE id = ?2")?;
    for (id, limit) in &rows {
        update.execute(params![convert(*limit, rate)?, id])?;
    }
    Ok(rows.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::fixtures::{archived_account, balance, new_account, new_transfer, snapshot, transfer, Rng, TODAY};
    use crate::accounts::transfers::NewTransfer;
    use crate::accounts::{self, AccountPatch, NewAccount, MAIN_ID};
    use crate::budgets::{self, Scope, SetBudget};
    use crate::categories::{self, CategoryType};
    use crate::transactions::{self, NewTransaction};

    fn db() -> (Connection, i64) {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn.execute("INSERT INTO settings (id, currency, timezone) VALUES (1, 'EUR', 'UTC')", []).unwrap();
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        (conn, food)
    }

    fn spend(conn: &Connection, category: i64, amount: &str) -> i64 {
        transactions::create(
            conn,
            &NewTransaction {
                name: "x".into(),
                amount: amount.into(),
                payment_date: "2026-06-15".into(),
                category_id: category.to_string(),
                description: None,
                client_uuid: None,
                account_id: None,
                native_amount: None,
            },
        )
        .unwrap()
        .id
    }

    fn amount_of(conn: &Connection, id: i64) -> i64 {
        conn.query_row("SELECT amount_minor FROM transactions WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    fn currency(conn: &Connection) -> String {
        conn.query_row("SELECT currency FROM settings", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn rates_parse_exactly_and_bad_ones_are_refused() {
        assert_eq!(parse_rate("1.08"), Ok(Rate { numerator: 108, denominator: 100 }));
        assert_eq!(parse_rate("160"), Ok(Rate { numerator: 160, denominator: 1 }));
        assert_eq!(parse_rate("0.0063"), Ok(Rate { numerator: 63, denominator: 10_000 }));
        assert_eq!(parse_rate("0.00000001"), Ok(Rate { numerator: 1, denominator: 100_000_000 }));
        for bad in ["", "0", "0.0", "-1", "+1", "1,5", "1.", ".5", "01.5", "1.123456789", "1e3", "abc", " 1.2", "1.2 ", "1234567890"] {
            assert_eq!(parse_rate(bad), Err(AppError::InvalidInput), "{bad:?}");
        }
    }

    #[test]
    fn conversion_rounds_half_up_to_the_cent() {
        let rate = |t: &str| parse_rate(t).unwrap();
        assert_eq!(convert(10_000, rate("1.08")), Ok(10_800));
        assert_eq!(convert(5, rate("0.5")), Ok(3), "2.5 cents rounds up");
        assert_eq!(convert(4, rate("0.5")), Ok(2));
        assert_eq!(convert(1, rate("0.4")), Ok(0));
        assert_eq!(convert(10_000_000, rate("0.0063")), Ok(63_000), "100000.00 at 0.0063");
        assert_eq!(convert(1, rate("1")), Ok(1));
        assert_eq!(convert(MAX_MINOR, rate("2")), Err(AppError::InvalidInput), "over the largest storable amount");
    }

    #[test]
    fn a_bill_with_no_fixed_amount_stays_without_one_when_the_currency_changes() {
        let (mut conn, food) = db();
        conn.execute(
            "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date)
             VALUES (?1, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-02-01'), (?1, 'Electricity', NULL, '2026-01-01', 'MONTH', '2026-02-01')",
            [food],
        )
        .unwrap();
        let done = change(&mut conn, "USD", Some("2"), "2026-10-01").unwrap();
        assert_eq!(done.recurring, 1, "only the item with an amount was converted");
        let amounts: Vec<Option<i64>> = conn
            .prepare("SELECT amount_minor FROM recurring_transactions ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(amounts, [Some(180_000), None]);
    }

    #[test]
    fn rows_with_no_value_stay_empty_and_deleted_categories_are_converted_too() {
        let (mut conn, food) = db();
        let id = spend(&conn, food, "10.00");
        categories::delete(&mut conn, food, "2026-06-20T10:00:00Z".parse().unwrap()).unwrap();
        let null_budgets: i64 = conn.query_row("SELECT count(*) FROM budgets WHERE amount_limit_minor IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(null_budgets, 1);

        let done = change(&mut conn, "USD", Some("2"), "2026-10-13").unwrap();

        assert_eq!(done.budgets, 0, "a removed budget has nothing to convert");
        assert_eq!(amount_of(&conn, id), 2_000);
        let planned: Option<i64> = conn.query_row("SELECT planned_amount_minor FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(planned, None);
        let still_null: i64 = conn.query_row("SELECT count(*) FROM budgets WHERE amount_limit_minor IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(still_null, 1);
    }

    #[test]
    fn tiny_amounts_stay_valid() {
        let (mut conn, food) = db();
        let id = spend(&conn, food, "0.01");
        budgets::set(
            &conn,
            &SetBudget { kind: CategoryType::Expense, scope: Scope::Global, month: "2026-06".into(), amount_limit: Some("0.01".into()), category_id: None },
            "2026-01-15T00:00:00Z".parse().unwrap(),
        )
        .unwrap();
        change(&mut conn, "USD", Some("0.1"), "2026-10-13").unwrap();
        assert_eq!(amount_of(&conn, id), 1, "a transaction never rounds down to zero");
        let limit: i64 = conn.query_row("SELECT amount_limit_minor FROM budgets", [], |r| r.get(0)).unwrap();
        assert_eq!(limit, 0, "a budget limit may");
    }

    #[test]
    fn a_failed_conversion_changes_nothing() {
        let (mut conn, food) = db();
        let small = spend(&conn, food, "10.00");
        let big = spend(&conn, food, "9999999999.99");
        let before = (amount_of(&conn, small), amount_of(&conn, big));
        assert_eq!(change(&mut conn, "USD", Some("2"), "2026-10-13"), Err(AppError::InvalidInput));
        assert_eq!((amount_of(&conn, small), amount_of(&conn, big)), before, "even the small one is untouched");
        assert_eq!(currency(&conn), "EUR");
    }

    #[test]
    fn invalid_requests_are_refused_without_effect() {
        let (mut conn, food) = db();
        let id = spend(&conn, food, "10.00");
        for (code, rate) in [("EUR", Some("1.1")), ("EUR", None), ("US", Some("1.1")), ("US1", None), ("", None), ("USD", Some("0")), ("USD", Some("-2")), ("USD", Some(""))] {
            assert_eq!(change(&mut conn, code, rate, "2026-10-13").err(), Some(AppError::InvalidInput), "{code:?} {rate:?}");
        }
        assert_eq!(currency(&conn), "EUR");
        assert_eq!(amount_of(&conn, id), 1_000);
    }

    #[test]
    fn a_rounded_conversion_keeps_all_constraints_satisfied() {
        let (mut conn, food) = db();
        for amount in ["0.01", "0.99", "1.00", "123456.78"] {
            spend(&conn, food, amount);
        }
        change(&mut conn, "JPY", Some("160.5"), "2026-10-13").unwrap();
        let min: i64 = conn.query_row("SELECT min(amount_minor) FROM transactions", [], |r| r.get(0)).unwrap();
        assert!(min >= 1);
    }

    // ---- accounts and transfers (docs/accounts-design.md, section 8 and section 5.5, item 15) ----

    /// An account that opened with `opening` ("-120.00") at the start of 2026. Returns its id.
    fn account(conn: &Connection, name: &str, opening: &str) -> i64 {
        accounts::create(conn, &NewAccount { opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), ..new_account(name) }, TODAY).unwrap().id
    }

    fn opening_of(conn: &Connection, id: i64) -> i64 {
        conn.query_row("SELECT opening_balance_minor FROM accounts WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    fn moved_by(conn: &Connection, id: i64) -> i64 {
        conn.query_row("SELECT amount_minor FROM transfers WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    /// The opening balance of every account and the amount of every transfer, in the order they were made.
    fn account_numbers(conn: &Connection) -> Vec<i64> {
        conn.prepare("SELECT n FROM (SELECT 0 AS t, id, opening_balance_minor AS n FROM accounts UNION ALL SELECT 1, id, amount_minor FROM transfers) ORDER BY t, id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn opening_balances_and_transfers_are_converted_with_the_rounding_of_the_others_and_counted() {
        let (mut conn, _) = db();
        let savings = account(&conn, "Savings", "100.00");
        let card = account(&conn, "Card", "-120.05");
        let odd = account(&conn, "Odd", "0.05");
        let odd_debt = account(&conn, "Odd debt", "-0.05");
        let empty = account(&conn, "Empty", "0.00");
        let big = transfer(&conn, MAIN_ID, savings, "200.00", "2026-02-01");
        let small = transfer(&conn, savings, card, "0.05", "2026-02-02");

        let done = change(&mut conn, "USD", Some("0.5"), "2026-10-13").unwrap();

        assert_eq!((done.accounts, done.transfers), (4, 2), "the two accounts with nothing to convert are not counted");
        assert_eq!(opening_of(&conn, savings), 5_000);
        assert_eq!(opening_of(&conn, card), -6_003, "-60.025 rounds away from zero, as 60.025 rounds up");
        assert_eq!(opening_of(&conn, odd), 3, "2.5 cents rounds up");
        assert_eq!(opening_of(&conn, odd_debt), -3, "and -2.5 cents rounds to -3: the size is rounded, the sign is put back");
        assert_eq!((opening_of(&conn, empty), opening_of(&conn, MAIN_ID)), (0, 0));
        assert_eq!((moved_by(&conn, big), moved_by(&conn, small)), (10_000, 3));
        // a balance is the sum of the converted pieces
        assert_eq!(balance(&conn, savings, TODAY), 5_000 + 10_000 - 3);
        assert_eq!(balance(&conn, card, TODAY), -6_003 + 3);
        assert_eq!(accounts::summary(&conn, TODAY).unwrap().in_accounts, "-10.03", "the transfers still add up to nothing");
    }

    #[test]
    fn an_opening_balance_may_round_to_nothing_and_a_transfer_never_does() {
        let (mut conn, _) = db();
        let cent = account(&conn, "Cent", "0.01");
        let cent_debt = account(&conn, "Cent debt", "-0.01");
        let moved = transfer(&conn, cent, cent_debt, "0.01", "2026-02-01");
        let done = change(&mut conn, "USD", Some("0.001"), "2026-10-13").unwrap();
        assert_eq!((done.accounts, done.transfers), (2, 1));
        assert_eq!((opening_of(&conn, cent), opening_of(&conn, cent_debt)), (0, 0), "an opening balance may be zero, and -0 is 0");
        assert_eq!(moved_by(&conn, moved), 1, "a transfer of one cent stays one cent");
        // an account whose opening balance became zero is not counted the next time
        let again = change(&mut conn, "EUR", Some("1000"), "2026-10-14").unwrap();
        assert_eq!((again.accounts, again.transfers), (0, 1));
        assert_eq!(moved_by(&conn, moved), 1_000);
    }

    #[test]
    fn an_archived_account_is_converted_like_the_others() {
        let (mut conn, _) = db();
        let old = account(&conn, "Old", "-80.00");
        let moved = transfer(&conn, MAIN_ID, old, "30.00", "2026-02-01");
        accounts::archive(&conn, old, TODAY).unwrap();
        let empty = archived_account(&conn, "Old and empty");
        let done = change(&mut conn, "USD", Some("1.08"), "2026-10-13").unwrap();
        assert_eq!((done.accounts, done.transfers), (1, 1));
        assert_eq!((opening_of(&conn, old), moved_by(&conn, moved), opening_of(&conn, empty)), (-8_640, 3_240, 0));
        let account = accounts::get(&conn, old, TODAY).unwrap();
        assert!(account.archived, "and it is still archived");
        assert_eq!(account.balance, "-54.00");
    }

    #[test]
    fn opening_balances_at_the_bounds_are_converted_and_one_that_leaves_the_range_refuses_the_whole_change() {
        for (opening, sign) in [("9999999999.99", 1), ("-9999999999.99", -1)] {
            let (mut conn, food) = db();
            let small = spend(&conn, food, "10.00");
            let edge = account(&conn, "Edge", opening);
            let other = account(&conn, "Other", "5.00");
            let moved = transfer(&conn, MAIN_ID, other, "7.00", "2026-02-01");
            let before = snapshot(&conn);
            for rate in ["1.00000001", "2", "999999999"] {
                assert_eq!(change(&mut conn, "USD", Some(rate), "2026-10-13"), Err(AppError::InvalidInput), "{opening} at {rate}");
                assert_eq!(snapshot(&conn), before, "{opening} at {rate}: no row, stamp or history line changed");
                assert_eq!(currency(&conn), "EUR");
            }
            assert_eq!((amount_of(&conn, small), opening_of(&conn, other), moved_by(&conn, moved)), (1_000, 500, 700), "even the small ones are untouched");

            // at a rate that keeps it in the range it is converted, the bound itself included
            change(&mut conn, "USD", Some("1"), "2026-10-13").unwrap();
            assert_eq!(opening_of(&conn, edge), sign * MAX_MINOR, "{opening} at 1");
            change(&mut conn, "GBP", Some("0.5"), "2026-10-14").unwrap();
            assert_eq!(opening_of(&conn, edge), sign * 500_000_000_000, "{opening} at 0.5: half a cent rounds away from zero");
            change(&mut conn, "CHF", Some("1.99999999"), "2026-10-15").unwrap();
            assert_eq!(opening_of(&conn, edge), sign * 999_999_995_000, "{opening}: just inside the range");
        }
    }

    #[test]
    fn a_transfer_that_leaves_the_range_refuses_the_whole_change() {
        let (mut conn, _) = db();
        let savings = account(&conn, "Savings", "12.00");
        let big = transfer(&conn, MAIN_ID, savings, "9999999999.99", "2026-02-01");
        let before = snapshot(&conn);
        assert_eq!(change(&mut conn, "USD", Some("1.5"), "2026-10-13"), Err(AppError::InvalidInput));
        assert_eq!(snapshot(&conn), before);
        assert_eq!((opening_of(&conn, savings), moved_by(&conn, big), currency(&conn).as_str()), (1_200, MAX_MINOR, "EUR"));
    }

    #[test]
    fn converting_there_and_back_gives_the_same_numbers_when_nothing_was_rounded() {
        let (mut conn, _) = db();
        let savings = account(&conn, "Savings", "1234.56");
        let card = account(&conn, "Card", "-0.07");
        transfer(&conn, MAIN_ID, savings, "250.00", "2026-02-01");
        transfer(&conn, savings, card, "0.01", "2026-02-02");
        let before = account_numbers(&conn);
        let stamps = snapshot(&conn);
        change(&mut conn, "USD", Some("4"), "2026-10-13").unwrap();
        assert_eq!(account_numbers(&conn), before.iter().map(|n| n * 4).collect::<Vec<_>>());
        change(&mut conn, "EUR", Some("0.25"), "2026-10-14").unwrap();
        assert_eq!(account_numbers(&conn), before);
        let generation: i64 = conn.query_row("SELECT currency_generation FROM settings", [], |r| r.get(0)).unwrap();
        assert_eq!(generation, 2, "two changes were recorded");
        // a conversion is not an edit: no stamp and no `updated_at` moved, on either table
        let marks = |text: &str| -> Vec<String> { text.lines().take_while(|l| *l != "== transactions").map(str::to_string).collect() };
        assert_eq!(marks(&snapshot(&conn)), marks(&stamps));
    }

    #[test]
    fn converting_twice_rounds_twice_and_what_was_rounded_away_does_not_come_back() {
        let (mut conn, _) = db();
        let savings = account(&conn, "Savings", "0.01");
        let card = account(&conn, "Card", "-33.33");
        let moved = transfer(&conn, MAIN_ID, savings, "0.01", "2026-02-01");
        change(&mut conn, "USD", Some("0.4"), "2026-10-13").unwrap();
        assert_eq!((opening_of(&conn, savings), opening_of(&conn, card), moved_by(&conn, moved)), (0, -1_333, 1));
        change(&mut conn, "EUR", Some("2.5"), "2026-10-14").unwrap();
        assert_eq!(opening_of(&conn, savings), 0, "the cent that became nothing stays nothing");
        assert_eq!(opening_of(&conn, card), -3_333, "-13.33 at 2.5 is -33.325, which rounds away from zero");
        assert_eq!(moved_by(&conn, moved), 3, "the transfer was kept at one cent, and that cent is converted");
    }

    #[test]
    fn a_change_of_the_label_alone_converts_no_account_and_no_transfer() {
        let (mut conn, _) = db();
        let savings = account(&conn, "Savings", "-5.00");
        transfer(&conn, MAIN_ID, savings, "2.00", "2026-02-01");
        let before = account_numbers(&conn);
        let done = change(&mut conn, "USD", None, "2026-10-13").unwrap();
        assert_eq!((done.accounts, done.transfers), (0, 0));
        assert_eq!(account_numbers(&conn), before);
    }

    /// The same accounts and transfers on any device: an archived account, a negative opening balance, one of nothing and a transfer of a cent.
    fn with_accounts() -> Connection {
        let (conn, _) = db();
        let savings = account(&conn, "Savings", "1500.37");
        let card = account(&conn, "Card", "-249.99");
        let old = account(&conn, "Old", "33.33");
        let empty = account(&conn, "Empty", "0.00");
        transfer(&conn, MAIN_ID, savings, "300.00", "2026-02-01");
        transfer(&conn, savings, card, "0.01", "2026-02-02");
        transfer(&conn, old, empty, "33.33", "2026-02-03");
        accounts::archive(&conn, old, TODAY).unwrap();
        conn
    }

    fn apply_change_of(desktop: &Connection, phone: &mut Connection) -> Result<Conversion, AppError> {
        let (generation, from, to, rate, stamp): (i64, String, String, Option<String>, String) = desktop
            .query_row("SELECT id, from_currency, to_currency, rate, stamp FROM currency_changes ORDER BY id DESC LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap();
        let tx = phone.transaction().unwrap();
        sync::stamp::set_applying(&tx, true).unwrap();
        let done = apply_recorded(&tx, &Recorded { generation, from: &from, to: &to, rate: rate.as_deref(), stamp: &stamp }, "2026-10-13")?;
        sync::stamp::set_applying(&tx, false).unwrap();
        tx.commit().unwrap();
        Ok(done)
    }

    #[test]
    fn the_same_conversion_arriving_by_sync_leaves_both_devices_with_the_same_accounts_and_transfers() {
        for rate in ["1.08", "0.0063", "160", "0.5", "2.5", "1.00000001", "0.001"] {
            let (mut desktop, mut phone) = (with_accounts(), with_accounts());
            let stamps = |conn: &Connection| -> Vec<String> {
                conn.prepare("SELECT stamp || updated_at FROM accounts UNION ALL SELECT stamp || updated_at FROM transfers").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
            };
            let before = stamps(&phone);
            let made = change(&mut desktop, "USD", Some(rate), "2026-10-13").unwrap();
            let applied = apply_change_of(&desktop, &mut phone).unwrap();
            assert_eq!((applied.accounts, applied.transfers), (made.accounts, made.transfers), "rate {rate}");
            assert_eq!((made.accounts, made.transfers), (3, 3), "rate {rate}: the archived account too");
            assert_eq!(account_numbers(&phone), account_numbers(&desktop), "rate {rate}");
            assert_eq!(accounts::summary(&phone, TODAY).unwrap(), accounts::summary(&desktop, TODAY).unwrap(), "rate {rate}");
            assert_eq!(stamps(&phone), before, "rate {rate}: no stamp moved on the phone");
            assert!(accounts::check(&phone).unwrap());
        }
    }

    #[test]
    fn a_conversion_arriving_by_sync_that_leaves_the_range_is_refused_and_the_phone_keeps_its_numbers() {
        let (mut desktop, mut phone) = (with_accounts(), with_accounts());
        // only the phone has the account that cannot be converted
        account(&phone, "Edge", "-9999999999.99");
        let before = snapshot(&phone);
        change(&mut desktop, "USD", Some("3"), "2026-10-13").unwrap();
        assert_eq!(apply_change_of(&desktop, &mut phone).err(), Some(AppError::InvalidInput));
        assert_eq!(snapshot(&phone), before, "the transaction was dropped: nothing changed");
        assert_eq!(currency(&phone), "EUR");
    }

    #[test]
    fn random_accounts_and_rates_keep_the_sign_the_range_and_at_least_a_cent_in_every_transfer() {
        let mut random = Rng(0xACC0_0575);
        for round in 0..40 {
            let (mut conn, _) = db();
            let rate_text = ["0.0001", "0.37", "1.08", "3.333", "160", "999.99"][round % 6];
            let rate = parse_rate(rate_text).unwrap();
            let mut ids = vec![MAIN_ID];
            for i in 0..5 {
                let minor = random.between(0, 400_000) as i64 - 200_000;
                ids.push(account(&conn, &format!("A{i}"), &crate::money::format_amount(minor)));
            }
            for _ in 0..8 {
                let (from, to) = (random.between(0, 5) as usize, random.between(0, 5) as usize);
                if from != to {
                    transfer(&conn, ids[from], ids[to], &crate::money::format_amount(random.between(1, 90_000) as i64), "2026-02-01");
                }
            }
            let before = account_numbers(&conn);
            let transfers_at = before.len() - conn.query_row::<i64, _, _>("SELECT count(*) FROM transfers", [], |r| r.get(0)).unwrap() as usize;
            change(&mut conn, "USD", Some(rate_text), "2026-10-13").unwrap_or_else(|e| panic!("rate {rate_text}: {e}"));
            for (at, (old, new)) in before.iter().zip(account_numbers(&conn)).enumerate() {
                if at < transfers_at {
                    assert_eq!(new, convert(old.abs(), rate).unwrap() * old.signum(), "opening {old} at {rate_text}");
                    assert!(new == 0 || new.signum() == old.signum(), "{old} became {new} at {rate_text}");
                } else {
                    assert_eq!(new, convert(*old, rate).unwrap().max(1), "transfer {old} at {rate_text}");
                }
                assert!(new.abs() <= MAX_MINOR);
            }
            // what left one account reached another, so the transfers still add up to nothing
            let (held, opened): (i64, i64) = conn
                .query_row("SELECT (SELECT COALESCE(SUM(delta_minor), 0) FROM account_movements), (SELECT SUM(opening_balance_minor) FROM accounts)", [], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap();
            assert_eq!(held, opened, "round {round} at {rate_text}");
        }
    }

    // ---- the virtual balance of an account (docs/accounts-design.md, section 20.3) ----

    /// An account that opened with `opening` at the start of 2026 and has a virtual balance ("1000.00": the limit of a card).
    fn account_with_virtual(conn: &Connection, name: &str, opening: &str, virtual_balance: &str) -> i64 {
        accounts::create(
            conn,
            &NewAccount { opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), virtual_balance: Some(virtual_balance.into()), ..new_account(name) },
            TODAY,
        )
        .unwrap()
        .id
    }

    fn virtual_of(conn: &Connection, id: i64) -> i64 {
        conn.query_row("SELECT virtual_balance_minor FROM accounts WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    /// The opening balance and the virtual balance of every account, in the order they were made.
    fn both_balances(conn: &Connection) -> Vec<(i64, i64)> {
        conn.prepare("SELECT opening_balance_minor, virtual_balance_minor FROM accounts ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    /// The stamp and `updated_at` of every account, as one text each.
    fn account_stamps(conn: &Connection) -> Vec<String> {
        conn.prepare("SELECT stamp || updated_at FROM accounts ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn a_virtual_balance_is_converted_with_the_sign_and_the_rounding_of_the_opening_balance_and_enters_no_balance() {
        let (mut conn, _) = db();
        let card = account_with_virtual(&conn, "Card", "-240.00", "1000.00");
        let owed = account_with_virtual(&conn, "Owed", "0.00", "-250.00");
        let odd = account_with_virtual(&conn, "Odd", "0.00", "0.05");
        let odd_debt = account_with_virtual(&conn, "Odd debt", "0.00", "-0.05");
        let plain = account(&conn, "Plain", "0.00");
        let held: Vec<i64> = [card, owed, odd, odd_debt, plain, MAIN_ID].iter().map(|id| balance(&conn, *id, TODAY)).collect();
        assert_eq!(held, [-24_000, 0, 0, 0, 0, 0]);

        let done = change(&mut conn, "USD", Some("0.5"), "2026-10-13").unwrap();

        assert_eq!((done.accounts, done.transfers), (4, 0), "the accounts with nothing to convert are not counted");
        assert_eq!((opening_of(&conn, card), virtual_of(&conn, card)), (-12_000, 50_000), "a limit and a debt are converted as they are");
        assert_eq!((opening_of(&conn, owed), virtual_of(&conn, owed)), (0, -12_500), "a negative virtual balance keeps its sign");
        assert_eq!(virtual_of(&conn, odd), 3, "2.5 cents rounds up");
        assert_eq!(virtual_of(&conn, odd_debt), -3, "and -2.5 cents rounds to -3: the size is rounded, the sign is put back");
        assert_eq!((virtual_of(&conn, plain), virtual_of(&conn, MAIN_ID)), (0, 0), "zero stays zero");
        // the balance is made of the opening balance alone, and `available` is the balance and the converted limit
        assert_eq!(balance(&conn, card, TODAY), -12_000);
        let shown = accounts::get(&conn, card, TODAY).unwrap();
        assert_eq!((shown.balance.as_str(), shown.virtual_balance.as_str(), shown.available.as_str()), ("-120.00", "500.00", "380.00"));
        assert_eq!(accounts::summary(&conn, TODAY).unwrap().in_accounts, "-120.00", "net worth has what the accounts hold and none of their virtual balances");
        assert_eq!(accounts::get(&conn, owed, TODAY).unwrap().available, "-125.00");
    }

    #[test]
    fn an_account_is_counted_once_whether_it_has_an_opening_balance_a_virtual_balance_or_both() {
        let (mut conn, _) = db();
        account(&conn, "Opening only", "10.00");
        account_with_virtual(&conn, "Virtual only", "0.00", "10.00");
        account_with_virtual(&conn, "Both", "10.00", "10.00");
        account_with_virtual(&conn, "Neither", "0.00", "0.00");
        assert_eq!(both_balances(&conn), [(0, 0), (1_000, 0), (0, 1_000), (1_000, 1_000), (0, 0)]);

        let done = change(&mut conn, "USD", Some("2"), "2026-10-13").unwrap();
        assert_eq!(done.accounts, 3, "an account with an opening balance of 0 and a virtual balance is counted, and one with both is counted once");
        assert_eq!(both_balances(&conn), [(0, 0), (2_000, 0), (0, 2_000), (2_000, 2_000), (0, 0)]);

        // the main account is an account like the others
        accounts::update(&conn, MAIN_ID, &accounts::AccountPatch { virtual_balance: Some("5.00".into()), ..Default::default() }, TODAY).unwrap();
        let done = change(&mut conn, "EUR", Some("0.5"), "2026-10-14").unwrap();
        assert_eq!(done.accounts, 4);
        assert_eq!(both_balances(&conn), [(0, 250), (1_000, 0), (0, 1_000), (1_000, 1_000), (0, 0)]);
    }

    #[test]
    fn a_virtual_balance_may_round_to_nothing_and_is_not_counted_afterwards() {
        let (mut conn, _) = db();
        let up = account_with_virtual(&conn, "Up", "0.00", "0.01");
        let down = account_with_virtual(&conn, "Down", "0.00", "-0.01");
        let done = change(&mut conn, "USD", Some("0.001"), "2026-10-13").unwrap();
        assert_eq!(done.accounts, 2);
        assert_eq!((virtual_of(&conn, up), virtual_of(&conn, down)), (0, 0), "a virtual balance may be zero, and -0 is 0");
        let shown = accounts::get(&conn, down, TODAY).unwrap();
        assert_eq!((shown.virtual_balance.as_str(), shown.available.as_str(), shown.balance.as_str()), ("0.00", "0.00", "0.00"));
        let again = change(&mut conn, "EUR", Some("1000"), "2026-10-14").unwrap();
        assert_eq!(again.accounts, 0, "what was rounded away does not come back");
        assert_eq!((virtual_of(&conn, up), virtual_of(&conn, down)), (0, 0));
    }

    #[test]
    fn virtual_balances_at_the_bounds_are_converted_and_one_that_leaves_the_range_refuses_the_whole_change() {
        for (limit, sign) in [("9999999999.99", 1), ("-9999999999.99", -1)] {
            let (mut conn, food) = db();
            let small = spend(&conn, food, "10.00");
            let edge = account_with_virtual(&conn, "Edge", "0.00", limit);
            let other = account_with_virtual(&conn, "Other", "5.00", "7.00");
            let moved = transfer(&conn, MAIN_ID, other, "7.00", "2026-02-01");
            let before = snapshot(&conn);
            for rate in ["1.00000001", "2", "999999999"] {
                assert_eq!(change(&mut conn, "USD", Some(rate), "2026-10-13"), Err(AppError::InvalidInput), "{limit} at {rate}");
                assert_eq!(snapshot(&conn), before, "{limit} at {rate}: no row, stamp or history line changed");
                assert_eq!(currency(&conn), "EUR");
            }
            assert_eq!((amount_of(&conn, small), opening_of(&conn, other), virtual_of(&conn, other), moved_by(&conn, moved)), (1_000, 500, 700, 700), "even the small ones are untouched");
            let generation: i64 = conn.query_row("SELECT currency_generation FROM settings", [], |r| r.get(0)).unwrap();
            assert_eq!(generation, 0, "and nothing was recorded");

            // at a rate that keeps it in the range it is converted, the bound itself included
            change(&mut conn, "USD", Some("1"), "2026-10-13").unwrap();
            assert_eq!(virtual_of(&conn, edge), sign * MAX_MINOR, "{limit} at 1");
            change(&mut conn, "GBP", Some("0.5"), "2026-10-14").unwrap();
            assert_eq!(virtual_of(&conn, edge), sign * 500_000_000_000, "{limit} at 0.5: half a cent rounds away from zero");
            change(&mut conn, "CHF", Some("1.99999999"), "2026-10-15").unwrap();
            assert_eq!(virtual_of(&conn, edge), sign * 999_999_995_000, "{limit}: just inside the range");
        }
    }

    #[test]
    fn an_opening_balance_that_leaves_the_range_refuses_the_change_even_when_every_virtual_balance_fits() {
        let (mut conn, _) = db();
        let fits = account_with_virtual(&conn, "Fits", "0.00", "10.00");
        account_with_virtual(&conn, "Edge", "9999999999.99", "1.00");
        let before = snapshot(&conn);
        assert_eq!(change(&mut conn, "USD", Some("2"), "2026-10-13"), Err(AppError::InvalidInput));
        assert_eq!(snapshot(&conn), before);
        assert_eq!((virtual_of(&conn, fits), currency(&conn).as_str()), (1_000, "EUR"));
    }

    #[test]
    fn an_archived_account_converts_its_virtual_balance_like_the_others() {
        let (mut conn, _) = db();
        let old = account_with_virtual(&conn, "Old", "0.00", "-80.00");
        accounts::archive(&conn, old, TODAY).unwrap();
        let done = change(&mut conn, "USD", Some("1.08"), "2026-10-13").unwrap();
        assert_eq!(done.accounts, 1);
        assert_eq!(virtual_of(&conn, old), -8_640);
        let shown = accounts::get(&conn, old, TODAY).unwrap();
        assert!(shown.archived, "and it is still archived");
        assert_eq!(shown.available, "-86.40");
    }

    #[test]
    fn converting_virtual_balances_there_and_back_moves_no_stamp_and_a_label_change_alone_moves_nothing() {
        let (mut conn, _) = db();
        account_with_virtual(&conn, "Card", "-240.00", "1000.00");
        account_with_virtual(&conn, "Owed", "0.00", "-0.08");
        let before = both_balances(&conn);
        let marked = account_stamps(&conn);

        let label = change(&mut conn, "GBP", None, "2026-10-12").unwrap();
        assert_eq!(label.accounts, 0);
        assert_eq!(both_balances(&conn), before, "a change of the label alone converts nothing");

        change(&mut conn, "USD", Some("4"), "2026-10-13").unwrap();
        assert_eq!(both_balances(&conn), before.iter().map(|(a, b)| (a * 4, b * 4)).collect::<Vec<_>>());
        change(&mut conn, "EUR", Some("0.25"), "2026-10-14").unwrap();
        assert_eq!(both_balances(&conn), before);
        assert_eq!(account_stamps(&conn), marked, "a conversion is not an edit: no stamp and no `updated_at` moved");
    }

    /// Accounts with virtual balances: a card with a limit, an archived account with a debt, one with only a cent, one with an opening balance and none.
    fn with_limits() -> Connection {
        let (conn, _) = db();
        let card = account_with_virtual(&conn, "Card", "-249.99", "1000.00");
        let old = account_with_virtual(&conn, "Old", "0.00", "-12.34");
        account_with_virtual(&conn, "Cent", "0.00", "0.01");
        let savings = account(&conn, "Savings", "1500.37");
        transfer(&conn, savings, card, "100.00", "2026-02-01");
        accounts::archive(&conn, old, TODAY).unwrap();
        conn
    }

    #[test]
    fn the_same_conversion_arriving_by_sync_leaves_both_devices_with_the_same_virtual_balances() {
        for rate in ["1.08", "0.0063", "160", "0.5", "2.5", "1.00000001", "0.001"] {
            let (mut desktop, mut phone) = (with_limits(), with_limits());
            let before = account_stamps(&phone);
            let made = change(&mut desktop, "USD", Some(rate), "2026-10-13").unwrap();
            let applied = apply_change_of(&desktop, &mut phone).unwrap();
            assert_eq!(applied, made, "rate {rate}");
            assert_eq!((made.accounts, made.transfers), (4, 1), "rate {rate}: the card, the archived account, the cent and the savings");
            assert_eq!(both_balances(&phone), both_balances(&desktop), "rate {rate}");
            assert_eq!(accounts::list(&phone, TODAY).unwrap(), accounts::list(&desktop, TODAY).unwrap(), "rate {rate}: `available` is the same too");
            assert_eq!(accounts::summary(&phone, TODAY).unwrap(), accounts::summary(&desktop, TODAY).unwrap(), "rate {rate}");
            assert_eq!(account_stamps(&phone), before, "rate {rate}: no stamp moved on the phone");
            assert!(accounts::check(&phone).unwrap());
        }
    }

    #[test]
    fn a_conversion_arriving_by_sync_that_leaves_the_range_through_a_virtual_balance_is_refused_and_the_phone_keeps_its_numbers() {
        let (mut desktop, mut phone) = (with_limits(), with_limits());
        // only the phone has the account that cannot be converted
        account_with_virtual(&phone, "Edge", "0.00", "-9999999999.99");
        let before = snapshot(&phone);
        change(&mut desktop, "USD", Some("3"), "2026-10-13").unwrap();
        assert_eq!(apply_change_of(&desktop, &mut phone).err(), Some(AppError::InvalidInput));
        assert_eq!(snapshot(&phone), before, "the transaction was dropped: nothing changed");
        assert_eq!(currency(&phone), "EUR");
        let generation: i64 = phone.query_row("SELECT currency_generation FROM settings", [], |r| r.get(0)).unwrap();
        assert_eq!(generation, 0, "and the change was not recorded");
    }

    #[test]
    fn random_virtual_balances_and_rates_keep_the_sign_and_the_range() {
        let mut random = Rng(0x71A7_0001);
        for round in 0..40 {
            let (mut conn, _) = db();
            let rate_text = ["0.0001", "0.37", "1.08", "3.333", "160", "999.99"][round % 6];
            let rate = parse_rate(rate_text).unwrap();
            for i in 0..6 {
                let minor = |random: &mut Rng| random.between(0, 400_000) as i64 - 200_000;
                let (opening, limit) = (minor(&mut random), minor(&mut random));
                account_with_virtual(&conn, &format!("A{i}"), &crate::money::format_amount(opening), &crate::money::format_amount(limit));
            }
            let before = both_balances(&conn);
            let done = change(&mut conn, "USD", Some(rate_text), "2026-10-13").unwrap_or_else(|e| panic!("rate {rate_text}: {e}"));
            let after = both_balances(&conn);
            assert_eq!(done.accounts as usize, before.iter().filter(|(a, b)| *a != 0 || *b != 0).count(), "round {round} at {rate_text}");
            for ((old_opening, old_virtual), (new_opening, new_virtual)) in before.iter().zip(&after) {
                for (old, new) in [(old_opening, new_opening), (old_virtual, new_virtual)] {
                    assert_eq!(*new, convert(old.abs(), rate).unwrap() * old.signum(), "{old} at {rate_text}");
                    assert!(*new == 0 || new.signum() == old.signum(), "{old} became {new} at {rate_text}");
                    assert!(new.abs() <= MAX_MINOR);
                }
            }
        }
    }

    // ---- accounts in a currency of their own (docs/accounts-design.md, sections 21.1 and 21.6) ----

    /// An account in another currency, opened with `opening` at the start of 2026. Returns its id.
    fn foreign(conn: &Connection, name: &str, code: &str, opening: &str) -> i64 {
        accounts::create(
            conn,
            &NewAccount { currency: Some(code.into()), rate: Some("5.00".into()), opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), ..new_account(name) },
            TODAY,
        )
        .unwrap()
        .id
    }

    fn generation(conn: &Connection) -> i64 {
        conn.query_row("SELECT currency_generation FROM settings", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn the_currency_is_not_changed_while_an_account_has_one_of_its_own_archived_or_not_and_the_refusal_changes_nothing() {
        let (mut conn, food) = db();
        let small = spend(&conn, food, "10.00");
        let savings = account(&conn, "Savings", "100.00");
        let lei = foreign(&conn, "Lei", "RON", "500.00");
        let usd = foreign(&conn, "Dollars", "USD", "20.00");
        transfer(&conn, MAIN_ID, savings, "5.00", "2026-02-01");
        let before = snapshot(&conn);
        let refused = |conn: &mut Connection, what: &str| {
            for (code, rate) in [("USD", Some("2")), ("GBP", None), ("USD", Some("1")), ("US", Some("2")), ("EUR", Some("2"))] {
                assert_eq!(change(&mut *conn, code, rate, "2026-10-13").err(), Some(AppError::InvalidInput), "{what}: {code} {rate:?}");
            }
        };
        refused(&mut conn, "with two foreign accounts");
        assert_eq!((currency(&conn).as_str(), generation(&conn), amount_of(&conn, small), opening_of(&conn, savings), opening_of(&conn, lei)), ("EUR", 0, 1_000, 10_000, 50_000));
        assert_eq!(snapshot(&conn), before, "no row, stamp, marker, history row or tick of the clock changed");
        assert_eq!(conn.query_row::<i64, _, _>("SELECT count(*) FROM currency_changes", [], |r| r.get(0)).unwrap(), 0, "and nothing was recorded");
        // one of them gone is not enough, and an archived one counts as much as the others
        accounts::delete(&conn, usd).unwrap();
        refused(&mut conn, "with one foreign account");
        accounts::archive(&conn, lei, TODAY).unwrap();
        let archived = snapshot(&conn);
        refused(&mut conn, "with only an archived foreign account");
        assert_eq!(snapshot(&conn), archived);
        // set back to the main currency, it is an account like the others and the change is allowed
        accounts::restore(&conn, lei, TODAY).unwrap();
        accounts::update(&conn, lei, &AccountPatch { currency: Some("eur".into()), ..Default::default() }, TODAY).unwrap();
        let done = change(&mut conn, "USD", Some("2"), "2026-10-13").unwrap();
        assert_eq!((done.accounts, done.transfers, currency(&conn).as_str(), generation(&conn)), (2, 1, "USD", 1));
        assert_eq!((opening_of(&conn, lei), opening_of(&conn, savings), amount_of(&conn, small)), (100_000, 20_000, 2_000), "the account that came back is converted like the others");
    }

    #[test]
    fn the_currency_is_changed_again_once_the_last_foreign_account_is_deleted() {
        let (mut conn, _) = db();
        let lei = foreign(&conn, "Lei", "RON", "500.00");
        let old = foreign(&conn, "Old lei", "RON", "0.00");
        accounts::archive(&conn, old, TODAY).unwrap();
        assert_eq!(change(&mut conn, "USD", None, "2026-10-13").err(), Some(AppError::InvalidInput), "not even a change of the label alone");
        accounts::delete(&conn, lei).unwrap();
        assert_eq!(change(&mut conn, "USD", None, "2026-10-13").err(), Some(AppError::InvalidInput), "the archived one is still there");
        accounts::delete(&conn, old).unwrap();
        change(&mut conn, "USD", None, "2026-10-13").unwrap();
        assert_eq!((currency(&conn).as_str(), generation(&conn)), ("USD", 1));
        // a new account in another currency makes it refuse again, and the main currency's own code is not "another"
        let same = accounts::create(&conn, &NewAccount { currency: Some("usd".into()), ..new_account("Dollars") }, TODAY).unwrap();
        assert!(!same.foreign, "an account that names the main currency is one in the main currency");
        change(&mut conn, "GBP", None, "2026-10-14").unwrap();
        foreign(&conn, "Lei", "RON", "1.00");
        assert_eq!(change(&mut conn, "EUR", None, "2026-10-15").err(), Some(AppError::InvalidInput));
        assert_eq!((currency(&conn).as_str(), generation(&conn)), ("GBP", 2));
    }

    /// A device with accounts in the main currency, accounts in lei and in dollars, and a transfer of every kind between them.
    fn with_foreign_accounts() -> (Connection, i64) {
        let (conn, food) = db();
        let savings = account(&conn, "Savings", "1000.00");
        accounts::update(&conn, savings, &AccountPatch { virtual_balance: Some("250.00".into()), ..Default::default() }, TODAY).unwrap();
        let lei = foreign(&conn, "Lei", "RON", "500.00");
        let more_lei = foreign(&conn, "More lei", "RON", "0.00");
        let usd = foreign(&conn, "Dollars", "USD", "10.00");
        accounts::update(&conn, lei, &AccountPatch { virtual_balance: Some("100.00".into()), ..Default::default() }, TODAY).unwrap();
        spend(&conn, food, "10.00");
        transactions::create(
            &conn,
            &NewTransaction {
                name: "Lei shop".into(),
                amount: "20.00".into(),
                payment_date: "2026-06-15".into(),
                category_id: food.to_string(),
                description: None,
                client_uuid: None,
                account_id: Some(lei.to_string()),
                native_amount: Some("100.00".into()),
            },
        )
        .unwrap();
        let arriving = |from: i64, to: i64, amount: &str, arrives: Option<&str>| {
            accounts::transfers::create(&conn, &NewTransfer { to_amount: arrives.map(str::to_string), ..new_transfer(from, to, amount, "2026-02-01") }).unwrap();
        };
        arriving(MAIN_ID, savings, "100.00", None); // 1: both in the main currency
        arriving(MAIN_ID, lei, "10.00", Some("49.00")); // 2: leaves in the main currency, arrives in lei
        arriving(lei, MAIN_ID, "30.00", Some("6.00")); // 3: leaves in lei, arrives in the main currency
        arriving(lei, more_lei, "15.00", None); // 4: lei to lei
        arriving(lei, usd, "20.00", Some("4.00")); // 5: lei to dollars
        arriving(usd, savings, "5.00", Some("4.50")); // 6: dollars to the main currency
        (conn, food)
    }

    fn transfer_amounts(conn: &Connection) -> Vec<(i64, i64, Option<i64>)> {
        conn.prepare("SELECT id, amount_minor, to_amount_minor FROM transfers ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    }

    fn account_figures(conn: &Connection) -> Vec<(String, i64, i64)> {
        conn.prepare("SELECT name, opening_balance_minor, virtual_balance_minor FROM accounts ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    }

    fn stamps_of_everything(conn: &Connection) -> Vec<String> {
        conn.prepare("SELECT stamp || updated_at FROM accounts UNION ALL SELECT stamp || updated_at FROM transfers UNION ALL SELECT stamp || updated_at FROM transactions")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn a_conversion_arriving_by_sync_converts_what_is_in_the_main_currency_and_leaves_a_foreign_accounts_own_amounts_alone() {
        for rate in ["2", "0.5", "1.08", "0.001", "160"] {
            let (mut desktop, mut phone) = (with_foreign_accounts().0, with_foreign_accounts().0);
            // the desktop never knew the accounts in other currencies: only the phone has them
            for table_and_filter in ["DELETE FROM transfers WHERE id > 1", "DELETE FROM transactions WHERE account_id > 1", "DELETE FROM accounts WHERE currency IS NOT NULL"] {
                desktop.execute(table_and_filter, []).unwrap();
            }
            let parsed = parse_rate(rate).unwrap();
            change(&mut desktop, "USD", Some(rate), "2026-10-13").unwrap();
            let (accounts_before, transfers_before, stamps) = (account_figures(&phone), transfer_amounts(&phone), stamps_of_everything(&phone));
            let natives_before: Vec<Option<i64>> = phone.prepare("SELECT native_amount_minor FROM transactions ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
            let lei_balance = balance(&phone, 3, TODAY);
            assert_eq!(natives_before, [None, Some(10_000)]);

            let applied = apply_change_of(&desktop, &mut phone).unwrap();

            let converted = |minor: i64| at_least_one(convert(minor, parsed).unwrap());
            let signed = |minor: i64| convert(minor.abs(), parsed).unwrap() * minor.signum();
            // the accounts: the ones in the main currency are converted, the ones in lei and in dollars are not (the main account, savings: 1000.00 and a limit of 250.00)
            let names = ["Main account", "Savings", "Lei", "More lei", "Dollars"];
            let after = account_figures(&phone);
            assert_eq!(after.iter().map(|a| a.0.as_str()).collect::<Vec<_>>(), names);
            assert_eq!((after[0].1, after[0].2), (0, 0));
            assert_eq!((after[1].1, after[1].2), (signed(100_000), signed(25_000)), "rate {rate}: savings, in the main currency");
            assert_eq!(&after[2..], &accounts_before[2..], "rate {rate}: the opening balance and the virtual balance of an account in its own currency are as they were");
            // the rows: the amount in the main currency of every transaction, and never a native amount
            let amounts: Vec<i64> = phone.prepare("SELECT amount_minor FROM transactions ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
            assert_eq!(amounts, [converted(1_000), converted(2_000)], "rate {rate}");
            let natives: Vec<Option<i64>> = phone.prepare("SELECT native_amount_minor FROM transactions ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
            assert_eq!(natives, natives_before, "rate {rate}: what was paid in lei is in lei still");
            // the transfers, each amount by the currency of its own account
            let t = |n: usize| transfers_before[n - 1];
            let expected = [
                (1, converted(t(1).1), None),                            // main currency to main currency: converted
                (2, converted(t(2).1), t(2).2),                          // leaves in the main currency, arrives in lei: only what leaves
                (3, t(3).1, t(3).2.map(converted)),                      // leaves in lei, arrives in the main currency: only what arrives
                (4, t(4).1, None),                                       // lei to lei: neither
                (5, t(5).1, t(5).2),                                     // lei to dollars: neither
                (6, t(6).1, t(6).2.map(converted)),                      // dollars to the main currency: only what arrives
            ];
            assert_eq!(transfer_amounts(&phone), expected, "rate {rate}");
            // counted as the others are: the rows that had something converted
            assert_eq!((applied.transactions, applied.accounts, applied.transfers), (2, 1, 4), "rate {rate}");
            // the lei account holds what it held, in lei
            assert_eq!(balance(&phone, 3, TODAY), lei_balance, "rate {rate}");
            assert_eq!(stamps_of_everything(&phone), stamps, "rate {rate}: no stamp moved on the phone");
            assert_eq!((currency(&phone).as_str(), generation(&phone)), ("USD", 1));
            assert!(accounts::check(&phone).unwrap());
        }
    }

    #[test]
    fn an_amount_that_arrives_in_the_main_currency_and_leaves_the_range_refuses_the_whole_conversion_but_one_that_arrives_in_a_foreign_account_does_not() {
        let desktop_with_a_change = || {
            let (mut desktop, _) = db();
            change(&mut desktop, "USD", Some("2"), "2026-10-13").unwrap();
            desktop
        };
        // lei to the main currency: 9999999999.99 arrives in the main currency, and twice that is out of range
        let (mut phone, _) = db();
        let lei = foreign(&phone, "Lei", "RON", "500.00");
        let big = accounts::transfers::create(&phone, &NewTransfer { to_amount: Some("9999999999.99".into()), ..new_transfer(lei, MAIN_ID, "10.00", "2026-02-01") }).unwrap().id;
        account(&phone, "Savings", "5.00");
        let before = snapshot(&phone);
        assert_eq!(apply_change_of(&desktop_with_a_change(), &mut phone).err(), Some(AppError::InvalidInput));
        assert_eq!(snapshot(&phone), before, "the whole conversion was dropped: nothing changed, the small amounts included");
        assert_eq!((currency(&phone).as_str(), generation(&phone)), ("EUR", 0));
        assert_eq!(transfer_amounts(&phone), [(big, 1_000, Some(MAX_MINOR))]);
        // the same amount arriving in the account in lei is in lei, and is left as it is
        let (mut phone, _) = db();
        let lei = foreign(&phone, "Lei", "RON", "500.00");
        accounts::transfers::create(&phone, &NewTransfer { to_amount: Some("9999999999.99".into()), ..new_transfer(MAIN_ID, lei, "10.00", "2026-02-01") }).unwrap();
        // and the one that leaves in lei is too: 9999999999.99 lei leaves, to the main account, which gets a small amount
        accounts::transfers::create(&phone, &NewTransfer { to_amount: Some("1.00".into()), ..new_transfer(lei, MAIN_ID, "9999999999.99", "2026-02-02") }).unwrap();
        let applied = apply_change_of(&desktop_with_a_change(), &mut phone).unwrap();
        assert_eq!(applied.transfers, 2);
        assert_eq!(transfer_amounts(&phone), [(1, 2_000, Some(MAX_MINOR)), (2, MAX_MINOR, Some(200))]);
        assert_eq!((currency(&phone).as_str(), generation(&phone)), ("USD", 1));
    }

    #[test]
    fn a_tiny_amount_that_arrives_in_the_main_currency_stays_one_cent_like_every_transfer() {
        let (mut desktop, _) = db();
        change(&mut desktop, "USD", Some("0.001"), "2026-10-13").unwrap();
        let (mut phone, _) = db();
        let lei = foreign(&phone, "Lei", "RON", "500.00");
        accounts::transfers::create(&phone, &NewTransfer { to_amount: Some("0.01".into()), ..new_transfer(lei, MAIN_ID, "10.00", "2026-02-01") }).unwrap();
        accounts::transfers::create(&phone, &NewTransfer { to_amount: Some("0.01".into()), ..new_transfer(MAIN_ID, lei, "0.01", "2026-02-02") }).unwrap();
        apply_change_of(&desktop, &mut phone).unwrap();
        assert_eq!(transfer_amounts(&phone), [(1, 1_000, Some(1)), (2, 1, Some(1))], "one cent converted at 0.001 is a cent, not nothing; what is in lei is not touched");
    }

    // ---- stashes (docs/stash-design.md, section 8) ----

    use crate::stashes::fixtures as st;

    /// The target of every stash and the amount of every entry (signed), in the order they were made: (stash id, target, [entry amounts]).
    fn stash_numbers(conn: &Connection) -> Vec<(i64, Option<i64>, Vec<i64>)> {
        let ids: Vec<(i64, Option<i64>)> =
            conn.prepare("SELECT id, target_minor FROM stashes ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        ids.into_iter()
            .map(|(id, target)| {
                let entries = conn
                    .prepare("SELECT amount_minor FROM stash_entries WHERE stash_id = ?1 ORDER BY id")
                    .unwrap()
                    .query_map([id], |r| r.get(0))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                (id, target, entries)
            })
            .collect()
    }

    /// The stamp and `updated_at` of every stash and entry: what a conversion must not move.
    fn stash_stamps(conn: &Connection) -> Vec<String> {
        conn.prepare("SELECT stamp || updated_at FROM stashes UNION ALL SELECT stamp || updated_at FROM stash_entries")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    /// Stashes in the main currency: Holiday (a target, two adds and a take), Rainy day (no target, one add), a stash with a target and no
    /// entry, one with neither (nothing to convert), and an archived one that was spent. Savings is archived last, so it is in an archived account.
    fn with_stashes() -> Connection {
        let (conn, _) = db();
        let savings = st::funded(&conn, "Savings", "5000.00");
        let everyday = st::funded(&conn, "Everyday", "900.00");
        let holiday = st::aimed(&conn, "Holiday", savings, "1200.00");
        st::put_on(&conn, holiday, "100.00", "2026-05-01");
        st::put_on(&conn, holiday, "250.50", "2026-06-01");
        st::pull(&conn, holiday, "40.00");
        let rainy = st::stash(&conn, "Rainy day", everyday);
        st::put(&conn, rainy, "75.25");
        st::aimed(&conn, "Empty goal", everyday, "300.00");
        st::stash(&conn, "Bare", everyday);
        let bike = st::aimed(&conn, "Bike", everyday, "450.00");
        st::put(&conn, bike, "450.00");
        st::pull(&conn, bike, "450.00");
        crate::stashes::archive(&conn, bike, TODAY).unwrap();
        accounts::archive(&conn, savings, TODAY).unwrap();
        conn
    }

    #[test]
    fn the_target_and_every_entry_of_a_stash_are_converted_and_counted_and_a_take_keeps_its_sign() {
        let mut conn = with_stashes();
        let before = stash_numbers(&conn);
        assert_eq!(before, [(1, Some(120_000), vec![10_000, 25_050, -4_000]), (2, None, vec![7_525]), (3, Some(30_000), vec![]), (4, None, vec![]), (5, Some(45_000), vec![45_000, -45_000])]);

        let done = change(&mut conn, "USD", Some("1.08"), "2026-10-13").unwrap();

        assert_eq!((done.stashes, done.stash_entries), (4, 6), "the stash with neither a target nor an entry is not counted; the archived stash and the archived account are");
        assert_eq!(
            stash_numbers(&conn),
            [(1, Some(129_600), vec![10_800, 27_054, -4_320]), (2, None, vec![8_127]), (3, Some(32_400), vec![]), (4, None, vec![]), (5, Some(48_600), vec![48_600, -48_600])],
            "a target and an add are converted, a take is converted and stays below 0"
        );
        // what a stash holds is the sum of its converted pieces; the money in the accounts did not move by this, so only the numbers were scaled
        let holiday = st::shown(&conn, 1);
        assert_eq!((holiday.saved.as_str(), holiday.target.as_deref(), holiday.left.as_deref()), ("335.34", Some("1296.00"), Some("960.66")));
        assert_eq!(st::shown(&conn, 5).saved, "0.00", "the bike was taken out in full, and still is");
        assert_eq!(accounts::get(&conn, 2, st::TODAY).unwrap().set_aside, "335.34", "the account that holds the savings says what its stash holds, in the new numbers");
        // the generation moved, and the whole change is recorded as any other
        assert_eq!((currency(&conn).as_str(), generation(&conn)), ("USD", 1));
        for table in ["stashes", "stash_entries"] {
            let broken: i64 = conn.query_row(&format!("SELECT count(*) FROM pragma_foreign_key_check('{table}')"), [], |r| r.get(0)).unwrap();
            assert_eq!(broken, 0, "{table}");
        }
    }

    #[test]
    fn a_conversion_moves_no_stamp_and_no_updated_at_of_a_stash_or_an_entry_and_a_change_of_the_label_alone_touches_nothing() {
        let mut conn = with_stashes();
        let (stamps, numbers) = (stash_stamps(&conn), stash_numbers(&conn));
        // everything of the two tables but the amounts: the sync id, the stamp, the name, the account, the dates, the note, the link and the times
        let others = |conn: &Connection| -> Vec<String> {
            conn.prepare(
                "SELECT sync_id || '|' || stamp || '|' || name || '|' || account_id || '|' || COALESCE(target_date, '') || '|' || COALESCE(notes, '') || '|' || archived || '|' || position
                        || '|' || created_at || '|' || updated_at FROM stashes
                 UNION ALL
                 SELECT sync_id || '|' || stamp || '|' || stash_id || '|' || entry_date || '|' || COALESCE(note, '') || '|' || COALESCE(transfer_id, '') || '|' || created_at || '|' || updated_at FROM stash_entries",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
        };
        let rows = others(&conn);

        let done = change(&mut conn, "USD", None, "2026-10-13").unwrap();
        assert_eq!((done.stashes, done.stash_entries), (0, 0));
        assert_eq!(stash_numbers(&conn), numbers, "the label is all that changed");
        assert_eq!(others(&conn), rows);

        let done = change(&mut conn, "EUR", Some("2.5"), "2026-10-14").unwrap();
        assert_eq!((done.stashes, done.stash_entries), (4, 6));
        assert_ne!(stash_numbers(&conn), numbers);
        assert_eq!(stash_stamps(&conn), stamps, "converting is not an edit: no stamp and no `updated_at` of a stash or an entry moved");
        assert_eq!(others(&conn), rows, "nor does anything else but the amounts change");
    }

    #[test]
    fn targets_and_entries_at_the_bounds_are_converted_and_one_that_leaves_the_range_refuses_the_whole_change() {
        for (sign_of_last, label) in [(1i64, "an add"), (-1, "a take")] {
            let (mut conn, food) = db();
            let small = spend(&conn, food, "10.00");
            let big = st::funded(&conn, "Big", "9999999999.99");
            let edge = st::aimed(&conn, "Edge", big, "9999999999.99");
            st::put_on(&conn, edge, "9999999999.99", "2026-05-01");
            if sign_of_last < 0 {
                st::pull(&conn, edge, "9999999999.98");
            }
            let tiny = st::aimed(&conn, "Tiny", big, "5.00");
            let before = snapshot(&conn);
            for rate in ["1.00000001", "2", "999999999"] {
                assert_eq!(change(&mut conn, "USD", Some(rate), "2026-10-13"), Err(AppError::InvalidInput), "{label} at {rate}");
                assert_eq!(snapshot(&conn), before, "{label} at {rate}: no row, stamp or history line changed");
                assert_eq!(currency(&conn), "EUR");
            }
            assert_eq!((amount_of(&conn, small), stash_numbers(&conn)[1].clone()), (1_000, (tiny, Some(500), vec![])), "{label}: even the small ones are untouched");

            // at a rate that keeps it in the range it is converted, the bound itself included
            let last = if sign_of_last < 0 { -999_999_999_998 } else { 0 };
            change(&mut conn, "USD", Some("1"), "2026-10-13").unwrap();
            let numbers = stash_numbers(&conn);
            assert_eq!(numbers[0].1, Some(MAX_MINOR), "{label} at 1");
            assert_eq!(numbers[0].2[0], MAX_MINOR, "{label} at 1");
            assert_eq!(numbers[0].2.get(1).copied().unwrap_or(0), last, "{label} at 1");
            change(&mut conn, "GBP", Some("0.5"), "2026-10-14").unwrap();
            let numbers = stash_numbers(&conn);
            assert_eq!(numbers[0].1, Some(500_000_000_000), "{label} at 0.5: half a cent rounds up");
            assert_eq!(numbers[0].2[0], 500_000_000_000, "{label} at 0.5");
            assert_eq!(numbers[0].2.get(1).copied().unwrap_or(0), if sign_of_last < 0 { -499_999_999_999 } else { 0 }, "{label} at 0.5: the size is rounded and the sign put back");
            change(&mut conn, "CHF", Some("1.99999999"), "2026-10-15").unwrap();
            let rate = parse_rate("1.99999999").unwrap();
            let numbers = stash_numbers(&conn);
            assert_eq!(numbers[0].1, Some(convert(500_000_000_000, rate).unwrap()), "{label}: just inside the range");
            assert_eq!(numbers[0].2[0], convert(500_000_000_000, rate).unwrap(), "{label}");
            assert!(numbers[0].2.iter().all(|n| n.abs() <= MAX_MINOR && *n != 0), "{label}: every entry is in the range the table allows and is not 0");
        }
    }

    #[test]
    fn a_tiny_rate_keeps_a_target_and_every_entry_at_one_cent_and_keeps_the_sign() {
        let (mut conn, _) = db();
        let cash = st::funded(&conn, "Cash", "100.00");
        let cent = st::aimed(&conn, "Cent", cash, "0.01");
        st::put_on(&conn, cent, "0.01", "2026-05-01");
        let mixed = st::aimed(&conn, "Mixed", cash, "0.50");
        st::put_on(&conn, mixed, "0.40", "2026-05-01");
        st::put_on(&conn, mixed, "0.03", "2026-05-02");
        st::pull(&conn, mixed, "0.02");
        st::pull(&conn, mixed, "0.01");
        let done = change(&mut conn, "USD", Some("0.001"), "2026-10-13").unwrap();
        assert_eq!((done.stashes, done.stash_entries), (2, 5));
        assert_eq!(
            stash_numbers(&conn),
            [(1, Some(1), vec![1]), (2, Some(1), vec![1, 1, -1, -1])],
            "a target of half a cent is one cent, an add of 0.4 cent is one cent and a take of 0.02 is a take of one cent: none is 0"
        );
        let zero: i64 = conn.query_row("SELECT count(*) FROM stash_entries WHERE amount_minor = 0", [], |r| r.get(0)).unwrap();
        assert_eq!(zero, 0);
        // and it goes on at a rate that makes everything big again
        change(&mut conn, "EUR", Some("1000"), "2026-10-14").unwrap();
        assert_eq!(stash_numbers(&conn), [(1, Some(1_000), vec![1_000]), (2, Some(1_000), vec![1_000, 1_000, -1_000, -1_000])]);
    }

    #[test]
    fn random_stashes_and_rates_keep_the_sign_the_range_and_at_least_a_cent_in_every_target_and_entry() {
        let mut random = Rng(0x057A_54E5);
        for round in 0..40 {
            let (mut conn, _) = db();
            let rate_text = ["0.0001", "0.37", "1.08", "3.333", "160", "999.99"][round % 6];
            let rate = parse_rate(rate_text).unwrap();
            let account = st::funded(&conn, "Pot", "1000000.00");
            for i in 0..4 {
                let target = (random.between(0, 1) == 1).then(|| random.between(1, 900_000));
                let stash = match target {
                    Some(t) => st::aimed(&conn, &format!("S{i}"), account, &crate::money::format_amount(t as i64)),
                    None => st::stash(&conn, &format!("S{i}"), account),
                };
                for _ in 0..random.between(0, 4) {
                    st::raw(&conn, stash, if random.between(0, 2) == 0 { -(random.between(1, 90_000) as i64) } else { random.between(1, 90_000) as i64 }, "2026-05-01");
                }
            }
            let before = stash_numbers(&conn);
            let done = change(&mut conn, "USD", Some(rate_text), "2026-10-13").unwrap_or_else(|e| panic!("rate {rate_text}: {e}"));
            let after = stash_numbers(&conn);
            let mut entries = 0;
            for ((_, old_target, old_entries), (_, new_target, new_entries)) in before.iter().zip(&after) {
                assert_eq!(*new_target, old_target.map(|t| convert(t, rate).unwrap().max(1)), "target {old_target:?} at {rate_text}");
                assert_eq!(old_entries.len(), new_entries.len());
                for (old, new) in old_entries.iter().zip(new_entries) {
                    assert_eq!(*new, convert(old.abs(), rate).unwrap().max(1) * old.signum(), "entry {old} at {rate_text}");
                    assert!(new.abs() <= MAX_MINOR && new.signum() == old.signum(), "{old} became {new} at {rate_text}");
                }
                entries += old_entries.len() as u32;
            }
            assert_eq!(done.stash_entries, entries, "round {round}");
            assert_eq!(done.stashes as usize, before.iter().filter(|(_, target, entries)| target.is_some() || !entries.is_empty()).count(), "round {round}");
        }
    }

    /// The same stashes on any device: Holiday (a target, an add and a take) and Rainy day in the main account, and a Wedding gift in an account in lei.
    fn with_stashes_and_a_foreign_one() -> Connection {
        let (conn, _) = db();
        let savings = st::funded(&conn, "Savings", "5000.00");
        let lei = st::foreign(&conn, "Lei", "RON", "2000.00");
        let holiday = st::aimed(&conn, "Holiday", savings, "1200.00");
        st::put_on(&conn, holiday, "333.33", "2026-05-01");
        st::pull(&conn, holiday, "0.01");
        let rainy = st::stash(&conn, "Rainy day", savings);
        st::put(&conn, rainy, "20.00");
        let wedding = st::aimed(&conn, "Wedding gift", lei, "1000.00");
        st::put_on(&conn, wedding, "300.00", "2026-05-01");
        st::pull(&conn, wedding, "50.00");
        conn
    }

    #[test]
    fn the_same_conversion_arriving_by_sync_leaves_both_devices_with_the_same_stashes_and_a_stash_in_a_foreign_account_is_left_alone() {
        for rate in ["1.08", "0.0063", "160", "0.5", "2.5", "1.00000001", "0.001"] {
            // the desktop never had the account in lei: only the phone has it, with the stash that is in it
            let desktop_has = |conn: &Connection| {
                conn.execute_batch("DELETE FROM stash_entries WHERE stash_id = 3; DELETE FROM stashes WHERE id = 3; DELETE FROM accounts WHERE currency IS NOT NULL;").unwrap();
            };
            let (mut desktop, mut phone) = (with_stashes_and_a_foreign_one(), with_stashes_and_a_foreign_one());
            desktop_has(&desktop);
            let (stamps, wedding) = (stash_stamps(&phone), stash_numbers(&phone)[2].clone());
            assert_eq!(wedding, (3, Some(100_000), vec![30_000, -5_000]));
            let made = change(&mut desktop, "USD", Some(rate), "2026-10-13").unwrap();
            let applied = apply_change_of(&desktop, &mut phone).unwrap();

            assert_eq!((applied.stashes, applied.stash_entries), (made.stashes, made.stash_entries), "rate {rate}");
            assert_eq!((made.stashes, made.stash_entries), (2, 3), "rate {rate}: the two stashes of the main currency, with their entries");
            let (on_desktop, on_phone) = (stash_numbers(&desktop), stash_numbers(&phone));
            assert_eq!(on_phone[..2], on_desktop[..], "rate {rate}: the stashes of the main currency are equal on both devices");
            assert_eq!(on_phone[2], wedding, "rate {rate}: the stash in lei is in lei still");
            assert_eq!(stash_stamps(&phone), stamps, "rate {rate}: no stamp moved on the phone");
            assert_eq!((currency(&phone).as_str(), generation(&phone)), ("USD", 1));
            // what the account in lei has set aside is what it was
            assert_eq!(accounts::get(&phone, 3, st::TODAY).unwrap().set_aside, "250.00", "rate {rate}");
        }
    }

    #[test]
    fn a_conversion_arriving_by_sync_gives_a_database_equal_to_the_one_that_made_it_in_every_table_of_the_stashes() {
        for rate in ["1.08", "0.0063", "160", "0.5", "2.5", "0.001", "999.99"] {
            let (mut desktop, mut phone) = (with_stashes(), with_stashes());
            let made = change(&mut desktop, "USD", Some(rate), "2026-10-13").unwrap();
            let applied = apply_change_of(&desktop, &mut phone).unwrap();
            assert_eq!((applied.stashes, applied.stash_entries), (made.stashes, made.stash_entries), "rate {rate}");
            assert_eq!(stash_numbers(&phone), stash_numbers(&desktop), "rate {rate}");
            let rows = |conn: &Connection| -> Vec<String> {
                conn.prepare(
                    "SELECT 'S' || id || '|' || name || '|' || account_id || '|' || COALESCE(target_minor, '') || '|' || COALESCE(target_date, '') || '|' || archived || '|' || position
                     FROM stashes UNION ALL
                     SELECT 'E' || id || '|' || stash_id || '|' || amount_minor || '|' || entry_date || '|' || COALESCE(note, '') || '|' || COALESCE(transfer_id, '') FROM stash_entries",
                )
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect()
            };
            assert_eq!(rows(&phone), rows(&desktop), "rate {rate}: the same rows");
            for conn in [&desktop, &phone] {
                assert_eq!(accounts::list(conn, st::TODAY).unwrap(), accounts::list(&desktop, st::TODAY).unwrap(), "rate {rate}: the same set aside and free");
                assert!(accounts::check(conn).unwrap());
            }
        }
    }

    #[test]
    fn a_conversion_arriving_by_sync_that_takes_a_stash_out_of_the_range_is_refused_and_the_phone_keeps_its_numbers() {
        let (mut desktop, mut phone) = (with_stashes(), with_stashes());
        // only the phone has the stash that cannot be converted
        let edge = st::funded(&phone, "Edge", "9999999999.99");
        st::aimed(&phone, "Edge goal", edge, "9999999999.99");
        let before = snapshot(&phone);
        change(&mut desktop, "USD", Some("3"), "2026-10-13").unwrap();
        assert_eq!(apply_change_of(&desktop, &mut phone).err(), Some(AppError::InvalidInput));
        assert_eq!(snapshot(&phone), before, "the transaction was dropped: nothing changed");
        assert_eq!(currency(&phone), "EUR");
    }

    #[test]
    fn the_history_of_a_conversion_has_a_line_for_each_stash_target_and_entry_it_changed_as_it_has_for_a_transfer() {
        // the triggers of the history do not stand aside while a conversion applies (only the stamps and `updated_at` do), so the history has a line
        // for each row whose amount changed: what the stashes do here is what the transfers, the accounts and the transactions do
        let mut conn = with_stashes();
        transfer(&conn, MAIN_ID, 3, "5.00", "2026-02-01");
        let lines = |conn: &Connection, table: &str| -> i64 { conn.query_row("SELECT count(*) FROM audit_log WHERE entity_type = ?1 AND action = 'UPDATE'", [table], |r| r.get(0)).unwrap() };
        // (archiving the bike made one line before, and the entries and the transfer were only added)
        let before = (lines(&conn, "stashes"), lines(&conn, "stash_entries"), lines(&conn, "transfers"));
        assert_eq!(before, (1, 0, 0));
        let done = change(&mut conn, "USD", Some("2"), "2026-10-13").unwrap();
        assert_eq!((done.stashes, done.stash_entries, done.transfers), (4, 6, 1));
        // three of the four stashes have a target, so three rows changed; the rainy day has none, and its row is not written
        assert_eq!((lines(&conn, "stashes") - before.0, lines(&conn, "stash_entries") - before.1, lines(&conn, "transfers") - before.2), (3, 6, 1));
    }
}
