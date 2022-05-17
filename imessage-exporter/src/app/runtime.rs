use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::Connection;

use crate::{
    app::options::{Options, SUPPORTED_FILE_TYPES},
    Exporter, TXT,
};
use imessage_database::{
    tables::table::{get_connection, Cacheable, Deduplicate, Diagnostic, Table, ME, UNKNOWN},
    Attachment, Chat, ChatToHandle, Handle, Message,
};

/// Stores the application state and handles application lifecycle
pub struct Config<'a> {
    /// Map of chatroom ID to chatroom information
    chatrooms: HashMap<i32, Chat>,
    // Map of chatroom ID to an internal unique chatroom ID
    real_chatrooms: HashMap<i32, i32>,
    /// Map of chatroom ID to chatroom participants
    chatroom_participants: HashMap<i32, BTreeSet<i32>>,
    /// Map of participant ID to contact info
    participants: HashMap<i32, String>,
    /// Map of participant ID to an internal unique participant ID
    real_participants: HashMap<i32, i32>,
    /// Messages that are reactions to other messages
    pub reactions: HashMap<String, Vec<String>>,
    /// App configuration options
    pub options: Options<'a>,
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
            real_chatrooms: Chat::dedupe(&chatroom_participants),
            chatroom_participants,
            real_participants: Handle::dedupe(&participants),
            participants,
            reactions,
            options,
            db: conn,
        })
    }

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

    fn iter_threads(&self) {
        for thread in &self.chatroom_participants {
            let (chat_id, participants) = thread;
            let chatroom = self.chatrooms.get(chat_id).unwrap();
            println!(
                "{} ({}: {}): {}",
                chatroom.name(),
                chat_id,
                self.real_chatrooms.get(chat_id).unwrap(),
                participants
                    .iter()
                    .map(|participant_id| format!(
                        "{}",
                        self.real_participants.get(participant_id).unwrap()
                    ))
                    .collect::<Vec<String>>()
                    .join(", ")
            )
        }
    }

    fn iter_handles(&self) {
        for handle in &self.real_participants {
            let (handle_id, handle_name) = handle;
            println!("{}: {}", handle_id, handle_name,)
        }
    }

    fn iter_reactions(&self) {
        for reaction in &self.reactions {
            let (message, reactions) = reaction;
            if reactions.len() > 1 {
                println!("{}: {:?}", message, reactions)
            }
        }
    }

    fn iter_attachments(&self) {
        let mut statement = Attachment::get(&self.db);
        let attachments = statement
            .query_map([], |row| Ok(Attachment::from_row(row)))
            .unwrap();

        for attachment in attachments {
            // println!("Attachment: {attachment:?}");
            let file = Attachment::extract(attachment);
            println!("{:?}", file.filename);
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
                other => {
                    panic!("{other} is not a valid export type! Must be one of <{SUPPORTED_FILE_TYPES}>")
                }
            }
        } else {
            // Run some app methods
            // self.iter_threads();
            // self.iter_handles();
            // self.iter_reactions();
            // self.iter_attachments();
            println!("Done!");
        }
    }
}
