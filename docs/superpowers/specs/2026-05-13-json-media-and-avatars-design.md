# JSON Export — Media Attachments & Avatars — Design Spec

**Date:** 2026-05-13
**Builds on:** `docs/superpowers/specs/2026-04-03-chatlab-json-export-design.md`
**Format version:** ChatLab 0.0.2
**Spec reference:** https://chatlab.fun/cn/standard/chatlab-format.html

---

## Overview

Two gaps in the existing ChatLab JSON exporter are addressed together:

1. **Media attachments are not copied.** Unlike `html` and `txt` exporters, `json` never invokes `AttachmentManager::handle_attachment`. The `content` field for image/voice/video/file messages is written as the source database's *absolute* path on the local machine, and no file is copied alongside the JSON. Exports therefore cannot be shared — references break the moment the JSON leaves the host.
2. **Contact and group avatars are not emitted.** The ChatLab spec defines `members[].avatar` and `meta.groupAvatar` as base64 Data URLs (the ChatLab UI renders them directly via `<img :src>`), but the current exporter writes neither.

This spec extends the JSON exporter to:
- Drive the existing `AttachmentManager` pipeline (matching `html`/`txt` behavior).
- Format `content` for attachment-bearing messages as human-readable placeholder labels with optional relative paths, aligned with the existing parser conventions in ChatLab (Instagram, Telegram, etc.).
- Read contact avatars from the AddressBook database and group avatars from the iMessage `attachment` table (via the already-parsed `chat.properties.group_photo_guid`), MIME-detect, transcode HEIC/TIFF if needed, base64-encode, and emit as Data URLs.

The HTML and TXT exporters are unchanged.

---

## Background: What's Already Built

| Capability | Where | Status |
|---|---|---|
| Attachment copy + HEIC→JPEG + CAF/MOV→MP4 | `AttachmentManager::handle_attachment` in `imessage-exporter/src/app/compatibility/attachment_manager.rs` | Used by `html.rs:507` and `txt.rs:402`; not used by `json.rs` |
| Relative path from `Attachment::copied_path` | `Config::message_attachment_path` in `imessage-exporter/src/app/runtime.rs:105` | Already returns relative path when `copied_path` is set |
| `chat.properties.group_photo_guid` (plist parse of `groupPhotoGuid`) | `imessage-database/src/tables/chat.rs:34` | Already extracted; never consumed |
| `base64` crate | `imessage-database/Cargo.toml:20` (=0.22.1) | Available; no new dep needed |
| HEIC→JPEG via `sips`/`imagemagick` | `imessage-exporter/src/app/compatibility/converters/image.rs::image_copy_convert` | Currently file→file only; will be wrapped in a bytes→bytes helper |
| AddressBook reads | `imessage-exporter/src/app/contacts.rs::build_from_macos`/`build_from_ios` | Reads name + phone + email only; no image columns |

The truly *new* work is the AddressBook image read and a bytes→Data URL helper. Everything else is wiring.

---

## CLI Surface

One new flag:

```
--embed-avatars[=on|off]    Embed contact and group avatars as base64 Data URLs in members[].avatar
                            and meta.groupAvatar.  Default: on.  Applies only to -f json.
```

- A single boolean — controls both contact and group avatars together.
- Default **on**: avatars are valuable, file size cost is bounded (typical 10 contacts × ~50 KB JPEG ≈ 0.5 MB JSON).
- Flag is **only meaningful for `-f json`**. Passing it with `-f html`/`-f txt` is an error (matches how `-c` interacts with `-f`).
- No granular `members`-only / `group`-only option in this revision; can be split into an enum (`none|members|all`) later if a use case emerges. YAGNI.

The existing `-c <disabled|clone|basic|full>` controls media file copying and continues to apply to all formats including `-f json`.

---

## Files Changed

| File | Change |
|---|---|
| `imessage-exporter/src/exporters/json.rs` | Invoke `AttachmentManager::handle_attachment`; reformat `content` as labeled placeholders; emit `members[].avatar` and `meta.groupAvatar` when `embed_avatars` is on |
| `imessage-exporter/src/app/options.rs` | Add `--embed-avatars` flag; add `embed_avatars: bool` to `Options`; validate flag is only used with `-f json` |
| `imessage-exporter/src/app/contacts.rs` | Add image-blob reads from `ZABCDIMAGE`/`ABImage`; extend `Name` with `avatar_bytes: Option<Vec<u8>>`; expose lookup |
| `imessage-exporter/src/app/avatar.rs` *(new)* | MIME sniffing from magic bytes; bytes→Data URL with optional HEIC/TIFF transcode via the existing `ImageConverter` |
| `imessage-exporter/src/app/mod.rs` | Register `pub mod avatar;` |
| `imessage-database/src/tables/attachment.rs` | Add `Attachment::from_guid(db, guid) -> Option<Attachment>` lookup (used for group avatar resolution) |

No new crate dependencies. `base64` is already in `imessage-database` and will be re-exported or used directly from there.

---

## Design

### Part 1 — Media Attachment Copying

In `json.rs::classify`, before computing the attachment path, call:

```rust
self.config.options.attachment_manager.handle_attachment(msg, attachment, self.config);
```

This is the same call HTML/TXT make. It honors `-c <mode>` (Disabled/Clone/Basic/Full), sets `attachment.copied_path` on success, and silently no-ops in `Disabled` mode.

After the call, `self.config.message_attachment_path(attachment)` returns:
- The **relative** path under the export dir when the file was copied
- The original source path otherwise (kept for fallback but **never written verbatim** into JSON — see content formatting below)

#### `content` field formatting

The ChatLab UI does not render media messages — it shows `content` as text. ChatLab's own parsers (Instagram, Telegram, QQ-native, etc.) write human-readable labels like `[图片] photo.uri` or `[photo]`. We follow that convention:

| Message type | `-c disabled` (no copy) | `-c clone/basic/full` (copy succeeded) |
|---|---|---|
| Image (`type=1`) | `[Image] IMG_5102.heic` | `[Image] attachments/12/8421.jpeg` |
| Voice (`type=2`), no transcription | `[Voice] 0001.caf` | `[Voice] attachments/12/8421.caf` |
| Voice with transcription | `[Voice] 0001.caf — Transcription: hello world` | `[Voice] attachments/12/8421.caf — Transcription: hello world` |
| Video (`type=3`) | `[Video] movie.mov` | `[Video] attachments/12/8421.mp4` |
| File (`type=4`) | `[File] notes.pdf` | `[File] attachments/12/8421.pdf` |
| Sticker (`type=5`) | `[Sticker] sticker.heic` | `[Sticker] attachments/12/8421.heic` |
| Missing filename | `[Image]` (no suffix) | `[Image] attachments/12/8421.jpeg` |

Rules:
- The bracketed label always comes first and matches the ChatLab `MessageType` enum verbatim.
- If a path or filename is present, append ` ` (single space) + the value.
- **Never embed an absolute path** — only relative-to-export-dir, or bare filename, or just the label.
- Transcription is appended with ` — Transcription: ` (em-dash with spaces), mirroring the `txt` exporter's format style. Source: each `BubbleComponent::Attachment(AttachmentMeta)` produced by `Message::parse_body(db)` carries an optional `transcription`. `parse_body` is already called in `iter_messages` (line 383-385); `classify` needs to be widened to accept the parsed body (or the relevant `AttachmentMeta`) so it can read the transcription for the first attachment-bearing bubble.
- For multiple attachments on one message (rare), only the first attachment drives `type` and `content` (matching current `classify()` behavior); additional attachments are still copied to disk but not referenced in `content`. This is consistent with the current single-attachment classification logic.

### Part 2 — Contact Avatars (`members[].avatar`)

#### AddressBook reads (`contacts.rs`)

**macOS** — extend the existing query in `build_from_macos`:

```sql
SELECT r.Z_PK, r.ZFIRSTNAME, r.ZLASTNAME,
       p.ZFULLNUMBER, e.ZADDRESSNORMALIZED,
       img.ZIMAGEDATA
FROM ZABCDRECORD AS r
LEFT JOIN ZABCDPHONENUMBER AS p ON r.Z_PK = p.ZOWNER
LEFT JOIN ZABCDEMAILADDRESS AS e ON r.Z_PK = e.ZOWNER
LEFT JOIN ZABCDIMAGE         AS img ON r.Z_PK = img.ZOWNER
```

**iOS** — `ABPersonFullTextSearch_content` doesn't carry images. Add a second query joining `ABImage`:

```sql
SELECT p.ROWID, i.data
FROM ABPerson AS p
LEFT JOIN ABImage AS i ON i.record_id = p.ROWID
```

The image bytes are attached to the `Name` row alongside name/phone/email. New field on `Name`:

```rust
pub struct Name {
    pub first: String,
    pub last: String,
    pub full: String,
    pub details: String,
    pub handle_ids: HashSet<i32>,
    pub avatar_bytes: Option<Vec<u8>>,   // NEW — raw image bytes from AddressBook
}
```

A contact may legitimately have **no** avatar; the field is `None` in that case.

The `upsert_best` logic stays the same — `score()` is still about name completeness, not avatar presence. (If a higher-scoring name lacks an avatar but a lower-scoring duplicate has one, the avatar is *lost* in this revision. Acceptable for v1 because the duplicate scenario is rare; flagged in Open Questions.)

#### Bytes → Data URL helper (`avatar.rs`, new)

```rust
/// Convert raw image bytes to a base64 Data URL, transcoding HEIC/TIFF to JPEG via
/// the system image converter if needed.  Returns None if the format is unrecognized
/// or transcoding fails (callers log and skip).
pub fn bytes_to_data_url(
    bytes: &[u8],
    converter: Option<&ImageConverter>,
) -> Option<String>;
```

Internal flow:
1. **MIME sniff** by magic bytes (no `infer` crate):
   - `FF D8 FF` → `image/jpeg`
   - `89 50 4E 47 0D 0A 1A 0A` → `image/png`
   - `47 49 46 38` → `image/gif`
   - `52 49 46 46 .. .. .. .. 57 45 42 50` → `image/webp`
   - `?? ?? ?? ?? 66 74 79 70` (ftyp box at offset 4) with `heic`/`heif`/`mif1`/`msf1` brand → `image/heic`
   - `49 49 2A 00` or `4D 4D 00 2A` → `image/tiff`
   - other → return `None`
2. For browser-renderable formats (`jpeg`/`png`/`gif`/`webp`) — base64-encode bytes directly.
3. For `heic`/`tiff` — write bytes to a tempfile, invoke the existing image converter (`sips` or `imagemagick`) targeting JPEG into a second tempfile, read it back, base64-encode, clean both tempfiles. If `converter` is `None` or the command fails → return `None` (warning logged by caller).
4. Format as `data:<mime>;base64,<b64>` and return.

No resizing in v1. AddressBook avatars are typically ≤512×512; base64 size is bounded enough. A `--avatar-size-limit` knob can be added later if needed.

#### Wiring in `json.rs`

When building `members`:

```rust
let avatar = if self.config.options.embed_avatars {
    name.avatar_bytes
        .as_deref()
        .and_then(|b| avatar::bytes_to_data_url(b, image_converter))
} else {
    None
};
```

If `avatar` is `Some`, emit `members[i].avatar = "data:...;base64,..."`; otherwise omit the key entirely (the field is optional in the spec).

### Part 3 — Group Avatar (`meta.groupAvatar`)

#### Resolution

```
chat.properties(db).group_photo_guid   →   Attachment::from_guid(db, guid)   →   resolved bytes   →   bytes_to_data_url
```

`Attachment::from_guid` is new (in `imessage-database/src/tables/attachment.rs`):

```rust
impl Attachment {
    pub fn from_guid(db: &Connection, guid: &str) -> Result<Option<Attachment>, TableError>;
}
```

It runs `SELECT * FROM attachment WHERE guid = ?1 LIMIT 1` and constructs the row via the existing `Table::from_row` impl.

In `json.rs`, when building each `ConversationBuffer`'s metadata:

```rust
let group_avatar = if self.config.options.embed_avatars && chat_type == "group" {
    chat.properties(db)
        .and_then(|p| p.group_photo_guid)
        .and_then(|guid| Attachment::from_guid(db, &guid).ok().flatten())
        .and_then(|att| att.resolved_attachment_path(
            &config.options.platform,
            &config.options.db_path,
            config.options.attachment_root.as_deref(),
        ))
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| avatar::bytes_to_data_url(&bytes, image_converter))
} else {
    None
};
```

If any step fails (no plist, no GUID, GUID not in attachment table, file missing, unsupported format, no converter), `meta.groupAvatar` is omitted (`spec: optional`).

The group avatar **does not** go through `AttachmentManager` — it is consumed entirely as bytes inline, not copied as a sibling file. (Could revisit if users want a sibling copy too, but the Data URL alone covers the ChatLab UI use case.)

### Part 4 — Error Handling

| Situation | Behavior |
|---|---|
| `--embed-avatars` passed without `-f json` | CLI error at parse time, mirrors existing `-c` × `-f` cross-validation in `options.rs` |
| AddressBook DB missing or unreadable | Existing behavior unchanged: build empty `ContactsIndex`; avatars will all be `None` |
| Single contact's image row exists but bytes are NULL/empty | Treated as no avatar (`None`), no error |
| MIME unrecognized | `bytes_to_data_url` returns `None`; `eprintln!` a warning once (`Skipped avatar for <handle>: unrecognized image format`); avatar field omitted |
| Transcoder missing (HEIC, no `sips`/`imagemagick`) | Same as above — warning + omit. Does not fail the export. |
| Group photo GUID present but no matching attachment row | Warning + omit |
| Attachment copy fails (existing behavior) | Existing eprintln behavior in `AttachmentManager`; message still emitted with placeholder + bare filename |

---

## Output Examples

### Group chat with avatars on (default)

```json
{
  "chatlab": { "version": "0.0.2", "exportedAt": 1747117234, "generator": "imessage-exporter" },
  "meta": {
    "name": "Family",
    "platform": "imessage",
    "type": "group",
    "ownerId": "+15551234567",
    "groupAvatar": "data:image/jpeg;base64,/9j/4AAQSkZJRg..."
  },
  "members": [
    { "platformId": "+15551234567", "accountName": "Me" },
    { "platformId": "+15555550100", "accountName": "Alice Chen",
      "avatar": "data:image/jpeg;base64,/9j/4AAQSkZJRg..." },
    { "platformId": "+15555550101", "accountName": "Bob Smith" }
  ],
  "messages": [
    { "sender": "+15555550100", "accountName": "Alice Chen",
      "timestamp": 1747100000, "type": 0,
      "content": "let's go!",
      "platformMessageId": "ABC-123" },
    { "sender": "+15555550100", "accountName": "Alice Chen",
      "timestamp": 1747100050, "type": 1,
      "content": "[Image] attachments/12/8421.jpeg",
      "platformMessageId": "ABC-124" },
    { "sender": "+15555550101", "accountName": "Bob Smith",
      "timestamp": 1747100100, "type": 2,
      "content": "[Voice] attachments/12/8422.caf — Transcription: on my way",
      "platformMessageId": "ABC-125" }
  ]
}
```

### Same export with `--embed-avatars=off` and `-c disabled`

```json
{
  "meta": { "name": "Family", "type": "group", "ownerId": "+15551234567" },
  "members": [
    { "platformId": "+15555550100", "accountName": "Alice Chen" }
  ],
  "messages": [
    { "sender": "+15555550100", "accountName": "Alice Chen",
      "timestamp": 1747100050, "type": 1,
      "content": "[Image] IMG_5102.heic",
      "platformMessageId": "ABC-124" }
  ]
}
```

Note: no `groupAvatar`, no `avatar` keys; `content` carries only the filename, no path.

---

## Test Strategy

Following the existing pattern in `json.rs` tests:

**Unit tests (`json.rs` module)**
- `classify_image_with_copy_emits_relative_path_label` — fake `Attachment` with `copied_path` set → `content == "[Image] <rel>"`.
- `classify_image_without_copy_emits_bare_filename_label` — `copied_path = None` → `content == "[Image] <filename>"`, no absolute path.
- `classify_voice_with_transcription_appends_transcription` — verifies the `— Transcription:` suffix.
- `classify_sticker_uses_sticker_label_not_image` — sticker attachment classified as `[Sticker]` even though sticker MediaType might be Image.

**Unit tests (`avatar.rs` module)**
- `sniff_jpeg_returns_image_jpeg` / `sniff_png_returns_image_png` / `sniff_gif_returns_image_gif` / `sniff_webp_returns_image_webp` / `sniff_heic_returns_image_heic` / `sniff_unknown_returns_none`.
- `bytes_to_data_url_jpeg_returns_data_url` — round-trip a real JPEG fixture.
- `bytes_to_data_url_unknown_returns_none`.
- `bytes_to_data_url_heic_without_converter_returns_none`.

**Unit tests (`contacts.rs` module)**
- `name_avatar_bytes_default_is_none`.
- `build_from_macos_reads_image_blob` — using a test fixture `.abcddb` with a known image row.

**Integration / smoke**
- End-to-end run against the test database (`imessage-database/test_data/`) with `-f json --embed-avatars=on -c clone` and assert: produced JSON parses; for at least one image-bearing message `content` starts with `[Image] `; if test DB has chat properties with `groupPhotoGuid`, `meta.groupAvatar` is a Data URL.

---

## Compatibility & Migration

- **Behavior change (potentially breaking for early adopters of `-f json`):** the `content` field for media messages no longer contains a source-machine absolute path. New format is `[Label] <filename or relative path>`. The ChatLab JSON exporter shipped only recently (commit `c30ebab`, 2026-04-03), so the surface area of consumers is small.
- **CHANGELOG entry required.** Section: "JSON exporter — breaking content format change for media messages; new `--embed-avatars` flag."
- ChatLab format version stays at `0.0.2`. No new top-level fields beyond what the spec already defines as optional.

---

## Open Questions / Implementation-time Validation

These don't block design approval but need verification when coding:

1. **Group photo GUID ↔ attachment row.** Confirmed in source that `chat.properties` parses `groupPhotoGuid`, but no existing code looks it up in the attachment table. Need real-DB validation that the GUID matches `attachment.guid` 1:1. If it doesn't (e.g., it's an asset ID rather than an attachment GUID), fall back to omitting `meta.groupAvatar` and document.
2. **iOS `ABImage.data` in encrypted backups.** May or may not be encrypted at the row level. If it is, route through the existing `decrypt_file` plumbing in `AttachmentManager`. Test on an encrypted backup.
3. **macOS `ZABCDIMAGE` schema stability.** Column names verified for `AddressBook-v22.abcddb`. Pre-v22 schemas (older macOS) are not supported by the current contacts module either, so no regression.
4. **Duplicate-contact avatar loss.** When `upsert_best` keeps the higher-scoring name but the lower-scoring duplicate is the one with `avatar_bytes`, the avatar is dropped. Consider merging: keep higher-scoring name, but take the avatar from whichever side has one. Defer to follow-up if it shows up in practice.
5. **Should `--embed-avatars` accept `auto`** that omits avatars when no contacts DB was provided? Probably overkill — when there's no DB, `avatar_bytes` is `None` anyway and `bytes_to_data_url` is never called; no extra flag value needed.
