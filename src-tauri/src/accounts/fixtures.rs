//! What the tests of accounts and transfers build their worlds from, on top of the fixtures of the debts.

use rusqlite::types::ValueRef;
use rusqlite::Connection;

pub(crate) use crate::debts::fixtures::{friend, one, text, world, Rng, World};

use super::transfers::{self, NewTransfer};
use super::{Kind, NewAccount};
use crate::vault::AppError;

/// The day the tests call today.
pub(crate) const TODAY: &str = "2026-11-13";

pub(crate) const INVALID: AppError = AppError::InvalidInput;

pub(crate) fn new_account(name: &str) -> NewAccount {
    NewAccount { name: name.into(), kind: Kind::Checking, number: None, opening_balance: None, opening_date: None }
}

/// A checking account with nothing in it. Returns its id.
pub(crate) fn account(conn: &Connection, name: &str) -> i64 {
    super::create(conn, &new_account(name), TODAY).unwrap().id
}

/// An account that was archived.
pub(crate) fn archived_account(conn: &Connection, name: &str) -> i64 {
    let id = account(conn, name);
    super::archive(conn, id, TODAY).unwrap();
    id
}

pub(crate) fn new_transfer(from: i64, to: i64, amount: &str, date: &str) -> NewTransfer {
    NewTransfer { from_account_id: from.to_string(), to_account_id: to.to_string(), amount: amount.into(), date: date.into(), note: None }
}

pub(crate) fn transfer(conn: &Connection, from: i64, to: i64, amount: &str, date: &str) -> i64 {
    transfers::create(conn, &new_transfer(from, to, amount, date)).unwrap().id
}

pub(crate) fn stamp_of(conn: &Connection, table: &str, id: i64) -> String {
    text(conn, &format!("SELECT stamp FROM {table} WHERE id = {id}"))
}

pub(crate) fn applying(conn: &Connection, on: bool) {
    conn.execute("UPDATE sync_clock SET applying = ?1", [on]).unwrap();
}

/// What an account holds at the end of `day`, read from the view.
pub(crate) fn balance(conn: &Connection, id: i64, day: &str) -> i64 {
    conn.query_row("SELECT COALESCE(SUM(delta_minor), 0) FROM account_movements WHERE account_id = ?1 AND day <= ?2", rusqlite::params![id, day], |r| r.get(0))
        .unwrap()
}

/// Everything a write could touch, as text: every row and column of the tables of this feature and of the rows that carry an account
/// (stamps and `updated_at` among them), the deletion markers, the history, and the clock that makes stamps. Two equal snapshots mean
/// that nothing was written and no stamp was made.
pub(crate) fn snapshot(conn: &Connection) -> String {
    let mut out = String::new();
    for table in ["accounts", "transfers", "transactions", "recurring_transactions", "ledger_entries", "transaction_split_lines", "people", "sync_deleted", "audit_log"] {
        out.push_str(&format!("== {table}\n"));
        let mut statement = conn.prepare(&format!("SELECT * FROM {table} ORDER BY id")).unwrap();
        let columns = statement.column_count();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for i in 0..columns {
                match row.get_ref(i).unwrap() {
                    ValueRef::Null => out.push_str("NULL"),
                    ValueRef::Integer(n) => out.push_str(&n.to_string()),
                    ValueRef::Real(n) => out.push_str(&n.to_string()),
                    ValueRef::Text(t) => out.push_str(&String::from_utf8_lossy(t)),
                    ValueRef::Blob(b) => out.push_str(&format!("{b:?}")),
                }
                out.push('|');
            }
            out.push('\n');
        }
    }
    out.push_str(&text(conn, "SELECT last_ms || '-' || last_counter FROM sync_clock WHERE id = 1"));
    out
}

/// A refused event: it answers `expected` and changes nothing (no row written, no stamp, no marker, no history row).
pub(crate) fn assert_refused<T: std::fmt::Debug>(conn: &Connection, expected: AppError, what: &str, event: impl FnOnce() -> Result<T, AppError>) {
    let before = snapshot(conn);
    assert_eq!(event().err(), Some(expected), "{what}");
    assert_eq!(snapshot(conn), before, "{what}: a refused event changes nothing");
}

/// An event that is allowed and writes nothing. Gives back what it answered.
pub(crate) fn assert_nothing_written<T: std::fmt::Debug>(conn: &Connection, what: &str, event: impl FnOnce() -> Result<T, AppError>) -> T {
    let before = snapshot(conn);
    let answer = event().unwrap_or_else(|e| panic!("{what}: {e:?}"));
    assert_eq!(snapshot(conn), before, "{what}: nothing is written");
    answer
}
