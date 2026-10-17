use super::fixtures::*;
use super::ledger::{self, Kind, NewEntry};
use super::people::{archive, create, get, list, rename, restore, set_reminder};
use super::splits::set_split;
use crate::vault::AppError;

#[test]
fn a_friend_is_a_name_and_a_zero_balance() {
    let w = world();
    let anna = create(&w.conn, "  Anna  ").unwrap();
    assert_eq!((anna.name.as_str(), anna.archived, anna.balance.as_str(), anna.remind_on.clone()), ("Anna", false, "0.00", None));
    let json = serde_json::to_value(&anna).unwrap();
    assert_eq!(json["id"], anna.id.to_string(), "ids are text, like everywhere");
    assert!(json.get("balanceMinor").is_none() && json.get("balance_minor").is_none(), "the minor units do not leave the app");
    assert_eq!(get(&w.conn, anna.id).unwrap(), anna);
    assert_eq!(get(&w.conn, 99), Err(AppError::NotFound));
}

#[test]
fn names_follow_the_rule_of_categories() {
    let w = world();
    for bad in ["", "   ", "tab\there", "new\nline", &"x".repeat(51)] {
        assert_eq!(create(&w.conn, bad), Err(AppError::InvalidInput), "{bad:?}");
    }
    assert!(create(&w.conn, &"x".repeat(50)).is_ok());
    assert!(create(&w.conn, "Zoë 日本語 ☃").is_ok());
    // a name that looks like markup or a formula is only text
    assert!(create(&w.conn, "<b>=1+1</b>").is_ok());
}

#[test]
fn a_name_is_taken_while_a_friend_with_it_is_not_archived_ignoring_case() {
    let w = world();
    let anna = create(&w.conn, "Anna").unwrap();
    assert_eq!(create(&w.conn, "anna"), Err(AppError::NameExists));
    assert_eq!(create(&w.conn, "ANNA "), Err(AppError::NameExists));
    let bob = create(&w.conn, "Bob").unwrap();
    assert_eq!(rename(&w.conn, bob.id, "ANNA"), Err(AppError::NameExists));
    assert_eq!(rename(&w.conn, bob.id, "Bobby").unwrap().name, "Bobby");
    assert_eq!(rename(&w.conn, bob.id, "bobby").unwrap().name, "bobby", "a friend can change the case of their own name");
    archive(&w.conn, anna.id).unwrap();
    let second = create(&w.conn, "Anna").unwrap();
    assert_eq!(restore(&w.conn, anna.id), Err(AppError::NameExists), "the name was taken in the meantime");
    archive(&w.conn, second.id).unwrap();
    assert!(!restore(&w.conn, anna.id).unwrap().archived);
    assert_eq!(rename(&w.conn, 999, "X"), Err(AppError::NotFound));
}

#[test]
fn archiving_hides_a_friend_and_removes_nothing() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let bob = friend(&w.conn, "Bob");
    let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-01-01");
    set_split(&w.conn, dinner, &equal(vec![who(anna), who(bob)])).unwrap();
    ledger::add(&w.conn, &NewEntry { person_id: anna.to_string(), kind: Kind::Lent, amount: "5.00".into(), date: "2026-01-02".into(), note: None, transaction_id: None }).unwrap();
    set_reminder(&w.conn, anna, Some("2026-03-01"), Some(7)).unwrap();
    let archived = archive(&w.conn, anna).unwrap();
    assert!(archived.archived);
    assert_eq!((archived.balance.as_str(), archived.remind_on.as_deref(), archived.remind_every_days), ("35.00", Some("2026-03-01"), Some(7)), "balance and reminder stay");
    assert_eq!(list(&w.conn, false).unwrap().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Bob"]);
    assert_eq!(list(&w.conn, true).unwrap().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Bob", "Anna"], "archived ones come last");
    assert_eq!(ledger::statement(&w.conn, anna).unwrap().lines.len(), 2, "the history stays");
    assert_eq!(archive(&w.conn, 99), Err(AppError::NotFound));
}

#[test]
fn the_list_has_the_largest_balance_first_in_either_direction() {
    let w = world();
    let (anna, bob, carl) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"), friend(&w.conn, "Carl"));
    friend(&w.conn, "Dina");
    let entry = |person: i64, kind: Kind, amount: &str| {
        ledger::add(&w.conn, &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: "2026-01-01".into(), note: None, transaction_id: None }).unwrap();
    };
    entry(anna, Kind::Lent, "10.00");
    entry(bob, Kind::Borrowed, "30.00");
    entry(carl, Kind::Lent, "20.00");
    let names: Vec<(String, String)> = list(&w.conn, false).unwrap().into_iter().map(|p| (p.name, p.balance)).collect();
    assert_eq!(names, [("Bob".into(), "-30.00".into()), ("Carl".into(), "20.00".into()), ("Anna".into(), "10.00".into()), ("Dina".into(), "0.00".into())]);
}

#[test]
fn a_reminder_is_a_date_and_perhaps_a_number_of_days() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let set = set_reminder(&w.conn, anna, Some("2026-03-01"), None).unwrap();
    assert_eq!((set.remind_on.as_deref(), set.remind_every_days), (Some("2026-03-01"), None));
    assert_eq!(set_reminder(&w.conn, anna, Some("2026-03-01"), Some(14)).unwrap().remind_every_days, Some(14));
    assert_eq!(set_reminder(&w.conn, anna, Some("2026-03-01"), Some(365)).unwrap().remind_every_days, Some(365));
    for bad in [(Some("2026-03-01"), Some(0)), (Some("2026-03-01"), Some(366)), (None, Some(7)), (Some("2026-02-30"), None), (Some("soon"), None), (Some("2026-3-1"), None)] {
        assert_eq!(set_reminder(&w.conn, anna, bad.0, bad.1), Err(AppError::InvalidInput), "{bad:?}");
    }
    assert_eq!(get(&w.conn, anna).unwrap().remind_every_days, Some(365), "a refused change changes nothing");
    let cleared = set_reminder(&w.conn, anna, None, None).unwrap();
    assert_eq!((cleared.remind_on, cleared.remind_every_days), (None, None));
    assert_eq!(set_reminder(&w.conn, 99, None, None), Err(AppError::NotFound));
}

#[test]
fn nothing_but_the_name_and_the_reminder_is_stored_about_a_person() {
    let w = world();
    let columns: Vec<String> = w.conn.prepare("SELECT name FROM pragma_table_info('people')").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    for forbidden in ["phone", "email", "address", "contact", "note", "birth", "photo", "surname"] {
        assert!(!columns.iter().any(|c| c.contains(forbidden)), "{columns:?}");
    }
}
