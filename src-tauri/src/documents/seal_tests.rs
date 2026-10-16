use super::seal::*;
use crate::db::DbKey;
use crate::keys;

// Written by `python3 scripts/doc-vectors.py --rust`, a second implementation made from RFC 5869 and RFC 8439 that checks
// itself against the RFCs' own test vectors before it prints anything.
const DB_KEY: &str = "101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f";
const DOC_ID: &str = "c0c1c2c3c4c5c6c7c8c9cacbcccdcecf";
const SALT: &str = "505152535455565758595a5b5c5d5e5f";
const ROOT: &str = "df4138942e18bfb39256fdfb284956ee1e888a5843ae75f0f54853e116b9bf6c";
const STEM: &str = "56b03b0814cca9baa88f8c37bfb33a61";
const FILE_KEY: &str = "ebc114cc8c2c0e807571cd66f14e5d6c64ff0f1118dc4a94bc58a97a7792cefd";
const HEADER: &str = "4d454f574e594446000100010000505152535455565758595a5b5c5d5e5f";
const EMPTY_LEN: usize = 46;
const EMPTY_SHA256: &str = "5743aa578027f5b200d0979b0e764d2530b789be5bfbf220f55f1ff802012690";
const EMPTY_SEALED: &str = "4d454f574e594446000100010000505152535455565758595a5b5c5d5e5fda9aa779a65d08c287424db2671abb32";
const ONE_BYTE_LEN: usize = 47;
const ONE_BYTE_SHA256: &str = "4e62480ea92348524129bafb57dfcf37aa16455dd05d96c3dd46d1c7bc259aa4";
const ONE_BYTE_SEALED: &str = "4d454f574e594446000100010000505152535455565758595a5b5c5d5e5f0f821c3fb1240c48ec36b1769df65f0919";
const SHORT_LEN: usize = 146;
const SHORT_SHA256: &str = "408119a5e215ae0d73600c4997395db7c299aad060f7e84d578abfdd465688ae";
const SHORT_SEALED: &str = "4d454f574e594446000100010000505152535455565758595a5b5c5d5e5f4e0ae804c22cd3eba2ee8a8acb4b89a9543ee9c65111a6172fcbfcff5783767512b6209a0c1973c7dca732d39b365437dd9018f2cd32116979e892d87f4f6561bb5d60bfb12fef7279b457e71bc6e25015f4d108fea6e447a5571fba47def81019408436b10db96a0cfe10209ce07a454cc0ee4c";
const CHUNK_LESS_ONE_LEN: usize = 65581;
const CHUNK_LESS_ONE_SHA256: &str = "cee39d2b2a9d4211f5bffbb224ecc54b3a2f49e06d96b6adb3d709b9cea435f7";
const ONE_CHUNK_LEN: usize = 65582;
const ONE_CHUNK_SHA256: &str = "c7b2d708b28fa52de4c5b63ca82ac6249079e65e88ebe78781072ba0088d1c86";
const CHUNK_PLUS_ONE_LEN: usize = 65599;
const CHUNK_PLUS_ONE_SHA256: &str = "35f0d7020357c176330b76b6e721a2477f0a2d34a6fd7b33a0a504b942e4d01d";
const TWO_CHUNKS_LEN: usize = 131134;
const TWO_CHUNKS_SHA256: &str = "7a3a90cf227957f0654072a67cbece5ff8c3bca4a1b2306c72b0de6ccca4c4e3";
const TWO_AND_A_BIT_LEN: usize = 150078;
const TWO_AND_A_BIT_SHA256: &str = "dbf01d2953c303e64126e0c00bbb6f03e0de4a6e13e4b5fc29c5725949d8bad7";

fn hex<const N: usize>(text: &str) -> [u8; N] {
    keys::hex_decode::<N>(text).expect("test vector")
}

fn to_hex(bytes: &[u8]) -> String {
    keys::hex_encode(bytes)
}

fn doc_keys() -> DocKeys {
    DocKeys::from_db_key(&DbKey::from_bytes(hex(DB_KEY)))
}

fn other_keys() -> DocKeys {
    DocKeys::from_db_key(&DbKey::from_bytes([9; 32]))
}

fn id() -> DocId {
    DocId::from_bytes(hex(DOC_ID))
}

/// Byte i is i modulo 251, as in the reference implementation.
fn pattern(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(&crate::documents::store::sha256(bytes))
}

/// The same file the reference implementation makes: the fixed salt instead of a random one.
fn sealed_with_fixed_salt(plain: &[u8]) -> Vec<u8> {
    let mut sealer = Sealer::with_salt(Vec::new(), &doc_keys(), &id(), plain.len() as u64, hex(SALT)).unwrap();
    sealer.write(plain).unwrap();
    sealer.finish().unwrap()
}

/// A file of two full chunks and ten bytes, so there is a middle chunk and a short last one.
fn three_chunks() -> (Vec<u8>, Vec<u8>) {
    let plain = pattern(2 * CHUNK_SIZE + 10);
    let sealed = seal_all(&doc_keys(), &id(), &plain).unwrap();
    assert_eq!(sealed.len(), HEADER_LEN + 2 * SEALED_CHUNK + 10 + TAG_LEN);
    (plain, sealed)
}

fn refused(sealed: &[u8]) -> bool {
    open_all(&doc_keys(), &id(), sealed).is_err()
}

#[test]
fn the_sizes_are_the_ones_in_the_design() {
    assert_eq!((HEADER_LEN, CHUNK_SIZE, TAG_LEN, SEALED_CHUNK), (30, 65536, 16, 65552));
    assert_eq!(MAX_DOCUMENT_BYTES, 26_214_400);
    assert_eq!(MAX_DOCUMENT_BYTES / CHUNK_SIZE as u64, 400);
    assert_eq!(MAX_SEALED_BYTES, 30 + 400 * 65552);
    assert_eq!((chunk_count(0), chunk_count(1), chunk_count(65536), chunk_count(65537)), (1, 1, 1, 2));
    assert_eq!((sealed_len(0), sealed_len(1), sealed_len(65536), sealed_len(65537)), (46, 47, 65582, 65599));
}

#[test]
fn the_keys_and_the_name_are_those_of_the_reference_implementation() {
    let keys = doc_keys();
    assert_eq!(to_hex(keys.root()), ROOT);
    assert_eq!(keys.stem(&id()), STEM);
    assert_eq!(keys.file_name(&id()), format!("{STEM}.mnd"));
    assert_eq!(to_hex(keys.file_key(&hex(SALT), &id()).as_ref()), FILE_KEY);
}

#[test]
fn sealed_files_are_the_bytes_of_the_reference_implementation() {
    let cases: [(usize, usize, &str, Option<&str>); 8] = [
        (0, EMPTY_LEN, EMPTY_SHA256, Some(EMPTY_SEALED)),
        (1, ONE_BYTE_LEN, ONE_BYTE_SHA256, Some(ONE_BYTE_SEALED)),
        (100, SHORT_LEN, SHORT_SHA256, Some(SHORT_SEALED)),
        (CHUNK_SIZE - 1, CHUNK_LESS_ONE_LEN, CHUNK_LESS_ONE_SHA256, None),
        (CHUNK_SIZE, ONE_CHUNK_LEN, ONE_CHUNK_SHA256, None),
        (CHUNK_SIZE + 1, CHUNK_PLUS_ONE_LEN, CHUNK_PLUS_ONE_SHA256, None),
        (2 * CHUNK_SIZE, TWO_CHUNKS_LEN, TWO_CHUNKS_SHA256, None),
        (150_000, TWO_AND_A_BIT_LEN, TWO_AND_A_BIT_SHA256, None),
    ];
    for (size, length, hash, whole) in cases {
        // The one byte case is the letter A, the others are the pattern.
        let plain = if size == 1 { b"A".to_vec() } else { pattern(size) };
        let sealed = sealed_with_fixed_salt(&plain);
        assert_eq!(sealed.len(), length, "length for {size} bytes");
        assert_eq!(sha256_hex(&sealed), hash, "hash for {size} bytes");
        if let Some(whole) = whole {
            assert_eq!(to_hex(&sealed), whole, "bytes for {size} bytes");
        }
        assert_eq!(open_all(&doc_keys(), &id(), &sealed).unwrap().as_slice(), plain.as_slice());
    }
}

#[test]
fn the_header_is_the_one_in_the_design() {
    let sealed = sealed_with_fixed_salt(b"x");
    assert_eq!(to_hex(&sealed[..HEADER_LEN]), HEADER);
    assert_eq!(&sealed[..8], b"MEOWNYDF");
    assert_eq!(&sealed[8..10], &[0, 1]);
    assert_eq!(&sealed[10..14], &[0, 1, 0, 0]);
}

#[test]
fn every_size_round_trips() {
    let c = CHUNK_SIZE;
    for size in [0, 1, 15, 16, 17, 255, c - 1, c, c + 1, 2 * c - 1, 2 * c, 2 * c + 1, 3 * c - 1, 3 * c, 150_001] {
        let plain = pattern(size);
        let sealed = seal_all(&doc_keys(), &id(), &plain).unwrap();
        assert_eq!(sealed.len() as u64, sealed_len(size as u64), "sealed length for {size}");
        let opened = open_all(&doc_keys(), &id(), &sealed).unwrap();
        assert_eq!(opened.as_slice(), plain.as_slice(), "contents for {size}");
    }
}

#[test]
fn the_plaintext_length_comes_from_the_length_of_the_file() {
    for size in [0u64, 1, 65_535, 65_536, 65_537, 200_000] {
        let sealed = seal_all(&doc_keys(), &id(), &pattern(size as usize)).unwrap();
        let opener = Opener::new(std::io::Cursor::new(&sealed), sealed.len() as u64, &doc_keys(), &id()).unwrap();
        assert_eq!(opener.plaintext_len(), size);
    }
}

#[test]
fn the_largest_file_round_trips_and_nothing_larger_is_taken() {
    let plain = pattern(MAX_DOCUMENT_BYTES as usize);
    let sealed = seal_all(&doc_keys(), &id(), &plain).unwrap();
    assert_eq!(sealed.len() as u64, MAX_SEALED_BYTES);
    assert_eq!(open_all(&doc_keys(), &id(), &sealed).unwrap().as_slice(), plain.as_slice());

    assert_eq!(Sealer::new(Vec::new(), &doc_keys(), &id(), MAX_DOCUMENT_BYTES + 1).err(), Some(DocError::TooLarge));
    let mut too_long = sealed;
    too_long.push(0);
    assert_eq!(open_all(&doc_keys(), &id(), &too_long).err(), Some(DocError::TooLarge));
}

#[test]
fn writing_in_odd_pieces_gives_the_same_file() {
    let plain = pattern(3 * CHUNK_SIZE + 17);
    let whole = sealed_with_fixed_salt(&plain);
    for piece in [1usize, 7, 1000, CHUNK_SIZE - 1, CHUNK_SIZE, CHUNK_SIZE + 1, 70_000, plain.len()] {
        // one-byte pieces over 200 KB would be slow in a debug build; they are only tried on the start
        let (head, tail) = if piece == 1 { plain.split_at(300) } else { plain.split_at(0) };
        let mut sealer = Sealer::with_salt(Vec::new(), &doc_keys(), &id(), plain.len() as u64, hex(SALT)).unwrap();
        for part in head.chunks(1) {
            sealer.write(part).unwrap();
        }
        for part in tail.chunks(if piece == 1 { 4099 } else { piece }) {
            sealer.write(part).unwrap();
        }
        assert_eq!(sealer.finish().unwrap(), whole, "pieces of {piece}");
    }
}

#[test]
fn sealing_twice_gives_different_bytes_with_different_keys() {
    let a = seal_all(&doc_keys(), &id(), b"the same receipt").unwrap();
    let b = seal_all(&doc_keys(), &id(), b"the same receipt").unwrap();
    assert_ne!(a, b);
    assert_ne!(a[14..30], b[14..30], "a new salt every time");
}

#[test]
fn the_sealer_holds_to_the_size_it_was_told() {
    let mut sealer = Sealer::new(Vec::new(), &doc_keys(), &id(), 5).unwrap();
    sealer.write(b"abc").unwrap();
    assert_eq!(sealer.write(b"def").err(), Some(DocError::TooLarge));
    let mut short = Sealer::new(Vec::new(), &doc_keys(), &id(), 5).unwrap();
    short.write(b"abc").unwrap();
    assert_eq!(short.finish().err(), Some(DocError::Incomplete));
    let mut empty = Sealer::new(Vec::new(), &doc_keys(), &id(), 0).unwrap();
    assert_eq!(empty.write(b"x").err(), Some(DocError::TooLarge));
}

#[test]
fn a_changed_byte_anywhere_in_a_small_file_is_refused() {
    let sealed = seal_all(&doc_keys(), &id(), &pattern(100)).unwrap();
    assert!(!refused(&sealed));
    for position in 0..sealed.len() {
        for bit in [0x01u8, 0x80] {
            let mut damaged = sealed.clone();
            damaged[position] ^= bit;
            assert!(refused(&damaged), "byte {position}, bit {bit:#x}");
        }
    }
}

#[test]
fn a_changed_byte_in_a_big_file_is_refused() {
    let (_, sealed) = three_chunks();
    let mut positions = vec![0, 8, 9, 10, 13, 14, 29];
    for chunk in 0..3 {
        let start = HEADER_LEN + chunk * SEALED_CHUNK;
        let end = if chunk == 2 { sealed.len() } else { start + SEALED_CHUNK };
        // the first byte, a middle one, the first and the last byte of the tag
        positions.extend([start, start + (end - start) / 2, end - TAG_LEN, end - 1]);
    }
    for position in positions {
        let mut damaged = sealed.clone();
        damaged[position] ^= 0x40;
        assert!(refused(&damaged), "byte {position}");
    }
}

#[test]
fn a_cut_file_is_refused() {
    let (_, sealed) = three_chunks();
    let mut cuts = vec![0, 10, HEADER_LEN - 1, HEADER_LEN, HEADER_LEN + 1, sealed.len() - 1, sealed.len() - TAG_LEN, sealed.len() - TAG_LEN - 1];
    for chunk in 1..3 {
        let boundary = HEADER_LEN + chunk * SEALED_CHUNK;
        cuts.extend([boundary - 1, boundary, boundary + 1, boundary + TAG_LEN, boundary + TAG_LEN + 1]);
    }
    for cut in cuts {
        assert!(refused(&sealed[..cut]), "cut to {cut} bytes");
    }
    // cut exactly after a whole chunk: every chunk is intact but the one now last was sealed as a middle one
    assert_eq!(open_all(&doc_keys(), &id(), &sealed[..HEADER_LEN + SEALED_CHUNK]).err(), Some(DocError::Damaged));
    assert_eq!(open_all(&doc_keys(), &id(), &sealed[..HEADER_LEN + 2 * SEALED_CHUNK]).err(), Some(DocError::Damaged));
}

#[test]
fn a_file_with_something_added_is_refused() {
    let (_, sealed) = three_chunks();
    let mut with_byte = sealed.clone();
    with_byte.push(0);
    let mut with_tag = sealed.clone();
    with_tag.extend_from_slice(&[0u8; TAG_LEN]);
    let mut with_chunk = sealed.clone();
    with_chunk.extend_from_slice(&sealed[HEADER_LEN..HEADER_LEN + SEALED_CHUNK]);
    let mut with_last_again = sealed.clone();
    with_last_again.extend_from_slice(&sealed[HEADER_LEN + 2 * SEALED_CHUNK..]);
    for (name, file) in [("a byte", with_byte), ("an empty chunk", with_tag), ("a chunk", with_chunk), ("the last chunk again", with_last_again)] {
        assert!(refused(&file), "{name} added");
    }
    // the same for a file of exactly one chunk, where the extra sixteen bytes make a second, empty chunk
    let one = seal_all(&doc_keys(), &id(), &pattern(CHUNK_SIZE)).unwrap();
    let mut one_and_empty = one.clone();
    one_and_empty.extend_from_slice(&[0u8; TAG_LEN]);
    assert_eq!(open_all(&doc_keys(), &id(), &one_and_empty).err(), Some(DocError::Damaged));
}

#[test]
fn chunks_swapped_dropped_or_repeated_are_refused() {
    let (_, sealed) = three_chunks();
    let chunk = |i: usize| sealed[HEADER_LEN + i * SEALED_CHUNK..HEADER_LEN + (i + 1) * SEALED_CHUNK].to_vec();
    let tail = sealed[HEADER_LEN + 2 * SEALED_CHUNK..].to_vec();
    let build = |parts: &[&[u8]]| {
        let mut file = sealed[..HEADER_LEN].to_vec();
        for part in parts {
            file.extend_from_slice(part);
        }
        file
    };
    assert!(!refused(&build(&[&chunk(0), &chunk(1), &tail])));
    assert!(refused(&build(&[&chunk(1), &chunk(0), &tail])), "first two swapped");
    assert!(refused(&build(&[&chunk(0), &tail])), "the middle one dropped");
    assert!(refused(&build(&[&chunk(0), &chunk(0), &tail])), "the first repeated");
    assert!(refused(&build(&[&chunk(0), &chunk(1), &chunk(1), &tail])), "the middle repeated");
    assert!(refused(&build(&[&chunk(1), &chunk(1), &tail])), "the first replaced by the second");
}

#[test]
fn a_file_under_another_documents_name_or_another_key_does_not_open() {
    let (plain, sealed) = three_chunks();
    assert_eq!(open_all(&doc_keys(), &id(), &sealed).unwrap().as_slice(), plain.as_slice());
    let other_id = DocId::from_bytes([7; 16]);
    assert_eq!(open_all(&doc_keys(), &other_id, &sealed).err(), Some(DocError::Damaged));
    assert_eq!(open_all(&other_keys(), &id(), &sealed).err(), Some(DocError::Damaged));
}

#[test]
fn a_header_that_is_not_ours_is_a_format_error() {
    let sealed = seal_all(&doc_keys(), &id(), b"receipt").unwrap();
    let change = |at: usize, to: &[u8]| {
        let mut file = sealed.clone();
        file[at..at + to.len()].copy_from_slice(to);
        file
    };
    for (name, file) in [
        ("another magic", change(0, b"MEOWNYDX")),
        ("another format", change(8, &[0, 2])),
        ("another chunk size", change(10, &[0, 0, 128, 0])),
        ("a chunk size of zero", change(10, &[0, 0, 0, 0])),
    ] {
        assert_eq!(open_all(&doc_keys(), &id(), &file).err(), Some(DocError::Format), "{name}");
    }
    assert_eq!(open_all(&doc_keys(), &id(), &sealed[..HEADER_LEN + TAG_LEN - 1]).err(), Some(DocError::Format));
    assert_eq!(open_all(&doc_keys(), &id(), &[]).err(), Some(DocError::Format));
    assert_eq!(open_all(&doc_keys(), &id(), b"%PDF-1.4 a plain receipt that is not sealed at all").err(), Some(DocError::Format));
}

#[test]
fn ids_are_32_lowercase_hex_characters() {
    let good = "00112233445566778899aabbccddeeff";
    assert_eq!(DocId::parse(good).unwrap().to_hex(), good);
    for bad in ["", "0011", "00112233445566778899aabbccddeeff0", "00112233445566778899aabbccddeefg", "00112233445566778899AABBCCDDEEFF", " 0112233445566778899aabbccddeeff", "../etc/passwd"] {
        assert_eq!(DocId::parse(bad).err(), Some(DocError::BadId), "{bad:?}");
    }
}

#[test]
fn file_names_are_stable_and_hide_the_id() {
    let keys = doc_keys();
    let a = DocId::from_bytes([1; 16]);
    let b = DocId::from_bytes([2; 16]);
    assert_eq!(keys.stem(&a), keys.stem(&a));
    assert_ne!(keys.stem(&a), keys.stem(&b));
    assert_ne!(keys.stem(&a), other_keys().stem(&a), "another database key gives other names");
    let name = keys.file_name(&a);
    assert_eq!(name.len(), 32 + 4);
    assert!(name.ends_with(".mnd"));
    assert!(name[..32].bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
    assert!(!name.contains(&a.to_hex()));
}

#[test]
fn the_start_of_an_unfinished_file_is_read_up_to_the_last_good_chunk() {
    let keys = doc_keys();
    let (plain, sealed) = three_chunks();
    let prefix = |bytes: &[u8]| {
        let mut seen = Vec::new();
        let (_, good) = read_prefix(std::io::Cursor::new(bytes), bytes.len() as u64, &keys, &id(), |p| seen.extend_from_slice(p)).unwrap();
        (good, seen)
    };
    // a finished file: its last chunk was sealed as the last, so only the two before it count as the start of a file
    let (good, seen) = prefix(&sealed);
    assert_eq!((good, seen.as_slice()), (2, &plain[..2 * CHUNK_SIZE]));
    // exact multiple of the chunk size: the final full chunk is the last one too
    let two = seal_all(&keys, &id(), &pattern(2 * CHUNK_SIZE)).unwrap();
    assert_eq!(prefix(&two).0, 1);
    // cut inside the third chunk, or in the middle of the second: the whole chunks before the cut count
    assert_eq!(prefix(&sealed[..HEADER_LEN + 2 * SEALED_CHUNK + 5]).0, 2);
    assert_eq!(prefix(&sealed[..HEADER_LEN + SEALED_CHUNK + 100]).0, 1);
    assert_eq!(prefix(&sealed[..HEADER_LEN]).0, 0);
    // a damaged chunk ends the start there
    let mut damaged = sealed.clone();
    damaged[HEADER_LEN + SEALED_CHUNK + 3] ^= 1;
    assert_eq!(prefix(&damaged).0, 1);
    // not a sealed file at all
    assert!(read_prefix(std::io::Cursor::new(b"junk"), 4, &keys, &id(), |_| {}).is_err());
}

#[test]
fn resuming_needs_something_left_to_write() {
    let sealed = sealed_with_fixed_salt(&pattern(2 * CHUNK_SIZE));
    let header: [u8; HEADER_LEN] = sealed[..HEADER_LEN].try_into().unwrap();
    assert!(Sealer::resume(Vec::new(), &doc_keys(), &id(), 2 * CHUNK_SIZE as u64, &header, 1).is_ok());
    assert_eq!(Sealer::resume(Vec::new(), &doc_keys(), &id(), 2 * CHUNK_SIZE as u64, &header, 2).err(), Some(DocError::Format));
    assert_eq!(Sealer::resume(Vec::new(), &doc_keys(), &id(), MAX_DOCUMENT_BYTES + 1, &header, 1).err(), Some(DocError::TooLarge));
}
