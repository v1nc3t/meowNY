//! Calendar months in the user's timezone. "This month" is never computed in UTC by accident:
//! someone in Dublin at 00:30 on 1 October is already in October while UTC says 30 September.

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

use crate::vault::AppError;

pub fn is_valid_timezone(name: &str) -> bool {
    name.parse::<Tz>().is_ok()
}

/// First day of the month that `now` falls in, as seen in timezone `tz`.
pub fn month_start(now: DateTime<Utc>, tz: &str) -> Result<NaiveDate, AppError> {
    let tz: Tz = tz.parse().map_err(|_| AppError::Internal)?;
    let local = tz.from_utc_datetime(&now.naive_utc()).date_naive();
    local.with_day(1).ok_or(AppError::Internal)
}

/// Today's date as seen in timezone `tz`.
pub fn today(now: DateTime<Utc>, tz: &str) -> Result<NaiveDate, AppError> {
    let tz: Tz = tz.parse().map_err(|_| AppError::Internal)?;
    Ok(tz.from_utc_datetime(&now.naive_utc()).date_naive())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    fn start(now: &str, tz: &str) -> String {
        month_start(at(now), tz).unwrap().to_string()
    }

    #[test]
    fn midday_is_the_same_month_everywhere() {
        assert_eq!(start("2026-06-15T12:00:00Z", "UTC"), "2026-06-01");
        assert_eq!(start("2026-06-15T12:00:00Z", "Europe/Dublin"), "2026-06-01");
    }

    #[test]
    fn east_of_utc_enters_the_new_month_first() {
        // 23:30 UTC on 30 September is 00:30 on 1 October in Dublin (UTC+1 in summer time).
        assert_eq!(start("2026-09-30T23:30:00Z", "UTC"), "2026-09-01");
        assert_eq!(start("2026-09-30T23:30:00Z", "Europe/Dublin"), "2026-10-01");
        // One minute earlier it is still September in Dublin.
        assert_eq!(start("2026-09-30T22:59:00Z", "Europe/Dublin"), "2026-09-01");
    }

    #[test]
    fn west_of_utc_leaves_the_month_last() {
        // 02:00 UTC on 1 October is 22:00 on 30 September in New York.
        assert_eq!(start("2026-10-01T02:00:00Z", "UTC"), "2026-10-01");
        assert_eq!(start("2026-10-01T02:00:00Z", "America/New_York"), "2026-09-01");
    }

    #[test]
    fn year_end_rolls_the_year() {
        assert_eq!(start("2026-12-31T23:30:00Z", "Pacific/Auckland"), "2027-01-01");
        assert_eq!(start("2027-01-01T03:00:00Z", "America/Los_Angeles"), "2026-12-01");
    }

    #[test]
    fn today_follows_the_timezone() {
        let now = at("2026-09-30T23:30:00Z");
        assert_eq!(today(now, "UTC").unwrap().to_string(), "2026-09-30");
        assert_eq!(today(now, "Europe/Dublin").unwrap().to_string(), "2026-10-01");
    }

    #[test]
    fn timezone_names_are_checked() {
        assert!(is_valid_timezone("Europe/Dublin"));
        assert!(is_valid_timezone("UTC"));
        assert!(!is_valid_timezone("Mars/Olympus"));
        assert!(!is_valid_timezone(""));
        assert_eq!(month_start(at("2026-06-15T12:00:00Z"), "Mars/Olympus"), Err(AppError::Internal));
    }
}
