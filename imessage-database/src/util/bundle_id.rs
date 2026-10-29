/*!
 [Apple Bundle ID](https://developer.apple.com/documentation/appstoreconnectapi/bundle-ids) helpers for Messages app balloons.
*/

/// Parse the app bundle ID from a balloon bundle ID.
///
/// For example, a balloon bundle ID like `com.apple.messages.MSMessageExtensionBalloonPlugin:0000000000:com.apple.SafetyMonitorApp.SafetyMonitorMessages`
/// should get parsed into `com.apple.SafetyMonitorApp.SafetyMonitorMessages`.
///
/// # Example
///
/// ```
/// use imessage_database::util::bundle_id::parse_balloon_bundle_id;
///
/// let bundle_id = "com.apple.messages.MSMessageExtensionBalloonPlugin:0000000000:com.apple.SafetyMonitorApp.SafetyMonitorMessages";
/// let parsed = parse_balloon_bundle_id(Some(bundle_id)); // Some("com.apple.SafetyMonitorApp.SafetyMonitorMessages")
/// ```
#[must_use]
pub fn parse_balloon_bundle_id(bundle_id: Option<&str>) -> Option<&str> {
    bundle_id.and_then(|id| {
        let mut parts = id.split(':');
        let first = parts.next();

        // If there is only one part, use that, otherwise get the third part
        match parts.next() {
            None => first,
            // Will be None if there is no third part
            Some(_) => parts.next(),
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::util::bundle_id::parse_balloon_bundle_id;

    #[test]
    fn can_get_no_balloon_bundle_id() {
        assert_eq!(parse_balloon_bundle_id(None), None);
    }

    #[test]
    fn can_get_balloon_bundle_id() {
        let cases = [
            (
                "os provider without plugin prefix",
                "com.apple.Handwriting.HandwritingProvider",
                "com.apple.Handwriting.HandwritingProvider",
            ),
            (
                "url provider without plugin prefix",
                "com.apple.messages.URLBalloonProvider",
                "com.apple.messages.URLBalloonProvider",
            ),
            (
                "apple plugin with placeholder team id",
                "com.apple.messages.MSMessageExtensionBalloonPlugin:0000000000:com.apple.PassbookUIService.PeerPaymentMessagesExtension",
                "com.apple.PassbookUIService.PeerPaymentMessagesExtension",
            ),
            (
                "third-party plugin with team id",
                "com.apple.messages.MSMessageExtensionBalloonPlugin:QPU8QS3E62:com.contextoptional.OpenTable.Messages",
                "com.contextoptional.OpenTable.Messages",
            ),
        ];
        for (label, input, expected) in cases {
            assert_eq!(
                parse_balloon_bundle_id(Some(input)),
                Some(expected),
                "{label}"
            );
        }
    }
}
