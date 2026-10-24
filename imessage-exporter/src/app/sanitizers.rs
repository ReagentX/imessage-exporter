/*!
 Defines routines for sanitizing text data.
*/

use std::borrow::Cow;

use askama::filters::Escaper;

use crate::app::escaping::ChatEscaper;

/// The character to replace disallowed chars with
const FILENAME_REPLACEMENT_CHAR: char = '_';

/// Windows reserves these device names — case-insensitive, regardless of
/// extension.
///
/// Detail at <https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file>
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON",
    "PRN",
    "AUX",
    "NUL",
    "COM0",
    "COM1",
    "COM2",
    "COM3",
    "COM4",
    "COM5",
    "COM6",
    "COM7",
    "COM8",
    "COM9",
    "COM\u{B9}",
    "COM\u{B2}",
    "COM\u{B3}",
    "LPT0",
    "LPT1",
    "LPT2",
    "LPT3",
    "LPT4",
    "LPT5",
    "LPT6",
    "LPT7",
    "LPT8",
    "LPT9",
    "LPT\u{B9}",
    "LPT\u{B2}",
    "LPT\u{B3}",
];

/// Returns true if a character is disallowed in filenames
#[inline]
fn is_filename_disallowed(c: char) -> bool {
    matches!(c, '*' | '"' | '/' | '\\' | '<' | '>' | ':' | '|' | '?')
}

/// Returns true if the basename (up to the first `.`) matches a Windows
/// reserved device name.
fn is_windows_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    // ASCII case-fold is sufficient: the only non-ASCII chars in the
    // reserved set (¹²³) appear identically on both sides.
    WINDOWS_RESERVED_NAMES
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

/// Remove unsafe chars in filenames, strip trailing `.` and ASCII space,
/// and escape Windows device names.
///
/// Returns `String` rather than `Cow<str>` because callers use the result
/// both as an on-disk filename and as a `HashMap` key.
pub fn sanitize_filename(filename: &str) -> String {
    let mut sanitized: String = filename
        .chars()
        .map(|letter| {
            if letter.is_control() || is_filename_disallowed(letter) {
                FILENAME_REPLACEMENT_CHAR
            } else {
                letter
            }
        })
        .collect();

    // Windows silently strips trailing `.` and ` `; do it up front so platforms
    // agree on the final name and e.g. "foo" / "foo." can't collide.
    let trimmed_len = sanitized.trim_end_matches(['.', ' ']).len();
    sanitized.truncate(trimmed_len);

    if is_windows_reserved(&sanitized) {
        // Prepend rather than replace so the original name stays readable.
        let mut out = String::with_capacity(sanitized.len() + 1);
        out.push(FILENAME_REPLACEMENT_CHAR);
        out.push_str(&sanitized);
        out
    } else {
        sanitized
    }
}

/// Escapes HTML special characters in the input string, allocating only if
/// at least one character needs escaping. Wraps [`ChatEscaper`] so the
/// character set and replacements stay aligned with Askama-rendered output.
pub fn sanitize_html(input: &'_ str) -> Cow<'_, str> {
    // Fast scan: every escape target is either a single ASCII byte or the
    // two-byte NBSP (`0xC2 0xA0`). None of these match a UTF-8 continuation
    // byte, so scanning bytes (rather than chars) is safe.
    let bytes = input.as_bytes();
    let needs_escape = bytes.iter().enumerate().any(|(i, &b)| {
        matches!(b, b'<' | b'>' | b'"' | b'\'' | b'`' | b'&')
            || (b == 0xC2 && bytes.get(i + 1) == Some(&0xA0))
    });
    if !needs_escape {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len() + 8);
    ChatEscaper
        .write_escaped_str(&mut out, input)
        .unwrap_or_default();
    Cow::Owned(out)
}

#[cfg(test)]
mod filename_sanitization_tests {
    use crate::app::sanitizers::sanitize_filename;

    #[test]
    fn can_sanitize_macos() {
        assert_eq!(sanitize_filename("a/b\\c:d"), "a_b_c_d");
    }

    #[test]
    fn sanitizes_by_character_class() {
        let cases = [
            // Control characters replaced
            ("tab", "tab\there", "tab_here"),
            ("newline", "new\nline", "new_line"),
            ("carriage return", "return\r", "return_"),
            ("ascii controls", "ascii\x01\x1F", "ascii__"),
            // Non-ASCII passed through
            ("chinese", "你好/世界", "你好_世界"),
            ("accented letters", "café/niño", "café_niño"),
            ("cyrillic", "привет/мир", "привет_мир"),
            // Trailing dots and spaces stripped
            ("trailing space", "trailing space ", "trailing space"),
            ("trailing dot", "trailing dot.", "trailing dot"),
            ("mixed trailing dots and spaces", "mixed. . .", "mixed"),
            ("multiple trailing dots", "dots...", "dots"),
            ("multiple trailing spaces", "spaces   ", "spaces"),
            // Allowed characters untouched
            ("underscores", "a_b_c_d", "a_b_c_d"),
            ("dot and hyphen", "file.name-version", "file.name-version"),
            (
                "internal dots and spaces",
                "file.name with spaces.txt",
                "file.name with spaces.txt",
            ),
        ];

        for (label, input, expected) in cases {
            assert_eq!(sanitize_filename(input), expected, "{label}");
        }
    }

    #[test]
    fn can_sanitize_only_bad() {
        assert_eq!(
            sanitize_filename("* \" / \\ < > : | ?"),
            "_ _ _ _ _ _ _ _ _"
        );
    }

    #[test]
    fn handles_emoji() {
        assert_eq!(sanitize_filename("hello🌍world"), "hello🌍world");
    }

    #[test]
    fn handles_leading_space() {
        assert_eq!(sanitize_filename(" leading space"), " leading space");
    }

    #[test]
    fn collapses_to_empty_when_only_trailing_chars() {
        assert_eq!(sanitize_filename(". . ."), "");
        assert_eq!(sanitize_filename("   "), "");
    }

    #[test]
    fn handles_empty_string() {
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn replaces_del_control_char() {
        assert_eq!(sanitize_filename("\x7F"), "_");
    }

    #[test]
    fn handles_mixed_control_and_disallowed() {
        assert_eq!(sanitize_filename("*\t?\r"), "____");
    }

    #[test]
    fn prefixes_reserved_name_con() {
        assert_eq!(sanitize_filename("CON"), "_CON");
    }

    #[test]
    fn prefixes_reserved_name_case_insensitive() {
        assert_eq!(sanitize_filename("con"), "_con");
        assert_eq!(sanitize_filename("NuL"), "_NuL");
        assert_eq!(sanitize_filename("Aux"), "_Aux");
    }

    #[test]
    fn prefixes_reserved_name_with_extension() {
        assert_eq!(sanitize_filename("CON.html"), "_CON.html");
        assert_eq!(sanitize_filename("nul.txt"), "_nul.txt");
        assert_eq!(sanitize_filename("PRN.tar.gz"), "_PRN.tar.gz");
    }

    #[test]
    fn prefixes_all_com_serial_names() {
        for i in 0..=9 {
            let actual = sanitize_filename(&format!("COM{i}"));
            let expected = format!("_COM{i}");
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn prefixes_com_superscript_variants() {
        assert_eq!(sanitize_filename("COM\u{B9}"), "_COM\u{B9}");
        assert_eq!(sanitize_filename("COM\u{B2}"), "_COM\u{B2}");
        assert_eq!(sanitize_filename("COM\u{B3}"), "_COM\u{B3}");
    }

    #[test]
    fn prefixes_all_lpt_parallel_names() {
        for i in 0..=9 {
            let actual = sanitize_filename(&format!("LPT{i}"));
            let expected = format!("_LPT{i}");
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn prefixes_lpt_superscript_variants() {
        assert_eq!(sanitize_filename("LPT\u{B9}"), "_LPT\u{B9}");
        assert_eq!(sanitize_filename("LPT\u{B2}"), "_LPT\u{B2}");
        assert_eq!(sanitize_filename("LPT\u{B3}"), "_LPT\u{B3}");
    }

    #[test]
    fn leaves_reserved_prefix_alone() {
        // The match is on the exact stem, not a prefix.
        assert_eq!(sanitize_filename("CONversation.html"), "CONversation.html");
        assert_eq!(sanitize_filename("NULL.txt"), "NULL.txt");
        assert_eq!(sanitize_filename("LPT10"), "LPT10");
        assert_eq!(sanitize_filename("COM"), "COM");
    }

    #[test]
    fn reserved_check_runs_on_sanitized_chars() {
        // After char sanitization "CON*" becomes "CON_", which is no longer
        // a reserved name and so is not prefixed.
        assert_eq!(sanitize_filename("CON*"), "CON_");
        assert_eq!(sanitize_filename("CON\x00"), "CON_");
    }

    #[test]
    fn reserved_check_runs_after_trim() {
        // Trailing strip happens before the reserved-name lookup, so a name
        // that Windows would silently rewrite to a reserved stem is caught.
        assert_eq!(sanitize_filename("CON."), "_CON");
        assert_eq!(sanitize_filename("nul "), "_nul");
        assert_eq!(sanitize_filename("PRN. "), "_PRN");
    }
}

#[cfg(test)]
mod html_sanitization_tests {
    use std::borrow::Cow;

    use crate::app::sanitizers::sanitize_html;

    // Character-set / replacement behavior is covered by
    // `app::escaping::tests`. These tests cover only what `sanitize_html`
    // itself adds: the `Cow` fast path and the integration with
    // `ChatEscaper`.

    #[test]
    fn empty_input_borrows() {
        assert!(matches!(sanitize_html(""), Cow::Borrowed(_)));
    }

    #[test]
    fn no_escape_targets_borrows() {
        let input = "Hello world, привет, 🌍";
        match sanitize_html(input) {
            Cow::Borrowed(s) => assert_eq!(s.as_ptr(), input.as_ptr()),
            Cow::Owned(_) => panic!("expected Borrowed for input with no escape targets"),
        }
    }

    #[test]
    fn ascii_escape_target_allocates() {
        assert!(matches!(sanitize_html("<"), Cow::Owned(_)));
    }

    #[test]
    fn nbsp_escape_target_allocates() {
        assert!(matches!(sanitize_html("\u{a0}"), Cow::Owned(_)));
    }

    #[test]
    fn matches_chat_escaper_on_mixed_content() {
        // One end-to-end smoke test through the wrapper. The full character
        // set is covered by `ChatEscaper`'s tests.
        assert_eq!(
            &sanitize_html("<div>Hello\u{a0}&amp;\u{a0}world</div>"),
            "&lt;div&gt;Hello&nbsp;&amp;amp;&nbsp;world&lt;/div&gt;"
        );
    }
}

/// Permit ordinary links, never executable or local-file URL schemes.
pub(crate) fn safe_link(value: &str) -> Option<&str> {
    if value.is_empty()
        || value.chars().any(|c| c.is_control())
        || value.trim() != value
        || value.starts_with(['/', '\\'])
        || value.contains('\\')
        || value.starts_with(".")
    {
        return None;
    }
    if let Some((scheme, _)) = value.split_once(':')
        && ![
            "https",
            "http",
            "mailto",
            "tel",
            "sms",
            "facetime",
            "facetime-audio",
        ]
        .iter()
        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
    {
        return None;
    }
    Some(value)
}
