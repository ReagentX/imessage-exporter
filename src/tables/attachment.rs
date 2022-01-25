use rusqlite::{Connection, Result, Row, Statement};

use crate::{
    tables::table::{Diagnostic, Table, ATTACHMENT},
    util::output::processing,
};

#[derive(Debug)]
pub struct Attachment {
    pub rowid: i32,
    pub guid: String,
    pub created_date: i32,
    pub start_date: i32,
    pub filename: Option<String>,
    pub uti: Option<String>,
    pub mime_type: Option<String>,
    pub transfer_state: i32,
    pub is_outgoing: i32,
    pub user_info: Option<Vec<u8>>,
    pub transfer_name: String,
    pub total_bytes: i32,
    pub is_sticker: i32,
    pub sticker_user_info: Option<Vec<u8>>,
    pub attribution_info: Option<Vec<u8>>,
    pub hide_attachment: i32,
    pub ck_sync_state: i32,
    pub ck_server_change_token_blob: Option<Vec<u8>>,
    pub ck_record_id: Option<String>,
    pub original_guid: String,
    pub sr_ck_record_id: Option<String>,
    pub sr_ck_sync_state: i32,
    pub sr_ck_server_change_token_blob: Option<Vec<u8>>,
    pub is_commsafety_sensitive: i32,
}

impl Table for Attachment {
    fn from_row(row: &Row) -> Result<Attachment> {
        Ok(Attachment {
            rowid: row.get(0)?,
            guid: row.get(1)?,
            created_date: row.get(2)?,
            start_date: row.get(3)?,
            filename: row.get(4)?,
            uti: row.get(5)?,
            mime_type: row.get(6)?,
            transfer_state: row.get(7)?,
            is_outgoing: row.get(8)?,
            user_info: row.get(9)?,
            transfer_name: row.get(10)?,
            total_bytes: row.get(11)?,
            is_sticker: row.get(12)?,
            sticker_user_info: row.get(13)?,
            attribution_info: row.get(14)?,
            hide_attachment: row.get(15)?,
            ck_sync_state: row.get(16)?,
            // This default is needed becuase ck_server_change_token_blob can sometimes be a String
            ck_server_change_token_blob: row.get(17).unwrap_or(None),
            ck_record_id: row.get(18)?,
            original_guid: row.get(19)?,
            sr_ck_record_id: row.get(20)?,
            sr_ck_sync_state: row.get(21)?,
            // This default is needed becuase sr_ck_server_change_token_blob can sometimes be a String
            sr_ck_server_change_token_blob: row.get(22).unwrap_or(None),
            is_commsafety_sensitive: row.get(23)?,
        })
    }

    fn get(db: &Connection) -> Statement {
        db.prepare(&format!("SELECT * from {}", ATTACHMENT))
            .unwrap()
    }
}

impl Diagnostic for Attachment {
    // TODO: make diagnostic methods/traits for issues like this!
    // TODO: Diagnostic subcommand like this!
    fn run_diagnostic(db: &Connection) {
        processing();
        let mut statement_ck = db
            .prepare(&format!(
                "SELECT count(rowid) FROM {} WHERE typeof(ck_server_change_token_blob) == 'text'",
                ATTACHMENT
            ))
            .unwrap();
        let num_blank_ck: Option<i32> = statement_ck.query_row([], |r| r.get(0)).unwrap_or(None);

        let mut statement_sr = db
            .prepare(&format!(
                "SELECT count(rowid) FROM {} WHERE typeof(sr_ck_server_change_token_blob) == 'text'", 
                ATTACHMENT
            ))
            .unwrap();
        let num_blank_sr: Option<i32> = statement_sr.query_row([], |r| r.get(0)).unwrap_or(None);

        if num_blank_ck.is_some() || num_blank_sr.is_some() {
            println!("\rMissing attachment data:");
        }
        if let Some(ck) = num_blank_ck {
            println!("    ck_server_change_token_blob: {ck:?}");
        }
        if let Some(sr) = num_blank_sr {
            println!("    ck_server_change_token_blob: {sr:?}");
        }
    }
}
