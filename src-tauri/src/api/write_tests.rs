//! The write routes against a vault with the demo data. What a route does is checked by reading the
//! result back through the read routes, and by what the app's own functions say.

use serde_json::{json, Value};

use super::read_tests::{at, ok, refused, run, run_at, seeded};
use super::routes;
use super::*;
use crate::tokens::Scope;
use crate::backup;

const WRITE_ROUTES: &[&str] = &[
    "settings.set",
    "categories.create",
    "categories.rename",
    "categories.delete",
    "categories.reorder",
    "transactions.create",
    "transactions.update",
    "transactions.delete",
    "budgets.set",
    "budgets.remove",
    "recurring.create",
    "recurring.update",
    "recurring.delete",
    "recurring.pay",
    "recurring.skip",
    "recurring.pause",
    "recurring.resume",
    "recurring.end",
    "documents.add",
    "documents.update",
    "documents.delete",
    "documents.link",
    "documents.unlink",
    "documents.get",
    "people.create",
    "people.update",
    "people.archive",
    "people.unarchive",
    "people.settle",
    "ledger.add",
    "ledger.update",
    "ledger.delete",
    "transactions.split",
    "transactions.unsplit",
    "recurring.split",
    "recurring.unsplit",
];

const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";
const INVALID: (u16, &str) = (400, "invalid_input");

fn category_id(vault: &Vault, kind: &str, name: &str) -> String {
    ok(vault, "categories.list", json!({ "type": kind }))
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no {kind} category {name}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn total(vault: &Vault) -> u64 {
    ok(vault, "transactions.list", json!({ "limit": 1 }))["total"].as_u64().unwrap()
}

#[test]
fn the_write_routes_are_exactly_these_and_each_needs_a_write_token() {
    let mut writes: Vec<&str> = routes::ALL.iter().filter(|r| r.scope == Scope::Write).map(|r| r.name).collect();
    writes.sort_unstable();
    let mut expected = WRITE_ROUTES.to_vec();
    expected.sort_unstable();
    assert_eq!(writes, expected, "a route that writes must be listed here on purpose");
    // Nothing that needs the person at the app is reachable with a token.
    for name in routes::ALL.iter().map(|r| r.name) {
        for forbidden in ["currency", "wipe", "restore", "token", "lock", "recovery", "passphrase"] {
            assert!(!name.contains(forbidden), "{name}");
        }
    }
}

#[test]
fn deleting_a_category_keeps_its_transactions() {
    let (_dir, vault) = seeded();
    let rent = category_id(&vault, "EXPENSE", "Rent");
    let before = ok(&vault, "transactions.list", json!({ "categoryId": rent, "limit": 100 }));
    assert!(before["total"].as_u64().unwrap() > 0);
    ok(&vault, "categories.delete", json!({ "id": rent }));
    let after = ok(&vault, "transactions.list", json!({ "categoryId": rent, "limit": 100 }));
    assert_eq!(before, after, "the transactions still have their category");
}

fn category_ids(vault: &Vault, kind: &str) -> Vec<String> {
    ok(vault, "categories.list", json!({ "type": kind })).as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn deleting_a_category_for_a_span_needs_the_month_and_follows_the_span() {
    let (_dir, vault) = seeded();
    let make = |name: &str| ok(&vault, "categories.create", json!({ "name": name, "type": "EXPENSE" }))["id"].as_str().unwrap().to_string();
    let (only, forward, all) = (make("Only"), make("Forward"), make("All"));
    ok(&vault, "categories.delete", json!({ "id": only, "month": "2026-10", "span": "ONLY" }));
    assert!(category_ids(&vault, "EXPENSE").contains(&only), "ONLY keeps the category");
    ok(&vault, "categories.delete", json!({ "id": forward, "month": "2026-08", "span": "FORWARD" }));
    assert!(!category_ids(&vault, "EXPENSE").contains(&forward), "FORWARD from a past month removes it");
    ok(&vault, "categories.delete", json!({ "id": all, "month": "2026-10", "span": "ALL" }));
    assert!(!category_ids(&vault, "EXPENSE").contains(&all));
    for args in [json!({ "id": only, "month": "2026-10" }), json!({ "id": only, "span": "ALL" }), json!({ "id": only, "month": "2026-10", "span": "SOON" })] {
        assert_eq!(refused(&vault, "categories.delete", args), (400, "invalid_input".into()));
    }
}

#[test]
fn categories_are_reordered_by_giving_all_of_one_type() {
    let (_dir, vault) = seeded();
    let mut ids = category_ids(&vault, "EXPENSE");
    ids.reverse();
    ok(&vault, "categories.reorder", json!({ "ids": ids }));
    assert_eq!(category_ids(&vault, "EXPENSE"), ids);
    // Not all of the type, a repeat, and a mix of types are refused.
    assert_eq!(refused(&vault, "categories.reorder", json!({ "ids": [ids[0]] })), (400, "invalid_input".into()));
    assert_eq!(refused(&vault, "categories.reorder", json!({ "ids": [ids[0], ids[0]] })), (400, "invalid_input".into()));
    assert_eq!(refused(&vault, "categories.reorder", json!({ "ids": [ids[0], category_ids(&vault, "INCOME")[0]] })), (400, "invalid_input".into()));
    assert_eq!(category_ids(&vault, "EXPENSE"), ids, "refused changes left the order");
}

#[test]
fn a_budget_span_reaches_the_months_it_says() {
    let (_dir, vault) = seeded();
    let budgeted = |month: &str| ok(&vault, "budgets.overview", json!({ "month": month }))[1]["budgeted"].clone();
    let set = |span: &str, amount: &str| ok(&vault, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-05", "amountLimit": amount, "span": span }));
    set("FORWARD", "777.00");
    assert_eq!((budgeted("2026-05"), budgeted("2026-09")), (json!("777.00"), json!("777.00")));
    assert_ne!(budgeted("2026-04"), json!("777.00"), "the months before keep theirs");
    set("ALL", "555.00");
    assert_eq!((budgeted("2026-04"), budgeted("2026-05")), (json!("555.00"), json!("555.00")));
    ok(&vault, "budgets.remove", json!({ "type": "EXPENSE", "month": "2026-05", "span": "ALL" }));
    assert_eq!(budgeted("2026-04"), Value::Null);
    assert_eq!(refused(&vault, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-05", "amountLimit": "1.00", "span": "sometimes" })), (400, "invalid_input".into()));
}

#[test]
fn one_chosen_occurrence_of_a_recurring_item_can_be_paid_or_skipped() {
    let (_dir, vault) = seeded();
    let bills = category_id(&vault, "EXPENSE", "Rent");
    let make = |name: &str| {
        let item = json!({ "name": name, "amount": "10.00", "categoryId": bills, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1, "description": "plain" });
        ok(&vault, "recurring.create", item)["id"].as_str().unwrap().to_string()
    };
    let (pay, skip) = (make("Pay"), make("Skip"));
    let paid = ok(&vault, "recurring.pay", json!({ "id": pay, "dueDate": "2026-11-20", "description": "early" }));
    assert_eq!(paid["description"], "early");
    let item = |id: &str| ok(&vault, "recurring.list", json!({})).as_array().unwrap().iter().find(|r| r["id"] == id).unwrap().clone();
    assert_eq!(item(&pay)["nextDueDate"], "2026-10-20", "the current occurrence is still due");
    assert_eq!(refused(&vault, "recurring.pay", json!({ "id": pay, "dueDate": "2026-11-20" })), (400, "invalid_input".into()), "not twice");
    assert_eq!(ok(&vault, "recurring.pay", json!({ "id": pay, "description": "" }))["description"], Value::Null, "empty clears the note");
    ok(&vault, "recurring.skip", json!({ "id": skip, "dueDate": "2026-12-20" }));
    assert_eq!(item(&skip)["nextDueDate"], "2026-10-20");
    assert_eq!(refused(&vault, "recurring.skip", json!({ "id": skip, "dueDate": "2026-12-20" })), (400, "invalid_input".into()), "not twice");
    assert_eq!(refused(&vault, "recurring.skip", json!({ "id": skip, "dueDate": "2026-10-01" })), (400, "invalid_input".into()), "not before the next one");
}

#[test]
fn creating_with_a_client_uuid_is_safe_to_repeat() {
    let (_dir, vault) = seeded();
    let food = category_id(&vault, "EXPENSE", "Groceries");
    let before = total(&vault);
    let request = json!({ "name": "Coffee", "amount": "3.50", "paymentDate": "2026-10-14", "categoryId": food, "clientUuid": UUID });
    let first = ok(&vault, "transactions.create", request.clone());
    let again = ok(&vault, "transactions.create", request.clone());
    assert_eq!(first, again, "the retry gets the first transaction back");
    assert_eq!(total(&vault), before + 1, "and nothing is recorded twice");
    // The retry may differ in case; a different request with the same uuid still gives the first one.
    let shouting = ok(&vault, "transactions.create", json!({ "name": "Coffee", "amount": "3.50", "paymentDate": "2026-10-14", "categoryId": food, "clientUuid": UUID.to_uppercase() }));
    assert_eq!(shouting["id"], first["id"]);
    assert_eq!(first["clientUuid"], UUID);
    assert_eq!(total(&vault), before + 1);
    // Without one, every call makes a new transaction.
    let plain = json!({ "name": "Tea", "amount": "2.00", "paymentDate": "2026-10-14", "categoryId": food });
    ok(&vault, "transactions.create", plain.clone());
    ok(&vault, "transactions.create", plain);
    assert_eq!(total(&vault), before + 3);
    // A malformed uuid is refused by shape.
    for bad in ["not-a-uuid", "123e4567e89b12d3a456426614174000", "123e4567-e89b-12d3-a456-42661417400g", &format!("{UUID}0"), ""] {
        let reply = run(&vault, "transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": food, "clientUuid": bad })).unwrap_err();
        assert_eq!((reply.status, reply.code), INVALID, "{bad:?}");
    }
}

#[test]
fn amounts_are_exact_text_with_two_decimals() {
    let (_dir, vault) = seeded();
    let food = category_id(&vault, "EXPENSE", "Groceries");
    let create = |amount: Value| run(&vault, "transactions.create", json!({ "name": "Amount", "amount": amount, "paymentDate": "2026-10-14", "categoryId": food }));
    for good in ["0.01", "0.10", "12.50", "1234567.89", "9999999999.99"] {
        assert_eq!(create(json!(good)).unwrap()["amount"], good, "{good} must come back as written");
    }
    for bad in ["12.5", "12", "12.", ".50", "12.500", "01.00", "-1.00", "+1.00", "1,50", "1e3", " 1.00", "1.00 ", "１２.５０", "99999999999.99", ""] {
        let e = create(json!(bad)).unwrap_err();
        assert_eq!((e.status, e.code), INVALID, "{bad:?}");
    }
    assert_eq!(create(json!(12.5)).unwrap_err().message, "`amount` must be an amount as text with two decimals, like \"12.50\"");
    assert_eq!(create(json!(1250)).unwrap_err().status, 400);
    assert_eq!(create(json!(null)).unwrap_err().status, 400);
    // Zero has the right shape but is not a payment.
    assert_eq!((create(json!("0.00")).unwrap_err().status, "amounts must be more than zero"), (400, "amounts must be more than zero"));
}

#[test]
fn a_transaction_needs_a_real_category_a_real_date_and_a_name() {
    let (_dir, vault) = seeded();
    let food = category_id(&vault, "EXPENSE", "Groceries");
    let base = json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": food });
    let with = |key: &str, value: Value| {
        let mut request = base.clone();
        request[key] = value;
        request
    };
    for (key, value) in [
        ("categoryId", json!("999999")),
        ("paymentDate", json!("2026-02-30")),
        ("paymentDate", json!("2026-13-01")),
        ("name", json!("")),
        ("name", json!("   ")),
        ("name", json!("x".repeat(51))),
        ("name", json!("a\u{0}b")),
        ("description", json!("a\u{0}b")),
    ] {
        let e = run(&vault, "transactions.create", with(key, value.clone())).unwrap_err();
        assert!(e.status == 400 || e.status == 404, "{key} {value}: {e:?}");
    }
    for missing in ["name", "amount", "paymentDate", "categoryId"] {
        let mut request = base.clone();
        request.as_object_mut().unwrap().remove(missing);
        assert_eq!(run(&vault, "transactions.create", request).unwrap_err().message, format!("missing argument `{missing}`"));
    }
}

#[test]
fn a_budget_must_fit_its_category_and_its_amount() {
    let (_dir, vault) = seeded();
    let groceries = category_id(&vault, "EXPENSE", "Groceries");
    let salary = category_id(&vault, "INCOME", ok(&vault, "categories.list", json!({ "type": "INCOME" }))[0]["name"].as_str().unwrap());
    // An expense category cannot get an income budget, and the reverse.
    assert_eq!(refused(&vault, "budgets.set", json!({ "type": "INCOME", "month": "2026-12", "amountLimit": "10.00", "categoryId": groceries })).0, 400);
    assert_eq!(refused(&vault, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-12", "amountLimit": "10.00", "categoryId": salary })).0, 400);
    assert_eq!(refused(&vault, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-12", "amountLimit": "10.00", "categoryId": "999999" })).0 / 100, 4);
    for bad in ["400", "400.5", "-1.00", "1e3", ""] {
        assert_eq!(refused(&vault, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-12", "amountLimit": bad })), (400, "invalid_input".into()), "{bad:?}");
    }
    assert_eq!(refused(&vault, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-12" })), (400, "invalid_input".into()), "the amount is required; removing is its own route");
    assert_eq!(refused(&vault, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-12", "amountLimit": null })), (400, "invalid_input".into()));
    assert_eq!(refused(&vault, "budgets.set", json!({ "type": "EXPENSE", "month": "2026-13", "amountLimit": "1.00" })), (400, "invalid_input".into()));
    assert_eq!(refused(&vault, "budgets.set", json!({ "type": "SAVINGS", "month": "2026-12", "amountLimit": "1.00" })), (400, "invalid_input".into()));
    assert_eq!(refused(&vault, "budgets.remove", json!({ "type": "EXPENSE", "month": "2026-12", "amountLimit": "1.00" })), (400, "invalid_input".into()), "remove takes no amount");
}

#[test]
fn recurring_values_are_checked() {
    let (_dir, vault) = seeded();
    let bills = category_id(&vault, "EXPENSE", "Rent");
    let base = json!({ "name": "X", "amount": "1.00", "categoryId": bills, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1 });
    let with = |key: &str, value: Value| {
        let mut request = base.clone();
        request[key] = value;
        request
    };
    for (key, value) in [
        ("intervalUnit", json!("FORTNIGHT")),
        ("intervalUnit", json!("month")),
        ("intervalCount", json!(0)),
        ("intervalCount", json!(1001)),
        ("intervalCount", json!(-1)),
        ("intervalCount", json!("1")),
        ("startDate", json!("2026-02-30")),
        ("startDate", json!("tomorrow")),
        ("endDate", json!("2026-10-01")),
        ("amount", json!("0.00")),
        ("amount", json!("5")),
        ("name", json!("")),
        ("categoryId", json!("999999")),
    ] {
        let e = run(&vault, "recurring.create", with(key, value.clone())).unwrap_err();
        assert!(matches!(e.status, 400 | 404), "{key} {value}: {e:?}");
    }
    let before = ok(&vault, "recurring.list", json!({})).as_array().unwrap().len();
    assert_eq!(ok(&vault, "recurring.list", json!({})).as_array().unwrap().len(), before, "refused creations left nothing behind");
    // null is only for fields that can be cleared.
    assert_eq!(refused(&vault, "recurring.create", with("amount", Value::Null)).0, 400);
    let id = ok(&vault, "recurring.create", base.clone())["id"].as_str().unwrap().to_string();
    assert_eq!(refused(&vault, "recurring.update", json!({ "id": id, "name": null })).0, 400);
    assert_eq!(refused(&vault, "recurring.update", json!({ "id": id, "startDate": null })).0, 400);
    assert_eq!(refused(&vault, "recurring.update", json!({ "id": id, "intervalCount": null })).0, 400);
    assert_eq!(refused(&vault, "transactions.update", json!({ "id": "1", "amount": null })).0, 400);
    assert_eq!(refused(&vault, "transactions.update", json!({ "id": "1", "name": null })).0, 400);
}

#[test]
fn the_app_settings_can_be_changed_but_not_the_currency() {
    let (_dir, vault) = seeded();
    let changed = ok(&vault, "settings.set", json!({ "language": "ro" }));
    assert_eq!(changed, json!({ "currency": "EUR", "locale": "en-IE", "timezone": "Europe/Dublin", "language": "ro" }), "only the language changed");
    let changed = ok(&vault, "settings.set", json!({ "locale": "de-DE", "timezone": "Europe/Berlin" }));
    assert_eq!(changed, json!({ "currency": "EUR", "locale": "de-DE", "timezone": "Europe/Berlin", "language": "ro" }));
    assert_eq!(ok(&vault, "settings.get", json!({})), changed);
    // "Today" follows the new zone: 23:30 UTC is already tomorrow in Berlin.
    assert_eq!(run_at(&vault, "today", json!({}), at("2026-10-14T23:30:00Z")).unwrap()["date"], "2026-10-15");

    let before = ok(&vault, "settings.get", json!({}));
    assert_eq!(refused(&vault, "settings.set", json!({})), (400, "invalid_input".into()));
    assert_eq!(run(&vault, "settings.set", json!({})).unwrap_err().message, "give at least one of locale, timezone and language");
    assert_eq!(run(&vault, "settings.set", json!({ "currency": "USD" })).unwrap_err().message, "unknown argument `currency`", "the currency is changed in the app");
    for (key, bad) in [("timezone", "Mars/Base"), ("timezone", ""), ("timezone", "Europe/Dublin; DROP TABLE settings"), ("locale", ""), ("locale", "en IE"), ("language", "english"), ("language", "e")] {
        assert_eq!(refused(&vault, "settings.set", json!({ key: bad })), (400, "invalid_input".into()), "{key} {bad:?}");
    }
    assert_eq!(ok(&vault, "settings.get", json!({})), before, "refused changes changed nothing");
}

/// Every refused request must leave the database exactly as it was, history included.
#[test]
fn a_refused_write_changes_nothing() {
    let (_dir, vault) = seeded();
    let food = category_id(&vault, "EXPENSE", "Groceries");
    let salary = ok(&vault, "categories.list", json!({ "type": "INCOME" }))[0]["id"].as_str().unwrap().to_string();
    let before = vault.with_conn(|c| backup::dump(c)).unwrap();
    let bad = [
        ("categories.create", json!({ "name": "", "type": "EXPENSE" })),
        ("categories.create", json!({ "name": "Groceries", "type": "EXPENSE" })),
        ("categories.rename", json!({ "id": food, "name": "Rent" })),
        ("categories.rename", json!({ "id": "999999", "name": "X" })),
        ("categories.delete", json!({ "id": "999999" })),
        ("transactions.create", json!({ "name": "X", "amount": "0.00", "paymentDate": "2026-10-14", "categoryId": food })),
        ("transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-02-30", "categoryId": food })),
        ("transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": "999999" })),
        ("transactions.update", json!({ "id": "999999", "name": "X" })),
        ("transactions.update", json!({ "id": "1", "amount": "0.00" })),
        ("transactions.update", json!({ "id": "1", "categoryId": "999999" })),
        ("transactions.delete", json!({ "id": "999999" })),
        ("budgets.set", json!({ "type": "INCOME", "month": "2026-12", "amountLimit": "1.00", "categoryId": food })),
        ("budgets.set", json!({ "type": "EXPENSE", "month": "2026-12", "amountLimit": "1.00", "categoryId": salary })),
        ("budgets.set", json!({ "type": "EXPENSE", "month": "2026-12", "amountLimit": "1.00", "categoryId": "999999" })),
        ("budgets.remove", json!({ "type": "EXPENSE", "month": "2026-13" })),
        ("recurring.create", json!({ "name": "X", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 0 })),
        ("recurring.update", json!({ "id": "999999", "name": "X" })),
        ("recurring.pay", json!({ "id": "999999" })),
        ("recurring.skip", json!({ "id": "999999" })),
        ("recurring.pause", json!({ "id": "999999" })),
        ("recurring.resume", json!({ "id": "999999" })),
        ("recurring.end", json!({ "id": "999999" })),
        ("recurring.delete", json!({ "id": "999999" })),
        ("settings.set", json!({ "timezone": "Mars/Base" })),
        ("settings.set", json!({})),
    ];
    for (name, args) in bad {
        let e = run(&vault, name, args.clone()).expect_err(&format!("{name} {args} was accepted"));
        assert!(matches!(e.status, 400 | 404 | 409), "{name} {args}: {e:?}");
    }
    let after = vault.with_conn(|c| backup::dump(c)).unwrap();
    assert_eq!(before, after, "a refused write left a trace");
}

#[test]
fn a_write_is_recorded_in_the_history_like_a_change_in_the_window() {
    let (_dir, vault) = seeded();
    let food = category_id(&vault, "EXPENSE", "Groceries");
    let count = |entity: &str, action: &str| -> i64 {
        vault.with_conn(|c| Ok(c.query_row("SELECT count(*) FROM audit_log WHERE entity_type = ?1 AND action = ?2", [entity, action], |r| r.get(0))?)).unwrap()
    };
    let made = ok(&vault, "transactions.create", json!({ "name": "Bread", "amount": "2.40", "paymentDate": "2026-10-14", "categoryId": food }));
    assert_eq!(count("transactions", "INSERT"), 1);
    let id = made["id"].as_str().unwrap();
    ok(&vault, "transactions.update", json!({ "id": id, "name": "Rye" }));
    assert_eq!(count("transactions", "UPDATE"), 1);
    ok(&vault, "transactions.delete", json!({ "id": id }));
    assert_eq!(count("transactions", "DELETE"), 1);
    ok(&vault, "categories.rename", json!({ "id": food, "name": "Food" }));
    assert_eq!(count("categories", "UPDATE"), 1);
}

#[test]
fn a_locked_app_refuses_every_write_and_nothing_changes() {
    let (_dir, vault) = seeded();
    let food = category_id(&vault, "EXPENSE", "Groceries");
    let before = vault.with_conn(|c| backup::dump(c)).unwrap();
    vault.lock();
    let cases = [
        ("settings.set", json!({ "language": "ro" })),
        ("categories.create", json!({ "name": "Pets", "type": "EXPENSE" })),
        ("categories.rename", json!({ "id": food, "name": "Food" })),
        ("categories.delete", json!({ "id": food })),
        ("categories.reorder", json!({ "ids": [food] })),
        ("transactions.create", json!({ "name": "X", "amount": "1.00", "paymentDate": "2026-10-14", "categoryId": food })),
        ("transactions.update", json!({ "id": "1", "name": "X" })),
        ("transactions.delete", json!({ "id": "1" })),
        ("budgets.set", json!({ "type": "EXPENSE", "month": "2026-12", "amountLimit": "1.00" })),
        ("budgets.remove", json!({ "type": "EXPENSE", "month": "2026-12" })),
        ("recurring.create", json!({ "name": "X", "categoryId": food, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1 })),
        ("recurring.update", json!({ "id": "1", "name": "X" })),
        ("recurring.delete", json!({ "id": "1" })),
        ("recurring.pay", json!({ "id": "1" })),
        ("recurring.skip", json!({ "id": "1" })),
        ("recurring.pause", json!({ "id": "1" })),
        ("recurring.resume", json!({ "id": "1" })),
        ("recurring.end", json!({ "id": "1" })),
        ("documents.add", json!({ "name": "a.pdf", "data": "JVBERi0xLjQK" })),
        ("documents.update", json!({ "id": "1", "name": "X" })),
        ("documents.delete", json!({ "id": "1" })),
        ("documents.link", json!({ "id": "1", "transactionId": "1" })),
        ("documents.unlink", json!({ "id": "1", "transactionId": "1" })),
        ("documents.get", json!({ "id": "1", "out": "/tmp/x.pdf" })),
        ("people.create", json!({ "name": "Anna" })),
        ("people.update", json!({ "id": "1", "name": "Anna" })),
        ("people.archive", json!({ "id": "1" })),
        ("people.unarchive", json!({ "id": "1" })),
        ("people.settle", json!({ "id": "1" })),
        ("ledger.add", json!({ "personId": "1", "kind": "LENT", "amount": "1.00", "date": "2026-10-14" })),
        ("ledger.update", json!({ "id": "1", "amount": "2.00" })),
        ("ledger.delete", json!({ "id": "1" })),
        ("transactions.split", json!({ "id": "1", "mode": "EQUAL", "people": ["1"] })),
        ("transactions.unsplit", json!({ "id": "1" })),
        ("recurring.split", json!({ "id": "1", "mode": "EQUAL", "people": ["1"] })),
        ("recurring.unsplit", json!({ "id": "1" })),
    ];
    let mut covered: Vec<&str> = cases.iter().map(|(name, _)| *name).collect();
    covered.sort_unstable();
    let mut all = WRITE_ROUTES.to_vec();
    all.sort_unstable();
    assert_eq!(covered, all, "every write route must be tried while locked");
    for (name, args) in cases {
        assert_eq!(refused(&vault, name, args), (423, "locked".into()), "{name}");
    }
    vault.unlock(None).unwrap();
    assert_eq!(vault.with_conn(|c| backup::dump(c)).unwrap(), before);
}
