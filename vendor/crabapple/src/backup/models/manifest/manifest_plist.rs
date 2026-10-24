//! `Manifest.plist` representation and parsing.

use std::{collections::HashMap, fs::File, path::Path};

use plist::Value;

use crate::{
    Authentication,
    backup::{
        crypto::{derive_key_from_password, unlock_keys_from_manifest},
        models::{
            keyring::{EncryptionKey, KeyRing, ProtectionClassKey},
            manifest::lockdown::ManifestLockdownInfo,
        },
        util::{
            hex::hex_decode,
            plist::{as_dictionary, get_key_as_boolean, get_key_as_data},
        },
    },
    error::{BackupError, Result},
};

use super::app::Application;

/// Holds in-memory parsed manifest (`Manifest.plist`) and associated decryption key and unwrapped class keys.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
///
/// use crabapple::Authentication;
/// use crabapple::backup::models::manifest::manifest_plist::{ManifestData, Manifest};
///
/// let auth = Authentication::None;
/// let data = ManifestData::from_plist(Path::new("/path/to/Manifest.plist"))?;
///
/// let manifest = Manifest::from_manifest_data(data, &auth)?;
/// # Ok::<(), crabapple::error::BackupError>(())
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Parsed `Manifest.plist` data.
    pub manifest_data: ManifestData,
    /// Derived decryption key (`32` bytes), if encrypted.
    pub main_decryption_key: Option<EncryptionKey>,
    /// Unwrapped per-class keys after decryption, if encrypted.
    pub unlocked_class_keys: Option<HashMap<u32, ProtectionClassKey>>,
}

impl Manifest {
    /// Load and parse the backup's `Manifest.plist` file, and derive decryption keys if needed.
    ///
    /// # Errors
    /// Returns [`BackupError`] if there are issues decrypting the `plist` data.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    ///
    /// use crabapple::Authentication;
    /// use crabapple::backup::models::manifest::manifest_plist::{ManifestData, Manifest};
    ///
    /// let auth = Authentication::Password("your_password".to_string());
    /// let data = ManifestData::from_plist(Path::new("/path/to/Manifest.plist"))?;
    ///
    /// let manifest = Manifest::from_manifest_data(data, &auth)?;
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn from_manifest_data(manifest_data: ManifestData, auth: &Authentication) -> Result<Self> {
        // Determine decryption strategy based on whether the backup is encrypted and the provided authentication.
        let (main_decryption_key, unlocked_class_keys) = match (manifest_data.is_encrypted, auth) {
            (true, Authentication::Password(password)) => {
                let backup_key_ring = manifest_data.key_ring.as_ref().ok_or_else(|| {
                    BackupError::MissingPlistKey(
                        "BackupKeyBag (required for encrypted backup)".to_string(),
                    )
                })?;
                let master_key = derive_key_from_password(
                    password.as_bytes(),
                    &backup_key_ring.dpsl,
                    backup_key_ring.dpic,
                    &backup_key_ring.salt,
                    backup_key_ring.iter,
                )?;
                let unlocked_keys_map = unlock_keys_from_manifest(&master_key, &manifest_data)
                    .map_err(|_| BackupError::PasswordOrKeyIncorrect)?;
                (Some(master_key), Some(unlocked_keys_map))
            }
            (true, Authentication::DerivedKey(key_hex)) => {
                manifest_data.key_ring.as_ref().ok_or_else(|| {
                    BackupError::MissingPlistKey(
                        "BackupKeyBag (required for encrypted backup)".to_string(),
                    )
                })?;
                let master_key = hex_decode(key_hex)?.into();
                let unlocked_keys_map = unlock_keys_from_manifest(&master_key, &manifest_data)
                    .map_err(|_| BackupError::PasswordOrKeyIncorrect)?;
                (Some(master_key), Some(unlocked_keys_map))
            }
            (true, Authentication::None) => return Err(BackupError::PasswordOrKeyRequired),
            (false, Authentication::None) => (None, None),
            (false, _) => return Err(BackupError::NotEncrypted),
        };

        Ok(Self {
            manifest_data,
            main_decryption_key,
            unlocked_class_keys,
        })
    }

    /// Get the `ProtectionClassKey` for a given protection class.
    ///
    /// # Errors
    /// Returns [`BackupError::Crypto`] if the class key is not found or [`BackupError::NotEncrypted`] if the backup is not encrypted.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    ///
    /// use crabapple::Authentication;
    /// use crabapple::backup::models::manifest::manifest_plist::{ManifestData, Manifest};
    ///
    /// let auth = Authentication::Password("your_password".to_string());
    /// let data = ManifestData::from_plist(Path::new("/path/to/Manifest.plist"))?;
    ///
    /// let manifest = Manifest::from_manifest_data(data, &auth)?;
    ///
    /// let protection_class = 1; // Example protection class
    ///
    /// println!("Key for class {protection_class}: {:?}", manifest.get_class_key(protection_class));
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn get_class_key(&self, protection_class: u32) -> Result<&ProtectionClassKey> {
        if !self.manifest_data.is_encrypted {
            return Err(BackupError::NotEncrypted);
        }
        self.unlocked_class_keys
            .as_ref()
            .and_then(|keys| keys.get(&protection_class))
            .ok_or_else(|| BackupError::Crypto(format!("Class {protection_class} key not found!",)))
    }

    /// Get all decryption keys for the manifest.
    ///
    /// # Errors
    /// Returns [`BackupError::Crypto`] if no class keys are available or [`BackupError::NotEncrypted`] if the backup is not encrypted.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    ///
    /// use crabapple::Authentication;
    /// use crabapple::backup::models::manifest::manifest_plist::{ManifestData, Manifest};
    ///
    /// let auth = Authentication::None;
    /// let data = ManifestData::from_plist(Path::new("/path/to/Manifest.plist"))?;
    ///
    /// let manifest = Manifest::from_manifest_data(data, &auth)?;
    ///
    /// println!("All keys: {:?}", manifest.keys()?);
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn keys(&self) -> Result<&HashMap<u32, ProtectionClassKey>> {
        if !self.manifest_data.is_encrypted {
            return Err(BackupError::NotEncrypted);
        }
        self.unlocked_class_keys.as_ref().ok_or_else(|| {
            BackupError::Crypto("Missing class keys for encrypted backup".to_string())
        })
    }
}

/// Parsed data from `Manifest.plist` describing the backup.
#[derive(Debug, Clone)]
pub struct ManifestData {
    /// Optional key bag containing encrypted class keys.
    pub(crate) key_ring: Option<KeyRing>,
    /// Whether the backup is encrypted.
    pub is_encrypted: bool,
    /// Device-specific lockdown info.
    pub lockdown: ManifestLockdownInfo,
    /// Optional raw manifest key (typically 40 bytes) used for Manifest.db decryption.
    pub manifest_key: Option<EncryptionKey>,
    /// Installed applications present in the backup.
    pub applications: Vec<Application>,
}

impl ManifestData {
    /// Load and parse the backup's `Manifest.plist` file.
    ///
    /// Returns a [`Manifest`] struct containing metadata and encryption parameters.
    ///
    /// # Arguments
    ///
    /// * `path` - Filesystem path to the `Manifest.plist` file.
    ///
    /// # Errors
    /// Returns [`BackupError::ManifestPlistNotFound`] if file missing, or other errors if parse fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    ///
    /// use crabapple::backup::models::manifest::manifest_plist::ManifestData;
    ///
    /// let data = ManifestData::from_plist(Path::new("/path/to/Manifest.plist"))?;
    /// println!("Encrypted: {}", data.is_encrypted);
    /// # Ok::<(), crabapple::error::BackupError>(())
    /// ```
    pub fn from_plist<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let file = File::open(path_ref)
            .map_err(|_| BackupError::ManifestPlistNotFound(path_ref.display().to_string()))?;

        let plist = Value::from_reader(file).map_err(|_| {
            BackupError::PlistParseError("Top-level plist is not a dictionary".into())
        })?;
        let dict = as_dictionary(&plist)?;

        // Determine if the backup is encrypted by checking the "IsEncrypted" key
        let is_encrypted = get_key_as_boolean(dict, "IsEncrypted").unwrap_or(false);

        // Parse the "BackupKeyBag" key for the key ring if the backup is encrypted
        let backup_key_ring = if is_encrypted {
            let data = get_key_as_data(dict, "BackupKeyBag")?;
            Some(KeyRing::from_bytes(&data)?)
        } else {
            None
        };

        // Parse the "ManifestKey" key for the raw manifest key
        let manifest_key = if is_encrypted {
            let data = get_key_as_data(dict, "ManifestKey")?;
            Some(data.clone().into())
        } else {
            None
        };

        // Parse the "Lockdown" key for device-specific information
        let lockdown_val = dict
            .get("Lockdown")
            .ok_or_else(|| BackupError::MissingPlistKey("Lockdown".into()))?;
        let lockdown = ManifestLockdownInfo::from_plist(lockdown_val)?;

        let app_dict = as_dictionary(
            dict.get("Applications")
                .ok_or_else(|| BackupError::MissingPlistKey("Applications".into()))?,
        )?;
        let applications = app_dict
            .iter()
            .filter_map(|(bundle_id, plist_data)| {
                Application::from_plist(bundle_id, plist_data).ok()
            })
            .collect::<Vec<Application>>();

        Ok(ManifestData {
            key_ring: backup_key_ring,
            is_encrypted,
            lockdown,
            manifest_key,
            applications,
        })
    }
}
