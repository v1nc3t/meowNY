//! Syncing the desktop with the phone (phase 8). The design is in `docs/sync-design.md`.
//!
//! * `stamp`: the stamp every shared row carries and the clock that makes them.
//! * `crypto`: the handshake and the sealed frames.
//! * `qr`: what a QR code holds and how it is read back.
//! * `wire`: the messages and the rows they carry, and the checks on both.
//! * `engine`: digests, the rows to send, and applying what arrives with every merge rule.
//! * `run`: the whole conversation between two databases in memory, for the tests.

pub mod crypto;
pub mod engine;
pub mod qr;
pub mod run;
pub mod stamp;
pub mod wire;

#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod foundation_tests;
