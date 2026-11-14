//! The numbers of accounts (`docs/accounts-design.md`, section 3): the balance on a day, the change this month, the last activity, the
//! statement, the chart and net worth, against a plain recomputation from random rows, and the cases that are easy to get wrong.

use chrono::{Days, NaiveDate};
use rusqlite::Connection;

use super::fixtures::*;
use super::transfers;
use super::{create, get, history, list, statement, summary, update, AccountPatch, HistoryPoint, Kind, Line, NewAccount, MAIN_ID, MAIN_NAME};
use crate::debts::fixtures::{equal, who};
use crate::debts::ledger::{self, Expense, NewEntry};
use crate::debts::people;
use crate::debts::splits::{set_split, SplitInput};
use crate::money::format_amount;
use crate::transactions::{self, NewTransaction};

fn on(conn: &Connection, account: i64, category: i64, name: &str, amount: &str, date: &str) -> i64 {
    transactions::create(
        conn,
        &NewTransaction { name: name.into(), amount: amount.into(), payment_date: date.into(), category_id: category.to_string(), description: None, client_uuid: None, account_id: Some(account.to_string()) },
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
