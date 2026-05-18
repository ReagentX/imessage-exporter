/*!
SQL exporter.

Reads the iMessage `chat.db` and produces a new, intelligible SQLite database
containing the conversations in a structured, easy-to-query format. Message
bodies (including app messages, edits, expressives, etc.) are formatted using
the same logic as the TXT exporter so that the `body` column is human readable
and downstream-friendly.

Output schema (in `<export_path>/messages.db`):

- `chats(id, chat_identifier, service_name, display_name, derived_name)`
- `handles(id, contact_id, display_name)`
- `chat_handles(chat_id, handle_id)`
- `messages(id, guid, chat_id, handle_id, sender, is_from_me, date_utc, date_unix_ns,
            date_read_utc, date_delivered_utc, date_edited_utc,
            is_reply, thread_originator_guid, is_edited, is_deleted,
            is_announcement, is_tapback, is_attachment_only,
            subject, body, service)`
- `attachments(id, message_id, filename, mime_type, transfer_name)`
- `tapbacks(id, target_guid, target_part, action, kind, sender, date_utc)`
*/

use std::{
    fs::File,
    io::BufWriter,
    path::PathBuf,
};

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

const OUTPUT_DB_NAME: &str = "messages.db";

/// The TXT exporter prepends two header lines to every message (timestamp and sender)
/// and appends a blank line. We strip those decorations so the `body` column contains
/// only the actual message content (timestamps and sender are stored in their own
/// columns).
fn strip_txt_header(formatted: &str, is_announcement: bool) -> String {
    let trimmed = formatted.trim_end_matches('\n');
    if is_announcement {
        return trimmed.to_string();
    }
    let mut lines = trimmed.splitn(3, '\n');
    let _ts = lines.next();
    let _who = lines.next();
    lines.next().unwrap_or("").to_string()
}

pub struct SQL<'a> {
    pub config: &'a Config,
    /// SQLite connection for the output database
    conn: Connection,
    /// Sentinel file required by the `Exporter` trait but unused by this exporter
    _sink: BufWriter<File>,
    /// TXT formatter we delegate to for human-readable message bodies
    txt: TXT<'a>,
    /// Progress bar
    pb: ExportProgress,
}

impl<'a> SQL<'a> {
    fn create_schema(conn: &Connection) -> Result<(), RuntimeError> {
        conn.execute_batch(
            r#"
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
                id                    INTEGER PRIMARY KEY,
                guid                  TEXT UNIQUE,
                chat_id               INTEGER,
                handle_id             INTEGER,
                sender                TEXT,
                is_from_me            INTEGER NOT NULL,
                date_utc              TEXT,
                date_unix_ns          INTEGER,
                date_read_utc         TEXT,
                date_delivered_utc    TEXT,
                date_edited_utc       TEXT,
                is_reply              INTEGER NOT NULL,
                thread_originator_guid TEXT,
                is_edited             INTEGER NOT NULL,
                is_deleted            INTEGER NOT NULL,
                is_announcement       INTEGER NOT NULL,
                is_tapback            INTEGER NOT NULL,
                is_attachment_only    INTEGER NOT NULL,
                subject               TEXT,
                body                  TEXT,
                service               TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_messages_chat   ON messages(chat_id);
            CREATE INDEX IF NOT EXISTS idx_messages_handle ON messages(handle_id);
            CREATE INDEX IF NOT EXISTS idx_messages_date   ON messages(date_unix_ns);

            CREATE TABLE IF NOT EXISTS attachments (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id     INTEGER NOT NULL,
                filename       TEXT,
                mime_type      TEXT,
                transfer_name  TEXT,
                FOREIGN KEY (message_id) REFERENCES messages(id)
            );

            CREATE TABLE IF NOT EXISTS tapbacks (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                target_guid  TEXT NOT NULL,
                target_part  INTEGER,
                action       TEXT NOT NULL,
                kind         TEXT NOT NULL,
                sender       TEXT,
                date_utc     TEXT
            );

            -- Convenience view to read conversations easily.
            -- One row per real (non-tapback, non-announcement) message.
            CREATE VIEW IF NOT EXISTS conversation_view AS
            SELECT
                m.id                AS message_id,
                m.date_utc          AS sent_at,
                COALESCE(c.derived_name, c.display_name, c.chat_identifier) AS chat,
                m.sender            AS sender,
                CASE WHEN m.is_from_me = 1 THEN 'me' ELSE 'them' END AS direction,
                m.body              AS body,
                m.is_edited         AS is_edited,
                m.is_deleted        AS is_deleted,
                m.guid              AS guid,
                m.chat_id           AS chat_id
            FROM messages m
            LEFT JOIN chats c ON c.id = m.chat_id
            WHERE m.is_tapback = 0 AND m.is_announcement = 0
            ORDER BY m.date_unix_ns ASC;
            "#,
        )
        .map_err(|e| RuntimeError::InvalidOptions(format!("Failed to create schema: {e}")))?;
        Ok(())
    }

    fn insert_chats(&mut self) -> Result<(), RuntimeError> {
        let tx = self
            .conn
            .transaction()
            .map_err(|e| RuntimeError::InvalidOptions(format!("Tx error: {e}")))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO chats (id, chat_identifier, service_name, display_name, derived_name) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(|e| RuntimeError::InvalidOptions(format!("Prepare error: {e}")))?;

            for (chat_id, chatroom) in &self.config.chatrooms {
                // Build a derived (intelligible) name using the same logic as filename(),
                // but without an extension.
                let derived = if let Some(name) = chatroom.display_name() {
                    name.to_string()
                } else if let Some(participants) =
                    self.config.chatroom_participants.get(&chatroom.rowid)
                {
                    let mut names: Vec<&str> = participants
                        .iter()
                        .map(|h| self.config.who(Some(*h), false, &None))
                        .collect();
                    names.sort();
                    names.dedup();
                    names.join(", ")
                } else {
                    chatroom.chat_identifier.clone()
                };

                stmt.execute(params![
                    chat_id,
                    chatroom.chat_identifier,
                    chatroom.service_name,
                    chatroom.display_name,
                    derived,
                ])
                .map_err(|e| RuntimeError::InvalidOptions(format!("Insert chat: {e}")))?;
            }

            // Insert handles
            let mut handle_stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO handles (id, contact_id, display_name) VALUES (?1, ?2, ?3)",
                )
                .map_err(|e| RuntimeError::InvalidOptions(format!("Prepare handles: {e}")))?;

            for (handle_id, _) in &self.config.real_participants {
                let display = self.config.who(Some(*handle_id), false, &None).to_string();
                // contact_id is the raw handle address; we don't have direct access here, so
                // reuse display name as a fallback. The full address remains in the original
                // `handle` table of chat.db (which we don't copy).
                handle_stmt
                    .execute(params![handle_id, display, display])
                    .map_err(|e| RuntimeError::InvalidOptions(format!("Insert handle: {e}")))?;
            }

            // Insert chat<->handle links
            let mut link_stmt = tx
                .prepare("INSERT OR REPLACE INTO chat_handles (chat_id, handle_id) VALUES (?1, ?2)")
                .map_err(|e| RuntimeError::InvalidOptions(format!("Prepare links: {e}")))?;
            for (chat_id, handles) in &self.config.chatroom_participants {
                for h in handles {
                    link_stmt
                        .execute(params![chat_id, h])
                        .map_err(|e| RuntimeError::InvalidOptions(format!("Insert link: {e}")))?;
                }
            }
        }
        tx.commit()
            .map_err(|e| RuntimeError::InvalidOptions(format!("Commit chats: {e}")))?;
        Ok(())
    }
}

impl<'a> Exporter<'a> for SQL<'a> {
    fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        // Open / create output db
        let mut db_path: PathBuf = config.options.export_path.clone();
        std::fs::create_dir_all(&db_path)?;
        db_path.push(OUTPUT_DB_NAME);

        // If the file exists from a previous run, remove it so we always
        // produce a clean export.
        if db_path.exists() {
            std::fs::remove_file(&db_path).ok();
        }

        let conn = Connection::open(&db_path).map_err(|e| {
            RuntimeError::InvalidOptions(format!("Failed to open output db {}: {e}", db_path.display()))
        })?;
        Self::create_schema(&conn)?;

        // Trait requires a BufWriter<File>; create a tiny sentinel file we never write to.
        let mut sink_path = config.options.export_path.clone();
        sink_path.push(format!("{ORPHANED}.sql.unused"));
        let sink_file = File::options()
            .append(true)
            .create(true)
            .open(&sink_path)?;
        let sink = BufWriter::new(sink_file);

        // We delegate body formatting to the TXT exporter for intelligibility.
        let txt = TXT::new(config)?;

        Ok(SQL {
            config,
            conn,
            _sink: sink,
            txt,
            pb: ExportProgress::new(),
        })
    }

    fn iter_messages(&mut self) -> Result<(), RuntimeError> {
        eprintln!(
            "Exporting to SQLite database at {}/{}...",
            self.config.options.export_path.display(),
            OUTPUT_DB_NAME
        );

        // Insert chats / handles / links first
        self.insert_chats()?;

        // Progress bar
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

        let tx = self
            .conn
            .transaction()
            .map_err(|e| RuntimeError::InvalidOptions(format!("Tx error: {e}")))?;

        {
            let mut msg_stmt = tx
                .prepare(
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
                )
                .map_err(|e| RuntimeError::InvalidOptions(format!("Prepare msg: {e}")))?;

            let mut att_stmt = tx
                .prepare(
                    "INSERT INTO attachments (message_id, filename, mime_type, transfer_name) \
                     VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(|e| RuntimeError::InvalidOptions(format!("Prepare att: {e}")))?;

            let mut tap_stmt = tx
                .prepare(
                    "INSERT INTO tapbacks (target_guid, target_part, action, kind, sender, date_utc) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|e| RuntimeError::InvalidOptions(format!("Prepare tap: {e}")))?;

            let mut current_message = 0i64;
            let mut current_message_row = -1i32;

            for message in messages {
                let mut msg = Message::extract(message)?;

                // De-dupe by rowid (same logic as TXT exporter)
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
                let is_poll_vote = msg.is_poll_vote();
                let is_poll_update = msg.is_poll_update();

                // Tapbacks go into their own table; skip from messages
                if is_tapback {
                    if let Variant::Tapback(_part, action, tapback) = msg.variant() {
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
                        let date_utc = msg
                            .date(self.config.offset)
                            .map(|d| format_date(&d))
                            .ok();
                        // associated_message_guid encodes the targeted message, sometimes prefixed
                        // with `p:<part>/`. We extract part if present.
                        let (target_guid, target_part) =
                            match msg.associated_message_guid.as_deref() {
                                Some(s) => {
                                    if let Some(rest) = s.strip_prefix("p:") {
                                        if let Some((part_str, guid)) = rest.split_once('/') {
                                            (
                                                Some(guid.to_string()),
                                                part_str.parse::<i64>().ok(),
                                            )
                                        } else {
                                            (Some(rest.to_string()), None)
                                        }
                                    } else if let Some(rest) = s.strip_prefix("bp:") {
                                        (Some(rest.to_string()), None)
                                    } else {
                                        (Some(s.to_string()), None)
                                    }
                                }
                                None => (None, None),
                            };
                        if let Some(tg) = target_guid {
                            tap_stmt
                                .execute(params![
                                    tg, target_part, action_str, kind_str, sender, date_utc
                                ])
                                .map_err(|e| {
                                    RuntimeError::InvalidOptions(format!("Insert tapback: {e}"))
                                })?;
                        }
                    }
                    current_message += 1;
                    if current_message % 99 == 0 {
                        self.pb.set_position(current_message as u64);
                    }
                    continue;
                }

                if is_poll_vote || is_poll_update {
                    current_message += 1;
                    continue;
                }

                // Build the body using the TXT formatter (intelligible plain text including
                // app/url/handwriting/etc. message types).
                let body = if is_announcement {
                    self.txt.format_announcement(&msg)
                } else {
                    self.txt
                        .format_message(&msg, 0)
                        .unwrap_or_else(|_| msg.text.clone().unwrap_or_default())
                };
                // The TXT formatter prefixes each message with two header lines
                // (timestamp and sender). Those values live in their own columns
                // here, so strip them from the body for cleaner output.
                let body = strip_txt_header(&body, is_announcement);

                let chat_id_val = msg.chat_id.or(msg.deleted_from);
                let sender = self
                    .config
                    .who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id)
                    .to_string();

                let date_utc = msg
                    .date(self.config.offset)
                    .map(|d| format_date(&d))
                    .ok();
                let date_read_utc = msg
                    .date_read(self.config.offset)
                    .ok()
                    .map(|d| format_date(&d));
                let date_delivered_utc = msg
                    .date_delivered(self.config.offset)
                    .ok()
                    .map(|d| format_date(&d));
                let date_edited_utc = if msg.date_edited != 0 {
                    msg.date_edited(self.config.offset).ok().map(|d| format_date(&d))
                } else {
                    None
                };

                let is_edited = msg.edited_parts.is_some() as i64;
                let is_deleted = msg.is_deleted() as i64;
                let is_reply = msg.is_reply() as i64;
                let is_attachment_only = (msg.num_attachments > 0
                    && msg.text.as_deref().unwrap_or("").trim().is_empty())
                    as i64;

                msg_stmt
                    .execute(params![
                        msg.rowid,
                        msg.guid,
                        chat_id_val,
                        msg.handle_id,
                        sender,
                        msg.is_from_me() as i64,
                        date_utc,
                        msg.date,
                        date_read_utc,
                        date_delivered_utc,
                        date_edited_utc,
                        is_reply,
                        msg.thread_originator_guid,
                        is_edited,
                        is_deleted,
                        is_announcement as i64,
                        0i64, // is_tapback (always 0 here; tapbacks go to their own table)
                        is_attachment_only,
                        msg.subject,
                        body,
                        msg.service,
                    ])
                    .map_err(|e| RuntimeError::InvalidOptions(format!("Insert msg: {e}")))?;

                // Attachments
                if msg.num_attachments > 0 {
                    if let Ok(attachments) =
                        Attachment::from_message(self.config.data_source.db(), &msg)
                    {
                        for att in attachments {
                            att_stmt
                                .execute(params![
                                    msg.rowid,
                                    att.filename,
                                    att.mime_type,
                                    att.transfer_name,
                                ])
                                .map_err(|e| {
                                    RuntimeError::InvalidOptions(format!("Insert att: {e}"))
                                })?;
                        }
                    }
                }

                current_message += 1;
                if current_message % 99 == 0 {
                    self.pb.set_position(current_message as u64);
                }
            }
        }

        tx.commit()
            .map_err(|e| RuntimeError::InvalidOptions(format!("Commit msgs: {e}")))?;

        // Drop the prepared statements before committing
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
        Ok(&mut self._sink)
    }
}
