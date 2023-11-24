use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    app::{error::RuntimeError, progress::build_progress_bar_export, runtime::Config},
    exporters::exporter::{BalloonFormatter, Exporter, Writer},
};

use imessage_database::{
    error::{message::MessageError, plist::PlistParseError, table::TableError},
    message_types::{
        app::AppMessage,
        app_store::AppStoreMessage,
        collaboration::CollaborationMessage,
        edited::EditedMessage,
        expressives::{BubbleEffect, Expressive, ScreenEffect},
        handwriting::HandwrittenMessage,
        music::MusicMessage,
        placemark::PlacemarkMessage,
        url::URLMessage,
        variants::{Announcement, BalloonProvider, CustomBalloon, URLOverride, Variant},
    },
    tables::{
        attachment::Attachment,
        messages::{BubbleType, Message},
        table::{Table, FITNESS_RECEIVER, ME, ORPHANED, YOU},
    },
    util::{
        dates::{format, get_local_time, readable_diff, TIMESTAMP_FACTOR},
        plist::parse_plist,
    },
};

pub struct TXT<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config,
    /// Handles to files we want to write messages to
    /// Map of internal unique chatroom ID to a filename
    pub files: HashMap<i32, PathBuf>,
    /// Path to file for orphaned messages
    pub orphaned: PathBuf,
}

impl<'a> Exporter<'a> for TXT<'a> {
    fn new(config: &'a Config) -> Self {
        let mut orphaned = config.options.export_path.clone();
        orphaned.push(ORPHANED);
        orphaned.set_extension("txt");
        TXT {
            config,
            files: HashMap::new(),
            orphaned,
        }
    }

    fn iter_messages(&mut self) -> Result<(), RuntimeError> {
        // Tell the user what we are doing
        eprintln!(
            "Exporting to {} as txt...",
            self.config.options.export_path.display()
        );

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

            // Render the announcement in-line
            if msg.is_announcement() {
                let announcement = self.format_announcement(&msg);
                TXT::write_to_file(self.get_or_create_file(&msg), &announcement);
            }
            // Message replies and reactions are rendered in context, so no need to render them separately
            else if !msg.is_reaction() {
                let _ = msg.gen_text(&self.config.db);
                let message = self
                    .format_message(&msg, 0)
                    .map_err(RuntimeError::DatabaseError)?;
                TXT::write_to_file(self.get_or_create_file(&msg), &message);
            }
            current_message += 1;
            if current_message % 99 == 0 {
                pb.set_position(current_message);
            }
        }
        pb.finish();
        Ok(())
    }

    /// Create a file for the given chat, caching it so we don't need to build it later
    fn get_or_create_file(&mut self, message: &Message) -> &Path {
        match self.config.conversation(message) {
            Some((chatroom, id)) => self.files.entry(*id).or_insert_with(|| {
                let mut path = self.config.options.export_path.clone();
                path.push(self.config.filename(chatroom));
                path.set_extension("txt");
                path
            }),
            None => &self.orphaned,
        }
    }
}

impl<'a> Writer<'a> for TXT<'a> {
    fn format_message(&self, message: &Message, indent_size: usize) -> Result<String, TableError> {
        let indent = String::from_iter((0..indent_size).map(|_| " "));
        // Data we want to write to a file
        let mut formatted_message = String::new();

        // Add message date
        self.add_line(&mut formatted_message, &self.get_time(message), &indent);

        // Add message sender
        self.add_line(
            &mut formatted_message,
            self.config.who(message.handle_id, message.is_from_me),
            &indent,
        );

        // If message was deleted, annotate it
        if message.is_deleted() {
            self.add_line(
                &mut formatted_message,
                "This message was deleted from the conversation!",
                &indent,
            );
        }

        // Useful message metadata
        let message_parts = message.body();
        let mut attachments = Attachment::from_message(&self.config.db, message)?;
        let mut replies = message.get_replies(&self.config.db)?;

        // Index of where we are in the attachment Vector
        let mut attachment_index: usize = 0;

        // Render subject
        if let Some(subject) = &message.subject {
            self.add_line(&mut formatted_message, subject, &indent);
        }

        // If message was removed, display it
        if message_parts.is_empty() && message.is_edited() {
            let edited = match self.format_edited(message, &indent) {
                Ok(s) => s,
                Err(why) => format!("{}, {}", message.guid, why),
            };
            self.add_line(&mut formatted_message, &edited, &indent);
        }

        // Handle SharePlay
        if message.is_shareplay() {
            self.add_line(&mut formatted_message, self.format_shareplay(), &indent);
        }

        // Generate the message body from it's components
        for (idx, message_part) in message_parts.iter().enumerate() {
            // Render edited messages
            if message.is_edited() {
                let edited = match self.format_edited(message, &indent) {
                    Ok(s) => s,
                    Err(why) => format!("{}, {}", message.guid, why),
                };
                self.add_line(&mut formatted_message, &edited, &indent);
                continue;
            }
            match message_part {
                // Fitness messages have a prefix that we need to replace with the opposite if who sent the message
                BubbleType::Text(text) => {
                    if text.starts_with(FITNESS_RECEIVER) {
                        self.add_line(
                            &mut formatted_message,
                            &text.replace(FITNESS_RECEIVER, YOU),
                            &indent,
                        );
                    } else {
                        self.add_line(&mut formatted_message, text, &indent);
                    }
                }
                BubbleType::Attachment => match attachments.get_mut(attachment_index) {
                    Some(attachment) => {
                        if attachment.is_sticker {
                            let result = self.format_sticker(attachment, message);
                            self.add_line(&mut formatted_message, &result, &indent);
                        } else {
                            match self.format_attachment(attachment, message) {
                                Ok(result) => {
                                    attachment_index += 1;
                                    self.add_line(&mut formatted_message, &result, &indent);
                                }
                                Err(result) => {
                                    self.add_line(&mut formatted_message, result, &indent);
                                }
                            }
                        }
                    }
                    // Attachment does not exist in attachments table
                    None => self.add_line(&mut formatted_message, "Attachment missing!", &indent),
                },
                BubbleType::App => match self.format_app(message, &mut attachments, &indent) {
                    // We use an empty indent here because `format_app` handles building the entire message
                    Ok(ok_bubble) => self.add_line(&mut formatted_message, &ok_bubble, ""),
                    Err(why) => self.add_line(
                        &mut formatted_message,
                        &format!("Unable to format app message: {why}"),
                        &indent,
                    ),
                },
            };

            // Handle expressives
            if message.expressive_send_style_id.is_some() {
                self.add_line(
                    &mut formatted_message,
                    self.format_expressive(message),
                    &indent,
                );
            }

            // Handle Reactions
            if let Some(reactions_map) = self.config.reactions.get(&message.guid) {
                if let Some(reactions) = reactions_map.get(&idx) {
                    let mut formatted_reactions = String::new();
                    reactions
                        .iter()
                        .try_for_each(|reaction| -> Result<(), TableError> {
                            let formatted = self.format_reaction(reaction)?;
                            if !formatted.is_empty() {
                                self.add_line(
                                    &mut formatted_reactions,
                                    &self.format_reaction(reaction)?,
                                    &indent,
                                );
                            }
                            Ok(())
                        })?;

                    if !formatted_reactions.is_empty() {
                        self.add_line(&mut formatted_message, "Reactions:", &indent);
                        self.add_line(&mut formatted_message, &formatted_reactions, &indent);
                    }
                }
            }

            // Handle Replies
            if let Some(replies) = replies.get_mut(&idx) {
                replies
                    .iter_mut()
                    .try_for_each(|reply| -> Result<(), TableError> {
                        let _ = reply.gen_text(&self.config.db);
                        if !reply.is_reaction() {
                            self.add_line(
                                &mut formatted_message,
                                &self.format_message(reply, 4)?,
                                &indent,
                            );
                        }
                        Ok(())
                    })?;
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

        if indent.is_empty() {
            // Add a newline for top-level messages
            formatted_message.push('\n');
        }

        Ok(formatted_message)
    }

    fn format_attachment(
        &self,
        attachment: &'a mut Attachment,
        message: &Message,
    ) -> Result<String, &'a str> {
        // Copy the file, if requested
        self.config
            .options
            .attachment_manager
            .handle_attachment(message, attachment, self.config)
            .ok_or(attachment.filename())?;

        // Build a relative filepath from the fully qualified one on the `Attachment`
        Ok(self.config.message_attachment_path(attachment))
    }

    fn format_sticker(&self, sticker: &'a mut Attachment, message: &Message) -> String {
        let who = self.config.who(message.handle_id, message.is_from_me);
        match self.format_attachment(sticker, message) {
            Ok(path_to_sticker) => {
                let sticker_effect = sticker.get_sticker_effect(
                    &self.config.options.platform,
                    &self.config.options.db_path,
                    self.config.options.attachment_root.as_deref(),
                );
                if let Ok(Some(sticker_effect)) = sticker_effect {
                    return format!("{sticker_effect} Sticker from {who}: {path_to_sticker}");
                }
                format!("Sticker from {who}: {path_to_sticker}")
            }
            Err(path) => format!("Sticker from {who}: {path}"),
        }
    }

    fn format_app(
        &self,
        message: &'a Message,
        attachments: &mut Vec<Attachment>,
        indent: &str,
    ) -> Result<String, PlistParseError> {
        if let Variant::App(balloon) = message.variant() {
            let mut app_bubble = String::new();

            // Handwritten messages use a different payload type, so handle that first
            if matches!(balloon, CustomBalloon::Handwriting) {
                return Ok(self.format_handwriting(&HandwrittenMessage::new(), indent));
            }

            if let Some(payload) = message.payload_data(&self.config.db) {
                // Handle URL messages separately since they are a special case
                let res = if message.is_url() {
                    let parsed = parse_plist(&payload)?;
                    let bubble = URLMessage::get_url_message_override(&parsed)?;
                    match bubble {
                        URLOverride::Normal(balloon) => self.format_url(&balloon, indent),
                        URLOverride::AppleMusic(balloon) => self.format_music(&balloon, indent),
                        URLOverride::Collaboration(balloon) => {
                            self.format_collaboration(&balloon, indent)
                        }
                        URLOverride::AppStore(balloon) => self.format_app_store(&balloon, indent),
                        URLOverride::SharedPlacemark(balloon) => {
                            self.format_placemark(&balloon, indent)
                        }
                    }
                // Handwriting uses a different payload type than the rest of the branches
                } else {
                    // Handle the app case
                    let parsed = parse_plist(&payload)?;
                    match AppMessage::from_map(&parsed) {
                        Ok(bubble) => match balloon {
                            CustomBalloon::Application(bundle_id) => {
                                self.format_generic_app(&bubble, bundle_id, attachments, indent)
                            }
                            CustomBalloon::ApplePay => self.format_apple_pay(&bubble, indent),
                            CustomBalloon::Fitness => self.format_fitness(&bubble, indent),
                            CustomBalloon::Slideshow => self.format_slideshow(&bubble, indent),
                            CustomBalloon::CheckIn => self.format_check_in(&bubble, indent),
                            CustomBalloon::FindMy => self.format_find_my(&bubble, indent),
                            CustomBalloon::Handwriting => unreachable!(),
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
                        return Ok(text.to_string());
                    }
                }
                return Err(PlistParseError::NoPayload);
            };
            Ok(app_bubble)
        } else {
            Err(PlistParseError::WrongMessageType)
        }
    }

    fn format_reaction(&self, msg: &Message) -> Result<String, TableError> {
        match msg.variant() {
            Variant::Reaction(_, added, reaction) => {
                if !added {
                    return Ok(String::new());
                }
                Ok(format!(
                    "{:?} by {}",
                    reaction,
                    self.config.who(msg.handle_id, msg.is_from_me),
                ))
            }
            Variant::Sticker(_) => {
                let mut paths = Attachment::from_message(&self.config.db, msg)?;
                let who = self.config.who(msg.handle_id, msg.is_from_me);
                // Sticker messages have only one attachment, the sticker image
                Ok(if let Some(sticker) = paths.get_mut(0) {
                    self.format_sticker(sticker, msg)
                } else {
                    format!("Sticker from {who} not found!")
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
        let mut who = self.config.who(msg.handle_id, msg.is_from_me);
        // Rename yourself so we render the proper grammar here
        if who == ME {
            who = self.config.options.custom_name.as_deref().unwrap_or(YOU);
        }

        let timestamp = format(&msg.date(&self.config.offset));

        return match msg.get_announcement() {
            Some(announcement) => match announcement {
                Announcement::NameChange(name) => {
                    format!("{timestamp} {who} renamed the conversation to {name}\n\n")
                }
                Announcement::PhotoChange => {
                    format!("{timestamp} {who} changed the group photo.\n\n")
                }
                Announcement::Unknown(num) => {
                    format!("{timestamp} {who} performed unknown action {num}.\n\n")
                }
            },
            None => String::from("Unable to format announcement!\n\n"),
        };
    }

    fn format_shareplay(&self) -> &str {
        "SharePlay Message\nEnded"
    }

    fn format_edited(&self, msg: &'a Message, indent: &str) -> Result<String, MessageError> {
        if let Some(payload) = msg.message_summary_info(&self.config.db) {
            // Parse the edited message
            let edited_message =
                EditedMessage::from_map(&payload).map_err(MessageError::PlistParseError)?;

            let mut out_s = String::new();
            let mut previous_timestamp: Option<&i64> = None;

            if edited_message.is_deleted() {
                let who = if msg.is_from_me {
                    self.config.options.custom_name.as_deref().unwrap_or(YOU)
                } else {
                    "They"
                };
                out_s.push_str(who);
                out_s.push_str(" deleted a message.");
            } else {
                for i in 0..edited_message.items() {
                    // If a message exists, build a string for it
                    if let Some((timestamp, text, _)) = edited_message.item_at(i) {
                        match previous_timestamp {
                            // Original message get an absolute timestamp
                            None => {
                                let parsed_timestamp =
                                    format(&get_local_time(timestamp, &self.config.offset));
                                out_s.push_str(&parsed_timestamp);
                                out_s.push(' ');
                            }
                            // Subsequent edits get a relative timestamp
                            Some(prev_timestamp) => {
                                let end = get_local_time(timestamp, &self.config.offset);
                                let start = get_local_time(prev_timestamp, &self.config.offset);
                                if let Some(diff) = readable_diff(start, end) {
                                    out_s.push_str(indent);
                                    out_s.push_str("Edited ");
                                    out_s.push_str(&diff);
                                    out_s.push_str(" later: ");
                                }
                            }
                        };

                        // Update the previous timestamp for the next loop
                        previous_timestamp = Some(timestamp);

                        // Render the message text
                        self.add_line(&mut out_s, text, indent);
                    }
                }
            }

            return Ok(out_s);
        }
        Err(MessageError::PlistParseError(PlistParseError::NoPayload))
    }

    fn write_to_file(file: &Path, text: &str) {
        match File::options().append(true).create(true).open(file) {
            Ok(mut file) => {
                let _ = file.write_all(text.as_bytes());
            }
            Err(why) => eprintln!("Unable to write to {file:?}: {why:?}"),
        };
    }
}

impl<'a> BalloonFormatter<&'a str> for TXT<'a> {
    fn format_url(&self, balloon: &URLMessage, indent: &str) -> String {
        let mut out_s = String::new();

        if let Some(url) = balloon.get_url() {
            self.add_line(&mut out_s, url, indent);
        }

        if let Some(title) = balloon.title {
            self.add_line(&mut out_s, title, indent);
        }

        if let Some(summary) = balloon.summary {
            self.add_line(&mut out_s, summary, indent);
        }

        // We want to keep the newlines between blocks, but the last one should be removed
        out_s.strip_suffix('\n').unwrap_or(&out_s).to_string()
    }

    fn format_music(&self, balloon: &MusicMessage, indent: &str) -> String {
        let mut out_s = String::new();

        if let Some(track_name) = balloon.track_name {
            self.add_line(&mut out_s, track_name, indent);
        }

        if let Some(album) = balloon.album {
            self.add_line(&mut out_s, album, indent);
        }

        if let Some(artist) = balloon.artist {
            self.add_line(&mut out_s, artist, indent);
        }

        if let Some(url) = balloon.url {
            self.add_line(&mut out_s, url, indent);
        }

        out_s
    }

    fn format_collaboration(&self, balloon: &CollaborationMessage, indent: &str) -> String {
        let mut out_s = String::from(indent);

        if let Some(name) = balloon.app_name {
            out_s.push_str(name);
        } else if let Some(bundle_id) = balloon.bundle_id {
            out_s.push_str(bundle_id);
        }

        if !out_s.is_empty() {
            out_s.push_str(" message:\n");
        }

        if let Some(title) = balloon.title {
            self.add_line(&mut out_s, title, indent);
        }

        if let Some(url) = balloon.get_url() {
            self.add_line(&mut out_s, url, indent);
        }

        // We want to keep the newlines between blocks, but the last one should be removed
        out_s.strip_suffix('\n').unwrap_or(&out_s).to_string()
    }

    fn format_app_store(&self, balloon: &AppStoreMessage, indent: &'a str) -> String {
        let mut out_s = String::from(indent);

        if let Some(name) = balloon.app_name {
            self.add_line(&mut out_s, name, indent);
        }

        if let Some(description) = balloon.description {
            self.add_line(&mut out_s, description, indent);
        }

        if let Some(platform) = balloon.platform {
            self.add_line(&mut out_s, platform, indent);
        }

        if let Some(genre) = balloon.genre {
            self.add_line(&mut out_s, genre, indent);
        }

        if let Some(url) = balloon.url {
            self.add_line(&mut out_s, url, indent);
        }

        // We want to keep the newlines between blocks, but the last one should be removed
        out_s.strip_suffix('\n').unwrap_or(&out_s).to_string()
    }

    fn format_placemark(&self, balloon: &PlacemarkMessage, indent: &'a str) -> String {
        let mut out_s = String::from(indent);

        if let Some(name) = balloon.place_name {
            self.add_line(&mut out_s, name, indent);
        }

        if let Some(url) = balloon.get_url() {
            self.add_line(&mut out_s, url, indent);
        }

        if let Some(name) = balloon.placemark.name {
            self.add_line(&mut out_s, name, indent);
        }

        if let Some(address) = balloon.placemark.address {
            self.add_line(&mut out_s, address, indent);
        }

        if let Some(state) = balloon.placemark.state {
            self.add_line(&mut out_s, state, indent);
        }

        if let Some(city) = balloon.placemark.city {
            self.add_line(&mut out_s, city, indent);
        }

        if let Some(iso_country_code) = balloon.placemark.iso_country_code {
            self.add_line(&mut out_s, iso_country_code, indent);
        }

        if let Some(postal_code) = balloon.placemark.postal_code {
            self.add_line(&mut out_s, postal_code, indent);
        }

        if let Some(country) = balloon.placemark.country {
            self.add_line(&mut out_s, country, indent);
        }

        if let Some(street) = balloon.placemark.street {
            self.add_line(&mut out_s, street, indent);
        }

        if let Some(sub_administrative_area) = balloon.placemark.sub_administrative_area {
            self.add_line(&mut out_s, sub_administrative_area, indent);
        }

        if let Some(sub_locality) = balloon.placemark.sub_locality {
            self.add_line(&mut out_s, sub_locality, indent);
        }

        // We want to keep the newlines between blocks, but the last one should be removed
        out_s.strip_suffix('\n').unwrap_or(&out_s).to_string()
    }

    fn format_handwriting(&self, _: &HandwrittenMessage, indent: &str) -> String {
        format!("{indent}Handwritten messages are not yet supported!")
    }

    fn format_apple_pay(&self, balloon: &AppMessage, indent: &str) -> String {
        let mut out_s = String::from(indent);
        if let Some(caption) = balloon.caption {
            out_s.push_str(caption);
            out_s.push_str(" transaction: ");
        }

        if let Some(ldtext) = balloon.ldtext {
            out_s.push_str(ldtext);
        } else {
            out_s.push_str("unknown amount");
        }

        out_s
    }

    fn format_fitness(&self, balloon: &AppMessage, indent: &str) -> String {
        let mut out_s = String::from(indent);
        if let Some(app_name) = balloon.app_name {
            out_s.push_str(app_name);
            out_s.push_str(" message: ");
        }
        if let Some(ldtext) = balloon.ldtext {
            out_s.push_str(ldtext);
        } else {
            out_s.push_str("unknown workout");
        }
        out_s
    }

    fn format_slideshow(&self, balloon: &AppMessage, indent: &str) -> String {
        let mut out_s = String::from(indent);
        if let Some(ldtext) = balloon.ldtext {
            out_s.push_str("Photo album: ");
            out_s.push_str(ldtext);
        }

        if let Some(url) = balloon.url {
            out_s.push(' ');
            out_s.push_str(url);
        }

        out_s
    }

    fn format_find_my(&self, balloon: &AppMessage, indent: &'a str) -> String {
        let mut out_s = String::from(indent);
        if let Some(app_name) = balloon.app_name {
            out_s.push_str(app_name);
            out_s.push_str(": ");
        }

        if let Some(ldtext) = balloon.ldtext {
            out_s.push(' ');
            out_s.push_str(ldtext);
        }

        out_s
    }

    fn format_check_in(&self, balloon: &AppMessage, indent: &'a str) -> String {
        let mut out_s = String::from(indent);

        out_s.push_str(balloon.caption.unwrap_or("Check In"));

        let metadata: HashMap<&str, &str> = balloon.parse_query_string();

        // Before manual check-in
        if let Some(date_str) = metadata.get("estimatedEndTime") {
            // Parse the estimated end time from the message's query string
            let date_stamp = date_str.parse::<f64>().unwrap_or(0.) as i64 * TIMESTAMP_FACTOR;
            let date_time = get_local_time(&date_stamp, &0);
            let date_string = format(&date_time);

            out_s.push_str("\nExpected at ");
            out_s.push_str(&date_string);
        }
        // Expired check-in
        else if let Some(date_str) = metadata.get("triggerTime") {
            // Parse the estimated end time from the message's query string
            let date_stamp = date_str.parse::<f64>().unwrap_or(0.) as i64 * TIMESTAMP_FACTOR;
            let date_time = get_local_time(&date_stamp, &0);
            let date_string = format(&date_time);

            out_s.push_str("\nWas expected at ");
            out_s.push_str(&date_string);
        }
        // Accepted check-in
        else if let Some(date_str) = metadata.get("sendDate") {
            // Parse the estimated end time from the message's query string
            let date_stamp = date_str.parse::<f64>().unwrap_or(0.) as i64 * TIMESTAMP_FACTOR;
            let date_time = get_local_time(&date_stamp, &0);
            let date_string = format(&date_time);

            out_s.push_str("\nChecked in at ");
            out_s.push_str(&date_string);
        }

        out_s
    }

    fn format_generic_app(
        &self,
        balloon: &AppMessage,
        bundle_id: &str,
        _: &mut Vec<Attachment>,
        indent: &str,
    ) -> String {
        let mut out_s = String::from(indent);

        if let Some(name) = balloon.app_name {
            out_s.push_str(name);
        } else {
            out_s.push_str(bundle_id);
        }

        if !out_s.is_empty() {
            out_s.push_str(" message:\n");
        }

        if let Some(title) = balloon.title {
            self.add_line(&mut out_s, title, indent);
        }

        if let Some(subtitle) = balloon.subtitle {
            self.add_line(&mut out_s, subtitle, indent);
        }

        if let Some(caption) = balloon.caption {
            self.add_line(&mut out_s, caption, indent);
        }

        if let Some(subcaption) = balloon.subcaption {
            self.add_line(&mut out_s, subcaption, indent);
        }

        if let Some(trailing_caption) = balloon.trailing_caption {
            self.add_line(&mut out_s, trailing_caption, indent);
        }

        if let Some(trailing_subcaption) = balloon.trailing_subcaption {
            self.add_line(&mut out_s, trailing_subcaption, indent);
        }

        // We want to keep the newlines between blocks, but the last one should be removed
        out_s.strip_suffix('\n').unwrap_or(&out_s).to_string()
    }
}

impl<'a> TXT<'a> {
    fn get_time(&self, message: &Message) -> String {
        let mut date = format(&message.date(&self.config.offset));
        let read_after = message.time_until_read(&self.config.offset);
        if let Some(time) = read_after {
            if !time.is_empty() {
                let who = if message.is_from_me {
                    "them"
                } else {
                    self.config.options.custom_name.as_deref().unwrap_or("you")
                };
                date.push_str(&format!(" (Read by {who} after {time})"));
            }
        }
        date
    }

    fn add_line(&self, string: &mut String, part: &str, indent: &str) {
        if !part.is_empty() {
            string.push_str(indent);
            string.push_str(part);
            string.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env::{current_dir, set_var},
        path::PathBuf,
    };

    use crate::{
        app::attachment_manager::AttachmentManager, exporters::exporter::Writer, Config, Exporter,
        Options, TXT,
    };
    use imessage_database::{
        tables::{attachment::Attachment, messages::Message},
        util::{dirs::default_db_path, platform::Platform, query_context::QueryContext},
    };

    fn blank() -> Message {
        Message {
            rowid: i32::default(),
            guid: String::default(),
            text: None,
            service: Some("iMessage".to_string()),
            handle_id: Some(i32::default()),
            subject: None,
            date: i64::default(),
            date_read: i64::default(),
            date_delivered: i64::default(),
            is_from_me: false,
            is_read: false,
            item_type: 0,
            group_title: None,
            group_action_type: 0,
            associated_message_guid: None,
            associated_message_type: Some(i32::default()),
            balloon_bundle_id: None,
            expressive_send_style_id: None,
            thread_originator_guid: None,
            thread_originator_part: None,
            date_edited: 0,
            chat_id: None,
            num_attachments: 0,
            deleted_from: None,
            num_replies: 0,
        }
    }

    pub fn fake_options() -> Options {
        Options {
            db_path: default_db_path(),
            attachment_root: None,
            attachment_manager: AttachmentManager::Disabled,
            diagnostic: false,
            export_type: None,
            export_path: PathBuf::new(),
            query_context: QueryContext::default(),
            no_lazy: false,
            custom_name: None,
            platform: Platform::macOS,
            ignore_disk_space: false,
        }
    }

    pub fn fake_attachment() -> Attachment {
        Attachment {
            rowid: 0,
            filename: Some("a/b/c/d.jpg".to_string()),
            uti: Some("public.png".to_string()),
            mime_type: Some("image/png".to_string()),
            transfer_name: Some("d.jpg".to_string()),
            total_bytes: 100,
            is_sticker: false,
            hide_attachment: 0,
            copied_path: None,
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
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

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
            "May 17, 2022  5:29:42 PM (Read by you after 1 hour, 49 seconds)"
        );
    }

    #[test]
    fn can_get_time_invalid() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

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
        assert_eq!(exporter.get_time(&message), "May 17, 2022  6:30:31 PM");
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

    #[test]
    fn can_format_txt_from_me_normal() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let mut message = blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "May 17, 2022  5:29:42 PM\nMe\nHello world\n\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_from_me_normal_deleted() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let mut message = blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        message.is_from_me = true;
        message.deleted_from = Some(0);

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "May 17, 2022  5:29:42 PM\nMe\nThis message was deleted from the conversation!\nHello world\n\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_from_me_normal_read() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let mut message = blank();
        message.text = Some("Hello world".to_string());
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_delivered = 674530231992568192;
        message.is_from_me = true;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected =
            "May 17, 2022  5:29:42 PM (Read by them after 1 hour, 49 seconds)\nMe\nHello world\n\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_from_them_normal() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let mut config = Config::new(options).unwrap();
        config
            .participants
            .insert(999999, "Sample Contact".to_string());
        let exporter = TXT::new(&config);

        let mut message = blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        message.handle_id = Some(999999);

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "May 17, 2022  5:29:42 PM\nSample Contact\nHello world\n\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_from_them_normal_read() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let mut config = Config::new(options).unwrap();
        config
            .participants
            .insert(999999, "Sample Contact".to_string());
        let exporter = TXT::new(&config);

        let mut message = blank();
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
            "May 17, 2022  5:29:42 PM (Read by you after 1 hour, 49 seconds)\nSample Contact\nHello world\n\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_from_them_custom_name_read() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let mut options = fake_options();
        options.custom_name = Some("Name".to_string());
        let mut config = Config::new(options).unwrap();
        config
            .participants
            .insert(999999, "Sample Contact".to_string());
        let exporter = TXT::new(&config);

        let mut message = blank();
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
            "May 17, 2022  5:29:42 PM (Read by Name after 1 hour, 49 seconds)\nSample Contact\nHello world\n\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_shareplay() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let mut message = blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.item_type = 6;

        let actual = exporter.format_message(&message, 0).unwrap();
        let expected = "May 17, 2022  5:29:42 PM\nMe\nSharePlay Message\nEnded\n\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_announcement() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let mut message = blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());

        let actual = exporter.format_announcement(&message);
        let expected = "May 17, 2022  5:29:42 PM You renamed the conversation to Hello world\n\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_announcement_custom_name() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let mut options = fake_options();
        options.custom_name = Some("Name".to_string());
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let mut message = blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());

        let actual = exporter.format_announcement(&message);
        let expected = "May 17, 2022  5:29:42 PM Name renamed the conversation to Hello world\n\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_reaction_me() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let mut message = blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2000);
        message.associated_message_guid = Some("fake_guid".to_string());

        let actual = exporter.format_reaction(&message).unwrap();
        let expected = "Loved by Me";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_reaction_them() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let mut config = Config::new(options).unwrap();
        config
            .participants
            .insert(999999, "Sample Contact".to_string());
        let exporter = TXT::new(&config);

        let mut message = blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2000);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);

        let actual = exporter.format_reaction(&message).unwrap();
        let expected = "Loved by Sample Contact";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_txt_attachment_macos() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let message = blank();

        let mut attachment = fake_attachment();

        let actual = exporter
            .format_attachment(&mut attachment, &message)
            .unwrap();

        assert_eq!(actual, "a/b/c/d.jpg");
    }

    #[test]
    fn can_format_txt_attachment_macos_invalid() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let message = blank();

        let mut attachment = fake_attachment();
        attachment.filename = None;

        let actual = exporter.format_attachment(&mut attachment, &message);

        assert_eq!(actual, Err("d.jpg"));
    }

    #[test]
    fn can_format_txt_attachment_ios() {
        // Create exporter
        let options = fake_options();
        let mut config = Config::new(options).unwrap();
        config.options.platform = Platform::iOS;
        let exporter = TXT::new(&config);

        let message = blank();

        let mut attachment = fake_attachment();

        let actual = exporter
            .format_attachment(&mut attachment, &message)
            .unwrap();

        assert!(actual.ends_with("33/33c81da8ae3194fc5a0ea993ef6ffe0b048baedb"));
    }

    #[test]
    fn can_format_txt_attachment_ios_invalid() {
        // Create exporter
        let options = fake_options();
        let mut config = Config::new(options).unwrap();
        // Modify this
        config.options.platform = Platform::iOS;
        let exporter = TXT::new(&config);

        let message = blank();

        let mut attachment = fake_attachment();
        attachment.filename = None;

        let actual = exporter.format_attachment(&mut attachment, &message);

        assert_eq!(actual, Err("d.jpg"));
    }

    #[test]
    fn can_format_txt_attachment_sticker() {
        // Create exporter
        let mut options = fake_options();
        options.export_path = current_dir().unwrap().parent().unwrap().to_path_buf();

        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let mut message = blank();
        // Set message to sticker variant
        message.associated_message_type = Some(1000);

        let mut attachment = fake_attachment();
        attachment.is_sticker = true;
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        attachment.filename = Some(sticker_path.to_string_lossy().to_string());
        attachment.copied_path = Some(PathBuf::from(sticker_path.to_string_lossy().to_string()));

        let actual = exporter.format_sticker(&mut attachment, &message);

        assert_eq!(
            actual,
            "Outline Sticker from Me: imessage-database/test_data/stickers/outline.heic"
        );
    }
}

#[cfg(test)]
mod balloon_format_tests {
    use std::env::set_var;

    use super::tests::fake_options;
    use crate::{exporters::exporter::BalloonFormatter, Config, Exporter, TXT};
    use imessage_database::message_types::{
        app::AppMessage,
        app_store::AppStoreMessage,
        collaboration::CollaborationMessage,
        music::MusicMessage,
        placemark::{Placemark, PlacemarkMessage},
        url::URLMessage,
    };

    #[test]
    fn can_format_txt_url() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

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

        let expected = exporter.format_url(&balloon, "");
        let actual = "url\ntitle\nsummary";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_music() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let balloon = MusicMessage {
            url: Some("url"),
            preview: Some("preview"),
            artist: Some("artist"),
            album: Some("album"),
            track_name: Some("track_name"),
        };

        let expected = exporter.format_music(&balloon, "");
        let actual = "track_name\nalbum\nartist\nurl\n";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_collaboration() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let balloon = CollaborationMessage {
            original_url: Some("original_url"),
            url: Some("url"),
            title: Some("title"),
            creation_date: Some(0.),
            bundle_id: Some("bundle_id"),
            app_name: Some("app_name"),
        };

        let expected = exporter.format_collaboration(&balloon, "");
        let actual = "app_name message:\ntitle\nurl";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_apple_pay() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

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

        let expected = exporter.format_apple_pay(&balloon, "");
        let actual = "caption transaction: ldtext";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_fitness() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

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

        let expected = exporter.format_fitness(&balloon, "");
        let actual = "app_name message: ldtext";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_slideshow() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

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

        let expected = exporter.format_slideshow(&balloon, "");
        let actual = "Photo album: ldtext url";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_find_my() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

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

        let expected = exporter.format_find_my(&balloon, "");
        let actual = "app_name:  ldtext";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_check_in_timer() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

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

        let expected = exporter.format_check_in(&balloon, "");
        let actual = "Check\u{a0}In: Timer Started\nChecked in at Oct 14, 2023  1:54:29 PM";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_check_in_timer_late() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

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

        let expected = exporter.format_check_in(&balloon, "");
        let actual = "Check\u{a0}In: Has not checked in when expected, location shared\nChecked in at Oct 14, 2023  1:54:29 PM";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_accepted_check_in() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

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

        let expected = exporter.format_check_in(&balloon, "");
        let actual = "Check\u{a0}In: Fake Location\nChecked in at Oct 14, 2023  1:54:29 PM";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_app_store() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

        let balloon = AppStoreMessage {
            url: Some("url"),
            app_name: Some("app_name"),
            original_url: Some("original_url"),
            description: Some("description"),
            platform: Some("platform"),
            genre: Some("genre"),
        };

        let expected = exporter.format_app_store(&balloon, "");
        let actual = "app_name\ndescription\nplatform\ngenre\nurl";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_placemark() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

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

        let expected = exporter.format_placemark(&balloon, "");
        let actual = "Name\nurl\nname\naddress\nstate\ncity\niso_country_code\npostal_code\ncountry\nstreet\nsub_administrative_area\nsub_locality";

        assert_eq!(expected, actual);
    }

    #[test]
    fn can_format_txt_generic_app() {
        // Create exporter
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = TXT::new(&config);

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

        let expected = exporter.format_generic_app(&balloon, "bundle_id", &mut vec![], "");
        let actual = "app_name message:\ntitle\nsubtitle\ncaption\nsubcaption\ntrailing_caption\ntrailing_subcaption";

        assert_eq!(expected, actual);
    }
}
