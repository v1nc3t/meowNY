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
            commands::account_breakdown,
            commands::accounts_summary,
            commands::create_transfer,
            commands::change_currency,
            commands::get_transfer,
            commands::update_transfer,
            commands::delete_transfer,
            commands::list_transfers,
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
            "isMain": true, "balance": "0.00", "changeThisMonth": "0.00", "lastActivity": null, "entries": 0,
            "bic": null, "notes": null, "group": null, "includeInNetWorth": true, "virtualBalance": "0.00", "available": "0.00",
            "currency": "EUR", "foreign": false, "rate": null, "balanceInMain": "0.00"
        }])
    );
    let card = h.ok("create_account", json!({ "input": { "name": "Card", "kind": "CREDIT_CARD", "number": "1234", "openingBalance": "-120.00", "openingDate": "2026-01-01" } }));
    assert_eq!(
        card,
        json!({
            "id": "2", "name": "Card", "kind": "CREDIT_CARD", "number": "1234", "openingBalance": "-120.00", "openingDate": "2026-01-01", "archived": false,
            "isMain": false, "balance": "-120.00", "changeThisMonth": "0.00", "lastActivity": null, "entries": 0,
            "bic": null, "notes": null, "group": null, "includeInNetWorth": true, "virtualBalance": "0.00", "available": "-120.00",
            "currency": "EUR", "foreign": false, "rate": null, "balanceInMain": "-120.00"
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
    assert_eq!(moved, json!({ "id": "1", "fromAccountId": "1", "toAccountId": cash, "amount": "200.00", "toAmount": null, "date": first, "note": "pocket money" }));
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
        ("account_breakdown", json!({ "id": "1", "from": "2026-11-01", "to": "2026-11-30" })),
        ("accounts_summary", json!({})),
        ("create_transfer", json!({ "input": { "fromAccountId": "1", "toAccountId": "2", "amount": "1.00", "date": "2026-11-01" } })),
        ("get_transfer", json!({ "id": "1" })),
        ("update_transfer", json!({ "id": "1", "patch": {} })),
        ("delete_transfer", json!({ "id": "1" })),
        ("list_transfers", json!({ "filter": {} })),
    ] {
        assert_eq!(h.code(cmd, args), "locked", "{cmd}");
    }
}

#[test]
fn the_details_of_an_account_go_through_the_commands_with_the_names_of_section_20_4() {
    let h = harness();
    let card = h.ok(
        "create_account",
        json!({ "input": {
            "name": "Card", "kind": "CREDIT_CARD", "openingBalance": "-240.00", "openingDate": "2026-01-01", "bic": " DEUTDEFF ", "notes": "limit 1000\r\nrenews in May", "group": "Bank A",
            "includeInNetWorth": false, "virtualBalance": "1000.00"
        } }),
    );
    assert_eq!(
        card,
        json!({
            "id": "2", "name": "Card", "kind": "CREDIT_CARD", "number": null, "openingBalance": "-240.00", "openingDate": "2026-01-01", "archived": false,
            "isMain": false, "balance": "-240.00", "changeThisMonth": "0.00", "lastActivity": null, "entries": 0,
            "bic": "DEUTDEFF", "notes": "limit 1000\nrenews in May", "group": "Bank A", "includeInNetWorth": false, "virtualBalance": "1000.00", "available": "760.00",
            "currency": "EUR", "foreign": false, "rate": null, "balanceInMain": "-240.00"
        })
    );
    // a patch: `null` clears the three texts, the flag and the money change, and what is left out stays
    let patched = h.ok("update_account", json!({ "id": "2", "patch": { "bic": null, "notes": null, "includeInNetWorth": true, "virtualBalance": "-10.00" } }));
    assert_eq!(
        (patched["bic"].clone(), patched["notes"].clone(), patched["group"].as_str(), patched["includeInNetWorth"].clone(), patched["virtualBalance"].as_str(), patched["available"].as_str()),
        (Value::Null, Value::Null, Some("Bank A"), json!(true), Some("-10.00"), Some("-250.00"))
    );
    assert_eq!(h.ok("update_account", json!({ "id": "2", "patch": { "group": null } }))["group"], Value::Null);
    // the main account takes them too
    assert_eq!(h.ok("update_account", json!({ "id": "1", "patch": { "bic": "BOFIIE2D", "includeInNetWorth": false } }))["includeInNetWorth"], false);
    assert_eq!(h.ok("list_accounts", json!({}))[0]["bic"], "BOFIIE2D");
    // net worth follows the flag: the card counts again (-240.00) and the main account (0.00) does not
    let today = h.ok("today", json!({}))["date"].as_str().unwrap().to_string();
    assert_eq!(h.ok("accounts_summary", json!({})), json!({ "inAccounts": "-240.00", "friends": "0.00", "netWorth": "-240.00", "accounts": 2, "inUse": true }), "on {today}");

    // what is refused, as the fixed codes the page gets
    for (what, patch) in [
        ("a BIC of 21 characters", json!({ "bic": "x".repeat(21) })),
        ("notes of 1001 characters", json!({ "notes": "x".repeat(1001) })),
        ("notes with a tab", json!({ "notes": "a\tb" })),
        ("a group of 51 characters", json!({ "group": "x".repeat(51) })),
        ("a virtual balance of -0.00", json!({ "virtualBalance": "-0.00" })),
        ("a virtual balance with one decimal", json!({ "virtualBalance": "1.5" })),
        ("a virtual balance beyond the range", json!({ "virtualBalance": "10000000000.00" })),
    ] {
        assert_eq!(h.code("update_account", json!({ "id": "2", "patch": patch })), "invalid_input", "{what}");
    }
    assert_eq!(h.code("create_account", json!({ "input": { "name": "X", "kind": "CASH", "bic": "x".repeat(21) } })), "invalid_input");
    assert_eq!(h.code("create_account", json!({ "input": { "name": "X", "kind": "CASH", "virtualBalance": "+1.00" } })), "invalid_input");
    // what does not have the shape of the input does not reach the code at all
    for (cmd, args) in [
        ("update_account", json!({ "id": "2", "patch": { "includeInNetWorth": "yes" } })),
        ("update_account", json!({ "id": "2", "patch": { "virtualBalance": 100 } })),
        ("update_account", json!({ "id": "2", "patch": { "bic": 5 } })),
        ("update_account", json!({ "id": "2", "patch": { "available": "1.00" } })),
        ("update_account", json!({ "id": "2", "patch": { "group_name": "x" } })),
        ("create_account", json!({ "input": { "name": "X", "kind": "CASH", "includeInNetWorth": 1 } })),
        ("create_account", json!({ "input": { "name": "X", "kind": "CASH", "virtualBalance": 5.5 } })),
    ] {
        assert!(h.call(cmd, args.clone()).is_err(), "{cmd} {args}");
    }
    assert_eq!(h.names(), ["Main account", "Card"], "nothing of that was written");
    h.ok("archive_account", json!({ "id": "2" }));
    assert_eq!(h.code("update_account", json!({ "id": "2", "patch": { "notes": "x" } })), "invalid_input", "an archived account takes no change");
}

#[test]
fn the_breakdown_and_the_list_of_transfers_go_through_the_commands_in_the_shape_the_pages_read() {
    let h = harness();
    let food = h.id_of("create_category", json!({ "name": "Food", "kind": "EXPENSE" }));
    let salary = h.id_of("create_category", json!({ "name": "Salary", "kind": "INCOME" }));
    let games = h.id_of("create_category", json!({ "name": "Games", "kind": "EXPENSE" }));
    let cash = h.id_of("create_account", json!({ "input": { "name": "Cash", "kind": "CASH" } }));
    let anna = h.id_of("create_person", json!({ "name": "Anna" }));
    let tx = |name: &str, amount: &str, date: &str, category: &str, account: &str| {
        h.ok("create_transaction", json!({ "input": { "name": name, "amount": amount, "paymentDate": date, "categoryId": category, "accountId": account } }))
    };
    tx("Shop", "12.50", "2026-11-01", &food, &cash);
    tx("Shop", "7.50", "2026-11-30", &food, &cash);
    tx("Chess", "20.00", "2026-11-15", &games, &cash);
    tx("Pay", "300.00", "2026-11-10", &salary, &cash);
    tx("Before", "99.00", "2026-10-31", &food, &cash);
    tx("After", "99.00", "2026-12-01", &salary, &cash);
    tx("Elsewhere", "99.00", "2026-11-10", &food, "1");
    h.ok("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "40.00", "date": "2026-11-10", "accountId": cash } }));
    h.ok("create_transfer", json!({ "input": { "fromAccountId": "1", "toAccountId": cash, "amount": "5.00", "date": "2026-11-10" } }));

    let b = h.ok("account_breakdown", json!({ "id": cash, "from": "2026-11-01", "to": "2026-11-30" }));
    assert_eq!(
        b,
        json!({
            "currency": "EUR", "from": "2026-11-01", "to": "2026-11-30",
            "expenses": [{ "categoryId": food, "name": "Food", "amount": "20.00" }, { "categoryId": games, "name": "Games", "amount": "20.00" }],
            "income": [{ "categoryId": salary, "name": "Salary", "amount": "300.00" }],
            "expenseTotal": "40.00", "incomeTotal": "300.00"
        })
    );
    let empty = h.ok("account_breakdown", json!({ "id": cash, "from": "2027-01-01", "to": "2027-01-31" }));
    assert_eq!(empty, json!({ "currency": "EUR", "from": "2027-01-01", "to": "2027-01-31", "expenses": [], "income": [], "expenseTotal": "0.00", "incomeTotal": "0.00" }));
    assert_eq!(h.code("account_breakdown", json!({ "id": "99", "from": "2026-11-01", "to": "2026-11-30" })), "not_found");
    assert_eq!(h.code("account_breakdown", json!({ "id": "x", "from": "2026-11-01", "to": "2026-11-30" })), "invalid_input");
    assert_eq!(h.code("account_breakdown", json!({ "id": "0", "from": "2026-11-01", "to": "2026-11-30" })), "invalid_input");
    assert_eq!(h.code("account_breakdown", json!({ "id": cash, "from": "2026-11-30", "to": "2026-11-01" })), "invalid_input");
    assert_eq!(h.code("account_breakdown", json!({ "id": cash, "from": "2026-02-30", "to": "2026-03-01" })), "invalid_input");
    assert!(h.call("account_breakdown", json!({ "id": cash, "from": "2026-11-01" })).is_err(), "`to` is needed");

    // the transfers: a page, newest first, with the same filter as the others
    h.ok("create_transfer", json!({ "input": { "fromAccountId": cash, "toAccountId": "1", "amount": "1.00", "date": "2026-11-20", "note": "back" } }));
    let all = h.ok("list_transfers", json!({ "filter": {} }));
    assert_eq!(
        all,
        json!({
            "transfers": [
                { "id": "2", "fromAccountId": cash, "toAccountId": "1", "amount": "1.00", "toAmount": null, "date": "2026-11-20", "note": "back" },
                { "id": "1", "fromAccountId": "1", "toAccountId": cash, "amount": "5.00", "toAmount": null, "date": "2026-11-10", "note": null },
            ],
            "total": 2, "limit": 50, "offset": 0
        })
    );
    assert_eq!(h.ok("list_transfers", json!({ "filter": { "accountId": cash, "from": "2026-11-15", "to": "2026-11-30" } }))["total"], 1);
    assert_eq!(h.ok("list_transfers", json!({ "filter": { "limit": 1, "offset": 1 } }))["transfers"][0]["id"], "1");
    assert_eq!(h.ok("list_transfers", json!({ "filter": { "accountId": "99" } }))["total"], 0);
    assert_eq!(h.code("list_transfers", json!({ "filter": { "accountId": "x" } })), "invalid_input");
    assert_eq!(h.code("list_transfers", json!({ "filter": { "limit": 0 } })), "invalid_input");
    assert_eq!(h.code("list_transfers", json!({ "filter": { "from": "November" } })), "invalid_input");
    assert!(h.call("list_transfers", json!({ "filter": { "kind": "INCOME" } })).is_err(), "a transfer has no kind: an unknown field is refused");
    assert!(h.call("list_transfers", json!({})).is_err(), "the filter is needed, as for the other lists");

    // the transactions list takes `kind`
    let page = |kind: Value| h.ok("list_transactions", json!({ "filter": { "kind": kind, "accountId": cash } }));
    assert_eq!((page(json!("INCOME"))["total"].clone(), page(json!("EXPENSE"))["total"].clone(), page(Value::Null)["total"].clone()), (json!(2), json!(4), json!(6)));
    assert_eq!(page(json!("INCOME"))["transactions"][0]["name"], "After");
    assert_eq!(h.code("list_transactions", json!({ "filter": { "kind": "income" } })), "invalid_input");
    assert_eq!(h.code("list_transactions", json!({ "filter": { "kind": "TRANSFER" } })), "invalid_input");
    assert!(h.call("list_transactions", json!({ "filter": { "kind": 1 } })).is_err());
}

#[test]
fn a_currency_of_its_own_goes_through_the_commands_with_the_names_of_section_21_4() {
    let h = harness();
    // every row is dated the first of this month, so that it is in the past whatever day the test runs
    let today = h.ok("today", json!({}));
    let (date, first) = (today["date"].as_str().unwrap().to_string(), format!("{}-01", today["month"].as_str().unwrap()));
    let food = h.id_of("create_category", json!({ "name": "Food", "kind": "EXPENSE" }));
    let salary = h.id_of("create_category", json!({ "name": "Salary", "kind": "INCOME" }));
    let lei = h.ok("create_account", json!({ "input": { "name": "Lei", "kind": "CASH", "currency": "ron", "rate": "5.00", "openingBalance": "100.00", "openingDate": "2026-01-01" } }));
    assert_eq!(
        lei,
        json!({
            "id": "2", "name": "Lei", "kind": "CASH", "number": null, "openingBalance": "100.00", "openingDate": "2026-01-01", "archived": false,
            "isMain": false, "balance": "100.00", "changeThisMonth": "0.00", "lastActivity": null, "entries": 0,
            "bic": null, "notes": null, "group": null, "includeInNetWorth": true, "virtualBalance": "0.00", "available": "100.00",
            "currency": "RON", "foreign": true, "rate": "5.00", "balanceInMain": "20.00"
        })
    );
    let lei_id = lei["id"].as_str().unwrap().to_string();
    // a transaction in it carries both amounts, and one in the main account only the first
    let shop = h.ok("create_transaction", json!({ "input": { "name": "Shop", "amount": "22.75", "paymentDate": first, "categoryId": food, "accountId": lei_id, "nativeAmount": "113.74" } }));
    assert_eq!((shop["amount"].as_str(), shop["nativeAmount"].as_str(), shop["accountId"].as_str()), (Some("22.75"), Some("113.74"), Some("2")));
    let plain = h.ok("create_transaction", json!({ "input": { "name": "Shop", "amount": "1.00", "paymentDate": first, "categoryId": food } }));
    assert_eq!(plain["nativeAmount"], Value::Null);
    let pay = h.ok("create_transaction", json!({ "input": { "name": "Pay", "amount": "50.00", "paymentDate": first, "categoryId": salary, "accountId": lei_id, "nativeAmount": "250.00" } }));
    assert_eq!(h.ok("list_transactions", json!({ "filter": { "accountId": lei_id } }))["transactions"][0]["nativeAmount"], "250.00");
    assert_eq!(h.code("create_transaction", json!({ "input": { "name": "Shop", "amount": "1.00", "paymentDate": first, "categoryId": food, "accountId": lei_id } })), "invalid_input", "a foreign account needs the native amount");
    assert_eq!(h.code("create_transaction", json!({ "input": { "name": "Shop", "amount": "1.00", "paymentDate": first, "categoryId": food, "nativeAmount": "5.00" } })), "invalid_input", "and any other refuses it");
    assert_eq!(h.code("create_transaction", json!({ "input": { "name": "Shop", "amount": "1.00", "paymentDate": first, "categoryId": food, "accountId": lei_id, "nativeAmount": "0.00" } })), "invalid_input");
    assert!(h.call("create_transaction", json!({ "input": { "name": "Shop", "amount": "1.00", "paymentDate": first, "categoryId": food, "accountId": lei_id, "nativeAmount": 5.5 } })).is_err(), "money is text");
    assert!(h.call("create_transaction", json!({ "input": { "name": "Shop", "amount": "1.00", "paymentDate": first, "categoryId": food, "foreignAmount": "5.00" } })).is_err());
    // an update: in place, a move out (cleared), a move in (needed)
    assert_eq!(h.ok("update_transaction", json!({ "id": shop["id"], "patch": { "nativeAmount": "120.00" } }))["nativeAmount"], "120.00");
    let out = h.ok("update_transaction", json!({ "id": shop["id"], "patch": { "accountId": "1" } }));
    assert_eq!((out["accountId"].as_str(), out["nativeAmount"].clone()), (Some("1"), Value::Null));
    assert_eq!(h.code("update_transaction", json!({ "id": shop["id"], "patch": { "accountId": lei_id } })), "invalid_input");
    assert_eq!(h.ok("update_transaction", json!({ "id": shop["id"], "patch": { "accountId": lei_id, "nativeAmount": "113.74" } }))["nativeAmount"], "113.74");
    // a transfer between currencies carries the amount that arrives
    let sent = h.ok("create_transfer", json!({ "input": { "fromAccountId": "1", "toAccountId": lei_id, "amount": "50.00", "toAmount": "249.00", "date": first } }));
    assert_eq!(sent, json!({ "id": "1", "fromAccountId": "1", "toAccountId": lei_id, "amount": "50.00", "toAmount": "249.00", "date": first, "note": null }));
    assert_eq!(h.ok("get_transfer", json!({ "id": "1" })), sent);
    assert_eq!(h.ok("update_transfer", json!({ "id": "1", "patch": { "toAmount": "250.00" } }))["toAmount"], "250.00");
    assert_eq!(h.ok("list_transfers", json!({ "filter": {} }))["transfers"][0]["toAmount"], "250.00");
    assert_eq!(h.code("create_transfer", json!({ "input": { "fromAccountId": "1", "toAccountId": lei_id, "amount": "5.00", "date": first } })), "invalid_input");
    let cash = h.id_of("create_account", json!({ "input": { "name": "Cash", "kind": "CASH" } }));
    assert_eq!(h.code("create_transfer", json!({ "input": { "fromAccountId": "1", "toAccountId": cash, "amount": "5.00", "toAmount": "5.00", "date": first } })), "invalid_input");
    assert_eq!(h.code("update_transfer", json!({ "id": "1", "patch": { "toAccountId": cash, "toAmount": "250.00" } })), "invalid_input", "both sides would be in the main currency, and then a patch gives no amount that arrives");
    // the numbers of the account, in its own currency
    let statement = h.ok("account_statement", json!({ "id": lei_id, "from": first, "to": date }));
    assert_eq!((statement["currency"].as_str(), statement["account"]["currency"].as_str(), statement["balanceBefore"].as_str(), statement["balanceAfter"].as_str()), (Some("RON"), Some("RON"), Some("100.00"), Some("486.26")));
    let lines: Vec<(&str, &str)> = statement["lines"].as_array().unwrap().iter().map(|l| (l["source"].as_str().unwrap(), l["amount"].as_str().unwrap())).collect();
    assert_eq!(lines, [("TRANSACTION", "-113.74"), ("TRANSACTION", "250.00"), ("TRANSFER_IN", "250.00")]);
    let breakdown = h.ok("account_breakdown", json!({ "id": lei_id, "from": first, "to": date }));
    assert_eq!((breakdown["currency"].as_str(), breakdown["expenseTotal"].as_str(), breakdown["incomeTotal"].as_str()), (Some("RON"), Some("113.74"), Some("250.00")));
    assert_eq!(h.ok("account_breakdown", json!({ "id": "1", "from": first, "to": date }))["currency"], "EUR");
    assert_eq!(h.ok("account_statement", json!({ "id": "1", "from": first, "to": date }))["currency"], "EUR");
    // net worth is in the main currency: the main account holds -51.00 (a euro spent and 50.00 sent), and 486.26 lei at 5.00 are 97.25
    let summary = h.ok("accounts_summary", json!({}));
    assert_eq!(summary["inAccounts"], "46.25", "{summary}");
    let shown = h.ok("list_accounts", json!({}));
    assert_eq!((shown[1]["balance"].as_str(), shown[1]["balanceInMain"].as_str(), shown[1]["currency"].as_str()), (Some("486.26"), Some("97.25"), Some("RON")));
    // a patch: the rate, the balance, the currency of an account with nothing in it; refusals as the usual code
    assert_eq!(h.ok("update_account", json!({ "id": lei_id, "patch": { "rate": "4.00" } }))["balanceInMain"], "121.57");
    let set = h.ok("update_account", json!({ "id": lei_id, "patch": { "balance": "1662.90" } }));
    assert_eq!((set["balance"].as_str(), set["openingBalance"].as_str()), (Some("1662.90"), Some("1276.64")));
    let fresh = h.ok("update_account", json!({ "id": cash, "patch": { "currency": "usd", "rate": "1.10" } }));
    assert_eq!((fresh["currency"].as_str(), fresh["foreign"].clone(), fresh["rate"].as_str()), (Some("USD"), json!(true), Some("1.10")));
    assert_eq!(h.code("update_account", json!({ "id": lei_id, "patch": { "currency": "usd" } })), "invalid_input", "it has rows");
    assert_eq!(h.code("update_account", json!({ "id": "1", "patch": { "currency": "RON", "rate": "5.00" } })), "invalid_input", "the main account");
    assert_eq!(h.code("update_account", json!({ "id": lei_id, "patch": { "rate": "0" } })), "invalid_input");
    assert_eq!(h.code("update_account", json!({ "id": lei_id, "patch": { "balance": "1.5" } })), "invalid_input");
    assert_eq!(h.code("update_account", json!({ "id": lei_id, "patch": { "balance": "1.00", "openingBalance": "1.00" } })), "invalid_input");
    assert_eq!(h.code("create_account", json!({ "input": { "name": "X", "kind": "CASH", "currency": "RON" } })), "invalid_input", "a rate is needed");
    assert_eq!(h.code("create_account", json!({ "input": { "name": "X", "kind": "CASH", "currency": "eur", "rate": "1.00" } })), "invalid_input");
    assert!(h.call("update_account", json!({ "id": lei_id, "patch": { "balanceInMain": "1.00" } })).is_err());
    assert!(h.call("update_account", json!({ "id": lei_id, "patch": { "rate": 5 } })).is_err());
    // recurring items and debts are in the main currency only
    let anna = h.id_of("create_person", json!({ "name": "Anna" }));
    assert_eq!(h.code("create_recurring", json!({ "input": { "name": "Gym", "amount": "30.00", "categoryId": food, "startDate": first, "intervalUnit": "MONTH", "intervalCount": 1, "accountId": lei_id } })), "invalid_input");
    assert_eq!(h.code("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "20.00", "date": first, "accountId": lei_id } })), "invalid_input");
    h.ok("add_ledger_entry", json!({ "input": { "personId": anna, "kind": "LENT", "amount": "20.00", "date": first } }));
    assert_eq!(h.code("settle_person", json!({ "id": anna, "accountId": lei_id })), "invalid_input");
    assert_eq!(pay["nativeAmount"], "250.00");
}

#[test]
fn the_currency_of_the_app_cannot_be_changed_through_the_command_while_an_account_has_its_own() {
    let h = harness();
    let lei = h.id_of("create_account", json!({ "input": { "name": "Lei", "kind": "CASH", "currency": "RON", "rate": "5.00" } }));
    for rate in [json!("2"), Value::Null] {
        assert_eq!(h.code("change_currency", json!({ "input": { "currency": "USD", "rate": rate } })), "invalid_input");
    }
    assert_eq!(h.ok("archive_account", json!({ "id": lei }))["archived"], true);
    assert_eq!(h.code("change_currency", json!({ "input": { "currency": "USD", "rate": "2" } })), "invalid_input", "archived or not");
    assert_eq!(h.ok("delete_account", json!({ "id": lei })), Value::Null);
    assert_eq!(h.ok("change_currency", json!({ "input": { "currency": "USD", "rate": "2" } }))["accounts"], 0, "and it is allowed again once the account is gone");
    assert_eq!(h.ok("list_accounts", json!({}))[0]["currency"], "USD", "the main account follows");
}
