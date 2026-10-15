//! Syncing the desktop with the phone (phase 8). The design is in `docs/sync-design.md`.
//!
//! * `stamp`: the stamp every shared row carries and the clock that makes them.

pub mod stamp;

#[cfg(test)]
mod foundation_tests;
