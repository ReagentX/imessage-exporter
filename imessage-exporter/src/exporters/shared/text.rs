// MARK: OptionalText

/// An `Option<&str>` that treats `Some("")` as absent.
///
/// iMessage plist payloads frequently carry empty-string fields where a
/// `None` would be more semantically accurate; wrapping balloon VM fields
/// in this type normalizes the two cases so templates don't emit blank
/// lines for empty-but-present strings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OptionalText<'a>(Option<&'a str>);

impl<'a> OptionalText<'a> {
    /// A link target with dangerous schemes and control characters rejected.
    pub fn get_url(&self) -> Option<&'a str> {
        self.0.and_then(crate::app::sanitizers::safe_link)
    }

    /// The wrapped value with empty strings filtered out.
    pub fn get(&self) -> Option<&'a str> {
        self.0
    }
}

impl<'a> From<Option<&'a str>> for OptionalText<'a> {
    fn from(value: Option<&'a str>) -> Self {
        Self(value.filter(|s| !s.is_empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::OptionalText;

    #[test]
    fn none_stays_none() {
        assert_eq!(OptionalText::from(None).get(), None);
    }

    #[test]
    fn empty_string_becomes_none() {
        assert_eq!(OptionalText::from(Some("")).get(), None);
    }

    #[test]
    fn whitespace_is_preserved() {
        // Only the literal empty string is filtered; whitespace-only content
        // is meaningful (e.g., a single space in a balloon field).
        assert_eq!(OptionalText::from(Some(" ")).get(), Some(" "));
    }

    #[test]
    fn populated_string_passes_through() {
        assert_eq!(OptionalText::from(Some("x")).get(), Some("x"));
    }
}
