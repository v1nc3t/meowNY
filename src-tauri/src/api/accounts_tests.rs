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

const ACCOUNT_KEYS: [&str; 29] = [
    "archived", "available", "balance", "balanceInMain", "bic", "changeThisMonth", "currency", "entries", "foreign", "free", "group", "id", "includeInNetWorth", "installment", "interestPeriod", "interestRate",
    "isMain", "kind", "lastActivity", "liability", "name", "notes", "number", "openingBalance", "openingDate", "rate", "returnPerMonth", "setAside", "virtualBalance",
];
const TRANSFER_KEYS: [&str; 7] = ["amount", "date", "fromAccountId", "id", "note", "toAccountId", "toAmount"];

const READ_ROUTES: [&str; 6] = ["accounts.list", "accounts.show", "accounts.history", "accounts.breakdown", "accounts.summary", "transfers.list"];
const WRITE_ROUTES: [&str; 10] = [
    "accounts.create",
    "accounts.update",
    "accounts.archive",
    "accounts.unarchive",
    "accounts.delete",
    "accounts.reorder",
    "accounts.pay",
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
        ("accounts.breakdown", json!({ "id": w.a, "from": "2026-10-01", "to": "2026-10-31" })),
        ("accounts.summary", json!({})),
        ("transfers.list", json!({ "accountId": w.a, "from": "2026-10-01", "to": "2026-10-31", "limit": 10, "offset": 0 })),
        (
            "accounts.create",
            json!({ "name": "New", "kind": "CASH", "number": "N1", "openingBalance": "1.00", "openingDate": "2026-10-01", "bic": "BIC1", "notes": "n", "group": "G", "includeInNetWorth": false, "virtualBalance": "-5.00", "currency": "RON", "rate": "5.00", "interestRate": "3.25", "interestPeriod": "YEARLY", "installment": "950.00" }),
        ),
        (
            "accounts.update",
            json!({ "id": w.a, "name": "Renamed", "kind": "SAVINGS", "number": "N2", "openingBalance": "2.00", "openingDate": "2026-10-02", "bic": "BIC2", "notes": "m", "group": "H", "includeInNetWorth": false, "virtualBalance": "5.00", "currency": "EUR", "interestRate": "3.25", "interestPeriod": "MONTHLY", "installment": "950.00" }),
        ),
        ("accounts.archive", json!({ "id": w.a })),
        ("accounts.unarchive", json!({ "id": w.archived })),
        ("accounts.delete", json!({ "id": w.empty })),
        ("accounts.reorder", json!({ "ids": w.active })),
        ("accounts.pay", json!({ "id": w.a, "fromAccountId": w.b, "amount": "950.00", "interest": "585.00", "categoryId": "1", "date": "2026-10-01", "note": "n" })),
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
    let table: [(&str, &[Arg]); 16] = [
        ("accounts.list", &[]),
        ("accounts.show", &[("id", Kind::Id, true, false), ("from", Kind::Date, false, false), ("to", Kind::Date, false, false)]),
        ("accounts.history", &[("id", Kind::Id, true, false), ("year", Kind::Year, true, false)]),
        ("accounts.breakdown", &[("id", Kind::Id, true, false), ("from", Kind::Date, false, false), ("to", Kind::Date, false, false)]),
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
                ("bic", Kind::String, false, false),
                ("notes", Kind::String, false, false),
                ("group", Kind::String, false, false),
                ("includeInNetWorth", Kind::Boolean, false, false),
                ("virtualBalance", Kind::String, false, false),
                ("currency", Kind::String, false, false),
                ("rate", Kind::String, false, false),
                ("interestRate", Kind::String, false, false),
                ("interestPeriod", Kind::String, false, false),
                ("installment", Kind::Money, false, false),
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
                ("bic", Kind::String, false, true),
                ("notes", Kind::String, false, true),
                ("group", Kind::String, false, true),
                ("includeInNetWorth", Kind::Boolean, false, false),
                ("virtualBalance", Kind::String, false, false),
                ("currency", Kind::String, false, false),
                ("rate", Kind::String, false, false),
                ("balance", Kind::String, false, false),
                ("interestRate", Kind::String, false, true),
                ("interestPeriod", Kind::String, false, false),
                ("installment", Kind::Money, false, true),
            ],
        ),
        ("accounts.archive", &[("id", Kind::Id, true, false)]),
        ("accounts.unarchive", &[("id", Kind::Id, true, false)]),
        ("accounts.delete", &[("id", Kind::Id, true, false)]),
        ("accounts.reorder", &[("ids", Kind::Ids, true, false)]),
        (
            "accounts.pay",
            &[
                ("id", Kind::Id, true, false),
                ("fromAccountId", Kind::Id, true, false),
                ("amount", Kind::Money, true, false),
                ("interest", Kind::Money, false, false),
                ("categoryId", Kind::Id, false, false),
                ("date", Kind::Date, false, false),
                ("note", Kind::String, false, false),
            ],
        ),
        (
            "transfers.create",
            &[
                ("fromAccountId", Kind::Id, true, false),
                ("toAccountId", Kind::Id, true, false),
                ("amount", Kind::Money, true, false),
                ("date", Kind::Date, true, false),
                ("note", Kind::String, false, false),
                ("toAmount", Kind::Money, false, false),
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
                ("toAmount", Kind::Money, false, false),
            ],
        ),
        ("transfers.delete", &[("id", Kind::Id, true, false)]),
    ];
    for (name, expected) in table {
        let got: Vec<Arg> = route(name).params.iter().map(|p| (p.name, p.kind, p.required, p.nullable)).collect();
        assert_eq!(got, expected, "{name}");
    }
    // The kind is the seven words of the core, and the opening balance, the balance and the virtual balance are text because money has no sign.
    // The currency is text, as the rate is: the core says what it takes. The two other amounts are money as `amount` is, so they take what it takes.
    let kinds = [
        crate::accounts::Kind::Checking,
        crate::accounts::Kind::Savings,
        crate::accounts::Kind::Cash,
        crate::accounts::Kind::CreditCard,
        crate::accounts::Kind::Credit,
        crate::accounts::Kind::Loan,
        crate::accounts::Kind::Mortgage,
    ];
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
        .filter(|r| r.params.iter().any(|p| p.name == "accountId") && !r.name.starts_with("accounts.") && !r.name.starts_with("transfers.") && !r.name.starts_with("stashes."))
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
    assert_eq!(
        names,
        [
            "Current account", "Savings", "Cash", "Credit card", "Lei account", "Flat", "Car", "Emergency fund at the credit union, long term", "Credit line", "Old savings",
            "Appliance loan"
        ],
        "the person's order, the archived ones last"
    );
    for a in &list {
        assert_eq!(keys(a), ACCOUNT_KEYS, "{a}");
        assert!(a["id"].as_str().unwrap().bytes().all(|b| b.is_ascii_digit()), "{a}");
        for money in ["openingBalance", "balance", "changeThisMonth", "virtualBalance", "available", "balanceInMain"] {
            assert!(is_money(a[money].as_str().unwrap_or("not text")), "{money} of {a}");
        }
        // the currency is always a code, the rate is text exactly for an account that is foreign, and an account in the app's currency is worth what it holds
        assert!(a["currency"].as_str().is_some_and(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase())) && a["foreign"].is_boolean(), "{a}");
        assert_eq!(a["rate"].is_string(), a["foreign"] == true, "{a}");
        assert!(a["foreign"] == true || (a["currency"] == "EUR" && a["balanceInMain"] == a["balance"]), "{a}");
        assert!(a["archived"].is_boolean() && a["isMain"].is_boolean() && a["includeInNetWorth"].is_boolean() && a["entries"].is_u64(), "{a}");
        assert!(a["number"].is_null() || a["number"].is_string(), "{a}");
        for text in ["bic", "notes", "group"] {
            assert!(a[text].is_null() || a[text].as_str().is_some_and(|t| !t.is_empty()), "{text} of {a}: text, or null for none, never an empty text");
        }
        assert_eq!(cents(a["available"].as_str().unwrap()), cents(a["balance"].as_str().unwrap()) + cents(a["virtualBalance"].as_str().unwrap()), "{a}");
        assert!(a["lastActivity"].is_null() || a["lastActivity"].as_str().is_some_and(|d| d.len() == 10), "{a}");
        assert!(["CHECKING", "SAVINGS", "CASH", "CREDIT_CARD", "CREDIT", "LOAN", "MORTGAGE"].contains(&a["kind"].as_str().unwrap()), "{a}");
    }
    assert_eq!((list[0]["id"].as_str(), list[0]["isMain"].as_bool()), (Some("1"), Some(true)));
    assert_eq!(list.iter().filter(|a| a["isMain"] == true).count(), 1);
    assert_eq!(list.iter().map(|a| a["archived"].as_bool().unwrap()).collect::<Vec<_>>(), [false, false, false, false, false, false, false, false, false, true, true]);
    // the demo's account in lei is the one foreign account: its rate is text, and it is worth its balance divided by the rate
    assert_eq!(list.iter().filter(|a| a["foreign"] == true).map(|a| a["name"].as_str().unwrap()).collect::<Vec<_>>(), ["Lei account"]);
    let lei_account = &list[4];
    assert_eq!((lei_account["id"].as_str(), lei_account["currency"].as_str(), lei_account["rate"].as_str()), (Some("6"), Some("RON"), Some("5.00")));
    assert_eq!(cents(lei_account["balanceInMain"].as_str().unwrap()), (cents(lei_account["balance"].as_str().unwrap()) as f64 / 5.0).round() as i64, "the balance at 5.00 lei to the euro");
    assert_eq!(list[3]["kind"], "CREDIT_CARD");
    assert!(cents(list[3]["balance"].as_str().unwrap()) < 0 && cents(list[3]["openingBalance"].as_str().unwrap()) < 0, "a card is in the red");

    // a statement: the account, the days, what it held before and after, and a line for each movement
    let statement = ok(&vault, "accounts.show", json!({ "id": "1" }));
    assert_eq!(keys(&statement), ["account", "balanceAfter", "balanceBefore", "currency", "from", "lines", "to"]);
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
    assert_eq!(keys(&summary), ["accounts", "debts", "friends", "inAccounts", "inUse", "netWorth"]);
    assert_eq!((summary["accounts"].as_u64(), summary["inUse"].as_bool()), (Some(11), Some(true)));
    for money in ["debts", "friends", "inAccounts", "netWorth"] {
        assert!(is_money(summary[money].as_str().unwrap()), "{money}");
    }
    // the accounts you owe on (the demo's mortgage, car loan and credit line) are the debts; the others are what is in the accounts
    let owed = |a: &&Value| ["CREDIT", "LOAN", "MORTGAGE"].contains(&a["kind"].as_str().unwrap());
    let counted = || list.iter().filter(|a| a["includeInNetWorth"] == true);
    let held_in_all: i64 = counted().filter(|a| !owed(a)).map(|a| cents(a["balanceInMain"].as_str().unwrap())).sum();
    let debts: i64 = counted().filter(owed).map(|a| cents(a["balanceInMain"].as_str().unwrap())).sum();
    assert!(debts < 0, "the demo owes on a mortgage and a car loan");
    assert_eq!(cents(summary["inAccounts"].as_str().unwrap()), held_in_all, "the accounts that count, the archived ones too, each in the app's currency; the others are left out");
    assert_eq!(cents(summary["debts"].as_str().unwrap()), debts, "what is owed on the debts, below 0");
    assert_eq!(cents(summary["netWorth"].as_str().unwrap()), held_in_all + debts + cents(summary["friends"].as_str().unwrap()));
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
        assert!(t["toAmount"].is_null() || t["toAmount"].as_str().is_some_and(|m| is_money(m) && cents(m) > 0), "{t}");
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
fn the_kind_of_an_account_is_one_of_seven_words_exactly() {
    let (_dir, vault) = fresh();
    // The route lists the words, as `categories.create` lists INCOME and EXPENSE; the command line matches them without regard to case.
    for kind in ["CHECKING", "SAVINGS", "CASH", "CREDIT_CARD", "CREDIT", "LOAN", "MORTGAGE"] {
        assert_eq!(ok(&vault, "accounts.create", json!({ "name": kind, "kind": kind }))["kind"], kind);
    }
    let accounts = listed(&vault);
    let id = id_of(&accounts[1]);
    for kind in ["savings", "Credit_Card", "credit card", "BANK", "", "CREDIT-CARD", "CHECKING "] {
        assert_eq!(refuse(&vault, "accounts.create", json!({ "name": "New", "kind": kind }), INVALID), "`kind` must be one of: CHECKING, SAVINGS, CASH, CREDIT_CARD, CREDIT, LOAN, MORTGAGE", "{kind:?}");
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

// ---- the details of an account -----------------------------------------------------------------

const DETAILS: [&str; 5] = ["bic", "notes", "group", "includeInNetWorth", "virtualBalance"];

/// A credit card with every detail set: a limit of 1000.00 on top of what it owes, and left out of net worth.
fn detailed(vault: &Vault, name: &str) -> Value {
    ok(
        vault,
        "accounts.create",
        json!({
            "name": name, "kind": "CREDIT_CARD", "openingBalance": "-240.00", "openingDate": "2026-01-01", "bic": "DEUTDEFF", "notes": "Limit raised\nin March", "group": "Bank",
            "includeInNetWorth": false, "virtualBalance": "1000.00"
        }),
    )
}

/// The same account again with one argument set: for a request that must work or must be refused on that one value.
fn with(base: Value, key: &str, value: Value) -> Value {
    let mut body = base;
    body[key] = value;
    body
}

#[test]
fn an_account_made_with_its_details_answers_them_and_made_without_answers_the_defaults() {
    let (_dir, vault) = fresh();
    let card = detailed(&vault, "Card");
    assert_eq!(keys(&card), ACCOUNT_KEYS);
    assert_eq!(
        (card["bic"].as_str(), card["notes"].as_str(), card["group"].as_str(), card["includeInNetWorth"].as_bool(), card["virtualBalance"].as_str(), card["available"].as_str(), card["balance"].as_str()),
        (Some("DEUTDEFF"), Some("Limit raised\nin March"), Some("Bank"), Some(false), Some("1000.00"), Some("760.00"), Some("-240.00"))
    );
    // the list and the statement say the same
    let id = id_of(&card);
    assert_eq!(find(&vault, &id), card);
    assert_eq!(ok(&vault, "accounts.show", json!({ "id": id }))["account"], card);

    // none of them: the defaults, and nothing to spend beyond the balance
    let plain = ok(&vault, "accounts.create", json!({ "name": "Plain", "kind": "CHECKING", "openingBalance": "35.50", "openingDate": "2026-01-01" }));
    assert_eq!(keys(&plain), ACCOUNT_KEYS);
    assert_eq!(
        (plain["bic"].clone(), plain["notes"].clone(), plain["group"].clone(), plain["includeInNetWorth"].clone(), plain["virtualBalance"].clone()),
        (Value::Null, Value::Null, Value::Null, json!(true), json!("0.00"))
    );
    assert_eq!((plain["available"].as_str(), plain["balance"].as_str()), (Some("35.50"), Some("35.50")));
    // the main account is made the same way by the migration
    let main = find(&vault, "1");
    assert_eq!((main["bic"].clone(), main["notes"].clone(), main["group"].clone(), main["includeInNetWorth"].clone(), main["virtualBalance"].clone(), main["available"].clone()), (Value::Null, Value::Null, Value::Null, json!(true), json!("0.00"), main["balance"].clone()));

    // each detail alone: the others are the defaults
    for (key, value) in [("bic", json!("BIC")), ("notes", json!("n")), ("group", json!("G")), ("includeInNetWorth", json!(false)), ("virtualBalance", json!("-5.00"))] {
        let made = ok(&vault, "accounts.create", json!({ "name": format!("Only {key}"), "kind": "CASH", key: value }));
        assert_eq!(made[key], value, "{key}");
        for other in DETAILS.iter().filter(|d| **d != key) {
            assert_eq!(made[*other], plain[*other], "{key} set alone changed {other}");
        }
    }
    // a person who leaves the new arguments out is not asked for them: the old request still works
    assert_eq!(ok(&vault, "accounts.create", json!({ "name": "Old way", "kind": "CASH", "number": "1" }))["includeInNetWorth"], true);
}

#[test]
fn the_details_are_accepted_at_their_bounds_and_refused_one_beyond_them() {
    let (_dir, vault) = fresh();
    let target = account(&vault, "Target");
    // (argument, the longest that is fine); lengths are counted in characters, not bytes
    for (key, longest) in [("bic", 20), ("notes", 1000), ("group", 50)] {
        for (n, text) in [if key == "group" { "g" } else { "x" }, "é"].into_iter().enumerate() {
            let (fits, too_long) = (text.repeat(longest), text.repeat(longest + 1));
            let made = ok(&vault, "accounts.create", json!({ "name": format!("Max {key} {n}"), "kind": "CASH", key: fits }));
            assert_eq!(made[key].as_str(), Some(fits.as_str()), "{key}");
            assert_eq!(ok(&vault, "accounts.update", json!({ "id": target, key: fits }))[key].as_str(), Some(fits.as_str()), "{key}");
            refuse(&vault, "accounts.create", json!({ "name": "Too long", "kind": "CASH", key: too_long }), INVALID);
            refuse(&vault, "accounts.update", json!({ "id": target, key: too_long }), INVALID);
        }
    }
    // a name or a BIC is one line; notes may have several, and the rest of the control characters are refused
    for key in ["bic", "group"] {
        for text in ["a\nb", "a\tb", "a\u{0}b", "\u{1b}[31mred", "bell\u{7}", "a\u{85}b"] {
            refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CASH", key: text }), INVALID);
            refuse(&vault, "accounts.update", json!({ "id": target, key: text }), INVALID);
        }
    }
    for text in ["a\tb", "a\u{0}b", "\u{1b}[31mred", "bell\u{7}", "a\u{85}b"] {
        refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CASH", "notes": text }), INVALID);
        refuse(&vault, "accounts.update", json!({ "id": target, "notes": text }), INVALID);
    }
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": target, "notes": "one\ntwo\n\nfour" }))["notes"], "one\ntwo\n\nfour");
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": target, "notes": "dos\r\nlines\rend" }))["notes"], "dos\nlines\nend", "a line break is one kind of line break");
    // trimmed, and a blank one is none
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": target, "bic": "  BIC  ", "group": " Bank ", "notes": "\n\n note \n" }))["bic"], "BIC");
    let trimmed = find(&vault, &target);
    assert_eq!((trimmed["group"].as_str(), trimmed["notes"].as_str()), (Some("Bank"), Some("note")));
    let blank = ok(&vault, "accounts.update", json!({ "id": target, "bic": "   ", "group": "\t ", "notes": "  \n  " }));
    assert_eq!((blank["bic"].clone(), blank["group"].clone(), blank["notes"].clone()), (Value::Null, Value::Null, Value::Null));
    assert_eq!(ok(&vault, "accounts.create", json!({ "name": "Blank", "kind": "CASH", "bic": " ", "notes": "\n", "group": "  " }))["group"], Value::Null);

    // the virtual balance is checked to the cent like an opening balance, and may be negative
    let good = ["0.00", "0.01", "-0.01", "-50.00", "9999999999.99", "-9999999999.99", "1234567890.12"];
    for (n, amount) in good.iter().enumerate() {
        let made = ok(&vault, "accounts.create", json!({ "name": format!("Virtual {n}"), "kind": "CREDIT_CARD", "virtualBalance": amount }));
        assert_eq!(made["virtualBalance"], *amount, "{amount} comes back as it was written");
        assert_eq!(made["balance"], "0.00", "and the balance is not touched");
        assert_eq!(ok(&vault, "accounts.update", json!({ "id": target, "virtualBalance": amount }))["virtualBalance"], *amount);
    }
    let bad = [
        "10000000000.00", "-10000000000.00", "9999999999.995", "99999999999.99", "-0.00", "+1.00", "1.5", "1", "-1", "1.", ".5", "-.50", "1.005", "--1.00", "- 1.00", " 1.00", "1.00 ", "1,00", "1e3", "NaN",
        "abc", "", "-", "０.００", "1.00\n", "0x10.00",
    ];
    for amount in bad {
        refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CREDIT_CARD", "virtualBalance": amount }), INVALID);
        refuse(&vault, "accounts.update", json!({ "id": target, "virtualBalance": amount }), INVALID);
    }
    // what is available can be more than either figure alone may be: it is a sum
    let both = ok(&vault, "accounts.create", json!({ "name": "Both", "kind": "CHECKING", "openingBalance": "9999999999.99", "openingDate": "2026-01-01", "virtualBalance": "9999999999.99" }));
    assert_eq!((both["balance"].as_str(), both["available"].as_str()), (Some("9999999999.99"), Some("19999999999.98")));
    let neither = ok(&vault, "accounts.create", json!({ "name": "Neither", "kind": "CHECKING", "openingBalance": "-9999999999.99", "openingDate": "2026-01-01", "virtualBalance": "-9999999999.99" }));
    assert_eq!(neither["available"], "-19999999999.98");
}

#[test]
fn a_detail_of_the_wrong_kind_is_refused_as_every_argument_of_that_kind_is() {
    let (_dir, vault) = fresh();
    let id = account(&vault, "Target");
    let create = json!({ "name": "New", "kind": "CASH" });
    let update = json!({ "id": id });
    // a switch is a JSON boolean and nothing else: not text, not 0 or 1, and not null (it has no "clear")
    for value in [json!("true"), json!("false"), json!("yes"), json!(""), json!(1), json!(0), json!(null), json!([true]), json!({ "a": true }), json!(1.5)] {
        assert_eq!(refuse(&vault, "accounts.create", with(create.clone(), "includeInNetWorth", value.clone()), INVALID), "`includeInNetWorth` must be a boolean", "{value}");
        assert_eq!(refuse(&vault, "accounts.update", with(update.clone(), "includeInNetWorth", value.clone()), INVALID), "`includeInNetWorth` must be a boolean", "{value}");
    }
    // the virtual balance is text, as the opening balance is, and not a JSON number
    for value in [json!(5), json!(5.5), json!(-1.0), json!(true), json!(null), json!(["1.00"]), json!({})] {
        assert_eq!(refuse(&vault, "accounts.create", with(create.clone(), "virtualBalance", value.clone()), INVALID), "`virtualBalance` must be a string", "{value}");
        assert_eq!(refuse(&vault, "accounts.update", with(update.clone(), "virtualBalance", value.clone()), INVALID), "`virtualBalance` must be a string", "{value}");
    }
    // the three texts: a number or a list is not text; null is a value only where an update clears
    for key in ["bic", "notes", "group"] {
        for value in [json!(5), json!(true), json!(["a"]), json!({ "a": 1 }), json!(1.5)] {
            assert_eq!(refuse(&vault, "accounts.create", with(create.clone(), key, value.clone()), INVALID), format!("`{key}` must be a string"));
            assert_eq!(refuse(&vault, "accounts.update", with(update.clone(), key, value.clone()), INVALID), format!("`{key}` must be a string"));
        }
        assert_eq!(refuse(&vault, "accounts.create", with(create.clone(), key, json!(null)), INVALID), format!("`{key}` must be a string"), "a new account has nothing to clear");
        ok(&vault, "accounts.update", with(update.clone(), key, json!(null)));
    }
    // names of the arguments are exact
    for wrong in ["BIC", "Bic", "include_in_net_worth", "includeInNetworth", "virtual_balance", "Group", "note", "available", "isMain"] {
        assert_eq!(refuse(&vault, "accounts.create", with(create.clone(), wrong, json!("x")), INVALID), format!("unknown argument `{wrong}`"));
        assert_eq!(refuse(&vault, "accounts.update", with(update.clone(), wrong, json!("x")), INVALID), format!("unknown argument `{wrong}`"));
    }
}

#[test]
fn each_detail_is_changed_alone_and_nothing_else_of_the_account_moves() {
    let (_dir, vault) = fresh();
    let card = detailed(&vault, "Card");
    let (id, other) = (id_of(&card), account(&vault, "Other"));
    let others = find(&vault, &other);
    // (argument, new value, what else the change shows)
    for (key, value, also) in [
        ("bic", json!("INGBNL2A"), None),
        ("notes", json!("New note"), None),
        ("group", json!("Credit"), None),
        ("includeInNetWorth", json!(true), None),
        ("virtualBalance", json!("-50.00"), Some(("available", "-290.00"))),
        ("virtualBalance", json!("0.00"), Some(("available", "-240.00"))),
        ("includeInNetWorth", json!(false), None),
    ] {
        let before = find(&vault, &id);
        let after = ok(&vault, "accounts.update", json!({ "id": id, key: value.clone() }));
        assert_eq!(after[key], value, "{key}");
        for (k, v) in after.as_object().unwrap() {
            if k != key && Some(k.as_str()) != also.map(|a| a.0) {
                assert_eq!(*v, before[k], "changing {key} changed {k}");
            }
        }
        if let Some((k, v)) = also {
            assert_eq!(after[k], v, "{key}: {k}");
        }
        assert_eq!(find(&vault, &id), after, "the list says what the update said");
        assert_eq!(find(&vault, &other), others, "and no other account moved");
    }
    // all five with the name, in one request
    let all = ok(&vault, "accounts.update", json!({ "id": id, "name": "Store card", "bic": "NEW", "notes": "N", "group": "Shops", "includeInNetWorth": true, "virtualBalance": "300.00" }));
    assert_eq!((all["name"].as_str(), all["bic"].as_str(), all["notes"].as_str(), all["group"].as_str(), all["includeInNetWorth"].as_bool(), all["available"].as_str()), (Some("Store card"), Some("NEW"), Some("N"), Some("Shops"), Some(true), Some("60.00")));
    assert_eq!(order(&vault), ["Main account", "Store card", "Other"], "the order of the list is not a detail");
    // a name that is taken still refuses the whole request: none of the details is kept
    let before = dump(&vault);
    refuse(&vault, "accounts.update", json!({ "id": id, "name": "OTHER", "bic": "LOST", "virtualBalance": "9.00" }), NAME_EXISTS);
    assert_eq!(dump(&vault), before);
    // an invalid detail refuses the valid ones sent with it
    refuse(&vault, "accounts.update", json!({ "id": id, "name": "Fine", "bic": "x".repeat(21), "group": "Kept?" }), INVALID);
    refuse(&vault, "accounts.update", json!({ "id": id, "group": "Kept?", "virtualBalance": "1.5" }), INVALID);
}

#[test]
fn bic_notes_and_group_are_cleared_with_null_or_an_empty_text_as_the_number_is_and_left_alone_when_left_out() {
    let (_dir, vault) = fresh();
    let id = id_of(&detailed(&vault, "Card"));
    ok(&vault, "accounts.update", json!({ "id": id, "number": "N1" }));
    for (key, again) in [("bic", "X1"), ("notes", "X2"), ("group", "X3")] {
        assert!(ok(&vault, "accounts.update", json!({ "id": id, "name": "Card" }))[key].is_string(), "{key}: left out, it stays");
        assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, key: null }))[key], Value::Null, "{key}: null clears");
        assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, key: again }))[key], again);
        assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, key: "" }))[key], Value::Null, "{key}: an empty one is none");
        assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, key: again }))[key], again);
        assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, key: " \n " }))[key], Value::Null, "{key}: so is a blank one");
        // clearing what is already clear answers the account and writes nothing
        let before = dump(&vault);
        assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, key: null }))[key], Value::Null);
        assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, key: "" }))[key], Value::Null);
        assert_eq!(dump(&vault), before, "{key}: cleared twice");
    }
    // clearing the three left the number, the switch and the virtual balance as they were
    let cleared = find(&vault, &id);
    assert_eq!((cleared["number"].as_str(), cleared["includeInNetWorth"].as_bool(), cleared["virtualBalance"].as_str()), (Some("N1"), Some(false), Some("1000.00")));
    // the switch and the virtual balance have no "none": they are always set
    for body in [json!({ "includeInNetWorth": null }), json!({ "virtualBalance": null }), json!({ "virtualBalance": "" })] {
        refuse(&vault, "accounts.update", with(body, "id", json!(id)), INVALID);
    }
    // 0.00 is how a virtual balance goes back to nothing, and then available is the balance
    let none = ok(&vault, "accounts.update", json!({ "id": id, "virtualBalance": "0.00" }));
    assert_eq!((none["virtualBalance"].as_str(), none["available"].as_str(), none["balance"].as_str()), (Some("0.00"), Some("-240.00"), Some("-240.00")));
    // and the answer of a clear on a new account that never had the details
    let fresh_one = account(&vault, "Fresh");
    assert_eq!(keys(&ok(&vault, "accounts.update", json!({ "id": fresh_one, "bic": null, "notes": null, "group": null }))), ACCOUNT_KEYS);
}

#[test]
fn a_patch_equal_to_the_current_details_answers_and_writes_nothing_but_a_real_change_does() {
    let (_dir, vault) = fresh();
    let card = detailed(&vault, "Card");
    let id = id_of(&card);
    let before = dump(&vault);
    for body in [
        json!({ "bic": "DEUTDEFF" }),
        json!({ "notes": "Limit raised\nin March" }),
        json!({ "group": "Bank" }),
        json!({ "includeInNetWorth": false }),
        json!({ "virtualBalance": "1000.00" }),
        json!({ "bic": "  DEUTDEFF ", "group": "Bank  " }),
        json!({ "notes": "Limit raised\r\nin March" }),
        json!({ "bic": "DEUTDEFF", "notes": "Limit raised\nin March", "group": "Bank", "includeInNetWorth": false, "virtualBalance": "1000.00" }),
        json!({ "name": "Card", "kind": "CREDIT_CARD", "openingBalance": "-240.00", "openingDate": "2026-01-01", "bic": "DEUTDEFF", "notes": "Limit raised\nin March", "group": "Bank", "includeInNetWorth": false, "virtualBalance": "1000.00" }),
        json!({}),
    ] {
        assert_eq!(ok(&vault, "accounts.update", with(body.clone(), "id", json!(id))), card, "{body}");
        assert_eq!(dump(&vault), before, "{body} wrote something: a stamp, a time or a line of the history");
    }
    // the check can see a change: one detail, and the stamp and the history move
    ok(&vault, "accounts.update", json!({ "id": id, "bic": "OTHER" }));
    assert_ne!(dump(&vault), before);
    // the case of a text is part of it
    let before = dump(&vault);
    ok(&vault, "accounts.update", json!({ "id": id, "bic": "other" }));
    assert_ne!(dump(&vault), before);
    // the details of an account that has none, set to what they are
    let plain = account(&vault, "Plain");
    let before = dump(&vault);
    ok(&vault, "accounts.update", json!({ "id": plain, "includeInNetWorth": true, "virtualBalance": "0.00" }));
    ok(&vault, "accounts.update", json!({ "id": plain, "bic": null, "notes": "", "group": "  " }));
    assert_eq!(dump(&vault), before);
}

#[test]
fn the_details_of_an_archived_account_are_kept_and_cannot_be_changed_until_it_is_brought_back() {
    let (_dir, vault) = fresh();
    let id = id_of(&detailed(&vault, "Card"));
    let archived = ok(&vault, "accounts.archive", json!({ "id": id }));
    assert_eq!((archived["bic"].as_str(), archived["includeInNetWorth"].as_bool(), archived["virtualBalance"].as_str(), archived["available"].as_str()), (Some("DEUTDEFF"), Some(false), Some("1000.00"), Some("760.00")), "archiving changes no detail");
    for body in [
        json!({ "bic": "NEW" }),
        json!({ "notes": "new" }),
        json!({ "group": "New" }),
        json!({ "includeInNetWorth": true }),
        json!({ "virtualBalance": "5.00" }),
        json!({ "bic": null }),
        json!({ "notes": "" }),
        json!({ "group": null }),
        json!({ "bic": "DEUTDEFF", "notes": "Limit raised\nin March", "group": "Bank", "includeInNetWorth": false, "virtualBalance": "1000.00" }),
    ] {
        refuse(&vault, "accounts.update", with(body, "id", json!(id)), INVALID);
    }
    assert_eq!(find(&vault, &id), archived);
    assert_eq!(ok(&vault, "accounts.show", json!({ "id": id }))["account"], archived, "an archived account is read with its details");
    // brought back, it takes the change
    assert_eq!(ok(&vault, "accounts.unarchive", json!({ "id": id }))["bic"], "DEUTDEFF");
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, "bic": "NEW", "includeInNetWorth": true }))["includeInNetWorth"], true);
    // an account can be made left out and put away at once, and stays left out
    let hidden = ok(&vault, "accounts.create", json!({ "name": "Hidden", "kind": "SAVINGS", "includeInNetWorth": false }));
    assert_eq!(ok(&vault, "accounts.archive", json!({ "id": id_of(&hidden) }))["includeInNetWorth"], false);
}

#[test]
fn the_main_account_takes_all_five_details_and_is_still_the_main_account() {
    let (_dir, vault) = fresh();
    let main = ok(&vault, "accounts.update", json!({ "id": "1", "bic": "MAINBIC", "notes": "The one", "group": "Home", "includeInNetWorth": false, "virtualBalance": "-50.00" }));
    assert_eq!(keys(&main), ACCOUNT_KEYS);
    assert_eq!(
        (main["isMain"].as_bool(), main["bic"].as_str(), main["notes"].as_str(), main["group"].as_str(), main["includeInNetWorth"].as_bool(), main["virtualBalance"].as_str(), main["available"].as_str()),
        (Some(true), Some("MAINBIC"), Some("The one"), Some("Home"), Some(false), Some("-50.00"), Some("-50.00"))
    );
    refuse(&vault, "accounts.archive", json!({ "id": "1" }), INVALID);
    refuse(&vault, "accounts.delete", json!({ "id": "1" }), INVALID);
    // and they are cleared again, one by one
    let back = ok(&vault, "accounts.update", json!({ "id": "1", "bic": null, "notes": null, "group": null, "includeInNetWorth": true, "virtualBalance": "0.00" }));
    assert_eq!((back["bic"].clone(), back["notes"].clone(), back["group"].clone(), back["includeInNetWorth"].clone(), back["virtualBalance"].clone()), (Value::Null, Value::Null, Value::Null, json!(true), json!("0.00")));
    assert_eq!(order(&vault), ["Main account"]);
}

// ---- net worth and what is available ------------------------------------------------------------

/// What `accounts.summary` says: what the accounts that count hold, what friends owe, and the two added. Checked against the list, so a
/// figure that disagrees with the balances shown fails here.
fn worth(vault: &Vault) -> (String, String, String) {
    let summary = ok(vault, "accounts.summary", json!({}));
    let counted: i64 = listed(vault).iter().filter(|a| a["includeInNetWorth"] == true).map(|a| cents(a["balanceInMain"].as_str().unwrap())).sum();
    let text = |key: &str| summary[key].as_str().unwrap().to_string();
    assert_eq!(cents(&text("inAccounts")), counted, "what the accounts that count hold: {summary}");
    assert_eq!(cents(&text("netWorth")), counted + cents(&text("friends")), "{summary}");
    (text("inAccounts"), text("friends"), text("netWorth"))
}

#[test]
fn net_worth_adds_only_the_accounts_that_count_and_counts_one_again_when_it_is_put_back() {
    let (_dir, vault) = fresh();
    // the main account holds 100.00 from January and lent 20.00; Savings holds 50.00; Old holds 30.00, is archived and is left out
    ok(&vault, "accounts.update", json!({ "id": "1", "openingBalance": "100.00", "openingDate": "2026-01-01" }));
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "20.00", "date": "2026-10-01" }));
    let savings = id_of(&ok(&vault, "accounts.create", json!({ "name": "Savings", "kind": "SAVINGS", "openingBalance": "50.00", "openingDate": "2026-01-01" })));
    let old = id_of(&ok(&vault, "accounts.create", json!({ "name": "Old", "kind": "SAVINGS", "openingBalance": "30.00", "openingDate": "2026-01-01", "includeInNetWorth": false })));
    ok(&vault, "accounts.archive", json!({ "id": old }));
    let all = |vault: &Vault| listed(vault).iter().map(|a| a["balance"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let balances = all(&vault);
    assert_eq!(balances, ["80.00", "50.00", "30.00"]);
    let triple = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
    assert_eq!(worth(&vault), triple("130.00", "20.00", "150.00"), "the archived account is left out, so it is not in it");

    // one left out: it is still listed with what it holds, and the account count says so
    ok(&vault, "accounts.update", json!({ "id": savings, "includeInNetWorth": false }));
    assert_eq!(worth(&vault), triple("80.00", "20.00", "100.00"));
    assert_eq!(all(&vault), balances, "a balance is what the account holds, whether net worth counts it");
    let summary = ok(&vault, "accounts.summary", json!({}));
    assert_eq!((summary["accounts"].as_u64(), summary["inUse"].as_bool()), (Some(3), Some(true)), "every account is counted as an account");
    // the main account left out too: nothing but what friends owe
    ok(&vault, "accounts.update", json!({ "id": "1", "includeInNetWorth": false }));
    assert_eq!(worth(&vault), triple("0.00", "20.00", "20.00"), "every account left out");
    assert_eq!(all(&vault), balances);
    // and back, one at a time
    ok(&vault, "accounts.update", json!({ "id": "1", "includeInNetWorth": true }));
    assert_eq!(worth(&vault), triple("80.00", "20.00", "100.00"));
    ok(&vault, "accounts.update", json!({ "id": savings, "includeInNetWorth": true }));
    assert_eq!(worth(&vault), triple("130.00", "20.00", "150.00"));
    // an archived account counts again when it counts, and brought back it counts as before
    ok(&vault, "accounts.unarchive", json!({ "id": old }));
    assert_eq!(worth(&vault), triple("130.00", "20.00", "150.00"), "brought back, it is still left out");
    ok(&vault, "accounts.update", json!({ "id": old, "includeInNetWorth": true }));
    assert_eq!(worth(&vault), triple("160.00", "20.00", "180.00"));
    ok(&vault, "accounts.archive", json!({ "id": old }));
    assert_eq!(worth(&vault), triple("160.00", "20.00", "180.00"), "the archived ones count when they count");
    // a virtual balance is in none of it, whatever its sign
    for virtual_balance in ["1000.00", "-1000.00", "0.00"] {
        ok(&vault, "accounts.update", json!({ "id": savings, "virtualBalance": virtual_balance }));
        assert_eq!(worth(&vault), triple("160.00", "20.00", "180.00"), "virtual balance {virtual_balance}");
    }
    // the move of money between a counted and a left out account is seen by net worth, as it is real
    ok(&vault, "accounts.update", json!({ "id": savings, "includeInNetWorth": false }));
    transfer(&vault, "1", &savings, "10.00", "2026-10-05");
    assert_eq!(worth(&vault), triple("100.00", "20.00", "120.00"), "10.00 went from a counted account to one that is left out");
    // a left out account is refused changes like any other when it is archived
    refuse(&vault, "accounts.update", json!({ "id": old, "includeInNetWorth": false }), INVALID);
    assert_eq!(worth(&vault), triple("100.00", "20.00", "120.00"));
}

#[test]
fn available_is_the_balance_plus_the_virtual_balance_whatever_the_signs() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    for (n, (opening, virtual_balance, available)) in [
        ("-240.00", "1000.00", "760.00"),
        ("-240.00", "0.00", "-240.00"),
        ("-240.00", "-10.00", "-250.00"),
        ("0.00", "1000.00", "1000.00"),
        ("0.00", "-1000.00", "-1000.00"),
        ("0.00", "0.00", "0.00"),
        ("100.00", "-150.00", "-50.00"),
        ("50.00", "-50.00", "0.00"),
        ("0.01", "-0.02", "-0.01"),
        ("-0.01", "0.01", "0.00"),
        ("-9999999999.99", "9999999999.99", "0.00"),
    ]
    .into_iter()
    .enumerate()
    {
        let made = ok(&vault, "accounts.create", json!({ "name": format!("A {n}"), "kind": "CREDIT_CARD", "openingBalance": opening, "openingDate": "2026-01-01", "virtualBalance": virtual_balance }));
        assert_eq!((made["balance"].as_str(), made["available"].as_str()), (Some(opening), Some(available)), "{opening} and {virtual_balance}");
        let id = id_of(&made);
        assert_eq!(find(&vault, &id)["available"], available);
        assert_eq!(ok(&vault, "accounts.show", json!({ "id": id }))["account"]["available"], available, "the statement says the same as the list");
        assert!(is_money(made["available"].as_str().unwrap()) && made["available"] != "-0.00", "{made}");
    }
    // available follows the balance, and the virtual balance stays what it was
    let card = id_of(&ok(&vault, "accounts.create", json!({ "name": "Card", "kind": "CREDIT_CARD", "openingBalance": "-240.00", "openingDate": "2026-01-01", "virtualBalance": "1000.00" })));
    spend(&vault, &food, "Shoes", "40.00", "2026-10-03", Some(&card));
    let after = find(&vault, &card);
    assert_eq!((after["balance"].as_str(), after["available"].as_str(), after["virtualBalance"].as_str()), (Some("-280.00"), Some("720.00"), Some("1000.00")));
    transfer(&vault, "1", &card, "100.00", "2026-10-04");
    assert_eq!((held(&vault, &card), find(&vault, &card)["available"].as_str().unwrap().to_string()), ("-180.00".to_string(), "820.00".to_string()));
    // a payment dated after today is in neither yet
    spend(&vault, &food, "Later", "10.00", "2026-10-20", Some(&card));
    assert_eq!((held(&vault, &card), find(&vault, &card)["available"].as_str().unwrap().to_string()), ("-180.00".to_string(), "820.00".to_string()));
    // changing the virtual balance moves available and nothing of the statement
    let statement = ok(&vault, "accounts.show", json!({ "id": card, "from": "2026-01-01", "to": "2026-12-31" }));
    ok(&vault, "accounts.update", json!({ "id": card, "virtualBalance": "500.00" }));
    let again = ok(&vault, "accounts.show", json!({ "id": card, "from": "2026-01-01", "to": "2026-12-31" }));
    assert_eq!((again["account"]["available"].as_str(), again["account"]["balance"].as_str()), (Some("320.00"), Some("-180.00")));
    assert_eq!((&again["lines"], &again["balanceBefore"], &again["balanceAfter"]), (&statement["lines"], &statement["balanceBefore"], &statement["balanceAfter"]), "the virtual balance is in no line and no balance of the statement");
    assert_eq!(ok(&vault, "accounts.history", json!({ "id": card, "year": "2026" })).as_array().unwrap().last().unwrap()["balance"], "-180.00", "nor in the chart");
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
    assert_eq!(ok(&vault, "accounts.summary", json!({})), json!({ "accounts": 1, "debts": "0.00", "friends": "0.00", "inAccounts": "0.00", "inUse": false, "netWorth": "0.00" }));
    // one account with an opening balance is already in use
    ok(&vault, "accounts.update", json!({ "id": "1", "openingBalance": "5.00", "openingDate": "2026-01-01" }));
    assert_eq!(ok(&vault, "accounts.summary", json!({})), json!({ "accounts": 1, "debts": "0.00", "friends": "0.00", "inAccounts": "5.00", "inUse": true, "netWorth": "5.00" }));
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
        json!({ "accounts": 3, "debts": "0.00", "friends": "20.00", "inAccounts": "-40.00", "inUse": true, "netWorth": "-20.00" }),
        "-20 left the main account, -50 on the card, +30 in the archived one; the friend owes 20"
    );
}

// ---- what an account paid and took in, by category ----------------------------------------------

/// The names of the categories of one side of a breakdown, and their amounts, in the order given.
fn shares(side: &Value) -> Vec<(String, String)> {
    side.as_array().unwrap().iter().map(|s| (s["name"].as_str().unwrap().to_string(), s["amount"].as_str().unwrap().to_string())).collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

fn total_of(side: &Value) -> i64 {
    side.as_array().unwrap().iter().map(|s| cents(s["amount"].as_str().unwrap())).sum()
}

/// An account that paid and took in over October, with everything that is not in a breakdown around it. Gives the account and the ids of the
/// categories `Art` and `Food`.
fn busy(vault: &Vault) -> (String, String, String) {
    let (food, rent, fun, art, zed) = (category(vault, "EXPENSE", "Food"), category(vault, "EXPENSE", "Rent"), category(vault, "EXPENSE", "Fun"), category(vault, "EXPENSE", "Art"), category(vault, "EXPENSE", "Zed"));
    let (pay, gift) = (category(vault, "INCOME", "Pay"), category(vault, "INCOME", "Gift"));
    let b = id_of(&ok(vault, "accounts.create", json!({ "name": "Savings", "kind": "SAVINGS", "openingBalance": "100.00", "openingDate": "2026-09-01" })));
    for (cat, name, amount, date) in [
        (&food, "Groceries", "30.00", "2026-10-02"),
        (&food, "More groceries", "20.00", "2026-10-20"),
        (&rent, "Rent", "400.00", "2026-10-05"),
        (&fun, "Cinema", "50.00", "2026-10-09"),
        (&art, "Paint", "50.00", "2026-10-15"),
        (&zed, "Zips", "50.00", "2026-10-15"),
        (&pay, "Salary", "500.00", "2026-10-01"),
        (&gift, "Birthday", "25.00", "2026-10-31"),
        (&pay, "September pay", "5.00", "2026-09-30"),
        (&food, "November food", "7.00", "2026-11-02"),
    ] {
        spend(vault, cat, name, amount, date, Some(&b));
    }
    // the main account has its own rows, and money moves between the two without being income or an expense
    spend(vault, &food, "Main food", "99.00", "2026-10-03", None);
    transfer(vault, "1", &b, "200.00", "2026-10-03");
    transfer(vault, &b, "1", "50.00", "2026-10-10");
    let anna = id_of(&ok(vault, "people.create", json!({ "name": "Anna" })));
    ok(vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "20.00", "date": "2026-10-12", "accountId": b }));
    // a bill a friend paid costs the account nothing; one it paid is in it whole, though only part of it is the person's own
    let dinner = id_of(&spend(vault, &food, "Dinner", "90.00", "2026-10-12", Some(&b)));
    ok(vault, "transactions.split", json!({ "id": dinner, "mode": "EQUAL", "people": [anna], "paidBy": anna }));
    let lunch = id_of(&spend(vault, &food, "Lunch", "60.00", "2026-10-13", Some(&b)));
    ok(vault, "transactions.split", json!({ "id": lunch, "mode": "EQUAL", "people": [anna] }));
    (b, art, food)
}

#[test]
fn a_breakdown_adds_up_what_an_account_paid_and_took_in_by_category_and_nothing_else() {
    let (_dir, vault) = fresh();
    let (b, _, _) = busy(&vault);
    let before = dump(&vault);
    let october = ok(&vault, "accounts.breakdown", json!({ "id": b, "from": "2026-10-01", "to": "2026-10-31" }));
    assert_eq!(keys(&october), ["currency", "expenseTotal", "expenses", "from", "income", "incomeTotal", "to"]);
    assert_eq!((october["from"].as_str(), october["to"].as_str()), (Some("2026-10-01"), Some("2026-10-31")));
    // the largest first, a tie by name; Food is the whole of the lunch the person paid (60.00) and not the dinner a friend paid
    assert_eq!(shares(&october["expenses"]), pairs(&[("Rent", "400.00"), ("Food", "110.00"), ("Art", "50.00"), ("Fun", "50.00"), ("Zed", "50.00")]));
    assert_eq!(shares(&october["income"]), pairs(&[("Pay", "500.00"), ("Gift", "25.00")]));
    assert_eq!((october["expenseTotal"].as_str(), october["incomeTotal"].as_str()), (Some("660.00"), Some("525.00")));
    // the totals are the sums of the shares, and ids and money are text
    assert_eq!((total_of(&october["expenses"]), total_of(&october["income"])), (cents("660.00"), cents("525.00")));
    for share in october["expenses"].as_array().unwrap().iter().chain(october["income"].as_array().unwrap()) {
        assert_eq!(keys(share), ["amount", "categoryId", "name"]);
        assert!(share["categoryId"].as_str().is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())), "{share}");
        assert!(is_money(share["amount"].as_str().unwrap()) && cents(share["amount"].as_str().unwrap()) > 0, "an amount is more than nothing, whatever the side: {share}");
    }
    // it agrees with the statement of the same days: the transactions in it are the income less the expenses
    let statement = ok(&vault, "accounts.show", json!({ "id": b, "from": "2026-10-01", "to": "2026-10-31" }));
    let moved: i64 = statement["lines"].as_array().unwrap().iter().filter(|l| l["source"] == "TRANSACTION").map(|l| cents(l["amount"].as_str().unwrap())).sum();
    assert_eq!(moved, cents("525.00") - cents("660.00"));
    assert_eq!(dump(&vault), before, "a report changes nothing");

    // both days are in it: the first and the last of the month, and a single day
    assert_eq!(shares(&ok(&vault, "accounts.breakdown", json!({ "id": b, "from": "2026-10-01", "to": "2026-10-01" }))["income"]), pairs(&[("Pay", "500.00")]));
    let last = ok(&vault, "accounts.breakdown", json!({ "id": b, "from": "2026-10-31", "to": "2026-10-31" }));
    assert_eq!((shares(&last["income"]), last["expenses"].clone(), last["expenseTotal"].as_str(), last["incomeTotal"].as_str()), (pairs(&[("Gift", "25.00")]), json!([]), Some("0.00"), Some("25.00")));
    let middle = ok(&vault, "accounts.breakdown", json!({ "id": b, "from": "2026-10-05", "to": "2026-10-09" }));
    assert_eq!((shares(&middle["expenses"]), middle["income"].clone(), middle["expenseTotal"].as_str(), middle["incomeTotal"].as_str()), (pairs(&[("Rent", "400.00"), ("Fun", "50.00")]), json!([]), Some("450.00"), Some("0.00")));
    // days that are in the future count: it is a report of the days asked for, not of today
    let november = ok(&vault, "accounts.breakdown", json!({ "id": b, "from": "2026-11-01", "to": "2026-11-30" }));
    assert_eq!(shares(&november["expenses"]), pairs(&[("Food", "7.00")]));
    // the rows of other accounts are not in it, and a day with nothing is an empty report
    let main = ok(&vault, "accounts.breakdown", json!({ "id": "1", "from": "2026-10-01", "to": "2026-10-31" }));
    assert_eq!((shares(&main["expenses"]), main["income"].clone()), (pairs(&[("Food", "99.00")]), json!([])), "the transfers and the loan are not in it");
    for (from, to) in [("2026-10-04", "2026-10-04"), ("2026-08-01", "2026-08-31"), ("2030-01-01", "2030-12-31")] {
        let empty = ok(&vault, "accounts.breakdown", json!({ "id": b, "from": from, "to": to }));
        assert_eq!((empty["expenses"].clone(), empty["income"].clone(), empty["expenseTotal"].as_str(), empty["incomeTotal"].as_str(), empty["from"].as_str(), empty["to"].as_str()), (json!([]), json!([]), Some("0.00"), Some("0.00"), Some(from), Some(to)), "{from} {to}");
    }
    // days that reach over the end of a month
    let across = ok(&vault, "accounts.breakdown", json!({ "id": b, "from": "2026-10-31", "to": "2026-11-02" }));
    assert_eq!((shares(&across["income"]), shares(&across["expenses"])), (pairs(&[("Gift", "25.00")]), pairs(&[("Food", "7.00")])));
    // an account with nothing in it
    let empty = account(&vault, "Nothing");
    let none = ok(&vault, "accounts.breakdown", json!({ "id": empty }));
    assert_eq!((none["expenses"].clone(), none["income"].clone(), none["expenseTotal"].as_str(), none["incomeTotal"].as_str()), (json!([]), json!([]), Some("0.00"), Some("0.00")));
    // an account with only transfers and a loan has an empty breakdown too
    let moved_only = account(&vault, "Only moved");
    transfer(&vault, "1", &moved_only, "5.00", "2026-10-06");
    let anna = id_of(&ok(&vault, "people.list", json!({}))[0]);
    ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "1.00", "date": "2026-10-06", "accountId": moved_only }));
    let only = ok(&vault, "accounts.breakdown", json!({ "id": moved_only, "from": "2026-10-01", "to": "2026-10-31" }));
    assert_eq!((only["expenses"].clone(), only["income"].clone(), only["expenseTotal"].as_str()), (json!([]), json!([]), Some("0.00")));
}

#[test]
fn a_breakdown_is_allowed_for_an_archived_account_and_keeps_the_name_of_a_category_that_is_gone() {
    let (_dir, vault) = fresh();
    let (b, art, _) = busy(&vault);
    let period = json!({ "id": b, "from": "2026-10-01", "to": "2026-10-31" });
    let live = ok(&vault, "accounts.breakdown", period.clone());
    ok(&vault, "accounts.archive", json!({ "id": b }));
    assert_eq!(ok(&vault, "accounts.breakdown", period.clone()), live, "an archived account is reported on as it was");
    // the category is deleted: its transactions keep it, and so does the report
    ok(&vault, "categories.delete", json!({ "id": art }));
    assert_eq!(ok(&vault, "accounts.breakdown", period.clone()), live, "a category that was deleted since keeps its name");
    // a transaction moved to another account leaves this one's report and joins the other's
    let paint = ok(&vault, "transactions.list", json!({ "q": "Paint" }))["transactions"][0]["id"].as_str().unwrap().to_string();
    ok(&vault, "accounts.unarchive", json!({ "id": b }));
    ok(&vault, "transactions.update", json!({ "id": paint, "accountId": "1" }));
    let after = ok(&vault, "accounts.breakdown", period);
    assert_eq!((shares(&after["expenses"]), after["expenseTotal"].as_str()), (pairs(&[("Rent", "400.00"), ("Food", "110.00"), ("Fun", "50.00"), ("Zed", "50.00")]), Some("610.00")));
    assert_eq!(shares(&ok(&vault, "accounts.breakdown", json!({ "id": "1", "from": "2026-10-01", "to": "2026-10-31" }))["expenses"]), pairs(&[("Food", "99.00"), ("Art", "50.00")]));
}

#[test]
fn a_breakdown_without_days_is_for_this_month_in_the_apps_time_zone_as_a_statement_is() {
    let (_dir, vault) = fresh();
    let days = |name: &str, now: &str| {
        let answer = run_at(&vault, name, json!({ "id": "1" }), at(now)).unwrap();
        (answer["from"].as_str().unwrap().to_string(), answer["to"].as_str().unwrap().to_string())
    };
    for now in [
        "2026-10-14T12:00:00Z",
        "2026-12-15T12:00:00Z",
        "2027-02-10T12:00:00Z",
        "2028-02-10T12:00:00Z",
        "2026-04-30T22:30:00Z",
        "2026-04-30T23:30:00Z",
        "2026-01-01T00:00:00Z",
        "2026-12-31T23:59:59Z",
    ] {
        assert_eq!(days("accounts.breakdown", now), days("accounts.show", now), "{now}");
    }
    assert_eq!(days("accounts.breakdown", "2028-02-10T12:00:00Z"), ("2028-02-01".into(), "2028-02-29".into()), "a leap year");
    assert_eq!(days("accounts.breakdown", "2026-04-30T23:30:00Z"), ("2026-05-01".into(), "2026-05-31".into()), "half an hour into May in Dublin");
    let mut settings = vault.settings().unwrap();
    settings.timezone = "Pacific/Auckland".into();
    vault.save_settings(&settings).unwrap();
    assert_eq!(days("accounts.breakdown", "2026-10-31T20:00:00Z"), ("2026-11-01".into(), "2026-11-30".into()), "the zone set in the app, not the computer's");
    // each end given alone: the other is this month's
    let one = |body: Value| {
        let answer = run_at(&vault, "accounts.breakdown", body, at("2026-10-14T12:00:00Z")).unwrap();
        (answer["from"].as_str().unwrap().to_string(), answer["to"].as_str().unwrap().to_string())
    };
    assert_eq!(one(json!({ "id": "1", "from": "2026-10-12" })), ("2026-10-12".into(), "2026-10-31".into()));
    assert_eq!(one(json!({ "id": "1", "to": "2026-10-03" })), ("2026-10-01".into(), "2026-10-03".into()));
    // and what is reported is this month's: October's rows at noon on the 14th, November's at noon on the 15th of November
    let food = category(&vault, "EXPENSE", "Food");
    spend(&vault, &food, "October", "3.00", "2026-10-30", None);
    spend(&vault, &food, "November", "4.00", "2026-11-03", None);
    let expenses = |now: &str| shares(&run_at(&vault, "accounts.breakdown", json!({ "id": "1" }), at(now)).unwrap()["expenses"]);
    assert_eq!(expenses("2026-10-14T12:00:00Z"), pairs(&[("Food", "3.00")]), "the 30th is later than today and still in this month");
    assert_eq!(expenses("2026-11-15T12:00:00Z"), pairs(&[("Food", "4.00")]));
}

#[test]
fn a_breakdown_refuses_days_that_are_not_days_or_the_wrong_way_round_and_an_account_that_is_not_there() {
    let (_dir, vault) = fresh();
    let w = world(&vault);
    for body in [
        json!({ "id": w.a, "from": "2026-10-31", "to": "2026-10-01" }),
        json!({ "id": w.a, "from": "2026-10-11", "to": "2026-10-10" }),
        json!({ "id": w.a, "from": "2026-11-01" }),
        json!({ "id": w.a, "to": "2026-09-30" }),
        json!({ "id": w.a, "from": "2026-02-30" }),
        json!({ "id": w.a, "to": "2026-13-01" }),
        json!({ "id": w.a, "from": "2026-10-01", "to": "2026-02-30" }),
        json!({ "id": w.a, "from": "2026-00-10", "to": "2026-10-31" }),
        json!({ "id": w.a, "from": "0000-00-00", "to": "2026-10-31" }),
    ] {
        assert_eq!(refuse(&vault, "accounts.breakdown", body.clone(), INVALID), "invalid input", "{body}");
    }
    // a value that is not shaped like a date never gets as far as the days
    for value in [json!("2026-10-1"), json!("tomorrow"), json!("2026-10-14T00:00:00Z"), json!(""), json!("２０２６-10-14"), json!(20261014), json!(true), json!(null)] {
        for key in ["from", "to"] {
            assert_eq!(refuse(&vault, "accounts.breakdown", with(json!({ "id": w.a }), key, value.clone()), INVALID), format!("`{key}` must be a date like 2026-10-14"), "{key} {value}");
        }
    }
    // the account: needed, shaped like an id, and there
    assert_eq!(refuse(&vault, "accounts.breakdown", json!({}), INVALID), "missing argument `id`");
    assert_eq!(refuse(&vault, "accounts.breakdown", json!({ "from": "2026-10-01", "to": "2026-10-31" }), INVALID), "missing argument `id`");
    for id in [json!("0"), json!("01"), json!("-1"), json!("abc"), json!(""), json!(" 1"), json!("1.5"), json!("９"), json!(1), json!(null), json!(["1"]), json!("9".repeat(19))] {
        assert_eq!(refuse(&vault, "accounts.breakdown", json!({ "id": id }), INVALID), "`id` must be an id as the app lists it", "{id}");
    }
    let gone = account(&vault, "Soon gone");
    ok(&vault, "accounts.delete", json!({ "id": gone }));
    for id in ["999999", "999999999999999999", gone.as_str()] {
        assert_eq!(refuse(&vault, "accounts.breakdown", json!({ "id": id }), NOT_FOUND), "not found");
        assert_eq!(refuse(&vault, "accounts.breakdown", json!({ "id": id, "from": "2026-10-01", "to": "2026-10-31" }), NOT_FOUND), "not found");
    }
    // the argument of another route is not one of this
    for extra in ["year", "accountId", "month", "kind"] {
        assert_eq!(refuse(&vault, "accounts.breakdown", with(json!({ "id": w.a }), extra, json!("2026")), INVALID), format!("unknown argument `{extra}`"));
    }
}

// ---- the kind of a transaction in a list ---------------------------------------------------------

fn names(page: &Value) -> Vec<String> {
    page["transactions"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect()
}

fn text(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn transactions_are_listed_by_kind_alone_and_with_the_other_filters_and_a_page_counts_all_that_match() {
    let (_dir, vault) = fresh();
    let (food, pay) = (category(&vault, "EXPENSE", "Food"), category(&vault, "INCOME", "Pay"));
    let b = account(&vault, "Savings");
    spend(&vault, &pay, "Salary", "1000.00", "2026-10-01", None);
    spend(&vault, &pay, "Bonus", "200.00", "2026-10-10", Some(&b));
    spend(&vault, &pay, "Refund", "20.00", "2026-09-15", Some(&b));
    spend(&vault, &food, "Bread", "2.00", "2026-10-02", None);
    spend(&vault, &food, "Boots", "40.00", "2026-10-03", Some(&b));
    spend(&vault, &food, "Coffee", "3.00", "2026-10-10", Some(&b));
    let list = |body: Value| ok(&vault, "transactions.list", body);
    let all = list(json!({}));
    assert_eq!(all["total"], 6);
    let (income, expense) = (list(json!({ "kind": "INCOME" })), list(json!({ "kind": "EXPENSE" })));
    assert_eq!((names(&income), income["total"].as_u64()), (text(&["Bonus", "Salary", "Refund"]), Some(3)), "newest first");
    assert_eq!((names(&expense), expense["total"].as_u64()), (text(&["Coffee", "Boots", "Bread"]), Some(3)));
    assert_eq!(income["total"].as_u64().unwrap() + expense["total"].as_u64().unwrap(), all["total"].as_u64().unwrap(), "every transaction is one or the other");
    assert!(income["transactions"].as_array().unwrap().iter().all(|t| t["categoryId"] == pay.as_str()));
    assert!(expense["transactions"].as_array().unwrap().iter().all(|t| t["categoryId"] == food.as_str()));
    // with the other filters
    assert_eq!(names(&list(json!({ "kind": "INCOME", "accountId": b }))), text(&["Bonus", "Refund"]));
    assert_eq!(names(&list(json!({ "kind": "EXPENSE", "accountId": b }))), text(&["Coffee", "Boots"]));
    assert_eq!(names(&list(json!({ "kind": "INCOME", "accountId": "1" }))), text(&["Salary"]));
    assert_eq!(names(&list(json!({ "kind": "INCOME", "from": "2026-10-01", "to": "2026-10-31" }))), text(&["Bonus", "Salary"]));
    assert_eq!((names(&list(json!({ "kind": "INCOME", "from": "2026-10-10", "to": "2026-10-10" }))), names(&list(json!({ "kind": "EXPENSE", "from": "2026-10-10", "to": "2026-10-10" })))), (text(&["Bonus"]), text(&["Coffee"])), "the days are inclusive");
    assert_eq!(names(&list(json!({ "kind": "INCOME", "q": "BO" }))), text(&["Bonus"]), "the search and the kind both hold");
    assert_eq!(names(&list(json!({ "kind": "EXPENSE", "q": "bo" }))), text(&["Boots"]));
    assert_eq!(list(json!({ "kind": "INCOME", "categoryId": food }))["total"], 0, "a category of the other kind has none of this one");
    assert_eq!(names(&list(json!({ "kind": "EXPENSE", "categoryId": food, "accountId": b, "from": "2026-10-03", "q": "o" }))), text(&["Coffee", "Boots"]), "every filter at once");
    // pages: the total is all that match, whatever the page
    let page = list(json!({ "kind": "INCOME", "limit": 1, "offset": 1 }));
    assert_eq!((names(&page), page["total"].as_u64(), page["limit"].as_u64(), page["offset"].as_u64()), (text(&["Salary"]), Some(3), Some(1), Some(1)));
    let page = list(json!({ "kind": "EXPENSE", "limit": 2, "offset": 2 }));
    assert_eq!((names(&page), page["total"].as_u64()), (text(&["Bread"]), Some(3)));
    let past = list(json!({ "kind": "EXPENSE", "limit": 2, "offset": 3 }));
    assert_eq!((names(&past), past["total"].as_u64()), (Vec::<String>::new(), Some(3)), "past the end: nothing, and the same total");
    let one_account = list(json!({ "kind": "EXPENSE", "accountId": b, "limit": 1 }));
    assert_eq!((names(&one_account), one_account["total"].as_u64()), (text(&["Coffee"]), Some(2)));
    // a transfer is neither, and a category that is deleted keeps the kind of its rows
    transfer(&vault, "1", &b, "10.00", "2026-10-05");
    assert_eq!((list(json!({ "kind": "INCOME" }))["total"].as_u64(), list(json!({ "kind": "EXPENSE" }))["total"].as_u64(), list(json!({}))["total"].as_u64()), (Some(3), Some(3), Some(6)));
    ok(&vault, "categories.delete", json!({ "id": food }));
    assert_eq!(names(&list(json!({ "kind": "EXPENSE" }))), text(&["Coffee", "Boots", "Bread"]));
    assert_eq!(names(&list(json!({ "kind": "INCOME" }))), text(&["Bonus", "Salary", "Refund"]));
    // a kind with nothing in it is a page of none
    let (_other, fresh_vault) = fresh();
    let nothing = ok(&fresh_vault, "transactions.list", json!({ "kind": "INCOME" }));
    assert_eq!((nothing["total"].as_u64(), nothing["transactions"].as_array().unwrap().len()), (Some(0), 0));
}

#[test]
fn the_kind_of_a_list_is_income_or_expense_in_capitals_as_the_type_of_a_category_is_and_nothing_else() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    spend(&vault, &food, "Bread", "2.00", "2026-10-01", None);
    // The route lists its two words as `categories.list` lists them: the command line matches them without regard to case (see the tests of
    // the command line), and over HTTP they are exact.
    for kind in ["income", "expense", "Income", "Expense", "iNcOmE", "INCOME ", " EXPENSE", "", "BOTH", "ALL", "TRANSFER", "INCOME,EXPENSE", "EXPENSES", "1", "ÉXPENSE"] {
        assert_eq!(refuse(&vault, "transactions.list", json!({ "kind": kind }), INVALID), "`kind` must be one of: INCOME, EXPENSE", "{kind:?}");
    }
    for value in [json!(1), json!(true), json!(null), json!(["INCOME"]), json!({ "kind": "INCOME" })] {
        assert_eq!(refuse(&vault, "transactions.list", json!({ "kind": value }), INVALID), "`kind` must be a string", "{value}");
    }
    // the same two words and the same refusal as the type of a category
    assert_eq!(refuse(&vault, "categories.list", json!({ "type": "income" }), INVALID), "`type` must be one of: INCOME, EXPENSE");
    let words: Vec<&str> = route("transactions.list").params.iter().find(|p| p.name == "kind").unwrap().values.to_vec();
    assert_eq!(words, ["INCOME", "EXPENSE"]);
    assert_eq!(ok(&vault, "transactions.list", json!({ "kind": "EXPENSE" }))["total"], 1);
    assert_eq!(ok(&vault, "transactions.list", json!({ "kind": "INCOME" }))["total"], 0);
    // a kind is not an argument of the routes that change a row
    for (name, body) in [("transactions.update", json!({ "id": "1", "kind": "INCOME" })), ("transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": food, "kind": "INCOME" })), ("transfers.list", json!({ "kind": "INCOME" }))] {
        assert_eq!(refuse(&vault, name, body, INVALID), "unknown argument `kind`", "{name}");
    }
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
    let total: u64 = ["1", "2", "3", "4", "5", "6"].iter().map(|id| ok(&vault, "transactions.list", json!({ "accountId": id }))["total"].as_u64().unwrap()).sum();
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
        // `categoryId` is not an argument of the routes of accounts, except of the payment of a liability, which books its interest in one
        for extra in ["extra", "accountID", "account-id", "Id", "kind ", "categoryId"].into_iter().filter(|extra| !route(name).params.iter().any(|p| p.name == *extra)) {
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
    // each new amount belongs to one kind of row: a transaction has a native amount, a transfer an amount that arrives, a new account no balance
    for (name, body, unknown) in [
        ("transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": "1", "toAmount": "1.00" }), "toAmount"),
        ("transactions.update", json!({ "id": "1", "toAmount": "1.00" }), "toAmount"),
        ("transfers.create", json!({ "fromAccountId": w.a, "toAccountId": w.b, "amount": "1.00", "date": "2026-10-01", "nativeAmount": "1.00" }), "nativeAmount"),
        ("transfers.update", json!({ "id": w.transfer, "nativeAmount": "1.00" }), "nativeAmount"),
        ("recurring.create", json!({ "name": "X", "categoryId": "1", "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1, "nativeAmount": "1.00" }), "nativeAmount"),
        ("recurring.update", json!({ "id": "1", "nativeAmount": "1.00" }), "nativeAmount"),
        ("ledger.add", json!({ "personId": "1", "kind": "LENT", "amount": "1.00", "date": "2026-10-14", "nativeAmount": "1.00" }), "nativeAmount"),
        ("accounts.create", json!({ "name": "New", "kind": "CASH", "balance": "1.00" }), "balance"),
        ("accounts.create", json!({ "name": "New", "kind": "CASH", "nativeAmount": "1.00" }), "nativeAmount"),
    ] {
        assert_eq!(refuse(&vault, name, body, INVALID), format!("unknown argument `{unknown}`"), "{name}");
    }
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
    assert_eq!(tried, 20, "every required argument of every route");
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
        Kind::Boolean => vec![json!("true"), json!("false"), json!(1), json!(0), json!("yes"), json!(""), json!([true]), json!({ "a": true }), json!(1.5)],
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
    // and for the second amount of a transaction, which is money as the first is
    for value in wrong(Kind::Money, false) {
        for (name, body) in [
            ("transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": food, "accountId": w.a })),
            ("transactions.update", json!({ "id": tx })),
        ] {
            let mut bad = body.clone();
            bad["nativeAmount"] = value.clone();
            let message = refuse(&vault, name, bad, INVALID);
            assert!(message.starts_with("`nativeAmount` must be "), "{name} {value}: {message}");
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
        ("accounts.create", json!({ "name": "Long", "kind": "CASH", "bic": "x".repeat(21) }), INVALID),
        ("accounts.create", json!({ "name": "Long", "kind": "CASH", "notes": "x".repeat(1001) }), INVALID),
        ("accounts.create", json!({ "name": "Long", "kind": "CASH", "group": "x".repeat(51) }), INVALID),
        ("accounts.create", json!({ "name": "Big", "kind": "CASH", "virtualBalance": "10000000000.00" }), INVALID),
        ("accounts.update", json!({ "id": w.b, "virtualBalance": "-10000000000.00" }), INVALID),
        ("accounts.update", json!({ "id": w.b, "bic": "a\nb" }), INVALID),
        ("accounts.update", json!({ "id": w.archived, "includeInNetWorth": false }), INVALID),
        ("accounts.breakdown", json!({ "id": w.a, "from": "2026-10-31", "to": "2026-10-01" }), INVALID),
        ("accounts.breakdown", json!({ "id": w.a, "from": "2026-02-30" }), INVALID),
        ("accounts.breakdown", json!({ "id": "999999" }), NOT_FOUND),
        ("transactions.list", json!({ "kind": "BOTH" }), INVALID),
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
    let reads: [(&str, &str); 6] = [
        ("accounts.list", "{}"),
        ("accounts.show", r#"{"id":"2"}"#),
        ("accounts.history", r#"{"id":"2","year":"2026"}"#),
        ("accounts.breakdown", r#"{"id":"2","from":"2026-09-01","to":"2026-09-30"}"#),
        ("accounts.summary", "{}"),
        ("transfers.list", r#"{"limit":5}"#),
    ];
    for (name, body) in reads {
        let reply = call(port, Some(&h.read), name, body);
        assert_eq!(reply.status, 200, "{name}: {}", reply.body);
        assert_eq!(reply.json()["ok"], true, "{name}");
        assert!(!reply.body.contains(&h.read) && !reply.body.contains(&h.write), "{name}");
    }
    assert_eq!(call(port, Some(&h.read), "accounts.list", "{}").data().as_array().unwrap().len(), 11);

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
    assert_eq!(tried, 10);
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
fn over_http_a_write_token_sets_the_details_a_read_token_reads_them_and_the_kind_of_a_list_is_exact() {
    let h = http();
    let port = h.server.port();
    let post = |token: &str, route: &str, body: Value| {
        let reply = call(port, Some(token), route, &body.to_string());
        assert_eq!(reply.status, 200, "{route}: {}", reply.body);
        assert_eq!(reply.json()["ok"], true);
        reply.data()
    };
    let details = json!({ "name": "Shop card", "kind": "CREDIT_CARD", "openingBalance": "-240.00", "openingDate": "2026-01-01", "bic": "DEUTDEFF", "notes": "Limit 1000", "group": "Cards", "includeInNetWorth": false, "virtualBalance": "1000.00" });
    let made = post(&h.write, "accounts.create", details.clone());
    assert_eq!(keys(&made), ACCOUNT_KEYS);
    assert_eq!(
        (made["bic"].as_str(), made["notes"].as_str(), made["group"].as_str(), made["includeInNetWorth"].as_bool(), made["virtualBalance"].as_str(), made["available"].as_str()),
        (Some("DEUTDEFF"), Some("Limit 1000"), Some("Cards"), Some(false), Some("1000.00"), Some("760.00"))
    );
    let id = id_of(&made);
    // the read token sees the same details, in the list and in the statement
    let list = post(&h.read, "accounts.list", json!({}));
    let seen = list.as_array().unwrap().iter().find(|a| a["id"] == id.as_str()).unwrap().clone();
    for key in DETAILS.iter().chain(["available", "balance"].iter()) {
        assert_eq!(seen[*key], made[*key], "{key}");
    }
    assert_eq!(post(&h.read, "accounts.show", json!({ "id": id }))["account"]["group"], "Cards");

    // a read token cannot set or change one, and nothing changes
    let before = dump(&h.vault);
    for (route, body) in [
        ("accounts.create", with(details.clone(), "name", json!("Another"))),
        ("accounts.update", json!({ "id": id, "virtualBalance": "5.00" })),
        ("accounts.update", json!({ "id": id, "includeInNetWorth": true })),
        ("accounts.update", json!({ "id": id, "bic": null })),
        ("accounts.update", json!({ "id": id, "includeInNetWorth": "yes" })),
    ] {
        let refused = call(port, Some(&h.read), route, &body.to_string());
        assert_eq!((refused.status, refused.code().as_str()), (403, "forbidden"), "{route} {body}");
    }
    assert_eq!(dump(&h.vault), before);
    // a null on the wire clears, and a wrong kind of value is a 400 with the reason
    let cleared = post(&h.write, "accounts.update", json!({ "id": id, "bic": null, "notes": null, "group": null }));
    assert_eq!((cleared["bic"].clone(), cleared["notes"].clone(), cleared["group"].clone(), cleared["virtualBalance"].as_str()), (Value::Null, Value::Null, Value::Null, Some("1000.00")));
    for (body, message) in [
        (format!(r#"{{"id":"{id}","includeInNetWorth":"false"}}"#), "`includeInNetWorth` must be a boolean"),
        (format!(r#"{{"id":"{id}","includeInNetWorth":null}}"#), "`includeInNetWorth` must be a boolean"),
        (format!(r#"{{"id":"{id}","virtualBalance":5}}"#), "`virtualBalance` must be a string"),
        (format!(r#"{{"id":"{id}","virtualBalance":"1.5"}}"#), "invalid input"),
        (format!(r#"{{"id":"{id}","bic":"{}"}}"#, "x".repeat(21)), "invalid input"),
    ] {
        let reply = call(port, Some(&h.write), "accounts.update", &body);
        assert_eq!((reply.status, reply.code().as_str(), reply.message().as_str()), (400, "invalid_input", message), "{body}");
    }

    // net worth follows the switch: the account's balance goes out of it and comes back
    let summary = |token: &str| post(token, "accounts.summary", json!({}))["inAccounts"].as_str().unwrap().to_string();
    let savings = post(&h.read, "accounts.list", json!({})).as_array().unwrap().iter().find(|a| a["id"] == "2").unwrap().clone();
    let was = summary(&h.read);
    let counted = savings["includeInNetWorth"].as_bool().unwrap();
    post(&h.write, "accounts.update", json!({ "id": "2", "includeInNetWorth": !counted }));
    let moved = cents(&was) + if counted { -cents(savings["balance"].as_str().unwrap()) } else { cents(savings["balance"].as_str().unwrap()) };
    assert_eq!(cents(&summary(&h.read)), moved);
    post(&h.write, "accounts.update", json!({ "id": "2", "includeInNetWorth": counted }));
    assert_eq!(summary(&h.read), was);

    // a breakdown is a read: with a token that can only read
    let report = post(&h.read, "accounts.breakdown", json!({ "id": "1", "from": "2025-01-01", "to": "2026-12-31" }));
    assert_eq!(keys(&report), ["currency", "expenseTotal", "expenses", "from", "income", "incomeTotal", "to"]);
    assert_eq!((total_of(&report["expenses"]), total_of(&report["income"])), (cents(report["expenseTotal"].as_str().unwrap()), cents(report["incomeTotal"].as_str().unwrap())));
    assert!(report["expenses"].as_array().unwrap().iter().all(|s| s["categoryId"].is_string() && s["amount"].is_string()));
    assert_eq!(post(&h.read, "accounts.breakdown", json!({ "id": "1" }))["from"], post(&h.read, "accounts.show", json!({ "id": "1" }))["from"], "the same days as a statement without days");
    let reversed = call(port, Some(&h.read), "accounts.breakdown", r#"{"id":"1","from":"2026-12-31","to":"2026-01-01"}"#);
    assert_eq!((reversed.status, reversed.code().as_str()), (400, "invalid_input"));
    assert_eq!(call(port, Some(&h.read), "accounts.breakdown", r#"{"id":"999999"}"#).status, 404);
    assert_eq!(call(port, Some(&h.read), "accounts.breakdown", "{}").message(), "missing argument `id`");
    assert_eq!(call(port, None, "accounts.breakdown", r#"{"id":"1"}"#).code(), "unauthorized");

    // the kind of a transaction list is exact over HTTP: the two words in capitals
    let all = post(&h.read, "transactions.list", json!({ "limit": 1 }))["total"].as_u64().unwrap();
    let (income, expense) = (post(&h.read, "transactions.list", json!({ "kind": "INCOME", "limit": 1 })), post(&h.read, "transactions.list", json!({ "kind": "EXPENSE", "limit": 1 })));
    assert_eq!(income["total"].as_u64().unwrap() + expense["total"].as_u64().unwrap(), all);
    assert!(income["total"].as_u64().unwrap() > 0 && expense["total"].as_u64().unwrap() > 0, "the demo data has both");
    for kind in ["income", "expense", "Income", "Expense", "", "BOTH"] {
        let reply = call(port, Some(&h.read), "transactions.list", &json!({ "kind": kind }).to_string());
        assert_eq!((reply.status, reply.code().as_str(), reply.message().as_str()), (400, "invalid_input", "`kind` must be one of: INCOME, EXPENSE"), "{kind:?}");
    }
    assert_eq!(h.server.panics(), 0);
}

#[test]
fn the_person_is_told_that_a_file_holds_accounts_and_transfers() {
    let r = rig(true);
    let json = r.path("all.json");
    assert_eq!(r.post("export.json", json!({ "out": json })).status, 200);
    assert!(r.last().contains("11 accounts by name, type, number, balance and details (BIC, group, notes, virtual balance, net worth, currency and rate), and ") && r.last().contains(" transfers between them"), "{}", r.last());
    let file: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(file["accounts"].as_array().unwrap().len(), 11, "what the person was told is what the file holds");
    assert!(file["transfers"].as_array().unwrap().len() >= 16);
    // a CSV is only the transactions, and says nothing of accounts
    assert_eq!(r.post("export.csv", json!({ "out": r.path("t.csv") })).status, 200);
    assert!(!r.last().contains("accounts"), "{}", r.last());
}

// ---- a currency per account ---------------------------------------------------------------------
//
// Section 21 of the design note. The app's currency is EUR here, and "1 EUR = 5.00 RON" is the rate "5.00". What a row does to an account
// in lei is in lei (`nativeAmount` of a transaction, `toAmount` of a transfer); `amount` of a transaction is always in the app's currency.

/// An account in lei, five to the euro, opened on 2026-01-01.
fn lei(vault: &Vault, name: &str, opening: &str) -> String {
    id_of(&ok(vault, "accounts.create", json!({ "name": name, "kind": "CHECKING", "currency": "RON", "rate": "5.00", "openingBalance": opening, "openingDate": "2026-01-01" })))
}

/// A transaction in an account of another currency: `amount` in the app's currency, `native` in the account's.
fn spend_native(vault: &Vault, category: &str, name: &str, amount: &str, native: &str, date: &str, account: &str) -> Value {
    ok(vault, "transactions.create", json!({ "name": name, "amount": amount, "paymentDate": date, "categoryId": category, "accountId": account, "nativeAmount": native }))
}

/// A transfer between accounts of two currencies: `amount` leaves the first, `arrives` reaches the second.
fn transfer_across(vault: &Vault, from: &str, to: &str, amount: &str, arrives: &str, date: &str) -> Value {
    ok(vault, "transfers.create", json!({ "fromAccountId": from, "toAccountId": to, "amount": amount, "toAmount": arrives, "date": date }))
}

/// What an account says of its currency: the code, whether it is foreign, the rate, and what it is worth in the app's currency.
fn currency_of(account: &Value) -> (&str, bool, Option<&str>, &str) {
    (account["currency"].as_str().unwrap(), account["foreign"].as_bool().unwrap(), account["rate"].as_str(), account["balanceInMain"].as_str().unwrap())
}

/// The row of `transactions.list` with this id.
fn row(vault: &Vault, id: &str) -> Value {
    ok(vault, "transactions.list", json!({ "limit": 100 }))["transactions"].as_array().unwrap().iter().find(|t| t["id"] == id).unwrap_or_else(|| panic!("no transaction {id}")).clone()
}

#[test]
fn an_account_in_another_currency_is_made_with_a_rate_and_says_what_it_is_worth_in_the_apps_currency() {
    let (_dir, vault) = fresh();
    let made = ok(&vault, "accounts.create", json!({ "name": "Lei account", "kind": "CHECKING", "currency": "RON", "rate": "5.00", "openingBalance": "1000.00", "openingDate": "2026-01-01", "virtualBalance": "500.00" }));
    assert_eq!(keys(&made), ACCOUNT_KEYS);
    assert_eq!(currency_of(&made), ("RON", true, Some("5.00"), "200.00"));
    // everything the account holds is in lei, what is available too; only balanceInMain is in euro, and the virtual balance is never in it
    assert_eq!((made["openingBalance"].as_str(), made["balance"].as_str(), made["virtualBalance"].as_str(), made["available"].as_str()), (Some("1000.00"), Some("1000.00"), Some("500.00"), Some("1500.00")));
    assert_eq!(find(&vault, &id_of(&made)), made, "the list says what the answer said");
    // the main account and any account made without a currency are in the app's currency, with no rate, and worth what they hold
    let main = find(&vault, "1");
    assert_eq!(currency_of(&main), ("EUR", false, None, main["balance"].as_str().unwrap()));
    let plain = account(&vault, "Wallet");
    assert_eq!(currency_of(&find(&vault, &plain)), ("EUR", false, None, "0.00"));
    // the statement and the breakdown of each say the currency of their amounts
    let (lei_statement, lei_breakdown) = (ok(&vault, "accounts.show", json!({ "id": id_of(&made) })), ok(&vault, "accounts.breakdown", json!({ "id": id_of(&made) })));
    assert_eq!((lei_statement["currency"].as_str(), lei_breakdown["currency"].as_str()), (Some("RON"), Some("RON")));
    assert_eq!(lei_statement["account"], made);
    assert_eq!((ok(&vault, "accounts.show", json!({ "id": plain }))["currency"].as_str(), ok(&vault, "accounts.breakdown", json!({ "id": plain }))["currency"].as_str()), (Some("EUR"), Some("EUR")));
}

#[test]
fn a_currency_is_three_letters_in_any_case_stored_in_capitals_and_the_apps_own_is_no_other_currency() {
    let (_dir, vault) = fresh();
    for (n, code) in ["ron", "Ron", "rOn", "RON"].iter().enumerate() {
        let made = ok(&vault, "accounts.create", json!({ "name": format!("Lei {n}"), "kind": "CASH", "currency": code, "rate": "5.00" }));
        assert_eq!(currency_of(&made), ("RON", true, Some("5.00"), "0.00"), "{code}");
    }
    // the app's own currency, in any case, is not another one: the account has no rate, and takes none
    for (n, code) in ["eur", "Eur", "EUR"].iter().enumerate() {
        let made = ok(&vault, "accounts.create", json!({ "name": format!("Euro {n}"), "kind": "CASH", "currency": code }));
        assert_eq!((currency_of(&made), made["rate"].clone()), (("EUR", false, None, "0.00"), Value::Null), "{code}");
        refuse(&vault, "accounts.create", json!({ "name": "Euro rate", "kind": "CASH", "currency": code, "rate": "1.00" }), INVALID);
        refuse(&vault, "accounts.create", json!({ "name": "Euro rate", "kind": "CASH", "currency": code, "rate": "5.00" }), INVALID);
    }
    // not a code: too short, too long, a digit, a space, a sign, letters that are not capitals once the case is folded, and three bytes that are one character
    for code in ["RO", "ROMN", "RO1", "R0N", "", " ", "1", "123", "R N", "RO-", "ÄÖ", "ÄÖÜ", "€", "日", "RÖN", "RON1", "R.N"] {
        refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CASH", "currency": code, "rate": "5.00" }), INVALID);
    }
    // another currency needs a rate, and a rate needs another currency
    refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CASH", "currency": "RON" }), INVALID);
    refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CASH", "rate": "5.00" }), INVALID);
    for value in [json!(5), json!(true), json!(null), json!(["RON"]), json!({ "a": 1 }), json!(1.5)] {
        assert_eq!(refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CASH", "currency": value, "rate": "5.00" }), INVALID), "`currency` must be a string", "{value}");
    }
    assert_eq!(listed(&vault).len(), 8, "the main account, four in lei and three in euro: nothing else was made");
}

#[test]
fn a_rate_is_more_than_zero_with_up_to_eight_decimals_and_is_kept_as_typed() {
    let (_dir, vault) = fresh();
    for (n, rate) in ["5", "5.00", "0.5", "0.00000001", "999999999.99999999", "4.9735", "10", "999999999"].iter().enumerate() {
        let made = ok(&vault, "accounts.create", json!({ "name": format!("Rate {n}"), "kind": "CASH", "currency": "RON", "rate": rate }));
        assert_eq!(made["rate"], *rate, "kept as it was typed");
    }
    let id = listed(&vault).iter().find(|a| a["name"] == "Rate 0").map(id_of).unwrap();
    // the spaces round it are dropped
    assert_eq!(ok(&vault, "accounts.create", json!({ "name": "Spaced", "kind": "CASH", "currency": "RON", "rate": "  4.97 " }))["rate"], "4.97");
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, "rate": " 5.10 " }))["rate"], "5.10");
    let long = "1".repeat(21);
    for rate in [
        "0", "0.0", "0.00", "0.00000000", "-1", "-1.00", "-0.5", "+5.00", "abc", "5,00", "", " ", ".5", "5.", "1.123456789", "0.000000001", "1000000000", "1000000000.00", "05.00", "1e2", "5 .00", "٥.٠٠", "NaN", "5.00.1", "5/1",
        long.as_str(),
    ] {
        refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CASH", "currency": "RON", "rate": rate }), INVALID);
        refuse(&vault, "accounts.update", json!({ "id": id, "rate": rate }), INVALID);
    }
    for value in [json!(5), json!(5.0), json!(true), json!(null), json!(["5.00"])] {
        assert_eq!(refuse(&vault, "accounts.create", json!({ "name": "Bad", "kind": "CASH", "currency": "RON", "rate": value }), INVALID), "`rate` must be a string", "{value}");
        assert_eq!(refuse(&vault, "accounts.update", json!({ "id": id, "rate": value }), INVALID), "`rate` must be a string", "{value}");
    }
    assert_eq!(find(&vault, &id)["rate"], "5.10", "a refused rate changed nothing");
}

#[test]
fn a_rate_so_small_that_the_balance_does_not_fit_shows_the_bound_it_passed_and_the_list_stays_readable() {
    let (_dir, vault) = fresh();
    let make = |name: &str, rate: &str, opening: &str| id_of(&ok(&vault, "accounts.create", json!({ "name": name, "kind": "CASH", "currency": "RON", "rate": rate, "openingBalance": opening, "openingDate": "2026-01-01" })));
    let (up, down, tiny) = (make("Up", "0.00000001", "1000.00"), make("Down", "0.00000001", "-1000.00"), make("Tiny", "999999999.99999999", "1000.00"));
    assert_eq!(find(&vault, &up)["balanceInMain"], "9999999999.99", "the bound it passed, and no error");
    assert_eq!(find(&vault, &down)["balanceInMain"], "-9999999999.99");
    assert_eq!(find(&vault, &tiny)["balanceInMain"], "0.00", "a rate that makes it less than a cent");
    assert_eq!(ok(&vault, "accounts.summary", json!({}))["inAccounts"], "0.00", "the two bounds cancel, and net worth can still be asked");
    ok(&vault, "accounts.update", json!({ "id": down, "includeInNetWorth": false }));
    assert_eq!(ok(&vault, "accounts.summary", json!({}))["inAccounts"], "9999999999.99");
    // the balance itself is still what it is
    assert_eq!((held(&vault, &up), held(&vault, &down)), ("1000.00".to_string(), "-1000.00".to_string()));
}

#[test]
fn the_main_account_stays_in_the_apps_currency_whatever_is_asked() {
    let (_dir, vault) = fresh();
    for body in [
        json!({ "currency": "RON" }),
        json!({ "currency": "ron", "rate": "5.00" }),
        json!({ "currency": "RON", "rate": "5.00" }),
        json!({ "currency": "EUR" }),
        json!({ "currency": "eur" }),
        json!({ "currency": "EUR", "rate": "5.00" }),
        json!({ "rate": "5.00" }),
        json!({ "name": "Renamed", "currency": "RON", "rate": "5.00" }),
        json!({ "name": "Renamed", "balance": "10.00", "rate": "5.00" }),
    ] {
        refuse(&vault, "accounts.update", with(body, "id", json!("1")), INVALID);
    }
    assert_eq!((order(&vault), currency_of(&find(&vault, "1"))), (vec!["Main account".to_string()], ("EUR", false, None, "0.00")));
    // what the main account may do, it still does
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": "1", "name": "Current account", "balance": "10.00" }))["balance"], "10.00");
}

#[test]
fn a_new_rate_changes_what_a_foreign_account_is_worth_in_the_apps_currency_and_no_amount_that_is_stored() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let (lei, euro) = (lei(&vault, "Lei account", "1000.00"), account(&vault, "Euro account"));
    spend_native(&vault, &food, "Boots", "4.00", "20.00", "2026-10-03", &lei);
    transfer_across(&vault, &euro, &lei, "40.00", "200.00", "2026-10-06");
    let (rows, moves) = (ok(&vault, "transactions.list", json!({})), ok(&vault, "transfers.list", json!({})));
    let statement = ok(&vault, "accounts.show", json!({ "id": lei, "from": "2026-01-01", "to": "2026-12-31" }));
    assert_eq!(currency_of(&find(&vault, &lei)), ("RON", true, Some("5.00"), "236.00"), "1180.00 lei at 5.00 to the euro");
    assert_eq!(ok(&vault, "accounts.summary", json!({}))["inAccounts"], "196.00", "236.00 in lei and 40.00 less in euro");

    let changed = ok(&vault, "accounts.update", json!({ "id": lei, "rate": "4.00" }));
    assert_eq!(currency_of(&changed), ("RON", true, Some("4.00"), "295.00"));
    assert_eq!((changed["balance"].as_str(), changed["openingBalance"].as_str(), changed["entries"].as_u64()), (Some("1180.00"), Some("1000.00"), Some(2)));
    // no amount that is stored moved: not the rows, not the statement
    assert_eq!((ok(&vault, "transactions.list", json!({})), ok(&vault, "transfers.list", json!({}))), (rows, moves));
    let after = ok(&vault, "accounts.show", json!({ "id": lei, "from": "2026-01-01", "to": "2026-12-31" }));
    assert_eq!((&after["lines"], &after["balanceBefore"], &after["balanceAfter"]), (&statement["lines"], &statement["balanceBefore"], &statement["balanceAfter"]));
    assert_eq!(ok(&vault, "accounts.summary", json!({}))["inAccounts"], "255.00", "net worth follows the rate");
    // a rate for an account in the app's currency, for the main account and for an archived one is refused
    refuse(&vault, "accounts.update", json!({ "id": euro, "rate": "5.00" }), INVALID);
    refuse(&vault, "accounts.update", json!({ "id": "1", "rate": "5.00" }), INVALID);
    ok(&vault, "accounts.archive", json!({ "id": lei }));
    refuse(&vault, "accounts.update", json!({ "id": lei, "rate": "3.00" }), INVALID);
    assert_eq!(currency_of(&find(&vault, &lei)), ("RON", true, Some("4.00"), "295.00"), "an archived account keeps its rate and still counts");
    assert_eq!(ok(&vault, "accounts.summary", json!({}))["inAccounts"], "255.00");
}

#[test]
fn the_currency_of_an_account_changes_only_while_nothing_is_in_it_and_the_apps_own_clears_the_rate() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    // each of the four kinds of row counts as an entry
    let (by_tx, by_rec, by_loan, by_transfer) = (account(&vault, "A"), account(&vault, "B"), account(&vault, "C"), account(&vault, "D"));
    let tx = id_of(&spend(&vault, &food, "Bread", "2.00", "2026-10-01", Some(&by_tx)));
    let rec = id_of(&ok(&vault, "recurring.create", json!({ "name": "Gym", "amount": "20.00", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1, "accountId": by_rec })));
    let loan = id_of(&ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "9.00", "date": "2026-10-02", "accountId": by_loan })));
    let moved = transfer(&vault, "1", &by_transfer, "5.00", "2026-10-01");
    for id in [&by_tx, &by_rec, &by_loan, &by_transfer] {
        assert_eq!(find(&vault, id)["entries"], 1);
        for code in ["RON", "ron", "USD"] {
            refuse(&vault, "accounts.update", json!({ "id": id, "currency": code, "rate": "5.00" }), INVALID);
        }
        // and the rest of that patch is not applied
        refuse(&vault, "accounts.update", json!({ "id": id, "name": "Renamed", "currency": "RON", "rate": "5.00" }), INVALID);
        assert_eq!(currency_of(&find(&vault, id)).0, "EUR");
    }
    // the entries go, and each account changes
    assert_eq!(ok(&vault, "transactions.delete", json!({ "id": tx })), Value::Null);
    assert_eq!(ok(&vault, "recurring.delete", json!({ "id": rec })), Value::Null);
    assert_eq!(ok(&vault, "ledger.delete", json!({ "id": loan })), Value::Null);
    assert_eq!(ok(&vault, "transfers.delete", json!({ "id": moved })), Value::Null);
    for id in [&by_tx, &by_rec, &by_loan, &by_transfer] {
        assert_eq!(find(&vault, id)["entries"], 0);
        assert_eq!(currency_of(&ok(&vault, "accounts.update", json!({ "id": id, "currency": "RON", "rate": "5.00" }))), ("RON", true, Some("5.00"), "0.00"));
    }
    // a foreign account that nothing is in: another code keeps the rate it has, or takes a new one with it
    assert_eq!(currency_of(&ok(&vault, "accounts.update", json!({ "id": by_tx, "currency": "usd" }))), ("USD", true, Some("5.00"), "0.00"));
    assert_eq!(currency_of(&ok(&vault, "accounts.update", json!({ "id": by_tx, "currency": "GBP", "rate": "0.85" }))), ("GBP", true, Some("0.85"), "0.00"));
    // the app's own currency makes it an ordinary account again, which has no rate, and a rate with it is refused
    refuse(&vault, "accounts.update", json!({ "id": by_tx, "currency": "EUR", "rate": "1.00" }), INVALID);
    refuse(&vault, "accounts.update", json!({ "id": by_tx, "currency": "eur", "rate": "0.85" }), INVALID);
    let back = ok(&vault, "accounts.update", json!({ "id": by_tx, "currency": "eur" }));
    assert_eq!((currency_of(&back), back["rate"].clone()), (("EUR", false, None, "0.00"), Value::Null), "the rate is cleared");
    refuse(&vault, "accounts.update", json!({ "id": by_tx, "rate": "5.00" }), INVALID);
    // an account in the app's currency becomes foreign only with a rate
    refuse(&vault, "accounts.update", json!({ "id": by_tx, "currency": "RON" }), INVALID);
    assert_eq!(currency_of(&ok(&vault, "accounts.update", json!({ "id": by_tx, "currency": "RON", "rate": "4.97" }))), ("RON", true, Some("4.97"), "0.00"));

    // a foreign account with an entry keeps its currency: the same code may come with a new rate, any other code is refused, the app's own too
    let l = lei(&vault, "Lei account", "100.00");
    let boots = id_of(&spend_native(&vault, &food, "Boots", "4.00", "20.00", "2026-10-03", &l));
    for body in [json!({ "currency": "USD" }), json!({ "currency": "USD", "rate": "1.10" }), json!({ "currency": "EUR" }), json!({ "currency": "eur" })] {
        refuse(&vault, "accounts.update", with(body, "id", json!(l)), INVALID);
    }
    assert_eq!(currency_of(&ok(&vault, "accounts.update", json!({ "id": l, "currency": "ron", "rate": "4.50" }))), ("RON", true, Some("4.50"), "17.78"), "80.00 lei at 4.50, rounded away from zero");
    // the row leaves for the main account (and loses its native amount), and the account is free to change
    ok(&vault, "transactions.update", json!({ "id": boots, "accountId": "1" }));
    assert_eq!(currency_of(&ok(&vault, "accounts.update", json!({ "id": l, "currency": "EUR" }))), ("EUR", false, None, "100.00"));
}

#[test]
fn a_patch_equal_to_what_the_account_has_writes_nothing_for_the_currency_the_rate_and_the_balance() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let (l, euro) = (lei(&vault, "Lei account", "1000.00"), account(&vault, "Euro account"));
    spend_native(&vault, &food, "Boots", "4.00", "20.00", "2026-10-03", &l);
    spend(&vault, &food, "Bread", "2.00", "2026-10-03", Some(&euro));
    let (foreign, plain) = (find(&vault, &l), find(&vault, &euro));
    let before = dump(&vault);
    for body in [
        json!({ "currency": "RON" }),
        json!({ "currency": "ron" }),
        json!({ "rate": "5.00" }),
        json!({ "rate": " 5.00 " }),
        json!({ "currency": "RON", "rate": "5.00" }),
        json!({ "balance": "980.00" }),
        json!({ "currency": "ron", "rate": "5.00", "balance": "980.00", "name": "Lei account" }),
    ] {
        assert_eq!(ok(&vault, "accounts.update", with(body.clone(), "id", json!(l))), foreign, "{body}");
        assert_eq!(dump(&vault), before, "{body} wrote something: a stamp, a time or a line of the history");
    }
    for body in [json!({ "currency": "EUR" }), json!({ "currency": "eur" }), json!({ "balance": "-2.00" })] {
        assert_eq!(ok(&vault, "accounts.update", with(body.clone(), "id", json!(euro))), plain, "{body}");
        assert_eq!(dump(&vault), before, "{body}");
    }
    // the check can see a change: the same rate written another way is kept as typed, and that is a change
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": l, "rate": "5.0" }))["rate"], "5.0");
    assert_ne!(dump(&vault), before);
}

#[test]
fn an_archived_account_in_another_currency_keeps_its_currency_and_its_rate_and_cannot_be_changed_until_it_is_brought_back() {
    let (_dir, vault) = fresh();
    let l = lei(&vault, "Lei account", "1000.00");
    let archived = ok(&vault, "accounts.archive", json!({ "id": l }));
    assert_eq!(currency_of(&archived), ("RON", true, Some("5.00"), "200.00"), "archiving changes no figure");
    assert_eq!(ok(&vault, "accounts.summary", json!({}))["inAccounts"], "200.00", "and it still counts in net worth");
    for body in [json!({ "rate": "4.00" }), json!({ "currency": "USD", "rate": "1.10" }), json!({ "currency": "EUR" }), json!({ "balance": "5.00" }), json!({ "rate": "5.00" })] {
        refuse(&vault, "accounts.update", with(body, "id", json!(l)), INVALID);
    }
    assert_eq!(find(&vault, &l), archived);
    assert_eq!(ok(&vault, "accounts.show", json!({ "id": l }))["currency"], "RON", "it is read with its currency");
    assert_eq!(currency_of(&ok(&vault, "accounts.unarchive", json!({ "id": l }))), ("RON", true, Some("5.00"), "200.00"));
    assert_eq!(currency_of(&ok(&vault, "accounts.update", json!({ "id": l, "rate": "4.00" }))), ("RON", true, Some("4.00"), "250.00"));
}

// ---- the balance of today -----------------------------------------------------------------------

#[test]
fn the_balance_of_today_is_set_by_moving_the_opening_balance_by_the_difference_and_every_earlier_balance_moves_with_it() {
    let (_dir, vault) = fresh();
    let (food, pay) = (category(&vault, "EXPENSE", "Food"), category(&vault, "INCOME", "Pay"));
    let a = id_of(&ok(&vault, "accounts.create", json!({ "name": "Current", "kind": "CHECKING", "openingBalance": "100.00", "openingDate": "2026-01-01" })));
    spend(&vault, &food, "Boots", "30.00", "2026-02-01", Some(&a));
    spend(&vault, &pay, "Gift", "10.00", "2026-03-01", Some(&a));
    transfer(&vault, "1", &a, "5.00", "2026-04-01");
    assert_eq!(held(&vault, &a), "85.00");
    let statement = || ok(&vault, "accounts.show", json!({ "id": a, "from": "2026-02-01", "to": "2026-12-31" }));
    let january = || ok(&vault, "accounts.history", json!({ "id": a, "year": "2026" }))[0]["balance"].clone();
    assert_eq!((statement()["balanceBefore"].clone(), january()), (json!("100.00"), json!("100.00")));

    let set = ok(&vault, "accounts.update", json!({ "id": a, "balance": "500.00" }));
    assert_eq!(keys(&set), ACCOUNT_KEYS);
    assert_eq!((set["balance"].as_str(), set["openingBalance"].as_str(), set["openingDate"].as_str(), set["entries"].as_u64()), (Some("500.00"), Some("515.00"), Some("2026-01-01"), Some(3)), "moved by 415.00");
    assert_eq!(held(&vault, &a), "500.00");
    // every earlier balance moved by the same 415.00, and what moved the account is as it was
    let after = statement();
    assert_eq!((after["balanceBefore"].as_str(), after["balanceAfter"].as_str()), (Some("515.00"), Some("500.00")));
    assert_eq!(after["lines"].as_array().unwrap().iter().map(|l| l["amount"].as_str().unwrap()).collect::<Vec<_>>(), ["-30.00", "10.00", "5.00"]);
    assert_eq!(january(), json!("515.00"));
    // below what it holds, below zero, and zero
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": a, "balance": "-50.00" }))["openingBalance"], "-35.00");
    assert_eq!(held(&vault, &a), "-50.00");
    let zero = ok(&vault, "accounts.update", json!({ "id": a, "balance": "0.00" }));
    assert_eq!((zero["balance"].as_str(), zero["openingBalance"].as_str()), (Some("0.00"), Some("15.00")));
    // with another field of the same patch: both are applied
    let both = ok(&vault, "accounts.update", json!({ "id": a, "name": "Renamed", "balance": "7.50" }));
    assert_eq!((both["name"].as_str(), both["balance"].as_str(), both["openingBalance"].as_str()), (Some("Renamed"), Some("7.50"), Some("22.50")));
    // the main account is like the others
    let main = ok(&vault, "accounts.update", json!({ "id": "1", "balance": "250.00" }));
    assert_eq!((main["balance"].as_str(), main["openingBalance"].as_str()), (Some("250.00"), Some("255.00")), "the transfer of 5.00 out of it is kept");
    // a balance equal to the current one writes nothing, whatever the account
    let before = dump(&vault);
    for (id, balance) in [(a.as_str(), "7.50"), ("1", "250.00")] {
        let shown = find(&vault, id);
        assert_eq!(ok(&vault, "accounts.update", json!({ "id": id, "balance": balance })), shown);
        assert_eq!(dump(&vault), before, "{id}");
    }
}

#[test]
fn the_balance_is_given_with_the_texts_of_an_opening_balance_and_nothing_else() {
    let (_dir, vault) = fresh();
    // on an account with nothing in it the two are the same number, so each text must get the same answer from both
    let (by_balance, by_opening) = (account(&vault, "By balance"), account(&vault, "By opening"));
    let texts = [
        "0.00", "0.01", "-0.01", "-120.00", "9999999999.99", "-9999999999.99", "1234567890.12", "-0.00", "+1.00", "1.5", "1", "-1", "1.", ".5", "-.50", "1.005", "10000000000.00", "-10000000000.00", "9999999999.995",
        "99999999999.99", "--1.00", "- 1.00", " 1.00", "1.00 ", "1,00", "1e3", "-1e3", "NaN", "abc", "", "-", "０.００", "1.00\n", "0x10.00", "05.00",
    ];
    let mut accepted = 0;
    for text in texts {
        let (b, o) = (run(&vault, "accounts.update", json!({ "id": by_balance, "balance": text })), run(&vault, "accounts.update", json!({ "id": by_opening, "openingBalance": text })));
        match (b, o) {
            (Ok(b), Ok(o)) => {
                assert_eq!((b["balance"].as_str(), b["openingBalance"].as_str()), (Some(text), Some(text)), "{text:?} comes back as written");
                assert_eq!(o["balance"], b["balance"]);
                accepted += 1;
            }
            (Err(b), Err(o)) => assert_eq!(((b.status, b.code), (o.status, o.code)), (INVALID, INVALID), "{text:?}"),
            (b, o) => panic!("{text:?}: balance {b:?}, opening balance {o:?}"),
        }
    }
    assert_eq!(accepted, 7, "the first seven are the good ones");
    // a number is not text, and null is not a value
    for value in [json!(5), json!(-1.5), json!(true), json!(null), json!(["1.00"])] {
        assert_eq!(refuse(&vault, "accounts.update", json!({ "id": by_balance, "balance": value }), INVALID), "`balance` must be a string", "{value}");
    }
}

#[test]
fn the_balance_may_not_take_the_opening_balance_past_its_bounds_and_nothing_changes_when_it_would() {
    let (_dir, vault) = fresh();
    let (food, pay) = (category(&vault, "EXPENSE", "Food"), category(&vault, "INCOME", "Pay"));
    let (up, down) = (account(&vault, "Up"), account(&vault, "Down"));
    spend(&vault, &pay, "Gift", "10.00", "2026-10-01", Some(&up));
    spend(&vault, &food, "Lunch", "10.00", "2026-10-01", Some(&down));
    // 10.00 came in: the opening balance is the balance less 10.00, and may not go below -9999999999.99
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": up, "balance": "9999999999.99" }))["openingBalance"], "9999999989.99");
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": up, "balance": "-9999999989.99" }))["openingBalance"], "-9999999999.99", "exactly the bound");
    refuse(&vault, "accounts.update", json!({ "id": up, "balance": "-9999999990.00" }), INVALID);
    refuse(&vault, "accounts.update", json!({ "id": up, "balance": "-9999999999.99" }), INVALID);
    // 10.00 went out: it may not go above 9999999999.99
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": down, "balance": "9999999989.99" }))["openingBalance"], "9999999999.99", "exactly the bound");
    refuse(&vault, "accounts.update", json!({ "id": down, "balance": "9999999990.00" }), INVALID);
    refuse(&vault, "accounts.update", json!({ "id": down, "balance": "9999999999.99" }), INVALID);
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": down, "balance": "-9999999999.99" }))["openingBalance"], "-9999999989.99");
    // beyond what money can be at all
    for balance in ["10000000000.00", "-10000000000.00", "99999999999999999999.00"] {
        refuse(&vault, "accounts.update", json!({ "id": up, "balance": balance }), INVALID);
    }
}

#[test]
fn the_balance_cannot_be_given_with_the_opening_balance_and_not_for_an_archived_account() {
    let (_dir, vault) = fresh();
    let a = id_of(&ok(&vault, "accounts.create", json!({ "name": "Current", "kind": "CHECKING", "openingBalance": "100.00", "openingDate": "2026-01-01" })));
    // together, whatever the two say, even when they agree
    for body in [json!({ "balance": "100.00", "openingBalance": "100.00" }), json!({ "balance": "5.00", "openingBalance": "5.00" }), json!({ "balance": "5.00", "openingBalance": "0.00" }), json!({ "name": "X", "balance": "5.00", "openingBalance": "9.00" })] {
        refuse(&vault, "accounts.update", with(body, "id", json!(a)), INVALID);
    }
    refuse(&vault, "accounts.update", json!({ "id": "1", "balance": "5.00", "openingBalance": "5.00" }), INVALID);
    // an archived account is brought back first, for this as for everything
    ok(&vault, "accounts.archive", json!({ "id": a }));
    refuse(&vault, "accounts.update", json!({ "id": a, "balance": "5.00" }), INVALID);
    refuse(&vault, "accounts.update", json!({ "id": a, "balance": "100.00" }), INVALID);
    ok(&vault, "accounts.unarchive", json!({ "id": a }));
    assert_eq!(ok(&vault, "accounts.update", json!({ "id": a, "balance": "5.00" }))["openingBalance"], "5.00");
    // an account that is not there
    refuse(&vault, "accounts.update", json!({ "id": "999999", "balance": "5.00" }), NOT_FOUND);
}

#[test]
fn the_balance_of_an_account_in_another_currency_is_in_its_currency_and_worth_its_value_in_the_apps() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let l = lei(&vault, "Lei account", "0.00");
    spend_native(&vault, &food, "Boots", "20.00", "100.00", "2026-10-03", &l);
    assert_eq!(currency_of(&find(&vault, &l)), ("RON", true, Some("5.00"), "-20.00"));
    let set = ok(&vault, "accounts.update", json!({ "id": l, "balance": "500.00" }));
    assert_eq!((set["balance"].as_str(), set["openingBalance"].as_str(), set["currency"].as_str()), (Some("500.00"), Some("600.00"), Some("RON")), "lei, and the 100.00 lei the boots cost are kept");
    assert_eq!(currency_of(&set), ("RON", true, Some("5.00"), "100.00"), "500.00 lei at 5.00 to the euro");
    // it can be negative, and with the rate changed in the same patch the new rate is what it is worth at
    let both = ok(&vault, "accounts.update", json!({ "id": l, "balance": "-50.00", "rate": "4.00" }));
    assert_eq!(currency_of(&both), ("RON", true, Some("4.00"), "-12.50"));
    assert_eq!(both["openingBalance"], "50.00");
}

#[test]
fn rows_dated_after_today_are_no_part_of_the_balance_of_today_and_are_kept_when_it_is_set() {
    let (_dir, vault) = fresh();
    let (food, pay) = (category(&vault, "EXPENSE", "Food"), category(&vault, "INCOME", "Pay"));
    let a = id_of(&ok(&vault, "accounts.create", json!({ "name": "Current", "kind": "CHECKING", "openingBalance": "100.00", "openingDate": "2026-01-01" })));
    spend(&vault, &pay, "Gift", "7.00", "2026-10-10", Some(&a));
    spend(&vault, &food, "Rent", "30.00", "2026-10-20", Some(&a));
    transfer(&vault, &a, "1", "1.00", "2026-10-25");
    assert_eq!(held(&vault, &a), "107.00", "today is the 14th");
    let set = ok(&vault, "accounts.update", json!({ "id": a, "balance": "200.00" }));
    assert_eq!((set["balance"].as_str(), set["openingBalance"].as_str()), (Some("200.00"), Some("193.00")), "only the 7.00 that came in before today is taken out");
    assert_eq!(ok(&vault, "accounts.show", json!({ "id": a, "from": "2026-10-01", "to": "2026-10-31" }))["balanceAfter"], "169.00", "the rows to come still change it: 200.00 less 30.00 and 1.00");
}

#[test]
fn an_opening_date_after_today_refuses_the_balance_unless_the_same_patch_moves_it_back() {
    let (_dir, vault) = fresh();
    let a = id_of(&ok(&vault, "accounts.create", json!({ "name": "Later", "kind": "CHECKING", "openingBalance": "50.00", "openingDate": "2026-11-01" })));
    assert_eq!(held(&vault, &a), "0.00", "an opening balance dated later is not in today's balance");
    refuse(&vault, "accounts.update", json!({ "id": a, "balance": "10.00" }), INVALID);
    refuse(&vault, "accounts.update", json!({ "id": a, "balance": "0.00" }), INVALID);
    // a date after today in the same patch is refused too, tomorrow as much as next year; today is not after
    let b = account(&vault, "Earlier");
    refuse(&vault, "accounts.update", json!({ "id": b, "balance": "10.00", "openingDate": "2026-10-15" }), INVALID);
    refuse(&vault, "accounts.update", json!({ "id": b, "balance": "10.00", "openingDate": "2027-01-01" }), INVALID);
    refuse(&vault, "accounts.update", json!({ "id": b, "balance": "10.00", "openingDate": "2026-02-30" }), INVALID);
    let today = ok(&vault, "accounts.update", json!({ "id": b, "balance": "10.00", "openingDate": "2026-10-14" }));
    assert_eq!((today["balance"].as_str(), today["openingBalance"].as_str(), today["openingDate"].as_str()), (Some("10.00"), Some("10.00"), Some("2026-10-14")));
    // the date that is after today is put right in the same patch
    let fixed = ok(&vault, "accounts.update", json!({ "id": a, "balance": "10.00", "openingDate": "2026-01-01" }));
    assert_eq!((fixed["balance"].as_str(), fixed["openingBalance"].as_str(), fixed["openingDate"].as_str()), (Some("10.00"), Some("10.00"), Some("2026-01-01")));
}

// ---- a transaction in an account of another currency --------------------------------------------

#[test]
fn a_transaction_in_an_account_of_another_currency_needs_what_really_moved_it_and_answers_both_amounts() {
    let (_dir, vault) = fresh();
    let (food, pay) = (category(&vault, "EXPENSE", "Food"), category(&vault, "INCOME", "Pay"));
    let (l, euro) = (lei(&vault, "Lei account", "1000.00"), account(&vault, "Euro account"));
    // without it: refused, and nothing is made
    let boots = json!({ "name": "Boots", "amount": "4.00", "paymentDate": "2026-10-03", "categoryId": food, "accountId": l });
    refuse(&vault, "transactions.create", boots.clone(), INVALID);
    let made = ok(&vault, "transactions.create", with(boots.clone(), "nativeAmount", json!("20.00")));
    assert_eq!((made["amount"].as_str(), made["nativeAmount"].as_str(), made["accountId"].as_str()), (Some("4.00"), Some("20.00"), Some(l.as_str())));
    // the account moved by what really left it, and is worth the amount in the app's currency at its rate
    assert_eq!(held(&vault, &l), "980.00");
    assert_eq!(currency_of(&find(&vault, &l)), ("RON", true, Some("5.00"), "196.00"));
    // an income is the same, the other way
    let gift = spend_native(&vault, &pay, "Gift", "2.00", "10.00", "2026-10-05", &l);
    assert_eq!((gift["amount"].as_str(), gift["nativeAmount"].as_str()), (Some("2.00"), Some("10.00")));
    assert_eq!(held(&vault, &l), "990.00");
    // listed with both, and the same numbers twice are fine
    let listed_row = row(&vault, &id_of(&made));
    assert_eq!((listed_row["amount"].as_str(), listed_row["nativeAmount"].as_str()), (Some("4.00"), Some("20.00")));
    assert_eq!(spend_native(&vault, &food, "Same", "3.00", "3.00", "2026-10-06", &l)["nativeAmount"], "3.00");
    assert_eq!(held(&vault, &l), "987.00");
    // an account in the app's currency, the main account and no account at all take none
    for account in [json!(euro), json!("1"), json!(null)] {
        let mut body = json!({ "name": "Bread", "amount": "4.00", "paymentDate": "2026-10-03", "categoryId": food, "nativeAmount": "4.00" });
        if !account.is_null() {
            body["accountId"] = account.clone();
        }
        refuse(&vault, "transactions.create", body, INVALID);
    }
    // made without one, such a row answers none, in the answer and in the list
    for account in [Some(euro.as_str()), Some("1"), None] {
        let plain = spend(&vault, &food, "Bread", "4.00", "2026-10-03", account);
        assert_eq!(plain["nativeAmount"], Value::Null, "{account:?}");
        assert_eq!(row(&vault, &id_of(&plain))["nativeAmount"], Value::Null, "{account:?}");
    }
    // an archived account of another currency is refused as any archived account is
    ok(&vault, "accounts.archive", json!({ "id": l }));
    refuse(&vault, "transactions.create", with(boots, "nativeAmount", json!("20.00")), INVALID);
}

#[test]
fn the_native_amount_is_checked_as_the_amount_is_and_a_refused_one_changes_nothing() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let l = lei(&vault, "Lei account", "0.00");
    let body = |native: Value| json!({ "name": "Boots", "amount": "4.00", "paymentDate": "2026-10-03", "categoryId": food, "accountId": l, "nativeAmount": native });
    for good in ["0.01", "0.10", "20.00", "1234567.89", "9999999999.99"] {
        let made = ok(&vault, "transactions.create", body(json!(good)));
        assert_eq!(made["nativeAmount"], good, "{good} comes back as written");
        assert_eq!(row(&vault, &id_of(&made))["nativeAmount"], good);
    }
    let t = id_of(&spend_native(&vault, &food, "One", "1.00", "5.00", "2026-10-04", &l));
    for bad in ["0.00", "12.5", "12", "12.", ".50", "12.500", "01.00", "-1.00", "+1.00", "1,50", "1e3", " 1.00", "1.00 ", "１２.５０", "10000000000.00", "99999999999.99", ""] {
        refuse(&vault, "transactions.create", body(json!(bad)), INVALID);
        refuse(&vault, "transactions.update", json!({ "id": t, "nativeAmount": bad }), INVALID);
    }
    for value in [json!(12.5), json!(20), json!(true), json!(null), json!(["1.00"])] {
        let message = refuse(&vault, "transactions.create", body(value.clone()), INVALID);
        assert_eq!(message, "`nativeAmount` must be an amount as text with two decimals, like \"12.50\"", "{value}");
        refuse(&vault, "transactions.update", json!({ "id": t, "nativeAmount": value }), INVALID);
    }
    // the amount is checked as ever: a good native amount does not make a bad amount good
    for amount in ["0.00", "-4.00", "4", "10000000000.00"] {
        refuse(&vault, "transactions.create", with(body(json!("20.00")), "amount", json!(amount)), INVALID);
        refuse(&vault, "transactions.update", json!({ "id": t, "amount": amount, "nativeAmount": "20.00" }), INVALID);
    }
    assert_eq!(row(&vault, &t)["nativeAmount"], "5.00");
}

#[test]
fn a_transaction_moved_between_accounts_takes_gives_up_and_keeps_its_native_amount_as_the_note_says() {
    let (_dir, vault) = fresh();
    let (food, pay) = (category(&vault, "EXPENSE", "Food"), category(&vault, "INCOME", "Pay"));
    let (l1, l2, euro) = (lei(&vault, "Lei one", "100.00"), lei(&vault, "Lei two", "100.00"), account(&vault, "Euro account"));
    let t = id_of(&spend(&vault, &food, "Bread", "2.00", "2026-10-01", None));
    assert_eq!(held(&vault, "1"), "-2.00");
    // a row that is and stays in an account in the app's currency takes none, named account or not
    for body in [json!({ "nativeAmount": "10.00" }), json!({ "accountId": "1", "nativeAmount": "10.00" }), json!({ "accountId": euro, "nativeAmount": "10.00" })] {
        refuse(&vault, "transactions.update", with(body, "id", json!(t)), INVALID);
    }
    // main to foreign: the native amount is needed in the same patch
    refuse(&vault, "transactions.update", json!({ "id": t, "accountId": l1 }), INVALID);
    refuse(&vault, "transactions.update", json!({ "id": t, "accountId": l1, "amount": "3.00" }), INVALID);
    let moved = ok(&vault, "transactions.update", json!({ "id": t, "accountId": l1, "nativeAmount": "10.00" }));
    assert_eq!((moved["accountId"].as_str(), moved["amount"].as_str(), moved["nativeAmount"].as_str()), (Some(l1.as_str()), Some("2.00"), Some("10.00")));
    assert_eq!((held(&vault, "1"), held(&vault, &l1)), ("0.00".into(), "90.00".to_string()), "the money followed the row, in its own amount");
    // in the foreign account it changes in place, and a patch that leaves it out keeps it
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": t, "nativeAmount": "12.00" }))["nativeAmount"], "12.00");
    assert_eq!(held(&vault, &l1), "88.00");
    let amount_only = ok(&vault, "transactions.update", json!({ "id": t, "amount": "3.00" }));
    assert_eq!((amount_only["amount"].as_str(), amount_only["nativeAmount"].as_str()), (Some("3.00"), Some("12.00")), "the euro amount changes alone");
    assert_eq!(held(&vault, &l1), "88.00", "and the account does not move: it is in lei");
    let other = ok(&vault, "transactions.update", json!({ "id": t, "name": "Bread 2", "paymentDate": "2026-10-02", "description": "note" }));
    assert_eq!((other["name"].as_str(), other["nativeAmount"].as_str()), (Some("Bread 2"), Some("12.00")));
    // the same native amount again changes nothing (a transaction takes a new stamp on any update, as it always did, but the history sees no change)
    let history = || vault.with_conn(|c| Ok(c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_, i64>(0))?)).unwrap();
    let (before, lines) = (row(&vault, &t), history());
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": t, "nativeAmount": "12.00" }))["nativeAmount"], "12.00");
    assert_eq!((row(&vault, &t), history()), (before, lines));
    // foreign to foreign keeps it unless a new one is given
    let kept = ok(&vault, "transactions.update", json!({ "id": t, "accountId": l2 }));
    assert_eq!((kept["accountId"].as_str(), kept["nativeAmount"].as_str()), (Some(l2.as_str()), Some("12.00")));
    assert_eq!((held(&vault, &l1), held(&vault, &l2)), ("100.00".into(), "88.00".to_string()));
    let given = ok(&vault, "transactions.update", json!({ "id": t, "accountId": l1, "nativeAmount": "15.00" }));
    assert_eq!((given["accountId"].as_str(), given["nativeAmount"].as_str()), (Some(l1.as_str()), Some("15.00")));
    assert_eq!((held(&vault, &l1), held(&vault, &l2)), ("85.00".into(), "100.00".to_string()));
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": t, "accountId": l1, "name": "Bread 3" }))["nativeAmount"], "15.00", "naming its own account again is no move");

    // foreign to an account in the app's currency: the native amount is cleared, and one given with it is refused
    for body in [json!({ "accountId": "1", "nativeAmount": "1.00" }), json!({ "accountId": euro, "nativeAmount": "1.00" })] {
        refuse(&vault, "transactions.update", with(body, "id", json!(t)), INVALID);
    }
    let back = ok(&vault, "transactions.update", json!({ "id": t, "accountId": euro }));
    assert_eq!((back["accountId"].as_str(), back["nativeAmount"].clone()), (Some(euro.as_str()), Value::Null));
    assert_eq!(row(&vault, &t)["nativeAmount"], Value::Null);
    assert_eq!((held(&vault, &euro), held(&vault, &l1)), ("-3.00".into(), "100.00".to_string()), "the amount in euro now moves it");
    // into a foreign account again it needs one: the old one is gone
    refuse(&vault, "transactions.update", json!({ "id": t, "accountId": l1 }), INVALID);
    refuse(&vault, "transactions.update", json!({ "id": t, "accountId": l2 }), INVALID);
    let again = ok(&vault, "transactions.update", json!({ "id": t, "accountId": l2, "nativeAmount": "9.00" }));
    assert_eq!((again["nativeAmount"].as_str(), held(&vault, &l2).as_str()), (Some("9.00"), "91.00"));
    // the main account too: out of it and into it
    let main = ok(&vault, "transactions.update", json!({ "id": t, "accountId": "1" }));
    assert_eq!((main["accountId"].as_str(), main["nativeAmount"].clone()), (Some("1"), Value::Null));
    assert_eq!(held(&vault, "1"), "-3.00");
    // an income moves the same way
    let income = id_of(&spend(&vault, &pay, "Gift", "2.00", "2026-10-05", Some(&euro)));
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": income, "accountId": l1, "nativeAmount": "10.00" }))["nativeAmount"], "10.00");
    assert_eq!((held(&vault, &euro), held(&vault, &l1)), ("0.00".into(), "110.00".to_string()));
}

#[test]
fn budgets_and_statistics_count_the_amount_in_the_apps_currency_and_the_balance_the_native_one() {
    let (_dir, vault) = fresh();
    let (food, pay) = (category(&vault, "EXPENSE", "Food"), category(&vault, "INCOME", "Pay"));
    ok(&vault, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-10", "amountLimit": "200.00" }));
    ok(&vault, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-10", "amountLimit": "100.00", "categoryId": food }));
    let l = lei(&vault, "Lei account", "1000.00");
    spend_native(&vault, &food, "Boots", "4.00", "20.00", "2026-10-03", &l);
    spend(&vault, &food, "Bread", "6.00", "2026-10-04", None);
    spend_native(&vault, &pay, "Gift", "2.00", "10.00", "2026-10-05", &l);
    // the budget, the month, the top list: 4.00 and 6.00 in euro, never 20.00 lei
    let line = |lines: &Value| lines.as_array().unwrap().iter().find(|c| c["categoryId"] == food.as_str()).unwrap().clone();
    let budget = ok(&vault, "stats.budget", json!({ "month": "2026-10" }));
    let food_line = line(&budget["categories"]);
    assert_eq!((food_line["actual"].as_str(), food_line["remaining"].as_str()), (Some("10.00"), Some("90.00")));
    let year = ok(&vault, "stats.year", json!({ "year": "2026" }));
    let october = &year[9];
    assert_eq!((october["month"].as_str(), october["expense"].as_str(), october["income"].as_str(), october["savings"].as_str()), (Some("2026-10"), Some("10.00"), Some("2.00"), Some("-8.00")));
    let top = ok(&vault, "stats.top", json!({ "month": "2026-10", "type": "EXPENSE" }));
    assert_eq!((top[0]["categoryId"].as_str(), top[0]["actual"].as_str()), (Some(food.as_str()), Some("10.00")));
    // the account moved by the native amounts, the main account by the amount in euro, and the list shows both of each
    assert_eq!((held(&vault, &l), held(&vault, "1")), ("990.00".into(), "-6.00".to_string()));
    let rows = ok(&vault, "transactions.list", json!({ "accountId": l }));
    let sums: Vec<(&str, &str)> = rows["transactions"].as_array().unwrap().iter().map(|t| (t["amount"].as_str().unwrap(), t["nativeAmount"].as_str().unwrap())).collect();
    assert_eq!(sums, [("2.00", "10.00"), ("4.00", "20.00")], "newest first");
    // a new rate moves no statistic and no budget
    ok(&vault, "accounts.update", json!({ "id": l, "rate": "2.00" }));
    assert_eq!(ok(&vault, "stats.budget", json!({ "month": "2026-10" })), budget);
}

// ---- transfers between currencies ---------------------------------------------------------------

#[test]
fn a_transfer_between_accounts_of_different_currencies_needs_what_arrives_and_each_side_moves_by_its_own_amount() {
    let (_dir, vault) = fresh();
    let euro = id_of(&ok(&vault, "accounts.create", json!({ "name": "Euro account", "kind": "CHECKING", "openingBalance": "1000.00", "openingDate": "2026-01-01" })));
    let (l, l2) = (lei(&vault, "Lei account", "0.00"), lei(&vault, "Lei savings", "0.00"));
    let usd = id_of(&ok(&vault, "accounts.create", json!({ "name": "Dollar account", "kind": "CHECKING", "currency": "USD", "rate": "1.10" })));
    let body = |from: &str, to: &str, amount: &str| json!({ "fromAccountId": from, "toAccountId": to, "amount": amount, "date": "2026-10-03" });
    // EUR to RON, RON to EUR, the main account (EUR) to RON, RON to USD and USD to RON: each needs it
    for (from, to) in [(&euro, &l), (&l, &euro), (&"1".to_string(), &l), (&l, &usd), (&usd, &l), (&euro, &usd)] {
        refuse(&vault, "transfers.create", body(from, to, "40.00"), INVALID);
    }
    // given, it is answered with the amount, and each side moves by its own: 40.00 euro leave, 200.00 lei arrive
    let made = transfer_across(&vault, &euro, &l, "40.00", "200.00", "2026-10-03");
    assert_eq!(keys(&made), TRANSFER_KEYS);
    assert_eq!((made["amount"].as_str(), made["toAmount"].as_str(), made["fromAccountId"].as_str(), made["toAccountId"].as_str()), (Some("40.00"), Some("200.00"), Some(euro.as_str()), Some(l.as_str())));
    assert_eq!((held(&vault, &euro), held(&vault, &l)), ("960.00".into(), "200.00".to_string()));
    // back: 100.00 lei leave, 19.00 euro arrive
    transfer_across(&vault, &l, &euro, "100.00", "19.00", "2026-10-04");
    assert_eq!((held(&vault, &euro), held(&vault, &l)), ("979.00".into(), "100.00".to_string()));
    // two foreign currencies; and the main account, in euro
    transfer_across(&vault, &l, &usd, "50.00", "11.00", "2026-10-05");
    assert_eq!((held(&vault, &l), held(&vault, &usd)), ("50.00".into(), "11.00".to_string()));
    transfer_across(&vault, "1", &l, "10.00", "52.00", "2026-10-06");
    assert_eq!((held(&vault, "1"), held(&vault, &l)), ("-10.00".into(), "102.00".to_string()));
    // each statement shows what that side did, in its own currency
    let into = ok(&vault, "accounts.show", json!({ "id": usd, "from": "2026-10-01", "to": "2026-10-31" }));
    assert_eq!((into["currency"].as_str(), into["lines"][0]["amount"].as_str(), into["lines"][0]["source"].as_str(), into["lines"][0]["other"].as_str()), (Some("USD"), Some("11.00"), Some("TRANSFER_IN"), Some("Lei account")));
    let out = ok(&vault, "accounts.show", json!({ "id": euro, "from": "2026-10-01", "to": "2026-10-31" }));
    assert_eq!(out["lines"].as_array().unwrap().iter().map(|l| (l["source"].as_str().unwrap(), l["amount"].as_str().unwrap())).collect::<Vec<_>>(), [("TRANSFER_OUT", "-40.00"), ("TRANSFER_IN", "19.00")]);
    // the same currency on both sides takes none: euro to euro, lei to lei, and a second amount equal to the first is no exception
    assert_eq!(ok(&vault, "transfers.create", body(&euro, "1", "5.00"))["toAmount"], Value::Null);
    assert_eq!(ok(&vault, "transfers.create", with(body(&l, &l2, "5.00"), "date", json!("2026-10-07")))["toAmount"], Value::Null);
    for (from, to) in [(euro.as_str(), "1"), (l.as_str(), l2.as_str()), (l2.as_str(), l.as_str())] {
        for arrives in ["5.00", "1.00", "0.01"] {
            refuse(&vault, "transfers.create", with(body(from, to, "5.00"), "toAmount", json!(arrives)), INVALID);
        }
    }
    // the list answers it, and it is null for a transfer of one currency
    let page = ok(&vault, "transfers.list", json!({ "accountId": l, "limit": 100 }));
    let seen: Vec<(Option<&str>, Option<&str>)> = page["transfers"].as_array().unwrap().iter().map(|t| (t["amount"].as_str(), t["toAmount"].as_str())).collect();
    assert_eq!(seen, [(Some("5.00"), None), (Some("10.00"), Some("52.00")), (Some("50.00"), Some("11.00")), (Some("100.00"), Some("19.00")), (Some("40.00"), Some("200.00"))], "newest first");
    // the sum of the balances over currencies is no number, and it is not zero-sum: each side moved by its own amount
    assert_eq!(ok(&vault, "transfers.list", json!({}))["total"], 6, "the refused ones made none");
}

#[test]
fn what_arrives_is_checked_as_an_amount_is_and_a_refused_one_makes_no_transfer() {
    let (_dir, vault) = fresh();
    let l = lei(&vault, "Lei account", "0.00");
    let body = |arrives: Value| json!({ "fromAccountId": "1", "toAccountId": l, "amount": "40.00", "toAmount": arrives, "date": "2026-10-03" });
    for good in ["0.01", "200.00", "9999999999.99"] {
        assert_eq!(ok(&vault, "transfers.create", body(json!(good)))["toAmount"], good);
    }
    let t = id_of(&transfer_across(&vault, "1", &l, "1.00", "5.00", "2026-10-03"));
    for bad in ["0.00", "12.5", "12", "12.", ".50", "12.500", "01.00", "-1.00", "+1.00", "1,50", "1e3", " 1.00", "1.00 ", "１２.５０", "10000000000.00", ""] {
        refuse(&vault, "transfers.create", body(json!(bad)), INVALID);
        refuse(&vault, "transfers.update", json!({ "id": t, "toAmount": bad }), INVALID);
    }
    for value in [json!(200), json!(12.5), json!(true), json!(null), json!(["1.00"])] {
        assert_eq!(refuse(&vault, "transfers.create", body(value.clone()), INVALID), "`toAmount` must be an amount as text with two decimals, like \"12.50\"", "{value}");
        refuse(&vault, "transfers.update", json!({ "id": t, "toAmount": value }), INVALID);
    }
    // what leaves is checked as ever, whatever arrives
    for amount in ["0.00", "-40.00", "40", "10000000000.00"] {
        refuse(&vault, "transfers.create", with(body(json!("200.00")), "amount", json!(amount)), INVALID);
        refuse(&vault, "transfers.update", json!({ "id": t, "amount": amount, "toAmount": "200.00" }), INVALID);
    }
    assert_eq!(ok(&vault, "transfers.list", json!({}))["total"], 4);
}

#[test]
fn a_transfer_is_changed_so_that_the_currencies_become_equal_and_different_again_as_the_note_says() {
    let (_dir, vault) = fresh();
    let (euro, euro2) = (account(&vault, "Euro account"), account(&vault, "Euro savings"));
    let (l1, l2) = (lei(&vault, "Lei one", "0.00"), lei(&vault, "Lei two", "0.00"));
    let t = id_of(&transfer_across(&vault, &euro, &l1, "40.00", "200.00", "2026-10-03"));
    let shown = || ok(&vault, "transfers.list", json!({ "limit": 100 }))["transfers"][0].clone();
    assert_eq!((held(&vault, &euro), held(&vault, &l1)), ("-40.00".into(), "200.00".to_string()));

    // the amount that leaves changes alone: what arrives is kept
    let more = ok(&vault, "transfers.update", json!({ "id": t, "amount": "50.00" }));
    assert_eq!((more["amount"].as_str(), more["toAmount"].as_str()), (Some("50.00"), Some("200.00")));
    // what arrives changes alone: the other side does not move
    let less = ok(&vault, "transfers.update", json!({ "id": t, "toAmount": "250.00" }));
    assert_eq!((less["amount"].as_str(), less["toAmount"].as_str()), (Some("50.00"), Some("250.00")));
    assert_eq!((held(&vault, &euro), held(&vault, &l1)), ("-50.00".into(), "250.00".to_string()));
    // and the same one again writes nothing
    let before = dump(&vault);
    assert_eq!(ok(&vault, "transfers.update", json!({ "id": t, "toAmount": "250.00" })), less);
    assert_eq!(dump(&vault), before);

    // a side changes so that both are in euro: what arrives is cleared, and one given with it is refused
    refuse(&vault, "transfers.update", json!({ "id": t, "toAccountId": euro2, "toAmount": "250.00" }), INVALID);
    refuse(&vault, "transfers.update", json!({ "id": t, "toAccountId": euro2, "toAmount": "50.00" }), INVALID);
    let same = ok(&vault, "transfers.update", json!({ "id": t, "toAccountId": euro2 }));
    assert_eq!((same["toAccountId"].as_str(), same["amount"].as_str(), same["toAmount"].clone()), (Some(euro2.as_str()), Some("50.00"), Value::Null));
    assert_eq!(shown()["toAmount"], Value::Null);
    assert_eq!((held(&vault, &euro), held(&vault, &euro2), held(&vault, &l1)), ("-50.00".into(), "50.00".into(), "0.00".to_string()), "the euro that left is the euro that arrived");
    // a patch that names a toAmount for a transfer of one currency is refused, whatever it says
    for arrives in ["50.00", "250.00", "0.01"] {
        refuse(&vault, "transfers.update", json!({ "id": t, "toAmount": arrives }), INVALID);
    }
    // and different again: what arrives is needed, as the transfer has none now
    refuse(&vault, "transfers.update", json!({ "id": t, "toAccountId": l1 }), INVALID);
    refuse(&vault, "transfers.update", json!({ "id": t, "toAccountId": l1, "amount": "60.00" }), INVALID);
    let different = ok(&vault, "transfers.update", json!({ "id": t, "toAccountId": l1, "toAmount": "180.00" }));
    assert_eq!((different["toAccountId"].as_str(), different["toAmount"].as_str()), (Some(l1.as_str()), Some("180.00")));
    assert_eq!((held(&vault, &euro2), held(&vault, &l1)), ("0.00".into(), "180.00".to_string()));
    // one side changes and they still differ: it is kept unless a new one is given
    let kept = ok(&vault, "transfers.update", json!({ "id": t, "toAccountId": l2 }));
    assert_eq!((kept["toAccountId"].as_str(), kept["toAmount"].as_str()), (Some(l2.as_str()), Some("180.00")));
    let from_main = ok(&vault, "transfers.update", json!({ "id": t, "fromAccountId": "1" }));
    assert_eq!((from_main["fromAccountId"].as_str(), from_main["toAmount"].as_str()), (Some("1"), Some("180.00")), "the main account is in euro as well");
    // both sides in lei: cleared; back to euro to lei: needed
    let lei_to_lei = ok(&vault, "transfers.update", json!({ "id": t, "fromAccountId": l1 }));
    assert_eq!((lei_to_lei["fromAccountId"].as_str(), lei_to_lei["toAccountId"].as_str(), lei_to_lei["toAmount"].clone()), (Some(l1.as_str()), Some(l2.as_str()), Value::Null));
    refuse(&vault, "transfers.update", json!({ "id": t, "toAmount": "5.00" }), INVALID);
    refuse(&vault, "transfers.update", json!({ "id": t, "fromAccountId": euro }), INVALID);
    let again = ok(&vault, "transfers.update", json!({ "id": t, "fromAccountId": euro, "toAmount": "175.00", "amount": "35.00" }));
    assert_eq!((again["amount"].as_str(), again["toAmount"].as_str()), (Some("35.00"), Some("175.00")));
    // both sides swapped: each side now leaves and arrives in the other currency, and what arrives is kept
    let swapped = ok(&vault, "transfers.update", json!({ "id": t, "fromAccountId": l2, "toAccountId": euro }));
    assert_eq!((swapped["fromAccountId"].as_str(), swapped["toAccountId"].as_str(), swapped["amount"].as_str(), swapped["toAmount"].as_str()), (Some(l2.as_str()), Some(euro.as_str()), Some("35.00"), Some("175.00")));
    assert_eq!(ok(&vault, "transfers.list", json!({}))["total"], 1);
}

#[test]
fn a_transfer_between_currencies_can_be_edited_while_the_foreign_side_is_archived_and_deleted_with_its_balances_back() {
    let (_dir, vault) = fresh();
    let (euro, l) = (account(&vault, "Euro account"), lei(&vault, "Lei account", "0.00"));
    let t = id_of(&transfer_across(&vault, &euro, &l, "40.00", "200.00", "2026-10-03"));
    ok(&vault, "accounts.archive", json!({ "id": l }));
    // the side that does not change may be archived: what arrives may change, and so may what leaves
    assert_eq!(ok(&vault, "transfers.update", json!({ "id": t, "toAmount": "210.00" }))["toAmount"], "210.00");
    assert_eq!(ok(&vault, "transfers.update", json!({ "id": t, "toAccountId": l, "amount": "41.00" }))["amount"], "41.00", "naming it again is no change");
    assert_eq!((held(&vault, &euro), held(&vault, &l)), ("-41.00".into(), "210.00".to_string()));
    // but no side moves into an archived account, and a transfer between currencies cannot be made with one
    let l2 = lei(&vault, "Lei two", "0.00");
    ok(&vault, "accounts.archive", json!({ "id": l2 }));
    refuse(&vault, "transfers.update", json!({ "id": t, "toAccountId": l2 }), INVALID);
    refuse(&vault, "transfers.create", json!({ "fromAccountId": euro, "toAccountId": l, "amount": "1.00", "toAmount": "5.00", "date": "2026-10-03" }), INVALID);
    // deleting it brings both balances back, each by its own amount
    assert_eq!(ok(&vault, "transfers.delete", json!({ "id": t })), Value::Null);
    assert_eq!((held(&vault, &euro), held(&vault, &l)), ("0.00".into(), "0.00".to_string()));
}

// ---- what stays in the app's currency ---------------------------------------------------------

#[test]
fn a_recurring_item_a_ledger_entry_and_a_settlement_are_refused_for_an_account_of_another_currency_and_nothing_changes() {
    let (_dir, vault) = fresh();
    let food = category(&vault, "EXPENSE", "Food");
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let (l, euro) = (lei(&vault, "Lei account", "100.00"), account(&vault, "Euro account"));
    let item = |account: &str| json!({ "name": "Gym", "amount": "20.00", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1, "accountId": account });
    let entry = |kind: &str, account: &str| json!({ "personId": anna, "kind": kind, "amount": "9.00", "date": "2026-10-02", "accountId": account });
    // each works in an account in the app's currency: what is refused below is refused for the currency
    let rec = id_of(&ok(&vault, "recurring.create", item(&euro)));
    let led = id_of(&ok(&vault, "ledger.add", entry("LENT", &euro)));
    assert_eq!(ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "BORROWED", "amount": "3.00", "date": "2026-10-02", "expenseName": "Pizza", "expenseCategoryId": food, "accountId": euro }))["accountId"], euro.as_str());

    refuse(&vault, "recurring.create", item(&l), INVALID);
    refuse(&vault, "recurring.update", json!({ "id": rec, "accountId": l }), INVALID);
    refuse(&vault, "recurring.update", json!({ "id": rec, "accountId": l, "name": "Gym 2", "amount": "5.00" }), INVALID);
    for kind in ["LENT", "BORROWED", "REPAID_TO_ME", "REPAID_BY_ME"] {
        refuse(&vault, "ledger.add", entry(kind, &l), INVALID);
    }
    // a borrowing that adds an expense: neither is made
    refuse(&vault, "ledger.add", json!({ "personId": anna, "kind": "BORROWED", "amount": "3.00", "date": "2026-10-02", "expenseName": "Taxi", "expenseCategoryId": food, "accountId": l }), INVALID);
    assert_eq!(ok(&vault, "transactions.list", json!({ "q": "Taxi" }))["total"], 0);
    refuse(&vault, "ledger.update", json!({ "id": led, "accountId": l }), INVALID);
    refuse(&vault, "ledger.update", json!({ "id": led, "accountId": l, "amount": "1.00", "note": "x" }), INVALID);
    refuse(&vault, "people.settle", json!({ "id": anna, "accountId": l }), INVALID);
    refuse(&vault, "people.settle", json!({ "id": anna, "amount": "1.00", "date": "2026-10-05", "note": "x", "accountId": l }), INVALID);
    // what was there stays where it was, and the account of the others still takes them
    assert_eq!(find(&vault, &l)["entries"], 0);
    assert_eq!(ok(&vault, "recurring.update", json!({ "id": rec, "name": "Gym 2" }))["accountId"], euro.as_str());
    assert_eq!(ok(&vault, "ledger.update", json!({ "id": led, "amount": "10.00" }))["accountId"], euro.as_str());
    assert_eq!(ok(&vault, "people.settle", json!({ "id": anna, "amount": "1.00", "accountId": euro }))["entry"]["accountId"], euro.as_str());
    // an account that was foreign and is not any more takes them
    assert_eq!(find(&vault, &l)["entries"], 0);
    ok(&vault, "accounts.update", json!({ "id": l, "currency": "EUR" }));
    assert_eq!(ok(&vault, "recurring.create", item(&l))["accountId"], l.as_str());
}

#[test]
fn the_apps_currency_cannot_change_while_an_account_is_in_another_currency_archived_or_not_and_the_routes_follow_it_after() {
    let (_dir, vault) = fresh();
    let l = lei(&vault, "Lei account", "100.00");
    // the person changes it in the app: there is no route for it, and `settings.set` does not take it
    assert_eq!(run(&vault, "settings.set", json!({ "currency": "USD" })).unwrap_err().message, "unknown argument `currency`");
    let change = |code: &str, rate: Option<&str>| vault.with_conn(|c| crate::currency::change(c, code, rate, "2026-10-14"));
    let refused_change = |when: &str| {
        let before = dump(&vault);
        for (code, rate) in [("USD", Some("1.10")), ("USD", None), ("RON", None), ("RON", Some("5.00")), ("EUR", None)] {
            assert_eq!(change(code, rate), Err(crate::vault::AppError::InvalidInput), "{when}: {code} {rate:?}");
            assert_eq!(dump(&vault), before, "{when}: {code} {rate:?}");
        }
        assert_eq!(ok(&vault, "settings.get", json!({}))["currency"], "EUR");
    };
    refused_change("with the account");
    ok(&vault, "accounts.archive", json!({ "id": l }));
    refused_change("with the account archived");
    // deleted, the account is gone and the change goes through: the main account follows it, and what is in the app's currency is converted
    assert_eq!(ok(&vault, "accounts.delete", json!({ "id": l })), Value::Null);
    change("USD", Some("2.00")).unwrap();
    assert_eq!(ok(&vault, "settings.get", json!({}))["currency"], "USD");
    assert_eq!(currency_of(&find(&vault, "1")), ("USD", false, None, "0.00"));
    // the routes follow it: the app's currency is no other currency, and the old one is
    assert_eq!(currency_of(&ok(&vault, "accounts.create", json!({ "name": "Dollars", "kind": "CASH", "currency": "usd" }))), ("USD", false, None, "0.00"));
    refuse(&vault, "accounts.create", json!({ "name": "Euros", "kind": "CASH", "currency": "EUR" }), INVALID);
    let euros = ok(&vault, "accounts.create", json!({ "name": "Euros", "kind": "CASH", "currency": "eur", "rate": "0.50" }));
    assert_eq!(currency_of(&euros), ("EUR", true, Some("0.50"), "0.00"));
    // and it holds the app's currency still while that account exists
    assert_eq!(change("GBP", None), Err(crate::vault::AppError::InvalidInput));
    ok(&vault, "accounts.delete", json!({ "id": id_of(&euros) }));
    assert!(change("GBP", None).is_ok());
}

#[test]
fn net_worth_turns_each_account_of_another_currency_into_the_apps_at_its_own_rate_and_rounds_away_from_zero() {
    let (_dir, vault) = fresh();
    ok(&vault, "accounts.update", json!({ "id": "1", "openingBalance": "100.00", "openingDate": "2026-01-01" }));
    let make = |name: &str, code: &str, rate: &str, opening: &str| id_of(&ok(&vault, "accounts.create", json!({ "name": name, "kind": "SAVINGS", "currency": code, "rate": rate, "openingBalance": opening, "openingDate": "2026-01-01" })));
    let (at_five, at_four) = (make("Lei at five", "RON", "5.00", "1000.00"), make("Lei at four", "RON", "4.00", "1000.00"));
    let (dollars, pounds, francs, owed, card) = (make("Dollars", "USD", "1.10", "110.00"), make("Pounds", "GBP", "3.00", "2.00"), make("Francs", "CHF", "2.00", "0.01"), make("Francs owed", "CHF", "2.00", "-0.01"), make("Card", "RON", "3.00", "-10.00"));
    let hidden = make("Hidden", "RON", "5.00", "5000.00");
    ok(&vault, "accounts.update", json!({ "id": hidden, "includeInNetWorth": false }));
    // each is worth its balance divided by its rate, to the cent, half away from zero, the sign kept
    for (id, worth_in_euro) in [(&at_five, "200.00"), (&at_four, "250.00"), (&dollars, "100.00"), (&pounds, "0.67"), (&francs, "0.01"), (&owed, "-0.01"), (&card, "-3.33"), (&hidden, "1000.00")] {
        assert_eq!(find(&vault, id)["balanceInMain"], worth_in_euro, "{}", find(&vault, id)["name"]);
    }
    // 100.00 and 200.00 and 250.00 and 100.00 and 0.67 and 0.01 and -0.01 and -3.33 make 647.34; the one left out is not in it
    assert_eq!(worth(&vault), ("647.34".to_string(), "0.00".to_string(), "647.34".to_string()));
    assert_eq!(ok(&vault, "accounts.summary", json!({}))["accounts"], 9);
    // friends: what is lent leaves the main account and is owed to you, so net worth stays
    let anna = id_of(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "20.00", "date": "2026-10-02" }));
    assert_eq!(worth(&vault), ("627.34".to_string(), "20.00".to_string(), "647.34".to_string()));
    // an archived account still counts
    ok(&vault, "accounts.archive", json!({ "id": dollars }));
    assert_eq!(worth(&vault).0, "627.34");
    // a new rate moves net worth by what it moves the account: 1000.00 lei at 2.50 is 400.00, not 200.00
    ok(&vault, "accounts.update", json!({ "id": at_five, "rate": "2.50" }));
    assert_eq!(worth(&vault).0, "827.34");
    // put back, it counts at its own rate; another account, left out, is out
    ok(&vault, "accounts.update", json!({ "id": hidden, "includeInNetWorth": true }));
    assert_eq!(worth(&vault).0, "1827.34");
    // every account left out: nothing in the accounts, and net worth is what friends owe
    for a in listed(&vault) {
        if a["archived"] == false {
            ok(&vault, "accounts.update", json!({ "id": a["id"], "includeInNetWorth": false }));
        } else {
            ok(&vault, "accounts.unarchive", json!({ "id": a["id"] }));
            ok(&vault, "accounts.update", json!({ "id": a["id"], "includeInNetWorth": false }));
        }
    }
    assert_eq!(worth(&vault), ("0.00".to_string(), "20.00".to_string(), "20.00".to_string()));
}

#[test]
fn the_statement_the_breakdown_and_the_history_of_an_account_in_another_currency_are_in_its_currency() {
    let (_dir, vault) = fresh();
    let (food, pay) = (category(&vault, "EXPENSE", "Food"), category(&vault, "INCOME", "Pay"));
    let (l, euro) = (lei(&vault, "Lei account", "1000.00"), account(&vault, "Euro account"));
    spend_native(&vault, &food, "Boots", "4.00", "20.00", "2026-10-03", &l);
    spend_native(&vault, &pay, "Gift", "2.00", "10.00", "2026-10-05", &l);
    transfer_across(&vault, &euro, &l, "40.00", "200.00", "2026-10-06");

    let shown = ok(&vault, "accounts.show", json!({ "id": l, "from": "2026-10-01", "to": "2026-10-31" }));
    assert_eq!(shown["currency"], "RON");
    let lines: Vec<(&str, &str, &str)> = shown["lines"].as_array().unwrap().iter().map(|x| (x["source"].as_str().unwrap(), x["amount"].as_str().unwrap(), x["balance"].as_str().unwrap())).collect();
    assert_eq!(lines, [("TRANSACTION", "-20.00", "980.00"), ("TRANSACTION", "10.00", "990.00"), ("TRANSFER_IN", "200.00", "1190.00")], "what really moved the account, in lei");
    assert_eq!((shown["balanceBefore"].as_str(), shown["balanceAfter"].as_str()), (Some("1000.00"), Some("1190.00")));
    assert_eq!((shown["account"]["changeThisMonth"].as_str(), shown["account"]["lastActivity"].as_str(), shown["account"]["balanceInMain"].as_str()), (Some("190.00"), Some("2026-10-06"), Some("238.00")));

    let breakdown = ok(&vault, "accounts.breakdown", json!({ "id": l, "from": "2026-10-01", "to": "2026-10-31" }));
    assert_eq!(
        breakdown,
        json!({
            "currency": "RON", "from": "2026-10-01", "to": "2026-10-31", "expenseTotal": "20.00", "incomeTotal": "10.00",
            "expenses": [{ "categoryId": food, "name": "Food", "amount": "20.00" }],
            "income": [{ "categoryId": pay, "name": "Pay", "amount": "10.00" }],
        }),
        "the native amounts, not the 4.00 and 2.00 that budgets count"
    );
    let history = ok(&vault, "accounts.history", json!({ "id": l, "year": "2026" }));
    assert_eq!((history[0]["balance"].as_str(), history[9]["balance"].as_str()), (Some("1000.00"), Some("1190.00")));
    // an account in the app's currency says the app's
    let (other, other_breakdown) = (ok(&vault, "accounts.show", json!({ "id": euro })), ok(&vault, "accounts.breakdown", json!({ "id": euro })));
    assert_eq!((other["currency"].as_str(), other_breakdown["currency"].as_str()), (Some("EUR"), Some("EUR")));
}

// ---- over HTTP ----------------------------------------------------------------------------------

#[test]
fn over_http_a_write_token_makes_accounts_and_rows_in_another_currency_and_a_read_token_reads_them_and_cannot_change_any() {
    let h = http();
    let port = h.server.port();
    let post = |token: &str, route: &str, body: Value| {
        let reply = call(port, Some(token), route, &body.to_string());
        assert_eq!(reply.status, 200, "{route}: {}", reply.body);
        assert_eq!(reply.json()["ok"], true);
        reply.data()
    };
    let food = ok(&h.vault, "categories.list", json!({ "type": "EXPENSE" }))[0]["id"].as_str().unwrap().to_string();
    let made = post(&h.write, "accounts.create", json!({ "name": "Lei wallet", "kind": "CASH", "currency": "ron", "rate": "5.00", "openingBalance": "100.00", "openingDate": "2026-01-01" }));
    assert_eq!(keys(&made), ACCOUNT_KEYS);
    assert_eq!(currency_of(&made), ("RON", true, Some("5.00"), "20.00"));
    let id = id_of(&made);
    let spent = post(&h.write, "transactions.create", json!({ "name": "Coffee", "amount": "1.00", "paymentDate": "2026-10-10", "categoryId": food, "accountId": id, "nativeAmount": "5.00" }));
    assert_eq!((spent["amount"].as_str(), spent["nativeAmount"].as_str()), (Some("1.00"), Some("5.00")));
    let moved = post(&h.write, "transfers.create", json!({ "fromAccountId": "1", "toAccountId": id, "amount": "10.00", "toAmount": "52.00", "date": "2026-10-11" }));
    assert_eq!(keys(&moved), TRANSFER_KEYS);
    assert_eq!(post(&h.write, "accounts.update", json!({ "id": id, "balance": "500.00" }))["openingBalance"], "453.00");
    assert_eq!(currency_of(&post(&h.write, "accounts.update", json!({ "id": id, "rate": "4.00" }))), ("RON", true, Some("4.00"), "125.00"));
    // a read token sees all of it
    let list = post(&h.read, "accounts.list", json!({}));
    let seen = list.as_array().unwrap().iter().find(|a| a["id"] == id.as_str()).unwrap().clone();
    assert_eq!((currency_of(&seen), seen["balance"].as_str()), (("RON", true, Some("4.00"), "125.00"), Some("500.00")));
    assert_eq!(post(&h.read, "accounts.show", json!({ "id": id }))["currency"], "RON");
    assert_eq!(post(&h.read, "transactions.list", json!({ "accountId": id }))["transactions"][0]["nativeAmount"], "5.00");
    assert_eq!(post(&h.read, "transfers.list", json!({ "accountId": id }))["transfers"][0]["toAmount"], "52.00");

    // a read token cannot write any of it, and nothing changes
    let before = dump(&h.vault);
    for (route, body) in [
        ("accounts.create", json!({ "name": "Another", "kind": "CASH", "currency": "USD", "rate": "1.10" })),
        ("accounts.update", json!({ "id": id, "rate": "3.00" })),
        ("accounts.update", json!({ "id": id, "currency": "EUR" })),
        ("accounts.update", json!({ "id": id, "balance": "1.00" })),
        ("transactions.create", json!({ "name": "Tea", "amount": "1.00", "paymentDate": "2026-10-10", "categoryId": food, "accountId": id, "nativeAmount": "5.00" })),
        ("transactions.update", json!({ "id": id_of(&spent), "nativeAmount": "6.00" })),
        ("transfers.create", json!({ "fromAccountId": "1", "toAccountId": id, "amount": "1.00", "toAmount": "5.00", "date": "2026-10-11" })),
        ("transfers.update", json!({ "id": id_of(&moved), "toAmount": "60.00" })),
    ] {
        let refused = call(port, Some(&h.read), route, &body.to_string());
        assert_eq!((refused.status, refused.code().as_str()), (403, "forbidden"), "{route} {body}");
    }
    assert_eq!(dump(&h.vault), before, "a read token changed something");

    // the refusals over the wire: 400, and the reason for a wrong kind of value, and nothing of the rules underneath
    for (route, body, message) in [
        ("transactions.create", json!({ "name": "Tea", "amount": "1.00", "paymentDate": "2026-10-10", "categoryId": food, "accountId": id }), "invalid input"),
        ("transactions.create", json!({ "name": "Tea", "amount": "1.00", "paymentDate": "2026-10-10", "categoryId": food, "nativeAmount": "5.00" }), "invalid input"),
        ("transactions.create", json!({ "name": "Tea", "amount": "1.00", "paymentDate": "2026-10-10", "categoryId": food, "accountId": id, "nativeAmount": 5 }), "`nativeAmount` must be an amount as text with two decimals, like \"12.50\""),
        ("transfers.create", json!({ "fromAccountId": "1", "toAccountId": id, "amount": "1.00", "date": "2026-10-11" }), "invalid input"),
        ("transfers.create", json!({ "fromAccountId": "1", "toAccountId": "2", "amount": "1.00", "toAmount": "1.00", "date": "2026-10-11" }), "invalid input"),
        ("transfers.update", json!({ "id": id_of(&moved), "toAmount": "5" }), "`toAmount` must be an amount as text with two decimals, like \"12.50\""),
        ("accounts.update", json!({ "id": id, "balance": "5.00", "openingBalance": "5.00" }), "invalid input"),
        ("accounts.update", json!({ "id": id, "rate": "0" }), "invalid input"),
        ("accounts.update", json!({ "id": id, "currency": "RO" }), "invalid input"),
        ("accounts.update", json!({ "id": "1", "currency": "RON", "rate": "5.00" }), "invalid input"),
    ] {
        let reply = call(port, Some(&h.write), route, &body.to_string());
        assert_eq!((reply.status, reply.code().as_str(), reply.message().as_str()), (400, "invalid_input", message), "{route} {body}");
    }
    assert_eq!(dump(&h.vault), before, "a refused request changed something");
    assert_eq!(call(port, Some(&h.read), "status", "").status, 200);
    assert_eq!(h.server.panics(), 0);
}

// ---- loans: the rate, the installment and the payment -----------------------------------------

#[test]
fn a_payment_of_a_loan_is_a_transfer_and_an_expense_and_a_wrong_one_changes_nothing() {
    let (_dir, vault) = fresh();
    let interest = ok(&vault, "categories.create", json!({ "name": "Interest", "type": "EXPENSE" }))["id"].as_str().unwrap().to_string();
    let loan = id_of(&ok(
        &vault,
        "accounts.create",
        json!({ "name": "Car loan", "kind": "LOAN", "openingBalance": "-9000.00", "openingDate": "2026-01-01", "interestRate": "6.5", "interestPeriod": "YEARLY", "installment": "950.00" }),
    ));
    let before = find(&vault, &loan);
    assert_eq!(keys(&before), ACCOUNT_KEYS);
    assert_eq!((before["interestRate"].as_str(), before["interestPeriod"].as_str(), before["installment"].as_str(), before["returnPerMonth"].clone()), (Some("6.5"), Some("YEARLY"), Some("950.00"), Value::Null));
    assert_eq!((before["liability"]["owed"].as_str(), before["liability"]["state"].as_str(), before["liability"]["interestPerMonth"].as_str()), (Some("9000.00"), Some("OWING"), Some("48.75")));
    assert!(before["liability"]["payoff"]["months"].as_u64().is_some() && before["liability"]["payoff"]["month"].as_str().is_some(), "{before}");

    // 950.00 paid from the main account, 585.00 of it interest: the rest lowers the debt, the interest is an expense
    let paid = ok(&vault, "accounts.pay", json!({ "id": loan, "fromAccountId": "1", "amount": "950.00", "interest": "585.00", "categoryId": interest, "date": "2026-10-05", "note": "October" }));
    assert_eq!(keys(&paid), ["transaction", "transfer"]);
    assert_eq!(keys(&paid["transfer"]), TRANSFER_KEYS);
    assert_eq!(
        (paid["transfer"]["fromAccountId"].as_str(), paid["transfer"]["toAccountId"].as_str(), paid["transfer"]["amount"].as_str(), paid["transfer"]["date"].as_str(), paid["transfer"]["note"].as_str()),
        (Some("1"), Some(loan.as_str()), Some("365.00"), Some("2026-10-05"), Some("October"))
    );
    assert_eq!(
        (paid["transaction"]["name"].as_str(), paid["transaction"]["amount"].as_str(), paid["transaction"]["categoryId"].as_str(), paid["transaction"]["accountId"].as_str()),
        (Some("Car loan"), Some("585.00"), Some(interest.as_str()), Some("1"))
    );
    assert_eq!((find(&vault, &loan)["liability"]["owed"].as_str(), held(&vault, "1").as_str()), (Some("8635.00"), "-950.00"));

    // a payment that is all principal makes no expense, and one that is all interest no transfer
    let plain = ok(&vault, "accounts.pay", json!({ "id": loan, "fromAccountId": "1", "amount": "100.00" }));
    assert_eq!((plain["transaction"].clone(), plain["transfer"]["amount"].as_str()), (Value::Null, Some("100.00")));
    let only = ok(&vault, "accounts.pay", json!({ "id": loan, "fromAccountId": "1", "amount": "20.00", "interest": "20.00", "categoryId": interest }));
    assert_eq!((only["transfer"].clone(), only["transaction"]["amount"].as_str()), (Value::Null, Some("20.00")));

    // refused as invalid_input, nothing written: an account that is not a liability, and interest above the payment
    refuse(&vault, "accounts.pay", json!({ "id": "1", "fromAccountId": &loan, "amount": "10.00" }), INVALID);
    refuse(&vault, "accounts.pay", json!({ "id": loan, "fromAccountId": "1", "amount": "10.00", "interest": "10.01", "categoryId": interest }), INVALID);
}
