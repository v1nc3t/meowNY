//! Stashes between two devices: what `docs/stash-design.md` (section 7, and the sync cases of 5.4 and 7.6) asks of the sync, on two in-memory
//! databases. After every two-device scenario the two are equal (`settle`), a second sync finds nothing to send, and every reference holds.

use rusqlite::Connection;

use super::accounts_tests::{account_id, account_sync_id, after, conflicts_between, fields, later_of, marker_of, only, refused_with_a_broken_reference, settle, texts, transfer_named};
use super::engine::{self, snapshot, Incoming, Summary, SyncError};
use super::engine_tests::{assert_same, category, count, device, edit_accounts, edit_stashes, first_seed, id_of, new_phone, now, pause, rows_to, seeds, sync_ok, text, Rng, TODAY};
use super::run::{sync, sync_cut_after};
use super::stamp::Stamp;
use super::wire::{Marker, Table};
use crate::accounts::transfers;
use crate::accounts::{self, AccountPatch, Kind, NewAccount, MAIN_ID};
use crate::categories::CategoryType;
use crate::stashes::entries::{self, EntryPatch, NewEntry, NewTake};
use crate::stashes::{self, NewStash, StashPatch};
use crate::vault::AppError;

// --- a world to work in ------------------------------------------------------------------------------------------------

/// An account with an opening balance, dated before every day these tests use, and nothing else in it. Returns its local id.
fn funded(conn: &Connection, name: &str, opening: &str) -> i64 {
    accounts::create(
        conn,
        &NewAccount { name: name.into(), kind: Kind::Checking, number: None, opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), bic: None, notes: None, group: None, include_in_net_worth: None, virtual_balance: None, currency: None, rate: None, interest_rate: None, interest_period: None, installment: None },
        TODAY,
    )
    .unwrap()
    .id
}

/// The same in a currency of its own, at 5.00 to the euro.
fn funded_in(conn: &Connection, name: &str, code: &str, opening: &str) -> i64 {
    accounts::create(
        conn,
        &NewAccount { name: name.into(), kind: Kind::Checking, number: None, opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), bic: None, notes: None, group: None, include_in_net_worth: None, virtual_balance: None, currency: Some(code.into()), rate: Some("5.00".into()), interest_rate: None, interest_period: None, installment: None },
        TODAY,
    )
    .unwrap()
    .id
}

/// A stash, with a target like "500.00" or none. Returns its local id.
fn make(conn: &Connection, name: &str, account: i64, target: Option<&str>) -> i64 {
    stashes::create(conn, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: name.into(), account_id: account.to_string(), target: target.map(str::to_string), target_date: None, notes: None }, TODAY).unwrap().id
}

/// An add; `note` is its name. Returns its local id.
fn put(conn: &Connection, stash: i64, amount: &str, note: &str) -> i64 {
    entries::add(conn, stash, &NewEntry { amount: amount.into(), date: Some("2026-03-02".into()), note: Some(note.into()), from_account_id: None }, TODAY).unwrap();
    entry_id(conn, note)
}

/// An add with a transfer from another account, which has the same note.
fn put_from(conn: &Connection, stash: i64, amount: &str, note: &str, from: i64) -> i64 {
    entries::add(conn, stash, &NewEntry { amount: amount.into(), date: Some("2026-03-02".into()), note: Some(note.into()), from_account_id: Some(from.to_string()) }, TODAY).unwrap();
    entry_id(conn, note)
}

/// A take; `note` is its name.
fn pull(conn: &Connection, stash: i64, amount: &str, note: &str) -> i64 {
    entries::take(conn, stash, &NewTake { amount: amount.into(), date: Some("2026-03-02".into()), note: Some(note.into()), to_account_id: None }, TODAY).unwrap();
    entry_id(conn, note)
}

fn stash_id(conn: &Connection, name: &str) -> i64 {
    id_of(conn, "stashes", name)
}

fn stash_sync_id(conn: &Connection, name: &str) -> String {
    text(conn, &format!("SELECT sync_id FROM stashes WHERE name = '{name}'"))
}

fn entry_id(conn: &Connection, note: &str) -> i64 {
    conn.query_row("SELECT id FROM stash_entries WHERE note = ?1", [note], |r| r.get(0)).unwrap()
}

fn entry_sync_id(conn: &Connection, note: &str) -> String {
    conn.query_row("SELECT sync_id FROM stash_entries WHERE note = ?1", [note], |r| r.get(0)).unwrap()
}

/// What an entry is on this device: its amount, the sync id of its transfer, its stamp and its `updated_at`.
fn entry_state(conn: &Connection, note: &str) -> (i64, Option<String>, String, String) {
    conn.query_row(
        "SELECT e.amount_minor, t.sync_id, e.stamp, e.updated_at FROM stash_entries e LEFT JOIN transfers t ON t.id = e.transfer_id WHERE e.note = ?1",
        [note],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .unwrap()
}

fn stash_stamp(conn: &Connection, name: &str) -> (String, String) {
    conn.query_row("SELECT stamp, updated_at FROM stashes WHERE name = ?1", [name], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
}

fn saved(conn: &Connection, name: &str) -> String {
    stashes::get(conn, stash_id(conn, name), TODAY).unwrap().stash.saved
}

fn names(conn: &Connection) -> Vec<String> {
    let mut names: Vec<String> = conn.prepare("SELECT name FROM stashes").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    names.sort();
    names
}

fn notes_of_entries(conn: &Connection) -> Vec<String> {
    let mut notes: Vec<String> = conn.prepare("SELECT COALESCE(note, '') FROM stash_entries").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    notes.sort();
    notes
}

/// The stashes a device shows, sorted: name, account, saved, target, archived, currency.
fn shown(conn: &Connection) -> Vec<(String, String, String, Option<String>, bool, String)> {
    let mut list: Vec<_> = stashes::list(conn, None, TODAY).unwrap().into_iter().map(|s| (s.name, s.account, s.saved, s.target, s.archived, s.currency)).collect();
    list.sort();
    list
}

fn stamps_of(conn: &Connection, table: &str) -> String {
    text(conn, &format!("SELECT COALESCE(group_concat(sync_id || ':' || stamp || ':' || updated_at, ','), '') FROM (SELECT sync_id, stamp, updated_at FROM {table} ORDER BY sync_id)"))
}

/// `settle`, started from `a` when `first_is_a` and from `b` when not; the summaries come back as (a's, b's).
fn settle_from(first_is_a: bool, a: &mut Connection, b: &mut Connection) -> (Summary, Summary) {
    if first_is_a {
        let outcome = settle(a, b);
        (outcome.first, outcome.second)
    } else {
        let outcome = settle(b, a);
        (outcome.second, outcome.first)
    }
}

fn newer_than_everything() -> String {
    Stamp::new(now() + 5_000, 0, "abcdef01").format()
}

/// A desktop and a phone that have met, with the accounts Cash (1000.00) and Savings (500.00) on both, and a moment after it.
fn met_with_accounts() -> (Connection, Connection) {
    let mut desktop = device("EUR");
    let mut phone = new_phone(&desktop);
    funded(&desktop, "Cash", "1000.00");
    funded(&desktop, "Savings", "500.00");
    settle(&mut desktop, &mut phone);
    pause();
    (desktop, phone)
}

/// ... and the stash Holiday in Cash, with a target of 500.00, a date and notes, holding 100.00 (`first`) and 50.00 (`second`).
fn met_with_a_stash() -> (Connection, Connection) {
    let (mut desktop, mut phone) = met_with_accounts();
    let cash = account_id(&desktop, "Cash");
    stashes::create(
        &desktop,
        &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: "Holiday".into(), account_id: cash.to_string(), target: Some("500.00".into()), target_date: Some("2027-06-30".into()), notes: Some("Lisbon".into()) },
        TODAY,
    )
    .unwrap();
    let holiday = stash_id(&desktop, "Holiday");
    put(&desktop, holiday, "100.00", "first");
    put(&desktop, holiday, "50.00", "second");
    settle(&mut desktop, &mut phone);
    pause();
    (desktop, phone)
}

/// ... and an entry of Holiday that came with a transfer from Savings: 20.00, named `brought` (the transfer has the same note).
fn met_with_a_linked_entry() -> (Connection, Connection) {
    let (mut desktop, mut phone) = met_with_a_stash();
    let (holiday, savings) = (stash_id(&desktop, "Holiday"), account_id(&desktop, "Savings"));
    put_from(&desktop, holiday, "20.00", "brought", savings);
    settle(&mut desktop, &mut phone);
    pause();
    (desktop, phone)
}

/// ... and the account Wallet (100.00), on both.
fn met_with_a_wallet() -> (Connection, Connection) {
    let (mut desktop, mut phone) = met_with_accounts();
    funded(&desktop, "Wallet", "100.00");
    settle(&mut desktop, &mut phone);
    pause();
    (desktop, phone)
}

// --- the first copy ------------------------------------------------------------------------------------------------------

#[test]
fn a_device_with_stashes_and_their_entries_gives_a_new_phone_a_full_copy() {
    let mut desktop = device("EUR");
    let mut phone = new_phone(&desktop);
    let cash = funded(&desktop, "Cash", "1000.00");
    let savings = funded(&desktop, "Savings", "200.00");
    stashes::create(&desktop, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: "Holiday".into(), account_id: cash.to_string(), target: Some("500.00".into()), target_date: Some("2027-06-30".into()), notes: Some("Lisbon\nin June".into()) }, TODAY).unwrap();
    let holiday = stash_id(&desktop, "Holiday");
    let laptop = make(&desktop, "Laptop", savings, None);
    let old = make(&desktop, "Old car", cash, Some("9999999999.99"));
    put(&desktop, holiday, "100.00", "first");
    put_from(&desktop, holiday, "50.00", "brought", savings);
    pull(&desktop, holiday, "30.00", "spent");
    // an entry with no note and a stash that was archived
    entries::add(&desktop, laptop, &NewEntry { amount: "20.00".into(), date: Some("2026-03-03".into()), note: None, from_account_id: None }, TODAY).unwrap();
    put(&desktop, old, "10.00", "kept");
    stashes::archive(&desktop, old, TODAY).unwrap();

    let outcome = settle(&mut desktop, &mut phone);
    assert_eq!((outcome.first.is_empty(), outcome.second.added), (true, 11), "two accounts, a transfer, three stashes and five entries: {outcome:?}");
    for conn in [&desktop, &phone] {
        assert_eq!(count(conn, "SELECT count(*) FROM stashes"), 3);
        assert_eq!(count(conn, "SELECT count(*) FROM stash_entries"), 5);
        assert_eq!(text(conn, "SELECT t.note FROM stash_entries e JOIN transfers t ON t.id = e.transfer_id WHERE e.note = 'brought'"), "brought", "the link to the transfer arrives");
        assert_eq!(count(conn, "SELECT count(*) FROM stash_entries WHERE transfer_id IS NOT NULL"), 1);
        assert_eq!(count(conn, "SELECT count(*) FROM stashes WHERE archived = 1 AND name = 'Old car'"), 1);
        assert_eq!(text(conn, "SELECT notes FROM stashes WHERE name = 'Holiday'"), "Lisbon\nin June");
        assert_eq!(count(conn, "SELECT target_minor FROM stashes WHERE name = 'Old car'"), 999_999_999_999);
    }
    // the same rows with the same stamps and the same `updated_at`, and the same numbers on the screens
    assert_eq!(stamps_of(&desktop, "stashes"), stamps_of(&phone, "stashes"));
    assert_eq!(stamps_of(&desktop, "stash_entries"), stamps_of(&phone, "stash_entries"));
    assert_eq!(shown(&desktop), shown(&phone));
    assert_eq!(saved(&phone, "Holiday"), "120.00");
}

#[test]
fn devices_that_never_made_a_stash_send_none() {
    let (mut desktop, mut phone) = met_with_accounts();
    let outcome = settle(&mut desktop, &mut phone);
    assert!(outcome.compared.is_empty(), "{outcome:?}");
    let digest = engine::digest(&desktop).unwrap();
    assert!(digest.0[&Table::Stashes].is_empty() && digest.0[&Table::StashEntries].is_empty());
    assert_eq!(engine::hashes(&digest).len(), 13, "a hash for each of the thirteen tables");
}

// --- random edits -----------------------------------------------------------------------------------------------------------

/// A step of the random walks: mostly stashes and their entries, now and then an account, a transfer or a deletion of one.
fn step(conn: &mut Connection, rng: &mut Rng) {
    if rng.below(10) < 7 {
        edit_stashes(conn, rng);
    } else {
        edit_accounts(conn, rng);
    }
}

/// When two devices that were just synced differ, shows what each held of the rows that differ before the sync, so a failure can be read.
fn show_what_differed(desktop: &Connection, phone: &Connection, before: &(Vec<String>, Vec<String>)) {
    let (x, y) = (snapshot(desktop).unwrap(), snapshot(phone).unwrap());
    for line in x.iter().filter(|l| !y.contains(l)).chain(y.iter().filter(|l| !x.contains(l))) {
        let id: String = line.split(|c: char| !c.is_ascii_hexdigit()).find(|part| part.len() == 32).unwrap_or("").to_string();
        for (who, held) in [("desktop before", &before.0), ("phone before", &before.1)] {
            for l in held.iter().filter(|l| l.contains(&id)) {
                eprintln!("{who}: {}", l.chars().take(330).collect::<String>());
            }
        }
    }
}

fn after_a_sync_every_rule_still_holds(conn: &Connection, label: &str) {
    assert!(accounts::check(conn).unwrap(), "{label}");
    assert_eq!(count(conn, "SELECT count(*) FROM pragma_foreign_key_check"), 0, "{label}");
    assert_eq!(count(conn, "SELECT applying FROM sync_clock"), 0, "{label}");
    assert_eq!(text(conn, "PRAGMA integrity_check"), "ok", "{label}");
    assert_eq!(count(conn, "SELECT count(*) FROM transfers WHERE from_account_id = to_account_id"), 0, "{label}");
    // the markers of a stash and of its entries are never for a row that is still there
    assert_eq!(count(conn, "SELECT count(*) FROM sync_deleted d JOIN stashes s ON s.sync_id = d.sync_id"), 0, "{label}: a stash with a marker");
    assert_eq!(count(conn, "SELECT count(*) FROM sync_deleted d JOIN stash_entries e ON e.sync_id = d.sync_id"), 0, "{label}: an entry with a marker");
}

/// Stashes, their entries, the accounts they live in and the transfers that come with them, with the deletions of accounts meeting stashes
/// the other device made in them and the deletions of transfers meeting entries that were linked to them.
#[test]
fn random_edits_of_stashes_always_end_the_same_after_one_sync() {
    for seed in first_seed(1)..first_seed(1) + seeds(80) {
        let mut rng = Rng::new(seed);
        let mut desktop = device("EUR");
        let mut phone = device("EUR");
        category(&desktop, "Pay", CategoryType::Income);
        funded(&desktop, "Cash", "300.00");
        funded(&desktop, "Savings", "300.00");
        sync_ok(&mut desktop, &mut phone);
        for round in 0..4 {
            for _ in 0..(3 + rng.below(12)) {
                step(&mut desktop, &mut rng);
            }
            pause();
            for _ in 0..(3 + rng.below(12)) {
                step(&mut phone, &mut rng);
            }
            if rng.below(2) == 0 {
                pause();
            }
            // the sync is started from either side
            let before = (snapshot(&desktop).unwrap(), snapshot(&phone).unwrap());
            let outcome = if round % 2 == 0 { sync(&mut desktop, &mut phone, TODAY, now(), None) } else { sync(&mut phone, &mut desktop, TODAY, now(), None) }
                .unwrap_or_else(|e| panic!("seed {seed} round {round}: {e}"));
            show_what_differed(&desktop, &phone, &before);
            assert_same(&desktop, &phone, &format!("seed {seed} round {round} ({outcome:?})"));
            assert!(!outcome.differences, "seed {seed} round {round}: differ after one sync: {outcome:?}");
            let again = sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap();
            assert!(again.compared.is_empty() && again.first.is_empty() && again.second.is_empty(), "seed {seed} round {round}: not settled: {again:?}");
            for conn in [&desktop, &phone] {
                after_a_sync_every_rule_still_holds(conn, &format!("seed {seed} round {round}"));
            }
        }
    }
}

/// Only stashes, on both devices at once and a lot of them: the same few names and the same accounts, so the two devices keep meeting.
#[test]
fn random_edits_of_stashes_on_both_devices_with_nothing_else_going_on_end_the_same() {
    for seed in first_seed(500)..first_seed(500) + seeds(60) {
        let mut rng = Rng::new(seed);
        let mut desktop = device("EUR");
        let mut phone = device("EUR");
        category(&desktop, "Pay", CategoryType::Income);
        funded(&desktop, "Cash", "500.00");
        funded(&desktop, "Savings", "500.00");
        sync_ok(&mut desktop, &mut phone);
        for round in 0..5 {
            for _ in 0..(4 + rng.below(14)) {
                edit_stashes(&mut desktop, &mut rng);
            }
            pause();
            for _ in 0..(4 + rng.below(14)) {
                edit_stashes(&mut phone, &mut rng);
            }
            let before = (snapshot(&desktop).unwrap(), snapshot(&phone).unwrap());
            let outcome = sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap_or_else(|e| panic!("seed {seed} round {round}: {e}"));
            show_what_differed(&desktop, &phone, &before);
            assert_same(&desktop, &phone, &format!("seed {seed} round {round} ({outcome:?})"));
            assert!(!outcome.differences, "seed {seed} round {round}: {outcome:?}");
            for conn in [&desktop, &phone] {
                after_a_sync_every_rule_still_holds(conn, &format!("seed {seed} round {round}"));
            }
        }
    }
}

// --- what a sync may leave (section 7.6) ---------------------------------------------------------------------------------

#[test]
fn two_devices_that_each_add_to_a_stash_with_a_target_end_above_it_and_nothing_is_refused() {
    let (mut desktop, mut phone) = met_with_accounts();
    make(&desktop, "Holiday", account_id(&desktop, "Cash"), Some("500.00"));
    settle(&mut desktop, &mut phone);
    pause();
    // each device saw 0 of 500 and added 300: fine alone
    put(&desktop, stash_id(&desktop, "Holiday"), "300.00", "from the desktop");
    put(&phone, stash_id(&phone, "Holiday"), "300.00", "from the phone");
    let outcome = settle(&mut desktop, &mut phone);
    assert_eq!((outcome.first.added, outcome.second.added, outcome.first.kept_yours, outcome.second.kept_yours), (1, 1, 0, 0), "{outcome:?}");
    for conn in [&desktop, &phone] {
        let detail = stashes::get(conn, stash_id(conn, "Holiday"), TODAY).unwrap();
        assert_eq!((detail.stash.saved.as_str(), detail.stash.reached, detail.stash.percent, detail.stash.left.as_deref()), ("600.00", true, Some(120), Some("0.00")));
        assert_eq!(detail.entries.len(), 2);
        // the core still guards what is written next: nothing more goes in, and money can come out
        assert_eq!(entries::add(conn, stash_id(conn, "Holiday"), &NewEntry { amount: "1.00".into(), date: None, note: None, from_account_id: None }, TODAY).err(), Some(AppError::InvalidInput));
        assert!(entries::take(conn, stash_id(conn, "Holiday"), &NewTake { amount: "150.00".into(), date: None, note: None, to_account_id: None }, TODAY).is_ok());
    }
}

#[test]
fn two_devices_that_each_take_out_the_last_100_end_with_minus_100_and_nothing_is_refused() {
    let (mut desktop, mut phone) = met_with_accounts();
    let rainy = make(&desktop, "Rainy day", account_id(&desktop, "Cash"), None);
    put(&desktop, rainy, "100.00", "deposit");
    settle(&mut desktop, &mut phone);
    pause();
    pull(&desktop, stash_id(&desktop, "Rainy day"), "100.00", "out on the desktop");
    pull(&phone, stash_id(&phone, "Rainy day"), "100.00", "out on the phone");
    let outcome = settle(&mut desktop, &mut phone);
    assert_eq!((outcome.first.added, outcome.second.added), (1, 1), "{outcome:?}");
    for conn in [&desktop, &phone] {
        let stash = stashes::get(conn, stash_id(conn, "Rainy day"), TODAY).unwrap().stash;
        assert_eq!(stash.saved, "-100.00");
        // a stash below 0 takes nothing away from its account
        assert_eq!(stashes::free(conn, account_id(conn, "Cash"), TODAY).unwrap(), 100_000);
        // only an add is offered: a take is refused, an add is not
        assert_eq!(entries::take(conn, stash_id(conn, "Rainy day"), &NewTake { amount: "1.00".into(), date: None, note: None, to_account_id: None }, TODAY).err(), Some(AppError::InvalidInput));
        assert!(entries::add(conn, stash_id(conn, "Rainy day"), &NewEntry { amount: "10.00".into(), date: None, note: None, from_account_id: None }, TODAY).is_ok());
    }
}

#[test]
fn two_devices_that_each_set_aside_all_the_free_money_of_an_account_leave_it_with_less_than_nothing_free() {
    let (mut desktop, mut phone) = met_with_accounts();
    let cash = account_id(&desktop, "Cash");
    make(&desktop, "Holiday", cash, None);
    make(&desktop, "Laptop", cash, None);
    settle(&mut desktop, &mut phone);
    pause();
    put(&desktop, stash_id(&desktop, "Holiday"), "1000.00", "all of it, on the desktop");
    put(&phone, stash_id(&phone, "Laptop"), "1000.00", "all of it, on the phone");
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(stashes::free(conn, account_id(conn, "Cash"), TODAY).unwrap(), -100_000);
        let cash = accounts::list(conn, TODAY).unwrap().into_iter().find(|a| a.name == "Cash").unwrap();
        assert_eq!((cash.balance.as_str(), cash.set_aside.as_str(), cash.free.as_str()), ("1000.00", "2000.00", "-1000.00"));
        // nothing more can be set aside there; money can come out
        assert_eq!(entries::add(conn, stash_id(conn, "Holiday"), &NewEntry { amount: "1.00".into(), date: None, note: None, from_account_id: None }, TODAY).err(), Some(AppError::InvalidInput));
        assert!(entries::take(conn, stash_id(conn, "Holiday"), &NewTake { amount: "1.00".into(), date: None, note: None, to_account_id: None }, TODAY).is_ok());
    }
}

// --- never merged by name --------------------------------------------------------------------------------------------------

#[test]
fn two_devices_that_make_a_stash_of_one_name_keep_both_and_merge_nothing() {
    let (mut desktop, mut phone) = met_with_accounts();
    let cash = |conn: &Connection| account_id(conn, "Cash");
    let (d, p) = (make(&desktop, "Holiday", cash(&desktop), Some("100.00")), make(&phone, "Holiday", cash(&phone), Some("200.00")));
    put(&desktop, d, "10.00", "d1");
    put(&phone, p, "20.00", "p1");
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(names(conn), ["Holiday", "Holiday"]);
        assert_eq!(count(conn, "SELECT count(*) FROM stash_entries"), 2);
        // each entry stayed with the stash it was made in
        assert_eq!(count(conn, "SELECT s.target_minor FROM stash_entries e JOIN stashes s ON s.id = e.stash_id WHERE e.note = 'd1'"), 10_000);
        assert_eq!(count(conn, "SELECT s.target_minor FROM stash_entries e JOIN stashes s ON s.id = e.stash_id WHERE e.note = 'p1'"), 20_000);
    }
    assert_eq!(shown(&desktop), shown(&phone));
    // a stash renamed onto the name another one has on the other device is no merge either
    pause();
    make(&desktop, "Gift", cash(&desktop), None);
    make(&phone, "Present", cash(&phone), None);
    stashes::update(&phone, stash_id(&phone, "Present"), &StashPatch { name: Some("Gift".into()), ..Default::default() }, TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    assert_eq!(names(&desktop), ["Gift", "Gift", "Holiday", "Holiday"]);
    assert_eq!(names(&phone), names(&desktop));
}

// --- a stash deleted on one device, and what the other does to it ------------------------------------------------------------

#[test]
fn a_stash_deleted_on_one_device_while_the_other_adds_to_it_is_gone_on_both_and_the_add_is_not_taken() {
    for deleter_is_desktop in [true, false] {
        for deleter_first in [true, false] {
            let (desktop, phone) = met_with_a_stash();
            let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
            let holiday = stash_sync_id(&a, "Holiday");
            stashes::delete(&a, stash_id(&a, "Holiday")).unwrap();
            pause();
            put(&b, stash_id(&b, "Holiday"), "20.00", "late");
            let deleted_at = marker_of(&a, &holiday).unwrap();
            let (summary_a, summary_b) = settle_from(deleter_first, &mut a, &mut b);
            // the deleter was sent the two entries it had lost with the stash and the new one, and took none of them
            assert_eq!((summary_a.added, summary_a.kept_yours), (0, 3), "deleter's summary: {summary_a:?}");
            assert_eq!(summary_b.deleted, 1, "the other device deleted the stash: {summary_b:?}");
            for conn in [&a, &b] {
                assert_eq!(count(conn, "SELECT count(*) FROM stashes"), 0);
                assert_eq!(count(conn, "SELECT count(*) FROM stash_entries"), 0, "the entries went with the stash, the new one included");
                assert_eq!(marker_of(conn, &holiday).as_deref(), Some(deleted_at.as_str()), "one marker, the same on both");
                assert_eq!(count(conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'stash_entries'"), 0, "no marker for what the stash took with it");
                assert_eq!(stashes::free(conn, account_id(conn, "Cash"), TODAY).unwrap(), 100_000, "the money is free again");
            }
        }
    }
}

#[test]
fn a_stash_edited_after_it_was_deleted_elsewhere_is_back_on_both_with_its_entries_sent_again() {
    for deleter_is_desktop in [true, false] {
        for deleter_first in [true, false] {
            let (desktop, phone) = met_with_a_stash();
            let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
            let holiday = stash_sync_id(&a, "Holiday");
            stashes::delete(&a, stash_id(&a, "Holiday")).unwrap();
            pause();
            stashes::update(&b, stash_id(&b, "Holiday"), &StashPatch { name: Some("Holiday 2027".into()), ..Default::default() }, TODAY).unwrap();
            put(&b, stash_id(&b, "Holiday 2027"), "20.00", "late");
            let (summary_a, _) = settle_from(deleter_first, &mut a, &mut b);
            assert_eq!(summary_a.added, 4, "the stash and its three entries come back to the deleter: {summary_a:?}");
            for conn in [&a, &b] {
                assert_eq!(names(conn), ["Holiday 2027"]);
                assert_eq!(notes_of_entries(conn), ["first", "late", "second"]);
                assert_eq!(marker_of(conn, &holiday), None, "the marker goes with the deletion it was");
                assert_eq!(saved(conn, "Holiday 2027"), "170.00");
            }
        }
    }
}

#[test]
fn a_stash_edited_before_it_was_deleted_elsewhere_stays_deleted_and_so_does_what_was_added_after() {
    for deleter_is_desktop in [true, false] {
        let (desktop, phone) = met_with_a_stash();
        let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
        let holiday = stash_sync_id(&a, "Holiday");
        stashes::update(&b, stash_id(&b, "Holiday"), &StashPatch { name: Some("Holiday 2027".into()), ..Default::default() }, TODAY).unwrap();
        pause();
        stashes::delete(&a, stash_id(&a, "Holiday")).unwrap();
        pause();
        put(&b, stash_id(&b, "Holiday 2027"), "20.00", "late");
        settle(&mut a, &mut b);
        for conn in [&a, &b] {
            assert_eq!(count(conn, "SELECT count(*) FROM stashes"), 0);
            assert_eq!(count(conn, "SELECT count(*) FROM stash_entries"), 0);
            assert!(marker_of(conn, &holiday).is_some());
        }
    }
}

#[test]
fn an_entry_deleted_before_its_stash_stays_deleted_when_the_stash_comes_back() {
    let (desktop, phone) = met_with_a_stash();
    let (mut a, mut b) = (desktop, phone);
    let (holiday, second) = (stash_sync_id(&a, "Holiday"), entry_sync_id(&a, "second"));
    entries::delete(&a, entry_id(&a, "second"), false, TODAY).unwrap();
    pause();
    stashes::delete(&a, stash_id(&a, "Holiday")).unwrap();
    // the entry made its marker while its stash was there, and the stash's cascade made none for `first`
    assert!(marker_of(&a, &second).is_some() && marker_of(&a, &holiday).is_some());
    assert_eq!(count(&a, "SELECT count(*) FROM sync_deleted WHERE kind = 'stash_entries'"), 1);
    pause();
    stashes::update(&b, stash_id(&b, "Holiday"), &StashPatch { notes: Some(Some("back".into())), ..Default::default() }, TODAY).unwrap();
    settle(&mut a, &mut b);
    for conn in [&a, &b] {
        assert_eq!(names(conn), ["Holiday"]);
        assert_eq!(notes_of_entries(conn), ["first"], "the entry that was deleted on its own stays deleted, the one the stash's deletion took comes back");
        assert!(marker_of(conn, &second).is_some());
        assert_eq!(marker_of(conn, &holiday), None);
    }
}

#[test]
fn an_entry_deleted_before_its_stash_and_edited_on_the_other_device_afterwards_leaves_no_marker_on_either() {
    // The deleter holds a marker for the entry (it was deleted while its stash was there) and one for the stash. The other device edits the entry
    // after that: its copy is newer than the marker, so it is sent, and it is not taken (the stash is deleted). The other device then deletes the stash
    // and with it the entry, and leaves no marker for that: so the deleter's marker goes, or the two would differ after the sync.
    for deleter_is_desktop in [true, false] {
        for deleter_first in [true, false] {
            let (desktop, phone) = met_with_a_stash();
            let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
            let holiday = stash_sync_id(&a, "Holiday");
            let second = entry_sync_id(&a, "second");
            entries::delete(&a, entry_id(&a, "second"), false, TODAY).unwrap();
            pause();
            stashes::delete(&a, stash_id(&a, "Holiday")).unwrap();
            pause();
            entries::update(&b, entry_id(&b, "second"), &EntryPatch { note: Some(Some("edited".into())), ..Default::default() }, TODAY).unwrap();
            let (summary_a, _) = settle_from(deleter_first, &mut a, &mut b);
            assert_eq!(summary_a.added, 0, "{summary_a:?}");
            for conn in [&a, &b] {
                assert_eq!(count(conn, "SELECT count(*) FROM stashes"), 0);
                assert_eq!(count(conn, "SELECT count(*) FROM stash_entries"), 0);
                assert!(marker_of(conn, &holiday).is_some());
                assert_eq!(marker_of(conn, &second), None, "the marker of the entry goes when a newer copy of it meets the marker of its stash");
            }
        }
    }
}

#[test]
fn a_stash_and_an_entry_deleted_on_both_devices_at_once_are_one_deletion_each() {
    let (mut desktop, mut phone) = met_with_a_stash();
    make(&desktop, "Laptop", account_id(&desktop, "Savings"), None);
    settle(&mut desktop, &mut phone);
    pause();
    let (laptop, first) = (stash_sync_id(&desktop, "Laptop"), entry_sync_id(&desktop, "first"));
    stashes::delete(&desktop, stash_id(&desktop, "Laptop")).unwrap();
    entries::delete(&desktop, entry_id(&desktop, "first"), false, TODAY).unwrap();
    pause();
    stashes::delete(&phone, stash_id(&phone, "Laptop")).unwrap();
    entries::delete(&phone, entry_id(&phone, "first"), false, TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(count(conn, &format!("SELECT count(*) FROM sync_deleted WHERE sync_id IN ('{laptop}', '{first}')")), 2);
        assert_eq!(notes_of_entries(conn), ["second"]);
    }
}

#[test]
fn the_newer_of_an_edit_and_a_delete_of_an_entry_wins_on_both() {
    for edit_is_newer in [true, false] {
        let (mut desktop, mut phone) = met_with_a_stash();
        let first = entry_sync_id(&desktop, "first");
        let patch = EntryPatch { amount: Some("60.00".into()), ..Default::default() };
        if edit_is_newer {
            entries::delete(&desktop, entry_id(&desktop, "first"), false, TODAY).unwrap();
            pause();
            entries::update(&phone, entry_id(&phone, "first"), &patch, TODAY).unwrap();
        } else {
            entries::update(&phone, entry_id(&phone, "first"), &patch, TODAY).unwrap();
            pause();
            entries::delete(&desktop, entry_id(&desktop, "first"), false, TODAY).unwrap();
        }
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            if edit_is_newer {
                assert_eq!(entry_state(conn, "first").0, 6000, "back, with the edit");
                assert_eq!(marker_of(conn, &first), None);
            } else {
                assert_eq!(count(conn, "SELECT count(*) FROM stash_entries WHERE note = 'first'"), 0);
                assert!(marker_of(conn, &first).is_some());
            }
        }
    }
}

#[test]
fn the_newer_whole_stash_wins_not_the_fields_that_changed() {
    let (mut desktop, mut phone) = met_with_a_stash();
    stashes::update(&desktop, stash_id(&desktop, "Holiday"), &StashPatch { target: Some(Some("700.00".into())), ..Default::default() }, TODAY).unwrap();
    pause();
    stashes::archive(&phone, stash_id(&phone, "Holiday"), TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(text(conn, "SELECT archived || '/' || target_minor FROM stashes"), "1/50000", "the phone's row as it is, with the old target");
    }
}

#[test]
fn a_new_order_a_new_account_and_an_archive_of_a_stash_arrive_on_the_other_device() {
    let (mut desktop, mut phone) = met_with_accounts();
    let cash = account_id(&desktop, "Cash");
    for name in ["Alpha", "Beta", "Gamma"] {
        make(&desktop, name, cash, None);
    }
    settle(&mut desktop, &mut phone);
    pause();
    let ids: Vec<i64> = ["Gamma", "Alpha", "Beta"].iter().map(|n| stash_id(&desktop, n)).collect();
    stashes::reorder(&desktop, &ids).unwrap();
    stashes::update(&desktop, stash_id(&desktop, "Beta"), &StashPatch { account_id: Some(account_id(&desktop, "Savings").to_string()), ..Default::default() }, TODAY).unwrap();
    stashes::archive(&desktop, stash_id(&desktop, "Alpha"), TODAY).unwrap();
    settle(&mut desktop, &mut phone);
    let order = |conn: &Connection| stashes::list(conn, None, TODAY).unwrap().into_iter().map(|s| format!("{}/{}/{}", s.name, s.account, s.archived)).collect::<Vec<_>>();
    assert_eq!(order(&phone), ["Gamma/Cash/false", "Beta/Savings/false", "Alpha/Cash/true"]);
    assert_eq!(order(&desktop), order(&phone));
}

// --- an account deleted on one device while the other makes a stash in it -----------------------------------------------------

#[test]
fn an_account_deleted_on_one_device_while_the_other_makes_a_stash_in_it_ends_equal_with_the_stash_in_the_main_account() {
    // who deletes, which of the two the sync is started from, and whether the stash is older or newer than the deletion
    for (deleter_is_desktop, deleter_first, stash_is_older) in [(true, true, true), (true, true, false), (true, false, true), (true, false, false), (false, true, true), (false, true, false), (false, false, true), (false, false, false)] {
        let label = format!("deleter on the desktop: {deleter_is_desktop}, sync started by the deleter: {deleter_first}, stash older: {stash_is_older}");
        let (desktop, phone) = met_with_a_wallet();
        let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
        let wallet = account_sync_id(&a, "Wallet");
        let make_trip = |b: &Connection| {
            let trip = make(b, "Trip", account_id(b, "Wallet"), Some("80.00"));
            put(b, trip, "40.00", "trip money");
        };
        if stash_is_older {
            make_trip(&b);
            pause();
            accounts::delete(&a, account_id(&a, "Wallet")).unwrap();
        } else {
            accounts::delete(&a, account_id(&a, "Wallet")).unwrap();
            pause();
            make_trip(&b);
        }
        let deleted_at = marker_of(&a, &wallet).unwrap();
        let (stash_before, entry_before) = (stash_stamp(&b, "Trip"), entry_state(&b, "trip money"));

        settle_from(deleter_first, &mut a, &mut b);

        for (who, conn) in [("deleter", &a), ("maker", &b)] {
            let label = format!("{label}, on the {who}");
            assert_eq!(count(conn, "SELECT count(*) FROM accounts WHERE name = 'Wallet'"), 0, "{label}");
            assert_eq!(marker_of(conn, &wallet).as_deref(), Some(deleted_at.as_str()), "{label}: the same marker");
            // in the main account, with the later of its stamp and the deletion's; `updated_at` as it was written
            let (account, target, stamp, updated_at): (i64, i64, String, String) =
                conn.query_row("SELECT account_id, target_minor, stamp, updated_at FROM stashes WHERE name = 'Trip'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
            assert_eq!((account, target), (MAIN_ID, 8000), "{label}");
            assert_eq!(stamp, later_of(&stash_before.0, &deleted_at), "{label}: the stamp");
            assert_eq!(updated_at, stash_before.1, "{label}: the stash keeps the time it was written");
            assert_eq!(stamp > stash_before.0, stash_is_older, "{label}: raised exactly when the deletion is the later");
            // the entry is not touched: it follows its stash, which has not changed
            assert_eq!(entry_state(conn, "trip money"), entry_before, "{label}: the entry");
            assert_eq!(saved(conn, "Trip"), "40.00", "{label}");
        }
    }
}

#[test]
fn a_stash_of_a_foreign_account_deleted_elsewhere_moves_to_the_main_account_with_its_amounts_as_they_are() {
    for deleter_first in [true, false] {
        let (mut desktop, mut phone) = met_with_accounts();
        funded_in(&desktop, "Lei", "RON", "100.00");
        settle(&mut desktop, &mut phone);
        pause();
        accounts::delete(&phone, account_id(&phone, "Lei")).unwrap();
        pause();
        let trip = make(&desktop, "Trip", account_id(&desktop, "Lei"), Some("50.00"));
        put(&desktop, trip, "40.00", "lei money");
        settle_from(!deleter_first, &mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            let stash = stashes::list(conn, None, TODAY).unwrap().remove(0);
            // 40.00 of 50.00, now counted in the app's currency: the person corrects it
            assert_eq!((stash.account.as_str(), stash.currency.as_str(), stash.saved.as_str(), stash.target.as_deref()), ("Main account", "EUR", "40.00", Some("50.00")), "deleter first: {deleter_first}");
        }
    }
}

#[test]
fn a_stash_deleted_on_one_device_before_its_account_is_deleted_stays_deleted_on_the_other_device() {
    // The other device holds the stash in the account. Its own deletion and the account's arrive together: the stash must go first, or the
    // account's deletion would move it to the main account with a stamp newer than its own deletion, and it would live on, on one device only.
    for deleter_is_desktop in [true, false] {
        for entry_too in [false, true] {
            let (mut desktop, mut phone) = met_with_a_wallet();
            let doomed = make(&desktop, "Doomed", account_id(&desktop, "Wallet"), None);
            if entry_too {
                put(&desktop, doomed, "10.00", "kept");
            }
            settle(&mut desktop, &mut phone);
            pause();
            let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
            let stash = stash_sync_id(&a, "Doomed");
            stashes::delete(&a, stash_id(&a, "Doomed")).unwrap();
            pause();
            accounts::delete(&a, account_id(&a, "Wallet")).unwrap();
            settle(&mut a, &mut b);
            for conn in [&a, &b] {
                assert_eq!(count(conn, "SELECT count(*) FROM stashes"), 0, "gone on both");
                assert_eq!(count(conn, "SELECT count(*) FROM stash_entries"), 0);
                assert!(marker_of(conn, &stash).is_some());
                assert_eq!(count(conn, "SELECT count(*) FROM accounts WHERE name = 'Wallet'"), 0);
            }
        }
    }
}

#[test]
fn a_stash_moved_to_another_account_while_that_account_is_deleted_elsewhere_ends_in_the_main_account() {
    let (mut desktop, mut phone) = met_with_a_wallet();
    let (holiday, wallet) = (make(&desktop, "Holiday", account_id(&desktop, "Cash"), None), account_sync_id(&desktop, "Wallet"));
    put(&desktop, holiday, "10.00", "kept");
    settle(&mut desktop, &mut phone);
    pause();
    accounts::delete(&desktop, account_id(&desktop, "Wallet")).unwrap();
    pause();
    stashes::update(&phone, stash_id(&phone, "Holiday"), &StashPatch { account_id: Some(account_id(&phone, "Wallet").to_string()), ..Default::default() }, TODAY).unwrap();
    let moved_at = stash_stamp(&phone, "Holiday").0;
    let deleted_at = marker_of(&desktop, &wallet).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(text(conn, "SELECT a.name FROM stashes s JOIN accounts a ON a.id = s.account_id"), "Main account");
        assert_eq!(stash_stamp(conn, "Holiday").0, later_of(&moved_at, &deleted_at));
    }
}

// --- a transfer deleted on one device, and the entries that came with it ---------------------------------------------------------

#[test]
fn a_linked_transfer_deleted_on_one_device_clears_the_link_on_both_with_equal_stamps_and_the_entry_stays() {
    for deleter_is_desktop in [true, false] {
        for deleter_first in [true, false] {
            let (desktop, phone) = met_with_a_linked_entry();
            let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
            let transfer = text(&a, "SELECT sync_id FROM transfers WHERE note = 'brought'");
            let (_, linked, before_stamp, before_updated_at) = entry_state(&a, "brought");
            assert_eq!(linked.as_deref(), Some(transfer.as_str()));
            transfers::delete(&a, transfer_named(&a, "brought")).unwrap();
            // here already: the entry has one new stamp and the marker of the transfer is made with it
            let marker = marker_of(&a, &transfer).unwrap();
            assert_eq!(entry_state(&a, "brought").2, marker);
            assert!(marker > before_stamp);
            settle_from(deleter_first, &mut a, &mut b);
            for (who, conn) in [("the one that deleted it", &a), ("the other", &b)] {
                let (amount, link, stamp, updated_at) = entry_state(conn, "brought");
                assert_eq!((amount, link), (2000, None), "{who}: the entry is there with its amount and no link");
                assert_eq!(stamp, marker, "{who}: the stamp of the deletion, on both");
                assert_eq!(updated_at, before_updated_at, "{who}: clearing the link is not an edit");
                assert_eq!(marker_of(conn, &transfer).as_deref(), Some(marker.as_str()), "{who}");
                assert_eq!(count(conn, "SELECT count(*) FROM transfers"), 0, "{who}");
                assert_eq!(saved(conn, "Holiday"), "170.00", "{who}: what the stash holds does not change");
            }
        }
    }
}

#[test]
fn an_entry_edited_after_its_transfer_was_deleted_elsewhere_ends_unlinked_with_the_edit_and_its_stamp() {
    for deleter_is_desktop in [true, false] {
        let (desktop, phone) = met_with_a_linked_entry();
        let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
        let transfer = text(&a, "SELECT sync_id FROM transfers WHERE note = 'brought'");
        transfers::delete(&a, transfer_named(&a, "brought")).unwrap();
        pause();
        entries::update(&b, entry_id(&b, "brought"), &EntryPatch { amount: Some("25.00".into()), ..Default::default() }, TODAY).unwrap();
        let edited_at = entry_state(&b, "brought").2;
        assert!(edited_at > marker_of(&a, &transfer).unwrap());
        settle(&mut a, &mut b);
        for conn in [&a, &b] {
            let (amount, link, stamp, _) = entry_state(conn, "brought");
            assert_eq!((amount, link, stamp.as_str()), (2500, None, edited_at.as_str()));
        }
    }
}

#[test]
fn a_transfer_edited_after_it_was_deleted_elsewhere_comes_back_and_its_entry_stays_unlinked_on_both() {
    for deleter_is_desktop in [true, false] {
        let (desktop, phone) = met_with_a_linked_entry();
        let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
        let transfer = text(&a, "SELECT sync_id FROM transfers WHERE note = 'brought'");
        transfers::delete(&a, transfer_named(&a, "brought")).unwrap();
        let unlinked_at = marker_of(&a, &transfer).unwrap();
        pause();
        transfers::update(&b, transfer_named(&b, "brought"), &accounts::transfers::TransferPatch { amount: Some("25.00".into()), ..Default::default() }).unwrap();
        settle(&mut a, &mut b);
        for conn in [&a, &b] {
            assert_eq!(count(conn, "SELECT amount_minor FROM transfers"), 2500, "the transfer is back with the edit");
            assert_eq!(marker_of(conn, &transfer), None);
            let (_, link, stamp, _) = entry_state(conn, "brought");
            assert_eq!((link, stamp.as_str()), (None, unlinked_at.as_str()), "the entry lost its link and has the stamp of that");
        }
    }
}

#[test]
fn an_entry_deleted_on_one_device_before_its_transfer_is_deleted_stays_deleted_on_the_other_device() {
    // The other device holds the entry linked. Its own deletion and the transfer's arrive together: the entry must go first, or the deletion
    // of the transfer would give it a stamp newer than its own deletion and it would live on, on one device only.
    for deleter_is_desktop in [true, false] {
        let (desktop, phone) = met_with_a_linked_entry();
        let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
        let brought = entry_sync_id(&a, "brought");
        entries::delete(&a, entry_id(&a, "brought"), false, TODAY).unwrap();
        pause();
        transfers::delete(&a, transfer_named(&a, "brought")).unwrap();
        settle(&mut a, &mut b);
        for conn in [&a, &b] {
            assert_eq!(count(conn, "SELECT count(*) FROM stash_entries WHERE note = 'brought'"), 0, "gone on both");
            assert!(marker_of(conn, &brought).is_some());
            assert_eq!(count(conn, "SELECT count(*) FROM transfers"), 0);
        }
    }
}

#[test]
fn an_entry_that_came_with_a_transfer_that_an_account_deletion_drops_loses_its_link_and_both_end_with_one_stamp() {
    // The deletion of Wallet elsewhere leaves a transfer between Wallet and the main account with the main account on both sides: it becomes a
    // marker both devices work out (`docs/accounts-design.md`, section 7.4), and the entry that came with it loses its link on both.
    for (deleter_is_desktop, deleter_first, entry_is_older) in [(true, true, true), (true, true, false), (true, false, true), (true, false, false), (false, true, true), (false, true, false), (false, false, true), (false, false, false)] {
        let label = format!("deleter on the desktop: {deleter_is_desktop}, started by the deleter: {deleter_first}, entry older: {entry_is_older}");
        let (desktop, phone) = met_with_a_wallet();
        let (mut a, mut b) = if deleter_is_desktop { (desktop, phone) } else { (phone, desktop) };
        let wallet = account_sync_id(&a, "Wallet");
        let make_trip = |b: &Connection| {
            let trip = make(b, "Trip", account_id(b, "Wallet"), None);
            put_from(b, trip, "20.00", "brought", MAIN_ID);
        };
        if entry_is_older {
            make_trip(&b);
            pause();
            accounts::delete(&a, account_id(&a, "Wallet")).unwrap();
        } else {
            accounts::delete(&a, account_id(&a, "Wallet")).unwrap();
            pause();
            make_trip(&b);
        }
        let transfer = text(&b, "SELECT sync_id FROM transfers WHERE note = 'brought'");
        let (before, transfer_before) = (entry_state(&b, "brought"), text(&b, "SELECT stamp FROM transfers WHERE note = 'brought'"));
        let deleted_at = marker_of(&a, &wallet).unwrap();
        settle_from(deleter_first, &mut a, &mut b);
        for (who, conn) in [("deleter", &a), ("maker", &b)] {
            let label = format!("{label}, on the {who}");
            assert_eq!(count(conn, "SELECT count(*) FROM transfers"), 0, "{label}: the transfer is gone");
            // the marker is the one both devices work out from the transfer and the deletion
            let marker = marker_of(conn, &transfer).unwrap_or_else(|| panic!("{label}: no marker"));
            assert_eq!(marker, after(&later_of(&transfer_before, &deleted_at)), "{label}");
            let (amount, link, stamp, updated_at) = entry_state(conn, "brought");
            assert_eq!((amount, link.as_deref()), (2000, None), "{label}: the entry is there with its amount and no link");
            assert_eq!(stamp, later_of(&before.2, &marker), "{label}: the later of its stamp and the transfer's marker, on both");
            assert_eq!(updated_at, before.3, "{label}");
            assert_eq!(text(conn, "SELECT a.name FROM stashes s JOIN accounts a ON a.id = s.account_id WHERE s.name = 'Trip'"), "Main account", "{label}");
        }
    }
}

// --- the rest of what arrives is checked -------------------------------------------------------------------------------------

#[test]
fn rows_naming_a_stash_a_transfer_or_an_account_nobody_heard_of_stop_the_sync_and_change_nothing() {
    let (desktop, phone) = met_with_accounts();
    funded(&desktop, "Wallet", "100.00");
    let trip = make(&desktop, "Trip", account_id(&desktop, "Wallet"), None);
    put_from(&desktop, trip, "20.00", "brought", account_id(&desktop, "Cash"));
    // the stash without its account, the entry without its stash, the entry without its transfer
    let (mut one, mut two, mut three) = (new_phone(&desktop), phone, new_phone(&desktop));
    let rows = only(rows_to(&desktop, &one), &[Table::Stashes]);
    refused_with_a_broken_reference(&mut one, &rows, "a stash of an account nobody heard of");
    let rows = only(rows_to(&desktop, &two), &[Table::StashEntries]);
    refused_with_a_broken_reference(&mut two, &rows, "an entry of a stash nobody heard of");
    let rows = only(rows_to(&desktop, &three), &[Table::Accounts, Table::Stashes, Table::StashEntries]);
    refused_with_a_broken_reference(&mut three, &rows, "an entry of a transfer nobody heard of");
    // with everything they apply, and the devices are equal
    let mut four = new_phone(&desktop);
    let rows = rows_to(&desktop, &four);
    engine::apply(&mut four, &rows, now(), None).unwrap();
    assert_same(&desktop, &four, "all of it at once");
}

#[test]
fn a_batch_with_the_rows_of_stashes_applies_whatever_order_the_rows_come_in() {
    let (desktop, _) = met_with_a_linked_entry();
    let mut rng = Rng::new(11);
    for round in 0..10 {
        let mut phone = new_phone(&desktop);
        let mut incoming = rows_to(&desktop, &phone);
        let mut shuffled = Incoming::default();
        let mut order: Vec<Table> = incoming.rows.keys().copied().collect();
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i as u64 + 1) as usize);
        }
        for table in order {
            let mut rows = incoming.rows.remove(&table).unwrap();
            for i in (1..rows.len()).rev() {
                rows.swap(i, rng.below(i as u64 + 1) as usize);
            }
            shuffled.rows.insert(table, rows);
        }
        engine::apply(&mut phone, &shuffled, now(), None).unwrap_or_else(|e| panic!("round {round}: {e}"));
        assert_same(&desktop, &phone, &format!("round {round}"));
    }
}

#[test]
fn a_marker_for_a_stash_this_device_never_had_is_remembered_so_the_stash_cannot_come_back() {
    let (desktop, _) = met_with_a_stash();
    let mut third = new_phone(&desktop);
    let incoming = rows_to(&desktop, &third);
    let stash = stash_sync_id(&desktop, "Holiday");
    let mut markers = Incoming::default();
    markers.markers.insert(Table::Stashes, vec![Marker { sync_id: stash.clone(), stamp: newer_than_everything() }]);
    engine::apply(&mut third, &markers, now(), None).unwrap();
    assert!(marker_of(&third, &stash).is_some());
    assert_eq!(count(&third, "SELECT count(*) FROM stashes"), 0);
    // the stash and its two entries arrive, the marker is newer than the stash, and none of them is taken (and none is an error)
    let summary = engine::apply(&mut third, &incoming, now(), None).unwrap();
    assert_eq!((summary.added >= 2, summary.kept_yours), (true, 3), "{summary:?}");
    assert_eq!(count(&third, "SELECT count(*) FROM stashes"), 0);
    assert_eq!(count(&third, "SELECT count(*) FROM stash_entries"), 0);
    assert_eq!(count(&third, "SELECT count(*) FROM sync_deleted WHERE kind = 'stash_entries'"), 0, "the entries leave no marker of their own");
}

#[test]
fn a_marker_for_an_entry_is_obeyed_alone_and_applying_the_same_rows_twice_changes_nothing_the_second_time() {
    let (desktop, mut phone) = met_with_a_linked_entry();
    let incoming = rows_to(&desktop, &phone);
    let before = snapshot(&phone).unwrap();
    let first = engine::apply(&mut phone, &incoming, now(), None).unwrap();
    assert_eq!(first.added + first.updated, 0, "nothing new: {first:?}");
    assert_eq!(snapshot(&phone).unwrap(), before);
    let first_entry = entry_sync_id(&phone, "first");
    let mut markers = Incoming::default();
    markers.markers.insert(Table::StashEntries, vec![Marker { sync_id: first_entry.clone(), stamp: newer_than_everything() }]);
    let summary = engine::apply(&mut phone, &markers, now(), None).unwrap();
    assert_eq!(summary.deleted, 1);
    assert_eq!(notes_of_entries(&phone), ["brought", "second"]);
    assert!(marker_of(&phone, &first_entry).is_some());
}

#[test]
fn a_phone_with_only_a_stash_is_not_empty_so_it_is_not_paired_and_one_with_nothing_is() {
    let phone = device("EUR");
    assert!(engine::is_empty(&phone).unwrap());
    // a stash in the main account is the only thing that was made
    let stash = make(&phone, "Holiday", MAIN_ID, None);
    assert!(!engine::is_empty(&phone).unwrap());
    stashes::delete(&phone, stash).unwrap();
    assert!(!engine::is_empty(&phone).unwrap(), "its marker is a record too");
}

// --- conflicts -----------------------------------------------------------------------------------------------------------------

#[test]
fn a_conflict_over_a_stash_and_an_entry_lists_what_differs() {
    let (mut desktop, mut phone) = met_with_a_stash();
    let met = now();
    pause();
    let savings = account_id(&desktop, "Savings");
    // the desktop changes everything about the stash and the amount, the day and the note of an entry ...
    stashes::update(
        &desktop,
        stash_id(&desktop, "Holiday"),
        &StashPatch { kind: None, interest_rate: None, interest_period: None, start_date: None, name: Some("Trip".into()), account_id: Some(savings.to_string()), target: Some(Some("600.00".into())), target_date: Some(Some("2027-07-01".into())), notes: Some(Some("from the desktop".into())) },
        TODAY,
    )
    .unwrap();
    entries::update(&desktop, entry_id(&desktop, "first"), &EntryPatch { amount: Some("30.00".into()), date: Some("2026-03-03".into()), note: Some(Some("by the desktop".into())) }, TODAY).unwrap();
    // ... and so does the phone, differently
    stashes::archive(&phone, stash_id(&phone, "Holiday"), TODAY).unwrap();
    entries::update(&phone, entry_id(&phone, "first"), &EntryPatch { amount: Some("40.00".into()), date: Some("2026-03-04".into()), note: Some(Some("by the phone".into())) }, TODAY).unwrap();

    let (conflicts, views) = conflicts_between(&desktop, &phone, met);
    assert_eq!(conflicts.iter().map(|c| c.table).collect::<Vec<_>>(), [Table::Stashes, Table::StashEntries]);
    // the keys are `name`, `account` (its name on this device), `target`, `targetDate`, `notes` and `archived`
    assert_eq!(
        fields(&views, Table::Stashes),
        texts(&[
            ("name", "Trip", "Holiday"),
            ("account", "Savings", "Cash"),
            ("target", "600.00", "500.00"),
            ("targetDate", "2027-07-01", "2027-06-30"),
            ("notes", "from the desktop", "Lisbon"),
            ("archived", "no", "yes"),
        ])
    );
    // `stash` (its name), `kind`, `amount` (always above 0), `date` and `note`: the stash and the kind are the same on both, so they are not listed
    assert_eq!(fields(&views, Table::StashEntries), texts(&[("amount", "30.00", "40.00"), ("date", "2026-03-03", "2026-03-04"), ("note", "by the desktop", "by the phone")]));
    assert_eq!(views.iter().map(|v| v.title.as_str()).collect::<Vec<_>>(), ["Trip", "Trip"], "an entry is named by its stash");

    // the choices are only stamps: the stash as the desktop had it, the entry as the phone had it
    engine::decide(&mut desktop, &conflicts, &[engine::Choice::Mine, engine::Choice::Theirs]).unwrap();
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(text(conn, "SELECT name || '/' || archived || '/' || target_minor FROM stashes"), "Trip/0/60000");
        assert_eq!(text(conn, "SELECT a.name FROM stashes s JOIN accounts a ON a.id = s.account_id"), "Savings");
        assert_eq!(entry_state(conn, "by the phone").0, 4000);
    }
}

#[test]
fn a_conflict_over_an_entry_that_is_an_add_on_one_device_and_a_take_on_the_other_lists_the_kind() {
    let (desktop, phone) = met_with_a_stash();
    let met = now();
    pause();
    // the core never turns an add into a take (the kind of an entry is its sign), so this is written straight into the table
    desktop.execute("UPDATE stash_entries SET amount_minor = -amount_minor WHERE note = 'first'", []).unwrap();
    entries::update(&phone, entry_id(&phone, "first"), &EntryPatch { note: Some(Some("edited".into())), ..Default::default() }, TODAY).unwrap();
    let (_, views) = conflicts_between(&desktop, &phone, met);
    assert_eq!(fields(&views, Table::StashEntries), texts(&[("kind", "TAKE", "ADD"), ("note", "first", "edited")]), "the amount is a size on both");
}

#[test]
fn a_conflict_over_a_stash_deleted_on_one_device_shows_what_the_other_still_has() {
    let (desktop, phone) = met_with_a_stash();
    let met = now();
    pause();
    stashes::delete(&desktop, stash_id(&desktop, "Holiday")).unwrap();
    stashes::update(&phone, stash_id(&phone, "Holiday"), &StashPatch { name: Some("Holiday 2".into()), ..Default::default() }, TODAY).unwrap();
    let (conflicts, views) = conflicts_between(&desktop, &phone, met);
    assert_eq!(conflicts.iter().map(|c| (c.table, c.mine_deleted, c.theirs_deleted)).collect::<Vec<_>>(), [(Table::Stashes, true, false)], "the entries went with the stash and left no marker: no conflict of theirs");
    assert_eq!(views[0].title, "Holiday 2");
    assert!(fields(&views, Table::Stashes).contains(&("name".to_string(), String::new(), "Holiday 2".to_string())));
}

// --- the version before stashes ----------------------------------------------------------------------------------------------

#[test]
fn a_device_that_speaks_protocol_11_is_told_to_update_whichever_of_the_two_it_is_and_nothing_is_exchanged() {
    let (a, b) = (device("EUR"), device("EUR"));
    let new = engine::hello(&a, "desktop", "0.1.0", now()).unwrap();
    let mut old = engine::hello(&b, "phone", "0.1.0", now()).unwrap();
    assert_eq!(new.protocol, 12);
    old.protocol = 11;
    assert!(matches!(engine::check_hello(&new, &old, now()), Err(SyncError::ProtocolMismatch)));
    assert!(matches!(engine::check_hello(&old, &new, now()), Err(SyncError::ProtocolMismatch)));
    // the schema is the other check: stashes are a migration too, and a device at schema 20 is told to update by that one
    assert_eq!(new.schema, crate::migrate::latest_version());
    assert!(new.schema >= 21);
}

// --- a sync that is cut -----------------------------------------------------------------------------------------------------------

/// Stashes in flight both ways: the desktop deletes Holiday and makes Laptop (with money brought from Cash, so a transfer is in it) and Gift,
/// while the phone adds to Holiday, takes out of it and makes a stash of its own.
fn stashes_in_flight() -> (Connection, Connection) {
    let (desktop, phone) = met_with_a_stash();
    stashes::delete(&desktop, stash_id(&desktop, "Holiday")).unwrap();
    let laptop = make(&desktop, "Laptop", account_id(&desktop, "Savings"), Some("300.00"));
    put_from(&desktop, laptop, "75.00", "for the laptop", account_id(&desktop, "Cash"));
    let gift = make(&desktop, "Gift", account_id(&desktop, "Cash"), None);
    put(&desktop, gift, "5.00", "ribbon");
    pause();
    put(&phone, stash_id(&phone, "Holiday"), "20.00", "late");
    pull(&phone, stash_id(&phone, "Holiday"), "10.00", "out");
    let own = make(&phone, "Rainy day", account_id(&phone, "Savings"), None);
    put(&phone, own, "15.00", "umbrella");
    (desktop, phone)
}

#[test]
fn a_sync_cut_after_any_message_changes_nothing_before_the_rows_are_in_and_the_next_one_finishes_it() {
    let total = {
        let (mut desktop, mut phone) = stashes_in_flight();
        sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap().messages
    };
    assert!(total > 10, "{total} messages");
    for cut in 0..total {
        let (mut desktop, mut phone) = stashes_in_flight();
        let before = (snapshot(&desktop).unwrap(), snapshot(&phone).unwrap());
        let result = sync_cut_after(&mut desktop, &mut phone, TODAY, now(), cut);
        assert!(result.is_err(), "cut after {cut} of {total} messages");
        // the rows are applied after the last of them is in: only the two closing messages come after
        if cut < total - 2 {
            assert_eq!((snapshot(&desktop).unwrap(), snapshot(&phone).unwrap()), before, "cut after {cut} of {total} messages: nothing was applied");
        } else {
            assert_same(&desktop, &phone, &format!("cut after {cut} of {total}: both had applied it"));
        }
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT applying FROM sync_clock"), 0, "cut after {cut}");
        }
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(names(conn), ["Gift", "Laptop", "Rainy day"], "cut after {cut}");
            assert_eq!(notes_of_entries(conn), ["for the laptop", "ribbon", "umbrella"], "cut after {cut}");
            assert_eq!(count(conn, "SELECT count(*) FROM transfers"), 1, "cut after {cut}");
        }
    }
}

#[test]
fn a_sync_that_only_one_side_finished_is_finished_by_the_next() {
    for desktop_applied in [true, false] {
        let (mut desktop, mut phone) = stashes_in_flight();
        // one device got everything the other sent and applied it; the other never did
        if desktop_applied {
            let incoming = rows_to(&phone, &desktop);
            engine::apply(&mut desktop, &incoming, now(), None).unwrap();
        } else {
            let incoming = rows_to(&desktop, &phone);
            engine::apply(&mut phone, &incoming, now(), None).unwrap();
        }
        settle(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(names(conn), ["Gift", "Laptop", "Rainy day"]);
        }
    }
}

#[test]
fn a_stash_follows_the_renaming_of_its_account_and_a_take_goes_through_when_another_account_is_archived_elsewhere() {
    let (mut desktop, mut phone) = met_with_a_stash();
    accounts::update(&desktop, account_id(&desktop, "Cash"), &AccountPatch { name: Some("Wallet of cash".into()), ..Default::default() }, TODAY).unwrap();
    accounts::archive(&phone, account_id(&phone, "Savings"), TODAY).unwrap();
    pause();
    pull(&phone, stash_id(&phone, "Holiday"), "10.00", "out");
    settle(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(text(conn, "SELECT a.name FROM stashes s JOIN accounts a ON a.id = s.account_id"), "Wallet of cash");
        assert_eq!(saved(conn, "Holiday"), "140.00");
    }
}

#[test]
fn what_a_device_changed_in_stashes_counts_as_waiting_for_the_next_sync() {
    let conn = device("EUR");
    let savings = funded(&conn, "Savings", "1000.00");
    let before = super::peers::changes_since(&conn, 0).unwrap();
    let holiday = stashes::create(&conn, &NewStash { kind: None, interest_rate: None, interest_period: None, start_date: None, name: "Holiday".into(), account_id: savings.to_string(), target: None, target_date: None, notes: None }, TODAY).unwrap().id;
    entries::add(&conn, holiday, &NewEntry { amount: "10.00".into(), date: None, note: None, from_account_id: None }, TODAY).unwrap();
    assert_eq!(super::peers::changes_since(&conn, 0).unwrap(), before + 2, "a stash and an entry");
    stashes::delete(&conn, holiday).unwrap();
    assert_eq!(super::peers::changes_since(&conn, 0).unwrap(), before + 1, "both are gone: the stash's marker is the one change in their place");
}
