use chrono::{DateTime, Local};

pub fn format(date: &DateTime<Local>) -> String {
    DateTime::format(date, "%b %d, %Y %l:%M:%S %p").to_string()
}
