# JSON Export — Media Attachments & Avatars Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close two gaps in the ChatLab JSON exporter — copy media attachments alongside the JSON (parity with HTML/TXT) and emit base64 Data URL avatars for members and groups.

**Architecture:** All work lives in the `imessage-exporter` and `imessage-database` crates. A new `app/avatar.rs` module owns MIME sniffing and bytes→Data URL conversion (reusing the existing `sips`/`imagemagick` transcoder for HEIC/TIFF). `contacts.rs` is extended to read AddressBook image blobs. `json.rs` invokes the existing `AttachmentManager::handle_attachment` (same call HTML/TXT make), reformats `content` as labeled placeholders, and emits `members[].avatar` + `meta.groupAvatar` when `--embed-avatars` is on (default).

**Tech Stack:** Rust 2024, `rusqlite` (for SQL), `base64` (already in `imessage-database/Cargo.toml`), `jzon` (already in JSON exporter), external `sips`/`imagemagick` for HEIC/TIFF transcoding.

**Design spec:** `docs/superpowers/specs/2026-05-13-json-media-and-avatars-design.md`

---

## File Structure

| File | Role | Status |
|---|---|---|
| `imessage-database/src/tables/attachment.rs` | Add `Attachment::from_guid` for group-avatar lookup | Modify |
| `imessage-exporter/src/app/mod.rs` | Register new `avatar` module | Modify |
| `imessage-exporter/src/app/avatar.rs` | MIME sniff + bytes→Data URL helper (with HEIC transcode) | **New** |
| `imessage-exporter/src/app/contacts.rs` | Add `avatar_bytes` to `Name`; read from `ZABCDIMAGE`/`ABImage` | Modify |
| `imessage-exporter/src/app/options.rs` | Add `--embed-avatars` CLI flag + `Options.embed_avatars: bool` | Modify |
| `imessage-exporter/src/exporters/json.rs` | Invoke `AttachmentManager`; new `content` formatting; emit avatars | Modify |
| `CHANGELOG.md` | Document breaking content-format change + new flag | Modify |

No new crate dependencies. `base64` is already in `imessage-database/Cargo.toml`; either re-export it via `pub use` in `imessage-database` or add `base64 = "=0.22.1"` directly to `imessage-exporter/Cargo.toml`. We pick the latter for clarity (Task 3 adds it).

---

## Running Tests

This is a Cargo workspace. Commands used in every task:

```bash
# Run tests in a specific crate, filtering by name
cargo test -p imessage-database <test_name>
cargo test -p imessage-exporter <test_name>

# Run an entire module's tests
cargo test -p imessage-exporter exporters::json::tests
cargo test -p imessage-exporter app::avatar::tests

# Run all workspace tests (final smoke before merge)
cargo test --workspace
```

When a step says "Expected: PASS", the cargo output should include `test result: ok` with no failures in the filtered output.

---

## Task 1: `Attachment::from_guid` lookup in imessage-database

**Files:**
- Modify: `imessage-database/src/tables/attachment.rs`

This unblocks group-avatar resolution (chat properties give us a GUID; we need a way to look up the corresponding attachment row).

- [ ] **Step 1: Read existing `Attachment` struct definition**

Open `imessage-database/src/tables/attachment.rs` and locate the `impl Attachment` block (search for `impl Attachment {`). Note the existing `from_message` method — `from_guid` will follow the same `Table::from_row` pattern.

- [ ] **Step 2: Add failing test**

Append to the existing `#[cfg(test)] mod tests { ... }` block at the bottom of `attachment.rs`:

```rust
    #[test]
    fn from_guid_returns_none_when_no_match() {
        let db = crate::tables::table::get_connection(
            &std::path::PathBuf::from("test_data/imessage2/chat.db"),
        )
        .unwrap();
        let result = Attachment::from_guid(&db, "DOES-NOT-EXIST-0000").unwrap();
        assert!(result.is_none());
    }
```

If `test_data/imessage2/chat.db` does not exist, search for an existing test that opens the test DB and reuse its path. (Run `grep -rn "test_data" imessage-database/src/tables/attachment.rs` if unsure.)

- [ ] **Step 3: Run test to confirm it fails**

```
cargo test -p imessage-database tables::attachment::tests::from_guid_returns_none_when_no_match
```

Expected: FAIL with `no method named from_guid found for struct Attachment` (or similar).

- [ ] **Step 4: Implement `Attachment::from_guid`**

In `impl Attachment { ... }`, add:

```rust
    /// Look up a single attachment by its GUID.
    ///
    /// Used by the JSON exporter to resolve `chat.properties.group_photo_guid`
    /// to a concrete attachment row for the group's avatar image.
    pub fn from_guid(
        db: &rusqlite::Connection,
        guid: &str,
    ) -> Result<Option<Self>, crate::error::table::TableError> {
        use crate::tables::table::Table;
        let mut stmt = db.prepare("SELECT * FROM attachment WHERE guid = ?1 LIMIT 1")
            .map_err(crate::error::table::TableError::QueryError)?;
        let mut rows = stmt
            .query_map([guid], |row| Ok(Self::from_row(row)))
            .map_err(crate::error::table::TableError::QueryError)?;
        match rows.next() {
            Some(row_result) => Ok(Some(Self::extract(row_result)?)),
            None => Ok(None),
        }
    }
```

If the existing code uses `?` directly on rusqlite errors (instead of `.map_err`), follow that style — check the surrounding methods first.

- [ ] **Step 5: Run test to confirm it passes**

```
cargo test -p imessage-database tables::attachment::tests::from_guid_returns_none_when_no_match
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add imessage-database/src/tables/attachment.rs
git commit -m "feat(db): add Attachment::from_guid lookup"
```

---

## Task 2: Add `--embed-avatars` CLI flag

**Files:**
- Modify: `imessage-exporter/src/app/options.rs`

- [ ] **Step 1: Locate the CLI arg definitions**

Open `imessage-exporter/src/app/options.rs`. Search for `OPTION_ATTACHMENT_MANAGER` to find the existing `-c` flag's `arg!` block — this is the model for the new flag. Note the surrounding pattern: `Options` struct → `from_arg_matches` constructor → `cli` function → `fake_options` (test helper).

- [ ] **Step 2: Add failing test**

In the existing `#[cfg(test)] mod tests` block, add:

```rust
    #[test]
    fn embed_avatars_default_is_true() {
        let opts = Options::fake_options(crate::app::export_type::ExportType::Json);
        assert!(opts.embed_avatars);
    }

    #[test]
    fn embed_avatars_off_when_explicitly_disabled() {
        let mut opts = Options::fake_options(crate::app::export_type::ExportType::Json);
        opts.embed_avatars = false;
        assert!(!opts.embed_avatars);
    }
```

- [ ] **Step 3: Run tests to confirm they fail**

```
cargo test -p imessage-exporter app::options::tests::embed_avatars_
```

Expected: FAIL with `no field 'embed_avatars' on type 'Options'`.

- [ ] **Step 4: Add the field to `Options`**

In the `pub struct Options { ... }` block, add (preserving existing field order, near `attachment_manager`):

```rust
    /// Whether to embed contact and group avatars as base64 Data URLs in JSON export
    pub embed_avatars: bool,
```

- [ ] **Step 5: Add the field in the constructor**

Find `Options::from_arg_matches` (or whatever the actual constructor is named — search `impl Options` and look for the `fn from(...)` or `pub fn from_args(...)`). Just before the final `Ok(Self { ... })`, add:

```rust
        // Default: on. Only meaningful for -f json, but parse it the same way regardless;
        // cross-flag validation happens below.
        let embed_avatars = match args.get_one::<bool>(OPTION_EMBED_AVATARS).copied() {
            Some(value) => value,
            None => true,
        };
```

Inside the final `Ok(Self { ... })` literal, add `embed_avatars,`.

- [ ] **Step 6: Add the constant + the CLI definition**

Near the other `OPTION_*` constants at the top of the file, add:

```rust
pub const OPTION_EMBED_AVATARS: &str = "embed-avatars";
```

Find the `fn cli()` function (where `arg!` blocks live). Add a new arg block near the `OPTION_ATTACHMENT_MANAGER` one:

```rust
        .arg(
            arg!(--"embed-avatars" <ENABLED> "Embed contact and group avatars as base64 Data URLs in JSON export\nApplies only to -f json. Default: true")
                .required(false)
                .value_parser(clap::value_parser!(bool))
                .action(clap::ArgAction::Set),
        )
```

- [ ] **Step 7: Add cross-flag validation**

In the constructor, after `embed_avatars` is computed but before `Ok(Self { ... })`, add:

```rust
        // --embed-avatars only applies to -f json
        if args.get_one::<bool>(OPTION_EMBED_AVATARS).is_some()
            && !matches!(export_type, Some(ExportType::Json))
        {
            return Err(RuntimeError::InvalidOptions(format!(
                "{OPTION_EMBED_AVATARS} only applies when using `-f json`"
            )));
        }
```

(Adjust the `export_type` variable name to match the local binding in the constructor — search for how `OPTION_ATTACHMENT_MANAGER` is validated against `export_type` and mirror it.)

- [ ] **Step 8: Update `fake_options`**

In the same file, find `pub fn fake_options(...)` (the test helper). Add to its returned literal:

```rust
        embed_avatars: true,
```

Also update every other existing `Options { ... }` literal in tests across `options.rs` (run `grep -n "attachment_manager:" imessage-exporter/src/app/options.rs` to find them all). For each match, add `embed_avatars: true,` in the same braced literal.

- [ ] **Step 9: Run tests to confirm they pass**

```
cargo test -p imessage-exporter app::options::tests::embed_avatars_
cargo test -p imessage-exporter app::options::tests
```

Expected: PASS for both. The second command should also be all-green — confirming no `Options` literal was missed.

- [ ] **Step 10: Add a validation-error test**

Append to the same tests module:

```rust
    #[test]
    fn embed_avatars_with_html_export_is_error() {
        let cli_matches = cli().get_matches_from(vec![
            "imessage-exporter",
            "-f", "html",
            "-o", "/tmp/foo",
            "--embed-avatars=true",
        ]);
        let result = Options::from(&cli_matches);
        assert!(result.is_err(), "expected --embed-avatars with -f html to error");
    }
```

(If `Options::from(&matches)` is not the constructor — replace with the actual function name discovered in Step 5.)

- [ ] **Step 11: Run validation test**

```
cargo test -p imessage-exporter app::options::tests::embed_avatars_with_html_export_is_error
```

Expected: PASS.

- [ ] **Step 12: Commit**

```bash
git add imessage-exporter/src/app/options.rs
git commit -m "feat(cli): add --embed-avatars flag (default on, json-only)"
```

---

## Task 3: New `app/avatar.rs` — MIME sniffing + base64 Data URL (no transcode yet)

**Files:**
- Create: `imessage-exporter/src/app/avatar.rs`
- Modify: `imessage-exporter/src/app/mod.rs`
- Modify: `imessage-exporter/Cargo.toml`

This task lands the pure-Rust portion of the helper. HEIC/TIFF transcoding via external tools is added in Task 4.

- [ ] **Step 1: Add `base64` to `imessage-exporter/Cargo.toml`**

Open `imessage-exporter/Cargo.toml` and add under `[dependencies]`, alphabetized:

```toml
base64 = "=0.22.1"
```

- [ ] **Step 2: Register the new module**

Open `imessage-exporter/src/app/mod.rs` and add (alphabetical order):

```rust
pub mod avatar;
```

- [ ] **Step 3: Create `avatar.rs` with module skeleton + failing tests**

Create `imessage-exporter/src/app/avatar.rs`:

```rust
/*!
 Convert raw image bytes (from AddressBook / iMessage attachment) into a base64
 Data URL suitable for ChatLab's `members[].avatar` and `meta.groupAvatar` fields.
*/

/// Recognized image MIME types.  Anything not in this list returns `None` from `sniff_mime`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ImageMime {
    Jpeg,
    Png,
    Gif,
    Webp,
    Heic,
    Tiff,
}

impl ImageMime {
    /// The `image/...` string used in Data URLs.
    pub fn as_str(self) -> &'static str {
        match self {
            ImageMime::Jpeg => "image/jpeg",
            ImageMime::Png  => "image/png",
            ImageMime::Gif  => "image/gif",
            ImageMime::Webp => "image/webp",
            ImageMime::Heic => "image/heic",
            ImageMime::Tiff => "image/tiff",
        }
    }

    /// True when major browsers render the format inline via `<img>`.
    pub fn is_browser_renderable(self) -> bool {
        matches!(self, ImageMime::Jpeg | ImageMime::Png | ImageMime::Gif | ImageMime::Webp)
    }
}

/// Detect the image format from the leading magic bytes.  Returns `None` for unknown formats.
pub fn sniff_mime(bytes: &[u8]) -> Option<ImageMime> {
    if bytes.len() < 12 {
        return None;
    }
    // JPEG: FF D8 FF
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(ImageMime::Jpeg);
    }
    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(ImageMime::Png);
    }
    // GIF: 47 49 46 38 (matches GIF87a and GIF89a)
    if bytes.starts_with(&[0x47, 0x49, 0x46, 0x38]) {
        return Some(ImageMime::Gif);
    }
    // WebP: RIFF .... WEBP
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some(ImageMime::Webp);
    }
    // HEIC: ?? ?? ?? ?? "ftyp" then brand at offset 8
    if bytes.get(4..8) == Some(b"ftyp") {
        if let Some(brand) = bytes.get(8..12) {
            if matches!(brand, b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx"
                              | b"mif1" | b"msf1" | b"avif")
            {
                return Some(ImageMime::Heic);
            }
        }
    }
    // TIFF: little-endian or big-endian
    if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
        return Some(ImageMime::Tiff);
    }
    None
}

/// Encode bytes as a `data:image/...;base64,...` URL.  No transcoding.
fn encode_data_url(mime: ImageMime, bytes: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    format!("data:{};base64,{}", mime.as_str(), STANDARD.encode(bytes))
}

/// Convert raw image bytes to a base64 Data URL.  Returns `None` if the format is unrecognized
/// or (for HEIC/TIFF) transcoding is required but not yet available (Task 4 lifts that limit).
pub fn bytes_to_data_url(bytes: &[u8]) -> Option<String> {
    let mime = sniff_mime(bytes)?;
    if mime.is_browser_renderable() {
        Some(encode_data_url(mime, bytes))
    } else {
        // HEIC / TIFF: needs transcode — implemented in Task 4
        None
    }
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_jpeg_returns_jpeg() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 0];
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Jpeg));
    }

    #[test]
    fn sniff_png_returns_png() {
        let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Png));
    }

    #[test]
    fn sniff_gif_returns_gif() {
        let bytes = [0x47, 0x49, 0x46, 0x38, b'9', b'a', 0, 0, 0, 0, 0, 0];
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Gif));
    }

    #[test]
    fn sniff_webp_returns_webp() {
        let bytes = *b"RIFF\x00\x00\x00\x00WEBP";
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Webp));
    }

    #[test]
    fn sniff_heic_returns_heic() {
        let bytes = *b"\x00\x00\x00\x18ftypheic";
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Heic));
    }

    #[test]
    fn sniff_tiff_little_endian_returns_tiff() {
        let bytes = [0x49, 0x49, 0x2A, 0x00, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Tiff));
    }

    #[test]
    fn sniff_unknown_returns_none() {
        let bytes = [0u8; 32];
        assert_eq!(sniff_mime(&bytes), None);
    }

    #[test]
    fn sniff_short_input_returns_none() {
        assert_eq!(sniff_mime(&[1, 2, 3]), None);
    }

    #[test]
    fn bytes_to_data_url_jpeg_round_trips() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 0];
        let url = bytes_to_data_url(&bytes).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
        // Decode the base64 portion and verify it matches the input
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let b64 = url.strip_prefix("data:image/jpeg;base64,").unwrap();
        let decoded = STANDARD.decode(b64).unwrap();
        assert_eq!(&decoded[..], &bytes[..]);
    }

    #[test]
    fn bytes_to_data_url_heic_returns_none_without_transcoder() {
        // Until Task 4 lifts this limit, HEIC bytes can't become a Data URL.
        let bytes = *b"\x00\x00\x00\x18ftypheic";
        assert_eq!(bytes_to_data_url(&bytes), None);
    }

    #[test]
    fn bytes_to_data_url_unknown_returns_none() {
        let bytes = [0u8; 16];
        assert_eq!(bytes_to_data_url(&bytes), None);
    }
}
```

- [ ] **Step 4: Run tests to confirm all pass**

```
cargo test -p imessage-exporter app::avatar::tests
```

Expected: PASS for all 11 tests. If anything fails, fix inline before moving on.

- [ ] **Step 5: Verify the workspace still builds cleanly**

```
cargo build -p imessage-exporter
```

Expected: clean build, no warnings about unused imports in `avatar.rs`.

- [ ] **Step 6: Commit**

```bash
git add imessage-exporter/Cargo.toml imessage-exporter/src/app/mod.rs imessage-exporter/src/app/avatar.rs
git commit -m "feat(avatar): MIME sniffing + base64 Data URL helper"
```

---

## Task 4: `avatar.rs` — HEIC/TIFF transcode via existing `ImageConverter`

**Files:**
- Modify: `imessage-exporter/src/app/avatar.rs`

- [ ] **Step 1: Add failing test**

In the existing `#[cfg(test)] mod tests { ... }` in `avatar.rs`, add:

```rust
    #[test]
    fn bytes_to_data_url_with_converter_none_for_heic_returns_none() {
        let bytes = *b"\x00\x00\x00\x18ftypheic";
        assert_eq!(bytes_to_data_url_with_converter(&bytes, None), None);
    }

    #[test]
    fn bytes_to_data_url_with_converter_passes_through_jpeg() {
        // Even with a converter available, JPEG should not be transcoded — direct encode.
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 0];
        // We pass None for the converter because for JPEG we never reach the conversion path.
        let url = bytes_to_data_url_with_converter(&bytes, None).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
    }
```

- [ ] **Step 2: Run tests to confirm they fail**

```
cargo test -p imessage-exporter app::avatar::tests::bytes_to_data_url_with_converter_
```

Expected: FAIL — function not defined.

- [ ] **Step 3: Implement `bytes_to_data_url_with_converter`**

In `avatar.rs`, just below the existing `bytes_to_data_url` function, add:

```rust
use crate::app::compatibility::models::ImageConverter;

/// Variant of [`bytes_to_data_url`] that can transcode HEIC/TIFF input to JPEG via
/// the system image converter.  Returns `None` if the format is unrecognized, or
/// HEIC/TIFF input is given without a converter available, or transcoding fails.
pub fn bytes_to_data_url_with_converter(
    bytes: &[u8],
    converter: Option<&ImageConverter>,
) -> Option<String> {
    let mime = sniff_mime(bytes)?;
    if mime.is_browser_renderable() {
        return Some(encode_data_url(mime, bytes));
    }
    // HEIC / TIFF — needs transcode
    let converter = converter?;
    let transcoded = transcode_to_jpeg(bytes, mime, converter)?;
    Some(encode_data_url(ImageMime::Jpeg, &transcoded))
}

/// Write input bytes to a temp file, invoke the converter to produce a JPEG, read the result.
/// Cleans up both temp files before returning.
fn transcode_to_jpeg(
    bytes: &[u8],
    src_mime: ImageMime,
    converter: &ImageConverter,
) -> Option<Vec<u8>> {
    use std::fs::{File, remove_file, write};
    use std::io::Read;

    // Pick a stable temp-dir location
    let tmp_dir = std::env::temp_dir();
    let stem = format!("imex-avatar-{}", std::process::id());
    let src_ext = match src_mime {
        ImageMime::Heic => "heic",
        ImageMime::Tiff => "tiff",
        _ => return None,
    };
    let src_path = tmp_dir.join(format!("{stem}.{src_ext}"));
    let dst_path = tmp_dir.join(format!("{stem}.jpg"));

    if write(&src_path, bytes).is_err() {
        return None;
    }

    // Reuse the existing convert helper from the image module.
    // It runs `sips` or `imagemagick` to produce a JPEG at dst_path.
    let ok = crate::app::compatibility::converters::image::convert_to_jpeg_for_avatar(
        &src_path, &dst_path, converter,
    );

    let result = if ok {
        let mut buf = Vec::new();
        File::open(&dst_path).ok()?.read_to_end(&mut buf).ok()?;
        Some(buf)
    } else {
        None
    };

    let _ = remove_file(&src_path);
    let _ = remove_file(&dst_path);
    result
}
```

- [ ] **Step 4: Expose a public bytes→JPEG file conversion helper in `converters/image.rs`**

The existing `convert_heic` function in `imessage-exporter/src/app/compatibility/converters/image.rs` is private and tied to an `ImageType` enum. Add a sibling that's reusable from the avatar pipeline. Open that file and append:

```rust
/// Public helper: invoke the system image converter to produce a JPEG file at `to`
/// from `from`.  Returns `true` on success, `false` on failure.
///
/// Used by the avatar pipeline to transcode HEIC/TIFF AddressBook images to a
/// browser-renderable JPEG for inline embedding.
pub(crate) fn convert_to_jpeg_for_avatar(
    from: &Path,
    to: &Path,
    converter: &ImageConverter,
) -> bool {
    convert_heic(from, to, converter, &ImageType::Jpeg).is_some()
}
```

- [ ] **Step 5: Run tests to confirm they pass**

```
cargo test -p imessage-exporter app::avatar::tests::bytes_to_data_url_with_converter_
```

Expected: PASS. (Note: the HEIC-with-`None`-converter test passes because the function returns `None` early; we don't actually invoke `sips` in tests.)

- [ ] **Step 6: Update the `bytes_to_data_url` HEIC test to reflect the new API**

The earlier test `bytes_to_data_url_heic_returns_none_without_transcoder` still applies (no-converter variant returns None) — leave it alone. `bytes_to_data_url` itself is now a thin wrapper. Update its impl:

```rust
pub fn bytes_to_data_url(bytes: &[u8]) -> Option<String> {
    bytes_to_data_url_with_converter(bytes, None)
}
```

(Delete the original branching impl since it's now duplicated.)

Re-run all avatar tests:

```
cargo test -p imessage-exporter app::avatar::tests
```

Expected: All previously-green tests still pass.

- [ ] **Step 7: Commit**

```bash
git add imessage-exporter/src/app/avatar.rs imessage-exporter/src/app/compatibility/converters/image.rs
git commit -m "feat(avatar): HEIC/TIFF transcode via sips/imagemagick"
```

---

## Task 5: Extend `Name` with `avatar_bytes`; read from AddressBook

**Files:**
- Modify: `imessage-exporter/src/app/contacts.rs`

- [ ] **Step 1: Add failing tests**

In the existing `#[cfg(test)] mod tests` in `contacts.rs`, add:

```rust
    #[test]
    fn name_avatar_bytes_default_is_none() {
        let n = Name::from_opt(Some("A".to_string()), Some("B".to_string())).unwrap();
        assert!(n.avatar_bytes.is_none());
    }

    #[test]
    fn contacts_index_can_carry_avatar_bytes() {
        let mut index = ContactsIndex::default();
        let mut n = Name::from_opt(Some("A".to_string()), Some("B".to_string())).unwrap();
        n.avatar_bytes = Some(vec![0xFF, 0xD8, 0xFF]);
        index.index.insert("test@example.com".to_string(), n);
        let looked_up = index.lookup("test@example.com").unwrap();
        assert_eq!(looked_up.avatar_bytes, Some(vec![0xFF, 0xD8, 0xFF]));
    }
```

- [ ] **Step 2: Run tests to confirm they fail**

```
cargo test -p imessage-exporter app::contacts::tests::name_avatar_bytes_
cargo test -p imessage-exporter app::contacts::tests::contacts_index_can_carry_avatar_bytes
```

Expected: FAIL — no field `avatar_bytes`.

- [ ] **Step 3: Add the field to `Name`**

In `contacts.rs`, modify the `pub struct Name { ... }` block to add at the end:

```rust
    /// Raw image bytes from AddressBook (JPEG/PNG/HEIC/etc.).  `None` if the
    /// contact has no photo or the photo column was unreadable.
    pub avatar_bytes: Option<Vec<u8>>,
```

- [ ] **Step 4: Update `Name::from_opt`**

Inside `from_opt`, add to the returned literal:

```rust
            avatar_bytes: None,
```

- [ ] **Step 5: Update `Name::from_details`**

Same — add `avatar_bytes: None,` to the literal.

- [ ] **Step 6: Update `Name::fake_name`**

The `#[cfg(test)] impl Name { fn fake_name }` literal also needs `avatar_bytes: None,`.

- [ ] **Step 7: Run tests to confirm Step 1 tests now pass**

```
cargo test -p imessage-exporter app::contacts::tests
```

Expected: PASS for new tests; no regressions.

- [ ] **Step 8: Update `build_from_macos` to read `ZIMAGEDATA`**

Locate the existing `build_from_macos` function. The current SQL:

```rust
let mut stmt = conn.prepare(
    "SELECT r.ZFIRSTNAME, r.ZLASTNAME, p.ZFULLNUMBER, e.ZADDRESSNORMALIZED
     FROM ZABCDRECORD AS r
     LEFT JOIN ZABCDPHONENUMBER AS p ON r.Z_PK = p.ZOWNER
     LEFT JOIN ZABCDEMAILADDRESS AS e ON r.Z_PK = e.ZOWNER",
)?;
```

Replace with:

```rust
let mut stmt = conn.prepare(
    "SELECT r.ZFIRSTNAME, r.ZLASTNAME, p.ZFULLNUMBER, e.ZADDRESSNORMALIZED, img.ZIMAGEDATA
     FROM ZABCDRECORD AS r
     LEFT JOIN ZABCDPHONENUMBER AS p ON r.Z_PK = p.ZOWNER
     LEFT JOIN ZABCDEMAILADDRESS AS e ON r.Z_PK = e.ZOWNER
     LEFT JOIN ZABCDIMAGE         AS img ON r.Z_PK = img.ZOWNER",
)?;
```

In the row iteration loop, replace:

```rust
if let Some(name) = name {
```

with:

```rust
if let Some(mut name) = name {
    // Image data is in column 4 (after first/last/phone/email)
    if let Ok(Some(img_bytes)) = row.get::<_, Option<Vec<u8>>>(4) {
        if !img_bytes.is_empty() {
            name.avatar_bytes = Some(img_bytes);
        }
    }
```

…and keep the existing body but operate on the now-`mut name`. Critically, `upsert_best` already accepts an immutable name reference and clones, which is fine — but it overwrites based on `score()` alone. **Patch `upsert_best`** so that when it replaces an entry but the existing one had `avatar_bytes` and the incoming doesn't, the avatar is preserved:

```rust
fn upsert_best(map: &mut HashMap<String, Name>, key: String, incoming: &Name) {
    match map.get_mut(&key) {
        Some(existing) => {
            if incoming.score() > existing.score() {
                // Preserve avatar from the displaced entry if the new one lacks one
                let preserved_avatar = existing.avatar_bytes.take();
                *existing = incoming.clone();
                if existing.avatar_bytes.is_none() {
                    existing.avatar_bytes = preserved_avatar;
                }
            } else if existing.avatar_bytes.is_none() && incoming.avatar_bytes.is_some() {
                existing.avatar_bytes = incoming.avatar_bytes.clone();
            }
        }
        None => {
            map.insert(key, incoming.clone());
        }
    }
}
```

- [ ] **Step 9: Update `build_from_ios` similarly**

The existing query reads from `ABPersonFullTextSearch_content` which doesn't include image data. Add a second pass after the main query:

```rust
        // Second pass: attach avatars from ABImage, keyed by phone/email of the owner
        let mut avatar_stmt = conn.prepare(
            "SELECT p.ROWID, ph.value, em.value, i.data
             FROM ABPerson AS p
             LEFT JOIN ABMultiValue AS ph ON ph.record_id = p.ROWID AND ph.property = 3
             LEFT JOIN ABMultiValue AS em ON em.record_id = p.ROWID AND em.property = 4
             LEFT JOIN ABImage      AS i  ON i.record_id  = p.ROWID",
        )?;
        let mut rows = avatar_stmt.query([])?;
        while let Some(row) = rows.next()? {
            let phone: Option<String> = row.get(1)?;
            let email: Option<String> = row.get(2)?;
            let img: Option<Vec<u8>> = row.get(3)?;
            let Some(img_bytes) = img else { continue };
            if img_bytes.is_empty() { continue }

            // Attach to phone & email keys
            if let Some(phone) = phone {
                for key in phone_keys(&phone) {
                    if let Some(entry) = index.get_mut(&key) {
                        if entry.avatar_bytes.is_none() {
                            entry.avatar_bytes = Some(img_bytes.clone());
                        }
                    }
                }
            }
            if let Some(email) = email {
                if let Some(norm) = normalize_email(&email) {
                    if let Some(entry) = index.get_mut(&norm) {
                        if entry.avatar_bytes.is_none() {
                            entry.avatar_bytes = Some(img_bytes.clone());
                        }
                    }
                }
            }
        }
```

(`ABMultiValue.property` numbers `3` and `4` are iOS contacts conventions for phone/email — verify against the existing iOS test fixture before locking. If the actual numbers differ, the resulting query would silently return zero rows; the smoke test in Task 10 will catch it.)

- [ ] **Step 10: Add a helper `ContactsIndex::get_avatar`**

Right after `pub fn lookup` in `impl ContactsIndex`, add:

```rust
    /// Look up just the avatar bytes for a handle, mirroring `lookup`'s matching rules.
    ///
    /// Note: we re-walk the index directly (rather than calling `lookup`) because `lookup`
    /// clones the `Name`, which would force us to clone the avatar bytes on every call.
    pub fn get_avatar(&self, id: &str) -> Option<&[u8]> {
        for id_part in id.split_whitespace() {
            if looks_like_email(id_part) {
                if let Some(key) = normalize_email(id_part) {
                    if let Some(n) = self.index.get(&key) {
                        return n.avatar_bytes.as_deref();
                    }
                }
                continue;
            }
            for k in phone_keys(id_part) {
                if let Some(n) = self.index.get(&k) {
                    return n.avatar_bytes.as_deref();
                }
            }
        }
        None
    }
```

- [ ] **Step 11: Add a final integration test that exercises `get_avatar`**

```rust
    #[test]
    fn contacts_index_get_avatar_returns_bytes() {
        let mut index = ContactsIndex::default();
        let mut n = Name::from_opt(Some("A".to_string()), Some("B".to_string())).unwrap();
        n.avatar_bytes = Some(vec![1, 2, 3]);
        index.index.insert("foo@example.com".to_string(), n);
        assert_eq!(index.get_avatar("foo@example.com"), Some(&[1u8, 2, 3][..]));
    }

    #[test]
    fn contacts_index_get_avatar_returns_none_without_bytes() {
        let mut index = ContactsIndex::default();
        let n = Name::from_opt(Some("A".to_string()), Some("B".to_string())).unwrap();
        index.index.insert("foo@example.com".to_string(), n);
        assert_eq!(index.get_avatar("foo@example.com"), None);
    }
```

- [ ] **Step 12: Run all contacts tests**

```
cargo test -p imessage-exporter app::contacts::tests
```

Expected: PASS (no regressions, new tests green).

- [ ] **Step 13: Commit**

```bash
git add imessage-exporter/src/app/contacts.rs
git commit -m "feat(contacts): read AddressBook avatars (macOS + iOS)"
```

---

## Task 6: JSON exporter — invoke `AttachmentManager` + reformat `content` as `[Label] ...`

**Files:**
- Modify: `imessage-exporter/src/exporters/json.rs`

- [ ] **Step 1: Read the current `classify` impl and pin down what we're changing**

Open `imessage-exporter/src/exporters/json.rs` around lines 117–164. The current attachment branch:

```rust
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
```

This is what we're transforming.

- [ ] **Step 2: Add failing tests**

In the existing `#[cfg(test)] mod tests { ... }` in `json.rs`, add:

```rust
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
            compose_attachment_content(TYPE_IMAGE, Some("attachments/12/8421.jpeg")),
            "[Image] attachments/12/8421.jpeg"
        );
    }

    #[test]
    fn compose_attachment_content_without_path() {
        assert_eq!(compose_attachment_content(TYPE_IMAGE, None), "[Image]");
    }

    #[test]
    fn compose_attachment_content_with_empty_path_string() {
        assert_eq!(compose_attachment_content(TYPE_IMAGE, Some("")), "[Image]");
    }
```

The first test is illustrative; the bulk of behavior verification is on the pure formatter functions `content_label` and `compose_attachment_content`.

- [ ] **Step 3: Run tests to confirm failure**

```
cargo test -p imessage-exporter exporters::json::tests::label_for_
cargo test -p imessage-exporter exporters::json::tests::compose_attachment_content_
```

Expected: FAIL — functions don't exist.

- [ ] **Step 4: Implement the pure formatting helpers**

In `json.rs`, just above the existing `impl<'a> JSON<'a> { ... }` block, add:

```rust
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

/// Compose `content` for an attachment-bearing message: `"[Label] <path>"` if the path is
/// non-empty, otherwise just `"[Label]"`.
fn compose_attachment_content(t: u8, path: Option<&str>) -> String {
    let label = content_label(t);
    match path {
        Some(p) if !p.is_empty() => format!("{label} {p}"),
        _ => label.to_string(),
    }
}
```

- [ ] **Step 5: Run pure-helper tests to confirm pass**

```
cargo test -p imessage-exporter exporters::json::tests::label_for_
cargo test -p imessage-exporter exporters::json::tests::compose_attachment_content_
```

Expected: PASS.

- [ ] **Step 6: Wire `AttachmentManager` + the formatters into `classify`**

Replace the attachment branch in `classify` (lines ~146–160 in the current code) with:

```rust
        // Attachment-based messages
        let mut attachments = Attachment::from_message(self.config.data_source.db(), msg)?;
        if let Some(first) = attachments.first_mut() {
            // Copy/transcode the file (no-op when -c disabled)
            let _ = self.config.options.attachment_manager.handle_attachment(
                msg,
                first,
                self.config,
            );

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

            return Ok((t, Some(compose_attachment_content(t, path_str.as_deref()))));
        }
```

- [ ] **Step 7: Run all json tests + smoke build**

```
cargo test -p imessage-exporter exporters::json::tests
cargo build -p imessage-exporter
```

Expected: PASS. No build warnings.

- [ ] **Step 8: Commit**

```bash
git add imessage-exporter/src/exporters/json.rs
git commit -m "feat(json): copy attachments and emit labeled content"
```

---

## Task 7: JSON — voice transcription suffix

**Files:**
- Modify: `imessage-exporter/src/exporters/json.rs`

The spec calls for `[Voice] <path> — Transcription: <text>` when an audio message has a transcription. The transcription lives in `BubbleComponent::Attachment(AttachmentMeta).transcription`, populated by `msg.parse_body(db)` + `msg.apply_body(body)` (already done in `iter_messages`).

- [ ] **Step 1: Add failing test**

```rust
    #[test]
    fn compose_voice_content_with_transcription_appends_suffix() {
        assert_eq!(
            compose_voice_content("attachments/12/8422.caf", Some("on my way")),
            "[Voice] attachments/12/8422.caf — Transcription: on my way"
        );
    }

    #[test]
    fn compose_voice_content_no_transcription_uses_plain_label() {
        assert_eq!(
            compose_voice_content("attachments/12/8422.caf", None),
            "[Voice] attachments/12/8422.caf"
        );
    }

    #[test]
    fn compose_voice_content_empty_path_still_appends_transcription() {
        assert_eq!(
            compose_voice_content("", Some("hi")),
            "[Voice] — Transcription: hi"
        );
    }
```

- [ ] **Step 2: Run tests to confirm failure**

```
cargo test -p imessage-exporter exporters::json::tests::compose_voice_content_
```

Expected: FAIL — function not defined.

- [ ] **Step 3: Implement `compose_voice_content`**

In `json.rs`, alongside `compose_attachment_content`:

```rust
/// Compose voice-message `content` with an optional transcription suffix.
fn compose_voice_content(path: &str, transcription: Option<&str>) -> String {
    let label = content_label(TYPE_VOICE);
    let base = if path.is_empty() {
        label.to_string()
    } else {
        format!("{label} {path}")
    };
    match transcription {
        Some(t) if !t.is_empty() => format!("{base} — Transcription: {t}"),
        _ => base,
    }
}
```

- [ ] **Step 4: Use `compose_voice_content` in `classify`**

Locate the snippet from Task 6, Step 6. Branch on TYPE_VOICE:

```rust
            // Compute the path string (same as Task 6)
            let path_str: Option<String> = if first.copied_path.is_some() {
                Some(self.config.message_attachment_path(first))
            } else {
                first.filename().map(|s| {
                    std::path::Path::new(s)
                        .file_name()
                        .and_then(|os| os.to_str())
                        .unwrap_or(s)
                        .to_string()
                })
            };

            // For voice, also look up the transcription from the parsed body
            if t == TYPE_VOICE {
                let transcription = msg.components.iter().find_map(|c| {
                    if let imessage_database::tables::messages::models::BubbleComponent::Attachment(meta) = c {
                        meta.transcription.as_deref()
                    } else {
                        None
                    }
                });
                let content = compose_voice_content(path_str.as_deref().unwrap_or(""), transcription);
                return Ok((TYPE_VOICE, Some(content)));
            }

            return Ok((t, Some(compose_attachment_content(t, path_str.as_deref()))));
```

- [ ] **Step 5: Run all json tests**

```
cargo test -p imessage-exporter exporters::json::tests
```

Expected: PASS, including the three new `compose_voice_content_*` tests.

- [ ] **Step 6: Commit**

```bash
git add imessage-exporter/src/exporters/json.rs
git commit -m "feat(json): append transcription to voice message content"
```

---

## Task 8: JSON — emit `members[].avatar`

**Files:**
- Modify: `imessage-exporter/src/exporters/json.rs`

- [ ] **Step 1: Find where members are emitted in `serialize_conversation`**

In `json.rs`, locate the `members` builder in `serialize_conversation` (around lines 207–216):

```rust
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
```

We need to extend this with optional `avatar` entries. Avatars are sourced from the `Config`'s `ContactsIndex` keyed by the platformId — but `serialize_conversation` is a static method without `&self`. We pass them in as a third tuple element on the `members` field.

- [ ] **Step 2: Add a `Vec<u8>` avatar field to `ConversationBuffer.members` tuple**

Change the type from `Vec<(String, String)>` to `Vec<(String, String, Option<Vec<u8>>)>`. Locate the struct around line 60:

```rust
pub(crate) struct ConversationBuffer {
    chat_name: String,
    chat_type: &'static str,
    owner_id: String,
    members: Vec<(String, String, Option<Vec<u8>>)>,    // CHANGED
    messages: Vec<ChatLabMessage>,
}
```

Update `add_member`:

```rust
fn add_member(&mut self, platform_id: String, display_name: String, avatar: Option<Vec<u8>>) {
    if !self.members.iter().any(|(id, _, _)| id == &platform_id) {
        self.members.push((platform_id, display_name, avatar));
    }
}
```

And update every `members: vec![(...)]` literal:
- The conversation-creation site in `iter_messages` (look for `members: vec![(owner_id, owner_name)],`).
- The orphaned-buffer construction in `write_all`.
- All test fixtures inside the `#[cfg(test)] mod tests` block — search `vec![("` to find them.

For each, change two-tuple `(a, b)` → three-tuple `(a, b, None)`.

- [ ] **Step 3: Add failing test for avatar emission**

In tests:

```rust
    #[test]
    fn serialize_member_with_avatar_emits_data_url_key() {
        let buf = ConversationBuffer {
            chat_name: "Test".to_string(),
            chat_type: "private",
            owner_id: "Me".to_string(),
            members: vec![
                ("Me".to_string(), "Me".to_string(), None),
                ("+15555550100".to_string(), "Alice".to_string(),
                 Some(vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 0])),
            ],
            messages: Vec::new(),
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(json_str.contains("\"avatar\": \"data:image/jpeg;base64,"));
    }

    #[test]
    fn serialize_member_without_avatar_omits_avatar_key() {
        let buf = ConversationBuffer {
            chat_name: "Test".to_string(),
            chat_type: "private",
            owner_id: "Me".to_string(),
            members: vec![("Me".to_string(), "Me".to_string(), None)],
            messages: Vec::new(),
        };
        let json_str = JSON::serialize_conversation(&buf, 1_700_000_000);
        assert!(!json_str.contains("\"avatar\""));
    }
```

- [ ] **Step 4: Run tests to confirm failure**

```
cargo test -p imessage-exporter exporters::json::tests::serialize_member_with_avatar_
cargo test -p imessage-exporter exporters::json::tests::serialize_member_without_avatar_
```

Expected: FAIL — `avatar` key not emitted.

- [ ] **Step 5: Update `serialize_conversation` to emit `avatar`**

Replace the `members` builder with:

```rust
let members: Vec<JsonValue> = buf
    .members
    .iter()
    .map(|(pid, name, avatar_bytes)| {
        let mut obj = JsonValue::new_object();
        obj["platformId"] = pid.as_str().into();
        obj["accountName"] = name.as_str().into();
        if let Some(bytes) = avatar_bytes {
            if let Some(url) = crate::app::avatar::bytes_to_data_url(bytes) {
                obj["avatar"] = url.as_str().into();
            }
        }
        obj
    })
    .collect();
```

(This uses the no-converter variant; HEIC contacts will be skipped silently. Step 6 wires in the converter from the config.)

- [ ] **Step 6: Run tests to confirm pass**

```
cargo test -p imessage-exporter exporters::json::tests::serialize_member_with_avatar_
cargo test -p imessage-exporter exporters::json::tests::serialize_member_without_avatar_
```

Expected: PASS.

- [ ] **Step 7: Wire avatars in from `ContactsIndex` during `iter_messages`**

In `iter_messages`, when computing the sender's `sender_id` and `account_name`, also fetch avatar bytes. After the `let account_name = ...` line, add:

```rust
            // Source the avatar (only if --embed-avatars is on)
            let sender_avatar: Option<Vec<u8>> = if self.config.options.embed_avatars {
                msg.handle_id.and_then(|h| {
                    self.config.real_participants.get(&h).and_then(|&internal_id| {
                        self.config.participants.get(&internal_id)
                            .and_then(|n| {
                                // ContactsIndex lookup by Name.details (the handle string)
                                self.config.contacts_index.get_avatar(&n.details)
                                    .map(|s| s.to_vec())
                            })
                    })
                })
            } else {
                None
            };
```

(Verify that `Config` exposes `contacts_index`. If not, expose it via a getter — search `pub.*contacts_index` in `runtime.rs`. If it's private, add `pub` or a method.)

Update `buffer.add_member(sender_id, account_name)` → `buffer.add_member(sender_id.clone(), account_name.clone(), sender_avatar)`.

- [ ] **Step 8: Update tests in `iter_messages` test cases**

Run all json tests once more to confirm no `add_member` call-site mismatches:

```
cargo test -p imessage-exporter exporters::json::tests
cargo build -p imessage-exporter
```

Expected: PASS, clean build.

- [ ] **Step 9: Switch `serialize_conversation` to use the converter-aware Data URL builder**

The avatar bytes may include HEIC. Update the `serialize_conversation` snippet (Step 5) to consult the converter. Since `serialize_conversation` is `&self`-less, the cleanest path is to make it a method that captures `&self` — but it's currently a `JSON::serialize_conversation` associated fn. Two options:

**A. Convert to a method** — change `fn serialize_conversation(buf, exported_at)` to `fn serialize_conversation(&self, buf, exported_at)`. Update its single caller in `write_all`. Inside, pass `self.config.options.attachment_manager.image_converter.as_ref()` to `bytes_to_data_url_with_converter`.

**B. Pre-encode the Data URL during `iter_messages`** — Replace the `avatar_bytes` field on `ConversationBuffer.members` with `avatar_data_url: Option<String>` (already-encoded). Encode in `iter_messages` where `&self.config` is available; `serialize_conversation` just embeds the string.

**Choose option B.** It localizes I/O (transcoding) to the iteration phase and keeps serialization pure. Refactor:

- Change `ConversationBuffer.members` to `Vec<(String, String, Option<String>)>` (the third element is now an already-encoded Data URL string, or `None`).
- In `iter_messages`, after computing `sender_avatar` bytes, immediately encode:

```rust
            let sender_avatar_url: Option<String> = sender_avatar.and_then(|bytes| {
                let conv = self.config.options.attachment_manager.image_converter.as_ref();
                crate::app::avatar::bytes_to_data_url_with_converter(&bytes, conv)
            });
```

- Pass `sender_avatar_url` to `add_member`.
- In `serialize_conversation`, the member builder becomes:

```rust
if let Some(url) = avatar_url {
    obj["avatar"] = url.as_str().into();
}
```

- Update the failing tests from Step 3 to pass a pre-encoded Data URL string (since the test now bypasses the byte→URL conversion):

```rust
    members: vec![
        ("Me".to_string(), "Me".to_string(), None),
        ("+15555550100".to_string(), "Alice".to_string(),
         Some("data:image/jpeg;base64,/9j/4A==".to_string())),
    ],
```

And the assertion becomes:

```rust
        assert!(json_str.contains("\"avatar\": \"data:image/jpeg;base64,/9j/4A==\""));
```

- [ ] **Step 10: Run all json tests**

```
cargo test -p imessage-exporter exporters::json::tests
```

Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add imessage-exporter/src/exporters/json.rs imessage-exporter/src/app/runtime.rs
git commit -m "feat(json): emit members[].avatar as Data URL"
```

(`runtime.rs` only appears in the commit if a `pub` visibility change was needed in Step 7.)

---

## Task 9: JSON — emit `meta.groupAvatar`

**Files:**
- Modify: `imessage-exporter/src/exporters/json.rs`

- [ ] **Step 1: Add failing test**

```rust
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
```

- [ ] **Step 2: Run tests to confirm failure**

```
cargo test -p imessage-exporter exporters::json::tests::serialize_meta_with_group_avatar_
cargo test -p imessage-exporter exporters::json::tests::serialize_meta_without_group_avatar_
```

Expected: FAIL — `group_avatar_url` field doesn't exist on `ConversationBuffer`.

- [ ] **Step 3: Add the field**

Modify `ConversationBuffer`:

```rust
pub(crate) struct ConversationBuffer {
    chat_name: String,
    chat_type: &'static str,
    owner_id: String,
    members: Vec<(String, String, Option<String>)>,
    messages: Vec<ChatLabMessage>,
    /// Pre-encoded base64 Data URL for the group photo, when available
    group_avatar_url: Option<String>,
}
```

Update every `ConversationBuffer { ... }` literal (search `ConversationBuffer {`) to include `group_avatar_url: None,` in the initial creation and the orphaned-buffer literal.

- [ ] **Step 4: Update `serialize_conversation` to emit the key**

In the meta builder section:

```rust
let mut meta = JsonValue::new_object();
meta["name"] = buf.chat_name.as_str().into();
meta["platform"] = PLATFORM.into();
meta["type"] = buf.chat_type.into();
meta["ownerId"] = buf.owner_id.as_str().into();
if let Some(url) = &buf.group_avatar_url {
    meta["groupAvatar"] = url.as_str().into();
}
```

- [ ] **Step 5: Run tests to confirm pass**

```
cargo test -p imessage-exporter exporters::json::tests::serialize_meta_
```

Expected: PASS.

- [ ] **Step 6: Resolve the group avatar in `iter_messages`**

In `iter_messages`, when a new conversation buffer is created (the `or_insert_with(|| ConversationBuffer { ... })` block), the closure runs once per conversation. Inside, compute the group avatar:

```rust
let group_avatar_url: Option<String> = if self.config.options.embed_avatars && chat_type == "group" {
    self.config.chatrooms
        .iter()
        .find(|(rowid, _)| self.config.real_chatrooms.get(*rowid) == Some(&real_id))
        .map(|(_, chat)| chat)
        .and_then(|chat| chat.properties(self.config.data_source.db()))
        .and_then(|props| props.group_photo_guid)
        .and_then(|guid| {
            imessage_database::tables::attachment::Attachment::from_guid(
                self.config.data_source.db(),
                &guid,
            ).ok().flatten()
        })
        .and_then(|att| att.resolved_attachment_path(
            &self.config.options.platform,
            &self.config.options.db_path,
            self.config.options.attachment_root.as_deref(),
        ))
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| {
            let conv = self.config.options.attachment_manager.image_converter.as_ref();
            crate::app::avatar::bytes_to_data_url_with_converter(&bytes, conv)
        })
} else {
    None
};
```

Then add `group_avatar_url` to the `ConversationBuffer { ... }` literal.

- [ ] **Step 7: Build + run all json tests**

```
cargo build -p imessage-exporter
cargo test -p imessage-exporter exporters::json::tests
```

Expected: PASS, clean build.

- [ ] **Step 8: Commit**

```bash
git add imessage-exporter/src/exporters/json.rs
git commit -m "feat(json): emit meta.groupAvatar as Data URL"
```

---

## Task 10: CHANGELOG entry + manual smoke test

**Files:**
- Modify: `CHANGELOG.md` (or equivalent — verify the project's release-notes file at repo root)

- [ ] **Step 1: Locate the CHANGELOG**

Run `ls -la imessage-exporter/CHANGELOG.md CHANGELOG.md 2>/dev/null` from the repo root. Use whichever exists. If neither does, create `CHANGELOG.md` at the repo root with a `## Unreleased` section.

- [ ] **Step 2: Append CHANGELOG entry**

Under the most recent unreleased section (or create one), add:

```markdown
## Unreleased

### Added
- `-f json` exporter now copies media attachments alongside the JSON when `-c clone|basic|full` is given, matching `html`/`txt` behavior. Paths in `content` become relative to the export directory.
- New `--embed-avatars` flag (default `true`, json-only): embeds AddressBook contact avatars in `members[].avatar` and the iMessage group photo in `meta.groupAvatar` as base64 Data URLs. HEIC/TIFF avatars are transcoded to JPEG using the existing `sips`/`imagemagick` pipeline.

### Changed
- **Breaking (json):** `content` for media messages now uses labeled placeholders (`[Image] filename` or `[Image] attachments/12/8421.jpeg`) instead of the source database's absolute path. The previous behavior — emitting `/Users/.../Library/Messages/Attachments/...` — was a bug that made JSON exports non-portable.
```

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): JSON media attachments and avatars"
```

- [ ] **Step 4: Workspace-wide test**

```
cargo test --workspace
```

Expected: PASS for everything. Investigate any regression before moving on.

- [ ] **Step 5: Manual smoke test against the local iMessage DB**

```bash
# Use the project's own test fixture or your personal DB
cargo run -p imessage-exporter -- \
    -f json \
    -c clone \
    -o /tmp/imex-smoke \
    --embed-avatars=true
```

Verify:
- `/tmp/imex-smoke/<chat>.json` exists for each conversation
- `/tmp/imex-smoke/attachments/<chat_id>/<rowid>.<ext>` exists for media messages
- One JSON file contains a `content` like `"[Image] attachments/<n>/<n>.jpeg"`
- At least one `members[].avatar` field contains `data:image/jpeg;base64,...`
- If you have a group chat with a custom photo, `meta.groupAvatar` is present

If any of these fail, do NOT mark the task complete — file an issue or fix.

- [ ] **Step 6: Manual smoke — embed off + disabled copy**

```bash
cargo run -p imessage-exporter -- \
    -f json \
    -o /tmp/imex-smoke-bare \
    --embed-avatars=false
```

Verify:
- No `attachments/` folder
- No `avatar` keys in `members[]`
- No `groupAvatar` in `meta`
- `content` for media is `"[Image] IMG_xxxx.heic"` style (bare filename, no path, no absolute paths leaked)

---

## Open Questions (from spec) — verification during/after implementation

These were flagged in the design spec. Verify them during the implementation and update this section with results:

1. **Group photo GUID ↔ attachment row 1:1.** If `Attachment::from_guid` returns `None` for a real group chat with a known custom photo, investigate whether `groupPhotoGuid` is an asset ID rather than an attachment GUID. If so, document the limitation and gracefully omit `meta.groupAvatar`.
2. **iOS `ABImage.data` in encrypted backups.** Test by exporting from an encrypted iOS backup. If the bytes are encrypted at the row level, route through `decrypt_file` in `AttachmentManager`.
3. **`ZABCDIMAGE` schema across macOS versions.** Verify on at least two macOS versions if accessible.
4. **`ABMultiValue.property` numbers for phone (3) and email (4).** If the iOS smoke test returns zero avatars, check the actual property numbers via `SELECT DISTINCT property FROM ABMultiValue` on a sample DB.

---

## Self-Review Notes

Spec coverage (each section of the design spec → task that implements it):
- §2.1 Media copy + content reformatting → Task 6, 7
- §2.2 Contact avatars (`members[].avatar`) → Task 5 (data) + Task 8 (emission)
- §2.3 Group avatar (`meta.groupAvatar`) → Task 1 (lookup) + Task 9 (emission)
- §2.4 CLI surface (`--embed-avatars`) → Task 2
- §3 Output examples → Task 10 manual smoke verifies
- §4 Error handling → All `Option`-returning branches in Tasks 5, 8, 9 produce no errors; warnings via `eprintln!` are spec'd but not enforced by tests
- §5 Compatibility — CHANGELOG → Task 10
- §6 Open questions → tracked in this plan under "Open Questions" above

No placeholders. Every step has runnable code or commands. Type names referenced (`ImageMime`, `ConversationBuffer`, `Attachment::from_guid`, `bytes_to_data_url_with_converter`) are introduced in the task where first used and reused consistently downstream.
