//! Where a confirmed route may write: a file the person has been shown by its full name, in a folder
//! that exists, that is not there yet. A token can ask for a file but cannot pick a place the person
//! would not have allowed, nor replace anything: a file that exists is never touched, so an export
//! cannot overwrite a startup script or someone's notes however it is worded.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use super::routes::is_plain_text;
use super::ApiError;

const MAX_CHARS: usize = 400;

/// The path as the person will see it in the dialog, if it is one the app will write to: absolute, no
/// `.` or `..` parts (so the dialog shows where the file really goes), a plain file name ending in
/// `extension`, in a folder that exists, and not there yet.
pub fn checked(text: &str, extension: &str) -> Result<PathBuf, ApiError> {
    if text.is_empty() || text.chars().count() > MAX_CHARS || !is_plain_text(text) {
        return Err(ApiError::invalid("the path must be plain text of at most 400 characters"));
    }
    let path = Path::new(text);
    if !path.is_absolute() {
        return Err(ApiError::invalid("the path must be absolute"));
    }
    // `Path::components` silently drops a `.` in the middle, so the text itself is looked at.
    let dotted = text.split(['/', std::path::MAIN_SEPARATOR]).any(|part| part == "." || part == "..");
    if dotted || path.components().any(|c| matches!(c, Component::CurDir | Component::ParentDir)) {
        return Err(ApiError::invalid("the path must not contain . or .. parts"));
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let suffix = format!(".{extension}");
    let stem_len = name.len().checked_sub(suffix.len());
    let right_end = stem_len.is_some_and(|n| n > 0 && name.is_char_boundary(n) && name[n..].eq_ignore_ascii_case(&suffix));
    if !right_end {
        return Err(ApiError::invalid(format!("the file name must end in {suffix}")));
    }
    if !path.parent().is_some_and(Path::is_dir) {
        return Err(ApiError::invalid("the folder does not exist"));
    }
    // Refused before the person is asked. `write_new` checks again, in the same step that creates the file.
    if fs::symlink_metadata(path).is_ok() {
        return Err(ApiError::FILE_EXISTS);
    }
    Ok(path.to_path_buf())
}

/// Writes a new file, readable by its owner only. If the file exists, even as a link to somewhere, nothing is
/// written: the check and the creation are one step, so there is no gap to slip into.
pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), ApiError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => ApiError::FILE_EXISTS,
        _ => ApiError::CANNOT_WRITE,
    })?;
    if file.write_all(bytes).and_then(|_| file.sync_all()).is_err() {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(ApiError::CANNOT_WRITE);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn at(dir: &tempfile::TempDir, name: &str) -> String {
        dir.path().join(name).to_str().unwrap().to_string()
    }

    #[test]
    fn a_plain_absolute_path_with_the_right_ending_in_a_real_folder_is_accepted() {
        let d = dir();
        let p = checked(&at(&d, "export.json"), "json").unwrap();
        assert_eq!(p, d.path().join("export.json"));
        assert!(checked(&at(&d, "EXPORT.JSON"), "json").is_ok(), "the ending is compared without regard to case");
        assert!(checked(&at(&d, "a.b.c.csv"), "csv").is_ok());
        assert!(checked(&at(&d, "Café “2026”.meowbak"), "meowbak").is_ok(), "ordinary text in a name");
    }

    #[test]
    fn anything_else_is_refused_with_the_reason() {
        let d = dir();
        let msg = |text: &str, ext: &str| checked(text, ext).unwrap_err().message.into_owned();
        assert!(msg("export.json", "json").contains("absolute"));
        assert!(msg("./export.json", "json").contains("absolute"));
        assert!(msg("", "json").contains("plain text"));
        assert!(msg(&format!("{}/../x.json", d.path().display()), "json").contains("not contain"));
        assert!(msg(&format!("{}/./x.json", d.path().display()), "json").contains("not contain"));
        for name in ["export.csv", "export", "export.json.txt", ".json", "json", "x.jsonx", "x.json "] {
            assert!(msg(&at(&d, name), "json").contains("must end in .json"), "{name:?}");
        }
        assert!(msg(&at(&d, "nope/export.json"), "json").contains("does not exist"));
        let a_file = at(&d, "plain");
        std::fs::write(&a_file, "x").unwrap();
        assert!(msg(&format!("{a_file}/export.json"), "json").contains("does not exist"), "a file is not a folder");
        // What the dialog could be made to misread.
        for bad in ["\n", "\r", "\u{0}", "\u{1b}[31m", "\u{202e}", "\u{2066}", "\u{200f}", "\u{feff}"] {
            let text = at(&d, &format!("a{bad}b.json"));
            assert!(msg(&text, "json").contains("plain text"), "{bad:?}");
        }
        assert!(msg(&format!("{}/{}.json", d.path().display(), "x".repeat(400)), "json").contains("400"));
    }

    #[test]
    fn a_name_that_is_taken_is_refused_before_anything_is_asked() {
        let d = dir();
        let taken = at(&d, "taken.json");
        std::fs::write(&taken, "mine").unwrap();
        assert_eq!(checked(&taken, "json"), Err(ApiError::FILE_EXISTS));
        // A folder of that name, and a link (even a dangling one), count as taken too.
        std::fs::create_dir(at(&d, "folder.json")).unwrap();
        assert_eq!(checked(&at(&d, "folder.json"), "json"), Err(ApiError::FILE_EXISTS));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(d.path().join("nowhere"), d.path().join("dangling.json")).unwrap();
            assert_eq!(checked(&at(&d, "dangling.json"), "json"), Err(ApiError::FILE_EXISTS));
        }
        assert_eq!(std::fs::read_to_string(&taken).unwrap(), "mine");
    }

    #[test]
    fn a_file_is_written_whole_and_private() {
        let d = dir();
        let path = d.path().join("a.json");
        write_new(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        write_new(&d.path().join("empty.json"), b"").unwrap();
    }

    #[test]
    fn a_file_that_exists_is_never_replaced() {
        let d = dir();
        let path = d.path().join("precious.json");
        std::fs::write(&path, "the original").unwrap();
        assert_eq!(write_new(&path, b"overwritten"), Err(ApiError::FILE_EXISTS));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "the original");
        // Not even through a link, whether it points at a file or at nothing.
        #[cfg(unix)]
        {
            let link = d.path().join("link.json");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert_eq!(write_new(&link, b"through the link"), Err(ApiError::FILE_EXISTS));
            let dangling = d.path().join("dangling.json");
            std::os::unix::fs::symlink(d.path().join("nowhere"), &dangling).unwrap();
            assert_eq!(write_new(&dangling, b"x"), Err(ApiError::FILE_EXISTS));
            assert!(!d.path().join("nowhere").exists(), "a dangling link was not followed to make its target");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "the original");
        }
    }

    #[test]
    fn a_folder_that_cannot_be_written_is_a_clear_error() {
        let d = dir();
        assert_eq!(write_new(&d.path().join("missing").join("a.json"), b"x"), Err(ApiError::CANNOT_WRITE));
        assert_eq!(write_new(d.path(), b"x"), Err(ApiError::FILE_EXISTS), "a folder is already there");
    }
}
