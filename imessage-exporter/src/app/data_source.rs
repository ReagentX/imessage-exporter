use std::{
    fs::remove_file,
    path::{Path, PathBuf},
};

use crabapple::Backup;
use imessage_database::{
    tables::{capabilities::Capabilities, table::get_connection},
    util::platform::Platform,
};
use rusqlite::Connection;

use crate::app::{
    compatibility::backup::{
        decrypt_backup, get_decrypted_contacts_database, get_decrypted_message_database,
    },
    contacts::{ContactsIndex, DEFAULT_PATH_IOS},
    error::RuntimeError,
    options::Options,
};

/// A decrypted temporary database file, removed from disk when dropped.
struct TempDatabase(PathBuf);

impl TempDatabase {
    /// Borrow the path, e.g. to open a connection to it.
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        if let Err(why) = remove_file(&self.0) {
            eprintln!(
                "warning: failed to remove temporary messages database at {}: {why}",
                self.0.display(),
            );
        }
    }
}

pub struct DataSource {
    /// Messages database connection.
    ///
    /// This is wrapped in `Option` to allow for taking/dropping it when cleaning up temporary files,
    /// but should always be `Some` during normal operation.
    messages_connection: Option<Connection>,
    /// Schema capabilities probed from the Messages database at construction.
    pub capabilities: Capabilities,
    /// Contacts index keyed by email and phone number.
    ///
    /// If construction fails, this will be an empty index, and a warning will be logged.
    pub contacts_index: ContactsIndex,
    /// Encrypted iOS backup when one was opened.
    pub backup: Option<Backup>,
    /// A temporary decrypted messages database this `DataSource` owns; its own
    /// [`Drop`] removes the file. `None` when the messages database is a real
    /// on-disk file (macOS, or iOS with an unencrypted backup).
    temp_messages_db: Option<TempDatabase>,
}

impl DataSource {
    /// Build the data source described by the provided options.
    ///
    /// Options constructor determines the platform and database location logic already,
    /// so this just uses that to create the appropriate connections and indexes.
    pub fn from(options: &Options) -> Result<Self, RuntimeError> {
        // Resolve the per-platform inputs first; the connection is opened and
        // probed once below, so the messages path for an encrypted backup must
        // point at the decrypted temporary file.
        let (messages_path, contacts_index, backup, temp_messages_db) = match options.platform {
            Platform::macOS => (
                options.get_db_path(),
                Self::get_contacts_index(options.contacts_path.as_deref()).unwrap_or_default(),
                None,
                None,
            ),
            Platform::iOS => match decrypt_backup(options)? {
                Some(backup) => {
                    let messages_db = TempDatabase(get_decrypted_message_database(&backup)?);
                    let contacts_path = get_decrypted_contacts_database(&backup)?;

                    eprintln!(
                        "Decrypted iOS backup: {} (version {})\n",
                        backup.lockdown().device_name,
                        backup.lockdown().product_version,
                    );

                    let contacts_index =
                        Self::get_contacts_index(Some(&contacts_path)).unwrap_or_default();

                    // Clean up decrypted contacts database file
                    if let Err(e) = remove_file(&contacts_path) {
                        eprintln!(
                            "warning: failed to remove temporary contacts database at {}: {e}",
                            contacts_path.display()
                        );
                    }

                    (
                        messages_db.path().to_path_buf(),
                        contacts_index,
                        Some(backup),
                        Some(messages_db),
                    )
                }
                None => (
                    options.get_db_path(),
                    Self::get_contacts_index(Some(&options.db_path.join(DEFAULT_PATH_IOS)))
                        .unwrap_or_default(),
                    None,
                    None,
                ),
            },
        };

        let messages_connection = get_connection(&messages_path)?;
        let capabilities = Capabilities::determine(&messages_connection)?;

        Ok(Self {
            messages_connection: Some(messages_connection),
            capabilities,
            contacts_index,
            backup,
            temp_messages_db,
        })
    }

    /// Build a contacts index, logging a warning on failure.
    fn get_contacts_index(path: Option<&Path>) -> Option<ContactsIndex> {
        match ContactsIndex::build(path) {
            Ok(index) => Some(index),
            Err(e) => {
                eprintln!(
                    "Unable to build contacts index: {e}\nContinuing without contact names..."
                );
                None
            }
        }
    }

    /// Return the active Messages database connection.
    ///
    /// # Panics
    ///
    /// Panics if the database connection is closed.
    pub(crate) fn db(&self) -> &Connection {
        match self.messages_connection.as_ref() {
            Some(db) => db,
            None => {
                panic!("Database connection is closed!");
            }
        }
    }
}

// MARK: Drop
impl Drop for DataSource {
    fn drop(&mut self) {
        // Close the connection before the temp database is removed, so the OS isn't
        // holding it open on Windows; `TempDatabase`'s own `Drop` does the removal.
        if let Some(conn) = self.messages_connection.take() {
            conn.close().ok();
        }
        drop(self.temp_messages_db.take());
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::app::{export_type::ExportType, options::Options};
    use imessage_database::util::platform::Platform;

    #[test]
    fn test_data_source_from_macos() {
        let mut options = Options::fake_options(ExportType::Txt);
        options.platform = Platform::macOS;

        // Test that DataSource can be created for macOS
        let result = DataSource::from(&options);
        assert!(result.is_ok());

        let ds = result.unwrap();
        assert!(ds.messages_connection.is_some());
        assert!(ds.backup.is_none());
    }

    #[test]
    fn test_data_source_db() {
        let mut options = Options::fake_options(ExportType::Txt);
        options.platform = Platform::macOS;
        let ds = DataSource::from(&options).unwrap();

        // Test that `db()` returns a connection
        let conn = ds.db();
        assert!(conn.path().is_some());
    }

    #[test]
    fn test_data_source_invalid_db_path() {
        let mut options = Options::fake_options(ExportType::Txt);
        options.platform = Platform::macOS;
        options.db_path = PathBuf::from("/nonexistent/path.db");

        // Test that creation fails with invalid path
        let result = DataSource::from(&options);
        assert!(result.is_err());
    }
}
