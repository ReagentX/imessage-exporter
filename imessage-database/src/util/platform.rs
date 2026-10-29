/*!
 Platform detection for Messages database paths.
*/

use std::{fmt::Display, path::Path};

use crate::{
    error::table::{TableConnectError, TableError},
    tables::table::DEFAULT_PATH_IOS,
    util::path_hint::{Presence, diagnose, presence},
};

/// Platform that produced the Messages database.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    /// macOS Messages database.
    #[allow(non_camel_case_types)]
    macOS,
    /// iOS backup containing a Messages database.
    #[allow(non_camel_case_types)]
    iOS,
}

impl Platform {
    /// Detect whether a path points to an iOS backup root or a macOS database.
    ///
    /// Require a Messages database at [`DEFAULT_PATH_IOS`] before accepting an iOS backup root;
    /// any other directory is [`MissingBackupDatabase`](TableConnectError::MissingBackupDatabase).
    /// Return an error for a missing or inaccessible path, with hints from the surrounding
    /// filesystem when available. Treat files as macOS database inputs; validate their contents
    /// with [`get_connection`](crate::tables::table::get_connection).
    pub fn determine(db_path: &Path) -> Result<Self, TableError> {
        // iOS inputs must be backup roots, not the nested database path.
        if db_path.ends_with(DEFAULT_PATH_IOS) {
            return Err(TableError::CannotConnect(TableConnectError::NotBackupRoot));
        }

        let connect_error = match presence(db_path) {
            Err(why) => TableConnectError::Inaccessible(db_path.to_path_buf(), why),
            Ok(Presence::Missing) => {
                TableConnectError::DoesNotExist(db_path.to_path_buf(), diagnose(db_path))
            }
            Ok(Presence::Directory) => {
                let messages_db = db_path.join(DEFAULT_PATH_IOS);
                match presence(&messages_db) {
                    Ok(Presence::File) => return Ok(Self::iOS),
                    Err(why) => TableConnectError::Inaccessible(messages_db, why),
                    Ok(_) => TableConnectError::MissingBackupDatabase(
                        db_path.to_path_buf(),
                        diagnose(db_path),
                    ),
                }
            }
            Ok(Presence::File | Presence::Other) => return Ok(Self::macOS),
        };
        Err(TableError::CannotConnect(connect_error))
    }

    /// Parse a platform name from CLI input.
    #[must_use]
    pub fn from_cli(platform: &str) -> Option<Self> {
        match platform.to_lowercase().as_str() {
            "macos" => Some(Self::macOS),
            "ios" => Some(Self::iOS),
            _ => None,
        }
    }
}

impl Default for Platform {
    /// The default Platform is [`Platform::macOS`].
    fn default() -> Self {
        Self::macOS
    }
}

impl Display for Platform {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::macOS => write!(fmt, "macOS"),
            Platform::iOS => write!(fmt, "iOS"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        fs::{File, create_dir_all, remove_dir_all},
        path::PathBuf,
        process,
    };

    use crate::{
        error::table::{TableConnectError, TableError},
        tables::table::DEFAULT_PATH_IOS,
        util::platform::Platform,
    };

    fn scratch(label: &str) -> PathBuf {
        let path = temp_dir().join(format!(
            "imessage-database-platform-{}-{label}",
            process::id()
        ));
        let _ = remove_dir_all(&path);
        create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn detects_backup_root() {
        let dir = scratch("root");
        let messages_db = dir.join(DEFAULT_PATH_IOS);
        create_dir_all(messages_db.parent().unwrap()).unwrap();
        File::create(&messages_db).unwrap();

        assert_eq!(Platform::determine(&dir).unwrap(), Platform::iOS);

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_is_macos() {
        let dir = scratch("file");
        let file = dir.join("chat.db");
        File::create(&file).unwrap();

        assert_eq!(Platform::determine(&file).unwrap(), Platform::macOS);

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_path_is_an_error() {
        let dir = scratch("missing");

        assert!(matches!(
            Platform::determine(&dir.join("absent")),
            Err(TableError::CannotConnect(TableConnectError::DoesNotExist(
                _,
                _
            )))
        ));

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn directory_without_backup_is_an_error() {
        let dir = scratch("plain-dir");

        assert!(matches!(
            Platform::determine(&dir),
            Err(TableError::CannotConnect(
                TableConnectError::MissingBackupDatabase(_, _)
            ))
        ));

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backup_without_messages_database_names_the_backup() {
        let dir = scratch("backup-no-messages");
        File::create(dir.join("Manifest.db")).unwrap();

        let error = Platform::determine(&dir).unwrap_err().to_string();
        assert!(error.contains("contains no Messages database"), "{error}");
        assert!(error.contains("is an iOS backup"), "{error}");

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn can_parse_macos_any_case() {
        assert!(matches!(Platform::from_cli("macos"), Some(Platform::macOS)));
        assert!(matches!(Platform::from_cli("MACOS"), Some(Platform::macOS)));
        assert!(matches!(Platform::from_cli("MacOS"), Some(Platform::macOS)));
    }

    #[test]
    fn can_parse_ios_any_case() {
        assert!(matches!(Platform::from_cli("ios"), Some(Platform::iOS)));
        assert!(matches!(Platform::from_cli("IOS"), Some(Platform::iOS)));
        assert!(matches!(Platform::from_cli("iOS"), Some(Platform::iOS)));
    }

    #[test]
    fn cant_parse_invalid() {
        assert!(Platform::from_cli("mac").is_none());
        assert!(Platform::from_cli("iphone").is_none());
        assert!(Platform::from_cli("").is_none());
    }

    #[test]
    fn cant_build_ends_with_ios_backup() {
        let path = std::path::PathBuf::from(DEFAULT_PATH_IOS);
        assert!(Platform::determine(&path).is_err());
    }
}
