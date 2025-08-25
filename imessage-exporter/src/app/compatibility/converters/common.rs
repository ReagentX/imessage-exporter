/*!
 Defines routines common across all converters.
*/
use filetime::{FileTime, set_file_times};
use std::{
    fs::{copy, create_dir_all, metadata, read_dir},
    path::Path,
    process::{Command, Stdio},
};

use imessage_database::tables::messages::Message;

use crate::app::runtime::Config;

/// Run a command, ignoring output; returning [`None`] on failure.
pub(super) fn run_command(command: &str, args: Vec<&str>) -> Option<()> {
    match Command::new(command)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(mut convert) => match convert.wait() {
            Ok(_) => Some(()),
            Err(why) => {
                eprintln!("Conversion failed: {why}");
                None
            }
        },
        Err(why) => {
            eprintln!("Conversion failed: {why}");
            None
        }
    }
}

/// Get the path details formatted for a CLI argument and ensure the directory tree exists
pub(super) fn ensure_paths<'a>(from: &'a Path, to: &'a Path) -> Option<(&'a str, &'a str)> {
    // Get the path we want to copy from
    let from_path = from.to_str()?;

    // Get the path we want to write to
    let to_path = to.to_str()?;

    // Ensure the directory tree exists
    if let Some(folder) = to.parent() {
        if !folder.exists() {
            if let Err(why) = create_dir_all(folder) {
                eprintln!("Unable to create {folder:?}: {why}");
                return None;
            }
        }
    }
    Some((from_path, to_path))
}

/// Copy a file or directory without altering it
pub(crate) fn copy_raw(from: &Path, to: &Path) {
    if from.is_dir() {
        // Ensure the directory tree exists
        if let Err(why) = create_dir_all(to) {
            eprintln!("Unable to create directory {to:?}: {why}");
            return;
        }

        // Iterate over the directory entries and copy them recursively
        match read_dir(from) {
            Ok(entries) => {
                for entry_result in entries {
                    match entry_result {
                        Ok(entry) => {
                            let from_path = entry.path();
                            let to_path = to.join(entry.file_name());
                            copy_raw(&from_path, &to_path);
                        }
                        Err(why) => {
                            eprintln!("Failed to read item in {from:?}: {why}");
                        }
                    }
                }
            }
            Err(why) => {
                eprintln!("Failed to read directory {from:?}: {why}");
            }
        }
    } else {
        // Ensure the directory tree exists
        if let Some(folder) = to.parent() {
            if !folder.exists() {
                if let Err(why) = create_dir_all(folder) {
                    eprintln!("Unable to create {folder:?}: {why}");
                    return;
                }
            }
        }

        if let Err(why) = copy(from, to) {
            eprintln!("Unable to copy {from:?} to {to:?}: {why}");
        }
    }
}

/// Update the metadata of a copied file, falling back to the original file's metadata if necessary
pub(crate) fn update_file_metadata(from: &Path, to: &Path, message: &Message, config: &Config) {
    // Update file metadata
    if let Ok(metadata) = metadata(from) {
        // The modification time is the message's date, otherwise the the original file's creation time
        let mtime = match message.date(&config.offset) {
            Ok(date) => FileTime::from_unix_time(date.timestamp(), date.timestamp_subsec_nanos()),
            Err(_) => FileTime::from_last_modification_time(&metadata),
        };

        // The new last access time comes from the metadata of the original file
        let atime = FileTime::from_last_access_time(&metadata);

        if let Err(why) = set_file_times(to, atime, mtime) {
            eprintln!("Unable to update {to:?} metadata: {why}");
        }
    }
}
