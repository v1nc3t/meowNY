//! The PDF of who owes what: the summary of every friend, or the statement of one. Made on this computer, nothing is sent anywhere. The
//! words come from the screens in the person's language (`Labels`), the numbers are written with the person's `DisplayFormat`.

use super::layout::{Color, Doc, Metrics, Op, CARD, FOOTER_SPACE, GRID, INCOME, INK, MARGIN, MUTED, OVER_TEXT};
use super::{render, PageSize};
use crate::debts::ledger::Statement;
use crate::debts::summary::{Labels, Summary};
use crate::display::DisplayFormat;
use crate::vault::AppError;

const ROW: f32 = 22.0;

struct Sheet<'a> {
    m: &'a Metrics,
    labels: &'a Labels,
    doc: Doc,
    left: f32,
    right: f32,
    bottom: f32,
    y: f32,
}

impl Sheet<'_> {
    fn ops(&mut self) -> &mut Vec<Op> {
        self.doc.pages.last_mut().expect("a page is open")
    }

    fn new_page(&mut self) {
        self.doc.pages.push(Vec::new());
        self.y = MARGIN;
    }

    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Color, text: &str) {
        let text = self.m.clean(text);
        if !text.is_empty() {
            self.ops().push(Op::Text { x, y, size, bold, color, text });
        }
    }

    #[allow(clippy::too_many_arguments)] // a position, a size, a style, the text and a width: one call draws one piece of text
    fn fit(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Color, text: &str, max: f32) {
        let text = self.m.fit(text, size, bold, max);
        self.text(x, y, size, bold, color, &text);
    }

    fn right(&mut self, right: f32, y: f32, size: f32, bold: bool, color: Color, text: &str) {
        let text = self.m.clean(text);
        let x = right - self.m.width(&text, size, bold);
        self.text(x, y, size, bold, color, &text);
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.ops().push(Op::Rect { x, y, w, h, color });
    }

    fn line(&mut self, y: f32, width: f32, color: Color) {
        let (x1, x2) = (self.left, self.right);
        self.ops().push(Op::Line { x1, y1: y, x2, y2: y, width, color });
    }

    fn baseline(top: f32, height: f32, size: f32) -> f32 {
        top + (height + size * 0.72) / 2.0
    }

    fn header(&mut self, title: &str, subtitle: &str, currency: &str) {
        self.text(self.left, self.y + 18.0, 20.0, true, INK, title);
        self.right(self.right, self.y + 18.0, 9.0, false, MUTED, currency);
        self.text(self.left, self.y + 36.0, 11.0, false, MUTED, subtitle);
        self.line(self.y + 46.0, 0.8, GRID);
        self.y += 62.0;
    }

    fn tiles(&mut self, tiles: &[(String, String, Color)]) {
        let gap = 8.0;
        let w = (self.right - self.left - gap * (tiles.len() as f32 - 1.0)) / tiles.len() as f32;
        for (i, (label, value, color)) in tiles.iter().enumerate() {
            let x = self.left + i as f32 * (w + gap);
            self.rect(x, self.y, w, 48.0, CARD);
            self.fit(x + 9.0, self.y + 17.0, 8.5, false, MUTED, label, w - 14.0);
            self.fit(x + 9.0, self.y + 38.0, 13.0, true, *color, value, w - 14.0);
        }
        self.y += 48.0 + 20.0;
    }

    /// Starts a new page when `height` does not fit. True if it did.
    fn ensure(&mut self, height: f32) -> bool {
        if self.y + height > self.bottom {
            self.new_page();
            true
        } else {
            false
        }
    }

    fn footers(&mut self, title: &str) {
        let count = self.doc.pages.len();
        let page = self.labels.page.clone();
        for i in 0..count {
            let y = self.doc.height - MARGIN + 8.0;
            self.doc.pages[i].push(Op::Line { x1: self.left, y1: y, x2: self.right, y2: y, width: 0.5, color: GRID });
            let label = self.m.clean(&format!("{page} {} / {count}", i + 1));
            let x = self.right - self.m.width(&label, 8.0, false);
            let title = self.m.clean(title);
            self.doc.pages[i].push(Op::Text { x: self.left, y: y + 14.0, size: 8.0, bold: false, color: MUTED, text: title });
            self.doc.pages[i].push(Op::Text { x, y: y + 14.0, size: 8.0, bold: false, color: MUTED, text: label });
        }
    }
}

fn sheet<'a>(m: &'a Metrics, labels: &'a Labels, page: PageSize) -> Sheet<'a> {
    let (width, height) = page.points();
    let mut sheet = Sheet {
        m,
        labels,
        doc: Doc { width, height, pages: Vec::new() },
        left: MARGIN,
        right: width - MARGIN,
        bottom: height - MARGIN - FOOTER_SPACE + 14.0,
        y: MARGIN,
    };
    sheet.new_page();
    sheet
}

/// A friend's balance in the colour of its direction: green when they owe you, red when you owe them.
fn direction(minor: i64) -> Color {
    if minor < 0 {
        OVER_TEXT
    } else {
        INCOME
    }
}

/// The pages of the summary of who owes what.
pub fn summary_doc(summary: &Summary, currency: &str, today: &str, labels: &Labels, fmt: &DisplayFormat, page: PageSize) -> Result<Doc, AppError> {
    let metrics = Metrics::new()?;
    let mut s = sheet(&metrics, labels, page);
    s.header(&labels.summary_title, today, currency);
    if summary.people.is_empty() {
        s.text(s.left, s.y + 10.0, 10.0, false, MUTED, &labels.nobody);
        s.footers(&labels.summary_title);
        return Ok(s.doc);
    }
    let net: i64 = summary.people.iter().map(|p| p.balance_minor).sum();
    let owed: i64 = summary.people.iter().map(|p| p.balance_minor).filter(|b| *b > 0).sum();
    let owe: i64 = -summary.people.iter().map(|p| p.balance_minor).filter(|b| *b < 0).sum::<i64>();
    s.tiles(&[
        (labels.owed_to_you.clone(), fmt.money(owed), INCOME),
        (labels.you_owe.clone(), fmt.money(owe), OVER_TEXT),
        (labels.net.clone(), fmt.money(net), direction(net)),
    ]);
    let amount_right = s.right;
    let name_width = (s.right - s.left) * 0.5;
    for friend in &summary.people {
        if s.ensure(ROW) {
            s.text(s.left, s.y + 10.0, 9.0, false, MUTED, &labels.name);
            s.y += ROW;
        }
        let base = Sheet::baseline(s.y, ROW, 10.0);
        s.fit(s.left, base, 10.0, false, INK, &friend.name, name_width);
        let phrase = if friend.balance_minor < 0 { &labels.you_owe_them } else { &labels.owes_you };
        s.text(s.left + name_width + 10.0, base, 9.0, false, MUTED, phrase);
        s.right(amount_right, base, 10.0, true, direction(friend.balance_minor), &fmt.money(friend.balance_minor.abs()));
        s.line(s.y + ROW, 0.4, GRID);
        s.y += ROW;
    }
    s.footers(&labels.summary_title);
    Ok(s.doc)
}

/// The pages of the statement of one friend.
pub fn statement_doc(statement: &Statement, currency: &str, today: &str, labels: &Labels, fmt: &DisplayFormat, page: PageSize) -> Result<Doc, AppError> {
    let metrics = Metrics::new()?;
    let mut s = sheet(&metrics, labels, page);
    let title = format!("{}: {}", labels.statement_title, statement.person.name);
    s.header(&title, today, currency);
    let balance = statement.person.balance_minor;
    let summary_line = match balance {
        0 => labels.settled.clone(),
        b if b < 0 => format!("{} {}", labels.you_owe_them, fmt.money(-b)),
        b => format!("{} {}", labels.owes_you, fmt.money(b)),
    };
    s.tiles(&[(statement.person.name.clone(), summary_line, direction(balance))]);
    let (date_w, change_w, balance_w) = (62.0, 74.0, 78.0);
    let what_x = s.left + date_w + 10.0;
    let what_w = s.right - s.left - date_w - change_w - balance_w - 30.0;
    let (change_right, balance_right) = (s.right - balance_w - 10.0, s.right);
    let heads = |s: &mut Sheet| {
        let (date, what, change, bal) = (s.labels.date.clone(), s.labels.what.clone(), s.labels.change.clone(), s.labels.balance.clone());
        let y = s.y + 10.0;
        s.text(s.left, y, 8.5, true, MUTED, &date);
        s.text(what_x, y, 8.5, true, MUTED, &what);
        s.right(change_right, y, 8.5, true, MUTED, &change);
        s.right(balance_right, y, 8.5, true, MUTED, &bal);
        s.line(s.y + 16.0, 0.6, GRID);
        s.y += 20.0;
    };
    heads(&mut s);
    for line in &statement.lines {
        if s.ensure(ROW) {
            heads(&mut s);
        }
        let base = Sheet::baseline(s.y, ROW, 9.5);
        let what = match (labels.kind(&line.kind), line.what.is_empty()) {
            (kind, true) => kind.to_string(),
            (kind, false) => format!("{kind}: {}", line.what),
        };
        s.text(s.left, base, 9.5, false, MUTED, &line.date);
        s.fit(what_x, base, 9.5, false, INK, &what, what_w);
        let signed = if line.effect_minor >= 0 { format!("+{}", fmt.money(line.effect_minor)) } else { fmt.money(line.effect_minor) };
        s.right(change_right, base, 9.5, false, direction(-line.effect_minor), &signed);
        s.right(balance_right, base, 9.5, true, INK, &fmt.money(line.balance_minor));
        s.line(s.y + ROW, 0.4, GRID);
        s.y += ROW;
    }
    s.footers(&title);
    Ok(s.doc)
}

pub fn pdf(doc: &Doc, title: &str, compress: bool) -> Result<Vec<u8>, AppError> {
    render::pdf(doc, title, compress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debts::fixtures::*;
    use crate::debts::ledger::{self, Kind, NewEntry};
    use crate::debts::splits;

    fn lend(conn: &rusqlite::Connection, person: i64, kind: Kind, amount: &str, note: Option<&str>) {
        ledger::add(
            conn,
            &NewEntry { person_id: person.to_string(), kind, amount: amount.into(), date: "2026-03-05".into(), note: note.map(str::to_string), transaction_id: None },
        )
        .unwrap();
    }

    fn format() -> DisplayFormat {
        DisplayFormat { currency_symbol: "€".into(), symbol_before: false, symbol_space: true, ..Default::default() }
    }

    #[test]
    fn the_summary_names_each_friend_with_the_direction_and_the_totals() {
        let w = world();
        let (anna, bob) = (friend(&w.conn, "Anna"), friend(&w.conn, "Bob"));
        lend(&w.conn, anna, Kind::Lent, "25.00", None);
        lend(&w.conn, bob, Kind::Borrowed, "7.50", None);
        let summary = crate::debts::summary::summary(&w.conn).unwrap();
        let doc = summary_doc(&summary, "EUR", "2026-10-17", &Labels::english(), &format(), PageSize::A4).unwrap();
        let texts = doc.texts();
        for expected in ["Who owes what", "2026-10-17", "EUR", "Owed to you", "You owe", "Net", "Anna", "owes you", "Bob", "you owe"] {
            assert!(texts.contains(&expected), "{expected:?} is not on the page: {texts:?}");
        }
        let money = |m: &str| texts.iter().any(|t| t.replace('\u{a0}', " ") == m);
        assert!(money("25.00 €") && money("7.50 €") && money("17.50 €"), "{texts:?}");
        assert_eq!(doc.pages.len(), 1);
        let pdf = pdf(&doc, "Who owes what", false).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn nobody_owing_anything_says_so_and_a_long_list_goes_on_to_more_pages() {
        let w = world();
        let empty = summary_doc(&crate::debts::summary::summary(&w.conn).unwrap(), "EUR", "2026-10-17", &Labels::english(), &format(), PageSize::A4).unwrap();
        assert!(empty.texts().contains(&"Nobody owes anything."));
        for i in 0..120 {
            let id = friend(&w.conn, &format!("Friend number {i}"));
            lend(&w.conn, id, Kind::Lent, "1.00", None);
        }
        let doc = summary_doc(&crate::debts::summary::summary(&w.conn).unwrap(), "EUR", "2026-10-17", &Labels::english(), &format(), PageSize::Letter).unwrap();
        assert!(doc.pages.len() >= 4, "{}", doc.pages.len());
        let texts = doc.texts();
        assert!(texts.iter().any(|t| *t == "Page 1 / 4" || t.starts_with("Page 1 /")), "{texts:?}");
        assert_eq!(texts.iter().filter(|t| t.starts_with("Friend number ")).count(), 120, "every friend is on some page");
        assert!(texts.iter().filter(|t| **t == "Friend").count() >= doc.pages.len() - 1, "the heading is repeated on the pages that continue it");
    }

    #[test]
    fn a_statement_has_every_line_with_the_balance_after_it_and_text_the_font_lacks_becomes_a_question_mark() {
        let w = world();
        let anna = friend(&w.conn, "Anna 日本語");
        let dinner = bill(&w.conn, w.food, "Dinner", "90.00", "2026-03-04");
        splits::set_split(&w.conn, dinner, &equal(vec![who(anna)])).unwrap();
        lend(&w.conn, anna, Kind::RepaidToMe, "20.00", Some("cash"));
        let statement = ledger::statement(&w.conn, anna).unwrap();
        let doc = statement_doc(&statement, "EUR", "2026-10-17", &Labels::english(), &format(), PageSize::A4).unwrap();
        let texts = doc.texts();
        assert!(texts.iter().any(|t| t.starts_with("Statement: Anna ")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains('?') && t.starts_with("Statement")), "the characters the font lacks are question marks: {texts:?}");
        let money = |m: &str| texts.iter().any(|t| t.replace('\u{a0}', " ") == m);
        assert!(money("+45.00 €") && money("-20.00 €") && money("45.00 €") && money("25.00 €"), "{texts:?}");
        assert!(texts.contains(&"share of a bill: Dinner") && texts.contains(&"paid back to me: cash"));
        assert!(texts.iter().any(|t| t.starts_with("owes you")), "where it ends up: {texts:?}");
    }

    #[test]
    fn the_two_files_are_made_from_a_database_in_one_step_and_with_the_labels_checked() {
        let w = world();
        let anna = friend(&w.conn, "Anna");
        lend(&w.conn, anna, Kind::Lent, "25.00", Some("taxi"));
        let (labels, look) = (Labels::english(), format());
        let summary = crate::report::debts_summary_pdf(&w.conn, "2026-10-17", &labels, &look, PageSize::A4).unwrap();
        let statement = crate::report::debts_statement_pdf(&w.conn, anna, "2026-10-17", &labels, &look, PageSize::Letter).unwrap();
        assert!(summary.starts_with(b"%PDF") && statement.starts_with(b"%PDF"));
        assert_eq!(crate::report::debts_statement_pdf(&w.conn, 999, "2026-10-17", &labels, &look, PageSize::A4), Err(AppError::NotFound));
        let mut bad = Labels::english();
        bad.page = "x".repeat(200);
        assert_eq!(crate::report::debts_summary_pdf(&w.conn, "2026-10-17", &bad, &look, PageSize::A4), Err(AppError::InvalidInput));
    }

    #[test]
    fn labels_that_are_too_long_or_hold_control_characters_are_refused() {
        let mut labels = Labels::english();
        assert!(labels.validate().is_ok());
        labels.nobody = "x".repeat(121);
        assert_eq!(labels.validate(), Err(AppError::InvalidInput));
        let mut labels = Labels::english();
        labels.date = "Da\nte".into();
        assert_eq!(labels.validate(), Err(AppError::InvalidInput));
        assert!(serde_json::from_str::<Labels>(r#"{"summaryTitle":"x"}"#).is_err(), "every word must be given");
    }
}
