/*!
 Edited and unsent message metadata from `message_summary_info`.
*/
use crabstep::TypedStreamDeserializer;
use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::variants::BalloonProvider,
    tables::messages::{body::parse_body_typedstream, models::BubbleComponent},
    util::{
        dates::TIMESTAMP_FACTOR,
        plist::{
            extract_array_key, extract_bytes_key, extract_dictionary, extract_int_key,
            plist_as_dictionary,
        },
    },
};

/// Edit state for one message body part.
#[derive(Debug, PartialEq, Eq)]
pub enum EditStatus {
    /// Body part was edited.
    Edited,
    /// Body part was unsent.
    Unsent,
    /// Body part was not changed.
    Original,
}

/// One edit-history entry for a message part.
#[derive(Debug, PartialEq)]
pub struct EditedEvent {
    /// Edit timestamp in Apple's nanosecond epoch.
    pub date: i64,
    /// The content of the edited message part, deserialized from the
    /// [`typedstream`](https://github.com/ReagentX/crabstep) format.
    pub text: String,
    /// Parsed body components for the edited text.
    pub components: Vec<BubbleComponent>,
    /// GUID reference to another message, when present.
    pub guid: Option<String>,
}

impl EditedEvent {
    pub(crate) fn new(
        date: i64,
        text: String,
        components: Vec<BubbleComponent>,
        guid: Option<String>,
    ) -> Self {
        Self {
            date,
            text,
            components,
            guid,
        }
    }
}

/// Edit state and history for one message body part.
#[derive(Debug, PartialEq)]
pub struct EditedMessagePart {
    /// Current edit state for this part.
    pub status: EditStatus,
    /// Historical edit entries for this part.
    pub edit_history: Vec<EditedEvent>,
}

impl Default for EditedMessagePart {
    fn default() -> Self {
        Self {
            status: EditStatus::Original,
            edit_history: vec![],
        }
    }
}

/// Parsed edit metadata for every body part in a message.
///
/// # Internal Representation
///
/// Edited or unsent messages are stored with a `NULL` `text` field.
/// Edited messages include `message_summary_info` that contains a dictionary
/// with message body part data, including [`typedstream`](https://github.com/ReagentX/crabstep)-encoded
/// edit history. The order of entries in the edit history represents the order
/// the part changed: item `0` is the original text and the last item is the
/// current text.
///
/// ## Message Body Parts
///
/// - `otr`: dictionary of message part indexes.
/// - `rp`: list of unsent message part indexes.
/// - `ec`: dictionary of edited message part indexes to edit history arrays.
/// - Each `ec` item stores `d` (edit timestamp) and `t` (edited
///   `attributedBody` typedstream).
///
/// # Documentation
///
/// Apple describes editing and unsending messages [here](https://support.apple.com/guide/iphone/unsend-and-edit-messages-iphe67195653/ios).
#[derive(Debug, PartialEq)]
pub struct EditedMessage {
    /// One entry per message body part.
    pub parts: Vec<EditedMessagePart>,
}

impl<'a> BalloonProvider<'a> for EditedMessage {
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError> {
        // Parse payload
        let plist_root = plist_as_dictionary(payload)?;

        // Get the parts of the message that may have been altered
        let message_parts = extract_dictionary(plist_root, "otr")?;

        // Prefill edited data
        let mut edited = Self::with_capacity(message_parts.len());
        message_parts
            .values()
            .for_each(|_| edited.parts.push(EditedMessagePart::default()));

        if let Ok(edited_message_events) = extract_dictionary(plist_root, "ec") {
            for (idx, (key, events)) in edited_message_events.iter().enumerate() {
                let events = events
                    .as_array()
                    .ok_or_else(|| PlistParseError::InvalidTypeIndex(idx, "array".to_string()))?;
                let parsed_key = key.parse::<usize>().map_err(|_| {
                    PlistParseError::InvalidType(
                        "ec dictionary key".to_string(),
                        "numeric string".to_string(),
                    )
                })?;

                for (event_idx, event) in events.iter().enumerate() {
                    let message_data = event.as_dictionary().ok_or_else(|| {
                        PlistParseError::InvalidTypeIndex(event_idx, "dictionary".to_string())
                    })?;

                    let timestamp = extract_int_key(message_data, "d")?
                        .checked_mul(TIMESTAMP_FACTOR)
                        .ok_or_else(|| {
                            PlistParseError::InvalidEditedMessage(
                                "edit timestamp out of range".to_string(),
                            )
                        })?;

                    let data = extract_bytes_key(message_data, "t")?;

                    let mut typedstream = TypedStreamDeserializer::new(data);
                    let result = parse_body_typedstream(Some(typedstream.iter_root()?), None)
                        .ok_or_else(|| {
                            PlistParseError::InvalidEditedMessage(
                                "Failed to parse typedstream data".to_string(),
                            )
                        })?;

                    let text = result.text.ok_or_else(|| {
                        PlistParseError::InvalidEditedMessage(
                            "Edit-history entry missing text!".to_string(),
                        )
                    })?;

                    let guid = message_data
                        .get("bcg")
                        .and_then(|item| item.as_string())
                        .map(Into::into);

                    if let Some(item) = edited.parts.get_mut(parsed_key) {
                        item.status = EditStatus::Edited;
                        item.edit_history.push(EditedEvent::new(
                            timestamp,
                            text,
                            result.components,
                            guid,
                        ));
                    }
                }
            }
        }

        if let Ok(unsent_message_indexes) = extract_array_key(plist_root, "rp") {
            for (idx, unsent_message_idx) in unsent_message_indexes.iter().enumerate() {
                let parsed_idx = unsent_message_idx
                    .as_signed_integer()
                    .ok_or_else(|| PlistParseError::InvalidTypeIndex(idx, "int".to_string()))?
                    as usize;
                if let Some(item) = edited.parts.get_mut(parsed_idx) {
                    item.status = EditStatus::Unsent;
                }
            }
        }

        Ok(edited)
    }
}

impl EditedMessage {
    /// Build an empty edit record with capacity for known body parts.
    fn with_capacity(capacity: usize) -> Self {
        EditedMessage {
            parts: Vec::with_capacity(capacity),
        }
    }

    /// Return edit metadata for the given body part index.
    #[must_use]
    pub fn part(&self, index: usize) -> Option<&EditedMessagePart> {
        self.parts.get(index)
    }

    /// `true` when the given body part exists and has not been edited or unsent.
    #[must_use]
    pub fn is_unedited_at(&self, index: usize) -> bool {
        match self.parts.get(index) {
            Some(part) => matches!(part.status, EditStatus::Original),
            None => false,
        }
    }

    /// Number of body parts tracked by this edit payload.
    #[must_use]
    pub fn items(&self) -> usize {
        self.parts.len()
    }
}

#[cfg(test)]
mod test_parser {
    use crate::message_types::edited::{EditStatus, EditedEvent, EditedMessagePart};
    use crate::message_types::text_effects::{style::Style, text_effect::TextEffect};
    use crate::message_types::{edited::EditedMessage, variants::BalloonProvider};
    use crate::tables::messages::models::{AttributedRange, BubbleComponent};

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
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![
                    EditedEvent::new(
                        690513474000000000,
                        "First message  ".to_string(),
                        vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            15,
                            vec![TextEffect::Default],
                        )])],
                        None,
                    ),
                    EditedEvent::new(
                        690513480000000000,
                        "Edit 1".to_string(),
                        vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            6,
                            vec![TextEffect::Default],
                        )])],
                        None,
                    ),
                    EditedEvent::new(
                        690513485000000000,
                        "Edit 2".to_string(),
                        vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            6,
                            vec![TextEffect::Default],
                        )])],
                        None,
                    ),
                    EditedEvent::new(
                        690513494000000000,
                        "Edited message".to_string(),
                        vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            14,
                            vec![TextEffect::Default],
                        )])],
                        None,
                    ),
                ],
            }],
        };

        assert_eq!(parsed, expected);

        let expected_item = Some(expected.parts.first().unwrap());
        assert_eq!(parsed.part(0), expected_item);
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
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![], // The first part of this is the URL preview
                },
                EditedMessagePart {
                    status: EditStatus::Edited,
                    edit_history: vec![
                        EditedEvent::new(
                            690514004000000000,
                            "here we go!".to_string(),
                            vec![BubbleComponent::Run(vec![AttributedRange::text(
                                0,
                                11,
                                vec![TextEffect::Default],
                            )])],
                            None,
                        ),
                        EditedEvent::new(
                            690514772000000000,
                            "https://github.com/ReagentX/imessage-exporter/issues/10".to_string(),
                            vec![BubbleComponent::Run(vec![AttributedRange::text(
                                0,
                                55,
                                vec![TextEffect::Default],
                            )])],
                            Some("292BF9C6-C9B8-4827-BE65-6EA1C9B5B384".to_string()),
                        ),
                    ],
                },
            ],
        };

        assert_eq!(parsed, expected);
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
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![
                    EditedEvent::new(
                        690514809000000000,
                        "This is a normal message".to_string(),
                        vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            24,
                            vec![TextEffect::Default],
                        )])],
                        None,
                    ),
                    EditedEvent::new(
                        690514819000000000,
                        "Edit to a url https://github.com/ReagentX/imessage-exporter/issues/10"
                            .to_string(),
                        vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            69,
                            vec![TextEffect::Default],
                        )])],
                        Some("0B9103FE-280C-4BD0-A66F-4EDEE3443247".to_string()),
                    ),
                    EditedEvent::new(
                        690514834000000000,
                        "And edit it back to a normal message...".to_string(),
                        vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            39,
                            vec![TextEffect::Default],
                        )])],
                        Some("0D93DF88-05BA-4418-9B20-79918ADD9923".to_string()),
                    ),
                ],
            }],
        };

        assert_eq!(parsed, expected);
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

        let expected = EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
            }],
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_multipart_deleted() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/edited_message/MultiPartOneDeleted.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = EditedMessage::from_map(&plist).unwrap();

        let expected = EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                },
            ],
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_multipart_edited_and_deleted() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/edited_message/EditedAndDeleted.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = EditedMessage::from_map(&plist).unwrap();

        let expected = EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Edited,
                    edit_history: vec![
                        EditedEvent::new(
                            743907180000000000,
                            "Second message".to_string(),
                            vec![BubbleComponent::Run(vec![AttributedRange::text(
                                0,
                                14,
                                vec![TextEffect::Default],
                            )])],
                            None,
                        ),
                        EditedEvent::new(
                            743907190000000000,
                            "Second message got edited!".to_string(),
                            vec![BubbleComponent::Run(vec![AttributedRange::text(
                                0,
                                26,
                                vec![TextEffect::Default],
                            )])],
                            None,
                        ),
                    ],
                },
            ],
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_multipart_edited_and_unsent() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/edited_message/EditedAndUnsent.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = EditedMessage::from_map(&plist).unwrap();

        let expected = EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Edited,
                    edit_history: vec![
                        EditedEvent::new(
                            743907435000000000,
                            "Second test".to_string(),
                            vec![BubbleComponent::Run(vec![AttributedRange::text(
                                0,
                                11,
                                vec![TextEffect::Default],
                            )])],
                            None,
                        ),
                        EditedEvent::new(
                            743907448000000000,
                            "Second test was edited!".to_string(),
                            vec![BubbleComponent::Run(vec![AttributedRange::text(
                                0,
                                23,
                                vec![TextEffect::Default],
                            )])],
                            None,
                        ),
                    ],
                },
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                },
            ],
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_edited_with_formatting() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/edited_message/EditedWithFormatting.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = EditedMessage::from_map(&plist).unwrap();

        let expected = EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![
                    EditedEvent::new(
                        758573156000000000,
                        "Test".to_string(),
                        vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            4,
                            vec![TextEffect::Default],
                        )])],
                        None,
                    ),
                    EditedEvent::new(
                        758573166000000000,
                        "Test".to_string(),
                        vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            4,
                            vec![TextEffect::Styles(vec![Style::Strikethrough])],
                        )])],
                        Some("76A466B8-D21E-4A20-AF62-FF2D3A20D31C".to_string()),
                    ),
                ],
            }],
        };

        assert_eq!(parsed, expected);

        let expected_item = Some(expected.parts.first().unwrap());
        assert_eq!(parsed.part(0), expected_item);
    }

    #[test]
    fn test_parse_edited_memoji() {
        // The `MemojiEdited` fixture: a message edited from "Check this out: ‹Memoji›"
        // to "Check this out: ‹Memoji› 😀". Both versions must parse the Memoji as
        // an inline (`emoji_image`) attachment range so the exporter can render it
        // as an image in the edit history rather than leaking the `\u{FFFC}` placeholder.
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/edited_message/MemojiEdited.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = EditedMessage::from_map(&plist).unwrap();

        let history = &parsed.parts[0].edit_history;
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].text, "Check this out: \u{FFFC} 😀");

        let BubbleComponent::Run(ranges) = &history[1].components[0] else {
            panic!("expected a Run, got {:?}", history[1].components);
        };
        let memoji = ranges
            .iter()
            .find(|range| range.attachment.is_some())
            .expect("the latest version should contain the Memoji attachment range");
        assert!(
            memoji.emoji_image,
            "the Memoji must be flagged as an inline (emoji_image) sticker"
        );
        assert_eq!(
            memoji.attachment.as_ref().unwrap().guid.as_deref(),
            Some("F2C223DB-0140-4D49-B38A-C1A3553B4CBA"),
        );
    }
}
