use std::{
    cmp::min,
    collections::{BTreeSet, HashMap, HashSet},
    fs::create_dir_all,
    path::PathBuf,
};

use rusqlite::Connection;

use crate::{
    app::{
        attachment_manager::AttachmentManager, converter::Converter, error::RuntimeError,
        options::Options, sanitizers::sanitize_filename,
    },
    Exporter, HTML, TXT,
};

use imessage_database::{
    error::table::TableError,
    tables::{
        attachment::Attachment,
        chat::Chat,
        chat_handle::ChatToHandle,
        handle::Handle,
        messages::Message,
        table::{
            get_connection, Cacheable, Deduplicate, Diagnostic, ATTACHMENTS_DIR, MAX_LENGTH, ME,
            ORPHANED, UNKNOWN,
        },
    },
    util::dates::get_offset,
};

/// Stores the application state and handles application lifecycle
pub struct Config<'a> {
    /// Map of chatroom ID to chatroom information
    pub chatrooms: HashMap<i32, Chat>,
    // Map of chatroom ID to an internal unique chatroom ID
    pub real_chatrooms: HashMap<i32, i32>,
    /// Map of chatroom ID to chatroom participants
    pub chatroom_participants: HashMap<i32, BTreeSet<i32>>,
    /// Map of participant ID to contact info
    pub participants: HashMap<i32, String>,
    /// Map of participant ID to an internal unique participant ID
    pub real_participants: HashMap<i32, i32>,
    /// Messages that are reactions to other messages
    pub reactions: HashMap<String, HashMap<usize, Vec<Message>>>,
    /// App configuration options
    pub options: Options<'a>,
    /// Global date offset used by the iMessage database:
    pub offset: i64,
    /// The connection we use to query the database
    pub db: Connection,
    /// Converter type used when converting image files
    pub converter: Option<Converter>,
}

impl<'a> Config<'a> {
    /// Get a deduplicated chat ID or a default value
    pub fn conversation(&self, message: &Message) -> Option<(&Chat, &i32)> {
        match message.chat_id.or(message.deleted_from) {
            Some(chat_id) => match self.chatrooms.get(&chat_id) {
                Some(chatroom) => self.real_chatrooms.get(&chat_id).map(|id| (chatroom, id)),
                // No chatroom for the given chat_id
                None => {
                    eprintln!("Chat ID {chat_id} does not exist in chat table!");
                    None
                }
            },
            // No chat_id provided
            None => None,
        }
    }

    /// Get the attachment path for the current session
    pub fn attachment_path(&self) -> PathBuf {
        let mut path = self.options.export_path.clone();
        path.push(ATTACHMENTS_DIR);
        path
    }

    /// Get the attachment path for a specific chat ID
    pub fn conversation_attachment_path(&self, chat_id: Option<i32>) -> String {
        if let Some(chat_id) = chat_id {
            if let Some(real_id) = self.real_chatrooms.get(&chat_id) {
                return real_id.to_string();
            }
        }
        String::from(ORPHANED)
    }

    /// Generate a file path for an attachment
    ///
    /// If the attachment was copied, use that path
    /// if not, default to the filename
    pub fn message_attachment_path(&self, attachment: &Attachment) -> String {
        // Build a relative filepath from the fully qualified one on the `Attachment`
        match &attachment.copied_path {
            Some(path) => {
                if let Ok(relative_path) = path.strip_prefix(&self.options.export_path) {
                    return relative_path.display().to_string();
                }
                path.display().to_string()
            }
            None => attachment
                .resolved_attachment_path(&self.options.platform, &self.options.db_path)
                .unwrap_or(attachment.filename().to_string()),
        }
    }

    /// Get a filename for a chat, possibly using cached data.
    ///
    /// If the chat has an assigned name, use that, truncating if necessary.
    ///
    /// If it does not, first try and make a flat list of its members. Failing that, use the unique `chat_identifier` field.
    pub fn filename(&self, chatroom: &Chat) -> String {
        let filename = match &chatroom.display_name() {
            // If there is a display name, use that
            Some(name) => {
                format!(
                    "{} - {}",
                    &name[..min(MAX_LENGTH, name.len())],
                    chatroom.rowid
                )
            }
            // Fallback if there is no name set
            None => match self.chatroom_participants.get(&chatroom.rowid) {
                // List of participant names
                Some(participants) => self.filename_from_participants(participants),
                // Unique chat_identifier
                None => {
                    eprintln!(
                        "Found error: message chat ID {} has no members!",
                        chatroom.rowid
                    );
                    chatroom.chat_identifier.to_owned()
                }
            },
        };
        sanitize_filename(filename)
    }

    /// Generate a filename from a set of participants, truncating if the name is too long
    ///
    /// - All names:
    ///   - Contact 1, Contact 2
    /// - Truncated Names
    ///   - Contact 1, Contact 2, ... Contact 13 and 4 others
    fn filename_from_participants(&self, participants: &BTreeSet<i32>) -> String {
        let mut added = 0;
        let mut out_s = String::with_capacity(MAX_LENGTH);
        for participant_id in participants.iter() {
            let participant = self.who(&Some(*participant_id), false);
            if participant.len() + out_s.len() < MAX_LENGTH {
                if !out_s.is_empty() {
                    out_s.push_str(", ")
                }
                out_s.push_str(participant);
                added += 1;
            } else {
                let extra = format!(", and {} others", participants.len() - added);
                let space_remaining = extra.len() + out_s.len();
                if space_remaining >= MAX_LENGTH {
                    out_s.replace_range((MAX_LENGTH - extra.len()).., &extra);
                } else if out_s.is_empty() {
                    out_s.push_str(&participant[..MAX_LENGTH])
                } else {
                    out_s.push_str(&extra);
                }
                break;
            }
        }
        out_s
    }

    /// Create a new instance of the application
    ///
    /// # Example:
    ///
    /// ```
    /// use crate::app::{
    ///    options::{from_command_line, Options},
    ///    runtime::Config,
    /// };
    ///
    /// let args = from_command_line();
    /// let options = Options::from_args(&args);
    /// let app = Config::new(options).unwrap();
    /// ```
    pub fn new(options: Options) -> Result<Config, RuntimeError> {
        let conn = get_connection(&options.get_db_path()).map_err(RuntimeError::DatabaseError)?;
        eprintln!("Building cache...");
        eprintln!("[1/4] Caching chats...");
        let chatrooms = Chat::cache(&conn).map_err(RuntimeError::DatabaseError)?;
        eprintln!("[2/4] Caching chatrooms...");
        let chatroom_participants =
            ChatToHandle::cache(&conn).map_err(RuntimeError::DatabaseError)?;
        eprintln!("[3/4] Caching participants...");
        let participants = Handle::cache(&conn).map_err(RuntimeError::DatabaseError)?;
        eprintln!("[4/4] Caching reactions...");
        let reactions = Message::cache(&conn).map_err(RuntimeError::DatabaseError)?;
        eprintln!("Cache built!");
        Ok(Config {
            chatrooms,
            real_chatrooms: ChatToHandle::dedupe(&chatroom_participants),
            chatroom_participants,
            real_participants: Handle::dedupe(&participants),
            participants,
            reactions,
            options,
            offset: get_offset(),
            db: conn,
            converter: Converter::determine(),
        })
    }

    /// Handles diagnostic tests for database
    fn run_diagnostic(&self) -> Result<(), TableError> {
        println!("\niMessage Database Diagnostics\n");
        Handle::run_diagnostic(&self.db)?;
        Message::run_diagnostic(&self.db)?;
        Attachment::run_diagnostic(&self.db, &self.options.db_path, &self.options.platform)?;
        ChatToHandle::run_diagnostic(&self.db)?;

        // Global Diagnostics
        let unique_handles: HashSet<i32> =
            HashSet::from_iter(self.real_participants.values().cloned());
        let duplicated_handles = self.participants.len() - unique_handles.len();
        if duplicated_handles > 0 {
            println!("Duplicated contacts: {duplicated_handles}");
        }

        let unique_chats: HashSet<i32> = HashSet::from_iter(self.real_chatrooms.values().cloned());
        let duplicated_chats = self.chatrooms.len() - unique_chats.len();
        if duplicated_chats > 0 {
            println!("Duplicated chats: {duplicated_chats}");
        }
        Ok(())
    }

    /// Start the app given the provided set of options. This will either run
    /// diagnostic tests on the database or export data to the specified file type.
    ///
    // # Example:
    ///
    /// ```
    /// use crate::app::{
    ///    options::{from_command_line, Options},
    ///    runtime::Config,
    /// };
    ///
    /// let args = from_command_line();
    /// let options = Options::from_args(&args);
    /// let app = Config::new(options).unwrap();
    /// app.start();
    /// ```
    pub fn start(&self) -> Result<(), RuntimeError> {
        if self.options.diagnostic {
            self.run_diagnostic().map_err(RuntimeError::DatabaseError)?;
        } else if self.options.export_type.is_some() {
            // Ensure the path we want to export to exists
            create_dir_all(&self.options.export_path).map_err(RuntimeError::DiskError)?;

            match self.options.export_type.unwrap_or_default() {
                "txt" => {
                    // Create exporter, pass it data we care about, then kick it off
                    TXT::new(self).iter_messages()?;
                }
                "html" => {
                    if !matches!(self.options.attachment_manager, AttachmentManager::Disabled) {
                        create_dir_all(self.attachment_path()).map_err(RuntimeError::DiskError)?;
                    }
                    HTML::new(self).iter_messages()?;
                }
                _ => {
                    unreachable!()
                }
            }
        }
        println!("Done!");
        Ok(())
    }

    /// Determine who sent a message
    pub fn who(&self, handle_id: &Option<i32>, is_from_me: bool) -> &str {
        if is_from_me {
            return self.options.custom_name.unwrap_or(ME);
        } else if let Some(handle_id) = handle_id {
            return match self.participants.get(handle_id) {
                Some(contact) => contact,
                None => UNKNOWN,
            };
        }
        UNKNOWN
    }
}

#[cfg(test)]
mod filename_tests {
    use crate::{app::attachment_manager::AttachmentManager, Config, Options};
    use imessage_database::{
        tables::{
            chat::Chat,
            table::{get_connection, MAX_LENGTH},
        },
        util::{dirs::default_db_path, platform::Platform, query_context::QueryContext},
    };
    use std::{
        collections::{BTreeSet, HashMap},
        path::PathBuf,
    };

    fn fake_options<'a>() -> Options<'a> {
        Options {
            db_path: default_db_path(),
            attachment_manager: AttachmentManager::Disabled,
            diagnostic: false,
            export_type: None,
            export_path: PathBuf::new(),
            query_context: QueryContext::default(),
            no_lazy: false,
            custom_name: None,
            platform: Platform::macOS,
        }
    }

    fn fake_chat() -> Chat {
        Chat {
            rowid: 0,
            chat_identifier: "Default".to_string(),
            service_name: "".to_string(),
            display_name: None,
        }
    }

    fn fake_app(options: Options) -> Config {
        let connection = get_connection(&options.db_path).unwrap();
        Config {
            chatrooms: HashMap::new(),
            real_chatrooms: HashMap::new(),
            chatroom_participants: HashMap::new(),
            participants: HashMap::new(),
            real_participants: HashMap::new(),
            reactions: HashMap::new(),
            options,
            offset: 0,
            db: connection,
            converter: Some(crate::app::converter::Converter::Sips),
        }
    }

    #[test]
    fn can_create() {
        let options = fake_options();
        let app = fake_app(options);
        app.start().unwrap();
    }

    #[test]
    fn can_get_filename_good() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create participant data
        app.participants.insert(10, "Person 10".to_string());
        app.participants.insert(11, "Person 11".to_string());

        // Add participants
        let mut people = BTreeSet::new();
        people.insert(10);
        people.insert(11);

        // Get filename
        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "Person 10, Person 11".to_string());
        assert!(filename.len() <= MAX_LENGTH);
    }

    #[test]
    fn can_get_filename_long_multiple() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create participant data
        app.participants.insert(
            10,
            "Person With An Extremely and Excessively Long Name 10".to_string(),
        );
        app.participants.insert(
            11,
            "Person With An Extremely and Excessively Long Name 11".to_string(),
        );
        app.participants.insert(
            12,
            "Person With An Extremely and Excessively Long Name 12".to_string(),
        );
        app.participants.insert(
            13,
            "Person With An Extremely and Excessively Long Name 13".to_string(),
        );
        app.participants.insert(
            14,
            "Person With An Extremely and Excessively Long Name 14".to_string(),
        );
        app.participants.insert(
            15,
            "Person With An Extremely and Excessively Long Name 15".to_string(),
        );
        app.participants.insert(
            16,
            "Person With An Extremely and Excessively Long Name 16".to_string(),
        );
        app.participants.insert(
            17,
            "Person With An Extremely and Excessively Long Name 17".to_string(),
        );

        // Add participants
        let mut people = BTreeSet::new();
        people.insert(10);
        people.insert(11);
        people.insert(12);
        people.insert(13);
        people.insert(14);
        people.insert(15);
        people.insert(16);
        people.insert(17);

        // Get filename
        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "Person With An Extremely and Excessively Long Name 10, Person With An Extremely and Excessively Long Name 11, Person With An Extremely and Excessively Long Name 12, Person With An Extremely and Excessively Long Name 13, and 4 others".to_string());
        assert!(filename.len() <= MAX_LENGTH);
    }

    #[test]
    fn can_get_filename_single_long() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create participant data
        app.participants.insert(10, "He slipped his key into the lock, and we all very quietly entered the cell. The sleeper half turned, and then settled down once more into a deep slumber. Holmes stooped to the water-jug, moistened his sponge, and then rubbed it twice vigorously across and down the prisoner's face.".to_string());

        // Add 1 person
        let mut people = BTreeSet::new();
        people.insert(10);

        // Get filename
        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "He slipped his key into the lock, and we all very quietly entered the cell. The sleeper half turned, and then settled down once more into a deep slumber. Holmes stooped to the water-jug, moistened his sponge, and then rubbed it twice vigoro".to_string());
        assert!(filename.len() <= MAX_LENGTH);
    }

    #[test]
    fn can_get_filename_chat_display_name_long() {
        let options = fake_options();
        let app = fake_app(options);

        // Create chat
        let mut chat = fake_chat();
        chat.display_name = Some("Life is infinitely stranger than anything which the mind of man could invent. We would not dare to conceive the things which are really mere commonplaces of existence. If we could fly out of that window hand in hand, hover over this great city, gently remove the roofs".to_string());

        // Get filename
        let filename = app.filename(&chat);
        assert_eq!(filename, "Life is infinitely stranger than anything which the mind of man could invent. We would not dare to conceive the things which are really mere commonplaces of existence. If we could fly out of that window hand in hand, hover over this great c - 0");
    }

    #[test]
    fn can_get_filename_chat_display_name_normal() {
        let options = fake_options();
        let app = fake_app(options);

        // Create chat
        let mut chat = fake_chat();
        chat.display_name = Some("Test Chat Name".to_string());

        // Get filename
        let filename = app.filename(&chat);
        assert_eq!(filename, "Test Chat Name - 0");
    }

    #[test]
    fn can_get_filename_chat_display_name_short() {
        let options = fake_options();
        let app = fake_app(options);

        // Create chat
        let mut chat = fake_chat();
        chat.display_name = Some("🤠".to_string());

        // Get filename
        let filename = app.filename(&chat);
        assert_eq!(filename, "🤠 - 0");
    }

    #[test]
    fn can_get_filename_chat_participants() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create chat
        let chat = fake_chat();

        // Create participant data
        app.participants.insert(10, "Person 10".to_string());
        app.participants.insert(11, "Person 11".to_string());

        // Add participants
        let mut people = BTreeSet::new();
        people.insert(10);
        people.insert(11);
        app.chatroom_participants.insert(chat.rowid, people);

        // Get filename
        let filename = app.filename(&chat);
        assert_eq!(filename, "Person 10, Person 11");
    }

    #[test]
    fn can_get_filename_chat_no_participants() {
        let options = fake_options();
        let app = fake_app(options);

        // Create chat
        let chat = fake_chat();

        // Get filename
        let filename = app.filename(&chat);
        assert_eq!(filename, "Default");
    }
}

#[cfg(test)]
mod who_tests {
    use crate::{app::attachment_manager::AttachmentManager, Config, Options};
    use imessage_database::{
        tables::{chat::Chat, messages::Message, table::get_connection},
        util::{dirs::default_db_path, platform::Platform, query_context::QueryContext},
    };
    use std::{collections::HashMap, path::PathBuf};

    fn fake_options<'a>() -> Options<'a> {
        Options {
            db_path: default_db_path(),
            attachment_manager: AttachmentManager::Disabled,
            diagnostic: false,
            export_type: None,
            export_path: PathBuf::new(),
            query_context: QueryContext::default(),
            no_lazy: false,
            custom_name: None,
            platform: Platform::macOS,
        }
    }

    fn fake_chat() -> Chat {
        Chat {
            rowid: 0,
            chat_identifier: "Default".to_string(),
            service_name: "".to_string(),
            display_name: None,
        }
    }

    fn fake_app(options: Options) -> Config {
        let connection = get_connection(&options.db_path).unwrap();
        Config {
            chatrooms: HashMap::new(),
            real_chatrooms: HashMap::new(),
            chatroom_participants: HashMap::new(),
            participants: HashMap::new(),
            real_participants: HashMap::new(),
            reactions: HashMap::new(),
            options,
            offset: 0,
            db: connection,
            converter: Some(crate::app::converter::Converter::Sips),
        }
    }

    fn blank() -> Message {
        Message {
            rowid: i32::default(),
            guid: String::default(),
            text: None,
            service: Some("iMessage".to_string()),
            handle_id: Some(i32::default()),
            subject: None,
            date: i64::default(),
            date_read: i64::default(),
            date_delivered: i64::default(),
            is_from_me: false,
            is_read: false,
            item_type: 0,
            group_title: None,
            group_action_type: 0,
            associated_message_guid: None,
            associated_message_type: Some(i32::default()),
            balloon_bundle_id: None,
            expressive_send_style_id: None,
            thread_originator_guid: None,
            thread_originator_part: None,
            date_edited: 0,
            chat_id: None,
            num_attachments: 0,
            deleted_from: None,
            num_replies: 0,
        }
    }

    #[test]
    fn can_get_who_them() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create participant data
        app.participants.insert(10, "Person 10".to_string());

        // Get participant name
        let who = app.who(&Some(10), false);
        assert_eq!(who, "Person 10".to_string());
    }

    #[test]
    fn can_get_who_them_missing() {
        let options = fake_options();
        let app = fake_app(options);

        // Get participant name
        let who = app.who(&Some(10), false);
        assert_eq!(who, "Unknown".to_string());
    }

    #[test]
    fn can_get_who_me() {
        let options = fake_options();
        let app = fake_app(options);

        // Get participant name
        let who = app.who(&Some(0), true);
        assert_eq!(who, "Me".to_string());
    }

    #[test]
    fn can_get_who_me_custom() {
        let mut options = fake_options();
        options.custom_name = Some("Name");
        let app = fake_app(options);

        // Get participant name
        let who = app.who(&Some(0), true);
        assert_eq!(who, "Name".to_string());
    }

    #[test]
    fn can_get_who_none_me() {
        let options = fake_options();
        let app = fake_app(options);

        // Get participant name
        let who = app.who(&None, true);
        assert_eq!(who, "Me".to_string());
    }

    #[test]
    fn can_get_who_none_them() {
        let options = fake_options();
        let app = fake_app(options);

        // Get participant name
        let who = app.who(&None, false);
        assert_eq!(who, "Unknown".to_string());
    }

    #[test]
    fn can_get_chat_valid() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create chat
        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        // Create message
        let mut message = blank();
        message.chat_id = Some(0);

        // Get filename
        let (_, id) = app.conversation(&message).unwrap();
        assert_eq!(id, &0);
    }

    #[test]
    fn can_get_chat_valid_deleted() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create chat
        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        // Create message
        let mut message = blank();
        message.chat_id = None;
        message.deleted_from = Some(0);

        // Get filename
        let (_, id) = app.conversation(&message).unwrap();
        assert_eq!(id, &0);
    }

    #[test]
    fn can_get_chat_invalid() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create chat
        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        // Create message
        let mut message = blank();
        message.chat_id = Some(1);

        // Get filename
        let room = app.conversation(&message);
        assert!(room.is_none());
    }

    #[test]
    fn can_get_chat_none() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create chat
        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        // Create message
        let mut message = blank();
        message.chat_id = None;
        message.deleted_from = None;

        // Get filename
        let room = app.conversation(&message);
        assert!(room.is_none());
    }
}

#[cfg(test)]
mod directory_tests {
    use crate::{app::attachment_manager::AttachmentManager, Config, Options};
    use imessage_database::{
        tables::{attachment::Attachment, table::get_connection},
        util::{dirs::default_db_path, platform::Platform, query_context::QueryContext},
    };
    use std::{collections::HashMap, path::PathBuf};

    fn fake_options<'a>() -> Options<'a> {
        Options {
            db_path: default_db_path(),
            attachment_manager: AttachmentManager::Disabled,
            diagnostic: false,
            export_type: None,
            export_path: PathBuf::new(),
            query_context: QueryContext::default(),
            no_lazy: false,
            custom_name: None,
            platform: Platform::macOS,
        }
    }

    fn fake_app(options: Options) -> Config {
        let connection = get_connection(&options.db_path).unwrap();
        Config {
            chatrooms: HashMap::new(),
            real_chatrooms: HashMap::new(),
            chatroom_participants: HashMap::new(),
            participants: HashMap::new(),
            real_participants: HashMap::new(),
            reactions: HashMap::new(),
            options,
            offset: 0,
            db: connection,
            converter: Some(crate::app::converter::Converter::Sips),
        }
    }

    pub fn fake_attachment() -> Attachment {
        Attachment {
            rowid: 0,
            filename: Some("a/b/c/d.jpg".to_string()),
            uti: Some("public.png".to_string()),
            mime_type: Some("image/png".to_string()),
            transfer_name: Some("d.jpg".to_string()),
            total_bytes: 100,
            hide_attachment: 0,
            copied_path: None,
        }
    }

    #[test]
    fn can_get_valid_attachment_sub_dir() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create chatroom ID
        app.real_chatrooms.insert(0, 0);

        // Get subdirectory
        let sub_dir = app.conversation_attachment_path(Some(0));
        assert_eq!(String::from("0"), sub_dir)
    }

    #[test]
    fn can_get_invalid_attachment_sub_dir() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create chatroom ID
        app.real_chatrooms.insert(0, 0);

        // Get subdirectory
        let sub_dir = app.conversation_attachment_path(Some(1));
        assert_eq!(String::from("orphaned"), sub_dir)
    }

    #[test]
    fn can_get_missing_attachment_sub_dir() {
        let options = fake_options();
        let mut app = fake_app(options);

        // Create chatroom ID
        app.real_chatrooms.insert(0, 0);

        // Get subdirectory
        let sub_dir = app.conversation_attachment_path(None);
        assert_eq!(String::from("orphaned"), sub_dir)
    }

    #[test]
    fn can_get_path_not_copied() {
        let options = fake_options();
        let app = fake_app(options);

        // Create attachment
        let attachment = fake_attachment();

        let result = app.message_attachment_path(&attachment);
        let expected = String::from("a/b/c/d.jpg");
        assert_eq!(result, expected);
    }

    #[test]
    fn can_get_path_copied() {
        let mut options = fake_options();
        // Set an export path
        options.export_path = PathBuf::from("/Users/ReagentX/exports");

        let app = fake_app(options);

        // Create attachment
        let mut attachment = fake_attachment();
        let mut full_path = PathBuf::from("/Users/ReagentX/exports/attachments");
        full_path.push(attachment.filename());
        attachment.copied_path = Some(full_path);

        let result = app.message_attachment_path(&attachment);
        let expected = String::from("attachments/d.jpg");
        assert_eq!(result, expected);
    }

    #[test]
    fn can_get_path_copied_bad() {
        let mut options = fake_options();
        // Set an export path
        options.export_path = PathBuf::from("/Users/ReagentX/exports");

        let app = fake_app(options);

        // Create attachment
        let mut attachment = fake_attachment();
        attachment.copied_path = Some(PathBuf::from(attachment.filename.as_ref().unwrap()));

        let result = app.message_attachment_path(&attachment);
        let expected = String::from("a/b/c/d.jpg");
        assert_eq!(result, expected);
    }
}
