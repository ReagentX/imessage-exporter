/*!
 Contains data structures used to describe export types
*/

use std::fmt::Display;

/// Represents the type of file to export iMessage data into
#[derive(PartialEq, Eq)]
pub enum ExportType {
    /// HTML file export
    HTML,
    /// Text file export
    TXT,
}

impl ExportType {
    /// Given user's input, return a variant if the input matches one
    pub fn from_cli(platform: &str) -> Option<Self> {
        match platform.to_lowercase().as_str() {
            "txt" => Some(Self::TXT),
            "html" => Some(Self::HTML),
            _ => None,
        }
    }
}

impl Display for ExportType {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportType::TXT => write!(fmt, "txt"),
            ExportType::HTML => write!(fmt, "html"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::util::export_type::ExportType;

    #[test]
    fn can_parse_html_any_case() {
        assert!(matches!(
            ExportType::from_cli("html"),
            Some(ExportType::HTML)
        ));
        assert!(matches!(
            ExportType::from_cli("HTML"),
            Some(ExportType::HTML)
        ));
        assert!(matches!(
            ExportType::from_cli("HtMl"),
            Some(ExportType::HTML)
        ));
    }

    #[test]
    fn can_parse_txt_any_case() {
        assert!(matches!(ExportType::from_cli("txt"), Some(ExportType::TXT)));
        assert!(matches!(ExportType::from_cli("TXT"), Some(ExportType::TXT)));
        assert!(matches!(ExportType::from_cli("tXt"), Some(ExportType::TXT)));
    }

    #[test]
    fn cant_parse_invalid() {
        assert!(ExportType::from_cli("pdf").is_none());
        assert!(ExportType::from_cli("json").is_none());
        assert!(ExportType::from_cli("").is_none());
    }
}
