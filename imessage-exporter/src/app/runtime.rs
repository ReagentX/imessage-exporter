use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Display,
};

use rusqlite::Connection;

use crate::app::options::{Options, SUPPORTED_FILE_TYPES};
use imessage_database::{
    message::variants::get_types_table,
    tables::{
        attachment::Attachment,
        chat::Chat,
        handle::Handle,
        join::ChatToHandle,
        messages::Message,
        table::{get_connection, Cacheable, Deduplicate, Diagnostic, Table, ME},
    },
    util::dates::format,
};

/// Stores the application state and handles application lifecycle
pub struct State<'a> {
    /// Map of chatroom ID to chatroom information
    chatrooms: HashMap<i32, Chat>,
    // Map of chatroom ID to an internal unique chatroom ID
    real_chatrooms: HashMap<i32, i32>,
    /// Map of chatroom ID to chatroom participants
    chatroom_participants: HashMap<i32, BTreeSet<i32>>,
    /// Map of participant ID to contact info
    participants: HashMap<i32, String>,
    // Map of participant ID to an internal unique participant ID
    real_participants: HashMap<i32, i32>,
    /// App configuration options
    options: Options<'a>,
    /// Types of messages we may encounter
    message_types: HashMap<i32, Box<dyn Display + 'static>>,
    /// The connection we use to query the database
    db: Connection,
}

impl<'a> State<'a> {
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
    pub fn new(options: Options) -> Option<State> {
        let conn = get_connection(&options.db_path);
        // TODO: Implement Try for these cache calls `?`
        let chatrooms = Chat::cache(&conn);
        let chatroom_participants = ChatToHandle::cache(&conn);
        let participants = Handle::cache(&conn);
        Some(State {
            chatrooms,
            real_chatrooms: Chat::dedupe(&chatroom_participants),
            chatroom_participants,
            real_participants: Handle::dedupe(&participants),
            participants,
            options,
            message_types: get_types_table(),
            db: conn,
        })
    }

    fn iter_messages(&self) {
        let unk: Vec<&String> = vec![];
        let mut statement = Message::get(&self.db);
        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .unwrap();
        for message in messages {
            let msg = message.unwrap().unwrap();
            // Skip messages that are replies, because we would have already rendered them
            if msg.is_reply() {
                continue;
            }
            // Emit message info
            println!(
                "Time: {:?} | Chat: {:?} {:?} | Sender: {} (deduped: {}) | {:?} |{}",
                format(&msg.date()),
                msg.chat_id,
                match msg.chat_id {
                    Some(id) => match self.chatroom_participants.get(&id) {
                        Some(chatroom) => chatroom
                            .iter()
                            .map(|x| self.participants.get(x).unwrap())
                            .collect::<Vec<&String>>(),
                        None => {
                            println!("Found error: message chat ID {} has no members!", id);
                            Vec::new()
                        }
                    },
                    None => {
                        println!("Found error: message has no chat ID!");
                        Vec::new()
                    }
                },
                // Get real participant info
                match msg.is_from_me {
                    true => ME,
                    false => self.participants.get(&msg.handle_id).unwrap(),
                },
                // Get unique participant info
                match msg.is_from_me {
                    true => &-1,
                    false => self.real_participants.get(&msg.handle_id).unwrap(),
                },
                match msg.num_attachments {
                    0 => msg.text.as_ref().unwrap_or(&String::new()).to_owned(),
                    _ => msg.num_attachments.to_string(),
                },
                match msg.num_replies {
                    0 => String::new(),
                    _ => {
                        let replies = msg.get_replies(&self.db);
                        format!(
                            "Replies: {:?}",
                            replies.iter().map(|m| &m.guid).collect::<Vec<&String>>()
                        )
                    }
                }
            );
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
        // 62, 122
        for handle in &self.real_participants {
            let (handle_id, handle_name) = handle;
            println!("{}: {}", handle_id, handle_name,)
        }
    }

    fn iter_attachments(&self) {
        let mut statement = Attachment::get(&self.db);
        let attachments = statement
            .query_map([], |row| Ok(Attachment::from_row(row)))
            .unwrap();

        for attachment in attachments {
            // println!("Attachment: {attachment:?}");
            let file = attachment.unwrap().unwrap();
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
        if !self.options.valid {
            //
        } else if self.options.diagnostic {
            self.run_diagnostic();
        } else if self.options.export_type.is_some() {
            match self.options.export_type.unwrap() {
                "txt" => {
                    println!("txt")
                }
                "csv" => {
                    println!("csv")
                }
                "pdf" => {
                    println!("pdf")
                }
                "html" => {
                    println!("html")
                }
                other => {
                    println!("{other} is not a valid export type! Must be one of <{SUPPORTED_FILE_TYPES}>")
                }
            }
        } else {
            // Run some app methods
            // self.iter_threads();
            // self.iter_handles();
            self.iter_messages();
            // self.iter_attachments();
        }
    }
}
