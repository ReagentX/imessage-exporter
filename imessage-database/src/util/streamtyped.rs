/*!
 This module contains logic to parse text from streamtyped binary data
*/

use crate::error::streamtyped::StreamTypedError;

/// Literals: `[<Start of Heading> (SOH), +]`
/// - <https://www.compart.com/en/unicode/U+0001>
/// - <https://www.compart.com/en/unicode/U+002b>
const START_PATTERN: [u8; 2] = [0x0001, 0x002b];

/// Literal: `[<Start of Selected Area> (SSA), <Index> (IND)]`
/// - https://www.compart.com/en/unicode/U+0086
/// - <https://www.compart.com/en/unicode/U+0084>
const END_PATTERN: [u8; 2] = [0x0086, 0x0084];

/// Parse the body text from a known type of `streamtyped` `attributedBody` file.
///
/// `attributedBody` `streamtyped` data looks like:
///
/// ```txt
/// streamtyped���@���NSAttributedString�NSObject����NSString��+Example message  ��iI���� NSDictionary��i����__kIMMessagePartAttributeName����NSNumber��NSValue��*������
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
        // TODO: Why does this logic work? Maybe the offset can be derived from the bytes
        // If the bytes are valid unicode, only one char prefixes the actual message
        // ['\u{6}', 'T', ...] where `T` is the first real char
        // The prefix char is not always the same
        Ok(string) => drop_chars(1, string),
        // If the bytes are not valid unicode, 3 chars prefix the actual message
        // ['�', '�', '\0', 'T', ...] where `T` is the first real char
        // The prefix chars are not always the same
        Err(string) => drop_chars(3, string),
    }
}

/// Drop `offset` chars from the front of a String
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
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/streamtyped/AttributedBodyTextOnly");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "Noter test".to_string();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_space() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/streamtyped/AttributedBodyTextOnly2");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "Test 3".to_string();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_weird_font() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/streamtyped/WeirdText");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "𝖍𝖊𝖑𝖑𝖔 𝖜𝖔𝖗𝖑𝖉".to_string();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_url() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/streamtyped/URL");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "https://github.com/ReagentX/Logria".to_string();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_multi_part() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/streamtyped/MultiPart");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "\u{FFFC}test 1\u{FFFC}test 2 \u{FFFC}test 3".to_string();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_app() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/streamtyped/ExtraData");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "This is parsing";

        assert_eq!(&parsed[..expected.len()], expected);
    }

    #[test]
    fn test_parse_text_blank() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/streamtyped/Blank");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes);

        assert!(&parsed.is_err());
    }

    #[test]
    fn test_can_drop_chars() {
        assert_eq!(
            drop_chars(1, String::from("Hello world")).unwrap(),
            String::from("ello world")
        );
    }

    #[test]
    fn test_can_drop_chars_none() {
        assert_eq!(
            drop_chars(0, String::from("Hello world")).unwrap(),
            String::from("Hello world")
        );
    }

    #[test]
    fn test_cant_drop_all() {
        assert!(drop_chars(1000, String::from("Hello world")).is_err());
    }
}
