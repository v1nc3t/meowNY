//! The routes for friends, the ledger and shared bills: what a token may do, the rules the screens follow, and the one file that needs the person.

use serde_json::{json, Value};

use super::confirmed_tests::{denied, rig};
use super::read_tests::{ok, refused, seeded};

const INVALID: (u16, &str) = (400, "invalid_input");

fn id(value: &Value) -> String {
    value["id"].as_str().unwrap().to_string()
}

fn first_expense_id(vault: &crate::vault::Vault) -> String {
    let category = ok(vault, "categories.list", json!({ "type": "EXPENSE" }))[0]["id"].as_str().unwrap().to_string();
    id(&ok(vault, "transactions.create", json!({ "name": "Dinner", "amount": "90.00", "paymentDate": "2026-10-14", "categoryId": category })))
}

#[test]
fn a_split_that_makes_no_sense_is_refused_with_a_reason_and_changes_nothing() {
    let (_dir, vault) = seeded();
    let dinner = first_expense_id(&vault);
    let anna = id(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let before = vault.with_conn(|c| crate::backup::dump(c)).unwrap();
    for body in [
        json!({ "id": dinner, "mode": "EQUAL" }),
        json!({ "id": dinner, "mode": "EQUAL", "people": [anna], "values": "1" }),
        json!({ "id": dinner, "mode": "PERCENT", "people": [anna] }),
        json!({ "id": dinner, "mode": "PERCENT", "people": [anna], "values": "101.00" }),
        json!({ "id": dinner, "mode": "PERCENT", "mine": "50.00", "people": [anna], "values": "49.00" }),
        json!({ "id": dinner, "mode": "EXACT", "mine": "1.00", "people": [anna], "values": "10.00" }),
        json!({ "id": dinner, "mode": "SHARES", "people": [anna], "values": "1" }),
        json!({ "id": dinner, "mode": "SHARES", "mine": "x", "people": [anna], "values": "1" }),
        json!({ "id": dinner, "mode": "EQUAL", "people": [anna, anna] }),
        json!({ "id": dinner, "mode": "EQUAL", "people": ["999"] }),
        json!({ "id": dinner, "mode": "EQUAL", "guests": "A,,B" }),
        json!({ "id": dinner, "mode": "EQUAL", "guests": ",".repeat(40) }),
        json!({ "id": dinner, "mode": "PERCENT", "people": [anna], "guests": "Bob", "values": "10.00" }),
    ] {
        let (status, code) = refused(&vault, "transactions.split", body.clone());
        assert_eq!((status, code.as_str()), INVALID, "{body}");
    }
    assert_eq!(refused(&vault, "transactions.split", json!({ "id": "99999999", "mode": "EQUAL", "people": [anna] })), (404, "not_found".into()));
    assert_eq!(refused(&vault, "transactions.split", json!({ "id": dinner, "mode": "equal", "people": [anna] })).0, 400, "the mode is one of the listed words");
    assert_eq!(vault.with_conn(|c| crate::backup::dump(c)).unwrap(), before);
    // an income is not shared
    let income = ok(&vault, "categories.list", json!({ "type": "INCOME" }))[0]["id"].as_str().unwrap().to_string();
    let pay = id(&ok(&vault, "transactions.create", json!({ "name": "Pay", "amount": "10.00", "paymentDate": "2026-10-14", "categoryId": income })));
    assert_eq!((refused(&vault, "transactions.split", json!({ "id": pay, "mode": "EQUAL", "people": [anna] })).0), 400);
}

#[test]
fn a_loan_is_never_income_or_expense_and_a_borrowing_can_add_your_share_to_the_expenses() {
    let (_dir, vault) = seeded();
    let anna = id(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let category = ok(&vault, "categories.list", json!({ "type": "EXPENSE" }))[0]["id"].as_str().unwrap().to_string();
    let before = ok(&vault, "export.preview", json!({}));
    ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "500.00", "date": "2026-10-01" }));
    ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "REPAID_TO_ME", "amount": "100.00", "date": "2026-10-02" }));
    let after = ok(&vault, "export.preview", json!({}));
    assert_eq!((&before["income"], &before["expense"], &before["transactions"]), (&after["income"], &after["expense"], &after["transactions"]), "nothing in the ledger is a transaction");
    let borrowed = ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "BORROWED", "amount": "12.00", "date": "2026-10-03", "expenseName": "Pizza", "expenseCategoryId": category }));
    assert!(borrowed["transactionId"].is_string());
    assert_eq!(ok(&vault, "export.preview", json!({}))["transactions"].as_u64().unwrap(), before["transactions"].as_u64().unwrap() + 1);
    for body in [
        json!({ "personId": anna, "kind": "BORROWED", "amount": "1.00", "date": "2026-10-03", "expenseName": "Pizza" }),
        json!({ "personId": anna, "kind": "BORROWED", "amount": "1.00", "date": "2026-10-03", "expenseCategoryId": category }),
        json!({ "personId": anna, "kind": "LENT", "amount": "1.00", "date": "2026-10-03", "expenseName": "Pizza", "expenseCategoryId": category }),
        json!({ "personId": anna, "kind": "BORROWED", "amount": "1.00", "date": "2026-10-03", "expenseName": "Pizza", "expenseCategoryId": category, "transactionId": "1" }),
        json!({ "personId": anna, "kind": "BORROWED", "amount": "1.00", "date": "2026-10-03", "expenseName": "", "expenseCategoryId": category }),
    ] {
        assert_eq!(refused(&vault, "ledger.add", body.clone()).0, 400, "{body}");
    }
    assert_eq!(ok(&vault, "export.preview", json!({}))["transactions"].as_u64().unwrap(), before["transactions"].as_u64().unwrap() + 1, "nothing was made by the refused ones");
}

// ---- the file ---------------------------------------------------------------------------------

#[test]
fn who_owes_what_is_saved_only_when_the_person_says_yes_and_the_dialog_says_what_it_is() {
    let r = rig(true);
    let anna = id(&r.post("people.create", json!({ "name": "Anna" })).data());
    r.post("ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "25.00", "date": "2026-10-01", "note": "taxi" }));
    let summary = r.path("owed.txt");
    assert_eq!(r.post("export.debts", json!({ "out": summary })).status, 200);
    assert!(r.last().contains("who owes what") && r.last().contains("names of your friends") && r.last().contains("not encrypted"), "{}", r.last());
    let text = std::fs::read_to_string(&summary).unwrap();
    assert!(text.starts_with("Who owes what\n") && text.contains("Anna") && text.contains("25.00"), "{text}");

    let statement = r.path("anna.csv");
    assert_eq!(r.post("export.debts", json!({ "out": statement, "what": "STATEMENT", "format": "CSV", "personId": anna })).status, 200);
    assert!(r.last().contains("the statement of Anna"), "{}", r.last());
    assert!(std::fs::read_to_string(&statement).unwrap().contains("2026-10-01,LENT,taxi,25.00,25.00,25.00"));

    let ledger = r.path("ledger.csv");
    assert_eq!(r.post("export.debts", json!({ "out": ledger, "what": "LEDGER" })).status, 200);
    assert!(r.last().contains("every entry of the ledger"));
    assert!(std::fs::read_to_string(&ledger).unwrap().starts_with("\u{feff}date,friend,kind,amount,currency,note"));

    // the same as PDF files, in English; the ending must be .pdf
    let (pdf_summary, pdf_statement) = (r.path("owed.pdf"), r.path("anna.pdf"));
    assert_eq!(r.post("export.debts", json!({ "out": pdf_summary, "format": "PDF" })).status, 200);
    let statement_reply = r.post("export.debts", json!({ "out": pdf_statement, "what": "STATEMENT", "format": "PDF", "personId": anna }));
    assert_eq!(statement_reply.status, 200, "{}", statement_reply.body);
    for path in [&pdf_summary, &pdf_statement] {
        assert!(std::fs::read(path).unwrap().starts_with(b"%PDF-"), "{path}");
    }
    assert_eq!(r.post("export.debts", json!({ "out": r.path("wrong.txt"), "format": "PDF" })).status, 400, "the ending must match the format");
    assert_eq!(r.asked(), 5);
}

#[test]
fn a_request_that_could_never_work_does_not_ask_the_person_and_a_no_writes_nothing() {
    let r = rig(true);
    let anna = id(&r.post("people.create", json!({ "name": "Anna" })).data());
    for (body, what) in [
        (json!({ "out": r.path("a.txt"), "what": "STATEMENT" }), "a statement needs a friend"),
        (json!({ "out": r.path("b.txt"), "personId": anna }), "a summary has no friend"),
        (json!({ "out": r.path("c.txt"), "what": "LEDGER" }), "the ledger is CSV only"),
        (json!({ "out": r.path("d.csv"), "what": "STATEMENT", "format": "CSV", "personId": "999" }), "no such friend"),
        (json!({ "out": r.path("e.csv") }), "the ending must match the format"),
        (json!({ "out": "relative.txt" }), "the path must be absolute"),
    ] {
        let reply = r.post("export.debts", body);
        assert!(reply.status == 400 || reply.status == 404, "{what}: {} {}", reply.status, reply.body);
    }
    assert_eq!(r.asked(), 0);
    assert!(r.written().is_empty());
    let no = rig(false);
    let message = denied(&no.post("export.debts", json!({ "out": no.path("owed.txt") })));
    assert!(message.contains("did not allow"), "{message}");
    assert!(no.written().is_empty());
}

#[test]
fn the_history_the_overdue_payments_and_the_reminders_are_read_within_their_limits() {
    let (_dir, vault) = seeded();
    let category = ok(&vault, "categories.list", json!({ "type": "EXPENSE" }))[0]["id"].as_str().unwrap().to_string();
    let anna = id(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "20.00", "date": "2020-01-01" }));
    // an item forgotten for years: its oldest 24 occurrences are overdue, the oldest first, and none is today or later
    ok(&vault, "recurring.create", json!({ "name": "Old gym", "amount": "10.00", "categoryId": category, "startDate": "2020-01-01", "intervalUnit": "MONTH", "intervalCount": 1 }));
    let due = ok(&vault, "debts.due", json!({}));
    let mine: Vec<&Value> = due["payments"].as_array().unwrap().iter().filter(|p| p["name"] == "Old gym").collect();
    assert_eq!(mine.len(), 24, "the most an item shows");
    assert_eq!(mine[0]["dueDate"], "2020-01-01");
    assert!(mine.iter().all(|p| p["daysOverdue"].as_u64().unwrap() >= 1), "a payment is overdue from the day after its day");

    // the history is paged, newest first, and a page that cannot be asked for is refused
    let page = ok(&vault, "debts.history", json!({ "limit": 2 }));
    assert_eq!((page["lines"].as_array().unwrap().len(), page["limit"].as_u64()), (2, Some(2)));
    assert!(page["total"].as_u64().unwrap() > 25);
    let only_anna = ok(&vault, "debts.history", json!({ "personId": anna }));
    assert_eq!((only_anna["total"].as_u64(), only_anna["lines"][0]["friend"].as_str()), (Some(1), Some("Anna")), "a friend's lines have no recurring payments");
    for body in [json!({ "limit": 0 }), json!({ "limit": 201 }), json!({ "offset": -1 }), json!({ "from": "2026-02-01", "to": "2026-01-01" }), json!({ "personId": "abc" })] {
        assert_eq!(refused(&vault, "debts.history", body.clone()).0, 400, "{body}");
    }

    // a reminder is due a period after the debt last changed, and the period has its limits
    let r = ok(&vault, "debts.reminders", json!({ "everyDays": 30 }));
    assert_eq!(r["due"][0]["name"], "Anna");
    assert_eq!(ok(&vault, "debts.reminders", json!({ "everyDays": 0 }))["due"], json!([]));
    for body in [json!({}), json!({ "everyDays": 366 }), json!({ "everyDays": -1 }), json!({ "everyDays": "7" })] {
        assert_eq!(refused(&vault, "debts.reminders", body.clone()).0, 400, "{body}");
    }
}
