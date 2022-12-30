/*!
 This module represents the chat to handle join table.
*/

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    error::table::TableError,
    tables::table::{
        Cacheable, Deduplicate, Diagnostic, Table, CHAT_HANDLE_JOIN, CHAT_MESSAGE_JOIN,
    },
    util::output::{done_processing, processing},
};
use rusqlite::{Connection, Error, Result, Row, Statement};

/// Represents a single row in the `chat_handle_join` table.
pub struct ChatToHandle {
    chat_id: i32,
    handle_id: i32,
}

impl Table for ChatToHandle {
    fn from_row(row: &Row) -> Result<ChatToHandle> {
        Ok(ChatToHandle {
            chat_id: row.get(0)?,
            handle_id: row.get(1)?,
        })
    }

    fn get(db: &Connection) -> Statement {
        db.prepare(&format!("SELECT * FROM {}", CHAT_HANDLE_JOIN))
            .unwrap()
    }

    fn extract(chat_to_handle: Result<Result<Self, Error>, Error>) -> Result<Self, TableError> {
        match chat_to_handle {
            Ok(chat_to_handle) => match chat_to_handle {
                Ok(c2h) => Ok(c2h),
                // TODO: When does this occur?
                Err(why) => Err(TableError::ChatToHandle(why)),
            },
            // TODO: When does this occur?
            Err(why) => Err(TableError::ChatToHandle(why)),
        }
    }
}

impl Cacheable for ChatToHandle {
    type K = i32;
    type V = BTreeSet<i32>;
    /// Generate a hashmap containing each chatroom's ID pointing to a HashSet of participant handle IDs
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Cacheable, get_connection};
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path);
    /// let chatrooms = ChatToHandle::cache(&conn);
    /// ```
    fn cache(db: &Connection) -> Result<HashMap<Self::K, Self::V>, TableError> {
        let mut cache: HashMap<i32, BTreeSet<i32>> = HashMap::new();

        let mut rows = ChatToHandle::get(db);
        let mappings = rows
            .query_map([], |row| Ok(ChatToHandle::from_row(row)))
            .unwrap();

        for mapping in mappings {
            let joiner = ChatToHandle::extract(mapping)?;
            match cache.get_mut(&joiner.chat_id) {
                Some(handles) => {
                    handles.insert(joiner.handle_id);
                }
                None => {
                    let mut data_to_cache = BTreeSet::new();
                    data_to_cache.insert(joiner.handle_id);
                    cache.insert(joiner.chat_id, data_to_cache);
                }
            }
        }

        Ok(cache)
    }
}

impl Deduplicate for ChatToHandle {
    type T = BTreeSet<i32>;

    /// Given the initial set of duplicated chats, deduplicate them based on the participants
    ///
    /// This returns a new hashmap that maps the real chat ID to a new deduplicated unique chat ID
    /// that represents a single chat for all of the same participants, even if they have multiple handles
    fn dedupe(duplicated_data: &HashMap<i32, Self::T>) -> HashMap<i32, i32> {
        let mut deduplicated_chats: HashMap<i32, i32> = HashMap::new();
        let mut participants_to_unique_chat_id: HashMap<Self::T, i32> = HashMap::new();

        // Build cache of each unique set of participants to a new identifier:
        let mut unique_chat_identifier = 0;
        for (chat_id, participants) in duplicated_data {
            match participants_to_unique_chat_id.get(participants) {
                Some(id) => {
                    deduplicated_chats.insert(chat_id.to_owned(), id.to_owned());
                }
                None => {
                    participants_to_unique_chat_id
                        .insert(participants.to_owned(), unique_chat_identifier);
                    deduplicated_chats.insert(chat_id.to_owned(), unique_chat_identifier);
                    unique_chat_identifier += 1;
                }
            }
        }
        deduplicated_chats
    }
}

impl Diagnostic for ChatToHandle {
    /// Emit diagnostic data for the Chat to Handle join table
    ///
    /// Get the number of chats referenced in the messages table
    /// that do not exist in this join table:
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Diagnostic, get_connection};
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path);
    /// ChatToHandle::run_diagnostic(&conn);
    /// ```
    fn run_diagnostic(db: &Connection) {
        processing();

        // Get the Chat IDs that are associated with messages
        let mut statement_message_chats = db
            .prepare(&format!("SELECT DISTINCT chat_id from {CHAT_MESSAGE_JOIN}"))
            .unwrap();
        let statement_message_chat_rows = statement_message_chats
            .query_map([], |row: &Row| -> Result<i32> { row.get(0) })
            .unwrap();
        let mut unique_chats_from_messages: HashSet<i32> = HashSet::new();
        statement_message_chat_rows.into_iter().for_each(|row| {
            unique_chats_from_messages.insert(row.unwrap());
        });

        // Get the Chat IDs that are associated with handles
        let mut statement_handle_chats = db
            .prepare(&format!("SELECT DISTINCT chat_id from {CHAT_HANDLE_JOIN}"))
            .unwrap();
        let statement_handle_chat_rows = statement_handle_chats
            .query_map([], |row: &Row| -> Result<i32> { row.get(0) })
            .unwrap();
        let mut unique_chats_from_handles: HashSet<i32> = HashSet::new();
        statement_handle_chat_rows.into_iter().for_each(|row| {
            unique_chats_from_handles.insert(row.unwrap());
        });

        done_processing();

        // Find the set difference and emit
        let chats_with_no_handles = unique_chats_from_messages
            .difference(&unique_chats_from_handles)
            .count();
        if chats_with_no_handles > 0 {
            println!("\rChats with no handles: {chats_with_no_handles:?}");
        }
    }
}
