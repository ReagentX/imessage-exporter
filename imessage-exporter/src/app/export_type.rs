/*!
 Contains data structures used to describe export types.
*/

use std::fmt::Display;

/// Represents the type of file to export iMessage data into
#[derive(PartialEq, Eq, Debug)]
pub enum ExportType {
    /// HTML file export
    Html,
    /// Text file export
    Txt,
    /// XML file export
    Xml,
}

impl ExportType {
    /// Given user's input, return a variant if the input matches one
    pub fn from_cli(platform: &str) -> Option<Self> {
        match platform.to_lowercase().as_str() {
            "txt" => Some(Self::Txt),
            "html" => Some(Self::Html),
            "xml" => Some(Self::Xml),
            _ => None,
        }
    }

    /// Get the file name extension for the given export type
    pub fn extension(&self) -> &str {
        match self {
            ExportType::Html => ".html",
            ExportType::Txt => ".txt",
            ExportType::Xml => ".xml",
        }
    }
}

impl Display for ExportType {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportType::Txt => write!(fmt, "txt"),
            ExportType::Html => write!(fmt, "html"),
            ExportType::Xml => write!(fmt, "xml"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::export_type::ExportType;

    #[test]
    fn can_parse_html_any_case() {
        assert!(matches!(
            ExportType::from_cli("html"),
            Some(ExportType::Html)
        ));
        assert!(matches!(
            ExportType::from_cli("HTML"),
            Some(ExportType::Html)
        ));
        assert!(matches!(
            ExportType::from_cli("HtMl"),
            Some(ExportType::Html)
        ));
    }

    #[test]
    fn can_parse_txt_any_case() {
        assert!(matches!(ExportType::from_cli("txt"), Some(ExportType::Txt)));
        assert!(matches!(ExportType::from_cli("TXT"), Some(ExportType::Txt)));
        assert!(matches!(ExportType::from_cli("tXt"), Some(ExportType::Txt)));
    }

    #[test]
    fn cant_parse_invalid() {
        assert!(ExportType::from_cli("pdf").is_none());
        assert!(ExportType::from_cli("json").is_none());
        assert!(ExportType::from_cli("").is_none());
    }
}
