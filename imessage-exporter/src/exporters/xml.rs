/*!
 * XML export format for iMessage conversations.
 *
 * This module implements the `Exporter` trait to export iMessage data as structured XML.
 * The XML format provides a machine-readable alternative to HTML and a more structured
 * format compared to plain text.
 *
 * ## Features
 *
 * - **Structured output**: Well-formed XML with proper character escaping
 * - **Attachment references**: Includes file paths with MIME types and dimensions
 * - **Audio transcriptions**: Embeds transcription text with audio attachments
 * - **Message metadata**: Timestamps, sender information, edit indicators
 *
 * ## Example Output
 *
 * ```xml
 * <?xml version="1.0" encoding="UTF-8"?>
 * <conversation>
 *   <message>
 *     <timestamp>Jan 15, 2024 10:30:45 AM</timestamp>
 *     <from>John Doe</from>
 *     <text>Hello world!</text>
 *     <media type="image" src="attachments/IMG_001.jpg" mime="image/jpeg" width="1920" height="1080"/>
 *   </message>
 * </conversation>
 * ```
 */

use std::{
    collections::{
        HashMap,
        hash_map::Entry::{Occupied, Vacant},
    },
    fs::File,
    io::{BufWriter, Write},
};

use crate::{
    app::{
        error::RuntimeError,
        progress::ExportProgress,
        runtime::Config,
    },
    exporters::exporter::Exporter,
};

use imessage_database::{
    error::table::TableError,
    tables::{
        attachment::{Attachment, MediaType},
        messages::{Message, models::BubbleComponent},
        table::{ORPHANED, Table},
    },
    util::dates::format,
};

pub struct XML<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config,
    /// Handles to files we want to write messages to
    /// Map of resolved chatroom file location to a buffered writer
    pub files: HashMap<String, BufWriter<File>>,
    /// Writer instance for orphaned messages
    pub orphaned: BufWriter<File>,
    /// Progress Bar model for alerting the user about current export state
    pb: ExportProgress,
}

// MARK: Exporter
impl<'a> Exporter<'a> for XML<'a> {
    fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        let mut orphaned = config.options.export_path.clone();
        orphaned.push(ORPHANED);
        orphaned.set_extension("xml");

        let file = File::options().append(true).create(true).open(&orphaned)?;

        // Write XML header to orphaned file
        let mut orphaned_writer = BufWriter::new(file);
        writeln!(orphaned_writer, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>")
            .map_err(RuntimeError::DiskError)?;
        writeln!(orphaned_writer, "<conversation>")
            .map_err(RuntimeError::DiskError)?;

        Ok(XML {
            config,
            files: HashMap::new(),
            orphaned: orphaned_writer,
            pb: ExportProgress::new(),
        })
    }

    fn iter_messages(&mut self) -> Result<(), RuntimeError> {
        // Tell the user what we are doing
        eprintln!(
            "Exporting to {} as xml...",
            self.config.options.export_path.display()
        );

        // Keep track of current message ROWID
        let mut current_message_row = -1;

        // Set up progress bar
        let mut current_message = 0;
        let total_messages =
            Message::get_count(self.config.db(), &self.config.options.query_context)?;
        self.pb.start(total_messages);

        let mut statement =
            Message::stream_rows(self.config.db(), &self.config.options.query_context)?;

        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .map_err(|err| RuntimeError::DatabaseError(TableError::QueryError(err)))?;

        for message in messages {
            let mut msg = Message::extract(message)?;

            // Early escape if we try and render the same message GUID twice
            if msg.rowid == current_message_row {
                current_message += 1;
                continue;
            }
            current_message_row = msg.rowid;

            // Generate the text of the message
            let _ = msg.generate_text(self.config.db());

            // Format and write the message
            if !msg.is_tapback() && !msg.is_poll_vote() && !msg.is_poll_update() {
                let xml_message = self.format_message(&msg)?;
                XML::write_to_file(self.get_or_create_file(&msg)?, &xml_message)?;
            }

            current_message += 1;
            if current_message % 99 == 0 {
                self.pb.set_position(current_message);
            }
        }

        // Close all XML files
        self.close_files()?;
        self.pb.finish();
        Ok(())
    }

    fn get_or_create_file(
        &mut self,
        message: &Message,
    ) -> Result<&mut BufWriter<File>, RuntimeError> {
        match self.config.conversation(message) {
            Some((chatroom, _)) => {
                let filename = self.config.filename(chatroom);
                match self.files.entry(filename) {
                    Occupied(entry) => Ok(entry.into_mut()),
                    Vacant(entry) => {
                        let mut path = self.config.options.export_path.clone();
                        path.push(self.config.filename(chatroom));
                        path.set_extension("xml");

                        let file = File::options().append(true).create(true).open(&path)?;
                        let mut writer = BufWriter::new(file);

                        // Write XML header
                        writeln!(writer, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>")
                            .map_err(RuntimeError::DiskError)?;
                        writeln!(writer, "<conversation>")
                            .map_err(RuntimeError::DiskError)?;

                        Ok(entry.insert(writer))
                    }
                }
            }
            None => Ok(&mut self.orphaned),
        }
    }

    fn write_to_file(file: &mut BufWriter<File>, text: &str) -> Result<(), RuntimeError> {
        file.write_all(text.as_bytes())
            .map_err(RuntimeError::DiskError)
    }
}

impl<'a> XML<'a> {
    fn format_message(&self, msg: &Message) -> Result<String, RuntimeError> {
        let mut xml = String::with_capacity(512);

        xml.push_str("  <message>\n");

        // Timestamp
        if let Ok(date) = msg.date(&self.config.offset) {
            let formatted_date = format(&Ok(date));
            xml.push_str(&format!("    <timestamp>{}</timestamp>\n", Self::escape_xml(&formatted_date)));
        }

        // Sender - map phone numbers to names if available
        let sender = self.get_sender_name(msg);
        xml.push_str(&format!("    <from>{}</from>\n", Self::escape_xml(&sender)));

        // Message text
        if let Some(text) = &msg.text
            && !text.is_empty() {
            xml.push_str(&format!("    <text>{}</text>\n", Self::escape_xml(text)));
        }

        // Get attachments for this message
        let mut attachments = Attachment::from_message(self.config.db(), msg)
            .map_err(RuntimeError::DatabaseError)?;
        let mut attachment_index: usize = 0;

        // Handle attachments with file references
        for component in &msg.components {
            if let BubbleComponent::Attachment(metadata) = component {
                match attachments.get_mut(attachment_index) {
                    Some(attachment) => {
                        // Copy the file if attachment manager is enabled
                        let _ = self.config
                            .options
                            .attachment_manager
                            .handle_attachment(msg, attachment, self.config);

                        // Get the file path (relative if copied, absolute otherwise)
                        let path = self.config.message_attachment_path(attachment);

                        // Determine media type
                        let media_type = match attachment.mime_type() {
                            MediaType::Image(_) => "image",
                            MediaType::Video(_) => "video",
                            MediaType::Audio(_) => "audio",
                            _ => "other",
                        };

                        // Get MIME type
                        let mime = attachment.mime_type().as_mime_type();

                        // Build XML with metadata
                        if let Some(transcription) = &metadata.transcription {
                            xml.push_str(&format!(
                                "    <media type=\"{}\" src=\"{}\" mime=\"{}\"",
                                media_type,
                                Self::escape_xml(&path),
                                Self::escape_xml(&mime)
                            ));

                            // Add dimensions if available
                            if let Some(w) = metadata.width {
                                xml.push_str(&format!(" width=\"{}\"", w));
                            }
                            if let Some(h) = metadata.height {
                                xml.push_str(&format!(" height=\"{}\"", h));
                            }

                            xml.push_str(">\n");
                            xml.push_str(&format!(
                                "      <transcription>{}</transcription>\n",
                                Self::escape_xml(transcription)
                            ));
                            xml.push_str("    </media>\n");
                        } else {
                            xml.push_str(&format!(
                                "    <media type=\"{}\" src=\"{}\" mime=\"{}\"",
                                media_type,
                                Self::escape_xml(&path),
                                Self::escape_xml(&mime)
                            ));

                            // Add dimensions if available
                            if let Some(w) = metadata.width {
                                xml.push_str(&format!(" width=\"{}\"", w));
                            }
                            if let Some(h) = metadata.height {
                                xml.push_str(&format!(" height=\"{}\"", h));
                            }

                            xml.push_str("/>\n");
                        }

                        attachment_index += 1;
                    }
                    None => {
                        // Attachment missing from database
                        xml.push_str("    <media type=\"missing\"/>\n");
                    }
                }
            }
        }

        // Read receipts (if available)
        if msg.date_read > 0 {
            xml.push_str("    <read_receipt>Read by recipient</read_receipt>\n");
        }

        // Edited message indicator (simplified for now)
        if msg.is_edited() {
            xml.push_str("    <edited>true</edited>\n");
        }

        xml.push_str("  </message>\n");

        Ok(xml)
    }

    fn get_sender_name(&self, msg: &Message) -> String {
        self.config.who(
            msg.handle_id,
            msg.is_from_me(),
            &msg.destination_caller_id,
        ).to_string()
    }

    fn escape_xml(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    fn close_files(&mut self) -> Result<(), RuntimeError> {
        // Close orphaned file
        writeln!(self.orphaned, "</conversation>").map_err(RuntimeError::DiskError)?;
        self.orphaned.flush().map_err(RuntimeError::DiskError)?;

        // Close all conversation files
        for (_, mut file) in self.files.drain() {
            writeln!(file, "</conversation>").map_err(RuntimeError::DiskError)?;
            file.flush().map_err(RuntimeError::DiskError)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Exporter, Options, app::export_type::ExportType};

    #[test]
    fn can_create() {
        let options = Options::fake_options(ExportType::Xml);
        let config = Config::fake_app(options);
        let exporter = XML::new(&config);
        assert!(exporter.is_ok());
        assert_eq!(exporter.unwrap().files.len(), 0);
    }

    #[test]
    fn can_escape_xml() {
        assert_eq!(
            XML::escape_xml("Hello & <world>"),
            "Hello &amp; &lt;world&gt;"
        );
        assert_eq!(
            XML::escape_xml("\"quotes\" and 'apostrophes'"),
            "&quot;quotes&quot; and &apos;apostrophes&apos;"
        );
        assert_eq!(
            XML::escape_xml("Test <tag> & \"value\""),
            "Test &lt;tag&gt; &amp; &quot;value&quot;"
        );
    }

    #[test]
    fn can_escape_empty_string() {
        assert_eq!(XML::escape_xml(""), "");
    }

    #[test]
    fn can_escape_no_special_chars() {
        assert_eq!(XML::escape_xml("Hello World"), "Hello World");
    }
}
