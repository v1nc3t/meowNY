//! Throwing odd input at the server: damaged requests at the socket, and odd values at every route. What is
//! checked is what a program that other programs depend on must guarantee: it answers (usually with a
//! refusal), no route ever panics, the data stays whole and the server is still up afterwards.
//!
//! The input comes from a small fixed-seed generator, so a failure repeats. No crates.

use std::io::{Read, Write};
use std::net::Shutdown;
use std::time::Duration;

use serde_json::{json, Map, Number, Value};

use super::routes::{self, Kind, Param, Route};
use super::tests::{call, connect, headers, make_token, new_vault, raw_request};
use super::*;
use crate::tokens::Scope;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn seeded_server(limits: Limits) -> (tempfile::TempDir, Arc<Vault>, ApiServer, String) {
    let dir = tempfile::tempdir().unwrap();
    let vault = new_vault(dir.path());
    vault.with_conn(|c| crate::seed::run_for(c, "2026-10-14").map_err(|_| AppError::Internal)).unwrap();
    // a few documents, so the routes that work on one have something to find
    vault
        .with_files(|conn, files| {
            for (n, bytes) in [(1, b"%PDF-1.4 a receipt".to_vec()), (2, b"plain notes".to_vec()), (3, vec![7u8; 3000])] {
                let input = crate::documents::library::NewDocument {
                    name: format!("scan {n}"),
                    links: vec![crate::documents::library::LinkTarget { kind: crate::documents::library::TargetKind::Transaction, id: n.to_string() }],
                    ..Default::default()
                };
                crate::documents::library::add(conn, files, &input, &bytes, "2026-10-14")?;
            }
            Ok(())
        })
        .unwrap();
    let token = make_token(&vault, Scope::Write);
    // No prompter: a route that needs the person's yes can only be refused, which is the point here.
    let server = ApiServer::start(vault.clone(), routes::ALL, limits).unwrap();
    (dir, vault, server, token)
}

/// What a sound server still is after everything: its data passes SQLite's own checks.
fn whole(vault: &Vault) {
    let (integrity, dangling): (String, bool) = vault
        .with_conn(|c| {
            let integrity = c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))?;
            let dangling = c.prepare("PRAGMA foreign_key_check")?.exists([])?;
            Ok((integrity, dangling))
        })
        .unwrap();
    assert_eq!(integrity, "ok");
    assert!(!dangling, "a row points at a row that is not there");
}

// ---- damaged requests at the socket ----------------------------------------------------------

fn mutate(rng: &mut Rng, mut bytes: Vec<u8>) -> Vec<u8> {
    for _ in 0..1 + rng.below(4) {
        match rng.below(5) {
            0 if !bytes.is_empty() => {
                let at = rng.below(bytes.len());
                bytes[at] = rng.next() as u8;
            }
            1 if !bytes.is_empty() => {
                let at = rng.below(bytes.len());
                bytes.truncate(at);
            }
            2 if !bytes.is_empty() => {
                let at = rng.below(bytes.len());
                let end = (at + rng.below(12)).min(bytes.len());
                bytes.drain(at..end);
            }
            3 => {
                let at = rng.below(bytes.len() + 1);
                let extra: Vec<u8> = (0..rng.below(16)).map(|_| *rng.pick(b"\r\n :;-=/%\0abc0123")).collect();
                bytes.splice(at..at, extra);
            }
            _ => bytes.extend((0..rng.below(24)).map(|_| rng.next() as u8)),
        }
    }
    bytes
}

#[test]
fn no_damaged_request_stops_the_server_or_makes_it_panic_or_hang() {
    let (_dir, vault, server, token) = seeded_server(Limits { request_time: Duration::from_secs(3), connections: 16 });
    let port = server.port();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let valid = [
        raw_request("POST", "/v1/status", &headers(port, Some(&token)), ""),
        raw_request("POST", "/v1/transactions.list", &headers(port, Some(&token)), r#"{"limit":3}"#),
        raw_request("POST", "/v1/categories.create", &headers(port, Some(&token)), r#"{"name":"Fuzz","type":"EXPENSE"}"#),
    ];
    let (mut answered, mut silent) = (0, 0);
    for i in 0..600 {
        let payload = match i % 4 {
            0 => (0..rng.below(300)).map(|_| rng.next() as u8).collect(),
            1 => {
                let base = rng.pick(&valid).clone();
                mutate(&mut rng, base)
            }
            2 => {
                // A head that promises an absurd or odd body.
                let length = *rng.pick(&["0", "1", "99999999999", "-5", "4294967296", "1048577", "abc", "00", "1 2"]);
                format!("POST /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Length: {length}\r\n\r\n{{}}").into_bytes()
            }
            _ => {
                let whole = rng.pick(&valid).clone();
                whole[..rng.below(whole.len())].to_vec()
            }
        };
        let mut stream = connect(port);
        let _ = stream.write_all(&payload);
        // Done sending: the server sees the end of the request and has to answer or let go, not wait.
        let _ = stream.shutdown(Shutdown::Write);
        // (The read timeout was set by `connect`. Setting it here fails on macOS with "invalid argument" when the
        // server has already closed its end, which it does at once for a request that is plainly wrong.)
        let mut reply = Vec::new();
        let _ = stream.read_to_end(&mut reply);
        if reply.is_empty() {
            silent += 1;
        } else {
            answered += 1;
            let head = String::from_utf8_lossy(&reply[..reply.len().min(12)]).into_owned();
            assert!(head.starts_with("HTTP/1.1 "), "a reply that is not HTTP: {head:?}");
        }
    }
    assert!(answered > 50 && silent > 50, "answered {answered}, let go {silent}: the generator reaches both");
    assert_eq!(server.panics(), 0, "a damaged request made a route panic");
    assert_eq!(call(port, Some(&token), "status", "").status, 200, "the server is not answering any more");
    whole(&vault);
}

#[test]
fn many_half_open_connections_do_not_starve_the_server_for_ever() {
    let (_dir, _vault, server, token) = seeded_server(Limits { request_time: Duration::from_millis(500), connections: 8 });
    let port = server.port();
    // Eight clients say nothing and hold their connections: all the slots.
    let held: Vec<_> = (0..8).map(|_| connect(port)).collect();
    std::thread::sleep(Duration::from_millis(150));
    let mut refused = connect(port);
    let _ = refused.write_all(&raw_request("POST", "/v1/status", &headers(port, Some(&token)), ""));
    let mut reply = Vec::new();
    let _ = refused.read_to_end(&mut reply);
    assert!(reply.is_empty(), "served while every slot was taken");
    // They time out, and the server is whole again.
    std::thread::sleep(Duration::from_millis(900));
    assert_eq!(call(port, Some(&token), "status", "").status, 200);
    drop(held);
}

// ---- odd values at every route ----------------------------------------------------------------

const TEXTS: &[&str] = &[
    "", " ", "x", "Groceries", "Rent", "日本語", "é", "\u{1b}[31mred", "a\nb", "a\u{202e}b", "'; DROP TABLE categories;--", "%", "_", "\\", "0", "-1", "1e400", "null",
    "true", "\u{0}", "{\"a\":1}", "../../etc/passwd", "ro", "en", "UTC", "Europe/Dublin", "Mars/Base", "en-IE", "\u{feff}",
];

fn junk(rng: &mut Rng) -> Value {
    match rng.below(9) {
        0 => Value::Null,
        1 => json!(rng.below(2) == 0),
        2 => json!(rng.next()),
        3 => json!(-(rng.next() as i64 / 2)),
        4 => Number::from_f64(rng.below(1000) as f64 / 7.0).map(Value::Number).unwrap_or(Value::Null),
        5 => json!(*rng.pick(TEXTS)),
        6 => json!("x".repeat(rng.below(700))),
        7 => json!([]),
        _ => json!({ "a": { "b": [1, 2, { "c": null }] } }),
    }
}

/// A value that has a chance of getting past the shape check and into the route itself.
fn plausible(rng: &mut Rng, param: &Param) -> Value {
    let pick = |rng: &mut Rng, items: &[&str]| json!(*rng.pick(items));
    if !param.values.is_empty() && rng.chance(80) {
        return json!(*rng.pick(param.values));
    }
    match param.kind {
        Kind::String => json!(*rng.pick(TEXTS)),
        Kind::Integer => json!(*rng.pick(&[0i64, 1, 2, 3, 5, 10, 50, 100, 101, 366, 367, 1000, 1001, -1, i64::MAX, i64::MIN])),
        Kind::Boolean => json!(rng.below(2) == 0),
        Kind::Date => pick(rng, &["2026-10-14", "2026-02-30", "2026-01-01", "2025-12-31", "0000-00-00", "9999-12-31", "2026-13-01", "2024-02-29"]),
        Kind::Month => pick(rng, &["2026-10", "2026-01", "2026-12", "2027-03", "2025-01", "2026-13", "0000-00", "9999-12", "2000-01"]),
        Kind::Year => pick(rng, &["2026", "2025", "0000", "9999", "2024"]),
        Kind::Id => json!((rng.below(30) + (rng.below(12) / 11) * 999_999).to_string().replace('0', if rng.chance(5) { "00" } else { "1" })),
        Kind::Money => pick(rng, &["0.00", "0.01", "12.50", "400.00", "9999999999.99", "99999999999.99", "1.5", "1000000.00", "3.14"]),
        Kind::Uuid => pick(rng, &["123e4567-e89b-12d3-a456-426614174000", "9b2f5d44-6e1a-4c3b-8f2d-1a2b3c4d5e6f", "not-a-uuid"]),
        Kind::Ids => Value::Array((0..rng.below(4)).map(|_| json!((rng.below(30) + 1).to_string())).collect()),
        Kind::Path => pick(rng, &["/", "/tmp", "relative.json", "/nonexistent/folder/x.json", "C:\\x.json", "/tmp/a.json/../b.json", ""]),
        Kind::Bytes => pick(rng, &["JVBERi0xLjQK", "iVBORw0KGgo", "AAAA", "-_-_", "not base64url!", "AA==", "x", "aGVsbG8gd29ybGQ"]),
    }
}

fn body_for(rng: &mut Rng, route: &Route) -> Value {
    let mut body = Map::new();
    for param in route.params {
        if !(param.required || rng.chance(60)) {
            continue;
        }
        let value = if rng.chance(70) { plausible(rng, param) } else { junk(rng) };
        body.insert(param.name.to_string(), value);
    }
    if rng.chance(5) {
        body.insert(rng.pick(TEXTS).to_string(), junk(rng));
    }
    if rng.chance(3) {
        body.clear();
    }
    Value::Object(body)
}

#[test]
fn no_value_makes_any_route_panic_and_the_data_stays_whole() {
    let (_dir, vault, server, token) = seeded_server(Limits::default());
    let port = server.port();
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let (mut ok, mut refused) = (0, 0);
    let mut by_route: std::collections::BTreeMap<&str, usize> = Default::default();
    for i in 0..4000 {
        let route = &routes::ALL[i % routes::ALL.len()];
        let body = body_for(&mut rng, route);
        let reply = call(port, Some(&token), route.name, &body.to_string());
        let json = reply.json();
        assert!(json["ok"].is_boolean(), "{} {body}: {}", route.name, reply.body);
        if json["ok"] == true {
            ok += 1;
            *by_route.entry(route.name).or_default() += 1;
        } else {
            refused += 1;
            let code = json["error"]["code"].as_str().unwrap_or("");
            assert!(!code.is_empty() && json["error"]["message"].is_string(), "{} {body}: {}", route.name, reply.body);
            // Nothing a route says in an error holds what the database said.
            let message = json["error"]["message"].as_str().unwrap().to_lowercase();
            for leak in ["sqlite", "select ", "insert ", "constraint", "rusqlite", "panicked", "unwrap"] {
                assert!(!message.contains(leak), "{} {body}: {message}", route.name);
            }
        }
        assert!([200, 400, 403, 404, 409, 413, 423, 500].contains(&reply.status), "{} gave {}", route.name, reply.status);
    }
    assert_eq!(server.panics(), 0, "a route panicked on odd input");
    // The generator reaches into the routes, not only the shape check in front of them.
    assert!(ok > 500 && refused > 500, "ok {ok}, refused {refused}");
    let reached = by_route.len();
    assert!(reached >= 25, "only {reached} routes ever succeeded: {by_route:?}");
    assert_eq!(call(port, Some(&token), "status", "").status, 200);
    whole(&vault);
}

#[test]
fn a_route_cannot_be_made_to_write_a_file_without_the_person() {
    // Whatever is thrown at the routes that write files, with nobody to ask, no file appears.
    let (dir, vault, server, token) = seeded_server(Limits::default());
    let port = server.port();
    let files = tempfile::tempdir().unwrap();
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    let writers: Vec<&Route> = routes::ALL.iter().filter(|r| r.confirm.is_some()).collect();
    assert_eq!(writers.len(), 6, "the exports, the backup and the copy of a document");
    for i in 0..400 {
        let route = writers[i % writers.len()];
        let mut body = body_for(&mut rng, route);
        // Often a path that would really work: absolute, the right ending, in a folder that exists.
        if rng.chance(70) {
            let ext = route.name.split('.').nth(1).map(|v| if v == "create" { "meowbak" } else if v == "get" { "pdf" } else { v }).unwrap();
            body["out"] = json!(files.path().join(format!("f{i}.{ext}")).to_str().unwrap());
        }
        let reply = call(port, Some(&token), route.name, &body.to_string());
        assert_ne!(reply.status, 200, "{} wrote something without being allowed: {body}", route.name);
    }
    assert_eq!(std::fs::read_dir(files.path()).unwrap().count(), 0, "a file was written without the person's yes");
    // And nothing appeared next to the app's own data either.
    let mut names: Vec<String> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    assert!(names.iter().all(|n| n == "meowny.db" || n == "vault.json" || n == "api.json" || n == "documents" || n.starts_with("meowny.db-")), "{names:?}");
    whole(&vault);
}
