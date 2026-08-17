/*!
 Message table rows, query helpers, and body parsing.

 # Iterating over Message Data

 Use [`Message::stream()`] to iterate over the default message query.

 ## Example
 ```no_run
 use imessage_database::{
     error::table::TableError,
     tables::{
         messages::Message,
         table::{get_connection, Table},
     },
     util::dirs::default_db_path,
 };

 #[derive(Debug)]
 struct ProgramError(TableError);

 impl From<TableError> for ProgramError {
     fn from(err: TableError) -> Self {
         Self(err)
     }
 }

 // Get the default database path and connect to it
 let db_path = default_db_path();
 let conn = get_connection(&db_path).unwrap();

 Message::stream(&conn, |message_result| {
    match message_result {
        Ok(message) => println!("Message: {:#?}", message),
        Err(e) => eprintln!("Error: {:?}", e),
    }
    Ok::<(), ProgramError>(())
 }).unwrap();
 ```

 # Making Custom Message Queries

 [`Message`] includes a few fields that are derived by the default query and
 are not direct `message` table columns:

 - [`Message::chat_id`]
 - [`Message::num_attachments`]
 - [`Message::deleted_from`]
 - [`Message::num_replies`]

 [`Message::filter_action`] and [`Message::filter_sub_action`] map optional
 `message` columns. Queries that use the explicit positional shape must select
 `NULL` for both fields when the source schema omits them. The `SELECT *` query
 heads decode by name and map missing columns to `None`.

 ## Sample Queries

 Custom queries must include those derived columns:

 ```sql
 SELECT
     *,
     c.chat_id,
     (SELECT COUNT(*) FROM message_attachment_join a WHERE m.ROWID = a.message_id) as num_attachments,
     d.chat_id as deleted_from,
     (SELECT COUNT(*) FROM message m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
 FROM
     message as m
 LEFT JOIN chat_message_join as c ON m.ROWID = c.message_id
 LEFT JOIN chat_recoverable_message_join as d ON m.ROWID = d.message_id
 ORDER BY
     m.date;
 ```

 If a source database does not include recoverable-message or reply columns,
 synthesize the missing values:

 ```sql
 SELECT
     *,
     c.chat_id,
     (SELECT COUNT(*) FROM message_attachment_join a WHERE m.ROWID = a.message_id) as num_attachments,
     NULL as deleted_from,
     0 as num_replies
 FROM
     message as m
 LEFT JOIN chat_message_join as c ON m.ROWID = c.message_id
 ORDER BY
     m.date;
 ```

 ## Custom Query Example

 This returns an iterator over messages that have an associated emoji:


 ```no_run
 use imessage_database::{
     tables::{
         messages::Message,
         table::{get_connection, Table},
     },
     util::dirs::default_db_path
 };

 let db_path = default_db_path();
 let db = get_connection(&db_path).unwrap();

 let mut statement = db.prepare_cached("
 SELECT
     *,
     c.chat_id,
     (SELECT COUNT(*) FROM message_attachment_join a WHERE m.ROWID = a.message_id) as num_attachments,
     d.chat_id as deleted_from,
     (SELECT COUNT(*) FROM message m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
 FROM
     message as m
 LEFT JOIN chat_message_join as c ON m.ROWID = c.message_id
 LEFT JOIN chat_recoverable_message_join as d ON m.ROWID = d.message_id
 WHERE m.associated_message_emoji IS NOT NULL
 ORDER BY
     m.date;
 ").unwrap();

 for message in Message::rows(&mut statement, []).unwrap() {
     println!("{:#?}", message);
 }
 ```
*/

use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    io::{Cursor, Read},
};

use chrono::{DateTime, offset::Local};
use crabstep::TypedStreamDeserializer;
use plist::Value;
use rusqlite::{CachedStatement, Connection, Result, Row};

use crate::{
    error::{message::MessageError, table::TableError},
    message_types::{
        edited::{EditStatus, EditedMessage},
        expressives::{BubbleEffect, Expressive, ScreenEffect},
        polls::Poll,
        text_effects::text_effect::TextEffect,
        translation::Translation,
        variants::{Announcement, BalloonProvider, CustomBalloon, Tapback, TapbackAction, Variant},
    },
    tables::{
        diagnostic::{MessageDiagnostic, count_query, table_exists},
        messages::{
            body::{parse_body_legacy, parse_body_typedstream},
            models::{BubbleComponent, FilterAction, GroupAction, Service, SharedLocation},
            query_parts::{
                ios_13_older_query, ios_14_15_query, ios_16_newer_query, ios_27_newer_query,
            },
        },
        table::{
            ATTRIBUTED_BODY, CHAT_MESSAGE_JOIN, Cacheable, MESSAGE, MESSAGE_ATTACHMENT_JOIN,
            MESSAGE_PAYLOAD, MESSAGE_SUMMARY_INFO, RECENTLY_DELETED, Table,
        },
    },
    util::{
        bundle_id::parse_balloon_bundle_id,
        dates::{get_local_time, readable_diff},
        query_context::QueryContext,
        streamtyped,
    },
};

// MARK: Columns
/// Columns shared by the explicit message query heads, in positional decode order.
pub(crate) const COLS: &str = "rowid, guid, text, service, handle_id, destination_caller_id, subject, date, date_read, date_delivered, is_from_me, is_read, item_type, other_handle, share_status, share_direction, group_title, group_action_type, associated_message_guid, associated_message_type, balloon_bundle_id, expressive_send_style_id, thread_originator_guid, thread_originator_part, date_edited, associated_message_emoji";

/// Row from the `message` table, plus body/edit metadata populated by [`parse_body`](Self::parse_body).
#[derive(Debug)]
#[allow(non_snake_case)]
pub struct Message {
    /// Message row ID.
    pub rowid: i32,
    /// Message GUID.
    pub guid: String,
    /// Plain body text. [`parse_body`](Self::parse_body) may populate this from `attributedBody`.
    pub text: Option<String>,
    /// Raw service name.
    pub service: Option<String>,
    /// Sender handle row ID.
    pub handle_id: Option<i32>,
    /// Address that received the message.
    pub destination_caller_id: Option<String>,
    /// Subject field.
    pub subject: Option<String>,
    /// Raw timestamp for when the message was written to the database.
    pub date: i64,
    /// Raw timestamp for when the message was read.
    pub date_read: i64,
    /// Raw timestamp for when the message was delivered.
    pub date_delivered: i64,
    /// `true` when the database owner sent the message.
    pub is_from_me: bool,
    /// `true` when the message was read by the recipient.
    pub is_read: bool,
    /// Message item type used by [`variant`](Self::variant).
    pub item_type: i32,
    /// Additional handle used by shared-location and group-action messages.
    pub other_handle: Option<i32>,
    /// Shared-location active/inactive flag.
    pub share_status: bool,
    /// Shared-location direction flag.
    pub share_direction: Option<bool>,
    /// Group title carried by group-name-change messages.
    pub group_title: Option<String>,
    /// Group action code.
    pub group_action_type: i32,
    /// GUID of the message this row references.
    pub associated_message_guid: Option<String>,
    /// Type code for the associated message, used by [`variant`](Self::variant).
    pub associated_message_type: Option<i32>,
    /// The [bundle ID](https://developer.apple.com/help/app-store-connect/reference/app-bundle-information) of the app that generated the [`AppMessage`](crate::message_types::app::AppMessage)
    pub balloon_bundle_id: Option<String>,
    /// Expressive-send identifier used by [`get_expressive`](Self::get_expressive).
    pub expressive_send_style_id: Option<String>,
    /// Indicates the first message in a thread of replies in [`get_replies()`](crate::tables::messages::Message::get_replies)
    pub thread_originator_guid: Option<String>,
    /// Body part index targeted by a reply.
    pub thread_originator_part: Option<String>,
    /// Raw timestamp for the most recent edit.
    pub date_edited: i64,
    /// Emoji associated with a custom emoji tapback.
    pub associated_message_emoji: Option<String>,
    /// Chat row ID this message belongs to.
    pub chat_id: Option<i32>,
    /// Number of attached files included in the message.
    pub num_attachments: i32,
    /// The [`rowid`](crate::tables::chat::Chat::rowid) of the chat the message was deleted from
    pub deleted_from: Option<i32>,
    /// Number of replies to the message.
    pub num_replies: i32,
    /// Raw message filter category code, read by [`filter_action`](Self::filter_action).
    pub filter_action: Option<i32>,
    /// Raw message filter subcategory code. This field has no parsed representation.
    pub filter_sub_action: Option<i32>,
    /// The components of the message body, parsed by a [`TypedStreamDeserializer`] or [`streamtyped::parse()`]
    pub components: Vec<BubbleComponent>,
    /// Parsed edit/unsent metadata from `message_summary_info`.
    pub edited_parts: Option<EditedMessage>,
}

/// Body data returned by [`Message::parse_body`].
///
/// Use [`Message::apply_body()`] to apply the parsed body back to the message:
///
/// ```no_run
/// # use imessage_database::tables::{messages::Message, table::get_connection};
/// # use imessage_database::util::dirs::default_db_path;
/// # let conn = get_connection(&default_db_path()).unwrap();
/// # let mut message = Message::from_guid("example", &conn).unwrap();
/// if let Ok(body) = message.parse_body(&conn) {
///     message.apply_body(body);
/// }
/// ```
#[derive(Debug)]
#[must_use]
pub struct ParsedBody {
    /// Plain body text.
    pub text: Option<String>,
    /// Parsed body components.
    pub components: Vec<BubbleComponent>,
    /// Parsed edit/unsent metadata.
    pub edited_parts: Option<EditedMessage>,
    /// Resolved balloon bundle ID.
    pub balloon_bundle_id: Option<String>,
}

// MARK: Table
impl Table for Message {
    fn from_row(row: &Row) -> Result<Message> {
        Self::from_row_idx(row).or_else(|_| Self::from_row_named(row))
    }

    /// Prepare the first message query compatible with the database schema.
    fn get(db: &'_ Connection) -> Result<CachedStatement<'_>, TableError> {
        Ok(db
            .prepare_cached(&ios_27_newer_query(None))
            .or_else(|_| db.prepare_cached(&ios_16_newer_query(None)))
            .or_else(|_| db.prepare_cached(&ios_14_15_query(None)))
            .or_else(|_| db.prepare_cached(&ios_13_older_query(None)))?)
    }
}

// MARK: Diagnostic
impl Message {
    /// Compute diagnostic data for the `message` table.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::get_connection;
    /// use imessage_database::tables::messages::Message;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// Message::run_diagnostic(&conn);
    /// ```
    pub fn run_diagnostic(db: &Connection) -> Result<MessageDiagnostic, TableError> {
        let messages_without_chat = count_query(
            db,
            &format!(
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
            ),
        )?;

        let messages_in_multiple_chats = count_query(
            db,
            &format!(
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
            ),
        )?;

        let total_messages = count_query(
            db,
            &format!(
                "
            SELECT
                COUNT(rowid)
            FROM
                {MESSAGE}
            "
            ),
        )?;

        // Recently deleted messages are stored in a separate table when present.
        let recoverable_messages = if table_exists(db, RECENTLY_DELETED)? {
            Some(count_query(
                db,
                &format!("SELECT COUNT(*) FROM {RECENTLY_DELETED}"),
            )?)
        } else {
            None
        };

        // The date range is nullable when the message table is empty.
        let mut date_range = db.prepare(&format!("SELECT MIN(date), MAX(date) FROM {MESSAGE}"))?;
        let (first_message_date, last_message_date): (Option<i64>, Option<i64>) = date_range
            .query_row([], |r| Ok((r.get(0).ok(), r.get(1).ok())))
            .unwrap_or((None, None));

        Ok(MessageDiagnostic {
            total_messages,
            messages_without_chat,
            messages_in_multiple_chats,
            recoverable_messages,
            first_message_date,
            last_message_date,
        })
    }
}

// MARK: Cache
impl Cacheable for Message {
    type K = String;
    type V = HashMap<usize, Vec<Self>>;
    /// Cache tapback messages by target message GUID and body component index.
    ///
    /// Builds a map like:
    ///
    /// ```json
    /// {
    ///     "message_guid": {
    ///         0: [Message, Message],
    ///         1: [Message]
    ///     }
    /// }
    /// ```
    ///
    /// The `0` and `1` keys are component indexes in the target message body.
    fn cache(db: &Connection) -> Result<HashMap<Self::K, Self::V>, TableError> {
        // Create cache for user IDs
        let mut map: HashMap<Self::K, Self::V> = HashMap::new();

        // Create query
        let statement = db.prepare(&format!(
            "SELECT
                 {COLS},
                 c.chat_id,
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 NULL as deleted_from,
                 0 as num_replies,
                 NULL as filter_action,
                 NULL as filter_sub_action
             FROM
                 {MESSAGE} as m
             LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
             WHERE m.associated_message_guid IS NOT NULL
            "
        )).or_else(|_| db.prepare(&format!(
            "SELECT
                 *,
                 c.chat_id,
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 NULL as deleted_from,
                 0 as num_replies
             FROM
                 {MESSAGE} as m
             LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
             WHERE m.associated_message_guid IS NOT NULL
            "
        )));

        if let Ok(mut statement) = statement {
            for message in Self::rows(&mut statement, [])? {
                let message = message?;
                if message.is_tapback()
                    && let Some((idx, tapback_target_guid)) = message.clean_associated_guid()
                {
                    map.entry(tapback_target_guid.to_string())
                        .or_insert_with(HashMap::new)
                        .entry(idx)
                        .or_insert_with(Vec::new)
                        .push(message);
                }
            }
        }

        Ok(map)
    }
}

// MARK: Impl
impl Message {
    /// Build a [`Message`] from a row using indexed columns.
    fn from_row_idx(row: &Row) -> Result<Message> {
        Ok(Message {
            rowid: row.get(0)?,
            guid: row.get(1)?,
            text: row.get(2).unwrap_or(None),
            service: row.get(3).unwrap_or(None),
            handle_id: row.get(4).unwrap_or(None),
            destination_caller_id: row.get(5).unwrap_or(None),
            subject: row.get(6).unwrap_or(None),
            date: row.get(7)?,
            date_read: row.get(8).unwrap_or(0),
            date_delivered: row.get(9).unwrap_or(0),
            is_from_me: row.get(10)?,
            is_read: row.get(11).unwrap_or(false),
            item_type: row.get(12).unwrap_or_default(),
            other_handle: row.get(13).unwrap_or(None),
            share_status: row.get(14).unwrap_or(false),
            share_direction: row.get(15).unwrap_or(None),
            group_title: row.get(16).unwrap_or(None),
            group_action_type: row.get(17).unwrap_or(0),
            associated_message_guid: row.get(18).unwrap_or(None),
            associated_message_type: row.get(19).unwrap_or(None),
            balloon_bundle_id: row.get(20).unwrap_or(None),
            expressive_send_style_id: row.get(21).unwrap_or(None),
            thread_originator_guid: row.get(22).unwrap_or(None),
            thread_originator_part: row.get(23).unwrap_or(None),
            date_edited: row.get(24).unwrap_or(0),
            associated_message_emoji: row.get(25).unwrap_or(None),
            chat_id: row.get(26).unwrap_or(None),
            num_attachments: row.get(27)?,
            deleted_from: row.get(28).unwrap_or(None),
            num_replies: row.get(29)?,
            filter_action: row.get(30).unwrap_or(None),
            filter_sub_action: row.get(31).unwrap_or(None),
            components: vec![],
            edited_parts: None,
        })
    }

    /// Build a [`Message`] from a row using named columns.
    fn from_row_named(row: &Row) -> Result<Message> {
        Ok(Message {
            rowid: row.get("rowid")?,
            guid: row.get("guid")?,
            text: row.get("text").unwrap_or(None),
            service: row.get("service").unwrap_or(None),
            handle_id: row.get("handle_id").unwrap_or(None),
            destination_caller_id: row.get("destination_caller_id").unwrap_or(None),
            subject: row.get("subject").unwrap_or(None),
            date: row.get("date")?,
            date_read: row.get("date_read").unwrap_or(0),
            date_delivered: row.get("date_delivered").unwrap_or(0),
            is_from_me: row.get("is_from_me")?,
            is_read: row.get("is_read").unwrap_or(false),
            item_type: row.get("item_type").unwrap_or_default(),
            other_handle: row.get("other_handle").unwrap_or(None),
            share_status: row.get("share_status").unwrap_or(false),
            share_direction: row.get("share_direction").unwrap_or(None),
            group_title: row.get("group_title").unwrap_or(None),
            group_action_type: row.get("group_action_type").unwrap_or(0),
            associated_message_guid: row.get("associated_message_guid").unwrap_or(None),
            associated_message_type: row.get("associated_message_type").unwrap_or(None),
            balloon_bundle_id: row.get("balloon_bundle_id").unwrap_or(None),
            expressive_send_style_id: row.get("expressive_send_style_id").unwrap_or(None),
            thread_originator_guid: row.get("thread_originator_guid").unwrap_or(None),
            thread_originator_part: row.get("thread_originator_part").unwrap_or(None),
            date_edited: row.get("date_edited").unwrap_or(0),
            associated_message_emoji: row.get("associated_message_emoji").unwrap_or(None),
            chat_id: row.get("chat_id").unwrap_or(None),
            num_attachments: row.get("num_attachments")?,
            deleted_from: row.get("deleted_from").unwrap_or(None),
            num_replies: row.get("num_replies")?,
            filter_action: row.get("filter_action").unwrap_or(None),
            filter_sub_action: row.get("filter_sub_action").unwrap_or(None),
            components: vec![],
            edited_parts: None,
        })
    }

    // MARK: Text Gen
    /// Parse the body of a message, deserializing it as [`typedstream`](crate::util::typedstream)
    /// (and falling back to [`streamtyped`]) data if necessary.
    ///
    /// This method performs pure parsing without mutating the message. Use [`Self::apply_body()`]
    /// to apply the result back to the message.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use imessage_database::tables::{messages::Message, table::get_connection};
    /// # use imessage_database::util::dirs::default_db_path;
    /// # let conn = get_connection(&default_db_path()).unwrap();
    /// # let mut message = Message::from_guid("example", &conn).unwrap();
    /// if let Ok(body) = message.parse_body(&conn) {
    ///     message.apply_body(body);
    /// }
    /// ```
    pub fn parse_body(&self, db: &Connection) -> Result<ParsedBody, MessageError> {
        // Parse the edited message data
        let edited_parts = self
            .is_edited()
            .then(|| self.message_summary_info(db))
            .flatten()
            .as_ref()
            .and_then(|payload| EditedMessage::from_map(payload).ok());

        // Initialize variables for the text, components, and balloon bundle ID that will be parsed from the body
        let mut text = None;
        let mut components = vec![];
        let mut balloon_bundle_id = None;

        // Grab the body data from the table
        if let Some(body) = self.attributed_body(db) {
            // Attempt to deserialize the typedstream data
            let mut typedstream = TypedStreamDeserializer::new(&body);
            match parse_body_typedstream(typedstream.iter_root().ok(), edited_parts.as_ref()) {
                Some(parsed) => {
                    text = parsed.text;

                    // Single-link messages can render as URL previews even
                    // when `balloon_bundle_id` is missing.
                    let is_single_url = match &parsed.components[..] {
                        [BubbleComponent::Run(ranges)] => match &ranges[..] {
                            [range] if range.attachment.is_none() => {
                                matches!(&range.effects[..], [TextEffect::Link(_)])
                            }
                            _ => false,
                        },
                        _ => false,
                    };

                    // App payloads render as a single app component.
                    if self.balloon_bundle_id.is_some() {
                        components = vec![BubbleComponent::App];
                    } else if is_single_url
                        && self.has_blob(db, MESSAGE, MESSAGE_PAYLOAD, self.rowid.into())
                    {
                        // URL previews may omit `balloon_bundle_id` while still carrying
                        // preview payload data.
                        balloon_bundle_id =
                            Some("com.apple.messages.URLBalloonProvider".to_string());
                        components = vec![BubbleComponent::App];
                    } else {
                        components = parsed.components;
                    }
                }
                None => {
                    // Typedstream failed entirely; try self.text before legacy parser
                    text = self.text.clone();
                }
            }

            // The legacy parser can still recover text from older attributed bodies.
            if text.is_none() {
                text = Some(streamtyped::parse(body)?);
            }
        }

        // Older message rows may already have the plain text field populated.
        let text = text.or_else(|| self.text.clone());

        // The balloon bundle ID can be set in the single URL case, otherwise it should fall back to the existing balloon bundle ID on the message
        let balloon_bundle_id = balloon_bundle_id.or_else(|| self.balloon_bundle_id.clone());

        // If typedstream did not produce components, derive simple text ranges.
        if components.is_empty() && text.is_some() {
            components = parse_body_legacy(&text);
        }

        // Fully unsent messages can have edit metadata without remaining text.
        if text.is_some() || !components.is_empty() || edited_parts.is_some() {
            Ok(ParsedBody {
                text,
                components,
                edited_parts,
                balloon_bundle_id,
            })
        } else {
            Err(MessageError::NoText)
        }
    }

    /// Apply a [`ParsedBody`] to this message, setting its text, components,
    /// edited parts, and balloon bundle ID.
    pub fn apply_body(&mut self, body: ParsedBody) {
        self.text = body.text;
        self.components = body.components;
        self.edited_parts = body.edited_parts;
        self.balloon_bundle_id = body.balloon_bundle_id;
    }

    /// Parse text with the legacy parser only.
    ///
    /// This ignores typedstream attributes and does not preserve every modern message type.
    pub fn generate_text_legacy<'a>(
        &'a mut self,
        db: &'a Connection,
    ) -> Result<&'a str, MessageError> {
        // If the text is missing, try and query for it
        if self.text.is_none()
            && let Some(body) = self.attributed_body(db)
        {
            self.text = Some(streamtyped::parse(body)?);
        }

        // Fallback component parser as well
        if self.components.is_empty() {
            self.components = parse_body_legacy(&self.text);
        }

        self.text.as_deref().ok_or(MessageError::NoText)
    }

    // MARK: Dates
    /// Convert [`date`](Self::date) to local time.
    ///
    /// This field is stored as a unix timestamp with an epoch of `2001-01-01 00:00:00` in the local time zone
    ///
    /// `offset` can be provided by [`get_offset`](crate::util::dates::get_offset) or manually.
    pub fn date(&self, offset: i64) -> Result<DateTime<Local>, MessageError> {
        get_local_time(self.date, offset)
    }

    /// Convert [`date_delivered`](Self::date_delivered) to local time.
    ///
    /// This field is stored as a unix timestamp with an epoch of `2001-01-01 00:00:00` in the local time zone
    ///
    /// `offset` can be provided by [`get_offset`](crate::util::dates::get_offset) or manually.
    pub fn date_delivered(&self, offset: i64) -> Result<DateTime<Local>, MessageError> {
        get_local_time(self.date_delivered, offset)
    }

    /// Convert [`date_read`](Self::date_read) to local time.
    ///
    /// This field is stored as a unix timestamp with an epoch of `2001-01-01 00:00:00` in the local time zone
    ///
    /// `offset` can be provided by [`get_offset`](crate::util::dates::get_offset) or manually.
    pub fn date_read(&self, offset: i64) -> Result<DateTime<Local>, MessageError> {
        get_local_time(self.date_read, offset)
    }

    /// Convert [`date_edited`](Self::date_edited) to local time.
    ///
    /// This field is stored as a unix timestamp with an epoch of `2001-01-01 00:00:00` in the local time zone
    ///
    /// `offset` can be provided by [`get_offset`](crate::util::dates::get_offset) or manually.
    pub fn date_edited(&self, offset: i64) -> Result<DateTime<Local>, MessageError> {
        get_local_time(self.date_edited, offset)
    }

    /// Calculate the elapsed time until the message was read or delivered.
    ///
    /// This can happen in two ways:
    ///
    /// - You received a message, then waited to read it
    /// - You sent a message, and the recipient waited to read it
    ///
    /// In the former case, this computes the difference from the date received (`date`) to the date read (`date_read`).
    /// In the latter case, this computes the difference from the date sent (`date`) to the date delivered (`date_delivered`).
    ///
    /// Not all messages get tagged with the read properties.
    /// If more than one message has been sent in a thread before getting read,
    /// only the most recent message will get the tag.
    ///
    /// `offset` can be provided by [`get_offset`](crate::util::dates::get_offset) or manually.
    #[must_use]
    pub fn time_until_read(&self, offset: i64) -> Option<String> {
        // Message we received
        if !self.is_from_me && self.date_read != 0 && self.date != 0 {
            return readable_diff(&self.date(offset).ok()?, &self.date_read(offset).ok()?);
        }
        // Message we sent
        else if self.is_from_me && self.date_delivered != 0 && self.date != 0 {
            return readable_diff(&self.date(offset).ok()?, &self.date_delivered(offset).ok()?);
        }
        None
    }

    // MARK: Bools
    /// `true` when the message is a thread reply.
    #[must_use]
    pub fn is_reply(&self) -> bool {
        self.thread_originator_guid.is_some()
    }

    /// `true` when the message is an [`Announcement`].
    #[must_use]
    pub fn is_announcement(&self) -> bool {
        self.get_announcement().is_some()
    }

    /// `true` when the message is a [`Tapback`] to another message.
    #[must_use]
    pub fn is_tapback(&self) -> bool {
        matches!(self.variant(), Variant::Tapback(..))
    }

    /// `true` when the message has an [`Expressive`] send effect.
    #[must_use]
    pub fn is_expressive(&self) -> bool {
        self.expressive_send_style_id.is_some()
    }

    /// `true` when the message has a [URL preview](crate::message_types::url).
    #[must_use]
    pub fn is_url(&self) -> bool {
        matches!(self.variant(), Variant::App(CustomBalloon::URL))
    }

    /// `true` when the message is a [`HandwrittenMessage`](crate::message_types::handwriting::models::HandwrittenMessage).
    #[must_use]
    pub fn is_handwriting(&self) -> bool {
        matches!(self.variant(), Variant::App(CustomBalloon::Handwriting))
    }

    /// `true` when the message is a [`Digital Touch`](crate::message_types::digital_touch::models) message.
    #[must_use]
    pub fn is_digital_touch(&self) -> bool {
        matches!(self.variant(), Variant::App(CustomBalloon::DigitalTouch))
    }

    /// `true` when the message is a [`Poll`].
    #[must_use]
    pub fn is_poll(&self) -> bool {
        matches!(self.variant(), Variant::App(CustomBalloon::Polls))
    }

    /// `true` when the message is a [`PollVote`](crate::message_types::polls::PollVote).
    #[must_use]
    pub fn is_poll_vote(&self) -> bool {
        self.associated_message_type == Some(4000)
    }

    /// `true` when the message adds or updates poll options.
    #[must_use]
    pub fn is_poll_update(&self) -> bool {
        matches!(self.variant(), Variant::PollUpdate)
    }

    /// `true` when the message was [`edited`](crate::message_types::edited).
    #[must_use]
    pub fn is_edited(&self) -> bool {
        self.date_edited != 0
    }

    /// `true` when the specified message component was [edited](crate::message_types::edited::EditStatus::Edited).
    #[must_use]
    pub fn is_part_edited(&self, index: usize) -> bool {
        if let Some(edited_parts) = &self.edited_parts
            && let Some(part) = edited_parts.part(index)
        {
            return matches!(part.status, EditStatus::Edited);
        }
        false
    }

    /// `true` when all message components were [unsent](crate::message_types::edited::EditStatus::Unsent).
    #[must_use]
    pub fn is_fully_unsent(&self) -> bool {
        self.edited_parts.as_ref().is_some_and(|ep| {
            ep.parts
                .iter()
                .all(|part| matches!(part.status, EditStatus::Unsent))
        })
    }

    /// `true` when the message contains [`Attachment`](crate::tables::attachment::Attachment)s.
    ///
    /// Attachments can be queried with [`Attachment::from_message()`](crate::tables::attachment::Attachment::from_message).
    #[must_use]
    pub fn has_attachments(&self) -> bool {
        self.num_attachments > 0
    }

    /// `true` when the message begins a thread.
    #[must_use]
    pub fn has_replies(&self) -> bool {
        self.num_replies > 0
    }

    /// `true` when the message indicates a sent audio message was kept.
    #[must_use]
    pub fn is_kept_audio_message(&self) -> bool {
        self.item_type == 5
    }

    /// `true` when the message is a [SharePlay/FaceTime](crate::message_types::variants::Variant::SharePlay) message.
    #[must_use]
    pub fn is_shareplay(&self) -> bool {
        self.item_type == 6
    }

    /// `true` when the message was sent by the database owner.
    #[must_use]
    pub fn is_from_me(&self) -> bool {
        // Share direction and other handle are only populated for shared location messages,
        // so this check is only necessary for those
        if self.item_type == 4
            && let (Some(other_handle), Some(share_direction)) =
                (self.other_handle, self.share_direction)
        {
            self.is_from_me || other_handle != 0 && !share_direction
        } else {
            self.is_from_me
        }
    }

    /// Returns the [`SharedLocation`] when the message is a legacy
    /// shared-location event.
    #[must_use]
    pub fn shared_location_kind(&self) -> Option<SharedLocation> {
        if self.item_type == 4 && self.group_action_type == 0 {
            Some(if self.share_status {
                SharedLocation::Stopped
            } else {
                SharedLocation::Started
            })
        } else {
            None
        }
    }

    /// `true` when the message is present in the recoverable deleted-message table.
    ///
    /// Messages removed by deleting an entire conversation or by deleting a single message
    /// from a conversation are moved to a separate collection for up to 30 days. Messages
    /// present in this collection are restored to the conversations they belong to. Apple
    /// details this process [here](https://support.apple.com/en-us/HT202549#delete).
    ///
    /// Messages that have expired from this restoration process are permanently deleted and
    /// cannot be recovered.
    ///
    /// Note: This is not the same as an [`Unsent`](crate::message_types::edited::EditStatus::Unsent) message.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.deleted_from.is_some()
    }

    /// `true` when the message summary includes translation metadata.
    pub fn has_translation(&self, db: &Connection) -> bool {
        // `7472616E736C6174696F6E4C616E6775616765` -> "translationLanguage"
        // `7472616E736C6174656454657874` -> "translatedText"
        let query = format!(
            "SELECT ROWID FROM {MESSAGE} 
                WHERE message_summary_info IS NOT NULL 
                AND length(message_summary_info) > 61 
                AND instr(message_summary_info, X'7472616E736C6174696F6E4C616E6775616765') > 0 
                AND instr(message_summary_info, X'7472616E736C6174656454657874') > 0 
                AND ROWID = ?"
        );
        if let Ok(mut statement) = db.prepare_cached(&query) {
            let result: Result<i32, _> = statement.query_row([self.rowid], |row| row.get(0));
            result.is_ok()
        } else {
            false
        }
    }

    /// Parse translation metadata for the message.
    pub fn get_translation(&self, db: &Connection) -> Result<Option<Translation>, MessageError> {
        if let Some(payload) = self.message_summary_info(db) {
            return Ok(Some(Translation::from_payload(&payload)?));
        }
        Ok(None)
    }

    /// Cache message GUIDs whose summaries include translation metadata.
    pub fn cache_translations(db: &Connection) -> Result<HashSet<String>, TableError> {
        // `7472616E736C6174696F6E4C616E6775616765` -> "translationLanguage"
        // `7472616E736C6174656454657874` -> "translatedText"
        let query = format!(
            "SELECT guid FROM {MESSAGE} 
                WHERE message_summary_info IS NOT NULL 
                AND length(message_summary_info) > 61 
                AND instr(message_summary_info, X'7472616E736C6174696F6E4C616E6775616765') > 0 
                AND instr(message_summary_info, X'7472616E736C6174656454657874') > 0"
        );

        let mut statement = db.prepare(&query)?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;

        let mut guids = HashSet::new();
        for guid_result in rows {
            guids.insert(guid_result?);
        }

        Ok(guids)
    }

    /// Parse the group action encoded by the message.
    #[must_use]
    pub fn group_action(&'_ self) -> Option<GroupAction<'_>> {
        GroupAction::from_message(self)
    }

    /// Parse the body component index targeted by a reply.
    fn get_reply_index(&self) -> usize {
        if let Some(parts) = &self.thread_originator_part {
            return match parts.split(':').next() {
                Some(part) => str::parse::<usize>(part).unwrap_or(0),
                None => 0,
            };
        }
        0
    }

    // MARK: SQL
    /// Build the SQL `WHERE` clause described by a [`QueryContext`].
    ///
    /// If `include_recoverable` is `true`, the filter includes messages from the recently deleted messages
    /// table that match the chat IDs. This allows recovery of deleted messages that are still
    /// present in the database but no longer visible in the Messages app.
    pub(crate) fn generate_filter_statement(
        context: &QueryContext,
        include_recoverable: bool,
    ) -> String {
        let mut filters = String::with_capacity(128);

        // Start date filter
        if let Some(start) = context.start {
            let _ = write!(filters, " m.date >= {start}");
        }

        // End date filter
        if let Some(end) = context.end {
            if !filters.is_empty() {
                filters.push_str(" AND ");
            }
            let _ = write!(filters, " m.date <= {end}");
        }

        // Chat ID filter, optionally including recoverable messages
        if let Some(chat_ids) = &context.selected_chat_ids {
            if !filters.is_empty() {
                filters.push_str(" AND ");
            }

            // Allocate the filter string for interpolation
            let ids = chat_ids
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<String>>()
                .join(", ");

            if include_recoverable {
                let _ = write!(filters, " (c.chat_id IN ({ids}) OR d.chat_id IN ({ids}))");
            } else {
                let _ = write!(filters, " c.chat_id IN ({ids})");
            }
        }

        if !filters.is_empty() {
            return format!("WHERE {filters}");
        }
        filters
    }

    /// Count messages matching the provided query context.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::get_connection;
    /// use imessage_database::tables::messages::Message;
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let context = QueryContext::default();
    /// Message::get_count(&conn, &context);
    /// ```
    pub fn get_count(db: &Connection, context: &QueryContext) -> Result<i64, TableError> {
        let mut statement = if context.has_filters() {
            db.prepare_cached(&format!(
                "SELECT
                     COUNT(*)
                 FROM {MESSAGE} as m
                 LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
                 LEFT JOIN {RECENTLY_DELETED} as d ON m.ROWID = d.message_id
                 {}",
                Self::generate_filter_statement(context, true)
            ))
            .or_else(|_| {
                db.prepare_cached(&format!(
                    "SELECT
                         COUNT(*)
                     FROM {MESSAGE} as m
                     LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
                    {}",
                    Self::generate_filter_statement(context, false)
                ))
            })?
        } else {
            db.prepare_cached(&format!("SELECT COUNT(*) FROM {MESSAGE}"))?
        };
        // Execute query, defaulting to zero if it fails
        let count: i64 = statement.query_row([], |r| r.get(0)).unwrap_or(0);

        Ok(count)
    }

    /// Stream messages from the database with optional filters.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use imessage_database::util::dirs::default_db_path;
    /// use imessage_database::tables::table::get_connection;
    /// use imessage_database::tables::{messages::Message, table::Table};
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    /// let context = QueryContext::default();
    ///
    /// let mut statement = Message::stream_rows(&conn, &context).unwrap();
    ///
    /// for message in Message::rows(&mut statement, []).unwrap() {
    ///     println!("{:#?}", message);
    /// }
    /// ```
    pub fn stream_rows<'a>(
        db: &'a Connection,
        context: &'a QueryContext,
    ) -> Result<CachedStatement<'a>, TableError> {
        if !context.has_filters() {
            return Self::get(db);
        }
        Ok(db
            .prepare_cached(&ios_27_newer_query(Some(&Self::generate_filter_statement(
                context, true,
            ))))
            .or_else(|_| {
                db.prepare_cached(&ios_16_newer_query(Some(&Self::generate_filter_statement(
                    context, true,
                ))))
            })
            .or_else(|_| {
                db.prepare_cached(&ios_14_15_query(Some(&Self::generate_filter_statement(
                    context, false,
                ))))
            })
            .or_else(|_| {
                db.prepare_cached(&ios_13_older_query(Some(&Self::generate_filter_statement(
                    context, false,
                ))))
            })?)
    }

    /// Parse the target body component index and GUID from `associated_message_guid`.
    ///
    /// Returns a tuple of (component index, message GUID) if present.
    #[must_use]
    pub fn clean_associated_guid(&self) -> Option<(usize, &str)> {
        if let Some(guid) = &self.associated_message_guid {
            if guid.starts_with("p:") {
                let mut split = guid.split('/');
                let index_str = split.next()?;
                let message_id = split.next()?;
                let index = str::parse::<usize>(&index_str.replace("p:", "")).unwrap_or(0);
                return Some((index, message_id.get(0..36)?));
            } else if guid.starts_with("bp:") {
                return Some((0, guid.get(3..39)?));
            }

            return Some((0, guid.get(0..36)?));
        }
        None
    }

    /// Parse the target body component index for a tapback.
    fn tapback_index(&self) -> usize {
        match self.clean_associated_guid() {
            Some((x, _)) => x,
            None => 0,
        }
    }

    /// Group replies by target body component index.
    pub fn get_replies(&self, db: &Connection) -> Result<HashMap<usize, Vec<Self>>, TableError> {
        let mut out_h: HashMap<usize, Vec<Self>> = HashMap::new();

        // No need to hit the DB if we know we don't have replies
        if self.has_replies() {
            // Use a parameterized filter so the prepared statement can be cached/reused
            let filters = "WHERE m.thread_originator_guid = ?1";

            // `thread_originator_guid` is absent from the iOS 13-era schema.
            let mut statement = db
                .prepare_cached(&ios_27_newer_query(Some(filters)))
                .or_else(|_| db.prepare_cached(&ios_16_newer_query(Some(filters))))
                .or_else(|_| db.prepare_cached(&ios_14_15_query(Some(filters))))?;

            for message in Message::rows(&mut statement, [self.guid.as_str()])? {
                let m = message?;
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

    // MARK: Polls
    /// Load messages that vote on or update the parent poll.
    pub fn get_votes(&self, db: &Connection) -> Result<Vec<Self>, TableError> {
        let mut out_v: Vec<Self> = Vec::new();

        // No need to hit the DB if we know we don't have a poll
        if self.is_poll() {
            // Use a parameterized filter so the prepared statement can be cached/reused
            let filters = "WHERE m.associated_message_guid = ?1";

            // `associated_message_guid` is absent from the iOS 13-era schema.
            let mut statement = db
                .prepare_cached(&ios_27_newer_query(Some(filters)))
                .or_else(|_| db.prepare_cached(&ios_16_newer_query(Some(filters))))
                .or_else(|_| db.prepare_cached(&ios_14_15_query(Some(filters))))?;

            for message in Message::rows(&mut statement, [self.guid.as_str()])? {
                out_v.push(message?);
            }
        }

        Ok(out_v)
    }

    /// Parse this message as a poll, including vote counts and option updates.
    pub fn as_poll(&self, db: &Connection) -> Result<Option<Poll>, MessageError> {
        if self.is_poll()
            && let Some(payload) = self.payload_data(db)
        {
            let mut poll = Poll::from_payload(&payload)?;

            // Get all votes associated with this poll
            let votes = self.get_votes(db).unwrap_or_default();

            // Later poll-option updates are stored as messages referencing the original poll.
            for vote in votes.iter().rev() {
                // The most recent non-vote message is the latest poll update
                // and contains all of the possible options
                if !vote.is_poll_vote()
                    && let Some(vote_payload) = vote.payload_data(db)
                    && let Ok(update) = Poll::from_payload(&vote_payload)
                {
                    poll = update;
                    break;
                }
            }

            // Poll update messages share the same association field but do not cast votes.
            for vote in &votes {
                if vote.is_poll_vote()
                    && let Some(vote_payload) = vote.payload_data(db)
                {
                    poll.count_votes(&vote_payload)?;
                }
            }

            return Ok(Some(poll));
        }

        Ok(None)
    }

    // MARK: Variant
    /// Classify the message using its associated-message fields and app balloon bundle ID.
    #[must_use]
    pub fn variant(&'_ self) -> Variant<'_> {
        // Edited messages expose their original type through `edited_parts`.
        if self.is_edited() {
            return Variant::Edited;
        }

        // Handle different types of associated message types
        if let Some(associated_message_type) = self.associated_message_type {
            match associated_message_type {
                // Standard iMessages with either text or an app payload.
                0 | 2 | 3 => return self.get_app_variant().unwrap_or(Variant::Normal),
                // Tapbacks, added or removed.
                1000 | 2000..=2007 | 3000..=3007 => {
                    if let Some((action, tapback)) = self.get_tapback() {
                        return Variant::Tapback(self.tapback_index(), action, tapback);
                    }
                }
                // A vote was cast on a poll.
                4000 => return Variant::Vote,
                x => return Variant::Unknown(x),
            }
        }

        // Any other rarer cases belong here
        if self.is_shareplay() {
            return Variant::SharePlay;
        }

        Variant::Normal
    }

    /// Classify app-message variants from the balloon bundle ID.
    #[must_use]
    fn get_app_variant(&self) -> Option<Variant<'_>> {
        let bundle_id = parse_balloon_bundle_id(self.balloon_bundle_id.as_deref())?;
        let custom = match bundle_id {
            "com.apple.messages.URLBalloonProvider" => CustomBalloon::URL,
            "com.apple.Handwriting.HandwritingProvider" => CustomBalloon::Handwriting,
            "com.apple.DigitalTouchBalloonProvider" => CustomBalloon::DigitalTouch,
            "com.apple.PassbookUIService.PeerPaymentMessagesExtension" => CustomBalloon::ApplePay,
            "com.apple.ActivityMessagesApp.MessagesExtension" => CustomBalloon::Fitness,
            "com.apple.mobileslideshow.PhotosMessagesApp" => CustomBalloon::Slideshow,
            "com.apple.SafetyMonitorApp.SafetyMonitorMessages" => CustomBalloon::CheckIn,
            "com.apple.findmy.FindMyMessagesApp" => CustomBalloon::FindMy,
            "com.apple.icloud.apps.messages.business.extension" => CustomBalloon::Business,
            "com.apple.messages.Polls" => {
                // Special case: Check if this is the original poll or an update
                if self
                    .associated_message_guid
                    .as_ref()
                    .is_none_or(|id| id == &self.guid)
                {
                    CustomBalloon::Polls
                } else {
                    return Some(Variant::PollUpdate);
                }
            }
            _ => CustomBalloon::Application(bundle_id),
        };
        Some(Variant::App(custom))
    }

    /// Classify tapback action and type from the associated message type.
    #[must_use]
    fn get_tapback(&self) -> Option<(TapbackAction, Tapback<'_>)> {
        match self.associated_message_type? {
            1000 => Some((TapbackAction::Added, Tapback::Sticker)),
            2000 => Some((TapbackAction::Added, Tapback::Loved)),
            2001 => Some((TapbackAction::Added, Tapback::Liked)),
            2002 => Some((TapbackAction::Added, Tapback::Disliked)),
            2003 => Some((TapbackAction::Added, Tapback::Laughed)),
            2004 => Some((TapbackAction::Added, Tapback::Emphasized)),
            2005 => Some((TapbackAction::Added, Tapback::Questioned)),
            2006 => Some((
                TapbackAction::Added,
                Tapback::Emoji(self.associated_message_emoji.as_deref()),
            )),
            2007 => Some((TapbackAction::Added, Tapback::Sticker)),
            3000 => Some((TapbackAction::Removed, Tapback::Loved)),
            3001 => Some((TapbackAction::Removed, Tapback::Liked)),
            3002 => Some((TapbackAction::Removed, Tapback::Disliked)),
            3003 => Some((TapbackAction::Removed, Tapback::Laughed)),
            3004 => Some((TapbackAction::Removed, Tapback::Emphasized)),
            3005 => Some((TapbackAction::Removed, Tapback::Questioned)),
            3006 => Some((
                TapbackAction::Removed,
                Tapback::Emoji(self.associated_message_emoji.as_deref()),
            )),
            3007 => Some((TapbackAction::Removed, Tapback::Sticker)),
            _ => None,
        }
    }

    /// Parse the announcement represented by this message.
    #[must_use]
    pub fn get_announcement(&'_ self) -> Option<Announcement<'_>> {
        if let Some(action) = self.group_action() {
            return Some(Announcement::GroupAction(action));
        }

        if self.is_fully_unsent() {
            return Some(Announcement::FullyUnsent);
        }

        if self.is_kept_audio_message() {
            return Some(Announcement::AudioMessageKept);
        }

        None
    }

    /// Parse the message service.
    #[must_use]
    pub fn service(&'_ self) -> Service<'_> {
        Service::from_name(self.service.as_deref())
    }

    /// Parse the message's raw filter category.
    ///
    /// A raw `0` maps to [`FilterAction::Unfiltered`]; an absent or `NULL` value
    /// maps to `None`.
    #[must_use]
    pub fn filter_action(&self) -> Option<FilterAction> {
        FilterAction::from_code(self.filter_action)
    }

    // MARK: BLOBs
    /// Parse the [`MESSAGE_PAYLOAD`] `BLOB` column as a property list.
    ///
    /// Calling this reads a `BLOB` from the database.
    ///
    /// This column contains data used by iMessage app balloons and can be parsed with
    /// [`parse_ns_keyed_archiver()`](crate::util::plist::parse_ns_keyed_archiver).
    pub fn payload_data(&self, db: &Connection) -> Option<Value> {
        // Read the blob into memory first, then parse from a `Cursor`.
        Value::from_reader(Cursor::new(self.raw_payload_data(db)?)).ok()
    }

    /// Read the raw [`MESSAGE_PAYLOAD`] `BLOB` bytes.
    ///
    /// Calling this reads a `BLOB` from the database.
    ///
    /// This column contains data used by [`HandwrittenMessage`](crate::message_types::handwriting::HandwrittenMessage)s.
    pub fn raw_payload_data(&self, db: &Connection) -> Option<Vec<u8>> {
        let mut buf = Vec::new();
        self.get_blob(db, MESSAGE, MESSAGE_PAYLOAD, self.rowid.into())?
            .read_to_end(&mut buf)
            .ok()?;
        Some(buf)
    }

    /// Parse the [`MESSAGE_SUMMARY_INFO`] `BLOB` column as a property list.
    ///
    /// Calling this reads a `BLOB` from the database.
    ///
    /// This column contains data used by [`edited`](crate::message_types::edited) iMessages.
    pub fn message_summary_info(&self, db: &Connection) -> Option<Value> {
        // Bulk-read the blob, then parse from memory.
        let mut buf = Vec::new();
        self.get_blob(db, MESSAGE, MESSAGE_SUMMARY_INFO, self.rowid.into())?
            .read_to_end(&mut buf)
            .ok()?;
        Value::from_reader(Cursor::new(buf)).ok()
    }

    /// Get a message's [typedstream](crate::util::typedstream) from the [`ATTRIBUTED_BODY`] BLOB column
    ///
    /// Calling this reads a `BLOB` from the database.
    ///
    /// This column contains the message's body text with any other attributes.
    pub fn attributed_body(&self, db: &Connection) -> Option<Vec<u8>> {
        let mut body = vec![];
        self.get_blob(db, MESSAGE, ATTRIBUTED_BODY, self.rowid.into())?
            .read_to_end(&mut body)
            .ok();
        Some(body)
    }

    // MARK: Expressive
    /// Parse the expressive send effect.
    #[must_use]
    pub fn get_expressive(&'_ self) -> Expressive<'_> {
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

    /// Query a single message by [`GUID`](Self::guid).
    ///
    /// # Example
    /// ```no_run
    /// use imessage_database::{
    ///     tables::{
    ///         messages::Message,
    ///         table::get_connection,
    ///     },
    ///     util::dirs::default_db_path,
    /// };
    ///
    /// let db_path = default_db_path();
    /// let conn = get_connection(&db_path).unwrap();
    ///
    /// if let Ok(mut message) = Message::from_guid("example-guid", &conn) {
    ///     if let Ok(body) = message.parse_body(&conn) {
    ///         message.apply_body(body);
    ///     }
    ///     println!("{:#?}", message)
    /// }
    /// ```
    pub fn from_guid(guid: &str, db: &Connection) -> Result<Self, TableError> {
        let mut statement = db
            .prepare_cached(&ios_27_newer_query(Some("WHERE m.guid = ?1")))
            .or_else(|_| db.prepare_cached(&ios_16_newer_query(Some("WHERE m.guid = ?1"))))
            .or_else(|_| db.prepare_cached(&ios_14_15_query(Some("WHERE m.guid = ?1"))))
            .or_else(|_| db.prepare_cached(&ios_13_older_query(Some("WHERE m.guid = ?1"))))?;

        Message::row(&mut statement, [guid])
    }
}

// MARK: Fixture
#[cfg(test)]
impl Message {
    #[must_use]
    /// Build a blank test message with default values.
    pub fn blank() -> Message {
        use std::vec;

        Message {
            rowid: i32::default(),
            guid: String::default(),
            text: None,
            service: Some("iMessage".to_string()),
            handle_id: Some(i32::default()),
            destination_caller_id: None,
            subject: None,
            date: i64::default(),
            date_read: i64::default(),
            date_delivered: i64::default(),
            is_from_me: false,
            is_read: false,
            item_type: 0,
            other_handle: None,
            share_status: false,
            share_direction: None,
            group_title: None,
            group_action_type: 0,
            associated_message_guid: None,
            associated_message_type: None,
            balloon_bundle_id: None,
            expressive_send_style_id: None,
            thread_originator_guid: None,
            thread_originator_part: None,
            date_edited: 0,
            associated_message_emoji: None,
            chat_id: None,
            num_attachments: 0,
            deleted_from: None,
            num_replies: 0,
            filter_action: None,
            filter_sub_action: None,
            components: vec![],
            edited_parts: None,
        }
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use rusqlite::Connection;

    use crate::tables::messages::Message;

    fn diagnostic_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "
            CREATE TABLE message (
                ROWID INTEGER PRIMARY KEY,
                date INTEGER
            );
            CREATE TABLE chat_message_join (
                chat_id INTEGER,
                message_id INTEGER
            );
            INSERT INTO message (ROWID, date) VALUES (1, 10), (2, 20);
            INSERT INTO chat_message_join (chat_id, message_id) VALUES (1, 1);
            ",
        )
        .unwrap();
        db
    }

    #[test]
    fn diagnostic_omits_recoverable_count_when_table_is_missing() {
        let db = diagnostic_db();

        let diagnostic = Message::run_diagnostic(&db).unwrap();

        assert_eq!(diagnostic.total_messages, 2);
        assert_eq!(diagnostic.messages_without_chat, 1);
        assert_eq!(diagnostic.recoverable_messages, None);
    }

    #[test]
    fn diagnostic_counts_recoverable_messages_when_table_exists() {
        let db = diagnostic_db();
        db.execute_batch(
            "
            CREATE TABLE chat_recoverable_message_join (
                chat_id INTEGER,
                message_id INTEGER
            );
            INSERT INTO chat_recoverable_message_join (chat_id, message_id) VALUES (1, 2);
            ",
        )
        .unwrap();

        let diagnostic = Message::run_diagnostic(&db).unwrap();

        assert_eq!(diagnostic.recoverable_messages, Some(1));
    }
}
