use rusqlite::{Connection, Result, Row, Statement};
use std::collections::{HashMap, HashSet};

use crate::tables::table::{Table, HANDLE, ME};

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
    /// Get the number of handles that are duplicated
    /// The person_centric_id is used to map handles that represent the
    /// same contact across ids (numbers, emails, etc) and across
    /// services (iMessage, Jabber, iChat, SMS, etc)
    fn get_merged_handle_count(db: &Connection) -> Result<i32> {
        let query = concat!(
            "SELECT COUNT(DISTINCT person_centric_id) ",
            "FROM handle ",
            "WHERE person_centric_id NOT NULL"
        );
        let mut rows = db.prepare(query).unwrap();
        rows.query_row([], |r| r.get(0))
    }

    fn get_person_id_map(db: &Connection) -> HashMap<i32, String> {
        let mut person_to_id: HashMap<String, HashSet<String>> = HashMap::new();
        let mut row_to_id = HashMap::new();
        let mut row_data: Vec<(String, i32, String)> = vec![];

        // Build query
        let query = concat!(
            "SELECT DISTINCT A.person_centric_id, A.rowid, A.id ",
            "FROM handle A ",
            "INNER JOIN handle B ON B.id = A.id ",
            "WHERE A.person_centric_id NOT NULL ",
            "ORDER BY A.person_centric_id",
        );
        let mut rows = db.prepare(query).unwrap();

        // Cache the results of the query in memory
        let contacts = rows
            .query_map([], |row| {
                let person_centric_id: String = row.get(0).unwrap();
                let rowid: i32 = row.get(1).unwrap();
                let id: String = row.get(2).unwrap();
                Ok((person_centric_id, rowid, id))
            })
            .unwrap();

        for contact in contacts {
            match contact {
                Ok(tup) => {
                    row_data.push(tup);
                }
                Err(why) => {
                    panic!("{:?}", why);
                }
            }
        }

        // First pass: generate a map of each person_centric_id to its matching ids
        for contact in &row_data {
            let (person_centric_id, _, id) = contact;
            match person_to_id.get_mut(person_centric_id) {
                Some(set) => {
                    set.insert(id.to_owned());
                }
                None => {
                    let mut set = HashSet::new();
                    set.insert(id.to_owned());
                    person_to_id.insert(person_centric_id.to_owned(), set);
                }
            }
        }

        // Second pass: point each ROWID to the matching ids
        for contact in &row_data {
            let (person_centric_id, rowid, _) = contact;
            let data_to_insert = match person_to_id.get_mut(person_centric_id) {
                Some(person) => person
                    .to_owned()
                    .into_iter()
                    .collect::<Vec<String>>()
                    .join(" "),
                None => panic!("Attempted to resolve contact with no person_centric_id!"),
            };
            row_to_id.insert(rowid.to_owned(), data_to_insert);
        }
        row_to_id
    }

    /// Generate a HashMap for looking up contacts by their IDs, collapsing
    /// duplicate contacts to the same ID String regardless of service
    pub fn make_cache(db: &Connection) -> HashMap<i32, String> {
        // Create cache for user IDs
        let mut map = HashMap::new();
        // Handle ID 0 is self in group chats
        map.insert(0, ME.to_string());

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

        // Condense contacts that share person_centric_id so their IDs map to the same strings
        println!(
            "Contacts with more than one ID: {}",
            Handle::get_merged_handle_count(db).unwrap()
        );
        let dupe_contacts = Handle::get_person_id_map(db);
        for contact in dupe_contacts {
            let (id, new) = contact;
            map.insert(id, new);
        }

        // Done!
        map
    }
}

// TODO: implement this sql somehow
