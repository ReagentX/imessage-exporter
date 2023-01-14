/*!
 This module defines traits for table representations and stores some shared table constants.
*/

use std::collections::HashMap;

use rusqlite::{Connection, Error, OpenFlags, Result, Row, Statement};

use crate::error::table::TableError;

/// Defines behavior for SQL Table data
pub trait Table {
    /// Serializes a single row of data to an instance of the struct that implements this Trait
    fn from_row(row: &Row) -> Result<Self>
    where
        Self: Sized;
    /// Gets a statement we can execute to iterate over the data in the table
    fn get(db: &Connection) -> Statement;

    /// Extract valid row data while handling both types of query errors
    fn extract(item: Result<Result<Self, Error>, Error>) -> Result<Self, TableError>
    where
        Self: Sized;
}

/// Defines behavior for table data that can be cached in memory
pub trait Cacheable {
    type K;
    type V;
    fn cache(db: &Connection) -> Result<HashMap<Self::K, Self::V>, TableError>;
}

/// Defines behavior for deduplicating data in a table
pub trait Deduplicate {
    type T;
    fn dedupe(duplicated_data: &HashMap<i32, Self::T>) -> HashMap<i32, i32>;
}

/// Defines behavior for printing diagnostic information for a table
pub trait Diagnostic {
    /// Emit diagnostic data about the table to `stdout`
    fn run_diagnostic(db: &Connection);
}

/// Get a connection to the iMessage SQLite database
pub fn get_connection(path: &str) -> Result<Connection, String> {
    match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(res) => Ok(res),
        Err(why) => Err(format!("Unable to read from chat database: {}\nEnsure full disk access is enabled for your terminal emulator in System Settings > Security and Privacy > Full Disk Access", why)),
    }
}

// Table Names
/// Handle table name
pub const HANDLE: &str = "handle";
/// Message table name
pub const MESSAGE: &str = "message";
/// Chat table name
pub const CHAT: &str = "chat";
/// Attachment table names
pub const ATTACHMENT: &str = "attachment";
/// Chat to message join table name
pub const CHAT_MESSAGE_JOIN: &str = "chat_message_join";
/// Message to attachment join table name
pub const MESSAGE_ATTACHMENT_JOIN: &str = "message_attachment_join";
/// Chat to handle join table name
pub const CHAT_HANDLE_JOIN: &str = "chat_handle_join";

// Column names
/// The payload data column contains app message data
pub const MESSAGE_PAYLOAD: &str = "payload_data";
/// The message summary info column contains edited message information
pub const MESSAGE_SUMMARY_INFO: &str = "message_summary_info";
/// The attributedBody column contains a message's body text with any other attributes
pub const ATTRIBUTED_BODY: &str = "attributedBody";

// Default information
/// Name used for messages sent by the database owner
pub const ME: &str = "Me";
/// Name used for contacts or chats where the name cannot be discovered
pub const UNKNOWN: &str = "Unknown";
/// Default location for the Messages database
pub const DEFAULT_PATH: &str = "Library/Messages/chat.db";
/// Chat name reserved for messages that do not belong to a chat in the table
pub const ORPHANED: &str = "orphaned";
/// Default export directory name
pub const DEFAULT_OUTPUT_DIR: &str = "imessage_export";
/// Maximum length a filename can be
pub const MAX_LENGTH: usize = 240;
/// Replacement text sent in Fitness.app messages
pub const FITNESS_RECEIVER: &str = "$(kIMTranscriptPluginBreadcrumbTextReceiverIdentifier)";
