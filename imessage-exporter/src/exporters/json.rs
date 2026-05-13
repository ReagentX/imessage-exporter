use std::{
    collections::HashMap,
    fs::File,
    io::{BufWriter, Write},
    time::{SystemTime, UNIX_EPOCH},
};

use jzon::JsonValue;

use imessage_database::{
    error::table::TableError,
    message_types::variants::{Announcement, CustomBalloon, TapbackAction, Variant},
    tables::{
        attachment::{Attachment, MediaType},
        messages::{
            Message,
            models::GroupAction,
        },
        table::{ME, ORPHANED, UNKNOWN, Table},
    },
    util::dates::TIMESTAMP_FACTOR,
};

use crate::app::{error::RuntimeError, progress::ExportProgress, runtime::Config};
use crate::exporters::exporter::Exporter;

// ChatLab message type codes (spec v0.0.2)
const TYPE_TEXT: u8 = 0;
const TYPE_IMAGE: u8 = 1;
const TYPE_VOICE: u8 = 2;
const TYPE_VIDEO: u8 = 3;
const TYPE_FILE: u8 = 4;
const TYPE_EMOJI: u8 = 5;
const TYPE_LINK: u8 = 7;
const TYPE_CALL: u8 = 23;
const TYPE_SYSTEM: u8 = 80;
const TYPE_RECALL: u8 = 81;
const TYPE_OTHER: u8 = 99;

const CHATLAB_VERSION: &str = "0.0.2";
const PLATFORM: &str = "imessage";
const GENERATOR: &str = "imessage-exporter";

/// A single message in ChatLab wire format
#[derive(Clone)]
pub(crate) struct ChatLabMessage {
    sender_id: String,
    account_name: String,
    timestamp: i64,
    msg_type: u8,
    content: Option<String>,
    platform_message_id: String,
    reply_to_id: Option<String>,
}

/// All data for one conversation, buffered during iteration
pub(crate) struct ConversationBuffer {
    chat_name: String,
    chat_type: &'static str, // "group" or "private"
    owner_id: String,
    /// Ordered (platformId, displayName, avatarDataUrl). Owner is always index 0.
    members: Vec<(String, String, Option<String>)>,
    messages: Vec<ChatLabMessage>,
    /// Pre-encoded base64 Data URL for the group photo, when available
    group_avatar_url: Option<String>,
}

impl ConversationBuffer {
    /// Append member if not already present (linear scan — member count is tiny)
    fn add_member(&mut self, platform_id: String, display_name: String, avatar_url: Option<String>) {
        if !self.members.iter().any(|(id, _, _)| id == &platform_id) {
            self.members.push((platform_id, display_name, avatar_url));
        }
    }
}

pub struct JSON<'a> {
    pub config: &'a Config,
    pub(crate) conversations: HashMap<i32, ConversationBuffer>,
    pub(crate) orphaned: Vec<ChatLabMessage>,
    pb: ExportProgress,
}

// ─── Attachment formatting helpers ───────────────────────────────────────────

/// Human-readable label for an attachment-bearing message type.
fn content_label(t: u8) -> &'static str {
    match t {
        TYPE_IMAGE => "[Image]",
        TYPE_VOICE => "[Voice]",
        TYPE_VIDEO => "[Video]",
        TYPE_FILE  => "[File]",
        TYPE_EMOJI => "[Sticker]",
        _          => "[Other]",
    }
}

/// Compose `content` for an attachment-bearing message.
///
/// Shape: `"[Label]"`, optionally followed by `" <path>"`, optionally followed by
/// `" \u{2014} <caption>"` when the user typed text alongside the attachment.
fn compose_attachment_content(t: u8, path: Option<&str>, caption: Option<&str>) -> String {
    let label = content_label(t);
    let mut out = match path {
        Some(p) if !p.is_empty() => format!("{label} {p}"),
        _ => label.to_string(),
    };
    if let Some(c) = caption
        && !c.is_empty()
    {
        out.push_str(" \u{2014} ");
        out.push_str(c);
    }
    out
}

/// Compose voice-message `content` with an optional transcription suffix and an optional
/// user-typed caption (rare — voice messages usually have no accompanying text).
fn compose_voice_content(
    path: &str,
    transcription: Option<&str>,
    caption: Option<&str>,
) -> String {
    let label = content_label(TYPE_VOICE);
    let mut out = if path.is_empty() {
        label.to_string()
    } else {
        format!("{label} {path}")
    };
    if let Some(t) = transcription
        && !t.is_empty()
    {
        out.push_str(" \u{2014} Transcription: ");
        out.push_str(t);
    }
    if let Some(c) = caption
        && !c.is_empty()
    {
        out.push_str(" \u{2014} ");
        out.push_str(c);
    }
    out
}

/// Extract a human-readable caption from a message's text, stripping Object Replacement
/// Characters (`U+FFFC`) that mark attachment positions and collapsing surrounding
/// whitespace.  Returns `None` if nothing is left after cleanup.
fn caption_from_message(msg: &Message) -> Option<String> {
    let raw = msg.text.as_deref()?;
    let cleaned: String = raw
        .replace('\u{FFFC}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

impl<'a> JSON<'a> {
    /// Owner's platform ID for a given message context
    fn owner_id(config: &Config, msg: &Message) -> String {
        if config.options.use_caller_id {
            msg.destination_caller_id
                .clone()
                .unwrap_or_else(|| ME.to_string())
        } else {
            config.options.custom_name
                .clone()
                .unwrap_or_else(|| ME.to_string())
        }
    }

    /// Sender's platform ID: owner ID for outgoing, Handle.id for incoming
    fn sender_platform_id(&self, msg: &Message) -> String {
        if msg.is_from_me() {
            return Self::owner_id(self.config, msg);
        }
        if let Some(handle_id) = msg.handle_id {
            if let Some(&internal_id) = self.config.real_participants.get(&handle_id) {
                if let Some(name) = self.config.participants.get(&internal_id) {
                    return name.details.clone();
                }
            }
            return UNKNOWN.to_string();
        }
        ME.to_string()
    }

    /// Classify a message into a (ChatLab type code, content string) pair.
    ///
    /// Precedence: announcement → tapback → app variant → attachment → plain text.
    pub(crate) fn classify(&self, msg: &Message) -> Result<(u8, Option<String>), TableError> {
        // Announcements (includes fully-unsent/recalled)
        if msg.is_announcement() {
            return match msg.get_announcement() {
                Some(Announcement::FullyUnsent) => Ok((TYPE_RECALL, None)),
                Some(Announcement::GroupAction(action)) => Ok((
                    TYPE_SYSTEM,
                    Some(Self::group_action_text(self.config, msg, &action)),
                )),
                Some(Announcement::AudioMessageKept) => {
                    Ok((TYPE_SYSTEM, Some("Kept audio message".to_string())))
                }
                _ => Ok((TYPE_SYSTEM, None)),
            };
        }

        // Tapbacks
        if msg.is_tapback() {
            return Ok((TYPE_OTHER, Self::tapback_text(self.config, msg)));
        }

        // App balloon variants
        match msg.variant() {
            Variant::SharePlay => return Ok((TYPE_CALL, msg.text.clone())),
            Variant::App(CustomBalloon::URL) => return Ok((TYPE_LINK, msg.text.clone())),
            Variant::App(_) => return Ok((TYPE_OTHER, msg.text.clone())),
            _ => {}
        }

        // Attachment-based messages
        let mut attachments = Attachment::from_message(self.config.data_source.db(), msg)?;
        if !attachments.is_empty() {
            // Copy/transcode EVERY attachment (no-op when -c disabled) so multi-attachment
            // messages don't silently lose files alongside the JSON.  ChatLab `content` is
            // a single string field, so classification below uses the first attachment as
            // the representative.
            for att in attachments.iter_mut() {
                let _ = self.config.options.attachment_manager.handle_attachment(
                    msg,
                    att,
                    self.config,
                );
            }
            let first = &attachments[0];

            let t = if first.is_sticker {
                TYPE_EMOJI
            } else {
                match first.mime_type() {
                    MediaType::Image(_) => TYPE_IMAGE,
                    MediaType::Audio(_) => TYPE_VOICE,
                    MediaType::Video(_) => TYPE_VIDEO,
                    _ => TYPE_FILE,
                }
            };

            // Use the relative path when the file was copied; otherwise just the filename.
            let path_str: Option<String> = if first.copied_path.is_some() {
                Some(self.config.message_attachment_path(first))
            } else {
                first.filename().map(|s| {
                    // Strip directory components to avoid leaking source paths
                    std::path::Path::new(s)
                        .file_name()
                        .and_then(|os| os.to_str())
                        .unwrap_or(s)
                        .to_string()
                })
            };

            // Preserve user-typed text that accompanies the attachment (e.g. "look at this"
            // sent with a photo).  Object Replacement Characters that mark attachment
            // positions in the body are stripped.
            let caption = caption_from_message(msg);

            // For voice, also look up the transcription from the parsed body
            if t == TYPE_VOICE {
                let transcription = msg.components.iter().find_map(|c| {
                    if let imessage_database::tables::messages::models::BubbleComponent::Attachment(meta) = c {
                        meta.transcription.as_deref()
                    } else {
                        None
                    }
                });
                let content = compose_voice_content(
                    path_str.as_deref().unwrap_or(""),
                    transcription,
                    caption.as_deref(),
                );
                return Ok((TYPE_VOICE, Some(content)));
            }

            return Ok((
                t,
                Some(compose_attachment_content(
                    t,
                    path_str.as_deref(),
                    caption.as_deref(),
                )),
            ));
        }

        // Plain text
        Ok((TYPE_TEXT, msg.text.clone()))
    }

    /// Build a human-readable system message for a group action
    fn group_action_text(config: &Config, msg: &Message, action: &GroupAction<'_>) -> String {
        let who = config.who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id);
        match action {
            GroupAction::ParticipantAdded(person) => {
                let name = config.who(Some(*person), false, &msg.destination_caller_id);
                format!("{who} added {name} to the conversation.")
            }
            GroupAction::ParticipantRemoved(person) => {
                let name = config.who(Some(*person), false, &msg.destination_caller_id);
                format!("{who} removed {name} from the conversation.")
            }
            GroupAction::NameChange(name) => {
                format!("{who} named the conversation \"{name}\".")
            }
            GroupAction::ParticipantLeft => format!("{who} left the conversation."),
            GroupAction::GroupIconChanged => format!("{who} changed the group photo."),
            GroupAction::GroupIconRemoved => format!("{who} removed the group photo."),
            GroupAction::ChatBackgroundChanged => format!("{who} changed the chat background."),
            GroupAction::ChatBackgroundRemoved => format!("{who} removed the chat background."),
            GroupAction::PhoneNumberChanged(person) => {
                let name = config.who(Some(*person), false, &msg.destination_caller_id);
                format!("{name} changed their phone number.")
            }
        }
    }

    /// Format a tapback reaction as plain text; returns None for removed tapbacks
    fn tapback_text(config: &Config, msg: &Message) -> Option<String> {
        match msg.variant() {
            Variant::Tapback(_, TapbackAction::Removed, _) => None,
            Variant::Tapback(_, _, tapback) => {
                let who = config.who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id);
                Some(format!("{tapback} by {who}"))
            }
            _ => None,
        }
    }

    /// Serialize a ConversationBuffer to a pretty-printed ChatLab JSON string
    fn serialize_conversation(buf: &ConversationBuffer, exported_at: i64) -> String {
        let members: Vec<JsonValue> = buf
            .members
            .iter()
            .map(|(pid, name, avatar_url)| {
                let mut obj = JsonValue::new_object();
                obj["platformId"] = pid.as_str().into();
                obj["accountName"] = name.as_str().into();
                if let Some(url) = avatar_url {
                    obj["avatar"] = url.as_str().into();
                }
                obj
            })
            .collect();

        let messages: Vec<JsonValue> = buf
            .messages
            .iter()
            .map(|m| {
                let mut obj = JsonValue::new_object();
                obj["sender"] = m.sender_id.as_str().into();
                obj["accountName"] = m.account_name.as_str().into();
                obj["timestamp"] = m.timestamp.into();
                obj["type"] = i64::from(m.msg_type).into();
                obj["content"] = match &m.content {
                    Some(c) => c.as_str().into(),
                    None => JsonValue::Null,
                };
                obj["platformMessageId"] = m.platform_message_id.as_str().into();
                if let Some(ref reply_id) = m.reply_to_id {
                    obj["replyToMessageId"] = reply_id.as_str().into();
                }
                obj
            })
            .collect();

        let mut header = JsonValue::new_object();
        header["version"] = CHATLAB_VERSION.into();
        header["exportedAt"] = exported_at.into();
        header["generator"] = GENERATOR.into();

        let mut meta = JsonValue::new_object();
        meta["name"] = buf.chat_name.as_str().into();
        meta["platform"] = PLATFORM.into();
        meta["type"] = buf.chat_type.into();
        meta["ownerId"] = buf.owner_id.as_str().into();
        if let Some(url) = &buf.group_avatar_url {
            meta["groupAvatar"] = url.as_str().into();
        }

        let mut root = JsonValue::new_object();
        root["chatlab"] = header;
        root["meta"] = meta;
        root["members"] = JsonValue::Array(members);
        root["messages"] = JsonValue::Array(messages);

        root.pretty(2)
    }

    /// Write all buffered conversations (and orphaned messages) to disk
    fn write_all(&self) -> Result<(), RuntimeError> {
        let exported_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        for (&real_id, buf) in &self.conversations {
            // Find the Chat whose real_id matches, to get the correct filename
            let chatroom = self
                .config
                .chatrooms
                .iter()
                .find(|(rowid, _)| self.config.real_chatrooms.get(*rowid) == Some(&real_id))
                .map(|(_, chat)| chat);

            let filename = chatroom
                .map(|c| self.config.filename(c))
                .unwrap_or_else(|| format!("{real_id}.json"));

            let mut path = self.config.options.export_path.clone();
            path.push(&filename);

            let json_str = Self::serialize_conversation(buf, exported_at);
            let file = File::options()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)?;
            BufWriter::new(file)
                .write_all(json_str.as_bytes())
                .map_err(RuntimeError::DiskError)?;
        }

        // Write orphaned.json if there are orphaned messages
        if !self.orphaned.is_empty() {
            let owner_id = self
                .config
                .options
                .custom_name
                .clone()
                .unwrap_or_else(|| ME.to_string());

            // Build member list from unique senders
            let mut members: Vec<(String, String, Option<String>)> =
                vec![(owner_id.clone(), ME.to_string(), None)];
            for msg in &self.orphaned {
                if !members.iter().any(|(id, _, _)| id == &msg.sender_id) {
                    members.push((msg.sender_id.clone(), msg.account_name.clone(), None));
                }
            }

            let orphaned_buf = ConversationBuffer {
                chat_name: ORPHANED.to_string(),
                chat_type: "private",
                owner_id,
                members,
                messages: self.orphaned.clone(),
                group_avatar_url: None,
            };

            let mut path = self.config.options.export_path.clone();
            path.push(ORPHANED);
            path.set_extension("json");

            let json_str = Self::serialize_conversation(&orphaned_buf, exported_at);
            let file = File::options()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)?;
            BufWriter::new(file)
                .write_all(json_str.as_bytes())
                .map_err(RuntimeError::DiskError)?;
        }

        Ok(())
    }
}

// ─── Exporter trait ───────────────────────────────────────────────────────────

impl<'a> Exporter<'a> for JSON<'a> {
    fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        Ok(JSON {
            config,
            conversations: HashMap::new(),
            orphaned: Vec::new(),
            pb: ExportProgress::new(),
        })
    }

    fn iter_messages(&mut self) -> Result<(), RuntimeError> {
        eprintln!(
            "Exporting to {} as json...",
            self.config.options.export_path.display()
        );

        let mut current_message_row = -1;
        let mut current_message: u64 = 0;
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

            // Skip duplicate ROWIDs
            if msg.rowid == current_message_row {
                current_message += 1;
                continue;
            }
            current_message_row = msg.rowid;

            if let Ok(body) = msg.parse_body(self.config.data_source.db()) {
                msg.apply_body(body);
            }

            // Skip poll votes and updates (no standalone meaning)
            if msg.is_poll_vote() || msg.is_poll_update() {
                current_message += 1;
                continue;
            }

            // Compute all config-derived values before mutably borrowing self.conversations
            let sender_id = self.sender_platform_id(&msg);
            let account_name = self
                .config
                .who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id)
                .to_string();
            // Older databases store timestamps as seconds since 2001-01-01;
            // newer databases store nanoseconds. The guard mirrors get_local_time() in dates.rs.
            let date_secs = if msg.date >= 1_000_000_000_000 {
                msg.date / TIMESTAMP_FACTOR
            } else {
                msg.date
            };
            let timestamp = date_secs + self.config.offset;
            let (msg_type, content) = match self.classify(&msg) {
                Ok(pair) => pair,
                Err(err) => {
                    eprintln!("Failed to classify message {}: {err}", msg.rowid);
                    current_message += 1;
                    continue;
                }
            };
            let platform_message_id = msg.guid.clone();
            let reply_to_id = msg.thread_originator_guid.clone();

            // Extract conversation metadata (borrows config immutably)
            let conv_data = self.config.conversation(&msg).map(|(chatroom, &real_id)| {
                // iMessage group chats always have identifiers that start with "chat";
                // individual conversations use phone numbers or email addresses.
                let chat_type = if chatroom.chat_identifier.starts_with("chat") {
                    "group"
                } else {
                    "private"
                };
                let chat_name = chatroom
                    .display_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        // config.filename() includes the ".json" extension; strip it for the
                        // human-readable meta.name field.
                        let full = self.config.filename(chatroom);
                        full.strip_suffix(".json")
                            .unwrap_or(&full)
                            .to_string()
                    });
                let owner_id = Self::owner_id(self.config, &msg);
                let owner_name = self
                    .config
                    .options
                    .custom_name
                    .as_deref()
                    .unwrap_or(ME)
                    .to_string();

                let group_avatar_url: Option<String> =
                    if self.config.options.embed_avatars && chat_type == "group" {
                        chatroom
                            .properties(self.config.data_source.db())
                            .and_then(|props| props.group_photo_guid)
                            .and_then(|guid| {
                                imessage_database::tables::attachment::Attachment::from_guid(
                                    self.config.data_source.db(),
                                    &guid,
                                )
                                .ok()
                                .flatten()
                            })
                            .and_then(|att| {
                                att.resolved_attachment_path(
                                    &self.config.options.platform,
                                    &self.config.options.db_path,
                                    self.config.options.attachment_root.as_deref(),
                                )
                            })
                            .and_then(|path| std::fs::read(path).ok())
                            .and_then(|bytes| {
                                let conv = self
                                    .config
                                    .options
                                    .attachment_manager
                                    .image_converter
                                    .as_ref();
                                crate::app::avatar::bytes_to_data_url_with_converter(&bytes, conv)
                            })
                    } else {
                        None
                    };

                (real_id, chat_type, chat_name, owner_id, owner_name, group_avatar_url)
            });

            let clm = ChatLabMessage {
                sender_id: sender_id.clone(),
                account_name: account_name.clone(),
                timestamp,
                msg_type,
                content,
                platform_message_id,
                reply_to_id,
            };

            match conv_data {
                Some((real_id, chat_type, chat_name, owner_id, owner_name, group_avatar_url)) => {
                    // Source the sender's avatar Data URL (only if --embed-avatars is on)
                    let sender_avatar_url: Option<String> = if self.config.options.embed_avatars {
                        msg.handle_id.and_then(|h| {
                            self.config.real_participants.get(&h).and_then(|&internal_id| {
                                self.config.participants.get(&internal_id).and_then(|n| {
                                    self.config.data_source.contacts_index.get_avatar(&n.details).and_then(|bytes| {
                                        let conv = self.config.options.attachment_manager.image_converter.as_ref();
                                        crate::app::avatar::bytes_to_data_url_with_converter(bytes, conv)
                                    })
                                })
                            })
                        })
                    } else {
                        None
                    };

                    let buffer =
                        self.conversations
                            .entry(real_id)
                            .or_insert_with(|| ConversationBuffer {
                                chat_name,
                                chat_type,
                                owner_id: owner_id.clone(),
                                members: vec![(owner_id, owner_name, None)],
                                messages: Vec::new(),
                                group_avatar_url,
                            });
                    buffer.add_member(sender_id, account_name, sender_avatar_url);
                    buffer.messages.push(clm);
                }
                None => {
                    self.orphaned.push(clm);
                }
            }

            current_message += 1;
            if current_message % 99 == 0 {
                self.pb.set_position(current_message);
            }
        }

        self.pb.finish();
        self.write_all()?;
        Ok(())
    }

    /// Not used by the JSON exporter — messages are buffered, not streamed to files.
    fn get_or_create_file(
        &mut self,
        _message: &Message,
    ) -> Result<&mut BufWriter<File>, RuntimeError> {
        // DESIGN: The JSON exporter buffers all messages in memory and writes in write_all(). This trait method is never called.
        unreachable!("JSON exporter buffers in memory; get_or_create_file is never called")
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::app::{export_type::ExportType, runtime::Config};
    use imessage_database::tables::messages::Message;

    use super::*;

    fn make_config() -> Config {
        Config::fake_app(crate::app::options::Options::fake_options(ExportType::Json))
    }

    fn make_msg() -> Message {
        Config::fake_message()
    }

    // ── content_label / compose_attachment_content ───────────────────────────

    #[test]
    fn label_for_image_is_bracket_image() {
        assert_eq!(content_label(TYPE_IMAGE), "[Image]");
    }

    #[test]
    fn label_for_voice_is_bracket_voice() {
        assert_eq!(content_label(TYPE_VOICE), "[Voice]");
    }

    #[test]
    fn label_for_video_is_bracket_video() {
        assert_eq!(content_label(TYPE_VIDEO), "[Video]");
    }

    #[test]
    fn label_for_file_is_bracket_file() {
        assert_eq!(content_label(TYPE_FILE), "[File]");
    }

    #[test]
    fn label_for_sticker_is_bracket_sticker() {
        assert_eq!(content_label(TYPE_EMOJI), "[Sticker]");
    }

    #[test]
    fn compose_attachment_content_with_path() {
        assert_eq!(
            compose_attachment_content(TYPE_IMAGE, Some("attachments/12/8421.jpeg"), None),
            "[Image] attachments/12/8421.jpeg"
        );
    }

    #[test]
    fn compose_attachment_content_with_path_and_caption() {
        assert_eq!(
            compose_attachment_content(
                TYPE_IMAGE,
                Some("attachments/12/8421.jpeg"),
                Some("look at this")
            ),
            "[Image] attachments/12/8421.jpeg \u{2014} look at this"
        );
    }

    #[test]
    fn compose_attachment_content_caption_only_no_path() {
        assert_eq!(
            compose_attachment_content(TYPE_FILE, None, Some("important")),
            "[File] \u{2014} important"
        );
    }

    #[test]
    fn compose_attachment_content_empty_caption_treated_as_none() {
        assert_eq!(
            compose_attachment_content(TYPE_IMAGE, Some("x.jpg"), Some("")),
            "[Image] x.jpg"
        );
    }

    #[test]
    fn compose_attachment_content_without_path() {
        assert_eq!(compose_attachment_content(TYPE_IMAGE, None, None), "[Image]");
    }

    #[test]
    fn compose_attachment_content_with_empty_path_string() {
        assert_eq!(compose_attachment_content(TYPE_IMAGE, Some(""), None), "[Image]");
    }

    // ── caption_from_message ─────────────────────────────────────────────────

    #[test]
    fn caption_from_message_strips_replacement_chars() {
        let mut msg = make_msg();
        msg.text = Some("\u{FFFC}look at this\u{FFFC}".to_string());
        assert_eq!(caption_from_message(&msg).as_deref(), Some("look at this"));
    }

    #[test]
    fn caption_from_message_collapses_whitespace_around_attachments() {
        let mut msg = make_msg();
        msg.text = Some("hey \u{FFFC} look \u{FFFC} cool".to_string());
        assert_eq!(caption_from_message(&msg).as_deref(), Some("hey look cool"));
    }

    #[test]
    fn caption_from_message_returns_none_for_attachment_only_text() {
        let mut msg = make_msg();
        msg.text = Some("\u{FFFC}\u{FFFC}".to_string());
        assert_eq!(caption_from_message(&msg), None);
    }

    #[test]
    fn caption_from_message_returns_none_when_text_missing() {
        let mut msg = make_msg();
        msg.text = None;
        assert_eq!(caption_from_message(&msg), None);
    }

    // ── classify: type code mapping ──────────────────────────────────────────

    #[test]
    fn classify_plain_text_is_type_0() {
        let config = make_config();
        let exporter = JSON {
            config: &config,
            conversations: HashMap::new(),
            orphaned: Vec::new(),
            pb: ExportProgress::new(),
        };
        let mut msg = make_msg();
        msg.text = Some("hello".to_string());
        let (t, content) = exporter.classify(&msg).unwrap();
        assert_eq!(t, TYPE_TEXT);
        assert_eq!(content, Some("hello".to_string()));
    }

    #[test]
    fn classify_announcement_with_no_action_falls_through_to_text() {
        let config = make_config();
        let exporter = JSON {
            config: &config,
            conversations: HashMap::new(),
            orphaned: Vec::new(),
            pb: ExportProgress::new(),
        };
        let mut msg = make_msg();
        // item_type=4 marks this as a group action announcement, but group_action_type=0
        // with no other_handle set means get_announcement() returns None, so the
        // announcement arm falls through to the catch-all which returns TYPE_TEXT.
        msg.item_type = 4;
        msg.group_action_type = 0;
        let (t, _) = exporter.classify(&msg).unwrap();
        assert_eq!(t, TYPE_TEXT);
    }

    #[test]
    fn classify_tapback_is_type_99() {
        let config = make_config();
        let exporter = JSON {
            config: &config,
            conversations: HashMap::new(),
            orphaned: Vec::new(),
            pb: ExportProgress::new(),
        };
        let mut msg = make_msg();
        // A tapback has associated_message_type in 2000..2999 range
        msg.associated_message_type = Some(2000); // "Loved" tapback
        msg.associated_message_guid = Some("p:0/some-guid".to_string());
        let (t, _) = exporter.classify(&msg).unwrap();
        assert_eq!(t, TYPE_OTHER);
    }

    // ── group/private detection ───────────────────────────────────────────────

    #[test]
    fn chat_identifier_starting_with_chat_is_group() {
        let chat_identifier = "chat123456";
        let chat_type = if chat_identifier.starts_with("chat") {
            "group"
        } else {
            "private"
        };
        assert_eq!(chat_type, "group");
    }

    #[test]
    fn phone_number_chat_identifier_is_private() {
        let chat_identifier = "+15558675309";
        let chat_type = if chat_identifier.starts_with("chat") {
            "group"
        } else {
            "private"
        };
        assert_eq!(chat_type, "private");
    }

    // ── JSON serialization ────────────────────────────────────────────────────

    #[test]
    fn serialize_empty_conversation_contains_required_keys() {
        let buf = ConversationBuffer {
            chat_name: "Test Chat".to_string(),
            chat_type: "private",
            owner_id: "Me".to_string(),
            members: vec![("Me".to_string(), "Me".to_string(), None)],
            messages: Vec::new(),
            group_avatar_url: None,
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(json_str.contains("\"chatlab\""));
        assert!(json_str.contains("\"0.0.2\""));
        assert!(json_str.contains("\"imessage\""));
        assert!(json_str.contains("\"members\""));
        assert!(json_str.contains("\"messages\""));
        assert!(json_str.contains("\"Test Chat\""));
        assert!(json_str.contains("1700000000"));
    }

    #[test]
    fn serialize_message_with_null_content() {
        let buf = ConversationBuffer {
            chat_name: "Test".to_string(),
            chat_type: "private",
            owner_id: "Me".to_string(),
            members: vec![("Me".to_string(), "Me".to_string(), None)],
            messages: vec![ChatLabMessage {
                sender_id: "Me".to_string(),
                account_name: "Me".to_string(),
                timestamp: 1_700_000_000,
                msg_type: TYPE_RECALL,
                content: None,
                platform_message_id: "guid-1".to_string(),
                reply_to_id: None,
            }],
            group_avatar_url: None,
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(json_str.contains("\"content\": null"));
        assert!(json_str.contains("\"type\": 81"));
    }

    #[test]
    fn serialize_reply_message_includes_reply_field() {
        let buf = ConversationBuffer {
            chat_name: "Test".to_string(),
            chat_type: "private",
            owner_id: "Me".to_string(),
            members: vec![("Me".to_string(), "Me".to_string(), None)],
            messages: vec![ChatLabMessage {
                sender_id: "Me".to_string(),
                account_name: "Me".to_string(),
                timestamp: 1_700_000_000,
                msg_type: TYPE_TEXT,
                content: Some("reply text".to_string()),
                platform_message_id: "guid-2".to_string(),
                reply_to_id: Some("guid-1".to_string()),
            }],
            group_avatar_url: None,
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(json_str.contains("\"replyToMessageId\""));
        assert!(json_str.contains("\"guid-1\""));
    }

    // ── compose_voice_content ─────────────────────────────────────────────────

    #[test]
    fn compose_voice_content_with_transcription_appends_suffix() {
        assert_eq!(
            compose_voice_content("attachments/12/8422.caf", Some("on my way"), None),
            "[Voice] attachments/12/8422.caf — Transcription: on my way"
        );
    }

    #[test]
    fn compose_voice_content_with_caption_appends_after_transcription() {
        assert_eq!(
            compose_voice_content(
                "attachments/12/8422.caf",
                Some("on my way"),
                Some("see you soon")
            ),
            "[Voice] attachments/12/8422.caf — Transcription: on my way — see you soon"
        );
    }

    #[test]
    fn compose_voice_content_with_caption_only_no_transcription() {
        assert_eq!(
            compose_voice_content("path.caf", None, Some("note")),
            "[Voice] path.caf — note"
        );
    }

    #[test]
    fn compose_voice_content_no_transcription_uses_plain_label() {
        assert_eq!(
            compose_voice_content("attachments/12/8422.caf", None, None),
            "[Voice] attachments/12/8422.caf"
        );
    }

    #[test]
    fn compose_voice_content_empty_path_still_appends_transcription() {
        assert_eq!(
            compose_voice_content("", Some("hi"), None),
            "[Voice] — Transcription: hi"
        );
    }

    #[test]
    fn serialize_message_without_reply_omits_reply_field() {
        let buf = ConversationBuffer {
            chat_name: "Test".to_string(),
            chat_type: "private",
            owner_id: "Me".to_string(),
            members: vec![("Me".to_string(), "Me".to_string(), None)],
            messages: vec![ChatLabMessage {
                sender_id: "Me".to_string(),
                account_name: "Me".to_string(),
                timestamp: 1_700_000_000,
                msg_type: TYPE_TEXT,
                content: Some("hello".to_string()),
                platform_message_id: "guid-3".to_string(),
                reply_to_id: None,
            }],
            group_avatar_url: None,
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(!json_str.contains("replyToMessageId"));
    }

    #[test]
    fn serialize_member_with_avatar_emits_data_url_key() {
        let buf = ConversationBuffer {
            chat_name: "Test".to_string(),
            chat_type: "private",
            owner_id: "Me".to_string(),
            members: vec![
                ("Me".to_string(), "Me".to_string(), None),
                ("+15555550100".to_string(), "Alice".to_string(),
                 Some("data:image/jpeg;base64,/9j/4A==".to_string())),
            ],
            messages: Vec::new(),
            group_avatar_url: None,
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(json_str.contains("\"avatar\": \"data:image/jpeg;base64,/9j/4A==\""));
    }

    #[test]
    fn serialize_member_without_avatar_omits_avatar_key() {
        let buf = ConversationBuffer {
            chat_name: "Test".to_string(),
            chat_type: "private",
            owner_id: "Me".to_string(),
            members: vec![("Me".to_string(), "Me".to_string(), None)],
            messages: Vec::new(),
            group_avatar_url: None,
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(!json_str.contains("\"avatar\""));
    }

    #[test]
    fn serialize_meta_with_group_avatar_emits_key() {
        let buf = ConversationBuffer {
            chat_name: "Family".to_string(),
            chat_type: "group",
            owner_id: "Me".to_string(),
            members: vec![("Me".to_string(), "Me".to_string(), None)],
            messages: Vec::new(),
            group_avatar_url: Some("data:image/jpeg;base64,/9j/4A==".to_string()),
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(json_str.contains("\"groupAvatar\": \"data:image/jpeg;base64,/9j/4A==\""));
    }

    #[test]
    fn serialize_meta_without_group_avatar_omits_key() {
        let buf = ConversationBuffer {
            chat_name: "Family".to_string(),
            chat_type: "group",
            owner_id: "Me".to_string(),
            members: vec![("Me".to_string(), "Me".to_string(), None)],
            messages: Vec::new(),
            group_avatar_url: None,
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(!json_str.contains("\"groupAvatar\""));
    }
}
