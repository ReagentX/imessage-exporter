/*!
 Defines routines common across all converters.
*/
use std::{
    fs::{FileTimes, OpenOptions, copy, create_dir_all, metadata, read_dir},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
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
    if let Some(folder) = to.parent()
        && !folder.exists()
        && let Err(why) = create_dir_all(folder)
    {
        eprintln!("Unable to create {}: {why}", folder.display());
        return None;
    }
    Some((from_path, to_path))
}

/// Copy a file or directory without altering it
pub(crate) fn copy_raw(from: &Path, to: &Path) {
    if from.is_dir() {
        // Ensure the directory tree exists
        if let Err(why) = create_dir_all(to) {
            eprintln!("Unable to create directory {}: {why}", to.display());
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
                            eprintln!("Failed to read item in {}: {why}", from.display());
                        }
                    }
                }
            }
            Err(why) => {
                eprintln!("Failed to read directory {}: {why}", from.display());
            }
        }
    } else {
        // Ensure the directory tree exists
        if let Some(folder) = to.parent()
            && !folder.exists()
            && let Err(why) = create_dir_all(folder)
        {
            eprintln!("Unable to create {}: {why}", folder.display());
            return;
        }

        if let Err(why) = copy(from, to) {
            eprintln!(
                "Unable to copy {} to {}: {why}",
                from.display(),
                to.display()
            );
        }
    }
}

/// Update the metadata of a copied file, falling back to the original file's metadata if necessary
pub(crate) fn update_file_metadata(from: &Path, to: &Path, message: &Message, config: &Config) {
    if to.is_dir() {
        return;
    }

    // Update file metadata
    if let Ok(metadata) = metadata(from) {
        // The modification time is the message's date, otherwise the original file's modification time
        let mtime = match message.date(config.offset) {
            Ok(date) => unix_to_system_time(date.timestamp(), date.timestamp_subsec_nanos())
                .or_else(|| metadata.modified().ok()),
            Err(_) => metadata.modified().ok(),
        };

        // The new last access time comes from the metadata of the original file
        let atime = metadata.accessed().ok();

        if let (Some(atime), Some(mtime)) = (atime, mtime) {
            match OpenOptions::new().write(true).open(to) {
                Ok(file) => {
                    let file_times = FileTimes::new().set_accessed(atime).set_modified(mtime);
                    if let Err(why) = file.set_times(file_times) {
                        eprintln!("Unable to update {} metadata: {why}", to.display());
                    }
                }
                Err(why) => {
                    eprintln!("Unable to open {} to update metadata: {why}", to.display());
                }
            }
        }
    }
}

fn unix_to_system_time(secs: i64, nanos: u32) -> Option<SystemTime> {
    if secs >= 0 {
        UNIX_EPOCH
            .checked_add(Duration::from_secs(secs as u64))?
            .checked_add(Duration::from_nanos(u64::from(nanos)))
    } else {
        UNIX_EPOCH
            .checked_sub(Duration::from_secs(secs.unsigned_abs()))?
            .checked_add(Duration::from_nanos(u64::from(nanos)))
    }
}
