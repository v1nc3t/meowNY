//! The devices this one is paired with, and the secret each pairing made (`docs/sync-design.md`, sections 2 and 5).
//!
//! The secrets live in the encrypted database (`sync_peers`) and nowhere else: not in a backup, an export, the command
//! line or a sync. A listing never carries one.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use zeroize::Zeroizing;

use super::crypto::{Secret, SECRET_LEN};
use super::engine::SyncError;
use super::stamp::Stamp;
use crate::keys;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub device_id: String,
    pub name: String,
    pub paired_at: String,
    pub last_sync_at: Option<String>,
    /// The currency was changed after the last sync with this device, so it still shows the old one until the next sync.
    pub currency_behind: bool,
    /// How many changes were made on this device since the last sync with that one (none if they never synced).
    pub changes_waiting: Option<u32>,
}

/// A device name as the person sees it in the list: 1 to 50 characters, nothing that is not text.
pub fn clean_name(name: &str) -> Result<String, SyncError> {
    let name = name.trim();
    if (1..=50).contains(&name.chars().count()) && !name.chars().any(char::is_control) {
        Ok(name.to_string())
    } else {
        Err(SyncError::Invalid("name"))
    }
}

pub fn list(conn: &Connection) -> Result<Vec<Peer>, SyncError> {
    // The newest currency change, in milliseconds (the start of its stamp), to compare with each device's last sync.
    let newest: Option<String> = conn.query_row("SELECT max(stamp) FROM currency_changes", [], |r| r.get(0))?;
    let changed_ms = newest.and_then(|text| Stamp::parse(&text)).map(|stamp| stamp.ms);
    let mut stmt = conn.prepare("SELECT device_id, name, paired_at, last_sync_at FROM sync_peers ORDER BY paired_at, id")?;
    let rows: Vec<(String, String, String, Option<String>)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
    let mut peers = Vec::with_capacity(rows.len());
    for (device_id, name, paired_at, last_sync_at) in rows {
        let synced_ms = last_sync_ms(conn, &device_id)?;
        let currency_behind = match (changed_ms, synced_ms) {
            (Some(changed), Some(synced)) => changed > synced,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let changes_waiting = match synced_ms {
            Some(synced) => Some(changes_since(conn, synced)?),
            None => None,
        };
        peers.push(Peer { device_id, name, paired_at, last_sync_at, currency_behind, changes_waiting });
    }
    Ok(peers)
}

/// How many rows this device changed after `since_ms` (a moment in milliseconds): what the next sync has to send. Only its own
/// changes count. A row that came from the other device carries that device's mark in its stamp, so the rows a sync brought in
/// are never counted as waiting, whatever the two clocks said.
pub fn changes_since(conn: &Connection, since_ms: u64) -> Result<u32, SyncError> {
    let device: String = conn.query_row("SELECT substr(device_id, 1, 8) FROM sync_clock WHERE id = 1", [], |r| r.get(0))?;
    let from = format!("{:013}", since_ms.saturating_add(1));
    let mut total: i64 = 0;
    for table in ["categories", "people", "ledger_entries", "budgets", "recurring_transactions", "transactions", "settings", "documents", "document_links", "sync_deleted"] {
        // The names are fixed above, never typed by anyone.
        let sql = format!("SELECT count(*) FROM {table} WHERE stamp >= ?1 AND substr(stamp, 20, 8) = ?2");
        total += conn.query_row(&sql, params![from, device], |r| r.get::<_, i64>(0))?;
    }
    Ok(u32::try_from(total).unwrap_or(u32::MAX))
}

/// Pairs with a device. A device that was paired before (the same id) is replaced, with the new secret.
pub fn add(conn: &Connection, device_id: &str, name: &str, secret: &[u8; SECRET_LEN], paired_at: &str) -> Result<(), SyncError> {
    let name = clean_name(name)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM sync_peers WHERE device_id = ?1", [device_id])?;
    tx.execute(
        "INSERT INTO sync_peers (device_id, name, secret, paired_at) VALUES (?1, ?2, ?3, ?4)",
        params![device_id, name, keys::hex_encode(secret), paired_at],
    )?;
    tx.commit()?;
    Ok(())
}

/// Forgets a device: its secret is gone, so it can never sync again. Says whether there was one.
pub fn remove(conn: &Connection, device_id: &str) -> Result<bool, SyncError> {
    Ok(conn.execute("DELETE FROM sync_peers WHERE device_id = ?1", [device_id])? > 0)
}

/// The paired devices with their secrets, for the handshake to try.
pub fn secrets(conn: &Connection) -> Result<Vec<(String, Secret)>, SyncError> {
    let mut stmt = conn.prepare("SELECT device_id, secret FROM sync_peers ORDER BY id")?;
    let rows: Vec<(String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (device_id, hex) in rows {
        let bytes = keys::hex_decode::<SECRET_LEN>(&hex).ok_or(SyncError::Internal)?;
        out.push((device_id, Zeroizing::new(bytes)));
    }
    Ok(out)
}

pub fn touch(conn: &Connection, device_id: &str, when: &str) -> Result<(), SyncError> {
    conn.execute("UPDATE sync_peers SET last_sync_at = ?2 WHERE device_id = ?1", params![device_id, when])?;
    Ok(())
}

/// When the last sync with this device was, in milliseconds since 1970 (none if there was none).
pub fn last_sync_ms(conn: &Connection, device_id: &str) -> Result<Option<u64>, SyncError> {
    let when: Option<Option<String>> = conn.query_row("SELECT last_sync_at FROM sync_peers WHERE device_id = ?1", [device_id], |r| r.get(0)).optional()?;
    Ok(when.flatten().and_then(|text| chrono::DateTime::parse_from_rfc3339(&text).ok()).map(|d| d.timestamp_millis().max(0) as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrate;

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate::run(&mut conn).unwrap();
        conn
    }

    const A: &str = "0123456789abcdef0123456789abcdef";
    const B: &str = "fedcba9876543210fedcba9876543210";

    #[test]
    fn a_pairing_is_stored_listed_without_its_secret_and_forgotten() {
        let conn = db();
        add(&conn, A, "Vincent's phone", &[7u8; 32], "2026-10-15T10:00:00.000Z").unwrap();
        add(&conn, B, "Tablet", &[9u8; 32], "2026-10-16T10:00:00.000Z").unwrap();
        let peers = list(&conn).unwrap();
        assert_eq!(peers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Vincent's phone", "Tablet"]);
        assert!(!serde_json::to_string(&peers).unwrap().contains("0707"), "a listing never carries a secret");
        let secrets = secrets(&conn).unwrap();
        assert_eq!((secrets[0].0.as_str(), *secrets[0].1), (A, [7u8; 32]));
        assert!(remove(&conn, A).unwrap());
        assert!(!remove(&conn, A).unwrap());
        assert_eq!(list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn pairing_the_same_device_again_replaces_its_secret() {
        let conn = db();
        add(&conn, A, "Phone", &[1u8; 32], "2026-10-15T10:00:00.000Z").unwrap();
        add(&conn, A, "Phone again", &[2u8; 32], "2026-10-16T10:00:00.000Z").unwrap();
        let all = secrets(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(*all[0].1, [2u8; 32]);
        assert_eq!(list(&conn).unwrap()[0].name, "Phone again");
    }

    #[test]
    fn names_are_checked() {
        let conn = db();
        for bad in ["", "   ", &"x".repeat(51), "tab\there"] {
            assert!(add(&conn, A, bad, &[1u8; 32], "now").is_err(), "{bad:?}");
        }
        assert_eq!(clean_name("  Phone  ").unwrap(), "Phone");
        assert_eq!(list(&conn).unwrap().len(), 0);
    }

    #[test]
    fn the_time_of_the_last_sync_is_kept_and_read_back_in_milliseconds() {
        let conn = db();
        add(&conn, A, "Phone", &[1u8; 32], "2026-10-15T10:00:00.000Z").unwrap();
        assert_eq!(last_sync_ms(&conn, A).unwrap(), None);
        touch(&conn, A, "2026-10-15T10:30:00.250Z").unwrap();
        assert_eq!(last_sync_ms(&conn, A).unwrap(), Some(1_792_060_200_250));
        assert_eq!(last_sync_ms(&conn, B).unwrap(), None);
    }

    #[test]
    fn a_phone_is_behind_when_the_currency_changed_after_its_last_sync() {
        let conn = db();
        let change = |stamp: &str| {
            conn.execute("INSERT INTO currency_changes (from_currency, to_currency, rate, stamp) VALUES ('EUR', 'USD', '1.1', ?1)", [stamp]).unwrap();
        };
        add(&conn, A, "Phone", &[1u8; 32], "2026-10-15T10:00:00.000Z").unwrap();
        assert!(!list(&conn).unwrap()[0].currency_behind, "nothing has changed yet");
        touch(&conn, A, "2026-10-15T10:30:00.250Z").unwrap(); // 1_792_060_200_250 ms
        change("1792060100000-0000-aaaaaaaa");
        assert!(!list(&conn).unwrap()[0].currency_behind, "the change came before the sync");
        change("1792060300000-0001-aaaaaaaa");
        assert!(list(&conn).unwrap()[0].currency_behind, "the change came after the sync");
        touch(&conn, A, "2026-10-15T10:40:00.000Z").unwrap();
        assert!(!list(&conn).unwrap()[0].currency_behind, "the next sync catches up");
        add(&conn, B, "Never synced", &[2u8; 32], "2026-10-16T10:00:00.000Z").unwrap();
        assert!(list(&conn).unwrap()[1].currency_behind, "a device that never synced has not seen the change");
        assert!(serde_json::to_string(&list(&conn).unwrap()[0]).unwrap().contains(r#""currencyBehind":false"#));
    }

    #[test]
    fn changes_waiting_are_this_devices_own_changes_after_the_last_sync() {
        let conn = db();
        add(&conn, A, "Phone", &[1u8; 32], "2026-10-15T10:00:00.000Z").unwrap();
        assert_eq!(list(&conn).unwrap()[0].changes_waiting, None, "they never synced, so there is nothing to count from");
        // A sync long ago: every row this device has is later than it.
        touch(&conn, A, "2001-01-01T00:00:00.000Z").unwrap();
        let base = list(&conn).unwrap()[0].changes_waiting.unwrap();
        conn.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')", []).unwrap();
        assert_eq!(list(&conn).unwrap()[0].changes_waiting, Some(base + 1), "a new row of this device waits");
        // A row that came from the other device is newer than that sync too, but it is not this device's to send.
        conn.execute("INSERT INTO categories (type, name, sync_id, stamp) VALUES ('INCOME', 'Pay', ?1, '1999999999999-0000-deadbeef')", [&"ab".repeat(16)]).unwrap();
        assert_eq!(list(&conn).unwrap()[0].changes_waiting, Some(base + 1), "what the other device sent is not counted");
        conn.execute("DELETE FROM categories WHERE name = 'Food'", []).unwrap();
        // A sync after the changes: nothing waits.
        touch(&conn, A, "2100-01-01T00:00:00.000Z").unwrap();
        assert_eq!(list(&conn).unwrap()[0].changes_waiting, Some(0));
        assert!(serde_json::to_string(&list(&conn).unwrap()[0]).unwrap().contains(r#""changesWaiting":0"#));
    }

    #[test]
    fn a_secret_in_the_table_that_is_not_a_secret_is_an_error_not_a_panic() {
        let conn = db();
        add(&conn, A, "Phone", &[1u8; 32], "now").unwrap();
        // the table's own checks stop a bad value from being stored at all
        assert!(conn.execute("UPDATE sync_peers SET secret = 'nope'", []).is_err());
    }
}
