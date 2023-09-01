use std::{
    fmt::Display,
    fs::{copy, create_dir_all, metadata},
    path::{Path, PathBuf},
};

use filetime::{set_file_times, FileTime};
use imessage_database::tables::{attachment::Attachment, messages::Message};
use uuid::Uuid;

use crate::app::{
    converter::{heic_to_jpeg, Converter},
    runtime::Config,
};

/// Represents different ways the app can interact with attachment data
pub enum AttachmentManager {
    /// Do not copy attachments
    Disabled,
    /// Copy and convert attachments to more compatible formats using a [crate::app::converter::Converter]
    Compatible,
    /// Copy attachments without converting; preserves quality but may not display correctly in all browsers
    Efficient,
}

impl AttachmentManager {
    /// Create an instance of the enum given user input
    pub fn from_cli(copy_state: &str) -> Option<Self> {
        match copy_state.to_lowercase().as_str() {
            "compatible" => Some(Self::Compatible),
            "efficient" => Some(Self::Efficient),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }

    /// Handle an attachment, copying and converting if requested
    ///
    /// If copied, update attachment's `copied_path`
    pub fn handle_attachment<'a>(
        &'a self,
        message: &Message,
        attachment: &'a mut Attachment,
        config: &Config,
    ) -> Option<()> {
        // Resolve the path to the attachment
        let attachment_path = attachment
            .resolved_attachment_path(&config.options.platform, &config.options.db_path)?;

        if !matches!(self, AttachmentManager::Disabled) {
            let from = Path::new(&attachment_path);

            // Ensure the file exists at the specified location
            if !from.exists() {
                eprintln!("Attachment not found at specified path: {from:?}");
                return None;
            }

            // Create a path to copy the file to
            let mut to = config.attachment_path();

            // Add the subdirectory
            let sub_dir = config.conversation_attachment_path(message.chat_id);
            to.push(sub_dir);

            // Add a random filename
            to.push(Uuid::new_v4().to_string());

            // Set the new file's extension to the original one
            to.set_extension(attachment.extension()?);

            match self {
                AttachmentManager::Compatible => match &config.converter {
                    Some(converter) => {
                        Self::copy_convert(from, &mut to, converter);
                    }
                    None => Self::copy_raw(from, &to),
                },
                AttachmentManager::Efficient => Self::copy_raw(from, &to),
                AttachmentManager::Disabled => unreachable!(),
            };

            // Update file metadata
            if let Ok(metadata) = metadata(from) {
                let mtime = match &message.date(&config.offset) {
                    Ok(date) => {
                        FileTime::from_unix_time(date.timestamp(), date.timestamp_subsec_nanos())
                    }
                    Err(_) => FileTime::from_last_modification_time(&metadata),
                };

                let atime = FileTime::from_last_access_time(&metadata);

                if let Err(why) = set_file_times(&to, atime, mtime) {
                    eprintln!("Unable to update {to:?} metadata: {why}")
                }
            }
            attachment.copied_path = Some(to);
        }
        Some(())
    }

    /// Copy a file without altering it
    fn copy_raw(from: &Path, to: &Path) {
        // Ensure the directory tree exists
        if let Some(folder) = to.parent() {
            if !folder.exists() {
                if let Err(why) = create_dir_all(folder) {
                    eprintln!("Unable to create {folder:?}: {why}");
                }
            }
        }
        if let Err(why) = copy(from, to) {
            eprintln!("Unable to copy {from:?} to {to:?}: {why}")
        };
    }

    /// Copy a file, converting if possible
    fn copy_convert(from: &Path, to: &mut PathBuf, converter: &Converter) {
        let ext = from.extension().unwrap_or_default();
        if ext == "heic" || ext == "HEIC" {
            // Update extension for conversion
            to.set_extension("jpg");
            if heic_to_jpeg(from, to, converter).is_none() {
                eprintln!("Unable to convert {from:?}")
            }
        } else {
            Self::copy_raw(from, to);
        }
    }
}

impl Default for AttachmentManager {
    fn default() -> Self {
        Self::Disabled
    }
}

impl Display for AttachmentManager {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachmentManager::Disabled => write!(fmt, "disabled"),
            AttachmentManager::Compatible => write!(fmt, "compatible"),
            AttachmentManager::Efficient => write!(fmt, "efficient"),
        }
    }
}
