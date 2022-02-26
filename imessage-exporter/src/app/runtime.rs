use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::app::options::Options;
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

pub struct State<'a> {
    chatrooms: HashMap<i32, Chat>, // Map of chatroom ID to chatroom information
    chatroom_participants: HashMap<i32, HashSet<i32>>, // Map of chatroom ID to chatroom participants
    participants: HashMap<i32, String>,                // Map of participant ID to contact info
    options: Options<'a>,                              // App configuration options
    db: Connection, // The connection we use to query the database
}

impl<'a> State<'a> {
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

    pub fn iter_messages(&self) {
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

    pub fn iter_threads(&self) {
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

    pub fn iter_attachments(&self) {
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
    pub fn run_diagnostic(&self) {
        println!("iMessage Database Diagnostics\n");
        Handle::run_diagnostic(&self.db);
        Message::run_diagnostic(&self.db);
        Attachment::run_diagnostic(&self.db);
        println!();
    }

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
                _ => {
                    println!("Unknown export type!")
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
