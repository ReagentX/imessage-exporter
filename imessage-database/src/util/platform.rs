/*!
 Contains data structures used to describe database platforms
*/

use std::fmt::Display;

/// Represents the platform that created the database this library connects to
pub enum Platform {
    /// MacOS-sourced data
    MacOS,
    /// iOS-sourced data
    IOS,
}

impl Platform {
    /// Given user's input, return a variant if the input matches one
    pub fn from_cli(platform: &str) -> Option<Self> {
        match platform.to_lowercase().as_str() {
            "macos" => Some(Self::MacOS),
            "ios" => Some(Self::IOS),
            _ => None,
        }
    }
}

impl Default for Platform {
    /// The default Platform is [Platform::MacOS].
    fn default() -> Self {
        Self::MacOS
    }
}

impl Display for Platform {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::MacOS => write!(fmt, "MacOS"),
            Platform::IOS => write!(fmt, "iOS"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::util::platform::Platform;

    #[test]
    fn can_parse_macos_any_case() {
        assert!(matches!(Platform::from_cli("macos"), Some(Platform::MacOS)));
        assert!(matches!(Platform::from_cli("MACOS"), Some(Platform::MacOS)));
        assert!(matches!(Platform::from_cli("MacOS"), Some(Platform::MacOS)));
    }

    #[test]
    fn can_parse_ios_any_case() {
        assert!(matches!(Platform::from_cli("ios"), Some(Platform::IOS)));
        assert!(matches!(Platform::from_cli("IOS"), Some(Platform::IOS)));
        assert!(matches!(Platform::from_cli("iOS"), Some(Platform::IOS)));
    }

    #[test]
    fn cant_parse_invalid() {
        assert!(matches!(Platform::from_cli("mac"), None));
        assert!(matches!(Platform::from_cli("iphone"), None));
        assert!(matches!(Platform::from_cli(""), None));
    }
}
