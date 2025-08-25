//! Routines for working with `typedstream` data, focussing specifically on [`NSAttributedString`](https://developer.apple.com/documentation/foundation/nsattributedstring).

use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

use crabstep::{PropertyIterator, deserializer::iter::Property};

use crate::{
    message_types::{
        edited::{EditStatus, EditedMessage},
        text_effects::{Animation, Style, TextEffect, Unit},
    },
    tables::messages::models::{AttachmentMeta, BubbleComponent, TextAttributes},
    util::typedstream::{
        as_ns_dictionary, as_nsstring, as_nsurl, as_signed_integer, as_type_length_pair,
    },
};

/// `NSDictionary` keys that are used to identify attachment metadata
/// If any of these keys are present in the message body, it is considered an attachment
/// and the `AttachmentMeta` struct will be populated with the relevant data.
static ATTACHMENT_META_KEYS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "__kIMFileTransferGUIDAttributeName",
        "__kIMInlineMediaHeightAttributeName",
        "__kIMFilenameAttributeName",
        "__kIMInlineMediaWidthAttributeName",
        "IMAudioTranscription",
    ])
});
/// Character found in message body text that indicates attachment position
const ATTACHMENT_CHAR: char = '\u{FFFC}';
/// Character found in message body text that indicates app message position
const APP_CHAR: char = '\u{FFFD}';
/// A collection of characters that represent non-text content within body text
const REPLACEMENT_CHARS: [char; 2] = [ATTACHMENT_CHAR, APP_CHAR];

/// Indicates the outcome of parsing an attributed range: either an optional text effect or a style change.
pub enum RangeResult {
    Effect(Option<TextEffect>),
    Style(Style),
}

/// The result of parsing a message body, containing its components and optional plain text.
pub struct ParseResult {
    pub components: Vec<BubbleComponent>,
    pub text: Option<String>,
}

/// Logic to use deserialized `typedstream` data to parse the message body
///
/// Parses `typedstream` components and optional edited parts into message body components and text.
///
/// Takes an optional [`PropertyIterator`] over `typedstream` data and optional edited parts,
/// returning `Some(ParseResult)` when parsing yields components, otherwise `None`.
///
/// # Parameters
///
/// - `components`: Iterator over `typedstream` properties representing an `NSAttributedString`.
/// - `edited_parts`: Optional edited message parts to mark unsent components.
///
/// # Returns
///
/// `Option<ParseResult>` containing parsed components and text, or `None` if no components found.
pub fn parse_body_typedstream<'a>(
    components: Option<PropertyIterator<'a, 'a>>,
    edited_parts: Option<&'a EditedMessage>,
) -> Option<ParseResult> {
    // Create the output data
    let mut message_text = None;
    let mut out_v = Vec::with_capacity(4);

    // Format ranges are only stored once and then referenced by order of appearance (starting at 1),
    // so we need cache them to properly apply styles and attributes.
    // The key is the range ID, and the value is the location of the original formatting data
    // in the `out_v` vector.
    let mut format_range_cache: HashMap<i64, BubbleComponent> = HashMap::with_capacity(4);

    // Start to iterate over the ranges
    let mut current_range_id;
    let mut current_start;
    let mut current_end = 0;

    if let Some(mut components) = components {
        // The first component is the text itself
        if let Some(text) = components.next().as_mut().and_then(as_nsstring) {
            message_text = Some(text.to_string());
            // We want to index into the message text, so we need a table to align
            // Apple's indexes with the actual chars, not the bytes
            let utf16_to_byte: Vec<usize> = build_utf16_to_byte_map(text);

            while let Some(mut property) = components.next() {
                // The first part of the range represents the index in the format cache
                // the second part is the length of the range in UTF-16 code units
                if let Some(range) = as_type_length_pair(&mut property) {
                    current_start = current_end;
                    current_end += range.length as usize;
                    current_range_id = range.type_index;

                    let new_bubble = format_range_cache
                        .get(&current_range_id)
                        .cloned()
                        // Try to reuse an existing bubble
                        .and_then(|mut bubble| {
                            if let BubbleComponent::Text(attrs) = &mut bubble {
                                let start = utf16_idx(text, current_start, &utf16_to_byte);
                                let end = utf16_idx(text, current_end, &utf16_to_byte);
                                for attr in attrs {
                                    attr.start = start;
                                    attr.end = end;
                                }
                                return Some(bubble);
                            }
                            None
                        })
                        // If that failed, try to build a new one
                        .or_else(|| {
                            components
                                .next()
                                .as_mut()
                                .and_then(as_ns_dictionary)
                                .and_then(|dict| {
                                    get_bubble_type(
                                        dict,
                                        Some(text),
                                        current_start,
                                        current_end,
                                        &utf16_to_byte,
                                    )
                                    .inspect(|bubble| {
                                        format_range_cache.insert(current_range_id, bubble.clone());
                                    })
                                })
                        });

                    match (out_v.last_mut(), new_bubble) {
                        (
                            Some(BubbleComponent::Text(attrs)),
                            Some(BubbleComponent::Text(current)),
                        ) => {
                            // last is Text and new bubble is Text: merge
                            attrs.extend(current);
                        }
                        (_, Some(b)) => {
                            // everything else: push the new bubble
                            out_v.push(b);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Add retracted components into the body
    if let Some(edited_message) = &edited_parts {
        for (idx, edited_message_part) in edited_message.parts.iter().enumerate() {
            if matches!(edited_message_part.status, EditStatus::Unsent) {
                if idx >= out_v.len() {
                    out_v.push(BubbleComponent::Retracted);
                } else {
                    out_v.insert(idx, BubbleComponent::Retracted);
                }
            }
        }
    }
    // If we have no components, return None
    (!out_v.is_empty()).then_some(ParseResult {
        components: out_v,
        text: message_text,
    })
}

/// Build a table so that `utf16_to_byte[n]` gives the byte offset
/// that corresponds to the *n*-th UTF-16 code-unit of `s`.
fn build_utf16_to_byte_map(s: &str) -> Vec<usize> {
    let mut map = Vec::with_capacity(s.encode_utf16().count() + 1);
    let mut byte = 0;
    for ch in s.chars() {
        // how many UTF-16 units does this scalar use (1 or 2)
        let units = ch.len_utf16();
        for _ in 0..units {
            map.push(byte);
        }
        byte += ch.len_utf8();
    }
    map.push(byte);
    map
}

/// Given the `attributedBody` range indexes, get the substring indexes from the `UTF-16` representation.
fn utf16_idx(text: &str, idx: usize, map: &[usize]) -> usize {
    *map.get(idx).unwrap_or(&text.len())
}

/// Determines the type of bubble component for a given typedstream range.
///
/// This inspects typedstream properties to classify a range as text, attachment, or app content,
/// returning `Some(BubbleComponent)` when a valid component is detected.
fn get_bubble_type<'a>(
    components: &'a mut PropertyIterator<'a, 'a>,
    text: Option<&str>,
    start: usize,
    end: usize,
    utf16_to_byte: &[usize],
) -> Option<BubbleComponent> {
    // The first item in `components` is the number of key/value pairs in the `NSDictionary`
    let num_objects = components.next().as_ref().and_then(as_signed_integer)?;
    let mut found_effects = Vec::with_capacity(num_objects as usize);
    let mut found_styles = Vec::with_capacity(num_objects as usize);

    // The start and end indexes are based on the `UTF-16` char indexes of the text, so we need to convert them
    let range_start = utf16_idx(text.as_ref()?, start, utf16_to_byte);
    let range_end = utf16_idx(text.as_ref()?, end, utf16_to_byte);

    // Iterate over the key/value pairs in the `NSDictionary` data
    for _ in 0..num_objects {
        let mut key = components.next()?;

        // Convert the key to a string
        let key_name = as_nsstring(&mut key)?;

        // Early exit for attachment components
        if ATTACHMENT_META_KEYS.contains(key_name) {
            return Some(BubbleComponent::Attachment(
                AttachmentMeta::from_components(key_name, components),
            ));
        }

        let mut value = components.next()?;

        // Determine the text effects or styles based on the key name
        let effect = get_text_effects(key_name, &mut value);
        match effect {
            RangeResult::Effect(Some(text_effect)) => found_effects.push(text_effect),
            RangeResult::Style(style) => found_styles.push(style),
            _ => {}
        }
    }

    // If no effects or styles were found, we still need to create a text bubble with the default effect
    let mut attributes = if found_effects.is_empty() && found_styles.is_empty() {
        TextAttributes::new(range_start, range_end, vec![TextEffect::Default])
    } else {
        TextAttributes::new(range_start, range_end, found_effects)
    };

    // If we found any styles, we need to add them to the text attributes
    if !found_styles.is_empty() {
        let format_styles = TextEffect::Styles(found_styles);
        attributes.effects.push(format_styles);
    }

    Some(BubbleComponent::Text(vec![attributes]))
}

/// Collect all text effects from the component attributes
fn get_text_effects<'a>(key_name: &'a str, value: &'a mut Property<'a, 'a>) -> RangeResult {
    match key_name {
        "__kIMMentionConfirmedMention" => {
            if let Some(mention_value) = as_nsstring(value) {
                return RangeResult::Effect(Some(TextEffect::Mention(mention_value.to_string())));
            }
        }
        "__kIMLinkAttributeName" => {
            if let Some(url) = as_nsurl(value) {
                return RangeResult::Effect(Some(TextEffect::Link(url.to_string())));
            }
        }
        "__kIMOneTimeCodeAttributeName" => {
            return RangeResult::Effect(Some(TextEffect::OTP));
        }
        "__kIMCalendarEventAttributeName" => {
            return RangeResult::Effect(Some(TextEffect::Conversion(Unit::Timezone)));
        }
        "__kIMTextEffectAttributeName" => {
            if let Some(effect_id) = as_signed_integer(value) {
                return RangeResult::Effect(Some(TextEffect::Animated(Animation::from_id(
                    effect_id,
                ))));
            }
        }
        // Collect style attributes for later processing
        "__kIMTextBoldAttributeName" => return RangeResult::Style(Style::Bold),
        "__kIMTextUnderlineAttributeName" => return RangeResult::Style(Style::Underline),
        "__kIMTextItalicAttributeName" => return RangeResult::Style(Style::Italic),
        "__kIMTextStrikethroughAttributeName" => {
            return RangeResult::Style(Style::Strikethrough);
        }
        _ => {}
    }

    RangeResult::Effect(None)
}

/// Fallback logic to parse the body from the message string content
pub(crate) fn parse_body_legacy(text: &Option<String>) -> Vec<BubbleComponent> {
    let mut out_v = vec![];
    // Naive logic for when `typedstream` component parsing fails
    match text {
        Some(text) => {
            let mut start: usize = 0;
            let mut end: usize = 0;

            for (idx, char) in text.char_indices() {
                if REPLACEMENT_CHARS.contains(&char) {
                    if start < end {
                        out_v.push(BubbleComponent::Text(vec![TextAttributes::new(
                            start,
                            idx,
                            vec![TextEffect::Default],
                        )]));
                    }
                    start = idx + 1;
                    end = idx;
                    match char {
                        ATTACHMENT_CHAR => {
                            out_v.push(BubbleComponent::Attachment(AttachmentMeta::default()));
                        }
                        APP_CHAR => out_v.push(BubbleComponent::App),
                        _ => {}
                    }
                } else {
                    if start > end {
                        start = idx;
                    }
                    end = idx;
                }
            }
            if start <= end && start < text.len() {
                out_v.push(BubbleComponent::Text(vec![TextAttributes::new(
                    start,
                    text.len(),
                    vec![TextEffect::Default],
                )]));
            }
            out_v
        }
        None => out_v,
    }
}

#[cfg(test)]
mod typedstream_tests {
    // TODO: Fix test structure so we get flat lists again!

    use std::{env::current_dir, fs::File, io::Read};

    use crabstep::TypedStreamDeserializer;

    use crate::{
        message_types::{
            edited::{EditStatus, EditedEvent, EditedMessage, EditedMessagePart},
            text_effects::{Animation, Style, TextEffect, Unit},
        },
        tables::messages::{
            Message,
            body::parse_body_typedstream,
            models::{AttachmentMeta, BubbleComponent, TextAttributes},
        },
    };

    #[test]
    fn can_get_message_body_simple() {
        let mut m = Message::blank();
        m.text = Some("Noter test".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/AttributedBodyTextOnly");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![TextAttributes::new(
                0,
                10,
                vec![TextEffect::Default]
            )])]
        );
    }

    #[test]
    fn can_get_message_body_app() {
        let mut m = Message::blank();
        m.text = Some("\u{FFFC}".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/AppMessage");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Attachment(AttachmentMeta {
                guid: Some("F0B18A15-E9A5-4B18-A38F-685B7B3FF037".to_string()),
                transcription: None,
                height: None,
                width: None,
                name: None
            })]
        );
    }

    #[test]
    fn can_get_message_body_simple_two() {
        let mut m = Message::blank();
        m.text = Some("Test 3".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/AttributedBodyTextOnly2");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![TextAttributes::new(
                0,
                6,
                vec![TextEffect::Default]
            )])]
        );
    }

    #[test]
    fn can_get_message_body_multi_part() {
        let mut m = Message::blank();
        m.text = Some("\u{FFFC}test 1\u{FFFC}test 2 \u{FFFC}test 3".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Multipart");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("at_0_F0668F79-20C2-49C9-A87F-1B007ABB0CED".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                }),
                BubbleComponent::Text(vec![TextAttributes::new(3, 9, vec![TextEffect::Default])]),
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("at_2_F0668F79-20C2-49C9-A87F-1B007ABB0CED".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                }),
                BubbleComponent::Text(vec![TextAttributes::new(12, 19, vec![TextEffect::Default])]),
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("at_4_F0668F79-20C2-49C9-A87F-1B007ABB0CED".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                }),
                BubbleComponent::Text(vec![TextAttributes::new(22, 28, vec![TextEffect::Default])]),
            ]
        );
    }

    #[test]
    fn can_get_message_body_multi_part_deleted() {
        let mut m = Message::blank();
        m.text = Some(
            "From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r".to_string(),
        );
        m.edited_parts = Some(EditedMessage {
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
        });

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/MultiPartWithDeleted");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![
                BubbleComponent::Text(vec![TextAttributes::new(0, 28, vec![TextEffect::Default])]),
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("D0551D89-4E11-43D0-9A0E-06F19704E97B".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                }),
                BubbleComponent::Text(vec![TextAttributes::new(31, 63, vec![TextEffect::Default])]),
                BubbleComponent::Retracted
            ]
        );
    }

    #[test]
    fn can_get_message_body_multi_part_deleted_edited() {
        let mut m = Message::blank();
        m.text = Some(
            "From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r".to_string(),
        );

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/MultiPartWithDeleted");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        m.edited_parts = Some(EditedMessage {
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
                            Some("Second test".to_string()),
                            vec![],
                            None,
                        ),
                        EditedEvent::new(
                            743907448000000000,
                            Some("Second test was edited!".to_string()),
                            vec![],
                            None,
                        ),
                    ],
                },
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                },
            ],
        });

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![
                BubbleComponent::Text(vec![TextAttributes::new(0, 28, vec![TextEffect::Default])]),
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("D0551D89-4E11-43D0-9A0E-06F19704E97B".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                }),
                BubbleComponent::Text(vec![TextAttributes::new(31, 63, vec![TextEffect::Default])]),
                BubbleComponent::Retracted,
            ]
        );
    }

    #[test]
    fn can_get_message_body_fully_unsent() {
        let mut m = Message::blank();
        m.text = Some(
            "From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r".to_string(),
        );

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Blank");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        m.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
            }],
        });

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(parsed.components, vec![BubbleComponent::Retracted,]);
    }

    #[test]
    fn can_get_message_body_edited_with_formatting() {
        let mut m = Message::blank();
        m.text = Some(
            "From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r".to_string(),
        );

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/EditedWithFormatting");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        m.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![
                    EditedEvent {
                        date: 758573156000000000,
                        text: Some("Test".to_string()),
                        components: vec![BubbleComponent::Text(vec![TextAttributes {
                            start: 0,
                            end: 4,
                            effects: vec![TextEffect::Default],
                        }])],
                        guid: None,
                    },
                    EditedEvent {
                        date: 758573166000000000,
                        text: Some("Test".to_string()),
                        components: vec![BubbleComponent::Text(vec![TextAttributes {
                            start: 0,
                            end: 4,
                            effects: vec![TextEffect::Styles(vec![Style::Strikethrough])],
                        }])],
                        guid: Some("76A466B8-D21E-4A20-AF62-FF2D3A20D31C".to_string()),
                    },
                ],
            }],
        });

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![TextAttributes::new(
                0,
                4,
                vec![TextEffect::Styles(vec![Style::Strikethrough])]
            )]),]
        );
    }

    #[test]
    fn can_get_message_body_attachment() {
        let mut m = Message::blank();
        m.text = Some(
            "\u{FFFC}This is how the notes look to me fyi, in case it helps make sense of anything"
                .to_string(),
        );

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Attachment");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("at_0_2E5F12C3-E649-48AA-954D-3EA67C016BCC".to_string()),
                    transcription: None,
                    height: Some(1139.0),
                    width: Some(952.0),
                    name: Some("Messages Image(785748029).png".to_string())
                }),
                BubbleComponent::Text(vec![TextAttributes::new(3, 80, vec![TextEffect::Default])]),
            ]
        );
    }

    #[test]
    fn can_get_message_body_attachment_i16() {
        let mut m = Message::blank();
        m.text = Some("\u{FFFC}".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/AttachmentI16");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Attachment(AttachmentMeta {
                guid: Some("at_0_BE588799-C4BC-47DF-A56D-7EE90C74911D".to_string()),
                transcription: None,
                height: None,
                width: None,
                name: Some("brilliant-kids-test-answers-32-93042.jpeg".to_string())
            })]
        );
    }

    #[test]
    fn can_get_message_body_url() {
        let mut m = Message::blank();
        m.text = Some("https://twitter.com/xxxxxxxxx/status/0000223300009216128".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/URLMessage");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![TextAttributes::new(
                0,
                56,
                vec![TextEffect::Link(
                    "https://twitter.com/xxxxxxxxx/status/0000223300009216128".to_string()
                )]
            )]),]
        );
    }

    #[test]
    fn can_get_message_body_mention() {
        let mut m = Message::blank();
        m.text = Some("Test Dad ".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Mention");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(0, 5, vec![TextEffect::Default]),
                TextAttributes::new(5, 8, vec![TextEffect::Mention("+15558675309".to_string())]),
                TextAttributes::new(8, 9, vec![TextEffect::Default])
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_code() {
        let mut m = Message::blank();
        m.text = Some("000123 is your security code. Don't share your code.".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Code");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(0, 6, vec![TextEffect::OTP]),
                TextAttributes::new(6, 52, vec![TextEffect::Default])
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_phone() {
        let mut m = Message::blank();
        m.text = Some("What about 0000000000".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/PhoneNumber");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(0, 11, vec![TextEffect::Default]),
                TextAttributes::new(11, 21, vec![TextEffect::Link("tel:0000000000".to_string())])
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_email() {
        let mut m = Message::blank();
        m.text = Some("asdfghjklq@gmail.com might work".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Email");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(
                    0,
                    20,
                    vec![TextEffect::Link("mailto:asdfghjklq@gmail.com".to_string())],
                ),
                TextAttributes::new(20, 31, vec![TextEffect::Default])
            ])]
        );
    }

    #[test]
    fn can_get_message_body_date() {
        let mut m = Message::blank();
        m.text = Some("Hi. Right now or tomorrow?".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Date");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(0, 17, vec![TextEffect::Default]),
                TextAttributes::new(17, 25, vec![TextEffect::Conversion(Unit::Timezone)]),
                TextAttributes::new(25, 26, vec![TextEffect::Default])
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_custom_tapback() {
        let mut m = Message::blank();
        m.text = Some(String::new());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/CustomReaction");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![
                BubbleComponent::Text(vec![TextAttributes::new(0, 79, vec![TextEffect::Default])]),
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("41C4376E-397E-4C42-84E2-B16F7801F638".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                })
            ]
        );
    }

    #[test]
    fn can_get_message_body_deleted_only() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
            }],
        });

        assert_eq!(
            parse_body_typedstream(None, m.edited_parts.as_ref())
                .unwrap()
                .components,
            vec![BubbleComponent::Retracted]
        );
    }

    #[test]
    fn can_get_message_body_text_styles() {
        let mut m = Message::blank();
        m.text = Some("Bold underline italic strikethrough all four".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/TextStyles");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(0, 4, vec![TextEffect::Styles(vec![Style::Bold])]),
                TextAttributes::new(4, 5, vec![TextEffect::Default]),
                TextAttributes::new(5, 14, vec![TextEffect::Styles(vec![Style::Underline])]),
                TextAttributes::new(14, 15, vec![TextEffect::Default]),
                TextAttributes::new(15, 21, vec![TextEffect::Styles(vec![Style::Italic])]),
                TextAttributes::new(21, 22, vec![TextEffect::Default]),
                TextAttributes::new(22, 35, vec![TextEffect::Styles(vec![Style::Strikethrough])]),
                TextAttributes::new(35, 40, vec![TextEffect::Default]),
                TextAttributes::new(
                    40,
                    44,
                    vec![TextEffect::Styles(vec![
                        Style::Bold,
                        Style::Strikethrough,
                        Style::Underline,
                        Style::Italic
                    ])]
                )
            ]),],
        );
    }

    #[test]
    fn can_get_message_body_text_effects() {
        let mut m = Message::blank();
        m.text = Some("Big small shake nod explode ripple bloom jitter".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/TextEffects");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(0, 3, vec![TextEffect::Animated(Animation::Big)]),
                TextAttributes::new(3, 4, vec![TextEffect::Default]),
                TextAttributes::new(4, 10, vec![TextEffect::Animated(Animation::Small)]),
                TextAttributes::new(10, 15, vec![TextEffect::Animated(Animation::Shake)]),
                TextAttributes::new(15, 16, vec![TextEffect::Animated(Animation::Small)]),
                TextAttributes::new(16, 19, vec![TextEffect::Animated(Animation::Nod)]),
                TextAttributes::new(19, 20, vec![TextEffect::Animated(Animation::Small)]),
                TextAttributes::new(20, 28, vec![TextEffect::Animated(Animation::Explode)]),
                TextAttributes::new(28, 34, vec![TextEffect::Animated(Animation::Ripple)]),
                TextAttributes::new(34, 35, vec![TextEffect::Animated(Animation::Explode)]),
                TextAttributes::new(35, 40, vec![TextEffect::Animated(Animation::Bloom)]),
                TextAttributes::new(40, 41, vec![TextEffect::Animated(Animation::Explode)]),
                TextAttributes::new(41, 47, vec![TextEffect::Animated(Animation::Jitter)])
            ]),],
        );
    }

    #[test]
    fn can_get_message_body_text_effects_styles_mixed() {
        let mut m = Message::blank();
        m.text = Some("Underline normal jitter normal".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/TextStylesMixed");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(0, 9, vec![TextEffect::Styles(vec![Style::Underline])]),
                TextAttributes::new(9, 17, vec![TextEffect::Default]),
                TextAttributes::new(17, 23, vec![TextEffect::Animated(Animation::Jitter)]),
                TextAttributes::new(23, 30, vec![TextEffect::Default])
            ]),],
        );
    }

    #[test]
    fn can_get_message_body_text_effects_styles_single_range() {
        let mut m = Message::blank();
        m.text = Some("Everything".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/TextStylesSingleRange");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![TextAttributes::new(
                0,
                10,
                vec![TextEffect::Styles(vec![
                    Style::Bold,
                    Style::Strikethrough,
                    Style::Underline,
                    Style::Italic
                ])]
            )])],
        );
    }

    #[test]
    fn can_get_message_body_audio_message() {
        let mut m = Message::blank();
        m.text = Some("\u{FFFC}".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/Transcription");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Attachment(AttachmentMeta {
                guid: Some("4C339597-EBBB-4978-9B87-521C0471A848".to_string()),
                transcription: Some("This is a test".to_string()),
                height: None,
                width: None,
                name: None
            }),]
        );
    }

    #[test]
    fn can_get_message_body_apple_music_lyrics() {
        let mut m = Message::blank();
        m.text = Some("\u{FFFC}".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/AppleMusicLyrics");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![TextAttributes::new(
                0,
                145,
                vec![TextEffect::Link(
                    "https://music.apple.com/us/lyrics/1329891623?ts=11.108&te=16.031&l=en&tk=2.v1.VsuX9f%2BaT1PyrgMgIT7ANQ%3D%3D&itsct=sharing_msg_lyrics&itscg=50401".to_string()
                )]
            )],)]
        );
    }

    #[test]
    fn can_get_message_body_multiple_attachment() {
        let mut m = Message::blank();
        m.text = Some("\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}\u{FFFC}".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/MultiAttachment");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        assert_eq!(
            parse_body_typedstream(Some(iter), m.edited_parts.as_ref())
                .unwrap()
                .components[..5],
            vec![
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("at_0_48B9C973-3466-438C-BE72-E5B498D30772".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                }),
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("at_1_48B9C973-3466-438C-BE72-E5B498D30772".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                }),
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("at_2_48B9C973-3466-438C-BE72-E5B498D30772".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                }),
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("at_3_48B9C973-3466-438C-BE72-E5B498D30772".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                }),
                BubbleComponent::Attachment(AttachmentMeta {
                    guid: Some("at_4_48B9C973-3466-438C-BE72-E5B498D30772".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                })
            ]
        );
    }

    #[test]
    fn can_get_message_body_text_styled_plain_link() {
        let mut m = Message::blank();
        m.text = Some("https://github.com/ReagentX/imessage-exporter/discussions/553".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/StyledLink");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![TextAttributes::new(
                0,
                61,
                vec![
                    TextEffect::Animated(Animation::Big),
                    TextEffect::Link(
                        "https://github.com/ReagentX/imessage-exporter/discussions/553".to_string()
                    )
                ]
            )]),]
        );
    }

    #[test]
    fn can_get_message_body_text_emoji() {
        let mut m = Message::blank();
        m.text = Some("🅱️Bold_Underline".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/EmojiBoldUnderline");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(0, 7, vec![TextEffect::Default]),
                TextAttributes::new(7, 11, vec![TextEffect::Styles(vec![Style::Bold])]),
                TextAttributes::new(11, 12, vec![TextEffect::Default]),
                TextAttributes::new(12, 21, vec![TextEffect::Styles(vec![Style::Underline])])
            ]),],
        );
    }

    #[test]
    fn can_get_message_body_text_overlapping_format_ranges() {
        let mut m = Message::blank();
        m.text = Some("8:00 pm".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/OverlappingFormat");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(
                    0,
                    1,
                    vec![
                        TextEffect::Conversion(Unit::Timezone),
                        TextEffect::Styles(vec![Style::Bold])
                    ]
                ),
                TextAttributes::new(1, 2, vec![TextEffect::Conversion(Unit::Timezone)]),
                TextAttributes::new(
                    2,
                    4,
                    vec![
                        TextEffect::Conversion(Unit::Timezone),
                        TextEffect::Styles(vec![Style::Underline])
                    ]
                ),
                TextAttributes::new(4, 5, vec![TextEffect::Conversion(Unit::Timezone)]),
                TextAttributes::new(
                    5,
                    7,
                    vec![
                        TextEffect::Conversion(Unit::Timezone),
                        TextEffect::Styles(vec![Style::Italic])
                    ]
                )
            ],),]
        );
    }

    #[test]
    fn can_get_message_body_text_overlapping_format_ranges_short() {
        let mut m = Message::blank();
        m.text = Some("8:00 pm".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/0123456789");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(
                    0,
                    6,
                    vec![
                        TextEffect::Link("tel:0123456789".to_string()),
                        TextEffect::Styles(vec![Style::Strikethrough])
                    ]
                ),
                TextAttributes::new(
                    6,
                    10,
                    vec![
                        TextEffect::Link("tel:0123456789".to_string()),
                        TextEffect::Styles(vec![Style::Italic])
                    ]
                )
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_text_overlapping_format_ranges_long() {
        let mut m = Message::blank();
        m.text = Some("8:00 pm".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/35123");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes {
                    start: 0,
                    end: 5,
                    effects: vec![
                        TextEffect::Link("tel:0123456789".to_string()),
                        TextEffect::Styles(vec![Style::Underline])
                    ]
                },
                TextAttributes {
                    start: 5,
                    end: 7,
                    effects: vec![
                        TextEffect::Link("tel:0123456789".to_string()),
                        TextEffect::Styles(vec![Style::Underline, Style::Strikethrough])
                    ]
                },
                TextAttributes {
                    start: 7,
                    end: 10,
                    effects: vec![
                        TextEffect::Link("tel:0123456789".to_string()),
                        TextEffect::Styles(vec![Style::Strikethrough])
                    ]
                },
                TextAttributes {
                    start: 10,
                    end: 16,
                    effects: vec![TextEffect::Styles(vec![Style::Bold])]
                },
                TextAttributes {
                    start: 16,
                    end: 24,
                    effects: vec![TextEffect::Styles(vec![Style::Bold, Style::Italic])]
                },
                TextAttributes {
                    start: 24,
                    end: 34,
                    effects: vec![TextEffect::Styles(vec![
                        Style::Bold,
                        Style::Underline,
                        Style::Italic
                    ])]
                },
                TextAttributes {
                    start: 34,
                    end: 47,
                    effects: vec![TextEffect::Styles(vec![
                        Style::Italic,
                        Style::Bold,
                        Style::Strikethrough,
                        Style::Underline
                    ])]
                },
                TextAttributes {
                    start: 47,
                    end: 51,
                    effects: vec![TextEffect::Default]
                },
                TextAttributes {
                    start: 51,
                    end: 55,
                    effects: vec![TextEffect::Animated(Animation::Big)]
                },
                TextAttributes {
                    start: 55,
                    end: 60,
                    effects: vec![TextEffect::Animated(Animation::Small)]
                },
                TextAttributes {
                    start: 60,
                    end: 61,
                    effects: vec![TextEffect::Default]
                },
                TextAttributes {
                    start: 61,
                    end: 66,
                    effects: vec![TextEffect::Animated(Animation::Shake)]
                },
                TextAttributes {
                    start: 66,
                    end: 67,
                    effects: vec![TextEffect::Default]
                },
                TextAttributes {
                    start: 67,
                    end: 70,
                    effects: vec![TextEffect::Animated(Animation::Nod)]
                },
                TextAttributes {
                    start: 70,
                    end: 71,
                    effects: vec![TextEffect::Default]
                },
                TextAttributes {
                    start: 71,
                    end: 78,
                    effects: vec![TextEffect::Animated(Animation::Explode)]
                },
                TextAttributes {
                    start: 78,
                    end: 79,
                    effects: vec![TextEffect::Default]
                },
                TextAttributes {
                    start: 79,
                    end: 85,
                    effects: vec![TextEffect::Animated(Animation::Ripple)]
                },
                TextAttributes {
                    start: 85,
                    end: 86,
                    effects: vec![TextEffect::Default]
                },
                TextAttributes {
                    start: 86,
                    end: 91,
                    effects: vec![TextEffect::Animated(Animation::Bloom)]
                },
                TextAttributes {
                    start: 91,
                    end: 92,
                    effects: vec![TextEffect::Default]
                },
                TextAttributes {
                    start: 92,
                    end: 98,
                    effects: vec![TextEffect::Animated(Animation::Jitter)]
                }
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_text_single_link() {
        let mut m = Message::blank();
        m.text = Some("8:00 pm".to_string());

        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/typedstream/SingleLink");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();

        let parsed = parse_body_typedstream(Some(iter), m.edited_parts.as_ref()).unwrap();
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Text(vec![
                TextAttributes::new(
                    0,
                    84,
                    vec![
                        TextEffect::Link("https://www.ghacks.net/2020/01/23/lastpass-no-longer-listed-on-the-chrome-web-store/".to_string()),
                    ]
                ),
            ]),]
        );
    }
}

#[cfg(test)]
mod legacy_tests {
    use crate::{
        message_types::text_effects::TextEffect,
        tables::messages::{
            Message,
            body::parse_body_legacy,
            models::{AttachmentMeta, BubbleComponent, TextAttributes},
        },
    };

    #[test]
    fn can_get_message_body_single_emoji() {
        let mut m = Message::blank();
        m.text = Some("🙈".to_string());
        assert_eq!(
            parse_body_legacy(&m.text),
            vec![BubbleComponent::Text(vec![TextAttributes::new(
                0,
                4,
                vec![TextEffect::Default]
            )],)]
        );
    }

    #[test]
    fn can_get_message_body_multiple_emoji() {
        let mut m = Message::blank();
        m.text = Some("🙈🙈🙈".to_string());
        assert_eq!(
            parse_body_legacy(&m.text),
            vec![BubbleComponent::Text(vec![TextAttributes::new(
                0,
                12,
                vec![TextEffect::Default]
            )],)]
        );
    }

    #[test]
    fn can_get_message_body_text_only() {
        let mut m = Message::blank();
        m.text = Some("Hello world".to_string());
        assert_eq!(
            parse_body_legacy(&m.text),
            vec![BubbleComponent::Text(vec![TextAttributes::new(
                0,
                11,
                vec![TextEffect::Default]
            )],)]
        );
    }

    #[test]
    fn can_get_message_body_attachment_text() {
        let mut m = Message::blank();
        m.text = Some("\u{FFFC}Hello world".to_string());
        assert_eq!(
            parse_body_legacy(&m.text),
            vec![
                BubbleComponent::Attachment(AttachmentMeta::default()),
                BubbleComponent::Text(vec![TextAttributes::new(3, 14, vec![TextEffect::Default])])
            ]
        );
    }

    #[test]
    fn can_get_message_body_app_text() {
        let mut m = Message::blank();
        m.text = Some("\u{FFFD}Hello world".to_string());
        assert_eq!(
            parse_body_legacy(&m.text),
            vec![
                BubbleComponent::App,
                BubbleComponent::Text(vec![TextAttributes::new(3, 14, vec![TextEffect::Default])])
            ]
        );
    }

    #[test]
    fn can_get_message_body_app_attachment_text_mixed_start_text() {
        let mut m = Message::blank();
        m.text = Some("One\u{FFFD}\u{FFFC}Two\u{FFFC}Three\u{FFFC}four".to_string());
        assert_eq!(
            parse_body_legacy(&m.text),
            vec![
                BubbleComponent::Text(vec![TextAttributes::new(0, 3, vec![TextEffect::Default])]),
                BubbleComponent::App,
                BubbleComponent::Attachment(AttachmentMeta::default()),
                BubbleComponent::Text(vec![TextAttributes::new(9, 12, vec![TextEffect::Default])]),
                BubbleComponent::Attachment(AttachmentMeta::default()),
                BubbleComponent::Text(vec![TextAttributes::new(15, 20, vec![TextEffect::Default])]),
                BubbleComponent::Attachment(AttachmentMeta::default()),
                BubbleComponent::Text(vec![TextAttributes::new(23, 27, vec![TextEffect::Default])]),
            ]
        );
    }

    #[test]
    fn can_get_message_body_app_attachment_text_mixed_start_app() {
        let mut m = Message::blank();
        m.text = Some("\u{FFFD}\u{FFFC}Two\u{FFFC}Three\u{FFFC}".to_string());
        assert_eq!(
            parse_body_legacy(&m.text),
            vec![
                BubbleComponent::App,
                BubbleComponent::Attachment(AttachmentMeta::default()),
                BubbleComponent::Text(vec![TextAttributes::new(6, 9, vec![TextEffect::Default])]),
                BubbleComponent::Attachment(AttachmentMeta::default()),
                BubbleComponent::Text(vec![TextAttributes::new(12, 17, vec![TextEffect::Default])]),
                BubbleComponent::Attachment(AttachmentMeta::default()),
            ]
        );
    }
}
