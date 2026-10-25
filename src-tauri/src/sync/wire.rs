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
use crate::documents::names::{is_clean_name, MediaType};
use crate::documents::seal::CHUNK_SIZE;
use crate::documents::MAX_DOCUMENT_BYTES;
use crate::money::MAX_MINOR;
use crate::{categories, transactions, vault};

/// The version of this protocol, in `hello`.
/// Protocol 3 adds the tables `documents` and `document_links`; protocol 4 adds `people` and `ledger_entries`, the parts of a shared
/// bill inside its transaction's row, the split template inside a recurring item's row, and a third target for a document link (a
/// ledger entry); protocol 5 adds the category order (`position`) and the skipped dates of a recurring item. A device that speaks an older protocol is told to update.
pub const PROTOCOL: u32 = 5;
/// Digest entries in one message, and rows in one message. Messages are also cut by size (`SOFT_MESSAGE_BYTES`).
pub const MAX_ENTRIES_PER_MESSAGE: usize = 400;
pub const MAX_ROWS_PER_MESSAGE: usize = 200;
/// A message under construction is closed when it passes this size, so every message fits one frame.
pub const SOFT_MESSAGE_BYTES: usize = 40 * 1024;
/// Bytes of a file in one `chunk` message. As base64url text they are `MAX_CHUNK_TEXT` characters, which with the rest of the
/// message fits one frame.
pub const CHUNK_BYTES: usize = 24 * 1024;
pub const MAX_CHUNK_TEXT: usize = CHUNK_BYTES / 3 * 4;
/// More than this many rows of one table in a sync is not a budget of one person.
pub const MAX_ROWS_PER_TABLE: usize = 2_000_000;
/// The highest place a category can have in its list.
const MAX_POSITION: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Table {
    Settings,
    Categories,
    People,
    RecurringTransactions,
    Budgets,
    Transactions,
    LedgerEntries,
    Documents,
    DocumentLinks,
}

impl Table {
    /// In the order they are sent and applied: a row's references always point at a table earlier in the list.
    pub const ALL: [Table; 9] = [
        Table::Settings,
        Table::Categories,
        Table::People,
        Table::RecurringTransactions,
        Table::Budgets,
        Table::Transactions,
        Table::LedgerEntries,
        Table::Documents,
        Table::DocumentLinks,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Table::Settings => "settings",
            Table::Categories => "categories",
            Table::People => "people",
            Table::LedgerEntries => "ledger_entries",
            Table::RecurringTransactions => "recurring_transactions",
            Table::Budgets => "budgets",
            Table::Transactions => "transactions",
            Table::Documents => "documents",
            Table::DocumentLinks => "document_links",
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
    /// The documents whose files this device holds.
    Have(HaveMessage),
    /// The files this device wants, and from where in each.
    Need(NeedMessage),
    /// The start of one file, or the word that it will not be sent.
    File(FileMessage),
    /// A piece of the file that was just announced.
    Chunk(ChunkMessage),
    /// The documents whose files the sender now holds, whole and checked.
    Stored(StoredMessage),
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
            Message::Have(m) => ids_ok(&m.ids),
            Message::Stored(m) => ids_ok(&m.ids),
            Message::Need(m) => {
                if m.files.len() > MAX_ENTRIES_PER_MESSAGE || !m.files.iter().all(|(id, offset)| sync_id_ok(id) && offset_ok(*offset)) {
                    return Err(WireError::Invalid("request for files"));
                }
                Ok(())
            }
            Message::File(m) => {
                let ok = if m.skipped {
                    m.offset == 0 && m.size == 0
                } else {
                    (1..=MAX_DOCUMENT_BYTES).contains(&m.size) && m.offset < m.size && offset_ok(m.offset)
                };
                if sync_id_ok(&m.id) && ok {
                    Ok(())
                } else {
                    Err(WireError::Invalid("file"))
                }
            }
            Message::Chunk(m) => {
                if !m.data.is_empty() && m.data.len() <= MAX_CHUNK_TEXT && m.data.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
                    Ok(())
                } else {
                    Err(WireError::Invalid("piece of a file"))
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

/// `ids` are `sync_id`s of documents. A list longer than one message is sent in several, the last marked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HaveMessage {
    pub ids: Vec<String>,
    pub last: bool,
}

/// `[sync_id, offset]`: the sender of this message has the first `offset` bytes of the file already (a whole number of sealed
/// chunks) and wants the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NeedMessage {
    pub files: Vec<(String, u64)>,
    pub last: bool,
}

/// What comes before the pieces of a file: which, how big it is in all, and where the pieces start. `skipped` says there will be
/// no pieces, because the file is not here or is not for the one who asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileMessage {
    pub id: String,
    pub offset: u64,
    pub size: u64,
    #[serde(default)]
    pub skipped: bool,
}

/// Up to `CHUNK_BYTES` of the file in base64url; `last` on the final one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChunkMessage {
    pub data: String,
    pub last: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredMessage {
    pub ids: Vec<String>,
    pub last: bool,
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

fn ids_ok(ids: &[String]) -> Result<(), WireError> {
    if ids.len() <= MAX_ENTRIES_PER_MESSAGE && ids.iter().all(|id| sync_id_ok(id)) {
        Ok(())
    } else {
        Err(WireError::Invalid("list of documents"))
    }
}

/// Files start at a whole sealed chunk, and no file is bigger than the limit.
fn offset_ok(offset: u64) -> bool {
    offset.is_multiple_of(CHUNK_SIZE as u64) && offset < MAX_DOCUMENT_BYTES
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

/// A note of a ledger entry: 1 to 200 characters, already trimmed, with no control characters.
fn note_ok(text: &str) -> bool {
    let length = text.chars().count();
    (1..=200).contains(&length) && text.trim() == text && !text.chars().any(char::is_control)
}

/// Who a part belongs to: exactly one of a friend (a `sync_id`) and a guest (a plain name).
fn who_ok(person: &Option<String>, guest: &Option<String>) -> bool {
    match (person, guest) {
        (Some(id), None) => sync_id_ok(id),
        (None, Some(name)) => category_name_ok(name),
        _ => false,
    }
}

/// Nobody twice: a friend once, a guest name once (ignoring case).
fn nobody_twice<'a>(who: impl Iterator<Item = (&'a Option<String>, &'a Option<String>)>) -> bool {
    let (mut people, mut guests) = (std::collections::HashSet::new(), std::collections::HashSet::new());
    who.into_iter().all(|(person, guest)| match (person, guest) {
        (Some(id), _) => people.insert(id.as_str().to_string()),
        (None, Some(name)) => guests.insert(name.to_lowercase()),
        _ => false,
    })
}

/// The order the parts of a bill are sent in, so two devices that hold the same parts send the same bytes: friends by `sync_id`, then
/// guests by name.
pub fn sort_split(parts: &mut [SplitPartRow]) {
    parts.sort_by(|a, b| (a.person.is_none(), &a.person, a.guest.as_ref().map(|g| g.to_lowercase())).cmp(&(b.person.is_none(), &b.person, b.guest.as_ref().map(|g| g.to_lowercase()))));
}

/// The same for a template.
pub fn sort_template(parts: &mut [TemplatePartRow]) {
    parts.sort_by(|a, b| (a.person.is_none(), &a.person, a.guest.as_ref().map(|g| g.to_lowercase())).cmp(&(b.person.is_none(), &b.person, b.guest.as_ref().map(|g| g.to_lowercase()))));
}

/// The parts of a shared bill: at most 30, each a friend or a guest with a part of at least a cent, nobody twice, and together no more
/// than the bill, in the order `sort_split` makes.
fn split_ok(parts: &[SplitPartRow], amount: i64) -> bool {
    let mut sorted = parts.to_vec();
    sort_split(&mut sorted);
    parts.len() <= 30
        && sorted == parts
        && parts.iter().all(|p| who_ok(&p.person, &p.guest) && money_ok(p.amount_minor, 1))
        && nobody_twice(parts.iter().map(|p| (&p.person, &p.guest)))
        && parts.iter().map(|p| p.amount_minor).sum::<i64>() <= amount
}

/// A template: the weight of the person who pays (0 or more), at most 30 friends and guests with a weight of at least 1, nobody twice,
/// and some weight above 0 somewhere.
fn template_ok(t: &TemplateRow) -> bool {
    let mut sorted = t.parts.clone();
    sort_template(&mut sorted);
    t.parts.len() <= 30
        && sorted == t.parts
        && money_ok(t.mine, 0)
        && t.parts.iter().all(|p| who_ok(&p.person, &p.guest) && money_ok(p.weight, 1))
        && nobody_twice(t.parts.iter().map(|p| (&p.person, &p.guest)))
        && (t.mine > 0 || !t.parts.is_empty())
}

fn money_ok(value: i64, min: i64) -> bool {
    (min..=MAX_MINOR).contains(&value)
}

fn option_sync_id(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(sync_id_ok)
}

/// The most skipped dates one recurring item carries.
const MAX_SKIPPED_DATES: usize = 100;

/// A JSON array of valid dates, sorted and without repeats, of at most `MAX_SKIPPED_DATES`.
fn skipped_dates_ok(text: &str) -> bool {
    match serde_json::from_str::<Vec<String>>(text) {
        Ok(dates) => dates.len() <= MAX_SKIPPED_DATES && dates.iter().all(|d| date_ok(d)) && dates.windows(2).all(|pair| pair[0] < pair[1]),
        Err(_) => false,
    }
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
    /// Place in the list of its type, from 0.
    pub position: i64,
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
    /// The later occurrences that were skipped one by one: a JSON array of dates, sorted, without repeats (at most `MAX_SKIPPED_DATES`).
    pub skipped_dates: String,
    pub end_date: Option<String>,
    pub is_active: i64,
    /// How each payment is shared, as weights; none when the payments are not split.
    pub split: Option<TemplateRow>,
    pub created_at: String,
    pub updated_at: String,
}

/// The split template of a recurring item: the weight of the person who pays and one weight for each friend or guest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateRow {
    pub mine: i64,
    pub parts: Vec<TemplatePartRow>,
}

/// One friend (by the `sync_id` of the person) or one guest (by name) in a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplatePartRow {
    pub person: Option<String>,
    pub guest: Option<String>,
    pub weight: i64,
}

/// One friend or guest and their part of a shared bill. The parts belong to their transaction: they travel in its row and replace
/// the parts of the version it replaces, so two devices can never end with parts from both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SplitPartRow {
    pub person: Option<String>,
    pub guest: Option<String>,
    pub amount_minor: i64,
}

/// A friend: a name, whether they are archived, the person they were merged into (when two devices each added the same friend), and a
/// reminder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonRow {
    pub sync_id: String,
    pub stamp: String,
    pub name: String,
    pub archived: i64,
    pub merged_into: Option<String>,
    pub remind_on: Option<String>,
    pub remind_every_days: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// An entry of the ledger: money lent, borrowed or paid back with one friend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LedgerRow {
    pub sync_id: String,
    pub stamp: String,
    /// The `sync_id` of the friend.
    pub person: String,
    pub kind: String,
    pub amount_minor: i64,
    pub entry_date: String,
    pub note: Option<String>,
    /// The `sync_id` of the expense it came with, if it is still there.
    pub transaction: Option<String>,
    /// The bill code this entry came from, if any.
    pub origin_id: Option<String>,
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
    /// The parts of the friends and guests the bill is shared with, in the order `sort_split` makes. Your own part is not here: it is
    /// the amount minus these.
    pub split: Vec<SplitPartRow>,
    pub created_at: String,
    pub updated_at: String,
}

/// A document, without its file: the name, the kind, the size and the hash of the contents, the date of the paper and the switch.
/// The contents of a document never change, so one `sync_id` always means the same bytes, and the hash is what the other device
/// checks a file against when it arrives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentRow {
    pub sync_id: String,
    pub stamp: String,
    pub name: String,
    pub media_type: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub document_date: String,
    pub keep_on_phone: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// One document linked to one transaction, one recurring item or one ledger entry (exactly one of the three).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentLinkRow {
    pub sync_id: String,
    pub stamp: String,
    /// The `sync_id` of the document.
    pub document: String,
    /// The `sync_id` of the transaction, of the recurring item or of the ledger entry.
    pub transaction: Option<String>,
    pub recurring: Option<String>,
    pub ledger: Option<String>,
    pub created_at: String,
}

/// A row of one table, read and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Settings(SettingsRow),
    Category(CategoryRow),
    Person(PersonRow),
    Ledger(LedgerRow),
    Recurring(RecurringRow),
    Budget(BudgetRow),
    Transaction(TransactionRow),
    Document(DocumentRow),
    Link(DocumentLinkRow),
}

impl Row {
    pub fn sync_id(&self) -> &str {
        match self {
            Row::Settings(_) => "settings",
            Row::Category(r) => &r.sync_id,
            Row::Person(r) => &r.sync_id,
            Row::Ledger(r) => &r.sync_id,
            Row::Recurring(r) => &r.sync_id,
            Row::Budget(r) => &r.sync_id,
            Row::Transaction(r) => &r.sync_id,
            Row::Document(r) => &r.sync_id,
            Row::Link(r) => &r.sync_id,
        }
    }

    pub fn stamp(&self) -> &str {
        match self {
            Row::Settings(r) => &r.stamp,
            Row::Category(r) => &r.stamp,
            Row::Person(r) => &r.stamp,
            Row::Ledger(r) => &r.stamp,
            Row::Recurring(r) => &r.stamp,
            Row::Budget(r) => &r.stamp,
            Row::Transaction(r) => &r.stamp,
            Row::Document(r) => &r.stamp,
            Row::Link(r) => &r.stamp,
        }
    }
}

/// Reads one row of `table` from the JSON a peer sent, and checks every field.
pub fn parse_row(table: Table, value: &Value) -> Result<Row, WireError> {
    let bad = WireError::Invalid("row");
    let row = match table {
        Table::Settings => Row::Settings(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::Categories => Row::Category(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::People => Row::Person(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::LedgerEntries => Row::Ledger(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::RecurringTransactions => Row::Recurring(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::Budgets => Row::Budget(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::Transactions => Row::Transaction(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::Documents => Row::Document(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
        Table::DocumentLinks => Row::Link(serde_json::from_value(value.clone()).map_err(|_| WireError::Format)?),
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
                && (0..=MAX_POSITION).contains(&r.position)
                && timestamp_ok(&r.created_at)
                && timestamp_ok(&r.updated_at)
        }
        Row::Person(r) => {
            sync_id_ok(&r.sync_id)
                && category_name_ok(&r.name)
                && [0, 1].contains(&r.archived)
                && option_sync_id(&r.merged_into)
                && r.merged_into.as_deref() != Some(r.sync_id.as_str())
                // a friend who was merged into another is archived
                && (r.merged_into.is_none() || r.archived == 1)
                && option_date(&r.remind_on)
                && r.remind_every_days.is_none_or(|n| (1..=365).contains(&n) && r.remind_on.is_some())
                && timestamp_ok(&r.created_at)
                && timestamp_ok(&r.updated_at)
        }
        Row::Ledger(r) => {
            sync_id_ok(&r.sync_id)
                && sync_id_ok(&r.person)
                && ["LENT", "BORROWED", "REPAID_TO_ME", "REPAID_BY_ME"].contains(&r.kind.as_str())
                && money_ok(r.amount_minor, 1)
                && date_ok(&r.entry_date)
                && r.note.as_deref().is_none_or(note_ok)
                && option_sync_id(&r.transaction)
                && option_sync_id(&r.origin_id)
                && timestamp_ok(&r.created_at)
                && timestamp_ok(&r.updated_at)
        }
        Row::Recurring(r) => {
            sync_id_ok(&r.sync_id)
                && r.split.as_ref().is_none_or(template_ok)
                && sync_id_ok(&r.category)
                && name_ok(&r.name)
                && description_ok(&r.description)
                && r.amount_minor.is_none_or(|a| money_ok(a, 1))
                && date_ok(&r.start_date)
                && ["DAY", "WEEK", "MONTH", "YEAR"].contains(&r.interval_unit.as_str())
                && (1..=1000).contains(&r.interval_count)
                && date_ok(&r.next_due_date)
                && r.next_due_date >= r.start_date
                && skipped_dates_ok(&r.skipped_dates)
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
                && split_ok(&r.split, r.amount_minor)
                && timestamp_ok(&r.created_at)
                && timestamp_ok(&r.updated_at)
        }
        Row::Document(r) => {
            sync_id_ok(&r.sync_id)
                && is_clean_name(&r.name)
                && MediaType::parse(&r.media_type).is_some()
                && (1..=MAX_DOCUMENT_BYTES as i64).contains(&r.size_bytes)
                && is_hash(&r.sha256)
                && date_ok(&r.document_date)
                && [0, 1].contains(&r.keep_on_phone)
                && timestamp_ok(&r.created_at)
                && timestamp_ok(&r.updated_at)
        }
        Row::Link(r) => {
            // exactly one target, and it is a real id
            let targets = [&r.transaction, &r.recurring, &r.ledger];
            let one_target = targets.iter().filter(|t| t.is_some()).count() == 1;
            sync_id_ok(&r.sync_id) && sync_id_ok(&r.document) && one_target && targets.iter().all(|t| option_sync_id(t)) && timestamp_ok(&r.created_at)
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
        Row::Person(r) => serde_json::to_value(r),
        Row::Ledger(r) => serde_json::to_value(r),
        Row::Recurring(r) => serde_json::to_value(r),
        Row::Budget(r) => serde_json::to_value(r),
        Row::Transaction(r) => serde_json::to_value(r),
        Row::Document(r) => serde_json::to_value(r),
        Row::Link(r) => serde_json::to_value(r),
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
        json!({"syncId": ID_A, "stamp": STAMP, "type": "EXPENSE", "name": "Food", "deletedAt": null, "mergedInto": null, "position": 0, "createdAt": TS, "updatedAt": TS})
    }

    fn transaction() -> Value {
        json!({"syncId": ID_B, "stamp": STAMP, "category": ID_A, "recurring": null, "recurringDueDate": null, "clientUuid": null,
               "name": "Coffee", "amountMinor": 350, "plannedAmountMinor": null, "paymentDate": "2026-03-01", "description": null,
               "split": [], "createdAt": TS, "updatedAt": TS})
    }

    fn recurring() -> Value {
        json!({"syncId": ID_B, "stamp": STAMP, "category": ID_A, "name": "Rent", "description": "Flat 4", "amountMinor": 90000,
               "startDate": "2026-01-01", "intervalUnit": "MONTH", "intervalCount": 1, "nextDueDate": "2026-03-01", "skippedDates": "[]", "endDate": null,
               "isActive": 1, "split": null, "createdAt": TS, "updatedAt": TS})
    }

    fn person() -> Value {
        json!({"syncId": ID_A, "stamp": STAMP, "name": "Anna", "archived": 0, "mergedInto": null, "remindOn": null, "remindEveryDays": null,
               "createdAt": TS, "updatedAt": TS})
    }

    fn entry() -> Value {
        json!({"syncId": ID_B, "stamp": STAMP, "person": ID_A, "kind": "LENT", "amountMinor": 2500, "entryDate": "2026-03-01", "note": "taxi",
               "transaction": null, "originId": null, "createdAt": TS, "updatedAt": TS})
    }

    fn shared() -> Value {
        // 90.00 shared with a friend and a guest: your part is what is left
        changed(
            transaction(),
            "split",
            json!([{"person": ID_A, "guest": null, "amountMinor": 3000}, {"person": null, "guest": "Carl", "amountMinor": 3000}]),
        )
        .as_object()
        .map(|o| {
            let mut o = o.clone();
            o.insert("amountMinor".into(), json!(9000));
            Value::Object(o)
        })
        .unwrap()
    }

    fn templated() -> Value {
        changed(recurring(), "split", json!({"mine": 2, "parts": [{"person": ID_A, "guest": null, "weight": 1}, {"person": null, "guest": "Carl", "weight": 1}]}))
    }

    fn budget() -> Value {
        json!({"syncId": ID_B, "stamp": STAMP, "type": "EXPENSE", "scope": "CATEGORY", "category": ID_A, "amountLimitMinor": 50000,
               "effectiveFrom": "2026-03-01", "createdAt": TS, "updatedAt": TS})
    }

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn document() -> Value {
        json!({"syncId": ID_B, "stamp": STAMP, "name": "Boiler warranty.pdf", "mediaType": "application/pdf", "sizeBytes": 48211, "sha256": SHA,
               "documentDate": "2026-03-01", "keepOnPhone": 0, "createdAt": TS, "updatedAt": TS})
    }

    fn link() -> Value {
        json!({"syncId": ID_B, "stamp": STAMP, "document": ID_A, "transaction": ID_A, "recurring": null, "ledger": null, "createdAt": TS})
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
            (Table::People, person()),
            (Table::People, changed(changed(person(), "archived", json!(1)), "mergedInto", json!(ID_B))),
            (Table::People, changed(changed(person(), "remindOn", json!("2026-04-01")), "remindEveryDays", json!(30))),
            (Table::LedgerEntries, entry()),
            (Table::LedgerEntries, changed(changed(entry(), "transaction", json!(ID_A)), "originId", json!(ID_B))),
            (Table::RecurringTransactions, recurring()),
            (Table::RecurringTransactions, templated()),
            (Table::Budgets, budget()),
            (Table::Transactions, transaction()),
            (Table::Transactions, shared()),
            (Table::Documents, document()),
            (Table::DocumentLinks, link()),
            (Table::DocumentLinks, changed(changed(link(), "transaction", json!(null)), "recurring", json!(ID_B))),
            (Table::DocumentLinks, changed(changed(link(), "transaction", json!(null)), "ledger", json!(ID_B))),
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
        assert!(parse_row(Table::DocumentLinks, &document()).is_err(), "a document is not a link");
        let mut extra = document();
        extra["file"] = json!("the bytes");
        assert!(parse_row(Table::Documents, &extra).is_err(), "no way to smuggle a file in a row");
    }

    #[test]
    fn there_is_a_table_for_every_kind_of_row_and_the_protocol_knows_it() {
        assert_eq!(PROTOCOL, 5);
        let names: Vec<&str> = Table::ALL.iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            ["settings", "categories", "people", "recurring_transactions", "budgets", "transactions", "ledger_entries", "documents", "document_links"]
        );
        // what a row points at is always in an earlier table, so it is there when the row is applied
        let position = |t: Table| Table::ALL.iter().position(|x| *x == t).unwrap();
        assert!(position(Table::Documents) > position(Table::Transactions) && position(Table::DocumentLinks) > position(Table::Documents));
        assert!(position(Table::DocumentLinks) > position(Table::RecurringTransactions));
        // the parts of a bill and the weights of a template name friends, the ledger names friends and expenses, a link names an entry
        assert!(position(Table::People) < position(Table::Transactions) && position(Table::People) < position(Table::RecurringTransactions));
        assert!(position(Table::LedgerEntries) > position(Table::Transactions) && position(Table::LedgerEntries) > position(Table::People));
        assert!(position(Table::DocumentLinks) > position(Table::LedgerEntries));
        // the order of the tables is the order they are declared in, which is how a map of them is walked
        let mut sorted = Table::ALL.to_vec();
        sorted.sort();
        assert_eq!(sorted, Table::ALL);
        // and each table has a name the digest messages accept
        for table in Table::ALL.into_iter().filter(|t| *t != Table::Settings) {
            let digest = format!(r#"{{"t":"digest","table":"{}","entries":[["{ID_A}","{STAMP}",0]],"last":true}}"#, table.name());
            assert!(Message::from_bytes(digest.as_bytes()).is_ok(), "{table:?}");
        }
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
            (Table::Categories, changed(category(), "position", json!(-1))),
            (Table::Categories, changed(category(), "position", json!(1_000_001))),
            (Table::Categories, changed(category(), "position", json!("1"))),
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
            (Table::RecurringTransactions, changed(recurring(), "skippedDates", json!("nope"))),
            (Table::RecurringTransactions, changed(recurring(), "skippedDates", json!("[\"2026-05-01\",\"2026-04-01\"]"))),
            (Table::RecurringTransactions, changed(recurring(), "skippedDates", json!("[\"2026-04-01\",\"2026-04-01\"]"))),
            (Table::RecurringTransactions, changed(recurring(), "skippedDates", json!("[\"2026-4-1\"]"))),
            (Table::RecurringTransactions, changed(recurring(), "skippedDates", json!("[1]"))),
            (Table::RecurringTransactions, changed(recurring(), "nextDueDate", json!("2025-12-31"))),
            (Table::RecurringTransactions, changed(recurring(), "endDate", json!("2025-12-31"))),
            (Table::RecurringTransactions, changed(recurring(), "amountMinor", json!(0))),
            (Table::Budgets, changed(budget(), "scope", json!("GLOBAL"))),
            (Table::Budgets, changed(budget(), "category", json!(null))),
            (Table::Budgets, changed(budget(), "effectiveFrom", json!("2026-03-15"))),
            (Table::Budgets, changed(budget(), "amountLimitMinor", json!(-1))),
            (Table::Budgets, changed(budget(), "type", json!("x"))),
            (Table::Documents, changed(document(), "name", json!(""))),
            (Table::Documents, changed(document(), "name", json!("a/b.pdf"))),
            (Table::Documents, changed(document(), "name", json!("..\\x.pdf"))),
            (Table::Documents, changed(document(), "name", json!(" padded.pdf"))),
            (Table::Documents, changed(document(), "name", json!("two  spaces.pdf"))),
            (Table::Documents, changed(document(), "name", json!("hidden\u{202e}fdp.exe"))),
            (Table::Documents, changed(document(), "name", json!("a".repeat(121)))),
            (Table::Documents, changed(document(), "mediaType", json!("image/svg+xml"))),
            (Table::Documents, changed(document(), "mediaType", json!("IMAGE/PNG"))),
            (Table::Documents, changed(document(), "sizeBytes", json!(0))),
            (Table::Documents, changed(document(), "sizeBytes", json!(26_214_401))),
            (Table::Documents, changed(document(), "sizeBytes", json!(-1))),
            (Table::Documents, changed(document(), "sizeBytes", json!(10.5))),
            (Table::Documents, changed(document(), "sha256", json!(&SHA[1..]))),
            (Table::Documents, changed(document(), "sha256", json!(SHA.to_uppercase()))),
            (Table::Documents, changed(document(), "documentDate", json!("2026-02-30"))),
            (Table::Documents, changed(document(), "keepOnPhone", json!(2))),
            (Table::Documents, changed(document(), "keepOnPhone", json!(true))),
            (Table::Documents, changed(document(), "syncId", json!("nope"))),
            (Table::DocumentLinks, changed(link(), "document", json!("1"))),
            (Table::DocumentLinks, changed(link(), "transaction", json!(null))),
            (Table::DocumentLinks, changed(link(), "recurring", json!(ID_B))),
            (Table::DocumentLinks, changed(link(), "transaction", json!("x"))),
            (Table::DocumentLinks, changed(changed(link(), "transaction", json!(null)), "recurring", json!("x"))),
            (Table::DocumentLinks, changed(link(), "createdAt", json!("2026-03-01"))),
            (Table::DocumentLinks, changed(link(), "ledger", json!(ID_B))),
            (Table::DocumentLinks, changed(changed(link(), "transaction", json!(null)), "ledger", json!("x"))),
            (Table::DocumentLinks, changed(changed(link(), "transaction", json!(null)), "ledger", json!(null))),
            // friends
            (Table::People, changed(person(), "name", json!(""))),
            (Table::People, changed(person(), "name", json!(" Anna"))),
            (Table::People, changed(person(), "name", json!("a".repeat(51)))),
            (Table::People, changed(person(), "name", json!("tab\there"))),
            (Table::People, changed(person(), "archived", json!(2))),
            (Table::People, changed(person(), "archived", json!(true))),
            (Table::People, changed(person(), "mergedInto", json!(ID_B))),
            (Table::People, changed(changed(person(), "archived", json!(1)), "mergedInto", json!(ID_A))),
            (Table::People, changed(changed(person(), "archived", json!(1)), "mergedInto", json!("x"))),
            (Table::People, changed(person(), "remindOn", json!("2026-02-30"))),
            (Table::People, changed(person(), "remindEveryDays", json!(7))),
            (Table::People, changed(changed(person(), "remindOn", json!("2026-04-01")), "remindEveryDays", json!(0))),
            (Table::People, changed(changed(person(), "remindOn", json!("2026-04-01")), "remindEveryDays", json!(366))),
            (Table::People, changed(person(), "syncId", json!("nope"))),
            (Table::People, changed(person(), "phone", json!("555"))),
            // the ledger
            (Table::LedgerEntries, changed(entry(), "kind", json!("GIFT"))),
            (Table::LedgerEntries, changed(entry(), "kind", json!("lent"))),
            (Table::LedgerEntries, changed(entry(), "amountMinor", json!(0))),
            (Table::LedgerEntries, changed(entry(), "amountMinor", json!(-1))),
            (Table::LedgerEntries, changed(entry(), "amountMinor", json!(MAX_MINOR + 1))),
            (Table::LedgerEntries, changed(entry(), "amountMinor", json!(1.5))),
            (Table::LedgerEntries, changed(entry(), "entryDate", json!("2026-02-30"))),
            (Table::LedgerEntries, changed(entry(), "entryDate", json!("2026-3-1"))),
            (Table::LedgerEntries, changed(entry(), "note", json!(""))),
            (Table::LedgerEntries, changed(entry(), "note", json!(" padded"))),
            (Table::LedgerEntries, changed(entry(), "note", json!("x".repeat(201)))),
            (Table::LedgerEntries, changed(entry(), "note", json!("line\nbreak"))),
            (Table::LedgerEntries, changed(entry(), "person", json!("1"))),
            (Table::LedgerEntries, changed(entry(), "transaction", json!("x"))),
            (Table::LedgerEntries, changed(entry(), "originId", json!("ab"))),
            (Table::LedgerEntries, changed(entry(), "originId", json!(ID_A.to_uppercase()))),
            // the parts of a bill
            (Table::Transactions, changed(shared(), "amountMinor", json!(5999))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": ID_A, "guest": null, "amountMinor": 0}]))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": ID_A, "guest": null, "amountMinor": -5}]))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": ID_A, "guest": null, "amountMinor": 9001}]))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": ID_A, "guest": "Carl", "amountMinor": 100}]))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": null, "guest": null, "amountMinor": 100}]))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": "x", "guest": null, "amountMinor": 100}]))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": null, "guest": "", "amountMinor": 100}]))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": null, "guest": " Carl", "amountMinor": 100}]))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": ID_A, "guest": null, "amountMinor": 100}, {"person": ID_A, "guest": null, "amountMinor": 100}]))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": null, "guest": "Carl", "amountMinor": 100}, {"person": null, "guest": "carl", "amountMinor": 100}]))),
            // not in the order the parts are sent in
            (Table::Transactions, changed(shared(), "split", json!([{"person": null, "guest": "Carl", "amountMinor": 3000}, {"person": ID_A, "guest": null, "amountMinor": 3000}]))),
            (Table::Transactions, changed(shared(), "split", json!({"person": ID_A}))),
            (Table::Transactions, changed(shared(), "split", json!(null))),
            (Table::Transactions, changed(shared(), "split", json!([{"person": ID_A, "guest": null, "amountMinor": 100, "note": "x"}]))),
            // a template
            (Table::RecurringTransactions, changed(templated(), "split", json!({"mine": -1, "parts": [{"person": ID_A, "guest": null, "weight": 1}]}))),
            (Table::RecurringTransactions, changed(templated(), "split", json!({"mine": 0, "parts": []}))),
            (Table::RecurringTransactions, changed(templated(), "split", json!({"mine": 1, "parts": [{"person": ID_A, "guest": null, "weight": 0}]}))),
            (Table::RecurringTransactions, changed(templated(), "split", json!({"mine": 1, "parts": [{"person": ID_A, "guest": null, "weight": MAX_MINOR + 1}]}))),
            (Table::RecurringTransactions, changed(templated(), "split", json!({"mine": 1, "parts": [{"person": ID_A, "guest": "Carl", "weight": 1}]}))),
            (Table::RecurringTransactions, changed(templated(), "split", json!({"mine": 1, "parts": [{"person": ID_A, "guest": null, "weight": 1}, {"person": ID_A, "guest": null, "weight": 1}]}))),
            (Table::RecurringTransactions, changed(templated(), "split", json!({"mine": 1}))),
            (Table::RecurringTransactions, changed(templated(), "split", json!({"mine": 1, "parts": [], "extra": 1}))),
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
