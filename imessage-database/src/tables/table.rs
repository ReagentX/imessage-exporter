/*!
 This module defines traits for table representations and stores some shared table constants.

 # Zero-Allocation Streaming API

 This module provides zero-allocation streaming capabilities for all database tables through a callback-based API.

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

 Note: you can substitute `TableError` with your own error type if you want to handle errors differently. See the [`Table::stream`] method for more details.
*/

use std::{collections::HashMap, fs::metadata, path::Path};

use rusqlite::{CachedStatement, Connection, Error, OpenFlags, Result, Row, blob::Blob};

use crate::error::table::{TableConnectError, TableError};

/// Defines behavior for SQL Table data
pub trait Table: Sized {
    /// Deserialize a single row into Self, returning a [`rusqlite::Result`]
    fn from_row(row: &Row) -> Result<Self>;

    /// Prepare SELECT * statement
    fn get(db: &Connection) -> Result<CachedStatement, TableError>;

    /// Map a `rusqlite::Result<Self>` into our `TableError`
    fn extract(item: Result<Result<Self, Error>, Error>) -> Result<Self, TableError>;

    /// Process all rows from the table using a callback.
    /// This is the most memory-efficient approach for large tables.
    ///
    /// Uses the default `Table` implementation to prepare the statement and query the rows.
    ///
    /// To execute custom queries, see the [`message`](crate::tables::messages::message) module docs for examples.
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
    /// // Get a connection to the database
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
    fn stream<F, E>(db: &Connection, callback: F) -> Result<(), TableError>
    where
        F: FnMut(Result<Self, TableError>) -> Result<(), E>,
    {
        stream_table_callback::<Self, F, E>(db, callback)
    }

    /// Get a BLOB from the table
    ///
    /// # Arguments
    ///
    /// * `db` - The database connection
    /// * `table` - The name of the table
    /// * `column` - The name of the column containing the BLOB
    /// * `rowid` - The row ID to retrieve the BLOB from
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

    /// Check if a BLOB exists in the table
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

fn stream_table_callback<T, F, E>(db: &Connection, mut callback: F) -> Result<(), TableError>
where
    T: Table + Sized,
    F: FnMut(Result<T, TableError>) -> Result<(), E>,
{
    let mut stmt = T::get(db)?;
    let rows = stmt.query_map([], |row| Ok(T::from_row(row)))?;

    for row_result in rows {
        let item_result = T::extract(row_result);
        let _ = callback(item_result);
    }
    Ok(())
}

/// Defines behavior for table data that can be cached in memory
pub trait Cacheable {
    /// The key type for the cache `HashMap`
    type K;
    /// The value type for the cache `HashMap`
    type V;
    /// Caches the table data in a `HashMap`
    fn cache(db: &Connection) -> Result<HashMap<Self::K, Self::V>, TableError>;
}

/// Defines behavior for deduplicating data in a table
pub trait Deduplicate {
    /// The type of data being deduplicated
    type T;
    /// Creates a mapping from duplicated IDs to canonical IDs
    fn dedupe(duplicated_data: &HashMap<i32, Self::T>) -> HashMap<i32, i32>;
}

/// Defines behavior for printing diagnostic information for a table
pub trait Diagnostic {
    /// Emit diagnostic data about the table to `stdout`
    fn run_diagnostic(db: &Connection) -> Result<(), TableError>;
}

/// Get a connection to the iMessage `SQLite` database
// # Example:
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
            Ok(res) => Ok(res),
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

/// Get the size of the database on the disk
// # Example:
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

// Table Names
/// Handle table name
pub const HANDLE: &str = "handle";
/// Message table name
pub const MESSAGE: &str = "message";
/// Chat table name
pub const CHAT: &str = "chat";
/// Attachment table name
pub const ATTACHMENT: &str = "attachment";
/// Chat to message join table name
pub const CHAT_MESSAGE_JOIN: &str = "chat_message_join";
/// Message to attachment join table name
pub const MESSAGE_ATTACHMENT_JOIN: &str = "message_attachment_join";
/// Chat to handle join table name
pub const CHAT_HANDLE_JOIN: &str = "chat_handle_join";
/// Recently deleted messages table
pub const RECENTLY_DELETED: &str = "chat_recoverable_message_join";

// Column names
/// The payload data column contains `plist`-encoded app message data
pub const MESSAGE_PAYLOAD: &str = "payload_data";
/// The message summary info column contains `plist`-encoded edited message information
pub const MESSAGE_SUMMARY_INFO: &str = "message_summary_info";
/// The `attributedBody` column contains [`typedstream`](crate::util::typedstream)-encoded a message's body text with many other attributes
pub const ATTRIBUTED_BODY: &str = "attributedBody";
/// The sticker user info column contains `plist`-encoded metadata for sticker attachments
pub const STICKER_USER_INFO: &str = "sticker_user_info";
/// The attribution info contains `plist`-encoded metadata for sticker attachments
pub const ATTRIBUTION_INFO: &str = "attribution_info";
/// The properties column contains `plist`-encoded metadata for a chat
pub const PROPERTIES: &str = "properties";

// Default information
/// Name used for messages sent by the database owner in a first-person context
pub const ME: &str = "Me";
/// Name used for messages sent by the database owner in a second-person context
pub const YOU: &str = "You";
/// Name used for contacts or chats where the name cannot be discovered
pub const UNKNOWN: &str = "Unknown";
/// Default location for the Messages database on macOS
pub const DEFAULT_PATH_MACOS: &str = "Library/Messages/chat.db";
/// Default location for the Messages database in an iOS backup
pub const DEFAULT_PATH_IOS: &str = "3d/3d0d7e5fb2ce288813306e4d4636395e047a3d28";
/// Chat name reserved for messages that do not belong to a chat in the table
pub const ORPHANED: &str = "orphaned";
/// Replacement text sent in Fitness.app messages
pub const FITNESS_RECEIVER: &str = "$(kIMTranscriptPluginBreadcrumbTextReceiverIdentifier)";
/// Name for attachments directory in exports
pub const ATTACHMENTS_DIR: &str = "attachments";
