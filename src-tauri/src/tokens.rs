//! Access tokens for the command-line client. A token is made in the app and shown once; the
//! database keeps only its SHA-256. Tokens are random (256 bits), so a fast hash is enough: there
//! is nothing to guess and no password to stretch.
//!
//! A token has a scope (`read` unless the person picks `write`, which also reads) and an expiry
//! of 1 to 365 days. Using one records the time, at most once a minute. Revoking deletes the row.
//! Tokens are made, listed and revoked only through the app's own window, never through the
//! command API, so a token cannot make another token.

use std::fmt;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::categories::serialize_id;
use crate::keys;
use crate::vault::AppError;

/// Every token starts like this, so a secret scanner (or a person) can tell what it is.
pub const PREFIX: &str = "mny_";
const SECRET_BYTES: usize = 32;
/// `PREFIX` and the secret as hex.
const TOKEN_LEN: usize = PREFIX.len() + SECRET_BYTES * 2;
const NAME_MAX_CHARS: usize = 50;
pub const DEFAULT_EXPIRY_DAYS: u32 = 30;
pub const MAX_EXPIRY_DAYS: u32 = 365;
/// A token in use is stamped at most this often, so a busy script does not write on every call.
const LAST_USED_EVERY: Duration = Duration::minutes(1);

/// What a token may do. `Write` includes `Read`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Read,
    Write,
}

impl Scope {
    fn as_sql(self) -> &'static str {
        match self {
            Scope::Read => "READ",
            Scope::Write => "WRITE",
        }
    }

    fn from_sql(text: &str) -> Result<Self, AppError> {
        match text {
            "READ" => Ok(Scope::Read),
            "WRITE" => Ok(Scope::Write),
            _ => Err(AppError::Internal),
        }
    }

    /// Whether a token of this scope may do something that needs `needed`.
    pub fn allows(self, needed: Scope) -> bool {
        self >= needed
    }
}

fn default_scope() -> Scope {
    Scope::Read
}

fn default_expiry() -> u32 {
    DEFAULT_EXPIRY_DAYS
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewToken {
    /// What the token is for ("budget agent"), shown in the list. Not unique.
    pub name: String,
    #[serde(default = "default_scope")]
    pub scope: Scope,
    #[serde(default = "default_expiry")]
    pub expires_in_days: u32,
}

/// Everything about a token except the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    pub name: String,
    pub scope: Scope,
    /// UTC, ISO 8601.
    pub created_at: String,
    pub expires_at: String,
    pub last_used_at: Option<String>,
    /// Worked out here, so the UI does not compare against the browser clock.
    pub expired: bool,
}

/// The answer to creating a token: the only time the secret is ever available.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedToken {
    pub token: String,
    pub info: TokenInfo,
}

impl fmt::Debug for CreatedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreatedToken").field("token", &"[hidden]").field("info", &self.info).finish()
    }
}

/// Who a valid token belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    pub id: i64,
    pub scope: Scope,
}

fn stamp(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn hash(token: &str) -> String {
    keys::hex_encode(&Sha256::digest(token.as_bytes()))
}

/// A token has a fixed shape; anything else is refused before it is hashed or looked up.
fn has_token_shape(text: &str) -> bool {
    text.len() == TOKEN_LEN
        && text.strip_prefix(PREFIX).is_some_and(|rest| rest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

fn clean_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    let length = name.chars().count();
    if length == 0 || length > NAME_MAX_CHARS || name.chars().any(char::is_control) {
        return Err(AppError::InvalidInput);
    }
    Ok(name.to_string())
}

/// Makes a token. The returned secret is not stored anywhere and cannot be shown again.
pub fn create(conn: &Connection, input: &NewToken, now: DateTime<Utc>) -> Result<CreatedToken, AppError> {
    let name = clean_name(&input.name)?;
    if !(1..=MAX_EXPIRY_DAYS).contains(&input.expires_in_days) {
        return Err(AppError::InvalidInput);
    }
    let token = format!("{PREFIX}{}", keys::hex_encode(&keys::random_bytes::<SECRET_BYTES>()?));
    let created_at = stamp(now);
    let expires_at = stamp(now + Duration::days(input.expires_in_days.into()));
    conn.execute(
        "INSERT INTO api_tokens (name, scope, token_hash, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, input.scope.as_sql(), hash(&token), created_at, expires_at],
    )?;
    let info = TokenInfo {
        id: conn.last_insert_rowid(),
        name,
        scope: input.scope,
        created_at,
        expires_at,
        last_used_at: None,
        expired: false,
    };
    Ok(CreatedToken { token, info })
}

/// Every token, newest first, including expired ones (they stay listed until revoked).
pub fn list(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<TokenInfo>, AppError> {
    let now = stamp(now);
    let mut statement = conn.prepare(
        "SELECT id, name, scope, created_at, expires_at, last_used_at FROM api_tokens ORDER BY created_at DESC, id DESC",
    )?;
    let rows = statement.query_map([], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get::<_, String>(2)?, r.get(3)?, r.get::<_, String>(4)?, r.get(5)?))
    })?;
    rows.map(|row| {
        let (id, name, scope, created_at, expires_at, last_used_at) = row?;
        let expired = expires_at <= now;
        Ok(TokenInfo { id, name, scope: Scope::from_sql(&scope)?, created_at, expires_at, last_used_at, expired })
    })
    .collect()
}

pub fn revoke(conn: &Connection, id: i64) -> Result<(), AppError> {
    match conn.execute("DELETE FROM api_tokens WHERE id = ?1", [id])? {
        0 => Err(AppError::NotFound),
        _ => Ok(()),
    }
}

/// Looks a token up. None for a token that is unknown, revoked, expired or malformed; the caller
/// cannot tell which, and neither can a caller who is guessing. A good token is stamped as used.
pub fn verify(conn: &Connection, token: &str, now: DateTime<Utc>) -> Result<Option<Grant>, AppError> {
    if !has_token_shape(token) {
        return Ok(None);
    }
    // The lookup compares hashes, which the caller cannot choose, so its timing says nothing about the token.
    let found: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, scope FROM api_tokens WHERE token_hash = ?1 AND expires_at > ?2",
            params![hash(token), stamp(now)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((id, scope)) = found else { return Ok(None) };
    conn.execute(
        "UPDATE api_tokens SET last_used_at = ?1 WHERE id = ?2 AND (last_used_at IS NULL OR last_used_at <= ?3)",
        params![stamp(now), id, stamp(now - LAST_USED_EVERY)],
    )?;
    Ok(Some(Grant { id, scope: Scope::from_sql(&scope)? }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn
    }

    fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 14, hour, minute, second).unwrap()
    }

    fn new(name: &str, scope: Scope, days: u32) -> NewToken {
        NewToken { name: name.into(), scope, expires_in_days: days }
    }

    fn make(conn: &Connection, scope: Scope) -> CreatedToken {
        create(conn, &new("budget agent", scope, 30), at(9, 0, 0)).unwrap()
    }

    fn last_used(conn: &Connection, id: i64) -> Option<String> {
        conn.query_row("SELECT last_used_at FROM api_tokens WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn a_new_token_has_the_expected_shape_and_no_two_are_alike() {
        let conn = db();
        let (a, b) = (make(&conn, Scope::Read), make(&conn, Scope::Read));
        assert!(has_token_shape(&a.token), "{}", a.token.len());
        assert!(a.token.starts_with("mny_"));
        assert_ne!(a.token, b.token);
    }

    #[test]
    fn only_the_hash_is_stored_and_the_secret_is_never_listed() {
        let conn = db();
        let created = make(&conn, Scope::Write);

        let mut stmt = conn.prepare("SELECT name, scope, token_hash, created_at, expires_at FROM api_tokens").unwrap();
        let stored: Vec<String> = stmt
            .query_map([], |r| (0..5).map(|i| r.get::<_, String>(i)).collect::<Result<Vec<_>, _>>())
            .unwrap()
            .flat_map(|row| row.unwrap())
            .collect();
        assert!(stored.iter().all(|cell| !cell.contains(&created.token[PREFIX.len()..])), "the secret is in the table");
        assert!(stored.contains(&hash(&created.token)));
        assert_ne!(hash(&created.token), created.token);

        let listed = serde_json::to_string(&list(&conn, at(9, 0, 1)).unwrap()).unwrap();
        assert!(!listed.contains(&created.token[PREFIX.len()..]));
        assert!(!format!("{created:?}").contains(&created.token[PREFIX.len()..]), "Debug leaks the secret");
        // The answer to creating is the one place the secret appears.
        assert!(serde_json::to_string(&created).unwrap().contains(&created.token));
    }

    #[test]
    fn a_good_token_is_accepted_with_its_scope() {
        let conn = db();
        let read = make(&conn, Scope::Read);
        let write = make(&conn, Scope::Write);
        let now = at(9, 5, 0);
        assert_eq!(verify(&conn, &read.token, now).unwrap(), Some(Grant { id: read.info.id, scope: Scope::Read }));
        assert_eq!(verify(&conn, &write.token, now).unwrap(), Some(Grant { id: write.info.id, scope: Scope::Write }));
    }

    #[test]
    fn unknown_changed_and_malformed_tokens_are_refused() {
        let conn = db();
        let created = make(&conn, Scope::Write);
        let now = at(9, 5, 0);

        let mut changed = created.token.clone();
        let last = if changed.ends_with('0') { '1' } else { '0' };
        changed.replace_range(changed.len() - 1.., &last.to_string());
        assert_eq!(verify(&conn, &changed, now).unwrap(), None);

        let hash_of_the_token = hash(&created.token);
        for bad in [
            "",
            "mny_",
            "wrong",
            &created.token[..created.token.len() - 1],
            &format!("{}0", created.token),
            &created.token.to_uppercase(),
            &created.token.replace("mny_", "xxx_"),
            // What is stored is not a credential.
            &hash_of_the_token,
            &format!("{PREFIX}{hash_of_the_token}"),
            &format!("{PREFIX}{}", "z".repeat(64)),
            &format!("{} ", created.token),
            "' OR 1=1 --",
        ] {
            assert_eq!(verify(&conn, bad, now).unwrap(), None, "{bad:?}");
        }
        assert_eq!(verify(&conn, &created.token, now).unwrap().map(|g| g.id), Some(created.info.id));
    }

    #[test]
    fn a_write_token_may_read_and_a_read_token_may_not_write() {
        assert!(Scope::Read.allows(Scope::Read));
        assert!(!Scope::Read.allows(Scope::Write));
        assert!(Scope::Write.allows(Scope::Read));
        assert!(Scope::Write.allows(Scope::Write));
    }

    #[test]
    fn a_token_is_read_only_for_30_days_unless_the_person_says_otherwise() {
        let input: NewToken = serde_json::from_str(r#"{ "name": "agent" }"#).unwrap();
        assert_eq!(input.scope, Scope::Read);
        assert_eq!(input.expires_in_days, 30);
        let input: NewToken = serde_json::from_str(r#"{ "name": "agent", "scope": "write", "expiresInDays": 7 }"#).unwrap();
        assert_eq!((input.scope, input.expires_in_days), (Scope::Write, 7));
        // An unknown field or scope is an error, not a silent default.
        assert!(serde_json::from_str::<NewToken>(r#"{ "name": "a", "scope": "admin" }"#).is_err());
        assert!(serde_json::from_str::<NewToken>(r#"{ "name": "a", "extra": 1 }"#).is_err());
    }

    #[test]
    fn a_token_stops_working_at_its_expiry() {
        let conn = db();
        let created = create(&conn, &new("short", Scope::Read, 1), at(9, 0, 0)).unwrap();
        assert_eq!(created.info.expires_at, "2026-10-15T09:00:00Z");
        let day = Duration::days(1);
        assert!(verify(&conn, &created.token, at(9, 0, 0) + day - Duration::seconds(1)).unwrap().is_some());
        assert!(verify(&conn, &created.token, at(9, 0, 0) + day).unwrap().is_none(), "valid at the instant it expires");
        assert!(verify(&conn, &created.token, at(9, 0, 0) + day + Duration::days(300)).unwrap().is_none());
        // An expired token stays in the list, marked, until it is revoked.
        let before = list(&conn, at(9, 0, 0) + day - Duration::seconds(1)).unwrap();
        let after = list(&conn, at(9, 0, 0) + day).unwrap();
        assert!(!before[0].expired);
        assert!(after[0].expired);
    }

    #[test]
    fn the_expiry_must_be_between_one_day_and_a_year() {
        let conn = db();
        for days in [0, 366, 10_000, u32::MAX] {
            assert_eq!(create(&conn, &new("x", Scope::Read, days), at(9, 0, 0)).unwrap_err(), AppError::InvalidInput, "{days}");
        }
        assert!(create(&conn, &new("x", Scope::Read, 1), at(9, 0, 0)).is_ok());
        assert!(create(&conn, &new("x", Scope::Read, 365), at(9, 0, 0)).is_ok());
    }

    #[test]
    fn names_are_trimmed_and_limited_like_category_names() {
        let conn = db();
        let created = create(&conn, &new("  nightly script  ", Scope::Read, 30), at(9, 0, 0)).unwrap();
        assert_eq!(created.info.name, "nightly script");
        for bad in ["", "   ", &"x".repeat(51), "two\nlines", "bell\u{7}"] {
            assert_eq!(create(&conn, &new(bad, Scope::Read, 30), at(9, 0, 0)).unwrap_err(), AppError::InvalidInput, "{bad:?}");
        }
        assert!(create(&conn, &new(&"é".repeat(50), Scope::Read, 30), at(9, 0, 0)).is_ok(), "50 characters, not 50 bytes");
        // The same name twice is fine; the list tells them apart by date and use.
        assert!(create(&conn, &new("nightly script", Scope::Read, 30), at(9, 0, 0)).is_ok());
    }

    #[test]
    fn a_revoked_token_stops_working_at_once_and_only_that_one() {
        let conn = db();
        let (keep, drop) = (make(&conn, Scope::Write), make(&conn, Scope::Write));
        let now = at(9, 5, 0);
        assert!(verify(&conn, &drop.token, now).unwrap().is_some());
        revoke(&conn, drop.info.id).unwrap();
        assert_eq!(verify(&conn, &drop.token, now).unwrap(), None);
        assert!(verify(&conn, &keep.token, now).unwrap().is_some());
        assert_eq!(list(&conn, now).unwrap().iter().map(|t| t.id).collect::<Vec<_>>(), vec![keep.info.id]);
        assert_eq!(revoke(&conn, drop.info.id), Err(AppError::NotFound), "revoking twice");
        assert_eq!(revoke(&conn, 9999), Err(AppError::NotFound));
    }

    #[test]
    fn use_is_stamped_at_most_once_a_minute() {
        let conn = db();
        let created = make(&conn, Scope::Read);
        let id = created.info.id;
        assert_eq!(last_used(&conn, id), None, "never used yet");
        assert_eq!(list(&conn, at(9, 0, 0)).unwrap()[0].last_used_at, None);

        verify(&conn, &created.token, at(9, 10, 0)).unwrap();
        assert_eq!(last_used(&conn, id).as_deref(), Some("2026-10-14T09:10:00Z"));
        verify(&conn, &created.token, at(9, 10, 59)).unwrap();
        assert_eq!(last_used(&conn, id).as_deref(), Some("2026-10-14T09:10:00Z"), "stamped again inside a minute");
        verify(&conn, &created.token, at(9, 11, 0)).unwrap();
        assert_eq!(last_used(&conn, id).as_deref(), Some("2026-10-14T09:11:00Z"));
        assert_eq!(list(&conn, at(9, 12, 0)).unwrap()[0].last_used_at.as_deref(), Some("2026-10-14T09:11:00Z"));
    }

    #[test]
    fn a_refused_token_leaves_no_trace_of_use() {
        let conn = db();
        let created = make(&conn, Scope::Read);
        assert_eq!(verify(&conn, &format!("{PREFIX}{}", "0".repeat(64)), at(9, 10, 0)).unwrap(), None);
        assert_eq!(last_used(&conn, created.info.id), None);
    }

    #[test]
    fn the_list_is_newest_first() {
        let conn = db();
        let first = create(&conn, &new("first", Scope::Read, 30), at(9, 0, 0)).unwrap();
        let second = create(&conn, &new("second", Scope::Write, 7), at(10, 0, 0)).unwrap();
        let third = create(&conn, &new("third", Scope::Read, 90), at(10, 0, 0)).unwrap();
        let ids: Vec<i64> = list(&conn, at(11, 0, 0)).unwrap().iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![third.info.id, second.info.id, first.info.id]);
    }

    #[test]
    fn a_token_info_reaches_the_ui_with_a_string_id_and_no_secret() {
        let conn = db();
        let created = make(&conn, Scope::Write);
        let json = serde_json::to_value(&created.info).unwrap();
        assert_eq!(json["id"], created.info.id.to_string());
        assert_eq!(json["scope"], "write");
        assert_eq!(json["expired"], false);
        assert_eq!(json["lastUsedAt"], serde_json::Value::Null);
        let keys: Vec<&String> = json.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 7, "{keys:?}");
    }
}
