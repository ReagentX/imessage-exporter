use std::collections::HashMap;

use crate::{
    app::runtime::Config,
    exporter::exporter::{Exporter, Writer},
};

use imessage_database::{
    tables::{
        messages::{BubbleType, MessageType},
    },
    util::dates,
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
                        match message {
                            Some(msg) => println!("{msg}"),
                            None => {}
                        }
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
    fn format_message(&self, message: &Message) -> Option<String> {
        // Message replies and reactions are rendered in context, so no need to render them separately
        if message.is_reply() || matches!(message.variant(), Variant::Reaction(..)) {
            return None;
        }

        // Data we want to write to a file
        let mut formatted_message = String::new();

        // Add message date
        formatted_message.push_str(&dates::format(&message.date()));
        formatted_message.push('\n');

        // Add message sender
        formatted_message.push_str(self.config.who(&message.handle_id, message.is_from_me));
        formatted_message.push('\n');

        // Useful message metadata
        let message_parts = message.body();
        let attachments = Attachment::from_message(&self.config.db, message.rowid);
        let replies = message.get_replies(&self.config.db);
        let reactions = message.get_reactions(&self.config.db, &self.config.reactions);

        // Iteration context variables
        let mut attachment_index: usize = 0;
        for (idx, message_part) in message_parts.iter().enumerate() {
            match message_part {
                BubbleType::Text(text) => {
                    formatted_message.push_str(text);
                }
                BubbleType::Attachment => match attachments.get(attachment_index) {
                    Some(attachment) => match &attachment.filename {
                        Some(filename) => {
                            formatted_message.push_str(filename);
                            attachment_index += 1
                        }
                        // Filepath missing!
                        None => {
                            formatted_message.push_str("Filepath missing: ");
                            formatted_message.push_str(&attachment.transfer_name);
                        }
                    },
                    // Attachment does not exist!
                    None => {
                        formatted_message.push_str("Attachment missing!");
                    }
                },
                BubbleType::App => {
                    formatted_message.push_str("Attachment missing!");
                }
            }
            // Handle Reactions
            if let Some(reactions) = reactions.get(&idx) {
                formatted_message.push('\n');
                reactions.iter().for_each(|reaction| {
                    formatted_message.push_str(&self.format_reaction(reaction));
                    formatted_message.push('\n');
                });
            }

            // Handle Reploes
            if let Some(replies) = replies.get(&idx) {
                formatted_message.push('\n');
                replies.iter().for_each(|reply| {
                    formatted_message.push_str(&self.format_reply(reply));
                    formatted_message.push('\n');
                });
            }
            // Add newline for each next message part
            formatted_message.push('\n');
        }
        formatted_message.push('\n');
        Some(formatted_message)

        // TODO: This is sample code, remove it!
        // Some(format!(
        //     "Time: {:?} | Type: {:?} | Chat: {:?} {:?} | Sender: {} (deduped: {}) | {:?} |{} |{} |{}",
        //     dates::format(&message.date()),
        //     message.case(),
        //     message.chat_id,
        //     match message.chat_id {
        //         Some(id) => match self.config.chatroom_participants.get(&id) {
        //             Some(chatroom) => chatroom
        //                 .iter()
        //                 .map(|x| self.config.participants.get(x).unwrap())
        //                 .collect::<Vec<&String>>(),
        //             None => {
        //                 // TODO: Orphaned chats!
        //                 println!("Found error: message chat ID {} has no members!", id);
        //                 Vec::new()
        //             }
        //         },
        //         None => {
        //             // TODO: Orphaned messages!
        //             println!("Found error: message has no chat ID!");
        //             Vec::new()
        //         }
        //     },
        //     // Get real participant info
        //     match message.is_from_me {
        //         true => ME,
        //         false => self.config.participants.get(&message.handle_id).unwrap(),
        //     },
        //     // Get unique participant info
        //     match message.is_from_me {
        //         true => &-1,
        //         false => self.config.real_participants.get(&message.handle_id).unwrap(),
        //     },
        //     message.body(),
        //     match message.num_replies {
        //         0 => String::new(),
        //         _ => {
        //             let replies = message.get_replies(&self.config.db);
        //             format!(
        //                 " Replies: {:?}",
        //                 replies.iter().map(|m| format!("{}: {}", &m.guid, m.get_reply_index())).collect::<Vec<String>>()
        //             )
        //         }
        //     },
        //     {
        //         let reactions = message.get_reactions(&self.config.db, &self.config.reactions);
        //         match reactions.len() {
        //             0 => String::new(),
        //             _ => format!(" Reactions: {:?}", reactions.iter().map(|m| format!("{:?}", m.variant())).collect::<Vec<String>>())
        //         }
        //     },
        //     {
        //         let attachments = Attachment::from_message(&self.config.db, message.rowid);
        //         match attachments.len() {
        //             0 => String::new(),
        //             _ => format!(" Attachments: {:?}", attachments.iter().map(|a| format!("{:?}", a.filename)).collect::<Vec<String>>())
        //         }
        //     }
        // ))
    }

    fn format_attachment(&self, msg: &Message) -> String {
        todo!()
    }

    fn format_reaction(&self, msg: &Message) -> String {
        format!(
            "    {}: {:?}",
            self.config.who(&msg.handle_id, msg.is_from_me),
            msg.variant()
        )
    }

    fn format_reply(&self, msg: &Message) -> String {
        format!(
            "    {}: {:?}",
            self.config.who(&msg.handle_id, msg.is_from_me),
            msg.body()
        )
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
