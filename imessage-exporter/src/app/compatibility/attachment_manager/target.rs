use std::path::{Path, PathBuf};

use imessage_database::tables::{
    attachment::{Attachment, MediaType},
    messages::Message,
};

use crate::app::runtime::Config;

// MARK: Target
/// Where an attachment's source lives and where its copy should go. Resolved
/// once so the dedup probe and the copy step cannot disagree on naming.
pub(crate) struct AttachmentTarget {
    /// Resolved source path on the host, before any decryption rebinds it.
    /// iOS backups resolve to a flat hash-named file, so backup attachments are
    /// always treated as files; directory attachments only occur for macOS
    /// filesystem sources.
    source: PathBuf,
    /// Whether the source is a directory.
    source_is_dir: bool,
    /// Destination directory for this conversation's attachments.
    dir: PathBuf,
}

impl AttachmentTarget {
    /// Resolve the source and destination directory, or [`None`] if the
    /// attachment's path cannot be resolved.
    pub(super) fn resolve(
        message: &Message,
        attachment: &Attachment,
        config: &Config,
    ) -> Option<Self> {
        let source = PathBuf::from(attachment.resolved_attachment_path(
            &config.options.platform,
            &config.options.db_path,
            config.options.attachment_root.as_deref(),
        )?);
        let source_is_dir = source.is_dir();

        let mut dir = config.attachment_path();
        dir.push(config.conversation_attachment_path(message.chat_rowid()));

        Some(Self {
            source,
            source_is_dir,
            dir,
        })
    }

    /// The resolved source path on the host, before any decryption rebinds it.
    pub(super) fn source(&self) -> &Path {
        &self.source
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

    /// Locate an existing output for this attachment, if any.
    ///
    /// The rowid is unique within the conversation directory, so any file there
    /// whose stem is the rowid belongs to this attachment.
    /// The source extension is probed first: [`copy_raw`](crate::app::compatibility::converters::common::copy_raw)
    /// (Clone mode, unconvertible types, and the fallback used when a converter
    /// fails) writes `<rowid>.<source_ext>` and leaves the media type unchanged.
    /// If that is absent, a converter must have run, so the conversion outputs
    /// are probed and the media type is rebuilt from the matched extension.
    ///
    /// The mode is deliberately not consulted, so reusing an export directory
    /// across modes can reuse an existing sibling. A clean export is exact. A
    /// conversion that keeps failing and only leaves a raw copy is served from
    /// that copy rather than retried on every reference.
    ///
    /// Returns the path to reuse and the media type to record; a [`None`] media
    /// type leaves the attachment's existing media type unchanged.
    pub(super) fn existing_copy(
        &self,
        attachment: &Attachment,
    ) -> Option<(PathBuf, Option<MediaType<'static>>)> {
        // A raw copy keeps the source extension, or no extension for a directory.
        let candidate = self.raw_path(attachment);
        if candidate.exists() {
            return Some((candidate, None));
        }

        // A converted copy uses the converted extension, so rebuild the media
        // type from that extension.
        let mime_type = attachment.mime_type();
        for &ext in conversion_output_extensions(&mime_type) {
            let converted = candidate.with_extension(ext);
            if converted.exists() {
                return Some((converted, converted_media_type(&mime_type, ext)));
            }
        }

        None
    }
}

// MARK: Dedup
/// The extensions a converter may write for a given source category, in
/// addition to the source's own extension. This reflects the converters'
/// output formats only.
fn conversion_output_extensions(mime_type: &MediaType) -> &'static [&'static str] {
    match mime_type {
        MediaType::Image(_) => &["jpeg", "png", "gif"],
        MediaType::Audio(_) | MediaType::Video(_) => &["mp4"],
        _ => &[],
    }
}

/// Rebuild the [`MediaType`] for a converted output. Every converter preserves
/// the source category and only changes the subtype, so the converted media
/// type is the source category paired with the output extension.
fn converted_media_type(
    source: &MediaType,
    output_ext: &'static str,
) -> Option<MediaType<'static>> {
    match source {
        MediaType::Image(_) => Some(MediaType::Image(output_ext)),
        MediaType::Audio(_) => Some(MediaType::Audio(output_ext)),
        MediaType::Video(_) => Some(MediaType::Video(output_ext)),
        _ => None,
    }
}

// MARK: Dedup tests
#[cfg(test)]
mod dedup_tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use imessage_database::tables::attachment::MediaType;

    use super::{AttachmentTarget, conversion_output_extensions, converted_media_type};
    use crate::app::{runtime::Config, test_dir::unique_test_dir};

    /// An [`AttachmentTarget`] pointing at `dir`; `source` is unused by
    /// [`AttachmentTarget::existing_copy`], so a placeholder is fine.
    fn target(dir: &Path, source_is_dir: bool) -> AttachmentTarget {
        AttachmentTarget {
            source: PathBuf::new(),
            source_is_dir,
            dir: dir.to_path_buf(),
        }
    }

    #[test]
    fn raw_copy_returns_path_and_leaves_mime() {
        let dir = unique_test_dir("raw");
        let mut attachment = Config::fake_attachment();
        attachment.rowid = 7;
        attachment.filename = Some("x/y/photo.heic".to_string());
        attachment.mime_type = Some("image/heic".to_string());
        fs::write(dir.join("7.heic"), b"data").unwrap();

        let (path, media_type) = target(&dir, false).existing_copy(&attachment).unwrap();
        assert_eq!(path, dir.join("7.heic"));
        assert!(media_type.is_none());
    }

    #[test]
    fn converted_image_rebuilds_jpeg_mime() {
        let dir = unique_test_dir("image");
        let mut attachment = Config::fake_attachment();
        attachment.rowid = 7;
        attachment.filename = Some("x/y/photo.heic".to_string());
        attachment.mime_type = Some("image/heic".to_string());
        fs::write(dir.join("7.jpeg"), b"data").unwrap();

        let (path, media_type) = target(&dir, false).existing_copy(&attachment).unwrap();
        assert_eq!(path, dir.join("7.jpeg"));
        assert_eq!(media_type.unwrap().as_mime_type(), "image/jpeg");
    }

    #[test]
    fn converted_sticker_rebuilds_png_mime() {
        let dir = unique_test_dir("sticker");
        let mut attachment = Config::fake_attachment();
        attachment.rowid = 7;
        attachment.filename = Some("x/y/sticker.heic".to_string());
        attachment.mime_type = Some("image/heic".to_string());
        attachment.is_sticker = true;
        fs::write(dir.join("7.png"), b"data").unwrap();

        let (path, media_type) = target(&dir, false).existing_copy(&attachment).unwrap();
        assert_eq!(path, dir.join("7.png"));
        assert_eq!(media_type.unwrap().as_mime_type(), "image/png");
    }

    #[test]
    fn converted_audio_keeps_audio_category() {
        let dir = unique_test_dir("audio");
        let mut attachment = Config::fake_attachment();
        attachment.rowid = 7;
        attachment.filename = Some("x/y/voice.caf".to_string());
        attachment.mime_type = Some("audio/x-caf".to_string());
        fs::write(dir.join("7.mp4"), b"data").unwrap();

        let (path, media_type) = target(&dir, false).existing_copy(&attachment).unwrap();
        assert_eq!(path, dir.join("7.mp4"));
        assert_eq!(media_type.unwrap().as_mime_type(), "audio/mp4");
    }

    #[test]
    fn converted_video_keeps_video_category() {
        let dir = unique_test_dir("video");
        let mut attachment = Config::fake_attachment();
        attachment.rowid = 7;
        attachment.filename = Some("x/y/clip.mov".to_string());
        attachment.mime_type = Some("video/quicktime".to_string());
        fs::write(dir.join("7.mp4"), b"data").unwrap();

        let (path, media_type) = target(&dir, false).existing_copy(&attachment).unwrap();
        assert_eq!(path, dir.join("7.mp4"));
        assert_eq!(media_type.unwrap().as_mime_type(), "video/mp4");
    }

    #[test]
    fn already_compatible_extension_is_left_as_raw() {
        // `jpeg` is also a conversion output, so this pins that the source
        // extension is probed first and the media type is left unchanged.
        let dir = unique_test_dir("compatible");
        let mut attachment = Config::fake_attachment();
        attachment.rowid = 7;
        attachment.filename = Some("x/y/photo.jpeg".to_string());
        attachment.mime_type = Some("image/jpeg".to_string());
        fs::write(dir.join("7.jpeg"), b"data").unwrap();

        let (path, media_type) = target(&dir, false).existing_copy(&attachment).unwrap();
        assert_eq!(path, dir.join("7.jpeg"));
        assert!(media_type.is_none(), "a raw copy must not rebuild the mime");
    }

    #[test]
    fn source_extension_wins_when_both_exist() {
        // Mixed-mode re-export: both the raw and converted outputs are present.
        // The source extension is matched first and the mime is left unchanged.
        let dir = unique_test_dir("both");
        let mut attachment = Config::fake_attachment();
        attachment.rowid = 7;
        attachment.filename = Some("x/y/photo.heic".to_string());
        attachment.mime_type = Some("image/heic".to_string());
        fs::write(dir.join("7.heic"), b"raw").unwrap();
        fs::write(dir.join("7.jpeg"), b"converted").unwrap();

        let (path, media_type) = target(&dir, false).existing_copy(&attachment).unwrap();
        assert_eq!(path, dir.join("7.heic"));
        assert!(media_type.is_none());
    }

    #[test]
    fn directory_attachment_matches_bare_rowid() {
        let dir = unique_test_dir("directory");
        let mut attachment = Config::fake_attachment();
        attachment.rowid = 7;
        attachment.filename = Some("x/y/bundle".to_string());
        attachment.mime_type = Some("application/octet-stream".to_string());
        fs::create_dir(dir.join("7")).unwrap();

        let (path, media_type) = target(&dir, true).existing_copy(&attachment).unwrap();
        assert_eq!(path, dir.join("7"));
        assert!(media_type.is_none());
    }

    #[test]
    fn no_existing_copy_returns_none() {
        let dir = unique_test_dir("none");
        let mut attachment = Config::fake_attachment();
        attachment.rowid = 7;
        attachment.filename = Some("x/y/photo.heic".to_string());
        attachment.mime_type = Some("image/heic".to_string());

        assert!(target(&dir, false).existing_copy(&attachment).is_none());
    }

    #[test]
    fn conversion_outputs_match_converter_formats() {
        assert_eq!(
            conversion_output_extensions(&MediaType::Image("heic")),
            ["jpeg", "png", "gif"].as_slice()
        );
        assert_eq!(
            conversion_output_extensions(&MediaType::Audio("caf")),
            ["mp4"].as_slice()
        );
        assert_eq!(
            conversion_output_extensions(&MediaType::Video("quicktime")),
            ["mp4"].as_slice()
        );
        assert!(conversion_output_extensions(&MediaType::Application("pdf")).is_empty());
        assert!(conversion_output_extensions(&MediaType::Text("plain")).is_empty());
        assert!(conversion_output_extensions(&MediaType::Unknown).is_empty());
    }

    #[test]
    fn converted_media_type_preserves_category() {
        assert_eq!(
            converted_media_type(&MediaType::Image("heic"), "jpeg"),
            Some(MediaType::Image("jpeg"))
        );
        assert_eq!(
            converted_media_type(&MediaType::Audio("caf"), "mp4"),
            Some(MediaType::Audio("mp4"))
        );
        assert_eq!(
            converted_media_type(&MediaType::Video("quicktime"), "mp4"),
            Some(MediaType::Video("mp4"))
        );
        assert_eq!(
            converted_media_type(&MediaType::Application("pdf"), "mp4"),
            None
        );
    }
}
