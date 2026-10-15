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

fn cell(value: &Value) -> String {
    cut(match value {
        Value::Null => "-".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => clean(s),
        other => clean(&compact(other)),
    })
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

/// Rows as aligned columns. Numbers (and amounts) are right-aligned, text left-aligned.
fn table(rows: &[Value]) -> String {
    if rows.is_empty() {
        return "(none)\n".into();
    }
    if rows.iter().any(|r| !r.is_object()) {
        return rows.iter().map(|r| format!("{}\n", cell(r))).collect();
    }
    let keys = columns(rows);
    let body: Vec<Vec<String>> = rows.iter().map(|row| keys.iter().map(|k| cell(row.get(k).unwrap_or(&Value::Null))).collect()).collect();
    let widths: Vec<usize> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| body.iter().map(|r| r[i].chars().count()).chain([k.chars().count()]).max().unwrap_or(0))
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
    let mut out = line(keys.iter().map(String::as_str).collect());
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
            let mut out: String = singles.iter().map(|(k, v)| format!("{k}: {}\n", cell(v))).collect();
            for (key, rows) in lists {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("{key}\n"));
                out.push_str(&table(rows.as_array().map(Vec::as_slice).unwrap_or(&[])));
            }
            out
        }
        other => format!("{}\n", cell(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_is_compact_on_one_line() {
        assert_eq!(compact(&json!({ "b": [1, 2], "a": null })), r#"{"a":null,"b":[1,2]}"#);
        assert_eq!(compact(&Value::Null), "null");
    }

    #[test]
    fn a_list_is_a_table_with_id_and_name_first_and_numbers_on_the_right() {
        let rows = json!([
            { "paymentDate": "2026-10-14", "amount": "12.50", "name": "Coffee", "id": "404", "description": null },
            { "paymentDate": "2026-10-13", "amount": "1034.00", "name": "Rent", "id": "7", "description": "October" }
        ]);
        let text = human(&rows);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], " id  name    paymentDate   amount  description");
        assert_eq!(lines[1], "404  Coffee  2026-10-14     12.50  -");
        assert_eq!(lines[2], "  7  Rent    2026-10-13   1034.00  October");
    }

    #[test]
    fn the_money_columns_come_in_the_order_a_person_reads_them() {
        let text = human(&json!([{ "remaining": "10.00", "actual": "90.00", "budgeted": "100.00", "name": "Food", "pctUsed": 90.0, "categoryId": "5", "type": "EXPENSE" }]));
        assert_eq!(text.lines().next().unwrap(), "name  type     budgeted  actual  remaining  pctUsed  categoryId");
    }

    #[test]
    fn a_field_of_lists_or_records_is_left_out_of_a_table_but_a_mixed_one_stays() {
        let totals = json!([
            { "type": "INCOME", "actual": "5.00", "categories": [{ "categoryId": "1", "amountLimit": "3.00" }] },
            { "type": "EXPENSE", "actual": "7.00", "categories": [] }
        ]);
        let text = human(&totals);
        assert_eq!(text.lines().collect::<Vec<_>>(), ["type     actual", "INCOME     5.00", "EXPENSE    7.00"]);
        // If some rows have a plain value in that field it is a column like any other.
        let mixed = human(&json!([{ "id": "1", "x": [1] }, { "id": "2", "x": "plain" }]));
        assert!(mixed.lines().next().unwrap().contains('x'));
    }

    #[test]
    fn an_empty_list_says_so() {
        assert_eq!(human(&json!([])), "(none)\n");
        assert_eq!(human(&json!({ "transactions": [] })), "transactions\n(none)\n");
    }

    #[test]
    fn missing_keys_are_dashes_and_the_columns_are_the_union() {
        let text = human(&json!([{ "id": "1", "a": "x" }, { "id": "2", "b": "y" }]));
        assert_eq!(text.lines().collect::<Vec<_>>(), ["id  a  b", " 1  x  -", " 2  -  y"]);
    }

    #[test]
    fn a_record_is_key_value_lines_and_its_lists_are_tables_below() {
        let page = json!({ "total": 404, "limit": 2, "offset": 0, "transactions": [{ "id": "1", "name": "A" }] });
        assert_eq!(human(&page), "limit: 2\noffset: 0\ntotal: 404\n\ntransactions\nid  name\n 1  A\n");
        assert_eq!(human(&json!({ "app": "meowny", "api": 1, "scope": "read" })), "api: 1\napp: meowny\nscope: read\n");
        let two = human(&json!({ "overdue": [{ "id": "1" }], "upcoming": [] }));
        assert_eq!(two, "overdue\nid\n 1\n\nupcoming\n(none)\n");
    }

    #[test]
    fn scalars_and_nothing_print_simply() {
        assert_eq!(human(&Value::Null), "");
        assert_eq!(human(&json!("hello")), "hello\n");
        assert_eq!(human(&json!(5)), "5\n");
        assert_eq!(human(&json!(["a", "b"])), "a\nb\n");
        assert_eq!(human(&json!({ "nested": { "a": 1 } })), "nested: {\"a\":1}\n");
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
}
