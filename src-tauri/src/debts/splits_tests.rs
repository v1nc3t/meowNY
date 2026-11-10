use super::fixtures::*;
use super::splits::{check, clear_split, clear_template, preview, set_split, set_template, Mode, SplitInput};
use super::{ledger, people};
use crate::recurring::{self, NewRecurring, Unit};
use crate::transactions::{self, TransactionPatch};
use crate::vault::AppError;

const INVALID: AppError = AppError::InvalidInput;

// ---- sharing a bill --------------------------------------------------------------------------

#[test]
fn a_guest_has_a_part_that_leaves_your_cost_and_makes_no_debt() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-01");
    set_split(&w.conn, dinner, &equal(vec![who(anna), guest("Carl")])).unwrap();
    assert_eq!(cost(&w.conn, dinner), 3000);
    assert_eq!(ledger::balance_minor(&w.conn, anna).unwrap(), 3000, "only the friend owes");
    let shown = transactions::get(&w.conn, dinner).unwrap();
    assert_eq!(shown.split[1].guest.as_deref(), Some("Carl"));
    assert_eq!(shown.split[1].person_id, None);
}

#[test]
fn a_part_that_rounds_to_nothing_or_a_weight_of_zero_leaves_the_bill() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let tip = bill(&w.conn, w.food, "Tip", "0.01", "2026-01-01");
    set_split(&w.conn, tip, &equal(vec![who(anna), who(bob)])).unwrap_err();
    // one cent between you and one friend: it is yours (the lower position), so nobody else has a part and there is no split
    let dinner = bill(&w.conn, w.food, "Dinner", "100.00", "2026-01-01");
    set_split(&w.conn, dinner, &input(Mode::Shares, Some("1"), vec![with(who(anna), "1"), with(who(bob), "0")])).unwrap();
    assert_eq!(parts(&w.conn, dinner), [("Anna".to_string(), 5000)], "a friend with 0 is dropped when the bill is saved");
    // you paid for someone's lunch: your row is 0
    set_split(&w.conn, dinner, &input(Mode::Shares, Some("0"), vec![with(who(anna), "1")])).unwrap();
    assert_eq!((parts(&w.conn, dinner), cost(&w.conn, dinner)), (vec![("Anna".to_string(), 10_000)], 0));
}

#[test]
fn numbers_that_do_not_add_up_or_make_no_sense_are_refused_and_change_nothing() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let dinner = bill(&w.conn, w.food, "Dinner", "100.00", "2026-01-01");
    set_split(&w.conn, dinner, &equal(vec![who(anna)])).unwrap();
    let before = parts(&w.conn, dinner);
    for bad in [
        input(Mode::Percent, Some("50.00"), vec![with(who(anna), "49.99")]),
        input(Mode::Percent, Some("50.00"), vec![with(who(anna), "50.01")]),
        input(Mode::Percent, None, vec![with(who(anna), "100.01")]),
        input(Mode::Percent, None, vec![with(who(anna), "60.00"), with(who(bob), "50.00")]),
        input(Mode::Percent, Some("50"), vec![with(who(anna), "50.00")]),
        input(Mode::Exact, Some("10.00"), vec![with(who(anna), "89.99")]),
        input(Mode::Exact, Some("10.00"), vec![with(who(anna), "90.01")]),
        input(Mode::Exact, None, vec![with(who(anna), "100.01")]),
        input(Mode::Exact, Some("10.00"), vec![with(who(anna), "x")]),
        input(Mode::Shares, Some("0"), vec![with(who(anna), "0")]),
        input(Mode::Shares, None, vec![with(who(anna), "1")]),
        input(Mode::Shares, Some("1"), vec![who(anna)]),
        input(Mode::Shares, Some("1"), vec![with(who(anna), "1.5")]),
        input(Mode::Shares, Some("1"), vec![with(who(anna), "-1")]),
        input(Mode::Shares, Some("1"), vec![with(who(anna), "")]),
        input(Mode::Equal, Some("1"), vec![who(anna)]),
        input(Mode::Equal, None, vec![with(who(anna), "1")]),
        input(Mode::Equal, None, vec![]),
        input(Mode::Equal, None, vec![who(anna), who(anna)]),
        input(Mode::Equal, None, vec![guest("Eve"), guest("eve")]),
        input(Mode::Equal, None, vec![guest("")]),
        input(Mode::Equal, None, vec![guest("bad\nname")]),
        input(Mode::Equal, None, vec![guest(&"x".repeat(51))]),
        input(Mode::Equal, None, vec![super::splits::Part::default()]),
        input(Mode::Equal, None, vec![super::splits::Part { person_id: Some(anna.to_string()), guest: Some("Eve".into()), value: None }]),
        input(Mode::Equal, None, vec![super::splits::Part { person_id: Some("999".into()), guest: None, value: None }]),
        input(Mode::Equal, None, vec![super::splits::Part { person_id: Some("abc".into()), guest: None, value: None }]),
    ] {
        assert_eq!(set_split(&w.conn, dinner, &bad), Err(INVALID), "{bad:?}");
    }
    assert_eq!(parts(&w.conn, dinner), before, "nothing changed");
    assert_eq!(set_split(&w.conn, 9999, &equal(vec![who(anna)])), Err(AppError::NotFound));
}

#[test]
fn up_to_thirty_others_can_share_a_bill() {
    let w = world();
    let dinner = bill(&w.conn, w.food, "Party", "310.00", "2026-01-01");
    let thirty: Vec<_> = (0..30).map(|i| guest(&format!("Guest {i}"))).collect();
    set_split(&w.conn, dinner, &equal(thirty.clone())).unwrap();
    assert_eq!(parts(&w.conn, dinner).len(), 30);
    assert_eq!(cost(&w.conn, dinner), 1000, "310.00 among thirty-one is ten each");
    let mut thirty_one = thirty;
    thirty_one.push(guest("One too many"));
    assert_eq!(set_split(&w.conn, dinner, &equal(thirty_one)), Err(INVALID));
}

#[test]
fn an_income_is_not_split_and_an_archived_friend_is_not_added_but_stays_where_they_are() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let pay = bill(&w.conn, w.salary, "Pay", "3000.00", "2026-01-25");
    assert_eq!(set_split(&w.conn, pay, &equal(vec![who(anna)])), Err(INVALID));
    let dinner = bill(&w.conn, w.food, "Dinner", "100.00", "2026-01-01");
    set_split(&w.conn, dinner, &equal(vec![who(anna), who(bob)])).unwrap();
    people::archive(&w.conn, bob).unwrap();
    let lunch = bill(&w.conn, w.food, "Lunch", "100.00", "2026-01-02");
    assert_eq!(set_split(&w.conn, lunch, &equal(vec![who(bob)])), Err(INVALID), "a new bill cannot name an archived friend");
    set_split(&w.conn, dinner, &input(Mode::Percent, None, vec![with(who(anna), "30.00"), with(who(bob), "30.00")])).unwrap();
    assert_eq!(parts(&w.conn, dinner), [("Anna".to_string(), 3000), ("Bob".to_string(), 3000)], "but one already on it stays editable");
}

#[test]
fn changing_the_split_touches_only_the_parts_that_differ() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-01");
    set_split(&w.conn, dinner, &equal(vec![who(anna), who(bob)])).unwrap();
    let ids = |conn: &rusqlite::Connection| -> Vec<i64> { conn.prepare("SELECT id FROM transaction_split_lines ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect() };
    let before = ids(&w.conn);
    set_split(&w.conn, dinner, &equal(vec![who(anna), who(bob)])).unwrap();
    assert_eq!(ids(&w.conn), before, "the same split again changes no row");
    let audited: i64 = one(&w.conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'transaction_split_lines'");
    set_split(&w.conn, dinner, &input(Mode::Shares, Some("1"), vec![with(who(anna), "1"), with(who(bob), "2")])).unwrap();
    assert_eq!(parts(&w.conn, dinner), [("Anna".to_string(), 2250), ("Bob".to_string(), 4500)]);
    assert_eq!(ids(&w.conn), before, "the same people keep their rows");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM audit_log WHERE entity_type = 'transaction_split_lines'"), audited + 2);
    set_split(&w.conn, dinner, &equal(vec![who(bob), guest("Carl")])).unwrap();
    assert_eq!(parts(&w.conn, dinner), [("Bob".to_string(), 3000), ("Carl".to_string(), 3000)]);
    assert_eq!(ledger::balance_minor(&w.conn, anna).unwrap(), 0, "Anna is off the bill");
}

#[test]
fn a_bill_a_friend_paid_needs_the_payer_on_it_and_a_part_of_yours() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-01");
    let paid_by = |payer: &str, split: SplitInput| SplitInput { paid_by: Some(payer.to_string()), ..split };
    assert_eq!(set_split(&w.conn, dinner, &paid_by(&bob.to_string(), equal(vec![who(anna)]))), Err(INVALID), "the payer is not on the bill");
    assert_eq!(set_split(&w.conn, dinner, &paid_by("x", equal(vec![who(anna)]))), Err(INVALID), "not an id");
    assert_eq!(set_split(&w.conn, dinner, &paid_by(&anna.to_string(), input(Mode::Shares, Some("0"), vec![with(who(anna), "1")]))), Err(INVALID), "nothing of the bill is yours");
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transaction_split_lines") + one(&w.conn, "SELECT count(*) FROM transactions WHERE paid_by IS NOT NULL"), 0, "nothing was written");
    // the payer's own part may be 0: they have no line, and what Bob owes is owed to them
    set_split(&w.conn, dinner, &paid_by(&anna.to_string(), input(Mode::Shares, Some("1"), vec![with(who(anna), "0"), with(who(bob), "1")]))).unwrap();
    assert_eq!(parts(&w.conn, dinner), [("Bob".to_string(), 4500)]);
    assert_eq!((cost(&w.conn, dinner), ledger::balance_minor(&w.conn, anna).unwrap(), ledger::balance_minor(&w.conn, bob).unwrap()), (4500, -4500, 0));
    assert_eq!(transactions::get(&w.conn, dinner).unwrap().paid_by.map(|p| (p.person_id, p.name)), Some((anna.to_string(), "Anna".to_string())));
    // the same split again changes nothing, also once the payer is archived (with no line, they are on the bill as its payer), and taking
    // the split off makes the bill yours again
    people::archive(&w.conn, anna).unwrap();
    let stamp = text(&w.conn, "SELECT stamp FROM transactions");
    set_split(&w.conn, dinner, &paid_by(&anna.to_string(), input(Mode::Shares, Some("1"), vec![with(who(anna), "0"), with(who(bob), "1")]))).unwrap();
    assert_eq!(text(&w.conn, "SELECT stamp FROM transactions"), stamp, "no new stamp for no change");
    clear_split(&w.conn, dinner).unwrap();
    assert!(transactions::get(&w.conn, dinner).unwrap().paid_by.is_none());
    assert_eq!(ledger::balance_minor(&w.conn, anna).unwrap(), 0);
}

#[test]
fn a_bill_with_a_payer_stays_an_expense() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-01");
    set_split(&w.conn, dinner, &SplitInput { paid_by: Some(anna.to_string()), ..equal(vec![who(anna)]) }).unwrap();
    // with no parts left (a tiny amount can drop them) the payer is still there, so the bill is still shared
    w.conn.execute("DELETE FROM transaction_split_lines", []).unwrap();
    let to_income = TransactionPatch { category_id: Some(w.salary.to_string()), ..Default::default() };
    assert_eq!(transactions::update(&w.conn, dinner, &to_income), Err(INVALID));
    // straight into the table, as no code of the app does
    assert!(w.conn.execute("UPDATE transactions SET category_id = ?1 WHERE id = ?2", [w.salary, dinner]).is_err());
}

// ---- editing a shared bill -------------------------------------------------------------------

fn amount(conn: &rusqlite::Connection, id: i64, text: &str) -> Result<transactions::Transaction, AppError> {
    transactions::update(conn, id, &TransactionPatch { amount: Some(text.into()), ..Default::default() })
}

#[test]
fn a_new_amount_scales_every_part_yours_included() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let dinner = bill(&w.conn, w.food, "Dinner", "100.00", "2026-01-01");
    set_split(&w.conn, dinner, &input(Mode::Percent, Some("50.00"), vec![with(who(anna), "30.00"), with(who(bob), "20.00")])).unwrap();
    let grown = amount(&w.conn, dinner, "200.00").unwrap();
    assert_eq!(grown.my_share, "100.00");
    assert_eq!(parts(&w.conn, dinner), [("Anna".to_string(), 6000), ("Bob".to_string(), 4000)]);
    let shrunk = amount(&w.conn, dinner, "50.00").unwrap();
    assert_eq!(shrunk.my_share, "25.00");
    assert_eq!(parts(&w.conn, dinner), [("Anna".to_string(), 1500), ("Bob".to_string(), 1000)]);
    // an amount that does not divide: the parts still add up to the cent
    let odd = amount(&w.conn, dinner, "33.33").unwrap();
    let total: i64 = parts(&w.conn, dinner).iter().map(|p| p.1).sum();
    assert_eq!(total + cost(&w.conn, dinner), 3333, "{odd:?}");
    assert!(cost(&w.conn, dinner) >= 0);
}

#[test]
fn the_odd_cents_of_a_new_amount_do_not_depend_on_the_order_the_parts_were_stored_in() {
    // Two devices that convert a currency each rescale their own copy of a bill; the parts are stored in a different order on each, and
    // the cents that are left must still go to the same people.
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let one_way = bill(&w.conn, w.food, "Dinner", "100.01", "2026-01-01");
    let other_way = bill(&w.conn, w.food, "Dinner again", "100.01", "2026-01-01");
    let exact = |parts: Vec<_>| input(Mode::Exact, None, parts.into_iter().map(|p| with(p, "33.33")).collect());
    set_split(&w.conn, one_way, &exact(vec![who(anna), who(bob), guest("Cy")])).unwrap();
    set_split(&w.conn, other_way, &exact(vec![guest("Cy"), who(bob), who(anna)])).unwrap();
    assert_ne!(parts(&w.conn, one_way).iter().map(|p| p.0.clone()).collect::<Vec<_>>(), parts(&w.conn, other_way).iter().map(|p| p.0.clone()).collect::<Vec<_>>(), "stored in opposite orders");
    amount(&w.conn, one_way, "100.00").unwrap();
    amount(&w.conn, other_way, "100.00").unwrap();
    let by_name = |id: i64| {
        let mut all = parts(&w.conn, id);
        all.sort();
        all
    };
    assert_eq!(by_name(one_way), by_name(other_way));
    assert_eq!(cost(&w.conn, one_way), cost(&w.conn, other_way));
}

#[test]
fn a_tiny_amount_can_drop_a_part_and_the_rest_still_adds_up() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let dinner = bill(&w.conn, w.food, "Dinner", "100.00", "2026-01-01");
    set_split(&w.conn, dinner, &input(Mode::Shares, Some("98"), vec![with(who(anna), "1"), with(who(bob), "1")])).unwrap();
    amount(&w.conn, dinner, "0.01").unwrap();
    assert_eq!((parts(&w.conn, dinner), cost(&w.conn, dinner)), (vec![], 1), "a part of less than a cent is gone and the cent is yours");
}

#[test]
fn a_shared_bill_cannot_move_to_an_income_category_and_other_edits_keep_the_parts() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "100.00", "2026-01-01");
    set_split(&w.conn, dinner, &equal(vec![who(anna)])).unwrap();
    let to_income = TransactionPatch { category_id: Some(w.salary.to_string()), ..Default::default() };
    assert_eq!(transactions::update(&w.conn, dinner, &to_income), Err(INVALID));
    assert_eq!(parts(&w.conn, dinner), [("Anna".to_string(), 5000)]);
    let other = TransactionPatch { category_id: Some(w.rent.to_string()), name: Some("Dinner out".into()), payment_date: Some("2026-02-01".into()), ..Default::default() };
    let edited = transactions::update(&w.conn, dinner, &other).unwrap();
    assert_eq!((edited.name.as_str(), edited.my_share.as_str(), edited.split.len()), ("Dinner out", "50.00", 1));
}

#[test]
fn a_bill_that_cannot_be_written_leaves_its_parts_alone() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "100.00", "2026-01-01");
    set_split(&w.conn, dinner, &equal(vec![who(anna)])).unwrap();
    // a bad date is refused before anything is written
    let bad = TransactionPatch { amount: Some("10.00".into()), payment_date: Some("nope".into()), ..Default::default() };
    assert_eq!(transactions::update(&w.conn, dinner, &bad), Err(INVALID));
    assert_eq!((parts(&w.conn, dinner), one(&w.conn, "SELECT amount_minor FROM transactions")), (vec![("Anna".to_string(), 5000)], 10_000));
}

#[test]
fn a_sharing_that_asks_the_database_for_more_than_the_bill_is_stopped_there_too() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "10.00", "2026-01-01");
    // straight into the table, as no code of the app does
    assert!(w.conn.execute("INSERT INTO transaction_split_lines (transaction_id, person_id, amount_minor) VALUES (?1, ?2, 1001)", [dinner, anna]).is_err());
    assert!(check(&w.conn).unwrap());
}

// ---- the preview -----------------------------------------------------------------------------

#[test]
fn the_preview_says_what_is_left_while_the_numbers_do_not_add_up() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let exact = preview(&w.conn, 10_000, &input(Mode::Exact, Some("10.00"), vec![with(who(anna), "30.00"), with(who(bob), "20.00")])).unwrap();
    assert!(!exact.complete);
    assert_eq!((exact.left.as_str(), exact.mine.as_str()), ("40.00", "10.00"));
    assert_eq!(exact.parts.iter().map(|p| p.amount.as_str()).collect::<Vec<_>>(), ["30.00", "20.00"]);
    let percent = preview(&w.conn, 10_000, &input(Mode::Percent, Some("50.00"), vec![with(who(anna), "30.00")])).unwrap();
    assert!(!percent.complete);
    assert_eq!(percent.left, "20.00", "20.00 percent are left");
    assert_eq!((percent.mine.as_str(), percent.parts[0].amount.as_str()), ("50.00", "30.00"));
    let over = preview(&w.conn, 10_000, &input(Mode::Exact, Some("90.00"), vec![with(who(anna), "30.00")])).unwrap();
    assert!(!over.complete);
    assert_eq!(over.left, "-20.00");
    assert_eq!(preview(&w.conn, 0, &equal(vec![who(anna)])), Err(INVALID));
    assert_eq!(preview(&w.conn, 10_000, &equal(vec![])), Err(INVALID));
    // nothing was written
    assert_eq!(one(&w.conn, "SELECT count(*) FROM transaction_split_lines"), 0);
}

#[test]
fn the_one_value_left_out_gets_the_rest_whoever_it_is() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let amounts = |p: &super::splits::Preview| (p.mine.clone(), p.parts.iter().map(|x| x.amount.clone()).collect::<Vec<_>>(), p.left.clone());
    let split = |mode, mine, parts| preview(&w.conn, 10_000, &input(mode, mine, parts));
    let percent = split(Mode::Percent, Some("20.00"), vec![with(who(anna), "30.00"), who(bob)]).unwrap();
    assert!(percent.complete);
    assert_eq!(amounts(&percent), ("20.00".into(), vec!["30.00".into(), "50.00".into()], "0.00".into()));
    let exact = split(Mode::Exact, Some("10.00"), vec![who(anna), with(who(bob), "30.00")]).unwrap();
    assert!(exact.complete);
    assert_eq!(amounts(&exact), ("10.00".into(), vec!["60.00".into(), "30.00".into()], "0.00".into()));
    // typing too much leaves the one left out with 0 and shows by how much it is over
    let over = split(Mode::Exact, Some("90.00"), vec![with(who(anna), "30.00"), who(bob)]).unwrap();
    assert!(!over.complete);
    assert_eq!(amounts(&over), ("90.00".into(), vec!["30.00".into(), "0.00".into()], "-20.00".into()));
    // two left out, or a share left out, is not a number yet
    assert_eq!(split(Mode::Percent, None, vec![with(who(anna), "30.00"), who(bob)]), Err(INVALID));
    assert_eq!(split(Mode::Exact, Some("10.00"), vec![who(anna), who(bob)]), Err(INVALID));
    assert_eq!(split(Mode::Shares, Some("1"), vec![with(who(anna), "1"), who(bob)]), Err(INVALID));
    // and a saved split takes the rest the same way
    let dinner = bill(&w.conn, w.food, "Dinner", "100.00", "2026-01-01");
    set_split(&w.conn, dinner, &input(Mode::Percent, Some("20.00"), vec![who(anna)])).unwrap();
    assert_eq!((parts(&w.conn, dinner), cost(&w.conn, dinner)), (vec![("Anna".to_string(), 8000)], 2000));
}

// ---- a recurring item ------------------------------------------------------------------------

fn rent_item(conn: &rusqlite::Connection, category: i64, amount: Option<&str>) -> recurring::Recurring {
    recurring::create(
        conn,
        &NewRecurring {
            name: "Rent".into(),
            amount: amount.map(str::to_string),
            category_id: category.to_string(),
            start_date: "2026-01-01".into(),
            interval_unit: Unit::Month,
            interval_count: 1,
            end_date: None,
            description: None,
        },
    )
    .unwrap()
}

#[test]
fn exact_amounts_in_a_template_give_exactly_what_was_typed_for_the_planned_amount() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let item = rent_item(&w.conn, w.rent, Some("900.00"));
    set_template(&w.conn, item.id, &input(Mode::Exact, Some("350.00"), vec![with(who(anna), "300.00"), with(who(bob), "250.00")])).unwrap();
    let paid = recurring::pay(&w.conn, item.id, None, Some("2026-01-01"), "2026-01-01", None, None).unwrap();
    assert_eq!((parts(&w.conn, paid.id), paid.my_share.as_str()), (vec![("Anna".to_string(), 30_000), ("Bob".to_string(), 25_000)], "350.00"));
    // the exact amounts must add up to the item's amount
    assert_eq!(set_template(&w.conn, item.id, &input(Mode::Exact, Some("350.00"), vec![with(who(anna), "300.01")])), Err(INVALID));
    assert_eq!(set_template(&w.conn, item.id, &input(Mode::Exact, None, vec![with(who(anna), "901.00")])), Err(INVALID));
}

#[test]
fn paying_a_later_occurrence_on_its_own_applies_the_split_like_any_payment() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let item = rent_item(&w.conn, w.rent, Some("900.00"));
    set_template(&w.conn, item.id, &equal(vec![who(anna)])).unwrap();
    let paid = recurring::pay(&w.conn, item.id, None, Some("2026-03-05"), "2026-03-05", Some("2026-03-01"), None).unwrap();
    assert_eq!((parts(&w.conn, paid.id), paid.my_share.as_str()), (vec![("Anna".to_string(), 45_000)], "450.00"));
    assert_eq!(recurring::get(&w.conn, item.id).unwrap().next_due_date, "2026-01-01", "the item stays on its oldest open occurrence");
}

#[test]
fn an_item_with_no_amount_is_split_by_percent_or_shares() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let bill_item = rent_item(&w.conn, w.rent, None);
    assert_eq!(set_template(&w.conn, bill_item.id, &input(Mode::Exact, None, vec![with(who(anna), "10.00")])), Err(INVALID), "exact amounts need an amount to add up to");
    set_template(&w.conn, bill_item.id, &input(Mode::Percent, Some("60.00"), vec![with(who(anna), "40.00")])).unwrap();
    let paid = recurring::pay(&w.conn, bill_item.id, Some("85.50"), Some("2026-01-01"), "2026-01-01", None, None).unwrap();
    assert_eq!(parts(&w.conn, paid.id), [("Anna".to_string(), 3420)]);
    assert_eq!(paid.my_share, "51.30");
    let shown = recurring::get(&w.conn, bill_item.id).unwrap().split.unwrap();
    assert_eq!((shown.mine, shown.parts[0].weight, shown.parts[0].person.as_deref()), (6000, 4000, Some("Anna")), "percent is kept in hundredths");
}

#[test]
fn editing_the_template_changes_the_payments_to_come_and_not_the_ones_made() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let item = rent_item(&w.conn, w.rent, Some("900.00"));
    set_template(&w.conn, item.id, &equal(vec![who(anna)])).unwrap();
    let first = recurring::pay(&w.conn, item.id, None, Some("2026-01-01"), "2026-01-01", None, None).unwrap();
    set_template(&w.conn, item.id, &equal(vec![who(bob), guest("Carl")])).unwrap();
    let second = recurring::pay(&w.conn, item.id, None, Some("2026-02-01"), "2026-02-01", None, None).unwrap();
    assert_eq!(parts(&w.conn, first.id), [("Anna".to_string(), 45_000)]);
    assert_eq!(parts(&w.conn, second.id), [("Bob".to_string(), 30_000), ("Carl".to_string(), 30_000)]);
    clear_template(&w.conn, item.id).unwrap();
    let third = recurring::pay(&w.conn, item.id, None, Some("2026-03-01"), "2026-03-01", None, None).unwrap();
    assert!(parts(&w.conn, third.id).is_empty() && third.my_share == "900.00");
    assert!(recurring::get(&w.conn, item.id).unwrap().split.is_none());
    assert_eq!(parts(&w.conn, second.id).len(), 2, "the payments made keep theirs");
}

#[test]
fn an_archived_friend_keeps_their_place_in_a_template() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let item = rent_item(&w.conn, w.rent, Some("900.00"));
    set_template(&w.conn, item.id, &equal(vec![who(anna)])).unwrap();
    people::archive(&w.conn, anna).unwrap();
    let paid = recurring::pay(&w.conn, item.id, None, Some("2026-01-01"), "2026-01-01", None, None).unwrap();
    assert_eq!(parts(&w.conn, paid.id), [("Anna".to_string(), 45_000)], "the debt goes on growing");
    set_template(&w.conn, item.id, &input(Mode::Percent, None, vec![with(who(anna), "40.00")])).unwrap();
    let other = rent_item(&w.conn, w.rent, Some("1.00"));
    assert_eq!(set_template(&w.conn, other.id, &equal(vec![who(anna)])), Err(INVALID), "but a new template cannot name them");
}

#[test]
fn an_income_item_has_no_template_and_a_split_item_cannot_become_income() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let pay = rent_item(&w.conn, w.salary, Some("3000.00"));
    assert_eq!(set_template(&w.conn, pay.id, &equal(vec![who(anna)])), Err(INVALID));
    let item = rent_item(&w.conn, w.rent, Some("900.00"));
    set_template(&w.conn, item.id, &equal(vec![who(anna)])).unwrap();
    let to_income = recurring::RecurringPatch { category_id: Some(w.salary.to_string()), ..Default::default() };
    assert_eq!(recurring::update(&w.conn, item.id, &to_income), Err(INVALID));
    let to_other = recurring::RecurringPatch { category_id: Some(w.food.to_string()), amount: Some(Some("1000.00".into())), ..Default::default() };
    assert!(recurring::update(&w.conn, item.id, &to_other).unwrap().split.is_some(), "other edits keep the template");
    assert_eq!(set_template(&w.conn, 999, &equal(vec![who(anna)])), Err(AppError::NotFound));
    assert_eq!(clear_template(&w.conn, 999), Err(AppError::NotFound));
}

#[test]
fn a_template_a_friend_pays_gives_the_payer_to_each_payment_and_needs_a_weight_of_yours() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let item = rent_item(&w.conn, w.rent, Some("900.00"));
    let paid_by = |payer: i64, split: SplitInput| SplitInput { paid_by: Some(payer.to_string()), ..split };
    assert_eq!(set_template(&w.conn, item.id, &paid_by(bob, equal(vec![who(anna)]))), Err(INVALID), "the payer is not on it");
    assert_eq!(set_template(&w.conn, item.id, &paid_by(anna, input(Mode::Shares, Some("0"), vec![with(who(anna), "1")]))), Err(INVALID), "none of a payment is yours");
    assert!(recurring::get(&w.conn, item.id).unwrap().split.is_none());
    set_template(&w.conn, item.id, &paid_by(anna, equal(vec![who(anna), who(bob)]))).unwrap();
    assert_eq!(recurring::get(&w.conn, item.id).unwrap().split.unwrap().paid_by.map(|p| p.name), Some("Anna".to_string()));
    let first = recurring::pay(&w.conn, item.id, None, Some("2026-01-01"), "2026-01-01", None, None).unwrap();
    assert_eq!((one(&w.conn, &format!("SELECT paid_by FROM transactions WHERE id = {}", first.id)), first.my_share.as_str()), (anna, "300.00"));
    assert_eq!((ledger::balance_minor(&w.conn, anna).unwrap(), ledger::balance_minor(&w.conn, bob).unwrap()), (-30_000, 0));
    // a template with no payer, and the payments to come are yours again; the one made keeps its payer
    set_template(&w.conn, item.id, &equal(vec![who(anna)])).unwrap();
    let second = recurring::pay(&w.conn, item.id, None, Some("2026-02-01"), "2026-02-01", None, None).unwrap();
    assert_eq!(one(&w.conn, &format!("SELECT paid_by IS NULL FROM transactions WHERE id = {}", second.id)), 1);
    assert_eq!(one(&w.conn, &format!("SELECT paid_by FROM transactions WHERE id = {}", first.id)), anna);
    set_template(&w.conn, item.id, &paid_by(anna, equal(vec![who(anna)]))).unwrap();
    clear_template(&w.conn, item.id).unwrap();
    assert_eq!(one(&w.conn, "SELECT count(*) FROM recurring_transactions WHERE split_paid_by IS NOT NULL"), 0, "taking the split off clears the payer");
}

#[test]
fn a_template_needs_somebody_to_share_with() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let item = rent_item(&w.conn, w.rent, Some("900.00"));
    assert_eq!(set_template(&w.conn, item.id, &input(Mode::Shares, Some("1"), vec![with(who(anna), "0")])), Err(INVALID));
    assert_eq!(set_template(&w.conn, item.id, &input(Mode::Shares, Some("0"), vec![with(who(anna), "0")])), Err(INVALID));
    assert_eq!(set_template(&w.conn, item.id, &equal(vec![])), Err(INVALID));
    assert!(recurring::get(&w.conn, item.id).unwrap().split.is_none());
}

#[test]
fn a_payment_and_its_parts_are_made_together_or_not_at_all() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let item = rent_item(&w.conn, w.rent, Some("900.00"));
    set_template(&w.conn, item.id, &equal(vec![who(anna)])).unwrap();
    // the same occurrence cannot be paid twice: the second payment fails and leaves nothing
    recurring::pay(&w.conn, item.id, None, Some("2026-01-01"), "2026-01-01", None, None).unwrap();
    w.conn.execute("UPDATE recurring_transactions SET next_due_date = start_date", []).unwrap();
    assert!(recurring::pay(&w.conn, item.id, None, Some("2026-01-01"), "2026-01-01", None, None).is_err());
    assert_eq!((one(&w.conn, "SELECT count(*) FROM transactions"), one(&w.conn, "SELECT count(*) FROM transaction_split_lines")), (1, 1));
}

// ---- the finished state ----------------------------------------------------------------------

#[test]
fn the_check_finds_a_state_the_steps_would_have_refused() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let dinner = bill(&w.conn, w.food, "Dinner", "10.00", "2026-01-01");
    let pay = bill(&w.conn, w.salary, "Pay", "30.00", "2026-01-25");
    let item = rent_item(&w.conn, w.rent, Some("900.00"));
    assert!(check(&w.conn).unwrap());
    w.conn.execute("UPDATE sync_clock SET applying = 1", []).unwrap();
    let add = |sql: &str, args: &[&dyn rusqlite::ToSql]| w.conn.execute(sql, args).unwrap();
    // parts above the bill
    add("INSERT INTO transaction_split_lines (transaction_id, person_id, amount_minor) VALUES (?1, ?2, 1001)", &[&dinner, &anna]);
    assert!(!check(&w.conn).unwrap());
    add("DELETE FROM transaction_split_lines", &[]);
    assert!(check(&w.conn).unwrap());
    // a part of an income
    add("INSERT INTO transaction_split_lines (transaction_id, person_id, amount_minor) VALUES (?1, ?2, 100)", &[&pay, &anna]);
    assert!(!check(&w.conn).unwrap());
    add("DELETE FROM transaction_split_lines", &[]);
    // a template line with no weight for the payer, and a template that gives nothing to anyone
    add("INSERT INTO recurring_split_lines (recurring_transaction_id, person_id, weight) VALUES (?1, ?2, 1)", &[&item.id, &anna]);
    assert!(!check(&w.conn).unwrap());
    add("UPDATE recurring_transactions SET split_my_weight = 0", &[]);
    assert!(check(&w.conn).unwrap());
    add("DELETE FROM recurring_split_lines", &[]);
    assert!(!check(&w.conn).unwrap(), "a template whose weights are all zero");
    add("UPDATE recurring_transactions SET split_my_weight = NULL", &[]);
    assert!(check(&w.conn).unwrap());
    // a payer on an income, the payer of a template that has no weight of yours, and a payer who was merged into another friend
    add("UPDATE transactions SET paid_by = ?1 WHERE id = ?2", &[&anna, &pay]);
    assert!(!check(&w.conn).unwrap());
    add("UPDATE transactions SET paid_by = NULL", &[]);
    add("UPDATE recurring_transactions SET split_paid_by = ?1", &[&anna]);
    assert!(!check(&w.conn).unwrap());
    add("UPDATE recurring_transactions SET split_paid_by = NULL", &[]);
    add("UPDATE transactions SET paid_by = ?1 WHERE id = ?2", &[&anna, &dinner]);
    assert!(check(&w.conn).unwrap(), "a bill that is an expense can have a payer");
    add("UPDATE people SET merged_into = ?1", &[&"ab".repeat(16)]);
    assert!(!check(&w.conn).unwrap());
    add("UPDATE people SET merged_into = NULL", &[]);
    add("UPDATE transactions SET paid_by = NULL", &[]);
    // thirty-one parts
    for i in 0..31 {
        add("INSERT INTO transaction_split_lines (transaction_id, guest_name, amount_minor) VALUES (?1, ?2, 1)", &[&dinner, &format!("G{i}")]);
    }
    assert!(!check(&w.conn).unwrap());
}
