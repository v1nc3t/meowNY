//! Accounts: the places your money is (`docs/accounts-design.md`). A bank account, savings, cash, a credit card.
//!
//! * Every transaction, recurring item and ledger entry belongs to one account, and a transfer (`transfers`) moves money between two.
//! * There is always the main account (`MAIN_ID`): a row that names no account goes there, and it is never archived or deleted.
//! * A balance is stored nowhere. The view `account_movements` has one row for everything that moved an account, and every number
//!   here is a sum of its `delta_minor`: the balance on a day is the sum up to that day (section 3).
//! * An account with rows is archived, not deleted: its history and its balance stay.
//! * An account may be in a currency of its own (section 21): its numbers are in that currency, and only `balance_in_main` and net worth are in
//!   the app's. Budgets, statistics and the like never see a foreign amount.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::budgets;
use crate::categories::{clean_name, parse_id, present_or_null, serialize_id};
use crate::currency;
use crate::money::{format_amount, parse_signed_amount, MAX_MINOR};
use crate::transactions::{clean_date, clean_lines};
use crate::vault::AppError;

pub mod transfers;

#[cfg(test)]
mod balance_tests;
#[cfg(test)]
mod command_tests;
#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod table_tests;
#[cfg(test)]
mod transfers_tests;

/// The main account: the same local id and `sync_id` on every device, made by migration 0018 (section 4.3).
pub const MAIN_ID: i64 = 1;
pub const MAIN_SYNC_ID: &str = "00000000000000000000000000000001";
/// The name the migration gives it. The screens show it in the person's language while it is still this.
pub const MAIN_NAME: &str = "Main account";

const NUMBER_MAX_CHARS: usize = 50;
const BIC_MAX_CHARS: usize = 20;
const RATE_MAX_CHARS: usize = 20;
const NOTES_MAX_CHARS: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Kind {
    Checking,
    Savings,
    Cash,
    CreditCard,
}

impl Kind {
    pub fn as_sql(self) -> &'static str {
        match self {
            Kind::Checking => "CHECKING",
            Kind::Savings => "SAVINGS",
            Kind::Cash => "CASH",
            Kind::CreditCard => "CREDIT_CARD",
        }
    }

    pub fn from_sql(text: &str) -> Result<Kind, AppError> {
        match text {
            "CHECKING" => Ok(Kind::Checking),
            "SAVINGS" => Ok(Kind::Savings),
            "CASH" => Ok(Kind::Cash),
            "CREDIT_CARD" => Ok(Kind::CreditCard),
            _ => Err(AppError::Internal),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    pub name: String,
    pub kind: Kind,
    /// An IBAN or anything that tells the account apart.
    pub number: Option<String>,
    /// Decimal string that may be negative, e.g. "-120.00". It counts from `opening_date`.
    pub opening_balance: String,
    pub opening_date: String,
    pub archived: bool,
    pub is_main: bool,
    /// What the account holds today.
    pub balance: String,
    /// What moved it from the first of this month to today.
    pub change_this_month: String,
    /// The latest day up to today on which something other than the opening balance moved it.
    pub last_activity: Option<String>,
    /// How many transactions, recurring items, ledger entries and transfers point at it. An account with none can be deleted.
    pub entries: u32,
    /// The bank's identifier code.
    pub bic: Option<String>,
    /// Free text, line breaks allowed.
    pub notes: Option<String>,
    /// The name of the group the account is listed under.
    pub group: Option<String>,
    /// Whether `Summary` counts what the account holds (section 20.3).
    pub include_in_net_worth: bool,
    /// Decimal string that may be negative, e.g. a credit card's limit "1000.00". It is in no balance, statement, chart or net worth.
    pub virtual_balance: String,
    /// What the account can still pay: `balance` plus `virtual_balance`.
    pub available: String,
    /// The currency every amount of the account is in: its own code, or the app's currency for an account in the main currency.
    pub currency: String,
    /// Whether the account has a currency of its own.
    pub foreign: bool,
    /// How many units of the account's currency one unit of the app's currency buys, as typed ("5.00"). None for an account in the main
    /// currency.
    pub rate: Option<String>,
    /// `balance` in the app's currency: at `rate` for a foreign account, the same as `balance` for the others.
    pub balance_in_main: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewAccount {
    pub name: String,
    pub kind: Kind,
    #[serde(default)]
    pub number: Option<String>,
    /// Left out means "0.00".
    #[serde(default)]
    pub opening_balance: Option<String>,
    /// Left out means today.
    #[serde(default)]
    pub opening_date: Option<String>,
    #[serde(default)]
    pub bic: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    /// Left out means true.
    #[serde(default)]
    pub include_in_net_worth: Option<bool>,
    /// Left out means "0.00".
    #[serde(default)]
    pub virtual_balance: Option<String>,
    /// Three letters, in any case. Left out, or the app's currency, means an account in the main currency, which takes no `rate`.
    #[serde(default)]
    pub currency: Option<String>,
    /// Required with another currency (section 21.3).
    #[serde(default)]
    pub rate: Option<String>,
}

/// Fields left out stay as they are. `number`, `bic`, `notes` and `group` clear with `null`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub kind: Option<Kind>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub number: Option<Option<String>>,
    #[serde(default)]
    pub opening_balance: Option<String>,
    #[serde(default)]
    pub opening_date: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub bic: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub notes: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub group: Option<Option<String>>,
    #[serde(default)]
    pub include_in_net_worth: Option<bool>,
    #[serde(default)]
    pub virtual_balance: Option<String>,
    /// Only while nothing points at the account; the main account takes none.
    #[serde(default)]
    pub currency: Option<String>,
    /// Any time for a foreign account; the main account and an account in the main currency take none.
    #[serde(default)]
    pub rate: Option<String>,
    /// What the account holds today, in its own currency: the core moves the opening balance by the difference (section 21.5). Not with
    /// `opening_balance`.
    #[serde(default)]
    pub balance: Option<String>,
}

/// A number or a BIC: one line of at most `max` characters. Empty means "none".
fn clean_short(text: Option<&str>, max: usize) -> Result<Option<String>, AppError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    if text.chars().count() > max || text.chars().any(char::is_control) {
        return Err(AppError::InvalidInput);
    }
    Ok(Some(text.to_string()))
}

/// Notes may hold line breaks and no other control character, as a description does. Empty means "no notes".
fn clean_notes(text: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    let text = clean_lines(text).ok_or(AppError::InvalidInput)?;
    if text.chars().count() > NOTES_MAX_CHARS {
        return Err(AppError::InvalidInput);
    }
    Ok(Some(text))
}

/// A group is named as an account is. Empty means "no group".
fn clean_group(text: Option<&str>) -> Result<Option<String>, AppError> {
    text.map(str::trim).filter(|t| !t.is_empty()).map(clean_name).transpose()
}

/// A rate as the person typed it: one the rate parser takes (above 0, up to 8 decimals), at most 20 characters, spaces round it dropped.
fn clean_rate(text: &str) -> Result<String, AppError> {
    let text = text.trim();
    currency::parse_rate(text)?;
    if text.chars().count() > RATE_MAX_CHARS {
        return Err(AppError::InvalidInput);
    }
    Ok(text.to_string())
}

/// The currency and the rate an account ends with (section 21.3); `current` is what it has now, none and none for a new one, and `entries`
/// how many rows point at it. A code equal to the app's currency, in any case, is stored as none and takes no rate. The code changes only while
/// nothing points at the account. Another code needs a rate, the one in the input or the one already there; a rate that stays for an account
/// that is foreign and stays so changes at any time.
fn resolve_currency(
    conn: &Connection,
    current: (Option<&str>, Option<&str>),
    entries: u32,
    currency: Option<&str>,
    rate: Option<&str>,
) -> Result<(Option<String>, Option<String>), AppError> {
    let rate = rate.map(clean_rate).transpose()?;
    let code = match currency {
        Some(text) => {
            let code = currency::normalize_code(text)?;
            (code != crate::export::currency(conn)?).then_some(code)
        }
        None => current.0.map(str::to_string),
    };
    if code.as_deref() != current.0 && entries > 0 {
        return Err(AppError::InvalidInput);
    }
    match (code, rate) {
        (None, None) => Ok((None, None)),
        (None, Some(_)) => Err(AppError::InvalidInput),
        (Some(code), rate) => match rate.or_else(|| current.1.map(str::to_string)) {
            Some(rate) => Ok((Some(code), Some(rate))),
            None => Err(AppError::InvalidInput),
        },
    }
}

/// Whether the account is in a currency of its own.
pub(crate) fn is_foreign(conn: &Connection, id: i64) -> Result<bool, AppError> {
    Ok(conn.query_row("SELECT currency IS NOT NULL FROM accounts WHERE id = ?1", [id], |r| r.get(0)).optional()?.unwrap_or(false))
}

/// Refuses an account in a currency of its own. A recurring item, a ledger entry, a settlement and a bill code are in the app's currency
/// only (section 21.1).
pub fn require_main_currency(conn: &Connection, account_id: i64) -> Result<(), AppError> {
    if is_foreign(conn, account_id)? {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}

/// Whether two accounts are in different currencies: the codes differ, and the app's currency (no code) is one of them as much as another.
pub(crate) fn currencies_differ(conn: &Connection, a: i64, b: i64) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT (SELECT currency FROM accounts WHERE id = ?1) IS NOT (SELECT currency FROM accounts WHERE id = ?2)",
        params![a, b],
        |r| r.get(0),
    )?)
}

/// `minor`, an amount in the currency of an account, in the app's currency: divided by `rate`, the number of units of the first that one unit
/// of the second buys. Half away from zero to the cent, sign kept; the rate is parsed as a change of currency parses it, and the result must
/// stay within ±`MAX_MINOR`, else `InvalidInput`.
pub fn to_main(minor: i64, rate: &str) -> Result<i64, AppError> {
    let rate = currency::parse_rate(rate)?;
    // dividing by a rate is multiplying by the same fraction upside down
    let inverse = currency::Rate { numerator: rate.denominator, denominator: rate.numerator };
    Ok(currency::convert(minor.checked_abs().ok_or(AppError::InvalidInput)?, inverse)? * minor.signum())
}

/// What `minor` is worth in the app's currency for an account with this `rate` (none: an account in the main currency). An amount that does
/// not fit shows the bound it passed, so one odd rate never makes the whole list of accounts unreadable.
fn in_main(minor: i64, rate: Option<&str>) -> i64 {
    match rate {
        None => minor,
        Some(rate) => to_main(minor, rate).unwrap_or(MAX_MINOR * minor.signum()),
    }
}

/// Whether the account exists and is not archived.
pub fn is_active(conn: &Connection, id: i64) -> Result<bool, AppError> {
    Ok(conn.query_row("SELECT 1 FROM accounts WHERE id = ?1 AND archived = 0", [id], |_| Ok(())).optional()?.is_some())
}

/// The account of a new row: the main account when none is named, otherwise the named one, which must exist and not be archived
/// (section 5.2). A missing account and an archived one are refused alike, as an unknown category is.
pub fn for_new_row(conn: &Connection, account_id: Option<&str>) -> Result<i64, AppError> {
    let Some(text) = account_id else { return Ok(MAIN_ID) };
    let id = parse_id(text)?;
    if !is_active(conn, id)? {
        return Err(AppError::InvalidInput);
    }
    Ok(id)
}

/// The account of a row that is edited: the one it has when none is named or its own is named again (it may be archived by now), otherwise
/// the named one, as for a new row.
pub(crate) fn for_moved_row(conn: &Connection, current: i64, account_id: Option<&str>) -> Result<i64, AppError> {
    match account_id {
        Some(text) if parse_id(text)? != current => for_new_row(conn, Some(text)),
        _ => Ok(current),
    }
}

/// Whether the accounts are as they must be: the main account is there, with its own `sync_id`, not archived and in the main currency, and
/// every account has a rate exactly when it has a currency of its own. A sync checks it before its commit and a restore before it keeps a file.
/// A restore checks a file of an older schema in a database of that schema, before it is migrated forward: where the table has no currency
/// yet, only the first part can be asked.
pub fn check(conn: &Connection) -> Result<bool, AppError> {
    let main = conn
        .query_row("SELECT 1 FROM accounts WHERE id = ?1 AND sync_id = ?2 AND archived = 0", params![MAIN_ID, MAIN_SYNC_ID], |_| Ok(()))
        .optional()?
        .is_some();
    let has_currency: bool = conn.query_row("SELECT count(*) > 0 FROM pragma_table_info('accounts') WHERE name = 'currency'", [], |r| r.get(0))?;
    if !main || !has_currency {
        return Ok(main);
    }
    Ok(conn.query_row(
        "SELECT NOT EXISTS (SELECT 1 FROM accounts WHERE id = ?1 AND currency IS NOT NULL)
            AND NOT EXISTS (SELECT 1 FROM accounts WHERE (currency IS NULL) <> (rate IS NULL))",
        [MAIN_ID],
        |r| r.get(0),
    )?)
}

/// `?1` is today. The numbers come from the view, so there is one definition of what moves an account.
const SELECT: &str = "SELECT a.id, a.name, a.kind, a.number, a.opening_balance_minor, a.opening_date, a.archived,
       COALESCE((SELECT SUM(m.delta_minor) FROM account_movements m WHERE m.account_id = a.id AND m.day <= ?1), 0),
       COALESCE((SELECT SUM(m.delta_minor) FROM account_movements m WHERE m.account_id = a.id AND m.day <= ?1 AND m.day >= substr(?1, 1, 8) || '01'), 0),
       (SELECT MAX(m.day) FROM account_movements m WHERE m.account_id = a.id AND m.day <= ?1 AND m.source <> 'OPENING'),
       (SELECT count(*) FROM transactions WHERE account_id = a.id) + (SELECT count(*) FROM recurring_transactions WHERE account_id = a.id)
       + (SELECT count(*) FROM ledger_entries WHERE account_id = a.id) + (SELECT count(*) FROM transfers WHERE from_account_id = a.id OR to_account_id = a.id),
       a.bic, a.notes, a.group_name, a.include_net_worth, a.virtual_balance_minor,
       COALESCE(a.currency, (SELECT currency FROM settings WHERE id = 1)), a.currency IS NOT NULL, a.rate
     FROM accounts a";

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Account> {
    let kind: String = row.get(2)?;
    let id: i64 = row.get(0)?;
    let (balance, virtual_balance): (i64, i64) = (row.get(7)?, row.get(15)?);
    let rate: Option<String> = row.get(18)?;
    Ok(Account {
        id,
        name: row.get(1)?,
        kind: Kind::from_sql(&kind).map_err(|_| rusqlite::Error::InvalidQuery)?,
        number: row.get(3)?,
        opening_balance: format_amount(row.get(4)?),
        opening_date: row.get(5)?,
        archived: row.get::<_, i64>(6)? == 1,
        is_main: id == MAIN_ID,
        balance: format_amount(balance),
        change_this_month: format_amount(row.get(8)?),
        last_activity: row.get(9)?,
        entries: row.get(10)?,
        bic: row.get(11)?,
        notes: row.get(12)?,
        group: row.get(13)?,
        include_in_net_worth: row.get::<_, i64>(14)? == 1,
        virtual_balance: format_amount(virtual_balance),
        available: format_amount(balance + virtual_balance),
        currency: row.get(16)?,
        foreign: row.get::<_, i64>(17)? == 1,
        balance_in_main: format_amount(in_main(balance, rate.as_deref())),
        rate,
    })
}

/// The accounts in the person's order, the archived ones after the others.
pub fn list(conn: &Connection, today: &str) -> Result<Vec<Account>, AppError> {
    let today = clean_date(today)?;
    let mut statement = conn.prepare_cached(&format!("{SELECT} ORDER BY a.archived, a.position, a.id"))?;
    let rows = statement.query_map([&today], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(conn: &Connection, id: i64, today: &str) -> Result<Account, AppError> {
    let today = clean_date(today)?;
    conn.query_row(&format!("{SELECT} WHERE a.id = ?2"), params![today, id], from_row).optional()?.ok_or(AppError::NotFound)
}

/// A name is checked on this device only, among the accounts that are not archived, ignoring case (SQLite folds ASCII letters only). There
/// is no unique index: two devices that pick the same name keep two accounts (section 16, choice 4).
fn name_is_taken(conn: &Connection, name: &str, except: Option<i64>) -> Result<bool, AppError> {
    Ok(conn
        .query_row("SELECT 1 FROM accounts WHERE name = ?1 COLLATE NOCASE AND archived = 0 AND id IS NOT ?2", params![name, except], |_| Ok(()))
        .optional()?
        .is_some())
}

/// A new account goes to the end of the list.
pub fn create(conn: &Connection, input: &NewAccount, today: &str) -> Result<Account, AppError> {
    let name = clean_name(&input.name)?;
    let number = clean_short(input.number.as_deref(), NUMBER_MAX_CHARS)?;
    let opening = input.opening_balance.as_deref().map(parse_signed_amount).transpose()?.unwrap_or(0);
    let date = clean_date(input.opening_date.as_deref().unwrap_or(today))?;
    let bic = clean_short(input.bic.as_deref(), BIC_MAX_CHARS)?;
    let notes = clean_notes(input.notes.as_deref())?;
    let group = clean_group(input.group.as_deref())?;
    let virtual_balance = input.virtual_balance.as_deref().map(parse_signed_amount).transpose()?.unwrap_or(0);
    let (currency, rate) = resolve_currency(conn, (None, None), 0, input.currency.as_deref(), input.rate.as_deref())?;
    let tx = conn.unchecked_transaction()?;
    if name_is_taken(&tx, &name, None)? {
        return Err(AppError::NameExists);
    }
    tx.execute(
        "INSERT INTO accounts (name, kind, number, opening_balance_minor, opening_date, position, bic, notes, group_name, include_net_worth, virtual_balance_minor, currency, rate)
         VALUES (?1, ?2, ?3, ?4, ?5, (SELECT MAX(position) + 1 FROM accounts), ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![name, input.kind.as_sql(), number, opening, date, bic, notes, group, input.include_in_net_worth.unwrap_or(true), virtual_balance, currency, rate],
    )?;
    let account = get(&tx, tx.last_insert_rowid(), today)?;
    tx.commit()?;
    Ok(account)
}

/// Changes an account that is not archived (restore it first). The stamp moves only when something changed. `balance` sets the opening balance
/// so that the account holds exactly that today (section 21.5): what moved it up to today besides the opening balance is kept, and the opening
/// balance becomes the rest. That needs an opening date that is not after today, since an opening balance dated later is not in today's balance.
pub fn update(conn: &Connection, id: i64, patch: &AccountPatch, today: &str) -> Result<Account, AppError> {
    let tx = conn.unchecked_transaction()?;
    let current = get(&tx, id, today)?;
    if current.archived || (id == MAIN_ID && (patch.currency.is_some() || patch.rate.is_some())) || (patch.balance.is_some() && patch.opening_balance.is_some()) {
        return Err(AppError::InvalidInput);
    }
    let name = match &patch.name {
        Some(name) => clean_name(name)?,
        None => current.name,
    };
    let kind = patch.kind.unwrap_or(current.kind);
    let number = match &patch.number {
        Some(number) => clean_short(number.as_deref(), NUMBER_MAX_CHARS)?,
        None => current.number,
    };
    let date = clean_date(patch.opening_date.as_deref().unwrap_or(&current.opening_date))?;
    let opening = match &patch.balance {
        Some(text) => {
            let today = clean_date(today)?;
            let others: i64 = tx.query_row(
                "SELECT COALESCE(SUM(delta_minor), 0) FROM account_movements WHERE account_id = ?1 AND day <= ?2 AND source <> 'OPENING'",
                params![id, today],
                |r| r.get(0),
            )?;
            let opening = parse_signed_amount(text)? - others;
            if date > today || opening.abs() > MAX_MINOR {
                return Err(AppError::InvalidInput);
            }
            opening
        }
        None => parse_signed_amount(patch.opening_balance.as_deref().unwrap_or(&current.opening_balance))?,
    };
    let bic = match &patch.bic {
        Some(bic) => clean_short(bic.as_deref(), BIC_MAX_CHARS)?,
        None => current.bic,
    };
    let notes = match &patch.notes {
        Some(notes) => clean_notes(notes.as_deref())?,
        None => current.notes,
    };
    let group = match &patch.group {
        Some(group) => clean_group(group.as_deref())?,
        None => current.group,
    };
    let include = patch.include_in_net_worth.unwrap_or(current.include_in_net_worth);
    let virtual_balance = parse_signed_amount(patch.virtual_balance.as_deref().unwrap_or(&current.virtual_balance))?;
    let held = current.foreign.then_some(current.currency.as_str());
    let (currency, rate) = resolve_currency(&tx, (held, current.rate.as_deref()), current.entries, patch.currency.as_deref(), patch.rate.as_deref())?;
    if name_is_taken(&tx, &name, Some(id))? {
        return Err(AppError::NameExists);
    }
    tx.execute(
        "UPDATE accounts SET name = ?1, kind = ?2, number = ?3, opening_balance_minor = ?4, opening_date = ?5,
                bic = ?6, notes = ?7, group_name = ?8, include_net_worth = ?9, virtual_balance_minor = ?10, currency = ?12, rate = ?13 WHERE id = ?11",
        params![name, kind.as_sql(), number, opening, date, bic, notes, group, include, virtual_balance, id, currency, rate],
    )?;
    let account = get(&tx, id, today)?;
    tx.commit()?;
    Ok(account)
}

/// Sets the order of the accounts that are not archived: `ids` is all of them, in the new order. Only the rows that move are written,
/// so the others keep their sync stamp.
pub fn reorder(conn: &Connection, ids: &[i64]) -> Result<(), AppError> {
    if ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len() {
        return Err(AppError::InvalidInput);
    }
    let tx = conn.unchecked_transaction()?;
    for id in ids {
        if !is_active(&tx, *id)? {
            return Err(AppError::InvalidInput);
        }
    }
    let active: i64 = tx.query_row("SELECT count(*) FROM accounts WHERE archived = 0", [], |r| r.get(0))?;
    if active != ids.len() as i64 {
        return Err(AppError::InvalidInput);
    }
    for (position, id) in ids.iter().enumerate() {
        tx.execute("UPDATE accounts SET position = ?1 WHERE id = ?2 AND position != ?1", params![position as i64, id])?;
    }
    tx.commit()?;
    Ok(())
}

/// Takes an account out of the lists and pickers. Its rows, its history and its balance stay, and no row of it is written. The main
/// account is never archived; an account that is archived already is answered as it is.
pub fn archive(conn: &Connection, id: i64, today: &str) -> Result<Account, AppError> {
    let current = get(conn, id, today)?;
    if current.is_main {
        return Err(AppError::InvalidInput);
    }
    if current.archived {
        return Ok(current);
    }
    conn.execute("UPDATE accounts SET archived = 1 WHERE id = ?1", [id])?;
    get(conn, id, today)
}

/// Brings an archived account back. The name must not have been taken by another account in the meantime. An account that is not
/// archived is answered as it is.
pub fn restore(conn: &Connection, id: i64, today: &str) -> Result<Account, AppError> {
    let tx = conn.unchecked_transaction()?;
    let current = get(&tx, id, today)?;
    if !current.archived {
        return Ok(current);
    }
    if name_is_taken(&tx, &current.name, Some(id))? {
        return Err(AppError::NameExists);
    }
    tx.execute("UPDATE accounts SET archived = 0 WHERE id = ?1", [id])?;
    let account = get(&tx, id, today)?;
    tx.commit()?;
    Ok(account)
}

/// Deletes an account that nothing points at, archived or not. The main account and an account with rows are refused; the trigger
/// `trg_accounts_keep_main` and the foreign keys are the net under this.
pub fn delete(conn: &Connection, id: i64) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    let entries: Option<i64> = tx
        .query_row(
            "SELECT (SELECT count(*) FROM transactions WHERE account_id = a.id) + (SELECT count(*) FROM recurring_transactions WHERE account_id = a.id)
                  + (SELECT count(*) FROM ledger_entries WHERE account_id = a.id) + (SELECT count(*) FROM transfers WHERE from_account_id = a.id OR to_account_id = a.id)
             FROM accounts a WHERE a.id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let entries = entries.ok_or(AppError::NotFound)?;
    if id == MAIN_ID || entries > 0 {
        return Err(AppError::InvalidInput);
    }
    tx.execute("DELETE FROM accounts WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(())
}

/// What an account holds at the end of `day`: everything that moved it up to that day (section 3).
fn balance_on(conn: &Connection, id: i64, day: &str) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(delta_minor), 0) FROM account_movements WHERE account_id = ?1 AND day <= ?2",
        params![id, day],
        |r| r.get(0),
    )?)
}

/// One movement of an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub date: String,
    /// `OPENING`, `TRANSACTION`, `TRANSFER_IN`, `TRANSFER_OUT` or `LEDGER`.
    pub source: String,
    /// The id of the account, the transaction, the transfer or the ledger entry, by `source`.
    pub id: String,
    /// The name of the transaction, or the note of the transfer or the entry; empty when there is none.
    pub what: String,
    /// The category, the other account of a transfer, or the friend.
    pub other: Option<String>,
    /// What it did to the account: negative when money left it.
    pub amount: String,
    /// The balance after this line.
    pub balance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Statement {
    pub account: Account,
    /// The currency of every amount in it: the account's.
    pub currency: String,
    pub from: String,
    pub to: String,
    /// What the account held before `from`.
    pub balance_before: String,
    /// Oldest first.
    pub lines: Vec<Line>,
    /// What it held at the end of `to`.
    pub balance_after: String,
}

/// What moved an account from `from` to `to` (both days included), oldest first, with the balance after each line. On one day the opening
/// balance comes first, then transactions, transfers in, transfers out and ledger entries.
pub fn statement(conn: &Connection, id: i64, from: &str, to: &str, today: &str) -> Result<Statement, AppError> {
    let from = clean_date(from)?;
    let to = clean_date(to)?;
    if from > to {
        return Err(AppError::InvalidInput);
    }
    let tx = conn.unchecked_transaction()?;
    let account = get(&tx, id, today)?;
    let before: i64 = tx.query_row(
        "SELECT COALESCE(SUM(delta_minor), 0) FROM account_movements WHERE account_id = ?1 AND day < ?2",
        params![id, from],
        |r| r.get(0),
    )?;
    let mut statement = tx.prepare_cached(
        "SELECT m.day, m.source, m.source_id, m.delta_minor,
                CASE m.source WHEN 'TRANSACTION' THEN t.name WHEN 'LEDGER' THEN e.note WHEN 'OPENING' THEN NULL ELSE x.note END,
                CASE m.source WHEN 'TRANSACTION' THEN c.name WHEN 'LEDGER' THEN p.name WHEN 'OPENING' THEN NULL ELSE o.name END
         FROM account_movements m
         LEFT JOIN transactions t ON m.source = 'TRANSACTION' AND t.id = m.source_id
         LEFT JOIN categories c ON c.id = t.category_id
         LEFT JOIN ledger_entries e ON m.source = 'LEDGER' AND e.id = m.source_id
         LEFT JOIN people p ON p.id = e.person_id
         LEFT JOIN transfers x ON m.source IN ('TRANSFER_IN', 'TRANSFER_OUT') AND x.id = m.source_id
         LEFT JOIN accounts o ON o.id = CASE m.source WHEN 'TRANSFER_IN' THEN x.from_account_id ELSE x.to_account_id END
         WHERE m.account_id = ?1 AND m.day >= ?2 AND m.day <= ?3
         ORDER BY m.day, CASE m.source WHEN 'OPENING' THEN 0 WHEN 'TRANSACTION' THEN 1 WHEN 'TRANSFER_IN' THEN 2 WHEN 'TRANSFER_OUT' THEN 3 ELSE 4 END, m.source_id",
    )?;
    let rows = statement.query_map(params![id, from, to], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?))
    })?;
    let mut running = before;
    let mut lines = Vec::new();
    for row in rows {
        let (date, source, source_id, delta, what, other) = row?;
        running += delta;
        lines.push(Line { date, source, id: source_id.to_string(), what: what.unwrap_or_default(), other, amount: format_amount(delta), balance: format_amount(running) });
    }
    Ok(Statement { currency: account.currency.clone(), account, from, to, balance_before: format_amount(before), lines, balance_after: format_amount(running) })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryPoint {
    /// "YYYY-MM".
    pub month: String,
    pub balance: String,
}

/// The balance at the end of each month of `year` ("YYYY"), for the chart: on the last day of a month that is over, on `today` for the
/// current month, and no point for a month that has not begun.
pub fn history(conn: &Connection, id: i64, year: &str, today: &str) -> Result<Vec<HistoryPoint>, AppError> {
    let today = clean_date(today)?;
    budgets::month_start(&format!("{year}-01"))?;
    let tx = conn.unchecked_transaction()?;
    get(&tx, id, &today)?;
    let mut points = Vec::new();
    for m in 1..=12 {
        let month = format!("{year}-{m:02}");
        let first = budgets::month_start(&month)?;
        if first.to_string() > today {
            break;
        }
        let last = budgets::next_month(first)?.pred_opt().ok_or(AppError::InvalidInput)?.to_string();
        let balance = balance_on(&tx, id, last.as_str().min(today.as_str()))?;
        points.push(HistoryPoint { month, balance: format_amount(balance) });
    }
    Ok(points)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// What every account that counts in net worth holds today, the archived ones too, in the app's currency: a foreign account at its rate.
    pub in_accounts: String,
    /// What friends owe you minus what you owe them: money a friend owes you is yours.
    pub friends: String,
    pub net_worth: String,
    /// How many accounts there are, the archived ones too, and the ones that do not count in net worth.
    pub accounts: u32,
    /// Whether the person uses accounts: there is more than the main account, or it has an opening balance.
    pub in_use: bool,
}

pub fn summary(conn: &Connection, today: &str) -> Result<Summary, AppError> {
    let today = clean_date(today)?;
    let tx = conn.unchecked_transaction()?;
    // each account is turned into the app's currency on its own: a sum over several currencies is no number
    let in_accounts: i64 = tx
        .prepare(
            "SELECT a.rate, COALESCE((SELECT SUM(m.delta_minor) FROM account_movements m WHERE m.account_id = a.id AND m.day <= ?1), 0)
             FROM accounts a WHERE a.include_net_worth = 1",
        )?
        .query_map([&today], |r| Ok(in_main(r.get(1)?, r.get::<_, Option<String>>(0)?.as_deref())))?
        .sum::<Result<i64, _>>()?;
    let friends: i64 = tx.query_row("SELECT COALESCE(SUM(balance_minor), 0) FROM person_balances", [], |r| r.get(0))?;
    let (accounts, in_use): (u32, bool) = tx.query_row(
        "SELECT count(*), count(*) > 1 OR COALESCE(SUM(opening_balance_minor <> 0), 0) > 0 FROM accounts",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(Summary { in_accounts: format_amount(in_accounts), friends: format_amount(friends), net_worth: format_amount(in_accounts + friends), accounts, in_use })
}

/// What an account paid or took in for one category in a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    #[serde(serialize_with = "serialize_id")]
    pub category_id: i64,
    /// A category that was deleted since keeps its name here.
    pub name: String,
    /// Always positive, the whole amount of the transactions.
    pub amount: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakdown {
    /// The currency of every amount in it: the account's.
    pub currency: String,
    pub from: String,
    pub to: String,
    /// Categories of expenses, the largest amount first, then by name.
    pub expenses: Vec<Share>,
    /// Categories of income, in the same order.
    pub income: Vec<Share>,
    pub expense_total: String,
    pub income_total: String,
}

/// What an account paid and took in from `from` to `to` (both days included), by category. It reads the same movements as the balance
/// does, so a bill a friend paid is not in it and a bill you paid is in it whole; transfers and ledger entries are not transactions and are
/// not in it. A period that reaches into the future holds the rows dated there: this is a report of a period, not of today.
pub fn breakdown(conn: &Connection, id: i64, from: &str, to: &str) -> Result<Breakdown, AppError> {
    let from = clean_date(from)?;
    let to = clean_date(to)?;
    if from > to {
        return Err(AppError::InvalidInput);
    }
    let tx = conn.unchecked_transaction()?;
    let currency: String = tx
        .query_row("SELECT COALESCE(currency, (SELECT currency FROM settings WHERE id = 1)) FROM accounts WHERE id = ?1", [id], |r| r.get(0))
        .optional()?
        .ok_or(AppError::NotFound)?;
    let mut statement = tx.prepare_cached(
        "SELECT c.id, c.name, c.type, SUM(m.delta_minor)
         FROM account_movements m
         JOIN transactions t ON m.source = 'TRANSACTION' AND t.id = m.source_id
         JOIN categories c ON c.id = t.category_id
         WHERE m.account_id = ?1 AND m.day >= ?2 AND m.day <= ?3
         GROUP BY c.id ORDER BY abs(SUM(m.delta_minor)) DESC, c.name COLLATE NOCASE, c.id",
    )?;
    let rows = statement.query_map(params![id, from, to], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?;
    let (mut expenses, mut income) = (Vec::new(), Vec::new());
    let (mut expense_total, mut income_total) = (0i64, 0i64);
    for row in rows {
        // the view signs a movement by the type of the category: money out is negative
        let (category_id, name, kind, delta) = row?;
        let share = Share { category_id, name, amount: format_amount(delta.abs()) };
        if kind == "INCOME" {
            income_total += delta;
            income.push(share);
        } else {
            expense_total -= delta;
            expenses.push(share);
        }
    }
    Ok(Breakdown { currency, from, to, expenses, income, expense_total: format_amount(expense_total), income_total: format_amount(income_total) })
}
