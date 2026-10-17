//! What the tests of this module build their worlds from.

use rusqlite::Connection;

use super::splits::{Mode, Part, SplitInput};
use crate::categories::{self, CategoryType};
use crate::transactions::{self, NewTransaction};

pub(crate) struct World {
    pub conn: Connection,
    pub food: i64,
    pub rent: i64,
    pub salary: i64,
}

/// A migrated database in memory with the euro, two expense categories and an income one.
pub(crate) fn world() -> World {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::migrate::run(&mut conn).unwrap();
    conn.execute("INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, 'EUR', 'en-IE', 'UTC', 'en')", []).unwrap();
    let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
    let rent = categories::create(&conn, "Rent", CategoryType::Expense).unwrap().id;
    let salary = categories::create(&conn, "Salary", CategoryType::Income).unwrap().id;
    World { conn, food, rent, salary }
}

/// A transaction; the amount is text like "100.00". Returns its id.
pub(crate) fn bill(conn: &Connection, category: i64, name: &str, amount: &str, date: &str) -> i64 {
    transactions::create(
        conn,
        &NewTransaction {
            name: name.into(),
            amount: amount.into(),
            payment_date: date.into(),
            category_id: category.to_string(),
            description: None,
            client_uuid: None,
        },
    )
    .unwrap()
    .id
}

pub(crate) fn friend(conn: &Connection, name: &str) -> i64 {
    super::people::create(conn, name).unwrap().id
}

pub(crate) fn who(person: i64) -> Part {
    Part { person_id: Some(person.to_string()), guest: None, value: None }
}

pub(crate) fn guest(name: &str) -> Part {
    Part { person_id: None, guest: Some(name.into()), value: None }
}

pub(crate) fn with(mut part: Part, value: &str) -> Part {
    part.value = Some(value.into());
    part
}

pub(crate) fn equal(parts: Vec<Part>) -> SplitInput {
    SplitInput { mode: Mode::Equal, mine: None, parts }
}

pub(crate) fn input(mode: Mode, mine: Option<&str>, parts: Vec<Part>) -> SplitInput {
    SplitInput { mode, mine: mine.map(str::to_string), parts }
}

pub(crate) fn one(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

pub(crate) fn text(conn: &Connection, sql: &str) -> String {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// The cost of a transaction in minor units.
pub(crate) fn cost(conn: &Connection, id: i64) -> i64 {
    one(conn, &format!("SELECT cost_minor FROM transaction_costs WHERE transaction_id = {id}"))
}

/// The parts of a bill as (name or guest, minor units), in the order they were added.
pub(crate) fn parts(conn: &Connection, id: i64) -> Vec<(String, i64)> {
    let mut statement = conn
        .prepare("SELECT COALESCE(p.name, l.guest_name), l.amount_minor FROM transaction_split_lines l LEFT JOIN people p ON p.id = l.person_id WHERE l.transaction_id = ?1 ORDER BY l.id")
        .unwrap();
    statement.query_map([id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
}

/// A small deterministic generator (xorshift), so that the random tests give the same answer every run.
pub(crate) struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number in `low..=high`.
    pub fn between(&mut self, low: u64, high: u64) -> u64 {
        low + self.next() % (high - low + 1)
    }
}
