/*!
 SQLite database exporter.

 Reads the iMessage `chat.db` and produces a new, intelligible SQLite database
 containing the conversations in a structured, easy-to-query format. Message
 bodies (including app messages, edits, expressives, etc.) are formatted using
 the same logic as the [`TXT`] exporter, so the `body` column is human-readable
 and downstream-friendly.

 The output is written to `<export_path>/messages.db` and contains the
 following tables:

 - `chats(id, chat_identifier, service_name, display_name, derived_name)`
 - `handles(id, contact_id, display_name)`
 - `chat_handles(chat_id, handle_id)`
 - `messages(id, guid, chat_id, handle_id, sender, is_from_me, date_utc,
            date_unix_ns, date_read_utc, date_delivered_utc, date_edited_utc,
            is_reply, thread_originator_guid, is_edited, is_deleted,
            is_announcement, is_tapback, is_attachment_only,
            subject, body, service)`
 - `attachments(id, message_id, filename, mime_type, transfer_name)`
 - `tapbacks(id, target_guid, target_part, action, kind, sender, date_utc)`

 In addition, a convenience view `conversation_view` joins messages with their
 chat for simple ordered reading.
*/

use std::{fs::File, io::BufWriter, path::PathBuf};

use rusqlite::{Connection, params};

use imessage_database::{
    error::table::TableError,
    message_types::variants::{Tapback, TapbackAction, Variant},
    tables::{
        attachment::Attachment,
        messages::Message,
        table::{ORPHANED, Table},
    },
    util::dates::format as format_date,
};

use crate::{
    app::{error::RuntimeError, progress::ExportProgress, runtime::Config},
    exporters::{
        exporter::{Exporter, MessageFormatter},
        txt::TXT,
    },
};

/// Filename of the SQLite database emitted into the export directory
const OUTPUT_DB_NAME: &str = "messages.db";

// MARK: Exporter
pub struct SQL<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config,
    /// SQLite connection for the output database
    conn: Connection,
    /// Writer for the orphaned sentinel file. The [`Exporter`] trait requires
    /// us to return a [`BufWriter<File>`] from [`get_or_create_file`], but the
    /// SQL exporter writes directly to SQLite instead of to flat files.
    orphaned: BufWriter<File>,
    /// TXT exporter we delegate to for human-readable message bodies
    txt: TXT<'a>,
    /// Progress Bar model for alerting the user about current export state
    pb: ExportProgress,
}

impl<'a> SQL<'a> {
    /// Create all output tables, indexes, and views
    fn create_schema(conn: &Connection) -> Result<(), RuntimeError> {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS chats (
                id              INTEGER PRIMARY KEY,
                chat_identifier TEXT,
                service_name    TEXT,
                display_name    TEXT,
                derived_name    TEXT
            );

            CREATE TABLE IF NOT EXISTS handles (
                id           INTEGER PRIMARY KEY,
                contact_id   TEXT,
                display_name TEXT
            );

            CREATE TABLE IF NOT EXISTS chat_handles (
                chat_id   INTEGER NOT NULL,
                handle_id INTEGER NOT NULL,
                PRIMARY KEY (chat_id, handle_id)
            );

            CREATE TABLE IF NOT EXISTS messages (
                id                     INTEGER PRIMARY KEY,
                guid                   TEXT UNIQUE,
                chat_id                INTEGER,
                handle_id              INTEGER,
                sender                 TEXT,
                is_from_me             INTEGER NOT NULL,
                date_utc               TEXT,
                date_unix_ns           INTEGER,
                date_read_utc          TEXT,
                date_delivered_utc     TEXT,
                date_edited_utc        TEXT,
                is_reply               INTEGER NOT NULL,
                thread_originator_guid TEXT,
                is_edited              INTEGER NOT NULL,
                is_deleted             INTEGER NOT NULL,
                is_announcement        INTEGER NOT NULL,
                is_tapback             INTEGER NOT NULL,
                is_attachment_only     INTEGER NOT NULL,
                subject                TEXT,
                body                   TEXT,
                service                TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_messages_chat   ON messages(chat_id);
            CREATE INDEX IF NOT EXISTS idx_messages_handle ON messages(handle_id);
            CREATE INDEX IF NOT EXISTS idx_messages_date   ON messages(date_unix_ns);

            CREATE TABLE IF NOT EXISTS attachments (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id    INTEGER NOT NULL,
                filename      TEXT,
                mime_type     TEXT,
                transfer_name TEXT,
                FOREIGN KEY (message_id) REFERENCES messages(id)
            );

            CREATE TABLE IF NOT EXISTS tapbacks (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                target_guid TEXT NOT NULL,
                target_part INTEGER,
                action      TEXT NOT NULL,
                kind        TEXT NOT NULL,
                sender      TEXT,
                date_utc    TEXT
            );

            -- Convenience view that yields one row per non-tapback,
            -- non-announcement message, ordered chronologically.
            CREATE VIEW IF NOT EXISTS conversation_view AS
            SELECT
                m.id        AS message_id,
                m.date_utc  AS sent_at,
                COALESCE(c.derived_name, c.display_name, c.chat_identifier) AS chat,
                m.sender    AS sender,
                CASE WHEN m.is_from_me = 1 THEN 'me' ELSE 'them' END AS direction,
                m.body      AS body,
                m.is_edited AS is_edited,
                m.is_deleted AS is_deleted,
                m.guid      AS guid,
                m.chat_id   AS chat_id
            FROM messages m
            LEFT JOIN chats c ON c.id = m.chat_id
            WHERE m.is_tapback = 0 AND m.is_announcement = 0
            ORDER BY m.date_unix_ns ASC;
            ",
        )?;
        Ok(())
    }

    /// Insert all chats, handles, and chat<->handle links discovered at startup
    fn insert_chats(&mut self) -> Result<(), RuntimeError> {
        let tx = self.conn.transaction()?;
        {
            let mut chat_stmt = tx.prepare(
                "INSERT OR REPLACE INTO chats (id, chat_identifier, service_name, display_name, derived_name) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;

            for (chat_id, chatroom) in &self.config.chatrooms {
                // Build a human-readable name. Prefer the chat's own display name,
                // then a comma-separated list of participant names, finally falling
                // back to the raw chat identifier.
                let derived = if let Some(name) = chatroom.display_name() {
                    name.to_string()
                } else if let Some(participants) =
                    self.config.chatroom_participants.get(&chatroom.rowid)
                {
                    let mut names: Vec<&str> = participants
                        .iter()
                        .map(|h| self.config.who(Some(*h), false, &None))
                        .collect();
                    names.sort_unstable();
                    names.dedup();
                    names.join(", ")
                } else {
                    chatroom.chat_identifier.clone()
                };

                chat_stmt.execute(params![
                    chat_id,
                    chatroom.chat_identifier,
                    chatroom.service_name,
                    chatroom.display_name,
                    derived,
                ])?;
            }

            let mut handle_stmt = tx.prepare(
                "INSERT OR REPLACE INTO handles (id, contact_id, display_name) VALUES (?1, ?2, ?3)",
            )?;

            // We don't have raw handle addresses in `Config` directly, so we
            // store the resolved display name in both columns. The full
            // address remains available in the source `handle` table of
            // `chat.db`, which is not copied.
            for handle_id in self.config.real_participants.keys() {
                let display = self.config.who(Some(*handle_id), false, &None).to_string();
                handle_stmt.execute(params![handle_id, display, display])?;
            }

            let mut link_stmt = tx.prepare(
                "INSERT OR REPLACE INTO chat_handles (chat_id, handle_id) VALUES (?1, ?2)",
            )?;
            for (chat_id, handles) in &self.config.chatroom_participants {
                for handle_id in handles {
                    link_stmt.execute(params![chat_id, handle_id])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }
}

impl<'a> Exporter<'a> for SQL<'a> {
    fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        // Open / create the output database
        let mut db_path: PathBuf = config.options.export_path.clone();
        std::fs::create_dir_all(&db_path)?;
        db_path.push(OUTPUT_DB_NAME);

        // Start from a clean slate if a previous run left a database behind,
        // so users get a fresh export rather than rows accumulating across runs.
        if db_path.exists() {
            std::fs::remove_file(&db_path)?;
        }

        let conn = Connection::open(&db_path)?;
        Self::create_schema(&conn)?;

        // The `Exporter` trait's `get_or_create_file` returns a `BufWriter<File>`.
        // The SQL exporter does not produce per-conversation flat files, but we
        // still provide an "orphaned" file path for trait conformance, mirroring
        // the pattern used by the TXT exporter.
        let mut orphaned_path = config.options.export_path.clone();
        orphaned_path.push(ORPHANED);
        orphaned_path.set_extension("sql");
        let orphaned_file = File::options()
            .append(true)
            .create(true)
            .open(&orphaned_path)?;
        let orphaned = BufWriter::new(orphaned_file);

        // We delegate body formatting to the TXT exporter for intelligibility.
        let txt = TXT::new(config)?;

        Ok(SQL {
            config,
            conn,
            orphaned,
            txt,
            pb: ExportProgress::new(),
        })
    }

    fn iter_messages(&mut self) -> Result<(), RuntimeError> {
        // Tell the user what we are doing
        eprintln!(
            "Exporting to {}/{} as sql...",
            self.config.options.export_path.display(),
            OUTPUT_DB_NAME
        );

        // Insert chats / handles / links first so message FKs are valid
        self.insert_chats()?;

        // Keep track of current message ROWID
        let mut current_message_row = -1;

        // Set up progress bar
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

        let tx = self.conn.transaction()?;

        {
            let mut msg_stmt = tx.prepare(
                "INSERT OR REPLACE INTO messages (
                    id, guid, chat_id, handle_id, sender, is_from_me,
                    date_utc, date_unix_ns,
                    date_read_utc, date_delivered_utc, date_edited_utc,
                    is_reply, thread_originator_guid,
                    is_edited, is_deleted, is_announcement, is_tapback,
                    is_attachment_only, subject, body, service
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8,
                    ?9, ?10, ?11,
                    ?12, ?13,
                    ?14, ?15, ?16, ?17,
                    ?18, ?19, ?20, ?21
                )",
            )?;

            let mut att_stmt = tx.prepare(
                "INSERT INTO attachments (message_id, filename, mime_type, transfer_name) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?;

            let mut tap_stmt = tx.prepare(
                "INSERT INTO tapbacks (target_guid, target_part, action, kind, sender, date_utc) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;

            for message in messages {
                let mut msg = Message::extract(message)?;

                // Early escape if we try and render the same message GUID twice
                // See https://github.com/ReagentX/imessage-exporter/issues/135 for rationale
                if msg.rowid == current_message_row {
                    current_message += 1;
                    continue;
                }
                current_message_row = msg.rowid;

                // Parse and apply the message body
                if let Ok(body) = msg.parse_body(self.config.data_source.db()) {
                    msg.apply_body(body);
                }

                let is_tapback = msg.is_tapback();
                let is_announcement = msg.is_announcement();

                // Tapbacks go into their own table; do not store them as messages
                if is_tapback {
                    if let Variant::Tapback(_part, action, tapback) = msg.variant() {
                        let (target_part, target_guid) = match msg.clean_associated_guid() {
                            Some((part, guid)) => (Some(part as i64), Some(guid.to_string())),
                            None => (None, None),
                        };

                        if let Some(target_guid) = target_guid {
                            let action_str = match action {
                                TapbackAction::Added => "added",
                                TapbackAction::Removed => "removed",
                            };
                            let kind_str = match tapback {
                                Tapback::Loved => "loved".to_string(),
                                Tapback::Liked => "liked".to_string(),
                                Tapback::Disliked => "disliked".to_string(),
                                Tapback::Laughed => "laughed".to_string(),
                                Tapback::Emphasized => "emphasized".to_string(),
                                Tapback::Questioned => "questioned".to_string(),
                                Tapback::Emoji(e) => format!("emoji:{}", e.unwrap_or("?")),
                                Tapback::Sticker => "sticker".to_string(),
                            };
                            let sender = self
                                .config
                                .who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id)
                                .to_string();
                            let date_utc =
                                msg.date(self.config.offset).map(|d| format_date(&d)).ok();

                            tap_stmt.execute(params![
                                target_guid,
                                target_part,
                                action_str,
                                kind_str,
                                sender,
                                date_utc,
                            ])?;
                        }
                    }
                    current_message += 1;
                    if current_message % 99 == 0 {
                        self.pb.set_position(current_message);
                    }
                    continue;
                }

                // Poll votes and incremental poll updates are rendered in
                // context inside the main poll message and do not produce
                // standalone rows.
                if msg.is_poll_vote() || msg.is_poll_update() {
                    current_message += 1;
                    continue;
                }

                // Build the human-readable body by delegating to the TXT
                // formatter (intelligible plain text including app, URL,
                // handwriting, etc. message types).
                let formatted_body = if is_announcement {
                    self.txt.format_announcement(&msg)
                } else {
                    self.txt
                        .format_message(&msg, 0)
                        .unwrap_or_else(|_| msg.text.clone().unwrap_or_default())
                };

                // The TXT formatter prefixes each message with two header
                // lines (timestamp and sender). Those values live in their
                // own columns here, so strip them for a cleaner body.
                let body = strip_txt_header(&formatted_body, is_announcement);

                let chat_id_val = msg.chat_id.or(msg.deleted_from);
                let sender = self
                    .config
                    .who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id)
                    .to_string();

                let date_utc = msg.date(self.config.offset).map(|d| format_date(&d)).ok();
                let date_read_utc = msg
                    .date_read(self.config.offset)
                    .ok()
                    .map(|d| format_date(&d));
                let date_delivered_utc = msg
                    .date_delivered(self.config.offset)
                    .ok()
                    .map(|d| format_date(&d));
                let date_edited_utc = if msg.date_edited == 0 {
                    None
                } else {
                    msg.date_edited(self.config.offset)
                        .ok()
                        .map(|d| format_date(&d))
                };

                let is_edited = i64::from(msg.edited_parts.is_some());
                let is_deleted = i64::from(msg.is_deleted());
                let is_reply = i64::from(msg.is_reply());
                let is_attachment_only = i64::from(
                    msg.num_attachments > 0 && msg.text.as_deref().unwrap_or("").trim().is_empty(),
                );

                msg_stmt.execute(params![
                    msg.rowid,
                    msg.guid,
                    chat_id_val,
                    msg.handle_id,
                    sender,
                    i64::from(msg.is_from_me()),
                    date_utc,
                    msg.date,
                    date_read_utc,
                    date_delivered_utc,
                    date_edited_utc,
                    is_reply,
                    msg.thread_originator_guid,
                    is_edited,
                    is_deleted,
                    i64::from(is_announcement),
                    0_i64, // is_tapback (always 0 here; tapbacks live in their own table)
                    is_attachment_only,
                    msg.subject,
                    body,
                    msg.service,
                ])?;

                // Attachments
                if msg.num_attachments > 0
                    && let Ok(attachments) =
                        Attachment::from_message(self.config.data_source.db(), &msg)
                {
                    for att in attachments {
                        att_stmt.execute(params![
                            msg.rowid,
                            att.filename,
                            att.mime_type,
                            att.transfer_name,
                        ])?;
                    }
                }

                current_message += 1;
                if current_message % 99 == 0 {
                    self.pb.set_position(current_message);
                }
            }
        }

        tx.commit()?;

        self.pb.finish();
        eprintln!(
            "Wrote SQLite database to {}/{}",
            self.config.options.export_path.display(),
            OUTPUT_DB_NAME
        );
        Ok(())
    }

    fn get_or_create_file(
        &mut self,
        _message: &Message,
    ) -> Result<&mut BufWriter<File>, RuntimeError> {
        // Not used by the SQL exporter; we provide the sentinel file for trait conformance.
        Ok(&mut self.orphaned)
    }
}

// MARK: Helpers
/// Strip the two header lines (timestamp + sender) and the trailing blank line
/// that the TXT exporter prepends/appends to each formatted message.
///
/// The timestamp and sender are stored in dedicated columns on the `messages`
/// table, so the `body` column should contain only the actual content.
///
/// Announcement messages have no header lines and are returned with only their
/// trailing whitespace trimmed.
fn strip_txt_header(formatted: &str, is_announcement: bool) -> String {
    let trimmed = formatted.trim_end_matches('\n');
    if is_announcement {
        return trimmed.to_string();
    }
    let mut lines = trimmed.splitn(3, '\n');
    // Discard the timestamp line and the sender line
    let _ = lines.next();
    let _ = lines.next();
    lines.next().unwrap_or("").to_string()
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rusqlite::Connection;

    use crate::{
        Config, Exporter, Options, SQL,
        app::export_type::ExportType,
        exporters::sql::{OUTPUT_DB_NAME, strip_txt_header},
    };

    #[test]
    fn strips_txt_header_for_normal_message() {
        let formatted = "May 17, 2022  5:29:42 PM\nMe\nHello world\n\n";
        assert_eq!(strip_txt_header(formatted, false), "Hello world");
    }

    #[test]
    fn strips_txt_header_for_multiline_body() {
        let formatted = "May 17, 2022  5:29:42 PM\nMe\nLine one\nLine two\n\n";
        assert_eq!(strip_txt_header(formatted, false), "Line one\nLine two");
    }

    #[test]
    fn preserves_announcement_body() {
        let announcement = "May 17, 2022  5:29:42 PM: Me renamed the conversation to \"Trip\"\n\n";
        let stripped = strip_txt_header(announcement, true);
        assert_eq!(
            stripped,
            "May 17, 2022  5:29:42 PM: Me renamed the conversation to \"Trip\""
        );
    }

    #[test]
    fn handles_empty_body() {
        let formatted = "May 17, 2022  5:29:42 PM\nMe\n\n";
        assert_eq!(strip_txt_header(formatted, false), "");
    }

    #[test]
    fn can_create() {
        let options = Options::fake_options(ExportType::Sql);
        let config = Config::fake_app(options);
        let exporter = SQL::new(&config).unwrap();

        // Ensure the database file was created at the expected location
        let mut db_path = PathBuf::from(&exporter.config.options.export_path);
        db_path.push(OUTPUT_DB_NAME);
        assert!(db_path.exists());

        // Ensure the schema was created
        let conn = Connection::open(&db_path).unwrap();
        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
                 ('chats', 'handles', 'chat_handles', 'messages', 'attachments', 'tapbacks')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 6);

        let view_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'view' AND name = 'conversation_view'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(view_count, 1);
    }
}
