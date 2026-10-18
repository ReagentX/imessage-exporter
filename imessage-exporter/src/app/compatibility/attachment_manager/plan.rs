use std::path::PathBuf;

use imessage_database::tables::attachment::MediaType;

use super::target::AttachmentTarget;

// MARK: CopiedFile
/// A file the manager wrote or found under the export root for one attachment.
pub(crate) struct CopiedFile {
    pub path: PathBuf,
    /// The converter's output type; `None` when the copy kept the source type.
    pub media_type: Option<MediaType<'static>>,
}

// MARK: Plan
/// The work an [`AttachmentManager`](super::AttachmentManager) resolved for an
/// attachment, ready to be executed.
pub(crate) enum Plan {
    /// Copying is disabled; nothing to do.
    Skip,
    /// Existing output should be reused.
    Reuse(CopiedFile),
    /// No existing copy; the attachment must be copied or converted to `target`.
    Process { target: AttachmentTarget },
}
