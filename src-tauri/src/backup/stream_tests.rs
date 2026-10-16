//! The sealed stream of a version 2 backup on its own: where the chunks end, what a changed, moved, cut or added chunk does, and what
//! the reader refuses.

use super::stream::{ChunkReader, ChunkWriter, CHUNK, TAG};
use crate::vault::AppError;

const KEY: [u8; 32] = [7; 32];
const SEALED: usize = CHUNK + TAG;

fn pattern(n: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 13 + i / 251) % 256) as u8).collect()
}

fn header() -> Vec<u8> {
    b"a header that every chunk authenticates".to_vec()
}

fn sealed(data: &[u8], pieces: usize) -> Vec<u8> {
    let mut writer = ChunkWriter::new(Vec::new(), &KEY, header());
    for piece in data.chunks(pieces.max(1)) {
        writer.write(piece).unwrap();
    }
    writer.finish().unwrap()
}

fn reader(bytes: Vec<u8>) -> ChunkReader<std::io::Cursor<Vec<u8>>> {
    let len = bytes.len() as u64;
    ChunkReader::new(std::io::Cursor::new(bytes), &KEY, header(), len)
}

/// Reads everything and checks that the stream ends where it should.
fn read_all(bytes: Vec<u8>, len: usize) -> Result<Vec<u8>, AppError> {
    let mut reader = reader(bytes);
    let mut out = vec![0u8; len];
    reader.take(&mut out)?;
    reader.finish()?;
    Ok(out)
}

#[test]
fn a_stream_comes_back_whole_at_every_size_around_the_chunk_edges() {
    for n in [1, 2, CHUNK - 1, CHUNK, CHUNK + 1, 2 * CHUNK - 1, 2 * CHUNK, 2 * CHUNK + 1, 3 * CHUNK + 777] {
        let data = pattern(n);
        let bytes = sealed(&data, CHUNK);
        assert_eq!(bytes.len(), n + TAG * n.div_ceil(CHUNK), "{n}: whole chunks and a tag each");
        assert_eq!(read_all(bytes, n).unwrap(), data, "{n}");
    }
}

#[test]
fn writing_in_odd_pieces_gives_the_same_bytes() {
    let data = pattern(2 * CHUNK + 5000);
    let whole = sealed(&data, data.len());
    for piece in [1, 7, 1000, CHUNK - 1, CHUNK, CHUNK + 1] {
        assert_eq!(sealed(&data, piece), whole, "pieces of {piece}");
    }
}

#[test]
fn a_chunk_is_held_back_until_more_arrives_because_it_might_be_the_last() {
    // after exactly one chunk nothing has been written: it is held, in case it is the last
    let mut writer = ChunkWriter::new(Vec::new(), &KEY, header());
    writer.write(&pattern(CHUNK)).unwrap();
    assert!(writer.finish().unwrap().len() == SEALED, "one full chunk, sealed as the last");

    // one more byte and the full chunk is sealed as a middle one, and the byte as the last
    let two = sealed(&pattern(CHUNK + 1), CHUNK + 1);
    assert_eq!(two.len(), SEALED + 1 + TAG);
    // and the first chunk of the two differs from the same chunk sealed as a last one (the flag is in the nonce)
    let alone = sealed(&pattern(CHUNK), CHUNK);
    assert_ne!(two[..SEALED], alone[..]);
}

#[test]
fn a_changed_bit_anywhere_is_refused() {
    let data = pattern(2 * CHUNK + 100);
    let bytes = sealed(&data, CHUNK);
    let mut positions = vec![0, 1, TAG, CHUNK - 1, CHUNK, SEALED - 1, SEALED, SEALED + 1, 2 * SEALED - 1, 2 * SEALED, bytes.len() - 1];
    positions.extend((0..bytes.len()).step_by(4099));
    for at in positions {
        let mut changed = bytes.clone();
        changed[at] ^= 0x01;
        assert!(read_all(changed, data.len()).is_err(), "bit changed at {at}");
    }
}

#[test]
fn a_wrong_key_is_wrong_credentials_in_the_first_chunk_and_damage_after_it() {
    let data = pattern(3 * CHUNK);
    let bytes = sealed(&data, CHUNK);
    let len = bytes.len() as u64;
    let mut wrong = ChunkReader::new(std::io::Cursor::new(bytes.clone()), &[8; 32], header(), len);
    let mut out = vec![0u8; 10];
    assert_eq!(wrong.take(&mut out), Err(AppError::WrongCredentials));

    let mut damaged = bytes;
    damaged[SEALED + 5] ^= 1;
    let mut reader = reader(damaged);
    let mut out = vec![0u8; 3 * CHUNK];
    assert_eq!(reader.take(&mut out), Err(AppError::InvalidBackup), "the key was right, since the first chunk opened");
}

#[test]
fn another_header_is_refused() {
    let bytes = sealed(&pattern(100), 100);
    let len = bytes.len() as u64;
    let mut reader = ChunkReader::new(std::io::Cursor::new(bytes), &KEY, b"another header".to_vec(), len);
    assert!(reader.take(&mut [0u8; 10]).is_err());
}

#[test]
fn a_stream_cut_anywhere_is_refused() {
    let data = pattern(3 * CHUNK + 1000);
    let bytes = sealed(&data, CHUNK);
    let mut cuts = vec![1, TAG - 1, TAG, SEALED - 1, SEALED, SEALED + 1, 2 * SEALED, 3 * SEALED, bytes.len() - 1];
    cuts.extend((0..bytes.len()).step_by(8191));
    for cut in cuts {
        let result = read_all(bytes[..cut].to_vec(), data.len());
        assert!(result.is_err(), "cut at {cut}");
    }
    // the cut at a chunk boundary is the one a plain reader would not notice: the last flag is what catches it
    assert_eq!(read_all(bytes[..SEALED].to_vec(), CHUNK), Err(AppError::WrongCredentials), "the first chunk was sealed as a middle one");
    assert_eq!(read_all(bytes[..3 * SEALED].to_vec(), 3 * CHUNK), Err(AppError::InvalidBackup));
}

#[test]
fn chunks_moved_dropped_repeated_or_added_are_refused() {
    let data = pattern(3 * CHUNK + 1000);
    let bytes = sealed(&data, CHUNK);
    let chunk = |i: usize| bytes[i * SEALED..((i + 1) * SEALED).min(bytes.len())].to_vec();
    let join = |parts: &[Vec<u8>]| parts.concat();

    let swapped = join(&[chunk(1), chunk(0), chunk(2), chunk(3)]);
    let dropped = join(&[chunk(0), chunk(2), chunk(3)]);
    let repeated = join(&[chunk(0), chunk(1), chunk(1), chunk(2), chunk(3)]);
    let added_copy = join(&[chunk(0), chunk(1), chunk(2), chunk(3), chunk(0)]);
    let added_garbage = join(&[bytes.clone(), vec![9u8; 40]]);
    let added_tag_only = join(&[bytes.clone(), vec![0u8; TAG]]);
    let last_first = join(&[chunk(3), chunk(0), chunk(1), chunk(2)]);
    for (what, file) in [
        ("swapped", swapped),
        ("dropped", dropped),
        ("repeated", repeated),
        ("added a copy", added_copy),
        ("added garbage", added_garbage),
        ("added a tag", added_tag_only),
        ("the last one first", last_first),
    ] {
        assert!(read_all(file, data.len()).is_err(), "{what}");
    }
    // the same data still reads
    assert_eq!(read_all(bytes, data.len()).unwrap(), data);
}

#[test]
fn a_stream_that_ends_early_or_late_in_the_records_is_not_whole() {
    let data = pattern(100);
    let bytes = sealed(&data, 100);
    // asking for more than there is
    assert_eq!(read_all(bytes.clone(), 101), Err(AppError::InvalidBackup));
    // asking for less leaves bytes behind, and `finish` says so
    let mut reader = reader(bytes.clone());
    let mut out = vec![0u8; 99];
    reader.take(&mut out).unwrap();
    assert_eq!(reader.finish(), Err(AppError::InvalidBackup));
    // and a stream with chunks left after what was read
    let long = sealed(&pattern(2 * CHUNK), CHUNK);
    let mut reader = self::reader(long);
    let mut out = vec![0u8; CHUNK];
    reader.take(&mut out).unwrap();
    assert_eq!(reader.finish(), Err(AppError::InvalidBackup));
}

#[test]
fn an_empty_chunk_is_refused_and_does_not_hang_the_reader() {
    // a writer that is finished with nothing written seals one empty chunk
    let bytes = ChunkWriter::new(Vec::new(), &KEY, header()).finish().unwrap();
    assert_eq!(bytes.len(), TAG);
    assert_eq!(read_all(bytes, 1), Err(AppError::InvalidBackup));
}

#[test]
fn a_length_that_lies_cannot_ask_for_memory_the_stream_does_not_hold() {
    let bytes = sealed(&pattern(1000), 1000);
    let mut reader = reader(bytes);
    // 4 TiB: the reader grows as bytes arrive and fails when they stop
    assert_eq!(reader.take_vec(4 * 1024 * 1024 * 1024 * 1024).map(|v| v.len()), Err(AppError::InvalidBackup));
}

#[test]
fn skipping_reads_and_checks_what_it_passes() {
    let data = pattern(2 * CHUNK + 10);
    let bytes = sealed(&data, CHUNK);
    let mut reader = reader(bytes.clone());
    reader.skip(2 * CHUNK as u64 + 10).unwrap();
    reader.finish().unwrap();

    let mut damaged = bytes;
    damaged[SEALED + 3] ^= 0x10;
    let mut reader = self::reader(damaged);
    assert!(reader.skip(2 * CHUNK as u64 + 10).is_err(), "what is skipped is still checked");
}
