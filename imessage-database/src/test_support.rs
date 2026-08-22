//! Shared in-memory schema builders for capability-driven tests.

use rusqlite::Connection;

/// Build an in-memory database whose `message` table carries the oldest
/// supported core layout, with each optional feature appended independently.
///
/// The toggles mirror [`Capabilities`](crate::tables::capabilities::Capabilities)
/// probes: `filters` adds the `filter_action`/`filter_sub_action` pair,
/// `recoverable` creates `chat_recoverable_message_join`, and `replies` adds
/// the `thread_originator_*` columns. `associated_message_guid` is always
/// present.
#[must_use]
pub(crate) fn schema_db(filters: bool, recoverable: bool, replies: bool) -> Connection {
    let filter_columns = if filters {
        ",\n                filter_action INTEGER DEFAULT 0,\n                filter_sub_action INTEGER DEFAULT 0"
    } else {
        ""
    };
    let reply_columns = if replies {
        "thread_originator_guid TEXT,\n                thread_originator_part TEXT,\n                "
    } else {
        ""
    };
    let recoverable_table = if recoverable {
        "CREATE TABLE chat_recoverable_message_join (chat_id INTEGER, message_id INTEGER);"
    } else {
        ""
    };

    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(&format!(
        "
        CREATE TABLE message (
            ROWID INTEGER PRIMARY KEY,
            guid TEXT,
            text TEXT,
            service TEXT,
            handle_id INTEGER,
            destination_caller_id TEXT,
            subject TEXT,
            date INTEGER,
            date_read INTEGER,
            date_delivered INTEGER,
            is_from_me INTEGER,
            is_read INTEGER,
            item_type INTEGER,
            other_handle INTEGER,
            share_status INTEGER,
            share_direction INTEGER,
            group_title TEXT,
            group_action_type INTEGER,
            associated_message_guid TEXT,
            associated_message_type INTEGER,
            balloon_bundle_id TEXT,
            expressive_send_style_id TEXT,{reply_columns}
            date_edited INTEGER,
            associated_message_emoji TEXT{filter_columns}
        );
        CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);
        CREATE TABLE message_attachment_join (message_id INTEGER, attachment_id INTEGER);
        {recoverable_table}
        "
    ))
    .unwrap();
    db
}
