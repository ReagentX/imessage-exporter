/*!
 HTML escaper used by both Askama-rendered `.html` templates (see
 `askama.toml`) and [`sanitize_html`](super::sanitizers::sanitize_html).
 Lives in the `app` layer so both consumers can share it without the
 sanitizer reaching into the HTML exporter.
*/

use std::fmt;

use askama::filters::Escaper;

/// Askama escaper for `.html` templates. Escapes the six named entities plus
/// a non-breaking-space replacement.
///
/// Bulk-writes runs of safe bytes between escape positions so a clean string
/// costs one `write_str`, matching the pattern in [`askama::filters::Html`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ChatEscaper;

impl Escaper for ChatEscaper {
    fn write_escaped_str<W: fmt::Write>(&self, mut dest: W, string: &str) -> fmt::Result {
        // Every escape target is either ASCII (one byte) or NBSP
        // (`0xC2 0xA0`). Slices into `string` only happen at escape positions,
        // which are always valid UTF-8 boundaries because none of the bytes we
        // match can appear as a continuation byte (those live in 0x80..=0xBF).
        let bytes = string.as_bytes();
        let mut last = 0;
        let mut i = 0;
        while i < bytes.len() {
            let (escape, width) = match bytes[i] {
                b'>' => ("&gt;", 1),
                b'<' => ("&lt;", 1),
                b'"' => ("&quot;", 1),
                b'\'' => ("&apos;", 1),
                b'`' => ("&grave;", 1),
                b'&' => ("&amp;", 1),
                0xC2 if bytes.get(i + 1) == Some(&0xA0) => ("&nbsp;", 2),
                _ => {
                    i += 1;
                    continue;
                }
            };
            if last < i {
                dest.write_str(&string[last..i])?;
            }
            dest.write_str(escape)?;
            i += width;
            last = i;
        }
        if last < bytes.len() {
            dest.write_str(&string[last..])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use askama::filters::Escaper;

    use super::ChatEscaper;

    fn escape(s: &str) -> String {
        let mut out = String::new();
        ChatEscaper.write_escaped_str(&mut out, s).unwrap();
        out
    }

    #[test]
    fn empty_input_writes_nothing() {
        assert_eq!(escape(""), "");
    }

    #[test]
    fn no_escape_targets_passes_through() {
        assert_eq!(escape("Hello world"), "Hello world");
    }

    #[test]
    fn escapes_each_ascii_target() {
        let cases = [
            ("gt", ">", "&gt;"),
            ("lt", "<", "&lt;"),
            ("double quote", "\"", "&quot;"),
            ("apostrophe", "'", "&apos;"),
            ("grave", "`", "&grave;"),
            ("ampersand", "&", "&amp;"),
        ];

        for (label, input, expected) in cases {
            assert_eq!(escape(input), expected, "{label}");
        }
    }

    #[test]
    fn escapes_nbsp_only() {
        assert_eq!(escape("\u{a0}"), "&nbsp;");
    }

    #[test]
    fn escapes_consecutive_nbsps() {
        assert_eq!(escape("\u{a0}\u{a0}"), "&nbsp;&nbsp;");
    }

    #[test]
    fn escapes_nbsp_between_ascii_targets() {
        assert_eq!(escape("<\u{a0}>"), "&lt;&nbsp;&gt;");
    }

    #[test]
    fn escapes_all_ascii_targets_together() {
        assert_eq!(escape("<>&\"`'"), "&lt;&gt;&amp;&quot;&grave;&apos;");
    }

    #[test]
    fn preserves_safe_runs_around_escapes() {
        assert_eq!(escape("foo <bar> baz"), "foo &lt;bar&gt; baz");
    }

    #[test]
    fn preserves_safe_runs_with_nbsp() {
        assert_eq!(escape("hello\u{a0}world"), "hello&nbsp;world");
    }

    #[test]
    fn handles_emoji_unchanged() {
        assert_eq!(escape("Hello 🌍 <world>"), "Hello 🌍 &lt;world&gt;");
    }

    #[test]
    fn handles_cyrillic_unchanged() {
        assert_eq!(escape("привет <b>"), "привет &lt;b&gt;");
    }

    /// `0xC2` only triggers NBSP escaping when followed by `0xA0`. Other
    /// `0xC2`-prefixed UTF-8 sequences (e.g. U+00A1 = `0xC2 0xA1`) must pass
    /// through, because the byte scanner skips over them via the `_` arm.
    #[test]
    fn does_not_escape_other_c2_prefixed_chars() {
        assert_eq!(escape("\u{a1}"), "\u{a1}");
        assert_eq!(escape("\u{a2}"), "\u{a2}");
        assert_eq!(escape("\u{ff}"), "\u{ff}");
    }
}
