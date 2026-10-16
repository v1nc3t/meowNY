use std::fs;

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use super::library::*;
use super::seal::{DocError, DocId, DocKeys, MAX_DOCUMENT_BYTES};
use super::store::Store;
use crate::db::DbKey;
use crate::keys;
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
fn adding_makes_the_row_and_the_file_together() {
    let env = Env::new();
    let outcome = env.add("/tmp/downloads/Receipt.pdf", PDF);
    assert!(!outcome.already_there);
    let doc = outcome.document;
    assert_eq!(doc.name, "Receipt.pdf");
    assert_eq!(doc.media_type, "application/pdf");
    assert_eq!(doc.size_bytes, PDF.len() as u64);
    assert_eq!(doc.sha256, keys::hex_encode(&Sha256::digest(PDF)));
    assert_eq!(doc.document_date, TODAY, "today when no date is given");
    assert!(!doc.keep_on_phone, "off when a document is added");
    assert_eq!((doc.link_count, doc.has_file), (0, true));

    assert_eq!(env.files_on_disk().len(), 1);
    assert!(env.path_of(&doc).exists());
    assert_eq!(read(&env.conn, &env.files(), doc.id).unwrap().as_slice(), PDF);
    assert_eq!(get(&env.conn, &env.files(), doc.id).unwrap(), doc);
}

#[test]
fn a_date_the_switch_and_the_links_can_be_given_when_adding() {
    let env = Env::new();
    let (tx, rec) = (env.transaction("Boiler"), env.recurring("Rent"));
    let input = NewDocument {
        name: "Boiler invoice.pdf".into(),
        document_date: Some("2026-03-04".into()),
        keep_on_phone: true,
        links: vec![transaction_target(tx), recurring_target(rec)],
    };
    let doc = add(&env.conn, &env.files(), &input, PDF, TODAY).unwrap().document;
    assert_eq!((doc.document_date.as_str(), doc.keep_on_phone, doc.link_count), ("2026-03-04", true, 2));
    let links = links_of(&env.conn, doc.id).unwrap();
    assert_eq!(links.len(), 2);
    assert!(links.iter().any(|l| l.kind == TargetKind::Transaction && l.id == tx && l.name == "Boiler" && l.date == "2026-01-10"));
    assert!(links.iter().any(|l| l.kind == TargetKind::Recurring && l.id == rec && l.name == "Rent" && l.date == "2026-02-01"));
}

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
fn a_document_is_renamed_re_dated_and_switched() {
    let env = Env::new();
    let doc = env.add("scan0001.pdf", PDF).document;
    let stamp = || -> String { env.conn.query_row("SELECT stamp FROM documents", [], |r| r.get(0)).unwrap() };
    let before = stamp();

    let patch = DocumentPatch { name: Some("  Boiler / warranty  ".into()), document_date: Some("2026-02-03".into()), keep_on_phone: Some(true) };
    let changed = update(&env.conn, &env.files(), doc.id, &patch).unwrap();
    assert_eq!((changed.name.as_str(), changed.document_date.as_str(), changed.keep_on_phone), ("Boiler - warranty", "2026-02-03", true));
    assert_eq!((changed.sha256.as_str(), changed.size_bytes), (doc.sha256.as_str(), doc.size_bytes));
    assert!(stamp() > before);

    // asking for what is there already changes nothing, not even the stamp
    let now = stamp();
    update(&env.conn, &env.files(), doc.id, &DocumentPatch::default()).unwrap();
    update(&env.conn, &env.files(), doc.id, &DocumentPatch { keep_on_phone: Some(true), ..Default::default() }).unwrap();
    assert_eq!(stamp(), now);
}

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

fn named(page: &Page) -> Vec<&str> {
    page.documents.iter().map(|d| d.name.as_str()).collect()
}

fn dated(env: &Env, name: &str, bytes: &[u8], date: &str) -> Document {
    let input = NewDocument { name: name.into(), document_date: Some(date.into()), ..Default::default() };
    add(&env.conn, &env.files(), &input, bytes, TODAY).unwrap().document
}

fn library() -> Env {
    let env = Env::new();
    dated(&env, "Rent contract.pdf", b"%PDF-1.7 rent", "2026-01-05");
    dated(&env, "Boiler photo.png", &png(), "2026-02-10");
    dated(&env, "100% cotton_shirt.pdf", b"%PDF-1.7 shirt", "2026-02-10");
    dated(&env, "Totals.csv", b"a,b\n1,2\n", "2026-03-01");
    dated(&env, "Odd thing", b"MZ\x90\x00 odd", "2025-12-31");
    env
}

#[test]
fn the_list_is_newest_paper_first_and_then_newest_added() {
    let env = library();
    let page = list(&env.conn, &env.files(), &Filter::default()).unwrap();
    assert_eq!(named(&page), ["Totals.csv", "100% cotton_shirt.pdf", "Boiler photo.png", "Rent contract.pdf", "Odd thing"]);
    assert_eq!((page.total, page.limit, page.offset), (5, DEFAULT_LIMIT, 0));
}

#[test]
fn the_list_can_be_searched_and_filtered() {
    let env = library();
    let find = |filter: Filter| -> Vec<String> { named(&list(&env.conn, &env.files(), &filter).unwrap()).into_iter().map(String::from).collect() };

    assert_eq!(find(Filter { q: Some("BOILER".into()), ..Default::default() }), ["Boiler photo.png"], "ignoring ASCII case");
    assert_eq!(find(Filter { q: Some("100%".into()), ..Default::default() }), ["100% cotton_shirt.pdf"], "a percent sign is not a wildcard");
    assert_eq!(find(Filter { q: Some("%".into()), ..Default::default() }), ["100% cotton_shirt.pdf"]);
    // "o_t" would match "ott" in "cotton" if the underscore were a wildcard
    assert_eq!(find(Filter { q: Some("o_t".into()), ..Default::default() }), Vec::<String>::new(), "an underscore is not a wildcard");
    assert_eq!(find(Filter { q: Some("cotton_s".into()), ..Default::default() }), ["100% cotton_shirt.pdf"]);
    assert_eq!(find(Filter { q: Some("  ".into()), ..Default::default() }).len(), 5, "a blank search is no search");

    assert_eq!(find(Filter { kind: Some(Kind::Image), ..Default::default() }), ["Boiler photo.png"]);
    assert_eq!(find(Filter { kind: Some(Kind::Pdf), ..Default::default() }), ["100% cotton_shirt.pdf", "Rent contract.pdf"]);
    assert_eq!(find(Filter { kind: Some(Kind::Other), ..Default::default() }), ["Totals.csv", "Odd thing"]);

    assert_eq!(find(Filter { from: Some("2026-02-10".into()), to: Some("2026-02-10".into()), ..Default::default() }), ["100% cotton_shirt.pdf", "Boiler photo.png"]);
    assert_eq!(find(Filter { from: Some("2026-02-11".into()), ..Default::default() }), ["Totals.csv"]);
    assert_eq!(find(Filter { to: Some("2026-01-05".into()), ..Default::default() }), ["Rent contract.pdf", "Odd thing"]);
    assert_eq!(find(Filter { q: Some("o".into()), kind: Some(Kind::Pdf), from: Some("2026-01-01".into()), ..Default::default() }).len(), 2);
}

#[test]
fn the_list_can_be_limited_to_what_is_linked_to_one_thing() {
    let env = library();
    let (tx, rec) = (env.transaction("Boiler"), env.recurring("Rent"));
    let id_of = |name: &str| -> i64 { env.conn.query_row("SELECT id FROM documents WHERE name = ?1", [name], |r| r.get(0)).unwrap() };
    link(&env.conn, id_of("Boiler photo.png"), &transaction_target(tx)).unwrap();
    link(&env.conn, id_of("Totals.csv"), &transaction_target(tx)).unwrap();
    link(&env.conn, id_of("Rent contract.pdf"), &recurring_target(rec)).unwrap();

    let by_tx = list(&env.conn, &env.files(), &Filter { transaction_id: Some(tx.to_string()), ..Default::default() }).unwrap();
    assert_eq!((named(&by_tx), by_tx.total), (vec!["Totals.csv", "Boiler photo.png"], 2));
    let by_rec = list(&env.conn, &env.files(), &Filter { recurring_transaction_id: Some(rec.to_string()), ..Default::default() }).unwrap();
    assert_eq!(named(&by_rec), ["Rent contract.pdf"]);
    let none = list(&env.conn, &env.files(), &Filter { transaction_id: Some("9999".into()), ..Default::default() }).unwrap();
    assert_eq!(none.total, 0);
}

#[test]
fn the_list_is_paged_and_says_how_many_there_are() {
    let env = library();
    let page = |limit, offset| list(&env.conn, &env.files(), &Filter { limit: Some(limit), offset: Some(offset), ..Default::default() }).unwrap();
    assert_eq!(named(&page(2, 0)), ["Totals.csv", "100% cotton_shirt.pdf"]);
    assert_eq!(named(&page(2, 2)), ["Boiler photo.png", "Rent contract.pdf"]);
    let last = page(2, 4);
    assert_eq!((named(&last), last.total), (vec!["Odd thing"], 5));
    assert!(page(2, 9).documents.is_empty());
    for bad in [0, MAX_LIMIT + 1] {
        assert_eq!(list(&env.conn, &env.files(), &Filter { limit: Some(bad), ..Default::default() }).err(), Some(AppError::InvalidInput));
    }
    assert_eq!(list(&env.conn, &env.files(), &Filter { from: Some("soon".into()), ..Default::default() }).err(), Some(AppError::InvalidInput));
    assert_eq!(list(&env.conn, &env.files(), &Filter { q: Some("x".repeat(121)), ..Default::default() }).err(), Some(AppError::InvalidInput));
}

#[test]
fn the_list_says_which_documents_have_no_file_on_this_device() {
    let env = library();
    let gone = env.conn.query_row("SELECT id FROM documents WHERE name = 'Totals.csv'", [], |r| r.get::<_, i64>(0)).unwrap();
    let doc = get(&env.conn, &env.files(), gone).unwrap();
    fs::remove_file(env.path_of(&doc)).unwrap();
    let page = list(&env.conn, &env.files(), &Filter::default()).unwrap();
    for d in &page.documents {
        assert_eq!(d.has_file, d.name != "Totals.csv", "{}", d.name);
    }
}

#[test]
fn the_size_of_the_library_is_the_sum_of_the_files() {
    let env = Env::new();
    assert_eq!(library_size(&env.conn, &env.files()).unwrap(), Size { count: 0, bytes: 0, held_count: 0, held_bytes: 0 });
    env.add("a.pdf", b"%PDF-1.7 one");
    let lost = env.add("b.pdf", b"%PDF-1.7 and two").document;
    assert_eq!(library_size(&env.conn, &env.files()).unwrap(), Size { count: 2, bytes: 12 + 16, held_count: 2, held_bytes: 12 + 16 });
    // a phone that keeps only some: what it holds is not what there is
    fs::remove_file(env.path_of(&lost)).unwrap();
    assert_eq!(library_size(&env.conn, &env.files()).unwrap(), Size { count: 2, bytes: 12 + 16, held_count: 1, held_bytes: 12 });
}

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

#[test]
fn what_is_sent_to_the_screens_has_the_fields_they_expect() {
    let env = Env::new();
    let doc = env.add("Receipt.pdf", PDF).document;
    let json = serde_json::to_value(&doc).unwrap();
    assert_eq!(json["id"], doc.id.to_string(), "ids are text, as everywhere");
    for key in ["name", "mediaType", "sizeBytes", "sha256", "documentDate", "keepOnPhone", "createdAt", "linkCount", "hasFile"] {
        assert!(json.get(key).is_some(), "{key}");
    }
    assert_eq!(json.as_object().unwrap().len(), 10, "no sync id and nothing else leaves");
    assert!(serde_json::from_str::<NewDocument>(r#"{"name":"x","unknown":1}"#).is_err());
    assert!(serde_json::from_str::<Filter>(r#"{"kind":"video"}"#).is_err());
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
fn the_copy_for_the_viewer_is_named_for_what_the_file_is() {
    let env = Env::new();
    let opened = super::opened::Opened::new(env.dir_path());
    let cases: [(&str, Vec<u8>, &str); 4] = [
        ("Receipt March.pdf", PDF.to_vec(), "Receipt March.pdf"),
        ("holiday.bat", png(), "holiday.png"),
        ("invoice.pdf.exe", [PDF, b"% a second file, not the same bytes\n"].concat(), "invoice.pdf.pdf"),
        ("notes.html", b"<html>hi</html>".to_vec(), "notes.html"),
    ];
    for (name, bytes, expected) in cases {
        let doc = env.add(name, &bytes).document;
        let path = open_copy(&env.conn, &env.files(), &opened, doc.id).unwrap();
        assert_eq!(path.file_name().unwrap().to_string_lossy(), expected, "{name}");
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(path.starts_with(env.dir_path().join("open")));
    }
    assert!(opened.clear());
    assert!(!env.dir_path().join("open").exists());
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

#[test]
fn a_description_is_checked_without_a_file() {
    assert!(check_input(&Env::input("x.pdf")).is_ok());
    let dated = |d: &str| NewDocument { document_date: Some(d.into()), ..Env::input("x") };
    assert!(check_input(&dated("2026-10-16")).is_ok());
    for bad in ["", "yesterday", "2026-1-1", "2026-02-30"] {
        assert_eq!(check_input(&dated(bad)), Err(AppError::InvalidInput), "{bad:?}");
    }
}

#[test]
fn the_documents_linked_to_each_item_of_a_list_are_counted() {
    let env = Env::new();
    let (t1, t2, t3, rec) = (env.transaction("One"), env.transaction("Two"), env.transaction("Three"), env.recurring("Rent"));
    let linked = |name: &str, bytes: &[u8], targets: Vec<LinkTarget>| {
        add(&env.conn, &env.files(), &NewDocument { name: name.into(), links: targets, ..Default::default() }, bytes, TODAY).unwrap();
    };
    linked("a.pdf", b"%PDF-1.7 a", vec![transaction_target(t1), transaction_target(t2), recurring_target(rec)]);
    linked("b.pdf", b"%PDF-1.7 b", vec![transaction_target(t1)]);
    linked("c.pdf", b"%PDF-1.7 c", vec![]);
    let ids = |list: &[i64]| list.iter().map(i64::to_string).collect::<Vec<_>>();

    let counts = link_counts(&env.conn, TargetKind::Transaction, &ids(&[t1, t2, t3])).unwrap();
    assert_eq!(counts.len(), 2, "an item with no document is not in the answer");
    assert_eq!((counts[&t1.to_string()], counts[&t2.to_string()]), (2, 1));
    let counts = link_counts(&env.conn, TargetKind::Recurring, &ids(&[rec, t1])).unwrap();
    assert_eq!(counts.into_iter().collect::<Vec<_>>(), vec![(rec.to_string(), 1)], "a transaction id is not a recurring id");
    assert!(link_counts(&env.conn, TargetKind::Transaction, &[]).unwrap().is_empty());
    assert_eq!(link_counts(&env.conn, TargetKind::Transaction, &["abc".to_string()]).err(), Some(AppError::InvalidInput));
    assert_eq!(link_counts(&env.conn, TargetKind::Transaction, &vec!["1".to_string(); 201]).err(), Some(AppError::InvalidInput));
}
