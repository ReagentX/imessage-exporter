/// The character to replace disallowed chars with
const REPLACEMENT_CHAR: char = '_';
/// Characters disallowed in a filename
const DISALLOWED_CHARS: [char; 3] = ['/', '\\', ':'];

/// Remove unsafe chars in [this list](DISALLOWED_CHARS).
pub fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|letter| {
            if DISALLOWED_CHARS.contains(&letter) {
                REPLACEMENT_CHAR
            } else {
                letter
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::app::sanitizers::sanitize_filename;

    #[test]
    fn can_sanitize_all() {
        let sanitized = String::from("a_b_c_d");

        assert_eq!(sanitize_filename("a/b\\c:d"), sanitized);
    }

    #[test]
    fn doesnt_sanitize_none() {
        let sanitized = String::from("a_b_c_d");

        assert_eq!(sanitize_filename("a_b_c_d"), sanitized);
    }

    #[test]
    fn can_sanitize_one() {
        let sanitized = String::from("ab_cd");

        assert_eq!(sanitize_filename("ab/cd"), sanitized);
    }
}
