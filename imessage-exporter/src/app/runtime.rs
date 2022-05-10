use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::Connection;

use crate::{
    app::options::{Options, SUPPORTED_FILE_TYPES},
    Exporter, TXT,
};
use imessage_database::{
    tables::table::{get_connection, Cacheable, Deduplicate, Diagnostic, Table, ME},
    util::dates::format,
    Attachment, Chat, ChatToHandle, Handle, Message, Variant,
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
    reactions: HashMap<String, Vec<String>>,
    /// App configuration options
    options: Options<'a>,
    /// The connection we use to query the database
    db: Connection,
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

    fn iter_messages(&self) {
        let unk: Vec<&String> = vec![];
        let mut statement = Message::get(&self.db);
        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .unwrap();
        for message in messages {
            let msg = message.unwrap().unwrap();
            if msg.is_reply() || matches!(msg.variant(), Variant::Reaction(_)) {
                continue;
            }
            // Emit message info
            println!(
                "Time: {:?} | Type: {:?} | Chat: {:?} {:?} | Sender: {} (deduped: {}) | {:?} |{} |{} |{}",
                format(&msg.date()),
                msg.case(),
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
                msg.body(),
                match msg.num_replies {
                    0 => String::new(),
                    _ => {
                        let replies = msg.get_replies(&self.db);
                        format!(
                            " Replies: {:?}",
                            replies.iter().map(|m| format!("{}: {}", &m.guid, m.get_reply_index())).collect::<Vec<String>>()
                        )
                    }
                },
                {
                    let reactions = msg.get_reactions(&self.db, &self.reactions);
                    match reactions.len() {
                        0 => String::new(),
                        _ => format!(" Reactions: {:?}", reactions.iter().map(|m| format!("{:?}", m.variant())).collect::<Vec<String>>())
                    }
                },
                {
                    let attachments = Attachment::from_message(&self.db, msg.rowid);
                    match attachments.len() {
                        0 => String::new(),
                        _ => format!(" Attachments: {:?}", attachments.iter().map(|a| format!("{:?}", a.filename)).collect::<Vec<String>>())
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
            panic!("Invalid options!")
        } else if self.options.diagnostic {
            self.run_diagnostic();
        } else if self.options.export_type.is_some() {
            match self.options.export_type.unwrap() {
                "txt" => {
                    println!("txt")
                    // Create exporter, pass it data we care about, then kick it off
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
            // self.iter_reactions();
            self.iter_messages();
            // self.iter_attachments();
            println!("Done!");
        }
    }
}
