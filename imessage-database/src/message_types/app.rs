/*!
  App messages are a specific type of message that developers can generate with their apps.
  Some built-in functionality also uses App Messages, like Apple Pay or Handwriting.
*/
use std::collections::HashMap;

use plist::Value;

/// This struct represents Apple's [`MSMessageTemplateLayout`](https://developer.apple.com/documentation/messages/msmessagetemplatelayout).
struct AppMessage<'a> {
    /// An image used to represent the message in the transcript
    image: Option<&'a str>,
    /// A media file used to represent the message in the transcript
    url: Option<&'a str>,
    /// The title for the image or media file
    title: Option<&'a str>,
    /// The subtitle for the image or media file
    subtitle: Option<&'a str>,
    /// A left-aligned caption for the message bubble
    caption: Option<&'a str>,
    /// A left-aligned subcaption for the message bubble
    subcaption: Option<&'a str>,
    /// A right-aligned caption for the message bubble
    trailing_caption: Option<&'a str>,
    /// A right-aligned subcaption for the message bubble
    trailing_subcaption: Option<&'a str>,
    /// The name of the app that created this message
    app_name: Option<&'a str>,
}

/// This struct is not documented by Apple, but represents messages created by
/// `com.apple.messages.URLBalloonProvider`. These are the link previews that
/// iMessage generates when sending links and can contain metadata even if the
/// webpage the link points to no longer exists on the internet.
struct URLMessage<'a> {
    /// The webpage's `<title>` attribute
    title: Option<&'a str>,
    /// The URL that ended up serving content, after all redirects
    url: Option<&'a str>,
    ///The original url, before any redirects
    original_url: Option<&'a str>,
    /// The type of image preview to render, sometiems this is the favicon
    mime_type: Option<&'a str>,
    image_type: Option<&'a u64>,
    placeholder: bool,
    /// The name of the app that created this message
    app_name: Option<&'a str>,
}

/// Defines behavior for different types of messages that have custom balloons
// TODO: Implement this for the two above message types
trait BalloonProvider {
    fn from_map(payload: HashMap<&str, &Value>) -> Self;
}

#[cfg(test)]
mod tests {
    use crate::util::plist::parse_plist;
    use plist::Value;
    use std::env::current_dir;
    use std::fs::File;

    #[test]
    fn test_parse_apple_pay_pay_1_dollar() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/Pay1.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();

        // TODO: real tests for all of these methods
        let parsed = parse_plist(&plist).unwrap();
        println!("{:?}", parse_plist(&plist).unwrap());
    }

    #[test]
    fn test_parse_opentable_invite() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/OpenTableInvited.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("{:?}", parse_plist(&plist).unwrap());
    }

    #[test]
    fn test_parse_url() {
        let plist_path = current_dir().unwrap().as_path().join("test_data/URL.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("{:?}", parse_plist(&plist).unwrap());
    }

    #[test]
    fn test_parse_slideshow() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/Slideshow.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("{:?}", parse_plist(&plist).unwrap());
    }

    #[test]
    fn test_parse_game() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/Game.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("{:?}", parse_plist(&plist).unwrap());
    }

    #[test]
    fn test_parse_business() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/Business.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        println!("{:?}", parse_plist(&plist).unwrap());
    }
}
