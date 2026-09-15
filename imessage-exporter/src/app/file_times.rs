/*!
 Apply filesystem timestamps.

 Opening or updating a path reports failures to stderr without propagating
 them.
*/

use std::{
    fs::{File, FileTimes},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Apply the supplied timestamps to `path`, leaving the rest untouched.
///
/// Call-site details:
/// - Nothing is fatal. Any failure to open `path` or to write its times prints
///   to stderr and returns.
/// - `created` is the birthtime, which only macOS and Windows can set; other
///   platforms ignore it. POSIX `ctime` is a different field (inode
///   status-change time) and cannot be set through this interface.
/// - Directory paths are supported on Unix. They follow the non-fatal error
///   path on Windows because opening a directory requires backup semantics.
pub(crate) fn set_file_times(
    path: &Path,
    created: Option<SystemTime>,
    modified: Option<SystemTime>,
    accessed: Option<SystemTime>,
) {
    if created.is_none() && modified.is_none() && accessed.is_none() {
        return;
    }

    // On Unix, `set_times` uses `futimens`, which does not require the file
    // descriptor to have write access. On Windows, `SetFileTime` requires
    // `FILE_WRITE_ATTRIBUTES`, so the file must be opened with write access.
    #[cfg(unix)]
    let file_result = File::open(path);
    #[cfg(not(unix))]
    let file_result = File::options().write(true).open(path);

    let file = match file_result {
        Ok(file) => file,
        Err(why) => {
            eprintln!(
                "Unable to open {} to update metadata: {why}",
                path.display()
            );
            return;
        }
    };

    let mut times = FileTimes::new();
    if let Some(accessed) = accessed {
        times = times.set_accessed(accessed);
    }
    if let Some(modified) = modified {
        times = times.set_modified(modified);
    }
    times = with_created(times, created);

    if let Err(why) = file.set_times(times) {
        eprintln!("Unable to update {} metadata: {why}", path.display());
    }
}

/// Add the birthtime to `times` on the platforms that can set one.
#[cfg(target_vendor = "apple")]
fn with_created(times: FileTimes, created: Option<SystemTime>) -> FileTimes {
    use std::os::darwin::fs::FileTimesExt;

    match created {
        Some(created) => times.set_created(created),
        None => times,
    }
}

/// Add the birthtime to `times` on the platforms that can set one.
#[cfg(windows)]
fn with_created(times: FileTimes, created: Option<SystemTime>) -> FileTimes {
    use std::os::windows::fs::FileTimesExt;

    match created {
        Some(created) => times.set_created(created),
        None => times,
    }
}

/// Drop the birthtime: no other platform exposes a settable one.
#[cfg(not(any(target_vendor = "apple", windows)))]
fn with_created(times: FileTimes, _created: Option<SystemTime>) -> FileTimes {
    times
}

/// Convert Unix seconds plus nanoseconds to a [`SystemTime`].
///
/// Negative seconds move before the epoch; nanoseconds remain a positive
/// offset from that whole-second boundary. Returns [`None`] on overflow.
pub(crate) fn unix_to_system_time(secs: i64, nanos: u32) -> Option<SystemTime> {
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

#[cfg(test)]
mod tests {
    use std::{
        fs::{File, metadata},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use crate::app::{
        file_times::{set_file_times, unix_to_system_time},
        test_dir::unique_test_dir,
    };

    /// Whole seconds, so the assertions cannot trip over a filesystem that
    /// truncates sub-second precision.
    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn can_set_modified_time_on_file() {
        let dir = unique_test_dir("file-times-modified");
        let path = dir.join("stamped.txt");
        File::create(&path).unwrap();

        let modified = at(1_000_000_000);
        set_file_times(&path, None, Some(modified), None);

        assert_eq!(metadata(&path).unwrap().modified().unwrap(), modified);
    }

    #[test]
    fn can_set_accessed_time_on_file() {
        let dir = unique_test_dir("file-times-accessed");
        let path = dir.join("stamped.txt");
        File::create(&path).unwrap();

        let accessed = at(1_100_000_000);
        set_file_times(&path, None, None, Some(accessed));

        assert_eq!(metadata(&path).unwrap().accessed().unwrap(), accessed);
    }

    #[test]
    #[cfg(unix)]
    fn can_set_times_on_directory() {
        use std::fs::create_dir;

        let dir = unique_test_dir("file-times-directory");
        let path = dir.join("subdir");
        create_dir(&path).unwrap();

        let modified = at(1_200_000_000);
        set_file_times(&path, None, Some(modified), None);

        assert_eq!(metadata(&path).unwrap().modified().unwrap(), modified);
    }

    /// Birth time is settable only on macOS and Windows.
    #[test]
    #[cfg(any(target_vendor = "apple", windows))]
    fn can_set_created_time() {
        let dir = unique_test_dir("file-times-created");
        let path = dir.join("stamped.txt");
        File::create(&path).unwrap();

        let created = at(1_300_000_000);
        let modified = at(1_400_000_000);
        set_file_times(&path, Some(created), Some(modified), None);

        let metadata = metadata(&path).unwrap();
        assert_eq!(metadata.created().unwrap(), created);
        assert_eq!(metadata.modified().unwrap(), modified);
    }

    #[test]
    fn missing_path_does_not_panic() {
        let dir = unique_test_dir("file-times-missing");
        let path = dir.join("absent.txt");

        set_file_times(&path, Some(at(1)), Some(at(2)), Some(at(3)));

        assert!(!path.exists());
    }

    #[test]
    fn empty_request_does_not_panic() {
        let dir = unique_test_dir("file-times-empty");
        let path = dir.join("stamped.txt");
        File::create(&path).unwrap();

        set_file_times(&path, None, None, None);
    }

    #[test]
    fn unix_to_system_time_handles_negative_seconds() {
        let expected = UNIX_EPOCH - Duration::from_secs(400_000_000);
        assert_eq!(unix_to_system_time(-400_000_000, 0), Some(expected));
    }

    #[test]
    fn unix_to_system_time_handles_positive_seconds() {
        let expected = UNIX_EPOCH + Duration::from_secs(1_500_000_000) + Duration::from_nanos(250);
        assert_eq!(unix_to_system_time(1_500_000_000, 250), Some(expected));
    }
}
