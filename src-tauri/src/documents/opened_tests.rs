use std::fs;

use super::opened::Opened;
use super::seal::DocError;

fn opened() -> (tempfile::TempDir, Opened) {
    let dir = tempfile::tempdir().unwrap();
    let opened = Opened::new(dir.path());
    (dir, opened)
}

#[test]
fn a_copy_is_a_file_in_a_folder_of_its_own() {
    let (dir, opened) = opened();
    let a = opened.make_copy("Receipt.pdf", b"%PDF-1.7 one").unwrap();
    let b = opened.make_copy("Receipt.pdf", b"%PDF-1.7 two").unwrap();
    assert_ne!(a, b, "the same name opened twice is two copies, in two folders");
    assert_eq!(fs::read(&a).unwrap(), b"%PDF-1.7 one");
    assert_eq!(fs::read(&b).unwrap(), b"%PDF-1.7 two");
    for path in [&a, &b] {
        assert_eq!(path.file_name().unwrap(), "Receipt.pdf");
        assert_eq!(path.parent().unwrap().parent().unwrap(), dir.path().join("open"), "{path:?}");
        let folder = path.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(folder.len(), 32, "a random name");
    }
}

#[cfg(unix)]
#[test]
fn the_copy_and_its_folders_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, opened) = opened();
    let path = opened.make_copy("a.txt", b"x").unwrap();
    let mode = |p: &std::path::Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    assert_eq!(mode(&dir.path().join("open")), 0o700);
}

#[test]
fn clearing_removes_every_copy_and_the_folder_and_is_quiet_when_there_is_none() {
    let (dir, opened) = opened();
    assert!(opened.clear(), "nothing to clear is fine");
    opened.make_copy("a.pdf", b"one").unwrap();
    opened.make_copy("b.pdf", b"two").unwrap();
    assert!(opened.clear());
    assert!(!dir.path().join("open").exists());
    assert!(opened.clear());
    // and it can be used again afterwards
    assert!(opened.make_copy("c.pdf", b"three").unwrap().exists());
}

#[test]
fn a_name_that_could_leave_the_folder_is_refused() {
    let (dir, opened) = opened();
    for bad in ["", ".", "..", "../x.pdf", "a/b.pdf", "a\\b.pdf", "/etc/passwd", "x\0.pdf", &"a".repeat(256)] {
        assert_eq!(opened.make_copy(bad, b"x").err(), Some(DocError::Format), "{bad:?}");
    }
    assert!(!dir.path().join("open").exists() || fs::read_dir(dir.path().join("open")).unwrap().next().is_none(), "nothing was made");
}
