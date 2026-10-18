//! How numbers, money and dates look in files the app writes for people to read: the Excel export
//! and the PDF report. The UI works the format out from the person's number and date format setting
//! (a locale such as "de-DE") with the browser's `Intl`; this side only applies it.
//!
//! Excel and LibreOffice pick the digit grouping and the decimal mark themselves, so the Excel export
//! uses only the date order, the date separator and the currency symbol. The PDF is plain text, so it
//! needs all of it.

use serde::Deserialize;

use crate::vault::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum DateOrder {
    #[serde(rename = "DMY")]
    DayMonthYear,
    #[serde(rename = "MDY")]
    MonthDayYear,
    #[default]
    #[serde(rename = "YMD")]
    YearMonthDay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum DateSeparator {
    #[serde(rename = "/")]
    Slash,
    #[serde(rename = ".")]
    Dot,
    #[default]
    #[serde(rename = "-")]
    Dash,
    #[serde(rename = " ")]
    Space,
}

impl DateSeparator {
    fn text(self) -> &'static str {
        match self {
            DateSeparator::Slash => "/",
            DateSeparator::Dot => ".",
            DateSeparator::Dash => "-",
            DateSeparator::Space => "\u{a0}",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct DisplayFormat {
    pub date_order: DateOrder,
    pub date_separator: DateSeparator,
    /// "€", "$", "kr". Empty for none; the currency code is shown in headings either way.
    pub currency_symbol: String,
    pub symbol_before: bool,
    /// A space between the symbol and the number.
    pub symbol_space: bool,
    /// One character, as in "1.234,50".
    pub decimal_mark: String,
    /// Between groups of three digits; empty for none.
    pub group_separator: String,
}

impl Default for DisplayFormat {
    fn default() -> Self {
        DisplayFormat {
            date_order: DateOrder::default(),
            date_separator: DateSeparator::default(),
            currency_symbol: String::new(),
            symbol_before: false,
            symbol_space: false,
            decimal_mark: ".".to_string(),
            group_separator: ",".to_string(),
        }
    }
}

fn plain_char(c: char) -> bool {
    !c.is_control() && !c.is_ascii_digit() && c != '"'
}

/// Spaces of any width become one no-break space, which every font has and no line break splits.
fn tidy(text: &str) -> String {
    text.chars().map(|c| if c.is_whitespace() { '\u{a0}' } else { c }).collect()
}

impl DisplayFormat {
    /// Refuses what the files cannot hold. The format arrives from the webview, so it is checked.
    pub fn validate(&self) -> Result<(), AppError> {
        let symbol_ok = self.currency_symbol.chars().count() <= 8 && self.currency_symbol.chars().all(plain_char);
        let mark_ok = self.decimal_mark.chars().count() == 1 && self.decimal_mark.chars().all(plain_char);
        let group_ok = self.group_separator.chars().count() <= 1 && self.group_separator.chars().all(plain_char);
        if symbol_ok && mark_ok && group_ok && tidy(&self.decimal_mark) != tidy(&self.group_separator) {
            Ok(())
        } else {
            Err(AppError::InvalidInput)
        }
    }

    fn grouped(&self, whole: u64) -> String {
        let digits = whole.to_string();
        let group = tidy(&self.group_separator);
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push_str(&group);
            }
            out.push(c);
        }
        out
    }

    /// 123456 minor units as "1,234.56", with the sign and no symbol.
    pub fn number(&self, minor: i64) -> String {
        let abs = minor.unsigned_abs();
        let sign = if minor < 0 { "-" } else { "" };
        format!("{sign}{}{}{:02}", self.grouped(abs / 100), tidy(&self.decimal_mark), abs % 100)
    }

    /// Whole units only, for the labels on a chart axis.
    pub fn whole(&self, minor: i64) -> String {
        let sign = if minor < 0 { "-" } else { "" };
        format!("{sign}{}", self.grouped(minor.unsigned_abs() / 100))
    }

    /// With the currency symbol where the format puts it.
    pub fn money(&self, minor: i64) -> String {
        let number = self.number(minor);
        if self.currency_symbol.is_empty() {
            return number;
        }
        let gap = if self.symbol_space { "\u{a0}" } else { "" };
        let symbol = tidy(&self.currency_symbol);
        match (self.symbol_before, number.strip_prefix('-')) {
            (true, Some(rest)) => format!("-{symbol}{gap}{rest}"),
            (true, None) => format!("{symbol}{gap}{number}"),
            (false, _) => format!("{number}{gap}{symbol}"),
        }
    }

    /// 12.5 as "12.5%" (one decimal, the decimal mark of the format).
    pub fn percent(&self, value: f64) -> String {
        format!("{:.1}", value).replace('.', &tidy(&self.decimal_mark)) + "%"
    }

    /// "2026-10-14" in the order and with the separator of the format.
    pub fn date(&self, iso: &str) -> String {
        let mut parts = iso.splitn(3, '-');
        let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else { return iso.to_string() };
        let sep = self.date_separator.text();
        match self.date_order {
            DateOrder::DayMonthYear => format!("{d}{sep}{m}{sep}{y}"),
            DateOrder::MonthDayYear => format!("{m}{sep}{d}{sep}{y}"),
            DateOrder::YearMonthDay => format!("{y}{sep}{m}{sep}{d}"),
        }
    }

    /// "2026-10" as "10/2026" or "2026/10".
    pub fn month(&self, iso: &str) -> String {
        let Some((y, m)) = iso.split_once('-') else { return iso.to_string() };
        let sep = self.date_separator.text();
        match self.date_order {
            DateOrder::YearMonthDay => format!("{y}{sep}{m}"),
            _ => format!("{m}{sep}{y}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn german() -> DisplayFormat {
        DisplayFormat {
            date_order: DateOrder::DayMonthYear,
            date_separator: DateSeparator::Dot,
            currency_symbol: "€".into(),
            symbol_before: false,
            symbol_space: true,
            decimal_mark: ",".into(),
            group_separator: ".".into(),
        }
    }

    fn american() -> DisplayFormat {
        DisplayFormat {
            date_order: DateOrder::MonthDayYear,
            date_separator: DateSeparator::Slash,
            currency_symbol: "$".into(),
            symbol_before: true,
            ..Default::default()
        }
    }

    #[test]
    fn money_follows_the_marks_and_the_symbol_position() {
        assert_eq!(american().money(123_456), "$1,234.56");
        assert_eq!(german().money(123_456), "1.234,56\u{a0}€");
        assert_eq!(DisplayFormat::default().money(123_456), "1,234.56");
        let french = DisplayFormat { group_separator: "\u{202f}".into(), decimal_mark: ",".into(), ..german() };
        assert_eq!(french.money(123_456_789), "1\u{a0}234\u{a0}567,89\u{a0}€", "narrow spaces become no-break spaces");
    }

    #[test]
    fn small_and_negative_amounts() {
        let f = american();
        for (minor, shown) in [(0, "$0.00"), (5, "$0.05"), (100, "$1.00"), (99_999, "$999.99")] {
            assert_eq!(f.money(minor), shown);
        }
        assert_eq!(f.money(-500), "-$5.00");
        assert_eq!(f.money(-123_456), "-$1,234.56");
        assert_eq!(german().money(-500), "-5,00\u{a0}€");
        assert_eq!(f.number(999_999_999_999), "9,999,999,999.99");
        assert!(f.number(i64::MIN).len() > 10, "no overflow on the extreme value");
    }

    #[test]
    fn a_format_the_files_cannot_hold_is_refused() {
        assert!(german().validate().is_ok());
        assert!(DisplayFormat::default().validate().is_ok());
        for bad in [
            DisplayFormat { currency_symbol: "\"".into(), ..Default::default() },
            DisplayFormat { currency_symbol: "123456789".into(), ..Default::default() },
            DisplayFormat { currency_symbol: "\n".into(), ..Default::default() },
            DisplayFormat { decimal_mark: "".into(), ..Default::default() },
            DisplayFormat { decimal_mark: ".,".into(), ..Default::default() },
            DisplayFormat { decimal_mark: "5".into(), ..Default::default() },
            DisplayFormat { group_separator: "55".into(), ..Default::default() },
            DisplayFormat { group_separator: ".".into(), ..Default::default() },
            DisplayFormat { group_separator: "\u{a0}".into(), decimal_mark: "\u{202f}".into(), ..Default::default() },
        ] {
            assert_eq!(bad.validate(), Err(AppError::InvalidInput), "{bad:?}");
        }
        assert!(DisplayFormat { group_separator: "".into(), ..Default::default() }.validate().is_ok(), "no grouping is allowed");
    }

    #[test]
    fn the_format_is_read_from_the_webview_strictly() {
        let ok: DisplayFormat = serde_json::from_str(
            r#"{"dateOrder":"DMY","dateSeparator":".","currencySymbol":"kr","symbolBefore":false,"decimalMark":",","groupSeparator":" "}"#,
        )
        .unwrap();
        assert_eq!((ok.date_order, ok.decimal_mark.as_str()), (DateOrder::DayMonthYear, ","));
        let partial: DisplayFormat = serde_json::from_str(r#"{"dateOrder":"MDY"}"#).unwrap();
        assert_eq!(partial.decimal_mark, ".", "what is left out keeps its default");
        assert!(serde_json::from_str::<DisplayFormat>(r#"{"dateOrder":"XYZ"}"#).is_err());
        assert!(serde_json::from_str::<DisplayFormat>(r#"{"dateSeparator":"; DROP"}"#).is_err());
        assert!(serde_json::from_str::<DisplayFormat>(r#"{"somethingElse":1}"#).is_err());
    }
}
