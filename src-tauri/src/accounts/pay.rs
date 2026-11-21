//! Pay a liability (`docs/loans-deposits-design.md`, section 5.4): one payment of a credit, a loan or a mortgage, split in two. The interest
//! part is an expense from the paying account, and the rest, the principal, is a transfer from it to the liability that lowers the debt. Both
//! are made in one database transaction, so a refusal writes neither, and both are ordinary rows afterwards: editing or deleting one does not
//! touch the other.
//!
//! Nothing is checked twice: the transfer and the expense are made by `transfers::create` and `transactions::create`, which keep their own rules.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::transfers::{self, NewTransfer, Transfer};
use super::{currencies_differ, is_active, to_main, Kind};
use crate::categories::parse_id;
use crate::debts::ledger::clean_note;
use crate::money::{format_amount, parse_amount, parse_positive_amount};
use crate::transactions::{self, category_is_expense, clean_date, NewTransaction, Transaction};
use crate::vault::AppError;

/// One payment, as the page sends it: `{ "liabilityId": "3", "fromAccountId": "1", "amount": "950.00", "interest": "585.00", "categoryId": "7",
/// "date": "2026-11-20", "note": "November" }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Payment {
    /// The account that is paid: a credit, a loan or a mortgage that is not archived.
    pub liability_id: String,
    /// The account the money comes from. It must not be archived, must not be the liability and must be in the liability's currency.
    pub from_account_id: String,
    /// The whole payment, interest included: "0.01" to "9999999999.99".
    pub amount: String,
    /// The part of `amount` that is interest, "0.00" to `amount`. Left out means "0.00".
    #[serde(default)]
    pub interest: Option<String>,
    /// An expense category, needed when the interest is above 0 and not looked at otherwise.
    #[serde(default)]
    pub category_id: Option<String>,
    /// Left out means today. A real date, as the date of a transfer.
    #[serde(default)]
    pub date: Option<String>,
    /// At most 200 characters, line breaks allowed. Empty means none. It goes on both rows.
    #[serde(default)]
    pub note: Option<String>,
}

/// What a payment made: `transfer` is the principal (none when the whole payment was interest), `transaction` the expense of the interest (none
/// when there was none). JSON: `{ "transfer": { ... } | null, "transaction": { ... } | null }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Paid {
    pub transfer: Option<Transfer>,
    pub transaction: Option<Transaction>,
}

/// The `amount` and `native_amount` that `transactions::create` is given for `minor`, an amount in the currency of `account`. An account in another
/// currency has its rows in that currency: the entry has what moved it as `nativeAmount`, and as `amount` what that is worth in the app's currency at
/// the account's rate, which is what the entry dialog suggests (never below a cent). An account in the app's currency has no native amount.
pub(crate) fn entry_amounts(conn: &Connection, account: i64, minor: i64) -> Result<(String, Option<String>), AppError> {
    let rate: Option<String> = conn.query_row("SELECT rate FROM accounts WHERE id = ?1", [account], |r| r.get(0))?;
    Ok(match rate {
        Some(rate) => (format_amount(to_main(minor, &rate)?.max(1)), Some(format_amount(minor))),
        None => (format_amount(minor), None),
    })
}

/// Makes the payment. A liability that does not exist is `NotFound`; every other refusal is `InvalidInput` and leaves nothing written.
pub fn pay(conn: &Connection, input: &Payment, today: &str) -> Result<Paid, AppError> {
    let today = clean_date(today)?;
    let liability = parse_id(&input.liability_id)?;
    let tx = conn.unchecked_transaction()?;
    let (kind, archived, name): (String, bool, String) = tx
        .query_row("SELECT kind, archived, name FROM accounts WHERE id = ?1", [liability], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?
        .ok_or(AppError::NotFound)?;
    if !Kind::from_sql(&kind)?.is_liability() || archived {
        return Err(AppError::InvalidInput);
    }
    let from = parse_id(&input.from_account_id)?;
    if from == liability || !is_active(&tx, from)? || currencies_differ(&tx, liability, from)? {
        return Err(AppError::InvalidInput);
    }
    let amount = parse_positive_amount(&input.amount)?;
    let interest = input.interest.as_deref().map(parse_amount).transpose()?.unwrap_or(0);
    if interest > amount {
        return Err(AppError::InvalidInput);
    }
    let date = match input.date.as_deref() {
        Some(date) => clean_date(date)?,
        None => today,
    };
    let note = clean_note(input.note.as_deref())?;
    // the category counts only when there is interest to book (section 5.4)
    let category = match interest {
        0 => None,
        _ => {
            let id = parse_id(input.category_id.as_deref().ok_or(AppError::InvalidInput)?)?;
            if !category_is_expense(&tx, id)? {
                return Err(AppError::InvalidInput);
            }
            Some(id)
        }
    };

    let principal = amount - interest;
    let transfer = if principal > 0 {
        let input = NewTransfer {
            from_account_id: from.to_string(),
            to_account_id: liability.to_string(),
            amount: format_amount(principal),
            date: date.clone(),
            note: note.clone(),
            to_amount: None,
        };
        Some(transfers::create(&tx, &input)?)
    } else {
        None
    };
    let transaction = match category {
        Some(category) => {
            let (amount, native_amount) = entry_amounts(&tx, from, interest)?;
            let input = NewTransaction {
                name,
                amount,
                payment_date: date,
                category_id: category.to_string(),
                description: note,
                client_uuid: None,
                account_id: Some(from.to_string()),
                native_amount,
            };
            Some(transactions::create(&tx, &input)?)
        }
        None => None,
    };
    tx.commit()?;
    Ok(Paid { transfer, transaction })
}
