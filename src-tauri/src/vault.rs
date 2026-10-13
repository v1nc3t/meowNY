//! The vault: owns the encrypted database and its lock state.
//!
//! Locked means no open connection and no key in memory. Unlocking derives the key again
//! from the keychain secret (and the passphrase, if one is set). Everything the UI learns
//! about failures is a short fixed code; error details never leave this module because they
//! can contain schema or value fragments.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::{self, DbError, DbKey};
use crate::keys::{self, KeyError, SecretStore, SALT_LEN};
use crate::migrate::MigrateError;

const DB_FILE: &str = "meowny.db";
const CONFIG_FILE: &str = "vault.json";
const BACKUP_DIR: &str = "backups";
const MIN_PASSPHRASE_CHARS: usize = 8;

/// What the UI is told when something fails. Serialized as the snake_case name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppError {
    Locked,
    NotSetUp,
    AlreadySetUp,
    WrongCredentials,
    KeyUnavailable,
    DatabaseTooNew,
    InvalidInput,
    NotFound,
    NameExists,
    Internal,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            AppError::Locked => "the app is locked",
            AppError::NotSetUp => "the app is not set up yet",
            AppError::AlreadySetUp => "the app is already set up",
            AppError::WrongCredentials => "wrong passphrase or damaged data file",
            AppError::KeyUnavailable => "the key is not available in the OS keychain",
            AppError::DatabaseTooNew => "this data file was written by a newer version of the app",
            AppError::InvalidInput => "invalid input",
            AppError::NotFound => "not found",
            AppError::NameExists => "that name already exists",
            AppError::Internal => "something went wrong",
        };
        write!(f, "{text}")
    }
}

impl std::error::Error for AppError {}

impl From<KeyError> for AppError {
    fn from(e: KeyError) -> Self {
        match e {
            KeyError::Store | KeyError::Corrupt => AppError::KeyUnavailable,
            KeyError::Random => AppError::Internal,
        }
    }
}

impl From<DbError> for AppError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::Unreadable => AppError::WrongCredentials,
            DbError::Migrate(MigrateError::DatabaseTooNew { .. }) => AppError::DatabaseTooNew,
            _ => AppError::Internal,
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(_: rusqlite::Error) -> Self {
        AppError::Internal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultStatus {
    NotSetUp,
    Locked,
    Unlocked,
}

/// Non-secret settings that must be readable while the app is locked.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultConfig {
    version: u32,
    passphrase: bool,
    /// Hex Argon2 salt. Public by design.
    salt: String,
    lock_after_secs: u32,
    lock_on_minimize: bool,
    lock_on_close: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockPrefs {
    /// 0 means never lock on idle.
    pub lock_after_secs: u32,
    pub lock_on_minimize: bool,
    pub lock_on_close: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub currency: String,
    pub locale: String,
    pub timezone: String,
}

struct State {
    conn: Option<Connection>,
    last_activity: Instant,
}

pub struct Vault {
    dir: PathBuf,
    store: Box<dyn SecretStore>,
    state: Mutex<State>,
}

impl Vault {
    pub fn new(dir: PathBuf, store: Box<dyn SecretStore>) -> Self {
        Self { dir, store, state: Mutex::new(State { conn: None, last_activity: Instant::now() }) }
    }

    pub fn data_dir(&self) -> &Path {
        &self.dir
    }

    fn db_path(&self) -> PathBuf {
        self.dir.join(DB_FILE)
    }

    fn config_path(&self) -> PathBuf {
        self.dir.join(CONFIG_FILE)
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, State> {
        // A poisoned lock still guards valid data: the state is only ever swapped whole.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> VaultStatus {
        if self.lock_state().conn.is_some() {
            VaultStatus::Unlocked
        } else if self.db_path().exists() && self.config_path().exists() {
            VaultStatus::Locked
        } else {
            VaultStatus::NotSetUp
        }
    }

    /// First run: creates the key, the encrypted database and the settings row, and leaves the app unlocked.
    pub fn create(&self, settings: &Settings, passphrase: Option<&str>) -> Result<(), AppError> {
        let settings = validate_settings(settings)?;
        let passphrase = normalize_passphrase(passphrase)?;
        if self.db_path().exists() {
            return Err(AppError::AlreadySetUp);
        }

        let result = self.create_inner(&settings, passphrase);
        if result.is_err() {
            self.remove_everything();
        }
        result
    }

    fn create_inner(&self, settings: &Settings, passphrase: Option<&str>) -> Result<(), AppError> {
        create_private_dir(&self.dir)?;
        let secret = keys::new_secret()?;
        let salt = keys::random_bytes::<SALT_LEN>()?;
        self.store.set(&secret)?;

        let config = VaultConfig {
            version: 1,
            passphrase: passphrase.is_some(),
            salt: keys::hex_encode(&salt),
            lock_after_secs: 300,
            lock_on_minimize: true,
            lock_on_close: true,
        };
        write_config(&self.config_path(), &config)?;

        let key = keys::derive_db_key(&secret, passphrase, &salt)?;
        let conn = db::open_and_migrate(&self.db_path(), &key)?;
        conn.execute(
            "INSERT INTO settings (id, currency, locale, timezone) VALUES (1, ?1, ?2, ?3)",
            [&settings.currency, &settings.locale, &settings.timezone],
        )?;

        let mut state = self.lock_state();
        state.conn = Some(conn);
        state.last_activity = Instant::now();
        Ok(())
    }

    pub fn unlock(&self, passphrase: Option<&str>) -> Result<(), AppError> {
        if self.lock_state().conn.is_some() {
            return Ok(());
        }
        if !self.db_path().exists() {
            return Err(AppError::NotSetUp);
        }
        let config = read_config(&self.config_path())?;
        let passphrase = match (config.passphrase, passphrase.filter(|p| !p.is_empty())) {
            (true, None) => return Err(AppError::WrongCredentials),
            (true, Some(p)) => Some(p),
            // A passphrase typed for a vault that has none must not change the key.
            (false, _) => None,
        };

        let secret = self.store.get()?.ok_or(AppError::KeyUnavailable)?;
        let salt = keys::hex_decode::<SALT_LEN>(&config.salt).ok_or(AppError::Internal)?;
        let key: DbKey = keys::derive_db_key(&secret, passphrase, &salt)?;
        let conn = db::open_and_migrate(&self.db_path(), &key)?;

        let mut state = self.lock_state();
        state.conn = Some(conn);
        state.last_activity = Instant::now();
        Ok(())
    }

    /// Closes the database. The connection owned the only copy of the key.
    pub fn lock(&self) {
        self.lock_state().conn = None;
    }

    /// Records user activity for the idle timer.
    pub fn touch(&self) {
        self.lock_state().last_activity = Instant::now();
    }

    /// Locks when the idle limit has passed. Returns true if it locked just now.
    pub fn lock_if_idle(&self, now: Instant) -> bool {
        let limit = match self.lock_prefs() {
            Ok(prefs) if prefs.lock_after_secs > 0 => Duration::from_secs(prefs.lock_after_secs.into()),
            _ => return false,
        };
        let mut state = self.lock_state();
        if state.conn.is_some() && now.saturating_duration_since(state.last_activity) >= limit {
            state.conn = None;
            return true;
        }
        false
    }

    /// Runs `f` on the open database, or fails with `Locked`. The only way data code gets a connection.
    pub fn with_conn<R>(&self, f: impl FnOnce(&mut Connection) -> Result<R, AppError>) -> Result<R, AppError> {
        let mut state = self.lock_state();
        let conn = state.conn.as_mut().ok_or(AppError::Locked)?;
        f(conn)
    }

    pub fn settings(&self) -> Result<Settings, AppError> {
        self.with_conn(|c| {
            Ok(c.query_row("SELECT currency, locale, timezone FROM settings WHERE id = 1", [], |r| {
                Ok(Settings { currency: r.get(0)?, locale: r.get(1)?, timezone: r.get(2)? })
            })?)
        })
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), AppError> {
        let s = validate_settings(settings)?;
        self.with_conn(|c| {
            c.execute(
                "UPDATE settings SET currency = ?1, locale = ?2, timezone = ?3 WHERE id = 1",
                [&s.currency, &s.locale, &s.timezone],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    /// Readable while locked, because the window handlers need it then.
    pub fn lock_prefs(&self) -> Result<LockPrefs, AppError> {
        let c = read_config(&self.config_path())?;
        Ok(LockPrefs {
            lock_after_secs: c.lock_after_secs,
            lock_on_minimize: c.lock_on_minimize,
            lock_on_close: c.lock_on_close,
        })
    }

    pub fn save_lock_prefs(&self, prefs: &LockPrefs) -> Result<(), AppError> {
        if self.lock_state().conn.is_none() {
            return Err(AppError::Locked);
        }
        if prefs.lock_after_secs > 24 * 3600 {
            return Err(AppError::InvalidInput);
        }
        let mut config = read_config(&self.config_path())?;
        config.lock_after_secs = prefs.lock_after_secs;
        config.lock_on_minimize = prefs.lock_on_minimize;
        config.lock_on_close = prefs.lock_on_close;
        write_config(&self.config_path(), &config)
    }

    /// "Delete all my data": database, key, config and backups the app created.
    /// Removing the keychain secret also makes any leftover copy of the file unreadable.
    /// Every step is tried even if an earlier one fails; the result says whether all succeeded.
    pub fn wipe(&self, confirmation: &str) -> Result<(), AppError> {
        if confirmation != "DELETE" {
            return Err(AppError::InvalidInput);
        }
        self.lock();
        if self.remove_everything() {
            Ok(())
        } else {
            Err(AppError::Internal)
        }
    }

    fn remove_everything(&self) -> bool {
        let db = self.db_path();
        let mut ok = true;
        for path in [
            db.clone(),
            db.with_extension("db-wal"),
            db.with_extension("db-shm"),
            db.with_extension("db-journal"),
            self.config_path(),
            self.config_path().with_extension("json.tmp"),
        ] {
            ok &= remove_file_if_exists(&path);
        }
        let backups = self.dir.join(BACKUP_DIR);
        if backups.exists() {
            ok &= fs::remove_dir_all(&backups).is_ok();
        }
        ok &= self.store.delete().is_ok();
        ok
    }
}

fn remove_file_if_exists(path: &Path) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

fn create_private_dir(dir: &Path) -> Result<(), AppError> {
    fs::create_dir_all(dir).map_err(|_| AppError::Internal)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|_| AppError::Internal)?;
    }
    Ok(())
}

fn read_config(path: &Path) -> Result<VaultConfig, AppError> {
    let text = fs::read_to_string(path).map_err(|_| AppError::NotSetUp)?;
    let config: VaultConfig = serde_json::from_str(&text).map_err(|_| AppError::Internal)?;
    if config.version != 1 {
        return Err(AppError::DatabaseTooNew);
    }
    Ok(config)
}

/// Writes through a temporary file so a crash never leaves half a config.
fn write_config(path: &Path, config: &VaultConfig) -> Result<(), AppError> {
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(config).map_err(|_| AppError::Internal)?;
    fs::write(&tmp, text).map_err(|_| AppError::Internal)?;
    fs::rename(&tmp, path).map_err(|_| AppError::Internal)
}

fn normalize_passphrase(passphrase: Option<&str>) -> Result<Option<&str>, AppError> {
    match passphrase.filter(|p| !p.is_empty()) {
        None => Ok(None),
        Some(p) if p.chars().count() >= MIN_PASSPHRASE_CHARS => Ok(Some(p)),
        Some(_) => Err(AppError::InvalidInput),
    }
}

pub fn validate_settings(settings: &Settings) -> Result<Settings, AppError> {
    let currency = settings.currency.trim().to_ascii_uppercase();
    let locale = settings.locale.trim().to_string();
    let timezone = settings.timezone.trim().to_string();

    let currency_ok = currency.len() == 3 && currency.bytes().all(|b| b.is_ascii_uppercase());
    let locale_ok = (2..=35).contains(&locale.len())
        && locale.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    let timezone_ok = crate::clock::is_valid_timezone(&timezone);

    if currency_ok && locale_ok && timezone_ok {
        Ok(Settings { currency, locale, timezone })
    } else {
        Err(AppError::InvalidInput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::memory::MemoryStore;

    fn settings() -> Settings {
        Settings { currency: "eur".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into() }
    }

    fn vault(dir: &Path) -> Vault {
        Vault::new(dir.to_path_buf(), Box::new(MemoryStore::default()))
    }

    #[test]
    fn first_run_creates_an_unlocked_encrypted_vault() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        assert_eq!(v.status(), VaultStatus::NotSetUp);
        v.create(&settings(), None).unwrap();
        assert_eq!(v.status(), VaultStatus::Unlocked);
        assert_eq!(v.settings().unwrap().currency, "EUR");
        assert_eq!(v.create(&settings(), None), Err(AppError::AlreadySetUp));
    }

    #[test]
    fn locked_state_holds_no_connection_and_refuses_data_access() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        v.lock();
        assert_eq!(v.status(), VaultStatus::Locked);
        assert!(v.lock_state().conn.is_none());
        assert_eq!(v.settings(), Err(AppError::Locked));
        assert_eq!(v.save_settings(&settings()), Err(AppError::Locked));
    }

    #[test]
    fn unlock_restores_access_and_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        v.lock();
        v.unlock(None).unwrap();
        assert_eq!(v.settings().unwrap().locale, "en-IE");
    }

    #[test]
    fn passphrase_vault_needs_the_right_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), Some("long enough pass")).unwrap();
        v.lock();
        assert_eq!(v.unlock(None), Err(AppError::WrongCredentials));
        assert_eq!(v.unlock(Some("not the passphrase")), Err(AppError::WrongCredentials));
        assert_eq!(v.status(), VaultStatus::Locked);
        v.unlock(Some("long enough pass")).unwrap();
        assert_eq!(v.status(), VaultStatus::Unlocked);
    }

    #[test]
    fn passphrase_alone_is_not_enough_without_the_keychain_secret() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), Some("long enough pass")).unwrap();
        v.lock();
        v.store.delete().unwrap();
        assert_eq!(v.unlock(Some("long enough pass")), Err(AppError::KeyUnavailable));
    }

    #[test]
    fn short_passphrase_is_rejected_and_nothing_is_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        assert_eq!(v.create(&settings(), Some("short")), Err(AppError::InvalidInput));
        assert_eq!(v.status(), VaultStatus::NotSetUp);
        assert!(!v.db_path().exists());
    }

    #[test]
    fn invalid_settings_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        for bad in [
            Settings { currency: "EU".into(), ..settings() },
            Settings { currency: "E1R".into(), ..settings() },
            Settings { locale: "".into(), ..settings() },
            Settings { timezone: "Europe/Dublin; DROP".into(), ..settings() },
        ] {
            assert_eq!(v.create(&bad, None), Err(AppError::InvalidInput));
        }
    }

    #[test]
    fn settings_can_be_changed() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        v.save_settings(&Settings { currency: "usd".into(), locale: "en-US".into(), timezone: "UTC".into() })
            .unwrap();
        assert_eq!(v.settings().unwrap().currency, "USD");
    }

    #[test]
    fn data_file_is_not_readable_without_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        v.lock();
        let bytes = fs::read(v.db_path()).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3"));
        let plain = Connection::open(v.db_path()).unwrap();
        assert!(plain.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).is_err());
    }

    #[test]
    fn idle_timeout_locks_and_activity_postpones_it() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        v.save_lock_prefs(&LockPrefs { lock_after_secs: 60, lock_on_minimize: true, lock_on_close: true }).unwrap();
        let start = Instant::now();
        assert!(!v.lock_if_idle(start + Duration::from_secs(30)));
        assert_eq!(v.status(), VaultStatus::Unlocked);
        assert!(v.lock_if_idle(start + Duration::from_secs(61)));
        assert_eq!(v.status(), VaultStatus::Locked);
        assert!(!v.lock_if_idle(start + Duration::from_secs(120)), "already locked");
    }

    #[test]
    fn idle_timeout_of_zero_never_locks() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        v.save_lock_prefs(&LockPrefs { lock_after_secs: 0, lock_on_minimize: true, lock_on_close: true }).unwrap();
        assert!(!v.lock_if_idle(Instant::now() + Duration::from_secs(86_400)));
        assert_eq!(v.status(), VaultStatus::Unlocked);
    }

    #[test]
    fn lock_prefs_persist_across_lock_and_unlock() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        let prefs = LockPrefs { lock_after_secs: 900, lock_on_minimize: false, lock_on_close: true };
        v.save_lock_prefs(&prefs).unwrap();
        v.lock();
        assert_eq!(v.lock_prefs().unwrap(), prefs);
        assert_eq!(v.save_lock_prefs(&prefs), Err(AppError::Locked));
    }

    #[test]
    fn wipe_needs_confirmation_and_removes_data_config_backups_and_key() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        fs::create_dir_all(dir.path().join(BACKUP_DIR)).unwrap();
        fs::write(dir.path().join(BACKUP_DIR).join("b1.meowbak"), b"x").unwrap();

        assert_eq!(v.wipe("delete"), Err(AppError::InvalidInput));
        assert_eq!(v.status(), VaultStatus::Unlocked, "a refused wipe changes nothing");

        v.wipe("DELETE").unwrap();
        assert_eq!(v.status(), VaultStatus::NotSetUp);
        assert!(!v.db_path().exists());
        assert!(!v.config_path().exists());
        assert!(!dir.path().join(BACKUP_DIR).exists());
        assert!(v.store.get().unwrap().is_none());
        // The app can start over.
        v.create(&settings(), None).unwrap();
    }

    #[test]
    fn errors_serialize_as_fixed_codes_without_details() {
        assert_eq!(serde_json::to_string(&AppError::WrongCredentials).unwrap(), "\"wrong_credentials\"");
        let from_sqlite: AppError = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(19),
            Some("UNIQUE constraint failed: categories.name = 'Secret salary'".into()),
        )
        .into();
        assert_eq!(serde_json::to_string(&from_sqlite).unwrap(), "\"internal\"");
        assert!(!from_sqlite.to_string().contains("Secret"));
    }
}
