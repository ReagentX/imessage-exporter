/*!
 This module represents common (but not all) columns in the `message` table.
*/

use std::{collections::HashMap, vec};

use chrono::{naive::NaiveDateTime, offset::Local, DateTime, Datelike, TimeZone, Timelike};
use rusqlite::{Connection, Error, Result, Row, Statement};

use crate::{
    tables::table::{
        Cacheable, Diagnostic, Table, CHAT_MESSAGE_JOIN, MESSAGE, MESSAGE_ATTACHMENT_JOIN,
    },
    util::{dates::readable_diff, output::processing},
    ApplePay, BubbleEffect, Expressive, Reaction, ScreenEffect, Variant,
};

const ATTACHMENT_CHAR: char = '\u{FFFC}';
const APP_CHAR: char = '\u{FFFD}';
const REPLACEMENT_CHARS: [char; 2] = [ATTACHMENT_CHAR, APP_CHAR];
const COLUMNS: &str = "m.rowid, m.guid, m.text, m.handle_id, m.subject, m.date, m.date_read, m.date_delivered, m.is_from_me, m.is_read, m.associated_message_guid, m.associated_message_type, m.expressive_send_style_id, m.thread_originator_guid, m.thread_originator_part";

/// Represents a broad category of messages: standalone, thread originators, and thread replies.
#[derive(Debug)]
pub enum MessageType<'a> {
    /// A normal message not associated with any others
    Normal(Variant, Expressive<'a>),
    /// A message that has replies
    Thread(Variant, Expressive<'a>),
    /// A message that is a reply to another message
    Reply(Variant, Expressive<'a>),
}

/// Defines the parts of a message bubble, i.e. the content that can exist in a single message.
#[derive(Debug, PartialEq)]
pub enum BubbleType<'a> {
    /// A normal text message
    Text(&'a str),
    /// An attachment
    Attachment,
    /// An app integration
    App,
}

/// Represents a single row in the `message` table.
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct Message {
    pub rowid: i32,
    pub guid: String,
    pub text: Option<String>,
    pub handle_id: i32,
    pub subject: Option<String>,
    pub date: i64,
    pub date_read: i64,
    pub date_delivered: i64,
    pub is_from_me: bool,
    pub is_read: bool,
    pub associated_message_guid: Option<String>,
    pub associated_message_type: i32,
    pub expressive_send_style_id: Option<String>,
    pub thread_originator_guid: Option<String>,
    pub thread_originator_part: Option<String>,
    pub chat_id: Option<i32>,
    pub num_attachments: i32,
    pub num_replies: i32,
}

impl Table for Message {
    fn from_row(row: &Row) -> Result<Message> {
        Ok(Message {
            rowid: row.get(0)?,
            guid: row.get(1)?,
            text: row.get(2)?,
            handle_id: row.get(3)?,
            subject: row.get(4)?,
            date: row.get(5)?,
            date_read: row.get(6)?,
            date_delivered: row.get(7)?,
            is_from_me: row.get(8)?,
            is_read: row.get(9)?,
            associated_message_guid: row.get(10)?,
            associated_message_type: row.get(11)?,
            expressive_send_style_id: row.get(12)?,
            thread_originator_guid: row.get(13)?,
            thread_originator_part: row.get(14)?,
            chat_id: row.get(15)?,
            num_attachments: row.get(16)?,
            num_replies: row.get(17)?,
        })
    }

    fn get(db: &Connection) -> Statement {
        db.prepare(&format!(
            "SELECT 
                 {COLUMNS},
                 c.chat_id, 
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
             FROM 
                 message as m 
                 LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id 
             ORDER BY 
                 m.date;
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
            .prepare(&format!(
                "
            SELECT
                COUNT(m.rowid)
            FROM
            {MESSAGE} as m
                LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.rowid = c.message_id
            WHERE
                c.chat_id is NULL
            ORDER BY
                m.date
            "
            ))
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
                 {COLUMNS}, 
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
            if reaction.is_reaction() {
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
                if start <= end && start < text.len() {
                    out_v.push(BubbleType::Text(text[start..].trim()));
                }
                out_v
            }
            None => vec![],
        }
    }

    fn get_local_time(&self, date_stamp: &i64, offset: &i64) -> DateTime<Local> {
        let utc_stamp = NaiveDateTime::from_timestamp((date_stamp / 1000000000) + offset, 0);
        let local_time = Local.from_utc_datetime(&utc_stamp);
        Local
            .ymd(local_time.year(), local_time.month(), local_time.day())
            .and_hms(local_time.hour(), local_time.minute(), local_time.second())
    }

    /// Calculates the date a message was written to the database.
    ///
    /// This field is stored as a unix timestamp with an epoch of `1/1/2001 00:00:00` in the local time zone
    pub fn date(&self, offset: &i64) -> DateTime<Local> {
        self.get_local_time(&self.date, offset)
    }

    /// Calculates the date a message was marked as delivered.
    ///
    /// This field is stored as a unix timestamp with an epoch of `1/1/2001 00:00:00` in the local time zone
    pub fn date_delivered(&self, offset: &i64) -> DateTime<Local> {
        self.get_local_time(&self.date_delivered, offset)
    }

    /// Calculates the date a message was marked as read.
    ///
    /// This field is stored as a unix timestamp with an epoch of `1/1/2001 00:00:00` in the local time zone
    pub fn date_read(&self, offset: &i64) -> DateTime<Local> {
        self.get_local_time(&self.date_read, offset)
    }

    /// Gets the time until the message was read. This can happen in two ways:
    ///
    /// - You recieved a message, then waited to read it
    /// - You sent a message, and the recipient waited to read it
    ///
    /// In the former case, this subtracts the date read column (`date_read`) from the date recieved column (`date`).
    /// In the latter case, this subtracts the date delivered column (`date_delivered`) from the date recieved column (`date`).
    ///
    /// Not all messages get tagged with the read properties.
    /// If more than one message has been sent in a thread before getting read,
    /// only the most recent message will get the tag.
    pub fn time_until_read(&self, offset: &i64) -> Option<String> {
        // Message we recieved
        if !self.is_from_me && self.date_read != 0 && self.date != 0 {
            return readable_diff(self.date(offset), self.date_read(offset));
        }
        // Message we sent
        else if self.is_from_me && self.date_delivered != 0 && self.date != 0 {
            return readable_diff(self.date(offset), self.date_delivered(offset));
        }
        None
    }

    /// `true` if the message is a response to a thread, else `false`
    pub fn is_reply(&self) -> bool {
        self.thread_originator_guid.is_some()
    }

    /// `true` if the message is a reaction to another message, else `false`
    pub fn is_reaction(&self) -> bool {
        matches!(self.variant(), Variant::Reaction(..))
            | (self.is_sticker() && self.associated_message_guid.is_some())
    }

    /// `true` if the message is sticker, else `false`
    pub fn is_sticker(&self) -> bool {
        matches!(self.variant(), Variant::Sticker(_))
    }

    /// `true` if the message has an expressive presentation, else `false`
    pub fn is_expressive(&self) -> bool {
        self.expressive_send_style_id.is_some()
    }

    /// `true` if the message has attachments, else `false`
    pub fn has_attachments(&self) -> bool {
        self.num_attachments > 0
    }

    /// `true` if the message begins a thread, else `false`
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

    /// Build a HashMap of message component index to messages that react to that component
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
    /// Message::get_count(&conn);
    /// ```
    pub fn get_count(db: &Connection) -> u64 {
        let mut statement = db
            .prepare(&format!("SELECT COUNT(*) FROM {}", MESSAGE))
            .unwrap();
        // Execute query to build the Handles
        let count: u64 = statement.query_row([], |r| r.get(0)).unwrap_or(0);
        count
    }

    /// In some special cases, the `guid` is stored with some additional data we need to parse out. There are two prefixes:
    ///
    /// - `bp:` GUID prefix for bubble message reactions (links, apps, etc)
    /// - `p:0/` GUID prefix for normal messages (body text, attachments)
    ///   - for `p:#/`, the # is the message index, so if a message has 3 attachments:
    ///     - 0 is the first image
    ///     - 1 is the second image
    ///     - 2 is the third image
    ///     - 3 is the text of the message
    /// In this example, a Like on `p:2/` is a like on the second message
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
                        {COLUMNS}, 
                        c.chat_id, 
                        (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                        (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
                    FROM 
                        message as m 
                        LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
                    WHERE m.guid IN ({})
                    ORDER BY 
                        m.date;
                    ",
                filter.join(",")
            )).unwrap();

            // Execute query to build the Handles
            let messages = statement
                .query_map([], |row| Ok(Message::from_row(row)))
                .unwrap();

            for message in messages {
                let msg = Message::extract(message);
                if let Variant::Reaction(idx, _, _) | Variant::Sticker(idx) = msg.variant() {
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
                     {COLUMNS}, 
                     c.chat_id, 
                     (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                     (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
                 FROM 
                     message as m 
                     LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id 
                 WHERE m.thread_originator_guid = \"{}\"
                 ORDER BY 
                     m.date;
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

            // Stickers overlayed on messages
            1000 => Variant::Sticker(self.reaction_index()),

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
            MessageType::Reply(self.variant(), self.get_expressive())
        } else if self.has_replies() {
            MessageType::Thread(self.variant(), self.get_expressive())
        } else {
            MessageType::Normal(self.variant(), self.get_expressive())
        }
    }

    pub fn get_expressive(&self) -> Expressive {
        match &self.expressive_send_style_id {
            Some(content) => match content.as_str() {
                "com.apple.MobileSMS.expressivesend.gentle" => {
                    Expressive::Bubble(BubbleEffect::Gentle)
                }
                "com.apple.MobileSMS.expressivesend.impact" => {
                    Expressive::Bubble(BubbleEffect::Slam)
                }
                "com.apple.MobileSMS.expressivesend.invisibleink" => {
                    Expressive::Bubble(BubbleEffect::InvisibleInk)
                }
                "com.apple.MobileSMS.expressivesend.loud" => Expressive::Bubble(BubbleEffect::Loud),
                "com.apple.messages.effect.CKConfettiEffect" => {
                    Expressive::Screen(ScreenEffect::Confetti)
                }
                "com.apple.messages.effect.CKEchoEffect" => Expressive::Screen(ScreenEffect::Echo),
                "com.apple.messages.effect.CKFireworksEffect" => {
                    Expressive::Screen(ScreenEffect::Fireworks)
                }
                "com.apple.messages.effect.CKHappyBirthdayEffect" => {
                    Expressive::Screen(ScreenEffect::Balloons)
                }
                "com.apple.messages.effect.CKHeartEffect" => {
                    Expressive::Screen(ScreenEffect::Heart)
                }
                "com.apple.messages.effect.CKLasersEffect" => {
                    Expressive::Screen(ScreenEffect::Lasers)
                }
                "com.apple.messages.effect.CKShootingStarEffect" => {
                    Expressive::Screen(ScreenEffect::ShootingStar)
                }
                "com.apple.messages.effect.CKSparklesEffect" => {
                    Expressive::Screen(ScreenEffect::Sparkles)
                }
                "com.apple.messages.effect.CKSpotlightEffect" => {
                    Expressive::Screen(ScreenEffect::Spotlight)
                }
                _ => Expressive::Unknown(content),
            },
            None => Expressive::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        message_types::expressives,
        tables::messages::{BubbleType, Message},
        util::dates::get_offset,
    };

    fn blank() -> Message {
        Message {
            rowid: i32::default(),
            guid: String::default(),
            text: None,
            handle_id: i32::default(),
            subject: None,
            date: i64::default(),
            date_read: i64::default(),
            date_delivered: i64::default(),
            is_from_me: false,
            is_read: false,
            associated_message_guid: None,
            associated_message_type: i32::default(),
            expressive_send_style_id: None,
            thread_originator_guid: None,
            thread_originator_part: None,
            chat_id: None,
            num_attachments: 0,
            num_replies: 0,
        }
    }

    #[test]
    fn can_gen_message() {
        blank();
    }

    #[test]
    fn can_get_message_body_single_emoji() {
        let mut m = blank();
        m.text = Some("🙈".to_string());
        assert_eq!(m.body(), vec![BubbleType::Text("🙈")]);
    }

    #[test]
    fn can_get_message_body_multiple_emoji() {
        let mut m = blank();
        m.text = Some("🙈🙈🙈".to_string());
        assert_eq!(m.body(), vec![BubbleType::Text("🙈🙈🙈")]);
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

    #[test]
    fn can_get_time_date_read_after_date() {
        // Get offset
        let offset = get_offset();

        // Create message
        let mut message = blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        // May 17, 2022  8:29:42 PM
        message.date_delivered = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_read = 674530231992568192;

        println!("{:?}", message.time_until_read(&offset));
        assert_eq!(
            message.time_until_read(&offset),
            Some("1 hour, 49 seconds".to_string())
        )
    }

    #[test]
    fn can_get_time_date_read_before_date() {
        // Get offset
        let offset = get_offset();

        // Create message
        let mut message = blank();
        // May 17, 2022  9:30:31 PM
        message.date = 674530231992568192;
        // May 17, 2022  9:30:31 PM
        message.date_delivered = 674530231992568192;
        // May 17, 2022  8:29:42 PM
        message.date_read = 674526582885055488;

        println!("{:?}", message.time_until_read(&offset));
        assert_eq!(message.time_until_read(&offset), None)
    }

    #[test]
    fn can_get_message_expression_none() {
        let m = blank();
        assert_eq!(m.get_expressive(), expressives::Expressive::Normal);
    }

    #[test]
    fn can_get_message_expression_bubble() {
        let mut m = blank();
        m.expressive_send_style_id = Some("com.apple.MobileSMS.expressivesend.gentle".to_string());
        assert_eq!(
            m.get_expressive(),
            expressives::Expressive::Bubble(expressives::BubbleEffect::Gentle)
        );
    }

    #[test]
    fn can_get_message_expression_screen() {
        let mut m = blank();
        m.expressive_send_style_id =
            Some("com.apple.messages.effect.CKHappyBirthdayEffect".to_string());
        assert_eq!(
            m.get_expressive(),
            expressives::Expressive::Screen(expressives::ScreenEffect::Balloons)
        );
    }
}
