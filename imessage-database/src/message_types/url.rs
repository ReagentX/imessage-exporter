/*!
These are the link previews that iMessage generates when sending links and can
contain metadata even if the webpage the link points to no longer exists on the internet.
*/

use std::collections::HashMap;

use plist::Value;

use crate::{
    error::plist::PlistParseError, message_types::variants::BalloonProvider,
    util::plist::extract_parsed_str,
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
    /// The type of image preview to render, sometiems this is the favicon
    pub mime_type: Option<&'a str>,
    /// The type of webpage Apple thinks the link represents
    pub item_type: Option<&'a str>,
    pub image_type: Option<u64>,
    pub placeholder: bool,
}

impl<'a> BalloonProvider<'a> for URLMessage<'a> {
    fn from_map(payload: &'a HashMap<&'a str, &'a Value>) -> Result<Self, PlistParseError<'a>> {
        Ok(URLMessage {
            title: extract_parsed_str(payload, "title"),
            summary: extract_parsed_str(payload, "summary"),
            url: extract_parsed_str(payload, "URL"),
            original_url: extract_parsed_str(payload, "originalURL"),
            mime_type: extract_parsed_str(payload, "MIMEType"),
            item_type: extract_parsed_str(payload, "itemType"),
            image_type: payload
                .get("imageType")
                .and_then(|item| item.as_unsigned_integer()),
            placeholder: payload
                .get("richLinkIsPlaceholder")
                .and_then(|item| item.as_boolean())
                .unwrap_or(false),
        })
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
            mime_type: Some("image/vnd.microsoft.icon"),
            item_type: None,
            image_type: Some(0),
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
        println!("{parsed:?}");

        let balloon = URLMessage::from_map(&parsed).unwrap();
        let expected = URLMessage {
            title: Some("Christopher Sardegna on Twitter"),
            summary: Some("“Hello Twitter, meet Bella”"),
            url: Some("https://twitter.com/rxcs/status/1175874352946077696"),
            original_url: Some("https://twitter.com/rxcs/status/1175874352946077696"),
            mime_type: Some("image/png"),
            item_type: Some("article"),
            image_type: Some(0),
            placeholder: false,
        };

        assert_eq!(balloon, expected);
    }
}
