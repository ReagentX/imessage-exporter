use std::collections::{BTreeSet, HashMap};

use crate::tables::table::{Cacheable, Table, CHAT_HANDLE_JOIN};
use rusqlite::{Connection, Error, Result, Row, Statement};

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

    fn extract(chat_to_handle: Result<Result<Self, Error>, Error>) -> Self {
        match chat_to_handle {
            Ok(chat_to_handle) => match chat_to_handle {
                Ok(c2h) => c2h,
                // TODO: When does this occur?
                Err(why) => panic!("Inner error: {}", why),
            },
            // TODO: When does this occur?
            Err(why) => panic!("Outer error: {}", why),
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
    fn cache(db: &Connection) -> HashMap<Self::K, Self::V> {
        let mut cache: HashMap<i32, BTreeSet<i32>> = HashMap::new();

        let mut rows = ChatToHandle::get(db);
        let mappings = rows
            .query_map([], |row| Ok(ChatToHandle::from_row(row)))
            .unwrap();

        for mapping in mappings {
            let joiner = ChatToHandle::extract(mapping);
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

        cache
    }

    // TODO: Implement Diagnostic, determine how many chats do not exist in the join table
}
