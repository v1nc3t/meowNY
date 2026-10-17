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
fn a_friend_is_made_renamed_given_a_reminder_archived_and_brought_back() {
    let (_dir, vault) = seeded();
    let anna = ok(&vault, "people.create", json!({ "name": " Anna " }));
    assert_eq!((anna["name"].as_str(), anna["balance"].as_str(), anna["archived"].as_bool()), (Some("Anna"), Some("0.00"), Some(false)));
    let anna = id(&anna);
    assert_eq!(refused(&vault, "people.create", json!({ "name": "ANNA" })), (409, "name_exists".into()));
    assert_eq!(refused(&vault, "people.create", json!({ "name": "" })), (INVALID.0, INVALID.1.into()));
    let renamed = ok(&vault, "people.update", json!({ "id": anna, "name": "Anne", "remindOn": "2026-11-01", "remindEveryDays": 14 }));
    assert_eq!((renamed["name"].as_str(), renamed["remindOn"].as_str(), renamed["remindEveryDays"].as_i64()), (Some("Anne"), Some("2026-11-01"), Some(14)));
    // a field left out stays, a null clears
    assert_eq!(ok(&vault, "people.update", json!({ "id": anna, "remindEveryDays": null }))["remindEveryDays"], Value::Null);
    assert_eq!(ok(&vault, "people.update", json!({ "id": anna }))["remindOn"], "2026-11-01");
    assert_eq!(ok(&vault, "people.update", json!({ "id": anna, "remindOn": null }))["remindOn"], Value::Null);
    assert_eq!(refused(&vault, "people.update", json!({ "id": anna, "remindEveryDays": 7 })), (INVALID.0, INVALID.1.into()), "a repeat needs a date");
    assert_eq!(refused(&vault, "people.update", json!({ "id": anna, "remindOn": "2026-11-01", "remindEveryDays": 366 })), (INVALID.0, INVALID.1.into()));
    assert_eq!(refused(&vault, "people.update", json!({ "id": "999" })), (404, "not_found".into()));
    assert_eq!(ok(&vault, "people.archive", json!({ "id": anna }))["archived"], true);
    assert!(ok(&vault, "people.list", json!({})).as_array().unwrap().is_empty());
    assert_eq!(ok(&vault, "people.list", json!({ "archived": true })).as_array().unwrap().len(), 1);
    assert_eq!(ok(&vault, "people.unarchive", json!({ "id": anna }))["archived"], false);
}

#[test]
fn a_transaction_is_shared_and_the_answer_says_what_was_paid_and_what_it_cost() {
    let (_dir, vault) = seeded();
    let dinner = first_expense_id(&vault);
    let (anna, bob) = (id(&ok(&vault, "people.create", json!({ "name": "Anna" }))), id(&ok(&vault, "people.create", json!({ "name": "Bob" }))));
    let shared = ok(&vault, "transactions.split", json!({ "id": dinner, "mode": "EQUAL", "people": [anna, bob], "guests": "Carl, Dina" }));
    assert_eq!((shared["amount"].as_str(), shared["myShare"].as_str(), shared["split"].as_array().unwrap().len()), (Some("90.00"), Some("18.00"), 4));
    assert_eq!(shared["split"][2]["guest"], "Carl");
    // the listing says the same, and so do the friends' balances
    let listed = ok(&vault, "transactions.list", json!({ "q": "Dinner" }))["transactions"][0].clone();
    assert_eq!(listed["myShare"], "18.00");
    assert_eq!(ok(&vault, "people.list", json!({}))[0]["balance"], "18.00");
    // the numbers are typed in the way the mode says
    let by_percent = ok(&vault, "transactions.split", json!({ "id": dinner, "mode": "PERCENT", "people": [anna, bob], "values": "30.00,20.00" }));
    assert_eq!(by_percent["myShare"], "45.00", "the rest is yours");
    let by_amount = ok(&vault, "transactions.split", json!({ "id": dinner, "mode": "EXACT", "mine": "10.00", "people": [anna], "guests": "Carl", "values": "60.00,20.00" }));
    assert_eq!(by_amount["myShare"], "10.00");
    let by_shares = ok(&vault, "transactions.split", json!({ "id": dinner, "mode": "SHARES", "mine": "1", "people": [anna], "values": "2" }));
    assert_eq!(by_shares["split"][0]["amount"], "60.00");
    let back = ok(&vault, "transactions.unsplit", json!({ "id": dinner }));
    assert_eq!((back["myShare"].as_str(), back["split"].as_array().map(Vec::len)), (Some("90.00"), Some(0)));
    assert_eq!(ok(&vault, "people.list", json!({}))[0]["balance"], "0.00");
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
fn the_preview_saves_nothing_and_says_what_is_left() {
    let (_dir, vault) = seeded();
    let anna = id(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let before = vault.with_conn(|c| crate::backup::dump(c)).unwrap();
    let shown = ok(&vault, "splits.preview", json!({ "amount": "100.00", "mode": "EXACT", "mine": "10.00", "people": [anna], "values": "30.00" }));
    assert_eq!((shown["complete"].as_bool(), shown["left"].as_str(), shown["mine"].as_str()), (Some(false), Some("60.00"), Some("10.00")));
    let shown = ok(&vault, "splits.preview", json!({ "amount": "100.00", "mode": "EQUAL", "people": [anna], "guests": "Bob" }));
    assert_eq!((shown["complete"].as_bool(), shown["mine"].as_str(), shown["parts"][1]["guest"].as_str(), shown["parts"][1]["amount"].as_str()), (Some(true), Some("33.34"), Some("Bob"), Some("33.33")));
    assert_eq!(refused(&vault, "splits.preview", json!({ "amount": "0.00", "mode": "EQUAL", "people": [anna] })).0, 400);
    assert_eq!(vault.with_conn(|c| crate::backup::dump(c)).unwrap(), before);
}

#[test]
fn the_ledger_is_added_changed_and_deleted_and_marking_paid_turns_the_balance_around() {
    let (_dir, vault) = seeded();
    let anna = id(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let loan = ok(&vault, "ledger.add", json!({ "personId": anna, "kind": "LENT", "amount": "25.00", "date": "2026-10-01", "note": "taxi" }));
    assert_eq!((loan["kind"].as_str(), loan["amount"].as_str(), loan["note"].as_str()), (Some("LENT"), Some("25.00"), Some("taxi")));
    assert_eq!(ok(&vault, "people.show", json!({ "id": anna }))["person"]["balance"], "25.00");
    let changed = ok(&vault, "ledger.update", json!({ "id": id(&loan), "amount": "30.00", "note": null }));
    assert_eq!((changed["amount"].as_str(), changed["note"].clone()), (Some("30.00"), Value::Null));
    let paid = ok(&vault, "people.settle", json!({ "id": anna, "amount": "40.00", "date": "2026-10-10" }));
    assert_eq!((paid["before"].as_str(), paid["after"].as_str(), paid["over"].as_str(), paid["entry"]["kind"].as_str()), (Some("30.00"), Some("-10.00"), Some("10.00"), Some("REPAID_TO_ME")));
    let story = ok(&vault, "people.show", json!({ "id": anna }));
    assert_eq!(story["lines"].as_array().unwrap().len(), 2);
    assert_eq!(story["lines"][1]["balance"], "-10.00");
    assert_eq!(ok(&vault, "debts.summary", json!({}))["youOwe"], "10.00");
    assert_eq!(refused(&vault, "ledger.add", json!({ "personId": anna, "kind": "GIFT", "amount": "1.00", "date": "2026-10-01" })).0, 400);
    assert_eq!(refused(&vault, "ledger.add", json!({ "personId": "999", "kind": "LENT", "amount": "1.00", "date": "2026-10-01" })).0, 400);
    assert_eq!(refused(&vault, "ledger.update", json!({ "id": "999", "amount": "1.00" })), (404, "not_found".into()));
    assert!(ok(&vault, "ledger.delete", json!({ "id": id(&loan) })).is_null());
    assert_eq!(refused(&vault, "ledger.delete", json!({ "id": id(&loan) })), (404, "not_found".into()));
    let summary = ok(&vault, "debts.summary", json!({}));
    assert_eq!((summary["owedToYou"].as_str(), summary["youOwe"].as_str()), (Some("0.00"), Some("40.00")), "the repayment is still there, so you owe Anna");
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

#[test]
fn a_recurring_item_gets_a_template_and_its_payments_are_shared() {
    let (_dir, vault) = seeded();
    let anna = id(&ok(&vault, "people.create", json!({ "name": "Anna" })));
    let category = ok(&vault, "categories.list", json!({ "type": "EXPENSE" }))[0]["id"].as_str().unwrap().to_string();
    let item = id(&ok(&vault, "recurring.create", json!({ "name": "Flat", "amount": "900.00", "categoryId": category, "startDate": "2026-11-01", "intervalUnit": "MONTH", "intervalCount": 1 })));
    let templated = ok(&vault, "recurring.split", json!({ "id": item, "mode": "SHARES", "mine": "2", "people": [anna], "values": "1" }));
    assert_eq!((templated["split"]["mine"].as_u64(), templated["split"]["parts"][0]["weight"].as_u64(), templated["split"]["parts"][0]["person"].as_str()), (Some(2), Some(1), Some("Anna")));
    assert_eq!(ok(&vault, "recurring.list", json!({})).as_array().unwrap().iter().find(|i| i["id"] == json!(item)).unwrap()["split"]["mine"], 2);
    let paid = ok(&vault, "recurring.pay", json!({ "id": item, "paidOn": "2026-11-01" }));
    assert_eq!((paid["myShare"].as_str(), paid["split"][0]["amount"].as_str()), (Some("600.00"), Some("300.00")));
    assert!(ok(&vault, "recurring.unsplit", json!({ "id": item }))["split"].is_null());
    assert_eq!(refused(&vault, "recurring.split", json!({ "id": item, "mode": "EXACT", "people": [anna], "values": "901.00" })).0, 400, "exact amounts must add up to the item's amount");
    assert_eq!(refused(&vault, "recurring.split", json!({ "id": "99999", "mode": "EQUAL", "people": [anna] })), (404, "not_found".into()));
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
    assert_eq!(r.asked(), 3);
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
