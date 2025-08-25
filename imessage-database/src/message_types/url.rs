/*!
 These are the link previews that iMessage generates when sending links.

 They may contain metadata, even if the page the link points to no longer exists on the internet.
*/

use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::{
        app_store::AppStoreMessage,
        collaboration::CollaborationMessage,
        music::MusicMessage,
        placemark::PlacemarkMessage,
        variants::{BalloonProvider, URLOverride},
    },
    util::plist::{get_bool_from_dict, get_string_from_dict, get_string_from_nested_dict},
};

/// This struct is not documented by Apple, but represents messages created by
/// `com.apple.messages.URLBalloonProvider`.
#[derive(Debug, PartialEq, Eq)]
pub struct URLMessage<'a> {
    /// The webpage's `<og:title>` attribute
    pub title: Option<&'a str>,
    /// The webpage's `<og:description>` attribute
    pub summary: Option<&'a str>,
    /// The URL that ended up serving content, after all redirects
    pub url: Option<&'a str>,
    /// The original url, before any redirects
    pub original_url: Option<&'a str>,
    /// The type of webpage Apple thinks the link represents
    pub item_type: Option<&'a str>,
    /// Up to 4 image previews displayed in the background of the bubble
    pub images: Vec<&'a str>,
    /// Icons that represent the website, generally the favicon or apple-touch-icon
    pub icons: Vec<&'a str>,
    /// The name of a website
    pub site_name: Option<&'a str>,
    /// Indicates whether this is a placeholder link preview that needs to be loaded
    pub placeholder: bool,
}

impl<'a> BalloonProvider<'a> for URLMessage<'a> {
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError> {
        let url_metadata = URLMessage::get_body(payload)?;
        Ok(URLMessage {
            title: get_string_from_dict(url_metadata, "title"),
            summary: get_string_from_dict(url_metadata, "summary"),
            url: get_string_from_nested_dict(url_metadata, "URL"),
            original_url: get_string_from_nested_dict(url_metadata, "originalURL"),
            item_type: get_string_from_dict(url_metadata, "itemType"),
            images: URLMessage::get_array_from_nested_dict(url_metadata, "images")
                .unwrap_or_default(),
            icons: URLMessage::get_array_from_nested_dict(url_metadata, "icons")
                .unwrap_or_default(),
            site_name: get_string_from_dict(url_metadata, "siteName"),
            placeholder: get_bool_from_dict(url_metadata, "richLinkIsPlaceholder").unwrap_or(false),
        })
    }
}

impl<'a> URLMessage<'a> {
    /// Gets the subtype of the URL message based on the payload
    pub fn get_url_message_override(
        payload: &'a Value,
    ) -> Result<URLOverride<'a>, PlistParseError> {
        if let Ok(balloon) = CollaborationMessage::from_map(payload) {
            return Ok(URLOverride::Collaboration(balloon));
        }
        if let Ok(balloon) = MusicMessage::from_map(payload) {
            return Ok(URLOverride::AppleMusic(balloon));
        }
        if let Ok(balloon) = AppStoreMessage::from_map(payload) {
            return Ok(URLOverride::AppStore(balloon));
        }
        if let Ok(balloon) = PlacemarkMessage::from_map(payload) {
            return Ok(URLOverride::SharedPlacemark(balloon));
        }
        if let Ok(balloon) = URLMessage::from_map(payload) {
            return Ok(URLOverride::Normal(balloon));
        }
        Err(PlistParseError::NoPayload)
    }

    /// Extract the main dictionary of data from the body of the payload
    ///
    /// There are two known ways this data is stored: the more recent `richLinkMetadata` style,
    /// or some kind of social integration stored under a `metadata` key
    fn get_body(payload: &'a Value) -> Result<&'a Value, PlistParseError> {
        let root_dict = payload.as_dictionary().ok_or_else(|| {
            PlistParseError::InvalidType("root".to_string(), "dictionary".to_string())
        })?;

        if let Some(meta) = root_dict.get("richLinkMetadata") {
            return Ok(meta);
        }
        if let Some(meta) = root_dict.get("metadata") {
            return Ok(meta);
        }
        Err(PlistParseError::NoPayload)
    }

    /// Extract the array of image URLs from a URL message payload.
    ///
    /// The array consists of dictionaries that look like this:
    /// ```json
    /// [
    ///     {
    ///         "size": String("{0, 0}"),
    ///         "URL": {
    ///              "URL": String("https://chrissardegna.com/example.png")
    ///          },
    ///         "images": Integer(1)
    ///     },
    ///     ...
    /// ]
    /// ```
    fn get_array_from_nested_dict(payload: &'a Value, key: &str) -> Option<Vec<&'a str>> {
        payload
            .as_dictionary()?
            .get(key)?
            .as_dictionary()?
            .get(key)?
            .as_array()?
            .iter()
            .map(|item| get_string_from_nested_dict(item, "URL"))
            .collect()
    }

    /// Get the redirected URL from a URL message, falling back to the original URL, if it exists
    #[must_use]
    pub fn get_url(&self) -> Option<&str> {
        self.url.or(self.original_url)
    }
}

#[cfg(test)]
mod url_tests {
    use crate::{
        message_types::{url::URLMessage, variants::BalloonProvider},
        util::plist::parse_ns_keyed_archiver,
    };
    use plist::Value;
    use std::env::current_dir;
    use std::fs::File;

    #[test]
    fn test_parse_url_me() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/url_message/URL.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = URLMessage::from_map(&parsed).unwrap();
        let expected = URLMessage {
            title: Some("Christopher Sardegna"),
            summary: None,
            url: Some("https://chrissardegna.com/"),
            original_url: Some("https://chrissardegna.com"),
            item_type: None,
            images: vec![],
            icons: vec!["https://chrissardegna.com/favicon.ico"],
            site_name: None,
            placeholder: false,
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_url_me_metadata() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/url_message/MetadataURL.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = URLMessage::from_map(&parsed).unwrap();
        let expected = URLMessage {
            title: Some("Christopher Sardegna"),
            summary: Some("Sample page description"),
            url: Some("https://chrissardegna.com"),
            original_url: Some("https://chrissardegna.com"),
            item_type: Some("article"),
            images: vec!["https://chrissardegna.com/ddc-facebook-icon.png"],
            icons: vec![
                "https://chrissardegna.com/apple-touch-icon-180x180.png",
                "https://chrissardegna.com/ddc-icon-32x32.png",
                "https://chrissardegna.com/ddc-icon-16x16.png",
            ],
            site_name: Some("Christopher Sardegna"),
            placeholder: false,
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_url_twitter() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/url_message/Twitter.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = URLMessage::from_map(&parsed).unwrap();
        let expected = URLMessage {
            title: Some("Christopher Sardegna on Twitter"),
            summary: Some("“Hello Twitter, meet Bella”"),
            url: Some("https://twitter.com/rxcs/status/1175874352946077696"),
            original_url: Some("https://twitter.com/rxcs/status/1175874352946077696"),
            item_type: Some("article"),
            images: vec![
                "https://pbs.twimg.com/media/EFGLfR2X4AE8ItK.jpg:large",
                "https://pbs.twimg.com/media/EFGLfRmX4AMnwqW.jpg:large",
                "https://pbs.twimg.com/media/EFGLfRlXYAYn9Ce.jpg:large",
            ],
            icons: vec![
                "https://abs.twimg.com/icons/apple-touch-icon-192x192.png",
                "https://abs.twimg.com/favicons/favicon.ico",
            ],
            site_name: Some("Twitter"),
            placeholder: false,
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_url_reminder() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/url_message/Reminder.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = URLMessage::from_map(&parsed).unwrap();
        let expected = URLMessage {
            title: None,
            summary: None,
            url: None,
            original_url: Some(
                "https://www.icloud.com/reminders/ZmFrZXVybF9mb3JfcmVtaW5kZXI#TestList",
            ),
            item_type: None,
            images: vec![],
            icons: vec![],
            site_name: None,
            placeholder: false,
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_get_url() {
        let expected = URLMessage {
            title: Some("Christopher Sardegna"),
            summary: None,
            url: Some("https://chrissardegna.com/"),
            original_url: Some("https://chrissardegna.com"),
            item_type: None,
            images: vec![],
            icons: vec!["https://chrissardegna.com/favicon.ico"],
            site_name: None,
            placeholder: false,
        };
        assert_eq!(expected.get_url(), Some("https://chrissardegna.com/"));
    }

    #[test]
    fn test_get_original_url() {
        let expected = URLMessage {
            title: Some("Christopher Sardegna"),
            summary: None,
            url: None,
            original_url: Some("https://chrissardegna.com"),
            item_type: None,
            images: vec![],
            icons: vec!["https://chrissardegna.com/favicon.ico"],
            site_name: None,
            placeholder: false,
        };
        assert_eq!(expected.get_url(), Some("https://chrissardegna.com"));
    }

    #[test]
    fn test_get_no_url() {
        let expected = URLMessage {
            title: Some("Christopher Sardegna"),
            summary: None,
            url: None,
            original_url: None,
            item_type: None,
            images: vec![],
            icons: vec!["https://chrissardegna.com/favicon.ico"],
            site_name: None,
            placeholder: false,
        };
        assert_eq!(expected.get_url(), None);
    }
}

#[cfg(test)]
mod url_override_tests {
    use crate::{
        message_types::{url::URLMessage, variants::URLOverride},
        util::plist::parse_ns_keyed_archiver,
    };
    use plist::Value;
    use std::env::current_dir;
    use std::fs::File;

    #[test]
    fn can_parse_normal() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/url_message/URL.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = URLMessage::get_url_message_override(&parsed).unwrap();
        assert!(matches!(balloon, URLOverride::Normal(_)));
    }

    #[test]
    fn can_parse_music() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/music_message/AppleMusic.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = URLMessage::get_url_message_override(&parsed).unwrap();
        assert!(matches!(balloon, URLOverride::AppleMusic(_)));
    }

    #[test]
    fn can_parse_app_store() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_store/AppStoreLink.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = URLMessage::get_url_message_override(&parsed).unwrap();
        assert!(matches!(balloon, URLOverride::AppStore(_)));
    }

    #[test]
    fn can_parse_collaboration() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/collaboration_message/Freeform.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = URLMessage::get_url_message_override(&parsed).unwrap();
        assert!(matches!(balloon, URLOverride::Collaboration(_)));
    }

    #[test]
    fn can_parse_placemark() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/shared_placemark/SharedPlacemark.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = URLMessage::get_url_message_override(&parsed).unwrap();
        println!("{balloon:?}");
        assert!(matches!(balloon, URLOverride::SharedPlacemark(_)));
    }
}
