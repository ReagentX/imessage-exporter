/*!
 Table traits, database connection helpers, and shared table constants.

 # Row iteration

 Prepare a statement with the appropriate table type, then use [`Table::rows`]
 to decode one row at a time. Use any statement that selects the table's columns.

 ```no_run
 use imessage_database::{
    tables::{handle::Handle, table::{Table, get_connection}},
    util::dirs::default_db_path,
 };

 let db_path = default_db_path().unwrap();
 let db = get_connection(&db_path).unwrap();

 let mut statement = Handle::get(&db).unwrap();
 for handle_result in Handle::rows(&mut statement, []).unwrap() {
     match handle_result {
         Ok(handle) => println!("Handle: {}", handle.id),
         Err(e) => eprintln!("Error: {:?}", e),
     }
 }
 ```
*/

use std::{
    fs::{File, metadata},
    io::{ErrorKind, Read},
    path::Path,
};

use rusqlite::{Connection, Error, OpenFlags, Params, Result, Row, Statement, blob::Blob};

use crate::{
    error::table::{TableConnectError, TableError},
    util::path_hint::{Presence, diagnose, presence},
};

// MARK: Traits
/// Deserialize a database record from a stepped SQLite row.
///
/// Implement [`from_row`](Self::from_row), then use [`rows`](Self::rows) and
/// [`row`](Self::row) with any statement that selects the table's columns.
pub trait Table: Sized {
    /// Deserialize a single row into `Self`. Returns [`rusqlite::Result`]
    /// for direct use inside `rusqlite::query_map` / `query_row`
    /// callbacks. For high-level iteration, prefer [`Table::rows`] or
    /// [`Table::row`].
    fn from_row(row: &Row) -> Result<Self>;

    /// Iterate over the results of `stmt`, deserializing each row through
    /// [`from_row`](Self::from_row). Return [`TableError`] for both fetch and
    /// deserialization failures. Pass either a [`rusqlite::Statement`] or a
    /// [`rusqlite::CachedStatement`] (via deref coercion).
    fn rows<'stmt, P: Params>(
        stmt: &'stmt mut Statement<'_>,
        params: P,
    ) -> Result<impl Iterator<Item = Result<Self, TableError>> + 'stmt, TableError>
    where
        Self: 'stmt,
    {
        let mapped = stmt.query_map(params, |row| Ok(Self::from_row(row)))?;
        Ok(mapped.map(flatten_row))
    }

    /// Fetch exactly one row from `stmt`. Returns
    /// [`TableError::QueryError`] if the row is missing or fails to
    /// deserialize. Accepts both [`rusqlite::Statement`] and
    /// [`rusqlite::CachedStatement`] (the latter via deref coercion).
    fn row<P: Params>(stmt: &mut Statement<'_>, params: P) -> Result<Self, TableError> {
        flatten_row(stmt.query_row(params, |row| Ok(Self::from_row(row))))
    }
}

/// Open a `BLOB` column for `rowid`, read-only.
pub fn open_blob<'a>(
    db: &'a Connection,
    table: &str,
    column: &str,
    rowid: i64,
) -> Option<Blob<'a>> {
    db.blob_open(rusqlite::MAIN_DB, table, column, rowid, true)
        .ok()
}

/// Flatten the doubly-nested result produced by `rusqlite::query_map` /
/// `query_row` callbacks into a single [`TableError`]. The outer layer
/// represents row-fetch failures, the inner layer represents row-deserialize
/// failures from [`Table::from_row`].
pub(crate) fn flatten_row<T>(item: Result<Result<T, Error>, Error>) -> Result<T, TableError> {
    match item {
        Ok(Ok(row)) => Ok(row),
        Err(why) | Ok(Err(why)) => Err(TableError::QueryError(why)),
    }
}

// MARK: Database
/// Open the Messages `SQLite` database read-only.
/// # Example:
///
/// ```
/// use imessage_database::{
///     util::dirs::default_db_path,
///     tables::table::get_connection
/// };
///
/// let db_path = default_db_path().unwrap();
/// let connection = get_connection(&db_path);
/// ```
pub fn get_connection(path: &Path) -> Result<Connection, TableError> {
    let connect_error = match presence(path) {
        Err(why) => TableConnectError::Inaccessible(path.to_path_buf(), why),
        Ok(Presence::Missing) => {
            TableConnectError::DoesNotExist(path.to_path_buf(), diagnose(path))
        }
        Ok(Presence::Directory) => {
            TableConnectError::IsDirectory(path.to_path_buf(), diagnose(path))
        }
        // Inspect only regular files to avoid blocking on a FIFO or device.
        Ok(Presence::Other) => TableConnectError::NotADatabase(path.to_path_buf()),
        Ok(Presence::File) => {
            if !has_sqlite_header(path)? {
                return Err(TableError::CannotConnect(TableConnectError::NotADatabase(
                    path.to_path_buf(),
                )));
            }
            return match Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) {
                Ok(connection) => {
                    // Read pages from the mapped region where SQLite supports it.
                    let _ = connection.pragma_update(None, "mmap_size", 8_589_934_592_i64); // up to 8 GiB
                    let _ = connection.pragma_update(None, "cache_size", -65_536_i64); // ~64 MiB
                    // With SQLite 3.54 and later, a cached statement with a `col = ?` term is
                    // expired after clearing its bindings. See #839.
                    let _ = connection.set_db_config(
                        rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_QPSG,
                        true,
                    );
                    Ok(connection)
                }
                Err(why) => Err(TableError::CannotConnect(TableConnectError::Permissions(
                    why,
                ))),
            };
        }
    };
    Err(TableError::CannotConnect(connect_error))
}

/// 16-byte magic string at the start of every `SQLite` file.
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// Check the magic bytes before opening a connection: with lazy file validation in `SQLite`,
/// you would otherwise encounter a query failure on first use for a foreign file.
fn has_sqlite_header(path: &Path) -> Result<bool, TableError> {
    let mut file = File::open(path).map_err(|why| match why.kind() {
        ErrorKind::PermissionDenied => {
            TableError::CannotConnect(TableConnectError::Inaccessible(path.to_path_buf(), why))
        }
        _ => TableError::CannotRead(why),
    })?;
    let mut header = [0u8; SQLITE_HEADER.len()];
    match file.read_exact(&mut header) {
        Ok(()) => Ok(&header == SQLITE_HEADER),
        Err(why) if why.kind() == ErrorKind::UnexpectedEof => Ok(false),
        Err(why) => Err(TableError::CannotRead(why)),
    }
}

/// Return the database file size on disk.
/// # Example:
///
/// ```
/// use imessage_database::{
///     util::dirs::default_db_path,
///     tables::table::get_db_size
/// };
///
/// let db_path = default_db_path().unwrap();
/// let database_size_in_bytes = get_db_size(&db_path);
/// ```
pub fn get_db_size(path: &Path) -> Result<u64, TableError> {
    Ok(metadata(path)?.len())
}

// MARK: Constants
// Table Names
/// Handle table name.
pub const HANDLE: &str = "handle";
/// Message table name.
pub const MESSAGE: &str = "message";
/// Chat table name.
pub const CHAT: &str = "chat";
/// Attachment table name.
pub const ATTACHMENT: &str = "attachment";
/// Chat-to-message join table name.
pub const CHAT_MESSAGE_JOIN: &str = "chat_message_join";
/// Message-to-attachment join table name.
pub const MESSAGE_ATTACHMENT_JOIN: &str = "message_attachment_join";
/// Chat-to-handle join table name.
pub const CHAT_HANDLE_JOIN: &str = "chat_handle_join";
/// Recently deleted messages table.
pub const RECENTLY_DELETED: &str = "chat_recoverable_message_join";

// Column names
/// [`plist`](crate::util::plist)-encoded app-message payload column.
pub const MESSAGE_PAYLOAD: &str = "payload_data";
/// [`plist`](crate::util::plist)-encoded message summary column.
pub const MESSAGE_SUMMARY_INFO: &str = "message_summary_info";
/// [`plist`](crate::util::plist)-encoded sticker metadata column.
pub const STICKER_USER_INFO: &str = "sticker_user_info";
/// [`plist`](crate::util::plist)-encoded attachment attribution column.
pub const ATTRIBUTION_INFO: &str = "attribution_info";
/// [`plist`](crate::util::plist)-encoded chat properties column.
pub const PROPERTIES: &str = "properties";

// Default information
/// First-person display name for the database owner.
pub const ME: &str = "Me";
/// Default macOS Messages database path.
pub const DEFAULT_PATH_MACOS: &str = "Library/Messages/chat.db";
/// Default Messages database path inside an iOS backup.
pub const DEFAULT_PATH_IOS: &str = "3d/3d0d7e5fb2ce288813306e4d4636395e047a3d28";
/// Replacement token found in Fitness.app messages.
pub const FITNESS_RECEIVER: &str = "$(kIMTranscriptPluginBreadcrumbTextReceiverIdentifier)";

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        fs::{create_dir_all, remove_dir_all, write},
        path::PathBuf,
        process,
    };

    use rusqlite::{Connection, Result, Row};

    use crate::error::table::{TableConnectError, TableError};

    use super::{Table, get_connection};

    fn scratch(label: &str) -> PathBuf {
        let path = temp_dir().join(format!("imessage-database-table-{}-{label}", process::id()));
        let _ = remove_dir_all(&path);
        create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn connection_rejects_directory() {
        let dir = scratch("dir");

        assert!(matches!(
            get_connection(&dir),
            Err(TableError::CannotConnect(TableConnectError::IsDirectory(
                _,
                _
            )))
        ));

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn connection_rejects_missing_file() {
        let dir = scratch("missing");

        assert!(matches!(
            get_connection(&dir.join("chat.db")),
            Err(TableError::CannotConnect(TableConnectError::DoesNotExist(
                _,
                _
            )))
        ));

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn connection_rejects_file_without_sqlite_header() {
        let dir = scratch("header");
        let text = dir.join("chat.db");
        write(&text, b"<?xml version=\"1.0\"?>").unwrap();
        let empty = dir.join("empty.db");
        write(&empty, b"").unwrap();

        assert!(matches!(
            get_connection(&text),
            Err(TableError::CannotConnect(TableConnectError::NotADatabase(
                _
            )))
        ));
        assert!(matches!(
            get_connection(&empty),
            Err(TableError::CannotConnect(TableConnectError::NotADatabase(
                _
            )))
        ));

        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn connection_opens_sqlite_file() {
        let dir = scratch("sqlite");
        let db = dir.join("chat.db");
        Connection::open(&db)
            .unwrap()
            .execute_batch("CREATE TABLE t (x)")
            .unwrap();

        assert!(get_connection(&db).is_ok());

        remove_dir_all(&dir).unwrap();
    }

    struct TestRow(i64);

    impl Table for TestRow {
        fn from_row(row: &Row) -> Result<Self> {
            Ok(Self(row.get(0)?))
        }
    }

    #[test]
    fn rows_decodes_every_row_in_order() {
        let db = Connection::open_in_memory().unwrap();
        let mut statement = db
            .prepare("SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3")
            .unwrap();

        let seen: Vec<i64> = TestRow::rows(&mut statement, [])
            .unwrap()
            .map(|row| row.unwrap().0)
            .collect();

        assert_eq!(seen, vec![1, 2, 3]);
    }

    #[test]
    fn row_without_a_result_is_a_query_error() {
        let db = Connection::open_in_memory().unwrap();
        let mut statement = db.prepare("SELECT 1 WHERE 0").unwrap();

        let result = TestRow::row(&mut statement, []);

        assert!(matches!(result, Err(TableError::QueryError(_))));
    }
}
