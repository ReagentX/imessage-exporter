use imessage_database::tables::{
    attachment::{Attachment, MediaType},
    messages::Message,
};

use crate::{
    app::{
        compatibility::attachment_manager::{AttachmentManagerMode, CopiedFile, Plan},
        runtime::Config,
    },
    exporters::{formatter::AttachmentRender, shared::driver::ExportState},
};

/// Run the per-attachment side effects every exporter needs before it can
/// emit a reference to a file on disk: surface a busy indicator on the
/// progress bar for conversions that spawn ffmpeg (video transcoding and
/// animated-sticker decoding), then ask the
/// [`AttachmentManager`](crate::app::compatibility::attachment_manager::AttachmentManager)
/// to copy or convert the file.
///
/// Returns `Ok(())` when the attachment has a filename and the copy/convert
/// succeeded; otherwise returns the [`AttachmentRender`] fallback the caller
/// should propagate.
pub(crate) fn prepare_attachment(
    config: &Config,
    state: &ExportState,
    attachment: &mut Attachment,
    message: &Message,
) -> Result<(), AttachmentRender> {
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

    let Some(filename) = attachment.filename() else {
        return Err(AttachmentRender::MissingFilename);
    };
    let copied: Option<CopiedFile> = match handle_result {
        Ok(copied) => copied,
        Err(why) => {
            state.pb.println(why);
            return Err(AttachmentRender::NamedFile(filename.to_string()));
        }
    };

    // The last writer of exporter state on the row; the next phase removes it.
    if let Some(copied) = copied {
        attachment.copied_path = Some(copied.path);
        if let Some(media_type) = copied.media_type {
            attachment.mime_type = Some(media_type.as_mime_type());
        }
    }
    Ok(())
}
