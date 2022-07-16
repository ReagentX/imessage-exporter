use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    app::{progress::build_progress_bar_export, runtime::Config},
    exporters::exporter::{Exporter, Writer},
};

use imessage_database::{
    tables::{
        messages::BubbleType,
        table::{ME, ORPHANED, UNKNOWN},
    },
    util::{dates, dirs::home},
    Attachment, {BubbleEffect, Expressive, Message, ScreenEffect, Table},
};

const HEADER: &str = "<html>\n<meta charset=\"UTF-8\">";
const FOOTER: &str = "</html>";
const STYLE: &str = include_str!("resources/style.css");

pub struct HTML<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config<'a>,
    /// Handles to files we want to write messages to
    /// Map of internal unique chatroom ID to a filename
    pub files: HashMap<i32, PathBuf>,
    /// Path to file for orphaned messages
    pub orphaned: PathBuf,
}

impl<'a> Exporter<'a> for HTML<'a> {
    fn new(config: &'a Config) -> Self {
        let mut orphaned = config.export_path();
        orphaned.push(ORPHANED);
        orphaned.set_extension("html");
        HTML {
            config,
            files: HashMap::new(),
            orphaned,
        }
    }

    fn iter_messages(&mut self) {
        // Tell the user what we are doing
        eprintln!(
            "Exporting to {} as html...",
            self.config.export_path().display()
        );

        // Write orphaned file headers
        HTML::write_headers(&self.orphaned);

        // Set up progress bar
        let mut current_message = 0;
        let total_messages = Message::get_count(&self.config.db);
        let pb = build_progress_bar_export(total_messages);

        let mut statement = Message::get(&self.config.db);

        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .unwrap();

        for message in messages {
            let msg = Message::extract(message);
            if msg.is_annoucement() {
                let annoucement = self.format_annoucement(&msg);
                HTML::write_to_file(self.get_or_create_file(&msg), &annoucement);
            }
            // Message replies and reactions are rendered in context, so no need to render them separately
            else if !msg.is_reaction() {
                let message = self.format_message(&msg, 0);
                HTML::write_to_file(self.get_or_create_file(&msg), &message);
            }
            current_message += 1;
            pb.set_position(current_message);
        }
        pb.finish_at_current_pos();

        eprintln!("Writing HTML footers...");
        self.files
            .iter()
            .for_each(|(_, path)| HTML::write_to_file(path, FOOTER));
        HTML::write_to_file(&self.orphaned, FOOTER);
    }

    /// Create a file for the given chat, caching it so we don't need to build it later
    fn get_or_create_file(&mut self, message: &Message) -> &Path {
        match self.config.conversation(message.chat_id) {
            Some((chatroom, id)) => self.files.entry(*id).or_insert_with(|| {
                let mut path = self.config.export_path();
                path.push(self.config.filename(chatroom));
                path.set_extension("html");

                // If the file already exists , don't write the headers again
                // This can happen if a group chat was deduplicated but resolves to the same name
                if !path.exists() {
                    // Write headers if the file does not exist
                    HTML::write_headers(&path);
                }

                path
            }),
            None => &self.orphaned,
        }
    }
}

impl<'a> Writer<'a> for HTML<'a> {
    fn format_message(&self, message: &Message, indent: usize) -> String {
        // Data we want to write to a file
        let mut formatted_message = String::new();

        // Message div
        self.add_line(&mut formatted_message, "<div class=\"message\">", "", "");

        // Start message div
        if message.is_from_me {
            self.add_line(
                &mut formatted_message,
                &format!("<div class=\"sent {:?}\">", message.service()),
                "",
                "",
            );
        } else {
            self.add_line(&mut formatted_message, "<div class=\"received\">", "", "");
        }

        // Add message date
        self.add_line(
            &mut formatted_message,
            &self.get_time(message),
            "<p><span class=\"timestamp\">",
            "</span>",
        );

        // Add message sender
        self.add_line(
            &mut formatted_message,
            self.config.who(&message.handle_id, message.is_from_me),
            "<span class=\"sender\">",
            "</span></p>",
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
            let line: String = match message_part {
                BubbleType::Text(text) => format!("<span class=\"bubble\">{text}</span>"),
                BubbleType::Attachment => match attachments.get(attachment_index) {
                    Some(attachment) => match self.format_attachment(attachment) {
                        Ok(result) => {
                            attachment_index += 1;
                            format!(
                                "<img src=\"{}\" loading=\"lazy\">",
                                result.replace("~", &home())
                            )
                        }
                        Err(result) => format!("<span class=\"attachment_error\">Missing attachment filepath: {result}</span>"),
                    },
                    // Attachment does not exist in attachments table
                    None => "Attachment does not exist!".to_owned(),
                },
                // TODO: Support app messages
                BubbleType::App => format!(
                    "<span class=\"attachment_error\">{}</span>",
                    self.format_app(message)
                ),
            };

            // Write the message
            self.add_line(
                &mut formatted_message,
                &line,
                "<hr><div class=\"message_part\">",
                "</div>",
            );

            // Handle expressives
            if message.expressive_send_style_id.is_some() {
                self.add_line(
                    &mut formatted_message,
                    self.format_expressive(message),
                    "<span class=\"expressive\">",
                    "</span>",
                );
            }

            // Handle Reactions
            if let Some(reactions) = reactions.get(&idx) {
                self.add_line(
                    &mut formatted_message,
                    "<hr><p>Reactions:</p>",
                    "<div class=\"reactions\">",
                    "",
                );
                reactions.iter().for_each(|reaction| {
                    self.add_line(
                        &mut formatted_message,
                        &self.format_reaction(reaction),
                        "<p><span class=\"reaction\">",
                        "</span></p>",
                    );
                });
                self.add_line(&mut formatted_message, "</div>", "", "")
            }

            // Handle Replies
            if let Some(replies) = replies.get(&idx) {
                self.add_line(&mut formatted_message, "<div class=\"replies\">", "", "");
                replies.iter().for_each(|reply| {
                    if !reply.is_reaction() {
                        // Set indent to 1 so we know this is a recursive call
                        self.add_line(
                            &mut formatted_message,
                            &self.format_message(reply, 1),
                            "<div class=\"reply\">",
                            "</div>",
                        );
                    }
                });
                self.add_line(&mut formatted_message, "</div>", "", "")
            }
        }

        // Add a note if the message is a reply and not rendered in a thread
        if message.is_reply() && indent == 0 {
            self.add_line(
                &mut formatted_message,
                "This message responded to an earlier message.",
                "<span class=\"reply_context\">",
                "</span>",
            );
        }

        // End message type div
        self.add_line(&mut formatted_message, "</div>", "", "");

        // End message div
        self.add_line(&mut formatted_message, "</div>", "", "");

        formatted_message
    }

    fn format_attachment(&self, attachment: &'a Attachment) -> Result<&'a str, &'a str> {
        match &attachment.filename {
            Some(filename) => Ok(filename),
            // Filepath missing!
            None => Err(&attachment.transfer_name),
        }
    }

    fn format_app(&self, _: &'a Message) -> &'a str {
        // TODO: Implement app messages
        // TODO: Support Apple Pay variants
        "App messages not yet implemented!"
    }

    fn format_reaction(&self, msg: &Message) -> String {
        match msg.variant() {
            imessage_database::Variant::Reaction(_, added, reaction) => {
                if !added {
                    return "".to_string();
                }
                format!(
                    "<b>{:?}</b> by {}",
                    reaction,
                    self.config.who(&msg.handle_id, msg.is_from_me),
                )
            }
            imessage_database::Variant::Sticker(_) => {
                let paths = Attachment::from_message(&self.config.db, msg);
                format!(
                    "Sticker from {}: <img src=\"{}\" loading=\"lazy\">",
                    self.config.who(&msg.handle_id, msg.is_from_me),
                    match paths.get(0) {
                        Some(sticker) => &sticker.filename.as_ref().unwrap(),
                        None => "Sticker not found!",
                    },
                )
            }
            _ => unreachable!(),
        }
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
            Expressive::Normal => "",
        }
    }

    fn format_annoucement(&self, msg: &'a Message) -> String {
        let mut who = self.config.who(&msg.handle_id, msg.is_from_me);
        // Rename yourself so we render the proper grammar here
        if who == ME {
            who = "You"
        }
        let timestamp = dates::format(&msg.date(&self.config.offset));
        format!(
            "\n<div class =\"announcement\"><p><span class=\"timestamp\">{timestamp}</span> {who} named the conversation <b>{}</b></p></div>\n",
            msg.group_title.as_deref().unwrap_or(UNKNOWN)
        )
    }
    fn write_to_file(file: &Path, text: &str) {
        let mut file = File::options()
            .append(true)
            .create(true)
            .open(file)
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }
}

impl<'a> HTML<'a> {
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

    fn add_line(&self, string: &mut String, part: &str, pre: &str, post: &str) {
        if !part.is_empty() {
            string.push_str(pre);
            string.push_str(part);
            string.push_str(post);
            string.push('\n');
        }
    }

    fn write_headers(path: &Path) {
        // Write file header
        HTML::write_to_file(path, HEADER);

        // Write CSS
        HTML::write_to_file(path, "<style>\n");
        HTML::write_to_file(path, STYLE);
        HTML::write_to_file(path, "\n</style>");
    }
}

#[cfg(test)]
mod tests {
    use crate::{Config, Exporter, Options, HTML};
    use imessage_database::{tables::messages::Message, util::dirs::default_db_path};

    fn blank() -> Message {
        Message {
            rowid: i32::default(),
            guid: String::default(),
            text: None,
            service: Some("iMessage".to_string()),
            handle_id: i32::default(),
            subject: None,
            date: i64::default(),
            date_read: i64::default(),
            date_delivered: i64::default(),
            is_from_me: false,
            is_read: false,
            group_title: None,
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
            export_type: Some("html"),
            export_path: None,
            valid: true,
        }
    }

    #[test]
    fn can_create() {
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = HTML::new(&config);
        assert_eq!(exporter.files.len(), 0);
    }

    #[test]
    fn can_get_time_valid() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = HTML::new(&config);

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
        let exporter = HTML::new(&config);

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
        let exporter = HTML::new(&config);

        // Create sample data
        let mut s = String::new();
        exporter.add_line(&mut s, "hello world", "", "");

        assert_eq!(s, "hello world\n".to_string());
    }

    #[test]
    fn can_add_line() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = HTML::new(&config);

        // Create sample data
        let mut s = String::new();
        exporter.add_line(&mut s, "hello world", "  ", "");

        assert_eq!(s, "  hello world\n".to_string());
    }

    #[test]
    fn can_add_line_pre_post() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = HTML::new(&config);

        // Create sample data
        let mut s = String::new();
        exporter.add_line(&mut s, "hello world", "<div>", "</div>");

        assert_eq!(s, "<div>hello world</div>\n".to_string());
    }
}
