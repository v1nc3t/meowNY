//! Dev only: how long the usual work takes with ten years of data.
//!
//!   cargo run --release --example bench            60 000 transactions (about 16 a day), 30 categories, a budget for every month
//!   cargo run --release --example bench -- 20000   another number of transactions
//!
//! It makes the data in memory (nothing on disk, nothing of the app's data is touched), then times the lists, the searches, the
//! budget and statistics queries, and a sync between two devices: the first copy to an empty device, a sync with nothing new, and
//! one with 500 changes. The first copy is also shown in pieces. Use `--release`: a debug build is many times slower.
use std::time::Instant;

use chrono::NaiveDate;
use meowny_lib::budgets::{self, Scope, SetBudget};
use meowny_lib::categories::{self, CategoryType};
use meowny_lib::migrate;
use meowny_lib::stats;
use meowny_lib::sync::{engine, run, stamp};
use meowny_lib::transactions::{self, Filter, NewTransaction};
use rusqlite::Connection;

fn device() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA temp_store = MEMORY; PRAGMA cache_size = -16384;").unwrap();
    migrate::run(&mut conn).unwrap();
    conn.execute("INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, 'EUR', 'en-IE', 'Europe/Dublin', 'en')", []).unwrap();
    conn
}

fn time<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    println!("{label:<48} {:>9.2} ms", start.elapsed().as_secs_f64() * 1000.0);
    out
}

fn main() {
    let count: usize = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(60_000);
    let a = &mut device();
    let mut cats = Vec::new();
    for i in 0..30 {
        cats.push(categories::create(a, &format!("Expense {i}"), CategoryType::Expense).unwrap().id);
    }
    let mut income = Vec::new();
    for i in 0..6 {
        income.push(categories::create(a, &format!("Income {i}"), CategoryType::Income).unwrap().id);
    }
    time("budgets: 30 categories x 118 months", || {
        for (i, cat) in cats.iter().enumerate() {
            for m in 0..118 {
                let (y, mo) = (2017 + m / 12, m % 12 + 1);
                budgets::set(a, &SetBudget { kind: CategoryType::Expense, scope: Scope::Category, month: format!("{y}-{mo:02}"), amount_limit: Some(format!("{}.00", 50 + i * 3 + (m % 7))), category_id: Some(cat.to_string()) }, chrono::Utc::now()).unwrap();
            }
        }
    });
    let start = NaiveDate::from_ymd_opt(2017, 1, 1).unwrap();
    time(&format!("create {count} transactions (one at a time)"), || {
        for i in 0..count {
            let day = start + chrono::Duration::days((i * 3500 / count) as i64);
            let (cat, name) = if i % 12 == 0 { (income[i % 6], "Salary") } else { (cats[(i * 7) % 30], if i % 3 == 0 { "Coffee" } else { "Groceries" }) };
            let input = NewTransaction { name: format!("{name} {}", i % 50), amount: format!("{}.{:02}", 1 + i % 90, i % 100), payment_date: day.to_string(), category_id: cat.to_string(), description: if i % 4 == 0 { Some(format!("note number {i} about something")) } else { None }, client_uuid: None };
            if let Err(e) = transactions::create(a, &input) { panic!("{e:?} for {input:?}"); }
        }
    });
    println!("--- reads");
    let page = |f: Filter| transactions::list(a, &f).unwrap();
    time("list: newest 50", || page(Filter::default()));
    time("list: offset 50000", || page(Filter { offset: Some(50000), ..Default::default() }));
    time("list: one category", || page(Filter { category_id: Some(cats[3].to_string()), ..Default::default() }));
    time("list: one month", || page(Filter { from: Some("2024-03-01".into()), to: Some("2024-03-31".into()), ..Default::default() }));
    time("list: search 'coffee 7'", || page(Filter { q: Some("coffee 7".into()), ..Default::default() }));
    time("list: search 'zzz' (no match)", || page(Filter { q: Some("zzz".into()), ..Default::default() }));
    time("budgets::overview this month", || budgets::overview(a, "2026-10").unwrap());
    time("budgets::history", || budgets::history(a, chrono::Utc::now()).unwrap());
    time("stats::budget_vs_actual", || stats::budget_vs_actual(a, "2026-10").unwrap());
    time("stats::year_trend", || stats::year_trend(a, "2025").unwrap());
    time("stats::top_categories", || stats::top_categories(a, "2026-10", CategoryType::Expense, 5).unwrap());
    time("stats::category_trend (10 years)", || stats::category_trend(a, cats[0], "2017-01", "2026-10").unwrap());
    time("stats::commitments", || stats::commitments(a).unwrap());
    time("stats::planned_vs_paid", || stats::planned_vs_paid(a, "2026-10").unwrap());
    println!("--- sync");
    let b = &mut device();
    let now = stamp::now_ms();
    time("digest (all rows)", || engine::digest(a).unwrap());
    let outcome = time("first copy: sync a -> empty b", || run::sync(a, b, "2026-10-15", now, None).unwrap());
    println!("   messages {} bytes {} diff {}", outcome.messages, outcome.bytes, outcome.differences);
    let outcome = time("second sync, nothing changed", || run::sync(a, b, "2026-10-15", now + 1, Some(now)).unwrap());
    println!("   messages {} bytes {}", outcome.messages, outcome.bytes);
    time("500 edits on a", || {
        for i in 0..500 {
            a.execute("UPDATE transactions SET amount_minor = amount_minor + 1 WHERE id = ?1", [i * 100 + 1]).unwrap();
        }
    });
    let outcome = time("sync with 500 changes", || run::sync(a, b, "2026-10-15", now + 2, Some(now + 1)).unwrap());
    println!("   messages {} bytes {} diff {}", outcome.messages, outcome.bytes, outcome.differences);
    println!("--- the first copy, in pieces");
    {
        use meowny_lib::sync::engine::RowsCollector;
        use meowny_lib::sync::wire::{Message, Table};
        let c = &mut device();
        let tables = Table::ALL.to_vec();
        let (da, dc) = (engine::digest(a).unwrap(), engine::digest(c).unwrap());
        let out = time("outgoing (read + build rows messages)", || engine::outgoing(a, &da, &dc, &tables).unwrap());
        let mut bytes = Vec::new();
        time("serialize messages", || for m in &out { bytes.push(m.to_bytes().unwrap()); });
        let total: usize = bytes.iter().map(|b| b.len()).sum();
        let back: Vec<Message> = time("parse messages", || bytes.iter().map(|b| Message::from_bytes(b).unwrap()).collect());
        let mut collector = RowsCollector::new(tables.clone());
        time("collect rows", || for m in back { if let Message::Rows(r) = m { collector.push(r).unwrap(); } });
        let incoming = collector.finish().unwrap();
        time("apply on the empty device", || engine::apply(c, &incoming, now + 10, None).unwrap());
        time("  (part of it) stamp::catch_up", || stamp::catch_up(c).unwrap());
        time("  (part of it) foreign_key_check", || c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get::<_, i64>(0)).unwrap());
        time("  (part of it) max stamp scan", || stamp::max_stamp(c).unwrap());
        println!("   {} messages, {} bytes ({} per transaction)", bytes.len(), total, total / 60000);
    }
    println!("db pages: {} x {}", a.query_row("PRAGMA page_count", [], |r| r.get::<_, i64>(0)).unwrap(), a.query_row("PRAGMA page_size", [], |r| r.get::<_, i64>(0)).unwrap());
}
