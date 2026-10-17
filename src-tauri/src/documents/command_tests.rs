//! The document commands, called the way the page calls them: a name and a JSON body through Tauri's message layer (its mock
//! runtime), with the vault as managed state. `open_document` and `save_document_copy` take the real window's handle and cannot
//! be called on a mock; they are thin, and what they lean on is tested in `library_tests.rs`.

use std::sync::Arc;

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::commands;
use crate::keys::memory::MemoryStore;
use crate::sync::qr::{base64url_decode, base64url_encode};
use crate::vault::{Settings, Vault};

const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n%%EOF\n";
const PNG: [u8; 32] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 3, 0, 0, 0, 2, 8, 2, 0, 0, 0, 0x12, 0x16, 0xf1,
];

struct Harness {
    _app: App<tauri::test::MockRuntime>,
    webview: WebviewWindow<tauri::test::MockRuntime>,
    vault: Arc<Vault>,
    _dir: tempfile::TempDir,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(Vault::new(dir.path().to_path_buf(), Box::new(MemoryStore::default())));
    vault
        .create(&Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() }, None)
        .unwrap();
    let app = mock_builder()
        .manage(vault.clone())
        .invoke_handler(tauri::generate_handler![
            commands::create_category,
            commands::create_transaction,
            commands::list_documents,
            commands::get_document,
            commands::update_document,
            commands::delete_document,
            commands::link_document,
            commands::unlink_document,
            commands::document_links,
            commands::document_library_size,
            commands::document_counts,
            commands::check_documents,
            commands::read_document_image,
            commands::read_document_preview,
            commands::document_upload_begin,
            commands::document_upload_chunk,
            commands::document_upload_finish,
            commands::document_upload_cancel,
            commands::document_add_dropped,
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
            // the page's own address: anything else is remote content, which a command can only reach with a capability made for it
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

    fn code(&self, cmd: &str, args: Value) -> String {
        match self.call(cmd, args) {
            Err(Value::String(code)) => code,
            other => panic!("{cmd} should have failed with a code, not {other:?}"),
        }
    }

    /// Sends a file the way the page does: begin, pieces, finish.
    fn upload(&self, input: Value, bytes: &[u8], piece: usize) -> Value {
        self.call("document_upload_begin", json!({ "input": input, "size": bytes.len() })).unwrap();
        let mut offset = 0;
        for part in bytes.chunks(piece) {
            let now = self.call("document_upload_chunk", json!({ "offset": offset, "data": base64url_encode(part) })).unwrap();
            offset += part.len();
            assert_eq!(now, json!(offset));
        }
        self.call("document_upload_finish", json!({})).unwrap()
    }

    fn transaction(&self) -> String {
        let category = self.call("create_category", json!({ "name": "Food", "kind": "EXPENSE" })).unwrap();
        let made = self
            .call(
                "create_transaction",
                json!({ "input": { "name": "Weekly shop", "amount": "42.50", "paymentDate": "2026-10-10", "categoryId": category["id"] } }),
            )
            .unwrap();
        made["id"].as_str().unwrap().to_string()
    }
}

#[test]
fn a_file_goes_in_in_pieces_and_comes_back_listed_linked_and_readable() {
    let h = harness();
    let tx = h.transaction();
    let outcome = h.upload(
        json!({ "name": "/home/me/Downloads/Shop receipt.pdf", "documentDate": "2026-10-10", "links": [{ "kind": "transaction", "id": tx }] }),
        PDF,
        20,
    );
    assert_eq!(outcome["alreadyThere"], false);
    let doc = &outcome["document"];
    assert_eq!(doc["name"], "Shop receipt.pdf");
    assert_eq!(doc["mediaType"], "application/pdf");
    assert_eq!((doc["sizeBytes"].as_u64(), doc["linkCount"].as_u64(), doc["hasFile"].as_bool()), (Some(PDF.len() as u64), Some(1), Some(true)));
    assert_eq!(doc["keepOnPhone"], false);
    let id = doc["id"].as_str().unwrap().to_string();

    let page = h.call("list_documents", json!({ "filter": {} })).unwrap();
    assert_eq!((page["total"].as_u64(), page["documents"][0]["id"].as_str()), (Some(1), Some(id.as_str())));
    assert_eq!(h.call("get_document", json!({ "id": id })).unwrap()["name"], "Shop receipt.pdf");
    let links = h.call("document_links", json!({ "id": id })).unwrap();
    assert_eq!((links[0]["kind"].as_str(), links[0]["id"].as_str(), links[0]["name"].as_str()), (Some("transaction"), Some(tx.as_str()), Some("Weekly shop")));
    let found = h.call("list_documents", json!({ "filter": { "transactionId": tx } })).unwrap();
    assert_eq!(found["total"], 1);
    assert_eq!(
        h.call("document_library_size", json!({})).unwrap(),
        json!({ "count": 1, "bytes": PDF.len(), "heldCount": 1, "heldBytes": PDF.len() })
    );
    // the mark on the list of transactions
    let counts = h.call("document_counts", json!({ "kind": "transaction", "ids": [tx, "999"] })).unwrap();
    assert_eq!(counts, json!({ tx.clone(): 1 }));
    assert_eq!(h.call("document_counts", json!({ "kind": "recurring", "ids": [tx] })).unwrap(), json!({}));
    assert_eq!(h.code("document_counts", json!({ "kind": "transaction", "ids": ["x"] })), "invalid_input");
}

#[test]
fn the_same_file_sent_again_is_not_stored_twice() {
    let h = harness();
    let first = h.upload(json!({ "name": "a.pdf" }), PDF, 1000);
    let second = h.upload(json!({ "name": "b.pdf" }), PDF, 1000);
    assert_eq!((first["alreadyThere"].as_bool(), second["alreadyThere"].as_bool()), (Some(false), Some(true)));
    assert_eq!(first["document"]["id"], second["document"]["id"]);
    assert_eq!(h.call("document_library_size", json!({})).unwrap()["count"], 1);
}

#[test]
fn an_image_is_given_for_drawing_and_a_pdf_is_not() {
    let h = harness();
    let image = h.upload(json!({ "name": "photo.png" }), &PNG, 10);
    let id = image["document"]["id"].as_str().unwrap();
    let given = h.call("read_document_image", json!({ "id": id })).unwrap();
    assert_eq!(given["mediaType"], "image/png");
    assert_eq!(base64url_decode(given["data"].as_str().unwrap()).unwrap(), PNG);

    let pdf = h.upload(json!({ "name": "a.pdf" }), PDF, 1000);
    assert_eq!(h.code("read_document_image", json!({ "id": pdf["document"]["id"] })), "invalid_input");
}

#[test]
fn a_preview_is_given_with_its_kind_and_refused_for_what_is_not_shown() {
    let h = harness();
    let pdf = h.upload(json!({ "name": "a.pdf" }), PDF, 1000);
    let given = h.call("read_document_preview", json!({ "id": pdf["document"]["id"] })).unwrap();
    assert_eq!(given["kind"], "pdf");
    assert_eq!(given["mediaType"], "application/pdf");
    assert_eq!(base64url_decode(given["data"].as_str().unwrap()).unwrap(), PDF);

    let mut docx = b"PK\x03\x04".to_vec();
    docx.extend_from_slice(&[0; 20]);
    let word = h.upload(json!({ "name": "Letter.docx" }), &docx, 10);
    assert_eq!(h.call("read_document_preview", json!({ "id": word["document"]["id"] })).unwrap()["kind"], "docx");

    let page = h.upload(json!({ "name": "page.html" }), b"<b>hi</b>", 10);
    assert_eq!(h.code("read_document_preview", json!({ "id": page["document"]["id"] })), "invalid_input");
    assert_eq!(h.code("read_document_preview", json!({ "id": "x" })), "invalid_input");
}

#[test]
fn a_document_is_renamed_linked_unlinked_and_deleted() {
    let h = harness();
    let tx = h.transaction();
    let id = h.upload(json!({ "name": "scan0001.pdf" }), PDF, 1000)["document"]["id"].as_str().unwrap().to_string();

    let changed = h.call("update_document", json!({ "id": id, "patch": { "name": "Shop receipt", "keepOnPhone": true } })).unwrap();
    assert_eq!((changed["name"].as_str(), changed["keepOnPhone"].as_bool()), (Some("Shop receipt"), Some(true)));
    let target = json!({ "kind": "transaction", "id": tx });
    h.call("link_document", json!({ "id": id, "target": target })).unwrap();
    h.call("link_document", json!({ "id": id, "target": target })).unwrap();
    assert_eq!(h.call("get_document", json!({ "id": id })).unwrap()["linkCount"], 1);
    h.call("unlink_document", json!({ "id": id, "target": target })).unwrap();
    assert_eq!(h.code("unlink_document", json!({ "id": id, "target": target })), "not_found");

    h.call("delete_document", json!({ "id": id })).unwrap();
    assert_eq!(h.code("get_document", json!({ "id": id })), "not_found");
    assert_eq!(h.call("document_library_size", json!({})).unwrap()["count"], 0);
    assert_eq!(h.call("check_documents", json!({})).unwrap(), json!({ "missing": 0, "damaged": 0, "cleaned": 0 }));
}

#[test]
fn the_check_says_how_many_files_are_missing() {
    let h = harness();
    h.upload(json!({ "name": "a.pdf" }), PDF, 1000);
    for entry in std::fs::read_dir(h.vault.data_dir().join("documents")).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    assert_eq!(h.call("check_documents", json!({})).unwrap()["missing"], 1);
    let page = h.call("list_documents", json!({ "filter": {} })).unwrap();
    assert_eq!(page["documents"][0]["hasFile"], false);
}

#[test]
fn what_goes_wrong_is_a_fixed_code() {
    let h = harness();
    assert_eq!(h.code("get_document", json!({ "id": "abc" })), "invalid_input");
    assert_eq!(h.code("get_document", json!({ "id": "0" })), "invalid_input");
    assert_eq!(h.code("get_document", json!({ "id": "999" })), "not_found");
    assert_eq!(h.code("delete_document", json!({ "id": "999" })), "not_found");
    assert_eq!(h.code("document_links", json!({ "id": "999" })), "not_found");
    assert_eq!(h.code("link_document", json!({ "id": "999", "target": { "kind": "transaction", "id": "1" } })), "not_found");

    assert_eq!(h.code("document_upload_begin", json!({ "input": { "name": "x" }, "size": 0 })), "invalid_input");
    assert_eq!(h.code("document_upload_begin", json!({ "input": { "name": "x" }, "size": 26_214_401u64 })), "too_large");
    assert_eq!(h.code("document_upload_begin", json!({ "input": { "name": "x", "documentDate": "soon" }, "size": 10 })), "invalid_input");
    assert_eq!(h.code("document_upload_chunk", json!({ "offset": 0, "data": "AAAA" })), "invalid_input", "no upload has begun");
    assert_eq!(h.code("document_upload_finish", json!({})), "invalid_input");

    h.call("document_upload_begin", json!({ "input": { "name": "x" }, "size": 10 })).unwrap();
    assert_eq!(h.code("document_upload_chunk", json!({ "offset": 4, "data": "AAAA" })), "invalid_input", "a gap");
    assert_eq!(h.code("document_upload_finish", json!({})), "invalid_input", "and the upload is over");
}

#[test]
fn arguments_the_command_does_not_know_are_refused() {
    let h = harness();
    // a typo or an extra field must not be quietly dropped, and it must be refused for that reason and no other
    let refused = |cmd: &str, args: Value, what: &str| match h.call(cmd, args) {
        Err(Value::String(text)) => assert!(text.starts_with("invalid args") && text.contains(what), "{cmd}: {text}"),
        other => panic!("{cmd} should have been refused, not {other:?}"),
    };
    refused("document_upload_begin", json!({ "input": { "name": "x", "bogus": 1 }, "size": 10 }), "bogus");
    refused("list_documents", json!({ "filter": { "kind": "video" } }), "video");
    refused("update_document", json!({ "id": "1", "patch": { "sha256": "x" } }), "sha256");
    refused("link_document", json!({ "id": "1", "target": { "kind": "category", "id": "1" } }), "category");
    // and the same calls without the mistake are understood
    assert!(h.call("document_upload_begin", json!({ "input": { "name": "x" }, "size": 10 })).is_ok());
    assert!(h.call("list_documents", json!({ "filter": { "kind": "image" } })).is_ok());
}

#[test]
fn when_the_app_is_locked_nothing_can_be_done_and_a_half_sent_file_is_gone() {
    let h = harness();
    h.call("document_upload_begin", json!({ "input": { "name": "x" }, "size": 10 })).unwrap();
    h.vault.lock();
    for (cmd, args) in [
        ("list_documents", json!({ "filter": {} })),
        ("get_document", json!({ "id": "1" })),
        ("document_library_size", json!({})),
        ("check_documents", json!({})),
        ("read_document_image", json!({ "id": "1" })),
        ("read_document_preview", json!({ "id": "1" })),
        ("document_upload_begin", json!({ "input": { "name": "x" }, "size": 10 })),
    ] {
        assert_eq!(h.code(cmd, args), "locked", "{cmd}");
    }
    assert_eq!(h.code("document_upload_chunk", json!({ "offset": 0, "data": "AAAA" })), "invalid_input", "the upload was dropped by the lock");
    h.vault.unlock(None).unwrap();
    assert_eq!(h.code("document_upload_finish", json!({})), "invalid_input");
}

#[test]
fn cancelling_drops_the_file() {
    let h = harness();
    h.call("document_upload_begin", json!({ "input": { "name": "x" }, "size": 4 })).unwrap();
    h.call("document_upload_chunk", json!({ "offset": 0, "data": base64url_encode(b"ab") })).unwrap();
    h.call("document_upload_cancel", json!({})).unwrap();
    assert_eq!(h.code("document_upload_finish", json!({})), "invalid_input");
}

#[test]
fn a_file_of_the_largest_size_goes_in_through_the_commands() {
    let h = harness();
    let bytes: Vec<u8> = (0..crate::documents::MAX_DOCUMENT_BYTES as usize).map(|i| (i % 251) as u8).collect();
    let outcome = h.upload(json!({ "name": "largest.bin" }), &bytes, 256 * 1024);
    assert_eq!(outcome["document"]["sizeBytes"], crate::documents::MAX_DOCUMENT_BYTES);
}

#[test]
fn files_dropped_on_the_window_are_added_when_the_page_asks_and_only_what_was_dropped() {
    let h = harness();
    let files = tempfile::tempdir().unwrap();
    // nothing was dropped: nothing is added, and the page cannot name a path (the command takes no argument that could)
    let none = h.call("document_add_dropped", json!({})).unwrap();
    assert_eq!(none, json!({ "outcomes": [], "ignored": 0 }));
    let rejected = h.call("document_add_dropped", json!({ "paths": ["/etc/passwd"] })).unwrap();
    assert_eq!(rejected["outcomes"], json!([]), "a path in the request is not a file that was dropped");
    assert_eq!(h.call("document_library_size", json!({})).unwrap()["count"], 0);

    std::fs::write(files.path().join("Receipt.pdf"), PDF).unwrap();
    std::fs::write(files.path().join("empty.pdf"), b"").unwrap();
    h.vault.dropped().set(vec![files.path().join("Receipt.pdf"), files.path().join("empty.pdf")]);
    let done = h.call("document_add_dropped", json!({})).unwrap();
    assert_eq!(
        done,
        json!({ "outcomes": [
            { "name": "Receipt.pdf", "status": "added", "error": null },
            { "name": "empty.pdf", "status": "failed", "error": "invalid_input" },
        ], "ignored": 0 })
    );
    assert_eq!(h.call("document_library_size", json!({})).unwrap()["count"], 1);
    // what was added is not added again by asking again
    assert_eq!(h.call("document_add_dropped", json!({})).unwrap()["outcomes"], json!([]));

    // a locked app adds nothing and forgets
    h.vault.dropped().set(vec![files.path().join("Receipt.pdf")]);
    h.vault.lock();
    assert_eq!(h.code("document_add_dropped", json!({})), "locked");
    assert!(h.vault.dropped().is_empty());
}
