//! Documents (phase 9): receipts, invoices and contracts kept next to the numbers. The design is in
//! `docs/documents-design.md`.
//!
//! * `seal`: the keys, the names of the files and the sealed file format (STREAM chunks of ChaCha20-Poly1305).
//! * `store`: the folder `documents/` next to the database: atomic writes, reading with a proof, files arriving in pieces that
//!   can be carried on, and the check of the folder against the rows.

pub mod seal;
pub mod store;

pub use seal::{DocError, DocId, DocKeys, MAX_DOCUMENT_BYTES};
pub use store::Store;

#[cfg(test)]
mod seal_tests;
#[cfg(test)]
mod store_tests;
