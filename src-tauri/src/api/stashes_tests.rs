//! The routes for stashes (`docs/stash-design.md`, sections 5, 6 and 12). What matters: every answer has the shape of section 6 (ids and money as
//! text), a read token reads and cannot write, every odd or wrong argument is refused by name, every refusal of section 5 is a status and a code and
//! leaves the data exactly as it was, and the odd transitions (archive twice, add to an archived stash, take more than is saved, delete an entry
//! whose money was taken out since, add with a transfer that makes the room) answer as the note says.

use std::sync::Arc;

use serde_json::{json, Value};

use super::read_tests::{at, expect, ok, route, run, run_at, seeded, TODAY};
use super::routes::{self, Kind};
use super::tests::{call, make_token, new_vault};
use super::*;
use crate::backup;
use crate::stashes;
use crate::tokens::Scope;

const INVALID: (u16, &str) = (400, "invalid_input");
const NOT_FOUND: (u16, &str) = (404, "not_found");
const NAME_EXISTS: (u16, &str) = (409, "name_exists");

const STASH_KEYS: [&str; 21] = [
    "account", "accountArchived", "accountId", "archived", "currency", "deposit", "entries", "free", "id", "kind", "lastEntry", "left", "name", "notes", "overdue", "perMonth", "percent", "reached", "saved", "target", "targetDate",
];
const ENTRY_KEYS: [&str; 10] = ["amount", "date", "id", "kind", "note", "otherAccount", "otherAccountId", "savedAfter", "stashId", "transferId"];

const READ_ROUTES: [&str; 4] = ["stashes.list", "stashes.show", "stashes.history", "stashes.summary"];
const WRITE_ROUTES: [&str; 10] = [
    "stashes.create",
    "stashes.update",
    "stashes.archive",
    "stashes.unarchive",
    "stashes.delete",
    "stashes.reorder",
    "stashes.add",
    "stashes.take",
    "stashes.amend",
    "stashes.erase",
];

// ---- helpers ----------------------------------------------------------------------------------

/// A set-up vault with one account (the main one), which holds nothing.
fn fresh() -> (tempfile::TempDir, Arc<Vault>) {
    let dir = tempfile::tempdir().unwrap();
    let vault = new_vault(dir.path());
    (dir, vault)
}

fn id_of(value: &Value) -> String {
    value["id"].as_str().unwrap().to_string()
}

fn dump(vault: &Vault) -> backup::Snapshot {
    vault.with_conn(|c| backup::dump(c)).unwrap()
}

fn keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys
}

/// "12.50", "-0.05": what money looks like in an answer.
fn is_money(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    digits.split_once('.').is_some_and(|(whole, cents)| !whole.is_empty() && whole.bytes().all(|b| b.is_ascii_digit()) && cents.len() == 2 && cents.bytes().all(|b| b.is_ascii_digit()))
}

fn is_id(text: &str) -> bool {
    !text.is_empty() && !text.starts_with('0') && text.bytes().all(|b| b.is_ascii_digit())
}

/// A message that says what was wrong with the request and nothing of the database.
fn plain(message: &str) {
    let message = message.to_lowercase();
    for leak in ["sqlite", "select", "insert", "table", "column", "constraint", "foreign key", "unique", "stash_entries", "amount_minor", "account_id", "transfers", "rusqlite"] {
        assert!(!message.contains(leak), "{message}");
    }
}

/// A request that is refused with this status and code: nothing was written, and the message is plain. Gives the message.
fn refuse(vault: &Vault, name: &str, args: Value, expected: (u16, &str)) -> String {
    let before = dump(vault);
    let e = run(vault, name, args.clone()).expect_err(&format!("{name} {args} was accepted"));
    assert_eq!((e.status, e.code), expected, "{name} {args}: {}", e.message);
    plain(&e.message);
    assert_eq!(dump(vault), before, "{name} {args} changed something");
    e.message.into_owned()
}

fn account(vault: &Vault, name: &str, balance: &str) -> String {
    id_of(&ok(vault, "accounts.create", json!({ "name": name, "kind": "SAVINGS", "openingBalance": balance, "openingDate": "2026-01-01" })))
}

fn lei_account(vault: &Vault, name: &str) -> String {
    id_of(&ok(vault, "accounts.create", json!({ "name": name, "kind": "CASH", "openingBalance": "500.00", "openingDate": "2026-01-01", "currency": "RON", "rate": "5.00" })))
}

fn stash(vault: &Vault, name: &str, account: &str, extra: Value) -> String {
    let mut body = json!({ "name": name, "accountId": account });
    for (key, value) in extra.as_object().unwrap() {
        body[key] = value.clone();
    }
    id_of(&ok(vault, "stashes.create", body))
}

fn add(vault: &Vault, stash: &str, amount: &str) -> Value {
    ok(vault, "stashes.add", json!({ "id": stash, "amount": amount, "date": "2026-10-01" }))
}

fn show(vault: &Vault, id: &str) -> Value {
    ok(vault, "stashes.show", json!({ "id": id }))
}

fn listed(vault: &Vault) -> Vec<Value> {
    ok(vault, "stashes.list", json!({})).as_array().unwrap().clone()
}

fn find(vault: &Vault, id: &str) -> Value {
    show(vault, id)["stash"].clone()
}

fn names(vault: &Vault) -> Vec<String> {
    listed(vault).iter().map(|s| s["name"].as_str().unwrap().to_string()).collect()
}

fn balance(vault: &Vault, account: &str) -> String {
    ok(vault, "accounts.list", json!({})).as_array().unwrap().iter().find(|a| a["id"] == account).unwrap()["balance"].as_str().unwrap().to_string()
}

fn transfer_count(vault: &Vault) -> u64 {
    ok(vault, "transfers.list", json!({ "limit": 1 }))["total"].as_u64().unwrap()
}

fn entry_ids(detail: &Value) -> Vec<String> {
    detail["entries"].as_array().unwrap().iter().map(id_of).collect()
}

/// A stash in an account that holds 1000.00, with a target of 1200.00 and a date, and one add of 300.00: what the requests below name.
struct World {
    /// 1000.00, EUR: the holiday is in it, which holds 300.00, so 700.00 is free.
    alpha: String,
    /// 500.00, EUR, nothing set aside.
    beta: String,
    /// 100.00, EUR, nothing set aside: too little for the holiday to move to.
    poor: String,
    /// In RON.
    lei: String,
    /// Archived.
    gone: String,
    holiday: String,
    /// The add of 300.00 on 2026-10-01.
    first: String,
}

fn world(vault: &Vault) -> World {
    let alpha = account(vault, "Alpha", "1000.00");
    let beta = account(vault, "Beta", "500.00");
    let poor = account(vault, "Poor", "100.00");
    let lei = lei_account(vault, "Lei");
    let gone = account(vault, "Gamma", "100.00");
    ok(vault, "accounts.archive", json!({ "id": gone }));
    let holiday = stash(vault, "Holiday", &alpha, json!({ "target": "1200.00", "targetDate": "2027-06-30", "notes": "a week away" }));
    let made = ok(vault, "stashes.add", json!({ "id": holiday, "amount": "300.00", "date": "2026-10-01", "note": "first" }));
    let first = id_of(&made["entries"][0]);
    World { alpha, beta, poor, lei, gone, holiday, first }
}

/// A request that would work, for each route.
fn requests(w: &World) -> Vec<(&'static str, Value)> {
    vec![
        ("stashes.list", json!({ "accountId": w.alpha })),
        ("stashes.show", json!({ "id": w.holiday })),
        ("stashes.history", json!({ "id": w.holiday, "year": "2026" })),
        ("stashes.summary", json!({})),
        ("stashes.create", json!({ "name": "New", "accountId": w.alpha, "target": "10.00", "targetDate": "2027-01-01", "notes": "n" })),
        ("stashes.update", json!({ "id": w.holiday, "name": "Trip", "accountId": w.beta, "target": "1300.00", "targetDate": "2027-07-01", "notes": "m" })),
        ("stashes.archive", json!({ "id": w.holiday })),
        ("stashes.unarchive", json!({ "id": w.holiday })),
        ("stashes.delete", json!({ "id": w.holiday })),
        ("stashes.reorder", json!({ "ids": [w.holiday] })),
        ("stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-02", "note": "n", "fromAccountId": w.beta })),
        ("stashes.take", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-02", "note": "n", "toAccountId": w.beta })),
        ("stashes.amend", json!({ "id": w.first, "amount": "2.00", "date": "2026-10-02", "note": "n" })),
        ("stashes.erase", json!({ "id": w.first, "deleteTransfer": false })),
    ]
}

/// The name, kind, whether it is required and whether it takes null, of an argument.
type Arg = (&'static str, Kind, bool, bool);

// ---- the table --------------------------------------------------------------------------------

#[test]
fn the_routes_have_the_scopes_and_arguments_of_the_design_note() {
    for name in READ_ROUTES {
        assert_eq!(route(name).scope, Scope::Read, "{name}");
        assert!(route(name).confirm.is_none(), "{name}");
    }
    for name in WRITE_ROUTES {
        assert_eq!(route(name).scope, Scope::Write, "{name}");
        assert!(route(name).confirm.is_none(), "{name}: the screens do not ask for these, and nothing leaves the app");
    }
    // (name, kind, required, nullable) of each argument, in order
    let table: [(&str, &[Arg]); 14] = [
        ("stashes.list", &[("accountId", Kind::Id, false, false)]),
        ("stashes.show", &[("id", Kind::Id, true, false)]),
        ("stashes.history", &[("id", Kind::Id, true, false), ("year", Kind::Year, true, false)]),
        ("stashes.summary", &[]),
        (
            "stashes.create",
            &[("name", Kind::String, true, false), ("accountId", Kind::Id, true, false), ("target", Kind::Money, false, false), ("targetDate", Kind::Date, false, false), ("notes", Kind::String, false, false)],
        ),
        (
            "stashes.update",
            &[
                ("id", Kind::Id, true, false),
                ("name", Kind::String, false, false),
                ("accountId", Kind::Id, false, false),
                ("target", Kind::Money, false, true),
                ("targetDate", Kind::Date, false, true),
                ("notes", Kind::String, false, true),
            ],
        ),
        ("stashes.archive", &[("id", Kind::Id, true, false)]),
        ("stashes.unarchive", &[("id", Kind::Id, true, false)]),
        ("stashes.delete", &[("id", Kind::Id, true, false)]),
        ("stashes.reorder", &[("ids", Kind::Ids, true, false)]),
        (
            "stashes.add",
            &[("id", Kind::Id, true, false), ("amount", Kind::Money, true, false), ("date", Kind::Date, false, false), ("note", Kind::String, false, false), ("fromAccountId", Kind::Id, false, false)],
        ),
        (
            "stashes.take",
            &[("id", Kind::Id, true, false), ("amount", Kind::Money, true, false), ("date", Kind::Date, false, false), ("note", Kind::String, false, false), ("toAccountId", Kind::Id, false, false)],
        ),
        ("stashes.amend", &[("id", Kind::Id, true, false), ("amount", Kind::Money, false, false), ("date", Kind::Date, false, false), ("note", Kind::String, false, true)]),
        ("stashes.erase", &[("id", Kind::Id, true, false), ("deleteTransfer", Kind::Boolean, false, false)]),
    ];
    for (name, expected) in table {
        let got: Vec<Arg> = route(name).params.iter().map(|p| (p.name, p.kind, p.required, p.nullable)).collect();
        assert_eq!(got, expected, "{name}");
    }
    // the routes are in the table of the server, once each, and the table is exactly these
    let ours: Vec<&str> = routes::ALL.iter().filter(|r| r.name.starts_with("stashes.")).map(|r| r.name).collect();
    let expected: Vec<&str> = READ_ROUTES.iter().chain(WRITE_ROUTES.iter()).copied().collect();
    assert_eq!(ours.len(), 14);
    for name in &expected {
        assert!(ours.contains(name), "{name}");
    }
}

#[test]
fn the_summaries_say_what_a_person_has_to_know_before_calling() {
    let text = |name: &str| route(name).summary;
    // the money stays in its account, in every route that puts money in a stash or takes it out and in those that show it
    for name in ["stashes.list", "stashes.show", "stashes.create", "stashes.update", "stashes.add", "stashes.take", "stashes.amend", "stashes.erase"] {
        assert!(text(name).contains("Money in a stash stays in its account"), "{name}");
    }
    // what free is, where it limits an add
    for name in ["stashes.list", "stashes.show", "stashes.update", "stashes.add"] {
        assert!(text(name).contains("free is what the account holds that none of its stashes has set aside: the balance minus what its stashes hold"), "{name}");
    }
    // every refusal of an add, a take, an amendment and a move, in a sentence
    let add = text("stashes.add");
    for refusal in ["above what is free", "left to the target", "after today", "a stash that is archived", "an account that is archived", "is in another currency"] {
        assert!(add.contains(refusal), "stashes.add does not say: {refusal}");
    }
    assert!(add.contains("fromAccountId") && add.contains("a transfer from that account into the stash's account is made with the entry"));
    let take = text("stashes.take");
    for refusal in ["above what the stash holds", "after today", "is in another currency", "an archived stash"] {
        assert!(take.contains(refusal) || take.contains("works on an archived stash"), "stashes.take does not say: {refusal}");
    }
    assert!(take.contains("toAccountId") && take.contains("a transfer from the stash's account to that account is made with the entry"));
    let update = text("stashes.update");
    for refusal in ["same currency", "at least that much free", "may not be below what the stash holds", "not be archived"] {
        assert!(update.contains(refusal), "stashes.update does not say: {refusal}");
    }
    let amend = text("stashes.amend");
    assert!(amend.contains("The kind of an entry never changes") && amend.contains("after today") && amend.contains("archived"));
    let erase = text("stashes.erase");
    assert!(erase.contains("The transfer that came with the entry stays, unless deleteTransfer is true"), "{erase}");
    assert!(text("stashes.create").contains("name_exists") && text("stashes.unarchive").contains("name_exists"));
}

// ---- the answers ------------------------------------------------------------------------------

#[test]
fn every_answer_has_the_fields_of_the_design_note_with_ids_and_money_as_text() {
    let (_dir, vault) = seeded();
    let list = listed(&vault);
    assert_eq!(names(&vault), ["Holiday in Lisbon", "New laptop", "Rainy day", "Wedding gift", "Bike"], "the person's order, the archived one last");
    for s in &list {
        assert_eq!(keys(s), STASH_KEYS, "{s}");
        assert!(is_id(s["id"].as_str().unwrap()) && is_id(s["accountId"].as_str().unwrap()), "{s}");
        for money in ["saved", "free"] {
            assert!(is_money(s[money].as_str().unwrap_or("not text")), "{money} of {s}");
        }
        for money in ["target", "left", "perMonth"] {
            assert!(s[money].is_null() || is_money(s[money].as_str().unwrap_or("not text")), "{money} of {s}");
        }
        assert!(s["currency"].as_str().is_some_and(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase())), "{s}");
        assert!(s["archived"].is_boolean() && s["accountArchived"].is_boolean() && s["reached"].is_boolean() && s["overdue"].is_boolean() && s["entries"].is_u64(), "{s}");
        assert!(s["percent"].is_null() || s["percent"].is_u64(), "{s}");
        assert!(s["notes"].is_null() || s["notes"].as_str().is_some_and(|t| !t.is_empty()), "{s}");
        assert!(s["targetDate"].is_null() || s["targetDate"].as_str().is_some_and(|d| d.len() == 10), "{s}");
        assert!(s["lastEntry"].is_null() || s["lastEntry"].as_str().is_some_and(|d| d.len() == 10), "{s}");
        assert_eq!(s["target"].is_null(), s["left"].is_null() && s["percent"].is_null(), "a stash without a target has no left and no percent: {s}");
    }
    // what the screens get from the same function
    assert_eq!(Value::Array(list.clone()), expect(&vault, |c| stashes::list(c, None, TODAY)));
    // the demo's stash in lei has its amounts in lei, and the others are in the app's currency
    let wedding = list.iter().find(|s| s["name"] == "Wedding gift").unwrap();
    assert_eq!((wedding["currency"].as_str(), wedding["saved"].as_str(), wedding["target"].as_str()), (Some("RON"), Some("550.00"), Some("1000.00")));
    assert!(list.iter().filter(|s| s["name"] != "Wedding gift").all(|s| s["currency"] == "EUR"));
    let bike = list.last().unwrap();
    assert_eq!((bike["archived"].as_bool(), bike["saved"].as_str(), bike["reached"].as_bool(), bike["entries"].as_u64()), (Some(true), Some("0.00"), Some(false), Some(4)));
    let laptop = &list[1];
    assert_eq!((laptop["reached"].as_bool(), laptop["percent"].as_u64(), laptop["left"].as_str(), laptop["perMonth"].clone()), (Some(true), Some(100), Some("0.00"), Value::Null));
    let rainy = &list[2];
    assert_eq!((rainy["target"].clone(), rainy["left"].clone(), rainy["percent"].clone(), rainy["perMonth"].clone(), rainy["reached"].as_bool()), (Value::Null, Value::Null, Value::Null, Value::Null, Some(false)));

    // one stash: the stash and its entries, newest first, each with what the stash held after it
    let detail = show(&vault, "1");
    assert_eq!(keys(&detail), ["entries", "stash"]);
    assert_eq!(keys(&detail["stash"]), STASH_KEYS);
    assert_eq!(detail, expect(&vault, |c| stashes::get(c, 1, TODAY)), "the screens get the same");
    let entries = detail["entries"].as_array().unwrap();
    assert_eq!((entries.len(), detail["stash"]["entries"].as_u64()), (7, Some(7)));
    for e in entries {
        assert_eq!(keys(e), ENTRY_KEYS, "{e}");
        assert!(is_id(e["id"].as_str().unwrap()) && e["stashId"] == "1", "{e}");
        assert!(is_money(e["amount"].as_str().unwrap()) && !e["amount"].as_str().unwrap().starts_with('-'), "an amount is above zero: the kind says which way it went: {e}");
        assert!(["ADD", "TAKE"].contains(&e["kind"].as_str().unwrap()) && is_money(e["savedAfter"].as_str().unwrap()), "{e}");
        assert!(e["note"].is_null() || e["note"].is_string(), "{e}");
        assert_eq!(e["transferId"].is_string(), e["otherAccountId"].is_string(), "{e}");
        assert_eq!(e["otherAccountId"].is_string(), e["otherAccount"].is_string(), "{e}");
    }
    let dates: Vec<&str> = entries.iter().map(|e| e["date"].as_str().unwrap()).collect();
    assert!(dates.windows(2).all(|w| w[0] >= w[1]), "newest first: {dates:?}");
    assert_eq!(entries[0]["savedAfter"], detail["stash"]["saved"], "the newest says what the stash holds now");
    assert_eq!(entries.iter().filter(|e| e["kind"] == "TAKE").count(), 1);
    let linked: Vec<&Value> = entries.iter().filter(|e| e["transferId"].is_string()).collect();
    assert_eq!(linked.len(), 5, "the first five adds of the holiday came with a transfer from the current account");
    assert!(linked.iter().all(|e| e["otherAccountId"] == "1" && e["kind"] == "ADD"));
    assert_eq!(entries.last().unwrap()["savedAfter"], "100.00", "the oldest, after its own amount");

    // the chart: a month and what the stash held at its end
    let history = ok(&vault, "stashes.history", json!({ "id": "1", "year": "2026" }));
    let points = history.as_array().unwrap();
    assert_eq!(points.len(), 10, "January to October, which is this month");
    assert!(points.iter().all(|p| keys(p) == ["month", "saved"] && is_money(p["saved"].as_str().unwrap())));
    assert_eq!((points[0]["month"].as_str(), points[9]["month"].as_str()), (Some("2026-01"), Some("2026-10")));
    assert_eq!(points[9]["saved"], detail["stash"]["saved"], "this month's point is what the stash holds today");
    assert_eq!(history, expect(&vault, |c| stashes::history(c, 1, "2026", TODAY)));

    // the totals, in the app's currency: the stash in lei at five to the euro, the archived one left out
    let summary = ok(&vault, "stashes.summary", json!({}));
    assert_eq!(keys(&summary), ["reached", "saved", "stashes", "target"]);
    assert_eq!((summary["stashes"].as_u64(), summary["reached"].as_u64()), (Some(4), Some(1)));
    assert_eq!((summary["saved"].as_str(), summary["target"].as_str()), (Some("2070.00"), Some("2300.00")), "560 + 900 + 500 + 550 lei / 5; 1200 + 900 + 1000 lei / 5");
    assert_eq!(summary, expect(&vault, |c| stashes::summary(c, TODAY)));
}

#[test]
fn the_list_can_be_limited_to_an_account_and_a_stash_does_not_move_the_account() {
    let (_dir, vault) = seeded();
    let of = |account: &str| -> Vec<String> { ok(&vault, "stashes.list", json!({ "accountId": account })).as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap().to_string()).collect() };
    assert_eq!(of("1"), ["New laptop"]);
    assert_eq!(of("2"), ["Holiday in Lisbon", "Rainy day", "Bike"], "the savings: in the person's order, the archived one last");
    assert_eq!(of("6"), ["Wedding gift"]);
    assert!(of("3").is_empty() && of("999999").is_empty(), "an account with none, and one that is not there");
    // the accounts say what is set aside in them; the balance is what it was
    let accounts = ok(&vault, "accounts.list", json!({}));
    let savings = accounts.as_array().unwrap().iter().find(|a| a["id"] == "2").unwrap();
    assert_eq!(savings["setAside"], "1060.00");
    let (balance, free) = (savings["balance"].as_str().unwrap(), savings["free"].as_str().unwrap());
    let cents = |t: &str| t.replace('.', "").parse::<i64>().unwrap();
    assert_eq!(cents(free), cents(balance) - 106_000);
    // every stash of the list says the free money of its account
    for s in ok(&vault, "stashes.list", json!({ "accountId": "2" })).as_array().unwrap() {
        assert_eq!(s["free"], savings["free"]);
    }
}

// ---- making and changing stashes --------------------------------------------------------------

#[test]
fn a_stash_is_made_changed_cleared_and_put_away() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    let made = ok(&vault, "stashes.create", json!({ "name": "  New laptop ", "accountId": w.beta, "target": "900.00", "targetDate": "2027-06-30", "notes": "line one\nline two" }));
    assert_eq!(keys(&made), STASH_KEYS);
    assert_eq!((made["name"].as_str(), made["accountId"].as_str(), made["account"].as_str(), made["currency"].as_str()), (Some("New laptop"), Some(w.beta.as_str()), Some("Beta"), Some("EUR")), "the name is trimmed");
    assert_eq!((made["target"].as_str(), made["targetDate"].as_str(), made["notes"].as_str()), (Some("900.00"), Some("2027-06-30"), Some("line one\nline two")), "line breaks are kept");
    assert_eq!((made["saved"].as_str(), made["left"].as_str(), made["percent"].as_u64(), made["reached"].as_bool(), made["entries"].as_u64(), made["lastEntry"].clone()), (Some("0.00"), Some("900.00"), Some(0), Some(false), Some(0), Value::Null));
    assert_eq!((made["archived"].as_bool(), made["accountArchived"].as_bool(), made["overdue"].as_bool(), made["free"].as_str()), (Some(false), Some(false), Some(false), Some("500.00")));
    assert_eq!(made["perMonth"], "112.50", "8 months from October 2026 to June 2027: 900.00 / 8");
    let id = id_of(&made);
    assert_eq!(names(&vault), ["Holiday", "New laptop"], "at the end of the list");
    assert_eq!(find(&vault, &id), made, "the list and the stash say the same");

    // only what is named changes
    let renamed = ok(&vault, "stashes.update", json!({ "id": id, "name": "Laptop" }));
    assert_eq!((renamed["name"].as_str(), renamed["target"].as_str(), renamed["targetDate"].as_str(), renamed["notes"].as_str(), renamed["accountId"].as_str()), (Some("Laptop"), Some("900.00"), Some("2027-06-30"), Some("line one\nline two"), Some(w.beta.as_str())));
    let more = ok(&vault, "stashes.update", json!({ "id": id, "target": "1000.00", "targetDate": "2027-07-31", "notes": "n" }));
    assert_eq!((more["target"].as_str(), more["targetDate"].as_str(), more["notes"].as_str(), more["name"].as_str()), (Some("1000.00"), Some("2027-07-31"), Some("n"), Some("Laptop")));
    // null clears the target, the date and the notes; leaving them out leaves them
    let none = ok(&vault, "stashes.update", json!({ "id": id, "target": null, "targetDate": null, "notes": null }));
    assert_eq!((none["target"].clone(), none["targetDate"].clone(), none["notes"].clone(), none["left"].clone(), none["percent"].clone(), none["perMonth"].clone()), (Value::Null, Value::Null, Value::Null, Value::Null, Value::Null, Value::Null));
    assert_eq!(ok(&vault, "stashes.update", json!({ "id": id, "target": "5.00" }))["target"], "5.00");
    assert_eq!(ok(&vault, "stashes.update", json!({ "id": id, "name": "Laptop 2" }))["target"], "5.00");
    // nothing named, or what it already is: the stash as it is, and nothing written
    let before = dump(&vault);
    assert_eq!(ok(&vault, "stashes.update", json!({ "id": id }))["name"], "Laptop 2");
    assert_eq!(ok(&vault, "stashes.update", json!({ "id": id, "name": "Laptop 2", "accountId": w.beta, "target": "5.00" }))["accountId"], w.beta.as_str());
    assert_eq!(dump(&vault), before);

    // a stash that holds something moves with it to an account that has the room, and the accounts say so
    let holiday = ok(&vault, "stashes.update", json!({ "id": w.holiday, "accountId": w.beta }));
    assert_eq!((holiday["accountId"].as_str(), holiday["account"].as_str(), holiday["saved"].as_str(), holiday["free"].as_str()), (Some(w.beta.as_str()), Some("Beta"), Some("300.00"), Some("200.00")), "500.00 less 300.00 (the laptop holds nothing)");
    assert_eq!(balance(&vault, &w.alpha), "1000.00");
    assert_eq!(balance(&vault, &w.beta), "500.00", "the balances are what they were");
    ok(&vault, "stashes.update", json!({ "id": w.holiday, "accountId": w.alpha }));

    // archive twice, and bring one back that was never put away
    let archived = ok(&vault, "stashes.archive", json!({ "id": id }));
    assert_eq!(archived["archived"], true);
    let before = dump(&vault);
    assert_eq!(ok(&vault, "stashes.archive", json!({ "id": id })), archived, "archiving an archived stash changes nothing");
    assert_eq!(ok(&vault, "stashes.unarchive", json!({ "id": w.holiday }))["archived"], false, "a stash that is not archived is answered as it is");
    assert_eq!(dump(&vault), before);
    assert_eq!(names(&vault), ["Holiday", "Laptop 2"], "still listed, the archived ones last");
    ok(&vault, "stashes.create", json!({ "name": "First", "accountId": w.alpha }));
    assert_eq!(names(&vault), ["Holiday", "First", "Laptop 2"], "the archived one goes to the end of the list");
    // an archived stash is changed after it is brought back
    refuse(&vault, "stashes.update", json!({ "id": id, "name": "Other" }), INVALID);
    assert_eq!(ok(&vault, "stashes.unarchive", json!({ "id": id }))["archived"], false);
    assert_eq!(ok(&vault, "stashes.update", json!({ "id": id, "name": "Other" }))["name"], "Other");

    // the order
    let ids: Vec<String> = listed(&vault).iter().map(id_of).collect();
    let reversed: Vec<String> = ids.iter().rev().cloned().collect();
    assert_eq!(ok(&vault, "stashes.reorder", json!({ "ids": reversed })), Value::Null);
    assert_eq!(listed(&vault).iter().map(id_of).collect::<Vec<_>>(), reversed);
    assert_eq!(names(&vault), ["First", "Other", "Holiday"]);

    // delete: the stash and its entries go, the account keeps its balance, and a second delete finds nothing
    assert_eq!(ok(&vault, "stashes.delete", json!({ "id": w.holiday })), Value::Null);
    assert_eq!(names(&vault), ["First", "Other"]);
    assert_eq!(refuse(&vault, "stashes.show", json!({ "id": w.holiday }), NOT_FOUND), "not found");
    refuse(&vault, "stashes.delete", json!({ "id": w.holiday }), NOT_FOUND);
    assert_eq!(balance(&vault, &w.alpha), "1000.00");
    assert_eq!(find(&vault, &id_of(&listed(&vault)[0]))["free"], "1000.00", "what the holiday held is free again");
}

#[test]
fn every_refusal_for_a_stash_is_a_status_and_a_code_and_changes_nothing() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    let other = stash(&vault, "Laptop", &w.alpha, json!({}));
    let long = "x".repeat(51);
    let many = |n: usize| "n".repeat(n);
    for (name, body, expected) in [
        // making
        ("stashes.create", json!({ "name": "", "accountId": w.alpha }), INVALID),
        ("stashes.create", json!({ "name": "   ", "accountId": w.alpha }), INVALID),
        ("stashes.create", json!({ "name": long, "accountId": w.alpha }), INVALID),
        ("stashes.create", json!({ "name": "a\nb", "accountId": w.alpha }), INVALID),
        ("stashes.create", json!({ "name": "a\u{0}b", "accountId": w.alpha }), INVALID),
        ("stashes.create", json!({ "name": "Fine", "accountId": "999999" }), INVALID),
        ("stashes.create", json!({ "name": "Fine", "accountId": w.gone }), INVALID),
        ("stashes.create", json!({ "name": "Fine", "accountId": w.alpha, "target": "0.00" }), INVALID),
        ("stashes.create", json!({ "name": "Fine", "accountId": w.alpha, "targetDate": "2026-02-30" }), INVALID),
        ("stashes.create", json!({ "name": "Fine", "accountId": w.alpha, "targetDate": "2026-13-01" }), INVALID),
        ("stashes.create", json!({ "name": "Fine", "accountId": w.alpha, "notes": many(1001) }), INVALID),
        ("stashes.create", json!({ "name": "holiday", "accountId": w.beta }), NAME_EXISTS),
        ("stashes.create", json!({ "name": "HOLIDAY", "accountId": w.alpha }), NAME_EXISTS),
        // changing
        ("stashes.update", json!({ "id": "999999", "name": "Fine" }), NOT_FOUND),
        ("stashes.update", json!({ "id": other, "name": "Holiday" }), NAME_EXISTS),
        ("stashes.update", json!({ "id": other, "name": "hOLIDAY" }), NAME_EXISTS),
        ("stashes.update", json!({ "id": other, "name": "" }), INVALID),
        ("stashes.update", json!({ "id": other, "target": "0.00" }), INVALID),
        ("stashes.update", json!({ "id": other, "targetDate": "2026-02-30" }), INVALID),
        ("stashes.update", json!({ "id": other, "notes": many(1001) }), INVALID),
        // a new target below what is saved (300.00)
        ("stashes.update", json!({ "id": w.holiday, "target": "299.99" }), INVALID),
        // a move: another currency, an archived account, an account that is not there, an account without the room (300.00 does not fit in 100.00)
        ("stashes.update", json!({ "id": w.holiday, "accountId": w.lei }), INVALID),
        ("stashes.update", json!({ "id": w.holiday, "accountId": w.gone }), INVALID),
        ("stashes.update", json!({ "id": w.holiday, "accountId": "999999" }), INVALID),
        ("stashes.update", json!({ "id": w.holiday, "accountId": w.poor }), INVALID),
        ("stashes.archive", json!({ "id": "999999" }), NOT_FOUND),
        ("stashes.unarchive", json!({ "id": "999999" }), NOT_FOUND),
        ("stashes.delete", json!({ "id": "999999" }), NOT_FOUND),
        ("stashes.show", json!({ "id": "999999" }), NOT_FOUND),
        ("stashes.history", json!({ "id": "999999", "year": "2026" }), NOT_FOUND),
        ("stashes.history", json!({ "id": w.holiday, "year": "9999-" }), INVALID),
        // the order: a stash missing, one twice, one that is not there, one that is archived
        ("stashes.reorder", json!({ "ids": [w.holiday] }), INVALID),
        ("stashes.reorder", json!({ "ids": [w.holiday, w.holiday, other] }), INVALID),
        ("stashes.reorder", json!({ "ids": [w.holiday, other, "999999"] }), INVALID),
        ("stashes.reorder", json!({ "ids": [] }), INVALID),
    ] {
        refuse(&vault, name, body, expected);
    }
    // exactly what is saved is a fine target (and reached), one cent more or none at all
    assert_eq!(ok(&vault, "stashes.update", json!({ "id": w.holiday, "target": "300.00" }))["reached"], true);
    assert_eq!(ok(&vault, "stashes.update", json!({ "id": w.holiday, "target": null }))["target"], Value::Null);
    // a move to an account that has exactly the room
    ok(&vault, "accounts.update", json!({ "id": w.poor, "balance": "300.00" }));
    assert_eq!(ok(&vault, "stashes.update", json!({ "id": w.holiday, "accountId": w.poor }))["free"], "0.00");
    // an account that is archived, and an archived stash
    ok(&vault, "stashes.archive", json!({ "id": other }));
    refuse(&vault, "stashes.reorder", json!({ "ids": [w.holiday, other] }), INVALID);
    ok(&vault, "stashes.reorder", json!({ "ids": [w.holiday] }));
    // a name that an archived stash has is free; bringing that one back is then refused until the other goes
    ok(&vault, "stashes.create", json!({ "name": "Laptop", "accountId": w.alpha }));
    assert_eq!(refuse(&vault, "stashes.unarchive", json!({ "id": other }), NAME_EXISTS), "that name already exists");
}

// ---- adding -----------------------------------------------------------------------------------

#[test]
fn an_add_sets_money_aside_and_no_balance_moves() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    let before = find(&vault, &w.holiday);
    assert_eq!((before["saved"].as_str(), before["free"].as_str(), before["left"].as_str(), before["percent"].as_u64(), before["lastEntry"].as_str()), (Some("300.00"), Some("700.00"), Some("900.00"), Some(25), Some("2026-10-01")));
    let detail = ok(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "100.50", "date": "2026-10-05", "note": "  from the bonus  " }));
    assert_eq!(keys(&detail), ["entries", "stash"]);
    let entry = &detail["entries"][0];
    assert_eq!(keys(entry), ENTRY_KEYS);
    assert_eq!((entry["kind"].as_str(), entry["amount"].as_str(), entry["date"].as_str(), entry["note"].as_str(), entry["savedAfter"].as_str(), entry["stashId"].as_str()), (Some("ADD"), Some("100.50"), Some("2026-10-05"), Some("from the bonus"), Some("400.50"), Some(w.holiday.as_str())), "the note is trimmed");
    assert_eq!((entry["transferId"].clone(), entry["otherAccountId"].clone(), entry["otherAccount"].clone()), (Value::Null, Value::Null, Value::Null), "no transfer unless it is asked for");
    let stash = &detail["stash"];
    assert_eq!((stash["saved"].as_str(), stash["free"].as_str(), stash["left"].as_str(), stash["percent"].as_u64(), stash["entries"].as_u64(), stash["lastEntry"].as_str()), (Some("400.50"), Some("599.50"), Some("799.50"), Some(33), Some(2), Some("2026-10-05")));
    // the money stays in the account: the balance is what it was, and the account says what is set aside
    assert_eq!(balance(&vault, &w.alpha), "1000.00");
    let account = ok(&vault, "accounts.list", json!({})).as_array().unwrap().iter().find(|a| a["id"] == w.alpha.as_str()).unwrap().clone();
    assert_eq!((account["setAside"].as_str(), account["free"].as_str()), (Some("400.50"), Some("599.50")));
    assert_eq!(transfer_count(&vault), 0);
    // the date is today if left out, the note is none if empty
    let today = ok(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "1.00", "note": "" }));
    assert_eq!((today["entries"][0]["date"].as_str(), today["entries"][0]["note"].clone()), (Some(TODAY), Value::Null));
    // a day long ago counts in what is saved
    let old = ok(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "1999-01-01" }));
    assert_eq!(old["stash"]["saved"], "402.50");
    assert_eq!(old["entries"].as_array().unwrap().last().unwrap()["date"], "1999-01-01", "the oldest is last");
    assert_eq!(old["entries"].as_array().unwrap().last().unwrap()["savedAfter"], "1.00", "and its saved runs from the oldest");
}

#[test]
fn an_add_is_limited_by_what_is_free_and_by_the_target_to_the_cent() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    // 700.00 is free in Alpha and 900.00 is left to the target of 1200.00
    refuse(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "700.01", "date": "2026-10-02" }), INVALID);
    assert_eq!(ok(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "700.00", "date": "2026-10-02" }))["stash"]["free"], "0.00", "exactly what is free");
    refuse(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "0.01", "date": "2026-10-02" }), INVALID);
    // a stash that is in an account with more than its target is limited by the target
    let rich = account(&vault, "Rich", "5000.00");
    let car = stash(&vault, "Car", &rich, json!({ "target": "1000.00" }));
    refuse(&vault, "stashes.add", json!({ "id": car, "amount": "1000.01", "date": "2026-10-02" }), INVALID);
    let reached = ok(&vault, "stashes.add", json!({ "id": car, "amount": "1000.00", "date": "2026-10-02" }));
    assert_eq!((reached["stash"]["reached"].as_bool(), reached["stash"]["left"].as_str(), reached["stash"]["percent"].as_u64()), (Some(true), Some("0.00"), Some(100)));
    refuse(&vault, "stashes.add", json!({ "id": car, "amount": "0.01", "date": "2026-10-02" }), INVALID);
    // one without a target is limited by what is free alone: 5000.00 less the car's 1000.00
    let open = stash(&vault, "Open", &rich, json!({}));
    refuse(&vault, "stashes.add", json!({ "id": open, "amount": "4000.01", "date": "2026-10-02" }), INVALID);
    assert_eq!(ok(&vault, "stashes.add", json!({ "id": open, "amount": "4000.00", "date": "2026-10-02" }))["stash"]["percent"], Value::Null);
    // two stashes in one account: the second cannot take what the first holds
    let second = stash(&vault, "Second", &w.beta, json!({}));
    let first = stash(&vault, "First", &w.beta, json!({}));
    add(&vault, &first, "300.00");
    refuse(&vault, "stashes.add", json!({ "id": second, "amount": "200.01", "date": "2026-10-02" }), INVALID);
    add(&vault, &second, "200.00");
    // spending after setting aside leaves less free than is set aside: nothing is refused but an add
    ok(&vault, "accounts.update", json!({ "id": w.beta, "balance": "100.00" }));
    let low = find(&vault, &first);
    assert_eq!(low["free"], "-400.00", "the stashes hold more than the account has");
    refuse(&vault, "stashes.add", json!({ "id": first, "amount": "0.01", "date": "2026-10-02" }), INVALID);
    assert_eq!(ok(&vault, "stashes.take", json!({ "id": first, "amount": "300.00", "date": "2026-10-03" }))["stash"]["saved"], "0.00");
}

#[test]
fn an_entry_is_never_dated_after_today_and_a_note_has_a_length() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    for (name, body) in [
        ("stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-15" })),
        ("stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2099-01-01" })),
        ("stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-02-30" })),
        ("stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-13-01" })),
        ("stashes.take", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-15" })),
        ("stashes.take", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-02-30" })),
        ("stashes.amend", json!({ "id": w.first, "date": "2026-10-15" })),
        ("stashes.amend", json!({ "id": w.first, "date": "2026-02-30" })),
        ("stashes.add", json!({ "id": w.holiday, "amount": "1.00", "note": "n".repeat(201) })),
        ("stashes.take", json!({ "id": w.holiday, "amount": "1.00", "note": "n".repeat(201) })),
        ("stashes.amend", json!({ "id": w.first, "note": "n".repeat(201) })),
        ("stashes.add", json!({ "id": w.holiday, "amount": "1.00", "note": "a\u{0}b" })),
        ("stashes.add", json!({ "id": w.holiday, "amount": "0.00" })),
        ("stashes.take", json!({ "id": w.holiday, "amount": "0.00" })),
        ("stashes.amend", json!({ "id": w.first, "amount": "0.00" })),
    ] {
        refuse(&vault, name, body, INVALID);
    }
    // today, and a note of 200 characters, are fine; a note may have line breaks
    ok(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-14", "note": "n".repeat(200) }));
    assert_eq!(ok(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "1.00", "note": "one\ntwo" }))["entries"][0]["note"], "one\ntwo");
    // today follows the time zone set in the app, not the computer's
    let late = at("2026-10-14T23:30:00Z");
    let made = run_at(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "1.00" }), late).unwrap();
    assert_eq!(made["entries"][0]["date"], "2026-10-15", "it is half past midnight on the 15th in Dublin");
    assert!(run_at(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-16" }), late).is_err());
    assert!(run_at(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-15" }), late).is_ok());
}

#[test]
fn an_add_with_a_from_account_makes_the_transfer_in_the_same_step_and_a_refusal_makes_none() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    // an account that holds nothing has nothing free: the transfer makes the room
    let empty = account(&vault, "Empty", "0.00");
    let jar = stash(&vault, "Jar", &empty, json!({}));
    refuse(&vault, "stashes.add", json!({ "id": jar, "amount": "100.00", "date": "2026-10-05" }), INVALID);
    let made = ok(&vault, "stashes.add", json!({ "id": jar, "amount": "100.00", "date": "2026-10-05", "note": "from Alpha", "fromAccountId": w.alpha }));
    let entry = &made["entries"][0];
    assert_eq!(keys(entry), ENTRY_KEYS);
    assert_eq!((entry["kind"].as_str(), entry["amount"].as_str(), entry["note"].as_str(), entry["savedAfter"].as_str()), (Some("ADD"), Some("100.00"), Some("from Alpha"), Some("100.00")));
    assert!(is_id(entry["transferId"].as_str().unwrap()));
    assert_eq!((entry["otherAccountId"].as_str(), entry["otherAccount"].as_str()), (Some(w.alpha.as_str()), Some("Alpha")), "the account the money came from");
    assert_eq!((made["stash"]["saved"].as_str(), made["stash"]["free"].as_str()), (Some("100.00"), Some("0.00")));
    // the transfer is a transfer like any other: the money moved
    assert_eq!((balance(&vault, &w.alpha).as_str(), balance(&vault, &empty).as_str()), ("900.00", "100.00"));
    let page = ok(&vault, "transfers.list", json!({}));
    assert_eq!(page["total"], 1);
    let transfer = &page["transfers"][0];
    assert_eq!((transfer["id"].clone(), transfer["fromAccountId"].as_str(), transfer["toAccountId"].as_str(), transfer["amount"].as_str(), transfer["date"].as_str(), transfer["note"].as_str()), (entry["transferId"].clone(), Some(w.alpha.as_str()), Some(empty.as_str()), Some("100.00"), Some("2026-10-05"), Some("from Alpha")));

    // each refusal writes neither the entry nor the transfer
    // the stash's own account, an account that is archived, one in another currency, one that is not there
    for from in [empty.as_str(), w.gone.as_str(), w.lei.as_str(), "999999"] {
        refuse(&vault, "stashes.add", json!({ "id": jar, "amount": "1.00", "date": "2026-10-05", "fromAccountId": from }), INVALID);
    }
    // more than is left to the target, even with the transfer: 900.00 is left of 1200.00, and 700.00 free before the transfer
    refuse(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "900.01", "date": "2026-10-05", "fromAccountId": w.beta }), INVALID);
    // a date after today, and a stash that is archived
    refuse(&vault, "stashes.add", json!({ "id": jar, "amount": "1.00", "date": "2026-10-15", "fromAccountId": w.alpha }), INVALID);
    ok(&vault, "stashes.archive", json!({ "id": jar }));
    refuse(&vault, "stashes.add", json!({ "id": jar, "amount": "1.00", "date": "2026-10-05", "fromAccountId": w.alpha }), INVALID);
    assert_eq!(transfer_count(&vault), 1);
    // the entry is checked after the transfer, so a stash that fits only because of the transfer is fine: 700.00 is free and 800.00 comes in
    let before = dump(&vault);
    assert_eq!(ok(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "800.00", "date": "2026-10-06", "fromAccountId": w.beta }))["stash"]["saved"], "1100.00", "700.00 free and 800.00 brought in");
    assert_ne!(dump(&vault), before);
    assert_eq!((balance(&vault, &w.alpha).as_str(), balance(&vault, &w.beta).as_str()), ("1700.00", "-300.00"), "a transfer may leave an account in the red, as it may when it is made on its own");
}

#[test]
fn a_take_frees_money_and_can_send_it_on_with_a_transfer() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    refuse(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "300.01", "date": "2026-10-06" }), INVALID);
    let taken = ok(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "50.00", "date": "2026-10-06", "note": "deposit" }));
    let entry = &taken["entries"][0];
    assert_eq!(keys(entry), ENTRY_KEYS);
    assert_eq!((entry["kind"].as_str(), entry["amount"].as_str(), entry["date"].as_str(), entry["note"].as_str(), entry["savedAfter"].as_str()), (Some("TAKE"), Some("50.00"), Some("2026-10-06"), Some("deposit"), Some("250.00")), "the amount is above zero and the kind says which way it went");
    assert_eq!((entry["transferId"].clone(), entry["otherAccountId"].clone()), (Value::Null, Value::Null));
    assert_eq!((taken["stash"]["saved"].as_str(), taken["stash"]["free"].as_str(), taken["stash"]["entries"].as_u64()), (Some("250.00"), Some("750.00"), Some(2)));
    assert_eq!(balance(&vault, &w.alpha), "1000.00", "the money never left the account");
    assert_eq!(transfer_count(&vault), 0);

    // sent on: a transfer out of the account is made with it
    let sent = ok(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "25.00", "date": "2026-10-07", "note": "to Beta", "toAccountId": w.beta }));
    let entry = &sent["entries"][0];
    assert_eq!((entry["kind"].as_str(), entry["savedAfter"].as_str(), entry["otherAccountId"].as_str(), entry["otherAccount"].as_str()), (Some("TAKE"), Some("225.00"), Some(w.beta.as_str()), Some("Beta")));
    assert_eq!((balance(&vault, &w.alpha).as_str(), balance(&vault, &w.beta).as_str()), ("975.00", "525.00"));
    let page = ok(&vault, "transfers.list", json!({}));
    let transfer = &page["transfers"][0];
    assert_eq!((page["total"].as_u64(), transfer["id"].clone(), transfer["fromAccountId"].as_str(), transfer["toAccountId"].as_str(), transfer["amount"].as_str(), transfer["note"].as_str()), (Some(1), entry["transferId"].clone(), Some(w.alpha.as_str()), Some(w.beta.as_str()), Some("25.00"), Some("to Beta")));
    // refusals: the stash's own account, an account that is archived, one in another currency, one that is not there
    for to in [w.alpha.as_str(), w.gone.as_str(), w.lei.as_str(), "999999"] {
        refuse(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-07", "toAccountId": to }), INVALID);
    }
    refuse(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "225.01", "date": "2026-10-07", "toAccountId": w.beta }), INVALID);
    // exactly what is saved, and then nothing is
    let empty = ok(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "225.00", "date": "2026-10-08" }));
    assert_eq!((empty["stash"]["saved"].as_str(), empty["stash"]["percent"].as_u64(), empty["stash"]["reached"].as_bool()), (Some("0.00"), Some(0), Some(false)));
    refuse(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "0.01", "date": "2026-10-08" }), INVALID);
}

#[test]
fn an_entry_is_amended_by_the_delta_rule_and_its_kind_never_changes() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    let taken = ok(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "100.00", "date": "2026-10-02", "note": "deposit" }));
    let take = id_of(&taken["entries"][0]);
    assert_eq!(taken["stash"]["saved"], "200.00");
    // an add edited down below what was taken since
    refuse(&vault, "stashes.amend", json!({ "id": w.first, "amount": "99.99" }), INVALID);
    let down = ok(&vault, "stashes.amend", json!({ "id": w.first, "amount": "250.00" }));
    assert_eq!(down["stash"]["saved"], "150.00");
    let first = down["entries"].as_array().unwrap().iter().find(|e| e["id"] == w.first.as_str()).unwrap();
    assert_eq!((first["kind"].as_str(), first["amount"].as_str(), first["note"].as_str(), first["date"].as_str()), (Some("ADD"), Some("250.00"), Some("first"), Some("2026-10-01")), "an add stays an add and the rest is what it was");
    // a take edited up beyond what is saved
    refuse(&vault, "stashes.amend", json!({ "id": take, "amount": "250.01" }), INVALID);
    let up = ok(&vault, "stashes.amend", json!({ "id": take, "amount": "250.00" }));
    assert_eq!(up["stash"]["saved"], "0.00");
    let kept = up["entries"].as_array().unwrap().iter().find(|e| e["id"] == take.as_str()).unwrap();
    assert_eq!((kept["kind"].as_str(), kept["amount"].as_str()), (Some("TAKE"), Some("250.00")), "a take stays a take: the amount is a size");
    // an add edited up beyond what is free: 1000.00 is free of Alpha with nothing set aside
    refuse(&vault, "stashes.amend", json!({ "id": w.first, "amount": "1250.01" }), INVALID);
    assert_eq!(ok(&vault, "stashes.amend", json!({ "id": w.first, "amount": "1250.00" }))["stash"]["saved"], "1000.00");
    // beyond the target: 1200.00 of the car
    let rich = account(&vault, "Rich", "5000.00");
    let car = stash(&vault, "Car", &rich, json!({ "target": "1000.00" }));
    let entry = id_of(&add(&vault, &car, "800.00")["entries"][0]);
    refuse(&vault, "stashes.amend", json!({ "id": entry, "amount": "1000.01" }), INVALID);
    assert_eq!(ok(&vault, "stashes.amend", json!({ "id": entry, "amount": "1000.00" }))["stash"]["reached"], true);
    // the note and the date: only what is named changes, null clears the note, nothing named changes nothing
    let note = ok(&vault, "stashes.amend", json!({ "id": take, "note": null, "date": "2026-09-30" }));
    let kept = note["entries"].as_array().unwrap().iter().find(|e| e["id"] == take.as_str()).unwrap();
    assert_eq!((kept["note"].clone(), kept["date"].as_str(), kept["amount"].as_str()), (Value::Null, Some("2026-09-30"), Some("250.00")));
    let before = dump(&vault);
    assert_eq!(ok(&vault, "stashes.amend", json!({ "id": take }))["stash"]["saved"], "1000.00");
    assert_eq!(dump(&vault), before);
    // an entry that is not there
    refuse(&vault, "stashes.amend", json!({ "id": "999999", "note": "x" }), NOT_FOUND);
    // the transfer that came with an entry is not changed by amending the entry
    let linked = ok(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "10.00", "date": "2026-10-03", "fromAccountId": w.beta }));
    let (linked_entry, transfer) = (id_of(&linked["entries"][0]), linked["entries"][0]["transferId"].as_str().unwrap().to_string());
    // 10.00 more, and a new day and note
    let changed = ok(&vault, "stashes.amend", json!({ "id": linked_entry, "amount": "5.00", "date": "2026-10-04", "note": "less" }));
    assert_eq!(changed["entries"][0]["transferId"], transfer.as_str());
    let page = ok(&vault, "transfers.list", json!({ "accountId": w.beta }));
    assert_eq!((page["transfers"][0]["amount"].as_str(), page["transfers"][0]["date"].as_str(), page["transfers"][0]["note"].clone()), (Some("10.00"), Some("2026-10-03"), Value::Null));
}

#[test]
fn an_entry_is_erased_by_the_delta_rule_and_its_transfer_stays_unless_it_is_asked_to_go() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    let taken = ok(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "100.00", "date": "2026-10-02" }));
    let take = id_of(&taken["entries"][0]);
    // an add whose money was taken out since: the stash would hold less than nothing
    refuse(&vault, "stashes.erase", json!({ "id": w.first }), INVALID);
    refuse(&vault, "stashes.erase", json!({ "id": w.first, "deleteTransfer": true }), INVALID);
    // a take, though, gives the money back
    let erased = ok(&vault, "stashes.erase", json!({ "id": take }));
    assert_eq!(keys(&erased), ["entries", "stash"]);
    assert_eq!((entry_ids(&erased), erased["stash"]["saved"].as_str(), erased["stash"]["entries"].as_u64()), (vec![w.first.clone()], Some("300.00"), Some(1)));
    refuse(&vault, "stashes.erase", json!({ "id": take }), NOT_FOUND);
    // a take whose money was set aside elsewhere since: giving it back would put more in the stash than is free
    let other = stash(&vault, "Other", &w.alpha, json!({}));
    let out = id_of(&ok(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "300.00", "date": "2026-10-03" }))["entries"][0]);
    let set_aside = id_of(&add(&vault, &other, "1000.00")["entries"][0]);
    refuse(&vault, "stashes.erase", json!({ "id": out }), INVALID);
    // an entry that came with no transfer is erased with deleteTransfer as it is without: nothing else goes
    assert_eq!(ok(&vault, "stashes.erase", json!({ "id": set_aside, "deleteTransfer": true }))["stash"]["saved"], "0.00");
    assert_eq!(ok(&vault, "stashes.erase", json!({ "id": out }))["stash"]["saved"], "300.00");

    // four adds that came with a transfer from Alpha: the transfer stays when the entry goes, unless deleteTransfer is true
    let pot = stash(&vault, "Pot", &w.beta, json!({}));
    let mut linked = Vec::new();
    for amount in ["10.00", "20.00", "30.00", "40.00"] {
        let made = ok(&vault, "stashes.add", json!({ "id": pot, "amount": amount, "date": "2026-10-04", "fromAccountId": w.alpha }));
        linked.push((id_of(&made["entries"][0]), made["entries"][0]["transferId"].as_str().unwrap().to_string()));
    }
    assert_eq!(transfer_count(&vault), 4);
    let transfers = |vault: &Vault| -> Vec<String> { ok(vault, "transfers.list", json!({})).get("transfers").unwrap().as_array().unwrap().iter().map(id_of).collect() };
    assert_eq!(ok(&vault, "stashes.erase", json!({ "id": linked[0].0 }))["stash"]["saved"], "90.00");
    assert_eq!(transfer_count(&vault), 4, "the transfer stays when it is not asked to go");
    assert_eq!(ok(&vault, "stashes.erase", json!({ "id": linked[1].0, "deleteTransfer": false }))["stash"]["saved"], "70.00");
    assert_eq!(transfer_count(&vault), 4);
    assert_eq!(ok(&vault, "stashes.erase", json!({ "id": linked[2].0, "deleteTransfer": true }))["stash"]["saved"], "40.00");
    assert_eq!(transfer_count(&vault), 3);
    assert!(!transfers(&vault).contains(&linked[2].1) && transfers(&vault).contains(&linked[0].1) && transfers(&vault).contains(&linked[1].1) && transfers(&vault).contains(&linked[3].1));
    assert_eq!(balance(&vault, &w.beta), "570.00", "the two that stay and the one that is left: 20.00 + 10.00 + 40.00 came in, the erased 30.00 did not stay");
    // a transfer that is deleted on its own leaves the entry, which forgets it
    ok(&vault, "transfers.delete", json!({ "id": linked[3].1 }));
    let kept = show(&vault, &pot);
    assert_eq!(kept["entries"].as_array().unwrap().len(), 1);
    assert_eq!((kept["entries"][0]["amount"].as_str(), kept["entries"][0]["transferId"].clone(), kept["entries"][0]["otherAccountId"].clone(), kept["entries"][0]["otherAccount"].clone()), (Some("40.00"), Value::Null, Value::Null, Value::Null));
    // the stash that is deleted takes its entries and leaves the transfers that came with them
    ok(&vault, "stashes.delete", json!({ "id": pot }));
    assert_eq!(transfer_count(&vault), 2);
}

#[test]
fn an_archived_stash_takes_no_money_and_can_be_emptied_and_so_with_an_archived_account() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    ok(&vault, "stashes.archive", json!({ "id": w.holiday }));
    // no more money in, with or without a transfer, by an add or by editing an entry up; changing the stash and ordering it are refused too
    refuse(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-02" }), INVALID);
    refuse(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-02", "fromAccountId": w.beta }), INVALID);
    refuse(&vault, "stashes.amend", json!({ "id": w.first, "amount": "301.00" }), INVALID);
    refuse(&vault, "stashes.update", json!({ "id": w.holiday, "name": "Trip" }), INVALID);
    // but out: a take, a take with a transfer, a note, an entry erased
    let taken = ok(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "100.00", "date": "2026-10-02" }));
    assert_eq!((taken["stash"]["archived"].as_bool(), taken["stash"]["saved"].as_str()), (Some(true), Some("200.00")));
    let sent = ok(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "50.00", "date": "2026-10-02", "toAccountId": w.beta }));
    assert_eq!(sent["stash"]["saved"], "150.00");
    assert_eq!(ok(&vault, "stashes.amend", json!({ "id": w.first, "note": "kept" }))["stash"]["saved"], "150.00");
    assert_eq!(ok(&vault, "stashes.amend", json!({ "id": id_of(&taken["entries"][0]), "amount": "120.00" }))["stash"]["saved"], "130.00", "more out is fine");
    refuse(&vault, "stashes.amend", json!({ "id": id_of(&taken["entries"][0]), "amount": "80.00" }), INVALID);
    assert_eq!(ok(&vault, "stashes.erase", json!({ "id": id_of(&sent["entries"][0]) }))["stash"]["saved"], "180.00", "less out would be money in");
    ok(&vault, "stashes.unarchive", json!({ "id": w.holiday }));

    // an account that is archived: the same, and the stash can be moved away
    ok(&vault, "accounts.archive", json!({ "id": w.alpha }));
    let held = find(&vault, &w.holiday);
    assert_eq!((held["accountArchived"].as_bool(), held["archived"].as_bool()), (Some(true), Some(false)));
    refuse(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": "2026-10-02" }), INVALID);
    refuse(&vault, "stashes.amend", json!({ "id": w.first, "amount": "301.00" }), INVALID);
    let sent = ok(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "30.00", "date": "2026-10-03", "toAccountId": w.beta }));
    assert_eq!(sent["stash"]["saved"], "150.00", "money can still leave an archived account");
    assert_eq!(ok(&vault, "stashes.update", json!({ "id": w.holiday, "accountId": w.beta }))["accountArchived"], false);
}

// ---- the chart and the totals -----------------------------------------------------------------

#[test]
fn the_history_is_what_the_stash_held_at_the_end_of_each_month_up_to_today() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    ok(&vault, "stashes.add", json!({ "id": w.holiday, "amount": "100.00", "date": "2026-08-15" }));
    ok(&vault, "stashes.take", json!({ "id": w.holiday, "amount": "50.00", "date": "2026-09-30" }));
    let points: Vec<(String, String)> = ok(&vault, "stashes.history", json!({ "id": w.holiday, "year": "2026" })).as_array().unwrap().iter().map(|p| (p["month"].as_str().unwrap().to_string(), p["saved"].as_str().unwrap().to_string())).collect();
    let expected: Vec<(&str, &str)> = vec![
        ("2026-01", "0.00"), ("2026-02", "0.00"), ("2026-03", "0.00"), ("2026-04", "0.00"), ("2026-05", "0.00"), ("2026-06", "0.00"), ("2026-07", "0.00"), ("2026-08", "100.00"), ("2026-09", "50.00"), ("2026-10", "350.00"),
    ];
    assert_eq!(points.iter().map(|(m, s)| (m.as_str(), s.as_str())).collect::<Vec<_>>(), expected, "the first add (300.00, 1 October) is in this month's point");
    assert_eq!(ok(&vault, "stashes.history", json!({ "id": w.holiday, "year": "2027" })), json!([]), "a year that has not begun has no point");
    assert_eq!(ok(&vault, "stashes.history", json!({ "id": w.holiday, "year": "1999" })).as_array().unwrap().len(), 12, "a year that is over has twelve");
}

#[test]
fn the_summary_adds_up_the_stashes_that_are_not_archived_in_the_apps_currency() {
    let (_dir, vault) = fresh();
    assert_eq!(ok(&vault, "stashes.summary", json!({})), json!({ "stashes": 0, "saved": "0.00", "target": "0.00", "reached": 0 }));
    let w = world(&vault);
    let summary = ok(&vault, "stashes.summary", json!({}));
    assert_eq!(summary, json!({ "stashes": 1, "saved": "300.00", "target": "1200.00", "reached": 0 }));
    // a stash in lei, five to the euro: 100.00 lei of 500.00 is 20.00 of 100.00 euro
    let gift = stash(&vault, "Gift", &w.lei, json!({ "target": "500.00" }));
    add(&vault, &gift, "100.00");
    assert_eq!(ok(&vault, "stashes.summary", json!({})), json!({ "stashes": 2, "saved": "320.00", "target": "1300.00", "reached": 0 }));
    add(&vault, &gift, "400.00");
    assert_eq!(ok(&vault, "stashes.summary", json!({})), json!({ "stashes": 2, "saved": "400.00", "target": "1300.00", "reached": 1 }));
    // the archived are left out of every figure, and what they hold stays set aside in the account
    ok(&vault, "stashes.archive", json!({ "id": w.holiday }));
    assert_eq!(ok(&vault, "stashes.summary", json!({})), json!({ "stashes": 1, "saved": "100.00", "target": "100.00", "reached": 1 }));
    assert_eq!(find(&vault, &w.holiday)["free"], "700.00");
}

// ---- arguments --------------------------------------------------------------------------------

#[test]
fn an_argument_of_the_wrong_kind_or_name_is_refused_by_name() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    let msg = |name: &str, body: Value| refuse(&vault, name, body, INVALID);
    let money = |name: &str| format!("`{name}` must be an amount as text with two decimals, like \"12.50\"");
    // money is text with two decimals and never a number
    for value in [json!(1.0), json!(5), json!(null), json!(true), json!("1"), json!("1.5"), json!("1.234"), json!("-1.00"), json!("1,00"), json!(""), json!("12345678901.00"), json!(["1.00"])] {
        assert_eq!(msg("stashes.add", json!({ "id": w.holiday, "amount": value })), money("amount"), "{value}");
        assert_eq!(msg("stashes.take", json!({ "id": w.holiday, "amount": value })), money("amount"), "{value}");
        assert_eq!(msg("stashes.amend", json!({ "id": w.first, "amount": value })), money("amount"), "{value}");
        assert_eq!(msg("stashes.create", json!({ "name": "N", "accountId": w.alpha, "target": value })), money("target"), "{value}");
        if !value.is_null() {
            assert_eq!(msg("stashes.update", json!({ "id": w.holiday, "target": value })), money("target"), "{value}");
        }
    }
    // an id is digits with no leading zero, as a string
    for value in [json!(5), json!("0"), json!("01"), json!("-1"), json!("1.5"), json!(""), json!(null), json!(["1"]), json!("1e3"), json!(" 1")] {
        for name in ["stashes.show", "stashes.update", "stashes.archive", "stashes.unarchive", "stashes.delete", "stashes.add", "stashes.take", "stashes.amend", "stashes.erase"] {
            let mut body = json!({ "id": value });
            if matches!(name, "stashes.add" | "stashes.take") {
                body["amount"] = json!("1.00");
            }
            assert_eq!(msg(name, body), "`id` must be an id as the app lists it", "{name} {value}");
        }
        assert_eq!(msg("stashes.list", json!({ "accountId": value })), "`accountId` must be an id as the app lists it", "{value}");
        assert_eq!(msg("stashes.create", json!({ "name": "N", "accountId": value })), "`accountId` must be an id as the app lists it", "{value}");
        assert_eq!(msg("stashes.update", json!({ "id": w.holiday, "accountId": value })), "`accountId` must be an id as the app lists it", "{value}");
        assert_eq!(msg("stashes.add", json!({ "id": w.holiday, "amount": "1.00", "fromAccountId": value })), "`fromAccountId` must be an id as the app lists it", "{value}");
        assert_eq!(msg("stashes.take", json!({ "id": w.holiday, "amount": "1.00", "toAccountId": value })), "`toAccountId` must be an id as the app lists it", "{value}");
    }
    for value in [json!([1, 2]), json!(["0"]), json!("1"), json!(null), json!({}), json!(["1", 2]), json!(["01"])] {
        assert_eq!(msg("stashes.reorder", json!({ "ids": value })), "`ids` must be a list of ids as the app lists them", "{value}");
    }
    assert_eq!(msg("stashes.reorder", json!({ "ids": vec!["1"; 501] })), "`ids` must be a list of ids as the app lists them", "no more than 500");
    // a year has four digits, a date a fixed shape
    for value in [json!(2026), json!("26"), json!("20266"), json!("abcd"), json!(null), json!("")] {
        assert_eq!(msg("stashes.history", json!({ "id": w.holiday, "year": value })), "`year` must be a year like 2026", "{value}");
    }
    for value in [json!("tomorrow"), json!("2026-1-1"), json!("20261014"), json!("2026/10/14"), json!("2026-10-14T00:00:00Z"), json!(20261014), json!(null), json!("")] {
        let date = "`date` must be a date like 2026-10-14";
        assert_eq!(msg("stashes.add", json!({ "id": w.holiday, "amount": "1.00", "date": value })), date, "{value}");
        assert_eq!(msg("stashes.take", json!({ "id": w.holiday, "amount": "1.00", "date": value })), date, "{value}");
        assert_eq!(msg("stashes.amend", json!({ "id": w.first, "date": value })), date, "{value}");
        assert_eq!(msg("stashes.create", json!({ "name": "N", "accountId": w.alpha, "targetDate": value })), "`targetDate` must be a date like 2026-10-14", "{value}");
        if !value.is_null() {
            assert_eq!(msg("stashes.update", json!({ "id": w.holiday, "targetDate": value })), "`targetDate` must be a date like 2026-10-14", "{value}");
        }
    }
    // a switch is a JSON boolean and nothing else
    for value in [json!("true"), json!("false"), json!("yes"), json!(""), json!(1), json!(0), json!(null), json!([true])] {
        assert_eq!(msg("stashes.erase", json!({ "id": w.first, "deleteTransfer": value })), "`deleteTransfer` must be a boolean", "{value}");
    }
    // a text is a string; null is a value only where an argument says so (the amend note, and the target, date and notes of an update)
    for value in [json!(5), json!(true), json!(["a"]), json!({ "a": 1 }), json!(1.5), json!(null)] {
        for (name, body, key) in [
            ("stashes.create", json!({ "accountId": w.alpha }), "name"),
            ("stashes.create", json!({ "name": "N", "accountId": w.alpha }), "notes"),
            ("stashes.update", json!({ "id": w.holiday }), "name"),
            ("stashes.add", json!({ "id": w.holiday, "amount": "1.00" }), "note"),
            ("stashes.take", json!({ "id": w.holiday, "amount": "1.00" }), "note"),
        ] {
            assert_eq!(msg(name, with(body, key, value.clone())), format!("`{key}` must be a string"), "{name} {key} {value}");
        }
        if !value.is_null() {
            assert_eq!(msg("stashes.update", json!({ "id": w.holiday, "notes": value })), "`notes` must be a string", "{value}");
            assert_eq!(msg("stashes.amend", json!({ "id": w.first, "note": value })), "`note` must be a string", "{value}");
        }
    }
    // a missing argument is named
    assert_eq!(msg("stashes.show", json!({})), "missing argument `id`");
    assert_eq!(msg("stashes.history", json!({ "id": w.holiday })), "missing argument `year`");
    assert_eq!(msg("stashes.create", json!({ "accountId": w.alpha })), "missing argument `name`");
    assert_eq!(msg("stashes.create", json!({ "name": "N" })), "missing argument `accountId`", "a stash always lives in an account");
    assert_eq!(msg("stashes.add", json!({ "id": w.holiday })), "missing argument `amount`");
    assert_eq!(msg("stashes.take", json!({ "id": w.holiday })), "missing argument `amount`");
    assert_eq!(msg("stashes.reorder", json!({})), "missing argument `ids`");
    assert_eq!(msg("stashes.erase", json!({})), "missing argument `id`");
    // the names of the arguments are exact, and an argument of another route is not one of this
    for wrong in ["target_date", "TargetDate", "account", "stashId", "kind", "amountMinor", "fromAccountId", "toAccountId", "deleteTransfer", "saved", "free"] {
        assert_eq!(msg("stashes.create", json!({ "name": "N", "accountId": w.alpha, wrong: "x" })), format!("unknown argument `{wrong}`"));
    }
    assert_eq!(msg("stashes.add", json!({ "id": w.holiday, "amount": "1.00", "toAccountId": w.beta })), "unknown argument `toAccountId`", "an add brings money in, a take sends it on");
    assert_eq!(msg("stashes.take", json!({ "id": w.holiday, "amount": "1.00", "fromAccountId": w.beta })), "unknown argument `fromAccountId`");
    assert_eq!(msg("stashes.amend", json!({ "id": w.first, "kind": "TAKE" })), "unknown argument `kind`", "the kind of an entry never changes");
    assert_eq!(msg("stashes.amend", json!({ "id": w.first, "deleteTransfer": true })), "unknown argument `deleteTransfer`");
    assert_eq!(msg("stashes.amend", json!({ "id": w.first, "amount": "-5.00" })), money("amount"), "the amount is a size, never signed");
    assert_eq!(msg("stashes.delete", json!({ "id": w.holiday, "yes": true })), "unknown argument `yes`", "--yes is for the command line");
    assert_eq!(msg("stashes.summary", json!({ "id": "1" })), "unknown argument `id`");
}

/// The same body again with one argument set.
fn with(base: Value, key: &str, value: Value) -> Value {
    let mut body = base;
    body[key] = value;
    body
}

// ---- tokens -----------------------------------------------------------------------------------

struct Http {
    _dir: tempfile::TempDir,
    vault: Arc<Vault>,
    server: ApiServer,
    read: String,
    write: String,
}

fn http() -> Http {
    let (dir, vault) = fresh();
    vault.with_conn(|c| crate::seed::run_for(c, TODAY).map_err(|_| AppError::Internal)).unwrap();
    let (read, write) = (make_token(&vault, Scope::Read), make_token(&vault, Scope::Write));
    let server = ApiServer::start(vault.clone(), routes::ALL, Limits::default()).unwrap();
    Http { _dir: dir, vault, server, read, write }
}

#[test]
fn a_read_token_reads_stashes_and_cannot_write_any_of_them() {
    let h = http();
    let port = h.server.port();
    let before = dump(&h.vault);
    let reads: [(&str, &str); 4] = [("stashes.list", "{}"), ("stashes.show", r#"{"id":"1"}"#), ("stashes.history", r#"{"id":"1","year":"2026"}"#), ("stashes.summary", "{}")];
    for (name, body) in reads {
        let reply = call(port, Some(&h.read), name, body);
        assert_eq!(reply.status, 200, "{name}: {}", reply.body);
        assert_eq!(reply.json()["ok"], true, "{name}");
        assert!(!reply.body.contains(&h.read) && !reply.body.contains(&h.write), "{name}");
    }
    assert_eq!(call(port, Some(&h.read), "stashes.list", "{}").data().as_array().unwrap().len(), 5);
    assert_eq!(call(port, Some(&h.write), "stashes.list", "{}").data().as_array().unwrap().len(), 5, "a write token reads too");

    // every write route: a read token is refused before anything else is looked at, with a valid request and with none
    let world = World { alpha: "2".into(), beta: "1".into(), poor: "3".into(), lei: "6".into(), gone: "5".into(), holiday: "1".into(), first: "1".into() };
    let mut tried = 0;
    for (name, body) in requests(&world).into_iter().filter(|(name, _)| WRITE_ROUTES.contains(name)) {
        for body in [body.to_string(), "{}".to_string(), "not json".to_string()] {
            let refused = call(port, Some(&h.read), name, &body);
            assert_eq!((refused.status, refused.code().as_str()), (403, "forbidden"), "{name} {body}");
            assert!(refused.message().contains("read-only"), "{name}: {}", refused.message());
        }
        // with a write token the route is reached: an empty body is a 400 for what is missing
        let reached = call(port, Some(&h.write), name, "{}");
        assert_eq!((reached.status, reached.code().as_str()), (400, "invalid_input"), "{name}: {}", reached.body);
        tried += 1;
    }
    assert_eq!(tried, 10);
    assert_eq!(dump(&h.vault), before, "a read token changed something");
    // and without any token, or with a wrong one, nothing is told
    for name in READ_ROUTES.iter().chain(WRITE_ROUTES.iter()) {
        assert_eq!(call(port, None, name, "{}").code(), "unauthorized", "{name}");
        assert_eq!(call(port, Some(&format!("mny_{}", "0".repeat(64))), name, "{}").code(), "unauthorized", "{name}");
    }
}

#[test]
fn a_write_token_makes_sets_aside_and_takes_out_over_http_and_the_read_token_sees_it() {
    let h = http();
    let port = h.server.port();
    let post = |token: &str, route: &str, body: Value| {
        let reply = call(port, Some(token), route, &body.to_string());
        assert_eq!(reply.status, 200, "{route}: {}", reply.body);
        assert_eq!(reply.json()["ok"], true);
        reply.data()
    };
    let account = post(&h.write, "accounts.create", json!({ "name": "Trip money", "kind": "SAVINGS", "openingBalance": "500.00", "openingDate": "2026-01-01" }));
    let made = post(&h.write, "stashes.create", json!({ "name": "Rome", "accountId": id_of(&account), "target": "300.00", "targetDate": "2027-12-31" }));
    assert_eq!(keys(&made), STASH_KEYS);
    let id = id_of(&made);
    let added = post(&h.write, "stashes.add", json!({ "id": id, "amount": "120.00", "date": "2026-10-01", "note": "start" }));
    assert_eq!(keys(&added), ["entries", "stash"]);
    assert_eq!((added["stash"]["saved"].as_str(), added["stash"]["free"].as_str()), (Some("120.00"), Some("380.00")));
    let taken = post(&h.write, "stashes.take", json!({ "id": id, "amount": "20.00", "date": "2026-10-02" }));
    assert_eq!(taken["stash"]["saved"], "100.00");
    let (take, add) = (id_of(&taken["entries"][0]), id_of(&taken["entries"][1]));
    let amended = post(&h.write, "stashes.amend", json!({ "id": add, "note": null, "amount": "130.00" }));
    assert_eq!(amended["stash"]["saved"], "110.00");
    // the read token sees what the write token did
    let seen = post(&h.read, "stashes.show", json!({ "id": id }));
    assert_eq!(seen, amended);
    assert_eq!(post(&h.read, "stashes.list", json!({ "accountId": id_of(&account) })).as_array().unwrap().len(), 1);
    let erased = post(&h.write, "stashes.erase", json!({ "id": take, "deleteTransfer": false }));
    assert_eq!((erased["stash"]["saved"].as_str(), erased["entries"].as_array().unwrap().len()), (Some("130.00"), 1));
    assert_eq!(post(&h.write, "stashes.archive", json!({ "id": id }))["archived"], true);
    assert_eq!(post(&h.write, "stashes.unarchive", json!({ "id": id }))["archived"], false);
    let active: Vec<String> = post(&h.read, "stashes.list", json!({})).as_array().unwrap().iter().filter(|s| s["archived"] == false).map(id_of).rev().collect();
    assert_eq!(post(&h.write, "stashes.reorder", json!({ "ids": active })), Value::Null);
    assert_eq!(post(&h.write, "stashes.delete", json!({ "id": id })), Value::Null);
    // a refusal over HTTP: the status and code of the note, and the server is still up
    let taken = call(port, Some(&h.write), "stashes.create", r#"{"name":"rainy day","accountId":"2"}"#);
    assert_eq!((taken.status, taken.code().as_str(), taken.message().as_str()), (409, "name_exists", "that name already exists"));
    let more = call(port, Some(&h.write), "stashes.add", r#"{"id":"2","amount":"9999999999.00","date":"2026-10-01"}"#);
    assert_eq!((more.status, more.code().as_str(), more.message().as_str()), (400, "invalid_input", "invalid input"));
    assert_eq!(call(port, Some(&h.write), "stashes.show", r#"{"id":"999999"}"#).status, 404);
    assert_eq!(call(port, Some(&h.write), "stashes.erase", r#"{"id":"999999"}"#).code(), "not_found");
    assert_eq!(call(port, Some(&h.read), "status", "").status, 200);
    assert_eq!(h.server.panics(), 0);
}

#[test]
fn a_locked_app_gives_nothing_from_any_stash_route() {
    let (_dir, vault) = seeded();
    vault.lock();
    for (name, body) in requests(&World { alpha: "2".into(), beta: "1".into(), poor: "3".into(), lei: "6".into(), gone: "5".into(), holiday: "1".into(), first: "1".into() }) {
        let e = run(&vault, name, body).unwrap_err();
        assert_eq!((e.status, e.code), (423, "locked"), "{name}");
    }
}
