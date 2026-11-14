//! The commands of accounts and transfers, called the way the page calls them: a name and a JSON body through Tauri's message layer (its
//! mock runtime), with the vault as managed state. What crosses is checked here: the names of the arguments, the JSON of the answers
//! (`docs/accounts-design.md`, section 6) and the fixed code of each refusal.

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
            commands::today,
            commands::create_category,
            commands::create_transaction,
            commands::list_transactions,
            commands::update_transaction,
            commands::create_recurring,
            commands::update_recurring,
            commands::pay_recurring,
            commands::create_person,
            commands::add_ledger_entry,
            commands::update_ledger_entry,
            commands::settle_person,
            commands::list_accounts,
            commands::create_account,
            commands::update_account,
            commands::archive_account,
            commands::unarchive_account,
            commands::delete_account,
            commands::reorder_accounts,
            commands::account_statement,
            commands::account_history,
            commands::accounts_summary,
            commands::create_transfer,
            commands::get_transfer,
            commands::update_transfer,
            commands::delete_transfer,
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

    fn id_of(&self, cmd: &str, args: Value) -> String {
        self.ok(cmd, args)["id"].as_str().unwrap().to_string()
    }

    fn names(&self) -> Vec<String> {
        self.ok("list_accounts", json!({})).as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap().to_string()).collect()
    }
}

#[test]
fn accounts_are_made_changed_ordered_archived_restored_and_deleted_through_the_commands() {
    let h = harness();
    let today = h.ok("today", json!({}))["date"].as_str().unwrap().to_string();
    // the main account is there from the start, with every field of the answer
    assert_eq!(
        h.ok("list_accounts", json!({})),
        json!([{
            "id": "1", "name": "Main account", "kind": "CHECKING", "number": null, "openingBalance": "0.00", "openingDate": "1970-01-01", "archived": false,
            "isMain": true, "balance": "0.00", "changeThisMonth": "0.00", "lastActivity": null, "entries": 0
        }])
    );
    let card = h.ok("create_account", json!({ "input": { "name": "Card", "kind": "CREDIT_CARD", "number": "1234", "openingBalance": "-120.00", "openingDate": "2026-01-01" } }));
    assert_eq!(
        card,
        json!({
            "id": "2", "name": "Card", "kind": "CREDIT_CARD", "number": "1234", "openingBalance": "-120.00", "openingDate": "2026-01-01", "archived": false,
            "isMain": false, "balance": "-120.00", "changeThisMonth": "0.00", "lastActivity": null, "entries": 0
        })
    );
    // only the name and the kind are needed: the balance is 0.00 and the date is today
    let cash = h.ok("create_account", json!({ "input": { "name": "Cash", "kind": "CASH" } }));
    assert_eq!((cash["openingBalance"].as_str(), cash["openingDate"].as_str(), cash["number"].clone()), (Some("0.00"), Some(today.as_str()), Value::Null));

    let changed = h.ok("update_account", json!({ "id": "2", "patch": { "name": "Visa", "number": null, "openingBalance": "-100.00" } }));
    assert_eq!((changed["name"].as_str(), changed["number"].clone(), changed["balance"].as_str(), changed["kind"].as_str()), (Some("Visa"), Value::Null, Some("-100.00"), Some("CREDIT_CARD")));
    assert_eq!(h.ok("reorder_accounts", json!({ "ids": ["3", "1", "2"] })), Value::Null);
    assert_eq!(h.names(), ["Cash", "Main account", "Visa"]);

    assert_eq!(h.ok("archive_account", json!({ "id": "3" }))["archived"], true);
    assert_eq!(h.ok("archive_account", json!({ "id": "3" }))["archived"], true, "again: the account as it is");
    assert_eq!(h.names(), ["Main account", "Visa", "Cash"], "archived accounts come last");
    assert_eq!(h.ok("unarchive_account", json!({ "id": "3" }))["archived"], false);
    assert_eq!(h.ok("delete_account", json!({ "id": "3" })), Value::Null);
    assert_eq!(h.names(), ["Main account", "Visa"]);

    // the refusals, as the fixed codes the page gets
    assert_eq!(h.code("create_account", json!({ "input": { "name": "visa", "kind": "CASH" } })), "name_exists");
    assert_eq!(h.code("update_account", json!({ "id": "2", "patch": { "name": "main account" } })), "name_exists");
    assert_eq!(h.code("create_account", json!({ "input": { "name": "", "kind": "CASH" } })), "invalid_input");
    assert_eq!(h.code("create_account", json!({ "input": { "name": "A", "kind": "CASH", "openingBalance": "-0.00" } })), "invalid_input");
    assert_eq!(h.code("archive_account", json!({ "id": "1" })), "invalid_input");
    assert_eq!(h.code("delete_account", json!({ "id": "1" })), "invalid_input");
    assert_eq!(h.code("reorder_accounts", json!({ "ids": ["1"] })), "invalid_input");
    assert_eq!(h.code("reorder_accounts", json!({ "ids": ["1", "x"] })), "invalid_input");
    assert_eq!(h.code("update_account", json!({ "id": "x", "patch": {} })), "invalid_input");
    for cmd in ["archive_account", "unarchive_account", "delete_account"] {
        assert_eq!(h.code(cmd, json!({ "id": "99" })), "not_found", "{cmd}");
    }
    assert_eq!(h.code("update_account", json!({ "id": "99", "patch": {} })), "not_found");
    // what does not have the shape of the input does not reach the code at all
    assert!(h.call("create_account", json!({ "input": { "name": "A", "kind": "LOAN" } })).is_err());
    assert!(h.call("create_account", json!({ "input": { "name": "A", "kind": "CASH", "archived": true } })).is_err());
    assert!(h.call("create_account", json!({ "input": { "name": "A", "kind": "CASH", "openingBalance": -1.5 } })).is_err(), "money is text");
    assert!(h.call("update_account", json!({ "id": "2", "patch": { "isMain": true } })).is_err());
    assert_eq!(h.names(), ["Main account", "Visa"], "nothing of that was written");
}

#[test]
fn transfers_and_rows_with_an_account_go_through_the_commands_and_the_numbers_follow() {
    let h = harness();
    let today = h.ok("today", json!({}));
    let (date, month) = (today["date"].as_str().unwrap(), today["month"].as_str().unwrap());
    let year = &date[..4];
    let first = format!("{month}-01");
    let food = h.id_of("create_category", json!({ "name": "Food", "kind": "EXPENSE" }));
    let anna = h.id_of("create_person", json!({ "name": "Anna" }));
    let cash = h.id_of("create_account", json!({ "input": { "name": "Cash", "kind": "CASH" } }));

    let moved = h.ok("create_transfer", json!({ "input": { "fromAccountId": "1", "toAccountId": cash, "amount": "200.00", "date": first, "note": "pocket money" } }));
    assert_eq!(moved, json!({ "id": "1", "fromAccountId": "1", "toAccountId": cash, "amount": "200.00", "date": first, "note": "pocket money" }));
    assert_eq!(h.ok("get_transfer", json!({ "id": "1" })), moved);
    let edited = h.ok("update_transfer", json!({ "id": "1", "patch": { "amount": "150.00", "note": null } }));
    assert_eq!((edited["amount"].as_str(), edited["note"].clone(), edited["toAccountId"].as_str()), (Some("150.00"), Value::Null, Some(cash.as_str())));

    // a transaction, a recurring item and its payment, a ledger entry and a repayment, each in the cash account
    let shop = h.ok("create_transaction", json!({ "input": { "name": "Shop", "amount": "12.50", "paymentDate": first, "categoryId": food, "accountId": cash } }));
    assert_eq!(shop["accountId"], json!(cash));
    assert_eq!(h.ok("create_transaction", json!({ "input": { "name": "Shop", "amount": "1.00", "paymentDate": first, "categoryId": food } }))["accountId"], "1", "none named: the main account");
    assert_eq!(h.ok("update_transaction", json!({ "id": shop["id"], "patch": { "accountId": "1" } }))["accountId"], "1");
    assert_eq!(h.ok("update_transaction", json!({ "id": shop["id"], "patch": { "accountId": cash } }))["accountId"], json!(cash));
    assert_eq!(h.ok("list_transactions", json!({ "filter": { "accountId": cash } }))["total"], 1);
    let rent = h.ok("create_recurring", json!({ "input": { "name": "Gym", "amount": "30.00", "categoryId": food, "startDate": first, "intervalUnit": "MONTH", "intervalCount": 1, "accountId": cash } }));
    assert_eq!(rent["accountId"], json!(cash));
    assert_eq!(h.ok("pay_recurring", json!({ "id": rent["id"], "paidOn": first }))["accountId"], json!(cash));
    assert_eq!(h.ok("update_recurring", json!({ "id": rent["id"], "patch": { "accountId": "1" } }))["accountId"], "1");
    let loan = h.ok("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "20.00", "date": first, "accountId": cash } }));
    assert_eq!(loan["accountId"], json!(cash));
    assert_eq!(h.ok("update_ledger_entry", json!({ "id": loan["id"], "patch": { "accountId": "1" } }))["accountId"], "1");
    let paid = h.ok("settle_person", json!({ "id": anna, "date": first, "accountId": cash }));
    assert_eq!((paid["entry"]["accountId"].clone(), paid["entry"]["kind"].as_str()), (json!(cash), Some("REPAID_TO_ME")));

    // cash: +150.00 - 12.50 - 30.00 + 20.00; the main account: -150.00 - 1.00 - 20.00
    let accounts = h.ok("list_accounts", json!({}));
    assert_eq!((accounts[0]["balance"].as_str(), accounts[0]["entries"].as_u64()), (Some("-171.00"), Some(4)));
    assert_eq!(
        (accounts[1]["balance"].as_str(), accounts[1]["changeThisMonth"].as_str(), accounts[1]["lastActivity"].as_str(), accounts[1]["entries"].as_u64()),
        (Some("127.50"), Some("127.50"), Some(first.as_str()), Some(4))
    );
    let statement = h.ok("account_statement", json!({ "id": cash, "from": first, "to": date }));
    assert_eq!((statement["from"].as_str(), statement["to"].as_str(), statement["balanceBefore"].as_str(), statement["balanceAfter"].as_str()), (Some(first.as_str()), Some(date), Some("0.00"), Some("127.50")));
    assert_eq!(statement["account"]["name"], "Cash");
    let lines: Vec<(&str, &str, &str)> = statement["lines"].as_array().unwrap().iter().map(|l| (l["source"].as_str().unwrap(), l["other"].as_str().unwrap(), l["amount"].as_str().unwrap())).collect();
    assert_eq!(lines, [("TRANSACTION", "Food", "-12.50"), ("TRANSACTION", "Food", "-30.00"), ("TRANSFER_IN", "Main account", "150.00"), ("LEDGER", "Anna", "20.00")]);
    let history = h.ok("account_history", json!({ "id": cash, "year": year }));
    let points = history.as_array().unwrap();
    assert_eq!(points.last().unwrap(), &json!({ "month": month, "balance": "127.50" }));
    assert_eq!(points.len(), month[5..].parse::<usize>().unwrap());
    assert_eq!(h.ok("accounts_summary", json!({})), json!({ "inAccounts": "-43.50", "friends": "0.00", "netWorth": "-43.50", "accounts": 2, "inUse": true }));

    // the refusals
    assert_eq!(h.code("create_transfer", json!({ "input": { "fromAccountId": cash, "toAccountId": cash, "amount": "1.00", "date": first } })), "invalid_input");
    assert_eq!(h.code("create_transfer", json!({ "input": { "fromAccountId": "1", "toAccountId": cash, "amount": "0.00", "date": first } })), "invalid_input");
    assert_eq!(h.code("create_transfer", json!({ "input": { "fromAccountId": "1", "toAccountId": "99", "amount": "1.00", "date": first } })), "invalid_input");
    assert_eq!(h.code("update_transfer", json!({ "id": "1", "patch": { "toAccountId": "1" } })), "invalid_input");
    assert_eq!(h.code("delete_account", json!({ "id": cash })), "invalid_input", "it has rows");
    assert_eq!(h.code("account_statement", json!({ "id": cash, "from": date, "to": first.replace("-01", "-00") })), "invalid_input");
    assert_eq!(h.code("account_statement", json!({ "id": "99", "from": first, "to": date })), "not_found");
    assert_eq!(h.code("account_history", json!({ "id": "99", "year": year })), "not_found");
    assert_eq!(h.code("account_history", json!({ "id": cash, "year": "next" })), "invalid_input");
    // an archived account takes no new row, and a payment of an item in it goes to the main account
    h.ok("update_recurring", json!({ "id": rent["id"], "patch": { "accountId": cash } }));
    h.ok("archive_account", json!({ "id": cash }));
    assert_eq!(h.code("create_transaction", json!({ "input": { "name": "Shop", "amount": "1.00", "paymentDate": first, "categoryId": food, "accountId": cash } })), "invalid_input");
    assert_eq!(h.code("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "1.00", "date": first, "accountId": cash } })), "invalid_input");
    assert_eq!(h.code("create_transfer", json!({ "input": { "fromAccountId": "1", "toAccountId": cash, "amount": "1.00", "date": first } })), "invalid_input");
    assert_eq!(h.ok("pay_recurring", json!({ "id": rent["id"], "paidOn": first }))["accountId"], "1");
    assert_eq!(h.ok("update_transfer", json!({ "id": "1", "patch": { "amount": "140.00" } }))["amount"], "140.00", "a transfer with an archived side can still be edited");

    assert_eq!(h.ok("delete_transfer", json!({ "id": "1" })), Value::Null);
    assert_eq!(h.code("delete_transfer", json!({ "id": "1" })), "not_found");
    assert_eq!(h.code("get_transfer", json!({ "id": "1" })), "not_found");
    assert_eq!(h.code("update_transfer", json!({ "id": "1", "patch": {} })), "not_found");
}

#[test]
fn a_locked_app_answers_every_command_of_accounts_and_transfers_with_locked() {
    let h = harness();
    h.vault.lock();
    for (cmd, args) in [
        ("list_accounts", json!({})),
        ("create_account", json!({ "input": { "name": "Cash", "kind": "CASH" } })),
        ("update_account", json!({ "id": "1", "patch": {} })),
        ("archive_account", json!({ "id": "2" })),
        ("unarchive_account", json!({ "id": "2" })),
        ("delete_account", json!({ "id": "2" })),
        ("reorder_accounts", json!({ "ids": ["1"] })),
        ("account_statement", json!({ "id": "1", "from": "2026-11-01", "to": "2026-11-30" })),
        ("account_history", json!({ "id": "1", "year": "2026" })),
        ("accounts_summary", json!({})),
        ("create_transfer", json!({ "input": { "fromAccountId": "1", "toAccountId": "2", "amount": "1.00", "date": "2026-11-01" } })),
        ("get_transfer", json!({ "id": "1" })),
        ("update_transfer", json!({ "id": "1", "patch": {} })),
        ("delete_transfer", json!({ "id": "1" })),
    ] {
        assert_eq!(h.code(cmd, args), "locked", "{cmd}");
    }
}
