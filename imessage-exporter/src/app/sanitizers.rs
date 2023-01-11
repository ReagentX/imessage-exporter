/// The character to replace disallowed chars with
const REPLACEMENT_CHAR: char = '_';
/// Characters disallowed in a filename
const DISALLOWED_CHARS: [char; 3] = ['/', '\\', ':'];

/// Remove unsafe chars in [this list](app::sanitizers::DISALLOWED_CHARS).
pub fn sanitize_filename(filename: String) -> String {
    filename.chars().into_iter().map(|letter| match DISALLOWED_CHARS.contains(&letter){
        true => REPLACEMENT_CHAR,
        false => letter,
    }).collect()
}

#[cfg(test)]
mod tests {
    use crate::app::sanitizers::sanitize_filename;

    #[test]
    fn can_sanitize_all() {
        let contaminated = String::from("a/b\\c:d");
        let sanitized = String::from("a_b_c_d");

        assert_eq!(sanitize_filename(contaminated), sanitized);
    }

    #[test]
    fn doesnt_sanitize_none() {
        let contaminated = String::from("a_b_c_d");
        let sanitized = String::from("a_b_c_d");

        assert_eq!(sanitize_filename(contaminated), sanitized);
    }

    #[test]
    fn can_sanitize_one() {
        let contaminated = String::from("ab/cd");
        let sanitized = String::from("ab_cd");

        assert_eq!(sanitize_filename(contaminated), sanitized);
    }
}
