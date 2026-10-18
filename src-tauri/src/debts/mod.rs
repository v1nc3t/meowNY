//! Debts and expense splitting (phase 10, `docs/debts-design.md`).
//!
//! * `splits`: sharing a bill. `allocate` turns weights into amounts that add up to the bill to the cent; the rest writes, rescales and
//!   reads the parts of a transaction and the template of a recurring item. Your own part is never stored: it is the amount minus the
//!   parts of the others, which is what the SQL view `transaction_costs` says and what every sum of spending reads.
//! * `people`: the friends of the ledger. A name and nothing else; archived, never deleted.
//! * `ledger`: loans, borrowings and repayments with one friend, "mark as paid", and the statement of a friend.
//! * `summary`: who owes what, as text or CSV, to save and send yourself.
//! * `codes`: the bill code (a friend's part of one bill as text for a QR code) and taking one in.
//! * `reminders`: which friends are due a reminder.
//! * `dues`: the recurring payments that have come due, kept with the debts.
//! * `history`: everything that changed what you owe or are owed, in one list.

pub mod codes;
pub mod dues;
pub mod history;
pub mod ledger;
pub mod people;
pub mod reminders;
pub mod splits;
pub mod summary;

#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod allocate_tests;
#[cfg(test)]
mod cost_tests;
#[cfg(test)]
mod dues_tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod codes_tests;
#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod currency_tests;
#[cfg(test)]
mod ledger_tests;
#[cfg(test)]
mod people_tests;
#[cfg(test)]
mod reminders_tests;
#[cfg(test)]
mod splits_tests;
#[cfg(test)]
mod summary_tests;
#[cfg(test)]
mod table_tests;
