//! Transactions. Amounts are positive; direction comes from the category type.
//! `client_uuid` makes create idempotent: sending the same one twice returns the first row.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::accounts;
use crate::categories::{parse_id, present_or_null, serialize_id};
use crate::db::is_unique_violation;
use crate::debts::splits::{self, PartOut, Payer};
use crate::money::{format_amount, parse_amount, parse_positive_amount};
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
    /// The account the money left or reached (`docs/accounts-design.md`).
    #[serde(serialize_with = "serialize_id")]
    pub account_id: i64,
    pub client_uuid: Option<String>,
    pub name: String,
    /// What was paid, in the app's currency. Decimal string, e.g. "12.50".
    pub amount: String,
    /// What really left or reached a foreign account, in that account's currency (`docs/accounts-design.md`, section 21). None for a row in an
    /// account in the main currency, whatever the column holds.
    pub native_amount: Option<String>,
    /// What it costs you: the amount minus the parts of the friends and guests it is shared with (`transaction_costs`). The same as
    /// `amount` for a bill that is not split. Budgets, statistics and reports add up this, never `amount`.
    pub my_share: String,
    /// The parts of the others, when the bill is shared.
    pub split: Vec<PartOut>,
    /// The friend who paid the bill, when it was not you: then `my_share` is what you owe them and the parts are what the others owe them.
    pub paid_by: Option<Payer>,
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
    /// Left out means the main account.
    #[serde(default)]
    pub account_id: Option<String>,
    /// What really left or reached the account, in its currency: required for a foreign account, refused for any other (section 21.3).
    #[serde(default)]
    pub native_amount: Option<String>,
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
    /// Moves the transaction to another account, which must not be archived.
    #[serde(default)]
    pub account_id: Option<String>,
    /// In a foreign account it may change; into one it is required; into any other account, or for a row that stays in one, it is refused.
    #[serde(default)]
    pub native_amount: Option<String>,
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
    #[serde(default)]
    pub account_id: Option<String>,
    /// "INCOME" or "EXPENSE": only the transactions whose category has that type.
    #[serde(default)]
    pub kind: Option<String>,
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

pub(crate) fn clean_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    let length = name.chars().count();
    if length == 0 || length > NAME_MAX_CHARS || name.chars().any(char::is_control) {
        return Err(AppError::InvalidInput);
    }
    Ok(name.to_string())
}

/// A description or a note may hold line breaks and no other control character; `\r\n` and `\r` become `\n`. `None` when it holds another one.
pub(crate) fn clean_lines(text: &str) -> Option<String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    (!text.chars().any(|c| c.is_control() && c != '\n')).then_some(text)
}

/// Empty means "no description".
pub(crate) fn clean_description(text: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    let text = clean_lines(text).ok_or(AppError::InvalidInput)?;
    if text.chars().count() > DESCRIPTION_MAX_CHARS {
        return Err(AppError::InvalidInput);
    }
    Ok(Some(text))
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

/// The native amount is read only for a row in a foreign account: one left on a row in any other account is no part of it.
const COLUMNS: &str = "id, category_id, client_uuid, name, amount_minor, payment_date, description, account_id,
    CASE WHEN EXISTS (SELECT 1 FROM accounts a WHERE a.id = transactions.account_id AND a.currency IS NOT NULL) THEN native_amount_minor END";

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Transaction> {
    Ok(Transaction {
        id: row.get(0)?,
        category_id: row.get(1)?,
        client_uuid: row.get(2)?,
        name: row.get(3)?,
        amount: format_amount(row.get(4)?),
        native_amount: row.get::<_, Option<i64>>(8)?.map(format_amount),
        my_share: format_amount(row.get(4)?),
        split: Vec::new(),
        paid_by: None,
        payment_date: row.get(5)?,
        description: row.get(6)?,
        account_id: row.get(7)?,
    })
}

/// Fills in what the row alone does not say: what each transaction costs you and how it is shared. One query for each, whatever the number.
fn hydrate(conn: &Connection, transactions: &mut [Transaction]) -> Result<(), AppError> {
    let ids: Vec<i64> = transactions.iter().map(|t| t.id).collect();
    let costs = splits::costs_of(conn, &ids)?;
    let mut parts = splits::parts_of(conn, &ids)?;
    let mut payers = splits::payers_of(conn, &ids)?;
    for t in transactions {
        if let Some(cost) = costs.get(&t.id) {
            t.my_share = format_amount(*cost);
        }
        t.split = parts.remove(&t.id).unwrap_or_default();
        t.paid_by = payers.remove(&t.id);
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

pub(crate) fn category_is_expense(conn: &Connection, id: i64) -> Result<bool, AppError> {
    Ok(conn
        .query_row("SELECT 1 FROM categories WHERE id = ?1 AND type = 'EXPENSE'", [id], |_| Ok(()))
        .optional()?
        .is_some())
}

pub(crate) fn category_is_income(conn: &Connection, id: i64) -> Result<bool, AppError> {
    Ok(conn
        .query_row("SELECT 1 FROM categories WHERE id = ?1 AND type = 'INCOME'", [id], |_| Ok(()))
        .optional()?
        .is_some())
}

/// Creates a transaction, or returns the one already created with the same `client_uuid`.
pub fn create(conn: &Connection, input: &NewTransaction) -> Result<Transaction, AppError> {
    let name = clean_name(&input.name)?;
    let minor = parse_positive_amount(&input.amount)?;
    let native = input.native_amount.as_deref().map(parse_positive_amount).transpose()?;
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
    let account_id = accounts::for_new_row(conn, input.account_id.as_deref())?;
    // a foreign account needs what really moved it, and any other account has no use for one (section 21.3)
    if accounts::is_foreign(conn, account_id)? != native.is_some() {
        return Err(AppError::InvalidInput);
    }

    let inserted = conn.execute(
        "INSERT INTO transactions (category_id, client_uuid, name, amount_minor, payment_date, description, account_id, native_amount_minor)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![category_id, uuid, name, minor, date, description, account_id, native],
    );
    match inserted {
        Ok(_) => Ok(Transaction {
            id: conn.last_insert_rowid(),
            category_id,
            account_id,
            client_uuid: uuid,
            name,
            amount: format_amount(minor),
            native_amount: native.map(format_amount),
            my_share: format_amount(minor),
            split: Vec::new(),
            paid_by: None,
            payment_date: date,
            description,
        }),
        // Lost a race with the same uuid: hand back the winner.
        Err(e) if is_unique_violation(&e) => {
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
    let native_given = patch.native_amount.as_deref().map(parse_positive_amount).transpose()?;
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
    let account_id = accounts::for_moved_row(conn, current.account_id, patch.account_id.as_deref())?;
    // In a foreign account the native amount is the one given, else the one the row has; a row that comes from another account must be given
    // one. In any other account there is none, and one that is given is refused; a value left on the row is dropped (section 21.3).
    let native = if accounts::is_foreign(conn, account_id)? {
        match native_given {
            Some(native) => Some(native),
            None if accounts::is_foreign(conn, current.account_id)? => current.native_amount.as_deref().map(parse_amount).transpose()?,
            None => return Err(AppError::InvalidInput),
        }
    } else if native_given.is_some() {
        return Err(AppError::InvalidInput);
    } else {
        None
    };

    // A bill that is shared keeps its parts adding up to the bill: a new amount rescales every part, yours included, and a split bill
    // stays an expense. The parts are written before the amount when it shrinks and after it when it grows, so they never pass the bill.
    let old_minor = parse_positive_amount(&current.amount)?;
    let shared = !current.split.is_empty() || current.paid_by.is_some();
    if shared && category_id != current.category_id && !category_is_expense(conn, category_id)? {
        return Err(AppError::InvalidInput);
    }
    let write = |conn: &Connection| -> Result<(), AppError> {
        conn.execute(
            "UPDATE transactions
             SET name = ?1, amount_minor = ?2, payment_date = ?3, category_id = ?4, description = ?5, account_id = ?7, native_amount_minor = ?8
             WHERE id = ?6",
            params![name, minor, date, category_id, description, id, account_id, native],
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
    let account = filter.account_id.as_deref().map(parse_id).transpose()?;
    let kind = match filter.kind.as_deref() {
        Some(kind @ ("INCOME" | "EXPENSE")) => Some(kind),
        Some(_) => return Err(AppError::InvalidInput),
        None => None,
    };
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
    if let Some(account) = account {
        clauses.push("account_id = ?");
        values.push(Box::new(account));
    }
    if let Some(kind) = kind {
        clauses.push("category_id IN (SELECT id FROM categories WHERE type = ?)");
        values.push(Box::new(kind.to_string()));
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
            account_id: None,
            native_amount: None,
        }
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
            NewTransaction { description: Some("tab\there".into()), ..new(food, "A", "1.00", "2026-06-15") },
            NewTransaction { description: Some("nul\u{0}here".into()), ..new(food, "A", "1.00", "2026-06-15") },
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
    fn a_description_may_hold_line_breaks_and_a_name_may_not() {
        let (conn, food, _) = db();
        let input = NewTransaction { description: Some(" one\r\ntwo\rthree\nfour ".into()), ..new(food, "A", "1.00", "2026-06-15") };
        assert_eq!(create(&conn, &input).unwrap().description.as_deref(), Some("one\ntwo\nthree\nfour"));
        assert_eq!(create(&conn, &new(food, "A\nB", "1.00", "2026-06-15")), Err(AppError::InvalidInput));
        let long = NewTransaction { description: Some("x\r\n".repeat(128)), ..new(food, "A", "1.00", "2026-06-15") };
        assert!(create(&conn, &long).is_ok(), "the length counts after the breaks are made one character");
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
    fn list_text_search_treats_like_wildcards_literally() {
        let (conn, _, _) = seeded();
        assert_eq!(names(&conn, Filter { q: Some("100%".into()), ..Default::default() }), ["Pay"]);
        assert_eq!(names(&conn, Filter { q: Some("pay_day".into()), ..Default::default() }), ["Pay"]);
        assert!(names(&conn, Filter { q: Some("%".into()), ..Default::default() }).len() == 1, "a lone % matches only text containing %");
        assert!(names(&conn, Filter { q: Some("p_y".into()), ..Default::default() }).is_empty(), "_ is not a wildcard");
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

    fn of_kind(kind: &str) -> Filter {
        Filter { kind: Some(kind.into()), ..Default::default() }
    }

    // A category's type never changes (a trigger refuses it, `schema_tests`), so "the category now has that type" cannot differ from
    // "the category had that type when the row was made": there is no test of a category that changed type.
    #[test]
    fn list_filters_by_the_type_of_the_category_and_total_counts_the_same_set() {
        let (conn, _, _) = seeded();
        assert_eq!(names(&conn, of_kind("INCOME")), ["Pay"]);
        assert_eq!(names(&conn, of_kind("EXPENSE")), ["Aldi", "Lidl", "Tesco"]);
        assert_eq!(list(&conn, &of_kind("INCOME")).unwrap().total, 1);
        assert_eq!(list(&conn, &of_kind("EXPENSE")).unwrap().total, 3);
        assert_eq!(list(&conn, &Filter::default()).unwrap().total, 4, "no kind: both");
        // a page of the set: the total is the whole set, not the page
        let page = list(&conn, &Filter { limit: Some(2), offset: Some(1), ..of_kind("EXPENSE") }).unwrap();
        assert_eq!((page.transactions.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), page.total, page.limit, page.offset), (vec!["Lidl", "Tesco"], 3, 2, 1));
        assert_eq!(list(&conn, &Filter { offset: Some(5), ..of_kind("EXPENSE") }).unwrap().total, 3, "past the end: no rows, the same total");
        // a kind with nothing in it: no rows, a total of 0
        let (empty, _, salary) = seeded();
        empty.execute("DELETE FROM transactions WHERE category_id = ?1", [salary]).unwrap();
        let none = list(&empty, &of_kind("INCOME")).unwrap();
        assert_eq!((none.transactions.len(), none.total), (0, 0));
        assert_eq!(list(&empty, &of_kind("EXPENSE")).unwrap().total, 3);
        assert_eq!(list(&db().0, &of_kind("EXPENSE")).unwrap().total, 0);
    }

    #[test]
    fn the_kind_filter_combines_with_the_account_the_dates_the_category_and_the_search() {
        let (conn, food, salary) = seeded();
        conn.execute("INSERT INTO accounts (name, kind, opening_date) VALUES ('Cash', 'CASH', '2026-01-01')", []).unwrap();
        let rent = categories::create(&conn, "Rent", CategoryType::Expense).unwrap().id;
        let bonus = categories::create(&conn, "Bonus", CategoryType::Income).unwrap().id;
        let cash = |category: i64, name: &str, date: &str| {
            create(&conn, &NewTransaction { account_id: Some("2".into()), ..new(category, name, "5.00", date) }).unwrap();
        };
        cash(food, "Kiosk", "2026-06-15");
        cash(rent, "Pitch", "2026-07-02");
        cash(bonus, "Tip", "2026-06-20");
        let combined = |kind: &str, account: Option<&str>, from: Option<&str>, to: Option<&str>, category: Option<i64>, q: Option<&str>| {
            names(
                &conn,
                Filter {
                    kind: Some(kind.into()),
                    account_id: account.map(str::to_string),
                    from: from.map(str::to_string),
                    to: to.map(str::to_string),
                    category_id: category.map(|c| c.to_string()),
                    q: q.map(str::to_string),
                    ..Default::default()
                },
            )
        };
        assert_eq!(combined("EXPENSE", Some("2"), None, None, None, None), ["Pitch", "Kiosk"]);
        assert_eq!(combined("INCOME", Some("2"), None, None, None, None), ["Tip"]);
        assert_eq!(combined("INCOME", Some("1"), None, None, None, None), ["Pay"]);
        assert_eq!(combined("EXPENSE", Some("2"), Some("2026-07-01"), None, None, None), ["Pitch"]);
        assert_eq!(combined("EXPENSE", None, Some("2026-06-15"), Some("2026-06-15"), None, None), ["Kiosk", "Lidl"]);
        assert_eq!(combined("INCOME", None, Some("2026-06-15"), Some("2026-06-30"), None, None), ["Tip", "Pay"]);
        assert_eq!(combined("EXPENSE", None, None, None, None, Some("ki")), ["Kiosk"]);
        assert_eq!(combined("INCOME", None, None, None, None, Some("ki")), Vec::<String>::new(), "the search and the kind both hold");
        assert_eq!(combined("EXPENSE", None, None, None, Some(food), None), ["Aldi", "Kiosk", "Lidl", "Tesco"]);
        // a category of the other type matches nothing, and a category of this type matches its rows
        assert_eq!(combined("INCOME", None, None, None, Some(food), None), Vec::<String>::new());
        assert_eq!(combined("INCOME", None, None, None, Some(salary), None), ["Pay"]);
        // everything together
        assert_eq!(combined("EXPENSE", Some("2"), Some("2026-06-01"), Some("2026-06-30"), Some(food), Some("kiosk")), ["Kiosk"]);
    }

    #[test]
    fn the_kind_filter_keeps_the_rows_of_a_category_that_was_deleted() {
        let (mut conn, food, _) = seeded();
        categories::delete(&mut conn, food, chrono::Utc::now()).unwrap();
        assert_eq!(names(&conn, of_kind("EXPENSE")), ["Aldi", "Lidl", "Tesco"], "a deleted category still has its type");
    }

    #[test]
    fn a_kind_that_is_not_exactly_income_or_expense_is_refused() {
        let (conn, _, _) = seeded();
        for kind in ["income", "expense", "Income", "TRANSFER", "", " INCOME", "INCOME ", "ALL", "INCOME,EXPENSE", "1"] {
            assert_eq!(list(&conn, &of_kind(kind)).err(), Some(AppError::InvalidInput), "{kind:?}");
        }
        // the JSON the page sends: `kind` is text, and `null` means no filter
        assert_eq!(serde_json::from_str::<Filter>(r#"{"kind":"EXPENSE"}"#).unwrap().kind.as_deref(), Some("EXPENSE"));
        assert_eq!(serde_json::from_str::<Filter>(r#"{"kind":null}"#).unwrap().kind, None);
        assert!(serde_json::from_str::<Filter>(r#"{"kind":1}"#).is_err());
        // a bad kind is refused before any other filter is looked at, and an empty string is not "no filter"
        assert_eq!(list(&conn, &Filter { kind: Some("TRANSFER".into()), limit: Some(0), ..Default::default() }).err(), Some(AppError::InvalidInput));
    }
}
