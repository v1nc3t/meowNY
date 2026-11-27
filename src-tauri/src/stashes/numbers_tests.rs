//! The numbers of section 3 (`docs/stash-design.md`): saved, held, set aside, free, left, percent, reached, per month, overdue, and the history
//! and the summary. The pure functions are checked at their bounds; the whole is checked against a recomputation from the rows, from fixed seeds.

use super::fixtures::*;
use super::{archive, delta_allowed, free, history, progress, summary, Progress};
use crate::accounts;
use crate::debts::fixtures::{one, Rng};

fn p(saved: i64, target: Option<i64>, date: Option<&str>, today: &str) -> Progress {
    progress(saved, target, date, today).unwrap()
}

// ---- left, percent, reached ---------------------------------------------------------------------

#[test]
fn a_stash_without_a_target_has_no_left_percent_or_per_month_and_is_never_reached_or_overdue() {
    for saved in [-500, 0, 1, 123_456] {
        assert_eq!(p(saved, None, None, "2026-11-13"), Progress { left: None, percent: None, reached: false, per_month: None, overdue: false }, "{saved}");
        // a date alone says nothing: it needs a target
        assert_eq!(p(saved, None, Some("2020-01-01"), "2026-11-13"), Progress { left: None, percent: None, reached: false, per_month: None, overdue: false }, "{saved}, a past date");
        assert_eq!(p(saved, None, Some("2027-01-01"), "2026-11-13").per_month, None);
    }
}

#[test]
fn left_is_what_is_missing_and_never_below_zero_and_reached_is_saved_at_least_the_target() {
    for (saved, target, left, reached) in [
        (0, 100_000, 100_000, false),
        (45_000, 120_000, 75_000, false),
        (119_999, 120_000, 1, false),
        (120_000, 120_000, 0, true),
        (120_001, 120_000, 0, true),
        (300_000, 120_000, 0, true),
        (-5_000, 120_000, 125_000, false),
        (1, 1, 0, true),
        (0, 1, 1, false),
    ] {
        let n = p(saved, Some(target), None, "2026-11-13");
        assert_eq!((n.left, n.reached), (Some(left), reached), "saved {saved} of {target}");
        assert_eq!(n.reached, saved >= target);
    }
}

#[test]
fn percent_is_saved_times_a_hundred_over_the_target_rounded_down_and_zero_when_nothing_is_saved() {
    for (saved, target, percent) in [
        (0, 300, 0),
        (-1, 300, 0),
        (-99_999, 300, 0),
        (1, 300, 0),
        (2, 300, 0),
        (3, 300, 1),
        (299, 300, 99),
        (300, 300, 100),
        (301, 300, 100),
        (1, 3, 33),
        (2, 3, 66),
        (45_000, 120_000, 37),
        (449_999, 1_000_000, 44),
        (1_000_000, 1_000_000, 100),
        (30_000, 10_000, 300),
        (999_999_999_999, 1, 99_999_999_999_900),
        (1, 999_999_999_999, 0),
    ] {
        assert_eq!(p(saved, Some(target), None, "2026-11-13").percent, Some(percent), "{saved} of {target}");
    }
    // every number: floor(saved * 100 / target), recomputed with another kind of number
    let mut rng = Rng(7);
    for _ in 0..500 {
        let (saved, target) = (rng.between(1, 2_000_000) as i64, rng.between(1, 2_000_000) as i64);
        let by_float = ((saved as f64) * 100.0 / (target as f64) + 1e-9).floor() as i64;
        let got = p(saved, Some(target), None, "2026-11-13").percent.unwrap();
        assert!(got == by_float || got == by_float - 1, "{saved} of {target}: {got} against {by_float}");
        assert!(got * target <= saved * 100 && (got + 1) * target > saved * 100, "{saved} of {target}: {got} is the floor");
    }
}

// ---- per month, overdue ------------------------------------------------------------------------------

#[test]
fn per_month_counts_calendar_months_from_the_month_of_today_to_the_month_of_the_date_whatever_the_day() {
    let left = 120_000;
    for (today, date, months) in [
        ("2026-11-13", "2027-06-30", 7),
        ("2026-11-13", "2026-12-01", 1),
        ("2026-11-13", "2026-12-31", 1),
        ("2026-11-30", "2026-12-01", 1),
        ("2026-12-31", "2027-01-01", 1),
        ("2026-01-31", "2026-02-28", 1),
        ("2026-01-31", "2026-03-01", 2),
        ("2026-03-31", "2026-04-30", 1),
        ("2026-05-31", "2026-06-30", 1),
        ("2024-01-31", "2024-02-29", 1),
        ("2026-11-13", "2027-11-13", 12),
        ("2026-01-01", "2028-01-01", 24),
        ("2026-12-31", "2027-12-31", 12),
    ] {
        let n = p(0, Some(left), Some(date), today);
        let expected = (left + months - 1) / months;
        assert_eq!((n.per_month, n.overdue), (Some(expected), false), "{today} to {date}: {months} months");
        assert!(expected * months >= left && (expected - 1) * months < left, "rounded up to the cent, and no more than that");
    }
}

#[test]
fn a_target_this_month_asks_for_all_that_is_left_on_any_day_of_the_month() {
    for (today, date) in [("2026-11-01", "2026-11-30"), ("2026-11-13", "2026-11-13"), ("2026-11-30", "2026-11-30"), ("2026-11-15", "2026-11-16"), ("2026-02-28", "2026-02-28")] {
        let n = p(20_000, Some(120_000), Some(date), today);
        assert_eq!((n.left, n.per_month, n.overdue), (Some(100_000), Some(100_000), false), "{today} to {date}: this month, so all of it");
    }
}

#[test]
fn per_month_rounds_up_to_the_cent_so_that_saving_it_each_month_reaches_the_target() {
    for (left, months, per_month) in [(10_000, 3, 3334), (10_000, 7, 1429), (120_000, 7, 17_143), (1, 5, 1), (9000, 3, 3000), (1, 1, 1), (999_999_999_999, 7, 142_857_142_857), (10_000, 1, 10_000), (10_001, 2, 5001)] {
        // months from 2026-11-13 to the date `months` months on
        let date = format!("{}-{:02}-28", 2026 + (10 + months) / 12, (10 + months) % 12 + 1);
        let n = p(0, Some(left), Some(&date), "2026-11-13");
        assert_eq!(n.per_month, Some(per_month), "{left} over {months} months ({date})");
    }
}

#[test]
fn a_date_that_is_past_makes_the_stash_overdue_with_no_per_month_unless_it_is_reached_or_has_nothing_left() {
    let n = p(20_000, Some(120_000), Some("2026-11-12"), "2026-11-13");
    assert_eq!((n.overdue, n.per_month, n.left, n.reached), (true, None, Some(100_000), false), "yesterday");
    assert!(p(0, Some(120_000), Some("2020-02-29"), "2026-11-13").overdue, "long ago");
    assert!(p(0, Some(120_000), Some("2026-10-31"), "2026-11-01").overdue, "the month before");
    let today = p(0, Some(120_000), Some("2026-11-13"), "2026-11-13");
    assert_eq!((today.overdue, today.per_month), (false, Some(120_000)), "the date is today: not past");
    // reached: nothing is missing, so there is nothing to be late with and nothing to save a month
    let done = p(120_000, Some(120_000), Some("2020-01-01"), "2026-11-13");
    assert_eq!((done.overdue, done.per_month, done.reached, done.left), (false, None, true, Some(0)));
    assert_eq!(p(130_000, Some(120_000), Some("2027-01-01"), "2026-11-13").per_month, None, "above the target: left is 0");
    // no date: no per month and never overdue
    let open = p(20_000, Some(120_000), None, "2026-11-13");
    assert_eq!((open.per_month, open.overdue), (None, false));
}

#[test]
fn the_numbers_of_a_stash_are_those_of_the_functions_on_the_stored_rows() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let id = create_aimed(&w.conn, acc, "1200.00", "2027-06-30");
    put(&w.conn, id, "450.00");
    let s = shown(&w.conn, id);
    assert_eq!(
        (s.saved.as_str(), s.left.as_deref(), s.percent, s.reached, s.per_month.as_deref(), s.overdue, s.free.as_str(), s.entries, s.last_entry.as_deref()),
        ("450.00", Some("750.00"), Some(37), false, Some("107.15"), false, "4550.00", 1, Some(TODAY)),
        "the example of section 6.1: 750.00 over 7 months is 107.15 rounded up"
    );
    // moving the clock: the same stash a year later is overdue
    let later = super::stash(&w.conn, id, "2028-01-15").unwrap();
    assert_eq!((later.overdue, later.per_month), (true, None));
    // reached
    put(&w.conn, id, "750.00");
    let reached = shown(&w.conn, id);
    assert_eq!((reached.reached, reached.percent, reached.left.as_deref(), reached.per_month, reached.overdue), (true, Some(100), Some("0.00"), None, false));
}

fn create_aimed(conn: &rusqlite::Connection, account: i64, target: &str, date: &str) -> i64 {
    super::create(conn, &super::NewStash { target: Some(target.into()), target_date: Some(date.into()), ..new_stash("Trip", account) }, TODAY).unwrap().id
}

// ---- the delta rule on numbers -------------------------------------------------------------------------

#[test]
fn the_delta_rule_at_every_bound() {
    // money in: after - before <= free, and after <= target when there is one
    assert!(delta_allowed(100, 150, 50, None), "exactly what is free");
    assert!(!delta_allowed(100, 151, 50, None), "one more than is free");
    assert!(delta_allowed(100, 150, 50, Some(150)), "exactly the target");
    assert!(!delta_allowed(100, 151, 51, Some(150)), "one more than the target, free is enough");
    assert!(!delta_allowed(100, 150, 49, Some(1000)), "the target is far but free is short");
    assert!(delta_allowed(0, 1, 1, Some(1)));
    assert!(!delta_allowed(0, 1, 0, None), "nothing free");
    assert!(!delta_allowed(0, 1, -5, None), "free is negative: no add at all");
    assert!(!delta_allowed(-100, -50, -1, None), "money in from below zero is money in");
    assert!(delta_allowed(-100, 50, 150, None), "from below zero, the whole difference must be free");
    assert!(!delta_allowed(-100, 50, 149, None));
    // money out: after >= 0
    assert!(delta_allowed(100, 0, -999, Some(10)), "exactly what is saved, with free negative and a target below it");
    assert!(!delta_allowed(100, -1, 999, None), "one more than is saved");
    assert!(delta_allowed(100, 99, 0, None));
    assert!(!delta_allowed(-100, -101, 999, None), "from below zero, a take is refused");
    // nothing changes: always allowed
    for (before, free, target) in [(0, 0, None), (-100, -100, Some(10)), (500, -1, Some(100)), (300, 0, Some(100)), (i64::MAX / 2, i64::MIN / 2, Some(1))] {
        assert!(delta_allowed(before, before, free, target), "{before} {free} {target:?}");
    }
}

// ---- held, set aside, free ----------------------------------------------------------------------------

#[test]
fn held_is_saved_or_zero_so_a_stash_below_zero_never_adds_money_to_an_account() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let (a, b) = (stash(&w.conn, "A", acc), stash(&w.conn, "B", acc));
    put(&w.conn, a, "300.00");
    raw(&w.conn, b, -10_000, "2026-01-01");
    let account = accounts::get(&w.conn, acc, TODAY).unwrap();
    assert_eq!((account.set_aside.as_str(), account.free.as_str()), ("300.00", "700.00"), "B holds -100.00 and takes nothing away, adds nothing");
    assert_eq!(shown(&w.conn, b).saved, "-100.00", "the screens show it as it is");
    assert_eq!(free_of(&w.conn, acc), 70_000);
}

#[test]
fn free_with_two_stashes_in_one_account_is_the_balance_less_what_both_hold_and_archived_ones_hold_too() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let (x, y, z) = (stash(&w.conn, "X", acc), stash(&w.conn, "Y", acc), stash(&w.conn, "Z", acc));
    put(&w.conn, x, "300.00");
    put(&w.conn, y, "200.00");
    assert_eq!(free_of(&w.conn, acc), 50_000, "the example of section 3.1: X 300, Y 200, free 500");
    for id in [x, y, z] {
        assert_eq!(shown(&w.conn, id).free, "500.00", "every stash of the account shows the one free");
    }
    put(&w.conn, z, "500.00");
    assert_eq!(free_of(&w.conn, acc), 0);
    archive(&w.conn, z, TODAY).unwrap();
    assert_eq!(free_of(&w.conn, acc), 0, "an archived stash still holds its money");
    // another account is not counted
    let other = funded(&w.conn, "Other", "50.00");
    assert_eq!(free_of(&w.conn, other), 5_000);
    // a transfer into the account is money in it: free grows
    let third = funded(&w.conn, "Third", "100.00");
    transfer(&w.conn, third, acc, "40.00", "2026-11-10");
    assert_eq!((free_of(&w.conn, acc), free_of(&w.conn, third)), (4_000, 6_000));
    // a future-dated transfer is not in today's balance
    transfer(&w.conn, third, acc, "10.00", "2026-12-01");
    assert_eq!(free_of(&w.conn, acc), 4_000);
    assert!(free(&w.conn, 99, TODAY).is_err(), "an account that is not there");
    assert!(free(&w.conn, acc, "tomorrow").is_err());
}

#[test]
fn free_is_negative_after_the_money_was_spent_and_the_screens_can_show_it() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = stash(&w.conn, "Holiday", acc);
    put(&w.conn, id, "800.00");
    spend(&w.conn, w.food, acc, "300.00", "2026-11-10");
    assert_eq!(free_of(&w.conn, acc), -10_000);
    let account = accounts::get(&w.conn, acc, TODAY).unwrap();
    assert_eq!((account.balance.as_str(), account.set_aside.as_str(), account.free.as_str()), ("700.00", "800.00", "-100.00"));
    assert_eq!(shown(&w.conn, id).free, "-100.00");
    // a spending dated tomorrow does not count today
    spend(&w.conn, w.food, acc, "100.00", "2026-11-14");
    assert_eq!(free_of(&w.conn, acc), -10_000);
    // earning it back
    spend(&w.conn, w.salary, acc, "500.00", "2026-11-12");
    assert_eq!(free_of(&w.conn, acc), 40_000);
}

#[test]
fn what_an_account_shows_set_aside_free_and_entries() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let plain = accounts::get(&w.conn, acc, TODAY).unwrap();
    assert_eq!((plain.set_aside.as_str(), plain.free.as_str(), plain.entries), ("0.00", "1000.00", 0));
    let (x, y) = (stash(&w.conn, "X", acc), stash(&w.conn, "Y", acc));
    put(&w.conn, x, "250.00");
    put(&w.conn, y, "100.00");
    let one_stash = accounts::get(&w.conn, acc, TODAY).unwrap();
    assert_eq!((one_stash.set_aside.as_str(), one_stash.free.as_str(), one_stash.balance.as_str(), one_stash.entries), ("350.00", "650.00", "1000.00", 2), "the stashes count in entries, their own entries do not");
    spend(&w.conn, w.food, acc, "10.00", "2026-11-10");
    assert_eq!(accounts::get(&w.conn, acc, TODAY).unwrap().entries, 3);
    archive(&w.conn, y, TODAY).unwrap();
    assert_eq!(accounts::get(&w.conn, acc, TODAY).unwrap().set_aside, "350.00", "an archived stash still holds");
    // the list has the same numbers as get
    let listed = accounts::list(&w.conn, TODAY).unwrap();
    let from_list = listed.iter().find(|a| a.id == acc).unwrap();
    assert_eq!(from_list, &accounts::get(&w.conn, acc, TODAY).unwrap());
    // the main account has none
    let main = &listed[0];
    assert_eq!((main.is_main, main.set_aside.as_str(), main.free.as_str()), (true, "0.00", "0.00"));
    // nothing else of the account changed: the balance, the change this month and the net worth ignore stashes
    let before = accounts::summary(&w.conn, TODAY).unwrap();
    put(&w.conn, x, "100.00");
    assert_eq!(accounts::summary(&w.conn, TODAY).unwrap(), before);
    assert_eq!(accounts::get(&w.conn, acc, TODAY).unwrap().balance, "990.00");
    assert_eq!(accounts::get(&w.conn, acc, TODAY).unwrap().change_this_month, "-10.00");
}

#[test]
fn a_stash_in_a_foreign_account_is_in_that_currency_and_the_account_says_its_set_aside_in_it() {
    let w = world();
    let lei = foreign(&w.conn, "Lei", "RON", "1000.00");
    let id = aimed(&w.conn, "Wedding gift", lei, "1000.00");
    put(&w.conn, id, "250.00");
    let s = shown(&w.conn, id);
    assert_eq!((s.currency.as_str(), s.saved.as_str(), s.free.as_str(), s.percent), ("RON", "250.00", "750.00", Some(25)));
    let account = accounts::get(&w.conn, lei, TODAY).unwrap();
    assert_eq!((account.currency.as_str(), account.set_aside.as_str(), account.free.as_str(), account.balance_in_main.as_str()), ("RON", "250.00", "750.00", "200.00"), "set aside is in lei; the balance in euro is at the rate");
    let main_stash = stash(&w.conn, "Euro", 1);
    assert_eq!(shown(&w.conn, main_stash).currency, "EUR");
}

// ---- recomputed from the rows ----------------------------------------------------------------------

fn month(date: &str) -> i64 {
    let mut parts = date.split('-').map(|n| n.parse::<i64>().unwrap());
    parts.next().unwrap() * 12 + parts.next().unwrap()
}

#[test]
fn every_number_of_every_stash_and_account_is_what_a_recomputation_from_the_rows_says_from_fixed_seeds() {
    for seed in [1u64, 2, 3, 5, 8, 13, 21, 34] {
        let w = world();
        let mut rng = Rng(seed * 7919 + 17);
        let accounts_made = [funded(&w.conn, "A", "1500.00"), funded(&w.conn, "B", "80.00"), foreign(&w.conn, "Lei", "RON", "700.00"), account(&w.conn, "Empty")];
        let mut stashes = Vec::new();
        for n in 0..rng.between(4, 9) {
            let acc = accounts_made[rng.between(0, 3) as usize];
            let target = (rng.between(0, 2) > 0).then(|| format!("{}.{:02}", rng.between(1, 3000), rng.between(0, 99)));
            let date = (rng.between(0, 2) > 0).then(|| format!("{}-{:02}-{:02}", rng.between(2025, 2028), rng.between(1, 12), rng.between(1, 28)));
            let id = super::create(&w.conn, &super::NewStash { target, target_date: date, ..new_stash(&format!("S{n}"), acc) }, TODAY).unwrap().id;
            stashes.push(id);
            for _ in 0..rng.between(0, 6) {
                // straight into the table, as a sync brings entries: the sums can be above a target, below 0, or more than is free
                let minor = rng.between(1, 90_000) as i64 * if rng.between(0, 3) == 0 { -1 } else { 1 };
                raw(&w.conn, id, minor, &format!("2026-{:02}-{:02}", rng.between(1, 11), rng.between(1, 13)));
            }
            if rng.between(0, 4) == 0 {
                archive(&w.conn, id, TODAY).unwrap();
            }
        }
        // some spending, so that free can be negative
        spend(&w.conn, w.food, accounts_made[0], &format!("{}.00", rng.between(1, 900)), "2026-11-10");

        let listed = super::list(&w.conn, None, TODAY).unwrap();
        assert_eq!(listed.len(), stashes.len());
        let (mut summary_saved, mut summary_target, mut summary_reached, mut summary_count) = (0i64, 0i64, 0u32, 0u32);
        for stash in &listed {
            let saved = one(&w.conn, &format!("SELECT COALESCE(SUM(amount_minor), 0) FROM stash_entries WHERE stash_id = {}", stash.id));
            let (target, date): (Option<i64>, Option<String>) = w.conn.query_row(&format!("SELECT target_minor, target_date FROM stashes WHERE id = {}", stash.id), [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            assert_eq!(stash.saved, crate::money::format_amount(saved), "seed {seed} stash {}", stash.id);
            assert_eq!(stash.target, target.map(crate::money::format_amount));
            assert_eq!(stash.left, target.map(|t| crate::money::format_amount((t - saved).max(0))), "left");
            assert_eq!(stash.reached, target.is_some_and(|t| saved >= t), "reached");
            assert_eq!(stash.percent, target.map(|t| if saved > 0 { saved * 100 / t } else { 0 }), "percent");
            let overdue = matches!((target, &date), (Some(t), Some(d)) if saved < t && d.as_str() < TODAY);
            assert_eq!(stash.overdue, overdue, "overdue");
            let per_month = match (target, &date) {
                (Some(t), Some(d)) if saved < t && d.as_str() >= TODAY => {
                    let months = (month(d) - month(TODAY)).max(1);
                    Some(crate::money::format_amount((t - saved + months - 1) / months))
                }
                _ => None,
            };
            assert_eq!(stash.per_month, per_month, "per month of {target:?} {date:?} saved {saved}");
            assert_eq!(stash.entries as i64, one(&w.conn, &format!("SELECT count(*) FROM stash_entries WHERE stash_id = {}", stash.id)));
            assert_eq!(stash.last_entry, w.conn.query_row(&format!("SELECT MAX(entry_date) FROM stash_entries WHERE stash_id = {}", stash.id), [], |r| r.get::<_, Option<String>>(0)).unwrap());
            // free: the balance of the account less what every stash in it holds, the archived ones too
            let held: i64 = one(&w.conn, &format!("SELECT COALESCE(SUM(MAX(h, 0)), 0) FROM (SELECT (SELECT COALESCE(SUM(amount_minor), 0) FROM stash_entries WHERE stash_id = s.id) AS h FROM stashes s WHERE s.account_id = {})", stash.account_id));
            assert_eq!(stash.free, crate::money::format_amount(balance(&w.conn, stash.account_id, TODAY) - held), "free");
            assert_eq!(free_of(&w.conn, stash.account_id), balance(&w.conn, stash.account_id, TODAY) - held);
            if !stash.archived {
                let rate = w.conn.query_row(&format!("SELECT rate FROM accounts WHERE id = {}", stash.account_id), [], |r| r.get::<_, Option<String>>(0)).unwrap();
                summary_count += 1;
                summary_saved += accounts::in_main(saved.max(0), rate.as_deref());
                if let Some(t) = target {
                    summary_target += accounts::in_main(t, rate.as_deref());
                    summary_reached += u32::from(saved >= t);
                }
            }
        }
        for account in accounts::list(&w.conn, TODAY).unwrap() {
            let held: i64 = one(&w.conn, &format!("SELECT COALESCE(SUM(MAX(h, 0)), 0) FROM (SELECT (SELECT COALESCE(SUM(amount_minor), 0) FROM stash_entries WHERE stash_id = s.id) AS h FROM stashes s WHERE s.account_id = {})", account.id));
            let balance = balance(&w.conn, account.id, TODAY);
            assert_eq!(account.set_aside, crate::money::format_amount(held), "seed {seed}: set aside of {}", account.name);
            assert_eq!(account.free, crate::money::format_amount(balance - held));
            assert_eq!(account.balance, crate::money::format_amount(balance), "stashes move no balance");
            assert!(!account.set_aside.starts_with('-'), "never negative");
        }
        let total = summary(&w.conn, TODAY).unwrap();
        assert_eq!(
            (total.stashes, total.saved.as_str(), total.target.as_str(), total.reached),
            (summary_count, crate::money::format_amount(summary_saved).as_str(), crate::money::format_amount(summary_target).as_str(), summary_reached),
            "seed {seed}"
        );
    }
}

// ---- history and summary ---------------------------------------------------------------------------

#[test]
fn the_history_is_what_the_stash_held_at_the_end_of_each_month_up_to_today() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "5000.00");
    let id = stash(&w.conn, "Holiday", acc);
    put_on(&w.conn, id, "100.00", "2026-01-31");
    put_on(&w.conn, id, "50.00", "2026-02-01");
    put_on(&w.conn, id, "25.00", "2026-04-15");
    let out = pull(&w.conn, id, "30.00");
    assert!(out > 0);
    put_on(&w.conn, id, "10.00", "2025-12-31");
    let points = history(&w.conn, id, "2026", TODAY).unwrap();
    assert_eq!(points.len(), 11, "January to November; December has not begun");
    assert_eq!(points.iter().map(|p| p.month.as_str()).collect::<Vec<_>>(), ["2026-01", "2026-02", "2026-03", "2026-04", "2026-05", "2026-06", "2026-07", "2026-08", "2026-09", "2026-10", "2026-11"]);
    assert_eq!(
        points.iter().map(|p| p.saved.as_str()).collect::<Vec<_>>(),
        ["110.00", "160.00", "160.00", "185.00", "185.00", "185.00", "185.00", "185.00", "185.00", "185.00", "155.00"],
        "the entry of the 31st is in January, the one of the 1st in February, the take of today is in November (the current month ends on today)"
    );
    // the year before: all twelve, the last carrying what was saved
    let before = history(&w.conn, id, "2025", TODAY).unwrap();
    assert_eq!((before.len(), before[0].saved.as_str(), before[10].saved.as_str(), before[11].saved.as_str()), (12, "0.00", "0.00", "10.00"));
    // a year to come has no point; a year long ago has twelve of zero; a bad year is refused
    assert_eq!(history(&w.conn, id, "2027", TODAY).unwrap(), []);
    assert!(history(&w.conn, id, "1990", TODAY).unwrap().iter().all(|p| p.saved == "0.00"));
    assert!(history(&w.conn, id, "26", TODAY).is_err());
    assert!(history(&w.conn, id, "abcd", TODAY).is_err());
    // an entry a sync dated after today is not in today's number
    raw(&w.conn, id, 99_900, "2026-12-25");
    assert_eq!(history(&w.conn, id, "2026", TODAY).unwrap().last().unwrap().saved, "155.00");
    assert_eq!(history(&w.conn, id, "2026", "2027-01-15").unwrap().last().unwrap().saved, "1154.00", "and is in the history of the month it is dated in");
    // the stash that is empty
    let empty = stash(&w.conn, "Empty", acc);
    assert!(history(&w.conn, empty, "2026", TODAY).unwrap().iter().all(|p| p.saved == "0.00"));
}

#[test]
fn the_summary_counts_the_stashes_that_are_not_archived_in_the_app_currency_at_each_accounts_rate() {
    let w = world();
    assert_eq!(summary(&w.conn, TODAY).unwrap(), super::Summary { stashes: 0, saved: "0.00".into(), target: "0.00".into(), reached: 0 });
    let acc = funded(&w.conn, "Savings", "5000.00");
    let lei = foreign(&w.conn, "Lei", "RON", "5000.00");
    let (holiday, laptop, rainy, bike, gift) = (aimed(&w.conn, "Holiday", acc, "1200.00"), aimed(&w.conn, "Laptop", acc, "900.00"), stash(&w.conn, "Rainy day", acc), aimed(&w.conn, "Bike", acc, "450.00"), aimed(&w.conn, "Gift", lei, "1000.00"));
    put(&w.conn, holiday, "450.00");
    put(&w.conn, laptop, "900.00");
    put(&w.conn, rainy, "100.00");
    put(&w.conn, bike, "450.00");
    put(&w.conn, gift, "500.00");
    archive(&w.conn, bike, TODAY).unwrap();
    let s = summary(&w.conn, TODAY).unwrap();
    // held in euro: 450 + 900 + 100; the lei stash holds 500.00 RON, which is 100.00 EUR at a rate of 5.00; the archived bike is left out
    // targets: 1200 + 900 in euro and 1000 lei = 200.00; the rainy day has none
    assert_eq!((s.stashes, s.saved.as_str(), s.target.as_str(), s.reached), (4, "1550.00", "2300.00", 1), "{s:?}");
    // a stash below zero adds nothing and one above its target counts as reached
    let over = aimed(&w.conn, "Over", acc, "10.00");
    raw(&w.conn, over, 5_000, "2026-01-01");
    let down = stash(&w.conn, "Down", acc);
    raw(&w.conn, down, -9_999, "2026-01-01");
    let s = summary(&w.conn, TODAY).unwrap();
    assert_eq!((s.stashes, s.saved.as_str(), s.target.as_str(), s.reached), (6, "1600.00", "2310.00", 2), "{s:?}");
    // the summary of the numbers is the sum of the stashes that the list shows as not archived
    let by_hand: i64 = super::list(&w.conn, None, TODAY).unwrap().iter().filter(|s| !s.archived).map(|s| {
        let rate = if s.currency == "RON" { Some("5.00") } else { None };
        accounts::in_main(crate::money::parse_signed_amount(&s.saved).unwrap().max(0), rate)
    }).sum();
    assert_eq!(crate::money::format_amount(by_hand), s.saved);
    assert!(summary(&w.conn, "nope").is_err());
}

#[test]
fn a_stash_move_or_an_entry_never_moves_a_balance_a_statement_or_net_worth() {
    let w = world();
    let acc = funded(&w.conn, "Savings", "1000.00");
    let id = stash(&w.conn, "Holiday", acc);
    let before = (accounts::summary(&w.conn, TODAY).unwrap(), accounts::statement(&w.conn, acc, "2026-01-01", TODAY, TODAY).unwrap().lines.len(), accounts::history(&w.conn, acc, "2026", TODAY).unwrap());
    put(&w.conn, id, "300.00");
    pull(&w.conn, id, "100.00");
    let after = (accounts::summary(&w.conn, TODAY).unwrap(), accounts::statement(&w.conn, acc, "2026-01-01", TODAY, TODAY).unwrap().lines.len(), accounts::history(&w.conn, acc, "2026", TODAY).unwrap());
    assert_eq!(before, after);
    assert_eq!(one(&w.conn, "SELECT count(*) FROM account_movements"), 1, "only the opening balance moved the account");
}
