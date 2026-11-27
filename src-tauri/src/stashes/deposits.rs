//! Book a term deposit's return (`docs/loans-deposits-design.md`, section 5.5): the interest of a matured deposit arrives in its account as an income,
//! and the same amount is added to the deposit, so what the bank paid stays in the deposit until the person takes it out. Both are made in one
//! database transaction, so a refusal writes neither, and both are ordinary rows afterwards.
//!
//! Nothing is checked twice: the income is made by `transactions::create` and the add by `entries::add_in`, which keep their own rules (the delta rule
//! sees the income, and the add is the one write that a deposit that is no longer running takes).

use rusqlite::{params, Connection};
use serde::Deserialize;

use super::entries::{add_in, NewEntry};
use super::{facts, stash, DepositState, Stash};
use crate::accounts::pay::entry_amounts;
use crate::categories::parse_id;
use crate::debts::ledger::clean_note;
use crate::money::parse_positive_amount;
use crate::transactions::{self, category_is_income, clean_date, NewTransaction};
use crate::vault::AppError;

/// What the page sends: `{ "amount": "31.00", "categoryId": "7", "date": "2027-01-02", "note": "interest" }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReturnInput {
    /// "0.01" to "9999999999.99", in the stash's currency (the account's).
    pub amount: String,
    /// An income category.
    pub category_id: String,
    /// Left out means today. Not after today and not before the maturity.
    #[serde(default)]
    pub date: Option<String>,
    /// At most 200 characters, line breaks allowed. Empty means none. It goes on both rows.
    #[serde(default)]
    pub note: Option<String>,
}

/// Books the return of a deposit that has matured and whose return is not booked: an income in its account (named as the stash), then an add of the
/// same amount, then `returned_on` = the date. The stash must not be archived and its account must be active. Every refusal is `InvalidInput`
/// (`NotFound` for an unknown stash) and leaves nothing written. Answers the stash, which is then booked.
pub fn book_return(conn: &Connection, stash_id: i64, input: &ReturnInput, today: &str) -> Result<Stash, AppError> {
    let today = clean_date(today)?;
    let minor = parse_positive_amount(&input.amount)?;
    let category = parse_id(&input.category_id)?;
    let date = match input.date.as_deref() {
        Some(date) => clean_date(date)?,
        None => today.clone(),
    };
    let note = clean_note(input.note.as_deref())?;
    let tx = conn.unchecked_transaction()?;
    let current = facts(&tx, stash_id)?;
    let deposit = current.deposit.as_ref().filter(|d| d.state(&today) == DepositState::Matured).ok_or(AppError::InvalidInput)?;
    if current.archived || current.account_archived || date > today || date < deposit.maturity || !category_is_income(&tx, category)? {
        return Err(AppError::InvalidInput);
    }
    let (amount, native_amount) = entry_amounts(&tx, current.account_id, minor)?;
    let income = NewTransaction {
        name: current.name.clone(),
        amount,
        payment_date: date.clone(),
        category_id: category.to_string(),
        description: note.clone(),
        client_uuid: None,
        account_id: Some(current.account_id.to_string()),
        native_amount,
    };
    transactions::create(&tx, &income)?;
    add_in(&tx, stash_id, &NewEntry { amount: input.amount.clone(), date: Some(date.clone()), note, from_account_id: None }, &today, true)?;
    tx.execute("UPDATE stashes SET returned_on = ?1 WHERE id = ?2", params![date, stash_id])?;
    let stash = stash(&tx, stash_id, &today)?;
    tx.commit()?;
    Ok(stash)
}
