use chrono::{DateTime, Local};

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
