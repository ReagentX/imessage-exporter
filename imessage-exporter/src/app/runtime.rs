use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::create_dir_all,
    path::PathBuf,
};

use rusqlite::Connection;

use crate::{app::options::Options, Exporter, TXT};
use imessage_database::{
    tables::table::{get_connection, Cacheable, Deduplicate, Diagnostic, ME, UNKNOWN},
    util::{dates::get_offset, dirs::home},
    Attachment, Chat, ChatToHandle, Handle, Message,
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
    pub reactions: HashMap<String, Vec<String>>,
    /// App configuration options
    pub options: Options<'a>,
    /// Global date offset used by the iMessage database:
    pub offset: i64,
    /// The connection we use to query the database
    pub db: Connection,
}

impl<'a> Config<'a> {
    /// Create a new instance of the application
    ///
    /// # Example:
    ///
    /// ```
    /// use crate::app::{
    ///    options::{from_command_line, Options},
    ///    runtime::State,
    /// };
    ///
    /// let args = from_command_line();
    /// let options = Options::from_args(&args);
    /// let app = State::new(options).unwrap();
    /// ```
    pub fn new(options: Options) -> Option<Config> {
        // Escape early if options are invalid
        if !options.valid {
            return None;
        }

        let conn = get_connection(&options.db_path);
        // TODO: Implement Try for these cache calls `?`
        println!("Caching chats...");
        let chatrooms = Chat::cache(&conn);
        println!("Caching chatrooms...");
        let chatroom_participants = ChatToHandle::cache(&conn);
        println!("Caching participants...");
        let participants = Handle::cache(&conn);
        println!("Caching reactions...");
        let reactions = Message::cache(&conn);
        Some(Config {
            chatrooms,
            real_chatrooms: ChatToHandle::dedupe(&chatroom_participants),
            chatroom_participants,
            real_participants: Handle::dedupe(&participants),
            participants,
            reactions,
            options,
            offset: get_offset(),
            db: conn,
        })
    }

    /// Determine who sent a message
    pub fn who(&self, handle_id: &i32, is_from_me: bool) -> &str {
        if is_from_me {
            ME
        } else {
            match self.participants.get(handle_id) {
                Some(contact) => contact,
                None => UNKNOWN,
            }
        }
    }

    /// Get a deduplicated chat ID or a default value
    pub fn conversation(&self, chat_id: Option<i32>) -> Option<(&Chat, &i32)> {
        match chat_id {
            Some(chat_id) => match self.chatrooms.get(&chat_id) {
                Some(chatroom) => self.real_chatrooms.get(&chat_id).map(|id| (chatroom, id)),
                // No chatroom for the given chat_id
                None => {
                    println!("Chat ID {chat_id} does not exist in chat table!");
                    None
                }
            },
            // No chat_id provided
            None => None,
        }
    }

    /// Get a filename for a chat, possibly using cached data.
    ///
    /// If the chat has an assigned name, use that.
    ///
    /// If it does not, first try and make a flat list of its members. Failing that, use the unique `chat_identifier` field.
    pub fn filename(&self, chatroom: &Chat) -> String {
        match &chatroom.display_name() {
            // If there is a display name, use that
            Some(name) => name.to_string(),
            // Fallback if there is no name set
            None => match self.chatroom_participants.get(&chatroom.rowid) {
                // List of participant names
                Some(participants) => participants
                    .iter()
                    .map(|participant_id| self.who(participant_id, false))
                    .collect::<Vec<&str>>()
                    .join(", "),
                // Unique chat_identifier
                None => {
                    println!(
                        "Found error: message chat ID {} has no members!",
                        chatroom.rowid
                    );
                    chatroom.chat_identifier.to_owned()
                }
            },
        }
    }

    /// Get the export path for the current session
    pub fn export_path(&self) -> PathBuf {
        match self.options.export_path {
            Some(path_str) => PathBuf::from(path_str),
            None => PathBuf::from(&format!("{}/imessage_export", home())),
        }
    }

    /// Handles diagnostic tests for database
    fn run_diagnostic(&self) {
        println!("iMessage Database Diagnostics\n");
        Handle::run_diagnostic(&self.db);
        Message::run_diagnostic(&self.db);
        Attachment::run_diagnostic(&self.db);

        // Global Diagnostics
        let unique_handles: HashSet<i32> =
            HashSet::from_iter(self.real_participants.values().cloned());
        let duplicated_handles = self.participants.len() - unique_handles.len();
        if duplicated_handles > 1 {
            println!("Duplicated contacts: {duplicated_handles}");
        }

        let unique_chats: HashSet<i32> = HashSet::from_iter(self.real_chatrooms.values().cloned());
        let duplicated_chats = self.chatrooms.len() - unique_chats.len();
        if duplicated_chats > 1 {
            println!("Duplicated chats: {duplicated_chats}");
        }
    }

    /// Start the app given the provided set of options. This will either run
    /// diagnostic tests on the database or export data to the specified file type.
    ///
    // # Example:
    ///
    /// ```
    /// use crate::app::{
    ///    options::{from_command_line, Options},
    ///    runtime::State,
    /// };
    ///
    /// let args = from_command_line();
    /// let options = Options::from_args(&args);
    /// let app = State::new(options).unwrap();
    /// app.start();
    /// ```
    pub fn start(&self) {
        if self.options.diagnostic {
            self.run_diagnostic();
        } else if self.options.export_type.is_some() {
            // Ensure the path we want to export to exists
            create_dir_all(self.export_path()).unwrap();

            match self.options.export_type.unwrap() {
                "txt" => {
                    // Create exporter, pass it data we care about, then kick it off
                    TXT::new(self).iter_messages();
                }
                "csv" => {
                    todo!()
                }
                "pdf" => {
                    todo!()
                }
                "html" => {
                    todo!()
                }
                _ => {
                    unreachable!()
                }
            }
        } else {
            println!("How did you get here?");
        }
        println!("Done!");
    }
}
