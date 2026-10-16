//! The write routes against a vault with the demo data. What a route does is checked by reading the
//! result back through the read routes, and by what the app's own functions say.

use serde_json::{json, Value};

use super::read_tests::{at, noon, ok, refused, run, run_at, seeded, TODAY};
use super::routes;
use super::*;
use crate::categories::CategoryType;
use crate::tokens::Scope;
use crate::{backup, budgets};

const WRITE_ROUTES: &[&str] = &[
    "settings.set",
    "categories.create",
    "categories.rename",
    "categories.delete",
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
fn categories_can_be_made_renamed_and_deleted() {
    let (_dir, vault) = seeded();
    let made = ok(&vault, "categories.create", json!({ "name": "  Pets  ", "type": "EXPENSE" }));
    assert_eq!((made["name"].clone(), made["type"].clone()), (json!("Pets"), json!("EXPENSE")), "the name is trimmed");
    let id = made["id"].as_str().unwrap().to_string();
    assert_eq!(category_id(&vault, "EXPENSE", "Pets"), id);

    // The same name on the other side is a different category.
    assert!(run(&vault, "categories.create", json!({ "name": "Pets", "type": "INCOME" })).is_ok());
    assert_eq!(refused(&vault, "categories.create", json!({ "name": "Pets", "type": "EXPENSE" })).1, "name_exists", "the same name twice on one side");
    assert_eq!(refused(&vault, "categories.create", json!({ "name": " Pets ", "type": "EXPENSE" })).1, "name_exists", "the name is trimmed before it is compared");
    for bad in ["", "   ", &"x".repeat(51), "two\nlines"] {
        assert_eq!(refused(&vault, "categories.create", json!({ "name": bad, "type": "EXPENSE" })), (400, "invalid_input".into()), "{bad:?}");
    }

    let renamed = ok(&vault, "categories.rename", json!({ "id": id, "name": "Animals" }));
    assert_eq!((renamed["id"].clone(), renamed["name"].clone()), (json!(id), json!("Animals")));
    assert_eq!(refused(&vault, "categories.rename", json!({ "id": id, "name": "Groceries" })).1, "name_exists");
    assert_eq!(refused(&vault, "categories.rename", json!({ "id": "999999", "name": "Nothing" })), (404, "not_found".into()));

    assert_eq!(ok(&vault, "categories.delete", json!({ "id": id })), Value::Null);
    assert!(ok(&vault, "categories.list", json!({})).as_array().unwrap().iter().all(|c| c["id"] != json!(id)));
    assert_eq!(refused(&vault, "categories.delete", json!({ "id": id })).0, 404, "deleting twice");
    assert_eq!(refused(&vault, "categories.delete", json!({ "id": "999999" })), (404, "not_found".into()));
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

#[test]
fn a_transaction_is_recorded_changed_and_deleted() {
    let (_dir, vault) = seeded();
    let food = category_id(&vault, "EXPENSE", "Groceries");
    let before = total(&vault);
    let made = ok(&vault, "transactions.create", json!({ "name": "Bread", "amount": "2.40", "paymentDate": "2026-10-14", "categoryId": food, "description": "Sourdough" }));
    assert_eq!(made["amount"], "2.40");
    assert_eq!((made["name"].clone(), made["paymentDate"].clone(), made["categoryId"].clone()), (json!("Bread"), json!("2026-10-14"), json!(food)));
    assert_eq!(made["description"], "Sourdough");
    assert!(made["id"].is_string() && made["clientUuid"].is_null());
    assert_eq!(total(&vault), before + 1);
    let id = made["id"].as_str().unwrap().to_string();

    // Only what is sent changes.
    let updated = ok(&vault, "transactions.update", json!({ "id": id, "name": "Rye bread", "amount": "2.95" }));
    assert_eq!((updated["name"].clone(), updated["amount"].clone(), updated["description"].clone()), (json!("Rye bread"), json!("2.95"), json!("Sourdough")));
    assert_eq!(updated["paymentDate"], "2026-10-14");
    // null clears the description; leaving it out keeps it.
    let cleared = ok(&vault, "transactions.update", json!({ "id": id, "description": null }));
    assert!(cleared["description"].is_null());
    let moved = ok(&vault, "transactions.update", json!({ "id": id, "paymentDate": "2026-10-01", "categoryId": category_id(&vault, "EXPENSE", "Rent") }));
    assert_eq!(moved["paymentDate"], "2026-10-01");
    assert_eq!(moved["categoryId"], json!(category_id(&vault, "EXPENSE", "Rent")));
    assert!(cleared["description"].is_null() && moved["description"].is_null());
    // Naming nothing to change is allowed and changes nothing.
    assert_eq!(ok(&vault, "transactions.update", json!({ "id": id })), moved);

    assert_eq!(ok(&vault, "transactions.delete", json!({ "id": id })), Value::Null);
    assert_eq!(total(&vault), before);
    assert_eq!(refused(&vault, "transactions.delete", json!({ "id": id })), (404, "not_found".into()));
    assert_eq!(refused(&vault, "transactions.update", json!({ "id": id, "name": "Ghost" })), (404, "not_found".into()));
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
fn budgets_are_set_from_a_month_and_removed_the_same_way() {
    let (_a, via_api) = seeded();
    let (_b, direct) = seeded();
    let groceries = category_id(&via_api, "EXPENSE", "Groceries");
    assert_eq!(groceries, category_id(&direct, "EXPENSE", "Groceries"));
    let steps = [
        json!({ "type": "EXPENSE", "month": "2026-12", "amountLimit": "2500.00" }),
        json!({ "type": "INCOME", "month": "2026-11", "amountLimit": "4000.00" }),
        json!({ "type": "EXPENSE", "month": "2026-12", "amountLimit": "450.00", "categoryId": groceries }),
    ];
    for step in &steps {
        let made = ok(&via_api, "budgets.set", step.clone());
        assert_eq!(made["amountLimit"], step["amountLimit"]);
        assert_eq!(made["type"], step["type"]);
        assert_eq!(made["scope"], if step.get("categoryId").is_some() { "CATEGORY" } else { "GLOBAL" });
        assert_eq!(made["effectiveFrom"], format!("{}-01", step["month"].as_str().unwrap()));
        let input = budgets::SetBudget {
            kind: if step["type"] == "INCOME" { CategoryType::Income } else { CategoryType::Expense },
            scope: if step.get("categoryId").is_some() { budgets::Scope::Category } else { budgets::Scope::Global },
            month: step["month"].as_str().unwrap().into(),
            amount_limit: Some(step["amountLimit"].as_str().unwrap().into()),
            category_id: step.get("categoryId").map(|c| c.as_str().unwrap().into()),
        };
        direct.with_conn(|c| budgets::set(c, &input, noon())).unwrap();
    }
    for month in ["2026-09", "2026-10", "2026-11", "2026-12", "2027-01"] {
        assert_eq!(ok(&via_api, "budgets.overview", json!({ "month": month })), ok(&direct, "budgets.overview", json!({ "month": month })), "{month}");
    }
    assert_eq!(ok(&via_api, "budgets.history", json!({})), ok(&direct, "budgets.history", json!({})));
    assert_eq!(ok(&via_api, "budgets.overview", json!({ "month": "2026-12" }))[1]["budgeted"], "2500.00");

    // Removing is setting nothing from that month on.
    ok(&via_api, "budgets.remove", json!({ "type": "EXPENSE", "month": "2027-02", "categoryId": groceries }));
    ok(&via_api, "budgets.remove", json!({ "type": "EXPENSE", "month": "2027-02" }));
    let overview = ok(&via_api, "budgets.overview", json!({ "month": "2027-02" }));
    let expense = overview.as_array().unwrap().iter().find(|o| o["type"] == "EXPENSE").unwrap();
    assert!(expense["budgeted"].is_null(), "{expense}");
    assert_eq!(ok(&via_api, "budgets.overview", json!({ "month": "2027-01" })).as_array().unwrap().iter().find(|o| o["type"] == "EXPENSE").unwrap()["budgeted"], "2500.00", "the month before keeps it");
    // Zero is a budget.
    assert_eq!(ok(&via_api, "budgets.set", json!({ "type": "EXPENSE", "month": "2027-03", "amountLimit": "0.00" }))["amountLimit"], "0.00");
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
fn recurring_items_are_made_changed_paid_and_ended() {
    let (_dir, vault) = seeded();
    let bills = category_id(&vault, "EXPENSE", "Rent");
    let made = ok(&vault, "recurring.create", json!({
        "name": "Gym", "amount": "29.99", "categoryId": bills, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 1, "description": "Monthly"
    }));
    let id = made["id"].as_str().unwrap().to_string();
    assert_eq!((made["amount"].clone(), made["nextDueDate"].clone(), made["isActive"].clone()), (json!("29.99"), json!("2026-10-20"), json!(true)));
    assert_eq!((made["intervalUnit"].clone(), made["intervalCount"].clone()), (json!("MONTH"), json!(1)));

    // A bill with no fixed amount: leave the amount out.
    let varying = ok(&vault, "recurring.create", json!({ "name": "Electricity", "categoryId": bills, "startDate": "2026-10-20", "intervalUnit": "MONTH", "intervalCount": 2 }));
    assert!(varying["amount"].is_null());
    let varying_id = varying["id"].as_str().unwrap().to_string();

    let changed = ok(&vault, "recurring.update", json!({ "id": id, "name": "Gym membership", "amount": "31.00", "endDate": "2027-12-31" }));
    assert_eq!((changed["name"].clone(), changed["amount"].clone(), changed["endDate"].clone()), (json!("Gym membership"), json!("31.00"), json!("2027-12-31")));
    assert_eq!(changed["description"], "Monthly", "left out stays");
    let cleared = ok(&vault, "recurring.update", json!({ "id": id, "endDate": null, "description": null }));
    assert!(cleared["endDate"].is_null() && cleared["description"].is_null());
    let no_amount = ok(&vault, "recurring.update", json!({ "id": id, "amount": null }));
    assert!(no_amount["amount"].is_null(), "null makes it a bill with no fixed amount");
    ok(&vault, "recurring.update", json!({ "id": id, "amount": "31.00" }));

    // Paying turns the occurrence into a transaction and moves on.
    let paid = ok(&vault, "recurring.pay", json!({ "id": id }));
    assert_eq!((paid["name"].clone(), paid["amount"].clone(), paid["paymentDate"].clone()), (json!("Gym membership"), json!("31.00"), json!(TODAY)));
    let after = ok(&vault, "recurring.list", json!({}));
    assert_eq!(after.as_array().unwrap().iter().find(|r| r["id"] == json!(id)).unwrap()["nextDueDate"], "2026-11-20");
    let odd = ok(&vault, "recurring.pay", json!({ "id": id, "amount": "28.40", "paidOn": "2026-11-18" }));
    assert_eq!((odd["amount"].clone(), odd["paymentDate"].clone()), (json!("28.40"), json!("2026-11-18")), "what was paid and when");
    // A bill with no amount cannot be paid without saying how much.
    assert_eq!(refused(&vault, "recurring.pay", json!({ "id": varying_id })), (400, "invalid_input".into()));
    assert_eq!(ok(&vault, "recurring.pay", json!({ "id": varying_id, "amount": "88.10" }))["amount"], "88.10");

    let skipped = ok(&vault, "recurring.skip", json!({ "id": id }));
    assert_ne!(skipped["nextDueDate"], json!("2026-11-20"));
    let paused = ok(&vault, "recurring.pause", json!({ "id": id }));
    assert_eq!(paused["isActive"], false);
    assert!(ok(&vault, "recurring.dues", json!({ "withinDays": 366 }))["upcoming"].as_array().unwrap().iter().all(|r| r["id"] != json!(id)), "a paused item is not due");
    assert_eq!(refused(&vault, "recurring.pay", json!({ "id": id })).0 / 100, 4, "a paused item cannot be paid");
    assert_eq!(ok(&vault, "recurring.resume", json!({ "id": id }))["isActive"], true);

    let ended = ok(&vault, "recurring.end", json!({ "id": id }));
    // The end date is today, or the start date if that is later (this item starts on the 20th).
    assert_eq!((ended["isActive"].clone(), ended["endDate"].clone()), (json!(false), json!("2026-10-20")));
    assert_eq!(refused(&vault, "recurring.resume", json!({ "id": id })).0, 400, "past its end date");

    // Deleting keeps what was paid.
    let paid_count = ok(&vault, "transactions.list", json!({ "q": "Gym membership" }))["total"].as_u64().unwrap();
    assert!(paid_count >= 2);
    assert_eq!(ok(&vault, "recurring.delete", json!({ "id": id })), Value::Null);
    assert_eq!(ok(&vault, "transactions.list", json!({ "q": "Gym membership" }))["total"].as_u64().unwrap(), paid_count);
    assert!(ok(&vault, "recurring.list", json!({})).as_array().unwrap().iter().all(|r| r["id"] != json!(id)));
    for name in ["recurring.delete", "recurring.pay", "recurring.skip", "recurring.pause", "recurring.resume", "recurring.end", "recurring.update"] {
        assert_eq!(refused(&vault, name, json!({ "id": id })).0, 404, "{name} on a deleted item");
    }
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
