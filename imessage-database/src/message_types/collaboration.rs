/*!
 These are the link previews that iMessage generates when sending Collaboration links, i.e. from Pages or Freeform.
*/

use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::variants::BalloonProvider,
    util::plist::{get_float_from_nested_dict, get_string_from_dict, get_string_from_nested_dict},
};

/// This struct is not documented by Apple, but represents messages displayed as
/// `com.apple.messages.URLBalloonProvider` but from [Rich Collaboration](https://developer.apple.com/videos/play/wwdc2022/10095/) messages
#[derive(Debug, PartialEq)]
pub struct CollaborationMessage<'a> {
    /// The URL the user interacts with to start the share session
    pub original_url: Option<&'a str>,
    /// The unique URL for the collaboration item
    pub url: Option<&'a str>,
    /// The title of the shared file
    pub title: Option<&'a str>,
    /// The date the session was initiated
    pub creation_date: Option<f64>,
    /// The Bundle ID of the application that generated the message
    pub bundle_id: Option<&'a str>,
    /// The name of the application that generated the message
    pub app_name: Option<&'a str>,
}

impl<'a> BalloonProvider<'a> for CollaborationMessage<'a> {
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError> {
        if let Ok((meta, base)) = CollaborationMessage::get_meta_and_specialization(payload) {
            return Ok(Self {
                original_url: get_string_from_nested_dict(base, "originalURL"),
                url: get_string_from_dict(meta, "collaborationIdentifier"),
                title: get_string_from_dict(meta, "title"),
                creation_date: get_float_from_nested_dict(meta, "creationDate"),
                bundle_id: CollaborationMessage::get_bundle_id(meta),
                app_name: CollaborationMessage::get_app_name(base),
            });
        }
        Err(PlistParseError::NoPayload)
    }
}

impl<'a> CollaborationMessage<'a> {
    /// Extract the main dictionary of data from the body of the payload
    ///
    /// Collaboration messages store the URL under `richLinkMetadata` like a normal URL, but has some
    /// extra data stored under `collaborationMetadata` that contains the collaboration information.
    fn get_meta_and_specialization(
        payload: &'a Value,
    ) -> Result<(&'a Value, &'a Value), PlistParseError> {
        let base = payload
            .as_dictionary()
            .ok_or_else(|| {
                PlistParseError::InvalidType("root".to_string(), "dictionary".to_string())
            })?
            .get("richLinkMetadata")
            .ok_or_else(|| PlistParseError::MissingKey("richLinkMetadata".to_string()))?;
        Ok((
            base.as_dictionary()
                .ok_or_else(|| {
                    PlistParseError::InvalidType("root".to_string(), "dictionary".to_string())
                })?
                .get("collaborationMetadata")
                .ok_or_else(|| PlistParseError::MissingKey("collaborationMetadata".to_string()))?,
            base,
        ))
    }

    /// Extract the Bundle ID from the `containerSetupInfo` dict
    fn get_bundle_id(payload: &'a Value) -> Option<&'a str> {
        payload
            .as_dictionary()?
            .get("containerSetupInfo")?
            .as_dictionary()?
            .get("containerID")?
            .as_dictionary()?
            .get("ContainerIdentifier")?
            .as_string()
    }

    /// Extract the application name from the `richLinkMetadata` dict
    fn get_app_name(payload: &'a Value) -> Option<&'a str> {
        payload
            .as_dictionary()?
            .get("specialization2")?
            .as_dictionary()?
            .get("specialization")?
            .as_dictionary()?
            .get("application")?
            .as_string()
    }

    /// Get the redirected URL from a URL message, falling back to the original URL, if it exists
    #[must_use]
    pub fn get_url(&self) -> Option<&str> {
        self.url.or(self.original_url)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        message_types::{collaboration::CollaborationMessage, variants::BalloonProvider},
        util::plist::parse_ns_keyed_archiver,
    };
    use plist::Value;
    use std::env::current_dir;
    use std::fs::File;

    #[test]
    fn test_parse_collaboration() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/collaboration_message/Freeform.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let actual = CollaborationMessage::from_map(&parsed).unwrap();
        let expected = CollaborationMessage {
            original_url: Some("https://www.icloud.com/freeform/REDACTED#Untitled"),
            url: Some("https://www.icloud.com/freeform/REDACTED"),
            title: Some("Untitled"),
            creation_date: Some(695179243.070923),
            bundle_id: Some("com.apple.freeform"),
            app_name: Some("Freeform"),
        };

        assert_eq!(actual, expected);
    }
}
