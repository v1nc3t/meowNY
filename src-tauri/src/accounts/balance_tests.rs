//! The numbers of accounts (`docs/accounts-design.md`, section 3): the balance on a day, the change this month, the last activity, the
//! statement, the chart and net worth, against a plain recomputation from random rows, and the cases that are easy to get wrong.

use chrono::{Days, NaiveDate};
use rusqlite::Connection;

use super::fixtures::*;
use super::transfers;
use super::{breakdown, create, get, history, list, statement, summary, to_main, update, AccountPatch, Breakdown, HistoryPoint, Kind, Line, NewAccount, Share, MAIN_ID, MAIN_NAME};
use crate::categories::{self, CategoryType};
use crate::debts::fixtures::{equal, who};
use crate::debts::ledger::{self, Expense, NewEntry};
use crate::debts::people;
use crate::debts::splits::{set_split, SplitInput};
use crate::money::{format_amount, MAX_MINOR};
use crate::transactions::{self, NewTransaction};
use crate::vault::AppError;

fn on(conn: &Connection, account: i64, category: i64, name: &str, amount: &str, date: &str) -> i64 {
    transactions::create(
        conn,
        &NewTransaction { name: name.into(), amount: amount.into(), payment_date: date.into(), category_id: category.to_string(), description: None, client_uuid: None, account_id: Some(account.to_string()), native_amount: None },
    )
    .unwrap()
    .id
}

fn entry(conn: &Connection, account: i64, person: i64, kind: ledger::Kind, amount: &str, date: &str) -> i64 {
    ledger::add(conn, &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: date.into(), note: None, transaction_id: None, account_id: Some(account.to_string()) })
        .unwrap()
        .id
}

fn opened(conn: &Connection, name: &str, balance: &str, date: &str) -> i64 {
    create(conn, &NewAccount { opening_balance: Some(balance.into()), opening_date: Some(date.into()), ..new_account(name) }, TODAY).unwrap().id
}

fn paid_by(payer: i64, split: SplitInput) -> SplitInput {
    SplitInput { paid_by: Some(payer.to_string()), ..split }
}

// ---- against a plain recomputation -------------------------------------------------------------

/// One thing that moved an account, as the test itself keeps it.
struct Move {
    account: i64,
    day: String,
    delta: i64,
    opening: bool,
}

fn model_balance(moves: &[Move], account: i64, day: &str) -> i64 {
    moves.iter().filter(|m| m.account == account && m.day.as_str() <= day).map(|m| m.delta).sum()
}

fn cents(rng: &mut Rng) -> (i64, String) {
    let minor = rng.between(1, 250_000) as i64;
    (minor, format_amount(minor))
}

/// A day from the end of 2025 to the start of 2027, so that some rows are before every opening date and some after today.
fn day(rng: &mut Rng) -> String {
    NaiveDate::from_ymd_opt(2025, 11, 1).unwrap().checked_add_days(Days::new(rng.between(0, 480))).unwrap().to_string()
}

#[test]
fn every_number_of_an_account_equals_a_plain_sum_of_what_moved_it_for_random_rows() {
    for seed in [1u64, 7, 42, 2026, 99_991] {
        let w = world();
        let mut rng = Rng(seed);
        let friends = [friend(&w.conn, "Anna"), friend(&w.conn, "Bob")];
        let mut moves: Vec<Move> = Vec::new();
        let mut rows: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();

        // the main account with an opening balance, and three more: one that starts at 0, one that starts negative, one in the future
        update(&w.conn, MAIN_ID, &AccountPatch { opening_balance: Some("1000.00".into()), opening_date: Some("2026-01-01".into()), ..Default::default() }, TODAY).unwrap();
        moves.push(Move { account: MAIN_ID, day: "2026-01-01".into(), delta: 100_000, opening: true });
        let savings = account(&w.conn, "Savings");
        let card = opened(&w.conn, "Card", "-350.25", "2026-03-15");
        moves.push(Move { account: card, day: "2026-03-15".into(), delta: -35_025, opening: true });
        let later = opened(&w.conn, "Later", "77.00", "2026-12-24");
        moves.push(Move { account: later, day: "2026-12-24".into(), delta: 7700, opening: true });
        let accounts = [MAIN_ID, savings, card, later];

        for step in 0..120 {
            let account = accounts[rng.between(0, 3) as usize];
            let (minor, amount) = cents(&mut rng);
            let date = day(&mut rng);
            *rows.entry(account).or_default() += 1;
            match rng.between(0, 9) {
                0 | 1 => {
                    on(&w.conn, account, w.food, "Shop", &amount, &date);
                    moves.push(Move { account, day: date, delta: -minor, opening: false });
                }
                2 => {
                    on(&w.conn, account, w.salary, "Pay", &amount, &date);
                    moves.push(Move { account, day: date, delta: minor, opening: false });
                }
                3 => {
                    // a bill you paid and share: the whole bill left the account
                    let id = on(&w.conn, account, w.food, "Dinner", &amount, &date);
                    if minor >= 2 {
                        set_split(&w.conn, id, &equal(vec![who(friends[0])])).unwrap();
                    }
                    moves.push(Move { account, day: date, delta: -minor, opening: false });
                }
                4 => {
                    // a bill a friend paid: nothing left the account
                    let id = on(&w.conn, account, w.food, "Their treat", &amount, &date);
                    if minor >= 2 {
                        set_split(&w.conn, id, &paid_by(friends[1], equal(vec![who(friends[1])]))).unwrap();
                    } else {
                        moves.push(Move { account, day: date, delta: -minor, opening: false });
                    }
                }
                5 | 6 => {
                    let mut to = accounts[rng.between(0, 3) as usize];
                    if to == account {
                        to = accounts[(accounts.iter().position(|a| *a == account).unwrap() + 1) % 4];
                    }
                    *rows.entry(to).or_default() += 1;
                    transfers::create(&w.conn, &new_transfer(account, to, &amount, &date)).unwrap();
                    moves.push(Move { account, day: date.clone(), delta: -minor, opening: false });
                    moves.push(Move { account: to, day: date, delta: minor, opening: false });
                }
                _ => {
                    let kind = [ledger::Kind::Lent, ledger::Kind::Borrowed, ledger::Kind::RepaidToMe, ledger::Kind::RepaidByMe][rng.between(0, 3) as usize];
                    entry(&w.conn, account, friends[(step % 2) as usize], kind, &amount, &date);
                    let sign = if matches!(kind, ledger::Kind::Borrowed | ledger::Kind::RepaidToMe) { 1 } else { -1 };
                    moves.push(Move { account, day: date, delta: sign * minor, opening: false });
                }
            }
        }
        // archiving changes no number
        super::archive(&w.conn, savings, TODAY).unwrap();

        // the balance on any day
        for probe in ["2025-10-31", "2025-12-31", "2026-01-01", "2026-03-14", "2026-03-15", "2026-06-30", TODAY, "2026-12-24", "2027-12-31"] {
            for account in accounts {
                assert_eq!(balance(&w.conn, account, probe), model_balance(&moves, account, probe), "seed {seed}, account {account} on {probe}");
            }
        }
        // what the list says of each account today
        let listed = list(&w.conn, TODAY).unwrap();
        assert_eq!(listed.iter().map(|a| a.id).collect::<Vec<_>>(), [MAIN_ID, card, later, savings], "seed {seed}: the archived one is last");
        for a in &listed {
            let mine = || moves.iter().filter(|m| m.account == a.id);
            assert_eq!(a.balance, format_amount(model_balance(&moves, a.id, TODAY)), "seed {seed}, balance of {}", a.name);
            let month: i64 = mine().filter(|m| m.day.as_str() >= "2026-11-01" && m.day.as_str() <= TODAY).map(|m| m.delta).sum();
            assert_eq!(a.change_this_month, format_amount(month), "seed {seed}, this month of {}", a.name);
            // a bill a friend paid is no activity of the account: it is not in `moves` either
            let last = mine().filter(|m| !m.opening && m.day.as_str() <= TODAY).map(|m| m.day.clone()).max();
            assert_eq!(a.last_activity, last, "seed {seed}, last activity of {}", a.name);
            assert_eq!(a.entries, rows.get(&a.id).copied().unwrap_or(0), "seed {seed}, rows of {}", a.name);
            assert_eq!(get(&w.conn, a.id, TODAY).unwrap(), *a, "seed {seed}: one account is what the list says of it");
        }
        // net worth: every account today, the archived one too, plus what friends owe
        let total: i64 = accounts.iter().map(|a| model_balance(&moves, *a, TODAY)).sum();
        let owed: i64 = people::list(&w.conn, true).unwrap().iter().map(|p| p.balance_minor).sum();
        let s = summary(&w.conn, TODAY).unwrap();
        assert_eq!((s.in_accounts, s.friends, s.net_worth, s.accounts, s.in_use), (format_amount(total), format_amount(owed), format_amount(total + owed), 4, true), "seed {seed}");
        // the transfers of all accounts together come to nothing
        assert_eq!(one(&w.conn, "SELECT COALESCE(SUM(delta_minor), 0) FROM account_movements WHERE source IN ('TRANSFER_IN', 'TRANSFER_OUT')"), 0, "seed {seed}");

        // the statement of any period: what was there before, plus the lines, is what is there after
        for account in accounts {
            for _ in 0..6 {
                let (a, b) = (day(&mut rng), day(&mut rng));
                let (from, to) = if a <= b { (a, b) } else { (b, a) };
                let st = statement(&w.conn, account, &from, &to, TODAY).unwrap();
                let before = NaiveDate::parse_from_str(&from, "%Y-%m-%d").unwrap().pred_opt().unwrap().to_string();
                assert_eq!(st.balance_before, format_amount(model_balance(&moves, account, &before)), "seed {seed}, {account}, before {from}");
                assert_eq!(st.balance_after, format_amount(model_balance(&moves, account, &to)), "seed {seed}, {account}, after {to}");
                let expected = moves.iter().filter(|m| m.account == account && m.day >= from && m.day <= to).count();
                assert_eq!(st.lines.len(), expected, "seed {seed}, {account}, {from} to {to}");
                let mut running = crate::money::parse_signed_amount(&st.balance_before).unwrap();
                for line in &st.lines {
                    running += crate::money::parse_signed_amount(&line.amount).unwrap();
                    assert_eq!(line.balance, format_amount(running), "seed {seed}: the balance runs from the one before");
                    assert!(line.date >= from && line.date <= to);
                }
                assert_eq!(st.balance_after, format_amount(running), "seed {seed}: before + lines = after");
                assert!(st.lines.windows(2).all(|pair| pair[0].date <= pair[1].date), "seed {seed}: oldest first");
            }
            // the chart of each year
            let months = |year: i32, last: u32| -> Vec<HistoryPoint> {
                (1..=last)
                    .map(|m| {
                        let next = if m == 12 { NaiveDate::from_ymd_opt(year + 1, 1, 1) } else { NaiveDate::from_ymd_opt(year, m + 1, 1) };
                        let end = next.unwrap().pred_opt().unwrap().to_string();
                        let end = if end.as_str() > TODAY { TODAY.to_string() } else { end };
                        HistoryPoint { month: format!("{year}-{m:02}"), balance: format_amount(model_balance(&moves, account, &end)) }
                    })
                    .collect()
            };
            assert_eq!(history(&w.conn, account, "2025", TODAY).unwrap(), months(2025, 12), "seed {seed}");
            assert_eq!(history(&w.conn, account, "2026", TODAY).unwrap(), months(2026, 11), "seed {seed}: up to this month, which ends today");
            assert_eq!(history(&w.conn, account, "2027", TODAY).unwrap(), [], "seed {seed}: no point for a month that has not begun");
        }
    }
}

// ---- the opening balance -----------------------------------------------------------------------

/// 5.5 item 17.
#[test]
fn the_opening_balance_counts_from_its_date_and_rows_before_it_still_count() {
    let w = world();
    let cash = account(&w.conn, "Cash");
    on(&w.conn, cash, w.salary, "Pay", "100.00", "2026-03-10");
    on(&w.conn, cash, w.food, "Shop", "30.00", "2026-06-10");
    let open = |date: &str| update(&w.conn, cash, &AccountPatch { opening_balance: Some("500.00".into()), opening_date: Some(date.into()), ..Default::default() }, TODAY).unwrap();
    let at = |days: [&str; 5]| days.map(|d| balance(&w.conn, cash, d));
    let days = ["2026-01-31", "2026-03-10", "2026-05-01", "2026-06-10", "2026-12-31"];

    // before the rows: it is there from the start
    assert_eq!(open("2026-01-01").balance, "570.00");
    assert_eq!(at(days), [50_000, 60_000, 60_000, 57_000, 57_000]);
    // after some rows: the rows before it still count, and the balance before the date simply does not hold it yet
    assert_eq!(open("2026-05-01").balance, "570.00");
    assert_eq!(at(days), [0, 10_000, 60_000, 57_000, 57_000]);
    assert_eq!([balance(&w.conn, cash, "2026-04-30"), balance(&w.conn, cash, "2026-05-01")], [10_000, 60_000], "the day before and the day of");
    // after every row
    assert_eq!(open("2026-07-01").balance, "570.00");
    assert_eq!(at(days), [0, 10_000, 10_000, 7000, 57_000]);
    // in the future: not in today's balance, in the balance of its day; the chart shows it missing
    let future = open("2026-12-01");
    assert_eq!((future.balance.as_str(), future.opening_balance.as_str(), future.opening_date.as_str()), ("70.00", "500.00", "2026-12-01"));
    assert_eq!([balance(&w.conn, cash, TODAY), balance(&w.conn, cash, "2026-11-30"), balance(&w.conn, cash, "2026-12-01")], [7000, 7000, 57_000]);
    assert_eq!(history(&w.conn, cash, "2026", TODAY).unwrap().last().unwrap().balance, "70.00");
    assert_eq!(summary(&w.conn, TODAY).unwrap().in_accounts, "70.00");
    // the statement shows it as the first line of its day
    let december = statement(&w.conn, cash, "2026-12-01", "2026-12-31", TODAY).unwrap();
    assert_eq!((december.balance_before.as_str(), december.balance_after.as_str()), ("70.00", "570.00"));
    assert_eq!(
        december.lines,
        [Line { date: "2026-12-01".into(), source: "OPENING".into(), id: cash.to_string(), what: String::new(), other: None, amount: "500.00".into(), balance: "570.00".into() }]
    );
    // an opening balance of 0 is no line at all, and a negative one takes away
    update(&w.conn, cash, &AccountPatch { opening_balance: Some("0.00".into()), ..Default::default() }, TODAY).unwrap();
    assert!(statement(&w.conn, cash, "2026-12-01", "2026-12-31", TODAY).unwrap().lines.is_empty());
    assert_eq!(update(&w.conn, cash, &AccountPatch { opening_balance: Some("-500.00".into()), opening_date: Some("2026-01-01".into()), ..Default::default() }, TODAY).unwrap().balance, "-430.00");
}

// ---- rows in the future ------------------------------------------------------------------------

/// 5.5 item 18.
#[test]
fn a_transaction_and_a_transfer_dated_after_today_count_from_their_day() {
    let w = world();
    let cash = account(&w.conn, "Cash");
    on(&w.conn, MAIN_ID, w.salary, "Pay", "1000.00", "2026-11-01");
    on(&w.conn, MAIN_ID, w.food, "Party", "200.00", "2026-11-20");
    transfer(&w.conn, MAIN_ID, cash, "300.00", "2026-11-25");
    let main = get(&w.conn, MAIN_ID, TODAY).unwrap();
    assert_eq!((main.balance.as_str(), main.change_this_month.as_str(), main.last_activity.as_deref(), main.entries), ("1000.00", "1000.00", Some("2026-11-01"), 3));
    let pocket = get(&w.conn, cash, TODAY).unwrap();
    assert_eq!((pocket.balance.as_str(), pocket.change_this_month.as_str(), pocket.last_activity, pocket.entries), ("0.00", "0.00", None, 1));
    assert_eq!(summary(&w.conn, TODAY).unwrap().net_worth, "1000.00");
    assert_eq!([MAIN_ID, cash].map(|a| balance(&w.conn, a, "2026-11-19")), [100_000, 0]);
    assert_eq!([MAIN_ID, cash].map(|a| balance(&w.conn, a, "2026-11-20")), [80_000, 0]);
    assert_eq!([MAIN_ID, cash].map(|a| balance(&w.conn, a, "2026-11-25")), [50_000, 30_000]);
    // on the day itself they are in
    let later = get(&w.conn, MAIN_ID, "2026-11-25").unwrap();
    assert_eq!((later.balance.as_str(), later.change_this_month.as_str(), later.last_activity.as_deref()), ("500.00", "500.00", Some("2026-11-25")));
    assert_eq!(get(&w.conn, cash, "2026-11-25").unwrap().balance, "300.00");
    // a statement of a period that reaches further shows them; the chart stops at today
    let month = statement(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30", TODAY).unwrap();
    assert_eq!((month.lines.len(), month.balance_after.as_str(), month.account.balance.as_str()), (3, "500.00", "1000.00"));
    assert_eq!(history(&w.conn, MAIN_ID, "2026", TODAY).unwrap().last().unwrap(), &HistoryPoint { month: "2026-11".into(), balance: "1000.00".into() });
    assert_eq!(history(&w.conn, MAIN_ID, "2026", "2026-12-05").unwrap()[10..], [HistoryPoint { month: "2026-11".into(), balance: "500.00".into() }, HistoryPoint { month: "2026-12".into(), balance: "500.00".into() }]);
    // the next month, nothing of November is "this month"
    assert_eq!(get(&w.conn, MAIN_ID, "2026-12-01").unwrap().change_this_month, "0.00");
}

// ---- shared bills and the ledger ---------------------------------------------------------------

/// 5.5 item 19.
#[test]
fn an_account_sees_the_money_that_moved_not_your_part_of_a_bill() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let cash = account(&w.conn, "Cash");
    let balances = || [MAIN_ID, cash].map(|a| balance(&w.conn, a, TODAY));

    // a bill of 90.00 you paid, shared in three: the whole bill left the account, your part is 30.00
    let dinner = on(&w.conn, MAIN_ID, w.food, "Dinner", "90.00", "2026-11-02");
    set_split(&w.conn, dinner, &equal(vec![who(anna), who(bob)])).unwrap();
    assert_eq!(balances(), [-9000, 0]);
    assert_eq!(transactions::get(&w.conn, dinner).unwrap().my_share, "30.00", "budgets and statistics still count your part");
    // Anna pays her part back in cash: another account than the bill
    let back = ledger::settle(&w.conn, anna, None, Some("2026-11-03"), None, None, Some(&cash.to_string()), TODAY).unwrap();
    assert_eq!((back.entry.kind, back.entry.account_id, back.entry.amount.as_str()), (ledger::Kind::RepaidToMe, cash, "30.00"));
    assert_eq!(balances(), [-9000, 3000]);

    // a bill of 40.00 a friend paid: nothing left your account, whatever account the row is in
    let lunch = on(&w.conn, cash, w.food, "Lunch", "40.00", "2026-11-04");
    assert_eq!(balances(), [-9000, -1000], "until it is known that Bob paid");
    set_split(&w.conn, lunch, &paid_by(bob, equal(vec![who(bob)]))).unwrap();
    assert_eq!(balances(), [-9000, 3000]);
    assert_eq!(get(&w.conn, cash, TODAY).unwrap().entries, 2, "it still has an account");
    assert_eq!(get(&w.conn, cash, TODAY).unwrap().last_activity.as_deref(), Some("2026-11-03"), "and is no activity of it");
    // paying Bob back is money that leaves: Bob is owed 20.00 for lunch and owes 30.00 for dinner, so he pays 10.00 in
    let settled = ledger::settle(&w.conn, bob, None, Some("2026-11-05"), None, None, None, TODAY).unwrap();
    assert_eq!((settled.entry.kind, settled.entry.amount.as_str()), (ledger::Kind::RepaidToMe, "10.00"));
    assert_eq!(balances(), [-8000, 3000]);

    // "this was my share of a bill": an expense and a borrowing in one account, which together move nothing
    let share = ledger::borrow_with_expense(
        &w.conn,
        &NewEntry { person_id: anna.to_string(), kind: ledger::Kind::Borrowed, amount: "25.00".into(), date: "2026-11-06".into(), note: None, transaction_id: None, account_id: Some(cash.to_string()) },
        &Expense { name: "Cinema".into(), category_id: w.food.to_string() },
    )
    .unwrap();
    assert_eq!(balances(), [-8000, 3000], "the friend paid");
    let cinema = statement(&w.conn, cash, "2026-11-06", "2026-11-06", TODAY).unwrap();
    assert_eq!(cinema.lines.iter().map(|l| (l.source.as_str(), l.amount.as_str(), l.balance.as_str())).collect::<Vec<_>>(), [("TRANSACTION", "-25.00", "5.00"), ("LEDGER", "25.00", "30.00")]);
    // paying Anna back later is the money that really leaves
    ledger::add(&w.conn, &NewEntry { person_id: anna.to_string(), kind: ledger::Kind::RepaidByMe, amount: "25.00".into(), date: "2026-11-07".into(), note: None, transaction_id: None, account_id: None }).unwrap();
    assert_eq!(balances(), [-10_500, 3000]);
    assert_eq!(share.account_id, cash);

    // lending takes money out, borrowing brings it in
    entry(&w.conn, cash, anna, ledger::Kind::Lent, "10.00", "2026-11-08");
    entry(&w.conn, cash, bob, ledger::Kind::Borrowed, "4.00", "2026-11-08");
    assert_eq!(balances(), [-10_500, 2400]);
    // net worth: the accounts, plus what friends owe (Anna 10.00) minus what you owe (Bob 4.00)
    let s = summary(&w.conn, TODAY).unwrap();
    assert_eq!((s.in_accounts.as_str(), s.friends.as_str(), s.net_worth.as_str()), ("-81.00", "6.00", "-75.00"));
}

// ---- the statement, the chart, the summary -----------------------------------------------------

#[test]
fn a_statement_names_each_line_and_orders_a_day_by_source_then_id() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let cash = opened(&w.conn, "Cash", "50.00", "2026-11-02");
    let repaid = entry(&w.conn, cash, anna, ledger::Kind::RepaidToMe, "5.00", "2026-11-02");
    let out = transfers::create(&w.conn, &transfers::NewTransfer { note: Some("to the bank".into()), ..new_transfer(cash, MAIN_ID, "20.00", "2026-11-02") }).unwrap().id;
    let into = transfer(&w.conn, MAIN_ID, cash, "7.00", "2026-11-02");
    let second = on(&w.conn, cash, w.food, "Bakery", "3.00", "2026-11-02");
    let pay = on(&w.conn, cash, w.salary, "Tips", "12.00", "2026-11-02");
    let lent = ledger::add(&w.conn, &NewEntry { person_id: anna.to_string(), kind: ledger::Kind::Lent, amount: "1.00".into(), date: "2026-11-01".into(), note: Some("bus".into()), transaction_id: None, account_id: Some(cash.to_string()) })
        .unwrap()
        .id;
    on(&w.conn, cash, w.food, "Before", "2.00", "2026-10-31");
    on(&w.conn, cash, w.food, "After", "9.00", "2026-11-04");
    on(&w.conn, MAIN_ID, w.food, "Elsewhere", "99.00", "2026-11-02");

    let st = statement(&w.conn, cash, "2026-11-01", "2026-11-03", TODAY).unwrap();
    let line = |date: &str, source: &str, id: i64, what: &str, other: Option<&str>, amount: &str, balance: &str| Line {
        date: date.into(),
        source: source.into(),
        id: id.to_string(),
        what: what.into(),
        other: other.map(str::to_string),
        amount: amount.into(),
        balance: balance.into(),
    };
    assert_eq!((st.from.as_str(), st.to.as_str(), st.balance_before.as_str(), st.balance_after.as_str()), ("2026-11-01", "2026-11-03", "-2.00", "48.00"));
    assert_eq!(
        st.lines,
        [
            line("2026-11-01", "LEDGER", lent, "bus", Some("Anna"), "-1.00", "-3.00"),
            line("2026-11-02", "OPENING", cash, "", None, "50.00", "47.00"),
            line("2026-11-02", "TRANSACTION", second, "Bakery", Some("Food"), "-3.00", "44.00"),
            line("2026-11-02", "TRANSACTION", pay, "Tips", Some("Salary"), "12.00", "56.00"),
            line("2026-11-02", "TRANSFER_IN", into, "", Some(MAIN_NAME), "7.00", "63.00"),
            line("2026-11-02", "TRANSFER_OUT", out, "to the bank", Some(MAIN_NAME), "-20.00", "43.00"),
            line("2026-11-02", "LEDGER", repaid, "", Some("Anna"), "5.00", "48.00"),
        ]
    );
    assert_eq!(st.account, get(&w.conn, cash, TODAY).unwrap());
    // the other side of the same transfers, from the main account
    let main = statement(&w.conn, MAIN_ID, "2026-11-02", "2026-11-02", TODAY).unwrap();
    assert_eq!(
        main.lines.iter().map(|l| (l.source.as_str(), l.other.as_deref(), l.amount.as_str())).collect::<Vec<_>>(),
        [("TRANSACTION", Some("Food"), "-99.00"), ("TRANSFER_IN", Some("Cash"), "20.00"), ("TRANSFER_OUT", Some("Cash"), "-7.00")]
    );
    // one day, an empty period, and the JSON the page gets
    assert_eq!(statement(&w.conn, cash, "2026-11-03", "2026-11-03", TODAY).unwrap().lines, []);
    let json = serde_json::to_value(&st).unwrap();
    assert_eq!(json["balanceBefore"], "-2.00");
    assert_eq!(json["lines"][4], serde_json::json!({ "date": "2026-11-02", "source": "TRANSFER_IN", "id": into.to_string(), "what": "", "other": MAIN_NAME, "amount": "7.00", "balance": "63.00" }));
    assert_eq!(json["account"]["openingBalance"], "50.00");

    for (from, to) in [("2026-11-03", "2026-11-01"), ("2026-11-1", "2026-11-03"), ("2026-11-01", "2026-11-31"), ("", "2026-11-03")] {
        assert_refused(&w.conn, INVALID, &format!("{from} to {to}"), || statement(&w.conn, cash, from, to, TODAY));
    }
    assert_nothing_written(&w.conn, "reading a statement", || statement(&w.conn, cash, "2026-01-01", "2026-12-31", TODAY));
}

#[test]
fn the_chart_has_the_balance_at_the_end_of_each_month_up_to_today() {
    let w = world();
    let cash = opened(&w.conn, "Cash", "100.00", "2026-02-28");
    on(&w.conn, cash, w.food, "Shop", "10.00", "2026-03-31");
    on(&w.conn, cash, w.food, "Shop", "5.00", "2026-04-01");
    on(&w.conn, cash, w.salary, "Pay", "50.00", "2026-11-13");
    on(&w.conn, cash, w.food, "Later", "1.00", "2026-11-14");
    let points = |year: &str, today: &str| -> Vec<(String, String)> { history(&w.conn, cash, year, today).unwrap().into_iter().map(|p| (p.month, p.balance)).collect() };
    let expected = [
        ("2026-01", "0.00"), ("2026-02", "100.00"), ("2026-03", "90.00"), ("2026-04", "85.00"), ("2026-05", "85.00"), ("2026-06", "85.00"), ("2026-07", "85.00"), ("2026-08", "85.00"),
        ("2026-09", "85.00"), ("2026-10", "85.00"), ("2026-11", "135.00"),
    ]
    .map(|(m, b)| (m.to_string(), b.to_string()));
    assert_eq!(points("2026", TODAY), expected, "the current month is the balance of today");
    assert_eq!(points("2026", "2026-01-01"), [("2026-01".to_string(), "0.00".to_string())], "on the first day of a year there is one point");
    assert_eq!(points("2026", "2027-03-01").len(), 12);
    assert_eq!(points("2026", "2027-03-01")[11], ("2026-12".to_string(), "134.00".to_string()));
    assert_eq!(points("2027", TODAY), []);
    assert_eq!(points("2025", TODAY), (1..=12).map(|m| (format!("2025-{m:02}"), "0.00".to_string())).collect::<Vec<_>>());
    assert_eq!(serde_json::to_value(&history(&w.conn, cash, "2026", TODAY).unwrap()[1]).unwrap(), serde_json::json!({ "month": "2026-02", "balance": "100.00" }));
    for bad in ["26", "abcd", "", "2026-01", "20261", "-026"] {
        assert_refused(&w.conn, INVALID, bad, || history(&w.conn, cash, bad, TODAY));
    }
    assert_refused(&w.conn, INVALID, "a today that is no date", || history(&w.conn, cash, "2026", "now"));
}

#[test]
fn net_worth_is_every_account_today_plus_what_friends_owe_and_accounts_are_in_use_once_there_is_more_than_the_untouched_main_one() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let s = summary(&w.conn, TODAY).unwrap();
    assert_eq!((s.in_accounts.as_str(), s.friends.as_str(), s.net_worth.as_str(), s.accounts, s.in_use), ("0.00", "0.00", "0.00", 1, false));
    assert_eq!(serde_json::to_value(&s).unwrap(), serde_json::json!({ "inAccounts": "0.00", "friends": "0.00", "netWorth": "0.00", "accounts": 1, "inUse": false }));

    // a person who never looks at accounts: everything is in the main account, and accounts are still not "in use"
    on(&w.conn, MAIN_ID, w.salary, "Pay", "1000.00", "2026-11-01");
    entry(&w.conn, MAIN_ID, anna, ledger::Kind::Lent, "40.00", "2026-11-02");
    let s = summary(&w.conn, TODAY).unwrap();
    assert_eq!((s.in_accounts.as_str(), s.friends.as_str(), s.net_worth.as_str(), s.accounts, s.in_use), ("960.00", "40.00", "1000.00", 1, false), "lending moves money to a friend: net worth stays");
    update(&w.conn, MAIN_ID, &AccountPatch { name: Some("Everyday".into()), kind: Some(Kind::Cash), ..Default::default() }, TODAY).unwrap();
    assert!(!summary(&w.conn, TODAY).unwrap().in_use, "a rename is not yet using accounts");

    // an opening balance on the main account, or a second account, is
    update(&w.conn, MAIN_ID, &AccountPatch { opening_balance: Some("-0.01".into()), ..Default::default() }, TODAY).unwrap();
    assert!(summary(&w.conn, TODAY).unwrap().in_use);
    update(&w.conn, MAIN_ID, &AccountPatch { opening_balance: Some("0.00".into()), ..Default::default() }, TODAY).unwrap();
    assert!(!summary(&w.conn, TODAY).unwrap().in_use);
    let savings = opened(&w.conn, "Savings", "5000.00", "2026-01-01");
    let s = summary(&w.conn, TODAY).unwrap();
    assert_eq!((s.in_accounts.as_str(), s.net_worth.as_str(), s.accounts, s.in_use), ("5960.00", "6000.00", 2, true));
    // archived, it still counts; a transfer changes nothing; a future row is not in yet
    transfer(&w.conn, savings, MAIN_ID, "500.00", "2026-11-03");
    super::archive(&w.conn, savings, TODAY).unwrap();
    on(&w.conn, MAIN_ID, w.food, "Later", "60.00", "2026-11-14");
    let s = summary(&w.conn, TODAY).unwrap();
    assert_eq!((s.in_accounts.as_str(), s.net_worth.as_str(), s.accounts, s.in_use), ("5960.00", "6000.00", 2, true));
    assert_eq!(summary(&w.conn, "2026-11-14").unwrap().net_worth, "5940.00");
    assert_refused(&w.conn, INVALID, "a today that is no date", || summary(&w.conn, "now"));
    assert_nothing_written(&w.conn, "reading the summary and the list", || summary(&w.conn, TODAY).and(list(&w.conn, TODAY)));
}

// ---- the virtual balance and what is available (section 20.3) ----------------------------------

fn limit_of(conn: &Connection, name: &str, opening: &str, virtual_balance: &str) -> i64 {
    create(conn, &NewAccount { opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), virtual_balance: Some(virtual_balance.into()), ..new_account(name) }, TODAY).unwrap().id
}

fn set_virtual(conn: &Connection, id: i64, amount: &str) -> super::Account {
    update(conn, id, &AccountPatch { virtual_balance: Some(amount.into()), ..Default::default() }, TODAY).unwrap()
}

fn count_in_net_worth(conn: &Connection, id: i64, counts: bool) {
    update(conn, id, &AccountPatch { include_in_net_worth: Some(counts), ..Default::default() }, TODAY).unwrap();
}

#[test]
fn available_is_the_balance_plus_the_virtual_balance_for_every_sign_of_either() {
    let w = world();
    for (n, (balance, virtual_balance, available)) in [
        ("100.00", "50.00", "150.00"),
        ("100.00", "0.00", "100.00"),
        ("100.00", "-30.00", "70.00"),
        ("100.00", "-100.00", "0.00"),
        ("100.00", "-130.00", "-30.00"),
        ("0.00", "0.00", "0.00"),
        ("0.00", "1000.00", "1000.00"),
        ("-240.00", "1000.00", "760.00"),
        ("-240.00", "240.00", "0.00"),
        ("-240.00", "100.00", "-140.00"),
        ("-240.00", "-10.00", "-250.00"),
        ("-0.01", "0.01", "0.00"),
        ("9999999999.99", "9999999999.99", "19999999999.98"),
        ("-9999999999.99", "-9999999999.99", "-19999999999.98"),
    ]
    .iter()
    .enumerate()
    {
        let id = limit_of(&w.conn, &format!("A{n}"), balance, virtual_balance);
        for shown in [get(&w.conn, id, TODAY).unwrap(), list(&w.conn, TODAY).unwrap().into_iter().find(|a| a.id == id).unwrap()] {
            assert_eq!((shown.balance.as_str(), shown.virtual_balance.as_str(), shown.available.as_str()), (*balance, *virtual_balance, *available), "balance {balance} and virtual {virtual_balance}");
        }
    }
    // an account that was given no virtual balance: available is the balance
    let plain = opened(&w.conn, "Plain", "-12.50", "2026-01-01");
    assert_eq!((get(&w.conn, plain, TODAY).unwrap().virtual_balance.as_str(), get(&w.conn, plain, TODAY).unwrap().available.as_str()), ("0.00", "-12.50"));
    // the answer of an update and of the list says the same
    let json = serde_json::to_value(set_virtual(&w.conn, plain, "100.00")).unwrap();
    assert_eq!((json["balance"].as_str(), json["virtualBalance"].as_str(), json["available"].as_str()), (Some("-12.50"), Some("100.00"), Some("87.50")));
}

#[test]
fn a_credit_card_with_a_limit_follows_its_rows_in_available_and_the_future_is_not_in_it() {
    let w = world();
    let card = limit_of(&w.conn, "Card", "-100.00", "1000.00");
    let now = |a: i64| {
        let c = get(&w.conn, a, TODAY).unwrap();
        (c.balance, c.available)
    };
    assert_eq!(now(card), ("-100.00".to_string(), "900.00".to_string()));
    on(&w.conn, card, w.food, "Shop", "140.00", "2026-11-01");
    assert_eq!(now(card), ("-240.00".to_string(), "760.00".to_string()), "balance -240.00 and a limit of 1000.00: 760.00 can still be spent");
    on(&w.conn, card, w.food, "Later", "100.00", "2026-11-20");
    assert_eq!(now(card), ("-240.00".to_string(), "760.00".to_string()), "a row dated after today is in neither");
    on(&w.conn, card, w.salary, "Refund", "40.00", "2026-11-05");
    assert_eq!(now(card), ("-200.00".to_string(), "800.00".to_string()));
    transfer(&w.conn, MAIN_ID, card, "200.00", "2026-11-06");
    assert_eq!(now(card), ("0.00".to_string(), "1000.00".to_string()));
    // the limit changes what is available and nothing else; a negative one is allowed
    assert_eq!(set_virtual(&w.conn, card, "500.00").available, "500.00");
    assert_eq!(set_virtual(&w.conn, card, "-50.00").available, "-50.00");
    assert_eq!((set_virtual(&w.conn, card, "0.00").available.as_str(), get(&w.conn, card, TODAY).unwrap().balance.as_str()), ("0.00", "0.00"));
    assert_eq!(get(&w.conn, card, "2026-11-25").unwrap().available, "-100.00", "a later today brings the row of the 20th into both");
}

/// Everything the virtual balance must leave alone, as text: each account's numbers, its statement, its chart, and net worth.
fn numbers(conn: &Connection, accounts: &[i64]) -> Vec<String> {
    let mut out = Vec::new();
    for id in accounts {
        let a = get(conn, *id, TODAY).unwrap();
        out.push(format!("{} {} {:?} {} {} {}", a.balance, a.change_this_month, a.last_activity, a.entries, a.opening_balance, a.include_in_net_worth));
        let s = statement(conn, *id, "2026-01-01", "2026-12-31", TODAY).unwrap();
        out.push(format!("{} {} {:?}", s.balance_before, s.balance_after, s.lines));
        out.push(format!("{:?}", history(conn, *id, "2026", TODAY).unwrap()));
    }
    out.push(format!("{:?}", summary(conn, TODAY).unwrap()));
    out
}

#[test]
fn the_virtual_balance_is_in_no_balance_statement_chart_or_net_worth() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let card = opened(&w.conn, "Card", "-100.00", "2026-01-01");
    let cash = opened(&w.conn, "Cash", "50.00", "2026-02-01");
    on(&w.conn, card, w.food, "Shop", "140.00", "2026-11-01");
    on(&w.conn, MAIN_ID, w.salary, "Pay", "1000.00", "2026-11-02");
    on(&w.conn, cash, w.food, "Later", "5.00", "2026-12-01");
    transfer(&w.conn, MAIN_ID, card, "200.00", "2026-11-06");
    entry(&w.conn, cash, anna, ledger::Kind::Lent, "10.00", "2026-11-07");
    let accounts = [MAIN_ID, card, cash];
    let before = numbers(&w.conn, &accounts);
    for amount in ["1000.00", "-500.00", "9999999999.99", "-9999999999.99", "0.01"] {
        for id in accounts {
            set_virtual(&w.conn, id, amount);
        }
        assert_eq!(numbers(&w.conn, &accounts), before, "virtual balance {amount} on every account");
    }
    // the account inside a statement carries the virtual balance and what is available, and the lines and balances are as before
    let st = statement(&w.conn, card, "2026-11-01", "2026-11-30", TODAY).unwrap();
    assert_eq!((st.account.virtual_balance.as_str(), st.account.available.as_str(), st.account.balance.as_str()), ("0.01", "-39.99", "-40.00"));
    // back to none: the same again
    for id in accounts {
        set_virtual(&w.conn, id, "0.00");
    }
    assert_eq!(numbers(&w.conn, &accounts), before);
    assert_eq!(balance(&w.conn, card, TODAY), -4000);
}

// ---- "count in net worth" ----------------------------------------------------------------------

#[test]
fn net_worth_adds_only_the_accounts_that_count_and_what_friends_owe_is_always_in() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    on(&w.conn, MAIN_ID, w.salary, "Pay", "1000.00", "2026-11-01");
    entry(&w.conn, MAIN_ID, anna, ledger::Kind::Lent, "40.00", "2026-11-02");
    let savings = opened(&w.conn, "Savings", "5000.00", "2026-01-01");
    let cash = opened(&w.conn, "Cash", "200.00", "2026-01-01");
    let card = opened(&w.conn, "Card", "-300.00", "2026-01-01");
    let figures = || {
        let s = summary(&w.conn, TODAY).unwrap();
        (s.in_accounts, s.friends, s.net_worth, s.accounts, s.in_use)
    };
    let expect = |in_accounts: &str, net_worth: &str| (in_accounts.to_string(), "40.00".to_string(), net_worth.to_string(), 4u32, true);
    // main 960.00, savings 5000.00, cash 200.00, card -300.00
    assert_eq!(figures(), expect("5860.00", "5900.00"));

    // one account that does not count
    count_in_net_worth(&w.conn, cash, false);
    assert_eq!(figures(), expect("5660.00", "5700.00"));
    // only the main account left out
    count_in_net_worth(&w.conn, cash, true);
    count_in_net_worth(&w.conn, MAIN_ID, false);
    assert_eq!(figures(), expect("4900.00", "4940.00"), "the main account may be left out like any other");
    // a negative account left out makes net worth larger
    count_in_net_worth(&w.conn, card, false);
    assert_eq!(figures(), expect("5200.00", "5240.00"));
    // every account left out: nothing in accounts, and friends still count
    count_in_net_worth(&w.conn, savings, false);
    count_in_net_worth(&w.conn, cash, false);
    assert_eq!(figures(), expect("0.00", "40.00"), "no account counts: 0.00 in accounts, what friends owe is net worth");
    assert_eq!(serde_json::to_value(summary(&w.conn, TODAY).unwrap()).unwrap(), serde_json::json!({ "inAccounts": "0.00", "friends": "40.00", "netWorth": "40.00", "accounts": 4, "inUse": true }));
    // the balances themselves did not change, whatever counts
    assert_eq!([MAIN_ID, savings, cash, card].map(|id| get(&w.conn, id, TODAY).unwrap().balance), ["960.00", "5000.00", "200.00", "-300.00"].map(String::from));
    // switching it back brings the number back
    for id in [MAIN_ID, savings, cash, card] {
        count_in_net_worth(&w.conn, id, true);
    }
    assert_eq!(figures(), expect("5860.00", "5900.00"));
    // turning it on for an account that counts, and off twice, are no changes of the number
    count_in_net_worth(&w.conn, cash, true);
    count_in_net_worth(&w.conn, cash, false);
    count_in_net_worth(&w.conn, cash, false);
    assert_eq!(figures(), expect("5660.00", "5700.00"));
}

#[test]
fn an_archived_account_counts_in_net_worth_when_its_flag_is_on_and_not_when_it_is_off() {
    let w = world();
    let old = opened(&w.conn, "Old", "700.00", "2026-01-01");
    let in_accounts = || summary(&w.conn, TODAY).unwrap().in_accounts;
    assert_eq!(in_accounts(), "700.00");
    // archived with the flag on: still counted, as before
    super::archive(&w.conn, old, TODAY).unwrap();
    assert_eq!(in_accounts(), "700.00");
    // the flag cannot be changed while it is archived; restore, change, archive again
    assert_refused(&w.conn, INVALID, "the flag of an archived account", || update(&w.conn, old, &AccountPatch { include_in_net_worth: Some(false), ..Default::default() }, TODAY));
    super::restore(&w.conn, old, TODAY).unwrap();
    count_in_net_worth(&w.conn, old, false);
    assert_eq!(in_accounts(), "0.00");
    super::archive(&w.conn, old, TODAY).unwrap();
    assert_eq!(in_accounts(), "0.00", "archived and left out");
    assert!(!get(&w.conn, old, TODAY).unwrap().include_in_net_worth, "the flag stays with it");
    assert_eq!(get(&w.conn, old, TODAY).unwrap().balance, "700.00");
    super::restore(&w.conn, old, TODAY).unwrap();
    assert_eq!(in_accounts(), "0.00", "restoring does not turn it on");
    count_in_net_worth(&w.conn, old, true);
    assert_eq!(in_accounts(), "700.00");
}

#[test]
fn the_count_of_accounts_and_in_use_do_not_depend_on_what_counts_in_net_worth() {
    let w = world();
    // only the main account, left out: with an opening balance it is in use; with none it is not
    count_in_net_worth(&w.conn, MAIN_ID, false);
    let s = summary(&w.conn, TODAY).unwrap();
    assert_eq!((s.in_accounts.as_str(), s.net_worth.as_str(), s.accounts, s.in_use), ("0.00", "0.00", 1, false), "an untouched main account is not in use, counted or not");
    update(&w.conn, MAIN_ID, &AccountPatch { opening_balance: Some("100.00".into()), opening_date: Some("2026-01-01".into()), ..Default::default() }, TODAY).unwrap();
    let s = summary(&w.conn, TODAY).unwrap();
    assert_eq!((s.in_accounts.as_str(), s.net_worth.as_str(), s.accounts, s.in_use), ("0.00", "0.00", 1, true), "it holds 100.00 and counts for nothing, but accounts are in use");
    count_in_net_worth(&w.conn, MAIN_ID, true);
    assert_eq!(summary(&w.conn, TODAY).unwrap().in_accounts, "100.00");
    // a second account, left out from the start, makes accounts in use and is counted in `accounts`
    let w = world();
    create(&w.conn, &NewAccount { include_in_net_worth: Some(false), opening_balance: Some("5.00".into()), opening_date: Some("2026-01-01".into()), ..new_account("Off") }, TODAY).unwrap();
    let s = summary(&w.conn, TODAY).unwrap();
    assert_eq!((s.in_accounts.as_str(), s.accounts, s.in_use), ("0.00", 2, true));
    // an account whose rows are all in the future and is left out
    let later = opened(&w.conn, "Later", "10.00", "2027-01-01");
    count_in_net_worth(&w.conn, later, false);
    assert_eq!(summary(&w.conn, "2027-02-01").unwrap().in_accounts, "0.00");
    count_in_net_worth(&w.conn, later, true);
    assert_eq!(summary(&w.conn, "2027-02-01").unwrap().in_accounts, "10.00");
    assert_eq!(summary(&w.conn, TODAY).unwrap().in_accounts, "0.00");
    assert_nothing_written(&w.conn, "reading the summary", || summary(&w.conn, TODAY));
}

#[test]
fn net_worth_and_available_equal_a_plain_sum_for_random_rows_flags_and_virtual_balances() {
    for seed in [3u64, 11, 2026, 424_242] {
        let w = world();
        let mut rng = Rng(seed);
        let ids = [MAIN_ID, account(&w.conn, "A"), opened(&w.conn, "B", "-80.00", "2026-02-01"), account(&w.conn, "C"), opened(&w.conn, "D", "300.00", "2026-12-20")];
        let mut moves = vec![Move { account: ids[2], day: "2026-02-01".into(), delta: -8000, opening: true }, Move { account: ids[4], day: "2026-12-20".into(), delta: 30_000, opening: true }];
        for _ in 0..90 {
            let account = ids[rng.between(0, 4) as usize];
            let (minor, amount) = cents(&mut rng);
            let date = day(&mut rng);
            match rng.between(0, 3) {
                0 => {
                    on(&w.conn, account, w.food, "Shop", &amount, &date);
                    moves.push(Move { account, day: date, delta: -minor, opening: false });
                }
                1 => {
                    on(&w.conn, account, w.salary, "Pay", &amount, &date);
                    moves.push(Move { account, day: date, delta: minor, opening: false });
                }
                _ => {
                    let to = ids[(ids.iter().position(|a| *a == account).unwrap() + 1 + rng.between(0, 3) as usize) % 5];
                    transfers::create(&w.conn, &new_transfer(account, to, &amount, &date)).unwrap();
                    moves.push(Move { account, day: date.clone(), delta: -minor, opening: false });
                    moves.push(Move { account: to, day: date, delta: minor, opening: false });
                }
            }
        }
        for round in 0..6 {
            let mut counts = Vec::new();
            let mut virtual_minor = Vec::new();
            for id in ids {
                let on = round == 0 || rng.between(0, 2) > 0;
                let limit = rng.between(0, 600_000) as i64 - 300_000;
                update(&w.conn, id, &AccountPatch { include_in_net_worth: Some(on), virtual_balance: Some(format_amount(limit)), ..Default::default() }, TODAY).unwrap();
                counts.push(on);
                virtual_minor.push(limit);
            }
            let held: Vec<i64> = ids.iter().map(|id| model_balance(&moves, *id, TODAY)).collect();
            let expected: i64 = held.iter().zip(&counts).filter(|(_, on)| **on).map(|(b, _)| *b).sum();
            let s = summary(&w.conn, TODAY).unwrap();
            assert_eq!((s.in_accounts.as_str(), s.net_worth.as_str(), s.friends.as_str(), s.accounts), (format_amount(expected).as_str(), format_amount(expected).as_str(), "0.00", 5), "seed {seed} round {round}");
            for (n, id) in ids.iter().enumerate() {
                let a = get(&w.conn, *id, TODAY).unwrap();
                assert_eq!((a.balance, a.available, a.include_in_net_worth), (format_amount(held[n]), format_amount(held[n] + virtual_minor[n]), counts[n]), "seed {seed} round {round} account {id}");
            }
        }
    }
}

// ---- what an account paid and took in, by category (section 20.4) ------------------------------

fn expense_category(conn: &Connection, name: &str) -> i64 {
    categories::create(conn, name, CategoryType::Expense).unwrap().id
}

fn income_category(conn: &Connection, name: &str) -> i64 {
    categories::create(conn, name, CategoryType::Income).unwrap().id
}

/// (name, amount) of each share, in order.
fn shares(list: &[Share]) -> Vec<(&str, &str)> {
    list.iter().map(|s| (s.name.as_str(), s.amount.as_str())).collect()
}

const NOT_FOUND: crate::vault::AppError = crate::vault::AppError::NotFound;

#[test]
fn the_breakdown_of_an_empty_period_has_empty_lists_and_totals_of_zero() {
    let w = world();
    let b = breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!(b, Breakdown { currency: "EUR".into(), from: "2026-11-01".into(), to: "2026-11-30".into(), expenses: vec![], income: vec![], expense_total: "0.00".into(), income_total: "0.00".into() });
    assert_eq!(serde_json::to_value(&b).unwrap(), serde_json::json!({ "currency": "EUR", "from": "2026-11-01", "to": "2026-11-30", "expenses": [], "income": [], "expenseTotal": "0.00", "incomeTotal": "0.00" }));
    // rows outside the period make it no less empty, and a one-day period is a period
    on(&w.conn, MAIN_ID, w.food, "Shop", "9.00", "2026-10-31");
    on(&w.conn, MAIN_ID, w.salary, "Pay", "9.00", "2026-12-01");
    assert_eq!(breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap(), b);
    assert_eq!(breakdown(&w.conn, MAIN_ID, "2026-11-13", "2026-11-13").unwrap().expense_total, "0.00");
    assert_nothing_written(&w.conn, "reading a breakdown", || breakdown(&w.conn, MAIN_ID, "2026-01-01", "2026-12-31"));
}

#[test]
fn transfers_ledger_entries_and_opening_balances_are_not_in_a_breakdown() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let cash = opened(&w.conn, "Cash", "500.00", "2026-11-02");
    transfer(&w.conn, MAIN_ID, cash, "200.00", "2026-11-03");
    transfer(&w.conn, cash, MAIN_ID, "10.00", "2026-11-04");
    entry(&w.conn, cash, anna, ledger::Kind::Lent, "20.00", "2026-11-05");
    entry(&w.conn, cash, anna, ledger::Kind::Borrowed, "30.00", "2026-11-05");
    entry(&w.conn, MAIN_ID, anna, ledger::Kind::RepaidToMe, "5.00", "2026-11-06");
    entry(&w.conn, MAIN_ID, anna, ledger::Kind::RepaidByMe, "6.00", "2026-11-06");
    for account in [MAIN_ID, cash] {
        let b = breakdown(&w.conn, account, "2026-11-01", "2026-11-30").unwrap();
        assert_eq!((b.expenses, b.income, b.expense_total.as_str(), b.income_total.as_str()), (vec![], vec![], "0.00", "0.00"), "account {account}: only transfers, entries and an opening balance moved it");
    }
    // one transaction among them is the only thing in it
    on(&w.conn, cash, w.food, "Shop", "12.50", "2026-11-07");
    let b = breakdown(&w.conn, cash, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!((shares(&b.expenses), b.expense_total.as_str(), b.income.len()), (vec![("Food", "12.50")], "12.50", 0));
}

#[test]
fn a_bill_you_paid_counts_whole_and_a_bill_a_friend_paid_not_at_all() {
    let w = world();
    let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
    let cash = account(&w.conn, "Cash");
    // 90.00 you paid, shared in three: the account paid all of it, whatever your part is
    let dinner = on(&w.conn, MAIN_ID, w.food, "Dinner", "90.00", "2026-11-02");
    set_split(&w.conn, dinner, &equal(vec![who(anna), who(bob)])).unwrap();
    // 40.00 Bob paid: nothing left the account
    let lunch = on(&w.conn, MAIN_ID, w.food, "Lunch", "40.00", "2026-11-04");
    set_split(&w.conn, lunch, &paid_by(bob, equal(vec![who(bob)]))).unwrap();
    // the same in another category, so that a bill nobody paid for the account cannot hide in a shared total
    let rent = on(&w.conn, MAIN_ID, w.rent, "Their rent", "500.00", "2026-11-04");
    set_split(&w.conn, rent, &paid_by(anna, equal(vec![who(anna)]))).unwrap();
    let b = breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!((shares(&b.expenses), b.expense_total.as_str()), (vec![("Food", "90.00")], "90.00"));
    // "my share of a bill": an expense in the account, and the borrowing beside it is not one
    ledger::borrow_with_expense(
        &w.conn,
        &NewEntry { person_id: anna.to_string(), kind: ledger::Kind::Borrowed, amount: "25.00".into(), date: "2026-11-06".into(), note: None, transaction_id: None, account_id: Some(cash.to_string()) },
        &Expense { name: "Cinema".into(), category_id: w.food.to_string() },
    )
    .unwrap();
    let b = breakdown(&w.conn, cash, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!((shares(&b.expenses), b.expense_total.as_str(), b.income_total.as_str()), (vec![("Food", "25.00")], "25.00", "0.00"));
    // the same bill, if its payer is changed back to you, comes in whole
    set_split(&w.conn, lunch, &equal(vec![who(bob)])).unwrap();
    assert_eq!(shares(&breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap().expenses), vec![("Food", "130.00")]);
}

#[test]
fn the_first_and_the_last_day_are_in_the_period_and_the_day_before_and_after_are_not() {
    let w = world();
    for (day, name) in [("2026-10-31", "Before"), ("2026-11-01", "First"), ("2026-11-15", "Middle"), ("2026-11-30", "Last"), ("2026-12-01", "After")] {
        on(&w.conn, MAIN_ID, w.food, name, "1.00", day);
        on(&w.conn, MAIN_ID, w.salary, name, "2.00", day);
    }
    let b = breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!((shares(&b.expenses), shares(&b.income), b.expense_total.as_str(), b.income_total.as_str()), (vec![("Food", "3.00")], vec![("Salary", "6.00")], "3.00", "6.00"), "three days of five");
    for (from, to, expense) in [("2026-11-01", "2026-11-01", "1.00"), ("2026-11-30", "2026-11-30", "1.00"), ("2026-10-31", "2026-11-01", "2.00"), ("2026-11-30", "2026-12-01", "2.00"), ("2026-11-02", "2026-11-29", "1.00"), ("2026-11-16", "2026-11-29", "0.00")] {
        assert_eq!(breakdown(&w.conn, MAIN_ID, from, to).unwrap().expense_total, expense, "{from} to {to}");
    }
}

#[test]
fn a_day_after_today_inside_the_period_is_in_it_and_the_period_decides_not_today() {
    let w = world();
    on(&w.conn, MAIN_ID, w.food, "Soon", "20.00", "2026-11-20");
    on(&w.conn, MAIN_ID, w.salary, "Next month", "100.00", "2026-12-24");
    assert_eq!(balance(&w.conn, MAIN_ID, TODAY), 0, "today's balance has neither");
    let b = breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!((shares(&b.expenses), b.income.len()), (vec![("Food", "20.00")], 0));
    let b = breakdown(&w.conn, MAIN_ID, "2026-12-01", "2027-01-31").unwrap();
    assert_eq!((b.expenses.len(), shares(&b.income)), (0, vec![("Salary", "100.00")]), "a period wholly after today");
    assert_eq!(breakdown(&w.conn, MAIN_ID, "2026-01-01", "2026-11-12").unwrap().expense_total, "0.00", "a period before them");
}

#[test]
fn categories_are_ordered_largest_first_then_by_name_ignoring_case_and_split_by_their_type() {
    let w = world();
    let (zoo, apple, bakery, cherry) = (expense_category(&w.conn, "Zoo"), expense_category(&w.conn, "apple"), expense_category(&w.conn, "Bakery"), expense_category(&w.conn, "cherry"));
    let (tips, bonus, big) = (income_category(&w.conn, "Tips"), income_category(&w.conn, "bonus"), income_category(&w.conn, "Gift"));
    for (category, amount) in [(cherry, "10.00"), (zoo, "20.00"), (bakery, "10.00"), (apple, "10.00"), (w.food, "10.01"), (w.rent, "9.99")] {
        on(&w.conn, MAIN_ID, category, "x", amount, "2026-11-10");
    }
    for (category, amount) in [(tips, "5.00"), (bonus, "5.00"), (big, "7.00"), (w.salary, "6.99")] {
        on(&w.conn, MAIN_ID, category, "x", amount, "2026-11-10");
    }
    // a second row in a category adds to it and can move it up
    on(&w.conn, MAIN_ID, w.rent, "y", "0.02", "2026-11-11");
    let b = breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!(shares(&b.expenses), [("Zoo", "20.00"), ("Food", "10.01"), ("Rent", "10.01"), ("apple", "10.00"), ("Bakery", "10.00"), ("cherry", "10.00")], "equal amounts go by name, upper and lower case alike");
    assert_eq!(shares(&b.income), [("Gift", "7.00"), ("Salary", "6.99"), ("bonus", "5.00"), ("Tips", "5.00")]);
    assert_eq!((b.expense_total.as_str(), b.income_total.as_str()), ("70.02", "23.99"));
    // a category of each type with one name is two shares, one in each list
    let same_expense = expense_category(&w.conn, "Gift");
    on(&w.conn, MAIN_ID, same_expense, "x", "3.00", "2026-11-12");
    let b = breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap();
    assert!(b.expenses.iter().any(|s| s.name == "Gift" && s.amount == "3.00") && b.income.iter().any(|s| s.name == "Gift" && s.amount == "7.00"));
    // the JSON of a share
    assert_eq!(serde_json::to_value(&b.income[0]).unwrap(), serde_json::json!({ "categoryId": big.to_string(), "name": "Gift", "amount": "7.00" }));
}

#[test]
fn a_category_that_was_deleted_still_shows_by_name_and_a_new_one_with_its_name_is_another_share() {
    let mut w = world();
    let games = expense_category(&w.conn, "Games");
    let gift = income_category(&w.conn, "Gift");
    on(&w.conn, MAIN_ID, games, "Chess", "12.00", "2026-11-03");
    on(&w.conn, MAIN_ID, gift, "Card", "30.00", "2026-11-03");
    categories::delete(&mut w.conn, games, chrono::Utc::now()).unwrap();
    categories::delete(&mut w.conn, gift, chrono::Utc::now()).unwrap();
    let b = breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!((shares(&b.expenses), shares(&b.income)), (vec![("Games", "12.00")], vec![("Gift", "30.00")]));
    assert_eq!(b.expenses[0].category_id, games);
    // its name is free again: a new category of that name is a second share, and the older one comes first at an equal amount
    let again = expense_category(&w.conn, "Games");
    on(&w.conn, MAIN_ID, again, "Go", "12.00", "2026-11-04");
    let b = breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!(b.expenses.iter().map(|s| (s.category_id, s.amount.as_str())).collect::<Vec<_>>(), [(games, "12.00"), (again, "12.00")]);
    assert_eq!(b.expense_total, "24.00");
}

#[test]
fn another_accounts_rows_are_out_and_an_archived_account_is_still_read() {
    let w = world();
    let cash = account(&w.conn, "Cash");
    let old = account(&w.conn, "Old");
    on(&w.conn, MAIN_ID, w.food, "Main", "1.00", "2026-11-05");
    on(&w.conn, cash, w.food, "Cash", "20.00", "2026-11-05");
    on(&w.conn, cash, w.salary, "Cash pay", "300.00", "2026-11-05");
    on(&w.conn, old, w.food, "Old", "400.00", "2026-11-05");
    on(&w.conn, old, w.salary, "Old pay", "5000.00", "2026-11-05");
    let of = |id: i64| {
        let b = breakdown(&w.conn, id, "2026-11-01", "2026-11-30").unwrap();
        (b.expense_total, b.income_total)
    };
    assert_eq!([of(MAIN_ID), of(cash), of(old)], [("1.00", "0.00"), ("20.00", "300.00"), ("400.00", "5000.00")].map(|(a, b)| (a.to_string(), b.to_string())));
    super::archive(&w.conn, old, TODAY).unwrap();
    assert_eq!(of(old), ("400.00".to_string(), "5000.00".to_string()), "archiving changes no number");
    // a row moved to another account moves with it
    let moved = on(&w.conn, MAIN_ID, w.food, "Moved", "7.00", "2026-11-06");
    assert_eq!(of(MAIN_ID).0, "8.00");
    crate::transactions::update(&w.conn, moved, &crate::transactions::TransactionPatch { account_id: Some(cash.to_string()), ..Default::default() }).unwrap();
    assert_eq!((of(MAIN_ID).0, of(cash).0), ("1.00".to_string(), "27.00".to_string()));
}

#[test]
fn a_breakdown_of_an_unknown_account_is_not_found_and_bad_dates_are_invalid() {
    let w = world();
    on(&w.conn, MAIN_ID, w.food, "Shop", "1.00", "2026-11-05");
    assert_refused(&w.conn, NOT_FOUND, "an account that is not there", || breakdown(&w.conn, 99, "2026-11-01", "2026-11-30"));
    assert_refused(&w.conn, NOT_FOUND, "id 0", || breakdown(&w.conn, 0, "2026-11-01", "2026-11-30"));
    let cash = account(&w.conn, "Cash");
    super::delete(&w.conn, cash).unwrap();
    assert_refused(&w.conn, NOT_FOUND, "an account that was deleted", || breakdown(&w.conn, cash, "2026-11-01", "2026-11-30"));
    for (from, to) in [("2026-11-30", "2026-11-01"), ("2026-02-30", "2026-03-01"), ("2026-02-01", "2026-02-30"), ("2026-2-1", "2026-11-30"), ("2026-11-01", "2026-11-3"), ("", "2026-11-30"), ("2026-11-01", ""), ("today", "2026-11-30"), ("2026-13-01", "2026-13-02"), ("20261101", "20261130")] {
        assert_refused(&w.conn, INVALID, &format!("{from:?} to {to:?}"), || breakdown(&w.conn, MAIN_ID, from, to));
    }
    // the dates are looked at first, as a statement does
    assert_refused(&w.conn, INVALID, "bad dates for an account that is not there", || breakdown(&w.conn, 99, "2026-11-30", "2026-11-01"));
    // a period of one day, and from = to, is fine
    assert_eq!(breakdown(&w.conn, MAIN_ID, "2026-11-05", "2026-11-05").unwrap().expense_total, "1.00");
}

#[test]
fn the_breakdown_equals_a_plain_sum_by_category_for_random_rows_and_periods() {
    struct Row {
        account: i64,
        category: i64,
        minor: i64,
        day: String,
        counted: bool,
    }
    for seed in [5u64, 17, 2026, 31_337] {
        let w = world();
        let mut rng = Rng(seed);
        let (games, bonus) = (expense_category(&w.conn, "games"), income_category(&w.conn, "Bonus"));
        let names = [(w.food, "Food", false), (w.rent, "Rent", false), (games, "games", false), (w.salary, "Salary", true), (bonus, "Bonus", true)];
        let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
        let ids = [MAIN_ID, account(&w.conn, "Savings"), opened(&w.conn, "Card", "-50.00", "2026-03-01")];
        let mut rows = Vec::new();
        for _ in 0..150 {
            let account = ids[rng.between(0, 2) as usize];
            let (category, _, income) = names[rng.between(0, 4) as usize];
            let (mut minor, _) = cents(&mut rng);
            if rng.between(0, 2) == 0 {
                // round amounts make equal totals, so that the order by name is used
                minor = (minor / 5000 + 1) * 5000;
            }
            let date = day(&mut rng);
            let id = on(&w.conn, account, category, "x", &format_amount(minor), &date);
            let mut counted = true;
            if !income && rng.between(0, 4) == 0 {
                set_split(&w.conn, id, &paid_by(bob, equal(vec![who(bob)]))).unwrap();
                counted = false;
            } else if !income && rng.between(0, 4) == 0 {
                set_split(&w.conn, id, &equal(vec![who(anna)])).unwrap();
            }
            rows.push(Row { account, category, minor, day: date, counted });
            if rng.between(0, 3) == 0 {
                let to = ids[(ids.iter().position(|a| *a == account).unwrap() + 1) % 3];
                transfers::create(&w.conn, &new_transfer(account, to, &format_amount(minor), &day(&mut rng))).unwrap();
                entry(&w.conn, account, anna, ledger::Kind::Lent, &format_amount(minor), &day(&mut rng));
            }
        }
        for _ in 0..60 {
            let (mut from, mut to) = (day(&mut rng), day(&mut rng));
            if from > to {
                std::mem::swap(&mut from, &mut to);
            }
            let account = ids[rng.between(0, 2) as usize];
            let mut sums: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
            for row in rows.iter().filter(|r| r.account == account && r.counted && r.day >= from && r.day <= to) {
                *sums.entry(row.category).or_default() += row.minor;
            }
            let mut wanted: Vec<(i64, i64, String, bool)> = sums.iter().map(|(c, m)| (*c, *m, names.iter().find(|n| n.0 == *c).unwrap().1.to_string(), names.iter().find(|n| n.0 == *c).unwrap().2)).collect();
            wanted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.to_lowercase().cmp(&b.2.to_lowercase())).then_with(|| a.0.cmp(&b.0)));
            let share = |income: bool| wanted.iter().filter(|r| r.3 == income).map(|r| Share { category_id: r.0, name: r.2.clone(), amount: format_amount(r.1) }).collect::<Vec<_>>();
            let total = |income: bool| format_amount(wanted.iter().filter(|r| r.3 == income).map(|r| r.1).sum());
            let expected = Breakdown { currency: "EUR".into(), from: from.clone(), to: to.clone(), expenses: share(false), income: share(true), expense_total: total(false), income_total: total(true) };
            let got = breakdown(&w.conn, account, &from, &to).unwrap();
            assert_eq!(got, expected, "seed {seed}, account {account}, {from} to {to}");
            // the totals are the sum of the shares, and every share is a positive amount
            let sum = |list: &[Share]| list.iter().map(|s| crate::money::parse_amount(&s.amount).unwrap()).sum::<i64>();
            assert_eq!((format_amount(sum(&got.expenses)), format_amount(sum(&got.income))), (got.expense_total.clone(), got.income_total.clone()));
            assert!(got.expenses.iter().chain(&got.income).all(|s| s.amount != "0.00" && !s.amount.starts_with('-')));
        }
    }
}

// ---- a currency of its own: the numbers (section 21.3) ---------------------------------------------

/// An account in another currency, opened with `opening` at the start of 2026. Returns its id.
fn lei(conn: &Connection, name: &str, code: &str, rate: &str, opening: &str) -> i64 {
    create(conn, &NewAccount { currency: Some(code.into()), rate: Some(rate.into()), opening_balance: Some(opening.into()), opening_date: Some("2026-01-01".into()), ..new_account(name) }, TODAY).unwrap().id
}

/// A transaction in a foreign account: `amount` in the main currency, `native` in the account's.
fn native_on(conn: &Connection, account: i64, category: i64, name: &str, amount: &str, native: &str, date: &str) -> i64 {
    transactions::create(
        conn,
        &NewTransaction {
            name: name.into(),
            amount: amount.into(),
            payment_date: date.into(),
            category_id: category.to_string(),
            description: None,
            client_uuid: None,
            account_id: Some(account.to_string()),
            native_amount: Some(native.into()),
        },
    )
    .unwrap()
    .id
}

/// `minor / (numerator / denominator)`, half away from zero, in 128 bits: the plain way to work it out.
fn model_to_main(minor: i64, numerator: i128, denominator: i128) -> i128 {
    let size = (i128::from(minor).abs() * denominator * 2 + numerator) / (2 * numerator);
    size * i128::from(minor.signum())
}

#[test]
fn to_main_divides_by_the_rate_and_rounds_half_away_from_zero_for_both_signs() {
    for (minor, rate, expected) in [
        (0, "2", 0),
        (1, "2", 1),
        (-1, "2", -1),
        (2, "2", 1),
        (3, "2", 2),
        (-3, "2", -2),
        (1, "3", 0),
        (2, "3", 1),
        (-2, "3", -1),
        (100, "3", 33),
        (101, "3", 34),
        (-101, "3", -34),
        (4, "10", 0),
        (5, "10", 1),
        (-5, "10", -1),
        (1, "0.5", 2),
        (-1, "0.5", -2),
        (12_345, "5.00", 2469),
        (12_345, "4.9769", 2480),
        (-12_345, "4.9769", -2480),
        (10_000, "1.08", 9259),
        (50, "1", 50),
        (-50, "1.0", -50),
        (1, "999999999.99999999", 0),
        (1, "0.00000001", 100_000_000),
    ] {
        assert_eq!(to_main(minor, rate), Ok(expected), "{minor} at {rate}");
        assert_eq!(to_main(-minor, rate), Ok(-expected), "{} at {rate}: the sign is kept and the rounding does not depend on it", -minor);
    }
    // against the plain way, at the bounds and with rates below and above 1
    for rate in [("1", 1, 1), ("2", 2, 1), ("0.5", 5, 10), ("1.00000001", 100_000_001, 100_000_000), ("0.99999999", 99_999_999, 100_000_000), ("4.9769", 49_769, 10_000), ("160.5", 1605, 10)] {
        for minor in [0, 1, 7, 99, 100, 1234, 500_000_000_000, MAX_MINOR - 1, MAX_MINOR] {
            for minor in [minor, -minor] {
                let wanted = model_to_main(minor, rate.1, rate.2);
                match to_main(minor, rate.0) {
                    Ok(got) => assert_eq!(i128::from(got), wanted, "{minor} at {}", rate.0),
                    Err(e) => assert!(e == INVALID && wanted.abs() > i128::from(MAX_MINOR), "{minor} at {}: {e:?} for {wanted}", rate.0),
                }
            }
        }
    }
    assert_eq!(to_main(MAX_MINOR, "1"), Ok(MAX_MINOR));
    assert_eq!(to_main(-MAX_MINOR, "1"), Ok(-MAX_MINOR));
}

#[test]
fn to_main_refuses_what_leaves_the_range_and_a_bad_rate_and_never_overflows() {
    // twice as much, at a rate of one half: the last amount that fits and the first that does not
    assert_eq!(to_main(499_999_999_999, "0.5"), Ok(999_999_999_998));
    assert_eq!(to_main(-499_999_999_999, "0.5"), Ok(-999_999_999_998));
    assert_eq!(to_main(500_000_000_000, "0.5"), Err(INVALID));
    assert_eq!(to_main(-500_000_000_000, "0.5"), Err(INVALID));
    for minor in [MAX_MINOR, -MAX_MINOR, i64::MAX, i64::MIN, i64::MIN + 1, 1_000_000_000_000] {
        for rate in ["0.5", "0.00000001", "0.99999999"] {
            assert_eq!(to_main(minor, rate), Err(INVALID), "{minor} at {rate}");
        }
    }
    for minor in [i64::MAX, i64::MIN, i64::MIN + 1, MAX_MINOR + 1, -MAX_MINOR - 1] {
        assert!(to_main(minor, "1").is_err(), "{minor} at 1");
    }
    assert_eq!(to_main(i64::MAX, "999999999").map(i128::from), Ok(model_to_main(i64::MAX, 999_999_999, 1)), "a big amount at a big rate fits, and the work does not overflow");
    assert_eq!(to_main(1, "0.00000001"), Ok(100_000_000));
    for bad in ["0", "0.0", "-1", "-0.5", "abc", "5,00", "", " ", " 5", "5 ", "5.", ".5", "+5", "1e3", "1234567890", "1.123456789"] {
        assert_eq!(to_main(100, bad), Err(INVALID), "{bad:?}");
    }
}

#[test]
fn the_value_of_a_foreign_account_in_the_main_currency_follows_the_sign_and_the_rate() {
    let w = world();
    let rich = lei(&w.conn, "Lei", "RON", "5.00", "123.45");
    let owing = lei(&w.conn, "Card", "RON", "5.00", "-123.45");
    let empty = lei(&w.conn, "Empty", "USD", "1.10", "0.00");
    let cent = lei(&w.conn, "Cents", "RON", "3", "0.02");
    let plain = opened(&w.conn, "Euros", "-77.70", "2026-01-01");
    let of = |id: i64| {
        let a = get(&w.conn, id, TODAY).unwrap();
        (a.balance, a.balance_in_main)
    };
    assert_eq!(of(rich), ("123.45".into(), "24.69".into()));
    assert_eq!(of(owing), ("-123.45".into(), "-24.69".into()), "the sign is kept");
    assert_eq!(of(empty), ("0.00".into(), "0.00".into()));
    assert_eq!(of(cent), ("0.02".into(), "0.01".into()), "two cents at a rate of three: 0.67 rounds up");
    assert_eq!(of(plain), ("-77.70".into(), "-77.70".into()), "an account in the main currency: the balance");
    assert_eq!(of(MAIN_ID), ("0.00".into(), "0.00".into()));
    // the rate decides, and a new rate moves the value and no stored number
    update(&w.conn, rich, &AccountPatch { rate: Some("0.5".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!(of(rich), ("123.45".into(), "246.90".into()));
    update(&w.conn, owing, &AccountPatch { rate: Some("160".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!(of(owing), ("-123.45".into(), "-0.77".into()));
    assert_eq!(list(&w.conn, TODAY).unwrap().len(), 6);
    // a value that cannot be shown shows the bound it passed, and does not make the list unreadable
    let big = lei(&w.conn, "Big", "RON", "0.5", "9999999999.99");
    let debt = lei(&w.conn, "Big debt", "RON", "0.00000001", "-9999999999.99");
    assert_eq!(of(big), ("9999999999.99".into(), "9999999999.99".into()));
    assert_eq!(of(debt), ("-9999999999.99".into(), "-9999999999.99".into()));
    assert_eq!(list(&w.conn, TODAY).unwrap().len(), 8);
    assert!(summary(&w.conn, TODAY).is_ok());
}

#[test]
fn net_worth_adds_each_account_in_the_main_currency_at_its_own_rate_whatever_its_state() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let savings = opened(&w.conn, "Savings", "100.00", "2026-01-01");
    let lei1 = lei(&w.conn, "Lei", "RON", "5.00", "1000.00");
    let lei2 = lei(&w.conn, "More lei", "RON", "4.00", "100.00");
    let usd = lei(&w.conn, "Dollars", "USD", "1.10", "330.00");
    let card = lei(&w.conn, "Card", "RON", "5.00", "-50.00");
    let summed = || {
        let s = summary(&w.conn, TODAY).unwrap();
        (s.in_accounts, s.friends, s.net_worth, s.accounts, s.in_use)
    };
    let s = |a: &str, f: &str, n: &str, count: u32| (a.to_string(), f.to_string(), n.to_string(), count, true);
    // 100.00 + 200.00 + 25.00 + 300.00 - 10.00
    assert_eq!(summed(), s("615.00", "0.00", "615.00", 6));
    // money a friend owes you is yours, in the main currency
    entry(&w.conn, MAIN_ID, anna, ledger::Kind::Lent, "40.00", "2026-11-02");
    assert_eq!(summed(), s("575.00", "40.00", "615.00", 6), "lending moved 40.00 out of the main account and into what friends owe");
    // the same currency at two rates: each account at its own
    update(&w.conn, lei2, &AccountPatch { rate: Some("5.00".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!(summed(), s("570.00", "40.00", "610.00", 6), "100.00 lei is 20.00 now, not 25.00");
    // an account that does not count is left out, however it is in its own currency
    update(&w.conn, usd, &AccountPatch { include_in_net_worth: Some(false), ..Default::default() }, TODAY).unwrap();
    assert_eq!(summed(), s("270.00", "40.00", "310.00", 6), "the count of accounts does not change");
    // archived, a foreign account still counts; archived and excluded it does not
    super::archive(&w.conn, lei1, TODAY).unwrap();
    assert_eq!(summed(), s("270.00", "40.00", "310.00", 6));
    update(&w.conn, card, &AccountPatch { include_in_net_worth: Some(false), ..Default::default() }, TODAY).unwrap();
    assert_eq!(summed(), s("280.00", "40.00", "320.00", 6), "the negative one is out: 10.00 more");
    update(&w.conn, usd, &AccountPatch { include_in_net_worth: Some(true), ..Default::default() }, TODAY).unwrap();
    assert_eq!(summed(), s("580.00", "40.00", "620.00", 6));
    // rows move a foreign account by its own amounts, and net worth by their value: a foreign expense of 100.00 lei, 20.00 at 5.00
    native_on(&w.conn, lei2, w.food, "Shop", "19.00", "100.00", "2026-11-03");
    assert_eq!(summed(), s("560.00", "40.00", "600.00", 6), "the amount in the main currency of the row, 19.00, plays no part: the account lost 100.00 lei");
    // a transfer between currencies: out of savings by its amount, into the foreign account by its own
    transfers::create(&w.conn, &transfers::NewTransfer { to_amount: Some("249.00".into()), ..new_transfer(savings, usd, "50.00", "2026-11-04") }).unwrap();
    assert_eq!(summed(), s("736.36", "40.00", "776.36", 6), "50.00 left savings and 249.00 dollars reached the other account, 226.36 worth of them: a transfer between currencies is not zero-sum");
}

#[test]
fn a_foreign_accounts_statement_chart_breakdown_and_change_this_month_are_in_its_own_currency() {
    let w = world();
    // this month is November 2026 (TODAY is 2026-11-13); the account opened with 100.00 lei in October
    let lei = create(&w.conn, &NewAccount { currency: Some("RON".into()), rate: Some("5.00".into()), opening_balance: Some("100.00".into()), opening_date: Some("2026-10-01".into()), ..new_account("Lei") }, TODAY).unwrap().id;
    native_on(&w.conn, lei, w.food, "Shop", "20.00", "100.00", "2026-11-02");
    let pay = transactions::create(
        &w.conn,
        &NewTransaction { native_amount: Some("49.00".into()), account_id: Some(lei.to_string()), ..tx_in(w.salary, "Pay", "10.00", "2026-11-03") },
    )
    .unwrap()
    .id;
    transfers::create(&w.conn, &transfers::NewTransfer { to_amount: Some("249.00".into()), ..new_transfer(MAIN_ID, lei, "50.00", "2026-11-05") }).unwrap();
    transfers::create(&w.conn, &transfers::NewTransfer { to_amount: Some("5.90".into()), ..new_transfer(lei, MAIN_ID, "30.00", "2026-11-06") }).unwrap();

    let account = get(&w.conn, lei, TODAY).unwrap();
    assert_eq!((account.balance.as_str(), account.change_this_month.as_str(), account.last_activity.as_deref(), account.currency.as_str()), ("268.00", "168.00", Some("2026-11-06"), "RON"));
    assert_eq!(account.balance_in_main, "53.60");

    let s = statement(&w.conn, lei, "2026-11-01", "2026-11-30", TODAY).unwrap();
    assert_eq!((s.currency.as_str(), s.account.currency.as_str(), s.balance_before.as_str(), s.balance_after.as_str()), ("RON", "RON", "100.00", "268.00"));
    let lines: Vec<(&str, &str, &str, &str)> = s.lines.iter().map(|l| (l.date.as_str(), l.source.as_str(), l.amount.as_str(), l.balance.as_str())).collect();
    assert_eq!(
        lines,
        [("2026-11-02", "TRANSACTION", "-100.00", "0.00"), ("2026-11-03", "TRANSACTION", "49.00", "49.00"), ("2026-11-05", "TRANSFER_IN", "249.00", "298.00"), ("2026-11-06", "TRANSFER_OUT", "-30.00", "268.00")],
        "every amount is the one the account really moved by, not the amount in the main currency"
    );
    assert_eq!(s.lines[1].id, pay.to_string());
    assert_eq!(serde_json::to_value(&s).unwrap()["currency"], "RON");
    // the main account is in euro: the same two transfers, each by its own amount
    let main = statement(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30", TODAY).unwrap();
    assert_eq!(main.currency, "EUR");
    assert_eq!(main.lines.iter().map(|l| (l.source.as_str(), l.amount.as_str())).collect::<Vec<_>>(), [("TRANSFER_OUT", "-50.00"), ("TRANSFER_IN", "5.90")]);
    // the chart: the balance at the end of each month, in lei
    let points = history(&w.conn, lei, "2026", TODAY).unwrap();
    assert_eq!(points.len(), 11);
    assert_eq!((points[8].balance.as_str(), points[9].balance.as_str(), points[10].balance.as_str()), ("0.00", "100.00", "268.00"));
    // the breakdown: what the account paid and took in, in lei
    let b = breakdown(&w.conn, lei, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!(b.currency, "RON");
    assert_eq!((shares(&b.expenses), shares(&b.income), b.expense_total.as_str(), b.income_total.as_str()), (vec![("Food", "100.00")], vec![("Salary", "49.00")], "100.00", "49.00"));
    let b = breakdown(&w.conn, MAIN_ID, "2026-11-01", "2026-11-30").unwrap();
    assert_eq!((b.currency.as_str(), b.expenses.len(), b.income.len()), ("EUR", 0, 0), "transfers are no part of it");
    assert_eq!(serde_json::to_value(&b).unwrap()["currency"], "EUR");
    // the rate moves none of this
    update(&w.conn, lei, &AccountPatch { rate: Some("4.00".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!(statement(&w.conn, lei, "2026-11-01", "2026-11-30", TODAY).unwrap().lines, s.lines);
    assert_eq!(get(&w.conn, lei, TODAY).unwrap().balance_in_main, "67.00");
    // an unknown account has no currency to report
    assert_eq!(breakdown(&w.conn, 99, "2026-11-01", "2026-11-30"), Err(AppError::NotFound));
}

/// A transaction as a main-currency account takes it, with the amount of the person's choice.
fn tx_in(category: i64, name: &str, amount: &str, date: &str) -> NewTransaction {
    NewTransaction { name: name.into(), amount: amount.into(), payment_date: date.into(), category_id: category.to_string(), description: None, client_uuid: None, account_id: None, native_amount: None }
}

#[test]
fn what_is_available_is_in_the_currency_of_the_account_and_the_virtual_balance_is_never_converted() {
    let w = world();
    let card = lei(&w.conn, "Card", "RON", "5.00", "-240.00");
    update(&w.conn, card, &AccountPatch { virtual_balance: Some("1000.00".into()), ..Default::default() }, TODAY).unwrap();
    let shown = get(&w.conn, card, TODAY).unwrap();
    assert_eq!((shown.balance.as_str(), shown.virtual_balance.as_str(), shown.available.as_str(), shown.balance_in_main.as_str()), ("-240.00", "1000.00", "760.00", "-48.00"));
    // the limit is no part of net worth or of the value in the main currency
    assert_eq!(summary(&w.conn, TODAY).unwrap().in_accounts, "-48.00");
    // a row of the account moves what is available by its native amount
    native_on(&w.conn, card, w.food, "Shop", "9.00", "45.00", "2026-11-02");
    let shown = get(&w.conn, card, TODAY).unwrap();
    assert_eq!((shown.balance.as_str(), shown.available.as_str(), shown.balance_in_main.as_str()), ("-285.00", "715.00", "-57.00"));
    // the limit in lei, and a limit that is negative
    update(&w.conn, card, &AccountPatch { virtual_balance: Some("-300.00".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!(get(&w.conn, card, TODAY).unwrap().available, "-585.00");
    // a rate that changes leaves the lei alone
    update(&w.conn, card, &AccountPatch { rate: Some("2".into()), ..Default::default() }, TODAY).unwrap();
    let shown = get(&w.conn, card, TODAY).unwrap();
    assert_eq!((shown.available.as_str(), shown.virtual_balance.as_str(), shown.balance_in_main.as_str()), ("-585.00", "-300.00", "-142.50"));
}

// ---- "set the balance" (section 21.5) --------------------------------------------------------------

fn set_balance(balance: &str) -> AccountPatch {
    AccountPatch { balance: Some(balance.into()), ..Default::default() }
}

fn opening_of(conn: &Connection, id: i64) -> i64 {
    one(conn, &format!("SELECT opening_balance_minor FROM accounts WHERE id = {id}"))
}

#[test]
fn setting_the_balance_moves_the_opening_balance_by_the_difference_up_or_down() {
    let w = world();
    let cash = opened(&w.conn, "Cash", "100.00", "2026-01-01");
    on(&w.conn, cash, w.food, "Shop", "20.00", "2026-11-02");
    assert_eq!(get(&w.conn, cash, TODAY).unwrap().balance, "80.00");
    let made = stamp_of(&w.conn, "accounts", cash);
    // higher
    let up = update(&w.conn, cash, &set_balance("1662.90"), TODAY).unwrap();
    assert_eq!((up.balance.as_str(), up.opening_balance.as_str()), ("1662.90", "1682.90"));
    assert!(stamp_of(&w.conn, "accounts", cash) > made);
    // lower, and negative
    let down = update(&w.conn, cash, &set_balance("50.00"), TODAY).unwrap();
    assert_eq!((down.balance.as_str(), down.opening_balance.as_str()), ("50.00", "70.00"));
    let negative = update(&w.conn, cash, &set_balance("-5.00"), TODAY).unwrap();
    assert_eq!((negative.balance.as_str(), negative.opening_balance.as_str()), ("-5.00", "15.00"));
    let zero = update(&w.conn, cash, &set_balance("0.00"), TODAY).unwrap();
    assert_eq!((zero.balance.as_str(), zero.opening_balance.as_str()), ("0.00", "20.00"));
    // a balance that makes the opening balance nothing leaves no opening movement at all
    let nothing = update(&w.conn, cash, &set_balance("-20.00"), TODAY).unwrap();
    assert_eq!((nothing.balance.as_str(), nothing.opening_balance.as_str()), ("-20.00", "0.00"));
    assert_eq!(one(&w.conn, &format!("SELECT count(*) FROM account_movements WHERE account_id = {cash} AND source = 'OPENING'")), 0);
    // the rows were never touched, and only the opening balance moved
    assert_eq!(text(&w.conn, "SELECT amount_minor || ' ' || payment_date FROM transactions"), "2000 2026-11-02");
    assert_eq!(get(&w.conn, cash, TODAY).unwrap().last_activity.as_deref(), Some("2026-11-02"));
    // the main account is an account like the others
    let main = update(&w.conn, MAIN_ID, &set_balance("250.00"), TODAY).unwrap();
    assert_eq!((main.balance.as_str(), main.opening_balance.as_str()), ("250.00", "250.00"));
}

#[test]
fn setting_the_balance_to_what_it_is_writes_nothing_and_makes_no_stamp() {
    let w = world();
    let cash = opened(&w.conn, "Cash", "100.00", "2026-01-01");
    on(&w.conn, cash, w.food, "Shop", "20.00", "2026-11-02");
    on(&w.conn, cash, w.food, "Later", "7.00", "2026-12-01");
    let before = get(&w.conn, cash, TODAY).unwrap();
    assert_eq!(before.balance, "80.00");
    assert_eq!(assert_nothing_written(&w.conn, "the balance it has", || update(&w.conn, cash, &set_balance("80.00"), TODAY)), before);
    // an account with no rows at all, and the main account
    let empty = account(&w.conn, "Empty");
    assert_nothing_written(&w.conn, "0.00 on an empty account", || update(&w.conn, empty, &set_balance("0.00"), TODAY));
    assert_nothing_written(&w.conn, "0.00 on the untouched main account", || update(&w.conn, MAIN_ID, &set_balance("0.00"), TODAY));
    // with a field that is as it is, too
    assert_nothing_written(&w.conn, "the balance and the name it has", || update(&w.conn, cash, &AccountPatch { name: Some("Cash".into()), balance: Some("80.00".into()), ..Default::default() }, TODAY));
    // and another balance and back is two changes
    update(&w.conn, cash, &set_balance("81.00"), TODAY).unwrap();
    update(&w.conn, cash, &set_balance("80.00"), TODAY).unwrap();
    assert_eq!(get(&w.conn, cash, TODAY).unwrap(), before);
}

#[test]
fn a_balance_that_would_leave_the_range_or_is_badly_written_changes_nothing() {
    let w = world();
    let cash = opened(&w.conn, "Cash", "100.00", "2026-01-01");
    on(&w.conn, cash, w.food, "Shop", "20.00", "2026-11-02");
    // the opening balance would be the balance wanted plus 20.00 (or minus 20.00 on the other side): one cent beyond is refused, the bound is allowed
    assert_refused(&w.conn, INVALID, "the largest balance, which needs an opening balance beyond the largest", || update(&w.conn, cash, &set_balance("9999999999.99"), TODAY));
    assert_refused(&w.conn, INVALID, "one cent beyond the opening balance", || update(&w.conn, cash, &set_balance("9999999980.00"), TODAY));
    let at_bound = update(&w.conn, cash, &set_balance("9999999979.99"), TODAY).unwrap();
    assert_eq!((at_bound.balance.as_str(), at_bound.opening_balance.as_str()), ("9999999979.99", "9999999999.99"));
    assert_eq!(opening_of(&w.conn, cash), MAX_MINOR);
    // negative side: with money in, the largest debt cannot be reached
    on(&w.conn, cash, w.salary, "Pay", "50.00", "2026-11-03");
    assert_refused(&w.conn, INVALID, "the largest debt, which needs an opening balance below the smallest", || update(&w.conn, cash, &set_balance("-9999999999.99"), TODAY));
    assert_refused(&w.conn, INVALID, "one cent beyond the smallest opening balance", || update(&w.conn, cash, &set_balance("-9999999970.00"), TODAY));
    assert_eq!(update(&w.conn, cash, &set_balance("-9999999969.99"), TODAY).unwrap().opening_balance, "-9999999999.99");
    // forms that are not an amount
    for bad in ["", " ", "abc", "1.5", "1", "-0.00", "+1.00", "1,00", "--1.00", "10000000000.00", "-10000000000.00", " 5.00", "5.00 "] {
        assert_refused(&w.conn, INVALID, &format!("balance {bad:?}"), || update(&w.conn, cash, &set_balance(bad), TODAY));
    }
    // with an opening balance in the same patch, whatever it says
    for opening in ["0.00", "5.00", "-5.00", "70.00"] {
        assert_refused(&w.conn, INVALID, &format!("balance and opening balance {opening}"), || update(&w.conn, cash, &AccountPatch { balance: Some("30.00".into()), opening_balance: Some(opening.into()), ..Default::default() }, TODAY));
    }
    // the first of the failures did not make a stamp either, and a good field with a bad balance changes nothing
    assert_refused(&w.conn, INVALID, "a good name with a bad balance", || update(&w.conn, cash, &AccountPatch { name: Some("Fine".into()), balance: Some("1.5".into()), ..Default::default() }, TODAY));
    assert_refused(&w.conn, AppError::NotFound, "an account that is not there", || update(&w.conn, 99, &set_balance("1.00"), TODAY));
}

#[test]
fn setting_the_balance_counts_today_and_leaves_the_rows_of_later_days_to_apply_later() {
    let w = world();
    let cash = opened(&w.conn, "Cash", "100.00", "2026-01-01");
    on(&w.conn, cash, w.food, "Shop", "20.00", "2026-11-02");
    on(&w.conn, cash, w.food, "Tomorrow", "30.00", "2026-11-14");
    on(&w.conn, cash, w.salary, "Pay day", "500.00", "2026-12-01");
    transfer(&w.conn, cash, MAIN_ID, "10.00", "2027-01-01");
    assert_eq!(balance(&w.conn, cash, TODAY), 8000, "today: 100.00 - 20.00");
    let set = update(&w.conn, cash, &set_balance("1000.00"), TODAY).unwrap();
    assert_eq!((set.balance.as_str(), set.opening_balance.as_str()), ("1000.00", "1020.00"), "the future rows are no part of today's balance");
    assert_eq!([TODAY, "2026-11-14", "2026-12-01", "2027-01-01"].map(|d| balance(&w.conn, cash, d)), [100_000, 97_000, 147_000, 146_000], "and they still apply on their days");
    // 'today' is the one the caller gives: a day later the row of that day is in
    assert_eq!(get(&w.conn, cash, "2026-11-14").unwrap().balance, "970.00");
    let later = update(&w.conn, cash, &set_balance("500.00"), "2026-11-14").unwrap();
    assert_eq!(later.balance, "500.00");
    assert_eq!(opening_of(&w.conn, cash), 55_000, "1020.00 - 970.00 + 500.00");
    // an opening balance dated after today is not in today's balance, so it cannot be moved to set it
    let ahead = create(&w.conn, &NewAccount { opening_balance: Some("40.00".into()), opening_date: Some("2026-12-31".into()), ..new_account("Ahead") }, TODAY).unwrap().id;
    assert_refused(&w.conn, INVALID, "a balance for an account that opens later", || update(&w.conn, ahead, &set_balance("10.00"), TODAY));
    // unless the same patch brings it to a day that is not after today
    let moved = update(&w.conn, ahead, &AccountPatch { balance: Some("10.00".into()), opening_date: Some("2026-11-13".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!((moved.balance.as_str(), moved.opening_balance.as_str(), moved.opening_date.as_str()), ("10.00", "10.00", "2026-11-13"));
    assert_refused(&w.conn, INVALID, "a patch that moves the opening date to after today", || update(&w.conn, cash, &AccountPatch { balance: Some("1.00".into()), opening_date: Some("2026-12-31".into()), ..Default::default() }, TODAY));
}

#[test]
fn setting_the_balance_moves_every_earlier_balance_by_the_same_amount() {
    let w = world();
    let cash = opened(&w.conn, "Cash", "100.00", "2026-01-01");
    on(&w.conn, cash, w.food, "Shop", "10.00", "2026-02-01");
    on(&w.conn, cash, w.salary, "Pay", "5.00", "2026-03-01");
    transfer(&w.conn, cash, MAIN_ID, "20.00", "2026-04-01");
    let read = |from: &str| {
        let s = statement(&w.conn, cash, from, "2026-12-31", TODAY).unwrap();
        (s.balance_before.clone(), s.lines.iter().map(|l| l.balance.clone()).collect::<Vec<_>>(), s.balance_after.clone())
    };
    let before = (read("2026-01-01"), read("2026-03-01"), history(&w.conn, cash, "2026", TODAY).unwrap());
    assert_eq!(before.0, ("0.00".into(), vec!["100.00".into(), "90.00".into(), "95.00".into(), "75.00".into()], "75.00".into()));
    // 75.00 becomes 500.00: every balance, on every day, is 425.00 more from the opening day on
    update(&w.conn, cash, &set_balance("500.00"), TODAY).unwrap();
    assert_eq!(read("2026-01-01"), ("0.00".into(), vec!["525.00".into(), "515.00".into(), "520.00".into(), "500.00".into()], "500.00".into()));
    assert_eq!(read("2026-03-01"), ("515.00".into(), vec!["520.00".into(), "500.00".into()], "500.00".into()), "a statement that starts later: the balance before it moved too");
    let chart: Vec<String> = history(&w.conn, cash, "2026", TODAY).unwrap().iter().map(|p| p.balance.clone()).collect();
    let was: Vec<String> = before.2.iter().map(|p| p.balance.clone()).collect();
    assert_eq!(chart.len(), was.len());
    for (new, old) in chart.iter().zip(&was) {
        assert_eq!(new.replace('.', "").parse::<i64>().unwrap() - old.replace('.', "").parse::<i64>().unwrap(), 42_500, "{new} against {old}");
    }
    // the other accounts and the transfer's other side did not move
    assert_eq!(balance(&w.conn, MAIN_ID, TODAY), 2000);
    assert_eq!(one(&w.conn, "SELECT amount_minor FROM transfers"), 2000);
}

#[test]
fn the_balance_of_a_foreign_account_is_set_in_its_own_currency_and_its_value_follows() {
    let w = world();
    let lei = lei(&w.conn, "Lei", "RON", "5.00", "100.00");
    native_on(&w.conn, lei, w.food, "Shop", "20.00", "100.00", "2026-11-02");
    assert_eq!(get(&w.conn, lei, TODAY).unwrap().balance, "0.00");
    let set = update(&w.conn, lei, &set_balance("1662.90"), TODAY).unwrap();
    assert_eq!((set.balance.as_str(), set.opening_balance.as_str(), set.balance_in_main.as_str(), set.currency.as_str()), ("1662.90", "1762.90", "332.58", "RON"));
    assert_eq!(text(&w.conn, "SELECT amount_minor || ' ' || native_amount_minor FROM transactions"), "2000 10000", "the transaction is as it was");
    // the currency and the balance together, for an account that is made foreign by the same patch: nothing points at it
    let fresh = account(&w.conn, "Fresh");
    let both = update(&w.conn, fresh, &AccountPatch { currency: Some("usd".into()), rate: Some("1.10".into()), balance: Some("330.00".into()), ..Default::default() }, TODAY).unwrap();
    assert_eq!((both.currency.as_str(), both.balance.as_str(), both.balance_in_main.as_str()), ("USD", "330.00", "300.00"));
    // and net worth sees it in the main currency
    assert_eq!(summary(&w.conn, TODAY).unwrap().in_accounts, "632.58");
}
