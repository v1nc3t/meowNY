//! The server against real sockets, a real (temporary) encrypted vault and real tokens.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::routes::{self, Ctx, Kind, Param, Route};
use super::*;
use crate::keys::memory::MemoryStore;
use crate::tokens::{NewToken, Scope};
use crate::vault::{LockPrefs, Settings};

// ---- routes used only here -------------------------------------------------------------------

fn echo(ctx: &Ctx, args: Map<String, Value>) -> Result<Value, ApiError> {
    Ok(json!({ "args": args, "scope": ctx.grant.scope }))
}

fn name_exists(_: &Ctx, _: Map<String, Value>) -> Result<Value, ApiError> {
    Err(AppError::NameExists.into())
}

fn internal(_: &Ctx, _: Map<String, Value>) -> Result<Value, ApiError> {
    Err(AppError::WrongRecoveryKey.into())
}

fn boom(_: &Ctx, _: Map<String, Value>) -> Result<Value, ApiError> {
    panic!("boom: lunch 42.50 mny_secret")
}

static TEST_ROUTES: &[Route] = &[
    routes::STATUS,
    routes::SCHEMA,
    Route {
        name: "test.echo",
        scope: Scope::Read,
        summary: "Gives the arguments back.",
        params: &[
            Param { name: "text", kind: Kind::String, required: true, nullable: false, summary: "A text.", values: &[] },
            Param { name: "count", kind: Kind::Integer, required: false, nullable: false, summary: "A number.", values: &[] },
            Param { name: "flag", kind: Kind::Boolean, required: false, nullable: false, summary: "A switch.", values: &[] },
            Param { name: "kind", kind: Kind::String, required: false, nullable: false, summary: "A or B.", values: &["A", "B"] },
        ],
        confirm: None,
        run: echo,
    },
    Route { name: "test.write", scope: Scope::Write, summary: "Needs a write token.", params: &[], confirm: None, run: echo },
    Route { name: "test.exists", scope: Scope::Read, summary: "Fails: name exists.", params: &[], confirm: None, run: name_exists },
    Route { name: "test.internal", scope: Scope::Read, summary: "Fails: internal.", params: &[], confirm: None, run: internal },
    Route { name: "test.boom", scope: Scope::Read, summary: "Panics.", params: &[], confirm: None, run: boom },
];

// ---- a vault, tokens and a server -------------------------------------------------------------

pub(super) fn settings() -> Settings {
    Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() }
}

pub(super) fn new_vault(dir: &std::path::Path) -> Arc<Vault> {
    let vault = Arc::new(Vault::new(dir.to_path_buf(), Box::new(MemoryStore::default())));
    vault.create(&settings(), None).unwrap();
    vault
}

pub(super) fn make_token(vault: &Vault, scope: Scope) -> String {
    let input = NewToken { name: "test".into(), scope, expires_in_days: 30 };
    vault.with_conn(|c| tokens::create(c, &input, Utc::now())).unwrap().token
}

struct Fixture {
    _dir: tempfile::TempDir,
    vault: Arc<Vault>,
    server: ApiServer,
    read: String,
    write: String,
}

impl Fixture {
    fn port(&self) -> u16 {
        self.server.port()
    }

    fn call(&self, token: &str, route: &str, body: &str) -> Reply {
        call(self.port(), Some(token), route, body)
    }
}

fn fixture() -> Fixture {
    fixture_with(Limits { request_time: Duration::from_secs(5), connections: 16 })
}

fn fixture_with(limits: Limits) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let vault = new_vault(dir.path());
    let (read, write) = (make_token(&vault, Scope::Read), make_token(&vault, Scope::Write));
    let server = ApiServer::start(vault.clone(), TEST_ROUTES, limits).unwrap();
    Fixture { _dir: dir, vault, server, read, write }
}

// ---- a small HTTP client ---------------------------------------------------------------------

pub(super) struct Reply {
    pub(super) status: u16,
    pub(super) head: String,
    pub(super) body: String,
}

impl Reply {
    pub(super) fn header(&self, name: &str) -> Option<String> {
        self.head
            .split("\r\n")
            .skip(1)
            .filter_map(|line| line.split_once(": "))
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.to_string())
    }

    pub(super) fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", self.body))
    }

    pub(super) fn code(&self) -> String {
        self.json()["error"]["code"].as_str().unwrap_or("").to_string()
    }

    pub(super) fn message(&self) -> String {
        self.json()["error"]["message"].as_str().unwrap_or("").to_string()
    }

    pub(super) fn data(&self) -> Value {
        assert_eq!(self.status, 200, "{}", self.body);
        assert_eq!(self.json()["ok"], true);
        self.json()["data"].clone()
    }
}

pub(super) fn connect(port: u16) -> TcpStream {
    let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    stream
}

/// Reads until the server closes. A server that closes without answering gives status 0.
pub(super) fn read_reply(stream: &mut TcpStream) -> Reply {
    let mut bytes = Vec::new();
    let _ = stream.read_to_end(&mut bytes);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let Some((head, body)) = text.split_once("\r\n\r\n") else {
        return Reply { status: 0, head: text, body: String::new() };
    };
    let status = head.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    Reply { status, head: head.to_string(), body: body.to_string() }
}

pub(super) fn send(port: u16, bytes: &[u8]) -> Reply {
    let mut stream = connect(port);
    let _ = stream.write_all(bytes);
    read_reply(&mut stream)
}

pub(super) fn raw_request(method: &str, target: &str, headers: &[String], body: &str) -> Vec<u8> {
    let mut out = format!("{method} {target} HTTP/1.1\r\n");
    for header in headers {
        out.push_str(header);
        out.push_str("\r\n");
    }
    out.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    out.into_bytes()
}

pub(super) fn headers(port: u16, token: Option<&str>) -> Vec<String> {
    let mut out = vec![format!("Host: 127.0.0.1:{port}")];
    if let Some(token) = token {
        out.push(format!("Authorization: Bearer {token}"));
    }
    out
}

pub(super) fn call(port: u16, token: Option<&str>, route: &str, body: &str) -> Reply {
    send(port, &raw_request("POST", &format!("/v1/{route}"), &headers(port, token), body))
}

fn works(f: &Fixture) {
    assert_eq!(f.call(&f.read, "status", "").status, 200, "the server stopped answering");
}

// ---- tests -----------------------------------------------------------------------------------

#[test]
fn status_tells_the_caller_what_it_is_talking_to_and_what_its_token_may_do() {
    let f = fixture();
    let read = f.call(&f.read, "status", "").data();
    assert_eq!(read["app"], "meowny");
    assert_eq!(read["api"], API_VERSION);
    assert_eq!(read["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(read["schemaVersion"], crate::migrate::latest_version());
    assert_eq!(read["scope"], "read");
    assert_eq!(f.call(&f.write, "status", "").data()["scope"], "write");
}

#[test]
fn the_schema_lists_every_route() {
    let f = fixture();
    let data = f.call(&f.read, "schema", "").data();
    assert_eq!(data["api"], API_VERSION);
    let routes = data["routes"].as_array().unwrap();
    assert_eq!(routes.len(), TEST_ROUTES.len());
    for (listed, route) in routes.iter().zip(TEST_ROUTES) {
        assert_eq!(*listed, route.describe());
    }
    let echo = routes.iter().find(|r| r["name"] == "test.echo").unwrap();
    assert_eq!(echo["params"][3]["values"], json!(["A", "B"]));
}

#[test]
fn every_answer_is_json_uncached_and_has_no_cors_headers() {
    let f = fixture();
    let port = f.port();
    let replies = [
        f.call(&f.read, "status", ""),
        call(port, None, "status", ""),
        f.call(&f.read, "nope", ""),
        f.call(&f.read, "status", "[]"),
        send(port, &raw_request("GET", "/v1/status", &headers(port, None), "")),
    ];
    for reply in &replies {
        assert_eq!(reply.header("content-type").as_deref(), Some("application/json; charset=utf-8"), "{}", reply.head);
        assert_eq!(reply.header("cache-control").as_deref(), Some("no-store"));
        assert_eq!(reply.header("x-content-type-options").as_deref(), Some("nosniff"));
        assert_eq!(reply.header("connection").as_deref(), Some("close"));
        assert_eq!(reply.header("content-length"), Some(reply.body.len().to_string()));
        assert!(!reply.head.to_lowercase().contains("access-control"), "{}", reply.head);
        assert!(reply.json()["ok"].is_boolean());
    }
}

#[test]
fn a_request_without_a_valid_token_is_refused() {
    let f = fixture();
    let port = f.port();
    let hash: String = f.vault.with_conn(|c| Ok(c.query_row("SELECT token_hash FROM api_tokens LIMIT 1", [], |r| r.get(0))?)).unwrap();
    let h = |auth: &str| {
        let mut h = headers(port, None);
        h.push(auth.to_string());
        send(port, &raw_request("POST", "/v1/status", &h, ""))
    };
    for reply in [
        call(port, None, "status", ""),
        h("Authorization: Basic dXNlcjpwYXNz"),
        h("Authorization: Bearer"),
        h("Authorization: Bearer "),
        h(&format!("Authorization: Bearer {} extra", f.read)),
        h(&format!("Authorization: {}", f.read)),
        h(&format!("Authorization: Token {}", f.read)),
        h(&format!("Authorization: Bearer {}", "mny_".to_string() + &"a".repeat(64))),
        h(&format!("Authorization: Bearer {}x", f.read)),
        h(&format!("Authorization: Bearer {hash}")),
        h(&format!("Authorization: Bearer {}", f.read.to_uppercase())),
        h(&format!("X-Authorization: Bearer {}", f.read)),
        call(port, Some("' OR '1'='1"), "status", ""),
    ] {
        assert_eq!((reply.status, reply.code().as_str()), (401, "unauthorized"), "{}", reply.body);
        assert_eq!(reply.header("www-authenticate").as_deref(), Some("Bearer"));
    }
    // The scheme is case-insensitive, as HTTP says.
    let lower = h(&format!("Authorization: bearer {}", f.read));
    assert_eq!(lower.status, 200);
    works(&f);
}

#[test]
fn the_token_is_not_accepted_anywhere_but_the_authorization_header() {
    let f = fixture();
    let port = f.port();
    let mut h = headers(port, None);
    h.push(format!("Cookie: token={}", f.read));
    assert_eq!(send(port, &raw_request("POST", &format!("/v1/status?token={}", f.read), &h, "")).status, 401);
    assert_eq!(send(port, &raw_request("POST", "/v1/status", &h, &format!("{{\"token\":\"{}\"}}", f.read))).status, 401);
}

#[test]
fn a_refused_token_is_never_echoed() {
    let f = fixture();
    let marker = format!("mny_{}", "feedbeef".repeat(8));
    let reply = call(f.port(), Some(&marker), "status", &format!("{{\"{marker}\":1}}"));
    assert_eq!(reply.status, 401);
    assert!(!reply.head.contains(&marker) && !reply.body.contains(&marker));
}

#[test]
fn an_expired_or_revoked_token_stops_working_on_the_next_request() {
    let f = fixture();
    assert_eq!(f.call(&f.write, "status", "").status, 200);
    f.vault
        .with_conn(|c| {
            c.execute("UPDATE api_tokens SET expires_at = '2000-01-01T00:00:00Z' WHERE scope = 'WRITE'", [])?;
            Ok(())
        })
        .unwrap();
    assert_eq!(f.call(&f.write, "status", "").code(), "unauthorized", "expired");

    assert_eq!(f.call(&f.read, "status", "").status, 200);
    let id = f.vault.with_conn(|c| Ok(tokens::list(c, Utc::now())?.into_iter().find(|t| t.scope == Scope::Read).unwrap().id)).unwrap();
    f.vault.with_conn(|c| tokens::revoke(c, id)).unwrap();
    assert_eq!(f.call(&f.read, "status", "").code(), "unauthorized", "revoked");
}

#[test]
fn a_request_stamps_the_token_it_used_and_no_other() {
    let f = fixture();
    f.call(&f.read, "status", "").data();
    let listed = f.vault.with_conn(|c| tokens::list(c, Utc::now())).unwrap();
    let used = |scope| listed.iter().find(|t| t.scope == scope).unwrap().last_used_at.clone();
    assert!(used(Scope::Read).is_some());
    assert!(used(Scope::Write).is_none());
}

#[test]
fn a_locked_app_answers_locked_and_gives_no_data() {
    let f = fixture();
    f.vault.lock();
    for reply in [
        f.call(&f.read, "status", ""),
        f.call(&f.write, "schema", ""),
        // A token cannot be checked without the database, so a wrong one gets the same answer.
        call(f.port(), Some(&format!("mny_{}", "0".repeat(64))), "status", ""),
        f.call(&f.read, "nope", ""),
    ] {
        assert_eq!((reply.status, reply.code().as_str()), (423, "locked"), "{}", reply.body);
        assert!(reply.json().get("data").is_none());
    }
    // With no token at all it is still a plain 401.
    assert_eq!(call(f.port(), None, "status", "").status, 401);

    f.vault.unlock(None).unwrap();
    assert_eq!(f.call(&f.read, "status", "").status, 200, "the same token works again after unlocking");
}

#[test]
fn an_app_that_is_not_set_up_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(Vault::new(dir.path().to_path_buf(), Box::new(MemoryStore::default())));
    let server = ApiServer::start(vault, TEST_ROUTES, Limits::default()).unwrap();
    let reply = call(server.port(), Some(&format!("mny_{}", "0".repeat(64))), "status", "");
    assert_eq!((reply.status, reply.code().as_str()), (409, "not_set_up"));
}

#[test]
fn wiping_the_data_closes_the_door_and_removes_the_port_file() {
    let f = fixture();
    let file = f.vault.data_dir().join(DISCOVERY_FILE);
    assert!(file.exists());
    f.vault.wipe("DELETE").unwrap();
    assert!(!file.exists());
    assert_eq!(f.call(&f.read, "status", "").code(), "not_set_up");
}

#[test]
fn only_the_loopback_host_and_this_port_are_accepted() {
    let f = fixture();
    let port = f.port();
    let with_host = |host: Option<&str>| {
        let mut h = vec![format!("Authorization: Bearer {}", f.read)];
        if let Some(host) = host {
            h.push(format!("Host: {host}"));
        }
        send(port, &raw_request("POST", "/v1/status", &h, ""))
    };
    for ok in [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("LOCALHOST:{port}"),
        format!("Localhost:{port}"),
        // White space around a header value is not part of it.
        format!("127.0.0.1:{port} "),
    ] {
        assert_eq!(with_host(Some(&ok)).status, 200, "{ok}");
    }
    for bad in [
        "127.0.0.1".to_string(),
        "localhost".to_string(),
        format!("127.0.0.1:{}", port.wrapping_add(1)),
        format!("localhost:{}", port.wrapping_add(1)),
        "127.0.0.1:80".to_string(),
        format!("evil.test:{port}"),
        format!("127.0.0.1.evil.test:{port}"),
        format!("localhost.evil.test:{port}"),
        format!("evil.test:{port}@127.0.0.1:{port}"),
        format!("0.0.0.0:{port}"),
        format!("[::1]:{port}"),
        format!("127.0.0.1:{port}x"),
        String::new(),
    ] {
        let reply = with_host(Some(&bad));
        assert_eq!((reply.status, reply.code().as_str()), (403, "forbidden"), "{bad:?}: {}", reply.body);
    }
    assert_eq!(with_host(None).status, 403, "no Host header at all");
}

#[test]
fn a_request_from_a_web_page_is_refused_even_with_a_valid_token() {
    let f = fixture();
    let port = f.port();
    for origin in ["https://evil.test", "null", "http://localhost", &format!("http://127.0.0.1:{port}"), ""] {
        let mut h = headers(port, Some(&f.write));
        h.push(format!("Origin: {origin}"));
        let reply = send(port, &raw_request("POST", "/v1/test.write", &h, ""));
        assert_eq!((reply.status, reply.code().as_str()), (403, "forbidden"), "{origin:?}");
        // A pre-flight check gets no permission either.
        h.push("Access-Control-Request-Method: POST".to_string());
        h.push("Access-Control-Request-Headers: authorization".to_string());
        let preflight = send(port, &raw_request("OPTIONS", "/v1/test.write", &h, ""));
        assert_eq!(preflight.status, 403);
        assert!(!preflight.head.to_lowercase().contains("access-control"));
    }
    works(&f);
}

#[test]
fn only_post_is_accepted() {
    let f = fixture();
    let port = f.port();
    for method in ["GET", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "TRACE", "CONNECT", "PROPFIND"] {
        let reply = send(port, &raw_request(method, "/v1/status", &headers(port, Some(&f.read)), ""));
        assert_eq!(reply.status, 405, "{method}");
        assert_eq!(reply.header("allow").as_deref(), Some("POST"));
        assert_eq!(reply.code(), "method_not_allowed");
    }
}

#[test]
fn unknown_routes_and_odd_paths_are_unknown_routes_and_a_caller_without_a_token_learns_nothing() {
    let f = fixture();
    let port = f.port();
    for route in ["nope", "test", "test.", ".echo", "status/", "STATUS", "Status", "../status", "status%20", "test.echo.x", "v1/status"] {
        assert_eq!(f.call(&f.read, route, "").code(), "unknown_route", "{route:?}");
        assert_eq!(call(port, None, route, "").code(), "unauthorized", "{route:?} without a token");
    }
    for target in ["/", "/v1", "/v1/", "/v2/status", "/V1/status", "/status", "/v1/status?x=1", "//v1/status", "/v1//status"] {
        let reply = send(port, &raw_request("POST", target, &headers(port, Some(&f.read)), ""));
        assert_eq!((reply.status, reply.code().as_str()), (404, "unknown_route"), "{target}");
    }
    works(&f);
}

#[test]
fn a_route_needs_the_scope_it_declares() {
    let f = fixture();
    let refused = f.call(&f.read, "test.write", "");
    assert_eq!((refused.status, refused.code().as_str()), (403, "forbidden"));
    assert!(refused.message().contains("read-only"));
    let allowed = f.call(&f.write, "test.write", "").data();
    assert_eq!(allowed["scope"], "write");
    // A write token also reads.
    assert_eq!(f.call(&f.write, "test.echo", r#"{"text":"a"}"#).status, 200);
    assert_eq!(f.call(&f.read, "test.echo", r#"{"text":"a"}"#).status, 200);
}

#[test]
fn arguments_are_checked_before_the_route_runs() {
    let f = fixture();
    let ok = f.call(&f.read, "test.echo", r#"{"text":"hi","count":3,"flag":true,"kind":"A"}"#).data();
    assert_eq!(ok["args"], json!({ "text": "hi", "count": 3, "flag": true, "kind": "A" }));
    for (body, why) in [
        ("{}", "missing argument `text`"),
        (r#"{"text":"a","other":1}"#, "unknown argument `other`"),
        (r#"{"text":1}"#, "`text` must be a string"),
        (r#"{"text":"a","count":"3"}"#, "`count` must be an integer"),
        (r#"{"text":"a","count":2.5}"#, "`count` must be an integer"),
        (r#"{"text":"a","flag":1}"#, "`flag` must be a boolean"),
        (r#"{"text":"a","kind":"C"}"#, "`kind` must be one of: A, B"),
        (r#"{"text":null}"#, "`text` must be a string"),
    ] {
        let reply = f.call(&f.read, "test.echo", body);
        assert_eq!((reply.status, reply.code().as_str()), (400, "invalid_input"), "{body}");
        assert_eq!(reply.message(), why, "{body}");
    }
    // A route with no arguments takes none.
    assert_eq!(f.call(&f.read, "status", r#"{"a":1}"#).message(), "unknown argument `a`");
}

#[test]
fn the_body_must_be_a_json_object_or_nothing() {
    let f = fixture();
    for ok in ["", "  \r\n ", "{}", " { } ", "{\n}\n"] {
        assert_eq!(f.call(&f.read, "status", ok).status, 200, "{ok:?}");
    }
    let deep = "[".repeat(5000);
    for bad in ["[]", "null", "\"x\"", "5", "true", "{", "{} x", "{}{}", "\u{feff}{}", "{'a':1}", &deep, "{\"a\":}"] {
        let reply = f.call(&f.read, "status", bad);
        assert_eq!((reply.status, reply.code().as_str()), (400, "invalid_input"), "{:?}", &bad[..bad.len().min(20)]);
    }
    works(&f);
}

#[test]
fn a_bad_body_is_not_quoted_back() {
    let f = fixture();
    let reply = f.call(&f.read, "status", r#"{"lunch 42.50 mny_secret": "#);
    assert_eq!(reply.message(), "the body is not valid JSON");
    assert_eq!(f.call(&f.read, "status", r#"["lunch 42.50"]"#).message(), "the body must be a JSON object");
}

#[test]
fn errors_from_the_app_keep_their_code_and_hide_everything_else() {
    let f = fixture();
    let exists = f.call(&f.read, "test.exists", "");
    assert_eq!((exists.status, exists.code().as_str()), (409, "name_exists"));
    // An error the client cannot act on is the same plain 500, whatever it was inside.
    let internal = f.call(&f.read, "test.internal", "");
    assert_eq!((internal.status, internal.code().as_str(), internal.message().as_str()), (500, "internal", "something went wrong"));
    assert!(!internal.body.to_lowercase().contains("recovery"));
}

#[test]
fn a_route_that_panics_gives_a_500_and_the_server_carries_on() {
    let f = fixture();
    assert_eq!(f.server.panics(), 0);
    let reply = f.call(&f.read, "test.boom", "");
    assert_eq!((reply.status, reply.code().as_str()), (500, "internal"));
    assert_eq!(f.server.panics(), 1, "the panic was counted, so the tests that look for panics can see one");
    assert!(!reply.body.contains("lunch") && !reply.body.contains("mny_secret"), "the panic text leaked: {}", reply.body);
    for _ in 0..3 {
        works(&f);
    }
}

#[test]
fn every_app_error_maps_to_a_status_and_none_leaks_detail() {
    use AppError::*;
    let table = [
        (Locked, 423, "locked"),
        (NotSetUp, 409, "not_set_up"),
        (AlreadySetUp, 500, "internal"),
        (WrongCredentials, 500, "internal"),
        (WrongRecoveryKey, 500, "internal"),
        (KeyUnavailable, 500, "internal"),
        (DatabaseTooNew, 500, "internal"),
        (InvalidInput, 400, "invalid_input"),
        (InvalidBackup, 500, "internal"),
        (NotFound, 404, "not_found"),
        (NameExists, 409, "name_exists"),
        (Internal, 500, "internal"),
    ];
    for (error, status, code) in table {
        let mapped = ApiError::from(error);
        assert_eq!((mapped.status, mapped.code), (status, code), "{error:?}");
        assert!(!mapped.message.is_empty());
        let body: Value = serde_json::from_slice(&mapped.body()).unwrap();
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"]["code"], code);
    }
}

#[test]
fn the_body_size_is_limited_before_it_is_read() {
    let f = fixture();
    let port = f.port();
    let head = |length: &str| format!("POST /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\nContent-Length: {length}\r\n\r\n", f.read);
    // The answer comes without the body ever being sent.
    for length in [http::MAX_BODY + 1, 2_000_000, 999_999_999] {
        let reply = send(port, head(&length.to_string()).as_bytes());
        assert_eq!((reply.status, reply.code().as_str()), (413, "too_large"), "{length}");
    }
    // Exactly the limit is fine.
    let body = " ".repeat(http::MAX_BODY);
    let mut stream = connect(port);
    stream.write_all(head(&body.len().to_string()).as_bytes()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    assert_eq!(read_reply(&mut stream).status, 200);
    works(&f);
}

#[test]
fn a_huge_header_block_is_refused() {
    let f = fixture();
    let port = f.port();
    let mut h = headers(port, Some(&f.read));
    h.push(format!("X-Filler: {}", "a".repeat(http::MAX_HEAD + 100)));
    assert_eq!(send(port, &raw_request("POST", "/v1/status", &h, "")).status, 413);
    // Many small headers add up the same way.
    let mut h = headers(port, Some(&f.read));
    h.extend((0..2000).map(|i| format!("X-{i}: aaaaaaaaaa")));
    assert_eq!(send(port, &raw_request("POST", "/v1/status", &h, "")).status, 413);
    works(&f);
}

#[test]
fn content_length_must_be_plain_digits_and_appear_once() {
    let f = fixture();
    let port = f.port();
    let base = format!("POST /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\n", f.read);
    for bad in ["-1", "+2", "1 2", "0x2", "abc", "", "2.0", "99999999999999999999"] {
        let reply = send(port, format!("{base}Content-Length: {bad}\r\n\r\n{{}}").as_bytes());
        assert_eq!(reply.status, 400, "{bad:?}");
    }
    let twice = send(port, format!("{base}Content-Length: 2\r\nContent-Length: 2\r\n\r\n{{}}").as_bytes());
    assert_eq!(twice.status, 400);
    let chunked = send(port, format!("{base}Transfer-Encoding: chunked\r\n\r\n2\r\n{{}}\r\n0\r\n\r\n").as_bytes());
    assert_eq!((chunked.status, chunked.code().as_str()), (400, "invalid_input"));
    // The smuggling shape: both headers at once.
    let both = send(port, format!("{base}Content-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n{{}}").as_bytes());
    assert_eq!(both.status, 400);
    works(&f);
}

#[test]
fn bytes_after_the_declared_body_are_ignored_and_a_short_body_is_dropped() {
    let f = fixture();
    let port = f.port();
    let base = format!("POST /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\n", f.read);
    let longer = send(port, format!("{base}Content-Length: 2\r\n\r\n{{}}GET /v1/status HTTP/1.1\r\n\r\n").as_bytes());
    assert_eq!(longer.status, 200);

    // The client promises 100 bytes, sends 5 and closes: nobody to answer.
    let mut stream = connect(port);
    stream.write_all(format!("{base}Content-Length: 100\r\n\r\n{{\"a\":").as_bytes()).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    assert_eq!(read_reply(&mut stream).status, 0);
    works(&f);
}

#[test]
fn a_client_that_waits_for_100_continue_gets_it() {
    let f = fixture();
    let port = f.port();
    let mut stream = connect(port);
    let head = format!(
        "POST /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\nExpect: 100-continue\r\nContent-Length: 2\r\n\r\n",
        f.read
    );
    stream.write_all(head.as_bytes()).unwrap();
    let mut interim = [0u8; 25];
    stream.read_exact(&mut interim).unwrap();
    assert_eq!(&interim, b"HTTP/1.1 100 Continue\r\n\r\n");
    stream.write_all(b"{}").unwrap();
    assert_eq!(read_reply(&mut stream).status, 200);
}

#[test]
fn malformed_requests_get_a_400_and_do_no_harm() {
    let f = fixture();
    let port = f.port();
    let h = format!("Host: 127.0.0.1:{port}\r\nAuthorization: Bearer {}", f.read);
    let cases: Vec<Vec<u8>> = vec![
        b"\x00\x01\x02\r\n\r\n".to_vec(),
        b"\xff\xfe\xfd\r\n\r\n".to_vec(),
        b"hello\r\n\r\n".to_vec(),
        b"POST\r\n\r\n".to_vec(),
        b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n".to_vec(),
        format!("post /v1/status HTTP/1.1\r\n{h}\r\n\r\n").into_bytes(),
        format!("POST /v1/status HTTP/2.0\r\n{h}\r\n\r\n").into_bytes(),
        format!("POST  /v1/status HTTP/1.1\r\n{h}\r\n\r\n").into_bytes(),
        format!("POST http://127.0.0.1:{port}/v1/status HTTP/1.1\r\n{h}\r\n\r\n").into_bytes(),
        format!("POST /v1/status HTTP/1.1\r\n{h}\r\nno colon\r\n\r\n").into_bytes(),
        format!("POST /v1/status HTTP/1.1\r\n{h}\r\n folded: x\r\n\r\n").into_bytes(),
        format!("POST /v1/status HTTP/1.1\r\n{h}\r\nBad Name: x\r\n\r\n").into_bytes(),
        format!("POST /v1/status HTTP/1.1\r\n{h}\r\nX: a\0b\r\n\r\n").into_bytes(),
        format!("POST /v1/status HTTP/1.1\r\n{h}\r\nHost: again\r\n\r\n").into_bytes(),
        format!("POST /v1/status HTTP/1.1\r\n{h}\r\nAuthorization: Bearer {}\r\n\r\n", f.write).into_bytes(),
    ];
    for (i, bytes) in cases.iter().enumerate() {
        let reply = send(port, bytes);
        assert_eq!((reply.status, reply.code().as_str()), (400, "invalid_input"), "case {i}: {:?}", String::from_utf8_lossy(bytes));
    }
    works(&f);
}

#[test]
fn silent_slow_and_trickling_clients_are_dropped_at_the_deadline() {
    let f = fixture_with(Limits { request_time: Duration::from_millis(400), connections: 16 });
    let port = f.port();
    let started = Instant::now();

    let mut silent = connect(port);
    assert_eq!(read_reply(&mut silent).status, 0);

    let mut partial = connect(port);
    partial.write_all(b"POST /v1/sta").unwrap();
    assert_eq!(read_reply(&mut partial).status, 0);

    // One byte at a time, each inside any per-read timeout, never finishing the head.
    let mut trickle = connect(port);
    let mut sent = 0;
    while sent < 40 && trickle.write_all(b"a").is_ok() {
        sent += 1;
        thread::sleep(Duration::from_millis(100));
    }
    assert!(sent < 40, "the server kept reading a head that never ends");
    assert_eq!(read_reply(&mut trickle).status, 0);

    // A body that never arrives.
    let mut stalled = connect(port);
    stalled.write_all(format!("POST /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 10\r\n\r\n{{").as_bytes()).unwrap();
    assert_eq!(read_reply(&mut stalled).status, 0);

    assert!(started.elapsed() < Duration::from_secs(8), "{:?}", started.elapsed());
    works(&f);
}

#[test]
fn too_many_connections_at_once_are_closed_and_the_slots_come_back() {
    let f = fixture_with(Limits { request_time: Duration::from_millis(1500), connections: 2 });
    let port = f.port();
    let (_a, _b) = (connect(port), connect(port));
    thread::sleep(Duration::from_millis(100));

    let mut third = connect(port);
    let _ = third.write_all(&raw_request("POST", "/v1/status", &headers(port, Some(&f.read)), ""));
    assert_eq!(read_reply(&mut third).status, 0, "served although two connections were already open");

    // The two idle ones time out and free their slots.
    thread::sleep(Duration::from_millis(1800));
    works(&f);
}

#[test]
fn many_clients_at_once_all_get_answers() {
    let f = fixture();
    let port = f.port();
    let token = f.read.clone();
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let token = token.clone();
            thread::spawn(move || (0..10).map(|_| call(port, Some(&token), "status", "").status).collect::<Vec<_>>())
        })
        .collect();
    for worker in workers {
        assert!(worker.join().unwrap().iter().all(|s| *s == 200));
    }
}

#[test]
fn using_the_api_does_not_keep_the_app_awake() {
    let f = fixture();
    let prefs = LockPrefs { lock_after_secs: 60, lock_on_minimize: false, lock_on_close: false };
    f.vault.save_lock_prefs(&prefs).unwrap();
    let base = Instant::now();
    thread::sleep(Duration::from_millis(50));
    for _ in 0..3 {
        works(&f);
    }
    // A minute after `base` the app is idle, whatever the script did since.
    assert!(f.vault.lock_if_idle(base + Duration::from_secs(60)), "API calls counted as activity");
    assert_eq!(f.call(&f.read, "status", "").code(), "locked");
}

#[test]
fn the_listener_is_loopback_only_and_the_port_file_is_private_and_exact() {
    let f = fixture();
    assert!(f.server.addr().ip().is_loopback());
    assert_eq!(f.server.addr().ip(), std::net::IpAddr::V4(Ipv4Addr::LOCALHOST));
    let file = f.vault.data_dir().join(DISCOVERY_FILE);
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), json!({ "api": API_VERSION, "port": f.port() }));
    assert!(!text.contains(&f.read) && !text.contains(&f.write));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn stopping_closes_the_port_and_removes_the_file_even_with_a_client_connected() {
    let dir = tempfile::tempdir().unwrap();
    let vault = new_vault(dir.path());
    let server = ApiServer::start(vault.clone(), TEST_ROUTES, Limits::default()).unwrap();
    let port = server.port();
    let _idle = connect(port);
    let file = dir.path().join(DISCOVERY_FILE);
    assert!(file.exists());

    let started = Instant::now();
    drop(server);
    assert!(started.elapsed() < Duration::from_secs(3), "stopping waited for a client");
    assert!(!file.exists());
    assert!(TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_millis(500)).is_err());
}

// ---- Host: the on/off switch -----------------------------------------------------------------

fn host_in(dir: &std::path::Path) -> (Arc<Vault>, Host) {
    let vault = new_vault(dir);
    (vault.clone(), Host::new(vault))
}

#[test]
fn the_api_is_off_by_default_and_listens_only_when_switched_on() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, host) = host_in(dir.path());
    assert_eq!(host.status(), HostStatus { enabled: false, running: false, port: None });
    host.start_if_enabled();
    assert!(!host.status().running, "started although the person never turned it on");
    assert!(!dir.path().join(DISCOVERY_FILE).exists());

    host.set_enabled(true).unwrap();
    let status = host.status();
    assert!(status.enabled && status.running);
    let port = status.port.unwrap();
    assert!(vault.api_enabled().unwrap());
    assert!(dir.path().join(DISCOVERY_FILE).exists());

    // The real route table is what is served.
    let token = make_token(&vault, Scope::Read);
    assert_eq!(call(port, Some(&token), "status", "").data()["app"], "meowny");
    let listed = call(port, Some(&token), "schema", "").data();
    let names: Vec<&str> = listed["routes"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names, routes::ALL.iter().map(|r| r.name).collect::<Vec<_>>());

    host.set_enabled(true).unwrap();
    assert_eq!(host.status().port, Some(port), "switching on twice changes nothing");

    host.set_enabled(false).unwrap();
    assert_eq!(host.status(), HostStatus { enabled: false, running: false, port: None });
    assert!(!dir.path().join(DISCOVERY_FILE).exists());
    assert!(TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_millis(500)).is_err());
    host.set_enabled(false).unwrap();
}

#[test]
fn switching_the_api_needs_the_unlocked_app_and_changes_nothing_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, host) = host_in(dir.path());
    vault.lock();
    assert_eq!(host.set_enabled(true), Err(AppError::Locked));
    assert_eq!(host.status(), HostStatus { enabled: false, running: false, port: None });

    vault.unlock(None).unwrap();
    host.set_enabled(true).unwrap();
    vault.lock();
    assert_eq!(host.set_enabled(false), Err(AppError::Locked));
    assert!(host.status().running, "still on, and answering 'locked'");
    let reply = call(host.status().port.unwrap(), Some(&format!("mny_{}", "0".repeat(64))), "status", "");
    assert_eq!(reply.code(), "locked");
}

#[test]
fn the_choice_survives_a_restart_and_a_stale_port_file_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, host) = host_in(dir.path());
    host.set_enabled(true).unwrap();
    vault.lock();
    drop(host);
    assert!(!dir.path().join(DISCOVERY_FILE).exists(), "closing the app removes the file");

    // The app starts again, locked, and listens at once so it can answer "locked".
    let restarted = Host::new(vault.clone());
    restarted.start_if_enabled();
    let status = restarted.status();
    assert!(status.enabled && status.running);
    assert_eq!(call(status.port.unwrap(), Some(&format!("mny_{}", "0".repeat(64))), "status", "").code(), "locked");

    // A crash leaves a file behind; a start with the API off removes it.
    vault.unlock(None).unwrap();
    restarted.set_enabled(false).unwrap();
    std::fs::write(dir.path().join(DISCOVERY_FILE), r#"{"api":1,"port":1}"#).unwrap();
    restarted.start_if_enabled();
    assert!(!dir.path().join(DISCOVERY_FILE).exists());
}

#[test]
fn stopping_the_host_keeps_the_choice() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, host) = host_in(dir.path());
    host.set_enabled(true).unwrap();
    host.stop();
    assert!(!host.status().running);
    assert!(vault.api_enabled().unwrap());
    host.start_if_enabled();
    assert!(host.status().running);
}

#[test]
fn an_app_that_is_not_set_up_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(Vault::new(dir.path().to_path_buf(), Box::new(MemoryStore::default())));
    let host = Host::new(vault);
    host.start_if_enabled();
    assert_eq!(host.status(), HostStatus { enabled: false, running: false, port: None });
    assert_eq!(host.set_enabled(true), Err(AppError::Locked));
}

#[test]
fn the_host_status_reaches_the_ui_in_camel_case() {
    let status = HostStatus { enabled: true, running: false, port: Some(5) };
    assert_eq!(serde_json::to_value(status).unwrap(), json!({ "enabled": true, "running": false, "port": 5 }));
}

// ---- the real route table over real sockets ---------------------------------------------------

/// Arguments that make each read route succeed.
fn sample_args(name: &str, category: &str) -> String {
    match name {
        "budgets.overview" | "stats.budget" | "stats.planned" => r#"{"month":"2026-09"}"#.into(),
        "stats.trend" => format!(r#"{{"categoryId":"{category}","from":"2026-01","to":"2026-09"}}"#),
        "stats.year" => r#"{"year":"2026"}"#.into(),
        "stats.top" => r#"{"month":"2026-09","type":"EXPENSE","limit":3}"#.into(),
        "transactions.list" => r#"{"limit":5}"#.into(),
        "categories.list" => r#"{"type":"EXPENSE"}"#.into(),
        "recurring.dues" => r#"{"withinDays":10}"#.into(),
        _ => "{}".into(),
    }
}

#[test]
fn a_read_token_uses_every_read_route_over_http_and_gets_what_the_screens_get() {
    let dir = tempfile::tempdir().unwrap();
    let vault = new_vault(dir.path());
    vault.with_conn(|c| crate::seed::run_for(c, "2026-10-14").map_err(|_| AppError::Internal)).unwrap();
    let token = make_token(&vault, Scope::Read);
    let server = ApiServer::start(vault.clone(), routes::ALL, Limits::default()).unwrap();
    let port = server.port();

    let category = call(port, Some(&token), "categories.list", r#"{"type":"EXPENSE"}"#).data()[0]["id"].as_str().unwrap().to_string();
    let mut called = 0;
    // The routes that need the person's yes have their own tests; here nobody can answer.
    for route in routes::ALL.iter().filter(|r| r.scope == Scope::Read && r.confirm.is_none()) {
        let reply = call(port, Some(&token), route.name, &sample_args(route.name, &category));
        assert_eq!(reply.status, 200, "{}: {}", route.name, reply.body);
        assert_eq!(reply.json()["ok"], true, "{}", route.name);
        called += 1;
    }
    assert!(called >= 14, "only {called} read routes were called");

    // The same numbers the desktop screens get from the same functions.
    let list = call(port, Some(&token), "transactions.list", r#"{"limit":5}"#).data();
    let direct = vault.with_conn(|c| crate::transactions::list(c, &crate::transactions::Filter { limit: Some(5), ..Default::default() })).unwrap();
    assert_eq!(list, serde_json::to_value(direct).unwrap());
    assert!(list["transactions"][0]["amount"].is_string(), "money crosses as a string");
    assert!(list["transactions"][0]["id"].is_string(), "and ids too");
    let overview = call(port, Some(&token), "budgets.overview", r#"{"month":"2026-09"}"#).data();
    assert_eq!(overview, serde_json::to_value(vault.with_conn(|c| crate::budgets::overview(c, "2026-09")).unwrap()).unwrap());

    // The schema describes exactly these routes, and a client can build its requests from it.
    let schema = call(port, Some(&token), "schema", "").data();
    let described: Vec<&str> = schema["routes"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(described, routes::ALL.iter().map(|r| r.name).collect::<Vec<_>>());
    let transactions = schema["routes"].as_array().unwrap().iter().find(|r| r["name"] == "transactions.list").unwrap();
    let params: Vec<(&str, &str)> = transactions["params"].as_array().unwrap().iter().map(|p| (p["name"].as_str().unwrap(), p["type"].as_str().unwrap())).collect();
    assert_eq!(params, [("from", "date"), ("to", "date"), ("categoryId", "id"), ("q", "string"), ("limit", "integer"), ("offset", "integer")]);
}

#[test]
fn a_bad_value_over_http_is_a_400_with_the_reason_and_the_server_keeps_going() {
    let dir = tempfile::tempdir().unwrap();
    let vault = new_vault(dir.path());
    let token = make_token(&vault, Scope::Read);
    let server = ApiServer::start(vault.clone(), routes::ALL, Limits::default()).unwrap();
    let port = server.port();
    let reply = call(port, Some(&token), "budgets.overview", r#"{"month":"October"}"#);
    assert_eq!((reply.status, reply.code().as_str()), (400, "invalid_input"));
    assert_eq!(reply.message(), "`month` must be a month like 2026-10");
    let reply = call(port, Some(&token), "budgets.overview", r#"{"month":"2026-13"}"#);
    assert_eq!((reply.status, reply.message().as_str()), (400, "invalid input"));
    assert_eq!(call(port, Some(&token), "stats.trend", r#"{"categoryId":"99","from":"2026-01","to":"2026-02"}"#).status, 404);
    assert_eq!(call(port, Some(&token), "today", "").status, 200);
}

#[test]
fn a_read_token_cannot_write_and_a_write_token_can_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let vault = new_vault(dir.path());
    vault.with_conn(|c| crate::seed::run_for(c, "2026-10-14").map_err(|_| AppError::Internal)).unwrap();
    let (read, write) = (make_token(&vault, Scope::Read), make_token(&vault, Scope::Write));
    let server = ApiServer::start(vault.clone(), routes::ALL, Limits::default()).unwrap();
    let port = server.port();
    let before = vault.with_conn(|c| crate::backup::dump(c)).unwrap();

    let writes: Vec<&str> = routes::ALL.iter().filter(|r| r.scope == Scope::Write).map(|r| r.name).collect();
    assert!(writes.len() >= 17, "{writes:?}");
    for name in &writes {
        // The scope is checked before the body, so even a request that is wrong in every other way is a 403.
        let refused = call(port, Some(&read), name, "{}");
        assert_eq!((refused.status, refused.code().as_str()), (403, "forbidden"), "{name}");
        assert!(refused.message().contains("read-only"), "{name}: {}", refused.message());
        // With a write token the route is reached: the empty body is then a 400 for what is missing.
        let reached = call(port, Some(&write), name, "{}");
        assert_eq!((reached.status, reached.code().as_str()), (400, "invalid_input"), "{name}: {}", reached.body);
    }
    // A real request that would work is still refused to the read token, and changes nothing.
    let food = call(port, Some(&read), "categories.list", r#"{"type":"EXPENSE"}"#).data()[0]["id"].as_str().unwrap().to_string();
    let request = format!(r#"{{"name":"Bread","amount":"2.40","paymentDate":"2026-10-14","categoryId":"{food}"}}"#);
    assert_eq!(call(port, Some(&read), "transactions.create", &request).status, 403);
    assert_eq!(vault.with_conn(|c| crate::backup::dump(c)).unwrap(), before, "a refused write changed something");
    // The write token's request goes through and shows up for the read token.
    let made = call(port, Some(&write), "transactions.create", &request);
    assert_eq!(made.status, 200, "{}", made.body);
    let seen = call(port, Some(&read), "transactions.list", r#"{"q":"Bread"}"#).data();
    assert_eq!(seen["total"], 1);
    assert_eq!(seen["transactions"][0]["id"], made.data()["id"]);
}

/// The plan's "done when" for the CLI, over the wire: with only a token, record expenses, set a budget
/// and read budget against actual. A budget of 400 and transactions of 356.20 leave 43.80.
#[test]
fn a_write_token_can_record_expenses_set_a_budget_and_read_the_result() {
    let dir = tempfile::tempdir().unwrap();
    let vault = new_vault(dir.path());
    let (read, write) = (make_token(&vault, Scope::Read), make_token(&vault, Scope::Write));
    let server = ApiServer::start(vault.clone(), routes::ALL, Limits::default()).unwrap();
    let port = server.port();
    let post = |token: &str, route: &str, body: Value| {
        let reply = call(port, Some(token), route, &body.to_string());
        assert_eq!(reply.status, 200, "{route}: {}", reply.body);
        reply.data()
    };

    let food = post(&write, "categories.create", json!({ "name": "Food", "type": "EXPENSE" }))["id"].as_str().unwrap().to_string();
    post(&write, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-03", "amountLimit": "400.00", "categoryId": food }));
    let uuids = ["11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222", "33333333-3333-4333-8333-333333333333"];
    for (uuid, amount) in uuids.iter().zip(["120.50", "99.70", "136.00"]) {
        let request = json!({ "name": "Shop", "amount": amount, "paymentDate": "2026-03-12", "categoryId": food, "clientUuid": uuid });
        let first = post(&write, "transactions.create", request.clone());
        // The script did not hear back and tries again: the same transaction, not a second one.
        assert_eq!(post(&write, "transactions.create", request), first);
    }

    let month = post(&read, "stats.budget", json!({ "month": "2026-03" }));
    let line = month["categories"].as_array().unwrap().iter().find(|c| c["name"] == "Food").unwrap();
    assert_eq!(line["budgeted"], "400.00");
    assert_eq!(line["actual"], "356.20", "three payments, each sent twice, counted once");
    assert_eq!(line["remaining"], "43.80");
    assert_eq!(line["pctUsed"], 89.1);
    assert_eq!(post(&read, "transactions.list", json!({ "categoryId": food }))["total"], 3);
    // A later change to the budget does not touch earlier months.
    post(&write, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-04", "amountLimit": "500.00", "categoryId": food }));
    let march = post(&read, "stats.budget", json!({ "month": "2026-03" }));
    assert_eq!(march["categories"].as_array().unwrap().iter().find(|c| c["name"] == "Food").unwrap()["budgeted"], "400.00");
}
