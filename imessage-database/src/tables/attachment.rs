/*!
 This module represents common (but not all) columns in the `attachment` table.
*/

use plist::Value;
use rusqlite::{CachedStatement, Connection, Error, Result, Row};
use sha1::{Digest, Sha1};

use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use crate::{
    error::{attachment::AttachmentError, table::TableError},
    message_types::sticker::{StickerEffect, StickerSource, get_sticker_effect},
    tables::{
        messages::Message,
        table::{ATTACHMENT, ATTRIBUTION_INFO, STICKER_USER_INFO, Table},
    },
    util::{
        dates::TIMESTAMP_FACTOR,
        dirs::home,
        output::{done_processing, processing},
        platform::Platform,
        plist::plist_as_dictionary,
        query_context::QueryContext,
        size::format_file_size,
    },
};

/// The default root directory for iMessage attachment data
pub const DEFAULT_ATTACHMENT_ROOT: &str = "~/Library/Messages/Attachments";
const COLS: &str = "a.rowid, a.filename, a.uti, a.mime_type, a.transfer_name, a.total_bytes, a.is_sticker, a.hide_attachment, a.emoji_image_short_description";

/// Represents the [MIME type](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_Types) of a message's attachment data
///
/// The interior `str` contains the subtype, i.e. `x-m4a` for `audio/x-m4a`
#[derive(Debug, PartialEq, Eq)]
pub enum MediaType<'a> {
    /// Image MIME type, such as `"image/png"` or `"image/jpeg"`
    Image(&'a str),
    /// Video MIME type, such as `"video/mp4"` or `"video/quicktime"`
    Video(&'a str),
    /// Audio MIME type, such as `"audio/mp3"` or `"audio/x-m4a`"
    Audio(&'a str),
    /// Text MIME type, such as `"text/plain"` or `"text/html"`
    Text(&'a str),
    /// Application MIME type, such as `"application/pdf"` or `"application/json"`
    Application(&'a str),
    /// Other MIME types that don't fit the standard categories
    Other(&'a str),
    /// Unknown MIME type when the type could not be determined
    Unknown,
}

impl MediaType<'_> {
    /// Given a [`MediaType`], generate the corresponding MIME type string
    ///
    /// # Example
    ///
    /// ```rust
    /// use imessage_database::tables::attachment::MediaType;
    ///
    /// println!("{:?}", MediaType::Image("png").as_mime_type()); // "image/png"
    /// ```
    #[must_use]
    pub fn as_mime_type(&self) -> String {
        match self {
            MediaType::Image(subtype) => format!("image/{subtype}"),
            MediaType::Video(subtype) => format!("video/{subtype}"),
            MediaType::Audio(subtype) => format!("audio/{subtype}"),
            MediaType::Text(subtype) => format!("text/{subtype}"),
            MediaType::Application(subtype) => format!("application/{subtype}"),
            MediaType::Other(mime) => (*mime).to_string(),
            MediaType::Unknown => String::new(),
        }
    }
}

/// Represents a single row in the `attachment` table.
#[derive(Debug)]
pub struct Attachment {
    /// The unique identifier for the attachment in the database
    pub rowid: i32,
    /// The path to the file on disk
    pub filename: Option<String>,
    /// The [Uniform Type Identifier](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/understanding_utis/understand_utis_intro/understand_utis_intro.html)
    pub uti: Option<String>,
    /// String representation of the file's MIME type
    pub mime_type: Option<String>,
    /// The name of the file when sent or received
    pub transfer_name: Option<String>,
    /// The total amount of data transferred over the network (not necessarily the size of the file)
    pub total_bytes: i64,
    /// `true` if the attachment was a sticker, else `false`
    pub is_sticker: bool,
    /// Flag indicating whether the attachment should be hidden in the UI
    pub hide_attachment: i32,
    /// The prompt used to generate a Genmoji
    pub emoji_description: Option<String>,
    /// Auxiliary data to denote that an attachment has been copied
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
            is_sticker: row.get("is_sticker").unwrap_or(false),
            hide_attachment: row.get("hide_attachment").unwrap_or(0),
            emoji_description: row.get("emoji_image_short_description").unwrap_or(None),
            copied_path: None,
        })
    }

    fn get(db: &Connection) -> Result<CachedStatement, TableError> {
        Ok(db.prepare_cached(&format!("SELECT * from {ATTACHMENT}"))?)
    }

    fn extract(attachment: Result<Result<Self, Error>, Error>) -> Result<Self, TableError> {
        match attachment {
            Ok(Ok(attachment)) => Ok(attachment),
            Err(why) | Ok(Err(why)) => Err(TableError::QueryError(why)),
        }
    }
}

impl Attachment {
    /// Gets a Vector of attachments associated with a single message
    ///
    /// The order of the attachments aligns with the order of the [`BubbleComponent::Attachment`](crate::tables::messages::models::BubbleComponent::Attachment)s in the message's [`body()`](crate::tables::table::AttributedBody).
    pub fn from_message(db: &Connection, msg: &Message) -> Result<Vec<Attachment>, TableError> {
        let mut out_l = vec![];
        if msg.has_attachments() {
            let mut statement = db
                .prepare(&format!(
                    "
                        SELECT {COLS}
                        FROM message_attachment_join j 
                        LEFT JOIN {ATTACHMENT} a ON j.attachment_id = a.ROWID
                        WHERE j.message_id = {}
                    ",
                    msg.rowid
                ))
                .or_else(|_| {
                    db.prepare(&format!(
                        "
                            SELECT *
                            FROM message_attachment_join j 
                            LEFT JOIN {ATTACHMENT} a ON j.attachment_id = a.ROWID
                            WHERE j.message_id = {}
                        ",
                        msg.rowid
                    ))
                })?;

            let iter = statement.query_map([], |row| Ok(Attachment::from_row(row)))?;

            for attachment in iter {
                let m = Attachment::extract(attachment)?;
                out_l.push(m);
            }
        }
        Ok(out_l)
    }

    /// Get the media type of an attachment
    #[must_use]
    pub fn mime_type(&'_ self) -> MediaType<'_> {
        match &self.mime_type {
            Some(mime) => {
                let mut mime_parts = mime.split('/');
                if let (Some(category), Some(subtype)) = (mime_parts.next(), mime_parts.next()) {
                    match category {
                        "image" => MediaType::Image(subtype),
                        "video" => MediaType::Video(subtype),
                        "audio" => MediaType::Audio(subtype),
                        "text" => MediaType::Text(subtype),
                        "application" => MediaType::Application(subtype),
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

    /// Read the attachment from the disk into a vector of bytes in memory
    ///
    /// `db_path` is the path to the root of the backup directory.
    /// This is the same path used by [`get_connection()`](crate::tables::table::get_connection).
    pub fn as_bytes(
        &self,
        platform: &Platform,
        db_path: &Path,
        custom_attachment_root: Option<&str>,
    ) -> Result<Option<Vec<u8>>, AttachmentError> {
        if let Some(file_path) =
            self.resolved_attachment_path(platform, db_path, custom_attachment_root)
        {
            let mut file = File::open(&file_path)
                .map_err(|err| AttachmentError::Unreadable(file_path.clone(), err))?;
            let mut bytes = vec![];
            file.read_to_end(&mut bytes)
                .map_err(|err| AttachmentError::Unreadable(file_path.clone(), err))?;

            return Ok(Some(bytes));
        }
        Ok(None)
    }

    /// Determine the [`StickerEffect`] of a sticker message
    ///
    /// `db_path` is the path to the root of the backup directory.
    /// This is the same path used by [`get_connection()`](crate::tables::table::get_connection).
    pub fn get_sticker_effect(
        &self,
        platform: &Platform,
        db_path: &Path,
        custom_attachment_root: Option<&str>,
    ) -> Result<Option<StickerEffect>, AttachmentError> {
        // Handle the non-sticker case
        if !self.is_sticker {
            return Ok(None);
        }

        // Try to parse the HEIC data
        if let Some(data) = self.as_bytes(platform, db_path, custom_attachment_root)? {
            return Ok(Some(get_sticker_effect(data)));
        }

        // Default if the attachment is a sticker and cannot be parsed/read
        Ok(Some(StickerEffect::default()))
    }

    /// Get the path to an attachment, if it exists
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match &self.filename {
            Some(name) => Some(Path::new(name)),
            None => None,
        }
    }

    /// Get the file name extension of an attachment, if it exists
    #[must_use]
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
    ///
    /// If the [`transfer_name`](Self::transfer_name) field is populated, use that. If it is not present, fall back to the `filename` field.
    #[must_use]
    pub fn filename(&self) -> Option<&str> {
        self.transfer_name.as_deref().or(self.filename.as_deref())
    }

    /// Get a human readable file size for an attachment using [`format_file_size`]
    #[must_use]
    pub fn file_size(&self) -> String {
        format_file_size(u64::try_from(self.total_bytes).unwrap_or(0))
    }

    /// Get the total attachment bytes referenced in the table
    pub fn get_total_attachment_bytes(
        db: &Connection,
        context: &QueryContext,
    ) -> Result<u64, TableError> {
        let mut bytes_query = if context.start.is_some() || context.end.is_some() {
            let mut statement = format!("SELECT IFNULL(SUM(total_bytes), 0) FROM {ATTACHMENT} a");

            statement.push_str(" WHERE ");
            if let Some(start) = context.start {
                statement.push_str(&format!(
                    "    a.created_date >= {}",
                    start / TIMESTAMP_FACTOR
                ));
            }
            if let Some(end) = context.end {
                if context.start.is_some() {
                    statement.push_str(" AND ");
                }
                statement.push_str(&format!("    a.created_date <= {}", end / TIMESTAMP_FACTOR));
            }

            db.prepare(&statement)?
        } else {
            db.prepare(&format!(
                "SELECT IFNULL(SUM(total_bytes), 0) FROM {ATTACHMENT}"
            ))?
        };
        Ok(bytes_query
            .query_row([], |r| -> Result<i64> { r.get(0) })
            .map(|res: i64| u64::try_from(res).unwrap_or(0))?)
    }

    /// Given a platform and database source, resolve the path for the current attachment
    ///
    /// For macOS, `db_path` is unused. For iOS, `db_path` is the path to the root of the backup directory.
    /// This is the same path used by [`get_connection()`](crate::tables::table::get_connection).
    ///
    /// On iOS, file names are derived from SHA-1 hash of `MediaDomain-` concatenated with the relative [`self.filename()`](Self::filename).
    /// Between the domain and the path there is a dash. Read more [here](https://theapplewiki.com/index.php?title=ITunes_Backup).
    ///
    /// Use the optional `custom_attachment_root` parameter when the attachments are not stored in
    /// the same place as the database expects.The expected location is [`DEFAULT_ATTACHMENT_ROOT`].
    /// A custom attachment root like `/custom/path` will overwrite a path like `~/Library/Messages/Attachments/3d/...` to `/custom/path/3d/...`
    #[must_use]
    pub fn resolved_attachment_path(
        &self,
        platform: &Platform,
        db_path: &Path,
        custom_attachment_root: Option<&str>,
    ) -> Option<String> {
        if let Some(mut path_str) = self.filename.clone() {
            // Apply custom attachment path
            if let Some(custom_attachment_path) = custom_attachment_root {
                path_str = path_str.replace(DEFAULT_ATTACHMENT_ROOT, custom_attachment_path);
            }
            return match platform {
                Platform::macOS => Some(Attachment::gen_macos_attachment(&path_str)),
                Platform::iOS => Attachment::gen_ios_attachment(&path_str, db_path),
            };
        }
        None
    }

    /// Emit diagnostic data for the Attachments table
    ///
    /// This is defined outside of [`Diagnostic`](crate::tables::table::Diagnostic) because it requires additional data.
    ///
    /// Get the number of attachments that are missing, either because the path is missing from the
    /// table or the path does not point to a file.
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
    ///
    /// `db_path` is the path to the root of the backup directory.
    /// This is the same path used by [`get_connection()`](crate::tables::table::get_connection).
    pub fn run_diagnostic(
        db: &Connection,
        db_path: &Path,
        platform: &Platform,
    ) -> Result<(), TableError> {
        processing();
        let mut total_attachments = 0;
        let mut null_attachments = 0;
        let mut size_on_disk: u64 = 0;
        let mut statement_paths = db.prepare(&format!("SELECT filename FROM {ATTACHMENT}"))?;
        let paths = statement_paths.query_map([], |r| Ok(r.get(0)))?;

        let missing_files = paths
            .filter_map(Result::ok)
            .filter(|path: &Result<String, Error>| {
                // Keep track of the number of attachments in the table
                total_attachments += 1;
                if let Ok(filepath) = path {
                    match platform {
                        Platform::macOS => {
                            let path = Attachment::gen_macos_attachment(filepath);
                            let file = Path::new(&path);
                            if let Ok(metadata) = file.metadata() {
                                size_on_disk += metadata.len();
                            }
                            !file.exists()
                        }
                        Platform::iOS => {
                            if let Some(parsed_path) =
                                Attachment::gen_ios_attachment(filepath, db_path)
                            {
                                let file = Path::new(&parsed_path);
                                if let Ok(metadata) = file.metadata() {
                                    size_on_disk += metadata.len();
                                }
                                return !file.exists();
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

        let total_bytes =
            Attachment::get_total_attachment_bytes(db, &QueryContext::default()).unwrap_or(0);

        done_processing();

        if total_attachments > 0 {
            println!("\rAttachment diagnostic data:");
            println!("    Total attachments: {total_attachments}");
            println!(
                "        Data referenced in table: {}",
                format_file_size(total_bytes)
            );
            println!(
                "        Data present on disk: {}",
                format_file_size(size_on_disk)
            );
            if missing_files > 0 && total_attachments > 0 {
                println!(
                    "    Missing files: {missing_files:?} ({:.0}%)",
                    (missing_files as f64 / f64::from(total_attachments)) * 100f64
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

    /// Get an attachment's plist from the [`STICKER_USER_INFO`] BLOB column
    ///
    /// Calling this hits the database, so it is expensive and should
    /// only get invoked when needed.
    ///
    /// This column contains data used for sticker attachments.
    fn sticker_info(&self, db: &Connection) -> Option<Value> {
        Value::from_reader(self.get_blob(db, ATTACHMENT, STICKER_USER_INFO, self.rowid.into())?)
            .ok()
    }

    /// Get an attachment's plist from the [`ATTRIBUTION_INFO`] BLOB column
    ///
    /// Calling this hits the database, so it is expensive and should
    /// only get invoked when needed.
    ///
    /// This column contains metadata used by image attachments.
    fn attribution_info(&self, db: &Connection) -> Option<Value> {
        Value::from_reader(self.get_blob(db, ATTACHMENT, ATTRIBUTION_INFO, self.rowid.into())?).ok()
    }

    /// Parse a sticker's source from the Bundle ID stored in [`STICKER_USER_INFO`] `plist` data
    ///
    /// Calling this hits the database, so it is expensive and should
    /// only get invoked when needed.
    pub fn get_sticker_source(&self, db: &Connection) -> Option<StickerSource> {
        if let Some(sticker_info) = self.sticker_info(db) {
            let plist = plist_as_dictionary(&sticker_info).ok()?;
            let bundle_id = plist.get("pid")?.as_string()?;
            return StickerSource::from_bundle_id(bundle_id);
        }
        None
    }

    /// Parse a sticker's application name stored in [`ATTRIBUTION_INFO`] `plist` data
    ///
    /// Calling this hits the database, so it is expensive and should
    /// only get invoked when needed.
    pub fn get_sticker_source_application_name(&self, db: &Connection) -> Option<String> {
        if let Some(attribution_info) = self.attribution_info(db) {
            let plist = plist_as_dictionary(&attribution_info).ok()?;
            return Some(plist.get("name")?.as_string()?.to_owned());
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        tables::{
            attachment::{Attachment, DEFAULT_ATTACHMENT_ROOT, MediaType},
            table::get_connection,
        },
        util::{platform::Platform, query_context::QueryContext},
    };

    use std::{
        collections::BTreeSet,
        env::current_dir,
        path::{Path, PathBuf},
    };

    fn sample_attachment() -> Attachment {
        Attachment {
            rowid: 1,
            filename: Some("a/b/c.png".to_string()),
            uti: Some("public.png".to_string()),
            mime_type: Some("image/png".to_string()),
            transfer_name: Some("c.png".to_string()),
            total_bytes: 100,
            is_sticker: false,
            hide_attachment: 0,
            emoji_description: None,
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
    fn can_get_mime_type_png() {
        let attachment = sample_attachment();
        assert_eq!(attachment.mime_type(), MediaType::Image("png"));
    }

    #[test]
    fn can_get_mime_type_heic() {
        let mut attachment = sample_attachment();
        attachment.mime_type = Some("image/heic".to_string());
        assert_eq!(attachment.mime_type(), MediaType::Image("heic"));
    }

    #[test]
    fn can_get_mime_type_fake() {
        let mut attachment = sample_attachment();
        attachment.mime_type = Some("fake/bloop".to_string());
        assert_eq!(attachment.mime_type(), MediaType::Other("fake/bloop"));
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
        assert_eq!(attachment.filename(), Some("c.png"));
    }

    #[test]
    fn can_get_filename_no_transfer_name() {
        let mut attachment = sample_attachment();
        attachment.transfer_name = None;
        assert_eq!(attachment.filename(), Some("a/b/c.png"));
    }

    #[test]
    fn can_get_filename_no_filename() {
        let mut attachment = sample_attachment();
        attachment.filename = None;
        assert_eq!(attachment.filename(), Some("c.png"));
    }

    #[test]
    fn can_get_filename_no_meta() {
        let mut attachment = sample_attachment();
        attachment.transfer_name = None;
        attachment.filename = None;
        assert_eq!(attachment.filename(), None);
    }

    #[test]
    fn can_get_resolved_path_macos() {
        let db_path = PathBuf::from("fake_root");
        let attachment = sample_attachment();

        assert_eq!(
            attachment.resolved_attachment_path(&Platform::macOS, &db_path, None),
            Some("a/b/c.png".to_string())
        );
    }

    #[test]
    fn can_get_resolved_path_macos_custom() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        // Sample path like `~/Library/Messages/Attachments/0a/10/.../image.jpeg`
        attachment.filename = Some(format!("{DEFAULT_ATTACHMENT_ROOT}/a/b/c.png"));

        assert_eq!(
            attachment.resolved_attachment_path(&Platform::macOS, &db_path, Some("custom/root")),
            Some("custom/root/a/b/c.png".to_string())
        );
    }

    #[test]
    fn can_get_resolved_path_macos_raw() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        attachment.filename = Some("~/a/b/c.png".to_string());

        assert!(
            attachment
                .resolved_attachment_path(&Platform::macOS, &db_path, None)
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

        assert!(
            attachment
                .resolved_attachment_path(&Platform::macOS, &db_path, None)
                .unwrap()
                .ends_with("c~d.png")
        );
    }

    #[test]
    fn can_get_resolved_path_ios() {
        let db_path = PathBuf::from("fake_root");
        let attachment = sample_attachment();

        assert_eq!(
            attachment.resolved_attachment_path(&Platform::iOS, &db_path, None),
            Some("fake_root/41/41746ffc65924078eae42725c979305626f57cca".to_string())
        );
    }

    #[test]
    fn can_get_resolved_path_ios_custom() {
        let db_path = PathBuf::from("fake_root");
        let attachment = sample_attachment();

        // iOS Backups store attachments at the same level as the database file, so if the backup
        // is intact, the custom root is not relevant
        assert_eq!(
            attachment.resolved_attachment_path(&Platform::iOS, &db_path, Some("custom/root")),
            Some("fake_root/41/41746ffc65924078eae42725c979305626f57cca".to_string())
        );
    }

    #[test]
    fn cant_get_missing_resolved_path_macos() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        attachment.filename = None;

        assert_eq!(
            attachment.resolved_attachment_path(&Platform::macOS, &db_path, None),
            None
        );
    }

    #[test]
    fn cant_get_missing_resolved_path_ios() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        attachment.filename = None;

        assert_eq!(
            attachment.resolved_attachment_path(&Platform::iOS, &db_path, None),
            None
        );
    }

    #[test]
    fn can_get_attachment_bytes_no_filter() {
        let db_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let connection = get_connection(&db_path).unwrap();

        let context = QueryContext::default();

        assert!(Attachment::get_total_attachment_bytes(&connection, &context).is_ok());
    }

    #[test]
    fn can_get_attachment_bytes_start_filter() {
        let db_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let connection = get_connection(&db_path).unwrap();

        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();

        assert!(Attachment::get_total_attachment_bytes(&connection, &context).is_ok());
    }

    #[test]
    fn can_get_attachment_bytes_end_filter() {
        let db_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let connection = get_connection(&db_path).unwrap();

        let mut context = QueryContext::default();
        context.set_end("2020-01-01").unwrap();

        assert!(Attachment::get_total_attachment_bytes(&connection, &context).is_ok());
    }

    #[test]
    fn can_get_attachment_bytes_start_end_filter() {
        let db_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let connection = get_connection(&db_path).unwrap();

        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();
        context.set_end("2021-01-01").unwrap();

        assert!(Attachment::get_total_attachment_bytes(&connection, &context).is_ok());
    }

    #[test]
    fn can_get_attachment_bytes_contact_filter() {
        let db_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let connection = get_connection(&db_path).unwrap();

        let mut context = QueryContext::default();
        context.set_selected_chat_ids(BTreeSet::from([1, 2, 3]));
        context.set_selected_handle_ids(BTreeSet::from([1, 2, 3]));

        assert!(Attachment::get_total_attachment_bytes(&connection, &context).is_ok());
    }

    #[test]
    fn can_get_attachment_bytes_contact_date_filter() {
        let db_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let connection = get_connection(&db_path).unwrap();

        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();
        context.set_end("2021-01-01").unwrap();
        context.set_selected_chat_ids(BTreeSet::from([1, 2, 3]));
        context.set_selected_handle_ids(BTreeSet::from([1, 2, 3]));

        assert!(Attachment::get_total_attachment_bytes(&connection, &context).is_ok());
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
