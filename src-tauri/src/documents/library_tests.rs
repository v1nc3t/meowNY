use std::fs;

use rusqlite::{params, Connection};

use super::library::*;
use super::seal::{DocError, DocId, DocKeys, MAX_DOCUMENT_BYTES};
use super::store::Store;
use crate::db::DbKey;
use crate::vault::AppError;

const TODAY: &str = "2026-10-16";
const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n%%EOF\n";
// a real PNG of 3 by 2 pixels
const PNG: &str = "89504e470d0a1a0a0000000d49484452000000030000000208020000001216f1";

struct Env {
    conn: Connection,
    _dir: tempfile::TempDir,
    store: Store,
    keys: DocKeys,
}

impl Env {
    fn new() -> Env {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let keys = DocKeys::from_db_key(&DbKey::from_bytes([5; 32]));
        Env { conn, _dir: dir, store, keys }
    }

    fn dir_path(&self) -> &std::path::Path {
        self._dir.path()
    }

    fn files(&self) -> Files<'_> {
        Files { store: &self.store, keys: &self.keys }
    }

    fn input(name: &str) -> NewDocument {
        NewDocument { name: name.into(), ..Default::default() }
    }

    fn add(&self, name: &str, bytes: &[u8]) -> AddOutcome {
        add(&self.conn, &self.files(), &Self::input(name), bytes, TODAY).unwrap()
    }

    fn category(&self) -> i64 {
        self.conn.execute("INSERT OR IGNORE INTO categories (type, name) VALUES ('EXPENSE', 'Food')", []).unwrap();
        self.conn.query_row("SELECT id FROM categories WHERE name = 'Food'", [], |r| r.get(0)).unwrap()
    }

    fn transaction(&self, name: &str) -> i64 {
        self.conn
            .execute(
                "INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (?1, ?2, 350, '2026-01-10')",
                params![self.category(), name],
            )
            .unwrap();
        self.conn.last_insert_rowid()
    }

    fn recurring(&self, name: &str) -> i64 {
        self.conn
            .execute(
                "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date)
                 VALUES (?1, ?2, 90000, '2026-01-01', 'MONTH', '2026-02-01')",
                params![self.category(), name],
            )
            .unwrap();
        self.conn.last_insert_rowid()
    }

    fn count(&self, sql: &str) -> i64 {
        self.conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn files_on_disk(&self) -> Vec<String> {
        let mut names: Vec<String> = match fs::read_dir(self.store.dir()) {
            Ok(entries) => entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
            Err(_) => Vec::new(),
        };
        names.sort();
        names
    }

    fn path_of(&self, doc: &Document) -> std::path::PathBuf {
        let sync_id: String = self.conn.query_row("SELECT sync_id FROM documents WHERE id = ?1", [doc.id], |r| r.get(0)).unwrap();
        self.store.dir().join(self.keys.file_name(&DocId::parse(&sync_id).unwrap()))
    }

    /// Nothing was left behind by a refused add: no row, no link, no file.
    fn assert_nothing_stored(&self, why: &str) {
        assert_eq!(self.count("SELECT count(*) FROM documents"), 0, "{why}: a row");
        assert_eq!(self.count("SELECT count(*) FROM document_links"), 0, "{why}: a link");
        assert!(self.files_on_disk().is_empty(), "{why}: {:?}", self.files_on_disk());
    }
}

fn transaction_target(id: i64) -> LinkTarget {
    LinkTarget { kind: TargetKind::Transaction, id: id.to_string() }
}

fn recurring_target(id: i64) -> LinkTarget {
    LinkTarget { kind: TargetKind::Recurring, id: id.to_string() }
}

fn png() -> Vec<u8> {
    (0..PNG.len() / 2).map(|i| u8::from_str_radix(&PNG[i * 2..i * 2 + 2], 16).unwrap()).collect()
}

// -- adding -------------------------------------------------------------------------------------------------------------

#[test]
fn the_kind_of_file_comes_from_its_bytes_and_not_its_name() {
    let env = Env::new();
    let cases: [(&str, Vec<u8>, &str); 5] = [
        ("photo.png", png(), "image/png"),
        ("photo.pdf", png(), "image/png"),
        ("totals.csv", b"a,b\n1,2\n".to_vec(), "text/csv"),
        ("holiday.jpg", b"MZ\x90\x00\x03\x00\x00\x00 a program".to_vec(), "application/octet-stream"),
        ("notes", "some notes".as_bytes().to_vec(), "text/plain"),
    ];
    for (name, bytes, expected) in cases {
        assert_eq!(env.add(name, &bytes).document.media_type, expected, "{name}");
    }
}

#[test]
fn a_hostile_name_is_cleaned_before_it_is_stored() {
    let env = Env::new();
    let doc = env.add("../../etc/\u{202e}fdp.exe\0", PDF).document;
    assert_eq!(doc.name, "fdp.exe");
    assert!(super::names::is_clean_name(&doc.name));
    let doc = env.add("", b"another file").document;
    assert_eq!(doc.name, "document");
}

#[test]
fn the_same_bytes_are_stored_once_and_the_new_links_go_to_the_document_that_is_there() {
    let env = Env::new();
    let (first, second) = (env.transaction("March rent"), env.transaction("April rent"));
    let one = add(
        &env.conn,
        &env.files(),
        &NewDocument { name: "Rent invoice.pdf".into(), links: vec![transaction_target(first)], ..Default::default() },
        PDF,
        TODAY,
    )
    .unwrap();
    let two = add(
        &env.conn,
        &env.files(),
        &NewDocument { name: "a different name.pdf".into(), links: vec![transaction_target(second), transaction_target(first)], ..Default::default() },
        PDF,
        TODAY,
    )
    .unwrap();
    assert!(!one.already_there && two.already_there);
    assert_eq!(one.document.id, two.document.id);
    assert_eq!(two.document.name, "Rent invoice.pdf", "the first name stays");
    assert_eq!(two.document.link_count, 2);
    assert_eq!((env.count("SELECT count(*) FROM documents"), env.files_on_disk().len()), (1, 1));
}

#[test]
fn adding_a_file_again_brings_back_one_this_device_lost() {
    let env = Env::new();
    let doc = env.add("Receipt.pdf", PDF).document;
    fs::remove_file(env.path_of(&doc)).unwrap();
    assert!(!get(&env.conn, &env.files(), doc.id).unwrap().has_file);
    assert_eq!(read(&env.conn, &env.files(), doc.id).err(), Some(AppError::FileMissing));

    let again = env.add("Receipt.pdf", PDF);
    assert!(again.already_there);
    assert!(again.document.has_file);
    assert_eq!(read(&env.conn, &env.files(), doc.id).unwrap().as_slice(), PDF);
}

#[test]
fn a_refused_file_leaves_nothing_behind() {
    let env = Env::new();
    let tx = env.transaction("Coffee");
    let with_links = |links: Vec<LinkTarget>| NewDocument { name: "x.pdf".into(), links, ..Default::default() };
    let try_add = |input: &NewDocument, bytes: &[u8]| add(&env.conn, &env.files(), input, bytes, TODAY).err();

    assert_eq!(try_add(&Env::input("x"), b""), Some(AppError::InvalidInput), "an empty file");
    env.assert_nothing_stored("empty");
    assert_eq!(try_add(&Env::input("x"), &vec![0u8; MAX_DOCUMENT_BYTES as usize + 1]), Some(AppError::TooLarge));
    env.assert_nothing_stored("too large");
    let bad_date = NewDocument { name: "x".into(), document_date: Some("2026-02-30".into()), ..Default::default() };
    assert_eq!(try_add(&bad_date, PDF), Some(AppError::InvalidInput));
    env.assert_nothing_stored("bad date");
    assert_eq!(try_add(&with_links(vec![transaction_target(tx), transaction_target(9999)]), PDF), Some(AppError::NotFound));
    env.assert_nothing_stored("a link to nothing");
    assert_eq!(try_add(&with_links(vec![LinkTarget { kind: TargetKind::Transaction, id: "abc".into() }]), PDF), Some(AppError::InvalidInput));
    env.assert_nothing_stored("a bad id");
    assert_eq!(try_add(&with_links((0..51).map(|_| transaction_target(tx)).collect()), PDF), Some(AppError::InvalidInput));
    env.assert_nothing_stored("too many links");
}

#[test]
fn the_largest_file_is_taken() {
    let env = Env::new();
    let big: Vec<u8> = (0..MAX_DOCUMENT_BYTES as usize).map(|i| (i % 251) as u8).collect();
    let doc = env.add("big.bin", &big).document;
    assert_eq!(doc.size_bytes, MAX_DOCUMENT_BYTES);
    assert_eq!(read(&env.conn, &env.files(), doc.id).unwrap().as_slice(), big.as_slice());
}

#[test]
fn a_file_that_cannot_be_written_leaves_no_row() {
    let env = Env::new();
    // a plain file where the folder should be
    let blocked_dir = tempfile::tempdir().unwrap();
    fs::write(blocked_dir.path().join("documents"), b"in the way").unwrap();
    let store = Store::new(blocked_dir.path());
    let files = Files { store: &store, keys: &env.keys };
    let failed = add(&env.conn, &files, &Env::input("x.pdf"), PDF, TODAY);
    assert_eq!(failed.err(), Some(AppError::Internal));
    assert_eq!(env.count("SELECT count(*) FROM documents"), 0);
    assert_eq!(env.count("SELECT count(*) FROM audit_log WHERE entity_type = 'documents'"), 0, "the rolled-back row left no history");
}

// -- changing, linking, deleting ------------------------------------------------------------------------------------------

#[test]
fn a_bad_change_is_refused_and_changes_nothing() {
    let env = Env::new();
    let doc = env.add("Receipt.pdf", PDF).document;
    for patch in [
        DocumentPatch { name: Some("   ".into()), ..Default::default() },
        DocumentPatch { name: Some("...".into()), ..Default::default() },
        DocumentPatch { name: Some("ok".into()), document_date: Some("tomorrow".into()), keep_on_phone: Some(true) },
    ] {
        assert_eq!(update(&env.conn, &env.files(), doc.id, &patch).err(), Some(AppError::InvalidInput), "{patch:?}");
    }
    assert_eq!(get(&env.conn, &env.files(), doc.id).unwrap(), doc);
    assert_eq!(update(&env.conn, &env.files(), 999, &DocumentPatch::default()).err(), Some(AppError::NotFound));
    assert_eq!(get(&env.conn, &env.files(), 999).err(), Some(AppError::NotFound));
}

#[test]
fn linking_is_idempotent_and_unlinking_says_when_there_was_nothing() {
    let env = Env::new();
    let doc = env.add("Receipt.pdf", PDF).document;
    let tx = env.transaction("Coffee");
    link(&env.conn, doc.id, &transaction_target(tx)).unwrap();
    link(&env.conn, doc.id, &transaction_target(tx)).unwrap();
    assert_eq!(get(&env.conn, &env.files(), doc.id).unwrap().link_count, 1);
    assert_eq!(env.count("SELECT count(*) FROM document_links"), 1);

    assert_eq!(link(&env.conn, 999, &transaction_target(tx)).err(), Some(AppError::NotFound));
    assert_eq!(link(&env.conn, doc.id, &transaction_target(999)).err(), Some(AppError::NotFound));
    assert_eq!(link(&env.conn, doc.id, &recurring_target(tx)).err(), Some(AppError::NotFound), "a transaction id is not a recurring id");

    unlink(&env.conn, doc.id, &transaction_target(tx)).unwrap();
    assert_eq!(unlink(&env.conn, doc.id, &transaction_target(tx)).err(), Some(AppError::NotFound));
    assert_eq!(get(&env.conn, &env.files(), doc.id).unwrap().link_count, 0);
    assert_eq!(env.count("SELECT count(*) FROM documents"), 1, "unlinking never deletes the document");
    assert_eq!(links_of(&env.conn, 999).err(), Some(AppError::NotFound));
}

#[test]
fn deleting_a_document_takes_its_file_and_its_links_and_leaves_the_rest() {
    let env = Env::new();
    let tx = env.transaction("Coffee");
    let doc = add(
        &env.conn,
        &env.files(),
        &NewDocument { name: "Receipt.pdf".into(), links: vec![transaction_target(tx)], ..Default::default() },
        PDF,
        TODAY,
    )
    .unwrap()
    .document;
    let other = env.add("Other.pdf", b"%PDF-1.7 another").document;
    delete(&env.conn, &env.files(), doc.id).unwrap();

    assert_eq!(env.count("SELECT count(*) FROM documents"), 1);
    assert_eq!(env.count("SELECT count(*) FROM document_links"), 0);
    assert_eq!(env.count("SELECT count(*) FROM transactions"), 1);
    assert_eq!(env.files_on_disk(), vec![env.path_of(&other).file_name().unwrap().to_string_lossy().into_owned()]);
    assert_eq!(env.count("SELECT count(*) FROM sync_deleted WHERE kind = 'documents'"), 1, "the other device must learn of it");
    assert_eq!(delete(&env.conn, &env.files(), doc.id).err(), Some(AppError::NotFound));
}

#[test]
fn deleting_a_document_whose_file_is_already_gone_works() {
    let env = Env::new();
    let doc = env.add("Receipt.pdf", PDF).document;
    fs::remove_file(env.path_of(&doc)).unwrap();
    delete(&env.conn, &env.files(), doc.id).unwrap();
    assert_eq!(env.count("SELECT count(*) FROM documents"), 0);
}

#[test]
fn deleting_a_transaction_or_a_recurring_item_never_deletes_a_document() {
    let env = Env::new();
    let (tx, rec) = (env.transaction("Coffee"), env.recurring("Rent"));
    let doc = add(
        &env.conn,
        &env.files(),
        &NewDocument { name: "Receipt.pdf".into(), links: vec![transaction_target(tx), recurring_target(rec)], ..Default::default() },
        PDF,
        TODAY,
    )
    .unwrap()
    .document;
    env.conn.execute("DELETE FROM transactions", []).unwrap();
    assert_eq!(get(&env.conn, &env.files(), doc.id).unwrap().link_count, 1);
    env.conn.execute("DELETE FROM recurring_transactions", []).unwrap();
    let kept = get(&env.conn, &env.files(), doc.id).unwrap();
    assert_eq!((kept.link_count, kept.has_file), (0, true), "a document with no links stays in the library, file and all");
    assert_eq!(read(&env.conn, &env.files(), doc.id).unwrap().as_slice(), PDF);
}

// -- listing ------------------------------------------------------------------------------------------------------------

// -- reading and checking ------------------------------------------------------------------------------------------------

#[test]
fn a_file_that_was_changed_on_disk_is_not_handed_over() {
    let env = Env::new();
    let doc = env.add("Receipt.pdf", PDF).document;
    let path = env.path_of(&doc);
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&path, &bytes).unwrap();
    assert_eq!(read(&env.conn, &env.files(), doc.id).err(), Some(AppError::FileDamaged));
    assert_eq!(read(&env.conn, &env.files(), 999).err(), Some(AppError::NotFound));
}

#[test]
fn a_file_that_does_not_match_its_row_is_not_handed_over() {
    let env = Env::new();
    let doc = env.add("Receipt.pdf", PDF).document;
    // the row is made to say something else about the same file (the trigger that guards the contents is the thing under test
    // elsewhere; here the row is rewritten the way a damaged database could be)
    env.conn.execute_batch("DROP TRIGGER trg_documents_contents_immutable").unwrap();
    env.conn.execute("UPDATE documents SET sha256 = ?1", ["b".repeat(64)]).unwrap();
    assert_eq!(read(&env.conn, &env.files(), doc.id).err(), Some(AppError::FileDamaged));
}

#[test]
fn checking_cleans_the_folder_and_reports_what_is_missing() {
    let env = Env::new();
    let kept = env.add("Kept.pdf", PDF).document;
    let lost = env.add("Lost.pdf", b"%PDF-1.7 lost").document;
    fs::remove_file(env.path_of(&lost)).unwrap();
    // a file that no row names, and what a crash leaves
    env.store.put(&env.keys, &DocId::from_bytes([9; 16]), b"orphan").unwrap();
    fs::write(env.store.dir().join(format!("{}.tmp", env.keys.stem(&DocId::from_bytes([8; 16])))), b"half").unwrap();

    let report = check(&env.conn, &env.files()).unwrap();
    assert_eq!((report.strays_removed, report.temp_removed), (1, 1));
    let lost_sync_id: String = env.conn.query_row("SELECT sync_id FROM documents WHERE id = ?1", [lost.id], |r| r.get(0)).unwrap();
    assert_eq!(report.missing, vec![DocId::parse(&lost_sync_id).unwrap()]);
    assert!(report.damaged.is_empty());
    assert_eq!(env.files_on_disk(), vec![env.path_of(&kept).file_name().unwrap().to_string_lossy().into_owned()]);
    assert_eq!(check(&env.conn, &env.files()).unwrap().strays_removed, 0, "a second check finds nothing more to clean");
}

#[test]
fn what_goes_wrong_with_a_file_is_told_to_the_screens_in_fixed_words() {
    for (error, expected) in [
        (DocError::TooLarge, AppError::TooLarge),
        (DocError::Missing, AppError::FileMissing),
        (DocError::Damaged, AppError::FileDamaged),
        (DocError::Mismatch, AppError::FileDamaged),
        (DocError::Format, AppError::FileDamaged),
        (DocError::BadId, AppError::Internal),
        (DocError::Incomplete, AppError::Internal),
        (DocError::Io(std::io::ErrorKind::PermissionDenied), AppError::Internal),
        (DocError::Random, AppError::Internal),
    ] {
        assert_eq!(AppError::from(error), expected, "{error:?}");
    }
}

#[test]
fn the_history_of_a_document_names_it_and_holds_no_contents() {
    let env = Env::new();
    let doc = env.add("Boiler warranty.pdf", PDF).document;
    update(&env.conn, &env.files(), doc.id, &DocumentPatch { name: Some("Boiler".into()), ..Default::default() }).unwrap();
    delete(&env.conn, &env.files(), doc.id).unwrap();
    let all: String = env
        .conn
        .query_row("SELECT group_concat(coalesce(old_values, '') || coalesce(new_values, ''), ' ') FROM audit_log WHERE entity_type = 'documents'", [], |r| r.get(0))
        .unwrap();
    assert!(all.contains("Boiler warranty.pdf") && all.contains("Boiler"));
    assert!(!all.contains(&doc.sha256) && !all.contains("%PDF"));
}

// -- what the screens and the commands lean on --------------------------------------------------------------------------

#[test]
fn the_report_counts_what_is_missing_and_on_the_phone_only_what_should_be_there() {
    let env = Env::new();
    let on_pc = env.add("Only on the computer.pdf", b"%PDF-1.7 pc").document;
    let kept = add(
        &env.conn,
        &env.files(),
        &NewDocument { name: "Kept on the phone.pdf".into(), keep_on_phone: true, ..Default::default() },
        b"%PDF-1.7 kept",
        TODAY,
    )
    .unwrap()
    .document;
    env.store.put(&env.keys, &DocId::from_bytes([9; 16]), b"stray").unwrap();
    fs::remove_file(env.path_of(&on_pc)).unwrap();
    fs::remove_file(env.path_of(&kept)).unwrap();

    let desktop = check_report(&env.conn, &env.files(), false).unwrap();
    assert_eq!(desktop, Report { missing: 2, damaged: 0, cleaned: 1 });
    // a phone does not miss the file of a document it was not asked to keep
    let phone = check_report(&env.conn, &env.files(), true).unwrap();
    assert_eq!(phone, Report { missing: 1, damaged: 0, cleaned: 0 });
}

#[test]
fn a_cut_file_is_reported_as_damaged() {
    let env = Env::new();
    let doc = env.add("Receipt.pdf", PDF).document;
    let path = env.path_of(&doc);
    let sealed = fs::read(&path).unwrap();
    fs::write(&path, &sealed[..sealed.len() - 3]).unwrap();
    let report = check_report(&env.conn, &env.files(), false).unwrap();
    assert_eq!((report.missing, report.damaged), (0, 1));
}

#[test]
fn an_image_is_given_to_the_page_as_text_and_only_when_the_app_may_draw_it() {
    let env = Env::new();
    let image = env.add("photo.png", &png()).document;
    let given = read_image(&env.conn, &env.files(), image.id).unwrap();
    assert_eq!(given.media_type, "image/png");
    assert_eq!(crate::sync::qr::base64url_decode(&given.data).unwrap(), png());

    // not an image, an image the system should open, and an image that says it is enormous
    let pdf = env.add("Receipt.pdf", PDF).document;
    assert_eq!(read_image(&env.conn, &env.files(), pdf.id).err(), Some(AppError::InvalidInput));
    let mut huge = png();
    huge[16..20].copy_from_slice(&20_000u32.to_be_bytes());
    huge[20..24].copy_from_slice(&20_000u32.to_be_bytes());
    let enormous = env.add("huge.png", &huge).document;
    assert_eq!(enormous.media_type, "image/png", "it is an image, only not one to draw");
    assert_eq!(read_image(&env.conn, &env.files(), enormous.id).err(), Some(AppError::InvalidInput));
    let mut heavy = png();
    heavy.resize(MAX_DRAW_BYTES as usize + 1, 7);
    let heavy = env.add("heavy.png", &heavy).document;
    assert_eq!(read_image(&env.conn, &env.files(), heavy.id).err(), Some(AppError::InvalidInput));
    assert_eq!(read_image(&env.conn, &env.files(), 999).err(), Some(AppError::NotFound));
}

/// The start of a zip file, enough for the app to take it for a Word or Excel file by its name.
fn zip_like(tail: u8) -> Vec<u8> {
    let mut bytes = b"PK\x03\x04".to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 1, 2, tail]);
    bytes
}

#[test]
fn the_preview_is_not_given_for_what_the_page_would_have_to_trust_or_cannot_read() {
    let env = Env::new();
    let refused = |name: &str, bytes: &[u8]| {
        let doc = env.add(name, bytes).document;
        read_preview(&env.conn, &env.files(), doc.id).err()
    };
    // markup of someone else, a kind the app has no reader for, and a name that says Word over bytes that are not a zip
    assert_eq!(refused("page.html", b"<script>alert(1)</script>"), Some(AppError::InvalidInput));
    assert_eq!(refused("old.doc", &zip_like(3)), Some(AppError::InvalidInput));
    assert_eq!(refused("slides.pptx", &zip_like(4)), Some(AppError::InvalidInput));
    assert_eq!(refused("Contract.docx", b"\x00\x01 not a zip"), Some(AppError::InvalidInput));
    assert_eq!(refused("Budget", &zip_like(5)), Some(AppError::InvalidInput), "no extension, no reader");
    // an image that says it is enormous
    let mut huge = png();
    huge[16..20].copy_from_slice(&20_000u32.to_be_bytes());
    huge[20..24].copy_from_slice(&20_000u32.to_be_bytes());
    assert_eq!(refused("huge.png", &huge), Some(AppError::InvalidInput));
    assert_eq!(read_preview(&env.conn, &env.files(), 999).err(), Some(AppError::NotFound));
}

#[test]
fn the_preview_has_a_limit_by_kind() {
    let env = Env::new();
    let mut heavy_image = png();
    heavy_image.resize(MAX_DRAW_BYTES as usize + 1, 7);
    let image = env.add("heavy.png", &heavy_image).document;
    assert_eq!(read_preview(&env.conn, &env.files(), image.id).err(), Some(AppError::InvalidInput));
    // a PDF may be heavier than an image, up to the preview limit
    let mut pdf = PDF.to_vec();
    pdf.resize(MAX_DRAW_BYTES as usize + 1, b' ');
    let fine = env.add("long.pdf", &pdf).document;
    assert_eq!(read_preview(&env.conn, &env.files(), fine.id).unwrap().kind, PreviewKind::Pdf);
    let mut too_heavy = PDF.to_vec();
    too_heavy.resize(MAX_PREVIEW_BYTES as usize + 1, b' ');
    let doc = env.add("heavier.pdf", &too_heavy).document;
    assert_eq!(read_preview(&env.conn, &env.files(), doc.id).err(), Some(AppError::InvalidInput));
}

#[test]
fn a_preview_whose_file_is_gone_or_changed_is_not_given() {
    let env = Env::new();
    let doc = env.add("Receipt.pdf", PDF).document;
    let path = env.path_of(&doc);
    let sealed = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(read_preview(&env.conn, &env.files(), doc.id).err(), Some(AppError::FileMissing));
    let mut changed = sealed;
    let at = changed.len() - 5;
    changed[at] ^= 1;
    fs::write(&path, changed).unwrap();
    assert_eq!(read_preview(&env.conn, &env.files(), doc.id).err(), Some(AppError::FileDamaged));
}

#[test]
fn an_image_whose_file_is_gone_or_changed_is_not_given() {
    let env = Env::new();
    let image = env.add("photo.png", &png()).document;
    let path = env.path_of(&image);
    let sealed = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(read_image(&env.conn, &env.files(), image.id).err(), Some(AppError::FileMissing));
    let mut changed = sealed;
    let at = changed.len() - 5;
    changed[at] ^= 1;
    fs::write(&path, changed).unwrap();
    assert_eq!(read_image(&env.conn, &env.files(), image.id).err(), Some(AppError::FileDamaged));
}

#[test]
fn a_file_that_is_not_known_is_never_opened_from_the_app() {
    let env = Env::new();
    let opened = super::opened::Opened::new(env.dir_path());
    for name in ["setup.exe", "run.sh", "holiday.jpg", "Receipt.pdf"] {
        let doc = env.add(name, &[b"MZ\x90\x00 not a thing ".as_slice(), name.as_bytes()].concat()).document;
        assert_eq!(doc.media_type, "application/octet-stream", "{name}");
        assert_eq!(open_copy(&env.conn, &env.files(), &opened, doc.id).err(), Some(AppError::InvalidInput), "{name}");
    }
    assert!(!env.dir_path().join("open").exists(), "no copy was made");
    // it can still be read, to be saved as a copy by the person
    let doc = env.add("keep.bin", b"MZ\x90\x00 keep me").document;
    assert_eq!(read(&env.conn, &env.files(), doc.id).unwrap().as_slice(), b"MZ\x90\x00 keep me");
}
