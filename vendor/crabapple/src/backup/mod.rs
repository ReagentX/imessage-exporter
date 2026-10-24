//! Data structures and logic for iOS backup handling.

pub mod crypto;
pub mod device;
pub mod models;
pub(crate) mod util;

use std::{
    collections::HashSet,
    fs::{File, read},
    io::BufReader,
    path::{Path, PathBuf},
};

use crate::{
    backup::{
        crypto::{AesCbcDecryptReader, aes_decrypt_cbc_with_padding, aes_kw_unwrap},
        models::{
            auth::Authentication,
            file::BackupFileEntry,
            keyring::EncryptionKey,
            manifest::{
                app::Application,
                lockdown::ManifestLockdownInfo,
                manifest_plist::{Manifest, ManifestData},
            },
            manifest_db::ManifestDb,
        },
        util::hex::hex_encode,
    },
    error::{BackupError, Result},
};

/// Main entry point for working with an iOS backup.
///
/// Provides methods to initialize, configure, and extract data from a backup,
/// including metadata loading, manifest database access, and file decryption.
#[derive(Debug)]
pub struct Backup {
    /// Filesystem path to the specific device backup folder
    pub backup_path: PathBuf,
    /// Parsed manifest and decryption state
    pub manifest: Manifest,
    /// Decrypted manifest database details
    pub manifest_db: ManifestDb,
}

impl Backup {
    /// Create a new [`Backup`] instance, loading manifest data.
    ///
    /// # Arguments
    ///
    /// * `backup_path` - Filesystem path to a specific device backup folder (the UDID directory).
    /// * `auth` - [`Authentication`] specifying password or derived key.
    ///
    /// # Errors
    /// Returns [`BackupError`] if paths are invalid, manifest loading fails, or decryption fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// println!("UDID: {}", backup.udid()?);
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn open<P: AsRef<Path>>(backup_path: P, auth: &Authentication) -> Result<Self> {
        let device_backup_path = backup_path.as_ref().to_path_buf();
        if !device_backup_path.is_dir() {
            return Err(BackupError::InvalidBackupRoot(
                device_backup_path.display().to_string(),
            ));
        }

        let manifest_plist = device_backup_path.join("Manifest.plist");

        // Ensure that the manifest plist file exists
        if !manifest_plist.exists() {
            return Err(BackupError::ManifestPlistNotFound(
                device_backup_path
                    .join("Manifest.plist")
                    .display()
                    .to_string(),
            ));
        }

        // Load `Manifest.plist` and extract necessary keys and info
        let manifest_data = ManifestData::from_plist(&manifest_plist)?;
        let manifest = Manifest::from_manifest_data(manifest_data, auth)?;
        let manifest_db = ManifestDb::new(&device_backup_path.join("Manifest.db"), &manifest)?;

        Ok(Self {
            backup_path: device_backup_path,
            manifest,
            manifest_db,
        })
    }

    /// Returns the current device `UDID` (the backup folder name).
    ///
    /// # Errors
    /// Returns [`BackupError::InvalidBackupRoot`] if the `UDID` cannot be retrieved as a string.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// let udid = backup.udid()?;
    /// println!("UDID: {}", udid);
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn udid(&self) -> Result<&str> {
        self.backup_path
            .file_name()
            .and_then(|os| os.to_str())
            .ok_or_else(|| BackupError::InvalidBackupRoot(self.backup_path.display().to_string()))
    }

    /// Returns device metadata from `Manifest.plist`.
    ///
    /// # Returns
    /// Manifest lockdown information parsed from `Manifest.plist`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// let lockdown = backup.lockdown();
    /// println!("Device name: {}", lockdown.device_name);
    /// # Ok::<(), crabapple::error::BackupError>(())
    #[must_use]
    pub fn lockdown(&self) -> &ManifestLockdownInfo {
        &self.manifest.manifest_data.lockdown
    }

    /// Indicates whether the backup is encrypted.
    ///
    /// # Returns
    /// `true` if the backup is encrypted, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// println!("Encrypted?: {}", backup.is_encrypted());
    /// # Ok::<(), crabapple::error::BackupError>(())
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        self.manifest.manifest_data.is_encrypted
    }

    /// Get number of applications in the backup.
    ///
    /// # Returns
    /// The number of applications in the backup.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// println!("Backup contains {} apps!", backup.num_apps());
    /// # Ok::<(), crabapple::error::BackupError>(())
    pub fn num_apps(&self) -> usize {
        self.manifest.manifest_data.applications.len()
    }

    /// Get a reference to the applications in the backup.
    ///
    /// # Returns
    /// A reference to a vector of [`Application`] objects parsed from the manifest.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// let apps = backup.apps();
    /// for app in apps {
    ///    println!("App: {}", app.bundle_id);
    /// }
    /// # Ok::<(), crabapple::error::BackupError>(())
    pub fn apps(&self) -> &[Application] {
        &self.manifest.manifest_data.applications
    }

    /// Returns the main decryption key as a hex string, if the backup is encrypted.
    ///
    /// # Returns
    /// An [`Option<String>`] containing the decryption key in hexadecimal representation,
    /// or [`None`] if the backup is not encrypted.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// if let Some(key_hex) = backup.decryption_key_hex() {
    ///     println!("Key: {}", key_hex);
    /// }
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    #[must_use]
    pub fn decryption_key_hex(&self) -> Option<String> {
        self.manifest
            .main_decryption_key
            .as_ref()
            .map(|v| hex_encode(v))
    }

    /// Retrieve the raw 32-byte decryption key, if available.
    ///
    /// # Returns
    /// An `Option<KeyEncryptionKey>` containing the main decryption key, or `None` if not encrypted.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// if let Some(key) = backup.decryption_key() {
    ///     println!("Key: {:?}", key);
    /// }
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    #[must_use]
    pub fn decryption_key(&self) -> Option<EncryptionKey> {
        self.manifest.main_decryption_key.clone()
    }

    /// Get all domains present in the backup's manifest database.
    ///
    /// Some common domains, in no particular order, include:
    ///
    /// * `AppDomain`
    /// * `AppDomainGroup`
    /// * `AppDomainPlugin`
    /// * `CameraRollDomain`
    /// * `DatabaseDomain`
    /// * `HealthDomain`
    /// * `HomeDomain`
    /// * `HomeKitDomain`
    /// * `InstallDomain`
    /// * `KeyboardDomain`
    /// * `KeychainDomain`
    /// * `ManagedPreferencesDomain`
    /// * `MediaDomain`
    /// * `MobileDeviceDomain`
    /// * `NetworkDomain`
    /// * `ProtectedDomain`
    /// * `RootDomain`
    /// * `SysContainerDomain`
    /// * `SysSharedContainerDomain`
    /// * `SystemPreferencesDomain`
    /// * `TonesDomain`
    /// * `WirelessDomain`
    ///
    /// # Returns
    /// A [`HashSet<String>`] containing each unique domain present in the backup.
    ///
    /// # Errors
    /// Returns [`BackupError::ManifestDbNotFound`] if the manifest database is unavailable,
    /// or [`BackupError::Database`] if the database query fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// let domains = backup.query_all_domains()?;
    /// println!("Domains: {:?}", domains);
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn query_all_domains(&self) -> Result<HashSet<String>> {
        self.manifest_db.query_all_domains()
    }

    /// Get the filesystem path to the decrypted (or raw) `Manifest.db` file.
    ///
    /// # Returns
    /// A [`Path`] pointing to the location of the manifest database file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// let db_path = backup.manifest_db_path();
    /// println!("Manifest.db path: {:?}", db_path);
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn manifest_db_path(&self) -> &Path {
        &self.manifest_db.db_path
    }

    /// List all files recorded in `Manifest.db`.
    ///
    /// # Errors
    /// Returns [`BackupError::Database`] if the database cannot be accessed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// let entries = backup.entries()?;
    /// for entry in entries {
    ///     println!("{:?}", entry);
    /// }
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn entries(&self) -> Result<Vec<BackupFileEntry>> {
        self.manifest_db.query_all_entries()
    }

    /// Get a single file entry by its file ID.
    ///
    /// # Arguments
    /// * `file_id` - The file's unique identifier (`SHA1` hash).
    ///
    /// # Errors
    /// Returns [`BackupError::FileNotFoundInBackup`] if the specified file ID is not found,
    /// or [`BackupError::Database`] if the database query fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// let entry = backup.get_file("fileid")?;
    /// println!("File encryption key: {:?}", entry.metadata.encryption_key);
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn get_file(&self, file_id: &str) -> Result<BackupFileEntry> {
        self.manifest_db
            .query_file_by_id(file_id)?
            .ok_or_else(|| BackupError::FileNotFoundInBackup(file_id.to_string()))
    }

    /// Decrypt the file represented by [`BackupFileEntry`], returning plaintext bytes.
    ///
    /// All operations are performed in memory, and the decrypted data is returned as a byte vector.
    ///
    /// # Arguments
    /// * `entry` - A [`BackupFileEntry`] containing metadata and encrypted file ID.
    ///
    /// # Returns
    /// Plaintext data as a byte vector.
    ///
    /// # Errors
    /// Returns [`BackupError::Crypto`] on decryption errors or missing keys.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// let entry = backup.get_file("fileid")?;
    /// let data = backup.decrypt_entry(&entry)?;
    /// println!("Data size: {} bytes", data.len());
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn decrypt_entry(&self, entry: &BackupFileEntry) -> Result<Vec<u8>> {
        if !self.is_encrypted() {
            return Err(BackupError::NotEncrypted);
        }

        let source = self
            .backup_path
            .join(&entry.file_id[0..2])
            .join(&entry.file_id);

        let ciphertext = read(&source)?;

        let key = self.unwrap_key_for_entry(entry)?;
        aes_decrypt_cbc_with_padding(&ciphertext, &key)
    }

    /// Decrypt the file represented by [`BackupFileEntry`], returning a streaming reader.
    ///
    /// All operations are streamed from the disk, and the decrypted data is returned as a reader.
    ///
    /// # Arguments
    /// * `entry` - A [`BackupFileEntry`] containing metadata and encrypted file ID.
    ///
    /// # Returns
    /// A streaming reader implementing `std::io::Read` that yields plaintext as it's read.
    ///
    /// # Errors
    /// Returns [`BackupError::Crypto`] on decryption errors or missing keys.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::{fs::File, io::copy};
    /// use crabapple::{Backup, Authentication};
    ///
    /// let backup = Backup::open(
    ///     "/path/to/backup",
    ///     &Authentication::Password("pass".into()),
    /// )?;
    ///
    /// let file = backup.get_file("41ee3469300471004e6d526ebd09c051c19f8a39")?;
    /// let mut reader = backup.decrypt_entry_stream(&file)?;
    /// let mut plain = Vec::new();
    /// copy(&mut reader, &mut plain)?;
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn decrypt_entry_stream(
        &self,
        entry: &BackupFileEntry,
    ) -> Result<AesCbcDecryptReader<BufReader<File>>> {
        if !self.is_encrypted() {
            return Err(BackupError::NotEncrypted);
        }

        let ciphertext = File::open(self.backup_path.join(entry.source()))?;

        let key = self.unwrap_key_for_entry(entry)?;
        AesCbcDecryptReader::from(ciphertext, &key)
    }

    /// Unwrap the encryption key for a specific file entry.
    ///
    /// # Arguments
    /// * `entry` - A [`BackupFileEntry`] containing metadata and encrypted file ID.
    ///
    /// # Returns
    /// A streaming reader implementing `std::io::Read` that yields plaintext as it's read.
    ///
    /// # Errors
    /// Returns [`BackupError::Crypto`] on decryption errors or missing keys.
    fn unwrap_key_for_entry(&self, entry: &BackupFileEntry) -> Result<EncryptionKey> {
        let class_key_entry = self
            .manifest
            .get_class_key(entry.metadata.protection_class)?;

        let key = aes_kw_unwrap(
            &class_key_entry.key,
            &entry
                .metadata
                .encryption_key
                .as_ref()
                .ok_or(BackupError::NotEncrypted)?
                .file_key,
        )?;

        Ok(key)
    }
}
