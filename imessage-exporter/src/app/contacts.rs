/*!
 VCF (vCard) contact parser for resolving contact names from phone numbers and email addresses.
*/

use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use crate::app::error::RuntimeError;

/// Represents a contact parsed from a VCF file
#[derive(Debug, Clone)]
pub struct Contact {
    /// The full name of the contact
    pub name: String,
    /// Phone numbers associated with this contact
    pub phone_numbers: Vec<String>,
    /// Email addresses associated with this contact
    pub emails: Vec<String>,
}

/// VCF parser that extracts contact information and creates lookup maps
pub struct VcfParser {
    /// Map from phone number to contact name
    pub phone_to_name: HashMap<String, String>,
    /// Map from email address to contact name
    pub email_to_name: HashMap<String, String>,
    /// Temporary storage for contacts during parsing (for merging duplicates)
    contacts_by_name: HashMap<String, Contact>,
}

impl VcfParser {
    /// Create a new VCF parser
    pub fn new() -> Self {
        Self {
            phone_to_name: HashMap::new(),
            email_to_name: HashMap::new(),
            contacts_by_name: HashMap::new(),
        }
    }

    /// Parse a VCF file and populate the lookup maps
    pub fn parse_vcf_file<P: AsRef<Path>>(&mut self, vcf_path: P) -> Result<(), RuntimeError> {
        let file = File::open(vcf_path)?;
        let reader = BufReader::new(file);
        
        let mut current_contact = Contact {
            name: String::new(),
            phone_numbers: Vec::new(),
            emails: Vec::new(),
        };
        let mut in_vcard = false;

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line == "BEGIN:VCARD" {
                in_vcard = true;
                current_contact = Contact {
                    name: String::new(),
                    phone_numbers: Vec::new(),
                    emails: Vec::new(),
                };
            } else if line == "END:VCARD" && in_vcard {
                // Store the contact for potential merging
                self.store_contact_for_merging(current_contact.clone());
                in_vcard = false;
            } else if in_vcard {
                self.parse_vcard_line(line, &mut current_contact);
            }
        }

        // After parsing all contacts, finalize by processing merged contacts
        self.finalize_contacts();

        Ok(())
    }

    /// Parse a single line from a vCard entry
    fn parse_vcard_line(&self, line: &str, contact: &mut Contact) {
        if let Some((field, value)) = line.split_once(':') {
            match field {
                "FN" => {
                    // Full name field
                    if !value.is_empty() {
                        contact.name = value.to_string();
                    }
                }
                field if field.starts_with("TEL") => {
                    // Phone number field
                    if !value.is_empty() {
                        contact.phone_numbers.push(self.normalize_phone_number(value));
                    }
                }
                field if field.starts_with("EMAIL") => {
                    // Email field
                    if !value.is_empty() {
                        contact.emails.push(value.to_lowercase());
                    }
                }
                _ => {
                    // Ignore other fields for now
                }
            }
        }
    }

    /// Store a contact for potential merging with duplicates
    fn store_contact_for_merging(&mut self, contact: Contact) {
        // Only process contacts that have a name
        if contact.name.is_empty() {
            return;
        }

        let name = contact.name.clone();
        
        // Check if we already have a contact with this name
        if let Some(existing_contact) = self.contacts_by_name.get_mut(&name) {
            // Merge phone numbers (avoid duplicates)
            for phone in contact.phone_numbers {
                if !existing_contact.phone_numbers.contains(&phone) {
                    existing_contact.phone_numbers.push(phone);
                }
            }
            
            // Merge email addresses (avoid duplicates)
            for email in contact.emails {
                if !existing_contact.emails.contains(&email) {
                    existing_contact.emails.push(email);
                }
            }
        } else {
            // First time seeing this contact name
            self.contacts_by_name.insert(name, contact);
        }
    }

    /// Finalize contacts by processing all merged contacts into lookup maps
    fn finalize_contacts(&mut self) {
        // Collect contacts to avoid borrow checker issues
        let contacts: Vec<Contact> = self.contacts_by_name.values().cloned().collect();
        
        // Clear the temporary storage first
        self.contacts_by_name.clear();
        
        // Now process all contacts
        for contact in contacts {
            self.process_contact(contact);
        }
    }

    /// Process a completed contact and add it to the lookup maps
    fn process_contact(&mut self, contact: Contact) {
        // Add phone number mappings
        for phone in &contact.phone_numbers {
            self.phone_to_name.insert(phone.clone(), contact.name.clone());
        }

        // Add email mappings
        for email in &contact.emails {
            self.email_to_name.insert(email.clone(), contact.name.clone());
        }
    }

    /// Normalize a phone number by removing formatting characters
    fn normalize_phone_number(&self, phone: &str) -> String {
        // Remove common formatting characters but keep the core number
        let normalized = phone
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '+')
            .collect::<String>();
        
        // Also store the original format in case it's needed
        normalized
    }

    /// Look up a contact name by phone number
    pub fn get_name_by_phone(&self, phone: &str) -> Option<&String> {
        let normalized = self.normalize_phone_number(phone);
        
        // Try exact match first
        if let Some(name) = self.phone_to_name.get(&normalized) {
            return Some(name);
        }

        // Try fuzzy matching - check if any stored number contains or is contained in the query
        for (stored_phone, name) in &self.phone_to_name {
            if self.phones_match(&normalized, stored_phone) {
                return Some(name);
            }
        }

        None
    }

    /// Look up a contact name by email address
    pub fn get_name_by_email(&self, email: &str) -> Option<&String> {
        self.email_to_name.get(&email.to_lowercase())
    }

    /// Check if two phone numbers match, accounting for different formatting
    fn phones_match(&self, phone1: &str, phone2: &str) -> bool {
        // Remove leading country codes and compare
        let clean1 = self.remove_country_code(phone1);
        let clean2 = self.remove_country_code(phone2);
        
        // Check if one contains the other (for cases like +1234567890 vs 234567890)
        clean1 == clean2 || 
        clean1.ends_with(&clean2) || 
        clean2.ends_with(&clean1) ||
        (clean1.len() >= 7 && clean2.len() >= 7 && clean1.ends_with(&clean2[clean2.len()-7..]))
    }

    /// Remove common country codes from phone numbers
    fn remove_country_code(&self, phone: &str) -> String {
        let phone = phone.strip_prefix('+').unwrap_or(phone);
        
        // Remove common North American country code
        if phone.starts_with('1') && phone.len() == 11 {
            phone[1..].to_string()
        } else {
            phone.to_string()
        }
    }

    /// Get the total number of contacts loaded
    pub fn contact_count(&self) -> usize {
        let phone_contacts = self.phone_to_name.len();
        let email_contacts = self.email_to_name.len();
        // Note: This might double-count contacts that have both phone and email
        phone_contacts + email_contacts
    }
}

impl Default for VcfParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_simple_vcard() {
        let mut parser = VcfParser::new();
        
        // Create a temporary VCF file
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "BEGIN:VCARD").unwrap();
        writeln!(temp_file, "VERSION:3.0").unwrap();
        writeln!(temp_file, "FN:John Doe").unwrap();
        writeln!(temp_file, "TEL;type=CELL:+1234567890").unwrap();
        writeln!(temp_file, "EMAIL:john@example.com").unwrap();
        writeln!(temp_file, "END:VCARD").unwrap();
        temp_file.flush().unwrap();

        parser.parse_vcf_file(temp_file.path()).unwrap();

        assert_eq!(parser.get_name_by_phone("+1234567890"), Some(&"John Doe".to_string()));
        assert_eq!(parser.get_name_by_email("john@example.com"), Some(&"John Doe".to_string()));
    }

    #[test]
    fn test_phone_normalization() {
        let parser = VcfParser::new();
        
        assert_eq!(parser.normalize_phone_number("+1 (234) 567-8900"), "+12345678900");
        assert_eq!(parser.normalize_phone_number("234-567-8900"), "2345678900");
        assert_eq!(parser.normalize_phone_number("(234) 567 8900"), "2345678900");
    }

    #[test]
    fn test_phone_matching() {
        let parser = VcfParser::new();
        
        assert!(parser.phones_match("+12345678900", "2345678900"));
        assert!(parser.phones_match("2345678900", "+12345678900"));
        assert!(parser.phones_match("5678900", "2345678900"));
    }

    #[test]
    fn test_empty_name_ignored() {
        let mut parser = VcfParser::new();
        
        // Create a temporary VCF file with no name
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "BEGIN:VCARD").unwrap();
        writeln!(temp_file, "VERSION:3.0").unwrap();
        writeln!(temp_file, "TEL;type=CELL:+1234567890").unwrap();
        writeln!(temp_file, "EMAIL:noreply@example.com").unwrap();
        writeln!(temp_file, "END:VCARD").unwrap();
        temp_file.flush().unwrap();

        parser.parse_vcf_file(temp_file.path()).unwrap();

        assert_eq!(parser.get_name_by_phone("+1234567890"), None);
        assert_eq!(parser.get_name_by_email("noreply@example.com"), None);
    }

    #[test]
    fn test_multiple_phone_numbers_single_contact() {
        let mut parser = VcfParser::new();
        
        // Create a contact with multiple phone numbers
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "BEGIN:VCARD").unwrap();
        writeln!(temp_file, "VERSION:3.0").unwrap();
        writeln!(temp_file, "FN:John Doe").unwrap();
        writeln!(temp_file, "TEL;type=HOME:+1234567890").unwrap();
        writeln!(temp_file, "TEL;type=CELL:+1987654321").unwrap();
        writeln!(temp_file, "TEL;type=WORK:(555) 123-4567").unwrap();
        writeln!(temp_file, "EMAIL:john@home.com").unwrap();
        writeln!(temp_file, "EMAIL:john@work.com").unwrap();
        writeln!(temp_file, "END:VCARD").unwrap();
        temp_file.flush().unwrap();

        parser.parse_vcf_file(temp_file.path()).unwrap();

        // All phone numbers should map to the same contact
        assert_eq!(parser.get_name_by_phone("+1234567890"), Some(&"John Doe".to_string()));
        assert_eq!(parser.get_name_by_phone("+1987654321"), Some(&"John Doe".to_string()));
        assert_eq!(parser.get_name_by_phone("5551234567"), Some(&"John Doe".to_string()));
        
        // All emails should map to the same contact
        assert_eq!(parser.get_name_by_email("john@home.com"), Some(&"John Doe".to_string()));
        assert_eq!(parser.get_name_by_email("john@work.com"), Some(&"John Doe".to_string()));
    }

    #[test]
    fn test_duplicate_contact_merging() {
        let mut parser = VcfParser::new();
        
        // Create VCF with duplicate contacts (like Hannah Agnew in the real file)
        let mut temp_file = NamedTempFile::new().unwrap();
        
        // First Hannah entry - just phone
        writeln!(temp_file, "BEGIN:VCARD").unwrap();
        writeln!(temp_file, "VERSION:3.0").unwrap();
        writeln!(temp_file, "FN:Hannah Agnew").unwrap();
        writeln!(temp_file, "TEL;type=pref:+19059143363").unwrap();
        writeln!(temp_file, "END:VCARD").unwrap();
        
        // Second Hannah entry - email and same phone
        writeln!(temp_file, "BEGIN:VCARD").unwrap();
        writeln!(temp_file, "VERSION:3.0").unwrap();
        writeln!(temp_file, "FN:Hannah Agnew").unwrap();
        writeln!(temp_file, "EMAIL;type=INTERNET:hannersthenanners@hotmail.com").unwrap();
        writeln!(temp_file, "TEL;type=pref:+19059143363").unwrap();
        writeln!(temp_file, "END:VCARD").unwrap();
        
        temp_file.flush().unwrap();

        parser.parse_vcf_file(temp_file.path()).unwrap();

        // Both phone and email should resolve to Hannah
        assert_eq!(parser.get_name_by_phone("+19059143363"), Some(&"Hannah Agnew".to_string()));
        assert_eq!(parser.get_name_by_email("hannersthenanners@hotmail.com"), Some(&"Hannah Agnew".to_string()));
        
        // Should not have duplicate phone numbers in the lookup
        let phone_entries: Vec<_> = parser.phone_to_name.iter()
            .filter(|(_, name)| *name == "Hannah Agnew")
            .collect();
        assert_eq!(phone_entries.len(), 1); // Only one phone entry despite appearing twice
    }

    #[test]
    fn test_duplicate_contact_merging_different_info() {
        let mut parser = VcfParser::new();
        
        // Create VCF with same person having different contact methods in separate entries
        let mut temp_file = NamedTempFile::new().unwrap();
        
        // First entry - home phone
        writeln!(temp_file, "BEGIN:VCARD").unwrap();
        writeln!(temp_file, "VERSION:3.0").unwrap();
        writeln!(temp_file, "FN:Alice Smith").unwrap();
        writeln!(temp_file, "TEL;type=HOME:+1234567890").unwrap();
        writeln!(temp_file, "END:VCARD").unwrap();
        
        // Second entry - work email
        writeln!(temp_file, "BEGIN:VCARD").unwrap();
        writeln!(temp_file, "VERSION:3.0").unwrap();
        writeln!(temp_file, "FN:Alice Smith").unwrap();
        writeln!(temp_file, "EMAIL;type=WORK:alice@company.com").unwrap();
        writeln!(temp_file, "END:VCARD").unwrap();
        
        // Third entry - cell phone
        writeln!(temp_file, "BEGIN:VCARD").unwrap();
        writeln!(temp_file, "VERSION:3.0").unwrap();
        writeln!(temp_file, "FN:Alice Smith").unwrap();
        writeln!(temp_file, "TEL;type=CELL:+1987654321").unwrap();
        writeln!(temp_file, "END:VCARD").unwrap();
        
        temp_file.flush().unwrap();

        parser.parse_vcf_file(temp_file.path()).unwrap();

        // All contact methods should resolve to Alice
        assert_eq!(parser.get_name_by_phone("+1234567890"), Some(&"Alice Smith".to_string()));
        assert_eq!(parser.get_name_by_phone("+1987654321"), Some(&"Alice Smith".to_string()));
        assert_eq!(parser.get_name_by_email("alice@company.com"), Some(&"Alice Smith".to_string()));
        
        // Should have merged all contact methods
        let alice_phone_entries: Vec<_> = parser.phone_to_name.iter()
            .filter(|(_, name)| *name == "Alice Smith")
            .collect();
        let alice_email_entries: Vec<_> = parser.email_to_name.iter()
            .filter(|(_, name)| *name == "Alice Smith")
            .collect();
            
        assert_eq!(alice_phone_entries.len(), 2); // Two phone numbers
        assert_eq!(alice_email_entries.len(), 1); // One email
    }
}
