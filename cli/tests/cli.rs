//! The real `meowny` binary against the app's own server code: a real encrypted vault in a temporary
//! folder, real tokens, a real listener on 127.0.0.1. A command and the route behind it are tested
//! together, from the command line to the database and back.

use std::collections::BTreeSet;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use meowny_lib::api::routes;
use meowny_lib::tokens::{self, Scope};
use serde_json::{json, Value};

mod common;
use common::*;

// ---- the commands exist for every route ------------------------------------------------------

#[test]
fn the_schema_file_in_the_cli_is_the_app_route_table() {
    let embedded: Value = serde_json::from_str(include_str!("../schema.json")).unwrap();
    let described: Vec<Value> = routes::ALL.iter().map(|r| r.describe()).collect();
    assert_eq!(
        embedded,
        json!({ "api": meowny_lib::api::API_VERSION, "routes": described }),
        "the routes changed: run `cargo run --example dump_schema > schema.json` in cli/ and commit it"
    );
}

#[test]
fn every_route_has_a_command_that_answers_help_and_names_all_its_flags() {
    let app = app(false);
    let schema = run(&app, "", &["schema"]).json();
    let commands = schema["commands"].as_array().unwrap();
    for route in routes::ALL {
        let command = commands.iter().find(|c| c["route"] == route.name).unwrap_or_else(|| panic!("no command for the route {}", route.name));
        let words: Vec<&str> = command["command"].as_str().unwrap().split(' ').collect();
        assert_eq!(words.join("."), route.name);
        let help = run(&app, "", &[&words[..], &["--help"]].concat());
        assert_eq!((help.code, help.err.as_str()), (0, ""), "{}", route.name);
        assert!(help.out.contains(route.summary), "{}: help does not say what it does", route.name);
        for param in route.params {
            let flag = command["flags"].as_array().unwrap().iter().find(|f| f["param"] == param.name).unwrap_or_else(|| panic!("{} has no flag for {}", route.name, param.name));
            assert!(help.out.contains(flag["flag"].as_str().unwrap()) || command["positional"] == param.name, "{} {}", route.name, param.name);
            // a command that reads a file for the person gives the document the name of the file unless told another
            let named_by_the_file = param.name == "name" && route.params.iter().any(|p| p.kind == meowny_lib::api::routes::Kind::Bytes);
            assert_eq!(flag["required"], param.required && !named_by_the_file, "{} {}", route.name, param.name);
        }
        assert_eq!(command["scope"], serde_json::to_value(route.scope).unwrap());
    }
    // And nothing in the CLI points at a route that does not exist.
    for command in commands {
        if let Some(route) = command["route"].as_str() {
            assert!(routes::ALL.iter().any(|r| r.name == route), "{route}");
        }
    }
}

#[test]
fn help_works_at_every_level_without_the_app() {
    let app = app(false);
    let schema = run(&app, "", &["schema"]).json();
    let mut nouns = BTreeSet::new();
    for command in schema["commands"].as_array().unwrap() {
        let words: Vec<&str> = command["command"].as_str().unwrap().split(' ').collect();
        if words.len() == 2 {
            nouns.insert(words[0].to_string());
        }
    }
    let mut calls: Vec<Vec<String>> = vec![vec!["--help".into()], vec!["help".into()], vec!["-h".into()]];
    calls.extend(nouns.iter().map(|n| vec![n.clone(), "--help".into()]));
    calls.extend(nouns.iter().map(|n| vec!["help".into(), n.clone()]));
    for call in calls {
        let args: Vec<&str> = call.iter().map(String::as_str).collect();
        let out = run(&app, "", &args);
        assert_eq!((out.code, out.err.as_str()), (0, ""), "{args:?}");
        assert!(out.out.len() > 100, "{args:?}");
    }
    let version = run(&app, "", &["--version"]);
    assert_eq!((version.code, version.out.trim()), (0, concat!("meowny ", env!("CARGO_PKG_VERSION"), " (API 1)")));
    let unknown = run(&app, "", &["frobnicate"]);
    assert_eq!((unknown.code, unknown.out.as_str()), (2, ""));
    assert!(unknown.err.contains("unknown command"));
}

#[test]
fn schema_is_answered_offline_and_the_app_can_be_asked_too() {
    let app = app(false);
    // No token, no app data folder: still an answer.
    let mut bare = Command::new(env!("CARGO_BIN_EXE_meowny"));
    clean(&mut bare).arg("schema");
    let offline = finish(bare);
    assert_eq!((offline.code, offline.err.as_str()), (0, ""));
    let schema = offline.json();
    assert_eq!(schema["cli"]["api"], 1);
    assert_eq!(schema["errorCodes"]["locked"], 3);
    assert_eq!(schema["exitCodes"].as_object().unwrap().len(), 5);
    assert_eq!(offline.out.lines().count(), 1);
    // `--app` asks the running app and gets the app's own description of its routes.
    let live = ok(&app, &app.read, &["schema", "--app"]);
    assert_eq!(live, serde_json::from_str::<Value>(include_str!("../schema.json")).unwrap());
    assert_eq!(failed(&app, "", &["schema", "--app"]), (3, "no_token".into()));
}

// ---- reading ---------------------------------------------------------------------------------

/// Arguments that make each read command work on the demo data.
fn sample(command: &str, category: &str) -> Vec<String> {
    let args: &[&str] = match command {
        "budgets overview" | "stats budget" | "stats planned" => &["--month", "2026-09"],
        "stats year" => &["--year", "2026"],
        "stats top" => &["--month", "2026-09", "--type", "expense", "--limit", "3"],
        "transactions list" => &["--limit", "5"],
        "categories list" => &["--type", "expense"],
        "recurring dues" => &["--within-days", "10"],
        "documents show" => &["1"],
        _ => &[],
    };
    let mut out: Vec<String> = command.split(' ').map(String::from).collect();
    out.extend(args.iter().map(|s| s.to_string()));
    if command == "stats trend" {
        out.extend(["--category-id", category, "--from", "2026-01", "--to", "2026-09"].map(String::from));
    }
    out
}

#[test]
fn every_read_command_runs_and_prints_what_the_app_returns() {
    let app = app(true);
    document(&app, "Receipt.pdf", b"%PDF-1.4 a receipt", Some("1"));
    let category = category(&app, "EXPENSE", "Groceries");
    let schema = run(&app, "", &["schema"]).json();
    let mut tried = 0;
    for command in schema["commands"].as_array().unwrap() {
        if command["scope"] != "read" || command["command"] == "schema" || command["confirm"] == true {
            continue;
        }
        let name = command["command"].as_str().unwrap();
        let args = sample(name, &category);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        // The JSON is exactly what the route returned, and a write token reads the same.
        let data = ok(&app, &app.read, &args);
        let http = ok(&app, &app.write, &args);
        if name != "status" {
            assert_eq!(data, http, "{name}: a write token and a read token see the same");
        } else {
            assert_eq!((data["scope"].clone(), http["scope"].clone()), (json!("read"), json!("write")), "status says what the token may do");
        }
        // The plain form is text a person can read: no escapes, no JSON, nothing on stderr.
        let plain = run(&app, &app.read, &args);
        assert_eq!(plain.code, 0, "{name}: {}", plain.err);
        assert!(plain.err.is_empty() || plain.err.starts_with("showing ") || (data.is_null() && plain.err == "there is nothing to show.\n"), "{name}: {}", plain.err);
        assert!((!plain.out.is_empty() || data.is_null()) && !plain.out.contains('\u{1b}') && !plain.out.starts_with('{'), "{name}: {:?}", plain.out);
        tried += 1;
    }
    assert!(tried >= 16, "only {tried} read commands were tried");
}

/// The shape of the JSON is a promise: fields may be added, never removed or changed in type.
#[test]
fn the_json_shapes_are_stable() {
    let app = app(true);
    let groceries = category(&app, "EXPENSE", "Groceries");
    let has = |value: &Value, keys: &[&str], what: &str| {
        for key in keys {
            assert!(value.get(*key).is_some(), "{what} lost its `{key}`: {value}");
        }
    };
    let list = ok(&app, &app.read, &["transactions", "list", "--limit", "2"]);
    has(&list, &["limit", "offset", "total", "transactions"], "transactions list");
    let row = &list["transactions"][0];
    has(row, &["amount", "categoryId", "clientUuid", "description", "id", "name", "paymentDate"], "a transaction");
    assert!(row["amount"].is_string() && row["id"].is_string() && row["categoryId"].is_string(), "money and ids are strings");
    assert!(list["total"].is_u64() && list["limit"].is_u64());

    has(&ok(&app, &app.read, &["categories", "list"])[0], &["id", "name", "type"], "a category");
    has(&ok(&app, &app.read, &["settings", "get"]), &["currency", "language", "locale", "timezone"], "settings");
    has(&ok(&app, &app.read, &["status"]), &["api", "app", "schemaVersion", "scope", "version"], "status");
    has(&ok(&app, &app.read, &["today"]), &["date", "month"], "today");
    has(&ok(&app, &app.read, &["budgets", "overview", "--month", "2026-09"])[0], &["type", "budgeted", "allocated", "unallocated", "actual", "remaining"], "a budget overview");
    has(&ok(&app, &app.read, &["budgets", "history"])[0], &["month", "expenseBudget", "expenseActual", "incomeBudget", "incomeActual"], "a history row");
    has(&ok(&app, &app.read, &["recurring", "list"])[0], &["amount", "categoryId", "description", "endDate", "id", "intervalCount", "intervalUnit", "isActive", "name", "nextDueDate", "startDate"], "a recurring item");
    has(&ok(&app, &app.read, &["recurring", "dues"]), &["overdue", "upcoming"], "dues");
    let budget = ok(&app, &app.read, &["stats", "budget", "--month", "2026-09"]);
    has(&budget, &["month", "categories", "totals"], "budget against actual");
    has(&budget["categories"][0], &["categoryId", "name", "type", "budgeted", "actual", "remaining", "pctUsed"], "a category line");
    has(&ok(&app, &app.read, &["stats", "year", "--year", "2026"])[0], &["month", "income", "expense", "savings", "savingsRate"], "a year point");
    has(&ok(&app, &app.read, &["stats", "top", "--month", "2026-09", "--type", "expense"])[0], &["categoryId", "name", "actual", "share"], "a top category");
    has(&ok(&app, &app.read, &["stats", "commitments"]), &["items"], "commitments");
    has(&ok(&app, &app.read, &["stats", "trend", "--category-id", &groceries, "--from", "2026-01", "--to", "2026-03"]), &["categoryId", "points"], "a trend");
    assert!(ok(&app, &app.read, &["stats", "planned", "--month", "2026-09"]).is_array());
}

// ---- the plan's "done when": only a token, no prompts ----------------------------------------

#[test]
fn with_only_a_token_a_script_can_record_expenses_set_a_budget_and_read_budget_against_actual() {
    let app = app(false);
    let food = ok(&app, &app.write, &["categories", "create", "--name", "Food", "--type", "expense"]);
    assert_eq!(food["name"], "Food");
    let id = food["id"].as_str().unwrap().to_string();
    let budget = ok(&app, &app.write, &["budgets", "set", "--type", "expense", "--month", "2026-03", "--amount-limit", "400.00", "--category-id", &id]);
    assert_eq!((budget["scope"].clone(), budget["amountLimit"].clone()), (json!("CATEGORY"), json!("400.00")));

    let uuids = ["11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222", "33333333-3333-4333-8333-333333333333"];
    for (uuid, amount) in uuids.iter().zip(["120.50", "99.70", "136.00"]) {
        let args = ["transactions", "create", "--name", "Shop", "--amount", amount, "--payment-date", "2026-03-12", "--category-id", &id, "--client-uuid", uuid];
        let first = ok(&app, &app.write, &args);
        // The script did not hear back and runs it again: the same transaction, not a second.
        assert_eq!(ok(&app, &app.write, &args), first);
        assert_eq!(first["amount"], amount);
    }
    let month = ok(&app, &app.read, &["stats", "budget", "--month", "2026-03"]);
    let line = month["categories"].as_array().unwrap().iter().find(|c| c["name"] == "Food").unwrap();
    assert_eq!((line["budgeted"].clone(), line["actual"].clone(), line["remaining"].clone()), (json!("400.00"), json!("356.20"), json!("43.80")));

    // The same for a person: a table.
    let plain = run(&app, &app.read, &["stats", "budget", "--month", "2026-03"]);
    assert_eq!(plain.code, 0);
    let row = plain.out.lines().find(|l| l.contains("Food")).unwrap();
    assert!(row.contains("400.00") && row.contains("356.20") && row.contains("43.80") && row.contains("89.1"), "{row}");
}

#[test]
fn a_new_transaction_gets_a_client_uuid_that_can_be_sent_again() {
    let app = app(false);
    let food = ok(&app, &app.write, &["categories", "create", "--name", "Food", "--type", "expense"])["id"].as_str().unwrap().to_string();
    let args = ["transactions", "create", "--name", "Tea", "--amount", "2.00", "--payment-date", "2026-10-14", "--category-id", &food];
    let first = ok(&app, &app.write, &args);
    let uuid = first["clientUuid"].as_str().unwrap().to_string();
    assert_eq!(uuid.len(), 36);
    let again = ok(&app, &app.write, &[&args[..], &["--client-uuid", &uuid]].concat());
    assert_eq!(again["id"], first["id"], "sending the uuid the CLI made gives the first transaction back");
    // Without a uuid every run is a new transaction with its own.
    let second = ok(&app, &app.write, &args);
    assert_ne!(second["id"], first["id"]);
    assert_ne!(second["clientUuid"], first["clientUuid"]);
    assert_eq!(ok(&app, &app.read, &["transactions", "list"])["total"], 2);
}

#[test]
fn changes_flow_through_the_commands_the_way_the_app_does_them() {
    let app = app(true);
    let groceries = category(&app, "EXPENSE", "Groceries");
    let made = ok(&app, &app.write, &["transactions", "create", "--name", "Bread", "--amount", "2.40", "--payment-date", "2026-10-14", "--category-id", &groceries, "--description", "Sourdough"]);
    let id = made["id"].as_str().unwrap();
    let updated = ok(&app, &app.write, &["transactions", "update", id, "--name", "Rye", "--amount", "2.95"]);
    assert_eq!((updated["name"].clone(), updated["amount"].clone(), updated["description"].clone()), (json!("Rye"), json!("2.95"), json!("Sourdough")));
    let cleared = ok(&app, &app.write, &["transactions", "update", id, "--clear-description"]);
    assert!(cleared["description"].is_null());

    let gym = ok(&app, &app.write, &["recurring", "create", "--name", "Gym", "--amount", "29.99", "--category-id", &groceries, "--start-date", "2026-10-20", "--interval-unit", "month", "--interval-count", "1"]);
    let gym_id = gym["id"].as_str().unwrap();
    assert_eq!((gym["intervalUnit"].clone(), gym["nextDueDate"].clone()), (json!("MONTH"), json!("2026-10-20")));
    let varying = ok(&app, &app.write, &["recurring", "update", gym_id, "--clear-amount"]);
    assert!(varying["amount"].is_null());
    let paid = ok(&app, &app.write, &["recurring", "pay", gym_id, "--amount", "31.00", "--paid-on", "2026-10-14"]);
    assert_eq!((paid["amount"].clone(), paid["paymentDate"].clone()), (json!("31.00"), json!("2026-10-14")));
    assert_eq!(ok(&app, &app.write, &["recurring", "pause", gym_id])["isActive"], false);
    assert_eq!(ok(&app, &app.write, &["recurring", "resume", gym_id])["isActive"], true);
    assert!(ok(&app, &app.write, &["recurring", "skip", gym_id])["nextDueDate"].is_string());

    let settings = ok(&app, &app.write, &["settings", "set", "--language", "ro"]);
    assert_eq!(settings["language"], "ro");
    assert_eq!(settings["currency"], "EUR");
    assert_eq!(failed(&app, &app.write, &["settings", "set", "--currency", "USD"]), (2, "usage".into()), "the currency is changed in the app");
}

// ---- failing the documented way --------------------------------------------------------------

#[test]
fn a_value_the_app_refuses_is_exit_2_with_the_reason_and_nothing_changes() {
    let app = app(true);
    let groceries = category(&app, "EXPENSE", "Groceries");
    let before = snapshot(&app);
    for args in [
        vec!["transactions", "create", "--name", "X", "--amount", "2.4", "--payment-date", "2026-10-14", "--category-id", &groceries],
        vec!["transactions", "create", "--name", "X", "--amount", "0.00", "--payment-date", "2026-10-14", "--category-id", &groceries],
        vec!["transactions", "create", "--name", "X", "--amount", "1.00", "--payment-date", "2026-02-30", "--category-id", &groceries],
        vec!["transactions", "create", "--name", "X", "--amount", "1.00", "--payment-date", "yesterday", "--category-id", &groceries],
        vec!["transactions", "create", "--name", "X", "--amount", "1.00", "--payment-date", "2026-10-14", "--category-id", "zero"],
        vec!["transactions", "create", "--name", "", "--amount", "1.00", "--payment-date", "2026-10-14", "--category-id", &groceries],
        vec!["budgets", "set", "--type", "expense", "--month", "2026-13", "--amount-limit", "1.00"],
        vec!["budgets", "overview", "--month", "October"],
        vec!["transactions", "list", "--limit", "0"],
    ] {
        let mut all = args.clone();
        all.push("--json");
        let out = run(&app, &app.write, &all);
        assert_eq!((out.code, out.out.as_str()), (2, ""), "{args:?}: {}", out.err);
        assert_eq!(out.error()["code"], "invalid_input", "{args:?}");
        assert!(!out.error()["message"].as_str().unwrap().is_empty());
    }
    let amount = run(&app, &app.write, &["transactions", "create", "--name", "X", "--amount", "2.4", "--payment-date", "2026-10-14", "--category-id", &groceries]);
    assert!(amount.err.contains("amount") && amount.err.contains("12.50"), "{}", amount.err);
    assert_eq!(snapshot(&app), before, "a refused command changed the data");
}

#[test]
fn usage_mistakes_are_exit_2_and_never_reach_the_app() {
    let app = app(true);
    let before = snapshot(&app);
    // No server at all would make a request fail with exit 1; these fail on their own, with 2.
    for args in [
        vec!["transactions", "create", "--name", "X"],
        vec!["transactions", "create", "--nope", "1"],
        vec!["transactions", "list", "--limit", "many"],
        vec!["categories", "list", "--type", "both"],
        vec!["transactions"],
        vec!["transactions", "delete", "5"],
        vec!["budgets", "remove", "--type", "expense", "--month", "2026-10"],
        vec![],
    ] {
        let mut each = Command::new(env!("CARGO_BIN_EXE_meowny"));
        clean(&mut each).env("MEOWNY_TOKEN", &app.write).env("MEOWNY_PORT", "1").args(&args);
        let out = finish(each);
        assert_eq!((out.code, out.out.as_str()), (2, ""), "{args:?}: {}", out.err);
        assert!(out.err.starts_with("error: "), "{args:?}: {}", out.err);
    }
    assert_eq!(snapshot(&app), before);
}

#[test]
fn deleting_needs_yes_and_without_it_nothing_is_deleted() {
    let app = app(true);
    let groceries = category(&app, "EXPENSE", "Groceries");
    let made = ok(&app, &app.write, &["transactions", "create", "--name", "Bread", "--amount", "2.40", "--payment-date", "2026-10-14", "--category-id", &groceries]);
    let id = made["id"].as_str().unwrap();
    let before = snapshot(&app);
    assert_eq!(failed(&app, &app.write, &["transactions", "delete", id]), (2, "usage".into()));
    assert_eq!(snapshot(&app), before);
    let out = run(&app, &app.write, &["transactions", "delete", id, "--yes"]);
    assert_eq!((out.code, out.out.as_str(), out.err.as_str()), (0, "", "done.\n"));
    assert_eq!(failed(&app, &app.write, &["transactions", "delete", id, "--yes"]), (4, "not_found".into()), "deleting twice");
    // Ending a recurring item and removing a budget ask the same.
    assert_eq!(failed(&app, &app.write, &["recurring", "end", "1"]).1, "usage");
    assert_eq!(failed(&app, &app.write, &["categories", "delete", &groceries]).1, "usage");
}

#[test]
fn something_that_is_not_there_is_exit_4() {
    let app = app(true);
    for args in [
        vec!["transactions", "delete", "999999", "--yes"],
        vec!["transactions", "update", "999999", "--name", "X"],
        vec!["categories", "rename", "999999", "--name", "X"],
        vec!["recurring", "pay", "999999"],
        vec!["recurring", "end", "999999", "--yes"],
        vec!["stats", "trend", "--category-id", "999999", "--from", "2026-01", "--to", "2026-02"],
    ] {
        assert_eq!(failed(&app, &app.write, &args), (4, "not_found".into()), "{args:?}");
    }
}

#[test]
fn a_token_the_app_does_not_accept_is_exit_3() {
    let app = app(true);
    let unknown = format!("mny_{}", "0".repeat(64));
    assert_eq!(failed(&app, &unknown, &["status"]), (3, "unauthorized".into()));
    let revoked = token(&app.vault, Scope::Read, 30);
    assert_eq!(run(&app, &revoked, &["status"]).code, 0);
    let ids: Vec<i64> = app.vault.with_conn(|c| Ok(tokens::list(c, chrono::Utc::now())?.into_iter().map(|t| t.id).collect())).unwrap();
    for id in ids {
        app.vault.with_conn(|c| tokens::revoke(c, id)).unwrap();
    }
    assert_eq!(failed(&app, &revoked, &["status"]), (3, "unauthorized".into()), "revoked");
    // Expired.
    let short = token(&app.vault, Scope::Read, 1);
    app.vault.with_conn(|c| Ok(c.execute("UPDATE api_tokens SET expires_at = '2000-01-01T00:00:00Z'", [])?)).unwrap();
    assert_eq!(failed(&app, &short, &["status"]), (3, "unauthorized".into()), "expired");
    // No token, and a token of the wrong shape, never get as far as the app.
    assert_eq!(failed(&app, "", &["status"]).0, 3);
    let (code, kind) = failed(&app, "not a token", &["status"]);
    assert_eq!((code, kind.as_str()), (3, "bad_token"));
}

#[test]
fn a_read_token_cannot_change_anything_and_the_cli_says_so() {
    let app = app(true);
    let groceries = category(&app, "EXPENSE", "Groceries");
    let before = snapshot(&app);
    let schema = run(&app, "", &["schema"]).json();
    let mut tried = 0;
    // a file that can be sent, for the command that takes one
    let sample_file = app.out("sample.bin");
    std::fs::write(&sample_file, b"a file").unwrap();
    for command in schema["commands"].as_array().unwrap().iter().filter(|c| c["scope"] == "write") {
        let words: Vec<&str> = command["command"].as_str().unwrap().split(' ').collect();
        // Every required flag with a plausible value, so the request is valid in everything but the token.
        let mut args: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        for flag in command["flags"].as_array().unwrap().iter().filter(|f| f["required"] == true) {
            let value = match flag["type"].as_str().unwrap() {
                "money" => "1.00".to_string(),
                "date" => "2026-10-14".to_string(),
                "month" => "2026-10".to_string(),
                "id" => groceries.clone(),
                "integer" => "1".to_string(),
                "bytes" => sample_file.clone(),
                _ if !flag["values"].as_array().unwrap().is_empty() => flag["values"][0].as_str().unwrap().to_string(),
                _ => "x".to_string(),
            };
            args.push(flag["flag"].as_str().unwrap().to_string());
            args.push(value);
        }
        args.push("--yes".into());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(failed(&app, &app.read, &args), (3, "forbidden".into()), "{args:?}");
        tried += 1;
    }
    assert!(tried >= 23, "only {tried} write commands were tried");
    assert_eq!(snapshot(&app), before, "a read token changed something");
}

#[test]
fn a_locked_app_is_exit_3_with_no_data_and_works_again_when_unlocked() {
    let app = app(true);
    app.vault.lock();
    for args in [vec!["categories", "list"], vec!["transactions", "list"], vec!["status"], vec!["transactions", "create", "--name", "X", "--amount", "1.00", "--payment-date", "2026-10-14", "--category-id", "1"]] {
        let token = if args[0] == "transactions" && args[1] == "create" { &app.write } else { &app.read };
        assert_eq!(failed(&app, token, &args), (3, "locked".into()), "{args:?}");
    }
    let plain = run(&app, &app.read, &["categories", "list"]);
    assert_eq!((plain.code, plain.out.as_str()), (3, ""));
    assert!(plain.err.contains("locked") && plain.err.contains("Unlock it in the app"), "{}", plain.err);
    app.vault.unlock(None).unwrap();
    assert_eq!(run(&app, &app.read, &["categories", "list"]).code, 0);
}

#[test]
fn an_app_that_is_not_running_or_has_the_api_off_is_exit_1() {
    let mut app = app(false);
    assert_eq!(run(&app, &app.read, &["status"]).code, 0);
    let file = app.dir.path().join("api.json");
    assert!(file.exists());
    // The app stops listening (command line access turned off, or the app closed).
    app.server = None;
    assert!(!file.exists());
    let out = run(&app, &app.read, &["status", "--json"]);
    assert_eq!((out.code, out.error()["code"].clone()), (1, json!("app_unavailable")));
    assert!(out.error()["message"].as_str().unwrap().contains("Settings"));
    // A leftover file from a crash points at a port nobody listens on.
    let closed = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
    std::fs::write(&file, format!(r#"{{"api":1,"port":{closed}}}"#)).unwrap();
    assert_eq!(failed(&app, &app.read, &["status"]), (1, "app_unavailable".into()));
    // An app that speaks another API version.
    std::fs::write(&file, format!(r#"{{"api":2,"port":{closed}}}"#)).unwrap();
    let out = run(&app, &app.read, &["status"]);
    assert_eq!(out.code, 1);
    assert!(out.err.contains("API 2"), "{}", out.err);
}

// ---- the token ------------------------------------------------------------------------------

#[test]
fn the_token_is_never_printed_not_on_success_and_not_on_any_failure() {
    let app = app(true);
    let groceries = category(&app, "EXPENSE", "Groceries");
    let hash: String = app.vault.with_conn(|c| Ok(c.query_row("SELECT token_hash FROM api_tokens LIMIT 1", [], |r| r.get(0))?)).unwrap();
    let mut seen = String::new();
    for token in [app.read.clone(), app.write.clone(), format!("mny_{}", "ab".repeat(32)), "not a token".to_string()] {
        for args in [
            vec!["status"],
            vec!["status", "--json"],
            vec!["schema"],
            vec!["schema", "--app"],
            vec!["transactions", "list", "--limit", "2"],
            vec!["transactions", "create", "--name", "X", "--amount", "bad", "--payment-date", "2026-10-14", "--category-id", &groceries],
            vec!["transactions", "delete", "999999", "--yes"],
            vec!["categories", "create", "--name", "Pets", "--type", "expense", "--json"],
            vec!["nope"],
            vec!["--help"],
            vec!["transactions", "create", "--help"],
        ] {
            let out = run(&app, &token, &args);
            seen.push_str(&out.all());
        }
        for file_arg in ["--token-file", "--data-dir"] {
            seen.push_str(&run(&app, &token, &["status", file_arg, "/does/not/exist"]).all());
        }
    }
    for secret in [&app.read, &app.write, &hash] {
        assert!(!seen.contains(secret.as_str()), "the token showed up in the output");
        assert!(!seen.contains(&secret[4..]), "the token's letters showed up in the output");
    }
    assert!(seen.len() > 5000, "the scenarios printed almost nothing: {}", seen.len());
}

#[cfg(unix)]
fn private(path: &std::path::Path, text: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(unix)]
#[test]
fn the_token_can_come_from_a_file_and_a_file_that_others_can_read_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let app = app(false);
    let file = app.dir.path().join("token");
    private(&file, &format!("{}\n", app.write), 0o600);
    let mut c = command(&app);
    c.args(["status", "--json", "--token-file", file.to_str().unwrap()]);
    let out = finish(c);
    assert_eq!((out.code, out.json()["scope"].clone()), (0, json!("write")));
    // The same through the variable that names a file.
    let mut c = command(&app);
    c.env("MEOWNY_TOKEN_FILE", &file).args(["status", "--json"]);
    assert_eq!(finish(c).json()["scope"], "write");

    for mode in [0o644, 0o640, 0o604, 0o666] {
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
        let mut c = command(&app);
        c.args(["status", "--json", "--token-file", file.to_str().unwrap()]);
        let out = finish(c);
        assert_eq!((out.code, out.out.as_str(), out.error()["code"].clone()), (3, "", json!("token_file_unsafe")), "{mode:o}");
        assert!(out.err.contains("chmod 600") && !out.err.contains(&app.write));
    }
    // The flag beats the variable: a bad file named by the flag is not rescued by a good variable.
    let mut c = command(&app);
    c.env("MEOWNY_TOKEN", &app.read).args(["status", "--json", "--token-file", "/does/not/exist"]);
    assert_eq!(finish(c).code, 3);
}

// The saved token's place follows `XDG_CONFIG_HOME` on Linux; macOS and Windows keep it elsewhere.
#[cfg(target_os = "linux")]
#[test]
fn login_saves_a_token_that_later_commands_use_and_logout_forgets_it() {
    let app = app(false);
    let config = app.dir.path().join("config");
    let with_config = |args: &[&str]| {
        let mut c = command(&app);
        c.env("XDG_CONFIG_HOME", &config).args(args);
        c
    };
    // Nothing saved yet.
    assert_eq!(finish(with_config(&["status", "--json"])).error()["code"], "no_token");
    // The token goes in on standard input, never on the command line.
    let mut login = with_config(&["login"]);
    login.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = login.spawn().unwrap();
    {
        use std::io::Write;
        child.stdin.take().unwrap().write_all(format!("{}\n", app.read).as_bytes()).unwrap();
    }
    let done = child.wait_with_output().unwrap();
    assert_eq!(done.status.code(), Some(0));
    let saved = config.join("meowny/token");
    assert!(String::from_utf8_lossy(&done.stderr).contains(saved.to_str().unwrap()));
    assert!(!String::from_utf8_lossy(&done.stderr).contains(&app.read) && done.stdout.is_empty());
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&saved).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let out = finish(with_config(&["status", "--json"]));
    assert_eq!((out.code, out.json()["scope"].clone()), (0, json!("read")));
    let out = finish(with_config(&["logout"]));
    assert_eq!(out.code, 0);
    assert!(!saved.exists());
    assert_eq!(finish(with_config(&["status", "--json"])).error()["code"], "no_token");
}

// ---- behaving well in scripts ----------------------------------------------------------------

#[test]
fn a_command_never_waits_for_input_it_was_not_asked_to_read() {
    let app = app(false);
    let mut c = command(&app);
    c.env("MEOWNY_TOKEN", &app.read).args(["status", "--json"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    let _open_stdin = child.stdin.take();
    let started = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(10), "the command is waiting on standard input");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_text_flag_given_a_dash_reads_standard_input() {
    let app = app(false);
    let food = ok(&app, &app.write, &["categories", "create", "--name", "Food", "--type", "expense"])["id"].as_str().unwrap().to_string();
    let mut c = command(&app);
    c.env("MEOWNY_TOKEN", &app.write)
        .args(["transactions", "create", "--name", "Lunch", "--amount", "9.90", "--payment-date", "2026-10-14", "--category-id", &food, "--description", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    {
        use std::io::Write;
        child.stdin.take().unwrap().write_all("Lunch with Sam, \"quoted\", ünïcode 日本語 & $HOME; rm -rf\n".as_bytes()).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let made: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    assert_eq!(made["description"], "Lunch with Sam, \"quoted\", ünïcode 日本語 & $HOME; rm -rf", "taken as is, without the last line break");
}

#[test]
fn control_characters_cannot_get_into_the_data_so_they_cannot_reach_a_terminal_from_it() {
    let app = app(false);
    let food = ok(&app, &app.write, &["categories", "create", "--name", "Food", "--type", "expense"])["id"].as_str().unwrap().to_string();
    let before = snapshot(&app);
    // The app refuses them in names and notes, so a list can never print an escape sequence it was given.
    for (name, note) in [("\u{1b}[31mred", "x"), ("plain", "a\u{1b}[2Jb"), ("bell\u{7}", "x"), ("plain", "two\nlines"), ("tab\there", "x")] {
        let args = ["transactions", "create", "--name", name, "--amount", "1.00", "--payment-date", "2026-10-14", "--category-id", &food, "--description", note];
        assert_eq!(failed(&app, &app.write, &args), (2, "invalid_input".into()), "{name:?} {note:?}");
    }
    assert_eq!(failed(&app, &app.write, &["categories", "create", "--name", "Pets\u{1b}[0m", "--type", "expense"]).0, 2);
    assert_eq!(snapshot(&app), before);
    // Text with accents, quotes and other scripts is fine, and comes back intact in both forms.
    let name = "Café “Zoë” 日本語";
    ok(&app, &app.write, &["transactions", "create", "--name", name, "--amount", "1.00", "--payment-date", "2026-10-14", "--category-id", &food]);
    assert_eq!(ok(&app, &app.read, &["transactions", "list"])["transactions"][0]["name"], name);
    assert!(run(&app, &app.read, &["transactions", "list"]).out.contains(name));
}

#[test]
fn many_scripts_at_once_do_not_trip_over_each_other() {
    let app = app(false);
    let food = ok(&app, &app.write, &["categories", "create", "--name", "Food", "--type", "expense"])["id"].as_str().unwrap().to_string();
    let path = env!("CARGO_BIN_EXE_meowny");
    let (data, write) = (app.dir.path().to_path_buf(), app.write.clone());
    let workers: Vec<_> = (0..8)
        .map(|i| {
            let (data, write, food) = (data.clone(), write.clone(), food.clone());
            std::thread::spawn(move || {
                (0..3)
                    .map(|j| {
                        let mut c = Command::new(path);
                        clean(&mut c)
                            .env("MEOWNY_DATA_DIR", &data)
                            .env("MEOWNY_TOKEN", &write)
                            .args(["transactions", "create", "--name", &format!("T{i}-{j}"), "--amount", "1.00", "--payment-date", "2026-10-14", "--category-id", &food, "--json"])
                            .output()
                            .unwrap()
                            .status
                            .code()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    for worker in workers {
        assert!(worker.join().unwrap().iter().all(|c| *c == Some(0)));
    }
    assert_eq!(ok(&app, &app.read, &["transactions", "list", "--limit", "1"])["total"], 24);
}

// ---- the commands the person must allow --------------------------------------------------------

#[test]
fn an_export_is_written_after_the_person_allows_it_and_not_before() {
    let app = app(true);
    let json = app.out("all.json");
    // The person takes a moment, so someone at a terminal is told where to look.
    *app.delay.lock().unwrap() = Duration::from_millis(1300);
    let out = run(&app, &app.write, &["export", "json", "--out", &json]);
    assert_eq!(out.code, 0, "{}", out.err);
    assert_eq!(out.err, "waiting for you to allow this in the meowNY app...\n");
    *app.delay.lock().unwrap() = Duration::ZERO;
    assert!(out.out.contains(&json) && out.out.contains("bytes"));
    let file: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert!(file["transactions"].as_array().unwrap().len() > 100);
    assert_eq!(app.asked(), 1);
    let said = app.shown.lock().unwrap()[0].text.clone();
    assert!(said.contains(&json) && said.contains("\"test\"") && said.contains("JSON"), "{said}");

    // As JSON for a script: the answer is the path and size, and nothing else is printed.
    let csv = app.out("t.csv");
    let made = ok(&app, &app.read, &["export", "csv", "--out", &csv]);
    assert_eq!(made["path"], csv.as_str());
    assert!(made["bytes"].as_u64().unwrap() > 1000);
    assert!(std::fs::read_to_string(&csv).unwrap().starts_with("\u{feff}date,type,category"));
    let xlsx = app.out("w.xlsx");
    ok(&app, &app.write, &["export", "xlsx", "--out", &xlsx]);
    assert_eq!(&std::fs::read(&xlsx).unwrap()[..2], b"PK");
    let pdf = app.out("r.pdf");
    ok(&app, &app.write, &["export", "pdf", "--out", &pdf, "--from-month", "2026-01", "--to-month", "2026-03", "--include-transactions", "--page", "letter"]);
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF-"));
    assert!(app.shown.lock().unwrap().last().unwrap().text.contains("Transaction list: yes"));
    assert_eq!(app.asked(), 4, "one question for each");
}

#[test]
fn a_no_in_the_app_is_exit_3_denied_and_nothing_is_written() {
    let app = app(true);
    *app.answer.lock().unwrap() = false;
    let file = app.out("never.json");
    assert_eq!(failed(&app, &app.write, &["export", "json", "--out", &file]), (3, "denied".into()));
    assert!(!std::path::Path::new(&file).exists());
    let plain = run(&app, &app.write, &["export", "csv", "--out", &app.out("never.csv")]);
    assert_eq!((plain.code, plain.out.as_str()), (3, ""));
    assert!(plain.err.contains("a moment ago"), "asking again at once is refused without asking the person: {}", plain.err);
    assert_eq!(app.asked(), 1);
    assert!(std::fs::read_dir(app.files.path()).unwrap().next().is_none(), "nothing was written");
}

#[test]
fn an_existing_file_is_never_replaced_and_the_person_is_not_asked_about_it() {
    let app = app(true);
    let file = app.out("mine.json");
    std::fs::write(&file, "my notes").unwrap();
    assert_eq!(failed(&app, &app.write, &["export", "json", "--out", &file]), (1, "file_exists".into()));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "my notes");
    assert_eq!(app.asked(), 0);
    // A name that is not an export of that kind, or not in a folder that exists, never asks either.
    for args in [
        vec!["export", "json", "--out", "/definitely/not/a/folder/x.json"],
        vec!["export", "csv", "--out", "/tmp/x.txt"],
    ] {
        assert_eq!(failed(&app, &app.write, &args), (2, "invalid_input".into()), "{args:?}");
    }
    let weird = app.out("a\nb.json");
    assert_eq!(failed(&app, &app.write, &["export", "json", "--out", &weird]), (2, "invalid_input".into()), "a name that could mislead the dialog");
    assert_eq!(app.asked(), 0);
}

#[test]
fn a_relative_path_is_made_absolute_from_where_the_program_runs() {
    let app = app(true);
    let mut c = command(&app);
    c.current_dir(app.files.path()).env("MEOWNY_TOKEN", &app.write).args(["export", "csv", "--out", "sub/../relative.csv", "--json"]);
    let out = finish(c);
    assert_eq!(out.code, 0, "{}", out.err);
    let shown = out.json()["path"].as_str().unwrap().to_string();
    let expected = std::fs::canonicalize(app.files.path()).unwrap().join("relative.csv");
    assert_eq!(std::fs::canonicalize(&shown).unwrap(), expected);
    assert!(app.shown.lock().unwrap()[0].text.contains(&shown), "the person is shown the full path, not the one that was typed");
    assert!(!shown.contains(".."));
}

#[test]
fn a_selection_by_category_and_date_narrows_the_file() {
    let app = app(true);
    let categories = ok(&app, &app.read, &["categories", "list", "--type", "expense"]);
    let (a, b) = (categories[0].clone(), categories[1].clone());
    let ids = format!("{},{}", a["id"].as_str().unwrap(), b["id"].as_str().unwrap());
    let file = app.out("two.json");
    ok(&app, &app.write, &["export", "json", "--out", &file, "--category-ids", &ids, "--from", "2026-09-01", "--to", "2026-09-30"]);
    let text = std::fs::read_to_string(&file).unwrap();
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["categories"].as_array().unwrap().len(), 2);
    assert!(parsed["transactions"].as_array().unwrap().iter().all(|t| t["date"].as_str().unwrap().starts_with("2026-09")));
    assert!(app.shown.lock().unwrap()[0].text.contains("Only the 2 categories chosen"));
    let preview = ok(&app, &app.read, &["export", "preview", "--category-ids", &ids, "--from", "2026-09-01", "--to", "2026-09-30"]);
    assert_eq!(preview["transactions"], parsed["transactions"].as_array().unwrap().len());
}

#[test]
fn a_backup_takes_its_passphrase_from_standard_input_and_opens_only_with_it() {
    let app = app(true);
    let file = app.out("b.meowbak");
    let mut c = command(&app);
    c.env("MEOWNY_TOKEN", &app.write)
        .args(["backup", "create", "--out", &file, "--passphrase", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    {
        use std::io::Write;
        child.stdin.take().unwrap().write_all(b"a long enough passphrase\n").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let bytes = std::fs::read(&file).unwrap();
    assert!(meowny_lib::backup::open(&bytes, "a long enough passphrase").is_ok());
    assert!(meowny_lib::backup::open(&bytes, "another passphrase here").is_err());
    let said = app.shown.lock().unwrap()[0].text.clone();
    assert!(!said.contains("a long enough passphrase") && said.contains("encrypted"), "{said}");
    // Nothing the program printed holds the passphrase either.
    assert!(!String::from_utf8_lossy(&out.stdout).contains("a long enough") && !String::from_utf8_lossy(&out.stderr).contains("a long enough"));
    // Too short: refused before the person is asked.
    let asked = app.asked();
    assert_eq!(failed(&app, &app.write, &["backup", "create", "--out", &app.out("c.meowbak"), "--passphrase", "short"]), (2, "invalid_input".into()));
    assert_eq!(app.asked(), asked);
}

#[test]
fn there_is_no_command_to_restore_a_backup_or_to_change_the_currency_or_to_delete_everything() {
    let app = app(false);
    for words in [vec!["backup", "restore"], vec!["restore"], vec!["settings", "currency"], vec!["wipe"], vec!["delete-all"], vec!["tokens", "create"]] {
        let out = run(&app, &app.write, &words);
        assert_eq!((out.code, out.out.as_str()), (2, ""), "{words:?}");
        assert!(out.err.contains("unknown command") || out.err.contains("needs one of"), "{words:?}: {}", out.err);
    }
    assert_eq!(app.asked(), 0);
}

// ---- every command, once, against a real app --------------------------------------------------

/// Runs commands and remembers which ones, so a command nobody tested is a failure.
struct Tour<'a> {
    app: &'a App,
    known: BTreeSet<String>,
    used: BTreeSet<String>,
}

impl Tour<'_> {
    fn note(&mut self, args: &[&str]) {
        for n in (1..=2.min(args.len())).rev() {
            let name = args[..n].join(" ");
            if self.known.contains(&name) {
                self.used.insert(name);
                return;
            }
        }
        panic!("{args:?} is not a command");
    }

    fn go(&mut self, token: &str, args: &[&str]) -> Value {
        self.note(args);
        ok(self.app, token, args)
    }
}

fn id_of(value: &Value) -> String {
    value["id"].as_str().unwrap().to_string()
}

#[test]
fn every_command_runs_against_a_real_app_and_a_new_one_without_a_test_fails_this() {
    let app = app(true);
    let schema = run(&app, "", &["schema"]).json();
    let known: BTreeSet<String> = schema["commands"].as_array().unwrap().iter().map(|c| c["command"].as_str().unwrap().to_string()).collect();
    let mut t = Tour { app: &app, known: known.clone(), used: BTreeSet::new() };
    let (r, w) = (app.read.clone(), app.write.clone());

    // The app itself.
    assert_eq!(t.go(&r, &["status"])["app"], "meowny");
    assert_eq!(t.go(&r, &["today"])["date"].as_str().unwrap().len(), 10);
    assert_eq!(t.go(&r, &["settings", "get"])["language"], "en");
    assert_eq!(t.go(&w, &["settings", "set", "--language", "ro", "--locale", "ro-RO"])["language"], "ro");
    assert_eq!(t.go(&r, &["settings", "get"])["locale"], "ro-RO");
    t.go(&w, &["settings", "set", "--language", "en", "--locale", "en-IE"]);

    // Categories.
    let before = t.go(&r, &["categories", "list"]).as_array().unwrap().len();
    let pets = id_of(&t.go(&w, &["categories", "create", "--name", "Pets", "--type", "expense"]));
    assert_eq!(t.go(&r, &["categories", "list"]).as_array().unwrap().len(), before + 1);
    assert_eq!(t.go(&w, &["categories", "rename", &pets, "--name", "Animals"])["name"], "Animals");

    // Transactions.
    let tx = id_of(&t.go(&w, &["transactions", "create", "--name", "Food", "--amount", "12.50", "--payment-date", "2026-10-14", "--category-id", &pets, "--description", "Kibble"]));
    assert_eq!(t.go(&r, &["transactions", "list", "--category-id", &pets])["total"], 1);
    assert_eq!(t.go(&w, &["transactions", "update", &tx, "--amount", "13.00"])["amount"], "13.00");
    assert!(t.go(&w, &["transactions", "delete", &tx, "--yes"]).is_null());
    assert_eq!(t.go(&r, &["transactions", "list", "--category-id", &pets])["total"], 0);

    // Budgets.
    assert_eq!(t.go(&w, &["budgets", "set", "--type", "expense", "--month", "2026-12", "--amount-limit", "100.00", "--category-id", &pets])["amountLimit"], "100.00");
    assert!(t.go(&r, &["budgets", "overview", "--month", "2026-12"]).is_array());
    assert!(t.go(&r, &["budgets", "history"]).is_array());
    t.go(&w, &["budgets", "remove", "--type", "expense", "--month", "2027-01", "--category-id", &pets, "--yes"]);

    // Recurring items.
    let item = id_of(&t.go(&w, &["recurring", "create", "--name", "Vet plan", "--amount", "20.00", "--category-id", &pets, "--start-date", "2026-10-20", "--interval-unit", "month", "--interval-count", "1"]));
    assert!(t.go(&r, &["recurring", "list"]).as_array().unwrap().iter().any(|i| i["id"] == json!(item)));
    assert_eq!(t.go(&w, &["recurring", "update", &item, "--name", "Vet plan plus"])["name"], "Vet plan plus");
    assert!(t.go(&r, &["recurring", "dues", "--within-days", "30"])["upcoming"].is_array());
    assert_eq!(t.go(&w, &["recurring", "pay", &item, "--paid-on", "2026-10-14"])["amount"], "20.00");
    t.go(&w, &["recurring", "skip", &item]);
    assert_eq!(t.go(&w, &["recurring", "pause", &item])["isActive"], false);
    assert_eq!(t.go(&w, &["recurring", "resume", &item])["isActive"], true);
    assert_eq!(t.go(&w, &["recurring", "end", &item, "--yes"])["isActive"], false);
    assert!(t.go(&w, &["recurring", "delete", &item, "--yes"]).is_null());

    // Documents: the program reads the file itself and sends its bytes; a read token sees names only; getting a file out is allowed
    // in the app.
    let source = app.out("Boiler receipt.pdf");
    std::fs::write(&source, b"%PDF-1.4 the boiler was repaired").unwrap();
    let added = t.go(&w, &["documents", "add", &source, "--date", "2026-10-01", "--transaction-ids", "1"]);
    let doc = id_of(&added);
    assert_eq!((added["alreadyThere"].clone(), added["name"].clone(), added["linkCount"].clone()), (json!(false), json!("Boiler receipt.pdf"), json!(1)));
    assert_eq!(t.go(&r, &["documents", "list", "--q", "boiler"])["total"], 1);
    assert_eq!(t.go(&r, &["documents", "show", &doc])["links"][0]["kind"], "transaction");
    assert_eq!(t.go(&w, &["documents", "update", &doc, "--name", "Boiler", "--keep-on-phone"])["keepOnPhone"], true);
    assert!(t.go(&w, &["documents", "link", &doc, "--recurring-id", "1"]).is_null());
    assert!(t.go(&w, &["documents", "unlink", &doc, "--transaction-id", "1"]).is_null());
    let copy = app.out("copy.pdf");
    assert_eq!(t.go(&w, &["documents", "get", &doc, "--out", &copy])["bytes"], 32);
    assert_eq!(std::fs::read(&copy).unwrap(), b"%PDF-1.4 the boiler was repaired");
    assert!(t.go(&w, &["documents", "delete", &doc, "--yes"]).is_null());
    assert_eq!(t.go(&r, &["documents", "list"])["total"], 0);

    // Statistics.
    assert!(t.go(&r, &["stats", "budget", "--month", "2026-09"])["categories"].is_array());
    assert!(t.go(&r, &["stats", "trend", "--category-id", &pets, "--from", "2026-01", "--to", "2026-03"])["points"].is_array());
    assert!(t.go(&r, &["stats", "year", "--year", "2026"]).is_array());
    assert!(t.go(&r, &["stats", "top", "--month", "2026-09", "--type", "expense", "--limit", "2"]).is_array());
    assert!(t.go(&r, &["stats", "commitments"]).is_object());
    assert!(t.go(&r, &["stats", "planned", "--month", "2026-10"]).is_array());

    // Files, which the person has to allow.
    let preview = t.go(&r, &["export", "preview"]);
    assert!(preview["transactions"].as_u64().unwrap() > 100);
    assert!(t.go(&r, &["backup", "schedule"]).is_null());
    for (words, name, extra) in [
        (vec!["export", "json"], "t.json", vec![]),
        (vec!["export", "csv"], "t.csv", vec![]),
        (vec!["export", "xlsx"], "t.xlsx", vec![]),
        (vec!["export", "pdf"], "t.pdf", vec!["--from-month", "2026-01", "--to-month", "2026-02"]),
        (vec!["backup", "create"], "t.meowbak", vec!["--passphrase", "a long enough passphrase"]),
    ] {
        let out = app.out(name);
        let mut args = words.clone();
        args.extend(["--out", &out]);
        args.extend(extra.iter().copied());
        let made = t.go(&w, &args);
        assert_eq!(made["path"], out.as_str());
        assert!(std::path::Path::new(&out).exists(), "{words:?} wrote no file");
    }

    // Last, because it removes the category the rows above used.
    assert!(t.go(&w, &["categories", "delete", &pets, "--yes"]).is_null());

    // The commands that are about this program.
    assert!(t.go(&r, &["schema"])["commands"].is_array());
    assert!(t.go(&r, &["schema", "--app"])["routes"].is_array());
    let file = app.dir.path().join("saved-token");
    let file = file.to_str().unwrap();
    for (words, stdin) in [(vec!["login", "--token-file", file], Some(format!("{}\n", app.read))), (vec!["logout", "--token-file", file], None)] {
        t.note(&words);
        let mut c = command(&app);
        c.args(&words).stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c.spawn().unwrap();
        if let Some(text) = stdin {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
        }
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(0), "{words:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
    assert!(!std::path::Path::new(file).exists(), "logout removed what login saved");

    let missing: Vec<&String> = known.difference(&t.used).collect();
    assert!(missing.is_empty(), "these commands have no test here: {missing:?}");
    let unknown: Vec<&String> = t.used.difference(&known).collect();
    assert!(unknown.is_empty(), "{unknown:?}");
}

// ---- documents --------------------------------------------------------------------------------

#[test]
fn a_file_goes_in_through_the_program_whole_and_comes_out_only_when_the_person_allows_it() {
    let app = app(true);
    // several megabytes: a request far over the usual limit, read because the token has been let in
    let bytes: Vec<u8> = (0..6 * 1024 * 1024).map(|i: usize| ((i * 31 + i / 251) % 251) as u8).collect();
    let source = app.out("scan.bin");
    std::fs::write(&source, &bytes).unwrap();
    let added = ok(&app, &app.write, &["documents", "add", &source, "--transaction-ids", "1,2"]);
    assert_eq!((added["alreadyThere"].clone(), added["name"].clone(), added["sizeBytes"].clone()), (json!(false), json!("scan.bin"), json!(bytes.len())));
    let id = id_of(&added);
    assert_eq!(added["linkCount"], 2);

    // the same file again is not stored twice
    let again = ok(&app, &app.write, &["documents", "add", &source, "--name", "Another name"]);
    assert_eq!((again["alreadyThere"].clone(), id_of(&again)), (json!(true), id.clone()));
    assert_eq!(ok(&app, &app.read, &["documents", "list"])["total"], 1);

    // a read-only token sees the names and the hash, never the bytes
    let shown = ok(&app, &app.read, &["documents", "show", &id]);
    assert_eq!(shown["sha256"].as_str().unwrap().len(), 64);
    assert!(!shown.to_string().contains("data"), "{shown}");
    assert_eq!(failed(&app, &app.read, &["documents", "get", &id, "--out", &app.out("x.bin")]), (3, "forbidden".into()));
    assert!(!app.files.path().join("x.bin").exists());

    // out: the person is asked, and the file is exactly what went in
    let copy = app.out("copy.bin");
    let before = app.asked();
    let got = ok(&app, &app.write, &["documents", "get", &id, "--out", &copy]);
    assert_eq!(got["bytes"], bytes.len());
    assert_eq!(app.asked(), before + 1);
    assert!(std::fs::read(&copy).unwrap() == bytes, "the copy is not the file");
    let text = app.shown.lock().unwrap().last().unwrap().text.clone();
    assert!(text.contains("scan.bin") && text.contains(&copy) && text.contains("not encrypted"), "{text}");

    // a no writes nothing; a name of another kind is refused before the person is asked; a file that is there is never replaced
    *app.answer.lock().unwrap() = false;
    let refused = app.out("refused.bin");
    assert_eq!(failed(&app, &app.write, &["documents", "get", &id, "--out", &refused]), (3, "denied".into()));
    assert!(!std::path::Path::new(&refused).exists());
    *app.answer.lock().unwrap() = true;
    let asked = app.asked();
    assert_eq!(failed(&app, &app.write, &["documents", "get", &id, "--out", &app.out("run.sh")]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.write, &["documents", "get", &id, "--out", &copy]), (1, "file_exists".into()));
    assert_eq!(app.asked(), asked, "a request that could never work does not ask");
}

#[test]
fn a_file_that_cannot_be_read_is_exit_2_and_nothing_reaches_the_app() {
    let app = app(true);
    let before = snapshot(&app);
    let (code, _) = failed(&app, &app.write, &["documents", "add", &app.out("missing.pdf")]);
    assert_eq!(code, 2);
    std::fs::write(app.out("empty.pdf"), b"").unwrap();
    assert_eq!(failed(&app, &app.write, &["documents", "add", &app.out("empty.pdf")]), (2, "usage".into()));
    assert_eq!(failed(&app, &app.write, &["documents", "add", app.files.path().to_str().unwrap()]), (2, "usage".into()));
    assert_eq!(failed(&app, &app.write, &["documents", "add"]), (2, "usage".into()));
    // the app refuses what the program let through, such as an attachment to something that is not there
    std::fs::write(app.out("a.pdf"), b"%PDF-1.4 x").unwrap();
    assert_eq!(failed(&app, &app.write, &["documents", "add", &app.out("a.pdf"), "--transaction-ids", "999999"]), (4, "not_found".into()));
    assert_eq!(snapshot(&app), before);
}

#[test]
fn the_plain_form_of_the_document_commands_is_text_a_person_can_read() {
    let app = app(true);
    std::fs::write(app.out("Boiler receipt.pdf"), b"%PDF-1.4 the boiler").unwrap();
    let added = run(&app, &app.write, &["documents", "add", &app.out("Boiler receipt.pdf"), "--date", "2026-10-01"]);
    assert_eq!(added.code, 0, "{}", added.err);
    assert!(added.out.contains("Boiler receipt.pdf") && !added.out.starts_with('{') && !added.out.contains('\u{1b}'), "{}", added.out);
    let list = run(&app, &app.read, &["documents", "list"]);
    assert_eq!(list.code, 0, "{}", list.err);
    assert!(list.out.contains("Boiler receipt.pdf") && list.out.contains("2026-10-01"), "{}", list.out);
    let help = run(&app, "", &["documents", "add", "--help"]);
    assert!(help.out.contains("usage: meowny documents add <file>") && help.out.contains("--file <path>") && help.out.contains("name of the file"), "{}", help.out);
}
