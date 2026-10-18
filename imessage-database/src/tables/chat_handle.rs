/*!
 Chat-to-handle join table helpers.
*/

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    error::table::TableError,
    tables::{
        chat::Chat,
        diagnostic::{ChatHandleDiagnostic, column_exists},
        handle::HandleGroups,
        table::{CHAT_HANDLE_JOIN, CHAT_MESSAGE_JOIN, MESSAGE, Table},
    },
    util::union_find::UnionFind,
};
use rusqlite::{CachedStatement, Connection, Result, Row};

// MARK: Struct
/// Row from the `chat_handle_join` table.
#[derive(Debug)]
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
}

// MARK: Rows
impl ChatToHandle {
    /// Prepare the `SELECT *` statement for the `chat_handle_join` table.
    pub fn get(db: &'_ Connection) -> Result<CachedStatement<'_>, TableError> {
        Ok(db.prepare_cached(&format!("SELECT * FROM {CHAT_HANDLE_JOIN}"))?)
    }

    /// Map participant handle IDs by chat row ID.
    ///
    /// # Example:
    ///
    /// ```no_run
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::get_connection;
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let chatrooms = ChatToHandle::participants_by_chat(&conn);
    /// ```
    pub fn participants_by_chat(
        db: &Connection,
    ) -> Result<HashMap<i32, BTreeSet<i32>>, TableError> {
        let mut cache: HashMap<i32, BTreeSet<i32>> = HashMap::new();

        let mut rows = ChatToHandle::get(db)?;
        for mapping in ChatToHandle::rows(&mut rows, [])? {
            let joiner = mapping?;
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

/// Participant identity used when comparing chats.
///
/// Use separate variants to distinguish handle row IDs from group IDs:
/// both are represented as `i32`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Participant {
    /// Handle group from [`HandleGroups::finalize`].
    Group(i32),
    /// Handle row with no entry in the group map.
    Handle(i32),
}

/// `true` when the chat row has a display name.
fn named(chats: &HashMap<i32, Chat>, chat_id: i32) -> bool {
    chats.get(&chat_id).and_then(Chat::display_name).is_some()
}

// MARK: Diagnostic
impl ChatToHandle {
    /// Compute diagnostic data for the `chat_handle_join` table.
    ///
    /// Counts chats referenced by messages but missing from this join table.
    ///
    /// # Example:
    ///
    /// ```no_run
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::get_connection;
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// ChatToHandle::run_diagnostic(&conn);
    /// ```
    pub fn run_diagnostic(db: &Connection) -> Result<ChatHandleDiagnostic, TableError> {
        // Chats referenced by messages.
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

        // Chats with associated handles.
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
        let all_chats = Self::participants_by_chat(db)?;

        // Cache chatroom participants
        let chat_handle_lookup = ChatToHandle::get_chat_lookup_map(db)?;
        let handle_groups = HandleGroups::from_db(db)?.finalize();

        // Deduplicate chatroom participants
        let chats = Chat::by_rowid(db)?;
        let real_chatrooms =
            ChatToHandle::dedupe(&all_chats, &chat_handle_lookup, &handle_groups, &chats);

        // Count only chats with handle rows in both totals. Include lookup-only
        // components in `real_chatrooms` for routing, but exclude them here to
        // match `Total chats`.
        let conversations: HashSet<&i32> = all_chats
            .keys()
            .filter_map(|chat_id| real_chatrooms.get(chat_id))
            .collect();
        let total_duplicated = all_chats.len() - conversations.len();

        // Find the set difference
        let chats_with_no_handles = unique_chats_from_messages
            .difference(&unique_chats_from_handles)
            .count();

        Ok(ChatHandleDiagnostic {
            total_chats: all_chats.len(),
            total_duplicated,
            chats_with_no_handles,
        })
    }
}

impl ChatToHandle {
    /// Load the `chat_lookup` canonical chat map when the table exists.
    ///
    /// The map links chat IDs split across services to a canonical chat ID for
    /// deduplication.
    ///
    /// # Example:
    ///
    /// ```no_run
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::get_connection;
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// ChatToHandle::get_chat_lookup_map(&conn);
    /// ```
    pub fn get_chat_lookup_map(conn: &Connection) -> Result<HashMap<i32, i32>, TableError> {
        // `chat_lookup` links chats split across services.
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

    /// Assign stable deduplicated IDs to chats.
    ///
    /// Use handle groups rather than handle row IDs to compare participants.
    /// Merge chats connected through `chat_lookup`, even with different names.
    /// For rows with matching participant groups:
    ///
    /// - Merge named rows only with matching display names.
    /// - Attach unnamed rows to the lowest-row-ID named row with the same
    ///   participant groups.
    /// - For an unnamed row linked to a named row through `chat_lookup`, use
    ///   that conversation when attaching other unnamed rows with matching
    ///   participant groups. Preserve existing assignments to other named
    ///   conversations from `chat_lookup`.
    /// - Without a matching named row, merge the unnamed rows.
    ///
    /// Assign an ID to every row referenced in `chat_lookup_map`, including
    /// rows without handle rows, to route their messages to the conversation.
    /// Assign a separate ID to each component made entirely of such rows.
    ///
    /// - `duplicated_data`: raw participant handle IDs keyed by chat row ID
    /// - `chat_lookup_map`: canonical chat ID keyed by each related chat ID,
    ///   from [`ChatToHandle::get_chat_lookup_map`]
    /// - `handle_groups`: group ID keyed by each handle row ID, from [`HandleGroups::finalize`];
    ///   an unmapped handle keeps its own identity
    /// - `chats`: chat rows keyed by row ID; treat a missing row as unnamed
    ///
    /// Assuming no new chat-handle relationships have been written to the database, deduplicated data is deterministic across runs.
    ///
    /// # Example:
    ///
    /// ```no_run
    /// use std::collections::HashMap;
    ///
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::get_connection;
    /// use imessage_database::tables::chat::Chat;
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    /// use imessage_database::tables::handle::HandleGroups;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let chatrooms = ChatToHandle::participants_by_chat(&conn).unwrap();
    /// let handle_groups = HandleGroups::from_db(&conn).unwrap().finalize();
    /// let chats = Chat::by_rowid(&conn).unwrap();
    /// let deduped_chatrooms = ChatToHandle::dedupe(&chatrooms, &HashMap::new(), &handle_groups, &chats);
    /// ```
    pub fn dedupe(
        duplicated_data: &HashMap<i32, BTreeSet<i32>>,
        chat_lookup_map: &HashMap<i32, i32>,
        handle_groups: &HashMap<i32, i32>,
        chats: &HashMap<i32, Chat>,
    ) -> HashMap<i32, i32> {
        let mut uf = UnionFind::new();

        for chat_id in duplicated_data.keys() {
            uf.make_set(*chat_id);
        }

        // Merge chats that the chat_lookup table says are the same conversation,
        // iterating in sorted order so union-by-rank picks deterministic representatives.
        // Union every link, including rows absent from duplicated_data: a chat
        // with no handle rows still bridges the rows that map to it, and its
        // display name still counts for the conversation.
        let mut sorted_lookup: Vec<(&i32, &i32)> = chat_lookup_map.iter().collect();
        sorted_lookup.sort_by_key(|(chat_id, _)| *chat_id);
        for (chat_id, canonical) in sorted_lookup {
            uf.union(*chat_id, *canonical);
        }

        // Merge chats that share the same participant set, iterating in
        // sorted order so union-by-rank picks deterministic representatives
        let mut keyed: Vec<(i32, BTreeSet<Participant>, Option<&str>)> = duplicated_data
            .iter()
            .map(|(chat_id, participants)| {
                let key = participants
                    .iter()
                    .map(|handle_id| match handle_groups.get(handle_id) {
                        Some(group) => Participant::Group(*group),
                        None => Participant::Handle(*handle_id),
                    })
                    .collect();
                let name = chats.get(chat_id).and_then(Chat::display_name);
                (*chat_id, key, name)
            })
            .collect();
        keyed.sort_by_key(|(id, _, _)| *id);

        // Merge rows that share participants and a name. Then attach unnamed
        // rows to the lowest-ID named row with the same participants.
        let mut first_named: HashMap<(&BTreeSet<Participant>, &str), i32> = HashMap::new();
        let mut anchor: HashMap<&BTreeSet<Participant>, i32> = HashMap::new();
        for (chat_id, key, name) in &keyed {
            let Some(name) = name else { continue };
            match first_named.get(&(key, *name)) {
                Some(first_chat) => uf.union(*chat_id, *first_chat),
                None => {
                    first_named.insert((key, name), *chat_id);
                }
            }
            anchor.entry(key).or_insert(*chat_id);
        }

        // Preserve named conversation assignments from `chat_lookup`: do not
        // merge two named components merely for matching participant groups.
        // Check every linked row for a name, including those without handle
        // rows and therefore absent from `keyed`. For an unnamed row already
        // grouped with a named row, merge any earlier unnamed anchor into that
        // component, then attach later unnamed rows with matching participants.
        // Record each merged root to recognize named membership through
        // subsequent links.
        let mut lookup_members: Vec<i32> = chat_lookup_map
            .iter()
            .flat_map(|(chat_id, canonical)| [*chat_id, *canonical])
            .collect();
        lookup_members.sort_unstable();
        lookup_members.dedup();
        let mut named_roots: HashSet<i32> = keyed
            .iter()
            .filter(|(_, _, name)| name.is_some())
            .map(|(chat_id, _, _)| *chat_id)
            .chain(
                lookup_members
                    .iter()
                    .copied()
                    .filter(|chat_id| named(chats, *chat_id)),
            )
            .map(|chat_id| uf.find(chat_id))
            .collect();
        for (chat_id, key, name) in &keyed {
            if name.is_some() {
                continue;
            }
            let row_named = named_roots.contains(&uf.find(*chat_id));
            match anchor.get(key) {
                Some(first_chat) => {
                    let anchor_named = named_roots.contains(&uf.find(*first_chat));
                    if row_named && anchor_named {
                        continue;
                    }
                    uf.union(*chat_id, *first_chat);
                    if row_named || anchor_named {
                        named_roots.insert(uf.find(*chat_id));
                    }
                }
                None => {
                    anchor.insert(key, *chat_id);
                }
            }
        }

        // Label keyed rows in chat ID order before labeling lookup-only rows,
        // also in chat ID order, to preserve IDs for keyed components. Assign
        // a separate ID to each lookup-only component without keyed members.
        let lookup_only = lookup_members
            .iter()
            .copied()
            .filter(|member| !duplicated_data.contains_key(member));
        uf.label(keyed.iter().map(|(id, _, _)| *id).chain(lookup_only))
    }
}

// MARK: Representatives
impl ChatToHandle {
    /// Return the latest message date for each chat row ID.
    ///
    /// We query `chat_message_join.message_date` when that column exists.
    /// Otherwise, we join `chat_message_join` to `message` and read
    /// `message.date`. This fallback is slower, but we can use it with every
    /// supported schema. We omit chats without messages from the result.
    ///
    /// # Example:
    ///
    /// ```no_run
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::get_connection;
    /// use imessage_database::tables::chat_handle::ChatToHandle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let last_activity = ChatToHandle::get_last_activity(&conn);
    /// ```
    pub fn get_last_activity(db: &Connection) -> Result<HashMap<i32, i64>, TableError> {
        let query = if column_exists(db, CHAT_MESSAGE_JOIN, "message_date")? {
            format!("SELECT chat_id, MAX(message_date) FROM {CHAT_MESSAGE_JOIN} GROUP BY chat_id")
        } else {
            format!(
                "SELECT j.chat_id, MAX(m.date) FROM {CHAT_MESSAGE_JOIN} j JOIN {MESSAGE} m ON m.ROWID = j.message_id GROUP BY j.chat_id"
            )
        };

        let mut statement = db.prepare(&query)?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i32>(0)?, row.get::<_, Option<i64>>(1)?))
        })?;

        let mut last_activity = HashMap::new();
        for row in rows {
            let (chat_id, date) = row?;
            if let Some(date) = date {
                last_activity.insert(chat_id, date);
            }
        }
        Ok(last_activity)
    }

    /// Choose one chat row to represent each deduplicated conversation.
    ///
    /// Account for different `display_name` values within a group:
    ///
    /// 1. Use the most recently active row if named.
    /// 2. Otherwise, use the named row with the lowest ID.
    /// 3. For an entirely unnamed group, use the most recently active row with
    ///    participant rows.
    /// 4. For an unnamed group without participant rows, use the most recently
    ///    active row.
    ///
    /// Prefer rows with a name or participants for building filenames; use an
    /// unnamed lookup-only row only for a group without either. When viewing
    /// a message in Messages.app, you see the name stored on its chat row.
    ///
    /// - `deduplicated_chats`: map from each chat row ID to its deduplicated ID, from [`ChatToHandle::dedupe`]
    /// - `chats`: chat rows keyed by row ID; treat a missing row as unnamed
    /// - `last_activity`: map from each chat row ID to its latest message date, from [`ChatToHandle::get_last_activity`]
    /// - `participants_by_chat`: raw participant handle IDs keyed by chat row ID, from [`ChatToHandle::participants_by_chat`]
    /// - Returns: representative chat row ID for each input chat row ID
    pub fn representatives(
        deduplicated_chats: &HashMap<i32, i32>,
        chats: &HashMap<i32, Chat>,
        last_activity: &HashMap<i32, i64>,
        participants_by_chat: &HashMap<i32, BTreeSet<i32>>,
    ) -> HashMap<i32, i32> {
        let mut members: HashMap<i32, Vec<i32>> = HashMap::new();
        for (chat_id, dedup_id) in deduplicated_chats {
            members.entry(*dedup_id).or_default().push(*chat_id);
        }

        // Rank by latest activity, breaking ties by lowest row ID to keep the
        // choice stable across runs.
        let activity = |chat_id: &i32| {
            (
                last_activity.get(chat_id).copied().unwrap_or(i64::MIN),
                std::cmp::Reverse(*chat_id),
            )
        };

        let mut representatives = HashMap::with_capacity(deduplicated_chats.len());
        for rows in members.into_values() {
            let most_active = rows
                .iter()
                .copied()
                .max_by_key(activity)
                .expect("a deduplicated group has at least one row");

            let representative = if named(chats, most_active) {
                most_active
            } else {
                rows.iter()
                    .copied()
                    .filter(|chat_id| named(chats, *chat_id))
                    .min()
                    .or_else(|| {
                        rows.iter()
                            .copied()
                            .filter(|chat_id| participants_by_chat.contains_key(chat_id))
                            .max_by_key(activity)
                    })
                    .unwrap_or(most_active)
            };

            for chat_id in rows {
                representatives.insert(chat_id, representative);
            }
        }

        representatives
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

        let output =
            ChatToHandle::dedupe(&input, &HashMap::new(), &HashMap::new(), &HashMap::new());
        let expected: HashMap<i32, i32> =
            HashMap::from([(1, 0), (2, 0), (3, 0), (4, 1), (5, 1), (6, 2)]);
        assert_eq!(output, expected);
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

        let output =
            ChatToHandle::dedupe(&input, &HashMap::new(), &HashMap::new(), &HashMap::new());
        let expected: HashMap<i32, i32> =
            HashMap::from([(1, 0), (2, 1), (3, 1), (4, 0), (5, 2), (6, 3)]);
        assert_eq!(output, expected);
    }

    #[test]
    fn can_dedupe_with_chat_lookup_map() {
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(0, BTreeSet::from([1])); // Canonical 0
        input.insert(1, BTreeSet::from([1])); // Maps to 0
        input.insert(2, BTreeSet::from([3])); // Maps to 5
        input.insert(4, BTreeSet::from([2])); // Maps to 0
        input.insert(5, BTreeSet::from([1])); // Canonical 5

        let chat_lookup_map = HashMap::from([(2, 5), (4, 0)]);

        let output =
            ChatToHandle::dedupe(&input, &chat_lookup_map, &HashMap::new(), &HashMap::new());

        // Chats 0,1,5 share participants {1}, and chat 2 maps to 5 via lookup,
        // and chat 4 maps to 0 via lookup. All are the same conversation
        assert_eq!(output.get(&0), output.get(&1));
        assert_eq!(output.get(&0), output.get(&4));
        assert_eq!(output.get(&0), output.get(&5));
        assert_eq!(output.get(&2), output.get(&5));
    }

    #[test]
    fn can_dedupe_with_lookup_map_overriding_participants() {
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(0, BTreeSet::from([1, 2])); // Canonical 0
        input.insert(1, BTreeSet::from([1, 2])); // Maps to 0
        input.insert(2, BTreeSet::from([3, 4])); // Maps to 0
        input.insert(3, BTreeSet::from([1, 2])); // No mapping

        let chat_lookup_map = HashMap::from([(1, 0), (2, 0)]);

        let output =
            ChatToHandle::dedupe(&input, &chat_lookup_map, &HashMap::new(), &HashMap::new());

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

        let chat_lookup_map = HashMap::from([(1, 0), (3, 0)]);

        let output =
            ChatToHandle::dedupe(&input, &chat_lookup_map, &HashMap::new(), &HashMap::new());

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

    #[test]
    fn lookup_merge_is_order_independent() {
        // Chats linked through `chat_lookup` must land in one conversation no
        // matter which linked chat is canonical. The SQL picks MAX(root), so
        // the higher-ID cases match production.
        let cases = [
            (
                "canonical is the lower id",
                HashMap::from([(0, BTreeSet::from([1, 2])), (5, BTreeSet::from([3, 4]))]),
                HashMap::from([(0, 0), (5, 0)]),
                HashMap::from([(0, 0), (5, 0)]),
            ),
            (
                "canonical is the higher id",
                HashMap::from([(0, BTreeSet::from([1, 2])), (5, BTreeSet::from([3, 4]))]),
                HashMap::from([(0, 5), (5, 5)]),
                HashMap::from([(0, 0), (5, 0)]),
            ),
            (
                "iMessage, SMS, and RCS splits share one canonical",
                HashMap::from([
                    (100, BTreeSet::from([10])),
                    (200, BTreeSet::from([20])),
                    (300, BTreeSet::from([30])),
                ]),
                HashMap::from([(100, 300), (200, 300), (300, 300)]),
                HashMap::from([(100, 0), (200, 0), (300, 0)]),
            ),
        ];
        for (label, input, chat_lookup_map, expected) in cases {
            let output =
                ChatToHandle::dedupe(&input, &chat_lookup_map, &HashMap::new(), &HashMap::new());
            assert_eq!(output, expected, "{label}");
        }
    }

    #[test]
    fn lookup_merges_through_missing_canonical() {
        // Chat 99 has no handle rows, so it is absent from duplicated_data.
        // Map chats 10 and 20 to the same canonical ID, 99; keep them in one group.
        let input = HashMap::from([(10, BTreeSet::from([1])), (20, BTreeSet::from([2]))]);

        let chat_lookup_map = HashMap::from([(10, 99), (20, 99)]);

        let output =
            ChatToHandle::dedupe(&input, &chat_lookup_map, &HashMap::new(), &HashMap::new());

        // Keep both chats in one group even though their canonical chat is absent.
        assert_eq!(
            output.get(&10),
            output.get(&20),
            "Chats sharing a canonical absent from duplicated_data should still merge"
        );
        // Route the canonical chat's messages to the same conversation, even
        // without handle rows.
        assert_eq!(output.len(), 3);
        assert_eq!(output.get(&99), output.get(&10));
    }

    #[test]
    fn lookup_component_without_handle_rows_gets_its_own_id() {
        let input = HashMap::from([(10, BTreeSet::from([1]))]);

        let chat_lookup_map = HashMap::from([(10, 20), (30, 40)]);

        let output =
            ChatToHandle::dedupe(&input, &chat_lookup_map, &HashMap::new(), &HashMap::new());

        assert_eq!(output.len(), 4);
        assert_eq!(output.get(&10), Some(&0));
        assert_eq!(output.get(&20), Some(&0));
        assert_eq!(output.get(&30), Some(&1));
        assert_eq!(output.get(&40), Some(&1));
    }

    #[test]
    fn chats_differing_only_by_grouped_handles_merge() {
        // Treat handles 1 and 2 as one person because they use the same number
        // on iMessage and SMS; handle 3 belongs to someone else.
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(10, BTreeSet::from([1, 3]));
        input.insert(20, BTreeSet::from([2, 3]));
        input.insert(30, BTreeSet::from([1, 2, 3]));

        let groups: HashMap<i32, i32> = HashMap::from([(1, 0), (2, 0), (3, 1)]);

        let output = ChatToHandle::dedupe(&input, &HashMap::new(), &groups, &HashMap::new());

        assert_eq!(output.get(&10), output.get(&20));
        assert_eq!(
            output.get(&10),
            output.get(&30),
            "a chat listing both of a person's handles is the same participant set"
        );
    }

    #[test]
    fn chats_differing_by_group_stay_separate() {
        let input = HashMap::from([(10, BTreeSet::from([1, 3])), (20, BTreeSet::from([2, 3]))]);

        let groups: HashMap<i32, i32> = HashMap::from([(1, 0), (2, 5), (3, 1)]);

        let output = ChatToHandle::dedupe(&input, &HashMap::new(), &groups, &HashMap::new());

        assert_ne!(output.get(&10), output.get(&20));
    }

    #[test]
    fn ungrouped_handle_does_not_collide_with_group_id() {
        // Handle 7 is absent from the group map; group 7 exists for handle 1.
        // Keep the chat with raw handle 7 separate from the chat whose member
        // belongs to group 7.
        let input = HashMap::from([(10, BTreeSet::from([1])), (20, BTreeSet::from([7]))]);

        let groups: HashMap<i32, i32> = HashMap::from([(1, 7)]);

        let output = ChatToHandle::dedupe(&input, &HashMap::new(), &groups, &HashMap::new());

        assert_ne!(output.get(&10), output.get(&20));
    }

    #[test]
    fn grouped_merge_is_transitive_with_lookup() {
        // Merge 10 and 20 because their participants share a handle group.
        // Then add a chat_lookup edge from 30 to 20.
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(10, BTreeSet::from([1]));
        input.insert(20, BTreeSet::from([2]));
        input.insert(30, BTreeSet::from([9]));

        let groups: HashMap<i32, i32> = HashMap::from([(1, 0), (2, 0), (9, 4)]);
        let lookup: HashMap<i32, i32> = HashMap::from([(30, 20), (20, 20)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &groups, &HashMap::new());

        let unique: HashSet<i32> = output.values().copied().collect();
        assert_eq!(unique.len(), 1);
    }

    // MARK: Representatives
    fn chat(rowid: i32, name: Option<&str>) -> crate::tables::chat::Chat {
        crate::tables::chat::Chat {
            rowid,
            chat_identifier: String::new(),
            service_name: None,
            display_name: name.map(str::to_string),
            is_filtered: None,
            is_blackholed: None,
            is_pending_review: None,
        }
    }

    #[test]
    fn representative_is_the_most_active_named_row() {
        let dedup: HashMap<i32, i32> = HashMap::from([(10, 0), (20, 0), (30, 1)]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, Some("Old Name"))),
            (20, chat(20, Some("New Name"))),
            (30, chat(30, None)),
        ]);
        let activity: HashMap<i32, i64> = HashMap::from([(10, 100), (20, 200)]);

        let reps = ChatToHandle::representatives(&dedup, &chats, &activity, &HashMap::new());

        assert_eq!(reps[&10], 20);
        assert_eq!(reps[&20], 20);
        assert_eq!(reps[&30], 30);
    }

    #[test]
    fn unnamed_active_row_defers_to_a_named_sibling() {
        // Use the older named row because the newest row has no group name.
        let dedup: HashMap<i32, i32> = HashMap::from([(10, 0), (20, 0), (25, 0)]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, Some("Sardegnas"))),
            (20, chat(20, Some(""))),
            (25, chat(25, None)),
        ]);
        let activity: HashMap<i32, i64> = HashMap::from([(10, 100), (20, 300), (25, 200)]);

        let reps = ChatToHandle::representatives(&dedup, &chats, &activity, &HashMap::new());

        assert_eq!(reps[&10], 10);
        assert_eq!(reps[&20], 10);
        assert_eq!(reps[&25], 10);
    }

    #[test]
    fn unnamed_group_uses_its_most_active_row_with_lowest_id_tiebreak() {
        let dedup: HashMap<i32, i32> = HashMap::from([(10, 0), (20, 0), (30, 0)]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, None)),
            (20, chat(20, None)),
            (30, chat(30, None)),
        ]);
        let activity: HashMap<i32, i64> = HashMap::from([(20, 500), (30, 500)]);

        let reps = ChatToHandle::representatives(&dedup, &chats, &activity, &HashMap::new());

        assert_eq!(reps[&10], 20);
        assert_eq!(reps[&20], 20);
        assert_eq!(reps[&30], 20);
    }

    #[test]
    fn unnamed_group_representative_has_participant_rows() {
        let dedup: HashMap<i32, i32> = HashMap::from([(10, 0), (20, 0), (99, 0)]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, None)),
            (20, chat(20, None)),
            (99, chat(99, None)),
        ]);
        let activity: HashMap<i32, i64> = HashMap::from([(10, 100), (20, 200), (99, 300)]);
        let participants: HashMap<i32, BTreeSet<i32>> =
            HashMap::from([(10, BTreeSet::from([1])), (20, BTreeSet::from([1]))]);

        let reps = ChatToHandle::representatives(&dedup, &chats, &activity, &participants);

        assert_eq!(reps[&10], 20);
        assert_eq!(reps[&20], 20);
        assert_eq!(reps[&99], 20);
    }

    #[test]
    fn diagnostic_counts_duplicates_over_chats_with_handle_rows() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE chat (ROWID INTEGER PRIMARY KEY, chat_identifier TEXT, service_name TEXT, display_name TEXT);
             CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT, person_centric_id TEXT);
             CREATE TABLE chat_handle_join (chat_id INTEGER, handle_id INTEGER);
             CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);
             CREATE TABLE chat_lookup (chat INTEGER, identifier TEXT);
             INSERT INTO chat VALUES (10, 'a', 'iMessage', NULL), (30, 'b', 'iMessage', NULL), (40, 'c', 'SMS', NULL);
             INSERT INTO handle VALUES (1, '+15555550100', NULL);
             INSERT INTO chat_handle_join VALUES (10, 1);
             INSERT INTO chat_message_join VALUES (10, 1), (30, 2);
             INSERT INTO chat_lookup VALUES (30, 'x'), (40, 'x');",
        )
        .unwrap();

        let diagnostic = ChatToHandle::run_diagnostic(&db).unwrap();

        assert_eq!(diagnostic.total_chats, 1);
        assert_eq!(diagnostic.total_duplicated, 0);
        assert_eq!(diagnostic.chats_with_no_handles, 1);
    }

    #[test]
    fn last_activity_reads_message_date_when_present() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER, message_date INTEGER DEFAULT 0);
             INSERT INTO chat_message_join VALUES (1, 1, 10), (1, 2, 30), (2, 3, 20);",
        )
        .unwrap();

        let activity = ChatToHandle::get_last_activity(&db).unwrap();

        assert_eq!(activity[&1], 30);
        assert_eq!(activity[&2], 20);
    }

    #[test]
    fn last_activity_falls_back_to_message_table() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);
             CREATE TABLE message (ROWID INTEGER PRIMARY KEY, date INTEGER);
             INSERT INTO message VALUES (1, 10), (2, 30), (3, 20);
             INSERT INTO chat_message_join VALUES (1, 1), (1, 2), (2, 3);",
        )
        .unwrap();

        let activity = ChatToHandle::get_last_activity(&db).unwrap();

        assert_eq!(activity[&1], 30);
        assert_eq!(activity[&2], 20);
    }

    // MARK: Display names
    #[test]
    fn same_people_with_different_names_are_different_groups() {
        let input = HashMap::from([(10, BTreeSet::from([1, 2])), (20, BTreeSet::from([1, 2]))]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, Some("Sunday Call"))),
            (20, chat(20, Some("Da Boys"))),
        ]);

        let output = ChatToHandle::dedupe(&input, &HashMap::new(), &HashMap::new(), &chats);

        assert_ne!(output.get(&10), output.get(&20));
    }

    #[test]
    fn same_people_with_the_same_name_merge() {
        let input = HashMap::from([(10, BTreeSet::from([1, 2])), (20, BTreeSet::from([1, 2]))]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, Some("Family"))),
            (20, chat(20, Some("Family"))),
        ]);

        let output = ChatToHandle::dedupe(&input, &HashMap::new(), &HashMap::new(), &chats);

        assert_eq!(output.get(&10), output.get(&20));
    }

    #[test]
    fn unnamed_row_joins_the_first_named_row_with_its_people() {
        let input = HashMap::from([10, 20, 30, 40].map(|id| (id, BTreeSet::from([1, 2]))));
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, Some("Sunday Call"))),
            (20, chat(20, Some("Da Boys"))),
            (30, chat(30, None)),
            (40, chat(40, Some(""))),
        ]);

        let output = ChatToHandle::dedupe(&input, &HashMap::new(), &HashMap::new(), &chats);

        assert_eq!(output.get(&30), output.get(&10));
        assert_eq!(output.get(&40), output.get(&10));
        assert_ne!(output.get(&30), output.get(&20));
    }

    #[test]
    fn unnamed_rows_merge_with_each_other_when_nothing_is_named() {
        let input = HashMap::from([(10, BTreeSet::from([1, 2])), (20, BTreeSet::from([1, 2]))]);
        let chats: HashMap<i32, crate::tables::chat::Chat> =
            HashMap::from([(10, chat(10, None)), (20, chat(20, None))]);

        let output = ChatToHandle::dedupe(&input, &HashMap::new(), &HashMap::new(), &chats);

        assert_eq!(output.get(&10), output.get(&20));
    }

    #[test]
    fn unnamed_row_keeps_its_chat_lookup_conversation() {
        let input = HashMap::from([10, 20, 30].map(|id| (id, BTreeSet::from([1, 2]))));
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, Some("Alpha"))),
            (20, chat(20, Some("Beta"))),
            (30, chat(30, None)),
        ]);
        let lookup: HashMap<i32, i32> = HashMap::from([(30, 20)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &HashMap::new(), &chats);

        assert_eq!(output.get(&30), output.get(&20));
        assert_ne!(output.get(&10), output.get(&20));
    }

    #[test]
    fn unnamed_row_linked_to_an_unnamed_row_still_joins_a_named_row() {
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(10, BTreeSet::from([1, 2]));
        input.insert(30, BTreeSet::from([1, 2]));
        input.insert(40, BTreeSet::from([1, 2, 3]));
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, Some("Alpha"))),
            (30, chat(30, None)),
            (40, chat(40, None)),
        ]);
        let lookup: HashMap<i32, i32> = HashMap::from([(40, 30)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &HashMap::new(), &chats);

        assert_eq!(output.get(&30), output.get(&10));
        assert_eq!(output.get(&40), output.get(&10));
    }

    #[test]
    fn unnamed_anchor_does_not_block_later_unnamed_merges() {
        let mut input: HashMap<i32, BTreeSet<i32>> = HashMap::new();
        input.insert(10, BTreeSet::from([1]));
        input.insert(20, BTreeSet::from([1]));
        input.insert(30, BTreeSet::from([2]));
        input.insert(40, BTreeSet::from([2]));
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, None)),
            (20, chat(20, None)),
            (30, chat(30, None)),
            (40, chat(40, None)),
        ]);
        let lookup: HashMap<i32, i32> = HashMap::from([(20, 30)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &HashMap::new(), &chats);

        assert_eq!(output.get(&10), output.get(&20));
        assert_eq!(output.get(&20), output.get(&30));
        assert_eq!(output.get(&30), output.get(&40));
    }

    #[test]
    fn named_lookup_canonical_without_handle_rows_still_counts_as_named() {
        let input = HashMap::from([(10, BTreeSet::from([1, 2])), (20, BTreeSet::from([1, 2]))]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, Some("Alpha"))),
            (20, chat(20, None)),
            (99, chat(99, Some("Beta"))),
        ]);
        let lookup: HashMap<i32, i32> = HashMap::from([(20, 99)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &HashMap::new(), &chats);

        assert_ne!(output.get(&10), output.get(&20));
    }

    #[test]
    fn lookup_members_without_handle_rows_join_the_conversation() {
        let input = HashMap::from([(10, BTreeSet::from([1])), (20, BTreeSet::from([1]))]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, Some("Beta"))),
            (20, chat(20, None)),
            (88, chat(88, Some("Alpha"))),
            (99, chat(99, None)),
        ]);
        let lookup: HashMap<i32, i32> = HashMap::from([(20, 99), (88, 99), (99, 99)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &HashMap::new(), &chats);

        assert_ne!(output.get(&10), output.get(&20));
        // Route messages from both linked rows to the conversation, even
        // without handle rows.
        assert_eq!(output.get(&88), output.get(&20));
        assert_eq!(output.get(&99), output.get(&20));
    }

    #[test]
    fn unnamed_rows_join_a_sibling_linked_to_a_named_row() {
        let input = HashMap::from([(10, BTreeSet::from([1])), (20, BTreeSet::from([1]))]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, None)),
            (20, chat(20, None)),
            (88, chat(88, Some("Alpha"))),
        ]);
        let lookup: HashMap<i32, i32> = HashMap::from([(10, 88), (88, 88)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &HashMap::new(), &chats);

        assert_eq!(output.get(&10), output.get(&20));
        assert_eq!(output.get(&10), output.get(&88));
    }

    #[test]
    fn linked_row_pulls_in_an_earlier_unnamed_anchor() {
        let input = HashMap::from([(10, BTreeSet::from([1])), (20, BTreeSet::from([1]))]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, None)),
            (20, chat(20, None)),
            (88, chat(88, Some("Alpha"))),
        ]);
        let lookup: HashMap<i32, i32> = HashMap::from([(20, 88), (88, 88)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &HashMap::new(), &chats);

        assert_eq!(output.get(&10), output.get(&20));
        assert_eq!(output.get(&10), output.get(&88));
    }

    #[test]
    fn unlinked_row_joins_the_lower_of_two_linked_conversations() {
        let input = HashMap::from([10, 20, 30].map(|id| (id, BTreeSet::from([1]))));
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, None)),
            (20, chat(20, None)),
            (30, chat(30, None)),
            (88, chat(88, Some("Alpha"))),
            (99, chat(99, Some("Beta"))),
        ]);
        let lookup: HashMap<i32, i32> = HashMap::from([(10, 88), (88, 88), (20, 99), (99, 99)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &HashMap::new(), &chats);

        assert_ne!(output.get(&10), output.get(&20));
        assert_eq!(output.get(&30), output.get(&10));
        assert_eq!(output.get(&88), output.get(&10));
        assert_eq!(output.get(&99), output.get(&20));
    }

    #[test]
    fn lookup_only_names_reach_the_representative() {
        let input = HashMap::from([(10, BTreeSet::from([1])), (20, BTreeSet::from([1]))]);
        let chats: HashMap<i32, crate::tables::chat::Chat> = HashMap::from([
            (10, chat(10, None)),
            (20, chat(20, None)),
            (88, chat(88, Some("Alpha"))),
            (99, chat(99, Some("Beta"))),
        ]);
        let lookup: HashMap<i32, i32> = HashMap::from([(10, 88), (88, 88), (20, 99), (99, 99)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &HashMap::new(), &chats);
        let representatives =
            ChatToHandle::representatives(&output, &chats, &HashMap::new(), &input);

        assert_ne!(output.get(&10), output.get(&20));
        assert_eq!(representatives.get(&10), Some(&88));
        assert_eq!(representatives.get(&20), Some(&99));
    }

    #[test]
    fn chat_lookup_still_joins_differently_named_rows() {
        // Honor Apple's `chat_lookup` link even when display names differ.
        let input = HashMap::from([(10, BTreeSet::from([1])), (20, BTreeSet::from([2]))]);
        let chats: HashMap<i32, crate::tables::chat::Chat> =
            HashMap::from([(10, chat(10, Some("A"))), (20, chat(20, Some("B")))]);
        let lookup: HashMap<i32, i32> = HashMap::from([(10, 20), (20, 20)]);

        let output = ChatToHandle::dedupe(&input, &lookup, &HashMap::new(), &chats);

        assert_eq!(output.get(&10), output.get(&20));
    }
}
