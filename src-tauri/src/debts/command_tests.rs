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
            commands::recurring_code,
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

    fn friend(&self, name: &str) -> String {
        self.ok("create_person", json!({ "name": name }))["id"].as_str().unwrap().to_string()
    }
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
        ("recurring_code", json!({ "recurringId": "1", "personId": "1", "from": "Me" })),
        ("bill_preview", json!({ "text": "MNB1:x" })),
        ("bill_accept", json!({ "input": { "text": "MNB1:x", "newFriend": "A", "addExpense": false } })),
        ("debts_reminders", json!({ "everyDays": 7 })),
    ] {
        assert_eq!(h.code(cmd, args), "locked", "{cmd}");
    }
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
