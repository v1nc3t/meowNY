//! Files the server hands out or takes in: the page itself, uploads on their way to a command, and downloads a command wrote.
//!
//! Uploads and downloads live in a staging folder of their own, readable by the server only, and each can be used once. What
//! nobody fetched is removed after a quarter of an hour (exports are not encrypted, so they do not stay).

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use meowny_lib::served::{self, Download};
use tiny_http::{Request, Response};

use crate::header;

const KEEP_FOR: Duration = Duration::from_secs(15 * 60);

/// Makes the folder if it is missing, readable by the server's user only.
pub fn private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn cookie<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .filter(|h| h.field.equiv("Cookie"))
        .flat_map(|h| h.value.as_str().split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}

#[derive(Debug)]
pub enum UploadError {
    TooLarge,
    Io,
}

struct Entry {
    path: PathBuf,
    name: String,
    at: Instant,
}

pub struct Staged {
    dir: PathBuf,
    uploads: Mutex<HashMap<String, Entry>>,
    downloads: Mutex<HashMap<String, Entry>>,
}

fn remove(entry: Entry) {
    let _ = fs::remove_file(entry.path);
}

impl Staged {
    pub fn new(dir: PathBuf) -> Staged {
        Staged { dir, uploads: Mutex::default(), downloads: Mutex::default() }
    }

    /// Writes what the browser sends to a new file, up to `limit` bytes, and returns its id.
    pub fn receive(&self, body: &mut dyn Read, limit: u64) -> Result<String, UploadError> {
        let id = served::random_name().map_err(|_| UploadError::Io)?;
        let path = self.dir.join(format!("upload-{id}"));
        let written = (|| -> Result<(), UploadError> {
            let mut file = File::create(&path).map_err(|_| UploadError::Io)?;
            let copied = io::copy(&mut body.take(limit + 1), &mut file).map_err(|_| UploadError::Io)?;
            if copied > limit {
                return Err(UploadError::TooLarge);
            }
            file.flush().map_err(|_| UploadError::Io)
        })();
        if let Err(e) = written {
            let _ = fs::remove_file(&path);
            return Err(e);
        }
        let entry = Entry { path, name: String::new(), at: Instant::now() };
        self.uploads.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), entry);
        Ok(id)
    }

    pub fn take_upload(&self, id: &str) -> Option<PathBuf> {
        self.uploads.lock().unwrap_or_else(|e| e.into_inner()).remove(id).map(|entry| entry.path)
    }

    /// Keeps a file a command wrote until the browser fetches it. Its id and name, for the page.
    pub fn offer(&self, download: Download) -> Option<(String, String)> {
        let Ok(id) = served::random_name() else {
            let _ = fs::remove_file(download.path);
            return None;
        };
        let entry = Entry { path: download.path, name: download.name.clone(), at: Instant::now() };
        self.downloads.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), entry);
        Some((id, download.name))
    }

    pub fn take_download(&self, id: &str) -> Option<Download> {
        let entry = self.downloads.lock().unwrap_or_else(|e| e.into_inner()).remove(id)?;
        Some(Download { path: entry.path, name: entry.name })
    }

    /// Removes what has waited too long.
    pub fn expire(&self, now: Instant) {
        for map in [&self.uploads, &self.downloads] {
            let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
            let old: Vec<String> = map.iter().filter(|(_, e)| now.saturating_duration_since(e.at) > KEEP_FOR).map(|(id, _)| id.clone()).collect();
            for id in old {
                if let Some(entry) = map.remove(&id) {
                    remove(entry);
                }
            }
        }
    }
}

/// A file name for the `Content-Disposition` header: the plain form keeps safe ASCII only, the `filename*` form keeps the rest.
fn disposition(name: &str) -> String {
    let plain: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "._- ()".contains(c) { c } else { '_' }).collect();
    let encoded: String = name
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"._-".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    format!("attachment; filename=\"{plain}\"; filename*=UTF-8''{encoded}")
}

pub fn send_download(request: Request, download: Download, respond: fn(Request, Response<File>)) {
    let file = File::open(&download.path);
    // The file is opened, so it can go from the folder now; the open handle still reads it.
    let _ = fs::remove_file(&download.path);
    let Ok(file) = file else {
        let _ = request.respond(Response::from_string("not found").with_status_code(404));
        return;
    };
    let response = Response::from_file(file)
        .with_header(header("Content-Type", "application/octet-stream"))
        .with_header(header("Content-Disposition", &disposition(&download.name)))
        .with_header(header("Cache-Control", "no-store"));
    respond(request, response);
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The page's file at `path`, or `index.html` for an address of the app (`/app/transactions`), so a reload stays on its page.
pub fn serve_page(web: &Path, path: &str, request: Request, respond: fn(Request, Response<File>)) {
    let relative = path.trim_start_matches('/');
    // Nothing outside the folder and nothing hidden: no `..`, no dot files, no backslashes.
    let safe = !relative.contains('\\') && relative.split('/').all(|part| !part.starts_with('.'));
    let wanted = web.join(relative);
    let last = relative.rsplit('/').next().unwrap_or("");
    let (file, immutable) = if safe && !relative.is_empty() && wanted.is_file() {
        (wanted, relative.starts_with("assets/"))
    } else if !safe || last.contains('.') {
        // a missing file is missing, not the page
        let _ = request.respond(Response::from_string("not found").with_status_code(404));
        return;
    } else {
        (web.join("index.html"), false)
    };
    let Ok(opened) = File::open(&file) else {
        let _ = request.respond(Response::from_string("not found").with_status_code(404));
        return;
    };
    let cache = if immutable { "public, max-age=31536000, immutable" } else { "no-cache" };
    let response = Response::from_file(opened).with_header(header("Content-Type", content_type(&file))).with_header(header("Cache-Control", cache));
    respond(request, response);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_name_cannot_break_the_header() {
        assert_eq!(disposition("meowny-2026-11-27.json"), "attachment; filename=\"meowny-2026-11-27.json\"; filename*=UTF-8''meowny-2026-11-27.json");
        let tricky = disposition("a\"b\r\nX: y é.pdf");
        assert!(!tricky.contains('\r') && !tricky.contains('\n'));
        assert!(tricky.starts_with("attachment; filename=\"a_b__X_ y _.pdf\";"));
        assert!(tricky.ends_with("filename*=UTF-8''a%22b%0D%0AX%3A%20y%20%C3%A9.pdf"));
    }

    #[test]
    fn staged_files_are_used_once_and_expire() {
        let dir = tempfile::tempdir().unwrap();
        let staged = Staged::new(dir.path().to_path_buf());
        let id = staged.receive(&mut &b"backup bytes"[..], 100).unwrap();
        let path = staged.take_upload(&id).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"backup bytes");
        assert!(staged.take_upload(&id).is_none());
        assert!(matches!(staged.receive(&mut &[0u8; 101][..], 100), Err(UploadError::TooLarge)));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "the refused upload left nothing");

        let written = dir.path().join("export");
        fs::write(&written, b"x").unwrap();
        let (id, name) = staged.offer(Download { path: written.clone(), name: "a.csv".into() }).unwrap();
        assert_eq!(name, "a.csv");
        staged.expire(Instant::now() + KEEP_FOR + Duration::from_secs(1));
        assert!(staged.take_download(&id).is_none());
        assert!(!written.exists());
    }
}
