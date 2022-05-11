use core::panic;
use std::collections::HashMap;

use crate::{
    app::runtime::Config,
    exporter::exporter::{Exporter, Writer},
};

use imessage_database::{
    tables::messages::BubbleType,
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
                        // Message replies and reactions are rendered in context, so no need to render them separately
                        if !msg.is_reply() && !matches!(msg.variant(), Variant::Reaction(..)) {
                            let message = self.format_message(&msg, 0);
                            println!("{message}");
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

impl<'a> Writer<'a> for TXT<'a> {
    fn format_message(&self, message: &Message, indent: usize) -> String {
        let indent = String::from_iter((0..indent).map(|_| " "));
        // Data we want to write to a file
        let mut formatted_message = String::new();

        // Add message date
        self.add_line(&mut formatted_message, &self.get_time(message), &indent);

        // Add message sender
        self.add_line(
            &mut formatted_message,
            self.config.who(&message.handle_id, message.is_from_me),
            &indent,
        );

        // Useful message metadata
        let message_parts = message.body();
        let attachments = Attachment::from_message(&self.config.db, message.rowid);
        let replies = message.get_replies(&self.config.db);
        let reactions = message.get_reactions(&self.config.db, &self.config.reactions);

        // Iteration context variables
        let mut attachment_index: usize = 0;
        for (idx, message_part) in message_parts.iter().enumerate() {
            let line: &str = match message_part {
                BubbleType::Text(text) => text,
                BubbleType::Attachment => match attachments.get(attachment_index) {
                    Some(attachment) => match self.format_attachment(attachment) {
                        Ok(result) => {
                            attachment_index += 1;
                            result
                        }
                        Err(result) => result,
                    },
                    // Attachment does not exist!
                    None => "Attachment missing!",
                },
                // TODO: Support app messages
                BubbleType::App => "App not yet supported!!",
            };

            self.add_line(&mut formatted_message, line, &indent);

            // Handle Reactions
            if let Some(reactions) = reactions.get(&idx) {
                reactions.iter().for_each(|reaction| {
                    self.add_line(
                        &mut formatted_message,
                        &self.format_reaction(reaction),
                        &indent,
                    );
                });
            }

            // Handle Replies
            if let Some(replies) = replies.get(&idx) {
                replies.iter().for_each(|reply| {
                    self.add_line(
                        &mut formatted_message,
                        &self.format_message(reply, 4),
                        &indent,
                    );
                });
            }
        }
        formatted_message

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

    // fn format_reply(&self, msg: &Message) -> String {
    //     format!(
    //         "    {}: {:?}",
    //         self.config.who(&msg.handle_id, msg.is_from_me),
    //         msg.body()
    //     )
    // }

    fn format_attachment(&self, attachment: &'a Attachment) -> Result<&'a str, &'a str> {
        match &attachment.filename {
            Some(filename) => Ok(filename),
            // Filepath missing!
            None => Err(&attachment.transfer_name),
        }
    }

    fn format_reaction(&self, msg: &Message) -> String {
        format!(
            "{}: {:?}",
            self.config.who(&msg.handle_id, msg.is_from_me),
            msg.variant()
        )
    }

    fn write_to_file(&self, file: &str, text: &str) {
        todo!()
    }
}

impl<'a> TXT<'a> {
    fn get_time(&self, message: &Message) -> String {
        let mut date = dates::format(&message.date());
        if let Some(time) = message.time_until_read() {
            date.push_str(&format!(" (Read after {} minutes)", time));
        }
        date
    }

    fn add_line(&self, string: &mut String, part: &str, indent: &str) {
        string.push_str(indent);
        string.push_str(part);
        string.push('\n');
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
