/*!
 Contains date parsing functions for iMessage dates.

 Most dates are stored as nanosecond-precision unix timestamps with an epoch of `1/1/2001 00:00:00` in the local time zone.
*/

use chrono::{DateTime, Duration, Local, TimeZone, Utc};

use crate::error::message::MessageError;

const SEPARATOR: &str = ", ";

/// Factor used to convert between nanosecond-precision timestamps and seconds
///
/// The iMessage database stores timestamps as nanoseconds, so this factor is used
/// to convert between the database format and standard Unix timestamps.
pub const TIMESTAMP_FACTOR: i64 = 1000000000;

/// Get the date offset for the iMessage Database
///
/// This offset is used to adjust the unix timestamps stored in the iMessage database
/// with a non-standard epoch of `2001-01-01 00:00:00` in the current machine's local time zone.
///
/// # Example
///
/// ```
/// use imessage_database::util::dates::get_offset;
///
/// let current_epoch = get_offset();
/// ```
#[must_use]
pub fn get_offset() -> i64 {
    Utc.with_ymd_and_hms(2001, 1, 1, 0, 0, 0)
        .unwrap()
        .timestamp()
}

/// Create a `DateTime<Local>` from an arbitrary date and offset
///
/// This is used to create date data for anywhere dates are stored in the table, including
/// `PLIST` payloads or [`typedstream`](crate::util::typedstream) data.
///
/// # Example
///
/// ```
/// use imessage_database::util::dates::{get_local_time, get_offset};
///
/// let current_offset = get_offset();
/// let local = get_local_time(&674526582885055488, &current_offset).unwrap();
/// ```
pub fn get_local_time(date_stamp: &i64, offset: &i64) -> Result<DateTime<Local>, MessageError> {
    let utc_stamp = DateTime::from_timestamp((date_stamp / TIMESTAMP_FACTOR) + offset, 0)
        .ok_or(MessageError::InvalidTimestamp(*date_stamp))?
        .naive_utc();
    Ok(Local.from_utc_datetime(&utc_stamp))
}

/// Format a date from the iMessage table for reading
///
/// # Example:
///
/// ```
/// use chrono::offset::Local;
/// use imessage_database::util::dates::format;
///
/// let date = format(&Ok(Local::now()));
/// println!("{date}");
/// ```
#[must_use]
pub fn format(date: &Result<DateTime<Local>, MessageError>) -> String {
    match date {
        Ok(d) => DateTime::format(d, "%b %d, %Y %l:%M:%S %p").to_string(),
        Err(why) => why.to_string(),
    }
}

/// Generate a readable diff from two local timestamps.
///
/// # Example:
///
/// ```
/// use chrono::prelude::*;
/// use imessage_database::util::dates::readable_diff;
///
/// let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
/// let end = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 15, 13).unwrap());
/// println!("{}", readable_diff(start, end).unwrap()) // "5 minutes, 2 seconds"
/// ```
#[must_use]
pub fn readable_diff(
    start: Result<DateTime<Local>, MessageError>,
    end: Result<DateTime<Local>, MessageError>,
) -> Option<String> {
    // Calculate diff
    let diff: Duration = end.ok()? - start.ok()?;
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
    use crate::{
        error::message::MessageError,
        util::dates::{format, readable_diff},
    };
    use chrono::prelude::*;

    #[test]
    fn can_format_date_single_digit() {
        let date = Local
            .with_ymd_and_hms(2020, 5, 20, 9, 10, 11)
            .single()
            .ok_or(MessageError::InvalidTimestamp(0));
        assert_eq!(format(&date), "May 20, 2020  9:10:11 AM");
    }

    #[test]
    fn can_format_date_double_digit() {
        let date = Local
            .with_ymd_and_hms(2020, 5, 20, 10, 10, 11)
            .single()
            .ok_or(MessageError::InvalidTimestamp(0));
        assert_eq!(format(&date), "May 20, 2020 10:10:11 AM");
    }

    #[test]
    fn cant_format_diff_backwards() {
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 30).unwrap());
        assert_eq!(readable_diff(start, end), None);
    }

    #[test]
    fn can_format_diff_all_singular() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 21, 10, 11, 12).unwrap());
        assert_eq!(
            readable_diff(start, end),
            Some("1 day, 1 hour, 1 minute, 1 second".to_owned())
        );
    }

    #[test]
    fn can_format_diff_mixed_singular() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 22, 10, 20, 12).unwrap());
        assert_eq!(
            readable_diff(start, end),
            Some("2 days, 1 hour, 10 minutes, 1 second".to_owned())
        );
    }

    #[test]
    fn can_format_diff_seconds() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 30).unwrap());
        assert_eq!(readable_diff(start, end), Some("19 seconds".to_owned()));
    }

    #[test]
    fn can_format_diff_minutes() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 15, 11).unwrap());
        assert_eq!(readable_diff(start, end), Some("5 minutes".to_owned()));
    }

    #[test]
    fn can_format_diff_hours() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 20, 12, 10, 11).unwrap());
        assert_eq!(readable_diff(start, end), Some("3 hours".to_owned()));
    }

    #[test]
    fn can_format_diff_days() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 30, 9, 10, 11).unwrap());
        assert_eq!(readable_diff(start, end), Some("10 days".to_owned()));
    }

    #[test]
    fn can_format_diff_minutes_seconds() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 15, 30).unwrap());
        assert_eq!(
            readable_diff(start, end),
            Some("5 minutes, 19 seconds".to_owned())
        );
    }

    #[test]
    fn can_format_diff_days_minutes() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 22, 9, 30, 11).unwrap());
        assert_eq!(
            readable_diff(start, end),
            Some("2 days, 20 minutes".to_owned())
        );
    }

    #[test]
    fn can_format_diff_month() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 7, 20, 9, 10, 11).unwrap());
        assert_eq!(readable_diff(start, end), Some("61 days".to_owned()));
    }

    #[test]
    fn can_format_diff_year() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2022, 7, 20, 9, 10, 11).unwrap());
        assert_eq!(readable_diff(start, end), Some("791 days".to_owned()));
    }

    #[test]
    fn can_format_diff_all() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 22, 14, 32, 45).unwrap());
        assert_eq!(
            readable_diff(start, end),
            Some("2 days, 5 hours, 22 minutes, 34 seconds".to_owned())
        );
    }

    #[test]
    fn can_format_no_diff() {
        let start = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        let end = Ok(Local.with_ymd_and_hms(2020, 5, 20, 9, 10, 11).unwrap());
        assert_eq!(readable_diff(start, end), Some(String::new()));
    }
}
