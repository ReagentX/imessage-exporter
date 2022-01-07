use rusqlite::{Connection, Result, Row, Statement};
use std::collections::HashMap;

use crate::tables::table::{Table, HANDLE};

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
    fn from_row(row: &Row) -> Result<Handle> {
        Ok(Handle {
            rowid: row.get(0)?,
            id: row.get(1)?,
            country: row.get(2)?,
            service: row.get(3)?,
            uncanonicalized_id: row.get(4)?,
            person_centric_id: row.get(5)?,
        })
    }

    fn get(db: &Connection) -> Statement {
        db.prepare(&format!("SELECT * from {}", HANDLE)).unwrap()
    }
}

impl Handle {
    fn get_merged_handle_count(db: &Connection) -> Result<i32> {
        let query = concat!(
            "SELECT COUNT(DISTINCT person_centric_id) ",
            "FROM handle ",
            "WHERE person_centric_id NOT NULL"
        );
        let mut rows = db.prepare(query).unwrap();
        rows.query_row([], |r| r.get(0))
    }
    /// Generate a HashMap for looking up contacts by their IDs, collapsing
    /// duplicate contacts to the same ID String
    pub fn make_cache(db: &Connection) -> HashMap<i32, String> {
        // Create cache for user IDs
        let mut map = HashMap::new();

        // Condense contacts that share person_centric_id so their IDs map to the same strings
        let mut dupe_contacts: HashMap<String, Vec<String>> = HashMap::new();
        println!(
            "Contacts with more than one ID: {}",
            Handle::get_merged_handle_count(db).unwrap()
        );

        // Create query
        let mut statement = Handle::get(db);

        // Execute query to build the Handles
        let handles = statement
            .query_map([], |row| Ok(Handle::from_row(row)))
            .unwrap();

        // Iterate over the handles and update the map
        for handle in handles {
            let contact = handle.unwrap().unwrap();
            map.insert(contact.rowid, contact.id);
        }

        // TODO: Map contacts to the unique `person_centric_id` if it exists somehow?

        // Done!
        map
    }
}

// TODO: implement this sql somehow
