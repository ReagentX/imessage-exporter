/*!
 Contains data structures used to describe export types.
*/

use std::fmt::Display;

/// Represents the type of file to export iMessage data into
#[derive(PartialEq, Eq, Debug)]
pub enum ExportType {
    /// HTML file export
    Html,
    /// JSON file export
    Json,
    /// Text file export
    Txt,
}

impl ExportType {
    /// Given user's input, return a variant if the input matches one
    pub fn from_cli(platform: &str) -> Option<Self> {
        match platform.to_lowercase().as_str() {
            "html" => Some(Self::Html),
            "json" => Some(Self::Json),
            "txt" => Some(Self::Txt),
            _ => None,
        }
    }

    /// Get the file name extension for the given export type
    pub fn extension(&self) -> &str {
        match self {
            ExportType::Html => ".html",
            ExportType::Json => ".json",
            ExportType::Txt => ".txt",
        }
    }
}

impl Display for ExportType {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportType::Html => write!(fmt, "html"),
            ExportType::Json => write!(fmt, "json"),
            ExportType::Txt => write!(fmt, "txt"),
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
    fn can_parse_json_any_case() {
        assert!(matches!(
            ExportType::from_cli("json"),
            Some(ExportType::Json)
        ));
        assert!(matches!(
            ExportType::from_cli("JSON"),
            Some(ExportType::Json)
        ));
        assert!(matches!(
            ExportType::from_cli("JsOn"),
            Some(ExportType::Json)
        ));
    }

    #[test]
    fn cant_parse_invalid() {
        assert!(ExportType::from_cli("pdf").is_none());
        assert!(ExportType::from_cli("xml").is_none());
        assert!(ExportType::from_cli("").is_none());
    }
}
