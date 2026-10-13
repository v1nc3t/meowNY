//! Encrypted SQLite (SQLCipher) access. The rest of the app gets a ready connection
//! and never handles the cipher settings itself.

use std::path::Path;

use rusqlite::Connection;

/// A raw 256-bit database key. Phase 2 decides where it comes from (OS keychain, passphrase).
pub struct DbKey([u8; 32]);

impl DbKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// SQLCipher raw-key syntax: skips the passphrase KDF, so the key must be random.
    fn pragma_value(&self) -> String {
        let hex: String = self.0.iter().map(|b| format!("{b:02x}")).collect();
        format!("\"x'{hex}'\"")
    }
}

#[derive(Debug)]
pub enum DbError {
    /// The linked SQLite has no SQLCipher, so nothing would be encrypted.
    CipherMissing,
    /// Wrong key, or the file is not an encrypted database.
    Unreadable,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::CipherMissing => write!(f, "SQLCipher is not available"),
            DbError::Unreadable => write!(f, "cannot open database: wrong key or not an encrypted database"),
            DbError::Sqlite(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sqlite(e)
    }
}

/// Opens the database at `path`, creating it if missing, and unlocks it with `key`.
pub fn open(path: &Path, key: &DbKey) -> Result<Connection, DbError> {
    let conn = Connection::open(path)?;
    conn.execute_batch(&format!("PRAGMA key = {};", key.pragma_value()))?;

    let cipher: Option<String> = conn
        .query_row("PRAGMA cipher_version", [], |row| row.get(0))
        .ok();
    if cipher.is_none_or(|v| v.is_empty()) {
        return Err(DbError::CipherMissing);
    }

    // The first read fails here if the key is wrong.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0))
        .map_err(|_| DbError::Unreadable)?;

    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> DbKey {
        DbKey::from_bytes([byte; 32])
    }

    fn temp_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meowny.db");
        (dir, path)
    }

    #[test]
    fn data_survives_close_and_reopen_with_same_key() {
        let (_dir, path) = temp_db();
        {
            let conn = open(&path, &key(1)).unwrap();
            conn.execute_batch("CREATE TABLE t (note TEXT); INSERT INTO t VALUES ('hello');")
                .unwrap();
        }
        let conn = open(&path, &key(1)).unwrap();
        let note: String = conn.query_row("SELECT note FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(note, "hello");
    }

    #[test]
    fn wrong_key_cannot_open() {
        let (_dir, path) = temp_db();
        {
            let conn = open(&path, &key(1)).unwrap();
            conn.execute_batch("CREATE TABLE t (note TEXT);").unwrap();
        }
        assert!(matches!(open(&path, &key(2)), Err(DbError::Unreadable)));
    }

    #[test]
    fn file_has_no_plaintext() {
        let (_dir, path) = temp_db();
        {
            let conn = open(&path, &key(1)).unwrap();
            conn.execute_batch("CREATE TABLE t (note TEXT); INSERT INTO t VALUES ('secret-note-123');")
                .unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3"));
        assert!(!bytes.windows(15).any(|w| w == b"secret-note-123"));
        assert!(!bytes.windows(7).any(|w| w == b"CREATE "));
    }

    #[test]
    fn plain_sqlite_open_fails() {
        let (_dir, path) = temp_db();
        {
            let conn = open(&path, &key(1)).unwrap();
            conn.execute_batch("CREATE TABLE t (note TEXT);").unwrap();
        }
        let plain = Connection::open(&path).unwrap();
        assert!(plain
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .is_err());
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let (_dir, path) = temp_db();
        let conn = open(&path, &key(1)).unwrap();
        conn.execute_batch(
            "CREATE TABLE a (id INTEGER PRIMARY KEY);
             CREATE TABLE b (a_id INTEGER REFERENCES a(id));",
        )
        .unwrap();
        assert!(conn.execute("INSERT INTO b VALUES (99)", []).is_err());
    }
}
