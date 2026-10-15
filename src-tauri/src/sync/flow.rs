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
use super::engine::{self, Digest, DigestCollector, RowsCollector, Summary, SyncError, Warning};
use super::net::{self, Channel, Limits, NetError, Window};
use super::peers;
use super::qr::Payload;
use super::stamp;
use super::wire::{AcceptMessage, CurrencyMessage, DoneMessage, ErrorMessage, HashesMessage, Hello, Message, Table};

/// Where the sync reads and writes. The app's is the vault (it fails with `Locked` when the app is locked); the tests' is a
/// database in memory. The closure runs with the database for as long as it takes, and never while waiting for the network.
pub trait Db: Send + Sync {
    fn with<R>(&self, f: impl FnOnce(&mut Connection) -> Result<R, SyncError>) -> Result<R, SyncError>;
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
fn next(channel: &mut Channel) -> Result<Message, FlowError> {
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

/// Everything after the two hellos: the currency, the lists, the rows, and the check that both end equal.
pub fn exchange<D: Db>(channel: &mut Channel, db: &D, role: Role, mine: &Hello, theirs: &Hello, since_ms: Option<u64>) -> Result<Exchange, FlowError> {
    let result = exchange_steps(channel, db, role, mine, theirs, since_ms);
    if let Err(error) = &result {
        tell_peer(channel, error);
    }
    result
}

fn exchange_steps<D: Db>(channel: &mut Channel, db: &D, role: Role, mine: &Hello, theirs: &Hello, since_ms: Option<u64>) -> Result<Exchange, FlowError> {
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

    // Which tables differ.
    let digest_mine = db.with(|c| engine::digest(c))?;
    let hashes_mine = engine::hashes(&digest_mine);
    channel.send(&Message::Hashes(HashesMessage { hashes: hashes_mine.clone() }))?;
    let hashes_theirs = expect_hashes(channel)?;
    let tables = engine::differing(&hashes_mine, &hashes_theirs);
    if tables.is_empty() {
        return Ok(Exchange { summary: Summary::default(), warnings, differences: false, compared: tables, peer: theirs.clone() });
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

    // What the other side lacks, worked out before anything here changes; then the rows, again the desktop first.
    let rows_out = db.with(|c| engine::outgoing(c, &digest_mine, &digest_theirs, &tables))?;
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
    let summary = db.with(|c| engine::apply(c, &incoming, stamp::now_ms(), since_ms))?;

    // Both say what they hold now. Equal means done.
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
        Message::Done(d) => d.hash != hash,
        _ => return Err(FlowError::Protocol("message instead of the end of the sync")),
    };
    Ok(Exchange { summary, warnings, differences, compared: tables, peer: theirs.clone() })
}

// ---------------------------------------------------------------------------------------------------------------
// a sync between two devices that are paired

/// The desktop's side of a sync: waits in the window for a paired phone, and syncs with it. `device_ids` are the paired
/// devices in the same order as the secrets the window was opened with. Returns what was done and with which device.
pub fn sync_as_desktop<D: Db>(
    window: Window,
    cancel: &Arc<AtomicBool>,
    db: &D,
    name: &str,
    device_ids: &[String],
    on_connect: &mut dyn FnMut(),
) -> Result<(Exchange, String), FlowError> {
    let (mut channel, index) = window.accept(cancel)?;
    on_connect();
    let expected = device_ids.get(index).ok_or(FlowError::Protocol("device"))?.clone();
    let result = (|| {
        let theirs = expect_hello(&mut channel)?;
        if theirs.device_id != expected {
            return Err(FlowError::Protocol("device"));
        }
        let mine = hello_of(db, name)?;
        channel.send(&Message::Hello(mine.clone()))?;
        let since = db.with(|c| peers::last_sync_ms(c, &expected))?;
        let done = exchange(&mut channel, db, Role::Desktop, &mine, &theirs, since)?;
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
    on_connect: &mut dyn FnMut(),
) -> Result<Exchange, FlowError> {
    if payload.kind != Kind::Sync {
        return Err(FlowError::Protocol("code"));
    }
    let paired = db.with(|c| peers::secrets(c))?;
    let (device_id, secret) = paired.into_iter().next().ok_or(FlowError::Protocol("pairing"))?;
    let mut channel = net::connect(payload, Ikm::for_sync(&secret, &payload.secret), limits, Some(cancel))?;
    on_connect();
    let result = (|| {
        let mine = hello_of(db, name)?;
        channel.send(&Message::Hello(mine.clone()))?;
        let theirs = expect_hello(&mut channel)?;
        if theirs.device_id != device_id {
            return Err(FlowError::Protocol("device"));
        }
        let since = db.with(|c| peers::last_sync_ms(c, &device_id))?;
        let done = exchange(&mut channel, db, Role::Phone, &mine, &theirs, since)?;
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

/// How long the phone waits for the person to compare the codes and say yes on the desktop.
pub const CONFIRM_WAIT: Duration = Duration::from_secs(5 * 60);

/// The desktop's side of a pairing. `secret` is the new pairing secret that is in the code. `decide` is shown the phone's name and the
/// check code and returns what the person said; it may block for a long time. Only a yes stores the phone and starts the first copy.
pub fn pair_as_desktop<D: Db>(
    window: Window,
    secret: &Secret,
    cancel: &Arc<AtomicBool>,
    db: &D,
    name: &str,
    decide: &mut dyn FnMut(&PairingRequest) -> bool,
) -> Result<Exchange, FlowError> {
    let (mut channel, _) = window.accept(cancel)?;
    let result = (|| {
        let first = expect_hello(&mut channel)?;
        let mine = hello_of(db, name)?;
        if first.device_id == mine.device_id {
            return Err(FlowError::Sync(SyncError::Invalid("device")));
        }
        engine::check_hello(&mine, &first, stamp::now_ms())?;
        let request = PairingRequest { peer_name: first.name.clone(), check_code: channel.check_code().to_string() };
        if !decide(&request) {
            let _ = channel.send(&Message::Error(ErrorMessage { code: "cancelled".into(), message: String::new() }));
            return Err(FlowError::Declined);
        }
        channel.send(&Message::Accept(AcceptMessage {}))?;
        db.with(|c| peers::add(c, &first.device_id, &first.name, secret, &now_iso()))?;
        channel.send(&Message::Hello(mine.clone()))?;
        // the new phone takes the currency, and the record of its changes, as they are
        let changes = db.with(|c| engine::changes_after(c, 0))?;
        channel.send(&Message::Currency(CurrencyMessage { changes }))?;
        // it answers with a hello that says what it holds now
        let theirs = expect_hello(&mut channel)?;
        let done = exchange(&mut channel, db, Role::Desktop, &mine, &theirs, None)?;
        db.with(|c| peers::touch(c, &first.device_id, &now_iso()))?;
        Ok(done)
    })();
    if let Err(error) = &result {
        tell_peer(&mut channel, error);
    }
    result
}

/// The phone's side of a pairing. It must be new or erased. `show_code` is called with the check code as soon as the connection is
/// made, and the phone then waits for the person to confirm on the desktop.
pub fn pair_as_phone<D: Db>(
    payload: &Payload,
    db: &D,
    name: &str,
    limits: Limits,
    cancel: Arc<AtomicBool>,
    show_code: &mut dyn FnMut(&str),
) -> Result<Exchange, FlowError> {
    if payload.kind != Kind::Pair {
        return Err(FlowError::Protocol("code"));
    }
    if !db.with(|c| engine::is_empty(c))? {
        return Err(FlowError::Sync(SyncError::NotEmpty));
    }
    let mut channel = net::connect(payload, Ikm::for_pairing(&payload.secret), limits, Some(cancel))?;
    let result = (|| {
        let first = hello_of(db, name)?;
        channel.send(&Message::Hello(first.clone()))?;
        show_code(channel.check_code());
        channel.wait_longer(CONFIRM_WAIT);
        match next(&mut channel)? {
            Message::Accept(_) => {}
            _ => return Err(FlowError::Protocol("message instead of the answer")),
        }
        channel.wait_normally();
        let theirs = expect_hello(&mut channel)?;
        engine::check_hello(&first, &theirs, stamp::now_ms())?;
        let changes = match next(&mut channel)? {
            Message::Currency(c) => c.changes,
            _ => return Err(FlowError::Protocol("message instead of the currency changes")),
        };
        db.with(|c| engine::adopt_currency(c, &theirs.currency, theirs.generation, &changes))?;
        db.with(|c| peers::add(c, &theirs.device_id, &theirs.name, &payload.secret, &now_iso()))?;
        let mine = hello_of(db, name)?;
        channel.send(&Message::Hello(mine.clone()))?;
        let done = exchange(&mut channel, db, Role::Phone, &mine, &theirs, None)?;
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

    /// A database in memory behind a lock, as the vault is; it can be "locked".
    pub struct LocalDb {
        pub conn: Mutex<Connection>,
        pub locked: std::sync::atomic::AtomicBool,
    }

    impl LocalDb {
        pub fn new(conn: Connection) -> LocalDb {
            LocalDb { conn: Mutex::new(conn), locked: std::sync::atomic::AtomicBool::new(false) }
        }
    }

    impl Db for LocalDb {
        fn with<R>(&self, f: impl FnOnce(&mut Connection) -> Result<R, SyncError>) -> Result<R, SyncError> {
            if self.locked.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(SyncError::Locked);
            }
            f(&mut self.conn.lock().unwrap_or_else(|e| e.into_inner()))
        }
    }
}
