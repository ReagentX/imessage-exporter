//! Device helper functions for listing device backups and retrieving basic device information.

use std::path::Path;

use crate::{
    backup::models::manifest::{lockdown::ManifestLockdownInfo, manifest_plist::ManifestData},
    error::{BackupError, Result},
};

/// Get basic device metadata for a specific UDID.
///
/// Reads and parses `Manifest.plist` from the provided device backup path to return lockdown info.
///
/// # Arguments
///
/// * `device_backup_path` - Path to a specific device UDID backup folder.
///
/// # Errors
/// Returns `BackupError::ManifestPlistNotFound` if `Manifest.plist` is missing,
/// or `BackupError::Plist` if parsing fails.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
///
/// use crabapple::backup::device::get_device_basic_info;
///
/// let path = Path::new("/path/to/backup");
/// let info = get_device_basic_info(path).unwrap();
/// println!("Device name: {}", info.device_name);
/// ```
pub fn get_device_basic_info(device_backup_path: &Path) -> Result<ManifestLockdownInfo> {
    let plist_path = device_backup_path.join("Manifest.plist");
    if !plist_path.exists() {
        return Err(BackupError::ManifestPlistNotFound(
            plist_path.display().to_string(),
        ));
    }
    let info = ManifestData::from_plist(&plist_path)?;
    Ok(info.lockdown)
}
