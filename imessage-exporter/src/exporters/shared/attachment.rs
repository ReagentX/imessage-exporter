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

/// Why no file could be prepared. Converts into the
/// [`AttachmentRender`](crate::exporters::formatter::AttachmentRender)
/// variant of the same name.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AttachmentFailure {
    /// The row has no filename metadata.
    MissingFilename,
    /// The manager could not process the file; carries the display name.
    NamedFile(String),
}

/// Run the per-attachment side effects every exporter needs before it can
/// emit a reference to a file on disk: surface a busy indicator on the
/// progress bar for conversions that spawn ffmpeg (video transcoding and
/// animated-sticker decoding), then ask the
/// [`AttachmentManager`](crate::app::compatibility::attachment_manager::AttachmentManager)
/// to copy or convert the file.
///
/// Returns the [`ExportFile`] the export should refer to: the manager's copy
/// when it produced or reused one, else the resolved source path, else the
/// bare filename. The filename check runs after the manager so a copy failure
/// on a nameless row is [`AttachmentFailure::MissingFilename`] and prints
/// nothing.
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
        return Err(AttachmentFailure::MissingFilename);
    };
    let file = match handle_result {
        Ok(file) => file,
        Err(why) => {
            state.pb.println(why);
            return Err(AttachmentFailure::NamedFile(name.to_string()));
        }
    };

    // `Plan::Skip`: the export points at the source. An unresolvable source
    // is not an error here, so the bare filename stands in.
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
