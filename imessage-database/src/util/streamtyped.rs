/*!
 Legacy text parser for `streamtyped` body blobs.

 This parser extracts plain text from `attributedBody` when the full
 typedstream deserializer cannot reconstruct the attributed string.
*/

use crate::error::streamtyped::StreamTypedError;

/// Literals: `[<Start of Heading> (SOH), +]`
/// - <https://www.compart.com/en/unicode/U+0001>
/// - <https://www.compart.com/en/unicode/U+002b>
const START_PATTERN: [u8; 2] = [0x0001, 0x002b];

/// Literals: `[<Start of Selected Area> (SSA), <Index> (IND)]`
/// - <https://www.compart.com/en/unicode/U+0086>
/// - <https://www.compart.com/en/unicode/U+0084>
const END_PATTERN: [u8; 2] = [0x0086, 0x0084];

/// Parse plain body [text](crate::tables::messages::message::Message::text) from a `streamtyped` `attributedBody` blob.
///
/// `attributedBody` `typedstream` data looks like:
///
/// ```txt
/// streamtyped���@���NSAttributedString�NSObject����NSString��+Example message  ��iI���� NSDictionary��i����__kIMMessagePartAttributeName����NSNumber��NSValue��*������
/// ```
///
/// In that example, the returned body text would be `"Example message"`.
///
/// ## Parser scope
///
/// This string parser only supports unstyled text.
///
/// If the message has attachments, there will be one [`U+FFFC`](https://www.compart.com/en/unicode/U+FFFC) character
/// for each attachment and one [`U+FFFD`](https://www.compart.com/en/unicode/U+FFFD) for app messages that we need
/// to format.
///
/// ## Sample
///
/// An iMessage that contains body text like:
///
/// ```
/// let message_text = "\u{FFFC}Check out this photo!";
/// ```
///
/// produces fallback body components like:
///
/// ```
/// use imessage_database::message_types::text_effects::text_effect::TextEffect;
/// use imessage_database::tables::messages::{models::{AttributedRange, BubbleComponent, AttachmentMeta}};
///
/// let result = vec![
///     BubbleComponent::Run(vec![AttributedRange::attachment(0, 3, AttachmentMeta::default())]),
///     BubbleComponent::Run(vec![AttributedRange::text(3, 24, vec![TextEffect::Default])]),
/// ];
/// ```
pub fn parse(mut stream: Vec<u8>) -> Result<String, StreamTypedError> {
    // Find the start index and drain
    for idx in 0..stream.len() {
        if idx + 2 > stream.len() {
            return Err(StreamTypedError::NoStartPattern);
        }
        let part = &stream[idx..idx + 2];

        if part == START_PATTERN {
            // Remove the start pattern from the string
            stream.drain(..idx + 2);
            break;
        }
    }

    // Find the end index and truncate
    for idx in 1..stream.len() {
        if idx >= stream.len() - 2 {
            return Err(StreamTypedError::NoEndPattern);
        }
        let part = &stream[idx..idx + 2];

        if part == END_PATTERN {
            // Remove the end pattern from the string
            stream.truncate(idx);
            break;
        }
    }

    // `from_utf8` doesn't allocate, but `from_utf8_lossy` does, so we try the allocation-free
    // version first and only allocate if it fails
    match String::from_utf8(stream)
        .map_err(|non_utf8| String::from_utf8_lossy(non_utf8.as_bytes()).into_owned())
    {
        // Valid UTF-8 bodies carry one non-text prefix character.
        // ['\u{6}', 'T', ...] where `T` is the first real char
        Ok(string) => drop_chars(1, string),
        // Lossy-decoded bodies carry three replacement/prefix characters.
        // ['�', '�', '\0', 'T', ...] where `T` is the first real char
        Err(string) => drop_chars(3, string),
    }
}

/// Drop `offset` chars from the front of a string.
fn drop_chars(offset: usize, mut string: String) -> Result<String, StreamTypedError> {
    // Find the index of the specified character offset
    let (position, _) = string
        .char_indices()
        .nth(offset)
        .ok_or(StreamTypedError::InvalidPrefix)?;

    // Remove the prefix and give the String back
    string.drain(..position);
    Ok(string)
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use std::fs::File;
    use std::io::Read;
    use std::vec;

    use crate::util::streamtyped::{drop_chars, parse};

    #[test]
    fn test_parse_text_clean() {
        let cases = [
            ("plain text", "AttributedBodyTextOnly", "Noter test"),
            ("text with a space", "AttributedBodyTextOnly2", "Test 3"),
            ("url", "URL", "https://github.com/ReagentX/Logria"),
        ];
        for (label, fixture, expected) in cases {
            let plist_path = current_dir()
                .unwrap()
                .as_path()
                .join("test_data/typedstream")
                .join(fixture);
            let mut file = File::open(plist_path).unwrap();
            let mut bytes = vec![];
            file.read_to_end(&mut bytes).unwrap();
            let parsed = parse(bytes).unwrap();

            assert_eq!(parsed, expected, "{label}");
        }
    }

    #[test]
    fn test_parse_text_weird_font() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/WeirdText");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "𝖍𝖊𝖑𝖑𝖔 𝖜𝖔𝖗𝖑𝖉".to_string();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_multi_part() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/MultiPart");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "\u{FFFC}test 1\u{FFFC}test 2 \u{FFFC}test 3".to_string();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_app() {
        // This test removed a block of text so the pointers become misaligned during parsing
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/ExtraData");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "This is parsing";

        assert_eq!(&parsed[..expected.len()], expected);
    }

    #[test]
    fn test_parse_text_long() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/LongMessage");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "Sed nibh velit,";

        assert_eq!(&parsed[..expected.len()], expected);
        assert_eq!(parsed.len(), 2359);
    }

    #[test]
    fn test_parse_text_blank() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Blank");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes);

        assert!(&parsed.is_err());
    }

    #[test]
    fn test_parse_text_multi_part_deleted() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/MultiPartWithDeleted");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();
        println!("{parsed:?}");

        let expected = "From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r";

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_attachment() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Attachment");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();
        println!("{parsed:?}");

        let expected =
            "\u{FFFC}This is how the notes look to me fyi, in case it helps make sense of anything";

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_array() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Array");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();
        println!("{parsed:?}");

        let expected = "A single ChatGPT instance takes 5MW of power to run";

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_cant_drop_all() {
        assert!(drop_chars(1000, String::from("Hello world")).is_err());
    }
}
