use std::collections::{HashMap, HashSet};

use crate::tables::table::{Table, CHAT_HANDLE_JOIN};
use rusqlite::{Connection, Result, Row, Statement};

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
}

impl ChatToHandle {
    pub fn build_cache(db: &Connection) -> HashMap<i32, HashSet<i32>> {
        let mut cache: HashMap<i32, HashSet<i32>> = HashMap::new();

        let mut rows = ChatToHandle::get(db);
        let mappings = rows
            .query_map([], |row| Ok(ChatToHandle::from_row(row)))
            .unwrap();

        for mapping in mappings {
            let joiner = mapping.unwrap().unwrap();
            match cache.get_mut(&joiner.chat_id) {
                Some(handles) => {
                    handles.insert(joiner.handle_id);
                }
                None => {
                    let mut data_to_cache = HashSet::new();
                    data_to_cache.insert(joiner.handle_id);
                    cache.insert(joiner.chat_id, data_to_cache);
                }
            }
        }

        cache
    }
}
