//! The app behind the server: the vault, the commands run through Tauri's mock runtime, and who is signed in.
//!
//! One person, one set of data. The data's passphrase is the login: opening the data (or checking the passphrase, when it is open
//! already) gives a browser a session. Any lock, by a person, by the idle timer or by deleting everything, ends every session, so
//! a browser that was left signed in cannot outlast the lock.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use meowny_lib::api::{Approval, Host, Prompter, Show};
use meowny_lib::keys::FileStore;
use meowny_lib::served;
use meowny_lib::sync::service::SyncService;
use meowny_lib::vault::{Policy, Vault, VaultStatus};
use serde_json::Value;
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::files::Staged;

pub const COOKIE: &str = "meowny_session";
/// A wrong passphrase costs this much on top of stretching it, one try at a time.
const WRONG_PASSPHRASE_DELAY: Duration = Duration::from_secs(1);
const MAX_SESSIONS: usize = 32;

/// Commands a browser may call before it is signed in. The rest answer `locked` until it is.
const OPEN: &[&str] = &["vault_status", "app_profile", "unlock_attempts", "unlock_vault", "create_vault", "recover_vault", "restore_backup_new_vault"];

/// Commands that save or read a file through a dialog on the desktop (see `meowny_lib::served`). One runs at a time.
const FILE_COMMANDS: &[&str] = &[
    "save_recovery_key",
    "create_backup",
    "restore_backup",
    "restore_backup_new_vault",
    "set_backup_schedule",
    "export_json",
    "export_csv",
    "export_xlsx",
    "export_pdf",
    "export_debts",
    "save_document_copy",
];

/// What a server does not offer: the phone sync (it finds the phone on the home network), the command line API (it listens on
/// the machine's loopback only), the system's file viewer, desktop notifications and files dropped on a window.
const NOT_SERVED: &[&str] = &[
    "open_document",
    "show_due_notification",
    "document_add_dropped",
    "set_api_enabled",
    "create_api_token",
    "revoke_api_token",
    "sync_start_pairing",
    "sync_enter_code",
    "sync_decide",
    "sync_start",
    "sync_scan",
];

/// The command API is never started here, so nothing is ever asked; if it were, the answer would be no.
struct NoOneToAsk;

impl Show for NoOneToAsk {
    fn show(&self, _: &Approval) -> bool {
        false
    }
}

pub struct Outcome {
    /// The command's answer, or its fixed error code.
    pub result: Result<Value, String>,
    /// A session to give the browser: it has just signed in.
    pub new_session: Option<String>,
    /// A file the command wrote for the browser to fetch: its id and its name.
    pub download: Option<(String, String)>,
}

impl Outcome {
    fn of(result: Result<Value, String>) -> Outcome {
        Outcome { result, new_session: None, download: None }
    }

    fn code(code: &str) -> Outcome {
        Outcome::of(Err(code.to_string()))
    }
}

pub struct App {
    webview: WebviewWindow<MockRuntime>,
    vault: Arc<Vault>,
    /// Session token to when it was last used.
    sessions: Mutex<HashMap<String, Instant>>,
    /// One passphrase is checked at a time.
    login: Mutex<()>,
    /// One file command runs at a time, since the exchange of `served` is one.
    file_command: Mutex<()>,
    pub staged: Staged,
}

fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn is_command_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= 64 && chars.next().is_some_and(|c| c.is_ascii_lowercase()) && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// What the page gets for a failure: a fixed code, never a message (one from Tauri can quote the arguments).
fn error_code(error: Value) -> String {
    match error {
        Value::String(code) if is_command_name(&code) => code,
        _ => "invalid_input".to_string(),
    }
}

impl App {
    /// The mock app must stay alive as long as the server runs, on this thread, so it is handed back on its own.
    pub fn start(data: PathBuf, staged: Staged) -> Result<(tauri::App<MockRuntime>, App), String> {
        let store = FileStore::new(data.join("server.secret"));
        let vault = Arc::new(Vault::with_policy(data, Box::new(store), Policy::SERVER));
        vault.clear_open_copies();
        let mock = mock_builder()
            .manage(vault.clone())
            .manage(Arc::new(SyncService::new(vault.clone())))
            // never started: the commands that show its state need it
            .manage(Arc::new(Host::with_prompter(vault.clone(), Prompter::new(NoOneToAsk))))
            .invoke_handler(meowny_lib::invoke_handler())
            .build(mock_context(noop_assets()))
            .map_err(|e| format!("cannot start the app: {e}"))?;
        let webview = WebviewWindowBuilder::new(&mock, "main", WebviewUrl::default())
            .build()
            .map_err(|e| format!("cannot start the app: {e}"))?;
        let app = App {
            webview,
            vault,
            sessions: Mutex::new(HashMap::new()),
            login: Mutex::new(()),
            file_command: Mutex::new(()),
            staged,
        };
        Ok((mock, app))
    }

    /// One command, as the page of the desktop app would send it.
    fn call(&self, command: &str, args: Value) -> Result<Value, String> {
        let request = InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("a valid url"),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        match get_ipc_response(&self.webview, request) {
            Ok(InvokeResponseBody::Json(text)) => serde_json::from_str(&text).map_err(|_| "internal".to_string()),
            Ok(InvokeResponseBody::Raw(_)) => Err("internal".to_string()),
            Err(error) => Err(error_code(error)),
        }
    }

    /// Ends every session when the data is not open. Returns whether it is.
    fn forget_if_closed(&self) -> bool {
        let open = self.vault.status() == VaultStatus::Unlocked;
        if !open {
            guard(&self.sessions).clear();
        }
        open
    }

    /// Whether this browser is signed in, and the data open. Using a session keeps it.
    pub fn signed_in(&self, session: Option<&str>) -> bool {
        if !self.forget_if_closed() {
            return false;
        }
        let Some(session) = session else { return false };
        match guard(&self.sessions).get_mut(session) {
            Some(used) => {
                *used = Instant::now();
                true
            }
            None => false,
        }
    }

    /// A backup may be sent in by a browser that is signed in, or by anyone before the data is set up (to start from a backup).
    pub fn may_upload(&self, session: Option<&str>) -> bool {
        self.vault.status() == VaultStatus::NotSetUp || self.signed_in(session)
    }

    fn new_session(&self) -> Option<String> {
        let token = served::random_name().ok()?;
        let mut sessions = guard(&self.sessions);
        if sessions.len() >= MAX_SESSIONS {
            if let Some(oldest) = sessions.iter().min_by_key(|(_, used)| **used).map(|(token, _)| token.clone()) {
                sessions.remove(&oldest);
            }
        }
        sessions.insert(token.clone(), Instant::now());
        Some(token)
    }

    pub fn lock_if_idle(&self) {
        if self.vault.lock_if_idle(Instant::now()) {
            guard(&self.sessions).clear();
        }
    }

    pub fn run_scheduled_backup(&self) {
        meowny_lib::schedule::run_if_due(&self.vault, chrono::Utc::now());
    }

    pub fn invoke(&self, command: &str, args: Value, session: Option<&str>, upload: Option<&str>) -> Outcome {
        if !is_command_name(command) || NOT_SERVED.contains(&command) {
            return Outcome::code("invalid_input");
        }
        let signed_in = self.signed_in(session);
        let outcome = match command {
            // Open data is locked as far as a browser without a session can tell.
            "vault_status" if !signed_in && self.vault.status() == VaultStatus::Unlocked => Outcome::of(Ok(Value::from("locked"))),
            "unlock_vault" => self.sign_in(args),
            _ if !signed_in && !OPEN.contains(&command) => Outcome::code("locked"),
            _ => {
                let mut outcome = self.run(command, args, upload);
                // Making the data, recovering it or starting from a backup opens it: that browser is signed in.
                let signs_in = matches!(command, "create_vault" | "recover_vault" | "restore_backup_new_vault");
                if signs_in && outcome.result.as_ref().is_ok_and(|v| !v.is_null() || command == "recover_vault") {
                    outcome.new_session = self.new_session();
                }
                outcome
            }
        };
        // a lock (or deleting everything) ends every session at once
        self.forget_if_closed();
        outcome
    }

    /// A command, with its file exchange when it has one.
    fn run(&self, command: &str, args: Value, upload: Option<&str>) -> Outcome {
        if !FILE_COMMANDS.contains(&command) {
            return Outcome::of(self.call(command, args));
        }
        let _one_at_a_time = guard(&self.file_command);
        let sent = upload.and_then(|id| self.staged.take_upload(id));
        served::begin(sent.clone());
        let result = self.call(command, args);
        let written = served::finish();
        if let Some(path) = sent {
            let _ = std::fs::remove_file(path);
        }
        let download = match (&result, written) {
            (Ok(_), Some(written)) => self.staged.offer(written),
            (_, Some(written)) => {
                let _ = std::fs::remove_file(written.path);
                None
            }
            _ => None,
        };
        Outcome { result, new_session: None, download }
    }

    /// The passphrase opens the data, or is checked against it when it is open already, and the browser gets a session.
    fn sign_in(&self, args: Value) -> Outcome {
        let passphrase = args.get("passphrase").and_then(Value::as_str).unwrap_or("");
        // nothing to check, and so nothing to slow down
        if passphrase.is_empty() {
            return Outcome::code("wrong_credentials");
        }
        let _one_at_a_time = guard(&self.login);
        let result = if self.vault.status() == VaultStatus::Unlocked {
            self.vault.verify_passphrase(passphrase).map(|()| Value::Null).map_err(|e| error_code(serde_json::to_value(e).unwrap_or_default()))
        } else {
            self.call("unlock_vault", args)
        };
        match result {
            Ok(value) => Outcome { result: Ok(value), new_session: self.new_session(), download: None },
            Err(code) => {
                if code == "wrong_credentials" {
                    std::thread::sleep(WRONG_PASSPHRASE_DELAY);
                }
                Outcome::of(Err(code))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_are_plain() {
        assert!(is_command_name("list_transactions"));
        assert!(!is_command_name("plugin:notification|notify"));
        assert!(!is_command_name("List"));
        assert!(!is_command_name(""));
        assert!(!is_command_name(&"a".repeat(65)));
    }

    #[test]
    fn only_a_code_reaches_the_page() {
        assert_eq!(error_code(Value::from("wrong_credentials")), "wrong_credentials");
        assert_eq!(error_code(Value::from("invalid args `id` for command `x`: 12.50 lunch")), "invalid_input");
        assert_eq!(error_code(Value::Null), "invalid_input");
    }
}
