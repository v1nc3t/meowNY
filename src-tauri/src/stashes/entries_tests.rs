//! The entries of a stash, row by row of the state machine (`docs/stash-design.md`, section 5.2) and the delta rule of section 3.1, for what
//! happens on this device. The items of section 5.4 that are about entries are named after their number. Every refusal is checked to have
//! changed nothing: no entry, no transfer, no stamp.

use super::entries::{add, delete, list, take, update, EntryPatch, Kind, NewEntry, NewTake};
use super::fixtures::*;
use super::{archive, get};
use crate::accounts::transfers;
use crate::accounts;
use crate::debts::fixtures::{one, text};
use rusqlite::Connection;

fn entry_ids(conn: &Connection, stash: i64) -> Vec<i64> {
    list(conn, stash).unwrap().iter().map(|e| e.id).collect()
}

fn amount(text: &str) -> EntryPatch {
    EntryPatch { amount: Some(text.into()), ..Default::default() }
}

fn note(text: &str) -> EntryPatch {
    EntryPatch { note: Some(Some(text.into())), ..Default::default() }
}

fn add_from(conn: &Connection, stash: i64, text: &str, from: i64) -> Result<super::Detail, crate::vault::AppError> {
    add(conn, stash, &NewEntry { from_account_id: Some(from.to_string()), ..new_entry(text) }, TODAY)
}

fn take_to(conn: &Connection, stash: i64, text: &str, to: i64) -> Result<super::Detail, crate::vault::AppError> {
    take(conn, stash, &NewTake { to_account_id: Some(to.to_string()), ..new_take(text) }, TODAY)
}

/// An account with 1000.00 and a stash in it with no target. Returns both.
fn savings(conn: &Connection) -> (i64, i64) {
    let acc = funded(conn, "Savings", "1000.00");
    (acc, stash(conn, "Holiday", acc))
}

// ---- NONE: add ---------------------------------------------------------------------------------

#[test]
fn an_add_makes_an_entry_of_what_was_given_dated_today_when_no_date_is_given_and_answers_the_whole_stash() {
    let w = world();
    let (acc, id) = savings(&w.conn);
    let detail = add(&w.conn, id, &NewEntry { note: Some("  november ".into()), ..new_entry("50.00") }, TODAY).unwrap();
    assert_eq!(detail.stash.saved, "50.00");
    assert_eq!((detail.stash.entries, detail.stash.last_entry.as_deref(), detail.stash.free.as_str()), (1, Some(TODAY), "950.00"));
    let entry = &detail.entries[0];
    assert_eq!(
        (entry.kind, entry.amount.as_str(), entry.date.as_str(), entry.note.as_deref(), entry.transfer_id, entry.other_account_id, entry.other_account.as_deref(), entry.saved_after.as_str(), entry.stash_id),
        (Kind::Add, "50.00", TODAY, Some("november"), None, None, None, "50.00", id)
    );
    assert_eq!(one(&w.conn, "SELECT amount_minor FROM stash_entries"), 5000, "stored as a positive number");
    assert_eq!(text(&w.conn, "SELECT entry_date FROM stash_entries"), TODAY);
    assert_eq!(accounts::get(&w.conn, acc, TODAY).unwrap().balance, "1000.00", "the balance does not move");
    assert_eq!(stamp_of(&w.conn, "stash_entries", entry.id).len(), 27);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'stash_entries' AND action = 'INSERT'"), 1);
    // an empty or blank note is none, line breaks stay, a date given is the date
    let blank = add(&w.conn, id, &NewEntry { note: Some("   ".into()), date: Some("2026-11-01".into()), ..new_entry("1.00") }, TODAY).unwrap();
    assert_eq!(list(&w.conn, id).unwrap().iter().find(|e| e.date == "2026-11-01").unwrap().note, None);
    assert_eq!(blank.stash.entries, 2);
    let lines = add(&w.conn, id, &NewEntry { note: Some("a\r\nb".into()), ..new_entry("1.00") }, TODAY).unwrap();
    assert_eq!(lines.entries[0].note.as_deref(), Some("a\nb"));
}

#[test]
fn an_add_with_a_bad_amount_date_or_note_is_refused_and_writes_nothing() {
    let w = world();
    let (_, id) = savings(&w.conn);
    let bad = [
        ("zero", new_entry("0.00")),
        ("negative", new_entry("-1.00")),
        ("one decimal", new_entry("1.5")),
        ("three decimals", new_entry("1.234")),
        ("no decimals", new_entry("5")),
        ("comma", new_entry("1,00")),
        ("empty", new_entry("")),
        ("plus sign", new_entry("+1.00")),
        ("beyond the largest", new_entry("10000000000.00")),
        ("a date that is not real", NewEntry { date: Some("2026-02-30".into()), ..new_entry("1.00") }),
        ("a date not padded", NewEntry { date: Some("2026-2-3".into()), ..new_entry("1.00") }),
        ("an empty date", NewEntry { date: Some("".into()), ..new_entry("1.00") }),
        ("a long note", NewEntry { note: Some("x".repeat(201)), ..new_entry("1.00") }),
        ("a control character in the note", NewEntry { note: Some("a\tb".into()), ..new_entry("1.00") }),
        ("an account that is not an id", NewEntry { from_account_id: Some("x".into()), ..new_entry("1.00") }),
    ];
    for (what, input) in bad {
        assert_refused(&w.conn, INVALID, what, || add(&w.conn, id, &input, TODAY));
    }
    assert_refused(&w.conn, INVALID, "a today that is not a date", || add(&w.conn, id, &new_entry("1.00"), "today"));
    // the bounds are in: a note of 200 characters, the smallest amount
    assert!(add(&w.conn, id, &NewEntry { note: Some("x".repeat(200)), ..new_entry("0.01") }, TODAY).is_ok());
}

#[test]
fn item_03_add_exactly_what_is_free_one_cent_more_when_free_is_negative_to_an_archived_stash_and_in_an_archived_account() {
    let w = world();
    let (acc, id) = savings(&w.conn);
    assert_refused(&w.conn, INVALID, "one cent more than is free", || add(&w.conn, id, &new_entry("1000.01"), TODAY));
    let all = add(&w.conn, id, &new_entry("1000.00"), TODAY).unwrap();
    assert_eq!((all.stash.saved.as_str(), all.stash.free.as_str()), ("1000.00", "0.00"), "exactly what is free is allowed");
    assert_refused(&w.conn, INVALID, "nothing is free now", || add(&w.conn, id, &new_entry("0.01"), TODAY));

    // free is negative after the money was spent: any add is refused
    spend(&w.conn, w.food, acc, "300.00", "2026-11-10");
    assert_eq!(free_of(&w.conn, acc), -30_000);
    assert_refused(&w.conn, INVALID, "free is negative", || add(&w.conn, id, &new_entry("0.01"), TODAY));

    // an archived stash takes no money
    let (other_acc, other) = {
        let a = funded(&w.conn, "Other", "500.00");
        (a, stash(&w.conn, "Other", a))
    };
    archive(&w.conn, other, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "an archived stash", || add(&w.conn, other, &new_entry("1.00"), TODAY));
    super::restore(&w.conn, other, TODAY).unwrap();
    // nor does a stash in an archived account, even when the money comes with a transfer from another account
    accounts::archive(&w.conn, other_acc, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "an archived account", || add(&w.conn, other, &new_entry("1.00"), TODAY));
    assert_refused(&w.conn, INVALID, "an archived account, with a transfer", || add_from(&w.conn, other, "1.00", 1));
    accounts::restore(&w.conn, other_acc, TODAY).unwrap();
    assert!(add(&w.conn, other, &new_entry("500.00"), TODAY).is_ok());
}

#[test]
fn item_04_add_exactly_what_is_left_to_the_target_one_cent_more_and_to_a_stash_without_a_target() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let aimed_id = aimed(&w.conn, "Laptop", acc, "900.00");
    put(&w.conn, aimed_id, "400.00");
    assert_refused(&w.conn, INVALID, "one cent more than is left", || add(&w.conn, aimed_id, &new_entry("500.01"), TODAY));
    let reached = add(&w.conn, aimed_id, &new_entry("500.00"), TODAY).unwrap();
    assert!(reached.stash.reached && reached.stash.percent == Some(100) && reached.stash.left.as_deref() == Some("0.00"), "{:?}", reached.stash);
    assert_refused(&w.conn, INVALID, "one cent over the target", || add(&w.conn, aimed_id, &new_entry("0.01"), TODAY));
    // without a target, only what is free limits it
    let open = stash(&w.conn, "Rainy day", acc);
    assert!(add(&w.conn, open, &new_entry("3000.00"), TODAY).is_ok());
    assert_eq!(free_of(&w.conn, acc), 110_000, "5000.00 less 900.00 and 3000.00");
    assert_refused(&w.conn, INVALID, "more than is free", || add(&w.conn, open, &new_entry("1100.01"), TODAY));
    // the target and free are both asked: a target that is far away does not lift the free limit
    let far = aimed(&w.conn, "Far", acc, "9999999999.99");
    assert_refused(&w.conn, INVALID, "the target allows it, free does not", || add(&w.conn, far, &new_entry("1100.01"), TODAY));
    assert!(add(&w.conn, far, &new_entry("1100.00"), TODAY).is_ok());
}

#[test]
fn an_add_to_a_stash_that_is_above_its_target_or_below_zero_after_a_sync_follows_the_rule_on_what_it_would_hold() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let over = aimed(&w.conn, "Over", acc, "100.00");
    raw(&w.conn, over, 30_000, "2026-01-01");
    assert_refused(&w.conn, INVALID, "already above its target: adding more is above it", || add(&w.conn, over, &new_entry("0.01"), TODAY));
    let below = aimed(&w.conn, "Below", acc, "100.00");
    raw(&w.conn, below, -10_000, "2026-01-01");
    // it holds -100.00: adding 150.00 brings it to 50.00, under the target and within free (held was 0)
    let up = add(&w.conn, below, &new_entry("150.00"), TODAY).unwrap();
    assert_eq!(up.stash.saved, "50.00");
    assert_refused(&w.conn, INVALID, "but not past the target", || add(&w.conn, below, &new_entry("50.01"), TODAY));
}

// ---- add with a transfer -------------------------------------------------------------------------

#[test]
fn an_add_with_a_transfer_makes_the_transfer_and_the_entry_together_and_the_entry_remembers_it() {
    let w = world();
    let current = funded(&w.conn, "Current", "1000.00");
    let savings = account(&w.conn, "Savings");
    let id = stash(&w.conn, "Holiday", savings);
    let detail = add(&w.conn, id, &NewEntry { note: Some("november".into()), date: Some("2026-11-05".into()), from_account_id: Some(current.to_string()), ..new_entry("100.00") }, TODAY).unwrap();
    let entry = &detail.entries[0];
    assert_eq!((entry.kind, entry.amount.as_str(), entry.other_account_id, entry.other_account.as_deref()), (Kind::Add, "100.00", Some(current), Some("Current")));
    let transfer = transfers::get(&w.conn, entry.transfer_id.unwrap()).unwrap();
    assert_eq!(
        (transfer.from_account_id, transfer.to_account_id, transfer.amount.as_str(), transfer.date.as_str(), transfer.note.as_deref(), transfer.to_amount),
        (current, savings, "100.00", "2026-11-05", Some("november"), None),
        "from the account the money came from into the stash's account, with the entry's amount, date and note"
    );
    assert_eq!((balance(&w.conn, current, TODAY), balance(&w.conn, savings, TODAY)), (90_000, 10_000), "the transfer moved the money between the accounts");
    assert_eq!(detail.stash.free, "0.00", "and it is set aside: the account holds 100.00 and the stash all of it");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 1);
    assert_eq!(stamp_of(&w.conn, "transfers", transfer.id).len(), 27);
}

#[test]
fn item_10_add_with_a_transfer_from_the_own_account_an_archived_one_or_another_currency_is_refused_and_no_transfer_is_made() {
    let w = world();
    let current = funded(&w.conn, "Current", "1000.00");
    let id = stash(&w.conn, "Holiday", current);
    let old = {
        let a = funded(&w.conn, "Old", "500.00");
        accounts::archive(&w.conn, a, TODAY).unwrap();
        a
    };
    let lei = foreign(&w.conn, "Lei", "RON", "9000.00");
    assert_refused(&w.conn, INVALID, "from the stash's own account", || add_from(&w.conn, id, "10.00", current));
    assert_refused(&w.conn, INVALID, "from an archived account", || add_from(&w.conn, id, "10.00", old));
    assert_refused(&w.conn, INVALID, "from an account in another currency", || add_from(&w.conn, id, "10.00", lei));
    assert_refused(&w.conn, INVALID, "from an account that is not there", || add_from(&w.conn, id, "10.00", 99));
    assert_eq!((one(&w.conn, "SELECT count(*) FROM transfers"), one(&w.conn, "SELECT count(*) FROM stash_entries")), (0, 0));

    // free is 0 before the transfer: the transfer makes the room
    let savings = account(&w.conn, "Savings");
    let empty = stash(&w.conn, "Empty", savings);
    assert_eq!(free_of(&w.conn, savings), 0);
    assert_refused(&w.conn, INVALID, "the same without the transfer", || add(&w.conn, empty, &new_entry("10.00"), TODAY));
    add_from(&w.conn, empty, "10.00", current).unwrap();
    assert_eq!((one(&w.conn, "SELECT count(*) FROM transfers"), free_of(&w.conn, savings)), (1, 0));
    // the delta rule is checked after the transfer, and a refusal leaves no transfer
    let more = aimed(&w.conn, "Aimed", savings, "5.00");
    assert_refused(&w.conn, INVALID, "above the target after the transfer", || add_from(&w.conn, more, "5.01", current));
}

#[test]
fn an_add_with_a_transfer_that_breaks_the_target_leaves_neither_the_transfer_nor_the_entry() {
    let w = world();
    let current = funded(&w.conn, "Current", "1000.00");
    let savings = account(&w.conn, "Savings");
    let id = aimed(&w.conn, "Holiday", savings, "50.00");
    assert_refused(&w.conn, INVALID, "above the target", || add_from(&w.conn, id, "50.01", current));
    assert_eq!((one(&w.conn, "SELECT count(*) FROM transfers"), one(&w.conn, "SELECT count(*) FROM stash_entries")), (0, 0));
    assert!(add_from(&w.conn, id, "50.00", current).is_ok());
}

// ---- NONE: take --------------------------------------------------------------------------------

#[test]
fn a_take_makes_an_entry_below_zero_that_counts_as_a_take_and_answers_the_whole_stash() {
    let w = world();
    let (acc, id) = savings(&w.conn);
    put(&w.conn, id, "300.00");
    let detail = take(&w.conn, id, &NewTake { note: Some(" deposit for the flat ".into()), ..new_take("40.00") }, TODAY).unwrap();
    assert_eq!((detail.stash.saved.as_str(), detail.stash.free.as_str()), ("260.00", "740.00"));
    let entry = &detail.entries[0];
    assert_eq!((entry.kind, entry.amount.as_str(), entry.note.as_deref(), entry.saved_after.as_str(), entry.transfer_id), (Kind::Take, "40.00", Some("deposit for the flat"), "260.00", None));
    assert_eq!(one(&w.conn, &format!("SELECT amount_minor FROM stash_entries WHERE id = {}", entry.id)), -4000, "stored below 0");
    assert_eq!(accounts::get(&w.conn, acc, TODAY).unwrap().balance, "1000.00", "the money never left the account");
    for (what, input) in [("zero", new_take("0.00")), ("negative", new_take("-1.00")), ("one decimal", new_take("1.5")), ("future", NewTake { date: Some("2026-11-14".into()), ..new_take("1.00") })] {
        assert_refused(&w.conn, INVALID, what, || take(&w.conn, id, &input, TODAY));
    }
}

#[test]
fn item_05_take_exactly_what_is_saved_one_cent_more_from_a_stash_below_zero_from_an_archived_one_and_in_an_archived_account() {
    let w = world();
    let (acc, id) = savings(&w.conn);
    put(&w.conn, id, "200.00");
    assert_refused(&w.conn, INVALID, "one cent more than is saved", || take(&w.conn, id, &new_take("200.01"), TODAY));
    let half = take(&w.conn, id, &new_take("150.00"), TODAY).unwrap();
    assert_eq!(half.stash.saved, "50.00");
    let all = take(&w.conn, id, &new_take("50.00"), TODAY).unwrap();
    assert_eq!((all.stash.saved.as_str(), all.stash.free.as_str()), ("0.00", "1000.00"), "exactly what is saved is allowed");
    assert_refused(&w.conn, INVALID, "nothing is saved now", || take(&w.conn, id, &new_take("0.01"), TODAY));

    // a stash that went below 0 after a sync: a take is refused (after < 0), an add is not
    let gap = stash(&w.conn, "Gap", acc);
    raw(&w.conn, gap, -10_000, "2026-01-01");
    assert_refused(&w.conn, INVALID, "below zero already", || take(&w.conn, gap, &new_take("1.00"), TODAY));
    assert!(add(&w.conn, gap, &new_entry("1.00"), TODAY).is_ok());

    // an archived stash can be emptied, and so can one in an archived account
    let old = stash(&w.conn, "Old", acc);
    put(&w.conn, old, "100.00");
    archive(&w.conn, old, TODAY).unwrap();
    assert_eq!(take(&w.conn, old, &new_take("60.00"), TODAY).unwrap().stash.saved, "40.00");
    let kept = stash(&w.conn, "Kept", acc);
    put(&w.conn, kept, "100.00");
    accounts::archive(&w.conn, acc, TODAY).unwrap();
    assert_eq!(take(&w.conn, kept, &new_take("100.00"), TODAY).unwrap().stash.saved, "0.00");
    assert_eq!(take(&w.conn, old, &new_take("40.00"), TODAY).unwrap().stash.saved, "0.00", "an archived stash in an archived account too");
}

#[test]
fn a_take_with_a_transfer_makes_the_entry_and_a_transfer_out_of_the_stashs_account() {
    let w = world();
    let savings = funded(&w.conn, "Savings", "1000.00");
    let current = account(&w.conn, "Current");
    let id = stash(&w.conn, "Holiday", savings);
    put(&w.conn, id, "300.00");
    let detail = take(&w.conn, id, &NewTake { note: Some("spent".into()), date: Some("2026-11-01".into()), to_account_id: Some(current.to_string()), ..new_take("100.00") }, TODAY).unwrap();
    let entry = &detail.entries.iter().find(|e| e.kind == Kind::Take).unwrap();
    assert_eq!((entry.other_account_id, entry.other_account.as_deref()), (Some(current), Some("Current")), "the other side of a take is where the money went");
    let transfer = transfers::get(&w.conn, entry.transfer_id.unwrap()).unwrap();
    assert_eq!(
        (transfer.from_account_id, transfer.to_account_id, transfer.amount.as_str(), transfer.date.as_str(), transfer.note.as_deref()),
        (savings, current, "100.00", "2026-11-01", Some("spent"))
    );
    assert_eq!((balance(&w.conn, savings, TODAY), balance(&w.conn, current, TODAY)), (90_000, 10_000));
    assert_eq!((detail.stash.saved.as_str(), detail.stash.free.as_str()), ("200.00", "700.00"), "the stash held 300.00 of 1000.00 and now 200.00 of 900.00");

    assert_refused(&w.conn, INVALID, "to its own account", || take_to(&w.conn, id, "1.00", savings));
    assert_refused(&w.conn, INVALID, "to an account that is not there", || take_to(&w.conn, id, "1.00", 99));
    let old = {
        let a = account(&w.conn, "Old");
        accounts::archive(&w.conn, a, TODAY).unwrap();
        a
    };
    assert_refused(&w.conn, INVALID, "to an archived account", || take_to(&w.conn, id, "1.00", old));
    let lei = foreign(&w.conn, "Lei", "RON", "0.00");
    assert_refused(&w.conn, INVALID, "to an account in another currency", || take_to(&w.conn, id, "1.00", lei));
    assert_refused(&w.conn, INVALID, "more than is saved, with a transfer", || take_to(&w.conn, id, "200.01", current));
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 1, "the refusals made no transfer");
}

#[test]
fn a_take_with_a_transfer_may_leave_an_account_that_is_archived_but_not_reach_one() {
    let w = world();
    let savings = funded(&w.conn, "Savings", "1000.00");
    let current = account(&w.conn, "Current");
    let id = stash(&w.conn, "Holiday", savings);
    put(&w.conn, id, "300.00");
    accounts::archive(&w.conn, savings, TODAY).unwrap();
    let detail = take_to(&w.conn, id, "100.00", current).unwrap();
    assert_eq!(detail.stash.saved, "200.00", "money can still leave an archived account");
    assert_eq!(transfers::get(&w.conn, detail.entries[0].transfer_id.unwrap()).unwrap().from_account_id, savings);
    // an archived stash in it can be emptied with a transfer too
    archive(&w.conn, id, TODAY).unwrap();
    assert_eq!(take_to(&w.conn, id, "200.00", current).unwrap().stash.saved, "0.00");
    // but the money cannot go into an archived account
    let other = stash(&w.conn, "Other", current);
    put(&w.conn, other, "250.00");
    assert_refused(&w.conn, INVALID, "to the archived account", || take_to(&w.conn, other, "1.00", savings));
    // and an add with a transfer from an archived account is refused (item 10)
    assert_refused(&w.conn, INVALID, "from the archived account", || add_from(&w.conn, other, "1.00", savings));
}

// ---- LIVE: edit --------------------------------------------------------------------------------

#[test]
fn an_edit_writes_the_amount_the_date_and_the_note_the_kind_stays_and_the_stamp_moves_only_with_a_change() {
    let w = world();
    let (_, id) = savings(&w.conn);
    let add_id = put(&w.conn, id, "100.00");
    let take_id = pull(&w.conn, id, "30.00");
    // an add edited keeps being an add; the amount is a size
    let up = update(&w.conn, add_id, &amount("120.00"), TODAY).unwrap();
    let edited = up.entries.iter().find(|e| e.id == add_id).unwrap();
    assert_eq!((edited.kind, edited.amount.as_str(), up.stash.saved.as_str()), (Kind::Add, "120.00", "90.00"));
    assert_eq!(one(&w.conn, &format!("SELECT amount_minor FROM stash_entries WHERE id = {add_id}")), 12_000);
    // a take edited keeps being a take
    let more = update(&w.conn, take_id, &amount("50.00"), TODAY).unwrap();
    let edited = more.entries.iter().find(|e| e.id == take_id).unwrap();
    assert_eq!((edited.kind, edited.amount.as_str(), more.stash.saved.as_str()), (Kind::Take, "50.00", "70.00"));
    assert_eq!(one(&w.conn, &format!("SELECT amount_minor FROM stash_entries WHERE id = {take_id}")), -5000);
    // date and note
    let dated = update(&w.conn, add_id, &EntryPatch { date: Some("2026-10-01".into()), note: Some(Some("october".into())), ..Default::default() }, TODAY).unwrap();
    let edited = dated.entries.iter().find(|e| e.id == add_id).unwrap();
    assert_eq!((edited.date.as_str(), edited.note.as_deref()), ("2026-10-01", Some("october")));
    assert_eq!(dated.entries.iter().map(|e| e.id).collect::<Vec<_>>(), [take_id, add_id], "newest first by date, so the take is first now");
    // `null` clears the note, empty means none
    assert_eq!(update(&w.conn, add_id, &EntryPatch { note: Some(None), ..Default::default() }, TODAY).unwrap().entries[1].note, None);
    update(&w.conn, add_id, &note("again"), TODAY).unwrap();
    assert_eq!(update(&w.conn, add_id, &note("   "), TODAY).unwrap().entries[1].note, None);

    // the same values again: nothing is written
    let stamp = stamp_of(&w.conn, "stash_entries", add_id);
    assert_nothing_written(&w.conn, "the same amount", || update(&w.conn, add_id, &amount("120.00"), TODAY));
    assert_nothing_written(&w.conn, "the same date", || update(&w.conn, add_id, &EntryPatch { date: Some("2026-10-01".into()), ..Default::default() }, TODAY));
    assert_nothing_written(&w.conn, "an empty patch", || update(&w.conn, add_id, &EntryPatch::default(), TODAY));
    assert_nothing_written(&w.conn, "no note, again", || update(&w.conn, add_id, &EntryPatch { note: Some(None), ..Default::default() }, TODAY));
    assert_eq!(stamp_of(&w.conn, "stash_entries", add_id), stamp);
    for patch in [amount("121.00"), EntryPatch { date: Some("2026-09-01".into()), ..Default::default() }, note("x")] {
        let before = stamp_of(&w.conn, "stash_entries", add_id);
        update(&w.conn, add_id, &patch, TODAY).unwrap();
        assert!(stamp_of(&w.conn, "stash_entries", add_id) > before, "{patch:?}");
    }
}

#[test]
fn an_edit_with_a_bad_amount_date_or_note_is_refused_and_the_date_may_not_be_after_today() {
    let w = world();
    let (_, id) = savings(&w.conn);
    let entry = put(&w.conn, id, "100.00");
    for (what, patch) in [
        ("zero", amount("0.00")),
        ("negative", amount("-5.00")),
        ("one decimal", amount("5.5")),
        ("beyond the largest", amount("10000000000.00")),
        ("a date that is not real", EntryPatch { date: Some("2026-02-30".into()), ..Default::default() }),
        ("tomorrow", EntryPatch { date: Some("2026-11-14".into()), ..Default::default() }),
        ("a long note", note(&"x".repeat(201))),
        ("a control character", note("a\u{7}")),
    ] {
        assert_refused(&w.conn, INVALID, what, || update(&w.conn, entry, &patch, TODAY));
    }
    assert!(update(&w.conn, entry, &EntryPatch { date: Some(TODAY.into()), ..Default::default() }, TODAY).is_ok());
    assert!(update(&w.conn, entry, &EntryPatch { date: Some("1999-01-01".into()), ..Default::default() }, TODAY).is_ok(), "long ago is fine");
}

#[test]
fn item_06_edit_an_add_down_below_what_was_taken_a_take_up_beyond_saved_an_add_up_beyond_free_or_target_and_only_the_note_of_a_stash_in_excess() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = aimed(&w.conn, "Holiday", acc, "500.00");
    let first = put(&w.conn, id, "300.00");
    let taken = pull(&w.conn, id, "200.00");
    // saved is 100.00 (300.00 in, 200.00 out): the add may not go below the 200.00 that was taken
    assert_refused(&w.conn, INVALID, "an add down below what was taken since", || update(&w.conn, first, &amount("199.99"), TODAY));
    assert_eq!(update(&w.conn, first, &amount("200.00"), TODAY).unwrap().stash.saved, "0.00", "down to exactly what was taken: allowed");
    update(&w.conn, first, &amount("300.00"), TODAY).unwrap();
    // a take edited up (more out) beyond what is saved
    assert_refused(&w.conn, INVALID, "a take up beyond saved", || update(&w.conn, taken, &amount("300.01"), TODAY));
    assert_eq!(update(&w.conn, taken, &amount("300.00"), TODAY).unwrap().stash.saved, "0.00", "up to exactly saved: allowed");
    update(&w.conn, taken, &amount("200.00"), TODAY).unwrap();
    // an add edited up: free is 900.00 and 400.00 is left to the target; each limits it
    assert_refused(&w.conn, INVALID, "an add up beyond the target", || update(&w.conn, first, &amount("700.01"), TODAY));
    assert_eq!(update(&w.conn, first, &amount("700.00"), TODAY).unwrap().stash.saved, "500.00", "to exactly the target: allowed");
    let open = stash(&w.conn, "Open", acc);
    let second = put(&w.conn, open, "100.00");
    // free is now 400.00 (1000 - 500 - 100)... and edited up by exactly that is allowed, one cent more is not
    assert_eq!(free_of(&w.conn, acc), 40_000);
    assert_refused(&w.conn, INVALID, "an add up beyond free", || update(&w.conn, second, &amount("500.01"), TODAY));
    assert_eq!(update(&w.conn, second, &amount("500.00"), TODAY).unwrap().stash.saved, "500.00");

    // a stash that holds more than its target after a sync: only the note (after == before) is allowed, and a lower amount
    let over = aimed(&w.conn, "Over", acc, "100.00");
    let big = raw(&w.conn, over, 30_000, "2026-01-01");
    assert_eq!(update(&w.conn, big, &note("it is over"), TODAY).unwrap().entries[0].note.as_deref(), Some("it is over"));
    assert_eq!(update(&w.conn, big, &EntryPatch { date: Some("2026-02-02".into()), ..Default::default() }, TODAY).unwrap().entries[0].date, "2026-02-02", "the date alone too");
    assert_refused(&w.conn, INVALID, "up from there", || update(&w.conn, big, &amount("300.01"), TODAY));
    assert_eq!(update(&w.conn, big, &amount("250.00"), TODAY).unwrap().stash.saved, "250.00", "money coming out is only asked to stay at 0 or more");
    // a stash below 0 after a sync: the same (in an account with money free, so that a take edited down can fit)
    let rich = funded(&w.conn, "Rich", "1000.00");
    let gap = stash(&w.conn, "Gap", rich);
    let hole = raw(&w.conn, gap, -10_000, "2026-01-01");
    assert_eq!(update(&w.conn, hole, &note("negative"), TODAY).unwrap().stash.saved, "-100.00");
    assert_refused(&w.conn, INVALID, "a take up from below zero", || update(&w.conn, hole, &amount("100.01"), TODAY));
    assert_eq!(update(&w.conn, hole, &amount("50.00"), TODAY).unwrap().stash.saved, "-50.00", "a take edited down is money going in: it fits in free and under no target");
}

#[test]
fn an_edit_that_adds_money_is_refused_for_an_archived_stash_and_an_archived_account_and_one_that_takes_it_out_is_not() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = stash(&w.conn, "Holiday", acc);
    let first = put(&w.conn, id, "300.00");
    let out = pull(&w.conn, id, "100.00");
    archive(&w.conn, id, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "an add edited up in an archived stash", || update(&w.conn, first, &amount("301.00"), TODAY));
    assert_refused(&w.conn, INVALID, "a take edited down in an archived stash: money goes in", || update(&w.conn, out, &amount("99.00"), TODAY));
    assert_eq!(update(&w.conn, first, &amount("250.00"), TODAY).unwrap().stash.saved, "150.00", "an add edited down: money comes out");
    assert_eq!(update(&w.conn, out, &amount("120.00"), TODAY).unwrap().stash.saved, "130.00", "a take edited up: money comes out");
    assert!(update(&w.conn, first, &note("kept"), TODAY).is_ok(), "the note alone, nothing changes");
    assert!(update(&w.conn, first, &EntryPatch { date: Some("2026-01-02".into()), ..Default::default() }, TODAY).is_ok());
    super::restore(&w.conn, id, TODAY).unwrap();
    accounts::archive(&w.conn, acc, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "an add edited up in an archived account", || update(&w.conn, first, &amount("251.00"), TODAY));
    assert!(update(&w.conn, first, &amount("240.00"), TODAY).is_ok());
}

#[test]
fn editing_an_entry_does_not_change_the_transfer_that_came_with_it() {
    let w = world();
    let current = funded(&w.conn, "Current", "1000.00");
    let savings = account(&w.conn, "Savings");
    let id = stash(&w.conn, "Holiday", savings);
    let entry = add_from(&w.conn, id, "100.00", current).unwrap().entries[0].clone();
    let transfer = entry.transfer_id.unwrap();
    let stamp = stamp_of(&w.conn, "transfers", transfer);
    let detail = update(&w.conn, entry.id, &EntryPatch { amount: Some("60.00".into()), note: Some(Some("less".into())), date: Some("2026-10-10".into()) }, TODAY).unwrap();
    assert_eq!((detail.entries[0].amount.as_str(), detail.entries[0].transfer_id), ("60.00", Some(transfer)), "the entry keeps the link");
    let kept = transfers::get(&w.conn, transfer).unwrap();
    assert_eq!((kept.amount.as_str(), kept.date.as_str(), kept.note), ("100.00", TODAY, None), "the transfer is its own row and is not changed");
    assert_eq!(stamp_of(&w.conn, "transfers", transfer), stamp);
}

// ---- LIVE: delete ------------------------------------------------------------------------------

#[test]
fn item_07_delete_an_add_whose_money_was_taken_out_since_and_a_take_only_within_free_and_target() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = aimed(&w.conn, "Holiday", acc, "500.00");
    let first = put(&w.conn, id, "300.00");
    let second = put(&w.conn, id, "100.00");
    let taken = pull(&w.conn, id, "250.00");
    // saved is 150.00
    assert_refused(&w.conn, INVALID, "delete an add whose money was taken out since (300 out of 150 is below 0)", || delete(&w.conn, first, false, TODAY));
    let gone = delete(&w.conn, second, false, TODAY).unwrap();
    assert_eq!((gone.stash.saved.as_str(), gone.stash.entries), ("50.00", 2), "an add that leaves 0 or more is allowed");
    // delete the take: 250.00 back in, to 300.00, within target 500.00 and free
    assert_eq!(delete(&w.conn, taken, false, TODAY).unwrap().stash.saved, "300.00");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM stash_entries"), 1);

    // a take is deleted only within the target ...
    let aimed_id = aimed(&w.conn, "Laptop", acc, "100.00");
    let big = put(&w.conn, aimed_id, "100.00");
    let out = pull(&w.conn, aimed_id, "60.00");
    put(&w.conn, aimed_id, "60.00");
    assert_refused(&w.conn, INVALID, "deleting the take would leave 160.00 of 100.00", || delete(&w.conn, out, false, TODAY));
    // ... and within free
    let open = stash(&w.conn, "Open", acc);
    put(&w.conn, open, "100.00");
    let out_open = pull(&w.conn, open, "100.00");
    put(&w.conn, open, "540.00");
    assert_eq!(free_of(&w.conn, acc), 6000, "1000.00 less 300.00, 100.00 and 540.00");
    spend(&w.conn, w.food, acc, "50.00", "2026-11-10");
    assert_eq!(free_of(&w.conn, acc), 1000);
    assert_refused(&w.conn, INVALID, "deleting a take puts 100.00 back and only 10.00 is free", || delete(&w.conn, out_open, false, TODAY));
    assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM stash_entries WHERE id = {big}")), 1);
    assert_refused(&w.conn, NOT_FOUND, "an entry that is not there", || delete(&w.conn, 999, false, TODAY));
}

#[test]
fn deleting_an_entry_leaves_a_marker_and_with_delete_transfer_the_transfer_goes_in_the_same_transaction() {
    let w = world();
    let current = funded(&w.conn, "Current", "1000.00");
    let savings = funded(&w.conn, "Savings", "10.00");
    let id = stash(&w.conn, "Holiday", savings);
    let plain = put_on(&w.conn, id, "5.00", "2026-11-01");
    let linked = add_from(&w.conn, id, "100.00", current).unwrap().entries[0].clone();
    let kept = add_from(&w.conn, id, "50.00", current).unwrap().entries[0].clone();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 2);

    // without `delete_transfer` the transfer stays
    let sync_id = text(&w.conn, &format!("SELECT sync_id FROM stash_entries WHERE id = {}", kept.id));
    let detail = delete(&w.conn, kept.id, false, TODAY).unwrap();
    assert_eq!(detail.stash.saved, "105.00");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 2, "the transfer stays");
    assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM sync_deleted WHERE kind = 'stash_entries' AND sync_id = '{sync_id}'")), 1, "the entry leaves a marker");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'transfers'"), 0);
    // with it, the transfer goes too and has its own marker
    let transfer_sync_id = text(&w.conn, &format!("SELECT sync_id FROM transfers WHERE id = {}", linked.transfer_id.unwrap()));
    let detail = delete(&w.conn, linked.id, true, TODAY).unwrap();
    assert_eq!(detail.stash.saved, "5.00");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 1);
    assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM sync_deleted WHERE kind = 'transfers' AND sync_id = '{transfer_sync_id}'")), 1, "its own marker");
    assert_eq!(balance(&w.conn, current, TODAY), 100_000 - 5000, "only the transfer that stayed moved money");
    // with it and no link: nothing else goes
    let before = one(&w.conn, "SELECT count(*) FROM transfers");
    delete(&w.conn, plain, true, TODAY).unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), before);
    assert_eq!(get(&w.conn, id, TODAY).unwrap().entries, []);
}

#[test]
fn a_refused_delete_with_delete_transfer_keeps_the_transfer_and_the_entry() {
    let w = world();
    let current = funded(&w.conn, "Current", "1000.00");
    let savings = account(&w.conn, "Savings");
    let id = stash(&w.conn, "Holiday", savings);
    let linked = add_from(&w.conn, id, "100.00", current).unwrap().entries[0].clone();
    pull(&w.conn, id, "100.00");
    assert_refused(&w.conn, INVALID, "the money was taken out since", || delete(&w.conn, linked.id, true, TODAY));
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), 1);
}

#[test]
fn a_delete_of_a_take_in_an_archived_stash_or_account_is_allowed_within_the_delta_rule() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = stash(&w.conn, "Holiday", acc);
    put(&w.conn, id, "300.00");
    let out = pull(&w.conn, id, "100.00");
    let out2 = pull(&w.conn, id, "50.00");
    archive(&w.conn, id, TODAY).unwrap();
    assert_eq!(delete(&w.conn, out, false, TODAY).unwrap().stash.saved, "250.00", "section 5.1: entries of an archived stash can be deleted, by the delta rule");
    accounts::archive(&w.conn, acc, TODAY).unwrap();
    assert_eq!(delete(&w.conn, out2, false, TODAY).unwrap().stash.saved, "300.00");
}

#[test]
fn any_edit_or_delete_of_an_entry_that_is_not_there_answers_not_found() {
    let w = world();
    let (_, id) = savings(&w.conn);
    let entry = put(&w.conn, id, "1.00");
    delete(&w.conn, entry, false, TODAY).unwrap();
    for missing in [entry, 999] {
        assert_refused(&w.conn, NOT_FOUND, "edit", || update(&w.conn, missing, &amount("2.00"), TODAY));
        assert_refused(&w.conn, NOT_FOUND, "delete", || delete(&w.conn, missing, false, TODAY));
        assert_refused(&w.conn, NOT_FOUND, "delete with the transfer", || delete(&w.conn, missing, true, TODAY));
    }
}

// ---- item 11, 13, 14, 15 ---------------------------------------------------------------------

#[test]
fn item_11_delete_a_linked_transfer_from_the_accounts_side_and_the_entry_stays_with_no_link_and_one_stamp() {
    let w = world();
    let current = funded(&w.conn, "Current", "1000.00");
    let savings = account(&w.conn, "Savings");
    let id = stash(&w.conn, "Holiday", savings);
    let entry = add_from(&w.conn, id, "100.00", current).unwrap().entries[0].clone();
    let other = add_from(&w.conn, id, "20.00", current).unwrap().entries[0].clone();
    let before = (stamp_of(&w.conn, "stash_entries", entry.id), one(&w.conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'stash_entries' AND action = 'UPDATE'"));
    let other_stamp = stamp_of(&w.conn, "stash_entries", other.id);

    transfers::delete(&w.conn, entry.transfer_id.unwrap()).unwrap();

    let detail = get(&w.conn, id, TODAY).unwrap();
    let kept = detail.entries.iter().find(|e| e.id == entry.id).unwrap();
    assert_eq!((kept.transfer_id, kept.other_account_id, kept.other_account.as_deref(), kept.amount.as_str()), (None, None, None, "100.00"), "the entry stays, with no link");
    assert_eq!(detail.stash.saved, "120.00", "and what it held stays set aside");
    assert!(stamp_of(&w.conn, "stash_entries", entry.id) > before.0, "one new stamp");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'stash_entries' AND action = 'UPDATE'"), before.1 + 1, "one row of history");
    assert_eq!(stamp_of(&w.conn, "stash_entries", other.id), other_stamp, "the other entry keeps its link and its stamp");
    assert_eq!(detail.entries.iter().find(|e| e.id == other.id).unwrap().other_account.as_deref(), Some("Current"));
}

#[test]
fn item_13_two_stashes_in_one_account_the_second_cannot_take_what_the_first_holds() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let (x, y) = (stash(&w.conn, "X", acc), stash(&w.conn, "Y", acc));
    put(&w.conn, x, "600.00");
    assert_eq!(free_of(&w.conn, acc), 40_000);
    assert_refused(&w.conn, INVALID, "Y adds more than is left after X", || add(&w.conn, y, &new_entry("400.01"), TODAY));
    put(&w.conn, y, "400.00");
    assert_eq!(free_of(&w.conn, acc), 0);
    // Y cannot take what X holds, although the account holds it
    assert_refused(&w.conn, INVALID, "Y takes more than Y holds", || take(&w.conn, y, &new_take("400.01"), TODAY));
    assert_eq!(take(&w.conn, y, &new_take("400.00"), TODAY).unwrap().stash.saved, "0.00");
    assert_eq!((shown(&w.conn, x).saved.as_str(), shown(&w.conn, x).free.as_str()), ("600.00", "400.00"), "X is where it was, and both see one free");
    assert_eq!(shown(&w.conn, y).free, "400.00");
    // an archived stash holds its money as well
    archive(&w.conn, x, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "X is archived and still holds 600.00", || add(&w.conn, y, &new_entry("400.01"), TODAY));
}

#[test]
fn item_14_an_entry_dated_tomorrow_is_refused_and_one_dated_long_ago_is_allowed_and_counts_in_saved() {
    let w = world();
    let (acc, id) = savings(&w.conn);
    assert_refused(&w.conn, INVALID, "tomorrow", || add(&w.conn, id, &NewEntry { date: Some("2026-11-14".into()), ..new_entry("1.00") }, TODAY));
    assert_refused(&w.conn, INVALID, "far ahead", || add(&w.conn, id, &NewEntry { date: Some("2099-01-01".into()), ..new_entry("1.00") }, TODAY));
    assert_refused(&w.conn, INVALID, "a take dated tomorrow", || take(&w.conn, id, &NewTake { date: Some("2026-11-14".into()), ..new_take("1.00") }, TODAY));
    assert_eq!(add(&w.conn, id, &NewEntry { date: Some(TODAY.into()), ..new_entry("5.00") }, TODAY).unwrap().stash.saved, "5.00", "today is allowed");
    let old = add(&w.conn, id, &NewEntry { date: Some("2000-01-01".into()), ..new_entry("7.00") }, TODAY).unwrap();
    assert_eq!(old.stash.saved, "12.00", "long ago counts in saved");
    assert_eq!(old.stash.last_entry.as_deref(), Some(TODAY), "the last entry is the latest date");
    // before the account opened: allowed too, the balance and the entries are separate things
    assert_eq!(add(&w.conn, id, &NewEntry { date: Some("1990-01-01".into()), ..new_entry("1.00") }, TODAY).unwrap().stash.saved, "13.00");
    assert_eq!(accounts::get(&w.conn, acc, TODAY).unwrap().free, "987.00");
}

#[test]
fn item_15_spend_after_setting_aside_free_is_negative_and_only_adds_are_refused() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = aimed(&w.conn, "Holiday", acc, "2000.00");
    put(&w.conn, id, "800.00");
    let out = pull(&w.conn, id, "100.00");
    let first = put(&w.conn, id, "100.00");
    spend(&w.conn, w.food, acc, "500.00", "2026-11-10");
    let account = accounts::get(&w.conn, acc, TODAY).unwrap();
    assert_eq!((account.balance.as_str(), account.set_aside.as_str(), account.free.as_str()), ("500.00", "800.00", "-300.00"), "the balance fell below what is set aside");
    assert_eq!(shown(&w.conn, id).free, "-300.00");
    // adds are refused, whatever the size
    assert_refused(&w.conn, INVALID, "an add", || add(&w.conn, id, &new_entry("0.01"), TODAY));
    assert_refused(&w.conn, INVALID, "an edit up", || update(&w.conn, first, &amount("100.01"), TODAY));
    assert_refused(&w.conn, INVALID, "the delete of a take", || delete(&w.conn, out, false, TODAY));
    // everything else is allowed
    assert!(update(&w.conn, first, &note("still fine"), TODAY).is_ok());
    assert!(update(&w.conn, first, &amount("50.00"), TODAY).is_ok());
    assert!(take(&w.conn, id, &new_take("100.00"), TODAY).is_ok());
    assert!(delete(&w.conn, first, false, TODAY).is_ok());
    assert!(super::update(&w.conn, id, &super::StashPatch { name: Some("Trip".into()), ..Default::default() }, TODAY).is_ok());
    assert!(archive(&w.conn, id, TODAY).is_ok());
    assert!(take(&w.conn, id, &new_take("0.01"), TODAY).is_ok(), "an archived stash is emptied while the account is over-booked");
}

// ---- the numbers of an entry ---------------------------------------------------------------------

#[test]
fn saved_after_runs_oldest_first_by_date_and_id_and_the_entries_come_newest_first() {
    let w = world();
    let (_, id) = savings(&w.conn);
    let march = put_on(&w.conn, id, "100.00", "2026-03-01");
    let january = put_on(&w.conn, id, "50.00", "2026-01-01");
    let march_two = put_on(&w.conn, id, "10.00", "2026-03-01");
    let out = pull(&w.conn, id, "30.00");
    let all = list(&w.conn, id).unwrap();
    assert_eq!(all.iter().map(|e| e.id).collect::<Vec<_>>(), [out, march_two, march, january], "newest first: by date, then by id");
    assert_eq!(all.iter().map(|e| e.saved_after.as_str()).collect::<Vec<_>>(), ["130.00", "160.00", "150.00", "50.00"], "oldest first: 50, 150, 160, 130");
    assert_eq!(all.iter().map(|e| e.kind).collect::<Vec<_>>(), [Kind::Take, Kind::Add, Kind::Add, Kind::Add]);
    assert_eq!(all.iter().map(|e| e.amount.as_str()).collect::<Vec<_>>(), ["30.00", "10.00", "100.00", "50.00"], "always above 0");
    assert_eq!(entry_ids(&w.conn, id), [out, march_two, march, january]);
    // the entries of another stash are not in it
    let other = stash(&w.conn, "Other", 1);
    assert_eq!(list(&w.conn, other).unwrap(), []);
    assert_eq!(get(&w.conn, id, TODAY).unwrap().entries, all);
}
