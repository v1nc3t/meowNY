//! Transactions. Amounts are positive; direction comes from the category type.
//! `client_uuid` makes create idempotent: sending the same one twice returns the first row.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Deserializer, Serialize};

use crate::categories::{parse_id, serialize_id};
use crate::debts::splits::{self, PartOut};
use crate::money::{format_amount, parse_positive_amount};
use crate::vault::AppError;

const NAME_MAX_CHARS: usize = 50;
const DESCRIPTION_MAX_CHARS: usize = 255;
const SEARCH_MAX_CHARS: usize = 255;
pub(crate) const DEFAULT_LIMIT: u32 = 50;
pub(crate) const MAX_LIMIT: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    #[serde(serialize_with = "serialize_id")]
    pub category_id: i64,
    pub client_uuid: Option<String>,
    pub name: String,
    /// What was paid. Decimal string, e.g. "12.50".
    pub amount: String,
    /// What it costs you: the amount minus the parts of the friends and guests it is shared with (`transaction_costs`). The same as
    /// `amount` for a bill that is not split. Budgets, statistics and reports add up this, never `amount`.
    pub my_share: String,
    /// The parts of the others, when the bill is shared.
    pub split: Vec<PartOut>,
    pub payment_date: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewTransaction {
    pub name: String,
    pub amount: String,
    pub payment_date: String,
    pub category_id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub client_uuid: Option<String>,
}

/// Fields left out stay as they are. `description: null` clears it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransactionPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub payment_date: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    pub description: Option<Option<String>>,
}

fn present_or_null<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::deserialize(deserializer)?))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Filter {
    /// Inclusive, "YYYY-MM-DD".
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    /// Matches the name or description, ignoring ASCII case.
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct Page {
    pub transactions: Vec<Transaction>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

fn no_controls(text: &str) -> bool {
    !text.chars().any(char::is_control)
}

pub(crate) fn clean_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    let length = name.chars().count();
    if length == 0 || length > NAME_MAX_CHARS || !no_controls(name) {
        return Err(AppError::InvalidInput);
    }
    Ok(name.to_string())
}

/// Empty means "no description".
pub(crate) fn clean_description(text: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    if text.chars().count() > DESCRIPTION_MAX_CHARS || !no_controls(text) {
        return Err(AppError::InvalidInput);
    }
    Ok(Some(text.to_string()))
}

pub fn clean_date(text: &str) -> Result<String, AppError> {
    let date = NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| AppError::InvalidInput)?;
    // chrono accepts "2026-6-5"; only the zero-padded form is a valid date for us.
    if date.format("%Y-%m-%d").to_string() != text {
        return Err(AppError::InvalidInput);
    }
    Ok(text.to_string())
}

pub(crate) fn clean_uuid(text: &str) -> Result<String, AppError> {
    let groups: Vec<&str> = text.split('-').collect();
    let shape = [8, 4, 4, 4, 12];
    let ok = groups.len() == 5
        && groups.iter().zip(shape).all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()));
    if ok {
        Ok(text.to_ascii_lowercase())
    } else {
        Err(AppError::InvalidInput)
    }
}

const COLUMNS: &str = "id, category_id, client_uuid, name, amount_minor, payment_date, description";

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Transaction> {
    Ok(Transaction {
        id: row.get(0)?,
        category_id: row.get(1)?,
        client_uuid: row.get(2)?,
        name: row.get(3)?,
        amount: format_amount(row.get(4)?),
        my_share: format_amount(row.get(4)?),
        split: Vec::new(),
        payment_date: row.get(5)?,
        description: row.get(6)?,
    })
}

/// Fills in what the row alone does not say: what each transaction costs you and how it is shared. One query for each, whatever the number.
fn hydrate(conn: &Connection, transactions: &mut [Transaction]) -> Result<(), AppError> {
    let ids: Vec<i64> = transactions.iter().map(|t| t.id).collect();
    let costs = splits::costs_of(conn, &ids)?;
    let mut parts = splits::parts_of(conn, &ids)?;
    for t in transactions {
        if let Some(cost) = costs.get(&t.id) {
            t.my_share = format_amount(*cost);
        }
        t.split = parts.remove(&t.id).unwrap_or_default();
    }
    Ok(())
}

fn find(conn: &Connection, id: i64) -> Result<Option<Transaction>, AppError> {
    let found = conn.query_row(&format!("SELECT {COLUMNS} FROM transactions WHERE id = ?1"), [id], from_row).optional()?;
    found
        .map(|t| {
            let mut one = [t];
            hydrate(conn, &mut one)?;
            let [t] = one;
            Ok(t)
        })
        .transpose()
}

fn find_by_uuid(conn: &Connection, uuid: &str) -> Result<Option<Transaction>, AppError> {
    let found = conn.query_row(&format!("SELECT {COLUMNS} FROM transactions WHERE client_uuid = ?1"), [uuid], from_row).optional()?;
    found
        .map(|t| {
            let mut one = [t];
            hydrate(conn, &mut one)?;
            let [t] = one;
            Ok(t)
        })
        .transpose()
}

pub(crate) fn category_is_active(conn: &Connection, id: i64) -> Result<bool, AppError> {
    Ok(conn
        .query_row("SELECT 1 FROM categories WHERE id = ?1 AND deleted_at IS NULL", [id], |_| Ok(()))
        .optional()?
        .is_some())
}

fn category_is_expense(conn: &Connection, id: i64) -> Result<bool, AppError> {
    Ok(conn
        .query_row("SELECT 1 FROM categories WHERE id = ?1 AND type = 'EXPENSE'", [id], |_| Ok(()))
        .optional()?
        .is_some())
}

fn is_uuid_conflict(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(e, _) if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

/// Creates a transaction, or returns the one already created with the same `client_uuid`.
pub fn create(conn: &Connection, input: &NewTransaction) -> Result<Transaction, AppError> {
    let name = clean_name(&input.name)?;
    let minor = parse_positive_amount(&input.amount)?;
    let date = clean_date(&input.payment_date)?;
    let category_id = parse_id(&input.category_id)?;
    let description = clean_description(input.description.as_deref())?;
    let uuid = input.client_uuid.as_deref().map(clean_uuid).transpose()?;

    if let Some(uuid) = &uuid {
        if let Some(existing) = find_by_uuid(conn, uuid)? {
            return Ok(existing);
        }
    }
    if !category_is_active(conn, category_id)? {
        return Err(AppError::InvalidInput);
    }

    let inserted = conn.execute(
        "INSERT INTO transactions (category_id, client_uuid, name, amount_minor, payment_date, description)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![category_id, uuid, name, minor, date, description],
    );
    match inserted {
        Ok(_) => Ok(Transaction {
            id: conn.last_insert_rowid(),
            category_id,
            client_uuid: uuid,
            name,
            amount: format_amount(minor),
            my_share: format_amount(minor),
            split: Vec::new(),
            payment_date: date,
            description,
        }),
        // Lost a race with the same uuid: hand back the winner.
        Err(e) if is_uuid_conflict(&e) => {
            uuid.as_deref().map(|u| find_by_uuid(conn, u)).transpose()?.flatten().ok_or(AppError::Internal)
        }
        Err(e) => Err(e.into()),
    }
}

pub fn get(conn: &Connection, id: i64) -> Result<Transaction, AppError> {
    find(conn, id)?.ok_or(AppError::NotFound)
}

pub fn update(conn: &Connection, id: i64, patch: &TransactionPatch) -> Result<Transaction, AppError> {
    let current = find(conn, id)?.ok_or(AppError::NotFound)?;

    let name = match &patch.name {
        Some(name) => clean_name(name)?,
        None => current.name.clone(),
    };
    let minor = match &patch.amount {
        Some(amount) => parse_positive_amount(amount)?,
        None => parse_positive_amount(&current.amount)?,
    };
    let date = match &patch.payment_date {
        Some(date) => clean_date(date)?,
        None => current.payment_date.clone(),
    };
    let category_id = match &patch.category_id {
        Some(category) => {
            let category_id = parse_id(category)?;
            if category_id != current.category_id && !category_is_active(conn, category_id)? {
                return Err(AppError::InvalidInput);
            }
            category_id
        }
        None => current.category_id,
    };
    let description = match &patch.description {
        Some(description) => clean_description(description.as_deref())?,
        None => current.description.clone(),
    };

    // A bill that is shared keeps its parts adding up to the bill: a new amount rescales every part, yours included, and a split bill
    // stays an expense. The parts are written before the amount when it shrinks and after it when it grows, so they never pass the bill.
    let old_minor = parse_positive_amount(&current.amount)?;
    let shared = !current.split.is_empty();
    if shared && category_id != current.category_id && !category_is_expense(conn, category_id)? {
        return Err(AppError::InvalidInput);
    }
    let write = |conn: &Connection| -> Result<(), AppError> {
        conn.execute(
            "UPDATE transactions
             SET name = ?1, amount_minor = ?2, payment_date = ?3, category_id = ?4, description = ?5
             WHERE id = ?6",
            params![name, minor, date, category_id, description, id],
        )?;
        Ok(())
    };
    if !shared {
        write(conn)?;
        return get(conn, id);
    }
    let tx = conn.unchecked_transaction()?;
    if minor < old_minor {
        splits::rescale(&tx, id, old_minor, minor)?;
    }
    write(&tx)?;
    if minor > old_minor {
        splits::rescale(&tx, id, old_minor, minor)?;
    }
    tx.commit()?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: i64) -> Result<(), AppError> {
    match conn.execute("DELETE FROM transactions WHERE id = ?1", [id])? {
        0 => Err(AppError::NotFound),
        _ => Ok(()),
    }
}

pub(crate) fn like_pattern(q: &str) -> String {
    let mut pattern = String::from("%");
    for c in q.chars() {
        if matches!(c, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

/// Newest payment date first, then newest id.
pub fn list(conn: &Connection, filter: &Filter) -> Result<Page, AppError> {
    let from = filter.from.as_deref().map(clean_date).transpose()?;
    let to = filter.to.as_deref().map(clean_date).transpose()?;
    let category = filter.category_id.as_deref().map(parse_id).transpose()?;
    let pattern = match filter.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) if q.chars().count() > SEARCH_MAX_CHARS => return Err(AppError::InvalidInput),
        Some(q) => Some(like_pattern(q)),
        None => None,
    };
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(AppError::InvalidInput);
    }
    let offset = filter.offset.unwrap_or(0);

    // Only the filters that are set go into the query, so SQLite can use an index for them
    // (a catch-all "? IS NULL OR column = ?" condition would force a scan).
    let mut clauses: Vec<&str> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(from) = from {
        clauses.push("payment_date >= ?");
        values.push(Box::new(from));
    }
    if let Some(to) = to {
        clauses.push("payment_date <= ?");
        values.push(Box::new(to));
    }
    if let Some(category) = category {
        clauses.push("category_id = ?");
        values.push(Box::new(category));
    }
    if let Some(pattern) = pattern {
        clauses.push("(name LIKE ? ESCAPE '\\' OR description LIKE ? ESCAPE '\\')");
        values.push(Box::new(pattern.clone()));
        values.push(Box::new(pattern));
    }
    let filter_sql = if clauses.is_empty() { String::new() } else { format!("WHERE {}", clauses.join(" AND ")) };

    let total: u32 = conn.query_row(
        &format!("SELECT count(*) FROM transactions {filter_sql}"),
        rusqlite::params_from_iter(values.iter().map(|v| v.as_ref())),
        |r| r.get(0),
    )?;
    let mut page_values: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    page_values.push(&limit);
    page_values.push(&offset);
    let mut statement = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM transactions {filter_sql} ORDER BY payment_date DESC, id DESC LIMIT ? OFFSET ?"
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(page_values), from_row)?;
    let mut transactions = rows.collect::<Result<Vec<_>, _>>()?;
    hydrate(conn, &mut transactions)?;
    Ok(Page { transactions, total, limit, offset })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::{self, CategoryType};

    const UUID: &str = "0b8f6c1e-5c52-4d6a-9d50-3f7a8a1b2c3d";

    fn db() -> (Connection, i64, i64) {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn.execute("INSERT INTO settings (id, currency, timezone) VALUES (1, 'EUR', 'Europe/Dublin')", []).unwrap();
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        let salary = categories::create(&conn, "Salary", CategoryType::Income).unwrap().id;
        (conn, food, salary)
    }

    fn new(category: i64, name: &str, amount: &str, date: &str) -> NewTransaction {
        NewTransaction {
            name: name.into(),
            amount: amount.into(),
            payment_date: date.into(),
            category_id: category.to_string(),
            description: None,
            client_uuid: None,
        }
    }

    #[test]
    fn create_returns_the_stored_values() {
        let (conn, food, _) = db();
        let t = create(&conn, &NewTransaction { description: Some("  weekly shop ".into()), ..new(food, " Tesco ", "42.10", "2026-06-15") }).unwrap();
        assert_eq!(t.name, "Tesco");
        assert_eq!(t.amount, "42.10");
        assert_eq!(t.description.as_deref(), Some("weekly shop"));
        assert_eq!(get(&conn, t.id).unwrap(), t);
        let stored: i64 = conn.query_row("SELECT amount_minor FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(stored, 4210, "stored as integer minor units");
    }

    #[test]
    fn create_rejects_bad_input() {
        let (conn, food, _) = db();
        let bad = [
            new(food, "", "1.00", "2026-06-15"),
            new(food, &"x".repeat(51), "1.00", "2026-06-15"),
            new(food, "A", "0.00", "2026-06-15"),
            new(food, "A", "1", "2026-06-15"),
            new(food, "A", "-1.00", "2026-06-15"),
            new(food, "A", "1.00", "2026-02-30"),
            new(food, "A", "1.00", "2026-6-5"),
            new(food, "A", "1.00", "15/06/2026"),
            NewTransaction { description: Some("x".repeat(256)), ..new(food, "A", "1.00", "2026-06-15") },
            NewTransaction { client_uuid: Some("not-a-uuid".into()), ..new(food, "A", "1.00", "2026-06-15") },
            NewTransaction { category_id: "abc".into(), ..new(food, "A", "1.00", "2026-06-15") },
        ];
        for input in bad {
            assert_eq!(create(&conn, &input), Err(AppError::InvalidInput), "{input:?}");
        }
        let count: i64 = conn.query_row("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn create_requires_an_active_category() {
        let (mut conn, food, _) = db();
        assert_eq!(create(&conn, &new(999, "A", "1.00", "2026-06-15")), Err(AppError::InvalidInput));
        categories::delete(&mut conn, food, "2026-06-15T10:00:00Z".parse().unwrap()).unwrap();
        assert_eq!(create(&conn, &new(food, "A", "1.00", "2026-06-15")), Err(AppError::InvalidInput));
    }

    #[test]
    fn same_client_uuid_creates_one_transaction() {
        let (conn, food, _) = db();
        let input = NewTransaction { client_uuid: Some(UUID.to_uppercase()), ..new(food, "Coffee", "3.50", "2026-06-15") };
        let first = create(&conn, &input).unwrap();
        let again = create(&conn, &input).unwrap();
        assert_eq!(first, again);
        assert_eq!(first.client_uuid.as_deref(), Some(UUID), "stored lowercase");

        // A retry that differs in content still returns the original, unchanged.
        let changed = NewTransaction { amount: "99.00".into(), ..input.clone() };
        assert_eq!(create(&conn, &changed).unwrap().amount, "3.50");

        let count: i64 = conn.query_row("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn replay_still_works_after_the_category_was_deleted() {
        let (mut conn, food, _) = db();
        let input = NewTransaction { client_uuid: Some(UUID.into()), ..new(food, "Coffee", "3.50", "2026-06-15") };
        let first = create(&conn, &input).unwrap();
        categories::delete(&mut conn, food, "2026-06-20T10:00:00Z".parse().unwrap()).unwrap();
        assert_eq!(create(&conn, &input).unwrap(), first);
    }

    #[test]
    fn transactions_without_a_uuid_never_collide() {
        let (conn, food, _) = db();
        create(&conn, &new(food, "A", "1.00", "2026-06-15")).unwrap();
        create(&conn, &new(food, "A", "1.00", "2026-06-15")).unwrap();
        assert_eq!(list(&conn, &Filter::default()).unwrap().total, 2);
    }

    #[test]
    fn update_changes_only_the_given_fields() {
        let (conn, food, salary) = db();
        let t = create(&conn, &NewTransaction { description: Some("note".into()), ..new(food, "A", "1.00", "2026-06-15") }).unwrap();

        let renamed = update(&conn, t.id, &TransactionPatch { name: Some("B".into()), ..Default::default() }).unwrap();
        assert_eq!((renamed.name.as_str(), renamed.amount.as_str(), renamed.description.as_deref()), ("B", "1.00", Some("note")));

        let moved = update(&conn, t.id, &TransactionPatch { category_id: Some(salary.to_string()), amount: Some("2.50".into()), ..Default::default() }).unwrap();
        assert_eq!((moved.category_id, moved.amount.as_str()), (salary, "2.50"));

        let cleared = update(&conn, t.id, &TransactionPatch { description: Some(None), ..Default::default() }).unwrap();
        assert_eq!(cleared.description, None);
    }

    #[test]
    fn update_validates_and_reports_missing() {
        let (conn, food, _) = db();
        let t = create(&conn, &new(food, "A", "1.00", "2026-06-15")).unwrap();
        for patch in [
            TransactionPatch { amount: Some("0.00".into()), ..Default::default() },
            TransactionPatch { payment_date: Some("nope".into()), ..Default::default() },
            TransactionPatch { category_id: Some("999".into()), ..Default::default() },
            TransactionPatch { name: Some(" ".into()), ..Default::default() },
        ] {
            assert_eq!(update(&conn, t.id, &patch), Err(AppError::InvalidInput), "{patch:?}");
        }
        assert_eq!(get(&conn, t.id).unwrap(), t, "a rejected update changes nothing");
        assert_eq!(update(&conn, 999, &TransactionPatch::default()), Err(AppError::NotFound));
    }

    #[test]
    fn update_keeps_a_deleted_category_when_it_is_not_changed() {
        let (mut conn, food, _) = db();
        let t = create(&conn, &new(food, "A", "1.00", "2026-06-15")).unwrap();
        categories::delete(&mut conn, food, "2026-06-20T10:00:00Z".parse().unwrap()).unwrap();
        let edited = update(&conn, t.id, &TransactionPatch { name: Some("B".into()), category_id: Some(food.to_string()), ..Default::default() }).unwrap();
        assert_eq!(edited.name, "B");
    }

    #[test]
    fn delete_removes_and_reports_missing() {
        let (conn, food, _) = db();
        let t = create(&conn, &new(food, "A", "1.00", "2026-06-15")).unwrap();
        delete(&conn, t.id).unwrap();
        assert_eq!(get(&conn, t.id), Err(AppError::NotFound));
        assert_eq!(delete(&conn, t.id), Err(AppError::NotFound));
    }

    fn seeded() -> (Connection, i64, i64) {
        let (conn, food, salary) = db();
        for (cat, name, amount, date, description) in [
            (food, "Tesco", "10.00", "2026-06-01", Some("weekly shop")),
            (food, "Lidl", "20.00", "2026-06-15", None),
            (salary, "Pay", "1000.00", "2026-06-15", Some("June 100% pay_day")),
            (food, "Aldi", "30.00", "2026-07-01", None),
        ] {
            create(&conn, &NewTransaction { description: description.map(String::from), ..new(cat, name, amount, date) }).unwrap();
        }
        (conn, food, salary)
    }

    fn names(conn: &Connection, filter: Filter) -> Vec<String> {
        list(conn, &filter).unwrap().transactions.into_iter().map(|t| t.name).collect()
    }

    #[test]
    fn list_is_newest_first_with_id_as_tie_breaker() {
        let (conn, _, _) = seeded();
        assert_eq!(names(&conn, Filter::default()), ["Aldi", "Pay", "Lidl", "Tesco"]);
    }

    #[test]
    fn list_filters_by_date_range_inclusively() {
        let (conn, _, _) = seeded();
        let june = Filter { from: Some("2026-06-01".into()), to: Some("2026-06-30".into()), ..Default::default() };
        assert_eq!(names(&conn, june), ["Pay", "Lidl", "Tesco"]);
        let from_only = Filter { from: Some("2026-06-15".into()), ..Default::default() };
        assert_eq!(names(&conn, from_only), ["Aldi", "Pay", "Lidl"]);
        let one_day = Filter { from: Some("2026-06-15".into()), to: Some("2026-06-15".into()), ..Default::default() };
        assert_eq!(names(&conn, one_day), ["Pay", "Lidl"]);
    }

    #[test]
    fn list_filters_by_category_and_text() {
        let (conn, food, salary) = seeded();
        assert_eq!(names(&conn, Filter { category_id: Some(salary.to_string()), ..Default::default() }), ["Pay"]);
        assert_eq!(names(&conn, Filter { category_id: Some(food.to_string()), q: Some("l".into()), ..Default::default() }), ["Aldi", "Lidl", "Tesco"]);
        assert_eq!(names(&conn, Filter { q: Some("WEEKLY".into()), ..Default::default() }), ["Tesco"], "name or description, any case");
    }

    #[test]
    fn list_text_search_treats_like_wildcards_literally() {
        let (conn, _, _) = seeded();
        assert_eq!(names(&conn, Filter { q: Some("100%".into()), ..Default::default() }), ["Pay"]);
        assert_eq!(names(&conn, Filter { q: Some("pay_day".into()), ..Default::default() }), ["Pay"]);
        assert!(names(&conn, Filter { q: Some("%".into()), ..Default::default() }).len() == 1, "a lone % matches only text containing %");
        assert!(names(&conn, Filter { q: Some("p_y".into()), ..Default::default() }).is_empty(), "_ is not a wildcard");
    }

    #[test]
    fn list_paginates_and_reports_the_total() {
        let (conn, _, _) = seeded();
        let first = list(&conn, &Filter { limit: Some(3), ..Default::default() }).unwrap();
        assert_eq!((first.transactions.len(), first.total, first.limit, first.offset), (3, 4, 3, 0));
        let second = list(&conn, &Filter { limit: Some(3), offset: Some(3), ..Default::default() }).unwrap();
        assert_eq!(second.transactions.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Tesco"]);
        assert_eq!(second.total, 4);
        let past_end = list(&conn, &Filter { offset: Some(50), ..Default::default() }).unwrap();
        assert!(past_end.transactions.is_empty());
    }

    #[test]
    fn list_rejects_bad_filters() {
        let (conn, _, _) = seeded();
        for filter in [
            Filter { limit: Some(0), ..Default::default() },
            Filter { limit: Some(101), ..Default::default() },
            Filter { from: Some("June".into()), ..Default::default() },
            Filter { category_id: Some("x".into()), ..Default::default() },
            Filter { q: Some("x".repeat(256)), ..Default::default() },
        ] {
            assert_eq!(list(&conn, &filter).err(), Some(AppError::InvalidInput), "{filter:?}");
        }
    }

    #[test]
    fn wire_format_matches_what_the_client_expects() {
        let (conn, food, _) = db();
        let t = create(&conn, &new(food, "A", "1.00", "2026-06-15")).unwrap();
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["id"], t.id.to_string());
        assert_eq!(json["categoryId"], food.to_string());
        assert_eq!(json["paymentDate"], "2026-06-15");
        assert_eq!(json["amount"], "1.00");
        assert!(json["clientUuid"].is_null());

        let patch: TransactionPatch = serde_json::from_str(r#"{"description": null}"#).unwrap();
        assert_eq!(patch.description, Some(None));
        let patch: TransactionPatch = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(patch.description, None);
        assert!(serde_json::from_str::<TransactionPatch>(r#"{"userId": 1}"#).is_err(), "unknown fields are refused");
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use crate::budgets::{self, Scope, SetBudget};
    use crate::categories::{self, CategoryType};
    use std::time::Instant;

    /// Not a pass/fail test: prints how long the main queries take on a large database.
    /// Run with `cargo test bench -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timings_on_a_large_database() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn.execute("INSERT INTO settings (id, currency, timezone) VALUES (1, 'EUR', 'UTC')", []).unwrap();
        let cats: Vec<i64> = (0..30).map(|i| categories::create(&conn, &format!("c{i}"), if i % 6 == 0 { CategoryType::Income } else { CategoryType::Expense }).unwrap().id).collect();
        let tx = conn.transaction().unwrap();
        for i in 0..50_000i64 {
            let month = 1 + (i % 60);
            let (year, m) = (2022 + (month - 1) / 12, 1 + (month - 1) % 12);
            tx.execute(
                "INSERT INTO transactions (category_id, name, amount_minor, payment_date, description) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![cats[(i % 30) as usize], format!("shop {i}"), 100 + i % 9000, format!("{year}-{m:02}-{:02}", 1 + i % 28), "weekly"],
            ).unwrap();
        }
        tx.commit().unwrap();
        for &c in &cats {
            let kind = if c % 6 == 1 { CategoryType::Income } else { CategoryType::Expense };
            let _ = budgets::set(&conn, &SetBudget { kind, scope: Scope::Category, month: "2022-01".into(), amount_limit: Some("100.00".into()), category_id: Some(c.to_string()) }, "2022-01-15T00:00:00Z".parse().unwrap());
        }
        let time = |label: &str, f: &dyn Fn()| {
            let start = Instant::now();
            f();
            println!("{label}: {:?}", start.elapsed());
        };
        time("list newest 50", &|| { list(&conn, &Filter::default()).unwrap(); });
        time("list page 500", &|| { list(&conn, &Filter { offset: Some(25_000), ..Default::default() }).unwrap(); });
        time("list one month", &|| { list(&conn, &Filter { from: Some("2025-03-01".into()), to: Some("2025-03-31".into()), ..Default::default() }).unwrap(); });
        time("list search", &|| { list(&conn, &Filter { q: Some("shop 4999".into()), ..Default::default() }).unwrap(); });
        time("overview one month", &|| { budgets::overview(&conn, "2025-03").unwrap(); });
        time("history 5 years", &|| { budgets::history(&conn, "2026-12-15T00:00:00Z".parse().unwrap()).unwrap(); });
        let plan: Vec<String> = conn
            .prepare("EXPLAIN QUERY PLAN SELECT id FROM transactions WHERE payment_date >= '2025-03-01' AND payment_date <= '2025-03-31' ORDER BY payment_date DESC, id DESC LIMIT 50")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        println!("plan: {plan:?}");
    }
}
