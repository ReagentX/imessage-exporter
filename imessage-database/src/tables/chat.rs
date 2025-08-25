/*!
 This module represents common (but not all) columns in the `chat` table.
*/

use std::collections::HashMap;

use plist::Value;
use rusqlite::{CachedStatement, Connection, Error, Result, Row};

use crate::{
    error::{plist::PlistParseError, table::TableError},
    tables::{
        messages::models::Service,
        table::{CHAT, Cacheable, PROPERTIES, Table},
    },
    util::plist::{get_bool_from_dict, get_owned_string_from_dict},
};

/// Chat properties are stored as a `plist` in the database
/// This represents the metadata for a chatroom
#[derive(Debug, PartialEq, Eq)]
pub struct Properties {
    /// Whether the chat has read receipts enabled
    read_receipts_enabled: bool,
    /// The most recent message in the chat
    last_message_guid: Option<String>,
    /// Whether the chat was forced to use SMS/RCS instead of iMessage
    forced_sms: bool,
    /// GUID of the group photo, if it exists in the attachments table
    group_photo_guid: Option<String>,
}

impl Properties {
    /// Create a new `Properties` given a `plist` blob
    pub(self) fn from_plist(plist: &Value) -> Result<Self, PlistParseError> {
        Ok(Self {
            read_receipts_enabled: get_bool_from_dict(plist, "EnableReadReceiptForChat")
                .unwrap_or(false),
            last_message_guid: get_owned_string_from_dict(plist, "lastSeenMessageGuid"),
            forced_sms: get_bool_from_dict(plist, "shouldForceToSMS").unwrap_or(false),
            group_photo_guid: get_owned_string_from_dict(plist, "groupPhotoGuid"),
        })
    }
}

/// Represents a single row in the `chat` table.
#[derive(Debug)]
pub struct Chat {
    /// The unique identifier for the chat in the database
    pub rowid: i32,
    /// The identifier for the chat, typically a phone number, email, or group chat ID
    pub chat_identifier: String,
    /// The service the chat used, i.e. iMessage, SMS, IRC, etc.
    pub service_name: Option<String>,
    /// Optional custom name created created for the chat
    pub display_name: Option<String>,
}

impl Table for Chat {
    fn from_row(row: &Row) -> Result<Chat> {
        Ok(Chat {
            rowid: row.get("rowid")?,
            chat_identifier: row.get("chat_identifier")?,
            service_name: row.get("service_name")?,
            display_name: row.get("display_name").unwrap_or(None),
        })
    }

    fn get(db: &Connection) -> Result<CachedStatement, TableError> {
        Ok(db.prepare_cached(&format!("SELECT * from {CHAT}"))?)
    }

    fn extract(chat: Result<Result<Self, Error>, Error>) -> Result<Self, TableError> {
        match chat {
            Ok(Ok(chat)) => Ok(chat),
            Err(why) | Ok(Err(why)) => Err(TableError::QueryError(why)),
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
    /// let conn = get_connection(&db_path).unwrap();
    /// let chatrooms = Chat::cache(&conn);
    /// ```
    fn cache(db: &Connection) -> Result<HashMap<Self::K, Self::V>, TableError> {
        let mut map = HashMap::new();

        let mut statement = Chat::get(db)?;

        let chats = statement.query_map([], |row| Ok(Chat::from_row(row)))?;

        for chat in chats {
            let result = Chat::extract(chat)?;
            map.insert(result.rowid, result);
        }
        Ok(map)
    }
}

impl Chat {
    /// Generate a name for a chat, falling back to the default if a custom one is not set
    #[must_use]
    pub fn name(&self) -> &str {
        match self.display_name() {
            Some(name) => name,
            None => &self.chat_identifier,
        }
    }

    /// Get the current display name for the chat, if it exists.
    #[must_use]
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

    /// Get the service used by the chat, i.e. iMessage, SMS, IRC, etc.
    #[must_use]
    pub fn service(&self) -> Service {
        Service::from(self.service_name.as_deref())
    }

    /// Get the [`Properties`] for the chat, if they exist
    ///
    /// Calling this hits the database, so it is expensive and should
    /// only get invoked when needed.
    #[must_use]
    pub fn properties(&self, db: &Connection) -> Option<Properties> {
        match Value::from_reader(self.get_blob(db, CHAT, PROPERTIES, self.rowid.into())?) {
            Ok(plist) => Properties::from_plist(&plist).ok(),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod test_properties {
    use plist::Value;
    use std::env::current_dir;
    use std::fs::File;

    use crate::tables::chat::Properties;

    #[test]
    fn test_can_parse_properties_simple() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/chat_properties/ChatProp1.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("Parsed plist: {plist:#?}");

        let actual = Properties::from_plist(&plist).unwrap();
        let expected = Properties {
            read_receipts_enabled: false,
            last_message_guid: Some(String::from("FF0615B9-C4AF-4BD8-B9A8-1B5F9351033F")),
            forced_sms: false,
            group_photo_guid: None,
        };
        print!("Parsed properties: {expected:?}");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_can_parse_properties_enable_read_receipts() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/chat_properties/ChatProp2.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("Parsed plist: {plist:#?}");

        let actual = Properties::from_plist(&plist).unwrap();
        let expected = Properties {
            read_receipts_enabled: true,
            last_message_guid: Some(String::from("678BA15C-C309-FAAC-3678-78ACE995EB54")),
            forced_sms: false,
            group_photo_guid: None,
        };
        print!("Parsed properties: {expected:?}");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_can_parse_properties_third_with_summary() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/chat_properties/ChatProp3.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("Parsed plist: {plist:#?}");

        let actual = Properties::from_plist(&plist).unwrap();
        let expected = Properties {
            read_receipts_enabled: false,
            last_message_guid: Some(String::from("CEE419B6-17C7-42F7-8C2A-09A38CCA5730")),
            forced_sms: false,
            group_photo_guid: None,
        };
        print!("Parsed properties: {expected:?}");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_can_parse_properties_forced_sms() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/chat_properties/ChatProp4.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("Parsed plist: {plist:#?}");

        let actual = Properties::from_plist(&plist).unwrap();
        let expected = Properties {
            read_receipts_enabled: false,
            last_message_guid: Some(String::from("87D5257D-6536-4067-A8A0-E7EF10ECBA9D")),
            forced_sms: true,
            group_photo_guid: None,
        };
        print!("Parsed properties: {expected:?}");
        assert_eq!(actual, expected);
    }
}
