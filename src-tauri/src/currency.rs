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
}

/// An exchange rate as an exact fraction, so no floating point touches money.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rate {
    numerator: i128,
    denominator: i128,
}

/// "1.08", "0.0063", "160": 1 to 9 whole digits (no leading zeros), up to 8 decimals, above zero.
fn parse_rate(text: &str) -> Result<Rate, AppError> {
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
fn convert(minor: i64, rate: Rate) -> Result<i64, AppError> {
    let scaled = i128::from(minor) * rate.numerator;
    let rounded = (scaled * 2 + rate.denominator) / (2 * rate.denominator);
    i64::try_from(rounded).ok().filter(|v| *v <= MAX_MINOR).ok_or(AppError::InvalidInput)
}

/// One amount converted at a rate the person typed, with the same rules and rounding as a change of currency (half up, to the cent).
pub(crate) fn convert_at(minor: i64, rate: &str) -> Result<i64, AppError> {
    convert(minor, parse_rate(rate)?)
}

fn normalize_code(code: &str) -> Result<String, AppError> {
    let code = code.trim().to_ascii_uppercase();
    if code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(code)
    } else {
        Err(AppError::InvalidInput)
    }
}

/// Changes the currency. With `rate`, every stored amount is converted; without it, only the label changes.
/// Either everything changes or nothing does.
pub fn change(conn: &mut Connection, new_currency: &str, rate: Option<&str>, today: &str) -> Result<Conversion, AppError> {
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
}
