/*!
 Defines routines common across all converters.
*/
use std::{
    ffi::OsStr,
    fs::{File, FileTimes, copy, create_dir_all, metadata, read_dir},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use imessage_database::tables::messages::Message;

use crate::app::{compatibility::models::resolve_program, runtime::Config};

#[cfg(target_family = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn command_no_window(command: &str) -> Option<Command> {
    #[cfg(target_family = "windows")]
    let program = resolve_program(command)?;

    #[cfg(not(target_family = "windows"))]
    let program = resolve_program(command).unwrap_or_else(|| command.into());

    #[cfg(target_family = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new(program);
        command.creation_flags(CREATE_NO_WINDOW);
        Some(command)
    }
    #[cfg(not(target_family = "windows"))]
    Some(Command::new(program))
}

/// Run a command, ignoring output. Returns [`None`] if the process cannot be
/// spawned, cannot be waited on, or exits with a non-zero status.
pub(super) fn run_command<I, S>(command: &str, args: I) -> Option<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let Some(mut child) = command_no_window(command) else {
        eprintln!("Conversion failed: bundled converter `{command}` was not found");
        return None;
    };

    match child
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(mut convert) => match convert.wait() {
            Ok(status) if status.success() => Some(()),
            Ok(status) => {
                eprintln!("Conversion failed: {command} exited with {status}");
                None
            }
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

/// Ensure the parent directory of `to` exists, creating it if necessary.
pub(super) fn ensure_output_dir(to: &Path) -> Option<()> {
    if let Some(folder) = to.parent()
        && !folder.exists()
        && let Err(why) = create_dir_all(folder)
    {
        eprintln!("Unable to create {}: {why}", folder.display());
        return None;
    }
    Some(())
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
            // On Unix, `set_times` uses `futimens`, which does not require the file
            // descriptor to have write access. On Windows, `SetFileTime` requires
            // `FILE_WRITE_ATTRIBUTES`, so the file must be opened with write access.
            #[cfg(unix)]
            let file_result = File::open(to);
            #[cfg(not(unix))]
            let file_result = File::options().write(true).open(to);
            match file_result {
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
