//! The database itself must refuse bad data about friends, the ledger and shared bills, whatever the code around it does (migration 0012).

use rusqlite::{params, Connection};

use super::fixtures::*;

fn exec(conn: &Connection, sql: &str) -> rusqlite::Result<usize> {
    conn.execute(sql, [])
}

fn refused(conn: &Connection, sql: &str) -> String {
    conn.execute(sql, []).expect_err(sql).to_string()
}

fn audit(conn: &Connection, entity: &str, action: &str) -> i64 {
    conn.query_row("SELECT count(*) FROM audit_log WHERE entity_type = ?1 AND action = ?2", params![entity, action], |r| r.get(0)).unwrap()
}

fn stamp_of(conn: &Connection, table: &str, id: i64) -> String {
    text(conn, &format!("SELECT stamp FROM {table} WHERE id = {id}"))
}

fn updated_at_of(conn: &Connection, table: &str, id: i64) -> String {
    text(conn, &format!("SELECT updated_at FROM {table} WHERE id = {id}"))
}

fn applying(conn: &Connection, on: bool) {
    conn.execute("UPDATE sync_clock SET applying = ?1", [on]).unwrap();
}

// ---- people ----------------------------------------------------------------------------------

#[test]
fn a_person_has_a_name_of_one_to_fifty_characters() {
    let w = world();
    assert!(exec(&w.conn, "INSERT INTO people (name) VALUES ('')").is_err());
    assert!(exec(&w.conn, &format!("INSERT INTO people (name) VALUES ('{}')", "x".repeat(51))).is_err());
    assert!(exec(&w.conn, &format!("INSERT INTO people (name) VALUES ('{}')", "x".repeat(50))).is_ok());
    assert!(exec(&w.conn, "INSERT INTO people (name) VALUES ('Zoë 日本語')").is_ok());
}

#[test]
fn names_are_unique_among_the_people_who_are_not_archived_ignoring_case() {
    let w = world();
    exec(&w.conn, "INSERT INTO people (name) VALUES ('Anna')").unwrap();
    assert!(exec(&w.conn, "INSERT INTO people (name) VALUES ('Anna')").is_err());
    assert!(exec(&w.conn, "INSERT INTO people (name) VALUES ('anna')").is_err());
    assert!(exec(&w.conn, "INSERT INTO people (name) VALUES ('ANNA')").is_err());
    exec(&w.conn, "UPDATE people SET archived = 1 WHERE name = 'Anna'").unwrap();
    assert!(exec(&w.conn, "INSERT INTO people (name) VALUES ('Anna')").is_ok(), "an archived friend frees the name");
    assert!(exec(&w.conn, "UPDATE people SET archived = 0 WHERE id = 1").is_err(), "and cannot come back while another has it");
}

#[test]
fn a_reminder_is_a_real_date_and_a_repeat_needs_one() {
    let w = world();
    exec(&w.conn, "INSERT INTO people (name) VALUES ('Anna')").unwrap();
    for bad in ["2026-02-30", "2026-2-3", "tomorrow", "20260101", "2026-13-01"] {
        assert!(exec(&w.conn, &format!("UPDATE people SET remind_on = '{bad}' WHERE id = 1")).is_err(), "{bad}");
    }
    assert!(exec(&w.conn, "UPDATE people SET remind_every_days = 7 WHERE id = 1").is_err(), "a repeat needs a date");
    assert!(exec(&w.conn, "UPDATE people SET remind_on = '2026-03-01', remind_every_days = 0 WHERE id = 1").is_err());
    assert!(exec(&w.conn, "UPDATE people SET remind_on = '2026-03-01', remind_every_days = 366 WHERE id = 1").is_err());
    assert!(exec(&w.conn, "UPDATE people SET remind_on = '2026-03-01', remind_every_days = 365 WHERE id = 1").is_ok());
    assert!(exec(&w.conn, "UPDATE people SET remind_on = NULL WHERE id = 1").is_err(), "the repeat would be left without a date");
    assert!(exec(&w.conn, "UPDATE people SET remind_on = NULL, remind_every_days = NULL WHERE id = 1").is_ok());
}

#[test]
fn a_person_is_archived_and_never_deleted() {
    let w = world();
    exec(&w.conn, "INSERT INTO people (name) VALUES ('Anna')").unwrap();
    let error = refused(&w.conn, "DELETE FROM people WHERE id = 1");
    assert!(error.contains("archived, never deleted"), "{error}");
    // only a restore or a sync, which turn `applying` on, can take rows away
    applying(&w.conn, true);
    assert_eq!(exec(&w.conn, "DELETE FROM people WHERE id = 1").unwrap(), 1);
}

#[test]
fn a_sync_or_a_restore_writing_a_person_keeps_the_stamp_it_brings() {
    let w = world();
    applying(&w.conn, true);
    let theirs = "1760000000000-0001-aaaaaaaa";
    w.conn.execute("INSERT INTO people (name, stamp) VALUES ('Anna', ?1)", [theirs]).unwrap();
    assert_eq!(stamp_of(&w.conn, "people", 1), theirs);
    exec(&w.conn, "UPDATE people SET name = 'Anne' WHERE id = 1").unwrap();
    assert_eq!(stamp_of(&w.conn, "people", 1), theirs, "no new stamp while applying");
}

// ---- the ledger ------------------------------------------------------------------------------

fn entry(conn: &Connection, extra: &str) -> rusqlite::Result<usize> {
    conn.execute(&format!("INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date{extra}"), [])
}

#[test]
fn a_ledger_entry_has_a_known_kind_an_amount_in_range_and_a_real_date() {
    let w = world();
    friend(&w.conn, "Anna");
    assert!(entry(&w.conn, ") VALUES (1, 'LENT', 100, '2026-01-01')").is_ok());
    for kind in ["BORROWED", "REPAID_TO_ME", "REPAID_BY_ME"] {
        assert!(entry(&w.conn, &format!(") VALUES (1, '{kind}', 100, '2026-01-01')")).is_ok(), "{kind}");
    }
    for bad in [
        ") VALUES (1, 'GIFT', 100, '2026-01-01')",
        ") VALUES (1, 'lent', 100, '2026-01-01')",
        ") VALUES (1, 'LENT', 0, '2026-01-01')",
        ") VALUES (1, 'LENT', -5, '2026-01-01')",
        ") VALUES (1, 'LENT', 1000000000000, '2026-01-01')",
        ") VALUES (1, 'LENT', 100, '2026-02-30')",
        ") VALUES (1, 'LENT', 100, '2026-2-3')",
        ") VALUES (99, 'LENT', 100, '2026-01-01')",
    ] {
        assert!(entry(&w.conn, bad).is_err(), "{bad}");
    }
    assert!(entry(&w.conn, ") VALUES (1, 'LENT', 999999999999, '2026-01-01')").is_ok());
}

#[test]
fn a_note_is_one_to_two_hundred_characters_or_none() {
    let w = world();
    friend(&w.conn, "Anna");
    assert!(entry(&w.conn, ", note) VALUES (1, 'LENT', 100, '2026-01-01', NULL)").is_ok());
    assert!(entry(&w.conn, ", note) VALUES (1, 'LENT', 100, '2026-01-01', '')").is_err(), "empty is none, which is NULL");
    assert!(entry(&w.conn, &format!(", note) VALUES (1, 'LENT', 100, '2026-01-01', '{}')", "n".repeat(200))).is_ok());
    assert!(entry(&w.conn, &format!(", note) VALUES (1, 'LENT', 100, '2026-01-01', '{}')", "n".repeat(201))).is_err());
}

#[test]
fn a_bill_code_is_accepted_once() {
    let w = world();
    friend(&w.conn, "Anna");
    let id = "ab".repeat(16);
    assert!(entry(&w.conn, &format!(", origin_id) VALUES (1, 'BORROWED', 100, '2026-01-01', '{id}')")).is_ok());
    assert!(entry(&w.conn, &format!(", origin_id) VALUES (1, 'BORROWED', 100, '2026-01-01', '{id}')")).is_err());
    assert!(entry(&w.conn, ", origin_id) VALUES (1, 'BORROWED', 100, '2026-01-01', 'not hex')").is_err());
    assert!(entry(&w.conn, ", origin_id) VALUES (1, 'BORROWED', 100, '2026-01-01', NULL)").is_ok(), "entries typed by hand have none");
    assert!(entry(&w.conn, ", origin_id) VALUES (1, 'BORROWED', 100, '2026-01-01', NULL)").is_ok());
    // deleting the entry frees the code for a corrected one
    exec(&w.conn, "DELETE FROM ledger_entries WHERE origin_id IS NOT NULL").unwrap();
    assert!(entry(&w.conn, &format!(", origin_id) VALUES (1, 'BORROWED', 100, '2026-01-01', '{id}')")).is_ok());
}

#[test]
fn deleting_the_expense_clears_the_link_and_nothing_else_of_the_entry() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "40.00", "2026-01-01");
    w.conn.execute("INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date, transaction_id) VALUES (?1, 'BORROWED', 1000, '2026-01-01', ?2)", [anna, dinner]).unwrap();
    let (stamp, updated) = (stamp_of(&w.conn, "ledger_entries", 1), updated_at_of(&w.conn, "ledger_entries", 1));
    let audited = audit(&w.conn, "ledger_entries", "UPDATE");
    exec(&w.conn, &format!("DELETE FROM transactions WHERE id = {dinner}")).unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM ledger_entries WHERE transaction_id IS NULL"), 1, "the debt outlives the expense");
    let marker = text(&w.conn, "SELECT stamp FROM sync_deleted WHERE kind = 'transactions'");
    assert_eq!(stamp_of(&w.conn, "ledger_entries", 1), marker, "the entry takes the stamp of the deletion (migration 0013), one tick for both");
    assert!(marker > stamp);
    assert_eq!(updated_at_of(&w.conn, "ledger_entries", 1), updated, "but it is not an edit of the entry");
    assert_eq!(audit(&w.conn, "ledger_entries", "UPDATE"), audited + 1, "but the history shows it");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'ledger_entries'"), 0);
}

#[test]
fn an_edit_of_an_entry_gets_a_new_stamp_and_so_does_a_change_of_its_link_alone() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "40.00", "2026-01-01");
    w.conn.execute("INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date) VALUES (?1, 'LENT', 1000, '2026-01-01')", [anna]).unwrap();
    let first = stamp_of(&w.conn, "ledger_entries", 1);
    exec(&w.conn, "UPDATE ledger_entries SET amount_minor = 1000 WHERE id = 1").unwrap();
    assert_eq!(stamp_of(&w.conn, "ledger_entries", 1), first, "no change, no stamp");
    exec(&w.conn, "UPDATE ledger_entries SET amount_minor = 1200 WHERE id = 1").unwrap();
    let second = stamp_of(&w.conn, "ledger_entries", 1);
    assert!(second > first);
    // the code that edits an entry writes every column, so linking it to an expense is an edit
    w.conn.execute("UPDATE ledger_entries SET person_id = person_id, transaction_id = ?1 WHERE id = 1", [dinner]).unwrap();
    assert!(stamp_of(&w.conn, "ledger_entries", 1) > second);
}

#[test]
fn deleting_an_entry_leaves_a_marker_and_a_restore_does_not() {
    let w = world();
    friend(&w.conn, "Anna");
    entry(&w.conn, ") VALUES (1, 'LENT', 100, '2026-01-01')").unwrap();
    let sync_id = text(&w.conn, "SELECT sync_id FROM ledger_entries");
    exec(&w.conn, "DELETE FROM ledger_entries").unwrap();
    assert_eq!(text(&w.conn, "SELECT sync_id FROM sync_deleted WHERE kind = 'ledger_entries'"), sync_id);
    entry(&w.conn, ") VALUES (1, 'LENT', 100, '2026-01-01')").unwrap();
    applying(&w.conn, true);
    exec(&w.conn, "DELETE FROM ledger_entries").unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'ledger_entries'"), 1, "no marker while a sync or a restore writes");
}

#[test]
fn a_friend_with_entries_cannot_be_removed_even_by_a_sync_that_forgot_the_entries() {
    let w = world();
    friend(&w.conn, "Anna");
    entry(&w.conn, ") VALUES (1, 'LENT', 100, '2026-01-01')").unwrap();
    applying(&w.conn, true);
    assert!(exec(&w.conn, "DELETE FROM people WHERE id = 1").is_err(), "the foreign key holds");
}

// ---- the parts of a bill ---------------------------------------------------------------------

fn person_line(conn: &Connection, transaction: i64, person: i64, amount: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO transaction_split_lines (transaction_id, person_id, amount_minor) VALUES (?1, ?2, ?3)",
        params![transaction, person, amount],
    )
}

fn guest_line(conn: &Connection, transaction: i64, guest: &str, amount: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO transaction_split_lines (transaction_id, guest_name, amount_minor) VALUES (?1, ?2, ?3)",
        params![transaction, guest, amount],
    )
}

#[test]
fn a_part_belongs_to_a_friend_or_a_guest_never_both_and_never_neither() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "40.00", "2026-01-01");
    assert!(w.conn.execute("INSERT INTO transaction_split_lines (transaction_id, amount_minor) VALUES (?1, 100)", [dinner]).is_err());
    assert!(w.conn.execute("INSERT INTO transaction_split_lines (transaction_id, person_id, guest_name, amount_minor) VALUES (?1, ?2, 'Bob', 100)", [dinner, anna]).is_err());
    assert!(person_line(&w.conn, dinner, anna, 100).is_ok());
    assert!(guest_line(&w.conn, dinner, "Bob", 100).is_ok());
    assert!(guest_line(&w.conn, dinner, "", 100).is_err());
    assert!(guest_line(&w.conn, dinner, &"g".repeat(51), 100).is_err());
    assert!(person_line(&w.conn, dinner, 99, 100).is_err(), "the friend must exist");
    assert!(person_line(&w.conn, 999, anna, 100).is_err(), "and so must the bill");
}

#[test]
fn a_part_is_an_amount_in_range_and_a_person_or_guest_is_on_a_bill_once() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "40.00", "2026-01-01");
    assert!(person_line(&w.conn, dinner, anna, 0).is_err());
    assert!(person_line(&w.conn, dinner, anna, -1).is_err());
    assert!(person_line(&w.conn, dinner, anna, 1000).is_ok());
    assert!(person_line(&w.conn, dinner, anna, 1000).is_err(), "once");
    assert!(guest_line(&w.conn, dinner, "Bob", 1000).is_ok());
    assert!(guest_line(&w.conn, dinner, "bob", 1000).is_err(), "a guest name is once too, ignoring case");
    let other = bill(&w.conn, w.food, "Lunch", "40.00", "2026-01-02");
    assert!(person_line(&w.conn, other, anna, 1000).is_ok(), "on another bill it is fine");
}

#[test]
fn the_parts_of_a_bill_are_never_more_than_the_bill() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let dinner = bill(&w.conn, w.food, "Dinner", "40.00", "2026-01-01");
    assert!(person_line(&w.conn, dinner, anna, 4001).is_err(), "more than the bill");
    assert!(person_line(&w.conn, dinner, anna, 4000).is_ok(), "all of it is allowed: you paid for them");
    assert!(person_line(&w.conn, dinner, bob, 1).is_err(), "together more than the bill");
    exec(&w.conn, &format!("UPDATE transaction_split_lines SET amount_minor = 1000 WHERE transaction_id = {dinner}")).unwrap();
    assert!(person_line(&w.conn, dinner, bob, 3000).is_ok());
    assert!(exec(&w.conn, "UPDATE transaction_split_lines SET amount_minor = 1001 WHERE person_id = 1").is_err(), "a part cannot grow past the bill");
    assert!(exec(&w.conn, "UPDATE transactions SET amount_minor = 3999").is_err(), "the bill cannot shrink below its parts");
    assert!(exec(&w.conn, "UPDATE transactions SET amount_minor = 4000").is_ok());
    assert!(exec(&w.conn, "UPDATE transactions SET amount_minor = 5000").is_ok(), "or grow");
}

#[test]
fn only_an_expense_is_split_and_a_split_bill_stays_one() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let pay = bill(&w.conn, w.salary, "Pay", "3000.00", "2026-01-25");
    let error = person_line(&w.conn, pay, anna, 100).unwrap_err().to_string();
    assert!(error.contains("only an expense"), "{error}");
    let dinner = bill(&w.conn, w.food, "Dinner", "40.00", "2026-01-01");
    person_line(&w.conn, dinner, anna, 100).unwrap();
    let error = w.conn.execute("UPDATE transactions SET category_id = ?1 WHERE id = ?2", [w.salary, dinner]).unwrap_err().to_string();
    assert!(error.contains("only an expense"), "{error}");
    assert!(w.conn.execute("UPDATE transactions SET category_id = ?1 WHERE id = ?2", [w.rent, dinner]).is_ok(), "another expense category is fine");
}

#[test]
fn a_bill_has_at_most_thirty_parts() {
    let w = world();
    let dinner = bill(&w.conn, w.food, "Party", "300.00", "2026-01-01");
    for i in 0..30 {
        guest_line(&w.conn, dinner, &format!("Guest {i}"), 100).unwrap();
    }
    let error = guest_line(&w.conn, dinner, "One too many", 100).unwrap_err().to_string();
    assert!(error.contains("at most 30"), "{error}");
}

#[test]
fn deleting_a_bill_takes_its_parts_and_leaves_one_marker() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "40.00", "2026-01-01");
    person_line(&w.conn, dinner, anna, 100).unwrap();
    guest_line(&w.conn, dinner, "Bob", 100).unwrap();
    exec(&w.conn, &format!("DELETE FROM transactions WHERE id = {dinner}")).unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transaction_split_lines"), 0);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'transactions'"), 1);
    assert_eq!(one(&w.conn, "SELECT balance_minor FROM person_balances WHERE person_id = 1"), 0, "their balance goes down by their part");
}

#[test]
fn a_sync_writing_parts_leaves_the_stamp_of_the_bill_alone() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "40.00", "2026-01-01");
    let before = (stamp_of(&w.conn, "transactions", dinner), updated_at_of(&w.conn, "transactions", dinner));
    applying(&w.conn, true);
    person_line(&w.conn, dinner, anna, 9999).unwrap();
    assert_eq!((stamp_of(&w.conn, "transactions", dinner), updated_at_of(&w.conn, "transactions", dinner)), before);
    // and the checks stand aside, which is why the finished state is looked at afterwards
    assert!(!super::splits::check(&w.conn).unwrap(), "parts above the bill are found");
}

// ---- the template of a recurring item --------------------------------------------------------

fn item(conn: &Connection, category: i64) -> i64 {
    conn.execute(
        "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date) VALUES (?1, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-01-01')",
        [category],
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[test]
fn a_template_has_a_weight_for_the_payer_and_lines_with_weights_above_zero() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let rent = item(&w.conn, w.rent);
    let add = |weight: i64| w.conn.execute("INSERT INTO recurring_split_lines (recurring_transaction_id, person_id, weight) VALUES (?1, ?2, ?3)", params![rent, anna, weight]);
    assert!(add(1).is_err(), "no weight for the payer yet");
    exec(&w.conn, "UPDATE recurring_transactions SET split_my_weight = -1").unwrap_err();
    exec(&w.conn, "UPDATE recurring_transactions SET split_my_weight = 1000000000000").unwrap_err();
    exec(&w.conn, "UPDATE recurring_transactions SET split_my_weight = 0").unwrap();
    assert!(add(0).is_err(), "a friend with 0 is not on the template");
    assert!(add(1).is_ok());
    assert!(add(1).is_err(), "once");
    assert!(exec(&w.conn, "UPDATE recurring_transactions SET split_my_weight = NULL").is_err(), "the lines go first");
    exec(&w.conn, "DELETE FROM recurring_split_lines").unwrap();
    assert!(exec(&w.conn, "UPDATE recurring_transactions SET split_my_weight = NULL").is_ok());
}

#[test]
fn an_income_item_cannot_have_a_template_and_a_split_item_cannot_become_income() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let pay = item(&w.conn, w.salary);
    exec(&w.conn, &format!("UPDATE recurring_transactions SET split_my_weight = 1 WHERE id = {pay}")).unwrap();
    let error = w.conn.execute("INSERT INTO recurring_split_lines (recurring_transaction_id, person_id, weight) VALUES (?1, ?2, 1)", [pay, anna]).unwrap_err().to_string();
    assert!(error.contains("only an expense"), "{error}");
    let rent = item(&w.conn, w.rent);
    exec(&w.conn, &format!("UPDATE recurring_transactions SET split_my_weight = 1 WHERE id = {rent}")).unwrap();
    assert!(w.conn.execute("UPDATE recurring_transactions SET category_id = ?1 WHERE id = ?2", [w.salary, rent]).is_err());
}

#[test]
fn a_template_has_at_most_thirty_parts_and_keeps_its_item_and_person() {
    let w = world();
    let rent = item(&w.conn, w.rent);
    exec(&w.conn, &format!("UPDATE recurring_transactions SET split_my_weight = 1 WHERE id = {rent}")).unwrap();
    for i in 0..30 {
        w.conn.execute("INSERT INTO recurring_split_lines (recurring_transaction_id, guest_name, weight) VALUES (?1, ?2, 1)", params![rent, format!("Guest {i}")]).unwrap();
    }
    assert!(w.conn.execute("INSERT INTO recurring_split_lines (recurring_transaction_id, guest_name, weight) VALUES (?1, 'More', 1)", [rent]).is_err());
    assert!(exec(&w.conn, "UPDATE recurring_split_lines SET guest_name = 'Other' WHERE id = 1").is_err());
    assert!(exec(&w.conn, "UPDATE recurring_split_lines SET weight = 5 WHERE id = 1").is_ok());
}

#[test]
fn changing_a_template_changes_the_item_s_stamp_and_the_history_knows_the_weight() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let rent = item(&w.conn, w.rent);
    let mut seen = vec![stamp_of(&w.conn, "recurring_transactions", rent)];
    exec(&w.conn, &format!("UPDATE recurring_transactions SET split_my_weight = 2 WHERE id = {rent}")).unwrap();
    seen.push(stamp_of(&w.conn, "recurring_transactions", rent));
    w.conn.execute("INSERT INTO recurring_split_lines (recurring_transaction_id, person_id, weight) VALUES (?1, ?2, 1)", [rent, anna]).unwrap();
    seen.push(stamp_of(&w.conn, "recurring_transactions", rent));
    exec(&w.conn, "UPDATE recurring_split_lines SET weight = 3").unwrap();
    seen.push(stamp_of(&w.conn, "recurring_transactions", rent));
    assert!(seen.windows(2).all(|pair| pair[0] < pair[1]), "{seen:?}");
    let recorded: String = text(&w.conn, "SELECT new_values FROM audit_log WHERE entity_type = 'recurring_transactions' AND action = 'UPDATE' ORDER BY id DESC LIMIT 1");
    assert!(recorded.contains("\"split_my_weight\":2"), "{recorded}");
    assert_eq!(audit(&w.conn, "recurring_split_lines", "INSERT"), 1);
}

#[test]
fn deleting_an_item_takes_its_template() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let rent = item(&w.conn, w.rent);
    exec(&w.conn, &format!("UPDATE recurring_transactions SET split_my_weight = 2 WHERE id = {rent}")).unwrap();
    w.conn.execute("INSERT INTO recurring_split_lines (recurring_transaction_id, person_id, weight) VALUES (?1, ?2, 1)", [rent, anna]).unwrap();
    exec(&w.conn, &format!("DELETE FROM recurring_transactions WHERE id = {rent}")).unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM recurring_split_lines"), 0);
}

// ---- documents on an entry -------------------------------------------------------------------

fn document(conn: &Connection, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date) VALUES (?1, 'application/pdf', 10, ?2, '2026-01-01')",
        params![name, "ab".repeat(32)],
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[test]
fn a_document_can_be_linked_to_a_ledger_entry_and_to_exactly_one_thing() {
    let w = world();
    friend(&w.conn, "Anna");
    entry(&w.conn, ") VALUES (1, 'LENT', 100, '2026-01-01')").unwrap();
    let dinner = bill(&w.conn, w.food, "Dinner", "40.00", "2026-01-01");
    let doc = document(&w.conn, "Loan note");
    assert!(w.conn.execute("INSERT INTO document_links (document_id, ledger_entry_id) VALUES (?1, 1)", [doc]).is_ok());
    assert!(w.conn.execute("INSERT INTO document_links (document_id, ledger_entry_id) VALUES (?1, 1)", [doc]).is_err(), "once");
    assert!(w.conn.execute("INSERT INTO document_links (document_id) VALUES (?1)", [doc]).is_err(), "a link has a target");
    assert!(w.conn.execute("INSERT INTO document_links (document_id, ledger_entry_id, transaction_id) VALUES (?1, 1, ?2)", [doc, dinner]).is_err(), "and one");
    assert!(w.conn.execute("INSERT INTO document_links (document_id, ledger_entry_id) VALUES (?1, 99)", [doc]).is_err());
    assert!(w.conn.execute("UPDATE document_links SET ledger_entry_id = NULL", []).is_err(), "a link is made or removed, never changed");
}

#[test]
fn deleting_an_entry_removes_its_links_without_a_marker_and_unlinking_leaves_one() {
    let w = world();
    friend(&w.conn, "Anna");
    entry(&w.conn, ") VALUES (1, 'LENT', 100, '2026-01-01')").unwrap();
    entry(&w.conn, ") VALUES (1, 'LENT', 200, '2026-01-02')").unwrap();
    let doc = document(&w.conn, "Loan note");
    w.conn.execute("INSERT INTO document_links (document_id, ledger_entry_id) VALUES (?1, 1)", [doc]).unwrap();
    w.conn.execute("INSERT INTO document_links (document_id, ledger_entry_id) VALUES (?1, 2)", [doc]).unwrap();
    exec(&w.conn, "DELETE FROM ledger_entries WHERE id = 1").unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM document_links"), 1, "the link went with its entry");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'document_links'"), 0, "the entry's own marker says so");
    exec(&w.conn, "DELETE FROM document_links").unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'document_links'"), 1, "a person unlinking leaves one");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM documents"), 1, "the document stays");
}

// ---- the numbers -----------------------------------------------------------------------------

#[test]
fn the_cost_of_a_transaction_is_its_amount_minus_the_parts_of_the_others() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let dinner = bill(&w.conn, w.food, "Dinner", "100.00", "2026-01-01");
    let plain = bill(&w.conn, w.food, "Bread", "2.40", "2026-01-02");
    assert_eq!((cost(&w.conn, dinner), cost(&w.conn, plain)), (10_000, 240), "unsplit: the whole amount");
    person_line(&w.conn, dinner, anna, 2500).unwrap();
    guest_line(&w.conn, dinner, "Carl", 2500).unwrap();
    assert_eq!(cost(&w.conn, dinner), 5000);
    person_line(&w.conn, dinner, bob, 5000).unwrap();
    assert_eq!(cost(&w.conn, dinner), 0, "you paid for everyone");
    let view: (i64, String, i64) = w.conn.query_row("SELECT category_id, payment_date, cost_minor FROM transaction_costs WHERE transaction_id = ?1", [dinner], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    assert_eq!(view, (w.food, "2026-01-01".to_string(), 0), "the view carries the category and the date");
}

#[test]
fn a_balance_is_the_parts_of_bills_and_the_signed_entries() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "100.00", "2026-01-01");
    let balance = |conn: &Connection| one(conn, &format!("SELECT balance_minor FROM person_balances WHERE person_id = {anna}"));
    assert_eq!(balance(&w.conn), 0);
    person_line(&w.conn, dinner, anna, 2500).unwrap();
    assert_eq!(balance(&w.conn), 2500, "a part of a bill counts like a loan");
    for (kind, amount, expected) in [("LENT", 1000, 3500), ("REPAID_TO_ME", 500, 3000), ("BORROWED", 2000, 1000), ("REPAID_BY_ME", 300, 1300), ("REPAID_TO_ME", 5000, -3700)] {
        w.conn.execute("INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date) VALUES (?1, ?2, ?3, '2026-01-05')", params![anna, kind, amount]).unwrap();
        assert_eq!(balance(&w.conn), expected, "{kind} {amount}");
    }
    exec(&w.conn, "UPDATE people SET archived = 1").unwrap();
    assert_eq!(balance(&w.conn), -3700, "archiving changes no balance");
}

// ---- the migration ---------------------------------------------------------------------------

#[test]
fn migrating_a_database_of_version_11_keeps_what_it_had_and_the_triggers_keep_working() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::migrate::run_up_to(&mut conn, 11).unwrap();
    conn.execute("INSERT INTO settings (id, currency) VALUES (1, 'EUR')", []).unwrap();
    conn.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')", []).unwrap();
    conn.execute("INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (1, 'Dinner', 4000, '2026-01-01')", []).unwrap();
    conn.execute("INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (1, 'Lunch', 1000, '2026-01-02')", []).unwrap();
    conn.execute(
        "INSERT INTO recurring_transactions (category_id, name, amount_minor, start_date, interval_unit, next_due_date) VALUES (1, 'Rent', 90000, '2026-01-01', 'MONTH', '2026-01-01')",
        [],
    )
    .unwrap();
    document(&conn, "Receipt");
    conn.execute("INSERT INTO document_links (document_id, transaction_id) VALUES (1, 1)", []).unwrap();
    conn.execute("INSERT INTO document_links (document_id, recurring_transaction_id) VALUES (1, 1)", []).unwrap();
    conn.execute("DELETE FROM transactions WHERE name = 'Lunch'", []).unwrap();
    let markers = one(&conn, "SELECT count(*) FROM sync_deleted");
    assert_eq!(markers, 1);
    let links: Vec<(String, String)> = conn.prepare("SELECT sync_id, stamp FROM document_links ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    let audited = one(&conn, "SELECT count(*) FROM audit_log");

    crate::migrate::run(&mut conn).unwrap();

    assert_eq!(one(&conn, "PRAGMA user_version"), crate::migrate::latest_version() as i64);
    let after: Vec<(String, String)> = conn.prepare("SELECT sync_id, stamp FROM document_links ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    assert_eq!(after, links, "the links, with their ids and stamps");
    assert_eq!(one(&conn, "SELECT count(*) FROM sync_deleted"), markers, "the markers");
    assert_eq!(one(&conn, "SELECT count(*) FROM audit_log"), audited, "the migration adds no history of its own");
    assert_eq!(one(&conn, "SELECT count(*) FROM transactions"), 1);
    assert_eq!(one(&conn, "SELECT count(*) FROM recurring_transactions WHERE split_my_weight IS NULL"), 1, "no item is split");
    // the triggers that write markers were made again and still do
    conn.execute("DELETE FROM transactions", []).unwrap();
    conn.execute("DELETE FROM recurring_transactions", []).unwrap();
    conn.execute("DELETE FROM documents", []).unwrap();
    for kind in ["transactions", "recurring_transactions", "documents"] {
        assert!(one(&conn, &format!("SELECT count(*) FROM sync_deleted WHERE kind = '{kind}'")) >= 1, "{kind}");
    }
    assert_eq!(one(&conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'document_links'"), 0, "links the foreign key removed leave none");
    conn.execute("INSERT INTO budgets (type, scope, amount_limit_minor, effective_from) VALUES ('EXPENSE', 'GLOBAL', 100, '2026-01-01')", []).unwrap();
    conn.execute("DELETE FROM budgets", []).unwrap();
    assert_eq!(one(&conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'budgets'"), 1);
}
