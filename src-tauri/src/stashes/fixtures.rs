//! What the tests of stashes build their worlds from, on top of the fixtures of accounts.

use rusqlite::Connection;

pub(crate) use crate::accounts::fixtures::*;

use super::entries::{self, NewEntry, NewTake};
use super::NewStash;
use crate::accounts::{self, NewAccount};
use crate::transactions::{self, NewTransaction};

pub(crate) const NOT_FOUND: crate::vault::AppError = crate::vault::AppError::NotFound;
pub(crate) const NAME_EXISTS: crate::vault::AppError = crate::vault::AppError::NameExists;

/// An account with an opening balance, dated well before the day the tests call today. Returns its id.
pub(crate) fn funded(conn: &Connection, name: &str, opening: &str) -> i64 {
    accounts::create(conn, &NewAccount { opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), ..new_account(name) }, TODAY).unwrap().id
}

/// An account in another currency, with an opening balance in that currency.
pub(crate) fn foreign(conn: &Connection, name: &str, code: &str, opening: &str) -> i64 {
    accounts::create(
        conn,
        &NewAccount { currency: Some(code.into()), rate: Some("5.00".into()), opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), ..new_account(name) },
        TODAY,
    )
    .unwrap()
    .id
}

pub(crate) fn new_stash(name: &str, account: i64) -> NewStash {
    NewStash { name: name.into(), account_id: account.to_string(), kind: None, target: None, target_date: None, notes: None, interest_rate: None, interest_period: None, start_date: None }
}

/// A stash with no target. Returns its id.
pub(crate) fn stash(conn: &Connection, name: &str, account: i64) -> i64 {
    super::create(conn, &new_stash(name, account), TODAY).unwrap().id
}

/// A stash with a target like "1200.00". Returns its id.
pub(crate) fn aimed(conn: &Connection, name: &str, account: i64, target: &str) -> i64 {
    super::create(conn, &NewStash { target: Some(target.into()), ..new_stash(name, account) }, TODAY).unwrap().id
}

pub(crate) fn new_entry(amount: &str) -> NewEntry {
    NewEntry { amount: amount.into(), date: None, note: None, from_account_id: None }
}

pub(crate) fn new_take(amount: &str) -> NewTake {
    NewTake { amount: amount.into(), date: None, note: None, to_account_id: None }
}

/// Adds money dated today. Returns the id of the entry.
pub(crate) fn put(conn: &Connection, stash: i64, amount: &str) -> i64 {
    entries::add(conn, stash, &new_entry(amount), TODAY).unwrap().entries[0].id
}

/// Adds money on a day. Returns the id of the entry.
pub(crate) fn put_on(conn: &Connection, stash: i64, amount: &str, date: &str) -> i64 {
    entries::add(conn, stash, &entries::NewEntry { date: Some(date.into()), ..new_entry(amount) }, TODAY).unwrap();
    one(conn, "SELECT MAX(id) FROM stash_entries")
}

/// Takes money out, dated today. Returns the id of the entry.
pub(crate) fn pull(conn: &Connection, stash: i64, amount: &str) -> i64 {
    entries::take(conn, stash, &new_take(amount), TODAY).unwrap().entries[0].id
}

/// What an account has free today, from the code that works it out.
pub(crate) fn free_of(conn: &Connection, account: i64) -> i64 {
    super::free(conn, account, TODAY).unwrap()
}

/// A transaction of this account, on a day: an expense from an expense category (the balance falls), an income from an income one (it grows).
pub(crate) fn spend(conn: &Connection, category: i64, account: i64, amount: &str, date: &str) -> i64 {
    transactions::create(
        conn,
        &NewTransaction {
            name: "Spent".into(),
            amount: amount.into(),
            payment_date: date.into(),
            category_id: category.to_string(),
            description: None,
            client_uuid: None,
            account_id: Some(account.to_string()),
            native_amount: None,
        },
    )
    .unwrap()
    .id
}

/// An entry written straight into the table, as a sync brings it: none of the rules of the core is asked. Returns its id.
pub(crate) fn raw(conn: &Connection, stash: i64, minor: i64, date: &str) -> i64 {
    conn.execute("INSERT INTO stash_entries (stash_id, amount_minor, entry_date) VALUES (?1, ?2, ?3)", rusqlite::params![stash, minor, date]).unwrap();
    conn.last_insert_rowid()
}

/// The stash as the screens show it.
pub(crate) fn shown(conn: &Connection, stash: i64) -> super::Stash {
    super::stash(conn, stash, TODAY).unwrap()
}
