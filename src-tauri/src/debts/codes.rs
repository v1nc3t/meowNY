//! The bill code (`docs/debts-design.md`, section 10): one friend's part of one bill as a line of text, which the app shows as a QR code
//! and a friend with the app scans (phone) or pastes (desktop) and then accepts or denies.
//!
//! It is a notice and nothing more. It is not signed or encrypted, it names nobody but the sender, and it holds no other person's part.
//! Reading one writes nothing (`preview`); taking one in (`accept`) makes the friend, the expense and the `BORROWED` entry together, or
//! nothing, and the same code is never taken in twice (its id is the `origin_id` of the entry). The one strict parser (`parse`) is the
//! only way in, and it is run on text from outside, so it checks everything and allocates from nothing the text says.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::ledger::{self, Entry, Expense, Kind, NewEntry};
use super::people;
use crate::categories::parse_id;
use crate::currency;
use crate::export;
use crate::money::{format_amount, parse_positive_amount, MAX_MINOR};
use crate::sync::qr::{base64url_decode, base64url_encode};
use crate::transactions::clean_date;
use crate::vault::AppError;

/// The start of the text, which tells a bill code from a pairing code (`MNY1:`).
pub const PREFIX: &str = "MNB1:";
/// The most the text may be, in bytes. A real code is under 300.
pub const MAX_CODE_BYTES: usize = 1024;
const NAME_MAX_CHARS: usize = 50;
const WHAT_MAX_CHARS: usize = 60;
/// The longest a name the app makes for the expense of a bill is: the rule of a transaction's name.
const EXPENSE_NAME_MAX_CHARS: usize = 50;

/// The JSON of a code, field for field. Unknown fields are refused.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    v: u32,
    id: String,
    from: String,
    what: String,
    date: String,
    amt: i64,
    total: i64,
    cur: String,
}

/// What a code says, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillCode {
    /// 32 lowercase hex characters: the same for the same bill and friend, whenever the code is shown.
    pub id: String,
    /// The name the sender gave.
    pub from: String,
    /// The name of the bill.
    pub what: String,
    pub date: String,
    /// The friend's part, in minor units of `currency`.
    pub amount_minor: i64,
    /// The whole bill.
    pub total_minor: i64,
    pub currency: String,
}

/// A character that is shown as itself. Control characters are refused as in every name, and so are the invisible ones that change how text
/// is laid out (direction marks and overrides, zero-width characters), which could make a name read as something else.
fn shown_as_itself(c: char) -> bool {
    !(c.is_control() || matches!(c, '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}' | '\u{fff9}'..='\u{fffb}'))
}

/// Plain text: not empty, no longer than `max` characters, no space at either end, every character shown as itself.
fn plain(text: &str, max: usize) -> bool {
    let length = text.chars().count();
    length >= 1 && length <= max && text.trim() == text && text.chars().all(shown_as_itself)
}

/// Reads the text of a code, strictly: the size, the prefix, the alphabet, the fields, their lengths, the date, the amounts and the currency.
/// Anything else is `InvalidInput`.
pub fn parse(text: &str) -> Result<BillCode, AppError> {
    let text = text.trim();
    if text.len() > MAX_CODE_BYTES {
        return Err(AppError::InvalidInput);
    }
    let body = text.strip_prefix(PREFIX).ok_or(AppError::InvalidInput)?;
    let bytes = base64url_decode(body).ok_or(AppError::InvalidInput)?;
    let wire: Wire = serde_json::from_slice(&bytes).map_err(|_| AppError::InvalidInput)?;
    let id_ok = wire.id.len() == 32 && wire.id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    let currency_ok = wire.cur.len() == 3 && wire.cur.bytes().all(|b| b.is_ascii_uppercase());
    if wire.v != 1
        || !id_ok
        || !plain(&wire.from, NAME_MAX_CHARS)
        || !plain(&wire.what, WHAT_MAX_CHARS)
        || clean_date(&wire.date).is_err()
        || !(1..=MAX_MINOR).contains(&wire.amt)
        || !(wire.amt..=MAX_MINOR).contains(&wire.total)
        || !currency_ok
    {
        return Err(AppError::InvalidInput);
    }
    Ok(BillCode { id: wire.id, from: wire.from, what: wire.what, date: wire.date, amount_minor: wire.amt, total_minor: wire.total, currency: wire.cur })
}

/// The text of a code.
pub fn encode(code: &BillCode) -> String {
    let wire = Wire {
        v: 1,
        id: code.id.clone(),
        from: code.from.clone(),
        what: code.what.clone(),
        date: code.date.clone(),
        amt: code.amount_minor,
        total: code.total_minor,
        cur: code.currency.clone(),
    };
    // a struct of numbers and strings always serializes
    let json = serde_json::to_vec(&wire).unwrap_or_default();
    format!("{PREFIX}{}", base64url_encode(&json))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The code of one friend's part of one bill, in the name the person gives for the sender. The same text every time for the same bill and
/// friend. The friend must be one of the parts of the bill (a guest has no code: they have no app to give it to).
pub fn make(conn: &Connection, transaction_id: i64, person_id: i64, from: &str) -> Result<String, AppError> {
    let from = from.trim();
    if !plain(from, NAME_MAX_CHARS) {
        return Err(AppError::InvalidInput);
    }
    let (bill_sync, name, date, total): (String, String, String, i64) = conn
        .query_row("SELECT sync_id, name, payment_date, amount_minor FROM transactions WHERE id = ?1", [transaction_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()?
        .ok_or(AppError::NotFound)?;
    let part: i64 = conn
        .query_row("SELECT amount_minor FROM transaction_split_lines WHERE transaction_id = ?1 AND person_id = ?2", [transaction_id, person_id], |r| r.get(0))
        .optional()?
        .ok_or(AppError::NotFound)?;
    let person_sync: String = conn.query_row("SELECT sync_id FROM people WHERE id = ?1", [person_id], |r| r.get(0))?;
    let mut hash = Sha256::new();
    hash.update(b"meowny-bill-1");
    hash.update(bill_sync.as_bytes());
    hash.update(person_sync.as_bytes());
    let id = hex(&hash.finalize()[..16]);
    // the name of a bill is a transaction's: at most 50 characters, and never something `parse` would refuse
    let what: String = name.chars().filter(|c| shown_as_itself(*c)).take(WHAT_MAX_CHARS).collect();
    let what = what.trim().to_string();
    if what.is_empty() {
        return Err(AppError::InvalidInput);
    }
    let currency = export::currency(conn)?;
    let code = BillCode { id, from: from.to_string(), what, date, amount_minor: part, total_minor: total, currency };
    // what is made is what `parse` takes
    parse(&encode(&code))?;
    Ok(encode(&code))
}

/// What the dialog shows for a code. Nothing is written to make it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub id: String,
    pub from: String,
    pub what: String,
    pub date: String,
    /// The friend's part, in the currency of the code.
    pub amount: String,
    pub total: String,
    pub currency: String,
    /// The currency of this app.
    pub own_currency: String,
    pub same_currency: bool,
    /// An entry that came from this code is here already: it is not taken in twice.
    pub already_added: bool,
    /// The friend, not removed from the list, whose name is the sender's (ignoring case), to choose first.
    pub friend_id: Option<String>,
}

fn already_added(conn: &Connection, id: &str) -> Result<bool, AppError> {
    Ok(conn.query_row("SELECT 1 FROM ledger_entries WHERE origin_id = ?1", [id], |_| Ok(())).optional()?.is_some())
}

/// Reads a code and says what it holds, for the person to decide. Writes nothing.
pub fn preview(conn: &Connection, text: &str) -> Result<Preview, AppError> {
    let code = parse(text)?;
    let own = export::currency(conn)?;
    let friend_id: Option<i64> = conn
        .query_row("SELECT id FROM people WHERE name = ?1 COLLATE NOCASE AND archived = 0 AND merged_into IS NULL ORDER BY id LIMIT 1", [&code.from], |r| r.get(0))
        .optional()?;
    Ok(Preview {
        already_added: already_added(conn, &code.id)?,
        friend_id: friend_id.map(|id| id.to_string()),
        same_currency: code.currency == own,
        own_currency: own,
        amount: format_amount(code.amount_minor),
        total: format_amount(code.total_minor),
        id: code.id,
        from: code.from,
        what: code.what,
        date: code.date,
        currency: code.currency,
    })
}

/// What the person chose in the dialog.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Accept {
    pub text: String,
    /// The friend the debt goes to, or...
    #[serde(default)]
    pub person_id: Option<String>,
    /// ...the name of a new one. Exactly one of the two.
    #[serde(default)]
    pub new_friend: Option<String>,
    /// Also add the part to the person's expenses.
    pub add_expense: bool,
    /// The expense category, when `add_expense`.
    #[serde(default)]
    pub category_id: Option<String>,
    /// For a code in another currency: how much of this app's currency one of the code's currency is worth. The person types it; nothing is fetched.
    #[serde(default)]
    pub rate: Option<String>,
    /// For a code in another currency: the converted amount, when the person changed what the rate gives.
    #[serde(default)]
    pub amount: Option<String>,
}

/// Takes a code in: the friend (made if new), the expense (when asked) and the `BORROWED` entry with the code's id, linked to the expense,
/// in one database transaction. In another currency the part is converted at the typed rate (or the amount typed instead) and the note
/// says how.
pub fn accept(conn: &Connection, input: &Accept) -> Result<Entry, AppError> {
    let code = parse(&input.text)?;
    let own = export::currency(conn)?;
    let tx = conn.unchecked_transaction()?;
    if already_added(&tx, &code.id)? {
        return Err(AppError::InvalidInput);
    }
    let (minor, note) = if code.currency == own {
        if input.rate.is_some() || input.amount.is_some() {
            return Err(AppError::InvalidInput);
        }
        (code.amount_minor, code.what.clone())
    } else {
        let rate = input.rate.as_deref().ok_or(AppError::InvalidInput)?;
        let converted = currency::convert_at(code.amount_minor, rate)?;
        let minor = match &input.amount {
            Some(typed) => parse_positive_amount(typed)?,
            None => converted,
        };
        if minor < 1 {
            return Err(AppError::InvalidInput);
        }
        (minor, format!("{}: {} {} at {rate}", code.what, format_amount(code.amount_minor), code.currency))
    };
    let person_id = match (&input.person_id, &input.new_friend) {
        (Some(id), None) => {
            let id = parse_id(id)?;
            let found = people::find(&tx, id)?.ok_or(AppError::InvalidInput)?;
            if found.archived {
                return Err(AppError::InvalidInput);
            }
            id
        }
        (None, Some(name)) => people::create(&tx, name)?.id,
        _ => return Err(AppError::InvalidInput),
    };
    let entry_input = NewEntry {
        person_id: person_id.to_string(),
        kind: Kind::Borrowed,
        amount: format_amount(minor),
        date: code.date.clone(),
        note: Some(note.chars().take(200).collect()),
        transaction_id: None,
    };
    let entry = if input.add_expense {
        let category_id = input.category_id.clone().ok_or(AppError::InvalidInput)?;
        let name: String = code.what.chars().take(EXPENSE_NAME_MAX_CHARS).collect();
        ledger::borrow_in(&tx, &entry_input, &Expense { name, category_id }, Some(&code.id))?
    } else {
        ledger::add_from(&tx, &entry_input, Some(&code.id))?
    };
    tx.commit()?;
    Ok(entry)
}
