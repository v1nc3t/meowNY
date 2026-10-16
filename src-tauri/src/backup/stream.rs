//! The sealed stream of a version 2 backup: the same kind of chunked encryption as a stored document (`documents/seal.rs`), under the
//! key stretched from the backup's passphrase.
//!
//! The bytes after the header are cut into chunks of 64 KiB (the last one may be shorter) and each is sealed with ChaCha20-Poly1305. The
//! nonce of a chunk holds its number and a flag that says whether it is the last, and the whole header is authenticated with every
//! chunk. So a chunk cannot be changed, moved, dropped, repeated or cut off the end without the opening failing, and a stream cut at a
//! chunk boundary is refused because the chunk before the cut was not sealed as the last.
//!
//! The writer does not know in advance how much it will write (a file that cannot be read is left out), so it holds back one chunk
//! until it knows whether more follows. The reader knows from the length of the file which chunk is the last.

use std::io::{Read, Write};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use zeroize::{Zeroize, Zeroizing};

use crate::vault::AppError;

pub(super) const CHUNK: usize = 64 * 1024;
pub(super) const TAG: usize = 16;
const SEALED_CHUNK: u64 = (CHUNK + TAG) as u64;

/// The first byte says "this is the last chunk", the last eight are its number. A key belongs to one backup (its salt is random) and
/// the numbers do not repeat in a stream, so no nonce is used twice under one key.
fn nonce(index: u64, last: bool) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[0] = last as u8;
    nonce[4..].copy_from_slice(&index.to_be_bytes());
    nonce
}

fn cipher(key: &[u8; 32]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(Key::from_slice(key))
}

/// Seals what is written to it in chunks, one behind.
pub(super) struct ChunkWriter<W: Write> {
    out: W,
    cipher: ChaCha20Poly1305,
    header: Vec<u8>,
    index: u64,
    held: Zeroizing<Vec<u8>>,
}

impl<W: Write> ChunkWriter<W> {
    /// `header` has been written to `out` already; it is what every chunk authenticates.
    pub(super) fn new(out: W, key: &[u8; 32], header: Vec<u8>) -> Self {
        ChunkWriter { out, cipher: cipher(key), header, index: 0, held: Zeroizing::new(Vec::with_capacity(CHUNK)) }
    }

    pub(super) fn write(&mut self, mut data: &[u8]) -> Result<(), AppError> {
        while !data.is_empty() {
            // a full chunk is sealed only when more arrives: it might be the last
            if self.held.len() == CHUNK {
                self.seal_held(false)?;
            }
            let take = (CHUNK - self.held.len()).min(data.len());
            self.held.extend_from_slice(&data[..take]);
            data = &data[take..];
        }
        Ok(())
    }

    fn seal_held(&mut self, last: bool) -> Result<(), AppError> {
        let sealed = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce(self.index, last)), Payload { msg: &self.held, aad: &self.header })
            .map_err(|_| AppError::Internal)?;
        self.out.write_all(&sealed).map_err(|_| AppError::Internal)?;
        self.index += 1;
        self.held.zeroize();
        Ok(())
    }

    /// Seals what is held as the last chunk and gives the output back.
    pub(super) fn finish(mut self) -> Result<W, AppError> {
        self.seal_held(true)?;
        Ok(self.out)
    }
}

/// Opens a stream chunk by chunk and hands out its bytes, never holding more than one chunk.
pub(super) struct ChunkReader<R: Read> {
    input: R,
    cipher: ChaCha20Poly1305,
    header: Vec<u8>,
    /// Sealed bytes of the stream that are not read yet.
    left: u64,
    index: u64,
    chunk: Zeroizing<Vec<u8>>,
    at: usize,
}

impl<R: Read> ChunkReader<R> {
    /// `input` is positioned after the header, and `sealed_len` is how many bytes are left in it.
    pub(super) fn new(input: R, key: &[u8; 32], header: Vec<u8>, sealed_len: u64) -> Self {
        ChunkReader { input, cipher: cipher(key), header, left: sealed_len, index: 0, chunk: Zeroizing::new(Vec::new()), at: 0 }
    }

    /// Opens the next chunk if the one in hand is used up. False when the stream is over.
    fn fill(&mut self) -> Result<bool, AppError> {
        if self.at < self.chunk.len() {
            return Ok(true);
        }
        if self.left == 0 {
            return Ok(false);
        }
        let size = self.left.min(SEALED_CHUNK);
        let last = size == self.left;
        if (size as usize) < TAG {
            return Err(AppError::InvalidBackup);
        }
        let mut sealed = vec![0u8; size as usize];
        self.input.read_exact(&mut sealed).map_err(|e| if e.kind() == std::io::ErrorKind::UnexpectedEof { AppError::InvalidBackup } else { AppError::Internal })?;
        self.left -= size;
        let plain = self
            .cipher
            .decrypt(Nonce::from_slice(&nonce(self.index, last)), Payload { msg: &sealed, aad: &self.header })
            // the first chunk fails to open under a wrong passphrase, and cannot be told from a damaged file; later ones were reached
            // with the right key, so they are damage
            .map_err(|_| if self.index == 0 { AppError::WrongCredentials } else { AppError::InvalidBackup })?;
        // the writer never seals an empty chunk (it seals what it holds only when there is something in it), and one in a file
        // would hand out no bytes while claiming there are more
        if plain.is_empty() || (!last && plain.len() != CHUNK) {
            return Err(AppError::InvalidBackup);
        }
        self.chunk = Zeroizing::new(plain);
        self.at = 0;
        self.index += 1;
        Ok(true)
    }

    /// Fills `out` from the stream, or fails if the stream ends first.
    pub(super) fn take(&mut self, out: &mut [u8]) -> Result<(), AppError> {
        let mut done = 0;
        while done < out.len() {
            if !self.fill()? {
                return Err(AppError::InvalidBackup);
            }
            let n = (self.chunk.len() - self.at).min(out.len() - done);
            out[done..done + n].copy_from_slice(&self.chunk[self.at..self.at + n]);
            self.at += n;
            done += n;
        }
        Ok(())
    }

    /// Reads `len` bytes into memory, growing as they arrive so a length that lies cannot ask for memory the file does not hold.
    pub(super) fn take_vec(&mut self, len: u64) -> Result<Zeroizing<Vec<u8>>, AppError> {
        let mut out = Zeroizing::new(Vec::new());
        let mut left = len;
        while left > 0 {
            let step = left.min(CHUNK as u64) as usize;
            let start = out.len();
            out.resize(start + step, 0);
            self.take(&mut out[start..])?;
            left -= step as u64;
        }
        Ok(out)
    }

    /// Reads and drops `len` bytes.
    pub(super) fn skip(&mut self, len: u64) -> Result<(), AppError> {
        let mut scratch = Zeroizing::new(vec![0u8; CHUNK]);
        let mut left = len;
        while left > 0 {
            let step = left.min(CHUNK as u64) as usize;
            self.take(&mut scratch[..step])?;
            left -= step as u64;
        }
        Ok(())
    }

    /// The stream must end exactly here: nothing is left in the chunk in hand or after it.
    pub(super) fn finish(&mut self) -> Result<(), AppError> {
        if self.at < self.chunk.len() || self.left != 0 {
            return Err(AppError::InvalidBackup);
        }
        Ok(())
    }
}
