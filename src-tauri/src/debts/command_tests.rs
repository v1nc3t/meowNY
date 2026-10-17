//! The commands of friends, the ledger and shared bills, called the way the page calls them: a name and a JSON body through Tauri's
//! message layer (its mock runtime), with the vault as managed state. `export_debts` takes the real window's handle and asks where to
//! save, so it cannot be called on a mock; what it writes is tested in `summary_tests.rs` and `report/debts.rs`. Its preview,
//! `export_debts_preview`, asks nothing and is called here.

use std::sync::Arc;

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::commands;
use crate::keys::memory::MemoryStore;
use crate::vault::{Settings, Vault};

struct Harness {
    _app: App<tauri::test::MockRuntime>,
    webview: WebviewWindow<tauri::test::MockRuntime>,
    vault: Arc<Vault>,
    _dir: tempfile::TempDir,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(Vault::new(dir.path().to_path_buf(), Box::new(MemoryStore::default())));
    vault.create(&Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() }, None).unwrap();
    let app = mock_builder()
        .manage(vault.clone())
        .invoke_handler(tauri::generate_handler![
            commands::create_category,
            commands::create_transaction,
            commands::list_transactions,
            commands::update_transaction,
            commands::delete_transaction,
            commands::create_recurring,
            commands::list_recurring,
            commands::pay_recurring,
            commands::budget_overview,
            commands::list_people,
            commands::create_person,
            commands::update_person,
            commands::archive_person,
            commands::unarchive_person,
            commands::person_statement,
            commands::debts_summary,
            commands::debts_due_payments,
            commands::debts_history,
            commands::export_debts_preview,
            commands::add_ledger_entry,
            commands::update_ledger_entry,
            commands::delete_ledger_entry,
            commands::settle_person,
            commands::split_preview,
            commands::set_transaction_split,
            commands::clear_transaction_split,
            commands::set_recurring_split,
            commands::clear_recurring_split,
            commands::document_counts,
            commands::bill_code,
            commands::bill_preview,
            commands::bill_accept,
            commands::debts_reminders,
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", WebviewUrl::default()).build().unwrap();
    Harness { _app: app, webview, vault, _dir: dir }
}

impl Harness {
    /// One command, with its arguments as the page would send them. The error is what the page would get: a fixed code.
    fn call(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(windows) { "http://tauri.localhost" } else { "tauri://localhost" }.parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        match get_ipc_response(&self.webview, request)? {
            InvokeResponseBody::Json(text) => Ok(serde_json::from_str(&text).unwrap()),
            InvokeResponseBody::Raw(bytes) => panic!("a raw answer of {} bytes", bytes.len()),
        }
    }

    fn ok(&self, cmd: &str, args: Value) -> Value {
        self.call(cmd, args.clone()).unwrap_or_else(|e| panic!("{cmd} {args}: {e}"))
    }

    fn code(&self, cmd: &str, args: Value) -> String {
        match self.call(cmd, args) {
            Err(Value::String(code)) => code,
            other => panic!("{cmd} should have failed with a code, not {other:?}"),
        }
    }

    fn category(&self) -> String {
        self.ok("create_category", json!({ "name": "Food", "kind": "EXPENSE" }))["id"].as_str().unwrap().to_string()
    }

    fn bill(&self, category: &str, amount: &str) -> String {
        self.ok("create_transaction", json!({ "input": { "name": "Dinner", "amount": amount, "paymentDate": "2026-10-10", "categoryId": category } }))["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn friend(&self, name: &str) -> String {
        self.ok("create_person", json!({ "name": name }))["id"].as_str().unwrap().to_string()
    }
}

#[test]
fn a_friend_is_made_changed_archived_and_listed_as_the_page_sees_it() {
    let h = harness();
    let anna = h.ok("create_person", json!({ "name": " Anna " }));
    assert_eq!((anna["name"].as_str(), anna["balance"].as_str(), anna["archived"].as_bool()), (Some("Anna"), Some("0.00"), Some(false)));
    assert!(anna["id"].is_string(), "ids are text");
    let id = anna["id"].as_str().unwrap().to_string();
    assert_eq!(h.code("create_person", json!({ "name": "anna" })), "name_exists");
    assert_eq!(h.code("create_person", json!({ "name": "" })), "invalid_input");
    let changed = h.ok("update_person", json!({ "id": id, "patch": { "name": "Anne", "remindOn": "2026-11-01", "remindEveryDays": 14 } }));
    assert_eq!((changed["name"].as_str(), changed["remindOn"].as_str(), changed["remindEveryDays"].as_i64()), (Some("Anne"), Some("2026-11-01"), Some(14)));
    assert_eq!(h.ok("update_person", json!({ "id": id, "patch": { "remindOn": null } }))["remindEveryDays"], Value::Null, "no date, no repeat");
    assert_eq!(h.code("update_person", json!({ "id": id, "patch": { "remindEveryDays": 7 } })), "invalid_input");
    let refused = h.call("update_person", json!({ "id": id, "patch": { "phone": "555" } })).unwrap_err();
    assert!(refused.as_str().is_some_and(|m| m.contains("unknown field")), "nothing else about a friend is kept: {refused}");
    assert_eq!(h.ok("archive_person", json!({ "id": id }))["archived"], true);
    assert_eq!(h.ok("list_people", json!({ "archived": false })).as_array().unwrap().len(), 0);
    assert_eq!(h.ok("list_people", json!({ "archived": true })).as_array().unwrap().len(), 1);
    assert_eq!(h.ok("unarchive_person", json!({ "id": id }))["archived"], false);
    assert_eq!(h.code("archive_person", json!({ "id": "999" })), "not_found");
    assert_eq!(h.code("archive_person", json!({ "id": "abc" })), "invalid_input");
}

#[test]
fn a_bill_is_previewed_shared_rescaled_and_cleared_and_the_page_gets_both_amounts() {
    let h = harness();
    let (food, anna, bob) = (h.category(), h.friend("Anna"), h.friend("Bob"));
    let bill = h.bill(&food, "90.00");
    let input = json!({ "mode": "EQUAL", "parts": [{ "personId": anna }, { "personId": bob }, { "guest": "Carl" }] });
    let shown = h.ok("split_preview", json!({ "amount": "90.00", "input": input }));
    assert_eq!((shown["complete"].as_bool(), shown["mine"].as_str(), shown["parts"].as_array().unwrap().len()), (Some(true), Some("22.50"), 3));
    assert_eq!(h.ok("list_transactions", json!({ "filter": {} }))["transactions"][0]["split"].as_array().unwrap().len(), 0, "a preview writes nothing");
    let saved = h.ok("set_transaction_split", json!({ "id": bill, "input": input }));
    assert_eq!((saved["amount"].as_str(), saved["myShare"].as_str(), saved["split"].as_array().unwrap().len()), (Some("90.00"), Some("22.50"), 3));
    assert_eq!(saved["split"][0]["person"], "Anna");
    assert_eq!(saved["split"][2]["guest"], "Carl");
    let rescaled = h.ok("update_transaction", json!({ "id": bill, "patch": { "amount": "40.00" } }));
    assert_eq!((rescaled["myShare"].as_str(), rescaled["split"][0]["amount"].as_str()), (Some("10.00"), Some("10.00")));
    assert_eq!(h.ok("list_people", json!({ "archived": false }))[0]["balance"], "10.00");
    let cleared = h.ok("clear_transaction_split", json!({ "id": bill }));
    assert_eq!((cleared["myShare"].as_str(), cleared["split"].as_array().unwrap().len()), (Some("40.00"), 0));
    // refusals are fixed codes
    assert_eq!(h.code("set_transaction_split", json!({ "id": bill, "input": { "mode": "PERCENT", "parts": [{ "personId": anna, "value": "101.00" }] } })), "invalid_input");
    assert_eq!(h.code("set_transaction_split", json!({ "id": bill, "input": { "mode": "EQUAL", "parts": [] } })), "invalid_input");
    assert_eq!(h.code("set_transaction_split", json!({ "id": "999", "input": input })), "not_found");
    let refused = h.call("set_transaction_split", json!({ "id": bill, "input": { "mode": "EQUAL", "parts": [{ "personId": anna }], "extra": 1 } })).unwrap_err();
    assert!(refused.as_str().is_some_and(|m| m.contains("unknown field")), "{refused}");
    assert_eq!(h.code("split_preview", json!({ "amount": "0.00", "input": input })), "invalid_input");
}

#[test]
fn the_ledger_is_kept_and_a_payment_larger_than_the_debt_turns_the_balance_around() {
    let h = harness();
    let (food, anna) = (h.category(), h.friend("Anna"));
    let loan = h.ok("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "25.00", "date": "2026-10-01", "note": "taxi" } }));
    assert_eq!((loan["kind"].as_str(), loan["amount"].as_str()), (Some("LENT"), Some("25.00")));
    let changed = h.ok("update_ledger_entry", json!({ "id": loan["id"], "patch": { "amount": "30.00", "note": null } }));
    assert_eq!((changed["amount"].as_str(), changed["note"].clone()), (Some("30.00"), Value::Null));
    let paid = h.ok("settle_person", json!({ "id": anna, "amount": "40.00", "date": "2026-10-10" }));
    assert_eq!((paid["before"].as_str(), paid["after"].as_str(), paid["over"].as_str()), (Some("30.00"), Some("-10.00"), Some("10.00")));
    let story = h.ok("person_statement", json!({ "id": anna }));
    assert_eq!(story["lines"].as_array().unwrap().len(), 2);
    assert_eq!(story["person"]["balance"], "-10.00");
    let summary = h.ok("debts_summary", json!({}));
    assert_eq!((summary["owedToYou"].as_str(), summary["youOwe"].as_str(), summary["net"].as_str()), (Some("0.00"), Some("10.00"), Some("-10.00")));
    // a borrowing that adds the person's share to their expenses
    let borrowed = h.ok(
        "add_ledger_entry",
        json!({ "input": { "personId": anna, "kind": "BORROWED", "amount": "12.00", "date": "2026-10-11" }, "expense": { "name": "Pizza", "categoryId": food } }),
    );
    assert!(borrowed["transactionId"].is_string());
    assert_eq!(h.ok("list_transactions", json!({ "filter": { "q": "Pizza" } }))["total"], 1);
    assert_eq!(h.code("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "1.00", "date": "2026-10-11" }, "expense": { "name": "X", "categoryId": food } })), "invalid_input");
    assert_eq!(h.code("settle_person", json!({ "id": h.friend("Bob") })), "invalid_input", "nothing to settle");
    h.ok("delete_ledger_entry", json!({ "id": loan["id"] }));
    assert_eq!(h.code("delete_ledger_entry", json!({ "id": loan["id"] })), "not_found");
}

#[test]
fn a_recurring_item_is_split_and_its_payment_comes_split() {
    let h = harness();
    let (food, anna) = (h.category(), h.friend("Anna"));
    let item = h.ok(
        "create_recurring",
        json!({ "input": { "name": "Flat", "amount": "900.00", "categoryId": food, "startDate": "2026-10-01", "intervalUnit": "MONTH", "intervalCount": 1 } }),
    );
    let id = item["id"].as_str().unwrap().to_string();
    assert!(item["split"].is_null());
    let templated = h.ok("set_recurring_split", json!({ "id": id, "input": { "mode": "SHARES", "mine": "2", "parts": [{ "personId": anna, "value": "1" }] } }));
    assert_eq!((templated["split"]["mine"].as_u64(), templated["split"]["parts"][0]["weight"].as_u64()), (Some(2), Some(1)));
    let paid = h.ok("pay_recurring", json!({ "id": id, "paidOn": "2026-10-01" }));
    assert_eq!((paid["myShare"].as_str(), paid["split"][0]["amount"].as_str()), (Some("600.00"), Some("300.00")));
    assert!(h.ok("clear_recurring_split", json!({ "id": id }))["split"].is_null());
    assert_eq!(h.code("set_recurring_split", json!({ "id": "999", "input": { "mode": "EQUAL", "parts": [{ "personId": anna }] } })), "not_found");
}

#[test]
fn a_code_is_shown_read_and_taken_in_as_the_page_does_it() {
    let h = harness();
    let (anna, category) = (h.friend("Anna"), h.category());
    let bill = h.bill(&category, "90.00");
    h.ok("set_transaction_split", json!({ "id": bill, "input": { "mode": "EQUAL", "parts": [{ "personId": anna }, { "guest": "Carl" }] } }));
    let code = h.ok("bill_code", json!({ "transactionId": bill, "personId": anna, "from": "Vincent" }));
    let text = code.as_str().unwrap().to_string();
    assert!(text.starts_with("MNB1:"));
    assert_eq!(h.code("bill_code", json!({ "transactionId": bill, "personId": anna, "from": "" })), "invalid_input");
    assert_eq!(h.code("bill_code", json!({ "transactionId": bill, "personId": "999", "from": "Vincent" })), "not_found");
    // the same app as the receiver here: the preview says what it holds, and writes nothing
    let shown = h.ok("bill_preview", json!({ "text": text }));
    assert_eq!((shown["from"].as_str(), shown["amount"].as_str(), shown["total"].as_str(), shown["sameCurrency"].as_bool(), shown["alreadyAdded"].as_bool()), (Some("Vincent"), Some("30.00"), Some("90.00"), Some(true), Some(false)));
    assert_eq!(h.code("bill_preview", json!({ "text": "MNY1:abc" })), "invalid_input");
    // taken in with a new friend and an expense; a field the page does not know is refused
    let accepted = h.ok("bill_accept", json!({ "input": { "text": text, "newFriend": "Vincent", "addExpense": true, "categoryId": category } }));
    assert_eq!((accepted["kind"].as_str(), accepted["amount"].as_str(), accepted["note"].as_str()), (Some("BORROWED"), Some("30.00"), Some("Dinner")));
    assert!(accepted["transactionId"].is_string());
    assert_eq!(h.ok("bill_preview", json!({ "text": text }))["alreadyAdded"], true);
    assert_eq!(h.code("bill_accept", json!({ "input": { "text": text, "newFriend": "Again", "addExpense": false } })), "invalid_input");
    assert!(h.call("bill_accept", json!({ "input": { "text": text, "newFriend": "Again", "addExpense": false, "extra": 1 } })).is_err(), "a field the page does not know is refused before the command runs");
}

#[test]
fn reminders_follow_the_period_the_device_asks_for_from_the_day_each_debt_last_changed() {
    let h = harness();
    let anna = h.friend("Anna");
    assert_eq!(h.ok("debts_reminders", json!({ "everyDays": 30 }))["due"], json!([]), "nobody owes anything, so nothing is live");
    h.ok("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "25.00", "date": "2020-01-01" } }));
    let live = h.ok("debts_reminders", json!({ "everyDays": 30 }));
    assert_eq!((live["due"][0]["name"].as_str(), live["due"][0]["personId"].as_str()), (Some("Anna"), Some(anna.as_str())));
    assert_eq!(live["upcoming"].as_array().unwrap().len(), 4);
    assert!(live["today"].as_str().unwrap().starts_with("20"));
    // none when the device says none, and a period that is too long or missing is refused
    assert_eq!(h.ok("debts_reminders", json!({ "everyDays": 0 }))["due"], json!([]));
    assert_eq!(h.code("debts_reminders", json!({ "everyDays": 366 })), "invalid_input");
    assert!(h.call("debts_reminders", json!({})).is_err());
    // a debt that changed today is not due for another period
    h.ok("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "1.00", "date": "2100-01-01" } }));
    assert_eq!(h.ok("debts_reminders", json!({ "everyDays": 30 }))["due"], json!([]), "the latest line is in the future: the count has not begun");
}

#[test]
fn documents_can_be_counted_on_ledger_entries_and_a_locked_app_answers_every_command_with_locked() {
    let h = harness();
    let anna = h.friend("Anna");
    let loan = h.ok("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "25.00", "date": "2026-10-01" } }));
    let id = loan["id"].as_str().unwrap().to_string();
    assert_eq!(h.ok("document_counts", json!({ "kind": "ledger_entry", "ids": [id] })), json!({}));
    h.vault.lock();
    for (cmd, args) in [
        ("list_people", json!({ "archived": false })),
        ("create_person", json!({ "name": "Bob" })),
        ("update_person", json!({ "id": "1", "patch": {} })),
        ("archive_person", json!({ "id": "1" })),
        ("unarchive_person", json!({ "id": "1" })),
        ("person_statement", json!({ "id": "1" })),
        ("debts_summary", json!({})),
        ("debts_due_payments", json!({})),
        ("debts_history", json!({ "filter": {} })),
        ("export_debts_preview", json!({ "request": { "what": "SUMMARY", "format": "TEXT", "labels": english_labels() } })),
        ("add_ledger_entry", json!({ "input": { "personId": "1", "kind": "LENT", "amount": "1.00", "date": "2026-10-01" } })),
        ("update_ledger_entry", json!({ "id": "1", "patch": {} })),
        ("delete_ledger_entry", json!({ "id": "1" })),
        ("settle_person", json!({ "id": "1" })),
        ("split_preview", json!({ "amount": "1.00", "input": { "mode": "EQUAL", "parts": [{ "guest": "A" }] } })),
        ("set_transaction_split", json!({ "id": "1", "input": { "mode": "EQUAL", "parts": [{ "guest": "A" }] } })),
        ("clear_transaction_split", json!({ "id": "1" })),
        ("set_recurring_split", json!({ "id": "1", "input": { "mode": "EQUAL", "parts": [{ "guest": "A" }] } })),
        ("clear_recurring_split", json!({ "id": "1" })),
        ("bill_code", json!({ "transactionId": "1", "personId": "1", "from": "Me" })),
        ("bill_preview", json!({ "text": "MNB1:x" })),
        ("bill_accept", json!({ "input": { "text": "MNB1:x", "newFriend": "A", "addExpense": false } })),
        ("debts_reminders", json!({ "everyDays": 7 })),
    ] {
        assert_eq!(h.code(cmd, args), "locked", "{cmd}");
    }
}

/// A recurring item that started and ended long ago, so what is due does not depend on the day the test runs: monthly from `start`, ending on
/// `end`.
fn old_item(h: &Harness, name: &str, amount: Option<&str>, category: &str, start: &str, end: &str) -> String {
    let mut input = json!({ "name": name, "categoryId": category, "startDate": start, "endDate": end, "intervalUnit": "MONTH", "intervalCount": 1 });
    if let Some(amount) = amount {
        input["amount"] = json!(amount);
    }
    h.ok("create_recurring", json!({ "input": input }))["id"].as_str().unwrap().to_string()
}

#[test]
fn the_recurring_payments_that_have_come_due_are_debts_and_paying_one_moves_it_to_the_history() {
    let h = harness();
    let food = h.category();
    let salary = h.ok("create_category", json!({ "name": "Salary", "kind": "INCOME" }))["id"].as_str().unwrap().to_string();
    let flat = old_item(&h, "Flat", Some("900.00"), &food, "2020-01-01", "2020-03-15");
    old_item(&h, "Phone", None, &food, "2020-06-01", "2020-06-15");
    old_item(&h, "Wages", Some("3000.00"), &salary, "2020-01-01", "2020-01-15");
    let later = h.ok(
        "create_recurring",
        json!({ "input": { "name": "Next century", "amount": "5.00", "categoryId": food, "startDate": "2099-01-01", "intervalUnit": "MONTH", "intervalCount": 1 } }),
    );
    assert!(later["id"].is_string());

    // three occurrences of the flat (a rent not paid for three months is three debts) and one of the phone bill; the wages are not owed
    let due = h.ok("debts_due_payments", json!({}));
    let names: Vec<(&str, &str)> = due["payments"].as_array().unwrap().iter().map(|p| (p["name"].as_str().unwrap(), p["dueDate"].as_str().unwrap())).collect();
    assert_eq!(names, [("Flat", "2020-01-01"), ("Flat", "2020-02-01"), ("Flat", "2020-03-01"), ("Phone", "2020-06-01")]);
    assert_eq!((due["total"].as_str(), due["unknown"].as_u64()), (Some("2700.00"), Some(1)), "the phone bill says only that it is due");
    let first = &due["payments"][0];
    assert_eq!((first["amount"].as_str(), first["category"].as_str(), first["recurringId"].as_str()), (Some("900.00"), Some("Food"), Some(flat.as_str())));
    assert!(first["daysOverdue"].as_u64().unwrap() > 2_000, "since 2020");
    assert!(due["payments"][3]["amount"].is_null());

    // paying the oldest moves it from the due payments to the history, with the day it fell due
    h.ok("pay_recurring", json!({ "id": flat, "amount": null, "paidOn": "2020-01-05" }));
    let after = h.ok("debts_due_payments", json!({}));
    assert_eq!(after["payments"].as_array().unwrap().len(), 3);
    assert_eq!(after["total"], "1800.00");
    let history = h.ok("debts_history", json!({ "filter": {} }));
    let lines = history["lines"].as_array().unwrap();
    assert_eq!(history["total"], 4, "one paid and three due");
    // newest first: the due ones of 2020-06, 2020-03 and 2020-02, then the payment of 2020-01-05
    let kinds: Vec<(&str, &str)> = lines.iter().map(|l| (l["kind"].as_str().unwrap(), l["date"].as_str().unwrap())).collect();
    assert_eq!(kinds, [("PAYMENT_DUE", "2020-06-01"), ("PAYMENT_DUE", "2020-03-01"), ("PAYMENT_DUE", "2020-02-01"), ("PAYMENT_PAID", "2020-01-05")]);
    let paid = &lines[3];
    assert_eq!((paid["what"].as_str(), paid["amount"].as_str(), paid["dueDate"].as_str()), (Some("Flat"), Some("900.00"), Some("2020-01-01")));
    assert!(paid["friend"].is_null() && paid["balance"].is_null() && paid["transactionId"].is_string());
}

#[test]
fn the_history_of_debts_has_the_lines_of_the_friends_and_the_payments_and_filters_by_date_and_friend() {
    let h = harness();
    let food = h.category();
    let (anna, bob) = (h.friend("Anna"), h.friend("Bob"));
    for (person, kind, amount, date) in [(&anna, "LENT", "25.00", "2026-01-02"), (&bob, "BORROWED", "7.50", "2026-02-02"), (&anna, "REPAID_TO_ME", "10.00", "2026-03-04")] {
        h.ok("add_ledger_entry", json!({ "input": { "personId": person, "kind": kind, "amount": amount, "date": date } }));
    }
    old_item(&h, "Phone", Some("30.00"), &food, "2020-06-01", "2020-06-15");
    let all = h.ok("debts_history", json!({ "filter": {} }));
    let line = |at: usize| (all["lines"][at]["date"].as_str().unwrap().to_string(), all["lines"][at]["kind"].as_str().unwrap().to_string());
    assert_eq!(all["total"], 4);
    assert_eq!(line(0), ("2026-03-04".into(), "REPAID_TO_ME".into()));
    assert_eq!(line(3), ("2020-06-01".into(), "PAYMENT_DUE".into()));
    // a friend's lines carry the balance after them, the debts screens' own
    assert_eq!((all["lines"][0]["friend"].as_str(), all["lines"][0]["effect"].as_str(), all["lines"][0]["balance"].as_str()), (Some("Anna"), Some("-10.00"), Some("15.00")));

    // one friend: no recurring payments
    let only_anna = h.ok("debts_history", json!({ "filter": { "personId": anna } }));
    assert_eq!(only_anna["total"], 2);
    assert!(only_anna["lines"].as_array().unwrap().iter().all(|l| l["friend"] == "Anna"));
    // dates, both ends inclusive
    let february = h.ok("debts_history", json!({ "filter": { "from": "2026-02-02", "to": "2026-02-02" } }));
    assert_eq!((february["total"].as_u64(), february["lines"][0]["friend"].as_str()), (Some(1), Some("Bob")));
    // a page at a time
    let page = h.ok("debts_history", json!({ "filter": { "limit": 2, "offset": 1 } }));
    assert_eq!((page["total"].as_u64(), page["lines"].as_array().unwrap().len(), page["offset"].as_u64()), (Some(4), 2, Some(1)));
    assert_eq!(page["lines"][0]["date"], all["lines"][1]["date"]);

    // what cannot be asked
    for bad in [json!({ "limit": 0 }), json!({ "limit": 201 }), json!({ "from": "2026-03-01", "to": "2026-02-01" }), json!({ "from": "yesterday" }), json!({ "personId": "x" })] {
        assert_eq!(h.code("debts_history", json!({ "filter": bad.clone() })), "invalid_input", "{bad}");
    }
    // an argument it does not know is refused before the command runs
    assert!(h.call("debts_history", json!({ "filter": { "unknown": 1 } })).is_err());
}

/// The fixed words of the files, as the page sends them (English).
fn english_labels() -> Value {
    let mut labels = serde_json::Map::new();
    for key in [
        "summaryTitle", "statementTitle", "historyTitle", "dueTitle", "dueTotal", "paidPayment", "duePayment", "owedToYou", "youOwe", "net", "owesYou", "youOweThem", "nobody", "settled", "lent",
        "borrowed", "repaidToMe", "repaidByMe", "bill", "date", "what", "change", "balance", "name", "amount", "page",
    ] {
        labels.insert(key.to_string(), json!(key));
    }
    Value::Object(labels)
}

#[test]
fn the_preview_of_a_debts_file_is_the_file_that_would_be_saved_and_a_choice_that_makes_no_file_is_refused() {
    let h = harness();
    let anna = h.friend("Anna");
    h.ok("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "25.00", "date": "2026-01-02" } }));
    let ask = |what: &str, format: &str, person: Option<&str>| {
        let mut request = json!({ "what": what, "format": format, "labels": english_labels() });
        if let Some(id) = person {
            request["personId"] = json!(id);
        }
        h.call("export_debts_preview", json!({ "request": request }))
    };
    let text = |answer: &Value| String::from_utf8(crate::sync::qr::base64url_decode(answer["data"].as_str().unwrap()).unwrap()).unwrap();

    let summary = ask("SUMMARY", "TEXT", None).unwrap();
    let written = text(&summary);
    assert_eq!(summary["bytes"].as_u64(), Some(written.len() as u64));
    assert!(written.contains("Anna") && written.contains("25.00"), "{written}");
    let csv = text(&ask("LEDGER", "CSV", None).unwrap());
    assert!(csv.contains("Anna") && (csv.contains("LENT") || csv.contains("lent")), "{csv}");
    let statement = text(&ask("STATEMENT", "TEXT", Some(&anna)).unwrap());
    assert!(statement.contains("25.00"), "{statement}");
    let pdf = crate::sync::qr::base64url_decode(ask("SUMMARY", "PDF", None).unwrap()["data"].as_str().unwrap()).unwrap();
    assert!(pdf.starts_with(b"%PDF"), "a PDF");

    // what makes no file: a statement with no friend, a ledger as a PDF, labels that are not plain text
    assert_eq!(match ask("STATEMENT", "TEXT", None) { Err(Value::String(code)) => code, other => panic!("{other:?}") }, "invalid_input");
    assert_eq!(match ask("LEDGER", "PDF", None) { Err(Value::String(code)) => code, other => panic!("{other:?}") }, "invalid_input");
    let mut bad = english_labels();
    bad["net"] = json!("two\nlines");
    assert_eq!(h.code("export_debts_preview", json!({ "request": { "what": "SUMMARY", "format": "TEXT", "labels": bad } })), "invalid_input");
}
