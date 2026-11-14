//! Accounts and transfers in the exports: all of them with no limit, only the names of the accounts the rows are in with a limit, and in
//! every file the account of each transaction, recurring item and ledger entry (`docs/accounts-design.md`, section 10).

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::filters_tests::{self, expected, limited, random_selections, wants, World};
use super::tests::{lived_in, read_csv, spend};
use super::*;
use crate::accounts::fixtures::{account, new_account, new_transfer, snapshot};
use crate::accounts::transfers::NewTransfer;
use crate::accounts::{self, Kind, NewAccount, MAIN_ID};
use crate::debts::fixtures::friend;
use crate::debts::ledger::{self, Kind as EntryKind, NewEntry};
use crate::recurring::RecurringPatch;
use crate::transactions::TransactionPatch;

/// The day of the exports below, and the same as a moment.
pub(super) const TODAY: &str = "2026-10-17";

pub(super) fn now() -> DateTime<Utc> {
    "2026-10-17T00:00:00Z".parse().unwrap()
}

pub(super) fn file(conn: &Connection, selection: &Selection) -> Value {
    serde_json::from_str(&json(conn, selection, now()).unwrap()).unwrap()
}

pub(super) struct Case {
    pub(super) conn: Connection,
    pub(super) savings: i64,
    pub(super) cash: i64,
    pub(super) card: i64,
    pub(super) old: i64,
    pub(super) salary: i64,
    pub(super) rent: i64,
    pub(super) secret: i64,
}

fn opened(name: &str, kind: Kind, number: Option<&str>, balance: &str) -> NewAccount {
    NewAccount { kind, number: number.map(str::to_string), opening_balance: Some(balance.into()), opening_date: Some("2025-12-01".into()), ..new_account(name) }
}

/// `lived_in()` (Pay 3200.00 on 2026-01-25 in Salary; Rent 900.00 on 02-01 and 905.50 on 03-01; Session 80.00 on 02-10 in Therapy; recurring
/// Rent and Therapy plan) spread over five accounts: Savings (a number and 1500.00), Cash (250.00), Credit card (-240.00), and Old savings (100.00,
/// archived). The session and a loan of 20.00 to Anna are in cash, the rent of March and the Therapy plan are on the card, and three transfers
/// move money around. Today is 2026-10-17, and what they hold: Main account 1800.00, Savings 1700.00, Cash 150.00, Credit card -945.50 and
/// Old savings 200.00.
pub(super) fn case() -> Case {
    let (conn, salary, rent, secret) = lived_in();
    let make = |new: NewAccount| accounts::create(&conn, &new, TODAY).unwrap().id;
    let savings = make(opened("Savings", Kind::Savings, Some("IE29 AIBK 9311 5212 3456 78"), "1500.00"));
    let cash = make(opened("Cash", Kind::Cash, None, "250.00"));
    let card = make(opened("Credit card", Kind::CreditCard, Some("4242"), "-240.00"));
    let old = make(opened("Old savings", Kind::Savings, None, "100.00"));
    let id_of = |name: &str, date: &str| -> i64 { conn.query_row("SELECT id FROM transactions WHERE name = ?1 AND payment_date = ?2", [name, date], |r| r.get(0)).unwrap() };
    let to = |account: i64| TransactionPatch { account_id: Some(account.to_string()), ..Default::default() };
    crate::transactions::update(&conn, id_of("Session", "2026-02-10"), &to(cash)).unwrap();
    crate::transactions::update(&conn, id_of("Rent", "2026-03-01"), &to(card)).unwrap();
    let plan: i64 = conn.query_row("SELECT id FROM recurring_transactions WHERE name = 'Therapy plan'", [], |r| r.get(0)).unwrap();
    crate::recurring::update(&conn, plan, &RecurringPatch { account_id: Some(card.to_string()), ..Default::default() }).unwrap();
    let anna = friend(&conn, "Anna");
    let loan = NewEntry { person_id: anna.to_string(), kind: EntryKind::Lent, amount: "20.00".into(), date: "2026-02-15".into(), note: Some("taxi".into()), transaction_id: None, account_id: Some(cash.to_string()) };
    ledger::add(&conn, &loan).unwrap();
    let moves = |from: i64, to: i64, amount: &str, date: &str, note: Option<&str>| {
        accounts::transfers::create(&conn, &NewTransfer { note: note.map(str::to_string), ..new_transfer(from, to, amount, date) }).unwrap();
    };
    moves(MAIN_ID, savings, "300.00", "2026-02-02", Some("monthly saving"));
    moves(savings, old, "100.00", "2026-02-03", None);
    moves(MAIN_ID, card, "200.00", "2026-02-04", Some("pay the card"));
    accounts::archive(&conn, old, TODAY).unwrap();
    Case { conn, savings, cash, card, old, salary, rent, secret }
}

fn named(list: &Value) -> Vec<String> {
    list.as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap().to_string()).collect()
}

/// The rows of a part of the file with this name (and date, for the transactions).
fn row<'a>(file: &'a Value, part: &str, name: &str, date: &str) -> &'a Value {
    file[part].as_array().unwrap().iter().find(|r| r["name"] == name && (date.is_empty() || r["date"] == date)).unwrap_or_else(|| panic!("no {name} in {part}"))
}

#[test]
fn the_json_has_every_account_with_what_it_holds_and_every_transfer_and_the_account_of_every_row() {
    let c = case();
    let file = file(&c.conn, &Selection::default());
    assert_eq!(named(&file["accounts"]), ["Main account", "Savings", "Cash", "Credit card", "Old savings"], "in the person's order, the archived one last");
    let balances: Vec<&str> = file["accounts"].as_array().unwrap().iter().map(|a| a["balance"].as_str().unwrap()).collect();
    assert_eq!(balances, ["1800.00", "1700.00", "150.00", "-945.50", "200.00"], "what each holds on the day of the export");
    // every field of an account but its row count
    assert_eq!(
        file["accounts"][2],
        json!({ "id": c.cash.to_string(), "name": "Cash", "kind": "CASH", "number": null, "openingBalance": "250.00", "openingDate": "2025-12-01", "archived": false,
                "isMain": false, "balance": "150.00", "changeThisMonth": "0.00", "lastActivity": "2026-02-15" })
    );
    assert_eq!(file["accounts"][3]["openingBalance"], "-240.00");
    assert_eq!((file["accounts"][1]["number"].as_str(), file["accounts"][4]["archived"].as_bool(), file["accounts"][0]["isMain"].as_bool()), (Some("IE29 AIBK 9311 5212 3456 78"), Some(true), Some(true)));
    assert!(file["accounts"].as_array().unwrap().iter().all(|a| a.get("entries").is_none()));

    let transfers = file["transfers"].as_array().unwrap();
    assert_eq!(transfers.len(), 3);
    assert_eq!(
        transfers[0],
        json!({ "id": "1", "date": "2026-02-02", "fromAccountId": "1", "from": "Main account", "toAccountId": c.savings.to_string(), "to": "Savings", "amount": "300.00", "note": "monthly saving" })
    );
    assert_eq!((transfers[1]["from"].as_str(), transfers[1]["to"].as_str(), transfers[1]["note"].is_null()), (Some("Savings"), Some("Old savings"), true), "an archived account is named too");
    assert_eq!(transfers[2]["amount"], "200.00");

    let on = |part: &str, name: &str, date: &str| -> (String, String) {
        let r = row(&file, part, name, date);
        (r["accountId"].as_str().unwrap().to_string(), r["account"].as_str().unwrap().to_string())
    };
    assert_eq!(on("transactions", "Pay", "2026-01-25"), ("1".into(), "Main account".into()));
    assert_eq!(on("transactions", "Session", "2026-02-10"), (c.cash.to_string(), "Cash".into()));
    assert_eq!(on("transactions", "Rent", "2026-02-01"), ("1".into(), "Main account".into()));
    assert_eq!(on("transactions", "Rent", "2026-03-01"), (c.card.to_string(), "Credit card".into()));
    assert_eq!(on("recurring", "Therapy plan", ""), (c.card.to_string(), "Credit card".into()));
    assert_eq!(on("recurring", "Rent", ""), ("1".into(), "Main account".into()));
    let loan = &file["ledger"][0];
    assert_eq!((loan["accountId"].as_str(), loan["account"].as_str()), (Some(c.cash.to_string().as_str()), Some("Cash")));
    // a transfer is neither income nor expense: no transaction, and no month of the summary, holds one
    assert_eq!(file["transactions"].as_array().unwrap().len(), 4);
    assert!(file["transactions"].as_array().unwrap().iter().all(|t| t["name"] != "monthly saving"));
}

#[test]
fn the_csv_has_the_account_as_its_last_column_and_the_others_keep_their_place() {
    let c = case();
    let text = csv(&c.conn, &Selection::default()).unwrap();
    assert!(text.starts_with("\u{feff}date,type,category,name,amount,currency,description,documents,my_share,account\r\n"), "{}", &text[..90]);
    let rows = read_csv(&text);
    assert!(rows.iter().all(|r| r.len() == 10));
    let account_of = |name: &str, date: &str| rows[1..].iter().find(|r| r[3] == name && r[0] == date).unwrap()[9].clone();
    assert_eq!(account_of("Pay", "2026-01-25"), "Main account");
    assert_eq!(account_of("Session", "2026-02-10"), "Cash");
    assert_eq!(account_of("Rent", "2026-02-01"), "Main account");
    assert_eq!(account_of("Rent", "2026-03-01"), "Credit card");
    // the columns of before: the amount and what it costs you are where they were
    let session = rows[1..].iter().find(|r| r[3] == "Session").unwrap();
    assert_eq!((session[4].as_str(), session[8].as_str()), ("80.00", "80.00"));
    // a limit changes which rows there are and nothing else
    let limited = csv(&c.conn, &Selection { category_ids: Some(vec![c.secret.to_string()]), ..Default::default() }).unwrap();
    assert_eq!(read_csv(&limited).iter().map(|r| r[9].as_str()).collect::<Vec<_>>(), ["account", "Cash"]);
}

#[test]
fn the_preview_counts_the_accounts_and_the_transfers() {
    let c = case();
    let all = preview(&c.conn, &Selection::default()).unwrap();
    assert_eq!((all.accounts, all.transfers, all.transactions, all.recurring), (5, 3, 4, 2));
    let limit = preview(&c.conn, &Selection { category_ids: Some(vec![c.secret.to_string()]), ..Default::default() }).unwrap();
    assert_eq!((limit.accounts, limit.transfers), (2, 0), "the card (the plan) and cash (the session): only the accounts of what is in it, and no transfer");
    let parts = |sections: &[Section]| Selection { sections: Some(sections.to_vec()), ..Default::default() };
    let none = preview(&c.conn, &parts(&[Section::Categories])).unwrap();
    assert_eq!((none.accounts, none.transfers), (0, 0), "a part that is left out counts for nothing");
    let only = preview(&c.conn, &parts(&[Section::Transfers])).unwrap();
    assert_eq!((only.accounts, only.transfers), (0, 3));
    // and the screen's words for the parts are the ones that are read
    let sent: Selection = serde_json::from_value(json!({ "sections": ["accounts", "transfers"] })).unwrap();
    assert_eq!(sent.sections, Some(vec![Section::Accounts, Section::Transfers]));
    assert_eq!((Section::Accounts.as_str(), Section::Transfers.as_str()), ("accounts", "transfers"));
}

#[test]
fn with_only_the_main_account_every_file_holds_it_and_nothing_else() {
    let (conn, ..) = lived_in();
    let file = file(&conn, &Selection::default());
    assert_eq!(named(&file["accounts"]), ["Main account"]);
    let main = &file["accounts"][0];
    assert_eq!((main["id"].as_str(), main["isMain"].as_bool(), main["balance"].as_str(), main["openingBalance"].as_str(), main["archived"].as_bool()), (Some("1"), Some(true), Some("1314.50"), Some("0.00"), Some(false)));
    assert_eq!(file["transfers"], json!([]), "no transfers, and still the part");
    for part in ["transactions", "recurring"] {
        assert!(file[part].as_array().unwrap().iter().all(|r| r["accountId"] == "1" && r["account"] == "Main account"), "{part}");
    }
    let rows = read_csv(&csv(&conn, &Selection::default()).unwrap());
    assert!(rows[1..].iter().all(|r| r[9] == "Main account"));
    let p = preview(&conn, &Selection::default()).unwrap();
    assert_eq!((p.accounts, p.transfers), (1, 0));
    // renamed, it is as typed, and its opening balance and number are in
    accounts::update(
        &conn,
        MAIN_ID,
        &accounts::AccountPatch { name: Some("Current".into()), number: Some(Some("IE00".into())), opening_balance: Some("-10.00".into()), ..Default::default() },
        TODAY,
    )
    .unwrap();
    let renamed = file_of(&conn);
    assert_eq!((renamed["accounts"][0]["name"].as_str(), renamed["accounts"][0]["number"].as_str(), renamed["accounts"][0]["balance"].as_str()), (Some("Current"), Some("IE00"), Some("1304.50")));
    assert_eq!(renamed["transactions"][0]["account"], "Current");
}

fn file_of(conn: &Connection) -> Value {
    file(conn, &Selection::default())
}

#[test]
fn a_limited_export_names_the_accounts_of_its_rows_and_nothing_more_about_them() {
    let c = case();
    // the Therapy category: the session (cash) and the plan (card)
    let selection = Selection { category_ids: Some(vec![c.secret.to_string()]), ..Default::default() };
    let limited = file(&c.conn, &selection);
    assert_eq!(limited["accounts"], json!([
        { "id": c.cash.to_string(), "name": "Cash", "kind": "CASH" },
        { "id": c.card.to_string(), "name": "Credit card", "kind": "CREDIT_CARD" },
    ]));
    assert!(limited.get("transfers").is_none(), "a transfer is between two accounts, and the file does not speak of the rest");
    let text = serde_json::to_string(&limited).unwrap();
    for hidden in ["Savings", "Old savings", "Main account", "IE29", "4242", "250.00", "-240.00", "1700.00", "150.00", "monthly saving", "pay the card", "openingBalance", "isMain"] {
        assert!(!text.contains(hidden), "{hidden:?} leaked into a limited export");
    }
    assert_eq!(row(&limited, "transactions", "Session", "2026-02-10")["account"], "Cash", "a row still says which account it is in");

    // Rent: the two payments (the main account, the card) and the item (the main account)
    let rent = file(&c.conn, &Selection { category_ids: Some(vec![c.rent.to_string()]), ..Default::default() });
    assert_eq!(named(&rent["accounts"]), ["Main account", "Credit card"]);
    // Salary has the pay only; an account that has no row of the file is not in it, even if transfers touch it
    let pay = file(&c.conn, &Selection { category_ids: Some(vec![c.salary.to_string()]), ..Default::default() });
    assert_eq!(named(&pay["accounts"]), ["Main account"]);
    // the ledger is in only when it was asked for, and then its entry names cash
    let with_loans = file(&c.conn, &Selection { category_ids: Some(vec![c.salary.to_string()]), include_debts: true, ..Default::default() });
    assert_eq!(named(&with_loans["accounts"]), ["Main account", "Cash"]);
    // dates limit the transactions; the recurring items that were running are in whatever the dates
    let feb = file(&c.conn, &Selection { from: Some("2026-02-10".into()), to: Some("2026-02-12".into()), ..Default::default() });
    assert_eq!(named(&feb["accounts"]), ["Main account", "Cash", "Credit card"], "the session, and the two recurring items");
    // with no category at all there are no rows, so no account; the loan of the ledger names cash when the ledger was asked for
    let nothing = file(&c.conn, &Selection { category_ids: Some(vec![]), ..Default::default() });
    assert_eq!(nothing["accounts"], json!([]));
    let loans = file(&c.conn, &Selection { category_ids: Some(vec![]), include_debts: true, ..Default::default() });
    assert_eq!(loans["ledger"].as_array().unwrap().len(), 1);
    assert_eq!(named(&loans["accounts"]), ["Cash"]);
}

#[test]
fn an_archived_account_is_listed_whole_with_no_limit_and_named_only_by_its_rows_with_one() {
    let c = case();
    // a row in the archived account (the app does not move one in, but a row stays in an account that is archived afterwards)
    c.conn.execute("UPDATE transactions SET account_id = ?1 WHERE name = 'Pay'", [c.old]).unwrap();
    let whole = file(&c.conn, &Selection::default());
    assert_eq!(whole["accounts"][4]["archived"], true);
    assert_eq!(whole["accounts"][4]["balance"], "3400.00", "200.00 and the pay");
    let limited = file(&c.conn, &Selection { category_ids: Some(vec![c.salary.to_string()]), ..Default::default() });
    assert_eq!(limited["accounts"], json!([{ "id": c.old.to_string(), "name": "Old savings", "kind": "SAVINGS" }]));
}

#[test]
fn the_parts_asked_for_decide_what_is_in_the_file() {
    let c = case();
    let only = |sections: &[Section]| Selection { sections: Some(sections.to_vec()), ..Default::default() };
    let keys = |value: &Value| -> BTreeSet<String> { value.as_object().unwrap().keys().cloned().collect() };
    let base: BTreeSet<String> = ["format", "version", "exportedAt", "scope", "settings"].map(String::from).into();
    let with = |extra: &[&str]| -> BTreeSet<String> { base.iter().cloned().chain(extra.iter().map(|k| k.to_string())).collect() };
    assert_eq!(keys(&file(&c.conn, &only(&[Section::Accounts]))), with(&["accounts"]));
    assert_eq!(keys(&file(&c.conn, &only(&[Section::Transfers]))), with(&["transfers"]));
    assert_eq!(file(&c.conn, &only(&[Section::Transfers]))["scope"]["sections"], json!(["transfers"]));
    // a limit leaves the transfers out, asked for or not, and an account is named only by the rows of the parts that are in the file
    let dated = |sections: &[Section]| Selection { from: Some("2026-01-01".into()), sections: Some(sections.to_vec()), ..Default::default() };
    let transfers = file(&c.conn, &dated(&[Section::Transfers, Section::Accounts]));
    assert_eq!(keys(&transfers), with(&["accounts"]));
    assert_eq!(transfers["accounts"], json!([]), "no part with rows in it, so no account is named");
    let with_rows = file(&c.conn, &dated(&[Section::Accounts, Section::Transactions]));
    assert_eq!(named(&with_rows["accounts"]), ["Main account", "Cash", "Credit card"]);
    let recurring_only = file(&c.conn, &dated(&[Section::Accounts, Section::Recurring]));
    assert_eq!(named(&recurring_only["accounts"]), ["Main account", "Credit card"], "the accounts of the two recurring items");
}

/// Names of accounts that a spreadsheet could take for something else. The last two have a control character, which the app never lets in a
/// name; a file or an old database may carry one, so it is written straight.
pub(super) const AWKWARD: [&str; 7] = ["Rent, \"joint\"", "=SUM(1+1)", "+cash", "-5% fund", "@home", "\t=tab", "x, y\r\nz"];

/// An account for each of `AWKWARD`, each with a transfer from the main account whose note is awkward too, and a transaction. Returns the ids.
pub(super) fn awkward(c: &Case) -> Vec<i64> {
    let mut ids = Vec::new();
    for (i, name) in AWKWARD.iter().enumerate() {
        let id = account(&c.conn, &format!("plain {i}"));
        c.conn.execute("UPDATE accounts SET name = ?1 WHERE id = ?2", rusqlite::params![name, id]).unwrap();
        let day = format!("2026-04-{:02}", i + 1);
        accounts::transfers::create(&c.conn, &NewTransfer { note: Some(format!("=note {i}\nsecond, \"line\"")), ..new_transfer(MAIN_ID, id, "1.00", &day) }).unwrap();
        spend(&c.conn, c.rent, &format!("in {i}"), "2.00", &day, None);
        let spent: i64 = c.conn.query_row("SELECT id FROM transactions WHERE name = ?1", [format!("in {i}")], |r| r.get(0)).unwrap();
        c.conn.execute("UPDATE transactions SET account_id = ?1 WHERE id = ?2", rusqlite::params![id, spent]).unwrap();
        ids.push(id);
    }
    ids
}

#[test]
fn a_name_with_commas_quotes_or_a_formula_is_text_in_the_csv_and_exact_in_the_json() {
    let c = case();
    awkward(&c);
    let json_file = file(&c.conn, &Selection::default());
    for (i, name) in AWKWARD.iter().enumerate() {
        assert_eq!(row(&json_file, "transactions", &format!("in {i}"), "")["account"].as_str(), Some(*name), "the JSON holds the name as it is");
        assert!(json_file["accounts"].as_array().unwrap().iter().any(|a| a["name"].as_str() == Some(*name)));
        let note = format!("=note {i}\nsecond, \"line\"");
        assert!(json_file["transfers"].as_array().unwrap().iter().any(|x| x["to"].as_str() == Some(*name) && x["note"].as_str() == Some(note.as_str())));
    }
    // the CSV: no cell starts like a formula, and a name with a comma, a quote or a line break comes back as it was
    let rows = read_csv(&csv(&c.conn, &Selection::default()).unwrap());
    for r in &rows[1..] {
        for cell in r {
            assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{cell:?} would run as a formula");
        }
    }
    for (i, name) in AWKWARD.iter().enumerate() {
        let r = rows[1..].iter().find(|r| r[3] == format!("in {i}")).unwrap();
        assert_eq!(r[9], neutralise(name), "{name:?}");
    }
    assert!(rows.iter().all(|r| r.len() == 10), "the line break did not split a row");
    assert_eq!(rows[1..].iter().find(|r| r[3] == "in 6").unwrap()[9], "x, y\r\nz");
    assert_eq!(rows[1..].iter().find(|r| r[3] == "in 0").unwrap()[9], "Rent, \"joint\"", "quoted, and no apostrophe: it does not start like a formula");
    assert_eq!(rows[1..].iter().find(|r| r[3] == "in 5").unwrap()[9], "'\t=tab");
}

#[test]
fn exporting_changes_nothing() {
    let c = case();
    let before = snapshot(&c.conn);
    let history: i64 = c.conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
    for selection in [Selection::default(), Selection { category_ids: Some(vec![c.rent.to_string()]), include_debts: true, ..Default::default() }] {
        csv(&c.conn, &selection).unwrap();
        json(&c.conn, &selection, now()).unwrap();
        preview(&c.conn, &selection).unwrap();
        preview_text(&c.conn, &selection, TextFormat::Json, now()).unwrap();
        workbook(&c.conn, &selection, &Default::default()).unwrap();
    }
    assert_eq!(snapshot(&c.conn), before, "no row, stamp, marker or history line");
    assert_eq!(c.conn.query_row::<i64, _, _>("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap(), history);
}

/// A transaction of every category and a recurring item and a ledger entry in several accounts, over the world of the choices tests: the
/// accounts of the transactions follow their category, those of the recurring items follow their name, and those of the entries their friend.
pub(super) struct Spread {
    /// The ids of the main account, Savings, Cash, Credit card and Old savings (archived).
    pub(super) accounts: [i64; 5],
    pub(super) names: [&'static str; 5],
    /// The account of each category, in the order of `World::categories`.
    pub(super) of_category: Vec<usize>,
    pub(super) of_friend: [(&'static str, usize); 2],
    pub(super) of_item: Vec<(&'static str, usize)>,
}

impl Spread {
    pub(super) fn of_item(&self, name: &str) -> usize {
        self.of_item.iter().find(|(n, _)| *n == name).unwrap().1
    }

    pub(super) fn of_friend(&self, name: &str) -> usize {
        self.of_friend.iter().find(|(n, _)| *n == name).unwrap().1
    }
}

pub(super) fn spread(world: &World) -> Spread {
    let conn = &world.conn;
    let names = ["Main account", "Savings", "Cash", "Credit card", "Old savings"];
    let mut accounts = [MAIN_ID; 5];
    for (at, name) in names.iter().enumerate().skip(1) {
        accounts[at] = crate::accounts::create(conn, &new_account(name), TODAY).unwrap().id;
    }
    let of_category = vec![0, 1, 2, 3, 4, 1];
    for (category, at) in world.categories.iter().zip(&of_category) {
        conn.execute("UPDATE transactions SET account_id = ?1 WHERE category_id = ?2", rusqlite::params![accounts[*at], category.0]).unwrap();
    }
    let of_item = vec![("Pay", 1), ("Rent", 0), ("Food box", 2), ("Gym", 4), ("Old magazine", 3), ("Soon", 1), ("Future", 2), ("Paused", 3)];
    for (name, at) in &of_item {
        conn.execute("UPDATE recurring_transactions SET account_id = ?1 WHERE name = ?2", rusqlite::params![accounts[*at], name]).unwrap();
    }
    let of_friend = [("Anna", 0), ("Bob", 4)];
    for (name, at) in of_friend {
        conn.execute("UPDATE ledger_entries SET account_id = ?1 WHERE person_id = (SELECT id FROM people WHERE name = ?2)", rusqlite::params![accounts[at], name]).unwrap();
    }
    for pair in [(1, 2, "300.00", "2026-02-02"), (2, 3, "5.00", "2026-02-03"), (3, 4, "7.77", "2026-02-04")] {
        crate::accounts::fixtures::transfer(conn, accounts[pair.0], accounts[pair.1], pair.2, pair.3);
    }
    crate::accounts::archive(conn, accounts[4], TODAY).unwrap();
    Spread { accounts, names, of_category, of_friend, of_item }
}

#[test]
fn every_choice_names_the_accounts_of_the_rows_it_holds_and_gives_each_row_its_account() {
    let world = filters_tests::world();
    let spread = spread(&world);
    let conn = &world.conn;
    let category_at = |id: &str| world.categories.iter().position(|c| c.0.to_string() == id).unwrap();
    let mut with_accounts = 0;
    for selection in random_selections(&world, 150, 0xacc0_0d17) {
        let label = filters_tests::describe(&selection);
        let file = file(conn, &selection);
        // every row has the account its category, its name or its friend has
        let mut held = BTreeSet::new();
        if wants(&selection, Section::Transactions) {
            for t in file["transactions"].as_array().unwrap() {
                let at = spread.of_category[category_at(t["categoryId"].as_str().unwrap())];
                assert_eq!((t["accountId"].as_str(), t["account"].as_str()), (Some(spread.accounts[at].to_string().as_str()), Some(spread.names[at])), "{label}");
                held.insert(at);
            }
        }
        if wants(&selection, Section::Recurring) {
            for item in file["recurring"].as_array().unwrap() {
                let at = spread.of_item(item["name"].as_str().unwrap());
                assert_eq!((item["accountId"].as_str(), item["account"].as_str()), (Some(spread.accounts[at].to_string().as_str()), Some(spread.names[at])), "{label}");
                held.insert(at);
            }
        }
        if wants(&selection, Section::Ledger) {
            for entry in file["ledger"].as_array().unwrap() {
                let at = spread.of_friend(entry["person"].as_str().unwrap());
                assert_eq!((entry["accountId"].as_str(), entry["account"].as_str()), (Some(spread.accounts[at].to_string().as_str()), Some(spread.names[at])), "{label}");
                held.insert(at);
            }
        }
        // the accounts: every one with no limit, otherwise those of the rows, and nothing but the name and the kind of them
        // (in the person's order, which is the order of the list, the archived account being the last)
        let want: Vec<usize> = if !limited(&selection) { (0..5).collect() } else { held.iter().copied().collect() };
        if wants(&selection, Section::Accounts) {
            let want_names: Vec<&str> = want.iter().map(|at| spread.names[*at]).collect();
            assert_eq!(named(&file["accounts"]), want_names, "accounts, {label}");
            if limited(&selection) {
                assert!(file["accounts"].as_array().unwrap().iter().all(|a| a.as_object().unwrap().len() == 3), "only the id, the name and the kind: {label}");
            }
            with_accounts += 1;
        } else {
            assert!(file.get("accounts").is_none(), "{label}");
        }
        assert_eq!(file.get("transfers").is_some(), !limited(&selection) && wants(&selection, Section::Transfers), "transfers, {label}");

        // the preview, and the CSV (which takes no part in the choice of parts)
        let p = preview(conn, &selection).unwrap();
        assert_eq!(p.accounts as usize, if wants(&selection, Section::Accounts) { want.len() } else { 0 }, "{label}");
        assert_eq!(p.transfers, if !limited(&selection) && wants(&selection, Section::Transfers) { 3 } else { 0 }, "{label}");
        let table = read_csv(&csv(conn, &selection).unwrap());
        let rows = expected(&world, &selection);
        assert_eq!(table.len() - 1, rows.len(), "{label}");
        for (line, &at) in table[1..].iter().zip(&rows) {
            let account = spread.of_category[category_at(&world.rows[at].category.to_string())];
            assert_eq!(line[9], spread.names[account], "{label}");
        }
    }
    assert!(with_accounts > 40, "enough of the choices asked for the accounts: {with_accounts}");
}
