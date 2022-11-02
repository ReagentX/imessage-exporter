/*!
These are the link previews that iMessage generates when sending links and can
contain metadata even if the webpage the link points to no longer exists on the internet.
*/

use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::variants::BalloonProvider,
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
    /// Icons that represent the websute, generally the favicon or apple-touch-icon
    pub icons: Vec<&'a str>,
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
                .unwrap_or(vec![]),
            icons: URLMessage::get_array_from_nested_dict(url_metadata, "icons").unwrap_or(vec![]),
            placeholder: get_bool_from_dict(url_metadata, "richLinkIsPlaceholder").unwrap_or(false),
        })
    }
}

impl<'a> URLMessage<'a> {
    /// Extract the main dictionary of data from the body of the payload
    ///
    /// There are two known ways this data is stored: the more recent `richLinkMetadata` style,
    /// or some kind of social integration stored under a `metadata` key
    fn get_body(payload: &'a Value) -> Result<&'a Value, PlistParseError> {
        let root_dict = payload.as_dictionary().ok_or(PlistParseError::InvalidType(
            "root".to_string(),
            "dictionary".to_string(),
        ))?;

        if root_dict.contains_key("richLinkMetadata") {
            // Unwrap is safe here because we validate the key
            return Ok(root_dict.get("richLinkMetadata").unwrap());
        } else if root_dict.contains_key("metadata") {
            return Ok(root_dict.get("metadata").unwrap());
        };
        Err(PlistParseError::MissingKey(
            "richLinkMetadata or metadata".to_string(),
        ))
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
}

#[cfg(test)]
mod tests {
    use crate::{
        message_types::{url::URLMessage, variants::BalloonProvider},
        util::plist::parse_plist,
    };
    use plist::Value;
    use std::env::current_dir;
    use std::fs::File;

    #[test]
    fn test_parse_url_me() {
        let plist_path = current_dir().unwrap().as_path().join("test_data/URL.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_plist(&plist).unwrap();

        let balloon = URLMessage::from_map(&parsed).unwrap();
        let expected = URLMessage {
            title: Some("Christopher Sardegna"),
            summary: None,
            url: Some("https://chrissardegna.com/"),
            original_url: Some("https://chrissardegna.com"),
            item_type: None,
            images: vec![],
            icons: vec!["https://chrissardegna.com/favicon.ico"],
            placeholder: false,
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_url_me_metadata() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/MetadataURL.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_plist(&plist).unwrap();

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
                "https://chrissardegna.com/ddc-icon-16x16.png"
            ],
            placeholder: false,
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_url_twitter() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/Twitter.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_plist(&plist).unwrap();

        let balloon = URLMessage::from_map(&parsed).unwrap();
        println!("{balloon:?}\n");
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
            placeholder: false,
        };

        assert_eq!(balloon, expected);
    }
}
