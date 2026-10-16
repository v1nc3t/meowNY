//! The conversations of `docs/sync-design.md`, section 8, over a real channel: a pairing, and a sync.
//!
//! `run.rs` does the same exchange in one function between two databases in memory; here each side is its own thread with its own
//! database and its own end of the connection. The steps are the same, with one difference that matters on a socket: when both sides
//! have a lot to say (the lists of a big database are megabytes), they must not both write at once, or neither would ever read. So
//! the desktop always speaks first and the phone listens, then the other way round.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;

use super::crypto::{Ikm, Kind, Secret};
use super::engine::{self, Choice, Conflict, ConflictView, Digest, DigestCollector, RowsCollector, Summary, SyncError, Warning};
use super::files::{self, FilesReport};
use super::net::{self, Channel, Limits, NetError, Window};
use super::peers;
use super::qr::Payload;
use super::stamp;
use crate::documents::library::Files;
use super::wire::{AcceptMessage, CurrencyMessage, DoneMessage, ErrorMessage, HashesMessage, Hello, Message, Mode, ReviewedMessage, RowsMessage, Table};

/// Where the sync reads and writes. The app's is the vault (it fails with `Locked` when the app is locked); the tests' is a
/// database in memory. The closure runs with the database for as long as it takes, and never while waiting for the network.
pub trait Db: Send + Sync {
    fn with<R>(&self, f: impl FnOnce(&mut Connection) -> Result<R, SyncError>) -> Result<R, SyncError>;

    /// The same, together with the folder of the documents' files and their keys. Fails with `Locked` when there are no keys.
    fn with_documents<R>(&self, f: impl FnOnce(&Connection, &Files) -> Result<R, SyncError>) -> Result<R, SyncError>;
}

impl Db for crate::vault::Vault {
    fn with<R>(&self, f: impl FnOnce(&mut Connection) -> Result<R, SyncError>) -> Result<R, SyncError> {
        let mut outcome = None;
        self.with_conn(|conn| {
            outcome = Some(f(conn));
            Ok(())
        })
        .map_err(|_| SyncError::Locked)?;
        outcome.unwrap_or(Err(SyncError::Internal))
    }

    fn with_documents<R>(&self, f: impl FnOnce(&Connection, &Files) -> Result<R, SyncError>) -> Result<R, SyncError> {
        let mut outcome = None;
        self.with_files(|conn, files| {
            outcome = Some(f(conn, files));
            Ok(())
        })
        .map_err(|_| SyncError::Locked)?;
        outcome.unwrap_or(Err(SyncError::Internal))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Desktop,
    Phone,
}

#[derive(Debug)]
pub enum FlowError {
    Net(NetError),
    Sync(SyncError),
    /// The other device sent an error message and ended the sync.
    Peer { code: String, message: String },
    /// A message that does not belong at this point.
    Protocol(&'static str),
    /// The person said no, on the desktop.
    Declined,
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowError::Net(e) => write!(f, "{e}"),
            FlowError::Sync(e) => write!(f, "{e}"),
            FlowError::Peer { code, .. } => write!(f, "the other device ended the sync ({code})"),
            FlowError::Protocol(what) => write!(f, "unexpected {what}"),
            FlowError::Declined => f.write_str("the pairing was declined"),
        }
    }
}

impl std::error::Error for FlowError {}

impl From<NetError> for FlowError {
    fn from(e: NetError) -> Self {
        FlowError::Net(e)
    }
}

impl From<SyncError> for FlowError {
    fn from(e: SyncError) -> Self {
        FlowError::Sync(e)
    }
}

impl From<super::wire::WireError> for FlowError {
    fn from(e: super::wire::WireError) -> Self {
        FlowError::Net(NetError::Wire(e))
    }
}

/// The code that goes in an error message to the other device (the list is `wire::ERROR_CODES`).
pub fn error_code(error: &FlowError) -> &'static str {
    match error {
        FlowError::Sync(SyncError::SchemaMismatch { .. }) => "schema_mismatch",
        FlowError::Sync(SyncError::ProtocolMismatch) => "protocol_mismatch",
        FlowError::Sync(SyncError::ClockFarOff) => "clock_far_off",
        FlowError::Sync(SyncError::Currency) => "currency",
        FlowError::Sync(SyncError::Invalid(_)) | FlowError::Sync(SyncError::Wire(_)) | FlowError::Protocol(_) => "invalid",
        FlowError::Sync(SyncError::NotEmpty) | FlowError::Sync(SyncError::Locked) => "busy",
        FlowError::Declined => "cancelled",
        _ => "internal",
    }
}

/// What a finished sync did.
#[derive(Debug)]
pub struct Exchange {
    /// What this device took over from the other.
    pub summary: Summary,
    pub warnings: Vec<Warning>,
    /// Whether the two lists differ at the end. This should never be; if it is, the person is told to sync again.
    pub differences: bool,
    /// The tables that had to be compared row by row.
    pub compared: Vec<Table>,
    pub peer: Hello,
    /// The sync went one way only, so the two are not meant to end equal.
    pub one_way: bool,
    /// What happened to the documents' files, which move after the rows.
    pub files: FilesReport,
}

/// The two things the desktop shows the person when a phone asks to be paired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingRequest {
    pub peer_name: String,
    pub check_code: String,
}

fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn hello_of<D: Db>(db: &D, name: &str) -> Result<Hello, SyncError> {
    db.with(|c| engine::hello(c, name, env!("CARGO_PKG_VERSION")))
}

/// The next message; an error message from the other device ends the conversation here.
pub(super) fn next(channel: &mut Channel) -> Result<Message, FlowError> {
    match channel.receive()? {
        Message::Error(e) if e.code == "cancelled" => Err(FlowError::Declined),
        Message::Error(e) => Err(FlowError::Peer { code: e.code, message: e.message }),
        message => Ok(message),
    }
}

fn expect_hello(channel: &mut Channel) -> Result<Hello, FlowError> {
    match next(channel)? {
        Message::Hello(h) => Ok(h),
        _ => Err(FlowError::Protocol("message instead of a hello")),
    }
}

fn expect_hashes(channel: &mut Channel) -> Result<Vec<(Table, String)>, FlowError> {
    match next(channel)? {
        Message::Hashes(h) => Ok(h.hashes),
        _ => Err(FlowError::Protocol("message instead of the hashes")),
    }
}

/// Best effort: tells the other device why this one stops, so its person sees a reason and not a dropped connection.
fn tell_peer(channel: &mut Channel, error: &FlowError) {
    if matches!(error, FlowError::Net(_) | FlowError::Peer { .. }) {
        return;
    }
    let _ = channel.send(&Message::Error(ErrorMessage { code: error_code(error).to_string(), message: String::new() }));
}

fn receive_digest(channel: &mut Channel, tables: &[Table]) -> Result<Digest, FlowError> {
    let mut collector = DigestCollector::new(tables.to_vec());
    while !collector.is_complete() {
        match next(channel)? {
            Message::Digest(d) => collector.push(d)?,
            _ => return Err(FlowError::Protocol("message instead of the lists")),
        }
    }
    Ok(collector.finish()?)
}

fn receive_rows(channel: &mut Channel, tables: &[Table]) -> Result<engine::Incoming, FlowError> {
    let mut collector = RowsCollector::new(tables.to_vec());
    while !collector.is_complete() {
        match next(channel)? {
            Message::Rows(r) => collector.push(r)?,
            _ => return Err(FlowError::Protocol("message instead of the rows")),
        }
    }
    Ok(collector.finish()?)
}

fn send_all(channel: &mut Channel, messages: &[Message]) -> Result<(), FlowError> {
    for message in messages {
        channel.send(message)?;
    }
    Ok(())
}

/// What the screens do while a pairing or a sync runs: they show things and ask the person. Every method may block for as long as
/// the person takes; the defaults do nothing, which is what a device with nobody to ask (the tests) wants for most of them.
pub trait Prompts {
    /// Desktop, pairing: the six digits to show while the person types them on the phone.
    fn show_code(&mut self, _request: &PairingRequest) {}
    /// Phone, pairing: the person types the six digits that the desktop shows. True when they are right.
    fn enter_code(&mut self, _expected: &str) -> bool {
        false
    }
    /// The connection is made and the exchange is on its way.
    fn running(&mut self) {}
    /// This device reviews the conflicts: the person chooses, for each, the version that is true. None means they gave up.
    /// `more` is how many further conflicts will be settled by the newer change.
    fn review(&mut self, _conflicts: &[ConflictView], _more: usize) -> Option<Vec<Choice>> {
        None
    }
    /// The other device is reviewing the conflicts, and this one waits.
    fn waiting_review(&mut self) {}
}

/// How long the other device waits for the person to review the conflicts.
pub const REVIEW_WAIT: Duration = Duration::from_secs(8 * 60);

/// The device that reviews the conflicts of a sync: the one that takes the changes in. When both do, the desktop.
fn reviewer(mode: Mode) -> Role {
    if mode == Mode::ToPhone {
        Role::Phone
    } else {
        Role::Desktop
    }
}

/// Whether this device sends its changes in a sync that goes this way.
pub(super) fn sends(role: Role, mode: Mode) -> bool {
    match mode {
        Mode::Both => true,
        Mode::ToDesktop => role == Role::Phone,
        Mode::ToPhone => role == Role::Desktop,
    }
}

/// Whether this device takes the other's changes in.
pub(super) fn receives(role: Role, mode: Mode) -> bool {
    match mode {
        Mode::Both => true,
        Mode::ToDesktop => role == Role::Desktop,
        Mode::ToPhone => role == Role::Phone,
    }
}

/// The rows messages of a device that sends nothing: one empty message for each table, so the other side's count is right.
fn nothing_to_send(tables: &[Table]) -> Vec<Message> {
    tables.iter().map(|table| Message::Rows(RowsMessage { table: *table, rows: Vec::new(), deleted: Vec::new(), last: true })).collect()
}

/// Everything after the two hellos: the currency, the lists, the conflicts, the rows, and the check that both end equal.
#[allow(clippy::too_many_arguments)]
pub fn exchange<D: Db>(channel: &mut Channel, db: &D, role: Role, mine: &Hello, theirs: &Hello, mode: Mode, prompts: &mut dyn Prompts) -> Result<Exchange, FlowError> {
    // The rows first, all the way to the check that they end equal; the files after, which a broken connection can cut short
    // without undoing any of that.
    let result = exchange_steps(channel, db, role, mine, theirs, mode, prompts).and_then(|mut done| {
        done.files = files::run(channel, db, role, mode)?;
        Ok(done)
    });
    if let Err(error) = &result {
        tell_peer(channel, error);
    }
    result
}

/// The hashes and the lists of the tables that differ, both ways. Returns this side's list, the other's, and the tables.
fn compare<D: Db>(channel: &mut Channel, db: &D, role: Role) -> Result<Option<(Digest, Digest, Vec<Table>)>, FlowError> {
    let digest_mine = db.with(|c| engine::digest(c))?;
    let hashes_mine = engine::hashes(&digest_mine);
    channel.send(&Message::Hashes(HashesMessage { hashes: hashes_mine.clone() }))?;
    let hashes_theirs = expect_hashes(channel)?;
    let tables = engine::differing(&hashes_mine, &hashes_theirs);
    if tables.is_empty() {
        return Ok(None);
    }
    // The lists of the tables that differ: the desktop sends first.
    let digest_messages = engine::digest_messages(&digest_mine, &tables);
    let digest_theirs = match role {
        Role::Desktop => {
            send_all(channel, &digest_messages)?;
            receive_digest(channel, &tables)?
        }
        Role::Phone => {
            let theirs = receive_digest(channel, &tables)?;
            send_all(channel, &digest_messages)?;
            theirs
        }
    };
    Ok(Some((digest_mine, digest_theirs, tables)))
}

/// The conflicts: the device that does not review sends its version of each, the reviewer shows both to the person and carries out
/// the choices on its own stamps. Both then compare their lists again.
fn review_conflicts<D: Db>(
    channel: &mut Channel,
    db: &D,
    role: Role,
    mode: Mode,
    conflicts: &[Conflict],
    more: usize,
    prompts: &mut dyn Prompts,
) -> Result<(), FlowError> {
    if role == reviewer(mode) {
        let tables = engine::conflict_tables(conflicts);
        let theirs = receive_rows(channel, &tables)?;
        let views = db.with(|c| engine::describe(c, conflicts, &theirs))?;
        channel.wait_longer(REVIEW_WAIT);
        let choices = prompts.review(&views, more).ok_or(FlowError::Declined)?;
        channel.wait_normally();
        db.with(|c| engine::decide(c, conflicts, &choices))?;
        channel.send(&Message::Reviewed(ReviewedMessage {}))?;
    } else {
        let messages = db.with(|c| engine::conflict_rows(c, conflicts))?;
        send_all(channel, &messages)?;
        prompts.waiting_review();
        channel.wait_longer(REVIEW_WAIT);
        match next(channel)? {
            Message::Reviewed(_) => {}
            _ => return Err(FlowError::Protocol("message instead of the end of the review")),
        }
        channel.wait_normally();
    }
    Ok(())
}

fn exchange_steps<D: Db>(channel: &mut Channel, db: &D, role: Role, mine: &Hello, theirs: &Hello, mode: Mode, prompts: &mut dyn Prompts) -> Result<Exchange, FlowError> {
    let now_ms = stamp::now_ms();
    let warnings = engine::check_hello(mine, theirs, now_ms)?;

    // Currency first: the side that is behind takes over the changes and converts its own rows. Only the desktop makes changes.
    match mine.generation.cmp(&theirs.generation) {
        std::cmp::Ordering::Greater => {
            if role == Role::Phone {
                return Err(FlowError::Sync(SyncError::Currency));
            }
            let changes = db.with(|c| engine::changes_after(c, theirs.generation))?;
            channel.send(&Message::Currency(CurrencyMessage { changes }))?;
        }
        std::cmp::Ordering::Less => {
            if role == Role::Desktop {
                return Err(FlowError::Sync(SyncError::Currency));
            }
            let changes = match next(channel)? {
                Message::Currency(c) => c.changes,
                _ => return Err(FlowError::Protocol("message instead of the currency changes")),
            };
            db.with(|c| engine::apply_currency(c, &changes, &today(), now_ms))?;
        }
        std::cmp::Ordering::Equal => {}
    }

    // Which tables differ and what is in them. If the same thing was changed on both devices since they last met, the person
    // says which version is true, and the lists are compared once more with the choices in them.
    let mut reviewed = false;
    let (digest_mine, digest_theirs, tables) = loop {
        let Some((digest_mine, digest_theirs, tables)) = compare(channel, db, role)? else {
            return Ok(Exchange { summary: Summary::default(), warnings, differences: false, compared: Vec::new(), peer: theirs.clone(), one_way: mode != Mode::Both, files: FilesReport::default() });
        };
        if !reviewed {
            let threshold = engine::conflict_threshold(mine.since, theirs.since);
            let (conflicts, more) = engine::conflicts(&digest_mine, &digest_theirs, &tables, threshold);
            if !conflicts.is_empty() {
                review_conflicts(channel, db, role, mode, &conflicts, more, prompts)?;
                reviewed = true;
                continue;
            }
        }
        break (digest_mine, digest_theirs, tables);
    };

    // What the other side lacks, worked out before anything here changes; then the rows, again the desktop first. In a sync
    // that goes one way, the device that only receives sends empty messages.
    let rows_out = if sends(role, mode) { db.with(|c| engine::outgoing(c, &digest_mine, &digest_theirs, &tables))? } else { nothing_to_send(&tables) };
    let incoming = match role {
        Role::Desktop => {
            send_all(channel, &rows_out)?;
            receive_rows(channel, &tables)?
        }
        Role::Phone => {
            let incoming = receive_rows(channel, &tables)?;
            send_all(channel, &rows_out)?;
            incoming
        }
    };
    let summary = if receives(role, mode) {
        db.with(|c| engine::apply(c, &incoming, stamp::now_ms(), mine.since))?
    } else if incoming.is_empty() {
        Summary::default()
    } else {
        return Err(FlowError::Protocol("rows sent against the direction of the sync"));
    };

    // Both say what they hold now. Equal means done; a sync that goes one way is not meant to end equal.
    let hash = db.with(|c| Ok(engine::full_hash(&engine::digest(c)?)))?;
    let done = Message::Done(DoneMessage { hash: hash.clone() });
    let theirs_done = match role {
        Role::Desktop => {
            channel.send(&done)?;
            next(channel)?
        }
        Role::Phone => {
            let theirs = next(channel)?;
            channel.send(&done)?;
            theirs
        }
    };
    let differences = match theirs_done {
        Message::Done(d) => mode == Mode::Both && d.hash != hash,
        _ => return Err(FlowError::Protocol("message instead of the end of the sync")),
    };
    Ok(Exchange { summary, warnings, differences, compared: tables, peer: theirs.clone(), one_way: mode != Mode::Both, files: FilesReport::default() })
}

// ---------------------------------------------------------------------------------------------------------------
// a sync between two devices that are paired

/// The date of the last sync with a device, in the hello that says it.
fn with_since(mut hello: Hello, since: Option<u64>, mode: Mode) -> Hello {
    hello.since = since;
    hello.mode = mode;
    hello
}

/// The desktop's side of a sync: waits in the window for a paired phone, and syncs with it. `device_ids` are the paired
/// devices in the same order as the secrets the window was opened with. Returns what was done and with which device.
pub fn sync_as_desktop<D: Db>(
    window: Window,
    cancel: &Arc<AtomicBool>,
    db: &D,
    name: &str,
    device_ids: &[String],
    mode: Mode,
    prompts: &mut dyn Prompts,
) -> Result<(Exchange, String), FlowError> {
    let (mut channel, index) = window.accept(cancel)?;
    prompts.running();
    let expected = device_ids.get(index).ok_or(FlowError::Protocol("device"))?.clone();
    let result = (|| {
        let theirs = expect_hello(&mut channel)?;
        if theirs.device_id != expected {
            return Err(FlowError::Protocol("device"));
        }
        let since = db.with(|c| peers::last_sync_ms(c, &expected))?;
        let mine = with_since(hello_of(db, name)?, since, mode);
        channel.send(&Message::Hello(mine.clone()))?;
        let done = exchange(&mut channel, db, Role::Desktop, &mine, &theirs, mode, prompts)?;
        db.with(|c| peers::touch(c, &expected, &now_iso()))?;
        Ok(done)
    })();
    match result {
        Ok(done) => Ok((done, expected)),
        Err(error) => {
            tell_peer(&mut channel, &error);
            Err(error)
        }
    }
}

/// The phone's side of a sync: reads the code, connects to the desktop, and syncs.
pub fn sync_as_phone<D: Db>(
    payload: &Payload,
    db: &D,
    name: &str,
    limits: Limits,
    cancel: Arc<AtomicBool>,
    prompts: &mut dyn Prompts,
) -> Result<Exchange, FlowError> {
    if payload.kind != Kind::Sync {
        return Err(FlowError::Protocol("code"));
    }
    let paired = db.with(|c| peers::secrets(c))?;
    let (device_id, secret) = paired.into_iter().next().ok_or(FlowError::Protocol("pairing"))?;
    let mut channel = net::connect(payload, Ikm::for_sync(&secret, &payload.secret), limits, Some(cancel))?;
    prompts.running();
    let result = (|| {
        let since = db.with(|c| peers::last_sync_ms(c, &device_id))?;
        let first = with_since(hello_of(db, name)?, since, Mode::Both);
        channel.send(&Message::Hello(first.clone()))?;
        let theirs = expect_hello(&mut channel)?;
        if theirs.device_id != device_id {
            return Err(FlowError::Protocol("device"));
        }
        // The way the sync goes is the desktop's to say.
        let done = exchange(&mut channel, db, Role::Phone, &first, &theirs, theirs.mode, prompts)?;
        db.with(|c| peers::touch(c, &device_id, &now_iso()))?;
        Ok(done)
    })();
    if let Err(error) = &result {
        tell_peer(&mut channel, error);
    }
    result
}

// ---------------------------------------------------------------------------------------------------------------
// pairing (once)

/// How long the desktop waits for the person to type the digits on the phone, and the phone for the desktop's answer.
pub const CONFIRM_WAIT: Duration = Duration::from_secs(5 * 60);

/// The desktop's side of a pairing. `secret` is the new pairing secret that is in the code. The desktop shows the six digits and
/// waits for the phone to say that the person typed them right; only then is the phone stored and the first copy made.
pub fn pair_as_desktop<D: Db>(
    window: Window,
    secret: &Secret,
    cancel: &Arc<AtomicBool>,
    db: &D,
    name: &str,
    prompts: &mut dyn Prompts,
) -> Result<Exchange, FlowError> {
    let (mut channel, _) = window.accept(cancel)?;
    let result = (|| {
        let first = expect_hello(&mut channel)?;
        let mine = with_since(hello_of(db, name)?, None, Mode::Both);
        if first.device_id == mine.device_id {
            return Err(FlowError::Sync(SyncError::Invalid("device")));
        }
        engine::check_hello(&mine, &first, stamp::now_ms())?;
        prompts.show_code(&PairingRequest { peer_name: first.name.clone(), check_code: channel.check_code().to_string() });
        channel.wait_longer(CONFIRM_WAIT);
        match next(&mut channel)? {
            Message::Accept(_) => {}
            _ => return Err(FlowError::Protocol("message instead of the answer")),
        }
        channel.wait_normally();
        channel.send(&Message::Hello(mine.clone()))?;
        // the new phone takes the currency, and the record of its changes, as they are
        let changes = db.with(|c| engine::changes_after(c, 0))?;
        channel.send(&Message::Currency(CurrencyMessage { changes }))?;
        // it answers with a hello that says what it holds now; only then is it stored, so a phone that turns the pairing down
        // (it holds data from another desktop) leaves nothing behind here
        let theirs = expect_hello(&mut channel)?;
        db.with(|c| peers::add(c, &first.device_id, &first.name, secret, &now_iso()))?;
        prompts.running();
        let done = exchange(&mut channel, db, Role::Desktop, &mine, &theirs, Mode::Both, prompts)?;
        db.with(|c| peers::touch(c, &first.device_id, &now_iso()))?;
        Ok(done)
    })();
    if let Err(error) = &result {
        tell_peer(&mut channel, error);
    }
    result
}

/// The phone's side of a pairing. It must be new or erased, or it must already be paired with the desktop it is pairing with (it
/// got its data from there, and the desktop forgot it, or the person wants new keys). The desktop shows six digits; the person
/// types them here, and only if they are right does the pairing go on.
pub fn pair_as_phone<D: Db>(
    payload: &Payload,
    db: &D,
    name: &str,
    limits: Limits,
    cancel: Arc<AtomicBool>,
    prompts: &mut dyn Prompts,
) -> Result<Exchange, FlowError> {
    if payload.kind != Kind::Pair {
        return Err(FlowError::Protocol("code"));
    }
    let empty = db.with(|c| engine::is_empty(c))?;
    let known = db.with(|c| peers::list(c))?;
    // Data and no desktop it came from: refused before anything is sent. (With a desktop it came from, the desktop's own
    // identity decides, below.)
    if !empty && known.is_empty() {
        return Err(FlowError::Sync(SyncError::NotEmpty));
    }
    let mut channel = net::connect(payload, Ikm::for_pairing(&payload.secret), limits, Some(cancel))?;
    let result = (|| {
        // When it last synced with any desktop it knows, so that the conflicts since then can be found.
        let since = known.iter().filter_map(|p| db.with(|c| peers::last_sync_ms(c, &p.device_id)).ok().flatten()).max();
        let first = with_since(hello_of(db, name)?, since, Mode::Both);
        channel.send(&Message::Hello(first.clone()))?;
        channel.wait_longer(CONFIRM_WAIT);
        if prompts.enter_code(channel.check_code()) {
            channel.send(&Message::Accept(AcceptMessage {}))?;
        } else {
            let _ = channel.send(&Message::Error(ErrorMessage { code: "cancelled".into(), message: String::new() }));
            return Err(FlowError::Declined);
        }
        channel.wait_normally();
        let theirs = expect_hello(&mut channel)?;
        engine::check_hello(&first, &theirs, stamp::now_ms())?;
        let changes = match next(&mut channel)? {
            Message::Currency(c) => c.changes,
            _ => return Err(FlowError::Protocol("message instead of the currency changes")),
        };
        let mut mine_since = None;
        if empty {
            db.with(|c| engine::adopt_currency(c, &theirs.currency, theirs.generation, &changes))?;
        } else if known.iter().any(|p| p.device_id == theirs.device_id) {
            // Its data came from this desktop: only the currency changes it has not seen yet are needed.
            if first.generation > theirs.generation {
                return Err(FlowError::Sync(SyncError::Currency));
            }
            let missing: Vec<_> = changes.into_iter().filter(|c| c.generation > first.generation).collect();
            db.with(|c| engine::apply_currency(c, &missing, &today(), stamp::now_ms()))?;
            mine_since = db.with(|c| peers::last_sync_ms(c, &theirs.device_id))?;
        } else {
            return Err(FlowError::Sync(SyncError::NotEmpty));
        }
        db.with(|c| peers::add(c, &theirs.device_id, &theirs.name, &payload.secret, &now_iso()))?;
        prompts.running();
        let mine = with_since(hello_of(db, name)?, mine_since, Mode::Both);
        channel.send(&Message::Hello(mine.clone()))?;
        let done = exchange(&mut channel, db, Role::Phone, &mine, &theirs, Mode::Both, prompts)?;
        db.with(|c| peers::touch(c, &theirs.device_id, &now_iso()))?;
        Ok(done)
    })();
    if let Err(error) = &result {
        tell_peer(&mut channel, error);
    }
    result
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::Mutex;

    /// A database in memory behind a lock, as the vault is; it can be "locked". It has a folder for the documents' files and keys of
    /// its own, so two of them hold the same file sealed in two ways, as two devices do.
    pub struct LocalDb {
        pub conn: Mutex<Connection>,
        pub locked: std::sync::atomic::AtomicBool,
        pub dir: tempfile::TempDir,
        pub keys: crate::documents::DocKeys,
    }

    impl LocalDb {
        pub fn new(conn: Connection) -> LocalDb {
            use std::sync::atomic::{AtomicU8, Ordering};
            static NEXT_KEY: AtomicU8 = AtomicU8::new(1);
            let seed = NEXT_KEY.fetch_add(1, Ordering::Relaxed);
            LocalDb {
                conn: Mutex::new(conn),
                locked: std::sync::atomic::AtomicBool::new(false),
                dir: tempfile::tempdir().expect("a folder for the files"),
                keys: crate::documents::DocKeys::from_db_key(&crate::db::DbKey::from_bytes([seed; 32])),
            }
        }

        pub fn store(&self) -> crate::documents::Store {
            crate::documents::Store::new(self.dir.path())
        }
    }

    impl Db for LocalDb {
        fn with<R>(&self, f: impl FnOnce(&mut Connection) -> Result<R, SyncError>) -> Result<R, SyncError> {
            if self.locked.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(SyncError::Locked);
            }
            f(&mut self.conn.lock().unwrap_or_else(|e| e.into_inner()))
        }

        fn with_documents<R>(&self, f: impl FnOnce(&Connection, &Files) -> Result<R, SyncError>) -> Result<R, SyncError> {
            if self.locked.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(SyncError::Locked);
            }
            let store = self.store();
            f(&self.conn.lock().unwrap_or_else(|e| e.into_inner()), &Files { store: &store, keys: &self.keys })
        }
    }
}
