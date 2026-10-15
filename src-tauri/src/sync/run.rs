//! The whole conversation of a sync between two devices, run in memory (`docs/sync-design.md`, section 8).
//!
//! Every message goes through the same bytes as on the wire (`Message::to_bytes` and `from_bytes`), so the size limit and
//! all the checks apply. The network code of the app drives the same steps one message at a time; this is the same
//! sequence in one function, which is what the tests and the property tests run.

use rusqlite::Connection;

use super::engine::{self, Digest, DigestCollector, Incoming, RowsCollector, Summary, SyncError, Warning};
use super::wire::{Message, Table, WireError};

#[derive(Debug, Default)]
pub struct Outcome {
    /// What the first device took over, and what the second did.
    pub first: Summary,
    pub second: Summary,
    /// The tables that differed and had to be compared row by row.
    pub compared: Vec<Table>,
    pub warnings: Vec<Warning>,
    /// Whether the two ended with different lists. This should never be; if it is, a sync run again will say more.
    pub differences: bool,
    pub messages: usize,
    pub bytes: usize,
}

struct Wire {
    messages: usize,
    bytes: usize,
}

impl Wire {
    /// Sends a message through its bytes and reads it back, as the other device would.
    fn carry(&mut self, message: &Message) -> Result<Message, SyncError> {
        let bytes = message.to_bytes()?;
        self.messages += 1;
        self.bytes += bytes.len();
        Ok(Message::from_bytes(&bytes)?)
    }
}

fn digest_of(wire: &mut Wire, mine: &Digest, tables: &[Table]) -> Result<DigestCollector, SyncError> {
    let mut collector = DigestCollector::new(tables.to_vec());
    for message in engine::digest_messages(mine, tables) {
        match wire.carry(&message)? {
            Message::Digest(d) => collector.push(d)?,
            _ => return Err(SyncError::Wire(WireError::Order)),
        }
    }
    Ok(collector)
}

fn rows_of(wire: &mut Wire, messages: Vec<Message>, tables: &[Table]) -> Result<Incoming, SyncError> {
    let mut collector = RowsCollector::new(tables.to_vec());
    for message in messages {
        match wire.carry(&message)? {
            Message::Rows(r) => collector.push(r)?,
            _ => return Err(SyncError::Wire(WireError::Order)),
        }
    }
    collector.finish()
}

/// Syncs `a` with `b`. `today` is the date for the history entries of a currency change, `now_ms` the wall clock, and
/// `since_ms` the time of their last sync.
pub fn sync(a: &mut Connection, b: &mut Connection, today: &str, now_ms: u64, since_ms: Option<u64>) -> Result<Outcome, SyncError> {
    let mut wire = Wire { messages: 0, bytes: 0 };
    let mut outcome = Outcome::default();

    // hello
    let hello_a = engine::hello(a, "first", "0.1.0")?;
    let hello_b = engine::hello(b, "second", "0.1.0")?;
    let seen_by_b = match wire.carry(&Message::Hello(hello_a.clone()))? {
        Message::Hello(h) => h,
        _ => return Err(SyncError::Wire(WireError::Order)),
    };
    let seen_by_a = match wire.carry(&Message::Hello(hello_b.clone()))? {
        Message::Hello(h) => h,
        _ => return Err(SyncError::Wire(WireError::Order)),
    };
    outcome.warnings.extend(engine::check_hello(&hello_a, &seen_by_a, now_ms)?);
    engine::check_hello(&hello_b, &seen_by_b, now_ms)?;

    // currency: the one that is behind takes over the other's changes before anything is compared
    if hello_a.generation != hello_b.generation {
        let (ahead, behind, behind_generation) =
            if hello_a.generation > hello_b.generation { (&*a, &mut *b, hello_b.generation) } else { (&*b, &mut *a, hello_a.generation) };
        let changes = engine::changes_after(ahead, behind_generation)?;
        let changes = match wire.carry(&Message::Currency(super::wire::CurrencyMessage { changes }))? {
            Message::Currency(c) => c.changes,
            _ => return Err(SyncError::Wire(WireError::Order)),
        };
        engine::apply_currency(behind, &changes, today, now_ms)?;
    }

    // which tables differ
    let (digest_a, digest_b) = (engine::digest(a)?, engine::digest(b)?);
    let (hashes_a, hashes_b) = (engine::hashes(&digest_a), engine::hashes(&digest_b));
    let theirs_for_a = match wire.carry(&Message::Hashes(super::wire::HashesMessage { hashes: hashes_b.clone() }))? {
        Message::Hashes(h) => h.hashes,
        _ => return Err(SyncError::Wire(WireError::Order)),
    };
    let theirs_for_b = match wire.carry(&Message::Hashes(super::wire::HashesMessage { hashes: hashes_a.clone() }))? {
        Message::Hashes(h) => h.hashes,
        _ => return Err(SyncError::Wire(WireError::Order)),
    };
    let tables = engine::differing(&hashes_a, &theirs_for_a);
    if tables != engine::differing(&hashes_b, &theirs_for_b) {
        return Err(SyncError::Invalid("hashes"));
    }
    outcome.compared = tables.clone();

    if !tables.is_empty() {
        // digests of the tables that differ, both ways
        let their_digest_for_a = digest_of(&mut wire, &digest_b, &tables)?.finish()?;
        let their_digest_for_b = digest_of(&mut wire, &digest_a, &tables)?.finish()?;
        // each works out what the other lacks, and sends it; both are worked out before either applies
        let from_a = engine::outgoing(a, &digest_a, &their_digest_for_a, &tables)?;
        let from_b = engine::outgoing(b, &digest_b, &their_digest_for_b, &tables)?;
        let incoming_for_a = rows_of(&mut wire, from_b, &tables)?;
        let incoming_for_b = rows_of(&mut wire, from_a, &tables)?;
        outcome.first = engine::apply(a, &incoming_for_a, now_ms, since_ms)?;
        outcome.second = engine::apply(b, &incoming_for_b, now_ms, since_ms)?;
    }

    // done
    let (done_a, done_b) = (engine::full_hash(&engine::digest(a)?), engine::full_hash(&engine::digest(b)?));
    wire.carry(&Message::Done(super::wire::DoneMessage { hash: done_a.clone() }))?;
    wire.carry(&Message::Done(super::wire::DoneMessage { hash: done_b.clone() }))?;
    outcome.differences = done_a != done_b;
    outcome.messages = wire.messages;
    outcome.bytes = wire.bytes;
    Ok(outcome)
}
