use super::library::{LinkTarget, NewDocument, TargetKind};
use super::seal::MAX_DOCUMENT_BYTES;
use super::upload::{Uploads, MAX_PIECE_TEXT};
use crate::sync::qr::base64url_encode;
use crate::vault::AppError;

fn input() -> NewDocument {
    NewDocument { name: "Receipt.pdf".into(), ..Default::default() }
}

fn file(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

/// Sends a file the way the page does, in pieces of `piece` bytes.
fn send(uploads: &Uploads, bytes: &[u8], piece: usize) -> Result<(), AppError> {
    uploads.begin(input(), bytes.len() as u64)?;
    let mut offset = 0;
    for part in bytes.chunks(piece) {
        let now = uploads.push(offset, &base64url_encode(part))?;
        offset += part.len() as u64;
        assert_eq!(now, offset);
    }
    Ok(())
}

#[test]
fn a_file_sent_in_pieces_is_put_together_whole() {
    for (size, piece) in [(1usize, 1usize), (10, 3), (300_000, 100_000), (300_001, 7919), (393_216, 393_216)] {
        let uploads = Uploads::default();
        let bytes = file(size);
        send(&uploads, &bytes, piece).unwrap();
        let (described, got) = uploads.finish().unwrap();
        assert_eq!(got.as_slice(), bytes.as_slice(), "{size} bytes in pieces of {piece}");
        assert_eq!(described.name, "Receipt.pdf");
        assert!(uploads.is_empty(), "finishing ends the upload");
    }
}

#[test]
fn what_is_wrong_with_the_description_is_said_before_any_byte_is_sent() {
    let uploads = Uploads::default();
    assert_eq!(uploads.begin(input(), 0), Err(AppError::InvalidInput), "an empty file");
    assert_eq!(uploads.begin(input(), MAX_DOCUMENT_BYTES + 1), Err(AppError::TooLarge));
    assert!(uploads.begin(input(), MAX_DOCUMENT_BYTES).is_ok(), "exactly 25 MiB is allowed");
    let bad_date = NewDocument { document_date: Some("2026-02-30".into()), ..input() };
    assert_eq!(uploads.begin(bad_date, 10), Err(AppError::InvalidInput));
    let bad_link = NewDocument { links: vec![LinkTarget { kind: TargetKind::Transaction, id: "abc".into() }], ..input() };
    assert_eq!(uploads.begin(bad_link, 10), Err(AppError::InvalidInput));
    let many = NewDocument { links: (0..51).map(|_| LinkTarget { kind: TargetKind::Recurring, id: "1".into() }).collect(), ..input() };
    assert_eq!(uploads.begin(many, 10), Err(AppError::InvalidInput));
}

#[test]
fn pieces_must_come_in_order_and_be_valid_or_the_upload_is_dropped() {
    let a = base64url_encode(b"hello");
    let cases: Vec<(&str, u64, String)> = vec![
        ("a gap", 3, a.clone()),
        ("a repeat", 0, a.clone()),
        ("padding", 5, "aGVsbG8=".into()),
        ("the other alphabet", 5, "a+b/".into()),
        ("a bad length", 5, "aGVsb".into()),
        ("spaces", 5, "aGVs bG8".into()),
        ("too much", 5, base64url_encode(b"world, and more than the ten that were announced")),
        ("too long a piece", 5, "A".repeat(MAX_PIECE_TEXT + 4)),
    ];
    for (name, offset, text) in cases {
        let uploads = Uploads::default();
        uploads.begin(input(), 10).unwrap();
        uploads.push(0, &a).unwrap();
        assert_eq!(uploads.push(offset, &text), Err(AppError::InvalidInput), "{name}");
        assert!(uploads.is_empty(), "{name}: the upload is over");
        assert_eq!(uploads.finish().err(), Some(AppError::InvalidInput), "{name}");
    }
}

#[test]
fn a_file_that_is_not_all_here_is_not_handed_over() {
    let uploads = Uploads::default();
    uploads.begin(input(), 10).unwrap();
    uploads.push(0, &base64url_encode(b"short")).unwrap();
    assert_eq!(uploads.finish().err(), Some(AppError::InvalidInput));
    assert!(uploads.is_empty());
    assert_eq!(uploads.finish().err(), Some(AppError::InvalidInput), "nothing to finish");
    assert_eq!(uploads.push(0, "AAAA").err(), Some(AppError::InvalidInput), "nothing to add to");
}

#[test]
fn starting_another_upload_drops_the_first_and_so_does_clearing() {
    let uploads = Uploads::default();
    uploads.begin(input(), 10).unwrap();
    uploads.push(0, &base64url_encode(b"first")).unwrap();
    uploads.begin(NewDocument { name: "Second.pdf".into(), ..Default::default() }, 3).unwrap();
    uploads.push(0, &base64url_encode(b"abc")).unwrap();
    let (described, bytes) = uploads.finish().unwrap();
    assert_eq!((described.name.as_str(), bytes.as_slice()), ("Second.pdf", &b"abc"[..]));

    uploads.begin(input(), 10).unwrap();
    uploads.push(0, &base64url_encode(b"half")).unwrap();
    uploads.clear();
    assert!(uploads.is_empty());
}

#[test]
fn the_largest_file_goes_through_in_the_pieces_the_page_sends() {
    let uploads = Uploads::default();
    let bytes = file(MAX_DOCUMENT_BYTES as usize);
    send(&uploads, &bytes, 256 * 1024).unwrap();
    let (_, got) = uploads.finish().unwrap();
    assert_eq!(got.len() as u64, MAX_DOCUMENT_BYTES);
    assert_eq!(got.as_slice(), bytes.as_slice());
}
