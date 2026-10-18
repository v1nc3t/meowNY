//! The document routes over real sockets. What matters: a read token sees names and never a file; a write token can add, change and
//! delete with the screens' rules; a file goes in as the bytes of one request and a request that big is read only for a caller that has
//! been let in; and a file comes out only when the person says yes, as a new file of the right kind.

use std::io::Write;

use serde_json::{json, Value};

use super::confirmed_tests::{denied, rig, rig_with, Rig};
use super::tests::{call, connect, read_reply};
use crate::documents::library::{self, LinkTarget, NewDocument, TargetKind};
use crate::documents::store::DIR_NAME;
use crate::documents::{DocId, MAX_DOCUMENT_BYTES};
use crate::sync::qr::base64url_encode;

const PDF: &[u8] = b"%PDF-1.4 a receipt for the boiler";

fn pattern(n: usize, seed: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 31 + seed * 7 + i / 251) % 251) as u8).collect()
}

/// A document made the way the screens make one, attached to the transaction with this id.
fn add(r: &Rig, name: &str, bytes: &[u8], transaction: Option<&str>) -> String {
    r.vault
        .with_files(|conn, files| {
            let links = transaction.map(|id| LinkTarget { kind: TargetKind::Transaction, id: id.into() }).into_iter().collect();
            let input = NewDocument { name: name.into(), links, ..Default::default() };
            Ok(library::add(conn, files, &input, bytes, "2026-10-14")?.document.id.to_string())
        })
        .unwrap()
}

fn count(r: &Rig, sql: &str) -> i64 {
    r.vault.with_conn(|c| Ok(c.query_row(sql, [], |row| row.get(0))?)).unwrap()
}

fn folder(r: &Rig) -> Vec<String> {
    let mut names: Vec<String> = match std::fs::read_dir(r.vault.data_dir().join(DIR_NAME)) {
        Ok(entries) => entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

fn call_with(r: &Rig, token: &str, route: &str, body: Value) -> Reply {
    call(r.server.port(), Some(token), route, &body.to_string())
}

type Reply = super::tests::Reply;

// ---- what a read token sees -------------------------------------------------------------------

#[test]
fn a_read_token_lists_and_shows_documents_and_never_a_file() {
    let r = rig(true);
    let a = add(&r, "Boiler receipt.pdf", PDF, Some("1"));
    add(&r, "Notes.txt", b"plain notes about the flat", Some("2"));
    add(&r, "Loose.bin", &[7u8; 3000], None);

    let page = call_with(&r, &r.read, "documents.list", json!({})).data();
    assert_eq!(page["total"], 3);
    let names: Vec<&str> = page["documents"].as_array().unwrap().iter().map(|d| d["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 3);
    let boiler = page["documents"].as_array().unwrap().iter().find(|d| d["name"] == "Boiler receipt.pdf").unwrap();
    assert_eq!(boiler["id"], a.as_str(), "ids are strings, as everywhere");
    assert_eq!(boiler["mediaType"], "application/pdf");
    assert_eq!(boiler["sizeBytes"], PDF.len());
    assert_eq!(boiler["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(boiler["linkCount"], 1);
    assert_eq!(boiler["hasFile"], true);
    let text = serde_json::to_string(&page).unwrap();
    for leak in ["data", "content", "bytes\"", "%PDF", "plain notes", "path"] {
        assert!(!text.contains(leak), "{leak} is in the list: {text}");
    }

    // narrowed by name, kind, attachment and date
    assert_eq!(call_with(&r, &r.read, "documents.list", json!({ "q": "boiler" })).data()["total"], 1);
    assert_eq!(call_with(&r, &r.read, "documents.list", json!({ "kind": "pdf" })).data()["total"], 1);
    assert_eq!(call_with(&r, &r.read, "documents.list", json!({ "kind": "other" })).data()["total"], 2);
    assert_eq!(call_with(&r, &r.read, "documents.list", json!({ "transactionId": "2" })).data()["documents"][0]["name"], "Notes.txt");
    assert_eq!(call_with(&r, &r.read, "documents.list", json!({ "from": "2026-10-15" })).data()["total"], 0);
    assert_eq!(call_with(&r, &r.read, "documents.list", json!({ "limit": 1 })).data()["documents"].as_array().unwrap().len(), 1);
    let bad = call_with(&r, &r.read, "documents.list", json!({ "limit": 1000 }));
    assert_eq!((bad.status, bad.code().as_str()), (400, "invalid_input"));

    let shown = call_with(&r, &r.read, "documents.show", json!({ "id": a })).data();
    assert_eq!(shown["name"], "Boiler receipt.pdf");
    assert_eq!(shown["links"][0]["kind"], "transaction");
    assert_eq!(shown["links"][0]["id"], "1");
    assert!(shown["links"][0]["name"].is_string());
    let text = serde_json::to_string(&shown).unwrap();
    assert!(!text.contains("%PDF") && !text.contains("boiler\""), "{text}");
    assert_eq!(call_with(&r, &r.read, "documents.show", json!({ "id": "999" })).status, 404);
}

#[test]
fn a_read_token_can_do_nothing_more_with_documents() {
    let r = rig(true);
    let id = add(&r, "Receipt.pdf", PDF, Some("1"));
    let before = (count(&r, "SELECT count(*) FROM documents"), count(&r, "SELECT count(*) FROM document_links"), folder(&r));
    for (route, body) in [
        ("documents.add", json!({ "name": "x.pdf", "data": base64url_encode(PDF) })),
        ("documents.update", json!({ "id": id, "name": "Other.pdf" })),
        ("documents.delete", json!({ "id": id })),
        ("documents.link", json!({ "id": id, "transactionId": "2" })),
        ("documents.unlink", json!({ "id": id, "transactionId": "1" })),
        ("documents.get", json!({ "id": id, "out": r.path("copy.pdf") })),
    ] {
        let reply = call_with(&r, &r.read, route, body);
        assert_eq!((reply.status, reply.code().as_str()), (403, "forbidden"), "{route}: {}", reply.body);
    }
    assert_eq!((count(&r, "SELECT count(*) FROM documents"), count(&r, "SELECT count(*) FROM document_links"), folder(&r)), before);
    assert_eq!(r.asked(), 0, "a read token never makes the app ask");
    assert!(r.written().is_empty());
}

// ---- adding -----------------------------------------------------------------------------------

#[test]
fn a_write_token_adds_a_file_from_its_bytes_and_it_is_stored_encrypted_and_attached() {
    let r = rig(true);
    let bytes = pattern(300_000, 1);
    let body = json!({ "name": "Heating contract.pdf", "data": base64url_encode(&bytes), "date": "2026-09-30", "keepOnPhone": true, "transactionIds": ["1", "2"], "recurringIds": ["1"] });
    let out = r.post("documents.add", body.clone()).data();
    assert_eq!(out["alreadyThere"], false);
    let doc = &out;
    assert_eq!((doc["name"].as_str(), doc["documentDate"].as_str(), doc["keepOnPhone"].as_bool()), (Some("Heating contract.pdf"), Some("2026-09-30"), Some(true)));
    assert_eq!(doc["sizeBytes"], bytes.len());
    assert_eq!(doc["linkCount"], 3);
    let id: i64 = doc["id"].as_str().unwrap().parse().unwrap();
    let read = r.vault.with_files(|conn, files| Ok(library::read(conn, files, id)?.to_vec())).unwrap();
    assert!(read == bytes, "the bytes come back whole");

    // on the disk the file is sealed: nothing of it is readable, and there is one file
    let names = folder(&r);
    assert_eq!(names.len(), 1);
    let sealed = std::fs::read(r.vault.data_dir().join(DIR_NAME).join(&names[0])).unwrap();
    assert!(!sealed.windows(64).any(|w| bytes.windows(64).next() == Some(w)), "the plain bytes are in the folder");

    // recorded in the history by name, never by hash or contents
    let history = count(&r, "SELECT count(*) FROM audit_log WHERE entity_type = 'documents' AND action = 'INSERT'");
    assert_eq!(history, 1);
    let recorded: String = r.vault.with_conn(|c| Ok(c.query_row("SELECT new_values FROM audit_log WHERE entity_type = 'documents'", [], |row| row.get(0))?)).unwrap();
    assert!(recorded.contains("Heating contract.pdf") && !recorded.contains(doc["sha256"].as_str().unwrap()), "{recorded}");

    // the same file again stores nothing and attaches to the one that is there
    let again = r.post("documents.add", json!({ "name": "Copy of it.pdf", "data": base64url_encode(&bytes), "transactionIds": ["3"] })).data();
    assert_eq!(again["alreadyThere"], true);
    assert_eq!(again["id"], doc["id"]);
    assert_eq!(again["linkCount"], 4);
    assert_eq!(folder(&r).len(), 1);
    assert_eq!(count(&r, "SELECT count(*) FROM documents"), 1);
    assert_eq!(r.asked(), 0, "adding does not ask: the program that calls read the file itself");
}

#[test]
fn what_cannot_be_stored_is_refused_and_nothing_is_left_behind() {
    let r = rig(true);
    let ok_data = base64url_encode(PDF);
    let cases: Vec<(&str, Value, u16, &str)> = vec![
        ("not base64url", json!({ "name": "a.pdf", "data": "not base64url!" }), 400, "invalid_input"),
        ("padding", json!({ "name": "a.pdf", "data": "JVBERg==" }), 400, "invalid_input"),
        ("no name", json!({ "data": ok_data }), 400, "invalid_input"),
        ("a date that is not one", json!({ "name": "a.pdf", "data": ok_data, "date": "2026-02-30" }), 400, "invalid_input"),
        ("a transaction that is not there", json!({ "name": "a.pdf", "data": ok_data, "transactionIds": ["999999"] }), 404, "not_found"),
        ("an argument nobody defined", json!({ "name": "a.pdf", "data": ok_data, "path": "/etc/passwd" }), 400, "invalid_input"),
        ("empty bytes", json!({ "name": "a.pdf", "data": "" }), 400, "invalid_input"),
    ];
    for (what, body, status, code) in cases {
        let reply = r.post("documents.add", body);
        assert_eq!((reply.status, reply.code().as_str()), (status, code), "{what}: {}", reply.body);
    }
    // more than the largest file
    let too_big = "A".repeat(super::documents::MAX_DATA_CHARS + 4);
    let reply = r.post("documents.add", json!({ "name": "big.bin", "data": too_big }));
    assert_eq!((reply.status, reply.code().as_str()), (413, "too_large"));
    assert_eq!(count(&r, "SELECT count(*) FROM documents"), 0);
    assert!(folder(&r).is_empty(), "{:?}", folder(&r));
}

#[test]
fn a_name_from_a_caller_is_cleaned_like_one_from_a_file_chooser() {
    let r = rig(true);
    for (given, kept) in [("../../etc/passwd", "passwd"), ("C:\\Users\\me\\invoice.pdf", "invoice.pdf"), ("a\u{202e}b.pdf", "ab.pdf")] {
        let out = r.post("documents.add", json!({ "name": given, "data": base64url_encode(format!("content of {given}").as_bytes()) })).data();
        assert_eq!(out["name"], kept, "{given:?}");
    }
    // a name that is too long is cut to 120 characters, keeping its extension
    let long = format!("{}.pdf", "n".repeat(300));
    let out = r.post("documents.add", json!({ "name": long, "data": base64url_encode(b"another content") })).data();
    let name = out["name"].as_str().unwrap();
    assert!(name.chars().count() <= 120 && name.ends_with(".pdf"), "{name}");
}

// ---- how big a request may be -----------------------------------------------------------------

/// Only the head of a request that announces `length` bytes of body, as a program would send it before the body.
fn head_only(r: &Rig, token: Option<&str>, route: &str, length: usize) -> Reply {
    let mut stream = connect(r.server.port());
    let mut head = format!("POST /v1/{route} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n", r.server.port());
    if let Some(token) = token {
        head.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    head.push_str(&format!("Content-Length: {length}\r\n\r\n"));
    stream.write_all(head.as_bytes()).unwrap();
    // the body never comes: the answer must not wait for it
    read_reply(&mut stream)
}

#[test]
fn a_request_over_a_megabyte_is_read_only_for_the_route_that_takes_a_file_and_a_caller_that_is_let_in() {
    let r = rig(true);
    let big = 20 * 1024 * 1024;
    // refused from the head alone, without a byte of the body
    assert_eq!(head_only(&r, Some(&r.write), "status", big).status, 413, "no other route takes it");
    assert_eq!(head_only(&r, Some(&r.write), "no.such.route", big).status, 413);
    let reply = head_only(&r, None, "documents.add", big);
    assert_eq!((reply.status, reply.code().as_str()), (401, "unauthorized"), "no token, no body is read");
    let reply = head_only(&r, Some("mny_notatoken"), "documents.add", big);
    assert_eq!(reply.status, 401);
    let reply = head_only(&r, Some(&r.read), "documents.add", big);
    assert_eq!((reply.status, reply.code().as_str()), (403, "forbidden"), "a read token is told so before it sends anything");
    assert_eq!(head_only(&r, Some(&r.write), "documents.add", super::http::MAX_UPLOAD_BODY + 1).status, 413, "and there is a limit for it too");

    // a caller that is let in can send a file of several megabytes
    let bytes = pattern(6 * 1024 * 1024, 2);
    let body = json!({ "name": "scan.bin", "data": base64url_encode(&bytes) }).to_string();
    assert!(body.len() > super::http::MAX_BODY * 7);
    let reply = call(r.server.port(), Some(&r.write), "documents.add", &body);
    assert_eq!(reply.status, 200, "{}", &reply.body[..reply.body.len().min(200)]);
    let id: i64 = reply.data()["id"].as_str().unwrap().parse().unwrap();
    assert!(r.vault.with_files(|conn, files| Ok(library::read(conn, files, id)?.to_vec())).unwrap() == bytes);
    // the same size to a route that does not take one is still refused
    let reply = call(r.server.port(), Some(&r.write), "documents.update", &json!({ "id": id.to_string(), "name": "x".repeat(2 * 1024 * 1024) }).to_string());
    assert_eq!(reply.status, 413);
    // and the server is whole
    assert_eq!(call(r.server.port(), Some(&r.read), "status", "").status, 200);
}

/// Takes half a minute in a debug build, so it runs only when asked (`cargo test -- --ignored`).
#[test]
#[ignore = "slow in a debug build"]
fn the_largest_file_goes_in_over_http() {
    let r = rig(true);
    let bytes = pattern(MAX_DOCUMENT_BYTES as usize, 3);
    let reply = call(r.server.port(), Some(&r.write), "documents.add", &json!({ "name": "largest.bin", "data": base64url_encode(&bytes) }).to_string());
    assert_eq!(reply.status, 200, "{}", &reply.body[..reply.body.len().min(200)]);
    let id: i64 = reply.data()["id"].as_str().unwrap().parse().unwrap();
    assert!(r.vault.with_files(|conn, files| Ok(library::read(conn, files, id)?.to_vec())).unwrap() == bytes);
}

// ---- changing, attaching, deleting ------------------------------------------------------------

#[test]
fn a_document_is_renamed_redated_switched_attached_detached_and_deleted() {
    let r = rig(true);
    let id = add(&r, "Receipt.pdf", PDF, None);
    let updated = r.post("documents.update", json!({ "id": id, "name": "Boiler receipt.pdf", "date": "2026-10-01", "keepOnPhone": true })).data();
    assert_eq!((updated["name"].as_str(), updated["documentDate"].as_str(), updated["keepOnPhone"].as_bool()), (Some("Boiler receipt.pdf"), Some("2026-10-01"), Some(true)));
    let only_name = r.post("documents.update", json!({ "id": id, "name": "Receipt, boiler.pdf" })).data();
    assert_eq!((only_name["documentDate"].as_str(), only_name["keepOnPhone"].as_bool()), (Some("2026-10-01"), Some(true)), "what is left out stays");
    let nothing = r.post("documents.update", json!({ "id": id }));
    assert_eq!((nothing.status, nothing.message().as_str()), (400, "give at least one of name, date and keepOnPhone"));
    assert_eq!(r.post("documents.update", json!({ "id": id, "name": "" })).status, 400);
    assert_eq!(r.post("documents.update", json!({ "id": "999", "name": "x.pdf" })).status, 404);

    // attached to a transaction and to a recurring item; once more changes nothing; exactly one target
    assert_eq!(r.post("documents.link", json!({ "id": id, "transactionId": "1" })).status, 200);
    assert_eq!(r.post("documents.link", json!({ "id": id, "transactionId": "1" })).status, 200);
    assert_eq!(r.post("documents.link", json!({ "id": id, "recurringId": "1" })).status, 200);
    assert_eq!(count(&r, "SELECT count(*) FROM document_links"), 2);
    for body in [json!({ "id": id }), json!({ "id": id, "transactionId": "1", "recurringId": "1" })] {
        let reply = r.post("documents.link", body);
        assert_eq!((reply.status, reply.message().as_str()), (400, "give exactly one of transactionId, recurringId and ledgerEntryId"));
    }
    assert_eq!(r.post("documents.link", json!({ "id": id, "transactionId": "999999" })).status, 404);
    assert_eq!(r.post("documents.link", json!({ "id": "999", "transactionId": "1" })).status, 404);
    assert_eq!(call_with(&r, &r.read, "documents.show", json!({ "id": id })).data()["links"].as_array().unwrap().len(), 2);

    assert_eq!(r.post("documents.unlink", json!({ "id": id, "transactionId": "1" })).status, 200);
    assert_eq!(r.post("documents.unlink", json!({ "id": id, "transactionId": "1" })).status, 404, "it was not attached any more");
    assert_eq!(count(&r, "SELECT count(*) FROM document_links"), 1);
    assert_eq!(count(&r, "SELECT count(*) FROM transactions WHERE id = 1"), 1, "the transaction stays");

    // deleting takes the file and the attachments, and leaves the transaction and the recurring item
    assert_eq!(folder(&r).len(), 1);
    assert_eq!(r.post("documents.delete", json!({ "id": id })).status, 200);
    assert_eq!((count(&r, "SELECT count(*) FROM documents"), count(&r, "SELECT count(*) FROM document_links")), (0, 0));
    assert!(folder(&r).is_empty());
    assert_eq!(count(&r, "SELECT count(*) FROM recurring_transactions WHERE id = 1"), 1);
    assert_eq!(r.post("documents.delete", json!({ "id": id })).status, 404);
    let history = |action: &str| count(&r, &format!("SELECT count(*) FROM audit_log WHERE entity_type = 'documents' AND action = '{action}'"));
    assert_eq!((history("INSERT"), history("DELETE")), (1, 1));
    assert!(history("UPDATE") >= 2, "the changes of a name are in the history");
}

#[test]
fn a_document_can_be_attached_to_an_entry_of_the_ledger_and_goes_with_it() {
    let r = rig(true);
    let anna = r.post("people.create", json!({ "name": "Anna" })).data()["id"].as_str().unwrap().to_string();
    let entry = r.post("ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "40.00", "date": "2026-10-01" })).data()["id"].as_str().unwrap().to_string();
    let id = add(&r, "Loan note.pdf", PDF, None);

    // attached once, and once more changes nothing; an entry that is not there is not found; exactly one target
    assert_eq!(r.post("documents.link", json!({ "id": id, "ledgerEntryId": entry })).status, 200);
    assert_eq!(r.post("documents.link", json!({ "id": id, "ledgerEntryId": entry })).status, 200);
    assert_eq!(count(&r, "SELECT count(*) FROM document_links WHERE ledger_entry_id IS NOT NULL"), 1);
    assert_eq!(r.post("documents.link", json!({ "id": id, "ledgerEntryId": "999999" })).status, 404);
    let two = r.post("documents.link", json!({ "id": id, "ledgerEntryId": entry, "transactionId": "1" }));
    assert_eq!((two.status, two.message().as_str()), (400, "give exactly one of transactionId, recurringId and ledgerEntryId"));

    // the read routes say what it is attached to, by the friend's name and the day
    let shown = call_with(&r, &r.read, "documents.show", json!({ "id": id })).data();
    assert_eq!((shown["links"][0]["kind"].as_str(), shown["links"][0]["name"].as_str(), shown["links"][0]["date"].as_str()), (Some("ledger_entry"), Some("Anna"), Some("2026-10-01")));
    // an entry also says which friend it is with, for a screen that goes to the friend's page; the other kinds have no such field
    assert_eq!(shown["links"][0]["personId"].as_str(), Some(anna.as_str()));
    assert_eq!(call_with(&r, &r.read, "documents.list", json!({ "ledgerEntryId": entry })).data()["total"], 1);
    assert_eq!(call_with(&r, &r.read, "documents.list", json!({ "transactionId": "1" })).data()["total"], 0);

    // a new document can be attached to entries as it is added
    let added = r.post("documents.add", json!({ "name": "Invoice.pdf", "data": base64url_encode(&pattern(2000, 3)), "ledgerEntryIds": [entry] })).data();
    assert_eq!(added["linkCount"], 1);
    assert_eq!(call_with(&r, &r.read, "documents.list", json!({ "ledgerEntryId": entry })).data()["total"], 2);

    // detaching leaves the document and the entry; deleting the entry leaves the document
    assert_eq!(r.post("documents.unlink", json!({ "id": id, "ledgerEntryId": entry })).status, 200);
    assert_eq!(r.post("documents.unlink", json!({ "id": id, "ledgerEntryId": entry })).status, 404);
    assert_eq!(count(&r, "SELECT count(*) FROM documents"), 2);
    assert_eq!(r.post("ledger.delete", json!({ "id": entry })).status, 200);
    assert_eq!((count(&r, "SELECT count(*) FROM documents"), count(&r, "SELECT count(*) FROM document_links WHERE ledger_entry_id IS NOT NULL")), (2, 0));
}

// ---- getting a file out -----------------------------------------------------------------------

#[test]
fn a_file_comes_out_only_when_the_person_says_yes_and_only_as_a_new_file_of_its_kind() {
    let r = rig(true);
    let id = add(&r, "Boiler receipt.pdf", PDF, Some("1"));
    let out = r.path("copy.pdf");
    let reply = r.post("documents.get", json!({ "id": id, "out": out })).data();
    assert_eq!((reply["path"].as_str(), reply["bytes"].as_u64()), (Some(out.as_str()), Some(PDF.len() as u64)));
    assert_eq!(std::fs::read(&out).unwrap(), PDF, "exactly the file that was added");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o600);
    }
    // what the person was shown
    assert_eq!(r.asked(), 1);
    let said = r.last();
    assert!(said.contains("\"test\""), "which token: {said}");
    assert!(said.contains("Boiler receipt.pdf") && said.contains("application/pdf") && said.contains(&format!("{} bytes", PDF.len())), "which document: {said}");
    assert!(said.contains(&out) && said.contains("not encrypted") && said.contains("Allow this?"), "the exact file: {said}");
    assert_eq!(r.shown.lock().unwrap()[0].title, "meowNY: a program wants to write a file");

    // a file that is there is never replaced
    std::fs::write(r.path("taken.pdf"), "someone else's").unwrap();
    let reply = r.post("documents.get", json!({ "id": id, "out": r.path("taken.pdf") }));
    assert_eq!((reply.status, reply.code().as_str()), (409, "file_exists"));
    assert_eq!(std::fs::read_to_string(r.path("taken.pdf")).unwrap(), "someone else's");
    assert_eq!(r.asked(), 1, "a request that could never work does not ask");
}

#[test]
fn the_name_of_a_copy_must_end_in_the_extension_of_its_kind_so_it_cannot_be_something_a_system_runs() {
    let r = rig(true);
    let pdf = add(&r, "Receipt.pdf", PDF, None);
    let unknown = add(&r, "Strange.exe", &pattern(3000, 4), None);
    let text = add(&r, "Notes.txt", b"plain notes about the flat", None);
    for (id, out, ok) in [
        (&pdf, "a.pdf", true),
        (&pdf, "a.png", false),
        (&pdf, "run.sh", false),
        (&pdf, "start.desktop", false),
        (&pdf, "evil.pdf.bat", false),
        (&pdf, "a", false),
        (&unknown, "a.exe", false),
        (&unknown, "a.pdf", false),
        (&unknown, "a.bin", true),
        (&text, "a.txt", true),
        (&text, "a.html", false),
    ] {
        let before = r.asked();
        let reply = r.post("documents.get", json!({ "id": id, "out": r.path(out) }));
        if ok {
            assert_eq!(reply.status, 200, "{out}: {}", reply.body);
        } else {
            assert_eq!((reply.status, reply.code().as_str()), (400, "invalid_input"), "{out}: {}", reply.body);
            assert_eq!(r.asked(), before, "{out}: a refusal does not ask");
        }
    }
    assert_eq!(r.written(), ["a.bin", "a.pdf", "a.txt"]);
}

#[test]
fn a_no_and_nobody_to_ask_write_nothing() {
    let no = rig(false);
    let id = add(&no, "Receipt.pdf", PDF, None);
    let reply = no.post("documents.get", json!({ "id": id, "out": no.path("copy.pdf") }));
    assert_eq!(denied(&reply), "the person did not allow this in the app");
    assert!(no.written().is_empty());
    assert_eq!(no.asked(), 1);

    let nobody = rig_with(true, 0, 3000, false, false);
    let id = add(&nobody, "Receipt.pdf", PDF, None);
    let reply = nobody.post("documents.get", json!({ "id": id, "out": nobody.path("copy.pdf") }));
    assert_eq!(denied(&reply), "this needs the person to allow it in the app, and the app cannot ask right now");
    assert!(nobody.written().is_empty());
}

#[test]
fn a_document_this_computer_has_no_good_file_for_is_not_given_out_and_nothing_is_written() {
    let r = rig(true);
    // a row with no file
    r.vault
        .with_conn(|c| {
            c.execute(
                "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date) VALUES ('Row only.pdf', 'application/pdf', 10, ?1, '2026-10-01')",
                ["ab".repeat(32)],
            )?;
            Ok(())
        })
        .unwrap();
    let reply = r.post("documents.get", json!({ "id": "1", "out": r.path("copy.pdf") }));
    assert_eq!((reply.status, reply.code().as_str()), (404, "not_found"));
    assert_eq!(r.asked(), 0, "nothing to ask about");

    // a file that no longer opens: the person is asked, says yes, and gets an error and no file
    let id = add(&r, "Receipt.pdf", PDF, None);
    let sync_id: String = r.vault.with_conn(|c| Ok(c.query_row("SELECT sync_id FROM documents WHERE id = ?1", [&id], |row| row.get(0))?)).unwrap();
    let name = r.vault.with_files(|_, files| Ok(files.keys.file_name(&DocId::parse(&sync_id).unwrap()))).unwrap();
    let path = r.vault.data_dir().join(DIR_NAME).join(name);
    let mut sealed = std::fs::read(&path).unwrap();
    let last = sealed.len() - 1;
    sealed[last] ^= 1;
    std::fs::write(&path, sealed).unwrap();
    let reply = r.post("documents.get", json!({ "id": id, "out": r.path("copy.pdf") }));
    assert_eq!((reply.status, reply.code().as_str()), (500, "internal"), "{}", reply.body);
    assert!(r.written().is_empty(), "nothing is written for a file that does not open");
}

#[test]
fn a_locked_app_gives_nothing_and_asks_nothing() {
    let r = rig(true);
    let id = add(&r, "Receipt.pdf", PDF, None);
    r.vault.lock();
    for (token, route, body) in [
        (&r.read, "documents.list", json!({})),
        (&r.read, "documents.show", json!({ "id": id })),
        (&r.write, "documents.add", json!({ "name": "a.pdf", "data": base64url_encode(PDF) })),
        (&r.write, "documents.get", json!({ "id": id, "out": r.path("copy.pdf") })),
    ] {
        let reply = call_with(&r, token, route, body);
        assert_eq!((reply.status, reply.code().as_str()), (423, "locked"), "{route}");
    }
    assert_eq!(r.asked(), 0);
    assert!(r.written().is_empty());
}

#[test]
fn a_big_body_to_a_locked_app_is_refused_before_it_is_read_like_any_other() {
    let r = rig(true);
    r.vault.lock();
    // a locked app cannot check any token, so it does not let a big body in either
    let reply = head_only(&r, Some(&r.write), "documents.add", 20 * 1024 * 1024);
    assert_eq!((reply.status, reply.code().as_str()), (423, "locked"));
}
