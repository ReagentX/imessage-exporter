use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::create_dir_all,
    path::PathBuf,
};

use rusqlite::Connection;

use crate::{app::options::Options, Exporter, HTML, TXT};
use imessage_database::{
    tables::table::{
        get_connection, Cacheable, Deduplicate, Diagnostic, DEFAULT_OUTPUT_DIR, MAX_LENGTH, ME,
        UNKNOWN,
    },
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

    /// Get the export path for the current session
    pub fn export_path(&self) -> PathBuf {
        match self.options.export_path {
            Some(path_str) => PathBuf::from(path_str),
            None => PathBuf::from(&format!("{}/{DEFAULT_OUTPUT_DIR}", home())),
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
                Some(participants) => self.filename_from_participants(participants),
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

    /// Generate a filename from a set of participants, truncating if the name is too long
    fn filename_from_participants(&self, participants: &BTreeSet<i32>) -> String {
        let mut added = 0;
        let mut out_s = String::with_capacity(MAX_LENGTH as usize);
        for participant_id in participants.iter() {
            let participant = self.who(participant_id, false);
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
                } else {
                    if out_s.is_empty() {
                        out_s.push_str(&participant[..MAX_LENGTH])
                    } else {
                        out_s.push_str(&extra);
                    }
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
        eprintln!("Building cache...");
        eprintln!("[1/4] Caching chats...");
        let chatrooms = Chat::cache(&conn);
        eprintln!("[2/4] Caching chatrooms...");
        let chatroom_participants = ChatToHandle::cache(&conn);
        eprintln!("[3/4] Caching participants...");
        let participants = Handle::cache(&conn);
        eprintln!("[4/4] Caching reactions...");
        let reactions = Message::cache(&conn);
        eprintln!("Cache built!");
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

    /// Handles diagnostic tests for database
    fn run_diagnostic(&self) {
        println!("\niMessage Database Diagnostics\n");
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
                    HTML::new(self).iter_messages();
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
}

#[cfg(test)]
mod tests {
    use crate::{Config, Options};
    use imessage_database::{
        tables::table::{get_connection, MAX_LENGTH},
        util::dirs::default_db_path,
    };
    use std::collections::{BTreeSet, HashMap};

    fn fake_options<'a>() -> Options<'a> {
        Options {
            db_path: default_db_path(),
            no_copy: false,
            diagnostic: false,
            export_type: None,
            export_path: None,
            valid: true,
        }
    }
    fn dummy<'a>(options: Options<'a>) -> Config<'a> {
        let connection = get_connection(&options.db_path);
        Config {
            chatrooms: HashMap::new(),
            real_chatrooms: HashMap::new(),
            chatroom_participants: HashMap::new(),
            participants: HashMap::new(),
            real_participants: HashMap::new(),
            reactions: HashMap::new(),
            options: options,
            offset: 0,
            db: connection,
        }
    }

    #[test]
    fn can_create() {
        let options = fake_options();
        let app = dummy(options);
        app.start();
    }

    #[test]
    fn can_get_filename_good() {
        let options = fake_options();
        let mut app = dummy(options);

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
        let mut app = dummy(options);

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
        let mut app = dummy(options);

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
}
