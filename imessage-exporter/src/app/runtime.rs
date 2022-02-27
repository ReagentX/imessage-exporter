use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::app::options::{Options, SUPPORTED_FILE_TYPES};
use imessage_database::{
    tables::{
        attachment::Attachment,
        chat::Chat,
        handle::Handle,
        join::ChatToHandle,
        messages::Message,
        table::{get_connection, Cacheable, Diagnostic, Table, ME},
    },
    util::dates::format,
};

/// Stores the application state and handles application lifecycle
pub struct State<'a> {
    /// Map of chatroom ID to chatroom information
    chatrooms: HashMap<i32, Chat>,
    /// Map of chatroom ID to chatroom participants
    chatroom_participants: HashMap<i32, HashSet<i32>>,
    /// Map of participant ID to contact info
    participants: HashMap<i32, String>,
    /// App configuration options
    options: Options<'a>,
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
        Some(State {
            // TODO: Implement Try for these cache calls `?`
            chatrooms: Chat::cache(&conn),
            chatroom_participants: ChatToHandle::cache(&conn),
            participants: Handle::cache(&conn),
            options,
            db: conn,
        })
    }

    fn iter_messages(&self) {
        let mut statement = Message::get(&self.db);
        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .unwrap();
        for message in messages {
            let msg = message.unwrap().unwrap();
            println!(
                "{:?} | {} {:?}",
                format(&msg.date()),
                match msg.is_from_me {
                    true => ME,
                    false => self.participants.get(&msg.handle_id).unwrap(),
                },
                match msg.attachment_id {
                    Some(id) => Some(format!(
                        "{:?}{:?}",
                        msg.text,
                        Attachment::path_from_message(id, &self.db)
                    )),
                    None => msg.text,
                }
            );
        }
    }

    fn iter_threads(&self) {
        for thread in &self.chatroom_participants {
            let (chat, participants) = thread;
            println!(
                "{}: {}",
                self.chatrooms.get(chat).unwrap().chat_identifier,
                participants
                    .iter()
                    .map(|f| self.participants.get(f).unwrap().to_owned())
                    .collect::<Vec<String>>()
                    .join(", ")
            )
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
        println!();
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
            self.iter_messages();
            // self.iter_attachments();
        }
    }
}
