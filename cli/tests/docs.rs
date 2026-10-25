//! The documentation's facts come from the program, not from memory. This test runs one scripted tour of
//! every command, and every way a command fails, against the real app and records what really came
//! out. The website shows those outputs.
//!
//!   cd cli && UPDATE_DOCS=1 cargo test --offline --test docs      # rewrite docs/site/commands.json and docs/site/examples.json
//!   node docs/site/build.mjs                                            # rebuild the site
//!
//! Without `UPDATE_DOCS` the same tour runs and is compared with the recorded files, so a command that
//! changes, a message that is reworded or a command with no example fails here until the docs are redone.

// Recorded on Linux, where the paths in the examples are Linux paths (`~/.config/meowny/token`). On another system the
// same commands print other paths, so comparing there would only fail for a reason that is not a mistake.
#![cfg(target_os = "linux")]

use std::collections::BTreeSet;
use std::io::Write;
use std::process::Stdio;

use serde_json::{json, Map, Value};

mod common;
use common::*;

const UUID: &str = "9b2f5d44-6e1a-4c3b-8f2d-1a2b3c4d5e6f";
const DINNER_UUID: &str = "5c1d8e07-3b2a-4f6e-9a41-7d0c2b9e8f13";
const PASSPHRASE: &str = "correct horse battery staple";
/// Where the files of the tour are said to be in the docs.
const HOME: &str = "/home/you";

/// Outputs that legitimately differ from run to run (the clock, file sizes, timestamps inside files). They are
/// compared by exit code, by the shape of their JSON and by the frame of their plain output, not word for word.
/// "export preview" counts the payments that are due, and more fall due as the days go by, as with "debts due".
const MOVES: &[&str] = &["today", "recurring dues", "debts due", "debts history", "debts reminders", "budgets history", "recurring end", "documents add", "documents list", "documents show", "documents update", "export debts", "export preview", "export json", "export csv", "export xlsx", "export pdf", "backup create", "schema"];

#[derive(Clone, Copy)]
enum Who {
    Read,
    Write,
    Nobody,
}

struct Tour<'a> {
    app: &'a App,
    examples: Map<String, Value>,
    errors: Vec<Value>,
}

impl Tour<'_> {
    /// What the docs show: the temporary folders are said to be somewhere a person would have them.
    fn fix(&self, text: &str) -> String {
        let (files, data) = (self.app.files.path().to_str().unwrap(), self.app.dir.path().to_str().unwrap());
        text.replace(files, HOME).replace(data, &format!("{HOME}/.local/share/org.meowny.app"))
    }

    fn capture(&self, who: Who, args: &[String], stdin: Option<&str>, env: &[(&str, String)]) -> Value {
        let mut c = command(self.app);
        match who {
            Who::Read => c.env("MEOWNY_TOKEN", &self.app.read),
            Who::Write => c.env("MEOWNY_TOKEN", &self.app.write),
            Who::Nobody => &mut c,
        };
        for (key, value) in env {
            c.env(key, value);
        }
        c.args(args).stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c.spawn().unwrap();
        if let Some(text) = stdin {
            child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
        }
        let out = child.wait_with_output().unwrap();
        let (mut stdout, stderr) = (self.fix(&String::from_utf8_lossy(&out.stdout)), self.fix(&String::from_utf8_lossy(&out.stderr)));
        let mut entry = Map::new();
        // A long answer (the schema) is shown cut; the docs say so.
        if stdout.chars().count() > 700 {
            stdout = stdout.chars().take(700).collect::<String>() + "…";
            entry.insert("truncated".into(), json!(true));
        }
        entry.insert("exit".into(), json!(out.status.code().unwrap_or(-1)));
        entry.insert("stdout".into(), json!(stdout));
        entry.insert("stderr".into(), json!(stderr));
        Value::Object(entry)
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    /// Runs a command for the docs. `human` and `json` say which of its two outputs to show. Returns the data
    /// the command printed as JSON, if it was run that way, so later steps can use the ids it made.
    #[allow(clippy::too_many_arguments)] // one call per documented example, each part is a separate thing to show
    fn example(&mut self, name: &str, who: Who, shown: &str, args: &[&str], human: bool, json_too: bool, stdin: Option<&str>) -> Value {
        let mut entry = Map::new();
        entry.insert("command".into(), json!(shown));
        let env = [("XDG_CONFIG_HOME", format!("{}/.config", self.app.files.path().display()))];
        let mut data = Value::Null;
        if human {
            let got = self.capture(who, &Self::strings(args), stdin, &env);
            assert_eq!(got["exit"], 0, "{name}: {}", got["stderr"]);
            entry.insert("human".into(), got);
        }
        if json_too {
            let mut with = Self::strings(args);
            with.push("--json".into());
            let got = self.capture(who, &with, stdin, &env);
            assert_eq!(got["exit"], 0, "{name} --json: {}", got["stderr"]);
            data = serde_json::from_str(got["stdout"].as_str().unwrap()).unwrap_or(Value::Null);
            entry.insert("json".into(), got);
        }
        assert!(self.examples.insert(name.into(), Value::Object(entry)).is_none(), "{name} twice");
        data
    }

    fn id(data: &Value) -> String {
        data["id"].as_str().unwrap_or_else(|| panic!("no id in {data}")).to_string()
    }

    fn error(&mut self, title: &str, shown: &str, who: Who, args: &[&str], env: &[(&str, String)]) {
        let got = self.capture(who, &Self::strings(args), None, env);
        assert_ne!(got["exit"], 0, "{title} did not fail");
        self.errors.push(json!({ "title": title, "command": shown, "exit": got["exit"], "stderr": got["stderr"] }));
    }
}

/// Every command, in an order that gives each one something to work on, then every kind of failure.
fn tour(app: &App) -> (Map<String, Value>, Vec<Value>) {
    let mut t = Tour { app, examples: Map::new(), errors: Vec::new() };
    let (read, write) = (Who::Read, Who::Write);
    let files = app.files.path().to_str().unwrap().to_string();
    let path = |name: &str| format!("{files}/{name}");

    // Looking.
    t.example("status", read, "meowny status", &["status"], true, true, None);
    t.example("today", read, "meowny today", &["today"], true, true, None);
    t.example("settings get", read, "meowny settings get", &["settings", "get"], true, true, None);
    t.example("categories list", read, "meowny categories list --type expense", &["categories", "list", "--type", "expense"], true, true, None);
    t.example("transactions list", read, "meowny transactions list --limit 4", &["transactions", "list", "--limit", "4"], true, true, None);
    t.example("budgets overview", read, "meowny budgets overview --month 2026-09", &["budgets", "overview", "--month", "2026-09"], true, true, None);
    t.example("budgets history", read, "meowny budgets history", &["budgets", "history"], true, true, None);
    t.example("recurring list", read, "meowny recurring list", &["recurring", "list"], true, true, None);
    t.example("recurring dues", read, "meowny recurring dues --within-days 10", &["recurring", "dues", "--within-days", "10"], true, true, None);
    t.example("stats budget", read, "meowny stats budget --month 2026-09", &["stats", "budget", "--month", "2026-09"], true, true, None);
    t.example("stats trend", read, "meowny stats trend --category-id 5 --from 2026-07 --to 2026-09", &["stats", "trend", "--category-id", "5", "--from", "2026-07", "--to", "2026-09"], true, true, None);
    t.example("stats year", read, "meowny stats year --year 2026", &["stats", "year", "--year", "2026"], true, true, None);
    t.example("stats top", read, "meowny stats top --month 2026-09 --type expense --limit 3", &["stats", "top", "--month", "2026-09", "--type", "expense", "--limit", "3"], true, true, None);
    t.example("stats commitments", read, "meowny stats commitments", &["stats", "commitments"], true, true, None);
    t.example("stats planned", read, "meowny stats planned --month 2026-09", &["stats", "planned", "--month", "2026-09"], true, true, None);
    t.example("export preview", read, "meowny export preview --from 2026-01-01 --to 2026-09-30", &["export", "preview", "--from", "2026-01-01", "--to", "2026-09-30"], true, true, None);
    t.example("backup schedule", read, "meowny backup schedule", &["backup", "schedule"], true, true, None);

    // Changing: a category, a transaction, a budget.
    t.example("settings set", write, "meowny settings set --language ro", &["settings", "set", "--language", "ro"], false, true, None);
    t.capture(write, &Tour::strings(&["settings", "set", "--language", "en"]), None, &[]); // and back, for what follows
    let pets = Tour::id(&t.example("categories create", write, "meowny categories create --name Pets --type expense", &["categories", "create", "--name", "Pets", "--type", "expense"], false, true, None));
    t.example("categories rename", write, &format!("meowny categories rename {pets} --name Animals"), &["categories", "rename", &pets, "--name", "Animals"], false, true, None);
    let bread = Tour::id(&t.example(
        "transactions create",
        write,
        &format!("meowny transactions create --name Bread --amount 2.40 --payment-date 2026-10-14 --category-id {pets} --client-uuid {UUID}"),
        &["transactions", "create", "--name", "Bread", "--amount", "2.40", "--payment-date", "2026-10-14", "--category-id", &pets, "--client-uuid", UUID],
        false,
        true,
        None,
    ));
    t.example("transactions update", write, &format!("meowny transactions update {bread} --amount 2.95 --description Sourdough"), &["transactions", "update", &bread, "--amount", "2.95", "--description", "Sourdough"], false, true, None);
    t.example("budgets set", write, &format!("meowny budgets set --type expense --month 2026-12 --amount-limit 100.00 --category-id {pets}"), &["budgets", "set", "--type", "expense", "--month", "2026-12", "--amount-limit", "100.00", "--category-id", &pets], false, true, None);
    t.example("budgets remove", write, &format!("meowny budgets remove --type expense --month 2027-01 --category-id {pets} --yes"), &["budgets", "remove", "--type", "expense", "--month", "2027-01", "--category-id", &pets, "--yes"], false, true, None);

    // Friends, shared bills and the ledger: money that is owed, in both directions. A loan is never income or an expense.
    let json_id = |got: Value| serde_json::from_str::<Value>(got["stdout"].as_str().unwrap()).unwrap()["id"].as_str().unwrap().to_string();
    let dinner = json_id(t.capture(write, &Tour::strings(&["transactions", "create", "--name", "Dinner", "--amount", "90.00", "--payment-date", "2026-10-12", "--category-id", &pets, "--client-uuid", DINNER_UUID, "--json"]), None, &[]));
    let anna = Tour::id(&t.example("people create", write, "meowny people create --name Anna", &["people", "create", "--name", "Anna"], false, true, None));
    let bob = json_id(t.capture(write, &Tour::strings(&["people", "create", "--name", "Bob", "--json"]), None, &[]));
    t.example(
        "people update",
        write,
        &format!("meowny people update {bob} --remind-on 2026-11-01 --remind-every-days 7"),
        &["people", "update", &bob, "--remind-on", "2026-11-01", "--remind-every-days", "7"],
        false,
        true,
        None,
    );
    let friends = format!("{anna},{bob}");
    t.example(
        "splits preview",
        read,
        &format!("meowny splits preview --amount 90.00 --mode percent --people {friends} --values 30.00,30.00"),
        &["splits", "preview", "--amount", "90.00", "--mode", "percent", "--people", &friends, "--values", "30.00,30.00"],
        true,
        true,
        None,
    );
    t.example(
        "transactions split",
        write,
        &format!("meowny transactions split {dinner} --mode equal --people {friends}"),
        &["transactions", "split", &dinner, "--mode", "equal", "--people", &friends],
        false,
        true,
        None,
    );
    let loan = Tour::id(&t.example(
        "ledger add",
        write,
        &format!("meowny ledger add --person-id {anna} --kind lent --amount 10.00 --date 2026-10-13 --note taxi"),
        &["ledger", "add", "--person-id", &anna, "--kind", "lent", "--amount", "10.00", "--date", "2026-10-13", "--note", "taxi"],
        false,
        true,
        None,
    ));
    t.example("ledger update", write, &format!("meowny ledger update {loan} --amount 12.00"), &["ledger", "update", &loan, "--amount", "12.00"], false, true, None);
    t.example("people list", read, "meowny people list", &["people", "list"], true, true, None);
    t.example("people show", read, &format!("meowny people show {anna}"), &["people", "show", &anna], true, true, None);
    t.example("debts summary", read, "meowny debts summary", &["debts", "summary"], true, true, None);
    t.example("debts due", read, "meowny debts due", &["debts", "due"], true, true, None);
    t.example("debts history", read, "meowny debts history --limit 5", &["debts", "history", "--limit", "5"], true, true, None);
    t.example("debts reminders", read, "meowny debts reminders --every-days 7", &["debts", "reminders", "--every-days", "7"], true, true, None);
    t.example(
        "people settle",
        write,
        &format!("meowny people settle {anna} --amount 50.00 --date 2026-10-15"),
        &["people", "settle", &anna, "--amount", "50.00", "--date", "2026-10-15"],
        false,
        true,
        None,
    );

    // Recurring items.
    let plan = Tour::id(&t.example(
        "recurring create",
        write,
        &format!("meowny recurring create --name \"Vet plan\" --amount 20.00 --category-id {pets} --start-date 2026-10-20 --interval-unit month --interval-count 1"),
        &["recurring", "create", "--name", "Vet plan", "--amount", "20.00", "--category-id", &pets, "--start-date", "2026-10-20", "--interval-unit", "month", "--interval-count", "1"],
        false,
        true,
        None,
    ));
    t.example("recurring update", write, &format!("meowny recurring update {plan} --name \"Vet plan plus\" --clear-end-date"), &["recurring", "update", &plan, "--name", "Vet plan plus", "--clear-end-date"], false, true, None);
    t.example(
        "recurring split",
        write,
        &format!("meowny recurring split {plan} --mode shares --mine 2 --people {anna} --values 1"),
        &["recurring", "split", &plan, "--mode", "shares", "--mine", "2", "--people", &anna, "--values", "1"],
        false,
        true,
        None,
    );
    t.example("recurring unsplit", write, &format!("meowny recurring unsplit {plan}"), &["recurring", "unsplit", &plan], false, true, None);
    t.example("recurring pay", write, &format!("meowny recurring pay {plan} --paid-on 2026-10-14"), &["recurring", "pay", &plan, "--paid-on", "2026-10-14"], false, true, None);
    t.example("recurring skip", write, &format!("meowny recurring skip {plan}"), &["recurring", "skip", &plan], false, true, None);
    t.example("recurring pause", write, &format!("meowny recurring pause {plan}"), &["recurring", "pause", &plan], false, true, None);
    t.example("recurring resume", write, &format!("meowny recurring resume {plan}"), &["recurring", "resume", &plan], false, true, None);
    t.example("recurring end", write, &format!("meowny recurring end {plan} --yes"), &["recurring", "end", &plan, "--yes"], false, true, None);
    t.example("recurring delete", write, &format!("meowny recurring delete {plan} --yes"), &["recurring", "delete", &plan, "--yes"], true, false, None);

    // Documents: the program reads the file and sends it; a read-only token can look; the person allows a copy out.
    std::fs::write(path("boiler-receipt.pdf"), b"%PDF-1.4 the boiler was repaired, 180.00 paid").unwrap();
    let added = t.example(
        "documents add",
        write,
        "meowny documents add /home/you/boiler-receipt.pdf --date 2026-10-01",
        &["documents", "add", &path("boiler-receipt.pdf"), "--date", "2026-10-01"],
        false,
        true,
        None,
    );
    let receipt = Tour::id(&added);
    t.example("documents list", read, "meowny documents list --q boiler", &["documents", "list", "--q", "boiler"], true, true, None);
    t.example("documents show", read, &format!("meowny documents show {receipt}"), &["documents", "show", &receipt], true, true, None);
    t.example("documents update", write, &format!("meowny documents update {receipt} --name \"Boiler repair\" --keep-on-phone"), &["documents", "update", &receipt, "--name", "Boiler repair", "--keep-on-phone"], false, true, None);
    t.example("documents link", write, &format!("meowny documents link {receipt} --transaction-id {bread}"), &["documents", "link", &receipt, "--transaction-id", &bread], true, false, None);
    t.example("documents unlink", write, &format!("meowny documents unlink {receipt} --transaction-id {bread}"), &["documents", "unlink", &receipt, "--transaction-id", &bread], true, false, None);
    t.example(
        "documents get",
        write,
        &format!("meowny documents get {receipt} --out /home/you/boiler-receipt-copy.pdf"),
        &["documents", "get", &receipt, "--out", &path("boiler-receipt-copy.pdf")],
        true,
        false,
        None,
    );
    t.example("documents delete", write, &format!("meowny documents delete {receipt} --yes"), &["documents", "delete", &receipt, "--yes"], true, false, None);

    // Files, which the person allows in the app.
    t.example("export debts", read, "meowny export debts --out /home/you/who-owes-what.txt", &["export", "debts", "--out", &path("who-owes-what.txt")], true, false, None);
    t.example("export json", read, "meowny export json --out /home/you/debts.json --sections people,due,ledger", &["export", "json", "--out", &path("debts.json"), "--sections", "people,due,ledger"], true, false, None);
    t.example(
        "export csv",
        read,
        "meowny export csv --out /home/you/budget-2026.csv --from 2026-01-01 --kind expense --min-amount 5.00",
        &["export", "csv", "--out", &path("budget-2026.csv"), "--from", "2026-01-01", "--kind", "expense", "--min-amount", "5.00"],
        true,
        false,
        None,
    );
    t.example("export xlsx", read, "meowny export xlsx --out /home/you/budget.xlsx", &["export", "xlsx", "--out", &path("budget.xlsx")], true, false, None);
    t.example(
        "export pdf",
        read,
        "meowny export pdf --out /home/you/report.pdf --from-month 2026-01 --to-month 2026-03 --include-transactions --include-debts",
        &["export", "pdf", "--out", &path("report.pdf"), "--from-month", "2026-01", "--to-month", "2026-03", "--include-transactions", "--include-debts"],
        true,
        false,
        None,
    );
    t.example(
        "backup create",
        read,
        &format!("echo \"{PASSPHRASE}\" | meowny backup create --out /home/you/meowny.meowbak --passphrase -"),
        &["backup", "create", "--out", &path("meowny.meowbak"), "--passphrase", "-"],
        true,
        false,
        Some(&format!("{PASSPHRASE}\n")),
    );

    // Clearing up what the tour made.
    t.example("transactions unsplit", write, &format!("meowny transactions unsplit {dinner}"), &["transactions", "unsplit", &dinner], false, true, None);
    t.example("people archive", write, &format!("meowny people archive {bob}"), &["people", "archive", &bob], false, true, None);
    t.example("people unarchive", write, &format!("meowny people unarchive {bob}"), &["people", "unarchive", &bob], false, true, None);
    t.example("ledger delete", write, &format!("meowny ledger delete {loan} --yes"), &["ledger", "delete", &loan, "--yes"], true, false, None);
    t.example("transactions delete", write, &format!("meowny transactions delete {bread} --yes"), &["transactions", "delete", &bread, "--yes"], true, false, None);
    let listed = t.capture(read, &Tour::strings(&["categories", "list", "--type", "expense", "--json"]), None, &[]);
    let listed: Value = serde_json::from_str(listed["stdout"].as_str().unwrap()).unwrap();
    let order: Vec<&str> = listed.as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).filter(|id| *id != pets).collect();
    let ids = format!("{pets},{}", order.join(","));
    t.example("categories reorder", write, &format!("meowny categories reorder --ids {ids}"), &["categories", "reorder", "--ids", &ids], false, true, None);
    t.example("categories delete", write, &format!("meowny categories delete {pets} --yes"), &["categories", "delete", &pets, "--yes"], true, false, None);

    // About this program.
    t.example("schema", read, "meowny schema", &["schema"], true, false, None);
    let saved = app.read.clone();
    t.example("login", read, "echo \"$MEOWNY_TOKEN\" | meowny login", &["login"], true, false, Some(&format!("{saved}\n")));
    t.example("logout", read, "meowny logout", &["logout"], true, false, None);

    // Going wrong.
    let none = [("XDG_CONFIG_HOME", format!("{files}/nothing"))];
    t.error("There is no token", "meowny status", Who::Nobody, &["status"], &none);
    t.error("The token is not the right form", "MEOWNY_TOKEN=hello meowny status", Who::Nobody, &["status"], &[("MEOWNY_TOKEN", "hello".into())]);
    t.error("The app does not know the token", "meowny status", Who::Nobody, &["status"], &[("MEOWNY_TOKEN", format!("mny_{}", "0".repeat(64)))]);
    t.error("A read-only token tries to change something", "meowny categories create --name Pets --type expense", read, &["categories", "create", "--name", "Pets", "--type", "expense"], &[]);
    t.error("A value the app refuses", "meowny transactions create --name Bread --amount 2.4 --payment-date 2026-10-14 --category-id 5", write, &["transactions", "create", "--name", "Bread", "--amount", "2.4", "--payment-date", "2026-10-14", "--category-id", "5"], &[]);
    t.error("A missing flag", "meowny transactions create --name Bread", write, &["transactions", "create", "--name", "Bread"], &[]);
    t.error("Deleting without --yes", "meowny transactions delete 5", write, &["transactions", "delete", "5"], &[]);
    t.error("Something that is not there", "meowny transactions delete 999999 --yes", write, &["transactions", "delete", "999999", "--yes"], &[]);
    t.error("A file the program cannot read", "meowny documents add /home/you/missing.pdf", write, &["documents", "add", &path("missing.pdf")], &[]);
    let lease = document(app, "Lease.pdf", b"%PDF-1.4 the lease", None);
    t.error("A name for a copy that a system could run", &format!("meowny documents get {lease} --out /home/you/copy.sh"), write, &["documents", "get", &lease, "--out", &path("copy.sh")], &[]);
    t.error("A name that is taken", "meowny categories create --name Groceries --type expense", write, &["categories", "create", "--name", "Groceries", "--type", "expense"], &[]);
    t.error("A command that does not exist", "meowny backup restore", write, &["backup", "restore"], &[]);
    app.vault.lock();
    t.error("The app is locked", "meowny categories list", read, &["categories", "list"], &[]);
    app.vault.unlock(None).unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    t.error("The app is not running, or CLI access is off", "meowny status", read, &["status"], &[("MEOWNY_DATA_DIR", elsewhere.path().to_str().unwrap().to_string())]);
    std::fs::write(path("taken.csv"), "mine").unwrap();
    t.error("A file that already exists", "meowny export csv --out /home/you/taken.csv", read, &["export", "csv", "--out", &path("taken.csv")], &[]);
    t.error("A file name that is not an export", "meowny export json --out /home/you/data.txt", read, &["export", "json", "--out", &path("data.txt")], &[]);
    *app.answer.lock().unwrap() = false;
    t.error("You click Deny in the app", "meowny export json --out /home/you/second.json", read, &["export", "json", "--out", &path("second.json")], &[]);
    t.error("Asking again at once after a Deny", "meowny export json --out /home/you/second.json", read, &["export", "json", "--out", &path("second.json")], &[]);
    *app.answer.lock().unwrap() = true;

    (t.examples, t.errors)
}

fn shape(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), shape(v))).collect()),
        Value::Array(items) => Value::Array(items.first().map(shape).into_iter().collect()),
        Value::Null => json!("null"),
        Value::Bool(_) => json!("boolean"),
        Value::Number(_) => json!("number"),
        Value::String(_) => json!("string"),
    }
}

/// The frame of a plain output without its values: the names of a record's lines (`bytes`, `path`), or the header
/// line of a table.
fn skeleton(text: &str) -> Vec<String> {
    let first = text.lines().next().unwrap_or("");
    if !first.contains(": ") {
        return vec![first.to_string()];
    }
    text.lines().map_while(|l| l.split_once(": ").map(|(key, _)| key.to_string())).collect()
}

/// What differs between a recorded output and a fresh one. Output that legitimately moves is compared loosely.
fn differences(name: &str, old: &Value, new: &Value, loose: bool, found: &mut Vec<String>) {
    if old["exit"] != new["exit"] {
        found.push(format!("{name}: exit code was {}, is {}", old["exit"], new["exit"]));
    }
    let (old_out, new_out) = (old["stdout"].as_str().unwrap_or(""), new["stdout"].as_str().unwrap_or(""));
    if loose {
        let parsed = |t: &str| serde_json::from_str::<Value>(t).ok().map(|v| shape(&v));
        let moved = match (parsed(old_out), parsed(new_out)) {
            (Some(a), Some(b)) => a != b,
            (None, None) => skeleton(old_out) != skeleton(new_out) && old["truncated"] != json!(true),
            _ => old["truncated"] != json!(true),
        };
        if moved {
            found.push(format!("{name}: the shape of the output changed"));
        }
    } else {
        if old_out != new_out {
            found.push(format!("{name}: stdout changed\n  was: {old_out:?}\n  now: {new_out:?}"));
        }
        if old["stderr"] != new["stderr"] {
            found.push(format!("{name}: stderr changed\n  was: {}\n  now: {}", old["stderr"], new["stderr"]));
        }
    }
}

fn docs_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/site")
}

fn read_json(name: &str) -> Value {
    let path = docs_dir().join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{} is missing; run: cd cli && UPDATE_DOCS=1 cargo test --offline --test docs", path.display()))).unwrap()
}

#[test]
fn the_documentation_matches_the_program() {
    let app = app(true);
    let commands_now = ok_text(&app, &["schema"]);
    let (examples, errors) = tour(&app);

    // Nothing secret can end up on a web page.
    let everything = serde_json::to_string(&json!({ "examples": examples, "errors": errors })).unwrap();
    for secret in [&app.read, &app.write] {
        assert!(!everything.contains(secret.as_str()) && !everything.contains(&secret[4..]), "a token is in the documentation");
    }
    assert!(!everything.contains("a long enough passphrase"));

    // Every command has an example, and every example is a command.
    let names: BTreeSet<String> = commands_now["commands"].as_array().unwrap().iter().map(|c| c["command"].as_str().unwrap().to_string()).collect();
    let documented: BTreeSet<String> = examples.keys().cloned().collect();
    assert_eq!(names.difference(&documented).collect::<Vec<_>>(), Vec::<&String>::new(), "commands with no example in the tour above");
    assert_eq!(documented.difference(&names).collect::<Vec<_>>(), Vec::<&String>::new(), "examples of commands that do not exist");
    // The errors cover every exit code a script can see.
    let exits: BTreeSet<i64> = errors.iter().map(|e| e["exit"].as_i64().unwrap()).collect();
    assert_eq!(exits, BTreeSet::from([1, 2, 3, 4]), "an example of each way to fail");

    let new = json!({ "examples": examples, "errors": errors });
    if std::env::var_os("UPDATE_DOCS").is_some() {
        std::fs::write(docs_dir().join("commands.json"), serde_json::to_string_pretty(&commands_now).unwrap() + "\n").unwrap();
        std::fs::write(docs_dir().join("examples.json"), serde_json::to_string_pretty(&new).unwrap() + "\n").unwrap();
        println!("docs/site/commands.json and docs/site/examples.json were rewritten; now run: node docs/site/build.mjs");
        return;
    }

    let mut found = Vec::new();
    if read_json("commands.json") != commands_now {
        found.push("docs/site/commands.json is not what `meowny schema` prints".to_string());
    }
    let recorded = read_json("examples.json");
    for (name, entry) in &examples {
        let old = &recorded["examples"][name.as_str()];
        if old.is_null() {
            found.push(format!("{name}: no recorded example"));
            continue;
        }
        let loose = MOVES.contains(&name.as_str());
        for kind in ["human", "json"] {
            match (old.get(kind), entry.get(kind)) {
                (Some(a), Some(b)) => differences(&format!("{name} ({kind})"), a, b, loose, &mut found),
                (None, None) => {}
                _ => found.push(format!("{name}: the recorded example shows a different set of outputs")),
            }
        }
        if old["command"] != entry["command"] {
            found.push(format!("{name}: the command line shown changed"));
        }
    }
    let old_errors = recorded["errors"].as_array().unwrap();
    if old_errors.len() != errors.len() {
        found.push(format!("the number of error examples changed: {} to {}", old_errors.len(), errors.len()));
    }
    for (old, new) in old_errors.iter().zip(&errors) {
        differences(old["title"].as_str().unwrap_or("?"), old, new, false, &mut found);
        if old["title"] != new["title"] || old["command"] != new["command"] {
            found.push(format!("the error example {} changed", old["title"]));
        }
    }
    assert!(
        found.is_empty(),
        "the documentation is out of date:\n{}\n\nrun: cd cli && UPDATE_DOCS=1 cargo test --offline --test docs && node ../docs/site/build.mjs",
        found.join("\n")
    );
}

/// The JSON of a command that prints JSON, for a test that has no token to give.
fn ok_text(app: &App, args: &[&str]) -> Value {
    let mut c = command(app);
    c.args(args);
    finish(c).json()
}
