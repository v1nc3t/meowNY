//! The messages of a sync, and the rows they carry (`docs/sync-design.md`, section 8).
//!
//! A message is one JSON object with a `t` field that fits one frame. Rows are named by `sync_id`: where a local row points
//! at another (a transaction at its category) the message carries the other row's `sync_id`, never the local integer id.
//!
//! Everything read here comes from the other device and is untrusted, even from a paired one. Every field is checked
//! with the rules the screens use (lengths, ranges, dates, allowed words), unknown fields are refused, and nothing is
//! trimmed or repaired: a row that is not already exactly what the app would have stored is refused.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::crypto::MAX_PLAINTEXT;
use super::stamp::Stamp;
use crate::money::MAX_MINOR;
use crate::{categories, transactions, vault};

/// The version of this protocol, in `hello`.
pub const PROTOCOL: u32 = 2;
/// Digest entries in one message, and rows in one message. Messages are also cut by size (`SOFT_MESSAGE_BYTES`).
pub const MAX_ENTRIES_PER_MESSAGE: usize = 400;
pub const MAX_ROWS_PER_MESSAGE: usize = 200;
/// A message under construction is closed when it passes this size, so every message fits one frame.
pub const SOFT_MESSAGE_BYTES: usize = 40 * 1024;
/// More than this many rows of one table in a sync is not a budget of one person.
pub const MAX_ROWS_PER_TABLE: usize = 2_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Table {
    Settings,
    Categories,
    RecurringTransactions,
    Budgets,
    Transactions,
}

impl Table {
    /// In the order they are sent and applied: a row's references always point at a table earlier in the list.
    pub const ALL: [Table; 5] = [Table::Settings, Table::Categories, Table::RecurringTransactions, Table::Budgets, Table::Transactions];

    pub fn name(self) -> &'static str {
        match self {
            Table::Settings => "settings",
            Table::Categories => "categories",
            Table::RecurringTransactions => "recurring_transactions",
            Table::Budgets => "budgets",
            Table::Transactions => "transactions",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// Not JSON of the right shape, or too big for a frame.
    Format,
    /// Something in it is outside the rules.
    Invalid(&'static str),
    /// A message that does not belong at this point of the conversation.
    Order,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Format => f.write_str("a message that could not be read"),
            WireError::Invalid(what) => write!(f, "a message with an invalid {what}"),
            WireError::Order => f.write_str("a message out of order"),
        }
    }
}

impl std::error::Error for WireError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Message {
    Hello(Hello),
    /// The phone's answer to a pairing: the person typed the six digits and they were right.
    Accept(AcceptMessage),
    /// The device that reviewed the conflicts has made its choices; the lists are compared again.
    Reviewed(ReviewedMessage),
    Currency(CurrencyMessage),
    Hashes(HashesMessage),
    Digest(DigestMessage),
    Rows(RowsMessage),
    Done(DoneMessage),
    Error(ErrorMessage),
}

impl Message {
    pub fn to_bytes(&self) -> Result<Vec<u8>, WireError> {
        let bytes = serde_json::to_vec(self).map_err(|_| WireError::Format)?;
        if bytes.len() > MAX_PLAINTEXT {
            return Err(WireError::Format);
        }
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Message, WireError> {
        if bytes.len() > MAX_PLAINTEXT {
            return Err(WireError::Format);
        }
        let message: Message = serde_json::from_slice(bytes).map_err(|_| WireError::Format)?;
        message.validate()?;
        Ok(message)
    }

    /// The shape checks that serde cannot do. Rows are checked as they are applied (`parse_row`).
    pub fn validate(&self) -> Result<(), WireError> {
        match self {
            Message::Hello(h) => h.validate(),
            Message::Accept(_) | Message::Reviewed(_) => Ok(()),
            Message::Currency(c) => {
                if c.changes.len() > 1000 {
                    return Err(WireError::Invalid("number of currency changes"));
                }
                c.changes.iter().try_for_each(CurrencyChange::validate)
            }
            Message::Hashes(h) => {
                let mut seen = std::collections::HashSet::new();
                for (table, hash) in &h.hashes {
                    if !seen.insert(*table) || !is_hash(hash) {
                        return Err(WireError::Invalid("hash"));
                    }
                }
                Ok(())
            }
            Message::Digest(d) => {
                if d.entries.len() > MAX_ENTRIES_PER_MESSAGE {
                    return Err(WireError::Invalid("number of entries"));
                }
                for (id, stamp, deleted) in &d.entries {
                    if !is_sync_id(id, d.table) || !Stamp::is_valid(stamp) || *deleted > 1 {
                        return Err(WireError::Invalid("entry"));
                    }
                }
                Ok(())
            }
            Message::Rows(r) => {
                if r.rows.len() > MAX_ROWS_PER_MESSAGE || r.deleted.len() > MAX_ENTRIES_PER_MESSAGE {
                    return Err(WireError::Invalid("number of rows"));
                }
                if r.table == Table::Settings && !r.deleted.is_empty() {
                    return Err(WireError::Invalid("marker"));
                }
                r.deleted.iter().try_for_each(|m| m.validate(r.table))
            }
            Message::Done(d) => {
                if is_hash(&d.hash) {
                    Ok(())
                } else {
                    Err(WireError::Invalid("hash"))
                }
            }
            Message::Error(e) => {
                if ERROR_CODES.contains(&e.code.as_str()) && e.message.chars().count() <= 200 && !e.message.chars().any(char::is_control) {
                    Ok(())
                } else {
                    Err(WireError::Invalid("error"))
                }
            }
        }
    }
}

pub const ERROR_CODES: &[&str] = &["schema_mismatch", "protocol_mismatch", "clock_far_off", "currency", "invalid", "busy", "cancelled", "internal"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hello {
    pub device_id: String,
    pub name: String,
    pub app_version: String,
    pub schema: u32,
    pub protocol: u32,
    /// The device's newest stamp, as its clock reads. Only its time is used.
    pub clock: String,
    pub currency: String,
    pub generation: i64,
    /// When this device last synced with the other one, in milliseconds since 1970 (none if it never did). Rows that both
    /// devices changed after both of their last syncs are conflicts.
    #[serde(default)]
    pub since: Option<u64>,
    /// Which way this sync goes. The desktop says; the phone's is ignored.
    #[serde(default)]
    pub mode: Mode,
}

/// Which way a sync goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Each takes what the other has newer.
    #[default]
    Both,
    /// The phone's changes go to the desktop and nothing goes back. The desktop reviews conflicts.
    ToDesktop,
    /// The desktop's changes go to the phone and nothing goes back. The phone reviews conflicts.
    ToPhone,
}

impl Hello {
    fn validate(&self) -> Result<(), WireError> {
        let name_ok = (1..=50).contains(&self.name.chars().count()) && !self.name.chars().any(char::is_control);
        let version_ok = (1..=32).contains(&self.app_version.len()) && self.app_version.bytes().all(|b| b.is_ascii_graphic());
        if !is_hex(&self.device_id, 32) || !name_ok || !version_ok || !Stamp::is_valid(&self.clock) {
            return Err(WireError::Invalid("hello"));
        }
        if !(self.currency.len() == 3 && self.currency.bytes().all(|b| b.is_ascii_uppercase())) || !(0..=1_000_000).contains(&self.generation) {
            return Err(WireError::Invalid("hello"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptMessage {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedMessage {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CurrencyMessage {
    pub changes: Vec<CurrencyChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CurrencyChange {
    pub generation: i64,
    pub from: String,
    pub to: String,
    /// `None` when the change only relabelled the amounts.
    pub rate: Option<String>,
    pub stamp: String,
}

impl CurrencyChange {
    fn validate(&self) -> Result<(), WireError> {
        let code = |c: &str| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase());
        // the rate's own rules (digits, length, above zero) are checked where it is used, by the currency module
        let rate_ok = self.rate.as_deref().is_none_or(|r| !r.is_empty() && r.len() <= 20 && r.bytes().all(|b| b.is_ascii_digit() || b == b'.'));
        if self.generation < 1 || !code(&self.from) || !code(&self.to) || !rate_ok || !Stamp::is_valid(&self.stamp) {
            return Err(WireError::Invalid("currency change"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HashesMessage {
    pub hashes: Vec<(Table, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DigestMessage {
    pub table: Table,
    /// `[sync_id, stamp, deleted]`, where `deleted` is 1 for a deletion marker.
    pub entries: Vec<(String, String, u8)>,
    pub last: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowsMessage {
    pub table: Table,
    pub rows: Vec<Value>,
    pub deleted: Vec<Marker>,
    pub last: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Marker {
    pub sync_id: String,
    pub stamp: String,
}

impl Marker {
    fn validate(&self, table: Table) -> Result<(), WireError> {
        if is_sync_id(&self.sync_id, table) && self.sync_id != "settings" && Stamp::is_valid(&self.stamp) {
            Ok(())
        } else {
            Err(WireError::Invalid("marker"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DoneMessage {
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorMessage {
    pub code: String,
    pub message: String,
}

fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_hash(text: &str) -> bool {
    is_hex(text, 64)
}

/// A `sync_id` is 32 lower-case hex characters. The settings row has no `sync_id`; the digest names it `settings`.
fn is_sync_id(text: &str, table: Table) -> bool {
    if table == Table::Settings {
        text == "settings"
    } else {
        is_hex(text, 32)
    }
}

pub fn sync_id_ok(text: &str) -> bool {
    is_hex(text, 32)
}

/// `YYYY-MM-DDTHH:MM:SS` with up to three decimals and `Z`, the form the database writes.
fn timestamp_ok(text: &str) -> bool {
    let b = text.as_bytes();
    if !text.is_ascii() || !(20..=24).contains(&b.len()) || b[b.len() - 1] != b'Z' {
        return false;
    }
    let digits = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18].iter().all(|&i| b[i].is_ascii_digit());
    let marks = b[4] == b'-' && b[7] == b'-' && b[10] == b'T' && b[13] == b':' && b[16] == b':';
    let fraction = match b.len() {
        20 => true,
        22..=24 => b[19] == b'.' && b[20..b.len() - 1].iter().all(u8::is_ascii_digit),
        _ => false,
    };
    digits && marks && fraction && chrono::DateTime::parse_from_rfc3339(text).is_ok()
}

fn date_ok(text: &str) -> bool {
    transactions::clean_date(text).is_ok()
}

fn first_of_month(text: &str) -> bool {
    date_ok(text) && text.ends_with("-01")
}

fn name_ok(text: &str) -> bool {
    transactions::clean_name(text).is_ok_and(|clean| clean == text)
}

fn category_name_ok(text: &str) -> bool {
    categories::clean_name(text).is_ok_and(|clean| clean == text)
}

fn description_ok(text: &Option<String>) -> bool {
    match text {
        None => true,
        Some(text) => transactions::clean_description(Some(text)).ok().flatten().as_deref() == Some(text.as_str()),
    }
}

fn money_ok(value: i64, min: i64) -> bool {
    (min..=MAX_MINOR).contains(&value)
}

fn option_sync_id(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(sync_id_ok)
}

fn option_date(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(date_ok)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsRow {
    pub stamp: String,
    pub locale: String,
    pub timezone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategoryRow {
    pub sync_id: String,
    pub stamp: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub deleted_at: Option<String>,
    pub merged_into: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecurringRow {
    pub sync_id: String,
    pub stamp: String,
    /// The `sync_id` of the category.
    pub category: String,
    pub name: String,
    pub description: Option<String>,
    pub amount_minor: Option<i64>,
    pub start_date: String,
    pub interval_unit: String,
    pub interval_count: i64,
    pub next_due_date: String,
    pub end_date: Option<String>,
    pub is_active: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BudgetRow {
    pub sync_id: String,
    pub stamp: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub scope: String,
    /// The `sync_id` of the category; none for the global budget.
    pub category: Option<String>,
    pub amount_limit_minor: Option<i64>,
    pub effective_from: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransactionRow {
    pub sync_id: String,
    pub stamp: String,
    pub category: String,
    /// The `sync_id` of the recurring item it was paid from, if there is one.
    pub recurring: Option<String>,
    pub recurring_due_date: Option<String>,
    pub client_uuid: Option<String>,
    pub name: String,
    pub amount_minor: i64,
    pub planned_amount_minor: Option<i64>,
    pub payment_date: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A row of one table, read and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Settings(SettingsRow),
    Category(CategoryRow),
    Recurring(RecurringRow),
    Budget(BudgetRow),
    Transaction(TransactionRow),
}

impl Row {
    pub fn sync_id(&self) -> &str {
        match self {
            Row::Settings(_) => "settings",
            Row::Category(r) => &r.sync_id,
            Row::Recurring(r) => &r.sync_id,
            Row::Budget(r) => &r.sync_id,
            Row::Transaction(r) => &r.sync_id,
        }
    }

    pub fn stamp(&self) -> &str {
        match self {
            Row::Settings(r) => &r.stamp,
            Row::Category(r) => &r.stamp,
            Row::Recurring(r) => &r.stamp,
            Row::Budget(r) => &r.stamp,
            Row::Transaction(r) => &r.stamp,
        }
    }
}

/// Reads one row of `table` from the JSON a peer sent, and checks every field.
pub fn parse_row(table: Table, value: &Value) -> Result<Row, WireError> {
    let bad = WireError::Invalid("row");
    let row = match table {
        Table::Settings => Row::Settings(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::Categories => Row::Category(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::RecurringTransactions => Row::Recurring(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::Budgets => Row::Budget(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::Transactions => Row::Transaction(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
    };
    if !Stamp::is_valid(row.stamp()) {
        return Err(bad);
    }
    let ok = match &row {
        Row::Settings(r) => {
            let probe = vault::Settings { currency: "EUR".into(), locale: r.locale.clone(), timezone: r.timezone.clone(), language: "en".into() };
            vault::validate_settings(&probe).is_ok_and(|clean| clean.locale == r.locale && clean.timezone == r.timezone)
        }
        Row::Category(r) => {
            sync_id_ok(&r.sync_id)
                && ["INCOME", "EXPENSE"].contains(&r.kind.as_str())
                && category_name_ok(&r.name)
                && r.deleted_at.as_deref().is_none_or(timestamp_ok)
                && option_sync_id(&r.merged_into)
                && r.merged_into.as_deref() != Some(r.sync_id.as_str())
                && timestamp_ok(&r.created_at)
                && timestamp_ok(&r.updated_at)
        }
        Row::Recurring(r) => {
            sync_id_ok(&r.sync_id)
                && sync_id_ok(&r.category)
                && name_ok(&r.name)
                && description_ok(&r.description)
                && r.amount_minor.is_none_or(|a| money_ok(a, 1))
                && date_ok(&r.start_date)
                && ["DAY", "WEEK", "MONTH", "YEAR"].contains(&r.interval_unit.as_str())
                && (1..=1000).contains(&r.interval_count)
                && date_ok(&r.next_due_date)
                && r.next_due_date >= r.start_date
                && option_date(&r.end_date)
                && r.end_date.as_deref().is_none_or(|end| end >= r.start_date.as_str())
                && [0, 1].contains(&r.is_active)
                && timestamp_ok(&r.created_at)
                && timestamp_ok(&r.updated_at)
        }
        Row::Budget(r) => {
            let scope_ok = match r.scope.as_str() {
                "GLOBAL" => r.category.is_none(),
                "CATEGORY" => r.category.is_some(),
                _ => false,
            };
            sync_id_ok(&r.sync_id)
                && ["INCOME", "EXPENSE"].contains(&r.kind.as_str())
                && scope_ok
                && option_sync_id(&r.category)
                && r.amount_limit_minor.is_none_or(|a| money_ok(a, 0))
                && first_of_month(&r.effective_from)
                && timestamp_ok(&r.created_at)
                && timestamp_ok(&r.updated_at)
        }
        Row::Transaction(r) => {
            sync_id_ok(&r.sync_id)
                && sync_id_ok(&r.category)
                && option_sync_id(&r.recurring)
                && option_date(&r.recurring_due_date)
                && (r.recurring.is_none() || r.recurring_due_date.is_some())
                && r.client_uuid.as_deref().is_none_or(|u| transactions::clean_uuid(u).is_ok_and(|clean| clean == u))
                && name_ok(&r.name)
                && money_ok(r.amount_minor, 1)
                && r.planned_amount_minor.is_none_or(|a| money_ok(a, 0))
                && date_ok(&r.payment_date)
                && description_ok(&r.description)
                && timestamp_ok(&r.created_at)
                && timestamp_ok(&r.updated_at)
        }
    };
    if ok {
        Ok(row)
    } else {
        Err(bad)
    }
}

/// Turns a row back into JSON. The inverse of `parse_row`.
pub fn row_to_value(row: &Row) -> Value {
    let value = match row {
        Row::Settings(r) => serde_json::to_value(r),
        Row::Category(r) => serde_json::to_value(r),
        Row::Recurring(r) => serde_json::to_value(r),
        Row::Budget(r) => serde_json::to_value(r),
        Row::Transaction(r) => serde_json::to_value(r),
    };
    value.expect("a row is always JSON")
}

/// What a peer's digest of one table says: `sync_id` to (stamp, is a deletion marker).
pub type Entries = BTreeMap<String, (String, bool)>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID_A: &str = "0123456789abcdef0123456789abcdef";
    const ID_B: &str = "fedcba9876543210fedcba9876543210";
    const STAMP: &str = "1760000000123-0001-0123abcd";
    const TS: &str = "2026-03-01T10:00:00.250Z";

    fn category() -> Value {
        json!({"syncId": ID_A, "stamp": STAMP, "type": "EXPENSE", "name": "Food", "deletedAt": null, "mergedInto": null, "createdAt": TS, "updatedAt": TS})
    }

    fn transaction() -> Value {
        json!({"syncId": ID_B, "stamp": STAMP, "category": ID_A, "recurring": null, "recurringDueDate": null, "clientUuid": null,
               "name": "Coffee", "amountMinor": 350, "plannedAmountMinor": null, "paymentDate": "2026-03-01", "description": null,
               "createdAt": TS, "updatedAt": TS})
    }

    fn recurring() -> Value {
        json!({"syncId": ID_B, "stamp": STAMP, "category": ID_A, "name": "Rent", "description": "Flat 4", "amountMinor": 90000,
               "startDate": "2026-01-01", "intervalUnit": "MONTH", "intervalCount": 1, "nextDueDate": "2026-03-01", "endDate": null,
               "isActive": 1, "createdAt": TS, "updatedAt": TS})
    }

    fn budget() -> Value {
        json!({"syncId": ID_B, "stamp": STAMP, "type": "EXPENSE", "scope": "CATEGORY", "category": ID_A, "amountLimitMinor": 50000,
               "effectiveFrom": "2026-03-01", "createdAt": TS, "updatedAt": TS})
    }

    fn changed(mut value: Value, key: &str, to: Value) -> Value {
        value[key] = to;
        value
    }

    #[test]
    fn good_rows_of_every_table_are_read_and_written_back_the_same() {
        let settings = json!({"stamp": STAMP, "locale": "en-IE", "timezone": "Europe/Dublin"});
        for (table, value) in [
            (Table::Settings, settings),
            (Table::Categories, category()),
            (Table::RecurringTransactions, recurring()),
            (Table::Budgets, budget()),
            (Table::Transactions, transaction()),
        ] {
            let row = parse_row(table, &value).unwrap_or_else(|e| panic!("{table:?}: {e}"));
            assert_eq!(row_to_value(&row), value, "{table:?}");
        }
    }

    #[test]
    fn a_row_with_a_field_too_many_or_too_few_is_refused() {
        let mut extra = category();
        extra["note"] = json!("x");
        assert!(parse_row(Table::Categories, &extra).is_err());
        let mut missing = category();
        missing.as_object_mut().unwrap().remove("name");
        assert!(parse_row(Table::Categories, &missing).is_err());
        assert!(parse_row(Table::Categories, &json!([])).is_err());
        assert!(parse_row(Table::Categories, &json!(null)).is_err());
        assert!(parse_row(Table::Transactions, &category()).is_err(), "a row of another table");
    }

    #[test]
    fn values_outside_the_rules_of_the_screens_are_refused() {
        let cases: Vec<(Table, Value)> = vec![
            (Table::Categories, changed(category(), "syncId", json!("ABCDEF"))),
            (Table::Categories, changed(category(), "syncId", json!(ID_A.to_uppercase()))),
            (Table::Categories, changed(category(), "stamp", json!("nope"))),
            (Table::Categories, changed(category(), "type", json!("OTHER"))),
            (Table::Categories, changed(category(), "name", json!(""))),
            (Table::Categories, changed(category(), "name", json!(" Food"))),
            (Table::Categories, changed(category(), "name", json!("a".repeat(51)))),
            (Table::Categories, changed(category(), "name", json!("tab\there"))),
            (Table::Categories, changed(category(), "mergedInto", json!(ID_A))),
            (Table::Categories, changed(category(), "createdAt", json!("2026-03-01 10:00:00"))),
            (Table::Categories, changed(category(), "updatedAt", json!("2026-13-01T10:00:00.250Z"))),
            (Table::Categories, changed(category(), "deletedAt", json!("yesterday"))),
            (Table::Transactions, changed(transaction(), "amountMinor", json!(0))),
            (Table::Transactions, changed(transaction(), "amountMinor", json!(-5))),
            (Table::Transactions, changed(transaction(), "amountMinor", json!(MAX_MINOR + 1))),
            (Table::Transactions, changed(transaction(), "amountMinor", json!(1.5))),
            (Table::Transactions, changed(transaction(), "amountMinor", json!("350"))),
            (Table::Transactions, changed(transaction(), "plannedAmountMinor", json!(-1))),
            (Table::Transactions, changed(transaction(), "paymentDate", json!("2026-3-1"))),
            (Table::Transactions, changed(transaction(), "paymentDate", json!("2026-02-30"))),
            (Table::Transactions, changed(transaction(), "description", json!("x".repeat(256)))),
            (Table::Transactions, changed(transaction(), "description", json!(""))),
            (Table::Transactions, changed(transaction(), "description", json!("line\nbreak"))),
            (Table::Transactions, changed(transaction(), "clientUuid", json!("0B8F6C1E-5C52-4D6A-9D50-3F7A8A1B2C3D"))),
            (Table::Transactions, changed(transaction(), "clientUuid", json!("not-a-uuid"))),
            (Table::Transactions, changed(transaction(), "recurring", json!(ID_A))),
            (Table::Transactions, changed(transaction(), "category", json!("1"))),
            (Table::RecurringTransactions, changed(recurring(), "intervalUnit", json!("HOUR"))),
            (Table::RecurringTransactions, changed(recurring(), "intervalCount", json!(0))),
            (Table::RecurringTransactions, changed(recurring(), "intervalCount", json!(1001))),
            (Table::RecurringTransactions, changed(recurring(), "isActive", json!(2))),
            (Table::RecurringTransactions, changed(recurring(), "nextDueDate", json!("2025-12-31"))),
            (Table::RecurringTransactions, changed(recurring(), "endDate", json!("2025-12-31"))),
            (Table::RecurringTransactions, changed(recurring(), "amountMinor", json!(0))),
            (Table::Budgets, changed(budget(), "scope", json!("GLOBAL"))),
            (Table::Budgets, changed(budget(), "category", json!(null))),
            (Table::Budgets, changed(budget(), "effectiveFrom", json!("2026-03-15"))),
            (Table::Budgets, changed(budget(), "amountLimitMinor", json!(-1))),
            (Table::Budgets, changed(budget(), "type", json!("x"))),
            (Table::Settings, json!({"stamp": STAMP, "locale": "x", "timezone": "Europe/Dublin"})),
            (Table::Settings, json!({"stamp": STAMP, "locale": "en-IE", "timezone": "Mars/Base"})),
            (Table::Settings, json!({"stamp": STAMP, "locale": " en-IE", "timezone": "UTC"})),
            (Table::Settings, json!({"stamp": STAMP, "locale": "en-IE", "timezone": "UTC", "currency": "EUR"})),
        ];
        for (table, value) in cases {
            assert!(parse_row(table, &value).is_err(), "{table:?} {value}");
        }
        // and the global budget has no category
        let global = json!({"syncId": ID_B, "stamp": STAMP, "type": "INCOME", "scope": "GLOBAL", "category": null, "amountLimitMinor": null,
                            "effectiveFrom": "2026-03-01", "createdAt": TS, "updatedAt": TS});
        assert!(parse_row(Table::Budgets, &global).is_ok(), "a removed budget has no limit");
    }

    #[test]
    fn timestamps_are_read_strictly() {
        for good in ["2026-03-01T10:00:00Z", "2026-03-01T10:00:00.2Z", "2026-03-01T10:00:00.25Z", "2026-03-01T10:00:00.250Z"] {
            assert!(timestamp_ok(good), "{good}");
        }
        for bad in [
            "", "2026-03-01T10:00:00", "2026-03-01T10:00:00.Z", "2026-03-01T10:00:00.2501Z", "2026-03-01t10:00:00Z", "2026-03-01T10:00:00z",
            "2026-03-01T10:00:00+00:00", "2026-03-01T25:00:00Z", "2026-03-01T10:60:00Z", "2026-02-30T10:00:00Z", " 2026-03-01T10:00:00Z",
            "2026-03-01T10:00:00Z ", "2026-03-01T10:00:00.2xZ",
        ] {
            assert!(!timestamp_ok(bad), "{bad:?}");
        }
    }

    #[test]
    fn messages_are_read_and_written_as_json_with_a_tag() {
        let hello = Message::Hello(Hello {
            device_id: ID_A.into(),
            name: "Vincent's phone".into(),
            app_version: "0.1.0".into(),
            schema: 8,
            protocol: PROTOCOL,
            clock: STAMP.into(),
            currency: "EUR".into(),
            generation: 2,
            since: Some(1_700_000_000_000),
            mode: Mode::ToPhone,
        });
        let bytes = hello.to_bytes().unwrap();
        assert!(std::str::from_utf8(&bytes).unwrap().starts_with("{\"t\":\"hello\""));
        match Message::from_bytes(&bytes).unwrap() {
            Message::Hello(h) => assert_eq!((h.schema, h.generation, h.name.as_str(), h.since, h.mode), (8, 2, "Vincent's phone", Some(1_700_000_000_000), Mode::ToPhone)),
            other => panic!("{other:?}"),
        }
        let accept = Message::Accept(AcceptMessage {});
        assert_eq!(accept.to_bytes().unwrap(), br#"{"t":"accept"}"#);
        assert!(Message::from_bytes(br#"{"t":"accept"}"#).is_ok());
        assert!(Message::from_bytes(br#"{"t":"accept","extra":1}"#).is_err());
        let digest = Message::Digest(DigestMessage { table: Table::Transactions, entries: vec![(ID_A.into(), STAMP.into(), 0), (ID_B.into(), STAMP.into(), 1)], last: true });
        assert!(Message::from_bytes(&digest.to_bytes().unwrap()).is_ok());
        let rows = Message::Rows(RowsMessage { table: Table::Categories, rows: vec![category()], deleted: vec![], last: true });
        assert!(Message::from_bytes(&rows.to_bytes().unwrap()).is_ok());
    }

    #[test]
    fn messages_with_unknown_tags_extra_fields_or_bad_values_are_refused() {
        for bad in [
            r#"{"t":"nothing"}"#,
            r#"{"t":"done"}"#,
            r#"{"t":"done","hash":"zz"}"#,
            r#"{"t":"done","hash":"00","extra":1}"#,
            r#"{"hello":1}"#,
            r#"[]"#,
            r#""hello""#,
            r#"{"t":"digest","table":"nope","entries":[],"last":true}"#,
            r#"{"t":"digest","table":"transactions","entries":[["bad","1760000000123-0001-0123abcd",0]],"last":true}"#,
            r#"{"t":"digest","table":"transactions","entries":[["0123456789abcdef0123456789abcdef","bad",0]],"last":true}"#,
            r#"{"t":"digest","table":"transactions","entries":[["0123456789abcdef0123456789abcdef","1760000000123-0001-0123abcd",2]],"last":true}"#,
            r#"{"t":"digest","table":"settings","entries":[["0123456789abcdef0123456789abcdef","1760000000123-0001-0123abcd",0]],"last":true}"#,
            r#"{"t":"rows","table":"settings","rows":[],"deleted":[{"syncId":"settings","stamp":"1760000000123-0001-0123abcd"}],"last":true}"#,
            r#"{"t":"error","code":"made-up","message":"x"}"#,
            r#"{"t":"error","code":"busy","message":"a\nb"}"#,
            r#"{"t":"hashes","hashes":[["transactions","00"]]}"#,
        ] {
            assert!(Message::from_bytes(bad.as_bytes()).is_err(), "{bad}");
        }
        assert!(Message::from_bytes(&vec![b' '; MAX_PLAINTEXT + 1]).is_err());
        assert!(Message::from_bytes(&[0xff, 0xfe]).is_err());
    }

    #[test]
    fn a_digest_names_the_settings_row_settings_and_no_other_table_may() {
        let ok = format!(r#"{{"t":"digest","table":"settings","entries":[["settings","{STAMP}",0]],"last":true}}"#);
        assert!(Message::from_bytes(ok.as_bytes()).is_ok());
    }

    #[test]
    fn a_message_too_big_for_a_frame_is_not_made() {
        let rows = (0..MAX_ROWS_PER_MESSAGE).map(|_| json!({"x": "y".repeat(400)})).collect();
        let big = Message::Rows(RowsMessage { table: Table::Categories, rows, deleted: vec![], last: true });
        assert_eq!(big.to_bytes().err(), Some(WireError::Format));
    }

    #[test]
    fn a_hello_without_since_and_mode_means_never_met_and_both_ways() {
        let text = format!(
            r#"{{"t":"hello","deviceId":"{ID_A}","name":"Phone","appVersion":"0.1.0","schema":8,"protocol":{PROTOCOL},"clock":"{STAMP}","currency":"EUR","generation":0}}"#
        );
        match Message::from_bytes(text.as_bytes()).unwrap() {
            Message::Hello(h) => assert_eq!((h.since, h.mode), (None, Mode::Both)),
            other => panic!("{other:?}"),
        }
        let odd = text.replace("\"generation\":0", "\"generation\":0,\"mode\":\"sideways\"");
        assert!(Message::from_bytes(odd.as_bytes()).is_err(), "a way that does not exist is refused");
    }

    #[test]
    fn the_messages_of_a_review_are_small_and_strict() {
        assert_eq!(Message::Reviewed(ReviewedMessage {}).to_bytes().unwrap(), br#"{"t":"reviewed"}"#);
        assert!(Message::from_bytes(br#"{"t":"reviewed"}"#).is_ok());
        assert!(Message::from_bytes(br#"{"t":"reviewed","choices":[]}"#).is_err());
    }

    #[test]
    fn a_hello_from_a_stranger_is_checked_field_by_field() {
        let good = Hello {
            device_id: ID_A.into(),
            name: "Phone".into(),
            app_version: "0.1.0".into(),
            schema: 8,
            protocol: PROTOCOL,
            clock: STAMP.into(),
            currency: "EUR".into(),
            generation: 0,
            since: None,
            mode: Mode::Both,
        };
        assert!(Message::Hello(good.clone()).validate().is_ok());
        let bad = |f: &dyn Fn(&mut Hello)| {
            let mut h = good.clone();
            f(&mut h);
            Message::Hello(h).validate().is_err()
        };
        assert!(bad(&|h| h.device_id = "x".into()));
        assert!(bad(&|h| h.name = String::new()));
        assert!(bad(&|h| h.name = "x".repeat(51)));
        assert!(bad(&|h| h.name = "a\u{7}b".into()));
        assert!(bad(&|h| h.app_version = "a b".into()));
        assert!(bad(&|h| h.clock = "no".into()));
        assert!(bad(&|h| h.currency = "eur".into()));
        assert!(bad(&|h| h.generation = -1));
    }
}
