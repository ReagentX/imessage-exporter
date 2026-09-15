/*!
 Diagnostic result types for Messages database tables.
*/

use rusqlite::Connection;

use crate::error::table::TableError;

use std::collections::HashSet;

pub(crate) fn count_query(db: &Connection, sql: &str) -> Result<usize, TableError> {
    let count = db.prepare(sql)?.query_row([], |row| row.get::<_, i64>(0))?;

    usize::try_from(count)
        .map_err(|_| TableError::QueryError(rusqlite::Error::IntegralValueOutOfRange(0, count)))
}

pub(crate) fn table_exists(db: &Connection, table_name: &str) -> Result<bool, TableError> {
    let exists = db.query_row(
        "
        SELECT EXISTS(
            SELECT 1
            FROM sqlite_master
            WHERE type = 'table'
              AND name = ?1
        )
        ",
        [table_name],
        |row| row.get::<_, i64>(0),
    )?;

    Ok(exists != 0)
}

pub(crate) fn column_exists(
    db: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, TableError> {
    let mut statement = db.prepare(&format!(
        "PRAGMA table_info({})",
        quote_sqlite_identifier(table_name)
    ))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;

    for column in columns {
        if column? == column_name {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Collect the declared column names of `table_name`, lowercased for
/// case-insensitive membership checks.
///
/// A missing table yields an empty set rather than an error, so callers can
/// treat "table absent" and "no recognized columns" uniformly.
pub(crate) fn column_names(
    db: &Connection,
    table_name: &str,
) -> Result<HashSet<String>, TableError> {
    let mut statement = db.prepare(&format!(
        "PRAGMA table_info({})",
        quote_sqlite_identifier(table_name)
    ))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    let mut names = HashSet::new();
    for name in columns {
        names.insert(name?.to_ascii_lowercase());
    }
    Ok(names)
}

fn quote_sqlite_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Diagnostic data for the `handle` table.
#[derive(Debug)]
pub struct HandleDiagnostic {
    /// Total handles in the table.
    pub total_handles: usize,
    /// Distinct `person_centric_id` values, or `None` when the column is unavailable.
    pub handles_with_multiple_ids: Option<usize>,
    /// Handles deduplicated into canonical handles.
    pub total_duplicated: usize,
}

/// Diagnostic data for the `message` table.
#[derive(Debug)]
pub struct MessageDiagnostic {
    /// Total messages in the table.
    pub total_messages: usize,
    /// Messages not associated with any chat.
    pub messages_without_chat: usize,
    /// Messages that belong to more than one chat.
    pub messages_in_multiple_chats: usize,
    /// Recently deleted messages that are still recoverable.
    pub recoverable_messages: Option<usize>,
    /// Raw `date` value of the earliest message.
    pub first_message_date: Option<i64>,
    /// Raw `date` value of the most recent message.
    pub last_message_date: Option<i64>,
}

/// Diagnostic data for the `attachment` table.
#[derive(Debug)]
pub struct AttachmentDiagnostic {
    /// Total attachments in the table.
    pub total_attachments: usize,
    /// Sum of `total_bytes` for all attachment rows.
    pub total_bytes_referenced: u64,
    /// Total size of attachment files present on disk.
    pub total_bytes_on_disk: u64,
    /// Attachments with no path or no file at the resolved path.
    pub missing_files: usize,
    /// Attachments with no path in the table.
    pub no_path_provided: usize,
}

impl AttachmentDiagnostic {
    /// Attachments with a path but no file at that location.
    #[must_use]
    pub fn no_file_located(&self) -> usize {
        self.missing_files.saturating_sub(self.no_path_provided)
    }

    /// Percentage of attachments that are missing.
    #[must_use]
    pub fn missing_percent(&self) -> Option<f64> {
        if self.total_attachments > 0 {
            Some(self.missing_files as f64 / self.total_attachments as f64 * 100.0)
        } else {
            None
        }
    }
}

/// Diagnostic data for chat-handle relationships.
#[derive(Debug)]
pub struct ChatHandleDiagnostic {
    /// Total chats in the table.
    pub total_chats: usize,
    /// Chats deduplicated into canonical chats.
    pub total_duplicated: usize,
    /// Chats with messages but no associated handles.
    pub chats_with_no_handles: usize,
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{column_exists, table_exists};

    #[test]
    fn table_exists_detects_existing_and_missing_tables() {
        let db = Connection::open_in_memory().unwrap();
        db.execute("CREATE TABLE test_table (id INTEGER)", [])
            .unwrap();

        assert!(table_exists(&db, "test_table").unwrap());
        assert!(!table_exists(&db, "missing_table").unwrap());
    }

    #[test]
    fn column_exists_detects_existing_and_missing_columns() {
        let db = Connection::open_in_memory().unwrap();
        db.execute("CREATE TABLE test_table (id INTEGER, name TEXT)", [])
            .unwrap();

        assert!(column_exists(&db, "test_table", "name").unwrap());
        assert!(!column_exists(&db, "test_table", "missing_column").unwrap());
        assert!(!column_exists(&db, "missing_table", "name").unwrap());
    }

    #[test]
    fn column_exists_quotes_table_identifiers() {
        let db = Connection::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE \"quoted\"\"table\" (\"weird column\" TEXT)",
            [],
        )
        .unwrap();

        assert!(column_exists(&db, "quoted\"table", "weird column").unwrap());
    }
}
