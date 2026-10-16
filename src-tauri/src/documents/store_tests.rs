use std::fs;

use super::seal::*;
use super::store::{sha256, Store};
use crate::db::DbKey;

fn doc_keys() -> DocKeys {
    DocKeys::from_db_key(&DbKey::from_bytes([3; 32]))
}

fn id(n: u8) -> DocId {
    DocId::from_bytes([n; 16])
}

fn pattern(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    (dir, store)
}

fn names(store: &Store) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(store.dir()) {
        Ok(entries) => entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

#[test]
fn a_file_is_stored_and_comes_back_proved() {
    let (_dir, store) = store();
    let keys = doc_keys();
    let plain = pattern(150_000);
    assert!(!store.has(&keys, &id(1)));
    store.put(&keys, &id(1), &plain).unwrap();
    assert!(store.has(&keys, &id(1)));
    let back = store.get(&keys, &id(1), plain.len() as u64, &sha256(&plain)).unwrap();
    assert_eq!(back.as_slice(), plain.as_slice());
    assert_eq!(names(&store), vec![keys.file_name(&id(1))], "one file, and no temporary one left behind");
}

#[test]
fn the_size_and_the_hash_of_the_row_are_checked() {
    let (_dir, store) = store();
    let keys = doc_keys();
    store.put(&keys, &id(1), b"a receipt").unwrap();
    let right = sha256(b"a receipt");
    assert!(store.get(&keys, &id(1), 9, &right).is_ok());
    assert_eq!(store.get(&keys, &id(1), 10, &right).err(), Some(DocError::Mismatch));
    assert_eq!(store.get(&keys, &id(1), 8, &right).err(), Some(DocError::Mismatch));
    assert_eq!(store.get(&keys, &id(1), 9, &sha256(b"another one")).err(), Some(DocError::Mismatch));
    assert_eq!(store.get(&keys, &id(2), 9, &right).err(), Some(DocError::Missing));
}

#[test]
fn nothing_readable_is_on_disk() {
    let (_dir, store) = store();
    let keys = doc_keys();
    let secret = b"IBAN DE89 3704 0044 0532 0130 00 secret-receipt-text";
    store.put(&keys, &id(1), secret).unwrap();
    let file = fs::read(store.dir().join(keys.file_name(&id(1)))).unwrap();
    assert!(!file.windows(14).any(|w| w == &secret[..14]));
    assert!(!names(&store)[0].contains(&id(1).to_hex()));
}

#[cfg(unix)]
#[test]
fn the_folder_and_the_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, store) = store();
    let keys = doc_keys();
    store.put(&keys, &id(1), b"x").unwrap();
    assert_eq!(fs::metadata(store.dir()).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(fs::metadata(store.dir().join(keys.file_name(&id(1)))).unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn a_file_over_the_limit_is_refused_and_nothing_is_written() {
    let (_dir, store) = store();
    let big = vec![0u8; MAX_DOCUMENT_BYTES as usize + 1];
    assert_eq!(store.put(&doc_keys(), &id(1), &big).err(), Some(DocError::TooLarge));
    assert!(names(&store).is_empty());
}

#[test]
fn an_empty_file_is_stored_and_comes_back() {
    let (_dir, store) = store();
    store.put(&doc_keys(), &id(1), b"").unwrap();
    assert_eq!(store.get(&doc_keys(), &id(1), 0, &sha256(b"")).unwrap().len(), 0);
}

#[test]
fn putting_again_replaces_the_file_whole() {
    let (_dir, store) = store();
    let keys = doc_keys();
    store.put(&keys, &id(1), b"first").unwrap();
    store.put(&keys, &id(1), b"second, and longer").unwrap();
    assert_eq!(store.get(&keys, &id(1), 18, &sha256(b"second, and longer")).unwrap().as_slice(), b"second, and longer");
    assert_eq!(names(&store).len(), 1);
}

#[test]
fn a_file_that_was_cut_or_changed_on_disk_is_refused_when_read() {
    let (_dir, store) = store();
    let keys = doc_keys();
    let plain = pattern(100_000);
    store.put(&keys, &id(1), &plain).unwrap();
    let path = store.dir().join(keys.file_name(&id(1)));
    let sealed = fs::read(&path).unwrap();

    fs::write(&path, &sealed[..sealed.len() - 100]).unwrap();
    assert!(store.get(&keys, &id(1), plain.len() as u64, &sha256(&plain)).is_err());

    let mut changed = sealed.clone();
    changed[5000] ^= 1;
    fs::write(&path, &changed).unwrap();
    assert_eq!(store.get(&keys, &id(1), plain.len() as u64, &sha256(&plain)).err(), Some(DocError::Damaged));

    // another document's file copied under this name
    store.put(&keys, &id(2), &plain).unwrap();
    fs::copy(store.dir().join(keys.file_name(&id(2))), &path).unwrap();
    assert_eq!(store.get(&keys, &id(1), plain.len() as u64, &sha256(&plain)).err(), Some(DocError::Damaged));
}

#[test]
fn removing_takes_every_trace_of_a_document() {
    let (_dir, store) = store();
    let keys = doc_keys();
    store.put(&keys, &id(1), b"one").unwrap();
    store.put(&keys, &id(2), b"two").unwrap();
    fs::write(store.dir().join(format!("{}.part", keys.stem(&id(1)))), b"x").unwrap();
    fs::write(store.dir().join(format!("{}.tmp", keys.stem(&id(1)))), b"x").unwrap();
    store.remove(&keys, &id(1)).unwrap();
    assert_eq!(names(&store), vec![keys.file_name(&id(2))]);
    store.remove(&keys, &id(1)).unwrap();
    store.remove(&keys, &id(9)).unwrap();
}

#[test]
fn the_list_of_files_present_is_read_once() {
    let (_dir, store) = store();
    let keys = doc_keys();
    assert!(store.present_stems().unwrap().is_empty(), "no folder yet");
    store.put(&keys, &id(1), b"one").unwrap();
    store.put(&keys, &id(2), b"two").unwrap();
    fs::write(store.dir().join(format!("{}.part", keys.stem(&id(3)))), b"x").unwrap();
    fs::write(store.dir().join("notes.txt"), b"x").unwrap();
    let present = store.present_stems().unwrap();
    assert_eq!(present.len(), 2);
    assert!(present.contains(&keys.stem(&id(1))) && present.contains(&keys.stem(&id(2))));
}

#[test]
fn remove_all_takes_the_folder() {
    let (dir, store) = store();
    store.put(&doc_keys(), &id(1), b"one").unwrap();
    store.remove_all().unwrap();
    assert!(!dir.path().join("documents").exists());
    store.remove_all().unwrap();
}

// -- files that arrive from the other device ----------------------------------------------------------------------------

fn receive(store: &Store, keys: &DocKeys, doc: u8, plain: &[u8], piece: usize) -> Result<(), DocError> {
    let mut incoming = store.begin_incoming(keys, &id(doc), plain.len() as u64)?;
    assert_eq!(incoming.offset(), 0);
    for part in plain.chunks(piece) {
        incoming.write(part)?;
    }
    incoming.finish(&sha256(plain))
}

#[test]
fn a_file_received_in_pieces_is_the_same_as_one_that_was_put() {
    let (_dir, store) = store();
    let keys = doc_keys();
    for (doc, size, piece) in [(1u8, 0usize, 10usize), (2, 5, 2), (3, 70_000, 24_576), (4, 2 * CHUNK_SIZE, 24_576), (5, 3 * CHUNK_SIZE + 1, 40_000)] {
        let plain = pattern(size);
        receive(&store, &keys, doc, &plain, piece).unwrap();
        let back = store.get(&keys, &id(doc), size as u64, &sha256(&plain)).unwrap();
        assert_eq!(back.as_slice(), plain.as_slice(), "{size} bytes in pieces of {piece}");
    }
    assert!(names(&store).iter().all(|n| n.ends_with(".mnd")), "no unfinished file is left: {:?}", names(&store));
}

#[test]
fn a_file_with_the_wrong_hash_is_removed_and_never_gets_its_name() {
    let (_dir, store) = store();
    let keys = doc_keys();
    let plain = pattern(100_000);
    let mut incoming = store.begin_incoming(&keys, &id(1), plain.len() as u64).unwrap();
    incoming.write(&plain).unwrap();
    assert_eq!(incoming.finish(&sha256(b"something else")).err(), Some(DocError::Mismatch));
    assert!(names(&store).is_empty());
    assert!(!store.has(&keys, &id(1)));
}

#[test]
fn more_bytes_than_announced_are_refused_and_fewer_leave_the_file_unfinished() {
    let (_dir, store) = store();
    let keys = doc_keys();
    let mut incoming = store.begin_incoming(&keys, &id(1), 10).unwrap();
    incoming.write(b"12345").unwrap();
    assert_eq!(incoming.write(b"678901").err(), Some(DocError::TooLarge));

    let mut short = store.begin_incoming(&keys, &id(2), 10).unwrap();
    short.write(b"12345").unwrap();
    assert_eq!(short.finish(&sha256(b"1234567890")).err(), Some(DocError::Incomplete));
    assert!(!store.has(&keys, &id(2)));

    assert_eq!(store.begin_incoming(&keys, &id(3), MAX_DOCUMENT_BYTES + 1).err(), Some(DocError::TooLarge));
}

#[test]
fn a_transfer_that_stopped_carries_on_from_the_last_whole_chunk() {
    let (_dir, store) = store();
    let keys = doc_keys();
    let plain = pattern(3 * CHUNK_SIZE + 1000);
    let hash = sha256(&plain);

    // two and a half chunks arrive, then the connection drops: the half chunk was only in memory
    {
        let mut incoming = store.begin_incoming(&keys, &id(1), plain.len() as u64).unwrap();
        incoming.write(&plain[..2 * CHUNK_SIZE + CHUNK_SIZE / 2]).unwrap();
        assert_eq!(incoming.received(), (2 * CHUNK_SIZE + CHUNK_SIZE / 2) as u64);
    }
    let mut incoming = store.begin_incoming(&keys, &id(1), plain.len() as u64).unwrap();
    assert_eq!(incoming.offset(), 2 * CHUNK_SIZE as u64);
    assert_eq!(incoming.received(), 2 * CHUNK_SIZE as u64);
    incoming.write(&plain[2 * CHUNK_SIZE..]).unwrap();
    incoming.finish(&hash).unwrap();
    assert_eq!(store.get(&keys, &id(1), plain.len() as u64, &hash).unwrap().as_slice(), plain.as_slice());
}

#[test]
fn a_transfer_stopped_again_and_again_still_ends_right() {
    let (_dir, store) = store();
    let keys = doc_keys();
    let plain = pattern(5 * CHUNK_SIZE + 7);
    let hash = sha256(&plain);
    let mut offset = 0;
    // each try gets a little further than the last whole chunk, and then stops
    for _ in 0..5 {
        let mut incoming = store.begin_incoming(&keys, &id(1), plain.len() as u64).unwrap();
        assert_eq!(incoming.offset(), offset);
        let to = (incoming.offset() as usize + CHUNK_SIZE + 100).min(plain.len() - 1);
        incoming.write(&plain[incoming.offset() as usize..to]).unwrap();
        offset += CHUNK_SIZE as u64;
    }
    let mut incoming = store.begin_incoming(&keys, &id(1), plain.len() as u64).unwrap();
    let from = incoming.offset() as usize;
    incoming.write(&plain[from..]).unwrap();
    incoming.finish(&hash).unwrap();
    assert_eq!(store.get(&keys, &id(1), plain.len() as u64, &hash).unwrap().as_slice(), plain.as_slice());
}

#[test]
fn a_half_written_chunk_or_garbage_after_the_whole_ones_is_cut_off() {
    let (_dir, store) = store();
    let keys = doc_keys();
    let plain = pattern(3 * CHUNK_SIZE);
    {
        let mut incoming = store.begin_incoming(&keys, &id(1), plain.len() as u64).unwrap();
        incoming.write(&plain[..CHUNK_SIZE + 10]).unwrap();
    }
    let part = store.dir().join(format!("{}.part", keys.stem(&id(1))));
    let mut bytes = fs::read(&part).unwrap();
    bytes.extend_from_slice(&[0xAB; 30_000]);
    fs::write(&part, bytes).unwrap();
    let incoming = store.begin_incoming(&keys, &id(1), plain.len() as u64).unwrap();
    assert_eq!(incoming.offset(), CHUNK_SIZE as u64);
    drop(incoming);
    assert_eq!(fs::metadata(&part).unwrap().len(), (HEADER_LEN + SEALED_CHUNK) as u64);
}

#[test]
fn a_damaged_start_is_not_trusted() {
    let (_dir, store) = store();
    let keys = doc_keys();
    let plain = pattern(4 * CHUNK_SIZE);
    {
        let mut incoming = store.begin_incoming(&keys, &id(1), plain.len() as u64).unwrap();
        incoming.write(&plain[..3 * CHUNK_SIZE]).unwrap();
    }
    let part = store.dir().join(format!("{}.part", keys.stem(&id(1))));
    let mut bytes = fs::read(&part).unwrap();
    bytes[HEADER_LEN + SEALED_CHUNK + 50] ^= 1;
    fs::write(&part, bytes).unwrap();
    let incoming = store.begin_incoming(&keys, &id(1), plain.len() as u64).unwrap();
    assert_eq!(incoming.offset(), CHUNK_SIZE as u64, "only the chunk before the damage is kept");
}

#[test]
fn an_unfinished_file_that_does_not_fit_the_row_starts_again() {
    let (_dir, store) = store();
    let keys = doc_keys();
    let plain = pattern(3 * CHUNK_SIZE);
    {
        let mut incoming = store.begin_incoming(&keys, &id(1), plain.len() as u64).unwrap();
        incoming.write(&plain[..2 * CHUNK_SIZE]).unwrap();
    }
    // the row now says the file is smaller than what was kept: nothing of the old start is used
    let incoming = store.begin_incoming(&keys, &id(1), CHUNK_SIZE as u64).unwrap();
    assert_eq!(incoming.offset(), 0);
    drop(incoming);
    // a start that is not ours at all
    let part = store.dir().join(format!("{}.part", keys.stem(&id(2))));
    fs::write(&part, b"not a sealed file").unwrap();
    assert_eq!(store.begin_incoming(&keys, &id(2), 100).unwrap().offset(), 0);
}

#[test]
fn a_start_made_with_another_key_is_not_used() {
    let (_dir, store) = store();
    let plain = pattern(2 * CHUNK_SIZE + 5);
    {
        let mut incoming = store.begin_incoming(&doc_keys(), &id(1), plain.len() as u64).unwrap();
        incoming.write(&plain[..CHUNK_SIZE + 5]).unwrap();
    }
    let other = DocKeys::from_db_key(&DbKey::from_bytes([4; 32]));
    // the name is different too, so there is nothing to resume
    assert_eq!(store.begin_incoming(&other, &id(1), plain.len() as u64).unwrap().offset(), 0);
}

// -- rows and files that disagree ---------------------------------------------------------------------------------------

#[test]
fn a_quiet_folder_gives_a_quiet_report() {
    let (_dir, store) = store();
    let keys = doc_keys();
    store.put(&keys, &id(1), b"one").unwrap();
    store.put(&keys, &id(2), &pattern(70_000)).unwrap();
    let report = store.check(&keys, &[(id(1), 3), (id(2), 70_000)]).unwrap();
    assert_eq!(report, Default::default());
    assert_eq!(names(&store).len(), 2);
}

#[test]
fn a_file_with_no_row_is_removed_and_a_row_with_no_file_is_reported() {
    let (_dir, store) = store();
    let keys = doc_keys();
    store.put(&keys, &id(1), b"one").unwrap();
    store.put(&keys, &id(2), b"two").unwrap();
    let report = store.check(&keys, &[(id(1), 3), (id(3), 5)]).unwrap();
    assert_eq!((report.strays_removed, report.missing.clone()), (1, vec![id(3)]));
    assert_eq!(names(&store), vec![keys.file_name(&id(1))]);
}

#[test]
fn what_a_crash_leaves_is_cleaned_up_but_a_transfer_in_progress_is_kept() {
    let (_dir, store) = store();
    let keys = doc_keys();
    store.put(&keys, &id(1), b"one").unwrap();
    fs::write(store.dir().join(format!("{}.tmp", keys.stem(&id(1)))), b"half").unwrap();
    fs::write(store.dir().join(format!("{}.tmp", keys.stem(&id(9)))), b"half").unwrap();
    {
        let mut incoming = store.begin_incoming(&keys, &id(2), 3 * CHUNK_SIZE as u64).unwrap();
        incoming.write(&pattern(CHUNK_SIZE)).unwrap();
    }
    {
        let mut incoming = store.begin_incoming(&keys, &id(8), 3 * CHUNK_SIZE as u64).unwrap();
        incoming.write(&pattern(CHUNK_SIZE)).unwrap();
    }
    let report = store.check(&keys, &[(id(1), 3), (id(2), 3 * CHUNK_SIZE as u64)]).unwrap();
    assert_eq!((report.temp_removed, report.parts_kept, report.parts_removed, report.strays_removed), (2, 1, 1, 0));
    assert_eq!(report.missing, vec![id(2)], "the document being received has no finished file yet");
    let left = names(&store);
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(left.contains(&keys.file_name(&id(1))) && left.contains(&format!("{}.part", keys.stem(&id(2)))));
}

#[test]
fn a_file_of_the_wrong_length_is_reported_and_not_touched() {
    let (_dir, store) = store();
    let keys = doc_keys();
    store.put(&keys, &id(1), &pattern(100)).unwrap();
    let path = store.dir().join(keys.file_name(&id(1)));
    let sealed = fs::read(&path).unwrap();
    fs::write(&path, &sealed[..sealed.len() - 1]).unwrap();
    let report = store.check(&keys, &[(id(1), 100)]).unwrap();
    assert_eq!(report.damaged, vec![id(1)]);
    assert!(report.missing.is_empty());
    assert!(path.exists());
}

#[test]
fn things_that_are_not_ours_are_left_alone() {
    let (_dir, store) = store();
    let keys = doc_keys();
    store.put(&keys, &id(1), b"one").unwrap();
    fs::write(store.dir().join("holiday.jpg"), b"x").unwrap();
    fs::write(store.dir().join("0123456789abcdef0123456789abcdef.txt"), b"x").unwrap();
    fs::write(store.dir().join("0123456789ABCDEF0123456789ABCDEF.mnd"), b"x").unwrap();
    fs::create_dir(store.dir().join("0123456789abcdef0123456789abcdee.mnd")).unwrap();
    let report = store.check(&keys, &[(id(1), 3)]).unwrap();
    assert_eq!((report.unknown, report.strays_removed), (4, 0));
    assert_eq!(names(&store).len(), 5);
}

#[test]
fn with_no_folder_every_row_is_missing() {
    let (_dir, store) = store();
    let report = store.check(&doc_keys(), &[(id(1), 3), (id(2), 4)]).unwrap();
    assert_eq!(report.missing, vec![id(1), id(2)]);
    assert!(!store.dir().exists(), "checking does not make the folder");
}
