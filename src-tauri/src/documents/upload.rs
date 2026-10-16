//! A file on its way in (`docs/documents-design.md`, section 7).
//!
//! The page cannot hand Rust a path (it reads the file itself, from a file field), and a request with raw bytes does not work on
//! Android. So the file goes over in pieces of text, each a few hundred kilobytes of base64url, and is put together here, in memory,
//! and nowhere else: a half-sent file is never written to the disk in the clear. There is one upload at a time. Locking the app, or
//! starting another upload, drops what was gathered, and the memory is zeroed as it goes.

use std::sync::Mutex;

use zeroize::Zeroizing;

use crate::sync::qr::base64url_decode;
use crate::vault::AppError;

use super::library::{check_input, NewDocument};
use super::seal::MAX_DOCUMENT_BYTES;

/// The most text one piece may be: 384 KiB of file is 512 KiB of base64url.
pub const MAX_PIECE_TEXT: usize = 512 * 1024;

struct Upload {
    input: NewDocument,
    size: u64,
    data: Zeroizing<Vec<u8>>,
}

#[derive(Default)]
pub struct Uploads(Mutex<Option<Upload>>);

impl Uploads {
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<Upload>> {
        // The slot is only ever swapped whole, so a poisoned lock still holds valid data.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Starts an upload of a file of `size` bytes, dropping any other. What is wrong with the description, or with the size, is
    /// said now and not after 25 MiB have been sent.
    pub fn begin(&self, input: NewDocument, size: u64) -> Result<(), AppError> {
        if size == 0 {
            return Err(AppError::InvalidInput);
        }
        if size > MAX_DOCUMENT_BYTES {
            return Err(AppError::TooLarge);
        }
        check_input(&input)?;
        *self.slot() = Some(Upload { input, size, data: Zeroizing::new(Vec::with_capacity(size as usize)) });
        Ok(())
    }

    /// Adds the next piece, which must start where the last one ended. Returns how many bytes there are now. A piece that is not
    /// valid, or that goes past the announced size, ends the upload.
    pub fn push(&self, offset: u64, text: &str) -> Result<u64, AppError> {
        let mut slot = self.slot();
        let Some(upload) = slot.as_mut() else { return Err(AppError::InvalidInput) };
        let piece = (text.len() <= MAX_PIECE_TEXT && offset == upload.data.len() as u64)
            .then(|| base64url_decode(text))
            .flatten()
            .filter(|piece| upload.data.len() as u64 + piece.len() as u64 <= upload.size);
        match piece {
            Some(piece) => {
                let piece = Zeroizing::new(piece);
                upload.data.extend_from_slice(&piece);
                Ok(upload.data.len() as u64)
            }
            None => {
                *slot = None;
                Err(AppError::InvalidInput)
            }
        }
    }

    /// The whole file and what was said about it, if every byte has come. Ends the upload either way.
    pub fn finish(&self) -> Result<(NewDocument, Zeroizing<Vec<u8>>), AppError> {
        let upload = self.slot().take().ok_or(AppError::InvalidInput)?;
        if upload.data.len() as u64 != upload.size {
            return Err(AppError::InvalidInput);
        }
        Ok((upload.input, upload.data))
    }

    /// Drops what was gathered.
    pub fn clear(&self) {
        *self.slot() = None;
    }

    pub fn is_empty(&self) -> bool {
        self.slot().is_none()
    }
}
