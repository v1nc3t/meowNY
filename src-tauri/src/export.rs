//! Plain-file exports: a JSON file with the data and a CSV of transactions. These files are not
//! encrypted (the encrypted copy is the backup), so the export screen says so before saving.
//!
//! A `Selection` narrows what goes in: a date range for transactions, and a set of categories for
//! everything that belongs to a category. With categories chosen, other category names, global
//! budgets and the change history are left out, so a limited export reveals nothing about the rest.
//! The device-specific scheduled-backup setup, which holds a passphrase, is never exported.

use std::borrow::Cow;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::categories::parse_id;
use crate::money::format_amount;
use crate::recurring;
use crate::transactions::clean_date;
use crate::vault::AppError;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    /// Inclusive, "YYYY-MM-DD". Narrows transactions.
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// Null means every category. An empty list means none.
    #[serde(default)]
    pub category_ids: Option<Vec<String>>,
}

struct Parsed {
    from: Option<String>,
    to: Option<String>,
    categories: Option<Vec<i64>>,
}

impl Parsed {
    fn everything(&self) -> bool {
        self.from.is_none() && self.to.is_none() && self.categories.is_none()
    }

    /// `AND`-ready SQL for a category column; ids are integers, so they are written in directly.
    fn category_filter(&self, column: &str) -> String {
        match &self.categories {
            None => "1".to_string(),
            Some(ids) => {
                let list: Vec<String> = ids.iter().map(i64::to_string).collect();
                format!("{column} IN ({})", list.join(","))
            }
        }
    }
}

fn parse(selection: &Selection) -> Result<Parsed, AppError> {
    let date = |d: &Option<String>| d.as_deref().map(clean_date).transpose();
    let (from, to) = (date(&selection.from)?, date(&selection.to)?);
    if matches!((&from, &to), (Some(f), Some(t)) if f > t) {
        return Err(AppError::InvalidInput);
    }
    let categories = selection
        .category_ids
        .as_ref()
        .map(|ids| ids.iter().map(|id| parse_id(id)).collect::<Result<Vec<_>, _>>())
        .transpose()?;
    Ok(Parsed { from, to, categories })
}

/// Spreadsheet programs run a cell that starts with one of these as a formula, so such a cell is
/// written with a `'` in front, which turns it into text. Tab and carriage return are included
/// because some programs skip them before reading the formula. Shared by CSV and Excel.
pub fn neutralise(cell: &str) -> Cow<'_, str> {
    if cell.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        Cow::Owned(format!("'{cell}"))
    } else {
        Cow::Borrowed(cell)
    }
}

struct Tx {
    id: i64,
    date: String,
    kind: String,
    category_id: i64,
    category: String,
    name: String,
    amount: i64,
    planned: Option<i64>,
    description: Option<String>,
    recurring_id: Option<i64>,
}

fn transactions(conn: &Connection, sel: &Parsed) -> Result<Vec<Tx>, AppError> {
    let mut statement = conn.prepare(&format!(
        "SELECT t.id, t.payment_date, c.type, t.category_id, c.name, t.name, t.amount_minor,
                t.planned_amount_minor, t.description, t.recurring_transaction_id
         FROM transactions t JOIN categories c ON c.id = t.category_id
         WHERE (?1 IS NULL OR t.payment_date >= ?1) AND (?2 IS NULL OR t.payment_date <= ?2)
           AND {}
         ORDER BY t.payment_date, t.id",
        sel.category_filter("t.category_id")
    ))?;
    let rows = statement.query_map(params![sel.from, sel.to], |r| {
        Ok(Tx {
            id: r.get(0)?,
            date: r.get(1)?,
            kind: r.get(2)?,
            category_id: r.get(3)?,
            category: r.get(4)?,
            name: r.get(5)?,
            amount: r.get(6)?,
            planned: r.get(7)?,
            description: r.get(8)?,
            recurring_id: r.get(9)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn currency(conn: &Connection) -> Result<String, AppError> {
    Ok(conn.query_row("SELECT currency FROM settings WHERE id = 1", [], |r| r.get(0))?)
}

fn csv_field(cell: &str) -> String {
    let cell = neutralise(cell);
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.into_owned()
    }
}

/// Transactions as CSV: a header row, then one row per transaction, oldest first. Amounts are
/// positive; `type` says whether it was income or an expense. The file starts with a byte order
/// mark so Excel reads the UTF-8 text correctly, and rows end with CRLF as the format says.
pub fn csv(conn: &Connection, selection: &Selection) -> Result<String, AppError> {
    let sel = parse(selection)?;
    let currency = currency(conn)?;
    let mut out = String::from("\u{feff}date,type,category,name,amount,currency,description\r\n");
    for t in transactions(conn, &sel)? {
        let cells = [
            t.date.as_str(),
            t.kind.as_str(),
            t.category.as_str(),
            t.name.as_str(),
            &format_amount(t.amount),
            currency.as_str(),
            t.description.as_deref().unwrap_or(""),
        ];
        let row: Vec<String> = cells.iter().map(|c| csv_field(c)).collect();
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }
    Ok(out)
}

fn categories_json(conn: &Connection, sel: &Parsed) -> Result<Vec<Value>, AppError> {
    let mut statement = conn.prepare(&format!(
        "SELECT id, type, name, deleted_at IS NOT NULL FROM categories WHERE {} ORDER BY id",
        sel.category_filter("id")
    ))?;
    let rows = statement.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?.to_string(),
            "type": r.get::<_, String>(1)?,
            "name": r.get::<_, String>(2)?,
            "deleted": r.get::<_, bool>(3)?,
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Every budget row, including the ones that remove a budget from a month on (`limit` null).
fn budgets_json(conn: &Connection, sel: &Parsed) -> Result<Vec<Value>, AppError> {
    // Global budgets belong to no category, so they only go out with an export of every category.
    let filter = if sel.categories.is_some() { sel.category_filter("category_id") } else { "1".to_string() };
    let mut statement = conn.prepare(&format!(
        "SELECT type, scope, category_id, amount_limit_minor, effective_from
         FROM budgets WHERE {filter} ORDER BY effective_from, id"
    ))?;
    let rows = statement.query_map([], |r| {
        Ok(json!({
            "type": r.get::<_, String>(0)?,
            "scope": r.get::<_, String>(1)?,
            "categoryId": r.get::<_, Option<i64>>(2)?.map(|id| id.to_string()),
            "month": r.get::<_, String>(4)?[..7].to_string(),
            "limit": r.get::<_, Option<i64>>(3)?.map(format_amount),
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn recurring_json(conn: &Connection, sel: &Parsed) -> Result<Vec<Value>, AppError> {
    recurring::list(conn)?
        .into_iter()
        .filter(|item| sel.categories.as_ref().is_none_or(|ids| ids.contains(&item.category_id)))
        .map(|item| serde_json::to_value(item).map_err(|_| AppError::Internal))
        .collect()
}

/// The change history as recorded; the old and new values are embedded as objects, not as text.
fn history_json(conn: &Connection) -> Result<Vec<Value>, AppError> {
    let embed = |text: Option<String>| text.and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or(Value::Null);
    let mut statement = conn.prepare(
        "SELECT entity_type, entity_id, action, old_values, new_values, changed_at FROM audit_log ORDER BY id",
    )?;
    let rows = statement.query_map([], |r| {
        Ok(json!({
            "entity": r.get::<_, String>(0)?,
            "entityId": r.get::<_, String>(1)?,
            "action": r.get::<_, String>(2)?,
            "oldValues": embed(r.get(3)?),
            "newValues": embed(r.get(4)?),
            "changedAt": r.get::<_, String>(5)?,
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// The data as one JSON file. Amounts are decimal strings and ids are strings, as in the app's own
/// commands. With no selection it is everything, including the change history; with one it is only
/// what the selection reaches.
pub fn json(conn: &Connection, selection: &Selection, now: DateTime<Utc>) -> Result<String, AppError> {
    let sel = parse(selection)?;
    let tx = conn.unchecked_transaction()?;
    let (currency, locale, timezone, language): (String, String, String, String) = tx.query_row(
        "SELECT currency, locale, timezone, language FROM settings WHERE id = 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let transactions: Vec<Value> = transactions(&tx, &sel)?
        .into_iter()
        .map(|t| {
            json!({
                "id": t.id.to_string(),
                "date": t.date,
                "type": t.kind,
                "categoryId": t.category_id.to_string(),
                "category": t.category,
                "name": t.name,
                "amount": format_amount(t.amount),
                "plannedAmount": t.planned.map(format_amount),
                "description": t.description,
                "recurringId": t.recurring_id.map(|id| id.to_string()),
            })
        })
        .collect();

    let mut file = json!({
        "format": "meowny-export",
        "version": 1,
        "exportedAt": now.to_rfc3339_opts(SecondsFormat::Secs, true),
        "scope": { "from": sel.from, "to": sel.to, "categoryIds": sel.categories.as_ref().map(|ids| ids.iter().map(|i| i.to_string()).collect::<Vec<_>>()) },
        "settings": { "currency": currency, "locale": locale, "timezone": timezone, "language": language },
        "categories": categories_json(&tx, &sel)?,
        "budgets": budgets_json(&tx, &sel)?,
        "recurring": recurring_json(&tx, &sel)?,
        "transactions": transactions,
    });
    if sel.everything() {
        file["history"] = Value::Array(history_json(&tx)?);
    }
    serde_json::to_string_pretty(&file).map_err(|_| AppError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budgets::{self, Scope, SetBudget};
    use crate::categories::{self, CategoryType};
    use crate::transactions::{self, NewTransaction};

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrate::run(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO settings (id, currency, locale, timezone, language) VALUES (1, 'EUR', 'en-IE', 'Europe/Dublin', 'en')",
            [],
        )
        .unwrap();
        conn
    }

    fn spend(conn: &Connection, category: i64, name: &str, amount: &str, date: &str, description: Option<&str>) {
        transactions::create(
            conn,
            &NewTransaction {
                name: name.into(),
                amount: amount.into(),
                payment_date: date.into(),
                category_id: category.to_string(),
                description: description.map(str::to_string),
                client_uuid: None,
            },
        )
        .unwrap();
    }

    fn set_budget(conn: &Connection, kind: CategoryType, scope: Scope, category: Option<i64>, month: &str, limit: Option<&str>) {
        budgets::set(
            conn,
            &SetBudget {
                kind,
                scope,
                month: month.into(),
                amount_limit: limit.map(str::to_string),
                category_id: category.map(|c| c.to_string()),
            },
            Utc::now(),
        )
        .unwrap();
    }

    /// Salary and rent (with a budget and a recurring item) plus a private category for the filter tests.
    fn lived_in() -> (Connection, i64, i64, i64) {
        let conn = db();
        let salary = categories::create(&conn, "Salary", CategoryType::Income).unwrap().id;
        let rent = categories::create(&conn, "Rent", CategoryType::Expense).unwrap().id;
        let secret = categories::create(&conn, "Therapy", CategoryType::Expense).unwrap().id;
        set_budget(&conn, CategoryType::Expense, Scope::Global, None, "2026-01", Some("2200.00"));
        set_budget(&conn, CategoryType::Expense, Scope::Category, Some(rent), "2026-01", Some("900.00"));
        set_budget(&conn, CategoryType::Expense, Scope::Category, Some(secret), "2026-01", Some("300.00"));
        set_budget(&conn, CategoryType::Expense, Scope::Category, Some(secret), "2026-03", None);
        for (category, name) in [(rent, "Rent"), (secret, "Therapy plan")] {
            recurring::create(
                &conn,
                &recurring::NewRecurring {
                    name: name.into(),
                    amount: "100.00".into(),
                    category_id: category.to_string(),
                    start_date: "2026-01-01".into(),
                    interval_unit: recurring::Unit::Month,
                    interval_count: 1,
                    end_date: None,
                    description: None,
                },
            )
            .unwrap();
        }
        spend(&conn, salary, "Pay", "3200.00", "2026-01-25", Some("Monthly pay"));
        spend(&conn, rent, "Rent", "900.00", "2026-02-01", None);
        spend(&conn, rent, "Rent", "905.50", "2026-03-01", None);
        spend(&conn, secret, "Session", "80.00", "2026-02-10", Some("Dr. Private"));
        (conn, salary, rent, secret)
    }

    /// A small reader for the CSV the app writes, to check the quoting by reading it back.
    fn read_csv(text: &str) -> Vec<Vec<String>> {
        let text = text.strip_prefix('\u{feff}').unwrap();
        let (mut rows, mut row, mut cell, mut quoted) = (Vec::new(), Vec::new(), String::new(), false);
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match (c, quoted) {
                ('"', true) if chars.peek() == Some(&'"') => {
                    cell.push('"');
                    chars.next();
                }
                ('"', _) => quoted = !quoted,
                (',', false) => row.push(std::mem::take(&mut cell)),
                ('\r', false) if chars.peek() == Some(&'\n') => {
                    chars.next();
                    row.push(std::mem::take(&mut cell));
                    rows.push(std::mem::take(&mut row));
                }
                (c, _) => cell.push(c),
            }
        }
        assert!(cell.is_empty() && row.is_empty(), "the file ends with a line break");
        rows
    }

    #[test]
    fn cells_that_start_like_a_formula_are_written_as_text() {
        for dangerous in ["=1+1", "+1", "-1", "@SUM(A1)", "\t=1", "\r=1", "=HYPERLINK(\"x\",\"y\")", "-5% promo"] {
            assert_eq!(neutralise(dangerous), format!("'{dangerous}"), "{dangerous:?}");
        }
        for fine in ["", "Rent", "a=b", "1+1", "x-y", "'=already text", " leading space", "12.50", "email@example"] {
            assert_eq!(neutralise(fine), fine, "{fine:?}");
        }
    }

    #[test]
    fn the_csv_has_one_row_per_transaction_oldest_first() {
        let (conn, ..) = lived_in();
        let text = csv(&conn, &Selection::default()).unwrap();
        assert!(text.starts_with("\u{feff}date,type,category,name,amount,currency,description\r\n"));
        let rows = read_csv(&text);
        assert_eq!(rows.len(), 5, "header and four transactions");
        assert_eq!(rows[1], ["2026-01-25", "INCOME", "Salary", "Pay", "3200.00", "EUR", "Monthly pay"]);
        assert_eq!(rows[2], ["2026-02-01", "EXPENSE", "Rent", "Rent", "900.00", "EUR", ""]);
        assert_eq!(rows[3][3..5], ["Session", "80.00"]);
        assert_eq!(rows[4][4], "905.50");
    }

    #[test]
    fn a_csv_cannot_carry_a_formula_into_a_spreadsheet() {
        let (conn, _, rent, _) = lived_in();
        spend(&conn, rent, "=cmd|' /C calc'!A0", "1.00", "2026-04-01", Some("+1+1"));
        spend(&conn, rent, "@SUM(1)", "2.00", "2026-04-02", Some("-5% off, \"really\""));
        let text = csv(&conn, &Selection::default()).unwrap();
        let rows = read_csv(&text);
        for row in &rows[1..] {
            for cell in row {
                assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{cell:?} would run as a formula");
            }
        }
        let evil = rows.iter().find(|r| r[3].contains("cmd")).unwrap();
        assert_eq!((evil[3].as_str(), evil[6].as_str()), ("'=cmd|' /C calc'!A0", "'+1+1"));
        let quoted = rows.iter().find(|r| r[3] == "'@SUM(1)").unwrap();
        assert_eq!(quoted[6], "'-5% off, \"really\"", "commas and quotes survive the quoting");
        // The amounts are plain numbers: positive, so never mistaken for a formula and never prefixed.
        assert!(rows[1..].iter().all(|r| r[4].chars().next().unwrap().is_ascii_digit()));
    }

    #[test]
    fn awkward_text_survives_the_quoting() {
        let (conn, _, rent, _) = lived_in();
        spend(&conn, rent, "Café, \"the\" best", "3.00", "2026-04-01", Some("日本語 and 'quotes', commas"));
        let rows = read_csv(&csv(&conn, &Selection::default()).unwrap());
        let row = rows.last().unwrap();
        assert_eq!((row[3].as_str(), row[6].as_str()), ("Café, \"the\" best", "日本語 and 'quotes', commas"));
    }

    #[test]
    fn a_selection_narrows_the_transactions() {
        let (conn, salary, rent, _) = lived_in();
        let names = |sel: &Selection| -> Vec<String> {
            read_csv(&csv(&conn, sel).unwrap())[1..].iter().map(|r| format!("{} {}", r[0], r[3])).collect()
        };
        let range = Selection { from: Some("2026-02-01".into()), to: Some("2026-02-10".into()), ..Default::default() };
        assert_eq!(names(&range), ["2026-02-01 Rent", "2026-02-10 Session"], "both ends are inclusive");

        let categories = Selection { category_ids: Some(vec![rent.to_string(), salary.to_string()]), ..Default::default() };
        assert_eq!(names(&categories).len(), 3);

        let both = Selection { from: Some("2026-03-01".into()), category_ids: Some(vec![rent.to_string()]), ..Default::default() };
        assert_eq!(names(&both), ["2026-03-01 Rent"]);

        let none = Selection { category_ids: Some(vec![]), ..Default::default() };
        assert!(names(&none).is_empty());
        let empty_range = Selection { from: Some("2030-01-01".into()), ..Default::default() };
        assert!(names(&empty_range).is_empty());
    }

    #[test]
    fn a_bad_selection_is_refused() {
        let (conn, ..) = lived_in();
        for bad in [
            Selection { from: Some("2026-1-1".into()), ..Default::default() },
            Selection { to: Some("tomorrow".into()), ..Default::default() },
            Selection { from: Some("2026-03-01".into()), to: Some("2026-02-01".into()), ..Default::default() },
            Selection { category_ids: Some(vec!["1; DROP TABLE transactions".into()]), ..Default::default() },
            Selection { category_ids: Some(vec!["-1".into()]), ..Default::default() },
            Selection { category_ids: Some(vec!["x".into()]), ..Default::default() },
        ] {
            assert_eq!(csv(&conn, &bad), Err(AppError::InvalidInput));
            assert_eq!(json(&conn, &bad, Utc::now()), Err(AppError::InvalidInput));
        }
        let count: i64 = conn.query_row("SELECT count(*) FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 4);
    }

    fn parsed(conn: &Connection, selection: &Selection) -> Value {
        serde_json::from_str(&json(conn, selection, Utc::now()).unwrap()).unwrap()
    }

    #[test]
    fn the_full_json_holds_all_of_the_data() {
        let (conn, ..) = lived_in();
        let file = parsed(&conn, &Selection::default());
        assert_eq!((file["format"].as_str(), file["version"].as_i64()), (Some("meowny-export"), Some(1)));
        assert_eq!(file["settings"]["currency"], "EUR");
        assert_eq!(file["categories"].as_array().unwrap().len(), 3);
        let stored: i64 = conn.query_row("SELECT count(*) FROM budgets", [], |r| r.get(0)).unwrap();
        assert_eq!(file["budgets"].as_array().unwrap().len() as i64, stored, "every budget row, carry-forward rows included");
        assert_eq!(file["recurring"].as_array().unwrap().len(), 2);
        assert_eq!(file["transactions"].as_array().unwrap().len(), 4);
        assert!(!file["history"].as_array().unwrap().is_empty(), "the change history is part of all the data");

        let pay = &file["transactions"][0];
        assert_eq!((pay["amount"].as_str(), pay["date"].as_str(), pay["type"].as_str()), (Some("3200.00"), Some("2026-01-25"), Some("INCOME")));
        assert!(pay["id"].is_string() && pay["categoryId"].is_string(), "ids are strings, as in the app");
        let removed = file["budgets"].as_array().unwrap().iter().find(|b| b["month"] == "2026-03").unwrap();
        assert!(removed["limit"].is_null(), "a removed budget stays visible");
        assert!(file["budgets"].as_array().unwrap().iter().any(|b| b["scope"] == "GLOBAL" && b["limit"] == "2200.00"));
    }

    #[test]
    fn money_in_the_json_is_always_a_decimal_string() {
        let (conn, ..) = lived_in();
        let text = json(&conn, &Selection::default(), Utc::now()).unwrap();
        let file: Value = serde_json::from_str(&text).unwrap();
        for section in ["budgets", "recurring", "transactions"] {
            for item in file[section].as_array().unwrap() {
                for key in ["amount", "limit", "plannedAmount"] {
                    if let Some(value) = item.get(key).filter(|v| !v.is_null()) {
                        assert!(value.as_str().is_some_and(|s| s.contains('.')), "{section}.{key} = {value}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_limited_json_reveals_nothing_about_the_rest() {
        let (conn, _, rent, _) = lived_in();
        let text = json(&conn, &Selection { category_ids: Some(vec![rent.to_string()]), ..Default::default() }, Utc::now()).unwrap();
        for hidden in ["Therapy", "Dr. Private", "Session", "Monthly pay", "Salary", "300.00", "2200.00"] {
            assert!(!text.contains(hidden), "{hidden:?} leaked into a Rent-only export");
        }
        let file: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(file["categories"].as_array().unwrap().len(), 1);
        let budgets = file["budgets"].as_array().unwrap();
        assert!(!budgets.is_empty());
        assert!(budgets.iter().all(|b| b["categoryId"] == rent.to_string()), "no global budget, no other category's budget");
        assert_eq!(file["recurring"].as_array().unwrap().len(), 1);
        assert_eq!(file["transactions"].as_array().unwrap().len(), 2);
        assert!(file.get("history").is_none(), "history mentions every category");
        assert_eq!(file["scope"]["categoryIds"][0], rent.to_string());
    }

    #[test]
    fn a_date_range_alone_is_not_the_whole_data() {
        let (conn, ..) = lived_in();
        let file = parsed(&conn, &Selection { from: Some("2026-03-01".into()), ..Default::default() });
        assert_eq!(file["transactions"].as_array().unwrap().len(), 1);
        assert!(file.get("history").is_none());
        assert_eq!(file["categories"].as_array().unwrap().len(), 3, "a range does not hide categories");
    }

    #[test]
    fn deleted_categories_are_marked_and_their_transactions_keep_the_name() {
        let (mut conn, _, _, secret) = lived_in();
        categories::delete(&mut conn, secret, Utc::now()).unwrap();
        let file = parsed(&conn, &Selection::default());
        let gone = file["categories"].as_array().unwrap().iter().find(|c| c["name"] == "Therapy").unwrap();
        assert_eq!(gone["deleted"], true);
        let rows = read_csv(&csv(&conn, &Selection::default()).unwrap());
        assert!(rows.iter().any(|r| r[2] == "Therapy"));
    }

    #[test]
    fn the_scheduled_backup_passphrase_is_never_exported() {
        let (conn, ..) = lived_in();
        conn.execute(
            "INSERT INTO backup_schedule (id, folder, every_days, passphrase) VALUES (1, '/somewhere/private', 7, 'unmistakable passphrase')",
            [],
        )
        .unwrap();
        let text = json(&conn, &Selection::default(), Utc::now()).unwrap();
        assert!(!text.contains("unmistakable") && !text.contains("/somewhere/private"));
        assert!(!csv(&conn, &Selection::default()).unwrap().contains("unmistakable"));
    }

    #[test]
    fn exporting_changes_nothing() {
        let (conn, ..) = lived_in();
        let before: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
        csv(&conn, &Selection::default()).unwrap();
        json(&conn, &Selection::default(), Utc::now()).unwrap();
        let after: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
    }
}
