//! The bytes of the documents' files: what follows the rows in a sync (`docs/documents-design.md`, section 9).
//!
//! The rows say which documents there are and which ones the phone keeps; this moves the files. Everything here is bound by two
//! rules that the rows cannot carry. A file the phone is not meant to keep never goes there, even if it asks, and the phone lets
//! go of a file only when the desktop has said, in this very sync, that it holds a whole and checked copy.
//!
//! The steps, each with the desktop speaking first and then the phone, as for the rows:
//!
//! 1. `have`: each lists the documents whose file it holds.
//! 2. `need`: each lists the files it wants, with how much of each it has already. The desktop wants every file it lacks that the
//!    phone holds. The phone wants the files it lacks that the person keeps on it, and only those. Newest first, and no more than
//!    `BUDGET` bytes in all: what does not fit waits for the next sync.
//! 3. `file` and `chunk`: the desktop sends what the phone asked for, then the phone what the desktop asked for. A file comes as
//!    a `file` message and pieces of 24 KiB. The receiver seals it into its own folder under its own key as it arrives, checks the
//!    hash at the end, and only then gives it its name. A file that stops half way is carried on from its last whole sealed chunk.
//! 4. `stored`: the desktop lists the documents it holds whole and checked that the phone does not have to keep. The phone removes
//!    those files.
//!
//! This runs after the rows are in and checked, so a connection that breaks here costs files, never the sync: the next one
//! carries on where this one stopped.

use std::collections::HashSet;

use rusqlite::Connection;
use zeroize::Zeroizing;

use super::engine::SyncError;
use super::flow::{next, receives, sends, Db, FlowError, Role};
use super::net::{Channel, NetError};
use super::qr::{base64url_decode, base64url_encode};
use super::wire::{ChunkMessage, FileMessage, HaveMessage, Message, Mode, NeedMessage, StoredMessage, CHUNK_BYTES, MAX_ENTRIES_PER_MESSAGE, MAX_ROWS_PER_TABLE};
use crate::documents::library::Files;
use crate::documents::seal::{DocError, DocId};
use crate::keys;

/// The bytes of files one sync may move, both ways together. A session may carry 256 MiB in all; base64url and the framing add a
/// third, and the rows come first.
pub const BUDGET: u64 = 100 * 1024 * 1024;

/// What the files of a sync did, as this device saw it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FilesReport {
    /// Files that arrived here whole.
    pub received: u32,
    /// Files this device sent.
    pub sent: u32,
    /// Files that were asked for and did not come: the other device could not send them, or they arrived damaged.
    pub failed: u32,
    /// Files this device wants that did not fit in this sync.
    pub waiting: u32,
    /// Files this phone let go of, because the desktop holds them and the person does not keep them here.
    pub removed: u32,
    /// The connection broke before everything was moved.
    pub interrupted: bool,
}

impl FilesReport {
    pub fn is_empty(&self) -> bool {
        *self == FilesReport::default()
    }
}

fn database(_: rusqlite::Error) -> SyncError {
    SyncError::Database
}

fn storage(_: DocError) -> SyncError {
    SyncError::Internal
}

/// A document as the transfer needs it.
struct Doc {
    sync_id: String,
    id: DocId,
    size: u64,
    sha256: [u8; 32],
    keep: bool,
}

fn doc_of(sync_id: String, size: i64, sha256: String, keep: i64) -> Option<Doc> {
    Some(Doc { id: DocId::parse(&sync_id).ok()?, sync_id, size: u64::try_from(size).ok()?, sha256: keys::hex_decode::<32>(&sha256)?, keep: keep == 1 })
}

type Raw = (String, i64, String, i64);

fn raw(r: &rusqlite::Row) -> rusqlite::Result<Raw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}

/// Every document, the newest first.
fn all_docs(conn: &Connection) -> Result<Vec<Doc>, SyncError> {
    let mut statement = conn.prepare("SELECT sync_id, size_bytes, sha256, keep_on_phone FROM documents ORDER BY created_at DESC, id DESC").map_err(database)?;
    let rows = statement.query_map([], raw).map_err(database)?;
    let mut docs = Vec::new();
    for row in rows {
        let (sync_id, size, hash, keep) = row.map_err(database)?;
        docs.extend(doc_of(sync_id, size, hash, keep));
    }
    Ok(docs)
}

fn one_doc(conn: &Connection, sync_id: &str) -> Result<Option<Doc>, SyncError> {
    let found = conn.query_row("SELECT sync_id, size_bytes, sha256, keep_on_phone FROM documents WHERE sync_id = ?1", [sync_id], raw);
    match found {
        Ok((sync_id, size, hash, keep)) => Ok(doc_of(sync_id, size, hash, keep)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(database(e)),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// what each step works out from the database and the folder (no network here, so each can be tested alone)

/// The documents whose file this device holds.
pub(super) fn have_list(conn: &Connection, files: &Files) -> Result<Vec<String>, SyncError> {
    let present = files.store.present_stems().map_err(storage)?;
    Ok(all_docs(conn)?.into_iter().filter(|doc| present.contains(&files.keys.stem(&doc.id))).map(|doc| doc.sync_id).collect())
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Wanted {
    /// `(sync_id, offset)`: the file, and how much of it is here already.
    pub files: Vec<(String, u64)>,
    /// Files that are wanted and could come, but do not fit in `budget`.
    pub waiting: u32,
}

/// The files this device lacks that the other holds (`peer_has`), newest first, as far as `budget` bytes reach. A phone wants only
/// the documents the person keeps on it.
pub(super) fn wanted(conn: &Connection, files: &Files, role: Role, peer_has: &HashSet<String>, budget: u64) -> Result<Wanted, SyncError> {
    let present = files.store.present_stems().map_err(storage)?;
    let mut left = budget;
    let mut wanted = Wanted::default();
    for doc in all_docs(conn)? {
        if !peer_has.contains(&doc.sync_id) || present.contains(&files.keys.stem(&doc.id)) || (role == Role::Phone && !doc.keep) {
            continue;
        }
        let offset = files.store.resume_offset(files.keys, &doc.id, doc.size);
        let cost = doc.size - offset;
        if cost > left {
            wanted.waiting += 1;
            continue;
        }
        left -= cost;
        wanted.files.push((doc.sync_id, offset));
    }
    Ok(wanted)
}

/// How many bytes the other device asked for, as far as this device knows the documents.
pub(super) fn cost(conn: &Connection, asked: &[(String, u64)]) -> Result<u64, SyncError> {
    let mut total = 0u64;
    for (id, offset) in asked {
        if let Some(doc) = one_doc(conn, id)? {
            total = total.saturating_add(doc.size.saturating_sub(*offset));
        }
    }
    Ok(total)
}

pub(super) enum Reply {
    /// Not sent, and the other device is told so.
    Skip,
    /// The whole file, opened and checked against its row.
    Send { size: u64, bytes: Zeroizing<Vec<u8>> },
}

/// The answer to one request. `role` is this device's: a desktop never sends the file of a document the person does not keep on the
/// phone, however it is asked, and nothing is sent that does not open and match its row.
pub(super) fn answer(conn: &Connection, files: &Files, role: Role, id: &str, offset: u64) -> Result<Reply, SyncError> {
    let Some(doc) = one_doc(conn, id)? else { return Ok(Reply::Skip) };
    if (role == Role::Desktop && !doc.keep) || offset >= doc.size {
        return Ok(Reply::Skip);
    }
    Ok(match files.store.get(files.keys, &doc.id, doc.size, &doc.sha256) {
        Ok(bytes) => Reply::Send { size: doc.size, bytes },
        Err(_) => Reply::Skip,
    })
}

/// Whether the phone may let go of this file: the document is not kept on it, and this device holds the file. `verify` reads the whole
/// file and checks it against its row; a file that arrived a moment ago was checked as it arrived.
pub(super) fn holds_whole(conn: &Connection, files: &Files, id: &str, verify: bool) -> Result<bool, SyncError> {
    let Some(doc) = one_doc(conn, id)? else { return Ok(false) };
    if doc.keep {
        return Ok(false);
    }
    Ok(if verify { files.store.get(files.keys, &doc.id, doc.size, &doc.sha256).is_ok() } else { files.store.has(files.keys, &doc.id) })
}

/// Phone: removes the files of the documents the desktop says it holds, unless the person keeps them here. Returns how many.
pub(super) fn drop_stored(conn: &Connection, files: &Files, ids: &[String]) -> Result<u32, SyncError> {
    let mut removed = 0;
    for id in ids {
        let Some(doc) = one_doc(conn, id)? else { continue };
        if !doc.keep && files.store.has(files.keys, &doc.id) {
            files.store.remove(files.keys, &doc.id).map_err(storage)?;
            removed += 1;
        }
    }
    Ok(removed)
}

// ---------------------------------------------------------------------------------------------------------------
// the conversation

/// Everything after the rows. A connection that breaks on the way is not an error: it is reported, and the files that did not
/// move wait for the next sync.
pub fn run<D: Db>(channel: &mut Channel, db: &D, role: Role, mode: Mode) -> Result<FilesReport, FlowError> {
    run_within(channel, db, role, mode, BUDGET)
}

/// `run` with another budget, which is how the tests see what happens when it is reached.
pub(super) fn run_within<D: Db>(channel: &mut Channel, db: &D, role: Role, mode: Mode, budget: u64) -> Result<FilesReport, FlowError> {
    let mut report = FilesReport::default();
    match phase(channel, db, role, mode, budget, &mut report) {
        Ok(()) => Ok(report),
        Err(FlowError::Net(NetError::Timeout | NetError::Closed | NetError::Io) | FlowError::Peer { .. }) => {
            report.interrupted = true;
            Ok(report)
        }
        Err(error) => Err(error),
    }
}

fn phase<D: Db>(channel: &mut Channel, db: &D, role: Role, mode: Mode, budget: u64, report: &mut FilesReport) -> Result<(), FlowError> {
    // 1. which files each holds
    let mine = db.with_documents(have_list)?;
    let theirs: HashSet<String> = match role {
        Role::Desktop => {
            send_ids(channel, &mine, |ids, last| Message::Have(HaveMessage { ids, last }))?;
            receive_have(channel)?
        }
        Role::Phone => {
            let theirs = receive_have(channel)?;
            send_ids(channel, &mine, |ids, last| Message::Have(HaveMessage { ids, last }))?;
            theirs
        }
    }
    .into_iter()
    .collect();

    // 2. which files each wants. The desktop's request comes first and has the whole budget; the phone's has what is left.
    let (want, asked) = match role {
        Role::Desktop => {
            let want = wanted_by(db, role, mode, &theirs, budget)?;
            send_need(channel, &want.files)?;
            (want, receive_need(channel)?)
        }
        Role::Phone => {
            let asked = receive_need(channel)?;
            let spent = db.with_documents(|c, _| cost(c, &asked))?;
            let want = wanted_by(db, role, mode, &theirs, budget.saturating_sub(spent))?;
            send_need(channel, &want.files)?;
            (want, asked)
        }
    };
    report.waiting = want.waiting;
    if !asked.is_empty() && !sends(role, mode) {
        return Err(FlowError::Protocol("request for files against the direction of the sync"));
    }

    // 3. the files themselves
    let mut arrived = Vec::new();
    match role {
        Role::Desktop => {
            serve(channel, db, role, &asked, budget, report)?;
            fetch(channel, db, &want.files, report, &mut arrived)?;
        }
        Role::Phone => {
            fetch(channel, db, &want.files, report, &mut arrived)?;
            serve(channel, db, role, &asked, budget, report)?;
        }
    }

    // 4. which files the phone may let go of. A sync that only goes to the desktop leaves the phone as it is.
    if mode != Mode::ToDesktop {
        match role {
            Role::Desktop => {
                let confirmed = confirmed(db, &theirs, &arrived)?;
                send_ids(channel, &confirmed, |ids, last| Message::Stored(StoredMessage { ids, last }))?;
            }
            Role::Phone => report.removed = receive_stored(channel, db)?,
        }
    }
    Ok(())
}

fn wanted_by<D: Db>(db: &D, role: Role, mode: Mode, peer_has: &HashSet<String>, budget: u64) -> Result<Wanted, FlowError> {
    if !receives(role, mode) {
        return Ok(Wanted::default());
    }
    Ok(db.with_documents(|c, f| wanted(c, f, role, peer_has, budget))?)
}

/// The documents the phone may let go of: those it holds a file of (`phone_has`, or just sent) that this desktop holds whole too.
fn confirmed<D: Db>(db: &D, phone_has: &HashSet<String>, arrived: &[String]) -> Result<Vec<String>, FlowError> {
    let fresh: HashSet<&String> = arrived.iter().collect();
    let mut seen = HashSet::new();
    let mut confirmed = Vec::new();
    for id in arrived.iter().chain(phone_has.iter()) {
        if seen.insert(id) && db.with_documents(|c, f| holds_whole(c, f, id, !fresh.contains(id)))? {
            confirmed.push(id.clone());
        }
    }
    confirmed.sort();
    Ok(confirmed)
}

/// Sends a list of ids in as many messages as it takes, the last marked; an empty list is one empty message.
fn send_ids(channel: &mut Channel, ids: &[String], make: impl Fn(Vec<String>, bool) -> Message) -> Result<(), FlowError> {
    let pieces: Vec<&[String]> = ids.chunks(MAX_ENTRIES_PER_MESSAGE).collect();
    if pieces.is_empty() {
        channel.send(&make(Vec::new(), true))?;
    }
    for (index, piece) in pieces.iter().enumerate() {
        channel.send(&make(piece.to_vec(), index + 1 == pieces.len()))?;
    }
    Ok(())
}

fn receive_have(channel: &mut Channel) -> Result<Vec<String>, FlowError> {
    let mut all = Vec::new();
    loop {
        let Message::Have(have) = next(channel)? else { return Err(FlowError::Protocol("message instead of the list of files")) };
        all.extend(have.ids);
        if all.len() > MAX_ROWS_PER_TABLE {
            return Err(FlowError::Protocol("list of files too long"));
        }
        if have.last {
            return Ok(all);
        }
    }
}

fn send_need(channel: &mut Channel, files: &[(String, u64)]) -> Result<(), FlowError> {
    let pieces: Vec<&[(String, u64)]> = files.chunks(MAX_ENTRIES_PER_MESSAGE).collect();
    if pieces.is_empty() {
        channel.send(&Message::Need(NeedMessage { files: Vec::new(), last: true }))?;
    }
    for (index, piece) in pieces.iter().enumerate() {
        channel.send(&Message::Need(NeedMessage { files: piece.to_vec(), last: index + 1 == pieces.len() }))?;
    }
    Ok(())
}

fn receive_need(channel: &mut Channel) -> Result<Vec<(String, u64)>, FlowError> {
    let mut all: Vec<(String, u64)> = Vec::new();
    let mut seen = HashSet::new();
    loop {
        let Message::Need(need) = next(channel)? else { return Err(FlowError::Protocol("message instead of the request for files")) };
        for (id, offset) in need.files {
            // asking twice for the same file is asking for more than the other device can have wanted
            if !seen.insert(id.clone()) {
                return Err(FlowError::Protocol("file asked for twice"));
            }
            all.push((id, offset));
        }
        if all.len() > MAX_ROWS_PER_TABLE {
            return Err(FlowError::Protocol("request for files too long"));
        }
        if need.last {
            return Ok(all);
        }
    }
}

/// Sends the files the other device asked for, in the order it asked, each as a `file` message and its pieces. What is not here, is
/// not for the other device, or would take more than the budget is answered with a `file` that says it is skipped.
fn serve<D: Db>(channel: &mut Channel, db: &D, role: Role, asked: &[(String, u64)], budget: u64, report: &mut FilesReport) -> Result<(), FlowError> {
    let mut spent = 0u64;
    for (id, offset) in asked {
        let reply = db.with_documents(|c, f| answer(c, f, role, id, *offset))?;
        let Reply::Send { size, bytes } = reply else {
            channel.send(&Message::File(FileMessage { id: id.clone(), offset: 0, size: 0, skipped: true }))?;
            continue;
        };
        spent = spent.saturating_add(size - offset);
        if spent > budget {
            channel.send(&Message::File(FileMessage { id: id.clone(), offset: 0, size: 0, skipped: true }))?;
            continue;
        }
        channel.send(&Message::File(FileMessage { id: id.clone(), offset: *offset, size, skipped: false }))?;
        let mut pieces = bytes[*offset as usize..].chunks(CHUNK_BYTES).peekable();
        while let Some(piece) = pieces.next() {
            channel.send(&Message::Chunk(ChunkMessage { data: base64url_encode(piece), last: pieces.peek().is_none() }))?;
        }
        report.sent += 1;
    }
    Ok(())
}

/// Receives the files asked for, one `file` message each in the same order. A file that cannot be had (the other side skips it, a
/// piece cannot be written, the hash is wrong) is counted and the next one goes on; what the other device sends against the
/// rules ends the sync.
fn fetch<D: Db>(channel: &mut Channel, db: &D, wanted: &[(String, u64)], report: &mut FilesReport, arrived: &mut Vec<String>) -> Result<(), FlowError> {
    for (id, offset) in wanted {
        let Message::File(file) = next(channel)? else { return Err(FlowError::Protocol("message instead of a file")) };
        if file.id != *id {
            return Err(FlowError::Protocol("file that was not asked for"));
        }
        if file.skipped {
            report.failed += 1;
            continue;
        }
        let Some((doc_id, size, sha256)) = db.with_documents(|c, _| Ok(one_doc(c, id)?.map(|d| (d.id, d.size, d.sha256))))? else {
            return Err(FlowError::Protocol("file of an unknown document"));
        };
        // the row says what the file is; a file that says otherwise is not the document's
        if file.size != size || file.offset != *offset {
            return Err(FlowError::Protocol("file that is not the one asked for"));
        }
        let mut incoming = db.with_documents(|_, f| Ok(f.store.begin_incoming(f.keys, &doc_id, size).ok().filter(|i| i.offset() == *offset)))?;
        let mut got = 0u64;
        loop {
            let Message::Chunk(chunk) = next(channel)? else { return Err(FlowError::Protocol("message instead of a piece of a file")) };
            let bytes = base64url_decode(&chunk.data).map(Zeroizing::new).filter(|b| !b.is_empty() && b.len() <= CHUNK_BYTES);
            let Some(bytes) = bytes else { return Err(FlowError::Protocol("piece of a file")) };
            got += bytes.len() as u64;
            if got > size - offset {
                return Err(FlowError::Protocol("file longer than its row says"));
            }
            if let Some(writing) = incoming.as_mut() {
                let written = db.with_documents(|_, _| Ok(writing.write(&bytes).is_ok()))?;
                if !written {
                    incoming = None;
                }
            }
            if chunk.last {
                break;
            }
        }
        match incoming {
            Some(done) if got == size - offset => match db.with_documents(|_, _| Ok(done.finish(&sha256).is_ok()))? {
                true => {
                    report.received += 1;
                    arrived.push(id.clone());
                }
                false => report.failed += 1,
            },
            _ => report.failed += 1,
        }
    }
    Ok(())
}

/// Phone: the desktop's list of files it holds whole; the ones the person does not keep here are removed.
fn receive_stored<D: Db>(channel: &mut Channel, db: &D) -> Result<u32, FlowError> {
    let (mut removed, mut count) = (0u32, 0usize);
    loop {
        let Message::Stored(stored) = next(channel)? else { return Err(FlowError::Protocol("message instead of the list of stored files")) };
        count += stored.ids.len();
        if count > MAX_ROWS_PER_TABLE {
            return Err(FlowError::Protocol("list of files too long"));
        }
        removed += db.with_documents(|c, f| drop_stored(c, f, &stored.ids))?;
        if stored.last {
            return Ok(removed);
        }
    }
}
