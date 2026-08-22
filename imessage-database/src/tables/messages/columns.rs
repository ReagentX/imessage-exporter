/*!
 Column layout resolution and row decoding for the `message` table.

 [`Message`] reads 32 columns whose ordinals differ per schema: the composed
 query projects the recognized columns contiguously, while custom queries
 (such as `m.*` projections) may append derived columns after every `message`
 column. Decoding each field by name repeats a scan of the result set for
 every field of every row. [`MessageColumns`] resolves that mapping once per
 statement so rows decode by ordinal, and [`Message::from_row_named`] is the
 fallback for layouts it rejects.
*/

use rusqlite::{Result, Row, Statement, types::FromSql};

use crate::tables::messages::Message;

// MARK: Columns
/// Recognized `message` source columns in canonical projection order.
///
/// [`Capabilities`](crate::tables::capabilities::Capabilities) probes these
/// names against the live schema; query composition projects exactly the
/// subset that exists, qualified with `m.`.
pub(crate) const MESSAGE_COLUMNS: [&str; 26] = [
    "rowid",
    "guid",
    "text",
    "service",
    "handle_id",
    "destination_caller_id",
    "subject",
    "date",
    "date_read",
    "date_delivered",
    "is_from_me",
    "is_read",
    "item_type",
    "other_handle",
    "share_status",
    "share_direction",
    "group_title",
    "group_action_type",
    "associated_message_guid",
    "associated_message_type",
    "balloon_bundle_id",
    "expressive_send_style_id",
    "thread_originator_guid",
    "thread_originator_part",
    "date_edited",
    "associated_message_emoji",
];

/// Size of the stack buffer used to case-fold column names. The two longest
/// recognized names, `associated_message_emoji` and
/// `expressive_send_style_id`, each occupy 24 bytes.
const LONGEST_COL: usize = 24;

// MARK: Layout
/// Resolved result-set ordinals for the columns [`Message`] reads.
///
/// [`Message::rows`] resolves this layout once because repeated named reads
/// scan the result set from column zero. Each comparison also calls
/// `sqlite3_column_name` through FFI. The `m.*` heads append derived
/// columns after every `message` column, making those repeated scans nearly
/// full.
///
/// Required ordinals correspond to the fallible reads in
/// [`Message::from_row_named`]. An absent required column rejects the layout;
/// optional ordinals preserve the defaults used for schema-specific columns.
#[derive(Debug)]
pub(super) struct MessageColumns {
    // Required.
    rowid: usize,
    guid: usize,
    date: usize,
    is_from_me: usize,
    num_attachments: usize,
    num_replies: usize,
    // Optional.
    text: Option<usize>,
    service: Option<usize>,
    handle_id: Option<usize>,
    destination_caller_id: Option<usize>,
    subject: Option<usize>,
    date_read: Option<usize>,
    date_delivered: Option<usize>,
    is_read: Option<usize>,
    item_type: Option<usize>,
    other_handle: Option<usize>,
    share_status: Option<usize>,
    share_direction: Option<usize>,
    group_title: Option<usize>,
    group_action_type: Option<usize>,
    associated_message_guid: Option<usize>,
    associated_message_type: Option<usize>,
    balloon_bundle_id: Option<usize>,
    expressive_send_style_id: Option<usize>,
    thread_originator_guid: Option<usize>,
    thread_originator_part: Option<usize>,
    date_edited: Option<usize>,
    associated_message_emoji: Option<usize>,
    chat_id: Option<usize>,
    deleted_from: Option<usize>,
    filter_action: Option<usize>,
    filter_sub_action: Option<usize>,
}

impl MessageColumns {
    /// Number of mapped fields. Tests compare this with `slots`; resolution
    /// scans the complete result set independently.
    #[cfg(test)]
    const FIELDS: usize = 32;

    /// Map every recognized column name to its ordinal, or return `None` when a
    /// required column is absent.
    ///
    /// The first case-insensitive match wins, matching
    /// [`Statement::column_index`](rusqlite::Statement::column_index). This is
    /// required because custom projections may duplicate names and schemas may
    /// report `ROWID` in uppercase.
    ///
    /// When the schema may change concurrently, resolve through a [`Row`] after
    /// the first step: `sqlite3_step` may recompile the statement.
    pub(super) fn resolve(stmt: &Statement<'_>) -> Option<Self> {
        let mut rowid = None;
        let mut guid = None;
        let mut date = None;
        let mut is_from_me = None;
        let mut num_attachments = None;
        let mut num_replies = None;
        let mut text = None;
        let mut service = None;
        let mut handle_id = None;
        let mut destination_caller_id = None;
        let mut subject = None;
        let mut date_read = None;
        let mut date_delivered = None;
        let mut is_read = None;
        let mut item_type = None;
        let mut other_handle = None;
        let mut share_status = None;
        let mut share_direction = None;
        let mut group_title = None;
        let mut group_action_type = None;
        let mut associated_message_guid = None;
        let mut associated_message_type = None;
        let mut balloon_bundle_id = None;
        let mut expressive_send_style_id = None;
        let mut thread_originator_guid = None;
        let mut thread_originator_part = None;
        let mut date_edited = None;
        let mut associated_message_emoji = None;
        let mut chat_id = None;
        let mut deleted_from = None;
        let mut filter_action = None;
        let mut filter_sub_action = None;

        for idx in 0..stmt.column_count() {
            let Ok(name) = stmt.column_name(idx) else {
                break;
            };

            // Fold into a stack buffer: matching is ASCII-case-insensitive and
            // allocates nothing. Longer names cannot match a recognized column.
            let bytes = name.as_bytes();
            if bytes.len() > LONGEST_COL {
                continue;
            }
            let mut folded = [0u8; LONGEST_COL];
            for (dst, src) in folded.iter_mut().zip(bytes) {
                *dst = src.to_ascii_lowercase();
            }

            let slot = match &folded[..bytes.len()] {
                b"rowid" => &mut rowid,
                b"guid" => &mut guid,
                b"date" => &mut date,
                b"is_from_me" => &mut is_from_me,
                b"num_attachments" => &mut num_attachments,
                b"num_replies" => &mut num_replies,
                b"text" => &mut text,
                b"service" => &mut service,
                b"handle_id" => &mut handle_id,
                b"destination_caller_id" => &mut destination_caller_id,
                b"subject" => &mut subject,
                b"date_read" => &mut date_read,
                b"date_delivered" => &mut date_delivered,
                b"is_read" => &mut is_read,
                b"item_type" => &mut item_type,
                b"other_handle" => &mut other_handle,
                b"share_status" => &mut share_status,
                b"share_direction" => &mut share_direction,
                b"group_title" => &mut group_title,
                b"group_action_type" => &mut group_action_type,
                b"associated_message_guid" => &mut associated_message_guid,
                b"associated_message_type" => &mut associated_message_type,
                b"balloon_bundle_id" => &mut balloon_bundle_id,
                b"expressive_send_style_id" => &mut expressive_send_style_id,
                b"thread_originator_guid" => &mut thread_originator_guid,
                b"thread_originator_part" => &mut thread_originator_part,
                b"date_edited" => &mut date_edited,
                b"associated_message_emoji" => &mut associated_message_emoji,
                b"chat_id" => &mut chat_id,
                b"deleted_from" => &mut deleted_from,
                b"filter_action" => &mut filter_action,
                b"filter_sub_action" => &mut filter_sub_action,
                _ => continue,
            };

            // First occurrence wins, matching `Statement::column_index`.
            if slot.is_none() {
                *slot = Some(idx);
            }
        }

        Some(Self {
            rowid: rowid?,
            guid: guid?,
            date: date?,
            is_from_me: is_from_me?,
            num_attachments: num_attachments?,
            num_replies: num_replies?,
            text,
            service,
            handle_id,
            destination_caller_id,
            subject,
            date_read,
            date_delivered,
            is_read,
            item_type,
            other_handle,
            share_status,
            share_direction,
            group_title,
            group_action_type,
            associated_message_guid,
            associated_message_type,
            balloon_bundle_id,
            expressive_send_style_id,
            thread_originator_guid,
            thread_originator_part,
            date_edited,
            associated_message_emoji,
            chat_id,
            deleted_from,
            filter_action,
            filter_sub_action,
        })
    }
}

/// Read a nullable column by resolved ordinal. Missing columns and conversion
/// failures yield `None`, matching [`Message::from_row_named`].
fn nullable<T: FromSql>(row: &Row, idx: Option<usize>) -> Option<T> {
    idx.and_then(|idx| row.get(idx).unwrap_or(None))
}

/// Read a non-nullable column by resolved ordinal. Missing columns and
/// conversion failures yield `T::default()`, matching
/// [`Message::from_row_named`].
fn defaulted<T: FromSql + Default>(row: &Row, idx: Option<usize>) -> T {
    idx.map_or_else(T::default, |idx| row.get(idx).unwrap_or_default())
}

// MARK: Decode
impl Message {
    /// Deserialize a [`Message`] through a resolved column layout.
    ///
    /// Required reads and optional defaults match
    /// [`from_row_named`](Self::from_row_named).
    pub(super) fn from_row_mapped(row: &Row, columns: &MessageColumns) -> Result<Message> {
        Ok(Message {
            rowid: row.get(columns.rowid)?,
            guid: row.get(columns.guid)?,
            text: nullable(row, columns.text),
            service: nullable(row, columns.service),
            handle_id: nullable(row, columns.handle_id),
            destination_caller_id: nullable(row, columns.destination_caller_id),
            subject: nullable(row, columns.subject),
            date: row.get(columns.date)?,
            date_read: defaulted(row, columns.date_read),
            date_delivered: defaulted(row, columns.date_delivered),
            is_from_me: row.get(columns.is_from_me)?,
            is_read: defaulted(row, columns.is_read),
            item_type: defaulted(row, columns.item_type),
            other_handle: nullable(row, columns.other_handle),
            share_status: defaulted(row, columns.share_status),
            share_direction: nullable(row, columns.share_direction),
            group_title: nullable(row, columns.group_title),
            group_action_type: defaulted(row, columns.group_action_type),
            associated_message_guid: nullable(row, columns.associated_message_guid),
            associated_message_type: nullable(row, columns.associated_message_type),
            balloon_bundle_id: nullable(row, columns.balloon_bundle_id),
            expressive_send_style_id: nullable(row, columns.expressive_send_style_id),
            thread_originator_guid: nullable(row, columns.thread_originator_guid),
            thread_originator_part: nullable(row, columns.thread_originator_part),
            date_edited: defaulted(row, columns.date_edited),
            associated_message_emoji: nullable(row, columns.associated_message_emoji),
            chat_id: nullable(row, columns.chat_id),
            num_attachments: row.get(columns.num_attachments)?,
            deleted_from: nullable(row, columns.deleted_from),
            num_replies: row.get(columns.num_replies)?,
            filter_action: nullable(row, columns.filter_action),
            filter_sub_action: nullable(row, columns.filter_sub_action),
            components: vec![],
            edited_parts: None,
        })
    }

    /// Build a [`Message`] from a row using named columns.
    pub(super) fn from_row_named(row: &Row) -> Result<Message> {
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
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;

    use rusqlite::{Connection, Statement};

    use super::{LONGEST_COL, Message, MessageColumns};
    use crate::tables::{
        capabilities::Capabilities,
        messages::query_parts::message_query,
        table::{Table, get_connection},
    };

    /// A `message` schema whose column order differs from
    /// [`super::MESSAGE_COLUMNS`]. Its `m.*` result exercises name resolution
    /// rather than projection order.
    const SCRAMBLED_SCHEMA: &str = "
        CREATE TABLE message (
            associated_message_emoji TEXT,
            date_edited INTEGER,
            ROWID INTEGER PRIMARY KEY,
            filter_sub_action INTEGER,
            subject TEXT,
            thread_originator_part TEXT,
            guid TEXT,
            is_read INTEGER,
            group_title TEXT,
            date INTEGER,
            handle_id INTEGER,
            balloon_bundle_id TEXT,
            share_direction INTEGER,
            associated_message_type INTEGER,
            item_type INTEGER,
            date_delivered INTEGER,
            service TEXT,
            filter_action INTEGER,
            other_handle INTEGER,
            expressive_send_style_id TEXT,
            is_from_me INTEGER,
            group_action_type INTEGER,
            destination_caller_id TEXT,
            thread_originator_guid TEXT,
            text TEXT,
            date_read INTEGER,
            associated_message_guid TEXT,
            share_status INTEGER
        );
        CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER, message_date INTEGER);
        CREATE TABLE chat_recoverable_message_join (chat_id INTEGER, message_id INTEGER, delete_date INTEGER);
        CREATE TABLE message_attachment_join (message_id INTEGER, attachment_id INTEGER);

        INSERT INTO message (
            ROWID, guid, text, service, handle_id, destination_caller_id, subject, date,
            date_read, date_delivered, is_from_me, is_read, item_type, other_handle,
            share_status, share_direction, group_title, group_action_type,
            associated_message_guid, associated_message_type, balloon_bundle_id,
            expressive_send_style_id, thread_originator_guid, thread_originator_part,
            date_edited, associated_message_emoji, filter_action, filter_sub_action
        ) VALUES (
            1, 'guid-one', 'hello', 'iMessage', 7, 'me@example.com', 'subject', 100,
            101, 102, 1, 1, 2, 8,
            1, 0, 'group', 3,
            'p:0/guid-two', 2000, 'com.apple.example',
            'style', 'guid-two', '0',
            103, '!', 2, 4
        );
        INSERT INTO message (ROWID, guid, date, is_from_me) VALUES (2, 'guid-two', 200, 0);
        INSERT INTO chat_message_join (chat_id, message_id) VALUES (9, 1);
        CREATE TABLE attachment (ROWID INTEGER PRIMARY KEY);
    ";

    /// The on-disk fixture contains 91 `message` columns, so `m.*` places
    /// appended derived columns far from column zero.
    fn ventura_db() -> Connection {
        let path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        get_connection(&path).unwrap()
    }

    fn scrambled_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(SCRAMBLED_SCHEMA).unwrap();
        db
    }

    /// Prepare the composed message query for `db`'s probed schema.
    fn prepared(db: &Connection) -> Statement<'_> {
        let capabilities = Capabilities::determine(db).unwrap();
        db.prepare(&message_query(&capabilities, None)).unwrap()
    }

    /// Every [`MessageColumns`] field paired with its column name and resolved
    /// ordinal, in the order used by explicit query heads.
    ///
    /// This mapping remains independent of [`MessageColumns::resolve`]:
    /// [`assert_resolve_matches_rusqlite`] can therefore detect a name wired to
    /// the wrong field.
    fn slots(columns: &MessageColumns) -> Vec<(&'static str, Option<usize>)> {
        vec![
            ("rowid", Some(columns.rowid)),
            ("guid", Some(columns.guid)),
            ("text", columns.text),
            ("service", columns.service),
            ("handle_id", columns.handle_id),
            ("destination_caller_id", columns.destination_caller_id),
            ("subject", columns.subject),
            ("date", Some(columns.date)),
            ("date_read", columns.date_read),
            ("date_delivered", columns.date_delivered),
            ("is_from_me", Some(columns.is_from_me)),
            ("is_read", columns.is_read),
            ("item_type", columns.item_type),
            ("other_handle", columns.other_handle),
            ("share_status", columns.share_status),
            ("share_direction", columns.share_direction),
            ("group_title", columns.group_title),
            ("group_action_type", columns.group_action_type),
            ("associated_message_guid", columns.associated_message_guid),
            ("associated_message_type", columns.associated_message_type),
            ("balloon_bundle_id", columns.balloon_bundle_id),
            ("expressive_send_style_id", columns.expressive_send_style_id),
            ("thread_originator_guid", columns.thread_originator_guid),
            ("thread_originator_part", columns.thread_originator_part),
            ("date_edited", columns.date_edited),
            ("associated_message_emoji", columns.associated_message_emoji),
            ("chat_id", columns.chat_id),
            ("num_attachments", Some(columns.num_attachments)),
            ("deleted_from", columns.deleted_from),
            ("num_replies", Some(columns.num_replies)),
            ("filter_action", columns.filter_action),
            ("filter_sub_action", columns.filter_sub_action),
        ]
    }

    /// Assert that every resolved ordinal equals `rusqlite`'s name lookup. This
    /// verifies field wiring, case-insensitive matching, and first-match
    /// behavior for duplicate names.
    fn assert_resolve_matches_rusqlite(stmt: &Statement<'_>) {
        let columns = MessageColumns::resolve(stmt).expect("layout should resolve");
        for (name, resolved) in slots(&columns) {
            assert_eq!(
                resolved,
                stmt.column_index(name).ok(),
                "ordinal for `{name}` disagrees with rusqlite"
            );
        }
    }

    fn assert_same_message(mapped: &Message, named: &Message) {
        assert_eq!(mapped.rowid, named.rowid, "rowid");
        assert_eq!(mapped.guid, named.guid, "guid");
        assert_eq!(mapped.text, named.text, "text");
        assert_eq!(mapped.service, named.service, "service");
        assert_eq!(mapped.handle_id, named.handle_id, "handle_id");
        assert_eq!(
            mapped.destination_caller_id, named.destination_caller_id,
            "destination_caller_id"
        );
        assert_eq!(mapped.subject, named.subject, "subject");
        assert_eq!(mapped.date, named.date, "date");
        assert_eq!(mapped.date_read, named.date_read, "date_read");
        assert_eq!(
            mapped.date_delivered, named.date_delivered,
            "date_delivered"
        );
        assert_eq!(mapped.is_from_me, named.is_from_me, "is_from_me");
        assert_eq!(mapped.is_read, named.is_read, "is_read");
        assert_eq!(mapped.item_type, named.item_type, "item_type");
        assert_eq!(mapped.other_handle, named.other_handle, "other_handle");
        assert_eq!(mapped.share_status, named.share_status, "share_status");
        assert_eq!(
            mapped.share_direction, named.share_direction,
            "share_direction"
        );
        assert_eq!(mapped.group_title, named.group_title, "group_title");
        assert_eq!(
            mapped.group_action_type, named.group_action_type,
            "group_action_type"
        );
        assert_eq!(
            mapped.associated_message_guid, named.associated_message_guid,
            "associated_message_guid"
        );
        assert_eq!(
            mapped.associated_message_type, named.associated_message_type,
            "associated_message_type"
        );
        assert_eq!(
            mapped.balloon_bundle_id, named.balloon_bundle_id,
            "balloon_bundle_id"
        );
        assert_eq!(
            mapped.expressive_send_style_id, named.expressive_send_style_id,
            "expressive_send_style_id"
        );
        assert_eq!(
            mapped.thread_originator_guid, named.thread_originator_guid,
            "thread_originator_guid"
        );
        assert_eq!(
            mapped.thread_originator_part, named.thread_originator_part,
            "thread_originator_part"
        );
        assert_eq!(mapped.date_edited, named.date_edited, "date_edited");
        assert_eq!(
            mapped.associated_message_emoji, named.associated_message_emoji,
            "associated_message_emoji"
        );
        assert_eq!(mapped.chat_id, named.chat_id, "chat_id");
        assert_eq!(
            mapped.num_attachments, named.num_attachments,
            "num_attachments"
        );
        assert_eq!(mapped.deleted_from, named.deleted_from, "deleted_from");
        assert_eq!(mapped.num_replies, named.num_replies, "num_replies");
        assert_eq!(mapped.filter_action, named.filter_action, "filter_action");
        assert_eq!(
            mapped.filter_sub_action, named.filter_sub_action,
            "filter_sub_action"
        );
    }

    /// Compare [`Message::rows`] with direct name-based deserialization of
    /// `query`.
    fn assert_paths_agree(db: &Connection, query: &str) {
        let mut stmt = db.prepare(query).unwrap();
        let mapped: Vec<Message> = Message::rows(&mut stmt, [])
            .unwrap()
            .map(Result::unwrap)
            .collect();

        let mut stmt = db.prepare(query).unwrap();
        let named: Vec<Message> = stmt
            .query_map([], Message::from_row_named)
            .unwrap()
            .map(Result::unwrap)
            .collect();

        assert!(!mapped.is_empty(), "fixture produced no rows");
        assert_eq!(mapped.len(), named.len(), "row counts differ");
        for (mapped, named) in mapped.iter().zip(&named) {
            assert_same_message(mapped, named);
        }
    }

    #[test]
    fn every_column_name_fits_the_fold_buffer() {
        // A name longer than the buffer is skipped before the match, so it
        // would silently never resolve.
        let db = ventura_db();
        let columns = MessageColumns::resolve(&prepared(&db)).unwrap();
        for (name, _) in slots(&columns) {
            assert!(name.len() <= LONGEST_COL, "`{name}` exceeds LONGEST_COL");
        }
    }

    #[test]
    fn slots_cover_every_resolved_field() {
        let db = ventura_db();
        let columns = MessageColumns::resolve(&prepared(&db)).unwrap();
        assert_eq!(slots(&columns).len(), MessageColumns::FIELDS);
    }

    #[test]
    fn resolve_matches_rusqlite_for_composed_head() {
        let db = ventura_db();
        assert_resolve_matches_rusqlite(&prepared(&db));

        let scrambled = scrambled_db();
        assert_resolve_matches_rusqlite(&prepared(&scrambled));
    }

    #[test]
    fn resolve_matches_rusqlite_for_wildcard_projection() {
        // Custom queries may project `m.*`; resolution must still work when
        // derived columns follow every `message` column.
        let db = ventura_db();
        let stmt = db
            .prepare(
                "
                SELECT m.*, c.chat_id,
                    (SELECT COUNT(*) FROM message_attachment_join a WHERE m.ROWID = a.message_id) as num_attachments,
                    0 as num_replies
                FROM message as m
                LEFT JOIN chat_message_join as c ON m.ROWID = c.message_id
                ",
            )
            .unwrap();
        assert_resolve_matches_rusqlite(&stmt);
    }

    fn assert_exact_explicit_projection(stmt: &Statement<'_>) {
        let columns = MessageColumns::resolve(stmt).unwrap();

        // Exact count catches drift between the composed projection and the
        // mapped fields. Ordinal equality constrains the composer's column
        // order, not deserialization.
        assert_eq!(stmt.column_count(), slots(&columns).len());
        for (idx, (name, resolved)) in slots(&columns).into_iter().enumerate() {
            assert_eq!(resolved, Some(idx), "`{name}` is not at ordinal {idx}");
        }
    }

    #[test]
    fn composed_head_selects_exactly_what_message_reads() {
        let ventura = ventura_db();
        assert_exact_explicit_projection(&prepared(&ventura));

        let scrambled = scrambled_db();
        assert_exact_explicit_projection(&prepared(&scrambled));
    }

    #[test]
    fn source_qualified_head_has_one_chat_id() {
        let db = ventura_db();
        let stmt = prepared(&db);

        let matches: Vec<usize> = stmt
            .column_names()
            .iter()
            .enumerate()
            .filter(|(_, name)| name.eq_ignore_ascii_case("chat_id"))
            .map(|(idx, _)| idx)
            .collect();
        assert_eq!(matches.len(), 1, "query exposed duplicate chat_id columns");

        let columns = MessageColumns::resolve(&stmt).unwrap();
        assert_eq!(columns.chat_id, Some(matches[0]));
    }

    #[test]
    fn resolve_reads_uppercase_column_names() {
        // `message.ROWID` is declared uppercase, and SQLite reports the
        // declared name even where the head writes `rowid`, so a
        // case-sensitive match would fail to resolve a required column.
        let db = ventura_db();
        let stmt = prepared(&db);
        assert_eq!(stmt.column_name(0).unwrap(), "ROWID");
        assert_eq!(MessageColumns::resolve(&stmt).unwrap().rowid, 0);
    }

    #[test]
    fn paths_agree_for_composed_query() {
        let ventura = ventura_db();
        let capabilities = Capabilities::determine(&ventura).unwrap();
        assert_paths_agree(&ventura, &message_query(&capabilities, None));

        let scrambled = scrambled_db();
        let capabilities = Capabilities::determine(&scrambled).unwrap();
        assert_paths_agree(&scrambled, &message_query(&capabilities, None));
    }

    #[test]
    fn resolve_ignores_column_order() {
        // No mapped field occupies its explicit-projection ordinal. Correct
        // values therefore depend on name resolution.
        let db = scrambled_db();
        let mut stmt = prepared(&db);
        let messages: Vec<Message> = Message::rows(&mut stmt, [])
            .unwrap()
            .map(Result::unwrap)
            .collect();

        let first = &messages[0];
        assert_eq!(first.rowid, 1);
        assert_eq!(first.guid, "guid-one");
        assert_eq!(first.text.as_deref(), Some("hello"));
        assert_eq!(first.date, 100);
        assert!(first.is_from_me);
        assert_eq!(first.item_type, 2);
        assert_eq!(first.date_edited, 103);
        assert_eq!(first.associated_message_emoji.as_deref(), Some("!"));
        assert_eq!(first.filter_action, Some(2));
        assert_eq!(first.filter_sub_action, Some(4));
        assert_eq!(first.chat_id, Some(9));
    }

    #[test]
    fn resolve_defaults_absent_optional_columns() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "
            CREATE TABLE message (
                ROWID INTEGER PRIMARY KEY,
                guid TEXT,
                date INTEGER,
                is_from_me INTEGER,
                text TEXT
            );
            CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);
            CREATE TABLE message_attachment_join (message_id INTEGER, attachment_id INTEGER);
            INSERT INTO message (ROWID, guid, date, is_from_me, text)
                VALUES (1, 'guid-one', 100, 1, 'hello');
            ",
        )
        .unwrap();

        let mut stmt = prepared(&db);
        let columns = MessageColumns::resolve(&stmt).expect("required columns are present");
        assert_eq!(columns.date_edited, None);
        // The composer always projects the filter aliases, so their ordinals
        // resolve even though the schema lacks the columns.
        assert!(columns.filter_action.is_some());

        let messages: Vec<Message> = Message::rows(&mut stmt, [])
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let first = &messages[0];
        assert_eq!(first.date_edited, 0);
        assert_eq!(first.filter_action, None);
        assert_eq!(first.item_type, 0);
        assert!(!first.is_read);
    }

    #[test]
    fn resolve_refuses_a_missing_required_column() {
        let db = scrambled_db();
        // No `num_replies`, which `from_row_named` reads with `?`.
        let stmt = db
            .prepare("SELECT rowid, guid, date, is_from_me, 0 as num_attachments FROM message")
            .unwrap();

        assert!(MessageColumns::resolve(&stmt).is_none());
    }

    #[test]
    fn wrong_names_are_not_decoded_positionally() {
        let db = scrambled_db();

        // Every value has a type accepted by the required reads, but no name is
        // recognized. Resolution must reject the projection instead of
        // interpreting its ordinals as message fields.
        let projection: Vec<String> = (0..MessageColumns::FIELDS)
            .map(|idx| {
                if idx == 1 {
                    format!("'not-a-guid' as col_{idx}")
                } else {
                    format!("{idx} as col_{idx}")
                }
            })
            .collect();
        let query = format!("SELECT {} FROM message", projection.join(", "));

        let stmt = db.prepare(&query).unwrap();
        assert!(MessageColumns::resolve(&stmt).is_none());

        let mut stmt = db.prepare(&query).unwrap();
        let first = Message::rows(&mut stmt, []).unwrap().next().unwrap();
        assert!(first.is_err(), "wrong names must not deserialize");
    }
}
