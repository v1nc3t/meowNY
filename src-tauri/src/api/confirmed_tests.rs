//! The routes that write files, over real sockets, with a stand-in for the dialog. What matters:
//! nothing is written unless the person says yes, a request that could never work does not ask,
//! a file that exists is never replaced, and what the person is shown is true.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use super::prompt::{Approval, Prompter, Show};
use super::routes;
use super::tests::{call, make_token, new_vault, Reply};
use super::*;
use crate::tokens::Scope;
use crate::backup;

const PASSPHRASE: &str = "a long enough passphrase";

/// The dialog. Records what it was shown, waits, answers as told, and can lock the app while it is "open".
struct Dialog {
    shown: Arc<Mutex<Vec<Approval>>>,
    answer: Arc<Mutex<bool>>,
    delay: Duration,
    lock: Option<Arc<Vault>>,
}

impl Show for Dialog {
    fn show(&self, approval: &Approval) -> bool {
        self.shown.lock().unwrap().push(approval.clone());
        thread::sleep(self.delay);
        if let Some(vault) = &self.lock {
            vault.lock();
        }
        *self.answer.lock().unwrap()
    }
}

pub(super) struct Rig {
    _app: tempfile::TempDir,
    pub(super) files: tempfile::TempDir,
    pub(super) vault: Arc<Vault>,
    pub(super) server: ApiServer,
    pub(super) read: String,
    pub(super) write: String,
    pub(super) shown: Arc<Mutex<Vec<Approval>>>,
}

/// An app with the demo data, and a person who answers `answer` after `delay_ms`.
pub(super) fn rig_with(answer: bool, delay_ms: u64, wait_ms: u64, lock_when_open: bool, prompt: bool) -> Rig {
    let app = tempfile::tempdir().unwrap();
    let vault = new_vault(app.path());
    vault.with_conn(|c| crate::seed::run_for(c, "2026-10-14").map_err(|_| AppError::Internal)).unwrap();
    let (read, write) = (make_token(&vault, Scope::Read), make_token(&vault, Scope::Write));
    let (shown, answer) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(answer)));
    let dialog = Dialog { shown: shown.clone(), answer: answer.clone(), delay: Duration::from_millis(delay_ms), lock: lock_when_open.then(|| vault.clone()) };
    let prompter = prompt.then(|| Prompter::with_limits(dialog, Duration::from_millis(wait_ms), Duration::from_millis(300)));
    let server = ApiServer::start_with(vault.clone(), routes::ALL, Limits::default(), prompter).unwrap();
    Rig { _app: app, files: tempfile::tempdir().unwrap(), vault, server, read, write, shown }
}

pub(super) fn rig(answer: bool) -> Rig {
    rig_with(answer, 0, 3000, false, true)
}

impl Rig {
    pub(super) fn path(&self, name: &str) -> String {
        self.files.path().join(name).to_str().unwrap().to_string()
    }

    pub(super) fn post(&self, route: &str, body: Value) -> Reply {
        call(self.server.port(), Some(&self.write), route, &body.to_string())
    }

    pub(super) fn asked(&self) -> usize {
        self.shown.lock().unwrap().len()
    }

    pub(super) fn last(&self) -> String {
        self.shown.lock().unwrap().last().expect("the person was asked nothing").text.clone()
    }

    pub(super) fn written(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.files.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        names
    }
}

pub(super) fn denied(reply: &Reply) -> String {
    assert_eq!((reply.status, reply.code().as_str()), (403, "denied"), "{}", reply.body);
    reply.message()
}

// ---- which routes need a yes ------------------------------------------------------------------

#[test]
fn exactly_the_routes_that_write_files_ask_the_person() {
    let asking: BTreeSet<&str> = routes::ALL.iter().filter(|r| r.confirm.is_some()).map(|r| r.name).collect();
    assert_eq!(asking, BTreeSet::from(["export.json", "export.csv", "export.xlsx", "export.pdf", "export.debts", "backup.create", "documents.get"]));
    // And the schema says so, so a client can tell the person to look at the app.
    let r = rig(true);
    let schema = call(r.server.port(), Some(&r.read), "schema", "").data();
    let flagged: BTreeSet<&str> = schema["routes"].as_array().unwrap().iter().filter(|x| x["confirm"] == true).map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(flagged, asking);
    // Nothing offers to restore, change the currency or delete everything.
    for route in routes::ALL {
        for word in ["restore", "wipe", "currency", "recovery", "token", "lock", "delete-all"] {
            assert!(!route.name.contains(word), "{}", route.name);
        }
    }
}

// ---- yes, no, and nobody to ask ---------------------------------------------------------------

#[test]
fn a_yes_writes_the_file_and_the_person_was_told_what_it_was() {
    let r = rig(true);
    // A change made now is in the change history, which a full JSON export carries.
    assert_eq!(r.post("categories.create", json!({ "name": "Zed", "type": "EXPENSE" })).status, 200);
    let total = call(r.server.port(), Some(&r.read), "export.preview", "{}").data()["transactions"].as_u64().unwrap();
    let out = r.path("everything.json");
    let reply = r.post("export.json", json!({ "out": out }));
    let data = reply.data();
    assert_eq!(data["path"], out.as_str());
    let text = std::fs::read_to_string(&out).unwrap();
    assert_eq!(data["bytes"], text.len());
    let file: Value = serde_json::from_str(&text).unwrap();
    assert!(file["transactions"].as_array().unwrap().len() > 100 && file["categories"].is_array());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(r.asked(), 1);
    let said = r.last();
    assert!(said.contains("\"test\""), "which token: {said}");
    assert!(said.contains(&out), "the exact file: {said}");
    assert!(said.contains("JSON") && said.contains("not encrypted") && said.contains("Allow this?"), "{said}");
    assert!(said.contains(&format!("Transactions: {total}")) && said.contains("change history"), "what is in it: {said}");
    assert_eq!(r.shown.lock().unwrap()[0].title, "meowNY: a program wants to write a file");
}

#[test]
fn exports_say_in_the_approval_that_documents_are_listed_by_name_and_never_as_files() {
    let r = rig(true);
    r.vault
        .with_conn(|c| {
            c.execute(
                "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date) VALUES ('Warranty.pdf', 'application/pdf', 10, ?1, '2026-03-01')",
                ["ab".repeat(32)],
            )?;
            Ok(())
        })
        .unwrap();
    let json_file = r.path("d.json");
    assert_eq!(r.post("export.json", json!({ "out": json_file })).status, 200);
    let said = r.last();
    assert!(said.contains("1 documents, by name") && said.contains("Never the files"), "{said}");
    let file: Value = serde_json::from_str(&std::fs::read_to_string(&json_file).unwrap()).unwrap();
    assert_eq!(file["documents"][0]["name"], "Warranty.pdf");

    assert_eq!(r.post("export.csv", json!({ "out": r.path("d.csv") })).status, 200);
    assert!(r.last().contains("names of the documents attached to each transaction"), "{}", r.last());
    assert!(!r.last().contains("1 documents, by name"));
    assert_eq!(r.post("export.xlsx", json!({ "out": r.path("d.xlsx") })).status, 200);
    assert!(r.last().contains("1 documents, by name"));
}

#[test]
fn a_no_writes_nothing_and_asking_again_at_once_does_not_ask_the_person_again() {
    let r = rig(false);
    let reply = r.post("export.json", json!({ "out": r.path("a.json") }));
    assert_eq!(denied(&reply), "the person did not allow this in the app");
    assert!(r.written().is_empty(), "a file was written after a no");
    let again = r.post("export.csv", json!({ "out": r.path("b.csv") }));
    assert!(denied(&again).contains("a moment ago"));
    assert_eq!(r.asked(), 1, "the person was asked a second time");
    assert!(r.written().is_empty());
}

#[test]
fn with_nobody_to_ask_nothing_is_written() {
    let r = rig_with(true, 0, 3000, false, false);
    for (route, body) in [
        ("export.json", json!({ "out": r.path("a.json") })),
        ("export.csv", json!({ "out": r.path("a.csv") })),
        ("export.xlsx", json!({ "out": r.path("a.xlsx") })),
        ("export.pdf", json!({ "out": r.path("a.pdf"), "fromMonth": "2026-01", "toMonth": "2026-02" })),
        ("backup.create", json!({ "out": r.path("a.meowbak"), "passphrase": PASSPHRASE })),
    ] {
        assert!(denied(&r.post(route, body)).contains("cannot ask"), "{route}");
    }
    assert!(r.written().is_empty());
}

#[test]
fn nobody_answering_is_a_no_and_a_yes_that_comes_too_late_does_nothing() {
    let r = rig_with(true, 600, 150, false, true);
    let reply = r.post("export.json", json!({ "out": r.path("late.json") }));
    assert!(denied(&reply).contains("in time"));
    thread::sleep(Duration::from_millis(700));
    assert!(r.written().is_empty(), "the late yes wrote a file");
}

#[test]
fn only_one_question_is_open_at_a_time() {
    let r = Arc::new(rig_with(true, 500, 3000, false, true));
    let first = {
        let r = r.clone();
        thread::spawn(move || r.post("export.json", json!({ "out": r.path("first.json") })))
    };
    thread::sleep(Duration::from_millis(150));
    let second = r.post("export.csv", json!({ "out": r.path("second.csv") }));
    assert!(denied(&second).contains("waiting for an answer"));
    assert_eq!(first.join().unwrap().status, 200);
    assert_eq!(r.written(), ["first.json"]);
    assert_eq!(r.asked(), 1);
}

#[test]
fn if_the_app_locks_while_the_question_is_open_the_yes_writes_nothing() {
    let r = rig_with(true, 100, 3000, true, true);
    let reply = r.post("export.json", json!({ "out": r.path("a.json") }));
    assert_eq!((reply.status, reply.code().as_str()), (423, "locked"));
    assert!(r.written().is_empty());
}

// ---- a request that could never work does not ask ---------------------------------------------

#[test]
fn a_bad_request_is_refused_without_asking_the_person_anything() {
    let r = rig(true);
    let taken = r.path("taken.json");
    std::fs::write(&taken, "mine").unwrap();
    std::fs::create_dir(r.path("folder.json")).unwrap();
    let cases: Vec<(&str, Value, u16, &str)> = vec![
        ("export.json", json!({ "out": "relative.json" }), 400, "absolute"),
        ("export.json", json!({ "out": r.path("x.csv") }), 400, "must end in .json"),
        ("export.json", json!({ "out": r.path("../escape.json") }), 400, "not contain"),
        ("export.json", json!({ "out": r.path("no/such/folder/x.json") }), 400, "does not exist"),
        ("export.json", json!({ "out": r.path("a\u{202e}b.json") }), 400, "must be a file path"),
        ("export.json", json!({ "out": r.path("a\nb.json") }), 400, "must be a file path"),
        ("export.json", json!({ "out": taken.clone() }), 409, "already exists"),
        ("export.json", json!({ "out": r.path("folder.json") }), 409, "already exists"),
        ("export.json", json!({}), 400, "missing argument `out`"),
        ("export.json", json!({ "out": r.path("x.json"), "format": "x" }), 400, "unknown argument"),
        ("export.json", json!({ "out": r.path("x.json"), "from": "2026-02-30" }), 400, "invalid input"),
        ("export.json", json!({ "out": r.path("x.json"), "categoryIds": "5" }), 400, "must be a list of ids"),
        ("export.csv", json!({ "out": r.path("x.json") }), 400, "must end in .csv"),
        ("export.xlsx", json!({ "out": r.path("x.xls") }), 400, "must end in .xlsx"),
        ("export.pdf", json!({ "out": r.path("x.pdf"), "fromMonth": "2026-03", "toMonth": "2026-01" }), 400, "invalid input"),
        ("export.pdf", json!({ "out": r.path("x.pdf"), "fromMonth": "2026-01", "toMonth": "2026-13" }), 400, "invalid input"),
        ("export.pdf", json!({ "out": r.path("x.pdf"), "fromMonth": "2000-01", "toMonth": "2026-10" }), 400, "invalid input"),
        ("export.pdf", json!({ "out": r.path("x.pdf"), "fromMonth": "2026-01", "toMonth": "2026-02", "page": "A3" }), 400, "must be one of"),
        ("export.pdf", json!({ "out": r.path("x.pdf"), "fromMonth": "2026-01" }), 400, "missing argument `toMonth`"),
        ("export.json", json!({ "out": r.path("x.json"), "sections": "transactions,bogus" }), 400, "unknown variant"),
        ("export.json", json!({ "out": r.path("x.json"), "sections": "" }), 400, "unknown variant"),
        ("export.xlsx", json!({ "out": r.path("x.xlsx"), "sections": "history" }), 400, "invalid input"),
        ("export.json", json!({ "out": r.path("x.json"), "minAmount": "9.5" }), 400, "amount"),
        ("export.csv", json!({ "out": r.path("x.csv"), "minAmount": "10.00", "maxAmount": "5.00" }), 400, "invalid input"),
        ("export.json", json!({ "out": r.path("x.json"), "kind": "income" }), 400, "must be one of"),
        ("export.csv", json!({ "out": r.path("x.csv"), "split": "maybe" }), 400, "must be one of"),
        ("export.csv", json!({ "out": r.path("x.csv"), "sections": "people" }), 400, "unknown argument"),
        ("export.pdf", json!({ "out": r.path("x.pdf"), "fromMonth": "2026-01", "toMonth": "2026-02", "includeDuePayments": "yes" }), 400, "must be a boolean"),
        ("backup.create", json!({ "out": r.path("x.meowbak"), "passphrase": "short" }), 400, "invalid input"),
        ("backup.create", json!({ "out": r.path("x.meowbak") }), 400, "missing argument `passphrase`"),
        ("backup.create", json!({ "out": r.path("x.backup"), "passphrase": PASSPHRASE }), 400, "must end in .meowbak"),
    ];
    for (route, body, status, why) in cases {
        let reply = r.post(route, body.clone());
        assert_eq!(reply.status, status, "{route} {body}: {}", reply.body);
        assert!(reply.message().contains(why), "{route} {body}: {}", reply.message());
    }
    assert_eq!(r.asked(), 0, "the person was asked about a request that could never work");
    assert_eq!(r.written(), ["folder.json", "taken.json"]);
    assert_eq!(std::fs::read_to_string(&taken).unwrap(), "mine", "an existing file was touched");
    // A locked app asks nothing either.
    r.vault.lock();
    assert_eq!(r.post("export.json", json!({ "out": r.path("x.json") })).code(), "locked");
    assert_eq!(r.asked(), 0);
}

#[test]
fn a_file_that_appears_after_the_yes_is_still_not_replaced() {
    // The person is asked while the name is free; before the file is written someone creates it.
    let r = rig_with(true, 200, 3000, false, true);
    let out = r.path("race.json");
    let racer = {
        let out = out.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            std::fs::write(out, "someone else's file").unwrap();
        })
    };
    let reply = r.post("export.json", json!({ "out": out }));
    racer.join().unwrap();
    assert_eq!((reply.status, reply.code().as_str()), (409, "file_exists"));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "someone else's file");
}

// ---- each file ---------------------------------------------------------------------------------

#[test]
fn every_route_writes_a_real_file_of_its_kind() {
    let r = rig(true);
    // CSV: the transactions, with a byte order mark for Excel and the header the app writes.
    let csv = r.path("t.csv");
    assert_eq!(r.post("export.csv", json!({ "out": csv })).status, 200);
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.starts_with("\u{feff}date,type,category,name,amount,currency,description,documents,my_share\r\n"), "{}", &text[..80]);
    assert!(!r.last().contains("budget entries"), "a CSV is only transactions: {}", r.last());
    // Excel: a zip.
    let xlsx = r.path("w.xlsx");
    assert_eq!(r.post("export.xlsx", json!({ "out": xlsx })).status, 200);
    assert_eq!(&std::fs::read(&xlsx).unwrap()[..2], b"PK");
    assert!(r.last().contains("Excel") && r.last().contains("budget entries"));
    // PDF.
    let pdf = r.path("r.pdf");
    let reply = r.post("export.pdf", json!({ "out": pdf, "fromMonth": "2026-01", "toMonth": "2026-03", "includeTransactions": true, "page": "LETTER" }));
    assert_eq!(reply.status, 200, "{}", reply.body);
    let bytes = std::fs::read(&pdf).unwrap();
    assert!(bytes.starts_with(b"%PDF-") && bytes.len() > 2000);
    let said = r.last();
    assert!(said.contains("Months: 2026-01 to 2026-03") && said.contains("Transaction list: yes") && said.contains("Descriptions shown: no"), "{said}");
    // Descriptions only if asked, and the person is told.
    let with = r.path("d.pdf");
    // (a "no" to this token a moment ago would be remembered; it said yes every time here.)
    assert_eq!(r.post("export.pdf", json!({ "out": with, "fromMonth": "2026-01", "toMonth": "2026-01", "includeTransactions": true, "showDescriptions": true })).status, 200);
    assert!(r.last().contains("Descriptions shown: yes"));
    let short = r.path("m.pdf");
    assert_eq!(r.post("export.pdf", json!({ "out": short, "fromMonth": "2026-02", "toMonth": "2026-02" })).status, 200);
    assert!(r.last().contains("Transaction list: no") && !r.last().contains("Descriptions shown"));
    assert_eq!(r.written(), ["d.pdf", "m.pdf", "r.pdf", "t.csv", "w.xlsx"]);
}

#[test]
fn a_selection_narrows_the_file_and_what_the_person_is_told() {
    let r = rig(true);
    let categories = call(r.server.port(), Some(&r.read), "categories.list", r#"{"type":"EXPENSE"}"#).data();
    let (id, name) = (categories[0]["id"].as_str().unwrap().to_string(), categories[0]["name"].as_str().unwrap().to_string());
    let other = categories[1]["name"].as_str().unwrap().to_string();
    let out = r.path("one.json");
    assert_eq!(r.post("export.json", json!({ "out": out, "categoryIds": [id], "from": "2026-09-01", "to": "2026-09-30" })).status, 200);
    let file: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert!(file["transactions"].as_array().unwrap().iter().all(|t| t["categoryId"] == json!(id) && t["date"].as_str().unwrap().starts_with("2026-09")));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains(&name) && !text.contains(&other), "another category's name is in a limited export");
    assert!(r.last().contains("Only the 1 categories chosen"), "{}", r.last());
    // What was shown is what the file holds: the count in the dialog is the count in the file.
    let shown_count: usize = r.last().split("Transactions: ").nth(1).unwrap().split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap();
    assert_eq!(shown_count, file["transactions"].as_array().unwrap().len());
    // And export.preview, which needs no yes, gives the same numbers.
    let preview = call(r.server.port(), Some(&r.read), "export.preview", &json!({ "categoryIds": [id], "from": "2026-09-01", "to": "2026-09-30" }).to_string()).data();
    assert_eq!(preview["transactions"], shown_count);
}

#[test]
fn a_backup_opens_with_its_passphrase_and_the_passphrase_is_never_shown_or_stored_in_it() {
    let r = rig(true);
    let out = r.path("b.meowbak");
    let total = call(r.server.port(), Some(&r.read), "export.preview", "{}").data()["transactions"].as_u64().unwrap();
    let reply = r.post("backup.create", json!({ "out": out, "passphrase": PASSPHRASE }));
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(!reply.body.contains(PASSPHRASE));
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[..8], b"MEOWNYBK");
    let snapshot = backup::open(&bytes, PASSPHRASE).unwrap();
    let live = r.vault.with_conn(|c| backup::dump(c)).unwrap();
    assert_eq!(snapshot.tables.keys().collect::<Vec<_>>(), live.tables.keys().collect::<Vec<_>>());
    assert_eq!(snapshot.rows("transactions").len(), live.rows("transactions").len());
    assert!(backup::open(&bytes, "a different passphrase").is_err(), "the wrong passphrase opens it");
    assert!(!String::from_utf8_lossy(&bytes).contains("Rent"), "the backup is not readable text");
    let said = r.last();
    assert!(!said.contains(PASSPHRASE) && said.contains("Everything") && said.contains("encrypted") && said.contains(&format!("Transactions: {total}")), "{said}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn writing_a_file_changes_nothing_in_the_data() {
    let r = rig(true);
    let before = r.vault.with_conn(|c| backup::dump(c)).unwrap();
    for (route, body) in [
        ("export.json", json!({ "out": r.path("a.json") })),
        ("export.csv", json!({ "out": r.path("a.csv") })),
        ("export.xlsx", json!({ "out": r.path("a.xlsx") })),
        ("export.pdf", json!({ "out": r.path("a.pdf"), "fromMonth": "2026-01", "toMonth": "2026-02" })),
        ("backup.create", json!({ "out": r.path("a.meowbak"), "passphrase": PASSPHRASE })),
    ] {
        assert_eq!(r.post(route, body).status, 200, "{route}");
    }
    assert_eq!(r.vault.with_conn(|c| backup::dump(c)).unwrap(), before);
}

#[test]
fn a_read_token_may_ask_too_because_the_person_decides() {
    let r = rig(true);
    let reply = call(r.server.port(), Some(&r.read), "export.csv", &json!({ "out": r.path("r.csv") }).to_string());
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(r.last().contains("\"test\""));
    // The answer given by the person is per request: a second request is a second question.
    assert_eq!(call(r.server.port(), Some(&r.read), "export.csv", &json!({ "out": r.path("r2.csv") }).to_string()).status, 200);
    assert_eq!(r.asked(), 2);
}

#[test]
fn the_preview_and_the_schedule_need_no_yes() {
    let r = rig_with(false, 0, 3000, false, true);
    let preview = call(r.server.port(), Some(&r.read), "export.preview", "{}").data();
    assert!(preview["transactions"].as_u64().unwrap() > 100);
    assert!(call(r.server.port(), Some(&r.read), "backup.schedule", "{}").data().is_null(), "no schedule yet");
    let folder = tempfile::tempdir().unwrap();
    crate::schedule::start(&r.vault, folder.path(), PASSPHRASE, 7, false, chrono::Utc::now()).unwrap();
    let schedule = call(r.server.port(), Some(&r.read), "backup.schedule", "{}").data();
    assert_eq!((schedule["everyDays"].clone(), schedule["lastOk"].clone()), (json!(7), json!(true)));
    assert_eq!(schedule["folder"], folder.path().to_str().unwrap());
    assert!(!schedule.to_string().contains(PASSPHRASE), "the passphrase is never shown");
    assert_eq!(r.asked(), 0);
}

#[test]
fn the_parts_the_limits_and_the_debts_of_a_report_are_taken_and_told_to_the_person() {
    let r = rig(true);
    // only the due payments and the history of debts, of expenses over 5.00 that are not shared
    let out = r.path("parts.json");
    let reply = r.post("export.json", json!({ "out": out, "sections": "due, Ledger", "kind": "EXPENSE", "minAmount": "5.00", "split": "not_split" }));
    assert_eq!(reply.status, 200, "{}", reply.body);
    let file: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let keys: BTreeSet<&str> = file.as_object().unwrap().keys().map(String::as_str).collect();
    assert!(keys.contains("duePayments") && keys.contains("debtHistory"), "{keys:?}");
    assert!(!keys.contains("transactions") && !keys.contains("categories"), "a part that is left out is not in the file: {keys:?}");
    // a report with the three lists of debts is made in English, and the person is told which are in it
    let pdf = r.path("report.pdf");
    let reply = r.post("export.pdf", json!({ "out": pdf, "fromMonth": "2026-01", "toMonth": "2026-03", "includeDebts": true, "includeDuePayments": true, "includeDebtHistory": true }));
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    let told = r.last();
    assert!(told.contains("Who owes what") && told.contains("Overdue recurring payments") && told.contains("History of debts"), "{told}");
}
