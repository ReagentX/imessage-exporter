/*!
 This module represents common (but not all) columns in the `attachment` table.
*/

use rusqlite::{Connection, Error, Result, Row, Statement};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};

use crate::{
    error::table::TableError,
    tables::{
        messages::Message,
        table::{Table, ATTACHMENT},
    },
    util::{
        dirs::home,
        output::{done_processing, processing},
        platform::Platform,
    },
};

const DIVISOR: f64 = 1024.;
const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

/// Represents the MIME type of a message's attachment data
///
/// The interior `str` contains the subtype, i.e. `x-m4a` for `audio/x-m4a`
#[derive(Debug, PartialEq, Eq)]
pub enum MediaType<'a> {
    Image(&'a str),
    Video(&'a str),
    Audio(&'a str),
    Text(&'a str),
    Application(&'a str),
    Other(&'a str),
    Unknown,
}

/// Represents a single row in the `attachment` table.
#[derive(Debug)]
pub struct Attachment {
    pub rowid: i32,
    pub filename: Option<String>,
    pub uti: Option<String>,
    pub mime_type: Option<String>,
    pub transfer_name: Option<String>,
    pub total_bytes: i64,
    pub hide_attachment: i32,
    pub copied_path: Option<PathBuf>,
}

impl Table for Attachment {
    fn from_row(row: &Row) -> Result<Attachment> {
        Ok(Attachment {
            rowid: row.get("rowid")?,
            filename: row.get("filename").unwrap_or(None),
            uti: row.get("uti").unwrap_or(None),
            mime_type: row.get("mime_type").unwrap_or(None),
            transfer_name: row.get("transfer_name").unwrap_or(None),
            total_bytes: row.get("total_bytes").unwrap_or_default(),
            hide_attachment: row.get("hide_attachment").unwrap_or(0),
            copied_path: None,
        })
    }

    fn get(db: &Connection) -> Result<Statement, TableError> {
        db.prepare(&format!("SELECT * from {}", ATTACHMENT))
            .map_err(TableError::Attachment)
    }

    fn extract(attachment: Result<Result<Self, Error>, Error>) -> Result<Self, TableError> {
        match attachment {
            Ok(Ok(attachment)) => Ok(attachment),
            Err(why) | Ok(Err(why)) => Err(TableError::Attachment(why)),
        }
    }
}

impl Attachment {
    /// Gets a Vector of attachments for a single message
    pub fn from_message(db: &Connection, msg: &Message) -> Result<Vec<Attachment>, TableError> {
        let mut out_l = vec![];
        if msg.has_attachments() {
            let mut statement = db
                .prepare(&format!(
                    "
                    SELECT * FROM message_attachment_join j 
                        LEFT JOIN attachment AS a ON j.attachment_id = a.ROWID
                    WHERE j.message_id = {}
                    ",
                    msg.rowid
                ))
                .map_err(TableError::Attachment)?;

            let iter = statement
                .query_map([], |row| Ok(Attachment::from_row(row)))
                .map_err(TableError::Attachment)?;

            for attachment in iter {
                let m = Attachment::extract(attachment)?;
                out_l.push(m)
            }
        }
        Ok(out_l)
    }

    /// Get the media type of an attachment
    pub fn mime_type(&'_ self) -> MediaType<'_> {
        match &self.mime_type {
            Some(mime) => {
                if let Some(mime_str) = mime.split('/').next() {
                    match mime_str {
                        "image" => MediaType::Image(mime),
                        "video" => MediaType::Video(mime),
                        "audio" => MediaType::Audio(mime),
                        "text" => MediaType::Text(mime),
                        "application" => MediaType::Application(mime),
                        _ => MediaType::Other(mime),
                    }
                } else {
                    MediaType::Other(mime)
                }
            }
            None => {
                // Fallback to `uti` if the MIME type cannot be inferred
                if let Some(uti) = &self.uti {
                    match uti.as_str() {
                        // This type is for audio messages, which are sent in `caf` format
                        // https://developer.apple.com/library/archive/documentation/MusicAudio/Reference/CAFSpec/CAF_overview/CAF_overview.html
                        "com.apple.coreaudio-format" => MediaType::Audio("x-caf; codecs=opus"),
                        _ => MediaType::Unknown,
                    }
                } else {
                    MediaType::Unknown
                }
            }
        }
    }

    /// Get the path to an attachment, if it exists
    pub fn path(&self) -> Option<&Path> {
        match &self.filename {
            Some(name) => Some(Path::new(name)),
            None => None,
        }
    }

    /// Get the extension of an attachment, if it exists
    pub fn extension(&self) -> Option<&str> {
        match self.path() {
            Some(path) => match path.extension() {
                Some(ext) => ext.to_str(),
                None => None,
            },
            None => None,
        }
    }

    /// Get a reasonable filename for an attachment
    pub fn filename(&self) -> &str {
        if let Some(transfer_name) = &self.transfer_name {
            return transfer_name;
        }
        if let Some(filename) = &self.filename {
            return filename;
        }
        "Attachment missing name metadata!"
    }

    /// Get a human readable file size for an attachment
    pub fn file_size(&self) -> String {
        Attachment::format_file_size(self.total_bytes)
    }

    /// Get a human readable file size for an arbitrary amount of bytes
    fn format_file_size(total_bytes: i64) -> String {
        let mut index: usize = 0;
        let mut bytes = total_bytes as f64;
        while index < UNITS.len() - 1 && bytes > DIVISOR {
            index += 1;
            bytes /= DIVISOR;
        }

        format!("{bytes:.2} {}", UNITS[index])
    }

    /// Given a platform and database source, resolve the path for the current attachment
    ///
    /// For macOS, `db_path` is unused. For iOS, `db_path` is the path to the root of the backup directory.
    ///
    /// iOS Parsing logic source is from [here](https://github.com/nprezant/iMessageBackup/blob/940d001fb7be557d5d57504eb26b3489e88de26e/imessage_backup_tools.py#L83-L85).
    pub fn resolved_attachment_path(&self, platform: &Platform, db_path: &Path) -> Option<String> {
        if let Some(path_str) = &self.filename {
            return match platform {
                Platform::macOS => Some(Attachment::gen_macos_attachment(path_str)),
                Platform::iOS => Attachment::gen_ios_attachment(path_str, db_path),
            };
        }
        None
    }

    /// Emit diagnostic data for the Attachments table
    ///
    /// This is defined outside of [crate::tables::table::Diagnostic] because it requires additional data.
    ///
    /// Get the number of attachments that are missing from the filesystem
    /// or are missing one of the following columns:
    ///
    /// - ck_server_change_token_blob
    /// - sr_ck_server_change_token_blob
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::{dirs::default_db_path, platform::Platform};
    /// use imessage_database::tables::table::{Diagnostic, get_connection};
    /// use imessage_database::tables::attachment::Attachment;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// Attachment::run_diagnostic(&conn, &db_path, &Platform::macOS);
    /// ```
    pub fn run_diagnostic(
        db: &Connection,
        db_path: &Path,
        platform: &Platform,
    ) -> Result<(), TableError> {
        processing();
        let mut total_attachments = 0;
        let mut null_attachments = 0;
        let mut statement_paths = db
            .prepare(&format!("SELECT filename FROM {ATTACHMENT}"))
            .map_err(TableError::Attachment)?;
        let paths = statement_paths
            .query_map([], |r| Ok(r.get(0)))
            .map_err(TableError::Attachment)?;

        let missing_files = paths
            .filter_map(Result::ok)
            .filter(|path: &Result<String, Error>| {
                // Keep track of the number of attachments in the table
                total_attachments += 1;
                if let Ok(filepath) = path {
                    match platform {
                        Platform::macOS => {
                            !Path::new(&Attachment::gen_macos_attachment(filepath)).exists()
                        }
                        Platform::iOS => {
                            if let Some(parsed_path) =
                                Attachment::gen_ios_attachment(filepath, db_path)
                            {
                                return !Path::new(&parsed_path).exists();
                            }
                            // This hits if the attachment path doesn't get generated
                            true
                        }
                    }
                } else {
                    // This hits if there is no path provided for the current attachment
                    null_attachments += 1;
                    true
                }
            })
            .count();

        let mut bytes_query = db
            .prepare(&format!("SELECT SUM(total_bytes) FROM {ATTACHMENT}"))
            .map_err(TableError::Messages)?;

        let total_bytes: i64 = bytes_query.query_row([], |r| r.get(0)).unwrap_or(0);

        done_processing();

        if total_attachments > 0 {
            println!("\rAttachment diagnostic data:");
            println!("    Total attachments: {total_attachments}");
            println!(
                "    Total attachment data: {}",
                Attachment::format_file_size(total_bytes)
            );
            if missing_files > 0 && total_attachments > 0 {
                println!(
                    "    Missing files: {missing_files:?} ({:.0}%)",
                    (missing_files as f64 / total_attachments as f64) * 100f64
                );
                println!("        No path provided: {null_attachments}");
                println!(
                    "        No file located: {}",
                    missing_files.saturating_sub(null_attachments)
                );
            }
        }
        Ok(())
    }

    /// Generate a macOS path for an attachment
    fn gen_macos_attachment(path: &str) -> String {
        if path.starts_with('~') {
            return path.replacen('~', &home(), 1);
        }
        path.to_string()
    }

    /// Generate an iOS path for an attachment
    fn gen_ios_attachment(file_path: &str, db_path: &Path) -> Option<String> {
        let input = file_path.get(2..)?;
        let filename = format!(
            "{:x}",
            Sha1::digest(format!("MediaDomain-{input}").as_bytes())
        );
        let directory = filename.get(0..2)?;

        Some(format!("{}/{directory}/{filename}", db_path.display()))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        tables::attachment::{Attachment, MediaType},
        util::platform::Platform,
    };

    use std::path::{Path, PathBuf};

    fn sample_attachment() -> Attachment {
        Attachment {
            rowid: 1,
            filename: Some("a/b/c.png".to_string()),
            uti: Some("public.png".to_string()),
            mime_type: Some("image".to_string()),
            transfer_name: Some("c.png".to_string()),
            total_bytes: 100,
            hide_attachment: 0,
            copied_path: None,
        }
    }

    #[test]
    fn can_get_path() {
        let attachment = sample_attachment();
        assert_eq!(attachment.path(), Some(Path::new("a/b/c.png")));
    }

    #[test]
    fn cant_get_path_missing() {
        let mut attachment = sample_attachment();
        attachment.filename = None;
        assert_eq!(attachment.path(), None);
    }

    #[test]
    fn can_get_extension() {
        let attachment = sample_attachment();
        assert_eq!(attachment.extension(), Some("png"));
    }

    #[test]
    fn cant_get_extension_missing() {
        let mut attachment = sample_attachment();
        attachment.filename = None;
        assert_eq!(attachment.extension(), None);
    }

    #[test]
    fn can_get_mime_type() {
        let attachment = sample_attachment();
        assert_eq!(attachment.mime_type(), MediaType::Image("image"));
    }

    #[test]
    fn can_get_mime_type_fake() {
        let mut attachment = sample_attachment();
        attachment.mime_type = Some("bloop".to_string());
        assert_eq!(attachment.mime_type(), MediaType::Other("bloop"));
    }

    #[test]
    fn can_get_mime_type_missing() {
        let mut attachment = sample_attachment();
        attachment.mime_type = None;
        assert_eq!(attachment.mime_type(), MediaType::Unknown);
    }

    #[test]
    fn can_get_filename() {
        let attachment = sample_attachment();
        assert_eq!(attachment.filename(), "c.png");
    }

    #[test]
    fn can_get_filename_no_transfer_name() {
        let mut attachment = sample_attachment();
        attachment.transfer_name = None;
        assert_eq!(attachment.filename(), "a/b/c.png");
    }

    #[test]
    fn can_get_filename_no_filename() {
        let mut attachment = sample_attachment();
        attachment.filename = None;
        assert_eq!(attachment.filename(), "c.png");
    }

    #[test]
    fn can_get_filename_no_meta() {
        let mut attachment = sample_attachment();
        attachment.transfer_name = None;
        attachment.filename = None;
        assert_eq!(attachment.filename(), "Attachment missing name metadata!");
    }

    #[test]
    fn can_get_resolved_path_macos() {
        let db_path = PathBuf::from("fake_root");
        let attachment = sample_attachment();

        assert_eq!(
            attachment.resolved_attachment_path(&Platform::macOS, &db_path),
            Some("a/b/c.png".to_string())
        );
    }

    #[test]
    fn can_get_resolved_path_macos_raw() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        attachment.filename = Some("~/a/b/c.png".to_string());

        assert!(
            attachment
                .resolved_attachment_path(&Platform::macOS, &db_path)
                .unwrap()
                .len()
                > attachment.filename.unwrap().len()
        );
    }

    #[test]
    fn can_get_resolved_path_macos_raw_tilde() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        attachment.filename = Some("~/a/b/c~d.png".to_string());

        assert!(attachment
            .resolved_attachment_path(&Platform::macOS, &db_path)
            .unwrap()
            .ends_with("c~d.png"));
    }

    #[test]
    fn can_get_resolved_path_ios() {
        let db_path = PathBuf::from("fake_root");
        let attachment = sample_attachment();

        assert_eq!(
            attachment.resolved_attachment_path(&Platform::iOS, &db_path),
            Some("fake_root/41/41746ffc65924078eae42725c979305626f57cca".to_string())
        );
    }

    #[test]
    fn cant_get_missing_resolved_path_macos() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        attachment.filename = None;

        assert_eq!(
            attachment.resolved_attachment_path(&Platform::macOS, &db_path),
            None
        );
    }

    #[test]
    fn cant_get_missing_resolved_path_ios() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        attachment.filename = None;

        assert_eq!(
            attachment.resolved_attachment_path(&Platform::iOS, &db_path),
            None
        );
    }

    #[test]
    fn can_get_file_size_bytes() {
        let attachment = sample_attachment();

        assert_eq!(attachment.file_size(), String::from("100.00 B"));
    }

    #[test]
    fn can_get_file_size_kb() {
        let mut attachment = sample_attachment();
        attachment.total_bytes = 2300;

        assert_eq!(attachment.file_size(), String::from("2.25 KB"));
    }

    #[test]
    fn can_get_file_size_mb() {
        let mut attachment = sample_attachment();
        attachment.total_bytes = 5612000;

        assert_eq!(attachment.file_size(), String::from("5.35 MB"));
    }

    #[test]
    fn can_get_file_size_gb() {
        let mut attachment: Attachment = sample_attachment();
        attachment.total_bytes = 9234712394;

        assert_eq!(attachment.file_size(), String::from("8.60 GB"));
    }

    #[test]
    fn can_get_file_size_cap() {
        let mut attachment: Attachment = sample_attachment();
        attachment.total_bytes = i64::MAX;

        assert_eq!(attachment.file_size(), String::from("8388608.00 TB"));
    }
}
