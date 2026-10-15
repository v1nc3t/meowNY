//! A pairing and a sync between two devices, each with its own database, thread and end of a real connection on this computer.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rusqlite::Connection;
use zeroize::Zeroizing;

use super::crypto::{Ikm, Kind, SECRET_LEN, SESSION_ID_LEN};
use super::engine::{self, snapshot, SyncError};
use super::flow::test_support::LocalDb;
use super::flow::{self, Db, Exchange, FlowError, PairingRequest};
use super::net::{self, Limits, NetError, Window};
use super::peers;
use super::qr::Payload;
use super::wire::{Message, Table};
use crate::budgets::{self, Scope, SetBudget};
use crate::categories::{self, CategoryType};
use crate::recurring::{self, NewRecurring, Unit};
use crate::transactions::{self, NewTransaction};
use crate::{currency, migrate};

const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// Times that are short enough for a refusal to show fast and long enough for a big database to be worked out.
fn limits() -> Limits {
    Limits {
        connect: Duration::from_secs(2),
        handshake: Duration::from_millis(400),
        idle: Duration::from_secs(20),
        session: Duration::from_secs(120),
        sync_window: Duration::from_secs(10),
        pair_window: Duration::from_secs(10),
        max_bad_handshakes: 5,
        max_connections: 30,
    }
}

fn db(currency: &str) -> Arc<LocalDb> {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrate::run(&mut conn).unwrap();
    conn.execute("INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, ?1, 'en-IE', 'Europe/Dublin', 'en')", [currency]).unwrap();
    Arc::new(LocalDb::new(conn))
}

fn spend(conn: &Connection, category: i64, name: &str, amount: &str) -> i64 {
    transactions::create(
        conn,
        &NewTransaction { name: name.into(), amount: amount.into(), payment_date: "2026-03-02".into(), category_id: category.to_string(), description: None, client_uuid: None },
    )
    .unwrap()
    .id
}

/// A desktop with a bit of everything.
fn desktop_with_data() -> Arc<LocalDb> {
    let desktop = db("EUR");
    {
        let conn = desktop.conn.lock().unwrap();
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        let housing = categories::create(&conn, "Housing", CategoryType::Expense).unwrap().id;
        spend(&conn, food, "Coffee", "3.50");
        spend(&conn, food, "Tea", "2.00");
        budgets::set(
            &conn,
            &SetBudget { kind: CategoryType::Expense, scope: Scope::Global, month: "2026-03".into(), amount_limit: Some("2000.00".into()), category_id: None },
            chrono::Utc::now(),
        )
        .unwrap();
        let item = recurring::create(
            &conn,
            &NewRecurring { name: "Rent".into(), amount: Some("900.00".into()), category_id: housing.to_string(), start_date: "2026-03-01".into(), interval_unit: Unit::Month, interval_count: 1, end_date: None, description: None },
        )
        .unwrap()
        .id;
        recurring::pay(&conn, item, None, Some("2026-03-02"), "2026-03-15").unwrap();
    }
    desktop
}

fn count(db: &LocalDb, sql: &str) -> i64 {
    db.conn.lock().unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn same(a: &LocalDb, b: &LocalDb) {
    let (x, y) = (snapshot(&a.conn.lock().unwrap()).unwrap(), snapshot(&b.conn.lock().unwrap()).unwrap());
    if x != y {
        let short = |l: &String| l.chars().take(300).collect::<String>();
        let only_x: Vec<_> = x.iter().filter(|l| !y.contains(l)).map(short).collect();
        let only_y: Vec<_> = y.iter().filter(|l| !x.contains(l)).map(short).collect();
        panic!("the devices differ\nonly on the first: {only_x:#?}\nonly on the second: {only_y:#?}");
    }
}

struct Paired {
    desktop: Result<Exchange, FlowError>,
    phone: Result<Exchange, FlowError>,
    asked: Option<PairingRequest>,
    shown: Option<String>,
}

fn new_secret(byte: u8) -> Zeroizing<[u8; SECRET_LEN]> {
    Zeroizing::new([byte; SECRET_LEN])
}

/// Pairs `phone` with `desktop`. `yes` is what the person says on the desktop.
fn pair(desktop: &Arc<LocalDb>, phone: &Arc<LocalDb>, yes: bool) -> Paired {
    let secret = new_secret(0x61);
    let sid = [0x71u8; SESSION_ID_LEN];
    let window = Window::open(Kind::Pair, LOOPBACK, sid, vec![Ikm::for_pairing(&secret)], limits()).unwrap();
    let payload = Payload { kind: Kind::Pair, session_id: sid, secret: new_secret(0x61), port: window.address().port(), addresses: vec![LOOPBACK] };
    let asked = Arc::new(Mutex::new(None));
    let handle = {
        let (desktop, asked) = (desktop.clone(), asked.clone());
        thread::spawn(move || {
            let cancel = Arc::new(AtomicBool::new(false));
            flow::pair_as_desktop(window, &secret, &cancel, &*desktop, "Desktop", &mut |request| {
                *asked.lock().unwrap() = Some(request.clone());
                yes
            })
        })
    };
    let mut shown = None;
    let phone_result = flow::pair_as_phone(&payload, &**phone, "Phone", limits(), Arc::new(AtomicBool::new(false)), &mut |code| shown = Some(code.to_string()));
    let desktop_result = handle.join().unwrap();
    let asked = asked.lock().unwrap().clone();
    Paired { desktop: desktop_result, phone: phone_result, asked, shown }
}

/// One sync with a fresh code. `one_time` is the secret the desktop shows; `phone_uses` is the one the phone scanned.
fn sync_with(desktop: &Arc<LocalDb>, phone: &Arc<LocalDb>, one_time: u8, phone_uses: u8) -> (Result<(Exchange, String), FlowError>, Result<Exchange, FlowError>) {
    let paired = desktop.with(|c| peers::secrets(c)).unwrap();
    let ids: Vec<String> = paired.iter().map(|(id, _)| id.clone()).collect();
    let ikms: Vec<Ikm> = paired.iter().map(|(_, ps)| Ikm::for_sync(ps, &[one_time; SECRET_LEN])).collect();
    let sid = [0x72u8; SESSION_ID_LEN];
    let window = Window::open(Kind::Sync, LOOPBACK, sid, ikms, limits()).unwrap();
    let payload = Payload { kind: Kind::Sync, session_id: sid, secret: new_secret(phone_uses), port: window.address().port(), addresses: vec![LOOPBACK] };
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = {
        let (desktop, cancel) = (desktop.clone(), cancel.clone());
        thread::spawn(move || flow::sync_as_desktop(window, &cancel, &*desktop, "Desktop", &ids, &mut || {}))
    };
    let phone_result = flow::sync_as_phone(&payload, &**phone, "Phone", limits(), Arc::new(AtomicBool::new(false)), &mut || {});
    if phone_result.is_err() && !matches!(&phone_result, Err(FlowError::Peer { .. })) {
        // the desktop is still waiting for a phone that never got in
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    (handle.join().unwrap(), phone_result)
}

#[test]
fn a_new_phone_is_paired_after_the_person_says_yes_and_gets_a_full_copy() {
    let desktop = desktop_with_data();
    {
        let conn = desktop.conn.lock().unwrap();
        // a currency change before pairing: the phone takes the currency and the generation as they are
        drop(conn);
        let mut conn = desktop.conn.lock().unwrap();
        currency::change(&mut conn, "USD", Some("2"), "2026-03-15").unwrap();
    }
    let phone = db("RON");
    let result = pair(&desktop, &phone, true);

    let (on_desktop, on_phone) = (result.desktop.unwrap(), result.phone.unwrap());
    assert!(!on_desktop.differences && !on_phone.differences);
    let rows = count(&desktop, "SELECT (SELECT count(*) FROM categories) + (SELECT count(*) FROM transactions) + (SELECT count(*) FROM budgets) + (SELECT count(*) FROM recurring_transactions)");
    assert_eq!(on_phone.summary.added as i64, rows, "every row the desktop has came to the phone");
    let asked = result.asked.unwrap();
    assert_eq!((asked.peer_name.as_str(), Some(asked.check_code.as_str())), ("Phone", result.shown.as_deref()), "both screens show the same six digits");
    assert_eq!(asked.check_code.len(), 6);
    same(&desktop, &phone);
    assert_eq!(count(&phone, "SELECT count(*) FROM transactions"), 3);
    assert_eq!(count(&phone, "SELECT amount_minor FROM transactions WHERE name = 'Coffee'"), 700, "in the desktop's currency, already converted");

    // each stores the other with the same secret
    let (d_peers, p_peers) = (desktop.with(|c| peers::list(c)).unwrap(), phone.with(|c| peers::list(c)).unwrap());
    assert_eq!((d_peers.len(), p_peers.len()), (1, 1));
    assert_eq!((d_peers[0].name.as_str(), p_peers[0].name.as_str()), ("Phone", "Desktop"));
    let (d_secret, p_secret) = (desktop.with(|c| peers::secrets(c)).unwrap(), phone.with(|c| peers::secrets(c)).unwrap());
    assert_eq!(*d_secret[0].1, *p_secret[0].1);
    assert!(d_peers[0].last_sync_at.is_some() && p_peers[0].last_sync_at.is_some());
    assert_eq!(text(&phone, "SELECT currency FROM settings"), "USD");
    assert_eq!(count(&phone, "SELECT currency_generation FROM settings"), 1);
}

fn text(db: &LocalDb, sql: &str) -> String {
    db.conn.lock().unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn a_no_on_the_desktop_pairs_nothing_and_copies_nothing() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    let result = pair(&desktop, &phone, false);
    assert!(matches!(result.desktop, Err(FlowError::Declined)), "{:?}", result.desktop);
    assert!(matches!(result.phone, Err(FlowError::Declined)), "{:?}", result.phone);
    assert_eq!(desktop.with(|c| peers::list(c)).unwrap().len(), 0);
    assert_eq!(phone.with(|c| peers::list(c)).unwrap().len(), 0);
    assert_eq!(count(&phone, "SELECT count(*) FROM transactions"), 0);
    assert!(result.shown.is_some(), "the phone had shown the code and waited");
}

#[test]
fn a_phone_that_already_holds_data_is_not_paired_and_never_connects() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    {
        let conn = phone.conn.lock().unwrap();
        let c = categories::create(&conn, "Mine", CategoryType::Expense).unwrap().id;
        spend(&conn, c, "Own", "1.00");
    }
    let secret = new_secret(0x61);
    let sid = [0x71u8; SESSION_ID_LEN];
    let window = Window::open(Kind::Pair, LOOPBACK, sid, vec![Ikm::for_pairing(&secret)], limits()).unwrap();
    let payload = Payload { kind: Kind::Pair, session_id: sid, secret: new_secret(0x61), port: window.address().port(), addresses: vec![LOOPBACK] };
    let result = flow::pair_as_phone(&payload, &*phone, "Phone", limits(), Arc::new(AtomicBool::new(false)), &mut |_| panic!("no code is shown"));
    assert!(matches!(result, Err(FlowError::Sync(SyncError::NotEmpty))), "{result:?}");
    // the window is still open and unused
    let cancel = Arc::new(AtomicBool::new(true));
    assert!(matches!(window.accept(&cancel), Err(NetError::Cancelled)));
    assert_eq!(desktop.with(|c| peers::list(c)).unwrap().len(), 0);
}

#[test]
fn a_phone_that_scanned_another_code_is_not_answered() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    let secret = new_secret(0x61);
    let sid = [0x71u8; SESSION_ID_LEN];
    let window = Window::open(Kind::Pair, LOOPBACK, sid, vec![Ikm::for_pairing(&secret)], limits()).unwrap();
    let port = window.address().port();
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = {
        let (desktop, cancel) = (desktop.clone(), cancel.clone());
        thread::spawn(move || flow::pair_as_desktop(window, &secret, &cancel, &*desktop, "Desktop", &mut |_| true))
    };
    let wrong = Payload { kind: Kind::Pair, session_id: sid, secret: new_secret(0x99), port, addresses: vec![LOOPBACK] };
    let result = flow::pair_as_phone(&wrong, &*phone, "Phone", limits(), Arc::new(AtomicBool::new(false)), &mut |_| {});
    assert!(matches!(result, Err(FlowError::Net(NetError::Closed)) | Err(FlowError::Net(NetError::Timeout))), "{result:?}");
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(matches!(handle.join().unwrap(), Err(FlowError::Net(NetError::Cancelled))));
    assert_eq!(desktop.with(|c| peers::list(c)).unwrap().len(), 0);
}

#[test]
fn a_paired_phone_syncs_again_with_a_fresh_code_and_both_changes_meet() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    let first = pair(&desktop, &phone, true);
    first.desktop.unwrap();
    first.phone.unwrap();

    {
        let conn = desktop.conn.lock().unwrap();
        let food: i64 = conn.query_row("SELECT id FROM categories WHERE name = 'Food'", [], |r| r.get(0)).unwrap();
        spend(&conn, food, "From the desktop", "10.00");
    }
    {
        let conn = phone.conn.lock().unwrap();
        let food: i64 = conn.query_row("SELECT id FROM categories WHERE name = 'Food'", [], |r| r.get(0)).unwrap();
        spend(&conn, food, "From the phone", "20.00");
        conn.execute("UPDATE categories SET name = 'Groceries' WHERE id = ?1", [food]).unwrap();
    }
    let (on_desktop, on_phone) = sync_with(&desktop, &phone, 0x33, 0x33);
    let ((on_desktop, device), on_phone) = (on_desktop.unwrap(), on_phone.unwrap());
    assert!(!on_desktop.differences && !on_phone.differences);
    same(&desktop, &phone);
    assert_eq!(count(&desktop, "SELECT count(*) FROM transactions"), 5);
    assert_eq!(text(&desktop, "SELECT name FROM categories WHERE name LIKE 'Gro%'"), "Groceries");
    assert_eq!((on_desktop.summary.added, on_phone.summary.added), (1, 1));
    assert_eq!(device, desktop.with(|c| peers::list(c)).unwrap()[0].device_id);

    // and a third, with nothing new, finds nothing to compare
    let (a, b) = sync_with(&desktop, &phone, 0x34, 0x34);
    let ((a, _), b) = (a.unwrap(), b.unwrap());
    assert!(a.compared.is_empty() && b.compared.is_empty() && a.summary.is_empty() && b.summary.is_empty());
}

#[test]
fn a_code_from_another_time_is_refused() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    pair(&desktop, &phone, true).phone.unwrap();
    let before = snapshot(&phone.conn.lock().unwrap()).unwrap();
    let (on_desktop, on_phone) = sync_with(&desktop, &phone, 0x33, 0x44);
    assert!(matches!(on_phone, Err(FlowError::Net(NetError::Closed)) | Err(FlowError::Net(NetError::Timeout))), "{on_phone:?}");
    assert!(matches!(on_desktop, Err(FlowError::Net(NetError::Cancelled))), "the desktop never heard a valid message");
    assert_eq!(snapshot(&phone.conn.lock().unwrap()).unwrap(), before);
}

#[test]
fn a_device_the_desktop_forgot_can_no_longer_sync() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    pair(&desktop, &phone, true).phone.unwrap();
    let id = desktop.with(|c| peers::list(c)).unwrap()[0].device_id.clone();
    assert!(desktop.with(|c| peers::remove(c, &id)).unwrap());

    let sid = [0x72u8; SESSION_ID_LEN];
    // with nobody paired there is nothing that could open a message, but a window can still be open
    let window = Window::open(Kind::Sync, LOOPBACK, sid, vec![], limits()).unwrap();
    let payload = Payload { kind: Kind::Sync, session_id: sid, secret: new_secret(0x33), port: window.address().port(), addresses: vec![LOOPBACK] };
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = {
        let cancel = cancel.clone();
        thread::spawn(move || window.accept(&cancel).map(|_| ()))
    };
    let result = flow::sync_as_phone(&payload, &*phone, "Phone", limits(), Arc::new(AtomicBool::new(false)), &mut || {});
    assert!(matches!(result, Err(FlowError::Net(NetError::Closed)) | Err(FlowError::Net(NetError::Timeout))), "{result:?}");
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(matches!(handle.join().unwrap(), Err(NetError::Cancelled)));
}

#[test]
fn a_currency_change_made_after_pairing_reaches_the_phone_at_the_next_sync() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    pair(&desktop, &phone, true).phone.unwrap();
    {
        let conn = phone.conn.lock().unwrap();
        let food: i64 = conn.query_row("SELECT id FROM categories WHERE name = 'Food'", [], |r| r.get(0)).unwrap();
        spend(&conn, food, "Euros on the phone", "5.00");
    }
    {
        let mut conn = desktop.conn.lock().unwrap();
        currency::change(&mut conn, "GBP", Some("0.8"), "2026-03-16").unwrap();
    }
    let (a, b) = sync_with(&desktop, &phone, 0x35, 0x35);
    a.unwrap();
    b.unwrap();
    same(&desktop, &phone);
    assert_eq!(text(&phone, "SELECT currency FROM settings"), "GBP");
    assert_eq!(count(&phone, "SELECT amount_minor FROM transactions WHERE name = 'Euros on the phone'"), 400, "converted on the phone, then sent");
    assert_eq!(count(&desktop, "SELECT amount_minor FROM transactions WHERE name = 'Euros on the phone'"), 400);
}

#[test]
fn the_phone_is_told_why_a_sync_stops() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    pair(&desktop, &phone, true).phone.unwrap();

    // a phone from another version of the app: its hello says another schema
    let paired = desktop.with(|c| peers::secrets(c)).unwrap();
    let sid = [0x72u8; SESSION_ID_LEN];
    let window = Window::open(Kind::Sync, LOOPBACK, sid, vec![Ikm::for_sync(&paired[0].1, &[0x36; SECRET_LEN])], limits()).unwrap();
    let payload = Payload { kind: Kind::Sync, session_id: sid, secret: new_secret(0x36), port: window.address().port(), addresses: vec![LOOPBACK] };
    let ids = vec![paired[0].0.clone()];
    let handle = {
        let desktop = desktop.clone();
        thread::spawn(move || flow::sync_as_desktop(window, &Arc::new(AtomicBool::new(false)), &*desktop, "Desktop", &ids, &mut || {}))
    };
    let mut channel = net::connect(&payload, Ikm::for_sync(&paired[0].1, &payload.secret), limits(), None).unwrap();
    let mut hello = phone.with(|c| engine::hello(c, "Phone", "9.9.9")).unwrap();
    hello.schema += 1;
    channel.send(&Message::Hello(hello)).unwrap();
    let _desktops_hello = channel.receive();
    let reply = channel.receive();
    // either the hello of the desktop and then the error, or the error at once
    let code = match reply {
        Ok(Message::Error(e)) => e.code,
        Ok(other) => match channel.receive() {
            Ok(Message::Error(e)) => e.code,
            r => panic!("expected an error after {other:?}, got {r:?}"),
        },
        Err(e) => panic!("{e}"),
    };
    assert_eq!(code, "schema_mismatch");
    assert!(matches!(handle.join().unwrap(), Err(FlowError::Sync(SyncError::SchemaMismatch { newer_here: false }))));
}

#[test]
fn a_locked_desktop_ends_the_sync_and_says_so() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    pair(&desktop, &phone, true).phone.unwrap();
    // the idle lock fires between the QR code and the phone's arrival
    let paired = desktop.with(|c| peers::secrets(c)).unwrap();
    let ids = vec![paired[0].0.clone()];
    let sid = [0x72u8; SESSION_ID_LEN];
    let window = Window::open(Kind::Sync, LOOPBACK, sid, vec![Ikm::for_sync(&paired[0].1, &[0x37; SECRET_LEN])], limits()).unwrap();
    let payload = Payload { kind: Kind::Sync, session_id: sid, secret: new_secret(0x37), port: window.address().port(), addresses: vec![LOOPBACK] };
    desktop.locked.store(true, std::sync::atomic::Ordering::Relaxed);
    let handle = {
        let desktop = desktop.clone();
        thread::spawn(move || flow::sync_as_desktop(window, &Arc::new(AtomicBool::new(false)), &*desktop, "Desktop", &ids, &mut || {}))
    };
    let result = flow::sync_as_phone(&payload, &*phone, "Phone", limits(), Arc::new(AtomicBool::new(false)), &mut || {});
    assert!(matches!(&result, Err(FlowError::Peer { code, .. }) if code == "busy"), "{result:?}");
    assert!(matches!(handle.join().unwrap(), Err(FlowError::Sync(SyncError::Locked))));
}

#[test]
fn a_message_that_does_not_belong_ends_the_sync() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    pair(&desktop, &phone, true).phone.unwrap();
    let paired = desktop.with(|c| peers::secrets(c)).unwrap();
    let sid = [0x72u8; SESSION_ID_LEN];
    let window = Window::open(Kind::Sync, LOOPBACK, sid, vec![Ikm::for_sync(&paired[0].1, &[0x38; SECRET_LEN])], limits()).unwrap();
    let payload = Payload { kind: Kind::Sync, session_id: sid, secret: new_secret(0x38), port: window.address().port(), addresses: vec![LOOPBACK] };
    let ids = vec![paired[0].0.clone()];
    let handle = {
        let desktop = desktop.clone();
        thread::spawn(move || flow::sync_as_desktop(window, &Arc::new(AtomicBool::new(false)), &*desktop, "Desktop", &ids, &mut || {}))
    };
    let mut channel = net::connect(&payload, Ikm::for_sync(&paired[0].1, &payload.secret), limits(), None).unwrap();
    channel.send(&Message::Done(super::wire::DoneMessage { hash: "0".repeat(64) })).unwrap();
    assert!(matches!(handle.join().unwrap(), Err(FlowError::Protocol(_))));
}

#[test]
fn a_pairing_phone_cannot_use_a_sync_code_and_the_other_way_round() {
    let desktop = desktop_with_data();
    let phone = db("EUR");
    let secret = new_secret(0x61);
    let sid = [0x71u8; SESSION_ID_LEN];
    let payload = Payload { kind: Kind::Sync, session_id: sid, secret, port: 1, addresses: vec![LOOPBACK] };
    assert!(matches!(flow::pair_as_phone(&payload, &*phone, "Phone", limits(), Arc::new(AtomicBool::new(false)), &mut |_| {}), Err(FlowError::Protocol(_))));
    let payload = Payload { kind: Kind::Pair, ..payload };
    assert!(matches!(flow::sync_as_phone(&payload, &*phone, "Phone", limits(), Arc::new(AtomicBool::new(false)), &mut || {}), Err(FlowError::Protocol(_))));
    let _ = desktop;
}

#[test]
fn big_databases_in_both_directions_cross_a_real_connection_without_a_standstill() {
    let desktop = db("EUR");
    let phone = db("EUR");
    {
        let conn = desktop.conn.lock().unwrap();
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..4000 {
            conn.execute(
                "INSERT INTO transactions (category_id, name, amount_minor, payment_date, description) VALUES (?1, ?2, ?3, '2026-03-02', 'a description of some length to make the rows less small')",
                rusqlite::params![food, format!("desktop {i}"), 100 + i],
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
    }
    pair(&desktop, &phone, true).phone.unwrap();
    for (who, n) in [(&desktop, 3000), (&phone, 3000)] {
        let conn = who.conn.lock().unwrap();
        let food: i64 = conn.query_row("SELECT id FROM categories WHERE name = 'Food'", [], |r| r.get(0)).unwrap();
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..n {
            conn.execute(
                "INSERT INTO transactions (category_id, name, amount_minor, payment_date, description) VALUES (?1, ?2, ?3, '2026-03-02', 'another description of some length for these rows too')",
                rusqlite::params![food, format!("more {i}"), 7 + i],
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
    }
    let (a, b) = sync_with(&desktop, &phone, 0x39, 0x39);
    let ((a, _), b) = (a.unwrap(), b.unwrap());
    assert!(!a.differences && !b.differences);
    assert_eq!(count(&desktop, "SELECT count(*) FROM transactions"), 10_000);
    same(&desktop, &phone);
    assert_eq!(a.compared, vec![Table::Transactions]);
}
