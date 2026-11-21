//! What the commands print. With `--json` that is the app's data, compact, on one line. Without
//! it, a plain text form for people: tables for lists, `key: value` lines for a record. Text that
//! came from the person's own data (a description can hold anything) has its control characters
//! turned into spaces first, so it cannot move the cursor or recolour a terminal.

use serde_json::Value;

const MAX_CELL: usize = 60;
/// Columns that come first, in this order, when a row has them; the others follow in alphabetical order.
const FIRST: &[&str] = &["id", "name", "type", "month", "paymentDate", "nextDueDate", "amount", "budgeted", "actual", "remaining", "pctUsed"];

pub fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

/// Control characters (escape sequences, bells, line breaks) are replaced by spaces.
pub fn clean(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

fn cut(text: String) -> String {
    if text.chars().count() <= MAX_CELL {
        return text;
    }
    let mut short: String = text.chars().take(MAX_CELL - 1).collect();
    short.push('…');
    short
}

/// A value as text on one line, whole.
fn plain(value: &Value) -> String {
    match value {
        Value::Null => "-".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => clean(s),
        other => clean(&compact(other)),
    }
}

/// A value in a table cell: cut, so one long text cannot push the other columns off the screen.
fn cell(value: &Value) -> String {
    cut(plain(value))
}

fn is_number(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    !whole.is_empty() && !fraction.is_empty() && whole.bytes().all(|b| b.is_ascii_digit()) && fraction.bytes().all(|b| b.is_ascii_digit())
}

/// The columns of a table. A field that holds lists or records in every row (the per-category limits in a
/// budget total, say) is left out: it does not fit in a cell, and `--json` has it.
fn columns(rows: &[Value]) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for row in rows {
        if let Value::Object(map) = row {
            for key in map.keys() {
                if !keys.contains(key) {
                    keys.push(key.clone());
                }
            }
        }
    }
    keys.retain(|k| rows.iter().filter_map(|r| r.get(k)).any(|v| !v.is_array() && !v.is_object()));
    keys.sort_by_key(|k| (FIRST.iter().position(|f| f == k).unwrap_or(FIRST.len()), k.clone()));
    keys
}

/// The records an account (`liability`) or a stash (`deposit`) can carry, and the fields of each that a table shows as columns of their own, like
/// `liability.owed` and `deposit.expectedReturn`: a cell cannot hold a record. `--json` has all of it.
const NESTED: &[(&str, &[&str])] = &[("liability", &["owed", "state", "payoff"]), ("deposit", &["state", "interestRate", "interestPeriod", "expectedReturn"])];

/// What is left to pay of a liability as one short text: `never`, or the payments to make and the month of the last, like `12 payments to 2027-11`.
fn payoff(value: &Value) -> Value {
    match (value.get("never").and_then(Value::as_bool), value.get("months").and_then(Value::as_u64), value.get("month").and_then(Value::as_str)) {
        (Some(true), _, _) => "never".into(),
        (_, Some(months), Some(month)) => format!("{months} payments to {month}").into(),
        _ => Value::Null,
    }
}

/// A row with each record of `NESTED` spread over columns. An account that is not a liability and a stash that is not a deposit have a null there,
/// so they add no column and show a dash under the ones the others add.
fn spread(row: &Value) -> Value {
    let Value::Object(map) = row else { return row.clone() };
    let mut out = serde_json::Map::new();
    for (key, value) in map {
        match (NESTED.iter().find(|(name, _)| name == key), value) {
            (Some((_, fields)), Value::Object(inner)) => {
                for field in *fields {
                    let shown = match (*field, inner.get(*field)) {
                        ("payoff", Some(v)) => payoff(v),
                        (_, v) => v.cloned().unwrap_or(Value::Null),
                    };
                    out.insert(format!("{key}.{field}"), shown);
                }
            }
            (Some(_), _) => {}
            (None, _) => {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(out)
}

/// Rows as aligned columns. Numbers (and amounts) are right-aligned, text left-aligned.
fn table(rows: &[Value]) -> String {
    if rows.is_empty() {
        return "(none)\n".into();
    }
    if rows.iter().any(|r| !r.is_object()) {
        return rows.iter().map(|r| format!("{}\n", cell(r))).collect();
    }
    let rows: Vec<Value> = rows.iter().map(spread).collect();
    let rows = rows.as_slice();
    let keys: Vec<String> = columns(rows);
    let body: Vec<Vec<String>> = rows.iter().map(|row| keys.iter().map(|k| cell(row.get(k).unwrap_or(&Value::Null))).collect()).collect();
    let widths: Vec<usize> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| body.iter().map(|r| r[i].chars().count()).chain([clean(k).chars().count()]).max().unwrap_or(0))
        .collect();
    let right: Vec<bool> = (0..keys.len()).map(|i| body.iter().all(|r| r[i] == "-" || is_number(&r[i])) && body.iter().any(|r| r[i] != "-")).collect();

    let line = |cells: Vec<&str>| {
        let padded: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let fill = " ".repeat(widths[i] - c.chars().count());
                if right[i] { format!("{fill}{c}") } else { format!("{c}{fill}") }
            })
            .collect();
        format!("{}\n", padded.join("  ").trim_end())
    };
    let heads: Vec<String> = keys.iter().map(|k| clean(k)).collect();
    let mut out = line(heads.iter().map(String::as_str).collect());
    for row in &body {
        out.push_str(&line(row.iter().map(String::as_str).collect()));
    }
    out
}

/// The plain text form of what a command returned.
pub fn human(data: &Value) -> String {
    match data {
        Value::Null => String::new(),
        Value::Array(rows) => table(rows),
        Value::Object(map) => {
            let (lists, singles): (Vec<_>, Vec<_>) = map.iter().partition(|(_, v)| v.is_array());
            // A line of `key: value` is shown whole: a path or an id is no use cut short.
            let mut out: String = singles.iter().map(|(k, v)| format!("{}: {}\n", clean(k), plain(v))).collect();
            for (key, rows) in lists {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("{}\n", clean(key)));
                out.push_str(&table(rows.as_array().map(Vec::as_slice).unwrap_or(&[])));
            }
            out
        }
        other => format!("{}\n", plain(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_keys_are_dashes_and_the_columns_are_the_union() {
        let text = human(&json!([{ "id": "1", "a": "x" }, { "id": "2", "b": "y" }]));
        assert_eq!(text.lines().collect::<Vec<_>>(), ["id  a  b", " 1  x  -", " 2  -  y"]);
    }

    #[test]
    fn long_text_is_cut_and_control_characters_become_spaces() {
        let long = "x".repeat(200);
        let text = human(&json!([{ "id": "1", "note": long }]));
        let cell = text.lines().nth(1).unwrap().split("  ").last().unwrap();
        assert_eq!(cell.chars().count(), MAX_CELL);
        assert!(cell.ends_with('…'));
        // An escape sequence in a description cannot reach the terminal as one.
        let nasty = human(&json!([{ "id": "1", "note": "\u{1b}[31mred\u{7}\nnext\r\u{0}x" }]));
        assert!(!nasty.contains('\u{1b}') && !nasty.contains('\u{7}') && !nasty.contains('\r') && !nasty.contains('\u{0}'));
        assert_eq!(nasty.lines().count(), 2, "a line break inside a cell does not add a row");
        assert_eq!(clean("a\tb\u{85}c"), "a b c");
        // Keys, and scalars outside a table, are cleaned too.
        assert!(!human(&json!({ "k": "\u{1b}[0m" })).contains('\u{1b}'));
    }

    #[test]
    fn a_line_of_key_and_value_is_never_cut_but_still_cleaned() {
        let path = format!("/home/someone/{}/export.json", "folder/".repeat(30));
        let text = human(&json!({ "path": path, "bytes": 5, "note": "a\u{1b}[31mb" }));
        assert!(text.contains(&format!("path: {path}\n")), "{text}");
        assert!(!text.contains('\u{1b}') && !text.contains('…'));
        assert_eq!(human(&json!(path)), format!("{path}\n"), "a lone value too");
    }

    #[test]
    fn wide_text_is_measured_in_characters_not_bytes() {
        let text = human(&json!([{ "id": "1", "name": "Café" }, { "id": "2", "name": "日本語" }]));
        let widths: Vec<usize> = text.lines().map(|l| l.chars().count()).collect();
        assert_eq!(widths[1], "1   Café".chars().count());
        assert_eq!(widths[2], "2   日本語".chars().count());
    }

    #[test]
    fn money_and_counts_line_up_but_words_that_look_like_numbers_do_not_break_it() {
        assert!(is_number("12.50") && is_number("-3") && is_number("0") && is_number("1034.00"));
        for no in ["", "-", ".5", "5.", "1e3", "1,5", "12.5.1", "abc", "1 2"] {
            assert!(!is_number(no), "{no:?}");
        }
        let text = human(&json!([{ "a": "1.00" }, { "a": "-20.50" }]));
        assert_eq!(text.lines().collect::<Vec<_>>(), ["     a", "  1.00", "-20.50"]);
    }

    #[test]
    fn what_a_liability_owes_and_what_a_deposit_returns_are_columns_and_a_row_without_them_shows_a_dash() {
        let loan = json!({ "id": "7", "name": "Car", "kind": "LOAN", "interestRate": "6.5", "liability": { "owed": "8635.00", "state": "OWING", "interestPerMonth": "46.78", "payoff": { "months": 12, "interestLeft": "123.45", "month": "2027-11" } } });
        let card = json!({ "id": "8", "name": "Card", "kind": "CHECKING", "interestRate": null, "liability": null });
        let text = human(&json!([loan, card]));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        for head in ["liability.owed", "liability.state", "liability.payoff"] {
            assert!(lines[0].contains(head), "{head}: {text}");
        }
        assert!(!lines[0].split_whitespace().any(|c| c == "liability") && !lines[0].contains("interestPerMonth"), "{text}");
        assert!(lines[1].contains("8635.00") && lines[1].contains("OWING") && lines[1].contains("12 payments to 2027-11"), "{text}");
        assert!(lines[2].split_whitespace().rev().take(3).all(|c| c == "-"), "the three columns of the liability, the last in order: {text}");
        // an installment that does not beat the interest is never paid off
        let never = human(&json!([{ "id": "1", "liability": { "owed": "100.00", "state": "OWING", "interestPerMonth": "9.00", "payoff": { "never": true } } }]));
        assert!(never.contains("never"), "{never}");
        // a list with no liability and no deposit has none of the columns
        let plain = human(&json!([{ "id": "1", "liability": null, "deposit": null, "name": "A" }]));
        assert_eq!(plain.lines().next().unwrap().split_whitespace().collect::<Vec<_>>(), ["id", "name"]);
        let deposit = human(&json!([{ "id": "3", "deposit": { "state": "MATURED", "interestRate": "3.1", "interestPeriod": "YEARLY", "startDate": "2026-01-05", "expectedReturn": "31.00", "daysLeft": 0 } }]));
        assert_eq!(deposit.lines().next().unwrap().split_whitespace().collect::<Vec<_>>(), ["id", "deposit.expectedReturn", "deposit.interestPeriod", "deposit.interestRate", "deposit.state"]);
        assert!(deposit.contains("MATURED") && deposit.contains("31.00") && !deposit.contains("2026-01-05"), "{deposit}");
    }
}
