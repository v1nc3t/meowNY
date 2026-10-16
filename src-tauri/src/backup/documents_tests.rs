//! Backups with the files of the documents: written from a vault, restored into a running one and into a new install with keys of
//! its own, and what a damaged, cut or hostile file does (nothing: the data and the folder stay as they were).

use std::fs;
use std::path::Path;

use super::stream::{ChunkWriter, CHUNK, TAG};
use super::tests;
use super::*;
use crate::documents::library::{self, NewDocument};
use crate::documents::store::DIR_NAME;
use crate::keys::memory::MemoryStore;
use crate::vault::{Settings, VaultStatus};

const PASS: &str = "backup passphrase";
const SEALED: usize = CHUNK + TAG;

fn settings() -> Settings {
    Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "UTC".into(), language: "en".into() }
}

fn vault(dir: &Path) -> Vault {
    let v = Vault::new(dir.to_path_buf(), Box::new(MemoryStore::default()));
    v.create(&settings(), None).unwrap();
    v
}

fn empty_install(dir: &Path) -> Vault {
    Vault::new(dir.to_path_buf(), Box::new(MemoryStore::default()))
}

fn pattern(n: usize, seed: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 31 + seed * 7 + i / 251) % 251) as u8).collect()
}

fn add(v: &Vault, name: &str, bytes: &[u8]) -> i64 {
    v.with_files(|conn, files| {
        let input = NewDocument { name: name.into(), ..NewDocument::default() };
        Ok(library::add(conn, files, &input, bytes, "2026-03-02")?.document.id)
    })
    .unwrap()
}

fn read(v: &Vault, id: i64) -> Vec<u8> {
    v.with_files(|conn, files| Ok(library::read(conn, files, id)?.to_vec())).unwrap()
}

fn sync_id(v: &Vault, id: i64) -> String {
    v.with_conn(|c| Ok(c.query_row("SELECT sync_id FROM documents WHERE id = ?1", [id], |r| r.get(0))?)).unwrap()
}

/// The names in the documents folder, sorted.
fn folder(v: &Vault) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(v.data_dir().join(DIR_NAME)) {
        Ok(entries) => entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

fn snapshot_of(v: &Vault) -> Snapshot {
    v.with_conn(|c| dump(c)).unwrap()
}

/// A backup in memory, with or without the files.
fn backup_of(v: &Vault, documents: bool) -> (Vec<u8>, Written) {
    let mut files = VaultFiles(v);
    let (bytes, written) = write_stream(Vec::new(), &snapshot_of(v), PASS, documents.then_some(&mut files as &mut dyn FileSource)).unwrap();
    (bytes, written)
}

/// A vault with documents of awkward sizes: one byte, exactly a chunk, three chunks and a bit.
fn library_of(dir: &Path) -> (Vault, Vec<(i64, Vec<u8>)>) {
    let v = vault(dir);
    let mut docs = Vec::new();
    for (name, size) in [("one.bin", 1), ("chunk.bin", CHUNK), ("big.bin", 3 * CHUNK + 17)] {
        let bytes = pattern(size, docs.len());
        docs.push((add(&v, name, &bytes), bytes));
    }
    (v, docs)
}

/// A vault with its own data, which a failed restore must leave alone: a document with a file and a category.
fn bystander(dir: &Path) -> Vault {
    let v = vault(dir);
    add(&v, "mine.bin", &pattern(5000, 99));
    v.with_conn(|c| crate::categories::create(c, "Mine", crate::categories::CategoryType::Expense)).unwrap();
    v
}

fn unchanged(v: &Vault, before: &(Snapshot, Vec<String>), what: &str) {
    assert_eq!(&snapshot_of(v), &before.0, "the data changed: {what}");
    assert_eq!(&folder(v), &before.1, "the folder changed: {what}");
}

fn state(v: &Vault) -> (Snapshot, Vec<String>) {
    (snapshot_of(v), folder(v))
}

// --- the round trip -----------------------------------------------------------------------------------------------------

#[test]
fn a_backup_with_documents_comes_back_into_a_new_install_with_keys_of_its_own() {
    let (da, db) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, docs) = library_of(da.path());
    let (bytes, written) = backup_of(&a, true);
    assert_eq!(written, Written { documents: 3, missing: 0 });

    let b = empty_install(db.path());
    b.create_from_backup(&bytes, PASS, None).unwrap();
    for (id, original) in &docs {
        assert_eq!(&read(&b, *id), original, "document {id}");
    }
    assert_eq!(snapshot_of(&b), snapshot_of(&a), "every row");
    assert_eq!(folder(&b).len(), 3);
    assert!(folder(&a).iter().all(|name| !folder(&b).contains(name)), "sealed again under the new vault's key, so the names are not the same");
    assert_eq!(b.check_documents().unwrap(), library::Report::default(), "nothing missing, damaged or stray");
}

#[test]
fn restoring_into_a_running_vault_replaces_its_files_with_the_backups() {
    let (da, db) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, docs) = library_of(da.path());
    let (bytes, _) = backup_of(&a, true);

    let b = bystander(db.path());
    assert_eq!(folder(&b).len(), 1);
    b.restore_backup(&bytes, PASS, "RESTORE").unwrap();
    assert_eq!(snapshot_of(&b), snapshot_of(&a));
    assert_eq!(folder(&b).len(), 3, "the file of the document that is gone is gone with it");
    for (id, original) in &docs {
        assert_eq!(&read(&b, *id), original);
    }

    // restoring the same backup again over data that has the same files and one more document of its own
    let extra = add(&b, "extra.bin", &pattern(777, 5));
    let before = folder(&b);
    assert_eq!(before.len(), 4);
    b.restore_backup(&bytes, PASS, "RESTORE").unwrap();
    assert_eq!(folder(&b).len(), 3, "the extra one went");
    assert!(b.with_files(|c, f| library::get(c, f, extra)).is_err());
    for (id, original) in &docs {
        assert_eq!(&read(&b, *id), original);
    }
}

#[test]
fn a_half_received_file_left_by_a_stopped_sync_does_not_confuse_a_restore() {
    let (da, db) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, docs) = library_of(da.path());
    let (bytes, _) = backup_of(&a, true);
    // the target has the rows of the same documents and, for the big one, the start of its file from a transfer that stopped
    let b = empty_install(db.path());
    b.create_from_backup(&backup_of(&a, false).0, PASS, None).unwrap();
    let big = DocId::parse(&sync_id(&b, docs[2].0)).unwrap();
    b.with_files(|_, files| {
        let mut incoming = files.store.begin_incoming(files.keys, &big, docs[2].1.len() as u64)?;
        // a start that is not this file's (a stopped transfer of anything else would be refused by the hash at the end)
        incoming.write(&pattern(CHUNK + 100, 77))?;
        Ok(())
    })
    .unwrap();
    assert_eq!(b.check_documents().unwrap().missing, 3);
    b.restore_backup(&bytes, PASS, "RESTORE").unwrap();
    assert_eq!(read(&b, docs[2].0), docs[2].1);
    assert_eq!(folder(&b).len(), 3, "and no unfinished file is left");
}

#[test]
fn a_backup_without_documents_restores_the_rows_and_leaves_the_files_missing_or_as_they_are() {
    let (da, db, dc) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, docs) = library_of(da.path());
    let (lean, written) = backup_of(&a, false);
    let (full, _) = backup_of(&a, true);
    assert_eq!(written, Written::default());
    assert!(lean.len() * 10 < full.len(), "{} against {}", lean.len(), full.len());

    // a new install: the rows come and the files are missing, to be filled by the phone at the next sync
    let b = empty_install(db.path());
    b.create_from_backup(&lean, PASS, None).unwrap();
    assert_eq!(snapshot_of(&b), snapshot_of(&a));
    assert!(folder(&b).is_empty());
    assert_eq!(b.check_documents().unwrap().missing, 3);

    // a vault that already holds these files (the same documents, as a restore of the full backup gave it) keeps them
    let c = empty_install(dc.path());
    c.create_from_backup(&full, PASS, None).unwrap();
    let held = folder(&c);
    c.restore_backup(&lean, PASS, "RESTORE").unwrap();
    assert_eq!(folder(&c), held);
    assert_eq!(c.check_documents().unwrap().missing, 0);
    assert_eq!(read(&c, docs[2].0), docs[2].1);
}

#[test]
fn a_document_with_no_good_file_is_left_out_of_the_backup_and_counted() {
    let dir = tempfile::tempdir().unwrap();
    let (a, docs) = library_of(dir.path());
    // the first has lost its file, the second's no longer opens
    let name_of = |id| {
        let doc = DocId::parse(&sync_id(&a, id)).unwrap();
        a.with_files(|_, f| Ok(f.keys.file_name(&doc))).unwrap()
    };
    let docs_dir = a.data_dir().join(DIR_NAME);
    fs::remove_file(docs_dir.join(name_of(docs[0].0))).unwrap();
    let mut sealed = fs::read(docs_dir.join(name_of(docs[1].0))).unwrap();
    let last = sealed.len() - 1;
    sealed[last] ^= 1;
    fs::write(docs_dir.join(name_of(docs[1].0)), sealed).unwrap();

    let (bytes, written) = backup_of(&a, true);
    assert_eq!(written, Written { documents: 1, missing: 2 });
    let other = tempfile::tempdir().unwrap();
    let b = empty_install(other.path());
    b.create_from_backup(&bytes, PASS, None).unwrap();
    assert_eq!(read(&b, docs[2].0), docs[2].1);
    assert_eq!(b.check_documents().unwrap().missing, 2, "the rows are all there");
}

#[test]
fn a_file_of_many_chunks_goes_through_a_backup_from_a_path_to_a_path() {
    let (da, db) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let a = vault(da.path());
    let bytes = pattern(2 * 1024 * 1024 + 321, 3);
    let id = add(&a, "many.bin", &bytes);
    let path = da.path().join("many.meowbak");
    let written = create_file(&path, &snapshot_of(&a), PASS, Some(&mut VaultFiles(&a))).unwrap();
    assert_eq!(written.documents, 1);
    let b = empty_install(db.path());
    b.create_from_backup(path.as_path(), PASS, None).unwrap();
    assert!(read(&b, id) == bytes);
}

/// Takes half a minute in a debug build, so it runs only when asked (`cargo test -- --ignored`).
#[test]
#[ignore = "slow in a debug build"]
fn the_largest_file_goes_through_a_backup() {
    let (da, db) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let a = vault(da.path());
    let bytes = pattern(MAX_DOCUMENT_BYTES as usize, 3);
    let id = add(&a, "largest.bin", &bytes);
    let path = da.path().join("big.meowbak");
    let written = create_file(&path, &snapshot_of(&a), PASS, Some(&mut VaultFiles(&a))).unwrap();
    assert_eq!(written.documents, 1);
    assert!(fs::metadata(&path).unwrap().len() > MAX_DOCUMENT_BYTES);

    let b = empty_install(db.path());
    b.create_from_backup(path.as_path(), PASS, None).unwrap();
    assert!(read(&b, id) == bytes);
}

// --- a file that is cut, changed or rearranged -----------------------------------------------------------------------------

/// A backup of two files (about three chunks), and a vault of someone else's data to restore it into.
fn two_files() -> (Vec<u8>, Vec<tempfile::TempDir>) {
    let dir = tempfile::tempdir().unwrap();
    let a = vault(dir.path());
    add(&a, "first.bin", &pattern(100_000, 1));
    add(&a, "second.bin", &pattern(70_000, 2));
    let (bytes, _) = backup_of(&a, true);
    assert!(bytes.len() > 2 * SEALED + HEADER_LEN, "the file has several chunks");
    (bytes, vec![dir])
}

fn refused_and_unchanged(bytes: &[u8], cases: Vec<(String, Vec<u8>)>) {
    let _ = bytes;
    let dir = tempfile::tempdir().unwrap();
    let b = bystander(dir.path());
    let before = state(&b);
    for (what, file) in cases {
        assert!(b.restore_backup(&file, PASS, "RESTORE").is_err(), "{what}");
        unchanged(&b, &before, &what);
        assert_eq!(read(&b, 1), pattern(5000, 99), "{what}");
    }
}

#[test]
fn a_backup_cut_anywhere_is_refused_and_changes_nothing() {
    let (bytes, _keep) = two_files();
    let mut cuts: Vec<usize> = vec![0, 1, 8, HEADER_LEN - 1, HEADER_LEN, HEADER_LEN + 1, HEADER_LEN + TAG, bytes.len() - 1];
    for k in 1..=(bytes.len() - HEADER_LEN) / SEALED {
        let boundary = HEADER_LEN + k * SEALED;
        cuts.extend([boundary - 1, boundary, boundary + 1]);
    }
    cuts.extend((HEADER_LEN..bytes.len()).step_by(17_011));
    cuts.retain(|c| *c < bytes.len());
    cuts.sort();
    cuts.dedup();
    let cases = cuts.iter().map(|c| (format!("cut at {c} of {}", bytes.len()), bytes[..*c].to_vec())).collect();
    refused_and_unchanged(&bytes, cases);
}

#[test]
fn a_changed_byte_anywhere_is_refused_and_changes_nothing() {
    let (bytes, _keep) = two_files();
    let mut positions: Vec<usize> = (0..HEADER_LEN).collect();
    let chunks = (bytes.len() - HEADER_LEN).div_ceil(SEALED);
    for k in 0..chunks {
        let start = HEADER_LEN + k * SEALED;
        let end = (start + SEALED).min(bytes.len());
        positions.extend([start, start + 1, (start + end) / 2, end - TAG - 1, end - TAG, end - 1]);
    }
    positions.extend((HEADER_LEN..bytes.len()).step_by(9_001));
    positions.sort();
    positions.dedup();
    let cases = positions
        .iter()
        .map(|at| {
            let mut changed = bytes.clone();
            changed[*at] ^= 0x01;
            (format!("bit changed at {at}"), changed)
        })
        .collect();
    refused_and_unchanged(&bytes, cases);
}

#[test]
fn chunks_moved_dropped_repeated_or_added_are_refused_and_change_nothing() {
    let (bytes, _keep) = two_files();
    let body = &bytes[HEADER_LEN..];
    let chunk = |i: usize| body[i * SEALED..((i + 1) * SEALED).min(body.len())].to_vec();
    let count = body.len().div_ceil(SEALED);
    assert!(count >= 3);
    let make = |parts: Vec<Vec<u8>>| [bytes[..HEADER_LEN].to_vec(), parts.concat()].concat();
    let all: Vec<Vec<u8>> = (0..count).map(chunk).collect();
    let mut swapped = all.clone();
    swapped.swap(0, 1);
    let mut dropped = all.clone();
    dropped.remove(1);
    let mut repeated = all.clone();
    repeated.insert(1, chunk(1));
    let mut added = all.clone();
    added.push(chunk(0));
    let mut garbage = all.clone();
    garbage.push(vec![9u8; 40]);
    let mut last_first = all.clone();
    last_first.rotate_right(1);
    let cases = vec![
        ("swapped".to_string(), make(swapped)),
        ("dropped".to_string(), make(dropped)),
        ("repeated".to_string(), make(repeated)),
        ("added a chunk".to_string(), make(added)),
        ("added garbage".to_string(), make(garbage)),
        ("the last chunk first".to_string(), make(last_first)),
    ];
    refused_and_unchanged(&bytes, cases);
}

#[test]
fn a_wrong_passphrase_is_told_from_damage_after_the_first_chunk() {
    let (bytes, _keep) = two_files();
    let dir = tempfile::tempdir().unwrap();
    let b = bystander(dir.path());
    let before = state(&b);
    assert_eq!(b.restore_backup(&bytes, "another passphrase", "RESTORE"), Err(AppError::WrongCredentials));
    let mut damaged = bytes.clone();
    damaged[HEADER_LEN + SEALED + 100] ^= 1;
    assert_eq!(b.restore_backup(&damaged, PASS, "RESTORE"), Err(AppError::InvalidBackup));
    unchanged(&b, &before, "wrong passphrase and damage");
}

// --- records that a good file does not hold ---------------------------------------------------------------------------------

/// A file that the app would not write: the rows of `source` and the records given, then an end record if `end`.
fn forge(tables: &serde_json::Value, records: &[Vec<u8>], end: bool) -> Vec<u8> {
    let kdf = Kdf::DEFAULT;
    let salt = [3u8; SALT_LEN];
    let head = header(FORMAT, migrate::latest_version(), kdf, &salt, &[4u8; NONCE_LEN]);
    let key = stretch(PASS, &salt, kdf).unwrap();
    let rows = tables.to_string().into_bytes();
    let mut stream = ChunkWriter::new(head.clone(), &key, head);
    stream.write(&[RECORD_SNAPSHOT]).unwrap();
    stream.write(&(rows.len() as u64).to_be_bytes()).unwrap();
    stream.write(&rows).unwrap();
    for record in records {
        stream.write(record).unwrap();
    }
    if end {
        stream.write(&[RECORD_END]).unwrap();
    }
    stream.finish().unwrap()
}

fn file_record(id: &[u8; 16], size: u64, sha256: &[u8; 32], body: &[u8]) -> Vec<u8> {
    [&[RECORD_FILE][..], id, &size.to_be_bytes(), sha256, body].concat()
}

#[test]
fn a_file_record_that_no_row_names_or_that_does_not_match_is_refused_and_nothing_stays_behind() {
    let dir = tempfile::tempdir().unwrap();
    let a = vault(dir.path());
    let bytes = pattern(150_000, 8);
    let id = add(&a, "doc.bin", &bytes);
    let tables = serde_json::to_value(&snapshot_of(&a).tables).unwrap();
    let doc = DocId::parse(&sync_id(&a, id)).unwrap();
    let sha = crate::documents::store::sha256(&bytes);
    let size = bytes.len() as u64;
    let good = file_record(doc.bytes(), size, &sha, &bytes);

    let mut other_body = bytes.clone();
    other_body[70_000] ^= 0x40;
    let mut other_sha = sha;
    other_sha[0] ^= 1;
    let stranger = DocId::from_bytes([0xee; 16]);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("an id no row names", forge(&tables, &[file_record(stranger.bytes(), size, &sha, &bytes)], true)),
        ("another size than the row", forge(&tables, &[file_record(doc.bytes(), size - 1, &sha, &bytes[..bytes.len() - 1])], true)),
        ("another hash than the row", forge(&tables, &[file_record(doc.bytes(), size, &other_sha, &bytes)], true)),
        ("bytes that do not hash to the row", forge(&tables, &[file_record(doc.bytes(), size, &sha, &other_body)], true)),
        ("the same file twice", forge(&tables, &[good.clone(), good.clone()], true)),
        ("a size of nothing", forge(&tables, &[file_record(doc.bytes(), 0, &sha, &[])], true)),
        ("a size over the largest file", forge(&tables, &[file_record(doc.bytes(), MAX_DOCUMENT_BYTES + 1, &sha, &[])], true)),
        ("a record of a kind nobody defined", forge(&tables, &[[&b"X"[..], &[0u8; 30]].concat()], true)),
        ("no end record", forge(&tables, std::slice::from_ref(&good), false)),
        ("a record after the end", forge(&tables, &[good.clone(), vec![RECORD_END], good.clone()], false)),
        ("fewer bytes than announced", forge(&tables, &[file_record(doc.bytes(), size, &sha, &bytes[..1000])], true)),
        ("a file that stops at the end of its record", forge(&tables, &[file_record(doc.bytes(), size, &sha, &bytes[..bytes.len() - 1])], true)),
    ];
    let cases = cases.into_iter().map(|(what, file)| (what.to_string(), file)).collect();
    refused_and_unchanged(&[], cases);

    // reading a file only for its rows (`open`) refuses the same kinds of record, whatever the rows say
    let nothing = file_record(doc.bytes(), 0, &sha, &[]);
    let too_big = file_record(doc.bytes(), MAX_DOCUMENT_BYTES + 1, &sha, &[]);
    for (what, record) in [("a size of nothing", nothing), ("a size over the largest file", too_big)] {
        assert_eq!(open(&forge(&tables, &[record], true), PASS), Err(AppError::InvalidBackup), "{what}");
    }
    assert!(open(&forge(&tables, std::slice::from_ref(&good), true), PASS).is_ok());

    // the same rows with the honest record are restored
    let honest = forge(&tables, &[good], true);
    let target = tempfile::tempdir().unwrap();
    let b = bystander(target.path());
    b.restore_backup(&honest, PASS, "RESTORE").unwrap();
    assert_eq!(read(&b, id), bytes);
}

#[test]
fn a_cut_backup_restored_as_a_new_install_leaves_the_app_not_set_up() {
    let (bytes, _keep) = two_files();
    let dir = tempfile::tempdir().unwrap();
    let v = empty_install(dir.path());
    assert!(v.create_from_backup(&bytes[..bytes.len() - 100], PASS, None).is_err());
    assert_eq!(v.status(), VaultStatus::NotSetUp);
    assert!(folder(&v).is_empty(), "no file of the half-read backup is left");
    // and the same install takes the whole file afterwards
    v.create_from_backup(&bytes, PASS, None).unwrap();
    assert_eq!(folder(&v).len(), 2);
}

// --- version 1 ---------------------------------------------------------------------------------------------------------------

#[test]
fn a_version_1_backup_still_opens_and_restores() {
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = library_of(dir.path());
    let snapshot = snapshot_of(&a);
    let old = seal_v1(&snapshot, PASS).unwrap();
    assert_eq!(u16::from_be_bytes([old[8], old[9]]), FORMAT_1);
    assert_eq!(open(&old, PASS).unwrap(), snapshot);
    assert_eq!(open(&seal(&snapshot, PASS).unwrap(), PASS).unwrap(), snapshot, "and a new one holds the same rows");

    let mut backup = Backup::open(&old, PASS).unwrap();
    assert_eq!(backup.snapshot(), &snapshot);
    let target = tempfile::tempdir().unwrap();
    let b = empty_install(target.path());
    b.create_from_backup(&old, PASS, None).unwrap();
    assert_eq!(snapshot_of(&b), snapshot);
    assert!(folder(&b).is_empty(), "version 1 holds no files");
    let prepared = prepare(&snapshot).unwrap();
    let count = b.with_files(|_, files| backup.restore_files(files, &prepared)).unwrap();
    assert_eq!(count, 0);

    assert_eq!(Backup::open(&old, "another passphrase").err(), Some(AppError::WrongCredentials));
    let mut changed = old.clone();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    assert_eq!(Backup::open(&changed, PASS).err(), Some(AppError::WrongCredentials));
    assert_eq!(Backup::open(&old[..old.len() - 1], PASS).err(), Some(AppError::WrongCredentials));
}

#[test]
fn a_version_1_file_over_the_ceiling_is_refused_before_it_is_read() {
    let old = seal_v1(&dump(&tests::db()).unwrap(), PASS).unwrap();
    let length = MAX_FILE_BYTES + 1;
    let reader: Box<dyn Read> = Box::new(std::io::Cursor::new(old));
    assert_eq!(Backup::from_reader(reader, length, PASS).err(), Some(AppError::InvalidBackup));
}

// --- writing -------------------------------------------------------------------------------------------------------------------

struct Failing {
    after: u32,
    error: AppError,
    calls: u32,
}

impl FileSource for Failing {
    fn file(&mut self, _: &DocId, size: u64, _: &[u8; 32]) -> Result<Option<Zeroizing<Vec<u8>>>, AppError> {
        self.calls += 1;
        if self.calls > self.after {
            return Err(self.error);
        }
        Ok(Some(Zeroizing::new(vec![1u8; size as usize])))
    }
}

#[test]
fn a_backup_that_cannot_be_finished_leaves_nothing_and_an_older_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = library_of(dir.path());
    let path = dir.path().join("backup.meowbak");
    fs::write(&path, b"an older backup").unwrap();
    let mut failing = Failing { after: 1, error: AppError::Locked, calls: 0 };
    let result = create_file(&path, &snapshot_of(&a), PASS, Some(&mut failing));
    assert_eq!(result, Err(AppError::Locked));
    assert_eq!(fs::read(&path).unwrap(), b"an older backup");
    assert!(!dir.path().join("backup.meowbak.tmp").exists());
}

#[test]
fn a_vault_that_locks_in_the_middle_of_a_backup_ends_it_cleanly() {
    struct LocksAfterOne<'a>(VaultFiles<'a>, u32);
    impl FileSource for LocksAfterOne<'_> {
        fn file(&mut self, id: &DocId, size: u64, sha256: &[u8; 32]) -> Result<Option<Zeroizing<Vec<u8>>>, AppError> {
            self.1 += 1;
            if self.1 == 2 {
                (self.0).0.lock();
            }
            self.0.file(id, size, sha256)
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = library_of(dir.path());
    let path = dir.path().join("backup.meowbak");
    let snapshot = snapshot_of(&a);
    let result = create_file(&path, &snapshot, PASS, Some(&mut LocksAfterOne(VaultFiles(&a), 0)));
    assert_eq!(result, Err(AppError::Locked));
    assert!(!path.exists() && !dir.path().join("backup.meowbak.tmp").exists());
}

#[test]
fn bytes_of_the_wrong_length_are_left_out_not_written() {
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = library_of(dir.path());
    struct Short;
    impl FileSource for Short {
        fn file(&mut self, _: &DocId, size: u64, _: &[u8; 32]) -> Result<Option<Zeroizing<Vec<u8>>>, AppError> {
            Ok(Some(Zeroizing::new(vec![0u8; size as usize - 1])))
        }
    }
    let (bytes, written) = write_stream(Vec::new(), &snapshot_of(&a), PASS, Some(&mut Short)).unwrap();
    assert_eq!(written, Written { documents: 0, missing: 3 });
    assert_eq!(open(&bytes, PASS).unwrap(), snapshot_of(&a));
}

#[test]
fn a_backup_file_is_private_and_made_through_a_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = library_of(dir.path());
    let path = dir.path().join("backup.meowbak");
    create_file(&path, &snapshot_of(&a), PASS, Some(&mut VaultFiles(&a))).unwrap();
    assert!(!dir.path().join("backup.meowbak.tmp").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    // a place that cannot be written is an error, and the snapshot is not lost
    let missing = dir.path().join("no-such-folder").join("backup.meowbak");
    assert_eq!(create_file(&missing, &snapshot_of(&a), PASS, None), Err(AppError::Internal));
}

#[test]
fn a_short_passphrase_is_refused_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let a = vault(dir.path());
    let path = dir.path().join("backup.meowbak");
    assert_eq!(create_file(&path, &snapshot_of(&a), "short", None), Err(AppError::InvalidInput));
    assert!(!path.exists() && !dir.path().join("backup.meowbak.tmp").exists());
}

// --- scheduled backups --------------------------------------------------------------------------------------------------------

#[test]
fn scheduled_backups_hold_the_files_when_asked_and_the_choice_is_remembered() {
    use crate::schedule;
    let (da, folder_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, docs) = library_of(da.path());
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-14T09:30:00Z").unwrap().with_timezone(&chrono::Utc);
    schedule::start(&a, folder_dir.path(), PASS, 1, true, now).unwrap();
    assert!(a.with_conn(|c| schedule::get(c)).unwrap().unwrap().include_documents);

    let first = folder_dir.path().join("meowny-auto-2026-10-14-093000.meowbak");
    let other = tempfile::tempdir().unwrap();
    let b = empty_install(other.path());
    b.create_from_backup(first.as_path(), PASS, None).unwrap();
    assert_eq!(read(&b, docs[2].0), docs[2].1);

    // the next one, when it is due, also holds them, and a change of the choice is kept
    let later = now + chrono::Duration::days(2);
    schedule::run_if_due(&a, later);
    let second = folder_dir.path().join("meowny-auto-2026-10-16-093000.meowbak");
    assert!(fs::metadata(&second).unwrap().len() > 3 * CHUNK as u64);
    schedule::start(&a, folder_dir.path(), PASS, 1, false, later + chrono::Duration::days(2)).unwrap();
    assert!(!a.with_conn(|c| schedule::get(c)).unwrap().unwrap().include_documents);
    let lean = folder_dir.path().join("meowny-auto-2026-10-18-093000.meowbak");
    assert!(fs::metadata(&lean).unwrap().len() < CHUNK as u64);
}

#[test]
fn a_backup_made_while_forged_rows_name_a_file_that_is_not_there_does_not_fail() {
    // a row whose file was never here: the backup is of the rows and counts the document as missing
    let dir = tempfile::tempdir().unwrap();
    let a = vault(dir.path());
    a.with_conn(|c| {
        c.execute(
            "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date) VALUES ('row only', 'application/pdf', 100, ?1, '2026-03-02')",
            ["ab".repeat(32)],
        )?;
        Ok(())
    })
    .unwrap();
    let (bytes, written) = backup_of(&a, true);
    assert_eq!(written, Written { documents: 0, missing: 1 });
    assert!(open(&bytes, PASS).is_ok());
}
