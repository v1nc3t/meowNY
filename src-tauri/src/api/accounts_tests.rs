//! The routes for accounts and transfers, and the `accountId` argument of the routes that make or change a transaction, a recurring item or
//! a ledger entry. What matters: every answer has the shape of section 6 of the design note (ids and money as text), a read token reads
//! and cannot write, every odd or wrong argument is refused by name and every refusal leaves the data exactly as it was, and the odd
//! transitions (archive twice, delete an account with a row, rename to a taken name in another case) answer as the note says.

use std::sync::Arc;

use serde_json::{json, Value};

use super::confirmed_tests::rig;
use super::read_tests::{at, ok, refused, route, run, run_at, seeded, TODAY};
use super::routes::{self, Kind};
use super::tests::{call, make_token, new_vault};
use super::*;
use crate::backup;
use crate::tokens::Scope;

const INVALID: (u16, &str) = (400, "invalid_input");
const NOT_FOUND: (u16, &str) = (404, "not_found");
const NAME_EXISTS: (u16, &str) = (409, "name_exists");

const ACCOUNT_KEYS: [&str; 18] = [
    "archived", "available", "balance", "bic", "changeThisMonth", "entries", "group", "id", "includeInNetWorth", "isMain", "kind", "lastActivity", "name", "notes", "number", "openingBalance", "openingDate",
    "virtualBalance",
];
const TRANSFER_KEYS: [&str; 6] = ["amount", "date", "fromAccountId", "id", "note", "toAccountId"];

const READ_ROUTES: [&str; 5] = ["accounts.list", "accounts.show", "accounts.history", "accounts.summary", "transfers.list"];
const WRITE_ROUTES: [&str; 9] = [
    "accounts.create",
    "accounts.update",
    "accounts.archive",
    "accounts.unarchive",
    "accounts.delete",
    "accounts.reorder",
    "transfers.create",
    "transfers.update",
    "transfers.delete",
];
/// The routes that already existed and now take `accountId`.
const WITH_ACCOUNT: [&str; 8] =
    ["transactions.create", "transactions.update", "transactions.list", "recurring.create", "recurring.update", "ledger.add", "ledger.update", "people.settle"];

// ---- helpers ----------------------------------------------------------------------------------

/// A set-up vault with one account (the main one), no categories and no friends.
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

fn cents(text: &str) -> i64 {
    assert!(is_money(text), "{text:?}");
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text),
    };
    let (whole, cents) = digits.split_once('.').unwrap();
    sign * (whole.parse::<i64>().unwrap() * 100 + cents.parse::<i64>().unwrap())
}

/// A message that says what was wrong with the request and nothing of the database.
fn plain(message: &str) {
    let message = message.to_lowercase();
    for leak in ["sqlite", "select", "insert", "table", "column", "constraint", "foreign key", "unique", "opening_balance", "account_id", "accounts", "transfers", "rusqlite"] {
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

fn account(vault: &Vault, name: &str) -> String {
    id_of(&ok(vault, "accounts.create", json!({ "name": name, "kind": "CHECKING" })))
}

fn archived(vault: &Vault, name: &str) -> String {
    let id = account(vault, name);
    ok(vault, "accounts.archive", json!({ "id": id }));
    id
}

fn listed(vault: &Vault) -> Vec<Value> {
    ok(vault, "accounts.list", json!({})).as_array().unwrap().clone()
}

fn find(vault: &Vault, id: &str) -> Value {
    listed(vault).into_iter().find(|a| a["id"] == id).unwrap_or_else(|| panic!("no account {id}"))
}

/// What the account holds today.
fn held(vault: &Vault, id: &str) -> String {
    find(vault, id)["balance"].as_str().unwrap().to_string()
}

fn order(vault: &Vault) -> Vec<String> {
    listed(vault).iter().map(|a| a["name"].as_str().unwrap().to_string()).collect()
}

fn category(vault: &Vault, kind: &str, name: &str) -> String {
    id_of(&ok(vault, "categories.create", json!({ "name": name, "type": kind })))
}

fn spend(vault: &Vault, category: &str, name: &str, amount: &str, date: &str, account: Option<&str>) -> Value {
    let mut body = json!({ "name": name, "amount": amount, "paymentDate": date, "categoryId": category });
    if let Some(account) = account {
        body["accountId"] = json!(account);
    }
    ok(vault, "transactions.create", body)
}

fn transfer(vault: &Vault, from: &str, to: &str, amount: &str, date: &str) -> String {
    id_of(&ok(vault, "transfers.create", json!({ "fromAccountId": from, "toAccountId": to, "amount": amount, "date": date })))
}

fn transfer_ids(page: &Value) -> Vec<String> {
    page["transfers"].as_array().unwrap().iter().map(id_of).collect()
}

/// Two accounts, one that is archived, one with nothing in it, and a transfer between the first two: what the requests below name.
struct World {
    a: String,
    b: String,
    archived: String,
    empty: String,
    transfer: String,
    /// Every account that is not archived, in order.
    active: Vec<String>,
}

fn world(vault: &Vault) -> World {
    let (a, b) = (account(vault, "Alpha"), account(vault, "Beta"));
    let archived = archived(vault, "Gamma");
    let empty = account(vault, "Delta");
    let transfer = transfer(vault, &a, &b, "10.00", "2026-10-01");
    World { active: vec!["1".into(), a.clone(), b.clone(), empty.clone()], a, b, archived, empty, transfer }
}

/// A request that would work, for each of the new routes.
fn requests(w: &World) -> Vec<(&'static str, Value)> {
    vec![
        ("accounts.list", json!({})),
        ("accounts.show", json!({ "id": w.a, "from": "2026-10-01", "to": "2026-10-31" })),
        ("accounts.history", json!({ "id": w.a, "year": "2026" })),
        ("accounts.summary", json!({})),
        ("transfers.list", json!({ "accountId": w.a, "from": "2026-10-01", "to": "2026-10-31", "limit": 10, "offset": 0 })),
        ("accounts.create", json!({ "name": "New", "kind": "CASH", "number": "N1", "openingBalance": "1.00", "openingDate": "2026-10-01" })),
        ("accounts.update", json!({ "id": w.a, "name": "Renamed", "kind": "SAVINGS", "number": "N2", "openingBalance": "2.00", "openingDate": "2026-10-02" })),
        ("accounts.archive", json!({ "id": w.a })),
        ("accounts.unarchive", json!({ "id": w.archived })),
        ("accounts.delete", json!({ "id": w.empty })),
        ("accounts.reorder", json!({ "ids": w.active })),
        ("transfers.create", json!({ "fromAccountId": w.a, "toAccountId": w.b, "amount": "1.00", "date": "2026-10-01", "note": "n" })),
        ("transfers.update", json!({ "id": w.transfer, "fromAccountId": w.a, "toAccountId": w.b, "amount": "3.00", "date": "2026-10-02", "note": "x" })),
        ("transfers.delete", json!({ "id": w.transfer })),
    ]
}

/// The name, kind, whether it is required and whether it takes null, of an argument.
type Arg = (&'static str, Kind, bool, bool);

// ---- the table --------------------------------------------------------------------------------

#[test]
fn the_new_routes_have_the_scopes_and_arguments_of_the_design_note() {
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
        ("accounts.list", &[]),
        ("accounts.show", &[("id", Kind::Id, true, false), ("from", Kind::Date, false, false), ("to", Kind::Date, false, false)]),
        ("accounts.history", &[("id", Kind::Id, true, false), ("year", Kind::Year, true, false)]),
        ("accounts.summary", &[]),
        (
            "transfers.list",
            &[("accountId", Kind::Id, false, false), ("from", Kind::Date, false, false), ("to", Kind::Date, false, false), ("limit", Kind::Integer, false, false), ("offset", Kind::Integer, false, false)],
        ),
        (
            "accounts.create",
            &[
                ("name", Kind::String, true, false),
                ("kind", Kind::String, true, false),
                ("number", Kind::String, false, false),
                ("openingBalance", Kind::String, false, false),
                ("openingDate", Kind::Date, false, false),
            ],
        ),
        (
            "accounts.update",
            &[
                ("id", Kind::Id, true, false),
                ("name", Kind::String, false, false),
                ("kind", Kind::String, false, false),
                ("number", Kind::String, false, true),
                ("openingBalance", Kind::String, false, false),
                ("openingDate", Kind::Date, false, false),
            ],
        ),
        ("accounts.archive", &[("id", Kind::Id, true, false)]),
        ("accounts.unarchive", &[("id", Kind::Id, true, false)]),
        ("accounts.delete", &[("id", Kind::Id, true, false)]),
        ("accounts.reorder", &[("ids", Kind::Ids, true, false)]),
        (
            "transfers.create",
            &[
                ("fromAccountId", Kind::Id, true, false),
                ("toAccountId", Kind::Id, true, false),
                ("amount", Kind::Money, true, false),
                ("date", Kind::Date, true, false),
                ("note", Kind::String, false, false),
            ],
        ),
        (
            "transfers.update",
            &[
                ("id", Kind::Id, true, false),
                ("fromAccountId", Kind::Id, false, false),
                ("toAccountId", Kind::Id, false, false),
                ("amount", Kind::Money, false, false),
                ("date", Kind::Date, false, false),
                ("note", Kind::String, false, true),
            ],
        ),
        ("transfers.delete", &[("id", Kind::Id, true, false)]),
    ];
    for (name, expected) in table {
        let got: Vec<Arg> = route(name).params.iter().map(|p| (p.name, p.kind, p.required, p.nullable)).collect();
        assert_eq!(got, expected, "{name}");
    }
    // The kind is the four words of the core, and the opening balance is text because money has no sign.
    let kinds = [crate::accounts::Kind::Checking, crate::accounts::Kind::Savings, crate::accounts::Kind::Cash, crate::accounts::Kind::CreditCard];
    for name in ["accounts.create", "accounts.update"] {
        let words: Vec<&str> = route(name).params.iter().find(|p| p.name == "kind").unwrap().values.to_vec();
        assert_eq!(words, kinds.map(crate::accounts::Kind::as_sql), "{name}");
    }
    // `accountId` is an optional id on each route that makes or changes a row of an account, and on nothing else of the old routes.
    for name in WITH_ACCOUNT {
        let param = route(name).params.iter().find(|p| p.name == "accountId").unwrap_or_else(|| panic!("{name} has no accountId"));
        assert_eq!((param.kind, param.required, param.nullable), (Kind::Id, false, false), "{name}");
    }
    let with: Vec<&str> = routes::ALL
        .iter()
        .filter(|r| r.params.iter().any(|p| p.name == "accountId") && !r.name.starts_with("accounts.") && !r.name.starts_with("transfers."))
        .map(|r| r.name)
        .collect();
    assert_eq!(with, ["transactions.list", "transactions.create", "transactions.update", "recurring.create", "recurring.update", "people.settle", "ledger.add", "ledger.update"]);
}

// ---- the answers ------------------------------------------------------------------------------

#[test]
fn every_account_answer_has_the_fields_of_the_design_note_with_ids_and_money_as_text() {
    let (_dir, vault) = seeded();
    let list = listed(&vault);
    let names: Vec<&str> = list.iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Current account", "Savings", "Cash", "Credit card", "Old savings"], "the person's order, the archived one last");
    for a in &list {
        assert_eq!(keys(a), ACCOUNT_KEYS, "{a}");
        assert!(a["id"].as_str().unwrap().bytes().all(|b| b.is_ascii_digit()), "{a}");
        for money in ["openingBalance", "balance", "changeThisMonth"] {
            assert!(is_money(a[money].as_str().unwrap_or("not text")), "{money} of {a}");
        }
        assert!(a["archived"].is_boolean() && a["isMain"].is_boolean() && a["entries"].is_u64(), "{a}");
        assert!(a["number"].is_null() || a["number"].is_string(), "{a}");
        assert!(a["lastActivity"].is_null() || a["lastActivity"].as_str().is_some_and(|d| d.len() == 10), "{a}");
        assert!(["CHECKING", "SAVINGS", "CASH", "CREDIT_CARD"].contains(&a["kind"].as_str().unwrap()), "{a}");
    }
    assert_eq!((list[0]["id"].as_str(), list[0]["isMain"].as_bool()), (Some("1"), Some(true)));
    assert_eq!(list.iter().filter(|a| a["isMain"] == true).count(), 1);
    assert_eq!(list.iter().map(|a| a["archived"].as_bool().unwrap()).collect::<Vec<_>>(), [false, false, false, false, true]);
    assert_eq!(list[3]["kind"], "CREDIT_CARD");
    assert!(cents(list[3]["balance"].as_str().unwrap()) < 0 && cents(list[3]["openingBalance"].as_str().unwrap()) < 0, "a card is in the red");

    // a statement: the account, the days, what it held before and after, and a line for each movement
    let statement = ok(&vault, "accounts.show", json!({ "id": "1" }));
    assert_eq!(keys(&statement), ["account", "balanceAfter", "balanceBefore", "from", "lines", "to"]);
    assert_eq!(keys(&statement["account"]), ACCOUNT_KEYS);
    assert_eq!((statement["from"].as_str(), statement["to"].as_str()), (Some("2026-10-01"), Some("2026-10-31")), "this month, by default");
    assert!(is_money(statement["balanceBefore"].as_str().unwrap()) && is_money(statement["balanceAfter"].as_str().unwrap()));
    let lines = statement["lines"].as_array().unwrap();
    assert!(lines.len() > 3, "the demo data has movements this month");
    for line in lines {
        assert_eq!(keys(line), ["amount", "balance", "date", "id", "other", "source", "what"], "{line}");
        assert!(line["id"].is_string() && line["what"].is_string() && is_money(line["amount"].as_str().unwrap()) && is_money(line["balance"].as_str().unwrap()), "{line}");
        assert!(["OPENING", "TRANSACTION", "TRANSFER_IN", "TRANSFER_OUT", "LEDGER"].contains(&line["source"].as_str().unwrap()), "{line}");
    }

    // the chart: a month and a balance, one for each month of the year up to today
    let history = ok(&vault, "accounts.history", json!({ "id": "1", "year": "2026" }));
    let points = history.as_array().unwrap();
    assert_eq!(points.len(), 10, "January to October, which is this month");
    assert!(points.iter().all(|p| keys(p) == ["balance", "month"] && is_money(p["balance"].as_str().unwrap())));
    assert_eq!((points[0]["month"].as_str(), points[9]["month"].as_str()), (Some("2026-01"), Some("2026-10")));
    assert_eq!(points[9]["balance"], list[0]["balance"], "this month's point is what the account holds today");

    // net worth
    let summary = ok(&vault, "accounts.summary", json!({}));
    assert_eq!(keys(&summary), ["accounts", "friends", "inAccounts", "inUse", "netWorth"]);
    assert_eq!((summary["accounts"].as_u64(), summary["inUse"].as_bool()), (Some(5), Some(true)));
    for money in ["friends", "inAccounts", "netWorth"] {
        assert!(is_money(summary[money].as_str().unwrap()), "{money}");
    }
    let held_in_all: i64 = list.iter().map(|a| cents(a["balance"].as_str().unwrap())).sum();
    assert_eq!(cents(summary["inAccounts"].as_str().unwrap()), held_in_all, "the archived account counts too");
    assert_eq!(cents(summary["netWorth"].as_str().unwrap()), held_in_all + cents(summary["friends"].as_str().unwrap()));
}

#[test]
fn a_transfer_answers_with_its_fields_and_a_page_of_them_with_its_counts() {
    let (_dir, vault) = seeded();
    let page = ok(&vault, "transfers.list", json!({}));
    assert_eq!(keys(&page), ["limit", "offset", "total", "transfers"]);
    assert_eq!((page["limit"].as_u64(), page["offset"].as_u64()), (Some(50), Some(0)), "50 a page unless asked");
    assert!(page["total"].as_u64().unwrap() >= 16 && page["total"].is_u64());
    let rows = page["transfers"].as_array().unwrap();
    assert_eq!(rows.len() as u64, page["total"].as_u64().unwrap().min(50), "a page of at most 50");
    for t in rows {
        assert_eq!(keys(t), TRANSFER_KEYS, "{t}");
        assert!(t["id"].is_string() && t["fromAccountId"].is_string() && t["toAccountId"].is_string(), "{t}");
        assert!(is_money(t["amount"].as_str().unwrap()) && cents(t["amount"].as_str().unwrap()) > 0, "{t}");
        assert!(t["note"].is_null() || t["note"].is_string(), "{t}");
        assert_ne!(t["fromAccountId"], t["toAccountId"], "{t}");
    }
    let dates: Vec<&str> = rows.iter().map(|t| t["date"].as_str().unwrap()).collect();
    assert!(dates.windows(2).all(|w| w[0] >= w[1]), "newest first: {dates:?}");

    let made = ok(&vault, "transfers.create", json!({ "fromAccountId": "1", "toAccountId": "2", "amount": "200.00", "date": "2026-10-03" }));
    assert_eq!(keys(&made), TRANSFER_KEYS);
    assert_eq!((made["amount"].as_str(), made["date"].as_str(), made["fromAccountId"].as_str(), made["toAccountId"].as_str(), made["note"].is_null()), (Some("200.00"), Some("2026-10-03"), Some("1"), Some("2"), true));
    let changed = ok(&vault, "transfers.update", json!({ "id": id_of(&made), "amount": "250.00", "note": "Rainy day" }));
    assert_eq!((changed["amount"].as_str(), changed["note"].as_str(), changed["date"].as_str()), (Some("250.00"), Some("Rainy day"), Some("2026-10-03")), "fields left out stay");
    assert_eq!(ok(&vault, "transfers.update", json!({ "id": id_of(&made), "note": null }))["note"], Value::Null, "null clears the note");
    assert_eq!(ok(&vault, "transfers.delete", json!({ "id": id_of(&made) })), Value::Null);
}

// ---- making and changing accounts -------------------------------------------------------------

#[test]
fn an_account_is_made_changed_and_its_number_cleared() {
    let (_dir, vault) = fresh();
    let made = ok(&vault, "accounts.create", json!({ "name": "Rainy day", "kind": "SAVINGS", "number": "NL91 ABNA 0417 1643 00", "openingBalance": "-120.00", "openingDate": "2026-01-01" }));
    assert_eq!(keys(&made), ACCOUNT_KEYS);
    assert_eq!((made["name"].as_str(), made["kind"].as_str(), made["number"].as_str(), made["openingBalance"].as_str(), made["openingDate"].as_str()), (Some("Rainy day"), Some("SAVINGS"), Some("NL91 ABNA 0417 1643 00"), Some("-120.00"), Some("2026-01-01")));
    assert_eq!((made["archived"].as_bool(), made["isMain"].as_bool(), made["entries"].as_u64(), made["balance"].as_str()), (Some(false), Some(false), Some(0), Some("-120.00")));
    let id = id_of(&made);
    assert_eq!(order(&vault), ["Main account", "Rainy day"], "at the end of the list");

    // only what is named changes
    let renamed = ok(&vault, "accounts.update", json!({ "id": id, "name": "  Emergency fund " }));
    assert_eq!((renamed["name"].as_str(), renamed["number"].as_str(), renamed["kind"].as_str(), renamed["openingBalance"].as_str()), (Some("Emergency fund"), Some("NL91 ABNA 0417 1643 00"), Some("SAVINGS"), Some("-120.00")), "the name is trimmed, the rest stays");
    let moved = ok(&vault, "accounts.update", json!({ "id": id, "kind": "CASH", "openingBalance": "35.50", "openingDate": "2026-02-01" }));
    assert_eq!((moved["kind"].as_str(), moved["openingBalance"].as_str(), moved["openingDate"].as_str(), moved["balance"].as_str()), (Some("CASH"), Some("35.50"), Some("2026-02-01"), Some("35.50")));
    // null clears the number; an empty one is none too; leaving it out leaves it
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, "number": "X1" }))["number"], "X1");
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, "name": "Fund" }))["number"], "X1");
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, "number": null }))["number"], Value::Null);
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, "number": "X2" }))["number"], "X2");
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, "number": "" }))["number"], Value::Null);
    // nothing named: the account as it is, and nothing written
    let before = dump(&vault);
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": id }))["name"], "Fund");
    assert_eq!(dump(&vault), before);
    // the main account can be renamed and given a number like any other
    let main = ok(&vault, "accounts.update", json!({ "id": "1", "name": "Current account", "number": "IBAN" }));
    assert_eq!((main["name"].as_str(), main["isMain"].as_bool()), (Some("Current account"), Some(true)));

    // what cannot be a name or a number
    let accounts = listed(&vault);
    let long = "x".repeat(51);
    for body in [
        json!({ "name": "", "kind": "CASH" }),
        json!({ "name": "   ", "kind": "CASH" }),
        json!({ "name": long, "kind": "CASH" }),
        json!({ "name": "a\u{0}b", "kind": "CASH" }),
        json!({ "name": "a\nb", "kind": "CASH" }),
        json!({ "name": "\u{1b}[31mred", "kind": "CASH" }),
        json!({ "name": "Fine", "kind": "CASH", "number": long }),
        json!({ "name": "Fine", "kind": "CASH", "number": "a\nb" }),
        json!({ "name": "Fine", "kind": "CASH", "openingDate": "2026-02-30" }),
        json!({ "name": "Fine", "kind": "CASH", "openingDate": "2026-13-01" }),
    ] {
        refuse(&vault, "accounts.create", body, INVALID);
    }
    for body in [
        json!({ "id": id, "name": "" }),
        json!({ "id": id, "name": long }),
        json!({ "id": id, "name": "a\tb" }),
        json!({ "id": id, "number": long }),
        json!({ "id": id, "openingDate": "2026-02-30" }),
    ] {
        refuse(&vault, "accounts.update", body, INVALID);
    }
    assert_eq!(listed(&vault), accounts);
    // a name of exactly 50 characters, and a number of 50, are fine
    assert_eq!(ok(&vault, "accounts.create", json!({ "name": "y".repeat(50), "kind": "CASH", "number": "9".repeat(50) }))["name"].as_str().unwrap().len(), 50);
}

#[test]
fn the_kind_of_an_account_is_one_of_four_words_exactly() {
    let (_dir, vault) = fresh();
    // The route lists the words, as `categories.create` lists INCOME and EXPENSE; the command line matches them without regard to case.
    for kind in ["CHECKING", "SAVINGS", "CASH", "CREDIT_CARD"] {
        assert_eq!(ok(&vault, "accounts.create", json!({ "name": kind, "kind": kind }))["kind"], kind);
    }
    let accounts = listed(&vault);
    let id = id_of(&accounts[1]);
    for kind in ["savings", "Credit_Card", "credit card", "BANK", "", "CREDIT-CARD", "CHECKING "] {
        assert_eq!(refuse(&vault, "accounts.create", json!({ "name": "New", "kind": kind }), INVALID), "`kind` must be one of: CHECKING, SAVINGS, CASH, CREDIT_CARD", "{kind:?}");
        refuse(&vault, "accounts.update", json!({ "id": id, "kind": kind }), INVALID);
    }
    for value in [json!(1), json!(true), json!(null), json!(["CASH"])] {
        refuse(&vault, "accounts.create", json!({ "name": "New", "kind": value }), INVALID);
        refuse(&vault, "accounts.update", json!({ "id": id, "kind": value }), INVALID);
    }
    assert_eq!(listed(&vault), accounts);
}

#[test]
fn an_opening_balance_may_be_negative_and_is_checked_to_the_cent() {
    let (_dir, vault) = fresh();
    let good = ["0.00", "0.01", "-0.01", "-120.00", "9999999999.99", "-9999999999.99", "1234567890.12"];
    for (n, balance) in good.iter().enumerate() {
        let made = ok(&vault, "accounts.create", json!({ "name": format!("Good {n}"), "kind": "CHECKING", "openingBalance": balance }));
        assert_eq!(made["openingBalance"], *balance, "{balance} comes back as it was written");
        assert_eq!(made["balance"], *balance, "an opening date of today counts today");
        // and the same on an update
        let other = account(&vault, &format!("Other {n}"));
        assert_eq!(ok(&vault, "accounts.update", json!({ "id": other, "openingBalance": balance }))["openingBalance"], *balance);
    }
    let bad = [
        "-0.00", "+1.00", "1.5", "1", "-1", "1.", ".5", "-.50", "1.005", "-1.005", "10000000000.00", "-10000000000.00", "9999999999.995", "99999999999.99", "--1.00", "- 1.00", " 1.00", "1.00 ", "1,00", "1e3", "-1e3", "NaN", "abc", "", "-", "０.００", "1.00\n", "0x10.00",
    ];
    let id = account(&vault, "Target");
    for balance in bad {
        refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CHECKING", "openingBalance": balance }), INVALID);
        refuse(&vault, "accounts.update", json!({ "id": id, "openingBalance": balance }), INVALID);
    }
    // one cent beyond, each way, is the first refusal
    refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CHECKING", "openingBalance": "10000000000.00" }), INVALID);
    refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CHECKING", "openingBalance": "-10000000000.00" }), INVALID);
    // a number is not text
    for value in [json!(-120.0), json!(5), json!(null), json!(true)] {
        assert_eq!(refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CHECKING", "openingBalance": value }), INVALID), "`openingBalance` must be a string");
    }
}

// ---- names ------------------------------------------------------------------------------------

#[test]
fn a_name_is_taken_whatever_the_case_until_the_account_with_it_is_archived() {
    let (_dir, vault) = fresh();
    let savings = ok(&vault, "accounts.create", json!({ "name": "Savings", "kind": "SAVINGS" }));
    let other = account(&vault, "Cash");
    for name in ["Savings", "savings", "SAVINGS", "sAvInGs", " Savings ", "Main account", "MAIN ACCOUNT"] {
        refuse(&vault, "accounts.create", json!({ "name": name, "kind": "CASH" }), NAME_EXISTS);
        refuse(&vault, "accounts.update", json!({ "id": other, "name": name }), NAME_EXISTS);
    }
    assert_eq!(refuse(&vault, "accounts.create", json!({ "name": "SAVINGS", "kind": "CASH" }), NAME_EXISTS), "that name already exists");
    // the account itself may change the case of its own name, or keep it
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": id_of(&savings), "name": "SAVINGS" }))["name"], "SAVINGS");
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": id_of(&savings), "name": "SAVINGS", "kind": "CASH" }))["kind"], "CASH");
    // an archived account frees its name; it comes back only when the name is free again
    ok(&vault, "accounts.archive", json!({ "id": id_of(&savings) }));
    let again = ok(&vault, "accounts.create", json!({ "name": "savings", "kind": "SAVINGS" }));
    refuse(&vault, "accounts.unarchive", json!({ "id": id_of(&savings) }), NAME_EXISTS);
    assert_eq!(find(&vault, &id_of(&savings))["archived"], true);
    ok(&vault, "accounts.update", json!({ "id": id_of(&again), "name": "Savings 2" }));
    assert_eq!(ok(&vault, "accounts.unarchive", json!({ "id": id_of(&savings) }))["archived"], false);
    // and now it is the one that has the name
    refuse(&vault, "accounts.create", json!({ "name": "Savings", "kind": "CASH" }), NAME_EXISTS);
}

#[test]
fn the_main_account_is_never_archived_or_deleted() {
    let (_dir, vault) = fresh();
    refuse(&vault, "accounts.archive", json!({ "id": "1" }), INVALID);
    refuse(&vault, "accounts.delete", json!({ "id": "1" }), INVALID);
    assert_eq!((find(&vault, "1")["archived"].as_bool(), find(&vault, "1")["isMain"].as_bool()), (Some(false), Some(true)));
    // the last active account besides the main one can be archived, and then the main one still cannot: one active account stays
    let extra = account(&vault, "Extra");
    ok(&vault, "accounts.archive", json!({ "id": extra }));
    refuse(&vault, "accounts.archive", json!({ "id": "1" }), INVALID);
    let active: Vec<Value> = listed(&vault).into_iter().filter(|a| a["archived"] == false).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0]["id"], "1");
    // with rows in it, the same
    spend(&vault, &category(&vault, "EXPENSE", "Food"), "Bread", "2.00", "2026-10-01", None);
    refuse(&vault, "accounts.delete", json!({ "id": "1" }), INVALID);
    // unarchiving it is the answer as it is
    assert_eq!(ok(&vault, "accounts.unarchive", json!({ "id": "1" }))["isMain"], true);
}

#[test]
fn an_account_with_a_row_is_not_deleted_until_the_last_row_is_gone() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));

    // a transaction: moved away, the account can go
    let a = account(&vault, "Has a transaction");
    let bread = id_of(&spend(&vault, &food, "Bread", "2.00", "2026-10-01", Some(&a)));
    assert_eq!(find(&vault, &a)["entries"], 1);
    refuse(&vault, "accounts.delete", json!({ "id": a }), INVALID);
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": bread, "accountId": "1" }))["accountId"], "1");
    assert_eq!(find(&vault, &a)["entries"], 0);
    assert_eq!(ok(&vault, "accounts.delete", json!({ "id": a })), Value::Null);
    refuse(&vault, "accounts.show", json!({ "id": a }), NOT_FOUND);
    refuse(&vault, "accounts.delete", json!({ "id": a }), NOT_FOUND);

    // only a recurring item
    let b = account(&vault, "Has a recurring item");
    let plan = id_of(&ok(&vault, "recurring.create", json!({ "name": "Gym", "amount": "20.00", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1, "accountId": b })));
    assert_eq!(find(&vault, &b)["entries"], 1);
    refuse(&vault, "accounts.delete", json!({ "id": b }), INVALID);
    ok(&vault, "recurring.delete", json!({ "id": plan }));
    assert_eq!(ok(&vault, "accounts.delete", json!({ "id": b })), Value::Null);

    // only a ledger entry
    let c = account(&vault, "Has a ledger entry");
    let loan = id_of(&ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "9.00", "date": "2026-10-01", "accountId": c })));
    assert_eq!(find(&vault, &c)["entries"], 1);
    refuse(&vault, "accounts.delete", json!({ "id": c }), INVALID);
    ok(&vault, "ledger.delete", json!({ "id": loan }));
    assert_eq!(ok(&vault, "accounts.delete", json!({ "id": c })), Value::Null);

    // only a transfer, on either side
    for (name, to_side) in [("To side", true), ("From side", false)] {
        let d = account(&vault, name);
        let t = if to_side { transfer(&vault, "1", &d, "5.00", "2026-10-01") } else { transfer(&vault, &d, "1", "5.00", "2026-10-01") };
        assert_eq!(find(&vault, &d)["entries"], 1);
        refuse(&vault, "accounts.delete", json!({ "id": d }), INVALID);
        ok(&vault, "accounts.archive", json!({ "id": d }));
        refuse(&vault, "accounts.delete", json!({ "id": d }), INVALID);
        ok(&vault, "transfers.delete", json!({ "id": t }));
        assert_eq!(ok(&vault, "accounts.delete", json!({ "id": d })), Value::Null, "{name}: an archived account with nothing in it can go too");
    }
    assert_eq!(order(&vault), ["Main account"]);
}

#[test]
fn an_account_that_is_not_there_is_a_404_on_every_route_that_names_one() {
    let (_dir, vault) = fresh();
    let gone = account(&vault, "Soon gone");
    ok(&vault, "accounts.delete", json!({ "id": gone }));
    for missing in ["999999", "999999999999999999", gone.as_str()] {
        for (name, body) in [
            ("accounts.show", json!({ "id": missing })),
            ("accounts.history", json!({ "id": missing, "year": "2026" })),
            ("accounts.update", json!({ "id": missing, "name": "X" })),
            ("accounts.update", json!({ "id": missing })),
            ("accounts.archive", json!({ "id": missing })),
            ("accounts.unarchive", json!({ "id": missing })),
            ("accounts.delete", json!({ "id": missing })),
            ("transfers.update", json!({ "id": missing, "amount": "1.00" })),
            ("transfers.delete", json!({ "id": missing })),
        ] {
            assert_eq!(refuse(&vault, name, body.clone(), NOT_FOUND), "not found", "{name} {body}");
        }
    }
    // naming an account that is not there when making a transfer is a refused value, as an unknown category is
    for missing in ["999999", gone.as_str()] {
        refuse(&vault, "transfers.create", json!({ "fromAccountId": "1", "toAccountId": missing, "amount": "1.00", "date": "2026-10-01" }), INVALID);
        refuse(&vault, "transfers.create", json!({ "fromAccountId": missing, "toAccountId": "1", "amount": "1.00", "date": "2026-10-01" }), INVALID);
    }
    // a read that filters by an account that is not there finds nothing
    let page = ok(&vault, "transfers.list", json!({ "accountId": "999999" }));
    assert_eq!((page["total"].as_u64(), page["transfers"].as_array().unwrap().len()), (Some(0), 0));
}

// ---- archiving --------------------------------------------------------------------------------

#[test]
fn an_archived_account_cannot_be_changed_or_ordered_and_comes_back_only_if_its_name_is_free() {
    let (_dir, vault) = fresh();
    let (kept, old) = (account(&vault, "Kept"), account(&vault, "Old"));
    let archived = ok(&vault, "accounts.archive", json!({ "id": old }));
    assert_eq!(archived["archived"], true);
    assert_eq!(order(&vault), ["Main account", "Kept", "Old"], "the archived ones last");
    // it can be read, but not changed or ordered: bring it back first
    assert_eq!(ok(&vault, "accounts.show", json!({ "id": old }))["account"]["archived"], true);
    assert_eq!(ok(&vault, "accounts.history", json!({ "id": old, "year": "2026" })).as_array().unwrap().len(), 10);
    for body in [json!({ "id": old, "name": "New name" }), json!({ "id": old }), json!({ "id": old, "number": null }), json!({ "id": old, "openingBalance": "1.00" })] {
        refuse(&vault, "accounts.update", body, INVALID);
    }
    refuse(&vault, "accounts.reorder", json!({ "ids": ["1", kept, old] }), INVALID);
    refuse(&vault, "accounts.reorder", json!({ "ids": [old, "1", kept] }), INVALID);
    refuse(&vault, "accounts.reorder", json!({ "ids": [old] }), INVALID);
    ok(&vault, "accounts.reorder", json!({ "ids": [kept, "1"] }));
    assert_eq!(order(&vault), ["Kept", "Main account", "Old"]);
    // an archived account with nothing in it can be deleted
    assert_eq!(ok(&vault, "accounts.delete", json!({ "id": old })), Value::Null);
}

#[test]
fn archiving_twice_or_bringing_back_what_was_never_archived_answers_and_changes_nothing() {
    let (_dir, vault) = fresh();
    let id = account(&vault, "Twice");
    let never = ok(&vault, "accounts.unarchive", json!({ "id": id }));
    assert_eq!(never["archived"], false);
    let before = dump(&vault);
    assert_eq!(ok(&vault, "accounts.unarchive", json!({ "id": id })), never, "an account that is not archived is answered as it is");
    assert_eq!(dump(&vault), before, "and nothing is written");

    let first = ok(&vault, "accounts.archive", json!({ "id": id }));
    assert_eq!(first["archived"], true);
    let before = dump(&vault);
    assert_eq!(ok(&vault, "accounts.archive", json!({ "id": id })), first, "the second time answers the account as it is");
    assert_eq!(dump(&vault), before, "and makes no change and no stamp");
    // back and forth
    assert_eq!(ok(&vault, "accounts.unarchive", json!({ "id": id }))["archived"], false);
    assert_eq!(ok(&vault, "accounts.archive", json!({ "id": id }))["archived"], true);
    assert_eq!(ok(&vault, "accounts.unarchive", json!({ "id": id }))["archived"], false);
    assert_eq!(order(&vault), ["Main account", "Twice"]);
}

#[test]
fn the_order_of_the_accounts_is_set_by_giving_every_active_one_once() {
    let (_dir, vault) = fresh();
    let (a, b) = (account(&vault, "A"), account(&vault, "B"));
    let c = archived(&vault, "C");
    assert_eq!(order(&vault), ["Main account", "A", "B", "C"]);
    assert_eq!(ok(&vault, "accounts.reorder", json!({ "ids": [b, "1", a] })), Value::Null);
    assert_eq!(order(&vault), ["B", "Main account", "A", "C"], "the archived one stays last");
    let before = order(&vault);
    for ids in [
        json!([b, "1"]),
        json!([b, b, "1", a]),
        json!([b, "1", a, c]),
        json!([b, "1", a, "999999"]),
        json!([b, "1", "999999"]),
        json!([]),
        json!(["1"]),
        json!([c]),
        json!([a, a]),
    ] {
        refuse(&vault, "accounts.reorder", json!({ "ids": ids }), INVALID);
        assert_eq!(order(&vault), before, "{ids}");
    }
    // a list that is not ids at all is refused before it gets to the accounts
    for ids in [json!(["abc"]), json!(["0"]), json!([1, 2]), json!("1,2"), json!(null), json!({})] {
        assert_eq!(refuse(&vault, "accounts.reorder", json!({ "ids": ids }), INVALID), "`ids` must be a list of ids as the app lists them");
    }
    // the same order again changes nothing: no row is written for a position that did not move
    let same = dump(&vault);
    ok(&vault, "accounts.reorder", json!({ "ids": [b, "1", a] }));
    assert_eq!(dump(&vault), same);
}

// ---- transfers --------------------------------------------------------------------------------

#[test]
fn a_transfer_moves_money_between_two_balances_and_is_never_income_or_expense() {
    let (_dir, vault) = fresh();
    let a = id_of(&ok(&vault, "accounts.create", json!({ "name": "Current", "kind": "CHECKING", "openingBalance": "1000.00", "openingDate": "2026-09-01" })));
    let b = account(&vault, "Savings");
    let (preview, year, listed_before) = (ok(&vault, "export.preview", json!({})), ok(&vault, "stats.year", json!({ "year": "2026" })), ok(&vault, "transactions.list", json!({})));

    let t = transfer(&vault, &a, &b, "200.00", "2026-10-03");
    assert_eq!((held(&vault, &a), held(&vault, &b)), ("800.00".into(), "200.00".to_string()));
    assert_eq!((find(&vault, &a)["changeThisMonth"].as_str(), find(&vault, &b)["changeThisMonth"].as_str()), (Some("-200.00"), Some("200.00")));
    assert_eq!((find(&vault, &a)["lastActivity"].as_str(), find(&vault, &b)["lastActivity"].as_str()), (Some("2026-10-03"), Some("2026-10-03")));
    assert_eq!((find(&vault, &a)["entries"].as_u64(), find(&vault, &b)["entries"].as_u64()), (Some(1), Some(1)));
    // it is in no statistic, budget or export total
    let after = ok(&vault, "export.preview", json!({}));
    assert_eq!((&after["transactions"], &after["income"], &after["expense"]), (&preview["transactions"], &preview["income"], &preview["expense"]));
    assert_eq!(after["transfers"].as_u64().unwrap(), preview["transfers"].as_u64().unwrap() + 1, "the export knows it");
    assert_eq!(ok(&vault, "stats.year", json!({ "year": "2026" })), year);
    assert_eq!(ok(&vault, "transactions.list", json!({})), listed_before);
    // the statement of each side shows it, signed by what it did, with the other account
    let out = ok(&vault, "accounts.show", json!({ "id": a }))["lines"].as_array().unwrap().last().unwrap().clone();
    assert_eq!((out["source"].as_str(), out["amount"].as_str(), out["other"].as_str(), out["id"].as_str()), (Some("TRANSFER_OUT"), Some("-200.00"), Some("Savings"), Some(t.as_str())));
    let into = ok(&vault, "accounts.show", json!({ "id": b }))["lines"][0].clone();
    assert_eq!((into["source"].as_str(), into["amount"].as_str(), into["other"].as_str()), (Some("TRANSFER_IN"), Some("200.00"), Some("Current")));

    // later than today: not in today's balance
    let later = transfer(&vault, &a, &b, "50.00", "2026-10-20");
    assert_eq!((held(&vault, &a), held(&vault, &b)), ("800.00".into(), "200.00".to_string()));
    assert_eq!(ok(&vault, "accounts.show", json!({ "id": b }))["balanceAfter"], "250.00", "but in the balance of its day");
    // changing it changes the balances
    ok(&vault, "transfers.update", json!({ "id": t, "amount": "300.00" }));
    assert_eq!((held(&vault, &a), held(&vault, &b)), ("700.00".into(), "300.00".to_string()));
    ok(&vault, "transfers.update", json!({ "id": t, "fromAccountId": b, "toAccountId": a }));
    assert_eq!((held(&vault, &a), held(&vault, &b)), ("1300.00".into(), "-300.00".to_string()), "turned around");
    // the sum over every account of every transfer is nothing
    let all: i64 = listed(&vault).iter().map(|x| cents(x["balance"].as_str().unwrap())).sum();
    assert_eq!(all, 100_000, "only the opening balance is in the accounts");
    // deleting brings the balances back
    ok(&vault, "transfers.delete", json!({ "id": t }));
    ok(&vault, "transfers.delete", json!({ "id": later }));
    assert_eq!((held(&vault, &a), held(&vault, &b)), ("1000.00".into(), "0.00".to_string()));
    refuse(&vault, "transfers.delete", json!({ "id": t }), NOT_FOUND);
    refuse(&vault, "transfers.update", json!({ "id": t, "amount": "1.00" }), NOT_FOUND);
}

#[test]
fn a_transfer_that_makes_no_sense_is_refused_and_changes_nothing() {
    let (_dir, vault) = fresh();
    let (a, b) = (account(&vault, "A"), account(&vault, "B"));
    let old = archived(&vault, "Old");
    let t = transfer(&vault, &a, &b, "10.00", "2026-10-01");
    let body = |from: &str, to: &str, amount: &str, date: &str| json!({ "fromAccountId": from, "toAccountId": to, "amount": amount, "date": date });
    // to itself, with an amount that is nothing, negative, or not to the cent
    refuse(&vault, "transfers.create", body(&a, &a, "5.00", "2026-10-01"), INVALID);
    refuse(&vault, "transfers.create", body("1", "1", "5.00", "2026-10-01"), INVALID);
    for amount in ["0.00", "-1.00", "1.005", "1.5", "5", "+1.00", "1e2", "10000000000.00", "0.001", ""] {
        refuse(&vault, "transfers.create", body(&a, &b, amount, "2026-10-01"), INVALID);
        refuse(&vault, "transfers.update", json!({ "id": t, "amount": amount }), INVALID);
    }
    // unknown accounts, archived ones, bad days and notes
    refuse(&vault, "transfers.create", body(&a, "999999", "5.00", "2026-10-01"), INVALID);
    refuse(&vault, "transfers.create", body("999999", &b, "5.00", "2026-10-01"), INVALID);
    refuse(&vault, "transfers.create", body(&a, &old, "5.00", "2026-10-01"), INVALID);
    refuse(&vault, "transfers.create", body(&old, &b, "5.00", "2026-10-01"), INVALID);
    for date in ["2026-02-30", "2026-13-01", "2026-00-10", "0000-00-00"] {
        refuse(&vault, "transfers.create", body(&a, &b, "5.00", date), INVALID);
        refuse(&vault, "transfers.update", json!({ "id": t, "date": date }), INVALID);
    }
    for note in ["x".repeat(201), "a\u{0}b".to_string(), "bell\u{7}".to_string()] {
        refuse(&vault, "transfers.create", json!({ "fromAccountId": a, "toAccountId": b, "amount": "5.00", "date": "2026-10-01", "note": note }), INVALID);
        refuse(&vault, "transfers.update", json!({ "id": t, "note": note }), INVALID);
    }
    // an update that makes both sides the same, by either side or by both
    refuse(&vault, "transfers.update", json!({ "id": t, "fromAccountId": b }), INVALID);
    refuse(&vault, "transfers.update", json!({ "id": t, "toAccountId": a }), INVALID);
    refuse(&vault, "transfers.update", json!({ "id": t, "fromAccountId": "1", "toAccountId": "1" }), INVALID);
    refuse(&vault, "transfers.update", json!({ "id": t, "toAccountId": old }), INVALID);
    refuse(&vault, "transfers.update", json!({ "id": t, "fromAccountId": "999999" }), INVALID);
    assert_eq!(ok(&vault, "transfers.list", json!({}))["total"], 1, "nothing was made");

    // what is allowed: the edges of the amount, a note of 200 characters or with a line break, an empty note (none), both sides swapped
    assert_eq!(ok(&vault, "transfers.create", body(&a, &b, "9999999999.99", "2026-10-01"))["amount"], "9999999999.99");
    assert_eq!(ok(&vault, "transfers.create", body(&a, &b, "0.01", "2026-10-01"))["amount"], "0.01");
    let long = ok(&vault, "transfers.create", json!({ "fromAccountId": a, "toAccountId": b, "amount": "1.00", "date": "2026-10-01", "note": "n".repeat(200) }));
    assert_eq!(long["note"].as_str().unwrap().len(), 200);
    let lines = ok(&vault, "transfers.create", json!({ "fromAccountId": a, "toAccountId": b, "amount": "1.00", "date": "2026-10-01", "note": "two\nlines" }));
    assert_eq!(lines["note"], "two\nlines");
    let none = ok(&vault, "transfers.create", json!({ "fromAccountId": a, "toAccountId": b, "amount": "1.00", "date": "2026-10-01", "note": "" }));
    assert_eq!(none["note"], Value::Null, "an empty note is none");
    let swapped = ok(&vault, "transfers.update", json!({ "id": t, "fromAccountId": b, "toAccountId": a }));
    assert_eq!((swapped["fromAccountId"].as_str(), swapped["toAccountId"].as_str()), (Some(b.as_str()), Some(a.as_str())));
}

#[test]
fn a_transfer_can_be_edited_while_one_side_is_archived_but_never_moved_into_an_archived_account() {
    let (_dir, vault) = fresh();
    let (a, b, d) = (account(&vault, "A"), account(&vault, "B"), archived(&vault, "D"));
    let t = transfer(&vault, &a, &b, "10.00", "2026-10-01");
    ok(&vault, "accounts.archive", json!({ "id": b }));
    assert_eq!(held(&vault, &b), "10.00", "an archived account keeps what it holds");
    assert_eq!(ok(&vault, "transfers.list", json!({ "accountId": b }))["total"], 1, "and its transfers");
    // the side that does not change may be archived, named again or not
    assert_eq!(ok(&vault, "transfers.update", json!({ "id": t, "amount": "12.00" }))["amount"], "12.00");
    assert_eq!(ok(&vault, "transfers.update", json!({ "id": t, "toAccountId": b, "note": "again" }))["note"], "again");
    assert_eq!(ok(&vault, "transfers.update", json!({ "id": t, "note": null }))["note"], Value::Null);
    // a side that changes must be active
    refuse(&vault, "transfers.update", json!({ "id": t, "fromAccountId": d }), INVALID);
    refuse(&vault, "transfers.update", json!({ "id": t, "toAccountId": d }), INVALID);
    assert_eq!(ok(&vault, "transfers.update", json!({ "id": t, "fromAccountId": "1" }))["fromAccountId"], "1", "the side that changes can go to the main account while the other stays archived");
    assert_eq!(held(&vault, &a), "0.00");
    // and a transfer with an archived side can be deleted
    assert_eq!(ok(&vault, "transfers.delete", json!({ "id": t })), Value::Null);
}

#[test]
fn transfers_are_listed_newest_first_for_either_side_in_pages_like_transactions() {
    let (_dir, vault) = fresh();
    let (a, b, c) = (account(&vault, "A"), account(&vault, "B"), account(&vault, "C"));
    let t1 = transfer(&vault, &a, &b, "10.00", "2026-10-01");
    let t2 = transfer(&vault, &b, &c, "20.00", "2026-10-02");
    let t3 = transfer(&vault, &c, &a, "30.00", "2026-10-03");
    let t4 = transfer(&vault, &a, &c, "40.00", "2026-10-03");
    let t5 = transfer(&vault, &a, &b, "5.00", "2026-09-15");
    let list = |body: Value| ok(&vault, "transfers.list", body);
    assert_eq!(transfer_ids(&list(json!({}))), [t4.as_str(), t3.as_str(), t2.as_str(), t1.as_str(), t5.as_str()], "newest date first, then the newest made");
    assert_eq!(list(json!({}))["total"], 5);
    // either side
    assert_eq!(transfer_ids(&list(json!({ "accountId": a }))), [t4.as_str(), t3.as_str(), t1.as_str(), t5.as_str()]);
    assert_eq!(transfer_ids(&list(json!({ "accountId": b }))), [t2.as_str(), t1.as_str(), t5.as_str()]);
    assert_eq!(transfer_ids(&list(json!({ "accountId": c }))), [t4.as_str(), t3.as_str(), t2.as_str()]);
    assert_eq!(list(json!({ "accountId": "1" }))["total"], 0, "the main account has none");
    // the days are inclusive
    assert_eq!(transfer_ids(&list(json!({ "from": "2026-10-02", "to": "2026-10-03" }))), [t4.as_str(), t3.as_str(), t2.as_str()]);
    assert_eq!(transfer_ids(&list(json!({ "from": "2026-10-03" }))), [t4.as_str(), t3.as_str()]);
    assert_eq!(transfer_ids(&list(json!({ "to": "2026-10-01" }))), [t1.as_str(), t5.as_str()]);
    assert_eq!(transfer_ids(&list(json!({ "from": "2026-10-03", "to": "2026-10-03" }))), [t4.as_str(), t3.as_str()]);
    assert_eq!(transfer_ids(&list(json!({ "accountId": a, "from": "2026-10-02" }))), [t4.as_str(), t3.as_str()]);
    assert_eq!(list(json!({ "from": "2026-10-31", "to": "2026-10-01" }))["total"], 0, "days the wrong way round find nothing");
    // pages: `total` is all that match, whatever the page
    let page = list(json!({ "limit": 2 }));
    assert_eq!((transfer_ids(&page), page["total"].as_u64(), page["limit"].as_u64(), page["offset"].as_u64()), ([t4.clone(), t3.clone()].to_vec(), Some(5), Some(2), Some(0)));
    assert_eq!(transfer_ids(&list(json!({ "limit": 2, "offset": 2 }))), [t2.as_str(), t1.as_str()]);
    assert_eq!(transfer_ids(&list(json!({ "limit": 2, "offset": 4 }))), [t5.as_str()]);
    let past = list(json!({ "limit": 2, "offset": 5 }));
    assert_eq!((past["transfers"].as_array().unwrap().len(), past["total"].as_u64(), past["offset"].as_u64()), (0, Some(5), Some(5)));
    assert_eq!(list(json!({ "offset": 4_000_000_000u64 }))["transfers"], json!([]));
    assert_eq!(list(json!({ "limit": 100 }))["limit"], 100);
    // the limits of `transactions.list`: 1 to 100
    for body in [json!({ "limit": 0 }), json!({ "limit": 101 }), json!({ "limit": -1 }), json!({ "limit": 4294967296u64 }), json!({ "offset": -1 }), json!({ "offset": 4294967296u64 })] {
        refuse(&vault, "transfers.list", body.clone(), INVALID);
        assert_eq!(refused(&vault, "transactions.list", body.clone()), (400, "invalid_input".into()), "{body}: the same limits as the transactions");
    }
    for body in [json!({ "from": "2026-02-30" }), json!({ "to": "2026-13-01" }), json!({ "accountId": "0" }), json!({ "accountId": "abc" }), json!({ "accountId": "-1" }), json!({ "accountId": 1 })] {
        refuse(&vault, "transfers.list", body, INVALID);
    }
}

// ---- statements, the chart and net worth -------------------------------------------------------

#[test]
fn a_statement_adds_up_and_the_days_are_this_month_unless_given() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let pay = category(&vault, "INCOME", "Pay");
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let b = id_of(&ok(&vault, "accounts.create", json!({ "name": "Savings", "kind": "SAVINGS", "openingBalance": "100.00", "openingDate": "2026-09-01" })));
    spend(&vault, &food, "Groceries", "30.00", "2026-09-20", Some(&b));
    spend(&vault, &pay, "Salary", "500.00", "2026-10-02", Some(&b));
    transfer(&vault, "1", &b, "200.00", "2026-10-03");
    transfer(&vault, &b, "1", "50.00", "2026-10-10");
    ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "20.00", "date": "2026-10-12", "note": "taxi", "accountId": b }));
    spend(&vault, &food, "Later", "10.00", "2026-10-20", Some(&b));
    spend(&vault, &food, "Next month", "7.00", "2026-11-02", Some(&b));

    let before = dump(&vault);
    let month = ok(&vault, "accounts.show", json!({ "id": b }));
    assert_eq!((month["from"].as_str(), month["to"].as_str()), (Some("2026-10-01"), Some("2026-10-31")));
    assert_eq!((month["balanceBefore"].as_str(), month["balanceAfter"].as_str()), (Some("70.00"), Some("690.00")));
    let lines = month["lines"].as_array().unwrap();
    let shown: Vec<(&str, &str, &str, &str, &str)> =
        lines.iter().map(|l| (l["date"].as_str().unwrap(), l["source"].as_str().unwrap(), l["what"].as_str().unwrap(), l["other"].as_str().unwrap_or("-"), l["amount"].as_str().unwrap())).collect();
    assert_eq!(
        shown,
        [
            ("2026-10-02", "TRANSACTION", "Salary", "Pay", "500.00"),
            ("2026-10-03", "TRANSFER_IN", "", "Main account", "200.00"),
            ("2026-10-10", "TRANSFER_OUT", "", "Main account", "-50.00"),
            ("2026-10-12", "LEDGER", "taxi", "Anna", "-20.00"),
            ("2026-10-20", "TRANSACTION", "Later", "Food", "-10.00"),
        ]
    );
    // balanceBefore + the lines = balanceAfter, and each line holds the running balance
    let mut running = cents(month["balanceBefore"].as_str().unwrap());
    for line in lines {
        running += cents(line["amount"].as_str().unwrap());
        assert_eq!(cents(line["balance"].as_str().unwrap()), running, "{line}");
    }
    assert_eq!(running, cents(month["balanceAfter"].as_str().unwrap()));
    assert_eq!(month["account"]["balance"], "700.00", "the account itself holds what it holds today: nothing dated after the 14th");
    assert!(lines.iter().all(|l| l["id"].is_string()));

    // days given: both included
    let some = ok(&vault, "accounts.show", json!({ "id": b, "from": "2026-10-03", "to": "2026-10-10" }));
    assert_eq!((some["balanceBefore"].as_str(), some["balanceAfter"].as_str(), some["lines"].as_array().unwrap().len()), (Some("570.00"), Some("720.00"), 2));
    let one_day = ok(&vault, "accounts.show", json!({ "id": b, "from": "2026-10-10", "to": "2026-10-10" }));
    assert_eq!((one_day["lines"].as_array().unwrap().len(), one_day["lines"][0]["source"].as_str()), (1, Some("TRANSFER_OUT")));
    // each end given alone: the other is this month's
    assert_eq!(ok(&vault, "accounts.show", json!({ "id": b, "from": "2026-10-12" }))["lines"].as_array().unwrap().len(), 2);
    assert_eq!(ok(&vault, "accounts.show", json!({ "id": b, "to": "2026-10-03" }))["lines"].as_array().unwrap().len(), 2);
    // before the account opened there is nothing; across the day it opened, the opening balance is the first line
    let early = ok(&vault, "accounts.show", json!({ "id": b, "from": "2026-08-01", "to": "2026-08-31" }));
    assert_eq!((early["balanceBefore"].as_str(), early["balanceAfter"].as_str(), early["lines"].as_array().unwrap().len()), (Some("0.00"), Some("0.00"), 0));
    let opening = ok(&vault, "accounts.show", json!({ "id": b, "from": "2026-08-15", "to": "2026-09-30" }));
    assert_eq!((opening["balanceBefore"].as_str(), opening["balanceAfter"].as_str()), (Some("0.00"), Some("70.00")));
    assert_eq!((opening["lines"][0]["source"].as_str(), opening["lines"][0]["amount"].as_str(), opening["lines"][0]["id"].as_str(), opening["lines"][0]["what"].as_str()), (Some("OPENING"), Some("100.00"), Some(b.as_str()), Some("")));
    assert_eq!(dump(&vault), before, "reading changes nothing");

    // days the wrong way round: the core refuses them, whatever the ends
    for body in [
        json!({ "id": b, "from": "2026-10-31", "to": "2026-10-01" }),
        json!({ "id": b, "from": "2026-10-11", "to": "2026-10-10" }),
        json!({ "id": b, "from": "2026-11-01" }),
        json!({ "id": b, "to": "2026-09-30" }),
        json!({ "id": b, "from": "2026-02-30" }),
        json!({ "id": b, "to": "2026-13-01" }),
    ] {
        refuse(&vault, "accounts.show", body, INVALID);
    }
}

#[test]
fn this_month_is_the_month_of_today_in_the_apps_time_zone() {
    let (_dir, vault) = fresh();
    let period = |now: &str| {
        let statement = run_at(&vault, "accounts.show", json!({ "id": "1" }), at(now)).unwrap();
        (statement["from"].as_str().unwrap().to_string(), statement["to"].as_str().unwrap().to_string())
    };
    assert_eq!(period("2026-10-14T12:00:00Z"), ("2026-10-01".into(), "2026-10-31".into()));
    assert_eq!(period("2026-12-15T12:00:00Z"), ("2026-12-01".into(), "2026-12-31".into()), "December ends in December");
    assert_eq!(period("2027-02-10T12:00:00Z"), ("2027-02-01".into(), "2027-02-28".into()));
    assert_eq!(period("2028-02-10T12:00:00Z"), ("2028-02-01".into(), "2028-02-29".into()), "a leap year");
    assert_eq!(period("2026-04-30T22:30:00Z"), ("2026-04-01".into(), "2026-04-30".into()), "23:30 in Dublin in summer is still April");
    assert_eq!(period("2026-04-30T23:30:00Z"), ("2026-05-01".into(), "2026-05-31".into()), "and half an hour later it is May");
    let mut settings = vault.settings().unwrap();
    settings.timezone = "Pacific/Auckland".into();
    vault.save_settings(&settings).unwrap();
    assert_eq!(period("2026-10-31T20:00:00Z"), ("2026-11-01".into(), "2026-11-30".into()), "the zone set in the app, not the computer's");
}

#[test]
fn the_history_has_a_point_for_each_month_up_to_today() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let b = id_of(&ok(&vault, "accounts.create", json!({ "name": "Savings", "kind": "SAVINGS", "openingBalance": "100.00", "openingDate": "2026-01-15" })));
    spend(&vault, &food, "Boots", "40.00", "2026-03-10", Some(&b));
    spend(&vault, &food, "Later", "5.00", "2026-10-20", Some(&b));
    let months = |year: &str, now: &str| -> Vec<(String, String)> {
        run_at(&vault, "accounts.history", json!({ "id": b, "year": year }), at(now))
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["month"].as_str().unwrap().to_string(), p["balance"].as_str().unwrap().to_string()))
            .collect()
    };
    let this_year = months("2026", "2026-10-14T12:00:00Z");
    assert_eq!(this_year.len(), 10);
    assert_eq!((this_year[0].clone(), this_year[1].clone(), this_year[2].clone()), (("2026-01".into(), "100.00".into()), ("2026-02".into(), "100.00".into()), ("2026-03".into(), "60.00".into())));
    assert_eq!(this_year[9], ("2026-10".to_string(), "60.00".to_string()), "this month is counted to today: the 20th is not in it yet");
    assert_eq!(months("2026", "2026-12-31T12:00:00Z").last().unwrap(), &("2026-12".to_string(), "55.00".to_string()), "a month that is over is counted to its last day");
    assert_eq!(months("2026", "2026-12-31T12:00:00Z").len(), 12);
    // before it opened, the account does not hold its opening balance yet
    let before = months("2025", "2026-10-14T12:00:00Z");
    assert_eq!(before.len(), 12);
    assert!(before.iter().all(|(_, balance)| balance == "0.00"), "{before:?}");
    assert_eq!(months("2027", "2026-10-14T12:00:00Z"), [], "no point for a month that has not begun");
    assert_eq!(run(&vault, "accounts.history", json!({ "id": b, "year": "9999" })).unwrap(), json!([]));
    // the year is a year, and the account must be there
    for year in ["26", "20266", "abcd", "٢٠٢٦", "2026-", "", " 2026"] {
        refuse(&vault, "accounts.history", json!({ "id": b, "year": year }), INVALID);
    }
    refuse(&vault, "accounts.history", json!({ "id": b, "year": 2026 }), INVALID);
    assert_eq!(refuse(&vault, "accounts.history", json!({ "id": b }), INVALID), "missing argument `year`");
    refuse(&vault, "accounts.history", json!({ "year": "2026" }), INVALID);
    refuse(&vault, "accounts.history", json!({ "id": "999999", "year": "2026" }), NOT_FOUND);
}

#[test]
fn the_summary_adds_what_friends_owe_to_what_the_accounts_hold() {
    let (_dir, vault) = fresh();
    assert_eq!(ok(&vault, "accounts.summary", json!({})), json!({ "accounts": 1, "friends": "0.00", "inAccounts": "0.00", "inUse": false, "netWorth": "0.00" }));
    // one account with an opening balance is already in use
    ok(&vault, "accounts.update", json!({ "id": "1", "openingBalance": "5.00", "openingDate": "2026-01-01" }));
    assert_eq!(ok(&vault, "accounts.summary", json!({})), json!({ "accounts": 1, "friends": "0.00", "inAccounts": "5.00", "inUse": true, "netWorth": "5.00" }));
    ok(&vault, "accounts.update", json!({ "id": "1", "openingBalance": "0.00" }));
    assert_eq!(ok(&vault, "accounts.summary", json!({}))["inUse"], false, "and not any more when it is back to nothing");
    // a card in the red, an archived account, and money lent to a friend: it is still yours
    ok(&vault, "accounts.create", json!({ "name": "Card", "kind": "CREDIT_CARD", "openingBalance": "-50.00", "openingDate": "2026-01-01" }));
    let old = id_of(&ok(&vault, "accounts.create", json!({ "name": "Old", "kind": "SAVINGS", "openingBalance": "30.00", "openingDate": "2026-01-01" })));
    ok(&vault, "accounts.archive", json!({ "id": old }));
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "20.00", "date": "2026-10-01" }));
    assert_eq!(
        ok(&vault, "accounts.summary", json!({})),
        json!({ "accounts": 3, "friends": "20.00", "inAccounts": "-40.00", "inUse": true, "netWorth": "-20.00" }),
        "-20 left the main account, -50 on the card, +30 in the archived one; the friend owes 20"
    );
}

// ---- the account of the rows that were there before ---------------------------------------------

#[test]
fn a_transaction_a_recurring_item_and_a_ledger_entry_are_made_in_the_account_asked_for_and_moved_by_an_update() {
    let (_dir, vault) = fresh();
    let (food, pay) = (category(&vault, "EXPENSE", "Food"), category(&vault, "INCOME", "Pay"));
    let (a, b) = (account(&vault, "Savings"), account(&vault, "Cash"));
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));

    // without an account: the main one
    let plain = spend(&vault, &food, "Bread", "2.00", "2026-10-01", None);
    assert_eq!(plain["accountId"], "1");
    let item = |extra: Value| {
        let mut body = json!({ "name": "Gym", "amount": "20.00", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1 });
        for (key, value) in extra.as_object().unwrap() {
            body[key] = value.clone();
        }
        ok(&vault, "recurring.create", body)
    };
    let entry = |extra: Value| {
        let mut body = json!({ "personId": anna, "kind": "LENT", "amount": "9.00", "date": "2026-10-02" });
        for (key, value) in extra.as_object().unwrap() {
            body[key] = value.clone();
        }
        ok(&vault, "ledger.add", body)
    };
    let (plain_item, plain_entry) = (item(json!({})), entry(json!({})));
    assert_eq!((plain_item["accountId"].as_str(), plain_entry["accountId"].as_str()), (Some("1"), Some("1")));

    // in an account: read back from the answer and from the lists
    let income = spend(&vault, &pay, "Salary", "100.00", "2026-10-01", Some(&a));
    let (tx, rec, led) = (spend(&vault, &food, "Boots", "40.00", "2026-10-03", Some(&a)), item(json!({ "accountId": a })), entry(json!({ "accountId": a })));
    for (what, answer) in [("transaction", &tx), ("recurring item", &rec), ("ledger entry", &led), ("income", &income)] {
        assert_eq!(answer["accountId"], a.as_str(), "{what}");
    }
    let seen = ok(&vault, "transactions.list", json!({ "accountId": a }));
    assert!(seen["transactions"].as_array().unwrap().iter().all(|t| t["accountId"] == a.as_str()));
    assert_eq!(ok(&vault, "recurring.list", json!({})).as_array().unwrap().iter().find(|r| r["id"] == rec["id"]).unwrap()["accountId"], a.as_str());
    assert_eq!((held(&vault, &a), held(&vault, &b), held(&vault, "1")), ("51.00".into(), "0.00".into(), "-11.00".into()), "100 in, 40 and a loan of 9 out; the main account has the rest");

    // moved by an update, one at a time
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": id_of(&tx), "accountId": b }))["accountId"], b.as_str());
    assert_eq!(ok(&vault, "recurring.update", json!({ "id": id_of(&rec), "accountId": b }))["accountId"], b.as_str());
    assert_eq!(ok(&vault, "ledger.update", json!({ "id": id_of(&led), "accountId": b }))["accountId"], b.as_str());
    assert_eq!((held(&vault, &a), held(&vault, &b)), ("100.00".into(), "-49.00".to_string()), "the money followed the rows");
    assert_eq!((find(&vault, &a)["entries"].as_u64(), find(&vault, &b)["entries"].as_u64()), (Some(1), Some(3)));
    // an update that names the account the row has, or no account, is not a move
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": id_of(&tx), "accountId": b, "name": "Boots 2" }))["accountId"], b.as_str());
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": id_of(&tx), "name": "Boots 3" }))["accountId"], b.as_str(), "left out, it stays where it is");
    assert_eq!(ok(&vault, "recurring.update", json!({ "id": id_of(&rec), "name": "Gym 2" }))["accountId"], b.as_str());
    assert_eq!(ok(&vault, "ledger.update", json!({ "id": id_of(&led), "amount": "10.00" }))["accountId"], b.as_str());
    // and back to the main account, by its id
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": id_of(&tx), "accountId": "1" }))["accountId"], "1");
}

#[test]
fn transactions_are_listed_for_one_account() {
    let (_dir, vault) = seeded();
    let cash = ok(&vault, "transactions.list", json!({ "accountId": "3", "limit": 100 }));
    let rows = cash["transactions"].as_array().unwrap();
    assert!(cash["total"].as_u64().unwrap() > 20 && !rows.is_empty());
    assert!(rows.iter().all(|t| t["accountId"] == "3"), "only the rows in that account");
    let all = ok(&vault, "transactions.list", json!({}));
    assert!(cash["total"].as_u64() < all["total"].as_u64());
    // the other filters still apply with it
    let some = ok(&vault, "transactions.list", json!({ "accountId": "3", "q": "taxi", "limit": 100 }));
    assert!(some["total"].as_u64().unwrap() >= 1 && some["total"].as_u64() < cash["total"].as_u64());
    assert!(some["transactions"].as_array().unwrap().iter().all(|t| t["accountId"] == "3" && t["name"].as_str().unwrap().to_lowercase().contains("taxi")));
    // each account has its own rows, and together they are all of them
    let total: u64 = ["1", "2", "3", "4", "5"].iter().map(|id| ok(&vault, "transactions.list", json!({ "accountId": id }))["total"].as_u64().unwrap()).sum();
    assert_eq!(total, all["total"].as_u64().unwrap());
    // an account that is not there has no rows; a shape that is not an id is refused
    assert_eq!(ok(&vault, "transactions.list", json!({ "accountId": "999999" }))["total"], 0);
    for id in ["0", "-1", "abc", "", "01", "1.5", &"9".repeat(19)] {
        refuse(&vault, "transactions.list", json!({ "accountId": id }), INVALID);
    }
    refuse(&vault, "transactions.list", json!({ "accountId": 3 }), INVALID);
}

#[test]
fn a_row_cannot_be_made_in_or_moved_into_an_unknown_or_archived_account_but_may_stay_in_one() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let a = account(&vault, "Soon archived");
    let tx = id_of(&spend(&vault, &food, "Boots", "40.00", "2026-10-03", Some(&a)));
    let rec = id_of(&ok(&vault, "recurring.create", json!({ "name": "Gym", "amount": "20.00", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1, "accountId": a })));
    let led = id_of(&ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "30.00", "date": "2026-10-02", "accountId": a })));
    let (other_tx, other_rec, other_led) = (id_of(&spend(&vault, &food, "Bread", "2.00", "2026-10-01", None)), id_of(&ok(&vault, "recurring.create", json!({ "name": "Rent", "amount": "9.00", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1 }))), id_of(&ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "1.00", "date": "2026-10-02" }))));
    ok(&vault, "accounts.archive", json!({ "id": a }));

    for gone in [a.as_str(), "999999"] {
        refuse(&vault, "transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": food, "accountId": gone }), INVALID);
        refuse(&vault, "recurring.create", json!({ "name": "X", "amount": "1.00", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1, "accountId": gone }), INVALID);
        refuse(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "1.00", "date": "2026-10-14", "accountId": gone }), INVALID);
        refuse(&vault, "people.settle", json!({ "id": anna, "amount": "1.00", "accountId": gone }), INVALID);
        refuse(&vault, "transactions.update", json!({ "id": other_tx, "accountId": gone }), INVALID);
        refuse(&vault, "recurring.update", json!({ "id": other_rec, "accountId": gone }), INVALID);
        refuse(&vault, "ledger.update", json!({ "id": other_led, "accountId": gone }), INVALID);
        // a borrowing that adds an expense: the expense is refused with it, and neither is made
        refuse(&vault, "ledger.add", json!({ "personId": anna, "kind": "BORROWED", "amount": "1.00", "date": "2026-10-14", "expenseName": "Pizza", "expenseCategoryId": food, "accountId": gone }), INVALID);
    }
    // a row that is already in the archived account may be edited, and may name that account again
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": tx, "amount": "41.00" }))["accountId"], a.as_str());
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": tx, "accountId": a, "name": "Boots 2" }))["accountId"], a.as_str());
    assert_eq!(ok(&vault, "recurring.update", json!({ "id": rec, "accountId": a, "name": "Gym 2" }))["accountId"], a.as_str());
    assert_eq!(ok(&vault, "ledger.update", json!({ "id": led, "accountId": a, "amount": "31.00" }))["accountId"], a.as_str());
    assert_eq!(held(&vault, &a), "-72.00", "an archived account still holds what it holds: the 41 and the loan of 31");
    // and may leave it for an account that is not archived
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": tx, "accountId": "1" }))["accountId"], "1");
    assert_eq!(ok(&vault, "ledger.update", json!({ "id": led, "accountId": "1" }))["accountId"], "1");
    // a row in an archived account can be deleted
    assert_eq!(ok(&vault, "recurring.delete", json!({ "id": rec })), Value::Null);
}

#[test]
fn a_recurring_item_pays_in_its_account_or_in_the_main_one_when_that_is_archived() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let (a, b) = (account(&vault, "Card"), account(&vault, "Cash"));
    let make = |name: &str, account: &str| id_of(&ok(&vault, "recurring.create", json!({ "name": name, "amount": "20.00", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1, "accountId": account })));
    let (on_card, on_cash) = (make("Streaming", &a), make("Lunch club", &b));
    assert_eq!(ok(&vault, "recurring.pay", json!({ "id": on_card, "paidOn": "2026-10-14" }))["accountId"], a.as_str(), "the payment is in the account of the item");
    assert_eq!(held(&vault, &a), "-20.00");
    ok(&vault, "accounts.archive", json!({ "id": b }));
    assert_eq!(ok(&vault, "recurring.pay", json!({ "id": on_cash, "paidOn": "2026-10-14" }))["accountId"], "1", "the account is archived: the payment is made in the main account");
    let item = ok(&vault, "recurring.list", json!({})).as_array().unwrap().iter().find(|r| r["id"] == on_cash.as_str()).unwrap().clone();
    assert_eq!(item["accountId"], b.as_str(), "and the item is not changed");
    assert_eq!((held(&vault, "1"), held(&vault, &b)), ("-20.00".into(), "0.00".to_string()));
}

#[test]
fn a_friend_who_pays_back_pays_into_the_account_named_and_by_default_into_the_main_one() {
    let (_dir, vault) = fresh();
    let a = account(&vault, "Savings");
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let loan = ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "25.00", "date": "2026-10-01" }));
    assert_eq!(loan["accountId"], "1", "the loan left the main account");
    let into_a = ok(&vault, "people.settle", json!({ "id": anna, "amount": "10.00", "date": "2026-10-05", "accountId": a }));
    assert_eq!((into_a["entry"]["kind"].as_str(), into_a["entry"]["accountId"].as_str(), into_a["before"].as_str(), into_a["after"].as_str()), (Some("REPAID_TO_ME"), Some(a.as_str()), Some("25.00"), Some("15.00")));
    assert_eq!((held(&vault, &a), held(&vault, "1")), ("10.00".into(), "-25.00".to_string()));
    let by_default = ok(&vault, "people.settle", json!({ "id": anna, "amount": "5.00", "date": "2026-10-06" }));
    assert_eq!(by_default["entry"]["accountId"], "1");
    assert_eq!((held(&vault, &a), held(&vault, "1")), ("10.00".into(), "-20.00".to_string()));
    // the whole balance, into an account
    let rest = ok(&vault, "people.settle", json!({ "id": anna, "date": "2026-10-07", "accountId": a }));
    assert_eq!((rest["entry"]["amount"].as_str(), rest["entry"]["accountId"].as_str(), rest["after"].as_str()), (Some("10.00"), Some(a.as_str()), Some("0.00")));
    assert_eq!(held(&vault, &a), "20.00");
    // a friend who owes nothing is not settled, into any account
    refuse(&vault, "people.settle", json!({ "id": anna, "accountId": a }), INVALID);
}

#[test]
fn a_borrowing_with_an_expense_puts_both_in_the_same_account() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let a = account(&vault, "Cash");
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let entry = ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "BORROWED", "amount": "12.00", "date": "2026-10-03", "expenseName": "Pizza", "expenseCategoryId": food, "accountId": a }));
    assert_eq!(entry["accountId"], a.as_str());
    let pizza = ok(&vault, "transactions.list", json!({ "q": "Pizza" }));
    assert_eq!((pizza["total"].as_u64(), pizza["transactions"][0]["accountId"].as_str()), (Some(1), Some(a.as_str())));
    assert_eq!(pizza["transactions"][0]["id"], entry["transactionId"]);
    assert_eq!(held(&vault, &a), "0.00", "the expense and the money borrowed cancel: the friend paid");
    assert_eq!(find(&vault, &a)["entries"], 2);
    assert_eq!(held(&vault, "1"), "0.00");
    // without an account, both are in the main one
    let plain = ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "BORROWED", "amount": "3.00", "date": "2026-10-03", "expenseName": "Coffee", "expenseCategoryId": food }));
    assert_eq!(plain["accountId"], "1");
    assert_eq!(ok(&vault, "transactions.list", json!({ "q": "Coffee" }))["transactions"][0]["accountId"], "1");
}

// ---- the arguments ----------------------------------------------------------------------------

#[test]
fn an_unknown_argument_is_refused_by_every_new_route_and_a_name_in_the_wrong_case_is_unknown() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    for (name, body) in requests(&w) {
        for extra in ["extra", "accountID", "account-id", "Id", "kind ", "categoryId"] {
            let mut with = body.clone();
            with[extra] = json!("1");
            assert_eq!(refuse(&vault, name, with, INVALID), format!("unknown argument `{extra}`"), "{name}");
        }
    }
    // the routes that were there before do not know the names of the new ones
    refuse(&vault, "categories.create", json!({ "name": "X", "type": "EXPENSE", "accountId": "1" }), INVALID);
    refuse(&vault, "transactions.delete", json!({ "id": "1", "accountId": "1" }), INVALID);
    refuse(&vault, "transactions.split", json!({ "id": "1", "mode": "EQUAL", "accountId": "1" }), INVALID);
    refuse(&vault, "recurring.pay", json!({ "id": "1", "accountId": "1" }), INVALID);
}

#[test]
fn a_missing_argument_is_refused_by_name() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    let mut tried = 0;
    for (name, body) in requests(&w) {
        for param in route(name).params.iter().filter(|p| p.required) {
            let mut without = body.clone();
            without.as_object_mut().unwrap().remove(param.name);
            assert_eq!(refuse(&vault, name, without, INVALID), format!("missing argument `{}`", param.name), "{name}");
            tried += 1;
        }
        // nothing at all
        if route(name).params.iter().any(|p| p.required) {
            assert!(refuse(&vault, name, json!({}), INVALID).starts_with("missing argument `"), "{name}");
        }
    }
    assert_eq!(tried, 16, "every required argument of every route");
    // the old routes have not started to need the account
    for name in WITH_ACCOUNT {
        assert!(!route(name).params.iter().any(|p| p.name == "accountId" && p.required), "{name}");
    }
}

/// Values of the wrong kind for an argument, each of which the shape check in front of the route refuses.
fn wrong(kind: Kind, nullable: bool) -> Vec<Value> {
    let mut values = match kind {
        Kind::String => vec![json!(5), json!(true), json!(["a"]), json!({ "a": 1 }), json!(1.5)],
        Kind::Id => vec![
            json!("0"),
            json!("00"),
            json!("-1"),
            json!("abc"),
            json!("1.5"),
            json!(" 1"),
            json!("1 "),
            json!(""),
            json!("１"),
            json!("9".repeat(19)),
            json!("1e3"),
            json!(5),
            json!(1.0),
            json!(["1"]),
            json!(true),
        ],
        Kind::Date => vec![json!("2026-2-30"), json!("tomorrow"), json!("2026-10-14T00:00:00Z"), json!(""), json!("２０２６-10-14"), json!(20261014), json!(true)],
        Kind::Money => vec![json!("1.5"), json!("-1.00"), json!("+1.00"), json!("1e3"), json!(""), json!(12.5), json!(12), json!("１.００"), json!("10000000000.00"), json!(["1.00"])],
        Kind::Integer => vec![json!("10"), json!(1.5), json!(true), json!([1]), json!({})],
        Kind::Year => vec![json!("26"), json!("20266"), json!("abcd"), json!(""), json!(2026), json!(true)],
        Kind::Ids => vec![json!([1]), json!(["abc"]), json!(["0"]), json!(["1", "-1"]), json!("1,2"), json!(7), json!({}), json!((0..501).map(|n| (n + 1).to_string()).collect::<Vec<_>>())],
        _ => vec![],
    };
    if !nullable {
        values.push(Value::Null);
    }
    values
}

#[test]
fn a_value_of_the_wrong_kind_is_refused_by_name_and_changes_nothing() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    let mut tried = 0;
    for (name, body) in requests(&w) {
        for param in route(name).params {
            for value in wrong(param.kind, param.nullable) {
                let mut bad = body.clone();
                bad[param.name] = value.clone();
                let message = refuse(&vault, name, bad, INVALID);
                assert!(message.starts_with(&format!("`{}` must be ", param.name)), "{name} {} = {value}: {message}", param.name);
                tried += 1;
            }
        }
    }
    assert!(tried > 250, "{tried}");
    // the same for the argument the old routes gained
    let food = category(&vault, "EXPENSE", "Food");
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let tx = id_of(&spend(&vault, &food, "Bread", "2.00", "2026-10-01", None));
    for value in wrong(Kind::Id, false) {
        for (name, body) in [
            ("transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": food })),
            ("transactions.update", json!({ "id": tx })),
            ("transactions.list", json!({})),
            ("recurring.create", json!({ "name": "X", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1 })),
            ("recurring.update", json!({ "id": "1" })),
            ("ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "1.00", "date": "2026-10-14" })),
            ("ledger.update", json!({ "id": "1" })),
            ("people.settle", json!({ "id": anna })),
        ] {
            let mut bad = body.clone();
            bad["accountId"] = value.clone();
            let message = refuse(&vault, name, bad, INVALID);
            assert!(message.starts_with("`accountId` must be "), "{name} {value}: {message}");
        }
    }
}

#[test]
fn an_id_that_is_far_too_big_or_of_no_account_is_not_found_or_refused_and_never_a_failure() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    let huge = "999999999999999999";
    for (name, body) in [
        ("accounts.show", json!({ "id": huge })),
        ("accounts.history", json!({ "id": huge, "year": "2026" })),
        ("accounts.update", json!({ "id": huge, "name": "X" })),
        ("accounts.archive", json!({ "id": huge })),
        ("accounts.unarchive", json!({ "id": huge })),
        ("accounts.delete", json!({ "id": huge })),
        ("transfers.update", json!({ "id": huge, "amount": "1.00" })),
        ("transfers.delete", json!({ "id": huge })),
    ] {
        refuse(&vault, name, body, NOT_FOUND);
    }
    for (name, body) in [
        ("accounts.reorder", json!({ "ids": [huge] })),
        ("accounts.reorder", json!({ "ids": ["1", &w.a, &w.b, &w.empty, huge] })),
        ("transfers.create", json!({ "fromAccountId": huge, "toAccountId": w.a, "amount": "1.00", "date": "2026-10-01" })),
        ("transfers.create", json!({ "fromAccountId": w.a, "toAccountId": huge, "amount": "1.00", "date": "2026-10-01" })),
        ("transfers.update", json!({ "id": w.transfer, "toAccountId": huge })),
        ("transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": "1", "accountId": huge })),
    ] {
        let (status, _) = refused(&vault, name, body.clone());
        assert_eq!(status, 400, "{name} {body}");
    }
    assert_eq!(ok(&vault, "transfers.list", json!({ "accountId": huge }))["total"], 0);
    assert_eq!(ok(&vault, "transactions.list", json!({ "accountId": huge }))["total"], 0);
}

#[test]
fn no_message_of_a_refusal_holds_what_the_database_said() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    let food = category(&vault, "EXPENSE", "Food");
    spend(&vault, &food, "Bread", "2.00", "2026-10-01", Some(&w.b));
    // the refusals that come from the rules under the routes: the foreign keys, the checks, the unique names
    for (name, body, expected) in [
        ("accounts.delete", json!({ "id": w.b }), INVALID),
        ("accounts.delete", json!({ "id": "1" }), INVALID),
        ("accounts.archive", json!({ "id": "1" }), INVALID),
        ("accounts.create", json!({ "name": "alpha", "kind": "CASH" }), NAME_EXISTS),
        ("accounts.update", json!({ "id": w.b, "name": "ALPHA" }), NAME_EXISTS),
        ("accounts.update", json!({ "id": w.archived, "name": "Fresh" }), INVALID),
        ("accounts.update", json!({ "id": w.b, "openingBalance": "10000000000.00" }), INVALID),
        ("accounts.create", json!({ "name": "Big", "kind": "CASH", "openingBalance": "-10000000000.00" }), INVALID),
        ("accounts.reorder", json!({ "ids": [w.a] }), INVALID),
        ("transfers.create", json!({ "fromAccountId": w.a, "toAccountId": w.a, "amount": "1.00", "date": "2026-10-01" }), INVALID),
        ("transfers.create", json!({ "fromAccountId": w.a, "toAccountId": w.archived, "amount": "1.00", "date": "2026-10-01" }), INVALID),
        ("transfers.create", json!({ "fromAccountId": w.a, "toAccountId": w.b, "amount": "0.00", "date": "2026-10-01" }), INVALID),
        ("transfers.update", json!({ "id": w.transfer, "toAccountId": w.a }), INVALID),
        ("transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": food, "accountId": w.archived }), INVALID),
        ("accounts.show", json!({ "id": "999999" }), NOT_FOUND),
    ] {
        refuse(&vault, name, body, expected);
    }
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
fn a_read_token_reads_accounts_and_transfers_and_cannot_write_any_of_them() {
    let h = http();
    let port = h.server.port();
    let before = dump(&h.vault);
    let reads: [(&str, &str); 5] = [
        ("accounts.list", "{}"),
        ("accounts.show", r#"{"id":"2"}"#),
        ("accounts.history", r#"{"id":"2","year":"2026"}"#),
        ("accounts.summary", "{}"),
        ("transfers.list", r#"{"limit":5}"#),
    ];
    for (name, body) in reads {
        let reply = call(port, Some(&h.read), name, body);
        assert_eq!(reply.status, 200, "{name}: {}", reply.body);
        assert_eq!(reply.json()["ok"], true, "{name}");
        assert!(!reply.body.contains(&h.read) && !reply.body.contains(&h.write), "{name}");
    }
    assert_eq!(call(port, Some(&h.read), "accounts.list", "{}").data().as_array().unwrap().len(), 5);

    // every write route: a read token is refused before anything else is looked at, with a valid request and with none
    let world = World { a: "2".into(), b: "3".into(), archived: "5".into(), empty: "2".into(), transfer: "1".into(), active: vec!["1".into(), "2".into(), "3".into(), "4".into()] };
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
    assert_eq!(tried, 9);
    // the old routes with the new argument are refused to it as well
    let food = ok(&h.vault, "categories.list", json!({ "type": "EXPENSE" }))[0]["id"].as_str().unwrap().to_string();
    let request = json!({ "name": "Bread", "amount": "2.40", "paymentDate": "2026-10-14", "categoryId": food, "accountId": "3" }).to_string();
    assert_eq!(call(port, Some(&h.read), "transactions.create", &request).status, 403);
    assert_eq!(dump(&h.vault), before, "a read token changed something");
    // and without any token, or with a wrong one, nothing is told
    for name in READ_ROUTES.iter().chain(WRITE_ROUTES.iter()) {
        assert_eq!(call(port, None, name, "{}").code(), "unauthorized", "{name}");
        assert_eq!(call(port, Some(&format!("mny_{}", "0".repeat(64))), name, "{}").code(), "unauthorized", "{name}");
    }
}

#[test]
fn a_write_token_makes_and_changes_accounts_over_http_and_the_read_token_sees_it() {
    let h = http();
    let port = h.server.port();
    let post = |token: &str, route: &str, body: Value| {
        let reply = call(port, Some(token), route, &body.to_string());
        assert_eq!(reply.status, 200, "{route}: {}", reply.body);
        assert_eq!(reply.json()["ok"], true);
        reply.data()
    };
    let made = post(&h.write, "accounts.create", json!({ "name": "Rainy day", "kind": "SAVINGS", "openingBalance": "-120.00", "openingDate": "2026-01-01" }));
    assert_eq!((made["openingBalance"].as_str(), made["name"].as_str()), (Some("-120.00"), Some("Rainy day")));
    let id = id_of(&made);
    let moved = post(&h.write, "transfers.create", json!({ "fromAccountId": "1", "toAccountId": id, "amount": "200.00", "date": "2026-10-03", "note": "first" }));
    assert_eq!(keys(&moved), TRANSFER_KEYS);
    let shown = post(&h.read, "accounts.show", json!({ "id": id, "from": "2026-01-01", "to": "2026-12-31" }));
    assert_eq!(shown["lines"].as_array().unwrap().iter().map(|l| l["source"].as_str().unwrap()).collect::<Vec<_>>(), ["OPENING", "TRANSFER_IN"]);
    assert_eq!(shown["balanceAfter"], "80.00");
    let page = post(&h.read, "transfers.list", json!({ "accountId": id }));
    assert_eq!((page["total"].as_u64(), page["transfers"][0]["id"].as_str()), (Some(1), moved["id"].as_str()));
    assert_eq!(post(&h.write, "accounts.update", json!({ "id": id, "name": "Emergency", "number": null }))["name"], "Emergency");
    assert_eq!(post(&h.write, "transfers.delete", json!({ "id": id_of(&moved) })), Value::Null);
    assert_eq!(post(&h.write, "accounts.delete", json!({ "id": id })), Value::Null);
    // a refusal over HTTP: the status and code of the note, and the server is still up
    let taken = call(port, Some(&h.write), "accounts.create", r#"{"name":"cash","kind":"CASH"}"#);
    assert_eq!((taken.status, taken.code().as_str(), taken.message().as_str()), (409, "name_exists", "that name already exists"));
    let main = call(port, Some(&h.write), "accounts.archive", r#"{"id":"1"}"#);
    assert_eq!((main.status, main.code().as_str()), (400, "invalid_input"));
    assert_eq!(call(port, Some(&h.write), "accounts.show", r#"{"id":"999999"}"#).status, 404);
    assert_eq!(call(port, Some(&h.read), "status", "").status, 200);
    assert_eq!(h.server.panics(), 0);
}

#[test]
fn the_person_is_told_that_a_file_holds_accounts_and_transfers() {
    let r = rig(true);
    let json = r.path("all.json");
    assert_eq!(r.post("export.json", json!({ "out": json })).status, 200);
    assert!(r.last().contains("5 accounts by name, type, number and balance, and ") && r.last().contains(" transfers between them"), "{}", r.last());
    let file: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(file["accounts"].as_array().unwrap().len(), 5, "what the person was told is what the file holds");
    assert!(file["transfers"].as_array().unwrap().len() >= 16);
    // a CSV is only the transactions, and says nothing of accounts
    assert_eq!(r.post("export.csv", json!({ "out": r.path("t.csv") })).status, 200);
    assert!(!r.last().contains("accounts"), "{}", r.last());
}
