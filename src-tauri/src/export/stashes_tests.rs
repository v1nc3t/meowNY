//! Stashes in the exports: all of them with their entries in a file with no limit, none with one, what each account has set aside and no `free`,
//! and the counts of the preview (`docs/stash-design.md`, section 10). The Excel sheets are checked in `xlsx.rs`, with the reader it has.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use super::accounts_tests::{case, file, foreign_case, now, Case, TODAY};
use super::tests::{lived_in, read_csv};
use super::*;
use crate::accounts::fixtures::snapshot;
use crate::accounts::{self, MAIN_ID};
use crate::stashes::entries::{self, NewEntry, NewTake};
use crate::stashes::{self, NewStash};

/// Notes with a leading `=`, a line break, a comma and quotes: what a spreadsheet and a CSV reader trip on. The JSON holds them as they are.
pub(super) const NOTES: &str = "=1+1 flights\nand a hotel, \"small\"";

pub(super) fn make(conn: &Connection, name: &str, account: i64, target: Option<&str>, date: Option<&str>, notes: Option<&str>) -> i64 {
    let input = NewStash { name: name.into(), account_id: account.to_string(), target: target.map(str::to_string), target_date: date.map(str::to_string), notes: notes.map(str::to_string) };
    stashes::create(conn, &input, TODAY).unwrap().id
}

/// Adds money on a day, with a transfer from `from` when it is given. Returns the id of the entry.
pub(super) fn add(conn: &Connection, stash: i64, amount: &str, date: &str, note: Option<&str>, from: Option<i64>) -> i64 {
    let input = NewEntry { amount: amount.into(), date: Some(date.into()), note: note.map(str::to_string), from_account_id: from.map(|id| id.to_string()) };
    entries::add(conn, stash, &input, TODAY).unwrap();
    conn.query_row("SELECT MAX(id) FROM stash_entries", [], |r| r.get(0)).unwrap()
}

/// Takes money out on a day, with a transfer to `to` when it is given. Returns the id of the entry.
pub(super) fn take(conn: &Connection, stash: i64, amount: &str, date: &str, note: Option<&str>, to: Option<i64>) -> i64 {
    let input = NewTake { amount: amount.into(), date: Some(date.into()), note: note.map(str::to_string), to_account_id: to.map(|id| id.to_string()) };
    entries::take(conn, stash, &input, TODAY).unwrap();
    conn.query_row("SELECT MAX(id) FROM stash_entries", [], |r| r.get(0)).unwrap()
}

pub(super) struct Stashed {
    pub(super) c: Case,
    pub(super) holiday: i64,
    pub(super) rainy: i64,
    pub(super) laptop: i64,
    pub(super) bike: i64,
    /// The entries of the holiday, in the order they were made: an add with a transfer, an add on the same day, and a take with a transfer.
    pub(super) holiday_entries: [i64; 3],
}

/// `case()` (Main account 1800.00, Savings 1700.00, Cash 150.00, Credit card -945.50, Old savings 200.00, archived) with four stashes, listed in this
/// order after a reorder: New laptop (in the main account, target 900.00, reached with one add), Holiday in Lisbon (Savings, target 1200.00 by
/// 2027-06-30, notes `NOTES`; an add of 100.00 on 2026-03-01 that came with a transfer from the main account, another of 50.00 the same day, and a
/// take of 30.00 on 2026-04-01 that went to Cash with a transfer: 120.00 held), Rainy day (Savings, no target, one add of 200.00) and, archived, Bike
/// (Savings, target 450.00, an add of 450.00 and a take of 450.00 that says "bought it": 0.00 held). So Main has 900.00 set aside and 800.00 free
/// after the 100.00 that left it, Savings 320.00 set aside and 1450.00 free, and the others none.
pub(super) fn stashed() -> Stashed {
    let c = case();
    let savings = c.savings;
    let holiday = make(&c.conn, "Holiday in Lisbon", savings, Some("1200.00"), Some("2027-06-30"), Some(NOTES));
    let first = add(&c.conn, holiday, "100.00", "2026-03-01", Some("start"), Some(MAIN_ID));
    let second = add(&c.conn, holiday, "50.00", "2026-03-01", None, None);
    let third = take(&c.conn, holiday, "30.00", "2026-04-01", Some("deposit for the flat"), Some(c.cash));
    let rainy = make(&c.conn, "Rainy day", savings, None, None, None);
    add(&c.conn, rainy, "200.00", "2026-02-20", None, None);
    let laptop = make(&c.conn, "New laptop", MAIN_ID, Some("900.00"), None, None);
    add(&c.conn, laptop, "900.00", "2026-05-01", None, None);
    let bike = make(&c.conn, "Bike", savings, Some("450.00"), None, None);
    add(&c.conn, bike, "450.00", "2026-01-15", None, None);
    take(&c.conn, bike, "450.00", "2026-02-10", Some("bought it"), None);
    stashes::archive(&c.conn, bike, TODAY).unwrap();
    stashes::reorder(&c.conn, &[laptop, holiday, rainy]).unwrap();
    Stashed { c, holiday, rainy, laptop, bike, holiday_entries: [first, second, third] }
}

fn names(list: &Value) -> Vec<&str> {
    list.as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect()
}

fn keys(value: &Value) -> BTreeSet<&str> {
    value.as_object().unwrap().keys().map(String::as_str).collect()
}

fn transfer_of(conn: &Connection, entry: i64) -> String {
    conn.query_row("SELECT transfer_id FROM stash_entries WHERE id = ?1", [entry], |r| r.get::<_, i64>(0)).unwrap().to_string()
}

#[test]
fn the_json_has_every_stash_with_its_account_target_and_what_it_holds_and_its_entries_oldest_first() {
    let s = stashed();
    let file = file(&s.c.conn, &Selection::default());
    let list = &file["stashes"];
    assert_eq!(names(list), ["New laptop", "Holiday in Lisbon", "Rainy day", "Bike"], "in the person's order, the archived one last");
    assert_eq!(
        list[1],
        json!({
            "id": s.holiday.to_string(), "name": "Holiday in Lisbon", "accountId": s.c.savings.to_string(), "account": "Savings", "currency": "EUR",
            "target": "1200.00", "targetDate": "2027-06-30", "notes": NOTES, "archived": false, "saved": "120.00",
            "entries": [
                { "id": s.holiday_entries[0].to_string(), "kind": "ADD", "amount": "100.00", "date": "2026-03-01", "note": "start", "transferId": transfer_of(&s.c.conn, s.holiday_entries[0]) },
                { "id": s.holiday_entries[1].to_string(), "kind": "ADD", "amount": "50.00", "date": "2026-03-01", "note": null, "transferId": null },
                { "id": s.holiday_entries[2].to_string(), "kind": "TAKE", "amount": "30.00", "date": "2026-04-01", "note": "deposit for the flat", "transferId": transfer_of(&s.c.conn, s.holiday_entries[2]) },
            ],
        }),
        "every field, the notes as they are, an entry with a transfer and one without, and a take with an amount above 0"
    );
    // no target, no date and no notes are null, and an account in the app's currency says so
    let rainy = &list[2];
    assert_eq!((&rainy["target"], &rainy["targetDate"], &rainy["notes"], &rainy["saved"], &rainy["currency"]), (&Value::Null, &Value::Null, &Value::Null, &json!("200.00"), &json!("EUR")));
    let laptop = &list[0];
    assert_eq!((laptop["accountId"].as_str(), laptop["account"].as_str(), laptop["target"].as_str(), laptop["saved"].as_str()), (Some("1"), Some("Main account"), Some("900.00"), Some("900.00")));
    // an archived stash is in, with its whole history: reached, and then taken out in full
    let bike = &list[3];
    assert_eq!((bike["id"].as_str(), bike["archived"].as_bool(), bike["saved"].as_str()), (Some(s.bike.to_string().as_str()), Some(true), Some("0.00")));
    let moves: Vec<(&str, &str, &str, Option<&str>)> =
        bike["entries"].as_array().unwrap().iter().map(|e| (e["kind"].as_str().unwrap(), e["amount"].as_str().unwrap(), e["date"].as_str().unwrap(), e["note"].as_str())).collect();
    assert_eq!(moves, [("ADD", "450.00", "2026-01-15", None), ("TAKE", "450.00", "2026-02-10", Some("bought it"))]);
    // the shape of every item: ids are strings, money is a decimal string above 0 in an entry, and nothing else is in it
    let stash_keys: BTreeSet<&str> = ["id", "name", "accountId", "account", "currency", "target", "targetDate", "notes", "archived", "saved", "entries"].into();
    let entry_keys: BTreeSet<&str> = ["id", "kind", "amount", "date", "note", "transferId"].into();
    for stash in list.as_array().unwrap() {
        assert_eq!(keys(stash), stash_keys, "{}", stash["name"]);
        assert!(stash["id"].is_string() && stash["accountId"].is_string() && stash["saved"].as_str().is_some_and(|m| m.contains('.')));
        for entry in stash["entries"].as_array().unwrap() {
            assert_eq!(keys(entry), entry_keys);
            assert!(entry["id"].is_string() && entry["transferId"].is_null() == entry["transferId"].as_str().is_none());
            assert!(entry["amount"].as_str().is_some_and(|m| m.contains('.') && !m.starts_with('-') && m != "0.00"), "{entry}");
            assert!(["ADD", "TAKE"].contains(&entry["kind"].as_str().unwrap()));
        }
    }
    // what each says it holds is the sum of its entries, which is what the screens show
    for (stash, shown) in list.as_array().unwrap().iter().zip(stashes::list(&s.c.conn, None, TODAY).unwrap()) {
        let sum: i64 = stash["entries"].as_array().unwrap().iter().map(|e| crate::money::parse_amount(e["amount"].as_str().unwrap()).unwrap() * if e["kind"] == "ADD" { 1 } else { -1 }).sum();
        assert_eq!((stash["saved"].as_str().unwrap(), format_amount(sum), stash["name"].as_str().unwrap()), (shown.saved.as_str(), shown.saved.clone(), shown.name.as_str()));
    }
}

#[test]
fn an_entry_whose_transfer_was_deleted_has_no_transfer_in_the_file_and_a_stash_below_zero_is_written_as_it_is() {
    let s = stashed();
    let transfer: i64 = transfer_of(&s.c.conn, s.holiday_entries[0]).parse().unwrap();
    accounts::transfers::delete(&s.c.conn, transfer).unwrap();
    // what a sync can leave: more taken out than was put in (`docs/stash-design.md`, section 7.6)
    let overdrawn = make(&s.c.conn, "Overdrawn", s.c.cash, None, None, None);
    crate::stashes::fixtures::raw(&s.c.conn, overdrawn, -500, "2026-05-02");
    let file = file(&s.c.conn, &Selection::default());
    let holiday = file["stashes"].as_array().unwrap().iter().find(|t| t["name"] == "Holiday in Lisbon").unwrap();
    assert_eq!(holiday["entries"][0]["transferId"], Value::Null, "the link was cleared, and the entry is still there");
    assert_eq!(holiday["saved"], "120.00");
    let overdrawn = file["stashes"].as_array().unwrap().iter().find(|t| t["name"] == "Overdrawn").unwrap();
    assert_eq!(overdrawn["saved"], "-5.00");
    assert_eq!((overdrawn["entries"][0]["kind"].as_str(), overdrawn["entries"][0]["amount"].as_str()), (Some("TAKE"), Some("5.00")), "the amount of an entry is a size, whatever the stash holds");
    let cash = file["accounts"].as_array().unwrap().iter().find(|a| a["name"] == "Cash").unwrap();
    assert_eq!(cash["setAside"], "0.00", "a stash below 0 never adds money to an account");
}

#[test]
fn every_account_says_what_its_stashes_hold_and_nothing_says_what_is_free() {
    let s = stashed();
    let file = file(&s.c.conn, &Selection::default());
    let held: Vec<(&str, &str, &str)> =
        file["accounts"].as_array().unwrap().iter().map(|a| (a["name"].as_str().unwrap(), a["balance"].as_str().unwrap(), a["setAside"].as_str().unwrap())).collect();
    assert_eq!(
        held,
        [("Main account", "1700.00", "900.00"), ("Savings", "1770.00", "320.00"), ("Cash", "180.00", "0.00"), ("Credit card", "-945.50", "0.00"), ("Old savings", "200.00", "0.00")],
        "the archived bike holds nothing, and the stashes of an account are summed"
    );
    assert!(file["accounts"].as_array().unwrap().iter().all(|a| a.get("free").is_none() && a.get("entries").is_none() && a.get("available").is_none()), "free is worked out from the balance and setAside, both in the file");
    // what the screens would call free, worked out from the file
    let free_of = |name: &str| {
        let a = file["accounts"].as_array().unwrap().iter().find(|a| a["name"] == name).unwrap();
        crate::money::parse_signed_amount(a["balance"].as_str().unwrap()).unwrap() - crate::money::parse_signed_amount(a["setAside"].as_str().unwrap()).unwrap()
    };
    for account in accounts::list(&s.c.conn, TODAY).unwrap() {
        assert_eq!(free_of(&account.name), crate::money::parse_signed_amount(&account.free).unwrap(), "{}", account.name);
    }
    // a world with no stash says 0.00 for every account
    let plain = self::file(&case().conn, &Selection::default());
    assert!(plain["accounts"].as_array().unwrap().iter().all(|a| a["setAside"] == "0.00"));
    assert_eq!(plain["stashes"], json!([]), "no stash, and still the part");
}

#[test]
fn a_limit_leaves_the_stashes_out_of_the_file_and_says_nothing_about_them() {
    let s = stashed();
    let c = &s.c;
    let only_stashes = |selection: Selection| Selection { sections: Some(vec![Section::Stashes]), ..selection };
    let limits = [
        ("dates", Selection { from: Some("2026-01-01".into()), ..Default::default() }),
        ("a last day", Selection { to: Some("2026-12-31".into()), ..Default::default() }),
        ("a category", Selection { category_ids: Some(vec![c.rent.to_string()]), ..Default::default() }),
        ("no category", Selection { category_ids: Some(Vec::new()), ..Default::default() }),
        ("expenses", Selection { kind: Some(Kind::Expense), ..Default::default() }),
        ("income", Selection { kind: Some(Kind::Income), ..Default::default() }),
        ("a least amount", Selection { min_amount: Some("1.00".into()), ..Default::default() }),
        ("a most amount", Selection { max_amount: Some("5000.00".into()), ..Default::default() }),
        ("a text", Selection { text: Some("rent".into()), ..Default::default() }),
        ("shared bills", Selection { split: Some(SplitFilter::Split), ..Default::default() }),
        ("bills that are not shared", Selection { split: Some(SplitFilter::NotSplit), ..Default::default() }),
        ("a category with the debts", Selection { category_ids: Some(vec![c.rent.to_string()]), include_debts: true, ..Default::default() }),
    ];
    for (label, limit) in limits {
        for selection in [limit.clone(), only_stashes(limit.clone())] {
            let text = json(&c.conn, &selection, now()).unwrap();
            let file: Value = serde_json::from_str(&text).unwrap();
            assert!(file.get("stashes").is_none(), "{label}: a file does not say what it was told to leave out");
            for hidden in ["Holiday in Lisbon", "Rainy day", "New laptop", "Bike", "deposit for the flat", "bought it", "setAside", "\"free\"", "1200.00"] {
                assert!(!text.contains(hidden), "{label}: {hidden:?} leaked into a limited file");
            }
            for account in file["accounts"].as_array().into_iter().flatten() {
                assert_eq!(keys(account), BTreeSet::from(["id", "name", "kind", "currency"]), "{label}: an account is named, with its kind and currency, and no more");
            }
            let p = preview(&c.conn, &selection).unwrap();
            assert_eq!((p.stashes, p.stash_entries), (0, 0), "{label}");
        }
    }
    // the same file with no limit has them: it is the limit that leaves them out, not the part
    assert_eq!(file(&c.conn, &Selection::default())["stashes"].as_array().unwrap().len(), 4);
}

#[test]
fn a_part_that_is_left_out_has_no_key_and_the_part_alone_is_the_stashes_alone() {
    let s = stashed();
    let c = &s.c;
    let only = |sections: &[Section]| Selection { sections: Some(sections.to_vec()), ..Default::default() };
    let base: BTreeSet<&str> = ["format", "version", "exportedAt", "scope", "settings"].into();
    let with = |extra: &[&'static str]| -> BTreeSet<&'static str> { base.iter().copied().chain(extra.iter().copied()).collect() };
    let alone = file(&c.conn, &only(&[Section::Stashes]));
    assert_eq!(keys(&alone), with(&["stashes"]), "no accounts, no transfers, nothing else");
    assert_eq!(alone["scope"]["sections"], json!(["stashes"]));
    assert_eq!(alone["stashes"].as_array().unwrap().len(), 4);
    for left_out in [only(&[Section::Accounts]), only(&[Section::Transfers]), only(&[Section::Categories, Section::Transactions, Section::Accounts, Section::Transfers])] {
        let file = file(&c.conn, &left_out);
        assert!(file.get("stashes").is_none(), "{left_out:?}: the part was not asked for");
    }
    assert!(!json(&c.conn, &only(&[Section::Accounts, Section::Transfers]), now()).unwrap().contains("Holiday"));
    // everything but the stashes: the keys are all there but that one
    let every_other: Vec<Section> = [Section::Transactions, Section::Categories, Section::Budgets, Section::Recurring, Section::Accounts, Section::Transfers, Section::Documents, Section::People, Section::Due, Section::Ledger, Section::History].into();
    let without = file(&c.conn, &only(&every_other));
    assert!(without.get("stashes").is_none() && without.get("accounts").is_some() && without.get("transfers").is_some());
    assert_eq!(without["accounts"][1]["setAside"], "320.00", "the accounts say what their stashes hold even when the stashes are not in the file");
    // with the part, the keys of a whole file
    assert!(file(&c.conn, &only(&[Section::Stashes, Section::Accounts])).get("accounts").is_some());
    // the words of the screen are the ones that are read
    let sent: Selection = serde_json::from_value(json!({ "sections": ["accounts", "stashes"] })).unwrap();
    assert_eq!(sent.sections, Some(vec![Section::Accounts, Section::Stashes]));
    assert_eq!(Section::Stashes.as_str(), "stashes");
    assert!(serde_json::from_value::<Selection>(json!({ "sections": ["stash"] })).is_err());
}

#[test]
fn the_preview_counts_the_stashes_and_their_entries_and_zero_when_there_is_a_limit_or_the_part_is_left_out() {
    let s = stashed();
    let c = &s.c;
    let all = preview(&c.conn, &Selection::default()).unwrap();
    assert_eq!((all.stashes, all.stash_entries), (4, 7), "the archived one and its two entries too");
    assert_eq!((all.accounts, all.transfers), (5, 5), "the transfers that came with two of the entries are transfers, and are counted as such");
    let parts = |sections: &[Section]| Selection { sections: Some(sections.to_vec()), ..Default::default() };
    let alone = preview(&c.conn, &parts(&[Section::Stashes])).unwrap();
    assert_eq!((alone.stashes, alone.stash_entries, alone.accounts, alone.transfers, alone.transactions), (4, 7, 0, 0, 0));
    let left_out = preview(&c.conn, &parts(&[Section::Accounts, Section::Transfers, Section::Transactions])).unwrap();
    assert_eq!((left_out.stashes, left_out.stash_entries), (0, 0), "a part that is left out counts for nothing");
    for limit in [Selection { from: Some("2026-01-01".into()), ..Default::default() }, Selection { category_ids: Some(vec![c.rent.to_string()]), ..Default::default() }, Selection { text: Some("a".into()), ..Default::default() }] {
        let p = preview(&c.conn, &limit).unwrap();
        assert_eq!((p.stashes, p.stash_entries), (0, 0), "{limit:?}");
        let both = preview(&c.conn, &Selection { sections: Some(vec![Section::Stashes]), ..limit.clone() }).unwrap();
        assert_eq!((both.stashes, both.stash_entries), (0, 0), "{limit:?} with the part");
    }
    // what the screen reads: the counts are in the answer under these names
    let sent = serde_json::to_value(&all).unwrap();
    assert_eq!((sent["stashes"].as_u64(), sent["stashEntries"].as_u64()), (Some(4), Some(7)));
    assert!(sent.get("stash_entries").is_none());
    // a count is what the file holds
    let file = file(&c.conn, &Selection::default());
    let held: usize = file["stashes"].as_array().unwrap().iter().map(|t| t["entries"].as_array().unwrap().len()).sum();
    assert_eq!(held, all.stash_entries as usize);
    // and a world with none has none
    let (conn, ..) = lived_in();
    let none = preview(&conn, &Selection::default()).unwrap();
    assert_eq!((none.stashes, none.stash_entries), (0, 0));
    assert_eq!(self::file(&conn, &Selection::default())["stashes"], json!([]));
}

#[test]
fn a_stash_in_an_account_of_another_currency_is_in_that_currency_and_one_in_the_main_currency_follows_a_change_of_it() {
    let (c, lei) = foreign_case();
    let wedding = make(&c.conn, "Wedding gift", lei, Some("1000.00"), None, Some("in lei"));
    add(&c.conn, wedding, "20.00", "2026-03-10", Some("first"), None);
    take(&c.conn, wedding, "5.00", "2026-03-11", None, None);
    let holiday = make(&c.conn, "Holiday", c.savings, Some("1200.00"), None, None);
    add(&c.conn, holiday, "100.00", "2026-03-10", None, None);
    let file = file(&c.conn, &Selection::default());
    let list = file["stashes"].as_array().unwrap();
    let wedding = list.iter().find(|t| t["name"] == "Wedding gift").unwrap();
    assert_eq!((wedding["currency"].as_str(), wedding["account"].as_str(), wedding["target"].as_str(), wedding["saved"].as_str()), (Some("RON"), Some("Lei account"), Some("1000.00"), Some("15.00")), "every amount of it is in lei");
    assert_eq!(wedding["entries"][0]["amount"], "20.00");
    assert_eq!(list.iter().find(|t| t["name"] == "Holiday").unwrap()["currency"], "EUR");
    let account = file["accounts"].as_array().unwrap().iter().find(|a| a["name"] == "Lei account").unwrap();
    assert_eq!((account["setAside"].as_str(), account["currency"].as_str()), (Some("15.00"), Some("RON")), "what is set aside is in the account's currency too");

    // the app's currency changed (an account of another currency refuses it, so a world with none): the code follows and the amounts are converted
    let s = stashed();
    let mut conn = s.c.conn;
    crate::currency::change(&mut conn, "USD", Some("2"), TODAY).unwrap();
    let usd = self::file(&conn, &Selection::default());
    let holiday = usd["stashes"].as_array().unwrap().iter().find(|t| t["name"] == "Holiday in Lisbon").unwrap();
    assert_eq!((holiday["currency"].as_str(), holiday["target"].as_str(), holiday["saved"].as_str()), (Some("USD"), Some("2400.00"), Some("240.00")));
    assert_eq!(holiday["entries"][2]["kind"], "TAKE");
    assert_eq!(holiday["entries"][2]["amount"], "60.00");
    assert_eq!(usd["accounts"][1]["setAside"], "640.00");
}

#[test]
fn a_name_or_a_note_that_looks_like_a_formula_or_has_commas_and_quotes_is_exact_in_the_json_and_changes_nothing_in_the_csv() {
    let s = stashed();
    let before = csv(&s.c.conn, &Selection::default()).unwrap();
    let awkward = ["Rent, \"joint\"", "=SUM(1+1)", "+cash", "-5% fund", "@home", "'=already text", "日本語 and ünï"];
    for (i, name) in awkward.iter().enumerate() {
        let stash = make(&s.c.conn, name, s.c.cash, Some("5.00"), None, Some(&format!("=note {i}\nsecond, \"line\"")));
        add(&s.c.conn, stash, "1.00", "2026-04-02", Some(&format!("-5% off {i}, \"really\"")), None);
    }
    let json_file = file(&s.c.conn, &Selection::default());
    let list = json_file["stashes"].as_array().unwrap();
    for (i, name) in awkward.iter().enumerate() {
        let stash = list.iter().find(|t| t["name"].as_str() == Some(*name)).unwrap_or_else(|| panic!("{name:?} is not in the file"));
        assert_eq!(stash["notes"].as_str(), Some(format!("=note {i}\nsecond, \"line\"").as_str()), "the JSON holds the text as it is");
        assert_eq!(stash["entries"][0]["note"].as_str(), Some(format!("-5% off {i}, \"really\"").as_str()));
    }
    // the CSV is the transactions: stashes are not in it, so it is the file it was before and no cell can run as a formula
    let after = csv(&s.c.conn, &Selection::default()).unwrap();
    assert_eq!(after, before);
    for row in &read_csv(&after)[1..] {
        for cell in row {
            assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{cell:?} would run as a formula");
        }
    }
    for name in awkward {
        assert!(!after.contains(name), "{name:?} is in the CSV");
    }
}

#[test]
fn the_order_is_by_position_then_id_with_the_archived_ones_last() {
    let s = stashed();
    let names_now = |conn: &Connection| names(&file(conn, &Selection::default())["stashes"]).iter().map(|n| n.to_string()).collect::<Vec<_>>();
    assert_eq!(names_now(&s.c.conn), ["New laptop", "Holiday in Lisbon", "Rainy day", "Bike"]);
    stashes::reorder(&s.c.conn, &[s.rainy, s.laptop, s.holiday]).unwrap();
    assert_eq!(names_now(&s.c.conn), ["Rainy day", "New laptop", "Holiday in Lisbon", "Bike"], "the screens' order, not the order they were made in");
    // an archived stash that is restored goes among the others by its position, and one that is archived goes last
    stashes::archive(&s.c.conn, s.rainy, TODAY).unwrap();
    stashes::restore(&s.c.conn, s.bike, TODAY).unwrap();
    assert_eq!(names_now(&s.c.conn), ["New laptop", "Holiday in Lisbon", "Bike", "Rainy day"]);
    // two stashes at one position are in the order they were made, and the archived ones are still after the others
    s.c.conn.execute("UPDATE stashes SET position = 0", []).unwrap();
    assert_eq!(names_now(&s.c.conn), ["Holiday in Lisbon", "New laptop", "Bike", "Rainy day"]);
}

#[test]
fn exporting_stashes_changes_nothing() {
    let s = stashed();
    let c = &s.c;
    let before = snapshot(&c.conn);
    let history: i64 = c.conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
    for selection in [
        Selection::default(),
        Selection { category_ids: Some(vec![c.rent.to_string()]), include_debts: true, ..Default::default() },
        Selection { sections: Some(vec![Section::Stashes]), ..Default::default() },
        Selection { sections: Some(vec![Section::Stashes, Section::Accounts]), from: Some("2026-01-01".into()), ..Default::default() },
    ] {
        csv(&c.conn, &selection).unwrap();
        json(&c.conn, &selection, now()).unwrap();
        preview(&c.conn, &selection).unwrap();
        preview_text(&c.conn, &selection, TextFormat::Json, now()).unwrap();
        let _ = workbook(&c.conn, &selection, &Default::default());
    }
    assert_eq!(snapshot(&c.conn), before, "no row, stamp, marker or history line: the stashes and their entries are in the snapshot");
    assert_eq!(c.conn.query_row::<i64, _, _>("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap(), history);
}
