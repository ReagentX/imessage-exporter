use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use imessage_database::{
    error::table::TableError, tables::table::get_connection, util::dirs::home,
};
use rusqlite::{Connection, Result};

/// Default contacts database path inside an iOS backup.
pub const DEFAULT_PATH_IOS: &str = "31/31bb7ba8914766d4ba40d6dfb6113c8b614be442";

/// Minimum digits required to index a phone number.
const MIN_PHONE_DIGITS: usize = 7;

// MARK: Name
#[derive(Clone, Debug, PartialEq, Eq)]
/// Contact name and the handle IDs it resolved from.
pub struct Name {
    /// First name.
    pub first: String,
    /// Last name.
    pub last: String,
    /// Full name.
    pub full: String,
    /// Handle address used for display and filenames.
    pub details: String,
    /// Original handle IDs that map to this name.
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

// MARK: Index
#[derive(Debug, Default)]
/// Contacts index keyed by normalized phone number or email address.
pub struct ContactsIndex {
    /// Names keyed by normalized identifier.
    index: HashMap<String, Name>,
}

impl ContactsIndex {
    /// Build a contacts index from one database path or all local macOS sources.
    ///
    /// - If `path` is `Some`, we only look at that database.
    /// - If `path` is `None`, scans macOS Contacts sources under
    ///   `~/Library/Application Support/AddressBook/Sources/*/AddressBook-v22.abcddb`
    ///
    /// Supports building from both macOS (`AddressBook-v22.abcddb`) and iOS (`AddressBook.sqlitedb`) databases.
    pub fn build(path: Option<&Path>) -> Result<Self, TableError> {
        if let Some(path) = path {
            let conn = get_connection(path)?;
            if table_exists(&conn, "ABPersonFullTextSearch_content") {
                return Ok(Self::build_from_ios(&conn)?);
            }
            return Ok(Self::build_from_macos(&conn)?);
        }

        let mut idx: HashMap<String, Name> = HashMap::new();

        for db_path in find_macos_addressbook_db_paths() {
            if let Ok(local_conn) = get_connection(&db_path) {
                let sub = Self::build_from_macos(&local_conn)?;

                for (k, v) in sub.index {
                    upsert_best(&mut idx, k, &v);
                }
            }
        }

        Ok(Self { index: idx })
    }

    // MARK: macOS
    /// Build a contacts index from a macOS Contacts database.
    fn build_from_macos(conn: &Connection) -> Result<Self> {
        let mut index = HashMap::new();

        let mut stmt = conn.prepare(
            "SELECT r.ZFIRSTNAME, r.ZLASTNAME, p.ZFULLNUMBER, e.ZADDRESSNORMALIZED
             FROM ZABCDRECORD AS r
             LEFT JOIN ZABCDPHONENUMBER AS p ON r.Z_PK = p.ZOWNER
             LEFT JOIN ZABCDEMAILADDRESS AS e ON r.Z_PK = e.ZOWNER",
        )?;

        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name = Name::from_opt(
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            );

            if let Some(name) = name {
                if let Some(email_raw) = row.get::<_, Option<String>>(3)? {
                    // Some macOS rows are like "<addr@dom>"
                    for email in parse_email_list(&email_raw) {
                        upsert_best(&mut index, email, &name);
                    }
                }

                if let Some(phone_raw) = row.get::<_, Option<String>>(2)? {
                    for key in phone_keys(&phone_raw) {
                        upsert_best(&mut index, key, &name);
                    }
                }
            }
        }

        Ok(Self { index })
    }

    // MARK: iOS
    /// Build a contacts index from an iOS backup Contacts database.
    fn build_from_ios(conn: &Connection) -> Result<Self> {
        // iOS backup contacts: ABPersonFullTextSearch_content with columns:
        // c0First (TEXT), c1Last (TEXT), c16Phone (TEXT: space-separated variants), c17Email (TEXT: space-separated)
        let mut index = HashMap::new();

        let mut stmt = conn.prepare(
            "SELECT c0First, c1Last, c16Phone, c17Email
             FROM ABPersonFullTextSearch_content",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name = Name::from_opt(
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            );

            if let Some(name) = name {
                if let Some(phones_blob) = row.get::<_, Option<String>>(2)? {
                    for token in phones_blob.split_whitespace() {
                        for key in phone_keys(token) {
                            upsert_best(&mut index, key, &name);
                        }
                    }
                }

                if let Some(emails_blob) = row.get::<_, Option<String>>(3)? {
                    for email in emails_blob.split_whitespace() {
                        if let Some(norm) = normalize_email(email) {
                            upsert_best(&mut index, norm, &name);
                        }
                    }
                }
            }
        }

        Ok(Self { index })
    }

    /// Look up a contact name by handle string.
    pub fn lookup(&self, id: &str) -> Option<Name> {
        // Look up each space-separated token
        for id_part in id.split_whitespace() {
            if looks_like_email(id_part) {
                if let Some(name) =
                    normalize_email(id_part).and_then(|k| self.index.get(&k).cloned())
                {
                    return Some(name);
                }
                continue;
            }

            for k in phone_keys(id_part) {
                if let Some(n) = self.index.get(&k) {
                    return Some(n.clone());
                }
            }
        }
        None
    }

    /// Build names keyed by deduplicated participant ID.
    ///
    /// - `participants`: map of handle ID to handle details
    /// - `deduped_handles`: map of handle ID to deduplicated handle ID
    /// - Returns: map of deduplicated handle ID to Name
    pub fn build_participants_map(
        &self,
        participants: &HashMap<i32, String>,
        deduped_handles: &HashMap<i32, i32>,
    ) -> HashMap<i32, Name> {
        let mut result: HashMap<i32, Name> = HashMap::new();

        for (&handle_id, details) in participants {
            let Some(&deduped_id) = deduped_handles.get(&handle_id) else {
                continue;
            };

            result
                .entry(deduped_id)
                .and_modify(|name| {
                    name.handle_ids.insert(handle_id);
                    name.addresses.insert(details.clone());
                })
                .or_insert_with(|| {
                    let mut name = self
                        .lookup(details)
                        .unwrap_or_else(|| Name::from_details(details.clone()));

                    // Keep the original details string for display/fallback
                    name.details = details.clone();
                    name.handle_ids.insert(handle_id);
                    name.addresses.insert(details.clone());
                    name
                });
        }

        result
    }
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

/// Insert a name when it is more complete than the existing value.
fn upsert_best(map: &mut HashMap<String, Name>, key: String, incoming: &Name) {
    match map.get_mut(&key) {
        Some(existing) => {
            if incoming.score() > existing.score() {
                *existing = incoming.clone();
            }
        }
        None => {
            map.insert(key, incoming.clone());
        }
    }
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
            index.index.insert(key, name.clone());
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
            index.index.insert(key, name.clone());
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
            index.index.insert(key, name.clone());
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
            index.index.insert(key, name.clone());
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
            index.index.insert(key, name.clone());
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
            index.index.insert(key, name.clone());
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
            index.index.insert(key, name.clone());
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
            index.index.insert(key, name.clone());
        }

        // For 10-digit db entry, no US variants added, so these should not match
        assert_eq!(index.lookup("+34567890"), None);
    }

    #[test]
    fn test_phone_lookup_uk_with_plus44() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Alice".to_string()), Some("Brown".to_string())).unwrap();
        // UK number +44 20 1234 5678
        index
            .index
            .insert("+442012345678".to_string(), name.clone());

        // Lookup with +44 should match
        assert_eq!(index.lookup("+442012345678"), Some(name.clone()));
    }

    #[test]
    fn test_phone_lookup_uk_without_plus() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Alice".to_string()), Some("Brown".to_string())).unwrap();
        // UK number +44 20 1234 5678
        index
            .index
            .insert("+442012345678".to_string(), name.clone());

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
        index
            .index
            .insert("steve@apple.com".to_string(), name.clone());

        // Exact match
        assert_eq!(index.lookup("steve@apple.com"), Some(name.clone()));
    }

    #[test]
    fn test_email_lookup_match_case_insensitive() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Steve".to_string()), Some("Jobs".to_string())).unwrap();
        index
            .index
            .insert("steve@apple.com".to_string(), name.clone());

        // Case insensitive
        assert_eq!(index.lookup("STEVE@APPLE.COM"), Some(name.clone()));
    }

    #[test]
    fn test_email_lookup_match_with_angle_brackets() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Steve".to_string()), Some("Jobs".to_string())).unwrap();
        index
            .index
            .insert("steve@apple.com".to_string(), name.clone());

        // With angle brackets
        assert_eq!(index.lookup("<steve@apple.com>"), Some(name.clone()));
    }

    #[test]
    fn test_email_lookup_match_trimmed() {
        let mut index = ContactsIndex::default();
        let name = Name::from_opt(Some("Steve".to_string()), Some("Jobs".to_string())).unwrap();
        index
            .index
            .insert("steve@apple.com".to_string(), name.clone());

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
            index.index.insert(key, correct_contact.clone());
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
        index.index.insert("1".to_string(), wrong_contact.clone());
        index.index.insert("+1".to_string(), wrong_contact.clone());

        // Correct contact has the full number
        let db_phone = "+15551234567";
        let keys = phone_keys(db_phone);
        for key in keys {
            index.index.insert(key, correct_contact.clone());
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
        index.index.insert("1".to_string(), contact.clone());
        index.index.insert("+1".to_string(), contact.clone());

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
        index.index.insert("1".to_string(), wrong_contact.clone());
        index.index.insert("+1".to_string(), wrong_contact.clone());

        // Index both correct contacts with full numbers
        for key in phone_keys("+15551234567") {
            index.index.insert(key, contact1.clone());
        }
        for key in phone_keys("+15559876543") {
            index.index.insert(key, contact2.clone());
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
            index.index.insert(key, contact.clone());
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
        index
            .index
            .insert("55512345675559876543".to_string(), wrong_contact.clone());

        // Index the correct contact with the first number
        for key in phone_keys("+15551234567") {
            index.index.insert(key, correct_contact.clone());
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
}
