//! Rates: the maths of the interest of a liability, the return of a savings account and the return of a term deposit
//! (`docs/loans-deposits-design.md`, sections 3 and 6). Pure functions on integers: no database and no clock.
//!
//! * A rate is a percent with at most 3 decimals, from 0 to 100, kept as `milli` = percent times 1000 (3.25 % is 3250), with a period:
//!   per month or per year.
//! * Every estimate is simple interest, worked out in `i128` as one division and rounded half away from zero to the minor unit.

use chrono::{Datelike, Months, NaiveDate};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use crate::money::format_amount;
use crate::vault::AppError;

/// The biggest rate: 100 %.
pub const MAX_MILLI: i64 = 100_000;

/// A payoff that needs more months than this is `Payoff::Never` (a hundred years): the loop stops, and so does a rate of 0.
pub const MAX_MONTHS: u32 = 1200;

/// What a rate is a percent of: per month, or per year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Period {
    Monthly,
    Yearly,
}

impl Period {
    pub fn as_sql(self) -> &'static str {
        match self {
            Period::Monthly => "MONTHLY",
            Period::Yearly => "YEARLY",
        }
    }

    pub fn from_sql(text: &str) -> Result<Period, AppError> {
        match text {
            "MONTHLY" => Ok(Period::Monthly),
            "YEARLY" => Ok(Period::Yearly),
            _ => Err(AppError::Internal),
        }
    }
}

/// "0" to "100" with at most 3 decimals ("3.25", "4.125", "0.5") to milli. The same strict shape as `money::parse_amount`: digits only, no
/// sign, no spaces, no leading zero, and a point needs digits on both sides ("3." and ".5" are refused).
pub fn parse_rate(text: &str) -> Result<i64, AppError> {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || whole.len() > 3 || (whole.len() > 1 && whole.starts_with('0')) {
        return Err(AppError::InvalidInput);
    }
    let mut milli: i64 = whole.parse().map_err(|_| AppError::InvalidInput)?;
    milli *= 1000;
    if let Some(fraction) = fraction {
        if !digits(fraction) || fraction.len() > 3 {
            return Err(AppError::InvalidInput);
        }
        let scale = 10i64.pow(3 - fraction.len() as u32);
        milli += fraction.parse::<i64>().map_err(|_| AppError::InvalidInput)? * scale;
    }
    if milli > MAX_MILLI {
        return Err(AppError::InvalidInput);
    }
    Ok(milli)
}

/// Milli back to the decimal string, without trailing zeros and without the point when whole: 3250 is "3.25", 4000 is "4", 0 is "0".
pub fn format_rate(milli: i64) -> String {
    let (whole, fraction) = (milli / 1000, milli % 1000);
    if fraction == 0 {
        return whole.to_string();
    }
    format!("{whole}.{fraction:03}").trim_end_matches('0').to_string()
}

/// `numerator / denominator` rounded half away from zero. The denominator is above 0.
fn divide_rounded(numerator: i128, denominator: i128) -> i128 {
    let size = (numerator.abs() * 2 + denominator) / (denominator * 2);
    if numerator < 0 {
        -size
    } else {
        size
    }
}

fn to_minor(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}

/// The interest of one month on `amount`: `amount * milli / 100000 / 12` for a yearly rate, `amount * milli / 100000` for a monthly one, as
/// one division rounded half away from zero. A rate of 0 gives 0.
pub fn month_interest(amount: i64, milli: i64, period: Period) -> i64 {
    let months_in_period = match period {
        Period::Monthly => 1,
        Period::Yearly => 12,
    };
    to_minor(divide_rounded(i128::from(amount) * i128::from(milli), 100_000 * months_in_period))
}

/// When a liability is paid off at its installment. The JSON is one of two shapes: `{"never":true}`, or
/// `{"months":12,"interestLeft":"123.45","month":"2027-11"}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payoff {
    /// The installment does not beat the interest of a month (the debt never falls), or it takes more than `MAX_MONTHS`.
    Never,
    Months {
        /// How many payments it takes, counting the one that clears the debt.
        months: u32,
        /// The interest that adds up over those months, in minor units.
        interest_left: i64,
        /// The month of the last payment: this month plus `months`, "YYYY-MM".
        month: String,
    },
}

impl Serialize for Payoff {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Payoff::Never => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("never", &true)?;
                map.end()
            }
            Payoff::Months { months, interest_left, month } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("months", months)?;
                map.serialize_entry("interestLeft", &format_amount(*interest_left))?;
                map.serialize_entry("month", month)?;
                map.end()
            }
        }
    }
}

/// "YYYY-MM" of the month of `today` ("YYYY-MM-DD") plus `months`.
fn month_after(today: &str, months: u32) -> Option<String> {
    let first = NaiveDate::parse_from_str(today, "%Y-%m-%d").ok()?.with_day(1)?;
    Some(first.checked_add_months(Months::new(months))?.format("%Y-%m").to_string())
}

/// How a debt of `owed` falls when `installment` is paid every month and the interest of `milli` and `period` is added every month (no rate:
/// `milli` 0). `None` when there is no installment or nothing is owed. Month by month: `i = interest(b)`; when the installment is not above `i`
/// the debt never falls; else `b = b + i - installment`, `i` goes to `interest_left` and the month is counted, until `b <= 0`. After
/// `MAX_MONTHS` it is `Never` too. Without a rate it takes `ceil(owed / installment)` months and costs no interest. `month` counts from the
/// month of `today`; a `today` that is not a date gives `None`.
pub fn payoff(owed: i64, installment: Option<i64>, milli: i64, period: Period, today: &str) -> Option<Payoff> {
    let installment = installment.filter(|i| *i > 0)?;
    if owed <= 0 {
        return None;
    }
    if milli == 0 {
        let months = (owed - 1) / installment + 1;
        return Some(match u32::try_from(months).ok().filter(|m| *m <= MAX_MONTHS) {
            Some(months) => Payoff::Months { months, interest_left: 0, month: month_after(today, months)? },
            None => Payoff::Never,
        });
    }
    let (mut balance, mut interest_left) = (owed, 0i64);
    for months in 1..=MAX_MONTHS {
        let interest = month_interest(balance, milli, period);
        if installment <= interest {
            return Some(Payoff::Never);
        }
        balance += interest - installment;
        interest_left += interest;
        if balance <= 0 {
            return Some(Payoff::Months { months, interest_left, month: month_after(today, months)? });
        }
    }
    Some(Payoff::Never)
}

/// Calendar days from date `from` to date `to` ("YYYY-MM-DD"); 0 when `to` is not after `from`. A text that is not a date counts as no term.
pub fn days(from: &str, to: &str) -> i64 {
    let date = |text: &str| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok();
    match (date(from), date(to)) {
        (Some(from), Some(to)) => to.signed_duration_since(from).num_days().max(0),
        _ => 0,
    }
}

/// What a deposit of `held` is expected to return over its whole term, `start` to `maturity`: simple interest on the days, with a year of 365
/// days: `held * milli * days / (100000 * 365)` for a yearly rate and `held * milli * days * 12 / (100000 * 365)` for a monthly one, in one
/// division rounded half away from zero.
pub fn deposit_return(held: i64, milli: i64, period: Period, start: &str, maturity: &str) -> i64 {
    let months_in_period = match period {
        Period::Monthly => 12,
        Period::Yearly => 1,
    };
    let numerator = i128::from(held) * i128::from(milli) * i128::from(days(start, maturity)) * months_in_period;
    to_minor(divide_rounded(numerator, 100_000 * 365))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-11-21";

    #[test]
    fn a_rate_is_a_percent_from_0_to_100_with_at_most_3_decimals() {
        for (text, milli) in [("0", 0), ("0.0", 0), ("0.5", 500), ("3.25", 3250), ("4.125", 4125), ("99.999", 99_999), ("100", 100_000), ("100.000", 100_000), ("12", 12_000)] {
            assert_eq!(parse_rate(text), Ok(milli), "{text:?}");
        }
        for bad in [
            "100.001", "101", "1000", "3.1415", "3.1234", "-1", "-0", "+1", "", "abc", "3.", ".5", ".", " 3", "3 ", "3,5", "03", "00.5", "1e1", "1.0a", "\u{661}", "3.2.1",
        ] {
            assert_eq!(parse_rate(bad), Err(AppError::InvalidInput), "{bad:?}");
        }
    }

    #[test]
    fn a_rate_is_written_without_trailing_zeros_and_reads_back_the_same() {
        for (milli, text) in [(0, "0"), (500, "0.5"), (3250, "3.25"), (4000, "4"), (4125, "4.125"), (100, "0.1"), (1, "0.001"), (100_000, "100")] {
            assert_eq!(format_rate(milli), text);
            assert_eq!(parse_rate(text), Ok(milli));
        }
        assert_eq!(format_rate(parse_rate("3.250").unwrap()), "3.25");
    }

    #[test]
    fn a_month_of_interest_rounds_half_away_from_zero_at_half_a_cent_for_both_periods() {
        // 12 % a year and 1 % a month are the same month: 50 minor is 0.50, whose interest is half a cent
        for (period, milli) in [(Period::Yearly, 12_000), (Period::Monthly, 1000)] {
            let got: Vec<i64> = [49, 50, 51, 150, 250, 0].iter().map(|amount| month_interest(*amount, milli, period)).collect();
            assert_eq!(got, [0, 1, 1, 2, 3, 0], "{period:?}: 0.49 rounds down, 0.50 up, 1.50 up, 2.50 up (not to even)");
            assert_eq!(month_interest(-50, milli, period), -1, "{period:?}: away from zero, whichever side");
        }
        // a yearly rate is divided by 12 after the product: 3.25 % of 1000.00 is 32.50 a year, 2.708333.. a month
        assert_eq!(month_interest(100_000, 3250, Period::Yearly), 271);
        assert_eq!(month_interest(100_000, 3250, Period::Monthly), 3250);
        assert_eq!(month_interest(999_999_999_999, 100_000, Period::Monthly), 999_999_999_999, "100 % a month: the whole amount, and no overflow");
        assert_eq!(month_interest(999_999_999_999, 100_000, Period::Yearly), 83_333_333_333);
    }

    #[test]
    fn a_rate_of_0_costs_nothing_whatever_the_amount_and_period() {
        for period in [Period::Monthly, Period::Yearly] {
            assert_eq!([0, 1, 999_999_999_999, -7].map(|a| month_interest(a, 0, period)), [0; 4]);
        }
    }

    fn months(owed: i64, installment: i64, milli: i64, period: Period) -> Option<(u32, i64, String)> {
        match payoff(owed, Some(installment), milli, period, TODAY) {
            Some(Payoff::Months { months, interest_left, month }) => Some((months, interest_left, month)),
            _ => None,
        }
    }

    #[test]
    fn an_installment_that_does_not_beat_the_first_months_interest_never_pays_the_debt_off() {
        // 1000.00 at 12 % a year: 10.00 of interest in the first month
        assert_eq!(month_interest(100_000, 12_000, Period::Yearly), 1000);
        assert_eq!(payoff(100_000, Some(1000), 12_000, Period::Yearly, TODAY), Some(Payoff::Never), "equal to the interest");
        assert_eq!(payoff(100_000, Some(999), 12_000, Period::Yearly, TODAY), Some(Payoff::Never), "below it");
        // a cent more falls by a cent in the first month, and the interest shrinks as it falls: it ends, long after, inside the 1200 months
        let (count, interest, month) = months(100_000, 1001, 12_000, Period::Yearly).expect("one cent above the interest finishes");
        assert_eq!((count, interest, month.as_str()), (703, 603_072, "2085-06"));
        assert_eq!(payoff(100_000, Some(1000), 12_000, Period::Yearly, TODAY), Some(Payoff::Never), "and the same debt at the interest itself still never does");
        assert_eq!(serde_json::to_string(&Payoff::Never).unwrap(), r#"{"never":true}"#);
    }

    #[test]
    fn a_debt_that_falls_too_slowly_is_never_paid_off_after_1200_months() {
        // 1000000.00 at 12 % a year with 10000.01 a month: 10000.00 of interest, so it falls by a cent a month at first, and a hundred years
        // are not enough (it is the cap that ends it, not the installment falling below the interest)
        assert_eq!(month_interest(100_000_000, 12_000, Period::Yearly), 1_000_000);
        assert_eq!(payoff(100_000_000, Some(1_000_001), 12_000, Period::Yearly, TODAY), Some(Payoff::Never));
    }

    #[test]
    fn a_rate_of_0_takes_the_owed_divided_by_the_installment_rounded_up_and_costs_no_interest() {
        assert_eq!(months(90_000, 30_000, 0, Period::Yearly), Some((3, 0, "2027-02".into())));
        assert_eq!(months(90_001, 30_000, 0, Period::Monthly), Some((4, 0, "2027-03".into())), "a cent over the third installment is a fourth month");
        assert_eq!(months(1, 1, 0, Period::Yearly), Some((1, 0, "2026-12".into())));
        // the month of the last payment counts from the month of today, over a year end and from its last day
        assert_eq!(payoff(30_000, Some(10_000), 0, Period::Yearly, "2026-12-31"), Some(Payoff::Months { months: 3, interest_left: 0, month: "2027-03".into() }));
        // 1200 months is a hundred years and still counts, 1201 is never (the cap holds for a rate of 0 as it does for the loop)
        assert_eq!(months(120_000, 100, 0, Period::Yearly).map(|m| m.0), Some(1200));
        assert_eq!(payoff(120_001, Some(100), 0, Period::Yearly, TODAY), Some(Payoff::Never));
    }

    #[test]
    fn a_debt_below_one_installment_is_paid_in_one_month_with_that_months_interest() {
        // 50.00 owed, 300.00 a month, 6 % a month: 3.00 of interest, then it is over
        assert_eq!(months(5000, 30_000, 6000, Period::Monthly), Some((1, 300, "2026-12".into())));
        assert_eq!(months(5000, 30_000, 6000, Period::Yearly).map(|m| m.0), Some(1));
        // exactly one installment with the interest on top is two months, not one
        assert_eq!(months(30_000, 30_000, 12_000, Period::Yearly).map(|m| m.0), Some(2));
    }

    #[test]
    fn there_is_no_payoff_without_an_installment_or_without_a_debt() {
        assert_eq!(payoff(0, Some(30_000), 3900, Period::Yearly, TODAY), None, "owed 0");
        assert_eq!(payoff(-5, Some(30_000), 3900, Period::Yearly, TODAY), None, "nothing is owed below 0");
        assert_eq!(payoff(100_000, None, 3900, Period::Yearly, TODAY), None, "no installment");
        assert_eq!(payoff(100_000, Some(0), 3900, Period::Yearly, TODAY), None, "an installment of 0 is none");
        assert_eq!(payoff(0, None, 0, Period::Yearly, TODAY), None);
    }

    #[test]
    fn a_mortgage_pays_off_with_what_the_loop_adds_up_and_the_answer_is_one_of_two_shapes() {
        // 180000.00 at 3.9 % a year with 950.00 a month, from November 2026
        let found = payoff(18_000_000, Some(95_000), 3900, Period::Yearly, TODAY).unwrap();
        assert_eq!(found, Payoff::Months { months: 295, interest_left: 10_006_549, month: "2051-06".into() });
        assert_eq!(serde_json::to_string(&found).unwrap(), r#"{"months":295,"interestLeft":"100065.49","month":"2051-06"}"#);
    }
}
