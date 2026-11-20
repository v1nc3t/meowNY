//! The commands of stashes, called the way the page calls them: a name and a JSON body through Tauri's message layer (its mock runtime), with
//! the vault as managed state. What crosses is checked here: the names of the arguments, the JSON of the answers (`docs/stash-design.md`,
//! section 6) and the fixed code of each refusal.

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
    _dir: tempfile::TempDir,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(Vault::new(dir.path().to_path_buf(), Box::new(MemoryStore::default())));
    vault.create(&Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() }, None).unwrap();
    let app = mock_builder()
        .manage(vault)
        .invoke_handler(tauri::generate_handler![
            commands::today,
            commands::list_accounts,
            commands::create_account,
            commands::archive_account,
            commands::delete_account,
            commands::delete_transfer,
            commands::list_transfers,
            commands::create_category,
            commands::create_transaction,
            commands::list_stashes,
            commands::get_stash,
            commands::create_stash,
            commands::update_stash,
            commands::archive_stash,
            commands::unarchive_stash,
            commands::delete_stash,
            commands::reorder_stashes,
            commands::stash_history,
            commands::stashes_summary,
            commands::add_to_stash,
            commands::take_from_stash,
            commands::update_stash_entry,
            commands::delete_stash_entry,
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", WebviewUrl::default()).build().unwrap();
    Harness { _app: app, webview, _dir: dir }
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
        self.ok("list_stashes", json!({})).as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap().to_string()).collect()
    }

    /// An account with money in it. Returns its id.
    fn account(&self, name: &str, opening: &str) -> String {
        self.id_of("create_account", json!({ "input": { "name": name, "kind": "SAVINGS", "openingBalance": opening, "openingDate": "2020-01-01" } }))
    }
}

#[test]
fn stashes_are_made_changed_moved_ordered_archived_restored_and_deleted_through_the_commands() {
    let h = harness();
    let savings = h.account("Savings", "1000.00");
    let cash = h.account("Cash", "50.00");
    // with every field of the answer
    let holiday = h.ok("create_stash", json!({ "input": { "name": "  Holiday ", "accountId": savings, "target": "1200.00", "targetDate": "2099-12-31", "notes": "two\r\nlines" } }));
    let per_month = holiday["perMonth"].as_str().unwrap().to_string();
    assert!(per_month.parse::<f64>().unwrap() > 0.0, "{per_month}: a far date, so a small amount a month");
    assert_eq!(
        holiday,
        json!({
            "id": "1", "name": "Holiday", "accountId": savings, "account": "Savings", "currency": "EUR", "archived": false, "accountArchived": false,
            "target": "1200.00", "targetDate": "2099-12-31", "notes": "two\nlines", "saved": "0.00", "left": "1200.00", "percent": 0, "reached": false,
            "perMonth": per_month, "overdue": false, "free": "1000.00", "entries": 0, "lastEntry": null
        })
    );
    // only the name and the account are needed
    let plain = h.ok("create_stash", json!({ "input": { "name": "Rainy day", "accountId": cash } }));
    assert_eq!(
        (plain["target"].clone(), plain["targetDate"].clone(), plain["notes"].clone(), plain["left"].clone(), plain["percent"].clone(), plain["perMonth"].clone(), plain["overdue"].clone(), plain["free"].as_str()),
        (Value::Null, Value::Null, Value::Null, Value::Null, Value::Null, Value::Null, json!(false), Some("50.00"))
    );
    assert_eq!(h.ok("get_stash", json!({ "id": "1" })), json!({ "stash": holiday, "entries": [] }));
    assert_eq!(h.ok("list_stashes", json!({})), json!([holiday, plain]));
    assert_eq!(h.ok("list_stashes", json!({ "accountId": cash })), json!([plain]), "cut to an account");
    assert_eq!(h.ok("list_stashes", json!({ "accountId": null })).as_array().unwrap().len(), 2);

    let changed = h.ok("update_stash", json!({ "id": "1", "patch": { "name": "Trip", "target": "1500.00", "targetDate": null, "notes": null } }));
    assert_eq!((changed["name"].as_str(), changed["target"].as_str(), changed["targetDate"].clone(), changed["notes"].clone(), changed["perMonth"].clone()), (Some("Trip"), Some("1500.00"), Value::Null, Value::Null, Value::Null));
    assert_eq!(h.ok("reorder_stashes", json!({ "ids": ["2", "1"] })), Value::Null);
    assert_eq!(h.names(), ["Rainy day", "Trip"]);
    // a move to another account, in the same patch as a rename
    let moved = h.ok("update_stash", json!({ "id": "2", "patch": { "accountId": savings, "name": "Rainy" } }));
    assert_eq!((moved["accountId"].as_str(), moved["account"].as_str(), moved["name"].as_str(), moved["free"].as_str()), (Some(savings.as_str()), Some("Savings"), Some("Rainy"), Some("1000.00")));

    assert_eq!(h.ok("archive_stash", json!({ "id": "1" }))["archived"], true);
    assert_eq!(h.ok("archive_stash", json!({ "id": "1" }))["archived"], true, "again: the stash as it is");
    assert_eq!(h.names(), ["Rainy", "Trip"], "the archived last");
    assert_eq!(h.ok("unarchive_stash", json!({ "id": "1" }))["archived"], false);
    assert_eq!(h.ok("delete_stash", json!({ "id": "2" })), Value::Null);
    assert_eq!(h.names(), ["Trip"]);

    // the refusals, as the fixed codes the page gets
    assert_eq!(h.code("create_stash", json!({ "input": { "name": "trip", "accountId": savings } })), "name_exists");
    let other = h.id_of("create_stash", json!({ "input": { "name": "Other", "accountId": savings } }));
    assert_eq!(h.code("update_stash", json!({ "id": other, "patch": { "name": "TRIP" } })), "name_exists");
    assert_eq!(h.ok("update_stash", json!({ "id": "1", "patch": { "name": "TRIP" } }))["name"], "TRIP", "its own name, in another case, is not a clash");
    h.ok("delete_stash", json!({ "id": other }));
    assert_eq!(h.code("create_stash", json!({ "input": { "name": "", "accountId": savings } })), "invalid_input");
    assert_eq!(h.code("create_stash", json!({ "input": { "name": "A", "accountId": "99" } })), "invalid_input");
    assert_eq!(h.code("create_stash", json!({ "input": { "name": "A", "accountId": "x" } })), "invalid_input");
    assert_eq!(h.code("create_stash", json!({ "input": { "name": "A", "accountId": savings, "target": "0.00" } })), "invalid_input");
    assert_eq!(h.code("create_stash", json!({ "input": { "name": "A", "accountId": savings, "targetDate": "2026-02-30" } })), "invalid_input");
    assert_eq!(h.code("update_stash", json!({ "id": "x", "patch": {} })), "invalid_input");
    assert_eq!(h.code("update_stash", json!({ "id": "1", "patch": { "accountId": "99" } })), "invalid_input");
    assert_eq!(h.code("reorder_stashes", json!({ "ids": [] })), "invalid_input");
    assert_eq!(h.code("reorder_stashes", json!({ "ids": ["1", "x"] })), "invalid_input");
    assert_eq!(h.code("reorder_stashes", json!({ "ids": ["1", "1"] })), "invalid_input");
    assert_eq!(h.code("stash_history", json!({ "id": "1", "year": "26" })), "invalid_input");
    for cmd in ["get_stash", "archive_stash", "unarchive_stash", "delete_stash"] {
        assert_eq!(h.code(cmd, json!({ "id": "99" })), "not_found", "{cmd}");
    }
    assert_eq!(h.code("update_stash", json!({ "id": "99", "patch": {} })), "not_found");
    assert_eq!(h.code("stash_history", json!({ "id": "99", "year": "2026" })), "not_found");
    // what does not have the shape of the input does not reach the code at all
    assert!(h.call("create_stash", json!({ "input": { "name": "A" } })).is_err(), "the account is always required");
    assert!(h.call("create_stash", json!({ "input": { "name": "A", "accountId": savings, "archived": true } })).is_err());
    assert!(h.call("create_stash", json!({ "input": { "name": "A", "accountId": savings, "target": 12.5 } })).is_err(), "money is text");
    assert!(h.call("create_stash", json!({ "input": { "name": "A", "accountId": 1 } })).is_err(), "an id is text");
    assert!(h.call("update_stash", json!({ "id": "1", "patch": { "saved": "1.00" } })).is_err());
    assert!(h.call("update_stash", json!({ "id": "1", "patch": { "account": "1" } })).is_err(), "the field is accountId");
    assert!(h.call("create_stash", json!({ "input": { "name": "A", "account_id": savings } })).is_err(), "arguments are camelCase");
    assert!(h.call("get_stash", json!({ "stashId": "1" })).is_err(), "the argument is called id");
    assert_eq!(h.names(), ["TRIP"], "nothing of that was written");
}

#[test]
fn entries_go_through_the_commands_and_every_one_answers_the_whole_stash() {
    let h = harness();
    let today = h.ok("today", json!({}))["date"].as_str().unwrap().to_string();
    let current = h.account("Current", "1000.00");
    let savings = h.account("Savings", "1000.00");
    let id = h.id_of("create_stash", json!({ "input": { "name": "Holiday", "accountId": savings, "target": "500.00" } }));

    // an add with a transfer from another account
    let added = h.ok("add_to_stash", json!({ "id": id, "input": { "amount": "100.00", "note": "november", "fromAccountId": current } }));
    let entry = added["entries"][0].clone();
    assert_eq!(
        entry,
        json!({
            "id": "1", "stashId": id, "kind": "ADD", "amount": "100.00", "date": today, "note": "november", "transferId": "1", "otherAccountId": current,
            "otherAccount": "Current", "savedAfter": "100.00"
        })
    );
    assert_eq!(added["stash"]["saved"], "100.00");
    assert_eq!((added["stash"]["percent"].clone(), added["stash"]["left"].as_str(), added["stash"]["free"].as_str(), added["stash"]["entries"].clone(), added["stash"]["lastEntry"].as_str()), (json!(20), Some("400.00"), Some("1000.00"), json!(1), Some(today.as_str())));
    assert_eq!(h.ok("get_stash", json!({ "id": id })), added);
    assert_eq!(h.ok("list_transfers", json!({ "filter": {} }))["total"], 1);

    // an add with no transfer and a date, then a take
    h.ok("add_to_stash", json!({ "id": id, "input": { "amount": "50.00", "date": "2020-06-01" } }));
    assert_eq!(h.code("add_to_stash", json!({ "id": id, "input": { "amount": "350.01" } })), "invalid_input", "150.00 and 350.01 is over the target of 500.00");
    let taken = h.ok("take_from_stash", json!({ "id": id, "input": { "amount": "40.00", "note": "deposit", "toAccountId": current } }));
    let out = taken["entries"][0].clone();
    assert_eq!((out["kind"].as_str(), out["amount"].as_str(), out["otherAccountId"].as_str(), out["otherAccount"].as_str(), out["savedAfter"].as_str(), out["transferId"].as_str()), (Some("TAKE"), Some("40.00"), Some(current.as_str()), Some("Current"), Some("110.00"), Some("2")));
    assert_eq!(taken["stash"]["saved"], "110.00");
    assert_eq!(taken["entries"].as_array().unwrap().len(), 3, "newest first");
    assert_eq!(taken["entries"][2]["date"], "2020-06-01");

    // edit an entry, then delete one with and without the transfer
    let edited = h.ok("update_stash_entry", json!({ "id": "1", "patch": { "amount": "90.00", "date": "2020-01-01", "note": null } }));
    let first = edited["entries"].as_array().unwrap().iter().find(|e| e["id"] == "1").unwrap().clone();
    assert_eq!((first["amount"].as_str(), first["date"].as_str(), first["note"].clone(), first["kind"].as_str()), (Some("90.00"), Some("2020-01-01"), Value::Null, Some("ADD")));
    assert_eq!(edited["stash"]["saved"], "100.00");
    let without = h.ok("delete_stash_entry", json!({ "id": "3" }));
    assert_eq!(without["stash"]["saved"], "140.00", "the take of 40.00 was entry 3; with no deleteTransfer given its transfer stays");
    assert_eq!(h.ok("list_transfers", json!({ "filter": {} }))["total"], 2);
    let explicit = h.ok("delete_stash_entry", json!({ "id": "2", "deleteTransfer": false }));
    assert_eq!(explicit["stash"]["saved"], "90.00");
    let with = h.ok("delete_stash_entry", json!({ "id": "1", "deleteTransfer": true }));
    assert_eq!(with, json!({ "stash": with["stash"], "entries": [] }));
    assert_eq!(with["stash"]["saved"], "0.00");
    assert_eq!(h.ok("list_transfers", json!({ "filter": {} }))["total"], 1, "the transfer that came with entry 1 went; the take's stayed");

    // the refusals
    assert_eq!(h.code("add_to_stash", json!({ "id": id, "input": { "amount": "0.00" } })), "invalid_input");
    assert_eq!(h.code("add_to_stash", json!({ "id": id, "input": { "amount": "1.5" } })), "invalid_input");
    assert_eq!(h.code("add_to_stash", json!({ "id": id, "input": { "amount": "1.00", "date": "2099-01-01" } })), "invalid_input");
    assert_eq!(h.code("add_to_stash", json!({ "id": id, "input": { "amount": "1.00", "fromAccountId": savings } })), "invalid_input", "from its own account");
    assert_eq!(h.code("take_from_stash", json!({ "id": id, "input": { "amount": "0.01" } })), "invalid_input", "nothing is saved");
    assert_eq!(h.code("take_from_stash", json!({ "id": id, "input": { "amount": "1.00", "toAccountId": "x" } })), "invalid_input");
    assert_eq!(h.code("update_stash_entry", json!({ "id": "x", "patch": {} })), "invalid_input");
    for (cmd, args) in [
        ("add_to_stash", json!({ "id": "99", "input": { "amount": "1.00" } })),
        ("take_from_stash", json!({ "id": "99", "input": { "amount": "1.00" } })),
        ("update_stash_entry", json!({ "id": "99", "patch": {} })),
        ("delete_stash_entry", json!({ "id": "99" })),
        ("delete_stash_entry", json!({ "id": "99", "deleteTransfer": true })),
    ] {
        assert_eq!(h.code(cmd, args), "not_found", "{cmd}");
    }
    // what does not have the shape of the input does not reach the code
    for (cmd, args) in [
        ("add_to_stash", json!({ "id": id, "input": { "amount": "1.00", "toAccountId": current } })),
        ("add_to_stash", json!({ "id": id, "input": { "amount": 1 } })),
        ("add_to_stash", json!({ "id": id, "input": { "amount": "1.00", "kind": "TAKE" } })),
        ("take_from_stash", json!({ "id": id, "input": { "amount": "1.00", "fromAccountId": current } })),
        ("update_stash_entry", json!({ "id": "1", "patch": { "kind": "TAKE" } })),
        ("update_stash_entry", json!({ "id": "1", "patch": { "transferId": "1" } })),
        ("delete_stash_entry", json!({ "id": "1", "deleteTransfer": "yes" })),
    ] {
        assert!(h.call(cmd, args.clone()).is_err(), "{cmd} {args}");
    }
}

#[test]
fn the_history_and_the_summary_go_through_the_commands() {
    let h = harness();
    let today = h.ok("today", json!({}))["date"].as_str().unwrap().to_string();
    let year = &today[..4];
    assert_eq!(h.ok("stashes_summary", json!({})), json!({ "stashes": 0, "saved": "0.00", "target": "0.00", "reached": 0 }));
    let savings = h.account("Savings", "1000.00");
    let a = h.id_of("create_stash", json!({ "input": { "name": "A", "accountId": savings, "target": "100.00" } }));
    let b = h.id_of("create_stash", json!({ "input": { "name": "B", "accountId": savings } }));
    h.ok("add_to_stash", json!({ "id": a, "input": { "amount": "100.00" } }));
    h.ok("add_to_stash", json!({ "id": b, "input": { "amount": "50.00", "date": format!("{year}-01-01") } }));
    assert_eq!(h.ok("stashes_summary", json!({})), json!({ "stashes": 2, "saved": "150.00", "target": "100.00", "reached": 1 }));
    let points = h.ok("stash_history", json!({ "id": b, "year": year }));
    let points = points.as_array().unwrap();
    assert!(!points.is_empty() && points.len() <= 12);
    assert_eq!(points[0], json!({ "month": format!("{year}-01"), "saved": "50.00" }), "the shape of a point: month and saved");
    assert_eq!(points.last().unwrap()["saved"], "50.00");
    h.ok("archive_stash", json!({ "id": b }));
    assert_eq!(h.ok("stashes_summary", json!({})), json!({ "stashes": 1, "saved": "100.00", "target": "100.00", "reached": 1 }), "the archived one is left out");
    // the account says what is set aside and what is free
    let accounts = h.ok("list_accounts", json!({}));
    let account = accounts.as_array().unwrap().iter().find(|a| a["id"] == json!(savings)).unwrap();
    assert_eq!((account["setAside"].as_str(), account["free"].as_str(), account["balance"].as_str(), account["entries"].clone()), (Some("150.00"), Some("850.00"), Some("1000.00"), json!(2)));
    // an account with a stash is not deleted, and the code says so
    assert_eq!(h.code("delete_account", json!({ "id": savings })), "invalid_input");
}
