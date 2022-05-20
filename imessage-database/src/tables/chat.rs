/*!
 This module represents common (but not all) columns in the `chat` table. 
*/

use std::collections::{BTreeSet, HashMap};

use crate::tables::table::{Cacheable, Deduplicate, Table, CHAT};
use rusqlite::{Connection, Error, Result, Row, Statement};

const COLUMNS: &str = "ROWID, chat_identifier, service_name, display_name";

/// Represents a single row in the `chat` table.
#[derive(Debug)]
pub struct Chat {
    pub rowid: i32,
    pub chat_identifier: String,
    pub service_name: String,
    pub display_name: Option<String>,
}

impl Table for Chat {
    fn from_row(row: &Row) -> Result<Chat> {
        Ok(Chat {
            rowid: row.get(0)?,
            chat_identifier: row.get(1)?,
            service_name: row.get(2)?,
            display_name: row.get(3)?,
        })
    }

    fn get(db: &Connection) -> Statement {
        db.prepare(&format!("SELECT {COLUMNS} from {}", CHAT))
            .unwrap()
    }

    fn extract(chat: Result<Result<Self, Error>, Error>) -> Self {
        match chat {
            Ok(chat) => match chat {
                Ok(ch) => ch,
                // TODO: When does this occur?
                Err(why) => panic!("Inner error: {}", why),
            },
            // TODO: When does this occur?
            Err(why) => panic!("Outer error: {}", why),
        }
    }
}

impl Cacheable for Chat {
    type K = i32;
    type V = Chat;
    /// Generate a hashmap containing each chatroom's ID pointing to the chatroom's metadata.
    ///
    /// These chatroom ID's contain duplicates and must be deduped later once we have all of
    /// the participants parsed out. On its own this data is not useful.
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Cacheable, get_connection};
    /// use imessage_database::tables::chat::Chat;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path);
    /// let chatrooms = Chat::cache(&conn);
    /// ```
    fn cache(db: &Connection) -> HashMap<Self::K, Self::V> {
        let mut map = HashMap::new();

        let mut statement = Chat::get(db);

        let chats = statement
            .query_map([], |row| Ok(Chat::from_row(row)))
            .unwrap();

        for chat in chats {
            let result = Chat::extract(chat);
            map.insert(result.rowid, result);
        }
        map
    }
}

impl Deduplicate for Chat {
    type T = BTreeSet<i32>;

    /// Given the initial set of duplciated chats, deduplciate them based on the participants
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

impl Chat {
    pub fn name(&self) -> &str {
        match &self.display_name {
            Some(name) => {
                if name.is_empty() {
                    &self.chat_identifier
                } else {
                    name
                }
            }
            None => &self.chat_identifier,
        }
    }
}
