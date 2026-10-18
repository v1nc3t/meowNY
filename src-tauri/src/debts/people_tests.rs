use super::fixtures::*;
use super::people::{archive, create, rename, restore};
use crate::vault::AppError;

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
fn nothing_but_the_name_and_the_reminder_is_stored_about_a_person() {
    let w = world();
    let columns: Vec<String> = w.conn.prepare("SELECT name FROM pragma_table_info('people')").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    for forbidden in ["phone", "email", "address", "contact", "note", "birth", "photo", "surname"] {
        assert!(!columns.iter().any(|c| c.contains(forbidden)), "{columns:?}");
    }
}
