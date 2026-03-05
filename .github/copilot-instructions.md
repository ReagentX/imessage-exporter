# Copilot Instructions for imessage-exporter

This repository contains a Rust workspace with two crates for parsing and exporting iMessage data from SQLite databases.

## Build, Test, and Lint

### Building
```bash
cargo build                  # Debug build
cargo build --release        # Release build with LTO optimizations
```

### Testing
```bash
cargo test                   # Run all tests in workspace
cargo test <test_name>       # Run a specific test by name
cargo test -p imessage-database        # Test only the database library
cargo test -p imessage-exporter        # Test only the exporter binary
```

### Linting
```bash
cargo clippy                 # Run Clippy on entire workspace
```

### Running the Binary
```bash
cargo run --release -- <args>          # Run with args
```

## Architecture

This is a Cargo workspace with two members:

### `imessage-database` (Library)
The database library parses and models iMessage SQLite data as cross-platform Rust structures.

**Key modules:**
- `tables/` - Database table models (messages, attachments, chats, handles)
  - `messages/` - Message parsing with multi-part support, threading, edits
  - Complex SQL queries join multiple tables to provide enriched data (chat_id, num_attachments, deleted_from, num_replies)
- `message_types/` - Specialized message content parsers (apps, stickers, digital touch, handwriting, URLs, polls, etc.)
- `util/` - Helper modules for dates, paths, parsing proprietary formats
  - `streamtyped.rs` - Parses Apple's `typedstream` format via crabstep
  - `plist.rs` - Handles plist payloads
- `error/` - Error types for each parsing domain

**Design patterns:**
- Uses `Table` trait for consistent database interaction
- `stream()` method pattern for memory-efficient iteration over large tables
- Most functions in `message.rs` are self-documenting with extensive module-level docs showing SQL query requirements

### `imessage-exporter` (Binary)
The binary exports iMessage data to HTML or TXT formats with optional media conversion.

**Key modules:**
- `app/` - Application logic and configuration
  - `runtime.rs` - Main `Config` struct coordinates export process
  - `options.rs` - CLI argument parsing and validation
  - `compatibility/` - Cross-platform attachment handling and format conversion
    - `converters/` - Image (HEIC→JPEG/PNG/GIF), audio (CAF→MP4), video (MOV→MP4) conversions
    - `attachment_manager.rs` - Copies/converts attachments based on mode
- `exporters/` - Format-specific export implementations
  - Implement `Exporter` trait for iteration and `Writer` trait for message formatting
  - `html.rs` - Generates HTML with embedded CSS, handles styling, threading UI
  - `txt.rs` - Plaintext export with threading annotations

**Export workflow:**
1. Parse CLI options → `Options`
2. Create `Config` with database connections and caches
3. Resolve contacts and filtered handles
4. Iterate messages via exporter's `iter_messages()`
5. Format each message type with appropriate `Writer` trait methods
6. Copy/convert attachments if enabled

## Key Conventions

### Code Style
- **Safety:** Both crates use `#![forbid(unsafe_code)]`
- **Documentation:** `imessage-database` uses `#![deny(missing_docs)]` - all public APIs must be documented
- **Comments:** Module-level docs (with `/*!` syntax) explain usage patterns and provide examples
- Minimal inline comments - code should be self-documenting

### Error Handling
- Errors are organized by domain in `error/` modules
- Each parsing domain has its own error type (e.g., `AttachmentError`, `MessageError`, `TableError`)
- Custom error types, not generic `anyhow`

### Database Queries
- Message queries must include synthetic columns: `chat_id`, `num_attachments`, `deleted_from`, `num_replies`
- Use LEFT JOINs with `chat_message_join` and `chat_recoverable_message_join`
- See `imessage-database/src/tables/messages/message.rs` module docs for required SQL patterns

### Testing
- Unit tests are in `#[cfg(test)]` modules within source files
- Extensive test coverage in `imessage-database/src/tables/messages/tests/`
- Doc tests validate API examples in documentation

### Platform Support
- Targets macOS, Linux, and Windows
- macOS-specific features (attachment conversion) fall back to external tools (ImageMagick, ffmpeg) on other platforms
- iOS backup support via `crabapple` for encrypted backups

### Version Management
- Both crates use `version = "0.0.0"` in development
- `build.sh` temporarily updates versions from git tags during release builds
- The binary depends on the library via path in development, version string in releases

### Constants
- Use descriptive const names for paths: `DEFAULT_ATTACHMENT_ROOT`, `DEFAULT_MESSAGES_ROOT`
- Magic numbers get named constants (e.g., `TIMESTAMP_FACTOR`)

### Module Organization
- Public modules re-export via `pub use` in parent module
- Use module-level docs to explain usage patterns
- Related functionality grouped in subdirectories (e.g., `message_types/`, `exporters/`)
