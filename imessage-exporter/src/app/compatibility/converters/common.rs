/*!
 Shared file and process helpers for attachment converters.
*/
use std::{
    ffi::OsStr,
    fs::{File, create_dir_all, metadata, read_dir},
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
pub(crate) fn copy_raw(from: &Path, to: &Path) -> std::io::Result<()> {
    let info = std::fs::symlink_metadata(from)?;
    if info.file_type().is_symlink() {
        return Err(std::io::Error::other("Refusing a symlink attachment"));
    }
    if info.is_dir() {
        create_dir_all(to)?;
        for entry in read_dir(from)? {
            let entry = entry?;
            copy_raw(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else if info.is_file() {
        if let Some(parent) = to.parent() {
            create_dir_all(parent)?;
        }
        let mut source = File::open(from)?;
        let mut output = tempfile::NamedTempFile::new_in(to.parent().unwrap())?;
        std::io::copy(&mut source, &mut output)?;
        output.as_file().sync_all()?;
        output.persist(to).map_err(|e| e.error)?;
        if !files_equal(from, to)? {
            return Err(std::io::Error::other("Attachment verification failed"));
        }
    } else {
        return Err(std::io::Error::other(
            "Attachment is not a regular file or directory",
        ));
    }
    Ok(())
}

/// Compare complete content (including directory trees), not only length. Reject symlinks.
pub(crate) fn files_equal(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    let am = std::fs::symlink_metadata(a)?;
    let bm = std::fs::symlink_metadata(b)?;
    if am.is_dir() && bm.is_dir() {
        let names =
            |path: &Path| -> std::io::Result<std::collections::BTreeSet<std::ffi::OsString>> {
                read_dir(path)?
                    .map(|entry| entry.map(|entry| entry.file_name()))
                    .collect()
            };
        let left = names(a)?;
        if left != names(b)? {
            return Ok(false);
        }
        for name in left {
            if !files_equal(&a.join(&name), &b.join(name))? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    if !am.is_file() || !bm.is_file() || am.len() != bm.len() {
        return Ok(false);
    }
    let mut a = std::io::BufReader::new(File::open(a)?);
    let mut b = std::io::BufReader::new(File::open(b)?);
    let mut left = [0u8; 65536];
    let mut right = [0u8; 65536];
    let mut remaining = am.len();
    while remaining > 0 {
        let n = remaining.min(left.len() as u64) as usize;
        a.read_exact(&mut left[..n])?;
        b.read_exact(&mut right[..n])?;
        if left[..n] != right[..n] {
            return Ok(false);
        }
        remaining -= n as u64;
    }
    Ok(a.read(&mut left[..1])? == 0 && b.read(&mut right[..1])? == 0)
}

/// Reject symlinks and non-directories below the user-selected output root.
pub(crate) fn output_directory(root: &Path, dir: &Path) -> std::io::Result<()> {
    let relative = dir.strip_prefix(root).map_err(std::io::Error::other)?;
    create_dir_all(root)?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        let std::path::Component::Normal(part) = part else {
            return Err(std::io::Error::other("Invalid output path component"));
        };
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(std::io::Error::other(
                    "Output directory is not a real directory",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir(&current)?,
            Err(e) => return Err(e),
        }
    }
    Ok(())
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
        let mtime = match message.date() {
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

    use imessage_database::{tables::messages::Message, util::dates::MESSAGES_EPOCH_UNIX_SECS};

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

    fn expected_message_time(message: &Message) -> SystemTime {
        let date = message.date().unwrap();
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
        message.date = MESSAGE_UNIX_SECS - MESSAGES_EPOCH_UNIX_SECS;

        update_file_metadata(&from, &to, &message, &config);

        let expected = expected_message_time(&message);
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
        message.date = MESSAGE_UNIX_SECS - MESSAGES_EPOCH_UNIX_SECS;

        update_file_metadata(&from, &to, &message, &config);

        let expected = expected_message_time(&message);
        let metadata = metadata(&to).unwrap();
        assert_eq!(metadata.modified().unwrap(), expected);

        // Birth time is settable only on macOS and Windows.
        #[cfg(any(target_vendor = "apple", windows))]
        assert_eq!(metadata.created().unwrap(), prior_created);
    }
}
