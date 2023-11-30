use std::borrow::Cow;

/// The character to replace disallowed chars with
const FILENAME_REPLACEMENT_CHAR: char = '_';
/// Characters disallowed in a filename
const FILENAME_DISALLOWED_CHARS: [char; 3] = ['/', '\\', ':'];

/// Remove unsafe chars in [this list](FILENAME_DISALLOWED_CHARS).
pub fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|letter| {
            if FILENAME_DISALLOWED_CHARS.contains(&letter) {
                FILENAME_REPLACEMENT_CHAR
            } else {
                letter
            }
        })
        .collect()
}

/// Escapes HTML special characters in the input string.
pub fn sanitize_html(input: &str) -> Cow<str> {
    for (idx, char) in input.char_indices() {
        if matches!(char, '<' | '>' | '"' | '’' | '&') {
            let mut res = String::from(&input[..idx]);
            input[idx..].chars().for_each(|c| match c {
                '<' => res.push_str("&lt;"),
                '>' => res.push_str("&gt;"),
                '"' => res.push_str("&quot;"),
                '’' => res.push_str("&#39;"),
                '&' => res.push_str("&amp;"),
                _ => res.push(c),
            });
            return Cow::Owned(res);
        }
    }
    Cow::Borrowed(input)
}

#[cfg(test)]
mod test_filename {
    use crate::app::sanitizers::sanitize_filename;

    #[test]
    fn can_sanitize_all() {
        assert_eq!(sanitize_filename("a/b\\c:d"), "a_b_c_d");
    }

    #[test]
    fn doesnt_sanitize_none() {
        assert_eq!(sanitize_filename("a_b_c_d"), "a_b_c_d");
    }

    #[test]
    fn can_sanitize_one() {
        assert_eq!(sanitize_filename("ab/cd"), "ab_cd");
    }
}

#[cfg(test)]
mod tests {
    use crate::app::sanitizers::sanitize_html;

    #[test]
    fn test_escape_html_chars_basic() {
        assert_eq!(
            &sanitize_html("<p>Hello, world > HTML</p>"),
            "&lt;p&gt;Hello, world &gt; HTML&lt;/p&gt;"
        );
    }

    #[test]
    fn doesnt_sanitize_empty_string() {
        assert_eq!(&sanitize_html(""), "");
    }

    #[test]
    fn doesnt_sanitize_no_special_chars() {
        assert_eq!(&sanitize_html("Hello world"), "Hello world");
    }

    #[test]
    fn can_sanitize_all_special_chars() {
        assert_eq!(&sanitize_html("<>&\"’"), "&lt;&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn can_sanitize_mixed_content() {
        assert_eq!(
            &sanitize_html("<div>Hello &amp; world</div>"),
            "&lt;div&gt;Hello &amp;amp; world&lt;/div&gt;"
        );
    }
}
