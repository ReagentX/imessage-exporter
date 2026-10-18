use std::path::PathBuf;

use imessage_database::tables::attachment::MediaType;

use super::target::AttachmentTarget;

// MARK: ExportFile
/// Path, media type, and display name for an exported attachment.
/// Use the copy under the export root when available, else the source.
pub(crate) struct ExportFile<'a> {
    pub path: PathBuf,
    /// Media type after conversion, else the source row's type.
    pub media_type: MediaType<'a>,
    /// [`Attachment::filename()`](imessage_database::tables::attachment::Attachment::filename):
    /// `transfer_name`, else the stored path.
    pub name: &'a str,
}

// MARK: Plan
/// Resolved work for one attachment, ready for execution by an
/// [`AttachmentManager`](super::AttachmentManager). The media type and display
/// name for the resulting [`ExportFile`] are borrowed from the source row.
pub(crate) enum Plan<'a> {
    /// Copying is disabled; nothing to do.
    Skip,
    /// Reuse existing output.
    Reuse(ExportFile<'a>),
    /// No existing copy; the attachment must be copied or converted to `target`.
    Process { target: AttachmentTarget<'a> },
}
