use std::{
    collections::{
        HashMap, HashSet,
        hash_map::Entry::{Occupied, Vacant},
    },
    fs::File,
    io::{BufWriter, Write},
};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::{
    app::{
        compatibility::attachment_manager::AttachmentManagerMode, error::RuntimeError,
        progress::ExportProgress, runtime::Config,
    },
    exporters::exporter::{ATTACHMENT_NO_FILENAME, BalloonFormatter, Exporter, MessageFormatter},
};

use imessage_database::{
    error::{message::MessageError, plist::PlistParseError, table::TableError},
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
        polls::Poll,
        text_effects::{Animation, Style, TextEffect},
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
            models::{AttachmentMeta, BubbleComponent, GroupAction, Service, TextAttributes},
        },
        table::{FITNESS_RECEIVER, ME, ORPHANED, Table, YOU},
    },
    util::{
        dates::{TIMESTAMP_FACTOR, get_local_time},
        plist::parse_ns_keyed_archiver,
    },
};

/// JSON-serializable message structure
#[derive(Serialize, Deserialize)]
pub struct JsonMessage {
    pub guid: String,
    pub timestamp: String,
    pub timestamp_unix: i64,
    pub sender: String,
    pub is_from_me: bool,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_read: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_read_unix: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_delivered: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_delivered_unix: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub text_effects: Vec<JsonTextEffect>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<JsonAttachment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tapbacks: Vec<JsonTapback>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<JsonMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expressive: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_message: Option<JsonAppMessage>,
    #[serde(skip_serializing_if = "is_false")]
    pub is_deleted: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub is_reply: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited: Option<Vec<JsonEditedPart>>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// JSON-serializable attachment structure
#[derive(Serialize, Deserialize)]
pub struct JsonAttachment {
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub is_sticker: bool,
}

/// JSON-serializable tapback structure
#[derive(Serialize, Deserialize)]
pub struct JsonTapback {
    pub sender: String,
    pub tapback_type: String,
}

/// JSON-serializable app message structure
#[derive(Serialize, Deserialize)]
pub struct JsonAppMessage {
    pub app_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, String>,
}

/// JSON-serializable edited message part
#[derive(Serialize, Deserialize)]
pub struct JsonEditedPart {
    pub timestamp: String,
    pub text: Option<String>,
}

/// JSON-serializable text effect
#[derive(Serialize, Deserialize, Clone)]
pub struct JsonTextEffect {
    /// The range start in the text
    pub start: usize,
    /// The range end in the text
    pub end: usize,
    /// The type of effect (mention, link, otp, conversion, styles, animated)
    pub effect_type: String,
    /// Additional data depending on effect type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

/// JSON-serializable announcement
#[derive(Serialize)]
pub struct JsonAnnouncement {
    pub timestamp: String,
    pub sender: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

pub struct JSON<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config,
    /// Handles to files we want to write messages to
    /// Map of resolved chatroom file location to a buffered writer
    pub files: HashMap<String, BufWriter<File>>,
    /// Track which files have had their first message written (for comma handling)
    files_with_messages: HashSet<String>,
    /// Writer instance for orphaned messages
    pub orphaned: BufWriter<File>,
    /// Whether orphaned file has had its first message written
    orphaned_has_messages: bool,
    /// Progress Bar model for alerting the user about current export state
    pb: ExportProgress,
}

// MARK: Exporter
impl<'a> Exporter<'a> for JSON<'a> {
    fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        let mut orphaned = config.options.export_path.clone();
        orphaned.push(ORPHANED);
        orphaned.set_extension("json");

        let file = File::options().append(true).create(true).open(&orphaned)?;
        let mut orphaned_writer = BufWriter::new(file);

        // Write opening bracket for orphaned file
        JSON::write_to_file(&mut orphaned_writer, "[\n")?;

        Ok(JSON {
            config,
            files: HashMap::new(),
            files_with_messages: HashSet::new(),
            orphaned: orphaned_writer,
            orphaned_has_messages: false,
            pb: ExportProgress::new(),
        })
    }

    fn iter_messages(&mut self) -> Result<(), RuntimeError> {
        // Tell the user what we are doing
        eprintln!(
            "Exporting to {} as json...",
            self.config.options.export_path.display()
        );

        // Keep track of current message ROWID
        let mut current_message_row = -1;

        // Set up progress bar
        let mut current_message = 0;
        let total_messages = Message::get_count(
            self.config.data_source.db(),
            &self.config.options.query_context,
        )?;
        self.pb.start(total_messages);

        let mut statement = Message::stream_rows(
            self.config.data_source.db(),
            &self.config.options.query_context,
        )?;

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
            let _ = msg.generate_text(self.config.data_source.db());

            // Render the announcement in-line
            if msg.is_announcement() {
                let announcement = self.format_announcement(&msg);
                self.write_message(&msg, &announcement)?;
            }
            // Message tapbacks and poll votes are rendered in context, so no need to render them
            else if !msg.is_tapback() && !msg.is_poll_vote() && !msg.is_poll_update() {
                let message_json = self.format_message(&msg, 0)?;
                self.write_message(&msg, &message_json)?;
            }
            current_message += 1;
            if current_message % 99 == 0 {
                self.pb.set_position(current_message);
            }
        }
        self.pb.finish();

        // Write closing brackets to all files
        eprintln!("Writing JSON footers...");
        for buf in self.files.values_mut() {
            JSON::write_to_file(buf, "\n]")?;
        }
        JSON::write_to_file(&mut self.orphaned, "\n]")?;

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
                        path.set_extension("json");

                        let file = File::options().append(true).create(true).open(&path)?;
                        let mut buf = BufWriter::new(file);

                        // Write opening bracket for new file
                        JSON::write_to_file(&mut buf, "[\n")?;

                        Ok(entry.insert(buf))
                    }
                }
            }
            None => Ok(&mut self.orphaned),
        }
    }

    fn write_to_file(file: &mut BufWriter<File>, text: &str) -> Result<(), RuntimeError> {
        file.write_all(text.as_bytes())
            .map_err(RuntimeError::DiskError)
    }
}

// MARK: Writer
impl<'a> MessageFormatter<'a> for JSON<'a> {
    fn format_message(&self, message: &Message, indent_size: usize) -> Result<String, TableError> {
        // Useful message metadata
        let message_parts = &message.components;
        let mut attachments = Attachment::from_message(self.config.data_source.db(), message)?;
        let mut replies = message.get_replies(self.config.data_source.db())?;

        // Index of where we are in the attachment Vector
        let mut attachment_index: usize = 0;

        // Collect message text
        let mut text_parts: Vec<String> = Vec::new();
        let mut text_effects: Vec<JsonTextEffect> = Vec::new();
        let mut json_attachments: Vec<JsonAttachment> = Vec::new();
        let mut json_tapbacks: Vec<JsonTapback> = Vec::new();
        let mut json_replies: Vec<JsonMessage> = Vec::new();
        let mut app_message: Option<JsonAppMessage> = None;
        let mut edited_parts: Option<Vec<JsonEditedPart>> = None;

        // Generate the message body from its components
        for (idx, message_part) in message_parts.iter().enumerate() {
            match message_part {
                BubbleComponent::Text(text_attrs) => {
                    if let Some(text) = &message.text {
                        // Render edited message content, if applicable
                        if message.is_part_edited(idx) {
                            if let Some(edit_parts) = &message.edited_parts {
                                edited_parts =
                                    self.build_edited_parts(message, edit_parts, idx);
                            }
                        } else {
                            let formatted_text = self.format_attributes(text, text_attrs);
                            let text_to_use = if formatted_text.is_empty() {
                                text.clone()
                            } else if formatted_text.starts_with(FITNESS_RECEIVER) {
                                formatted_text.replace(FITNESS_RECEIVER, YOU)
                            } else {
                                formatted_text
                            };
                            text_parts.push(text_to_use);

                            // Collect text effects
                            for attr in text_attrs {
                                for effect in &attr.effects {
                                    if let Some(json_effect) =
                                        self.build_text_effect(attr.start, attr.end, effect)
                                    {
                                        text_effects.push(json_effect);
                                    }
                                }
                            }
                        }
                    }
                }
                BubbleComponent::Attachment(metadata) => {
                    if let Some(attachment) = attachments.get_mut(attachment_index) {
                        if attachment.is_sticker {
                            json_attachments.push(self.build_sticker_attachment(attachment, message));
                        } else {
                            json_attachments.push(
                                self.build_attachment(attachment, message, metadata),
                            );
                        }
                        attachment_index += 1;
                    }
                }
                BubbleComponent::App => {
                    if let Ok(app_msg) = self.format_app(message, &mut attachments, "") {
                        if let Ok(parsed) = serde_json::from_str::<JsonAppMessage>(&app_msg) {
                            app_message = Some(parsed);
                        } else {
                            // If parsing fails, store the raw string as title
                            app_message = Some(JsonAppMessage {
                                app_type: "unknown".to_string(),
                                title: Some(app_msg),
                                subtitle: None,
                                caption: None,
                                url: None,
                                extra: HashMap::new(),
                            });
                        }
                    }
                }
                BubbleComponent::Retracted => {
                    if let Some(edit_parts) = &message.edited_parts {
                        edited_parts = self.build_edited_parts(message, edit_parts, idx);
                    }
                }
            }

            // Handle Tapbacks
            if let Some(tapbacks_map) = self.config.tapbacks.get(&message.guid)
                && let Some(tapbacks) = tapbacks_map.get(&idx)
            {
                for tapback in tapbacks {
                    if let Ok(tb) = self.build_tapback(tapback) {
                        json_tapbacks.push(tb);
                    }
                }
            }

            // Handle Replies (only at top level)
            if indent_size == 0 {
                if let Some(msg_replies) = replies.get_mut(&idx) {
                    for reply in msg_replies.iter_mut() {
                        let _ = reply.generate_text(self.config.data_source.db());
                        if !reply.is_tapback() {
                            if let Ok(reply_json) = self.format_message(reply, indent_size + 1) {
                                if let Ok(parsed) = serde_json::from_str(&reply_json) {
                                    json_replies.push(parsed);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Build expressive info
        let expressive = if message.expressive_send_style_id.is_some() {
            Some(self.format_expressive(message).to_string())
        } else {
            None
        };

        // Build date_read and date_delivered
        let (date_read, date_read_unix) = self.get_date_read(message);
        let (date_delivered, date_delivered_unix) = self.get_date_delivered(message);

        // Format timestamp as ISO 8601
        let timestamp = self
            .iso_format(&message.date(&self.config.offset))
            .unwrap_or_default();

        let json_msg = JsonMessage {
            guid: message.guid.clone(),
            timestamp,
            timestamp_unix: message.date / 1_000_000_000,
            sender: self
                .config
                .who(
                    message.handle_id,
                    message.is_from_me(),
                    &message.destination_caller_id,
                )
                .to_string(),
            is_from_me: message.is_from_me(),
            service: self.service_to_string(message.service()),
            date_read,
            date_read_unix,
            date_delivered,
            date_delivered_unix,
            subject: message.subject.clone(),
            text: if text_parts.is_empty() {
                None
            } else {
                Some(text_parts.join("\n"))
            },
            text_effects,
            attachments: json_attachments,
            tapbacks: json_tapbacks,
            replies: json_replies,
            expressive,
            app_message,
            is_deleted: message.is_deleted(),
            is_reply: message.is_reply(),
            edited: edited_parts,
        };

        Ok(serde_json::to_string(&json_msg).unwrap_or_default() + "\n")
    }

    fn format_attachment(
        &self,
        attachment: &'a mut Attachment,
        message: &Message,
        _metadata: &AttachmentMeta,
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
        Ok(self.config.message_attachment_path(attachment))
    }

    fn format_sticker(&self, sticker: &'a mut Attachment, message: &Message) -> String {
        match self.format_attachment(sticker, message, &AttachmentMeta::default()) {
            Ok(path_to_sticker) => path_to_sticker,
            Err(path) => path.to_string(),
        }
    }

    fn format_app(
        &self,
        message: &'a Message,
        attachments: &mut Vec<Attachment>,
        _indent: &str,
    ) -> Result<String, MessageError> {
        if let Variant::App(balloon) = message.variant() {
            // Handwritten messages use a different payload type
            if message.is_handwriting()
                && let Some(payload) = message.raw_payload_data(self.config.data_source.db())
            {
                return match HandwrittenMessage::from_payload(&payload) {
                    Ok(bubble) => Ok(self.format_handwriting(message, &bubble, "")),
                    Err(why) => Err(MessageError::PlistParseError(
                        PlistParseError::HandwritingError(why),
                    )),
                };
            }

            // Digital touch messages use a different payload type
            if message.is_digital_touch()
                && let Some(payload) = message.raw_payload_data(self.config.data_source.db())
            {
                return match digital_touch::from_payload(&payload) {
                    Some(bubble) => Ok(self.format_digital_touch(message, &bubble, "")),
                    None => Err(MessageError::PlistParseError(
                        PlistParseError::DigitalTouchError,
                    )),
                };
            }

            // Poll messages use a different payload type
            if message.is_poll() {
                let poll = message.as_poll(self.config.data_source.db())?;
                return match poll {
                    Some(poll) => Ok(self.format_poll(&poll, "")),
                    None => Err(MessageError::PlistParseError(
                        PlistParseError::WrongMessageType,
                    )),
                };
            }

            if let Some(payload) = message.payload_data(self.config.data_source.db()) {
                // Handle URL messages separately since they are a special case
                let parsed = parse_ns_keyed_archiver(&payload)?;
                let res = if message.is_url() {
                    let bubble = URLMessage::get_url_message_override(&parsed)?;
                    match bubble {
                        URLOverride::Normal(balloon) => self.format_url(message, &balloon, ""),
                        URLOverride::AppleMusic(balloon) => self.format_music(&balloon, ""),
                        URLOverride::Collaboration(balloon) => {
                            self.format_collaboration(&balloon, "")
                        }
                        URLOverride::AppStore(balloon) => self.format_app_store(&balloon, ""),
                        URLOverride::SharedPlacemark(balloon) => {
                            self.format_placemark(&balloon, "")
                        }
                    }
                } else {
                    // Handle the app case
                    match AppMessage::from_map(&parsed) {
                        Ok(bubble) => match balloon {
                            CustomBalloon::Application(bundle_id) => {
                                self.format_generic_app(&bubble, bundle_id, attachments, "")
                            }
                            CustomBalloon::ApplePay => self.format_apple_pay(&bubble, ""),
                            CustomBalloon::Fitness => self.format_fitness(&bubble, ""),
                            CustomBalloon::Slideshow => self.format_slideshow(&bubble, ""),
                            CustomBalloon::CheckIn => self.format_check_in(&bubble, ""),
                            CustomBalloon::FindMy => self.format_find_my(&bubble, ""),
                            CustomBalloon::Polls
                            | CustomBalloon::Handwriting
                            | CustomBalloon::DigitalTouch
                            | CustomBalloon::URL => {
                                unreachable!()
                            }
                        },
                        Err(why) => {
                            return Err(MessageError::PlistParseError(why));
                        }
                    }
                };
                Ok(res)
            } else {
                // Sometimes, URL messages are missing their payloads
                if message.is_url()
                    && let Some(text) = &message.text
                {
                    return Ok(text.clone());
                }
                Err(MessageError::PlistParseError(PlistParseError::NoPayload))
            }
        } else {
            Err(MessageError::PlistParseError(
                PlistParseError::WrongMessageType,
            ))
        }
    }

    fn format_tapback(&self, msg: &Message) -> Result<String, TableError> {
        match msg.variant() {
            Variant::Tapback(_, action, tapback) => {
                if let TapbackAction::Removed = action {
                    return Ok(String::new());
                }

                let who = self.config.who(
                    msg.handle_id,
                    msg.is_from_me(),
                    &msg.destination_caller_id,
                );

                let tapback_type = match tapback {
                    Tapback::Sticker => "sticker".to_string(),
                    _ => format!("{tapback}"),
                };

                let json_tapback = JsonTapback {
                    sender: who.to_string(),
                    tapback_type,
                };

                Ok(serde_json::to_string(&json_tapback).unwrap_or_default())
            }
            _ => unreachable!(),
        }
    }

    fn format_expressive(&self, msg: &'a Message) -> &'a str {
        match msg.get_expressive() {
            Expressive::Screen(effect) => match effect {
                ScreenEffect::Confetti => "confetti",
                ScreenEffect::Echo => "echo",
                ScreenEffect::Fireworks => "fireworks",
                ScreenEffect::Balloons => "balloons",
                ScreenEffect::Heart => "heart",
                ScreenEffect::Lasers => "lasers",
                ScreenEffect::ShootingStar => "shooting_star",
                ScreenEffect::Sparkles => "sparkles",
                ScreenEffect::Spotlight => "spotlight",
            },
            Expressive::Bubble(effect) => match effect {
                BubbleEffect::Slam => "slam",
                BubbleEffect::Loud => "loud",
                BubbleEffect::Gentle => "gentle",
                BubbleEffect::InvisibleInk => "invisible_ink",
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
            who = self.config.options.custom_name.as_deref().unwrap_or(YOU);
        }

        let timestamp = self
            .iso_format(&msg.date(&self.config.offset))
            .unwrap_or_default();

        match msg.get_announcement() {
            Some(announcement) => {
                let (action, target) = match announcement {
                    Announcement::GroupAction(action) => match action {
                        GroupAction::ParticipantAdded(person)
                        | GroupAction::ParticipantRemoved(person) => {
                            let resolved_person =
                                self.config
                                    .who(Some(person), false, &msg.destination_caller_id);
                            let action_word = if matches!(action, GroupAction::ParticipantAdded(_))
                            {
                                "participant_added"
                            } else {
                                "participant_removed"
                            };
                            (action_word.to_string(), Some(resolved_person.to_string()))
                        }
                        GroupAction::NameChange(name) => {
                            ("name_changed".to_string(), Some(name.to_string()))
                        }
                        GroupAction::ParticipantLeft => ("participant_left".to_string(), None),
                        GroupAction::GroupIconChanged => ("group_icon_changed".to_string(), None),
                        GroupAction::GroupIconRemoved => ("group_icon_removed".to_string(), None),
                        GroupAction::ChatBackgroundChanged => {
                            ("chat_background_changed".to_string(), None)
                        }
                        GroupAction::ChatBackgroundRemoved => {
                            ("chat_background_removed".to_string(), None)
                        }
                    },
                    Announcement::AudioMessageKept => ("audio_message_kept".to_string(), None),
                    Announcement::FullyUnsent => ("fully_unsent".to_string(), None),
                    Announcement::Unknown(num) => (format!("unknown_{num}"), None),
                };

                let json_announcement = JsonAnnouncement {
                    timestamp,
                    sender: who.to_string(),
                    action,
                    target,
                };
                serde_json::to_string(&json_announcement).unwrap_or_default() + "\n"
            }
            None => String::new(),
        }
    }

    fn format_shareplay(&self) -> &'static str {
        "shareplay"
    }

    fn format_shared_location(&self, msg: &'a Message) -> &'static str {
        if msg.started_sharing_location() {
            return "started_sharing_location";
        } else if msg.stopped_sharing_location() {
            return "stopped_sharing_location";
        }
        "shared_location"
    }

    fn format_edited(
        &self,
        msg: &'a Message,
        edited_message: &'a EditedMessage,
        message_part_idx: usize,
        _indent: &str,
    ) -> Option<String> {
        if let Some(edited_message_part) = edited_message.part(message_part_idx) {
            let mut edits: Vec<JsonEditedPart> = Vec::new();

            match edited_message_part.status {
                EditStatus::Edited => {
                    for event in &edited_message_part.edit_history {
                        let parsed_timestamp = self
                            .iso_format(&get_local_time(&event.date, &self.config.offset))
                            .unwrap_or_default();
                        edits.push(JsonEditedPart {
                            timestamp: parsed_timestamp,
                            text: event.text.clone(),
                        });
                    }
                }
                EditStatus::Unsent => {
                    let timestamp = self
                        .iso_format(&msg.date_edited(&self.config.offset))
                        .unwrap_or_default();
                    edits.push(JsonEditedPart {
                        timestamp,
                        text: None,
                    });
                }
                EditStatus::Original => {
                    return None;
                }
            }

            return Some(serde_json::to_string(&edits).unwrap_or_default());
        }
        None
    }

    fn format_attributes(&'a self, text: &'a str, attributes: &'a [TextAttributes]) -> String {
        let mut formatted_text = String::with_capacity(text.len());
        let mut prev_start = 0;
        let mut prev_end = 0;

        for effect in attributes {
            if prev_start == effect.start && prev_end == effect.end {
                continue;
            }
            if let Some(message_content) = text.get(effect.start..effect.end) {
                prev_start = effect.start;
                prev_end = effect.end;
                // For JSON, we just use plain text (no formatting needed)
                formatted_text.push_str(message_content);
            }
        }
        formatted_text
    }
}

// MARK: Balloon
impl<'a> BalloonFormatter<&'a str> for JSON<'a> {
    fn format_url(&self, msg: &Message, balloon: &URLMessage, _indent: &str) -> String {
        let mut extra = HashMap::new();

        if let Some(title) = balloon.title {
            extra.insert("title".to_string(), title.to_string());
        }

        if let Some(summary) = balloon.summary {
            extra.insert("summary".to_string(), summary.to_string());
        }

        let app_msg = JsonAppMessage {
            app_type: "url".to_string(),
            title: balloon.title.map(|s| s.to_string()),
            subtitle: None,
            caption: balloon.summary.map(|s| s.to_string()),
            url: balloon.get_url().map(|s| s.to_string()).or_else(|| msg.text.clone()),
            extra: HashMap::new(),
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_music(&self, balloon: &MusicMessage, _indent: &str) -> String {
        let mut extra = HashMap::new();

        if let Some(track_name) = balloon.track_name {
            extra.insert("track_name".to_string(), track_name.to_string());
        }

        if let Some(album) = balloon.album {
            extra.insert("album".to_string(), album.to_string());
        }

        if let Some(artist) = balloon.artist {
            extra.insert("artist".to_string(), artist.to_string());
        }

        if let Some(lyrics) = &balloon.lyrics {
            extra.insert("lyrics".to_string(), lyrics.join("\n"));
        }

        let app_msg = JsonAppMessage {
            app_type: "music".to_string(),
            title: balloon.track_name.map(|s| s.to_string()),
            subtitle: balloon.artist.map(|s| s.to_string()),
            caption: balloon.album.map(|s| s.to_string()),
            url: balloon.url.map(|s| s.to_string()),
            extra,
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_collaboration(&self, balloon: &CollaborationMessage, _indent: &str) -> String {
        let app_msg = JsonAppMessage {
            app_type: "collaboration".to_string(),
            title: balloon.title.map(|s| s.to_string()),
            subtitle: balloon.app_name.map(|s| s.to_string()),
            caption: None,
            url: balloon.get_url().map(|s| s.to_string()),
            extra: HashMap::new(),
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_app_store(&self, balloon: &AppStoreMessage, _indent: &str) -> String {
        let mut extra = HashMap::new();

        if let Some(platform) = balloon.platform {
            extra.insert("platform".to_string(), platform.to_string());
        }

        if let Some(genre) = balloon.genre {
            extra.insert("genre".to_string(), genre.to_string());
        }

        let app_msg = JsonAppMessage {
            app_type: "app_store".to_string(),
            title: balloon.app_name.map(|s| s.to_string()),
            subtitle: balloon.description.map(|s| s.to_string()),
            caption: None,
            url: balloon.url.map(|s| s.to_string()),
            extra,
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_placemark(&self, balloon: &PlacemarkMessage, _indent: &str) -> String {
        let mut extra = HashMap::new();

        if let Some(name) = balloon.placemark.name {
            extra.insert("name".to_string(), name.to_string());
        }

        if let Some(address) = balloon.placemark.address {
            extra.insert("address".to_string(), address.to_string());
        }

        if let Some(city) = balloon.placemark.city {
            extra.insert("city".to_string(), city.to_string());
        }

        if let Some(state) = balloon.placemark.state {
            extra.insert("state".to_string(), state.to_string());
        }

        if let Some(country) = balloon.placemark.country {
            extra.insert("country".to_string(), country.to_string());
        }

        if let Some(postal_code) = balloon.placemark.postal_code {
            extra.insert("postal_code".to_string(), postal_code.to_string());
        }

        let app_msg = JsonAppMessage {
            app_type: "placemark".to_string(),
            title: balloon.place_name.map(|s| s.to_string()),
            subtitle: None,
            caption: None,
            url: balloon.get_url().map(|s| s.to_string()),
            extra,
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_handwriting(
        &self,
        _msg: &Message,
        _balloon: &HandwrittenMessage,
        _indent: &str,
    ) -> String {
        let app_msg = JsonAppMessage {
            app_type: "handwriting".to_string(),
            title: None,
            subtitle: None,
            caption: None,
            url: None,
            extra: HashMap::new(),
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_digital_touch(&self, _: &Message, balloon: &DigitalTouch, _indent: &str) -> String {
        let app_msg = JsonAppMessage {
            app_type: "digital_touch".to_string(),
            title: Some(format!("{balloon:?}")),
            subtitle: None,
            caption: None,
            url: None,
            extra: HashMap::new(),
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_apple_pay(&self, balloon: &AppMessage, _indent: &str) -> String {
        let app_msg = JsonAppMessage {
            app_type: "apple_pay".to_string(),
            title: balloon.caption.map(|s| s.to_string()),
            subtitle: balloon.ldtext.map(|s| s.to_string()),
            caption: None,
            url: None,
            extra: HashMap::new(),
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_fitness(&self, balloon: &AppMessage, _indent: &str) -> String {
        let app_msg = JsonAppMessage {
            app_type: "fitness".to_string(),
            title: balloon.app_name.map(|s| s.to_string()),
            subtitle: balloon.ldtext.map(|s| s.to_string()),
            caption: None,
            url: None,
            extra: HashMap::new(),
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_slideshow(&self, balloon: &AppMessage, _indent: &str) -> String {
        let app_msg = JsonAppMessage {
            app_type: "slideshow".to_string(),
            title: balloon.ldtext.map(|s| s.to_string()),
            subtitle: None,
            caption: None,
            url: balloon.url.map(|s| s.to_string()),
            extra: HashMap::new(),
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_find_my(&self, balloon: &AppMessage, _indent: &str) -> String {
        let app_msg = JsonAppMessage {
            app_type: "find_my".to_string(),
            title: balloon.app_name.map(|s| s.to_string()),
            subtitle: balloon.ldtext.map(|s| s.to_string()),
            caption: None,
            url: None,
            extra: HashMap::new(),
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_check_in(&self, balloon: &AppMessage, _indent: &str) -> String {
        let mut extra = HashMap::new();
        let metadata: HashMap<&str, &str> = balloon.parse_query_string();

        // Before manual check-in
        if let Some(date_str) = metadata.get("estimatedEndTime") {
            let date_stamp = date_str.parse::<f64>().unwrap_or(0.) as i64 * TIMESTAMP_FACTOR;
            let date_time = get_local_time(&date_stamp, &0);
            let date_string = self.iso_format(&date_time).unwrap_or_default();
            extra.insert("estimated_end_time".to_string(), date_string);
        }
        // Expired check-in
        else if let Some(date_str) = metadata.get("triggerTime") {
            let date_stamp = date_str.parse::<f64>().unwrap_or(0.) as i64 * TIMESTAMP_FACTOR;
            let date_time = get_local_time(&date_stamp, &0);
            let date_string = self.iso_format(&date_time).unwrap_or_default();
            extra.insert("trigger_time".to_string(), date_string);
        }
        // Accepted check-in
        else if let Some(date_str) = metadata.get("sendDate") {
            let date_stamp = date_str.parse::<f64>().unwrap_or(0.) as i64 * TIMESTAMP_FACTOR;
            let date_time = get_local_time(&date_stamp, &0);
            let date_string = self.iso_format(&date_time).unwrap_or_default();
            extra.insert("send_date".to_string(), date_string);
        }

        let app_msg = JsonAppMessage {
            app_type: "check_in".to_string(),
            title: balloon.caption.map(|s| s.to_string()),
            subtitle: None,
            caption: None,
            url: None,
            extra,
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_poll(&self, poll: &Poll, _indent: &str) -> String {
        let mut extra = HashMap::new();

        for poll_option_id in &poll.order {
            if let Some(option) = poll.options.get(poll_option_id) {
                let voters: Vec<String> = option.votes.iter().map(|v| v.voter.clone()).collect();
                extra.insert(
                    option.text.clone(),
                    format!("{} votes: {}", option.votes.len(), voters.join(", ")),
                );
            }
        }

        let app_msg = JsonAppMessage {
            app_type: "poll".to_string(),
            title: None,
            subtitle: None,
            caption: None,
            url: None,
            extra,
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }

    fn format_generic_app(
        &self,
        balloon: &AppMessage,
        bundle_id: &str,
        _: &mut Vec<Attachment>,
        _indent: &str,
    ) -> String {
        let mut extra = HashMap::new();
        extra.insert("bundle_id".to_string(), bundle_id.to_string());

        if let Some(subcaption) = balloon.subcaption {
            extra.insert("subcaption".to_string(), subcaption.to_string());
        }

        if let Some(trailing_caption) = balloon.trailing_caption {
            extra.insert("trailing_caption".to_string(), trailing_caption.to_string());
        }

        if let Some(trailing_subcaption) = balloon.trailing_subcaption {
            extra.insert(
                "trailing_subcaption".to_string(),
                trailing_subcaption.to_string(),
            );
        }

        let app_msg = JsonAppMessage {
            app_type: "app".to_string(),
            title: balloon.title.map(|s| s.to_string()),
            subtitle: balloon.subtitle.map(|s| s.to_string()),
            caption: balloon.caption.map(|s| s.to_string()),
            url: balloon.url.map(|s| s.to_string()),
            extra,
        };

        serde_json::to_string(&app_msg).unwrap_or_default()
    }
}

// MARK: Impl
impl JSON<'_> {
    /// Write a message to the appropriate file, handling comma placement for valid JSON arrays
    fn write_message(&mut self, message: &Message, json_content: &str) -> Result<(), RuntimeError> {
        // Determine which file this message belongs to and get its identifier
        let file_id = match self.config.conversation(message) {
            Some((chatroom, _)) => Some(self.config.filename(chatroom)),
            None => None,
        };

        // Get the file and check if it needs a comma
        let (file, needs_comma) = match &file_id {
            Some(filename) => {
                // Ensure the file exists (this also writes the opening bracket if new)
                let _ = self.get_or_create_file(message)?;
                let needs_comma = self.files_with_messages.contains(filename);
                if !needs_comma {
                    self.files_with_messages.insert(filename.clone());
                }
                (
                    self.files.get_mut(filename).expect("File should exist"),
                    needs_comma,
                )
            }
            None => {
                let needs_comma = self.orphaned_has_messages;
                self.orphaned_has_messages = true;
                (&mut self.orphaned, needs_comma)
            }
        };

        // Write comma if not the first message in this file
        if needs_comma {
            JSON::write_to_file(file, ",\n")?;
        }

        // Write the message content (without trailing newline - format functions should not add one)
        JSON::write_to_file(file, json_content.trim_end())
    }

    fn build_attachment(
        &self,
        attachment: &mut Attachment,
        message: &Message,
        metadata: &AttachmentMeta,
    ) -> JsonAttachment {
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
        let _ = self.config.options.attachment_manager.handle_attachment(
            message,
            attachment,
            self.config,
        );

        if will_encode {
            self.pb.set_default_style();
        }

        let filename = self.config.message_attachment_path(attachment);
        let mime_type = match attachment.mime_type() {
            MediaType::Image(mime)
            | MediaType::Video(mime)
            | MediaType::Audio(mime)
            | MediaType::Text(mime)
            | MediaType::Application(mime) => Some(mime.to_string()),
            MediaType::Other(mime) => Some(mime.to_string()),
            MediaType::Unknown => None,
        };

        JsonAttachment {
            filename,
            mime_type,
            transcription: metadata.transcription.clone(),
            is_sticker: false,
        }
    }

    fn build_sticker_attachment(
        &self,
        sticker: &mut Attachment,
        message: &Message,
    ) -> JsonAttachment {
        let _ = self.config.options.attachment_manager.handle_attachment(
            message,
            sticker,
            self.config,
        );

        let filename = self.config.message_attachment_path(sticker);

        JsonAttachment {
            filename,
            mime_type: None,
            transcription: None,
            is_sticker: true,
        }
    }

    fn service_to_string(&self, service: Service) -> String {
        match service {
            Service::iMessage => "iMessage".to_string(),
            Service::SMS => "SMS".to_string(),
            Service::RCS => "RCS".to_string(),
            Service::Satellite => "Satellite".to_string(),
            Service::Other(s) => s.to_string(),
            Service::Unknown => "Unknown".to_string(),
        }
    }

    /// Format a date as ISO 8601 string
    fn iso_format(&self, date: &Result<DateTime<Local>, MessageError>) -> Option<String> {
        match date {
            Ok(d) => Some(d.format("%Y-%m-%dT%H:%M:%S%:z").to_string()),
            Err(_) => None,
        }
    }

    /// Get date_read formatted as ISO 8601 if available
    fn get_date_read(&self, message: &Message) -> (Option<String>, Option<i64>) {
        if message.date_read != 0 {
            let date_read = message.date_read(&self.config.offset);
            (
                self.iso_format(&date_read),
                Some(message.date_read / 1_000_000_000),
            )
        } else {
            (None, None)
        }
    }

    /// Get date_delivered formatted as ISO 8601 if available
    fn get_date_delivered(&self, message: &Message) -> (Option<String>, Option<i64>) {
        if message.date_delivered != 0 {
            let date_delivered = message.date_delivered(&self.config.offset);
            (
                self.iso_format(&date_delivered),
                Some(message.date_delivered / 1_000_000_000),
            )
        } else {
            (None, None)
        }
    }

    fn build_text_effect(
        &self,
        start: usize,
        end: usize,
        effect: &TextEffect,
    ) -> Option<JsonTextEffect> {
        match effect {
            TextEffect::Default => None,
            TextEffect::Mention(mentioned) => Some(JsonTextEffect {
                start,
                end,
                effect_type: "mention".to_string(),
                data: Some(mentioned.clone()),
            }),
            TextEffect::Link(url) => Some(JsonTextEffect {
                start,
                end,
                effect_type: "link".to_string(),
                data: Some(url.clone()),
            }),
            TextEffect::OTP => Some(JsonTextEffect {
                start,
                end,
                effect_type: "otp".to_string(),
                data: None,
            }),
            TextEffect::Styles(styles) => {
                let style_names: Vec<&str> = styles
                    .iter()
                    .map(|s| match s {
                        Style::Bold => "bold",
                        Style::Italic => "italic",
                        Style::Strikethrough => "strikethrough",
                        Style::Underline => "underline",
                    })
                    .collect();
                Some(JsonTextEffect {
                    start,
                    end,
                    effect_type: "styles".to_string(),
                    data: Some(style_names.join(",")),
                })
            }
            TextEffect::Animated(animation) => {
                let animation_name = match animation {
                    Animation::Big => "big",
                    Animation::Small => "small",
                    Animation::Shake => "shake",
                    Animation::Nod => "nod",
                    Animation::Explode => "explode",
                    Animation::Ripple => "ripple",
                    Animation::Bloom => "bloom",
                    Animation::Jitter => "jitter",
                    Animation::Unknown(id) => return Some(JsonTextEffect {
                        start,
                        end,
                        effect_type: "animated".to_string(),
                        data: Some(format!("unknown_{id}")),
                    }),
                };
                Some(JsonTextEffect {
                    start,
                    end,
                    effect_type: "animated".to_string(),
                    data: Some(animation_name.to_string()),
                })
            }
            TextEffect::Conversion(unit) => Some(JsonTextEffect {
                start,
                end,
                effect_type: "conversion".to_string(),
                data: Some(format!("{unit:?}")),
            }),
        }
    }

    fn build_tapback(&self, msg: &Message) -> Result<JsonTapback, TableError> {
        match msg.variant() {
            Variant::Tapback(_, action, tapback) => {
                if let TapbackAction::Removed = action {
                    return Err(TableError::QueryError(rusqlite::Error::QueryReturnedNoRows));
                }

                let who = self.config.who(
                    msg.handle_id,
                    msg.is_from_me(),
                    &msg.destination_caller_id,
                );

                let tapback_type = match tapback {
                    Tapback::Sticker => "sticker".to_string(),
                    _ => format!("{tapback}"),
                };

                Ok(JsonTapback {
                    sender: who.to_string(),
                    tapback_type,
                })
            }
            _ => Err(TableError::QueryError(rusqlite::Error::QueryReturnedNoRows)),
        }
    }

    fn build_edited_parts(
        &self,
        msg: &Message,
        edited_message: &EditedMessage,
        message_part_idx: usize,
    ) -> Option<Vec<JsonEditedPart>> {
        if let Some(edited_message_part) = edited_message.part(message_part_idx) {
            let mut edits: Vec<JsonEditedPart> = Vec::new();

            match edited_message_part.status {
                EditStatus::Edited => {
                    for event in &edited_message_part.edit_history {
                        let parsed_timestamp = self
                            .iso_format(&get_local_time(&event.date, &self.config.offset))
                            .unwrap_or_default();
                        edits.push(JsonEditedPart {
                            timestamp: parsed_timestamp,
                            text: event.text.clone(),
                        });
                    }
                }
                EditStatus::Unsent => {
                    let timestamp = self
                        .iso_format(&msg.date_edited(&self.config.offset))
                        .unwrap_or_default();
                    edits.push(JsonEditedPart {
                        timestamp,
                        text: None,
                    });
                }
                EditStatus::Original => {
                    return None;
                }
            }

            return Some(edits);
        }
        None
    }
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use crate::{
        Config, Exporter, JSON, Options,
        app::{contacts::Name, export_type::ExportType},
        exporters::exporter::MessageFormatter,
    };
    use imessage_database::tables::table::ME;

    #[test]
    fn can_create() {
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();
        assert_eq!(exporter.files.len(), 0);
    }

    #[test]
    fn can_format_json_from_me_normal() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["sender"], "Me");
        assert_eq!(parsed["text"], "Hello world");
        assert_eq!(parsed["is_from_me"], true);
    }

    #[test]
    fn can_format_json_from_me_normal_deleted() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.text = Some("Hello world".to_string());
        message.date = 674526582885055488;
        message.is_from_me = true;
        message.deleted_from = Some(0);
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["is_deleted"], true);
        assert_eq!(parsed["text"], "Hello world");
    }

    #[test]
    fn can_format_json_from_them_normal() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, Name::fake_name("Sample Contact"));
        config.real_participants.insert(999999, 999999);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        message.handle_id = Some(999999);
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["sender"], "Sample Contact");
        assert_eq!(parsed["text"], "Hello world");
        assert_eq!(parsed["is_from_me"], false);
    }

    #[test]
    fn can_format_json_shareplay() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.item_type = 6;

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["sender"], "Me");
        // SharePlay messages are rendered with minimal text content
        assert!(parsed["guid"].is_string());
    }

    #[test]
    fn can_format_json_announcement() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 2;

        let actual = exporter.format_announcement(&message);
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["action"], "name_changed");
        assert_eq!(parsed["target"], "Hello world");
    }

    #[test]
    fn can_format_json_group_removed() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.participants.insert(1, Name::fake_name("Other"));
        config.real_participants.insert(0, 0);
        config.real_participants.insert(1, 1);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 1;
        message.group_action_type = 1;
        message.other_handle = Some(1);

        let actual = exporter.format_announcement(&message);
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["action"], "participant_removed");
        assert_eq!(parsed["target"], "Other");
    }

    #[test]
    fn can_format_json_group_added() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.participants.insert(1, Name::fake_name("Other"));
        config.real_participants.insert(0, 0);
        config.real_participants.insert(1, 1);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 1;
        message.group_action_type = 0;
        message.other_handle = Some(1);

        let actual = exporter.format_announcement(&message);
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["action"], "participant_added");
        assert_eq!(parsed["target"], "Other");
    }

    #[test]
    fn can_format_json_group_left() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;

        let actual = exporter.format_announcement(&message);
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["action"], "participant_left");
    }

    #[test]
    fn can_format_json_group_icon_removed() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;
        message.group_action_type = 2;

        let actual = exporter.format_announcement(&message);
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["action"], "group_icon_removed");
    }

    #[test]
    fn can_format_json_group_icon_added() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;
        message.group_action_type = 1;

        let actual = exporter.format_announcement(&message);
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["action"], "group_icon_changed");
    }

    #[test]
    fn can_format_json_chat_background_removed() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;
        message.group_action_type = 6;

        let actual = exporter.format_announcement(&message);
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["action"], "chat_background_removed");
    }

    #[test]
    fn can_format_json_chat_background_added() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;
        message.group_action_type = 4;

        let actual = exporter.format_announcement(&message);
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["action"], "chat_background_changed");
    }

    #[test]
    fn can_format_json_audio_message_kept() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.is_from_me = true;
        message.item_type = 5;

        let actual = exporter.format_announcement(&message);
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["action"], "audio_message_kept");
    }

    #[test]
    fn can_format_json_tapback_me() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2000);
        message.associated_message_guid = Some("fake_guid".to_string());

        let actual = exporter.format_tapback(&message).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["sender"], "Me");
        assert_eq!(parsed["tapback_type"], "Loved");
    }

    #[test]
    fn can_format_json_tapback_them() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, Name::fake_name("Sample Contact"));
        config.real_participants.insert(999999, 999999);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2000);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);

        let actual = exporter.format_tapback(&message).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["sender"], "Sample Contact");
        assert_eq!(parsed["tapback_type"], "Loved");
    }

    #[test]
    fn can_format_json_sharing_location() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.is_from_me = true;
        message.share_status = false;
        message.item_type = 4;

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        // Sharing location messages produce valid JSON
        assert!(parsed["guid"].is_string());
        assert_eq!(parsed["sender"], "Me");
    }

    #[test]
    fn can_format_json_expressive_slam() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.text = Some("Hello!".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.expressive_send_style_id = Some("com.apple.MobileSMS.expressivesend.impact".to_string());
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["expressive"], "slam");
    }

    #[test]
    fn can_format_json_expressive_loud() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.text = Some("Hello!".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.expressive_send_style_id = Some("com.apple.MobileSMS.expressivesend.loud".to_string());
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        assert_eq!(parsed["expressive"], "loud");
    }
}

#[cfg(test)]
mod balloon_format_tests {
    use std::collections::HashMap;

    use crate::{
        Config, Exporter, JSON, Options,
        app::export_type::ExportType::Json,
        exporters::exporter::BalloonFormatter,
    };
    use imessage_database::message_types::{
        app::AppMessage,
        app_store::AppStoreMessage,
        collaboration::CollaborationMessage,
        music::MusicMessage,
        placemark::{Placemark, PlacemarkMessage},
        polls::{Poll, PollOption, PollOptionID, PollVote},
        url::URLMessage,
    };

    #[test]
    fn can_format_json_url() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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

        let result = exporter.format_url(&Config::fake_message(), &balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "url");
        assert_eq!(parsed["title"], "title");
        assert_eq!(parsed["caption"], "summary");
        assert_eq!(parsed["url"], "url");
    }

    #[test]
    fn can_format_json_music() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let balloon = MusicMessage {
            url: Some("url"),
            preview: Some("preview"),
            artist: Some("artist"),
            album: Some("album"),
            track_name: Some("track_name"),
            lyrics: None,
        };

        let result = exporter.format_music(&balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "music");
        assert_eq!(parsed["title"], "track_name");
        assert_eq!(parsed["subtitle"], "artist");
        assert_eq!(parsed["caption"], "album");
        assert_eq!(parsed["url"], "url");
    }

    #[test]
    fn can_format_json_music_lyrics() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let balloon = MusicMessage {
            url: Some("url"),
            preview: None,
            artist: Some("artist"),
            album: Some("album"),
            track_name: Some("track_name"),
            lyrics: Some(vec!["line1", "line2"]),
        };

        let result = exporter.format_music(&balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "music");
        // extra fields are flattened to the same level
        assert_eq!(parsed["lyrics"], "line1\nline2");
    }

    #[test]
    fn can_format_json_collaboration() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let balloon = CollaborationMessage {
            original_url: Some("original_url"),
            url: Some("url"),
            title: Some("title"),
            creation_date: Some(0.),
            bundle_id: Some("bundle_id"),
            app_name: Some("app_name"),
        };

        let result = exporter.format_collaboration(&balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "collaboration");
        assert_eq!(parsed["title"], "title");
        assert_eq!(parsed["subtitle"], "app_name");
        assert_eq!(parsed["url"], "url");
    }

    #[test]
    fn can_format_json_apple_pay() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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

        let result = exporter.format_apple_pay(&balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "apple_pay");
        assert_eq!(parsed["title"], "caption");
        assert_eq!(parsed["subtitle"], "ldtext");
    }

    #[test]
    fn can_format_json_fitness() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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

        let result = exporter.format_fitness(&balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "fitness");
        assert_eq!(parsed["title"], "app_name");
        assert_eq!(parsed["subtitle"], "ldtext");
    }

    #[test]
    fn can_format_json_slideshow() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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

        let result = exporter.format_slideshow(&balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "slideshow");
        assert_eq!(parsed["title"], "ldtext");
        assert_eq!(parsed["url"], "url");
    }

    #[test]
    fn can_format_json_find_my() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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

        let result = exporter.format_find_my(&balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "find_my");
        assert_eq!(parsed["title"], "app_name");
        assert_eq!(parsed["subtitle"], "ldtext");
    }

    #[test]
    fn can_format_json_check_in() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Timer Started"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Timer Started"),
        };

        let result = exporter.format_check_in(&balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "check_in");
        assert_eq!(parsed["title"], "Check In: Timer Started");
        // extra fields are flattened to the same level
        assert!(parsed["send_date"].is_string());
    }

    #[test]
    fn can_format_json_app_store() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let balloon = AppStoreMessage {
            url: Some("url"),
            app_name: Some("app_name"),
            original_url: Some("original_url"),
            description: Some("description"),
            platform: Some("platform"),
            genre: Some("genre"),
        };

        let result = exporter.format_app_store(&balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "app_store");
        assert_eq!(parsed["title"], "app_name");
        assert_eq!(parsed["subtitle"], "description");
        assert_eq!(parsed["url"], "url");
        // extra fields are flattened to the same level
        assert_eq!(parsed["platform"], "platform");
        assert_eq!(parsed["genre"], "genre");
    }

    #[test]
    fn can_format_json_placemark() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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

        let result = exporter.format_placemark(&balloon, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "placemark");
        assert_eq!(parsed["title"], "Name");
        assert_eq!(parsed["url"], "url");
        // extra fields are flattened to the same level
        assert_eq!(parsed["name"], "name");
        assert_eq!(parsed["city"], "city");
        assert_eq!(parsed["state"], "state");
    }

    #[test]
    fn can_format_json_poll() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut poll_options: HashMap<PollOptionID, PollOption> = HashMap::new();

        let id1: PollOptionID = "1".to_string();
        let id2: PollOptionID = "2".to_string();

        poll_options.insert(
            id1.clone(),
            PollOption {
                text: "Rust".to_string(),
                creator: "alice".to_string(),
                votes: vec![PollVote {
                    voter: "carol".to_string(),
                    option_id: id1.clone(),
                }],
            },
        );

        poll_options.insert(
            id2.clone(),
            PollOption {
                text: "Go".to_string(),
                creator: "bob".to_string(),
                votes: vec![
                    PollVote {
                        voter: "alice".to_string(),
                        option_id: id2.clone(),
                    },
                    PollVote {
                        voter: "bob".to_string(),
                        option_id: id2.clone(),
                    },
                ],
            },
        );

        let poll = Poll {
            options: poll_options,
            order: vec![id1, id2],
        };

        let result = exporter.format_poll(&poll, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "poll");
        // extra fields are flattened to the same level
        assert!(parsed["Rust"].is_string());
        assert!(parsed["Go"].is_string());
    }

    #[test]
    fn can_format_json_generic_app() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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

        let result = exporter.format_generic_app(&balloon, "bundle_id", &mut vec![], "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["app_type"], "app");
        assert_eq!(parsed["title"], "title");
        assert_eq!(parsed["subtitle"], "subtitle");
        assert_eq!(parsed["caption"], "caption");
        assert_eq!(parsed["url"], "url");
        // extra fields are flattened to the same level
        assert_eq!(parsed["bundle_id"], "bundle_id");
    }
}

#[cfg(test)]
mod text_effect_tests {
    use imessage_database::{
        message_types::text_effects::{Animation, Style, TextEffect, Unit},
        tables::messages::models::{BubbleComponent, TextAttributes},
    };

    use crate::{
        Config, Exporter, JSON, Options,
        app::export_type::ExportType,
        exporters::exporter::MessageFormatter,
    };

    #[test]
    fn can_format_json_text_styles_mixed_end_to_end() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        // Check text effects are captured
        assert!(parsed["text_effects"].is_array());
        let effects = parsed["text_effects"].as_array().unwrap();
        assert_eq!(effects.len(), 2); // underline and jitter

        // Check first effect (underline)
        assert_eq!(effects[0]["effect_type"], "styles");
        assert_eq!(effects[0]["start"], 0);
        assert_eq!(effects[0]["end"], 9);

        // Check second effect (jitter)
        assert_eq!(effects[1]["effect_type"], "animated");
        assert_eq!(effects[1]["start"], 17);
        assert_eq!(effects[1]["end"], 23);
    }

    #[test]
    fn can_format_json_text_styled_plain_link() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        // Check text effects are captured
        assert!(parsed["text_effects"].is_array());
        let effects = parsed["text_effects"].as_array().unwrap();
        assert_eq!(effects.len(), 2); // animated and link

        // Verify link effect is present
        let link_effect = effects.iter().find(|e| e["effect_type"] == "link").unwrap();
        assert_eq!(
            link_effect["data"],
            "https://github.com/ReagentX/imessage-exporter/discussions/553"
        );
    }

    #[test]
    fn can_format_json_text_styled_emoji_bold_underline() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        // Check text effects are captured
        let effects = parsed["text_effects"].as_array().unwrap();
        assert_eq!(effects.len(), 2); // bold and underline

        // Verify bold effect
        let bold_effect = effects.iter().find(|e| e["data"] == "bold").unwrap();
        assert_eq!(bold_effect["effect_type"], "styles");

        // Verify underline effect
        let underline_effect = effects.iter().find(|e| e["data"] == "underline").unwrap();
        assert_eq!(underline_effect["effect_type"], "styles");
    }

    #[test]
    fn can_format_json_text_styled_overlapping_ranges() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

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
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        // Should have multiple text effects
        let effects = parsed["text_effects"].as_array().unwrap();
        assert!(!effects.is_empty());

        // Verify conversion effects are captured
        let conversion_effects: Vec<_> = effects
            .iter()
            .filter(|e| e["effect_type"] == "conversion")
            .collect();
        assert!(!conversion_effects.is_empty());
    }

    #[test]
    fn can_format_json_mention() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.text = Some("Hello @John".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Text(vec![
            TextAttributes::new(0, 6, vec![TextEffect::Default]),
            TextAttributes::new(6, 11, vec![TextEffect::Mention("John".to_string())]),
        ])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        let effects = parsed["text_effects"].as_array().unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0]["effect_type"], "mention");
        assert_eq!(effects[0]["data"], "John");
    }

    #[test]
    fn can_format_json_otp() {
        // Create exporter
        let options = Options::fake_options(ExportType::Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.text = Some("Your code is 123456".to_string());
        message.is_from_me = false;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Text(vec![
            TextAttributes::new(0, 13, vec![TextEffect::Default]),
            TextAttributes::new(13, 19, vec![TextEffect::OTP]),
        ])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        let effects = parsed["text_effects"].as_array().unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0]["effect_type"], "otp");
    }
}

#[cfg(test)]
mod edited_tests {
    use imessage_database::{
        message_types::{
            edited::{EditedEvent, EditStatus, EditedMessage, EditedMessagePart},
            text_effects::TextEffect,
        },
        tables::messages::models::{BubbleComponent, TextAttributes},
    };

    use crate::{
        Config, Exporter, JSON, Options,
        app::export_type::ExportType::Json,
        exporters::exporter::MessageFormatter,
    };

    #[test]
    fn can_format_json_edited_message() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.date_edited = 674530231992568192;
        message.text = Some("Edited text".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![
                    EditedEvent {
                        date: 674526582885055488,
                        text: Some("Original text".to_string()),
                        components: vec![],
                        guid: None,
                    },
                    EditedEvent {
                        date: 674530231992568192,
                        text: Some("Edited text".to_string()),
                        components: vec![],
                        guid: None,
                    },
                ],
            }],
        });

        message.components = vec![BubbleComponent::Text(vec![TextAttributes::new(
            0,
            11,
            vec![TextEffect::Default],
        )])];

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        // Check that edited field is present
        assert!(parsed["edited"].is_array());
        let edits = parsed["edited"].as_array().unwrap();
        assert_eq!(edits.len(), 2);

        // Check first edit (original)
        assert_eq!(edits[0]["text"], "Original text");

        // Check second edit
        assert_eq!(edits[1]["text"], "Edited text");
    }

    #[test]
    fn can_format_json_unsent_message() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.date_edited = 674530231992568192;
        message.text = Some("".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
            }],
        });

        message.components = vec![BubbleComponent::Retracted];

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        // Check that edited field is present with unsent marker
        assert!(parsed["edited"].is_array());
        let edits = parsed["edited"].as_array().unwrap();
        assert_eq!(edits.len(), 1);

        // Text should be null for unsent
        assert!(edits[0]["text"].is_null());
    }

    #[test]
    fn can_format_json_no_edits() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.text = Some("Normal message".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Text(vec![TextAttributes::new(
            0,
            14,
            vec![TextEffect::Default],
        )])];

        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        // edited field should not be present for non-edited messages
        assert!(parsed.get("edited").is_none());
    }

    #[test]
    fn can_format_json_multipart_with_unsent() {
        // Create exporter
        let options = Options::fake_options(Json);
        let config = Config::fake_app(options);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.date_edited = 674530231992568192;
        message.text = Some("Part 1\r\u{FFFC}Part 2\r".to_string());
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
            BubbleComponent::Text(vec![TextAttributes::new(0, 6, vec![TextEffect::Default])]),
            BubbleComponent::Attachment(Default::default()),
            BubbleComponent::Text(vec![TextAttributes::new(8, 14, vec![TextEffect::Default])]),
            BubbleComponent::Retracted,
        ];

        let actual = exporter.format_message(&message, 0).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&actual).unwrap();

        // Should have edited field with unsent marker
        assert!(parsed["edited"].is_array());
    }
}
