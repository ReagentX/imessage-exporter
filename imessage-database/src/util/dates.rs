/*!
 Contains date parsing functions for iMessage dates.

 Dates are stored as nanosecond-precision unix timestamps with an epoch of `1/1/2001 00:00:00` in the local time zone.
*/

use chrono::{DateTime, Duration, Local, TimeZone, Utc};

const SEPARATOR: &str = ", ";

/// Get the date offset for the iMessage Database
///
pub fn get_offset() -> i64 {
    Utc.ymd(2001, 1, 1).and_hms(0, 0, 0).timestamp()
}

/// Format a date from the iMessage table for reading
///
/// # Example:
///
/// ```
/// use chrono::offset::Local;
/// use imessage_database::util::dates::format;
///
/// let date = format(&Local::now());
/// println!("{date}");
/// ```
pub fn format(date: &DateTime<Local>) -> String {
    DateTime::format(date, "%b %d, %Y %l:%M:%S %p").to_string()
}

/// Generate a readable diff from two local timestamps
///
/// # Example:
///
/// ```
/// use chrono::prelude::*;
/// use imessage_database::util::dates::readable_diff;
///
/// let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
/// let end = Local.ymd(2020, 5, 20).and_hms_milli(9, 15, 11, 12);
/// println!("{}", readable_diff(start, end).unwrap())
/// ```
pub fn readable_diff(start: DateTime<Local>, end: DateTime<Local>) -> Option<String> {
    // Calculate diff
    let diff: Duration = end - start;
    let seconds = diff.num_seconds();

    // Early escape for invalid date diff
    if seconds < 0 {
        return None;
    }

    // 42 is the length of a diff string that has all components with 2 digits each
    // This allocation improved performance over `::new()` by 20%
    // (21.99s to 27.79s over 250k messages)
    let mut out_s = String::with_capacity(42);

    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 86400 % 3600) / 60;
    let secs = seconds % 86400 % 3600 % 60;

    if days != 0 {
        let metric = match days {
            1 => "day",
            _ => "days",
        };
        out_s.push_str(&format!("{days} {metric}"));
    }
    if hours != 0 {
        let metric = match hours {
            1 => "hour",
            _ => "hours",
        };
        if !out_s.is_empty() {
            out_s.push_str(SEPARATOR);
        }
        out_s.push_str(&format!("{hours} {metric}"));
    }
    if minutes != 0 {
        let metric = match minutes {
            1 => "minute",
            _ => "minutes",
        };
        if !out_s.is_empty() {
            out_s.push_str(SEPARATOR);
        }
        out_s.push_str(&format!("{minutes} {metric}"));
    }
    if secs != 0 {
        let metric = match secs {
            1 => "second",
            _ => "seconds",
        };
        if !out_s.is_empty() {
            out_s.push_str(SEPARATOR);
        }
        out_s.push_str(&format!("{secs} {metric}"));
    }
    Some(out_s)
}

#[cfg(test)]
mod tests {
    use super::{format, readable_diff};
    use chrono::prelude::*;

    #[test]
    fn can_format_date_single_digit() {
        let date = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        assert_eq!(format(&date), "May 20, 2020  9:10:11 AM")
    }

    #[test]
    fn can_format_date_double_digit() {
        let date = Local.ymd(2020, 5, 20).and_hms_milli(10, 10, 11, 12);
        assert_eq!(format(&date), "May 20, 2020 10:10:11 AM")
    }

    #[test]
    fn cant_format_diff_backwards() {
        let end = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 30, 12);
        assert_eq!(readable_diff(start, end), None)
    }

    #[test]
    fn can_format_diff_all_signular() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2020, 5, 21).and_hms_milli(10, 11, 12, 12);
        assert_eq!(
            readable_diff(start, end),
            Some("1 day, 1 hour, 1 minute, 1 second".to_owned())
        )
    }

    #[test]
    fn can_format_diff_mixed_signular() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2020, 5, 22).and_hms_milli(10, 20, 12, 12);
        assert_eq!(
            readable_diff(start, end),
            Some("2 days, 1 hour, 10 minutes, 1 second".to_owned())
        )
    }

    #[test]
    fn can_format_diff_seconds() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 30, 12);
        assert_eq!(readable_diff(start, end), Some("19 seconds".to_owned()))
    }

    #[test]
    fn can_format_diff_minutes() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2020, 5, 20).and_hms_milli(9, 15, 11, 12);
        assert_eq!(readable_diff(start, end), Some("5 minutes".to_owned()))
    }

    #[test]
    fn can_format_diff_hours() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2020, 5, 20).and_hms_milli(12, 10, 11, 12);
        assert_eq!(readable_diff(start, end), Some("3 hours".to_owned()))
    }

    #[test]
    fn can_format_diff_days() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2020, 5, 30).and_hms_milli(9, 10, 11, 12);
        assert_eq!(readable_diff(start, end), Some("10 days".to_owned()))
    }

    #[test]
    fn can_format_diff_minutes_seconds() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2020, 5, 20).and_hms_milli(9, 15, 30, 12);
        assert_eq!(
            readable_diff(start, end),
            Some("5 minutes, 19 seconds".to_owned())
        )
    }

    #[test]
    fn can_format_diff_days_minutes() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2020, 5, 22).and_hms_milli(9, 30, 11, 12);
        assert_eq!(
            readable_diff(start, end),
            Some("2 days, 20 minutes".to_owned())
        )
    }

    #[test]
    fn can_format_diff_month() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2020, 7, 20).and_hms_milli(9, 10, 11, 12);
        assert_eq!(readable_diff(start, end), Some("61 days".to_owned()))
    }

    #[test]
    fn can_format_diff_year() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2022, 7, 20).and_hms_milli(9, 10, 11, 12);
        assert_eq!(readable_diff(start, end), Some("791 days".to_owned()))
    }

    #[test]
    fn can_format_diff_all() {
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let end = Local.ymd(2020, 5, 22).and_hms_milli(14, 32, 45, 234);
        assert_eq!(
            readable_diff(start, end),
            Some("2 days, 5 hours, 22 minutes, 34 seconds".to_owned())
        )
    }
}
