//! Backups with friends, the ledger and shared bills: they come back exactly, and a file that breaks the rules of the parts is refused whole.

use serde_json::{json, Value};

use super::tests::{db, lived_in};
use super::*;
use crate::debts::{ledger, splits};

fn table<'a>(snapshot: &'a mut Snapshot, name: &str) -> &'a mut Vec<Row> {
    snapshot.tables.get_mut(name).unwrap_or_else(|| panic!("no rows for {name}"))
}

fn set(snapshot: &mut Snapshot, name: &str, index: usize, column: &str, value: Value) {
    table(snapshot, name)[index].insert(column.into(), value);
}

fn restores(snapshot: &Snapshot) -> (Result<(), AppError>, Connection, Snapshot) {
    let mut target = lived_in();
    // something of its own that must survive a refused restore
    target.execute("INSERT INTO people (name) VALUES ('Only here')", []).unwrap();
    let before = dump(&target).unwrap();
    let result = restore_snapshot(&mut target, snapshot);
    (result, target, before)
}

#[test]
fn friends_the_ledger_and_the_parts_of_bills_come_back_exactly() {
    let source = lived_in();
    let file = seal(&dump(&source).unwrap(), "correct horse").unwrap();
    let mut target = db();
    restore_snapshot(&mut target, &open(&file, "correct horse").unwrap()).unwrap();
    for name in ["people", "ledger_entries", "transaction_split_lines", "recurring_split_lines"] {
        assert!(!dump(&source).unwrap().rows(name).is_empty(), "the test data fills {name}");
    }
    assert_eq!(dump(&target).unwrap(), dump(&source).unwrap());
    // and they are alive: the numbers are the same and the rules still hold
    let balance = |conn: &Connection| -> Vec<(String, i64)> { conn.prepare("SELECT p.name, b.balance_minor FROM people p JOIN person_balances b ON b.person_id = p.id ORDER BY p.id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect() };
    assert_eq!(balance(&target), balance(&source));
    assert!(splits::check(&target).unwrap());
    assert_eq!(splits::all_template_weights(&target).unwrap(), splits::all_template_weights(&source).unwrap());
    let anna: i64 = target.query_row("SELECT id FROM people WHERE name LIKE 'Anna%'", [], |r| r.get(0)).unwrap();
    assert!(ledger::settle(&target, anna, None, None, None, "2026-03-01").is_ok(), "an entry made after a restore works");
}

#[test]
fn a_backup_made_before_this_phase_restores_with_none_of_it() {
    // the schema of the version before: the four tables do not exist and the restore still passes the checks that look at them
    let old = db_at(11);
    old.execute("INSERT INTO settings (id, currency) VALUES (1, 'EUR')", []).unwrap();
    old.execute("INSERT INTO categories (type, name) VALUES ('EXPENSE', 'Food')", []).unwrap();
    old.execute("INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (1, 'Dinner', 4000, '2026-01-01')", []).unwrap();
    let snapshot = dump(&old).unwrap();
    assert_eq!(snapshot.schema_version, 11);
    assert!(snapshot.tables.keys().all(|name| name != "people"));
    let mut target = db();
    restore_snapshot(&mut target, &snapshot).unwrap();
    assert_eq!(target.query_row::<i64, _, _>("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap(), 1);
    assert_eq!(target.query_row::<i64, _, _>("SELECT count(*) FROM people", [], |r| r.get(0)).unwrap(), 0);
    assert_eq!(target.query_row::<i64, _, _>("SELECT cost_minor FROM transaction_costs", [], |r| r.get(0)).unwrap(), 4000);
}

fn db_at(version: u32) -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrate::run_up_to(&mut conn, version).unwrap();
    conn
}

/// Each change to the rows below makes a file the app could not have written. Every one is refused and nothing changes.
#[test]
fn a_file_that_breaks_the_rules_of_the_parts_is_refused_whole_and_changes_nothing() {
    let base = dump(&lived_in()).unwrap();
    let income_transaction = base.rows("transactions").iter().find(|r| r["name"] == "Salary").unwrap()["id"].clone();
    let dinner = base.rows("transactions").iter().find(|r| r["name"] == "Dinner").unwrap()["id"].clone();
    let dinner_amount = base.rows("transactions").iter().find(|r| r["name"] == "Dinner").unwrap()["amount_minor"].as_i64().unwrap();
    let item = base.rows("recurring_transactions")[0]["id"].clone();

    type Change = Box<dyn Fn(&mut Snapshot)>;
    let cases: Vec<(&str, Change)> = vec![
        ("a part above the bill", {
            let amount = dinner_amount + 1;
            Box::new(move |s| set(s, "transaction_split_lines", 0, "amount_minor", json!(amount)))
        }),
        ("parts that together pass the bill", {
            let (dinner, amount) = (dinner.clone(), dinner_amount);
            Box::new(move |s| {
                let rows = table(s, "transaction_split_lines");
                let mut extra = rows[0].clone();
                extra.insert("id".into(), json!(900));
                extra.insert("person_id".into(), Value::Null);
                extra.insert("guest_name".into(), json!("Extra"));
                extra.insert("transaction_id".into(), dinner.clone());
                extra.insert("amount_minor".into(), json!(amount));
                rows.push(extra);
            })
        }),
        ("a part of an income", {
            let income = income_transaction.clone();
            Box::new(move |s| set(s, "transaction_split_lines", 0, "transaction_id", income.clone()))
        }),
        ("a part for a bill that is not there", Box::new(|s| set(s, "transaction_split_lines", 0, "transaction_id", json!(9999)))),
        ("a part for a friend that is not there", Box::new(|s| set(s, "transaction_split_lines", 0, "person_id", json!(9999)))),
        ("a part with both a friend and a guest", Box::new(|s| set(s, "transaction_split_lines", 0, "guest_name", json!("Both")))),
        ("a part of nothing", Box::new(|s| set(s, "transaction_split_lines", 0, "amount_minor", json!(0)))),
        ("a kind that does not exist", Box::new(|s| set(s, "ledger_entries", 0, "kind", json!("GIFT")))),
        ("an amount of nothing in the ledger", Box::new(|s| set(s, "ledger_entries", 0, "amount_minor", json!(0)))),
        ("an entry for a friend that is not there", Box::new(|s| set(s, "ledger_entries", 0, "person_id", json!(9999)))),
        ("an entry that is not dated", Box::new(|s| set(s, "ledger_entries", 0, "entry_date", json!("2026-02-30")))),
        ("a bill code accepted twice", Box::new(|s| {
            let rows = table(s, "ledger_entries");
            let mut twin = rows[0].clone();
            twin.insert("id".into(), json!(901));
            twin.insert("sync_id".into(), json!("ee".repeat(16)));
            for r in [&mut rows[0], &mut twin] {
                r.insert("origin_id".into(), json!("ab".repeat(16)));
            }
            rows.push(twin);
        })),
        ("two friends with the same name", Box::new(|s| {
            let rows = table(s, "people");
            let mut twin = rows[0].clone();
            twin.insert("id".into(), json!(902));
            twin.insert("sync_id".into(), json!("dd".repeat(16)));
            twin.insert("archived".into(), json!(0));
            rows[0].insert("archived".into(), json!(0));
            rows.push(twin);
        })),
        ("a name that is too long", Box::new(|s| set(s, "people", 0, "name", json!("x".repeat(51))))),
        ("a repeat with no date", Box::new(|s| {
            let row = &mut table(s, "people")[0];
            row.insert("remind_on".into(), Value::Null);
            row.insert("remind_every_days".into(), json!(7));
        })),
        ("a template line without a weight for the payer", {
            let item = item.clone();
            Box::new(move |s| {
                for r in table(s, "recurring_transactions").iter_mut().filter(|r| r["id"] == item) {
                    r.insert("split_my_weight".into(), Value::Null);
                }
            })
        }),
        ("a template of nothing", {
            let item = item.clone();
            Box::new(move |s| {
                for r in table(s, "recurring_transactions").iter_mut().filter(|r| r["id"] == item) {
                    r.insert("split_my_weight".into(), json!(0));
                }
                table(s, "recurring_split_lines").clear();
            })
        }),
        ("a column the table does not have", Box::new(|s| set(s, "people", 0, "phone", json!("555")))),
        ("a part on a bill twice for the same friend", {
            let dinner = dinner.clone();
            Box::new(move |s| {
                let rows = table(s, "transaction_split_lines");
                let mut again = rows[0].clone();
                again.insert("id".into(), json!(903));
                again.insert("transaction_id".into(), dinner.clone());
                again.insert("amount_minor".into(), json!(1));
                rows.push(again);
            })
        }),
    ];
    for (what, change) in cases {
        // the first line of the base is the part of the dinner Anna has; make sure of it
        let mut hostile = base.clone();
        assert_eq!(hostile.rows("transaction_split_lines")[0]["transaction_id"], dinner, "the fixture changed");
        change(&mut hostile);
        let (result, target, before) = restores(&hostile);
        assert_eq!(result, Err(AppError::InvalidBackup), "{what}");
        assert_eq!(dump(&target).unwrap(), before, "{what}: nothing was restored");
        assert!(splits::check(&target).unwrap(), "{what}");
    }
}
