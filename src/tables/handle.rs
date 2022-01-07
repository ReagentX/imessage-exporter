use rusqlite::Connection;
use std::collections::HashMap;

use crate::tables::table::{Table, HANDLE};
use rusqlite::{Result, Row};

#[derive(Debug)]
pub struct Handle {
    pub rowid: i32,
    pub id: String,
    pub country: String,
    pub service: String,
    pub uncanonicalized_id: Option<String>,
    pub person_centric_id: Option<String>,
}

impl Table for Handle {
    type A = Handle;

    fn from_row(row: &Row) -> Result<Self::A> {
        Ok(Handle {
            rowid: row.get(0)?,
            id: row.get(1)?,
            country: row.get(2)?,
            service: row.get(3)?,
            uncanonicalized_id: row.get(4)?,
            person_centric_id: row.get(5)?,
        })
    }
}

impl Handle {
    pub fn get_map(db: &Connection) -> HashMap<i32, String> {
        // Create cache for user IDs
        let mut map = HashMap::new();

        // Create query
        let mut statement = db
            .prepare(&format!("SELECT * from {}", HANDLE))
            .unwrap();

        // Execute query to build the Handles
        let handles = statement
            .query_map([], |row| Ok(Handle::from_row(row)))
            .unwrap();

        // Iterate over the handles and update the map
        for handle in handles {
            let contact = handle.unwrap().unwrap();
            map.insert(contact.rowid, contact.id);
        }

        // Done!
        map
    }
}
