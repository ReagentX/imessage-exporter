use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::tables::{
    chat::Chat,
    handle::Handle,
    join::ChatToHandle,
    messages::Message,
    table::{get_connection, Cacheable, Table, ME},
};

pub struct State {
    chats: HashMap<i32, Chat>,
    chat_participants: HashMap<i32, HashSet<i32>>,
    contacts: HashMap<i32, String>,
    no_copy: bool,
    db: Connection,
}

impl State {
    pub fn new(db_path: String, no_copy: bool) -> Option<State> {
        let conn = get_connection(&db_path);
        Some(State {
            // TODO: Implement Try for these `?`
            chats: Chat::cache(&conn),
            chat_participants: ChatToHandle::cache(&conn),
            contacts: Handle::cache(&conn),
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
                &msg.date(),
                match msg.is_from_me {
                    true => ME,
                    false => self.contacts.get(&msg.handle_id).unwrap(),
                },
                msg.text
            );
        }
    }

    pub fn iter_threads(&self) {
        for thread in &self.chat_participants {
            let (chat, participants) = thread;
            println!(
                "{}: {}",
                self.chats.get(chat).unwrap().chat_identifier,
                participants
                    .iter()
                    .map(|f| self.contacts.get(f).unwrap().to_owned())
                    .collect::<Vec<String>>()
                    .join(", ")
            )
        }
    }
}
