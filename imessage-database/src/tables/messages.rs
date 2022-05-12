use std::{collections::HashMap, vec};

use chrono::{naive::NaiveDateTime, offset::Local, DateTime, Datelike, TimeZone, Timelike, Utc};
use rusqlite::{Connection, Error, Result, Row, Statement};

use crate::{
    tables::table::{
        Cacheable, Diagnostic, Table, CHAT_MESSAGE_JOIN, MESSAGE, MESSAGE_ATTACHMENT_JOIN,
    },
    util::output::processing,
    ApplePay, Reaction, Variant,
};

const ATTACHMENT_CHAR: char = '\u{FFFC}';
const APP_CHAR: char = '\u{FFFD}';
const REPLACEMENT_CHARS: [char; 2] = [ATTACHMENT_CHAR, APP_CHAR];

#[derive(Debug)]
pub enum MessageType {
    /// A normal message not associated with any others
    Normal(Variant),
    /// A message that has replies
    Thread(Variant),
    /// A message that is a reply to another message
    Reply(Variant),
}

#[derive(Debug, PartialEq)]
pub enum BubbleType<'a> {
    /// A normal text message
    Text(&'a str),
    /// An attachment
    Attachment,
    /// An app integration
    App,
}

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
    pub num_attachments: i32,
    pub num_replies: i32,
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
            num_attachments: row.get(79)?,
            num_replies: row.get(80)?,
            // TODO: Calculate once, not for each object
            offset: Utc.ymd(2001, 1, 1).and_hms(0, 0, 0).timestamp(),
        })
    }

    fn get(db: &Connection) -> Statement {
        db.prepare(&format!(
            "SELECT 
                 m.*, 
                 c.chat_id, 
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
             FROM 
                 message as m 
                 LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id 
             ORDER BY 
                 m.ROWID;
            "
        ))
        .unwrap()
    }

    fn extract(message: Result<Result<Self, Error>, Error>) -> Self {
        match message {
            Ok(message) => match message {
                Ok(msg) => msg,
                // TODO: When does this occur?
                Err(why) => panic!("Inner error: {}", why),
            },
            // TODO: When does this occur?
            Err(why) => panic!("Outer error: {}", why),
        }
    }
}

impl Diagnostic for Message {
    /// Emit diagnotsic data for the Messages table
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Diagnostic, get_connection};
    /// use imessage_database::tables::messages::Message;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path);
    /// Message::run_diagnostic(&conn);
    /// ```
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

impl Cacheable for Message {
    type K = String;
    type V = Vec<String>;
    /// Used for reactions that do not exist in a foreign key table
    fn cache(db: &Connection) -> std::collections::HashMap<Self::K, Self::V> {
        // Create cache for user IDs
        let mut map: HashMap<Self::K, Self::V> = HashMap::new();

        // Create query
        let mut statement = db.prepare(&format!(
            "SELECT 
                 m.*, 
                 c.chat_id, 
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
             FROM 
                 message as m 
                 LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
             WHERE m.associated_message_guid NOT NULL
            "
        ))
        .unwrap();

        // Execute query to build the Handles
        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .unwrap();

        // Iterate over the messages and update the map
        for reaction in messages {
            let reaction = Self::extract(reaction);
            if let Variant::Reaction(..) = reaction.variant() {
                match reaction.clean_associated_guid() {
                    Some((_, reaction_target_guid)) => match map.get_mut(reaction_target_guid) {
                        Some(reactions) => {
                            reactions.push(reaction.guid);
                        }
                        None => {
                            map.insert(reaction_target_guid.to_string(), vec![reaction.guid]);
                        }
                    },
                    None => (),
                }
            }
        }
        map
    }
}

impl Message {
    /// Get a vector of string slices of the message's components
    ///
    /// If the message has attachments, there will be one [`U+FFFC`]((https://www.fileformat.info/info/unicode/char/fffc/index.htm)) character
    /// for each attachment and one [`U+FFFD`](https://www.fileformat.info/info/unicode/char/fffd/index.htm) for app messages that we need
    /// to format.
    pub fn body(&self) -> Vec<BubbleType> {
        match &self.text {
            // Attachment: "\u{FFFC}"
            // Replacement: "\u{FFFD}"
            Some(text) => {
                let mut out_v = vec![];
                let mut start: usize = 0;
                let mut end: usize = 0;
                for (idx, char) in text.char_indices() {
                    if REPLACEMENT_CHARS.contains(&char) {
                        if start < end {
                            out_v.push(BubbleType::Text(text[start..idx].trim()));
                        }
                        start = idx + 1;
                        end = idx;
                        match char {
                            ATTACHMENT_CHAR => out_v.push(BubbleType::Attachment),
                            APP_CHAR => out_v.push(BubbleType::App),
                            _ => {}
                        };
                    } else {
                        if start > end {
                            start = idx;
                        }
                        end = idx;
                    }
                }
                if start < end && start < text.len() {
                    out_v.push(BubbleType::Text(text[start..].trim()));
                }
                out_v
            }
            None => vec![],
        }
    }

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

    pub fn time_until_read(&self) -> Option<String> {
        // TODO: Does this work?
        if self.date_delivered != 0 && self.date_read != 0 {
            let duration = self.date_read() - self.date();
            return Some(format!("{}", duration.num_minutes()));
        }
        None
    }

    pub fn is_reply(&self) -> bool {
        self.thread_originator_guid.is_some()
    }

    fn has_attachments(&self) -> bool {
        self.num_attachments > 0
    }

    fn has_replies(&self) -> bool {
        self.num_replies > 0
    }

    /// Get the index of the part of a message a reply is pointing to
    pub fn get_reply_index(&self) -> usize {
        if let Some(parts) = &self.thread_originator_part {
            return match parts.split(':').next() {
                Some(part) => str::parse::<usize>(part).unwrap(),
                None => 0,
            };
        }
        0
    }

    fn clean_associated_guid(&self) -> Option<(usize, &str)> {
        // TODO: Test that the GUID length is correct!
        if let Some(guid) = &self.associated_message_guid {
            if guid.starts_with("p:") {
                let mut split = guid.split('/');
                let index_str = split.next();
                let message_id = split.next();
                let index = str::parse::<usize>(&index_str.unwrap().replace("p:", "")).unwrap_or(0);
                return Some((index, message_id.unwrap()));
            } else if guid.starts_with("bp:") {
                return Some((0, &guid[3..guid.len()]));
            } else {
                return Some((0, guid.as_str()));
            }
        }
        None
    }

    /// Parse the index of a reaction from it's associated GUID field
    fn reaction_index(&self) -> usize {
        match self.clean_associated_guid() {
            Some((x, _)) => x,
            None => 0,
        }
    }

    /// Build a HashMap of message component index to messages that react to that component
    pub fn get_reactions<'a>(
        &self,
        db: &Connection,
        reactions: &'a HashMap<String, Vec<String>>,
    ) -> HashMap<usize, Vec<Self>> {
        let mut out_h: HashMap<usize, Vec<Self>> = HashMap::new();
        if let Some(rxs) = reactions.get(&self.guid) {
            let filter: Vec<String> = rxs.iter().map(|guid| format!("\"{}\"", guid)).collect();
            // Create query
            let mut statement = db.prepare(&format!(
                "SELECT 
                        m.*, 
                        c.chat_id, 
                        (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                        (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
                    FROM 
                        message as m 
                        LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
                    WHERE m.guid IN ({})
                    ORDER BY 
                        m.ROWID;
                    ",
                filter.join(",")
            )).unwrap();

            // Execute query to build the Handles
            let messages = statement
                .query_map([], |row| Ok(Message::from_row(row)))
                .unwrap();

            for message in messages {
                let msg = Message::extract(message);
                if let Variant::Reaction(idx, _, _) = msg.variant() {
                    match out_h.get_mut(&idx) {
                        Some(body_part) => body_part.push(msg),
                        None => {
                            out_h.insert(idx, vec![msg]);
                        }
                    }
                }
            }
        }
        out_h
    }

    /// Build a HashMap of message component index to messages that reply to that component
    pub fn get_replies(&self, db: &Connection) -> HashMap<usize, Vec<Self>> {
        let mut out_h: HashMap<usize, Vec<Self>> = HashMap::new();

        // No need to hit the DB if we know we don't have replies
        if self.has_replies() {
            let mut statement = db.prepare(&format!(
                "SELECT 
                     m.*, 
                     c.chat_id, 
                     (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                     (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
                 FROM 
                     message as m 
                     LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id 
                 WHERE m.thread_originator_guid = \"{}\"
                 ORDER BY 
                     m.ROWID;
                ", self.guid
            ))
            .unwrap();

            let iter = statement
                .query_map([], |row| Ok(Message::from_row(row)))
                .unwrap();

            for message in iter {
                let m = Message::extract(message);
                let idx = m.get_reply_index();
                match out_h.get_mut(&idx) {
                    Some(body_part) => body_part.push(m),
                    None => {
                        out_h.insert(idx, vec![m]);
                    }
                }
            }
        }

        out_h
    }

    pub fn variant(&self) -> Variant {
        match self.associated_message_type {
            // Normal message
            0 => Variant::Normal,

            // Apple Pay
            2 => Variant::ApplePay(ApplePay::Send(self.text.as_ref().unwrap().to_owned())),
            3 => Variant::ApplePay(ApplePay::Recieve(self.text.as_ref().unwrap().to_owned())),

            // Reactions
            2000 => Variant::Reaction(self.reaction_index(), true, Reaction::Loved),
            2001 => Variant::Reaction(self.reaction_index(), true, Reaction::Liked),
            2002 => Variant::Reaction(self.reaction_index(), true, Reaction::Disliked),
            2003 => Variant::Reaction(self.reaction_index(), true, Reaction::Laughed),
            2004 => Variant::Reaction(self.reaction_index(), true, Reaction::Emphasized),
            2005 => Variant::Reaction(self.reaction_index(), true, Reaction::Questioned),
            3000 => Variant::Reaction(self.reaction_index(), false, Reaction::Loved),
            3001 => Variant::Reaction(self.reaction_index(), false, Reaction::Liked),
            3002 => Variant::Reaction(self.reaction_index(), false, Reaction::Disliked),
            3003 => Variant::Reaction(self.reaction_index(), false, Reaction::Laughed),
            3004 => Variant::Reaction(self.reaction_index(), false, Reaction::Emphasized),
            3005 => Variant::Reaction(self.reaction_index(), false, Reaction::Questioned),

            // Unknown
            x => Variant::Unknown(x),
        }
    }

    pub fn case(&self) -> MessageType {
        if self.is_reply() {
            MessageType::Reply(self.variant())
        } else if self.has_replies() {
            MessageType::Thread(self.variant())
        } else {
            MessageType::Normal(self.variant())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tables::messages::{BubbleType, Message};

    fn blank() -> Message {
        Message {
            rowid: i32::default(),
            guid: String::default(),
            text: None,
            replace: i32::default(),
            service_center: None,
            handle_id: i32::default(),
            subject: None,
            country: None,
            attributedBody: None,
            version: i32::default(),
            r#type: i32::default(),
            service: String::default(),
            account: None,
            account_guid: None,
            error: i32::default(),
            date: i64::default(),
            date_read: i64::default(),
            date_delivered: i64::default(),
            is_delivered: false,
            is_finished: false,
            is_emote: false,
            is_from_me: false,
            is_empty: false,
            is_delayed: false,
            is_auto_reply: false,
            is_prepared: false,
            is_read: false,
            is_system_message: false,
            is_sent: false,
            has_dd_results: i32::default(),
            is_service_message: false,
            is_forward: false,
            was_downgraded: i32::default(),
            is_archive: false,
            cache_has_attachments: i32::default(),
            cache_roomnames: None,
            was_data_detected: i32::default(),
            was_deduplicated: i32::default(),
            is_audio_message: false,
            is_played: false,
            date_played: i64::default(),
            item_type: i32::default(),
            other_handle: i32::default(),
            group_title: None,
            group_action_type: i32::default(),
            share_status: i32::default(),
            share_direction: i32::default(),
            is_expirable: false,
            expire_state: i32::default(),
            message_action_type: i32::default(),
            message_source: i32::default(),
            associated_message_guid: None,
            balloon_bundle_id: None,
            payload_data: None,
            associated_message_type: i32::default(),
            expressive_send_style_id: None,
            associated_message_range_location: i32::default(),
            associated_message_range_length: i32::default(),
            time_expressive_send_played: i64::default(),
            message_summary_info: None,
            ck_sync_state: i32::default(),
            ck_record_id: None,
            ck_record_change_tag: None,
            destination_caller_id: None,
            sr_ck_sync_state: i32::default(),
            sr_ck_record_id: None,
            sr_ck_record_change_tag: None,
            is_corrupt: false,
            reply_to_guid: None,
            sort_id: i32::default(),
            is_spam: false,
            has_unseen_mention: i32::default(),
            thread_originator_guid: None,
            thread_originator_part: None,
            syndication_ranges: None,
            was_delivered_quietly: i32::default(),
            did_notify_recipient: i32::default(),
            synced_syndication_ranges: None,
            chat_id: None,
            num_attachments: 0,
            num_replies: 0,
            offset: 0,
        }
    }

    #[test]
    fn can_gen_message() {
        let m = blank();
    }

    #[test]
    fn can_get_message_body_text_only() {
        let mut m = blank();
        m.text = Some("Hello world".to_string());
        assert_eq!(m.body(), vec![BubbleType::Text("Hello world")]);
    }

    #[test]
    fn can_get_message_body_attachment_text() {
        let mut m = blank();
        m.text = Some("\u{FFFC}Hello world".to_string());
        assert_eq!(
            m.body(),
            vec![BubbleType::Attachment, BubbleType::Text("Hello world")]
        );
    }

    #[test]
    fn can_get_message_body_app_text() {
        let mut m = blank();
        m.text = Some("\u{FFFD}Hello world".to_string());
        assert_eq!(
            m.body(),
            vec![BubbleType::App, BubbleType::Text("Hello world")]
        );
    }

    #[test]
    fn can_get_message_body_app_attachment_text_mixed_start_text() {
        let mut m = blank();
        m.text = Some("One\u{FFFD}\u{FFFC}Two\u{FFFC}Three\u{FFFC}four".to_string());
        assert_eq!(
            m.body(),
            vec![
                BubbleType::Text("One"),
                BubbleType::App,
                BubbleType::Attachment,
                BubbleType::Text("Two"),
                BubbleType::Attachment,
                BubbleType::Text("Three"),
                BubbleType::Attachment,
                BubbleType::Text("four")
            ]
        );
    }

    #[test]
    fn can_get_message_body_app_attachment_text_mixed_start_app() {
        let mut m = blank();
        m.text = Some("\u{FFFD}\u{FFFC}Two\u{FFFC}Three\u{FFFC}".to_string());
        assert_eq!(
            m.body(),
            vec![
                BubbleType::App,
                BubbleType::Attachment,
                BubbleType::Text("Two"),
                BubbleType::Attachment,
                BubbleType::Text("Three"),
                BubbleType::Attachment
            ]
        );
    }
}
