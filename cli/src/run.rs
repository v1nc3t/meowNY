//! One run of the program: parse the command line, do what it says, print the result, and return the
//! exit code. Everything it reads and writes is passed in, so tests can drive it without a terminal.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::args::{self, Invocation, Parsed};
use crate::catalog::{Action, Catalog, Command, API_VERSION};
use crate::client::{self, Env};
use crate::error::CliError;
use crate::{help, local, output};

/// A random version 4 UUID, from the operating system's random keys for hashing plus the clock. It
/// only has to be different each time: it is the key that makes a retried transaction safe.
pub fn uuid_v4() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut bytes = [0u8; 16];
    for half in bytes.chunks_mut(8) {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        hasher.write_u128(nanos);
        hasher.write_u32(std::process::id());
        half.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// How long to wait for the app. A command the person has to allow waits for them, which takes longer than one
/// that does not.
fn timeout_for(command: &Command, flag: Option<u64>) -> Duration {
    let default = if command.confirm { client::CONFIRM_TIMEOUT_SECS } else { client::DEFAULT_TIMEOUT_SECS };
    Duration::from_secs(flag.unwrap_or(default))
}

/// How long an answer may take before the person at the terminal is told what it is waiting for. A request the app
/// refuses at once never says it.
const TELL_AFTER: Duration = Duration::from_millis(1000);

/// The request, with a line on standard error if it takes more than a moment: for a command the person has to allow in
/// the app, that is where they should look.
fn call_telling_the_person(port: u16, token: &str, route: &str, body: &Value, timeout: Duration, err: &mut dyn Write) -> Result<Value, CliError> {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let _ = send.send(client::call(port, token, route, body, timeout));
        });
        match receive.recv_timeout(TELL_AFTER) {
            Ok(result) => result,
            Err(_) => {
                let _ = writeln!(err, "waiting for you to allow this in the meowNY app...");
                receive.recv().unwrap_or_else(|_| Err(CliError::new("internal", "the request stopped unexpectedly")))
            }
        }
    })
}

/// Whether `--json` was given, even when the rest of the command line could not be read.
fn wants_json(args: &[String]) -> bool {
    args.iter().take_while(|a| a.as_str() != "--").any(|a| a == "--json" || a == "--json=true")
}

/// What a person reads: the app's message, plus what to do about the cases that have an obvious next step.
fn human(e: &CliError) -> String {
    let message = output::clean(&e.message);
    match e.code.as_str() {
        "locked" => format!("{message}. Unlock it in the app, then try again."),
        "unauthorized" => format!("{message}. Make a new token in the app if this one has expired or was revoked."),
        _ => message,
    }
}

fn fail(e: &CliError, json: bool, err: &mut dyn Write) -> u8 {
    let _ = if json { writeln!(err, "{}", output::compact(&e.to_json())) } else { writeln!(err, "error: {}", human(e)) };
    e.exit()
}

/// For a page of a longer list (an answer with `total`, `limit`, `offset` and a list), where the rest is.
/// People get it on standard error; a script reads the same three numbers in the JSON.
fn page_hint(data: &Value) -> Option<String> {
    let map = data.as_object()?;
    let (total, offset) = (map.get("total")?.as_u64()?, map.get("offset")?.as_u64()?);
    map.get("limit")?.as_u64()?;
    let shown = map.values().find_map(Value::as_array)?.len() as u64;
    let end = offset + shown;
    if shown == 0 && offset > 0 && offset >= total {
        return Some(format!("{total} in all; --offset {offset} is past the end"));
    }
    (end < total).then(|| format!("showing {}-{end} of {total}; next page: --offset {end}", offset + 1))
}

/// `reads` is whether the command only looks: when it finds nothing, "done" would be wrong.
fn show(data: &Value, json: bool, reads: bool, out: &mut dyn Write, err: &mut dyn Write) {
    if json {
        let _ = writeln!(out, "{}", output::compact(data));
    } else if data.is_null() {
        let _ = writeln!(err, "{}", if reads { "there is nothing to show." } else { "done." });
    } else {
        let _ = out.write_all(output::human(data).as_bytes());
        if let Some(hint) = page_hint(data) {
            let _ = writeln!(err, "{hint}");
        }
    }
}

fn execute(
    inv: Invocation,
    catalog: &Catalog,
    env: Env,
    stdin: &mut dyn BufRead,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let g = &inv.globals;
    if g.app && inv.command.action != Action::Schema {
        return Err(CliError::usage("--app only applies to schema"));
    }
    match inv.command.action {
        Action::Schema if !g.app => {
            let _ = writeln!(out, "{}", output::compact(&local::schema(catalog)));
            Ok(())
        }
        Action::Login => {
            let path = local::login(env, g, stdin)?;
            if g.json {
                let _ = writeln!(out, "{}", output::compact(&json!({ "saved": true, "path": path.display().to_string() })));
            } else {
                let _ = writeln!(err, "token saved to {}", path.display());
            }
            Ok(())
        }
        Action::Logout => {
            let (path, existed) = local::logout(env, g)?;
            if g.json {
                let _ = writeln!(out, "{}", output::compact(&json!({ "removed": existed, "path": path.display().to_string() })));
            } else if existed {
                let _ = writeln!(err, "token removed from {}", path.display());
            } else {
                let _ = writeln!(err, "there was no saved token at {}", path.display());
            }
            Ok(())
        }
        Action::Route | Action::Schema => {
            let route = inv.command.route.clone().unwrap_or_default();
            // Local checks first: a missing token is the same answer whether or not the app is up.
            let token = client::token(env, g.token_file.as_deref())?;
            let port = client::port(env, g.port, g.data_dir.as_deref())?;
            let timeout = timeout_for(&inv.command, g.timeout_secs);
            let mut body = inv.body;
            if route == "transactions.create" && !body.contains_key("clientUuid") {
                body.insert("clientUuid".into(), Value::String(uuid_v4()));
            }
            let body = Value::Object(body);
            let data = if inv.command.confirm && !g.json {
                call_telling_the_person(port, &token, &route, &body, timeout, err)?
            } else {
                client::call(port, &token, &route, &body, timeout)?
            };
            show(&data, g.json, inv.command.scope.as_deref() == Some("read"), out, err);
            Ok(())
        }
    }
}

/// Runs one command line (without the program name). Returns the exit code.
pub fn run(args: &[String], env: Env, stdin: &mut dyn BufRead, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let catalog = Catalog::embedded();
    let json = wants_json(args);
    let parsed = match args::parse(args, &catalog, stdin) {
        Ok(parsed) => parsed,
        Err(e) => return fail(&e, json, err),
    };
    match parsed {
        Parsed::Help(words) => match help::render(&catalog, &words) {
            Ok(text) => {
                let _ = out.write_all(text.as_bytes());
                0
            }
            Err(e) => fail(&e, json, err),
        },
        Parsed::Version => {
            let _ = writeln!(out, "meowny {} (API {API_VERSION})", env!("CARGO_PKG_VERSION"));
            0
        }
        Parsed::Run(inv) => {
            let json = inv.globals.json;
            match execute(*inv, &catalog, env, stdin, out, err) {
                Ok(()) => 0,
                Err(e) => fail(&e, json, err),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::io::Read;
    use std::net::TcpListener;

    const TOKEN: &str = "mny_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    struct Ran {
        code: u8,
        out: String,
        err: String,
    }

    fn go(line: &str, vars: &[(&str, &str)], stdin: &str) -> Ran {
        let args: Vec<String> = line.split_whitespace().map(String::from).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let e = env(vars);
        let code = run(&args, &e, &mut stdin.as_bytes(), &mut out, &mut err);
        Ran { code, out: String::from_utf8(out).unwrap(), err: String::from_utf8(err).unwrap() }
    }

    /// A stand-in for the app: answers one request with `reply` and hands back what it received.
    fn fake_app(reply: &'static str) -> (u16, std::thread::JoinHandle<String>) {
        fake_app_after(reply, Duration::ZERO)
    }

    /// The same, but it takes `delay` to answer, like a person deciding in a dialog.
    fn fake_app_after(reply: &'static str, delay: Duration) -> (u16, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut received = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = stream.read(&mut chunk).unwrap();
                received.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&received).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length = head.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
                    if body.len() >= length {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            std::thread::sleep(delay);
            let _ = stream.write_all(reply.as_bytes());
            String::from_utf8_lossy(&received).to_string()
        });
        (port, handle)
    }

    fn answer(body: &str) -> &'static str {
        Box::leak(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_boxed_str())
    }

    fn answer_with(status: &str, body: &str) -> &'static str {
        Box::leak(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_boxed_str())
    }

    #[test]
    fn a_uuid_has_the_version_4_shape_and_is_never_repeated() {
        let mut seen = HashSet::new();
        for _ in 0..2000 {
            let id = uuid_v4();
            let b = id.as_bytes();
            assert_eq!(id.len(), 36);
            assert!([8, 13, 18, 23].iter().all(|i| b[*i] == b'-'), "{id}");
            assert_eq!(b[14], b'4', "{id}");
            assert!(matches!(b[19], b'8' | b'9' | b'a' | b'b'), "{id}");
            assert!(id.bytes().all(|c| c == b'-' || c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{id}");
            assert!(seen.insert(id), "a repeated uuid");
        }
    }

    #[test]
    fn help_and_version_need_no_app_and_no_token() {
        let top = go("--help", &[], "");
        assert_eq!((top.code, top.err.as_str()), (0, ""));
        assert!(top.out.contains("commands:") && top.out.contains("transactions"));
        assert!(go("transactions create --help", &[], "").out.contains("usage: meowny transactions create"));
        assert!(go("help budgets", &[], "").out.contains("overview"));
        let v = go("--version", &[], "");
        assert_eq!((v.code, v.out.starts_with("meowny 0.1.0 (API 1)")), (0, true));
        assert_eq!(go("nope --help", &[], "").code, 2);
    }

    #[test]
    fn schema_is_answered_without_the_app_or_a_token() {
        let r = go("schema", &[], "");
        assert_eq!((r.code, r.err.as_str()), (0, ""));
        let json: Value = serde_json::from_str(&r.out).unwrap();
        assert!(json["commands"].as_array().unwrap().len() > 30);
        assert_eq!(r.out.lines().count(), 1, "one line");
        assert_eq!(go("schema --json", &[], "").out, r.out);
    }

    #[test]
    fn usage_errors_end_with_2_before_anything_else_is_tried() {
        // No token and no app: a usage error is still a usage error.
        for line in ["transactions create", "nope", "transactions", "transactions list --limit ten", "transactions delete 5", "status --what", ""] {
            let r = go(line, &[], "");
            assert_eq!(r.code, 2, "{line}: {}", r.err);
            assert!(r.err.starts_with("error: "), "{line}: {}", r.err);
            assert_eq!(r.out, "", "{line}");
        }
        let r = go("transactions create --json", &[], "");
        let e: Value = serde_json::from_str(&r.err).unwrap();
        assert_eq!((r.code, e["code"].clone()), (2, json!("usage")));
        assert!(e["message"].as_str().unwrap().contains("missing --name"));
        assert_eq!(go("status --app", &[("MEOWNY_TOKEN", TOKEN)], "").code, 2, "--app is for schema");
        assert_eq!(go("--json --nope status", &[], "").err.lines().count(), 1);
    }

    #[test]
    fn a_missing_token_is_exit_3_whether_or_not_the_app_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_str().unwrap();
        let none = [("XDG_CONFIG_HOME", d), ("HOME", d), ("USERPROFILE", d), ("APPDATA", d), ("MEOWNY_DATA_DIR", d)];
        let r = go("status", &none, "");
        assert_eq!((r.code, r.out.as_str()), (3, ""));
        assert!(r.err.contains("MEOWNY_TOKEN") && r.err.contains("meowny login"));
        let r = go("status --json", &none, "");
        assert_eq!(serde_json::from_str::<Value>(&r.err).unwrap()["code"], "no_token");
        let r = go("status", &[("MEOWNY_TOKEN", "garbage"), ("MEOWNY_DATA_DIR", dir.path().to_str().unwrap())], "");
        assert_eq!(r.code, 3);
        assert!(!r.err.contains("garbage"));
    }

    #[test]
    fn with_a_token_but_no_app_it_is_exit_1() {
        let dir = tempfile::tempdir().unwrap();
        let vars = [("MEOWNY_TOKEN", TOKEN), ("MEOWNY_DATA_DIR", dir.path().to_str().unwrap())];
        let r = go("status", &vars, "");
        assert_eq!(r.code, 1);
        assert!(r.err.contains("not running") && r.err.contains("Settings") && !r.err.contains(TOKEN));
        // A port nobody listens on.
        let closed = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
        let r = go(&format!("status --port {closed} --json"), &vars, "");
        assert_eq!((r.code, serde_json::from_str::<Value>(&r.err).unwrap()["code"].clone()), (1, json!("app_unavailable")));
    }

    #[test]
    fn a_request_is_what_the_app_expects_and_the_token_is_in_the_header_only() {
        let (port, app) = fake_app(answer(r#"{"data":{"app":"meowny","api":1,"scope":"read","version":"0.1.0","schemaVersion":7},"ok":true}"#));
        let r = go(&format!("status --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "");
        let request = app.join().unwrap();
        assert!(request.starts_with("POST /v1/status HTTP/1.1\r\n"), "{request}");
        assert!(request.contains(&format!("Host: 127.0.0.1:{port}\r\n")));
        assert!(request.contains(&format!("Authorization: Bearer {TOKEN}\r\n")));
        assert!(request.ends_with("\r\n\r\n{}"), "an empty object: {request:?}");
        assert!(request.matches(TOKEN).count() == 1, "the token is sent once, in the header");
        assert_eq!(r.code, 0);
        assert_eq!(r.out, "api: 1\napp: meowny\nschemaVersion: 7\nscope: read\nversion: 0.1.0\n");
        assert_eq!(r.err, "");
        assert!(!r.out.contains(TOKEN) && !r.err.contains(TOKEN));
    }

    #[test]
    fn the_body_holds_the_flags_and_a_new_transaction_gets_a_fresh_client_uuid() {
        let (port, app) = fake_app(answer(r#"{"data":{"id":"5"},"ok":true}"#));
        let line = format!("transactions create --name Bread --amount 2.40 --payment-date 2026-10-14 --category-id 7 --port {port} --json");
        let r = go(&line, &[("MEOWNY_TOKEN", TOKEN)], "");
        let request = app.join().unwrap();
        let body: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!((body["name"].clone(), body["amount"].clone(), body["paymentDate"].clone(), body["categoryId"].clone()), (json!("Bread"), json!("2.40"), json!("2026-10-14"), json!("7")));
        assert_eq!(body["clientUuid"].as_str().unwrap().len(), 36);
        assert!(request.starts_with("POST /v1/transactions.create "));
        assert_eq!((r.code, r.out.as_str()), (0, "{\"id\":\"5\"}\n"));

        // A uuid the caller gives is kept, so a retry is the same request.
        let (port, app) = fake_app(answer(r#"{"data":null,"ok":true}"#));
        go(&format!("transactions create --name B --amount 1.00 --payment-date 2026-10-14 --category-id 7 --client-uuid 123e4567-e89b-12d3-a456-426614174000 --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "");
        assert!(app.join().unwrap().contains("\"clientUuid\":\"123e4567-e89b-12d3-a456-426614174000\""));
    }

    #[test]
    fn a_read_that_finds_nothing_says_so_instead_of_done() {
        let (port, app) = fake_app(answer(r#"{"data":null,"ok":true}"#));
        let r = go(&format!("backup schedule --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "");
        app.join().unwrap();
        assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "", "there is nothing to show.\n"));
    }

    #[test]
    fn nothing_returned_prints_done_on_standard_error_and_null_in_json() {
        let (port, app) = fake_app(answer(r#"{"data":null,"ok":true}"#));
        let r = go(&format!("transactions delete 5 --yes --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "");
        let request = app.join().unwrap();
        assert!(request.starts_with("POST /v1/transactions.delete ") && request.ends_with("{\"id\":\"5\"}"));
        assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "", "done.\n"));
        let (port, _app) = fake_app(answer(r#"{"data":null,"ok":true}"#));
        let r = go(&format!("transactions delete 5 --yes --json --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "");
        assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "null\n", ""));
    }

    #[test]
    fn what_the_app_refuses_maps_to_the_documented_exit_codes() {
        let cases: [(&str, &str, &str, u8); 12] = [
            ("400 Bad Request", "invalid_input", "`amount` must be an amount", 2),
            ("401 Unauthorized", "unauthorized", "the token is missing, unknown, expired or revoked", 3),
            ("403 Forbidden", "forbidden", "this token is read-only", 3),
            ("404 Not Found", "not_found", "not found", 4),
            ("404 Not Found", "unknown_route", "this app has no such route; update the app or the client", 1),
            ("403 Forbidden", "denied", "the person did not allow this in the app", 3),
            ("409 Conflict", "file_exists", "that file already exists; choose a name that does not", 1),
            ("409 Conflict", "name_exists", "that name already exists", 1),
            ("409 Conflict", "not_set_up", "the app is not set up yet", 1),
            ("413 Content Too Large", "too_large", "the request is too large", 1),
            ("423 Locked", "locked", "the app is locked", 3),
            ("500 Internal Server Error", "internal", "something went wrong", 1),
        ];
        for (status, code, message, exit) in cases {
            let body = format!(r#"{{"error":{{"code":"{code}","message":"{message}"}},"ok":false}}"#);
            for json in [false, true] {
                let (port, app) = fake_app(answer_with(status, &body));
                let flag = if json { " --json" } else { "" };
                let r = go(&format!("status --port {port}{flag}"), &[("MEOWNY_TOKEN", TOKEN)], "");
                app.join().unwrap();
                assert_eq!((r.code, r.out.as_str()), (exit, ""), "{code}");
                if json {
                    assert_eq!(serde_json::from_str::<Value>(&r.err).unwrap(), json!({ "code": code, "message": message }));
                } else {
                    assert!(r.err.starts_with(&format!("error: {message}")), "{}", r.err);
                }
                assert!(!r.err.contains(TOKEN));
            }
        }
        // Locked and unauthorized say what to do next.
        let (port, _a) = fake_app(answer_with("423 Locked", r#"{"error":{"code":"locked","message":"the app is locked"},"ok":false}"#));
        assert!(go(&format!("status --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "").err.contains("Unlock it in the app"));
        let (port, _a) = fake_app(answer_with("401 Unauthorized", r#"{"error":{"code":"unauthorized","message":"x"},"ok":false}"#));
        assert!(go(&format!("status --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "").err.contains("Make a new token"));
    }

    #[test]
    fn a_message_from_the_app_cannot_move_the_cursor_or_recolour_the_terminal() {
        let body = r#"{"error":{"code":"invalid_input","message":"bad \u001b[31mred\u001b]0;title\u0007 value\nnext"},"ok":false}"#;
        let (port, app) = fake_app(answer_with("400 Bad Request", body));
        let r = go(&format!("status --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "");
        app.join().unwrap();
        assert_eq!(r.code, 2);
        assert!(!r.err.contains('\u{1b}') && !r.err.contains('\u{7}'), "{:?}", r.err);
        assert_eq!(r.err.lines().count(), 1, "a line break in a message does not add lines: {:?}", r.err);
        // In JSON it is escaped by the encoder, which is safe to pipe.
        let (port, app) = fake_app(answer_with("400 Bad Request", body));
        let r = go(&format!("status --port {port} --json"), &[("MEOWNY_TOKEN", TOKEN)], "");
        app.join().unwrap();
        assert!(!r.err.contains('\u{1b}') && r.err.contains("\\u001b"));
    }

    #[test]
    fn an_answer_the_program_cannot_read_is_an_error_not_a_crash() {
        for reply in [
            answer("not json"),
            answer("[]"),
            answer(r#"{"data":1}"#),
            "garbage",
            "HTTP/1.1 200 OK\r\nContent-Length: 99\r\n\r\n{}",
            "",
        ] {
            let (port, app) = fake_app(reply);
            let r = go(&format!("status --port {port} --json"), &[("MEOWNY_TOKEN", TOKEN)], "");
            app.join().unwrap();
            assert_eq!(r.code, 1, "{reply:?}: {}", r.err);
            assert_eq!(r.out, "");
            assert!(["bad_answer", "app_unavailable"].contains(&serde_json::from_str::<Value>(&r.err).unwrap()["code"].as_str().unwrap()), "{reply:?}: {}", r.err);
        }
    }

    #[test]
    fn an_app_that_never_answers_is_a_timeout_after_the_time_asked() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let _silent = std::thread::spawn(move || {
            let _held = listener.accept();
            std::thread::sleep(Duration::from_secs(5));
        });
        let started = std::time::Instant::now();
        let r = go(&format!("status --port {port} --timeout 1 --json"), &[("MEOWNY_TOKEN", TOKEN)], "");
        assert_eq!(r.code, 1);
        assert_eq!(serde_json::from_str::<Value>(&r.err).unwrap()["code"], "timeout");
        assert!(started.elapsed() < Duration::from_secs(4), "{:?}", started.elapsed());
    }

    #[test]
    fn a_command_the_person_must_allow_waits_longer_than_the_apps_own_two_minutes() {
        let catalog = Catalog::embedded();
        let asks = catalog.find(&["export", "json"]).unwrap();
        let plain = catalog.find(&["transactions", "list"]).unwrap();
        assert!(asks.confirm && !plain.confirm);
        assert_eq!(timeout_for(plain, None), Duration::from_secs(30));
        assert_eq!(timeout_for(asks, None), Duration::from_secs(180));
        assert!(timeout_for(asks, None) > Duration::from_secs(120), "the app gives up on the person after two minutes");
        assert_eq!(timeout_for(asks, Some(7)), Duration::from_secs(7), "--timeout wins");
        assert_eq!(timeout_for(plain, Some(500)), Duration::from_secs(500));
    }

    #[test]
    fn it_says_it_is_waiting_for_the_person_only_if_the_answer_takes_a_moment_and_only_in_the_plain_form() {
        let reply = r#"{"data":{"path":"/tmp/x.json","bytes":5},"ok":true}"#;
        let vars = [("MEOWNY_TOKEN", TOKEN)];
        // The person takes their time: told where to look.
        let (port, app) = fake_app_after(answer(reply), Duration::from_millis(1300));
        let slow = go(&format!("export json --out /tmp/x.json --port {port}"), &vars, "");
        let request = app.join().unwrap();
        assert!(request.starts_with("POST /v1/export.json "), "{request}");
        if !cfg!(windows) {
            assert!(request.ends_with("{\"out\":\"/tmp/x.json\"}"), "{request}");
        }
        assert_eq!(slow.err, "waiting for you to allow this in the meowNY app...\n");
        assert_eq!(slow.out, "bytes: 5\npath: /tmp/x.json\n");
        // The app answers at once (a refusal, say): nothing to wait for, nothing said.
        let (port, app) = fake_app(answer(reply));
        let quick = go(&format!("export json --out /tmp/x.json --port {port}"), &vars, "");
        app.join().unwrap();
        assert_eq!((quick.err.as_str(), quick.code), ("", 0));
        let (port, app) = fake_app(answer_with("409 Conflict", r#"{"error":{"code":"file_exists","message":"that file already exists"},"ok":false}"#));
        let refused = go(&format!("export json --out /tmp/x.json --port {port}"), &vars, "");
        app.join().unwrap();
        assert_eq!((refused.code, refused.err.as_str()), (1, "error: that file already exists\n"), "no 'waiting' before an instant refusal");
        // A script reading JSON gets no extra text even when it waits.
        let (port, app) = fake_app_after(answer(reply), Duration::from_millis(1300));
        let json = go(&format!("export json --out /tmp/x.json --json --port {port}"), &vars, "");
        app.join().unwrap();
        assert_eq!((json.err.as_str(), json.out.as_str()), ("", "{\"bytes\":5,\"path\":\"/tmp/x.json\"}\n"));
        // A command that does not ask the person never says it waits.
        let (port, app) = fake_app_after(answer(r#"{"data":{"scope":"read"},"ok":true}"#), Duration::from_millis(1300));
        assert_eq!(go(&format!("status --port {port}"), &vars, "").err, "");
        app.join().unwrap();
    }

    #[test]
    fn a_page_of_a_longer_list_says_where_the_rest_is() {
        let page = |total: u64, limit: u64, offset: u64, shown: usize| json!({ "total": total, "limit": limit, "offset": offset, "transactions": vec![json!({ "id": "1" }); shown] });
        assert_eq!(page_hint(&page(404, 4, 0, 4)).unwrap(), "showing 1-4 of 404; next page: --offset 4");
        assert_eq!(page_hint(&page(404, 50, 100, 50)).unwrap(), "showing 101-150 of 404; next page: --offset 150");
        assert_eq!(page_hint(&page(10, 4, 8, 2)), None, "the last page has nothing more");
        assert_eq!(page_hint(&page(4, 50, 0, 4)), None, "everything fit on one page");
        assert_eq!(page_hint(&page(0, 50, 0, 0)), None, "nothing at all");
        assert_eq!(page_hint(&page(10, 4, 40, 0)).unwrap(), "10 in all; --offset 40 is past the end");
        for not_a_page in [json!([1]), json!(null), json!({ "total": 5, "transactions": [] }), json!({ "total": "5", "limit": 1, "offset": 0, "x": [1] }), json!({ "total": 5, "limit": 1, "offset": 0 })] {
            assert_eq!(page_hint(&not_a_page), None, "{not_a_page}");
        }
    }

    #[test]
    fn the_hint_goes_to_standard_error_in_the_plain_form_only() {
        let body = r#"{"data":{"total":404,"limit":2,"offset":0,"transactions":[{"id":"1"},{"id":"2"}]},"ok":true}"#;
        let (port, app) = fake_app(answer(body));
        let plain = go(&format!("transactions list --limit 2 --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "");
        app.join().unwrap();
        assert_eq!(plain.err, "showing 1-2 of 404; next page: --offset 2\n");
        assert!(plain.out.contains("transactions") && !plain.out.contains("showing"), "data on standard output only");
        let (port, app) = fake_app(answer(body));
        let json = go(&format!("transactions list --limit 2 --json --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "");
        app.join().unwrap();
        assert_eq!(json.err, "", "a script gets no extra text");
    }

    #[test]
    fn a_text_flag_reads_standard_input_for_its_value() {
        let (port, app) = fake_app(answer(r#"{"data":{"transactions":[],"total":0,"limit":50,"offset":0},"ok":true}"#));
        let r = go(&format!("transactions list --q - --port {port}"), &[("MEOWNY_TOKEN", TOKEN)], "coffee shop\n");
        assert!(app.join().unwrap().ends_with("{\"q\":\"coffee shop\"}"));
        assert_eq!(r.code, 0);
        assert_eq!(r.out, "limit: 50\noffset: 0\ntotal: 0\n\ntransactions\n(none)\n");
    }

    #[test]
    fn login_and_logout_work_without_the_app() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("t");
        let vars = [("MEOWNY_TOKEN_FILE", file.to_str().unwrap())];
        let r = go("login", &vars, &format!("{TOKEN}\n"));
        assert_eq!((r.code, r.out.as_str()), (0, ""));
        assert!(r.err.starts_with("token saved to ") && !r.err.contains(TOKEN));
        let r = go("login --json", &vars, &format!("{TOKEN}\n"));
        assert_eq!(serde_json::from_str::<Value>(&r.out).unwrap()["saved"], true);
        // The saved token is now used: a request carries it.
        let (port, app) = fake_app(answer(r#"{"data":{"scope":"read"},"ok":true}"#));
        let r = go(&format!("status --port {port}"), &vars, "");
        assert!(app.join().unwrap().contains(&format!("Bearer {TOKEN}")), "{}", r.err);
        assert_eq!(go("login", &vars, "nonsense\n").code, 3);
        assert!(go("logout", &vars, "").err.starts_with("token removed from "));
        assert!(go("logout", &vars, "").err.starts_with("there was no saved token"));
        assert_eq!(serde_json::from_str::<Value>(&go("logout --json", &vars, "").out).unwrap()["removed"], false);
    }
}
