/*!
 Capability detection for Messages databases.

Schemas vary independently of OS release: even on a single macOS build, two
machines can carry different `message` columns. [`Capabilities::determine`]
probes the live schema once; query composition then includes exactly what the
database supports.
*/

use std::path::PathBuf;

use rusqlite::Connection;

use crate::{
    error::table::{TableConnectError, TableError},
    tables::{
        attachment::ATTACHMENT_COLUMNS,
        diagnostic::{column_names, table_exists},
        messages::columns::MESSAGE_COLUMNS,
        table::{ATTACHMENT, MESSAGE, RECENTLY_DELETED},
    },
};

/// `message` columns that exist only to feed body parsing during row decoding.
const BODY_INPUT_COLUMNS: [&str; 3] = ["attributedbody", "message_summary_info", "payload_data"];

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
    /// Text values need a blob cast to preserve their UTF-16 storage bytes.
    pub(crate) utf16_text: bool,
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
        if !table_exists(db, MESSAGE)? {
            return Err(TableError::CannotConnect(
                TableConnectError::NotMessagesDatabase(PathBuf::from(
                    db.path().unwrap_or_default(),
                )),
            ));
        }
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
            utf16_text: db
                .query_row("PRAGMA encoding", [], |row| row.get::<_, String>(0))?
                .starts_with("UTF-16"),
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

    /// Return a copy whose message projection omits the body-parser inputs:
    /// `attributedBody`, `message_summary_info`, and `payload_data`.
    ///
    /// Row decoding parses a body only from projected bytes, so a scan that
    /// never renders its rows uses this to skip the blob reads and the
    /// typedstream decode for every row.
    #[must_use]
    pub fn without_body_inputs(&self) -> Self {
        Self {
            message_columns: self
                .message_columns
                .iter()
                .copied()
                .filter(|column| !BODY_INPUT_COLUMNS.contains(column))
                .collect(),
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
    use rusqlite::Connection;

    use super::Capabilities;
    use crate::{
        error::table::{TableConnectError, TableError},
        test_support::schema_db,
    };

    #[test]
    fn rejects_database_without_message_table() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE Files (fileID TEXT)")
            .unwrap();

        assert!(matches!(
            Capabilities::determine(&db),
            Err(TableError::CannotConnect(
                TableConnectError::NotMessagesDatabase(_)
            ))
        ));
    }

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

    #[test]
    fn without_body_inputs_drops_only_the_parser_columns() {
        let db = schema_db(true, true, true);
        db.execute_batch(
            "ALTER TABLE message ADD COLUMN attributedBody BLOB;
             ALTER TABLE message ADD COLUMN message_summary_info BLOB;
             ALTER TABLE message ADD COLUMN payload_data BLOB;",
        )
        .unwrap();
        let capabilities = Capabilities::determine(&db).unwrap();
        assert_eq!(capabilities.message_columns().len(), 29);

        let lean = capabilities.without_body_inputs();
        let expected: Vec<&str> = capabilities
            .message_columns()
            .iter()
            .copied()
            .filter(|column| {
                !matches!(
                    *column,
                    "attributedbody" | "message_summary_info" | "payload_data"
                )
            })
            .collect();
        assert_eq!(lean.message_columns(), expected.as_slice());
        assert_eq!(lean.message_columns().len(), 26);
        assert!(lean.filter_actions);
        assert!(lean.recoverable_messages);
        assert!(lean.replies);
    }
}
