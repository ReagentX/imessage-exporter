/*!
 This module represents common (but not all) columns in the `chat` table.
*/

use std::collections::HashMap;

use rusqlite::{Connection, Error, Result, Row, Statement};

use crate::{
    error::table::TableError,
    tables::table::{Cacheable, Table, CHAT},
};

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
            display_name: row.get(3).unwrap_or(None),
        })
    }

    fn get(db: &Connection) -> Statement {
        db.prepare(&format!("SELECT {COLUMNS} from {}", CHAT))
            .unwrap()
    }

    fn extract(chat: Result<Result<Self, Error>, Error>) -> Result<Self, TableError> {
        match chat {
            Ok(chat) => match chat {
                Ok(ch) => Ok(ch),
                // TODO: When does this occur?
                Err(why) => Err(TableError::Chat(why)),
            },
            // TODO: When does this occur?
            Err(why) => Err(TableError::Chat(why)),
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
    fn cache(db: &Connection) -> Result<HashMap<Self::K, Self::V>, TableError> {
        let mut map = HashMap::new();

        let mut statement = Chat::get(db);

        let chats = statement
            .query_map([], |row| Ok(Chat::from_row(row)))
            .unwrap();

        for chat in chats {
            let result = Chat::extract(chat)?;
            map.insert(result.rowid, result);
        }
        Ok(map)
    }
}

impl Chat {
    pub fn name(&self) -> &str {
        match &self.display_name() {
            Some(name) => name,
            None => &self.chat_identifier,
        }
    }

    pub fn display_name(&self) -> Option<&str> {
        match &self.display_name {
            Some(name) => {
                if !name.is_empty() {
                    return Some(name.as_str());
                }
                None
            }
            None => None,
        }
    }
}
