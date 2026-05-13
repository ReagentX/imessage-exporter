use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use imessage_database::{
    error::table::TableError, tables::table::get_connection, util::dirs::home,
};
use rusqlite::{Connection, Result};

/// The default contacts file location from the root of an iOS backup
pub const DEFAULT_PATH_IOS: &str = "31/31bb7ba8914766d4ba40d6dfb6113c8b614be442";

/// Minimum number of digits required to consider a string a valid phone number
const MIN_PHONE_DIGITS: usize = 7;

// MARK: Name
#[derive(Clone, Debug, PartialEq, Eq)]
/// Simple first/last name struct
pub struct Name {
    /// First name
    pub first: String,
    /// Last name
    pub last: String,
    /// Full name as a single string
    pub full: String,
    /// Combined handle details from iMessage's database
    pub details: String,
    /// Set of original handle IDs that map to this name
    pub handle_ids: HashSet<i32>,
    /// Raw image bytes from AddressBook (JPEG/PNG/HEIC/etc.).  `None` if the
    /// contact has no photo or the photo column was unreadable.
    pub avatar_bytes: Option<Vec<u8>>,
}

impl Name {
    /// Create from optional first/last name
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
            avatar_bytes: None,
        })
    }

    /// Simple scoring: 1 point for first name, 1 point for last name
    fn score(&self) -> u8 {
        u8::from(!self.first.is_empty()) + u8::from(!self.last.is_empty())
    }

    /// Check if the name matches the provided string in any field
    pub fn contains(&self, s: &str) -> bool {
        self.first.contains(s)
            || self.last.contains(s)
            || self.full.contains(s)
            || self.details.contains(s)
    }

    /// Get the contact's full name, falling back to details if full name is empty
    pub fn get_display_name(&self) -> &str {
        if self.full.is_empty() {
            &self.details
        } else {
            &self.full
        }
    }

    /// Create a Name that only carries the details string
    pub fn from_details<D: Into<String>>(details: D) -> Self {
        Name {
            first: String::new(),
            last: String::new(),
            full: String::new(),
            details: details.into(),
            handle_ids: HashSet::new(),
            avatar_bytes: None,
        }
    }
}

#[cfg(test)]
impl Name {
    /// Create a fake Name for testing
    pub fn fake_name(name: &str) -> Name {
        Name {
            first: String::new(),
            last: String::new(),
            full: String::new(),
            details: name.to_string(),
            handle_ids: HashSet::new(),
            avatar_bytes: None,
        }
    }
}

// MARK: Index
#[derive(Debug, Default)]
/// Contacts index for looking up names by phone/email
pub struct ContactsIndex {
    /// Map of identifier (phone/email) to [`Name`]
    index: HashMap<String, Name>,
}

impl ContactsIndex {
    /// Build a contacts index
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
            if let Ok(local_conn) = Connection::open(&db_path) {
                let sub = Self::build_from_macos(&local_conn)?;

                for (k, v) in sub.index {
                    upsert_best(&mut idx, k, &v);
                }
            }
        }

        Ok(Self { index: idx })
    }

    // MARK: macOS
    /// Build contacts index from macOS Contacts database.
    ///
    /// `ZABCDIMAGE` is joined when available (modern macOS schemas) so each contact
    /// can carry its avatar; older fixtures without that table still load names.
    fn build_from_macos(conn: &Connection) -> Result<Self> {
        let mut index = HashMap::new();

        let with_images = table_exists(conn, "ZABCDIMAGE");
        let sql = if with_images {
            "SELECT r.ZFIRSTNAME, r.ZLASTNAME, p.ZFULLNUMBER, e.ZADDRESSNORMALIZED, img.ZIMAGEDATA
             FROM ZABCDRECORD AS r
             LEFT JOIN ZABCDPHONENUMBER AS p ON r.Z_PK = p.ZOWNER
             LEFT JOIN ZABCDEMAILADDRESS AS e ON r.Z_PK = e.ZOWNER
             LEFT JOIN ZABCDIMAGE         AS img ON r.Z_PK = img.ZOWNER"
        } else {
            "SELECT r.ZFIRSTNAME, r.ZLASTNAME, p.ZFULLNUMBER, e.ZADDRESSNORMALIZED
             FROM ZABCDRECORD AS r
             LEFT JOIN ZABCDPHONENUMBER AS p ON r.Z_PK = p.ZOWNER
             LEFT JOIN ZABCDEMAILADDRESS AS e ON r.Z_PK = e.ZOWNER"
        };

        let mut stmt = conn.prepare(sql)?;

        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name = Name::from_opt(
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            );

            if let Some(mut name) = name {
                if with_images {
                    // Image data is in column 4 (after first/last/phone/email)
                    if let Ok(Some(img_bytes)) = row.get::<_, Option<Vec<u8>>>(4) {
                        if !img_bytes.is_empty() {
                            name.avatar_bytes = Some(img_bytes);
                        }
                    }
                }

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
    /// Build contacts index from iOS backup database
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

        // Second pass: attach avatars from ABImage, keyed by phone/email of the owner
        if let Ok(mut avatar_stmt) = conn.prepare(
            "SELECT p.ROWID, ph.value, em.value, i.data
             FROM ABPerson AS p
             LEFT JOIN ABMultiValue AS ph ON ph.record_id = p.ROWID AND ph.property = 3
             LEFT JOIN ABMultiValue AS em ON em.record_id = p.ROWID AND em.property = 4
             LEFT JOIN ABImage      AS i  ON i.record_id  = p.ROWID",
        ) {
            if let Ok(mut rows) = avatar_stmt.query([]) {
                while let Ok(Some(row)) = rows.next() {
                    let phone: Option<String> = row.get(1).unwrap_or(None);
                    let email: Option<String> = row.get(2).unwrap_or(None);
                    let img: Option<Vec<u8>> = row.get(3).unwrap_or(None);
                    let Some(img_bytes) = img else { continue };
                    if img_bytes.is_empty() { continue }

                    if let Some(phone) = phone {
                        for key in phone_keys(&phone) {
                            if let Some(entry) = index.get_mut(&key) {
                                if entry.avatar_bytes.is_none() {
                                    entry.avatar_bytes = Some(img_bytes.clone());
                                }
                            }
                        }
                    }
                    if let Some(email) = email {
                        if let Some(norm) = normalize_email(&email) {
                            if let Some(entry) = index.get_mut(&norm) {
                                if entry.avatar_bytes.is_none() {
                                    entry.avatar_bytes = Some(img_bytes.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(Self { index })
    }

    /// Returns first/last name if found
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

    /// Look up just the avatar bytes for a handle, mirroring `lookup`'s matching rules.
    ///
    /// Note: we re-walk the index directly (rather than calling `lookup`) because `lookup`
    /// clones the `Name`, which would force us to clone the avatar bytes on every call.
    pub fn get_avatar(&self, id: &str) -> Option<&[u8]> {
        for id_part in id.split_whitespace() {
            if looks_like_email(id_part) {
                if let Some(key) = normalize_email(id_part) {
                    if let Some(n) = self.index.get(&key) {
                        return n.avatar_bytes.as_deref();
                    }
                }
                continue;
            }
            for k in phone_keys(id_part) {
                if let Some(n) = self.index.get(&k) {
                    return n.avatar_bytes.as_deref();
                }
            }
        }
        None
    }

    /// Build a map of participant handle IDs to Names
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
                })
                .or_insert_with(|| {
                    let mut name = self
                        .lookup(details)
                        .unwrap_or_else(|| Name::from_details(details.clone()));

                    // Keep the original details string for display/fallback
                    name.details = details.clone();
                    name.handle_ids.insert(handle_id);
                    name
                });
        }

        result
    }
}

/// Check if a table or view exists in the database
fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name = ?1 LIMIT 1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

/// Upsert a [`Name`] into the map if it has a better [`Name::score`] than existing
fn upsert_best(map: &mut HashMap<String, Name>, key: String, incoming: &Name) {
    match map.get_mut(&key) {
        Some(existing) => {
            if incoming.score() > existing.score() {
                // Preserve avatar from the displaced entry if the new one lacks one
                let preserved_avatar = existing.avatar_bytes.take();
                *existing = incoming.clone();
                if existing.avatar_bytes.is_none() {
                    existing.avatar_bytes = preserved_avatar;
                }
            } else if existing.avatar_bytes.is_none() && incoming.avatar_bytes.is_some() {
                existing.avatar_bytes = incoming.avatar_bytes.clone();
            }
        }
        None => {
            map.insert(key, incoming.clone());
        }
    }
}

// MARK: Email
/// Simple heuristic to determine if the identifier looks like an email
fn looks_like_email(s: &str) -> bool {
    s.contains('@')
}

/// Normalize email: trim, lowercase, remove angle-brackets
fn normalize_email(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Guard for angle-brackets
    let s = s.trim_start_matches('<').trim_end_matches('>');
    if s.is_empty() {
        return None;
    }
    Some(s.to_lowercase())
}

/// Parse a space-separated list of emails
fn parse_email_list(raw: &str) -> Vec<String> {
    // macOS may store a single value; guard for angle-brackets
    if raw.contains(' ') {
        raw.split_whitespace().filter_map(normalize_email).collect()
    } else {
        normalize_email(raw).into_iter().collect()
    }
}

// MARK: Phone
/// Generate possible phone number keys from a raw phone number
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

    // Create keys with and without '+' prefix for country code
    let mut keys = vec![digits.clone(), format!("+{digits}")];

    // If the original was 12 chars starting with +1, add a variant without the `+1` (USA) country code
    if digits.len() == 11 && raw.starts_with("+1") {
        let last_10 = &digits[digits.len() - 10..];
        keys.push(last_10.to_string());
        keys.push(format!("+{last_10}"));
    }

    keys.dedup();
    keys
}

/// Extract digits from a raw phone number string
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
    fn name_avatar_bytes_default_is_none() {
        let n = Name::from_opt(Some("A".to_string()), Some("B".to_string())).unwrap();
        assert!(n.avatar_bytes.is_none());
    }

    #[test]
    fn contacts_index_can_carry_avatar_bytes() {
        let mut index = ContactsIndex::default();
        let mut n = Name::from_opt(Some("A".to_string()), Some("B".to_string())).unwrap();
        n.avatar_bytes = Some(vec![0xFF, 0xD8, 0xFF]);
        index.index.insert("test@example.com".to_string(), n);
        let looked_up = index.lookup("test@example.com").unwrap();
        assert_eq!(looked_up.avatar_bytes, Some(vec![0xFF, 0xD8, 0xFF]));
    }

    #[test]
    fn contacts_index_get_avatar_returns_bytes() {
        let mut index = ContactsIndex::default();
        let mut n = Name::from_opt(Some("A".to_string()), Some("B".to_string())).unwrap();
        n.avatar_bytes = Some(vec![1, 2, 3]);
        index.index.insert("foo@example.com".to_string(), n);
        assert_eq!(index.get_avatar("foo@example.com"), Some(&[1u8, 2, 3][..]));
    }

    #[test]
    fn contacts_index_get_avatar_returns_none_without_bytes() {
        let mut index = ContactsIndex::default();
        let n = Name::from_opt(Some("A".to_string()), Some("B".to_string())).unwrap();
        index.index.insert("foo@example.com".to_string(), n);
        assert_eq!(index.get_avatar("foo@example.com"), None);
    }

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
}
