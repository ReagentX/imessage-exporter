/*!
 Contains data structures used to describe database platforms.
*/

use std::{fmt::Display, path::Path};

use crate::{
    error::table::{TableConnectError, TableError},
    tables::table::DEFAULT_PATH_IOS,
};

/// Represents the platform that created the database this library connects to
#[derive(PartialEq, Eq, Debug)]
pub enum Platform {
    /// macOS-sourced data
    #[allow(non_camel_case_types)]
    macOS,
    /// iOS-sourced data
    #[allow(non_camel_case_types)]
    iOS,
}

impl Platform {
    /// Try to determine the current platform, defaulting to macOS.
    pub fn determine(db_path: &Path) -> Result<Self, TableError> {
        // If the path ends with the default iOS path,
        // the user accidentally passed the full iOS path,
        // not the root of the backup
        if db_path.ends_with(DEFAULT_PATH_IOS) {
            return Err(TableError::CannotConnect(TableConnectError::NotBackupRoot));
        }

        if db_path.join(DEFAULT_PATH_IOS).exists() {
            return Ok(Self::iOS);
        } else if db_path.is_file() {
            return Ok(Self::macOS);
        }
        // If we get here, the database is missing; that error is handled in the connection lifecycle
        Ok(Self::default())
    }

    /// Given user's input, return a variant if the input matches one
    #[must_use]
    pub fn from_cli(platform: &str) -> Option<Self> {
        match platform.to_lowercase().as_str() {
            "macos" => Some(Self::macOS),
            "ios" => Some(Self::iOS),
            _ => None,
        }
    }
}

impl Default for Platform {
    /// The default Platform is [`Platform::macOS`].
    fn default() -> Self {
        Self::macOS
    }
}

impl Display for Platform {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::macOS => write!(fmt, "macOS"),
            Platform::iOS => write!(fmt, "iOS"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{tables::table::DEFAULT_PATH_IOS, util::platform::Platform};

    #[test]
    fn can_parse_macos_any_case() {
        assert!(matches!(Platform::from_cli("macos"), Some(Platform::macOS)));
        assert!(matches!(Platform::from_cli("MACOS"), Some(Platform::macOS)));
        assert!(matches!(Platform::from_cli("MacOS"), Some(Platform::macOS)));
    }

    #[test]
    fn can_parse_ios_any_case() {
        assert!(matches!(Platform::from_cli("ios"), Some(Platform::iOS)));
        assert!(matches!(Platform::from_cli("IOS"), Some(Platform::iOS)));
        assert!(matches!(Platform::from_cli("iOS"), Some(Platform::iOS)));
    }

    #[test]
    fn cant_parse_invalid() {
        assert!(Platform::from_cli("mac").is_none());
        assert!(Platform::from_cli("iphone").is_none());
        assert!(Platform::from_cli("").is_none());
    }

    #[test]
    fn cant_build_ends_with_ios_backup() {
        let path = std::path::PathBuf::from(DEFAULT_PATH_IOS);
        assert!(Platform::determine(&path).is_err());
    }
}
