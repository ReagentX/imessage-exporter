//! Represents a single application entry in the backup's manifest.

use plist::Value;

use crate::{
    backup::util::plist::{as_dictionary, get_key_as_string},
    error::Result,
};

/// Represents a single application entry in the backup's manifest.
///
/// Contains the bundle identifier, optional version string,
/// container class, and optional backup container path.
#[derive(Debug, Clone)]
pub struct Application {
    /// The app’s bundle identifier, e.g. `"com.apple.MobileSMS"`.
    pub bundle_id: String,
    /// The bundle version, if provided.
    pub bundle_version: Option<String>,
    /// The container content class (`"Data/Application"`, `"Data/PluginKitPlugin"`, `"Shared/AppGroup"`, etc.).
    pub container_class: String,
    /// The path within the backup where this app’s container is stored.
    pub path: Option<String>,
}

impl Application {
    /// Create a new `Application` instance from a `plist` dictionary.
    ///
    /// # Errors
    ///
    /// Returns [`BackupError::PlistParseError`](crate::error::BackupError::PlistParseError) if the plist data is not a dictionary or if the required keys are missing.
    pub fn from_plist(bundle_id: &str, plist_data: &Value) -> Result<Self> {
        let dict = as_dictionary(plist_data)?;

        Ok(Application {
            bundle_id: bundle_id.to_owned(),
            bundle_version: get_key_as_string(dict, "CFBundleVersion").ok(),
            container_class: get_key_as_string(dict, "ContainerContentClass")?,
            path: get_key_as_string(dict, "Path").ok(),
        })
    }
}

#[cfg(test)]
mod tests {
    use plist::Dictionary;

    use super::*;
    use crate::error::BackupError;

    fn make_dict(entries: Vec<(&str, Value)>) -> Value {
        let mut map = Dictionary::new();
        for (k, v) in entries {
            map.insert(k.to_string(), v);
        }
        Value::Dictionary(map)
    }

    #[test]
    fn from_plist_all_fields() {
        let plist = make_dict(vec![
            ("CFBundleVersion", Value::String("1.2.3".into())),
            ("ContainerContentClass", Value::String("User".into())),
            ("Path", Value::String("App/Path".into())),
        ]);
        let app = Application::from_plist("com.example.app", &plist).unwrap();
        assert_eq!(app.bundle_id, "com.example.app");
        assert_eq!(app.bundle_version, Some("1.2.3".into()));
        assert_eq!(app.container_class, "User");
        assert_eq!(app.path, Some("App/Path".into()));
    }

    #[test]
    fn from_plist_missing_optional() {
        let plist = make_dict(vec![
            // no CFBundleVersion, no Path
            ("ContainerContentClass", Value::String("System".into())),
        ]);
        let app = Application::from_plist("com.example.min", &plist).unwrap();
        assert_eq!(app.bundle_id, "com.example.min");
        assert_eq!(app.bundle_version, None);
        assert_eq!(app.container_class, "System");
        assert_eq!(app.path, None);
    }

    #[test]
    fn from_plist_missing_required() {
        let plist = make_dict(vec![
            ("CFBundleVersion", Value::String("0.1".into())),
            // missing ContainerContentClass
        ]);
        let err = Application::from_plist("com.missing.required", &plist).unwrap_err();
        match err {
            BackupError::PlistParseError(msg) => {
                assert!(msg.contains("ContainerContentClass"));
            }
            _ => panic!("Expected PlistParseError, got {err:?}"),
        }
    }
}
