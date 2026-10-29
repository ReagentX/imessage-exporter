/*!
 App Store link previews stored in URL balloon payloads.
*/

use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::variants::{BalloonProvider, HasUrl},
    util::plist::{
        get_string_from_dict, get_string_from_nested_dict, rich_link_metadata_and_nested,
    },
};

/// This struct is not documented by Apple, but represents messages displayed as
/// `com.apple.messages.URLBalloonProvider` but for App Store apps
#[derive(Debug, PartialEq, Eq)]
pub struct AppStoreMessage<'a> {
    /// URL that served the preview content.
    pub url: Option<&'a str>,
    /// Original URL before redirects.
    pub original_url: Option<&'a str>,
    /// Full App Store app name.
    pub app_name: Option<&'a str>,
    /// Short App Store description.
    pub description: Option<&'a str>,
    /// Target platform.
    pub platform: Option<&'a str>,
    /// App Store genre.
    pub genre: Option<&'a str>,
}

impl<'a> BalloonProvider<'a> for AppStoreMessage<'a> {
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError> {
        if let Ok((body, app_metadata)) = rich_link_metadata_and_nested(payload, "specialization") {
            // Music payloads share this nesting but carry album metadata.
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

impl AppStoreMessage<'_> {
    /// Resolve this message's URL via [`HasUrl::get_url`].
    #[must_use]
    pub fn get_url(&self) -> Option<&str> {
        <Self as HasUrl>::get_url(self)
    }
}

impl HasUrl for AppStoreMessage<'_> {
    fn url(&self) -> Option<&str> {
        self.url
    }

    fn original_url(&self) -> Option<&str> {
        self.original_url
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

    #[test]
    fn test_get_url() {
        let balloon = AppStoreMessage {
            url: Some("https://apps.apple.com/app/id1560298214"),
            original_url: Some("https://apps.apple.com/original"),
            app_name: None,
            description: None,
            platform: None,
            genre: None,
        };

        assert_eq!(
            balloon.get_url(),
            Some("https://apps.apple.com/app/id1560298214")
        );
    }

    #[test]
    fn test_get_original_url() {
        let balloon = AppStoreMessage {
            url: None,
            original_url: Some("https://apps.apple.com/original"),
            app_name: None,
            description: None,
            platform: None,
            genre: None,
        };

        assert_eq!(balloon.get_url(), Some("https://apps.apple.com/original"));
    }
}
