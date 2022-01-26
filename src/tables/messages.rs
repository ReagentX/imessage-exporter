use chrono::{naive::NaiveDateTime, offset::Local, DateTime, Datelike, TimeZone, Timelike, Utc};
use rusqlite::{Connection, Result, Row, Statement};

use crate::{
    tables::table::{Diagnostic, Table, CHAT_MESSAGE_JOIN, MESSAGE, MESSAGE_ATTACHMENT_JOIN},
    util::output::processing,
};

#[derive(Debug)]
#[allow(non_snake_case)]
pub struct Message {
    pub rowid: i32,
    pub guid: String,
    pub text: Option<String>,
    pub replace: i32,
    pub service_center: Option<String>,
    pub handle_id: i32,
    pub subject: Option<String>,
    pub country: Option<String>,
    pub attributedBody: Option<Vec<u8>>, // Field name comes from from table
    pub version: i32,
    pub r#type: i32, // Field name comes from from table
    pub service: String,
    pub account: Option<String>,
    pub account_guid: Option<String>,
    pub error: i32,
    pub date: i64,
    pub date_read: i64,
    pub date_delivered: i64,
    pub is_delivered: bool,
    pub is_finished: bool,
    pub is_emote: bool,
    pub is_from_me: bool,
    pub is_empty: bool,
    pub is_delayed: bool,
    pub is_auto_reply: bool,
    pub is_prepared: bool,
    pub is_read: bool,
    pub is_system_message: bool,
    pub is_sent: bool,
    pub has_dd_results: i32,
    pub is_service_message: bool,
    pub is_forward: bool,
    pub was_downgraded: i32,
    pub is_archive: bool,
    pub cache_has_attachments: i32,
    pub cache_roomnames: Option<String>,
    pub was_data_detected: i32,
    pub was_deduplicated: i32,
    pub is_audio_message: bool,
    pub is_played: bool,
    pub date_played: i64,
    pub item_type: i32,
    pub other_handle: i32,
    pub group_title: Option<String>,
    pub group_action_type: i32,
    pub share_status: i32,
    pub share_direction: i32,
    pub is_expirable: bool,
    pub expire_state: i32,
    pub message_action_type: i32,
    pub message_source: i32,
    pub associated_message_guid: Option<String>,
    pub balloon_bundle_id: Option<String>,
    pub payload_data: Option<Vec<u8>>,
    pub associated_message_type: i32,
    pub expressive_send_style_id: Option<String>,
    pub associated_message_range_location: i32,
    pub associated_message_range_length: i32,
    pub time_expressive_send_played: i64,
    pub message_summary_info: Option<Vec<u8>>,
    pub ck_sync_state: i32,
    pub ck_record_id: Option<String>,
    pub ck_record_change_tag: Option<String>,
    pub destination_caller_id: Option<String>,
    pub sr_ck_sync_state: i32,
    pub sr_ck_record_id: Option<String>,
    pub sr_ck_record_change_tag: Option<String>,
    pub is_corrupt: bool,
    pub reply_to_guid: Option<String>,
    pub sort_id: i32,
    pub is_spam: bool,
    pub has_unseen_mention: i32,
    pub thread_originator_guid: Option<String>,
    pub thread_originator_part: Option<String>,
    pub syndication_ranges: Option<String>,
    pub was_delivered_quietly: i32,
    pub did_notify_recipient: i32,
    pub synced_syndication_ranges: Option<String>,
    pub chat_id: Option<i32>,
    pub attachment_id: Option<i32>,
    offset: i64,
}

impl Table for Message {
    fn from_row(row: &Row) -> Result<Message> {
        Ok(Message {
            rowid: row.get(0)?,
            guid: row.get(1)?,
            text: row.get(2)?,
            replace: row.get(3)?,
            service_center: row.get(4)?,
            handle_id: row.get(5)?,
            subject: row.get(6)?,
            country: row.get(7)?,
            attributedBody: row.get(8)?,
            version: row.get(9)?,
            r#type: row.get(10)?,
            service: row.get(11)?,
            account: row.get(12)?,
            account_guid: row.get(13)?,
            error: row.get(14)?,
            date: row.get(15)?,
            date_read: row.get(16)?,
            date_delivered: row.get(17)?,
            is_delivered: row.get(18)?,
            is_finished: row.get(19)?,
            is_emote: row.get(20)?,
            is_from_me: row.get(21)?,
            is_empty: row.get(22)?,
            is_delayed: row.get(23)?,
            is_auto_reply: row.get(24)?,
            is_prepared: row.get(25)?,
            is_read: row.get(26)?,
            is_system_message: row.get(27)?,
            is_sent: row.get(28)?,
            has_dd_results: row.get(29)?,
            is_service_message: row.get(30)?,
            is_forward: row.get(31)?,
            was_downgraded: row.get(32)?,
            is_archive: row.get(33)?,
            cache_has_attachments: row.get(34)?,
            cache_roomnames: row.get(35)?,
            was_data_detected: row.get(36)?,
            was_deduplicated: row.get(37)?,
            is_audio_message: row.get(38)?,
            is_played: row.get(39)?,
            date_played: row.get(40)?,
            item_type: row.get(41)?,
            other_handle: row.get(42)?,
            group_title: row.get(43)?,
            group_action_type: row.get(44)?,
            share_status: row.get(45)?,
            share_direction: row.get(46)?,
            is_expirable: row.get(47)?,
            expire_state: row.get(48)?,
            message_action_type: row.get(49)?,
            message_source: row.get(50)?,
            associated_message_guid: row.get(51)?,
            balloon_bundle_id: row.get(52)?,
            payload_data: row.get(53)?,
            associated_message_type: row.get(54)?,
            expressive_send_style_id: row.get(55)?,
            associated_message_range_location: row.get(56)?,
            associated_message_range_length: row.get(57)?,
            time_expressive_send_played: row.get(58)?,
            message_summary_info: row.get(59)?,
            ck_sync_state: row.get(60)?,
            ck_record_id: row.get(61)?,
            ck_record_change_tag: row.get(62)?,
            destination_caller_id: row.get(63)?,
            sr_ck_sync_state: row.get(64)?,
            sr_ck_record_id: row.get(65)?,
            sr_ck_record_change_tag: row.get(66)?,
            is_corrupt: row.get(67)?,
            reply_to_guid: row.get(68)?,
            sort_id: row.get(69)?,
            is_spam: row.get(70)?,
            has_unseen_mention: row.get(71)?,
            thread_originator_guid: row.get(72)?,
            thread_originator_part: row.get(73)?,
            syndication_ranges: row.get(74)?,
            was_delivered_quietly: row.get(75)?,
            did_notify_recipient: row.get(76)?,
            synced_syndication_ranges: row.get(77)?,
            chat_id: row.get(78)?,
            attachment_id: row.get(79)?,
            offset: Utc.ymd(2001, 1, 1).and_hms(0, 0, 0).timestamp(),
        })
    }

    fn get(db: &Connection) -> Statement {
        // TODO: use conversation table to sort messages to their respective chats
        // TODO: FYI, Group chats set the handle to 0 for the sender (i.e., "you")
        db.prepare(&format!(
            "SELECT m.*, c.chat_id, a.attachment_id
            FROM {MESSAGE} as m
            LEFT JOIN {CHAT_MESSAGE_JOIN} as c
            ON m.rowid = c.message_id
            LEFT JOIN {MESSAGE_ATTACHMENT_JOIN} as a
            ON m.rowid = a.message_id
            ORDER BY m.ROWID
            LIMIT 10;",
        ))
        .unwrap()
    }
}

impl Diagnostic for Message {
    fn run_diagnostic(db: &Connection) {
        processing();
        let mut messages_without_chat = db
            .prepare(&format!("SELECT COUNT(m.rowid) from {MESSAGE} as m LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.rowid = c.message_id WHERE c.chat_id is NULL ORDER BY m.ROWID"))
            .unwrap();

        let num_dangling: Option<i32> = messages_without_chat
            .query_row([], |r| r.get(0))
            .unwrap_or(None);

        if let Some(dangling) = num_dangling {
            println!("\rMessages not associated with a chat: {dangling}");
        }
    }
}

impl Message {
    fn get_local_time(&self, date_stamp: &i64) -> DateTime<Local> {
        let utc_stamp = NaiveDateTime::from_timestamp((date_stamp / 1000000000) + self.offset, 0);
        let local_time = Local.from_utc_datetime(&utc_stamp);
        Local
            .ymd(local_time.year(), local_time.month(), local_time.day())
            .and_hms(local_time.hour(), local_time.minute(), local_time.second())
    }

    pub fn date(&self) -> DateTime<Local> {
        self.get_local_time(&self.date)
    }

    pub fn date_delivered(&self) -> DateTime<Local> {
        self.get_local_time(&self.date_delivered)
    }

    pub fn date_read(&self) -> DateTime<Local> {
        self.get_local_time(&self.date_read)
    }
}
