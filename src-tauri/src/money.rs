//! Money crosses the UI boundary as decimal strings ("12.50") and is stored as integer minor
//! units (1250). No floating point anywhere on the way.
//!
//! Two decimals are assumed for every currency for now. Currencies with a different number of
//! minor digits (JPY, BHD) need an exponent per currency before they are fully supported.

use crate::vault::AppError;

/// Largest storable amount in minor units; the same bound the database CHECKs use.
pub const MAX_MINOR: i64 = 999_999_999_999;

/// Parses "0.01" to "9999999999.99": 1 to 10 whole digits (no leading zeros), exactly two decimals.
/// Zero is accepted here; callers that need a positive amount check it themselves.
pub fn parse_amount(text: &str) -> Result<i64, AppError> {
    let (whole, fraction) = text.split_once('.').ok_or(AppError::InvalidInput)?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || whole.len() > 10 || (whole.len() > 1 && whole.starts_with('0')) {
        return Err(AppError::InvalidInput);
    }
    if fraction.len() != 2 || !digits(fraction) {
        return Err(AppError::InvalidInput);
    }
    let minor: i64 = format!("{whole}{fraction}").parse().map_err(|_| AppError::InvalidInput)?;
    if minor > MAX_MINOR {
        return Err(AppError::InvalidInput);
    }
    Ok(minor)
}

pub fn parse_positive_amount(text: &str) -> Result<i64, AppError> {
    match parse_amount(text)? {
        0 => Err(AppError::InvalidInput),
        minor => Ok(minor),
    }
}

/// Negative values appear in results such as "remaining" when spending passes the budget.
pub fn format_amount(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exact_two_decimal_strings() {
        assert_eq!(parse_amount("0.01"), Ok(1));
        assert_eq!(parse_amount("12.50"), Ok(1250));
        assert_eq!(parse_amount("1000.00"), Ok(100_000));
        assert_eq!(parse_amount("9999999999.99"), Ok(MAX_MINOR));
        assert_eq!(parse_amount("0.00"), Ok(0));
    }

    #[test]
    fn rejects_everything_else() {
        for bad in [
            "", ".", "1", "1.", "1.5", "1.234", ".50", "01.00", "-1.00", "+1.00", "1,00", " 1.00", "1.00 ", "1e3",
            "10000000000.00", "99999999999.99", "abc", "1.0a", "١.٠٠",
        ] {
            assert_eq!(parse_amount(bad), Err(AppError::InvalidInput), "{bad:?}");
        }
    }

    #[test]
    fn positive_rejects_zero() {
        assert_eq!(parse_positive_amount("0.00"), Err(AppError::InvalidInput));
        assert_eq!(parse_positive_amount("0.01"), Ok(1));
    }

    #[test]
    fn formats_back_without_loss() {
        assert_eq!(format_amount(1), "0.01");
        assert_eq!(format_amount(1250), "12.50");
        assert_eq!(format_amount(MAX_MINOR), "9999999999.99");
        assert_eq!(format_amount(0), "0.00");
        assert_eq!(format_amount(-5), "-0.05");
        assert_eq!(format_amount(-1250), "-12.50");
        for text in ["0.07", "3.10", "100.00", "9999999999.99"] {
            assert_eq!(format_amount(parse_amount(text).unwrap()), text);
        }
    }
}
