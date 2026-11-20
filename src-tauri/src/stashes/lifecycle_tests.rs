//! The life of a stash, row by row of the state machine (`docs/stash-design.md`, section 5.1), for what happens on this device: made, changed,
//! moved, reordered, archived, restored, deleted. Every refusal is checked to have changed nothing. The items of section 5.4 that are about a
//! stash itself are named after their number.

use serde_json::json;

use super::fixtures::*;
use super::{archive, create, delete, get, history, list, reorder, restore, update, NewStash, Stash, StashPatch};
use crate::accounts::{self, AccountPatch};
use crate::debts::fixtures::{one, text};

fn names(conn: &rusqlite::Connection) -> Vec<String> {
    list(conn, None, TODAY).unwrap().into_iter().map(|s| s.name).collect()
}

fn rename(name: &str) -> StashPatch {
    StashPatch { name: Some(name.into()), ..Default::default() }
}

fn move_to(account: i64) -> StashPatch {
    StashPatch { account_id: Some(account.to_string()), ..Default::default() }
}

// ---- NONE: create ----------------------------------------------------------------------------

#[test]
fn a_new_stash_goes_to_the_end_of_the_list_with_what_was_given_and_the_defaults_for_the_rest() {
    let w = world();
    let savings = funded(&w.conn, "Savings", "1000.00");
    let plain = create(&w.conn, &new_stash("  Holiday  ", savings), TODAY).unwrap();
    assert_eq!(
        plain,
        Stash {
            id: 1,
            name: "Holiday".into(),
            account_id: savings,
            account: "Savings".into(),
            currency: "EUR".into(),
            archived: false,
            account_archived: false,
            target: None,
            target_date: None,
            notes: None,
            saved: "0.00".into(),
            left: None,
            percent: None,
            reached: false,
            per_month: None,
            overdue: false,
            free: "1000.00".into(),
            entries: 0,
            last_entry: None,
        }
    );
    let full = create(
        &w.conn,
        &NewStash { target: Some("1200.00".into()), target_date: Some("2027-06-30".into()), notes: Some("  two\r\nlines  ".into()), ..new_stash("Laptop", 1) },
        TODAY,
    )
    .unwrap();
    assert_eq!(
        (full.target.as_deref(), full.target_date.as_deref(), full.notes.as_deref(), full.left.as_deref(), full.percent, full.per_month.as_deref(), full.overdue, full.reached),
        (Some("1200.00"), Some("2027-06-30"), Some("two\nlines"), Some("1200.00"), Some(0), Some("171.43"), false, false),
        "seven months from November to June, and 1200.00 over seven rounds up to the cent"
    );
    assert_eq!((full.account.as_str(), full.account_id, full.free.as_str()), ("Main account", 1, "0.00"));
    assert_eq!(create(&w.conn, &NewStash { notes: Some("   ".into()), ..new_stash("Bike", 1) }, TODAY).unwrap().notes, None, "empty notes mean none");
    assert_eq!(names(&w.conn), ["Holiday", "Laptop", "Bike"]);
    assert_eq!(text(&w.conn, "SELECT group_concat(position) FROM (SELECT position FROM stashes ORDER BY id)"), "0,1,2", "the first one is at 0");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'stashes' AND action = 'INSERT'"), 3);
    assert_eq!(get(&w.conn, 1, TODAY).unwrap().entries, []);
}

#[test]
fn a_stash_with_a_bad_name_target_date_or_notes_is_not_made() {
    let w = world();
    let bad = [
        ("empty name", new_stash("", 1)),
        ("blank name", new_stash("   ", 1)),
        ("long name", new_stash(&"x".repeat(51), 1)),
        ("control character in the name", new_stash("Hol\niday", 1)),
        ("target of zero", NewStash { target: Some("0.00".into()), ..new_stash("A", 1) }),
        ("negative target", NewStash { target: Some("-1.00".into()), ..new_stash("A", 1) }),
        ("target with one decimal", NewStash { target: Some("1.5".into()), ..new_stash("A", 1) }),
        ("target without decimals", NewStash { target: Some("12".into()), ..new_stash("A", 1) }),
        ("target beyond the largest", NewStash { target: Some("10000000000.00".into()), ..new_stash("A", 1) }),
        ("empty target", NewStash { target: Some("".into()), ..new_stash("A", 1) }),
        ("target date that is not real", NewStash { target_date: Some("2026-02-30".into()), ..new_stash("A", 1) }),
        ("target date not padded", NewStash { target_date: Some("2026-2-3".into()), ..new_stash("A", 1) }),
        ("empty target date", NewStash { target_date: Some("".into()), ..new_stash("A", 1) }),
        ("long notes", NewStash { notes: Some("x".repeat(1001)), ..new_stash("A", 1) }),
        ("control character in the notes", NewStash { notes: Some("a\tb".into()), ..new_stash("A", 1) }),
    ];
    for (what, input) in bad {
        assert_refused(&w.conn, INVALID, what, || create(&w.conn, &input, TODAY));
    }
    assert_refused(&w.conn, INVALID, "a today that is not a date", || create(&w.conn, &new_stash("A", 1), "today"));
    assert_eq!(names(&w.conn), Vec::<String>::new());
    // the bounds are in
    let ok = create(
        &w.conn,
        &NewStash { target: Some("9999999999.99".into()), target_date: Some("2020-02-29".into()), notes: Some("x".repeat(1000)), ..new_stash(&"x".repeat(50), 1) },
        TODAY,
    )
    .unwrap();
    assert_eq!((ok.target.as_deref(), ok.target_date.as_deref(), ok.overdue), (Some("9999999999.99"), Some("2020-02-29"), true), "a past date is allowed: the stash is overdue");
    assert!(create(&w.conn, &NewStash { target: Some("0.01".into()), ..new_stash("Cent", 1) }, TODAY).is_ok());
    // line breaks stay in the notes
    assert_eq!(create(&w.conn, &NewStash { notes: Some("a\nb".into()), ..new_stash("Lines", 1) }, TODAY).unwrap().notes.as_deref(), Some("a\nb"));
}

#[test]
fn item_02_a_stash_with_no_account_an_unknown_one_or_an_archived_one_is_refused_and_with_one_account_it_is_allowed() {
    let w = world();
    // no account: the field is always there, so a request without it does not even reach the core
    assert!(serde_json::from_value::<NewStash>(json!({ "name": "A" })).is_err());
    assert!(serde_json::from_value::<NewStash>(json!({ "name": "A", "accountId": "1", "extra": 1 })).is_err(), "unknown fields are refused");
    for (what, account) in [("empty", "".to_string()), ("unknown", "99".to_string()), ("not an id", "x".to_string()), ("zero", "0".to_string()), ("negative", "-1".to_string()), ("spaced", " 1".to_string())] {
        assert_refused(&w.conn, INVALID, what, || create(&w.conn, &NewStash { account_id: account.clone(), ..new_stash("A", 1) }, TODAY));
    }
    let old = archived_account(&w.conn, "Old");
    assert_refused(&w.conn, INVALID, "an archived account", || create(&w.conn, &new_stash("A", old), TODAY));
    // with the main account alone, the stash is made in it
    let w = world();
    assert_eq!(create(&w.conn, &new_stash("A", 1), TODAY).unwrap().account, "Main account");
}

#[test]
fn a_name_taken_by_an_active_stash_is_refused_ignoring_case_and_a_name_of_an_archived_one_is_free() {
    let w = world();
    stash(&w.conn, "Holiday", 1);
    for taken in ["Holiday", "holiday", "HOLIDAY", "  Holiday "] {
        assert_refused(&w.conn, NAME_EXISTS, taken, || create(&w.conn, &new_stash(taken, 1), TODAY));
    }
    // the check is among all accounts: a stash is not tied to a place by its name
    let cash = account(&w.conn, "Cash");
    assert_refused(&w.conn, NAME_EXISTS, "another account", || create(&w.conn, &new_stash("Holiday", cash), TODAY));
    assert!(create(&w.conn, &new_stash("Holidays", 1), TODAY).is_ok());
    let id = list(&w.conn, None, TODAY).unwrap()[0].id;
    archive(&w.conn, id, TODAY).unwrap();
    assert!(create(&w.conn, &new_stash("Holiday", 1), TODAY).is_ok(), "an archived stash does not hold its name");
    assert_eq!(names(&w.conn), ["Holidays", "Holiday", "Holiday"], "the archived one last");
}

// ---- ACTIVE: update --------------------------------------------------------------------------

#[test]
fn an_update_writes_what_it_names_and_clears_with_null_and_a_new_stamp_comes_only_with_a_change() {
    let w = world();
    let id = create(
        &w.conn,
        &NewStash { target: Some("500.00".into()), target_date: Some("2027-01-31".into()), notes: Some("first".into()), ..new_stash("Holiday", 1) },
        TODAY,
    )
    .unwrap()
    .id;
    let patch = StashPatch { name: Some("  Trip ".into()), target: Some(Some("800.00".into())), target_date: Some(Some("2027-03-01".into())), notes: Some(Some("second".into())), ..Default::default() };
    let changed = update(&w.conn, id, &patch, TODAY).unwrap();
    assert_eq!((changed.name.as_str(), changed.target.as_deref(), changed.target_date.as_deref(), changed.notes.as_deref()), ("Trip", Some("800.00"), Some("2027-03-01"), Some("second")));
    let stamp = stamp_of(&w.conn, "stashes", id);
    // the same values again: nothing is written, no stamp, no history, no tick
    let again = assert_nothing_written(&w.conn, "the same values", || update(&w.conn, id, &patch, TODAY));
    assert_eq!(again, changed);
    assert_nothing_written(&w.conn, "an empty patch", || update(&w.conn, id, &StashPatch::default(), TODAY));
    assert_eq!(stamp_of(&w.conn, "stashes", id), stamp);
    // each field alone is a change
    for patch in [rename("Journey"), StashPatch { target: Some(Some("900.00".into())), ..Default::default() }, StashPatch { target_date: Some(Some("2027-04-01".into())), ..Default::default() }, StashPatch { notes: Some(Some("third".into())), ..Default::default() }] {
        let before = stamp_of(&w.conn, "stashes", id);
        update(&w.conn, id, &patch, TODAY).unwrap();
        assert!(stamp_of(&w.conn, "stashes", id) > before, "{patch:?}");
    }
    // `null` clears the three that can be cleared; a name cannot be
    let cleared = update(&w.conn, id, &StashPatch { target: Some(None), target_date: Some(None), notes: Some(None), ..Default::default() }, TODAY).unwrap();
    assert_eq!((cleared.target, cleared.target_date, cleared.notes, cleared.left, cleared.percent, cleared.per_month), (None, None, None, None, None, None));
    assert_eq!(serde_json::from_value::<StashPatch>(json!({ "target": null })).unwrap().target, Some(None), "null is not the same as left out");
    assert_eq!(serde_json::from_value::<StashPatch>(json!({})).unwrap().target, None);
    assert!(serde_json::from_value::<StashPatch>(json!({ "kind": "x" })).is_err());
}

#[test]
fn an_update_with_a_bad_value_is_refused_and_writes_nothing() {
    let w = world();
    let id = aimed(&w.conn, "Holiday", 1, "500.00");
    stash(&w.conn, "Other", 1);
    let bad = [
        ("empty name", rename("")),
        ("long name", rename(&"x".repeat(51))),
        ("control character in the name", rename("a\u{7}b")),
        ("target of zero", StashPatch { target: Some(Some("0.00".into())), ..Default::default() }),
        ("target without decimals", StashPatch { target: Some(Some("5".into())), ..Default::default() }),
        ("target beyond the largest", StashPatch { target: Some(Some("10000000000.00".into())), ..Default::default() }),
        ("target date that is not real", StashPatch { target_date: Some(Some("2026-02-30".into())), ..Default::default() }),
        ("long notes", StashPatch { notes: Some(Some("x".repeat(1001))), ..Default::default() }),
        ("an account that is not an id", StashPatch { account_id: Some("x".into()), ..Default::default() }),
        ("an unknown account", move_to(99)),
    ];
    for (what, patch) in bad {
        assert_refused(&w.conn, INVALID, what, || update(&w.conn, id, &patch, TODAY));
    }
    assert_refused(&w.conn, NAME_EXISTS, "a name an active stash has", || update(&w.conn, id, &rename("other"), TODAY));
    assert_refused(&w.conn, NOT_FOUND, "a stash that is not there", || update(&w.conn, 99, &rename("A"), TODAY));
    // a refusal in one field stops the others too
    assert_refused(&w.conn, INVALID, "a good name with a bad target", || update(&w.conn, id, &StashPatch { name: Some("Fine".into()), target: Some(Some("0.00".into())), ..Default::default() }, TODAY));
    // renaming to its own name, in another case, is not a clash
    assert_eq!(update(&w.conn, id, &rename("HOLIDAY"), TODAY).unwrap().name, "HOLIDAY");
}

#[test]
fn item_08_lowering_the_target_below_saved_is_refused_to_exactly_saved_allowed_and_clearing_it_is_always_allowed() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = aimed(&w.conn, "Holiday", acc, "500.00");
    put(&w.conn, id, "200.00");
    let lower = |target: &str| update(&w.conn, id, &StashPatch { target: Some(Some(target.into())), ..Default::default() }, TODAY);
    assert_refused(&w.conn, INVALID, "one cent below saved", || lower("199.99"));
    assert_refused(&w.conn, INVALID, "far below saved", || lower("1.00"));
    let exact = lower("200.00").unwrap();
    assert!(exact.reached && exact.percent == Some(100) && exact.left.as_deref() == Some("0.00"), "exactly saved is allowed, and the stash is reached: {exact:?}");
    assert!(lower("200.01").is_ok(), "above saved is allowed");
    let cleared = update(&w.conn, id, &StashPatch { target: Some(None), ..Default::default() }, TODAY).unwrap();
    assert_eq!((cleared.target, cleared.reached, cleared.percent), (None, false, None));
    // the stash that holds more than its target after a sync can still be renamed and re-dated: only a new target is checked
    let id = aimed(&w.conn, "Laptop", acc, "100.00");
    raw(&w.conn, id, 30_000, "2026-01-01");
    assert_eq!(shown(&w.conn, id).percent, Some(300));
    assert!(update(&w.conn, id, &rename("Laptop 2"), TODAY).is_ok());
    assert!(update(&w.conn, id, &StashPatch { target: Some(Some("100.00".into())), ..Default::default() }, TODAY).is_ok(), "the target it has is not a new target");
    assert_refused(&w.conn, INVALID, "a lower one than saved", || update(&w.conn, id, &StashPatch { target: Some(Some("299.99".into())), ..Default::default() }, TODAY));
    // a stash below 0 takes any target
    let id = aimed(&w.conn, "Gap", acc, "100.00");
    raw(&w.conn, id, -500, "2026-01-01");
    assert!(update(&w.conn, id, &StashPatch { target: Some(Some("0.01".into())), ..Default::default() }, TODAY).is_ok());
}

#[test]
fn updating_with_the_account_it_has_is_not_a_move_nothing_is_checked_even_when_that_account_is_archived() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = stash(&w.conn, "Holiday", acc);
    put(&w.conn, id, "300.00");
    accounts::archive(&w.conn, acc, TODAY).unwrap();
    let same = StashPatch { account_id: Some(acc.to_string()), ..Default::default() };
    assert_nothing_written(&w.conn, "the account it has, archived", || update(&w.conn, id, &same, TODAY));
    let renamed = update(&w.conn, id, &StashPatch { name: Some("Trip".into()), account_id: Some(acc.to_string()), ..Default::default() }, TODAY).unwrap();
    assert_eq!((renamed.name.as_str(), renamed.account_id, renamed.account_archived), ("Trip", acc, true), "the other fields are written, the stash stays where it is");
    // the main account's number, as a string with the same value
    let main = stash(&w.conn, "Main one", 1);
    assert_nothing_written(&w.conn, "main", || update(&w.conn, main, &StashPatch { account_id: Some("1".into()), ..Default::default() }, TODAY));
}

// ---- ACTIVE: move ----------------------------------------------------------------------------

#[test]
fn item_09_a_stash_moves_to_an_active_account_of_its_currency_where_what_it_holds_is_free() {
    let w = world();
    let a = funded(&w.conn, "A", "1000.00");
    let b = funded(&w.conn, "B", "500.00");
    let x = stash(&w.conn, "X", a);
    let y = stash(&w.conn, "Y", a);
    put(&w.conn, x, "300.00");
    put(&w.conn, y, "200.00");
    assert_eq!((free_of(&w.conn, a), free_of(&w.conn, b)), (50_000, 50_000));

    // another currency: refused
    let lei = foreign(&w.conn, "Lei", "RON", "9000.00");
    assert_refused(&w.conn, INVALID, "another currency", || update(&w.conn, x, &move_to(lei), TODAY));
    // an archived account: refused; an unknown one too
    let old = archived_account(&w.conn, "Old");
    assert_refused(&w.conn, INVALID, "an archived account", || update(&w.conn, x, &move_to(old), TODAY));
    assert_refused(&w.conn, INVALID, "an unknown account", || update(&w.conn, x, &move_to(99), TODAY));
    // too little free there: one cent short is refused, exactly enough is allowed
    let short = funded(&w.conn, "Short", "299.99");
    assert_refused(&w.conn, INVALID, "one cent short", || update(&w.conn, x, &move_to(short), TODAY));
    let exact = funded(&w.conn, "Exact", "300.00");
    let moved = update(&w.conn, x, &move_to(exact), TODAY).unwrap();
    assert_eq!((moved.account_id, moved.account.as_str(), moved.saved.as_str(), moved.free.as_str()), (exact, "Exact", "300.00", "0.00"));
    assert_eq!((free_of(&w.conn, a), free_of(&w.conn, exact)), (80_000, 0), "what it held is free again where it was, and set aside where it went");
    assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM stash_entries WHERE stash_id = {x}")), 1, "its entries went with it");
    // the move is a change of the row, with a newer stamp and a row of history
    assert_eq!(one(&w.conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'stashes' AND action = 'UPDATE'"), 1);
    // a stash that holds 0 moves to any active account of its currency, even one with nothing free
    let empty = stash(&w.conn, "Empty", a);
    let poor = account(&w.conn, "Poor");
    assert_eq!(update(&w.conn, empty, &move_to(poor), TODAY).unwrap().account_id, poor);
    // so does one that holds less than 0 after a sync: it takes nothing away from an account
    let gap = stash(&w.conn, "Gap", a);
    raw(&w.conn, gap, -700, "2026-01-01");
    assert_eq!(update(&w.conn, gap, &move_to(poor), TODAY).unwrap().account_id, poor);
    // and one that holds 0 moves even into an account whose stashes hold more than it has (free below 0 after a sync)
    let over = funded(&w.conn, "Over", "100.00");
    let full = stash(&w.conn, "Full", over);
    raw(&w.conn, full, 20_000, "2026-01-01");
    assert_eq!(free_of(&w.conn, over), -10_000);
    assert_eq!(update(&w.conn, empty, &move_to(over), TODAY).unwrap().account_id, over);
    assert_refused(&w.conn, INVALID, "a stash that holds money does not fit below 0", || update(&w.conn, x, &move_to(over), TODAY));
    // the account it has: not a move, allowed (also in the same patch as another change)
    assert_nothing_written(&w.conn, "the account it has", || update(&w.conn, y, &move_to(a), TODAY));
    // an archived stash is not moved at all (restore it first)
    archive(&w.conn, y, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "an archived stash", || update(&w.conn, y, &move_to(b), TODAY));
    // a stash in an archived account can be moved out of it
    let stuck = stash(&w.conn, "Stuck", b);
    put(&w.conn, stuck, "100.00");
    accounts::archive(&w.conn, b, TODAY).unwrap();
    assert_eq!(update(&w.conn, stuck, &move_to(a), TODAY).unwrap().account_id, a);
}

#[test]
fn a_move_counts_the_money_of_the_stashes_already_in_the_account_it_goes_to() {
    let w = world();
    let a = funded(&w.conn, "A", "1000.00");
    let b = funded(&w.conn, "B", "400.00");
    let (x, y) = (stash(&w.conn, "X", a), stash(&w.conn, "Y", b));
    put(&w.conn, x, "250.00");
    put(&w.conn, y, "200.00");
    assert_eq!(free_of(&w.conn, b), 20_000);
    assert_refused(&w.conn, INVALID, "250 does not fit in 200", || update(&w.conn, x, &move_to(b), TODAY));
    // an archived stash in B holds its money too
    archive(&w.conn, y, TODAY).unwrap();
    assert_eq!(free_of(&w.conn, b), 20_000);
    assert_refused(&w.conn, INVALID, "still 200 free", || update(&w.conn, x, &move_to(b), TODAY));
    // a move together with a rename: the whole patch is refused or applied
    assert_refused(&w.conn, INVALID, "rename and a move that does not fit", || {
        update(&w.conn, x, &StashPatch { name: Some("Renamed".into()), account_id: Some(b.to_string()), ..Default::default() }, TODAY)
    });
    assert_eq!(names(&w.conn), ["X", "Y"]);
}

// ---- reorder ---------------------------------------------------------------------------------

#[test]
fn stashes_are_reordered_and_only_the_rows_that_move_are_written() {
    let w = world();
    let (a, b, c) = (stash(&w.conn, "A", 1), stash(&w.conn, "B", 1), stash(&w.conn, "C", 1));
    let stamps = |ids: &[i64]| ids.iter().map(|id| stamp_of(&w.conn, "stashes", *id)).collect::<Vec<_>>();
    let before = stamps(&[a, b, c]);
    reorder(&w.conn, &[a, c, b]).unwrap();
    assert_eq!(names(&w.conn), ["A", "C", "B"]);
    let after = stamps(&[a, b, c]);
    assert_eq!(after[0], before[0], "A did not move: no new stamp");
    assert!(after[1] > before[1] && after[2] > before[2], "B and C moved");
    assert_nothing_written(&w.conn, "the same order again", || reorder(&w.conn, &[a, c, b]));
    reorder(&w.conn, &[c, b, a]).unwrap();
    assert_eq!(names(&w.conn), ["C", "B", "A"]);
    assert_eq!(text(&w.conn, "SELECT group_concat(position) FROM (SELECT position FROM stashes ORDER BY id)"), "2,1,0");
    // the lists of both kinds of account show the same order
    assert_eq!(list(&w.conn, Some(1), TODAY).unwrap().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["C", "B", "A"]);
}

#[test]
fn item_17_a_reorder_with_a_missing_a_repeated_an_archived_or_an_unknown_id_is_refused_and_writes_nothing() {
    let w = world();
    let (a, b, c) = (stash(&w.conn, "A", 1), stash(&w.conn, "B", 1), stash(&w.conn, "C", 1));
    let old = stash(&w.conn, "Old", 1);
    archive(&w.conn, old, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "a missing id", || reorder(&w.conn, &[a, b]));
    assert_refused(&w.conn, INVALID, "no ids", || reorder(&w.conn, &[]));
    assert_refused(&w.conn, INVALID, "a repeated id", || reorder(&w.conn, &[a, b, b]));
    assert_refused(&w.conn, INVALID, "a repeated id with the right count", || reorder(&w.conn, &[a, a, b]));
    assert_refused(&w.conn, INVALID, "an archived id", || reorder(&w.conn, &[a, b, c, old]));
    assert_refused(&w.conn, INVALID, "an archived id in place of an active one", || reorder(&w.conn, &[a, b, old]));
    assert_refused(&w.conn, INVALID, "an unknown id", || reorder(&w.conn, &[a, b, 99]));
    assert_refused(&w.conn, INVALID, "an extra unknown id", || reorder(&w.conn, &[a, b, c, 99]));
    assert_eq!(names(&w.conn), ["A", "B", "C", "Old"]);
    assert!(reorder(&w.conn, &[b, c, a]).is_ok());
}

// ---- archive and restore ---------------------------------------------------------------------

#[test]
fn item_01_archive_twice_and_restore_one_never_archived_answer_the_stash_and_make_no_stamp() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = stash(&w.conn, "Holiday", acc);
    put(&w.conn, id, "250.00");
    let active = shown(&w.conn, id);
    assert_eq!(assert_nothing_written(&w.conn, "restore one never archived", || restore(&w.conn, id, TODAY)), active);

    let archived = archive(&w.conn, id, TODAY).unwrap();
    assert!(archived.archived);
    assert_eq!((archived.saved.as_str(), archived.entries, archived.free.as_str()), ("250.00", 1, "750.00"), "the entries stay and what it holds stays set aside");
    assert_eq!(assert_nothing_written(&w.conn, "archive twice", || archive(&w.conn, id, TODAY)), archived);

    let back = restore(&w.conn, id, TODAY).unwrap();
    assert_eq!(back, active);
    assert_eq!(assert_nothing_written(&w.conn, "restore twice", || restore(&w.conn, id, TODAY)), back);
    assert_refused(&w.conn, NOT_FOUND, "archive a stash that is not there", || archive(&w.conn, 99, TODAY));
    assert_refused(&w.conn, NOT_FOUND, "restore a stash that is not there", || restore(&w.conn, 99, TODAY));
}

#[test]
fn an_archived_stash_keeps_its_entries_and_its_money_set_aside_and_cannot_be_updated_or_reordered() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let (id, other) = (stash(&w.conn, "Holiday", acc), stash(&w.conn, "Other", acc));
    put(&w.conn, id, "300.00");
    archive(&w.conn, id, TODAY).unwrap();
    assert_eq!(free_of(&w.conn, acc), 70_000, "archived stashes still hold their money");
    assert_eq!(accounts::get(&w.conn, acc, TODAY).unwrap().set_aside, "300.00");
    assert_refused(&w.conn, INVALID, "update an archived stash", || update(&w.conn, id, &rename("Trip"), TODAY));
    assert_refused(&w.conn, INVALID, "update it with nothing", || update(&w.conn, id, &StashPatch::default(), TODAY));
    assert_refused(&w.conn, INVALID, "reorder with it", || reorder(&w.conn, &[id, other]));
    assert_refused(&w.conn, INVALID, "reorder without it that leaves one out", || reorder(&w.conn, &[]));
    assert!(reorder(&w.conn, &[other]).is_ok());
    // it is listed after the active ones, whatever its position
    assert_eq!(names(&w.conn), ["Other", "Holiday"]);
    assert_eq!(list(&w.conn, None, TODAY).unwrap().iter().map(|s| s.archived).collect::<Vec<_>>(), [false, true]);
}

#[test]
fn item_16_a_name_taken_by_an_active_stash_is_refused_by_an_archived_one_allowed_and_a_restore_checks_it_again() {
    let w = world();
    let first = stash(&w.conn, "Holiday", 1);
    assert_refused(&w.conn, NAME_EXISTS, "a name an active stash has", || create(&w.conn, &new_stash("holiday", 1), TODAY));
    archive(&w.conn, first, TODAY).unwrap();
    let second = create(&w.conn, &new_stash("Holiday", 1), TODAY).unwrap().id;
    // the name was taken meanwhile: the archived one does not come back
    assert_refused(&w.conn, NAME_EXISTS, "restore an archived stash whose name was taken", || restore(&w.conn, first, TODAY));
    assert!(shown(&w.conn, first).archived);
    // after the other is renamed, or archived, it does
    update(&w.conn, second, &rename("Holiday 2"), TODAY).unwrap();
    assert!(!restore(&w.conn, first, TODAY).unwrap().archived);
    // an update to a name that only an archived stash has is allowed
    archive(&w.conn, second, TODAY).unwrap();
    let third = stash(&w.conn, "Third", 1);
    assert_eq!(update(&w.conn, third, &rename("holiday 2"), TODAY).unwrap().name, "holiday 2");
    assert_refused(&w.conn, NAME_EXISTS, "and now the archived one's name is taken", || restore(&w.conn, second, TODAY));
}

// ---- delete ----------------------------------------------------------------------------------

#[test]
fn deleting_a_stash_removes_its_entries_and_makes_one_marker_and_the_transfers_stay() {
    let w = world();
    let savings = funded(&w.conn, "Savings", "0.00");
    let cash = funded(&w.conn, "Cash", "1000.00");
    let id = stash(&w.conn, "Holiday", savings);
    let other = stash(&w.conn, "Other", cash);
    super::entries::add(&w.conn, id, &super::entries::NewEntry { from_account_id: Some(cash.to_string()), ..new_entry("100.00") }, TODAY).unwrap();
    put(&w.conn, other, "50.00");
    pull(&w.conn, id, "10.00");
    let transfers = one(&w.conn, "SELECT count(*) FROM transfers");
    assert_eq!(transfers, 1);
    let sync_id = text(&w.conn, &format!("SELECT sync_id FROM stashes WHERE id = {id}"));
    delete(&w.conn, id).unwrap();
    assert_eq!(names(&w.conn), ["Other"]);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM stash_entries"), 1, "the entries of the other stash are not touched");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transfers"), transfers, "the transfer stays");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 1, "one marker, for the stash");
    assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM sync_deleted WHERE kind = 'stashes' AND sync_id = '{sync_id}'")), 1);
    assert_eq!(free_of(&w.conn, savings), 10_000, "what it held is free again: the 100.00 that came in less the 10.00 that is gone");
    assert_refused(&w.conn, NOT_FOUND, "delete again", || delete(&w.conn, id));
    assert_refused(&w.conn, NOT_FOUND, "an unknown id", || delete(&w.conn, 99));
    // an archived stash can be deleted too
    archive(&w.conn, other, TODAY).unwrap();
    delete(&w.conn, other).unwrap();
    assert_eq!(names(&w.conn), Vec::<String>::new());
    assert_eq!(one(&w.conn, "SELECT count(*) FROM stash_entries"), 0);
}

#[test]
fn item_12_an_account_with_a_stash_is_not_deleted_and_after_the_stash_is_moved_or_deleted_it_is() {
    let w = world();
    let a = account(&w.conn, "A");
    let b = account(&w.conn, "B");
    let id = stash(&w.conn, "Holiday", a);
    assert_eq!(accounts::get(&w.conn, a, TODAY).unwrap().entries, 1, "a stash counts in the rows of an account");
    assert_refused(&w.conn, INVALID, "an account with a stash", || accounts::delete(&w.conn, a));
    // archived or not: an archived stash points at its account as much as another
    archive(&w.conn, id, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "an account with an archived stash", || accounts::delete(&w.conn, a));
    restore(&w.conn, id, TODAY).unwrap();
    // moved away: allowed
    update(&w.conn, id, &move_to(b), TODAY).unwrap();
    assert_eq!((accounts::get(&w.conn, a, TODAY).unwrap().entries, accounts::get(&w.conn, b, TODAY).unwrap().entries), (0, 1));
    accounts::delete(&w.conn, a).unwrap();
    // deleted: allowed
    assert_refused(&w.conn, INVALID, "B has the stash", || accounts::delete(&w.conn, b));
    delete(&w.conn, id).unwrap();
    accounts::delete(&w.conn, b).unwrap();
    // an archived account with a stash is refused as well
    let c = account(&w.conn, "C");
    stash(&w.conn, "Trip", c);
    accounts::archive(&w.conn, c, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "an archived account with a stash", || accounts::delete(&w.conn, c));
    // and the foreign key is the net under the refusal, whoever writes
    assert!(w.conn.execute(&format!("DELETE FROM accounts WHERE id = {c}"), []).is_err());
}

// ---- NONE, DELETED: any event by id ------------------------------------------------------------

#[test]
fn any_event_on_a_stash_that_is_not_there_answers_not_found_and_writes_nothing() {
    let w = world();
    let id = stash(&w.conn, "Gone", 1);
    delete(&w.conn, id).unwrap();
    for missing in [id, 99] {
        assert_refused(&w.conn, NOT_FOUND, "get", || get(&w.conn, missing, TODAY));
        assert_refused(&w.conn, NOT_FOUND, "update", || update(&w.conn, missing, &rename("A"), TODAY));
        assert_refused(&w.conn, NOT_FOUND, "archive", || archive(&w.conn, missing, TODAY));
        assert_refused(&w.conn, NOT_FOUND, "restore", || restore(&w.conn, missing, TODAY));
        assert_refused(&w.conn, NOT_FOUND, "delete", || delete(&w.conn, missing));
        assert_refused(&w.conn, NOT_FOUND, "history", || history(&w.conn, missing, "2026", TODAY));
        assert_refused(&w.conn, NOT_FOUND, "add", || super::entries::add(&w.conn, missing, &new_entry("1.00"), TODAY));
        assert_refused(&w.conn, NOT_FOUND, "take", || super::entries::take(&w.conn, missing, &new_take("1.00"), TODAY));
        assert_refused(&w.conn, NOT_FOUND, "edit an entry", || super::entries::update(&w.conn, missing, &Default::default(), TODAY));
        assert_refused(&w.conn, NOT_FOUND, "delete an entry", || super::entries::delete(&w.conn, missing, false, TODAY));
    }
}

// ---- its account is archived -------------------------------------------------------------------

#[test]
fn a_stash_in_an_archived_account_is_shown_as_such_is_still_changed_and_is_written_by_nothing_the_archive_does() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = aimed(&w.conn, "Holiday", acc, "500.00");
    put(&w.conn, id, "100.00");
    let stamp = stamp_of(&w.conn, "stashes", id);
    accounts::archive(&w.conn, acc, TODAY).unwrap();
    assert_eq!(stamp_of(&w.conn, "stashes", id), stamp, "archiving the account writes nothing of the stash");
    let shown_now = shown(&w.conn, id);
    assert!(shown_now.account_archived && !shown_now.archived);
    assert_eq!(shown_now.free, "900.00");
    assert!(update(&w.conn, id, &rename("Trip"), TODAY).is_ok(), "it can still be changed");
    assert!(archive(&w.conn, id, TODAY).is_ok());
    accounts::restore(&w.conn, acc, TODAY).unwrap();
    assert!(stamp_of(&w.conn, "stashes", id) > stamp);
}

// ---- lists -----------------------------------------------------------------------------------

#[test]
fn the_list_is_in_the_persons_order_the_archived_last_and_can_be_cut_to_one_account() {
    let w = world();
    let a = account(&w.conn, "A");
    let b = account(&w.conn, "B");
    let (x, y, z, old) = (stash(&w.conn, "X", a), stash(&w.conn, "Y", b), stash(&w.conn, "Z", a), stash(&w.conn, "Old", a));
    archive(&w.conn, old, TODAY).unwrap();
    assert_eq!(names(&w.conn), ["X", "Y", "Z", "Old"]);
    assert_eq!(list(&w.conn, Some(a), TODAY).unwrap().iter().map(|s| s.id).collect::<Vec<_>>(), [x, z, old]);
    assert_eq!(list(&w.conn, Some(b), TODAY).unwrap().iter().map(|s| s.id).collect::<Vec<_>>(), [y]);
    assert!(list(&w.conn, Some(1), TODAY).unwrap().is_empty());
    assert!(list(&w.conn, Some(99), TODAY).unwrap().is_empty(), "an account that is not there has no stashes");
    reorder(&w.conn, &[z, y, x]).unwrap();
    assert_eq!(names(&w.conn), ["Z", "Y", "X", "Old"]);
    assert!(list(&w.conn, None, "2026-13-01").is_err());
    assert_eq!(accounts::list(&w.conn, TODAY).unwrap().iter().map(|a| a.entries).collect::<Vec<_>>(), [0, 3, 1]);
    // a stash of an account that changes AccountPatch is untouched
    accounts::update(&w.conn, a, &AccountPatch { name: Some("Renamed".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!(shown(&w.conn, x).account, "Renamed");
}
