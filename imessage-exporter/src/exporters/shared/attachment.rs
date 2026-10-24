use std::path::PathBuf;

use imessage_database::tables::{
    attachment::{Attachment, MediaType},
    messages::Message,
};

use crate::{
    app::{
        compatibility::attachment_manager::{AttachmentManagerMode, Plan},
        runtime::Config,
    },
    exporters::shared::driver::ExportState,
};

pub(crate) use crate::app::compatibility::attachment_manager::ExportFile;

/// Reason for an attachment preparation failure. Convert into the
/// [`AttachmentRender`](crate::exporters::formatter::AttachmentRender)
/// variant of the same name.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AttachmentFailure {
    /// No filename metadata on the row.
    MissingFilename,
    /// Failure to process the file, with the attachment's display name.
    NamedFile(String),
}

/// Prepare an attachment before rendering a reference to its file: show a busy
/// indicator for ffmpeg video transcoding and animated-sticker decoding, then
/// copy or convert the file with the
/// [`AttachmentManager`](crate::app::compatibility::attachment_manager::AttachmentManager).
///
/// Return an [`ExportFile`] for the produced or reused copy, else the resolved
/// source path, else the bare filename. Check for a filename after processing
/// the attachment. Without one, return [`AttachmentFailure::MissingFilename`]
/// and suppress any copy error instead of printing it.
pub(crate) fn prepare_attachment<'a>(
    config: &Config,
    state: &ExportState,
    attachment: &'a Attachment,
    message: &Message,
) -> Result<ExportFile<'a>, AttachmentFailure> {
    let manager = &config.options.attachment_manager;

    // Classify the `ffmpeg`-spawning conversion paths, which get a busy bar:
    //   - animated sticker (image MIME, e.g. HEICS) in Basic or Full
    //     → `sticker_copy_convert`
    //   - animated sticker (video MIME, e.g. video Memoji) in Full only
    //     → `video_copy_convert`
    //   - any non-sticker video in Full → `video_copy_convert`
    let is_video_full = matches!(attachment.mime_type(), MediaType::Video(_))
        && matches!(manager.mode, AttachmentManagerMode::Full);
    let is_animated_sticker_encoding = attachment.is_animated_sticker()
        && match attachment.mime_type() {
            // HEICS etc. → `sticker_copy_convert`, which runs in Basic or Full.
            MediaType::Image(_) => {
                matches!(
                    manager.mode,
                    AttachmentManagerMode::Basic | AttachmentManagerMode::Full
                )
            }
            // Video Memoji etc. → `video_copy_convert`, which runs in Full only.
            _ => matches!(manager.mode, AttachmentManagerMode::Full),
        };

    // Resolve the work up front so the busy indicator is only shown when a
    // conversion will actually run: a duplicate reference whose output already
    // exists plans as `Reuse` and spawns no ffmpeg.
    let plan = manager.plan(message, attachment, config);
    let will_convert = matches!(plan, Ok(Plan::Process { .. }));

    let busy_label = if !will_convert {
        None
    } else if is_animated_sticker_encoding {
        Some("Encoding animated sticker, estimates paused...")
    } else if is_video_full {
        Some("Encoding video, estimates paused...")
    } else {
        None
    };

    if let Some(label) = busy_label {
        state.pb.set_busy_style(label.to_string());
    }

    let handle_result = plan.and_then(|plan| manager.execute(plan, message, attachment, config));

    if busy_label.is_some() {
        state.pb.set_default_style();
    }

    let Some(name) = attachment.filename() else {
        state.errors.set(state.errors.get() + 1);
        return Err(AttachmentFailure::MissingFilename);
    };
    let file = match handle_result {
        Ok(file) => file,
        Err(why) => {
            state.errors.set(state.errors.get() + 1);
            state.pb.println(why);
            return Err(AttachmentFailure::NamedFile(name.to_string()));
        }
    };

    // For `Plan::Skip`, use the source path in the export. If the source cannot
    // be resolved, use the bare filename without reporting an error.
    Ok(file.unwrap_or_else(|| ExportFile {
        path: attachment
            .resolved_attachment_path(
                &config.options.platform,
                &config.options.db_path,
                config.options.attachment_root.as_deref(),
            )
            .map_or_else(|| PathBuf::from(name), PathBuf::from),
        media_type: attachment.mime_type(),
        name,
    }))
}
