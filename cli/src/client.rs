//! Finding the running app, proving who is asking, and one HTTP request.
//!
//! The app writes the port it listens on to `api.json` in its data folder; this program reads it
//! from there (or takes `--port`). It only ever connects to `127.0.0.1`. The token comes from a
//! file given with `--token-file`, the `MEOWNY_TOKEN` variable, `MEOWNY_TOKEN_FILE`, or the saved
//! file `meowny login` writes, in that order. There is no `--token` flag on purpose: a command line
//! can be read by other users of the computer, an environment variable or a private file cannot.
//! A token file that other users can read is refused. The token is never printed, in any error.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::catalog::API_VERSION;
use crate::error::CliError;

/// The most this program will read as an answer.
const MAX_ANSWER: usize = 32 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Where the program looks things up: environment variables, passed in so tests need not set real ones.
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

fn var(env: Env, name: &str) -> Option<String> {
    env(name).filter(|v| !v.is_empty())
}

/// `mny_` and 64 lower-case hex characters.
pub fn has_token_shape(text: &str) -> bool {
    text.len() == 68 && text.strip_prefix("mny_").is_some_and(|rest| rest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

fn home(env: Env) -> Option<PathBuf> {
    var(env, "HOME").or_else(|| var(env, "USERPROFILE")).map(PathBuf::from)
}

/// The app's data folder, where `api.json` is.
pub fn data_dir(env: Env, flag: Option<&str>) -> Result<PathBuf, CliError> {
    if let Some(dir) = flag.map(str::to_string).or_else(|| var(env, "MEOWNY_DATA_DIR")) {
        return Ok(PathBuf::from(dir));
    }
    let missing = || CliError::new("app_unavailable", "cannot tell where the app keeps its data; give --data-dir or set MEOWNY_DATA_DIR");
    if cfg!(windows) {
        return var(env, "APPDATA").map(|d| Path::new(&d).join("org.meowny.app")).ok_or_else(missing);
    }
    if cfg!(target_os = "macos") {
        return home(env).map(|h| h.join("Library/Application Support/org.meowny.app")).ok_or_else(missing);
    }
    match var(env, "XDG_DATA_HOME") {
        Some(dir) => Ok(Path::new(&dir).join("org.meowny.app")),
        None => home(env).map(|h| h.join(".local/share/org.meowny.app")).ok_or_else(missing),
    }
}

/// Where `meowny login` keeps the token when no other place is given.
pub fn saved_token_path(env: Env) -> Result<PathBuf, CliError> {
    let missing = || CliError::new("no_token", "cannot tell where to keep a token; give --token-file or set MEOWNY_TOKEN_FILE");
    if cfg!(windows) {
        return var(env, "APPDATA").map(|d| Path::new(&d).join("meowny").join("token")).ok_or_else(missing);
    }
    if cfg!(target_os = "macos") {
        return home(env).map(|h| h.join("Library/Application Support/meowny/token")).ok_or_else(missing);
    }
    match var(env, "XDG_CONFIG_HOME") {
        Some(dir) => Ok(Path::new(&dir).join("meowny/token")),
        None => home(env).map(|h| h.join(".config/meowny/token")).ok_or_else(missing),
    }
}

/// The port the app listens on: `--port` or `MEOWNY_PORT`, else what the app wrote to `api.json`.
pub fn port(env: Env, flag_port: Option<u16>, flag_dir: Option<&str>) -> Result<u16, CliError> {
    if let Some(port) = flag_port {
        return Ok(port);
    }
    if let Some(text) = var(env, "MEOWNY_PORT") {
        return text.parse::<u16>().ok().filter(|p| *p > 0).ok_or_else(|| CliError::usage("MEOWNY_PORT must be a port number"));
    }
    let file = data_dir(env, flag_dir)?.join("api.json");
    let off = || {
        CliError::new(
            "app_unavailable",
            "the app is not running, or command line access is off (turn it on in Settings, command line access)",
        )
    };
    let text = std::fs::read_to_string(&file).map_err(|_| off())?;
    let info: Value = serde_json::from_str(&text).map_err(|_| CliError::new("app_unavailable", "the app's port file is not readable"))?;
    match info.get("api").and_then(Value::as_u64) {
        Some(API_VERSION) => {}
        Some(other) => {
            return Err(CliError::new("app_unavailable", format!("the app speaks API {other} and this program speaks API {API_VERSION}; update the older one")))
        }
        None => return Err(CliError::new("app_unavailable", "the app's port file is not readable")),
    }
    info.get("port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p > 0)
        .ok_or_else(|| CliError::new("app_unavailable", "the app's port file is not readable"))
}

/// The first line of a token file, if the file is private to its owner.
fn read_token_file(path: &Path) -> Result<String, CliError> {
    let file = std::fs::File::open(path)
        .map_err(|_| CliError::new("no_token", format!("cannot read the token file {}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = file.metadata().map_err(|_| CliError::new("no_token", "cannot read the token file"))?.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(CliError::new(
                "token_file_unsafe",
                format!("the token file {} can be read by other users; run: chmod 600 {}", path.display(), path.display()),
            ));
        }
    }
    let mut text = String::new();
    file.take(4096).read_to_string(&mut text).map_err(|_| CliError::new("no_token", "the token file is not text"))?;
    Ok(text.lines().next().unwrap_or("").trim().to_string())
}

/// The token to use, from the first place that has one. It is checked for its shape only.
pub fn token(env: Env, flag_file: Option<&str>) -> Result<String, CliError> {
    let found = if let Some(file) = flag_file {
        read_token_file(Path::new(file))?
    } else if let Some(token) = var(env, "MEOWNY_TOKEN") {
        token.trim().to_string()
    } else if let Some(file) = var(env, "MEOWNY_TOKEN_FILE") {
        read_token_file(Path::new(&file))?
    } else {
        let path = saved_token_path(env)?;
        if !path.exists() {
            return Err(CliError::new(
                "no_token",
                "no token: set MEOWNY_TOKEN, give --token-file, or save one with: meowny login  (make a token in the app, Settings, command line access)",
            ));
        }
        read_token_file(&path)?
    };
    if !has_token_shape(&found) {
        return Err(CliError::new("bad_token", "the token is not in the right form (mny_ and 64 letters and digits); copy it again from the app"));
    }
    Ok(found)
}

/// What the app said: its status code and its JSON, or why that could not be read.
pub fn parse_answer(bytes: &[u8]) -> Result<(u16, Value), CliError> {
    let bad = || CliError::new("bad_answer", "the app gave an answer this program cannot read");
    let split = bytes.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(bad)?;
    let head = std::str::from_utf8(&bytes[..split]).map_err(|_| bad())?;
    let body = &bytes[split + 4..];
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .filter(|l| l.starts_with("HTTP/1."))
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(bad)?;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") && value.trim().parse::<usize>().ok() != Some(body.len()) {
                return Err(bad());
            }
        }
    }
    Ok((status, serde_json::from_slice(body).map_err(|_| bad())?))
}

/// Turns the app's JSON into the data, or into the error it describes.
pub fn unwrap_answer(answer: &Value) -> Result<Value, CliError> {
    match answer.get("ok").and_then(Value::as_bool) {
        Some(true) => Ok(answer.get("data").cloned().unwrap_or(Value::Null)),
        Some(false) => {
            let error = answer.get("error");
            let field = |key: &str| error.and_then(|e| e.get(key)).and_then(Value::as_str);
            Err(CliError::new(field("code").unwrap_or("internal"), field("message").unwrap_or("the app refused the request")))
        }
        None => Err(CliError::new("bad_answer", "the app gave an answer this program cannot read")),
    }
}

/// One request to one route. Returns what the route returned.
pub fn call(port: u16, token: &str, route: &str, body: &Value, timeout: Duration) -> Result<Value, CliError> {
    let payload = serde_json::to_vec(body).map_err(|_| CliError::new("internal", "cannot encode the request"))?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let unavailable = || CliError::new("app_unavailable", format!("cannot connect to the app on port {port}; is it running with command line access on?"));
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|_| unavailable())?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let mut request = format!(
        "POST /v1/{route} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    )
    .into_bytes();
    request.extend_from_slice(&payload);
    let slow = |_| CliError::new("timeout", format!("the app did not answer within {} seconds", timeout.as_secs()));
    stream.write_all(&request).map_err(slow)?;

    let mut answer = Vec::new();
    stream.take(MAX_ANSWER as u64 + 1).read_to_end(&mut answer).map_err(|e| match e.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => slow(e),
        _ => CliError::new("app_unavailable", "the connection to the app broke"),
    })?;
    if answer.len() > MAX_ANSWER {
        return Err(CliError::new("bad_answer", "the app's answer is too large"));
    }
    let (_, json) = parse_answer(&answer)?;
    unwrap_answer(&json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const TOKEN: &str = "mny_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn a_token_has_a_fixed_shape() {
        assert!(has_token_shape(TOKEN));
        for bad in ["", "mny_", &TOKEN[..67], &format!("{TOKEN}0"), &TOKEN.to_uppercase(), &TOKEN.replace("mny_", "xxx_"), &format!(" {TOKEN}"), &format!("mny_{}", "g".repeat(64))] {
            assert!(!has_token_shape(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_data_folder_follows_the_flag_then_the_variable_then_the_platform() {
        assert_eq!(data_dir(&env(&[("MEOWNY_DATA_DIR", "/env")]), Some("/flag")).unwrap(), PathBuf::from("/flag"));
        assert_eq!(data_dir(&env(&[("MEOWNY_DATA_DIR", "/env")]), None).unwrap(), PathBuf::from("/env"));
        if cfg!(target_os = "linux") {
            assert_eq!(data_dir(&env(&[("HOME", "/home/a")]), None).unwrap(), PathBuf::from("/home/a/.local/share/org.meowny.app"));
            assert_eq!(data_dir(&env(&[("HOME", "/home/a"), ("XDG_DATA_HOME", "/data")]), None).unwrap(), PathBuf::from("/data/org.meowny.app"));
            assert_eq!(data_dir(&env(&[("HOME", "/home/a"), ("XDG_DATA_HOME", "")]), None).unwrap(), PathBuf::from("/home/a/.local/share/org.meowny.app"), "an empty variable counts as unset");
        }
        assert_eq!(data_dir(&env(&[]), None).unwrap_err().code, "app_unavailable");
    }

    #[test]
    fn the_saved_token_lives_in_the_config_folder() {
        if cfg!(target_os = "linux") {
            assert_eq!(saved_token_path(&env(&[("HOME", "/home/a")])).unwrap(), PathBuf::from("/home/a/.config/meowny/token"));
            assert_eq!(saved_token_path(&env(&[("XDG_CONFIG_HOME", "/cfg")])).unwrap(), PathBuf::from("/cfg/meowny/token"));
        }
        assert_eq!(saved_token_path(&env(&[])).unwrap_err().code, "no_token");
    }

    #[test]
    fn the_port_comes_from_the_flag_the_variable_or_the_file_the_app_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("api.json"), r#"{"api":1,"port":4321}"#).unwrap();
        assert_eq!(port(&env(&[]), Some(9), Some(path)).unwrap(), 9, "the flag wins");
        assert_eq!(port(&env(&[("MEOWNY_PORT", "7")]), None, Some(path)).unwrap(), 7);
        assert_eq!(port(&env(&[]), None, Some(path)).unwrap(), 4321);
        assert_eq!(port(&env(&[("MEOWNY_DATA_DIR", path)]), None, None).unwrap(), 4321);
        assert_eq!(port(&env(&[("MEOWNY_PORT", "abc")]), None, Some(path)).unwrap_err().code, "usage");
        assert_eq!(port(&env(&[("MEOWNY_PORT", "0")]), None, Some(path)).unwrap_err().code, "usage");
    }

    #[test]
    fn a_missing_or_odd_port_file_says_the_app_is_not_there() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        let missing = port(&env(&[]), None, Some(path)).unwrap_err();
        assert_eq!(missing.code, "app_unavailable");
        assert!(missing.message.contains("not running") && missing.message.contains("Settings"));
        for (text, why) in [
            ("not json", "not readable"),
            ("{}", "not readable"),
            (r#"{"api":1}"#, "not readable"),
            (r#"{"api":1,"port":0}"#, "not readable"),
            (r#"{"api":1,"port":70000}"#, "not readable"),
            (r#"{"api":1,"port":"80"}"#, "not readable"),
            (r#"{"api":2,"port":80}"#, "API 2"),
        ] {
            std::fs::write(dir.path().join("api.json"), text).unwrap();
            let e = port(&env(&[]), None, Some(path)).unwrap_err();
            assert_eq!(e.code, "app_unavailable", "{text}");
            assert!(e.message.contains(why), "{text}: {}", e.message);
        }
    }

    fn private_file(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        path
    }

    #[test]
    fn the_token_comes_from_a_flag_file_then_the_variable_then_a_file_variable_then_the_saved_file() {
        let dir = tempfile::tempdir().unwrap();
        let other = format!("mny_{}", "ab".repeat(32));
        let a = private_file(dir.path(), "a", &format!("{TOKEN}\n"));
        let b = private_file(dir.path(), "b", &format!("{other}\n"));
        let saved_dir = dir.path().join("cfg/meowny");
        std::fs::create_dir_all(&saved_dir).unwrap();
        let saved = private_file(&saved_dir, "token", &format!("mny_{}\n", "cd".repeat(32)));
        let cfg = dir.path().join("cfg");
        let all = env(&[("MEOWNY_TOKEN", &other), ("MEOWNY_TOKEN_FILE", b.to_str().unwrap()), ("XDG_CONFIG_HOME", cfg.to_str().unwrap())]);

        assert_eq!(token(&all, Some(a.to_str().unwrap())).unwrap(), TOKEN, "--token-file beats the variable");
        assert_eq!(token(&all, None).unwrap(), other, "the variable beats the file variable");
        let no_var = env(&[("MEOWNY_TOKEN_FILE", a.to_str().unwrap()), ("XDG_CONFIG_HOME", cfg.to_str().unwrap())]);
        assert_eq!(token(&no_var, None).unwrap(), TOKEN);
        if cfg!(target_os = "linux") {
            let only_saved = env(&[("XDG_CONFIG_HOME", cfg.to_str().unwrap())]);
            assert_eq!(token(&only_saved, None).unwrap(), format!("mny_{}", "cd".repeat(32)));
        }
        let _ = saved;
    }

    #[test]
    fn no_token_anywhere_says_how_to_give_one_without_naming_one() {
        let dir = tempfile::tempdir().unwrap();
        let e = token(&env(&[("XDG_CONFIG_HOME", dir.path().to_str().unwrap())]), None).unwrap_err();
        assert_eq!(e.code, "no_token");
        assert!(e.message.contains("MEOWNY_TOKEN") && e.message.contains("--token-file") && e.message.contains("meowny login"));
        assert_eq!(token(&env(&[("MEOWNY_TOKEN", "")]), Some("/no/such/file")).unwrap_err().code, "no_token");
    }

    #[test]
    fn a_token_that_is_not_the_right_shape_is_refused_before_it_is_sent_and_never_shown() {
        for bad in ["hello", "mny_short", &format!("{TOKEN}00"), "Bearer abc"] {
            let e = token(&env(&[("MEOWNY_TOKEN", bad)]), None).unwrap_err();
            assert_eq!(e.code, "bad_token", "{bad}");
            assert!(!e.message.contains(bad) || bad.is_empty(), "the message repeats the token: {}", e.message);
        }
        assert_eq!(token(&env(&[("MEOWNY_TOKEN", &format!("  {TOKEN}\n"))]), None).unwrap(), TOKEN, "white space around it is dropped");
    }

    #[test]
    fn only_the_first_line_of_a_token_file_counts() {
        let dir = tempfile::tempdir().unwrap();
        let file = private_file(dir.path(), "t", &format!("  {TOKEN}  \nsecond line\n"));
        assert_eq!(token(&env(&[]), Some(file.to_str().unwrap())).unwrap(), TOKEN);
        let empty = private_file(dir.path(), "e", "");
        assert_eq!(token(&env(&[]), Some(empty.to_str().unwrap())).unwrap_err().code, "bad_token");
        let huge = private_file(dir.path(), "h", &"x".repeat(100_000));
        assert_eq!(token(&env(&[]), Some(huge.to_str().unwrap())).unwrap_err().code, "bad_token");
    }

    #[cfg(unix)]
    #[test]
    fn a_token_file_that_others_can_read_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = private_file(dir.path(), "t", TOKEN);
        for mode in [0o640, 0o604, 0o644, 0o660, 0o666, 0o777] {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
            let e = token(&env(&[]), Some(file.to_str().unwrap())).unwrap_err();
            assert_eq!(e.code, "token_file_unsafe", "{mode:o}");
            assert!(e.message.contains("chmod 600") && !e.message.contains(TOKEN));
        }
        for mode in [0o600, 0o400] {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(token(&env(&[]), Some(file.to_str().unwrap())).unwrap(), TOKEN, "{mode:o}");
        }
    }

    #[test]
    fn an_answer_is_read_as_a_status_and_json() {
        let ok = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n{\"data\":{\"a\":1},\"ok\":true}";
        let body = br#"{"data":{"a":1},"ok":true}"#;
        let answer = [format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", body.len()).as_bytes(), body].concat();
        let (status, json) = parse_answer(&answer).unwrap();
        assert_eq!((status, json["data"]["a"].clone()), (200, serde_json::json!(1)));
        assert!(parse_answer(ok).is_err(), "a length that does not match is not trusted");
        for bad in [&b""[..], b"HTTP/1.1 200 OK", b"HTTP/1.1 200 OK\r\n\r\nnot json", b"garbage\r\n\r\n{}", b"HTTP/1.1 abc OK\r\n\r\n{}", b"\xff\xfe\r\n\r\n{}"] {
            assert_eq!(parse_answer(bad).unwrap_err().code, "bad_answer", "{bad:?}");
        }
    }

    #[test]
    fn the_apps_json_becomes_data_or_the_error_it_describes() {
        use serde_json::json;
        assert_eq!(unwrap_answer(&json!({ "ok": true, "data": [1, 2] })).unwrap(), json!([1, 2]));
        assert_eq!(unwrap_answer(&json!({ "ok": true })).unwrap(), Value::Null);
        let e = unwrap_answer(&json!({ "ok": false, "error": { "code": "locked", "message": "the app is locked" } })).unwrap_err();
        assert_eq!((e.code.as_str(), e.message.as_str()), ("locked", "the app is locked"));
        assert_eq!(unwrap_answer(&json!({ "ok": false })).unwrap_err().code, "internal");
        assert_eq!(unwrap_answer(&json!({ "ok": false, "error": { "code": 5 } })).unwrap_err().code, "internal");
        assert_eq!(unwrap_answer(&json!({ "data": 1 })).unwrap_err().code, "bad_answer");
        assert_eq!(unwrap_answer(&json!("ok")).unwrap_err().code, "bad_answer");
    }
}
