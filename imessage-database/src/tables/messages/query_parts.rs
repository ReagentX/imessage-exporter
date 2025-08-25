/*!
 Query data to ensure query compatibility with older known schemas

 - If the database has `chat_recoverable_message_join`, we can restore some deleted messages.
 - If database has `thread_originator_guid`, we can parse replies, otherwise default to 0
*/

use std::sync::LazyLock;

use crate::tables::{
    messages::message::COLS,
    table::{CHAT_MESSAGE_JOIN, MESSAGE, MESSAGE_ATTACHMENT_JOIN, RECENTLY_DELETED},
};

/// macOS Ventura+ and i0S 16+ schema, interpolated with required columns for performance
static IOS_16_NEWER_HEAD: LazyLock<String> = LazyLock::new(|| {
    format!("
SELECT
    {COLS},
    c.chat_id,
    (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
    d.chat_id as deleted_from,
    (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
FROM
    {MESSAGE} as m
LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
LEFT JOIN {RECENTLY_DELETED} as d ON m.ROWID = d.message_id
")
});

/// macOS Big Sur to Monterey, iOS 14 to iOS 15 schema
static IOS_14_15_HEAD: LazyLock<String> = LazyLock::new(|| {
    format!("
SELECT
    *,
    c.chat_id,
    (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
    NULL as deleted_from,
    (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
FROM
    {MESSAGE} as m
LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
")
});

/// macOS Catalina, iOS 13 and older schema
static IOS_13_OLDER_HEAD: LazyLock<String> = LazyLock::new(|| {
    format!("
SELECT
    *,
    c.chat_id,
    (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
    NULL as deleted_from,
    0 as num_replies
FROM
    {MESSAGE} as m
LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
")
});

const ORDER_BY: &str = "
ORDER BY
    m.date;
";

/// Generate a SQL Query compatible with the macOS Ventura+ and i0S 16+ schema
pub(crate) fn ios_16_newer_query(filters: Option<&str>) -> String {
    format!(
        "{}{}{}",
        *IOS_16_NEWER_HEAD,
        filters.unwrap_or_default(),
        ORDER_BY
    )
}

/// Generate a SQL Query compatible with the macOS Big Sur to Monterey, iOS 14 to iOS 15 schema
pub(crate) fn ios_14_15_query(filters: Option<&str>) -> String {
    format!(
        "{}{}{}",
        *IOS_14_15_HEAD,
        filters.unwrap_or_default(),
        ORDER_BY
    )
}

/// Generate a SQL Query compatible with the macOS Catalina, iOS 13 and older schema
pub(crate) fn ios_13_older_query(filters: Option<&str>) -> String {
    format!(
        "{}{}{}",
        *IOS_13_OLDER_HEAD,
        filters.unwrap_or_default(),
        ORDER_BY
    )
}
