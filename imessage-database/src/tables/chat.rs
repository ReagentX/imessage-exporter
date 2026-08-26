/*!
 Chat table rows and chat metadata helpers.
*/

use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use plist::Value;
use rusqlite::{CachedStatement, Connection, Result, Row};

use crate::{
    error::{plist::PlistParseError, table::TableError},
    tables::{
        messages::models::Service,
        table::{CHAT, Cacheable, PROPERTIES, Table},
    },
    util::plist::{
        extract_dictionary, extract_string_key, get_bool_from_dict, get_owned_string_from_dict,
        plist_as_dictionary,
    },
};

// MARK: Chat Props
/// Metadata stored in the `chat.properties` plist.
#[derive(Debug, PartialEq, Eq)]
pub struct Properties {
    /// Whether read receipts are enabled for the chat.
    pub read_receipts_enabled: bool,
    /// Most recent message GUID recorded for the chat.
    pub last_message_guid: Option<String>,
    /// Whether Messages forced SMS/RCS instead of iMessage.
    pub forced_sms: bool,
    /// Group photo attachment GUID.
    pub group_photo_guid: Option<String>,
    /// Whether the chat has a custom background.
    pub has_chat_background: bool,
}

impl Properties {
    /// Parse chat properties from a plist value.
    pub(self) fn from_plist(plist: &Value) -> Result<Self, PlistParseError> {
        Ok(Self {
            read_receipts_enabled: get_bool_from_dict(plist, "EnableReadReceiptForChat")
                .unwrap_or(false),
            last_message_guid: get_owned_string_from_dict(plist, "lastSeenMessageGuid"),
            forced_sms: get_bool_from_dict(plist, "shouldForceToSMS").unwrap_or(false),
            group_photo_guid: get_owned_string_from_dict(plist, "groupPhotoGuid"),
            has_chat_background: plist_as_dictionary(plist)
                .and_then(|dict| extract_dictionary(dict, "backgroundProperties"))
                .and_then(|dict| extract_string_key(dict, "trabar"))
                .is_ok(),
        })
    }
}

// MARK: Chat Filter Status
/// Conversation-list tier stored in `chat.is_filtered`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatFilterStatus {
    /// Main conversation list (`0`).
    Unfiltered,
    /// Unknown Senders bucket (`1`).
    UnknownSenders,
    /// Junk bucket (`2`).
    Junk,
    /// Unrecognized raw value.
    Unknown(i32),
}

impl ChatFilterStatus {
    /// Map a raw `is_filtered` value to its conversation-list tier.
    ///
    /// Preserve a missing value as `None` and an unrecognized value as
    /// [`Self::Unknown`].
    #[must_use]
    pub fn from_code(code: Option<i32>) -> Option<Self> {
        Some(match code? {
            0 => Self::Unfiltered,
            1 => Self::UnknownSenders,
            2 => Self::Junk,
            other => Self::Unknown(other),
        })
    }

    /// Return whether the status is outside the main conversation list.
    ///
    /// Every variant except [`Self::Unfiltered`] is filtered, including
    /// [`Self::Unknown`].
    #[must_use]
    pub fn is_filtered(&self) -> bool {
        !matches!(self, Self::Unfiltered)
    }
}

impl Display for ChatFilterStatus {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(code) => write!(fmt, "Unknown ({code})"),
            _ => write!(fmt, "{self:?}"),
        }
    }
}

// MARK: Chat Struct
/// Row from the `chat` table.
#[derive(Debug)]
pub struct Chat {
    /// Chat row ID.
    pub rowid: i32,
    /// Phone number, email, or group chat identifier.
    pub chat_identifier: String,
    /// Service name stored for the chat.
    pub service_name: Option<String>,
    /// User-provided chat display name.
    pub display_name: Option<String>,
    /// Raw conversation-list tier from `chat.is_filtered`, used to build
    /// the [`ChatFilterStatus`] via [`Self::filter_status`].
    pub is_filtered: Option<i32>,
    /// Raw `chat.is_blackholed` flag: `1` denotes a chat whose incoming
    /// messages are silently dropped.
    pub is_blackholed: Option<bool>,
    /// Raw `chat.is_pending_review` flag: `1` denotes a filtered chat awaiting
    /// review.
    pub is_pending_review: Option<bool>,
}

// MARK: Table
impl Table for Chat {
    fn from_row(row: &Row) -> Result<Chat> {
        Ok(Chat {
            rowid: row.get("rowid")?,
            chat_identifier: row.get("chat_identifier")?,
            service_name: row.get("service_name")?,
            display_name: row.get("display_name").unwrap_or(None),
            is_filtered: row.get("is_filtered").unwrap_or(None),
            is_blackholed: row.get("is_blackholed").unwrap_or(None),
            is_pending_review: row.get("is_pending_review").unwrap_or(None),
        })
    }

    fn get(db: &'_ Connection) -> Result<CachedStatement<'_>, TableError> {
        Ok(db.prepare_cached(&format!("SELECT * from {CHAT}"))?)
    }
}

// MARK: Cache
impl Cacheable for Chat {
    type K = i32;
    type V = Chat;
    /// Cache chat rows by row ID.
    ///
    /// Chat row IDs can represent duplicate conversations; deduplication happens
    /// after participant handles are loaded.
    ///
    /// # Example:
    ///
    /// ```no_run
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

        for chat in Chat::rows(&mut statement, [])? {
            let result = chat?;
            map.insert(result.rowid, result);
        }
        Ok(map)
    }
}

impl Chat {
    /// Return the display name, falling back to the chat identifier.
    #[must_use]
    pub fn name(&self) -> &str {
        match self.display_name() {
            Some(name) => name,
            None => &self.chat_identifier,
        }
    }

    /// Return the non-empty custom display name.
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

    /// Return the chat service as a [`Service`].
    #[must_use]
    pub fn service(&'_ self) -> Service<'_> {
        Service::from_name(self.service_name.as_deref())
    }

    /// Parse the conversation-list tier from [`Self::is_filtered`].
    ///
    /// A raw `0` maps to [`ChatFilterStatus::Unfiltered`]. `None` remains
    /// `None`, and every unrecognized value maps to [`ChatFilterStatus::Unknown`].
    #[must_use]
    pub fn filter_status(&self) -> Option<ChatFilterStatus> {
        ChatFilterStatus::from_code(self.is_filtered)
    }

    /// Parse [`Properties`] from the chat's plist blob.
    ///
    /// Calling this reads a BLOB from the database.
    #[must_use]
    pub fn properties(&self, db: &Connection) -> Option<Properties> {
        match Value::from_reader(self.get_blob(db, CHAT, PROPERTIES, self.rowid.into())?) {
            Ok(plist) => Properties::from_plist(&plist).ok(),
            Err(_) => None,
        }
    }
}

// MARK: Tests
#[cfg(test)]
mod test_filter_status {
    use crate::tables::chat::ChatFilterStatus;

    #[test]
    fn maps_known_codes() {
        assert_eq!(
            ChatFilterStatus::from_code(Some(0)),
            Some(ChatFilterStatus::Unfiltered)
        );
        assert_eq!(
            ChatFilterStatus::from_code(Some(1)),
            Some(ChatFilterStatus::UnknownSenders)
        );
        assert_eq!(
            ChatFilterStatus::from_code(Some(2)),
            Some(ChatFilterStatus::Junk)
        );
    }

    #[test]
    fn preserves_unrecognized_code() {
        assert_eq!(
            ChatFilterStatus::from_code(Some(7)),
            Some(ChatFilterStatus::Unknown(7))
        );
    }

    #[test]
    fn missing_value_is_none() {
        assert_eq!(ChatFilterStatus::from_code(None), None);
    }

    #[test]
    fn is_filtered_covers_every_nonzero_tier() {
        assert!(!ChatFilterStatus::Unfiltered.is_filtered());
        assert!(ChatFilterStatus::UnknownSenders.is_filtered());
        assert!(ChatFilterStatus::Junk.is_filtered());
        assert!(ChatFilterStatus::Unknown(7).is_filtered());
    }

    #[test]
    fn display_names_the_unknown_code() {
        assert_eq!(ChatFilterStatus::Junk.to_string(), "Junk");
        assert_eq!(ChatFilterStatus::Unknown(7).to_string(), "Unknown (7)");
    }
}

#[cfg(test)]
mod test_from_row {
    use rusqlite::Connection;

    use crate::tables::{
        chat::{Chat, ChatFilterStatus},
        table::Table,
    };

    /// Build a minimal in-memory `chat` table, optionally including filter-state columns.
    fn chat_db(with_filter_columns: bool) -> Connection {
        let filter_columns = if with_filter_columns {
            ",
                is_filtered INTEGER DEFAULT 0,
                is_blackholed INTEGER DEFAULT 0,
                is_pending_review INTEGER DEFAULT 0"
        } else {
            ""
        };
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(&format!(
            "CREATE TABLE chat (
                ROWID INTEGER PRIMARY KEY,
                chat_identifier TEXT,
                service_name TEXT,
                display_name TEXT{filter_columns}
            );"
        ))
        .unwrap();
        db
    }

    fn all_chats(db: &Connection) -> Vec<Chat> {
        let mut statement = Chat::get(db).unwrap();
        Chat::rows(&mut statement, [])
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn reads_filter_state_codes() {
        let db = chat_db(true);
        db.execute_batch(
            "INSERT INTO chat (ROWID, chat_identifier, is_filtered, is_blackholed, is_pending_review) VALUES
                (1, 'a', 0, 0, 0),
                (2, 'b', 1, 1, 1),
                (3, 'c', NULL, NULL, NULL);",
        )
        .unwrap();

        let chats = all_chats(&db);
        assert_eq!(chats[0].is_filtered, Some(0));
        assert_eq!(chats[0].filter_status(), Some(ChatFilterStatus::Unfiltered));
        assert_eq!(chats[0].is_blackholed, Some(false));
        assert_eq!(chats[0].is_pending_review, Some(false));
        assert_eq!(chats[1].is_filtered, Some(1));
        assert_eq!(
            chats[1].filter_status(),
            Some(ChatFilterStatus::UnknownSenders)
        );
        assert_eq!(chats[1].is_blackholed, Some(true));
        assert_eq!(chats[1].is_pending_review, Some(true));
        assert_eq!(chats[2].is_filtered, None);
        assert_eq!(chats[2].filter_status(), None);
        assert_eq!(chats[2].is_blackholed, None);
        assert_eq!(chats[2].is_pending_review, None);
    }

    #[test]
    fn schema_without_filter_columns_reads_none() {
        let db = chat_db(false);
        db.execute_batch("INSERT INTO chat (ROWID, chat_identifier) VALUES (1, 'a');")
            .unwrap();

        let chats = all_chats(&db);
        assert_eq!(chats[0].is_filtered, None);
        assert_eq!(chats[0].is_blackholed, None);
        assert_eq!(chats[0].is_pending_review, None);
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
            has_chat_background: false,
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
            has_chat_background: false,
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
            has_chat_background: false,
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
            has_chat_background: false,
        };
        print!("Parsed properties: {expected:?}");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_can_parse_properties_no_background() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/chat_properties/before_background.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("Parsed plist: {plist:#?}");

        let actual = Properties::from_plist(&plist).unwrap();
        let expected = Properties {
            read_receipts_enabled: true,
            last_message_guid: Some(String::from("49DA49E8-0000-0000-B59E-290294670E7D")),
            forced_sms: false,
            group_photo_guid: None,
            has_chat_background: false,
        };
        print!("Parsed properties: {expected:?}");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_can_parse_properties_added_background() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/chat_properties/after_background_preset.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("Parsed plist: {plist:#?}");

        let actual = Properties::from_plist(&plist).unwrap();
        let expected = Properties {
            read_receipts_enabled: true,
            last_message_guid: Some(String::from("49DA49E8-0000-0000-B59E-290294670E7D")),
            forced_sms: false,
            group_photo_guid: None,
            has_chat_background: true,
        };
        print!("Parsed properties: {expected:?}");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_can_parse_properties_removed_background() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/chat_properties/after_background_removed.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("Parsed plist: {plist:#?}");

        let actual = Properties::from_plist(&plist).unwrap();
        let expected = Properties {
            read_receipts_enabled: true,
            last_message_guid: Some(String::from("49DA49E8-0000-0000-B59E-290294670E7D")),
            forced_sms: false,
            group_photo_guid: None,
            has_chat_background: false,
        };
        print!("Parsed properties: {expected:?}");
        assert_eq!(actual, expected);
    }
}
