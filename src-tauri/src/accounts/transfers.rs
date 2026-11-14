//! Transfers: money moved between two of your accounts (`docs/accounts-design.md`, section 5.3). A transfer is never income or expense
//! and is in no budget or statistic: it lives only in `transfers`, and only the balances of its two accounts read it.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{for_moved_row, for_new_row};
use crate::categories::{parse_id, present_or_null, serialize_id};
use crate::debts::ledger::clean_note;
use crate::money::{format_amount, parse_positive_amount};
use crate::transactions::{clean_date, DEFAULT_LIMIT, MAX_LIMIT};
use crate::vault::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    #[serde(serialize_with = "serialize_id")]
    pub from_account_id: i64,
    #[serde(serialize_with = "serialize_id")]
    pub to_account_id: i64,
    /// Decimal string, e.g. "200.00". Always positive: it leaves `from` and arrives in `to`.
    pub amount: String,
    pub date: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewTransfer {
    pub from_account_id: String,
    pub to_account_id: String,
    pub amount: String,
    pub date: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Fields left out stay as they are. `note: null` clears it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferPatch {
    #[serde(default)]
    pub from_account_id: Option<String>,
    #[serde(default)]
    pub to_account_id: Option<String>,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub note: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Filter {
    /// The transfers that leave or reach this account.
    #[serde(default)]
    pub account_id: Option<String>,
    /// Inclusive, "YYYY-MM-DD".
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct Page {
    pub transfers: Vec<Transfer>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

const COLUMNS: &str = "id, from_account_id, to_account_id, amount_minor, transfer_date, note";

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Transfer> {
    Ok(Transfer {
        id: row.get(0)?,
        from_account_id: row.get(1)?,
        to_account_id: row.get(2)?,
        amount: format_amount(row.get(3)?),
        date: row.get(4)?,
        note: row.get(5)?,
    })
}

pub fn get(conn: &Connection, id: i64) -> Result<Transfer, AppError> {
    conn.query_row(&format!("SELECT {COLUMNS} FROM transfers WHERE id = ?1"), [id], from_row).optional()?.ok_or(AppError::NotFound)
}

/// Both accounts must exist, not be archived, and differ.
pub fn create(conn: &Connection, input: &NewTransfer) -> Result<Transfer, AppError> {
    let minor = parse_positive_amount(&input.amount)?;
    let date = clean_date(&input.date)?;
    let note = clean_note(input.note.as_deref())?;
    let from = for_new_row(conn, Some(&input.from_account_id))?;
    let to = for_new_row(conn, Some(&input.to_account_id))?;
    if from == to {
        return Err(AppError::InvalidInput);
    }
    conn.execute(
        "INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date, note) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![from, to, minor, date, note],
    )?;
    get(conn, conn.last_insert_rowid())
}

/// A side that does not change may be an archived account by now; a side that changes must be one that is not. The stamp moves only when
/// something changed.
pub fn update(conn: &Connection, id: i64, patch: &TransferPatch) -> Result<Transfer, AppError> {
    let current = get(conn, id)?;
    let from = for_moved_row(conn, current.from_account_id, patch.from_account_id.as_deref())?;
    let to = for_moved_row(conn, current.to_account_id, patch.to_account_id.as_deref())?;
    if from == to {
        return Err(AppError::InvalidInput);
    }
    let minor = parse_positive_amount(patch.amount.as_deref().unwrap_or(&current.amount))?;
    let date = clean_date(patch.date.as_deref().unwrap_or(&current.date))?;
    let note = match &patch.note {
        Some(note) => clean_note(note.as_deref())?,
        None => current.note,
    };
    conn.execute(
        "UPDATE transfers SET from_account_id = ?1, to_account_id = ?2, amount_minor = ?3, transfer_date = ?4, note = ?5 WHERE id = ?6",
        params![from, to, minor, date, note, id],
    )?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: i64) -> Result<(), AppError> {
    match conn.execute("DELETE FROM transfers WHERE id = ?1", [id])? {
        0 => Err(AppError::NotFound),
        _ => Ok(()),
    }
}

/// Newest date first, then newest id.
pub fn list(conn: &Connection, filter: &Filter) -> Result<Page, AppError> {
    let from = filter.from.as_deref().map(clean_date).transpose()?;
    let to = filter.to.as_deref().map(clean_date).transpose()?;
    let account = filter.account_id.as_deref().map(parse_id).transpose()?;
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(AppError::InvalidInput);
    }
    let offset = filter.offset.unwrap_or(0);

    // Only the filters that are set go into the query, as in `transactions::list`.
    let mut clauses: Vec<&str> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(from) = from {
        clauses.push("transfer_date >= ?");
        values.push(Box::new(from));
    }
    if let Some(to) = to {
        clauses.push("transfer_date <= ?");
        values.push(Box::new(to));
    }
    if let Some(account) = account {
        clauses.push("(from_account_id = ? OR to_account_id = ?)");
        values.push(Box::new(account));
        values.push(Box::new(account));
    }
    let filter_sql = if clauses.is_empty() { String::new() } else { format!("WHERE {}", clauses.join(" AND ")) };

    let total: u32 = conn.query_row(
        &format!("SELECT count(*) FROM transfers {filter_sql}"),
        rusqlite::params_from_iter(values.iter().map(|v| v.as_ref())),
        |r| r.get(0),
    )?;
    let mut page_values: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    page_values.push(&limit);
    page_values.push(&offset);
    let mut statement = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM transfers {filter_sql} ORDER BY transfer_date DESC, id DESC LIMIT ? OFFSET ?"
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(page_values), from_row)?;
    Ok(Page { transfers: rows.collect::<Result<_, _>>()?, total, limit, offset })
}
