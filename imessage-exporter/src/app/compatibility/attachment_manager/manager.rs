use std::{io::Write, path::PathBuf};

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
            common::{copy_raw, output_directory, update_file_metadata},
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
    ) -> std::io::Result<Option<PathBuf>> {
        if matches!(self.mode, AttachmentManagerMode::Disabled) {
            return Ok(None);
        }
        let dir = config
            .attachment_path()
            .join(config.conversation_attachment_path(message.chat_rowid()));
        output_directory(&config.options.export_path, &dir)?;
        // The database row ID is numeric; payload IDs never enter filesystem paths.
        let to = dir.join(format!("handwriting-{}.svg", message.rowid));
        let mut file = tempfile::NamedTempFile::new_in(&dir)?;
        file.write_all(handwriting.render_svg().as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&to).map_err(|e| e.error)?;
        update_file_metadata(&to, &to, message, config);
        Ok(Some(to))
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

        let output = target.raw_path(attachment);
        output_directory(&config.options.export_path, output.parent().unwrap())?;
        // Check for output on disk because a fresh attachment row is loaded
        // for each reference.
        if matches!(self.mode, AttachmentManagerMode::Clone)
            && config.data_source.backup.is_none()
            && let Some(file) = target.existing_copy(attachment)
        {
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
        let mut decrypted = None;

        // Handle encrypted files from iOS backups
        if let Some(backup) = &config.data_source.backup {
            // We shouldn't get here without an encrypted backup, but just in case, validate it
            if backup.is_encrypted() {
                match decrypt_file(backup, &from) {
                    Ok(decrypted_path) => {
                        // Use the decrypted file; it is temporary, so remove it later
                        from = decrypted_path.path().to_path_buf();
                        decrypted = Some(decrypted_path);
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
        let destination = target.raw_path(attachment);
        let parent = destination
            .parent()
            .ok_or_else(|| std::io::Error::other("No output parent"))?;
        output_directory(&config.options.export_path, parent)?;
        // Converters only write into an exclusive per-attachment staging directory.
        let stage = tempfile::tempdir_in(parent)?;
        let mut to = stage.path().join(destination.file_name().unwrap());

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
                                )?;
                            } else {
                                new_media_type =
                                    image_copy_convert(&from, &mut to, converter, &mime_type)?;
                            }
                        }
                        None => copy_raw(&from, &to)?,
                    }
                }
                AttachmentManagerMode::Clone => copy_raw(&from, &to)?,
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
                        )?;
                    }
                    None => copy_raw(&from, &to)?,
                },
                AttachmentManagerMode::Clone | AttachmentManagerMode::Basic => {
                    copy_raw(&from, &to)?;
                }
                AttachmentManagerMode::Disabled => unreachable!(),
            },
            MediaType::Audio(_) => match self.mode {
                AttachmentManagerMode::Full => match &self.audio_converter {
                    Some(converter) => {
                        new_media_type = audio_copy_convert(&from, &mut to, converter, &mime_type)?;
                    }
                    None => copy_raw(&from, &to)?,
                },
                AttachmentManagerMode::Clone | AttachmentManagerMode::Basic => {
                    copy_raw(&from, &to)?;
                }
                AttachmentManagerMode::Disabled => unreachable!(),
            },
            _ => copy_raw(&from, &to)?,
        }

        // A converter exit code alone is not evidence that it wrote an output.
        let info = std::fs::symlink_metadata(&to)?;
        if info.file_type().is_symlink() || (!info.is_file() && !info.is_dir()) {
            return Err(std::io::Error::other("Invalid attachment output").into());
        }
        if new_media_type.is_some() && info.len() == 0 {
            return Err(std::io::Error::other("Empty converted attachment").into());
        }
        let mut final_path = destination;
        if let Some(ext) = to.extension() {
            final_path.set_extension(ext);
        }
        // rename replaces an existing file/symlink without following it. Directory
        // collisions fail closed instead of merging potentially stale trees.
        std::fs::rename(&to, &final_path)?;
        update_file_metadata(target.source(), &final_path, message, config);
        drop(decrypted);
        let file = ExportFile {
            path: final_path,
            media_type: new_media_type.unwrap_or(mime_type),
            name: target.name(),
        };
        Ok(file)
    }
}
