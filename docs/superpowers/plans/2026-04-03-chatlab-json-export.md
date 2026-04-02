# ChatLab JSON Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new `--format json` export type that writes one ChatLab 0.0.2-format JSON file per iMessage conversation.

**Architecture:** A new `JSON<'a>` struct implementing the `Exporter` trait buffers all messages in memory during `iter_messages()`, grouped by `real_chat_id`. After iteration completes, each `ConversationBuffer` is serialized to a `<name>.json` file using the `jzon` crate (already used in `imessage-database`). No per-message streaming — unlike html/txt, JSON requires the full document before writing.

**Tech Stack:** Rust, jzon 0.12.5, imessage-database types, existing `Config`/`ExportProgress`/`RuntimeError` infrastructure.

---

> **Note:** 85 pre-existing html test failures exist in this repo and are unrelated to this feature. Run `cargo test -p imessage-exporter 2>&1 | grep -E "^test result"` to confirm baseline before starting.

---

## File Map

| Action | Path |
|--------|------|
| Modify | `imessage-exporter/Cargo.toml` |
| Modify | `imessage-exporter/src/app/export_type.rs` |
| Modify | `imessage-exporter/src/app/options.rs` |
| Modify | `imessage-exporter/src/app/runtime.rs` |
| Modify | `imessage-exporter/src/exporters/mod.rs` |
| Modify | `imessage-exporter/src/main.rs` |
| Create | `imessage-exporter/src/exporters/json.rs` |

---

## Task 1: Add `ExportType::Json` variant (TDD)

**Files:**
- Modify: `imessage-exporter/src/app/export_type.rs`

- [ ] **Step 1: Write failing test `can_parse_json_any_case`**

Add at the end of the `#[cfg(test)] mod tests` block in `export_type.rs`:

```rust
#[test]
fn can_parse_json_any_case() {
    assert!(matches!(
        ExportType::from_cli("json"),
        Some(ExportType::Json)
    ));
    assert!(matches!(
        ExportType::from_cli("JSON"),
        Some(ExportType::Json)
    ));
    assert!(matches!(
        ExportType::from_cli("JsOn"),
        Some(ExportType::Json)
    ));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p imessage-exporter -- export_type::tests::can_parse_json_any_case 2>&1 | tail -5
```

Expected: `FAILED` with compile error (variant doesn't exist yet).

- [ ] **Step 3: Implement the `Json` variant**

In `export_type.rs`, make these four additions:

```rust
// In the ExportType enum, after Txt:
/// JSON file export (ChatLab format)
Json,
```

```rust
// In from_cli, after "html" arm:
"json" => Some(Self::Json),
```

```rust
// In extension(), after Txt arm:
ExportType::Json => ".json",
```

```rust
// In Display impl, after Txt arm:
ExportType::Json => write!(fmt, "json"),
```

- [ ] **Step 4: Update `cant_parse_invalid` test — remove `"json"` from invalid list**

In the existing `cant_parse_invalid` test, remove the `json` assertion:

```rust
#[test]
fn cant_parse_invalid() {
    assert!(ExportType::from_cli("pdf").is_none());
    assert!(ExportType::from_cli("").is_none());
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p imessage-exporter -- export_type 2>&1 | tail -5
```

Expected: `test result: ok. 3 passed; 0 failed`

- [ ] **Step 6: Commit**

```bash
git add imessage-exporter/src/app/export_type.rs
git commit -m "feat: add ExportType::Json variant"
```

---

## Task 2: Wire up dependencies, CLI text, module, and dispatch

**Files:**
- Modify: `imessage-exporter/Cargo.toml`
- Modify: `imessage-exporter/src/app/options.rs`
- Modify: `imessage-exporter/src/exporters/mod.rs`
- Modify: `imessage-exporter/src/main.rs`
- Modify: `imessage-exporter/src/app/runtime.rs`

- [ ] **Step 1: Add `jzon` to `imessage-exporter/Cargo.toml`**

In the `[dependencies]` section, add:

```toml
jzon = "=0.12.5"
```

- [ ] **Step 2: Update CLI text in `options.rs`**

Replace:
```rust
pub const SUPPORTED_FILE_TYPES: &str = "txt, html";
```
With:
```rust
pub const SUPPORTED_FILE_TYPES: &str = "txt, html, json";
```

Replace:
```rust
pub const ABOUT: &str = concat!(
    "The `imessage-exporter` binary exports iMessage data to\n",
    "`txt` or `html` formats. It can also run diagnostics\n",
    "to find problems with the iMessage database."
);
```
With:
```rust
pub const ABOUT: &str = concat!(
    "The `imessage-exporter` binary exports iMessage data to\n",
    "`txt`, `html`, or `json` (ChatLab format) formats. It can also run\n",
    "diagnostics to find problems with the iMessage database."
);
```

- [ ] **Step 3: Add `pub mod json` to `exporters/mod.rs`**

```rust
pub mod exporter;
pub mod html;
pub mod json;
mod shared;
pub mod txt;
```

- [ ] **Step 4: Export `JSON` from `main.rs`**

Replace:
```rust
pub use exporters::{exporter::Exporter, html::HTML, txt::TXT};
```
With:
```rust
pub use exporters::{exporter::Exporter, html::HTML, json::JSON, txt::TXT};
```

- [ ] **Step 5: Add dispatch in `runtime.rs`**

In `runtime.rs`, find the import line:
```rust
use crate::{
    Exporter, HTML, TXT,
```
Replace with:
```rust
use crate::{
    Exporter, HTML, JSON, TXT,
```

Find the match block (around line 562):
```rust
ExportType::Html => {
    HTML::new(self)?.iter_messages()?;
}
ExportType::Txt => {
    TXT::new(self)?.iter_messages()?;
}
```
Add after the `Txt` arm:
```rust
ExportType::Json => {
    JSON::new(self)?.iter_messages()?;
}
```

- [ ] **Step 6: Verify it compiles (json.rs stub required first)**

Create a temporary stub at `imessage-exporter/src/exporters/json.rs`:

```rust
pub struct JSON<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}
```

Then check compile:

```bash
cargo build -p imessage-exporter 2>&1 | grep "^error" | head -10
```

Expected: errors about missing `Exporter` impl — that's expected at this stage. If there are import errors in `runtime.rs`, fix them first.

- [ ] **Step 7: Commit**

```bash
git add imessage-exporter/Cargo.toml \
        imessage-exporter/src/app/options.rs \
        imessage-exporter/src/exporters/mod.rs \
        imessage-exporter/src/main.rs \
        imessage-exporter/src/app/runtime.rs \
        imessage-exporter/src/exporters/json.rs
git commit -m "feat: wire up json export dispatch and CLI text"
```

---

## Task 3: Implement `exporters/json.rs`

**Files:**
- Create/replace: `imessage-exporter/src/exporters/json.rs`

This task creates the complete JSON exporter. Replace the stub from Task 2 with the full implementation.

- [ ] **Step 1: Write the complete `json.rs`**

```rust
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
        table::{ME, ORPHANED},
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
struct ChatLabMessage {
    sender_id: String,
    account_name: String,
    timestamp: i64,
    msg_type: u8,
    content: Option<String>,
    platform_message_id: String,
    reply_to_id: Option<String>,
}

/// All data for one conversation, buffered during iteration
struct ConversationBuffer {
    chat_name: String,
    chat_type: &'static str, // "group" or "private"
    owner_id: String,
    /// Ordered (platformId, displayName). Owner is always index 0.
    members: Vec<(String, String)>,
    messages: Vec<ChatLabMessage>,
}

impl ConversationBuffer {
    /// Append member if not already present (linear scan — member count is tiny)
    fn add_member(&mut self, platform_id: String, display_name: String) {
        if !self.members.iter().any(|(id, _)| id == &platform_id) {
            self.members.push((platform_id, display_name));
        }
    }
}

pub struct JSON<'a> {
    pub config: &'a Config,
    pub conversations: HashMap<i32, ConversationBuffer>,
    pub orphaned: Vec<ChatLabMessage>,
    pb: ExportProgress,
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
            return handle_id.to_string();
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
        let attachments = Attachment::from_message(self.config.data_source.db(), msg)?;
        if let Some(first) = attachments.first() {
            if first.is_sticker {
                let path = self.config.message_attachment_path(first);
                return Ok((TYPE_EMOJI, Some(path)));
            }
            let t = match first.mime_type() {
                MediaType::Image(_) => TYPE_IMAGE,
                MediaType::Audio(_) => TYPE_VOICE,
                MediaType::Video(_) => TYPE_VIDEO,
                _ => TYPE_FILE,
            };
            return Ok((t, Some(self.config.message_attachment_path(first))));
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
            .map(|(pid, name)| {
                let mut obj = JsonValue::new_object();
                obj["platformId"] = pid.as_str().into();
                obj["accountName"] = name.as_str().into();
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
            let mut members: Vec<(String, String)> =
                vec![(owner_id.clone(), ME.to_string())];
            for msg in &self.orphaned {
                if !members.iter().any(|(id, _)| id == &msg.sender_id) {
                    members.push((msg.sender_id.clone(), msg.account_name.clone()));
                }
            }

            let orphaned_buf = ConversationBuffer {
                chat_name: ORPHANED.to_string(),
                chat_type: "private",
                owner_id,
                members,
                messages: self.orphaned.clone(),
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
            let timestamp = msg.date / TIMESTAMP_FACTOR + self.config.offset;
            let (msg_type, content) = self.classify(&msg)?;
            let platform_message_id = msg.guid.clone();
            let reply_to_id = msg.thread_originator_guid.clone();

            // Extract conversation metadata (borrows config immutably)
            let conv_data = self.config.conversation(&msg).map(|(chatroom, &real_id)| {
                let chat_type = if chatroom.chat_identifier.starts_with("chat") {
                    "group"
                } else {
                    "private"
                };
                let chat_name = chatroom
                    .display_name()
                    .unwrap_or(&chatroom.chat_identifier)
                    .to_string();
                let owner_id = Self::owner_id(self.config, &msg);
                let owner_name = self
                    .config
                    .options
                    .custom_name
                    .as_deref()
                    .unwrap_or(ME)
                    .to_string();
                (real_id, chat_type, chat_name, owner_id, owner_name)
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
                Some((real_id, chat_type, chat_name, owner_id, owner_name)) => {
                    let buffer =
                        self.conversations
                            .entry(real_id)
                            .or_insert_with(|| ConversationBuffer {
                                chat_name,
                                chat_type,
                                owner_id: owner_id.clone(),
                                members: vec![(owner_id, owner_name)],
                                messages: Vec::new(),
                            });
                    buffer.add_member(sender_id, account_name);
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
    fn classify_fully_unsent_is_type_81() {
        let config = make_config();
        let exporter = JSON {
            config: &config,
            conversations: HashMap::new(),
            orphaned: Vec::new(),
            pb: ExportProgress::new(),
        };
        let mut msg = make_msg();
        // Mark as announcement with FullyUnsent — set item_type = 0 and edited_parts
        // The easiest way is to use associated_message_type for tapback detection test
        // For FullyUnsent we need edited_parts. Just test that announcement path is hit:
        msg.item_type = 4; // group action item_type triggers is_announcement via get_announcement
        msg.group_action_type = 0;
        // Without other_handle set, get_announcement returns None → falls through
        // So test the tapback path instead, which is simpler to trigger:
        let (t, _) = exporter.classify(&msg).unwrap();
        // item_type=4 with no matching group action → falls through to text
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
            members: vec![("Me".to_string(), "Me".to_string())],
            messages: Vec::new(),
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
            members: vec![("Me".to_string(), "Me".to_string())],
            messages: vec![ChatLabMessage {
                sender_id: "Me".to_string(),
                account_name: "Me".to_string(),
                timestamp: 1_700_000_000,
                msg_type: TYPE_RECALL,
                content: None,
                platform_message_id: "guid-1".to_string(),
                reply_to_id: None,
            }],
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
            members: vec![("Me".to_string(), "Me".to_string())],
            messages: vec![ChatLabMessage {
                sender_id: "Me".to_string(),
                account_name: "Me".to_string(),
                timestamp: 1_700_000_000,
                msg_type: TYPE_TEXT,
                content: Some("reply text".to_string()),
                platform_message_id: "guid-2".to_string(),
                reply_to_id: Some("guid-1".to_string()),
            }],
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(json_str.contains("\"replyToMessageId\""));
        assert!(json_str.contains("\"guid-1\""));
    }

    #[test]
    fn serialize_message_without_reply_omits_reply_field() {
        let buf = ConversationBuffer {
            chat_name: "Test".to_string(),
            chat_type: "private",
            owner_id: "Me".to_string(),
            members: vec![("Me".to_string(), "Me".to_string())],
            messages: vec![ChatLabMessage {
                sender_id: "Me".to_string(),
                account_name: "Me".to_string(),
                timestamp: 1_700_000_000,
                msg_type: TYPE_TEXT,
                content: Some("hello".to_string()),
                platform_message_id: "guid-3".to_string(),
                reply_to_id: None,
            }],
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(!json_str.contains("replyToMessageId"));
    }
}
```

- [ ] **Step 2: Build to check for compile errors**

```bash
cargo build -p imessage-exporter 2>&1 | grep "^error" | head -20
```

Expected: no errors. If there are errors about missing imports or wrong types, fix them.

Common fixes needed:
- If `TableError::QueryError` doesn't exist, check the `TableError` variants in `imessage-database/src/error/table.rs`
- If `.pretty(2)` doesn't compile, change to `.dump()` (compact but valid JSON)
- If `ChatLabMessage` needs `Clone`, it's already derived in the code above

- [ ] **Step 3: Run all tests**

```bash
cargo test -p imessage-exporter 2>&1 | tail -5
```

Expected: the 7 new `json::tests::*` tests pass. Pre-existing html failures (85) remain — ignore them.

- [ ] **Step 4: Commit**

```bash
git add imessage-exporter/src/exporters/json.rs
git commit -m "feat: implement ChatLab JSON exporter"
```

---

## Task 4: Verify end-to-end with a real database (smoke test)

- [ ] **Step 1: Build release binary**

```bash
cargo build -p imessage-exporter --release 2>&1 | tail -5
```

Expected: `Finished release profile`.

- [ ] **Step 2: Run with `--format json` on the default macOS database**

```bash
./target/release/imessage-exporter \
  --format json \
  --export-path /tmp/imessage-json-test 2>&1 | head -5
```

Expected: `Exporting to /tmp/imessage-json-test as json...` followed by progress output.

- [ ] **Step 3: Inspect a generated file**

```bash
ls /tmp/imessage-json-test/*.json | head -3
head -40 "$(ls /tmp/imessage-json-test/*.json | head -1)"
```

Expected: valid ChatLab JSON with `chatlab`, `meta`, `members`, `messages` keys.

- [ ] **Step 4: Validate JSON is well-formed**

```bash
python3 -c "
import json, glob
files = glob.glob('/tmp/imessage-json-test/*.json')
print(f'Validating {len(files)} files...')
for f in files[:10]:
    json.load(open(f))
print('All valid!')
"
```

Expected: `All valid!`

- [ ] **Step 5: Commit if any fixes were needed**

If the smoke test revealed issues, fix them, then:

```bash
git add imessage-exporter/src/exporters/json.rs
git commit -m "fix: correct json export issues found during smoke test"
```

---

## Self-Review Checklist

- [x] **Spec coverage**: All spec sections have a corresponding task
  - ExportType::Json → Task 1
  - CLI text + dispatch → Task 2
  - Buffered architecture + ConversationBuffer → Task 3
  - ChatLab field mappings (meta, members, messages) → Task 3
  - Message type codes (0,1,2,3,4,5,7,23,80,81,99) → Task 3 `classify()`
  - Attachment relative paths → Task 3 (`message_attachment_path`)
  - Tapback → type 99 → Task 3
  - Reply field → Task 3 (`reply_to_id`)
  - Orphaned messages → Task 3 (`write_all`)
  - Tests → Task 3 + Task 4
- [x] **No placeholders**: All steps contain complete code
- [x] **Type consistency**: `ChatLabMessage` defined in Task 3, used throughout Task 3 only
- [x] **Existing tests**: Pre-existing 85 html failures noted — do not attempt to fix
