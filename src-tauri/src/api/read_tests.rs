//! The read routes against a vault with the demo data. Each route is compared with the function the
//! desktop screens call, so "the CLI shows what the app shows" is a test, not a hope.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};

use super::routes::{self, Ctx, Route};
use super::*;
use crate::categories::{self, CategoryType};
use crate::keys::memory::MemoryStore;
use crate::tokens::Scope;
use crate::transactions::{self, Filter};
use crate::vault::Settings;
use crate::{backup, budgets, recurring, seed, stats};

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
fn every_read_route_is_in_the_table_and_needs_only_a_read_token() {
    for name in READ_ROUTES {
        assert_eq!(route(name).scope, Scope::Read, "{name}");
    }
}

#[test]
fn the_demo_data_is_there_so_the_comparisons_below_mean_something() {
    let (_dir, vault) = seeded();
    assert!(ok(&vault, "categories.list", json!({}))
        .as_array()
        .is_some_and(|rows| rows.len() > 5));
    assert!(ok(&vault, "transactions.list", json!({}))["total"].as_u64().unwrap() > 100);
    assert!(!ok(&vault, "budgets.history", json!({})).as_array().unwrap().is_empty());
    assert!(!ok(&vault, "recurring.list", json!({})).as_array().unwrap().is_empty());
    assert!(!ok(&vault, "stats.year", json!({ "year": "2026" })).as_array().unwrap().is_empty());
}

#[test]
fn today_and_settings_come_from_the_app() {
    let (_dir, vault) = seeded();
    assert_eq!(ok(&vault, "today", json!({})), json!({ "date": TODAY, "month": "2026-10" }));
    assert_eq!(
        ok(&vault, "settings.get", json!({})),
        json!({ "currency": "EUR", "locale": "en-IE", "timezone": "Europe/Dublin", "language": "en" })
    );
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
fn categories_are_listed_as_the_screens_list_them() {
    let (_dir, vault) = seeded();
    assert_eq!(ok(&vault, "categories.list", json!({})), expect(&vault, |c| categories::list(c, None)));
    let income = ok(&vault, "categories.list", json!({ "type": "INCOME" }));
    let expense = ok(&vault, "categories.list", json!({ "type": "EXPENSE" }));
    assert_eq!(income, expect(&vault, |c| categories::list(c, Some(CategoryType::Income))));
    assert_eq!(expense, expect(&vault, |c| categories::list(c, Some(CategoryType::Expense))));
    assert!(income.as_array().unwrap().iter().all(|c| c["type"] == "INCOME"));
    assert!(expense.as_array().unwrap().iter().all(|c| c["type"] == "EXPENSE"));
    let all = ok(&vault, "categories.list", json!({}));
    assert_eq!(all.as_array().unwrap().len(), income.as_array().unwrap().len() + expense.as_array().unwrap().len());
    // An id is a string.
    assert!(all[0]["id"].is_string());
}

#[test]
fn transactions_are_listed_with_their_filters() {
    let (_dir, vault) = seeded();
    let direct = |filter: Filter| expect(&vault, |c| transactions::list(c, &filter));

    let first = ok(&vault, "transactions.list", json!({}));
    assert_eq!(first, direct(Filter::default()));
    assert_eq!(first["transactions"].as_array().unwrap().len(), 50, "the default page");
    assert_eq!((first["limit"].clone(), first["offset"].clone()), (json!(50), json!(0)));

    let page = ok(&vault, "transactions.list", json!({ "limit": 3, "offset": 2 }));
    assert_eq!(page, direct(Filter { limit: Some(3), offset: Some(2), ..Default::default() }));
    assert_eq!(page["transactions"].as_array().unwrap().len(), 3);
    assert_eq!(page["transactions"][0], first["transactions"][2], "offset skips from the newest");
    assert_eq!(page["total"], first["total"], "the total is of the filter, not of the page");

    let september = ok(&vault, "transactions.list", json!({ "from": "2026-09-01", "to": "2026-09-30", "limit": 100 }));
    assert_eq!(september, direct(Filter { from: Some("2026-09-01".into()), to: Some("2026-09-30".into()), limit: Some(100), ..Default::default() }));
    let rows = september["transactions"].as_array().unwrap();
    assert!(!rows.is_empty() && rows.iter().all(|t| ("2026-09-01"..="2026-09-30").contains(&t["paymentDate"].as_str().unwrap())));

    let category = first_expense(&vault);
    let id = category["id"].as_str().unwrap().to_string();
    let by_category = ok(&vault, "transactions.list", json!({ "categoryId": id }));
    assert_eq!(by_category, direct(Filter { category_id: Some(id.clone()), ..Default::default() }));
    assert!(by_category["transactions"].as_array().unwrap().iter().all(|t| t["categoryId"] == id));

    let name = first["transactions"][0]["name"].as_str().unwrap().to_string();
    let searched = ok(&vault, "transactions.list", json!({ "q": name.to_uppercase() }));
    assert_eq!(searched, direct(Filter { q: Some(name.to_uppercase()), ..Default::default() }));
    assert!(searched["total"].as_u64().unwrap() >= 1);

    let nothing = ok(&vault, "transactions.list", json!({ "categoryId": "999999" }));
    assert_eq!((nothing["total"].clone(), nothing["transactions"].clone()), (json!(0), json!([])));
}

#[test]
fn budgets_and_recurring_items_are_listed_as_the_screens_list_them() {
    let (_dir, vault) = seeded();
    for month in ["2026-10", "2026-03", "2025-08", "2019-01"] {
        assert_eq!(ok(&vault, "budgets.overview", json!({ "month": month })), expect(&vault, |c| budgets::overview(c, month)), "{month}");
    }
    assert_ne!(
        ok(&vault, "budgets.overview", json!({ "month": "2026-10" })),
        ok(&vault, "budgets.overview", json!({ "month": "2025-08" })),
        "the month argument is used (the demo budgets change over time)"
    );
    assert_eq!(ok(&vault, "budgets.history", json!({})), expect(&vault, |c| budgets::history(c, noon())));
    assert_eq!(ok(&vault, "recurring.list", json!({})), expect(&vault, |c| recurring::list(c)));

    for (args, within) in [(json!({}), None), (json!({ "withinDays": 0 }), Some(0)), (json!({ "withinDays": 7 }), Some(7)), (json!({ "withinDays": 366 }), Some(366))] {
        assert_eq!(ok(&vault, "recurring.dues", args.clone()), expect(&vault, |c| recurring::dues(c, TODAY, within)), "{args}");
    }
    let short = ok(&vault, "recurring.dues", json!({ "withinDays": 0 }));
    let long = ok(&vault, "recurring.dues", json!({ "withinDays": 366 }));
    assert!(long["upcoming"].as_array().unwrap().len() >= short["upcoming"].as_array().unwrap().len());
}

#[test]
fn statistics_are_what_the_dashboard_shows() {
    let (_dir, vault) = seeded();
    for month in ["2026-10", "2026-09", "2025-08"] {
        assert_eq!(ok(&vault, "stats.budget", json!({ "month": month })), expect(&vault, |c| stats::budget_vs_actual(c, month)), "{month}");
        assert_eq!(ok(&vault, "stats.planned", json!({ "month": month })), expect(&vault, |c| stats::planned_vs_paid(c, month)), "{month}");
    }
    assert_eq!(ok(&vault, "stats.year", json!({ "year": "2026" })), expect(&vault, |c| stats::year_trend(c, "2026")));
    assert_eq!(ok(&vault, "stats.commitments", json!({})), expect(&vault, |c| stats::commitments(c)));

    let category = first_expense(&vault);
    let id: i64 = category["id"].as_str().unwrap().parse().unwrap();
    let trend = ok(&vault, "stats.trend", json!({ "categoryId": id.to_string(), "from": "2026-01", "to": "2026-10" }));
    assert_eq!(trend, expect(&vault, |c| stats::category_trend(c, id, "2026-01", "2026-10")));
    assert_eq!(trend["points"].as_array().unwrap().len(), 10);

    let top = ok(&vault, "stats.top", json!({ "month": "2026-09", "type": "EXPENSE" }));
    assert_eq!(top, expect(&vault, |c| stats::top_categories(c, "2026-09", CategoryType::Expense, 5)), "the default is 5");
    assert!(top.as_array().unwrap().len() <= 5 && !top.as_array().unwrap().is_empty());
    let two = ok(&vault, "stats.top", json!({ "month": "2026-09", "type": "EXPENSE", "limit": 2 }));
    assert_eq!(two, expect(&vault, |c| stats::top_categories(c, "2026-09", CategoryType::Expense, 2)));
    assert_eq!(two.as_array().unwrap().len(), 2);
    let income = ok(&vault, "stats.top", json!({ "month": "2026-09", "type": "INCOME" }));
    assert_eq!(income, expect(&vault, |c| stats::top_categories(c, "2026-09", CategoryType::Income, 5)));
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
    ] {
        let message = run(&vault, name, args).unwrap_err().message.to_lowercase();
        for leak in ["sqlite", "select", "table", "column", "constraint", "categories", "payment_date"] {
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
        for args in [json!({}), json!({ "month": "2026-10", "type": "EXPENSE" }), json!({ "year": "2026" }), json!({ "categoryId": id, "from": "2026-01", "to": "2026-10" })] {
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
    // would contain (`transactions`, `categories`, `budgets`, `recurring`, `documents`, `people`, `ledger`, `splitBills`, `duePayments`, `history`), the weights of a split template (`mine`, `weight`: proportions, not money), the size of a file in bytes and the number of things it is attached to (`sizeBytes`, `linkCount`), `everyDays` (a backup schedule). Percentages: `pctUsed`,
    // `share`, `savingsRate`. Everything else, money above all, is a string.
    let allowed: BTreeSet<String> = ["intervalCount", "limit", "offset", "paidCount", "pctUsed", "savingsRate", "share", "total", "transactions", "categories", "budgets", "recurring", "documents", "people", "ledger", "splitBills", "duePayments", "history", "everyDays", "sizeBytes", "linkCount", "mine", "weight"]
        .into_iter()
        .map(String::from)
        .collect();
    let new: Vec<_> = numbers.difference(&allowed).collect();
    assert!(new.is_empty(), "a new number in an answer: {new:?}. Money must be a string; if this is not money, add it to the list above");
    assert!(numbers.len() >= 5, "the walk found almost no numbers, so it is not looking at the answers: {numbers:?}");
}
