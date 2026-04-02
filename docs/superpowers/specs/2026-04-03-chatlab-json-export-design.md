# ChatLab JSON Export — Design Spec

**Date:** 2026-04-03
**Format version:** ChatLab 0.0.2
**Spec reference:** https://chatlab.fun/cn/standard/chatlab-format.html

---

## Overview

Add a new `json` export type to `imessage-exporter` that produces one ChatLab-formatted JSON file per conversation. Output structure mirrors the existing `html`/`txt` exporters: one file per chat, placed in the user-specified export directory. All existing CLI options (including `--conversation-filter`, `--start-date`, `--end-date`, `--custom-name`, `--use-caller-id`) apply without modification.

---

## Files Changed

| File | Change |
|------|--------|
| `imessage-exporter/src/exporters/json.rs` | New file — JSON exporter |
| `imessage-exporter/src/app/export_type.rs` | Add `Json` variant to `ExportType` enum |
| `imessage-exporter/src/app/options.rs` | Update `SUPPORTED_FILE_TYPES` and CLI `ABOUT` text |
| `imessage-exporter/src/app/runtime.rs` | Dispatch `ExportType::Json` to `JSON::new(self)?.iter_messages()?` |
| `imessage-exporter/src/main.rs` | Add `pub use exporters::json::JSON` |
| `imessage-exporter/Cargo.toml` | Add `jzon = "=0.12.5"` |

No changes to `imessage-database`. No new transitive dependencies.

---

## Architecture

### Approach: Fully Buffered Per Conversation

Messages are buffered in memory during `iter_messages()`, grouped by `real_chat_id`. After iteration completes, each conversation buffer is serialized to a single JSON file. This approach is consistent with how the existing exporters already buffer tapbacks (`HashMap<String, Vec<Message>>`).

### Core Structs

```rust
pub struct JSON<'a> {
    pub config: &'a Config,
    /// Buffered conversation data, keyed by real_chat_id
    pub conversations: HashMap<i32, ConversationBuffer>,
    /// Messages with no associated chat
    pub orphaned: Vec<ChatLabMessage>,
    pb: ExportProgress,
}

struct ConversationBuffer {
    chat_id: i32,
    /// Ordered list of (platformId, displayName)
    /// Owner is always inserted first; duplicates checked with linear scan (member count is tiny)
    members: Vec<(String, String)>,
    messages: Vec<ChatLabMessage>,
}

struct ChatLabMessage {
    sender_id: String,
    account_name: String,
    timestamp: i64,          // Unix seconds
    msg_type: u8,            // ChatLab type code
    content: Option<String>,
    platform_message_id: String,  // Message.guid
    reply_to_id: Option<String>,  // thread_originator_guid
}
```

`Vec` preserves insertion order for the `members` array (owner first, then contacts in order encountered). No extra dependency needed.

---

## ChatLab JSON Output Structure

One `.json` file per conversation:

```json
{
  "chatlab": {
    "version": "0.0.2",
    "exportedAt": 1743609600,
    "generator": "imessage-exporter"
  },
  "meta": {
    "name": "张三",
    "platform": "imessage",
    "type": "private",
    "ownerId": "+1234567890"
  },
  "members": [
    { "platformId": "+1234567890", "accountName": "Me" },
    { "platformId": "+0987654321", "accountName": "张三" }
  ],
  "messages": [
    {
      "sender": "+0987654321",
      "accountName": "张三",
      "timestamp": 1743609500,
      "type": 0,
      "content": "你好",
      "platformMessageId": "GUID-abc123"
    },
    {
      "sender": "+1234567890",
      "accountName": "Me",
      "timestamp": 1743609510,
      "type": 99,
      "content": "Loved \"你好\"",
      "platformMessageId": "GUID-def456"
    },
    {
      "sender": "+1234567890",
      "accountName": "Me",
      "timestamp": 1743609520,
      "type": 1,
      "content": "attachments/1/photo.jpg",
      "platformMessageId": "GUID-ghi789",
      "replyToMessageId": "GUID-abc123"
    }
  ]
}
```

Orphaned messages are written to `orphaned.json` using the same structure, with `meta.name` set to `"orphaned"`.

---

## Field Mappings

### `meta`

| ChatLab field | Source |
|---|---|
| `name` | `Chat.display_name()` or `config.filename()` |
| `platform` | Hardcoded `"imessage"` |
| `type` | `chat_identifier` starts with `"chat"` → `"group"`, else `"private"` |
| `ownerId` | `--use-caller-id` caller ID, else `--custom-name`, else `"Me"` |

### `members`

| ChatLab field | Source |
|---|---|
| `platformId` | `Handle.id` (phone/email) for contacts; owner's caller ID or `"Me"` |
| `accountName` | Resolved contact name from `Config.participants`, fallback to `Handle.id` |

Owner member is always inserted first when the first message is processed for a conversation.

### `messages`

| ChatLab field | Source |
|---|---|
| `sender` | `is_from_me=true` → owner platformId; else `Handle.id` via `handle_id` |
| `accountName` | Resolved display name |
| `timestamp` | `Message.date` converted to Unix seconds using `Config.offset` |
| `type` | See type mapping table below |
| `content` | See content rules below |
| `platformMessageId` | `Message.guid` |
| `replyToMessageId` | `Message.thread_originator_guid` (omitted if `None`) |

### Message Type Mapping

| iMessage characteristic | ChatLab `type` |
|---|---|
| Text, no attachment | 0 (Text) |
| Image attachment (`image/*`) | 1 (Image) |
| Audio attachment (`audio/*`) | 2 (Voice) |
| Video attachment (`video/*`) | 3 (Video) |
| Other file attachment | 4 (File) |
| Sticker | 5 (Emoji) |
| URL balloon | 7 (Link) |
| Placemark / location | 8 (Location) |
| FaceTime / call | 23 (Call) |
| Group action / announcement | 80 (System) |
| Recalled / unsent message | 81 (Recall) |
| Tapback reaction | 99 (Other) |
| App message (Apple Pay, etc.) | 99 (Other) |

### Content Rules

- **Text message**: `Message.text`
- **Attachment**: relative path to copied file (e.g. `"attachments/1/photo.jpg"`); `null` if attachment is missing
- **Tapback**: human-readable string matching existing txt exporter format (e.g. `"Loved \"Hello\""`)
- **System/announcement**: description string (e.g. `"张三 added 李四"`)
- **Recalled message**: `null`

---

## JSON Serialization

Uses `jzon = "=0.12.5"` (already present in `imessage-database`, pinned to match). No `serde` introduced. Each `ChatLabMessage` and `ConversationBuffer` implements a `to_jzon()` method returning a `jzon::JsonValue`. The final object is serialized with `jzon::stringify()`.

---

## Write Flow

```
iter_messages() start
  └─ for each Message:
       1. Determine sender platformId
       2. Resolve display name
       3. Map to ChatLabMessage (type, content, timestamp)
       4. Ensure owner member exists in ConversationBuffer.members
       5. Ensure sender member exists in ConversationBuffer.members
       6. Append ChatLabMessage to ConversationBuffer.messages
iter_messages() end
  └─ for each ConversationBuffer:
       1. Build jzon object tree
       2. Write to <export_path>/<filename>.json
  └─ Write orphaned.json if orphaned non-empty
```

Error handling mirrors existing exporters: disk errors propagate as `RuntimeError::DiskError`; message parse errors are logged to stderr but do not abort the export.

---

## Testing

- Add `ExportType::Json` to all existing `Options::fake_options()` test cases
- Unit tests for the type mapping function (all 12 variants)
- Unit test for `meta.type` determination (group vs private)
- Filename generation uses existing `config.filename()` — no new tests needed there
