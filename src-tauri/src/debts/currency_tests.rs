//! Changing the currency converts the ledger and the shared bills (`docs/debts-design.md`, section 8).

use rusqlite::Connection;

use super::fixtures::*;
use super::ledger::{self, Kind, NewEntry};
use super::splits::{set_split, set_template, Mode};
use crate::currency::{apply_recorded, change, Recorded};
use crate::recurring::{self, NewRecurring, Unit};
use crate::sync;

/// Bills shared in every way, a template, and a ledger, built the same way every time.
fn lived_in() -> (Connection, i64, i64) {
    let w = world();
    let conn = w.conn;
    let (anna, bob) = (friend(&conn, "Anna"), friend(&conn, "Bob"));
    let dinner = bill(&conn, w.food, "Dinner", "100.00", "2026-01-01");
    set_split(&conn, dinner, &equal(vec![who(anna), who(bob), guest("Carl")])).unwrap();
    let lunch = bill(&conn, w.food, "Lunch", "33.33", "2026-01-02");
    set_split(&conn, lunch, &input(Mode::Percent, Some("33.34"), vec![with(who(anna), "33.33"), with(who(bob), "33.33")])).unwrap();
    let coffee = bill(&conn, w.food, "Coffee", "0.03", "2026-01-03");
    set_split(&conn, coffee, &input(Mode::Shares, Some("1"), vec![with(who(anna), "2")])).unwrap();
    bill(&conn, w.food, "Bread", "2.40", "2026-01-04");
    let item = recurring::create(
        &conn,
        &NewRecurring {
            name: "Rent".into(),
            amount: Some("900.00".into()),
            category_id: w.rent.to_string(),
            start_date: "2026-01-01".into(),
            interval_unit: Unit::Month,
            interval_count: 1,
            end_date: None,
            description: None,
        },
    )
    .unwrap();
    set_template(&conn, item.id, &input(Mode::Shares, Some("2"), vec![with(who(anna), "1")])).unwrap();
    for (person, kind, amount) in [(anna, Kind::Lent, "20.00"), (bob, Kind::Borrowed, "0.01"), (anna, Kind::RepaidToMe, "5.55")] {
        ledger::add(&conn, &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: "2026-01-05".into(), note: None, transaction_id: None }).unwrap();
    }
    (conn, anna, bob)
}

/// (name, amount, what it costs you, the parts)
type Bill = (String, i64, i64, Vec<(String, i64)>);

fn bills(conn: &Connection) -> Vec<Bill> {
    let mut statement = conn.prepare("SELECT id, name, amount_minor FROM transactions ORDER BY id").unwrap();
    let rows: Vec<(i64, String, i64)> = statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
    rows.into_iter().map(|(id, name, amount)| (name, amount, cost(conn, id), parts(conn, id))).collect()
}

fn entries(conn: &Connection) -> Vec<i64> {
    conn.prepare("SELECT amount_minor FROM ledger_entries ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn a_shared_bill_is_converted_by_scaling_every_part_so_they_still_add_up() {
    let (mut conn, _, _) = lived_in();
    change(&mut conn, "USD", Some("1.08"), "2026-02-01").unwrap();
    for (name, amount, cost, parts) in bills(&conn) {
        assert_eq!(cost + parts.iter().map(|p| p.1).sum::<i64>(), amount, "{name}: the parts and yours add up to the bill to the cent");
        assert!(cost >= 0 && parts.iter().all(|p| p.1 > 0), "{name}");
    }
    let by_name = |name: &str| bills(&conn).into_iter().find(|b| b.0 == name).unwrap();
    assert_eq!(by_name("Dinner").1, 10_800);
    assert_eq!(by_name("Dinner").3, [("Anna".to_string(), 2700), ("Bob".to_string(), 2700), ("Carl".to_string(), 2700)]);
    assert_eq!(by_name("Dinner").2, 2700, "you, the fourth");
    assert_eq!(by_name("Bread").1, 259, "2.40 at 1.08 is 2.592, rounded to the cent");
    assert!(by_name("Bread").3.is_empty());
    assert!(super::splits::check(&conn).unwrap());
}

#[test]
fn rounding_each_part_alone_could_pass_the_bill_and_this_does_not() {
    // three parts of 0.33 on a bill of 0.99 at a rate of 0.5: each part alone would round to 0.17 (0.165 up), 0.51 against a bill of 0.50
    let w = world();
    let conn = w.conn;
    let mut conn = conn;
    let (a, b) = (friend(&conn, "A"), friend(&conn, "B"));
    let id = bill(&conn, w.food, "Odd", "0.99", "2026-01-01");
    set_split(&conn, id, &input(Mode::Exact, Some("0.33"), vec![with(who(a), "0.33"), with(who(b), "0.33")])).unwrap();
    change(&mut conn, "GBP", Some("0.5"), "2026-02-01").unwrap();
    let converted = bills(&conn).remove(0);
    assert_eq!(converted.1, 50);
    assert_eq!(converted.2 + converted.3.iter().map(|p| p.1).sum::<i64>(), 50);
    assert_eq!(converted.3.len() + usize::from(converted.2 > 0), 3, "three parts of 0.17, 0.17 and 0.16");
}

#[test]
fn the_ledger_is_converted_and_never_below_a_cent() {
    let (mut conn, anna, bob) = lived_in();
    let before = (ledger::balance_minor(&conn, anna).unwrap(), ledger::balance_minor(&conn, bob).unwrap());
    let done = change(&mut conn, "JPY", Some("0.0063"), "2026-02-01").unwrap();
    assert_eq!(done.ledger, 3);
    assert_eq!(entries(&conn), [13, 1, 3], "20.00 is 0.126, 0.01 stays a cent, 5.55 is 0.03");
    assert_ne!(before.0, 0);
    // a relabel leaves everything as it was
    let (mut other, _, _) = lived_in();
    let amounts = entries(&other);
    change(&mut other, "CHF", None, "2026-02-01").unwrap();
    assert_eq!(entries(&other), amounts);
}

#[test]
fn a_template_holds_proportions_so_it_is_left_alone() {
    let (mut conn, _, _) = lived_in();
    let before = super::splits::all_template_weights(&conn).unwrap();
    change(&mut conn, "USD", Some("2.5"), "2026-02-01").unwrap();
    assert_eq!(super::splits::all_template_weights(&conn).unwrap(), before);
    let item = recurring::list(&conn).unwrap().remove(0);
    assert_eq!(item.amount.as_deref(), Some("2250.00"), "the item's own amount is converted as before");
    let paid = recurring::pay(&conn, item.id, None, Some("2026-01-01"), "2026-01-01", None, None).unwrap();
    assert_eq!(paid.my_share, "1500.00");
}

#[test]
fn a_conversion_is_not_an_edit_so_no_row_gets_a_new_stamp() {
    let (mut conn, ..) = lived_in();
    let stamps = |conn: &Connection| -> Vec<String> { conn.prepare("SELECT stamp FROM transactions UNION ALL SELECT stamp FROM ledger_entries UNION ALL SELECT stamp FROM people").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect() };
    let before = stamps(&conn);
    change(&mut conn, "USD", Some("1.08"), "2026-02-01").unwrap();
    assert_eq!(stamps(&conn), before);
}

/// Makes the friends of `to` the same people as those of `from`: friends are matched by their sync id, so two devices that have met
/// hold the same ones, and the odd cents of a conversion follow them.
fn same_friends(from: &Connection, to: &Connection) {
    let all: Vec<(String, String)> = from.prepare("SELECT name, sync_id FROM people").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    for (name, sync_id) in all {
        to.execute("UPDATE people SET sync_id = ?1 WHERE name = ?2", rusqlite::params![sync_id, name]).unwrap();
    }
}

#[test]
fn two_devices_converting_with_the_same_rate_end_with_the_same_numbers() {
    for rate in ["1.08", "0.0063", "160", "0.5", "2.5", "1.00000001"] {
        let (mut desktop, ..) = lived_in();
        let (mut phone, ..) = lived_in();
        same_friends(&desktop, &phone);
        change(&mut desktop, "USD", Some(rate), "2026-02-01").unwrap();
        let (generation, from, to, recorded_rate, stamp): (i64, String, String, Option<String>, String) =
            desktop.query_row("SELECT id, from_currency, to_currency, rate, stamp FROM currency_changes", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).unwrap();
        let tx = phone.transaction().unwrap();
        sync::stamp::set_applying(&tx, true).unwrap();
        apply_recorded(&tx, &Recorded { generation, from: &from, to: &to, rate: recorded_rate.as_deref(), stamp: &stamp }, "2026-02-01").unwrap();
        sync::stamp::set_applying(&tx, false).unwrap();
        tx.commit().unwrap();
        assert_eq!(bills(&desktop), bills(&phone), "rate {rate}");
        assert_eq!(entries(&desktop), entries(&phone), "rate {rate}");
        assert!(super::splits::check(&phone).unwrap());
    }
}

#[test]
fn random_bills_and_rates_always_leave_parts_that_add_up() {
    let mut rng = Rng(0xC0FFEE);
    for round in 0..60 {
        let w = world();
        let mut conn = w.conn;
        let friends: Vec<i64> = (0..4).map(|i| friend(&conn, &format!("F{i}"))).collect();
        for i in 0..12 {
            let minor = rng.between(1, 200_000) as i64;
            let id = bill(&conn, w.food, &format!("B{i}"), &crate::money::format_amount(minor), "2026-01-01");
            let count = rng.between(1, 4) as usize;
            let typed: Vec<_> = friends.iter().take(count).map(|f| with(who(*f), &rng.between(0, 6).to_string())).collect();
            let _ = set_split(&conn, id, &input(Mode::Shares, Some(&rng.between(1, 6).to_string()), typed));
        }
        let rate = ["0.0001", "0.37", "1.08", "3.333", "160", "999.99"][round % 6];
        change(&mut conn, "USD", Some(rate), "2026-02-01").unwrap_or_else(|e| panic!("rate {rate}: {e}"));
        assert!(super::splits::check(&conn).unwrap(), "rate {rate} round {round}");
        for (name, amount, cost, parts) in bills(&conn) {
            assert_eq!(cost + parts.iter().map(|p| p.1).sum::<i64>(), amount, "{name} at {rate}");
            assert!(cost >= 0 && amount >= 1, "{name} at {rate}");
        }
    }
}
