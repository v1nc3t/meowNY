//! The sync engine between two databases in memory: every merge rule of `docs/sync-design.md` (section 11), the checks on
//! what arrives (section 12), and a fixed-seed random test that edits both devices with the app's own functions and
//! checks that one sync makes them the same and the next one changes nothing.

use std::time::Duration;

use rusqlite::{params, Connection};

use super::engine::{self, snapshot, Incoming, RowsCollector, SyncError, Warning};
use super::run::{sync, Outcome};
use super::stamp::{self, Stamp};
use super::wire::{self, Marker, Message, Row, Table};
use crate::budgets::{self, Scope, SetBudget};
use crate::categories::{self, CategoryType};
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
        let short = |l: &String| l.chars().take(400).collect::<String>();
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
fn what_the_desktop_has_deleted_is_deleted_on_the_phone_too() {
    let (mut desktop, mut phone) = paired();
    assert_eq!(count(&desktop, "SELECT count(*) FROM sync_deleted"), 1);
    assert_eq!(count(&phone, "SELECT count(*) FROM sync_deleted"), 1, "the marker of the payment deleted before the first sync travelled too");
    let coffee = id_of(&desktop, "transactions", "Coffee");
    transactions::delete(&desktop, coffee).unwrap();
    let outcome = sync_ok(&mut desktop, &mut phone);
    assert_eq!(outcome.second.deleted, 1);
    assert_eq!(count(&phone, "SELECT count(*) FROM transactions WHERE name = 'Coffee'"), 0);
    assert_eq!(count(&phone, "SELECT count(*) FROM sync_deleted"), 2);
}

#[test]
fn transactions_added_on_both_devices_are_all_kept() {
    let (mut desktop, mut phone) = paired();
    let (d_food, p_food) = (id_of(&desktop, "categories", "Food"), id_of(&phone, "categories", "Food"));
    spend(&desktop, d_food, "From the desktop", "10.00");
    spend(&phone, p_food, "From the phone", "20.00");
    spend(&phone, p_food, "Again from the phone", "30.00");
    let outcome = sync_ok(&mut desktop, &mut phone);
    assert_eq!((outcome.first.added, outcome.second.added), (2, 1));
    assert_eq!(count(&desktop, "SELECT count(*) FROM transactions"), 6, "three before (Coffee, Tea, the rent payment) and three new");
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
    match rng.below(14) {
        0 => {
            let _ = categories::create(conn, rng.pick(NAMES), kind);
        }
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
                conn.execute("UPDATE transactions SET name = ?2, amount_minor = ?3 WHERE id = ?1", params![id, format!("edited {}", rng.below(1000)), (1 + rng.below(100_000)) as i64]).unwrap();
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

/// How many random cases to run. `MEOWNY_SYNC_SEEDS=2000 cargo test sync::engine_tests::random` hunts for rare ones.
fn seeds(default: u64) -> u64 {
    std::env::var("MEOWNY_SYNC_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
fn random_edits_on_both_devices_always_end_the_same_after_one_sync() {
    for seed in 1..=seeds(60) {
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
    for seed in 100..100 + seeds(20) {
        let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
        let mut desktop = device("EUR");
        let mut phone = device("EUR");
        for _ in 0..20 {
            edit(&mut desktop, &mut rng);
        }
        sync_ok(&mut desktop, &mut phone);
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
const SHARED_TABLES: [&str; 7] = ["settings", "categories", "budgets", "recurring_transactions", "transactions", "sync_deleted", "currency_changes"];
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
    // The sync can only name five tables of rows, so nothing else can be asked for or sent.
    assert_eq!(wire::Table::ALL.len(), 5);
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

