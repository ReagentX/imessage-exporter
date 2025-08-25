/*!
 Errors that can happen when extracting data from a `SQLite` table.
*/

use std::{
    fmt::{Display, Formatter, Result},
    path::PathBuf,
};

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
    /// The database file could not be opened due to lack of full disk access
    Permissions(rusqlite::Error),
    /// The database file is not a valid `SQLite` database
    NotAFile(PathBuf),
    /// The database file does not exist
    DoesNotExist(PathBuf),
    /// Not a backup root directory
    NotBackupRoot,
}

impl Display for TableError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            TableError::CannotConnect(why) => match why {
                TableConnectError::Permissions(why) => write!(
                    fmt,
                    "Unable to read from chat database: {why}\nEnsure full disk access is enabled for your terminal emulator in System Settings > Privacy & Security > Full Disk Access"
                ),
                TableConnectError::NotAFile(path) => {
                    write!(
                        fmt,
                        "Specified path `{}` is not a valid SQLite database file!",
                        path.to_string_lossy()
                    )
                }
                TableConnectError::DoesNotExist(path) => {
                    write!(
                        fmt,
                        "Database file `{}` does not exist at the specified path!",
                        path.to_string_lossy()
                    )
                }
                TableConnectError::NotBackupRoot => write!(
                    fmt,
                    "The path provided points to a database inside of an iOS backup, not the root of the backup."
                ),
            },
            TableError::CannotRead(why) => write!(fmt, "Cannot read from filesystem: {why}"),
            TableError::QueryError(error) => write!(fmt, "Failed to query table: {error}"),
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
