//! The app the command line tests talk to: a real encrypted vault in a temporary folder, the app's own
//! server code on 127.0.0.1, a stand-in for the person who allows or refuses a file, and helpers that
//! run the real `meowny` binary against it. Shared by `cli.rs` and `docs.rs`.

#![allow(dead_code)]

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use meowny_lib::api::{routes, Approval, ApiServer, Limits, Prompter, Show};
use meowny_lib::keys::{KeyError, SecretStore, SECRET_LEN};
use meowny_lib::tokens::{self, NewToken, Scope};
use meowny_lib::vault::{AppError, Settings, Vault};
use serde_json::Value;
use zeroize::Zeroizing;

// ---- a real app ------------------------------------------------------------------------------

#[derive(Default)]
pub struct MemoryStore(pub Mutex<Option<[u8; SECRET_LEN]>>);

impl SecretStore for MemoryStore {
    fn get(&self) -> Result<Option<Zeroizing<[u8; SECRET_LEN]>>, KeyError> {
        Ok(self.0.lock().unwrap().map(Zeroizing::new))
    }
    fn set(&self, secret: &[u8; SECRET_LEN]) -> Result<(), KeyError> {
        *self.0.lock().unwrap() = Some(*secret);
        Ok(())
    }
    fn delete(&self) -> Result<(), KeyError> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

/// The person at the app: records what they were asked and answers as set.
pub struct Dialog {
    pub shown: Arc<Mutex<Vec<Approval>>>,
    pub answer: Arc<Mutex<bool>>,
    /// How long they take to decide.
    pub delay: Arc<Mutex<Duration>>,
}

impl Show for Dialog {
    fn show(&self, approval: &Approval) -> bool {
        self.shown.lock().unwrap().push(approval.clone());
        let delay = *self.delay.lock().unwrap();
        std::thread::sleep(delay);
        *self.answer.lock().unwrap()
    }
}

pub struct App {
    pub dir: tempfile::TempDir,
    /// Where exports and backups go.
    pub files: tempfile::TempDir,
    pub vault: Arc<Vault>,
    pub server: Option<ApiServer>,
    pub read: String,
    pub write: String,
    pub shown: Arc<Mutex<Vec<Approval>>>,
    pub answer: Arc<Mutex<bool>>,
    pub delay: Arc<Mutex<Duration>>,
}

impl App {
    pub fn out(&self, name: &str) -> String {
        self.files.path().join(name).to_str().unwrap().to_string()
    }

    pub fn asked(&self) -> usize {
        self.shown.lock().unwrap().len()
    }
}

pub fn token(vault: &Vault, scope: Scope, days: u32) -> String {
    let input = NewToken { name: "test".into(), scope, expires_in_days: days };
    vault.with_conn(|c| tokens::create(c, &input, chrono::Utc::now())).unwrap().token
}

/// A set-up app with the API on. `demo` loads sixteen months of demo data ending on 2026-10-14.
pub fn app(demo: bool) -> App {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(Vault::new(dir.path().to_path_buf(), Box::new(MemoryStore::default())));
    let settings = Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() };
    vault.create(&settings, None).unwrap();
    if demo {
        vault.with_conn(|c| meowny_lib::seed::run_for(c, "2026-10-14").map_err(|_| AppError::Internal)).unwrap();
    }
    let (read, write) = (token(&vault, Scope::Read, 30), token(&vault, Scope::Write, 30));
    let (shown, answer, delay) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(true)), Arc::new(Mutex::new(Duration::ZERO)));
    let prompter = Prompter::with_limits(Dialog { shown: shown.clone(), answer: answer.clone(), delay: delay.clone() }, Duration::from_secs(10), Duration::from_millis(200));
    let server = Some(ApiServer::start_with(vault.clone(), routes::ALL, Limits::default(), Some(prompter)).unwrap());
    App { dir, files: tempfile::tempdir().unwrap(), vault, server, read, write, shown, answer, delay }
}

// ---- running the binary ----------------------------------------------------------------------

pub struct Out {
    pub code: i32,
    pub out: String,
    pub err: String,
}

impl Out {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.out).unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?} / stderr {:?}", self.out, self.err))
    }

    pub fn error(&self) -> Value {
        serde_json::from_str(&self.err).unwrap_or_else(|e| panic!("stderr is not JSON ({e}): {:?}", self.err))
    }

    pub fn all(&self) -> String {
        format!("{}{}", self.out, self.err)
    }
}

/// A command with nothing inherited from the real environment (Windows needs `SystemRoot` to open a socket).
pub fn clean(c: &mut Command) -> &mut Command {
    c.env_clear();
    if let Some(root) = std::env::var_os("SystemRoot") {
        c.env("SystemRoot", root);
    }
    c
}

pub fn command(app: &App) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_meowny"));
    clean(&mut c).env("HOME", app.dir.path()).env("MEOWNY_DATA_DIR", app.dir.path()).stdin(Stdio::null());
    c
}

pub fn finish(mut c: Command) -> Out {
    let output = c.output().unwrap();
    Out { code: output.status.code().unwrap_or(-1), out: String::from_utf8(output.stdout).unwrap(), err: String::from_utf8(output.stderr).unwrap() }
}

pub fn run(app: &App, token: &str, args: &[&str]) -> Out {
    let mut c = command(app);
    c.env("MEOWNY_TOKEN", token).args(args);
    finish(c)
}

pub fn ok(app: &App, token: &str, args: &[&str]) -> Value {
    let mut all = args.to_vec();
    all.push("--json");
    let out = run(app, token, &all);
    assert_eq!(out.code, 0, "{args:?}: {}", out.err);
    assert_eq!(out.err, "", "{args:?} printed on stderr");
    out.json()
}

pub fn failed(app: &App, token: &str, args: &[&str]) -> (i32, String) {
    let mut all = args.to_vec();
    all.push("--json");
    let out = run(app, token, &all);
    assert_eq!(out.out, "", "{args:?} printed data on failure");
    (out.code, out.error()["code"].as_str().unwrap_or("").to_string())
}

/// A document made the way the screens make one, attached to the transaction with this id if one is given. Returns its id.
pub fn document(app: &App, name: &str, bytes: &[u8], transaction: Option<&str>) -> String {
    use meowny_lib::documents::library::{self, LinkTarget, NewDocument, TargetKind};
    app.vault
        .with_files(|conn, files| {
            let links = transaction.map(|id| LinkTarget { kind: TargetKind::Transaction, id: id.into() }).into_iter().collect();
            let input = NewDocument { name: name.into(), links, ..Default::default() };
            Ok(library::add(conn, files, &input, bytes, "2026-10-14")?.document.id.to_string())
        })
        .unwrap()
}

pub fn snapshot(app: &App) -> meowny_lib::backup::Snapshot {
    app.vault.with_conn(|c| meowny_lib::backup::dump(c)).unwrap()
}

pub fn category(app: &App, kind: &str, name: &str) -> String {
    ok(app, &app.read, &["categories", "list", "--type", kind])
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no category {name}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

