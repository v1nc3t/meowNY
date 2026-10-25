//! Making two databases equal (`docs/sync-design.md`, sections 8, 10, 11 and 12).
//!
//! Each side lists `(sync_id, stamp, deleted)` for every shared row and deletion marker (`digest`). Both lists are known to
//! both sides, so each works out for itself what the other lacks or has older (`outgoing`) and sends exactly that. What
//! arrives is checked row by row (`wire::parse_row`) and applied in one transaction (`apply`), with the same rules on
//! both sides, so they end with the same rows.
//!
//! Nothing here touches the network or the clock of the machine except through its arguments, so the whole conversation
//! can be run between two databases in memory (`run.rs`) and tested hard.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sha2::{Digest as _, Sha256};

use super::stamp::{self, Stamp};
use super::wire::{
    self, BudgetRow, CategoryRow, DigestMessage, DocumentLinkRow, DocumentRow, Entries, LedgerRow, Marker, Message, PersonRow, RecurringRow, Row,
    RowsMessage, SplitPartRow, Table, TemplatePartRow, TemplateRow, TransactionRow, WireError, MAX_ENTRIES_PER_MESSAGE, MAX_ROWS_PER_MESSAGE,
    MAX_ROWS_PER_TABLE, SOFT_MESSAGE_BYTES,
};
use crate::debts::splits::{self, Who};
use crate::recurring;

/// A stamp from the other device this far ahead of this device's clock is refused: it would win every later fight.
pub const MAX_CLOCK_LEAD_MS: u64 = 24 * 60 * 60 * 1000;
/// The sync is refused when the clocks differ by more than this; a smaller gap above `CLOCK_WARNING_MS` is only a warning.
pub const CLOCK_WARNING_MS: u64 = 60 * 1000;

#[derive(Debug)]
pub enum SyncError {
    /// The other device has another schema version; `newer_here` says which one has to be updated.
    SchemaMismatch { newer_here: bool },
    ProtocolMismatch,
    ClockFarOff,
    /// The currencies or their generations do not fit together.
    Currency,
    /// A message or a row broke a rule. Says what kind of thing, never the content.
    Invalid(&'static str),
    /// The app is locked, so there is no database to sync.
    Locked,
    /// A phone that already holds data cannot be paired: it must be new or erased.
    NotEmpty,
    Wire(WireError),
    Database,
    Internal,
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::SchemaMismatch { newer_here: true } => f.write_str("the other device must be updated"),
            SyncError::SchemaMismatch { newer_here: false } => f.write_str("this app must be updated"),
            SyncError::ProtocolMismatch => f.write_str("the other device speaks another version of the sync"),
            SyncError::ClockFarOff => f.write_str("the clocks of the two devices are too far apart"),
            SyncError::Currency => f.write_str("the currencies of the two devices do not fit together"),
            SyncError::Invalid(what) => write!(f, "invalid {what} from the other device"),
            SyncError::Locked => f.write_str("the app is locked"),
            SyncError::NotEmpty => f.write_str("this device already holds data"),
            SyncError::Wire(e) => write!(f, "{e}"),
            SyncError::Database => f.write_str("the database refused a change"),
            SyncError::Internal => f.write_str("something went wrong"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<WireError> for SyncError {
    fn from(e: WireError) -> Self {
        SyncError::Wire(e)
    }
}

impl From<rusqlite::Error> for SyncError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            // a row that breaks a rule of the database is a bad row from the other device, not a fault of this one
            rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => SyncError::Invalid("row"),
            _ => SyncError::Database,
        }
    }
}

impl From<crate::vault::AppError> for SyncError {
    fn from(e: crate::vault::AppError) -> Self {
        match e {
            crate::vault::AppError::InvalidInput => SyncError::Currency,
            _ => SyncError::Internal,
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// digests

/// What each side holds: for every table, `sync_id` to (stamp, is a deletion marker).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Digest(pub BTreeMap<Table, Entries>);

const LISTED: [Table; 8] = [
    Table::Categories,
    Table::People,
    Table::RecurringTransactions,
    Table::Budgets,
    Table::Transactions,
    Table::LedgerEntries,
    Table::Documents,
    Table::DocumentLinks,
];

pub fn digest(conn: &Connection) -> Result<Digest, SyncError> {
    let mut out = BTreeMap::new();
    let mut settings = Entries::new();
    if let Some(stamp) = conn.query_row("SELECT stamp FROM settings WHERE id = 1", [], |r| r.get::<_, String>(0)).optional()? {
        settings.insert("settings".to_string(), (stamp, false));
    }
    out.insert(Table::Settings, settings);
    for table in LISTED {
        let mut entries = Entries::new();
        let mut stmt = conn.prepare(&format!("SELECT sync_id, stamp FROM {}", table.name()))?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (id, stamp) = row?;
            entries.insert(id, (stamp, false));
        }
        let mut stmt = conn.prepare("SELECT sync_id, stamp FROM sync_deleted WHERE kind = ?1")?;
        for row in stmt.query_map([table.name()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (id, stamp) = row?;
            // a marker and a row of one id cannot both stand; if they ever do, the newer one counts
            match entries.get(&id) {
                Some((live, _)) if *live >= stamp => {}
                _ => {
                    entries.insert(id, (stamp, true));
                }
            }
        }
        out.insert(table, entries);
    }
    Ok(Digest(out))
}

/// A hash of one table's list. Two tables with the same hash hold the same rows at the same stamps.
pub fn table_hash(digest: &Digest, table: Table) -> String {
    let mut hasher = Sha256::new();
    hasher.update(table.name().as_bytes());
    if let Some(entries) = digest.0.get(&table) {
        for (id, (stamp, deleted)) in entries {
            hasher.update([0]);
            hasher.update(id.as_bytes());
            hasher.update([0]);
            hasher.update(stamp.as_bytes());
            hasher.update([0, u8::from(*deleted), b'\n']);
        }
    }
    crate::keys::hex_encode(&hasher.finalize())
}

pub fn hashes(digest: &Digest) -> Vec<(Table, String)> {
    Table::ALL.iter().map(|t| (*t, table_hash(digest, *t))).collect()
}

/// A hash of everything: the same on two devices exactly when their lists are the same.
pub fn full_hash(digest: &Digest) -> String {
    let mut hasher = Sha256::new();
    for (_, hash) in hashes(digest) {
        hasher.update(hash.as_bytes());
    }
    crate::keys::hex_encode(&hasher.finalize())
}

/// The tables whose hashes differ, in sending order. A table the other side does not report counts as different.
pub fn differing(mine: &[(Table, String)], theirs: &[(Table, String)]) -> Vec<Table> {
    let theirs: HashMap<Table, &String> = theirs.iter().map(|(t, h)| (*t, h)).collect();
    Table::ALL
        .iter()
        .copied()
        .filter(|t| mine.iter().find(|(m, _)| m == t).map(|(_, h)| h) != theirs.get(t).copied())
        .collect()
}

pub fn digest_messages(digest: &Digest, tables: &[Table]) -> Vec<Message> {
    let mut out = Vec::new();
    for table in tables {
        let empty = Entries::new();
        let entries = digest.0.get(table).unwrap_or(&empty);
        let all: Vec<(String, String, u8)> = entries.iter().map(|(id, (stamp, deleted))| (id.clone(), stamp.clone(), u8::from(*deleted))).collect();
        let mut chunks: Vec<&[(String, String, u8)]> = all.chunks(MAX_ENTRIES_PER_MESSAGE).collect();
        if chunks.is_empty() {
            chunks.push(&[]);
        }
        let count = chunks.len();
        for (i, chunk) in chunks.into_iter().enumerate() {
            out.push(Message::Digest(DigestMessage { table: *table, entries: chunk.to_vec(), last: i + 1 == count }));
        }
    }
    out
}

/// Collects the digest messages of the other side, in the order and shape they must come.
pub struct DigestCollector {
    expected: Vec<Table>,
    position: usize,
    collected: Digest,
    current: Entries,
}

impl DigestCollector {
    pub fn new(expected: Vec<Table>) -> DigestCollector {
        DigestCollector { expected, position: 0, collected: Digest::default(), current: Entries::new() }
    }

    pub fn push(&mut self, message: DigestMessage) -> Result<(), SyncError> {
        let table = *self.expected.get(self.position).ok_or(SyncError::Wire(WireError::Order))?;
        if message.table != table {
            return Err(SyncError::Wire(WireError::Order));
        }
        for (id, stamp, deleted) in message.entries {
            if self.current.len() >= MAX_ROWS_PER_TABLE || self.current.insert(id, (stamp, deleted == 1)).is_some() {
                return Err(SyncError::Invalid("list"));
            }
        }
        if message.last {
            self.collected.0.insert(table, std::mem::take(&mut self.current));
            self.position += 1;
        }
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.position == self.expected.len()
    }

    pub fn finish(self) -> Result<Digest, SyncError> {
        if self.is_complete() {
            Ok(self.collected)
        } else {
            Err(SyncError::Wire(WireError::Order))
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// outgoing rows

/// Which ids of `table` this side has to send: those the other side lacks, and those it has at an older stamp.
fn to_send(mine: &Digest, theirs: &Digest, table: Table) -> Vec<String> {
    let empty = Entries::new();
    let theirs = theirs.0.get(&table).unwrap_or(&empty);
    mine.0
        .get(&table)
        .map(|entries| {
            entries
                .iter()
                .filter(|(id, (stamp, _))| theirs.get(*id).is_none_or(|(other, _)| other < stamp))
                .map(|(id, _)| id.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn load_rows(conn: &Connection, table: Table, wanted: &HashSet<&String>) -> Result<Vec<Row>, SyncError> {
    let mut rows = Vec::new();
    match table {
        Table::Settings => {
            let mut stmt = conn.prepare("SELECT stamp, locale, timezone FROM settings WHERE id = 1")?;
            for r in stmt.query_map([], |r| Ok(wire::SettingsRow { stamp: r.get(0)?, locale: r.get(1)?, timezone: r.get(2)? }))? {
                let r = r?;
                if wanted.contains(&"settings".to_string()) {
                    rows.push(Row::Settings(r));
                }
            }
        }
        Table::Categories => {
            let mut stmt = conn.prepare("SELECT sync_id, stamp, type, name, deleted_at, merged_into, position, created_at, updated_at FROM categories ORDER BY sync_id")?;
            for r in stmt.query_map([], |r| {
                Ok(CategoryRow {
                    sync_id: r.get(0)?,
                    stamp: r.get(1)?,
                    kind: r.get(2)?,
                    name: r.get(3)?,
                    deleted_at: r.get(4)?,
                    merged_into: r.get(5)?,
                    position: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            })? {
                let r = r?;
                if wanted.contains(&r.sync_id) {
                    rows.push(Row::Category(r));
                }
            }
        }
        Table::People => {
            let mut stmt = conn.prepare(
                "SELECT sync_id, stamp, name, archived, merged_into, remind_on, remind_every_days, created_at, updated_at FROM people ORDER BY sync_id",
            )?;
            for r in stmt.query_map([], |r| {
                Ok(PersonRow {
                    sync_id: r.get(0)?,
                    stamp: r.get(1)?,
                    name: r.get(2)?,
                    archived: r.get(3)?,
                    merged_into: r.get(4)?,
                    remind_on: r.get(5)?,
                    remind_every_days: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            })? {
                let r = r?;
                if wanted.contains(&r.sync_id) {
                    rows.push(Row::Person(r));
                }
            }
        }
        Table::LedgerEntries => {
            let mut stmt = conn.prepare(
                "SELECT e.sync_id, e.stamp, p.sync_id, e.kind, e.amount_minor, e.entry_date, e.note, t.sync_id, e.origin_id, e.created_at, e.updated_at
                 FROM ledger_entries e JOIN people p ON p.id = e.person_id LEFT JOIN transactions t ON t.id = e.transaction_id ORDER BY e.sync_id",
            )?;
            for r in stmt.query_map([], |r| {
                Ok(LedgerRow {
                    sync_id: r.get(0)?,
                    stamp: r.get(1)?,
                    person: r.get(2)?,
                    kind: r.get(3)?,
                    amount_minor: r.get(4)?,
                    entry_date: r.get(5)?,
                    note: r.get(6)?,
                    transaction: r.get(7)?,
                    origin_id: r.get(8)?,
                    created_at: r.get(9)?,
                    updated_at: r.get(10)?,
                })
            })? {
                let r = r?;
                if wanted.contains(&r.sync_id) {
                    rows.push(Row::Ledger(r));
                }
            }
        }
        Table::RecurringTransactions => {
            let mut templates = templates_by_item(conn)?;
            let mut stmt = conn.prepare(
                "SELECT x.sync_id, x.stamp, c.sync_id, x.name, x.description, x.amount_minor, x.start_date, x.interval_unit, x.interval_count,
                        x.next_due_date, x.end_date, x.is_active, x.created_at, x.updated_at
                 FROM recurring_transactions x JOIN categories c ON c.id = x.category_id ORDER BY x.sync_id",
            )?;
            for r in stmt.query_map([], |r| {
                Ok(RecurringRow {
                    sync_id: r.get(0)?,
                    stamp: r.get(1)?,
                    category: r.get(2)?,
                    name: r.get(3)?,
                    description: r.get(4)?,
                    amount_minor: r.get(5)?,
                    start_date: r.get(6)?,
                    interval_unit: r.get(7)?,
                    interval_count: r.get(8)?,
                    next_due_date: r.get(9)?,
                    end_date: r.get(10)?,
                    is_active: r.get(11)?,
                    split: None,
                    created_at: r.get(12)?,
                    updated_at: r.get(13)?,
                })
            })? {
                let mut r = r?;
                if wanted.contains(&r.sync_id) {
                    r.split = templates.remove(&r.sync_id);
                    rows.push(Row::Recurring(r));
                }
            }
        }
        Table::Budgets => {
            let mut stmt = conn.prepare(
                "SELECT b.sync_id, b.stamp, b.type, b.scope, c.sync_id, b.amount_limit_minor, b.effective_from, b.created_at, b.updated_at
                 FROM budgets b LEFT JOIN categories c ON c.id = b.category_id ORDER BY b.sync_id",
            )?;
            for r in stmt.query_map([], |r| {
                Ok(BudgetRow {
                    sync_id: r.get(0)?,
                    stamp: r.get(1)?,
                    kind: r.get(2)?,
                    scope: r.get(3)?,
                    category: r.get(4)?,
                    amount_limit_minor: r.get(5)?,
                    effective_from: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            })? {
                let r = r?;
                if wanted.contains(&r.sync_id) {
                    rows.push(Row::Budget(r));
                }
            }
        }
        Table::Transactions => {
            let mut parts = parts_by_bill(conn)?;
            let mut stmt = conn.prepare(
                "SELECT t.sync_id, t.stamp, c.sync_id, r.sync_id, t.recurring_due_date, t.client_uuid, t.name, t.amount_minor,
                        t.planned_amount_minor, t.payment_date, t.description, t.created_at, t.updated_at
                 FROM transactions t JOIN categories c ON c.id = t.category_id
                 LEFT JOIN recurring_transactions r ON r.id = t.recurring_transaction_id ORDER BY t.sync_id",
            )?;
            for r in stmt.query_map([], |r| {
                Ok(TransactionRow {
                    sync_id: r.get(0)?,
                    stamp: r.get(1)?,
                    category: r.get(2)?,
                    recurring: r.get(3)?,
                    recurring_due_date: r.get(4)?,
                    client_uuid: r.get(5)?,
                    name: r.get(6)?,
                    amount_minor: r.get(7)?,
                    planned_amount_minor: r.get(8)?,
                    payment_date: r.get(9)?,
                    description: r.get(10)?,
                    split: Vec::new(),
                    created_at: r.get(11)?,
                    updated_at: r.get(12)?,
                })
            })? {
                let mut r = r?;
                if wanted.contains(&r.sync_id) {
                    r.split = parts.remove(&r.sync_id).unwrap_or_default();
                    rows.push(Row::Transaction(r));
                }
            }
        }
        Table::Documents => {
            let mut stmt = conn.prepare(
                "SELECT sync_id, stamp, name, media_type, size_bytes, sha256, document_date, keep_on_phone, created_at, updated_at
                 FROM documents ORDER BY sync_id",
            )?;
            for r in stmt.query_map([], |r| {
                Ok(DocumentRow {
                    sync_id: r.get(0)?,
                    stamp: r.get(1)?,
                    name: r.get(2)?,
                    media_type: r.get(3)?,
                    size_bytes: r.get(4)?,
                    sha256: r.get(5)?,
                    document_date: r.get(6)?,
                    keep_on_phone: r.get(7)?,
                    created_at: r.get(8)?,
                    updated_at: r.get(9)?,
                })
            })? {
                let r = r?;
                if wanted.contains(&r.sync_id) {
                    rows.push(Row::Document(r));
                }
            }
        }
        Table::DocumentLinks => {
            let mut stmt = conn.prepare(
                "SELECT l.sync_id, l.stamp, d.sync_id, t.sync_id, r.sync_id, e.sync_id, l.created_at
                 FROM document_links l JOIN documents d ON d.id = l.document_id
                 LEFT JOIN transactions t ON t.id = l.transaction_id
                 LEFT JOIN recurring_transactions r ON r.id = l.recurring_transaction_id
                 LEFT JOIN ledger_entries e ON e.id = l.ledger_entry_id ORDER BY l.sync_id",
            )?;
            for r in stmt.query_map([], |r| {
                Ok(DocumentLinkRow {
                    sync_id: r.get(0)?,
                    stamp: r.get(1)?,
                    document: r.get(2)?,
                    transaction: r.get(3)?,
                    recurring: r.get(4)?,
                    ledger: r.get(5)?,
                    created_at: r.get(6)?,
                })
            })? {
                let r = r?;
                if wanted.contains(&r.sync_id) {
                    rows.push(Row::Link(r));
                }
            }
        }
    }
    Ok(rows)
}

/// The parts of every shared bill by the `sync_id` of the bill, in the order they are sent in.
fn parts_by_bill(conn: &Connection) -> Result<HashMap<String, Vec<SplitPartRow>>, SyncError> {
    let mut stmt = conn.prepare(
        "SELECT t.sync_id, p.sync_id, l.guest_name, l.amount_minor
         FROM transaction_split_lines l JOIN transactions t ON t.id = l.transaction_id LEFT JOIN people p ON p.id = l.person_id",
    )?;
    let mut out: HashMap<String, Vec<SplitPartRow>> = HashMap::new();
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, SplitPartRow { person: r.get(1)?, guest: r.get(2)?, amount_minor: r.get(3)? })))? {
        let (bill, part) = row?;
        out.entry(bill).or_default().push(part);
    }
    out.values_mut().for_each(|parts| wire::sort_split(parts));
    Ok(out)
}

/// The template of every split recurring item by the `sync_id` of the item.
fn templates_by_item(conn: &Connection) -> Result<HashMap<String, TemplateRow>, SyncError> {
    let mut out: HashMap<String, TemplateRow> = HashMap::new();
    let mut stmt = conn.prepare("SELECT sync_id, split_my_weight FROM recurring_transactions WHERE split_my_weight IS NOT NULL")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (item, mine) = row?;
        out.insert(item, TemplateRow { mine, parts: Vec::new() });
    }
    let mut stmt = conn.prepare(
        "SELECT x.sync_id, p.sync_id, l.guest_name, l.weight
         FROM recurring_split_lines l JOIN recurring_transactions x ON x.id = l.recurring_transaction_id LEFT JOIN people p ON p.id = l.person_id",
    )?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, TemplatePartRow { person: r.get(1)?, guest: r.get(2)?, weight: r.get(3)? })))? {
        let (item, part) = row?;
        if let Some(template) = out.get_mut(&item) {
            template.parts.push(part);
        }
    }
    out.values_mut().for_each(|t| wire::sort_template(&mut t.parts));
    Ok(out)
}

fn load_markers(conn: &Connection, table: Table, wanted: &HashSet<&String>) -> Result<Vec<Marker>, SyncError> {
    let mut stmt = conn.prepare("SELECT sync_id, stamp FROM sync_deleted WHERE kind = ?1 ORDER BY sync_id")?;
    let mut out = Vec::new();
    for m in stmt.query_map([table.name()], |r| Ok(Marker { sync_id: r.get(0)?, stamp: r.get(1)? }))? {
        let m = m?;
        if wanted.contains(&m.sync_id) {
            out.push(m);
        }
    }
    Ok(out)
}

/// The rows message(s) for `tables`: exactly what the other side lacks or has older. Every table gets at least one
/// message and the last one of each is marked, so the other side knows when a table is complete.
pub fn outgoing(conn: &Connection, mine: &Digest, theirs: &Digest, tables: &[Table]) -> Result<Vec<Message>, SyncError> {
    let ids: BTreeMap<Table, Vec<String>> = tables.iter().map(|table| (*table, to_send(mine, theirs, *table))).collect();
    rows_messages(conn, tables, &ids)
}

/// The rows messages that carry exactly the rows (and deletion markers) with these ids, for each of `tables`.
fn rows_messages(conn: &Connection, tables: &[Table], ids: &BTreeMap<Table, Vec<String>>) -> Result<Vec<Message>, SyncError> {
    let mut out = Vec::new();
    let none = Vec::new();
    for table in tables {
        let ids = ids.get(table).unwrap_or(&none);
        let wanted: HashSet<&String> = ids.iter().collect();
        let rows = load_rows(conn, *table, &wanted)?;
        let markers = if *table == Table::Settings { Vec::new() } else { load_markers(conn, *table, &wanted)? };

        let mut messages: Vec<RowsMessage> = Vec::new();
        let mut current = RowsMessage { table: *table, rows: Vec::new(), deleted: Vec::new(), last: false };
        let mut size = 0usize;
        let close = |current: &mut RowsMessage, size: &mut usize, messages: &mut Vec<RowsMessage>| {
            messages.push(std::mem::replace(current, RowsMessage { table: *table, rows: Vec::new(), deleted: Vec::new(), last: false }));
            *size = 0;
        };
        for row in &rows {
            let value = wire::row_to_value(row);
            let length = serde_json::to_vec(&value).map_err(|_| SyncError::Internal)?.len();
            if !current.rows.is_empty() && (current.rows.len() >= MAX_ROWS_PER_MESSAGE || size + length > SOFT_MESSAGE_BYTES) {
                close(&mut current, &mut size, &mut messages);
            }
            current.rows.push(value);
            size += length + 1;
        }
        for marker in markers {
            if !current.deleted.is_empty() && current.deleted.len() >= MAX_ENTRIES_PER_MESSAGE {
                close(&mut current, &mut size, &mut messages);
            }
            current.deleted.push(marker);
        }
        messages.push(current);
        let count = messages.len();
        for (i, mut message) in messages.into_iter().enumerate() {
            message.last = i + 1 == count;
            out.push(Message::Rows(message));
        }
    }
    Ok(out)
}

/// What arrived, checked, ready to apply.
#[derive(Debug, Default)]
pub struct Incoming {
    pub rows: BTreeMap<Table, Vec<Row>>,
    pub markers: BTreeMap<Table, Vec<Marker>>,
}

impl Incoming {
    pub fn is_empty(&self) -> bool {
        self.rows.values().all(Vec::is_empty) && self.markers.values().all(Vec::is_empty)
    }
}

/// Collects the rows messages of the other side.
pub struct RowsCollector {
    expected: Vec<Table>,
    position: usize,
    incoming: Incoming,
    counts: usize,
}

impl RowsCollector {
    pub fn new(expected: Vec<Table>) -> RowsCollector {
        RowsCollector { expected, position: 0, incoming: Incoming::default(), counts: 0 }
    }

    pub fn push(&mut self, message: RowsMessage) -> Result<(), SyncError> {
        let table = *self.expected.get(self.position).ok_or(SyncError::Wire(WireError::Order))?;
        if message.table != table {
            return Err(SyncError::Wire(WireError::Order));
        }
        self.counts += message.rows.len() + message.deleted.len();
        if self.counts > MAX_ROWS_PER_TABLE * Table::ALL.len() {
            return Err(SyncError::Invalid("number of rows"));
        }
        for value in &message.rows {
            self.incoming.rows.entry(table).or_default().push(wire::parse_row(table, value)?);
        }
        self.incoming.markers.entry(table).or_default().extend(message.deleted);
        if message.last {
            self.position += 1;
        }
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.position == self.expected.len()
    }

    pub fn finish(self) -> Result<Incoming, SyncError> {
        if self.is_complete() {
            Ok(self.incoming)
        } else {
            Err(SyncError::Wire(WireError::Order))
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// applying

/// What one `apply` did, for the summary shown after a sync.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Rows this device did not have.
    pub added: u32,
    /// Rows the other device had changed more recently, now taken over.
    pub updated: u32,
    /// Of those, rows that had also been changed here since the last sync: a change made here was replaced by a newer
    /// one on the other device. (Changes made here that are newer than the other device's win without being counted
    /// here; the other device counts them as replaced.)
    pub replaced: u32,
    /// Rows of the other device that this device holds at a newer stamp. They are normally not sent at all.
    pub kept_yours: u32,
    /// Rows deleted because the other device deleted them.
    pub deleted: u32,
    /// Categories that were made one (same type and name on both devices).
    pub merged: u32,
    /// Rows removed because two rows said the same thing (the same entry, the same payment, the same month's budget).
    pub dropped: u32,
}

impl Summary {
    pub fn is_empty(&self) -> bool {
        *self == Summary::default()
    }
}

/// A stamp for a change that follows from other changes (a merge, a row moved to another category). It depends only on
/// the stamps it follows from, never on the clock, so both devices work out the same one.
pub fn derive(a: &Stamp, b: &Stamp) -> Stamp {
    let newest = std::cmp::max(a, b);
    let (ms, counter) = if newest.counter == u16::MAX { (newest.ms + 1, 0) } else { (newest.ms, newest.counter + 1) };
    Stamp { ms, counter, device: "00000000".to_string() }
}

/// The stamp for the marker of a row that lost to a duplicate (or for the merged-away category): one step after the stamp of the
/// row itself, whatever it lost to. It depends on that row alone, so two devices that pair up the duplicates in a different
/// order still write the same marker.
pub fn after(stamp: &Stamp) -> Stamp {
    derive(stamp, stamp)
}

/// The stamp of a row that a category merge moved to another category: the same time and counter with the highest device part,
/// so it is newer than the stamp it came from (and the other device, which moves its copy the same way, ends with the same
/// stamp), and two versions of the row that were different stay different and in the same order after the move. (A stamp
/// made by `derive` would give two versions of a row the same stamp whenever the merge is newer than both.)
pub fn moved(stamp: &Stamp) -> Stamp {
    Stamp { ms: stamp.ms, counter: stamp.counter, device: "ffffffff".to_string() }
}

fn iso(ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64).map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()).unwrap_or_default()
}

/// Whether `derived` is the stamp `moved` made from `original` (a merge moved the row), and not one that was kept or raised to
/// the stamp of a deletion.
fn moved_by_merge(derived: &str, original: &str) -> bool {
    derived != original && derived.ends_with("-ffffffff")
}

fn parse(text: &str) -> Result<Stamp, SyncError> {
    Stamp::parse(text).ok_or(SyncError::Invalid("stamp"))
}

struct Applier<'a> {
    tx: &'a Transaction<'a>,
    /// The time of the last sync with this device, in milliseconds, if there was one.
    since: Option<u64>,
    /// The deletion markers that came with this batch: `sync_id` to stamp.
    batch_markers: HashMap<String, String>,
    /// Categories merged away in this batch, whose budgets are moved at the end.
    merged_away: Vec<i64>,
    summary: Summary,
}

type Found = (i64, String);

/// What `check_recurring` reads of a recurring item: start date, unit, next due date, stamp, count, end date, active flag, sync id.
type RecurringItem = (String, String, String, String, i64, Option<String>, i64, String);

impl<'a> Applier<'a> {
    // One apply runs the same few statements for every row, so they are prepared once and kept (`prepare_cached`): parsing
    // them again for each of 60 000 transactions took most of the time of a first copy.
    fn row<T, P: rusqlite::Params, F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>>(&self, sql: &str, params: P, map: F) -> rusqlite::Result<T> {
        self.tx.prepare_cached(sql)?.query_row(params, map)
    }

    fn run<P: rusqlite::Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        self.tx.prepare_cached(sql)?.execute(params)
    }

    fn find(&self, table: Table, sync_id: &str) -> Result<Option<Found>, SyncError> {
        Ok(self.row(&format!("SELECT id, stamp FROM {} WHERE sync_id = ?1", table.name()), [sync_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    }

    fn marker(&self, sync_id: &str) -> Result<Option<String>, SyncError> {
        Ok(self.row("SELECT stamp FROM sync_deleted WHERE sync_id = ?1", [sync_id], |r| r.get(0)).optional()?)
    }

    fn put_marker(&self, table: Table, sync_id: &str, stamp: &str) -> Result<(), SyncError> {
        if self.marker(sync_id)?.is_some_and(|existing| existing.as_str() >= stamp) {
            return Ok(());
        }
        self.run("INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (?1, ?2, ?3)", params![sync_id, table.name(), stamp])?;
        Ok(())
    }

    /// The entries of the ledger that were linked to this expense lose the link when it is deleted, and take the stamp of the deletion, as the
    /// payments of a recurring item do (migration 0013): if the expense comes back on the other device with a newer edit, they are sent again.
    fn unlink_entries(&self, transaction_id: i64, deleted_at: &str) -> Result<(), SyncError> {
        self.run("UPDATE ledger_entries SET stamp = ?1 WHERE transaction_id = ?2 AND stamp < ?1", params![deleted_at, transaction_id])?;
        Ok(())
    }

    /// A newer row from the other device is about to replace this one: was it changed here since the last sync?
    fn note_overwrite(&mut self, local_stamp: &str) {
        let changed_here_since = match (self.since, Stamp::parse(local_stamp)) {
            (Some(since), Some(local)) => local.ms > since,
            _ => false,
        };
        if changed_here_since {
            self.summary.replaced += 1;
        }
    }

    fn remove_marker(&self, sync_id: &str) -> Result<(), SyncError> {
        self.run("DELETE FROM sync_deleted WHERE sync_id = ?1", [sync_id])?;
        Ok(())
    }

    /// The stamp for the marker of a row that lost to a duplicate. If the other device has deleted that same row for good (its marker
    /// is in this batch and covers the row), the marker is that one, so both devices end with the same; otherwise it is the stamp
    /// both can work out from the loser alone (`after`).
    fn loser_marker(&self, loser_sync_id: &str, loser_stamp: &str) -> String {
        match self.batch_markers.get(loser_sync_id) {
            Some(deleted) if deleted.as_str() > loser_stamp => deleted.clone(),
            _ => Stamp::parse(loser_stamp).map(|s| after(&s).format()).unwrap_or_else(|| loser_stamp.to_string()),
        }
    }

    /// After a write that the `updated_at` trigger may have touched: puts back the value this change is meant to have.
    fn set_updated_at(&self, table: Table, id: i64, wanted: &str) -> Result<(), SyncError> {
        let table = table.name();
        let current: String = self.row(&format!("SELECT updated_at FROM {table} WHERE id = ?1"), [id], |r| r.get(0))?;
        if current != wanted {
            self.run(&format!("UPDATE {table} SET updated_at = ?2 WHERE id = ?1"), params![id, wanted])?;
        }
        Ok(())
    }

    /// The local id of a category, following a merge: a category that was merged into another is that other one, and a
    /// row that pointed at it is moved (`true`) and takes the moved stamp, as it does on the device that merged it.
    fn category(&self, sync_id: &str) -> Result<(i64, bool), SyncError> {
        let mut current = sync_id.to_string();
        let mut moved = false;
        for _ in 0..8 {
            let row: Option<(i64, Option<String>, String)> = self.row("SELECT id, merged_into, stamp FROM categories WHERE sync_id = ?1", [&current], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()?;
            let (id, merged_into, _) = row.ok_or(SyncError::Invalid("reference"))?;
            match merged_into {
                None => return Ok((id, moved)),
                Some(next) => {
                    moved = true;
                    current = next;
                }
            }
        }
        Err(SyncError::Invalid("reference"))
    }

    /// The local id of a friend, following a merge: a friend who was merged into another is that other one, and a row that pointed at
    /// the first is moved (`true`) and takes the moved stamp, as it does on the device that merged them.
    fn person(&self, sync_id: &str) -> Result<(i64, bool), SyncError> {
        let mut current = sync_id.to_string();
        let mut moved = false;
        for _ in 0..8 {
            let row: Option<(i64, Option<String>)> =
                self.row("SELECT id, merged_into FROM people WHERE sync_id = ?1", [&current], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
            let (id, merged_into) = row.ok_or(SyncError::Invalid("reference"))?;
            match merged_into {
                None => return Ok((id, moved)),
                Some(next) => {
                    moved = true;
                    current = next;
                }
            }
        }
        Err(SyncError::Invalid("reference"))
    }

    /// The parts of an incoming bill (or the weights of a template) with the friends turned into local ids. Two entries that end up
    /// with one friend after a merge become one, with the amounts added. `true` when some friend was merged into another.
    fn local_parts<'p>(&self, parts: impl Iterator<Item = (&'p Option<String>, &'p Option<String>, i64)>) -> Result<(Vec<(Who, i64)>, bool), SyncError> {
        let mut out: Vec<(Who, i64)> = Vec::new();
        let mut any_moved = false;
        for (person, guest, amount) in parts {
            let who = match (person, guest) {
                (Some(sync_id), None) => {
                    let (id, moved) = self.person(sync_id)?;
                    any_moved |= moved;
                    Who::Person(id)
                }
                (None, Some(name)) => Who::Guest(name.clone()),
                _ => return Err(SyncError::Invalid("split")),
            };
            match out.iter_mut().find(|(w, _)| *w == who) {
                Some((_, total)) => *total += amount,
                None => out.push((who, amount)),
            }
        }
        Ok((out, any_moved))
    }

    fn settings(&mut self, rows: &[Row]) -> Result<(), SyncError> {
        for row in rows {
            let Row::Settings(r) = row else { return Err(SyncError::Invalid("row")) };
            let local: Option<String> = self.row("SELECT stamp FROM settings WHERE id = 1", [], |r| r.get(0)).optional()?;
            match local {
                Some(local) if r.stamp <= local => {
                    if r.stamp < local {
                        self.summary.kept_yours += 1;
                    }
                }
                Some(local) => {
                    self.note_overwrite(&local);
                    self.run("UPDATE settings SET locale = ?1, timezone = ?2, stamp = ?3 WHERE id = 1", params![r.locale, r.timezone, r.stamp])?;
                    let wanted = iso(parse(&r.stamp)?.ms);
                    let current: String = self.row("SELECT updated_at FROM settings WHERE id = 1", [], |r| r.get(0))?;
                    if current != wanted {
                        self.run("UPDATE settings SET updated_at = ?1 WHERE id = 1", [&wanted])?;
                    }
                    self.summary.updated += 1;
                }
                // a device without settings has not been set up; it takes the other's currency in the first copy
                None => return Err(SyncError::Invalid("settings")),
            }
        }
        Ok(())
    }

    // --- categories ---------------------------------------------------------------------------------------

    fn write_category(&mut self, existing: Option<&Found>, r: &CategoryRow) -> Result<i64, SyncError> {
        match existing {
            Some((id, _)) => {
                let kind: String = self.row("SELECT type FROM categories WHERE id = ?1", [id], |r| r.get(0))?;
                if kind != r.kind {
                    return Err(SyncError::Invalid("category"));
                }
                self.run(
                    "UPDATE categories SET name = ?2, deleted_at = ?3, merged_into = ?4, stamp = ?5, created_at = ?6, position = ?7 WHERE id = ?1",
                    params![id, r.name, r.deleted_at, r.merged_into, r.stamp, r.created_at, r.position],
                )?;
                self.set_updated_at(Table::Categories, *id, &r.updated_at)?;
                Ok(*id)
            }
            None => {
                self.run(
                    "INSERT INTO categories (sync_id, stamp, type, name, deleted_at, merged_into, position, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![r.sync_id, r.stamp, r.kind, r.name, r.deleted_at, r.merged_into, r.position, r.created_at, r.updated_at],
                )?;
                Ok(self.tx.last_insert_rowid())
            }
        }
    }

    /// Marks a local category as merged into another (the same type and name), with the stamp both devices work out.
    fn merge_away(&mut self, loser_id: i64, winner_sync_id: &str, star: &Stamp) -> Result<(), SyncError> {
        let when = iso(star.ms);
        self.run(
            "UPDATE categories SET deleted_at = ?2, merged_into = ?3, stamp = ?4 WHERE id = ?1",
            params![loser_id, when, winner_sync_id, star.format()],
        )?;
        self.set_updated_at(Table::Categories, loser_id, &when)?;
        self.summary.merged += 1;
        Ok(())
    }

    /// Moves everything that belongs to a category that was merged away to the one it was merged into.
    fn repoint(&mut self, loser: i64, winner: i64) -> Result<(), SyncError> {
        for (table, name) in [(Table::RecurringTransactions, "recurring_transactions"), (Table::Transactions, "transactions")] {
            let rows: Vec<(i64, String)> = self.tx.prepare_cached(&format!("SELECT id, stamp FROM {name} WHERE category_id = ?1"))?
                .query_map([loser], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            for (id, stamp) in rows {
                let new = moved(&parse(&stamp)?);
                self.run(&format!("UPDATE {name} SET category_id = ?2, stamp = ?3 WHERE id = ?1"), params![id, winner, new.format()])?;
                self.set_updated_at(table, id, &iso(new.ms))?;
                if table == Table::RecurringTransactions {
                }
            }
        }
        // the budgets of the merged-away category can clash with the winner's (the same month): that is settled once every row is in,
        // from the final versions, so both devices settle it from the same rows
        self.merged_away.push(loser);
        Ok(())
    }

    /// Moves the budgets of every category that was merged away to the category it was merged into. Two budgets for the same month
    /// become one: the newer stays (on a tie the lower id), and the other leaves a marker.
    fn settle_budgets(&mut self) -> Result<(), SyncError> {
        let losers = std::mem::take(&mut self.merged_away);
        for loser in losers {
            let sync_id: String = self.row("SELECT sync_id FROM categories WHERE id = ?1", [loser], |r| r.get(0))?;
            let (winner, _) = self.category(&sync_id)?;
            if winner == loser {
                continue;
            }
        let budgets: Vec<(i64, String, String, String)> = self.tx.prepare_cached("SELECT id, sync_id, stamp, effective_from FROM budgets WHERE category_id = ?1")?
            .query_map([loser], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<_, _>>()?;
        for (id, sync_id, stamp, month) in budgets {
            let other: Option<(i64, String, String)> = self.row("SELECT id, sync_id, stamp FROM budgets WHERE category_id = ?1 AND effective_from = ?2", params![winner, month], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .optional()?;
            match other {
                None => {
                    let new = moved(&parse(&stamp)?);
                    self.run("UPDATE budgets SET category_id = ?2, stamp = ?3 WHERE id = ?1", params![id, winner, new.format()])?;
                    self.set_updated_at(Table::Budgets, id, &iso(new.ms))?;
                }
                Some((other_id, other_sync_id, other_stamp)) => {
                    // both categories had a budget for this month: the newer one stays, as for any budget that two devices both made
                    let moved_wins = (stamp.as_str(), std::cmp::Reverse(sync_id.as_str())) > (other_stamp.as_str(), std::cmp::Reverse(other_sync_id.as_str()));
                    if moved_wins {
                        let marker = self.loser_marker(&other_sync_id, &other_stamp);
                        self.run("DELETE FROM budgets WHERE id = ?1", [other_id])?;
                        self.put_marker(Table::Budgets, &other_sync_id, &marker)?;
                        let new = moved(&parse(&stamp)?);
                        self.run("UPDATE budgets SET category_id = ?2, stamp = ?3 WHERE id = ?1", params![id, winner, new.format()])?;
                        self.set_updated_at(Table::Budgets, id, &iso(new.ms))?;
                    } else {
                        let marker = self.loser_marker(&sync_id, &stamp);
                        self.run("DELETE FROM budgets WHERE id = ?1", [id])?;
                        self.put_marker(Table::Budgets, &sync_id, &marker)?;
                    }
                    self.summary.dropped += 1;
                }
            }
        }
        }
        Ok(())
    }

    fn category_row(&mut self, r: &CategoryRow) -> Result<(), SyncError> {
        let existing = self.find(Table::Categories, &r.sync_id)?;
        if let Some((_, stamp)) = &existing {
            if r.stamp <= *stamp {
                if r.stamp < *stamp {
                    self.summary.kept_yours += 1;
                }
                return Ok(());
            }
        }
        let active = r.deleted_at.is_none() && r.merged_into.is_none();
        if active {
            let rival: Option<(i64, String, String)> = self.row(
                    "SELECT id, sync_id, stamp FROM categories WHERE type = ?1 AND name = ?2 AND deleted_at IS NULL AND sync_id != ?3",
                    params![r.kind, r.name, r.sync_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            if let Some((rival_id, rival_sync_id, rival_stamp)) = rival {
                // the same type and name on both devices: they become one category, the one with the lower id
                if r.sync_id < rival_sync_id {
                    let star = after(&parse(&rival_stamp)?);
                    self.merge_away(rival_id, &r.sync_id, &star)?;
                    let id = self.write_category(existing.as_ref(), r)?;
                    self.repoint(rival_id, id)?;
                } else {
                    let star = after(&parse(&r.stamp)?);
                    let mut merged = r.clone();
                    merged.deleted_at = Some(iso(star.ms));
                    merged.merged_into = Some(rival_sync_id.clone());
                    merged.stamp = star.format();
                    merged.updated_at = iso(star.ms);
                    let id = self.write_category(existing.as_ref(), &merged)?;
                    self.summary.merged += 1;
                    self.repoint(id, rival_id)?;
                }
                return Ok(());
            }
        }
        let id = self.write_category(existing.as_ref(), r)?;
        if let Some((_, local)) = &existing {
            self.note_overwrite(local);
            self.summary.updated += 1;
        } else {
            self.summary.added += 1;
        }
        if let Some(target) = &r.merged_into {
            // the other device merged this category away: what is here that pointed at it moves too
            let (winner, _) = self.category(target)?;
            self.repoint(id, winner)?;
        }
        Ok(())
    }

    /// The local active category that has the name this incoming row wants, if any: a collision (or the first sign of one).
    fn rival_of(&self, r: &CategoryRow) -> Result<Option<(i64, String, String)>, SyncError> {
        Ok(self.row(
                "SELECT id, sync_id, stamp FROM categories WHERE type = ?1 AND name = ?2 AND deleted_at IS NULL AND sync_id != ?3",
                params![r.kind, r.name, r.sync_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?)
    }

    fn categories(&mut self, rows: &[Row]) -> Result<(), SyncError> {
        let mut queue: Vec<&CategoryRow> = rows
            .iter()
            .map(|r| match r {
                Row::Category(c) => Ok(c),
                _ => Err(SyncError::Invalid("row")),
            })
            .collect::<Result<_, _>>()?;
        // the categories that stand come first, so the one a merged category points at is there
        queue.sort_by_key(|c| (c.merged_into.is_some(), c.sync_id.clone()));

        // Names are unique among active categories, and the other device may have renamed two of them in one go (one takes the
        // name the other just gave up). A row that wants a name held by a category that is itself about to change waits for it.
        let mut waiting: HashSet<String> = queue.iter().map(|c| c.sync_id.clone()).collect();
        while !queue.is_empty() {
            let mut deferred: Vec<&CategoryRow> = Vec::new();
            let mut progressed = false;
            for r in queue {
                let blocked = r.deleted_at.is_none()
                    && r.merged_into.is_none()
                    && self.rival_of(r)?.is_some_and(|(_, rival, _)| waiting.contains(&rival));
                if blocked {
                    deferred.push(r);
                } else {
                    waiting.remove(&r.sync_id);
                    self.category_row(r)?;
                    progressed = true;
                }
            }
            if !progressed {
                // two categories that swap names: the first waits no longer; the one holding its name steps aside under a name
                // no person can type (it gets its real name from its own row before this is over)
                let r = deferred.remove(0);
                if let Some((rival_id, rival, _)) = self.rival_of(r)? {
                    self.run("UPDATE categories SET name = ?2 WHERE id = ?1", params![rival_id, format!("\u{1}{rival}")])?;
                }
                waiting.remove(&r.sync_id);
                self.category_row(r)?;
            }
            queue = deferred;
        }
        Ok(())
    }

    // --- people -----------------------------------------------------------------------------------------------
    //
    // The rules are those of the categories: the newer whole row wins; two friends with the same name who are both active become one
    // (the lower id stays, the other is archived with `merged_into` and the stamp both devices work out), and what pointed at the one
    // that went is moved to the one that stays. A friend is never deleted, so there are no markers.

    fn write_person(&mut self, existing: Option<&Found>, r: &PersonRow) -> Result<i64, SyncError> {
        match existing {
            Some((id, _)) => {
                self.run(
                    "UPDATE people SET name = ?2, archived = ?3, merged_into = ?4, remind_on = ?5, remind_every_days = ?6, stamp = ?7, created_at = ?8 WHERE id = ?1",
                    params![id, r.name, r.archived, r.merged_into, r.remind_on, r.remind_every_days, r.stamp, r.created_at],
                )?;
                self.set_updated_at(Table::People, *id, &r.updated_at)?;
                Ok(*id)
            }
            None => {
                self.run(
                    "INSERT INTO people (sync_id, stamp, name, archived, merged_into, remind_on, remind_every_days, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![r.sync_id, r.stamp, r.name, r.archived, r.merged_into, r.remind_on, r.remind_every_days, r.created_at, r.updated_at],
                )?;
                Ok(self.tx.last_insert_rowid())
            }
        }
    }

    /// Marks a local friend as merged into another, with the stamp both devices work out.
    fn merge_away_person(&mut self, loser_id: i64, winner_sync_id: &str, star: &Stamp) -> Result<(), SyncError> {
        let when = iso(star.ms);
        self.run("UPDATE people SET archived = 1, merged_into = ?2, stamp = ?3 WHERE id = ?1", params![loser_id, winner_sync_id, star.format()])?;
        self.set_updated_at(Table::People, loser_id, &when)?;
        self.summary.merged += 1;
        Ok(())
    }

    /// Moves everything that belonged to a friend who was merged away to the one they were merged into: their ledger entries, and their
    /// parts of bills and of templates (a bill that held both gets one part with the amounts added). What moves takes the moved stamp.
    fn repoint_person(&mut self, loser: i64, winner: i64) -> Result<(), SyncError> {
        let entries: Vec<(i64, String)> = self.tx.prepare_cached("SELECT id, stamp FROM ledger_entries WHERE person_id = ?1")?
            .query_map([loser], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        for (id, stamp) in entries {
            let new = moved(&parse(&stamp)?);
            self.run("UPDATE ledger_entries SET person_id = ?2, stamp = ?3 WHERE id = ?1", params![id, winner, new.format()])?;
            self.set_updated_at(Table::LedgerEntries, id, &iso(new.ms))?;
        }
        for (lines, parent, amount_column, parent_table) in [
            ("transaction_split_lines", "transaction_id", "amount_minor", Table::Transactions),
            ("recurring_split_lines", "recurring_transaction_id", "weight", Table::RecurringTransactions),
        ] {
            let parts: Vec<(i64, i64, i64, String)> = self
                .tx
                .prepare_cached(&format!(
                    "SELECT l.id, l.{parent}, l.{amount_column}, p.stamp FROM {lines} l JOIN {} p ON p.id = l.{parent} WHERE l.person_id = ?1",
                    parent_table.name()
                ))?
                .query_map([loser], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<Result<_, _>>()?;
            for (id, bill, amount, stamp) in parts {
                let other: Option<i64> = self
                    .row(&format!("SELECT id FROM {lines} WHERE {parent} = ?1 AND person_id = ?2"), params![bill, winner], |r| r.get(0))
                    .optional()?;
                match other {
                    Some(other_id) => {
                        self.run(&format!("UPDATE {lines} SET {amount_column} = {amount_column} + ?2 WHERE id = ?1"), params![other_id, amount])?;
                        self.run(&format!("DELETE FROM {lines} WHERE id = ?1"), [id])?;
                    }
                    None => {
                        self.run(&format!("UPDATE {lines} SET person_id = ?2 WHERE id = ?1"), params![id, winner])?;
                    }
                }
                let new = moved(&parse(&stamp)?);
                self.run(&format!("UPDATE {} SET stamp = ?2 WHERE id = ?1", parent_table.name()), params![bill, new.format()])?;
                self.set_updated_at(parent_table, bill, &iso(new.ms))?;
            }
        }
        Ok(())
    }

    fn person_row(&mut self, r: &PersonRow) -> Result<(), SyncError> {
        let existing = self.find(Table::People, &r.sync_id)?;
        if let Some((_, stamp)) = &existing {
            if r.stamp <= *stamp {
                if r.stamp < *stamp {
                    self.summary.kept_yours += 1;
                }
                return Ok(());
            }
        }
        let active = r.archived == 0 && r.merged_into.is_none();
        if active {
            if let Some((rival_id, rival_sync_id, rival_stamp)) = self.person_rival(r)? {
                // the same name on both devices: they become one friend, the one with the lower id
                if r.sync_id < rival_sync_id {
                    let star = after(&parse(&rival_stamp)?);
                    self.merge_away_person(rival_id, &r.sync_id, &star)?;
                    let id = self.write_person(existing.as_ref(), r)?;
                    self.repoint_person(rival_id, id)?;
                } else {
                    let star = after(&parse(&r.stamp)?);
                    let mut merged = r.clone();
                    merged.archived = 1;
                    merged.merged_into = Some(rival_sync_id.clone());
                    merged.stamp = star.format();
                    merged.updated_at = iso(star.ms);
                    let id = self.write_person(existing.as_ref(), &merged)?;
                    self.summary.merged += 1;
                    self.repoint_person(id, rival_id)?;
                }
                return Ok(());
            }
        }
        let id = self.write_person(existing.as_ref(), r)?;
        if let Some((_, local)) = &existing {
            self.note_overwrite(local);
            self.summary.updated += 1;
        } else {
            self.summary.added += 1;
        }
        if let Some(target) = &r.merged_into {
            // the other device merged this friend away: what is here that pointed at them moves too
            let (winner, _) = self.person(target)?;
            self.repoint_person(id, winner)?;
        }
        Ok(())
    }

    /// The local active friend with the name this incoming row wants, if any: a collision (or the first sign of one).
    fn person_rival(&self, r: &PersonRow) -> Result<Option<(i64, String, String)>, SyncError> {
        Ok(self
            .row(
                "SELECT id, sync_id, stamp FROM people WHERE name = ?1 COLLATE NOCASE AND archived = 0 AND sync_id != ?2",
                params![r.name, r.sync_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?)
    }

    fn people(&mut self, rows: &[Row]) -> Result<(), SyncError> {
        let mut queue: Vec<&PersonRow> = rows
            .iter()
            .map(|r| match r {
                Row::Person(c) => Ok(c),
                _ => Err(SyncError::Invalid("row")),
            })
            .collect::<Result<_, _>>()?;
        // the friends who stand come first, so the one a merged friend points at is there
        queue.sort_by_key(|c| (c.merged_into.is_some(), c.sync_id.clone()));
        let mut waiting: HashSet<String> = queue.iter().map(|c| c.sync_id.clone()).collect();
        while !queue.is_empty() {
            let mut deferred: Vec<&PersonRow> = Vec::new();
            let mut progressed = false;
            for r in queue {
                let blocked = r.archived == 0
                    && r.merged_into.is_none()
                    && self.person_rival(r)?.is_some_and(|(_, rival, _)| waiting.contains(&rival));
                if blocked {
                    deferred.push(r);
                } else {
                    waiting.remove(&r.sync_id);
                    self.person_row(r)?;
                    progressed = true;
                }
            }
            if !progressed {
                // two friends who swap names: the one holding the name steps aside under a name no person can type
                let r = deferred.remove(0);
                if let Some((rival_id, rival, _)) = self.person_rival(r)? {
                    self.run("UPDATE people SET name = ?2 WHERE id = ?1", params![rival_id, format!("\u{1}{rival}")])?;
                }
                waiting.remove(&r.sync_id);
                self.person_row(r)?;
            }
            queue = deferred;
        }
        Ok(())
    }

    // --- recurring items ------------------------------------------------------------------------------------

    fn recurring(&mut self, rows: &[Row]) -> Result<(), SyncError> {
        let mut list: Vec<&RecurringRow> = rows
            .iter()
            .map(|r| match r {
                Row::Recurring(c) => Ok(c),
                _ => Err(SyncError::Invalid("row")),
            })
            .collect::<Result<_, _>>()?;
        list.sort_by(|a, b| a.sync_id.cmp(&b.sync_id));
        for r in list {
            let mut stamp = r.stamp.clone();
            let (category, moved_category) = self.category(&r.category)?;
            if moved_category {
                stamp = moved(&parse(&stamp)?).format();
            }
            let (template, moved_person) = match &r.split {
                Some(t) => {
                    let (lines, moved_person) = self.local_parts(t.parts.iter().map(|p| (&p.person, &p.guest, p.weight)))?;
                    (Some((t.mine, lines)), moved_person)
                }
                None => (None, false),
            };
            if moved_person {
                stamp = moved(&parse(&stamp)?).format();
            }
            if let Some(marker) = self.marker(&r.sync_id)? {
                if marker >= stamp {
                    self.summary.kept_yours += 1;
                    continue;
                }
                self.remove_marker(&r.sync_id)?;
            }
            let existing = self.find(Table::RecurringTransactions, &r.sync_id)?;
            if let Some((_, local)) = &existing {
                if stamp <= *local {
                    if stamp < *local {
                        self.summary.kept_yours += 1;
                    }
                    continue;
                }
            }
            if template.is_some() && self.is_income(category)? {
                return Err(SyncError::Invalid("split"));
            }
            let when = if stamp == r.stamp { r.updated_at.clone() } else { iso(parse(&stamp)?.ms) };
            let item_id = match &existing {
                Some((id, local)) => {
                    self.note_overwrite(local);
                    self.run(
                        "UPDATE recurring_transactions SET stamp = ?2, category_id = ?3, name = ?4, description = ?5, amount_minor = ?6, start_date = ?7,
                         interval_unit = ?8, interval_count = ?9, next_due_date = ?10, end_date = ?11, is_active = ?12, created_at = ?13 WHERE id = ?1",
                        params![id, stamp, category, r.name, r.description, r.amount_minor, r.start_date, r.interval_unit, r.interval_count, r.next_due_date, r.end_date, r.is_active, r.created_at],
                    )?;
                    self.set_updated_at(Table::RecurringTransactions, *id, &when)?;
                    self.summary.updated += 1;
                    *id
                }
                None => {
                    self.run(
                        "INSERT INTO recurring_transactions (sync_id, stamp, category_id, name, description, amount_minor, start_date, interval_unit,
                         interval_count, next_due_date, end_date, is_active, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                        params![r.sync_id, stamp, category, r.name, r.description, r.amount_minor, r.start_date, r.interval_unit, r.interval_count, r.next_due_date, r.end_date, r.is_active, r.created_at, when],
                    )?;
                    self.summary.added += 1;
                    self.tx.last_insert_rowid()
                }
            };
            // the template comes with the item and replaces this device's
            splits::replace_template(self.tx, item_id, template.as_ref().map(|(mine, lines)| (*mine, lines.as_slice()))).map_err(|_| SyncError::Invalid("split"))?;
        }
        Ok(())
    }

    fn is_income(&self, category_id: i64) -> Result<bool, SyncError> {
        Ok(self.row("SELECT type = 'INCOME' FROM categories WHERE id = ?1", [category_id], |r| r.get(0))?)
    }

    // --- budgets ----------------------------------------------------------------------------------------------

    fn budgets(&mut self, rows: &[Row]) -> Result<(), SyncError> {
        let mut list: Vec<&BudgetRow> = rows
            .iter()
            .map(|r| match r {
                Row::Budget(c) => Ok(c),
                _ => Err(SyncError::Invalid("row")),
            })
            .collect::<Result<_, _>>()?;
        list.sort_by(|a, b| a.sync_id.cmp(&b.sync_id));
        for r in list {
            let mut stamp = r.stamp.clone();
            let category = match &r.category {
                Some(sync_id) => {
                    let (id, moved_category) = self.category(sync_id)?;
                    if moved_category {
                        stamp = moved(&parse(&stamp)?).format();
                    }
                    Some(id)
                }
                None => None,
            };
            if let Some(marker) = self.marker(&r.sync_id)? {
                if marker >= stamp {
                    self.summary.kept_yours += 1;
                    continue;
                }
                self.remove_marker(&r.sync_id)?;
            }
            let existing = self.find(Table::Budgets, &r.sync_id)?;
            if let Some((_, local)) = &existing {
                if stamp <= *local {
                    if stamp < *local {
                        self.summary.kept_yours += 1;
                    }
                    continue;
                }
            }
            // another budget for the same month and category (made on the other device) is the same fact twice
            let rival: Option<(i64, String, String)> = match category {
                Some(c) => self.row(
                    "SELECT id, sync_id, stamp FROM budgets WHERE category_id = ?1 AND effective_from = ?2 AND sync_id != ?3",
                    params![c, r.effective_from, r.sync_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                ),
                None => self.row(
                    "SELECT id, sync_id, stamp FROM budgets WHERE scope = 'GLOBAL' AND type = ?1 AND effective_from = ?2 AND sync_id != ?3",
                    params![r.kind, r.effective_from, r.sync_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                ),
            }
            .optional()?;
            if let Some((rival_id, rival_sync_id, rival_stamp)) = rival {
                let incoming_wins = (stamp.as_str(), std::cmp::Reverse(r.sync_id.as_str())) > (rival_stamp.as_str(), std::cmp::Reverse(rival_sync_id.as_str()));
                self.summary.dropped += 1;
                if incoming_wins {
                    let marker = self.loser_marker(&rival_sync_id, &rival_stamp);
                    self.run("DELETE FROM budgets WHERE id = ?1", [rival_id])?;
                    self.put_marker(Table::Budgets, &rival_sync_id, &marker)?;
                } else {
                    if let Some((id, _)) = &existing {
                        self.run("DELETE FROM budgets WHERE id = ?1", [id])?;
                    }
                    self.put_marker(Table::Budgets, &r.sync_id, &after(&parse(&stamp)?).format())?;
                    continue;
                }
            }
            let when = if stamp == r.stamp { r.updated_at.clone() } else { iso(parse(&stamp)?.ms) };
            match &existing {
                Some((id, local)) => {
                    self.note_overwrite(local);
                    self.run(
                        "UPDATE budgets SET stamp = ?2, type = ?3, scope = ?4, category_id = ?5, amount_limit_minor = ?6, effective_from = ?7, created_at = ?8 WHERE id = ?1",
                        params![id, stamp, r.kind, r.scope, category, r.amount_limit_minor, r.effective_from, r.created_at],
                    )?;
                    self.set_updated_at(Table::Budgets, *id, &when)?;
                    self.summary.updated += 1;
                }
                None => {
                    self.run(
                        "INSERT INTO budgets (sync_id, stamp, type, scope, category_id, amount_limit_minor, effective_from, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![r.sync_id, stamp, r.kind, r.scope, category, r.amount_limit_minor, r.effective_from, r.created_at, when],
                    )?;
                    self.summary.added += 1;
                }
            }
        }
        Ok(())
    }

    // --- transactions ----------------------------------------------------------------------------------------

    fn transactions(&mut self, rows: &[Row]) -> Result<(), SyncError> {
        let mut list: Vec<&TransactionRow> = rows
            .iter()
            .map(|r| match r {
                Row::Transaction(c) => Ok(c),
                _ => Err(SyncError::Invalid("row")),
            })
            .collect::<Result<_, _>>()?;
        list.sort_by(|a, b| a.sync_id.cmp(&b.sync_id));
        for r in list {
            let mut stamp = r.stamp.clone();
            let (category, moved_category) = self.category(&r.category)?;
            if moved_category {
                stamp = moved(&parse(&stamp)?).format();
            }
            let (parts, moved_person) = self.local_parts(r.split.iter().map(|p| (&p.person, &p.guest, p.amount_minor)))?;
            if moved_person {
                stamp = moved(&parse(&stamp)?).format();
            }
            // updated_at follows the stamp only when the stamp was derived from a merge; any other stamp change keeps it
            let when = if moved_by_merge(&stamp, &r.stamp) { iso(parse(&stamp)?.ms) } else { r.updated_at.clone() };
            if let Some(marker) = self.marker(&r.sync_id)? {
                if marker >= stamp {
                    self.summary.kept_yours += 1;
                    continue;
                }
                self.remove_marker(&r.sync_id)?;
            }
            // a payment of a recurring item that was deleted here has lost its link and takes the stamp of the deletion, unless
            // it is newer (as the payments of an item take it on the device that deletes the item); one whose item is not here
            // and was never deleted is a broken reference
            let recurring: Option<i64> = match &r.recurring {
                Some(sync_id) => match self.find(Table::RecurringTransactions, sync_id)? {
                    Some((id, _)) => Some(id),
                    None => match self.marker(sync_id)? {
                        Some(deleted_at) => {
                            if deleted_at > stamp {
                                stamp = deleted_at;
                            }
                            None
                        }
                        None => return Err(SyncError::Invalid("reference")),
                    },
                },
                None => None,
            };
            let existing = self.find(Table::Transactions, &r.sync_id)?;
            if let Some((_, local)) = &existing {
                if stamp <= *local {
                    if stamp < *local {
                        self.summary.kept_yours += 1;
                    }
                    continue;
                }
            }
            // another row that says the same thing: the same entry (client_uuid) or the same payment of the same item
            let mut rivals: Vec<(i64, String, String)> = Vec::new();
            let existing_id = existing.as_ref().map(|(id, _)| *id).unwrap_or(0);
            if let Some(uuid) = &r.client_uuid {
                let mut stmt = self.tx.prepare_cached("SELECT id, sync_id, stamp FROM transactions WHERE client_uuid = ?1 AND sync_id != ?2")?;
                for rival in stmt.query_map(params![uuid, r.sync_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))? {
                    rivals.push(rival?);
                }
            }
            if let (Some(item), Some(due)) = (recurring, &r.recurring_due_date) {
                let mut stmt = self.tx.prepare_cached("SELECT id, sync_id, stamp FROM transactions WHERE recurring_transaction_id = ?1 AND recurring_due_date = ?2 AND sync_id != ?3")?;
                for rival in stmt.query_map(params![item, due, r.sync_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))? {
                    let rival = rival?;
                    if !rivals.iter().any(|(id, _, _)| *id == rival.0) {
                        rivals.push(rival);
                    }
                }
            }
            let incoming_loses = rivals
                .iter()
                .any(|(_, sync_id, rival_stamp)| (rival_stamp.as_str(), std::cmp::Reverse(sync_id.as_str())) > (stamp.as_str(), std::cmp::Reverse(r.sync_id.as_str())));
            if incoming_loses {
                let marker = after(&parse(&stamp)?).format();
                if existing_id != 0 {
                    self.unlink_entries(existing_id, &marker)?;
                    self.run("DELETE FROM transactions WHERE id = ?1", [existing_id])?;
                }
                self.put_marker(Table::Transactions, &r.sync_id, &marker)?;
                self.summary.dropped += 1;
                continue;
            }
            for (rival_id, rival_sync_id, rival_stamp) in &rivals {
                let marker = self.loser_marker(rival_sync_id, rival_stamp);
                self.unlink_entries(*rival_id, &marker)?;
                self.run("DELETE FROM transactions WHERE id = ?1", [rival_id])?;
                self.put_marker(Table::Transactions, rival_sync_id, &marker)?;
                self.summary.dropped += 1;
            }
            if !parts.is_empty() && self.is_income(category)? {
                return Err(SyncError::Invalid("split"));
            }
            let bill = match &existing {
                Some((id, local)) => {
                    self.note_overwrite(local);
                    self.run(
                        "UPDATE transactions SET stamp = ?2, category_id = ?3, recurring_transaction_id = ?4, recurring_due_date = ?5, client_uuid = ?6,
                         name = ?7, amount_minor = ?8, planned_amount_minor = ?9, payment_date = ?10, description = ?11, created_at = ?12 WHERE id = ?1",
                        params![id, stamp, category, recurring, r.recurring_due_date, r.client_uuid, r.name, r.amount_minor, r.planned_amount_minor, r.payment_date, r.description, r.created_at],
                    )?;
                    self.set_updated_at(Table::Transactions, *id, &when)?;
                    self.summary.updated += 1;
                    *id
                }
                None => {
                    self.run(
                        "INSERT INTO transactions (sync_id, stamp, category_id, recurring_transaction_id, recurring_due_date, client_uuid, name,
                         amount_minor, planned_amount_minor, payment_date, description, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![r.sync_id, stamp, category, recurring, r.recurring_due_date, r.client_uuid, r.name, r.amount_minor, r.planned_amount_minor, r.payment_date, r.description, r.created_at, when],
                    )?;
                    self.summary.added += 1;
                    self.tx.last_insert_rowid()
                }
            };
            // the parts come with the bill and replace this device's: the newer whole bill wins, parts included
            splits::replace_lines(self.tx, bill, &parts).map_err(|_| SyncError::Invalid("split"))?;
        }
        Ok(())
    }

    // --- the ledger -------------------------------------------------------------------------------------------
    //
    // Entries are rows of the ordinary kind: the newer wins, a deletion leaves a marker, and an entry follows its friend if the friend
    // was merged into another. Its link to the expense it came with is cleared by the foreign key when the expense goes, with no stamp
    // (as for a payment and its recurring item), so an entry that arrives linked to an expense this device knows as deleted is taken
    // with no link. Two entries from one bill code are the same entry: the newer stays and the other leaves a marker.

    fn ledger(&mut self, rows: &[Row]) -> Result<(), SyncError> {
        let mut list: Vec<&LedgerRow> = rows
            .iter()
            .map(|r| match r {
                Row::Ledger(c) => Ok(c),
                _ => Err(SyncError::Invalid("row")),
            })
            .collect::<Result<_, _>>()?;
        list.sort_by(|a, b| a.sync_id.cmp(&b.sync_id));
        for r in list {
            let mut stamp = r.stamp.clone();
            let (person, moved_person) = self.person(&r.person)?;
            if moved_person {
                stamp = moved(&parse(&stamp)?).format();
            }
            if let Some(marker) = self.marker(&r.sync_id)? {
                if marker >= stamp {
                    self.summary.kept_yours += 1;
                    continue;
                }
                self.remove_marker(&r.sync_id)?;
            }
            let existing = self.find(Table::LedgerEntries, &r.sync_id)?;
            if let Some((_, local)) = &existing {
                if stamp <= *local {
                    if stamp < *local {
                        self.summary.kept_yours += 1;
                    }
                    continue;
                }
            }
            // an entry linked to an expense that was deleted here arrives with no link, and takes the stamp of the deletion unless it is newer
            let transaction = match &r.transaction {
                Some(sync_id) => match self.find(Table::Transactions, sync_id)? {
                    Some((id, _)) => Some(id),
                    None => match self.marker(sync_id)? {
                        Some(deleted_at) => {
                            if deleted_at > stamp {
                                stamp = deleted_at;
                            }
                            None
                        }
                        None => return Err(SyncError::Invalid("reference")),
                    },
                },
                None => None,
            };
            if let Some((_, local)) = &existing {
                if stamp <= *local {
                    if stamp < *local {
                        self.summary.kept_yours += 1;
                    }
                    continue;
                }
            }
            if let Some(origin) = &r.origin_id {
                let rival: Option<(i64, String, String)> = self
                    .row("SELECT id, sync_id, stamp FROM ledger_entries WHERE origin_id = ?1 AND sync_id != ?2", params![origin, r.sync_id], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })
                    .optional()?;
                if let Some((rival_id, rival_sync_id, rival_stamp)) = rival {
                    let incoming_wins = (stamp.as_str(), std::cmp::Reverse(r.sync_id.as_str())) > (rival_stamp.as_str(), std::cmp::Reverse(rival_sync_id.as_str()));
                    self.summary.dropped += 1;
                    if incoming_wins {
                        let marker = self.loser_marker(&rival_sync_id, &rival_stamp);
                        self.run("DELETE FROM ledger_entries WHERE id = ?1", [rival_id])?;
                        self.put_marker(Table::LedgerEntries, &rival_sync_id, &marker)?;
                    } else {
                        if let Some((id, _)) = &existing {
                            self.run("DELETE FROM ledger_entries WHERE id = ?1", [id])?;
                        }
                        self.put_marker(Table::LedgerEntries, &r.sync_id, &after(&parse(&stamp)?).format())?;
                        continue;
                    }
                }
            }
            // updated_at follows the stamp only when the stamp was derived from a merge (a friend moved); a stamp raised to a deletion's keeps it
            let when = if moved_by_merge(&stamp, &r.stamp) { iso(parse(&stamp)?.ms) } else { r.updated_at.clone() };
            match &existing {
                Some((id, local)) => {
                    self.note_overwrite(local);
                    self.run(
                        "UPDATE ledger_entries SET stamp = ?2, person_id = ?3, kind = ?4, amount_minor = ?5, entry_date = ?6, note = ?7, transaction_id = ?8,
                         origin_id = ?9, created_at = ?10 WHERE id = ?1",
                        params![id, stamp, person, r.kind, r.amount_minor, r.entry_date, r.note, transaction, r.origin_id, r.created_at],
                    )?;
                    self.set_updated_at(Table::LedgerEntries, *id, &when)?;
                    self.summary.updated += 1;
                }
                None => {
                    self.run(
                        "INSERT INTO ledger_entries (sync_id, stamp, person_id, kind, amount_minor, entry_date, note, transaction_id, origin_id, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                        params![r.sync_id, stamp, person, r.kind, r.amount_minor, r.entry_date, r.note, transaction, r.origin_id, r.created_at, when],
                    )?;
                    self.summary.added += 1;
                }
            }
        }
        Ok(())
    }

    // --- documents ---------------------------------------------------------------------------------------------
    //
    // Only the rows. The file of a document goes by a conversation of its own after the rows, and is checked against the hash a row
    // carries. The contents of a document never change, so a row that names other contents under an id this device already holds
    // is not a document of this app, and the sync stops.

    fn documents(&mut self, rows: &[Row]) -> Result<(), SyncError> {
        let mut list: Vec<&DocumentRow> = rows
            .iter()
            .map(|r| match r {
                Row::Document(c) => Ok(c),
                _ => Err(SyncError::Invalid("row")),
            })
            .collect::<Result<_, _>>()?;
        list.sort_by(|a, b| a.sync_id.cmp(&b.sync_id));
        for r in list {
            if let Some(marker) = self.marker(&r.sync_id)? {
                if marker >= r.stamp {
                    self.summary.kept_yours += 1;
                    continue;
                }
                self.remove_marker(&r.sync_id)?;
            }
            match self.find(Table::Documents, &r.sync_id)? {
                Some((id, local)) => {
                    let same: i64 = self.row(
                        "SELECT media_type = ?2 AND size_bytes = ?3 AND sha256 = ?4 FROM documents WHERE id = ?1",
                        params![id, r.media_type, r.size_bytes, r.sha256],
                        |row| row.get(0),
                    )?;
                    if same != 1 {
                        return Err(SyncError::Invalid("document"));
                    }
                    if r.stamp <= local {
                        if r.stamp < local {
                            self.summary.kept_yours += 1;
                        }
                        continue;
                    }
                    self.note_overwrite(&local);
                    self.run(
                        "UPDATE documents SET stamp = ?2, name = ?3, document_date = ?4, keep_on_phone = ?5, created_at = ?6 WHERE id = ?1",
                        params![id, r.stamp, r.name, r.document_date, r.keep_on_phone, r.created_at],
                    )?;
                    self.set_updated_at(Table::Documents, id, &r.updated_at)?;
                    self.summary.updated += 1;
                }
                None => {
                    self.run(
                        "INSERT INTO documents (sync_id, stamp, name, media_type, size_bytes, sha256, document_date, keep_on_phone, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                        params![r.sync_id, r.stamp, r.name, r.media_type, r.size_bytes, r.sha256, r.document_date, r.keep_on_phone, r.created_at, r.updated_at],
                    )?;
                    self.summary.added += 1;
                }
            }
        }
        Ok(())
    }

    // --- links --------------------------------------------------------------------------------------------------
    //
    // A link lives and dies with its document and its transaction, recurring item or ledger entry: the foreign key removes it with them, and no
    // marker is made for that (the parent's marker says it). A link whose document or target this device knows as deleted is therefore
    // not taken, and costs nothing: the other device, which still holds the link, is sent the deletion of the parent in the same
    // sync and drops the link with it. A marker for a link means a person unlinked it.

    fn links(&mut self, rows: &[Row]) -> Result<(), SyncError> {
        let mut list: Vec<&DocumentLinkRow> = rows
            .iter()
            .map(|r| match r {
                Row::Link(c) => Ok(c),
                _ => Err(SyncError::Invalid("row")),
            })
            .collect::<Result<_, _>>()?;
        list.sort_by(|a, b| a.sync_id.cmp(&b.sync_id));
        for r in list {
            if let Some(marker) = self.marker(&r.sync_id)? {
                if marker >= r.stamp {
                    self.summary.kept_yours += 1;
                    continue;
                }
                self.remove_marker(&r.sync_id)?;
            }
            // what it points at: here, or deleted here (then the link is not taken), or unknown (a broken row, and the sync stops)
            let document = match self.find(Table::Documents, &r.document)? {
                Some((id, _)) => id,
                None if self.marker(&r.document)?.is_some() => {
                    self.summary.kept_yours += 1;
                    continue;
                }
                None => return Err(SyncError::Invalid("reference")),
            };
            let (column, target_table, target_id) = match (&r.transaction, &r.recurring, &r.ledger) {
                (Some(id), None, None) => ("transaction_id", Table::Transactions, id),
                (None, Some(id), None) => ("recurring_transaction_id", Table::RecurringTransactions, id),
                (None, None, Some(id)) => ("ledger_entry_id", Table::LedgerEntries, id),
                _ => return Err(SyncError::Invalid("link")),
            };
            let target = match self.find(target_table, target_id)? {
                Some((id, _)) => id,
                None if self.marker(target_id)?.is_some() => {
                    self.summary.kept_yours += 1;
                    continue;
                }
                None => return Err(SyncError::Invalid("reference")),
            };
            if let Some((id, local)) = self.find(Table::DocumentLinks, &r.sync_id)? {
                // a link never changes: the same document and the same target, or it is not a link of this app
                let same: i64 = self.row(&format!("SELECT COALESCE(document_id = ?2 AND {column} = ?3, 0) FROM document_links WHERE id = ?1"), params![id, document, target], |row| row.get(0)).optional()?.unwrap_or(0);
                if same != 1 {
                    return Err(SyncError::Invalid("link"));
                }
                if r.stamp > local {
                    self.run("UPDATE document_links SET stamp = ?2 WHERE id = ?1", params![id, r.stamp])?;
                }
                continue;
            }
            // another link of the same document and target (made on the other device) is the same fact twice: the newer stays
            let rival: Option<(i64, String, String)> = self
                .row(
                    &format!("SELECT id, sync_id, stamp FROM document_links WHERE document_id = ?1 AND {column} = ?2 AND sync_id != ?3"),
                    params![document, target, r.sync_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            if let Some((rival_id, rival_sync_id, rival_stamp)) = rival {
                let incoming_wins = (r.stamp.as_str(), std::cmp::Reverse(r.sync_id.as_str())) > (rival_stamp.as_str(), std::cmp::Reverse(rival_sync_id.as_str()));
                self.summary.dropped += 1;
                if incoming_wins {
                    let marker = self.loser_marker(&rival_sync_id, &rival_stamp);
                    self.run("DELETE FROM document_links WHERE id = ?1", [rival_id])?;
                    self.put_marker(Table::DocumentLinks, &rival_sync_id, &marker)?;
                } else {
                    self.put_marker(Table::DocumentLinks, &r.sync_id, &after(&parse(&r.stamp)?).format())?;
                    continue;
                }
            }
            let (transaction, recurring, ledger) = match column {
                "transaction_id" => (Some(target), None, None),
                "recurring_transaction_id" => (None, Some(target), None),
                _ => (None, None, Some(target)),
            };
            self.run(
                "INSERT INTO document_links (sync_id, stamp, document_id, transaction_id, recurring_transaction_id, ledger_entry_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![r.sync_id, r.stamp, document, transaction, recurring, ledger, r.created_at],
            )?;
            self.summary.added += 1;
        }
        Ok(())
    }

    // --- deletions --------------------------------------------------------------------------------------------

    fn delete_row(&mut self, table: Table, id: i64) -> Result<(), SyncError> {
        // deleting a recurring item clears the link of its payments by itself (ON DELETE SET NULL), as on the device that deleted it
        self.run(&format!("DELETE FROM {} WHERE id = ?1", table.name()), [id])?;
        Ok(())
    }

    fn markers(&mut self, table: Table, markers: &[Marker]) -> Result<(), SyncError> {
        let mut list: Vec<&Marker> = markers.iter().collect();
        list.sort_by(|a, b| a.sync_id.cmp(&b.sync_id));
        for m in list {
            match self.find(table, &m.sync_id)? {
                Some((id, stamp)) => {
                    if m.stamp > stamp {
                        if table == Table::RecurringTransactions {
                            // the payments lose their link to the item and take the stamp of the deletion, as on the device that deleted it
                            self.run("UPDATE transactions SET stamp = ?1 WHERE recurring_transaction_id = ?2 AND stamp < ?1", params![m.stamp, id])?;
                        }
                        if table == Table::Transactions {
                            // and so do the entries of the ledger that were linked to the expense
                            self.unlink_entries(id, &m.stamp)?;
                        }
                        self.delete_row(table, id)?;
                        self.put_marker(table, &m.sync_id, &m.stamp)?;
                        self.summary.deleted += 1;
                    } else {
                        self.summary.kept_yours += 1;
                    }
                }
                None => self.put_marker(table, &m.sync_id, &m.stamp)?,
            }
        }
        Ok(())
    }

    // --- after everything is in ---------------------------------------------------------------------------------

    /// An item whose next due date is not after a payment it already has (paid on the other device) moves on to its next
    /// due date, so that payment does not come due again. Both devices work this out from the same rows.
    fn check_recurring(&mut self) -> Result<(), SyncError> {
        // every active item, touched by this sync or not: the answer depends only on the rows as they are once everything is in, so
        // both devices come to the same one
        let ids: Vec<i64> = self.tx.prepare_cached("SELECT id FROM recurring_transactions WHERE is_active = 1 ORDER BY id")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        for id in ids {
            let item: Option<RecurringItem> = self.row(
                    "SELECT start_date, interval_unit, next_due_date, stamp, interval_count, end_date, is_active, sync_id FROM recurring_transactions WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
                )
                .optional()?;
            let Some((start, unit, due, stamp, count, end, active, _)) = item else { continue };
            if active == 0 {
                continue;
            }
            let latest: Option<(String, String)> = self.row(
                    "SELECT recurring_due_date, stamp FROM transactions WHERE recurring_transaction_id = ?1 AND recurring_due_date IS NOT NULL
                     ORDER BY recurring_due_date DESC LIMIT 1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((paid_due, paid_stamp)) = latest else { continue };
            if due > paid_due {
                continue;
            }
            let unit = match unit.as_str() {
                "DAY" => recurring::Unit::Day,
                "WEEK" => recurring::Unit::Week,
                "MONTH" => recurring::Unit::Month,
                "YEAR" => recurring::Unit::Year,
                _ => return Err(SyncError::Invalid("recurring item")),
            };
            let parse_date = |text: &str| chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| SyncError::Invalid("date"));
            let (start, paid) = (parse_date(&start)?, parse_date(&paid_due)?);
            let (next_text, still_active) = match recurring::next_after(start, unit, count as u32, paid) {
                Some(next) => (next.to_string(), end.as_deref().is_none_or(|e| next.to_string().as_str() <= e)),
                None => (due.clone(), false),
            };
            let star = derive(&parse(&stamp)?, &parse(&paid_stamp)?);
            self.run(
                "UPDATE recurring_transactions SET next_due_date = ?2, is_active = ?3, stamp = ?4 WHERE id = ?1",
                params![id, next_text, still_active, star.format()],
            )?;
            self.set_updated_at(Table::RecurringTransactions, id, &iso(star.ms))?;
        }
        Ok(())
    }
}

/// Applies what arrived, in one database transaction: either all of it or none. `now_ms` is this device's wall clock, and
/// `since_ms` the time of the last sync with this device (none if there was none), which tells a change made here since then
/// from an older one.
pub fn apply(conn: &mut Connection, incoming: &Incoming, now_ms: u64, since_ms: Option<u64>) -> Result<Summary, SyncError> {
    let newest_allowed = now_ms.saturating_add(MAX_CLOCK_LEAD_MS);
    let too_new = |text: &str| Stamp::parse(text).is_none_or(|s| s.ms > newest_allowed);
    for rows in incoming.rows.values() {
        if rows.iter().any(|r| too_new(r.stamp())) {
            return Err(SyncError::ClockFarOff);
        }
    }
    if incoming.markers.values().flatten().any(|m| too_new(&m.stamp)) {
        return Err(SyncError::ClockFarOff);
    }

    let tx = conn.transaction()?;
    tx.set_prepared_statement_cache_capacity(64);
    stamp::set_applying(&tx, true)?;
    let batch_markers = incoming.markers.values().flatten().map(|m| (m.sync_id.clone(), m.stamp.clone())).collect();
    let mut applier = Applier { tx: &tx, since: since_ms, batch_markers, merged_away: Vec::new(), summary: Summary::default() };
    let empty = Vec::new();
    applier.settings(incoming.rows.get(&Table::Settings).unwrap_or(&empty))?;
    applier.categories(incoming.rows.get(&Table::Categories).unwrap_or(&empty))?;
    applier.people(incoming.rows.get(&Table::People).unwrap_or(&empty))?;
    applier.recurring(incoming.rows.get(&Table::RecurringTransactions).unwrap_or(&empty))?;
    applier.budgets(incoming.rows.get(&Table::Budgets).unwrap_or(&empty))?;
    applier.transactions(incoming.rows.get(&Table::Transactions).unwrap_or(&empty))?;
    applier.ledger(incoming.rows.get(&Table::LedgerEntries).unwrap_or(&empty))?;
    applier.documents(incoming.rows.get(&Table::Documents).unwrap_or(&empty))?;
    applier.links(incoming.rows.get(&Table::DocumentLinks).unwrap_or(&empty))?;
    // payments before the recurring item they belong to: deleting the item gives the payments it still finds a newer stamp, which
    // must not happen to a payment that was deleted on the other device (and is in this same batch)
    // (a deleted document, transaction or recurring item takes its links with it, silently, by the foreign key)
    // (a deleted ledger entry takes its links with it, silently, and a deleted expense clears the link of its entries and gives them the
    // stamp of its deletion, which must not happen to an entry that was deleted on the other device and is in this same batch: so the
    // entries go first, as the payments go before their recurring item)
    for table in [Table::LedgerEntries, Table::Transactions, Table::Budgets, Table::RecurringTransactions, Table::DocumentLinks, Table::Documents] {
        applier.markers(table, incoming.markers.get(&table).map(Vec::as_slice).unwrap_or(&[]))?;
    }
    applier.settle_budgets()?;
    applier.check_recurring()?;
    let summary = applier.summary;
    // the parts of a bill travel with it and the checks of the database stood aside while they were written: the finished state must
    // still follow the rules (no more than the bill, no more than 30, only an expense, a template that gives someone a weight)
    if !splits::check(&tx).map_err(|_| SyncError::Internal)? {
        return Err(SyncError::Invalid("split"));
    }

    let broken: i64 = tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
    if broken != 0 {
        return Err(SyncError::Invalid("references"));
    }
    stamp::set_applying(&tx, false)?;
    stamp::catch_up(&tx).map_err(|_| SyncError::Internal)?;
    tx.commit()?;
    Ok(summary)
}

/// Whether this device holds no budget data yet, so it can be paired and given a full copy (docs/sync-design.md, section 5).
pub fn is_empty(conn: &Connection) -> Result<bool, SyncError> {
    for table in ["categories", "people", "ledger_entries", "budgets", "recurring_transactions", "transactions", "documents", "sync_deleted", "currency_changes"] {
        let count: i64 = conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        if count != 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The stamp of settings that have never been changed by a person: older than any real one, so the other device's win.
pub const UNSET_STAMP: &str = "0000000000000-0000-00000000";

/// A new device takes the other's currency and its record of changes as they are, with nothing to convert, and gives up
/// its own first-run settings (they get the oldest possible stamp, so the other device's shared settings replace them).
pub fn adopt_currency(conn: &mut Connection, currency: &str, generation: i64, changes: &[wire::CurrencyChange]) -> Result<(), SyncError> {
    if !is_empty(conn)? {
        return Err(SyncError::Currency);
    }
    let tx = conn.transaction()?;
    stamp::set_applying(&tx, true)?;
    tx.execute("UPDATE settings SET currency = ?1, currency_generation = ?2, stamp = ?3 WHERE id = 1", params![currency, generation, UNSET_STAMP])?;
    for change in changes {
        tx.execute(
            "INSERT INTO currency_changes (id, from_currency, to_currency, rate, stamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![change.generation, change.from, change.to, change.rate, change.stamp],
        )?;
    }
    stamp::set_applying(&tx, false)?;
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// hello, clocks and currency

pub fn hello(conn: &Connection, name: &str, app_version: &str) -> Result<wire::Hello, SyncError> {
    let device_id = stamp::device_id(conn).map_err(|_| SyncError::Internal)?;
    let (ms, counter): (i64, i64) = conn.query_row("SELECT last_ms, last_counter FROM sync_clock WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let (currency, generation): (String, i64) =
        conn.query_row("SELECT currency, currency_generation FROM settings WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(wire::Hello {
        device_id: device_id.clone(),
        name: name.to_string(),
        app_version: app_version.to_string(),
        schema: crate::migrate::latest_version(),
        protocol: wire::PROTOCOL,
        clock: Stamp::new(ms as u64, counter as u16, &device_id[..stamp::DEVICE_CHARS]).format(),
        currency,
        generation,
        since: None,
        mode: wire::Mode::Both,
    })
}

/// A reason to tell the person something, without stopping the sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warning {
    /// The clocks differ by this many seconds.
    ClockOff(u64),
}

/// What the other device's hello means for this sync: stop, or go on (maybe with a warning).
pub fn check_hello(mine: &wire::Hello, theirs: &wire::Hello, now_ms: u64) -> Result<Vec<Warning>, SyncError> {
    if theirs.protocol != mine.protocol {
        return Err(SyncError::ProtocolMismatch);
    }
    if theirs.schema != mine.schema {
        return Err(SyncError::SchemaMismatch { newer_here: mine.schema > theirs.schema });
    }
    if theirs.device_id == mine.device_id {
        return Err(SyncError::Invalid("device"));
    }
    let their_time = Stamp::parse(&theirs.clock).ok_or(SyncError::Invalid("clock"))?.ms;
    let gap = their_time.abs_diff(now_ms);
    if gap > MAX_CLOCK_LEAD_MS {
        return Err(SyncError::ClockFarOff);
    }
    // the generation of the currency: equal generations must mean the same currency
    if theirs.generation == mine.generation && theirs.currency != mine.currency {
        return Err(SyncError::Currency);
    }
    let mut warnings = Vec::new();
    if gap > CLOCK_WARNING_MS {
        warnings.push(Warning::ClockOff(gap / 1000));
    }
    Ok(warnings)
}

/// The currency changes the other device has not seen yet, in order.
pub fn changes_after(conn: &Connection, generation: i64) -> Result<Vec<wire::CurrencyChange>, SyncError> {
    let mut stmt = conn.prepare("SELECT id, from_currency, to_currency, rate, stamp FROM currency_changes WHERE id > ?1 ORDER BY id")?;
    let rows = stmt
        .query_map([generation], |r| Ok(wire::CurrencyChange { generation: r.get(0)?, from: r.get(1)?, to: r.get(2)?, rate: r.get(3)?, stamp: r.get(4)? }))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Brings this device's currency up to the other's: every missing change in turn, converting every amount here with the
/// same rate and rounding. All or nothing. `today` is for the history entry.
pub fn apply_currency(conn: &mut Connection, changes: &[wire::CurrencyChange], today: &str, now_ms: u64) -> Result<(), SyncError> {
    if changes.is_empty() {
        return Ok(());
    }
    let newest_allowed = now_ms.saturating_add(MAX_CLOCK_LEAD_MS);
    if changes.iter().any(|c| Stamp::parse(&c.stamp).is_none_or(|s| s.ms > newest_allowed)) {
        return Err(SyncError::ClockFarOff);
    }
    let tx = conn.transaction()?;
    stamp::set_applying(&tx, true)?;
    for change in changes {
        let recorded = crate::currency::Recorded {
            generation: change.generation,
            from: &change.from,
            to: &change.to,
            rate: change.rate.as_deref(),
            stamp: &change.stamp,
        };
        crate::currency::apply_recorded(&tx, &recorded, today)?;
    }
    stamp::set_applying(&tx, false)?;
    stamp::catch_up(&tx).map_err(|_| SyncError::Internal)?;
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// conflicts: the same thing changed on both devices since they last met

/// How many conflicts one sync asks the person about. The rest are settled by the newer change, and the screen says how many.
pub const MAX_CONFLICTS: usize = 200;

/// A row (or deletion) that both devices changed, differently, after their last sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub table: Table,
    pub id: String,
    pub mine_stamp: String,
    pub mine_deleted: bool,
    pub theirs_stamp: String,
    pub theirs_deleted: bool,
}

/// After which moment a change counts as a change since the last sync: the later of the two devices' last syncs, or none when
/// they never met (a first copy has nothing to fight over).
pub fn conflict_threshold(mine: Option<u64>, theirs: Option<u64>) -> Option<u64> {
    match (mine, theirs) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    }
}

/// The rows that both sides changed after `threshold` and that now differ, in the same order on both sides. At most
/// `MAX_CONFLICTS`; the second value is how many more there were.
pub fn conflicts(mine: &Digest, theirs: &Digest, tables: &[Table], threshold: Option<u64>) -> (Vec<Conflict>, usize) {
    let Some(threshold) = threshold else { return (Vec::new(), 0) };
    let after = |stamp: &str| Stamp::parse(stamp).is_some_and(|s| s.ms > threshold);
    let empty = Entries::new();
    let mut found = Vec::new();
    for table in Table::ALL {
        if !tables.contains(&table) {
            continue;
        }
        let (a, b) = (mine.0.get(&table).unwrap_or(&empty), theirs.0.get(&table).unwrap_or(&empty));
        for (id, (mine_stamp, mine_deleted)) in a {
            let Some((theirs_stamp, theirs_deleted)) = b.get(id) else { continue };
            if (mine_stamp, mine_deleted) == (theirs_stamp, theirs_deleted) || (*mine_deleted && *theirs_deleted) {
                continue;
            }
            if after(mine_stamp) && after(theirs_stamp) {
                found.push(Conflict {
                    table,
                    id: id.clone(),
                    mine_stamp: mine_stamp.clone(),
                    mine_deleted: *mine_deleted,
                    theirs_stamp: theirs_stamp.clone(),
                    theirs_deleted: *theirs_deleted,
                });
            }
        }
    }
    let more = found.len().saturating_sub(MAX_CONFLICTS);
    found.truncate(MAX_CONFLICTS);
    (found, more)
}

/// The tables the conflicts are in, in sending order.
pub fn conflict_tables(conflicts: &[Conflict]) -> Vec<Table> {
    Table::ALL.iter().copied().filter(|t| conflicts.iter().any(|c| c.table == *t)).collect()
}

/// This device's version of each conflicting row, for the other device to show next to its own.
pub fn conflict_rows(conn: &Connection, conflicts: &[Conflict]) -> Result<Vec<Message>, SyncError> {
    let mut ids: BTreeMap<Table, Vec<String>> = BTreeMap::new();
    for c in conflicts {
        ids.entry(c.table).or_default().push(c.id.clone());
    }
    rows_messages(conn, &conflict_tables(conflicts), &ids)
}

/// One field that differs between the two versions, as text. The name is a fixed word the screens translate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDiff {
    pub field: &'static str,
    pub mine: String,
    pub theirs: String,
}

/// What the person is shown for one conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictView {
    pub table: Table,
    pub id: String,
    pub title: String,
    pub mine_deleted: bool,
    pub theirs_deleted: bool,
    pub fields: Vec<FieldDiff>,
}

/// What the person chooses for a conflict: this device's version, or the other's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    Mine,
    Theirs,
}

fn category_name(conn: &Connection, sync_id: &str) -> String {
    conn.query_row("SELECT name FROM categories WHERE sync_id = ?1", [sync_id], |r| r.get::<_, String>(0)).unwrap_or_default()
}

fn name_of(conn: &Connection, sql: &str, sync_id: &str) -> String {
    conn.query_row(sql, [sync_id], |r| r.get::<_, String>(0)).unwrap_or_default()
}

/// "Anna 30.00, Bob 30.00": the parts of a bill, by the names of the friends on this device.
fn split_text(conn: &Connection, parts: &[SplitPartRow]) -> String {
    parts
        .iter()
        .map(|p| {
            let who = match (&p.person, &p.guest) {
                (Some(id), _) => name_of(conn, "SELECT name FROM people WHERE sync_id = ?1", id),
                (_, Some(name)) => name.clone(),
                _ => String::new(),
            };
            format!("{who} {}", crate::money::format_amount(p.amount_minor))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn template_text(conn: &Connection, template: &Option<TemplateRow>) -> String {
    let Some(t) = template else { return String::new() };
    let parts: Vec<String> = t
        .parts
        .iter()
        .map(|p| {
            let who = match (&p.person, &p.guest) {
                (Some(id), _) => name_of(conn, "SELECT name FROM people WHERE sync_id = ?1", id),
                (_, Some(name)) => name.clone(),
                _ => String::new(),
            };
            format!("{who} {}", p.weight)
        })
        .collect();
    format!("{} {}", t.mine, parts.join(" "))
}

fn money_text(minor: Option<i64>) -> String {
    minor.map(crate::money::format_amount).unwrap_or_default()
}

/// The facts of a row worth comparing, as (field, text).
fn facts(conn: &Connection, row: &Row) -> Vec<(&'static str, String)> {
    let some = |v: &Option<String>| v.clone().unwrap_or_default();
    match row {
        Row::Settings(r) => vec![("locale", r.locale.clone()), ("timezone", r.timezone.clone())],
        Row::Category(r) => vec![("name", r.name.clone()), ("type", r.kind.clone())],
        Row::Transaction(r) => vec![
            ("name", r.name.clone()),
            ("amount", money_text(Some(r.amount_minor))),
            ("date", r.payment_date.clone()),
            ("category", category_name(conn, &r.category)),
            ("description", some(&r.description)),
            ("split", split_text(conn, &r.split)),
        ],
        Row::Person(r) => vec![
            ("name", r.name.clone()),
            ("archived", if r.archived == 1 { "yes" } else { "no" }.to_string()),
            ("reminder", match (&r.remind_on, r.remind_every_days) {
                (Some(on), Some(every)) => format!("{on} / {every}"),
                (Some(on), None) => on.clone(),
                _ => String::new(),
            }),
        ],
        Row::Ledger(r) => vec![
            ("friend", name_of(conn, "SELECT name FROM people WHERE sync_id = ?1", &r.person)),
            ("kind", r.kind.clone()),
            ("amount", money_text(Some(r.amount_minor))),
            ("date", r.entry_date.clone()),
            ("note", some(&r.note)),
        ],
        Row::Budget(r) => vec![("amount", money_text(r.amount_limit_minor)), ("month", r.effective_from.clone()), ("type", r.kind.clone())],
        Row::Recurring(r) => vec![
            ("name", r.name.clone()),
            ("amount", money_text(r.amount_minor)),
            ("every", format!("{} {}", r.interval_count, r.interval_unit)),
            ("nextDue", r.next_due_date.clone()),
            ("endDate", some(&r.end_date)),
            ("active", if r.is_active == 1 { "yes" } else { "no" }.to_string()),
            ("category", category_name(conn, &r.category)),
            ("description", some(&r.description)),
            ("split", template_text(conn, &r.split)),
        ],
        Row::Document(r) => vec![
            ("name", r.name.clone()),
            ("date", r.document_date.clone()),
            ("keepOnPhone", if r.keep_on_phone == 1 { "yes" } else { "no" }.to_string()),
        ],
        Row::Link(r) => {
            let target = match (&r.transaction, &r.recurring, &r.ledger) {
                (Some(id), _, _) => name_of(conn, "SELECT name FROM transactions WHERE sync_id = ?1", id),
                (_, Some(id), _) => name_of(conn, "SELECT name FROM recurring_transactions WHERE sync_id = ?1", id),
                (_, _, Some(id)) => name_of(
                    conn,
                    "SELECT p.name FROM ledger_entries e JOIN people p ON p.id = e.person_id WHERE e.sync_id = ?1",
                    id,
                ),
                _ => String::new(),
            };
            vec![("document", name_of(conn, "SELECT name FROM documents WHERE sync_id = ?1", &r.document)), ("target", target)]
        }
    }
}

fn find_row<'a>(rows: &'a [Row], id: &str) -> Option<&'a Row> {
    rows.iter().find(|r| r.sync_id() == id)
}

/// Puts the two versions of each conflict side by side. `theirs` is what the other device sent for these ids.
pub fn describe(conn: &Connection, conflicts: &[Conflict], theirs: &Incoming) -> Result<Vec<ConflictView>, SyncError> {
    let mut out = Vec::new();
    for c in conflicts {
        let wanted: HashSet<&String> = std::iter::once(&c.id).collect();
        let mine_rows = if c.mine_deleted { Vec::new() } else { load_rows(conn, c.table, &wanted)? };
        let mine = mine_rows.first();
        let other = if c.theirs_deleted { None } else { theirs.rows.get(&c.table).and_then(|rows| find_row(rows, &c.id)) };
        let mine_facts = mine.map(|r| facts(conn, r)).unwrap_or_default();
        let their_facts = other.map(|r| facts(conn, r)).unwrap_or_default();
        let mut fields = Vec::new();
        if !c.mine_deleted && !c.theirs_deleted {
            for (field, a) in &mine_facts {
                let b = their_facts.iter().find(|(f, _)| f == field).map(|(_, v)| v.clone()).unwrap_or_default();
                if *a != b {
                    fields.push(FieldDiff { field, mine: a.clone(), theirs: b });
                }
            }
        } else if c.mine_deleted != c.theirs_deleted {
            // One side deleted it: show what the other side still has, so the person knows what they would keep.
            for (field, value) in mine_facts.iter().chain(their_facts.iter()) {
                let (mine, theirs) = if c.mine_deleted { (String::new(), value.clone()) } else { (value.clone(), String::new()) };
                fields.push(FieldDiff { field, mine, theirs });
            }
        }
        let title = [mine_facts.iter(), their_facts.iter()]
            .into_iter()
            .flatten()
            .find(|(f, v)| *f == "name" && !v.is_empty())
            .map(|(_, v)| v.clone())
            .or_else(|| mine_facts.iter().chain(their_facts.iter()).find(|(f, v)| *f == "month" && !v.is_empty()).map(|(_, v)| v.clone()))
            .or_else(|| mine_facts.iter().chain(their_facts.iter()).find(|(f, v)| *f == "friend" && !v.is_empty()).map(|(_, v)| v.clone()))
            .unwrap_or_default();
        out.push(ConflictView { table: c.table, id: c.id.clone(), title, mine_deleted: c.mine_deleted, theirs_deleted: c.theirs_deleted, fields });
    }
    Ok(out)
}

/// Carries out the person's choices on this device. The version that is meant to win is made the newer one: this device's gets a
/// fresh stamp, or its stamp is set to the oldest possible one so that the other device's version, which the exchange that
/// follows brings in the ordinary way, is the newer. Only stamps change here, never what a row says.
pub fn decide(conn: &mut Connection, conflicts: &[Conflict], choices: &[Choice]) -> Result<(), SyncError> {
    if conflicts.len() != choices.len() {
        return Err(SyncError::Invalid("choices"));
    }
    let tx = conn.transaction()?;
    stamp::set_applying(&tx, true)?;
    for (c, choice) in conflicts.iter().zip(choices) {
        let new = match choice {
            Choice::Mine => stamp::tick(&tx).map_err(|_| SyncError::Internal)?.format(),
            Choice::Theirs => UNSET_STAMP.to_string(),
        };
        if c.mine_deleted {
            tx.execute("UPDATE sync_deleted SET stamp = ?1 WHERE sync_id = ?2 AND kind = ?3", params![new, c.id, c.table.name()])?;
        } else if c.table == Table::Settings {
            tx.execute("UPDATE settings SET stamp = ?1 WHERE id = 1", [&new])?;
        } else {
            tx.execute(&format!("UPDATE {} SET stamp = ?1 WHERE sync_id = ?2", c.table.name()), params![new, c.id])?;
        }
    }
    stamp::set_applying(&tx, false)?;
    tx.commit()?;
    Ok(())
}

/// A value for a test or a report: the rows of every shared table as text, sorted, for comparing two devices.
#[cfg(test)]
pub fn snapshot(conn: &Connection) -> Result<Vec<String>, SyncError> {
    let mut out = Vec::new();
    for table in LISTED {
        let ids: HashSet<String> = {
            let mut stmt = conn.prepare(&format!("SELECT sync_id FROM {}", table.name()))?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<_, _>>()?;
            rows
        };
        let wanted: HashSet<&String> = ids.iter().collect();
        for row in load_rows(conn, table, &wanted)? {
            out.push(format!("{}:{}", table.name(), serde_json::to_string(&wire::row_to_value(&row)).map_err(|_| SyncError::Internal)?));
        }
        let all: HashSet<String> = digest(conn)?.0.get(&table).map(|e| e.iter().filter(|(_, (_, d))| *d).map(|(id, _)| id.clone()).collect()).unwrap_or_default();
        let wanted: HashSet<&String> = all.iter().collect();
        for m in load_markers(conn, table, &wanted)? {
            out.push(format!("{}:deleted:{}:{}", table.name(), m.sync_id, m.stamp));
        }
    }
    let settings: (String, String, String, i64) =
        conn.query_row("SELECT locale, timezone, currency, currency_generation FROM settings WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    out.push(format!("settings:{settings:?}"));
    out.sort();
    Ok(out)
}
