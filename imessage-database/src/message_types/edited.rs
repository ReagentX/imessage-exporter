/*!
 Logic and containers for the `message_summary_info` of an edited or unsent iMessage.
*/

use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::variants::BalloonProvider,
    util::{
        dates::TIMESTAMP_FACTOR,
        plist::{extract_bytes_key, extract_dictionary, extract_int_key},
        streamtyped::parse,
    },
};

#[derive(Debug, PartialEq, Eq)]
pub struct EditedEvent<'a> {
    /// The date the messages were edited
    pub date: i64,
    /// The content of the edited messages in [`streamtyped`](crate::util::streamtyped) format
    pub text: String,
    /// A GUID reference to another message
    pub guid: Option<&'a str>,
}

impl<'a> EditedEvent<'a> {
    fn new(date: i64, text: String, guid: Option<&'a str>) -> Self {
        Self { date, text, guid }
    }
}

/// iMessage permits editing sent messages up to five times
/// within 15 minutes of sending the first message and unsending
/// sent messages within 2 minutes.
///
/// Edited or unsent messages are stored with a `NULL` `text` field.
/// Edited messages include `message_summary_info` that contains an array of
/// [`streamtyped`](crate::util::streamtyped) data where each array item contains the edited
/// message. The order in the array represents the order the messages
/// were edited in, i.e. item 0 was the original and the last item is
/// the current message.
///
/// For each dictionary item in this array, The `d` key represents the
/// time the message was edited and the `t` key represents the message's
/// `attributedBody` text in the [`streamtyped`](crate::util::streamtyped) format.
///
/// There is no data in the array if the message was unsent.
///
/// Apple describes editing and unsending messages [here](https://support.apple.com/guide/iphone/unsend-and-edit-messages-iphe67195653/ios).
#[derive(Debug, PartialEq, Eq)]
pub struct EditedMessage<'a> {
    pub events: Vec<EditedEvent<'a>>,
}

impl<'a> BalloonProvider<'a> for EditedMessage<'a> {
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError> {
        // Parse payload
        let plist_root = payload.as_dictionary().ok_or_else(|| {
            PlistParseError::InvalidType("root".to_string(), "dictionary".to_string())
        })?;

        if !plist_root.contains_key("ec") {
            return Ok(Self::empty());
        }

        let edited_messages = extract_dictionary(plist_root, "ec")?
            .values()
            .next()
            .ok_or_else(|| PlistParseError::MissingKey("ec".to_string()))?
            .as_array()
            .ok_or_else(|| PlistParseError::InvalidType("ec".to_string(), "array".to_string()))?;

        let mut edited = Self::with_capacity(edited_messages.len());

        for (idx, message) in edited_messages.iter().enumerate() {
            let message_data = message
                .as_dictionary()
                .ok_or_else(|| PlistParseError::InvalidTypeIndex(idx, "dictionary".to_string()))?;

            let timestamp = extract_int_key(message_data, "d")? * TIMESTAMP_FACTOR;

            let raw_streamtyped = extract_bytes_key(message_data, "t")?;
            let text =
                parse(raw_streamtyped.to_vec()).map_err(PlistParseError::StreamTypedError)?;

            let guid = message_data.get("bcg").and_then(|item| item.as_string());

            edited.events.push(EditedEvent::new(timestamp, text, guid));
        }

        Ok(edited)
    }
}

impl<'a> EditedMessage<'a> {
    /// A new empty edited message
    fn empty() -> Self {
        EditedMessage { events: Vec::new() }
    }

    /// A new message with a preallocated capacity
    fn with_capacity(capacity: usize) -> Self {
        EditedMessage {
            events: Vec::with_capacity(capacity),
        }
    }

    /// `true` if the message was deleted, `false` if it was edited
    pub fn is_deleted(&self) -> bool {
        self.events.is_empty()
    }

    /// Gets a tuple for the message at the provided position
    pub fn item_at(&self, position: usize) -> Option<&EditedEvent> {
        self.events.get(position)
    }

    /// Gets the number of items in the edit history
    pub fn items(&self) -> usize {
        self.events.len()
    }
}

#[cfg(test)]
mod tests {
    use crate::message_types::edited::EditedEvent;
    use crate::message_types::{edited::EditedMessage, variants::BalloonProvider};
    use plist::Value;
    use std::env::current_dir;
    use std::fs::File;

    #[test]
    fn test_parse_edited() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/edited_message/Edited.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = EditedMessage::from_map(&plist).unwrap();

        let expected = EditedMessage {
            events: vec![
                EditedEvent::new(690513474000000000, "First message  ".to_string(), None),
                EditedEvent::new(690513480000000000, "Edit 1".to_string(), None),
                EditedEvent::new(690513485000000000, "Edit 2".to_string(), None),
                EditedEvent::new(690513494000000000, "Edited message".to_string(), None),
            ],
        };

        assert_eq!(parsed, expected);
        assert_eq!(parsed.items(), 4);

        let expected_item = Some(expected.events.first().unwrap());
        assert_eq!(parsed.item_at(0), expected_item);
    }

    #[test]
    fn test_parse_edited_to_link() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/edited_message/EditedToLink.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = EditedMessage::from_map(&plist).unwrap();

        let expected = EditedMessage {
            events: vec![
                EditedEvent::new(690514004000000000, "here we go!".to_string(), None),
                EditedEvent::new(
                    690514772000000000,
                    "https://github.com/ReagentX/imessage-exporter/issues/10".to_string(),
                    Some("292BF9C6-C9B8-4827-BE65-6EA1C9B5B384"),
                ),
            ],
        };

        assert_eq!(parsed, expected);
        assert_eq!(parsed.items(), 2);

        let expected_item = Some(expected.events.first().unwrap());
        assert_eq!(parsed.item_at(0), expected_item);
    }

    #[test]
    fn test_parse_edited_to_link_and_back() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/edited_message/EditedToLinkAndBack.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = EditedMessage::from_map(&plist).unwrap();

        let expected = EditedMessage {
            events: vec![
                EditedEvent::new(
                    690514809000000000,
                    "This is a normal message".to_string(),
                    None,
                ),
                EditedEvent::new(
                    690514819000000000,
                    "Edit to a url https://github.com/ReagentX/imessage-exporter/issues/10"
                        .to_string(),
                    Some("0B9103FE-280C-4BD0-A66F-4EDEE3443247"),
                ),
                EditedEvent::new(
                    690514834000000000,
                    "And edit it back to a normal message...".to_string(),
                    Some("0D93DF88-05BA-4418-9B20-79918ADD9923"),
                ),
            ],
        };

        assert_eq!(parsed, expected);
        assert_eq!(parsed.items(), 3);

        let expected_item = Some(expected.events.first().unwrap());
        assert_eq!(parsed.item_at(0), expected_item);
    }

    #[test]
    fn test_parse_deleted() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/edited_message/Deleted.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = EditedMessage::from_map(&plist).unwrap();

        let expected = EditedMessage { events: vec![] };

        assert_eq!(parsed, expected);
        assert!(parsed.is_deleted());
        assert_eq!(parsed.items(), 0);

        let expected_item = None;
        assert_eq!(parsed.item_at(0), expected_item);
    }
}
