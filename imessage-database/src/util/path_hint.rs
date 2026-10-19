/*!
 Diagnose why a database path cannot be opened by inspecting the filesystem around it.
*/

use std::{
    ffi::OsString,
    fmt::{Display, Formatter, Result as FmtResult},
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use crate::{
    tables::table::DEFAULT_PATH_IOS,
    util::dirs::{default_backup_roots, default_db_path},
};

/// Host OS name for diagnostics.
const OS_NAME: &str = if cfg!(target_os = "windows") {
    "Windows"
} else if cfg!(target_os = "linux") {
    "Linux"
} else {
    std::env::consts::OS
};

/// Backup file index, present in every backup made since iOS 10, encrypted or not.
const MANIFEST_DB: &str = "Manifest.db";
/// Backup metadata written beside `Manifest.db`.
const MANIFEST_PLIST: &str = "Manifest.plist";
/// Device metadata written at the backup root.
const INFO_PLIST: &str = "Info.plist";
/// Subdirectory for per-device backups from Finder, iTunes, and Apple Devices.
const BACKUP_DIR: &str = "Backup";
/// macOS Messages database file name.
const MACOS_DB: &str = "chat.db";

/// Quote characters to recognize in path arguments, including literal single quotes from
/// `cmd.exe` and curly quotes from word processors.
const SHELL_QUOTES: [char; 6] = ['"', '\'', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}'];

/// Filesystem state of a path.
#[derive(Debug, PartialEq, Eq)]
pub enum Presence {
    /// No entry at the path
    Missing,
    /// A regular file
    File,
    /// A directory
    Directory,
    /// Neither a regular file nor a directory
    Other,
}

/// Distinguish a missing path from a filesystem access error.
///
/// With `Path::exists()`, you receive `false` for both a missing path and an unreadable path,
/// including a backup under `~/Library` without Full Disk Access. Return
/// [`Presence::Missing`] only for `NotFound` and `NotADirectory`; propagate all other errors.
pub fn presence(path: &Path) -> io::Result<Presence> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Presence::File),
        Ok(metadata) if metadata.is_dir() => Ok(Presence::Directory),
        Ok(_) => Ok(Presence::Other),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            Ok(Presence::Missing)
        }
        Err(error) => Err(error),
    }
}

/// Diagnostic hints inferred from the filesystem around a failed path.
#[derive(Debug, PartialEq, Eq)]
pub enum PathHint {
    /// Literal quote character at either end of the path
    QuotedPath(char),
    /// Default macOS database path on a host without a Messages app
    NoLocalDatabase,
    /// iOS backup directories found at these paths
    BackupsFound(Vec<PathBuf>),
    /// macOS Messages database found at this path
    DatabaseFound(PathBuf),
    /// iOS backup root without a Messages database
    BackupWithoutMessages(PathBuf),
    /// Messages database at the backup root: the flat layout used before iOS 10
    LegacyBackupLayout(PathBuf),
    /// Nearest existing ancestor and the first missing component beneath it
    MissingComponent {
        /// Deepest existing path prefix
        ancestor: PathBuf,
        /// First missing entry below `ancestor`
        component: OsString,
    },
}

impl Display for PathHint {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        match self {
            PathHint::QuotedPath(quote) => write!(
                fmt,
                "The path contains the quote character `{quote}`; the shell passed it through as part of the path. Wrap the path in straight double quotes (\") instead"
            ),
            PathHint::NoLocalDatabase => write!(
                fmt,
                "{OS_NAME} has no local Messages database; pass the path to an iOS backup directory or a copy of a macOS `chat.db` instead"
            ),
            PathHint::BackupsFound(paths) => {
                write!(fmt, "iOS backups found; pass the path to one of them:")?;
                for path in paths {
                    write!(fmt, "\n  {}", path.display())?;
                }
                Ok(())
            }
            PathHint::DatabaseFound(path) => write!(
                fmt,
                "This directory contains a Messages database; pass the path to the file: {}",
                path.display()
            ),
            PathHint::BackupWithoutMessages(path) => write!(
                fmt,
                "`{}` is an iOS backup, but it contains no Messages database; ensure the backup completed successfully",
                path.display()
            ),
            PathHint::LegacyBackupLayout(path) => write!(
                fmt,
                "`{}` stores its files at the root, the layout of backups made before iOS 10; backups older than iOS 13 are not supported",
                path.display()
            ),
            PathHint::MissingComponent {
                ancestor,
                component,
            } => write!(
                fmt,
                "`{}` exists, but it does not contain `{}`",
                ancestor.display(),
                component.display()
            ),
        }
    }
}

/// Diagnose a failed database or backup path from the surrounding filesystem.
///
/// Inspect the nearest existing ancestor, then check the host's default backup locations.
/// Stop at the first inaccessible ancestor; do not infer contents you cannot inspect.
#[must_use]
pub fn diagnose(path: &Path) -> Vec<PathHint> {
    let mut hints = Vec::new();
    if !cfg!(target_os = "macos") && default_db_path().is_some_and(|default| default == path) {
        hints.push(PathHint::NoLocalDatabase);
    }

    // Report the quote itself instead of treating it as part of a missing path component.
    if let Some(quote) = shell_quote(path) {
        hints.push(PathHint::QuotedPath(quote));
    } else if let Some((ancestor, missing)) = nearest_existing(path) {
        match classify_directory(&ancestor) {
            // Stop after finding a database or backup nearby; skip unrelated backups elsewhere.
            Some(hint @ (PathHint::BackupsFound(_) | PathHint::DatabaseFound(_))) => {
                hints.push(hint);
                return hints;
            }
            Some(hint) => hints.push(hint),
            None => {
                if let Some(component) = missing {
                    hints.push(PathHint::MissingComponent {
                        ancestor,
                        component,
                    });
                }
            }
        }
    }

    let on_host: Vec<PathBuf> = default_backup_roots()
        .iter()
        .flat_map(|root| list_backups(root))
        .collect();
    if !on_host.is_empty() {
        hints.push(PathHint::BackupsFound(on_host));
    }
    hints
}

/// Walk up to the nearest existing ancestor and name the first missing component beneath it.
///
/// For a relative path, use the current directory after reaching an empty parent.
/// Stop with `None` if an ancestor cannot be inspected.
fn nearest_existing(path: &Path) -> Option<(PathBuf, Option<OsString>)> {
    let mut missing = None;
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            return Some((std::env::current_dir().ok()?, missing));
        }
        match presence(ancestor) {
            Ok(Presence::Missing) => missing = ancestor.file_name().map(OsString::from),
            Ok(_) => return Some((ancestor.to_path_buf(), missing)),
            Err(_) => return None,
        }
    }
    None
}

/// Return the quote character at either end of the path, if any.
fn shell_quote(path: &Path) -> Option<char> {
    let text = path.to_string_lossy();
    [text.chars().next(), text.chars().last()]
        .into_iter()
        .flatten()
        .find(|end| SHELL_QUOTES.contains(end))
}

/// Identify an existing directory from its Apple backup markers and database files.
fn classify_directory(dir: &Path) -> Option<PathHint> {
    if dir.join(DEFAULT_PATH_IOS).is_file() {
        return None;
    }
    // Require a backup marker before reporting the pre-iOS 10 flat layout: in a modern
    // backup, you can find the same Messages filename inside its hash subdirectory.
    if has_backup_markers(dir) {
        let flat_messages_db = Path::new(DEFAULT_PATH_IOS).file_name()?;
        if dir.join(flat_messages_db).is_file() {
            return Some(PathHint::LegacyBackupLayout(dir.to_path_buf()));
        }
        return Some(PathHint::BackupWithoutMessages(dir.to_path_buf()));
    }
    let database = dir.join(MACOS_DB);
    if database.is_file() {
        return Some(PathHint::DatabaseFound(database));
    }
    [dir.to_path_buf(), dir.join(BACKUP_DIR)]
        .iter()
        .map(|candidate| list_backups(candidate))
        .find(|backups| !backups.is_empty())
        .map(PathHint::BackupsFound)
}

/// List immediate subdirectories of `dir` with `Manifest.db`, `Manifest.plist`, or `Info.plist`.
/// Return an empty list if `dir` cannot be read.
#[must_use]
pub fn list_backups(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut backups: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && has_backup_markers(path))
        .collect();
    backups.sort();
    backups
}

/// Look for metadata files at the backup root: `Info.plist` and `Manifest.plist` in every
/// layout, plus `Manifest.db` since iOS 10.
fn has_backup_markers(dir: &Path) -> bool {
    [MANIFEST_DB, MANIFEST_PLIST, INFO_PLIST]
        .iter()
        .any(|marker| dir.join(marker).is_file())
}

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        fs::{File, create_dir_all, remove_dir_all},
        path::{Path, PathBuf},
        process,
    };

    use crate::{
        tables::table::DEFAULT_PATH_IOS,
        util::path_hint::{PathHint, Presence, diagnose, list_backups, presence},
    };

    /// Create a fresh directory per test, with the process ID to separate parallel test binaries.
    fn scratch(label: &str) -> PathBuf {
        let path = temp_dir().join(format!(
            "imessage-database-path-hint-{}-{label}",
            process::id()
        ));
        let _ = remove_dir_all(&path);
        create_dir_all(&path).unwrap();
        path
    }

    fn touch(path: &Path) {
        create_dir_all(path.parent().unwrap()).unwrap();
        File::create(path).unwrap();
    }

    #[test]
    fn presence_distinguishes_missing_file_and_directory() {
        let dir = scratch("presence");
        let file = dir.join("file");
        touch(&file);

        assert_eq!(presence(&dir).unwrap(), Presence::Directory);
        assert_eq!(presence(&file).unwrap(), Presence::File);
        assert_eq!(presence(&dir.join("absent")).unwrap(), Presence::Missing);
        assert_eq!(
            presence(&file.join("below-a-file")).unwrap(),
            Presence::Missing
        );

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lists_backups_under_parent() {
        let dir = scratch("parent");
        touch(&dir.join("A/Manifest.db"));
        touch(&dir.join("B/Info.plist"));
        create_dir_all(dir.join("C")).unwrap();
        touch(&dir.join("D"));

        assert_eq!(list_backups(&dir), vec![dir.join("A"), dir.join("B")]);
        assert_eq!(
            diagnose(&dir),
            vec![PathHint::BackupsFound(vec![dir.join("A"), dir.join("B")])]
        );

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn descends_into_backup_subdirectory() {
        let dir = scratch("mobilesync");
        touch(&dir.join("Backup/A/Manifest.plist"));

        assert_eq!(
            diagnose(&dir),
            vec![PathHint::BackupsFound(vec![dir.join("Backup/A")])]
        );

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backup_root_without_messages_database() {
        let dir = scratch("no-messages");
        touch(&dir.join("Manifest.db"));

        assert_eq!(
            diagnose(&dir.join(DEFAULT_PATH_IOS)).first(),
            Some(&PathHint::BackupWithoutMessages(dir.clone()))
        );

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn legacy_flat_backup_layout() {
        let dir = scratch("legacy");
        touch(&dir.join(Path::new(DEFAULT_PATH_IOS).file_name().unwrap()));
        touch(&dir.join("Manifest.plist"));

        assert_eq!(
            diagnose(&dir).first(),
            Some(&PathHint::LegacyBackupLayout(dir.clone()))
        );
        assert_eq!(
            diagnose(&dir.join(DEFAULT_PATH_IOS)).first(),
            Some(&PathHint::LegacyBackupLayout(dir.clone()))
        );

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn directory_holding_macos_database() {
        let dir = scratch("messages-dir");
        touch(&dir.join("chat.db"));

        assert_eq!(
            diagnose(&dir).first(),
            Some(&PathHint::DatabaseFound(dir.join("chat.db")))
        );

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn names_first_missing_component() {
        let dir = scratch("typo");
        create_dir_all(dir.join("Library")).unwrap();

        assert_eq!(
            diagnose(&dir.join("Library/ApplicationSupport/MobileSync/Backup/X")).first(),
            Some(&PathHint::MissingComponent {
                ancestor: dir.join("Library"),
                component: "ApplicationSupport".into(),
            })
        );

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn quoted_path_is_called_out() {
        let dir = scratch("quoted");

        let quoted = format!("\u{201C}{}\u{201D}", dir.join("backup").display());
        let hints = diagnose(Path::new(&quoted));
        assert_eq!(hints.first(), Some(&PathHint::QuotedPath('\u{201C}')));

        let hints = diagnose(&dir.join("MobileSync'"));
        assert_eq!(hints.first(), Some(&PathHint::QuotedPath('\'')));

        assert!(
            !diagnose(&dir.join("Chris's backup"))
                .iter()
                .any(|hint| matches!(hint, PathHint::QuotedPath(_)))
        );

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn relative_path_resolves_against_current_dir() {
        let hints = diagnose(Path::new("NoSuchBackup/chat.db"));

        assert_eq!(
            hints.first(),
            Some(&PathHint::MissingComponent {
                ancestor: std::env::current_dir().unwrap(),
                component: "NoSuchBackup".into(),
            })
        );
    }

    #[test]
    fn valid_backup_root_is_not_classified() {
        let dir = scratch("valid-root");
        touch(&dir.join(DEFAULT_PATH_IOS));
        touch(&dir.join("Manifest.db"));

        assert_eq!(
            diagnose(&dir.join("3d/other")).first(),
            Some(&PathHint::MissingComponent {
                ancestor: dir.join("3d"),
                component: "other".into(),
            })
        );

        remove_dir_all(&dir).unwrap();
    }
}
