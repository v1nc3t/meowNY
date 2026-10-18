//! The sync engine between two databases in memory: every merge rule of `docs/sync-design.md` (section 11), the checks on
//! what arrives (section 12), and a fixed-seed random test that edits both devices with the app's own functions and
//! checks that one sync makes them the same and the next one changes nothing.

use std::collections::HashSet;
use std::time::Duration;

use rusqlite::{params, Connection};

use super::engine::{self, snapshot, Incoming, RowsCollector, SyncError, Warning};
use super::run::{sync, Outcome};
use super::stamp::{self, Stamp};
use super::wire::{self, Marker, Message, Row, Table};
use crate::budgets::{self, Scope, SetBudget};
use crate::categories::{self, CategoryType};
use crate::debts::{ledger, people, splits};
use crate::documents::library::{self as library, LinkTarget, TargetKind};
use crate::recurring::{self, NewRecurring, Unit};
use crate::transactions::{self, NewTransaction};
use crate::{currency, migrate};

const TODAY: &str = "2026-03-15";

fn now() -> u64 {
    stamp::now_ms()
}

/// A device that has been through its first run: its settings exist, in `currency`.
fn device(currency: &str) -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrate::run(&mut conn).unwrap();
    conn.execute("INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, ?1, 'en-IE', 'Europe/Dublin', 'en')", [currency]).unwrap();
    conn
}

/// Fails with only the lines that differ, so a failure can be read.
fn assert_same(a: &Connection, b: &Connection, label: &str) {
    let (x, y) = (snapshot(a).unwrap(), snapshot(b).unwrap());
    if x != y {
        let short = |l: &String| l.chars().take(1000).collect::<String>();
        let only_first: Vec<String> = x.iter().filter(|l| !y.contains(l)).map(short).collect();
        let only_second: Vec<String> = y.iter().filter(|l| !x.contains(l)).map(short).collect();
        panic!("{label}: the devices differ\nonly on the first: {only_first:#?}\nonly on the second: {only_second:#?}");
    }
}

/// A sync that must end with the two equal after this one round, and a second one that must find nothing to do.
fn sync_ok(a: &mut Connection, b: &mut Connection) -> Outcome {
    let outcome = sync(a, b, TODAY, now(), None).unwrap();
    assert!(!outcome.differences, "the two still differ after one sync: {outcome:?}");
    assert_same(a, b, "after one sync");
    let again = sync(a, b, TODAY, now(), None).unwrap();
    assert!(again.compared.is_empty() && again.first.is_empty() && again.second.is_empty() && !again.differences, "a second sync changed something: {again:?}");
    outcome
}

fn pause() {
    std::thread::sleep(Duration::from_millis(3));
}

fn category(conn: &Connection, name: &str, kind: CategoryType) -> i64 {
    categories::create(conn, name, kind).unwrap().id
}

fn spend(conn: &Connection, category_id: i64, name: &str, amount: &str) -> i64 {
    transactions::create(
        conn,
        &NewTransaction { name: name.into(), amount: amount.into(), payment_date: "2026-03-02".into(), category_id: category_id.to_string(), description: None, client_uuid: None },
    )
    .unwrap()
    .id
}

fn budget(conn: &Connection, category_id: Option<i64>, month: &str, limit: Option<&str>) {
    budgets::set(
        conn,
        &SetBudget { kind: CategoryType::Expense, scope: if category_id.is_some() { Scope::Category } else { Scope::Global }, month: month.into(), amount_limit: limit.map(str::to_string), category_id: category_id.map(|c| c.to_string()) },
        chrono::Utc::now(),
    )
    .unwrap();
}

fn rent(conn: &Connection, category_id: i64) -> i64 {
    recurring::create(
        conn,
        &NewRecurring { name: "Rent".into(), amount: Some("900.00".into()), category_id: category_id.to_string(), start_date: "2026-03-01".into(), interval_unit: Unit::Month, interval_count: 1, end_date: None, description: None },
    )
    .unwrap()
    .id
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn text(conn: &Connection, sql: &str) -> String {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// A phone as it is when it has just been paired: its first-run settings give way to the desktop's.
fn new_phone(desktop: &Connection) -> Connection {
    let mut phone = device("EUR");
    let changes = engine::changes_after(desktop, 0).unwrap();
    let (currency, generation): (String, i64) = desktop.query_row("SELECT currency, currency_generation FROM settings", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    engine::adopt_currency(&mut phone, &currency, generation, &changes).unwrap();
    phone
}

/// A desktop with a bit of everything, already synced to a new phone.
fn paired() -> (Connection, Connection) {
    let mut desktop = device("EUR");
    let mut phone = new_phone(&desktop);
    let income = category(&desktop, "Salary", CategoryType::Income);
    let food = category(&desktop, "Food", CategoryType::Expense);
    let housing = category(&desktop, "Housing", CategoryType::Expense);
    let _ = income;
    spend(&desktop, food, "Coffee", "3.50");
    spend(&desktop, food, "Tea", "2.00");
    budget(&desktop, None, "2026-03", Some("2000.00"));
    budget(&desktop, Some(food), "2026-03", Some("300.00"));
    let item = rent(&desktop, housing);
    recurring::pay(&desktop, item, None, Some("2026-03-02"), TODAY).unwrap();
    let doomed = spend(&desktop, food, "Mistake", "9.99");
    transactions::delete(&desktop, doomed).unwrap();
    sync_ok(&mut desktop, &mut phone);
    (desktop, phone)
}

fn id_of(conn: &Connection, table: &str, name: &str) -> i64 {
    conn.query_row(&format!("SELECT id FROM {table} WHERE name = ?1"), [name], |r| r.get(0)).unwrap()
}

// ---------------------------------------------------------------------------------------------------------------

#[test]
fn a_new_phone_gets_a_full_copy_and_a_second_sync_finds_nothing() {
    let mut desktop = device("EUR");
    let mut phone = new_phone(&desktop);
    let food = category(&desktop, "Food", CategoryType::Expense);
    for i in 0..30 {
        spend(&desktop, food, &format!("t{i}"), "1.25");
    }
    let outcome = sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&phone, "SELECT count(*) FROM transactions"), 30);
    assert_eq!((outcome.first.is_empty(), outcome.second.added, outcome.second.updated), (true, 31, 1), "the category and the 30 payments are new, the settings are taken over");
    assert_eq!(count(&phone, "SELECT count(*) FROM categories"), 1);
}

#[test]
fn the_same_entry_sent_from_both_devices_becomes_one() {
    let (mut desktop, mut phone) = paired();
    let uuid = "0b8f6c1e-5c52-4d6a-9d50-3f7a8a1b2c3d";
    for (conn, name) in [(&desktop, "first"), (&phone, "second")] {
        let food = id_of(conn, "categories", "Food");
        pause();
        transactions::create(
            conn,
            &NewTransaction { name: name.into(), amount: "4.00".into(), payment_date: "2026-03-03".into(), category_id: food.to_string(), description: None, client_uuid: Some(uuid.into()) },
        )
        .unwrap();
    }
    let outcome = sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&desktop, "SELECT count(*) FROM transactions WHERE client_uuid IS NOT NULL"), 1);
    assert_eq!(text(&desktop, "SELECT name FROM transactions WHERE client_uuid IS NOT NULL"), "second", "the newer one stays");
    assert_eq!(outcome.first.dropped + outcome.second.dropped, 1 + 1);
}

#[test]
fn an_edit_made_later_wins_on_either_side() {
    let (mut desktop, mut phone) = paired();
    let (d, p) = (id_of(&desktop, "transactions", "Coffee"), id_of(&phone, "transactions", "Coffee"));
    let before = now();
    pause();
    desktop.execute("UPDATE transactions SET name = 'Desktop edit', amount_minor = 111 WHERE id = ?1", [d]).unwrap();
    pause();
    phone.execute("UPDATE transactions SET name = 'Phone edit', amount_minor = 222 WHERE id = ?1", [p]).unwrap();
    let outcome = sync(&mut desktop, &mut phone, TODAY, now(), Some(before)).unwrap();
    assert!(!outcome.differences);
    assert_same(&desktop, &phone, "after one sync");
    assert_eq!(text(&desktop, "SELECT name FROM transactions WHERE amount_minor = 222"), "Phone edit");
    assert_eq!(count(&desktop, "SELECT count(*) FROM transactions WHERE name = 'Desktop edit'"), 0);
    assert_eq!((outcome.first.updated, outcome.first.replaced, outcome.second.updated), (1, 1, 0), "the desktop's own change was replaced by the phone's newer one; the phone has nothing to take over");

    // and the other way round
    let (d, p) = (id_of(&desktop, "transactions", "Tea"), id_of(&phone, "transactions", "Tea"));
    phone.execute("UPDATE transactions SET name = 'Phone first' WHERE id = ?1", [p]).unwrap();
    pause();
    desktop.execute("UPDATE transactions SET name = 'Desktop second' WHERE id = ?1", [d]).unwrap();
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&phone, "SELECT count(*) FROM transactions WHERE name = 'Desktop second'"), 1);
}

#[test]
fn the_newer_of_an_edit_and_a_delete_wins() {
    let (mut desktop, mut phone) = paired();
    // delete is newer: the row goes
    let (d, p) = (id_of(&desktop, "transactions", "Coffee"), id_of(&phone, "transactions", "Coffee"));
    phone.execute("UPDATE transactions SET name = 'Edited' WHERE id = ?1", [p]).unwrap();
    pause();
    transactions::delete(&desktop, d).unwrap();
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&phone, "SELECT count(*) FROM transactions WHERE name IN ('Coffee', 'Edited')"), 0);

    // edit is newer: the row stays on both
    let (d, p) = (id_of(&desktop, "transactions", "Tea"), id_of(&phone, "transactions", "Tea"));
    transactions::delete(&desktop, d).unwrap();
    pause();
    phone.execute("UPDATE transactions SET name = 'Rescued' WHERE id = ?1", [p]).unwrap();
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&desktop, "SELECT count(*) FROM transactions WHERE name = 'Rescued'"), 1);
    assert_eq!(count(&desktop, "SELECT count(*) FROM sync_deleted WHERE kind = 'transactions'"), 2, "the earlier ones stay, but the marker of the rescued row is gone");
}

#[test]
fn a_recurring_item_paid_on_both_devices_gives_one_payment_and_is_not_due_twice() {
    let (mut desktop, mut phone) = paired();
    let (d, p) = (id_of(&desktop, "recurring_transactions", "Rent"), id_of(&phone, "recurring_transactions", "Rent"));
    // the item is due on 2026-04-01 on both; each device pays it
    recurring::pay(&desktop, d, Some("905.50"), Some("2026-04-01"), TODAY).unwrap();
    pause();
    recurring::pay(&phone, p, Some("899.00"), Some("2026-04-02"), TODAY).unwrap();
    let outcome = sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&desktop, "SELECT count(*) FROM transactions WHERE recurring_due_date = '2026-04-01'"), 1);
    assert_eq!(text(&desktop, "SELECT payment_date FROM transactions WHERE recurring_due_date = '2026-04-01'"), "2026-04-02", "the newer payment stays");
    assert_eq!(text(&desktop, "SELECT next_due_date FROM recurring_transactions"), "2026-05-01", "paid once, so due once more, not twice");
    assert!(outcome.first.dropped + outcome.second.dropped >= 1);
}

#[test]
fn a_payment_made_on_one_device_survives_an_edit_of_the_item_on_the_other() {
    let (mut desktop, mut phone) = paired();
    let (d, p) = (id_of(&desktop, "recurring_transactions", "Rent"), id_of(&phone, "recurring_transactions", "Rent"));
    recurring::pay(&phone, p, None, Some("2026-04-01"), TODAY).unwrap();
    pause();
    desktop.execute("UPDATE recurring_transactions SET name = 'Flat rent' WHERE id = ?1", [d]).unwrap();
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(text(&desktop, "SELECT name FROM recurring_transactions"), "Flat rent", "the newer edit wins");
    assert_eq!(text(&desktop, "SELECT next_due_date FROM recurring_transactions"), "2026-05-01", "but the payment is not due again");
    assert_eq!(count(&desktop, "SELECT count(*) FROM transactions WHERE recurring_due_date = '2026-04-01'"), 1);
}

#[test]
fn deleting_a_recurring_item_on_one_device_clears_the_link_on_the_other() {
    let (mut desktop, mut phone) = paired();
    let d = id_of(&desktop, "recurring_transactions", "Rent");
    recurring::delete(&desktop, d).unwrap();
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&phone, "SELECT count(*) FROM recurring_transactions"), 0);
    assert_eq!(count(&phone, "SELECT count(*) FROM transactions WHERE recurring_due_date IS NOT NULL AND recurring_transaction_id IS NULL"), 1);
}

#[test]
fn budgets_for_the_same_month_and_category_on_both_devices_become_one() {
    let (mut desktop, mut phone) = paired();
    let (d_food, p_food) = (id_of(&desktop, "categories", "Food"), id_of(&phone, "categories", "Food"));
    budget(&desktop, Some(d_food), "2026-04", Some("100.00"));
    pause();
    budget(&phone, Some(p_food), "2026-04", Some("250.00"));
    budget(&desktop, None, "2026-04", Some("1500.00"));
    pause();
    budget(&phone, None, "2026-04", Some("1800.00"));
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&desktop, "SELECT amount_limit_minor FROM budgets WHERE effective_from = '2026-04-01' AND scope = 'CATEGORY'"), 25000);
    assert_eq!(count(&desktop, "SELECT amount_limit_minor FROM budgets WHERE effective_from = '2026-04-01' AND scope = 'GLOBAL'"), 180000);
    assert_eq!(count(&desktop, "SELECT count(*) FROM budgets WHERE effective_from = '2026-04-01'"), 2);
}

#[test]
fn a_category_with_the_same_name_on_both_devices_becomes_one_and_takes_everything_with_it() {
    let (mut desktop, mut phone) = paired();
    let (d_gym, p_gym) = (category(&desktop, "Gym", CategoryType::Expense), category(&phone, "Gym", CategoryType::Expense));
    spend(&desktop, d_gym, "Membership", "30.00");
    spend(&phone, p_gym, "Towel", "8.00");
    budget(&desktop, Some(d_gym), "2026-05", Some("50.00"));
    budget(&phone, Some(p_gym), "2026-05", Some("60.00"));
    budget(&phone, Some(p_gym), "2026-06", Some("70.00"));
    let d_item = recurring::create(
        &desktop,
        &NewRecurring { name: "Gym fee".into(), amount: Some("30.00".into()), category_id: d_gym.to_string(), start_date: "2026-03-05".into(), interval_unit: Unit::Month, interval_count: 1, end_date: None, description: None },
    )
    .unwrap()
    .id;
    let _ = d_item;
    let outcome = sync_ok(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(count(conn, "SELECT count(*) FROM categories WHERE name = 'Gym' AND deleted_at IS NULL"), 1);
        assert_eq!(count(conn, "SELECT count(*) FROM categories WHERE name = 'Gym'"), 2, "the other one stays, merged away");
        assert_eq!(count(conn, "SELECT count(*) FROM categories WHERE merged_into IS NOT NULL"), 1);
        let gym: i64 = conn.query_row("SELECT id FROM categories WHERE name = 'Gym' AND deleted_at IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(count(conn, &format!("SELECT count(*) FROM transactions WHERE category_id = {gym}")), 2, "both payments are on the one Gym");
        assert_eq!(count(conn, &format!("SELECT count(*) FROM recurring_transactions WHERE category_id = {gym}")), 1);
        let months: Vec<String> = conn.prepare(&format!("SELECT effective_from || ':' || ifnull(amount_limit_minor, 'none') FROM budgets WHERE category_id = {gym} ORDER BY 1")).unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        let in_month = |m: &str| months.iter().filter(|x| x.starts_with(m)).count();
        assert_eq!((in_month("2026-05"), in_month("2026-06")), (1, 1), "May once, June once: {months:?}");
    }
    assert!(outcome.first.merged + outcome.second.merged >= 1);
}

#[test]
fn a_category_deleted_on_one_device_and_used_on_the_other_stays_deleted_and_keeps_the_payment() {
    let (mut desktop, mut phone) = paired();
    let (d_food, p_food) = (id_of(&desktop, "categories", "Food"), id_of(&phone, "categories", "Food"));
    categories::delete(&mut desktop, d_food, chrono::Utc::now()).unwrap();
    spend(&phone, p_food, "Late lunch", "12.00");
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&phone, "SELECT count(*) FROM categories WHERE name = 'Food' AND deleted_at IS NOT NULL"), 1);
    assert_eq!(count(&desktop, "SELECT count(*) FROM transactions WHERE name = 'Late lunch'"), 1);
}

#[test]
fn a_renamed_category_wins_by_time_and_a_rename_onto_a_taken_name_merges() {
    let (mut desktop, mut phone) = paired();
    let (d, p) = (id_of(&desktop, "categories", "Food"), id_of(&phone, "categories", "Food"));
    categories::rename(&desktop, d, "Groceries").unwrap();
    pause();
    categories::rename(&phone, p, "Eating").unwrap();
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&desktop, "SELECT count(*) FROM categories WHERE name = 'Eating'"), 1);

    // the desktop renames Housing to Gym while the phone makes a Gym: one category from the two
    let d_housing = id_of(&desktop, "categories", "Housing");
    categories::rename(&desktop, d_housing, "Gym").unwrap();
    let p_gym = category(&phone, "Gym", CategoryType::Expense);
    spend(&phone, p_gym, "Towel", "8.00");
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&desktop, "SELECT count(*) FROM categories WHERE name = 'Gym' AND deleted_at IS NULL"), 1);
}

#[test]
fn shared_settings_follow_the_newer_change_and_the_language_stays_with_the_device() {
    let (mut desktop, mut phone) = paired();
    desktop.execute("UPDATE settings SET timezone = 'UTC'", []).unwrap();
    pause();
    phone.execute("UPDATE settings SET locale = 'de-DE', language = 'ro'", []).unwrap();
    desktop.execute("UPDATE settings SET language = 'fr'", []).unwrap();
    sync_ok(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(text(conn, "SELECT locale FROM settings"), "de-DE");
    }
    assert_eq!(text(&phone, "SELECT timezone FROM settings"), "Europe/Dublin", "the newer change is the phone's locale, which carries the whole shared row");
    assert_eq!((text(&desktop, "SELECT language FROM settings"), text(&phone, "SELECT language FROM settings")), ("fr".into(), "ro".into()));
    assert_eq!(text(&desktop, "SELECT currency FROM settings"), "EUR");
}

// --- currency -------------------------------------------------------------------------------------------------

#[test]
fn a_currency_change_on_the_desktop_reaches_the_phone_with_its_amounts_converted() {
    let (mut desktop, mut phone) = paired();
    let p_food = id_of(&phone, "categories", "Food");
    spend(&phone, p_food, "Added on the phone in euros", "5.00");
    currency::change(&mut desktop, "USD", Some("2"), TODAY).unwrap();
    assert_eq!(count(&desktop, "SELECT amount_minor FROM transactions WHERE name = 'Coffee'"), 700);

    let outcome = sync_ok(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(text(conn, "SELECT currency FROM settings"), "USD");
        assert_eq!(count(conn, "SELECT currency_generation FROM settings"), 1);
        assert_eq!(count(conn, "SELECT amount_minor FROM transactions WHERE name = 'Coffee'"), 700);
        assert_eq!(count(conn, "SELECT amount_minor FROM transactions WHERE name = 'Added on the phone in euros'"), 1000, "the phone's own new row was converted too");
        assert_eq!(count(conn, "SELECT amount_limit_minor FROM budgets WHERE scope = 'GLOBAL'"), 400_000);
    }
    assert_eq!(outcome.second.added, 0, "converting is not a change of rows: only the phone's new row travels");
    assert_eq!(outcome.first.added, 1);
}

#[test]
fn a_relabel_only_leaves_the_amounts_alone_on_both() {
    let (mut desktop, mut phone) = paired();
    currency::change(&mut desktop, "GBP", None, TODAY).unwrap();
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(text(&phone, "SELECT currency FROM settings"), "GBP");
    assert_eq!(count(&phone, "SELECT amount_minor FROM transactions WHERE name = 'Coffee'"), 350);
}

#[test]
fn two_currency_changes_in_a_row_are_applied_in_order() {
    let (mut desktop, mut phone) = paired();
    currency::change(&mut desktop, "USD", Some("1.1"), TODAY).unwrap();
    currency::change(&mut desktop, "GBP", Some("0.8"), TODAY).unwrap();
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&phone, "SELECT currency_generation FROM settings"), 2);
    assert_eq!(count(&phone, "SELECT count(*) FROM currency_changes"), 2);
    assert_eq!(count(&phone, "SELECT amount_minor FROM transactions WHERE name = 'Coffee'"), 308, "350 x 1.1 = 385, then x 0.8 = 308");
    assert_eq!(count(&desktop, "SELECT amount_minor FROM transactions WHERE name = 'Coffee'"), 308);
}

#[test]
fn a_currency_change_that_cannot_be_converted_on_the_phone_changes_nothing_there() {
    let (mut desktop, mut phone) = paired();
    let p_food = id_of(&phone, "categories", "Food");
    phone.execute("INSERT INTO transactions (category_id, name, amount_minor, payment_date) VALUES (?1, 'Huge', 600000000000, '2026-03-02')", [p_food]).unwrap();
    currency::change(&mut desktop, "USD", Some("2"), TODAY).unwrap();
    let before = snapshot(&phone).unwrap();
    let result = sync(&mut desktop, &mut phone, TODAY, now(), None);
    assert!(matches!(result, Err(SyncError::Currency)), "{result:?}");
    assert_eq!(snapshot(&phone).unwrap(), before);
    assert_eq!(text(&phone, "SELECT currency FROM settings"), "EUR");
    assert_eq!(count(&phone, "SELECT applying FROM sync_clock"), 0);
}

#[test]
fn a_new_phone_with_another_currency_adopts_the_desktops_without_converting() {
    let mut desktop = device("EUR");
    let food = category(&desktop, "Food", CategoryType::Expense);
    spend(&desktop, food, "Coffee", "3.50");
    currency::change(&mut desktop, "USD", Some("2"), TODAY).unwrap();
    currency::change(&mut desktop, "GBP", None, TODAY).unwrap();

    let mut phone = device("RON");
    assert!(engine::is_empty(&phone).unwrap());
    let changes = engine::changes_after(&desktop, 0).unwrap();
    engine::adopt_currency(&mut phone, "GBP", 2, &changes).unwrap();
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(text(&phone, "SELECT currency FROM settings"), "GBP");
    assert_eq!(count(&phone, "SELECT amount_minor FROM transactions"), 700);

    // a phone that has data cannot adopt another currency
    let mut used = device("RON");
    let c = category(&used, "Food", CategoryType::Expense);
    spend(&used, c, "x", "1.00");
    assert!(!engine::is_empty(&used).unwrap());
    assert!(matches!(engine::adopt_currency(&mut used, "GBP", 2, &changes), Err(SyncError::Currency)));
    assert_eq!(text(&used, "SELECT currency FROM settings"), "RON");
}

#[test]
fn devices_with_the_same_generation_but_another_currency_do_not_sync() {
    let mut a = device("EUR");
    let mut b = device("USD");
    let result = sync(&mut a, &mut b, TODAY, now(), None);
    assert!(matches!(result, Err(SyncError::Currency)), "{result:?}");
}

// --- hello ------------------------------------------------------------------------------------------------------

#[test]
fn hellos_are_checked_for_version_clock_and_identity() {
    let (a, b) = (device("EUR"), device("EUR"));
    let mine = engine::hello(&a, "desktop", "0.1.0").unwrap();
    let theirs = engine::hello(&b, "phone", "0.1.0").unwrap();
    assert_eq!(engine::check_hello(&mine, &theirs, now()).unwrap(), vec![]);

    let changed = |f: &dyn Fn(&mut wire::Hello)| {
        let mut h = theirs.clone();
        f(&mut h);
        h
    };
    assert!(matches!(engine::check_hello(&mine, &changed(&|h| h.schema += 1), now()), Err(SyncError::SchemaMismatch { newer_here: false })));
    assert!(matches!(engine::check_hello(&mine, &changed(&|h| h.schema -= 1), now()), Err(SyncError::SchemaMismatch { newer_here: true })));
    assert!(matches!(engine::check_hello(&mine, &changed(&|h| h.protocol += 1), now()), Err(SyncError::ProtocolMismatch)));
    assert!(matches!(engine::check_hello(&mine, &changed(&|h| h.device_id = mine.device_id.clone()), now()), Err(SyncError::Invalid(_))));
    assert!(matches!(engine::check_hello(&mine, &changed(&|h| h.currency = "USD".into()), now()), Err(SyncError::Currency)));

    let at = |ms: u64| changed(&|h| h.clock = Stamp::new(ms, 0, "abcdef01").format());
    let warned = engine::check_hello(&mine, &at(now() + 90_000), now()).unwrap();
    assert!(matches!(warned.as_slice(), [Warning::ClockOff(s)] if (89..=91).contains(s)), "{warned:?}");
    assert!(engine::check_hello(&mine, &at(now() + 30_000), now()).unwrap().is_empty(), "half a minute is nothing to mention");
    assert!(matches!(engine::check_hello(&mine, &at(now() + 2 * 86_400_000), now()), Err(SyncError::ClockFarOff)));
    assert!(matches!(engine::check_hello(&mine, &at(now().saturating_sub(2 * 86_400_000)), now()), Err(SyncError::ClockFarOff)));
}

// --- what arrives is not trusted --------------------------------------------------------------------------------

fn rows_to(source: &Connection, target: &Connection) -> Incoming {
    let (ds, dt) = (engine::digest(source).unwrap(), engine::digest(target).unwrap());
    let tables = Table::ALL.to_vec();
    let mut collector = RowsCollector::new(tables.clone());
    for message in engine::outgoing(source, &ds, &dt, &tables).unwrap() {
        match Message::from_bytes(&message.to_bytes().unwrap()).unwrap() {
            Message::Rows(r) => collector.push(r).unwrap(),
            _ => unreachable!(),
        }
    }
    collector.finish().unwrap()
}

#[test]
fn applying_the_same_rows_twice_changes_nothing_the_second_time() {
    let (desktop, mut phone) = paired();
    let food = id_of(&desktop, "categories", "Food");
    spend(&desktop, food, "New", "1.00");
    let incoming = rows_to(&desktop, &phone);
    let first = engine::apply(&mut phone, &incoming, now(), None).unwrap();
    assert!(first.added >= 1);
    let after_first = snapshot(&phone).unwrap();
    let second = engine::apply(&mut phone, &incoming, now(), None).unwrap();
    assert!(second.is_empty(), "{second:?}");
    assert_eq!(snapshot(&phone).unwrap(), after_first);
}

#[test]
fn rows_that_break_a_reference_are_refused_and_nothing_at_all_is_applied() {
    let (desktop, mut phone) = paired();
    let food = id_of(&desktop, "categories", "Food");
    spend(&desktop, food, "Good row", "1.00");
    let mut incoming = rows_to(&desktop, &phone);
    // a second row that points at a category nobody has
    let Some(Row::Transaction(good)) = incoming.rows.get(&Table::Transactions).and_then(|r| r.first()).cloned() else { panic!("no transaction") };
    let mut bad = good.clone();
    bad.sync_id = "ffffffffffffffffffffffffffffffff".into();
    bad.category = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into();
    incoming.rows.get_mut(&Table::Transactions).unwrap().push(Row::Transaction(bad));
    let before = snapshot(&phone).unwrap();
    assert!(matches!(engine::apply(&mut phone, &incoming, now(), None), Err(SyncError::Invalid("reference"))));
    assert_eq!(snapshot(&phone).unwrap(), before, "the good row was not kept either");
    assert_eq!(count(&phone, "SELECT applying FROM sync_clock"), 0);
}

#[test]
fn a_row_that_breaks_a_rule_of_the_database_is_refused_without_a_trace() {
    let (desktop, mut phone) = paired();
    let mut incoming = rows_to(&desktop, &phone);
    incoming.rows.clear();
    // a budget whose type does not match its category's type
    let p_food = text(&phone, "SELECT sync_id FROM categories WHERE name = 'Food'");
    incoming.rows.insert(
        Table::Budgets,
        vec![Row::Budget(wire::BudgetRow {
            sync_id: "abababababababababababababababab".into(),
            stamp: Stamp::new(now(), 9, "abcdef01").format(),
            kind: "INCOME".into(),
            scope: "CATEGORY".into(),
            category: Some(p_food),
            amount_limit_minor: Some(100),
            effective_from: "2026-09-01".into(),
            created_at: "2026-03-01T10:00:00.000Z".into(),
            updated_at: "2026-03-01T10:00:00.000Z".into(),
        })],
    );
    let before = snapshot(&phone).unwrap();
    assert!(matches!(engine::apply(&mut phone, &incoming, now(), None), Err(SyncError::Invalid(_))));
    assert_eq!(snapshot(&phone).unwrap(), before);
}

#[test]
fn a_category_cannot_change_its_type_through_a_sync() {
    let (desktop, mut phone) = paired();
    let mut incoming = rows_to(&desktop, &phone);
    incoming.rows.clear();
    let food: Vec<Row> = {
        let wanted = text(&phone, "SELECT sync_id FROM categories WHERE name = 'Food'");
        let mut row = match rows_to(&desktop, &device("EUR")).rows.remove(&Table::Categories).unwrap().into_iter().find(|r| r.sync_id() == wanted).unwrap() {
            Row::Category(c) => c,
            _ => unreachable!(),
        };
        row.kind = "INCOME".into();
        row.stamp = Stamp::new(now() + 1000, 0, "abcdef01").format();
        vec![Row::Category(row)]
    };
    incoming.rows.insert(Table::Categories, food);
    assert!(matches!(engine::apply(&mut phone, &incoming, now(), None), Err(SyncError::Invalid("category"))));
}

#[test]
fn a_stamp_from_the_far_future_is_refused() {
    let (desktop, mut phone) = paired();
    let food = id_of(&desktop, "categories", "Food");
    spend(&desktop, food, "x", "1.00");
    let mut incoming = rows_to(&desktop, &phone);
    for row in incoming.rows.get_mut(&Table::Transactions).unwrap() {
        if let Row::Transaction(t) = row {
            t.stamp = Stamp::new(now() + 3 * 86_400_000, 0, "abcdef01").format();
        }
    }
    let before = snapshot(&phone).unwrap();
    assert!(matches!(engine::apply(&mut phone, &incoming, now(), None), Err(SyncError::ClockFarOff)));
    assert_eq!(snapshot(&phone).unwrap(), before);

    let marker = Incoming {
        rows: Default::default(),
        markers: [(Table::Transactions, vec![Marker { sync_id: "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd".into(), stamp: Stamp::new(now() + 3 * 86_400_000, 0, "abcdef01").format() }])].into(),
    };
    assert!(matches!(engine::apply(&mut phone, &marker, now(), None), Err(SyncError::ClockFarOff)));
}

#[test]
fn a_marker_for_a_row_this_device_never_had_is_remembered_so_the_row_cannot_come_back() {
    let (_desktop, mut phone) = paired();
    let id = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";
    let marker = Incoming { rows: Default::default(), markers: [(Table::Transactions, vec![Marker { sync_id: id.into(), stamp: Stamp::new(now(), 5, "abcdef01").format() }])].into() };
    engine::apply(&mut phone, &marker, now(), None).unwrap();
    assert_eq!(count(&phone, "SELECT count(*) FROM sync_deleted WHERE sync_id = 'cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd'"), 1);
    // an older row with that id now arrives: it loses
    let p_food = text(&phone, "SELECT sync_id FROM categories WHERE name = 'Food'");
    let late = Incoming {
        rows: [(
            Table::Transactions,
            vec![Row::Transaction(wire::TransactionRow {
                sync_id: id.into(),
                stamp: Stamp::new(now() - 1000, 0, "abcdef01").format(),
                category: p_food,
                recurring: None,
                recurring_due_date: None,
                client_uuid: None,
                name: "Ghost".into(),
                amount_minor: 100,
                planned_amount_minor: None,
                payment_date: "2026-03-01".into(),
                description: None,
                split: Vec::new(),
                created_at: "2026-03-01T10:00:00.000Z".into(),
                updated_at: "2026-03-01T10:00:00.000Z".into(),
            })],
        )]
        .into(),
        markers: Default::default(),
    };
    engine::apply(&mut phone, &late, now(), None).unwrap();
    assert_eq!(count(&phone, "SELECT count(*) FROM transactions WHERE name = 'Ghost'"), 0);
}

#[test]
fn the_collectors_insist_on_the_order_and_the_shape() {
    let (desktop, phone) = paired();
    let (dd, dp) = (engine::digest(&desktop).unwrap(), engine::digest(&phone).unwrap());
    let tables = vec![Table::Categories, Table::Transactions];

    let mut collector = engine::DigestCollector::new(tables.clone());
    let messages = engine::digest_messages(&dd, &tables);
    let Message::Digest(second) = messages.last().unwrap().clone() else { panic!() };
    assert!(collector.push(second).is_err(), "the second table cannot come first");

    let mut collector = engine::DigestCollector::new(tables.clone());
    for m in &messages {
        if let Message::Digest(d) = m {
            collector.push(d.clone()).unwrap();
        }
    }
    assert!(collector.is_complete());
    assert_eq!(collector.finish().unwrap().0.get(&Table::Categories), dd.0.get(&Table::Categories));

    // the same id twice in one table is refused
    let mut collector = engine::DigestCollector::new(vec![Table::Categories]);
    let id = "0123456789abcdef0123456789abcdef".to_string();
    let stamp = Stamp::new(1, 0, "abcdef01").format();
    let twice = wire::DigestMessage { table: Table::Categories, entries: vec![(id.clone(), stamp.clone(), 0), (id, stamp, 0)], last: true };
    assert!(collector.push(twice).is_err());

    // not finished is not finished
    let collector = engine::DigestCollector::new(vec![Table::Categories]);
    assert!(collector.finish().is_err());
    let _ = dp;
}

#[test]
fn a_table_that_is_the_same_on_both_devices_is_left_out_of_the_comparison() {
    let (mut desktop, mut phone) = paired();
    let food = id_of(&desktop, "categories", "Food");
    spend(&desktop, food, "Only a payment", "1.00");
    let outcome = sync_ok(&mut desktop, &mut phone);
    assert_eq!(outcome.compared, vec![Table::Transactions], "only the table that differs is listed and sent");
    assert!(outcome.messages < 12, "{} messages", outcome.messages);
}

#[test]
fn a_large_database_travels_in_frames_that_fit_and_converges() {
    let mut desktop = device("EUR");
    let mut phone = new_phone(&desktop);
    let food = category(&desktop, "Food", CategoryType::Expense);
    desktop.execute_batch("BEGIN").unwrap();
    for i in 0..3000 {
        desktop
            .execute(
                "INSERT INTO transactions (category_id, name, amount_minor, payment_date, description) VALUES (?1, ?2, ?3, '2026-03-02', 'a description of some length to make the rows less small')",
                params![food, format!("payment {i}"), 100 + i],
            )
            .unwrap();
    }
    desktop.execute_batch("COMMIT").unwrap();
    let outcome = sync_ok(&mut desktop, &mut phone);
    assert_eq!(count(&phone, "SELECT count(*) FROM transactions"), 3000);
    assert!(outcome.messages > 15 && outcome.bytes > 300_000, "{} messages, {} bytes", outcome.messages, outcome.bytes);
}

// --- random edits ---------------------------------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

const NAMES: &[&str] = &["Food", "Rent", "Gym", "Fun", "Pets", "Cars", "Books"];
const MONTHS: &[&str] = &["2026-03", "2026-04", "2026-05"];
const UUIDS: &[&str] = &["11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222"];

fn ids(conn: &Connection, sql: &str) -> Vec<i64> {
    conn.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

/// One thing a person might do on a screen, chosen at random. Refusals (a name taken, a bill with no amount) are part of life.
fn edit(conn: &mut Connection, rng: &mut Rng) {
    let kind = if rng.below(4) == 0 { CategoryType::Income } else { CategoryType::Expense };
    let categories_now = ids(conn, "SELECT id FROM categories WHERE deleted_at IS NULL AND type = 'EXPENSE'");
    let any_category = ids(conn, "SELECT id FROM categories WHERE deleted_at IS NULL");
    match rng.below(31) {
        0 => {
            let _ = categories::create(conn, rng.pick(NAMES), kind);
        }
        14..=20 => edit_documents(conn, rng),
        21..=30 => edit_debts(conn, rng),
        1 if !any_category.is_empty() => {
            let _ = categories::rename(conn, *rng.pick(&any_category), rng.pick(NAMES));
        }
        2 if !any_category.is_empty() && rng.below(3) == 0 => {
            let _ = categories::delete(conn, *rng.pick(&any_category), chrono::Utc::now());
        }
        3..=5 if !categories_now.is_empty() => {
            let uuid = if rng.below(6) == 0 { Some(rng.pick(UUIDS).to_string()) } else { None };
            let _ = transactions::create(
                conn,
                &NewTransaction {
                    name: format!("payment {}", rng.below(1000)),
                    amount: format!("{}.{:02}", 1 + rng.below(500), rng.below(100)),
                    payment_date: format!("2026-03-{:02}", 1 + rng.below(28)),
                    category_id: rng.pick(&categories_now).to_string(),
                    description: if rng.below(3) == 0 { Some("a note".into()) } else { None },
                    client_uuid: uuid,
                },
            );
        }
        6 => {
            let all = ids(conn, "SELECT id FROM transactions");
            if !all.is_empty() {
                let id = *rng.pick(&all);
                // through the app's own function, so a shared bill is scaled with its new amount
                let patch = transactions::TransactionPatch {
                    name: Some(format!("edited {}", rng.below(1000))),
                    amount: Some(format!("{}.{:02}", 1 + rng.below(1000), rng.below(100))),
                    ..Default::default()
                };
                let _ = transactions::update(conn, id, &patch);
            }
        }
        7 => {
            let all = ids(conn, "SELECT id FROM transactions");
            if !all.is_empty() {
                let _ = transactions::delete(conn, *rng.pick(&all));
            }
        }
        8 if !categories_now.is_empty() => {
            let global = rng.below(3) == 0;
            let limit = if rng.below(5) == 0 { None } else { Some(format!("{}.00", 50 + rng.below(2000))) };
            let _ = budgets::set(
                conn,
                &SetBudget {
                    kind: CategoryType::Expense,
                    scope: if global { Scope::Global } else { Scope::Category },
                    month: rng.pick(MONTHS).to_string(),
                    amount_limit: limit,
                    category_id: if global { None } else { Some(rng.pick(&categories_now).to_string()) },
                },
                chrono::Utc::now(),
            );
        }
        9 if !categories_now.is_empty() => {
            let _ = recurring::create(
                conn,
                &NewRecurring {
                    name: format!("bill {}", rng.below(5)),
                    amount: if rng.below(4) == 0 { None } else { Some("19.99".into()) },
                    category_id: rng.pick(&categories_now).to_string(),
                    start_date: "2026-03-01".into(),
                    interval_unit: *rng.pick(&[Unit::Day, Unit::Week, Unit::Month]),
                    interval_count: 1 + rng.below(3) as u32,
                    end_date: None,
                    description: None,
                },
            );
        }
        10 | 11 => {
            let items = ids(conn, "SELECT id FROM recurring_transactions WHERE is_active = 1");
            if !items.is_empty() {
                let _ = recurring::pay(conn, *rng.pick(&items), Some("12.50"), Some("2026-03-20"), TODAY);
            }
        }
        12 => {
            let items = ids(conn, "SELECT id FROM recurring_transactions");
            if !items.is_empty() {
                let id = *rng.pick(&items);
                match rng.below(4) {
                    0 => {
                        let _ = recurring::delete(conn, id);
                    }
                    1 => {
                        let _ = recurring::skip(conn, id);
                    }
                    2 => {
                        let _ = recurring::set_active(conn, id, rng.below(2) == 0);
                    }
                    _ => {
                        conn.execute("UPDATE recurring_transactions SET name = ?2 WHERE id = ?1", params![id, format!("renamed {}", rng.below(5))]).unwrap();
                    }
                }
            }
        }
        13 => {
            let zone = rng.pick(&["UTC", "Europe/Dublin", "Europe/Bucharest"]);
            let locale = rng.pick(&["en-IE", "de-DE", "ro-RO"]);
            conn.execute("UPDATE settings SET timezone = ?1, locale = ?2", params![zone, locale]).unwrap();
        }
        _ => {}
    }
}

/// Something a person might do with documents and their links. A document has a row and no file here: the rows are what the sync of
/// this batch moves. Names are few, so devices often make the same one, and the hash comes from the name, so they often make the
/// same file too; the choices of ids are what make two devices link the same two things.
fn edit_documents(conn: &mut Connection, rng: &mut Rng) {
    const DOCUMENTS: &[&str] = &["Warranty.pdf", "Receipt.png", "Contract.pdf", "Invoice.pdf"];
    let all = ids(conn, "SELECT id FROM documents");
    let transactions_now = ids(conn, "SELECT id FROM transactions");
    let items = ids(conn, "SELECT id FROM recurring_transactions");
    match rng.below(8) {
        0 | 1 => {
            let name = rng.pick(DOCUMENTS);
            conn.execute(
                "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date, keep_on_phone) VALUES (?1, 'application/pdf', ?2, ?3, ?4, ?5)",
                params![name, 100 + rng.below(3) as i64, sha_of(name), format!("2026-03-{:02}", 1 + rng.below(28)), rng.below(2) as i64],
            )
            .unwrap();
        }
        2 if !all.is_empty() => {
            let _ = conn.execute("UPDATE documents SET name = ?2 WHERE id = ?1", params![rng.pick(&all), rng.pick(DOCUMENTS)]);
        }
        3 if !all.is_empty() => {
            conn.execute("UPDATE documents SET keep_on_phone = ?2, document_date = ?3 WHERE id = ?1", params![rng.pick(&all), rng.below(2) as i64, format!("2026-04-{:02}", 1 + rng.below(28))]).unwrap();
        }
        4 if !all.is_empty() && rng.below(2) == 0 => {
            conn.execute("DELETE FROM documents WHERE id = ?1", [rng.pick(&all)]).unwrap();
        }
        5 | 6 if !all.is_empty() => {
            let doc = *rng.pick(&all);
            let target = if !items.is_empty() && (transactions_now.is_empty() || rng.below(3) == 0) {
                LinkTarget { kind: TargetKind::Recurring, id: rng.pick(&items).to_string() }
            } else if !transactions_now.is_empty() {
                LinkTarget { kind: TargetKind::Transaction, id: rng.pick(&transactions_now).to_string() }
            } else {
                return;
            };
            let _ = library::link(conn, doc, &target);
        }
        7 if !all.is_empty() => {
            let links: Vec<(i64, Option<i64>, Option<i64>)> = {
                let mut stmt = conn.prepare("SELECT document_id, transaction_id, recurring_transaction_id FROM document_links").unwrap();
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
            };
            if !links.is_empty() {
                let (doc, tx, rec) = *rng.pick(&links);
                let target = match (tx, rec) {
                    (Some(id), _) => LinkTarget { kind: TargetKind::Transaction, id: id.to_string() },
                    (_, Some(id)) => LinkTarget { kind: TargetKind::Recurring, id: id.to_string() },
                    _ => return,
                };
                library::unlink(conn, doc, &target).unwrap();
            }
        }
        _ => {}
    }
}

const FRIENDS: &[&str] = &["Anna", "Bob", "Carl", "Dina"];

/// Something a person might do with friends, the ledger and shared bills. Few names, so the two devices often make the same friend
/// (which becomes one) and often give two friends one name (which is refused).
fn edit_debts(conn: &mut Connection, rng: &mut Rng) {
    use crate::debts::splits::{Mode, Part, SplitInput};
    let everyone = ids(conn, "SELECT id FROM people");
    let active = ids(conn, "SELECT id FROM people WHERE archived = 0");
    let bills = ids(conn, "SELECT t.id FROM transactions t JOIN categories c ON c.id = t.category_id WHERE c.type = 'EXPENSE'");
    let items = ids(conn, "SELECT r.id FROM recurring_transactions r JOIN categories c ON c.id = r.category_id WHERE c.type = 'EXPENSE'");
    let entries = ids(conn, "SELECT id FROM ledger_entries");
    let parts = |rng: &mut Rng| -> Vec<Part> {
        let mut out: Vec<Part> = Vec::new();
        let mut taken = HashSet::new();
        for _ in 0..(1 + rng.below(3)) {
            if rng.below(4) == 0 {
                out.push(Part { person_id: None, guest: Some(rng.pick(&["Walk-in", "Eve"]).to_string()), value: None });
            } else if !active.is_empty() {
                let id = *rng.pick(&active);
                if taken.insert(id) {
                    out.push(Part { person_id: Some(id.to_string()), guest: None, value: None });
                }
            }
        }
        let mut seen = HashSet::new();
        out.retain(|p| seen.insert((p.person_id.clone(), p.guest.clone())));
        out
    };
    let split = |rng: &mut Rng, mut parts: Vec<Part>| -> SplitInput {
        if rng.below(2) == 0 {
            SplitInput { mode: Mode::Equal, mine: None, parts }
        } else {
            for p in &mut parts {
                p.value = Some(rng.below(5).to_string());
            }
            SplitInput { mode: Mode::Shares, mine: Some((1 + rng.below(4)).to_string()), parts }
        }
    };
    match rng.below(16) {
        0 | 1 => {
            let _ = people::create(conn, rng.pick(FRIENDS));
        }
        2 if !everyone.is_empty() => {
            let _ = people::rename(conn, *rng.pick(&everyone), rng.pick(FRIENDS));
        }
        3 if !active.is_empty() => {
            let _ = people::archive(conn, *rng.pick(&active));
        }
        4 if !everyone.is_empty() => {
            let _ = people::restore(conn, *rng.pick(&everyone));
        }
        5 if !everyone.is_empty() => {
            let on = (rng.below(3) != 0).then(|| format!("2026-04-{:02}", 1 + rng.below(28)));
            let every = on.as_ref().and_then(|_| (rng.below(2) == 0).then(|| 1 + rng.below(30) as u32));
            let _ = people::set_reminder(conn, *rng.pick(&everyone), on.as_deref(), every);
        }
        6 | 7 if !everyone.is_empty() => {
            let kind = *rng.pick(&[ledger::Kind::Lent, ledger::Kind::Borrowed, ledger::Kind::RepaidToMe, ledger::Kind::RepaidByMe]);
            let all_bills = ids(conn, "SELECT id FROM transactions");
            let link = (rng.below(3) == 0 && !all_bills.is_empty()).then(|| rng.pick(&all_bills).to_string());
            let _ = ledger::add(
                conn,
                &ledger::NewEntry {
                    person_id: rng.pick(&everyone).to_string(),
                    kind,
                    amount: format!("{}.{:02}", 1 + rng.below(200), rng.below(100)),
                    date: format!("2026-03-{:02}", 1 + rng.below(28)),
                    note: (rng.below(2) == 0).then(|| format!("note {}", rng.below(4))),
                    transaction_id: link,
                },
            );
        }
        8 if !entries.is_empty() => {
            let patch = ledger::EntryPatch {
                amount: Some(format!("{}.00", 1 + rng.below(300))),
                person_id: (!everyone.is_empty() && rng.below(3) == 0).then(|| rng.pick(&everyone).to_string()),
                note: Some((rng.below(2) == 0).then(|| "edited".to_string())),
                ..Default::default()
            };
            let _ = ledger::update(conn, *rng.pick(&entries), &patch);
        }
        9 if !entries.is_empty() => {
            let _ = ledger::delete(conn, *rng.pick(&entries));
        }
        10 | 11 if !bills.is_empty() => {
            let id = *rng.pick(&bills);
            let p = parts(rng);
            let _ = splits::set_split(conn, id, &split(rng, p));
        }
        12 if !bills.is_empty() => {
            if rng.below(2) == 0 {
                let _ = splits::clear_split(conn, *rng.pick(&bills));
            } else {
                let patch = transactions::TransactionPatch { amount: Some(format!("{}.00", 1 + rng.below(400))), ..Default::default() };
                let _ = transactions::update(conn, *rng.pick(&bills), &patch);
            }
        }
        13 if !items.is_empty() => {
            let id = *rng.pick(&items);
            if rng.below(4) == 0 {
                let _ = splits::clear_template(conn, id);
            } else {
                let p = parts(rng);
                let _ = splits::set_template(conn, id, &SplitInput { mode: Mode::Shares, mine: Some((1 + rng.below(3)).to_string()), parts: p.into_iter().map(|mut x| { x.value = Some((1 + rng.below(3)).to_string()); x }).collect() });
            }
        }
        14 if !items.is_empty() => {
            let _ = recurring::pay(conn, *rng.pick(&items), Some("30.00"), Some("2026-03-21"), TODAY);
        }
        15 if !entries.is_empty() => {
            let docs = ids(conn, "SELECT id FROM documents");
            if !docs.is_empty() {
                let _ = library::link(conn, *rng.pick(&docs), &LinkTarget { kind: TargetKind::LedgerEntry, id: rng.pick(&entries).to_string() });
            }
        }
        _ => {}
    }
}

/// How many random cases to run. `MEOWNY_SYNC_SEEDS=2000 cargo test sync::engine_tests::random` hunts for rare ones.
fn seeds(default: u64) -> u64 {
    std::env::var("MEOWNY_SYNC_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// The first seed to run. `MEOWNY_SYNC_FROM=1774 MEOWNY_SYNC_SEEDS=1` runs the one seed that a hunt reported.
fn first_seed(default: u64) -> u64 {
    std::env::var("MEOWNY_SYNC_FROM").ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
fn random_edits_on_both_devices_always_end_the_same_after_one_sync() {
    for seed in first_seed(1)..first_seed(1) + seeds(60) {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut desktop = device("EUR");
        let mut phone = device("EUR");
        for _ in 0..25 {
            edit(&mut desktop, &mut rng);
        }
        sync_ok(&mut desktop, &mut phone);
        for round in 0..3 {
            for _ in 0..(5 + rng.below(25)) {
                edit(&mut desktop, &mut rng);
            }
            // Two edits of one row, made on two devices in the same millisecond and with the same counter, cannot be told apart once a
            // category merge has moved both (the stamp of a moved row has no device part, `engine::moved`). A person cannot make them
            // and a loop can, so the two devices do not edit in the same millisecond here (docs/sync-design.md, section 11).
            pause();
            for _ in 0..(5 + rng.below(25)) {
                edit(&mut phone, &mut rng);
            }
            if rng.below(2) == 0 {
                pause();
            }
            let (before_desktop, before_phone) = (snapshot(&desktop).unwrap(), snapshot(&phone).unwrap());
            let outcome = sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap_or_else(|e| panic!("seed {seed} round {round}: {e}"));
            if snapshot(&desktop).unwrap() != snapshot(&phone).unwrap() {
                // show what each device held before, for the rows that ended up different
                let (x, y) = (snapshot(&desktop).unwrap(), snapshot(&phone).unwrap());
                for line in x.iter().filter(|l| !y.contains(l)).chain(y.iter().filter(|l| !x.contains(l))) {
                    let id: String = line.split(|c: char| !c.is_ascii_hexdigit()).find(|part| part.len() == 32).unwrap_or("").to_string();
                    for (who, before) in [("desktop before", &before_desktop), ("phone before", &before_phone)] {
                        for l in before.iter().filter(|l| l.contains(&id)) {
                            eprintln!("{who}: {}", l.chars().take(330).collect::<String>());
                        }
                    }
                }
            }
            assert_same(&desktop, &phone, &format!("seed {seed} round {round} ({outcome:?})"));
            assert!(!outcome.differences, "seed {seed} round {round}: differ after one sync: {outcome:?}");
            let again = sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap();
            assert!(again.compared.is_empty() && again.first.is_empty() && again.second.is_empty(), "seed {seed} round {round}: not settled: {again:?}");
            for conn in [&desktop, &phone] {
                assert_eq!(count(conn, "SELECT count(*) FROM pragma_foreign_key_check"), 0, "seed {seed}");
                assert_eq!(count(conn, "SELECT applying FROM sync_clock"), 0);
                assert_eq!(text(conn, "PRAGMA integrity_check"), "ok");
                assert_eq!(count(conn, "SELECT count(*) FROM (SELECT name, type FROM categories WHERE deleted_at IS NULL GROUP BY name, type HAVING count(*) > 1)"), 0, "seed {seed}: two active categories of one name");
            }
        }
    }
}

#[test]
fn random_edits_with_a_currency_change_in_between_still_converge() {
    for seed in first_seed(100)..first_seed(100) + seeds(20) {
        let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
        let mut desktop = device("EUR");
        let mut phone = device("EUR");
        for _ in 0..20 {
            edit(&mut desktop, &mut rng);
        }
        sync_ok(&mut desktop, &mut phone);
        pause();
        for _ in 0..10 {
            edit(&mut phone, &mut rng);
        }
        currency::change(&mut desktop, "USD", Some("1.07"), TODAY).unwrap();
        for _ in 0..10 {
            edit(&mut desktop, &mut rng);
        }
        let outcome = sync(&mut desktop, &mut phone, TODAY, now(), None);
        match outcome {
            Ok(o) => {
                assert_same(&desktop, &phone, &format!("seed {seed} ({o:?})"));
                assert!(!o.differences, "seed {seed}: {o:?}");
                assert_eq!(text(&phone, "SELECT currency FROM settings"), "USD");
            }
            Err(e) => panic!("seed {seed}: {e}"),
        }
    }
}

// --- what stays on a device ---------------------------------------------------------------------------------------------

/// Tables whose rows are sent to the other device (the settings only in their shared part), and tables that never are.
///
/// Phase 10 (debts): `people` and `ledger_entries` are shared like the rest; the two tables of parts belong to their transaction and to
/// their recurring item and travel inside their rows.
const SHARED_TABLES: [&str; 13] = [
    "settings",
    "categories",
    "budgets",
    "recurring_transactions",
    "transactions",
    "documents",
    "document_links",
    "people",
    "ledger_entries",
    "transaction_split_lines",
    "recurring_split_lines",
    "sync_deleted",
    "currency_changes",
];
const LOCAL_TABLES: [&str; 5] = ["audit_log", "api_tokens", "backup_schedule", "sync_clock", "sync_peers"];

#[test]
fn every_table_is_either_shared_or_stays_on_its_device() {
    let conn = device("EUR");
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
    let names: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert!(names.len() >= 12, "the tables were not found: {names:?}");
    for name in &names {
        let shared = SHARED_TABLES.contains(&name.as_str());
        let local = LOCAL_TABLES.contains(&name.as_str());
        assert!(shared || local, "the table {name} is new: decide whether the sync sends it (SHARED_TABLES) or it stays on its device (LOCAL_TABLES), and say so in docs/sync-design.md section 9");
        assert!(!(shared && local), "{name} is in both lists");
    }
    for listed in SHARED_TABLES.iter().chain(LOCAL_TABLES.iter()) {
        assert!(names.iter().any(|n| n == listed), "{listed} is listed but there is no such table");
    }
    // The sync can only name nine tables of rows (the parts of bills and the weights of templates go inside two of them), so nothing
    // else can be asked for or sent.
    assert_eq!(wire::Table::ALL.len(), 9);
}

#[test]
fn nothing_that_stays_on_a_device_is_ever_sent() {
    let (mut desktop, mut phone) = paired();
    let sentinel = |tag: &str| format!("SECRET-{tag}-9f3a1c");
    for (conn, tag) in [(&desktop, "DESKTOP"), (&phone, "PHONE")] {
        let s = sentinel(tag);
        conn.execute("INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values) VALUES ('thing', '1', 'UPDATE', ?1, ?1)", [&s]).unwrap();
        conn.execute("INSERT INTO api_tokens (name, scope, token_hash, created_at, expires_at) VALUES (?1, 'READ', ?2, '2026-01-01T00:00:00Z', '2027-01-01T00:00:00Z')", params![s, "ab".repeat(32)]).unwrap();
        conn.execute("INSERT INTO backup_schedule (id, folder, every_days, passphrase) VALUES (1, ?1, 7, ?1)", [&s]).unwrap();
        let id = if tag == "DESKTOP" { "aa".repeat(16) } else { "bb".repeat(16) };
        conn.execute("INSERT INTO sync_peers (device_id, name, secret, paired_at) VALUES (?1, ?2, ?3, '2026-01-01T00:00:00.000Z')", params![id, s, "cd".repeat(32)]).unwrap();
    }
    // The language belongs to the device too.
    desktop.execute("UPDATE settings SET language = 'de'", []).unwrap();
    phone.execute("UPDATE settings SET language = 'ro'", []).unwrap();
    // Something to send in both directions, so that a transcript is there to look through.
    let food = id_of(&desktop, "categories", "Food");
    spend(&desktop, food, "From the desktop", "4.00");
    let phone_food = id_of(&phone, "categories", "Food");
    spend(&phone, phone_food, "From the phone", "5.00");

    let outcome = sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap();
    assert!(!outcome.differences);
    let said = String::from_utf8_lossy(&outcome.transcript).to_string();
    assert!(said.contains("From the desktop") && said.contains("From the phone"), "the transcript has nothing in it: {} bytes", outcome.transcript.len());
    for tag in ["DESKTOP", "PHONE"] {
        assert!(!said.contains(&sentinel(tag)), "something that stays on the {tag} was sent");
    }
    assert!(!said.contains(&"ab".repeat(32)), "a token hash was sent");
    assert!(!said.contains(&"cd".repeat(32)), "a pairing secret was sent");
    // And nothing arrived in the tables of the other side.
    assert_eq!(count(&desktop, "SELECT count(*) FROM audit_log WHERE new_values LIKE '%SECRET-PHONE%'"), 0);
    assert_eq!(count(&phone, "SELECT count(*) FROM audit_log WHERE new_values LIKE '%SECRET-DESKTOP%'"), 0);
    assert_eq!(count(&desktop, "SELECT count(*) FROM api_tokens WHERE name LIKE '%PHONE%'"), 0);
    assert_eq!(count(&phone, "SELECT count(*) FROM api_tokens WHERE name LIKE '%DESKTOP%'"), 0);
    assert_eq!(text(&desktop, "SELECT language FROM settings"), "de");
    assert_eq!(text(&phone, "SELECT language FROM settings"), "ro");
}

// --- finding the conflicts ----------------------------------------------------------------------------------------------

fn listed(entries: &[(&str, &str, bool)]) -> engine::Digest {
    let mut digest = engine::Digest::default();
    let mut table = wire::Entries::new();
    for (id, stamp, deleted) in entries {
        table.insert(format!("{:0>32}", id), (stamp.to_string(), *deleted));
    }
    digest.0.insert(Table::Transactions, table);
    digest
}

fn at(ms: u64, device: &str) -> String {
    format!("{ms:013}-0000-{device}")
}

#[test]
fn only_what_both_sides_changed_after_they_last_met_is_a_conflict() {
    let tables = [Table::Transactions];
    let met = Some(1_000);
    let found = |a: &[(&str, &str, bool)], b: &[(&str, &str, bool)], threshold| engine::conflicts(&listed(a), &listed(b), &tables, threshold).0.len();
    let (old_a, old_b) = (at(500, "aaaaaaaa"), at(600, "bbbbbbbb"));
    let (new_a, new_b) = (at(2_000, "aaaaaaaa"), at(3_000, "bbbbbbbb"));
    assert_eq!(found(&[("1", &new_a, false)], &[("1", &new_b, false)], met), 1, "changed on both after the last sync");
    assert_eq!(found(&[("1", &new_a, false)], &[("1", &old_b, false)], met), 0, "only one side changed it");
    assert_eq!(found(&[("1", &old_a, false)], &[("1", &old_b, false)], met), 0, "neither changed it since");
    assert_eq!(found(&[("1", &new_a, false)], &[("1", &new_a, false)], met), 0, "the same version on both");
    assert_eq!(found(&[("1", &new_a, true)], &[("1", &new_b, true)], met), 0, "deleted on both");
    assert_eq!(found(&[("1", &new_a, true)], &[("1", &new_b, false)], met), 1, "deleted on one side and changed on the other");
    assert_eq!(found(&[("1", &new_a, false)], &[], met), 0, "a row only one side has is not a conflict");
    assert_eq!(found(&[("1", &new_a, false)], &[("1", &new_b, false)], None), 0, "devices that never met have nothing to fight over");
    assert_eq!(engine::conflict_threshold(Some(5), Some(9)), Some(9));
    assert_eq!(engine::conflict_threshold(None, Some(9)), Some(9));
    assert_eq!(engine::conflict_threshold(None, None), None);
}

#[test]
fn a_huge_number_of_conflicts_is_cut_short_and_the_rest_is_counted() {
    let many = engine::MAX_CONFLICTS + 37;
    let (mut a, mut b) = (Vec::new(), Vec::new());
    let (sa, sb) = (at(2_000, "aaaaaaaa"), at(3_000, "bbbbbbbb"));
    let ids: Vec<String> = (0..many).map(|i| format!("{i:x}")).collect();
    for id in &ids {
        a.push((id.as_str(), sa.as_str(), false));
        b.push((id.as_str(), sb.as_str(), false));
    }
    let (found, more) = engine::conflicts(&listed(&a), &listed(&b), &[Table::Transactions], Some(1_000));
    assert_eq!((found.len(), more), (engine::MAX_CONFLICTS, 37));
    // both sides work out the same list, in the same order
    let (again, _) = engine::conflicts(&listed(&b), &listed(&a), &[Table::Transactions], Some(1_000));
    assert_eq!(found.iter().map(|c| c.id.clone()).collect::<Vec<_>>(), again.iter().map(|c| c.id.clone()).collect::<Vec<_>>());
}

#[test]
fn the_choices_only_change_stamps_never_what_a_row_says() {
    let mut conn = device("EUR");
    let food = category(&conn, "Food", CategoryType::Expense);
    let id = spend(&conn, food, "Coffee", "3.50");
    let sync_id: String = conn.query_row("SELECT sync_id FROM transactions WHERE id = ?1", [id], |r| r.get(0)).unwrap();
    let before = text(&conn, "SELECT stamp FROM transactions");
    let conflict = |deleted| engine::Conflict { table: Table::Transactions, id: sync_id.clone(), mine_stamp: before.clone(), mine_deleted: deleted, theirs_stamp: String::new(), theirs_deleted: false };
    engine::decide(&mut conn, &[conflict(false)], &[engine::Choice::Mine]).unwrap();
    let mine = text(&conn, "SELECT stamp FROM transactions");
    assert!(mine > before, "the chosen version got a newer stamp");
    engine::decide(&mut conn, &[conflict(false)], &[engine::Choice::Theirs]).unwrap();
    assert_eq!(text(&conn, "SELECT stamp FROM transactions"), engine::UNSET_STAMP, "the other version will be the newer one");
    assert_eq!(count(&conn, "SELECT amount_minor FROM transactions"), 350);
    assert!(engine::decide(&mut conn, &[conflict(false)], &[]).is_err(), "one choice for each conflict");
    // the changes of a decision are not changes of the person's: no row is marked as changed on this device by it
    assert_eq!(count(&conn, "SELECT applying FROM sync_clock"), 0);
}



// --- documents and their links ------------------------------------------------------------------------------------------------
//
// The rows only: the files of the documents are not here (docs/documents-design.md, sections 9 and 10).

fn sha_of(name: &str) -> String {
    use sha2::{Digest, Sha256};
    crate::keys::hex_encode(&Sha256::digest(name.as_bytes()))
}

/// A document: its row, with the hash made from its name, so the same name on two devices is the same file.
fn document(conn: &Connection, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO documents (name, media_type, size_bytes, sha256, document_date) VALUES (?1, 'application/pdf', 1000, ?2, '2026-03-02')",
        params![name, sha_of(name)],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn to_transaction(id: i64) -> LinkTarget {
    LinkTarget { kind: TargetKind::Transaction, id: id.to_string() }
}

fn to_item(id: i64) -> LinkTarget {
    LinkTarget { kind: TargetKind::Recurring, id: id.to_string() }
}

fn attach(conn: &Connection, document_name: &str, target: LinkTarget) {
    library::link(conn, id_of(conn, "documents", document_name), &target).unwrap();
}

fn links(conn: &Connection) -> i64 {
    count(conn, "SELECT count(*) FROM document_links")
}

fn link_markers(conn: &Connection) -> i64 {
    count(conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'document_links'")
}

fn documents_named(conn: &Connection) -> Vec<String> {
    let mut stmt = conn.prepare("SELECT name FROM documents ORDER BY name").unwrap();
    stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn a_document_added_on_both_devices_is_kept_both_times_even_with_the_same_file() {
    let (mut desktop, mut phone) = paired();
    document(&desktop, "Receipt.pdf");
    document(&phone, "Receipt.pdf");
    sync_ok(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(count(conn, "SELECT count(*) FROM documents WHERE name = 'Receipt.pdf'"), 2, "two documents that happen to hold the same bytes");
        assert_eq!(count(conn, "SELECT count(DISTINCT sha256) FROM documents"), 1);
    }
}

#[test]
fn a_document_deleted_on_one_device_goes_from_the_other_with_its_links_and_leaves_no_marker_for_them() {
    let (mut desktop, mut phone) = paired();
    document(&desktop, "Warranty.pdf");
    document(&desktop, "Keep.pdf");
    attach(&desktop, "Warranty.pdf", to_transaction(id_of(&desktop, "transactions", "Coffee")));
    attach(&desktop, "Warranty.pdf", to_item(id_of(&desktop, "recurring_transactions", "Rent")));
    attach(&desktop, "Keep.pdf", to_transaction(id_of(&desktop, "transactions", "Tea")));
    sync_ok(&mut desktop, &mut phone);
    assert_eq!(links(&phone), 3);

    pause();
    library::delete(&phone, &files_of(&phone), id_of(&phone, "documents", "Warranty.pdf")).unwrap();
    sync_ok(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(documents_named(conn), ["Keep.pdf"]);
        assert_eq!(links(conn), 1);
        assert_eq!(count(conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'documents'"), 1);
        assert_eq!(link_markers(conn), 0, "the document's marker says it; its links need none");
        assert_eq!(count(conn, "SELECT count(*) FROM transactions"), 3, "what it was linked to stays: Coffee, Tea and the payment of the rent");
    }
}

/// The folder and keys of a device that has no files: `delete` removes a file that is not there, which is not an error.
fn files_of(_conn: &Connection) -> library::Files<'static> {
    use std::sync::OnceLock;
    static STORE: OnceLock<(crate::documents::Store, crate::documents::DocKeys, tempfile::TempDir)> = OnceLock::new();
    let (store, keys, _) = STORE.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        (crate::documents::Store::new(dir.path()), crate::documents::DocKeys::from_db_key(&crate::db::DbKey::from_bytes([1; 32])), dir)
    });
    library::Files { store, keys }
}

#[test]
fn the_newer_of_an_edit_and_a_delete_of_a_document_wins_and_a_document_that_comes_back_brings_its_links() {
    // the delete is newer: the document goes
    let (mut desktop, mut phone) = paired();
    document(&desktop, "Warranty.pdf");
    attach(&desktop, "Warranty.pdf", to_transaction(id_of(&desktop, "transactions", "Coffee")));
    sync_ok(&mut desktop, &mut phone);
    pause();
    desktop.execute("UPDATE documents SET name = 'Edited.pdf'", []).unwrap();
    pause();
    phone.execute("DELETE FROM documents", []).unwrap();
    sync_ok(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!((count(conn, "SELECT count(*) FROM documents"), links(conn), link_markers(conn)), (0, 0, 0));
    }

    // the edit is newer: the document stays, and the phone's links come back with it
    let (mut desktop, mut phone) = paired();
    document(&desktop, "Warranty.pdf");
    attach(&desktop, "Warranty.pdf", to_transaction(id_of(&desktop, "transactions", "Coffee")));
    sync_ok(&mut desktop, &mut phone);
    pause();
    desktop.execute("DELETE FROM documents", []).unwrap();
    pause();
    phone.execute("UPDATE documents SET name = 'Edited.pdf'", []).unwrap();
    sync_ok(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(documents_named(conn), ["Edited.pdf"]);
        assert_eq!(links(conn), 1, "the link the phone still held came back with the document");
        assert_eq!(count(conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'documents'"), 0, "the marker of the delete is gone");
    }
}

#[test]
fn the_same_link_made_on_both_devices_becomes_one_and_both_end_with_the_same_marker() {
    let (mut desktop, mut phone) = paired();
    document(&desktop, "Warranty.pdf");
    sync_ok(&mut desktop, &mut phone);
    pause();
    attach(&desktop, "Warranty.pdf", to_transaction(id_of(&desktop, "transactions", "Coffee")));
    pause();
    attach(&phone, "Warranty.pdf", to_transaction(id_of(&phone, "transactions", "Coffee")));
    let phone_link: String = text(&phone, "SELECT sync_id FROM document_links");
    let outcome = sync_ok(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(links(conn), 1);
        assert_eq!(link_markers(conn), 1, "the loser left a marker, so the device that made it drops it too");
        assert_eq!(text(conn, "SELECT sync_id FROM document_links"), phone_link, "the newer one stays");
    }
    assert!(outcome.first.dropped + outcome.second.dropped >= 1, "{outcome:?}");
}

#[test]
fn a_link_that_a_person_removes_is_removed_on_the_other_device_and_can_be_made_again() {
    let (mut desktop, mut phone) = paired();
    document(&desktop, "Warranty.pdf");
    attach(&desktop, "Warranty.pdf", to_transaction(id_of(&desktop, "transactions", "Coffee")));
    sync_ok(&mut desktop, &mut phone);
    let first: String = text(&desktop, "SELECT sync_id FROM document_links");

    pause();
    library::unlink(&phone, id_of(&phone, "documents", "Warranty.pdf"), &to_transaction(id_of(&phone, "transactions", "Coffee"))).unwrap();
    sync_ok(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!((links(conn), link_markers(conn)), (0, 1), "removed on purpose: this is what a marker for a link means");
    }

    pause();
    attach(&desktop, "Warranty.pdf", to_transaction(id_of(&desktop, "transactions", "Coffee")));
    sync_ok(&mut desktop, &mut phone);
    for conn in [&desktop, &phone] {
        assert_eq!(links(conn), 1);
        assert_ne!(text(conn, "SELECT sync_id FROM document_links"), first, "a new link, which the old marker does not touch");
    }
}

/// What a device does when the parent of a link is deleted on one device while the other attaches a document to it, for every kind of
/// parent and each way round. All of them must end the same after one sync, with no link and no marker for one.
#[test]
fn a_parent_deleted_on_one_device_while_the_other_attaches_to_it_ends_the_same_after_one_sync() {
    for parent in ["document", "transaction", "recurring"] {
        for desktop_deletes in [true, false] {
            let (mut desktop, mut phone) = paired();
            document(&desktop, "Warranty.pdf");
            sync_ok(&mut desktop, &mut phone);
            pause();
            let (deleter, attacher) = if desktop_deletes { (&mut desktop, &mut phone) } else { (&mut phone, &mut desktop) };
            let target = |conn: &Connection| match parent {
                "recurring" => to_item(id_of(conn, "recurring_transactions", "Rent")),
                _ => to_transaction(id_of(conn, "transactions", "Coffee")),
            };
            attach(attacher, "Warranty.pdf", target(attacher));
            match parent {
                "document" => {
                    deleter.execute("DELETE FROM documents WHERE name = 'Warranty.pdf'", []).unwrap();
                }
                "transaction" => transactions::delete(deleter, id_of(deleter, "transactions", "Coffee")).unwrap(),
                _ => recurring::delete(deleter, id_of(deleter, "recurring_transactions", "Rent")).unwrap(),
            }
            pause();
            let label = format!("{parent}, deleted on the {}", if desktop_deletes { "desktop" } else { "phone" });
            let outcome = sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap_or_else(|e| panic!("{label}: {e}"));
            assert!(!outcome.differences, "{label}: {outcome:?}");
            assert_same(&desktop, &phone, &label);
            for conn in [&desktop, &phone] {
                assert_eq!((links(conn), link_markers(conn)), (0, 0), "{label}");
                assert_eq!(count(conn, "SELECT count(*) FROM documents"), if parent == "document" { 0 } else { 1 }, "{label}");
                assert_eq!(count(conn, "SELECT count(*) FROM pragma_foreign_key_check"), 0, "{label}");
            }
            let again = sync(&mut desktop, &mut phone, TODAY, now(), None).unwrap();
            assert!(again.compared.is_empty() && again.first.is_empty() && again.second.is_empty(), "{label}: {again:?}");
        }
    }
}

#[test]
fn a_link_to_a_document_this_device_deleted_is_not_taken_and_is_not_an_error() {
    let (mut desktop, mut phone) = paired();
    document(&desktop, "Warranty.pdf");
    sync_ok(&mut desktop, &mut phone);
    pause();
    attach(&phone, "Warranty.pdf", to_transaction(id_of(&phone, "transactions", "Coffee")));
    desktop.execute("DELETE FROM documents", []).unwrap();
    let incoming = rows_to(&phone, &desktop);
    assert_eq!(incoming.rows.get(&Table::DocumentLinks).map(Vec::len), Some(1));
    let summary = engine::apply(&mut desktop, &incoming, now(), None).unwrap();
    assert_eq!((summary.added, summary.kept_yours), (0, 1), "{summary:?}");
    assert_eq!(links(&desktop), 0);
}

#[test]
fn rows_that_point_at_nothing_this_device_ever_heard_of_stop_the_sync_and_change_nothing() {
    let (desktop, mut phone) = paired();
    let before = snapshot(&phone).unwrap();
    // a link whose document is unknown, and then one whose transaction is
    let mut source = desktop;
    let mut unknown_document = Incoming::default();
    unknown_document.rows.insert(
        Table::DocumentLinks,
        vec![Row::Link(wire::DocumentLinkRow {
            sync_id: "ab".repeat(16),
            stamp: stamp::tick(&source).unwrap().format(),
            document: "cd".repeat(16),
            transaction: Some(text(&source, "SELECT sync_id FROM transactions LIMIT 1")),
            recurring: None,
            ledger: None,
            created_at: "2026-03-01T10:00:00.000Z".into(),
        })],
    );
    assert!(matches!(engine::apply(&mut phone, &unknown_document, now(), None), Err(SyncError::Invalid("reference"))));
    assert_eq!(snapshot(&phone).unwrap(), before);

    document(&source, "Warranty.pdf");
    let mut unknown_target = rows_to(&source, &phone);
    unknown_target.rows.remove(&Table::Transactions);
    unknown_target.rows.insert(
        Table::DocumentLinks,
        vec![Row::Link(wire::DocumentLinkRow {
            sync_id: "ab".repeat(16),
            stamp: stamp::tick(&source).unwrap().format(),
            document: text(&source, "SELECT sync_id FROM documents"),
            transaction: Some("ef".repeat(16)),
            recurring: None,
            ledger: None,
            created_at: "2026-03-01T10:00:00.000Z".into(),
        })],
    );
    assert!(matches!(engine::apply(&mut phone, &unknown_target, now(), None), Err(SyncError::Invalid("reference"))));
    assert_eq!(snapshot(&phone).unwrap(), before, "nothing of it was applied, not even the document that was fine");
    let _ = &mut source;
}

#[test]
fn a_document_row_with_other_contents_under_a_known_id_is_refused() {
    let (mut desktop, mut phone) = paired();
    document(&desktop, "Warranty.pdf");
    sync_ok(&mut desktop, &mut phone);
    pause();
    desktop.execute("UPDATE documents SET name = 'Renamed.pdf'", []).unwrap();
    let before = snapshot(&phone).unwrap();
    for (what, tamper) in [("hash", 0), ("size", 1), ("kind", 2)] {
        let mut incoming = rows_to(&desktop, &phone);
        let Some(Row::Document(row)) = incoming.rows.get_mut(&Table::Documents).and_then(|rows| rows.first_mut()) else { panic!("no document row") };
        match tamper {
            0 => row.sha256 = sha_of("another file"),
            1 => row.size_bytes += 1,
            _ => row.media_type = "text/plain".into(),
        }
        assert!(matches!(engine::apply(&mut phone, &incoming, now(), None), Err(SyncError::Invalid("document"))), "{what}");
        assert_eq!(snapshot(&phone).unwrap(), before, "{what}: the rename that came with it was not applied either");
    }
    // the honest row is taken
    let incoming = rows_to(&desktop, &phone);
    engine::apply(&mut phone, &incoming, now(), None).unwrap();
    assert_eq!(documents_named(&phone), ["Renamed.pdf"]);
}

#[test]
fn a_link_row_that_names_another_document_or_target_under_a_known_id_is_refused() {
    let (mut desktop, mut phone) = paired();
    document(&desktop, "Warranty.pdf");
    document(&desktop, "Other.pdf");
    attach(&desktop, "Warranty.pdf", to_transaction(id_of(&desktop, "transactions", "Coffee")));
    sync_ok(&mut desktop, &mut phone);
    let before = snapshot(&phone).unwrap();
    // the link as the desktop sends it (to a device that has none of it)
    let Row::Link(mut link) = rows_to(&desktop, &device("EUR")).rows.remove(&Table::DocumentLinks).unwrap().remove(0) else { panic!("no link row") };
    link.stamp = stamp::tick(&desktop).unwrap().format();
    let mut elsewhere = link.clone();
    elsewhere.document = text(&desktop, "SELECT sync_id FROM documents WHERE name = 'Other.pdf'");
    let mut other_target = link.clone();
    other_target.transaction = Some(text(&desktop, "SELECT sync_id FROM transactions WHERE name = 'Tea'"));
    let mut recurring_instead = link;
    recurring_instead.transaction = None;
    recurring_instead.recurring = Some(text(&desktop, "SELECT sync_id FROM recurring_transactions"));
    for (what, row) in [("another document", elsewhere), ("another transaction", other_target), ("a recurring item instead", recurring_instead)] {
        let mut incoming = Incoming::default();
        incoming.rows.insert(Table::DocumentLinks, vec![Row::Link(row)]);
        assert!(matches!(engine::apply(&mut phone, &incoming, now(), None), Err(SyncError::Invalid("link"))), "{what}");
        assert_eq!(snapshot(&phone).unwrap(), before, "{what}");
    }
}

#[test]
fn a_marker_for_a_document_this_device_never_had_is_remembered_so_the_row_cannot_come_back() {
    let (mut desktop, mut phone) = paired();
    document(&desktop, "Warranty.pdf");
    let rows_before_delete = rows_to(&desktop, &phone);
    desktop.execute("DELETE FROM documents", []).unwrap();
    let markers = rows_to(&desktop, &phone);
    engine::apply(&mut phone, &markers, now(), None).unwrap();
    assert_eq!(count(&phone, "SELECT count(*) FROM sync_deleted WHERE kind = 'documents'"), 1);
    // the row, older than the marker, arrives late: it is not taken
    let summary = engine::apply(&mut phone, &rows_before_delete, now(), None).unwrap();
    assert_eq!(count(&phone, "SELECT count(*) FROM documents"), 0);
    assert_eq!(summary.added, 0);
    sync_ok(&mut desktop, &mut phone);
}

#[test]
fn a_device_with_only_documents_is_not_empty_so_a_phone_with_some_is_not_paired() {
    let conn = device("EUR");
    assert!(engine::is_empty(&conn).unwrap());
    document(&conn, "Warranty.pdf");
    assert!(!engine::is_empty(&conn).unwrap());
}

#[test]
fn what_a_device_changed_in_documents_counts_as_waiting_for_the_next_sync() {
    let (desktop, _phone) = paired();
    let since = now();
    pause();
    assert_eq!(super::peers::changes_since(&desktop, since).unwrap(), 0);
    document(&desktop, "Warranty.pdf");
    attach(&desktop, "Warranty.pdf", to_transaction(id_of(&desktop, "transactions", "Coffee")));
    assert_eq!(super::peers::changes_since(&desktop, since).unwrap(), 2, "the document and the link");
    library::unlink(&desktop, id_of(&desktop, "documents", "Warranty.pdf"), &to_transaction(id_of(&desktop, "transactions", "Coffee"))).unwrap();
    assert_eq!(super::peers::changes_since(&desktop, since).unwrap(), 2, "the document and the marker of the link");
}


// --- friends, the ledger and shared bills (phase 10) ----------------------------------------------------------------------

mod debts {
    use super::*;
    use crate::debts::fixtures::{equal, friend, guest, input, with, who};
    use crate::debts::splits::{check, set_split, set_template, Mode};

    fn food(conn: &Connection) -> i64 {
        id_of(conn, "categories", "Food")
    }

    fn lend(conn: &Connection, person: i64, amount: &str, date: &str) -> i64 {
        ledger::add(
            conn,
            &ledger::NewEntry { person_id: person.to_string(), kind: ledger::Kind::Lent, amount: amount.into(), date: date.into(), note: None, transaction_id: None },
        )
        .unwrap()
        .id
    }

    fn balance(conn: &Connection, name: &str) -> i64 {
        count(conn, &format!("SELECT b.balance_minor FROM people p JOIN person_balances b ON b.person_id = p.id WHERE p.name = '{name}' COLLATE NOCASE AND p.merged_into IS NULL AND p.archived = 0"))
    }

    fn parts_of(conn: &Connection, bill: &str) -> Vec<(String, i64)> {
        let mut statement = conn
            .prepare(
                "SELECT COALESCE(p.name, l.guest_name), l.amount_minor FROM transaction_split_lines l JOIN transactions t ON t.id = l.transaction_id
                 LEFT JOIN people p ON p.id = l.person_id WHERE t.name = ?1 ORDER BY 1",
            )
            .unwrap();
        statement.query_map([bill], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    fn people_text(conn: &Connection) -> Vec<String> {
        let mut statement = conn.prepare("SELECT name || ':' || archived || ':' || COALESCE(merged_into IS NOT NULL, 0) FROM people ORDER BY sync_id").unwrap();
        statement.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn a_bill_shared_differently_on_both_devices_ends_with_the_parts_of_the_newer_one_and_never_a_mix() {
        let (mut desktop, mut phone) = paired();
        let (anna, bob) = (friend(&desktop, "Anna"), friend(&desktop, "Bob"));
        let dinner = spend(&desktop, food(&desktop), "Dinner", "90.00");
        set_split(&desktop, dinner, &equal(vec![who(anna)])).unwrap();
        sync_ok(&mut desktop, &mut phone);
        let on_phone = id_of(&phone, "transactions", "Dinner");
        let bob_on_phone = id_of(&phone, "people", "Bob");
        // the desktop adds Bob; later the phone gives the whole bill to Anna and a guest
        set_split(&desktop, dinner, &equal(vec![who(anna), who(bob)])).unwrap();
        pause();
        let anna_on_phone = id_of(&phone, "people", "Anna");
        set_split(&phone, on_phone, &input(Mode::Shares, Some("1"), vec![with(who(anna_on_phone), "1"), with(guest("Dina"), "1")])).unwrap();
        let _ = bob_on_phone;
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(parts_of(&desktop, "Dinner"), [("Anna".to_string(), 3000), ("Dina".to_string(), 3000)], "the newer bill, all of it");
        assert_eq!(balance(&desktop, "Bob"), 0, "Bob's part is gone with the older version");
        // the other way round
        pause();
        set_split(&desktop, dinner, &equal(vec![who(bob)])).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(parts_of(&phone, "Dinner"), [("Bob".to_string(), 4500)]);
    }

    #[test]
    fn a_split_template_follows_the_item_and_the_newer_item_wins_with_its_template() {
        let (mut desktop, mut phone) = paired();
        let anna = friend(&desktop, "Anna");
        let item = id_of(&desktop, "recurring_transactions", "Rent");
        set_template(&desktop, item, &equal(vec![who(anna)])).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(count(&phone, "SELECT split_my_weight FROM recurring_transactions"), 1);
        pause();
        let on_phone = id_of(&phone, "recurring_transactions", "Rent");
        set_template(&phone, on_phone, &input(Mode::Shares, Some("3"), vec![with(who(id_of(&phone, "people", "Anna")), "2"), with(guest("Eve"), "1")])).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(count(&desktop, "SELECT split_my_weight FROM recurring_transactions"), 3);
        assert_eq!(count(&desktop, "SELECT count(*) FROM recurring_split_lines"), 2);
        pause();
        splits_clear_template(&desktop, item);
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(count(&phone, "SELECT count(*) FROM recurring_transactions WHERE split_my_weight IS NOT NULL"), 0);
        // a payment of the item is split on the device that pays it, and the payment arrives with its parts
        set_template(&phone, on_phone, &equal(vec![who(id_of(&phone, "people", "Anna"))])).unwrap();
        recurring::pay(&phone, on_phone, Some("600.00"), Some("2026-03-20"), TODAY).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(parts_of(&desktop, "Rent"), [("Anna".to_string(), 30_000)]);
    }

    fn splits_clear_template(conn: &Connection, id: i64) {
        crate::debts::splits::clear_template(conn, id).unwrap();
    }

    #[test]
    fn the_same_friend_added_on_both_devices_becomes_one_and_takes_everything_with_it() {
        let (mut desktop, mut phone) = paired();
        // two Annas, each with a bill, an entry, a template line and a document link of their own
        let on_desktop = friend(&desktop, "Anna");
        let on_phone = friend(&phone, "anna");
        let dinner = spend(&desktop, food(&desktop), "Dinner", "90.00");
        set_split(&desktop, dinner, &equal(vec![who(on_desktop)])).unwrap();
        let lunch = spend(&phone, food(&phone), "Lunch", "40.00");
        set_split(&phone, lunch, &equal(vec![who(on_phone), guest("Carl")])).unwrap();
        lend(&desktop, on_desktop, "10.00", "2026-03-05");
        lend(&phone, on_phone, "5.00", "2026-03-06");
        let item = id_of(&desktop, "recurring_transactions", "Rent");
        set_template(&desktop, item, &equal(vec![who(on_desktop)])).unwrap();
        sync_ok(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT count(*) FROM people WHERE archived = 0"), 1, "one Anna");
            assert_eq!(count(conn, "SELECT count(*) FROM people WHERE merged_into IS NOT NULL AND archived = 1"), 1, "and one merged away");
            // 45.00 and 10.00 from the desktop, 13.33 and 5.00 from the phone
            assert_eq!(balance(conn, "Anna"), 4500 + 1000 + 1333 + 500, "everything is hers now");
            assert_eq!(count(conn, "SELECT count(*) FROM ledger_entries e JOIN people p ON p.id = e.person_id WHERE p.merged_into IS NOT NULL"), 0);
            assert_eq!(count(conn, "SELECT count(*) FROM transaction_split_lines l JOIN people p ON p.id = l.person_id WHERE p.merged_into IS NOT NULL"), 0);
            assert_eq!(count(conn, "SELECT count(*) FROM recurring_split_lines l JOIN people p ON p.id = l.person_id WHERE p.merged_into IS NOT NULL"), 0);
            assert!(check(conn).unwrap());
        }
        // the one who stays is the one with the lower id, on both
        let kept = |conn: &Connection| text(conn, "SELECT sync_id FROM people WHERE archived = 0");
        assert_eq!(kept(&desktop), kept(&phone));
        let ids = [text(&desktop, "SELECT min(sync_id) FROM people")];
        assert_eq!(kept(&desktop), ids[0]);
        assert_eq!(people_text(&desktop), people_text(&phone));
    }

    #[test]
    fn a_bill_that_names_two_friends_who_are_one_gets_one_part_with_the_amounts_added() {
        // not something one device can make (it never holds two active friends of one name), so the rows are written by hand: the
        // winner, the friend who is merged into it, and a bill that holds a part for each
        let (mut desktop, _) = paired();
        let winner = friend(&desktop, "Anna");
        let winner_sync_id = text(&desktop, "SELECT sync_id FROM people WHERE name = 'Anna'");
        let loser_sync_id = "ff".repeat(16);
        assert!(winner_sync_id < loser_sync_id);
        let stamp = |n: u64| Stamp::new(now() + n, 0, "abcdef01").format();
        let person = wire::PersonRow {
            sync_id: loser_sync_id.clone(),
            stamp: stamp(1),
            name: "Anna".into(),
            archived: 0,
            merged_into: None,
            remind_on: None,
            remind_every_days: None,
            created_at: "2026-03-01T10:00:00.000Z".into(),
            updated_at: "2026-03-01T10:00:00.000Z".into(),
        };
        let mut bill = match load_one(&desktop, Table::Transactions, "Coffee") {
            Row::Transaction(r) => r,
            _ => unreachable!(),
        };
        bill.stamp = stamp(2);
        bill.amount_minor = 9000;
        bill.split = vec![
            wire::SplitPartRow { person: Some(winner_sync_id.clone()), guest: None, amount_minor: 1000 },
            wire::SplitPartRow { person: Some(loser_sync_id.clone()), guest: None, amount_minor: 2000 },
        ];
        let mut incoming = Incoming::default();
        incoming.rows.insert(Table::People, vec![Row::Person(person)]);
        incoming.rows.insert(Table::Transactions, vec![Row::Transaction(bill)]);
        let summary = engine::apply(&mut desktop, &incoming, now(), None).unwrap();
        assert_eq!(summary.merged, 1);
        assert_eq!(parts_of(&desktop, "Coffee"), [("Anna".to_string(), 3000)], "one part, 10.00 and 20.00 added");
        assert_eq!(count(&desktop, "SELECT count(*) FROM transaction_split_lines WHERE person_id = (SELECT id FROM people WHERE sync_id = '{}')".replace("{}", &loser_sync_id).as_str()), 0);
        assert_eq!(balance(&desktop, "Anna"), 3000);
        assert_eq!(count(&desktop, "SELECT archived FROM people WHERE sync_id = (SELECT merged_into FROM people WHERE merged_into IS NOT NULL)"), 0);
        let _ = winner;
        assert!(check(&desktop).unwrap());
    }

    #[test]
    fn friends_who_swap_names_or_take_a_name_just_given_up_do_not_clash() {
        let (mut desktop, mut phone) = paired();
        let (a, b) = (friend(&desktop, "Anna"), friend(&desktop, "Bob"));
        sync_ok(&mut desktop, &mut phone);
        pause();
        // a swap on the desktop
        people::rename(&desktop, a, "Tmp").unwrap();
        people::rename(&desktop, b, "Anna").unwrap();
        people::rename(&desktop, a, "Bob").unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(text(&phone, "SELECT name FROM people WHERE sync_id = (SELECT sync_id FROM people WHERE name = 'Bob')"), "Bob");
        assert_eq!(count(&phone, "SELECT count(*) FROM people WHERE name IN ('Anna', 'Bob')"), 2);
        assert_eq!(people_text(&desktop), people_text(&phone));
    }

    #[test]
    fn archiving_a_friend_on_one_device_while_the_other_adds_a_part_keeps_both() {
        let (mut desktop, mut phone) = paired();
        let anna = friend(&desktop, "Anna");
        sync_ok(&mut desktop, &mut phone);
        people::archive(&desktop, anna).unwrap();
        let dinner = spend(&phone, food(&phone), "Dinner", "90.00");
        set_split(&phone, dinner, &equal(vec![who(id_of(&phone, "people", "Anna"))])).unwrap();
        lend(&phone, id_of(&phone, "people", "Anna"), "10.00", "2026-03-05");
        sync_ok(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT archived FROM people WHERE name = 'Anna'"), 1);
            assert_eq!(count(conn, "SELECT b.balance_minor FROM people p JOIN person_balances b ON b.person_id = p.id WHERE p.name = 'Anna'"), 5500, "archiving removes nothing");
        }
    }

    #[test]
    fn a_ledger_entry_is_edited_deleted_and_the_newer_of_the_two_wins() {
        let (mut desktop, mut phone) = paired();
        let anna = friend(&desktop, "Anna");
        let loan = lend(&desktop, anna, "20.00", "2026-03-05");
        sync_ok(&mut desktop, &mut phone);
        pause();
        ledger::update(&desktop, loan, &ledger::EntryPatch { amount: Some("25.00".into()), ..Default::default() }).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(count(&phone, "SELECT amount_minor FROM ledger_entries"), 2500);
        pause();
        let on_phone = count(&phone, "SELECT id FROM ledger_entries");
        ledger::delete(&phone, on_phone).unwrap();
        pause();
        ledger::update(&desktop, loan, &ledger::EntryPatch { note: Some(Some("taxi".into())), ..Default::default() }).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(count(&phone, "SELECT count(*) FROM ledger_entries"), 1, "the edit came after the deletion and brings the entry back");
        pause();
        ledger::delete(&desktop, loan).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!((count(&phone, "SELECT count(*) FROM ledger_entries"), count(&phone, "SELECT count(*) FROM sync_deleted WHERE kind = 'ledger_entries'")), (0, 1));
    }

    #[test]
    fn two_entries_from_one_bill_code_are_one_entry_and_the_newer_stays() {
        let (mut desktop, mut phone) = paired();
        let (a, b) = (friend(&desktop, "Anna"), 0);
        let _ = b;
        sync_ok(&mut desktop, &mut phone);
        let code = "ab".repeat(16);
        let accept = |conn: &Connection, person: i64| {
            conn.execute(
                "INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date, origin_id) VALUES (?1, 'BORROWED', 1000, '2026-03-05', ?2)",
                params![person, code],
            )
            .unwrap();
        };
        accept(&desktop, a);
        pause();
        accept(&phone, id_of(&phone, "people", "Anna"));
        sync_ok(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT count(*) FROM ledger_entries"), 1);
            assert_eq!(count(conn, "SELECT count(*) FROM sync_deleted WHERE kind = 'ledger_entries'"), 1);
        }
    }

    #[test]
    fn a_code_taken_in_on_both_devices_ends_as_one_entry_and_two_expenses_stay_because_each_device_made_its_own() {
        use crate::debts::codes::{accept, encode, Accept, BillCode};
        let (mut desktop, mut phone) = paired();
        let code = encode(&BillCode { id: "cd".repeat(16), from: "Vincent".into(), what: "Dinner".into(), date: "2026-03-04".into(), amount_minor: 2250, total_minor: 9000, currency: "EUR".into() });
        let take = |conn: &Connection, expense: bool| {
            accept(
                conn,
                &Accept { text: code.clone(), person_id: None, new_friend: Some("Vincent".into()), add_expense: expense, category_id: expense.then(|| food(conn).to_string()), rate: None, amount: None },
            )
            .unwrap();
        };
        take(&desktop, true);
        pause();
        take(&phone, true);
        sync_ok(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT count(*) FROM ledger_entries WHERE origin_id IS NOT NULL"), 1, "one entry for one code");
            assert_eq!(count(conn, "SELECT count(*) FROM people WHERE name = 'Vincent' AND merged_into IS NULL"), 1, "and one friend");
            assert_eq!(count(conn, "SELECT count(*) FROM transactions WHERE name = 'Dinner'"), 2, "each device made an expense: the person takes a code in on one device");
        }
    }

    #[test]
    fn deleting_an_expense_clears_the_link_of_its_entries_on_both_and_a_newer_edit_brings_it_back_unlinked() {
        let (mut desktop, mut phone) = paired();
        let anna = friend(&desktop, "Anna");
        let dinner = spend(&desktop, food(&desktop), "Dinner", "90.00");
        let loan = lend(&desktop, anna, "20.00", "2026-03-05");
        ledger::update(&desktop, loan, &ledger::EntryPatch { transaction_id: Some(Some(dinner.to_string())), ..Default::default() }).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(count(&phone, "SELECT count(*) FROM ledger_entries WHERE transaction_id IS NOT NULL"), 1);
        // the desktop deletes the expense; the phone edits it later
        transactions::delete(&desktop, dinner).unwrap();
        pause();
        let on_phone = id_of(&phone, "transactions", "Dinner");
        transactions::update(&phone, on_phone, &transactions::TransactionPatch { name: Some("Dinner out".into()), ..Default::default() }).unwrap();
        sync_ok(&mut desktop, &mut phone);
        for conn in [&desktop, &phone] {
            assert_eq!(count(conn, "SELECT count(*) FROM transactions WHERE name = 'Dinner out'"), 1, "the newer edit brought the expense back");
            assert_eq!(count(conn, "SELECT count(*) FROM ledger_entries WHERE transaction_id IS NULL"), 1, "and the entry is unlinked on both");
        }
    }

    #[test]
    fn deleting_an_entry_on_one_device_takes_its_documents_links_on_the_other() {
        let (mut desktop, mut phone) = paired();
        let anna = friend(&desktop, "Anna");
        let loan = lend(&desktop, anna, "20.00", "2026-03-05");
        let doc = document(&desktop, "Loan note.pdf");
        library::link(&desktop, doc, &LinkTarget { kind: TargetKind::LedgerEntry, id: loan.to_string() }).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(count(&phone, "SELECT count(*) FROM document_links WHERE ledger_entry_id IS NOT NULL"), 1);
        pause();
        ledger::delete(&desktop, loan).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!((links(&phone), link_markers(&phone), count(&phone, "SELECT count(*) FROM documents")), (0, 0, 1), "the link goes with its entry, without a marker, and the document stays");
    }

    // ---- what arrives is untrusted ----

    fn incoming_for(table: Table, rows: Vec<Row>) -> Incoming {
        let mut incoming = Incoming::default();
        incoming.rows.insert(table, rows);
        incoming
    }

    #[test]
    fn rows_that_break_the_rules_of_friends_the_ledger_and_the_parts_are_refused_and_change_nothing() {
        let (mut desktop, mut phone) = paired();
        let anna = friend(&desktop, "Anna");
        let dinner = spend(&desktop, food(&desktop), "Dinner", "90.00");
        set_split(&desktop, dinner, &equal(vec![who(anna)])).unwrap();
        sync_ok(&mut desktop, &mut phone);
        let before = snapshot(&phone).unwrap();
        let person = text(&desktop, "SELECT sync_id FROM people");
        let bill = |conn: &Connection| -> wire::TransactionRow {
            match load_one(conn, Table::Transactions, "Dinner") {
                Row::Transaction(r) => r,
                _ => unreachable!(),
            }
        };
        let newer = |stamp: &str| {
            let mut s = Stamp::parse(stamp).unwrap();
            s.ms += 10;
            s.format()
        };
        // a part for a friend nobody has heard of
        let mut unknown_friend = bill(&desktop);
        unknown_friend.stamp = newer(&unknown_friend.stamp);
        unknown_friend.split = vec![wire::SplitPartRow { person: Some("ab".repeat(16)), guest: None, amount_minor: 100 }];
        assert!(matches!(engine::apply(&mut phone, &incoming_for(Table::Transactions, vec![Row::Transaction(unknown_friend)]), now(), None), Err(SyncError::Invalid("reference"))));
        // parts on a bill in an income category
        let mut on_income = bill(&desktop);
        on_income.stamp = newer(&on_income.stamp);
        on_income.category = text(&desktop, "SELECT sync_id FROM categories WHERE name = 'Salary'");
        on_income.split = vec![wire::SplitPartRow { person: Some(person.clone()), guest: None, amount_minor: 100 }];
        assert!(matches!(engine::apply(&mut phone, &incoming_for(Table::Transactions, vec![Row::Transaction(on_income)]), now(), None), Err(SyncError::Invalid("split"))));
        // an entry for a friend nobody has heard of
        let entry = wire::LedgerRow {
            sync_id: "cd".repeat(16),
            stamp: Stamp::new(now(), 0, "abcdef01").format(),
            person: "ab".repeat(16),
            kind: "LENT".into(),
            amount_minor: 100,
            entry_date: "2026-03-01".into(),
            note: None,
            transaction: None,
            origin_id: None,
            created_at: "2026-03-01T10:00:00.000Z".into(),
            updated_at: "2026-03-01T10:00:00.000Z".into(),
        };
        assert!(matches!(engine::apply(&mut phone, &incoming_for(Table::LedgerEntries, vec![Row::Ledger(entry.clone())]), now(), None), Err(SyncError::Invalid("reference"))));
        // an entry linked to an expense that never existed
        let linked = wire::LedgerRow { person: person.clone(), transaction: Some("ef".repeat(16)), ..entry };
        assert!(matches!(engine::apply(&mut phone, &incoming_for(Table::LedgerEntries, vec![Row::Ledger(linked)]), now(), None), Err(SyncError::Invalid("reference"))));
        assert_eq!(snapshot(&phone).unwrap(), before, "nothing of any of it was applied");
        assert!(check(&phone).unwrap());
    }

    fn load_one(conn: &Connection, table: Table, name: &str) -> Row {
        let id = text(conn, &format!("SELECT sync_id FROM {} WHERE name = '{name}'", table.name()));
        let digest = engine::digest(conn).unwrap();
        let messages = engine::outgoing(conn, &digest, &engine::Digest::default(), &[table]).unwrap();
        let mut collector = RowsCollector::new(vec![table]);
        for message in messages {
            if let Message::Rows(rows) = message {
                collector.push(rows).unwrap();
            }
        }
        collector.finish().unwrap().rows.remove(&table).unwrap().into_iter().find(|r| r.sync_id() == id).unwrap()
    }

    #[test]
    fn a_device_with_only_friends_is_not_empty_and_what_it_changed_counts_as_waiting() {
        let mut desktop = device("EUR");
        assert!(engine::is_empty(&desktop).unwrap());
        friend(&desktop, "Anna");
        assert!(!engine::is_empty(&desktop).unwrap(), "so a phone with a friend is not paired");
        let lender = device("EUR");
        let before = super::super::peers::changes_since(&lender, 0).unwrap();
        lender.execute("INSERT INTO people (name) VALUES ('Bob')", []).unwrap();
        lender.execute("INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date) VALUES (1, 'LENT', 100, '2026-03-01')", []).unwrap();
        assert_eq!(super::super::peers::changes_since(&lender, 0).unwrap(), before + 2, "a friend and an entry");
        let _ = &mut desktop;
    }

    #[test]
    fn a_conflict_over_a_friend_a_bill_s_parts_or_an_entry_is_shown_with_what_differs() {
        let (mut desktop, mut phone) = paired();
        let anna = friend(&desktop, "Anna");
        let dinner = spend(&desktop, food(&desktop), "Dinner", "90.00");
        set_split(&desktop, dinner, &equal(vec![who(anna)])).unwrap();
        let loan = lend(&desktop, anna, "20.00", "2026-03-05");
        sync_ok(&mut desktop, &mut phone);
        let met = now();
        pause();
        // both change the same three rows after they last met
        people::rename(&desktop, anna, "Annie").unwrap();
        set_split(&desktop, dinner, &equal(vec![who(anna), guest("Carl")])).unwrap();
        ledger::update(&desktop, loan, &ledger::EntryPatch { amount: Some("21.00".into()), ..Default::default() }).unwrap();
        let on_phone = id_of(&phone, "people", "Anna");
        people::rename(&phone, on_phone, "Ana").unwrap();
        set_split(&phone, id_of(&phone, "transactions", "Dinner"), &equal(vec![who(on_phone), guest("Dina")])).unwrap();
        ledger::update(&phone, count(&phone, "SELECT id FROM ledger_entries"), &ledger::EntryPatch { amount: Some("22.00".into()), ..Default::default() }).unwrap();
        let (mine, theirs) = (engine::digest(&desktop).unwrap(), engine::digest(&phone).unwrap());
        let (conflicts, more) = engine::conflicts(&mine, &theirs, &Table::ALL, Some(met));
        assert_eq!((conflicts.len(), more), (3, 0), "{conflicts:?}");
        let messages = engine::conflict_rows(&phone, &conflicts).unwrap();
        let mut collector = RowsCollector::new(engine::conflict_tables(&conflicts));
        for message in messages {
            if let Message::Rows(rows) = message {
                collector.push(rows).unwrap();
            }
        }
        let views = engine::describe(&desktop, &conflicts, &collector.finish().unwrap()).unwrap();
        let fields = |table: Table| -> Vec<(String, String, String)> {
            views.iter().find(|v| v.table == table).unwrap().fields.iter().map(|f| (f.field.to_string(), f.mine.clone(), f.theirs.clone())).collect()
        };
        assert_eq!(fields(Table::People), [("name".into(), "Annie".into(), "Ana".into())]);
        assert_eq!(fields(Table::LedgerEntries), [("amount".into(), "21.00".into(), "22.00".into())]);
        let parts = fields(Table::Transactions);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].0, "split");
        assert!(parts[0].1.contains("Carl") && parts[0].2.contains("Dina"), "{parts:?}");
        // the person's choices are only stamps: the version that is meant to win ends up on both
        let choices = vec![engine::Choice::Theirs; conflicts.len()];
        engine::decide(&mut desktop, &conflicts, &choices).unwrap();
        sync_ok(&mut desktop, &mut phone);
        assert_eq!(parts_of(&desktop, "Dinner"), [("Ana".to_string(), 3000), ("Dina".to_string(), 3000)]);
        assert_eq!(count(&desktop, "SELECT amount_minor FROM ledger_entries"), 2200);
    }

}
