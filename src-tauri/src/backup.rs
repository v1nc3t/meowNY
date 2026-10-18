//! Encrypted backups: one file with a full copy of the data, locked with a passphrase chosen for
//! the backup.
//!
//! The file is not the database file. It holds every row as JSON, and the files of the documents if the person asked for them, sealed
//! with ChaCha20-Poly1305 under a key stretched from the passphrase with Argon2id. A backup is therefore independent of the vault's
//! keys and of the SQLCipher file layout, and opens on a clean install with a new keychain entry. The header (magic, format,
//! schema version of the app that wrote it, Argon2 cost, salt, nonce) is authenticated but not secret.
//!
//! ```text
//!  0..8   magic "MEOWNYBK"        26..42  salt
//!  8..10  format version (u16)    42..54  nonce (version 1; filler in version 2)
//! 10..14  schema version (u32)    54..    what is sealed
//! 14..26  Argon2 memory KiB, iterations, lanes (u32 each)
//! ```
//!
//! **Version 1** seals the rows as one message (ciphertext and a 16 byte tag). It cannot hold files, and is still read.
//!
//! **Version 2**, what is written now, seals a stream of chunks of 64 KiB (`stream.rs`) that holds records:
//!
//! ```text
//! S  u64 length, the rows as JSON     once, first
//! F  document id (16), u64 size, SHA-256 (32), the bytes     0 to many
//! E  the end                          once, last
//! ```
//!
//! Neither writing nor reading holds more than a chunk and one file at a time, so a library of many gigabytes is a backup like any other.
//! A backup made without documents has only `S` and `E`.
//!
//! Restoring replaces all data. The rows go into a scratch in-memory database built at the
//! backup's schema version, so every constraint is checked before the real data is touched; the
//! scratch is then migrated forward, which is how a backup from an older app version is read. Then the files are read from the stream
//! and sealed into the vault's folder under its own key (each only if a row of the backup names it, with that size and hash), and
//! only when the stream has ended whole are the rows written to the real database in one transaction. A backup from a newer app
//! version is refused.

mod stream;

#[cfg(test)]
mod stream_tests;
#[cfg(test)]
mod documents_tests;
#[cfg(test)]
mod debts_tests;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rusqlite::types::{Value as Sql, ValueRef};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use self::stream::{ChunkReader, ChunkWriter, CHUNK};
use crate::documents::library::Files;
use crate::documents::{DocId, MAX_DOCUMENT_BYTES};
use crate::keys::{self, SALT_LEN};
use crate::migrate;
use crate::sync;
use crate::vault::{AppError, Vault, MIN_PASSPHRASE_CHARS};

/// File extension of a backup, without the dot.
pub const EXTENSION: &str = "meowbak";

/// Every table in the schema, parents before children. A test fails when a migration adds a table
/// that is missing here, so a backup cannot silently leave data out.
const TABLES: &[&str] = &[
    "settings",
    "categories",
    "budgets",
    "people",
    "recurring_transactions",
    "recurring_split_lines",
    "transactions",
    "transaction_split_lines",
    "ledger_entries",
    "documents",
    "document_links",
    "audit_log",
    "sync_deleted",
    "currency_changes",
];

/// Tables a backup leaves out on purpose, and restoring leaves alone. `backup_schedule` belongs to
/// this device and holds the backup passphrase; `api_tokens` holds this device's access grants for
/// the command-line client, which a restore on another machine must not bring along; `sync_clock`
/// and `sync_peers` hold this device's id and the secrets it shares with a paired phone (after a
/// restore the device gets a new id and has to pair again, see `sync::stamp::after_restore`).
#[cfg(test)]
const EXCLUDED_TABLES: &[&str] = &["backup_schedule", "api_tokens", "sync_clock", "sync_peers"];

/// Restoring writes the history from the file, not the history the restore itself would trigger.
const HISTORY_TABLE: &str = "audit_log";

const MAGIC: &[u8; 8] = b"MEOWNYBK";
/// The rows sealed as one message. Still read, no longer written.
const FORMAT_1: u16 = 1;
/// A stream of sealed chunks that holds the rows and, if the person wanted them, the files.
const FORMAT: u16 = 2;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = 8 + 2 + 4 + 12 + SALT_LEN + NONCE_LEN;
/// A version 1 file this big is not a backup of one person's budget; refusing it keeps a wrong pick from filling memory. (A version 2
/// file has no such ceiling: it is never held in memory.)
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
/// The rows of version 2 are held in memory while they are read, so they have a ceiling of their own.
const MAX_SNAPSHOT_BYTES: u64 = 512 * 1024 * 1024;
const RECORD_SNAPSHOT: u8 = b'S';
const RECORD_FILE: u8 = b'F';
const RECORD_END: u8 = b'E';

/// Argon2id cost, written into the header so it can change later without breaking old backups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Kdf {
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
}

impl Kdf {
    #[cfg(not(test))]
    const DEFAULT: Kdf = Kdf { memory_kib: 64 * 1024, iterations: 3, lanes: 1 };
    /// Tests use a cheap setting so they stay fast.
    #[cfg(test)]
    const DEFAULT: Kdf = Kdf { memory_kib: 8, iterations: 1, lanes: 1 };

    /// A file is untrusted input: its cost settings must not be able to ask for gigabytes or hours.
    fn in_bounds(self) -> bool {
        (8..=256 * 1024).contains(&self.memory_kib)
            && (1..=10).contains(&self.iterations)
            && (1..=8).contains(&self.lanes)
            && self.memory_kib >= 8 * self.lanes
    }
}

/// One row, keyed by column name. Values are integers, text or null: that is all the schema holds.
pub type Row = Map<String, Value>;

/// All rows of all tables, as they were in one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The `user_version` of the database the rows came from.
    pub schema_version: u32,
    pub tables: BTreeMap<String, Vec<Row>>,
}

impl Snapshot {
    pub fn rows(&self, table: &str) -> &[Row] {
        self.tables.get(table).map(Vec::as_slice).unwrap_or(&[])
    }
}

fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT name FROM pragma_table_info(?1)")?;
    let names = stmt.query_map([table], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(names)
}

fn json_row(columns: &[String], row: &rusqlite::Row) -> rusqlite::Result<Row> {
    let mut out = Row::new();
    for (i, name) in columns.iter().enumerate() {
        let value = match row.get_ref(i)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(n) => Value::from(n),
            ValueRef::Text(_) => Value::String(row.get(i)?),
            other => return Err(rusqlite::Error::InvalidColumnType(i, name.clone(), other.data_type())),
        };
        out.insert(name.clone(), value);
    }
    Ok(out)
}

/// Reads every table in one read transaction, so the copy is consistent.
pub fn dump(conn: &Connection) -> Result<Snapshot, AppError> {
    let tx = conn.unchecked_transaction()?;
    let schema_version: u32 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let mut tables = BTreeMap::new();
    for name in TABLES {
        let columns = table_columns(&tx, name)?;
        if columns.is_empty() {
            continue;
        }
        let mut stmt = tx.prepare(&format!("SELECT * FROM {name} ORDER BY id"))?;
        let rows = stmt.query_map([], |row| json_row(&columns, row))?.collect::<Result<Vec<_>, _>>()?;
        tables.insert((*name).to_string(), rows);
    }
    Ok(Snapshot { schema_version, tables })
}

fn sql_value(value: &Value) -> Result<Sql, AppError> {
    match value {
        Value::Null => Ok(Sql::Null),
        Value::String(text) => Ok(Sql::Text(text.clone())),
        Value::Number(n) => n.as_i64().map(Sql::Integer).ok_or(AppError::InvalidBackup),
        _ => Err(AppError::InvalidBackup),
    }
}

fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn insert_rows(conn: &Connection, table: &str, columns: &[String], rows: &[Row]) -> Result<(), AppError> {
    for row in rows {
        // The names come from the file; only names the table really has are put into SQL.
        if row.keys().any(|key| !columns.contains(key)) {
            return Err(AppError::InvalidBackup);
        }
        let names: Vec<String> = row.keys().map(|k| quoted(k)).collect();
        let marks: Vec<String> = (1..=row.len()).map(|i| format!("?{i}")).collect();
        let sql = format!("INSERT INTO {} ({}) VALUES ({})", quoted(table), names.join(", "), marks.join(", "));
        let values = row.values().map(sql_value).collect::<Result<Vec<_>, _>>()?;
        conn.prepare_cached(&sql)
            .and_then(|mut stmt| stmt.execute(rusqlite::params_from_iter(values)))
            .map_err(|_| AppError::InvalidBackup)?;
    }
    Ok(())
}

/// Empties the database and fills it with the snapshot's rows, in one transaction: on any error
/// nothing changes. Constraints, foreign keys and checks all apply, so a bad file fails here.
fn replace_all(conn: &mut Connection, tables: &BTreeMap<String, Vec<Row>>) -> Result<(), AppError> {
    let tx = conn.transaction()?;
    // The rows keep the sync ids and stamps they come with, and the deletes below leave no markers: the file's
    // markers replace them. (A database from before the sync tables has no clock to switch.)
    let has_clock = !table_columns(&tx, "sync_clock")?.is_empty();
    if has_clock {
        sync::stamp::set_applying(&tx, true)?;
    }
    let mut present = Vec::new();
    for name in TABLES {
        let columns = table_columns(&tx, name)?;
        if !columns.is_empty() {
            present.push((*name, columns));
        }
    }
    // Rows for a table this schema version does not have cannot be placed anywhere.
    for (name, rows) in tables {
        if !rows.is_empty() && !present.iter().any(|(n, _)| n == name) {
            return Err(AppError::InvalidBackup);
        }
    }

    for (name, _) in present.iter().rev() {
        tx.execute(&format!("DELETE FROM {name}"), [])?;
    }
    for (name, columns) in &present {
        if *name == HISTORY_TABLE {
            // The triggers wrote history for the rows just inserted; the file's history replaces it.
            tx.execute(&format!("DELETE FROM {HISTORY_TABLE}"), [])?;
        }
        insert_rows(&tx, name, columns, tables.get(*name).map(Vec::as_slice).unwrap_or(&[]))?;
    }
    // The checks of the parts against their bills stood aside while the rows were written in some order: the file is refused whole if the
    // finished state breaks them.
    let has_parts = present.iter().any(|(name, _)| *name == "recurring_split_lines");
    if has_parts && !crate::debts::splits::check(&tx)? {
        return Err(AppError::InvalidBackup);
    }
    if has_clock {
        sync::stamp::set_applying(&tx, false)?;
    }
    tx.commit()?;
    Ok(())
}

/// Checks the snapshot in a scratch database at its own schema version, which tests every constraint, and brings it to the
/// current one. Nothing real is touched. The result is what `commit` writes, and what the files of a backup are matched against.
pub fn prepare(snapshot: &Snapshot) -> Result<Snapshot, AppError> {
    if snapshot.schema_version == 0 || snapshot.schema_version > migrate::latest_version() {
        return Err(AppError::DatabaseTooNew);
    }
    let mut scratch = Connection::open_in_memory()?;
    scratch.execute_batch("PRAGMA foreign_keys = ON;")?;
    migrate::run_up_to(&mut scratch, snapshot.schema_version).map_err(|_| AppError::Internal)?;
    replace_all(&mut scratch, &snapshot.tables)?;
    migrate::run(&mut scratch).map_err(|_| AppError::Internal)?;
    dump(&scratch)
}

/// Replaces everything in `target` with a prepared snapshot, in one transaction.
pub fn commit(target: &mut Connection, prepared: &Snapshot) -> Result<(), AppError> {
    replace_all(target, &prepared.tables)?;
    // The data is another device's history now: see `after_restore`.
    sync::stamp::after_restore(target)
}

/// Replaces everything in `target` with the snapshot. An older snapshot is migrated forward first. (The rows only; the files of a
/// backup are read from the stream by `Backup::restore_files` between the two steps.)
pub fn restore_snapshot(target: &mut Connection, snapshot: &Snapshot) -> Result<(), AppError> {
    commit(target, &prepare(snapshot)?)
}

pub(crate) fn check_passphrase(passphrase: &str) -> Result<(), AppError> {
    if passphrase.chars().count() >= MIN_PASSPHRASE_CHARS {
        Ok(())
    } else {
        Err(AppError::InvalidInput)
    }
}

fn stretch(passphrase: &str, salt: &[u8], kdf: Kdf) -> Result<Zeroizing<[u8; 32]>, AppError> {
    let params = Params::new(kdf.memory_kib, kdf.iterations, kdf.lanes, Some(32)).map_err(|_| AppError::InvalidBackup)?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, key.as_mut())
        .map_err(|_| AppError::Internal)?;
    Ok(key)
}

fn header(format: u16, schema_version: u32, kdf: Kdf, salt: &[u8; SALT_LEN], nonce: &[u8; NONCE_LEN]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&format.to_be_bytes());
    out.extend_from_slice(&schema_version.to_be_bytes());
    for part in [kdf.memory_kib, kdf.iterations, kdf.lanes] {
        out.extend_from_slice(&part.to_be_bytes());
    }
    out.extend_from_slice(salt);
    out.extend_from_slice(nonce);
    out
}

// ---------------------------------------------------------------------------------------------------------------
// writing

/// Where the files of the documents come from when a backup is written.
pub trait FileSource {
    /// The bytes of the file of a document, if this device holds a whole one that matches the row (`size` and `sha256`); otherwise
    /// None, and the document is left out of the backup's files and counted. An error stops the backup.
    fn file(&mut self, id: &DocId, size: u64, sha256: &[u8; 32]) -> Result<Option<Zeroizing<Vec<u8>>>, AppError>;
}

/// The files of the vault, read one at a time, each under a short hold of the vault, so the app stays usable while a big backup is
/// written.
pub struct VaultFiles<'a>(pub &'a Vault);

impl FileSource for VaultFiles<'_> {
    fn file(&mut self, id: &DocId, size: u64, sha256: &[u8; 32]) -> Result<Option<Zeroizing<Vec<u8>>>, AppError> {
        self.0.with_files(|_, files| Ok(files.store.get(files.keys, id, size, sha256).ok()))
    }
}

/// What a backup holds of the documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    /// Documents whose file is in the backup.
    pub documents: u32,
    /// Documents that have a row in it and no file: this device has none, or its file does not match the row.
    pub missing: u32,
}

/// The documents of a snapshot: the id, the size and the hash of each file, in the order of the ids.
fn document_rows(snapshot: &Snapshot) -> Result<Vec<(DocId, u64, [u8; 32])>, AppError> {
    let mut rows = Vec::new();
    for row in snapshot.rows("documents") {
        let text = |key: &str| row.get(key).and_then(Value::as_str);
        let id = text("sync_id").and_then(|t| DocId::parse(t).ok());
        let size = row.get("size_bytes").and_then(Value::as_u64).filter(|n| (1..=MAX_DOCUMENT_BYTES).contains(n));
        let sha = text("sha256").and_then(keys::hex_decode::<32>);
        let (Some(id), Some(size), Some(sha)) = (id, size, sha) else { return Err(AppError::InvalidBackup) };
        rows.push((id, size, sha));
    }
    rows.sort_by_key(|(id, _, _)| *id.bytes());
    Ok(rows)
}

/// Writes a version 2 backup to `out`: the rows, then (if `files` is given) the file of each document that has one, then the end.
pub fn write_stream<W: Write>(mut out: W, snapshot: &Snapshot, passphrase: &str, files: Option<&mut dyn FileSource>) -> Result<(W, Written), AppError> {
    check_passphrase(passphrase)?;
    let rows = Zeroizing::new(serde_json::to_vec(&snapshot.tables).map_err(|_| AppError::Internal)?);
    if rows.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(AppError::TooLarge);
    }
    let documents = if files.is_some() { document_rows(snapshot)? } else { Vec::new() };

    let kdf = Kdf::DEFAULT;
    let salt = keys::random_bytes::<SALT_LEN>()?;
    // version 2 has no use for a nonce of its own (the chunks have theirs); the field keeps the layout of version 1
    let filler = keys::random_bytes::<NONCE_LEN>()?;
    let head = header(FORMAT, snapshot.schema_version, kdf, &salt, &filler);
    out.write_all(&head).map_err(|_| AppError::Internal)?;
    let key = stretch(passphrase, &salt, kdf)?;

    let mut stream = ChunkWriter::new(out, &key, head);
    stream.write(&[RECORD_SNAPSHOT])?;
    stream.write(&(rows.len() as u64).to_be_bytes())?;
    stream.write(&rows)?;
    let mut written = Written::default();
    if let Some(source) = files {
        for (id, size, sha256) in documents {
            match source.file(&id, size, &sha256)? {
                Some(bytes) if bytes.len() as u64 == size => {
                    stream.write(&[RECORD_FILE])?;
                    stream.write(id.bytes())?;
                    stream.write(&size.to_be_bytes())?;
                    stream.write(&sha256)?;
                    stream.write(&bytes)?;
                    written.documents += 1;
                }
                _ => written.missing += 1,
            }
        }
    }
    stream.write(&[RECORD_END])?;
    Ok((stream.finish()?, written))
}

/// Encrypts a snapshot under the passphrase, without documents. The passphrase is the one chosen for this backup, not the app's.
pub fn seal(snapshot: &Snapshot, passphrase: &str) -> Result<Vec<u8>, AppError> {
    Ok(write_stream(Vec::new(), snapshot, passphrase, None)?.0)
}

/// What a file for a backup is opened with: owner only.
fn private_options() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

/// Writes a backup to `path`, streamed, readable by the owner only. It goes through a temporary file, so a crash, a full disk or a
/// vault that locks half way never leaves a half-written backup under the real name.
pub fn create_file(path: &Path, snapshot: &Snapshot, passphrase: &str, files: Option<&mut dyn FileSource>) -> Result<Written, AppError> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let file = private_options().open(&tmp).map_err(|_| AppError::Internal)?;
    let written = write_stream(std::io::BufWriter::new(file), snapshot, passphrase, files).and_then(|(out, written)| {
        let file = out.into_inner().map_err(|_| AppError::Internal)?;
        file.sync_all().map_err(|_| AppError::Internal)?;
        Ok(written)
    });
    match written {
        Ok(written) => {
            fs::rename(&tmp, path).map_err(|_| {
                let _ = fs::remove_file(&tmp);
                AppError::Internal
            })?;
            Ok(written)
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// reading

/// Where a backup is read from.
#[derive(Debug, Clone, Copy)]
pub enum Source<'a> {
    Bytes(&'a [u8]),
    File(&'a Path),
}

impl<'a> From<&'a [u8]> for Source<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Source::Bytes(bytes)
    }
}

impl<'a> From<&'a Vec<u8>> for Source<'a> {
    fn from(bytes: &'a Vec<u8>) -> Self {
        Source::Bytes(bytes)
    }
}

impl<'a> From<&'a Path> for Source<'a> {
    fn from(path: &'a Path) -> Self {
        Source::File(path)
    }
}

impl<'a> From<&'a PathBuf> for Source<'a> {
    fn from(path: &'a PathBuf) -> Self {
        Source::File(path)
    }
}

/// What the header says, checked.
struct Head {
    format: u16,
    schema_version: u32,
    kdf: Kdf,
    salt: [u8; SALT_LEN],
}

fn be_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
}

/// Checks the magic and the versions, and the cost settings, which are untrusted: `InvalidBackup` for a file that is not a backup,
/// `DatabaseTooNew` for one a newer app wrote.
fn parse_header(head: &[u8]) -> Result<Head, AppError> {
    if head.len() < HEADER_LEN || &head[..8] != MAGIC {
        return Err(AppError::InvalidBackup);
    }
    let format = u16::from_be_bytes([head[8], head[9]]);
    if format > FORMAT {
        return Err(AppError::DatabaseTooNew);
    }
    if format == 0 {
        return Err(AppError::InvalidBackup);
    }
    let schema_version = be_u32(head, 10);
    if schema_version > migrate::latest_version() {
        return Err(AppError::DatabaseTooNew);
    }
    if schema_version == 0 {
        return Err(AppError::InvalidBackup);
    }
    let kdf = Kdf { memory_kib: be_u32(head, 14), iterations: be_u32(head, 18), lanes: be_u32(head, 22) };
    if !kdf.in_bounds() {
        return Err(AppError::InvalidBackup);
    }
    Ok(Head { format, schema_version, kdf, salt: head[26..26 + SALT_LEN].try_into().expect("a salt") })
}

fn parse_tables(plain: &[u8]) -> Result<BTreeMap<String, Vec<Row>>, AppError> {
    let tables: BTreeMap<String, Vec<Row>> = serde_json::from_slice(plain).map_err(|_| AppError::InvalidBackup)?;
    if tables.keys().any(|name| !TABLES.contains(&name.as_str())) {
        return Err(AppError::InvalidBackup);
    }
    Ok(tables)
}

/// A backup that is open: the passphrase was right and the rows are read. The files, if there are any, are still in the stream.
pub struct Backup {
    snapshot: Snapshot,
    /// The rest of a version 2 stream; none for version 1, and none once the end has been read.
    records: Option<ChunkReader<Box<dyn Read>>>,
}

impl Backup {
    /// Checks the version, stretches the passphrase (slowly), opens the stream and reads the rows. Fails with `InvalidBackup` for a
    /// file that is not a backup, `DatabaseTooNew` for one a newer app wrote, and `WrongCredentials` for a wrong passphrase or a
    /// file changed at its start (those cannot be told apart).
    pub fn open<'a>(source: impl Into<Source<'a>>, passphrase: &str) -> Result<Backup, AppError> {
        match source.into() {
            Source::Bytes(bytes) => Backup::from_reader(Box::new(std::io::Cursor::new(bytes.to_vec())), bytes.len() as u64, passphrase),
            Source::File(path) => {
                let file = fs::File::open(path).map_err(|_| AppError::NotFound)?;
                let length = file.metadata().map_err(|_| AppError::Internal)?.len();
                Backup::from_reader(Box::new(std::io::BufReader::new(file)), length, passphrase)
            }
        }
    }

    fn from_reader(mut input: Box<dyn Read>, length: u64, passphrase: &str) -> Result<Backup, AppError> {
        if length < (HEADER_LEN + TAG_LEN) as u64 {
            return Err(AppError::InvalidBackup);
        }
        let mut head_bytes = vec![0u8; HEADER_LEN];
        input.read_exact(&mut head_bytes).map_err(|_| AppError::InvalidBackup)?;
        let head = parse_header(&head_bytes)?;
        let key = stretch(passphrase, &head.salt, head.kdf)?;
        let sealed_len = length - HEADER_LEN as u64;

        if head.format == FORMAT_1 {
            if length > MAX_FILE_BYTES {
                return Err(AppError::InvalidBackup);
            }
            let mut sealed = Vec::new();
            input.read_to_end(&mut sealed).map_err(|_| AppError::Internal)?;
            let plain = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()))
                .decrypt(Nonce::from_slice(&head_bytes[26 + SALT_LEN..]), Payload { msg: &sealed, aad: &head_bytes })
                .map(Zeroizing::new)
                .map_err(|_| AppError::WrongCredentials)?;
            let tables = parse_tables(&plain)?;
            return Ok(Backup { snapshot: Snapshot { schema_version: head.schema_version, tables }, records: None });
        }

        let mut reader = ChunkReader::new(input, &key, head_bytes, sealed_len);
        let mut tag = [0u8; 1];
        reader.take(&mut tag)?;
        if tag[0] != RECORD_SNAPSHOT {
            return Err(AppError::InvalidBackup);
        }
        let mut size = [0u8; 8];
        reader.take(&mut size)?;
        let size = u64::from_be_bytes(size);
        if size > MAX_SNAPSHOT_BYTES {
            return Err(AppError::InvalidBackup);
        }
        let rows = reader.take_vec(size)?;
        let tables = parse_tables(&rows)?;
        Ok(Backup { snapshot: Snapshot { schema_version: head.schema_version, tables }, records: Some(reader) })
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Goes through the records that follow the rows. `on_file` is given the stream, the document and the size and hash the record
    /// announces, and must read exactly `size` bytes. Ends at the end record, which must be the last thing in the stream.
    fn each_file(&mut self, mut on_file: impl FnMut(&mut ChunkReader<Box<dyn Read>>, DocId, u64, [u8; 32]) -> Result<(), AppError>) -> Result<(), AppError> {
        let Some(reader) = self.records.as_mut() else { return Ok(()) };
        loop {
            let mut tag = [0u8; 1];
            reader.take(&mut tag)?;
            match tag[0] {
                RECORD_END => {
                    reader.finish()?;
                    self.records = None;
                    return Ok(());
                }
                RECORD_FILE => {
                    let mut head = [0u8; 16 + 8 + 32];
                    reader.take(&mut head)?;
                    let id = DocId::from_bytes(head[..16].try_into().expect("16 bytes"));
                    let size = u64::from_be_bytes(head[16..24].try_into().expect("8 bytes"));
                    let sha256: [u8; 32] = head[24..].try_into().expect("32 bytes");
                    if !(1..=MAX_DOCUMENT_BYTES).contains(&size) {
                        return Err(AppError::InvalidBackup);
                    }
                    on_file(reader, id, size, sha256)?;
                }
                _ => return Err(AppError::InvalidBackup),
            }
        }
    }

    /// Reads the files out of the stream and seals each into the vault's folder under its own key, to the end of the stream. A file is
    /// taken only if a row of `prepared` names it with this size and hash, and only once; it is checked against its hash as it is
    /// written and not kept if it does not match. Returns how many were written. Call it before `commit`: if it fails the data has not
    /// been touched, and what it wrote is for the next check of the folder to remove.
    pub fn restore_files(&mut self, files: &Files, prepared: &Snapshot) -> Result<u32, AppError> {
        let expected: BTreeMap<[u8; 16], (u64, [u8; 32])> = document_rows(prepared)?.into_iter().map(|(id, size, sha)| (*id.bytes(), (size, sha))).collect();
        let mut seen = HashSet::new();
        let mut count = 0;
        self.each_file(|reader, id, size, sha256| {
            if expected.get(id.bytes()) != Some(&(size, sha256)) || !seen.insert(*id.bytes()) {
                return Err(AppError::InvalidBackup);
            }
            // from the first byte: what an earlier, stopped transfer left of this file is not used
            files.store.discard_unfinished(files.keys, &id);
            let result = (|| {
                let mut incoming = files.store.begin_incoming(files.keys, &id, size)?;
                let mut piece = Zeroizing::new(vec![0u8; CHUNK]);
                let mut left = size;
                while left > 0 {
                    let step = left.min(CHUNK as u64) as usize;
                    reader.take(&mut piece[..step])?;
                    incoming.write(&piece[..step])?;
                    left -= step as u64;
                }
                incoming.finish(&sha256).map_err(|e| match e {
                    crate::documents::DocError::Mismatch => AppError::InvalidBackup,
                    other => other.into(),
                })
            })();
            if result.is_err() {
                files.store.discard_unfinished(files.keys, &id);
            }
            result?;
            count += 1;
            Ok(())
        })?;
        Ok(count)
    }

    /// Reads to the end without keeping the files, so a cut or damaged backup is found. Gives the rows back.
    pub fn finish(mut self) -> Result<Snapshot, AppError> {
        self.each_file(|reader, _, size, _| reader.skip(size))?;
        Ok(self.snapshot)
    }
}

/// Decrypts a backup that is in memory and gives its rows; the stream is read to its end, so a damaged one is refused.
pub fn open(bytes: &[u8], passphrase: &str) -> Result<Snapshot, AppError> {
    Backup::open(bytes, passphrase)?.finish()
}

/// Writes bytes that are not a backup (an export, a copy of a document, the port file of the API) readable by the owner only. It goes
/// through a temporary file, so a crash or a full disk never leaves a half-written file under the real name.
pub fn write_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);

    let written = private_options().open(&tmp).and_then(|mut f| f.write_all(bytes).and_then(|_| f.sync_all()));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
        return Err(AppError::Internal);
    }
    fs::rename(&tmp, path).map_err(|_| {
        let _ = fs::remove_file(&tmp);
        AppError::Internal
    })
}

/// A version 1 backup, as the app used to write it: the rows as one sealed message. Kept to test that old files still open.
#[cfg(test)]
pub(crate) fn seal_v1(snapshot: &Snapshot, passphrase: &str) -> Result<Vec<u8>, AppError> {
    check_passphrase(passphrase)?;
    let plain = Zeroizing::new(serde_json::to_vec(&snapshot.tables).map_err(|_| AppError::Internal)?);
    let kdf = Kdf::DEFAULT;
    let salt = keys::random_bytes::<SALT_LEN>()?;
    let nonce = keys::random_bytes::<NONCE_LEN>()?;
    let mut out = header(FORMAT_1, snapshot.schema_version, kdf, &salt, &nonce);
    let key = stretch(passphrase, &salt, kdf)?;
    let sealed = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()))
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: &plain, aad: &out })
        .map_err(|_| AppError::Internal)?;
    out.extend_from_slice(&sealed);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::{self, CategoryType};
    use crate::{budgets, recurring, transactions};

    pub(super) fn db_at(version: u32) -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate::run_up_to(&mut conn, version).unwrap();
        conn
    }

    pub(super) fn db() -> Connection {
        db_at(migrate::latest_version())
    }

    /// A database with a bit of everything: every table has rows, text has awkward characters,
    /// there are edits and a deleted category so the history is not trivial.
    pub(super) fn lived_in() -> Connection {
        let conn = db();
        conn.execute(
            "INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, 'EUR', 'en-IE', 'Europe/Dublin', 'ro')",
            [],
        )
        .unwrap();
        let salary = categories::create(&conn, "Salary", CategoryType::Income).unwrap();
        let rent = categories::create(&conn, "Rent", CategoryType::Expense).unwrap();
        let gym = categories::create(&conn, "Gym", CategoryType::Expense).unwrap();
        for (kind, scope, category_id, limit, month) in [
            (CategoryType::Expense, budgets::Scope::Global, None, Some("2200.00"), "2026-01"),
            (CategoryType::Expense, budgets::Scope::Category, Some(rent.id.to_string()), Some("900.00"), "2026-01"),
            (CategoryType::Income, budgets::Scope::Global, None, Some("3500.00"), "2026-02"),
        ] {
            let input = budgets::SetBudget {
                kind,
                scope,
                month: month.into(),
                amount_limit: limit.map(str::to_string),
                category_id,
            };
            budgets::set(&conn, &input, chrono::Utc::now()).unwrap();
        }
        let plan = recurring::create(
            &conn,
            &recurring::NewRecurring {
                name: "Rent".into(),
                amount: Some("900.00".into()),
                category_id: rent.id.to_string(),
                start_date: "2026-01-01".into(),
                interval_unit: recurring::Unit::Month,
                interval_count: 1,
                end_date: None,
                description: Some("Flat 4, \"Maple\" house".into()),
            },
        )
        .unwrap();
        recurring::pay(&conn, plan.id, Some("905.50"), Some("2026-01-02"), "2026-01-02").unwrap();
        for (name, amount, date, category, description) in [
            ("Salary", "3200.00", "2026-01-25", &salary, Some("Café “bonus” \\ 日本語 'x'; DROP TABLE transactions;--")),
            ("Gym", "30.00", "2026-01-10", &gym, None),
            ("Coffee", "3.50", "2026-02-03", &rent, Some("=SUM(A1:A9)+cmd|' /C calc'!A0")),
        ] {
            transactions::create(
                &conn,
                &transactions::NewTransaction {
                    name: name.into(),
                    amount: amount.into(),
                    payment_date: date.into(),
                    category_id: category.id.to_string(),
                    description: description.map(str::to_string),
                    client_uuid: None,
                },
            )
            .unwrap();
        }
        categories::rename(&conn, rent.id, "Housing").unwrap();
        // a document linked to the recurring item (the file itself is not in the database, and so not in this backup)
        conn.execute(
            "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date, keep_on_phone)
             VALUES ('Contract “Maple” 日本語.pdf', 'application/pdf', 1234, ?1, '2026-01-02', 1)",
            ["ab".repeat(32)],
        )
        .unwrap();
        conn.execute("INSERT INTO document_links (document_id, recurring_transaction_id) VALUES (1, ?1)", [plan.id]).unwrap();
        // friends, a shared bill with a friend and a guest, a split template that makes a split payment, the ledger, and a document on an entry
        use crate::debts::{ledger, people, splits};
        let anna = people::create(&conn, "Anna “A” 日本語").unwrap();
        let bob = people::create(&conn, "Bob").unwrap();
        let dinner = transactions::create(
            &conn,
            &transactions::NewTransaction {
                name: "Dinner".into(),
                amount: "90.00".into(),
                payment_date: "2026-01-12".into(),
                category_id: rent.id.to_string(),
                description: None,
                client_uuid: None,
            },
        )
        .unwrap();
        let part = |person: Option<&crate::debts::people::Person>, guest: Option<&str>| splits::Part {
            person_id: person.map(|p| p.id.to_string()),
            guest: guest.map(str::to_string),
            value: None,
        };
        splits::set_split(&conn, dinner.id, &splits::SplitInput { mode: splits::Mode::Equal, mine: None, parts: vec![part(Some(&anna), None), part(None, Some("Carl"))] })
            .unwrap();
        splits::set_template(
            &conn,
            plan.id,
            &splits::SplitInput {
                mode: splits::Mode::Shares,
                mine: Some("2".into()),
                parts: vec![splits::Part { person_id: Some(bob.id.to_string()), guest: None, value: Some("1".into()) }],
            },
        )
        .unwrap();
        recurring::pay(&conn, plan.id, Some("900.00"), Some("2026-02-01"), "2026-02-01").unwrap();
        let loan = ledger::add(
            &conn,
            &ledger::NewEntry {
                person_id: anna.id.to_string(),
                kind: ledger::Kind::Lent,
                amount: "20.00".into(),
                date: "2026-01-13".into(),
                note: Some("taxi, \"Maple\" street".into()),
                transaction_id: Some(dinner.id.to_string()),
            },
        )
        .unwrap();
        people::set_reminder(&conn, bob.id, Some("2026-03-01"), Some(30)).unwrap();
        people::archive(&conn, bob.id).unwrap();
        conn.execute(
            "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date, keep_on_phone)
             VALUES ('Loan note.pdf', 'application/pdf', 99, ?1, '2026-01-13', 0)",
            ["cd".repeat(32)],
        )
        .unwrap();
        conn.execute("INSERT INTO document_links (document_id, ledger_entry_id) VALUES (2, ?1)", [loan.id]).unwrap();
        let mut conn = conn;
        categories::delete(&mut conn, gym.id, chrono::Utc::now()).unwrap();
        // a deletion (it leaves a marker for sync) and a change of currency (it is recorded for sync)
        conn.execute("DELETE FROM transactions WHERE name = 'Coffee'", []).unwrap();
        crate::currency::change(&mut conn, "GBP", None, "2026-02-04").unwrap();
        conn
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn the_lists_of_tables_cover_the_whole_schema() {
        let conn = db();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap();
        let mut in_schema: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        let mut listed: Vec<String> = TABLES.iter().chain(EXCLUDED_TABLES).map(|t| t.to_string()).collect();
        in_schema.sort();
        listed.sort();
        assert_eq!(listed, in_schema, "a table that is in neither list is missing from every backup");
    }

    #[test]
    fn a_backup_restores_every_row_of_every_table_exactly() {
        let source = lived_in();
        let before = dump(&source).unwrap();
        for table in TABLES {
            assert!(!before.rows(table).is_empty(), "the test data should fill {table}");
        }

        let file = seal(&before, "correct horse").unwrap();
        let mut target = db();
        let snapshot = open(&file, "correct horse").unwrap();
        restore_snapshot(&mut target, &snapshot).unwrap();

        assert_eq!(dump(&target).unwrap(), before, "ids, timestamps and the whole change history come back");
    }

    #[test]
    fn restoring_replaces_what_was_there_and_adds_no_history_of_its_own() {
        let source = lived_in();
        let before = dump(&source).unwrap();
        let file = seal(&before, "correct horse").unwrap();

        let mut target = db();
        target.execute("INSERT INTO settings (id, currency) VALUES (1, 'USD')", []).unwrap();
        categories::create(&target, "Old category", CategoryType::Expense).unwrap();
        restore(&mut target, &file, "correct horse");
        assert_eq!(dump(&target).unwrap(), before);
        let old: i64 =
            target.query_row("SELECT count(*) FROM categories WHERE name = 'Old category'", [], |r| r.get(0)).unwrap();
        assert_eq!(old, 0);
        assert_eq!(count(&target, "audit_log"), before.rows("audit_log").len() as i64);
    }

    fn restore(target: &mut Connection, file: &[u8], passphrase: &str) {
        restore_snapshot(target, &open(file, passphrase).unwrap()).unwrap();
    }

    #[test]
    fn text_with_quotes_unicode_and_sql_survives() {
        let source = lived_in();
        let file = seal(&dump(&source).unwrap(), "correct horse").unwrap();
        let mut target = db();
        restore(&mut target, &file, "correct horse");
        let description: String = target
            .query_row("SELECT description FROM transactions WHERE name = 'Salary'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(description, "Café “bonus” \\ 日本語 'x'; DROP TABLE transactions;--");
        assert_eq!(count(&target, "transactions"), 5, "six were made and one deleted");
    }

    #[test]
    fn the_wrong_passphrase_opens_nothing() {
        let file = seal(&dump(&lived_in()).unwrap(), "correct horse").unwrap();
        for wrong in ["wrong horse", "Correct horse", "", "correct horse "] {
            assert_eq!(open(&file, wrong), Err(AppError::WrongCredentials), "{wrong:?}");
        }
    }

    #[test]
    fn a_backup_needs_a_passphrase_of_the_usual_length() {
        let snapshot = dump(&db()).unwrap();
        assert_eq!(seal(&snapshot, "short"), Err(AppError::InvalidInput));
        assert_eq!(seal(&snapshot, ""), Err(AppError::InvalidInput));
        assert!(seal(&snapshot, "eight!!!").is_ok());
    }

    #[test]
    fn the_file_holds_no_plaintext_and_two_backups_of_the_same_data_differ() {
        let snapshot = dump(&lived_in()).unwrap();
        let a = seal(&snapshot, "correct horse").unwrap();
        let b = seal(&snapshot, "correct horse").unwrap();
        assert_ne!(a, b, "fresh salt and nonce every time");
        assert!(a.starts_with(MAGIC));
        // Long enough that a chance match in random bytes is out of the question.
        for needle in ["Salary", "Housing", "Café", "Europe/Dublin", "amount_minor", "\"audit_log\""] {
            assert!(!a.windows(needle.len()).any(|w| w == needle.as_bytes()), "{needle:?} is visible in the file");
        }
    }

    #[test]
    fn a_file_changed_after_it_was_written_is_refused() {
        let file = seal(&dump(&lived_in()).unwrap(), "correct horse").unwrap();
        // One flipped bit in the salt, the nonce, the body and the tag.
        for at in [30, 45, HEADER_LEN + 10, file.len() - 1] {
            let mut changed = file.clone();
            changed[at] ^= 1;
            assert_eq!(open(&changed, "correct horse"), Err(AppError::WrongCredentials), "byte {at}");
        }
    }

    #[test]
    fn files_that_are_not_backups_are_refused() {
        let file = seal(&dump(&db()).unwrap(), "correct horse").unwrap();
        let mut wrong_magic = file.clone();
        wrong_magic[0] = b'X';
        for bad in [
            Vec::new(),
            b"MEOWNYBK".to_vec(),
            b"hello, this is not a backup file at all, not even close to one".to_vec(),
            file[..HEADER_LEN + TAG_LEN - 1].to_vec(),
            wrong_magic,
        ] {
            assert_eq!(open(&bad, "correct horse"), Err(AppError::InvalidBackup));
        }
    }

    #[test]
    fn a_backup_from_a_newer_app_is_refused_before_any_work_is_done() {
        let file = seal(&dump(&db()).unwrap(), "correct horse").unwrap();

        let mut newer_schema = file.clone();
        newer_schema[10..14].copy_from_slice(&(migrate::latest_version() + 1).to_be_bytes());
        assert_eq!(open(&newer_schema, "correct horse"), Err(AppError::DatabaseTooNew));
        assert_eq!(open(&newer_schema, "wrong passphrase"), Err(AppError::DatabaseTooNew), "checked before the key");

        let mut newer_format = file.clone();
        newer_format[8..10].copy_from_slice(&(FORMAT + 1).to_be_bytes());
        assert_eq!(open(&newer_format, "correct horse"), Err(AppError::DatabaseTooNew));

        let mut snapshot = dump(&db()).unwrap();
        snapshot.schema_version = migrate::latest_version() + 1;
        let mut target = db();
        assert_eq!(restore_snapshot(&mut target, &snapshot), Err(AppError::DatabaseTooNew));
    }

    #[test]
    fn a_file_cannot_ask_for_an_absurd_key_stretching_cost() {
        let file = seal(&dump(&db()).unwrap(), "correct horse").unwrap();
        for (at, value) in [(14, u32::MAX), (14, 4), (18, 0), (18, 11), (22, 0), (22, 99)] {
            let mut changed = file.clone();
            changed[at..at + 4].copy_from_slice(&u32::to_be_bytes(value));
            assert_eq!(open(&changed, "correct horse"), Err(AppError::InvalidBackup), "{at} = {value}");
        }
    }

    /// Builds a backup (version 2) whose JSON the app would never write, to check what a hostile or damaged file can do.
    pub(super) fn forged(tables: Value) -> Vec<u8> {
        let kdf = Kdf::DEFAULT;
        let salt = [3u8; SALT_LEN];
        let nonce = [4u8; NONCE_LEN];
        let head = header(FORMAT, migrate::latest_version(), kdf, &salt, &nonce);
        let key = stretch("correct horse", &salt, kdf).unwrap();
        let rows = tables.to_string().into_bytes();
        let mut stream = ChunkWriter::new(head.clone(), &key, head);
        stream.write(&[RECORD_SNAPSHOT]).unwrap();
        stream.write(&(rows.len() as u64).to_be_bytes()).unwrap();
        stream.write(&rows).unwrap();
        stream.write(&[RECORD_END]).unwrap();
        stream.finish().unwrap()
    }

    /// The same in the layout of version 1.
    fn forged_v1(tables: Value) -> Vec<u8> {
        let kdf = Kdf::DEFAULT;
        let salt = [3u8; SALT_LEN];
        let nonce = [4u8; NONCE_LEN];
        let mut out = header(FORMAT_1, migrate::latest_version(), kdf, &salt, &nonce);
        let key = stretch("correct horse", &salt, kdf).unwrap();
        let sealed = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()))
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: tables.to_string().as_bytes(), aad: &out })
            .unwrap();
        out.extend_from_slice(&sealed);
        out
    }

    #[test]
    fn a_damaged_backup_changes_nothing() {
        let mut target = lived_in();
        let before = dump(&target).unwrap();
        let cases = [
            // Not JSON of the right shape.
            serde_json::json!([1, 2, 3]),
            serde_json::json!({ "categories": "nope" }),
            // A table that does not exist, including ones SQLite keeps for itself.
            serde_json::json!({ "sqlite_master": [{ "name": "x" }] }),
            serde_json::json!({ "users": [{ "id": 1 }] }),
            // A column that does not exist.
            serde_json::json!({ "categories": [{ "id": 1, "type": "EXPENSE", "name": "A", "evil": 1 }] }),
            serde_json::json!({ "categories": [{ "id": 1, "type": "EXPENSE", "name": "A", "name\") VALUES (1); --": 1 }] }),
            // Values the schema never holds.
            serde_json::json!({ "categories": [{ "id": 1, "type": "EXPENSE", "name": 1.5 }] }),
            serde_json::json!({ "categories": [{ "id": 1, "type": "EXPENSE", "name": ["A"] }] }),
            serde_json::json!({ "categories": [{ "id": 1, "type": "EXPENSE", "name": true }] }),
            // Rows the constraints refuse: a bad type, a missing required column, a duplicate id,
            // a transaction in a category that is not there.
            serde_json::json!({ "categories": [{ "id": 1, "type": "SIDEWAYS", "name": "A" }] }),
            serde_json::json!({ "categories": [{ "id": 1, "type": "EXPENSE" }] }),
            serde_json::json!({ "categories": [
                { "id": 1, "type": "EXPENSE", "name": "A" }, { "id": 1, "type": "EXPENSE", "name": "B" }] }),
            serde_json::json!({ "transactions": [
                { "id": 1, "category_id": 99, "name": "x", "amount_minor": 100, "payment_date": "2026-01-01" }] }),
            serde_json::json!({ "categories": [{ "id": 1, "type": "EXPENSE", "name": "A" }], "transactions": [
                { "id": 1, "category_id": 1, "name": "x", "amount_minor": -5, "payment_date": "2026-01-01" }] }),
        ];
        for case in cases {
            for file in [forged(case.clone()), forged_v1(case.clone())] {
                let outcome = open(&file, "correct horse").and_then(|s| restore_snapshot(&mut target, &s));
                assert_eq!(outcome, Err(AppError::InvalidBackup), "{case}");
                assert_eq!(dump(&target).unwrap(), before, "{case}");
            }
        }
    }

    #[test]
    fn an_empty_backup_restores_to_an_empty_database() {
        let file = forged(serde_json::json!({}));
        let mut target = lived_in();
        restore(&mut target, &file, "correct horse");
        for table in TABLES {
            assert_eq!(count(&target, table), 0, "{table}");
        }
    }

    #[test]
    fn a_backup_from_an_older_app_version_is_migrated_forward() {
        // Version 2 had no language column; version 3 added it with a default.
        let old = db_at(2);
        old.execute("INSERT INTO settings (id, currency, locale, timezone) VALUES (1, 'EUR', 'de-DE', 'Europe/Berlin')", [])
            .unwrap();
        old.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')", []).unwrap();
        old.execute(
            "INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (1, 'Bread', 250, '2026-03-01')",
            [],
        )
        .unwrap();
        let snapshot = dump(&old).unwrap();
        assert_eq!(snapshot.schema_version, 2);
        assert!(snapshot.rows("settings")[0].get("language").is_none());

        let file = seal(&snapshot, "correct horse").unwrap();
        let mut target = db();
        restore(&mut target, &file, "correct horse");

        let language: String = target.query_row("SELECT language FROM settings", [], |r| r.get(0)).unwrap();
        assert_eq!(language, "en", "the column the old version lacked takes its default");
        let (name, amount): (String, i64) =
            target.query_row("SELECT name, amount_minor FROM transactions", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((name.as_str(), amount), ("Bread", 250));
        assert_eq!(dump(&target).unwrap().schema_version, migrate::latest_version());
        // The newer indexes exist: the restored database is a current one, not a copy of the old shape.
        let indexed: i64 = target
            .query_row("SELECT count(*) FROM sqlite_master WHERE name = 'idx_tx_date_id'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(indexed, 1);
    }

    #[test]
    fn a_backup_from_before_the_amount_became_optional_keeps_its_recurring_items_and_their_payments() {
        // Version 5 still required an amount on a recurring item.
        let old = db_at(5);
        old.execute("INSERT INTO settings (id, currency, locale, timezone) VALUES (1, 'EUR', 'en-IE', 'UTC')", []).unwrap();
        old.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Rent')", []).unwrap();
        old.execute(
            "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date)
             VALUES (1, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-02-01')",
            [],
        )
        .unwrap();
        old.execute(
            "INSERT INTO transactions (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date)
             VALUES (1, 1, '2026-01-01', 'Rent', 90000, 90000, '2026-01-01')",
            [],
        )
        .unwrap();
        let snapshot = dump(&old).unwrap();
        assert_eq!(snapshot.schema_version, 5);

        let file = seal(&snapshot, "correct horse").unwrap();
        let mut target = db();
        restore(&mut target, &file, "correct horse");
        let (amount, link): (Option<i64>, Option<i64>) = target
            .query_row(
                "SELECT (SELECT amount_minor FROM recurring_transactions), (SELECT recurring_transaction_id FROM transactions)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((amount, link), (Some(90000), Some(1)), "the item and the link to its payment came through");
        assert!(target.execute("INSERT INTO recurring_transactions (category_id, name, start_date, interval_unit, next_due_date) VALUES (1, 'Gas', '2026-01-01', 'MONTH', '2026-01-01')", []).is_ok(), "the restored database has the newer rule");
    }

    #[test]
    fn tokens_stay_on_this_device_a_backup_has_none_and_a_restore_keeps_them() {
        use crate::tokens::{self, NewToken, Scope};
        let now = chrono::Utc::now();
        let input = NewToken { name: "agent".into(), scope: Scope::Write, expires_in_days: 30 };

        let source = lived_in();
        tokens::create(&source, &input, now).unwrap();
        let snapshot = dump(&source).unwrap();
        assert!(!snapshot.tables.contains_key("api_tokens"));
        let file = seal(&snapshot, "correct horse").unwrap();

        let mut target = db();
        let mine = tokens::create(&target, &input, now).unwrap();
        restore(&mut target, &file, "correct horse");
        assert_eq!(count(&target, "categories"), count(&source, "categories"), "the data did come over");
        assert!(tokens::verify(&target, &mine.token, now).unwrap().is_some(), "the token on this device still works");
        assert_eq!(count(&target, "api_tokens"), 1, "and the source's token did not arrive");
    }

    #[test]
    fn a_backup_made_now_carries_the_current_schema_version() {
        let snapshot = dump(&db()).unwrap();
        assert_eq!(snapshot.schema_version, migrate::latest_version());
        let file = seal(&snapshot, "correct horse").unwrap();
        assert_eq!(be_u32(&file, 10), migrate::latest_version());
    }

    #[test]
    fn files_are_written_whole_and_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.meowbak");
        write_file(&path, b"first").unwrap();
        write_file(&path, b"second, longer").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second, longer");
        assert!(!dir.path().join("a.meowbak.tmp").exists(), "no temporary file is left behind");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert_eq!(Backup::open(dir.path().join("missing.meowbak").as_path(), "correct horse").err(), Some(AppError::NotFound));
    }

    #[test]
    fn writing_into_a_missing_folder_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-such-folder").join("a.meowbak");
        assert_eq!(write_file(&path, b"x"), Err(AppError::Internal));
    }
}
