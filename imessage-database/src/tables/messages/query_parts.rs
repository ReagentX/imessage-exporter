/*!
 Compose `message` queries from probed schema [`Capabilities`].

The projection contains exactly the columns the schema declares, and every
schema-specific fragment (`deleted_from`, `num_replies`, the filter codes)
degrades to a neutral placeholder when its capability is absent, so one
statement shape serves every schema. Queries are built from what the database
supports, never from which OS release wrote it.
*/

use rusqlite::{CachedStatement, Connection};

use crate::{
    error::table::TableError,
    tables::{
        capabilities::Capabilities,
        table::{CHAT_MESSAGE_JOIN, MESSAGE, MESSAGE_ATTACHMENT_JOIN, RECENTLY_DELETED},
    },
};

const ORDER_BY: &str = "\nORDER BY\n    m.date;";

/// Prepare the message query for the probed schema.
///
/// `filters` is an optional rendered `WHERE` clause, e.g. from
/// [`Message::generate_filter_statement`](crate::tables::messages::Message::generate_filter_statement).
pub(crate) fn prepare_message_query<'db>(
    db: &'db Connection,
    capabilities: &Capabilities,
    filters: Option<&str>,
) -> Result<CachedStatement<'db>, TableError> {
    Ok(db.prepare_cached(&message_query(capabilities, filters))?)
}

/// Build the `FROM`/`JOIN` fragment shared by every message query that reads
/// chat associations.
pub(crate) fn from_clause(capabilities: &Capabilities) -> String {
    let recoverable_join = if capabilities.recoverable_messages {
        format!("\nLEFT JOIN {RECENTLY_DELETED} as d ON m.ROWID = d.message_id")
    } else {
        String::new()
    };
    format!(
        "\nFROM\n    {MESSAGE} as m\nLEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id{recoverable_join}"
    )
}

/// Build the message query for the probed schema.
///
/// The projection lists the recognized columns the schema declares (qualified
/// with `m.`), the derived `chat_id`/`num_attachments` values, and the
/// capability-gated fragments.
pub(crate) fn message_query(capabilities: &Capabilities, filters: Option<&str>) -> String {
    let mut projection: Vec<String> = capabilities
        .message_columns()
        .iter()
        .map(|column| format!("m.{column}"))
        .collect();
    projection.push("c.chat_id".to_owned());
    projection.push(format!(
        "(SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments"
    ));
    projection.push(
        if capabilities.recoverable_messages {
            "d.chat_id as deleted_from"
        } else {
            "NULL as deleted_from"
        }
        .to_owned(),
    );
    projection.push(if capabilities.replies {
        format!("(SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies")
    } else {
        "0 as num_replies".to_owned()
    });
    if capabilities.filter_actions {
        projection.push("m.filter_action".to_owned());
        projection.push("m.filter_sub_action".to_owned());
    } else {
        projection.push("NULL as filter_action".to_owned());
        projection.push("NULL as filter_sub_action".to_owned());
    }

    format!(
        "\nSELECT\n    {}{}\n{}{ORDER_BY}",
        projection.join(",\n    "),
        from_clause(capabilities),
        filters.unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{from_clause, message_query};
    use crate::{
        tables::{capabilities::Capabilities, messages::Message},
        test_support::schema_db,
        util::query_context::QueryContext,
    };

    #[test]
    fn full_capabilities_read_real_values() {
        let db = schema_db(true, true, true);
        let capabilities = Capabilities::determine(&db).unwrap();
        let query = message_query(&capabilities, None);

        assert!(query.contains("m.filter_action,\n    m.filter_sub_action"));
        assert!(query.contains("d.chat_id as deleted_from"));
        assert!(query.contains(
            "(SELECT COUNT(*) FROM message m2 WHERE m2.thread_originator_guid = m.guid) as num_replies"
        ));
        assert!(
            query
                .contains("LEFT JOIN chat_recoverable_message_join as d ON m.ROWID = d.message_id")
        );
        // Every recognized column is projected, qualified with the source alias.
        assert!(query.contains("    m.rowid,\n    m.guid,"));
    }

    #[test]
    fn absent_capabilities_degrade_to_placeholders() {
        let db = schema_db(false, false, false);
        let capabilities = Capabilities::determine(&db).unwrap();
        let query = message_query(&capabilities, None);

        assert!(query.contains("NULL as filter_action,\n    NULL as filter_sub_action"));
        assert!(query.contains("NULL as deleted_from"));
        assert!(query.contains("0 as num_replies"));
        assert!(!query.contains("chat_recoverable_message_join"));
        // Neither the projection nor the reply subquery may reference a
        // column the schema lacks.
        assert!(!query.contains("m.thread_originator_guid"));
        assert!(!query.contains("thread_originator_guid = m.guid"));
    }

    #[test]
    fn capabilities_degrade_independently() {
        // Replies exist but the recoverable table does not: the subquery and
        // the placeholder must coexist.
        let db = schema_db(true, false, true);
        let capabilities = Capabilities::determine(&db).unwrap();
        let query = message_query(&capabilities, None);

        assert!(query.contains(
            "(SELECT COUNT(*) FROM message m2 WHERE m2.thread_originator_guid = m.guid) as num_replies"
        ));
        assert!(query.contains("NULL as deleted_from"));
        assert!(query.contains("m.filter_action,\n    m.filter_sub_action"));
        assert!(!query.contains("LEFT JOIN chat_recoverable_message_join"));
    }

    #[test]
    fn filters_are_appended_verbatim() {
        let db = schema_db(false, false, false);
        let capabilities = Capabilities::determine(&db).unwrap();
        let query = message_query(&capabilities, Some("WHERE m.guid = \"fake\""));

        assert!(query.contains("WHERE m.guid = \"fake\"\nORDER BY\n    m.date;"));
    }

    #[test]
    fn context_filters_follow_recoverable_capability() {
        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();
        context.set_selected_chat_ids(BTreeSet::from([1, 2, 3]));
        let start_ns = context.start.unwrap();

        // With the table present, chat filters also match deleted rows.
        let db = schema_db(false, true, false);
        let capabilities = Capabilities::determine(&db).unwrap();
        let filters =
            Message::generate_filter_statement(&context, capabilities.recoverable_messages);
        assert!(filters.contains(&format!(
            "WHERE  m.date >= {start_ns} AND  (c.chat_id IN (1, 2, 3) OR d.chat_id IN (1, 2, 3))"
        )));

        // Without it, filtering against `d.chat_id` would fail to prepare.
        let db = schema_db(false, false, false);
        let capabilities = Capabilities::determine(&db).unwrap();
        let filters =
            Message::generate_filter_statement(&context, capabilities.recoverable_messages);
        assert!(filters.contains(&format!(
            "WHERE  m.date >= {start_ns} AND  c.chat_id IN (1, 2, 3)"
        )));
        assert!(!filters.contains("d.chat_id IN"));
    }

    #[test]
    fn from_clause_matches_recoverable_capability() {
        let db = schema_db(false, true, false);
        let capabilities = Capabilities::determine(&db).unwrap();
        assert!(
            from_clause(&capabilities).contains(
                "\nLEFT JOIN chat_recoverable_message_join as d ON m.ROWID = d.message_id"
            )
        );

        let db = schema_db(false, false, false);
        let capabilities = Capabilities::determine(&db).unwrap();
        assert!(!from_clause(&capabilities).contains("chat_recoverable_message_join"));
    }
}
