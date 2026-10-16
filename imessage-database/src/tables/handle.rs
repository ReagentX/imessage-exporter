/*!
 Handle table rows and handle grouping helpers.
*/

use rusqlite::{CachedStatement, Connection, Result, Row};
use std::collections::{
    BTreeSet, HashMap,
    hash_map::Entry::{Occupied, Vacant},
};

use crate::{
    error::table::TableError,
    tables::{
        diagnostic::{HandleDiagnostic, column_exists, count_query},
        table::{Cacheable, HANDLE, ME, Table},
    },
    util::union_find::UnionFind,
};

// MARK: Handle
/// Row from the `handle` table.
#[derive(Debug)]
pub struct Handle {
    /// Handle row ID.
    pub rowid: i32,
    /// Phone number, email address, or service identifier.
    pub id: String,
    /// Legacy merge ID that the iMessage server used to identify handles for
    /// one sender account.
    pub person_centric_id: Option<String>,
}

// MARK: Table
impl Table for Handle {
    fn from_row(row: &Row) -> Result<Handle> {
        Ok(Handle {
            rowid: row.get("rowid")?,
            id: row.get("id")?,
            person_centric_id: row.get("person_centric_id").unwrap_or(None),
        })
    }

    fn get(db: &'_ Connection) -> Result<CachedStatement<'_>, TableError> {
        Ok(db.prepare_cached(&format!("SELECT * from {HANDLE}"))?)
    }
}

// MARK: Cache
impl Cacheable for Handle {
    type K = i32;
    type V = String;
    /// Cache handle identifiers by row ID.
    ///
    /// In group chats, treat row ID `0` as the local user and map it to [`ME`].
    /// Use [`HandleGroups`] to group rows that belong to one person.
    ///
    /// # Example:
    ///
    /// ```no_run
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Cacheable, get_connection};
    /// use imessage_database::tables::handle::Handle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let handles = Handle::cache(&conn);
    /// ```
    fn cache(db: &Connection) -> Result<HashMap<Self::K, Self::V>, TableError> {
        let mut map = HashMap::new();
        map.insert(0, ME.to_string());

        let mut statement = Handle::get(db)?;
        for handle in Handle::rows(&mut statement, [])? {
            let handle = handle?;
            map.insert(handle.rowid, handle.id);
        }

        Ok(map)
    }
}

// MARK: Groups
/// Groups of `handle` rows that belong to one person.
///
/// We group rows when their `id` values match byte-for-byte (one address
/// registered on several services) or their `person_centric_id` values match.
/// Call [`HandleGroups::union`] to add relationships from outside the database,
/// such as contact matches. Call [`HandleGroups::finalize`] to assign one
/// sequential group ID to each class.
///
/// Compare stored `id` values without normalization. Messages lowercases email
/// addresses and canonicalizes phone numbers before storing them. Do not match
/// phone numbers on digits alone; you could combine a short code with a full
/// number that shares its suffix.
///
/// The iMessage server attached `person_centric_id` to incoming messages as a
/// merge ID, but stopped issuing it for new senders long ago. Current databases
/// retain this field only for the oldest handles. Keep a present ID as a
/// grouping edge; it remains reliable even though current databases rarely
/// include it.
///
/// # Example:
///
/// ```no_run
/// use imessage_database::util::dirs::default_db_path;
/// use imessage_database::tables::table::get_connection;
/// use imessage_database::tables::handle::HandleGroups;
///
/// let db_path = default_db_path();
/// let conn = get_connection(&db_path).unwrap();
/// let mut groups = HandleGroups::from_db(&conn).unwrap();
/// groups.union(3, 7);
/// let group_by_handle = groups.finalize();
/// ```
#[derive(Debug)]
pub struct HandleGroups {
    sets: UnionFind,
    /// Store table row IDs plus `0` in sorted order for stable group assignment
    /// across runs.
    rowids: BTreeSet<i32>,
}

impl HandleGroups {
    /// Seed groups from every row of the `handle` table.
    ///
    /// Include row ID `0` as its own group. Chat participant lists can use it
    /// for the local user.
    pub fn from_db(db: &Connection) -> Result<Self, TableError> {
        let mut sets = UnionFind::new();
        let mut rowids = BTreeSet::new();

        sets.make_set(0);
        rowids.insert(0);

        let mut first_by_id: HashMap<String, i32> = HashMap::new();
        let mut first_by_person: HashMap<String, i32> = HashMap::new();

        let mut statement = Handle::get(db)?;
        for handle in Handle::rows(&mut statement, [])? {
            let handle = handle?;
            sets.make_set(handle.rowid);
            rowids.insert(handle.rowid);

            if !handle.id.is_empty() {
                match first_by_id.entry(handle.id) {
                    Occupied(first) => sets.union(handle.rowid, *first.get()),
                    Vacant(slot) => {
                        slot.insert(handle.rowid);
                    }
                }
            }

            if let Some(person) = handle.person_centric_id.filter(|p| !p.is_empty()) {
                match first_by_person.entry(person) {
                    Occupied(first) => sets.union(handle.rowid, *first.get()),
                    Vacant(slot) => {
                        slot.insert(handle.rowid);
                    }
                }
            }
        }

        Ok(Self { sets, rowids })
    }

    /// Merge the groups containing two handle row IDs.
    ///
    /// Pass row IDs from the table. Unknown IDs have no effect, so an external
    /// source cannot create a phantom group with a stale ID.
    pub fn union(&mut self, a: i32, b: i32) {
        if self.rowids.contains(&a) && self.rowids.contains(&b) {
            self.sets.union(a, b);
        }
    }

    /// Total handle rows seeded from the table, excluding row ID `0`.
    pub fn len(&self) -> usize {
        self.rowids.len() - 1
    }

    /// Returns `true` when the `handle` table had no rows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Assign a sequential group ID to each class and return it keyed by handle row ID.
    ///
    /// Order classes by their smallest row ID before assigning sequential IDs.
    /// This keeps results stable for an unchanged table, even when union-find
    /// picks a different representative.
    pub fn finalize(mut self) -> HashMap<i32, i32> {
        let mut group_by_handle: HashMap<i32, i32> = HashMap::with_capacity(self.rowids.len());
        let mut group_by_root: HashMap<i32, i32> = HashMap::new();
        let mut next_id = 0;

        for rowid in &self.rowids {
            let root = self.sets.find(*rowid);
            let group = *group_by_root.entry(root).or_insert_with(|| {
                let id = next_id;
                next_id += 1;
                id
            });
            group_by_handle.insert(*rowid, group);
        }

        group_by_handle
    }
}

// MARK: Diagnostic
impl Handle {
    /// Compute diagnostic data for the `handle` table.
    ///
    /// Group rows with [`HandleGroups`], then report the extra rows absorbed
    /// into groups and the number of groups with multiple rows. Count
    /// `person_centric_id` values separately when the column exists.
    ///
    /// # Example:
    ///
    /// ```no_run
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::get_connection;
    /// use imessage_database::tables::handle::Handle;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// Handle::run_diagnostic(&conn);
    /// ```
    pub fn run_diagnostic(db: &Connection) -> Result<HandleDiagnostic, TableError> {
        let query = concat!(
            "SELECT COUNT(DISTINCT person_centric_id) ",
            "FROM handle ",
            "WHERE person_centric_id NOT NULL AND person_centric_id != ''"
        );

        let person_centric_ids = if column_exists(db, HANDLE, "person_centric_id")? {
            Some(count_query(db, query)?)
        } else {
            None
        };

        let groups = HandleGroups::from_db(db)?;
        let total_handles = groups.len();

        // Exclude synthetic row 0 from group sizes; it has no handle row.
        let mut rows_per_group: HashMap<i32, usize> = HashMap::new();
        for (rowid, group) in groups.finalize() {
            if rowid != 0 {
                *rows_per_group.entry(group).or_insert(0) += 1;
            }
        }

        Ok(HandleDiagnostic {
            total_handles,
            person_centric_ids,
            multi_handle_groups: rows_per_group.values().filter(|rows| **rows > 1).count(),
            merged_handles: total_handles - rows_per_group.len(),
        })
    }
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use crate::tables::handle::{Handle, HandleGroups};
    use rusqlite::Connection;
    use std::collections::{HashMap, HashSet};

    /// Build an in-memory `handle` table from `(rowid, id, service, person_centric_id)` rows.
    fn db_with(rows: &[(i32, &str, &str, Option<&str>)]) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE handle (
                ROWID INTEGER PRIMARY KEY,
                id TEXT NOT NULL,
                service TEXT NOT NULL,
                person_centric_id TEXT
            )",
            [],
        )
        .unwrap();
        for (rowid, id, service, person) in rows {
            db.execute(
                "INSERT INTO handle (ROWID, id, service, person_centric_id) VALUES (?1, ?2, ?3, ?4)",
                (rowid, id, service, person),
            )
            .unwrap();
        }
        db
    }

    fn groups_of(map: &HashMap<i32, i32>, rowids: &[i32]) -> HashSet<i32> {
        rowids.iter().map(|r| map[r]).collect()
    }

    #[test]
    fn identical_ids_across_services_share_a_group() {
        let db = db_with(&[
            (1, "+15551234567", "iMessage", None),
            (2, "+15551234567", "SMS", None),
            (3, "+15551234567", "RCS", None),
            (4, "a@example.com", "iMessage", None),
        ]);
        let map = HandleGroups::from_db(&db).unwrap().finalize();

        assert_eq!(groups_of(&map, &[1, 2, 3]).len(), 1);
        assert_ne!(map[&1], map[&4]);
        assert_eq!(map[&0], 0, "the local user is group 0");
        assert_eq!(map.len(), 5);
    }

    #[test]
    fn ids_are_not_normalized() {
        let db = db_with(&[
            (1, "+15551234567", "iMessage", None),
            (2, "15551234567", "SMS", None),
            (3, "A@example.com", "iMessage", None),
            (4, "a@example.com", "iMessage", None),
        ]);
        let map = HandleGroups::from_db(&db).unwrap().finalize();

        assert_ne!(map[&1], map[&2]);
        assert_ne!(map[&3], map[&4]);
    }

    #[test]
    fn person_centric_id_joins_different_ids() {
        let db = db_with(&[
            (1, "+15551234567", "iMessage", Some("p1")),
            (2, "a@example.com", "iMessage", Some("p1")),
            (3, "b@example.com", "iMessage", Some("p2")),
            (4, "+15559876543", "iMessage", None),
        ]);
        let map = HandleGroups::from_db(&db).unwrap().finalize();

        assert_eq!(map[&1], map[&2]);
        assert_ne!(map[&1], map[&3]);
        assert_ne!(map[&1], map[&4]);
    }

    #[test]
    fn edges_are_transitive() {
        // Group rows 1, 2, and 3: connect 1 to 2 by person_centric_id, then 2
        // to 3 by id.
        let db = db_with(&[
            (1, "+15551234567", "iMessage", Some("p1")),
            (2, "a@example.com", "iMessage", Some("p1")),
            (3, "a@example.com", "SMS", None),
        ]);
        let map = HandleGroups::from_db(&db).unwrap().finalize();

        assert_eq!(groups_of(&map, &[1, 2, 3]).len(), 1);
    }

    #[test]
    fn empty_person_centric_id_is_not_an_edge() {
        let db = db_with(&[
            (1, "+15551234567", "iMessage", Some("")),
            (2, "a@example.com", "iMessage", Some("")),
        ]);
        let map = HandleGroups::from_db(&db).unwrap().finalize();

        assert_ne!(map[&1], map[&2]);
    }

    #[test]
    fn external_union_merges_groups() {
        let db = db_with(&[
            (1, "+15551234567", "iMessage", None),
            (2, "a@example.com", "iMessage", None),
            (3, "b@example.com", "iMessage", None),
        ]);
        let mut groups = HandleGroups::from_db(&db).unwrap();
        groups.union(1, 2);
        let map = groups.finalize();

        assert_eq!(map[&1], map[&2]);
        assert_ne!(map[&1], map[&3]);
    }

    #[test]
    fn external_union_ignores_unknown_rowids() {
        let db = db_with(&[(1, "+15551234567", "iMessage", None)]);
        let mut groups = HandleGroups::from_db(&db).unwrap();
        groups.union(1, 99);
        groups.union(98, 99);
        let map = groups.finalize();

        assert_eq!(map.len(), 2);
        assert!(!map.contains_key(&99));
    }

    #[test]
    fn group_ids_are_sequential_in_rowid_order() {
        let db = db_with(&[
            (5, "c", "iMessage", None),
            (3, "b", "iMessage", None),
            (9, "b", "SMS", None),
            (1, "a", "iMessage", None),
        ]);
        let map = HandleGroups::from_db(&db).unwrap().finalize();

        assert_eq!(map[&0], 0);
        assert_eq!(map[&1], 1);
        assert_eq!(map[&3], 2);
        assert_eq!(map[&5], 3);
        assert_eq!(map[&9], 2);
    }

    #[test]
    fn finalize_is_stable_across_runs() {
        let rows = [
            (1, "+15551234567", "iMessage", Some("p1")),
            (2, "+15551234567", "SMS", Some("p1")),
            (3, "a@example.com", "iMessage", Some("p1")),
            (4, "b@example.com", "iMessage", None),
            (5, "b@example.com", "SMS", None),
            (6, "+15559876543", "iMessage", None),
        ];
        let mut runs: Vec<Vec<(i32, i32)>> = (0..3)
            .map(|_| {
                let db = db_with(&rows);
                HandleGroups::from_db(&db)
                    .unwrap()
                    .finalize()
                    .into_iter()
                    .collect()
            })
            .collect();
        for run in &mut runs {
            run.sort_unstable();
        }

        assert_eq!(runs[0], runs[1]);
        assert_eq!(runs[1], runs[2]);
    }

    #[test]
    fn diagnostic_omits_person_centric_count_when_column_is_missing() {
        let db = Connection::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT NOT NULL)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO handle (ROWID, id) VALUES (1, 'first'), (2, 'second')",
            [],
        )
        .unwrap();

        let diagnostic = Handle::run_diagnostic(&db).unwrap();

        assert_eq!(diagnostic.total_handles, 2);
        assert_eq!(diagnostic.person_centric_ids, None);
        assert_eq!(diagnostic.multi_handle_groups, 0);
        assert_eq!(diagnostic.merged_handles, 0);
    }

    #[test]
    fn diagnostic_counts_groups_and_merged_rows() {
        let db = db_with(&[
            (1, "+15551234567", "iMessage", Some("p1")),
            (2, "+15551234567", "SMS", Some("p1")),
            (3, "a@example.com", "iMessage", Some("p1")),
            (4, "b@example.com", "iMessage", None),
            (5, "b@example.com", "SMS", None),
            (6, "+15559876543", "iMessage", Some("p2")),
        ]);

        let diagnostic = Handle::run_diagnostic(&db).unwrap();

        assert_eq!(diagnostic.total_handles, 6);
        assert_eq!(diagnostic.person_centric_ids, Some(2));
        // {1,2,3}, {4,5}, {6}
        assert_eq!(diagnostic.multi_handle_groups, 2);
        assert_eq!(diagnostic.merged_handles, 3);
    }

    #[test]
    fn diagnostic_ignores_empty_person_centric_ids() {
        let db = db_with(&[
            (1, "a@example.com", "iMessage", Some("")),
            (2, "b@example.com", "iMessage", Some("")),
        ]);

        let diagnostic = Handle::run_diagnostic(&db).unwrap();

        assert_eq!(diagnostic.person_centric_ids, Some(0));
        assert_eq!(diagnostic.multi_handle_groups, 0);
        assert_eq!(diagnostic.merged_handles, 0);
    }
}
