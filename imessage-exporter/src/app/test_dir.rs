/*!
 Per-test scratch directories under [`std::env::temp_dir`].

 Many tests build a fake [`Options`](crate::app::options::Options) and then
 construct an exporter, which opens an `orphaned.<ext>` file under
 `export_path`. A shared path (such as `/tmp`) collides between parallel
 test processes and leaks state across runs, so every caller gets a fresh,
 uniquely-named directory instead.
*/

use std::{
    env::temp_dir,
    fs::{create_dir_all, read_dir, remove_dir_all},
    path::PathBuf,
    process,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Prefix shared by every directory this module creates, so the sweep can
/// recognize its own entries without touching unrelated temp files.
const PREFIX: &str = "imessage-exporter-test-";

/// Sweep entries older than this on first call per process.
///
/// Directory names carry creation time because tests may change filesystem
/// timestamps beneath them.
const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

/// Build a fresh, uniquely-named directory under [`temp_dir`] and return its
/// path. `label` is a human-readable suffix that helps when manually
/// inspecting leftover entries.
pub fn unique_test_dir(label: &str) -> PathBuf {
    static SWEEP: OnceLock<()> = OnceLock::new();
    SWEEP.get_or_init(sweep_stale_test_dirs);

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = temp_dir().join(format!(
        "{PREFIX}{}-{nanos}-{counter}-{label}",
        process::id(),
    ));
    create_dir_all(&path).expect("create unique test dir");
    path
}

/// The creation time [`unique_test_dir`] recorded in a directory name, or
/// [`None`] when the name carries no parseable one.
///
/// The name is `{PREFIX}{pid}-{nanos}-{counter}-{label}`, so the second
/// hyphen-separated field is Unix-epoch nanoseconds at creation. `label` may
/// itself contain hyphens, which is why the split is bounded.
fn creation_time_from_name(name: &str) -> Option<SystemTime> {
    let mut fields = name.strip_prefix(PREFIX)?.splitn(3, '-');
    fields.next()?;
    let nanos: u128 = fields.next()?.parse().ok()?;
    let seconds = u64::try_from(nanos / 1_000_000_000).ok()?;
    let subsec = u32::try_from(nanos % 1_000_000_000).ok()?;
    UNIX_EPOCH.checked_add(Duration::new(seconds, subsec))
}

/// Remove leftover [`PREFIX`]-tagged directories created more than
/// [`STALE_AFTER`] ago, dating each one from its name.
fn sweep_stale_test_dirs() {
    let Ok(entries) = read_dir(temp_dir()) else {
        return;
    };
    let cutoff = SystemTime::now()
        .checked_sub(STALE_AFTER)
        .unwrap_or(UNIX_EPOCH);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        // Require both the module prefix and its parseable timestamp before
        // removing an entry from the shared temp directory.
        let Some(created) = creation_time_from_name(name) else {
            continue;
        };
        if created < cutoff {
            let _ = remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::metadata;

    use super::*;
    use crate::app::file_times::set_file_times;

    /// The timestamp in the name controls age even when mtime predates the
    /// stale cutoff.
    #[test]
    #[cfg(unix)]
    fn can_keep_backdated_directory_with_recent_name() {
        let dir = unique_test_dir("sweep-backdated");
        set_file_times(
            &dir,
            None,
            Some(UNIX_EPOCH + Duration::from_secs(1_000_000_000)),
            None,
        );
        let modified = metadata(&dir).unwrap().modified().unwrap();
        assert!(
            modified < SystemTime::now() - STALE_AFTER,
            "precondition: the directory must look stale by mtime"
        );

        sweep_stale_test_dirs();

        assert!(
            dir.is_dir(),
            "sweep deleted a directory created seconds ago"
        );
    }

    /// An epoch-old timestamp in the name is stale even when mtime is recent.
    #[test]
    fn can_sweep_directory_with_old_name() {
        let path = temp_dir().join(format!("{PREFIX}{}-1-0-sweep-old", process::id()));
        create_dir_all(&path).unwrap();

        sweep_stale_test_dirs();

        assert!(!path.exists(), "sweep kept a directory named as epoch-old");
    }

    #[test]
    fn cannot_sweep_directory_without_a_parseable_name() {
        let path = temp_dir().join(format!("{PREFIX}{}-undatable", process::id()));
        create_dir_all(&path).unwrap();

        sweep_stale_test_dirs();

        assert!(path.is_dir(), "sweep deleted a directory it cannot date");
        // Undated entries are deliberately retained, so remove the fixture.
        remove_dir_all(&path).unwrap();
    }

    #[test]
    fn can_read_creation_time_from_name() {
        let dir = unique_test_dir("name-time");
        let name = dir.file_name().unwrap().to_str().unwrap();
        let created = creation_time_from_name(name).expect("name carries a timestamp");

        // The encoded creation time falls within the live window.
        assert!(created > SystemTime::now() - STALE_AFTER);
        assert!(creation_time_from_name("unrelated-temp-entry").is_none());
    }

    #[test]
    fn unique_test_dir_is_unique_per_call() {
        let a = unique_test_dir("uniq-a");
        let b = unique_test_dir("uniq-b");
        assert_ne!(a, b);
        assert!(a.is_dir());
        assert!(b.is_dir());
    }

    #[test]
    fn unique_test_dir_uses_expected_prefix() {
        let dir = unique_test_dir("prefix-check");
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with(PREFIX), "actual name: {name}");
        assert!(name.ends_with("prefix-check"), "actual name: {name}");
    }
}
