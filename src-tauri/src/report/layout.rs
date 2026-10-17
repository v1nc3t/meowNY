//! Puts the report on pages. The result is a list of drawing operations per page (`Doc`), in points
//! with the origin at the top left, which `render` turns into a PDF. Keeping this step separate means
//! the tests can read exactly which text goes on which page.

use skrifa::charmap::Charmap;
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::GlyphMetrics;
use skrifa::{FontRef, MetadataProvider};

use super::{ReportData, ReportLabels, ReportOptions};
use crate::categories::CategoryType;
use crate::display::DisplayFormat;
use crate::stats::pct_tenths;
use crate::vault::AppError;

pub(super) const REGULAR_FONT: &[u8] = include_bytes!("../../fonts/DejaVuSans.ttf");
pub(super) const BOLD_FONT: &[u8] = include_bytes!("../../fonts/DejaVuSans-Bold.ttf");

pub type Color = (u8, u8, u8);

const INK: Color = (0x1f, 0x23, 0x28);
const MUTED: Color = (0x5f, 0x66, 0x73);
const GRID: Color = (0xd8, 0xdc, 0xe2);
const TRACK: Color = (0xec, 0xee, 0xf1);
const CARD: Color = (0xf5, 0xf6, 0xf8);
const WHITE: Color = (0xff, 0xff, 0xff);
// The dashboard's colours: income green, expenses red, the rest of the palette for the donut.
const INCOME: Color = (0x2e, 0x8b, 0x57);
const EXPENSE: Color = (0xf7, 0x76, 0x8e);
const OVER_TEXT: Color = (0xc0, 0x39, 0x4d);
const WITHIN: Color = (0x7a, 0xa2, 0xf7);
const PALETTE: [Color; 8] = [
    (0x2e, 0x8b, 0x57),
    (0x7a, 0xa2, 0xf7),
    (0xe0, 0xaf, 0x68),
    (0xbb, 0x9a, 0xf7),
    (0x5f, 0xb3, 0xa1),
    (0xf7, 0x76, 0x8e),
    (0x7d, 0xcf, 0xff),
    (0x8a, 0x8f, 0x98),
];

const MARGIN: f32 = 44.0;
const FOOTER_SPACE: f32 = 34.0;
const CHART_HEIGHT: f32 = 150.0;
const DONUT_OUTER: f32 = 58.0;
const DONUT_INNER: f32 = 33.0;
const DONUT_SLICES: usize = 8;

#[derive(Debug, Clone)]
pub enum Op {
    /// `y` is the baseline.
    Text { x: f32, y: f32, size: f32, bold: bool, color: Color, text: String },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Color },
    Poly { points: Vec<(f32, f32)>, fill: Color, outline: Option<(Color, f32)> },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color },
}

pub struct Doc {
    pub width: f32,
    pub height: f32,
    pub pages: Vec<Vec<Op>>,
}

impl Doc {
    /// Every piece of text, in drawing order.
    #[cfg(test)]
    pub fn texts(&self) -> Vec<&str> {
        self.pages
            .iter()
            .flatten()
            .filter_map(|op| if let Op::Text { text, .. } = op { Some(text.as_str()) } else { None })
            .collect()
    }
}

struct Face {
    cmap: Charmap<'static>,
    advance: GlyphMetrics<'static>,
    units_per_em: f32,
}

impl Face {
    fn new(data: &'static [u8]) -> Result<Face, AppError> {
        let font = FontRef::new(data).map_err(|_| AppError::Internal)?;
        Ok(Face {
            cmap: font.charmap(),
            advance: font.glyph_metrics(Size::unscaled(), LocationRef::default()),
            units_per_em: font.metrics(Size::unscaled(), LocationRef::default()).units_per_em as f32,
        })
    }

    fn has(&self, c: char) -> bool {
        self.cmap.map(c).is_some()
    }

    fn width(&self, text: &str, size: f32) -> f32 {
        let units: f32 = text
            .chars()
            .map(|c| self.cmap.map(c).and_then(|g| self.advance.advance_width(g)).unwrap_or(self.units_per_em * 0.6))
            .sum();
        units * size / self.units_per_em
    }
}

/// Measures text in the two embedded fonts, so columns can be aligned and long text cut to fit.
pub struct Metrics {
    regular: Face,
    bold: Face,
}

impl Metrics {
    pub fn new() -> Result<Metrics, AppError> {
        Ok(Metrics { regular: Face::new(REGULAR_FONT)?, bold: Face::new(BOLD_FONT)? })
    }

    fn face(&self, bold: bool) -> &Face {
        if bold {
            &self.bold
        } else {
            &self.regular
        }
    }

    pub fn width(&self, text: &str, size: f32, bold: bool) -> f32 {
        self.face(bold).width(text, size)
    }

    /// Text the font can draw: control characters go, and a character the font lacks becomes `?`.
    pub fn clean(&self, text: &str) -> String {
        text.chars()
            .filter(|c| !c.is_control())
            .map(|c| if self.regular.has(c) && self.bold.has(c) { c } else { '?' })
            .collect()
    }

    /// The text cut short with an ellipsis when it is wider than `max`.
    pub fn fit(&self, text: &str, size: f32, bold: bool, max: f32) -> String {
        let text = self.clean(text);
        if self.width(&text, size, bold) <= max {
            return text;
        }
        let mut chars: Vec<char> = text.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "\u{2026}";
            if self.width(&candidate, size, bold) <= max {
                return candidate;
            }
        }
        String::new()
    }
}

/// The top of a chart axis in minor units and the size of one step, such as 400 and 100 for 356.20:
/// four steps of 1, 2, 2.5 or 5 times a power of ten.
pub fn axis_top(max_minor: i64) -> (i64, i64) {
    if max_minor <= 0 {
        return (10_000, 2_500);
    }
    let per_step = (max_minor as f64 / 100.0 / 4.0).max(0.01);
    let power = 10f64.powf(per_step.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0].into_iter().map(|m| m * power).find(|s| *s >= per_step - 1e-9).unwrap_or(10.0 * power);
    let step_minor = (step * 100.0).round() as i64;
    (step_minor * 4, step_minor)
}

/// Every `n`th label, so the labels along an axis never run into each other.
pub fn label_stride(count: usize, slot: f32, widest: f32) -> usize {
    if count == 0 || slot <= 0.0 {
        return 1;
    }
    ((widest + 6.0) / slot).ceil().max(1.0) as usize
}

/// The donut's slices: the biggest seven and everything else together, as (name, amount).
pub fn donut_slices(spending: &[(String, i64)], other: &str) -> Vec<(String, i64)> {
    let mut slices: Vec<(String, i64)> = spending.iter().filter(|(_, m)| *m > 0).take(DONUT_SLICES - 1).cloned().collect();
    let rest: i64 = spending.iter().filter(|(_, m)| *m > 0).skip(DONUT_SLICES - 1).map(|(_, m)| m).sum();
    if rest > 0 {
        slices.push((other.to_string(), rest));
    }
    slices
}

struct Builder<'a> {
    m: &'a Metrics,
    fmt: &'a DisplayFormat,
    labels: &'a ReportLabels,
    doc: Doc,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    y: f32,
}

impl Builder<'_> {
    fn ops(&mut self) -> &mut Vec<Op> {
        self.doc.pages.last_mut().expect("a page is always open")
    }

    fn new_page(&mut self) {
        self.doc.pages.push(Vec::new());
        self.y = self.top;
    }

    /// Starts a new page unless `height` still fits. True if it did start one.
    fn ensure(&mut self, height: f32) -> bool {
        if self.y + height > self.bottom {
            self.new_page();
            true
        } else {
            false
        }
    }

    fn width(&self) -> f32 {
        self.right - self.left
    }

    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Color, text: &str) {
        let text = self.m.clean(text);
        if !text.is_empty() {
            self.ops().push(Op::Text { x, y, size, bold, color, text });
        }
    }

    /// Text that is cut to `max` wide.
    #[allow(clippy::too_many_arguments)] // a position, a size, a style, the text and a width: one call draws one piece of text
    fn text_fit(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Color, text: &str, max: f32) {
        let text = self.m.fit(text, size, bold, max);
        self.text(x, y, size, bold, color, &text);
    }

    fn text_right(&mut self, right: f32, y: f32, size: f32, bold: bool, color: Color, text: &str) {
        let text = self.m.clean(text);
        let x = right - self.m.width(&text, size, bold);
        self.text(x, y, size, bold, color, &text);
    }

    fn text_center(&mut self, center: f32, y: f32, size: f32, color: Color, text: &str) {
        let text = self.m.clean(text);
        let x = center - self.m.width(&text, size, false) / 2.0;
        self.text(x, y, size, false, color, &text);
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if w > 0.0 && h > 0.0 {
            self.ops().push(Op::Rect { x, y, w, h, color });
        }
    }

    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color) {
        self.ops().push(Op::Line { x1, y1, x2, y2, width, color });
    }

    /// The baseline that centres text of `size` in a row of `height` starting at `top`.
    fn baseline(top: f32, height: f32, size: f32) -> f32 {
        top + (height + size * 0.72) / 2.0
    }

    fn header(&mut self, data: &ReportData) {
        let (title, period) = (self.labels.title.clone(), self.labels.period.clone());
        self.text(self.left, self.y + 18.0, 20.0, true, INK, &title);
        self.text_right(self.right, self.y + 18.0, 9.0, false, MUTED, &data.currency);
        self.text(self.left, self.y + 36.0, 11.0, false, MUTED, &period);
        self.line(self.left, self.y + 46.0, self.right, self.y + 46.0, 0.8, GRID);
        self.y += 62.0;
    }

    fn tiles(&mut self, data: &ReportData) {
        let savings = data.income - data.expense;
        let rate = data.savings_rate.map_or_else(|| "\u{2013}".to_string(), |r| self.fmt.percent(r));
        let tiles = [
            (self.labels.income.clone(), self.fmt.money(data.income)),
            (self.labels.expenses.clone(), self.fmt.money(data.expense)),
            (self.labels.savings.clone(), self.fmt.money(savings)),
            (self.labels.savings_rate.clone(), rate),
        ];
        let gap = 8.0;
        let w = (self.width() - gap * 3.0) / 4.0;
        for (i, (label, value)) in tiles.iter().enumerate() {
            let x = self.left + i as f32 * (w + gap);
            self.rect(x, self.y, w, 48.0, CARD);
            self.text_fit(x + 9.0, self.y + 17.0, 8.5, false, MUTED, label, w - 14.0);
            self.text_fit(x + 9.0, self.y + 38.0, 13.0, true, INK, value, w - 14.0);
        }
        self.y += 48.0 + 20.0;
    }

    fn section_title(&mut self, title: &str, keep_with: f32) {
        self.ensure(24.0 + keep_with);
        let title = title.to_string();
        self.text(self.left, self.y + 12.0, 12.0, true, INK, &title);
        self.y += 24.0;
    }

    fn month_chart(&mut self, data: &ReportData) {
        let title = self.labels.by_month.clone();
        self.section_title(&title, CHART_HEIGHT);

        let peak = data.months.iter().map(|m| m.income.max(m.expense)).max().unwrap_or(0);
        let (top, step) = axis_top(peak);
        let widest = (0..=4).map(|i| self.m.width(&self.fmt.whole(i * step), 7.5, false)).fold(0.0, f32::max);
        let plot_left = self.left + widest + 8.0;
        let plot_top = self.y + 14.0;
        let plot_h = CHART_HEIGHT - 14.0 - 18.0;
        let plot_w = self.right - plot_left;

        // Legend, top right.
        let mut x = self.right;
        for (label, color) in [(self.labels.expenses.clone(), EXPENSE), (self.labels.income.clone(), INCOME)] {
            let text = self.m.clean(&label);
            x -= self.m.width(&text, 8.0, false);
            self.text(x, self.y + 8.0, 8.0, false, MUTED, &text);
            x -= 12.0;
            self.rect(x, self.y + 1.0, 8.0, 8.0, color);
            x -= 12.0;
        }

        for i in 0..=4i64 {
            let value = i * step;
            let gy = plot_top + plot_h - plot_h * (value as f32 / top as f32);
            self.line(plot_left, gy, self.right, gy, 0.4, GRID);
            self.text_right(plot_left - 5.0, gy + 2.6, 7.5, false, MUTED, &self.fmt.whole(value));
        }

        let n = data.months.len();
        let slot = plot_w / n as f32;
        let bar = (slot * 0.34).clamp(1.5, 16.0);
        let widest_label = data.months.iter().map(|m| self.m.width(&self.fmt.month(&m.month), 7.5, false)).fold(0.0, f32::max);
        let stride = label_stride(n, slot, widest_label);
        for (i, m) in data.months.iter().enumerate() {
            let center = plot_left + slot * (i as f32 + 0.5);
            for (value, color, x) in [(m.income, INCOME, center - bar - 0.5), (m.expense, EXPENSE, center + 0.5)] {
                let h = plot_h * (value as f32 / top as f32);
                self.rect(x, plot_top + plot_h - h, bar, h, color);
            }
            if i % stride == 0 {
                self.text_center(center, plot_top + plot_h + 12.0, 7.5, MUTED, &self.fmt.month(&m.month));
            }
        }
        self.y += CHART_HEIGHT + 16.0;
    }

    fn donut(&mut self, data: &ReportData) {
        let slices = donut_slices(&data.spending, &self.labels.other);
        let total: i64 = slices.iter().map(|(_, m)| m).sum();
        if total == 0 {
            return;
        }
        let title = self.labels.spending.clone();
        self.section_title(&title, DONUT_OUTER * 2.0 + 10.0);
        let (cx, cy) = (self.left + DONUT_OUTER + 4.0, self.y + DONUT_OUTER + 4.0);
        let mut start = -std::f32::consts::FRAC_PI_2;
        for (i, (_, minor)) in slices.iter().enumerate() {
            let sweep = std::f32::consts::TAU * (*minor as f32 / total as f32);
            let steps = ((sweep / (std::f32::consts::PI / 60.0)).ceil() as usize).max(2);
            let mut points = Vec::with_capacity(steps * 2 + 2);
            for s in 0..=steps {
                let a = start + sweep * s as f32 / steps as f32;
                points.push((cx + DONUT_OUTER * a.cos(), cy + DONUT_OUTER * a.sin()));
            }
            for s in (0..=steps).rev() {
                let a = start + sweep * s as f32 / steps as f32;
                points.push((cx + DONUT_INNER * a.cos(), cy + DONUT_INNER * a.sin()));
            }
            let color = if slices.len() == DONUT_SLICES && i == DONUT_SLICES - 1 { PALETTE[DONUT_SLICES - 1] } else { PALETTE[i % PALETTE.len()] };
            let outline = (slices.len() > 1).then_some((WHITE, 0.8));
            self.ops().push(Op::Poly { points, fill: color, outline });
            start += sweep;
        }

        // The legend, to the right of the donut.
        let lx = cx + DONUT_OUTER + 26.0;
        let row = 15.0;
        let first = cy - row * slices.len() as f32 / 2.0;
        let percent_w = self.m.width("100.0%", 8.5, false);
        for (i, (name, minor)) in slices.iter().enumerate() {
            let top = first + i as f32 * row;
            let base = Self::baseline(top, row, 8.5);
            self.rect(lx, top + 3.5, 8.0, 8.0, PALETTE[i % PALETTE.len()]);
            let amount = self.fmt.money(*minor);
            let amount_w = self.m.width(&amount, 8.5, false);
            let share = pct_tenths(*minor, total).unwrap_or(0.0);
            self.text_right(self.right, base, 8.5, false, MUTED, &self.fmt.percent(share));
            self.text_right(self.right - percent_w - 10.0, base, 8.5, false, INK, &amount);
            let room = self.right - lx - 14.0 - percent_w - 10.0 - amount_w - 10.0;
            self.text_fit(lx + 14.0, base, 8.5, false, INK, name, room.max(20.0));
        }
        self.y += DONUT_OUTER * 2.0 + 8.0 + 16.0;
    }

    fn bar_row(&mut self, name: &str, actual: i64, budget: Option<i64>, kind: CategoryType, bold: bool) {
        self.ensure(18.0);
        let row = 17.0;
        let base = Self::baseline(self.y, row, 9.0);
        let w = self.width();
        let name_w = w * 0.34;
        let bar_x = self.left + w * 0.66;
        let bar_w = w * 0.22;
        self.text_fit(self.left, base, 9.0, bold, INK, name, name_w - 8.0);
        match budget {
            Some(limit) => {
                let text = format!("{} / {}", self.fmt.money(actual), self.fmt.money(limit));
                self.text_right(self.left + w * 0.63, base, 8.5, bold, INK, &text);
                let pct = pct_tenths(actual, limit);
                let fraction = pct.map_or(if actual > 0 { 1.0 } else { 0.0 }, |p| (p / 100.0).clamp(0.0, 1.0)) as f32;
                let over = kind == CategoryType::Expense && actual > limit;
                let color = match (kind, over) {
                    (CategoryType::Income, _) => INCOME,
                    (_, true) => EXPENSE,
                    _ => WITHIN,
                };
                self.rect(bar_x, self.y + 5.0, bar_w, 7.0, TRACK);
                self.rect(bar_x, self.y + 5.0, bar_w * fraction, 7.0, color);
                let shown = pct.map_or_else(|| "\u{2013}".to_string(), |p| self.fmt.percent(p));
                self.text_right(self.right, base, 8.5, bold || over, if over { OVER_TEXT } else { INK }, &shown);
            }
            None => {
                self.text_right(self.left + w * 0.63, base, 8.5, bold, INK, &self.fmt.money(actual));
                let none = self.labels.no_budget.clone();
                self.text_fit(bar_x, base, 8.0, false, MUTED, &none, self.right - bar_x);
            }
        }
        self.line(self.left, self.y + row, self.right, self.y + row, 0.3, GRID);
        self.y += row;
    }

    fn budget_section(&mut self, data: &ReportData) {
        if data.budget_lines.is_empty() && data.global_budgets.is_empty() {
            return;
        }
        let title = self.labels.budget_vs_actual.clone();
        self.section_title(&title, 18.0 * 3.0);
        for kind in [CategoryType::Expense, CategoryType::Income] {
            let lines: Vec<_> = data.budget_lines.iter().filter(|l| l.kind == kind).collect();
            let global = data.global_budgets.iter().find(|g| g.kind == kind);
            if lines.is_empty() && global.is_none() {
                continue;
            }
            self.ensure(18.0 * 3.0);
            let heading = if kind == CategoryType::Expense { self.labels.expenses.clone() } else { self.labels.income.clone() };
            self.text(self.left, self.y + 10.0, 9.5, true, MUTED, &heading.to_uppercase());
            self.y += 17.0;
            if let Some(g) = global {
                self.bar_row(&heading, g.actual, Some(g.budgeted), kind, true);
            }
            for line in lines {
                self.bar_row(&line.name, line.actual, line.budgeted, kind, false);
            }
            self.y += 10.0;
        }
        self.y += 6.0;
    }

    fn table_header(&mut self) {
        let (date, category, name, amount) =
            (self.labels.date.clone(), self.labels.category.clone(), self.labels.name.clone(), self.labels.amount.clone());
        let base = Self::baseline(self.y, 16.0, 8.5);
        let cols = self.columns();
        self.text(cols.date, base, 8.5, true, MUTED, &date);
        self.text(cols.category, base, 8.5, true, MUTED, &category);
        self.text(cols.name, base, 8.5, true, MUTED, &name);
        self.text_right(self.right, base, 8.5, true, MUTED, &amount);
        self.line(self.left, self.y + 17.0, self.right, self.y + 17.0, 0.8, GRID);
        self.y += 20.0;
    }

    fn columns(&self) -> Columns {
        let date = self.left;
        let category = self.left + 66.0;
        let name = category + 118.0;
        Columns { date, category, name }
    }

    fn transactions(&mut self, data: &ReportData, show_descriptions: bool) {
        let title = self.labels.transactions.clone();
        self.section_title(&title, 20.0 + 26.0 * 2.0);
        self.table_header();
        let cols = self.columns();
        let amount_w = 84.0;
        for t in &data.transactions {
            let description = if show_descriptions { t.description.as_deref().filter(|d| !d.is_empty()) } else { None };
            let row = if description.is_some() { 27.0 } else { 16.0 };
            if self.ensure(row) {
                self.table_header();
            }
            let base = Self::baseline(self.y, 16.0, 8.5);
            self.text(cols.date, base, 8.5, false, INK, &self.fmt.date(&t.date));
            self.text_fit(cols.category, base, 8.5, false, INK, &t.category, 110.0);
            self.text_fit(cols.name, base, 8.5, false, INK, &t.name, self.right - amount_w - cols.name - 8.0);
            let income = t.kind == "INCOME";
            self.text_right(self.right, base, 8.5, false, if income { INCOME } else { INK }, &self.fmt.money(t.my_share));
            if let Some(description) = description {
                self.text_fit(cols.name, base + 10.0, 7.5, false, MUTED, description, self.right - cols.name);
            }
            self.line(self.left, self.y + row, self.right, self.y + row, 0.3, GRID);
            self.y += row;
        }
    }

    fn footers(&mut self) {
        let count = self.doc.pages.len();
        let (title, page) = (self.labels.title.clone(), self.labels.page.clone());
        for i in 0..count {
            self.doc.pages[i].push(Op::Line {
                x1: self.left,
                y1: self.doc.height - MARGIN + 8.0,
                x2: self.right,
                y2: self.doc.height - MARGIN + 8.0,
                width: 0.5,
                color: GRID,
            });
            let y = self.doc.height - MARGIN + 22.0;
            let label = self.m.clean(&format!("{page} {} / {count}", i + 1));
            let x = self.right - self.m.width(&label, 8.0, false);
            let title = self.m.clean(&title);
            self.doc.pages[i].push(Op::Text { x: self.left, y, size: 8.0, bold: false, color: MUTED, text: title });
            self.doc.pages[i].push(Op::Text { x, y, size: 8.0, bold: false, color: MUTED, text: label });
        }
    }
}

struct Columns {
    date: f32,
    category: f32,
    name: f32,
}

pub fn build(data: &ReportData, options: &ReportOptions) -> Result<Doc, AppError> {
    let metrics = Metrics::new()?;
    let (width, height) = options.page.points();
    let mut b = Builder {
        m: &metrics,
        fmt: &options.format,
        labels: &options.labels,
        doc: Doc { width, height, pages: Vec::new() },
        left: MARGIN,
        right: width - MARGIN,
        top: MARGIN,
        bottom: height - MARGIN - FOOTER_SPACE + 14.0,
        y: MARGIN,
    };
    b.new_page();
    b.header(data);
    b.tiles(data);
    if data.income == 0 && data.expense == 0 && data.budget_lines.is_empty() && data.transactions.is_empty() {
        let none = b.labels.no_data.clone();
        b.text(b.left, b.y + 10.0, 10.0, false, MUTED, &none);
    } else {
        b.month_chart(data);
        b.donut(data);
        b.budget_section(data);
        if options.include_transactions && !data.transactions.is_empty() {
            b.transactions(data, options.show_descriptions);
        }
    }
    b.footers();
    Ok(b.doc)
}
