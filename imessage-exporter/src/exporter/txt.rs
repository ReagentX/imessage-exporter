use std::collections::HashMap;

use crate::{
    app::runtime::Config,
    exporter::exporter::{Exporter, Writer},
};

use imessage_database::{
    tables::table::ME,
    util::dates::format,
    Attachment, {Message, Table, Variant},
};

pub struct TXT<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config<'a>,
    /// Handles to files we want to write messages to
    pub files: HashMap<i32, String>,
}

impl<'a> Exporter<'a> for TXT<'a> {
    fn new(config: &'a Config) -> Self {
        TXT {
            config,
            files: HashMap::new(),
        }
    }

    fn iter_messages(&self) {
        let mut statement = Message::get(&self.config.db);

        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .unwrap();

        for message in messages {
            match message {
                Ok(message) => match message {
                    Ok(msg) => {
                        let message = self.format_message(&msg);
                        println!("{message:?}");
                    }
                    // TODO: When does this occur?
                    Err(why) => panic!("Inner error: {}", why),
                },
                // TODO: When does this occur?
                Err(why) => panic!("Outer error: {}", why),
            }
        }
    }

    fn get_or_create_file(&self) -> String {
        todo!()
    }
}

impl<'a> Writer for TXT<'a> {
    fn format_message(&self, msg: &imessage_database::Message) -> Option<String> {
        if msg.is_reply() || matches!(msg.variant(), Variant::Reaction(_)) {
            return None;
        }

        Some(format!(
            "Time: {:?} | Type: {:?} | Chat: {:?} {:?} | Sender: {} (deduped: {}) | {:?} |{} |{} |{}",
            format(&msg.date()),
            msg.case(),
            msg.chat_id,
            match msg.chat_id {
                Some(id) => match self.config.chatroom_participants.get(&id) {
                    Some(chatroom) => chatroom
                        .iter()
                        .map(|x| self.config.participants.get(x).unwrap())
                        .collect::<Vec<&String>>(),
                    None => {
                        // TODO: Orphaned chats!
                        println!("Found error: message chat ID {} has no members!", id);
                        Vec::new()
                    }
                },
                None => {
                    // TODO: Orphaned messages!
                    println!("Found error: message has no chat ID!");
                    Vec::new()
                }
            },
            // Get real participant info
            match msg.is_from_me {
                true => ME,
                false => self.config.participants.get(&msg.handle_id).unwrap(),
            },
            // Get unique participant info
            match msg.is_from_me {
                true => &-1,
                false => self.config.real_participants.get(&msg.handle_id).unwrap(),
            },
            msg.body(),
            match msg.num_replies {
                0 => String::new(),
                _ => {
                    let replies = msg.get_replies(&self.config.db);
                    format!(
                        " Replies: {:?}",
                        replies.iter().map(|m| format!("{}: {}", &m.guid, m.get_reply_index())).collect::<Vec<String>>()
                    )
                }
            },
            {
                let reactions = msg.get_reactions(&self.config.db, &self.config.reactions);
                match reactions.len() {
                    0 => String::new(),
                    _ => format!(" Reactions: {:?}", reactions.iter().map(|m| format!("{:?}", m.variant())).collect::<Vec<String>>())
                }
            },
            {
                let attachments = Attachment::from_message(&self.config.db, msg.rowid);
                match attachments.len() {
                    0 => String::new(),
                    _ => format!(" Attachments: {:?}", attachments.iter().map(|a| format!("{:?}", a.filename)).collect::<Vec<String>>())
                }
            }
        ))
    }

    fn format_attachment(&self, msg: &imessage_database::Message) -> String {
        todo!()
    }

    fn format_reaction(&self, msg: &imessage_database::Message) -> String {
        todo!()
    }

    fn write_to_file(&self, file: &str, text: &str) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Config, Exporter, Options};
    use imessage_database::util::dirs::default_db_path;

    fn fake_options() -> Options<'static> {
        Options {
            db_path: default_db_path(),
            no_copy: true,
            diagnostic: false,
            export_type: Some("txt"),
            export_path: None,
            valid: true,
        }
    }

    use super::TXT;
    #[test]
    fn can_create() {
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);
        assert_eq!(exporter.files.len(), 0)
    }
}
