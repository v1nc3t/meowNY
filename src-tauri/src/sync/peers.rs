//! The devices this one is paired with, and the secret each pairing made (`docs/sync-design.md`, sections 2 and 5).
//!
//! The secrets live in the encrypted database (`sync_peers`) and nowhere else: not in a backup, an export, the command
//! line or a sync. A listing never carries one.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use zeroize::Zeroizing;

use super::crypto::{Secret, SECRET_LEN};
use super::engine::SyncError;
use crate::keys;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub device_id: String,
    pub name: String,
    pub paired_at: String,
    pub last_sync_at: Option<String>,
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
    let mut stmt = conn.prepare("SELECT device_id, name, paired_at, last_sync_at FROM sync_peers ORDER BY paired_at, id")?;
    let rows = stmt
        .query_map([], |r| Ok(Peer { device_id: r.get(0)?, name: r.get(1)?, paired_at: r.get(2)?, last_sync_at: r.get(3)? }))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
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
    fn a_secret_in_the_table_that_is_not_a_secret_is_an_error_not_a_panic() {
        let conn = db();
        add(&conn, A, "Phone", &[1u8; 32], "now").unwrap();
        // the table's own checks stop a bad value from being stored at all
        assert!(conn.execute("UPDATE sync_peers SET secret = 'nope'", []).is_err());
    }
}
