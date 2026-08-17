/*!
 Message query templates for supported database schemas.

 - If the database has `chat_recoverable_message_join`, we can restore some deleted messages.
 - If the database has `thread_originator_guid`, we can count replies.
 - If the database has `filter_action`, we can read message filter categories.

 Explicit heads select the same columns in positional decode order. `SELECT *`
 heads use named decoding because their source-column order varies by schema.
*/

use std::sync::LazyLock;

use crate::tables::{
    messages::message::COLS,
    table::{CHAT_MESSAGE_JOIN, MESSAGE, MESSAGE_ATTACHMENT_JOIN, RECENTLY_DELETED},
};

// MARK: Queries
/// Query head for schemas with `filter_action` and `filter_sub_action` columns.
static IOS_27_NEWER_HEAD: LazyLock<String> = LazyLock::new(|| {
    format!("
SELECT
    {COLS},
    c.chat_id,
    (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
    d.chat_id as deleted_from,
    (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies,
    m.filter_action,
    m.filter_sub_action
FROM
    {MESSAGE} as m
LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
LEFT JOIN {RECENTLY_DELETED} as d ON m.ROWID = d.message_id
")
});

/// Query head that reads recoverable-message and reply data and substitutes
/// `NULL` filter values.
static IOS_16_NEWER_HEAD: LazyLock<String> = LazyLock::new(|| {
    format!("
SELECT
    {COLS},
    c.chat_id,
    (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
    d.chat_id as deleted_from,
    (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies,
    NULL as filter_action,
    NULL as filter_sub_action
FROM
    {MESSAGE} as m
LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
LEFT JOIN {RECENTLY_DELETED} as d ON m.ROWID = d.message_id
")
});

/// Query head that reads reply data without joining recoverable messages.
///
/// `SELECT *` preserves the source schema. Named decoding maps missing filter
/// columns to `None` without introducing aliases that could duplicate real
/// columns.
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

/// Query head without recoverable-message or reply data.
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

// MARK: Functions
/// Build a query for schemas with message filter columns.
pub(crate) fn ios_27_newer_query(filters: Option<&str>) -> String {
    format!(
        "{}{}{}",
        *IOS_27_NEWER_HEAD,
        filters.unwrap_or_default(),
        ORDER_BY
    )
}

/// Build a query that reads recoverable-message and reply data.
pub(crate) fn ios_16_newer_query(filters: Option<&str>) -> String {
    format!(
        "{}{}{}",
        *IOS_16_NEWER_HEAD,
        filters.unwrap_or_default(),
        ORDER_BY
    )
}

/// Build a query that reads reply data without joining recoverable messages.
pub(crate) fn ios_14_15_query(filters: Option<&str>) -> String {
    format!(
        "{}{}{}",
        *IOS_14_15_HEAD,
        filters.unwrap_or_default(),
        ORDER_BY
    )
}

/// Build a query without recoverable-message or reply data.
pub(crate) fn ios_13_older_query(filters: Option<&str>) -> String {
    format!(
        "{}{}{}",
        *IOS_13_OLDER_HEAD,
        filters.unwrap_or_default(),
        ORDER_BY
    )
}
