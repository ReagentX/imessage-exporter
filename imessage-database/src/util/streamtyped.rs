/*!
 This module contains logic to parse text from streamtyped binary data
*/

use crate::error::streamtyped::StreamTypedError;

/// Literals: `[<Start of Heading> (SOH), +]`
/// - <https://www.compart.com/en/unicode/U+0001>
/// - <https://www.compart.com/en/unicode/U+002b>
const START_PATTERN: [u8; 2] = [0x0001, 0x002b];
/// Literal: `<Index> (IND)`
/// - <https://www.compart.com/en/unicode/U+0084>
const END_PATTERN: u8 = 0x0084;

/// Parse the body text from a known type of `streamtyped` `attributedBody` file
pub fn parse(mut stream: Vec<u8>) -> Result<String, StreamTypedError> {
    // Find the start index and trim
    for idx in 0..stream.len() as usize {
        if idx + 2 > stream.len() {
            return Err(StreamTypedError::NoStartPattern);
        }
        let part = &stream[idx..idx + 2];

        if part == START_PATTERN {
            // Remove one more byte that seems to change every message
            // TODO: is this always correct?
            stream.drain(..idx + 3);
            break;
        }
    }

    for idx in 0..stream.len() as usize {
        if idx >= stream.len() {
            return Err(StreamTypedError::NoEndPattern);
        }
        let part = &stream[idx];

        if part == &END_PATTERN {
            stream.truncate(idx - 1);
            break;
        }
    }

    match String::from_utf8(stream)
        .map_err(|non_utf8| String::from_utf8_lossy(non_utf8.as_bytes()).into_owned())
    {
        Ok(string) => Ok(string),
        Err(string) => Ok(string),
    }
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use std::fs::File;
    use std::io::Read;
    use std::vec;

    use crate::util::streamtyped::parse;

    #[test]
    fn test_parse_text() {
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
        let plist_path = current_dir().unwrap().as_path().join("test_data/streamtyped/WeirdText");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "𝖍𝖊𝖑𝖑𝖔 𝖜𝖔𝖗𝖑𝖉".to_string();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_url() {
        let plist_path = current_dir().unwrap().as_path().join("test_data/streamtyped/URL");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "https://github.com/ReagentX/Logria".to_string();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_text_multi_part() {
        let plist_path = current_dir().unwrap().as_path().join("test_data/streamtyped/MultiPart");
        let mut file = File::open(plist_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        let parsed = parse(bytes).unwrap();

        let expected = "\u{FFFC}test 1\u{FFFC}test 2 \u{FFFC}test 3".to_string();

        assert_eq!(parsed, expected);
    }
}
