use std::{
    fs::{create_dir_all, remove_file, write},
    path::PathBuf,
};

use imessage_database::{
    message_types::handwriting::HandwrittenMessage,
    tables::{
        attachment::{Attachment, MediaType},
        messages::Message,
    },
};

use crate::app::{
    compatibility::{
        backup::decrypt_file,
        converters::{
            audio::audio_copy_convert,
            common::{copy_raw, update_file_metadata},
            image::image_copy_convert,
            sticker::sticker_copy_convert,
            video::video_copy_convert,
        },
        error::ConversionError,
        models::{AudioConverter, Converter, HardwareEncoder, ImageConverter, VideoConverter},
    },
    runtime::Config,
};

use super::{
    mode::AttachmentManagerMode,
    plan::{ExportFile, Plan},
    target::AttachmentTarget,
};

// MARK: Manager
#[derive(Debug, PartialEq, Eq, Default)]
pub struct AttachmentManager {
    pub mode: AttachmentManagerMode,
    pub image_converter: Option<ImageConverter>,
    pub audio_converter: Option<AudioConverter>,
    pub video_converter: Option<VideoConverter>,
    hardware_encoder: Option<HardwareEncoder>,
}

impl AttachmentManager {
    pub fn from(mode: AttachmentManagerMode) -> Self {
        AttachmentManager {
            mode,
            image_converter: ImageConverter::determine(),
            audio_converter: AudioConverter::determine(),
            video_converter: VideoConverter::determine(),
            hardware_encoder: HardwareEncoder::detect(),
        }
    }
}

impl AttachmentManager {
    pub(crate) fn diagnostic(&self) {
        println!("Detected converters:");

        if let Some(converter) = &self.image_converter {
            println!("    Image converter: {converter}");
        } else {
            println!("    Image converter: None");
        }

        if let Some(converter) = &self.audio_converter {
            println!("    Audio converter: {converter}");
        } else {
            println!("    Audio converter: None");
        }

        if let Some(converter) = &self.video_converter {
            println!("    Video converter: {converter}");
        } else {
            println!("    Video converter: None");
        }
    }

    // MARK: Handwriting
    /// Write a handwriting message as SVG when attachment output is enabled.
    pub fn handle_handwriting(
        &self,
        message: &Message,
        handwriting: &HandwrittenMessage,
        config: &Config,
    ) -> Option<PathBuf> {
        if !matches!(self.mode, AttachmentManagerMode::Disabled) {
            // Create a path to copy the file to
            let mut to = config.attachment_path();

            // Add the subdirectory
            let sub_dir = config.conversation_attachment_path(message.chat_rowid());
            to.push(sub_dir);

            // Each handwriting payload has a unique ID.
            to.push(&handwriting.id);

            // Set the new file's extension to svg
            to.set_extension("svg");
            if to.exists() {
                return Some(to);
            }

            // Ensure the directory tree exists
            if let Some(folder) = to.parent()
                && !folder.exists()
                && let Err(why) = create_dir_all(folder)
            {
                eprintln!("Unable to create {}: {why}", folder.display());
            }

            // Attempt the svg render
            if let Err(why) = write(&to, handwriting.render_svg()) {
                eprintln!("Unable to write to {}: {why}", to.display());
            }

            // Update file metadata
            update_file_metadata(&to, &to, message, config);

            return Some(to);
        }
        None
    }

    // MARK: Files
    /// Decide what to do for an attachment without performing any decryption or
    /// conversion.
    ///
    /// Check the returned [`Plan`] for reusable output before calling
    /// [`execute`](Self::execute), and suppress conversion progress when reusing it.
    pub(crate) fn plan<'a>(
        &self,
        message: &Message,
        attachment: &'a Attachment,
        config: &Config,
    ) -> Result<Plan<'a>, ConversionError> {
        if matches!(self.mode, AttachmentManagerMode::Disabled) {
            return Ok(Plan::Skip);
        }

        let Some(target) = AttachmentTarget::resolve(message, attachment, config) else {
            return Err(ConversionError::UnresolvedPath {
                transfer_name: attachment.transfer_name.clone(),
            });
        };

        // Check for output on disk because a fresh attachment row is loaded
        // for each reference.
        if let Some(file) = target.existing_copy(attachment) {
            return Ok(Plan::Reuse(file));
        }

        Ok(Plan::Process { target })
    }

    /// Carry out a [`Plan`]. Return `None` for [`Plan::Skip`]; in that case,
    /// use the source file in the export.
    pub(crate) fn execute<'a>(
        &self,
        plan: Plan<'a>,
        message: &Message,
        attachment: &'a Attachment,
        config: &Config,
    ) -> Result<Option<ExportFile<'a>>, ConversionError> {
        match plan {
            Plan::Skip => Ok(None),
            Plan::Reuse(file) => Ok(Some(file)),
            Plan::Process { target } => {
                self.process(message, attachment, config, &target).map(Some)
            }
        }
    }

    /// Decrypt (when needed), copy or convert, and write the attachment to the
    /// resolved `target`.
    fn process<'a>(
        &self,
        message: &Message,
        attachment: &'a Attachment,
        config: &Config,
        target: &AttachmentTarget<'a>,
    ) -> Result<ExportFile<'a>, ConversionError> {
        // The working source; `target.source()` is retained for metadata below
        let mut from = target.source().to_path_buf();
        let mut is_temp = false;

        // Handle encrypted files from iOS backups
        if let Some(backup) = &config.data_source.backup {
            // We shouldn't get here without an encrypted backup, but just in case, validate it
            if backup.is_encrypted() {
                match decrypt_file(backup, &from) {
                    Ok(decrypted_path) => {
                        // Use the decrypted file; it is temporary, so remove it later
                        from = decrypted_path;
                        is_temp = true;
                    }
                    Err(why) => {
                        return Err(ConversionError::DecryptFailed {
                            path: from,
                            source: why,
                        });
                    }
                }
            }
        }

        // Ensure the file exists at the specified location
        if !from.exists() {
            return Err(ConversionError::NotFound { path: from });
        }

        // Target path with the source extension; the converters reassign this to
        // their output path on success and fall back to it on failure.
        let mut to = target.raw_path(attachment);

        // If we convert the attachment, we need to update the media type
        let mut new_media_type: Option<MediaType> = None;

        let mime_type = attachment.mime_type();
        match mime_type {
            MediaType::Image(_) => match self.mode {
                AttachmentManagerMode::Basic | AttachmentManagerMode::Full => {
                    match &self.image_converter {
                        Some(converter) => {
                            if attachment.is_sticker {
                                new_media_type = sticker_copy_convert(
                                    &from,
                                    &mut to,
                                    converter,
                                    self.video_converter.as_ref(),
                                    &mime_type,
                                );
                            } else {
                                new_media_type =
                                    image_copy_convert(&from, &mut to, converter, &mime_type);
                            }
                        }
                        None => copy_raw(&from, &to),
                    }
                }
                AttachmentManagerMode::Clone => copy_raw(&from, &to),
                AttachmentManagerMode::Disabled => unreachable!(),
            },
            MediaType::Video(_) => match self.mode {
                AttachmentManagerMode::Full => match &self.video_converter {
                    Some(converter) => {
                        new_media_type = video_copy_convert(
                            &from,
                            &mut to,
                            converter,
                            &self.hardware_encoder,
                            &mime_type,
                        );
                    }
                    None => copy_raw(&from, &to),
                },
                AttachmentManagerMode::Clone | AttachmentManagerMode::Basic => {
                    copy_raw(&from, &to);
                }
                AttachmentManagerMode::Disabled => unreachable!(),
            },
            MediaType::Audio(_) => match self.mode {
                AttachmentManagerMode::Full => match &self.audio_converter {
                    Some(converter) => {
                        new_media_type = audio_copy_convert(&from, &mut to, converter, &mime_type);
                    }
                    None => copy_raw(&from, &to),
                },
                AttachmentManagerMode::Clone | AttachmentManagerMode::Basic => {
                    copy_raw(&from, &to);
                }
                AttachmentManagerMode::Disabled => unreachable!(),
            },
            _ => copy_raw(&from, &to),
        }

        // Update file metadata; a decrypted file takes the original's metadata
        if is_temp {
            update_file_metadata(target.source(), &to, message, config);
        } else {
            update_file_metadata(&from, &to, message, config);
        }

        let file = ExportFile {
            path: to,
            media_type: new_media_type.unwrap_or(mime_type),
            name: target.name(),
        };

        // Remove the temporary file used for decryption, if it exists
        if is_temp && let Err(why) = remove_file(&from) {
            eprintln!("Unable to remove encrypted file {}: {why}", from.display());
        }

        Ok(file)
    }
}
