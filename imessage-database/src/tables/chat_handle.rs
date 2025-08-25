/*!
 This module represents the chat to handle join table.
*/

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    error::table::TableError,
    tables::table::{
        CHAT_HANDLE_JOIN, CHAT_MESSAGE_JOIN, Cacheable, Deduplicate, Diagnostic, Table,
    },
    util::output::{done_processing, processing},
};
use rusqlite::{CachedStatement, Connection, Error, Result, Row};

/// Represents a single row in the `chat_handle_join` table.
pub struct ChatToHandle {
    chat_id: i32,
    handle_id: i32,
}

impl Table for ChatToHandle {
    fn from_row(row: &Row) -> Result<ChatToHandle> {
        Ok(ChatToHandle {
            chat_id: row.get("chat_id")?,
            handle_id: row.get("handle_id")?,
        })
    }

    fn get(db: &Connection) -> Result<CachedStatement, TableError> {
        Ok(db.prepare_cached(&format!("SELECT * FROM {CHAT_HANDLE_JOIN}"))?)
    }

    fn extract(chat_to_handle: Result<Result<Self, Error>, Error>) -> Result<Self, TableError> {
        match chat_to_handle {
            Ok(Ok(chat_to_handle)) => Ok(chat_to_handle),
            Err(why) | Ok(Err(why)) => Err(TableError::QueryError(why)),
        }
    }
}

impl Cacheable for ChatToHandle {
    type K = i32;
    type V = BTreeSet<i32>;
    /// Generate a hashmap containing each chatroom's ID pointing to a `HashSet` of participant handle IDs
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Cacheable, get_connection};
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let chatrooms = ChatToHandle::cache(&conn);
    /// ```
    fn cache(db: &Connection) -> Result<HashMap<Self::K, Self::V>, TableError> {
        let mut cache: HashMap<i32, BTreeSet<i32>> = HashMap::new();

        let mut rows = ChatToHandle::get(db)?;
        let mappings = rows.query_map([], |row| Ok(ChatToHandle::from_row(row)))?;

        for mapping in mappings {
            let joiner = ChatToHandle::extract(mapping)?;
            if let Some(handles) = cache.get_mut(&joiner.chat_id) {
                handles.insert(joiner.handle_id);
            } else {
                let mut data_to_cache = BTreeSet::new();
                data_to_cache.insert(joiner.handle_id);
                cache.insert(joiner.chat_id, data_to_cache);
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
    /// that represents a single chat for all of the same participants, even if they have multiple handles.
    ///
    /// Assuming no new chat-handle relationships have been written to the database, deduplicated data is deterministic across runs.
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Cacheable, Deduplicate, get_connection};
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let chatrooms = ChatToHandle::cache(&conn).unwrap();
    /// let deduped_chatrooms = ChatToHandle::dedupe(&chatrooms);
    /// ```
    fn dedupe(duplicated_data: &HashMap<i32, Self::T>) -> HashMap<i32, i32> {
        let mut deduplicated_chats: HashMap<i32, i32> = HashMap::new();
        let mut participants_to_unique_chat_id: HashMap<Self::T, i32> = HashMap::new();

        // Build cache of each unique set of participants to a new identifier
        let mut unique_chat_identifier = 0;

        // Iterate over the values in a deterministic order
        let mut sorted_dupes: Vec<(&i32, &Self::T)> = duplicated_data.iter().collect();
        sorted_dupes.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (chat_id, participants) in sorted_dupes {
            if let Some(id) = participants_to_unique_chat_id.get(participants) {
                deduplicated_chats.insert(chat_id.to_owned(), id.to_owned());
            } else {
                participants_to_unique_chat_id
                    .insert(participants.to_owned(), unique_chat_identifier);
                deduplicated_chats.insert(chat_id.to_owned(), unique_chat_identifier);
                unique_chat_identifier += 1;
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
    /// let conn = get_connection(&db_path).unwrap();
    /// ChatToHandle::run_diagnostic(&conn);
    /// ```
    fn run_diagnostic(db: &Connection) -> Result<(), TableError> {
        processing();

        // Get the Chat IDs that are associated with messages
        let mut statement_message_chats =
            db.prepare(&format!("SELECT DISTINCT chat_id from {CHAT_MESSAGE_JOIN}"))?;
        let statement_message_chat_rows =
            statement_message_chats.query_map([], |row: &Row| -> Result<i32> { row.get(0) })?;
        let mut unique_chats_from_messages: HashSet<i32> = HashSet::new();
        statement_message_chat_rows.into_iter().for_each(|row| {
            if let Ok(row) = row {
                unique_chats_from_messages.insert(row);
            }
        });

        // Get the Chat IDs that are associated with handles
        let mut statement_handle_chats =
            db.prepare(&format!("SELECT DISTINCT chat_id from {CHAT_HANDLE_JOIN}"))?;
        let statement_handle_chat_rows =
            statement_handle_chats.query_map([], |row: &Row| -> Result<i32> { row.get(0) })?;
        let mut unique_chats_from_handles: HashSet<i32> = HashSet::new();
        statement_handle_chat_rows.into_iter().for_each(|row| {
            if let Ok(row) = row {
                unique_chats_from_handles.insert(row);
            }
        });

        done_processing();

        // Find the set difference and emit
        let chats_with_no_handles = unique_chats_from_messages
            .difference(&unique_chats_from_handles)
            .count();
        if chats_with_no_handles > 0 {
            println!("Thread diagnostic data:");
            println!("    Chats with no handles: {chats_with_no_handles:?}");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::tables::{chat_handle::ChatToHandle, table::Deduplicate};
    use std::collections::{BTreeSet, HashMap, HashSet};

    #[test]
    fn can_dedupe() {
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(1, BTreeSet::from([1])); // 0
        input.insert(2, BTreeSet::from([1])); // 0
        input.insert(3, BTreeSet::from([1])); // 0
        input.insert(4, BTreeSet::from([2])); // 1
        input.insert(5, BTreeSet::from([2])); // 1
        input.insert(6, BTreeSet::from([3])); // 2

        let output = ChatToHandle::dedupe(&input);
        let expected_deduped_ids: HashSet<i32> = output.values().copied().collect();
        assert_eq!(expected_deduped_ids.len(), 3);
    }

    #[test]
    fn can_dedupe_multi() {
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(1, BTreeSet::from([1, 2])); // 0
        input.insert(2, BTreeSet::from([1])); // 1
        input.insert(3, BTreeSet::from([1])); // 1
        input.insert(4, BTreeSet::from([2, 1])); // 0
        input.insert(5, BTreeSet::from([2, 3])); // 2
        input.insert(6, BTreeSet::from([3])); // 3

        let output = ChatToHandle::dedupe(&input);
        let expected_deduped_ids: HashSet<i32> = output.values().copied().collect();
        assert_eq!(expected_deduped_ids.len(), 4);
    }

    #[test]
    // Simulate 3 runs of the program and ensure that the order of the deduplicated contacts is stable
    fn test_same_values() {
        let mut input_1: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input_1.insert(1, BTreeSet::from([1]));
        input_1.insert(2, BTreeSet::from([1]));
        input_1.insert(3, BTreeSet::from([1]));
        input_1.insert(4, BTreeSet::from([2]));
        input_1.insert(5, BTreeSet::from([2]));
        input_1.insert(6, BTreeSet::from([3]));

        let mut input_2: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input_2.insert(1, BTreeSet::from([1]));
        input_2.insert(2, BTreeSet::from([1]));
        input_2.insert(3, BTreeSet::from([1]));
        input_2.insert(4, BTreeSet::from([2]));
        input_2.insert(5, BTreeSet::from([2]));
        input_2.insert(6, BTreeSet::from([3]));

        let mut input_3: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input_3.insert(1, BTreeSet::from([1]));
        input_3.insert(2, BTreeSet::from([1]));
        input_3.insert(3, BTreeSet::from([1]));
        input_3.insert(4, BTreeSet::from([2]));
        input_3.insert(5, BTreeSet::from([2]));
        input_3.insert(6, BTreeSet::from([3]));

        let mut output_1 = ChatToHandle::dedupe(&input_1)
            .into_iter()
            .collect::<Vec<(i32, i32)>>();
        let mut output_2 = ChatToHandle::dedupe(&input_2)
            .into_iter()
            .collect::<Vec<(i32, i32)>>();
        let mut output_3 = ChatToHandle::dedupe(&input_3)
            .into_iter()
            .collect::<Vec<(i32, i32)>>();

        output_1.sort_unstable();
        output_2.sort_unstable();
        output_3.sort_unstable();

        assert_eq!(output_1, output_2);
        assert_eq!(output_1, output_3);
        assert_eq!(output_2, output_3);
    }
}
