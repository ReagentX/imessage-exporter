/*!
Contains logic for creating human-readable file size strings.
*/

const DIVISOR: f64 = 1024.;
const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

/// Get a human readable file size for an arbitrary amount of bytes
///
// # Example:
///
/// ```
/// use imessage_database::util::size::format_file_size;
///
/// let size: String = format_file_size(5612000);
/// println!("{size}"); // 5.35 MB
/// ```
#[must_use]
pub fn format_file_size(total_bytes: u64) -> String {
    let mut index: usize = 0;
    let mut bytes = total_bytes as f64;
    while index < UNITS.len() - 1 && bytes > DIVISOR {
        index += 1;
        bytes /= DIVISOR;
    }

    format!("{bytes:.2} {}", UNITS[index])
}

#[cfg(test)]
mod tests {
    use crate::util::size::format_file_size;

    #[test]
    fn can_get_file_size_bytes() {
        assert_eq!(format_file_size(100), String::from("100.00 B"));
    }

    #[test]
    fn can_get_file_size_kb() {
        let expected = format_file_size(2300);
        assert_eq!(expected, String::from("2.25 KB"));
    }

    #[test]
    fn can_get_file_size_mb() {
        let expected = format_file_size(5612000);
        assert_eq!(expected, String::from("5.35 MB"));
    }

    #[test]
    fn can_get_file_size_gb() {
        let expected = format_file_size(9234712394);
        assert_eq!(expected, String::from("8.60 GB"));
    }

    #[test]
    fn can_get_file_size_cap() {
        let expected = format_file_size(u64::MAX);
        assert_eq!(expected, String::from("16777216.00 TB"));
    }
}
