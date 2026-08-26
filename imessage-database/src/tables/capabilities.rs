/*!
 Capability detection for Messages databases.

Schemas vary independently of OS release: even on a single macOS build, two
machines can carry different `message` columns. [`Capabilities::determine`]
probes the live schema once; query composition then includes exactly what the
database supports.
*/

use rusqlite::Connection;

use crate::{
    error::table::TableError,
    tables::{
        attachment::ATTACHMENT_COLUMNS,
        diagnostic::{column_names, table_exists},
        messages::columns::MESSAGE_COLUMNS,
        table::{ATTACHMENT, MESSAGE, RECENTLY_DELETED},
    },
};

/// Features of a Messages database schema, probed from the live database.
///
/// Each flag guards the SQL constructs that reference its column or table:
/// when a flag is `false`, composed queries substitute a neutral placeholder
/// so deserialization behaves uniformly across schemas.
#[derive(Clone, Debug)]
pub struct Capabilities {
    /// Recognized `message` columns this schema declares, in canonical
    /// ([`MESSAGE_COLUMNS`]) order.
    message_columns: Vec<&'static str>,
    /// Recognized `attachment` columns this schema declares, in canonical
    /// ([`ATTACHMENT_COLUMNS`]) order.
    attachment_columns: Vec<&'static str>,
    /// Both `filter_action` and `filter_sub_action` exist on `message`.
    ///
    /// The pair is one feature: queries project either both real columns or
    /// two `NULL` placeholders. A schema carrying only one of the pair reads
    /// `None` for both, keeping [`FilterAction`](crate::tables::messages::models::FilterAction)
    /// parsing off partial data.
    pub filter_actions: bool,
    /// The `chat_recoverable_message_join` table exists, so recently deleted
    /// messages can be identified and filtered.
    pub recoverable_messages: bool,
    /// `thread_originator_guid` exists on `message`, so replies can be counted
    /// and grouped.
    pub replies: bool,
    /// `associated_message_guid` exists on `message`, so tapbacks and poll
    /// votes can be resolved.
    pub associated_message_guids: bool,
}

impl Capabilities {
    /// Probe the schema of the supplied database.
    pub fn determine(db: &Connection) -> Result<Self, TableError> {
        let message_names = column_names(db, MESSAGE)?;
        let attachment_names = column_names(db, ATTACHMENT)?;

        Ok(Self {
            message_columns: MESSAGE_COLUMNS
                .into_iter()
                .filter(|column| message_names.contains(*column))
                .collect(),
            attachment_columns: ATTACHMENT_COLUMNS
                .into_iter()
                .filter(|column| attachment_names.contains(*column))
                .collect(),
            filter_actions: message_names.contains("filter_action")
                && message_names.contains("filter_sub_action"),
            recoverable_messages: table_exists(db, RECENTLY_DELETED)?,
            replies: message_names.contains("thread_originator_guid"),
            associated_message_guids: message_names.contains("associated_message_guid"),
        })
    }

    /// Return a copy with the row-enrichment features disabled:
    /// `recoverable_messages`, `replies`, and `filter_actions`.
    ///
    /// Narrow scans that only need the base projection (e.g. the tapback
    /// cache) use this to skip correlated subqueries and joins those
    /// features would add.
    #[must_use]
    pub fn without_derived_features(&self) -> Self {
        Self {
            recoverable_messages: false,
            replies: false,
            filter_actions: false,
            ..self.clone()
        }
    }

    /// Recognized `message` columns this schema declares, in canonical order.
    pub fn message_columns(&self) -> &[&'static str] {
        &self.message_columns
    }

    /// Recognized `attachment` columns this schema declares, in canonical order.
    pub fn attachment_columns(&self) -> &[&'static str] {
        &self.attachment_columns
    }
}

#[cfg(test)]
mod tests {
    use super::Capabilities;
    use crate::test_support::schema_db;

    #[test]
    fn detects_present_features() {
        let db = schema_db(true, true, true);
        let capabilities = Capabilities::determine(&db).unwrap();

        assert!(capabilities.filter_actions);
        assert!(capabilities.recoverable_messages);
        assert!(capabilities.replies);
        assert!(capabilities.associated_message_guids);

        // Every recognized column joins the projection.
        assert_eq!(capabilities.message_columns().len(), 26);
    }

    #[test]
    fn detects_absent_features() {
        let db = schema_db(false, false, false);
        let capabilities = Capabilities::determine(&db).unwrap();

        assert!(!capabilities.filter_actions);
        assert!(!capabilities.recoverable_messages);
        assert!(!capabilities.replies);
        // The reply columns leave the projection with their capability.
        assert!(
            !capabilities
                .message_columns()
                .contains(&"thread_originator_guid")
        );
    }

    #[test]
    fn pre_tapback_schema_probes_no_associated_guids() {
        let db = schema_db(false, false, false);
        db.execute_batch("ALTER TABLE message DROP COLUMN associated_message_guid")
            .unwrap();

        assert!(
            !Capabilities::determine(&db)
                .unwrap()
                .associated_message_guids
        );
    }

    #[test]
    fn attachment_columns_track_the_schema() {
        // The pre-emoji `attachment` layout declares every recognized column
        // except `emoji_image_short_description`.
        let db = schema_db(false, false, false);
        db.execute_batch(
            "CREATE TABLE attachment (
                ROWID INTEGER PRIMARY KEY,
                guid TEXT,
                filename TEXT,
                uti TEXT,
                mime_type TEXT,
                transfer_name TEXT,
                total_bytes INTEGER,
                is_sticker INTEGER,
                hide_attachment INTEGER
            );",
        )
        .unwrap();

        let capabilities = Capabilities::determine(&db).unwrap();

        assert_eq!(capabilities.attachment_columns().len(), 9);
        assert!(
            !capabilities
                .attachment_columns()
                .contains(&"emoji_image_short_description")
        );
    }
}
