//! Documents (phase 9): receipts, invoices and contracts kept next to the numbers. The design is in
//! `docs/documents-design.md`.
//!
//! * `seal`: the keys, the names of the files and the sealed file format (STREAM chunks of ChaCha20-Poly1305).
//! * `store`: the folder `documents/` next to the database: atomic writes, reading with a proof, files arriving in pieces that
//!   can be carried on, and the check of the folder against the rows.
//! * `names`: how a document is named and what kind of file it is, decided from the name and the first bytes.
//! * `library`: the documents and their links in the database, together with their files.

pub mod library;
pub mod names;
pub mod seal;
pub mod store;

pub use seal::{DocError, DocId, DocKeys, MAX_DOCUMENT_BYTES};
pub use store::Store;

use crate::vault::AppError;

impl From<DocError> for AppError {
    fn from(e: DocError) -> Self {
        match e {
            DocError::TooLarge => AppError::TooLarge,
            DocError::Missing => AppError::FileMissing,
            DocError::Damaged | DocError::Mismatch | DocError::Format => AppError::FileDamaged,
            DocError::BadId | DocError::Incomplete | DocError::Io(_) | DocError::Random => AppError::Internal,
        }
    }
}

#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod names_tests;
#[cfg(test)]
mod seal_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod table_tests;
