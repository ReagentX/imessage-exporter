/*!
 Table traits, database connection helpers, and shared table constants.

 # Streaming API

 The streaming API processes each row through a callback without collecting the
 table into a `Vec`.

 ```no_run
 use imessage_database::{
    error::table::TableError,
    tables::{
        table::{get_connection, Table},
        messages::Message,
    },
    util::dirs::default_db_path
 };

 let db_path = default_db_path();
 let db = get_connection(&db_path).unwrap();

 Message::stream(&db, |message_result| {
     match message_result {
         Ok(message) => println!("Message: {:#?}", message),
         Err(e) => eprintln!("Error: {:?}", e),
     }
    Ok::<(), TableError>(())
 }).unwrap();
 ```

 The callback may return any error type that implements `From<TableError>`.
*/

use std::{collections::HashMap, fs::metadata, path::Path};

use rusqlite::{
    CachedStatement, Connection, Error, OpenFlags, Params, Result, Row, Statement, blob::Blob,
};

use crate::error::table::{TableConnectError, TableError};

// MARK: Traits
/// Database table model that can deserialize itself from SQLite rows.
pub trait Table: Sized {
    /// Deserialize a single row into `Self`. Returns [`rusqlite::Result`]
    /// for direct use inside `rusqlite::query_map` / `query_row`
    /// callbacks. For high-level iteration, prefer [`Table::rows`] or
    /// [`Table::row`].
    fn from_row(row: &Row) -> Result<Self>;

    /// Prepare the table's default `SELECT *` statement.
    fn get(db: &'_ Connection) -> Result<CachedStatement<'_>, TableError>;

    /// Iterate over rows produced by `stmt`. The default implementation
    /// deserializes each through [`from_row`](Self::from_row). Row-fetch and
    /// row-deserialization failures are surfaced uniformly as [`TableError`].
    /// Accepts both [`rusqlite::Statement`] and [`rusqlite::CachedStatement`]
    /// (the latter via deref coercion).
    ///
    /// Use this when the caller owns a custom prepared statement (with
    /// filters, joins, or bound parameters). For a full-table scan against
    /// the default `SELECT *` with a callback API, see [`Table::stream`].
    ///
    /// Implementations may override this method to resolve column layout once
    /// and reuse it for every row. Resolve through the first [`Row`] when the
    /// schema may change concurrently: `sqlite3_step` may recompile the
    /// statement after preparation.
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
    ///
    /// Implementations may override this method to resolve the stepped row's
    /// column layout instead of decoding by name.
    fn row<P: Params>(stmt: &mut Statement<'_>, params: P) -> Result<Self, TableError> {
        flatten_row(stmt.query_row(params, |row| Ok(Self::from_row(row))))
    }

    /// Process every row from the table's default `SELECT *` query using a
    /// callback. Builds and discards the prepared statement internally, so
    /// the caller never sees it.
    ///
    /// Use this for full-table scans where the callback style fits. For
    /// custom statements (filters, joins, bound parameters), prepare the
    /// statement yourself and iterate via [`Table::rows`]. See the
    /// [`message`](crate::tables::messages::message) module docs for an
    /// example.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use imessage_database::{
    ///    error::table::TableError,
    ///    tables::{
    ///        table::{get_connection, Table},
    ///        handle::Handle,
    ///    },
    ///    util::dirs::default_db_path
    /// };
    ///
    /// let db_path = default_db_path();
    /// let db = get_connection(&db_path).unwrap();
    ///
    /// // Stream the Handle table, processing each row with a callback
    /// Handle::stream(&db, |handle_result| {
    ///     match handle_result {
    ///         Ok(handle) => println!("Handle: {}", handle.id),
    ///         Err(e) => eprintln!("Error: {:?}", e),
    ///     }
    ///     Ok::<(), TableError>(())
    /// }).unwrap();
    /// ```
    fn stream<F, E>(db: &Connection, callback: F) -> Result<(), E>
    where
        E: From<TableError>,
        F: FnMut(Result<Self, TableError>) -> Result<(), E>,
    {
        stream_table_callback::<Self, F, E>(db, callback)
    }

    /// Open a `BLOB` column for the supplied `rowid`.
    fn get_blob<'a>(
        &self,
        db: &'a Connection,
        table: &str,
        column: &str,
        rowid: i64,
    ) -> Option<Blob<'a>> {
        db.blob_open(rusqlite::MAIN_DB, table, column, rowid, true)
            .ok()
    }

    /// Return whether a `BLOB` column is non-null for the supplied `rowid`.
    fn has_blob(&self, db: &Connection, table: &str, column: &str, rowid: i64) -> bool {
        let sql = std::format!(
            "SELECT ({column} IS NOT NULL) AS not_null
         FROM {table}
         WHERE rowid = ?1",
        );

        // This returns 1 for true, 0 for false.
        db.query_row(&sql, [rowid], |row| row.get(0))
            .ok()
            .is_some_and(|v: i32| v != 0)
    }
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

fn stream_table_callback<T, F, E>(db: &Connection, mut callback: F) -> Result<(), E>
where
    T: Table + Sized,
    E: From<TableError>,
    F: FnMut(Result<T, TableError>) -> Result<(), E>,
{
    let mut stmt = T::get(db).map_err(E::from)?;
    for row_result in T::rows(&mut stmt, []).map_err(E::from)? {
        callback(row_result)?;
    }
    Ok(())
}

/// Table data that can be materialized into an in-memory map.
pub trait Cacheable {
    /// Key type for the cache map.
    type K;
    /// Value type for the cache map.
    type V;
    /// Build the cache from the database.
    fn cache(db: &Connection) -> Result<HashMap<Self::K, Self::V>, TableError>;
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
/// let db_path = default_db_path();
/// let connection = get_connection(&db_path);
/// ```
pub fn get_connection(path: &Path) -> Result<Connection, TableError> {
    if path.exists() && path.is_file() {
        return match Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(connection) => {
                // Read pages from the mapped region where SQLite supports it.
                let _ = connection.pragma_update(None, "mmap_size", 8_589_934_592_i64); // up to 8 GiB
                let _ = connection.pragma_update(None, "cache_size", -65_536_i64); // ~64 MiB
                Ok(connection)
            }
            Err(why) => Err(TableError::CannotConnect(TableConnectError::Permissions(
                why,
            ))),
        };
    }

    // Path does not point to a file
    if path.exists() && !path.is_file() {
        return Err(TableError::CannotConnect(TableConnectError::NotAFile(
            path.to_path_buf(),
        )));
    }

    // File is missing
    Err(TableError::CannotConnect(TableConnectError::DoesNotExist(
        path.to_path_buf(),
    )))
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
/// let db_path = default_db_path();
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
/// [`typedstream`](crate::util::typedstream)-encoded attributed body column.
pub const ATTRIBUTED_BODY: &str = "attributedBody";
/// [`plist`](crate::util::plist)-encoded sticker metadata column.
pub const STICKER_USER_INFO: &str = "sticker_user_info";
/// [`plist`](crate::util::plist)-encoded attachment attribution column.
pub const ATTRIBUTION_INFO: &str = "attribution_info";
/// [`plist`](crate::util::plist)-encoded chat properties column.
pub const PROPERTIES: &str = "properties";

// Default information
/// First-person display name for the database owner.
pub const ME: &str = "Me";
/// Second-person display name for the database owner.
pub const YOU: &str = "You";
/// Display name used when a contact or chat name is unavailable.
pub const UNKNOWN: &str = "Unknown";
/// Default macOS Messages database path.
pub const DEFAULT_PATH_MACOS: &str = "Library/Messages/chat.db";
/// Default Messages database path inside an iOS backup.
pub const DEFAULT_PATH_IOS: &str = "3d/3d0d7e5fb2ce288813306e4d4636395e047a3d28";
/// Chat name reserved for messages that do not belong to a chat row.
pub const ORPHANED: &str = "orphaned";
/// Replacement token found in Fitness.app messages.
pub const FITNESS_RECEIVER: &str = "$(kIMTranscriptPluginBreadcrumbTextReceiverIdentifier)";
/// Attachments directory name used in exports.
pub const ATTACHMENTS_DIR: &str = "attachments";

#[cfg(test)]
mod tests {
    use rusqlite::{CachedStatement, Connection, Result, Row};

    use crate::error::table::TableError;

    use super::Table;

    struct TestRow(i64);

    impl Table for TestRow {
        fn from_row(row: &Row) -> Result<Self> {
            Ok(Self(row.get(0)?))
        }

        fn get(db: &'_ Connection) -> Result<CachedStatement<'_>, TableError> {
            Ok(db.prepare_cached("SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3")?)
        }
    }

    #[derive(Debug)]
    enum StreamError {
        Table(TableError),
        Stop,
    }

    impl From<TableError> for StreamError {
        fn from(err: TableError) -> Self {
            Self::Table(err)
        }
    }

    #[test]
    fn stream_propagates_callback_errors() {
        let db = Connection::open_in_memory().unwrap();
        let mut seen = vec![];

        let result = TestRow::stream(&db, |row| {
            let row = row.map_err(StreamError::from)?;
            seen.push(row.0);
            if row.0 == 2 {
                return Err(StreamError::Stop);
            }
            Ok(())
        });

        assert!(matches!(result, Err(StreamError::Stop)));
        assert_eq!(seen, vec![1, 2]);
    }

    #[test]
    fn stream_converts_setup_errors() {
        struct BrokenTable;

        impl Table for BrokenTable {
            fn from_row(_row: &Row) -> Result<Self> {
                Ok(Self)
            }

            fn get(_db: &'_ Connection) -> Result<CachedStatement<'_>, TableError> {
                Err(TableError::CannotRead(std::io::Error::other("boom")))
            }
        }

        let db = Connection::open_in_memory().unwrap();
        let result = BrokenTable::stream(&db, |_| Ok::<(), StreamError>(()));

        assert!(matches!(
            result,
            Err(StreamError::Table(TableError::CannotRead(_)))
        ));
    }
}
