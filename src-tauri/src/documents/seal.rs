//! The sealed file and the keys it needs (`docs/documents-design.md`, sections 3 and 4).
//!
//! A document is stored as a header and a list of chunks, each sealed with ChaCha20-Poly1305 under a key of its own file.
//! The layout is the usual STREAM one: the nonce of a chunk holds its number and a flag that is set on the last chunk, so a
//! changed, cut, extended, reordered or moved file does not open. Nothing here touches the disk or the database: callers
//! give it readers, writers and bytes. The known answers in the tests come from a second implementation written from the
//! RFCs (`scripts/doc-vectors.py`, in git history up to 07a329b).

use std::io::{Read, Write};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::db::DbKey;
use crate::keys;

pub const MAGIC: &[u8; 8] = b"MEOWNYDF";
pub const FORMAT: u16 = 1;
/// Plaintext bytes in every chunk but the last. It is written into the header, and a file with another value is refused.
pub const CHUNK_SIZE: usize = 64 * 1024;
pub const TAG_LEN: usize = 16;
pub const SALT_LEN: usize = 16;
pub const HEADER_LEN: usize = 8 + 2 + 4 + SALT_LEN;
/// The most one document may hold: 25 MiB, which is exactly 400 chunks.
pub const MAX_DOCUMENT_BYTES: u64 = 25 * 1024 * 1024;
pub const SEALED_CHUNK: usize = CHUNK_SIZE + TAG_LEN;
/// The longest sealed file there can be.
pub const MAX_SEALED_BYTES: u64 = HEADER_LEN as u64 + (MAX_DOCUMENT_BYTES / CHUNK_SIZE as u64) * SEALED_CHUNK as u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocError {
    /// Not 32 lowercase hex characters.
    BadId,
    /// Over the limit of 25 MiB, or more bytes than the size that was announced.
    TooLarge,
    /// Not a sealed file of this format.
    Format,
    /// It did not open: changed, cut, extended, reordered, moved to another document, or the wrong key.
    Damaged,
    /// It opened, but it is not what the row says (size or hash).
    Mismatch,
    /// Fewer bytes than were announced.
    Incomplete,
    /// There is no file.
    Missing,
    Io(std::io::ErrorKind),
    Random,
}

impl std::fmt::Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            DocError::BadId => "not a document id",
            DocError::TooLarge => "the file is too large",
            DocError::Format => "not a sealed document",
            DocError::Damaged => "the file did not open",
            DocError::Mismatch => "the file is not what was announced",
            DocError::Incomplete => "the file is not complete",
            DocError::Missing => "there is no file",
            DocError::Io(_) => "the file could not be read or written",
            DocError::Random => "no random numbers",
        })
    }
}

impl std::error::Error for DocError {}

impl From<std::io::Error> for DocError {
    fn from(e: std::io::Error) -> Self {
        DocError::Io(e.kind())
    }
}

impl From<keys::KeyError> for DocError {
    fn from(_: keys::KeyError) -> Self {
        DocError::Random
    }
}

/// The `sync_id` of a document as 16 bytes. It goes into the key and the name of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocId([u8; 16]);

impl DocId {
    /// 32 lowercase hex characters, as the database and the sync write them.
    pub fn parse(text: &str) -> Result<DocId, DocError> {
        if text.bytes().any(|b| b.is_ascii_uppercase()) {
            return Err(DocError::BadId);
        }
        keys::hex_decode::<16>(text).map(DocId).ok_or(DocError::BadId)
    }

    pub fn from_bytes(bytes: [u8; 16]) -> DocId {
        DocId(bytes)
    }

    pub fn bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        keys::hex_encode(&self.0)
    }
}

/// What is needed to seal and open files while the app is unlocked: one key made from the database key. It is dropped,
/// and zeroed, together with the connection when the app locks, so a locked app cannot read a file.
#[derive(Clone)]
pub struct DocKeys(Zeroizing<[u8; 32]>);

impl DocKeys {
    pub fn from_db_key(key: &DbKey) -> DocKeys {
        let mut root = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(None, key.as_bytes())
            .expand(b"meowny documents v1", root.as_mut())
            .expect("an output of at most 255 hashes");
        DocKeys(root)
    }

    #[cfg(test)]
    pub(crate) fn root(&self) -> &[u8; 32] {
        &self.0
    }

    /// The name of the file of a document: 32 hex characters that say nothing about the document. It is worked out from the
    /// row, so nothing has to store it.
    pub fn stem(&self, id: &DocId) -> String {
        let mut info = Vec::with_capacity(19 + 16);
        info.extend_from_slice(b"meowny doc name v1");
        info.extend_from_slice(id.bytes());
        let mut out = [0u8; 16];
        Hkdf::<Sha256>::from_prk(self.0.as_ref())
            .expect("a prk of 32 bytes")
            .expand(&info, &mut out)
            .expect("an output of at most 255 hashes");
        keys::hex_encode(&out)
    }

    pub fn file_name(&self, id: &DocId) -> String {
        format!("{}.mnd", self.stem(id))
    }

    /// The key of one file: its own salt and the id of its document, so no two files share a key and a file moved to the
    /// name of another document does not open.
    pub(crate) fn file_key(&self, salt: &[u8; SALT_LEN], id: &DocId) -> Zeroizing<[u8; 32]> {
        let mut info = Vec::with_capacity(18 + 16);
        info.extend_from_slice(b"meowny doc file v1");
        info.extend_from_slice(id.bytes());
        let mut out = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(Some(salt), self.0.as_ref()).expand(&info, out.as_mut()).expect("an output of at most 255 hashes");
        out
    }
}

fn header_bytes(salt: &[u8; SALT_LEN]) -> [u8; HEADER_LEN] {
    let mut header = [0u8; HEADER_LEN];
    header[..8].copy_from_slice(MAGIC);
    header[8..10].copy_from_slice(&FORMAT.to_be_bytes());
    header[10..14].copy_from_slice(&(CHUNK_SIZE as u32).to_be_bytes());
    header[14..].copy_from_slice(salt);
    header
}

fn parse_header(header: &[u8; HEADER_LEN]) -> Result<[u8; SALT_LEN], DocError> {
    let format = u16::from_be_bytes([header[8], header[9]]);
    let chunk = u32::from_be_bytes([header[10], header[11], header[12], header[13]]);
    if &header[..8] != MAGIC || format != FORMAT || chunk as usize != CHUNK_SIZE {
        return Err(DocError::Format);
    }
    Ok(header[14..].try_into().expect("16 bytes"))
}

/// The first byte says "this is the last chunk", the last eight are its number. A key is used for one file only and the
/// numbers do not repeat in a file, so no nonce is used twice under one key.
fn nonce_bytes(index: u64, last: bool) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[0] = last as u8;
    nonce[4..].copy_from_slice(&index.to_be_bytes());
    nonce
}

fn cipher_for(keys: &DocKeys, id: &DocId, salt: &[u8; SALT_LEN]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(Key::from_slice(keys.file_key(salt, id).as_ref()))
}

fn seal_chunk(cipher: &ChaCha20Poly1305, header: &[u8; HEADER_LEN], index: u64, last: bool, plain: &[u8]) -> Vec<u8> {
    cipher
        .encrypt(Nonce::from_slice(&nonce_bytes(index, last)), Payload { msg: plain, aad: header })
        .expect("sealing a chunk cannot fail")
}

fn open_chunk(cipher: &ChaCha20Poly1305, header: &[u8; HEADER_LEN], index: u64, last: bool, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, DocError> {
    cipher
        .decrypt(Nonce::from_slice(&nonce_bytes(index, last)), Payload { msg: sealed, aad: header })
        .map(Zeroizing::new)
        .map_err(|_| DocError::Damaged)
}

/// How many chunks hold `plain` bytes: an empty file is still one chunk, the last.
pub fn chunk_count(plain: u64) -> u64 {
    plain.div_ceil(CHUNK_SIZE as u64).max(1)
}

/// The length of the sealed file that holds `plain` bytes.
pub fn sealed_len(plain: u64) -> u64 {
    HEADER_LEN as u64 + plain + TAG_LEN as u64 * chunk_count(plain)
}

/// Writes a sealed file. The size is known before the first byte (a person's file, a row, a backup record), so the sealer
/// knows which chunk is the last without holding one back.
pub struct Sealer<W: Write> {
    out: W,
    cipher: ChaCha20Poly1305,
    header: [u8; HEADER_LEN],
    size: u64,
    fed: u64,
    index: u64,
    buf: Zeroizing<Vec<u8>>,
}

impl<W: Write> Sealer<W> {
    /// Starts a file of `size` bytes with a new random salt and writes its header.
    pub fn new(out: W, keys: &DocKeys, id: &DocId, size: u64) -> Result<Self, DocError> {
        Self::with_salt(out, keys, id, size, keys::random_bytes::<SALT_LEN>()?)
    }

    pub(crate) fn with_salt(mut out: W, keys: &DocKeys, id: &DocId, size: u64, salt: [u8; SALT_LEN]) -> Result<Self, DocError> {
        if size > MAX_DOCUMENT_BYTES {
            return Err(DocError::TooLarge);
        }
        let header = header_bytes(&salt);
        out.write_all(&header)?;
        Ok(Sealer { out, cipher: cipher_for(keys, id, &salt), header, size, fed: 0, index: 0, buf: Zeroizing::new(Vec::new()) })
    }

    /// Carries on a file whose first `chunks_done` chunks are already written (a transfer that stopped). `out` must be
    /// positioned right after them, and `header` is the header of that file.
    pub fn resume(out: W, keys: &DocKeys, id: &DocId, size: u64, header: &[u8; HEADER_LEN], chunks_done: u64) -> Result<Self, DocError> {
        let salt = parse_header(header)?;
        let plain_done = chunks_done * CHUNK_SIZE as u64;
        if size > MAX_DOCUMENT_BYTES {
            return Err(DocError::TooLarge);
        }
        // There must be something left to write, or the last chunk is already in the file.
        if plain_done >= size {
            return Err(DocError::Format);
        }
        Ok(Sealer { out, cipher: cipher_for(keys, id, &salt), header: *header, size, fed: plain_done, index: chunks_done, buf: Zeroizing::new(Vec::new()) })
    }

    fn emit(&mut self, chunk: &[u8]) -> Result<(), DocError> {
        let last = self.index * CHUNK_SIZE as u64 + chunk.len() as u64 == self.size;
        let sealed = seal_chunk(&self.cipher, &self.header, self.index, last, chunk);
        self.out.write_all(&sealed)?;
        self.index += 1;
        Ok(())
    }

    /// Takes the next bytes of the file, in any pieces.
    pub fn write(&mut self, mut data: &[u8]) -> Result<(), DocError> {
        if self.fed + data.len() as u64 > self.size {
            return Err(DocError::TooLarge);
        }
        self.fed += data.len() as u64;
        while !data.is_empty() {
            if self.buf.is_empty() && data.len() >= CHUNK_SIZE {
                let (chunk, rest) = data.split_at(CHUNK_SIZE);
                self.emit(chunk)?;
                data = rest;
                continue;
            }
            let take = (CHUNK_SIZE - self.buf.len()).min(data.len());
            self.buf.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buf.len() == CHUNK_SIZE {
                let chunk = std::mem::take(&mut self.buf);
                self.emit(&chunk)?;
            }
        }
        Ok(())
    }

    /// Seals what is left as the last chunk. Fails if fewer bytes came than were announced.
    pub fn finish(mut self) -> Result<W, DocError> {
        if self.fed != self.size {
            return Err(DocError::Incomplete);
        }
        if !self.buf.is_empty() || (self.size == 0 && self.index == 0) {
            let chunk = std::mem::take(&mut self.buf);
            self.emit(&chunk)?;
        }
        self.out.flush()?;
        Ok(self.out)
    }
}

/// Reads a sealed file chunk by chunk. The length of the file says how many chunks there are and so which one is the last.
pub struct Opener<R: Read> {
    input: R,
    cipher: ChaCha20Poly1305,
    header: [u8; HEADER_LEN],
    chunks: u64,
    next: u64,
    last_len: usize,
    plain_len: u64,
}

impl<R: Read> Opener<R> {
    pub fn new(mut input: R, file_len: u64, keys: &DocKeys, id: &DocId) -> Result<Self, DocError> {
        if file_len > MAX_SEALED_BYTES {
            return Err(DocError::TooLarge);
        }
        if file_len < (HEADER_LEN + TAG_LEN) as u64 {
            return Err(DocError::Format);
        }
        let mut header = [0u8; HEADER_LEN];
        input.read_exact(&mut header).map_err(|_| DocError::Format)?;
        let salt = parse_header(&header)?;
        let body = file_len - HEADER_LEN as u64;
        let chunks = body.div_ceil(SEALED_CHUNK as u64);
        let last_len = (body - (chunks - 1) * SEALED_CHUNK as u64) as usize;
        // A last chunk holds at least one byte, unless the whole file is empty: the sealer never writes any other shape.
        if last_len < TAG_LEN || (chunks > 1 && last_len == TAG_LEN) {
            return Err(DocError::Damaged);
        }
        Ok(Opener { input, cipher: cipher_for(keys, id, &salt), header, chunks, next: 0, last_len, plain_len: body - TAG_LEN as u64 * chunks })
    }

    /// The size of the contents, worked out from the length of the file (it is proved only when every chunk has opened).
    pub fn plaintext_len(&self) -> u64 {
        self.plain_len
    }

    /// The next chunk, opened, or none after the last.
    pub fn next_chunk(&mut self) -> Result<Option<Zeroizing<Vec<u8>>>, DocError> {
        if self.next == self.chunks {
            return Ok(None);
        }
        let last = self.next + 1 == self.chunks;
        let mut sealed = vec![0u8; if last { self.last_len } else { SEALED_CHUNK }];
        self.input.read_exact(&mut sealed).map_err(|_| DocError::Damaged)?;
        let plain = open_chunk(&self.cipher, &self.header, self.next, last, &sealed)?;
        self.next += 1;
        Ok(Some(plain))
    }
}

/// The unfinished start of a file that was being received: reads the header and then as many whole chunks as open as chunks
/// that are not the last, and hands their contents to `sink`. Stops at the first one that does not open, or at a piece that
/// is shorter than a chunk (a write the crash cut off). Returns the header and the number of good chunks.
pub fn read_prefix<R: Read>(
    mut input: R,
    file_len: u64,
    keys: &DocKeys,
    id: &DocId,
    mut sink: impl FnMut(&[u8]),
) -> Result<([u8; HEADER_LEN], u64), DocError> {
    if file_len < HEADER_LEN as u64 {
        return Err(DocError::Format);
    }
    let mut header = [0u8; HEADER_LEN];
    input.read_exact(&mut header).map_err(|_| DocError::Format)?;
    let salt = parse_header(&header)?;
    let cipher = cipher_for(keys, id, &salt);
    let whole = ((file_len - HEADER_LEN as u64) / SEALED_CHUNK as u64).min(MAX_DOCUMENT_BYTES / CHUNK_SIZE as u64);
    let mut good = 0;
    let mut sealed = vec![0u8; SEALED_CHUNK];
    while good < whole {
        if input.read_exact(&mut sealed).is_err() {
            break;
        }
        match open_chunk(&cipher, &header, good, false, &sealed) {
            Ok(plain) => sink(&plain),
            Err(_) => break,
        }
        good += 1;
    }
    Ok((header, good))
}

/// Seals a whole file held in memory.
pub fn seal_all(keys: &DocKeys, id: &DocId, bytes: &[u8]) -> Result<Vec<u8>, DocError> {
    let mut sealer = Sealer::new(Vec::with_capacity(sealed_len(bytes.len() as u64) as usize), keys, id, bytes.len() as u64)?;
    sealer.write(bytes)?;
    sealer.finish()
}

/// Opens a whole sealed file held in memory.
pub fn open_all(keys: &DocKeys, id: &DocId, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, DocError> {
    let mut opener = Opener::new(std::io::Cursor::new(sealed), sealed.len() as u64, keys, id)?;
    let mut out = Zeroizing::new(Vec::with_capacity(opener.plaintext_len() as usize));
    while let Some(chunk) = opener.next_chunk()? {
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}
