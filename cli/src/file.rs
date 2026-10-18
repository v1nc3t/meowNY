//! Reading a file to send to the app. The app never opens a path on a program's behalf: this program runs as the person, with the
//! person's rights, reads the file itself and sends its bytes as base64url text in the body of one request (the route
//! `documents.add`). That is what the `bytes` kind of argument is, and why its flag is `--file` and not `--data`.

use crate::error::CliError;

/// The largest file the app stores (25 MiB). The app refuses a bigger one too; this is so a wrong pick says so before it is read.
pub const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url without padding: the form the app reads.
pub fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for shift in [18, 12, 6, 0].into_iter().take(chunk.len() + 1) {
            out.push(ALPHABET[((n >> shift) & 63) as usize] as char);
        }
    }
    out
}

/// What the person typed, cut short and quoted, so an odd path cannot garble the message.
fn shown(text: &str) -> String {
    let cut: String = text.chars().take(60).collect();
    format!("{cut:?}")
}

/// The bytes of the file at `path` as base64url text, and the name to keep it under (the last part of the path). A file that is
/// not a regular file, is empty or is over the limit is refused here.
pub fn load(flag: &str, path: &str) -> Result<(String, String), CliError> {
    use std::io::Read;
    let meta = std::fs::metadata(path).map_err(|e| CliError::usage(format!("{flag}: cannot read {}: {}", shown(path), reason(e.kind()))))?;
    if !meta.is_file() {
        return Err(CliError::usage(format!("{flag}: {} is not a file", shown(path))));
    }
    if meta.len() == 0 {
        return Err(CliError::usage(format!("{flag}: {} is empty", shown(path))));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(CliError::usage(format!("{flag}: {} is {} bytes; the app keeps files of at most 25 MiB", shown(path), meta.len())));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| CliError::usage(format!("{flag}: cannot read {}: {}", shown(path), reason(e.kind()))))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(CliError::usage(format!("{flag}: {} grew while it was read", shown(path))));
    }
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
    Ok((base64url(&bytes), name))
}

fn reason(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "there is no such file",
        std::io::ErrorKind::PermissionDenied => "permission denied",
        _ => "it could not be read",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_is_the_unpadded_url_safe_alphabet() {
        for (plain, coded) in [("", ""), ("f", "Zg"), ("fo", "Zm8"), ("foo", "Zm9v"), ("foob", "Zm9vYg"), ("fooba", "Zm9vYmE"), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64url(plain.as_bytes()), coded);
        }
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8", "- and _ instead of + and /");
        assert_eq!(base64url(&[0xff, 0xff, 0xff]), "____");
        let all: Vec<u8> = (0..=255).collect();
        let coded = base64url(&all);
        assert_eq!(coded.len(), 342, "256 bytes are 342 characters without padding");
        assert!(coded.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    }

    #[test]
    fn what_is_not_a_file_to_send_is_refused_with_the_reason() {
        let dir = tempfile::tempdir().unwrap();
        let text = |path: &std::path::Path| load("--file", path.to_str().unwrap()).unwrap_err().message;
        assert!(text(&dir.path().join("missing.pdf")).contains("there is no such file"));
        assert!(text(dir.path()).contains("is not a file"));
        let empty = dir.path().join("empty.pdf");
        std::fs::write(&empty, b"").unwrap();
        assert!(text(&empty).contains("is empty"));
        let big = dir.path().join("big.bin");
        let file = std::fs::File::create(&big).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        assert!(text(&big).contains("at most 25 MiB"));
        // the largest file is read
        file.set_len(MAX_FILE_BYTES).unwrap();
        assert_eq!(load("--file", big.to_str().unwrap()).unwrap().0.len(), 34_952_534, "25 MiB as base64url text");
    }
}
