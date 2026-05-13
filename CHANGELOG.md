# Changelog

## Unreleased

### Added
- `-f json` exporter now copies media attachments alongside the JSON when `-c clone|basic|full` is given, matching `html`/`txt` behavior. Paths in `content` become relative to the export directory.
- New `--embed-avatars` flag (default `true`, json-only): embeds AddressBook contact avatars in `members[].avatar` and the iMessage group photo in `meta.groupAvatar` as base64 Data URLs. HEIC/TIFF avatars are transcoded to JPEG using the existing `sips`/`imagemagick` pipeline.

### Changed
- **Breaking (json):** `content` for media messages now uses labeled placeholders (`[Image] filename` or `[Image] attachments/12/8421.jpeg`) instead of the source database's absolute path. The previous behavior — emitting `/Users/.../Library/Messages/Attachments/...` — was a bug that made JSON exports non-portable.

### Internal
- New `app::avatar` module owns MIME sniffing and bytes→Data URL conversion.
- New `Attachment::from_guid(db, guid)` lookup in `imessage-database`.
- `Name` (in `contacts.rs`) now carries optional `avatar_bytes` populated from `ZABCDIMAGE` (macOS) or `ABImage` (iOS).
- `ContactsIndex` gains a `get_avatar(handle_id)` helper that returns the borrowed bytes without cloning.
