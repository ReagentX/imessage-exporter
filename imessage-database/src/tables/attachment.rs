/*!
 Attachment table rows and attachment file path helpers.
*/

use plist::Value;
use rusqlite::{CachedStatement, Connection, Error, Result, Row};
use sha1::{Digest, Sha1};

use std::{
    borrow::Cow,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use crate::{
    error::{attachment::AttachmentError, table::TableError},
    message_types::sticker::{StickerDecoration, StickerEffect, StickerSource, get_sticker_effect},
    tables::{
        diagnostic::AttachmentDiagnostic,
        messages::Message,
        table::{
            ATTACHMENT, ATTRIBUTION_INFO, CHAT_MESSAGE_JOIN, MESSAGE, MESSAGE_ATTACHMENT_JOIN,
            STICKER_USER_INFO, Table,
        },
    },
    util::{
        dirs::home, platform::Platform, plist::plist_as_dictionary, query_context::QueryContext,
        size::format_file_size,
    },
};

// MARK: Constants
/// Default macOS Messages root used by attachment paths.
pub const DEFAULT_MESSAGES_ROOT: &str = "~/Library/Messages";
/// Alternate root directory used by a jailbroken iOS device's `sms.db`
///
/// The `sms.db` database uses the same schema and path conventions as a macOS `chat.db`,
/// but attachment paths are rooted under `~/Library/SMS` instead of `~/Library/Messages`.
pub const DEFAULT_SMS_ROOT: &str = "~/Library/SMS";
/// Default macOS attachment root.
pub const DEFAULT_ATTACHMENT_ROOT: &str = "~/Library/Messages/Attachments";
/// Default macOS sticker cache root.
pub const DEFAULT_STICKER_CACHE_ROOT: &str = "~/Library/Messages/StickerCache";
const COLS: &str = "a.rowid, a.guid, a.filename, a.uti, a.mime_type, a.transfer_name, a.total_bytes, a.is_sticker, a.hide_attachment, a.emoji_image_short_description";

// MARK: MediaType
/// Represents the [MIME type](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_Types) of a message's attachment data
///
/// The borrowed string stores the subtype for known categories, e.g. `x-m4a`
/// for `audio/x-m4a`.
#[derive(Debug, PartialEq, Eq)]
pub enum MediaType<'a> {
    /// Image MIME type, such as `"image/png"` or `"image/jpeg"`.
    Image(&'a str),
    /// Video MIME type, such as `"video/mp4"` or `"video/quicktime"`.
    Video(&'a str),
    /// Audio MIME type, such as `"audio/mp3"` or `"audio/x-m4a"`.
    Audio(&'a str),
    /// Text MIME type, such as `"text/plain"` or `"text/html"`.
    Text(&'a str),
    /// Application MIME type, such as `"application/pdf"` or `"application/json"`.
    Application(&'a str),
    /// Other MIME type stored as the complete MIME string.
    Other(&'a str),
    /// MIME type could not be determined.
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

/// Row from the `attachment` table.
#[derive(Debug)]
pub struct Attachment {
    /// Attachment row ID.
    pub rowid: i32,
    /// The attachment's GUID. Matches the `__kIMFileTransferGUIDAttributeName`
    /// carried on the message body's attachment ranges, letting the exporter
    /// pair a body placeholder with its resolved attachment by identity rather
    /// than by position.
    pub guid: Option<String>,
    /// Path to the file on disk as stored in the database.
    pub filename: Option<String>,
    /// The [Uniform Type Identifier](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/understanding_utis/understand_utis_intro/understand_utis_intro.html).
    pub uti: Option<String>,
    /// MIME type string.
    pub mime_type: Option<String>,
    /// Original transfer filename.
    pub transfer_name: Option<String>,
    /// Total bytes recorded by Messages.
    pub total_bytes: i64,
    /// `true` when the attachment is a sticker.
    pub is_sticker: bool,
    /// Messages UI hide flag.
    pub hide_attachment: i32,
    /// Prompt used to generate a Genmoji.
    pub emoji_description: Option<String>,
    /// Auxiliary data to denote that an attachment has been copied or converted.
    pub copied_path: Option<PathBuf>,
}

// MARK: Table
impl Table for Attachment {
    fn from_row(row: &Row) -> Result<Attachment> {
        Ok(Attachment {
            rowid: row.get("rowid")?,
            guid: row.get("guid").unwrap_or(None),
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

    fn get(db: &'_ Connection) -> Result<CachedStatement<'_>, TableError> {
        Ok(db.prepare_cached(&format!("SELECT * from {ATTACHMENT}"))?)
    }
}

// MARK: Impl
impl Attachment {
    /// Load attachments associated with one message.
    ///
    /// Rows are returned in the `message_attachment_join` table's order, which is
    /// not guaranteed to align with the order of the attachment
    /// [`AttributedRange`](crate::tables::messages::models::AttributedRange)s
    /// (those whose [`attachment`](crate::tables::messages::models::AttributedRange::attachment)
    /// is `Some`) in the message's
    /// [`attributed_body()`](crate::tables::messages::message::Message::attributed_body).
    /// Callers pairing body ranges to rows should match on the file-transfer GUID
    /// rather than relying on position.
    pub fn from_message(db: &Connection, msg: &Message) -> Result<Vec<Attachment>, TableError> {
        let mut out_l = vec![];
        if msg.has_attachments() {
            let mut statement = db
                .prepare_cached(&format!(
                    "
                        SELECT {COLS}
                        FROM message_attachment_join j 
                        LEFT JOIN {ATTACHMENT} a ON j.attachment_id = a.ROWID
                        WHERE j.message_id = ?1
                    ",
                ))
                .or_else(|_| {
                    db.prepare_cached(&format!(
                        "
                            SELECT *
                            FROM message_attachment_join j 
                            LEFT JOIN {ATTACHMENT} a ON j.attachment_id = a.ROWID
                            WHERE j.message_id = ?1
                        ",
                    ))
                })?;

            for attachment in Attachment::rows(&mut statement, [msg.rowid])? {
                out_l.push(attachment?);
            }
        }
        Ok(out_l)
    }

    /// Classify this attachment's MIME type.
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
                if let Some(uti) = &self.uti {
                    match uti.as_str() {
                        // Audio messages are stored as Core Audio Format files.
                        "com.apple.coreaudio-format" => MediaType::Audio("x-caf; codecs=opus"),
                        _ => MediaType::Unknown,
                    }
                } else {
                    MediaType::Unknown
                }
            }
        }
    }

    /// `true` when this sticker's underlying file is animated: a HEIC
    /// sequence (`image/heics`, `image/heic-sequence`) or a video
    /// (`video/*`). Static stickers (HEIC, PNG, etc.) return `false`.
    /// Renderer presentation decisions are the caller's concern.
    #[must_use]
    pub fn is_animated_sticker(&self) -> bool {
        self.is_sticker
            && matches!(
                self.mime_type(),
                MediaType::Image("heics" | "HEICS" | "heic-sequence") | MediaType::Video(_),
            )
    }

    /// Read the attachment file into memory.
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

    /// Parse the [`StickerEffect`] for a sticker attachment.
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
            return Ok(Some(get_sticker_effect(&data)));
        }

        // Default if the attachment is a sticker and cannot be parsed/read
        Ok(Some(StickerEffect::default()))
    }

    /// Return the stored attachment path.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match &self.filename {
            Some(name) => Some(Path::new(name)),
            None => None,
        }
    }

    /// Return the stored attachment path extension.
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

    /// Return the transfer filename, falling back to the stored path.
    ///
    /// [`transfer_name`](Self::transfer_name) is the display filename when present; [`filename`](Self::filename) is the
    /// database path.
    #[must_use]
    pub fn filename(&self) -> Option<&str> {
        self.transfer_name.as_deref().or(self.filename.as_deref())
    }

    /// Return [`total_bytes`](Self::total_bytes) as a human-readable size.
    #[must_use]
    pub fn file_size(&self) -> String {
        format_file_size(u64::try_from(self.total_bytes).unwrap_or(0))
    }

    /// Sum on-disk bytes for attachments the export would copy under this [`QueryContext`].
    ///
    /// # Errors
    ///
    /// Returns [`TableError`] if the query fails.
    pub fn get_total_attachment_bytes(
        db: &Connection,
        context: &QueryContext,
    ) -> Result<u64, TableError> {
        let statement = if context.has_filters() {
            format!(
                "SELECT IFNULL(SUM(a.total_bytes), 0) FROM {ATTACHMENT} a \
             WHERE a.ROWID IN ( \
                 SELECT maj.attachment_id \
                 FROM {MESSAGE_ATTACHMENT_JOIN} maj \
                 JOIN {MESSAGE} m ON m.ROWID = maj.message_id \
                 LEFT JOIN {CHAT_MESSAGE_JOIN} c ON c.message_id = m.ROWID \
                 {} \
             )",
                Message::generate_filter_statement(context, false)
            )
        } else {
            format!("SELECT IFNULL(SUM(total_bytes), 0) FROM {ATTACHMENT}")
        };

        let mut bytes_query = db.prepare(&statement)?;
        Ok(bytes_query
            .query_row([], |r| -> Result<i64> { r.get(0) })
            .map(|res: i64| u64::try_from(res).unwrap_or(0))?)
    }

    /// Resolve the on-disk path for this attachment.
    ///
    /// For macOS, `db_path` is unused. For iOS, `db_path` is the path to the root of the backup directory.
    /// This is the same path used by [`get_connection()`](crate::tables::table::get_connection).
    ///
    /// On encrypted iOS backups, file names are derived from the SHA-1 hash of
    /// `MediaDomain-` plus the relative [`filename`](Self::filename). Read more [here](https://theapplewiki.com/index.php?title=ITunes_Backup).
    ///
    /// Use the optional `custom_attachment_root` parameter when attachment data is stored under a
    /// different Messages root than the database expects. This replaces the leading Messages root,
    /// not just the `Attachments` directory, so it affects both [`DEFAULT_ATTACHMENT_ROOT`] and
    /// [`DEFAULT_STICKER_CACHE_ROOT`].
    ///
    /// For example, a custom attachment root like `/custom/path` will rewrite
    /// `~/Library/Messages/Attachments/3d/...` to `/custom/path/Attachments/3d/...` and
    /// `~/Library/Messages/StickerCache/ab/...` to `/custom/path/StickerCache/ab/...`.
    ///
    /// For a jailbroken iOS `sms.db`, attachment paths start with [`DEFAULT_SMS_ROOT`] (`~/Library/SMS`)
    /// instead of [`DEFAULT_MESSAGES_ROOT`]. These databases behave like macOS databases and should
    /// use [`Platform::macOS`], not [`Platform::iOS`], which is reserved for encrypted Finder/Apple Devices/iTunes backups.
    #[must_use]
    pub fn resolved_attachment_path(
        &self,
        platform: &Platform,
        db_path: &Path,
        custom_attachment_root: Option<&str>,
    ) -> Option<String> {
        let mut path_str = self.filename.clone()?;

        // Apply custom attachment path, if provided
        if matches!(platform, Platform::macOS)
            && let Some(custom_attachment_path) = custom_attachment_root
        {
            path_str =
                Attachment::apply_custom_root(&path_str, custom_attachment_path).into_owned();
        }

        match platform {
            Platform::macOS => Some(Attachment::gen_macos_attachment(&path_str)),
            Platform::iOS => Attachment::gen_ios_attachment(&path_str, db_path),
        }
    }

    /// Compute diagnostic data for the `attachment` table.
    ///
    /// Counts the number of attachments that are missing, either because the path is missing from the
    /// table or the path does not point to a file.
    ///
    /// # Example:
    ///
    /// ```no_run
    /// use imessage_database::util::{dirs::default_db_path, platform::Platform};
    /// use imessage_database::tables::table::get_connection;
    /// use imessage_database::tables::attachment::Attachment;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// Attachment::run_diagnostic(&conn, &db_path, &Platform::macOS, None);
    /// ```
    ///
    /// `db_path` is the path to the root of the backup directory.
    /// This is the same path used by [`get_connection()`](crate::tables::table::get_connection).
    pub fn run_diagnostic(
        db: &Connection,
        db_path: &Path,
        platform: &Platform,
        custom_attachment_root: Option<&str>,
    ) -> Result<AttachmentDiagnostic, TableError> {
        let mut total_attachments = 0usize;
        let mut no_path_provided = 0usize;
        let mut total_bytes_on_disk: u64 = 0;
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
                            let path = match custom_attachment_root {
                                Some(custom_root) => Attachment::gen_macos_attachment(
                                    &Attachment::apply_custom_root(filepath, custom_root),
                                ),
                                None => Attachment::gen_macos_attachment(filepath),
                            };
                            let file = Path::new(&path);
                            match file.metadata() {
                                Ok(metadata) => {
                                    total_bytes_on_disk += metadata.len();
                                    false
                                }
                                Err(_) => true,
                            }
                        }
                        Platform::iOS => {
                            if let Some(parsed_path) =
                                Attachment::gen_ios_attachment(filepath, db_path)
                            {
                                let file = Path::new(&parsed_path);
                                return match file.metadata() {
                                    Ok(metadata) => {
                                        total_bytes_on_disk += metadata.len();
                                        false
                                    }
                                    Err(_) => true,
                                };
                            }
                            // This hits if the attachment path doesn't get generated
                            true
                        }
                    }
                } else {
                    // This hits if there is no path provided for the current attachment
                    no_path_provided += 1;
                    true
                }
            })
            .count();

        let total_bytes_referenced =
            Attachment::get_total_attachment_bytes(db, &QueryContext::default()).unwrap_or(0);

        Ok(AttachmentDiagnostic {
            total_attachments,
            total_bytes_referenced,
            total_bytes_on_disk,
            missing_files,
            no_path_provided,
        })
    }

    /// Replace the default Messages or SMS root prefix with a custom attachment root.
    fn apply_custom_root<'a>(path: &'a str, custom_root: &str) -> Cow<'a, str> {
        let prefix = if path.starts_with(DEFAULT_MESSAGES_ROOT) {
            Some(DEFAULT_MESSAGES_ROOT)
        } else if path.starts_with(DEFAULT_SMS_ROOT) {
            Some(DEFAULT_SMS_ROOT)
        } else {
            None
        };
        match prefix {
            Some(old) => Cow::Owned(path.replacen(old, custom_root, 1)),
            None => Cow::Borrowed(path),
        }
    }

    /// Resolve a macOS attachment path.
    fn gen_macos_attachment(path: &str) -> String {
        if path.starts_with('~') {
            return path.replacen('~', &home(), 1);
        }
        path.to_string()
    }

    /// Resolve an encrypted iOS backup attachment path.
    fn gen_ios_attachment(file_path: &str, db_path: &Path) -> Option<String> {
        let input = file_path.get(2..)?;
        let digest = Sha1::digest(format!("MediaDomain-{input}").as_bytes());
        let filename = digest
            .iter()
            .map(|byte| format!("{:02x}", byte))
            .collect::<String>();
        let directory = filename.get(0..2)?;

        Some(format!("{}/{directory}/{filename}", db_path.display()))
    }

    /// Parse the [`STICKER_USER_INFO`] `BLOB` column as a property list.
    ///
    /// Calling this reads a `BLOB` from the database.
    ///
    /// This column contains data used for sticker attachments.
    fn sticker_info(&self, db: &Connection) -> Option<Value> {
        Value::from_reader(self.get_blob(db, ATTACHMENT, STICKER_USER_INFO, self.rowid.into())?)
            .ok()
    }

    /// Parse the [`ATTRIBUTION_INFO`] `BLOB` column as a property list.
    ///
    /// Calling this reads a `BLOB` from the database.
    ///
    /// This column contains metadata used by image attachments.
    fn attribution_info(&self, db: &Connection) -> Option<Value> {
        Value::from_reader(self.get_blob(db, ATTACHMENT, ATTRIBUTION_INFO, self.rowid.into())?).ok()
    }

    /// Parse a sticker's source from the bundle ID in [`STICKER_USER_INFO`].
    ///
    /// Calling this reads a `BLOB` from the database.
    pub fn get_sticker_source(&self, db: &Connection) -> Option<StickerSource> {
        if let Some(sticker_info) = self.sticker_info(db) {
            let plist = plist_as_dictionary(&sticker_info).ok()?;
            let bundle_id = plist.get("pid")?.as_string()?;
            return StickerSource::from_bundle_id(bundle_id);
        }
        None
    }

    /// Parse a sticker's source application name from [`ATTRIBUTION_INFO`].
    ///
    /// Calling this reads a `BLOB` from the database.
    pub fn get_sticker_source_application_name(&self, db: &Connection) -> Option<String> {
        if let Some(attribution_info) = self.attribution_info(db) {
            let plist = plist_as_dictionary(&attribution_info).ok()?;
            return Some(plist.get("name")?.as_string()?.to_owned());
        }
        None
    }

    /// Resolve a sticker's [`StickerSource`] into a [`StickerDecoration`].
    /// Combines [`get_sticker_source`](Self::get_sticker_source),
    /// [`get_sticker_source_application_name`](Self::get_sticker_source_application_name),
    /// and [`get_sticker_effect`](Self::get_sticker_effect) so
    /// consumers don't have to re-derive the dispatch.
    ///
    /// Returns `None` in three cases:
    /// - The sticker has no readable source (missing [`STICKER_USER_INFO`],
    ///   malformed plist, or unrecognized bundle id).
    /// - The source is [`StickerSource::Genmoji`] but `emoji_description` is
    ///   unset.
    /// - The source is [`StickerSource::UserGenerated`] but the effect blob
    ///   is missing or unreadable.
    ///
    /// [`StickerSource::Memoji`] and [`StickerSource::App`] always yield
    /// `Some`.
    pub fn get_sticker_decoration(
        &self,
        db: &Connection,
        platform: &Platform,
        db_path: &Path,
        attachment_root: Option<&str>,
    ) -> Option<StickerDecoration> {
        let source = self.get_sticker_source(db)?;
        match source {
            StickerSource::Genmoji => self
                .emoji_description
                .as_deref()
                .map(|prompt| StickerDecoration::GenmojiPrompt(prompt.to_string())),
            StickerSource::Memoji => Some(StickerDecoration::Memoji),
            StickerSource::UserGenerated => self
                .get_sticker_effect(platform, db_path, attachment_root)
                .ok()
                .flatten()
                .map(StickerDecoration::Effect),
            StickerSource::App(bundle_id) => Some(StickerDecoration::AppName(
                self.get_sticker_source_application_name(db)
                    .unwrap_or(bundle_id),
            )),
        }
    }
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use crate::{
        tables::{
            attachment::{
                Attachment, DEFAULT_ATTACHMENT_ROOT, DEFAULT_SMS_ROOT, DEFAULT_STICKER_CACHE_ROOT,
                MediaType,
            },
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
            guid: None,
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
    fn is_animated_sticker_static_heic() {
        let mut attachment = sample_attachment();
        attachment.is_sticker = true;
        attachment.mime_type = Some("image/heic".to_string());
        assert!(!attachment.is_animated_sticker());
    }

    #[test]
    fn is_animated_sticker_heic_sequence() {
        let mut attachment = sample_attachment();
        attachment.is_sticker = true;
        attachment.mime_type = Some("image/heic-sequence".to_string());
        assert!(attachment.is_animated_sticker());
    }

    #[test]
    fn is_animated_sticker_video_memoji() {
        let mut attachment = sample_attachment();
        attachment.is_sticker = true;
        attachment.mime_type = Some("video/quicktime".to_string());
        assert!(attachment.is_animated_sticker());
    }

    #[test]
    fn is_animated_sticker_requires_sticker_flag() {
        let mut attachment = sample_attachment();
        attachment.is_sticker = false;
        attachment.mime_type = Some("video/quicktime".to_string());
        assert!(!attachment.is_animated_sticker());
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
            Some("custom/root/Attachments/a/b/c.png".to_string())
        );
    }

    #[test]
    fn can_get_resolved_path_macos_custom_sticker() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        // Sample path like `~/Library/Messages/StickerCache/0a/10/.../image.jpeg`
        attachment.filename = Some(format!("{DEFAULT_STICKER_CACHE_ROOT}/a/b/c.png"));

        assert_eq!(
            attachment.resolved_attachment_path(&Platform::macOS, &db_path, Some("custom/root")),
            Some("custom/root/StickerCache/a/b/c.png".to_string())
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
    fn can_get_resolved_path_ios_custom_ignores_prefixed_path() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        attachment.filename = Some(format!("{DEFAULT_ATTACHMENT_ROOT}/a/b/c.png"));
        let expected = attachment.resolved_attachment_path(&Platform::iOS, &db_path, None);

        // Custom attachment roots do not apply to iOS backups, even when the stored filename
        // resembles a macOS-style attachment path.
        assert_eq!(
            attachment.resolved_attachment_path(&Platform::iOS, &db_path, Some("/custom/root")),
            expected
        );
    }

    #[test]
    fn can_get_resolved_path_ios_smsdb() {
        let db_path = PathBuf::from("fake_root");
        let mut attachment = sample_attachment();
        attachment.filename = Some(format!("{DEFAULT_SMS_ROOT}/Attachments/a/b/c.png"));

        assert_eq!(
            attachment.resolved_attachment_path(
                // A jailbroken iOS sms.db uses `Platform::macOS` conventions, not `Platform::iOS`,
                // since the attachments are stored in direct filesystem paths, not SHA-1 hashed backup paths
                &Platform::macOS,
                &db_path,
                Some("/custom/path"),
            ),
            Some("/custom/path/Attachments/a/b/c.png".to_string())
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
