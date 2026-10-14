//! The Excel export: one workbook with a sheet each for transactions, categories, budgets,
//! recurring items and a monthly summary.
//!
//! Dates are real dates and money is real numbers with a currency number format, so the sheets sort,
//! filter and sum in Excel. Amounts are stored as whole minor units and divided by 100 only when a cell
//! is written. Text cells go through `neutralise`, as in the CSV, and no cell holds a formula.
//!
//! The person's number and date format (`DisplayFormat`) decide how the cells look: the order and
//! separator of dates, and where the currency symbol goes. Excel and LibreOffice choose the digit
//! grouping and the decimal mark themselves from the computer they run on, so those are not part of
//! the file.

use std::collections::HashMap;

use rusqlite::Connection;
use rust_xlsxwriter::{ExcelDateTime, Format, Workbook, Worksheet, XlsxError};

use super::{
    budget_rows, category_rows, currency, monthly_sums, neutralise, parse, recurring_rows, transactions, BudgetRow,
    CategoryRow, Parsed, Selection, Tx,
};
use crate::display::{DateOrder, DateSeparator, DisplayFormat};
use crate::recurring::Recurring;
use crate::vault::AppError;

/// Excel's limit is 1,048,576 rows per sheet, one of them the header.
const MAX_ROWS: usize = 1_000_000;
/// Gaps in the monthly summary are filled in only for spans up to this many months.
const MAX_FILLED_MONTHS: usize = 1200;

struct Codes {
    date: String,
    month: String,
    money: String,
}

impl DisplayFormat {
    /// Excel number format codes. Separators and the symbol are written as quoted or escaped
    /// literals, so Excel shows exactly these characters and not its own idea of a date separator.
    fn codes(&self) -> Result<Codes, AppError> {
        self.validate()?;
        let symbol = &self.currency_symbol;
        let sep = match self.date_separator {
            DateSeparator::Slash => "\\/",
            DateSeparator::Dot => "\\.",
            DateSeparator::Dash => "\\-",
            DateSeparator::Space => "\\ ",
        };
        let (date, month) = match self.date_order {
            DateOrder::DayMonthYear => (format!("dd{sep}mm{sep}yyyy"), format!("mm{sep}yyyy")),
            DateOrder::MonthDayYear => (format!("mm{sep}dd{sep}yyyy"), format!("mm{sep}yyyy")),
            DateOrder::YearMonthDay => (format!("yyyy{sep}mm{sep}dd"), format!("yyyy{sep}mm")),
        };
        let gap = if self.symbol_space { " " } else { "" };
        let money = match (symbol.is_empty(), self.symbol_before) {
            (true, _) => "#,##0.00".to_string(),
            (false, true) => format!("\"{symbol}{gap}\"#,##0.00"),
            (false, false) => format!("#,##0.00\"{gap}{symbol}\""),
        };
        Ok(Codes { date, month, money })
    }
}

struct Data {
    currency: String,
    transactions: Vec<Tx>,
    categories: Vec<CategoryRow>,
    budgets: Vec<BudgetRow>,
    recurring: Vec<Recurring>,
    /// Name and type of every category, for rows that point at one.
    names: HashMap<i64, (String, String)>,
    /// (month "YYYY-MM", income, expense) in minor units, every month from the first to the last.
    summary: Vec<(String, i64, i64)>,
}

fn next_month(month: &str) -> Option<String> {
    let (year, number) = month.split_once('-')?;
    let (year, number): (u32, u32) = (year.parse().ok()?, number.parse().ok()?);
    Some(if number == 12 { format!("{:04}-01", year + 1) } else { format!("{year:04}-{:02}", number + 1) })
}

fn summary(conn: &Connection, sel: &Parsed) -> Result<Vec<(String, i64, i64)>, AppError> {
    let sums = monthly_sums(conn, sel)?;

    let (Some(first), Some(last)) = (sums.keys().next().cloned(), sums.keys().next_back().cloned()) else {
        return Ok(Vec::new());
    };
    // A quiet month is a row of zeros, so the sheet reads as a calendar. Absurd spans stay sparse.
    let mut months = Vec::new();
    let mut month = first;
    while month <= last && months.len() <= MAX_FILLED_MONTHS {
        months.push(month.clone());
        month = next_month(&month).ok_or(AppError::Internal)?;
    }
    if month <= last {
        return Ok(sums.into_iter().map(|(m, (i, e))| (m, i, e)).collect());
    }
    Ok(months
        .into_iter()
        .map(|m| {
            let (income, expense) = sums.get(&m).copied().unwrap_or_default();
            (m, income, expense)
        })
        .collect())
}

fn load(conn: &Connection, sel: &Parsed) -> Result<Data, AppError> {
    let everything = Parsed { from: None, to: None, categories: None };
    let data = Data {
        currency: currency(conn)?,
        transactions: transactions(conn, sel)?,
        categories: category_rows(conn, sel)?,
        budgets: budget_rows(conn, sel)?,
        recurring: recurring_rows(conn, sel)?,
        names: category_rows(conn, &everything)?.into_iter().map(|c| (c.id, (c.name, c.kind))).collect(),
        summary: summary(conn, sel)?,
    };
    if data.transactions.len() > MAX_ROWS {
        return Err(AppError::InvalidInput);
    }
    Ok(data)
}

/// The workbook as the bytes of an .xlsx file.
pub fn workbook(conn: &Connection, selection: &Selection, format: &DisplayFormat) -> Result<Vec<u8>, AppError> {
    let sel = parse(selection)?;
    let codes = format.codes()?;
    // One read transaction, so the sheets agree with each other.
    let tx = conn.unchecked_transaction()?;
    let data = load(&tx, &sel)?;
    write(&data, &codes).map_err(|_| AppError::Internal)
}

/// "2026-01-25" as a year, month and day Excel can hold. Dates before 1900 have no Excel date.
fn ymd(text: &str) -> Option<(u16, u8, u8)> {
    let mut parts = text.splitn(3, '-');
    let year: u16 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next().map_or(Some(1), |d| d.parse().ok())?;
    (year >= 1900).then_some((year, month, day))
}

fn text(sheet: &mut Worksheet, row: u32, col: u16, value: &str) -> Result<(), XlsxError> {
    sheet.write_string(row, col, &*neutralise(value))?;
    Ok(())
}

fn date(sheet: &mut Worksheet, row: u32, col: u16, value: &str, format: &Format) -> Result<(), XlsxError> {
    match ymd(value) {
        Some((y, m, d)) => sheet.write_datetime_with_format(row, col, ExcelDateTime::from_ymd(y, m, d)?, format)?,
        None => sheet.write_string(row, col, &*neutralise(value))?,
    };
    Ok(())
}

fn money(sheet: &mut Worksheet, row: u32, col: u16, minor: i64, format: &Format) -> Result<(), XlsxError> {
    // Exact to the cent for every amount the database allows (at most 12 digits).
    sheet.write_number_with_format(row, col, minor as f64 / 100.0, format)?;
    Ok(())
}

fn sheet<'a>(book: &'a mut Workbook, name: &str, columns: &[(&str, f64)]) -> Result<&'a mut Worksheet, XlsxError> {
    let head = Format::new().set_bold();
    let sheet = book.add_worksheet();
    sheet.set_name(name)?;
    for (i, (title, width)) in columns.iter().enumerate() {
        sheet.write_string_with_format(0, i as u16, *title, &head)?;
        sheet.set_column_width(i as u16, *width)?;
    }
    sheet.set_freeze_panes(1, 0)?;
    Ok(sheet)
}

fn write(data: &Data, codes: &Codes) -> Result<Vec<u8>, XlsxError> {
    let day = Format::new().set_num_format(&codes.date);
    let month = Format::new().set_num_format(&codes.month);
    let cash = Format::new().set_num_format(&codes.money);
    let percent = Format::new().set_num_format("0.0%");
    let amount = format!("Amount ({})", data.currency);
    let mut book = Workbook::new();

    let s = sheet(
        &mut book,
        "Transactions",
        &[("Date", 12.0), ("Type", 10.0), ("Category", 22.0), ("Name", 30.0), (&amount, 14.0), ("Description", 44.0)],
    )?;
    for (i, t) in data.transactions.iter().enumerate() {
        let row = i as u32 + 1;
        date(s, row, 0, &t.date, &day)?;
        text(s, row, 1, &t.kind)?;
        text(s, row, 2, &t.category)?;
        text(s, row, 3, &t.name)?;
        money(s, row, 4, t.amount, &cash)?;
        text(s, row, 5, t.description.as_deref().unwrap_or(""))?;
    }

    let s = sheet(&mut book, "Categories", &[("Name", 26.0), ("Type", 10.0), ("Deleted", 10.0)])?;
    for (i, c) in data.categories.iter().enumerate() {
        let row = i as u32 + 1;
        text(s, row, 0, &c.name)?;
        text(s, row, 1, &c.kind)?;
        s.write_boolean(row, 2, c.deleted)?;
    }

    let limit = format!("Limit ({})", data.currency);
    let s = sheet(
        &mut book,
        "Budgets",
        &[("From month", 12.0), ("Type", 10.0), ("Scope", 10.0), ("Category", 26.0), (&limit, 14.0)],
    )?;
    for (i, b) in data.budgets.iter().enumerate() {
        let row = i as u32 + 1;
        date(s, row, 0, &b.effective_from, &month)?;
        text(s, row, 1, &b.kind)?;
        text(s, row, 2, &b.scope)?;
        if let Some((name, _)) = b.category_id.and_then(|id| data.names.get(&id)) {
            text(s, row, 3, name)?;
        }
        // A budget removed from this month on has no limit: the cell stays empty.
        if let Some(minor) = b.limit {
            money(s, row, 4, minor, &cash)?;
        }
    }

    let s = sheet(
        &mut book,
        "Recurring",
        &[
            ("Name", 26.0), ("Category", 22.0), ("Type", 10.0), (&amount, 14.0), ("Every", 8.0), ("Unit", 8.0),
            ("Start", 12.0), ("Next due", 12.0), ("End", 12.0), ("Active", 8.0), ("Description", 40.0),
        ],
    )?;
    for (i, r) in data.recurring.iter().enumerate() {
        let row = i as u32 + 1;
        let (category, kind) = data.names.get(&r.category_id).cloned().unwrap_or_default();
        text(s, row, 0, &r.name)?;
        text(s, row, 1, &category)?;
        text(s, row, 2, &kind)?;
        // A bill with no fixed amount leaves the cell empty.
        if let Some(amount) = r.amount.as_deref().and_then(|a| crate::money::parse_amount(a).ok()) {
            money(s, row, 3, amount, &cash)?;
        }
        s.write_number(row, 4, r.interval_count)?;
        text(s, row, 5, &format!("{:?}", r.interval_unit).to_uppercase())?;
        date(s, row, 6, &r.start_date, &day)?;
        date(s, row, 7, &r.next_due_date, &day)?;
        if let Some(end) = &r.end_date {
            date(s, row, 8, end, &day)?;
        }
        s.write_boolean(row, 9, r.is_active)?;
        text(s, row, 10, r.description.as_deref().unwrap_or(""))?;
    }

    let (income, expense, savings) = (format!("Income ({})", data.currency), format!("Expense ({})", data.currency), format!("Savings ({})", data.currency));
    let s = sheet(
        &mut book,
        "Monthly summary",
        &[("Month", 12.0), (&income, 16.0), (&expense, 16.0), (&savings, 16.0), ("Savings rate", 13.0)],
    )?;
    for (i, (m, earned, spent)) in data.summary.iter().enumerate() {
        let row = i as u32 + 1;
        date(s, row, 0, m, &month)?;
        money(s, row, 1, *earned, &cash)?;
        money(s, row, 2, *spent, &cash)?;
        money(s, row, 3, earned - spent, &cash)?;
        // Savings as a share of income; no income that month, no rate.
        if *earned > 0 {
            s.write_number_with_format(row, 4, (earned - spent) as f64 / *earned as f64, &percent)?;
        }
    }

    book.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Read;

    use chrono::{NaiveDate, Utc};

    use super::super::tests::{db, lived_in, set_budget, spend};
    use super::*;
    use crate::budgets::Scope;
    use crate::categories::{self, CategoryType};


    /// What a cell holds, as a spreadsheet program would show it.
    #[derive(Debug, Clone, PartialEq)]
    enum Cell {
        Empty,
        Text(String),
        Number(f64),
        /// A number whose format is a date format: days since 1899-12-30.
        Date(f64),
        Bool(bool),
    }

    /// An .xlsx file taken apart, for the tests: a zip of XML parts. Only what this app writes is understood.
    struct Book {
        parts: HashMap<String, Vec<u8>>,
    }

    fn u16_at(b: &[u8], at: usize) -> usize {
        u16::from_le_bytes([b[at], b[at + 1]]) as usize
    }

    fn u32_at(b: &[u8], at: usize) -> usize {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize
    }

    fn unzip(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
        let end = (0..=bytes.len() - 22).rev().find(|&i| bytes[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).expect("a zip file");
        let (count, mut at) = (u16_at(bytes, end + 10), u32_at(bytes, end + 16));
        let mut parts = HashMap::new();
        for _ in 0..count {
            assert_eq!(&bytes[at..at + 4], [0x50, 0x4b, 0x01, 0x02]);
            let (method, packed) = (u16_at(bytes, at + 10), u32_at(bytes, at + 20));
            let (name_len, extra_len, comment_len) = (u16_at(bytes, at + 28), u16_at(bytes, at + 30), u16_at(bytes, at + 32));
            let local = u32_at(bytes, at + 42);
            let name = String::from_utf8(bytes[at + 46..at + 46 + name_len].to_vec()).unwrap();
            let start = local + 30 + u16_at(bytes, local + 26) + u16_at(bytes, local + 28);
            let data = &bytes[start..start + packed];
            let mut out = Vec::new();
            match method {
                0 => out.extend_from_slice(data),
                8 => {
                    flate2::read::DeflateDecoder::new(data).read_to_end(&mut out).unwrap();
                }
                other => panic!("zip method {other}"),
            }
            parts.insert(name, out);
            at += 46 + name_len + extra_len + comment_len;
        }
        parts
    }

    fn unescape(text: &str) -> String {
        text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
    }

    /// The value of `name="..."` in the text of a tag.
    fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
        let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        Some(&tag[start..start + tag[start..].find('"')?])
    }

    /// The text inside every `<t ...>` element of `xml`, joined.
    fn texts_in(xml: &str) -> String {
        let mut out = String::new();
        let mut rest = xml;
        while let Some(open) = rest.find("<t") {
            let after = &rest[open..];
            let Some(gt) = after.find('>') else { break };
            if after.as_bytes().get(2).is_some_and(|c| *c != b'>' && *c != b' ') {
                rest = &after[2..];
                continue;
            }
            let body = &after[gt + 1..];
            let close = body.find("</t>").unwrap();
            out.push_str(&unescape(&body[..close]));
            rest = &body[close..];
        }
        out
    }

    impl Book {
        fn open(bytes: &[u8]) -> Book {
            Book { parts: unzip(bytes) }
        }

        fn part(&self, name: &str) -> String {
            String::from_utf8(self.parts[name].clone()).unwrap()
        }

        fn sheet_names(&self) -> Vec<String> {
            let workbook = self.part("xl/workbook.xml");
            workbook.split("<sheet ").skip(1).map(|tag| unescape(attribute(&format!(" {tag}"), "name").unwrap())).collect()
        }

        /// The number format code of each cell style, by style index, and whether it is a date format.
        fn date_styles(&self) -> Vec<bool> {
            let styles = self.part("xl/styles.xml");
            let formats: HashMap<usize, String> = styles
                .split("<numFmt ")
                .skip(1)
                .map(|tag| {
                    let tag = format!(" {tag}");
                    (attribute(&tag, "numFmtId").unwrap().parse().unwrap(), unescape(attribute(&tag, "formatCode").unwrap()))
                })
                .collect();
            let xfs = styles.split("<cellXfs").nth(1).unwrap().split("</cellXfs>").next().unwrap();
            xfs.split("<xf ")
                .skip(1)
                .map(|tag| {
                    let id: usize = attribute(&format!(" {tag}"), "numFmtId").unwrap().parse().unwrap();
                    let code = formats.get(&id).cloned().unwrap_or_default();
                    // Quoted text and escaped characters are not format letters.
                    let mut bare = String::new();
                    let (mut quoted, mut escaped) = (false, false);
                    for c in code.chars() {
                        match (quoted, escaped, c) {
                            (_, true, _) => escaped = false,
                            (false, false, '\\') => escaped = true,
                            (_, _, '"') => quoted = !quoted,
                            (false, false, c) => bare.push(c),
                            _ => {}
                        }
                    }
                    bare.contains('y') || (id >= 14 && id <= 22 && !formats.contains_key(&id))
                })
                .collect()
        }

        fn shared_strings(&self) -> Vec<String> {
            match self.parts.get("xl/sharedStrings.xml") {
                Some(_) => self.part("xl/sharedStrings.xml").split("<si>").skip(1).map(|si| texts_in(si.split("</si>").next().unwrap())).collect(),
                None => Vec::new(),
            }
        }

        /// The rows of a sheet, every row as wide as the widest.
        fn rows(&self, name: &str) -> Vec<Vec<Cell>> {
            let index = self.sheet_names().iter().position(|n| n == name).unwrap_or_else(|| panic!("no sheet {name}"));
            let xml = self.part(&format!("xl/worksheets/sheet{}.xml", index + 1));
            let (shared, dates) = (self.shared_strings(), self.date_styles());
            let data = xml.split("<sheetData>").nth(1).unwrap_or("").split("</sheetData>").next().unwrap();
            let mut rows: Vec<Vec<(usize, Cell)>> = Vec::new();
            for row in data.split("<row ").skip(1) {
                let mut cells = Vec::new();
                for cell in row.split("<c ").skip(1) {
                    let tag_end = cell.find('>').unwrap();
                    let tag = format!(" {}", &cell[..tag_end]);
                    let reference = attribute(&tag, "r").unwrap();
                    let column = reference.trim_end_matches(|c: char| c.is_ascii_digit()).bytes().fold(0, |n, b| n * 26 + (b - b'A') as usize + 1) - 1;
                    let body = if cell[..tag_end].ends_with('/') { "" } else { cell[tag_end + 1..].split("</c>").next().unwrap() };
                    let value = body.split("<v>").nth(1).and_then(|v| v.split("</v>").next());
                    let style: usize = attribute(&tag, "s").map_or(0, |s| s.parse().unwrap());
                    let kind = attribute(&tag, "t");
                    let cell = match (kind, value) {
                        (Some("inlineStr"), _) => Cell::Text(texts_in(body)),
                        (_, None) => Cell::Empty,
                        (Some("s"), Some(v)) => Cell::Text(shared[v.parse::<usize>().unwrap()].clone()),
                        (Some("b"), Some(v)) => Cell::Bool(v == "1"),
                        (Some("str"), Some(v)) => Cell::Text(unescape(v)),
                        (None | Some("n"), Some(v)) if dates.get(style) == Some(&true) => Cell::Date(v.parse().unwrap()),
                        (None | Some("n"), Some(v)) => Cell::Number(v.parse().unwrap()),
                        (other, _) => panic!("cell type {other:?}"),
                    };
                    cells.push((column, cell));
                }
                rows.push(cells);
            }
            let width = rows.iter().flatten().map(|(c, _)| c + 1).max().unwrap_or(0);
            rows.into_iter()
                .map(|cells| {
                    let mut row = vec![Cell::Empty; width];
                    for (column, cell) in cells {
                        row[column] = cell;
                    }
                    row
                })
                .collect()
        }
    }

    type Sheets = Book;

    fn open(bytes: &[u8]) -> Sheets {
        Book::open(bytes)
    }

    fn rows(book: &mut Sheets, name: &str) -> Vec<Vec<Cell>> {
        book.rows(name)
    }

    fn build(conn: &Connection, selection: &Selection, format: &DisplayFormat) -> Vec<u8> {
        workbook(conn, selection, format).unwrap()
    }

    fn serial(year: i32, month: u32, day: u32) -> f64 {
        let epoch = NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
        (NaiveDate::from_ymd_opt(year, month, day).unwrap() - epoch).num_days() as f64
    }

    fn styles(bytes: &[u8]) -> String {
        Book::open(bytes).part("xl/styles.xml")
    }

    fn strings(cells: &[Vec<Cell>]) -> Vec<String> {
        cells.iter().flatten().filter_map(|c| if let Cell::Text(s) = c { Some(s.clone()) } else { None }).collect()
    }

    #[test]
    fn the_workbook_has_the_five_sheets() {
        let (conn, ..) = lived_in();
        let bytes = build(&conn, &Selection::default(), &DisplayFormat::default());
        assert!(bytes.starts_with(b"PK"), "an .xlsx file is a zip");
        assert_eq!(open(&bytes).sheet_names(), ["Transactions", "Categories", "Budgets", "Recurring", "Monthly summary"]);
    }

    #[test]
    fn transactions_have_real_dates_and_numeric_money() {
        let (conn, ..) = lived_in();
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        let sheet = rows(&mut book, "Transactions");
        assert_eq!(sheet.len(), 5, "heading and four transactions");
        assert_eq!(sheet[0][4], Cell::Text("Amount (EUR)".into()));

        assert_eq!(sheet[1][0], Cell::Date(serial(2026, 1, 25)), "a real date, not text");
        assert_eq!(sheet[1][1], Cell::Text("INCOME".into()));
        assert_eq!(sheet[1][3], Cell::Text("Pay".into()));
        assert_eq!(sheet[1][5], Cell::Text("Monthly pay".into()));
        for (row, minor) in [(1, 320_000), (2, 90_000), (3, 8_000), (4, 90_550)] {
            let Cell::Number(value) = sheet[row][4] else { panic!("money is a number: {:?}", sheet[row][4]) };
            assert_eq!((value * 100.0).round() as i64, minor, "row {row}");
        }
        assert_eq!(sheet[1][5], Cell::Text("Monthly pay".into()));
    }

    #[test]
    fn dates_and_money_follow_the_number_and_date_format() {
        let (conn, ..) = lived_in();
        let all = Selection::default();
        let german = DisplayFormat {
            date_order: DateOrder::DayMonthYear,
            date_separator: DateSeparator::Dot,
            currency_symbol: "€".into(),
            symbol_before: false,
            symbol_space: true,
            ..Default::default()
        };
        let styles_de = styles(&build(&conn, &all, &german));
        assert!(styles_de.contains(r#"formatCode="dd\.mm\.yyyy""#), "{styles_de}");
        assert!(styles_de.contains(r#"formatCode="mm\.yyyy""#));
        assert!(styles_de.contains("formatCode=\"#,##0.00&quot; €&quot;\""));

        let american = DisplayFormat {
            date_order: DateOrder::MonthDayYear,
            date_separator: DateSeparator::Slash,
            currency_symbol: "$".into(),
            symbol_before: true,
            symbol_space: false,
            ..Default::default()
        };
        let styles_us = styles(&build(&conn, &all, &american));
        assert!(styles_us.contains(r#"formatCode="mm\/dd\/yyyy""#));
        assert!(styles_us.contains("formatCode=\"&quot;$&quot;#,##0.00\""));

        let japanese = DisplayFormat { date_order: DateOrder::YearMonthDay, date_separator: DateSeparator::Slash, ..Default::default() };
        let styles_jp = styles(&build(&conn, &all, &japanese));
        assert!(styles_jp.contains(r#"formatCode="yyyy\/mm\/dd""#) && styles_jp.contains(r#"formatCode="yyyy\/mm""#));
        assert!(styles_jp.contains(r##"formatCode="#,##0.00""##), "no symbol, a plain number");

        let default = styles(&build(&conn, &all, &DisplayFormat::default()));
        assert!(default.contains(r#"formatCode="yyyy\-mm\-dd""#));
    }

    #[test]
    fn the_format_does_not_change_the_values() {
        let (conn, ..) = lived_in();
        let german = DisplayFormat { date_order: DateOrder::DayMonthYear, date_separator: DateSeparator::Dot, ..Default::default() };
        let a = rows(&mut open(&build(&conn, &Selection::default(), &DisplayFormat::default())), "Transactions");
        let b = rows(&mut open(&build(&conn, &Selection::default(), &german)), "Transactions");
        assert_eq!(a, b);
    }

    #[test]
    fn a_format_the_file_cannot_hold_is_refused() {
        let (conn, ..) = lived_in();
        for symbol in ["\"", "a\"b", "\n", "123456789", "\u{7}"] {
            let format = DisplayFormat { currency_symbol: symbol.into(), ..Default::default() };
            assert_eq!(workbook(&conn, &Selection::default(), &format), Err(AppError::InvalidInput), "{symbol:?}");
        }
    }

    #[test]
    fn text_that_looks_like_a_formula_is_written_as_text_and_comes_back_as_text() {
        let (conn, _, rent, _) = lived_in();
        spend(&conn, rent, "=cmd|' /C calc'!A0", "1.00", "2026-04-01", Some("+1+1"));
        spend(&conn, rent, "@SUM(1)", "2.00", "2026-04-02", Some("-5% promo"));
        let bytes = build(&conn, &Selection::default(), &DisplayFormat::default());
        let mut book = open(&bytes);
        for name in ["Transactions", "Categories", "Budgets", "Recurring", "Monthly summary"] {
            for cell in strings(&rows(&mut book, name)) {
                assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "{name}: {cell:?}");
            }
        }
        let sheet = rows(&mut book, "Transactions");
        let evil = sheet.iter().find(|r| matches!(&r[3], Cell::Text(s) if s.contains("cmd"))).unwrap();
        assert_eq!((&evil[3], &evil[5]), (&Cell::Text("'=cmd|' /C calc'!A0".into()), &Cell::Text("'+1+1".into())));
        // No cell is a formula: the file has no <f> elements at all.
        let sheets: Vec<String> = Book::open(&bytes).parts.keys().filter(|n| n.starts_with("xl/worksheets/sheet")).cloned().collect();
        assert_eq!(sheets.len(), 5);
        for name in sheets {
            let part = Book::open(&bytes).part(&name);
            assert!(!part.contains("<f>") && !part.contains("<f "), "{name} has a formula");
        }
    }

    #[test]
    fn the_monthly_summary_matches_the_dashboard() {
        let (conn, ..) = lived_in();
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        let sheet = rows(&mut book, "Monthly summary");
        let dashboard = crate::stats::year_trend(&conn, "2026").unwrap();
        assert_eq!(sheet.len(), 1 + 3, "January to March, the months with transactions");
        for row in &sheet[1..] {
            let Cell::Date(first) = &row[0] else { panic!("{:?}", row[0]) };
            let month = (0..12).find(|m| serial(2026, m + 1, 1) == *first).unwrap() as usize;
            let point = &dashboard[month];
            let number = |cell: &Cell| if let Cell::Number(f) = cell { *f } else { panic!("{cell:?}") };
            let money = |text: &str| text.parse::<f64>().unwrap();
            assert_eq!(number(&row[1]), money(&point.income), "income, {}", point.month);
            assert_eq!(number(&row[2]), money(&point.expense), "expense, {}", point.month);
            assert_eq!(number(&row[3]), money(&point.savings), "savings, {}", point.month);
            match point.savings_rate {
                Some(rate) => assert!((number(&row[4]) * 100.0 - rate).abs() < 0.05, "rate, {}", point.month),
                None => assert_eq!(row[4], Cell::Empty, "no income, no rate"),
            }
        }
        // January: 3200.00 earned, nothing spent. February: 980.00 spent with no income.
        assert_eq!(sheet[1][1], Cell::Number(3200.0));
        assert_eq!(sheet[1][4], Cell::Number(1.0));
        assert_eq!(sheet[2][4], Cell::Empty);
    }

    #[test]
    fn quiet_months_are_filled_in_with_zeros() {
        let conn = db();
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        spend(&conn, food, "Bread", "2.50", "2025-11-30", None);
        spend(&conn, food, "Bread", "2.50", "2026-02-01", None);
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        let sheet = rows(&mut book, "Monthly summary");
        assert_eq!(sheet.len(), 1 + 4, "November, December, January, February");
        assert_eq!(sheet[2][2], Cell::Number(0.0));
    }

    #[test]
    fn a_selection_limits_every_sheet() {
        let (conn, _, rent, _) = lived_in();
        let selection = Selection { category_ids: Some(vec![rent.to_string()]), ..Default::default() };
        let mut book = open(&build(&conn, &selection, &DisplayFormat::default()));
        for name in ["Transactions", "Categories", "Budgets", "Recurring", "Monthly summary"] {
            let sheet = rows(&mut book, name);
            for cell in strings(&sheet) {
                for hidden in ["Therapy", "Salary", "Pay", "Session", "Dr. Private", "Monthly pay"] {
                    assert!(!cell.contains(hidden), "{name} leaked {hidden:?}");
                }
            }
        }
        assert_eq!(rows(&mut book, "Transactions").len(), 3, "heading and the two Rent payments");
        assert_eq!(rows(&mut book, "Categories").len(), 2);
        assert_eq!(rows(&mut book, "Recurring").len(), 2);
        let expense_only = rows(&mut book, "Monthly summary");
        assert!(expense_only[1..].iter().all(|r| r[1] == Cell::Number(0.0)), "no income in a Rent-only export");

        let range = Selection { from: Some("2026-03-01".into()), ..Default::default() };
        assert_eq!(rows(&mut open(&build(&conn, &range, &DisplayFormat::default())), "Transactions").len(), 2);
    }

    #[test]
    fn budgets_recurring_items_and_categories_are_listed() {
        let (mut conn, _, _, secret) = lived_in();
        categories::delete(&mut conn, secret, Utc::now()).unwrap();
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));

        let budgets = rows(&mut book, "Budgets");
        let stored: i64 = conn.query_row("SELECT count(*) FROM budgets", [], |r| r.get(0)).unwrap();
        assert_eq!(budgets.len() as i64, 1 + stored);
        let global = budgets.iter().find(|r| r[2] == Cell::Text("GLOBAL".into())).unwrap();
        assert_eq!((&global[3], &global[4]), (&Cell::Empty, &Cell::Number(2200.0)));
        let removed = budgets.iter().find(|r| r[0] == Cell::Date(serial(2026, 3, 1)) && r[3] == Cell::Text("Therapy".into()));
        assert_eq!(removed.map(|r| &r[4]), Some(&Cell::Empty), "a removed budget has no limit");

        let recurring = rows(&mut book, "Recurring");
        assert_eq!(recurring.len(), 3);
        let rent_item = recurring.iter().find(|r| r[0] == Cell::Text("Rent".into())).unwrap();
        assert_eq!(rent_item[1], Cell::Text("Rent".into()));
        assert_eq!((&rent_item[3], &rent_item[4], &rent_item[5]), (&Cell::Number(100.0), &Cell::Number(1.0), &Cell::Text("MONTH".into())));
        assert_eq!(rent_item[9], Cell::Bool(true));

        let sheet = rows(&mut book, "Categories");
        let gone = sheet.iter().find(|r| r[0] == Cell::Text("Therapy".into())).unwrap();
        assert_eq!(gone[2], Cell::Bool(true));
    }

    #[test]
    fn a_bill_with_no_fixed_amount_leaves_its_amount_cell_empty() {
        let (conn, _, rent, _) = lived_in();
        crate::recurring::create(
            &conn,
            &crate::recurring::NewRecurring {
                name: "Electricity".into(),
                amount: None,
                category_id: rent.to_string(),
                start_date: "2026-01-20".into(),
                interval_unit: crate::recurring::Unit::Month,
                interval_count: 1,
                end_date: None,
                description: None,
            },
        )
        .unwrap();
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        let sheet = rows(&mut book, "Recurring");
        let electricity = sheet.iter().find(|r| r[0] == Cell::Text("Electricity".into())).unwrap();
        assert_eq!(electricity[3], Cell::Empty, "no amount, no zero");
        let rent_item = sheet.iter().find(|r| r[0] == Cell::Text("Rent".into())).unwrap();
        assert_eq!(rent_item[3], Cell::Number(100.0));
    }

    #[test]
    fn an_empty_database_still_gives_a_valid_workbook() {
        let conn = db();
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        for name in ["Transactions", "Categories", "Budgets", "Recurring", "Monthly summary"] {
            assert_eq!(rows(&mut book, name).len(), 1, "{name}: only the heading");
        }
    }

    #[test]
    fn dates_before_1900_are_written_as_text_not_lost() {
        let conn = db();
        let food = categories::create(&conn, "Food", CategoryType::Expense).unwrap().id;
        spend(&conn, food, "Old", "1.00", "1850-05-01", None);
        set_budget(&conn, CategoryType::Expense, Scope::Global, None, "2026-01", Some("10.00"));
        let mut book = open(&build(&conn, &Selection::default(), &DisplayFormat::default()));
        assert_eq!(rows(&mut book, "Transactions")[1][0], Cell::Text("1850-05-01".into()));
    }

    #[test]
    fn building_the_workbook_changes_nothing() {
        let (conn, ..) = lived_in();
        let before: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
        build(&conn, &Selection::default(), &DisplayFormat::default());
        let after: i64 = conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
    }
}
