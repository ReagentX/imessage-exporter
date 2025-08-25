/*!
 These are the link previews that iMessage generates when sending links to apps in the App Store.
*/

use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::variants::BalloonProvider,
    util::plist::{get_string_from_dict, get_string_from_nested_dict},
};

/// This struct is not documented by Apple, but represents messages displayed as
/// `com.apple.messages.URLBalloonProvider` but for App Store apps
#[derive(Debug, PartialEq, Eq)]
pub struct AppStoreMessage<'a> {
    /// The URL that ended up serving content, after all redirects
    pub url: Option<&'a str>,
    /// The original url, before any redirects
    pub original_url: Option<&'a str>,
    /// The full name of the app in the App Store
    pub app_name: Option<&'a str>,
    /// The short description of the app in the App Store
    pub description: Option<&'a str>,
    /// The platform the app is compiled for
    pub platform: Option<&'a str>,
    /// The app's genre
    pub genre: Option<&'a str>,
}

impl<'a> BalloonProvider<'a> for AppStoreMessage<'a> {
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError> {
        if let Ok((app_metadata, body)) = AppStoreMessage::get_body_and_url(payload) {
            // Ensure the message is not a Music message
            if get_string_from_dict(app_metadata, "album").is_some() {
                return Err(PlistParseError::WrongMessageType);
            }

            return Ok(Self {
                url: get_string_from_nested_dict(body, "URL"),
                original_url: get_string_from_nested_dict(body, "originalURL"),
                app_name: get_string_from_dict(app_metadata, "name"),
                description: get_string_from_dict(app_metadata, "subtitle"),
                platform: get_string_from_dict(app_metadata, "platform"),
                genre: get_string_from_dict(app_metadata, "genre"),
            });
        }
        Err(PlistParseError::NoPayload)
    }
}

impl<'a> AppStoreMessage<'a> {
    /// Extract the main dictionary of data from the body of the payload
    ///
    /// App Store messages store the URL under `richLinkMetadata` like a normal URL, but has some
    /// extra data stored under `specialization` that contains the linked App's metadata.
    fn get_body_and_url(payload: &'a Value) -> Result<(&'a Value, &'a Value), PlistParseError> {
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
                .get("specialization")
                .ok_or_else(|| PlistParseError::MissingKey("specialization".to_string()))?,
            base,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        message_types::{app_store::AppStoreMessage, variants::BalloonProvider},
        util::plist::parse_ns_keyed_archiver,
    };
    use plist::Value;
    use std::env::current_dir;
    use std::fs::File;

    #[test]
    fn test_parse_app_store_link() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_store/AppStoreLink.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppStoreMessage::from_map(&parsed).unwrap();
        let expected = AppStoreMessage {
            url: Some("https://apps.apple.com/app/id1560298214"),
            original_url: Some("https://apps.apple.com/app/id1560298214"),
            app_name: Some("SortPuz - Water Puzzles Games"),
            description: Some("Sort the water color match 3d"),
            platform: Some("iOS"),
            genre: Some("Games"),
        };

        assert_eq!(balloon, expected);
    }
}
