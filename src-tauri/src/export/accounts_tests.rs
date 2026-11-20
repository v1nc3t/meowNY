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
    // every field of an account but its row count and what is worked out from two others in the file (`available` and `free`)
    assert_eq!(
        file["accounts"][2],
        json!({ "id": c.cash.to_string(), "name": "Cash", "kind": "CASH", "number": null, "openingBalance": "250.00", "openingDate": "2025-12-01", "archived": false,
                "isMain": false, "balance": "150.00", "changeThisMonth": "0.00", "lastActivity": "2026-02-15",
                "bic": null, "notes": null, "group": null, "includeInNetWorth": true, "virtualBalance": "0.00",
                "currency": "EUR", "foreign": false, "rate": null, "balanceInMain": "150.00", "setAside": "0.00" })
    );
    assert_eq!(file["accounts"][3]["openingBalance"], "-240.00");
    assert_eq!((file["accounts"][1]["number"].as_str(), file["accounts"][4]["archived"].as_bool(), file["accounts"][0]["isMain"].as_bool()), (Some("IE29 AIBK 9311 5212 3456 78"), Some(true), Some(true)));
    assert!(file["accounts"].as_array().unwrap().iter().all(|a| a.get("entries").is_none() && a.get("available").is_none() && a.get("free").is_none()));

    let transfers = file["transfers"].as_array().unwrap();
    assert_eq!(transfers.len(), 3);
    assert_eq!(
        transfers[0],
        json!({ "id": "1", "date": "2026-02-02", "fromAccountId": "1", "from": "Main account", "toAccountId": c.savings.to_string(), "to": "Savings", "amount": "300.00", "toAmount": null, "note": "monthly saving" })
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
fn the_csv_has_the_account_what_it_really_paid_and_its_currency_after_the_columns_of_before() {
    let c = case();
    let text = csv(&c.conn, &Selection::default()).unwrap();
    assert!(text.starts_with("\u{feff}date,type,category,name,amount,currency,description,documents,my_share,account,native_amount,account_currency\r\n"), "{}", &text[..120]);
    let rows = read_csv(&text);
    assert!(rows.iter().all(|r| r.len() == 12));
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
    // an account in the app's currency has no native amount, and says the code of its currency, which is the app's
    assert!(rows[1..].iter().all(|r| r[10].is_empty() && r[11] == "EUR"), "{rows:?}");
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
    assert!(rows[1..].iter().all(|r| r[9] == "Main account" && r[10].is_empty() && r[11] == "EUR"));
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
        { "id": c.cash.to_string(), "name": "Cash", "kind": "CASH", "currency": "EUR" },
        { "id": c.card.to_string(), "name": "Credit card", "kind": "CREDIT_CARD", "currency": "EUR" },
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
    assert_eq!(limited["accounts"], json!([{ "id": c.old.to_string(), "name": "Old savings", "kind": "SAVINGS", "currency": "EUR" }]));
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
    assert!(rows.iter().all(|r| r.len() == 12), "the line break did not split a row");
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

// ---- the details of an account (docs/accounts-design.md, section 20.5) -----------------------------------------------

/// The notes of Cash below: a leading `=`, a line break, a comma and quotes, which are what a spreadsheet and a CSV reader trip on.
pub(super) const AWKWARD_NOTES: &str = "=1+1 first line\nsecond, \"quoted\" line";

/// `case()` with details on four of its accounts. Cash: a BIC, `AWKWARD_NOTES`, a group, left out of net worth. The credit card: a BIC, a group
/// and a limit of 1000.00 as its virtual balance. Savings: a BIC, a group and a note that all look like formulas, and a virtual balance of
/// -25.50. Old savings (archived, so by SQL): left out of net worth, with a note and a group. The main account keeps the defaults.
pub(super) fn detailed() -> Case {
    let c = case();
    let set = |id: i64, patch: accounts::AccountPatch| {
        accounts::update(&c.conn, id, &patch, TODAY).unwrap();
    };
    set(c.cash, accounts::AccountPatch { bic: Some(Some("COBADEFFXXX".into())), notes: Some(Some(AWKWARD_NOTES.into())), group: Some(Some("Everyday".into())), include_in_net_worth: Some(false), ..Default::default() });
    set(c.card, accounts::AccountPatch { bic: Some(Some("DEUTDEFF500".into())), group: Some(Some("Cards".into())), virtual_balance: Some("1000.00".into()), ..Default::default() });
    set(
        c.savings,
        accounts::AccountPatch { bic: Some(Some("@BIC".into())), notes: Some(Some("-minus note".into())), group: Some(Some("=Savings".into())), virtual_balance: Some("-25.50".into()), ..Default::default() },
    );
    c.conn.execute("UPDATE accounts SET include_net_worth = 0, notes = 'closed 2025', group_name = 'Old' WHERE id = ?1", [c.old]).unwrap();
    c
}

/// What the file says about one account, by name.
fn account_named<'a>(file: &'a Value, name: &str) -> &'a Value {
    file["accounts"].as_array().unwrap().iter().find(|a| a["name"] == name).unwrap_or_else(|| panic!("no account {name}"))
}

#[test]
fn the_json_has_the_five_details_of_every_account_and_not_the_amount_worked_out_from_two_of_them() {
    let c = detailed();
    let file = file(&c.conn, &Selection::default());
    assert_eq!(
        account_named(&file, "Cash"),
        &json!({ "id": c.cash.to_string(), "name": "Cash", "kind": "CASH", "number": null, "openingBalance": "250.00", "openingDate": "2025-12-01", "archived": false,
                 "isMain": false, "balance": "150.00", "changeThisMonth": "0.00", "lastActivity": "2026-02-15",
                 "bic": "COBADEFFXXX", "notes": AWKWARD_NOTES, "group": "Everyday", "includeInNetWorth": false, "virtualBalance": "0.00",
                 "currency": "EUR", "foreign": false, "rate": null, "balanceInMain": "150.00", "setAside": "0.00" }),
        "the JSON holds the text as it is: the line break, the comma, the quotes and the `=` are not touched"
    );
    let card = account_named(&file, "Credit card");
    assert_eq!((&card["bic"], &card["notes"], &card["group"], &card["includeInNetWorth"], &card["virtualBalance"]), (&json!("DEUTDEFF500"), &json!(null), &json!("Cards"), &json!(true), &json!("1000.00")));
    assert_eq!(card["balance"], "-945.50", "a virtual balance is in no balance");
    let savings = account_named(&file, "Savings");
    assert_eq!((&savings["bic"], &savings["notes"], &savings["group"], &savings["virtualBalance"]), (&json!("@BIC"), &json!("-minus note"), &json!("=Savings"), &json!("-25.50")));
    let old = account_named(&file, "Old savings");
    assert_eq!((&old["archived"], &old["includeInNetWorth"], &old["notes"], &old["group"], &old["bic"]), (&json!(true), &json!(false), &json!("closed 2025"), &json!("Old"), &json!(null)));
    let main = account_named(&file, "Main account");
    assert_eq!((&main["bic"], &main["notes"], &main["group"], &main["includeInNetWorth"], &main["virtualBalance"]), (&json!(null), &json!(null), &json!(null), &json!(true), &json!("0.00")));
    // every account has the same sixteen fields: the five are always there, null or a default when the person set nothing
    let keys = |a: &Value| -> BTreeSet<String> { a.as_object().unwrap().keys().cloned().collect() };
    let expected: BTreeSet<String> = [
        "id", "name", "kind", "number", "openingBalance", "openingDate", "archived", "isMain", "balance", "changeThisMonth", "lastActivity", "bic", "notes", "group", "includeInNetWorth", "virtualBalance", "currency",
        "foreign", "rate", "balanceInMain", "setAside",
    ]
    .map(String::from)
    .into();
    for a in file["accounts"].as_array().unwrap() {
        assert_eq!(keys(a), expected, "{}", a["name"]);
    }
    // they are figures the file can work out: the balance and the virtual balance are both in it, and so are the balance and what the stashes set aside
    assert!(file["accounts"].as_array().unwrap().iter().all(|a| a.get("available").is_none() && a.get("free").is_none()));
}

#[test]
fn a_limited_export_gives_an_account_no_detail_in_the_json_and_the_csv_has_none_at_all() {
    let c = detailed();
    let secrets = ["COBADEFFXXX", "DEUTDEFF500", "@BIC", "Everyday", "Cards", "=Savings", "1000.00", "-25.50", "first line", "quoted", "-minus note", "closed 2025"];
    let selections = [
        Selection { category_ids: Some(vec![c.secret.to_string()]), ..Default::default() },
        Selection { category_ids: Some(vec![c.rent.to_string(), c.salary.to_string()]), include_debts: true, ..Default::default() },
        Selection { from: Some("2026-02-10".into()), to: Some("2026-02-12".into()), ..Default::default() },
        Selection { from: Some("2026-01-01".into()), sections: Some(vec![Section::Accounts, Section::Transactions, Section::Recurring]), ..Default::default() },
    ];
    for selection in &selections {
        let limited = file(&c.conn, selection);
        let listed = limited["accounts"].as_array().unwrap();
        assert!(!listed.is_empty(), "{selection:?}: some account is named");
        for a in listed {
            let keys: Vec<&str> = a.as_object().unwrap().keys().map(String::as_str).collect();
            assert_eq!(keys.len(), 4, "{selection:?}: only the id, the name, the kind and the currency: {keys:?}");
            for key in ["id", "name", "kind", "currency"] {
                assert!(keys.contains(&key), "{selection:?}: {key}");
            }
        }
        let text = serde_json::to_string(&limited).unwrap();
        for hidden in secrets.iter().chain(["\"bic\"", "\"notes\"", "\"group\"", "\"includeInNetWorth\"", "\"virtualBalance\"", "\"available\"", "\"free\"", "\"setAside\"", "\"stashes\""].iter()) {
            assert!(!text.contains(hidden), "{selection:?}: {hidden:?} leaked into a limited export");
        }
    }
    // Cash and the card are named by the session and the plan, and have details, and the file has none of them
    let therapy = file(&c.conn, &selections[0]);
    assert_eq!(
        therapy["accounts"],
        json!([{ "id": c.cash.to_string(), "name": "Cash", "kind": "CASH", "currency": "EUR" }, { "id": c.card.to_string(), "name": "Credit card", "kind": "CREDIT_CARD", "currency": "EUR" }])
    );
    // the CSV is the transactions, and an account is a name in its last column: nothing of the details, with a limit or without
    let before = csv(&case().conn, &Selection::default()).unwrap();
    let all = csv(&c.conn, &Selection::default()).unwrap();
    assert_eq!(all, before, "the same CSV as without the details");
    for selection in std::iter::once(&Selection::default()).chain(&selections) {
        let text = csv(&c.conn, selection).unwrap();
        assert!(!secrets.iter().any(|secret| text.contains(secret)), "{selection:?}");
        assert!(text.lines().next().unwrap().ends_with("my_share,account,native_amount,account_currency"), "the columns are what they were");
    }
}

#[test]
fn a_file_of_the_accounts_alone_has_the_details_and_a_limited_one_of_them_has_none() {
    let c = detailed();
    let only = file(&c.conn, &Selection { sections: Some(vec![Section::Accounts]), ..Default::default() });
    assert_eq!(account_named(&only, "Credit card")["virtualBalance"], "1000.00");
    assert_eq!(only.as_object().unwrap().len(), 6, "the accounts and the ordinary head of the file: {:?}", only.as_object().unwrap().keys().collect::<Vec<_>>());
    // with a limit that names every account, they are still only named
    let dated = file(&c.conn, &Selection { from: Some("2026-01-01".into()), sections: Some(vec![Section::Accounts, Section::Transactions, Section::Recurring, Section::Ledger]), ..Default::default() });
    assert!(dated["accounts"].as_array().unwrap().iter().all(|a| a.as_object().unwrap().len() == 4));
}

#[test]
fn exporting_accounts_with_details_changes_nothing() {
    let c = detailed();
    let before = snapshot(&c.conn);
    let history: i64 = c.conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
    let details = |conn: &Connection| -> Vec<String> {
        conn.prepare("SELECT stamp || '|' || updated_at || '|' || COALESCE(bic, '-') || '|' || COALESCE(notes, '-') || '|' || COALESCE(group_name, '-') || '|' || include_net_worth || '|' || virtual_balance_minor FROM accounts ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let held = details(&c.conn);
    for selection in [Selection::default(), Selection { category_ids: Some(vec![c.rent.to_string()]), include_debts: true, ..Default::default() }, Selection { sections: Some(vec![Section::Accounts]), ..Default::default() }] {
        csv(&c.conn, &selection).unwrap();
        json(&c.conn, &selection, now()).unwrap();
        preview(&c.conn, &selection).unwrap();
        preview_text(&c.conn, &selection, TextFormat::Json, now()).unwrap();
        let _ = workbook(&c.conn, &selection, &Default::default());
    }
    assert_eq!(snapshot(&c.conn), before, "no row, stamp, marker or history line: the new columns are in the snapshot");
    assert_eq!(details(&c.conn), held);
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
                assert!(file["accounts"].as_array().unwrap().iter().all(|a| a.as_object().unwrap().len() == 4), "only the id, the name, the kind and the currency: {label}");
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

// ---- a currency per account (docs/accounts-design.md, section 21.6) --------------------------------------------------------

/// `case()` and a Romanian-lei account at 5.00 lei to the euro with 100.00 lei on it (since 2025-12-01), which has a payment of 4.00 in the
/// Rent category that really paid 20.00 lei (2026-03-02) and two transfers between currencies: 10.00 euro from Cash that became 50.50 lei
/// (2026-03-03), and 100.00 lei to Savings that became 19.00 euro (2026-03-04). So it holds 30.50 lei, which is 6.10 euro, Cash 140.00 and Savings
/// 1719.00. Returns the case and the id of the lei account.
pub(super) fn foreign_case() -> (Case, i64) {
    let c = case();
    let lei = accounts::create(&c.conn, &NewAccount { currency: Some("RON".into()), rate: Some("5.00".into()), ..opened("Lei account", Kind::Checking, None, "100.00") }, TODAY).unwrap().id;
    crate::transactions::create(
        &c.conn,
        &crate::transactions::NewTransaction {
            name: "Lei lunch".into(),
            amount: "4.00".into(),
            payment_date: "2026-03-02".into(),
            category_id: c.rent.to_string(),
            description: None,
            client_uuid: None,
            account_id: Some(lei.to_string()),
            native_amount: Some("20.00".into()),
        },
    )
    .unwrap();
    for (from, to, amount, arrives, date, note) in [(c.cash, lei, "10.00", "50.50", "2026-03-03", "cash to lei"), (lei, c.savings, "100.00", "19.00", "2026-03-04", "lei to savings")] {
        accounts::transfers::create(&c.conn, &NewTransfer { note: Some(note.into()), to_amount: Some(arrives.into()), ..new_transfer(from, to, amount, date) }).unwrap();
    }
    (c, lei)
}

#[test]
fn the_json_says_the_currency_rate_and_worth_of_each_account_and_what_each_row_really_moved() {
    let (c, lei) = foreign_case();
    let file = file(&c.conn, &Selection::default());
    assert_eq!(named(&file["accounts"]), ["Main account", "Savings", "Cash", "Credit card", "Lei account", "Old savings"], "the archived one is last");
    let lei_row = account_named(&file, "Lei account");
    assert_eq!(
        lei_row,
        &json!({ "id": lei.to_string(), "name": "Lei account", "kind": "CHECKING", "number": null, "openingBalance": "100.00", "openingDate": "2025-12-01", "archived": false,
                 "isMain": false, "balance": "30.50", "changeThisMonth": "0.00", "lastActivity": "2026-03-04",
                 "bic": null, "notes": null, "group": null, "includeInNetWorth": true, "virtualBalance": "0.00",
                 "currency": "RON", "foreign": true, "rate": "5.00", "balanceInMain": "6.10", "setAside": "0.00" }),
        "every amount of it is in lei, and what it is worth in the app's currency is beside them"
    );
    // an account in the app's currency says so: its code, no rate, and its worth is its balance
    let savings = account_named(&file, "Savings");
    assert_eq!((&savings["currency"], &savings["foreign"], &savings["rate"], &savings["balance"], &savings["balanceInMain"]), (&json!("EUR"), &json!(false), &json!(null), &json!("1719.00"), &json!("1719.00")));
    // the worth of the accounts that count is what the dashboard calls them
    let worth: i64 = file["accounts"].as_array().unwrap().iter().filter(|a| a["includeInNetWorth"] == true).map(|a| crate::money::parse_signed_amount(a["balanceInMain"].as_str().unwrap()).unwrap()).sum();
    assert_eq!(format_amount(worth), accounts::summary(&c.conn, TODAY).unwrap().in_accounts);
    assert_eq!(format_amount(worth), "2919.60", "1800.00 + 1719.00 + 140.00 - 945.50 + 200.00 + 6.10");
    // the lunch has its amount in the app's currency, which the budgets count, and the lei the account really paid; the others have no native amount
    let lunch = row(&file, "transactions", "Lei lunch", "");
    assert_eq!((&lunch["amount"], &lunch["nativeAmount"], &lunch["account"], &lunch["myShare"]), (&json!("4.00"), &json!("20.00"), &json!("Lei account"), &json!("4.00")));
    for t in file["transactions"].as_array().unwrap().iter().filter(|t| t["name"] != "Lei lunch") {
        assert_eq!(t["nativeAmount"], json!(null), "{}", t["name"]);
    }
    // the transfers between currencies say what arrived, the others do not
    let arrives = |note: &str| file["transfers"].as_array().unwrap().iter().find(|x| x["note"] == note).unwrap()["toAmount"].clone();
    assert_eq!((arrives("cash to lei"), arrives("lei to savings"), arrives("monthly saving")), (json!("50.50"), json!("19.00"), json!(null)));
    let to_savings = file["transfers"].as_array().unwrap().iter().find(|x| x["note"] == "lei to savings").unwrap();
    assert_eq!((&to_savings["from"], &to_savings["to"], &to_savings["amount"]), (&json!("Lei account"), &json!("Savings"), &json!("100.00")), "what left is in lei");
    // the app's currency of the file is still the app's: the amounts of the budgets and the statistics are in it
    assert_eq!(file["settings"]["currency"], "EUR");
    assert_eq!(preview(&c.conn, &Selection::default()).unwrap().accounts, 6);
}

#[test]
fn the_csv_has_what_each_payment_really_paid_and_the_currency_of_its_account_and_the_old_columns_are_as_before() {
    let (c, _) = foreign_case();
    let text = csv(&c.conn, &Selection::default()).unwrap();
    let rows = read_csv(&text);
    assert!(rows.iter().all(|r| r.len() == 12));
    assert_eq!(rows[0][9..], ["account", "native_amount", "account_currency"]);
    let lunch = rows[1..].iter().find(|r| r[3] == "Lei lunch").unwrap();
    // the amount and the currency of the file are the app's, as for every row; the account's own currency and amount are the last two
    assert_eq!(lunch[..], ["2026-03-02", "EXPENSE", "Rent", "Lei lunch", "4.00", "EUR", "", "", "4.00", "Lei account", "20.00", "RON"]);
    for r in rows[1..].iter().filter(|r| r[3] != "Lei lunch") {
        assert_eq!((r[10].as_str(), r[11].as_str()), ("", "EUR"), "{}: no native amount, and the account is in the app's currency", r[3]);
    }
    // the ten columns of before are what they were: the same file with the two columns taken off is the file of the world without the lei
    let before = csv(&case().conn, &Selection::default()).unwrap();
    let cut = |text: &str| -> Vec<Vec<String>> { read_csv(text).into_iter().map(|r| r[..10].to_vec()).filter(|r| r[3] != "Lei lunch").collect() };
    assert_eq!(cut(&text), cut(&before));
    // a payment of a main-currency account: the cells are empty, not zero
    assert!(before.lines().skip(1).all(|l| l.ends_with(",,EUR")), "{before}");
    // a limit keeps the two columns, and a row in the lei account still says what it paid
    let limited = read_csv(&csv(&c.conn, &Selection { category_ids: Some(vec![c.rent.to_string()]), ..Default::default() }).unwrap());
    assert!(limited.iter().all(|r| r.len() == 12));
    assert_eq!(limited.iter().find(|r| r[3] == "Lei lunch").map(|r| (r[10].as_str(), r[11].as_str())), Some(("20.00", "RON")));
}

#[test]
fn a_limited_json_names_the_currency_of_the_accounts_of_its_rows_and_nothing_about_their_rate_or_worth() {
    let (c, lei) = foreign_case();
    let rent = file(&c.conn, &Selection { category_ids: Some(vec![c.rent.to_string()]), ..Default::default() });
    assert_eq!(
        rent["accounts"],
        json!([
            { "id": "1", "name": "Main account", "kind": "CHECKING", "currency": "EUR" },
            { "id": c.card.to_string(), "name": "Credit card", "kind": "CREDIT_CARD", "currency": "EUR" },
            { "id": lei.to_string(), "name": "Lei account", "kind": "CHECKING", "currency": "RON" },
        ])
    );
    assert!(rent.get("transfers").is_none());
    assert_eq!(row(&rent, "transactions", "Lei lunch", "")["nativeAmount"], "20.00", "a row still says what it really paid");
    let text = serde_json::to_string(&rent).unwrap();
    for hidden in ["\"rate\"", "\"balanceInMain\"", "\"foreign\"", "5.00", "30.50", "6.10", "openingBalance", "toAmount", "50.50", "19.00", "cash to lei"] {
        assert!(!text.contains(hidden), "{hidden:?} leaked into a limited export");
    }
    // an export of the accounts alone under a limit has them named by their rows, with their currency
    let alone = file(&c.conn, &Selection { from: Some("2026-03-02".into()), to: Some("2026-03-02".into()), sections: Some(vec![Section::Accounts, Section::Transactions]), ..Default::default() });
    assert_eq!(alone["accounts"], json!([{ "id": lei.to_string(), "name": "Lei account", "kind": "CHECKING", "currency": "RON" }]), "the lunch is the one row of that day");
}

#[test]
fn a_foreign_account_with_a_name_that_looks_like_a_formula_is_text_in_the_csv_and_its_amounts_are_numbers() {
    let (c, lei) = foreign_case();
    c.conn.execute("UPDATE accounts SET name = '=SUM(1+1)' WHERE id = ?1", [lei]).unwrap();
    let rows = read_csv(&csv(&c.conn, &Selection::default()).unwrap());
    let lunch = rows[1..].iter().find(|r| r[3] == "Lei lunch").unwrap();
    assert_eq!((lunch[9].as_str(), lunch[10].as_str(), lunch[11].as_str()), ("'=SUM(1+1)", "20.00", "RON"), "the name is text, the amount is a number that is not prefixed");
    for row in &rows[1..] {
        for cell in row {
            assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{cell:?} would run as a formula");
        }
    }
    let json_file = file(&c.conn, &Selection::default());
    assert_eq!(row(&json_file, "transactions", "Lei lunch", "")["account"], "=SUM(1+1)", "the JSON holds the name as it is");
}

#[test]
fn exporting_accounts_in_other_currencies_changes_nothing() {
    let (c, _) = foreign_case();
    let before = snapshot(&c.conn);
    let history: i64 = c.conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
    for selection in [Selection::default(), Selection { category_ids: Some(vec![c.rent.to_string()]), include_debts: true, ..Default::default() }, Selection { sections: Some(vec![Section::Accounts, Section::Transfers]), ..Default::default() }] {
        csv(&c.conn, &selection).unwrap();
        json(&c.conn, &selection, now()).unwrap();
        preview(&c.conn, &selection).unwrap();
        preview_text(&c.conn, &selection, TextFormat::Json, now()).unwrap();
        workbook(&c.conn, &selection, &Default::default()).unwrap();
    }
    assert_eq!(snapshot(&c.conn), before, "no row, stamp, marker or history line: the new columns are in the snapshot");
    assert_eq!(c.conn.query_row::<i64, _, _>("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap(), history);
}

#[test]
fn a_main_currency_world_gives_null_and_empty_values_for_the_new_fields_in_every_format() {
    let c = case();
    let file = file(&c.conn, &Selection::default());
    assert!(file["transactions"].as_array().unwrap().iter().all(|t| t.as_object().unwrap().contains_key("nativeAmount") && t["nativeAmount"].is_null()), "the key is there, and null");
    assert!(file["transfers"].as_array().unwrap().iter().all(|x| x.as_object().unwrap().contains_key("toAmount") && x["toAmount"].is_null()));
    assert!(file["accounts"].as_array().unwrap().iter().all(|a| a["currency"] == "EUR" && a["rate"].is_null() && a["balanceInMain"] == a["balance"]));
    // the currency changed to another one: the accounts' code follows it (they have none of their own)
    let mut conn = c.conn;
    crate::currency::change(&mut conn, "USD", Some("2"), TODAY).unwrap();
    let usd = self::file(&conn, &Selection::default());
    assert!(usd["accounts"].as_array().unwrap().iter().all(|a| a["currency"] == "USD" && a["foreign"] == false));
    let rows = read_csv(&csv(&conn, &Selection::default()).unwrap());
    assert!(rows[1..].iter().all(|r| r[10].is_empty() && r[11] == "USD"));
}
