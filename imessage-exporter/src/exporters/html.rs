use std::{
    borrow::Cow,
    cmp::{
        Ordering::{Equal, Greater, Less},
        min,
    },
    collections::{
        HashMap,
        hash_map::Entry::{Occupied, Vacant},
    },
    fs::File,
    io::{BufWriter, Write},
};

use crate::{
    app::{
        compatibility::attachment_manager::AttachmentManagerMode, error::RuntimeError,
        progress::ExportProgress, runtime::Config, sanitizers::sanitize_html,
    },
    exporters::exporter::{
        ATTACHMENT_NO_FILENAME, BalloonFormatter, Exporter, TextEffectFormatter, Writer,
    },
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
        variants::{
            Announcement, BalloonProvider, CustomBalloon, Tapback, TapbackAction, URLOverride,
            Variant,
        },
    },
    tables::{
        attachment::{Attachment, MediaType},
        messages::{
            Message,
            models::{AttachmentMeta, BubbleComponent, GroupAction, TextAttributes},
        },
        table::{FITNESS_RECEIVER, ME, ORPHANED, Table, YOU},
    },
    util::{
        dates::{TIMESTAMP_FACTOR, format, get_local_time, readable_diff},
        plist::parse_ns_keyed_archiver,
    },
};

const HEADER: &str = "<html>\n<head>\n<meta charset=\"UTF-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">";
const FOOTER: &str = "</body></html>";
const STYLE: &str = include_str!("resources/style.css");

#[derive(Debug, Clone)]
/// [`EventType`] is used to track the start and end of HTML text attributes
/// so we can render them correctly in the HTML output.
enum EventType<'a> {
    /// Start event for text attributes, contains the index of the text part
    Start(usize, &'a [TextEffect]),
    /// End event for text attributes, contains the index of the text part
    End(usize),
}

pub struct HTML<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config,
    /// Handles to files we want to write messages to
    /// Map of resolved chatroom file location to a buffered writer
    pub files: HashMap<String, BufWriter<File>>,
    /// Writer instance for orphaned messages
    pub orphaned: BufWriter<File>,
    /// Progress Bar model for alerting the user about current export state
    pb: ExportProgress,
}

impl<'a> Exporter<'a> for HTML<'a> {
    fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        let mut orphaned = config.options.export_path.clone();
        orphaned.push(ORPHANED);
        orphaned.set_extension("html");
        let file = File::options().append(true).create(true).open(&orphaned)?;

        Ok(HTML {
            config,
            files: HashMap::new(),
            orphaned: BufWriter::new(file),
            pb: ExportProgress::new(),
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
            Message::get_count(self.config.db(), &self.config.options.query_context)?;
        self.pb.start(total_messages);

        let mut statement =
            Message::stream_rows(self.config.db(), &self.config.options.query_context)?;

        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .map_err(|err| RuntimeError::DatabaseError(TableError::QueryError(err)))?;

        for message in messages {
            let mut msg = Message::extract(message)?;

            // Early escape if we try and render the same message GUID twice
            // See https://github.com/ReagentX/imessage-exporter/issues/135 for rationale
            if msg.rowid == current_message_row {
                current_message += 1;
                continue;
            }
            current_message_row = msg.rowid;

            // Generate the text of the message
            let _ = msg.generate_text(self.config.db());

            // Render the announcement in-line
            if msg.is_announcement() {
                let announcement = self.format_announcement(&msg);
                HTML::write_to_file(self.get_or_create_file(&msg)?, &announcement)?;
            }
            // Message replies and tapbacks are rendered in context, so no need to render them separately
            else if !msg.is_tapback() {
                let message = self.format_message(&msg, 0)?;
                HTML::write_to_file(self.get_or_create_file(&msg)?, &message)?;
            }
            current_message += 1;
            if current_message % 99 == 0 {
                self.pb.set_position(current_message);
            }
        }
        self.pb.finish();

        eprintln!("Writing HTML footers...");
        for buf in self.files.values_mut() {
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

                        let file = File::options().append(true).create(true).open(&path)?;

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
        let (date, read_after) = self.get_time(message);
        let linked_time = format!(
            "<a title=\"Reveal in Messages app\" href=\"sms://open?message-guid={}\">{date}</a>",
            message.guid
        );
        self.add_line(
            &mut formatted_message,
            &format!("{linked_time} {read_after}"),
            "<p><span class=\"timestamp\">",
            "</span>",
        );

        // Add reply anchor if necessary
        if message.is_reply() {
            if indent_size > 0 {
                // If we are indented it means we are rendering in a thread
                self.add_line(
                    &mut formatted_message,
                    &format!(
                        "<a title=\"View in context\" href=\"#r-{}\">⇲</a>",
                        message.guid
                    ),
                    "<span class=\"reply_anchor\">",
                    "</span>",
                );
            } else {
                // If there is no ident we are rendering a top-level message
                self.add_line(
                    &mut formatted_message,
                    &format!(
                        "<a title=\"View in thread\" href=\"#{}\">⇱</a>",
                        message.guid
                    ),
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
        let message_parts = &message.components;
        let mut attachments = Attachment::from_message(self.config.db(), message)?;
        let mut replies = message.get_replies(self.config.db())?;

        // Index of where we are in the attachment Vector
        let mut attachment_index: usize = 0;

        // Add message subject
        if let Some(subject) = &message.subject {
            // Add message subject
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
                                }
                            }
                        } else {
                            let mut formatted_text = self.format_attributes(text, text_attrs);

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
                        }
                    }
                }
            }

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
                        let _ = reply.generate_text(self.config.db());
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
        // When encoding videos, alert the user that the time estimate may be inaccurate
        let will_encode = matches!(attachment.mime_type(), MediaType::Video(_))
            && matches!(
                self.config.options.attachment_manager.mode,
                AttachmentManagerMode::Full
            );

        if will_encode {
            self.pb
                .set_busy_style("Encoding video, estimates paused...".to_string());
        }

        // Copy the file, if requested
        self.config
            .options
            .attachment_manager
            .handle_attachment(message, attachment, self.config)
            .ok_or(attachment.filename().ok_or(ATTACHMENT_NO_FILENAME)?)?;

        if will_encode {
            self.pb.set_default_style();
        }

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
                format!(
                    "<video controls> <source src=\"{embed_path}\" type=\"{media_type}\"> <source src=\"{embed_path}\"> </video>"
                )
            }
            MediaType::Audio(media_type) => {
                if let Some(transcription) = &metadata.transcription {
                    return Ok(format!(
                        "<div><audio controls src=\"{embed_path}\" type=\"{media_type}\" </audio></div> <hr><span class=\"transcription\">Transcription: {transcription}</span>"
                    ));
                }
                format!("<audio controls src=\"{embed_path}\" type=\"{media_type}\" </audio>")
            }
            MediaType::Text(_) => {
                format!(
                    "<a href=\"{embed_path}\">Click to download {} ({})</a>",
                    attachment.filename().ok_or(ATTACHMENT_NO_FILENAME)?,
                    attachment.file_size()
                )
            }
            MediaType::Application(_) => format!(
                "<a href=\"{embed_path}\">Click to download {} ({})</a>",
                attachment.filename().ok_or(ATTACHMENT_NO_FILENAME)?,
                attachment.file_size()
            ),
            MediaType::Unknown => {
                if attachment
                    .copied_path
                    .as_ref()
                    .is_some_and(|path| path.is_dir())
                {
                    format!(
                        "<p>Folder: <i>{}</i> ({}) <a href=\"{embed_path}\">Click to open</a></p>",
                        attachment.filename().ok_or(ATTACHMENT_NO_FILENAME)?,
                        attachment.file_size()
                    )
                } else {
                    format!(
                        "<p>Unknown attachment type: {embed_path}</p> <a href=\"{embed_path}\">Download ({})</a>",
                        attachment.file_size()
                    )
                }
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
                if let Some(sticker_source) = sticker.get_sticker_source(self.config.db()) {
                    match sticker_source {
                        StickerSource::Genmoji => {
                            // Add sticker prompt
                            if let Some(prompt) = &sticker.emoji_description {
                                sticker_embed.push_str(&format!(
                                    "\n<div class=\"genmoji_prompt\">Genmoji prompt: {prompt}</div>"
                                ));
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
                                ));
                            }
                        }
                        StickerSource::App(bundle_id) => {
                            // Add the application name used to generate/send the sticker
                            let app_name = sticker
                                .get_sticker_source_application_name(self.config.db())
                                .unwrap_or(bundle_id);
                            sticker_embed.push_str(&format!(
                                "\n<div class=\"sticker_name\">App: {app_name}</div>"
                            ));
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
                if let Some(payload) = message.raw_payload_data(self.config.db()) {
                    return match HandwrittenMessage::from_payload(&payload) {
                        Ok(bubble) => Ok(self.format_handwriting(message, &bubble, message)),
                        Err(why) => Err(PlistParseError::HandwritingError(why)),
                    };
                }
            }

            if message.is_digital_touch() {
                if let Some(payload) = message.raw_payload_data(self.config.db()) {
                    return match digital_touch::from_payload(&payload) {
                        Some(bubble) => Ok(self.format_digital_touch(message, &bubble, message)),
                        None => Err(PlistParseError::DigitalTouchError),
                    };
                }
            }

            if let Some(payload) = message.payload_data(self.config.db()) {
                let parsed = parse_ns_keyed_archiver(&payload)?;

                let res = if message.is_url() {
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
            Variant::Tapback(_, action, tapback) => {
                if let TapbackAction::Removed = action {
                    return Ok(String::new());
                }
                match tapback {
                    Tapback::Sticker => {
                        let mut paths = Attachment::from_message(self.config.db(), msg)?;
                        let who = self.config.who(
                            msg.handle_id,
                            msg.is_from_me(),
                            &msg.destination_caller_id,
                        );
                        // Sticker messages have only one attachment, the sticker image
                        Ok(match paths.get_mut(0) {
                            Some(sticker) => format!(
                                "{} <div class=\"sticker_tapback\">&nbsp;by {who}</div>",
                                self.format_sticker(sticker, msg)
                            ),
                            None => {
                                format!(
                                    "<span class=\"tapback\">Sticker from {who} not found!</span>"
                                )
                            }
                        })
                    }
                    _ => Ok(format!(
                        "<span class=\"tapback\"><b>{}</b> by {}</span>",
                        tapback,
                        self.config.who(
                            msg.handle_id,
                            msg.is_from_me(),
                            &msg.destination_caller_id
                        ),
                    )),
                }
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
            Some(announcement) => {
                let action_text = match &announcement {
                    Announcement::GroupAction(action) => match action {
                        GroupAction::ParticipantAdded(person)
                        | GroupAction::ParticipantRemoved(person) => {
                            let resolved_person =
                                self.config
                                    .who(Some(*person), false, &msg.destination_caller_id);
                            let action_word = if matches!(action, GroupAction::ParticipantAdded(_))
                            {
                                "added"
                            } else {
                                "removed"
                            };
                            format!(
                                "{action_word} {resolved_person} {} the conversation.",
                                if matches!(action, GroupAction::ParticipantAdded(_)) {
                                    "to"
                                } else {
                                    "from"
                                }
                            )
                        }
                        GroupAction::NameChange(name) => {
                            let clean_name = sanitize_html(name);
                            format!("named the conversation <b>{clean_name}</b>")
                        }
                        GroupAction::ParticipantLeft => "left the conversation.".to_string(),
                        GroupAction::GroupIconChanged => "changed the group photo.".to_string(),
                        GroupAction::GroupIconRemoved => "removed the group photo.".to_string(),
                    },
                    Announcement::AudioMessageKept => "kept an audio message.".to_string(),
                    Announcement::FullyUnsent => "unsent a message.".to_string(),
                    Announcement::Unknown(num) => format!("performed unknown action {num}"),
                };

                let newlines = if matches!(announcement, Announcement::FullyUnsent) {
                    ""
                } else {
                    "\n"
                };

                format!(
                    "{newlines}<div class =\"announcement\"><p><span class=\"timestamp\">{timestamp}</span> {who} {action_text}</p></div>{newlines}"
                )
            }
            None => String::from(
                "\n<div class =\"announcement\"><p>Unable to format announcement!</p></div>\n",
            ),
        }
    }

    fn format_shareplay(&self) -> &'static str {
        "<hr>SharePlay Message Ended"
    }

    fn format_shared_location(&self, msg: &'a Message) -> &'static str {
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
                        if let Some(text) = &event.text {
                            let clean_text = if let Some(BubbleComponent::Text(attributes)) =
                                event.components.first()
                            {
                                Cow::Owned(self.format_attributes(text, attributes))
                            } else {
                                sanitize_html(text)
                            };

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
                            ));
                        }
                        None => {
                            out_s.push_str(&format!(
                                "<span class=\"unsent\">{who} unsent this message part!</span>"
                            ));
                        }
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

    fn format_attributes(&'a self, text: &'a str, attributes: &'a [TextAttributes]) -> String {
        if attributes.is_empty() {
            return text.to_string();
        }

        // Create events for attribute starts and ends
        let mut events = Vec::new();

        // Create events for each attribute, marking start and end positions. The ID is the index of the attribute in the list.
        for (attr_id, attr) in attributes.iter().enumerate() {
            events.push((attr.start, EventType::Start(attr_id, &attr.effects)));
            events.push((attr.end, EventType::End(attr_id)));
        }

        // Sort events by position, with ends before starts at the same position
        events.sort_by(|a, b| {
            a.0.cmp(&b.0).then_with(|| match (&a.1, &b.1) {
                (EventType::End(_), EventType::Start(_, _)) => Less,
                (EventType::Start(_, _), EventType::End(_)) => Greater,
                _ => Equal,
            })
        });

        let mut result = String::new();
        // The currently active attributes, stored as (attribute ID, TextAttributes)
        let mut active_attrs = Vec::new();
        let mut last_pos = events.first().map_or(0, |(pos, _)| *pos);

        for (pos, event) in events {
            // Add text before this event with current active attributes
            if pos > last_pos && last_pos < text.len() {
                // Get the text slice from last position to current position
                let end_pos = min(pos, text.len());
                let text_slice = &text[last_pos..end_pos];
                // Sanitize the text slice
                let sanitized_text = sanitize_html(text_slice);
                result.push_str(&self.apply_active_attributes(&sanitized_text, &active_attrs));
            }

            // Update active attributes based on the event
            match event {
                EventType::Start(attr_id, attr) => {
                    // Add the attribute that starts
                    active_attrs.push((attr_id, attr));
                }
                EventType::End(attr_id) => {
                    // Remove the attribute that ends
                    active_attrs.retain(|(id, _)| *id != attr_id);
                }
            }

            last_pos = pos;
        }
        result
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

        // Add lyrics, if any
        if let Some(lyrics) = &balloon.lyrics {
            out_s.push_str("<div class=\"ldtext\">");
            for line in lyrics {
                out_s.push_str("<p>");
                out_s.push_str(line);
                out_s.push_str("</p>");
            }
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
            "<div class=\"app_header\"><div class=\"name\">Digital Touch Message</div></div>\n<div class=\"app_footer\"><div class=\"caption\">{balloon:?}</div></div>"
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

impl<'a> TextEffectFormatter<'a> for HTML<'a> {
    fn format_effect(&'a self, text: &'a str, effect: &'a TextEffect) -> Cow<'a, str> {
        match effect {
            TextEffect::Default => Cow::Borrowed(text),
            TextEffect::Mention(mentioned) => Cow::Owned(self.format_mention(text, mentioned)),
            TextEffect::Link(url) => Cow::Owned(self.format_link(text, url)),
            TextEffect::OTP => Cow::Owned(self.format_otp(text)),
            TextEffect::Styles(styles) => Cow::Owned(self.format_styles(text, styles)),
            TextEffect::Animated(animation) => Cow::Owned(self.format_animated(text, animation)),
            TextEffect::Conversion(unit) => Cow::Owned(self.format_conversion(text, unit)),
        }
    }

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
    fn get_time(&self, message: &Message) -> (String, String) {
        let date = format(&message.date(&self.config.offset));
        let mut read_at = String::new();
        let read_after = message.time_until_read(&self.config.offset);
        if let Some(time) = read_after {
            if !time.is_empty() {
                let who = if message.is_from_me() {
                    "them"
                } else {
                    self.config.options.custom_name.as_deref().unwrap_or("you")
                };
                read_at = format!("(Read by {who} after {time})");
            }
        }
        (date, read_at)
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
        HTML::write_to_file(file, "<link rel=\"stylesheet\" href=\"style.css\">")?;
        HTML::write_to_file(file, "\n</head>\n<body>\n")?;
        Ok(())
    }

    fn edited_to_html(&self, timestamp: &str, text: &str, last: bool) -> String {
        let tag = if last { "tfoot" } else { "tbody" };
        format!(
            "<{tag}><tr><td><span class=\"timestamp\">{timestamp}</span></td><td>{text}</td></tr></{tag}>"
        )
    }

    fn apply_active_attributes<'a>(
        &'a self,
        text: &'a str,
        active_attrs: &'a [(usize, &[TextEffect])],
    ) -> Cow<'a, str> {
        // If there are no active attributes, return the original text
        if active_attrs.is_empty() {
            return Cow::Borrowed(text);
        }

        // If there are active attributes, we need to format the text
        let mut result = Cow::Borrowed(text);

        // Iterate through the active attributes and apply their effects
        // If we encounter a TextEffect that modifies the text, we will convert it to an owned type
        // to ensure we can modify it.
        for (_, effects) in active_attrs {
            for effect in *effects {
                // If the effect is `Default`, we can skip it, because it does not modify the text
                if !matches!(effect, TextEffect::Default) {
                    // Once we need to modify, convert to owned and stay owned
                    let owned_text = result.into_owned();
                    let formatted = self.format_effect(&owned_text, effect);
                    result = Cow::Owned(formatted.into_owned());
                }
            }
        }

        result
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
    use std::{env::current_dir, path::PathBuf};

    use crate::{
        Config, Exporter, HTML, Options,
        app::{compatibility::attachment_manager::AttachmentManagerMode, export_type::ExportType},
        exporters::exporter::Writer,
    };
    use imessage_database::{
        message_types::text_effects::TextEffect,
        tables::{
            messages::models::{AttachmentMeta, BubbleComponent, TextAttributes},
            table::ME,
        },
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
            (
                "May 17, 2022  5:29:42 PM".to_string(),
                "(Read by you after 1 hour, 49 seconds)".to_string()
            )
        );
    }

    #[test]
    fn can_get_time_invalid() {
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
        assert_eq!(
            exporter.get_time(&message),
            ("May 17, 2022  6:30:31 PM".to_string(), String::new())
        );
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
        message.generate_text_legacy(config.db()).unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_message_with_html() {
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
        message.generate_text_legacy(config.db()).unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">&lt;table&gt;&lt;/table&gt;</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_me_normal_deleted() {
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
        message.generate_text_legacy(config.db()).unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<span class=\"deleted\">This message was deleted from the conversation!</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_me_normal_read() {
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
        message.generate_text_legacy(config.db()).unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> (Read by them after 1 hour, 49 seconds)</span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_them_normal() {
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
        message.generate_text_legacy(config.db()).unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Sample Contact</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_them_normal_read() {
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
        message.generate_text_legacy(config.db()).unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> (Read by you after 1 hour, 49 seconds)</span>\n<span class=\"sender\">Sample Contact</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_them_custom_name_read() {
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
        message.generate_text_legacy(config.db()).unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> (Read by Name after 1 hour, 49 seconds)</span>\n<span class=\"sender\">Sample Contact</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hello world</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_shareplay() {
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
        let expected = "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<span class=\"shareplay\"><hr>SharePlay Message Ended</span>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_announcement() {
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
        message.item_type = 2;

        let actual = exporter.format_announcement(&message);
        let expected = "\n<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You named the conversation <b>Hello world</b></p></div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_announcement_custom_name() {
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
        message.item_type = 2;

        let actual = exporter.format_announcement(&message);
        let expected = "\n<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Name named the conversation <b>Hello world</b></p></div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_removed() {
        // Create exporter
        let options = Options::fake_options(ExportType::Txt);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, ME.to_string());
        config.participants.insert(1, "Other".to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 1;
        message.group_action_type = 1;
        message.other_handle = Some(1);

        let actual = exporter.format_announcement(&message);
        let expected = "\n<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You removed Other from the conversation.</p></div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_added() {
        // Create exporter
        let options = Options::fake_options(ExportType::Txt);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, ME.to_string());
        config.participants.insert(1, "Other".to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 1;
        message.group_action_type = 0;
        message.other_handle = Some(1);

        let actual = exporter.format_announcement(&message);
        let expected = "\n<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You added Other to the conversation.</p></div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_left() {
        // Create exporter
        let options = Options::fake_options(ExportType::Txt);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, ME.to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;

        let actual = exporter.format_announcement(&message);
        let expected = "\n<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You left the conversation.</p></div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_icon_removed() {
        // Create exporter
        let options = Options::fake_options(ExportType::Txt);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, ME.to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;
        message.group_action_type = 2;

        let actual = exporter.format_announcement(&message);
        let expected = "\n<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You removed the group photo.</p></div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_icon_added() {
        // Create exporter
        let options = Options::fake_options(ExportType::Txt);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, ME.to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;
        message.group_action_type = 1;

        let actual = exporter.format_announcement(&message);
        let expected = "\n<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You changed the group photo.</p></div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_audio_message_kept() {
        // Create exporter
        let options = Options::fake_options(ExportType::Txt);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, ME.to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.is_from_me = true;
        message.item_type = 5;

        let actual = exporter.format_announcement(&message);
        let expected = "\n<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You kept an audio message.</p></div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_me() {
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
        message.num_attachments = 1;

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = "<span class=\"tapback\">Sticker from Sample Contact not found!</span>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_custom_sticker_exists() {
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
        message.num_attachments = 1;
        message.rowid = 452567;

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = "<img src=\"/Users/chris/Library/Messages/StickerCache/8e682c381ab52ec2-289D9E83-33EE-4153-AF13-43DB31792C6F/289D9E83-33EE-4153-AF13-43DB31792C6F.heic\" loading=\"lazy\">\n<div class=\"sticker_name\">App: Free People</div> <div class=\"sticker_tapback\">&nbsp;by Sample Contact</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_custom_sticker_removed() {
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
        message.associated_message_type = Some(3007);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);
        message.num_attachments = 1;
        message.rowid = 452567;

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = "";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_started_sharing_location_me() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.is_from_me = false;
        message.other_handle = Some(2);
        message.share_status = false;
        message.share_direction = Some(false);
        message.item_type = 4;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  4:00:00 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<span class=\"shared_location\"><hr>Started sharing location!</span>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_stopped_sharing_location_me() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.is_from_me = false;
        message.other_handle = Some(2);
        message.share_status = true;
        message.share_direction = Some(false);
        message.item_type = 4;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  4:00:00 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<span class=\"shared_location\"><hr>Stopped sharing location!</span>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_started_sharing_location_them() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.handle_id = None;
        message.is_from_me = false;
        message.other_handle = Some(0);
        message.share_status = false;
        message.share_direction = Some(false);
        message.item_type = 4;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  4:00:00 PM</a> </span>\n<span class=\"sender\">Unknown</span></p>\n<span class=\"shared_location\"><hr>Started sharing location!</span>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_stopped_sharing_location_them() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.handle_id = None;
        message.is_from_me = false;
        message.other_handle = Some(0);
        message.share_status = true;
        message.share_direction = Some(false);
        message.item_type = 4;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  4:00:00 PM</a> </span>\n<span class=\"sender\">Unknown</span></p>\n<span class=\"shared_location\"><hr>Stopped sharing location!</span>\n</div>\n</div>\n";

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
    fn can_format_html_attachment_macos_invalid_disabled() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.filename = None;
        attachment.transfer_name = None;

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(actual, Err("Attachment missing name metadata!"));
    }

    #[test]
    fn can_format_html_attachment_macos_invalid_clone() {
        // Create exporter
        let mut options = Options::fake_options(ExportType::Html);
        options.attachment_manager.mode = AttachmentManagerMode::Clone;

        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.filename = None;
        attachment.transfer_name = None;

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(actual, Err("Attachment missing name metadata!"));
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
    fn can_format_html_attachment_ios_invalid_disabled() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.filename = None;
        attachment.transfer_name = None;

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(actual, Err("Attachment missing name metadata!"));
    }

    #[test]
    fn can_format_html_attachment_ios_invalid_clone() {
        // Create exporter
        let mut options = Options::fake_options(ExportType::Html);
        options.attachment_manager.mode = AttachmentManagerMode::Clone;

        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.filename = None;
        attachment.transfer_name = None;

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(actual, Err("Attachment missing name metadata!"));
    }

    #[test]
    fn can_format_html_attachment_folder() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        let folder_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/");
        attachment.mime_type = None;
        attachment.transfer_name = Some("test_data".to_string());
        attachment.copied_path = Some(folder_path);

        let actual = exporter
            .format_attachment(&mut attachment, &message, &AttachmentMeta::default())
            .unwrap();

        assert!(actual.starts_with("<p>Folder: <i>test_data</i> (100.00 B) <a href="));
    }

    #[test]
    fn can_format_html_attachment_unknown() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        let folder_path = "Fake";
        attachment.mime_type = None;
        attachment.transfer_name = Some("test_data".to_string());
        attachment.copied_path = Some(PathBuf::from(folder_path));

        let actual = exporter
            .format_attachment(&mut attachment, &message, &AttachmentMeta::default())
            .unwrap();

        assert_eq!(
            actual,
            "<p>Unknown attachment type: Fake</p> <a href=\"Fake\">Download (100.00 B)</a>"
        );
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
        attachment.copied_path = Some(sticker_path);

        let actual = exporter.format_sticker(&mut attachment, &message);

        assert_eq!(
            actual,
            "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\">\n<div class=\"sticker_effect\">Sent with Outline effect</div>"
        );

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
        attachment.copied_path = Some(sticker_path);
        attachment.emoji_description = Some("pink poodle".to_string());

        let actual = exporter.format_sticker(&mut attachment, &message);

        assert_eq!(
            actual,
            "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\">\n<div class=\"genmoji_prompt\">Genmoji prompt: pink poodle</div>"
        );

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
        attachment.copied_path = Some(sticker_path);

        let actual = exporter.format_sticker(&mut attachment, &message);

        assert_eq!(
            actual,
            "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\">\n<div class=\"sticker_name\">App: Free People</div>"
        );

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

        let meta = AttachmentMeta {
            transcription: Some("Test".to_string()),
            ..Default::default()
        };

        let actual = exporter
            .format_attachment(&mut attachment, &message, &meta)
            .unwrap();

        assert_eq!(
            actual,
            "<div><audio controls src=\"Audio Message.caf\" type=\"x-caf; codecs=opus\" </audio></div> <hr><span class=\"transcription\">Transcription: Test</span>"
        );
    }

    #[test]
    fn can_format_html_single_url_no_bundle_id() {
        // Create exporter
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();

        // Use test message payload from test database
        message.guid = "FAKEGUID-D0C8-4212-AA87-DD8AE4FD1203".to_string();
        message.rowid = 123445;

        message.date = 674526582885055488;
        // Set the message components to a single url
        message.text = Some("https://example.com".to_string());
        message.components = vec![BubbleComponent::Text(vec![
                TextAttributes::new(
                    0,
                    84,
                    vec![
                        TextEffect::Link("https://www.ghacks.net/2020/01/23/lastpass-no-longer-listed-on-the-chrome-web-store/".to_string()),
                    ]
                ),
            ]),];
        let _ = message.generate_text(config.db());

        let actual = exporter.format_message(&message, 0).unwrap();

        assert_eq!(
            actual,
            "<div class=\"message\">\n<div class=\"received\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=FAKEGUID-D0C8-4212-AA87-DD8AE4FD1203\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Unknown</span></p>\n<hr><div class=\"message_part\">\n<div class=\"app\"><a href=\"https://www.ghacks.net/2020/01/23/lastpass-no-longer-listed-on-the-chrome-web-store/\"><div class=\"app_header\"><img src=\"https://www.ghacks.net/wp-content/uploads/2020/01/lastpass-chrome-extension.png\" loading=\"lazy\", onerror=\"this.style.display='none'\"><div class=\"name\">gHacks Technology News</div></div><div class=\"app_footer\"><div class=\"caption\">LastPass no longer listed on the Chrome Web Store - gHacks Tech News</div><div class=\"subcaption\">LastPass customers and new users searching for password managers on Google&apos;s Chrome Web Store may have noticed that the LastPass extension for Google Chrome is currently no longer listed on the store.</div></div></a></div>\n</div>\n</div>\n</div>\n"
        );
    }
}

#[cfg(test)]
mod balloon_format_tests {
    use crate::{Config, Exporter, HTML, Options, exporters::exporter::BalloonFormatter};
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
            lyrics: None,
        };

        let expected = exporter.format_music(&balloon, &Config::fake_message());
        let actual = "<div class=\"app_header\"><div class=\"name\">track_name</div><audio controls src=\"preview\" </audio></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">artist</div><div class=\"subcaption\">album</div></div></a>";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_html_music_lyrics() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = MusicMessage {
            url: Some("url"),
            preview: None,
            artist: Some("artist"),
            album: Some("album"),
            track_name: Some("track_name"),
            lyrics: Some(vec!["a", "b"]),
        };

        let expected = exporter.format_music(&balloon, &Config::fake_message());
        let actual = "<div class=\"app_header\"><div class=\"name\">track_name</div><div class=\"ldtext\"><p>a</p><p>b</p></div></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">artist</div><div class=\"subcaption\">album</div></div></a>";

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
    use imessage_database::{
        message_types::text_effects::{Animation, Style, TextEffect, Unit},
        tables::messages::models::{BubbleComponent, TextAttributes},
    };

    use crate::{
        Config, Exporter, HTML, Options,
        exporters::exporter::{TextEffectFormatter, Writer},
    };

    #[test]
    fn can_format_html_default() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let expected = exporter.format_effect("Chris", &TextEffect::Default);
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

        message.components = vec![BubbleComponent::Text(vec![
            TextAttributes::new(0, 5, vec![TextEffect::Default]),
            TextAttributes::new(5, 8, vec![TextEffect::Mention("+15558675309".to_string())]),
            TextAttributes::new(8, 9, vec![TextEffect::Default]),
        ])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Test <span title=\"+15558675309\"><b>Dad</b></span> </span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_otp_end_to_end() {
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

        message.components = vec![BubbleComponent::Text(vec![
            TextAttributes::new(0, 6, vec![TextEffect::OTP]),
            TextAttributes::new(6, 52, vec![TextEffect::Default]),
        ])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><u>000123</u> is your security code. Don&apos;t share your code.</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_link_end_to_end() {
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

        message.components = vec![BubbleComponent::Text(vec![TextAttributes::new(
            0,
            56,
            vec![TextEffect::Link(
                "https://twitter.com/xxxxxxxxx/status/0000223300009216128".to_string(),
            )],
        )])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><a href=\"https://twitter.com/xxxxxxxxx/status/0000223300009216128\">https://twitter.com/xxxxxxxxx/status/0000223300009216128</a></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_end_to_end() {
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

        message.components = vec![BubbleComponent::Text(vec![
            TextAttributes::new(0, 17, vec![TextEffect::Default]),
            TextAttributes::new(17, 25, vec![TextEffect::Conversion(Unit::Timezone)]),
            TextAttributes::new(25, 26, vec![TextEffect::Default]),
        ])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">Hi. Right now or <u>tomorrow</u>?</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_effect_end_to_end() {
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

        message.components = vec![BubbleComponent::Text(vec![
            TextAttributes::new(0, 3, vec![TextEffect::Animated(Animation::Big)]),
            TextAttributes::new(3, 4, vec![TextEffect::Default]),
            TextAttributes::new(4, 10, vec![TextEffect::Animated(Animation::Small)]),
            TextAttributes::new(10, 15, vec![TextEffect::Animated(Animation::Shake)]),
            TextAttributes::new(15, 16, vec![TextEffect::Animated(Animation::Small)]),
            TextAttributes::new(16, 19, vec![TextEffect::Animated(Animation::Nod)]),
            TextAttributes::new(19, 20, vec![TextEffect::Animated(Animation::Small)]),
            TextAttributes::new(20, 28, vec![TextEffect::Animated(Animation::Explode)]),
            TextAttributes::new(28, 34, vec![TextEffect::Animated(Animation::Ripple)]),
            TextAttributes::new(34, 35, vec![TextEffect::Animated(Animation::Explode)]),
            TextAttributes::new(35, 40, vec![TextEffect::Animated(Animation::Bloom)]),
            TextAttributes::new(40, 41, vec![TextEffect::Animated(Animation::Explode)]),
            TextAttributes::new(41, 47, vec![TextEffect::Animated(Animation::Jitter)]),
        ])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><span class=\"animationBig\">Big</span> <span class=\"animationSmall\">small </span><span class=\"animationShake\">shake</span><span class=\"animationSmall\"> </span><span class=\"animationNod\">nod</span><span class=\"animationSmall\"> </span><span class=\"animationExplode\">explode </span><span class=\"animationRipple\">ripple</span><span class=\"animationExplode\"> </span><span class=\"animationBloom\">bloom</span><span class=\"animationExplode\"> </span><span class=\"animationJitter\">jitter</span></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styles_end_to_end() {
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

        message.components = vec![BubbleComponent::Text(vec![
            TextAttributes::new(0, 4, vec![TextEffect::Styles(vec![Style::Bold])]),
            TextAttributes::new(4, 5, vec![TextEffect::Default]),
            TextAttributes::new(5, 14, vec![TextEffect::Styles(vec![Style::Underline])]),
            TextAttributes::new(14, 15, vec![TextEffect::Default]),
            TextAttributes::new(15, 21, vec![TextEffect::Styles(vec![Style::Italic])]),
            TextAttributes::new(21, 22, vec![TextEffect::Default]),
            TextAttributes::new(22, 35, vec![TextEffect::Styles(vec![Style::Strikethrough])]),
            TextAttributes::new(35, 40, vec![TextEffect::Default]),
            TextAttributes::new(
                40,
                44,
                vec![TextEffect::Styles(vec![
                    Style::Bold,
                    Style::Strikethrough,
                    Style::Underline,
                    Style::Italic,
                ])],
            ),
        ])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><b>Bold</b> <u>underline</u> <i>italic</i> <s>strikethrough</s> all <i><u><s><b>four</b></s></u></i></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styles_single_end_to_end() {
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

        message.components = vec![BubbleComponent::Text(vec![TextAttributes::new(
            0,
            10,
            vec![TextEffect::Styles(vec![
                Style::Bold,
                Style::Strikethrough,
                Style::Underline,
                Style::Italic,
            ])],
        )])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><i><u><s><b>Everything</b></s></u></i></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styles_mixed_end_to_end() {
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

        message.components = vec![BubbleComponent::Text(vec![
            TextAttributes::new(0, 9, vec![TextEffect::Styles(vec![Style::Underline])]),
            TextAttributes::new(9, 17, vec![TextEffect::Default]),
            TextAttributes::new(17, 23, vec![TextEffect::Animated(Animation::Jitter)]),
            TextAttributes::new(23, 30, vec![TextEffect::Default]),
        ])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><u>Underline</u> normal <span class=\"animationJitter\">jitter</span> normal</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styled_plain_link() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text =
            Some("https://github.com/ReagentX/imessage-exporter/discussions/553".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Text(vec![TextAttributes::new(
            0,
            61,
            vec![
                TextEffect::Animated(Animation::Big),
                TextEffect::Link(
                    "https://github.com/ReagentX/imessage-exporter/discussions/553".to_string(),
                ),
            ],
        )])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><a href=\"https://github.com/ReagentX/imessage-exporter/discussions/553\"><span class=\"animationBig\">https://github.com/ReagentX/imessage-exporter/discussions/553</span></a></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styled_emoji_bold_underline() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("🅱️Bold_Underline".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Text(vec![
            TextAttributes::new(0, 7, vec![TextEffect::Default]),
            TextAttributes::new(7, 11, vec![TextEffect::Styles(vec![Style::Bold])]),
            TextAttributes::new(11, 12, vec![TextEffect::Default]),
            TextAttributes::new(12, 21, vec![TextEffect::Styles(vec![Style::Underline])]),
        ])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">🅱\u{fe0f}<b>Bold</b>_<u>Underline</u></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styled_overlapping_ranges() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("8:00 pm".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Text(vec![
            TextAttributes::new(
                0,
                1,
                vec![
                    TextEffect::Conversion(Unit::Timezone),
                    TextEffect::Styles(vec![Style::Bold]),
                ],
            ),
            TextAttributes::new(1, 2, vec![TextEffect::Conversion(Unit::Timezone)]),
            TextAttributes::new(
                2,
                4,
                vec![
                    TextEffect::Conversion(Unit::Timezone),
                    TextEffect::Styles(vec![Style::Underline]),
                ],
            ),
            TextAttributes::new(4, 5, vec![TextEffect::Conversion(Unit::Timezone)]),
            TextAttributes::new(
                5,
                7,
                vec![
                    TextEffect::Conversion(Unit::Timezone),
                    TextEffect::Styles(vec![Style::Italic]),
                ],
            ),
        ])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\"><b><u>8</u></b><u>:</u><u><u>00</u></u><u> </u><i><u>pm</u></i></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }
}

#[cfg(test)]
mod edited_tests {
    use std::{env::current_dir, fs::File, io::Read};

    use crate::{Config, Exporter, HTML, Options, exporters::exporter::Writer};
    use imessage_database::{
        message_types::{
            edited::{EditStatus, EditedEvent, EditedMessage, EditedMessagePart},
            text_effects::{Style, TextEffect},
        },
        tables::messages::models::{AttachmentMeta, BubbleComponent, TextAttributes},
    };

    #[test]
    fn can_format_html_edited_with_formatting() {
        // Create exporter
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        // Create edited message data
        let edited_message = EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![
                    EditedEvent {
                        date: 758573156000000000,
                        text: Some("Test".to_string()),
                        components: vec![BubbleComponent::Text(vec![TextAttributes {
                            start: 0,
                            end: 4,
                            effects: vec![TextEffect::Default],
                        }])],
                        guid: None,
                    },
                    EditedEvent {
                        date: 758573166000000000,
                        text: Some("Test".to_string()),
                        components: vec![BubbleComponent::Text(vec![TextAttributes {
                            start: 0,
                            end: 4,
                            effects: vec![TextEffect::Styles(vec![Style::Strikethrough])],
                        }])],
                        guid: Some("76A466B8-D21E-4A20-AF62-FF2D3A20D31C".to_string()),
                    },
                ],
            }],
        };

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.date_edited = 674530231992568192;
        message.text = Some("Test".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.edited_parts = Some(edited_message);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/EditedWithFormatting");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        message.components = vec![BubbleComponent::Text(vec![TextAttributes::new(
            0,
            4,
            vec![TextEffect::Styles(vec![Style::Strikethrough])],
        )])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<div class=\"edited\"><table><tbody><tr><td><span class=\"timestamp\"></span></td><td>Test</td></tr></tbody><tfoot><tr><td><span class=\"timestamp\">Edited 10 seconds later</span></td><td><s>Test</s></td></tr></tfoot></table></div>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_final_unsent() {
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

        message.components = vec![
            BubbleComponent::Text(vec![TextAttributes::new(0, 28, vec![TextEffect::Default])]),
            BubbleComponent::Attachment(AttachmentMeta {
                guid: Some("D0551D89-4E11-43D0-9A0E-06F19704E97B".to_string()),
                transcription: None,
                height: None,
                width: None,
                name: None,
            }),
            BubbleComponent::Text(vec![TextAttributes::new(31, 63, vec![TextEffect::Default])]),
            BubbleComponent::Retracted,
        ];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">From arbitrary byte stream:\r</span>\n</div>\n<hr><div class=\"message_part\">\n<span class=\"attachment_error\">Attachment does not exist!</span>\n</div>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">To native Rust data structures:\r</span>\n</div>\n<hr><div class=\"message_part\">\n<span class=\"unsent\"><span class=\"unsent\">You unsent this message part 1 hour, 49 seconds after sending!</span></span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_no_edits() {
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

        message.components = vec![
            BubbleComponent::Text(vec![TextAttributes::new(0, 28, vec![TextEffect::Default])]),
            BubbleComponent::Attachment(AttachmentMeta {
                guid: Some("D0551D89-4E11-43D0-9A0E-06F19704E97B".to_string()),
                transcription: None,
                height: None,
                width: None,
                name: None,
            }),
            BubbleComponent::Text(vec![TextAttributes::new(31, 63, vec![TextEffect::Default])]),
        ];

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "<div class=\"message\">\n<div class=\"sent iMessage\">\n<p><span class=\"timestamp\"><a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a> </span>\n<span class=\"sender\">Me</span></p>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">From arbitrary byte stream:\r</span>\n</div>\n<hr><div class=\"message_part\">\n<span class=\"attachment_error\">Attachment does not exist!</span>\n</div>\n<hr><div class=\"message_part\">\n<span class=\"bubble\">To native Rust data structures:\r</span>\n</div>\n</div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_fully_unsent() {
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

        message.components = vec![];

        let actual = exporter.format_announcement(&message);
        let expected = "<div class =\"announcement\"><p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You unsent a message.</p></div>";

        assert_eq!(actual, expected);
    }
}
