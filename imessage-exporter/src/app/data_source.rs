use std::path::Path;

use crabapple::Backup;
use imessage_database::{
    rusqlite::Connection,
    tables::{capabilities::Capabilities, table::get_connection},
    util::platform::Platform,
};

use crate::app::{
    compatibility::backup::{
        decrypt_backup, get_decrypted_contacts_database, get_decrypted_message_database,
    },
    contacts::{ContactsIndex, DEFAULT_PATH_IOS},
    error::RuntimeError,
    options::Options,
};

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
    temp_messages_db: Option<tempfile::NamedTempFile>,
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
                    let messages_db = get_decrypted_message_database(&backup)?;

                    // Use a user-supplied Contacts database instead of decrypting the backup's
                    // copy.
                    let contacts_index = match options.contacts_path.as_deref() {
                        Some(custom) => Self::get_contacts_index(Some(custom)),
                        None => {
                            let contacts_path = get_decrypted_contacts_database(&backup)?;
                            Self::get_contacts_index(Some(contacts_path.path()))
                        }
                    }
                    .unwrap_or_default();

                    eprintln!(
                        "Decrypted iOS backup: {} (version {})\n",
                        backup.lockdown().device_name,
                        backup.lockdown().product_version,
                    );

                    (
                        messages_db.path().to_path_buf(),
                        contacts_index,
                        Some(backup),
                        Some(messages_db),
                    )
                }
                None => {
                    let contacts_path = options
                        .contacts_path
                        .clone()
                        .unwrap_or_else(|| options.db_path.join(DEFAULT_PATH_IOS));
                    (
                        options.get_db_path(),
                        Self::get_contacts_index(Some(contacts_path.as_path())).unwrap_or_default(),
                        None,
                        None,
                    )
                }
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
        // holding it open on Windows; `NamedTempFile`'s own `Drop` does the removal.
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
    fn test_data_source_invalid_db_path() {
        let mut options = Options::fake_options(ExportType::Txt);
        options.platform = Platform::macOS;
        options.db_path = PathBuf::from("/nonexistent/path.db");

        // Test that creation fails with invalid path
        let result = DataSource::from(&options);
        assert!(result.is_err());
    }
}
