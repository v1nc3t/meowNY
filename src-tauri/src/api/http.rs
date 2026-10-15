//! Just enough HTTP/1.1 for the command API: one request per connection, a JSON body, no chunked
//! bodies, no keep-alive. Strict on purpose: anything odd is a 400, so a request is read the same
//! way by this code and by anything that might sit in front of it.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant};

use super::ApiError;

/// Request line and headers together.
pub const MAX_HEAD: usize = 16 * 1024;
pub const MAX_BODY: usize = 1024 * 1024;
const MAX_TARGET: usize = 200;
/// Headers that may appear once. A repeated one is how two parsers end up disagreeing.
const SINGLE: &[&str] = &["host", "content-length", "authorization", "origin", "expect"];
/// After an early answer (a 413, say) the rest of what the client sends is read and thrown away
/// for a moment, so closing does not reset the connection before the client has read the answer.
const DRAIN_FOR: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub target: String,
    headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// `name` in lower case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug)]
pub enum ReadError {
    /// The client went away or ran out of time. There is nobody to answer.
    Dropped,
    /// The request is bad; the client gets this answer.
    Refused(ApiError),
}

impl From<ApiError> for ReadError {
    fn from(e: ApiError) -> Self {
        ReadError::Refused(e)
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Appends whatever arrives next. Fails when the deadline has passed or the client has gone.
fn fill(stream: &mut TcpStream, buf: &mut Vec<u8>, deadline: Instant) -> Result<(), ReadError> {
    let left = deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or(ReadError::Dropped)?;
    stream.set_read_timeout(Some(left)).map_err(|_| ReadError::Dropped)?;
    let mut chunk = [0u8; 4096];
    match stream.read(&mut chunk) {
        Ok(0) => Err(ReadError::Dropped),
        Ok(n) => {
            buf.extend_from_slice(&chunk[..n]);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => Ok(()),
        Err(_) => Err(ReadError::Dropped),
    }
}

pub fn read_request(stream: &mut TcpStream, deadline: Instant) -> Result<Request, ReadError> {
    let mut buf = Vec::with_capacity(1024);
    let head_end = loop {
        if let Some(at) = find(&buf, b"\r\n\r\n") {
            break at;
        }
        if buf.len() > MAX_HEAD {
            return Err(ApiError::TOO_LARGE.into());
        }
        fill(stream, &mut buf, deadline)?;
    };
    if head_end > MAX_HEAD {
        return Err(ApiError::TOO_LARGE.into());
    }
    let head = std::str::from_utf8(&buf[..head_end]).map_err(|_| ApiError::invalid("the request is not text"))?;
    let mut request = parse_head(head)?;

    let length = content_length(&request)?;
    if length > MAX_BODY {
        return Err(ApiError::TOO_LARGE.into());
    }
    if length > 0 && request.header("expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue")) {
        stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").map_err(|_| ReadError::Dropped)?;
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        fill(stream, &mut body, deadline)?;
    }
    // Bytes past the declared length are not part of this request.
    body.truncate(length);
    request.body = body;
    Ok(request)
}

fn content_length(request: &Request) -> Result<usize, ApiError> {
    match request.header("content-length") {
        None => Ok(0),
        Some(v) if !v.is_empty() && v.len() <= 10 && v.bytes().all(|b| b.is_ascii_digit()) => {
            v.parse().map_err(|_| ApiError::invalid("bad Content-Length"))
        }
        Some(_) => Err(ApiError::invalid("bad Content-Length")),
    }
}

fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// The request line and the headers, without the body.
pub fn parse_head(head: &str) -> Result<Request, ApiError> {
    let bad = || ApiError::invalid("malformed request");
    let mut lines = head.split("\r\n");
    let mut parts = lines.next().unwrap_or("").split(' ');
    let (Some(method), Some(target), Some(version), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad());
    };
    if !matches!(version, "HTTP/1.1" | "HTTP/1.0")
        || method.is_empty()
        || method.len() > 16
        || !method.bytes().all(|b| b.is_ascii_uppercase())
        // Origin form only: a path. No absolute URLs, no `*`.
        || !target.starts_with('/')
        || target.len() > MAX_TARGET
        || !target.bytes().all(|b| (0x21..=0x7e).contains(&b))
    {
        return Err(bad());
    }

    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        // A line that starts with white space continues the one before it (obsolete folding). Refused.
        let (name, value) = line.split_once(':').ok_or_else(bad)?;
        if name.is_empty() || !name.bytes().all(is_token_char) {
            return Err(bad());
        }
        let value = value.trim_matches(|c| c == ' ' || c == '\t');
        if value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
            return Err(bad());
        }
        let name = name.to_ascii_lowercase();
        if SINGLE.contains(&name.as_str()) && headers.iter().any(|(n, _)| *n == name) {
            return Err(ApiError::invalid("a header is repeated"));
        }
        headers.push((name, value.to_string()));
    }
    if headers.iter().any(|(n, _)| n == "transfer-encoding") {
        return Err(ApiError::invalid("chunked bodies are not supported; send Content-Length"));
    }
    Ok(Request { method: method.to_string(), target: target.to_string(), headers, body: Vec::new() })
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Content Too Large",
        423 => "Locked",
        500 => "Internal Server Error",
        _ => "Error",
    }
}

/// Sends one JSON answer and closes. There are no CORS headers on purpose: a web page must not be
/// able to read an answer.
pub fn respond(stream: &mut TcpStream, status: u16, extra: &[(&str, &str)], body: &[u8]) {
    let mut out = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
        reason(status),
        body.len()
    );
    for (name, value) in extra {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(body);
    let _ = stream.write_all(&bytes).and_then(|_| stream.flush());
    let _ = stream.shutdown(Shutdown::Write);

    let end = Instant::now() + DRAIN_FOR;
    let mut chunk = [0u8; 8192];
    while let Some(left) = end.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()) {
        if stream.set_read_timeout(Some(left)).is_err() || !matches!(stream.read(&mut chunk), Ok(n) if n > 0) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(head: &str) -> Request {
        parse_head(head).unwrap_or_else(|e| panic!("{head:?}: {e:?}"))
    }

    #[test]
    fn a_plain_request_is_parsed() {
        let r = ok("POST /v1/status HTTP/1.1\r\nHost: 127.0.0.1:5\r\nAuthorization:   Bearer abc \r\nX-Thing:\tvalue");
        assert_eq!((r.method.as_str(), r.target.as_str()), ("POST", "/v1/status"));
        assert_eq!(r.header("host"), Some("127.0.0.1:5"));
        assert_eq!(r.header("authorization"), Some("Bearer abc"), "white space around a value is dropped");
        assert_eq!(r.header("x-thing"), Some("value"));
        assert_eq!(r.header("missing"), None);
        ok("POST /v1/status HTTP/1.0\r\nHost: x");
    }

    #[test]
    fn header_names_are_case_insensitive() {
        let r = ok("POST /v1/status HTTP/1.1\r\nHOST: a\r\nAuthORIZATION: b");
        assert_eq!((r.header("host"), r.header("authorization")), (Some("a"), Some("b")));
    }

    #[test]
    fn malformed_requests_are_refused() {
        for head in [
            "",
            "POST",
            "POST /v1/status",
            "POST /v1/status HTTP/1.1 extra",
            "POST  /v1/status HTTP/1.1",
            "post /v1/status HTTP/1.1",
            "PO ST /v1/status HTTP/1.1",
            "POST /v1/status HTTP/2.0",
            "POST /v1/status HTTP/1.2",
            "POST /v1/status http/1.1",
            "PRI * HTTP/2.0",
            "POST * HTTP/1.1",
            "POST http://127.0.0.1/v1/status HTTP/1.1",
            "POST v1/status HTTP/1.1",
            "POST /v1/status\u{0} HTTP/1.1",
            "POST /v1/ä HTTP/1.1",
            "POST /v1/status HTTP/1.1\nHost: a",
            "POST /v1/status HTTP/1.1\r\nno colon here",
            "POST /v1/status HTTP/1.1\r\n: empty name",
            "POST /v1/status HTTP/1.1\r\nBad Name: x",
            "POST /v1/status HTTP/1.1\r\nHost: a\r\n folded",
            "POST /v1/status HTTP/1.1\r\nHost: a\r\n\tfolded",
            "POST /v1/status HTTP/1.1\r\nHost: a\u{0}b",
            "POST /v1/status HTTP/1.1\r\nHost: a\rb",
            "POST /v1/status HTTP/1.1\r\nHost: a\u{7f}",
            "POST /v1/status HTTP/1.1\r\nTransfer-Encoding: chunked",
            "POST /v1/status HTTP/1.1\r\nTransfer-Encoding: identity",
        ] {
            assert!(parse_head(head).is_err(), "{head:?} was accepted");
        }
        let long = format!("POST /{} HTTP/1.1", "a".repeat(MAX_TARGET));
        assert!(parse_head(&long).is_err());
    }

    #[test]
    fn a_header_that_must_appear_once_cannot_repeat() {
        for name in ["Host", "Content-Length", "Authorization", "Origin", "Expect"] {
            let head = format!("POST /v1/status HTTP/1.1\r\n{name}: 1\r\n{}: 2", name.to_ascii_lowercase());
            assert!(parse_head(&head).is_err(), "{name}");
        }
        // Others may repeat.
        ok("POST /v1/status HTTP/1.1\r\nX-A: 1\r\nX-A: 2");
    }

    #[test]
    fn content_length_is_digits_only_and_small() {
        let with = |v: &str| content_length(&ok(&format!("POST /v1/x HTTP/1.1\r\nContent-Length: {v}")));
        assert_eq!(with("0"), Ok(0));
        assert_eq!(with("17"), Ok(17));
        assert_eq!(with("0017"), Ok(17));
        for bad in ["", "-1", "+5", "1 2", "0x10", "1e3", "abc", "12345678901", "99999999999999999999"] {
            assert!(with(bad).is_err(), "{bad:?}");
        }
        assert_eq!(content_length(&ok("POST /v1/x HTTP/1.1\r\nHost: a")), Ok(0), "no body");
    }

    #[test]
    fn status_lines_have_reasons() {
        for status in [200, 400, 401, 403, 404, 405, 409, 413, 423, 500] {
            assert_ne!(reason(status), "Error", "{status}");
        }
    }
}
