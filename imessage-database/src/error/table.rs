/*!
 Errors that can happen when extracting data from a `SQLite` table.
*/

use std::{
    fmt::{Display, Formatter, Result},
    path::PathBuf,
};

use crate::{tables::table::DEFAULT_PATH_IOS, util::path_hint::PathHint};

/// Errors that can happen when extracting data from a `SQLite` table
#[derive(Debug)]
pub enum TableError {
    /// Error when querying the table
    QueryError(rusqlite::Error),
    /// Error when connecting to the database
    CannotConnect(TableConnectError),
    /// Error when reading from the database file
    CannotRead(std::io::Error),
}

/// Reasons why the database could not be connected to or read from
#[derive(Debug)]
pub enum TableConnectError {
    /// Failure to open the file with `SQLite`
    Permissions(rusqlite::Error),
    /// Failure to inspect or open the path: missing Full Disk Access on macOS, or a path
    /// that cannot be parsed by the OS
    Inaccessible(PathBuf, std::io::Error),
    /// Directory supplied in place of a database file, with hints from its contents
    IsDirectory(PathBuf, Vec<PathHint>),
    /// Directory with no Messages database at the iOS backup path, with hints from its contents
    MissingBackupDatabase(PathBuf, Vec<PathHint>),
    /// File without the `SQLite` header
    NotADatabase(PathBuf),
    /// `SQLite` database without a `message` table
    NotMessagesDatabase(PathBuf),
    /// Missing path, with hints from the nearest existing ancestor
    DoesNotExist(PathBuf, Vec<PathHint>),
    /// Not a backup root directory
    NotBackupRoot,
}

/// Full Disk Access guidance for permission errors on macOS.
const FULL_DISK_ACCESS: &str = if cfg!(target_os = "macos") {
    "\nEnsure full disk access is enabled for your terminal emulator in System Settings > Privacy & Security > Full Disk Access"
} else {
    ""
};

fn write_hints(fmt: &mut Formatter<'_>, hints: &[PathHint]) -> Result {
    for hint in hints {
        write!(fmt, "\n{hint}")?;
    }
    Ok(())
}

impl Display for TableConnectError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            TableConnectError::Permissions(why) => write!(
                fmt,
                "Unable to read from chat database: {why}{FULL_DISK_ACCESS}"
            ),
            TableConnectError::Inaccessible(path, why) => write!(
                fmt,
                "Unable to access `{}`: {why}{FULL_DISK_ACCESS}",
                path.display()
            ),
            TableConnectError::IsDirectory(path, hints) => {
                write!(
                    fmt,
                    "Specified path `{}` is a directory, not a database file!",
                    path.display()
                )?;
                write_hints(fmt, hints)
            }
            TableConnectError::MissingBackupDatabase(path, hints) => {
                write!(
                    fmt,
                    "Specified path `{}` is a directory, but it contains no Messages database at `{DEFAULT_PATH_IOS}`!",
                    path.display()
                )?;
                write_hints(fmt, hints)
            }
            TableConnectError::NotADatabase(path) => write!(
                fmt,
                "Specified path `{}` is not a SQLite database file!",
                path.display()
            ),
            TableConnectError::NotMessagesDatabase(path) => write!(
                fmt,
                "`{}` is a SQLite database, but it has no `message` table, so it is not a Messages database!",
                path.display()
            ),
            TableConnectError::DoesNotExist(path, hints) => {
                write!(
                    fmt,
                    "Database file `{}` does not exist at the specified path!",
                    path.display()
                )?;
                write_hints(fmt, hints)
            }
            TableConnectError::NotBackupRoot => write!(
                fmt,
                "The path provided points to a database inside of an iOS backup, not the root of the backup."
            ),
        }
    }
}

impl Display for TableError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            TableError::CannotConnect(why) => write!(fmt, "{why}"),
            TableError::CannotRead(why) => write!(fmt, "Cannot read from filesystem: {why}"),
            TableError::QueryError(error) => write!(fmt, "Failed to query table: {error}"),
        }
    }
}

impl std::error::Error for TableError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TableError::QueryError(e) => Some(e),
            TableError::CannotConnect(e) => Some(e),
            TableError::CannotRead(e) => Some(e),
        }
    }
}

impl std::error::Error for TableConnectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TableConnectError::Permissions(e) => Some(e),
            TableConnectError::Inaccessible(_, e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TableError {
    fn from(err: std::io::Error) -> Self {
        TableError::CannotRead(err)
    }
}

impl From<rusqlite::Error> for TableError {
    fn from(err: rusqlite::Error) -> Self {
        TableError::QueryError(err)
    }
}
