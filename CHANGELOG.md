# Changelog

## Unreleased

### Added
- `-f json` exporter now copies media attachments alongside the JSON when `-c clone|basic|full` is given, matching `html`/`txt` behavior. All attachments on multi-attachment messages are copied; paths in `content` become relative to the export directory.
- New `--embed-avatars` flag (default `true`, json-only): embeds AddressBook contact avatars in `members[].avatar` and the iMessage group photo in `meta.groupAvatar` as base64 Data URLs. HEIC/TIFF avatars are transcoded to JPEG using the existing `sips`/`imagemagick` pipeline.
- JSON `content` for attachment messages now preserves any accompanying user text (e.g. a caption typed alongside a photo) after the placeholder, separated by an em-dash: `[Image] attachments/12/8421.jpeg — look at this`. Voice transcriptions and captions stack in the same order.

### Changed
- **Breaking (json):** `content` for media messages now uses labeled placeholders (`[Image] filename` or `[Image] attachments/12/8421.jpeg`) instead of the source database's absolute path. The previous behavior — emitting `/Users/.../Library/Messages/Attachments/...` — was a bug that made JSON exports non-portable.

### Internal
- New `app::avatar` module owns MIME sniffing and bytes→Data URL conversion.
- New `Attachment::from_guid(db, guid)` lookup in `imessage-database`.
- `Name` (in `contacts.rs`) now carries optional `avatar_bytes` populated from `ZABCDIMAGE` (macOS) or `ABImage` (iOS).
- `ContactsIndex` gains a `get_avatar(handle_id)` helper that returns the borrowed bytes without cloning.

### Known Limitations
- Some modern macOS AddressBook schemas don't have a `ZABCDIMAGE` table at all — instead they may use `ZABCDLIKENESS` or store contact photos as external blob files under `.AddressBook-v22_SUPPORT/_EXTERNAL_DATA/`. In those cases this release falls back gracefully (names still load, avatars stay empty). Broader avatar source support is a follow-up.
- iOS `ABMultiValue.property` numbers for phone/email (3/4) are best-effort; if a backup's schema differs, the iOS avatar pass silently returns zero results.
- The group avatar resolution reads attachment bytes via `fs::read`, which works for macOS sources and unencrypted iOS backups but bypasses the encrypted-backup decryption layer. On encrypted iOS backups, `meta.groupAvatar` is silently omitted even when the chat has a custom group photo. Routing the read through `AttachmentManager` (or a dedicated decrypt path) is a follow-up.
- Shared-location start/stop events do not produce a dedicated system message in the JSON output — they currently fall through to a `type: 0, content: null` entry. The `html`/`txt` exporters render these explicitly; bringing parity to JSON is a follow-up.
- When attachment copying is requested (`-c clone|basic|full`) but the source file cannot be read, decrypted, or copied, the JSON exporter still writes the bare filename in `content`. There is no in-band signal that the copy failed; consumers should treat any path that doesn't resolve on disk as missing. Emitting an explicit marker (matching the `html`/`txt` behavior) is a follow-up.
