use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::{
    tables::{
        attachment::Attachment,
        chat::Chat,
        handle::Handle,
        join::ChatToHandle,
        messages::Message,
        table::{get_connection, Cacheable, Table, ME},
    },
    util::dates::format,
};

pub struct State {
    chatrooms: HashMap<i32, Chat>, // Map of chatroom ID to chatroom information
    chatroom_participants: HashMap<i32, HashSet<i32>>, // Map of chatroom ID to chatroom participants
    participants: HashMap<i32, String>,                // Map of participant ID to contact info
    no_copy: bool,  // If true, do not copy files from the Libary to the Archive
    db: Connection, // The connection we use to query the database
}

impl State {
    pub fn new(db_path: String, no_copy: bool) -> Option<State> {
        let conn = get_connection(&db_path);
        Some(State {
            // TODO: Implement Try for these cache calls `?`
            chatrooms: Chat::cache(&conn),
            chatroom_participants: ChatToHandle::cache(&conn),
            participants: Handle::cache(&conn),
            no_copy,
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
                msg.text
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

    // TODO: Finish implementation
    /// Handles diagnostic tests for database
    pub fn run_diagnostic(&self) {
        println!("");
        println!(
            "Contacts with more than one ID: {}",
            Handle::run_diagnostic(&self.db).unwrap()
        );
        Attachment::run_diagnostic(&self.db);
        println!("");
    }

    pub fn start(&self) {
        todo!()
    }
}
