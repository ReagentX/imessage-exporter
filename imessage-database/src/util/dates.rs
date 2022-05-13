use chrono::{DateTime, Duration, Local};

const SEPARATOR: &str = ", ";

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
pub fn readable_diff(start: DateTime<Local>, end: DateTime<Local>) -> Option<String> {
    let mut out_s = String::new();
    let diff: Duration = end - start;
    let seconds = diff.num_seconds();

    if seconds < 0 {
        return None;
    }

    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 86400 % 3600) / 60;
    let secs = seconds % 86400 % 3600 % 60;

    if days != 0 {
        out_s.push_str(&format!("{} days", days));
    }
    if hours != 0 {
        if !out_s.is_empty() {
            out_s.push_str(SEPARATOR);
        }
        out_s.push_str(&format!("{} hours", hours));
    }
    if minutes != 0 {
        if !out_s.is_empty() {
            out_s.push_str(SEPARATOR);
        }
        out_s.push_str(&format!("{} minutes", minutes));
    }
    if secs != 0 {
        if !out_s.is_empty() {
            out_s.push_str(SEPARATOR);
        }
        out_s.push_str(&format!("{} seconds", secs));
    }
    Some(out_s)
}

#[cfg(test)]
mod tests {
    use super::{format, readable_diff};
    use chrono::prelude::*;

    #[test]
    fn can_format_date() {
        let date = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        assert_eq!(format(&date), "May 20, 2020  9:10:11 AM")
    }

    #[test]
    fn cant_format_diff_backwards() {
        let end = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 11, 12);
        let start = Local.ymd(2020, 5, 20).and_hms_milli(9, 10, 30, 12);
        assert_eq!(readable_diff(start, end), Some("".to_owned()))
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
