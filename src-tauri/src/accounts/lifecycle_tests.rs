//! The life of an account, row by row of the state machine (`docs/accounts-design.md`, section 5.1), for what happens on this device:
//! made, changed, reordered, archived, restored, deleted. Every refusal is checked to have changed nothing.

use super::fixtures::*;
use super::transfers;
use super::{archive, create, delete, get, list, reorder, restore, update, Account, AccountPatch, Kind, NewAccount, MAIN_ID, MAIN_NAME};
use crate::debts::ledger::{self, NewEntry};
use crate::recurring::{self, NewRecurring, Unit};
use crate::transactions::{self, TransactionPatch};
use crate::vault::AppError;

const NOT_FOUND: AppError = AppError::NotFound;
const NAME_EXISTS: AppError = AppError::NameExists;

fn names(conn: &rusqlite::Connection) -> Vec<String> {
    list(conn, TODAY).unwrap().into_iter().map(|a| a.name).collect()
}

fn rename(name: &str) -> AccountPatch {
    AccountPatch { name: Some(name.into()), ..Default::default() }
}

fn recurring_in(conn: &rusqlite::Connection, category: i64, account: i64) -> i64 {
    recurring::create(
        conn,
        &NewRecurring {
            name: "Rent".into(),
            amount: Some("900.00".into()),
            category_id: category.to_string(),
            start_date: "2026-01-01".into(),
            interval_unit: Unit::Month,
            interval_count: 1,
            end_date: None,
            description: None,
            account_id: Some(account.to_string()),
        },
    )
    .unwrap()
    .id
}

fn entry_in(conn: &rusqlite::Connection, person: i64, account: i64) -> i64 {
    ledger::add(
        conn,
        &NewEntry { person_id: person.to_string(), kind: ledger::Kind::Lent, amount: "5.00".into(), date: "2026-01-02".into(), note: None, transaction_id: None, account_id: Some(account.to_string()) },
    )
    .unwrap()
    .id
}

// ---- NONE: create ----------------------------------------------------------------------------

#[test]
fn a_new_account_goes_to_the_end_of_the_list_with_what_was_given_and_the_defaults_for_the_rest() {
    let w = world();
    let plain = create(&w.conn, &new_account("  Savings  "), TODAY).unwrap();
    assert_eq!(
        plain,
        Account {
            id: 2,
            name: "Savings".into(),
            kind: Kind::Checking,
            number: None,
            opening_balance: "0.00".into(),
            opening_date: TODAY.into(),
            archived: false,
            is_main: false,
            balance: "0.00".into(),
            change_this_month: "0.00".into(),
            last_activity: None,
            entries: 0,
            bic: None,
            notes: None,
            group: None,
            include_in_net_worth: true,
            virtual_balance: "0.00".into(),
            available: "0.00".into(),
        }
    );
    let card = create(
        &w.conn,
        &NewAccount { kind: Kind::CreditCard, number: Some(" 1234 5678 ".into()), opening_balance: Some("-120.00".into()), opening_date: Some("2026-01-01".into()), ..new_account("Card") },
        TODAY,
    )
    .unwrap();
    assert_eq!((card.kind, card.number.as_deref(), card.opening_balance.as_str(), card.opening_date.as_str(), card.balance.as_str()), (Kind::CreditCard, Some("1234 5678"), "-120.00", "2026-01-01", "-120.00"));
    assert_eq!(card.change_this_month, "0.00", "the opening balance moved the account in January, not this month");
    assert_eq!(card.last_activity, None, "and it is not activity");
    assert_eq!(create(&w.conn, &NewAccount { number: Some("   ".into()), ..new_account("Cash") }, TODAY).unwrap().number, None, "an empty number is none");
    assert_eq!(names(&w.conn), [MAIN_NAME, "Savings", "Card", "Cash"]);
    assert_eq!(text(&w.conn, "SELECT group_concat(position) FROM (SELECT position FROM accounts ORDER BY id)"), "0,1,2,3");
    let main = get(&w.conn, MAIN_ID, TODAY).unwrap();
    assert!(main.is_main && !main.archived && main.name == MAIN_NAME);
}

#[test]
fn an_account_with_a_bad_name_kind_number_balance_or_date_is_not_made() {
    let w = world();
    let bad = [
        ("empty name", new_account("")),
        ("blank name", new_account("   ")),
        ("long name", new_account(&"x".repeat(51))),
        ("control character in the name", new_account("Sav\ning")),
        ("long number", NewAccount { number: Some("1".repeat(51)), ..new_account("A") }),
        ("control character in the number", NewAccount { number: Some("12\t34".into()), ..new_account("A") }),
        ("date that is not real", NewAccount { opening_date: Some("2026-02-30".into()), ..new_account("A") }),
        ("date not padded", NewAccount { opening_date: Some("2026-2-3".into()), ..new_account("A") }),
        ("balance without decimals", NewAccount { opening_balance: Some("12".into()), ..new_account("A") }),
    ];
    for (what, input) in bad {
        assert_refused(&w.conn, INVALID, what, || create(&w.conn, &input, TODAY));
    }
    assert_refused(&w.conn, INVALID, "a today that is not a date", || create(&w.conn, &new_account("A"), "today"));
    assert_eq!(names(&w.conn), [MAIN_NAME]);
    assert!(create(&w.conn, &new_account(&"x".repeat(50)), TODAY).is_ok());
    // the kind is one of four: anything else does not even reach the code
    for kind in ["\"CHECKING\"", "\"SAVINGS\"", "\"CASH\"", "\"CREDIT_CARD\""] {
        assert!(serde_json::from_str::<Kind>(kind).is_ok(), "{kind}");
    }
    for kind in ["\"checking\"", "\"LOAN\"", "\"CreditCard\"", "\"\"", "null", "1"] {
        assert!(serde_json::from_str::<Kind>(kind).is_err(), "{kind}");
    }
    assert!(serde_json::from_str::<NewAccount>(r#"{"name":"A","kind":"CASH","color":"red"}"#).is_err(), "an unknown field is refused");
    assert!(serde_json::from_str::<NewAccount>(r#"{"name":"A"}"#).is_err(), "the kind is needed");
}

/// 5.5 item 16.
#[test]
fn the_opening_balance_may_be_anything_from_minus_to_plus_the_largest_amount_written_one_way() {
    let w = world();
    for (n, ok) in ["-9999999999.99", "9999999999.99", "0.00", "-0.01", "0.01"].iter().enumerate() {
        let made = create(&w.conn, &NewAccount { opening_balance: Some(ok.to_string()), ..new_account(&format!("Account {n}")) }, TODAY).unwrap();
        assert_eq!((made.opening_balance.as_str(), made.balance.as_str()), (*ok, *ok));
    }
    let first = 2;
    for bad in ["-10000000000.00", "10000000000.00", "-0.00", "1.5", "+1.00", "", "1,00", "--1.00", "1e3", " 1.00"] {
        assert_refused(&w.conn, INVALID, bad, || create(&w.conn, &NewAccount { opening_balance: Some(bad.to_string()), ..new_account("Other") }, TODAY));
        assert_refused(&w.conn, INVALID, bad, || update(&w.conn, first, &AccountPatch { opening_balance: Some(bad.to_string()), ..Default::default() }, TODAY));
    }
    assert_eq!(get(&w.conn, first, TODAY).unwrap().opening_balance, "-9999999999.99");
}

/// 5.5 item 12, the part on this device.
#[test]
fn a_name_that_an_active_account_has_is_refused_ignoring_case_and_an_archived_one_frees_it() {
    let w = world();
    let savings = account(&w.conn, "Savings");
    for taken in ["Savings", "savings", "  SAVINGS ", MAIN_NAME, "main ACCOUNT"] {
        assert_refused(&w.conn, NAME_EXISTS, taken, || create(&w.conn, &new_account(taken), TODAY));
    }
    let cash = account(&w.conn, "Cash");
    assert_refused(&w.conn, NAME_EXISTS, "rename to a taken name", || update(&w.conn, cash, &rename("savings"), TODAY));
    assert_refused(&w.conn, NAME_EXISTS, "rename to the main account's name", || update(&w.conn, cash, &rename(MAIN_NAME), TODAY));
    assert_refused(&w.conn, NAME_EXISTS, "rename the main account to a taken name", || update(&w.conn, MAIN_ID, &rename("Cash"), TODAY));
    // its own name, in another case, is not taken
    assert_eq!(update(&w.conn, cash, &rename("CASH"), TODAY).unwrap().name, "CASH");

    archive(&w.conn, savings, TODAY).unwrap();
    let second = create(&w.conn, &new_account("Savings"), TODAY).unwrap();
    assert_refused(&w.conn, NAME_EXISTS, "restore while another has the name", || restore(&w.conn, savings, TODAY));
    assert!(get(&w.conn, savings, TODAY).unwrap().archived);
    // once the name is free again, it comes back
    update(&w.conn, second.id, &rename("Rainy day"), TODAY).unwrap();
    assert!(!restore(&w.conn, savings, TODAY).unwrap().archived);
    // two archived accounts may share a name, and only one of them can come back
    archive(&w.conn, savings, TODAY).unwrap();
    update(&w.conn, second.id, &rename("Savings"), TODAY).unwrap();
    archive(&w.conn, second.id, TODAY).unwrap();
    restore(&w.conn, second.id, TODAY).unwrap();
    assert_refused(&w.conn, NAME_EXISTS, "the second of two archived accounts with one name", || restore(&w.conn, savings, TODAY));
}

// ---- ACTIVE: update --------------------------------------------------------------------------

#[test]
fn an_update_writes_the_fields_given_and_makes_a_stamp_only_when_something_changed() {
    let w = world();
    let id = account(&w.conn, "Savings");
    let made = stamp_of(&w.conn, "accounts", id);
    let patch = AccountPatch {
        name: Some(" Rainy day ".into()),
        kind: Some(Kind::Savings),
        number: Some(Some("IE29 AIBK".into())),
        opening_balance: Some("1500.00".into()),
        opening_date: Some("2026-01-01".into()),
        ..Default::default()
    };
    let changed = update(&w.conn, id, &patch, TODAY).unwrap();
    assert_eq!(
        (changed.name.as_str(), changed.kind, changed.number.as_deref(), changed.opening_balance.as_str(), changed.opening_date.as_str(), changed.balance.as_str()),
        ("Rainy day", Kind::Savings, Some("IE29 AIBK"), "1500.00", "2026-01-01", "1500.00")
    );
    let stamp = stamp_of(&w.conn, "accounts", id);
    assert!(stamp > made);

    // the same values again, and an empty patch: allowed, and nothing is written
    assert_eq!(assert_nothing_written(&w.conn, "the same values", || update(&w.conn, id, &patch, TODAY)), changed);
    assert_eq!(assert_nothing_written(&w.conn, "an empty patch", || update(&w.conn, id, &AccountPatch::default(), TODAY)), changed);

    // one field at a time; `number: null` clears it, a field left out stays
    let cleared = update(&w.conn, id, &serde_json::from_str::<AccountPatch>(r#"{"number":null}"#).unwrap(), TODAY).unwrap();
    assert_eq!((cleared.number, cleared.name.as_str(), cleared.opening_balance.as_str()), (None, "Rainy day", "1500.00"));
    assert!(stamp_of(&w.conn, "accounts", id) > stamp);
    assert!(serde_json::from_str::<AccountPatch>(r#"{"archived":true}"#).is_err(), "archiving is not an update");
    assert!(serde_json::from_str::<AccountPatch>(r#"{"position":3}"#).is_err());

    // the main account can be changed like any other, and gets its first real stamp
    let main = update(&w.conn, MAIN_ID, &AccountPatch { name: Some("Current account".into()), opening_balance: Some("250.00".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!((main.name.as_str(), main.balance.as_str(), main.is_main), ("Current account", "250.00", true));
    assert!(stamp_of(&w.conn, "accounts", MAIN_ID).as_str() > crate::sync::engine::UNSET_STAMP);
}

#[test]
fn an_update_with_a_bad_field_changes_nothing() {
    let w = world();
    let id = account(&w.conn, "Savings");
    let bad = [
        ("blank name", rename(" ")),
        ("long name", rename(&"x".repeat(51))),
        ("control character", rename("a\u{7}b")),
        ("long number", AccountPatch { number: Some(Some("1".repeat(51))), ..Default::default() }),
        ("bad date", AccountPatch { opening_date: Some("31/01/2026".into()), ..Default::default() }),
        ("bad balance", AccountPatch { opening_balance: Some("12.5".into()), ..Default::default() }),
        // one bad field refuses the good ones with it
        ("a good name with a bad date", AccountPatch { name: Some("Fine".into()), opening_date: Some("2026-13-01".into()), ..Default::default() }),
    ];
    for (what, patch) in bad {
        assert_refused(&w.conn, INVALID, what, || update(&w.conn, id, &patch, TODAY));
    }
    assert_eq!(get(&w.conn, id, TODAY).unwrap().name, "Savings");
}

// ---- the details: BIC, notes, group, net worth, virtual balance (section 20) ---------------------

use serde_json::{json, Value};

/// A new account of one field, built the way the page sends it (JSON), so that `null` and a missing field are what they are on the wire.
fn made(conn: &rusqlite::Connection, field: &str, value: Value, name: &str) -> Result<Account, AppError> {
    create(conn, &serde_json::from_value(json!({ "name": name, "kind": "CASH", field: value })).unwrap(), TODAY)
}

fn changed(conn: &rusqlite::Connection, id: i64, field: &str, value: Value) -> Result<Account, AppError> {
    update(conn, id, &serde_json::from_value(json!({ field: value })).unwrap(), TODAY)
}

/// What the page reads of an account in `field`.
fn read(account: &Account, field: &str) -> Value {
    serde_json::to_value(account).unwrap()[field].clone()
}

#[test]
fn a_new_account_keeps_its_details_cleaned_and_the_defaults_for_the_rest() {
    let w = world();
    let full = create(
        &w.conn,
        &NewAccount {
            bic: Some("  DEUTDEFF500 ".into()),
            notes: Some("  Opened in 2019.\r\nCall the branch on Mondays.  ".into()),
            group: Some(" Bank A ".into()),
            include_in_net_worth: Some(false),
            virtual_balance: Some("1000.00".into()),
            opening_balance: Some("-240.00".into()),
            ..new_account("Card")
        },
        TODAY,
    )
    .unwrap();
    assert_eq!(
        (full.bic.as_deref(), full.notes.as_deref(), full.group.as_deref(), full.include_in_net_worth, full.virtual_balance.as_str(), full.balance.as_str(), full.available.as_str()),
        (Some("DEUTDEFF500"), Some("Opened in 2019.\nCall the branch on Mondays."), Some("Bank A"), false, "1000.00", "-240.00", "760.00"),
        "trimmed, the line break made `\\n`, and available is the balance plus the virtual balance"
    );
    assert_eq!(get(&w.conn, full.id, TODAY).unwrap(), full);
    assert_eq!(list(&w.conn, TODAY).unwrap()[1], full);
    // nothing given: no BIC, no notes, no group, counted in net worth, no virtual balance; and `null` is nothing given
    let plain = create(&w.conn, &new_account("Plain"), TODAY).unwrap();
    assert_eq!((plain.bic, plain.notes, plain.group, plain.include_in_net_worth, plain.virtual_balance.as_str(), plain.available.as_str()), (None, None, None, true, "0.00", "0.00"));
    let nulls: NewAccount = serde_json::from_value(json!({ "name": "Nulls", "kind": "CASH", "bic": null, "notes": null, "group": null, "includeInNetWorth": null, "virtualBalance": null })).unwrap();
    let nulls = create(&w.conn, &nulls, TODAY).unwrap();
    assert_eq!((nulls.bic, nulls.notes, nulls.group, nulls.include_in_net_worth, nulls.virtual_balance.as_str()), (None, None, None, true, "0.00"));
    // a field that is only spaces is none
    let blank = create(&w.conn, &NewAccount { bic: Some("   ".into()), notes: Some(" \n\t \r\n ".into()), group: Some("  ".into()), ..new_account("Blank") }, TODAY).unwrap();
    assert_eq!((blank.bic, blank.notes, blank.group), (None, None, None));
    // the row in the table holds the cleaned text
    assert_eq!(text(&w.conn, &format!("SELECT bic || '|' || replace(notes, char(10), '/') || '|' || group_name || '|' || include_net_worth || '|' || virtual_balance_minor FROM accounts WHERE id = {}", full.id)), "DEUTDEFF500|Opened in 2019./Call the branch on Mondays.|Bank A|0|100000");
}

/// A field with what is accepted (as sent, and as read back) and what is refused.
type FieldCases = Vec<(&'static str, Vec<(String, Value)>, Vec<String>)>;

/// The values of each field: what is accepted with what it becomes, and what is refused, whether the account is made or changed.
fn details() -> FieldCases {
    let ok = |pairs: Vec<(String, &str)>| pairs.into_iter().map(|(input, shown)| (input, json!(shown))).collect::<Vec<_>>();
    vec![
        (
            "bic",
            ok(vec![("D".into(), "D"), ("x".repeat(20), &"x".repeat(20)), (" DEUT DEFF ".into(), "DEUT DEFF"), ("é".repeat(20), &"é".repeat(20)), (format!("  {}  ", "x".repeat(20)), &"x".repeat(20))]),
            vec!["x".repeat(21), "é".repeat(21), "AB\tCD".into(), "AB\nCD".into(), "AB\u{7}".into(), "A\u{85}B".into(), "AB\u{0}".into()],
        ),
        (
            "notes",
            ok(vec![
                ("x".repeat(1000), &"x".repeat(1000)),
                ("one\ntwo".into(), "one\ntwo"),
                ("one\r\ntwo".into(), "one\ntwo"),
                ("one\rtwo".into(), "one\ntwo"),
                ("é".repeat(1000), &"é".repeat(1000)),
                // the length is counted after the line breaks are made `\n`: 499 breaks and 2 letters are 1000 characters
                (format!("{}xx", "x\r\n".repeat(499)), &format!("{}xx", "x\n".repeat(499))),
            ]),
            vec![
                "x".repeat(1001),
                "é".repeat(1001),
                format!("{}xxx", "x\r\n".repeat(499)),
                "a\tb".into(),
                "a\u{7}b".into(),
                "a\u{0}b".into(),
                "a\u{85}b".into(),
                "a\u{1b}[0m".into(),
            ],
        ),
        (
            "group",
            ok(vec![("g".into(), "g"), ("x".repeat(50), &"x".repeat(50)), (" Bank A ".into(), "Bank A"), ("é".repeat(50), &"é".repeat(50))]),
            vec!["x".repeat(51), "é".repeat(51), "a\nb".into(), "a\tb".into(), "a\u{7}b".into()],
        ),
        (
            "virtualBalance",
            ok(vec![
                ("9999999999.99".into(), "9999999999.99"),
                ("-9999999999.99".into(), "-9999999999.99"),
                ("0.00".into(), "0.00"),
                ("-0.01".into(), "-0.01"),
                ("0.01".into(), "0.01"),
                ("1000.00".into(), "1000.00"),
            ]),
            ["10000000000.00", "-10000000000.00", "9999999999.995", "-0.00", "1.5", "+1.00", "", "1,00", "1e3", " 1.00", "1000", "--1.00", ".50", "0.5", "00.50", "1.000", "NaN", "-"].map(String::from).to_vec(),
        ),
    ]
}

#[test]
fn each_new_text_or_money_field_is_accepted_at_its_bound_and_refused_beyond_it_when_made_and_when_changed() {
    let w = world();
    let target = account(&w.conn, "Target");
    for (field, accepted, refused) in details() {
        for (n, (input, shown)) in accepted.iter().enumerate() {
            let made = made(&w.conn, field, json!(input), &format!("{field} {n}")).unwrap_or_else(|e| panic!("create {field} {input:?}: {e:?}"));
            assert_eq!(&read(&made, field), shown, "create {field}");
            let after = changed(&w.conn, target, field, json!(input)).unwrap_or_else(|e| panic!("update {field} {input:?}: {e:?}"));
            assert_eq!(&read(&after, field), shown, "update {field}");
            assert_eq!(read(&get(&w.conn, target, TODAY).unwrap(), field), *shown);
        }
        let kept = read(&get(&w.conn, target, TODAY).unwrap(), field);
        for (n, input) in refused.iter().enumerate() {
            assert_refused(&w.conn, INVALID, &format!("create {field} {input:?}"), || made(&w.conn, field, json!(input), &format!("refused {field} {n}")));
            assert_refused(&w.conn, INVALID, &format!("update {field} {input:?}"), || changed(&w.conn, target, field, json!(input)));
        }
        assert_eq!(read(&get(&w.conn, target, TODAY).unwrap(), field), kept, "{field}: a refused change leaves the value");
    }
}

#[test]
fn the_new_fields_of_the_json_a_page_sends_are_text_text_text_a_boolean_and_text_and_nothing_else() {
    for (what, body) in [
        ("a number for the BIC", json!({ "bic": 5 })),
        ("a boolean for the notes", json!({ "notes": true })),
        ("an array for the group", json!({ "group": ["a"] })),
        ("text for the flag", json!({ "includeInNetWorth": "yes" })),
        ("a number for the flag", json!({ "includeInNetWorth": 1 })),
        ("a number for the virtual balance", json!({ "virtualBalance": 1000 })),
        ("a float for the virtual balance", json!({ "virtualBalance": 10.5 })),
        ("the answer's own field", json!({ "available": "1.00" })),
        ("a snake_case name", json!({ "include_in_net_worth": true })),
        ("the column's name", json!({ "group_name": "x" })),
        ("an unknown field", json!({ "color": "red" })),
    ] {
        assert!(serde_json::from_value::<AccountPatch>(body.clone()).is_err(), "patch: {what}");
        let mut input = json!({ "name": "A", "kind": "CASH" });
        input.as_object_mut().unwrap().extend(body.as_object().unwrap().clone());
        assert!(serde_json::from_value::<NewAccount>(input).is_err(), "new account: {what}");
    }
    // `null` clears the three that can be cleared, and is "unchanged" for the other two
    let patch: AccountPatch = serde_json::from_value(json!({ "bic": null, "notes": null, "group": null, "includeInNetWorth": null, "virtualBalance": null })).unwrap();
    assert_eq!((patch.bic, patch.notes, patch.group, patch.include_in_net_worth, patch.virtual_balance), (Some(None), Some(None), Some(None), None, None));
    let patch: AccountPatch = serde_json::from_value(json!({})).unwrap();
    assert_eq!((patch.bic, patch.notes, patch.group), (None, None, None), "left out is unchanged");
}

#[test]
fn null_clears_a_bic_a_notes_and_a_group_and_a_field_of_only_spaces_does_too() {
    let w = world();
    let id = create(
        &w.conn,
        &NewAccount { bic: Some("DEUTDEFF".into()), notes: Some("a\nb".into()), group: Some("Bank".into()), include_in_net_worth: Some(false), virtual_balance: Some("5.00".into()), ..new_account("A") },
        TODAY,
    )
    .unwrap()
    .id;
    for (field, field_name) in [("bic", "bic"), ("notes", "notes"), ("group", "group")] {
        let before = stamp_of(&w.conn, "accounts", id);
        let cleared = changed(&w.conn, id, field, Value::Null).unwrap();
        assert_eq!(read(&cleared, field_name), Value::Null, "{field}");
        assert!(stamp_of(&w.conn, "accounts", id) > before, "{field}: clearing a value is a change");
        // the others stayed
        assert_eq!((cleared.name.as_str(), cleared.include_in_net_worth, cleared.virtual_balance.as_str()), ("A", false, "5.00"));
        assert_nothing_written(&w.conn, "clearing what is clear", || changed(&w.conn, id, field, Value::Null));
    }
    let account = get(&w.conn, id, TODAY).unwrap();
    assert_eq!((account.bic, account.notes, account.group), (None, None, None));
    // only spaces (and for the notes only breaks and tabs) clear it as well
    for (field, blank) in [("bic", "   "), ("notes", " \n\r\n\t "), ("group", "  \t ")] {
        changed(&w.conn, id, field, json!("x")).unwrap();
        let before = stamp_of(&w.conn, "accounts", id);
        assert_eq!(read(&changed(&w.conn, id, field, json!(blank)).unwrap(), field), Value::Null, "{field}");
        assert!(stamp_of(&w.conn, "accounts", id) > before, "{field}");
        assert_nothing_written(&w.conn, "blank again", || changed(&w.conn, id, field, json!(blank)));
    }
    // a field that is not in the patch stays
    changed(&w.conn, id, "bic", json!("KEEP")).unwrap();
    assert_eq!(update(&w.conn, id, &AccountPatch { name: Some("B".into()), ..Default::default() }, TODAY).unwrap().bic.as_deref(), Some("KEEP"));
}

/// One account that holds `first`, and then every spelling of "the value it has" writes nothing and makes no stamp.
fn nothing_changes(field: &str, first: Value, same: &[Value]) {
    let w = world();
    let id = account(&w.conn, "A");
    let before_any = stamp_of(&w.conn, "accounts", id);
    changed(&w.conn, id, field, first.clone()).unwrap();
    assert!(stamp_of(&w.conn, "accounts", id) > before_any, "{field}: the first value is a change");
    for value in std::iter::once(&first).chain(same) {
        assert_nothing_written(&w.conn, &format!("{field} = {value}"), || changed(&w.conn, id, field, value.clone()));
    }
    // together with the other fields at their values, and with an empty patch
    assert_nothing_written(&w.conn, "an empty patch", || update(&w.conn, id, &AccountPatch::default(), TODAY));
    let current = get(&w.conn, id, TODAY).unwrap();
    assert_nothing_written(&w.conn, "all the fields as they are", || {
        update(
            &w.conn,
            id,
            &AccountPatch {
                name: Some(current.name.clone()),
                kind: Some(current.kind),
                bic: Some(current.bic.clone()),
                notes: Some(current.notes.clone()),
                group: Some(current.group.clone()),
                include_in_net_worth: Some(current.include_in_net_worth),
                virtual_balance: Some(current.virtual_balance.clone()),
                ..Default::default()
            },
            TODAY,
        )
    });
    // and a field that was never set, cleared or set to nothing
    if field != "includeInNetWorth" && field != "virtualBalance" {
        let fresh = account(&w.conn, "Never set");
        assert_nothing_written(&w.conn, "null on a field that is null", || changed(&w.conn, fresh, field, Value::Null));
        assert_nothing_written(&w.conn, "spaces on a field that is null", || changed(&w.conn, fresh, field, json!("   ")));
    }
}

#[test]
fn a_patch_that_sets_the_bic_to_what_it_has_writes_nothing() {
    nothing_changes("bic", json!("DEUTDEFF"), &[json!("  DEUTDEFF "), json!("DEUTDEFF\u{20}")]);
}

#[test]
fn a_patch_that_sets_the_notes_to_what_they_are_writes_nothing() {
    nothing_changes("notes", json!("one\ntwo"), &[json!("one\r\ntwo"), json!("one\rtwo"), json!("\n one\ntwo \n")]);
}

#[test]
fn a_patch_that_sets_the_group_to_what_it_is_writes_nothing() {
    nothing_changes("group", json!("Bank A"), &[json!(" Bank A "), json!("Bank A\t")]);
}

#[test]
fn a_patch_that_sets_the_flag_to_what_it_is_writes_nothing() {
    nothing_changes("includeInNetWorth", json!(false), &[json!(false)]);
    // the default is true: saying so is no change either
    let w = world();
    let id = account(&w.conn, "A");
    assert_nothing_written(&w.conn, "true on an account that counts", || changed(&w.conn, id, "includeInNetWorth", json!(true)));
}

#[test]
fn a_patch_that_sets_the_virtual_balance_to_what_it_is_writes_nothing() {
    nothing_changes("virtualBalance", json!("1000.00"), &[json!("1000.00")]);
    let w = world();
    let id = account(&w.conn, "A");
    assert_nothing_written(&w.conn, "0.00 on an account that has none", || changed(&w.conn, id, "virtualBalance", json!("0.00")));
}

#[test]
fn a_new_detail_that_changes_makes_a_stamp_and_a_good_one_with_a_bad_one_changes_nothing() {
    let w = world();
    let id = account(&w.conn, "A");
    for (field, value) in [("bic", json!("DEUTDEFF")), ("notes", json!("n")), ("group", json!("g")), ("includeInNetWorth", json!(false)), ("virtualBalance", json!("1.00"))] {
        let before = stamp_of(&w.conn, "accounts", id);
        changed(&w.conn, id, field, value).unwrap();
        assert!(stamp_of(&w.conn, "accounts", id) > before, "{field}");
    }
    let other = account(&w.conn, "Other");
    let bad = [
        ("a good BIC with a long group", json!({ "bic": "NEW", "group": "x".repeat(51) })),
        ("a good group with a bad virtual balance", json!({ "group": "New", "virtualBalance": "-0.00" })),
        ("a good flag with notes that hold a tab", json!({ "includeInNetWorth": true, "notes": "a\tb" })),
        ("a good virtual balance with a long BIC", json!({ "virtualBalance": "9.00", "bic": "x".repeat(21) })),
        ("good details with a bad opening balance", json!({ "bic": "NEW", "notes": "n", "openingBalance": "1.5" })),
        ("good details with a taken name", json!({ "bic": "NEW", "name": "Other" })),
    ];
    for (what, body) in bad {
        let patch: AccountPatch = serde_json::from_value(body).unwrap();
        let expected = if what.contains("taken name") { NAME_EXISTS } else { INVALID };
        assert_refused(&w.conn, expected, what, || update(&w.conn, id, &patch, TODAY));
    }
    // the same on create: a bad detail makes no account
    for (what, input) in [
        ("long BIC", NewAccount { bic: Some("x".repeat(21)), ..new_account("New") }),
        ("notes with a tab", NewAccount { notes: Some("a\tb".into()), ..new_account("New") }),
        ("long group", NewAccount { group: Some("x".repeat(51)), ..new_account("New") }),
        ("virtual balance beyond the range", NewAccount { virtual_balance: Some("10000000000.00".into()), ..new_account("New") }),
        ("good details and a taken name", NewAccount { bic: Some("NEW".into()), ..new_account("other") }),
    ] {
        let expected = if what.contains("taken name") { NAME_EXISTS } else { INVALID };
        assert_refused(&w.conn, expected, what, || create(&w.conn, &input, TODAY));
    }
    assert_eq!(get(&w.conn, other, TODAY).unwrap().bic, None);
}

#[test]
fn an_archived_account_takes_no_new_detail_until_it_is_restored() {
    let w = world();
    let id = create(&w.conn, &NewAccount { bic: Some("OLD".into()), notes: Some("old".into()), group: Some("Old".into()), virtual_balance: Some("5.00".into()), ..new_account("Old") }, TODAY).unwrap().id;
    archive(&w.conn, id, TODAY).unwrap();
    let held = get(&w.conn, id, TODAY).unwrap();
    for (field, value) in [
        ("bic", json!("NEW")),
        ("bic", json!(null)),
        ("bic", json!("OLD")),
        ("notes", json!("new")),
        ("notes", json!(null)),
        ("group", json!("New")),
        ("group", json!(null)),
        ("includeInNetWorth", json!(false)),
        ("includeInNetWorth", json!(true)),
        ("virtualBalance", json!("9.00")),
        ("virtualBalance", json!("5.00")),
    ] {
        assert_refused(&w.conn, INVALID, &format!("{field} = {value} on an archived account"), || changed(&w.conn, id, field, value.clone()));
    }
    assert_eq!(get(&w.conn, id, TODAY).unwrap(), held, "nothing changed");
    restore(&w.conn, id, TODAY).unwrap();
    assert_eq!(read(&changed(&w.conn, id, "bic", json!("NEW")).unwrap(), "bic"), json!("NEW"));
    assert!(!changed(&w.conn, id, "includeInNetWorth", json!(false)).unwrap().include_in_net_worth);
    // the details stay while it is archived and come back with it
    archive(&w.conn, id, TODAY).unwrap();
    let archived = get(&w.conn, id, TODAY).unwrap();
    assert_eq!((archived.bic.as_deref(), archived.notes.as_deref(), archived.group.as_deref(), archived.include_in_net_worth, archived.virtual_balance.as_str()), (Some("NEW"), Some("old"), Some("Old"), false, "5.00"));
}

#[test]
fn the_main_account_takes_all_five_fields_like_any_other_and_stays_the_main_account() {
    let w = world();
    let made = update(
        &w.conn,
        MAIN_ID,
        &AccountPatch {
            bic: Some(Some("DEUTDEFF".into())),
            notes: Some(Some("the one that always stays\nsecond line".into())),
            group: Some(Some("Bank A".into())),
            include_in_net_worth: Some(false),
            virtual_balance: Some("-50.00".into()),
            ..Default::default()
        },
        TODAY,
    )
    .unwrap();
    assert_eq!(
        (made.bic.as_deref(), made.notes.as_deref(), made.group.as_deref(), made.include_in_net_worth, made.virtual_balance.as_str(), made.available.as_str(), made.is_main, made.archived),
        (Some("DEUTDEFF"), Some("the one that always stays\nsecond line"), Some("Bank A"), false, "-50.00", "-50.00", true, false)
    );
    assert!(stamp_of(&w.conn, "accounts", MAIN_ID).as_str() > crate::sync::engine::UNSET_STAMP, "the first real edit gives it a real stamp");
    assert_eq!(text(&w.conn, "SELECT sync_id FROM accounts WHERE id = 1"), super::MAIN_SYNC_ID);
    assert!(super::check(&w.conn).unwrap());
    // and each of the five can be cleared again
    let cleared = update(&w.conn, MAIN_ID, &serde_json::from_value(json!({ "bic": null, "notes": null, "group": null, "includeInNetWorth": true, "virtualBalance": "0.00" })).unwrap(), TODAY).unwrap();
    assert_eq!((cleared.bic, cleared.notes, cleared.group, cleared.include_in_net_worth, cleared.virtual_balance.as_str()), (None, None, None, true, "0.00"));
    assert_refused(&w.conn, INVALID, "it still cannot be archived", || archive(&w.conn, MAIN_ID, TODAY));
}

// ---- ACTIVE: reorder -------------------------------------------------------------------------

#[test]
fn a_reorder_sets_the_order_and_writes_only_the_rows_that_move() {
    let w = world();
    let (a, b, c) = (account(&w.conn, "A"), account(&w.conn, "B"), account(&w.conn, "C"));
    let old = archived_account(&w.conn, "Old");
    let stamps = |ids: &[i64]| -> Vec<String> { ids.iter().map(|id| stamp_of(&w.conn, "accounts", *id)).collect() };
    let before = stamps(&[MAIN_ID, a, b, c, old]);

    reorder(&w.conn, &[MAIN_ID, a, c, b]).unwrap();
    assert_eq!(names(&w.conn), [MAIN_NAME, "A", "C", "B", "Old"], "the archived one stays after the others");
    let after = stamps(&[MAIN_ID, a, b, c, old]);
    assert_eq!((&after[0], &after[1], &after[4]), (&before[0], &before[1], &before[4]), "the main account, A and the archived one did not move");
    assert!(after[2] > before[2] && after[3] > before[3], "B and C did");

    assert_nothing_written(&w.conn, "the order it has", || reorder(&w.conn, &[MAIN_ID, a, c, b]));
    // the main account can be anywhere in the list
    reorder(&w.conn, &[c, b, MAIN_ID, a]).unwrap();
    assert_eq!(names(&w.conn), ["C", "B", MAIN_NAME, "A", "Old"]);
    // a new account still goes to the end
    account(&w.conn, "D");
    assert_eq!(names(&w.conn), ["C", "B", MAIN_NAME, "A", "D", "Old"]);
}

/// 5.5 item 20.
#[test]
fn a_reorder_that_is_not_exactly_the_active_accounts_each_once_changes_nothing() {
    let w = world();
    let (a, b) = (account(&w.conn, "A"), account(&w.conn, "B"));
    let old = archived_account(&w.conn, "Old");
    let food = w.food; // the id of a category: an id of another list
    assert!(food != a && food != b);
    let bad: [(&str, Vec<i64>); 9] = [
        ("a missing id", vec![MAIN_ID, a]),
        ("without the main account", vec![a, b]),
        ("a repeated id", vec![MAIN_ID, a, b, a]),
        ("a repeated id in place of another", vec![MAIN_ID, a, a]),
        ("an archived id", vec![MAIN_ID, a, b, old]),
        ("an archived id in place of an active one", vec![MAIN_ID, a, old]),
        ("an id that is no account", vec![MAIN_ID, a, b, 999]),
        ("an id that is no account in place of one", vec![MAIN_ID, a, 999]),
        ("nothing", vec![]),
    ];
    for (what, ids) in bad {
        assert_refused(&w.conn, INVALID, what, || reorder(&w.conn, &ids));
    }
    assert_eq!(names(&w.conn), [MAIN_NAME, "A", "B", "Old"]);
}

// ---- archive and restore ---------------------------------------------------------------------

/// 5.5 items 1 and 2.
#[test]
fn archiving_twice_and_restoring_what_was_never_archived_answer_the_account_and_write_nothing() {
    let w = world();
    let id = account(&w.conn, "Savings");
    let untouched = assert_nothing_written(&w.conn, "restore an active account", || restore(&w.conn, id, TODAY));
    assert!(!untouched.archived);
    assert_nothing_written(&w.conn, "restore the main account", || restore(&w.conn, MAIN_ID, TODAY));

    let made = stamp_of(&w.conn, "accounts", id);
    let archived = archive(&w.conn, id, TODAY).unwrap();
    assert!(archived.archived);
    let stamp = stamp_of(&w.conn, "accounts", id);
    assert!(stamp > made, "archiving is a change of the account");
    let again = assert_nothing_written(&w.conn, "archive again", || archive(&w.conn, id, TODAY));
    assert_eq!(again, archived);

    let back = restore(&w.conn, id, TODAY).unwrap();
    assert!(!back.archived);
    assert!(stamp_of(&w.conn, "accounts", id) > stamp);
    assert_nothing_written(&w.conn, "restore again", || restore(&w.conn, id, TODAY));
    assert_eq!(names(&w.conn), [MAIN_NAME, "Savings"]);
}

#[test]
fn archiving_changes_no_number_and_writes_none_of_the_rows_of_the_account() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let cash = create(&w.conn, &NewAccount { opening_balance: Some("100.00".into()), opening_date: Some("2026-01-01".into()), ..new_account("Cash") }, TODAY).unwrap().id;
    let shop = transactions::create(&w.conn, &crate::transactions::NewTransaction { account_id: Some(cash.to_string()), ..tx(w.food, "Shop", "12.50", "2026-11-02") }).unwrap().id;
    let rent = recurring_in(&w.conn, w.rent, cash);
    let loan = entry_in(&w.conn, anna, cash);
    let moved = transfer(&w.conn, cash, MAIN_ID, "20.00", "2026-11-03");
    let rows = [("transactions", shop), ("recurring_transactions", rent), ("ledger_entries", loan), ("transfers", moved)];
    let stamps = || rows.map(|(table, id)| (stamp_of(&w.conn, table, id), text(&w.conn, &format!("SELECT updated_at FROM {table} WHERE id = {id}"))));
    let before = (stamps(), get(&w.conn, cash, TODAY).unwrap(), super::summary(&w.conn, TODAY).unwrap());
    assert_eq!((before.1.balance.as_str(), before.1.entries, before.1.last_activity.as_deref()), ("62.50", 4, Some("2026-11-03")));

    let archived = archive(&w.conn, cash, TODAY).unwrap();
    assert_eq!(stamps(), before.0, "no row of the account was written");
    assert_eq!(Account { archived: false, ..archived.clone() }, before.1, "its numbers are the same");
    assert_eq!(super::summary(&w.conn, TODAY).unwrap(), before.2, "and net worth still counts it");
    assert_eq!(names(&w.conn), [MAIN_NAME, "Cash"]);
    assert_eq!(list(&w.conn, TODAY).unwrap()[1], archived);

    restore(&w.conn, cash, TODAY).unwrap();
    assert_eq!(stamps(), before.0, "restoring writes none of them either");
    assert_eq!(get(&w.conn, cash, TODAY).unwrap(), before.1);
}

fn tx(category: i64, name: &str, amount: &str, date: &str) -> crate::transactions::NewTransaction {
    crate::transactions::NewTransaction { name: name.into(), amount: amount.into(), payment_date: date.into(), category_id: category.to_string(), description: None, client_uuid: None, account_id: None }
}

/// 5.5 items 3 (the part on this device) and 4.
#[test]
fn the_main_account_is_never_archived_or_deleted_so_one_active_account_always_stays() {
    let w = world();
    assert_refused(&w.conn, INVALID, "archive the main account", || archive(&w.conn, MAIN_ID, TODAY));
    assert_refused(&w.conn, INVALID, "delete the main account", || delete(&w.conn, MAIN_ID));
    let other = account(&w.conn, "Savings");
    // with a second account it is still the main one that stays
    assert_refused(&w.conn, INVALID, "archive the main account when there is another", || archive(&w.conn, MAIN_ID, TODAY));
    assert_refused(&w.conn, INVALID, "delete the main account when there is another", || delete(&w.conn, MAIN_ID));
    // the last active account besides the main one goes, then the main one is asked: refused
    archive(&w.conn, other, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "archive the last active account", || archive(&w.conn, MAIN_ID, TODAY));
    assert_refused(&w.conn, INVALID, "delete the last active account", || delete(&w.conn, MAIN_ID));
    assert_eq!(one(&w.conn, "SELECT count(*) FROM accounts WHERE archived = 0"), 1);
    assert!(super::check(&w.conn).unwrap());
    // renamed, it is still the main account
    update(&w.conn, MAIN_ID, &rename("Everyday"), TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "archive the renamed main account", || archive(&w.conn, MAIN_ID, TODAY));
    assert_refused(&w.conn, INVALID, "delete the renamed main account", || delete(&w.conn, MAIN_ID));
    assert!(super::check(&w.conn).unwrap());
}

#[test]
fn an_archived_account_is_not_updated_or_reordered_until_it_is_restored() {
    let w = world();
    let (a, old) = (account(&w.conn, "A"), archived_account(&w.conn, "Old"));
    assert_refused(&w.conn, INVALID, "rename an archived account", || update(&w.conn, old, &rename("New"), TODAY));
    assert_refused(&w.conn, INVALID, "an empty patch on an archived account", || update(&w.conn, old, &AccountPatch::default(), TODAY));
    assert_refused(&w.conn, INVALID, "change its opening balance", || update(&w.conn, old, &AccountPatch { opening_balance: Some("5.00".into()), ..Default::default() }, TODAY));
    assert_refused(&w.conn, INVALID, "reorder with it", || reorder(&w.conn, &[old, MAIN_ID, a]));
    restore(&w.conn, old, TODAY).unwrap();
    assert_eq!(update(&w.conn, old, &rename("New"), TODAY).unwrap().name, "New");
    reorder(&w.conn, &[old, MAIN_ID, a]).unwrap();
    assert_eq!(names(&w.conn), ["New", MAIN_NAME, "A"]);
}

// ---- delete ----------------------------------------------------------------------------------

#[test]
fn an_empty_account_is_deleted_active_or_archived_and_leaves_a_marker() {
    let w = world();
    for archived in [false, true] {
        let id = account(&w.conn, "Savings");
        if archived {
            archive(&w.conn, id, TODAY).unwrap();
        }
        let (sync_id, stamp) = (text(&w.conn, &format!("SELECT sync_id FROM accounts WHERE id = {id}")), stamp_of(&w.conn, "accounts", id));
        delete(&w.conn, id).unwrap();
        assert_eq!(get(&w.conn, id, TODAY), Err(NOT_FOUND));
        let marker = text(&w.conn, &format!("SELECT stamp FROM sync_deleted WHERE sync_id = '{sync_id}' AND kind = 'accounts'"));
        assert!(marker > stamp, "archived: {archived}");
    }
    assert_eq!(names(&w.conn), [MAIN_NAME]);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM sync_deleted"), 2);
    // the name is free again
    account(&w.conn, "Savings");
}

/// 5.5 item 5.
#[test]
fn an_account_with_one_row_of_any_kind_is_not_deleted_until_the_row_is_moved_away_or_gone() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let other = account(&w.conn, "Other");

    // one transaction
    let a = account(&w.conn, "With a transaction");
    let shop = transactions::create(&w.conn, &crate::transactions::NewTransaction { account_id: Some(a.to_string()), ..tx(w.food, "Shop", "12.50", "2026-11-02") }).unwrap().id;
    assert_eq!(get(&w.conn, a, TODAY).unwrap().entries, 1);
    assert_refused(&w.conn, INVALID, "delete with one transaction", || delete(&w.conn, a));
    transactions::update(&w.conn, shop, &TransactionPatch { account_id: Some(other.to_string()), ..Default::default() }).unwrap();
    delete(&w.conn, a).unwrap();

    // only a recurring item
    let b = account(&w.conn, "With a recurring item");
    let rent = recurring_in(&w.conn, w.rent, b);
    assert_refused(&w.conn, INVALID, "delete with only a recurring item", || delete(&w.conn, b));
    recurring::update(&w.conn, rent, &recurring::RecurringPatch { account_id: Some(MAIN_ID.to_string()), ..Default::default() }).unwrap();
    delete(&w.conn, b).unwrap();

    // only a ledger entry
    let c = account(&w.conn, "With a ledger entry");
    let loan = entry_in(&w.conn, anna, c);
    assert_refused(&w.conn, INVALID, "delete with only a ledger entry", || delete(&w.conn, c));
    ledger::delete(&w.conn, loan).unwrap();
    delete(&w.conn, c).unwrap();

    // only a transfer, on its `to` side, and then on its `from` side
    let d = account(&w.conn, "With a transfer in");
    let moved = transfer(&w.conn, other, d, "5.00", "2026-11-03");
    assert_eq!(get(&w.conn, d, TODAY).unwrap().entries, 1);
    assert_refused(&w.conn, INVALID, "delete with only a transfer to it", || delete(&w.conn, d));
    transfers::update(&w.conn, moved, &transfers::TransferPatch { from_account_id: Some(d.to_string()), to_account_id: Some(other.to_string()), ..Default::default() }).unwrap();
    assert_refused(&w.conn, INVALID, "delete with only a transfer from it", || delete(&w.conn, d));
    // archived, with rows: still refused
    archive(&w.conn, d, TODAY).unwrap();
    assert_refused(&w.conn, INVALID, "delete an archived account with rows", || delete(&w.conn, d));
    transfers::update(&w.conn, moved, &transfers::TransferPatch { from_account_id: Some(MAIN_ID.to_string()), ..Default::default() }).unwrap();
    assert_eq!(get(&w.conn, d, TODAY).unwrap().entries, 0);
    delete(&w.conn, d).unwrap();

    assert_eq!(names(&w.conn), [MAIN_NAME, "Other"]);
    // the account the rows went to holds them, and is not deleted either
    assert_eq!(get(&w.conn, other, TODAY).unwrap().entries, 2);
    assert_refused(&w.conn, INVALID, "delete the account they moved to", || delete(&w.conn, other));
}

// ---- NONE, DELETED: any event by id -----------------------------------------------------------

#[test]
fn an_account_that_is_not_there_or_was_deleted_is_not_found_by_any_event() {
    let w = world();
    let gone = account(&w.conn, "Gone");
    account(&w.conn, "Stays");
    delete(&w.conn, gone).unwrap();
    for id in [gone, 999] {
        assert_refused(&w.conn, NOT_FOUND, "get", || get(&w.conn, id, TODAY));
        assert_refused(&w.conn, NOT_FOUND, "update", || update(&w.conn, id, &rename("X"), TODAY));
        assert_refused(&w.conn, NOT_FOUND, "archive", || archive(&w.conn, id, TODAY));
        assert_refused(&w.conn, NOT_FOUND, "restore", || restore(&w.conn, id, TODAY));
        assert_refused(&w.conn, NOT_FOUND, "delete", || delete(&w.conn, id));
        assert_refused(&w.conn, NOT_FOUND, "statement", || super::statement(&w.conn, id, "2026-11-01", "2026-11-30", TODAY));
        assert_refused(&w.conn, NOT_FOUND, "history", || super::history(&w.conn, id, "2026", TODAY));
        assert_refused(&w.conn, INVALID, "reorder", || reorder(&w.conn, &[MAIN_ID, 3, id]));
    }
    assert_eq!(names(&w.conn), [MAIN_NAME, "Stays"]);
}

#[test]
fn the_check_of_the_main_account_fails_when_it_is_missing_or_is_another_row() {
    let w = world();
    assert!(super::check(&w.conn).unwrap());
    account(&w.conn, "Savings");
    assert!(super::check(&w.conn).unwrap());
    // what a damaged backup or a hostile sync could leave behind
    applying(&w.conn, true);
    w.conn.execute("UPDATE accounts SET sync_id = ?1 WHERE id = 1", ["a".repeat(32)]).unwrap();
    assert!(!super::check(&w.conn).unwrap(), "id 1 with another sync id is not the main account");
    w.conn.execute("DELETE FROM accounts WHERE id = 1", []).unwrap();
    assert!(!super::check(&w.conn).unwrap(), "no row with id 1");
    w.conn.execute("UPDATE accounts SET sync_id = ?1 WHERE id = 2", [super::MAIN_SYNC_ID]).unwrap();
    assert!(!super::check(&w.conn).unwrap(), "the main sync id on another id");
}
