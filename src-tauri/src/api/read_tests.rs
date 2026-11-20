//! The read routes against a vault with the demo data. Each route is compared with the function the
//! desktop screens call, so "the CLI shows what the app shows" is a test, not a hope.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};

use super::routes::{self, Ctx, Route};
use super::*;
use crate::keys::memory::MemoryStore;
use crate::tokens::Scope;
use crate::vault::Settings;
use crate::{backup, recurring, seed};

pub(super) const TODAY: &str = "2026-10-14";

const READ_ROUTES: &[&str] = &[
    "today",
    "settings.get",
    "categories.list",
    "transactions.list",
    "budgets.overview",
    "budgets.history",
    "recurring.list",
    "recurring.dues",
    "stats.budget",
    "stats.trend",
    "stats.year",
    "stats.top",
    "stats.commitments",
    "stats.planned",
    "export.preview",
    "backup.schedule",
    "documents.list",
    "documents.show",
    "people.list",
    "people.show",
    "debts.summary",
    "splits.preview",
    "accounts.list",
    "accounts.show",
    "accounts.history",
    "accounts.breakdown",
    "accounts.summary",
    "transfers.list",
];

pub(super) fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}

pub(super) fn noon() -> DateTime<Utc> {
    at("2026-10-14T12:00:00Z")
}

fn settings() -> Settings {
    Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() }
}

/// A vault with sixteen months of demo data ending on `TODAY`.
pub(super) fn seeded() -> (tempfile::TempDir, Vault) {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::new(dir.path().to_path_buf(), Box::new(MemoryStore::default()));
    vault.create(&settings(), None).unwrap();
    vault.with_conn(|c| seed::run_for(c, TODAY).map_err(|_| AppError::Internal)).unwrap();
    (dir, vault)
}

pub(super) fn route(name: &str) -> &'static Route {
    routes::ALL.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no route {name}"))
}

/// What the server does after it has found the route and checked the token: check the arguments, run.
pub(super) fn run_at(vault: &Vault, name: &str, args: Value, now: DateTime<Utc>) -> Result<Value, ApiError> {
    let route = route(name);
    let args = args.as_object().unwrap().clone();
    route.check(&args)?;
    let grant = tokens::Grant { id: 1, scope: Scope::Read };
    (route.run)(&Ctx { vault, grant, routes: routes::ALL, now }, args)
}

pub(super) fn run(vault: &Vault, name: &str, args: Value) -> Result<Value, ApiError> {
    run_at(vault, name, args, noon())
}

pub(super) fn ok(vault: &Vault, name: &str, args: Value) -> Value {
    run(vault, name, args.clone()).unwrap_or_else(|e| panic!("{name} {args}: {e:?}"))
}

/// What the screens would get from the same function.
pub(super) fn expect<T: Serialize>(vault: &Vault, f: impl FnOnce(&mut Connection) -> Result<T, AppError>) -> Value {
    vault.with_conn(|c| Ok(serde_json::to_value(f(c)?).unwrap())).unwrap()
}

pub(super) fn refused(vault: &Vault, name: &str, args: Value) -> (u16, String) {
    let e = run(vault, name, args.clone()).expect_err(&format!("{name} {args} was accepted"));
    (e.status, e.code.to_string())
}

pub(super) fn first_expense(vault: &Vault) -> Value {
    ok(vault, "categories.list", json!({ "type": "EXPENSE" }))[0].clone()
}

#[test]
fn today_follows_the_time_zone_set_in_the_app_not_the_computers() {
    let (_dir, vault) = seeded();
    let mut s = settings();
    s.timezone = "Pacific/Auckland".into();
    vault.save_settings(&s).unwrap();
    // 20:00 UTC on the 14th is already the 15th in Auckland (UTC+13 in October).
    let now = at("2026-10-14T20:00:00Z");
    assert_eq!(run_at(&vault, "today", json!({}), now).unwrap(), json!({ "date": "2026-10-15", "month": "2026-10" }));
    // And a month end: 23:30 UTC on 31 October is 1 November there.
    let month_end = at("2026-10-31T23:30:00Z");
    assert_eq!(run_at(&vault, "today", json!({}), month_end).unwrap(), json!({ "date": "2026-11-01", "month": "2026-11" }));

    // "Overdue" uses the same date.
    let dues = run_at(&vault, "recurring.dues", json!({}), month_end).unwrap();
    assert_eq!(dues, expect(&vault, |c| recurring::dues(c, "2026-11-01", None)));
}

#[test]
fn values_that_are_not_valid_are_refused_with_a_400_or_404() {
    let (_dir, vault) = seeded();
    let id = first_expense(&vault)["id"].as_str().unwrap().to_string();
    let invalid = (400, "invalid_input".to_string());
    for (name, args) in [
        ("budgets.overview", json!({ "month": "2026-13" })),
        ("budgets.overview", json!({ "month": "2026-00" })),
        ("stats.budget", json!({ "month": "0000-00" })),
        ("stats.planned", json!({ "month": "2026-99" })),
        ("transactions.list", json!({ "from": "2026-02-30" })),
        ("transactions.list", json!({ "to": "2026-13-01" })),
        ("transactions.list", json!({ "limit": 0 })),
        ("transactions.list", json!({ "limit": 101 })),
        ("transactions.list", json!({ "limit": 4294967296u64 })),
        ("transactions.list", json!({ "limit": -1 })),
        ("transactions.list", json!({ "offset": -1 })),
        ("transactions.list", json!({ "q": "x".repeat(256) })),
        ("recurring.dues", json!({ "withinDays": 367 })),
        ("recurring.dues", json!({ "withinDays": -1 })),
        ("stats.top", json!({ "month": "2026-09", "type": "EXPENSE", "limit": 0 })),
        ("stats.top", json!({ "month": "2026-09", "type": "EXPENSE", "limit": 51 })),
        ("stats.trend", json!({ "categoryId": id, "from": "2026-10", "to": "2026-01" })),
        ("stats.trend", json!({ "categoryId": id, "from": "2000-01", "to": "2026-10" })),
    ] {
        assert_eq!(refused(&vault, name, args.clone()), invalid, "{name} {args}");
    }
    assert_eq!(refused(&vault, "stats.trend", json!({ "categoryId": "999999", "from": "2026-01", "to": "2026-10" })), (404, "not_found".into()));
}

#[test]
fn the_shape_of_each_kind_of_argument_is_checked_before_the_route_runs() {
    let (_dir, vault) = seeded();
    let invalid = (400, "invalid_input".to_string());
    for month in ["2026-1", "2026-100", "October", "2026/10", "202610", "２０２６-10", "2026-10 ", "2026-1\n", "", "2026-10-14"] {
        assert_eq!(refused(&vault, "budgets.overview", json!({ "month": month })), invalid, "month {month:?}");
    }
    for date in ["2026-10-1", "2026-1-01", "10-14-2026", "2026-10-14T00:00:00Z", "2026-10-14 ", "２０２６-10-14", "2026-10", ""] {
        assert_eq!(refused(&vault, "transactions.list", json!({ "from": date })), invalid, "date {date:?}");
    }
    for year in ["26", "20266", "abcd", "٢٠٢٦", "2026-", "", " 2026"] {
        assert_eq!(refused(&vault, "stats.year", json!({ "year": year })), invalid, "year {year:?}");
    }
    for id in ["0", "01", "-1", "1.5", "abc", "", " 1", "1 ", &"1".repeat(19), "１"] {
        assert_eq!(refused(&vault, "transactions.list", json!({ "categoryId": id })), invalid, "id {id:?}");
    }
    assert_eq!(refused(&vault, "categories.list", json!({ "type": "income" })), invalid);
    // the kind of a transaction list is the same two words, exact
    for kind in ["income", "Expense", "BOTH", "", "INCOME "] {
        assert_eq!(refused(&vault, "transactions.list", json!({ "kind": kind })), invalid, "kind {kind:?}");
    }
    assert_eq!(refused(&vault, "transactions.list", json!({ "kind": 1 })), invalid);
    assert_eq!(refused(&vault, "categories.list", json!({ "type": "BOTH" })), invalid);
    assert_eq!(refused(&vault, "stats.top", json!({ "month": "2026-09" })), invalid, "type is required");
    assert_eq!(refused(&vault, "stats.trend", json!({ "from": "2026-01", "to": "2026-10" })), invalid, "categoryId is required");
    // A number where a string belongs, and the reverse.
    assert_eq!(refused(&vault, "budgets.overview", json!({ "month": 202610 })), invalid);
    assert_eq!(refused(&vault, "transactions.list", json!({ "limit": "10" })), invalid);
    assert_eq!(refused(&vault, "transactions.list", json!({ "categoryId": 5 })), invalid);
    // Arguments the route does not have.
    assert_eq!(refused(&vault, "settings.get", json!({ "currency": "USD" })), invalid);
    assert_eq!(refused(&vault, "transactions.list", json!({ "category": "5" })), invalid);
    // Well-formed values pass the shape check; whether they are real is the route's call.
    for (name, args) in [("budgets.overview", json!({ "month": "2026-10" })), ("stats.year", json!({ "year": "2026" })), ("transactions.list", json!({ "from": "2026-10-01", "categoryId": "12" }))] {
        assert!(route(name).check(args.as_object().unwrap()).is_ok(), "{name} {args}");
    }
}

#[test]
fn an_error_message_never_holds_what_the_database_said() {
    let (_dir, vault) = seeded();
    for (name, args) in [
        ("transactions.list", json!({ "limit": 4294967296u64 })),
        ("transactions.list", json!({ "offset": -3 })),
        ("budgets.overview", json!({ "month": "2026-13" })),
        ("accounts.show", json!({ "id": "1", "from": "2026-10-31", "to": "2026-10-01" })),
        ("accounts.show", json!({ "id": "1", "from": "2026-02-30" })),
        ("accounts.history", json!({ "id": "1", "year": "2026-" })),
        ("accounts.breakdown", json!({ "id": "1", "from": "2026-10-31", "to": "2026-10-01" })),
        ("accounts.breakdown", json!({ "id": "1", "to": "2026-13-01" })),
        ("accounts.breakdown", json!({ "id": "999999" })),
        ("transactions.list", json!({ "kind": "income" })),
        ("transactions.list", json!({ "kind": "BOTH" })),
        ("transfers.list", json!({ "limit": 4294967296u64 })),
        ("transfers.list", json!({ "from": "2026-13-01" })),
    ] {
        let message = run(&vault, name, args).unwrap_err().message.to_lowercase();
        for leak in ["sqlite", "select", "table", "column", "constraint", "categories", "payment_date", "accounts", "transfers", "opening_balance"] {
            assert!(!message.contains(leak), "{name}: {message}");
        }
    }
}

#[test]
fn reading_changes_nothing() {
    let (_dir, vault) = seeded();
    let before = vault.with_conn(|c| backup::dump(c)).unwrap();
    let id = first_expense(&vault)["id"].as_str().unwrap().to_string();
    for name in READ_ROUTES {
        for args in [
            json!({}),
            json!({ "month": "2026-10", "type": "EXPENSE" }),
            json!({ "year": "2026" }),
            json!({ "categoryId": id, "from": "2026-01", "to": "2026-10" }),
            json!({ "id": "1" }),
            json!({ "id": "2", "year": "2026" }),
        ] {
            let _ = run(&vault, name, args);
        }
    }
    let after = vault.with_conn(|c| backup::dump(c)).unwrap();
    assert_eq!(before, after, "a read route wrote something");
}

#[test]
fn a_locked_app_gives_no_data_from_any_read_route() {
    let (_dir, vault) = seeded();
    vault.lock();
    let id = "1";
    let args = |name: &str| match name {
        "budgets.overview" | "stats.budget" | "stats.planned" => json!({ "month": "2026-10" }),
        "stats.trend" => json!({ "categoryId": id, "from": "2026-01", "to": "2026-10" }),
        "stats.year" => json!({ "year": "2026" }),
        "stats.top" => json!({ "month": "2026-10", "type": "EXPENSE" }),
        "documents.show" => json!({ "id": "1" }),
        "people.show" => json!({ "id": "1" }),
        "splits.preview" => json!({ "amount": "100.00", "mode": "EQUAL", "people": ["1"] }),
        "accounts.show" => json!({ "id": "1" }),
        "accounts.history" => json!({ "id": "1", "year": "2026" }),
        "accounts.breakdown" => json!({ "id": "1" }),
        _ => json!({}),
    };
    for name in READ_ROUTES {
        assert_eq!(refused(&vault, name, args(name)), (423, "locked".into()), "{name}");
    }
}

/// Money is a string such as "12.50", never a JSON number, so no reader ever rounds it. This walks
/// every answer and lists the numbers in it; a new number has to be added here on purpose.
#[test]
fn money_and_ids_are_strings_and_the_only_numbers_are_counts_and_percentages() {
    let (_dir, vault) = seeded();
    let id = first_expense(&vault)["id"].as_str().unwrap().to_string();
    let args = |name: &str| match name {
        "budgets.overview" | "stats.budget" | "stats.planned" => json!({ "month": "2026-09" }),
        "stats.trend" => json!({ "categoryId": id, "from": "2026-01", "to": "2026-10" }),
        "stats.year" => json!({ "year": "2026" }),
        "stats.top" => json!({ "month": "2026-09", "type": "EXPENSE" }),
        "transactions.list" => json!({ "limit": 100 }),
        "documents.show" => json!({ "id": "1" }),
        "people.show" => json!({ "id": "1" }),
        "splits.preview" => json!({ "amount": "100.00", "mode": "SHARES", "mine": "2", "people": ["1"], "guests": "Bob", "values": "1,1" }),
        "accounts.show" => json!({ "id": "1", "from": "2025-06-01", "to": "2026-10-31" }),
        "accounts.history" => json!({ "id": "1", "year": "2026" }),
        "accounts.breakdown" => json!({ "id": "1", "from": "2025-06-01", "to": "2026-10-31" }),
        _ => json!({}),
    };
    // a friend, a shared bill, a loan and a split template, so that their answers are walked too
    let bill = ok(&vault, "transactions.list", json!({ "limit": 1 }))["transactions"][0]["id"].as_str().unwrap().to_string();
    let friend = ok(&vault, "people.create", json!({ "name": "Anna" }))["id"].as_str().unwrap().to_string();
    ok(&vault, "transactions.split", json!({ "id": bill, "mode": "EQUAL", "people": [friend], "guests": "Bob" }));
    ok(&vault, "ledger.add", json!({ "personId": friend, "kind": "LENT", "amount": "5.00", "date": "2026-10-01" }));
    let plan = ok(&vault, "recurring.list", json!({}))[0]["id"].as_str().unwrap().to_string();
    ok(&vault, "recurring.split", json!({ "id": plan, "mode": "SHARES", "mine": "2", "people": [friend], "values": "1" }));
    // a document, so that its answers are walked too
    vault
        .with_files(|conn, files| {
            let input = crate::documents::library::NewDocument { name: "receipt.pdf".into(), ..Default::default() };
            crate::documents::library::add(conn, files, &input, b"%PDF-1.4 receipt", TODAY)?;
            Ok(())
        })
        .unwrap();
    // an account in another currency with a row and a transfer in it, so that the answers that carry two amounts and a rate are walked too
    let wallet = ok(&vault, "accounts.create", json!({ "name": "Lei wallet", "kind": "CASH", "currency": "RON", "rate": "5.00" }))["id"].as_str().unwrap().to_string();
    let category = first_expense(&vault)["id"].as_str().unwrap().to_string();
    ok(&vault, "transactions.create", json!({ "name": "Coffee", "amount": "1.00", "paymentDate": "2026-10-10", "categoryId": category, "accountId": wallet, "nativeAmount": "5.00" }));
    ok(&vault, "transfers.create", json!({ "fromAccountId": "1", "toAccountId": wallet, "amount": "10.00", "toAmount": "52.00", "date": "2026-10-11" }));
    let mut numbers = BTreeSet::new();
    fn walk(value: &Value, key: &str, numbers: &mut BTreeSet<String>) {
        match value {
            Value::Number(_) => {
                numbers.insert(key.to_string());
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, key, numbers)),
            Value::Object(map) => map.iter().for_each(|(k, v)| walk(v, k, numbers)),
            _ => {}
        }
    }
    for name in READ_ROUTES {
        walk(&ok(&vault, name, args(name)), name, &mut numbers);
    }
    // Counts: `limit`, `offset`, `total` (a page), `intervalCount` (every 2 weeks), `paidCount`, and what an export
    // would contain (`transactions`, `categories`, `budgets`, `recurring`, `accounts`, `transfers`, `stashes`, `stashEntries`, `documents`, `people`, `ledger`, `splitBills`, `duePayments`, `debtHistory`, `history`), how many rows an account holds (`entries`), the weights of a split template (`mine`, `weight`: proportions, not money), the size of a file in bytes and the number of things it is attached to (`sizeBytes`, `linkCount`), `everyDays` (a backup schedule). Percentages: `pctUsed`,
    // `share`, `savingsRate`. Everything else, money above all, is a string.
    let allowed: BTreeSet<String> = ["intervalCount", "limit", "offset", "paidCount", "pctUsed", "savingsRate", "share", "total", "transactions", "categories", "budgets", "recurring", "accounts", "transfers", "stashes", "stashEntries", "entries", "documents", "people", "ledger", "splitBills", "duePayments", "debtHistory", "history", "everyDays", "sizeBytes", "linkCount", "mine", "weight"]
        .into_iter()
        .map(String::from)
        .collect();
    let new: Vec<_> = numbers.difference(&allowed).collect();
    assert!(new.is_empty(), "a new number in an answer: {new:?}. Money must be a string; if this is not money, add it to the list above");
    assert!(numbers.len() >= 5, "the walk found almost no numbers, so it is not looking at the answers: {numbers:?}");
}

#[test]
fn the_transactions_of_the_demo_data_are_listed_by_kind_and_the_two_kinds_are_all_of_them() {
    let (_dir, vault) = seeded();
    let all = ok(&vault, "transactions.list", json!({}));
    let kind = |kind: &str, extra: Value| {
        let mut body = json!({ "kind": kind, "limit": 100 });
        for (key, value) in extra.as_object().unwrap() {
            body[key] = value.clone();
        }
        ok(&vault, "transactions.list", body)
    };
    let (income, expense) = (kind("INCOME", json!({})), kind("EXPENSE", json!({})));
    assert_eq!(income["total"].as_u64().unwrap() + expense["total"].as_u64().unwrap(), all["total"].as_u64().unwrap());
    assert!(income["total"].as_u64().unwrap() > 0 && expense["total"].as_u64().unwrap() > 0);
    // no row is in a category of the other side (a category that was deleted since is not listed, but its rows keep their kind)
    let side = |kind: &str| -> BTreeSet<String> { ok(&vault, "categories.list", json!({ "type": kind })).as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap().to_string()).collect() };
    let (income_ids, expense_ids) = (side("INCOME"), side("EXPENSE"));
    assert!(!income_ids.is_empty() && !expense_ids.is_empty());
    for row in income["transactions"].as_array().unwrap() {
        assert!(!expense_ids.contains(row["categoryId"].as_str().unwrap()), "{row}");
    }
    for row in expense["transactions"].as_array().unwrap() {
        assert!(!income_ids.contains(row["categoryId"].as_str().unwrap()), "{row}");
    }
    // the same page of the same days as a list without the kind, less the other kind
    let (from, to) = ("2026-09-01", "2026-09-30");
    let (in_month, out_month, both) = (kind("INCOME", json!({ "from": from, "to": to })), kind("EXPENSE", json!({ "from": from, "to": to })), ok(&vault, "transactions.list", json!({ "from": from, "to": to })));
    assert_eq!(in_month["total"].as_u64().unwrap() + out_month["total"].as_u64().unwrap(), both["total"].as_u64().unwrap());
    // a page of one: the same row as the first of the whole kind, and the total of the kind
    let first = kind("INCOME", json!({ "limit": 1 }));
    assert_eq!((&first["transactions"][0], &first["total"]), (&income["transactions"][0], &income["total"]));
    // an account: the rows of its kind in it
    for id in ["1", "2", "3"] {
        let (a, b, both) = (kind("INCOME", json!({ "accountId": id })), kind("EXPENSE", json!({ "accountId": id })), ok(&vault, "transactions.list", json!({ "accountId": id })));
        assert_eq!(a["total"].as_u64().unwrap() + b["total"].as_u64().unwrap(), both["total"].as_u64().unwrap(), "account {id}");
    }
}

#[test]
fn the_rows_of_the_demo_account_in_lei_list_what_moved_it_in_lei_and_no_other_row_has_a_second_amount() {
    let (_dir, vault) = seeded();
    // every transaction of the app is listed, page by page: only those in the account in lei have a native amount
    let mut rows = Vec::new();
    for offset in (0..).step_by(100) {
        let page = ok(&vault, "transactions.list", json!({ "limit": 100, "offset": offset }));
        let more = page["transactions"].as_array().unwrap().clone();
        if more.is_empty() {
            break;
        }
        rows.extend(more);
    }
    assert_eq!(rows.len() as u64, ok(&vault, "transactions.list", json!({ "limit": 1 }))["total"].as_u64().unwrap());
    let (in_lei, elsewhere): (Vec<&Value>, Vec<&Value>) = rows.iter().partition(|t| t["accountId"] == "6");
    assert_eq!(in_lei.len(), 12, "ten expenses and two incomes");
    assert!(elsewhere.iter().all(|t| t["nativeAmount"].is_null()), "a row in an account in the app's currency has no second amount");
    for t in &in_lei {
        let (amount, native) = (t["amount"].as_str().unwrap(), t["nativeAmount"].as_str().expect("a row in lei has what moved the account"));
        assert!(amount.len() >= 4 && native.len() >= 4 && native.contains('.'), "{t}");
        assert_ne!(amount, native, "the demo data has the two apart: {t}");
    }
    // the account moves by the native amount: each line of its statement is a native amount, the sign by the side of the category
    let by_id: std::collections::BTreeMap<&str, &str> = in_lei.iter().map(|t| (t["id"].as_str().unwrap(), t["nativeAmount"].as_str().unwrap())).collect();
    let statement = ok(&vault, "accounts.show", json!({ "id": "6", "from": "2000-01-01", "to": "2026-12-31" }));
    assert_eq!(statement["currency"], "RON");
    let mut seen = 0;
    for line in statement["lines"].as_array().unwrap().iter().filter(|l| l["source"] == "TRANSACTION") {
        assert_eq!(line["amount"].as_str().unwrap().trim_start_matches('-'), by_id[line["id"].as_str().unwrap()], "{line}");
        seen += 1;
    }
    assert_eq!(seen, 12);
    // the two transfers have a second amount, and no other transfer has
    let page = ok(&vault, "transfers.list", json!({ "limit": 100 }));
    let total = page["total"].as_u64().unwrap();
    let mut all: Vec<Value> = page["transfers"].as_array().unwrap().clone();
    if total > 100 {
        all.extend(ok(&vault, "transfers.list", json!({ "limit": 100, "offset": 100 }))["transfers"].as_array().unwrap().clone());
    }
    assert_eq!(all.len() as u64, total);
    for t in &all {
        let touches_lei = t["fromAccountId"] == "6" || t["toAccountId"] == "6";
        assert_eq!(t["toAmount"].is_string(), touches_lei, "{t}");
    }
    let (leaving, arriving) = (all.iter().filter(|t| t["fromAccountId"] == "6").count(), all.iter().filter(|t| t["toAccountId"] == "6").count());
    assert_eq!((leaving, arriving), (1, 1), "a transfer each way");
}
