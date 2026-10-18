use std::path::PathBuf;

use imessage_database::tables::attachment::MediaType;

use super::target::AttachmentTarget;

// MARK: ExportFile
/// The file an export refers to for one attachment: the copy under the
/// export root when the manager produced or reused one, else the source.
pub(crate) struct ExportFile<'a> {
    pub path: PathBuf,
    /// The copy's type after conversion, else the row's.
    pub media_type: MediaType<'a>,
    /// [`Attachment::filename()`](imessage_database::tables::attachment::Attachment::filename):
    /// `transfer_name`, else the stored path.
    pub name: &'a str,
}

// MARK: Plan
/// The work an [`AttachmentManager`](super::AttachmentManager) resolved for an
/// attachment, ready to be executed. Borrows the row for the media type and
/// display name the resulting [`ExportFile`] carries.
pub(crate) enum Plan<'a> {
    /// Copying is disabled; nothing to do.
    Skip,
    /// Existing output should be reused.
    Reuse(ExportFile<'a>),
    /// No existing copy; the attachment must be copied or converted to `target`.
    Process { target: AttachmentTarget<'a> },
}
