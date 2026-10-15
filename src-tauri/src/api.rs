//! The local command API: how the CLI (and any script with a token) talks to the running app.
//!
//! It is off unless the person turns it on in Settings. When on, the app listens on `127.0.0.1`
//! only, on a port the system picks, and writes that port to `api.json` in its data folder
//! (owner-only), where the CLI looks for it. Nothing is reachable from another machine.
//!
//! Wire format: HTTP/1.1, one request per connection, always `POST /v1/<route>` with a JSON object
//! as the body (nothing means `{}`) and `Authorization: Bearer <token>`. Answers are JSON:
//! `{"ok": true, "data": ...}` or `{"ok": false, "error": {"code": ..., "message": ...}}`, with the
//! status code to match. The routes are in `routes.rs`.
//!
//! What stands between a request and the data, in this order:
//! 1. the `Host` header must name the loopback address and this port, and an `Origin` header is
//!    refused. A web page open in the person's browser can reach `127.0.0.1`, but browsers always
//!    add `Origin` to a cross-site request and `Host` to a rebound name, so neither gets in;
//! 2. only `POST`, only `/v1/...`;
//! 3. a bearer token. A locked app cannot check any token, so it answers `locked` to every request
//!    that carries a token, and gives no data. No token at all is a 401, locked or not;
//! 4. the route must exist and the token's scope must allow it;
//! 5. the body is checked against the route's declared arguments;
//! 6. a route that writes outside the app (an export, a backup) builds a plain description of what it is
//!    about to do and the person has to allow it in the app's window, every time. A token is not enough.
//!    Nothing is written until they do. See `prompt.rs`.
//!
//! Requests never count as user activity, so the idle lock still fires while a script is running.
//! The server logs nothing and keeps nothing: tokens and bodies exist only for the length of a call.

mod confirmed;
mod files;
mod http;
mod prompt;
mod read;
pub mod routes;
mod write;

pub use prompt::{Approval, Prompter, Show};

use std::borrow::Cow;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::backup;
use crate::tokens::{self, Grant};
use crate::vault::{AppError, Vault, VaultStatus};
use routes::{Ctx, Route};

/// Bumped when a route changes in a way an old client cannot cope with.
pub const API_VERSION: u32 = 1;
/// Where the app says which port it listens on, inside the data folder.
pub const DISCOVERY_FILE: &str = "api.json";

/// A failure as the client sees it: an HTTP status, a fixed code for programs and a message for
/// people. The message never holds data from the database or the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: Cow<'static, str>,
}

impl ApiError {
    const fn fixed(status: u16, code: &'static str, message: &'static str) -> Self {
        Self { status, code, message: Cow::Borrowed(message) }
    }

    /// Something wrong with what the client sent. `detail` may quote the client's own request.
    pub fn invalid(detail: impl Into<Cow<'static, str>>) -> Self {
        Self { status: 400, code: "invalid_input", message: detail.into() }
    }

    pub const INVALID_INPUT: Self = Self::fixed(400, "invalid_input", "invalid input");
    pub const UNAUTHORIZED: Self = Self::fixed(401, "unauthorized", "the token is missing, unknown, expired or revoked");
    pub const FORBIDDEN_HOST: Self = Self::fixed(403, "forbidden", "host not allowed");
    pub const FORBIDDEN_ORIGIN: Self = Self::fixed(403, "forbidden", "requests from web pages are not allowed");
    pub const FORBIDDEN_SCOPE: Self =
        Self::fixed(403, "forbidden", "this token is read-only; make a token that can write in the app");
    /// A record that is not there (a transaction that was already deleted).
    pub const NOT_FOUND: Self = Self::fixed(404, "not_found", "not found");
    /// A route this app does not have, say because the client is newer than the app. A different code from
    /// `not_found`, so a script can tell "already gone" from "update the app".
    pub const UNKNOWN_ROUTE: Self = Self::fixed(404, "unknown_route", "this app has no such route; update the app or the client");
    pub const METHOD_NOT_ALLOWED: Self = Self::fixed(405, "method_not_allowed", "use POST");
    /// The person was asked in the app and did not allow it (or could not be asked). All the ways are 403 `denied`.
    pub const DENIED: Self = Self::fixed(403, "denied", "the person did not allow this in the app");
    pub const DENIED_BUSY: Self = Self::fixed(403, "denied", "another request is waiting for an answer in the app");
    pub const DENIED_RECENTLY: Self = Self::fixed(403, "denied", "the person said no to this token a moment ago; try again later");
    pub const DENIED_TIMEOUT: Self = Self::fixed(403, "denied", "nobody answered in the app in time");
    pub const DENIED_NO_PROMPT: Self = Self::fixed(403, "denied", "this needs the person to allow it in the app, and the app cannot ask right now");
    /// A file that is already there is never replaced.
    pub const FILE_EXISTS: Self = Self::fixed(409, "file_exists", "that file already exists; choose a name that does not");
    pub const CANNOT_WRITE: Self = Self::fixed(500, "internal", "cannot write the file; check the folder and its permissions");
    pub const NAME_EXISTS: Self = Self::fixed(409, "name_exists", "that name already exists");
    pub const NOT_SET_UP: Self = Self::fixed(409, "not_set_up", "the app is not set up yet");
    pub const TOO_LARGE: Self = Self::fixed(413, "too_large", "the request is too large");
    pub const LOCKED: Self = Self::fixed(423, "locked", "the app is locked");
    pub const INTERNAL: Self = Self::fixed(500, "internal", "something went wrong");

    fn body(&self) -> Vec<u8> {
        let body = json!({ "ok": false, "error": { "code": self.code, "message": self.message } });
        serde_json::to_vec(&body).unwrap_or_default()
    }
}

impl From<AppError> for ApiError {
    /// No wildcard, so a new `AppError` has to be placed here on purpose.
    fn from(e: AppError) -> Self {
        match e {
            AppError::Locked => Self::LOCKED,
            AppError::NotSetUp => Self::NOT_SET_UP,
            AppError::InvalidInput => Self::INVALID_INPUT,
            AppError::NotFound => Self::NOT_FOUND,
            AppError::NameExists => Self::NAME_EXISTS,
            AppError::AlreadySetUp
            | AppError::WrongCredentials
            | AppError::WrongRecoveryKey
            | AppError::KeyUnavailable
            | AppError::DatabaseTooNew
            | AppError::InvalidBackup
            | AppError::Internal => Self::INTERNAL,
        }
    }
}

/// How much one client may take.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// A whole request must arrive within this.
    pub request_time: Duration,
    /// More connections at once than this are closed without an answer.
    pub connections: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { request_time: Duration::from_secs(10), connections: 16 }
    }
}

/// A running listener. Dropping it stops it and removes the port file.
pub struct ApiServer {
    panics: Arc<AtomicUsize>,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    file: PathBuf,
}

impl ApiServer {
    /// A server that cannot ask the person anything: a route that needs their yes is always refused.
    pub fn start(vault: Arc<Vault>, routes: &'static [Route], limits: Limits) -> io::Result<Self> {
        Self::start_with(vault, routes, limits, None)
    }

    pub fn start_with(vault: Arc<Vault>, routes: &'static [Route], limits: Limits, prompter: Option<Arc<Prompter>>) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let file = vault.data_dir().join(DISCOVERY_FILE);
        let discovery = json!({ "api": API_VERSION, "port": addr.port() }).to_string();
        backup::write_file(&file, discovery.as_bytes()).map_err(|_| io::Error::other("cannot write the port file"))?;

        let stop = Arc::new(AtomicBool::new(false));
        let panics = Arc::new(AtomicUsize::new(0));
        let thread = {
            let (stop, panics) = (stop.clone(), panics.clone());
            thread::Builder::new().name("command-api".into()).spawn(move || accept_loop(listener, vault, routes, limits, prompter, stop, panics))
        };
        match thread {
            Ok(thread) => Ok(Self { panics, addr, stop, thread: Some(thread), file }),
            Err(e) => {
                let _ = std::fs::remove_file(&file);
                Err(e)
            }
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// How many requests made a route panic since this server started. It answers them with a plain 500 and goes
    /// on; this number is for the tests that throw odd input at every route, and should stay 0.
    pub fn panics(&self) -> usize {
        self.panics.load(Ordering::SeqCst)
    }
}

impl Drop for ApiServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // `accept` only returns when something connects.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.file);
    }
}

/// Counts a connection while it is being served, however it ends.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn accept_loop(
    listener: TcpListener,
    vault: Arc<Vault>,
    routes: &'static [Route],
    limits: Limits,
    prompter: Option<Arc<Prompter>>,
    stop: Arc<AtomicBool>,
    panics: Arc<AtomicUsize>,
) {
    let open = Arc::new(AtomicUsize::new(0));
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    for stream in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else {
            // Out of file handles, say. Do not spin.
            thread::sleep(Duration::from_millis(50));
            continue;
        };
        if open.fetch_add(1, Ordering::SeqCst) >= limits.connections {
            open.fetch_sub(1, Ordering::SeqCst);
            continue;
        }
        let slot = Slot(open.clone());
        let (vault, prompter, panics) = (vault.clone(), prompter.clone(), panics.clone());
        let spawned = thread::Builder::new().name("command-api-call".into()).spawn(move || {
            let _slot = slot;
            serve(stream, &vault, routes, port, limits, prompter.as_deref(), &panics);
        });
        // On failure the closure, and the slot in it, is dropped.
        drop(spawned);
    }
}

fn serve(mut stream: TcpStream, vault: &Vault, routes: &'static [Route], port: u16, limits: Limits, prompter: Option<&Prompter>, panics: &AtomicUsize) {
    let _ = stream.set_write_timeout(Some(limits.request_time));
    let request = match http::read_request(&mut stream, Instant::now() + limits.request_time) {
        Ok(request) => Ok(request),
        Err(http::ReadError::Dropped) => return,
        Err(http::ReadError::Refused(e)) => Err(e),
    };
    // A bug in one route must not take the connection, or the server, down with it. It is counted, so the tests that
    // throw odd input at every route can tell a panic from an ordinary refusal.
    let outcome = request.and_then(|request| match catch_unwind(AssertUnwindSafe(|| handle(vault, routes, port, prompter, &request, Utc::now()))) {
        Ok(outcome) => outcome,
        Err(_) => {
            panics.fetch_add(1, Ordering::SeqCst);
            Err(ApiError::INTERNAL)
        }
    });
    match outcome {
        Ok(data) => {
            let body = serde_json::to_vec(&json!({ "ok": true, "data": data })).unwrap_or_default();
            http::respond(&mut stream, 200, &[], &body);
        }
        Err(e) => {
            let extra: &[(&str, &str)] = match e.status {
                401 => &[("WWW-Authenticate", "Bearer")],
                405 => &[("Allow", "POST")],
                _ => &[],
            };
            http::respond(&mut stream, e.status, extra, &e.body());
        }
    }
}

fn host_allowed(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else { return false };
    let host = host.to_ascii_lowercase();
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

fn bearer(header: Option<&str>) -> Option<&str> {
    let (scheme, token) = header?.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() && !token.contains(' ')).then_some(token)
}

fn authenticate(vault: &Vault, token: &str, now: DateTime<Utc>) -> Result<Grant, ApiError> {
    match vault.with_conn(|conn| tokens::verify(conn, token, now)) {
        Ok(Some(grant)) => Ok(grant),
        Ok(None) => Err(ApiError::UNAUTHORIZED),
        // With no open database there is nothing to check a token against.
        Err(AppError::Locked) if vault.status() == VaultStatus::NotSetUp => Err(ApiError::NOT_SET_UP),
        Err(e) => Err(e.into()),
    }
}

fn parse_body(body: &[u8]) -> Result<Map<String, Value>, ApiError> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Map::new());
    }
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(ApiError::invalid("the body must be a JSON object")),
        Err(_) => Err(ApiError::invalid("the body is not valid JSON")),
    }
}

/// One request, start to finish. Everything it can refuse, it refuses here, before a handler runs.
fn handle(vault: &Vault, routes: &'static [Route], port: u16, prompter: Option<&Prompter>, request: &http::Request, now: DateTime<Utc>) -> Result<Value, ApiError> {
    if !host_allowed(request.header("host"), port) {
        return Err(ApiError::FORBIDDEN_HOST);
    }
    if request.header("origin").is_some() {
        return Err(ApiError::FORBIDDEN_ORIGIN);
    }
    if request.method != "POST" {
        return Err(ApiError::METHOD_NOT_ALLOWED);
    }
    let name = request.target.strip_prefix("/v1/").ok_or(ApiError::UNKNOWN_ROUTE)?;
    let token = bearer(request.header("authorization")).ok_or(ApiError::UNAUTHORIZED)?;
    let grant = authenticate(vault, token, now)?;
    // After the token, so a caller without one learns nothing about which routes exist.
    let route = routes.iter().find(|r| r.name == name).ok_or(ApiError::UNKNOWN_ROUTE)?;
    if !grant.scope.allows(route.scope) {
        return Err(ApiError::FORBIDDEN_SCOPE);
    }
    let args = parse_body(&request.body)?;
    route.check(&args)?;
    let ctx = Ctx { vault, grant, routes, now };
    if let Some(describe) = route.confirm {
        // The request is checked and described first, so one that could never work does not ask. Then the person decides.
        let approval = describe(&ctx, &args)?;
        prompter.ok_or(ApiError::DENIED_NO_PROMPT)?.ask(grant.id, approval)?;
    }
    (route.run)(&ctx, args)
}

/// What the Settings screen shows about the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
    /// The person's choice.
    pub enabled: bool,
    /// Whether the listener is up. Can differ from `enabled`: it could not bind, or the app has just started.
    pub running: bool,
    pub port: Option<u16>,
}

/// Owns the listener for the life of the app and follows the person's on/off choice.
pub struct Host {
    vault: Arc<Vault>,
    prompter: Option<Arc<Prompter>>,
    server: Mutex<Option<ApiServer>>,
}

impl Host {
    /// A host that cannot ask the person anything, so routes that need their yes are refused.
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault, prompter: None, server: Mutex::new(None) }
    }

    /// A host that asks the person through `prompter` (the app's window) before an export or a backup.
    pub fn with_prompter(vault: Arc<Vault>, prompter: Arc<Prompter>) -> Self {
        Self { vault, prompter: Some(prompter), server: Mutex::new(None) }
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Option<ApiServer>> {
        self.server.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn launch(&self) -> io::Result<ApiServer> {
        ApiServer::start_with(self.vault.clone(), routes::ALL, Limits::default(), self.prompter.clone())
    }

    /// At app start: listen if the person left it on. If not, remove a port file that a crash left behind.
    pub fn start_if_enabled(&self) {
        let mut slot = self.slot();
        if slot.is_some() {
            return;
        }
        if self.vault.api_enabled().unwrap_or(false) {
            *slot = self.launch().ok();
        } else {
            let _ = std::fs::remove_file(self.vault.data_dir().join(DISCOVERY_FILE));
        }
    }

    /// Saves the choice and starts or stops the listener to match. Needs the unlocked app. If the
    /// listener cannot start, the choice stays off.
    pub fn set_enabled(&self, on: bool) -> Result<(), AppError> {
        let mut slot = self.slot();
        self.vault.set_api_enabled(on)?;
        if !on {
            *slot = None;
        } else if slot.is_none() {
            match self.launch() {
                Ok(server) => *slot = Some(server),
                Err(_) => {
                    let _ = self.vault.set_api_enabled(false);
                    return Err(AppError::Internal);
                }
            }
        }
        Ok(())
    }

    pub fn stop(&self) {
        *self.slot() = None;
    }

    pub fn status(&self) -> HostStatus {
        let slot = self.slot();
        HostStatus {
            enabled: self.vault.api_enabled().unwrap_or(false),
            running: slot.is_some(),
            port: slot.as_ref().map(ApiServer::port),
        }
    }
}

#[cfg(test)]
mod confirmed_tests;
#[cfg(test)]
mod robust_tests;
#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod write_tests;
