//! Files dropped on the window: what the window remembers, what each file comes to when it is added, and what a locked app does.

use std::fs;
use std::path::{Path, PathBuf};

use tauri::{DragDropEvent, PhysicalPosition};

use super::dropped::*;
use super::library::{self, Document};
use super::MAX_DOCUMENT_BYTES;
use crate::keys::memory::MemoryStore;
use crate::vault::{AppError, Settings, Vault};

const PDF: &[u8] = b"%PDF-1.4 a receipt for the boiler";

fn vault(dir: &Path) -> Vault {
    let v = Vault::new(dir.to_path_buf(), Box::new(MemoryStore::default()));
    let settings = Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "UTC".into(), language: "en".into() };
    v.create(&settings, None).unwrap();
    v
}

/// What the command does: add each dropped file with the library's own function.
fn add(v: &Vault, pending: Pending) -> DroppedResult {
    add_all(pending, |input, bytes| v.with_files(|conn, files| library::add(conn, files, input, bytes, "2026-10-16")))
}

fn put(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn documents(v: &Vault) -> Vec<Document> {
    v.with_files(|conn, files| Ok(library::list(conn, files, &Default::default())?.documents)).unwrap()
}

fn status(result: &DroppedResult) -> Vec<(&str, &'static str, Option<AppError>)> {
    result.outcomes.iter().map(|o| (o.name.as_str(), o.status, o.error)).collect()
}

fn pending(paths: Vec<PathBuf>) -> Pending {
    let held = Dropped::default();
    held.set(paths);
    held.take()
}

#[test]
fn each_dropped_file_becomes_a_document_named_after_it_and_the_rest_go_on_when_one_cannot() {
    let (app, files) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let v = vault(app.path());
    std::fs::create_dir(files.path().join("a folder")).unwrap();
    let paths = vec![
        put(files.path(), "Boiler receipt.pdf", PDF),
        put(files.path(), "notes.txt", b"plain notes about the flat"),
        put(files.path(), "empty.pdf", b""),
        files.path().join("a folder"),
        files.path().join("gone.pdf"),
        put(files.path(), "Boiler receipt again.pdf", PDF),
    ];
    let result = add(&v, pending(paths));
    assert_eq!(
        status(&result),
        [
            ("Boiler receipt.pdf", "added", None),
            ("notes.txt", "added", None),
            ("empty.pdf", "failed", Some(AppError::InvalidInput)),
            ("a folder", "failed", Some(AppError::InvalidInput)),
            ("gone.pdf", "failed", Some(AppError::NotFound)),
            ("Boiler receipt again.pdf", "alreadyThere", None),
        ]
    );
    assert_eq!(result.ignored, 0);
    let names: Vec<String> = documents(&v).into_iter().map(|d| d.name).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"Boiler receipt.pdf".to_string()) && names.contains(&"notes.txt".to_string()));
    let doc = documents(&v).into_iter().find(|d| d.name == "Boiler receipt.pdf").unwrap();
    assert_eq!((doc.media_type.as_str(), doc.size_bytes, doc.has_file, doc.link_count, doc.keep_on_phone), ("application/pdf", PDF.len() as u64, true, 0, false));
    assert_eq!(doc.document_date, "2026-10-16", "today, as nothing was said");
}

#[test]
fn a_file_over_the_limit_is_refused_before_it_is_read_and_the_largest_is_taken() {
    let (app, files) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let v = vault(app.path());
    let big = files.path().join("big.bin");
    let file = fs::File::create(&big).unwrap();
    file.set_len(MAX_DOCUMENT_BYTES + 1).unwrap();
    let result = add(&v, pending(vec![big.clone()]));
    assert_eq!(status(&result), [("big.bin", "failed", Some(AppError::TooLarge))]);
    assert!(documents(&v).is_empty());
    file.set_len(MAX_DOCUMENT_BYTES).unwrap();
    // 25 MiB of nothing is a file all the same
    assert_eq!(status(&add(&v, pending(vec![big]))), [("big.bin", "added", None)]);
}

#[test]
fn a_name_that_is_not_clean_is_cleaned_in_the_document_and_in_what_the_page_is_told() {
    let (app, files) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let v = vault(app.path());
    let result = add(&v, pending(vec![put(files.path(), "in\u{202e}voice  2026.pdf", b"%PDF-1.4 x")]));
    assert_eq!(status(&result), [("invoice 2026.pdf", "added", None)]);
    assert_eq!(documents(&v)[0].name, "invoice 2026.pdf");
}

#[test]
fn a_drop_holds_at_most_a_hundred_files_and_says_how_many_were_left_out() {
    let held = Dropped::default();
    held.set((0..130).map(|i| PathBuf::from(format!("/nowhere/{i}.pdf"))).collect());
    let got = held.take();
    assert_eq!((got.paths.len(), got.ignored), (MAX_PER_DROP, 30));
    assert!(held.is_empty(), "what is taken is gone");
    assert_eq!(held.take(), Pending::default());
    // a second drop replaces the first
    held.set(vec![PathBuf::from("/a.pdf")]);
    held.set(vec![PathBuf::from("/b.pdf")]);
    assert_eq!(held.take().paths, [PathBuf::from("/b.pdf")]);
    assert_eq!(add_all(pending((0..130).map(|i| PathBuf::from(format!("/nowhere/{i}.pdf"))).collect()), |_, _| Err(AppError::Internal)).ignored, 30);
}

// --- what the window does with a drag ----------------------------------------------------------------------------------

fn at() -> PhysicalPosition<f64> {
    PhysicalPosition::new(10.0, 10.0)
}

fn names(events: &[(&'static str, serde_json::Value)]) -> Vec<String> {
    events.iter().map(|(name, payload)| format!("{name}={payload}")).collect()
}

#[test]
fn the_window_remembers_what_is_dropped_and_tells_the_page() {
    let (app, files) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let v = vault(app.path());
    let one = put(files.path(), "a.pdf", PDF);
    let two = put(files.path(), "b.pdf", b"%PDF-1.4 other");

    assert_eq!(names(&on_drag(&v, &DragDropEvent::Enter { paths: vec![one.clone()], position: at() })), ["documents-drag=true"]);
    assert!(on_drag(&v, &DragDropEvent::Over { position: at() }).is_empty(), "moving over the window says nothing");
    assert_eq!(names(&on_drag(&v, &DragDropEvent::Leave)), ["documents-drag=false"]);
    assert!(v.dropped().is_empty(), "nothing was dropped yet");

    let events = on_drag(&v, &DragDropEvent::Drop { paths: vec![one.clone(), two.clone()], position: at() });
    assert_eq!(names(&events), ["documents-drag=false", "documents-dropped=2"]);
    let got = v.dropped().take();
    assert_eq!(got.paths, [one, two]);
    assert_eq!(add(&v, got).outcomes.len(), 2);
    assert_eq!(documents(&v).len(), 2);
}

#[test]
fn a_locked_app_takes_nothing_and_a_lock_forgets_what_was_dropped() {
    let (app, files) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let v = vault(app.path());
    let one = put(files.path(), "a.pdf", PDF);
    on_drag(&v, &DragDropEvent::Drop { paths: vec![one.clone()], position: at() });
    assert!(!v.dropped().is_empty());
    v.lock();
    assert!(v.dropped().is_empty(), "locking forgets the paths");

    // while locked, a drop is not remembered and the page is not told anything but that the hover is over
    assert!(on_drag(&v, &DragDropEvent::Enter { paths: vec![one.clone()], position: at() }).is_empty());
    assert_eq!(names(&on_drag(&v, &DragDropEvent::Drop { paths: vec![one], position: at() })), ["documents-drag=false"]);
    assert!(v.dropped().is_empty());
    // and a drop of nothing is nothing
    v.unlock(None).unwrap();
    assert_eq!(names(&on_drag(&v, &DragDropEvent::Drop { paths: vec![], position: at() })), ["documents-drag=false"]);
    assert!(on_drag(&v, &DragDropEvent::Enter { paths: vec![], position: at() }).is_empty());
}
