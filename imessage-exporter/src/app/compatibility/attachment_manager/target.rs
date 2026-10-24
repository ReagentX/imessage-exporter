use std::path::{Path, PathBuf};

use imessage_database::tables::{attachment::Attachment, messages::Message};

use crate::app::runtime::Config;

use super::plan::ExportFile;

// MARK: Target
/// Where an attachment's source lives and where its copy should go. Resolved
/// once so the dedup probe and the copy step cannot disagree on naming.
pub(crate) struct AttachmentTarget<'a> {
    /// Resolved source path on the host, before any decryption rebinds it.
    /// iOS backups resolve to a flat hash-named file, so backup attachments are
    /// always treated as files; directory attachments only occur for macOS
    /// filesystem sources.
    source: PathBuf,
    /// Whether the source is a directory.
    source_is_dir: bool,
    /// Destination directory for this conversation's attachments.
    dir: PathBuf,
    /// Display name for the resulting [`ExportFile`].
    name: &'a str,
}

impl<'a> AttachmentTarget<'a> {
    /// Resolve the source and destination directory, or [`None`] if the
    /// attachment's path cannot be resolved.
    pub(super) fn resolve(
        message: &Message,
        attachment: &'a Attachment,
        config: &Config,
    ) -> Option<Self> {
        let source = PathBuf::from(attachment.resolved_attachment_path(
            &config.options.platform,
            &config.options.db_path,
            config.options.attachment_root.as_deref(),
        )?);
        let source_is_dir = source.is_dir();
        // The stored filename is available as a fallback after resolving the source.
        let name = attachment.filename()?;

        let mut dir = config.attachment_path();
        dir.push(config.conversation_attachment_path(message.chat_rowid()));

        Some(Self {
            source,
            source_is_dir,
            dir,
            name,
        })
    }

    /// The resolved source path on the host, before any decryption rebinds it.
    pub(super) fn source(&self) -> &Path {
        &self.source
    }

    /// The attachment's display name.
    pub(super) fn name(&self) -> &'a str {
        self.name
    }

    /// The destination for a raw copy: `<dir>/<rowid>.<source_ext>`, or bare
    /// `<dir>/<rowid>` for a directory source.
    pub(super) fn raw_path(&self, attachment: &Attachment) -> PathBuf {
        let mut path = self.dir.join(attachment.rowid.to_string());
        if !self.source_is_dir
            && let Some(ext) = attachment.extension()
        {
            path.set_extension(ext);
        }
        path
    }

    /// Reuse only a regular file whose complete bytes match the source.
    /// Converted or encrypted files cannot be verified this way and are rebuilt.
    pub(super) fn existing_copy(&self, attachment: &'a Attachment) -> Option<ExportFile<'a>> {
        // Check the source extension for a raw copy, or no extension for a directory.
        let candidate = self.raw_path(attachment);
        if crate::app::compatibility::converters::common::files_equal(&self.source, &candidate)
            .unwrap_or(false)
        {
            return Some(ExportFile {
                path: candidate,
                media_type: attachment.mime_type(),
                name: self.name,
            });
        }

        None
    }
}
