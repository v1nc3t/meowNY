//! Every figure about what you spent or earned counts your part of a shared bill. These tests keep it so: one reads the source and fails
//! when a query adds up `amount_minor` of the transactions, the others make random bills and compare every module with a plain
//! recomputation that uses the parts.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::fixtures::*;
use super::splits::{set_split, Mode};
use crate::categories::CategoryType;
use crate::{budgets, export, report, stats, transactions};

// ---- the guard --------------------------------------------------------------------------------

/// The string literals of a Rust file: what is between double quotes in code, not in comments or characters.
fn literals(source: &str) -> Vec<String> {
    let bytes: Vec<char> = source.chars().collect();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < bytes.len() {
        match bytes[i] {
            '/' if bytes.get(i + 1) == Some(&'/') => {
                while i < bytes.len() && bytes[i] != '\n' {
                    i += 1;
                }
            }
            '/' if bytes.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == '*' && bytes[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            // 'x' and '\n' and '"' are characters; a lifetime like 'a has no closing quote right after
            '\'' => {
                if bytes.get(i + 1) == Some(&'\\') {
                    i += 2;
                    while i < bytes.len() && bytes[i] != '\'' {
                        i += 1;
                    }
                    i += 1;
                } else if bytes.get(i + 2) == Some(&'\'') {
                    i += 3;
                } else {
                    i += 1;
                }
            }
            'r' if matches!(bytes.get(i + 1), Some('"') | Some('#')) => {
                let hashes = bytes[i + 1..].iter().take_while(|c| **c == '#').count();
                if bytes.get(i + 1 + hashes) == Some(&'"') {
                    let close: String = std::iter::once('"').chain(std::iter::repeat_n('#', hashes)).collect();
                    let start = i + 2 + hashes;
                    let rest: String = bytes[start..].iter().collect();
                    let end = rest.find(&close).expect("a raw string that ends");
                    out.push(rest[..end].to_string());
                    i = start + rest[..end].chars().count() + close.chars().count();
                } else {
                    i += 1;
                }
            }
            '"' => {
                let mut text = String::new();
                i += 1;
                while i < bytes.len() && bytes[i] != '"' {
                    if bytes[i] == '\\' {
                        i += 1;
                        if bytes.get(i) == Some(&'\n') {
                            // a line continuation drops the white space after it
                            while bytes.get(i).is_some_and(|c| c.is_whitespace()) {
                                i += 1;
                            }
                            continue;
                        }
                    }
                    if let Some(c) = bytes.get(i) {
                        text.push(*c);
                    }
                    i += 1;
                }
                i += 1;
                out.push(text);
            }
            _ => i += 1,
        }
    }
    out
}

fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let name = path.file_name().unwrap().to_str().unwrap();
            if !(name.ends_with("_tests.rs") || name == "tests.rs" || name == "fixtures.rs") {
                found.push(path);
            }
        }
    }
}

/// Queries that add up the plain amount of transactions on purpose, each with the reason.
const ALLOWED: &[(&str, &str)] = &[
    (
        "SUM(COALESCE(t.planned_amount_minor, t.amount_minor))",
        "planned against paid: both sides are the whole bill, because it is about how the price moved and a split changes none of that",
    ),
    ("SUM(l.amount_minor) > t.amount_minor", "the finished-state check: the parts of a bill against the bill, not a figure about spending"),
];

#[test]
fn no_query_adds_up_the_plain_amount_of_transactions_for_a_figure_about_spending() {
    let mut files = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    assert!(files.len() > 30, "the source files were not found: {files:?}");
    let mut seen_costs = 0;
    let mut matched: BTreeMap<&str, usize> = ALLOWED.iter().map(|(snippet, _)| (*snippet, 0)).collect();
    let mut offences = Vec::new();
    for path in files {
        let code = fs::read_to_string(&path).unwrap();
        // the tests of a file come last, from the first `#[cfg(test)]` on
        let production = code.split("#[cfg(test)]").next().unwrap();
        for literal in literals(production) {
            let lower = literal.to_lowercase();
            seen_costs += usize::from(lower.contains("transaction_costs"));
            let sums = lower.contains("sum(") && lower.contains("amount_minor");
            let reads_transactions = lower.contains("from transactions") || lower.contains("join transactions");
            if !(sums && reads_transactions) {
                continue;
            }
            match ALLOWED.iter().find(|(snippet, _)| literal.contains(snippet)) {
                Some((snippet, _)) => *matched.get_mut(snippet).unwrap() += 1,
                None => offences.push(format!("{}: {}", path.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap().display(), literal.split_whitespace().collect::<Vec<_>>().join(" "))),
            }
        }
    }
    assert!(offences.is_empty(), "a query adds up the plain amount of a transaction; use transaction_costs.cost_minor (or add it to ALLOWED with a reason):\n{}", offences.join("\n"));
    assert!(seen_costs >= 8, "the scan found only {seen_costs} queries that read the cost, so it is not looking at the right code");
    for (snippet, count) in matched {
        assert!(count >= 1, "{snippet} is allowed but no query has it any more: take it off the list");
    }
}

#[test]
fn the_guard_sees_what_it_is_for() {
    let bad = r##"let sql = "SELECT SUM(t.amount_minor) FROM transactions t"; // SUM(amount_minor) FROM transactions in a comment
        let ok = "SELECT SUM(t.cost_minor) FROM transaction_costs t";
        let raw = r#"SELECT SUM(amount_minor) FROM transactions"#;
        let c = '"'; let lifetime: &'static str = "x";"##;
    let found = literals(bad);
    assert_eq!(found.len(), 4, "{found:?}");
    assert!(found[0].contains("FROM transactions") && found[2].contains("SUM(amount_minor)"));
    assert_eq!(found[3], "x");
}

// ---- random bills against a plain recomputation -----------------------------------------------

struct Model {
    /// (month "YYYY-MM", category id, kind, cost minor)
    costs: Vec<(String, i64, CategoryType, i64)>,
}

impl Model {
    fn spent(&self, month: &str, kind: CategoryType) -> i64 {
        self.costs.iter().filter(|(m, _, k, _)| m == month && *k == kind).map(|c| c.3).sum()
    }
}

/// A database with random bills in two months, some shared with friends and guests in every mode, and what each should cost by hand.
fn random_world(seed: u64) -> (crate::debts::fixtures::World, Model) {
    let w = world();
    let mut rng = Rng(seed);
    let friends: Vec<i64> = (0..5).map(|i| friend(&w.conn, &format!("Friend {i}"))).collect();
    let mut model = Model { costs: Vec::new() };
    for i in 0..60 {
        let month = if i % 2 == 0 { "2026-03" } else { "2026-04" };
        let (category, kind) = match rng.between(0, 3) {
            0 => (w.salary, CategoryType::Income),
            1 => (w.rent, CategoryType::Expense),
            _ => (w.food, CategoryType::Expense),
        };
        let minor = rng.between(1, 500_000) as i64;
        let id = bill(&w.conn, category, &format!("Bill {i}"), &crate::money::format_amount(minor), &format!("{month}-{:02}", rng.between(1, 28)));
        let mut cost = minor;
        if kind == CategoryType::Expense && rng.between(0, 2) == 0 {
            let count = rng.between(1, 4) as usize;
            let mut others: Vec<_> = friends.iter().take(count).map(|f| who(*f)).collect();
            if rng.between(0, 2) == 0 {
                others.push(guest("Walk-in"));
            }
            let mode = match rng.between(0, 3) {
                0 => Mode::Equal,
                1 => Mode::Shares,
                2 => Mode::Percent,
                _ => Mode::Exact,
            };
            let typed: Vec<_> = match mode {
                Mode::Equal => others,
                Mode::Shares => others.into_iter().map(|p| with(p, &rng.between(0, 5).to_string())).collect(),
                Mode::Percent => others.into_iter().map(|p| with(p, &format!("{}.{:02}", rng.between(0, 15), rng.between(0, 99)))).collect(),
                Mode::Exact => {
                    let share = minor / 10;
                    others.into_iter().map(|p| with(p, &crate::money::format_amount(share))).collect()
                }
            };
            let mine = match mode {
                Mode::Equal => None,
                Mode::Shares => Some("1"),
                _ => None,
            };
            if set_split(&w.conn, id, &input(mode, mine, typed)).is_ok() {
                cost = minor - parts(&w.conn, id).iter().map(|p| p.1).sum::<i64>();
            }
        }
        model.costs.push((month.to_string(), category, kind, cost));
    }
    (w, model)
}

fn minor(text: &str) -> i64 {
    crate::money::parse_amount(&text.replace('-', "")).unwrap() * if text.starts_with('-') { -1 } else { 1 }
}

#[test]
fn every_module_counts_your_part_of_a_shared_bill() {
    for seed in 1..=40u64 {
        let (w, model) = random_world(seed * 7919);
        let conn: &Connection = &w.conn;
        let shared = one(conn, "SELECT count(DISTINCT transaction_id) FROM transaction_split_lines");
        assert!(shared > 0 || seed > 3, "the random bills should include shared ones");
        for month in ["2026-03", "2026-04"] {
            let expense = model.spent(month, CategoryType::Expense);
            let income = model.spent(month, CategoryType::Income);
            // the budget screens
            let overview = budgets::overview(conn, month).unwrap();
            let actual = |kind: CategoryType| minor(&overview.iter().find(|o| o.kind == kind).unwrap().actual);
            assert_eq!((actual(CategoryType::Expense), actual(CategoryType::Income)), (expense, income), "budgets {month} seed {seed}");
            // budget against actual, per category
            let view = stats::budget_vs_actual(conn, month).unwrap();
            for line in &view.categories {
                let by_hand: i64 = model.costs.iter().filter(|(m, c, _, _)| m == month && *c == line.category_id).map(|c| c.3).sum();
                assert_eq!(minor(&line.actual), by_hand, "{} {month} seed {seed}", line.name);
            }
            // the biggest categories
            let top = stats::top_categories(conn, month, CategoryType::Expense, 50).unwrap();
            assert_eq!(top.iter().map(|t| minor(&t.actual)).sum::<i64>(), expense, "top {month} seed {seed}");
            // one category month by month
            let trend = stats::category_trend(conn, w.food, month, month).unwrap();
            let food: i64 = model.costs.iter().filter(|(m, c, _, _)| m == month && *c == w.food).map(|c| c.3).sum();
            assert_eq!(minor(&trend.points[0].actual), food, "trend {month} seed {seed}");
        }
        // the year
        let year = stats::year_trend(conn, "2026").unwrap();
        let (income, expense): (i64, i64) = year.iter().map(|p| (minor(&p.income), minor(&p.expense))).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        assert_eq!((income, expense), (model.spent("2026-03", CategoryType::Income) + model.spent("2026-04", CategoryType::Income), model.spent("2026-03", CategoryType::Expense) + model.spent("2026-04", CategoryType::Expense)), "year seed {seed}");
        // the history of budgets starts at the first budget, so give it one
        budgets::set(conn, &budgets::SetBudget { kind: CategoryType::Expense, scope: budgets::Scope::Global, month: "2026-03".into(), amount_limit: Some("1000.00".into()), category_id: None }, "2026-01-15T00:00:00Z".parse().unwrap()).unwrap();
        let history = budgets::history(conn, "2026-04-30T12:00:00Z".parse().unwrap()).unwrap();
        for row in &history {
            assert_eq!(minor(&row.expense_actual), model.spent(&row.month, CategoryType::Expense), "history {} seed {seed}", row.month);
            assert_eq!(minor(&row.income_actual), model.spent(&row.month, CategoryType::Income), "history {} seed {seed}", row.month);
        }
        // the exports
        let all = export::Selection::default();
        let preview = export::preview(conn, &all).unwrap();
        let total = |kind| model.costs.iter().filter(|c| c.2 == kind).map(|c| c.3).sum::<i64>();
        assert_eq!((minor(&preview.income), minor(&preview.expense)), (total(CategoryType::Income), total(CategoryType::Expense)), "preview seed {seed}");
        let parsed = export::parse(&all).unwrap();
        let sums = export::monthly_sums(conn, &parsed).unwrap();
        for (month, (income, expense)) in sums {
            assert_eq!((income, expense), (model.spent(&month, CategoryType::Income), model.spent(&month, CategoryType::Expense)), "export {month} seed {seed}");
        }
        // the transaction list, the CSV and the JSON say both what was paid and what it cost
        let page = transactions::list(conn, &transactions::Filter { limit: Some(100), ..Default::default() }).unwrap();
        let listed: i64 = page.transactions.iter().filter(|t| t.name != "-").map(|t| minor(&t.my_share)).sum();
        let by_hand: i64 = model.costs.iter().map(|c| c.3).sum();
        assert_eq!(listed, by_hand, "list seed {seed}");
        for t in &page.transactions {
            assert_eq!(minor(&t.amount) - t.split.iter().map(|p| minor(&p.amount)).sum::<i64>(), minor(&t.my_share), "{} seed {seed}", t.name);
        }
        let json: serde_json::Value = serde_json::from_str(&export::json(conn, &all, "2026-05-01T00:00:00Z".parse().unwrap()).unwrap()).unwrap();
        let from_json: i64 = json["transactions"].as_array().unwrap().iter().map(|t| minor(t["myShare"].as_str().unwrap())).sum();
        assert_eq!(from_json, by_hand, "json seed {seed}");
        let csv = export::csv(conn, &all).unwrap();
        let from_csv: i64 = csv.lines().skip(1).map(|l| minor(l.trim_end().rsplit(',').next().unwrap())).sum();
        assert_eq!(from_csv, by_hand, "csv seed {seed}");
    }
}

#[test]
fn the_report_counts_your_part_too() {
    let (w, model) = random_world(424_242);
    let labels = report::ReportLabels::english("2026-03", "2026-04").unwrap();
    let options: report::ReportOptions = serde_json::from_value(serde_json::json!({
        "fromMonth": "2026-03", "toMonth": "2026-04", "includeTransactions": true,
        "labels": serde_json::json!({
            "title": labels.title, "period": labels.period, "income": labels.income, "expenses": labels.expenses, "savings": labels.savings,
            "savingsRate": labels.savings_rate, "byMonth": labels.by_month, "spending": labels.spending, "other": labels.other,
            "budgetVsActual": labels.budget_vs_actual, "noBudget": labels.no_budget, "transactions": labels.transactions, "date": labels.date,
            "category": labels.category, "name": labels.name, "amount": labels.amount, "page": labels.page, "noData": labels.no_data,
        }),
    }))
    .unwrap();
    let (income, expense, spending, listed) = report::read(&w.conn, &options).unwrap().figures();
    assert_eq!(income, model.spent("2026-03", CategoryType::Income) + model.spent("2026-04", CategoryType::Income));
    assert_eq!(expense, model.spent("2026-03", CategoryType::Expense) + model.spent("2026-04", CategoryType::Expense));
    assert_eq!(spending, expense, "where the money went adds up to the expenses");
    assert_eq!(listed, model.costs.iter().map(|c| c.3).sum::<i64>(), "and so does the list of transactions");
}

#[test]
fn what_a_recurring_item_costs_in_an_average_month_is_your_part() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let item = crate::recurring::create(
        &w.conn,
        &crate::recurring::NewRecurring {
            name: "Rent".into(),
            amount: Some("900.00".into()),
            category_id: w.rent.to_string(),
            start_date: "2026-01-01".into(),
            interval_unit: crate::recurring::Unit::Month,
            interval_count: 1,
            end_date: None,
            description: None,
        },
    )
    .unwrap();
    assert_eq!(stats::commitments(&w.conn).unwrap().expense, "900.00");
    crate::debts::splits::set_template(&w.conn, item.id, &equal(vec![who(anna)])).unwrap();
    let after = stats::commitments(&w.conn).unwrap();
    assert_eq!((after.expense.as_str(), after.items[0].per_month.as_deref()), ("450.00", Some("450.00")));
    crate::debts::splits::clear_template(&w.conn, item.id).unwrap();
    assert_eq!(stats::commitments(&w.conn).unwrap().expense, "900.00");
}

#[test]
fn planned_against_paid_stays_whole_because_a_split_does_not_move_the_price() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let item = crate::recurring::create(
        &w.conn,
        &crate::recurring::NewRecurring {
            name: "Power".into(),
            amount: Some("100.00".into()),
            category_id: w.rent.to_string(),
            start_date: "2026-03-01".into(),
            interval_unit: crate::recurring::Unit::Month,
            interval_count: 1,
            end_date: None,
            description: None,
        },
    )
    .unwrap();
    crate::debts::splits::set_template(&w.conn, item.id, &equal(vec![who(anna)])).unwrap();
    crate::recurring::pay(&w.conn, item.id, Some("120.00"), Some("2026-03-01"), "2026-03-01").unwrap();
    let line = &stats::planned_vs_paid(&w.conn, "2026-03").unwrap()[0];
    assert_eq!((line.planned.as_deref(), line.paid.as_str(), line.difference.as_deref()), (Some("100.00"), "120.00", Some("20.00")));
}

#[test]
fn a_filter_by_amount_would_still_use_what_was_paid_so_the_list_shows_both() {
    let w = world();
    let anna = friend(&w.conn, "Anna");
    let id = bill(&w.conn, w.food, "Dinner", "100.00", "2026-03-01");
    set_split(&w.conn, id, &equal(vec![who(anna)])).unwrap();
    let shown = &transactions::list(&w.conn, &Default::default()).unwrap().transactions[0];
    assert_eq!((shown.amount.as_str(), shown.my_share.as_str()), ("100.00", "50.00"));
    let json = serde_json::to_value(shown).unwrap();
    assert_eq!((json["amount"].as_str(), json["myShare"].as_str(), json["split"][0]["person"].as_str()), (Some("100.00"), Some("50.00"), Some("Anna")));
    // a bill that is not split says so with an empty list and the same two amounts
    let plain = bill(&w.conn, w.food, "Bread", "2.00", "2026-03-02");
    let plain = serde_json::to_value(transactions::get(&w.conn, plain).unwrap()).unwrap();
    assert_eq!((plain["amount"].as_str(), plain["myShare"].as_str(), plain["split"].as_array().map(Vec::len)), (Some("2.00"), Some("2.00"), Some(0)));
}
