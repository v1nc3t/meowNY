//! The real `meowny` binary against the app's own server code: a real encrypted vault in a temporary
//! folder, real tokens, a real listener on 127.0.0.1. A command and the route behind it are tested
//! together, from the command line to the database and back.

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

// ---- reading ---------------------------------------------------------------------------------

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
    assert!(tried >= 45, "only {tried} write commands were tried");
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
    // The app stops listening (CLI access turned off, or the app closed).
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
fn control_characters_cannot_get_into_the_data_so_they_cannot_reach_a_terminal_from_it() {
    let app = app(false);
    let food = ok(&app, &app.write, &["categories", "create", "--name", "Food", "--type", "expense"])["id"].as_str().unwrap().to_string();
    let before = snapshot(&app);
    // The app refuses them in names and notes (a note may hold line breaks), so a list can never print an escape sequence it was given.
    for (name, note) in [("\u{1b}[31mred", "x"), ("plain", "a\u{1b}[2Jb"), ("bell\u{7}", "x"), ("tab\there", "x")] {
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
    // A line break in a note is kept in the data and shown as a space in the table.
    ok(&app, &app.write, &["transactions", "create", "--name", "Two", "--amount", "1.00", "--payment-date", "2026-10-15", "--category-id", &food, "--description", "two\nlines"]);
    assert_eq!(ok(&app, &app.read, &["transactions", "list"])["transactions"][0]["description"], "two\nlines");
    assert!(run(&app, &app.read, &["transactions", "list"]).out.contains("two lines"));
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

fn id_of(value: &Value) -> String {
    value["id"].as_str().unwrap().to_string()
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

// ---- accounts and transfers --------------------------------------------------------------------

#[test]
fn a_negative_opening_balance_reaches_the_app_with_or_without_an_equals_sign() {
    let app = app(false);
    let card = ok(&app, &app.write, &["accounts", "create", "--name", "Card", "--kind", "credit_card", "--opening-balance=-120.00"]);
    assert_eq!((card["kind"].clone(), card["openingBalance"].clone(), card["balance"].clone(), card["isMain"].clone()), (json!("CREDIT_CARD"), json!("-120.00"), json!("-120.00"), json!(false)));
    // the word after a flag is its value, so a space works too
    let loan = ok(&app, &app.write, &["accounts", "create", "--name", "Loan", "--kind", "Cash", "--opening-balance", "-0.01"]);
    assert_eq!(loan["openingBalance"], "-0.01");
    // an update by id as a plain word, a number set and then cleared
    let id = id_of(&card);
    let changed = ok(&app, &app.write, &["accounts", "update", &id, "--opening-balance=-35.50", "--number", "4242"]);
    assert_eq!((changed["openingBalance"].clone(), changed["number"].clone(), changed["name"].clone()), (json!("-35.50"), json!("4242"), json!("Card")));
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--clear-number"])["number"], Value::Null);
    // in the table the sign is there
    let plain = run(&app, &app.read, &["accounts", "list"]);
    assert_eq!(plain.code, 0);
    assert!(plain.out.lines().any(|l| l.contains("Card") && l.contains("-35.50")), "{}", plain.out);
    // forms the app refuses are exit 2, and nothing changes
    let before = snapshot(&app);
    for balance in ["-0.00", "+1.00", "1.5", "-1", "10000000000.00", "-10000000000.00", "abc"] {
        assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, &format!("--opening-balance={balance}")]), (2, "invalid_input".into()), "{balance}");
    }
    assert_eq!(snapshot(&app), before);
    // the largest, each way, is fine
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--opening-balance=-9999999999.99"])["openingBalance"], "-9999999999.99");
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--opening-balance", "9999999999.99"])["openingBalance"], "9999999999.99");
}

#[test]
fn deleting_an_account_or_a_transfer_needs_yes_and_without_it_nothing_is_deleted() {
    let app = app(true);
    let spare = id_of(&ok(&app, &app.write, &["accounts", "create", "--name", "Spare", "--kind", "cash"]));
    let moved = id_of(&ok(&app, &app.write, &["transfers", "create", "--from-account-id", "1", "--to-account-id", &spare, "--amount", "200.00", "--date", "2026-11-03"]));
    let before = snapshot(&app);
    assert_eq!(failed(&app, &app.write, &["accounts", "delete", &spare]), (2, "usage".into()));
    assert_eq!(failed(&app, &app.write, &["transfers", "delete", &moved]), (2, "usage".into()));
    let plain = run(&app, &app.write, &["accounts", "delete", &spare]);
    assert!(plain.err.contains("accounts delete cannot be undone; add --yes"), "{}", plain.err);
    assert_eq!(snapshot(&app), before, "without --yes nothing is deleted");
    // with --yes the app still refuses an account that has a transfer in it, and the main account
    assert_eq!(failed(&app, &app.write, &["accounts", "delete", &spare, "--yes"]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "delete", "1", "--yes"]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "archive", "1"]), (2, "invalid_input".into()), "the main account is never archived");
    assert_eq!(snapshot(&app), before);
    // the transfer goes, then the account
    let gone = run(&app, &app.write, &["transfers", "delete", &moved, "--yes"]);
    assert_eq!((gone.code, gone.out.as_str(), gone.err.as_str()), (0, "", "done.\n"));
    assert_eq!(failed(&app, &app.write, &["transfers", "delete", &moved, "--yes"]), (4, "not_found".into()), "deleting twice");
    let gone = run(&app, &app.write, &["accounts", "delete", &spare, "--yes"]);
    assert_eq!((gone.code, gone.out.as_str(), gone.err.as_str()), (0, "", "done.\n"));
    assert_eq!(failed(&app, &app.write, &["accounts", "delete", &spare, "--yes"]), (4, "not_found".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "show", &spare]), (4, "not_found".into()));
}

#[test]
fn an_account_is_read_and_ordered_from_the_command_line_and_a_taken_name_is_exit_1() {
    let app = app(true);
    // a read token reads everything about accounts
    let list = ok(&app, &app.read, &["accounts", "list"]);
    let names: Vec<&str> = list.as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Current account", "Savings", "Cash", "Credit card", "Old savings"]);
    let shown = ok(&app, &app.read, &["accounts", "show", "2", "--from", "2026-09-01", "--to", "2026-09-30"]);
    assert_eq!((shown["account"]["name"].clone(), shown["from"].clone(), shown["to"].clone()), (json!("Savings"), json!("2026-09-01"), json!("2026-09-30")));
    assert!(!shown["lines"].as_array().unwrap().is_empty());
    assert_eq!(ok(&app, &app.read, &["accounts", "history", "2", "--year", "2025"]).as_array().unwrap().len(), 12);
    assert_eq!(ok(&app, &app.read, &["accounts", "summary"])["accounts"], 5);
    assert!(ok(&app, &app.read, &["transfers", "list", "--account-id", "2", "--limit", "3"])["transfers"].as_array().unwrap().len() == 3);
    assert_eq!(failed(&app, &app.read, &["accounts", "show"]), (2, "usage".into()), "the id is needed");
    assert_eq!(failed(&app, &app.read, &["accounts", "show", "999999"]), (4, "not_found".into()));
    assert_eq!(failed(&app, &app.read, &["accounts", "show", "1", "--from", "2026-10-31", "--to", "2026-10-01"]), (2, "invalid_input".into()));
    // the order: every account that is not archived, as a list of ids
    assert_eq!(ok(&app, &app.write, &["accounts", "reorder", "--ids", "2,1,4,3"]), Value::Null);
    let names: Vec<String> = ok(&app, &app.read, &["accounts", "list"]).as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, ["Savings", "Current account", "Credit card", "Cash", "Old savings"]);
    assert_eq!(failed(&app, &app.write, &["accounts", "reorder", "--ids", "2,1,4"]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "reorder", "--ids", "2,1,4,5"]), (2, "invalid_input".into()), "an archived one is not in the order");
    // a name that is taken, in any case, is exit 1 `name_exists`; an archived one frees its name
    assert_eq!(failed(&app, &app.write, &["accounts", "create", "--name", "SAVINGS", "--kind", "cash"]), (1, "name_exists".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "update", "3", "--name", "credit card"]), (1, "name_exists".into()));
    // archive twice and bring back: the same answer each time
    let first = ok(&app, &app.write, &["accounts", "archive", "3"]);
    assert_eq!(ok(&app, &app.write, &["accounts", "archive", "3"]), first);
    assert_eq!(ok(&app, &app.write, &["accounts", "unarchive", "3"])["archived"], false);
    assert_eq!(ok(&app, &app.write, &["accounts", "unarchive", "3"])["archived"], false);
}

#[test]
fn a_transfer_and_a_transaction_name_the_account_from_the_command_line() {
    let app = app(true);
    let moved = ok(&app, &app.write, &["transfers", "create", "--from-account-id", "1", "--to-account-id", "2", "--amount", "200.00", "--date", "2026-11-03", "--note", "Rainy day"]);
    assert_eq!((moved["amount"].clone(), moved["note"].clone(), moved["fromAccountId"].clone()), (json!("200.00"), json!("Rainy day"), json!("1")));
    let id = id_of(&moved);
    let changed = ok(&app, &app.write, &["transfers", "update", &id, "--amount", "250.00", "--clear-note"]);
    assert_eq!((changed["amount"].clone(), changed["note"].clone()), (json!("250.00"), Value::Null));
    assert_eq!(failed(&app, &app.write, &["transfers", "create", "--from-account-id", "2", "--to-account-id", "2", "--amount", "1.00", "--date", "2026-11-03"]), (2, "invalid_input".into()), "to itself");
    assert_eq!(failed(&app, &app.write, &["transfers", "create", "--from-account-id", "1", "--to-account-id", "5", "--amount", "1.00", "--date", "2026-11-03"]), (2, "invalid_input".into()), "into an archived account");
    assert_eq!(failed(&app, &app.write, &["transfers", "create", "--from-account-id", "1", "--to-account-id", "2", "--amount", "0.00", "--date", "2026-11-03"]), (2, "invalid_input".into()));

    let groceries = category(&app, "EXPENSE", "Groceries");
    let bread = ok(&app, &app.write, &["transactions", "create", "--name", "Bread", "--amount", "2.40", "--payment-date", "2026-10-14", "--category-id", &groceries, "--account-id", "3"]);
    assert_eq!(bread["accountId"], "3");
    let in_cash = ok(&app, &app.read, &["transactions", "list", "--account-id", "3", "--q", "Bread"]);
    assert_eq!((in_cash["total"].clone(), in_cash["transactions"][0]["id"].clone()), (json!(1), bread["id"].clone()));
    assert_eq!(ok(&app, &app.write, &["transactions", "update", &id_of(&bread), "--account-id", "1"])["accountId"], "1");
    assert_eq!(failed(&app, &app.write, &["transactions", "create", "--name", "X", "--amount", "1.00", "--payment-date", "2026-10-14", "--category-id", &groceries, "--account-id", "5"]), (2, "invalid_input".into()), "an archived account");
    assert_eq!(failed(&app, &app.write, &["transactions", "list", "--account-id", "zero"]), (2, "invalid_input".into()));
}

#[test]
fn help_for_accounts_and_transfers_lists_their_commands_and_flags() {
    let app = app(false);
    let top = run(&app, "", &["--help"]);
    assert!(top.out.contains("accounts") && top.out.contains("transfers"), "{}", top.out);
    let accounts = run(&app, "", &["accounts", "--help"]);
    assert_eq!((accounts.code, accounts.err.as_str()), (0, ""));
    for verb in ["list", "show", "history", "breakdown", "summary", "create", "update", "archive", "unarchive", "delete", "reorder"] {
        assert!(accounts.out.lines().any(|l| l.trim_start().starts_with(verb)), "accounts {verb}: {}", accounts.out);
    }
    let transfers = run(&app, "", &["transfers", "--help"]);
    for verb in ["list", "create", "update", "delete"] {
        assert!(transfers.out.lines().any(|l| l.trim_start().starts_with(verb)), "transfers {verb}: {}", transfers.out);
    }
    let create = run(&app, "", &["help", "accounts", "create"]);
    assert_eq!(create.code, 0);
    for text in [
        "--name <text> ",
        "--kind <CHECKING|SAVINGS|CASH|CREDIT_CARD>",
        "--opening-balance <text>",
        "--opening-date <date>",
        "--number <text>",
        "--bic <text>",
        "--notes <text>",
        "--group <text>",
        "--include-in-net-worth <boolean>",
        "--virtual-balance <text>",
        "Needs a token that can write.",
    ] {
        assert!(create.out.contains(text), "{text}: {}", create.out);
    }
    assert!(!create.out.contains("--clear-bic"), "a new account has nothing to clear: {}", create.out);
    let update = run(&app, "", &["accounts", "update", "--help"]);
    for text in ["usage: meowny accounts update <id> ", "--clear-number", "--clear-bic", "--clear-notes", "--clear-group", "--include-in-net-worth <boolean>", "--virtual-balance <text>"] {
        assert!(update.out.contains(text), "{text}: {}", update.out);
    }
    assert!(!update.out.contains("--clear-virtual-balance") && !update.out.contains("--clear-include-in-net-worth"), "{}", update.out);
    let breakdown = run(&app, "", &["accounts", "breakdown", "--help"]);
    assert_eq!((breakdown.code, breakdown.err.as_str()), (0, ""));
    for text in ["usage: meowny accounts breakdown <id> [--from <date>] [--to <date>]", "by category", "The first of this month if left out.", "Works with a read-only token."] {
        assert!(breakdown.out.contains(text), "{text}: {}", breakdown.out);
    }
    let list = run(&app, "", &["transactions", "list", "--help"]);
    assert!(list.out.contains("[--kind <INCOME|EXPENSE>]") && list.out.contains("Only INCOME or only EXPENSE."), "{}", list.out);
    let delete = run(&app, "", &["accounts", "delete", "--help"]);
    assert!(delete.out.contains("usage: meowny accounts delete <id>") && delete.out.contains("needs --yes"), "{}", delete.out);
    assert!(run(&app, "", &["transfers", "create", "--help"]).out.contains("--from-account-id <id>"));
    assert!(run(&app, "", &["transactions", "create", "--help"]).out.contains("--account-id <id>"));
}

// ---- the details of an account, the breakdown and the kind of a list --------------------------------

#[test]
fn a_negative_virtual_balance_reaches_the_app_with_or_without_an_equals_sign() {
    let app = app(false);
    let card = ok(&app, &app.write, &["accounts", "create", "--name", "Card", "--kind", "credit_card", "--opening-balance=-120.00", "--opening-date", "2026-01-01", "--virtual-balance=-50.00"]);
    assert_eq!((card["virtualBalance"].clone(), card["balance"].clone(), card["available"].clone()), (json!("-50.00"), json!("-120.00"), json!("-170.00")));
    // the word after a flag is its value, so a space works too
    let other = ok(&app, &app.write, &["accounts", "create", "--name", "Other", "--kind", "Cash", "--virtual-balance", "-50.00"]);
    assert_eq!((other["virtualBalance"].clone(), other["available"].clone()), (json!("-50.00"), json!("-50.00")));
    let id = id_of(&card);
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--virtual-balance=-0.01"])["virtualBalance"], "-0.01");
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--virtual-balance", "-50.00"])["virtualBalance"], "-50.00");
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--virtual-balance", "1000.00"])["available"], "880.00");
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--virtual-balance=-9999999999.99"])["virtualBalance"], "-9999999999.99");
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--virtual-balance=9999999999.99"])["virtualBalance"], "9999999999.99");
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--virtual-balance", "0.00"])["available"], "-120.00", "0.00 is how it goes back to nothing");
    // the sign is in the table and in the account of a statement
    ok(&app, &app.write, &["accounts", "update", &id, "--virtual-balance=-50.00"]);
    let plain = run(&app, &app.read, &["accounts", "list"]);
    assert_eq!(plain.code, 0);
    assert!(plain.out.lines().any(|l| l.contains("Card") && l.contains("-50.00") && l.contains("-170.00")), "{}", plain.out);
    let shown = run(&app, &app.read, &["accounts", "show", &id]);
    assert!(shown.out.contains(r#""virtualBalance":"-50.00""#) && shown.out.contains(r#""available":"-170.00""#), "{}", shown.out);
    // forms the app refuses are exit 2 and nothing changes
    let before = snapshot(&app);
    for amount in ["-0.00", "+1.00", "1.5", "-1", "10000000000.00", "-10000000000.00", "abc", ""] {
        assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, &format!("--virtual-balance={amount}")]), (2, "invalid_input".into()), "{amount:?}");
        assert_eq!(failed(&app, &app.write, &["accounts", "create", "--name", "Bad", "--kind", "cash", &format!("--virtual-balance={amount}")]), (2, "invalid_input".into()), "{amount:?}");
    }
    assert_eq!(snapshot(&app), before);
}

#[test]
fn a_yes_or_no_flag_is_on_when_given_and_off_with_an_equals_sign_and_net_worth_follows() {
    let app = app(false);
    let money = ok(&app, &app.write, &["accounts", "create", "--name", "Hidden", "--kind", "savings", "--opening-balance", "70.00", "--opening-date", "2026-01-01", "--include-in-net-worth=false"]);
    let id = id_of(&money);
    assert_eq!(money["includeInNetWorth"], false);
    assert_eq!(ok(&app, &app.read, &["accounts", "summary"])["inAccounts"], "0.00", "left out of net worth");
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--include-in-net-worth"])["includeInNetWorth"], true, "given alone it is on");
    assert_eq!(ok(&app, &app.read, &["accounts", "summary"])["inAccounts"], "70.00");
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--include-in-net-worth=false"])["includeInNetWorth"], false);
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--include-in-net-worth=true"])["includeInNetWorth"], true);
    assert_eq!(ok(&app, &app.write, &["accounts", "create", "--name", "Counted", "--kind", "cash"])["includeInNetWorth"], true, "on when left out");
    // the main account too
    assert_eq!(ok(&app, &app.write, &["accounts", "update", "1", "--include-in-net-worth=false", "--opening-balance", "5.00"])["includeInNetWorth"], false);
    assert_eq!((ok(&app, &app.read, &["accounts", "summary"])["inAccounts"].clone(), ok(&app, &app.read, &["accounts", "summary"])["accounts"].clone()), (json!("70.00"), json!(3)));
    // a word after the flag is not its value: `false` is then a stray word, and anything but true and false after `=` is refused
    let before = snapshot(&app);
    let out = run(&app, &app.write, &["accounts", "update", &id, "--include-in-net-worth", "false", "--json"]);
    assert_eq!((out.code, out.out.as_str(), out.error()["code"].clone()), (2, "", json!("usage")));
    assert!(out.error()["message"].as_str().unwrap().contains("unexpected argument \"false\""), "{}", out.err);
    for value in ["yes", "1", "0", "no", "False", "TRUE", ""] {
        let out = run(&app, &app.write, &["accounts", "update", &id, &format!("--include-in-net-worth={value}"), "--json"]);
        assert_eq!((out.code, out.error()["code"].clone()), (2, json!("usage")), "{value:?}");
        assert!(out.error()["message"].as_str().unwrap().contains("--include-in-net-worth must be true or false"), "{}", out.err);
    }
    // given twice, or with a clear flag that does not exist for it
    assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, "--include-in-net-worth", "--include-in-net-worth=false"]), (2, "usage".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, "--clear-include-in-net-worth"]), (2, "usage".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, "--clear-virtual-balance"]), (2, "usage".into()));
    assert_eq!(snapshot(&app), before, "a refused command changed the data");
}

#[test]
fn the_details_of_an_account_are_set_changed_and_cleared_from_the_command_line() {
    let app = app(false);
    let card = ok(
        &app,
        &app.write,
        &["accounts", "create", "--name", "Card", "--kind", "credit_card", "--opening-balance=-240.00", "--opening-date", "2026-01-01", "--bic", "DEUTDEFF", "--notes", "Limit raised", "--group", "Bank", "--virtual-balance", "1000.00"],
    );
    assert_eq!(
        (card["bic"].clone(), card["notes"].clone(), card["group"].clone(), card["includeInNetWorth"].clone(), card["virtualBalance"].clone(), card["available"].clone()),
        (json!("DEUTDEFF"), json!("Limit raised"), json!("Bank"), json!(true), json!("1000.00"), json!("760.00"))
    );
    let id = id_of(&card);
    let changed = ok(&app, &app.write, &["accounts", "update", &id, "--bic", "INGBNL2A", "--group", "Cards"]);
    assert_eq!((changed["bic"].clone(), changed["group"].clone(), changed["notes"].clone(), changed["virtualBalance"].clone()), (json!("INGBNL2A"), json!("Cards"), json!("Limit raised"), json!("1000.00")), "only what is named changes");
    // each is cleared on its own, and `--clear-` and a value together are one flag twice
    assert_eq!(ok(&app, &app.write, &["accounts", "update", &id, "--clear-bic"])["bic"], Value::Null);
    let cleared = ok(&app, &app.write, &["accounts", "update", &id, "--clear-notes", "--clear-group"]);
    assert_eq!((cleared["notes"].clone(), cleared["group"].clone(), cleared["virtualBalance"].clone()), (Value::Null, Value::Null, json!("1000.00")));
    assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, "--bic", "X", "--clear-bic"]), (2, "usage".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, "--clear-bic=x"]), (2, "usage".into()));
    // a new account has nothing to clear
    assert_eq!(failed(&app, &app.write, &["accounts", "create", "--name", "X", "--kind", "cash", "--clear-bic"]), (2, "usage".into()));
    // notes with line breaks come from standard input, as every text flag can
    let mut c = command(&app);
    c.env("MEOWNY_TOKEN", &app.write).args(["accounts", "update", &id, "--notes", "-", "--json"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    {
        use std::io::Write;
        child.stdin.take().unwrap().write_all(b"line one\nline two\n").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap()["notes"], "line one\nline two");
    // refused: too long, a control character, an archived account
    let before = snapshot(&app);
    assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, "--bic", &"x".repeat(21)]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, "--group", &"x".repeat(51)]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, "--notes", &"x".repeat(1001)]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, "--bic", "a\tb"]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.write, &["accounts", "create", "--name", "Long", "--kind", "cash", "--group", &"x".repeat(51)]), (2, "invalid_input".into()));
    assert_eq!(snapshot(&app), before);
    ok(&app, &app.write, &["accounts", "archive", &id]);
    assert_eq!(failed(&app, &app.write, &["accounts", "update", &id, "--bic", "NEW"]), (2, "invalid_input".into()), "an archived account is brought back first");
    assert_eq!(ok(&app, &app.write, &["accounts", "unarchive", &id])["notes"], "line one\nline two");
    // a read token cannot set any of them
    for flag in [vec!["--bic", "X"], vec!["--include-in-net-worth=false"], vec!["--virtual-balance", "1.00"], vec!["--clear-group"]] {
        let mut args = vec!["accounts", "update", id.as_str()];
        args.extend(flag.iter().copied());
        assert_eq!(failed(&app, &app.read, &args), (3, "forbidden".into()), "{args:?}");
    }
}

#[test]
fn a_breakdown_is_printed_for_a_person_and_as_json_and_a_read_token_may_ask_for_it() {
    let app = app(false);
    let (food, pay) = (ok(&app, &app.write, &["categories", "create", "--name", "Food", "--type", "expense"]), ok(&app, &app.write, &["categories", "create", "--name", "Pay", "--type", "income"]));
    let (food, pay) = (id_of(&food), id_of(&pay));
    let savings = id_of(&ok(&app, &app.write, &["accounts", "create", "--name", "Savings", "--kind", "savings"]));
    for (name, amount, date, category) in [("Groceries", "30.00", "2026-11-02", &food), ("More", "12.50", "2026-11-20", &food), ("Salary", "500.00", "2026-11-01", &pay), ("Last month", "9.00", "2026-10-30", &food)] {
        ok(&app, &app.write, &["transactions", "create", "--name", name, "--amount", amount, "--payment-date", date, "--category-id", category, "--account-id", &savings]);
    }
    ok(&app, &app.write, &["transfers", "create", "--from-account-id", "1", "--to-account-id", &savings, "--amount", "200.00", "--date", "2026-11-03"]);
    let report = ok(&app, &app.read, &["accounts", "breakdown", &savings, "--from", "2026-11-01", "--to", "2026-11-30"]);
    assert_eq!(
        report,
        json!({
            "from": "2026-11-01", "to": "2026-11-30", "expenseTotal": "42.50", "incomeTotal": "500.00",
            "expenses": [{ "categoryId": food, "name": "Food", "amount": "42.50" }],
            "income": [{ "categoryId": pay, "name": "Pay", "amount": "500.00" }],
        })
    );
    // the id may be a flag
    assert_eq!(ok(&app, &app.read, &["accounts", "breakdown", "--id", &savings, "--from", "2026-11-01", "--to", "2026-11-30"]), report);
    // without days it is this month's, which is the same period as a statement without days
    let (this_month, statement) = (ok(&app, &app.read, &["accounts", "breakdown", &savings]), ok(&app, &app.read, &["accounts", "show", &savings]));
    assert_eq!((&this_month["from"], &this_month["to"]), (&statement["from"], &statement["to"]));
    let empty = ok(&app, &app.read, &["accounts", "breakdown", &savings, "--from", "2026-01-01", "--to", "2026-01-31"]);
    assert_eq!((empty["expenses"].clone(), empty["income"].clone(), empty["expenseTotal"].clone(), empty["incomeTotal"].clone()), (json!([]), json!([]), json!("0.00"), json!("0.00")));
    // a person reads two tables and the totals
    let plain = run(&app, &app.read, &["accounts", "breakdown", &savings, "--from", "2026-11-01", "--to", "2026-11-30"]);
    assert_eq!((plain.code, plain.err.as_str()), (0, ""));
    for text in ["from: 2026-11-01", "to: 2026-11-30", "expenseTotal: 42.50", "incomeTotal: 500.00"] {
        assert!(plain.out.lines().any(|l| l == text), "{text}: {}", plain.out);
    }
    assert!(plain.out.lines().any(|l| l == "expenses") && plain.out.lines().any(|l| l == "income"), "{}", plain.out);
    assert!(plain.out.lines().any(|l| l.contains("Food") && l.contains("42.50")) && plain.out.lines().any(|l| l.contains("Pay") && l.contains("500.00")), "{}", plain.out);
    let none = run(&app, &app.read, &["accounts", "breakdown", &savings, "--from", "2026-01-01", "--to", "2026-01-31"]);
    assert_eq!(none.out.matches("(none)").count(), 2, "{}", none.out);
    // refused the documented way
    let before = snapshot(&app);
    assert_eq!(failed(&app, &app.read, &["accounts", "breakdown"]), (2, "usage".into()), "the id is needed");
    assert_eq!(failed(&app, &app.read, &["accounts", "breakdown", "999999"]), (4, "not_found".into()));
    assert_eq!(failed(&app, &app.read, &["accounts", "breakdown", &savings, "--from", "2026-11-30", "--to", "2026-11-01"]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.read, &["accounts", "breakdown", &savings, "--from", "2026-02-30"]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.read, &["accounts", "breakdown", &savings, "--from", "tomorrow"]), (2, "invalid_input".into()));
    assert_eq!(failed(&app, &app.read, &["accounts", "breakdown", &savings, "--year", "2026"]), (2, "usage".into()), "a flag of another command");
    assert_eq!(snapshot(&app), before);
}

#[test]
fn a_list_of_transactions_takes_the_kind_in_any_case_on_the_command_line() {
    let app = app(true);
    let both = ok(&app, &app.read, &["transactions", "list", "--limit", "1"])["total"].as_u64().unwrap();
    let income = ok(&app, &app.read, &["transactions", "list", "--kind", "INCOME", "--limit", "5"]);
    let expense = ok(&app, &app.read, &["transactions", "list", "--kind", "EXPENSE", "--limit", "5"]);
    assert!(income["total"].as_u64().unwrap() > 0 && expense["total"].as_u64().unwrap() > 0);
    assert_eq!(income["total"].as_u64().unwrap() + expense["total"].as_u64().unwrap(), both);
    // the command line matches the two words without regard to case, as the type of a category
    for kind in ["income", "Income", "INCOME", "iNcOmE"] {
        assert_eq!(ok(&app, &app.read, &["transactions", "list", "--kind", kind, "--limit", "5"]), income, "{kind}");
        assert_eq!(ok(&app, &app.read, &["transactions", "list", &format!("--kind={kind}"), "--limit", "5"]), income, "{kind}");
    }
    for kind in ["expense", "Expense", "EXPENSE"] {
        assert_eq!(ok(&app, &app.read, &["transactions", "list", "--kind", kind, "--limit", "5"]), expense, "{kind}");
    }
    // with the other filters
    let in_cash = ok(&app, &app.read, &["transactions", "list", "--kind", "income", "--account-id", "1", "--from", "2026-09-01", "--to", "2026-09-30", "--limit", "100"]);
    assert!(in_cash["transactions"].as_array().unwrap().iter().all(|t| t["accountId"] == "1" && t["paymentDate"].as_str().unwrap().starts_with("2026-09")));
    // a person sees a table
    let plain = run(&app, &app.read, &["transactions", "list", "--kind", "income", "--limit", "3"]);
    assert_eq!(plain.code, 0);
    assert_eq!(plain.out.lines().filter(|l| l.contains("2026-")).count(), 3, "a row for each of the three, newest first: {}", plain.out);
    // any other word is a usage error from the command line, and nothing reaches the app (over HTTP the app itself wants the capitals: `api/accounts_tests.rs`)
    let before = snapshot(&app);
    for kind in ["nonsense", "both", "income,expense", "incomes", ""] {
        let out = run(&app, &app.read, &["transactions", "list", "--kind", kind, "--json"]);
        assert_eq!((out.code, out.out.as_str(), out.error()["code"].clone()), (2, "", json!("usage")), "{kind:?}");
        assert_eq!(out.error()["message"], "--kind must be one of: INCOME, EXPENSE", "{kind:?}");
    }
    assert_eq!(failed(&app, &app.read, &["transactions", "list", "--kind"]), (2, "usage".into()));
    assert_eq!(snapshot(&app), before);
}
