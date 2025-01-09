use std::{
    borrow::Cow,
    collections::{
        hash_map::Entry::{Occupied, Vacant},
        HashMap,
    },
    fs::File,
    io::{BufWriter, Write},
};

use crate::{
    app::{
        error::RuntimeError, progress::build_progress_bar_export, runtime::Config,
        sanitizers::sanitize_html,
    },
    exporters::exporter::{BalloonFormatter, Exporter, TextEffectFormatter, Writer},
};

use imessage_database::{
    error::{plist::PlistParseError, table::TableError},
    message_types::{
        app::AppMessage,
        app_store::AppStoreMessage,
        collaboration::CollaborationMessage,
        digital_touch::{self, DigitalTouch},
        edited::{EditStatus, EditedMessage},
        expressives::{BubbleEffect, Expressive, ScreenEffect},
        handwriting::HandwrittenMessage,
        music::MusicMessage,
        placemark::PlacemarkMessage,
        sticker::StickerSource,
        text_effects::{Animation, Style, TextEffect, Unit},
        url::URLMessage,
        variants::{Announcement, BalloonProvider, CustomBalloon, URLOverride, Variant},
    },
    tables::{
        attachment::{Attachment, MediaType},
        messages::{
            models::{AttachmentMeta, BubbleComponent},
            Message,
        },
        table::{Table, FITNESS_RECEIVER, ME, ORPHANED, YOU},
    },
    util::{
        dates::{format, get_local_time, readable_diff, TIMESTAMP_FACTOR},
        plist::parse_ns_keyed_archiver,
    },
};

const HEADER: &str = "<html>\n<head>\n<meta charset=\"UTF-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">";
const FOOTER: &str = "</body></html>";
const STYLE: &str = include_str!("resources/style.css");

pub struct HTML<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config,
    /// Handles to files we want to write messages to
    /// Map of resolved chatroom file location to a buffered writer
    pub files: HashMap<String, BufWriter<File>>,
    /// Writer instance for orphaned messages
    pub orphaned: BufWriter<File>,
}

impl<'a> Exporter<'a> for HTML<'a> {
    fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        let mut orphaned = config.options.export_path.clone();
        orphaned.push(ORPHANED);
        orphaned.set_extension("html");
        let file = File::options()
            .append(true)
            .create(true)
            .open(&orphaned)
            .map_err(|err| RuntimeError::CreateError(err, orphaned))?;

        Ok(HTML {
            config,
            files: HashMap::new(),
            orphaned: BufWriter::new(file),
        })
    }

    fn iter_messages(&mut self) -> Result<(), RuntimeError> {
        // Tell the user what we are doing
        eprintln!(
            "Exporting to {} as html...",
            self.config.options.export_path.display()
        );

        // Write orphaned file headers
        HTML::write_headers(&mut self.orphaned)?;

        // Keep track of current message ROWID
        let mut current_message_row = -1;

        // Set up progress bar
        let mut current_message = 0;
        let total_messages =
            Message::get_count(&self.config.db, &self.config.options.query_context)
                .map_err(RuntimeError::DatabaseError)?;
        let pb = build_progress_bar_export(total_messages);

        let mut statement =
            Message::stream_rows(&self.config.db, &self.config.options.query_context)
                .map_err(RuntimeError::DatabaseError)?;

        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .map_err(|err| RuntimeError::DatabaseError(TableError::Messages(err)))?;

        for message in messages {
            let mut msg = Message::extract(message).map_err(RuntimeError::DatabaseError)?;

            // Early escape if we try and render the same message GUID twice
            // See https://github.com/ReagentX/imessage-exporter/issues/135 for rationale
            if msg.rowid == current_message_row {
                current_message += 1;
                continue;
            }
            current_message_row = msg.rowid;

            // Skip non-deleted messages if only_deleted is enabled  
            if self.config.options.only_deleted && !msg.is_deleted() {
                current_message += 1;
                continue;
            }

            // Generate the text of the message
            let _ = msg.generate_text(&self.config.db);

            // Render the announcement in-line
            if msg.is_announcement() {
                let announcement = self.format_announcement(&msg);
                HTML::write_to_file(self.get_or_create_file(&msg)?, &announcement)?;
            }
            // Message replies and tapbacks are rendered in context, so no need to render them separately
            else if !msg.is_tapback() {
                let message = self
                    .format_message(&msg, 0)
                    .map_err(RuntimeError::DatabaseError)?;
                HTML::write_to_file(self.get_or_create_file(&msg)?, &message)?;
            }
            current_message += 1;
            if current_message % 99 == 0 {
                pb.set_position(current_message);
            }
        }
        pb.finish();

        eprintln!("Writing HTML footers...");
        for (_, buf) in self.files.iter_mut() {
            HTML::write_to_file(buf, FOOTER)?;
        }
        HTML::write_to_file(&mut self.orphaned, FOOTER)?;

        Ok(())
    }

    /// Create a file for the given chat, caching it so we don't need to build it later
    fn get_or_create_file(
        &mut self,
        message: &Message,
    ) -> Result<&mut BufWriter<File>, RuntimeError> {
        match self.config.conversation(message) {
            Some((chatroom, _)) => {
                let filename = self.config.filename(chatroom);
                match self.files.entry(filename) {
                    Occupied(entry) => Ok(entry.into_mut()),
                    Vacant(entry) => {
                        let mut path = self.config.options.export_path.clone();
                        path.push(self.config.filename(chatroom));
                        path.set_extension("html");

                        // If the file already exists, don't write the headers again
                        // This can happen if multiple chats use the same group name
                        let file_exists = path.exists();

                        let file = File::options()
                            .append(true)
                            .create(true)
                            .open(&path)
                            .map_err(|err| RuntimeError::CreateError(err, path))?;

                        let mut buf = BufWriter::new(file);

                        // Write headers if the file does not exist
                        if !file_exists {
                            let _ = HTML::write_headers(&mut buf);
                        }

                        Ok(entry.insert(buf))
                    }
                }
            }
            None => Ok(&mut self.orphaned),
        }
    }
}

impl<'a> Writer<'a> for HTML<'a> {
    fn format_message(&self, message: &Message, indent_size: usize) -> Result<String, TableError> {
        // Data we want to write to a file
        let mut formatted_message = String::new();

        // Message div
        if message.is_reply() && indent_size == 0 {
            // Add an ID for any top-level message so we can link to them in threads
            self.add_line(
                &mut formatted_message,
                &format!("<div class=\"message\", id=\"r-{}\">", message.guid),
                "",
                "",
            );
        } else {
            // No ID needed if the message has no replies
            self.add_line(&mut formatted_message, "<div class=\"message\">", "", "");
        }

        // Start message div
        if message.is_from_me() {
            self.add_line(
                &mut formatted_message,
                &format!("<div class=\"sent {}\">", message.service()),
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

        // Add reply anchor if necessary
        if message.is_reply() {
            if indent_size > 0 {
                // If we are indented it means we are rendering in a thread
                self.add_line(
                    &mut formatted_message,
                    &format!("<a href=\"#r-{}\">⇲</a>", message.guid),
                    "<span class=\"reply_anchor\">",
                    "</span>",
                );
            } else {
                // If there is no ident we are rendering a top-level message
                self.add_line(
                    &mut formatted_message,
                    &format!("<a href=\"#{}\">⇱</a>", message.guid),
                    "<span class=\"reply_anchor\">",
                    "</span>",
                );
            }
        }

        // Add message sender
        self.add_line(
            &mut formatted_message,
            self.config.who(
                message.handle_id,
                message.is_from_me(),
                &message.destination_caller_id,
            ),
            "<span class=\"sender\">",
            "</span></p>",
        );

        // If message was deleted (not unsent), annotate it
        if message.is_deleted() {
            self.add_line(
                &mut formatted_message,
                "This message was deleted from the conversation!",
                "<span class=\"deleted\">",
                "</span></p>",
            );
        }

        // Useful message metadata
        let message_parts = message.body();
        let mut attachments = Attachment::from_message(&self.config.db, message)?;
        let mut replies = message.get_replies(&self.config.db)?;

        // Index of where we are in the attachment Vector
        let mut attachment_index: usize = 0;

        // Add message subject
        if let Some(subject) = &message.subject {
            // Add message sender
            self.add_line(
                &mut formatted_message,
                &sanitize_html(subject),
                "<p>Subject: <span class=\"subject\">",
                "</span></p>",
            );
        }

        // Handle SharePlay
        if message.is_shareplay() {
            self.add_line(
                &mut formatted_message,
                self.format_shareplay(),
                "<span class=\"shareplay\">",
                "</span>",
            );
        }

        // Handle Shared Location
        if message.started_sharing_location() || message.stopped_sharing_location() {
            self.add_line(
                &mut formatted_message,
                self.format_shared_location(message),
                "<span class=\"shared_location\">",
                "</span>",
            );
        }

        // Generate the message body from it's components
        for (idx, message_part) in message_parts.iter().enumerate() {
            // Write the part div start
            self.add_line(
                &mut formatted_message,
                "<hr><div class=\"message_part\">",
                "",
                "",
            );

            match message_part {
                BubbleComponent::Text(text_attrs) => {
                    if let Some(text) = &message.text {
                        // Render edited message content, if applicable
                        if message.is_part_edited(idx) {
                            if let Some(edited_parts) = &message.edited_parts {
                                if let Some(edited) =
                                    self.format_edited(message, edited_parts, idx, "")
                                {
                                    self.add_line(
                                        &mut formatted_message,
                                        &edited,
                                        "<div class=\"edited\">",
                                        "</div>",
                                    );
                                };
                            }
                        } else {
                            let mut formatted_text = String::with_capacity(text.len());

                            for text_attr in text_attrs {
                                // We cannot sanitize the html beforehand because it may change the length of the text
                                if let Some(message_content) =
                                    text.get(text_attr.start..text_attr.end)
                                {
                                    formatted_text.push_str(&self.format_attributed(
                                        &sanitize_html(message_content),
                                        &text_attr.effect,
                                    ))
                                }
                            }

                            // If we failed to parse any text above, make sure we sanitize if before using it
                            if formatted_text.is_empty() {
                                formatted_text.push_str(&sanitize_html(text));
                            }

                            // Render the message body if the message or message part was not edited
                            // If it was edited, it was rendered already
                            // if match &edited_parts {
                            //     Some(edited_parts) => edited_parts.is_unedited_at(idx),
                            //     None => !message.is_edited(),
                            // } {
                            if formatted_text.starts_with(FITNESS_RECEIVER) {
                                self.add_line(
                                    &mut formatted_message,
                                    &formatted_text.replace(FITNESS_RECEIVER, YOU),
                                    "<span class=\"bubble\">",
                                    "</span>",
                                );
                            } else {
                                self.add_line(
                                    &mut formatted_message,
                                    &formatted_text,
                                    "<span class=\"bubble\">",
                                    "</span>",
                                );
                            }
                        }
                    }
                }
                BubbleComponent::Attachment(metadata) => {
                    match attachments.get_mut(attachment_index) {
                        Some(attachment) => {
                            if attachment.is_sticker {
                                let result = self.format_sticker(attachment, message);
                                self.add_line(
                                    &mut formatted_message,
                                    &result,
                                    "<div class=\"sticker\">",
                                    "</div>",
                                );
                            } else {
                                match self.format_attachment(attachment, message, metadata) {
                                    Ok(result) => {
                                        attachment_index += 1;
                                        self.add_line(
                                            &mut formatted_message,
                                            &result,
                                            "<div class=\"attachment\">",
                                            "</div>",
                                        );
                                    }
                                    Err(result) => {
                                        self.add_line(
                                        &mut formatted_message,
                                        result,
                                        "<span class=\"attachment_error\">Unable to locate attachment: ",
                                        "</span>",
                                    );
                                    }
                                }
                            }
                        }
                        // Attachment does not exist in attachments table
                        None => self.add_line(
                            &mut formatted_message,
                            "Attachment does not exist!",
                            "<span class=\"attachment_error\">",
                            "</span>",
                        ),
                    }
                }
                BubbleComponent::App => match self.format_app(message, &mut attachments, "") {
                    Ok(ok_bubble) => self.add_line(
                        &mut formatted_message,
                        &ok_bubble,
                        "<div class=\"app\">",
                        "</div>",
                    ),
                    Err(why) => self.add_line(
                        &mut formatted_message,
                        &format!("Unable to format {:?} message: {why}", message.variant()),
                        "<div class=\"app_error\">",
                        "</div>",
                    ),
                },
                BubbleComponent::Retracted => {
                    if let Some(edited_parts) = &message.edited_parts {
                        if let Some(edited) = self.format_edited(message, edited_parts, idx, "") {
                            self.add_line(
                                &mut formatted_message,
                                &edited,
                                "<span class=\"unsent\">",
                                "</span>",
                            );
                        };
                    }
                }
            };

            // Write the part div end
            self.add_line(&mut formatted_message, "</div>", "", "");

            // Handle expressives
            if message.expressive_send_style_id.is_some() {
                self.add_line(
                    &mut formatted_message,
                    self.format_expressive(message),
                    "<span class=\"expressive\">",
                    "</span>",
                );
            }

            // Handle Tapbacks
            if let Some(tapbacks_map) = self.config.tapbacks.get(&message.guid) {
                if let Some(tapbacks) = tapbacks_map.get(&idx) {
                    let mut formatted_tapbacks = String::new();

                    tapbacks
                        .iter()
                        .try_for_each(|tapback| -> Result<(), TableError> {
                            let formatted = self.format_tapback(tapback)?;
                            if !formatted.is_empty() {
                                self.add_line(
                                    &mut formatted_tapbacks,
                                    &self.format_tapback(tapback)?,
                                    "<div class=\"tapback\">",
                                    "</div>",
                                );
                            }
                            Ok(())
                        })?;

                    if !formatted_tapbacks.is_empty() {
                        self.add_line(
                            &mut formatted_message,
                            "<hr><p>Tapbacks:</p>",
                            "<div class=\"tapbacks\">",
                            "",
                        );
                        self.add_line(&mut formatted_message, &formatted_tapbacks, "", "");
                    }
                    self.add_line(&mut formatted_message, "</div>", "", "");
                }
            }

            // Handle Replies
            if let Some(replies) = replies.get_mut(&idx) {
                self.add_line(&mut formatted_message, "<div class=\"replies\">", "", "");
                replies
                    .iter_mut()
                    .try_for_each(|reply| -> Result<(), TableError> {
                        let _ = reply.generate_text(&self.config.db);
                        if !reply.is_tapback() {
                            // Set indent to 1 so we know this is a recursive call
                            self.add_line(
                                &mut formatted_message,
                                &self.format_message(reply, 1)?,
                                &format!("<div class=\"reply\" id=\"{}\">", reply.guid),
                                "</div>",
                            );
                        }
                        Ok(())
                    })?;
                self.add_line(&mut formatted_message, "</div>", "", "");
            }
        }

        // Add a note if the message is a reply and not rendered in a thread
        if message.is_reply() && indent_size == 0 {
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

        Ok(formatted_message)
    }

    fn format_attachment(
        &self,
        attachment: &'a mut Attachment,
        message: &Message,
        metadata: &AttachmentMeta,
    ) -> Result<String, &'a str> {
        // Copy the file, if requested
        self.config
            .options
            .attachment_manager
            .handle_attachment(message, attachment, self.config)
            .ok_or(attachment.filename())?;

        // Build a relative filepath from the fully qualified one on the `Attachment`
        let embed_path = self.config.message_attachment_path(attachment);

        Ok(match attachment.mime_type() {
            MediaType::Image(_) => {
                if self.config.options.no_lazy {
                    format!("<img src=\"{embed_path}\">")
                } else {
                    format!("<img src=\"{embed_path}\" loading=\"lazy\">")
                }
            }
            MediaType::Video(media_type) => {
                // See https://github.com/ReagentX/imessage-exporter/issues/73 for why duplicate the source tag
                format!("<video controls> <source src=\"{embed_path}\" type=\"{media_type}\"> <source src=\"{embed_path}\"> </video>")
            }
            MediaType::Audio(media_type) => {
                if let Some(transcription) = metadata.transcription {
                    return Ok(format!("<div><audio controls src=\"{embed_path}\" type=\"{media_type}\" </audio></div> <hr><span class=\"transcription\">Transcription: {transcription}</span>"));
                }
                format!("<audio controls src=\"{embed_path}\" type=\"{media_type}\" </audio>")
            }
            MediaType::Text(_) => {
                format!(
                    "<a href=\"{embed_path}\">Click to download {} ({})</a>",
                    attachment.filename(),
                    attachment.file_size()
                )
            }
            MediaType::Application(_) => format!(
                "<a href=\"{embed_path}\">Click to download {} ({})</a>",
                attachment.filename(),
                attachment.file_size()
            ),
            MediaType::Unknown => {
                format!("<p>Unknown attachment type: {embed_path}</p> <a href=\"{embed_path}\">Download ({})</a>", attachment.file_size())
            }
            MediaType::Other(media_type) => {
                format!("<p>Unable to embed {media_type} attachments: {embed_path}</p>")
            }
        })
    }

    fn format_sticker(&self, sticker: &'a mut Attachment, message: &Message) -> String {
        match self.format_attachment(sticker, message, &AttachmentMeta::default()) {
            Ok(mut sticker_embed) => {
                // Determine the source of the sticker
                if let Some(sticker_source) = sticker.get_sticker_source(&self.config.db) {
                    match sticker_source {
                        StickerSource::Genmoji => {
                            // Add sticker prompt
                            if let Some(prompt) = &sticker.emoji_description {
                                sticker_embed.push_str(&format!(
                                    "\n<div class=\"genmoji_prompt\">Genmoji prompt: {prompt}</div>"
                                ))
                            }
                        }
                        StickerSource::Memoji => sticker_embed
                            .push_str("\n<div class=\"sticker_name\">App: Memoji</div>"),
                        StickerSource::UserGenerated => {
                            // Add sticker effect
                            if let Ok(Some(sticker_effect)) = sticker.get_sticker_effect(
                                &self.config.options.platform,
                                &self.config.options.db_path,
                                self.config.options.attachment_root.as_deref(),
                            ) {
                                sticker_embed.push_str(&format!(
                                    "\n<div class=\"sticker_effect\">Sent with {sticker_effect} effect</div>"
                                ))
                            }
                        }
                        StickerSource::App(bundle_id) => {
                            // Add the application name used to generate/send the sticker
                            let app_name = sticker
                                .get_sticker_source_application_name(&self.config.db)
                                .unwrap_or(bundle_id);
                            sticker_embed.push_str(&format!(
                                "\n<div class=\"sticker_name\">App: {app_name}</div>"
                            ))
                        }
                    }
                }

                sticker_embed
            }
            Err(embed) => embed.to_string(),
        }
    }

    fn format_app(
        &self,
        message: &'a Message,
        attachments: &mut Vec<Attachment>,
        _: &str,
    ) -> Result<String, PlistParseError> {
        if let Variant::App(balloon) = message.variant() {
            let mut app_bubble = String::new();

            // Handwritten messages use a different payload type, so check that first
            if message.is_handwriting() {
                if let Some(payload) = message.raw_payload_data(&self.config.db) {
                    return match HandwrittenMessage::from_payload(&payload) {
                        Ok(bubble) => Ok(self.format_handwriting(message, &bubble, message)),
                        Err(why) => Err(PlistParseError::HandwritingError(why)),
                    };
                }
            }

            if message.is_digital_touch() {
                if let Some(payload) = message.raw_payload_data(&self.config.db) {
                    return match digital_touch::from_payload(&payload) {
                        Some(bubble) => Ok(self.format_digital_touch(message, &bubble, message)),
                        None => Err(PlistParseError::DigitalTouchError),
                    };
                }
            }

            if let Some(payload) = message.payload_data(&self.config.db) {
                let res = if message.is_url() {
                    let parsed = parse_ns_keyed_archiver(&payload)?;
                    let bubble = URLMessage::get_url_message_override(&parsed)?;
                    match bubble {
                        URLOverride::Normal(balloon) => self.format_url(message, &balloon, message),
                        URLOverride::AppleMusic(balloon) => self.format_music(&balloon, message),
                        URLOverride::Collaboration(balloon) => {
                            self.format_collaboration(&balloon, message)
                        }
                        URLOverride::AppStore(balloon) => self.format_app_store(&balloon, message),
                        URLOverride::SharedPlacemark(balloon) => {
                            self.format_placemark(&balloon, message)
                        }
                    }
                } else {
                    let parsed = parse_ns_keyed_archiver(&payload)?;
                    match AppMessage::from_map(&parsed) {
                        Ok(bubble) => match balloon {
                            CustomBalloon::Application(bundle_id) => {
                                self.format_generic_app(&bubble, bundle_id, attachments, message)
                            }
                            CustomBalloon::ApplePay => self.format_apple_pay(&bubble, message),
                            CustomBalloon::Fitness => self.format_fitness(&bubble, message),
                            CustomBalloon::Slideshow => self.format_slideshow(&bubble, message),
                            CustomBalloon::CheckIn => self.format_check_in(&bubble, message),
                            CustomBalloon::FindMy => self.format_find_my(&bubble, message),
                            CustomBalloon::Handwriting => unreachable!(),
                            CustomBalloon::DigitalTouch => unreachable!(),
                            CustomBalloon::URL => unreachable!(),
                        },
                        Err(why) => return Err(why),
                    }
                };
                app_bubble.push_str(&res);
            } else {
                // Sometimes, URL messages are missing their payloads
                if message.is_url() {
                    if let Some(text) = &message.text {
                        let mut out_s = String::new();
                        out_s.push_str("<a href=\"");
                        out_s.push_str(text);
                        out_s.push_str("\">");

                        out_s.push_str("<div class=\"app_header\"><div class=\"name\">");
                        out_s.push_str(text);
                        out_s.push_str("</div></div>");

                        out_s.push_str("<div class=\"app_footer\"><div class=\"caption\">");
                        out_s.push_str(text);
                        out_s.push_str("</div></div></a>");

                        return Ok(out_s);
                    }
                }
                return Err(PlistParseError::NoPayload);
            }
            Ok(app_bubble)
        } else {
            Err(PlistParseError::WrongMessageType)
        }
    }

    fn format_tapback(&self, msg: &Message) -> Result<String, TableError> {
        match msg.variant() {
            Variant::Tapback(_, added, tapback) => {
                if !added {
                    return Ok(String::new());
                }
                Ok(format!(
                    "<span class=\"tapback\"><b>{}</b> by {}</span>",
                    tapback,
                    self.config
                        .who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id),
                ))
            }
            Variant::Sticker(_) => {
                let mut paths = Attachment::from_message(&self.config.db, msg)?;
                let who =
                    self.config
                        .who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id);
                // Sticker messages have only one attachment, the sticker image
                Ok(match paths.get_mut(0) {
                    Some(sticker) => format!(
                        "{} <div class=\"sticker_tapback\">&nbsp;by {who}</div>",
                        self.format_sticker(sticker, msg)
                    ),
                    None => {
                        format!("<span class=\"tapback\">Sticker from {who} not found!</span>")
                    }
                })
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
                ScreenEffect::ShootingStar => "Sent with Shooting Star",
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
            Expressive::None => "",
        }
    }

    fn format_announcement(&self, msg: &'a Message) -> String {
        let mut who = self
            .config
            .who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id);
        // Rename yourself so we render the proper grammar here
        if who == ME {
            who = self.config.options.custom_name.as_deref().unwrap_or("You");
        }
        let timestamp = format(&msg.date(&self.config.offset));

        match msg.get_announcement() {
            Some(announcement) => match announcement {
                Announcement::NameChange(name) => {
                    let clean_name = sanitize_html(name);
                    format!(
                        "\n<div class =\"announcement\"><p><span class=\"timestamp\">{timestamp}</span> {who} named the conversation <b>{clean_name}</b></p></div>\n"
                    )
                }
                Announcement::PhotoChange => {
                    format!(
                        "\n<div class =\"announcement\"><p><span class=\"timestamp\">{timestamp}</span> {who} changed the group photo.</p></div>\n"
                    )
                }
                Announcement::Unknown(num) => {
                    format!(
                        "\n<div class =\"announcement\"><p><span class=\"timestamp\">{timestamp}</span> {who} performed unknown action {num}</p></div>\n"
                    )
                }
                Announcement::FullyUnsent => {
                    format!(
                        "<div class =\"announcement\"><p><span class=\"timestamp\">{timestamp}</span> {who} unsent a message.</p></div>"
                    )
                }
            },
            None => String::from(
                "\n<div class =\"announcement\"><p>Unable to format announcement!</p></div>\n",
            ),
        }
    }

    fn format_shareplay(&self) -> &str {
        "<hr>SharePlay Message Ended"
    }

    fn format_shared_location(&self, msg: &'a Message) -> &str {
        // Handle Shared Location
        if msg.started_sharing_location() {
            return "<hr>Started sharing location!";
        } else if msg.stopped_sharing_location() {
            return "<hr>Stopped sharing location!";
        }
        "<hr>Shared location!"
    }

    fn format_edited(
        &self,
        msg: &'a Message,
        edited_message: &'a EditedMessage,
        message_part_idx: usize,
        _: &str,
    ) -> Option<String> {
        if let Some(edited_message_part) = edited_message.part(message_part_idx) {
            let mut out_s = String::new();
            let mut previous_timestamp: Option<&i64> = None;

            match edited_message_part.status {
                EditStatus::Edited => {
                    out_s.push_str("<table>");

                    for (idx, event) in edited_message_part.edit_history.iter().enumerate() {
                        let last = idx == edited_message_part.edit_history.len() - 1;
                        let clean_text = sanitize_html(&event.text);
                        match previous_timestamp {
                            None => out_s.push_str(&self.edited_to_html("", &clean_text, last)),
                            Some(prev_timestamp) => {
                                let end = get_local_time(&event.date, &self.config.offset);
                                let start = get_local_time(prev_timestamp, &self.config.offset);

                                let diff = readable_diff(start, end).unwrap_or_default();
                                out_s.push_str(&self.edited_to_html(
                                    &format!("Edited {diff} later"),
                                    &clean_text,
                                    last,
                                ));
                            }
                        }

                        // Update the previous timestamp for the next loop
                        previous_timestamp = Some(&event.date);
                    }

                    out_s.push_str("</table>");
                }
                EditStatus::Unsent => {
                    let who = if msg.is_from_me() {
                        self.config.options.custom_name.as_deref().unwrap_or(YOU)
                    } else {
                        self.config
                            .who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id)
                    };

                    match readable_diff(
                        msg.date(&self.config.offset),
                        msg.date_edited(&self.config.offset),
                    ) {
                        Some(diff) => {
                            out_s.push_str(&format!(
                                "<span class=\"unsent\">{who} unsent this message part {diff} after sending!</span>"
                            ))
                        },
                        None => {
                            out_s.push_str(&format!(
                                "<span class=\"unsent\">{who} unsent this message part!</span>"
                            ))
                        },
                    }
                }
                EditStatus::Original => {
                    return None;
                }
            }
            return Some(out_s);
        }
        None
    }

    fn format_attributed(&'a self, text: &'a str, attribute: &'a TextEffect) -> Cow<'a, str> {
        match attribute {
            TextEffect::Default => Cow::Borrowed(text),
            TextEffect::Mention(mentioned) => Cow::Owned(self.format_mention(text, mentioned)),
            TextEffect::Link(url) => Cow::Owned(self.format_link(text, url)),
            TextEffect::OTP => Cow::Owned(self.format_otp(text)),
            TextEffect::Styles(styles) => Cow::Owned(self.format_styles(text, styles)),
            TextEffect::Animated(animation) => Cow::Owned(self.format_animated(text, animation)),
            TextEffect::Conversion(unit) => Cow::Owned(self.format_conversion(text, unit)),
        }
    }

    fn write_to_file(file: &mut BufWriter<File>, text: &str) -> Result<(), RuntimeError> {
        file.write_all(text.as_bytes())
            .map_err(RuntimeError::DiskError)
    }
}

impl<'a> BalloonFormatter<&'a Message> for HTML<'a> {
    fn format_url(&self, msg: &Message, balloon: &URLMessage, _: &Message) -> String {
        let mut out_s = String::new();

        // Make the whole bubble clickable
        let mut close_url = false;
        if let Some(url) = balloon.get_url() {
            out_s.push_str("<a href=\"");
            out_s.push_str(url);
            out_s.push_str("\">");
            close_url = true;
        } else if let Some(text) = &msg.text {
            // Fallback if the balloon data does not contain the URL
            out_s.push_str("<a href=\"");
            out_s.push_str(text);
            out_s.push_str("\">");
            close_url = true;
        }

        // Header section
        out_s.push_str("<div class=\"app_header\">");

        // Add preview images
        balloon.images.iter().for_each(|image| {
            out_s.push_str("<img src=\"");
            out_s.push_str(image);
            if self.config.options.no_lazy {
                out_s.push_str("\" onerror=\"this.style.display='none'\">");
            } else {
                out_s.push_str("\" loading=\"lazy\", onerror=\"this.style.display='none'\">");
            }
        });

        if let Some(site_name) = balloon.site_name {
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(site_name);
            out_s.push_str("</div>");
        } else if let Some(url) = balloon.get_url() {
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(url);
            out_s.push_str("</div>");
        } else if let Some(text) = &msg.text {
            // Fallback if the balloon data does not contain the URL
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(text);
            out_s.push_str("</div>");
        }

        // Header end
        out_s.push_str("</div>");

        // Only write the footer if there is data to write
        if balloon.title.is_some() || balloon.summary.is_some() {
            out_s.push_str("<div class=\"app_footer\">");

            // Title
            if let Some(title) = balloon.title {
                out_s.push_str("<div class=\"caption\">");
                out_s.push_str(&sanitize_html(title));
                out_s.push_str("</div>");
            }

            // Subtitle
            if let Some(summary) = balloon.summary {
                out_s.push_str("<div class=\"subcaption\">");
                out_s.push_str(&sanitize_html(summary));
                out_s.push_str("</div>");
            }

            // End footer
            out_s.push_str("</div>");
        }

        // End the link
        if close_url {
            out_s.push_str("</a>");
        }
        out_s
    }

    fn format_music(&self, balloon: &MusicMessage, _: &Message) -> String {
        let mut out_s = String::new();

        // Header section
        out_s.push_str("<div class=\"app_header\">");

        // Name
        if let Some(track_name) = balloon.track_name {
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(track_name);
            out_s.push_str("</div>");
        }

        // Add preview section
        if let Some(preview) = balloon.preview {
            out_s.push_str("<audio controls src=\"");
            out_s.push_str(preview);
            out_s.push_str("\" </audio>");
        }

        // Header end
        out_s.push_str("</div>");

        // Make the footer clickable so we can interact with the preview
        if let Some(url) = balloon.url {
            out_s.push_str("<a href=\"");
            out_s.push_str(url);
            out_s.push_str("\">");
        }

        // Only write the footer if there is data to write
        if balloon.artist.is_some() || balloon.album.is_some() {
            out_s.push_str("<div class=\"app_footer\">");

            // artist
            if let Some(artist) = balloon.artist {
                out_s.push_str("<div class=\"caption\">");
                out_s.push_str(artist);
                out_s.push_str("</div>");
            }

            // Subtitle
            if let Some(album) = balloon.album {
                out_s.push_str("<div class=\"subcaption\">");
                out_s.push_str(album);
                out_s.push_str("</div>");
            }

            // End footer
            out_s.push_str("</div>");
        }

        // End the link
        if balloon.url.is_some() {
            out_s.push_str("</a>");
        }
        out_s
    }

    fn format_collaboration(&self, balloon: &CollaborationMessage, _: &Message) -> String {
        let mut out_s = String::new();

        // Header section
        out_s.push_str("<div class=\"app_header\">");

        // Name
        if let Some(app_name) = balloon.app_name {
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(app_name);
            out_s.push_str("</div>");
        } else if let Some(bundle_id) = balloon.bundle_id {
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(bundle_id);
            out_s.push_str("</div>");
        }

        // Header end
        out_s.push_str("</div>");

        // Make the footer clickable so we can interact with the preview
        if let Some(url) = balloon.url {
            out_s.push_str("<a href=\"");
            out_s.push_str(url);
            out_s.push_str("\">");
        }

        // Only write the footer if there is data to write
        if balloon.title.is_some() || balloon.get_url().is_some() {
            out_s.push_str("<div class=\"app_footer\">");

            // artist
            if let Some(title) = balloon.title {
                out_s.push_str("<div class=\"caption\">");
                out_s.push_str(title);
                out_s.push_str("</div>");
            }

            // Subtitle
            if let Some(url) = balloon.get_url() {
                out_s.push_str("<div class=\"subcaption\">");
                out_s.push_str(url);
                out_s.push_str("</div>");
            }

            // End footer
            out_s.push_str("</div>");
        }

        // End the link
        if balloon.url.is_some() {
            out_s.push_str("</a>");
        }

        out_s
    }

    fn format_app_store(&self, balloon: &AppStoreMessage, _: &'a Message) -> String {
        let mut out_s = String::new();

        // Header section
        out_s.push_str("<div class=\"app_header\">");

        // App name
        if let Some(app_name) = balloon.app_name {
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(app_name);
            out_s.push_str("</div>");
        }

        // Header end
        out_s.push_str("</div>");

        // Make the footer clickable so we can interact with the preview
        if let Some(url) = balloon.url {
            out_s.push_str("<a href=\"");
            out_s.push_str(url);
            out_s.push_str("\">");
        }

        // Only write the footer if there is data to write
        if balloon.description.is_some() || balloon.genre.is_some() {
            out_s.push_str("<div class=\"app_footer\">");

            // App description
            if let Some(description) = balloon.description {
                out_s.push_str("<div class=\"caption\">");
                out_s.push_str(description);
                out_s.push_str("</div>");
            }

            // App platform
            if let Some(platform) = balloon.platform {
                out_s.push_str("<div class=\"subcaption\">");
                out_s.push_str(platform);
                out_s.push_str("</div>");
            }

            // App genre
            if let Some(genre) = balloon.genre {
                out_s.push_str("<div class=\"trailing_subcaption\">");
                out_s.push_str(genre);
                out_s.push_str("</div>");
            }

            // End footer
            out_s.push_str("</div>");
        }

        // End the link
        if balloon.url.is_some() {
            out_s.push_str("</a>");
        }
        out_s
    }

    fn format_placemark(&self, balloon: &PlacemarkMessage, _: &'a Message) -> String {
        let mut out_s = String::new();

        // Make the whole bubble clickable
        if let Some(url) = balloon.get_url() {
            out_s.push_str("<a href=\"");
            out_s.push_str(url);
            out_s.push_str("\">");
        }

        // Header section
        out_s.push_str("<div class=\"app_header\">");

        if let Some(place_name) = balloon.place_name {
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(place_name);
            out_s.push_str("</div>");
        } else if let Some(url) = balloon.get_url() {
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(url);
            out_s.push_str("</div>");
        }

        // Header end
        out_s.push_str("</div>");

        // Only write the footer if there is data to write
        if balloon.placemark.address.is_some()
            || balloon.placemark.postal_code.is_some()
            || balloon.placemark.country.is_some()
            || balloon.placemark.sub_administrative_area.is_some()
        {
            out_s.push_str("<div class=\"app_footer\">");

            // Address
            if let Some(address) = balloon.placemark.address {
                out_s.push_str("<div class=\"caption\">");
                out_s.push_str(address);
                out_s.push_str("</div>");
            }

            // Postal Code
            if let Some(postal_code) = balloon.placemark.postal_code {
                out_s.push_str("<div class=\"trailing_caption\">");
                out_s.push_str(postal_code);
                out_s.push_str("</div>");
            }

            // Country
            if let Some(country) = balloon.placemark.country {
                out_s.push_str("<div class=\"subcaption\">");
                out_s.push_str(country);
                out_s.push_str("</div>");
            }

            // Administrative Area
            if let Some(area) = balloon.placemark.sub_administrative_area {
                out_s.push_str("<div class=\"trailing_subcaption\">");
                out_s.push_str(area);
                out_s.push_str("</div>");
            }

            // End footer
            out_s.push_str("</div>");
        }

        // End the link
        if balloon.get_url().is_some() {
            out_s.push_str("</a>");
        }
        out_s
    }

    fn format_handwriting(&self, _: &Message, balloon: &HandwrittenMessage, _: &Message) -> String {
        // svg can be embedded directly into the html
        balloon.render_svg()
    }

    fn format_digital_touch(&self, _: &Message, balloon: &DigitalTouch, _: &'a Message) -> String {
        format!(
            "<div class=\"app_header\"><div class=\"name\">Digital Touch Message</div></div>\n<div class=\"app_footer\"><div class=\"caption\">{:?}</div></div>",
            balloon
        )
    }

    fn format_apple_pay(&self, balloon: &AppMessage, _: &Message) -> String {
        let mut out_s = String::new();

        out_s.push_str("<div class=\"app_header\">");

        if let Some(app_name) = balloon.app_name {
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(app_name);
            out_s.push_str("</div>");
        }

        // Header end, footer begin
        out_s.push_str("</div>");
        out_s.push_str("<div class=\"app_footer\">");

        if let Some(ldtext) = balloon.ldtext {
            out_s.push_str("<div class=\"caption\">");
            out_s.push_str(ldtext);
            out_s.push_str("</div>");
        }

        // End footer
        out_s.push_str("</div>");

        out_s
    }

    fn format_fitness(&self, balloon: &AppMessage, message: &Message) -> String {
        self.balloon_to_html(balloon, "Fitness", &mut [], message)
    }

    fn format_slideshow(&self, balloon: &AppMessage, message: &Message) -> String {
        self.balloon_to_html(balloon, "Slideshow", &mut [], message)
    }

    fn format_find_my(&self, balloon: &AppMessage, _: &'a Message) -> String {
        let mut out_s = String::new();

        out_s.push_str("<div class=\"app_header\">");

        if let Some(app_name) = balloon.app_name {
            out_s.push_str("<div class=\"name\">");
            out_s.push_str(app_name);
            out_s.push_str("</div>");
        }

        // Header end, footer begin
        out_s.push_str("</div>");
        out_s.push_str("<div class=\"app_footer\">");

        if let Some(ldtext) = balloon.ldtext {
            out_s.push_str("<div class=\"caption\">");
            out_s.push_str(ldtext);
            out_s.push_str("</div>");
        }

        // End footer
        out_s.push_str("</div>");

        out_s
    }

    fn format_check_in(&self, balloon: &AppMessage, _: &Message) -> String {
        let mut out_s = String::new();

        out_s.push_str("<div class=\"app_header\">");

        // Name
        out_s.push_str("<div class=\"name\">");
        out_s.push_str(balloon.app_name.unwrap_or("Check In"));
        out_s.push_str("</div>");

        // ldtext
        if let Some(ldtext) = balloon.ldtext {
            out_s.push_str("<div class=\"ldtext\">");
            out_s.push_str(ldtext);
            out_s.push_str("</div>");
        }

        // Header end, footer begin
        out_s.push_str("</div>");

        // Only write the footer if there is data to write
        let metadata: HashMap<&str, &str> = balloon.parse_query_string();

        // Before manual check-in
        if let Some(date_str) = metadata.get("estimatedEndTime") {
            // Parse the estimated end time from the message's query string
            let date_stamp = date_str.parse::<f64>().unwrap_or(0.) as i64 * TIMESTAMP_FACTOR;
            let date_time = get_local_time(&date_stamp, &0);
            let date_string = format(&date_time);

            out_s.push_str("<div class=\"app_footer\">");

            out_s.push_str("<div class=\"caption\">Expected around ");
            out_s.push_str(&date_string);
            out_s.push_str("</div>");

            out_s.push_str("</div>");
        }
        // Expired check-in
        else if let Some(date_str) = metadata.get("triggerTime") {
            // Parse the estimated end time from the message's query string
            let date_stamp = date_str.parse::<f64>().unwrap_or(0.) as i64 * TIMESTAMP_FACTOR;
            let date_time = get_local_time(&date_stamp, &0);
            let date_string = format(&date_time);

            out_s.push_str("<div class=\"app_footer\">");

            out_s.push_str("<div class=\"caption\">Was expected around ");
            out_s.push_str(&date_string);
            out_s.push_str("</div>");

            out_s.push_str("</div>");
        }
        // Accepted check-in
        else if let Some(date_str) = metadata.get("sendDate") {
            // Parse the estimated end time from the message's query string
            let date_stamp = date_str.parse::<f64>().unwrap_or(0.) as i64 * TIMESTAMP_FACTOR;
            let date_time = get_local_time(&date_stamp, &0);
            let date_string = format(&date_time);

            out_s.push_str("<div class=\"app_footer\">");

            out_s.push_str("<div class=\"caption\">Checked in at ");
            out_s.push_str(&date_string);
            out_s.push_str("</div>");

            out_s.push_str("</div>");
        }

        out_s
    }

    fn format_generic_app(
        &self,
        balloon: &AppMessage,
        bundle_id: &str,
        attachments: &mut Vec<Attachment>,
        message: &Message,
    ) -> String {
        self.balloon_to_html(balloon, bundle_id, attachments, message)
    }
}

impl TextEffectFormatter for HTML<'_> {
    fn format_mention(&self, text: &str, mentioned: &str) -> String {
        format!("<span title=\"{mentioned}\"><b>{text}</b></span>")
    }

    fn format_link(&self, text: &str, url: &str) -> String {
        format!("<a href=\"{url}\">{text}</a>")
    }

    fn format_otp(&self, text: &str) -> String {
        format!("<u>{text}</u>")
    }

    fn format_conversion(&self, text: &str, _: &Unit) -> String {
        format!("<u>{text}</u>")
    }

    fn format_styles(&self, text: &str, styles: &[Style]) -> String {
        let (prefix, suffix): (String, String) = styles.iter().rev().fold(
            (String::new(), String::new()),
            |(mut prefix, mut suffix), style| {
                let (open, close) = match style {
                    Style::Bold => ("<b>", "</b>"),
                    Style::Italic => ("<i>", "</i>"),
                    Style::Strikethrough => ("<s>", "</s>"),
                    Style::Underline => ("<u>", "</u>"),
                };
                prefix.push_str(open);
                suffix.insert_str(0, close);
                (prefix, suffix)
            },
        );

        format!("{prefix}{text}{suffix}")
    }

    fn format_animated(&self, text: &str, animation: &Animation) -> String {
        format!("<span class=\"animation{animation:?}\">{text}</span>")
    }
}

impl HTML<'_> {
    fn get_time(&self, message: &Message) -> String {
        let mut date = format(&message.date(&self.config.offset));
        let read_after = message.time_until_read(&self.config.offset);
        if let Some(time) = read_after {
            if !time.is_empty() {
                let who = if message.is_from_me() {
                    "them"
                } else {
                    self.config.options.custom_name.as_deref().unwrap_or("you")
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

    fn write_headers(file: &mut BufWriter<File>) -> Result<(), RuntimeError> {
        // Write file header
        HTML::write_to_file(file, HEADER)?;

        // Write CSS
        HTML::write_to_file(file, "<style>\n")?;
        HTML::write_to_file(file, STYLE)?;
        HTML::write_to_file(file, "\n</style>")?;
        HTML::write_to_file(file, "\n</head>\n<body>\n")?;
        Ok(())
    }

    fn edited_to_html(&self, timestamp: &str, text: &str, last: bool) -> String {
        let tag = if last { "tfoot" } else { "tbody" };
        format!("<{tag}><tr><td><span class=\"timestamp\">{timestamp}</span></td><td>{text}</td></tr></{tag}>")
    }

    fn balloon_to_html(
        &self,
        balloon: &AppMessage,
        bundle_id: &str,
        attachments: &mut [Attachment],
        message: &Message,
    ) -> String {
        let mut out_s = String::new();
        if let Some(url) = balloon.url {
            out_s.push_str("<a href=\"");
            out_s.push_str(url);
            out_s.push_str("\">");
        }
        out_s.push_str("<div class=\"app_header\">");

        // Image
        if let Some(image) = balloon.image {
            out_s.push_str("<img src=\"");
            out_s.push_str(image);
            out_s.push_str("\">");
        } else if let Some(attachment) = attachments.get_mut(0) {
            out_s.push_str(
                &self
                    .format_attachment(attachment, message, &AttachmentMeta::default())
                    .unwrap_or_default(),
            );
        }

        // Name
        out_s.push_str("<div class=\"name\">");
        out_s.push_str(balloon.app_name.unwrap_or(bundle_id));
        out_s.push_str("</div>");

        // Title
        if let Some(title) = balloon.title {
            out_s.push_str("<div class=\"image_title\">");
            out_s.push_str(title);
            out_s.push_str("</div>");
        }

        // Subtitle
        if let Some(subtitle) = balloon.subtitle {
            out_s.push_str("<div class=\"image_subtitle\">");
            out_s.push_str(subtitle);
            out_s.push_str("</div>");
        }

        // ldtext
        if let Some(ldtext) = balloon.ldtext {
            out_s.push_str("<div class=\"ldtext\">");
            out_s.push_str(ldtext);
            out_s.push_str("</div>");
        }

        // Header end, footer begin
        out_s.push_str("</div>");

        // Only write the footer if there is data to write
        if balloon.caption.is_some()
            || balloon.subcaption.is_some()
            || balloon.trailing_caption.is_some()
            || balloon.trailing_subcaption.is_some()
        {
            out_s.push_str("<div class=\"app_footer\">");

            // Caption
            if let Some(caption) = balloon.caption {
                out_s.push_str("<div class=\"caption\">");
                out_s.push_str(caption);
                out_s.push_str("</div>");
            }

            // Subcaption
            if let Some(subcaption) = balloon.subcaption {
                out_s.push_str("<div class=\"subcaption\">");
                out_s.push_str(subcaption);
                out_s.push_str("</div>");
            }

            // Trailing Caption
            if let Some(trailing_caption) = balloon.trailing_caption {
                out_s.push_str("<div class=\"trailing_caption\">");
                out_s.push_str(trailing_caption);
                out_s.push_str("</div>");
            }

            // Trailing Subcaption
            if let Some(trailing_subcaption) = balloon.trailing_subcaption {
                out_s.push_str("<div class=\"trailing_subcaption\">");
                out_s.push_str(trailing_subcaption);
                out_s.push_str("</div>");
            }

            out_s.push_str("</div>");
        }
        if balloon.url.is_some() {
            out_s.push_str("</a>");
        }
        out_s
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env::{current_dir, set_var},
        path::PathBuf,
    };

    use crate::{
        app::export_type::ExportType, exporters::exporter::Writer, Config, Exporter, Options, HTML,
    };
    use imessage_database::{
        tables::{messages::models::AttachmentMeta, table::ME},
        util::platform::Platform,
    };

    #[test]
    fn can_create() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();
        assert_eq!(exporter.files.len(), 0);
    }

    #[test]
    fn can_get_time_valid() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        // let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        // Create fake message
        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        // May 17, 2022  8:29:42 PM
        message.date_delivered = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_read = 674530231992568192;

        assert_eq!(
            exporter.get_time(&message),
            "May 17, 2022  5:29:42 PM (Read by you after 1 hour, 49 seconds)"
        );
    }

    #[test]
    fn can_get_time_invalid() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        // Create fake message
        let mut message = Config::fake_message();
        // May 17, 2022  9:30:31 PM
        message.date = 674530231992568192;
        // May 17, 2022  9:30:31 PM
        message.date_delivered = 674530231992568192;
        // Wed May 18 2022 02:36:24 GMT+0000
        message.date_read = 674526582885055488;
        assert_eq!(exporter.get_time(&message), "May 17, 2022  6:30:31 PM");
    }

    #[test]
    fn can_add_line_no_indent() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        // Create sample data
        let mut s = String::new();
        exporter.add_line(&mut s, "hello world", "", "");

        assert_eq!(s, "hello world\n".to_string());
    }

    #[test]
    fn can_add_line() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        // Create sample data
        let mut s = String::new();
        exporter.add_line(&mut s, "hello world", "  ", "");

        assert_eq!(s, "  hello world\n".to_string());
    }

    #[test]
    fn can_add_line_pre_post() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        // Create sample data
        let mut s = String::new();
        exporter.add_line(&mut s, "hello world", "<div>", "</div>");

        assert_eq!(s, "<div>hello world</div>\n".to_string());
    }

    #[test]
    fn can_format_html_from_me_normal() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_message_with_html() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("<table></table>".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">&lt;table&gt;&lt;/table&gt;</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_me_normal_deleted() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.text = Some("Hello world".to_string());
        message.date = 674526582885055488;
        message.is_from_me = true;
        message.deleted_from = Some(0);

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<span class=\"deleted\">This message was deleted from the conversation!</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_me_normal_read() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("Hello world".to_string());
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_delivered = 674530231992568192;
        message.is_from_me = true;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected =
            "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM (Read by them after 1 hour, 49 seconds)</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_them_normal() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, "Sample Contact".to_string());
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        message.handle_id = Some(999999);

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Sample Contact</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_them_normal_read() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, "Sample Contact".to_string());
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.handle_id = Some(999999);
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        // May 17, 2022  8:29:42 PM
        message.date_delivered = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_read = 674530231992568192;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected =
            "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM (Read by you after 1 hour, 49 seconds)</span>\n<span class=\"sender\">Sample Contact</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_them_custom_name_read() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let mut options = Options::fake_options(ExportType::Html);
        options.custom_name = Some("Name".to_string());
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, "Sample Contact".to_string());
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.handle_id = Some(999999);
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        // May 17, 2022  8:29:42 PM
        message.date_delivered = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_read = 674530231992568192;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected =
            "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM (Read by Name after 1 hour, 49 seconds)</span>\n<span class=\"sender\">Sample Contact</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_shareplay() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, ME.to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.item_type = 6;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<span class=\"shareplay\"><hr>SharePlay Message Ended</span>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_announcement() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, ME.to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;

        let actual = exporter.format_announcement(&message);
        let expected = "\n<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You named the conversation <b>Hello world</b></p></div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_announcement_custom_name() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let mut options = Options::fake_options(ExportType::Html);
        options.custom_name = Some("Name".to_string());
        let mut config = Config::fake_app(options);
        config.participants.insert(0, ME.to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());

        let actual = exporter.format_announcement(&message);
        let expected = "\n<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Name named the conversation <b>Hello world</b></p></div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_me() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, ME.to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2000);
        message.associated_message_guid = Some("fake_guid".to_string());

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = "<span class=\"tapback\"><b>Loved</b> by Me</span>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_them() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, "Sample Contact".to_string());
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2000);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = "<span class=\"tapback\"><b>Loved</b> by Sample Contact</span>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_custom_emoji() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, "Sample Contact".to_string());
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2006);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);
        message.associated_message_emoji = Some("☕️".to_string());

        let actual = exporter.format_tapback(&message).unwrap();
        // The result contains `&nbsp;`
        let expected = "<span class=\"tapback\"><b>☕\u{fe0f}</b> by Sample Contact</span>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_custom_sticker() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, "Sample Contact".to_string());
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2007);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);
        message.associated_message_emoji = Some("☕️".to_string());

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = "<span class=\"tapback\">Sticker from Sample Contact not found!</span>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_started_sharing_location_me() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.is_from_me = false;
        message.other_handle = 2;
        message.share_status = false;
        message.share_direction = false;
        message.item_type = 4;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">Dec 31, 2000  4:00:00 PM</span>\n<span class=\"sender\">Me</span></p>\n<span class=\"shared_location\"><hr>Started sharing location!</span>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_stopped_sharing_location_me() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.is_from_me = false;
        message.other_handle = 2;
        message.share_status = true;
        message.share_direction = false;
        message.item_type = 4;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">Dec 31, 2000  4:00:00 PM</span>\n<span class=\"sender\">Me</span></p>\n<span class=\"shared_location\"><hr>Stopped sharing location!</span>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_started_sharing_location_them() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.handle_id = None;
        message.is_from_me = false;
        message.other_handle = 0;
        message.share_status = false;
        message.share_direction = false;
        message.item_type = 4;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\">Dec 31, 2000  4:00:00 PM</span>\n<span class=\"sender\">Unknown</span></p>\n<span class=\"shared_location\"><hr>Started sharing location!</span>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_stopped_sharing_location_them() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.handle_id = None;
        message.is_from_me = false;
        message.other_handle = 0;
        message.share_status = true;
        message.share_direction = false;
        message.item_type = 4;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\">Dec 31, 2000  4:00:00 PM</span>\n<span class=\"sender\">Unknown</span></p>\n<span class=\"shared_location\"><hr>Stopped sharing location!</span>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_attachment_macos() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();

        let actual = exporter
            .format_attachment(&mut attachment, &message, &AttachmentMeta::default())
            .unwrap();

        assert_eq!(actual, "<img src=\"a/b/c/d.jpg\" loading=\"lazy\">");
    }

    #[test]
    fn can_format_html_attachment_macos_invalid() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.filename = None;

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(actual, Err("d.jpg"));
    }

    #[test]
    fn can_format_html_attachment_ios() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.options.no_lazy = true;
        config.options.platform = Platform::iOS;
        let exporter = HTML::new(&config).unwrap();
        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();

        let actual = exporter
            .format_attachment(&mut attachment, &message, &AttachmentMeta::default())
            .unwrap();

        assert!(actual.ends_with("33/33c81da8ae3194fc5a0ea993ef6ffe0b048baedb\">"));
    }

    #[test]
    fn can_format_html_attachment_ios_invalid() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.filename = None;

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(actual, Err("d.jpg"));
    }

    #[test]
    fn can_format_html_attachment_sticker() {
        // Create exporter
        let mut options = Options::fake_options(ExportType::Html);
        options.export_path = current_dir().unwrap().parent().unwrap().to_path_buf();

        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.rowid = 3;
        attachment.is_sticker = true;
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        attachment.filename = Some(sticker_path.to_string_lossy().to_string());
        attachment.copied_path = Some(PathBuf::from(sticker_path.to_string_lossy().to_string()));

        let actual = exporter.format_sticker(&mut attachment, &message);

        assert_eq!(actual, "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\">\n<div class=\"sticker_effect\">Sent with Outline effect</div>");

        // Remove the file created by the constructor for this test
        let orphaned_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("orphaned.html");
        let _ = std::fs::remove_file(orphaned_path);
    }

    #[test]
    fn can_format_html_attachment_sticker_genmoji() {
        // Create exporter
        let mut options = Options::fake_options(ExportType::Html);
        options.export_path = current_dir().unwrap().parent().unwrap().to_path_buf();

        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.rowid = 2;
        attachment.is_sticker = true;
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        attachment.filename = Some(sticker_path.to_string_lossy().to_string());
        attachment.copied_path = Some(PathBuf::from(sticker_path.to_string_lossy().to_string()));
        attachment.emoji_description = Some("pink poodle".to_string());

        let actual = exporter.format_sticker(&mut attachment, &message);

        assert_eq!(actual, "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\">\n<div class=\"genmoji_prompt\">Genmoji prompt: pink poodle</div>");

        // Remove the file created by the constructor for this test
        let orphaned_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("orphaned.html");
        let _ = std::fs::remove_file(orphaned_path);
    }

    #[test]
    fn can_format_html_attachment_sticker_app() {
        // Create exporter
        let mut options = Options::fake_options(ExportType::Html);
        options.export_path = current_dir().unwrap().parent().unwrap().to_path_buf();

        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.rowid = 1;
        attachment.is_sticker = true;
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        attachment.filename = Some(sticker_path.to_string_lossy().to_string());
        attachment.copied_path = Some(PathBuf::from(sticker_path.to_string_lossy().to_string()));

        let actual = exporter.format_sticker(&mut attachment, &message);

        assert_eq!(actual, "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\">\n<div class=\"sticker_name\">App: Free People</div>");

        // Remove the file created by the constructor for this test
        let orphaned_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("orphaned.html");
        let _ = std::fs::remove_file(orphaned_path);
    }

    #[test]
    fn can_format_html_attachment_audio_transcript() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.uti = Some("com.apple.coreaudio-format".to_string());
        attachment.transfer_name = Some("Audio Message.caf".to_string());
        attachment.filename = Some("Audio Message.caf".to_string());
        attachment.mime_type = None;

        let meta = AttachmentMeta::<'_> {
            transcription: Some("Test"),
            ..Default::default()
        };

        let actual = exporter
            .format_attachment(&mut attachment, &message, &meta)
            .unwrap();

        assert_eq!(actual, "<div><audio controls src=\"Audio Message.caf\" type=\"x-caf; codecs=opus\" </audio></div> <hr><span class=\"transcription\">Transcription: Test</span>");
    }
}

#[cfg(test)]
mod balloon_format_tests {
    use std::env::set_var;

    use crate::{exporters::exporter::BalloonFormatter, Config, Exporter, Options, HTML};
    use imessage_database::message_types::{
        app::AppMessage,
        app_store::AppStoreMessage,
        collaboration::CollaborationMessage,
        music::MusicMessage,
        placemark::{Placemark, PlacemarkMessage},
        url::URLMessage,
    };

    #[test]
    fn can_format_html_url() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = URLMessage {
            title: Some("title"),
            summary: Some("summary"),
            url: Some("url"),
            original_url: Some("original_url"),
            item_type: Some("item_type"),
            images: vec!["images"],
            icons: vec!["icons"],
            site_name: Some("site_name"),
            placeholder: false,
        };

        let expected =
            exporter.format_url(&Config::fake_message(), &balloon, &Config::fake_message());
        let actual = "<a href=\"url\"><div class=\"app_header\"><img src=\"images\" loading=\"lazy\", onerror=\"this.style.display='none'\"><div class=\"name\">site_name</div></div><div class=\"app_footer\"><div class=\"caption\">title</div><div class=\"subcaption\">summary</div></div></a>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_url_no_lazy() {
        // Create exporter
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.no_lazy = true;
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = URLMessage {
            title: Some("title"),
            summary: Some("summary"),
            url: Some("url"),
            original_url: Some("original_url"),
            item_type: Some("item_type"),
            images: vec!["images"],
            icons: vec!["icons"],
            site_name: Some("site_name"),
            placeholder: false,
        };

        let expected =
            exporter.format_url(&Config::fake_message(), &balloon, &Config::fake_message());
        let actual = "<a href=\"url\"><div class=\"app_header\"><img src=\"images\" onerror=\"this.style.display='none'\"><div class=\"name\">site_name</div></div><div class=\"app_footer\"><div class=\"caption\">title</div><div class=\"subcaption\">summary</div></div></a>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_music() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = MusicMessage {
            url: Some("url"),
            preview: Some("preview"),
            artist: Some("artist"),
            album: Some("album"),
            track_name: Some("track_name"),
        };

        let expected = exporter.format_music(&balloon, &Config::fake_message());
        let actual = "<div class=\"app_header\"><div class=\"name\">track_name</div><audio controls src=\"preview\" </audio></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">artist</div><div class=\"subcaption\">album</div></div></a>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_collaboration() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = CollaborationMessage {
            original_url: Some("original_url"),
            url: Some("url"),
            title: Some("title"),
            creation_date: Some(0.),
            bundle_id: Some("bundle_id"),
            app_name: Some("app_name"),
        };

        let expected = exporter.format_collaboration(&balloon, &Config::fake_message());
        let actual = "<div class=\"app_header\"><div class=\"name\">app_name</div></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">title</div><div class=\"subcaption\">url</div></div></a>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_apple_pay() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: Some("image"),
            url: Some("url"),
            title: Some("title"),
            subtitle: Some("subtitle"),
            caption: Some("caption"),
            subcaption: Some("subcaption"),
            trailing_caption: Some("trailing_caption"),
            trailing_subcaption: Some("trailing_subcaption"),
            app_name: Some("app_name"),
            ldtext: Some("ldtext"),
        };

        let expected = exporter.format_apple_pay(&balloon, &Config::fake_message());
        let actual = "<div class=\"app_header\"><div class=\"name\">app_name</div></div><div class=\"app_footer\"><div class=\"caption\">ldtext</div></div>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_fitness() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: Some("image"),
            url: Some("url"),
            title: Some("title"),
            subtitle: Some("subtitle"),
            caption: Some("caption"),
            subcaption: Some("subcaption"),
            trailing_caption: Some("trailing_caption"),
            trailing_subcaption: Some("trailing_subcaption"),
            app_name: Some("app_name"),
            ldtext: Some("ldtext"),
        };

        let expected = exporter.format_fitness(&balloon, &Config::fake_message());
        let actual = "<a href=\"url\"><div class=\"app_header\"><img src=\"image\"><div class=\"name\">app_name</div><div class=\"image_title\">title</div><div class=\"image_subtitle\">subtitle</div><div class=\"ldtext\">ldtext</div></div><div class=\"app_footer\"><div class=\"caption\">caption</div><div class=\"subcaption\">subcaption</div><div class=\"trailing_caption\">trailing_caption</div><div class=\"trailing_subcaption\">trailing_subcaption</div></div></a>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_slideshow() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: Some("image"),
            url: Some("url"),
            title: Some("title"),
            subtitle: Some("subtitle"),
            caption: Some("caption"),
            subcaption: Some("subcaption"),
            trailing_caption: Some("trailing_caption"),
            trailing_subcaption: Some("trailing_subcaption"),
            app_name: Some("app_name"),
            ldtext: Some("ldtext"),
        };

        let expected = exporter.format_slideshow(&balloon, &Config::fake_message());
        let actual = "<a href=\"url\"><div class=\"app_header\"><img src=\"image\"><div class=\"name\">app_name</div><div class=\"image_title\">title</div><div class=\"image_subtitle\">subtitle</div><div class=\"ldtext\">ldtext</div></div><div class=\"app_footer\"><div class=\"caption\">caption</div><div class=\"subcaption\">subcaption</div><div class=\"trailing_caption\">trailing_caption</div><div class=\"trailing_subcaption\">trailing_subcaption</div></div></a>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_find_my() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: Some("image"),
            url: Some("url"),
            title: Some("title"),
            subtitle: Some("subtitle"),
            caption: Some("caption"),
            subcaption: Some("subcaption"),
            trailing_caption: Some("trailing_caption"),
            trailing_subcaption: Some("trailing_subcaption"),
            app_name: Some("app_name"),
            ldtext: Some("ldtext"),
        };

        let expected = exporter.format_find_my(&balloon, &Config::fake_message());
        let actual = "<div class=\"app_header\"><div class=\"name\">app_name</div></div><div class=\"app_footer\"><div class=\"caption\">ldtext</div></div>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_check_in_timer() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Timer Started"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Timer Started"),
        };

        let expected = exporter.format_check_in(&balloon, &Config::fake_message());
        let actual = "<div class=\"app_header\"><div class=\"name\">Check\u{a0}In</div><div class=\"ldtext\">Check\u{a0}In: Timer Started</div></div><div class=\"app_footer\"><div class=\"caption\">Checked in at Oct 14, 2023  1:54:29 PM</div></div>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_check_in_timer_late() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Has not checked in when expected, location shared"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Has not checked in when expected, location shared"),
        };

        let expected = exporter.format_check_in(&balloon, &Config::fake_message());
        let actual = "<div class=\"app_header\"><div class=\"name\">Check\u{a0}In</div><div class=\"ldtext\">Check\u{a0}In: Has not checked in when expected, location shared</div></div><div class=\"app_footer\"><div class=\"caption\">Checked in at Oct 14, 2023  1:54:29 PM</div></div>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_accepted_check_in() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Fake Location"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Fake Location"),
        };

        let expected = exporter.format_check_in(&balloon, &Config::fake_message());
        let actual = "<div class=\"app_header\"><div class=\"name\">Check\u{a0}In</div><div class=\"ldtext\">Check\u{a0}In: Fake Location</div></div><div class=\"app_footer\"><div class=\"caption\">Checked in at Oct 14, 2023  1:54:29 PM</div></div>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_app_store() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppStoreMessage {
            url: Some("url"),
            app_name: Some("app_name"),
            original_url: Some("original_url"),
            description: Some("description"),
            platform: Some("platform"),
            genre: Some("genre"),
        };

        let expected = exporter.format_app_store(&balloon, &Config::fake_message());
        let actual = "<div class=\"app_header\"><div class=\"name\">app_name</div></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">description</div><div class=\"subcaption\">platform</div><div class=\"trailing_subcaption\">genre</div></div></a>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_placemark() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = PlacemarkMessage {
            url: Some("url"),
            original_url: Some("original_url"),
            place_name: Some("Name"),
            placemark: Placemark {
                name: Some("name"),
                address: Some("address"),
                state: Some("state"),
                city: Some("city"),
                iso_country_code: Some("iso_country_code"),
                postal_code: Some("postal_code"),
                country: Some("country"),
                street: Some("street"),
                sub_administrative_area: Some("sub_administrative_area"),
                sub_locality: Some("sub_locality"),
            },
        };

        let expected = exporter.format_placemark(&balloon, &Config::fake_message());
        let actual = "<a href=\"url\"><div class=\"app_header\"><div class=\"name\">Name</div></div><div class=\"app_footer\"><div class=\"caption\">address</div><div class=\"trailing_caption\">postal_code</div><div class=\"subcaption\">country</div><div class=\"trailing_subcaption\">sub_administrative_area</div></div></a>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_generic_app() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: Some("image"),
            url: Some("url"),
            title: Some("title"),
            subtitle: Some("subtitle"),
            caption: Some("caption"),
            subcaption: Some("subcaption"),
            trailing_caption: Some("trailing_caption"),
            trailing_subcaption: Some("trailing_subcaption"),
            app_name: Some("app_name"),
            ldtext: Some("ldtext"),
        };

        let expected = exporter.format_generic_app(
            &balloon,
            "bundle_id",
            &mut vec![],
            &Config::fake_message(),
        );
        let actual = "<a href=\"url\"><div class=\"app_header\"><img src=\"image\"><div class=\"name\">app_name</div><div class=\"image_title\">title</div><div class=\"image_subtitle\">subtitle</div><div class=\"ldtext\">ldtext</div></div><div class=\"app_footer\"><div class=\"caption\">caption</div><div class=\"subcaption\">subcaption</div><div class=\"trailing_caption\">trailing_caption</div><div class=\"trailing_subcaption\">trailing_subcaption</div></div></a>";

        assert_eq!(expected, actual);
    }
}

#[cfg(test)]
mod text_effect_tests {
    use crate::{
        exporters::exporter::{TextEffectFormatter, Writer},
        Config, Exporter, Options, HTML,
    };
    use imessage_database::{
        message_types::text_effects::{Style, TextEffect, Unit},
        util::typedstream::parser::TypedStreamReader,
    };
    use std::{
        env::{current_dir, set_var},
        fs::File,
        io::Read,
    };

    #[test]
    fn can_format_html_default() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let expected = exporter.format_attributed("Chris", &TextEffect::Default);
        let actual = "Chris";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_mention() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let expected = exporter.format_mention("Chris", "+15558675309");
        let actual = "<span title=\"+15558675309\"><b>Chris</b></span>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_link() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let expected = exporter.format_link("chrissardegna.com", "https://chrissardegna.com");
        let actual = "<a href=\"https://chrissardegna.com\">chrissardegna.com</a>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_otp() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let expected = exporter.format_otp("123456");
        let actual = "<u>123456</u>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_style_single() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let expected = exporter.format_styles("Bold", &[Style::Bold]);
        let actual = "<b>Bold</b>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_style_multiple() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let expected = exporter.format_styles("Bold", &[Style::Bold, Style::Strikethrough]);
        let actual = "<s><b>Bold</b></s>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_style_all() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let expected = exporter.format_styles(
            "Bold",
            &[
                Style::Bold,
                Style::Strikethrough,
                Style::Italic,
                Style::Underline,
            ],
        );
        let actual = "<u><i><s><b>Bold</b></s></i></u>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_conversion() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let expected = exporter.format_conversion("100 Miles", &Unit::Distance);
        let actual = "<u>100 Miles</u>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_mention_end_to_end() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Test Dad ".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/Mention");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Test <span title=\"+15558675309\"><b>Dad</b></span> </span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_otp_end_to_end() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("000123 is your security code. Don't share your code.".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/Code");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><u>000123</u> is your security code. Don&apos;t share your code.</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_link_end_to_end() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("https://twitter.com/xxxxxxxxx/status/0000223300009216128".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/URLMessage");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><a href=\"https://twitter.com/xxxxxxxxx/status/0000223300009216128\">https://twitter.com/xxxxxxxxx/status/0000223300009216128</a></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_end_to_end() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hi. Right now or tomorrow?".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/Date");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hi. Right now or <u>tomorrow</u>?</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_effect_end_to_end() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Big small shake nod explode ripple bloom jitter".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/TextEffects");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><span class=\"animationBig\">Big</span> <span class=\"animationSmall\">small </span><span class=\"animationShake\">shake</span> <span class=\"animationNod\">nod</span> <span class=\"animationExplode\">explode </span><span class=\"animationRipple\">ripple</span> <span class=\"animationBloom\">bloom</span> <span class=\"animationJitter\">jitter</span></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styles_end_to_end() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Bold underline italic strikethrough all four".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/TextStyles");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><b>Bold</b> <u>underline</u> <i>italic</i> <s>strikethrough</s> all <i><u><s><b>four</b></s></u></i></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styles_single_end_to_end() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Everything".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/TextStylesSingleRange");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><i><u><s><b>Everything</b></s></u></i></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styles_mixed_end_to_end() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Underline normal jitter normal".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/TextStylesMixed");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><u>Underline</u> normal <span class=\"animationJitter\">jitter</span> normal</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }
}

#[cfg(test)]
mod edited_tests {
    use std::{
        env::{current_dir, set_var},
        fs::File,
        io::Read,
    };

    use crate::{exporters::exporter::Writer, Config, Exporter, Options, HTML};
    use imessage_database::{
        message_types::edited::{EditStatus, EditedMessage, EditedMessagePart},
        util::typedstream::parser::TypedStreamReader,
    };

    #[test]
    fn can_format_html_conversion_final_unsent() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.date_edited = 674530231992568192;
        message.text = Some(
            "From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r".to_string(),
        );
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                },
            ],
        });

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/MultiPartWithDeleted");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">From arbitrary byte stream:\r</span>\n</div>\n<hr><div class=\"message_part\">\n<span class=\"attachment_error\">Attachment does not exist!</span>\n</div>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">To native Rust data structures:\r</span>\n</div>\n<hr><div class=\"message_part\">\n<span class=\"unsent\"><span class=\"unsent\">You unsent this message part 1 hour, 49 seconds after sending!</span></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_no_edits() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some(
            "From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r".to_string(),
        );
        message.is_from_me = true;
        message.chat_id = Some(0);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/MultiPartWithDeleted");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">From arbitrary byte stream:\r</span>\n</div>\n<hr><div class=\"message_part\">\n<span class=\"attachment_error\">Attachment does not exist!</span>\n</div>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">To native Rust data structures:\r</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_fully_unsent() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.date_edited = 674530231992568192;
        message.text = None;
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
            }],
        });

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/Blank");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamReader::from(&bytes);
        message.components = parser.parse().ok();

        let actual = exporter.format_announcement(&message);
        let expected = "<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You unsent a message.</p></div>";

        assert_eq!(actual, expected);
    }
}
