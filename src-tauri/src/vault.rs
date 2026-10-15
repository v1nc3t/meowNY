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

use crate::backup::{self, Snapshot};
use crate::db::{self, DbError};
use crate::keys::{self, KeyError, Purpose, RecoveryKey, SecretStore, SALT_LEN};
use crate::migrate::MigrateError;

const DB_FILE: &str = "meowny.db";
const CONFIG_FILE: &str = "vault.json";
const BACKUP_DIR: &str = "backups";
pub(crate) const MIN_PASSPHRASE_CHARS: usize = 8;

/// What the UI is told when something fails. Serialized as the snake_case name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppError {
    Locked,
    NotSetUp,
    AlreadySetUp,
    WrongCredentials,
    WrongRecoveryKey,
    KeyUnavailable,
    DatabaseTooNew,
    InvalidInput,
    /// Not a meowNY backup, or damaged. A wrong passphrase is `WrongCredentials`.
    InvalidBackup,
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
            AppError::WrongRecoveryKey => "that recovery key does not open this data",
            AppError::KeyUnavailable => "the key is not available in the OS keychain",
            AppError::DatabaseTooNew => "this data file was written by a newer version of the app",
            AppError::InvalidInput => "invalid input",
            AppError::InvalidBackup => "that is not a meowNY backup, or it is damaged",
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
            KeyError::Wrong => AppError::WrongCredentials,
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
    /// The database key encrypted under the keychain secret (and passphrase).
    wrapped_everyday: String,
    /// The same key encrypted under the recovery key.
    wrapped_recovery: String,
    lock_after_secs: u32,
    lock_on_minimize: bool,
    lock_on_close: bool,
    /// Desktop notification for recurring transactions that are due. On unless the person turns it off.
    #[serde(default = "default_notify_due")]
    notify_due: bool,
    /// The local command API for the CLI. Off unless the person turns it on. Kept here, not in the
    /// database, so the app knows at start whether to listen while the database is still locked.
    #[serde(default)]
    api_enabled: bool,
}

fn default_notify_due() -> bool {
    true
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
    /// How numbers and dates are written, e.g. "en-IE".
    pub locale: String,
    pub timezone: String,
    /// The language the app is shown in, e.g. "en" or "ro". Separate from `locale`.
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String {
    "en".to_string()
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

    /// First run: creates the keys, the encrypted database and the settings row, and leaves the app unlocked.
    /// Returns the recovery key. It is shown to the person once and is not stored anywhere.
    pub fn create(&self, settings: &Settings, passphrase: Option<&str>) -> Result<String, AppError> {
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

    fn create_inner(&self, settings: &Settings, passphrase: Option<&str>) -> Result<String, AppError> {
        create_private_dir(&self.dir)?;
        let secret = keys::new_secret()?;
        let salt = keys::random_bytes::<SALT_LEN>()?;
        self.store.set(&secret)?;

        let db_key = keys::new_db_key()?;
        let recovery = RecoveryKey::generate()?;
        let config = VaultConfig {
            version: 1,
            passphrase: passphrase.is_some(),
            salt: keys::hex_encode(&salt),
            wrapped_everyday: keys::wrap(&keys::derive_kek(&secret, passphrase, &salt)?, &db_key, Purpose::Everyday)?,
            wrapped_recovery: keys::wrap(&recovery.kek()?, &db_key, Purpose::Recovery)?,
            lock_after_secs: 300,
            lock_on_minimize: true,
            lock_on_close: true,
            notify_due: true,
            api_enabled: false,
        };
        write_config(&self.config_path(), &config)?;

        let conn = db::open_and_migrate(&self.db_path(), &db_key)?;
        conn.execute(
            "INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, ?1, ?2, ?3, ?4)",
            [&settings.currency, &settings.locale, &settings.timezone, &settings.language],
        )?;

        let mut state = self.lock_state();
        state.conn = Some(conn);
        state.last_activity = Instant::now();
        Ok(recovery.to_display())
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

        let key = self.everyday_key(&config, passphrase)?;
        let conn = db::open_and_migrate(&self.db_path(), &key)?;

        let mut state = self.lock_state();
        state.conn = Some(conn);
        state.last_activity = Instant::now();
        Ok(())
    }

    /// Opens the everyday copy of the database key with the keychain secret and the passphrase.
    fn everyday_key(&self, config: &VaultConfig, passphrase: Option<&str>) -> Result<crate::db::DbKey, AppError> {
        let secret = self.store.get()?.ok_or(AppError::KeyUnavailable)?;
        let salt = keys::hex_decode::<SALT_LEN>(&config.salt).ok_or(AppError::Internal)?;
        let kek = keys::derive_kek(&secret, passphrase, &salt)?;
        Ok(keys::unwrap(&kek, &config.wrapped_everyday, Purpose::Everyday)?)
    }

    /// "Forgot my passphrase": opens the data with the recovery key, then sets a new passphrase
    /// (or none) for everyday use. Works even if the keychain entry is gone, and puts a fresh one there.
    /// The recovery key itself stays valid.
    pub fn recover(&self, recovery_key: &str, new_passphrase: Option<&str>) -> Result<(), AppError> {
        if !self.db_path().exists() {
            return Err(AppError::NotSetUp);
        }
        let new_passphrase = normalize_passphrase(new_passphrase)?;
        let recovery = RecoveryKey::parse(recovery_key).ok_or(AppError::WrongRecoveryKey)?;
        let mut config = read_config(&self.config_path())?;
        let key = keys::unwrap(&recovery.kek()?, &config.wrapped_recovery, Purpose::Recovery).map_err(|e| match e {
            KeyError::Wrong => AppError::WrongRecoveryKey,
            other => other.into(),
        })?;

        // Prove the key opens the database before changing anything on disk.
        let conn = db::open_and_migrate(&self.db_path(), &key)?;

        let secret = match self.store.get()? {
            Some(secret) => secret,
            None => {
                let fresh = keys::new_secret()?;
                self.store.set(&fresh)?;
                fresh
            }
        };
        let salt = keys::random_bytes::<SALT_LEN>()?;
        config.passphrase = new_passphrase.is_some();
        config.salt = keys::hex_encode(&salt);
        config.wrapped_everyday =
            keys::wrap(&keys::derive_kek(&secret, new_passphrase, &salt)?, &key, Purpose::Everyday)?;
        write_config(&self.config_path(), &config)?;

        let mut state = self.lock_state();
        state.conn = Some(conn);
        state.last_activity = Instant::now();
        Ok(())
    }

    /// Replaces the recovery key. The old one stops working. Needs the current passphrase (if any)
    /// again, so someone at an unlocked screen cannot quietly make themselves a key.
    pub fn new_recovery_key(&self, passphrase: Option<&str>) -> Result<String, AppError> {
        if self.lock_state().conn.is_none() {
            return Err(AppError::Locked);
        }
        let mut config = read_config(&self.config_path())?;
        let passphrase = if config.passphrase { passphrase.filter(|p| !p.is_empty()) } else { None };
        if config.passphrase && passphrase.is_none() {
            return Err(AppError::WrongCredentials);
        }
        let key = self.everyday_key(&config, passphrase)?;

        let recovery = RecoveryKey::generate()?;
        config.wrapped_recovery = keys::wrap(&recovery.kek()?, &key, Purpose::Recovery)?;
        write_config(&self.config_path(), &config)?;
        Ok(recovery.to_display())
    }

    /// Replaces all data with the contents of a backup. Needs the typed confirmation, because what
    /// is in the app now is gone afterwards. Nothing changes if the backup cannot be opened or fails a check.
    pub fn restore_backup(&self, bytes: &[u8], backup_passphrase: &str, confirmation: &str) -> Result<(), AppError> {
        if confirmation != "RESTORE" {
            return Err(AppError::InvalidInput);
        }
        if self.lock_state().conn.is_none() {
            return Err(AppError::Locked);
        }
        // The slow key stretching happens before the connection is borrowed.
        let snapshot = backup::open(bytes, backup_passphrase)?;
        self.with_conn(|c| backup::restore_snapshot(c, &snapshot))?;
        self.touch();
        Ok(())
    }

    /// First run, from a backup instead of an empty start: makes new keys and a new database, then
    /// loads the backup into it. Returns the recovery key, as `create` does. A backup that fails
    /// leaves the app as it was, not set up.
    pub fn create_from_backup(
        &self,
        bytes: &[u8],
        backup_passphrase: &str,
        passphrase: Option<&str>,
    ) -> Result<String, AppError> {
        if self.db_path().exists() {
            return Err(AppError::AlreadySetUp);
        }
        let snapshot = backup::open(bytes, backup_passphrase)?;
        let settings = settings_of(&snapshot)
            .and_then(|s| validate_settings(&s).ok())
            .ok_or(AppError::InvalidBackup)?;
        let recovery = self.create(&settings, passphrase)?;
        if let Err(e) = self.with_conn(|c| backup::restore_snapshot(c, &snapshot)) {
            self.lock();
            self.remove_everything();
            return Err(e);
        }
        Ok(recovery)
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
            Ok(c.query_row("SELECT currency, locale, timezone, language FROM settings WHERE id = 1", [], |r| {
                Ok(Settings { currency: r.get(0)?, locale: r.get(1)?, timezone: r.get(2)?, language: r.get(3)? })
            })?)
        })
    }

    /// Saves language, locale and timezone. The currency cannot change here: changing it can mean converting
    /// every amount, so it goes through `currency::change`, which makes that choice explicit.
    pub fn save_settings(&self, settings: &Settings) -> Result<(), AppError> {
        let s = validate_settings(settings)?;
        self.with_conn(|c| {
            let current: String = c.query_row("SELECT currency FROM settings WHERE id = 1", [], |r| r.get(0))?;
            if current != s.currency {
                return Err(AppError::InvalidInput);
            }
            c.execute(
                "UPDATE settings SET locale = ?1, timezone = ?2, language = ?3 WHERE id = 1",
                [&s.locale, &s.timezone, &s.language],
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

    pub fn notify_due(&self) -> Result<bool, AppError> {
        Ok(read_config(&self.config_path())?.notify_due)
    }

    pub fn set_notify_due(&self, on: bool) -> Result<(), AppError> {
        if self.lock_state().conn.is_none() {
            return Err(AppError::Locked);
        }
        let mut config = read_config(&self.config_path())?;
        config.notify_due = on;
        write_config(&self.config_path(), &config)
    }

    pub fn api_enabled(&self) -> Result<bool, AppError> {
        Ok(read_config(&self.config_path())?.api_enabled)
    }

    pub fn set_api_enabled(&self, on: bool) -> Result<(), AppError> {
        if self.lock_state().conn.is_none() {
            return Err(AppError::Locked);
        }
        let mut config = read_config(&self.config_path())?;
        config.api_enabled = on;
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
            self.dir.join(crate::api::DISCOVERY_FILE),
            self.dir.join(crate::api::DISCOVERY_FILE).with_extension("json.tmp"),
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

/// The settings row of a backup, which decides the currency and formats of the restored app.
fn settings_of(snapshot: &Snapshot) -> Option<Settings> {
    let row = snapshot.rows("settings").first()?;
    let text = |key: &str| row.get(key).and_then(|v| v.as_str()).map(str::to_string);
    Some(Settings {
        currency: text("currency")?,
        locale: text("locale")?,
        timezone: text("timezone")?,
        language: text("language").unwrap_or_else(default_language),
    })
}

pub fn validate_settings(settings: &Settings) -> Result<Settings, AppError> {
    let currency = settings.currency.trim().to_ascii_uppercase();
    let locale = settings.locale.trim().to_string();
    let timezone = settings.timezone.trim().to_string();
    let language = settings.language.trim().to_string();

    let currency_ok = currency.len() == 3 && currency.bytes().all(|b| b.is_ascii_uppercase());
    let locale_ok = (2..=35).contains(&locale.len())
        && locale.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    let timezone_ok = crate::clock::is_valid_timezone(&timezone);
    // A language tag such as "en", "pt-BR" or "zh-Hant": a 2 or 3 letter language, then optional parts.
    let language_ok = (2..=35).contains(&language.len())
        && language.split('-').enumerate().all(|(i, part)| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_alphanumeric())
                && (i > 0 || ((2..=3).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_alphabetic())))
        });

    if currency_ok && locale_ok && timezone_ok && language_ok {
        Ok(Settings { currency, locale, timezone, language })
    } else {
        Err(AppError::InvalidInput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::memory::MemoryStore;

    fn settings() -> Settings {
        Settings { currency: "eur".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() }
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
            Settings { language: "".into(), ..settings() },
            Settings { language: "e".into(), ..settings() },
            Settings { language: "en_US".into(), ..settings() },
            Settings { language: "en-".into(), ..settings() },
            Settings { language: "english".into(), ..settings() },
            Settings { language: "1n".into(), ..settings() },
        ] {
            assert_eq!(v.create(&bad, None), Err(AppError::InvalidInput));
        }
    }

    #[test]
    fn settings_can_be_changed() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        v.save_settings(&Settings { currency: "eur".into(), locale: "en-US".into(), timezone: "UTC".into(), language: "ro".into() })
            .unwrap();
        let saved = v.settings().unwrap();
        assert_eq!(
            (saved.locale.as_str(), saved.timezone.as_str(), saved.currency.as_str(), saved.language.as_str()),
            ("en-US", "UTC", "EUR", "ro")
        );
    }

    #[test]
    fn language_tags_are_accepted_in_their_usual_shapes() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        for tag in ["en", "ro", "pt-BR", "zh-Hant", "sr-Latn-RS", "fil"] {
            v.save_settings(&Settings { language: tag.into(), ..settings() }).unwrap();
            assert_eq!(v.settings().unwrap().language, tag);
        }
    }

    #[test]
    fn a_missing_language_defaults_to_english_so_older_callers_still_work() {
        let parsed: Settings =
            serde_json::from_str(r#"{"currency":"EUR","locale":"en-IE","timezone":"UTC"}"#).unwrap();
        assert_eq!(parsed.language, "en");
    }

    #[test]
    fn the_currency_cannot_be_changed_through_the_plain_settings_save() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        let usd = Settings { currency: "usd".into(), ..settings() };
        assert_eq!(v.save_settings(&usd), Err(AppError::InvalidInput));
        assert_eq!(v.settings().unwrap().currency, "EUR");
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
    fn due_notifications_are_on_by_default_and_the_choice_persists() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        assert!(v.notify_due().unwrap());
        v.set_notify_due(false).unwrap();
        v.lock();
        assert!(!v.notify_due().unwrap());
        assert_eq!(v.set_notify_due(true), Err(AppError::Locked));
    }

    #[test]
    fn a_config_from_before_the_option_counts_as_on() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        let path = v.config_path();
        let mut json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("notify_due");
        fs::write(&path, json.to_string()).unwrap();
        assert!(v.notify_due().unwrap());
    }

    #[test]
    fn the_command_api_is_off_until_turned_on_and_the_choice_persists() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        assert!(!v.api_enabled().unwrap(), "off by default");
        v.set_api_enabled(true).unwrap();
        v.lock();
        assert!(v.api_enabled().unwrap(), "readable while locked, so the app can start the listener");
        assert_eq!(v.set_api_enabled(false), Err(AppError::Locked), "turning it off, like on, needs the unlocked app");
        assert!(v.api_enabled().unwrap());
        v.unlock(None).unwrap();
        v.set_api_enabled(false).unwrap();
        assert!(!v.api_enabled().unwrap());
    }

    #[test]
    fn a_config_from_before_the_command_api_reads_as_off() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&fs::read_to_string(v.config_path()).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("api_enabled");
        fs::write(v.config_path(), json.to_string()).unwrap();
        assert!(!v.api_enabled().unwrap());
    }

    #[test]
    fn wipe_needs_confirmation_and_removes_data_config_backups_and_key() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        fs::create_dir_all(dir.path().join(BACKUP_DIR)).unwrap();
        fs::write(dir.path().join(BACKUP_DIR).join("b1.meowbak"), b"x").unwrap();
        fs::write(dir.path().join(crate::api::DISCOVERY_FILE), b"{}").unwrap();

        assert_eq!(v.wipe("delete"), Err(AppError::InvalidInput));
        assert_eq!(v.status(), VaultStatus::Unlocked, "a refused wipe changes nothing");

        v.wipe("DELETE").unwrap();
        assert_eq!(v.status(), VaultStatus::NotSetUp);
        assert!(!v.db_path().exists());
        assert!(!v.config_path().exists());
        assert!(!dir.path().join(BACKUP_DIR).exists());
        assert!(!dir.path().join(crate::api::DISCOVERY_FILE).exists(), "the API's port file is removed too");
        assert!(v.store.get().unwrap().is_none());
        // The app can start over.
        v.create(&settings(), None).unwrap();
    }

    #[test]
    fn create_returns_a_recovery_key_that_is_not_stored_in_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        let key = v.create(&settings(), None).unwrap();
        assert_eq!(key.len(), 8 * 4 + 7);
        let on_disk = fs::read_to_string(v.config_path()).unwrap();
        assert!(!on_disk.contains(&key) && !on_disk.contains(&key.replace('-', "")));
    }

    #[test]
    fn recovery_key_unlocks_after_a_forgotten_passphrase_and_sets_a_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        let recovery = v.create(&settings(), Some("old passphrase")).unwrap();
        v.save_settings(&Settings { locale: "en-US".into(), ..settings() }).unwrap();
        v.lock();
        assert_eq!(v.unlock(Some("forgotten")), Err(AppError::WrongCredentials));

        v.recover(&recovery, Some("brand new passphrase")).unwrap();
        assert_eq!(v.status(), VaultStatus::Unlocked);
        assert_eq!(v.settings().unwrap().locale, "en-US", "data is intact");

        v.lock();
        assert_eq!(v.unlock(Some("old passphrase")), Err(AppError::WrongCredentials), "the old passphrase is dead");
        v.unlock(Some("brand new passphrase")).unwrap();
    }

    #[test]
    fn recovery_can_also_remove_the_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        let recovery = v.create(&settings(), Some("old passphrase")).unwrap();
        v.lock();
        v.recover(&recovery, None).unwrap();
        v.lock();
        v.unlock(None).unwrap();
        assert!(!read_config(&v.config_path()).unwrap().passphrase);
    }

    #[test]
    fn recovery_key_is_forgiving_about_case_and_spacing() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        let recovery = v.create(&settings(), None).unwrap();
        v.lock();
        v.recover(&recovery.to_lowercase().replace('-', " "), None).unwrap();
    }

    #[test]
    fn recovery_works_when_the_keychain_entry_is_gone_and_restores_it() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        let recovery = v.create(&settings(), None).unwrap();
        v.lock();
        v.store.delete().unwrap();
        assert_eq!(v.unlock(None), Err(AppError::KeyUnavailable));

        v.recover(&recovery, Some("a new passphrase")).unwrap();
        assert!(v.store.get().unwrap().is_some(), "a fresh keychain secret was stored");
        v.lock();
        v.unlock(Some("a new passphrase")).unwrap();
    }

    #[test]
    fn wrong_or_malformed_recovery_keys_change_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), Some("long enough pass")).unwrap();
        v.lock();
        let before = fs::read_to_string(v.config_path()).unwrap();
        let other = RecoveryKey::generate().unwrap().to_display();
        for bad in [other.as_str(), "nonsense", ""] {
            assert_eq!(v.recover(bad, Some("another passphrase")), Err(AppError::WrongRecoveryKey), "{bad:?}");
        }
        assert_eq!(v.status(), VaultStatus::Locked);
        assert_eq!(fs::read_to_string(v.config_path()).unwrap(), before, "the config is untouched");
        v.unlock(Some("long enough pass")).unwrap();
    }

    #[test]
    fn recovery_rejects_a_too_short_new_passphrase_before_doing_anything() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        let recovery = v.create(&settings(), None).unwrap();
        v.lock();
        assert_eq!(v.recover(&recovery, Some("short")), Err(AppError::InvalidInput));
        assert_eq!(v.status(), VaultStatus::Locked);
    }

    #[test]
    fn a_new_recovery_key_replaces_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        let old = v.create(&settings(), Some("long enough pass")).unwrap();
        let new = v.new_recovery_key(Some("long enough pass")).unwrap();
        assert_ne!(old, new);
        v.lock();
        assert_eq!(v.recover(&old, None), Err(AppError::WrongRecoveryKey), "the old key is dead");
        v.recover(&new, Some("long enough pass")).unwrap();
    }

    #[test]
    fn making_a_new_recovery_key_needs_the_passphrase_and_an_unlocked_app() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        let old = v.create(&settings(), Some("long enough pass")).unwrap();
        assert_eq!(v.new_recovery_key(None), Err(AppError::WrongCredentials));
        assert_eq!(v.new_recovery_key(Some("not the passphrase")), Err(AppError::WrongCredentials));
        v.lock();
        assert_eq!(v.new_recovery_key(Some("long enough pass")), Err(AppError::Locked));
        v.recover(&old, Some("long enough pass")).unwrap();
    }

    #[test]
    fn without_a_passphrase_a_new_recovery_key_needs_none() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        let old = v.create(&settings(), None).unwrap();
        let new = v.new_recovery_key(None).unwrap();
        assert_ne!(old, new);
    }

    #[test]
    fn the_recovery_key_is_never_in_an_error() {
        let key = RecoveryKey::generate().unwrap().to_display();
        for e in [AppError::WrongRecoveryKey, AppError::WrongCredentials, AppError::KeyUnavailable] {
            assert!(!e.to_string().contains(&key));
            assert!(!serde_json::to_string(&e).unwrap().contains(&key));
        }
    }

    /// Some of everything, written through the same functions the app uses.
    fn fill(v: &Vault) {
        use crate::categories::{self, CategoryType};
        use crate::{recurring, transactions};
        v.with_conn(|c| {
            let salary = categories::create(c, "Salary", CategoryType::Income)?;
            let rent = categories::create(c, "Rent", CategoryType::Expense)?;
            let plan = recurring::create(
                c,
                &recurring::NewRecurring {
                    name: "Rent".into(),
                    amount: Some("900.00".into()),
                    category_id: rent.id.to_string(),
                    start_date: "2026-01-01".into(),
                    interval_unit: recurring::Unit::Month,
                    interval_count: 1,
                    end_date: None,
                    description: None,
                },
            )?;
            recurring::pay(c, plan.id, None, Some("2026-01-01"), "2026-01-01")?;
            for (name, amount, category) in [("Pay", "3200.00", salary.id), ("Coffee", "3.50", rent.id)] {
                transactions::create(
                    c,
                    &transactions::NewTransaction {
                        name: name.into(),
                        amount: amount.into(),
                        payment_date: "2026-01-15".into(),
                        category_id: category.to_string(),
                        description: Some("Café \"quoted\"".into()),
                        client_uuid: None,
                    },
                )?;
            }
            categories::rename(c, rent.id, "Housing")?;
            Ok(())
        })
        .unwrap();
    }

    fn snapshot_of(v: &Vault) -> backup::Snapshot {
        v.with_conn(|c| backup::dump(c)).unwrap()
    }

    #[test]
    fn back_up_wipe_and_restore_on_a_clean_install_brings_back_every_row() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), Some("old app passphrase")).unwrap();
        fill(&v);
        let before = snapshot_of(&v);
        let file = backup::seal(&before, "backup passphrase").unwrap();

        v.wipe("DELETE").unwrap();
        assert_eq!(v.status(), VaultStatus::NotSetUp);

        // A clean install: no data file, no keychain entry.
        let recovery = v.create_from_backup(&file, "backup passphrase", Some("new app passphrase")).unwrap();
        assert_eq!(v.status(), VaultStatus::Unlocked);
        assert_eq!(snapshot_of(&v), before, "all rows of all tables, including the change history");
        assert_eq!(v.settings().unwrap(), settings_after_create());

        // The restored install has keys of its own. The backup's passphrase is not the app's.
        v.lock();
        assert_eq!(v.unlock(Some("old app passphrase")), Err(AppError::WrongCredentials));
        assert_eq!(v.unlock(Some("backup passphrase")), Err(AppError::WrongCredentials));
        v.unlock(Some("new app passphrase")).unwrap();
        v.lock();
        v.recover(&recovery, None).unwrap();
        assert_eq!(snapshot_of(&v), before);
    }

    fn settings_after_create() -> Settings {
        Settings { currency: "EUR".into(), ..settings() }
    }

    #[test]
    fn a_restore_that_fails_leaves_a_clean_install() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        fill(&v);
        let good = snapshot_of(&v);
        v.wipe("DELETE").unwrap();

        // A transaction in a category that does not exist: the file is intact but its content is not valid.
        let mut broken = good.clone();
        let mut orphan = broken.rows("transactions")[0].clone();
        orphan.insert("id".into(), 999.into());
        orphan.insert("category_id".into(), 12345.into());
        orphan.insert("client_uuid".into(), serde_json::Value::Null);
        orphan.insert("recurring_transaction_id".into(), serde_json::Value::Null);
        orphan.insert("recurring_due_date".into(), serde_json::Value::Null);
        broken.tables.get_mut("transactions").unwrap().push(orphan);

        let mut no_settings = good.clone();
        no_settings.tables.insert("settings".into(), Vec::new());
        let mut bad_locale = good.clone();
        bad_locale.tables.get_mut("settings").unwrap()[0].insert("locale".into(), "".into());

        for (file, expected) in [
            (backup::seal(&broken, "backup passphrase").unwrap(), AppError::InvalidBackup),
            (backup::seal(&no_settings, "backup passphrase").unwrap(), AppError::InvalidBackup),
            (backup::seal(&bad_locale, "backup passphrase").unwrap(), AppError::InvalidBackup),
            (backup::seal(&good, "backup passphrase").unwrap(), AppError::WrongCredentials),
            (b"not a backup at all, just some text in a file".to_vec(), AppError::InvalidBackup),
        ] {
            let passphrase = if expected == AppError::WrongCredentials { "the wrong one" } else { "backup passphrase" };
            assert_eq!(v.create_from_backup(&file, passphrase, None), Err(expected));
            assert_eq!(v.status(), VaultStatus::NotSetUp);
            assert!(!v.db_path().exists());
            assert!(!v.config_path().exists());
            assert!(v.store.get().unwrap().is_none(), "no keychain entry is left behind");
        }

        // And the install still works afterwards.
        v.create_from_backup(&backup::seal(&good, "backup passphrase").unwrap(), "backup passphrase", None).unwrap();
        assert_eq!(snapshot_of(&v), good);
    }

    #[test]
    fn a_backup_cannot_be_restored_as_a_new_install_over_an_existing_one() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        fill(&v);
        let file = backup::seal(&snapshot_of(&v), "backup passphrase").unwrap();
        let before = snapshot_of(&v);
        assert_eq!(v.create_from_backup(&file, "backup passphrase", None), Err(AppError::AlreadySetUp));
        v.lock();
        assert_eq!(v.create_from_backup(&file, "backup passphrase", None), Err(AppError::AlreadySetUp));
        v.unlock(None).unwrap();
        assert_eq!(snapshot_of(&v), before);
    }

    #[test]
    fn restoring_over_an_open_vault_replaces_the_data_after_the_typed_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), Some("long enough pass")).unwrap();
        fill(&v);
        let saved = snapshot_of(&v);
        let file = backup::seal(&saved, "backup passphrase").unwrap();

        // Changes made after the backup.
        v.with_conn(|c| crate::categories::create(c, "Added later", crate::categories::CategoryType::Expense)).unwrap();
        v.save_settings(&Settings { language: "ro".into(), ..settings_after_create() }).unwrap();
        assert_ne!(snapshot_of(&v), saved);

        for wrong in ["", "restore", "DELETE", "RESTORE "] {
            assert_eq!(v.restore_backup(&file, "backup passphrase", wrong), Err(AppError::InvalidInput), "{wrong:?}");
        }
        assert_eq!(v.restore_backup(&file, "wrong passphrase", "RESTORE"), Err(AppError::WrongCredentials));
        assert_ne!(snapshot_of(&v), saved, "nothing changed yet");

        v.restore_backup(&file, "backup passphrase", "RESTORE").unwrap();
        assert_eq!(snapshot_of(&v), saved);
        assert_eq!(v.settings().unwrap().language, "en");

        // The keys are untouched: the same app passphrase still opens it.
        v.lock();
        v.unlock(Some("long enough pass")).unwrap();
        assert_eq!(snapshot_of(&v), saved);
    }

    #[test]
    fn a_locked_vault_cannot_be_restored_into() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        let file = backup::seal(&snapshot_of(&v), "backup passphrase").unwrap();
        v.lock();
        assert_eq!(v.restore_backup(&file, "backup passphrase", "RESTORE"), Err(AppError::Locked));
    }

    #[test]
    fn a_damaged_backup_leaves_an_open_vault_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        v.create(&settings(), None).unwrap();
        fill(&v);
        let before = snapshot_of(&v);

        let mut broken = before.clone();
        broken.tables.get_mut("budgets").unwrap().push(
            serde_json::json!({ "id": 1, "type": "EXPENSE", "scope": "GLOBAL", "category_id": 77, "effective_from": "2026-01-01" })
                .as_object()
                .unwrap()
                .clone(),
        );
        let file = backup::seal(&broken, "backup passphrase").unwrap();
        assert_eq!(v.restore_backup(&file, "backup passphrase", "RESTORE"), Err(AppError::InvalidBackup));
        assert_eq!(snapshot_of(&v), before);
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
