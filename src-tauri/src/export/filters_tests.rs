//! The choices an export offers (a range of dates, categories, income or expenses, an amount, a text, and which parts go in), tested
//! against a brute-force reading of the data: for hundreds of choices made by a fixed random sequence, every file an export can write
//! (CSV, JSON, Excel) and the preview must hold exactly what the choice names, and nothing else.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::documents_tests::{add_doc, attach_recurring, attach_transaction};
use super::tests::{db, parsed, read_csv, set_budget, spend};
use super::*;
use crate::budgets::Scope;
use crate::categories::{self, CategoryType};
use crate::debts::fixtures::{equal, friend, who};
use crate::debts::ledger::{self as ledger_entries, Kind as EntryKind, NewEntry};
use crate::debts::splits::set_split;

#[derive(Clone)]
pub(crate) struct Row {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) date: String,
    pub(crate) category: i64,
    /// What was paid.
    pub(crate) minor: i64,
    /// What it cost: less than was paid for a bill that is shared.
    pub(crate) cost: i64,
    /// Whether the bill is shared with a friend, and with whom (a friend's name).
    pub(crate) shared_with: Option<&'static str>,
}

/// What a document is attached to, as the test made it: a transaction (by its index in `rows`) or a recurring item (by its name).
#[derive(Clone)]
enum Link {
    Transaction(usize),
    Recurring(&'static str),
}

#[derive(Clone)]
struct Recurring {
    name: &'static str,
    category: i64,
    start: &'static str,
    end: Option<&'static str>,
    /// A paused item is not due.
    paused: bool,
}

#[derive(Clone)]
struct Budget {
    income: bool,
    category: Option<i64>,
    /// None: a row that removes the budget from its month on.
    limit: Option<i64>,
    month: String,
    scope: &'static str,
}

pub(crate) struct World {
    pub(crate) conn: Connection,
    pub(crate) rows: Vec<Row>,
    /// Every category, with whether it is income; the last one is deleted.
    pub(crate) categories: Vec<(i64, bool)>,
    recurring: Vec<Recurring>,
    /// (name, links) of each document.
    documents: Vec<(String, Vec<Link>)>,
    budgets: Vec<Budget>,
    /// The entries of the ledger: (date, friend).
    pub(crate) entries: Vec<(String, &'static str)>,
    /// Every friend there is.
    pub(crate) friends: Vec<&'static str>,
}

/// The same numbers every time: xorshift, which is plenty for choosing dates and categories.
pub(crate) struct Random(u64);

impl Random {
    pub(crate) fn new(seed: u64) -> Self {
        Random(seed | 1)
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Days that sit on an edge: the end and the start of a month, of a year and of a leap day, so a range that stops one day early or one
/// day late is seen.
const EDGES: [&str; 14] = [
    "2023-12-31", "2024-01-01", "2024-02-28", "2024-02-29", "2024-03-01", "2024-12-31", "2025-01-01", "2025-06-30", "2025-07-01",
    "2025-12-31", "2026-01-01", "2026-02-28", "2026-03-01", "2026-03-31",
];

/// Words in the descriptions, to search for. Two of them hold the wild characters of a `LIKE`.
const WORDS: [&str; 6] = ["coffee shop", "Rent-a-car", "100% organic", "snake_case", "ünïcode café", "plain"];

pub(crate) fn cents(minor: i64) -> String {
    format!("{}.{:02}", minor / 100, minor % 100)
}

/// 240 transactions in five categories (two of them income) and a sixth that is deleted afterwards, some of them shared with a friend,
/// budgets with changes and removals, recurring items that start and end at different times, and documents attached to some of it.
pub(crate) fn world() -> World {
    let conn = db();
    let mut random = Random::new(0x5eed_f11e);
    let mut categories_made: Vec<(i64, bool)> = Vec::new();
    for (name, kind) in [("Salary", CategoryType::Income), ("Freelance", CategoryType::Income), ("Rent", CategoryType::Expense), ("Food", CategoryType::Expense), ("Fun", CategoryType::Expense), ("Old", CategoryType::Expense)] {
        categories_made.push((categories::create(&conn, name, kind).unwrap().id, kind == CategoryType::Income));
    }
    let (anna, bob, _carol) = (friend(&conn, "Anna"), friend(&conn, "Bob"), friend(&conn, "Carol"));
    let mut dates: Vec<String> = EDGES.iter().map(|d| d.to_string()).collect();
    for _ in 0..60 {
        dates.push(format!("{}-{:02}-{:02}", 2023 + random.below(4), 1 + random.below(12), 1 + random.below(28)));
    }
    let mut rows = Vec::new();
    for at in 0..240 {
        let (category, _) = categories_made[random.below(categories_made.len())];
        let date = dates[random.below(dates.len())].clone();
        let minor = 1 + random.below(50_000) as i64;
        let name = format!("T{at:03}");
        let description = (random.below(100) < 40).then(|| format!("{} {at}", WORDS[random.below(WORDS.len())]));
        spend(&conn, category, &name, &cents(minor), &date, description.as_deref());
        // every sixth expense is shared with Anna and every tenth of the others with Bob, who have half (the odd cent stays with the
        // person); an income cannot be shared
        let (mut cost, mut shared_with) = (minor, None);
        let income = categories_made.iter().find(|c| c.0 == category).unwrap().1;
        if !income && (at % 6 == 0 || at % 10 == 3) {
            let (who_has, name_of) = if at % 6 == 0 { (anna, "Anna") } else { (bob, "Bob") };
            let id: i64 = conn.query_row("SELECT id FROM transactions WHERE name = ?1", [&name], |r| r.get(0)).unwrap();
            set_split(&conn, id, &equal(vec![who(who_has)])).unwrap();
            cost = minor - minor / 2;
            shared_with = Some(name_of);
        }
        rows.push(Row { name, description, date, category, minor, cost, shared_with });
    }
    let (salary, rent, food) = (categories_made[0].0, categories_made[2].0, categories_made[3].0);
    for (kind, scope, category, month, limit) in [
        (CategoryType::Expense, Scope::Category, Some(rent), "2024-01", Some("100.00")),
        (CategoryType::Expense, Scope::Category, Some(rent), "2024-06", Some("150.00")),
        (CategoryType::Expense, Scope::Category, Some(rent), "2025-01", None),
        (CategoryType::Expense, Scope::Category, Some(rent), "2025-06", Some("200.00")),
        (CategoryType::Expense, Scope::Category, Some(food), "2024-03", Some("80.00")),
        (CategoryType::Income, Scope::Category, Some(salary), "2024-02", Some("3000.00")),
        (CategoryType::Expense, Scope::Global, None, "2024-01", Some("2000.00")),
        (CategoryType::Expense, Scope::Global, None, "2025-06", Some("2100.00")),
    ] {
        set_budget(&conn, kind, scope, category, month, limit);
    }
    // the ledger: loans and repayments with Anna and Bob (Carol has none), on the edge days and others
    let mut entries: Vec<(String, &'static str)> = Vec::new();
    for at in 0..40 {
        let (person, name_of) = if at % 3 == 0 { (bob, "Bob") } else { (anna, "Anna") };
        let date = dates[random.below(dates.len())].clone();
        let kind = [EntryKind::Lent, EntryKind::Borrowed, EntryKind::RepaidToMe, EntryKind::RepaidByMe][random.below(4)];
        ledger_entries::add(&conn, &NewEntry { person_id: person.to_string(), kind, amount: cents(100 + random.below(5_000) as i64), date: date.clone(), note: None, transaction_id: None }).unwrap();
        entries.push((date, name_of));
    }
    // budgets that stay as they are until a later row changes them, which `set` does not write (it pins the next month): two rows of
    // Fun, so that a range can start in the middle of one
    let fun = categories_made[4].0;
    for (month, limit) in [("2024-01-01", 5_000), ("2025-01-01", 7_000)] {
        conn.execute(
            "INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from) VALUES ('EXPENSE', 'CATEGORY', ?1, ?2, ?3)",
            rusqlite::params![fun, limit, month],
        )
        .unwrap();
    }
    // three days from today, whenever the test runs
    let soon: &'static str = Box::leak(
        crate::clock::today(Utc::now(), "Europe/Dublin").unwrap().checked_add_days(chrono::Days::new(3)).unwrap().to_string().into_boxed_str(),
    );
    let mut recurring_made = Vec::new();
    for (name, index, start, end, paused) in [
        ("Pay", 0, "2025-01-01", None, false),
        ("Rent", 2, "2024-06-01", Some("2025-03-01"), false),
        ("Food box", 3, "2026-01-01", None, false),
        ("Gym", 4, "2023-01-01", Some("2023-12-31"), false),
        ("Old magazine", 5, "2025-06-01", None, false),
        // not due yet (one of them in a few days, one far off), and not due because it is paused
        ("Soon", 3, soon, None, false),
        ("Future", 2, "2099-01-01", None, false),
        ("Paused", 3, "2025-03-01", None, true),
    ] {
        let category = categories_made[index].0;
        crate::recurring::create(
            &conn,
            &crate::recurring::NewRecurring {
                name: name.into(),
                amount: Some("10.00".into()),
                category_id: category.to_string(),
                start_date: start.into(),
                interval_unit: crate::recurring::Unit::Month,
                interval_count: 1,
                end_date: end.map(String::from),
                description: None,
            },
        )
        .unwrap();
        if paused {
            let id: i64 = conn.query_row("SELECT id FROM recurring_transactions WHERE name = ?1", [name], |r| r.get(0)).unwrap();
            crate::recurring::set_active(&conn, id, false).unwrap();
        }
        recurring_made.push(Recurring { name, category, start, end, paused });
    }
    let mut documents = Vec::new();
    for (at, links) in [
        (0, vec![Link::Transaction(0)]),
        (1, vec![Link::Transaction(1), Link::Transaction(2)]),
        (2, vec![Link::Recurring("Rent")]),
        (3, vec![Link::Transaction(3), Link::Recurring("Food box")]),
        (4, vec![Link::Transaction(100), Link::Transaction(200)]),
        (5, vec![]),
        (6, vec![Link::Recurring("Old magazine")]),
        (7, vec![Link::Recurring("Gym"), Link::Transaction(7)]),
    ] {
        let name = format!("Doc {at}.pdf");
        let id = add_doc(&conn, &name, "2026-01-01");
        for link in &links {
            match link {
                Link::Transaction(index) => attach_transaction(&conn, id, &rows[*index].name, &rows[*index].date),
                Link::Recurring(item) => attach_recurring(&conn, id, item),
            }
        }
        documents.push((name, links));
    }
    let mut conn = conn;
    categories::delete(&mut conn, categories_made[5].0, Utc::now()).unwrap();
    // read after the deletion, which writes a budget row of its own
    let budgets: Vec<Budget> = {
        let mut statement = conn.prepare("SELECT type, scope, category_id, amount_limit_minor, effective_from FROM budgets ORDER BY id").unwrap();
        statement
            .query_map([], |r| {
                let (kind, scope, month): (String, String, String) = (r.get(0)?, r.get(1)?, r.get(4)?);
                Ok(Budget { income: kind == "INCOME", category: r.get(2)?, limit: r.get(3)?, month: month[..7].to_string(), scope: if scope == "GLOBAL" { "GLOBAL" } else { "CATEGORY" } })
            })
            .unwrap()
            .map(|b| b.unwrap())
            .collect()
    };
    World { conn, rows, categories: categories_made, recurring: recurring_made, documents, budgets, entries, friends: vec!["Anna", "Bob", "Carol"] }
}

const SECTIONS: [Section; 8] = [Section::Transactions, Section::Categories, Section::Budgets, Section::Recurring, Section::Documents, Section::People, Section::Ledger, Section::History];

/// Choices by a fixed random sequence: every mix of a first day, a last day, categories (including none, one that is deleted, one that
/// does not exist and one named twice), income or expenses, an amount range, a text, and the parts of the file.
pub(crate) fn random_selections(world: &World, count: usize, seed: u64) -> Vec<Selection> {
    let mut random = Random::new(seed);
    let mut dates: Vec<String> = world.rows.iter().map(|r| r.date.clone()).collect();
    dates.extend(["2020-01-01", "2030-12-31", "2024-06-15"].map(String::from));
    let amounts: Vec<i64> = world.rows.iter().map(|r| r.cost).chain([1, 100, 25_000, 99_999_999]).collect();
    let texts = ["T01", "t12", "coffee", "COFFEE SHOP", "100%", "_case", "rent-a", "café", "plain 1", "zzz", " T2 "];
    let mut out = Vec::new();
    for _ in 0..count {
        let pick_date = |random: &mut Random| (random.below(100) >= 25).then(|| dates[random.below(dates.len())].clone());
        let (mut from, mut to) = (pick_date(&mut random), pick_date(&mut random));
        if let (Some(a), Some(b)) = (&from, &to) {
            if a > b {
                std::mem::swap(&mut from, &mut to);
            }
        }
        let category_ids = match random.below(100) {
            0..=34 => None,
            35..=39 => Some(vec![]),
            _ => {
                let mut ids: Vec<String> = world.categories.iter().filter(|_| random.below(100) < 45).map(|c| c.0.to_string()).collect();
                if random.below(100) < 10 {
                    ids.push("9999".into());
                }
                if random.below(100) < 8 && !ids.is_empty() {
                    ids.push(ids[0].clone());
                }
                Some(ids)
            }
        };
        let kind = match random.below(10) {
            0..=1 => Some(Kind::Income),
            2..=3 => Some(Kind::Expense),
            _ => None,
        };
        let (mut low, mut high) = (random.below(100) < 30, random.below(100) < 30);
        let (mut a, mut b) = (amounts[random.below(amounts.len())], amounts[random.below(amounts.len())]);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        if !low && !high {
            (low, high) = (false, false);
        }
        let text = (random.below(100) < 20).then(|| texts[random.below(texts.len())].to_string());
        let sections = (random.below(100) < 45).then(|| {
            let mut list: Vec<Section> = SECTIONS.iter().copied().filter(|_| random.below(100) < 55).collect();
            if list.is_empty() {
                list.push(SECTIONS[random.below(SECTIONS.len())]);
            }
            list
        });
        let split = match random.below(10) {
            0..=1 => Some(SplitFilter::Split),
            2..=3 => Some(SplitFilter::NotSplit),
            _ => None,
        };
        let include_debts = random.below(100) < 40;
        out.push(Selection { from, to, category_ids, include_debts, kind, min_amount: low.then(|| cents(a)), max_amount: high.then(|| cents(b)), text, split, sections });
    }
    out
}

fn chosen(selection: &Selection) -> Option<BTreeSet<i64>> {
    selection.category_ids.as_ref().map(|ids| ids.iter().map(|id| id.parse().unwrap()).collect())
}

fn is_income(world: &World, category: i64) -> bool {
    world.categories.iter().find(|c| c.0 == category).unwrap().1
}

fn minor(text: &Option<String>) -> Option<i64> {
    text.as_deref().map(|t| t.replace('.', "").parse().unwrap())
}

/// The indexes of the transactions the selection names, in the order a file lists them: by date, then as they were made.
pub(crate) fn expected(world: &World, selection: &Selection) -> Vec<usize> {
    let categories = chosen(selection);
    let (least, most) = (minor(&selection.min_amount), minor(&selection.max_amount));
    let text = selection.text.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_lowercase);
    let mut found: Vec<usize> = (0..world.rows.len())
        .filter(|&at| {
            let row = &world.rows[at];
            selection.from.as_deref().is_none_or(|from| row.date.as_str() >= from)
                && selection.to.as_deref().is_none_or(|to| row.date.as_str() <= to)
                && categories.as_ref().is_none_or(|set| set.contains(&row.category))
                && selection.kind.is_none_or(|kind| (kind == Kind::Income) == is_income(world, row.category))
                && selection.split.is_none_or(|split| (split == SplitFilter::Split) == row.shared_with.is_some())
                && least.is_none_or(|least| row.cost >= least)
                && most.is_none_or(|most| row.cost <= most)
                // the search ignores case in the letters A to Z, as the search of the transactions page does
                && text.as_deref().is_none_or(|text| {
                    row.name.to_ascii_lowercase().contains(&text.to_ascii_lowercase())
                        || row.description.as_deref().is_some_and(|d| d.to_ascii_lowercase().contains(&text.to_ascii_lowercase()))
                })
        })
        .collect();
    found.sort_by(|a, b| (&world.rows[*a].date, a).cmp(&(&world.rows[*b].date, b)));
    found
}

/// Whether the selection limits anything but the parts of the file.
pub(crate) fn limited(selection: &Selection) -> bool {
    selection.from.is_some()
        || selection.to.is_some()
        || selection.category_ids.is_some()
        || selection.kind.is_some()
        || selection.min_amount.is_some()
        || selection.max_amount.is_some()
        || selection.split.is_some()
        || selection.text.as_deref().is_some_and(|t| !t.trim().is_empty())
}

pub(crate) fn wants(selection: &Selection, section: Section) -> bool {
    selection.sections.as_ref().is_none_or(|list| list.contains(&section))
}

/// The budget rows (by index) that a file holds, worked out another way than the program does: a row holds from its month until the
/// next row of the same budget, so with dates a row is wanted if that stretch touches the range (and it sets a budget), or if it is a row
/// that removes a budget in a month inside the range.
fn expected_budgets(world: &World, selection: &Selection) -> usize {
    let categories = chosen(selection);
    let first = selection.from.as_deref().map(|d| d[..7].to_string());
    let last = selection.to.as_deref().map(|d| d[..7].to_string());
    let mut count = 0;
    for (at, row) in world.budgets.iter().enumerate() {
        if let Some(set) = &categories {
            if !row.category.is_some_and(|c| set.contains(&c)) {
                continue;
            }
        }
        if selection.kind.is_some_and(|kind| (kind == Kind::Income) != row.income) {
            continue;
        }
        let next = world.budgets.iter().filter(|o| o.income == row.income && o.scope == row.scope && o.category == row.category && o.month > row.month).map(|o| o.month.clone()).min();
        let starts_in_time = last.as_ref().is_none_or(|last| row.month <= *last);
        let still_holds = first.as_ref().is_none_or(|first| next.as_ref().is_none_or(|next| next > first));
        let inside = first.as_ref().is_none_or(|first| row.month >= *first) && starts_in_time;
        let wanted = if row.limit.is_some() { starts_in_time && still_holds } else { inside };
        if wanted {
            count += 1;
        }
        let _ = at;
    }
    count
}

fn expected_recurring(world: &World, selection: &Selection) -> Vec<String> {
    let categories = chosen(selection);
    let mut names: Vec<String> = world
        .recurring
        .iter()
        .filter(|r| {
            categories.as_ref().is_none_or(|set| set.contains(&r.category))
                && selection.kind.is_none_or(|kind| (kind == Kind::Income) == is_income(world, r.category))
                && selection.from.as_deref().is_none_or(|from| r.end.is_none_or(|end| end >= from))
                && selection.to.as_deref().is_none_or(|to| r.start <= to)
        })
        .map(|r| r.name.to_string())
        .collect();
    names.sort();
    names
}

/// How many entries of the ledger a file holds: those in the dates, and only when the part is asked for and there is no limit or the
/// ledger was asked for (`include_debts`). The entries are about no category, so the categories never narrow them.
pub(crate) fn expected_entries(world: &World, selection: &Selection) -> Vec<&'static str> {
    let held = wants(selection, Section::Ledger) && (!limited(selection) || selection.include_debts);
    if !held {
        return Vec::new();
    }
    world
        .entries
        .iter()
        .filter(|(date, _)| selection.from.as_deref().is_none_or(|from| date.as_str() >= from) && selection.to.as_deref().is_none_or(|to| date.as_str() <= to))
        .map(|entry| entry.1)
        .collect()
}

/// How many friends a file names: everyone with no limit, otherwise those with a part in a bill of the file or an entry in it.
pub(crate) fn expected_friends(world: &World, selection: &Selection, want: &[usize]) -> usize {
    if !limited(selection) {
        return world.friends.len();
    }
    let mut named: BTreeSet<&str> = want.iter().filter_map(|&at| world.rows[at].shared_with).collect();
    named.extend(expected_entries(world, selection));
    named.len()
}

/// How many of the transactions are bills shared with a friend.
pub(crate) fn shared_among(world: &World, want: &[usize]) -> usize {
    want.iter().filter(|&&at| world.rows[at].shared_with.is_some()).count()
}

/// The names of the recurring payments that are due today and have not been paid or skipped: active, with the day they fall due on or
/// before today (none of the items here has been paid, so that is the day they start). The categories and the kind narrow them, the dates
/// do not.
pub(crate) fn expected_due(world: &World, selection: &Selection) -> Vec<&'static str> {
    if !wants(selection, Section::People) {
        return Vec::new();
    }
    let today = crate::clock::today(Utc::now(), "Europe/Dublin").unwrap().to_string();
    let categories = chosen(selection);
    let mut names: Vec<&'static str> = world
        .recurring
        .iter()
        .filter(|r| !r.paused && r.start <= today.as_str())
        .filter(|r| categories.as_ref().is_none_or(|set| set.contains(&r.category)))
        .filter(|r| selection.kind.is_none_or(|kind| (kind == Kind::Income) == is_income(world, r.category)))
        .map(|r| r.name)
        .collect();
    names.sort();
    names
}

pub(crate) fn describe(selection: &Selection) -> String {
    format!(
        "from {:?} to {:?} categories {:?} kind {:?} amount {:?}..{:?} text {:?} split {:?} debts {} sections {:?}",
        selection.from,
        selection.to,
        selection.category_ids,
        selection.kind.map(Kind::as_str),
        selection.min_amount,
        selection.max_amount,
        selection.text,
        selection.split.map(|s| if s == SplitFilter::Split { "split" } else { "not_split" }),
        selection.include_debts,
        selection.sections.as_ref().map(|s| s.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
    )
}

fn now() -> DateTime<Utc> {
    "2026-10-17T00:00:00Z".parse().unwrap()
}

#[test]
fn every_choice_gives_the_same_things_in_every_file_and_the_preview() {
    let world = world();
    let conn = &world.conn;
    let mut with_text = 0;
    for selection in random_selections(&world, 600, 0xd47e_5e1e) {
        let label = describe(&selection);
        let want = expected(&world, &selection);
        with_text += usize::from(selection.text.is_some() && !want.is_empty());
        let names: Vec<&str> = want.iter().map(|&at| world.rows[at].name.as_str()).collect();
        let categories = chosen(&selection);
        let everything = !limited(&selection);

        // the CSV is the transactions and takes no part in the choice of parts: a row for each, oldest first, with the right day, name,
        // amount paid and share
        let table = read_csv(&csv(conn, &selection).unwrap());
        assert_eq!(table.len() - 1, want.len(), "{label}");
        for (row, &at) in table[1..].iter().zip(&want) {
            let wanted = &world.rows[at];
            assert_eq!((row[0].as_str(), row[3].as_str(), row[4].as_str(), row[8].as_str()), (wanted.date.as_str(), wanted.name.as_str(), cents(wanted.minor).as_str(), cents(wanted.cost).as_str()), "{label}");
        }

        // the JSON holds the parts asked for, and only those
        let file = parsed(conn, &selection);
        let named = |value: &Value, key: &str| -> Vec<String> { value.as_array().unwrap().iter().map(|v| v[key].as_str().unwrap().to_string()).collect() };
        for (section, key) in [
            (Section::Transactions, "transactions"),
            (Section::Categories, "categories"),
            (Section::Budgets, "budgets"),
            (Section::Recurring, "recurring"),
            (Section::Documents, "documents"),
            (Section::People, "people"),
            (Section::Ledger, "ledger"),
        ] {
            assert_eq!(file.get(key).is_some(), wants(&selection, section), "{key} is there only when asked for, {label}");
        }
        assert_eq!(file.get("history").is_some(), everything && wants(&selection, Section::History), "history, {label}");
        if wants(&selection, Section::Transactions) {
            let in_json: Vec<&str> = file["transactions"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
            assert_eq!(in_json, names, "{label}");
            for (t, &at) in file["transactions"].as_array().unwrap().iter().zip(&want) {
                assert_eq!(t["myShare"].as_str(), Some(cents(world.rows[at].cost).as_str()), "{label}");
            }
        }
        let mut want_categories: Vec<i64> = world.categories.iter().filter(|c| selection.kind.is_none_or(|kind| (kind == Kind::Income) == c.1)).map(|c| c.0).filter(|id| categories.as_ref().is_none_or(|set| set.contains(id))).collect();
        want_categories.sort();
        if wants(&selection, Section::Categories) {
            assert_eq!(named(&file["categories"], "id"), want_categories.iter().map(|id| id.to_string()).collect::<Vec<_>>(), "categories, {label}");
        }
        let want_recurring = expected_recurring(&world, &selection);
        if wants(&selection, Section::Recurring) {
            let mut have = named(&file["recurring"], "name");
            have.sort();
            assert_eq!(have, want_recurring, "recurring, {label}");
        }
        let want_budgets = expected_budgets(&world, &selection);
        if wants(&selection, Section::Budgets) {
            assert_eq!(file["budgets"].as_array().unwrap().len(), want_budgets, "budgets, {label}");
        }

        // the friends and the history of debts: the entries by date only (a limit leaves them out unless they were asked for), the friends
        // by what the file holds
        let want_entries = expected_entries(&world, &selection);
        let want_friends = expected_friends(&world, &selection, &want);
        if wants(&selection, Section::People) {
            assert_eq!(file["people"].as_array().unwrap().len(), want_friends, "friends, {label}");
        }
        if wants(&selection, Section::Ledger) {
            assert_eq!(file["ledger"].as_array().unwrap().len(), want_entries.len(), "ledger, {label}");
        }
        // the recurring payments that have come due are listed with the friends
        let want_due = expected_due(&world, &selection);
        assert_eq!(file.get("duePayments").is_some(), wants(&selection, Section::People), "duePayments is there only with the friends, {label}");
        if wants(&selection, Section::People) {
            let mut have = named(&file["duePayments"], "name");
            have.sort();
            assert_eq!(have, want_due, "due payments, {label}");
        }

        // the documents: with a limit only those attached to something in the export, and only those attachments
        let mut want_documents: Vec<String> = Vec::new();
        for (name, links) in &world.documents {
            let kept = links
                .iter()
                .filter(|link| match link {
                    Link::Transaction(index) => want.contains(index),
                    Link::Recurring(item) => want_recurring.iter().any(|r| r == item),
                })
                .count();
            if everything || kept > 0 {
                want_documents.push(name.clone());
            }
        }
        want_documents.sort();
        if wants(&selection, Section::Documents) {
            let mut have = named(&file["documents"], "name");
            have.sort();
            assert_eq!(have, want_documents, "documents, {label}");
        }

        // the preview says what the files hold, and counts a part that is left out as nothing
        let preview = preview(conn, &selection).unwrap();
        let with_transactions = wants(&selection, Section::Transactions);
        let shown: Vec<usize> = if with_transactions { want.clone() } else { Vec::new() };
        assert_eq!(preview.transactions as usize, shown.len(), "{label}");
        assert_eq!(preview.first_date.as_deref(), shown.iter().map(|&at| world.rows[at].date.as_str()).min(), "{label}");
        assert_eq!(preview.last_date.as_deref(), shown.iter().map(|&at| world.rows[at].date.as_str()).max(), "{label}");
        let (income, expense) = shown.iter().fold((0, 0), |(i, e), &at| {
            let row = &world.rows[at];
            if is_income(&world, row.category) { (i + row.cost, e) } else { (i, e + row.cost) }
        });
        assert_eq!((preview.income, preview.expense), (cents(income), cents(expense)), "{label}");
        let count = |section: Section, n: usize| if wants(&selection, section) { n } else { 0 };
        assert_eq!(preview.categories as usize, count(Section::Categories, want_categories.len()), "{label}");
        assert_eq!(preview.recurring as usize, count(Section::Recurring, want_recurring.len()), "{label}");
        assert_eq!(preview.budgets as usize, count(Section::Budgets, want_budgets), "{label}");
        assert_eq!(preview.documents as usize, count(Section::Documents, want_documents.len()), "{label}");
        assert_eq!(preview.people as usize, count(Section::People, want_friends), "{label}");
        assert_eq!(preview.ledger as usize, want_entries.len(), "{label}");
        assert_eq!(preview.due_payments as usize, want_due.len(), "{label}");
        let shared = shared_among(&world, &want);
        assert_eq!(preview.split_bills as usize, if wants(&selection, Section::Transactions) || wants(&selection, Section::Ledger) { shared } else { 0 }, "{label}");

        // the months of the summary add up to the same
        let mut months: BTreeMap<String, (i64, i64)> = BTreeMap::new();
        for &at in &want {
            let row = &world.rows[at];
            let entry = months.entry(row.date[..7].to_string()).or_default();
            if is_income(&world, row.category) { entry.0 += row.cost } else { entry.1 += row.cost }
        }
        assert_eq!(monthly_sums(conn, &parse(&selection).unwrap()).unwrap(), months, "{label}");
    }
    assert!(with_text > 10, "the choices searched for text that was there often enough to mean something: {with_text}");
}

#[test]
fn a_range_of_one_day_gives_that_day_and_no_other_at_every_edge() {
    let world = world();
    for day in EDGES {
        let selection = Selection { from: Some(day.into()), to: Some(day.into()), ..Default::default() };
        let mut want: Vec<&str> = world.rows.iter().filter(|r| r.date == day).map(|r| r.name.as_str()).collect();
        let table = read_csv(&csv(&world.conn, &selection).unwrap());
        let mut have: Vec<&str> = table[1..].iter().map(|r| r[3].as_str()).collect();
        have.sort();
        want.sort();
        assert_eq!(have, want, "{day}");
        assert!(table[1..].iter().all(|r| r[0] == day), "{day}");
    }
}

#[test]
fn a_month_ends_on_its_own_last_day_in_a_leap_year_and_in_another() {
    let world = world();
    let count = |from: &str, to: &str| expected(&world, &Selection { from: Some(from.into()), to: Some(to.into()), ..Default::default() }).len();
    let on = |day: &str| world.rows.iter().filter(|r| r.date == day).count();
    assert!(on("2024-02-29") > 0 && on("2024-03-01") > 0 && on("2026-02-28") > 0, "the world has transactions on those edges");
    assert_eq!(count("2024-02-01", "2024-02-29") - count("2024-02-01", "2024-02-28"), on("2024-02-29"));
    assert_eq!(count("2026-02-01", "2026-02-28"), count("2026-02-01", "2026-03-01") - on("2026-03-01"));
    for (from, to) in [("2024-02-01", "2024-02-29"), ("2024-02-01", "2024-02-28"), ("2026-02-01", "2026-02-28"), ("2025-01-01", "2025-12-31"), ("2024-12-31", "2025-01-01")] {
        let selection = Selection { from: Some(from.into()), to: Some(to.into()), ..Default::default() };
        let table = read_csv(&csv(&world.conn, &selection).unwrap());
        assert_eq!(table.len() - 1, expected(&world, &selection).len(), "{from} to {to}");
    }
}

#[test]
fn a_range_outside_the_data_gives_header_rows_and_zeros_and_no_error() {
    let world = world();
    for (from, to) in [("2000-01-01", "2000-12-31"), ("2031-01-01", "2031-01-31")] {
        let selection = Selection { from: Some(from.into()), to: Some(to.into()), ..Default::default() };
        assert_eq!(read_csv(&csv(&world.conn, &selection).unwrap()).len(), 1);
        let preview = preview(&world.conn, &selection).unwrap();
        assert_eq!((preview.transactions, preview.first_date, preview.last_date, preview.income.as_str(), preview.expense.as_str()), (0, None, None, "0.00", "0.00"));
        let file = parsed(&world.conn, &selection);
        assert!(file["transactions"].as_array().unwrap().is_empty());
        assert_eq!(file["categories"].as_array().unwrap().len(), world.categories.len(), "a range alone hides no category");
    }
}

#[test]
fn the_budgets_of_a_range_are_those_that_applied_in_it() {
    let world = world();
    let months = |category: usize, from: Option<&str>, to: Option<&str>| -> Vec<String> {
        let selection = Selection {
            from: from.map(String::from),
            to: to.map(String::from),
            category_ids: Some(vec![world.categories[category].0.to_string()]),
            sections: Some(vec![Section::Budgets]),
            ..Default::default()
        };
        let file = parsed(&world.conn, &selection);
        file["budgets"].as_array().unwrap().iter().map(|b| format!("{} {}", b["month"].as_str().unwrap(), b["limit"].as_str().unwrap_or("removed"))).collect()
    };
    // Fun has two rows that stay until the next one changes them: 50.00 from 2024-01 and 70.00 from 2025-01
    let fun = |from: Option<&str>, to: Option<&str>| months(4, from, to);
    assert_eq!(fun(Some("2024-06-10"), Some("2024-08-31")), ["2024-01 50.00"], "the row in force when the range starts, though it began before it");
    assert_eq!(fun(Some("2024-12-01"), Some("2025-02-28")), ["2024-01 50.00", "2025-01 70.00"], "the one in force at the start, and the one that begins inside");
    assert_eq!(fun(Some("2025-01-01"), Some("2025-12-31")), ["2025-01 70.00"], "the first row was changed in the first month: it applied on none of the days");
    assert_eq!(fun(Some("2025-01-31"), Some("2025-01-31")), ["2025-01 70.00"]);
    assert_eq!(fun(Some("2023-01-01"), Some("2023-12-31")), Vec::<String>::new(), "before the first budget");
    assert_eq!(fun(None, Some("2024-12-31")), ["2024-01 50.00"], "one end only");
    assert_eq!(fun(Some("2025-06-01"), None), ["2025-01 70.00"]);
    assert_eq!(fun(None, None), ["2024-01 50.00", "2025-01 70.00"], "every date, every row");
    // Rent is set one month at a time: each `set` writes a row that ends it, and the removal rows are part of the history
    let rent = |from: Option<&str>, to: Option<&str>| months(2, from, to);
    assert_eq!(rent(Some("2024-03-10"), Some("2024-09-30")), ["2024-06 150.00", "2024-07 removed"], "the budget of January ended in February: nothing is in force in March");
    assert_eq!(rent(Some("2024-06-01"), Some("2024-06-30")), ["2024-06 150.00"]);
    assert_eq!(rent(Some("2024-12-31"), Some("2025-01-01")), ["2025-01 removed"], "a removal inside the range stays visible");
    assert_eq!(rent(Some("2025-02-01"), Some("2025-05-31")), Vec::<String>::new(), "nothing applied: the budget had been removed");
    assert_eq!(rent(Some("2025-07-01"), Some("2026-12-31")), ["2025-07 removed"]);
    assert_eq!(rent(None, Some("2024-12-31")).len(), 4);
    assert_eq!(rent(None, None).len(), 7);
}

#[test]
fn the_recurring_items_of_a_range_are_those_that_ran_in_it() {
    let world = world();
    let names = |from: Option<&str>, to: Option<&str>| -> Vec<String> {
        let selection = Selection { from: from.map(String::from), to: to.map(String::from), sections: Some(vec![Section::Recurring]), ..Default::default() };
        let mut found: Vec<String> = parsed(&world.conn, &selection)["recurring"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap().to_string()).collect();
        found.sort();
        found
    };
    assert_eq!(names(None, None).len(), 8);
    assert_eq!(names(Some("2023-06-01"), Some("2023-06-30")), ["Gym"], "the others had not started");
    assert_eq!(names(Some("2024-01-01"), Some("2024-05-31")), Vec::<String>::new(), "Gym had ended, Rent had not begun");
    assert_eq!(names(Some("2025-03-01"), Some("2025-03-01")), ["Paused", "Pay", "Rent"], "Rent ended that very day, and a paused item still ran");
    assert_eq!(names(Some("2025-03-02"), Some("2025-12-31")), ["Old magazine", "Paused", "Pay"]);
    assert_eq!(names(Some("2026-01-01"), None), ["Food box", "Future", "Old magazine", "Paused", "Pay", "Soon"]);
}

#[test]
fn income_or_expenses_only_narrows_every_part() {
    let world = world();
    for kind in [Kind::Income, Kind::Expense] {
        let selection = Selection { kind: Some(kind), ..Default::default() };
        let file = parsed(&world.conn, &selection);
        let income = kind == Kind::Income;
        assert!(file["transactions"].as_array().unwrap().iter().all(|t| (t["type"] == "INCOME") == income));
        assert!(file["categories"].as_array().unwrap().iter().all(|c| (c["type"] == "INCOME") == income));
        assert!(file["budgets"].as_array().unwrap().iter().all(|b| (b["type"] == "INCOME") == income));
        let ids: BTreeSet<String> = world.categories.iter().filter(|c| c.1 == income).map(|c| c.0.to_string()).collect();
        assert!(file["recurring"].as_array().unwrap().iter().all(|r| ids.contains(r["categoryId"].as_str().unwrap())));
        assert!(file.get("history").is_none(), "a limit, so no history");
    }
    // the global budget is for no kind of category, so it goes with neither
    let file = parsed(&world.conn, &Selection { kind: Some(Kind::Expense), ..Default::default() });
    assert!(file["budgets"].as_array().unwrap().iter().any(|b| b["scope"] == "GLOBAL"), "an expense budget of all spending is an expense budget");
}

#[test]
fn an_amount_range_is_about_what_it_cost_you_not_what_was_paid() {
    let world = world();
    // a shared bill: 90.00 paid with a friend is 45.00 to you
    let shared = world.rows.iter().find(|r| r.cost < r.minor && r.minor > 1000).unwrap();
    let cost = shared.cost;
    let with = |least: Option<i64>, most: Option<i64>| -> BTreeSet<String> {
        let selection = Selection { min_amount: least.map(cents), max_amount: most.map(cents), ..Default::default() };
        read_csv(&csv(&world.conn, &selection).unwrap())[1..].iter().map(|r| r[3].clone()).collect()
    };
    assert!(with(Some(cost), Some(cost)).contains(&shared.name), "at its cost: in");
    assert!(!with(Some(shared.minor), None).contains(&shared.name) || shared.minor == cost, "at what was paid and over: out");
    assert!(!with(Some(cost + 1), None).contains(&shared.name), "a cent over its cost");
    assert!(!with(None, Some(cost - 1)).contains(&shared.name), "a cent under its cost");
    assert_eq!(with(None, None).len(), world.rows.len());
}

#[test]
fn a_text_is_looked_for_in_the_name_and_the_description_and_the_wild_characters_are_only_themselves() {
    let world = world();
    let found = |text: &str| -> Vec<String> {
        let selection = Selection { text: Some(text.into()), ..Default::default() };
        read_csv(&csv(&world.conn, &selection).unwrap())[1..].iter().map(|r| r[3].clone()).collect()
    };
    let by_hand = |needle: &str| -> Vec<String> {
        let mut rows: Vec<&Row> = world.rows.iter().filter(|r| r.name.to_ascii_lowercase().contains(needle) || r.description.as_deref().is_some_and(|d| d.to_ascii_lowercase().contains(needle))).collect();
        rows.sort_by(|a, b| (&a.date, &a.name).cmp(&(&b.date, &b.name)));
        rows.into_iter().map(|r| r.name.clone()).collect()
    };
    for needle in ["coffee shop", "rent-a-car", "100% organic", "snake_case", "plain", "t03"] {
        let mut have = found(needle);
        have.sort();
        let mut want = by_hand(needle);
        want.sort();
        assert!(!want.is_empty(), "{needle}");
        assert_eq!(have, want, "{needle}");
        assert_eq!(found(&needle.to_uppercase()).len(), want.len(), "the letters A to Z match either way: {needle}");
    }
    // % and _ do not match anything: "%" finds only the descriptions that hold a percent sign
    let percent = found("%");
    assert!(percent.len() < world.rows.len() && !percent.is_empty());
    assert_eq!(percent.len(), by_hand("%").len());
    assert_eq!(found("_").len(), by_hand("_").len());
    assert_eq!(found("zzzz").len(), 0);
    assert_eq!(found("   ").len(), world.rows.len(), "only spaces is no text");
}

#[test]
fn the_parts_asked_for_are_the_only_parts_and_a_workbook_has_a_sheet_for_each() {
    let world = world();
    let only = |sections: &[Section]| Selection { sections: Some(sections.to_vec()), ..Default::default() };
    let file = parsed(&world.conn, &only(&[Section::Budgets]));
    let keys: BTreeSet<&str> = file.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, BTreeSet::from(["format", "version", "exportedAt", "scope", "settings", "budgets"]));
    assert_eq!(file["scope"]["sections"], json!(["budgets"]));
    // the history needs no limit and is asked for
    let history = parsed(&world.conn, &only(&[Section::History]));
    assert!(history.get("history").is_some() && history.get("transactions").is_none());
    let limited_history = parsed(&world.conn, &Selection { sections: Some(vec![Section::History, Section::Budgets]), kind: Some(Kind::Expense), ..Default::default() });
    assert!(limited_history.get("history").is_none(), "history mentions everything, so a limit leaves it out");
    // nothing asked for is refused
    assert_eq!(json(&world.conn, &Selection { sections: Some(vec![]), ..Default::default() }, now()), Err(AppError::InvalidInput));
    // a workbook of the history alone would have no sheet
    let book = |sections: &[Section]| workbook(&world.conn, &only(sections), &crate::display::DisplayFormat::default());
    assert_eq!(book(&[Section::History]), Err(AppError::InvalidInput));
    assert!(book(&[Section::Categories]).is_ok());
}

#[test]
fn a_deleted_category_can_be_chosen_and_keeps_its_transactions_and_its_mark() {
    let world = world();
    let old = world.categories[5].0;
    let selection = Selection { category_ids: Some(vec![old.to_string()]), ..Default::default() };
    let want = expected(&world, &selection);
    assert!(!want.is_empty());
    let file = parsed(&world.conn, &selection);
    assert_eq!(file["transactions"].as_array().unwrap().len(), want.len());
    let listed = file["categories"].as_array().unwrap();
    assert_eq!((listed.len(), listed[0]["deleted"].as_bool()), (1, Some(true)));
}

#[test]
fn a_category_named_twice_or_not_there_does_not_double_or_break_anything() {
    let world = world();
    let rent = world.categories[2].0.to_string();
    let once = Selection { category_ids: Some(vec![rent.clone()]), ..Default::default() };
    let twice = Selection { category_ids: Some(vec![rent.clone(), rent.clone(), "9999".into()]), ..Default::default() };
    assert_eq!(csv(&world.conn, &once).unwrap(), csv(&world.conn, &twice).unwrap());
    assert_eq!(parsed(&world.conn, &once)["transactions"], parsed(&world.conn, &twice)["transactions"]);
    assert_eq!(preview(&world.conn, &once).unwrap().transactions, preview(&world.conn, &twice).unwrap().transactions);
    let missing = Selection { category_ids: Some(vec!["9999".into()]), ..Default::default() };
    assert_eq!(read_csv(&csv(&world.conn, &missing).unwrap()).len(), 1);
}

#[test]
fn a_choice_that_cannot_be_is_refused_by_every_file_and_the_preview() {
    let world = world();
    let wrong = |edit: fn(&mut Selection)| {
        let mut selection = Selection::default();
        edit(&mut selection);
        selection
    };
    for bad in [
        wrong(|s| s.from = Some("2025-02-30".into())),
        wrong(|s| s.to = Some("2025-13-01".into())),
        wrong(|s| {
            s.from = Some("2026-01-02".into());
            s.to = Some("2026-01-01".into());
        }),
        wrong(|s| s.category_ids = Some(vec!["0".into()])),
        wrong(|s| s.category_ids = Some(vec!["1,2".into()])),
        wrong(|s| s.min_amount = Some("12".into())),
        wrong(|s| s.min_amount = Some("12.5".into())),
        wrong(|s| s.max_amount = Some("-1.00".into())),
        wrong(|s| s.max_amount = Some("abc".into())),
        wrong(|s| {
            s.min_amount = Some("20.00".into());
            s.max_amount = Some("19.99".into());
        }),
        wrong(|s| s.text = Some("a\u{0}b".into())),
        wrong(|s| s.text = Some("x".repeat(101))),
        wrong(|s| s.sections = Some(vec![])),
    ] {
        let label = describe(&bad);
        assert_eq!(csv(&world.conn, &bad), Err(AppError::InvalidInput), "{label}");
        assert_eq!(json(&world.conn, &bad, now()), Err(AppError::InvalidInput), "{label}");
        assert_eq!(preview(&world.conn, &bad).err(), Some(AppError::InvalidInput), "{label}");
        assert_eq!(workbook(&world.conn, &bad, &crate::display::DisplayFormat::default()).err(), Some(AppError::InvalidInput), "{label}");
        assert_eq!(preview_text(&world.conn, &bad, TextFormat::Csv, now()).err(), Some(AppError::InvalidInput), "{label}");
    }
    // the same amount for both ends, and a text of exactly 100 characters, are fine
    let fine = Selection { min_amount: Some("20.00".into()), max_amount: Some("20.00".into()), text: Some("x".repeat(100)), ..Default::default() };
    assert!(csv(&world.conn, &fine).is_ok());
}

#[test]
fn the_choice_the_screen_sends_is_read_as_it_is_written() {
    let sent = json!({
        "from": "2026-01-01", "to": null, "categoryIds": ["3", "4"], "kind": "EXPENSE", "minAmount": "1.00", "maxAmount": null,
        "text": "coffee", "sections": ["transactions", "budgets", "people", "history"]
    });
    let selection: Selection = serde_json::from_value(sent).unwrap();
    assert_eq!((selection.kind, selection.min_amount.as_deref(), selection.text.as_deref()), (Some(Kind::Expense), Some("1.00"), Some("coffee")));
    assert_eq!(selection.sections.as_ref().map(Vec::len), Some(4));
    for bad in [json!({ "kind": "BOTH" }), json!({ "sections": ["everything"] }), json!({ "colour": "red" }), json!({ "minAmount": 5 })] {
        assert!(serde_json::from_value::<Selection>(bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn the_recurring_payments_that_have_come_due_are_listed_with_the_debts() {
    let world = world();
    let due = |selection: &Selection| -> Vec<String> {
        let mut names: Vec<String> = parsed(&world.conn, selection)["duePayments"].as_array().unwrap().iter().map(|d| d["name"].as_str().unwrap().to_string()).collect();
        names.sort();
        names
    };
    // everything not paused and started: Soon is due in three days, Future begins in 2099 and Paused is paused
    let all = Selection::default();
    assert_eq!(due(&all), ["Food box", "Gym", "Old magazine", "Pay", "Rent"]);
    // the dates of an export do not narrow what is due, the categories and the kind do
    let old_dates = Selection { from: Some("2020-01-01".into()), to: Some("2020-12-31".into()), ..Default::default() };
    assert_eq!(due(&old_dates), due(&all));
    let rent = Selection { category_ids: Some(vec![world.categories[2].0.to_string()]), ..Default::default() };
    assert_eq!(due(&rent), ["Rent"], "Future is in the same category and is not due");
    assert_eq!(due(&Selection { kind: Some(Kind::Income), ..Default::default() }), ["Pay"]);
    // they go with the friends, and only then
    let ledger_only = parsed(&world.conn, &Selection { sections: Some(vec![Section::Ledger]), ..Default::default() });
    assert!(ledger_only.get("duePayments").is_none());
    // they are the same items the recurring screen calls overdue or due today, in the order of their days
    let today = crate::clock::today(Utc::now(), "Europe/Dublin").unwrap().to_string();
    let screen = crate::recurring::dues(&world.conn, &today, Some(0)).unwrap();
    let on_screen: Vec<String> = screen.overdue.iter().chain(&screen.upcoming).map(|r| r.name.clone()).collect();
    let in_file: Vec<String> = parsed(&world.conn, &all)["duePayments"].as_array().unwrap().iter().map(|d| d["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(in_file, on_screen);
    let file = parsed(&world.conn, &all);
    let days: Vec<&str> = file["duePayments"].as_array().unwrap().iter().map(|d| d["nextDueDate"].as_str().unwrap()).collect();
    assert!(days.windows(2).all(|pair| pair[0] <= pair[1]), "the oldest first: {days:?}");
}
