/*!
 This module represents common (but not all) columns in the `message` table.
*/

use std::{collections::HashMap, io::Read};

use chrono::{naive::NaiveDateTime, offset::Local, DateTime, TimeZone};
use plist::Value;
use rusqlite::{blob::Blob, Connection, Error, Result, Row, Statement};

use crate::{
    error::{message::MessageError, table::TableError},
    message_types::{
        expressives::{BubbleEffect, Expressive, ScreenEffect},
        variants::{Announcement, CustomBalloon, Reaction, Variant},
    },
    tables::table::{
        Cacheable, Diagnostic, Table, ATTRIBUTED_BODY, CHAT_MESSAGE_JOIN, MESSAGE,
        MESSAGE_ATTACHMENT_JOIN, MESSAGE_PAYLOAD, MESSAGE_SUMMARY_INFO, RECENTLY_DELETED,
    },
    util::{
        dates::{readable_diff, TIMESTAMP_FACTOR},
        output::{done_processing, processing},
        query_context::QueryContext,
        streamtyped,
    },
};

/// Character found in message body text that indicates attachment position
const ATTACHMENT_CHAR: char = '\u{FFFC}';
/// Character found in message body text that indicates app message position
const APP_CHAR: char = '\u{FFFD}';
/// A collection of characters that represent non-text content within body text
const REPLACEMENT_CHARS: [char; 2] = [ATTACHMENT_CHAR, APP_CHAR];

/// Represents a broad category of messages: standalone, thread originators, and thread replies.
#[derive(Debug)]
pub enum MessageType<'a> {
    /// A normal message not associated with any others
    Normal(Variant<'a>, Expressive<'a>),
    /// A message that has replies
    Thread(Variant<'a>, Expressive<'a>),
    /// A message that is a reply to another message
    Reply(Variant<'a>, Expressive<'a>),
}

/// Defines the parts of a message bubble, i.e. the content that can exist in a single message.
#[derive(Debug, PartialEq, Eq)]
pub enum BubbleType<'a> {
    /// A normal text message
    Text(&'a str),
    /// An attachment
    Attachment,
    /// An app integration
    App,
}

/// Defines different types of services we can receive messages from.
#[derive(Debug)]
pub enum Service<'a> {
    /// An iMessage
    #[allow(non_camel_case_types)]
    iMessage,
    /// A message sent as SMS
    SMS,
    /// Any other type of message
    Other(&'a str),
    /// Used when service field is not set
    Unknown,
}

/// Represents a single row in the `message` table.
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct Message {
    pub rowid: i32,
    pub guid: String,
    pub text: Option<String>,
    pub service: Option<String>,
    pub handle_id: Option<i32>,
    pub subject: Option<String>,
    pub date: i64,
    pub date_read: i64,
    pub date_delivered: i64,
    pub is_from_me: bool,
    pub is_read: bool,
    pub item_type: i32,
    pub group_title: Option<String>,
    pub group_action_type: i32,
    pub associated_message_guid: Option<String>,
    pub associated_message_type: Option<i32>,
    pub balloon_bundle_id: Option<String>,
    pub expressive_send_style_id: Option<String>,
    pub thread_originator_guid: Option<String>,
    pub thread_originator_part: Option<String>,
    pub date_edited: i64,
    pub chat_id: Option<i32>,
    pub num_attachments: i32,
    pub deleted_from: Option<i32>,
    pub num_replies: i32,
}

impl Table for Message {
    fn from_row(row: &Row) -> Result<Message> {
        Ok(Message {
            rowid: row.get("rowid")?,
            guid: row.get("guid")?,
            text: row.get("text").unwrap_or(None),
            service: row.get("service").unwrap_or(None),
            handle_id: row.get("handle_id").unwrap_or(None),
            subject: row.get("subject").unwrap_or(None),
            date: row.get("date")?,
            date_read: row.get("date_read").unwrap_or(0),
            date_delivered: row.get("date_delivered").unwrap_or(0),
            is_from_me: row.get("is_from_me")?,
            is_read: row.get("is_read")?,
            item_type: row.get("item_type").unwrap_or_default(),
            group_title: row.get("group_title").unwrap_or(None),
            group_action_type: row.get("group_action_type").unwrap_or(0),
            associated_message_guid: row.get("associated_message_guid").unwrap_or(None),
            associated_message_type: row.get("associated_message_type").unwrap_or(None),
            balloon_bundle_id: row.get("balloon_bundle_id").unwrap_or(None),
            expressive_send_style_id: row.get("expressive_send_style_id").unwrap_or(None),
            thread_originator_guid: row.get("thread_originator_guid").unwrap_or(None),
            thread_originator_part: row.get("thread_originator_part").unwrap_or(None),
            date_edited: row.get("date_edited").unwrap_or(0),
            chat_id: row.get("chat_id").unwrap_or(None),
            num_attachments: row.get("num_attachments")?,
            deleted_from: row.get("deleted_from").unwrap_or(None),
            num_replies: row.get("num_replies")?,
        })
    }

    fn get(db: &Connection) -> Result<Statement, TableError> {
        // If the database has `chat_recoverable_message_join`, we can restore some deleted messages.
        // If database has `thread_originator_guid`, we can parse replies, otherwise default to 0
        Ok(db.prepare(&format!(
            // macOS Ventura+ and i0S 16+ schema
            "SELECT
                 *,
                 c.chat_id,
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 (SELECT b.chat_id FROM {RECENTLY_DELETED} b WHERE m.ROWID = b.message_id) as deleted_from,
                 (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
             FROM
                 message as m
                 LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
             ORDER BY
                 m.date;
            "
        )).or(db.prepare(&format!(
            // macOS Big Sur to Monterey, iOS 14 to iOS 15 schema
            "SELECT
                 *,
                 c.chat_id,
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 NULL as deleted_from,
                 (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
             FROM
                 message as m
                 LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
             ORDER BY
                 m.date;
            "
        )))
        .unwrap_or(db.prepare(&format!(
            // macOS Catalina, iOS 13 and older 
            "SELECT
                 *,
                 c.chat_id,
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 NULL as deleted_from,
                 0 as num_replies
             FROM
                 message as m
                 LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
             ORDER BY
                 m.date;
            "
        )).map_err(TableError::Messages)?)
    )
    }

    fn extract(message: Result<Result<Self, Error>, Error>) -> Result<Self, TableError> {
        match message {
            Ok(Ok(message)) => Ok(message),
            Err(why) | Ok(Err(why)) => Err(TableError::Messages(why)),
        }
    }
}

impl Diagnostic for Message {
    /// Emit diagnostic data for the Messages table
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Diagnostic, get_connection};
    /// use imessage_database::tables::messages::Message;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// Message::run_diagnostic(&conn);
    /// ```
    fn run_diagnostic(db: &Connection) -> Result<(), TableError> {
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
            .map_err(TableError::Messages)?;

        let num_dangling: i32 = messages_without_chat
            .query_row([], |r| r.get(0))
            .unwrap_or(0);

        let mut messages_in_more_than_one_chat_q = db
            .prepare(&format!(
                "
            SELECT
                COUNT(*)
            FROM (
            SELECT DISTINCT
                message_id
              , COUNT(chat_id) AS c
            FROM {CHAT_MESSAGE_JOIN}
            GROUP BY
                message_id
            HAVING c > 1);
            "
            ))
            .map_err(TableError::Messages)?;

        let messages_in_more_than_one_chat: i32 = messages_in_more_than_one_chat_q
            .query_row([], |r| r.get(0))
            .unwrap_or(0);

        done_processing();

        if num_dangling > 0 || messages_in_more_than_one_chat > 0 {
            println!("Message diagnostic data:");
            if num_dangling > 0 {
                println!("    Messages not associated with a chat: {num_dangling}");
            }
            if messages_in_more_than_one_chat > 0 {
                println!("    Messages belonging to more than one chat: {messages_in_more_than_one_chat}");
            }
        }
        Ok(())
    }
}

impl Cacheable for Message {
    type K = String;
    type V = HashMap<usize, Vec<Self>>;
    /// Used for reactions that do not exist in a foreign key table
    ///
    /// Builds a map like:
    ///
    /// {
    ///     message_guid: {
    ///         0: [Message, Message],
    ///         1: [Message]
    ///     }
    /// }
    ///
    /// Where the `0` and `1` are the reaction indexes in the body of the message mapped by `message_guid`
    fn cache(db: &Connection) -> Result<HashMap<Self::K, Self::V>, TableError> {
        // Create cache for user IDs
        let mut map: HashMap<Self::K, Self::V> = HashMap::new();

        // Create query
        let statement = db.prepare(&format!(
            "SELECT 
                 *, 
                 c.chat_id, 
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
             FROM 
                 message as m 
                 LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
             WHERE m.associated_message_guid NOT NULL
            "
        ));

        if let Ok(mut statement) = statement {
            // Execute query to build the Handles
            let messages = statement
                .query_map([], |row| Ok(Message::from_row(row)))
                .map_err(TableError::Messages)?;

            // Iterate over the messages and update the map
            for reaction in messages {
                let reaction = Self::extract(reaction)?;
                if reaction.is_reaction() {
                    if let Some((idx, reaction_target_guid)) = reaction.clean_associated_guid() {
                        match map.get_mut(reaction_target_guid) {
                            Some(reactions) => match reactions.get_mut(&idx) {
                                Some(reactions_vec) => {
                                    reactions_vec.push(reaction);
                                }
                                None => {
                                    reactions.insert(idx, vec![reaction]);
                                }
                            },
                            None => {
                                map.insert(
                                    reaction_target_guid.to_string(),
                                    HashMap::from([(idx, vec![reaction])]),
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(map)
    }
}

impl Message {
    /// Get the body text of a message, parsing it as [`streamtyped`](crate::util::streamtyped) data if necessary.
    // TODO: resolve the compiler warnings with this method
    pub fn gen_text<'a>(&'a mut self, db: &'a Connection) -> Result<&'a str, MessageError> {
        if self.text.is_none() {
            let body = self.attributed_body(db).ok_or(MessageError::MissingData)?;
            self.text =
                Some(streamtyped::parse(body).map_err(MessageError::StreamTypedParseError)?);
        }

        if let Some(t) = &self.text {
            Ok(t)
        } else {
            Err(MessageError::NoText)
        }
    }

    /// Get a vector of a message's components
    ///
    /// If the message has attachments, there will be one [`U+FFFC`](https://www.compart.com/en/unicode/U+FFFC) character
    /// for each attachment and one [`U+FFFD`](https://www.compart.com/en/unicode/U+FFFD) for app messages that we need
    /// to format.
    ///
    /// An iMessage that contains body text like:
    ///
    /// `\u{FFFC}Check out this photo!`
    ///
    /// Will have a `body()` of:
    ///
    /// `[BubbleType::Attachment, BubbleType::Text("Check out this photo!")]`
    pub fn body(&self) -> Vec<BubbleType> {
        let mut out_v = vec![];

        // If the message is an app, it will be rendered differently, so just escape there
        if self.balloon_bundle_id.is_some() {
            out_v.push(BubbleType::App);
            return out_v;
        }

        match &self.text {
            Some(text) => {
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
            None => out_v,
        }
    }

    /// Create a `DateTime<Local>` from an arbitrary date and offset
    ///
    /// This is used to create date data for anywhere dates are stored in the table, including
    /// plist payload or [`streamtyped`](crate::util::streamtyped) data. In this struct, the
    /// other date methods invoke this method.
    pub fn get_local_time(
        &self,
        date_stamp: &i64,
        offset: &i64,
    ) -> Result<DateTime<Local>, MessageError> {
        let utc_stamp =
            NaiveDateTime::from_timestamp_opt((date_stamp / TIMESTAMP_FACTOR) + offset, 0)
                .ok_or(MessageError::InvalidTimestamp(*date_stamp))?;
        Ok(Local.from_utc_datetime(&utc_stamp))
    }

    /// Calculates the date a message was written to the database.
    ///
    /// This field is stored as a unix timestamp with an epoch of `2001-01-01 00:00:00` in the local time zone
    pub fn date(&self, offset: &i64) -> Result<DateTime<Local>, MessageError> {
        self.get_local_time(&self.date, offset)
    }

    /// Calculates the date a message was marked as delivered.
    ///
    /// This field is stored as a unix timestamp with an epoch of `2001-01-01 00:00:00` in the local time zone
    pub fn date_delivered(&self, offset: &i64) -> Result<DateTime<Local>, MessageError> {
        self.get_local_time(&self.date_delivered, offset)
    }

    /// Calculates the date a message was marked as read.
    ///
    /// This field is stored as a unix timestamp with an epoch of `2001-01-01 00:00:00` in the local time zone
    pub fn date_read(&self, offset: &i64) -> Result<DateTime<Local>, MessageError> {
        self.get_local_time(&self.date_read, offset)
    }

    /// Calculates the date a message was most recently edited.
    ///
    /// This field is stored as a unix timestamp with an epoch of `2001-01-01 00:00:00` in the local time zone
    pub fn date_edited(&self, offset: &i64) -> Result<DateTime<Local>, MessageError> {
        self.get_local_time(&self.date_read, offset)
    }

    /// Gets the time until the message was read. This can happen in two ways:
    ///
    /// - You received a message, then waited to read it
    /// - You sent a message, and the recipient waited to read it
    ///
    /// In the former case, this subtracts the date read column (`date_read`) from the date received column (`date`).
    /// In the latter case, this subtracts the date delivered column (`date_delivered`) from the date received column (`date`).
    ///
    /// Not all messages get tagged with the read properties.
    /// If more than one message has been sent in a thread before getting read,
    /// only the most recent message will get the tag.
    pub fn time_until_read(&self, offset: &i64) -> Option<String> {
        // Message we received
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

    /// `true` if the message renames a thread, else `false`
    pub fn is_announcement(&self) -> bool {
        self.group_title.is_some() || self.group_action_type != 0
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

    /// `true` if the message has a URL preview, else `false`
    pub fn is_url(&self) -> bool {
        matches!(self.variant(), Variant::App(CustomBalloon::URL))
    }

    /// `true` if the message was edited, else `false`
    pub fn is_edited(&self) -> bool {
        self.date_edited != 0
    }

    /// `true` if the message has attachments, else `false`
    pub fn has_attachments(&self) -> bool {
        self.num_attachments > 0
    }

    /// `true` if the message begins a thread, else `false`
    pub fn has_replies(&self) -> bool {
        self.num_replies > 0
    }

    /// `true` if the message is a SharePlay/FaceTime message, else `false`
    pub fn is_shareplay(&self) -> bool {
        self.item_type == 6
    }

    /// `true` if the message was deleted and is recoverable, else `false`
    ///
    /// Messages removed by deleting an entire conversation or by deleting a single message
    /// from a conversation are moved to a separate collection for up to 30 days. Messages
    /// present in this collection are restored to the conversations they belong to. Apple
    /// details this process [here](https://support.apple.com/en-us/HT202549#delete).
    ///
    /// Messages that have expired from this restoration process are permanently deleted and
    /// cannot be recovered.
    pub fn is_deleted(&self) -> bool {
        self.deleted_from.is_some()
    }

    /// Get the index of the part of a message a reply is pointing to
    fn get_reply_index(&self) -> usize {
        if let Some(parts) = &self.thread_originator_part {
            return match parts.split(':').next() {
                Some(part) => str::parse::<usize>(part).unwrap_or(0),
                None => 0,
            };
        }
        0
    }

    /// Get the number of messages in the database
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Diagnostic, get_connection};
    /// use imessage_database::tables::messages::Message;
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let context = QueryContext::default();
    /// Message::get_count(&conn, &context);
    /// ```
    pub fn get_count(db: &Connection, context: &QueryContext) -> Result<u64, TableError> {
        let mut statement = if context.has_filters() {
            db.prepare(&format!(
                "SELECT COUNT(*) FROM {MESSAGE} as m {}",
                context.generate_filter_statement()
            ))
            .map_err(TableError::Messages)?
        } else {
            db.prepare(&format!("SELECT COUNT(*) FROM {MESSAGE}"))
                .map_err(TableError::Messages)?
        };
        // Execute query to build the Handles
        let count: u64 = statement.query_row([], |r| r.get(0)).unwrap_or(0);
        Ok(count)
    }

    /// Stream messages from the database with optional filters
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::{Diagnostic, get_connection};
    /// use imessage_database::tables::messages::Message;
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let context = QueryContext::default();
    /// Message::stream_rows(&conn, &context).unwrap();
    pub fn stream_rows<'a>(
        db: &'a Connection,
        context: &'a QueryContext,
    ) -> Result<Statement<'a>, TableError> {
        if !context.has_filters() {
            return Self::get(db);
        } else {
            let filters = context.generate_filter_statement();

            // If database has `thread_originator_guid`, we can parse replies, otherwise default to 0
            Ok(db.prepare(&format!(
                "SELECT
                     *,
                     c.chat_id,
                     (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                     (SELECT b.chat_id FROM {RECENTLY_DELETED} b WHERE m.ROWID = b.message_id) as deleted_from,
                     (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
                 FROM
                     message as m
                     LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
                 {filters}
                 ORDER BY
                     m.date;
                "
            ))
            .unwrap_or(db.prepare(&format!(
                "SELECT
                     *,
                     c.chat_id,
                     (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                     (SELECT NULL) as deleted_from,
                     (SELECT 0) as num_replies
                 FROM
                     message as m
                     LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
                 {filters}
                 ORDER BY
                     m.date;
                "
            )).map_err(TableError::Messages)?))
        }
    }

    /// See [Reaction](crate::message_types::variants::Reaction) for details on this data.
    fn clean_associated_guid(&self) -> Option<(usize, &str)> {
        if let Some(guid) = &self.associated_message_guid {
            if guid.starts_with("p:") {
                let mut split = guid.split('/');
                let index_str = split.next()?;
                let message_id = split.next()?;
                let index = str::parse::<usize>(&index_str.replace("p:", "")).unwrap_or(0);
                return Some((index, message_id.get(0..36)?));
            } else if guid.starts_with("bp:") {
                return Some((0, guid.get(3..39)?));
            } else {
                return Some((0, guid.get(0..36)?));
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
    pub fn get_reactions(
        &self,
        db: &Connection,
        reactions: &HashMap<String, Vec<String>>,
    ) -> Result<HashMap<usize, Vec<Self>>, TableError> {
        let mut out_h: HashMap<usize, Vec<Self>> = HashMap::new();
        if let Some(rxs) = reactions.get(&self.guid) {
            let filter: Vec<String> = rxs.iter().map(|guid| format!("\"{guid}\"")).collect();
            // Create query
            let mut statement = db.prepare(&format!(
                "SELECT 
                        *, 
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
            )).map_err(TableError::Messages)?;

            // Execute query to build the Handles
            let messages = statement
                .query_map([], |row| Ok(Message::from_row(row)))
                .map_err(TableError::Messages)?;

            for message in messages {
                let msg = Message::extract(message)?;
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
        Ok(out_h)
    }

    /// Build a HashMap of message component index to messages that reply to that component
    pub fn get_replies(&self, db: &Connection) -> Result<HashMap<usize, Vec<Self>>, TableError> {
        let mut out_h: HashMap<usize, Vec<Self>> = HashMap::new();

        // No need to hit the DB if we know we don't have replies
        if self.has_replies() {
            let mut statement = db.prepare(&format!(
                "SELECT 
                     *, 
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
            .map_err(TableError::Messages)?;

            let iter = statement
                .query_map([], |row| Ok(Message::from_row(row)))
                .map_err(TableError::Messages)?;

            for message in iter {
                let m = Message::extract(message)?;
                let idx = m.get_reply_index();
                match out_h.get_mut(&idx) {
                    Some(body_part) => body_part.push(m),
                    None => {
                        out_h.insert(idx, vec![m]);
                    }
                }
            }
        }

        Ok(out_h)
    }

    /// Parse the App's Bundle ID out of the Balloon's Bundle ID
    fn parse_balloon_bundle_id(&self) -> Option<&str> {
        if let Some(bundle_id) = &self.balloon_bundle_id {
            let mut parts = bundle_id.split(':');
            let bundle_id = parts.next();
            // If there is only one part, use that, otherwise get the third part
            if parts.next().is_none() {
                bundle_id
            } else {
                // Will be None if there is no third part
                parts.next()
            }
        } else {
            None
        }
    }

    /// Get the variant of a message, see [crate::message_types::variants] for detail.
    pub fn variant(&self) -> Variant {
        // Check if a message was edited first as those have special properties
        if self.is_edited() {
            return Variant::Edited;
        }

        // Handle different types of bundle IDs next, as those are most common
        if let Some(associated_message_type) = self.associated_message_type {
            return match associated_message_type {
                // Standard iMessages with either text or a message payload
                0 | 2 | 3 => match self.parse_balloon_bundle_id() {
                    Some(bundle_id) => match bundle_id {
                        "com.apple.messages.URLBalloonProvider" => Variant::App(CustomBalloon::URL),
                        "com.apple.Handwriting.HandwritingProvider" => {
                            Variant::App(CustomBalloon::Handwriting)
                        }
                        "com.apple.PassbookUIService.PeerPaymentMessagesExtension" => {
                            Variant::App(CustomBalloon::ApplePay)
                        }
                        "com.apple.ActivityMessagesApp.MessagesExtension" => {
                            Variant::App(CustomBalloon::Fitness)
                        }
                        "com.apple.mobileslideshow.PhotosMessagesApp" => {
                            Variant::App(CustomBalloon::Slideshow)
                        }
                        _ => Variant::App(CustomBalloon::Application(bundle_id)),
                    },
                    // This is the most common case
                    None => Variant::Normal,
                },

                // Stickers overlaid on messages
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
            };
        }

        // Any other rarer cases belong here
        if self.is_shareplay() {
            return Variant::SharePlay;
        }

        Variant::Normal
    }

    /// Determine the type of announcement a message contains, if it contains one
    pub fn get_announcement(&self) -> Option<Announcement> {
        if let Some(name) = &self.group_title {
            return Some(Announcement::NameChange(name));
        }

        return match &self.group_action_type {
            0 => None,
            1 => Some(Announcement::PhotoChange),
            other => Some(Announcement::Unknown(other)),
        };
    }

    /// Determine the service the message was sent from, i.e. iMessage, SMS, IRC, etc.
    pub fn service(&self) -> Service {
        match self.service.as_deref() {
            Some("iMessage") => Service::iMessage,
            Some("SMS") => Service::SMS,
            Some(service_name) => Service::Other(service_name),
            None => Service::Unknown,
        }
    }

    /// Extract a blob of data that belongs to a single message from a given column
    fn get_blob<'a>(&self, db: &'a Connection, column: &str) -> Option<Blob<'a>> {
        match db.blob_open(
            rusqlite::DatabaseName::Main,
            MESSAGE,
            column,
            self.rowid as i64,
            true,
        ) {
            Ok(blob) => Some(blob),
            Err(_) => None,
        }
    }

    /// Get a message's plist from the `payload_data` BLOB column
    ///
    /// Calling this hits the database, so it is expensive and should
    /// only get invoked when needed.
    ///
    /// This column contains data used by iMessage app balloons.
    pub fn payload_data(&self, db: &Connection) -> Option<Value> {
        Value::from_reader(self.get_blob(db, MESSAGE_PAYLOAD)?).ok()
    }

    /// Get a message's plist from the `message_summary_info` BLOB column
    ///
    /// Calling this hits the database, so it is expensive and should
    /// only get invoked when needed.
    ///
    /// This column contains data used by edited iMessages.
    pub fn message_summary_info(&self, db: &Connection) -> Option<Value> {
        Value::from_reader(self.get_blob(db, MESSAGE_SUMMARY_INFO)?).ok()
    }

    /// Get a message's plist from the `attributedBody` BLOB column
    ///
    /// Calling this hits the database, so it is expensive and should
    /// only get invoked when needed.
    ///
    /// This column contains the message's body text with any other attributes.
    pub fn attributed_body(&self, db: &Connection) -> Option<Vec<u8>> {
        let mut body = vec![];
        self.get_blob(db, ATTRIBUTED_BODY)?
            .read_to_end(&mut body)
            .ok();
        Some(body)
    }

    /// Determine which expressive the message was sent with
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
            None => Expressive::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        message_types::{
            expressives,
            variants::{CustomBalloon, Variant},
        },
        tables::messages::{BubbleType, Message},
        util::dates::get_offset,
    };

    fn blank() -> Message {
        Message {
            rowid: i32::default(),
            guid: String::default(),
            text: None,
            service: Some("iMessage".to_string()),
            handle_id: Some(i32::default()),
            subject: None,
            date: i64::default(),
            date_read: i64::default(),
            date_delivered: i64::default(),
            is_from_me: false,
            is_read: false,
            item_type: 0,
            group_title: None,
            group_action_type: 0,
            associated_message_guid: None,
            associated_message_type: Some(i32::default()),
            balloon_bundle_id: None,
            expressive_send_style_id: None,
            thread_originator_guid: None,
            thread_originator_part: None,
            date_edited: 0,
            chat_id: None,
            num_attachments: 0,
            deleted_from: None,
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

        assert_eq!(message.time_until_read(&offset), None)
    }

    #[test]
    fn can_get_message_expression_none() {
        let m = blank();
        assert_eq!(m.get_expressive(), expressives::Expressive::None);
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

    #[test]
    fn can_get_no_balloon_bundle_id() {
        let m = blank();
        assert_eq!(m.parse_balloon_bundle_id(), None)
    }

    #[test]
    fn can_get_balloon_bundle_id_os() {
        let mut m = blank();
        m.balloon_bundle_id = Some("com.apple.Handwriting.HandwritingProvider".to_owned());
        assert_eq!(
            m.parse_balloon_bundle_id(),
            Some("com.apple.Handwriting.HandwritingProvider")
        )
    }

    #[test]
    fn can_get_balloon_bundle_id_url() {
        let mut m = blank();
        m.balloon_bundle_id = Some("com.apple.messages.URLBalloonProvider".to_owned());
        assert_eq!(
            m.parse_balloon_bundle_id(),
            Some("com.apple.messages.URLBalloonProvider")
        )
    }

    #[test]
    fn can_get_balloon_bundle_id_apple() {
        let mut m = blank();
        m.balloon_bundle_id = Some("com.apple.messages.MSMessageExtensionBalloonPlugin:0000000000:com.apple.PassbookUIService.PeerPaymentMessagesExtension".to_owned());
        assert_eq!(
            m.parse_balloon_bundle_id(),
            Some("com.apple.PassbookUIService.PeerPaymentMessagesExtension")
        )
    }

    #[test]
    fn can_get_balloon_bundle_id_third_party() {
        let mut m = blank();
        m.balloon_bundle_id = Some("com.apple.messages.MSMessageExtensionBalloonPlugin:QPU8QS3E62:com.contextoptional.OpenTable.Messages".to_owned());
        assert_eq!(
            m.parse_balloon_bundle_id(),
            Some("com.contextoptional.OpenTable.Messages")
        );
        assert!(matches!(
            m.variant(),
            Variant::App(CustomBalloon::Application(
                "com.contextoptional.OpenTable.Messages"
            ))
        ));
    }

    #[test]
    fn can_get_valid_guid() {
        let mut m = blank();
        m.associated_message_guid = Some("A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A".to_string());

        assert_eq!(
            Some((0usize, "A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A")),
            m.clean_associated_guid()
        );
    }

    #[test]
    fn cant_get_invalid_guid() {
        let mut m = blank();
        m.associated_message_guid = Some("FAKE_GUID".to_string());

        assert_eq!(None, m.clean_associated_guid());
    }

    #[test]
    fn can_get_valid_guid_p() {
        let mut m = blank();
        m.associated_message_guid = Some("p:1/A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A".to_string());

        assert_eq!(
            Some((1usize, "A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A")),
            m.clean_associated_guid()
        );
    }

    #[test]
    fn cant_get_invalid_guid_p() {
        let mut m = blank();
        m.associated_message_guid = Some("p:1/FAKE_GUID".to_string());

        assert_eq!(None, m.clean_associated_guid());
    }

    #[test]
    fn can_get_valid_guid_bp() {
        let mut m = blank();
        m.associated_message_guid = Some("bp:A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A".to_string());

        assert_eq!(
            Some((0usize, "A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A")),
            m.clean_associated_guid()
        );
    }

    #[test]
    fn cant_get_invalid_guid_bp() {
        let mut m = blank();
        m.associated_message_guid = Some("bp:FAKE_GUID".to_string());

        assert_eq!(None, m.clean_associated_guid());
    }
}
