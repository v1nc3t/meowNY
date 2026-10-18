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

/// Rows as aligned columns. Numbers (and amounts) are right-aligned, text left-aligned.
fn table(rows: &[Value]) -> String {
    if rows.is_empty() {
        return "(none)\n".into();
    }
    if rows.iter().any(|r| !r.is_object()) {
        return rows.iter().map(|r| format!("{}\n", cell(r))).collect();
    }
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
}
