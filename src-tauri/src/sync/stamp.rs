//! Stamps and the hybrid clock behind them (`docs/sync-design.md`, section 10).
//!
//! A stamp says when a row was last changed and on which device: `{ms:013}-{counter:04x}-{device:8 hex}`. It is text of a
//! fixed width, so comparing two stamps as text gives the same answer as comparing (milliseconds, counter, device), and a
//! larger stamp is a newer change. The triggers of migration 0008 make the stamps of local changes from the clock row
//! `sync_clock`; this module reads and moves that clock for the code around it (a sync, a restore, the start of the app).

use rusqlite::{params, Connection};

use crate::keys;
use crate::vault::AppError;

/// How many characters of the device id a stamp carries.
pub const DEVICE_CHARS: usize = 8;
/// The length of a stamp in characters.
pub const STAMP_LEN: usize = 13 + 1 + 4 + 1 + DEVICE_CHARS;

/// The tables that carry a `stamp` (and, except `settings`, a `sync_id`).
pub const STAMPED_TABLES: &[&str] = &["settings", "categories", "budgets", "recurring_transactions", "transactions"];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stamp {
    pub ms: u64,
    pub counter: u16,
    pub device: String,
}

impl Stamp {
    pub fn new(ms: u64, counter: u16, device: &str) -> Stamp {
        Stamp { ms, counter, device: device.to_string() }
    }

    /// `None` unless `text` has exactly the shape `format` makes. A stamp that comes from outside is never trusted.
    pub fn parse(text: &str) -> Option<Stamp> {
        let b = text.as_bytes();
        if b.len() != STAMP_LEN || b[13] != b'-' || b[18] != b'-' {
            return None;
        }
        let lower_hex = |part: &[u8]| part.iter().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c));
        if !b[..13].iter().all(u8::is_ascii_digit) || !lower_hex(&b[14..18]) || !lower_hex(&b[19..]) {
            return None;
        }
        Some(Stamp {
            ms: text[..13].parse().ok()?,
            counter: u16::from_str_radix(&text[14..18], 16).ok()?,
            device: text[19..].to_string(),
        })
    }

    pub fn format(&self) -> String {
        format!("{:013}-{:04x}-{}", self.ms, self.counter, self.device)
    }

    pub fn is_valid(text: &str) -> bool {
        Stamp::parse(text).is_some()
    }
}

/// The wall clock in milliseconds since 1970.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// This device's id: 32 hex characters, made when the database was first migrated.
pub fn device_id(conn: &Connection) -> Result<String, AppError> {
    Ok(conn.query_row("SELECT device_id FROM sync_clock WHERE id = 1", [], |r| r.get(0))?)
}

/// While this is on, the stamp triggers leave stamps alone and deletes leave no marker. A sync, a restore and a
/// currency conversion turn it on inside their transaction, so a rollback turns it off again.
pub fn set_applying(conn: &Connection, on: bool) -> Result<(), AppError> {
    conn.execute("UPDATE sync_clock SET applying = ?1 WHERE id = 1", [on as i64])?;
    Ok(())
}

/// Makes a new stamp from the clock the way the triggers do: the wall clock if it is ahead, otherwise one step on from the
/// last stamp. The clock never goes backwards.
pub fn tick(conn: &Connection) -> Result<Stamp, AppError> {
    let now = now_ms() as i64;
    conn.execute(
        "UPDATE sync_clock SET
           last_counter = CASE WHEN ?1 > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
           last_ms = CASE WHEN ?1 > last_ms THEN ?1 WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
         WHERE id = 1",
        [now],
    )?;
    let (ms, counter, device): (i64, i64, String) =
        conn.query_row("SELECT last_ms, last_counter, device_id FROM sync_clock WHERE id = 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    Ok(Stamp { ms: ms as u64, counter: counter as u16, device: device[..DEVICE_CHARS].to_string() })
}

/// Moves the clock up to `stamp` if it is behind, so the next local stamp is newer than every stamp this device has seen.
pub fn observe(conn: &Connection, stamp: &Stamp) -> Result<(), AppError> {
    conn.execute(
        "UPDATE sync_clock SET last_ms = ?1, last_counter = ?2
         WHERE id = 1 AND (?1 > last_ms OR (?1 = last_ms AND ?2 > last_counter))",
        params![stamp.ms as i64, stamp.counter],
    )?;
    Ok(())
}

/// The newest stamp in any shared row or deletion marker, if there is one.
pub fn max_stamp(conn: &Connection) -> Result<Option<Stamp>, AppError> {
    let mut newest: Option<Stamp> = None;
    for table in STAMPED_TABLES.iter().copied().chain(["sync_deleted", "currency_changes"]) {
        let text: Option<String> = conn.query_row(&format!("SELECT max(stamp) FROM {table}"), [], |r| r.get(0))?;
        if let Some(stamp) = text.as_deref().and_then(Stamp::parse) {
            if newest.as_ref().is_none_or(|n| stamp > *n) {
                newest = Some(stamp);
            }
        }
    }
    Ok(newest)
}

/// Run when the database is opened, and after a restore: the clock must not be behind any stamp in the data, or a later
/// edit could get a stamp older than the row it changes and lose to it.
pub fn catch_up(conn: &Connection) -> Result<(), AppError> {
    if let Some(newest) = max_stamp(conn)? {
        observe(conn, &newest)?;
    }
    Ok(())
}

/// After a backup was restored the device is a new one as far as sync goes: it gets a new device id and forgets the devices it
/// was paired with, so it has to pair again and receives a full copy. That keeps the merge rules free of special cases.
pub fn after_restore(conn: &Connection) -> Result<(), AppError> {
    let id = keys::hex_encode(&keys::random_bytes::<16>()?);
    conn.execute("UPDATE sync_clock SET device_id = ?1, applying = 0 WHERE id = 1", [&id])?;
    conn.execute("DELETE FROM sync_peers", [])?;
    catch_up(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stamp_prints_and_parses_back() {
        let s = Stamp::new(1_760_000_000_123, 0x2a, "0123abcd");
        assert_eq!(s.format(), "1760000000123-002a-0123abcd");
        assert_eq!(Stamp::parse(&s.format()), Some(s));
        assert_eq!(STAMP_LEN, "1760000000123-002a-0123abcd".len());
    }

    #[test]
    fn text_that_is_not_a_stamp_is_refused() {
        for bad in [
            "",
            "1760000000123-002a-0123abc",
            "1760000000123-002a-0123abcde",
            "1760000000123-002A-0123abcd",
            "1760000000123-002a-0123ABCD",
            "176000000012x-002a-0123abcd",
            "1760000000123_002a-0123abcd",
            "1760000000123-002a_0123abcd",
            "1760000000123-002g-0123abcd",
            " 760000000123-002a-0123abcd",
            "+760000000123-002a-0123abcd",
            "1760000000123-002a-0123abcd\n",
            "１７６００００００００１２３-002a-0123abcd",
        ] {
            assert!(Stamp::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn comparing_stamps_as_text_is_comparing_them_as_numbers() {
        let mut stamps = vec![
            Stamp::new(9, 0, "00000000"),
            Stamp::new(10, 0, "00000000"),
            Stamp::new(10, 1, "00000000"),
            Stamp::new(10, 1, "0000000a"),
            Stamp::new(10, 0xffff, "00000000"),
            Stamp::new(1_000_000_000_000, 0, "ffffffff"),
            Stamp::new(9_999_999_999_999, 0xffff, "ffffffff"),
        ];
        let by_value = stamps.clone();
        let mut by_text = stamps.clone();
        by_text.sort_by_key(Stamp::format);
        stamps.sort();
        assert_eq!(by_text, stamps);
        assert_eq!(by_value, stamps, "the list above is already in order");
    }
}
