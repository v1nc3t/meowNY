//! The files of the documents in a sync: what each step works out from a database and a folder, the messages that carry them, and
//! the whole conversation over real connections, with a real device on one end or one made by hand to misbehave.

use std::collections::HashSet;
use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;

use rusqlite::Connection;

use super::crypto::{Ikm, Kind, SESSION_ID_LEN};
use super::files::{self, FilesReport, Reply};
use super::flow::test_support::LocalDb;
use super::flow::{Db, FlowError, Role};
use super::flow_tests::{db, desktop_with_data, limits, met, new_secret, pair, pause, same, sync_run, Person, LOOPBACK};
use super::net::{self, Channel, Window};
use super::qr::{base64url_decode, base64url_encode, Payload};
use super::wire::{ChunkMessage, FileMessage, HaveMessage, Message, Mode, NeedMessage, StoredMessage, WireError, CHUNK_BYTES, MAX_CHUNK_TEXT};
use crate::documents::library::{self, Files, NewDocument};
use crate::documents::seal::{DocId, CHUNK_SIZE};
use crate::documents::store::sha256;
use crate::documents::MAX_DOCUMENT_BYTES;
use crate::keys;

// --- helpers --------------------------------------------------------------------------------------------------------------

fn inside<R>(db: &LocalDb, f: impl FnOnce(&Connection, &Files) -> R) -> R {
    db.with_documents(|conn, files| Ok(f(conn, files))).unwrap()
}

fn pattern(n: usize, seed: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 31 + seed * 7 + i / 251) % 251) as u8).collect()
}

/// Adds a file the way a person does, and returns the `sync_id` of its document.
fn add_file(db: &LocalDb, name: &str, bytes: &[u8], keep: bool) -> String {
    inside(db, |conn, files| {
        let input = NewDocument { name: name.into(), keep_on_phone: keep, ..NewDocument::default() };
        let added = library::add(conn, files, &input, bytes, "2026-03-02").unwrap();
        conn.query_row("SELECT sync_id FROM documents WHERE id = ?1", [added.document.id], |r| r.get(0)).unwrap()
    })
}

/// A row with no file, with a given time of creation (the order the files are asked for in).
fn add_row(db: &LocalDb, name: &str, size: u64, keep: bool, created: &str) -> String {
    let conn = db.conn.lock().unwrap();
    let hash = keys::hex_encode(&sha256(name.as_bytes()));
    conn.execute(
        "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date, keep_on_phone, created_at) VALUES (?1, 'application/octet-stream', ?2, ?3, '2026-03-02', ?4, ?5)",
        rusqlite::params![name, size as i64, hash, keep as i64, created],
    )
    .unwrap();
    conn.query_row("SELECT sync_id FROM documents WHERE name = ?1", [name], |r| r.get(0)).unwrap()
}

/// The row of a document on another device, as the sync would have brought it.
fn copy_row(from: &LocalDb, to: &LocalDb, sync_id: &str, keep: bool, created: &str) {
    let (name, media, size, hash, date): (String, String, i64, String, String) = from
        .conn
        .lock()
        .unwrap()
        .query_row("SELECT name, media_type, size_bytes, sha256, document_date FROM documents WHERE sync_id = ?1", [sync_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap();
    to.conn
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO documents (sync_id, name, media_type, size_bytes, sha256, document_date, keep_on_phone, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![sync_id, name, media, size, hash, date, keep as i64, created],
        )
        .unwrap();
}

fn doc_id(sync_id: &str) -> DocId {
    DocId::parse(sync_id).unwrap()
}

fn has_file(db: &LocalDb, sync_id: &str) -> bool {
    inside(db, |_, files| files.store.has(files.keys, &doc_id(sync_id)))
}

/// The contents of a file, opened and checked against its row.
fn read_file(db: &LocalDb, sync_id: &str) -> Vec<u8> {
    inside(db, |conn, files| {
        let (size, hash): (i64, String) =
            conn.query_row("SELECT size_bytes, sha256 FROM documents WHERE sync_id = ?1", [sync_id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        files.store.get(files.keys, &doc_id(sync_id), size as u64, &keys::hex_decode::<32>(&hash).unwrap()).unwrap().to_vec()
    })
}

fn files_on_disk(db: &LocalDb) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(db.store().dir()) {
        Ok(entries) => entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

fn sync(desktop: &Arc<LocalDb>, phone: &Arc<LocalDb>, byte: u8, mode: Mode) -> (FilesReport, FilesReport) {
    let run = sync_run(desktop, phone, byte, byte, mode, Person::default(), Person::default());
    let (on_desktop, _) = run.desktop.unwrap();
    let on_phone = run.phone.unwrap();
    assert!(!on_desktop.differences && !on_phone.differences);
    (on_desktop.files, on_phone.files)
}

// --- what each step works out ---------------------------------------------------------------------------------------------

#[test]
fn the_list_of_files_held_names_only_documents_with_a_whole_file() {
    let device = db("EUR");
    let with_file = add_file(&device, "a.bin", &pattern(1000, 1), false);
    let without = add_row(&device, "row only", 500, false, "2026-03-02T10:00:00.000Z");
    let unfinished = add_row(&device, "unfinished", 100_000, false, "2026-03-02T10:00:01.000Z");
    inside(&device, |_, files| {
        let mut incoming = files.store.begin_incoming(files.keys, &doc_id(&unfinished), 100_000).unwrap();
        incoming.write(&pattern(70_000, 2)).unwrap();
        // and a file that no row names
        files.store.put(files.keys, &DocId::from_bytes([9; 16]), b"nobody's").unwrap();
    });
    let held = inside(&device, |conn, files| files::have_list(conn, files).unwrap());
    assert_eq!(held, vec![with_file]);
    assert!(!held.contains(&without));
}

#[test]
fn the_desktop_wants_every_file_it_lacks_that_the_phone_holds_and_the_phone_only_those_the_person_keeps() {
    let device = db("EUR");
    let held_here = add_file(&device, "here.bin", &pattern(1000, 3), true);
    let kept = add_row(&device, "kept", 1000, true, "2026-03-02T10:00:00.000Z");
    let not_kept = add_row(&device, "not kept", 1000, false, "2026-03-02T10:00:01.000Z");
    let theirs_lacks = add_row(&device, "other has none", 1000, true, "2026-03-02T10:00:02.000Z");
    let peer_has: HashSet<String> = [held_here.clone(), kept.clone(), not_kept.clone()].into_iter().collect();

    let ask = |role| inside(&device, |conn, files| files::wanted(conn, files, role, &peer_has, u64::MAX).unwrap());
    let desktop = ask(Role::Desktop);
    let ids: Vec<&str> = desktop.files.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, [not_kept.as_str(), kept.as_str()], "newest first; not the one it has, not the one the other lacks");
    assert!(desktop.files.iter().all(|(_, offset)| *offset == 0) && desktop.waiting == 0);

    let phone = ask(Role::Phone);
    assert_eq!(phone.files, vec![(kept.clone(), 0)], "only the document the person keeps on the phone");
    assert!(!ids.contains(&theirs_lacks.as_str()));
}

#[test]
fn what_does_not_fit_in_the_budget_waits_and_a_smaller_file_after_it_still_goes() {
    let device = db("EUR");
    let newest = add_row(&device, "newest", 40_000, false, "2026-03-02T10:00:03.000Z");
    let big = add_row(&device, "big", 30_000, false, "2026-03-02T10:00:02.000Z");
    let small = add_row(&device, "small", 20_000, false, "2026-03-02T10:00:01.000Z");
    let peer_has: HashSet<String> = [newest.clone(), big.clone(), small.clone()].into_iter().collect();
    let wanted = inside(&device, |conn, files| files::wanted(conn, files, Role::Desktop, &peer_has, 60_000).unwrap());
    assert_eq!(wanted.files, vec![(newest, 0), (small, 0)]);
    assert_eq!(wanted.waiting, 1);
}

#[test]
fn a_file_partly_here_is_asked_for_from_where_it_stopped_and_only_the_rest_counts() {
    let device = db("EUR");
    let size = 3 * CHUNK_SIZE as u64 + 100;
    let id = add_row(&device, "partly", size, false, "2026-03-02T10:00:00.000Z");
    inside(&device, |_, files| {
        let mut incoming = files.store.begin_incoming(files.keys, &doc_id(&id), size).unwrap();
        incoming.write(&pattern(2 * CHUNK_SIZE + 5, 4)).unwrap();
    });
    let peer_has: HashSet<String> = [id.clone()].into_iter().collect();
    let rest = size - 2 * CHUNK_SIZE as u64;
    let fits = inside(&device, |conn, files| files::wanted(conn, files, Role::Desktop, &peer_has, rest).unwrap());
    assert_eq!(fits.files, vec![(id.clone(), 2 * CHUNK_SIZE as u64)]);
    let too_little = inside(&device, |conn, files| files::wanted(conn, files, Role::Desktop, &peer_has, rest - 1).unwrap());
    assert_eq!((too_little.files.len(), too_little.waiting), (0, 1));
    let cost = inside(&device, |conn, _| files::cost(conn, &[(id, 2 * CHUNK_SIZE as u64), ("ab".repeat(16), 0)]).unwrap());
    assert_eq!(cost, rest, "a document this device does not know costs nothing");
}

#[test]
fn a_desktop_answers_only_for_documents_the_person_keeps_on_the_phone() {
    let device = db("EUR");
    let bytes = pattern(1000, 5);
    let kept = add_file(&device, "kept.bin", &bytes, true);
    let not_kept = add_file(&device, "not kept.bin", &pattern(1000, 6), false);
    let answer = |role, id: &str, offset| inside(&device, |conn, files| files::answer(conn, files, role, id, offset).unwrap());

    match answer(Role::Desktop, &kept, 0) {
        Reply::Send { size, bytes: sent } => assert_eq!((size, sent.to_vec()), (1000, bytes.clone())),
        Reply::Skip => panic!("a file the person keeps on the phone is sent"),
    }
    assert!(matches!(answer(Role::Desktop, &not_kept, 0), Reply::Skip), "the desktop refuses a file that is not kept on the phone, however it is asked");
    assert!(matches!(answer(Role::Phone, &not_kept, 0), Reply::Send { .. }), "a phone sends what it holds");
    assert!(matches!(answer(Role::Desktop, &"ab".repeat(16), 0), Reply::Skip), "a document it does not know");
    assert!(matches!(answer(Role::Desktop, &kept, CHUNK_SIZE as u64), Reply::Skip), "a start beyond the end");

    // a file that no longer opens is not sent
    let name = device.keys.file_name(&doc_id(&kept));
    fs::write(device.store().dir().join(name), b"damaged").unwrap();
    assert!(matches!(answer(Role::Desktop, &kept, 0), Reply::Skip));
}

#[test]
fn the_phone_lets_go_only_of_what_the_person_does_not_keep_and_the_desktop_holds() {
    let phone = db("EUR");
    let not_kept = add_file(&phone, "not kept.bin", &pattern(1000, 7), false);
    let kept = add_file(&phone, "kept.bin", &pattern(1000, 8), true);
    let unnamed = "ab".repeat(16);
    let removed = inside(&phone, |conn, files| files::drop_stored(conn, files, &[kept.clone(), unnamed, not_kept.clone()]).unwrap());
    assert_eq!(removed, 1);
    assert!(!has_file(&phone, &not_kept) && has_file(&phone, &kept));
    // a second time there is nothing left to remove
    assert_eq!(inside(&phone, |conn, files| files::drop_stored(conn, files, &[not_kept]).unwrap()), 0);
}

#[test]
fn the_desktop_confirms_a_file_only_if_it_holds_it_whole() {
    let desktop = db("EUR");
    let good = add_file(&desktop, "good.bin", &pattern(1000, 9), false);
    let damaged = add_file(&desktop, "damaged.bin", &pattern(1000, 10), false);
    let kept = add_file(&desktop, "kept.bin", &pattern(1000, 11), true);
    let missing = add_row(&desktop, "missing", 1000, false, "2026-03-02T10:00:00.000Z");
    let name = desktop.keys.file_name(&doc_id(&damaged));
    let mut sealed = fs::read(desktop.store().dir().join(&name)).unwrap();
    let last = sealed.len() - 1;
    sealed[last] ^= 1;
    fs::write(desktop.store().dir().join(&name), sealed).unwrap();

    let holds = |id: &str, verify| inside(&desktop, |conn, files| files::holds_whole(conn, files, id, verify).unwrap());
    assert!(holds(&good, true));
    assert!(!holds(&damaged, true), "a copy that does not open is not a copy");
    assert!(holds(&damaged, false), "one that arrived a moment ago is not read again");
    assert!(!holds(&kept, true), "the phone keeps this one, so there is nothing to let go of");
    assert!(!holds(&missing, true) && !holds(&"ab".repeat(16), true));
}

// --- the messages ---------------------------------------------------------------------------------------------------------

fn invalid(message: Message) -> bool {
    matches!(Message::from_bytes(&serde_json::to_vec(&message).unwrap()), Err(WireError::Invalid(_)))
}

fn id(n: u8) -> String {
    format!("{n:02x}").repeat(16)
}

#[test]
fn the_messages_about_files_round_trip_and_the_biggest_fits_a_frame() {
    let piece = pattern(CHUNK_BYTES, 12);
    let messages = [
        Message::Have(HaveMessage { ids: vec![id(1), id(2)], last: true }),
        Message::Need(NeedMessage { files: vec![(id(1), 0), (id(2), 2 * CHUNK_SIZE as u64)], last: false }),
        Message::File(FileMessage { id: id(1), offset: CHUNK_SIZE as u64, size: 100_000, skipped: false }),
        Message::File(FileMessage { id: id(1), offset: 0, size: 0, skipped: true }),
        Message::Chunk(ChunkMessage { data: base64url_encode(&piece), last: true }),
        Message::Stored(StoredMessage { ids: Vec::new(), last: true }),
    ];
    for message in messages {
        let bytes = message.to_bytes().unwrap();
        assert!(bytes.len() <= super::crypto::MAX_PLAINTEXT);
        let back = Message::from_bytes(&bytes).unwrap();
        assert_eq!(back.to_bytes().unwrap(), bytes);
    }
    let text = base64url_encode(&piece);
    assert_eq!(text.len(), MAX_CHUNK_TEXT);
    assert_eq!(base64url_decode(&text).unwrap(), piece);
    // a file message may leave `skipped` out
    let plain = format!(r#"{{"t":"file","id":"{}","offset":0,"size":10}}"#, id(3));
    assert!(matches!(Message::from_bytes(plain.as_bytes()), Ok(Message::File(FileMessage { skipped: false, .. }))));
}

#[test]
fn messages_about_files_that_break_a_rule_are_refused() {
    let many: Vec<String> = (0..401).map(|i| format!("{i:032x}")).collect();
    assert!(invalid(Message::Have(HaveMessage { ids: many.clone(), last: true })));
    assert!(invalid(Message::Stored(StoredMessage { ids: many, last: true })));
    assert!(invalid(Message::Have(HaveMessage { ids: vec!["AB".repeat(16)], last: true })));
    assert!(invalid(Message::Have(HaveMessage { ids: vec!["ab".repeat(15)], last: true })));
    // a start that is not a whole sealed chunk, or past the biggest file
    assert!(invalid(Message::Need(NeedMessage { files: vec![(id(1), 1000)], last: true })));
    assert!(invalid(Message::Need(NeedMessage { files: vec![(id(1), MAX_DOCUMENT_BYTES)], last: true })));
    assert!(invalid(Message::Need(NeedMessage { files: vec![("nonsense".into(), 0)], last: true })));
    // a file: no size, too big, starting at its end, or "skipped" with a size
    let file = |offset, size, skipped| Message::File(FileMessage { id: id(1), offset, size, skipped });
    assert!(invalid(file(0, 0, false)));
    assert!(invalid(file(0, MAX_DOCUMENT_BYTES + 1, false)));
    assert!(invalid(file(CHUNK_SIZE as u64, CHUNK_SIZE as u64, false)));
    assert!(invalid(file(1, CHUNK_SIZE as u64 * 2, false)));
    assert!(invalid(file(0, 10, true)));
    assert!(invalid(file(CHUNK_SIZE as u64, 0, true)));
    // a piece: empty, too long, or not base64url
    assert!(invalid(Message::Chunk(ChunkMessage { data: String::new(), last: true })));
    assert!(invalid(Message::Chunk(ChunkMessage { data: "A".repeat(MAX_CHUNK_TEXT + 1), last: true })));
    assert!(invalid(Message::Chunk(ChunkMessage { data: "ab+/".into(), last: true })));
    assert!(invalid(Message::Chunk(ChunkMessage { data: "ab==".into(), last: true })));
    // a field nobody defined
    let extra = r#"{"t":"have","ids":[],"last":true,"more":1}"#;
    assert!(Message::from_bytes(extra.as_bytes()).is_err());
}

// --- a whole sync, over real connections ----------------------------------------------------------------------------------

#[test]
fn a_file_the_person_keeps_on_the_phone_goes_there_whole_and_sealed_under_the_phones_own_key() {
    let (desktop, phone) = met();
    let bytes = pattern(200_000, 13);
    let id = add_file(&desktop, "scan.bin", &bytes, true);

    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x41, Mode::Both);
    assert_eq!(on_desktop, FilesReport { sent: 1, ..Default::default() });
    assert_eq!(on_phone, FilesReport { received: 1, ..Default::default() });
    assert_eq!(read_file(&phone, &id), bytes, "byte for byte");
    let (on_desktop_disk, on_phone_disk) = (desktop.keys.file_name(&doc_id(&id)), phone.keys.file_name(&doc_id(&id)));
    assert_ne!(on_desktop_disk, on_phone_disk, "another key, another name");
    assert_ne!(fs::read(desktop.store().dir().join(&on_desktop_disk)).unwrap(), fs::read(phone.store().dir().join(&on_phone_disk)).unwrap(), "sealed again, not copied");
    assert_eq!(files_on_disk(&phone), vec![on_phone_disk], "nothing else was left behind");
    same(&desktop, &phone);

    // and the next sync has nothing to move
    pause();
    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x42, Mode::Both);
    assert!(on_desktop.is_empty() && on_phone.is_empty(), "{on_desktop:?} {on_phone:?}");
}

#[test]
fn a_file_the_person_does_not_keep_on_the_phone_never_goes_there() {
    let (desktop, phone) = met();
    let id = add_file(&desktop, "private.bin", &pattern(5000, 14), false);
    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x43, Mode::Both);
    assert!(on_desktop.is_empty() && on_phone.is_empty());
    assert!(files_on_disk(&phone).is_empty(), "the phone has the row and no file");
    assert_eq!(count_rows(&phone), 1);
    assert!(has_file(&desktop, &id));

    // keeping it there from now on brings it at the next sync
    desktop.conn.lock().unwrap().execute("UPDATE documents SET keep_on_phone = 1", []).unwrap();
    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x44, Mode::Both);
    assert_eq!((on_desktop.sent, on_phone.received), (1, 1));
    assert_eq!(read_file(&phone, &id), read_file(&desktop, &id));
}

fn count_rows(db: &LocalDb) -> i64 {
    db.conn.lock().unwrap().query_row("SELECT count(*) FROM documents", [], |r| r.get(0)).unwrap()
}

#[test]
fn a_file_added_on_the_phone_goes_to_the_desktop_and_leaves_the_phone_when_it_is_not_kept_there() {
    let (desktop, phone) = met();
    let bytes = pattern(150_000, 15);
    let id = add_file(&phone, "receipt.bin", &bytes, false);
    let kept = add_file(&phone, "contract.bin", &pattern(90_000, 16), true);

    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x45, Mode::Both);
    assert_eq!(on_desktop, FilesReport { received: 2, ..Default::default() });
    assert_eq!(on_phone, FilesReport { sent: 2, removed: 1, ..Default::default() });
    assert_eq!(read_file(&desktop, &id), bytes);
    assert_eq!(read_file(&desktop, &kept), read_file(&phone, &kept));
    assert!(!has_file(&phone, &id), "the phone let go of the one it does not keep, after the desktop had it");
    assert!(has_file(&phone, &kept), "and kept the other");
    same(&desktop, &phone);

    pause();
    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x46, Mode::Both);
    assert!(on_desktop.is_empty() && on_phone.is_empty());
}

#[test]
fn the_phone_keeps_its_file_when_the_desktop_could_not_take_it() {
    let (desktop, phone) = met();
    let bytes = pattern(100_000, 17);
    let id = add_file(&phone, "receipt.bin", &bytes, false);
    // a directory where the unfinished file would go: the desktop cannot write there
    let blocker = desktop.store().dir().join(format!("{}.part", desktop.keys.stem(&doc_id(&id))));
    fs::create_dir_all(&blocker).unwrap();

    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x47, Mode::Both);
    assert_eq!(on_desktop, FilesReport { failed: 1, ..Default::default() });
    assert_eq!(on_phone, FilesReport { sent: 1, ..Default::default() });
    assert!(!has_file(&desktop, &id));
    assert!(has_file(&phone, &id), "the only copy stays where it is");

    fs::remove_dir(&blocker).unwrap();
    pause();
    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x48, Mode::Both);
    assert_eq!((on_desktop.received, on_phone.sent, on_phone.removed), (1, 1, 1));
    assert_eq!(read_file(&desktop, &id), bytes);
    assert!(!has_file(&phone, &id));
}

#[test]
fn a_file_missing_from_the_phone_comes_back_in_a_sync_where_no_rows_differ() {
    let (desktop, phone) = met();
    let id = add_file(&desktop, "scan.bin", &pattern(70_000, 18), true);
    sync(&desktop, &phone, 0x49, Mode::Both);
    assert!(has_file(&phone, &id));

    inside(&phone, |_, files| files.store.remove(files.keys, &doc_id(&id)).unwrap());
    pause();
    let run = sync_run(&desktop, &phone, 0x4a, 0x4a, Mode::Both, Person::default(), Person::default());
    let (on_desktop, on_phone) = (run.desktop.unwrap().0, run.phone.unwrap());
    assert!(on_phone.compared.is_empty(), "the rows were equal");
    assert_eq!((on_desktop.files.sent, on_phone.files.received), (1, 1));
    assert_eq!(read_file(&phone, &id), read_file(&desktop, &id));
}

#[test]
fn big_files_cross_in_many_pieces_in_both_directions_at_once() {
    let (desktop, phone) = met();
    let from_desktop = pattern(3 * 1024 * 1024 + 123, 19);
    let from_phone = pattern(2 * 1024 * 1024 + 7, 20);
    let a = add_file(&desktop, "big from the desktop.bin", &from_desktop, true);
    let b = add_file(&phone, "big from the phone.bin", &from_phone, true);

    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x4b, Mode::Both);
    assert_eq!(on_desktop, FilesReport { sent: 1, received: 1, ..Default::default() });
    assert_eq!(on_phone, FilesReport { sent: 1, received: 1, ..Default::default() });
    assert_eq!(read_file(&phone, &a), from_desktop);
    assert_eq!(read_file(&desktop, &b), from_phone);
}

#[test]
fn a_sync_to_the_desktop_moves_files_to_it_and_leaves_the_phone_as_it_is() {
    let (desktop, phone) = met();
    let from_phone = pattern(80_000, 21);
    let a = add_file(&phone, "receipt.bin", &from_phone, false);
    let b = add_file(&desktop, "for the phone.bin", &pattern(80_000, 22), true);

    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x4c, Mode::ToDesktop);
    assert_eq!(on_desktop, FilesReport { received: 1, ..Default::default() });
    assert_eq!(on_phone, FilesReport { sent: 1, ..Default::default() });
    assert_eq!(read_file(&desktop, &a), from_phone);
    assert!(has_file(&phone, &a), "a sync that goes one way does not take a file off the phone");
    assert_eq!(count_rows(&phone), 1, "and brings none to it");
    assert!(!has_file(&phone, &b));
}

#[test]
fn a_sync_to_the_phone_moves_files_to_it_and_takes_nothing_from_it() {
    let (desktop, phone) = met();
    let for_phone = pattern(80_000, 23);
    let a = add_file(&desktop, "for the phone.bin", &for_phone, true);
    let b = add_file(&phone, "receipt.bin", &pattern(80_000, 24), false);

    let (on_desktop, on_phone) = sync(&desktop, &phone, 0x4d, Mode::ToPhone);
    assert_eq!(on_desktop, FilesReport { sent: 1, ..Default::default() });
    assert_eq!(on_phone, FilesReport { received: 1, ..Default::default() });
    assert_eq!(read_file(&phone, &a), for_phone);
    assert!(has_file(&phone, &b), "the phone's own file is not asked for, and is not let go of");
    assert!(!has_file(&desktop, &b));
}

#[test]
fn a_new_phone_gets_the_files_the_person_keeps_on_it_when_it_is_paired() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    let kept = add_file(&desktop, "kept.bin", &pattern(60_000, 25), true);
    let private = add_file(&desktop, "private.bin", &pattern(60_000, 26), false);
    let paired = pair(&desktop, &phone, true);
    let (on_desktop, on_phone) = (paired.desktop.unwrap(), paired.phone.unwrap());
    assert_eq!((on_desktop.files.sent, on_phone.files.received), (1, 1));
    assert_eq!(read_file(&phone, &kept), read_file(&desktop, &kept));
    assert!(!has_file(&phone, &private) && has_file(&desktop, &private));
}

// --- a device made by hand, to see what the real one does with what it should not be sent or asked ------------------------

/// The two ends of a connection, without a sync around them: `(desktop's, phone's)`.
fn channels() -> (Channel, Channel) {
    let secret = new_secret(0x61);
    let sid = [0x71u8; SESSION_ID_LEN];
    let window = Window::open(Kind::Pair, LOOPBACK, sid, vec![Ikm::for_pairing(&secret)], limits()).unwrap();
    let payload = Payload { kind: Kind::Pair, session_id: sid, secret: new_secret(0x61), port: window.address().port(), addresses: vec![LOOPBACK] };
    let accepting = thread::spawn(move || window.accept(&Arc::new(AtomicBool::new(false))).unwrap().0);
    let phone = net::connect(&payload, Ikm::for_pairing(&payload.secret), limits(), None).unwrap();
    (accepting.join().unwrap(), phone)
}

fn hear_ids(channel: &mut Channel) -> Vec<String> {
    let mut all = Vec::new();
    loop {
        match channel.receive().unwrap() {
            Message::Have(m) => {
                all.extend(m.ids);
                if m.last {
                    return all;
                }
            }
            Message::Stored(m) => {
                all.extend(m.ids);
                if m.last {
                    return all;
                }
            }
            other => panic!("a list of documents was expected, not {other:?}"),
        }
    }
}

fn hear_need(channel: &mut Channel) -> Vec<(String, u64)> {
    let mut all = Vec::new();
    loop {
        let Message::Need(m) = channel.receive().unwrap() else { panic!("a request for files was expected") };
        all.extend(m.files);
        if m.last {
            return all;
        }
    }
}

fn say_have(channel: &mut Channel, ids: &[&str]) {
    channel.send(&Message::Have(HaveMessage { ids: ids.iter().map(|s| s.to_string()).collect(), last: true })).unwrap();
}

fn say_need(channel: &mut Channel, files: &[(&str, u64)]) {
    channel.send(&Message::Need(NeedMessage { files: files.iter().map(|(id, offset)| (id.to_string(), *offset)).collect(), last: true })).unwrap();
}

fn say_stored(channel: &mut Channel) {
    channel.send(&Message::Stored(StoredMessage { ids: Vec::new(), last: true })).unwrap();
}

fn say_file(channel: &mut Channel, id: &str, size: u64, offset: u64, body: &[u8], pieces_to_send: usize) {
    channel.send(&Message::File(FileMessage { id: id.into(), offset, size, skipped: false })).unwrap();
    let pieces: Vec<&[u8]> = body.chunks(CHUNK_BYTES).collect();
    for (index, piece) in pieces.iter().take(pieces_to_send).enumerate() {
        channel.send(&Message::Chunk(ChunkMessage { data: base64url_encode(piece), last: index + 1 == pieces.len() })).unwrap();
    }
}

/// Reads a file the real device sends: the announcement, then pieces to the last.
fn hear_file(channel: &mut Channel) -> (FileMessage, Vec<u8>) {
    let Message::File(file) = channel.receive().unwrap() else { panic!("a file was expected") };
    let mut body = Vec::new();
    if !file.skipped {
        loop {
            let Message::Chunk(piece) = channel.receive().unwrap() else { panic!("a piece was expected") };
            body.extend(base64url_decode(&piece.data).unwrap());
            if piece.last {
                break;
            }
        }
    }
    (file, body)
}

#[test]
fn a_phone_that_asks_for_what_it_should_not_have_is_told_the_file_is_skipped() {
    let desktop = db("EUR");
    let kept_bytes = pattern(70_000, 27);
    let kept = add_file(&desktop, "kept.bin", &kept_bytes, true);
    let marker = b"THIS-FILE-STAYS-ON-THE-DESKTOP";
    let mut private = pattern(70_000, 28);
    private[30_000..30_000 + marker.len()].copy_from_slice(marker);
    let not_kept = add_file(&desktop, "not kept.bin", &private, false);
    let small = add_file(&desktop, "small.bin", &pattern(1000, 29), true);
    let unknown = "ab".repeat(16);

    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let desktop = desktop.clone();
        thread::spawn(move || files::run(&mut at_desktop, &*desktop, Role::Desktop, Mode::Both))
    };
    assert_eq!(hear_ids(&mut at_phone).len(), 3, "the desktop lists every file it holds, kept on the phone or not");
    say_have(&mut at_phone, &[]);
    assert!(hear_need(&mut at_phone).is_empty(), "it wants nothing: the phone holds nothing");
    say_need(&mut at_phone, &[(&not_kept, 0), (&kept, 0), (&unknown, 0), (&small, CHUNK_SIZE as u64)]);

    let mut heard = Vec::new();
    let (first, none) = hear_file(&mut at_phone);
    assert!(first.skipped && first.id == not_kept && none.is_empty(), "a file that is not kept on the phone");
    heard.extend(none);
    let (second, body) = hear_file(&mut at_phone);
    assert!(!second.skipped && second.id == kept && second.size == 70_000 && second.offset == 0);
    assert_eq!(body, kept_bytes);
    heard.extend(body);
    let (third, none) = hear_file(&mut at_phone);
    assert!(third.skipped && third.id == unknown);
    heard.extend(none);
    let (fourth, none) = hear_file(&mut at_phone);
    assert!(fourth.skipped && fourth.id == small, "a start beyond the end");
    heard.extend(none);
    assert!(!heard.windows(marker.len()).any(|w| w == marker), "nothing of the file that stays crossed");
    assert!(hear_ids(&mut at_phone).is_empty(), "nothing is confirmed that the phone did not hold");
    assert_eq!(real.join().unwrap().unwrap(), FilesReport { sent: 1, ..Default::default() });
}

#[test]
fn a_phone_that_asks_for_files_in_a_sync_that_goes_to_the_desktop_ends_it() {
    let desktop = db("EUR");
    let kept = add_file(&desktop, "kept.bin", &pattern(1000, 30), true);
    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let desktop = desktop.clone();
        thread::spawn(move || files::run(&mut at_desktop, &*desktop, Role::Desktop, Mode::ToDesktop))
    };
    hear_ids(&mut at_phone);
    say_have(&mut at_phone, &[]);
    hear_need(&mut at_phone);
    say_need(&mut at_phone, &[(&kept, 0)]);
    assert!(matches!(real.join().unwrap(), Err(FlowError::Protocol(_))));
}

/// A phone with the rows of some documents and none of the files, and the files themselves on a desktop.
fn phone_waiting_for(files: &[(&str, usize)], seed: usize) -> (Arc<LocalDb>, Vec<(String, Vec<u8>)>) {
    let source = db("EUR");
    let phone = db("EUR");
    let mut out = Vec::new();
    for (index, (name, size)) in files.iter().enumerate() {
        let bytes = pattern(*size, seed + index);
        let id = add_file(&source, name, &bytes, true);
        copy_row(&source, &phone, &id, true, &format!("2026-03-02T10:00:0{index}.000Z"));
        out.push((id, bytes));
    }
    (phone, out)
}

#[test]
fn a_transfer_that_breaks_half_way_carries_on_from_its_last_whole_chunk() {
    let (phone, files) = phone_waiting_for(&[("scan.bin", 3 * CHUNK_SIZE + 4000)], 31);
    let (id, bytes) = (files[0].0.clone(), files[0].1.clone());
    let size = bytes.len() as u64;

    // the first time: three pieces of 24 KiB arrive and the connection goes
    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let phone = phone.clone();
        thread::spawn(move || files::run(&mut at_phone, &*phone, Role::Phone, Mode::Both))
    };
    say_have(&mut at_desktop, &[&id]);
    hear_ids(&mut at_desktop);
    say_need(&mut at_desktop, &[]);
    assert_eq!(hear_need(&mut at_desktop), vec![(id.clone(), 0)]);
    say_file(&mut at_desktop, &id, size, 0, &bytes, 3);
    drop(at_desktop);
    let report = real.join().unwrap().unwrap();
    assert_eq!(report, FilesReport { interrupted: true, ..Default::default() }, "a broken connection is reported, not an error");
    assert!(!has_file(&phone, &id));
    let start = inside(&phone, |_, f| f.store.resume_offset(f.keys, &doc_id(&id), size));
    assert_eq!(start, CHUNK_SIZE as u64, "one whole sealed chunk is kept of the 72 KiB that came");

    // the second time it asks for the rest, and only the rest is sent
    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let phone = phone.clone();
        thread::spawn(move || files::run(&mut at_phone, &*phone, Role::Phone, Mode::Both))
    };
    say_have(&mut at_desktop, &[&id]);
    hear_ids(&mut at_desktop);
    say_need(&mut at_desktop, &[]);
    assert_eq!(hear_need(&mut at_desktop), vec![(id.clone(), CHUNK_SIZE as u64)]);
    let rest = &bytes[CHUNK_SIZE..];
    say_file(&mut at_desktop, &id, size, CHUNK_SIZE as u64, rest, usize::MAX);
    say_stored(&mut at_desktop);
    assert_eq!(real.join().unwrap().unwrap(), FilesReport { received: 1, ..Default::default() });
    assert_eq!(read_file(&phone, &id), bytes);
    assert_eq!(files_on_disk(&phone), vec![phone.keys.file_name(&doc_id(&id))], "the unfinished file is gone");
}

#[test]
fn a_file_that_arrives_with_other_contents_is_refused_and_the_next_one_goes_on() {
    let (phone, files) = phone_waiting_for(&[("first.bin", 100_000), ("second.bin", 50_000)], 40);
    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let phone = phone.clone();
        thread::spawn(move || files::run(&mut at_phone, &*phone, Role::Phone, Mode::Both))
    };
    let all: Vec<&str> = files.iter().map(|(id, _)| id.as_str()).collect();
    say_have(&mut at_desktop, &all);
    hear_ids(&mut at_desktop);
    say_need(&mut at_desktop, &[]);
    let asked = hear_need(&mut at_desktop);
    assert_eq!(asked.len(), 2);
    for (index, (id, offset)) in asked.iter().enumerate() {
        let mut bytes = files.iter().find(|(f, _)| f == id).unwrap().1.clone();
        if index == 0 {
            bytes[500] ^= 0x40;
        }
        say_file(&mut at_desktop, id, bytes.len() as u64, *offset, &bytes, usize::MAX);
    }
    say_stored(&mut at_desktop);
    assert_eq!(real.join().unwrap().unwrap(), FilesReport { received: 1, failed: 1, ..Default::default() });
    assert!(!has_file(&phone, &asked[0].0), "the damaged one was not kept");
    assert_eq!(read_file(&phone, &asked[1].0), files.iter().find(|(f, _)| *f == asked[1].0).unwrap().1);
    assert_eq!(files_on_disk(&phone), vec![phone.keys.file_name(&doc_id(&asked[1].0))], "and left nothing behind");
}

#[test]
fn a_transfer_stopped_after_any_number_of_pieces_resumes_into_a_whole_file() {
    let size = 3 * CHUNK_SIZE + 4000;
    let pieces = size.div_ceil(CHUNK_BYTES);
    for stop in 0..=pieces {
        let (phone, files) = phone_waiting_for(&[("scan.bin", size)], 90 + stop);
        let (id, bytes) = (files[0].0.clone(), files[0].1.clone());
        let (mut at_desktop, mut at_phone) = channels();
        let real = {
            let phone = phone.clone();
            thread::spawn(move || files::run(&mut at_phone, &*phone, Role::Phone, Mode::Both))
        };
        say_have(&mut at_desktop, &[&id]);
        hear_ids(&mut at_desktop);
        say_need(&mut at_desktop, &[]);
        hear_need(&mut at_desktop);
        say_file(&mut at_desktop, &id, size as u64, 0, &bytes, stop);
        drop(at_desktop);
        let report = real.join().unwrap().unwrap();
        assert!(report.interrupted, "stopped after {stop} pieces");
        if stop == pieces {
            // every piece came, so the file is whole; it is only the end of the conversation that was cut
            assert_eq!((report.received, read_file(&phone, &id)), (1, bytes));
            continue;
        }
        assert!(!has_file(&phone, &id), "stopped after {stop} pieces");

        // what was kept is the whole sealed chunks among the bytes that came
        let kept = ((stop * CHUNK_BYTES).min(size) / CHUNK_SIZE * CHUNK_SIZE) as u64;
        let (mut at_desktop, mut at_phone) = channels();
        let real = {
            let phone = phone.clone();
            thread::spawn(move || files::run(&mut at_phone, &*phone, Role::Phone, Mode::Both))
        };
        say_have(&mut at_desktop, &[&id]);
        hear_ids(&mut at_desktop);
        say_need(&mut at_desktop, &[]);
        assert_eq!(hear_need(&mut at_desktop), vec![(id.clone(), kept)], "stopped after {stop} pieces");
        say_file(&mut at_desktop, &id, size as u64, kept, &bytes[kept as usize..], usize::MAX);
        say_stored(&mut at_desktop);
        assert_eq!(real.join().unwrap().unwrap().received, 1, "stopped after {stop} pieces");
        assert_eq!(read_file(&phone, &id), bytes, "stopped after {stop} pieces");
    }
}

#[test]
fn a_piece_where_a_file_was_expected_ends_the_sync() {
    let (phone, files) = phone_waiting_for(&[("only.bin", 40_000)], 70);
    let id = files[0].0.clone();
    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let phone = phone.clone();
        thread::spawn(move || files::run(&mut at_phone, &*phone, Role::Phone, Mode::Both))
    };
    say_have(&mut at_desktop, &[&id]);
    hear_ids(&mut at_desktop);
    say_need(&mut at_desktop, &[]);
    hear_need(&mut at_desktop);
    at_desktop.send(&Message::Chunk(ChunkMessage { data: base64url_encode(&[1, 2, 3]), last: true })).unwrap();
    assert!(matches!(real.join().unwrap(), Err(FlowError::Protocol(_))));
    assert!(!has_file(&phone, &id));
}

#[test]
fn a_file_asked_for_twice_ends_the_sync() {
    let desktop = db("EUR");
    let kept = add_file(&desktop, "kept.bin", &pattern(1000, 72), true);
    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let desktop = desktop.clone();
        thread::spawn(move || files::run(&mut at_desktop, &*desktop, Role::Desktop, Mode::Both))
    };
    hear_ids(&mut at_phone);
    say_have(&mut at_phone, &[]);
    hear_need(&mut at_phone);
    say_need(&mut at_phone, &[(&kept, 0), (&kept, 0)]);
    assert!(matches!(real.join().unwrap(), Err(FlowError::Protocol(_))));
}

#[test]
fn a_file_the_other_side_skips_is_counted_and_the_sync_goes_on() {
    let (phone, files) = phone_waiting_for(&[("only.bin", 10_000)], 50);
    let id = files[0].0.clone();
    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let phone = phone.clone();
        thread::spawn(move || files::run(&mut at_phone, &*phone, Role::Phone, Mode::Both))
    };
    say_have(&mut at_desktop, &[&id]);
    hear_ids(&mut at_desktop);
    say_need(&mut at_desktop, &[]);
    hear_need(&mut at_desktop);
    at_desktop.send(&Message::File(FileMessage { id: id.clone(), offset: 0, size: 0, skipped: true })).unwrap();
    say_stored(&mut at_desktop);
    assert_eq!(real.join().unwrap().unwrap(), FilesReport { failed: 1, ..Default::default() });
}

/// A phone waiting for one file, and a desktop made by hand that announces it as `announce` says (the name and the size it gives)
/// and then sends the real contents followed by `extra` bytes more. The real device must end the sync and keep nothing.
fn refuses(what: &str, announce: impl Fn(&str, u64) -> (String, u64), offset: u64, extra: usize) {
    let (phone, files) = phone_waiting_for(&[("only.bin", 200_000)], 60);
    let (id, bytes) = (files[0].0.clone(), files[0].1.clone());
    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let phone = phone.clone();
        thread::spawn(move || files::run(&mut at_phone, &*phone, Role::Phone, Mode::Both))
    };
    say_have(&mut at_desktop, &[&id]);
    hear_ids(&mut at_desktop);
    say_need(&mut at_desktop, &[]);
    hear_need(&mut at_desktop);
    let (named, size) = announce(&id, bytes.len() as u64);
    // the real device may end the conversation before all of this is sent
    let _ = at_desktop.send(&Message::File(FileMessage { id: named, offset, size, skipped: false }));
    let body = [bytes, vec![1u8; extra]].concat();
    let pieces: Vec<&[u8]> = body.chunks(CHUNK_BYTES).collect();
    for (index, piece) in pieces.iter().enumerate() {
        if at_desktop.send(&Message::Chunk(ChunkMessage { data: base64url_encode(piece), last: index + 1 == pieces.len() })).is_err() {
            break;
        }
    }
    assert!(matches!(real.join().unwrap(), Err(FlowError::Protocol(_))), "{what}");
    assert!(!has_file(&phone, &id), "{what}");
}

#[test]
fn a_file_that_is_not_the_one_asked_for_ends_the_sync() {
    refuses("another size than the row", |id, size| (id.to_string(), size + 1), 0, 0);
    refuses("more bytes than the row", |id, size| (id.to_string(), size), 0, 100);
    refuses("another file than the one asked for", |_, size| ("cd".repeat(16), size), 0, 0);
    refuses("a start nobody asked for", |id, size| (id.to_string(), size), CHUNK_SIZE as u64, 0);
}

#[test]
fn the_phones_share_of_the_budget_is_what_the_desktops_request_leaves() {
    // a budget of 300 KB: the desktop asks the phone for a file of 200 KB, so the phone can ask for 100 KB, and of two files it
    // would like (80 KB, newer, and 60 KB) only the newer fits
    let source = db("EUR");
    let phone = db("EUR");
    let newer = add_file(&source, "newer.bin", &pattern(80_000, 80), true);
    let older = add_file(&source, "older.bin", &pattern(60_000, 81), true);
    copy_row(&source, &phone, &newer, true, "2026-03-02T10:00:09.000Z");
    copy_row(&source, &phone, &older, true, "2026-03-02T10:00:01.000Z");
    let held = add_file(&phone, "held.bin", &pattern(200_000, 82), false);

    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let phone = phone.clone();
        thread::spawn(move || files::run_within(&mut at_phone, &*phone, Role::Phone, Mode::Both, 300_000))
    };
    say_have(&mut at_desktop, &[&newer, &older]);
    assert_eq!(hear_ids(&mut at_desktop), vec![held.clone()]);
    say_need(&mut at_desktop, &[(&held, 0)]);
    assert_eq!(hear_need(&mut at_desktop), vec![(newer.clone(), 0)]);
    say_file(&mut at_desktop, &newer, 80_000, 0, &read_file(&source, &newer), usize::MAX);
    let (file, body) = hear_file(&mut at_desktop);
    assert_eq!((file.id.as_str(), body.len()), (held.as_str(), 200_000));
    say_stored(&mut at_desktop);
    assert_eq!(real.join().unwrap().unwrap(), FilesReport { received: 1, sent: 1, waiting: 1, ..Default::default() });
}

#[test]
fn a_sender_stops_at_the_budget_and_skips_what_would_go_over() {
    let desktop = db("EUR");
    let a = add_file(&desktop, "a.bin", &pattern(60_000, 83), true);
    let b = add_file(&desktop, "b.bin", &pattern(60_000, 84), true);
    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let desktop = desktop.clone();
        thread::spawn(move || files::run_within(&mut at_desktop, &*desktop, Role::Desktop, Mode::Both, 100_000))
    };
    hear_ids(&mut at_phone);
    say_have(&mut at_phone, &[]);
    hear_need(&mut at_phone);
    say_need(&mut at_phone, &[(&a, 0), (&b, 0)]);
    let (first, _) = hear_file(&mut at_phone);
    let (second, _) = hear_file(&mut at_phone);
    assert!(!first.skipped && second.skipped, "a phone that asks for more than the budget is not sent more");
    hear_ids(&mut at_phone);
    assert_eq!(real.join().unwrap().unwrap().sent, 1);
}

#[test]
fn a_connection_that_drops_while_files_are_sent_is_an_interruption_for_the_sender_too() {
    let desktop = db("EUR");
    let kept = add_file(&desktop, "kept.bin", &pattern(200_000, 70), true);
    let (mut at_desktop, mut at_phone) = channels();
    let real = {
        let desktop = desktop.clone();
        thread::spawn(move || files::run(&mut at_desktop, &*desktop, Role::Desktop, Mode::Both))
    };
    hear_ids(&mut at_phone);
    say_have(&mut at_phone, &[]);
    hear_need(&mut at_phone);
    say_need(&mut at_phone, &[(&kept, 0)]);
    let Message::File(_) = at_phone.receive().unwrap() else { panic!() };
    let Message::Chunk(_) = at_phone.receive().unwrap() else { panic!() };
    drop(at_phone);
    let report = real.join().unwrap().unwrap();
    assert!(report.interrupted);
}
