/*!
 This module represents the chat to handle join table.
*/

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    error::table::TableError,
    tables::table::{CHAT_HANDLE_JOIN, CHAT_MESSAGE_JOIN, Cacheable, Diagnostic, Table},
    util::output::{done_processing, processing},
};
use rusqlite::{CachedStatement, Connection, Result, Row};

// MARK: Struct
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

    fn get(db: &'_ Connection) -> Result<CachedStatement<'_>, TableError> {
        Ok(db.prepare_cached(&format!("SELECT * FROM {CHAT_HANDLE_JOIN}"))?)
    }

}

// MARK: Cache
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

// MARK: Diagnostic
impl Diagnostic for ChatToHandle {
    /// Emit diagnostic data for the Chat to Handle join table
    ///
    /// Get the number of chats referenced in the messages table
    /// that do not exist in this join table:
    ///
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

        // Cache all chats
        let all_chats = Self::cache(db)?;

        // Cache chatroom participants
        let chatroom_participants = ChatToHandle::cache(db)?;
        let chat_handle_lookup = ChatToHandle::get_chat_lookup_map(db)?;

        // Deduplicate chatroom participants
        let real_chatrooms = ChatToHandle::dedupe(&chatroom_participants, &chat_handle_lookup)?;

        // Calculate total duplicated chats
        let total_dupes =
            all_chats.len() - HashSet::<&i32>::from_iter(real_chatrooms.values()).len();

        done_processing();

        // Find the set difference and emit
        let chats_with_no_handles = unique_chats_from_messages
            .difference(&unique_chats_from_handles)
            .count();

        println!("Thread diagnostic data:");
        println!("    Total chats: {}", all_chats.len());

        if total_dupes > 0 {
            println!("    Total duplicated chats: {total_dupes}");
        }

        if chats_with_no_handles > 0 {
            println!("    Chats with no handles: {chats_with_no_handles:?}");
        }
        Ok(())
    }
}

impl ChatToHandle {
    /// Get the chat lookup map from the database, if it exists
    ///
    /// This is used to map chat IDs that are split across services to a canonical chat ID
    /// for deduplication purposes.
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Diagnostic, get_connection};
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// ChatToHandle::get_chat_lookup_map(&conn);
    /// ```
    pub fn get_chat_lookup_map(conn: &Connection) -> Result<HashMap<i32, i32>, TableError> {
        // Query `chat_lookup`, if it exists, to merge chat IDs split across services
        let mut stmt = conn.prepare(
            "
WITH RECURSIVE
  adj AS (
    SELECT DISTINCT a.chat AS u, b.chat AS v
    FROM chat_lookup a
    JOIN chat_lookup b
      ON a.identifier = b.identifier
  ),
  reach(root, chat) AS (
    SELECT u AS root, v AS chat FROM adj
    UNION
    SELECT r.root, a.v
    FROM reach r
    JOIN adj a ON a.u = r.chat
  ),
  canon AS (
    SELECT chat, MAX(root) AS canonical_chat
    FROM reach
    GROUP BY chat
  )
SELECT chat, canonical_chat
FROM canon
ORDER BY chat;
        ",
        );
        let mut chat_lookup_map: HashMap<i32, i32> = HashMap::new();

        if let Ok(statement) = stmt.as_mut() {
            // Build chat lookup map
            let chat_lookup_rows = statement.query_map([], |row| {
                let chat: i32 = row.get(0)?;
                let canonical: i32 = row.get(1)?;
                Ok((chat, canonical))
            });

            // Populate chat lookup map
            if let Ok(chat_lookup_rows) = chat_lookup_rows {
                for row in chat_lookup_rows {
                    let (chat_id, canonical_chat) = row?;
                    chat_lookup_map.insert(chat_id, canonical_chat);
                }
            }
        }
        Ok(chat_lookup_map)
    }

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
    /// use std::collections::HashMap;
    ///
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Cacheable, Deduplicate, get_connection};
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let chatrooms = ChatToHandle::cache(&conn).unwrap();
    /// let deduped_chatrooms = ChatToHandle::dedupe(&chatrooms, &HashMap::new());
    /// ```
    pub fn dedupe(
        duplicated_data: &HashMap<i32, BTreeSet<i32>>,
        chat_lookup_map: &HashMap<i32, i32>,
    ) -> Result<HashMap<i32, i32>, TableError> {
        let mut deduplicated_chats: HashMap<i32, i32> = HashMap::new();
        let mut participants_to_unique_chat_id: HashMap<BTreeSet<i32>, i32> = HashMap::new();

        // Build cache of each unique set of participants to a new identifier
        let mut unique_chat_identifier = 0;

        // Iterate over the values in a deterministic order
        let mut sorted_dupes: Vec<(&i32, &BTreeSet<i32>)> = duplicated_data.iter().collect();
        sorted_dupes.sort_by(|(a, _), (b, _)| a.cmp(b));

        // Map each chat ID to the deduplicated unique chat ID
        for (chat_id, participants) in sorted_dupes {
            // If this set of participants has already been seen, map to the existing unique chat ID
            if let Some(id) = participants_to_unique_chat_id.get(participants) {
                deduplicated_chats.insert(chat_id.to_owned(), id.to_owned());
            } else {
                // If `chat_lookup` exists, map to canonical chat ID
                let mapped_id = if let Some(canonical_chat) = chat_lookup_map.get(chat_id) {
                    canonical_chat
                } else {
                    chat_id
                };

                // Check if the mapped ID has already been seen
                if let Some(id) = deduplicated_chats.get(mapped_id) {
                    // Map to the existing unique chat ID
                    deduplicated_chats.insert(*chat_id, id.to_owned());
                } else {
                    // New set of participants, assign a new unique chat ID
                    participants_to_unique_chat_id
                        .insert(participants.to_owned(), unique_chat_identifier);

                    // Map chat ID to unique chat ID
                    deduplicated_chats.insert(chat_id.to_owned(), unique_chat_identifier);
                    unique_chat_identifier += 1;
                }
            }
        }
        Ok(deduplicated_chats)
    }
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use crate::tables::chat_handle::ChatToHandle;
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

        let output = ChatToHandle::dedupe(&input, &HashMap::new());
        let expected_deduped_ids: HashSet<i32> = output.unwrap().values().copied().collect();
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

        let output = ChatToHandle::dedupe(&input, &HashMap::new());
        let expected_deduped_ids: HashSet<i32> = output.unwrap().values().copied().collect();
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

        let mut output_1 = ChatToHandle::dedupe(&input_1, &HashMap::new())
            .unwrap()
            .into_iter()
            .collect::<Vec<(i32, i32)>>();
        let mut output_2 = ChatToHandle::dedupe(&input_2, &HashMap::new())
            .unwrap()
            .into_iter()
            .collect::<Vec<(i32, i32)>>();
        let mut output_3 = ChatToHandle::dedupe(&input_3, &HashMap::new())
            .unwrap()
            .into_iter()
            .collect::<Vec<(i32, i32)>>();

        output_1.sort_unstable();
        output_2.sort_unstable();
        output_3.sort_unstable();

        assert_eq!(output_1, output_2);
        assert_eq!(output_1, output_3);
        assert_eq!(output_2, output_3);
    }

    #[test]
    fn can_dedupe_with_chat_lookup_map() {
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(0, BTreeSet::from([1])); // Canonical 0
        input.insert(1, BTreeSet::from([1])); // Maps to 0
        input.insert(2, BTreeSet::from([3])); // Maps to 5
        input.insert(4, BTreeSet::from([2])); // Maps to 0
        input.insert(5, BTreeSet::from([1])); // Canonical 5

        let mut chat_lookup_map: HashMap<i32, i32> = HashMap::new();
        chat_lookup_map.insert(2, 5);
        chat_lookup_map.insert(4, 0);

        let output = ChatToHandle::dedupe(&input, &chat_lookup_map).unwrap();

        // Chats 0,1,4 map to 0, so same deduplicated ID
        assert_eq!(output.get(&0), output.get(&1));
        assert_eq!(output.get(&0), output.get(&4));
        // Chat 2 maps to 5, different
        assert_ne!(output.get(&2), output.get(&1));
    }

    #[test]
    fn can_dedupe_with_lookup_map_overriding_participants() {
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(0, BTreeSet::from([1, 2])); // Canonical 0
        input.insert(1, BTreeSet::from([1, 2])); // Maps to 0
        input.insert(2, BTreeSet::from([3, 4])); // Maps to 0
        input.insert(3, BTreeSet::from([1, 2])); // No mapping

        let mut chat_lookup_map: HashMap<i32, i32> = HashMap::new();
        chat_lookup_map.insert(1, 0);
        chat_lookup_map.insert(2, 0);

        let output = ChatToHandle::dedupe(&input, &chat_lookup_map).unwrap();

        // Chats 0,1,2 all map to 0, so same deduplicated ID
        assert_eq!(output.get(&0), output.get(&1));
        assert_eq!(output.get(&0), output.get(&2));
        // Chat 3 no mapping, same participants as 0 and 1, so same
        assert_eq!(output.get(&3), output.get(&0));
    }

    #[test]
    fn can_dedupe_mixed_lookup_and_participants() {
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(0, BTreeSet::from([1])); // Canonical 0
        input.insert(1, BTreeSet::from([1])); // Maps to 0
        input.insert(2, BTreeSet::from([3])); // No mapping
        input.insert(3, BTreeSet::from([2])); // Maps to 0
        input.insert(4, BTreeSet::from([3])); // No mapping

        let mut chat_lookup_map: HashMap<i32, i32> = HashMap::new();
        chat_lookup_map.insert(1, 0);
        chat_lookup_map.insert(3, 0);

        let output = ChatToHandle::dedupe(&input, &chat_lookup_map).unwrap();

        // Chats 0,1,3 map to 0, same ID
        assert_eq!(output.get(&0), output.get(&1));
        assert_eq!(output.get(&0), output.get(&3));
        // Chat 2 no mapping, different from 1
        assert_ne!(output.get(&2), output.get(&1));
        // Chat 4 same participants as 2, same as 2
        assert_eq!(output.get(&4), output.get(&2));
        // 3 and 4 different
        assert_ne!(output.get(&3), output.get(&4));
    }
}
