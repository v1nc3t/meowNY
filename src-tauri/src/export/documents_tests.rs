//! Documents in the JSON and CSV exports: what is listed, what a limited export keeps back, and that no file is ever in one.

use serde_json::Value;

use super::tests::{lived_in, parsed, read_csv};
use super::*;

/// A document with no file (an export only ever reads the row), returning its id.
pub(super) fn add_doc(conn: &Connection, name: &str, date: &str) -> i64 {
    conn.execute(
        "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date) VALUES (?1, 'application/pdf', 1234, ?2, ?3)",
        params![name, crate::keys::hex_encode(&crate::documents::store::sha256(name.as_bytes())), date],
    )
    .unwrap();
    conn.last_insert_rowid()
}

pub(super) fn attach_transaction(conn: &Connection, doc: i64, name: &str, date: &str) {
    let id: i64 = conn.query_row("SELECT id FROM transactions WHERE name = ?1 AND payment_date = ?2", params![name, date], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO document_links (document_id, transaction_id) VALUES (?1, ?2)", params![doc, id]).unwrap();
}

pub(super) fn attach_recurring(conn: &Connection, doc: i64, name: &str) {
    let id: i64 = conn.query_row("SELECT id FROM recurring_transactions WHERE name = ?1", [name], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO document_links (document_id, recurring_transaction_id) VALUES (?1, ?2)", params![doc, id]).unwrap();
}

/// `lived_in()` (Pay in Salary; two Rents and a recurring Rent in Rent; a Session and a recurring Therapy plan in the private
/// category Therapy) with documents on all of it, one attached to both a public and a private thing, and one attached to nothing.
pub(super) fn with_documents() -> (Connection, i64, i64, i64) {
    let (conn, salary, rent, secret) = lived_in();
    attach_transaction(&conn, add_doc(&conn, "Pay slip.pdf", "2026-01-26"), "Pay", "2026-01-25");
    let lease = add_doc(&conn, "Lease.pdf", "2026-01-02");
    attach_transaction(&conn, lease, "Rent", "2026-02-01");
    attach_recurring(&conn, lease, "Rent");
    attach_transaction(&conn, add_doc(&conn, "Therapy invoice.pdf", "2026-02-10"), "Session", "2026-02-10");
    let shared = add_doc(&conn, "Shared scan.pdf", "2026-02-11");
    attach_transaction(&conn, shared, "Session", "2026-02-10");
    attach_transaction(&conn, shared, "Pay", "2026-01-25");
    attach_recurring(&conn, add_doc(&conn, "Therapy plan contract.pdf", "2026-01-03"), "Therapy plan");
    add_doc(&conn, "Loose receipt.pdf", "2026-03-05");
    (conn, salary, rent, secret)
}

fn names(file: &Value) -> Vec<String> {
    file["documents"].as_array().unwrap().iter().map(|d| d["name"].as_str().unwrap().to_string()).collect()
}

#[test]
fn the_full_json_lists_every_document_with_what_it_is_attached_to() {
    let (conn, ..) = with_documents();
    let file = parsed(&conn, &Selection::default());
    // oldest paper first
    assert_eq!(
        names(&file),
        ["Lease.pdf", "Therapy plan contract.pdf", "Pay slip.pdf", "Therapy invoice.pdf", "Shared scan.pdf", "Loose receipt.pdf"]
    );
    let lease = &file["documents"][0];
    assert_eq!(lease["date"], "2026-01-02");
    assert_eq!(lease["type"], "application/pdf");
    assert_eq!(lease["sizeBytes"], 1234);
    assert_eq!(lease["sha256"], crate::keys::hex_encode(&crate::documents::store::sha256(b"Lease.pdf")));
    assert!(lease["id"].is_string(), "ids are strings, as everywhere in the file");
    let links = lease["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!((links[0]["kind"].as_str(), links[0]["name"].as_str(), links[0]["date"].as_str()), (Some("transaction"), Some("Rent"), Some("2026-02-01")));
    assert_eq!((links[1]["kind"].as_str(), links[1]["name"].as_str(), links[1]["date"].is_null()), (Some("recurring"), Some("Rent"), true));
    // an attachment points at something that is in the file
    let tx_ids: Vec<&str> = file["transactions"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(tx_ids.contains(&links[0]["id"].as_str().unwrap()));
    // a document that is attached to nothing is listed, with no links
    let loose = file["documents"].as_array().unwrap().iter().find(|d| d["name"] == "Loose receipt.pdf").unwrap();
    assert_eq!(loose["links"], serde_json::json!([]));
    // nothing but the description of a document is in the file: no bytes, no path, no sealed name
    let keys: Vec<&str> = loose.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["date", "id", "links", "name", "sha256", "sizeBytes", "type"]);
}

#[test]
fn a_file_without_documents_has_an_empty_list() {
    let (conn, ..) = lived_in();
    assert_eq!(parsed(&conn, &Selection::default())["documents"], serde_json::json!([]));
}

#[test]
fn a_limited_export_lists_only_documents_attached_to_what_is_in_it_and_only_those_attachments() {
    let (conn, salary, rent, secret) = with_documents();
    let private = ["Therapy invoice.pdf", "Therapy plan contract.pdf", "Session", "Therapy plan", "Dr. Private"];

    // the public categories: the shared scan is there, with only its attachment to the pay
    let file = parsed(&conn, &Selection { category_ids: Some(vec![salary.to_string(), rent.to_string()]), ..Default::default() });
    assert_eq!(names(&file), ["Lease.pdf", "Pay slip.pdf", "Shared scan.pdf"]);
    let shared = file["documents"].as_array().unwrap().iter().find(|d| d["name"] == "Shared scan.pdf").unwrap();
    assert_eq!(shared["links"].as_array().unwrap().len(), 1);
    assert_eq!(shared["links"][0]["name"], "Pay");
    let text = serde_json::to_string(&file).unwrap();
    for word in private {
        assert!(!text.contains(word), "{word} is in an export that left Therapy out");
    }
    assert!(!text.contains("Loose receipt"), "a document attached to nothing is not part of a limited export");

    // only the private category: the pay slip and the lease are not in it, and the shared scan shows only its session
    let file = parsed(&conn, &Selection { category_ids: Some(vec![secret.to_string()]), ..Default::default() });
    assert_eq!(names(&file), ["Therapy plan contract.pdf", "Therapy invoice.pdf", "Shared scan.pdf"]);
    let shared = file["documents"].as_array().unwrap().iter().find(|d| d["name"] == "Shared scan.pdf").unwrap();
    assert_eq!(shared["links"].as_array().unwrap().len(), 1);
    assert_eq!(shared["links"][0]["name"], "Session");
    let text = serde_json::to_string(&file).unwrap();
    assert!(!text.contains("Pay slip") && !text.contains("Lease"), "{text}");

    // no categories at all: nothing is listed
    let file = parsed(&conn, &Selection { category_ids: Some(vec![]), ..Default::default() });
    assert!(names(&file).is_empty());
}

#[test]
fn a_range_of_dates_keeps_the_documents_of_the_transactions_in_it_and_of_the_recurring_items_listed() {
    let (conn, ..) = with_documents();
    // March only: the one transaction in it has no document, so only what hangs on the recurring items (which a date does not limit)
    let file = parsed(&conn, &Selection { from: Some("2026-03-01".into()), ..Default::default() });
    assert_eq!(names(&file), ["Lease.pdf", "Therapy plan contract.pdf"]);
    let lease = &file["documents"][0];
    assert_eq!(lease["links"].as_array().unwrap().len(), 1, "its attachment to the February rent is out of the range");
    assert_eq!(lease["links"][0]["kind"], "recurring");
}

#[test]
fn whatever_the_selection_a_listed_document_is_attached_to_something_listed_and_nothing_attached_is_missed() {
    let (conn, salary, rent, secret) = with_documents();
    for selection in [
        Selection::default(),
        Selection { from: Some("2026-02-01".into()), ..Default::default() },
        Selection { from: Some("2026-02-01".into()), to: Some("2026-02-28".into()), ..Default::default() },
        Selection { category_ids: Some(vec![rent.to_string()]), ..Default::default() },
        Selection { category_ids: Some(vec![salary.to_string(), secret.to_string()]), from: Some("2026-02-05".into()), to: None },
        Selection { category_ids: Some(vec![]), ..Default::default() },
    ] {
        let file = parsed(&conn, &selection);
        let ids = |section: &str| -> Vec<String> { file[section].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect() };
        let (transactions, recurring) = (ids("transactions"), ids("recurring"));
        let everything = selection.from.is_none() && selection.to.is_none() && selection.category_ids.is_none();
        for doc in file["documents"].as_array().unwrap() {
            let links = doc["links"].as_array().unwrap();
            assert!(everything || !links.is_empty(), "{selection:?}: {} has nothing in the file", doc["name"]);
            for link in links {
                let id = link["id"].as_str().unwrap().to_string();
                let in_file = if link["kind"] == "transaction" { transactions.contains(&id) } else { recurring.contains(&id) };
                assert!(in_file, "{selection:?}: {} is attached to something that is not in the file", doc["name"]);
            }
        }
        // and every attachment to something in the file is there: count them in the database
        let expected: i64 = conn
            .query_row(
                "SELECT (SELECT count(*) FROM document_links WHERE transaction_id IN (SELECT value FROM json_each(?1)))
                      + (SELECT count(*) FROM document_links WHERE recurring_transaction_id IN (SELECT value FROM json_each(?2)))",
                params![serde_json::to_string(&transactions.iter().map(|i| i.parse::<i64>().unwrap()).collect::<Vec<_>>()).unwrap(), serde_json::to_string(&recurring.iter().map(|i| i.parse::<i64>().unwrap()).collect::<Vec<_>>()).unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        let listed: usize = file["documents"].as_array().unwrap().iter().map(|d| d["links"].as_array().unwrap().len()).sum();
        assert_eq!(listed as i64, expected, "{selection:?}");
    }
}

#[test]
fn the_preview_counts_the_documents_the_json_lists() {
    let (conn, salary, rent, _) = with_documents();
    for selection in [
        Selection::default(),
        Selection { from: Some("2026-03-01".into()), ..Default::default() },
        Selection { category_ids: Some(vec![salary.to_string(), rent.to_string()]), ..Default::default() },
        Selection { category_ids: Some(vec![]), ..Default::default() },
    ] {
        let shown = preview(&conn, &selection).unwrap();
        assert_eq!(shown.documents as usize, names(&parsed(&conn, &selection)).len(), "{selection:?}");
    }
    assert_eq!(preview(&conn, &Selection::default()).unwrap().documents, 6);
}

#[test]
fn the_csv_names_the_documents_attached_to_each_transaction() {
    let (conn, _, rent, _) = with_documents();
    let rows = read_csv(&csv(&conn, &Selection::default()).unwrap());
    assert_eq!(rows[0].last().map(String::as_str), Some("documents"));
    let cell = |name: &str, date: &str| rows.iter().find(|r| r[3] == name && r[0] == date).unwrap().last().unwrap().clone();
    assert_eq!(cell("Pay", "2026-01-25"), "Pay slip.pdf; Shared scan.pdf", "in the order of the names");
    assert_eq!(cell("Rent", "2026-02-01"), "Lease.pdf");
    assert_eq!(cell("Rent", "2026-03-01"), "", "a transaction with none has an empty cell");
    assert_eq!(cell("Session", "2026-02-10"), "Shared scan.pdf; Therapy invoice.pdf");

    // a limited CSV has only the rows it has, and so only their documents
    let limited = csv(&conn, &Selection { category_ids: Some(vec![rent.to_string()]), ..Default::default() }).unwrap();
    assert!(limited.contains("Lease.pdf") && !limited.contains("Therapy") && !limited.contains("Shared scan") && !limited.contains("Pay slip"), "{limited}");
}

#[test]
fn a_document_name_cannot_run_as_a_formula_in_the_csv() {
    let (conn, ..) = lived_in();
    let doc = add_doc(&conn, "=HYPERLINK(\"http://evil\",\"x\")", "2026-02-01");
    attach_transaction(&conn, doc, "Rent", "2026-02-01");
    let second = add_doc(&conn, "Receipt, \"signed\".pdf", "2026-02-01");
    attach_transaction(&conn, second, "Rent", "2026-02-01");
    let rows = read_csv(&csv(&conn, &Selection::default()).unwrap());
    let cell = rows.iter().find(|r| r[3] == "Rent" && r[0] == "2026-02-01").unwrap().last().unwrap();
    // the names are in the order of the names: the one that starts with an equals sign is first, so the whole cell is made text
    assert_eq!(cell, "'=HYPERLINK(\"http://evil\",\"x\"); Receipt, \"signed\".pdf");
    for row in &rows[1..] {
        assert!(!row.last().unwrap().starts_with(['=', '+', '-', '@', '\t', '\r']));
    }
}

#[test]
fn exporting_documents_changes_nothing_and_never_asks_for_a_file() {
    let (conn, ..) = with_documents();
    let before = crate::backup::dump(&conn).unwrap();
    csv(&conn, &Selection::default()).unwrap();
    json(&conn, &Selection::default(), Utc::now()).unwrap();
    preview(&conn, &Selection::default()).unwrap();
    assert_eq!(crate::backup::dump(&conn).unwrap(), before);
}

#[test]
fn deleting_a_transaction_takes_its_attachment_out_of_the_list_and_keeps_the_document() {
    let (conn, ..) = with_documents();
    conn.execute("DELETE FROM transactions WHERE name = 'Pay'", []).unwrap();
    let file = parsed(&conn, &Selection::default());
    assert!(names(&file).contains(&"Pay slip.pdf".to_string()), "the document stays");
    let slip = file["documents"].as_array().unwrap().iter().find(|d| d["name"] == "Pay slip.pdf").unwrap();
    assert_eq!(slip["links"], serde_json::json!([]));
    // and so it is not part of a limited export
    let limited = parsed(&conn, &Selection { from: Some("2026-01-01".into()), ..Default::default() });
    assert!(!names(&limited).contains(&"Pay slip.pdf".to_string()));
}
