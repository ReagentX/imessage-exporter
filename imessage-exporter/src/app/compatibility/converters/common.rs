/*!
 Shared file and process helpers for attachment converters.
*/
use std::{
    ffi::OsStr,
    fs::{copy, create_dir_all, metadata, read_dir},
    path::Path,
    process::{Command, Stdio},
};

use imessage_database::tables::messages::Message;

use crate::app::{
    file_times::{set_file_times, unix_to_system_time},
    runtime::Config,
};

/// Run a command, ignoring output. Returns [`None`] if the process cannot be
/// spawned, cannot be waited on, or exits with a non-zero status.
pub(super) fn run_command<I, S>(command: &str, args: I) -> Option<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    match Command::new(command)
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

/// Copy a file or directory without altering it.
pub(crate) fn copy_raw(from: &Path, to: &Path) {
    if from.is_dir() {
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

/// Update an attachment output's access and modification times.
///
/// Access time comes from `from`. Modification time comes from the message
/// date, with `from`'s modification time as a fallback. With
/// `use_message_times` set, creation time matches modification time. Directory
/// targets and metadata errors leave `to` unchanged.
pub(crate) fn update_file_metadata(from: &Path, to: &Path, message: &Message, config: &Config) {
    if to.is_dir() {
        return;
    }

    // Update file metadata
    if let Ok(metadata) = metadata(from) {
        // Prefer the message date for mtime, then fall back to the source file.
        let mtime = match message.date(config.offset) {
            Ok(date) => unix_to_system_time(date.timestamp(), date.timestamp_subsec_nanos())
                .or_else(|| metadata.modified().ok()),
            Err(_) => metadata.modified().ok(),
        };

        // The new last access time comes from the metadata of the original file
        let atime = metadata.accessed().ok();

        // Apply both values or leave the destination unchanged.
        if let (Some(atime), Some(mtime)) = (atime, mtime) {
            // Modification and access time always apply; the option also sets
            // creation time.
            let ctime = config.options.use_message_times.then_some(mtime);
            set_file_times(to, ctime, Some(mtime), Some(atime));
        }
    }
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use std::{
        fs::{File, metadata},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use imessage_database::tables::messages::Message;

    use crate::app::{
        export_type::ExportType,
        file_times::{set_file_times, unix_to_system_time},
        options::Options,
        runtime::Config,
        test_dir::unique_test_dir,
    };

    use super::update_file_metadata;

    /// Whole Unix seconds so assertions cannot trip over a filesystem that
    /// truncates sub-second precision.
    const MESSAGE_UNIX_SECS: i64 = 1_500_000_000;

    fn expected_message_time(config: &Config, message: &Message) -> SystemTime {
        let date = message.date(config.offset).unwrap();
        unix_to_system_time(date.timestamp(), date.timestamp_subsec_nanos()).unwrap()
    }

    #[test]
    fn can_stamp_attachment_with_message_times() {
        let mut options = Options::fake_options(ExportType::Txt);
        options.use_message_times = true;
        let config = Config::fake_app(options);

        let dir = unique_test_dir("attachment-times-on");
        let from = dir.join("src.bin");
        let to = dir.join("dest.bin");
        File::create(&from).unwrap();
        File::create(&to).unwrap();

        let mut message = Config::fake_message();
        message.date = MESSAGE_UNIX_SECS - config.offset;

        update_file_metadata(&from, &to, &message, &config);

        let expected = expected_message_time(&config, &message);
        let metadata = metadata(&to).unwrap();
        assert_eq!(metadata.modified().unwrap(), expected);

        // Birth time is settable only on macOS and Windows.
        #[cfg(any(target_vendor = "apple", windows))]
        assert_eq!(metadata.created().unwrap(), expected);
    }

    #[test]
    fn cannot_stamp_attachment_created_without_message_times() {
        let options = Options::fake_options(ExportType::Txt);
        assert!(!options.use_message_times);
        let config = Config::fake_app(options);

        let dir = unique_test_dir("attachment-times-off");
        let from = dir.join("src.bin");
        let to = dir.join("dest.bin");
        File::create(&from).unwrap();
        File::create(&to).unwrap();

        // A known birthtime distinct from the message date, so a regression
        // that always writes `ctime = Some(mtime)` cannot stay green.
        let prior_created = UNIX_EPOCH + Duration::from_secs(1_300_000_000);
        set_file_times(&to, Some(prior_created), None, None);

        let mut message = Config::fake_message();
        message.date = MESSAGE_UNIX_SECS - config.offset;

        update_file_metadata(&from, &to, &message, &config);

        let expected = expected_message_time(&config, &message);
        let metadata = metadata(&to).unwrap();
        assert_eq!(metadata.modified().unwrap(), expected);

        // Birth time is settable only on macOS and Windows.
        #[cfg(any(target_vendor = "apple", windows))]
        assert_eq!(metadata.created().unwrap(), prior_created);
    }
}
