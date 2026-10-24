//! Device metadata from the backup's `Manifest.plist`.

use plist::Value;

use crate::{
    backup::util::plist::{as_dictionary, get_key_as_string},
    error::Result,
};

/// Device metadata from the backup's `Manifest.plist`.
///
/// Holds various device properties parsed from the lockdown section of the plist.
#[derive(Debug, Clone)]
pub struct ManifestLockdownInfo {
    /// iOS build version (e.g., `"18E199"`).
    pub build_version: String,
    /// Human-readable device name.
    pub device_name: String,
    /// Device product type (e.g., `"iPhone9,4"`).
    pub product_type: String,
    /// iOS version (e.g., `"15.5"`).
    pub product_version: String,
    /// Device serial number.
    pub serial_number: String,
    /// Unique device identifier (`UDID`).
    pub unique_device_id: String,
}

impl ManifestLockdownInfo {
    /// Parse from a plist `Value`.
    ///
    /// # Errors
    /// Returns [`BackupError::PlistParseError`] if the structure is invalid.
    pub(crate) fn from_plist(plist_data: &Value) -> Result<ManifestLockdownInfo> {
        let dict = as_dictionary(plist_data)?;

        Ok(ManifestLockdownInfo {
            build_version: get_key_as_string(dict, "BuildVersion")?,
            device_name: get_key_as_string(dict, "DeviceName")?,
            product_type: get_key_as_string(dict, "ProductType")?,
            product_version: get_key_as_string(dict, "ProductVersion")?,
            serial_number: get_key_as_string(dict, "SerialNumber")?,
            unique_device_id: get_key_as_string(dict, "UniqueDeviceID")?,
        })
    }
}

#[cfg(test)]
mod tests_types {
    use plist::Value;

    use crate::backup::models::manifest::lockdown::ManifestLockdownInfo;

    #[test]
    fn test_manifest_lockdown_info_from_plist() {
        // Build a plist dictionary
        let mut dict = plist::Dictionary::new();
        dict.insert("BuildVersion".into(), Value::String("1.2.3".into()));
        dict.insert("DeviceName".into(), Value::String("TestDevice".into()));
        dict.insert("ProductType".into(), Value::String("TestType".into()));
        dict.insert("ProductVersion".into(), Value::String("14.0".into()));
        dict.insert("SerialNumber".into(), Value::String("SN123".into()));
        dict.insert("UniqueDeviceID".into(), Value::String("UDID456".into()));
        let value = Value::Dictionary(dict.clone());
        let info = ManifestLockdownInfo::from_plist(&value).unwrap();
        assert_eq!(info.build_version, "1.2.3");
        assert_eq!(info.device_name, "TestDevice");
        assert_eq!(info.product_type, "TestType");
        assert_eq!(info.product_version, "14.0");
        assert_eq!(info.serial_number, "SN123");
        assert_eq!(info.unique_device_id, "UDID456");
    }
}
