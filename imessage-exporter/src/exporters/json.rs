use std::{collections::HashMap, fs::File, io::BufWriter};

use serde::Serialize;

use crate::app::error::RuntimeError;
use crate::app::runtime::Config;
use crate::exporters::exporter::Exporter;

use imessage_database::tables::table::Table;
use imessage_database::tables::table::Cacheable;
use imessage_database::tables::{attachment::Attachment, handle::Handle, messages::Message, table::ME};
use imessage_database::util::dates::get_local_time;
use std::io::Write;
#[derive(Serialize)]
struct AttachmentDto {
    filename: Option<String>,
    mime_type: Option<String>,
    size: i64,
    path: String,
}

#[derive(Serialize)]
struct ParticipantDto {
    handle_id: Option<i32>,
    handle: Option<String>,
    display_name: Option<String>,
    is_me: bool,
}

#[derive(Serialize)]
struct MessageDto {
    rowid: i32,
    guid: String,
    date_iso: String,
    is_from_me: bool,
    sender: ParticipantDto,
    recipients: Vec<ParticipantDto>,
    text: Option<String>,
    chat_id: Option<i32>,
    attachments: Vec<AttachmentDto>,
}

pub struct JSON<'a> {
    pub config: &'a Config,
    pub files: HashMap<String, BufWriter<File>>,
    pub orphaned: BufWriter<File>,
}

impl<'a> Exporter<'a> for JSON<'a> {
    fn new(config: &'a Config) -> Result<Self, RuntimeError>
    where
        Self: Sized,
    {
        let mut orphaned = config.options.export_path.clone();
        orphaned.push("orphaned");
        orphaned.set_extension("ndjson");

        let file = File::options().append(true).create(true).open(&orphaned)?;

        Ok(JSON {
            config,
            files: HashMap::new(),
            orphaned: BufWriter::new(file),
        })
    }

    fn iter_messages(&mut self) -> Result<(), RuntimeError> {
        eprintln!(
            "Exporting to {} as json (ndjson)...",
            self.config.options.export_path.display()
        );

        let mut statement = imessage_database::tables::messages::Message::stream_rows(
            self.config.data_source.db(),
            &self.config.options.query_context,
        )?;

        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .map_err(|err| {
                RuntimeError::DatabaseError(
                    imessage_database::error::table::TableError::QueryError(err),
                )
            })?;

        // Cache handles map once for lookups
        let handle_map = Handle::cache(self.config.data_source.db()).unwrap_or_default();

        for message in messages {
            let mut msg = Message::extract(message)?;
            // Avoid duplicate rowids
            // Generate textual content
            let _ = msg.generate_text(self.config.data_source.db());

            // Build sender participant
            let sender = if msg.is_from_me {
                ParticipantDto {
                    handle_id: None,
                    handle: msg.destination_caller_id.clone(),
                    display_name: Some(self.config.who(msg.handle_id, msg.is_from_me, &msg.destination_caller_id).to_string()),
                    is_me: true,
                }
            } else if let Some(hid) = msg.handle_id {
                let handle_str = handle_map.get(&hid).cloned();
                let display = self.config.real_participants.get(&hid)
                    .and_then(|internal| self.config.participants.get(internal).map(|n| n.get_display_name().to_string()))
                    .or(handle_str.clone());
                ParticipantDto {
                    handle_id: Some(hid),
                    handle: handle_str,
                    display_name: display,
                    is_me: false,
                }
            } else {
                ParticipantDto {
                    handle_id: None,
                    handle: None,
                    display_name: None,
                    is_me: false,
                }
            };

            // Build recipients list
            let mut recipients: Vec<ParticipantDto> = vec![];
            if let Some(chat_id) = msg.chat_id {
                if let Some(handles) = self.config.chatroom_participants.get(&chat_id) {
                    for hid in handles {
                        let handle_str = handle_map.get(hid).cloned();
                        let display = self.config.real_participants.get(hid)
                            .and_then(|internal| self.config.participants.get(internal).map(|n| n.get_display_name().to_string()))
                            .or(handle_str.clone());
                        let is_me = match handle_str.as_deref() {
                            Some(s) => s == ME,
                            None => false,
                        };
                        recipients.push(ParticipantDto {
                            handle_id: Some(*hid),
                            handle: handle_str,
                            display_name: display,
                            is_me,
                        });
                    }
                }
            } else if msg.is_from_me {
                // If no chat_id and this message is from me, include the destination as recipient
                if let Some(dest) = &msg.destination_caller_id {
                    recipients.push(ParticipantDto {
                        handle_id: None,
                        handle: Some(dest.clone()),
                        display_name: None,
                        is_me: false,
                    });
                }
            }

            // Build DTO
            let dt = MessageDto {
                rowid: msg.rowid,
                guid: msg.guid.clone(),
                date_iso: get_local_time(&msg.date, &self.config.offset)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default(),
                is_from_me: msg.is_from_me,
                sender,
                recipients,
                text: msg.text.clone(),
                chat_id: msg.chat_id,
                attachments: {
                    let mut atts = vec![];
                    if msg.num_attachments > 0 {
                        if let Ok(att_list) =
                            Attachment::from_message(self.config.data_source.db(), &msg)
                        {
                            for a in att_list {
                                let path = self.config.message_attachment_path(&a);
                                atts.push(AttachmentDto {
                                    filename: a.filename().map(|s| s.to_string()),
                                    mime_type: a.mime_type.clone(),
                                    size: a.total_bytes,
                                    path,
                                });
                            }
                        }
                    }
                    atts
                },
            };

            let file = self.get_or_create_file(&msg)?;
            serde_json::to_writer(&mut *file, &dt).map_err(|e| RuntimeError::InvalidOptions(format!("JSON serialization error: {e}")))?;
            // Write newline to keep ndjson semantics
            file.write_all(b"\n").map_err(RuntimeError::from)?;
        }

        Ok(())
    }

    fn get_or_create_file(
        &mut self,
        message: &Message,
    ) -> Result<&mut BufWriter<File>, RuntimeError> {
        match self.config.conversation(message) {
            Some((chatroom, _)) => {
                let filename = self.config.filename(chatroom);
                use std::collections::hash_map::Entry::{Occupied, Vacant};
                match self.files.entry(filename) {
                    Occupied(entry) => Ok(entry.into_mut()),
                    Vacant(entry) => {
                        let mut path = self.config.options.export_path.clone();
                        path.push(self.config.filename(chatroom));
                        path.set_extension("ndjson");

                        let file = File::options().append(true).create(true).open(&path)?;
                        let inserted = entry.insert(BufWriter::new(file));
                        Ok(inserted)
                    }
                }
            }
            None => Ok(&mut self.orphaned),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};


    #[test]
    fn ndjson_export_creates_parsable_output() {
        // Create a unique temporary export directory
        let mut export_dir = env::temp_dir();
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        export_dir.push(format!("imessage_export_test_{ts}"));
        fs::create_dir_all(&export_dir).unwrap();

        // Build options and point export_path at the temp dir
        let mut options = crate::app::options::Options::fake_options(crate::app::export_type::ExportType::Json);
        options.export_path = export_dir.clone();

        // Initialize the app config (reads test DB)
        let config = crate::app::runtime::Config::new(options).unwrap();

        // Run the JSON exporter
        let mut exporter = JSON::new(&config).unwrap();
        exporter.iter_messages().unwrap();

        // Find any ndjson file in the export dir and verify it contains at least one parsable JSON line
        let mut found_line: Option<String> = None;
        for entry in fs::read_dir(&export_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) == Some("ndjson") {
                let s = fs::read_to_string(&path).unwrap();
                if let Some(line) = s.lines().find(|l| !l.trim().is_empty()) {
                    found_line = Some(line.to_string());
                    break;
                }
            }
        }
        let first_nonempty = found_line.expect("No JSON lines found in any ndjson file");
        let v: serde_json::Value = serde_json::from_str(&first_nonempty).unwrap();
        assert!(v.get("rowid").is_some(), "Serialized object missing rowid");
    }

    #[test]
    fn sender_is_me_matches_participant() {
        // Create a unique temporary export directory
        let mut export_dir = env::temp_dir();
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        export_dir.push(format!("imessage_export_test_{}", ts));
        fs::create_dir_all(&export_dir).unwrap();

        // Build options and point export_path at the temp dir
        let mut options = crate::app::options::Options::fake_options(crate::app::export_type::ExportType::Json);
        options.export_path = export_dir.clone();

        // Initialize the app config (reads test DB)
        let config = crate::app::runtime::Config::new(options).unwrap();

        // Run the JSON exporter
        let mut exporter = JSON::new(&config).unwrap();
        exporter.iter_messages().unwrap();

        // Read first JSON line across any ndjson files in the export dir
        let mut first_nonempty_opt: Option<String> = None;
        for entry in fs::read_dir(&export_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) == Some("ndjson") {
                let s = fs::read_to_string(&path).unwrap();
                if let Some(line) = s.lines().find(|l| !l.trim().is_empty()) {
                    first_nonempty_opt = Some(line.to_string());
                    break;
                }
            }
        }
        let first_nonempty = first_nonempty_opt.expect("No JSON lines found in any ndjson file");
        let v: serde_json::Value = serde_json::from_str(&first_nonempty).unwrap();

        let is_from_me = v.get("is_from_me").and_then(|b| b.as_bool()).unwrap_or(false);
        let sender_is_me = v.get("sender").and_then(|s| s.get("is_me")).and_then(|b| b.as_bool()).unwrap_or(false);
        assert_eq!(is_from_me, sender_is_me, "Invariant failed: message.is_from_me != sender.is_me");
    }
}
