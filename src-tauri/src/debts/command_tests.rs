//! The commands of friends, the ledger and shared bills, called the way the page calls them: a name and a JSON body through Tauri's
//! message layer (its mock runtime), with the vault as managed state. `export_debts` takes the real window's handle and asks where to
//! save, so it cannot be called on a mock; what it writes is tested in `summary_tests.rs` and `report/debts.rs`.

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
fn reminders_are_listed_for_the_page_with_the_days_to_come() {
    let h = harness();
    let anna = h.friend("Anna");
    assert_eq!(h.ok("debts_reminders", json!({}))["due"], json!([]));
    h.ok("update_person", json!({ "id": anna, "patch": { "remindOn": "2020-01-01", "remindEveryDays": 30 } }));
    assert_eq!(h.ok("debts_reminders", json!({}))["due"], json!([]), "nobody owes anything, so nothing is live");
    h.ok("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "25.00", "date": "2026-10-01" } }));
    let live = h.ok("debts_reminders", json!({}));
    assert_eq!((live["due"][0]["name"].as_str(), live["due"][0]["personId"].as_str()), (Some("Anna"), Some(anna.as_str())));
    assert_eq!(live["upcoming"].as_array().unwrap().len(), 4);
    assert!(live["today"].as_str().unwrap().starts_with("20"));
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
        ("debts_reminders", json!({})),
    ] {
        assert_eq!(h.code(cmd, args), "locked", "{cmd}");
    }
}
