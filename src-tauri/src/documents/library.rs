//! The library: documents and their links, with the files that go with them (`docs/documents-design.md`, sections 5, 7 and 8).
//!
//! Every function takes the open database. The ones that touch a file also take `Files`, the folder and the keys of an unlocked
//! app, so a locked app has nothing to call them with. Adding a document writes the row and the file in one step: the row is
//! made inside a transaction, the sealed file is written before it commits, and if either fails the other is undone.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::categories::{parse_id, serialize_id};
use crate::keys;
use crate::transactions::clean_date;
use crate::vault::AppError;

use super::names::{clean_file_name, clean_title, sniff, MediaType};
use super::seal::{DocId, DocKeys, MAX_DOCUMENT_BYTES};
use super::store::{sha256, Check, Store};

pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 100;
const SEARCH_MAX_CHARS: usize = 120;
/// How many things one new document may be linked to when it is added.
const MAX_LINKS_AT_ADD: usize = 50;

/// The folder and the keys, which an unlocked app has and a locked one does not.
#[derive(Clone, Copy)]
pub struct Files<'a> {
    pub store: &'a Store,
    pub keys: &'a DocKeys,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    pub name: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    /// The day of the paper, `YYYY-MM-DD`.
    pub document_date: String,
    pub keep_on_phone: bool,
    pub created_at: String,
    /// How many transactions and recurring items it is linked to.
    pub link_count: u32,
    /// Whether this device holds the file. On the phone a document that is not kept there has none, and that is normal.
    pub has_file: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Transaction,
    Recurring,
}

/// What a document is linked to: a transaction or a recurring item, by id as the app writes ids.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkTarget {
    pub kind: TargetKind,
    pub id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewDocument {
    /// The name of the file, or any name; a path is cut to its last part.
    pub name: String,
    /// The day of the paper. Today when left out.
    #[serde(default)]
    pub document_date: Option<String>,
    #[serde(default)]
    pub keep_on_phone: bool,
    #[serde(default)]
    pub links: Vec<LinkTarget>,
}

/// Fields left out stay as they are.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub document_date: Option<String>,
    #[serde(default)]
    pub keep_on_phone: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Image,
    Pdf,
    Other,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Filter {
    /// Matches the name, ignoring ASCII case.
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub kind: Option<Kind>,
    /// Inclusive, `YYYY-MM-DD`, on the date of the paper.
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// Only the documents linked to this transaction.
    #[serde(default)]
    pub transaction_id: Option<String>,
    /// Only the documents linked to this recurring item.
    #[serde(default)]
    pub recurring_transaction_id: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub documents: Vec<Document>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddOutcome {
    pub document: Document,
    /// The same bytes were in the library already: nothing new was stored, the links were added to the existing document.
    pub already_there: bool,
}

/// What a document is linked to, for the screens: the kind, the id, a name to show and the date of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkView {
    pub kind: TargetKind,
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    pub name: String,
    /// The payment date of a transaction, the next due date of a recurring item.
    pub date: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Size {
    pub count: u32,
    pub bytes: u64,
}

#[derive(Clone, Copy)]
enum Target {
    Transaction(i64),
    Recurring(i64),
}

fn parse_target(target: &LinkTarget) -> Result<Target, AppError> {
    let id = parse_id(&target.id)?;
    Ok(match target.kind {
        TargetKind::Transaction => Target::Transaction(id),
        TargetKind::Recurring => Target::Recurring(id),
    })
}

fn target_exists(conn: &Connection, target: Target) -> Result<bool, AppError> {
    let (sql, id) = match target {
        Target::Transaction(id) => ("SELECT 1 FROM transactions WHERE id = ?1", id),
        Target::Recurring(id) => ("SELECT 1 FROM recurring_transactions WHERE id = ?1", id),
    };
    Ok(conn.query_row(sql, [id], |_| Ok(())).optional()?.is_some())
}

fn document_exists(conn: &Connection, id: i64) -> Result<bool, AppError> {
    Ok(conn.query_row("SELECT 1 FROM documents WHERE id = ?1", [id], |_| Ok(())).optional()?.is_some())
}

/// Makes the link, or leaves the one that is there. The target and the document must exist.
fn insert_link(conn: &Connection, document_id: i64, target: Target) -> Result<(), AppError> {
    if !target_exists(conn, target)? {
        return Err(AppError::NotFound);
    }
    let (transaction, recurring) = match target {
        Target::Transaction(id) => (Some(id), None),
        Target::Recurring(id) => (None, Some(id)),
    };
    conn.execute(
        "INSERT OR IGNORE INTO document_links (document_id, transaction_id, recurring_transaction_id) VALUES (?1, ?2, ?3)",
        params![document_id, transaction, recurring],
    )?;
    Ok(())
}

const COLUMNS: &str = "d.id, d.sync_id, d.name, d.media_type, d.size_bytes, d.sha256, d.document_date, d.keep_on_phone, d.created_at,
    (SELECT count(*) FROM document_links l WHERE l.document_id = d.id)";

/// A row as the database holds it, and the `sync_id` that names its file. `has_file` is filled in afterwards.
fn from_row(row: &rusqlite::Row) -> rusqlite::Result<(Document, String)> {
    Ok((
        Document {
            id: row.get(0)?,
            name: row.get(2)?,
            media_type: row.get(3)?,
            size_bytes: row.get::<_, i64>(4)? as u64,
            sha256: row.get(5)?,
            document_date: row.get(6)?,
            keep_on_phone: row.get::<_, i64>(7)? == 1,
            created_at: row.get(8)?,
            link_count: row.get(9)?,
            has_file: false,
        },
        row.get(1)?,
    ))
}

/// Looks at the folder once for all of them.
fn with_presence(rows: Vec<(Document, String)>, files: &Files) -> Result<Vec<Document>, AppError> {
    let present: HashSet<String> = files.store.present_stems()?;
    rows.into_iter()
        .map(|(mut document, sync_id)| {
            document.has_file = present.contains(&files.keys.stem(&DocId::parse(&sync_id)?));
            Ok(document)
        })
        .collect()
}

pub fn get(conn: &Connection, files: &Files, id: i64) -> Result<Document, AppError> {
    let (mut document, sync_id) = conn
        .query_row(&format!("SELECT {COLUMNS} FROM documents d WHERE d.id = ?1"), [id], from_row)
        .optional()?
        .ok_or(AppError::NotFound)?;
    document.has_file = files.store.has(files.keys, &DocId::parse(&sync_id)?);
    Ok(document)
}

/// The id of a document and the `sync_id` that names its file, for the one that holds these bytes.
fn find_by_hash(conn: &Connection, hash: &str) -> Result<Option<(i64, String)>, AppError> {
    Ok(conn
        .query_row("SELECT id, sync_id FROM documents WHERE sha256 = ?1 ORDER BY id LIMIT 1", [hash], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?)
}

/// Stores a file and makes its document. `bytes` is the whole file, `today` the date to use when none is given.
///
/// If the same bytes are in the library already nothing is stored again: the links are added to the document that is there, and
/// if this device had lost its file, adding the file again brings it back.
pub fn add(conn: &Connection, files: &Files, input: &NewDocument, bytes: &[u8], today: &str) -> Result<AddOutcome, AppError> {
    if bytes.is_empty() {
        return Err(AppError::InvalidInput);
    }
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(AppError::TooLarge);
    }
    if input.links.len() > MAX_LINKS_AT_ADD {
        return Err(AppError::InvalidInput);
    }
    let name = clean_file_name(&input.name);
    let date = clean_date(input.document_date.as_deref().unwrap_or(today))?;
    let targets = input.links.iter().map(parse_target).collect::<Result<Vec<_>, _>>()?;
    let hash = keys::hex_encode(&sha256(bytes));
    let media = sniff(&name, bytes);

    let tx = conn.unchecked_transaction()?;
    if let Some((id, sync_id)) = find_by_hash(&tx, &hash)? {
        let doc_id = DocId::parse(&sync_id)?;
        if !files.store.has(files.keys, &doc_id) {
            files.store.put(files.keys, &doc_id, bytes)?;
        }
        for target in &targets {
            insert_link(&tx, id, *target)?;
        }
        tx.commit()?;
        return Ok(AddOutcome { document: get(conn, files, id)?, already_there: true });
    }

    tx.execute(
        "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date, keep_on_phone) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![name, media.as_str(), bytes.len() as i64, hash, date, input.keep_on_phone as i64],
    )?;
    let id = tx.last_insert_rowid();
    let sync_id: String = tx.query_row("SELECT sync_id FROM documents WHERE id = ?1", [id], |r| r.get(0))?;
    let doc_id = DocId::parse(&sync_id)?;
    for target in &targets {
        insert_link(&tx, id, *target)?;
    }
    // The file is written before the row is committed. If the write fails nothing is committed; if the commit fails the file is
    // removed; and if the process dies between the two, a file with no row is what the check of the folder cleans up.
    files.store.put(files.keys, &doc_id, bytes)?;
    if let Err(e) = tx.commit() {
        let _ = files.store.remove(files.keys, &doc_id);
        return Err(e.into());
    }
    Ok(AddOutcome { document: get(conn, files, id)?, already_there: false })
}

/// Renames, re-dates or switches "keep on phone". The contents cannot be changed.
pub fn update(conn: &Connection, files: &Files, id: i64, patch: &DocumentPatch) -> Result<Document, AppError> {
    let current = get(conn, files, id)?;
    let name = match &patch.name {
        Some(name) => clean_title(name).ok_or(AppError::InvalidInput)?,
        None => current.name.clone(),
    };
    let date = match &patch.document_date {
        Some(date) => clean_date(date)?,
        None => current.document_date.clone(),
    };
    let keep = patch.keep_on_phone.unwrap_or(current.keep_on_phone);
    if name != current.name || date != current.document_date || keep != current.keep_on_phone {
        conn.execute(
            "UPDATE documents SET name = ?1, document_date = ?2, keep_on_phone = ?3 WHERE id = ?4",
            params![name, date, keep as i64, id],
        )?;
    }
    get(conn, files, id)
}

/// Deletes the document, its links and its file. Transactions and recurring items are not touched.
pub fn delete(conn: &Connection, files: &Files, id: i64) -> Result<(), AppError> {
    let sync_id: String =
        conn.query_row("SELECT sync_id FROM documents WHERE id = ?1", [id], |r| r.get(0)).optional()?.ok_or(AppError::NotFound)?;
    let doc_id = DocId::parse(&sync_id)?;
    // The row goes first, in one statement (the links go with it by the foreign key). A file that is left behind by a failure
    // here has no row, which the check of the folder removes; a row without its file would be the worse state.
    conn.execute("DELETE FROM documents WHERE id = ?1", [id])?;
    let _ = files.store.remove(files.keys, &doc_id);
    Ok(())
}

/// Links a document to a transaction or a recurring item. A link that is there already is left as it is.
pub fn link(conn: &Connection, document_id: i64, target: &LinkTarget) -> Result<(), AppError> {
    let target = parse_target(target)?;
    if !document_exists(conn, document_id)? {
        return Err(AppError::NotFound);
    }
    insert_link(conn, document_id, target)
}

/// Removes a link. The document and the transaction stay.
pub fn unlink(conn: &Connection, document_id: i64, target: &LinkTarget) -> Result<(), AppError> {
    let removed = match parse_target(target)? {
        Target::Transaction(id) => {
            conn.execute("DELETE FROM document_links WHERE document_id = ?1 AND transaction_id = ?2", params![document_id, id])?
        }
        Target::Recurring(id) => {
            conn.execute("DELETE FROM document_links WHERE document_id = ?1 AND recurring_transaction_id = ?2", params![document_id, id])?
        }
    };
    if removed == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// What a document is linked to, newest first.
pub fn links_of(conn: &Connection, document_id: i64) -> Result<Vec<LinkView>, AppError> {
    if !document_exists(conn, document_id)? {
        return Err(AppError::NotFound);
    }
    let mut stmt = conn.prepare(
        "SELECT 'transaction', t.id, t.name, t.payment_date
           FROM document_links l JOIN transactions t ON t.id = l.transaction_id WHERE l.document_id = ?1
         UNION ALL
         SELECT 'recurring', r.id, r.name, r.next_due_date
           FROM document_links l JOIN recurring_transactions r ON r.id = l.recurring_transaction_id WHERE l.document_id = ?1
         ORDER BY 4 DESC, 2 DESC",
    )?;
    let rows = stmt.query_map([document_id], |r| {
        let kind: String = r.get(0)?;
        Ok(LinkView {
            kind: if kind == "transaction" { TargetKind::Transaction } else { TargetKind::Recurring },
            id: r.get(1)?,
            name: r.get(2)?,
            date: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn like_pattern(q: &str) -> String {
    let mut pattern = String::from("%");
    for c in q.chars() {
        if matches!(c, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

/// Newest paper first, then newest id.
pub fn list(conn: &Connection, files: &Files, filter: &Filter) -> Result<Page, AppError> {
    let from = filter.from.as_deref().map(clean_date).transpose()?;
    let to = filter.to.as_deref().map(clean_date).transpose()?;
    let transaction = filter.transaction_id.as_deref().map(parse_id).transpose()?;
    let recurring = filter.recurring_transaction_id.as_deref().map(parse_id).transpose()?;
    let pattern = match filter.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) if q.chars().count() > SEARCH_MAX_CHARS => return Err(AppError::InvalidInput),
        Some(q) => Some(like_pattern(q)),
        None => None,
    };
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(AppError::InvalidInput);
    }
    let offset = filter.offset.unwrap_or(0);

    let mut clauses: Vec<&str> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(from) = from {
        clauses.push("d.document_date >= ?");
        values.push(Box::new(from));
    }
    if let Some(to) = to {
        clauses.push("d.document_date <= ?");
        values.push(Box::new(to));
    }
    if let Some(pattern) = pattern {
        clauses.push("d.name LIKE ? ESCAPE '\\'");
        values.push(Box::new(pattern));
    }
    match filter.kind {
        Some(Kind::Image) => clauses.push("d.media_type IN ('image/jpeg', 'image/png', 'image/gif', 'image/webp')"),
        Some(Kind::Pdf) => clauses.push("d.media_type = 'application/pdf'"),
        Some(Kind::Other) => clauses.push("d.media_type IN ('text/plain', 'text/html', 'text/csv', 'application/octet-stream')"),
        None => {}
    }
    if let Some(id) = transaction {
        clauses.push("d.id IN (SELECT document_id FROM document_links WHERE transaction_id = ?)");
        values.push(Box::new(id));
    }
    if let Some(id) = recurring {
        clauses.push("d.id IN (SELECT document_id FROM document_links WHERE recurring_transaction_id = ?)");
        values.push(Box::new(id));
    }
    let filter_sql = if clauses.is_empty() { String::new() } else { format!("WHERE {}", clauses.join(" AND ")) };

    let total: u32 = conn.query_row(
        &format!("SELECT count(*) FROM documents d {filter_sql}"),
        rusqlite::params_from_iter(values.iter().map(|v| v.as_ref())),
        |r| r.get(0),
    )?;
    let mut page_values: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    page_values.push(&limit);
    page_values.push(&offset);
    let mut statement = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM documents d {filter_sql} ORDER BY d.document_date DESC, d.id DESC LIMIT ? OFFSET ?"
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(page_values), from_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(Page { documents: with_presence(rows, files)?, total, limit, offset })
}

/// How many documents there are and how much they hold, for the page that lists them.
pub fn library_size(conn: &Connection) -> Result<Size, AppError> {
    Ok(conn.query_row("SELECT count(*), COALESCE(sum(size_bytes), 0) FROM documents", [], |r| {
        Ok(Size { count: r.get(0)?, bytes: r.get::<_, i64>(1)? as u64 })
    })?)
}

/// The contents of a document, opened and proved to be what its row says. For showing, opening and saving a copy.
pub fn read(conn: &Connection, files: &Files, id: i64) -> Result<Zeroizing<Vec<u8>>, AppError> {
    let (sync_id, size, hash): (String, i64, String) = conn
        .query_row("SELECT sync_id, size_bytes, sha256 FROM documents WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?
        .ok_or(AppError::NotFound)?;
    let hash = keys::hex_decode::<32>(&hash).ok_or(AppError::Internal)?;
    Ok(files.store.get(files.keys, &DocId::parse(&sync_id)?, size as u64, &hash)?)
}

/// Compares the folder with the rows: removes what no row names and what a crash left, and reports the rows with no file.
pub fn check(conn: &Connection, files: &Files) -> Result<Check, AppError> {
    let mut stmt = conn.prepare("SELECT sync_id, size_bytes FROM documents ORDER BY id")?;
    let docs = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(sync_id, size)| Ok((DocId::parse(&sync_id)?, size as u64)))
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(files.store.check(files.keys, &docs)?)
}

/// The kind of file a row says, for code that decides whether to draw, open or only save it.
pub fn media_type_of(document: &Document) -> MediaType {
    MediaType::parse(&document.media_type).unwrap_or(MediaType::Other)
}
