/*!
 This module represents common (but not all) columns in the `attachment` table.
*/

use rusqlite::{Connection, Error, Error as E, Result, Row, Statement};
use std::path::Path;

use crate::{
    tables::table::{Diagnostic, Table, ATTACHMENT},
    util::{
        dirs::home,
        output::{done_processing, processing},
    },
    Message,
};

const COLUMNS: &str = "a.ROWID, a.filename, a.mime_type, a.transfer_name, a.total_bytes, a.is_sticker, a.attribution_info, a.hide_attachment";

#[derive(Debug)]
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
    pub mime_type: Option<String>,
    pub transfer_name: String,
    pub total_bytes: i32,
    pub is_sticker: i32,
    pub attribution_info: Option<Vec<u8>>,
    pub hide_attachment: i32,
    pub copied_path: Option<String>,
}

impl Table for Attachment {
    fn from_row(row: &Row) -> Result<Attachment> {
        Ok(Attachment {
            rowid: row.get(0)?,
            filename: row.get(1)?,
            mime_type: row.get(2)?,
            transfer_name: row.get(3)?,
            total_bytes: row.get(4)?,
            is_sticker: row.get(5)?,
            attribution_info: row.get(6)?,
            hide_attachment: row.get(7)?,
            copied_path: None,
        })
    }

    fn get(db: &Connection) -> Statement {
        db.prepare(&format!("SELECT * from {}", ATTACHMENT))
            .unwrap()
    }

    fn extract(attachment: Result<Result<Self, Error>, Error>) -> Self {
        match attachment {
            Ok(attachment) => match attachment {
                Ok(att) => att,
                // TODO: When does this occur?
                Err(why) => panic!("Inner error: {}", why),
            },
            // TODO: When does this occur?
            Err(why) => panic!("Outer error: {}", why),
        }
    }
}

impl Diagnostic for Attachment {
    /// Emit diagnotsic data for the Attachments table
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
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Diagnostic, get_connection};
    /// use imessage_database::tables::attachment::Attachment;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path);
    /// Attachment::run_diagnostic(&conn);
    /// ```
    fn run_diagnostic(db: &Connection) {
        processing();
        let mut statement_ck = db
            .prepare(&format!(
                "SELECT count(rowid) FROM {ATTACHMENT} WHERE typeof(ck_server_change_token_blob) == 'text'"
            ))
            .unwrap();
        let num_blank_ck: i32 = statement_ck.query_row([], |r| r.get(0)).unwrap_or(0);

        let mut statement_sr = db
            .prepare(&format!("SELECT filename FROM {ATTACHMENT}"))
            .unwrap();
        let paths = statement_sr.query_map([], |r| Ok(r.get(0))).unwrap();

        let home = home();
        let missing_files = paths
            .filter_map(Result::ok)
            .filter(|path: &Result<String, E>| {
                if let Ok(path) = path {
                    !Path::new(&path.replace('~', &home)).exists()
                } else {
                    false
                }
            })
            .count();

        if num_blank_ck > 0 || missing_files > 0 {
            println!("Missing attachment data:");
        } else {
            done_processing();
        }
        if missing_files > 0 {
            println!("    Missing files: {missing_files:?}");
        }
        if num_blank_ck > 0 {
            println!("    ck_server_change_token_blob: {num_blank_ck:?}");
        }
    }
}

impl Attachment {
    pub fn from_message(db: &Connection, msg: &Message) -> Vec<Attachment> {
        let mut out_l = vec![];
        if msg.has_attachments() {
            let mut statement = db
                .prepare(&format!(
                    "
                    SELECT {COLUMNS} FROM message_attachment_join j 
                        LEFT JOIN attachment AS a ON j.attachment_id = a.ROWID
                    WHERE j.message_id = {}
                    ",
                    msg.rowid
                ))
                .unwrap();

            let iter = statement
                .query_map([], |row| Ok(Attachment::from_row(row)))
                .unwrap();

            for attachment in iter {
                let m = Attachment::extract(attachment);
                out_l.push(m)
            }
        }
        out_l
    }

    /// Get the media type of an attachment
    pub fn mime_type<'a>(&'a self) -> MediaType<'a> {
        match &self.mime_type {
            Some(mime) => {
                if let Some(mime_str) = mime.split('/').into_iter().next() {
                    match mime_str {
                        "image" => MediaType::Image(&mime),
                        "video" => MediaType::Video(&mime),
                        "audio" => MediaType::Audio(&mime),
                        "text" => MediaType::Text(&mime),
                        "application" => MediaType::Application(&mime),
                        _ => MediaType::Other(&mime),
                    }
                } else {
                    MediaType::Other(&mime)
                }
            }
            None => MediaType::Unknown,
        }
    }

    /// Get the path to an attachment, if it exists
    pub fn path(&self) -> Option<&Path> {
        match &self.filename {
            Some(name) => Some(&Path::new(name)),
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
}
