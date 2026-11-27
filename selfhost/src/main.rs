//! `meowny-server`: meowNY served to a browser over HTTP, for one person who hosts it themselves (docs/self-host.md).
//!
//! It is the desktop app without the window. The page is the same, built from `ui/`; the commands are the same, run through
//! Tauri's mock runtime by name, exactly as the window's page calls them. What differs:
//! - the data's secret is in a file beside it and the passphrase is required, since there is no keychain (`Policy::SERVER`);
//! - the passphrase is also the login. Opening the data gives the browser a session cookie, and every lock ends every session;
//! - file dialogs become downloads and uploads (`meowny_lib::served`);
//! - the phone sync, the command line API, notifications and the system's file viewer are not offered.
//!
//! Routes: `POST /api/invoke/<command>` (a JSON object of arguments), `POST /api/upload` (raw bytes, before a command that reads a
//! file), `GET /api/download/<id>` (a file a command wrote), `GET /healthz`, and the page's files for everything else.
//!
//! It logs no requests and no data, like the app. It is its own package (`selfhost/`), built apart from the desktop app.

mod app;
mod files;

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use app::{App, Outcome};

/// The largest JSON body of a command. A document is sent in base64 chunks well under this.
const MAX_JSON_BODY: u64 = 32 * 1024 * 1024;
const IDLE_CHECK_EVERY: Duration = Duration::from_secs(5);
const SCHEDULE_CHECK_EVERY: Duration = Duration::from_secs(60);

struct Config {
    listen: String,
    data: PathBuf,
    backups: PathBuf,
    web: PathBuf,
    max_upload: u64,
    threads: usize,
    /// None: Secure only when a proxy says the browser came over https.
    secure_cookie: Option<bool>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    fn from_env() -> Result<Config, String> {
        let data = PathBuf::from(env("MEOWNY_DATA_DIR").unwrap_or_else(|| "/data".into()));
        let backups = env("MEOWNY_BACKUP_DIR").map(PathBuf::from).unwrap_or_else(|| data.join("backups"));
        let number = |name: &str, default: u64| -> Result<u64, String> {
            env(name).map_or(Ok(default), |v| v.trim().parse().map_err(|_| format!("{name} must be a whole number")))
        };
        let secure_cookie = match env("MEOWNY_COOKIE_SECURE").as_deref().map(str::to_ascii_lowercase).as_deref() {
            None | Some("auto") => None,
            Some("true" | "1" | "yes") => Some(true),
            Some("false" | "0" | "no") => Some(false),
            Some(_) => return Err("MEOWNY_COOKIE_SECURE must be auto, true or false".into()),
        };
        Ok(Config {
            listen: env("MEOWNY_LISTEN").unwrap_or_else(|| "127.0.0.1:8080".into()),
            data,
            backups,
            web: PathBuf::from(env("MEOWNY_WEB_DIR").unwrap_or_else(|| "/app/web".into())),
            max_upload: number("MEOWNY_MAX_UPLOAD_MB", 1024)?.saturating_mul(1024 * 1024),
            threads: number("MEOWNY_THREADS", 8)?.clamp(2, 64) as usize,
            secure_cookie,
        })
    }
}

fn main() {
    // Panic messages can quote data, so only the place is printed (as the desktop app does).
    std::panic::set_hook(Box::new(|info| match info.location() {
        Some(at) => eprintln!("meowny-server panicked at {}:{}", at.file(), at.line()),
        None => eprintln!("meowny-server panicked"),
    }));
    if let Err(message) = run() {
        eprintln!("meowny-server: {message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let config = Config::from_env()?;
    if !config.web.join("index.html").is_file() {
        return Err(format!("no index.html in {} (set MEOWNY_WEB_DIR to the built ui/dist)", config.web.display()));
    }
    files::private_dir(&config.data).map_err(|e| format!("cannot use the data folder {}: {e}", config.data.display()))?;
    files::private_dir(&config.backups).map_err(|e| format!("cannot use the backup folder {}: {e}", config.backups.display()))?;
    // Files on their way to or from a browser. What a crash left behind there goes now: exports are not encrypted.
    let staging = std::env::temp_dir().join("meowny-server");
    let _ = std::fs::remove_dir_all(&staging);
    files::private_dir(&staging).map_err(|e| format!("cannot make the staging folder: {e}"))?;
    meowny_lib::served::install(staging.clone(), config.backups.clone());

    let (mock, app) = App::start(config.data.clone(), files::Staged::new(staging))?;
    let app = Arc::new(app);
    let server = Arc::new(Server::http(&config.listen).map_err(|e| format!("cannot listen on {}: {e}", config.listen))?);
    let config = Arc::new(config);
    println!("meowNY server on http://{} (data in {}, backups in {})", config.listen, config.data.display(), config.backups.display());

    // The idle lock, which ends every session, and the clean-up of files nobody fetched.
    let idle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(IDLE_CHECK_EVERY);
        idle.lock_if_idle();
        idle.staged.expire(Instant::now());
    });
    // Scheduled backups, while the data is open.
    let scheduled = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(SCHEDULE_CHECK_EVERY);
        scheduled.run_scheduled_backup();
    });

    let workers: Vec<_> = (0..config.threads)
        .map(|_| {
            let (server, app, config) = (server.clone(), app.clone(), config.clone());
            std::thread::spawn(move || {
                while let Ok(request) = server.recv() {
                    handle(&app, &config, request);
                }
            })
        })
        .collect();
    for worker in workers {
        let _ = worker.join();
    }
    drop(mock);
    Ok(())
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes().to_vec(), value.as_bytes().to_vec()).expect("a valid header")
}

fn request_header<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

/// Headers every answer carries: the page may only load itself, and may not be framed.
fn with_safety<R: Read>(response: Response<R>) -> Response<R> {
    const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; \
        font-src 'self'; connect-src 'self'; worker-src 'self' blob:; object-src 'none'; base-uri 'none'; form-action 'self'; \
        frame-ancestors 'none'";
    response
        .with_header(header("Content-Security-Policy", CSP))
        .with_header(header("X-Content-Type-Options", "nosniff"))
        .with_header(header("X-Frame-Options", "DENY"))
        .with_header(header("Referrer-Policy", "no-referrer"))
        .with_header(header("Cross-Origin-Opener-Policy", "same-origin"))
}

fn respond<R: Read>(request: Request, response: Response<R>) {
    let _ = request.respond(with_safety(response));
}

fn json_response(status: u16, body: &Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(serde_json::to_vec(body).unwrap_or_default())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json"))
        .with_header(header("Cache-Control", "no-store"))
}

fn failure(request: Request, status: u16, code: &str) {
    respond(request, json_response(status, &json!({ "ok": false, "error": code })));
}

fn handle(app: &App, config: &Config, request: Request) {
    let url = request.url().to_string();
    let path = url.split(['?', '#']).next().unwrap_or("/").to_string();
    let method = request.method().clone();

    if path == "/healthz" {
        return respond(request, Response::from_string("ok").with_header(header("Cache-Control", "no-store")));
    }
    if let Some(rest) = path.strip_prefix("/api/") {
        return api(app, config, request, &method, rest);
    }
    match method {
        Method::Get | Method::Head => files::serve_page(&config.web, &path, request, respond),
        _ => failure(request, 405, "method_not_allowed"),
    }
}

fn api(app: &App, config: &Config, request: Request, method: &Method, rest: &str) {
    let session = files::cookie(&request, app::COOKIE).map(str::to_string);

    if let (Method::Get, Some(id)) = (method, rest.strip_prefix("download/")) {
        if !app.signed_in(session.as_deref()) {
            return failure(request, 401, "locked");
        }
        return match app.staged.take_download(id) {
            Some(download) => files::send_download(request, download, respond),
            None => failure(request, 404, "not_found"),
        };
    }
    if *method != Method::Post {
        return failure(request, 405, "method_not_allowed");
    }
    // A page on another site cannot send this header without the browser asking first, and the answer to that is no. With the
    // SameSite cookie, a request from anywhere but this page is refused.
    if request_header(&request, "X-Meowny") != Some("1") {
        return failure(request, 403, "forbidden");
    }

    if rest == "upload" {
        if !app.may_upload(session.as_deref()) {
            return failure(request, 401, "locked");
        }
        let mut request = request;
        return match app.staged.receive(request.as_reader(), config.max_upload) {
            Ok(id) => respond(request, json_response(200, &json!({ "ok": true, "data": id }))),
            Err(files::UploadError::TooLarge) => failure(request, 413, "too_large"),
            Err(files::UploadError::Io) => failure(request, 500, "internal"),
        };
    }

    let Some(command) = rest.strip_prefix("invoke/") else { return failure(request, 404, "not_found") };
    let command = command.to_string();
    let upload = request_header(&request, "X-Meowny-Upload").map(str::to_string);
    let https = request_header(&request, "X-Forwarded-Proto").is_some_and(|p| p.eq_ignore_ascii_case("https"));
    let mut request = request;
    if request.body_length().is_some_and(|n| n as u64 > MAX_JSON_BODY) {
        return failure(request, 413, "too_large");
    }
    let mut body = Vec::new();
    if request.as_reader().take(MAX_JSON_BODY + 1).read_to_end(&mut body).is_err() || body.len() as u64 > MAX_JSON_BODY {
        return failure(request, 413, "too_large");
    }
    let args = if body.iter().all(u8::is_ascii_whitespace) {
        Value::Object(Default::default())
    } else {
        match serde_json::from_slice::<Value>(&body) {
            Ok(value @ Value::Object(_)) => value,
            _ => return failure(request, 400, "invalid_input"),
        }
    };

    let Outcome { result, new_session, download } = app.invoke(&command, args, session.as_deref(), upload.as_deref());
    let mut response = match result {
        Ok(data) => {
            let mut body = json!({ "ok": true, "data": data });
            if let Some((id, name)) = download {
                body["download"] = json!({ "id": id, "name": name });
            }
            json_response(200, &body)
        }
        Err(code) => json_response(200, &json!({ "ok": false, "error": code })),
    };
    if let Some(token) = new_session {
        let secure = config.secure_cookie.unwrap_or(https);
        let cookie = format!("{}={token}; Path=/; HttpOnly; SameSite=Strict{}", app::COOKIE, if secure { "; Secure" } else { "" });
        response = response.with_header(header("Set-Cookie", &cookie));
    }
    respond(request, response);
}
