/*!
 Attachment metadata carried on a body range.
*/

use crabstep::deserializer::iter::Property;

/// Attachment metadata attached to a body range.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct AttachmentMeta {
    /// GUID of the attachment row.
    pub guid: Option<String>,
    /// Audio transcription stored on the attributed range.
    pub transcription: Option<String>,
    /// Inline media height in points.
    pub height: Option<f64>,
    /// Inline media width in points.
    pub width: Option<f64>,
    /// Original attachment filename.
    pub name: Option<String>,
}

impl AttachmentMeta {
    /// Applies a single typedstream attribute key/value pair to the metadata,
    /// ignoring any key that isn't attachment metadata. Driven per-key by the
    /// body parser's `build_range`, which walks the full attribute dictionary
    /// so non-attachment-meta keys on the same range are still processed.
    pub(crate) fn set_from_key_value<'a>(&mut self, key: &str, value: &Property<'a, 'a>) {
        match key {
            "__kIMFileTransferGUIDAttributeName" => {
                self.guid = value.as_string().map(String::from);
            }
            "IMAudioTranscription" => self.transcription = value.as_string().map(String::from),
            "__kIMInlineMediaHeightAttributeName" => self.height = value.as_f64(),
            "__kIMInlineMediaWidthAttributeName" => self.width = value.as_f64(),
            "__kIMFilenameAttributeName" => self.name = value.as_string().map(String::from),
            _ => {}
        }
    }
}
