use std::{
    collections::{
        BTreeSet, HashMap, HashSet,
        hash_map::Entry::{Occupied, Vacant},
    },
    fs,
    path::{Path, PathBuf},
};

use imessage_database::{
    error::table::TableError,
    tables::{handle::HandleGroups, table::get_connection},
    util::dirs::home,
};
use rusqlite::{Connection, Result};

/// Default contacts database path inside an iOS backup.
pub const DEFAULT_PATH_IOS: &str = "31/31bb7ba8914766d4ba40d6dfb6113c8b614be442";

/// Minimum digits required to index a phone number.
const MIN_PHONE_DIGITS: usize = 7;

/// Core Data entity names whose `ZABCDRECORD` rows are people. The other entities in that table
/// are groups, containers, and bookkeeping.
const MACOS_CONTACT_ENTITIES: [&str; 2] = ["ABCDContact", "ABCDSubscribedContact"];

/// Fallback `Z_ENT` values observed for [`MACOS_CONTACT_ENTITIES`] in every macOS model so far.
const MACOS_CONTACT_ENTITY_FALLBACK: [i64; 2] = [22, 23];

/// `ABMultiValue.property` of a phone number.
const IOS_PROPERTY_PHONE: i64 = 3;

/// `ABMultiValue.property` of an email address.
const IOS_PROPERTY_EMAIL: i64 = 4;

/// Sentinel value for an unset `ABPerson.PersonLink` or `ABStore.MeIdentifier`.
const IOS_NONE: i64 = -1;

// MARK: Name
#[derive(Clone, Debug, PartialEq, Eq)]
/// Contact name and the Messages handle IDs associated with it.
pub struct Name {
    /// First name.
    pub first: String,
    /// Last name.
    pub last: String,
    /// Full name.
    pub full: String,
    /// Handle address used for display and filenames.
    pub details: String,
    /// Handle row IDs whose addresses resolve to this contact name.
    pub handle_ids: HashSet<i32>,
    /// The `handle.id` value of each of those handles: a phone number, an email, or a short code.
    pub addresses: BTreeSet<String>,
}

impl Name {
    /// Build a name from optional first and last name fields.
    fn from_opt(first: Option<String>, last: Option<String>) -> Option<Self> {
        // Return None if both are None
        if first.is_none() && last.is_none() {
            return None;
        }

        // Build full name
        let full = format!(
            "{}{}{}",
            first.as_deref().unwrap_or(""),
            if first.is_some() && last.is_some() {
                " "
            } else {
                ""
            },
            last.as_deref().unwrap_or(""),
        );

        Some(Name {
            first: first.unwrap_or_default(),
            last: last.unwrap_or_default(),
            full,
            details: String::new(),
            handle_ids: HashSet::new(),
            addresses: BTreeSet::new(),
        })
    }

    /// Score name completeness: one point each for first and last name.
    fn score(&self) -> u8 {
        u8::from(!self.first.is_empty()) + u8::from(!self.last.is_empty())
    }

    /// `true` when any name field or any address contains the provided string.
    pub fn contains(&self, s: &str) -> bool {
        self.first.contains(s)
            || self.last.contains(s)
            || self.full.contains(s)
            || self.details.contains(s)
            || self.addresses.iter().any(|address| address.contains(s))
    }

    /// Return the full name, falling back to handle details.
    pub fn get_display_name(&self) -> &str {
        if self.full.is_empty() {
            &self.details
        } else {
            &self.full
        }
    }

    /// Build a name that only carries the details string.
    pub fn from_details<D: Into<String>>(details: D) -> Self {
        let details = details.into();
        Name {
            first: String::new(),
            last: String::new(),
            full: String::new(),
            addresses: BTreeSet::from([details.clone()]),
            details,
            handle_ids: HashSet::new(),
        }
    }
}

#[cfg(test)]
impl Name {
    /// Build a fake name for tests.
    pub fn fake_name(name: &str) -> Name {
        Name {
            first: String::new(),
            last: String::new(),
            full: String::new(),
            details: name.to_string(),
            handle_ids: HashSet::new(),
            addresses: BTreeSet::new(),
        }
    }
}

// MARK: Record identity
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Identity of one contact record, or of one set of linked records, inside a [`ContactsIndex`].
///
/// IDs are assigned sequentially within one index. Do not persist them: they have no meaning
/// outside that index and may change even when we read the same files again. When you merge cards
/// in Contacts, the app deletes a row, so source row IDs can change too. We assign one ID to records
/// with the same link (`ZLINKID` on macOS, `PersonLink` on iOS), but never share IDs across files.
pub struct RecordId(u32);

#[derive(Clone, Debug, PartialEq, Eq)]
/// One indexed key, its resolved name, and its owning record.
struct Entry {
    /// Most complete name we found for this key, or `None` when no owning record has either name
    /// field.
    name: Option<Name>,
    /// Owning record, or `None` when multiple records claim the key. We still return a name for an
    /// ambiguous key, but we do not use it to join handles.
    identity: Option<RecordId>,
}

#[derive(Default)]
/// Contact rows from one source file, with resolved identities and names.
///
/// We resolve names and identities from each platform's contact table first, then attach keys from
/// the child table in a second pass.
struct Records {
    /// Identity for each source row.
    by_row: HashMap<i64, RecordId>,
    /// Resolved name for each identity.
    names: HashMap<RecordId, Candidate>,
}

#[derive(Default)]
/// Name chosen so far for one identity.
struct Candidate {
    name: Option<Name>,
    /// `true` after we select the link's preferred member name; we never replace it with a later
    /// row's name.
    preferred: bool,
}

impl Records {
    /// Register `row` under `id`. Keep the link's preferred name when present; otherwise keep the
    /// more complete name and retain the first name on ties. Callers add rows in ascending row
    /// order, so a tie resolves to the lowest row.
    fn add(&mut self, row: i64, id: RecordId, name: Option<Name>, preferred: bool) {
        self.by_row.insert(row, id);
        let candidate = self.names.entry(id).or_default();
        if candidate.preferred {
            return;
        }
        if preferred && name.is_some() {
            candidate.name = name;
            candidate.preferred = true;
            return;
        }
        if let Some(incoming) = name {
            let better = candidate
                .name
                .as_ref()
                .is_none_or(|existing| incoming.score() > existing.score());
            if better {
                candidate.name = Some(incoming);
            }
        }
    }

    /// Return the identity and name for `row`, if we registered it.
    fn get(&self, row: i64) -> Option<(RecordId, Option<&Name>)> {
        let id = *self.by_row.get(&row)?;
        Some((id, self.names.get(&id).and_then(|c| c.name.as_ref())))
    }
}

// MARK: Index
#[derive(Debug, Default)]
/// Contacts index keyed by normalized phone number or email address.
pub struct ContactsIndex {
    /// Entries keyed by normalized phone number or email address.
    index: HashMap<String, Entry>,
    /// Next unassigned [`RecordId`].
    next_id: u32,
}

impl ContactsIndex {
    /// Build a contacts index from one database path or all local macOS sources.
    ///
    /// - If `path` is `Some`, we read only that database.
    /// - If `path` is `None`, we scan macOS Contacts sources under
    ///   `~/Library/Application Support/AddressBook/Sources/*/AddressBook-v22.abcddb`
    ///
    /// We support macOS (`AddressBook-v22.abcddb`) and iOS (`AddressBook.sqlitedb`) databases.
    pub fn build(path: Option<&Path>) -> Result<Self, TableError> {
        let mut index = Self::default();

        if let Some(path) = path {
            let conn = get_connection(path)?;
            index.add_connection(&conn)?;
            return Ok(index);
        }

        // We use one counter for every file, so records from different sources never share an ID.
        for db_path in find_macos_addressbook_db_paths() {
            if let Ok(local_conn) = get_connection(&db_path) {
                index.add_macos(&local_conn)?;
            }
        }

        Ok(index)
    }

    /// Inspect the schema, then add every record with the matching reader: iOS when the search table
    /// exists, macOS otherwise.
    fn add_connection(&mut self, conn: &Connection) -> Result<()> {
        if table_exists(conn, "ABPersonFullTextSearch_content") {
            self.add_ios(conn)
        } else {
            self.add_macos(conn)
        }
    }

    /// Allocate the next identity.
    fn alloc(&mut self) -> RecordId {
        let id = RecordId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Insert `key` as owned by `record`.
    ///
    /// When a second record claims a key, mark it ambiguous and keep it that way. Keep the most
    /// complete name among the records that claim the key.
    fn insert_key(&mut self, key: String, record: RecordId, name: Option<&Name>) {
        match self.index.get_mut(&key) {
            Some(existing) => {
                if existing.identity != Some(record) {
                    existing.identity = None;
                }
                if let Some(incoming) = name {
                    let better = existing
                        .name
                        .as_ref()
                        .is_none_or(|current| incoming.score() > current.score());
                    if better {
                        existing.name = Some(incoming.clone());
                    }
                }
            }
            None => {
                self.index.insert(
                    key,
                    Entry {
                        name: name.cloned(),
                        identity: Some(record),
                    },
                );
            }
        }
    }

    /// Insert `keys` for the record owning source row `row`. We skip me-cards and rows with the
    /// wrong entity before calling this method.
    fn insert_keys(&mut self, records: &Records, row: i64, keys: Vec<String>) {
        if let Some((id, name)) = records.get(row) {
            for key in keys {
                self.insert_key(key, id, name);
            }
        }
    }

    // MARK: macOS
    /// Add every contact from a macOS Contacts database.
    ///
    /// We read `ZABCDRECORD` rows for the contact entity and skip the me-card. We assign one
    /// identity to rows with the same non-NULL `ZLINKID`. Current macOS databases leave `ZLINKID`
    /// null, but older databases may set it.
    fn add_macos(&mut self, conn: &Connection) -> Result<()> {
        let mut records = Records::default();
        let mut by_link: HashMap<String, RecordId> = HashMap::new();

        // Read the contact entity's `Z_ENT` from the model's own table because Core Data assigns
        // entity numbers per model version.
        let entities = macos_contact_entities(conn);
        let mut stmt = conn.prepare(
            "SELECT Z_PK, ZLINKID, ZFIRSTNAME, ZLASTNAME
             FROM ZABCDRECORD
             WHERE Z_ENT IN (?1, ?2) AND ZCONTAINERWHERECONTACTISME IS NULL
             ORDER BY Z_PK",
        )?;
        let mut rows = stmt.query(entities)?;
        while let Some(row) = rows.next()? {
            let pk: i64 = row.get(0)?;
            let link = row
                .get::<_, Option<String>>(1)?
                .filter(|link| !link.is_empty());
            let name = Name::from_opt(row.get(2)?, row.get(3)?);
            let id = match link {
                Some(link) => *by_link.entry(link).or_insert_with(|| self.alloc()),
                None => self.alloc(),
            };
            records.add(pk, id, name, false);
        }

        let mut stmt = conn.prepare(
            "SELECT ZOWNER, ZFULLNUMBER
             FROM ZABCDPHONENUMBER
             WHERE ZOWNER IS NOT NULL AND ZFULLNUMBER IS NOT NULL",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let owner: i64 = row.get(0)?;
            let phone: String = row.get(1)?;
            self.insert_keys(&records, owner, phone_keys(&phone));
        }

        let mut stmt = conn.prepare(
            "SELECT ZOWNER, ZADDRESSNORMALIZED
             FROM ZABCDEMAILADDRESS
             WHERE ZOWNER IS NOT NULL AND ZADDRESSNORMALIZED IS NOT NULL",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let owner: i64 = row.get(0)?;
            let email: String = row.get(1)?;
            // Some email values in macOS databases include angle brackets, such as "<addr@dom>".
            self.insert_keys(&records, owner, parse_email_list(&email));
        }

        Ok(())
    }

    // MARK: iOS
    /// Add every contact from an iOS backup Contacts database.
    ///
    /// We skip each store's me-card and assign one identity to `ABPerson` rows with the same
    /// `PersonLink`. When the preferred member has a name, we use the link's `PreferredNamePersonID`
    /// to select it; otherwise, we keep the most complete member name. We read keys from
    /// `ABMultiValue`, not `ABPersonFullTextSearch_content`: its phone column contains derived
    /// tokens, including the 7-digit local part, that collide across area codes.
    fn add_ios(&mut self, conn: &Connection) -> Result<()> {
        let mut me: HashSet<i64> = HashSet::new();
        let mut stmt = conn.prepare(
            "SELECT MeIdentifier FROM ABStore
             WHERE MeIdentifier IS NOT NULL AND MeIdentifier != ?1",
        )?;
        for row in stmt.query_map([IOS_NONE], |r| r.get::<_, i64>(0))? {
            me.insert(row?);
        }

        let mut preferred: HashMap<i64, i64> = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT ROWID, PreferredNamePersonID FROM ABPersonLink
             WHERE PreferredNamePersonID IS NOT NULL",
        )?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (link, person) = row?;
            preferred.insert(link, person);
        }

        let mut records = Records::default();
        let mut by_link: HashMap<i64, RecordId> = HashMap::new();
        let mut stmt =
            conn.prepare("SELECT ROWID, First, Last, PersonLink FROM ABPerson ORDER BY ROWID")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let row_id: i64 = row.get(0)?;
            if me.contains(&row_id) {
                continue;
            }
            let name = Name::from_opt(row.get(1)?, row.get(2)?);
            let link = row.get::<_, Option<i64>>(3)?.filter(|l| *l != IOS_NONE);
            let id = match link {
                Some(link) => *by_link.entry(link).or_insert_with(|| self.alloc()),
                None => self.alloc(),
            };
            let is_preferred = link.and_then(|l| preferred.get(&l)) == Some(&row_id);
            records.add(row_id, id, name, is_preferred);
        }

        let mut stmt = conn.prepare(
            "SELECT record_id, property, value FROM ABMultiValue
             WHERE property IN (?1, ?2) AND record_id IS NOT NULL AND value IS NOT NULL",
        )?;
        let mut rows = stmt.query([IOS_PROPERTY_PHONE, IOS_PROPERTY_EMAIL])?;
        while let Some(row) = rows.next()? {
            let record_id: i64 = row.get(0)?;
            let property: i64 = row.get(1)?;
            let value: String = row.get(2)?;
            let keys = if property == IOS_PROPERTY_EMAIL {
                normalize_email(&value).into_iter().collect()
            } else {
                phone_keys(&value)
            };
            self.insert_keys(&records, record_id, keys);
        }

        Ok(())
    }

    // MARK: Lookup
    /// Return the first entry that matches `id` and satisfies `accept`.
    ///
    /// Try whitespace-separated tokens in order: look up an email by its normalized address and a
    /// phone by each key variant. We generate no keys for `urn:` business IDs, so they never match.
    fn resolve(&self, id: &str, accept: impl Fn(&Entry) -> bool) -> Option<&Entry> {
        for id_part in id.split_whitespace() {
            if looks_like_email(id_part) {
                if let Some(entry) = normalize_email(id_part)
                    .and_then(|k| self.index.get(&k))
                    .filter(|e| accept(e))
                {
                    return Some(entry);
                }
                continue;
            }

            for k in phone_keys(id_part) {
                if let Some(entry) = self.index.get(&k).filter(|e| accept(e)) {
                    return Some(entry);
                }
            }
        }
        None
    }

    /// Look up a contact name by handle string.
    ///
    /// Skip entries without a name so we can continue searching later key variants and tokens.
    pub fn lookup(&self, id: &str) -> Option<Name> {
        self.resolve(id, |e| e.name.is_some())
            .and_then(|e| e.name.clone())
    }

    /// Look up the contact record owning a handle string.
    ///
    /// Return the first existing key's identity, or `None` when no key exists or that key is
    /// ambiguous.
    pub fn identity(&self, id: &str) -> Option<RecordId> {
        self.resolve(id, |_| true).and_then(|e| e.identity)
    }

    /// Join handles that belong to one contact record.
    ///
    /// For each contact record, union every matching handle with the lowest-row-ID handle that
    /// resolved to that record. Skip addresses claimed by multiple records and row `0` (the local
    /// user).
    ///
    /// - `participants`: handle address keyed by handle row ID, from `Handle::cache`
    /// - `groups`: the groups to add edges to, before `finalize`
    /// - Returns: the number of edges added
    pub fn link_handles(
        &self,
        participants: &HashMap<i32, String>,
        groups: &mut HandleGroups,
    ) -> usize {
        let mut first_by_record: HashMap<RecordId, i32> = HashMap::new();
        let mut edges = 0;

        // Sort by row ID so each record's first handle stays the same across runs.
        let mut handles: Vec<(&i32, &String)> = participants.iter().collect();
        handles.sort_by_key(|(handle_id, _)| **handle_id);

        for (handle_id, address) in handles {
            if *handle_id == 0 {
                continue;
            }
            let Some(record) = self.identity(address) else {
                continue;
            };
            match first_by_record.entry(record) {
                Occupied(first) => {
                    groups.union(*handle_id, *first.get());
                    edges += 1;
                }
                Vacant(slot) => {
                    slot.insert(*handle_id);
                }
            }
        }

        edges
    }

    /// Build names keyed by handle group.
    ///
    /// Use the contact name from the lowest-row-ID handle that matches Contacts. If no handle
    /// matches, use the lowest-row-ID handle's address. Keep that address in `details` so a new
    /// handle with a higher row ID does not change the filename.
    ///
    /// - `participants`: handle address keyed by handle row ID
    /// - `handle_groups`: group ID keyed by handle row ID, from `HandleGroups::finalize`
    /// - Returns: name keyed by group ID
    pub fn build_participants_map(
        &self,
        participants: &HashMap<i32, String>,
        handle_groups: &HashMap<i32, i32>,
    ) -> HashMap<i32, Name> {
        // Sort each group's members by row ID before choosing its name and details.
        let mut members: HashMap<i32, Vec<i32>> = HashMap::new();
        for (&handle_id, &group) in handle_groups {
            if participants.contains_key(&handle_id) {
                members.entry(group).or_default().push(handle_id);
            }
        }

        let mut result: HashMap<i32, Name> = HashMap::with_capacity(members.len());
        for (group, mut handle_ids) in members {
            handle_ids.sort_unstable();
            let first = &participants[&handle_ids[0]];

            let mut name = handle_ids
                .iter()
                .find_map(|handle_id| self.lookup(&participants[handle_id]))
                .unwrap_or_else(|| Name::from_details(first.clone()));

            // Keep the lowest-row-ID handle's address for display and fallback.
            name.details = first.clone();
            name.addresses = handle_ids
                .iter()
                .map(|handle_id| participants[handle_id].clone())
                .collect();
            name.handle_ids = handle_ids.into_iter().collect();
            result.insert(group, name);
        }

        result
    }
}

/// `Z_ENT` values of the person entities in a macOS Contacts database.
///
/// Read each entity's `Z_ENT` from `Z_PRIMARYKEY`, the model table that stores entity names and
/// numbers. When we cannot read the table or it lacks an entity, use the fallback number observed
/// in every model so far. This prevents a missing entity from silently using `0`.
fn macos_contact_entities(conn: &Connection) -> [i64; 2] {
    let mut entities = MACOS_CONTACT_ENTITY_FALLBACK;
    for (slot, name) in entities.iter_mut().zip(MACOS_CONTACT_ENTITIES) {
        if let Ok(ent) = conn.query_row(
            "SELECT Z_ENT FROM Z_PRIMARYKEY WHERE Z_NAME = ?1",
            [name],
            |row| row.get::<_, i64>(0),
        ) {
            *slot = ent;
        }
    }
    entities
}

/// Return whether a table or view exists in the database.
fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name = ?1 LIMIT 1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

// MARK: Email
/// `true` when the identifier looks like an email address.
fn looks_like_email(s: &str) -> bool {
    s.contains('@')
}

/// Normalize an email by trimming, lowercasing, and removing angle brackets.
fn normalize_email(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let s = s.trim_start_matches('<').trim_end_matches('>');
    if s.is_empty() {
        return None;
    }
    Some(s.to_lowercase())
}

/// Parse a space-separated email list.
fn parse_email_list(raw: &str) -> Vec<String> {
    if raw.contains(' ') {
        raw.split_whitespace().filter_map(normalize_email).collect()
    } else {
        normalize_email(raw).into_iter().collect()
    }
}

// MARK: Phone
/// Build lookup keys from a raw phone number.
///
/// - If the number contains "urn:", returns an empty vector
/// - If the number has fewer than [`MIN_PHONE_DIGITS`] digits, returns an empty vector
/// - Returns keys with and without '+' prefix
/// - For US numbers starting with +1 and 11 digits, also adds variants without the `+1` country code
fn phone_keys(raw: &str) -> Vec<String> {
    // Skip iMessage business accounts
    if raw.contains("urn:") {
        return vec![];
    }

    // The digits include the country code portion of the number
    let digits = to_phone_digits(raw);

    // Skip numbers that are too short to be valid phone numbers
    // This prevents matching on country codes alone (e.g., "+1" -> "1")
    if digits.len() < MIN_PHONE_DIGITS {
        return vec![];
    }

    let mut keys = vec![digits.clone(), format!("+{digits}")];

    // US numbers may appear with or without the country code.
    if digits.len() == 11 && raw.starts_with("+1") {
        let last_10 = &digits[digits.len() - 10..];
        keys.push(last_10.to_string());
        keys.push(format!("+{last_10}"));
    }

    keys.dedup();
    keys
}

/// Extract digits from a raw phone number string.
fn to_phone_digits(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_digit() {
            out.push(ch);
        }
    }
    out
}

// MARK: macOS Dirs
/// Scans the macOS Contacts Sources directory (`~/Library/Application Support/AddressBook/Sources`)
/// for AddressBook-v22.abcddb database files.
fn find_macos_addressbook_db_paths() -> Vec<PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = fs::read_dir(macos_sources_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let db_path = path.join("AddressBook-v22.abcddb");
                if db_path.is_file() {
                    results.push(db_path);
                }
            }
        }
    }
    results
}

/// Resolve the standard macOS Contacts Sources directory: `~/Library/Application Support/AddressBook/Sources`
fn macos_sources_dir() -> PathBuf {
    PathBuf::from(&home())
        .join("Library")
        .join("Application Support")
        .join("AddressBook")
        .join("Sources")
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phone_lookup_us_with_country_code_with_plus() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("John".to_string()), Some("Doe".to_string())).unwrap();
        // Simulate building from db with "+12345678901" (US number with +1 and 10 digits)
        let db_phone = "+12345678901";
        let keys = phone_keys(db_phone);
        for key in keys {
            seed(&mut index, key, &name);
        }

        // Lookup with +1 should match
        assert_eq!(index.lookup("+12345678901"), Some(name.clone()));
    }

    #[test]
    fn test_phone_lookup_us_with_country_code_without_plus() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("John".to_string()), Some("Doe".to_string())).unwrap();
        // Simulate building from db with "+12345678901" (US number with +1 and 10 digits)
        let db_phone = "+12345678901";
        let keys = phone_keys(db_phone);
        for key in keys {
            seed(&mut index, key, &name);
        }

        // Lookup without + should match
        assert_eq!(index.lookup("12345678901"), Some(name.clone()));
    }

    #[test]
    fn test_phone_lookup_us_with_country_code_without_plus1() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("John".to_string()), Some("Doe".to_string())).unwrap();
        // Simulate building from db with "+12345678901" (US number with +1 and 10 digits)
        let db_phone = "+12345678901";
        let keys = phone_keys(db_phone);
        for key in keys {
            seed(&mut index, key, &name);
        }

        // Lookup without +1 should match (US variant)
        assert_eq!(index.lookup("2345678901"), Some(name.clone()));
    }

    #[test]
    fn test_phone_lookup_us_with_country_code_with_plus_without1() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("John".to_string()), Some("Doe".to_string())).unwrap();
        // Simulate building from db with "+12345678901" (US number with +1 and 10 digits)
        let db_phone = "+12345678901";
        let keys = phone_keys(db_phone);
        for key in keys {
            seed(&mut index, key, &name);
        }

        // Lookup with + but without 1 should match
        assert_eq!(index.lookup("+2345678901"), Some(name.clone()));
    }

    #[test]
    fn test_phone_lookup_us_without_country_code_with_plus1() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Jane".to_string()), Some("Smith".to_string())).unwrap();
        // Simulate building from db with "1234567890" (no +1)
        let db_phone = "1234567890";
        let keys = phone_keys(db_phone);
        for key in keys {
            seed(&mut index, key, &name);
        }

        // Lookup with +1 should match
        assert_eq!(index.lookup("+1234567890"), Some(name.clone()));
    }

    #[test]
    fn test_phone_lookup_us_without_country_code_without_plus() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Jane".to_string()), Some("Smith".to_string())).unwrap();
        // Simulate building from db with "1234567890" (no +1)
        let db_phone = "1234567890";
        let keys = phone_keys(db_phone);
        for key in keys {
            seed(&mut index, key, &name);
        }

        // Lookup without + should match
        assert_eq!(index.lookup("1234567890"), Some(name.clone()));
    }

    #[test]
    fn test_phone_lookup_us_without_country_code_miss_without_plus() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Jane".to_string()), Some("Smith".to_string())).unwrap();
        // Simulate building from db with "1234567890" (no +1)
        let db_phone = "1234567890";
        let keys = phone_keys(db_phone);
        for key in keys {
            seed(&mut index, key, &name);
        }

        // For 10-digit db entry, no US variants added, so these should not match
        assert_eq!(index.lookup("34567890"), None);
    }

    #[test]
    fn test_phone_lookup_us_without_country_code_miss_with_plus() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Jane".to_string()), Some("Smith".to_string())).unwrap();
        // Simulate building from db with "1234567890" (no +1)
        let db_phone = "1234567890";
        let keys = phone_keys(db_phone);
        for key in keys {
            seed(&mut index, key, &name);
        }

        // For 10-digit db entry, no US variants added, so these should not match
        assert_eq!(index.lookup("+34567890"), None);
    }

    #[test]
    fn test_phone_lookup_uk_with_plus44() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Alice".to_string()), Some("Brown".to_string())).unwrap();
        // UK number +44 20 1234 5678
        seed(&mut index, "+442012345678".to_string(), &name);

        // Lookup with +44 should match
        assert_eq!(index.lookup("+442012345678"), Some(name.clone()));
    }

    #[test]
    fn test_phone_lookup_uk_without_plus() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Alice".to_string()), Some("Brown".to_string())).unwrap();
        // UK number +44 20 1234 5678
        seed(&mut index, "+442012345678".to_string(), &name);

        // Lookup without + should match
        assert_eq!(index.lookup("442012345678"), Some(name.clone()));
    }

    #[test]
    fn test_phone_lookup_miss_without_plus() {
        let index = ContactsIndex::default();
        // No entries, should miss
        assert_eq!(index.lookup("1234567890"), None);
    }

    #[test]
    fn test_phone_lookup_miss_with_plus() {
        let index = ContactsIndex::default();
        // No entries, should miss
        assert_eq!(index.lookup("+1234567890"), None);
    }

    #[test]
    fn test_email_lookup_match_exact() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Steve".to_string()), Some("Jobs".to_string())).unwrap();
        seed(&mut index, "steve@apple.com".to_string(), &name);

        // Exact match
        assert_eq!(index.lookup("steve@apple.com"), Some(name.clone()));
    }

    #[test]
    fn test_email_lookup_match_case_insensitive() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Steve".to_string()), Some("Jobs".to_string())).unwrap();
        seed(&mut index, "steve@apple.com".to_string(), &name);

        // Case insensitive
        assert_eq!(index.lookup("STEVE@APPLE.COM"), Some(name.clone()));
    }

    #[test]
    fn test_email_lookup_match_with_angle_brackets() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Steve".to_string()), Some("Jobs".to_string())).unwrap();
        seed(&mut index, "steve@apple.com".to_string(), &name);

        // With angle brackets
        assert_eq!(index.lookup("<steve@apple.com>"), Some(name.clone()));
    }

    #[test]
    fn test_email_lookup_match_trimmed() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Steve".to_string()), Some("Jobs".to_string())).unwrap();
        seed(&mut index, "steve@apple.com".to_string(), &name);

        // Trimmed
        assert_eq!(index.lookup(" steve@apple.com "), Some(name.clone()));
    }

    #[test]
    fn test_email_lookup_miss_no_entries() {
        let index = ContactsIndex::default();
        // No entries
        assert_eq!(index.lookup("not@here.com"), None);
    }

    #[test]
    fn test_email_lookup_miss_phone_like() {
        let index = ContactsIndex::default();
        // Phone-like but looks like email
        assert_eq!(index.lookup("123@456"), None);
    }

    #[test]
    fn test_phone_keys_contains_digits() {
        // Test phone_keys function
        let keys = phone_keys("+12345678901");
        assert!(keys.contains(&"12345678901".to_string()));
    }

    #[test]
    fn test_phone_keys_contains_with_plus() {
        // Test phone_keys function
        let keys = phone_keys("+12345678901");
        assert!(keys.contains(&"+12345678901".to_string()));
    }

    #[test]
    fn test_phone_keys_contains_last_10() {
        // Test phone_keys function
        let keys = phone_keys("+12345678901");
        assert!(keys.contains(&"2345678901".to_string()));
    }

    #[test]
    fn test_phone_keys_contains_last_10_with_plus() {
        // Test phone_keys function
        let keys = phone_keys("+12345678901");
        assert!(keys.contains(&"+2345678901".to_string()));
    }

    #[test]
    fn test_phone_lookup_with_space_after_country_code() {
        // From issue #671: "+1 5551234567" should match the correct contact
        let mut index = ContactsIndex::default();
        let correct_contact =
            Name::from_opt(Some("Correct".to_string()), Some("Person".to_string())).unwrap();

        // Contact is stored with the full number
        let db_phone = "+15551234567";
        let keys = phone_keys(db_phone);
        for key in keys {
            seed(&mut index, key, &correct_contact);
        }

        // iOS SMS format with space after country code should match
        assert_eq!(index.lookup("+1 5551234567"), Some(correct_contact.clone()));
    }

    #[test]
    fn test_phone_lookup_with_space_does_not_match_wrong_contact() {
        // Ensure that "+1 5551234567" doesn't match a contact that only has "+1" as a key
        let mut index = ContactsIndex::default();
        let wrong_contact =
            Name::from_opt(Some("Wrong".to_string()), Some("Person".to_string())).unwrap();
        let correct_contact =
            Name::from_opt(Some("Correct".to_string()), Some("Person".to_string())).unwrap();

        // Wrong contact only has short keys that could match "+1"
        seed(&mut index, "1".to_string(), &wrong_contact);
        seed(&mut index, "+1".to_string(), &wrong_contact);

        // Correct contact has the full number
        let db_phone = "+15551234567";
        let keys = phone_keys(db_phone);
        for key in keys {
            seed(&mut index, key, &correct_contact);
        }

        // Should match the correct contact, not the wrong one
        let result = index.lookup("+1 5551234567");
        assert_eq!(result, Some(correct_contact.clone()));
    }

    #[test]
    fn test_phone_lookup_short_segment_skipped_in_split() {
        // When a handle string like "+1 " (country code with trailing space but no number)
        // is looked up, the split path should skip the short "+1" segment
        let mut index = ContactsIndex::default();
        let contact = Name::from_opt(Some("Some".to_string()), Some("Person".to_string())).unwrap();

        // Only index with short country code keys
        seed(&mut index, "1".to_string(), &contact);
        seed(&mut index, "+1".to_string(), &contact);

        // A lookup with country code followed by only spaces should not match
        // because the full lookup produces too few digits and split segments are skipped
        assert_eq!(index.lookup("+1 "), None);
    }

    #[test]
    fn test_phone_lookup_multiple_numbers_with_spaces() {
        // Test that multiple space-separated numbers work correctly
        // and don't match a wrong contact indexed with just "+1"
        let mut index = ContactsIndex::default();
        let wrong_contact =
            Name::from_opt(Some("Wrong".to_string()), Some("Contact".to_string())).unwrap();
        let contact1 =
            Name::from_opt(Some("First".to_string()), Some("Contact".to_string())).unwrap();
        let contact2 =
            Name::from_opt(Some("Second".to_string()), Some("Contact".to_string())).unwrap();

        // Index a wrong contact with short country code keys
        // Without the fix, the split lookup would match this first
        seed(&mut index, "1".to_string(), &wrong_contact);
        seed(&mut index, "+1".to_string(), &wrong_contact);

        // Index both correct contacts with full numbers
        for key in phone_keys("+15551234567") {
            seed(&mut index, key, &contact1);
        }
        for key in phone_keys("+15559876543") {
            seed(&mut index, key, &contact2);
        }

        // Space-separated numbers should match the first valid full number, not the wrong contact
        assert_eq!(
            index.lookup("+1 5551234567 +1 5559876543"),
            Some(contact1.clone())
        );
    }

    #[test]
    fn test_lookup_unknown_email_then_phone() {
        // If an unknown email precedes a known phone, we should still find the phone
        let mut index = ContactsIndex::default();
        let contact =
            Name::from_opt(Some("Phone".to_string()), Some("Contact".to_string())).unwrap();

        // Only index the phone number
        for key in phone_keys("+15551234567") {
            seed(&mut index, key, &contact);
        }

        // Input has unknown email first, then a known phone
        assert_eq!(
            index.lookup("unknown@example.com +15551234567"),
            Some(contact.clone())
        );
    }

    #[test]
    fn test_multiple_phones_no_concatenation() {
        // Two phone numbers should not be concatenated and matched as one
        let mut index = ContactsIndex::default();
        let wrong_contact =
            Name::from_opt(Some("Wrong".to_string()), Some("Contact".to_string())).unwrap();
        let correct_contact =
            Name::from_opt(Some("Correct".to_string()), Some("Contact".to_string())).unwrap();

        // Index a contact whose number equals the concatenation of two other numbers
        // e.g., "5551234567" + "5559876543" = "55512345675559876543"
        seed(
            &mut index,
            "55512345675559876543".to_string(),
            &wrong_contact,
        );

        // Index the correct contact with the first number
        for key in phone_keys("+15551234567") {
            seed(&mut index, key, &correct_contact);
        }

        // Looking up two separate phone numbers should NOT match the concatenated one
        let result = index.lookup("+15551234567 +15559876543");
        assert_eq!(result, Some(correct_contact.clone()));
    }

    #[test]
    fn test_participants_map_keeps_every_address_in_a_group() {
        let index = ContactsIndex::default();
        let participants = HashMap::from([
            (1, "+15551234567".to_string()),
            (2, "alice@example.com".to_string()),
            (3, "+15559876543".to_string()),
        ]);
        let deduped = HashMap::from([(1, 0), (2, 0), (3, 1)]);

        let map = index.build_participants_map(&participants, &deduped);

        let merged = &map[&0];
        assert_eq!(merged.handle_ids, HashSet::from([1, 2]));
        assert_eq!(
            merged.addresses,
            BTreeSet::from(["+15551234567".to_string(), "alice@example.com".to_string()])
        );
        assert!(merged.contains("alice@example.com"));
        assert!(merged.contains("+15551234567"));
        assert!(!merged.contains("+15559876543"));
        assert_eq!(
            map[&1].addresses,
            BTreeSet::from(["+15559876543".to_string()])
        );
    }

    // MARK: Identity helpers
    /// Insert `key` under one shared test record.
    fn seed(index: &mut ContactsIndex, key: String, name: &Name) {
        index.insert_key(key, RecordId(0), Some(name));
    }

    /// Open an in-memory macOS Contacts database with only the tables and columns the index reads.
    fn macos_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ZABCDRECORD (
                Z_PK INTEGER PRIMARY KEY,
                Z_ENT INTEGER,
                ZLINKID VARCHAR,
                ZCONTAINERWHERECONTACTISME INTEGER,
                ZFIRSTNAME VARCHAR,
                ZLASTNAME VARCHAR
            );
            CREATE TABLE ZABCDPHONENUMBER (Z_PK INTEGER PRIMARY KEY, ZOWNER INTEGER, ZFULLNUMBER VARCHAR);
            CREATE TABLE ZABCDEMAILADDRESS (Z_PK INTEGER PRIMARY KEY, ZOWNER INTEGER, ZADDRESSNORMALIZED VARCHAR);",
        )
        .unwrap();
        conn
    }

    /// Insert a macOS contact row. `me` sets `ZCONTAINERWHERECONTACTISME`.
    fn macos_record(
        conn: &Connection,
        pk: i64,
        ent: i64,
        link: Option<&str>,
        me: bool,
        first: Option<&str>,
        last: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO ZABCDRECORD VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![pk, ent, link, me.then_some(1_i64), first, last],
        )
        .unwrap();
    }

    fn macos_phone(conn: &Connection, owner: i64, number: &str) {
        conn.execute(
            "INSERT INTO ZABCDPHONENUMBER (ZOWNER, ZFULLNUMBER) VALUES (?1, ?2)",
            rusqlite::params![owner, number],
        )
        .unwrap();
    }

    fn macos_email(conn: &Connection, owner: i64, address: &str) {
        conn.execute(
            "INSERT INTO ZABCDEMAILADDRESS (ZOWNER, ZADDRESSNORMALIZED) VALUES (?1, ?2)",
            rusqlite::params![owner, address],
        )
        .unwrap();
    }

    /// Open an in-memory iOS Contacts database with only the tables and columns the index reads,
    /// plus the search table that `add_connection` uses to detect the iOS schema.
    fn ios_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ABPerson (ROWID INTEGER PRIMARY KEY, First TEXT, Last TEXT, PersonLink INTEGER DEFAULT -1);
            CREATE TABLE ABMultiValue (UID INTEGER PRIMARY KEY, record_id INTEGER, property INTEGER, value TEXT);
            CREATE TABLE ABStore (ROWID INTEGER PRIMARY KEY, MeIdentifier INTEGER DEFAULT -1);
            CREATE TABLE ABPersonLink (ROWID INTEGER PRIMARY KEY, PreferredNamePersonID INTEGER);
            CREATE TABLE ABPersonFullTextSearch_content (
                docid INTEGER PRIMARY KEY,
                c0First TEXT,
                c1Last TEXT,
                c16Phone TEXT,
                c17Email TEXT
            );",
        )
        .unwrap();
        conn
    }

    fn ios_person(
        conn: &Connection,
        rowid: i64,
        first: Option<&str>,
        last: Option<&str>,
        link: i64,
    ) {
        conn.execute(
            "INSERT INTO ABPerson VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![rowid, first, last, link],
        )
        .unwrap();
    }

    fn ios_value(conn: &Connection, record: i64, property: i64, value: &str) {
        conn.execute(
            "INSERT INTO ABMultiValue (record_id, property, value) VALUES (?1, ?2, ?3)",
            rusqlite::params![record, property, value],
        )
        .unwrap();
    }

    fn full(index: &ContactsIndex, id: &str) -> Option<String> {
        index.lookup(id).map(|n| n.full)
    }

    // MARK: Identity tests
    #[test]
    fn test_macos_keys_on_one_record_share_identity() {
        let conn = macos_db();
        macos_record(&conn, 1, 22, None, false, Some("Ada"), Some("Lovelace"));
        macos_phone(&conn, 1, "+15551234567");
        macos_email(&conn, 1, "ada@example.com");

        let index = {
            let mut index = ContactsIndex::default();
            index.add_connection(&conn).unwrap();
            index
        };

        let by_phone = index.identity("+15551234567");
        assert!(by_phone.is_some());
        assert_eq!(by_phone, index.identity("ada@example.com"));
        assert_eq!(
            full(&index, "+15551234567").as_deref(),
            Some("Ada Lovelace")
        );
    }

    #[test]
    fn test_macos_keys_on_two_records_differ() {
        let conn = macos_db();
        macos_record(&conn, 1, 22, None, false, Some("Ada"), None);
        macos_record(&conn, 2, 22, None, false, Some("Grace"), None);
        macos_phone(&conn, 1, "+15551234567");
        macos_phone(&conn, 2, "+15559876543");

        let mut index = ContactsIndex::default();
        index.add_macos(&conn).unwrap();

        let ada = index.identity("+15551234567").unwrap();
        let grace = index.identity("+15559876543").unwrap();
        assert_ne!(ada, grace);
    }

    #[test]
    fn test_macos_key_on_two_records_is_ambiguous_but_named() {
        let conn = macos_db();
        macos_record(&conn, 1, 22, None, false, Some("Ada"), None);
        macos_record(&conn, 2, 22, None, false, Some("Ada"), Some("Lovelace"));
        macos_phone(&conn, 1, "+15551234567");
        macos_phone(&conn, 2, "+15551234567");
        macos_email(&conn, 2, "ada@example.com");

        let mut index = ContactsIndex::default();
        index.add_macos(&conn).unwrap();

        assert_eq!(index.identity("+15551234567"), None);
        // Check that we keep the more complete name for display.
        assert_eq!(
            full(&index, "+15551234567").as_deref(),
            Some("Ada Lovelace")
        );
        // Record 2 retains its identity through its other, unshared key.
        assert!(index.identity("ada@example.com").is_some());
    }

    #[test]
    fn test_macos_zlinkid_pair_shares_identity() {
        let conn = macos_db();
        macos_record(&conn, 1, 22, Some("link-1"), false, Some("Ada"), None);
        macos_record(
            &conn,
            2,
            22,
            Some("link-1"),
            false,
            Some("Ada"),
            Some("Lovelace"),
        );
        macos_record(&conn, 3, 22, None, false, Some("Grace"), None);
        macos_phone(&conn, 1, "+15551234567");
        macos_email(&conn, 2, "ada@example.com");
        macos_phone(&conn, 3, "+15559876543");

        let mut index = ContactsIndex::default();
        index.add_macos(&conn).unwrap();

        let linked = index.identity("+15551234567");
        assert!(linked.is_some());
        assert_eq!(linked, index.identity("ada@example.com"));
        assert_ne!(linked, index.identity("+15559876543"));
        // Check that lookup returns the most complete member name for every linked key.
        assert_eq!(
            full(&index, "+15551234567").as_deref(),
            Some("Ada Lovelace")
        );
    }

    #[test]
    fn test_macos_empty_zlinkid_is_not_a_link() {
        let conn = macos_db();
        macos_record(&conn, 1, 22, Some(""), false, Some("Ada"), None);
        macos_record(&conn, 2, 22, Some(""), false, Some("Grace"), None);
        macos_phone(&conn, 1, "+15551234567");
        macos_phone(&conn, 2, "+15559876543");

        let mut index = ContactsIndex::default();
        index.add_macos(&conn).unwrap();

        assert_ne!(
            index.identity("+15551234567"),
            index.identity("+15559876543")
        );
    }

    #[test]
    fn test_macos_zlinkid_tie_keeps_lowest_row_name() {
        let conn = macos_db();
        // Insert the higher row first: the tie must resolve by row, not by insertion order.
        macos_record(
            &conn,
            2,
            22,
            Some("link-1"),
            false,
            Some("Augusta"),
            Some("King"),
        );
        macos_record(
            &conn,
            1,
            22,
            Some("link-1"),
            false,
            Some("Ada"),
            Some("Lovelace"),
        );
        macos_phone(&conn, 1, "+15551234567");
        macos_email(&conn, 2, "ada@example.com");

        let mut index = ContactsIndex::default();
        index.add_macos(&conn).unwrap();

        assert_eq!(
            full(&index, "+15551234567").as_deref(),
            Some("Ada Lovelace")
        );
        assert_eq!(
            full(&index, "ada@example.com").as_deref(),
            Some("Ada Lovelace")
        );
    }

    #[test]
    fn test_macos_me_card_contributes_no_keys() {
        let conn = macos_db();
        macos_record(&conn, 1, 22, None, true, Some("Me"), Some("Myself"));
        macos_phone(&conn, 1, "+15551234567");
        macos_email(&conn, 1, "me@example.com");

        let mut index = ContactsIndex::default();
        index.add_macos(&conn).unwrap();

        assert_eq!(index.lookup("+15551234567"), None);
        assert_eq!(index.identity("+15551234567"), None);
        assert_eq!(index.lookup("me@example.com"), None);
        assert_eq!(index.identity("me@example.com"), None);
    }

    #[test]
    fn test_macos_other_entities_are_not_records() {
        let conn = macos_db();
        macos_record(&conn, 1, 21, None, false, Some("Group"), None);
        macos_phone(&conn, 1, "+15551234567");

        let mut index = ContactsIndex::default();
        index.add_macos(&conn).unwrap();

        assert_eq!(index.lookup("+15551234567"), None);
        assert_eq!(index.identity("+15551234567"), None);
    }

    #[test]
    fn test_macos_identities_scoped_per_source_file() {
        // Keep the same Z_PK in both files; the records must still get different identities.
        let a = macos_db();
        macos_record(&a, 1, 22, None, false, Some("Ada"), None);
        macos_phone(&a, 1, "+15551234567");
        let b = macos_db();
        macos_record(&b, 1, 22, None, false, Some("Grace"), None);
        macos_phone(&b, 1, "+15559876543");
        // The same key appears in both files, so we cannot assign it one identity.
        macos_email(&a, 1, "shared@example.com");
        macos_email(&b, 1, "shared@example.com");

        let mut index = ContactsIndex::default();
        index.add_macos(&a).unwrap();
        index.add_macos(&b).unwrap();

        let ada = index.identity("+15551234567").unwrap();
        let grace = index.identity("+15559876543").unwrap();
        assert_ne!(ada, grace);
        assert_eq!(index.identity("shared@example.com"), None);
        assert!(index.lookup("shared@example.com").is_some());
    }

    #[test]
    fn test_macos_nameless_record_has_identity_but_no_name() {
        let conn = macos_db();
        macos_record(&conn, 1, 22, None, false, None, None);
        macos_phone(&conn, 1, "+15551234567");
        macos_email(&conn, 1, "org@example.com");

        let mut index = ContactsIndex::default();
        index.add_macos(&conn).unwrap();

        assert_eq!(index.lookup("+15551234567"), None);
        let id = index.identity("+15551234567");
        assert!(id.is_some());
        assert_eq!(id, index.identity("org@example.com"));
    }

    #[test]
    fn test_ios_person_link_pair_shares_identity_and_preferred_name() {
        let conn = ios_db();
        conn.execute("INSERT INTO ABPersonLink VALUES (5, 2)", [])
            .unwrap();
        // We use row 2's name because we set it as `PreferredNamePersonID`, despite row 1's fuller name.
        ios_person(&conn, 1, Some("Ada"), Some("Lovelace"), 5);
        ios_person(&conn, 2, Some("Countess"), None, 5);
        ios_person(&conn, 3, Some("Grace"), None, -1);
        ios_value(&conn, 1, IOS_PROPERTY_PHONE, "+15551234567");
        ios_value(&conn, 2, IOS_PROPERTY_EMAIL, "Ada@Example.com");
        ios_value(&conn, 3, IOS_PROPERTY_PHONE, "+15559876543");

        let mut index = ContactsIndex::default();
        index.add_connection(&conn).unwrap();

        let linked = index.identity("+15551234567");
        assert!(linked.is_some());
        assert_eq!(linked, index.identity("ada@example.com"));
        assert_ne!(linked, index.identity("+15559876543"));
        assert_eq!(full(&index, "+15551234567").as_deref(), Some("Countess"));
        assert_eq!(full(&index, "ada@example.com").as_deref(), Some("Countess"));
    }

    #[test]
    fn test_ios_person_link_without_preferred_falls_back_to_best_name() {
        let conn = ios_db();
        ios_person(&conn, 1, Some("Ada"), None, 5);
        ios_person(&conn, 2, Some("Ada"), Some("Lovelace"), 5);
        ios_value(&conn, 1, IOS_PROPERTY_PHONE, "+15551234567");

        let mut index = ContactsIndex::default();
        index.add_ios(&conn).unwrap();

        assert_eq!(
            full(&index, "+15551234567").as_deref(),
            Some("Ada Lovelace")
        );
    }

    #[test]
    fn test_ios_person_link_tie_keeps_lowest_row_name() {
        let conn = ios_db();
        // No `ABPersonLink` row, so no preferred member; both names score the same.
        ios_person(&conn, 2, Some("Augusta"), Some("King"), 5);
        ios_person(&conn, 1, Some("Ada"), Some("Lovelace"), 5);
        ios_value(&conn, 1, IOS_PROPERTY_PHONE, "+15551234567");
        ios_value(&conn, 2, IOS_PROPERTY_EMAIL, "ada@example.com");

        let mut index = ContactsIndex::default();
        index.add_ios(&conn).unwrap();

        assert_eq!(
            full(&index, "+15551234567").as_deref(),
            Some("Ada Lovelace")
        );
        assert_eq!(
            full(&index, "ada@example.com").as_deref(),
            Some("Ada Lovelace")
        );
    }

    #[test]
    fn test_ios_me_card_contributes_no_keys() {
        let conn = ios_db();
        conn.execute_batch(
            "INSERT INTO ABStore VALUES (1, 7);
             INSERT INTO ABStore VALUES (2, -1);
             INSERT INTO ABStore VALUES (3, NULL);",
        )
        .unwrap();
        ios_person(&conn, 7, Some("Me"), Some("Myself"), -1);
        ios_person(&conn, 8, Some("Ada"), None, -1);
        ios_value(&conn, 7, IOS_PROPERTY_PHONE, "+15551234567");
        ios_value(&conn, 7, IOS_PROPERTY_EMAIL, "me@example.com");
        ios_value(&conn, 8, IOS_PROPERTY_PHONE, "+15559876543");

        let mut index = ContactsIndex::default();
        index.add_ios(&conn).unwrap();

        assert_eq!(index.lookup("+15551234567"), None);
        assert_eq!(index.identity("+15551234567"), None);
        assert_eq!(index.lookup("me@example.com"), None);
        assert!(index.identity("+15559876543").is_some());
    }

    #[test]
    fn test_ios_keys_come_from_multivalue_not_search_table() {
        let conn = ios_db();
        ios_person(&conn, 1, Some("Ada"), Some("Lovelace"), -1);
        ios_value(&conn, 1, IOS_PROPERTY_PHONE, "+15551234567");
        // Give the search table a different phone number and a 7-digit token for this person.
        conn.execute(
            "INSERT INTO ABPersonFullTextSearch_content VALUES (1, 'Ada', 'Lovelace', '+15559876543 5559876543 9876543', '')",
            [],
        )
        .unwrap();

        let mut index = ContactsIndex::default();
        // Include the search table for schema detection; key lookup must still use `ABMultiValue`.
        index.add_connection(&conn).unwrap();

        assert_eq!(
            full(&index, "+15551234567").as_deref(),
            Some("Ada Lovelace")
        );
        assert_eq!(index.lookup("+15559876543"), None);
        assert_eq!(index.identity("+15559876543"), None);
        assert_eq!(index.lookup("9876543"), None);
    }

    #[test]
    fn test_ios_key_on_two_records_is_ambiguous_but_named() {
        let conn = ios_db();
        ios_person(&conn, 1, Some("Ada"), None, -1);
        ios_person(&conn, 2, Some("Ada"), Some("Lovelace"), -1);
        ios_value(&conn, 1, IOS_PROPERTY_EMAIL, "ada@example.com");
        ios_value(&conn, 2, IOS_PROPERTY_EMAIL, "ada@example.com");

        let mut index = ContactsIndex::default();
        index.add_ios(&conn).unwrap();

        assert_eq!(index.identity("ada@example.com"), None);
        assert_eq!(
            full(&index, "ada@example.com").as_deref(),
            Some("Ada Lovelace")
        );
    }

    #[test]
    fn test_identity_never_matches_business_id() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Biz".to_string()), None).unwrap();
        seed(&mut index, "urn:biz:1234567".to_string(), &name);

        assert_eq!(index.identity("urn:biz:1234567"), None);
        assert_eq!(index.lookup("urn:biz:1234567"), None);
    }

    // MARK: Participants map
    fn seeded_index(pairs: &[(&str, &str, u32)]) -> ContactsIndex {
        let mut index = ContactsIndex::default();
        for (key, name, record) in pairs {
            let (first, last) = name.split_once(' ').unwrap_or((name, ""));
            let name = Name::from_opt(
                Some(first.to_string()),
                (!last.is_empty()).then(|| last.to_string()),
            );
            index.insert_key((*key).to_string(), RecordId(*record), name.as_ref());
        }
        index.next_id = pairs.iter().map(|p| p.2 + 1).max().unwrap_or(0);
        index
    }

    fn handle_db(rows: &[(i32, &str)]) -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT NOT NULL, service TEXT, person_centric_id TEXT)",
            [],
        )
        .unwrap();
        for (rowid, id) in rows {
            db.execute(
                "INSERT INTO handle (ROWID, id, service) VALUES (?1, ?2, 'iMessage')",
                (rowid, id),
            )
            .unwrap();
        }
        db
    }

    #[test]
    fn participants_map_prefers_a_member_with_a_contact_name() {
        // The email has a contact record; the lower-row-ID phone does not.
        let index = seeded_index(&[("a@example.com", "Alice Example", 0)]);
        let participants: HashMap<i32, String> = HashMap::from([
            (5, "+15551234567".to_string()),
            (9, "a@example.com".to_string()),
        ]);
        let groups: HashMap<i32, i32> = HashMap::from([(5, 0), (9, 0)]);

        let map = index.build_participants_map(&participants, &groups);
        let name = &map[&0];

        assert_eq!(name.full, "Alice Example");
        assert_eq!(
            name.details, "+15551234567",
            "details follow the lowest row ID"
        );
        assert_eq!(name.handle_ids, HashSet::from([5, 9]));
    }

    #[test]
    fn participants_map_is_deterministic_across_runs() {
        let index = seeded_index(&[("b@example.com", "Bob", 0), ("c@example.com", "Carol", 1)]);
        let participants: HashMap<i32, String> = HashMap::from([
            (3, "b@example.com".to_string()),
            (4, "c@example.com".to_string()),
            (7, "+15550000000".to_string()),
        ]);
        // Put all three handles in one group. Bob's handle has the lowest row ID
        // among the two that resolve, so his name must win.
        let groups: HashMap<i32, i32> = HashMap::from([(3, 0), (4, 0), (7, 0)]);

        let names: Vec<String> = (0..20)
            .map(|_| {
                index.build_participants_map(&participants, &groups)[&0]
                    .full
                    .clone()
            })
            .collect();

        assert!(names.iter().all(|n| n == "Bob"), "{names:?}");
    }

    #[test]
    fn participants_map_falls_back_to_details() {
        let index = ContactsIndex::default();
        let participants: HashMap<i32, String> =
            HashMap::from([(0, "Me".to_string()), (2, "+15551234567".to_string())]);
        let groups: HashMap<i32, i32> = HashMap::from([(0, 0), (2, 1)]);

        let map = index.build_participants_map(&participants, &groups);

        assert_eq!(map[&1].full, "");
        assert_eq!(map[&1].details, "+15551234567");
        assert_eq!(map[&0].details, "Me");
    }

    #[test]
    fn link_handles_unions_addresses_of_one_record() {
        let index = seeded_index(&[
            ("+15551234567", "Alice", 0),
            ("a@example.com", "Alice", 0),
            ("b@example.com", "Bob", 1),
        ]);
        let db = handle_db(&[
            (1, "+15551234567"),
            (2, "a@example.com"),
            (3, "b@example.com"),
            (4, "nobody@example.com"),
        ]);
        let participants: HashMap<i32, String> = HashMap::from([
            (0, "Me".to_string()),
            (1, "+15551234567".to_string()),
            (2, "a@example.com".to_string()),
            (3, "b@example.com".to_string()),
            (4, "nobody@example.com".to_string()),
        ]);

        let mut groups = HandleGroups::from_db(&db).unwrap();
        let edges = index.link_handles(&participants, &mut groups);
        let map = groups.finalize();

        assert_eq!(edges, 1);
        assert_eq!(map[&1], map[&2]);
        assert_ne!(map[&1], map[&3]);
        assert_ne!(map[&3], map[&4]);
        assert_ne!(map[&0], map[&1]);
    }

    #[test]
    fn link_handles_skips_ambiguous_addresses() {
        let mut index = seeded_index(&[("+15551234567", "Alice", 0)]);
        // Give a second record the same number; identity lookup should return none.
        index.insert_key(
            "+15551234567".to_string(),
            RecordId(1),
            Name::from_opt(Some("Alice".to_string()), Some("Dupe".to_string())).as_ref(),
        );
        index.insert_key("a@example.com".to_string(), RecordId(0), None);
        let db = handle_db(&[(1, "+15551234567"), (2, "a@example.com")]);
        let participants: HashMap<i32, String> = HashMap::from([
            (1, "+15551234567".to_string()),
            (2, "a@example.com".to_string()),
        ]);

        let mut groups = HandleGroups::from_db(&db).unwrap();
        let edges = index.link_handles(&participants, &mut groups);
        let map = groups.finalize();

        assert_eq!(edges, 0);
        assert_ne!(map[&1], map[&2]);
        assert!(
            index.lookup("+15551234567").is_some(),
            "ambiguous keys still name"
        );
    }

    #[test]
    fn macos_contact_entities_are_read_from_the_model() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE Z_PRIMARYKEY (Z_ENT INTEGER, Z_NAME TEXT);
             INSERT INTO Z_PRIMARYKEY VALUES (40, 'ABCDContact');
             INSERT INTO Z_PRIMARYKEY VALUES (41, 'ABCDSubscribedContact');",
        )
        .unwrap();
        assert_eq!(macos_contact_entities(&conn), [40, 41]);

        let bare = rusqlite::Connection::open_in_memory().unwrap();
        assert_eq!(macos_contact_entities(&bare), MACOS_CONTACT_ENTITY_FALLBACK);
    }
}
