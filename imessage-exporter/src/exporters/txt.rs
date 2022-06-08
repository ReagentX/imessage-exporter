use std::collections::HashMap;

use crate::{
    app::runtime::Config,
    exporters::exporter::{Exporter, Writer},
};

use imessage_database::{
    tables::{messages::BubbleType, table::ORPHANED},
    util::dates,
    Attachment, {BubbleEffect, Expressive, Message, ScreenEffect, Table},
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

    fn iter_messages(&mut self) {
        let mut statement = Message::get(&self.config.db);

        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .unwrap();

        for message in messages {
            let msg = Message::extract(message);
            // Message replies and reactions are rendered in context, so no need to render them separately
            if !msg.is_reaction() {
                let message = self.format_message(&msg, 0);
                TXT::write_to_file(self.get_or_create_file(&msg), &message);
            }
        }
    }

    fn get_or_create_file(&mut self, message: &Message) -> &str {
        match self.config.conversation(message.chat_id) {
            Some((chatroom, id)) => self
                .files
                .entry(*id)
                .or_insert_with(|| self.config.filename(chatroom)),
            None => ORPHANED,
        }
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
        let attachments = Attachment::from_message(&self.config.db, message);
        let replies = message.get_replies(&self.config.db);
        let reactions = message.get_reactions(&self.config.db, &self.config.reactions);

        // Index of where we are in the attachment Vector
        let mut attachment_index: usize = 0;

        // Generate the message body from it's components
        for (idx, message_part) in message_parts.iter().enumerate() {
            let line: &str = match message_part {
                BubbleType::Text(text) => *text,
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
                BubbleType::App => self.format_app(message),
            };

            // Write the message
            self.add_line(&mut formatted_message, line, &indent);

            // Handle expressives
            if message.expressive_send_style_id.is_some() {
                self.add_line(
                    &mut formatted_message,
                    self.format_expressive(message),
                    &indent,
                );
            }

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
                    if !reply.is_reaction() {
                        self.add_line(
                            &mut formatted_message,
                            &self.format_message(reply, 4),
                            &indent,
                        );
                    }
                });
            }
        }

        // Add a note if the message is a reply
        if message.is_reply() && indent.is_empty() {
            self.add_line(
                &mut formatted_message,
                "This message responded to an earlier message.",
                &indent,
            );
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

    fn format_attachment(&self, attachment: &'a Attachment) -> Result<&'a str, &'a str> {
        match &attachment.filename {
            Some(filename) => Ok(filename),
            // Filepath missing!
            None => Err(&attachment.transfer_name),
        }
    }

    fn format_app(&self, msg: &'a Message) -> &'a str {
        // TODO: Implement app messages
        "App messages not yet implemented!"
    }

    fn format_reaction(&self, msg: &Message) -> String {
        format!(
            "{}: {:?}",
            self.config.who(&msg.handle_id, msg.is_from_me),
            msg.variant()
        )
    }

    fn format_expressive(&self, msg: &'a Message) -> &'a str {
        match msg.get_expressive() {
            Expressive::Screen(effect) => match effect {
                ScreenEffect::Confetti => "Sent with Confetti",
                ScreenEffect::Echo => "Sent with Echo",
                ScreenEffect::Fireworks => "Sent with Fireworks",
                ScreenEffect::Balloons => "Sent with Balloons",
                ScreenEffect::Heart => "Sent with Heart",
                ScreenEffect::Lasers => "Sent with Lasers",
                ScreenEffect::ShootingStar => "Sent with Shooting Start",
                ScreenEffect::Sparkles => "Sent with Sparkles",
                ScreenEffect::Spotlight => "Sent with Spotlight",
            },
            Expressive::Bubble(effect) => match effect {
                BubbleEffect::Slam => "Sent with Slam",
                BubbleEffect::Loud => "Sent with Loud",
                BubbleEffect::Gentle => "Sent with Gentle",
                BubbleEffect::InvisibleInk => "Sent with Invisible Ink",
            },
            Expressive::Unknown(effect) => effect,
            Expressive::Normal => "Sent with ",
        }
    }

    fn write_to_file(file: &str, text: &str) {
        println!("{file}");
        println!("{text}");
    }
}

impl<'a> TXT<'a> {
    fn get_time(&self, message: &Message) -> String {
        let mut date = dates::format(&message.date(&self.config.offset));
        let read_after = message.time_until_read(&self.config.offset);
        if let Some(time) = read_after {
            if !time.is_empty() {
                let who = match message.is_from_me {
                    true => "them",
                    false => "you",
                };
                date.push_str(&format!(" (Read by {who} after {time})"));
            }
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
    use crate::{Config, Exporter, Options, TXT};
    use imessage_database::{tables::messages::Message, util::dirs::default_db_path};

    fn blank() -> Message {
        Message {
            rowid: i32::default(),
            guid: String::default(),
            text: None,
            handle_id: i32::default(),
            subject: None,
            date: i64::default(),
            date_read: i64::default(),
            date_delivered: i64::default(),
            is_from_me: false,
            is_read: false,
            associated_message_guid: None,
            associated_message_type: i32::default(),
            expressive_send_style_id: None,
            thread_originator_guid: None,
            thread_originator_part: None,
            chat_id: None,
            num_attachments: 0,
            num_replies: 0,
        }
    }

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

    #[test]
    fn can_create() {
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);
        assert_eq!(exporter.files.len(), 0);
    }

    #[test]
    fn can_get_time_valid() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        // Create fake message
        let mut message = blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        // May 17, 2022  8:29:42 PM
        message.date_delivered = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_read = 674530231992568192;

        assert_eq!(
            exporter.get_time(&message),
            "May 17, 2022  8:29:42 PM (Read by you after 1 hour, 49 seconds)"
        );
    }

    #[test]
    fn can_get_time_invalid() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        // Create fake message
        let mut message = blank();
        // May 17, 2022  9:30:31 PM
        message.date = 674530231992568192;
        // May 17, 2022  9:30:31 PM
        message.date_delivered = 674530231992568192;
        // Wed May 18 2022 02:36:24 GMT+0000
        message.date_read = 674526582885055488;
        assert_eq!(exporter.get_time(&message), "May 17, 2022  9:30:31 PM");
    }

    #[test]
    fn can_add_line_no_indent() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        // Create sample data
        let mut s = String::new();
        exporter.add_line(&mut s, "hello world", "");

        assert_eq!(s, "hello world\n".to_string());
    }

    #[test]
    fn can_add_line_indent() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        // Create sample data
        let mut s = String::new();
        exporter.add_line(&mut s, "hello world", "  ");

        assert_eq!(s, "  hello world\n".to_string());
    }
}
