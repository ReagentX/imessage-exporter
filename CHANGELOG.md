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

### Known Limitations
- Some modern macOS AddressBook schemas don't have a `ZABCDIMAGE` table at all — instead they may use `ZABCDLIKENESS` or store contact photos as external blob files under `.AddressBook-v22_SUPPORT/_EXTERNAL_DATA/`. In those cases this release falls back gracefully (names still load, avatars stay empty). Broader avatar source support is a follow-up.
- iOS `ABMultiValue.property` numbers for phone/email (3/4) are best-effort; if a backup's schema differs, the iOS avatar pass silently returns zero results.
