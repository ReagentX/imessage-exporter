/*
 Routines for working with `typedstream` data, focusing on [`NSAttributedString`](https://developer.apple.com/documentation/foundation/nsattributedstring) payloads from `message.attributedBody`.
*/

use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

use crabstep::{
    PropertyIterator,
    deserializer::{foundation::FoundationDict, iter::Property},
};

use crate::{
    message_types::{
        edited::{EditStatus, EditedMessage},
        text_effects::{
            animation::Animation,
            detected::{
                address::DetectedAddress, currency::DetectedCurrency, flight::Flight,
                shipment_tracking::ShipmentTracking, unit::Unit,
            },
            style::Style,
            text_effect::TextEffect,
        },
    },
    tables::messages::models::{AttachmentMeta, AttributedRange, BubbleComponent},
    util::data_detected::FromScannerResult,
    util::typedstream::as_type_length_pair,
};

// MARK: Constants
/// `NSDictionary` keys that identify attachment metadata on a body range.
static ATTACHMENT_META_KEYS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "__kIMFileTransferGUIDAttributeName",
        "__kIMInlineMediaHeightAttributeName",
        "__kIMFilenameAttributeName",
        "__kIMInlineMediaWidthAttributeName",
        "IMAudioTranscription",
    ])
});
/// Character that marks an attachment position in body text.
const ATTACHMENT_CHAR: char = '\u{FFFC}';
/// Character that marks an app-message position in body text.
const APP_CHAR: char = '\u{FFFD}';
/// Non-text replacement characters in body text.
const REPLACEMENT_CHARS: [char; 2] = [ATTACHMENT_CHAR, APP_CHAR];

/// Attribute parser output for one key/value pair on an attributed range.
#[derive(Debug, PartialEq)]
pub enum RangeResult {
    /// A complete text effect, or `None` when the key was not handled.
    Effect(Option<TextEffect>),
    /// A style marker that will be merged into [`TextEffect::Styles`].
    Style(Style),
}

/// Parsed attributed body content.
#[derive(Debug, PartialEq)]
pub struct ParseResult {
    /// Bubble components reconstructed from attributed ranges.
    pub components: Vec<BubbleComponent>,
    /// Plain message text, when present in the typedstream payload.
    pub text: Option<String>,
}

// MARK: Logic
/// Parse a deserialized `typedstream` attributed body.
///
/// The parser reads the [`NSAttributedString`](https://developer.apple.com/documentation/foundation/nsattributedstring) text, converts UTF-16 range
/// offsets to byte offsets, builds [`AttributedRange`]s for text effects,
/// styles, and inline attachments, then groups ranges into
/// [`BubbleComponent::Run`] values by message part. Unsent edit parts are
/// inserted as retracted components.
///
/// Returns `None` only when no text was read and no components were produced.
pub fn parse_body_typedstream<'a>(
    components: Option<PropertyIterator<'a, 'a>>,
    edited_parts: Option<&'a EditedMessage>,
) -> Option<ParseResult> {
    let mut message_text = None;

    // Flat list of attributed ranges paired with their
    // `__kIMMessagePartAttributeName` index, in stream order. Grouped into
    // `Run`s by part after the walk.
    let mut ranges: Vec<(AttributedRange, i64)> = Vec::with_capacity(4);

    // Format ranges are only stored once and then referenced by order of
    // appearance, so we cache them to reapply styles and attributes. The key is
    // the range ID; the value is the cached range plus its part index. A cache
    // hit means an identical attribute dictionary, hence an identical part
    // index, so reusing the cached part is sound.
    let mut format_range_cache: HashMap<i64, (AttributedRange, i64)> = HashMap::with_capacity(4);

    let mut current_range_id;
    let mut current_start;
    let mut current_end: usize = 0;

    // The first component is the text itself
    if let Some(mut components) = components
        && let Some(text) = components.next().as_ref().and_then(Property::as_string)
    {
        message_text = Some(text.to_string());

        // We want to index into the message text, so we need a table to align
        // Apple's indexes with the actual chars, not the bytes
        let utf16_to_byte: Vec<usize> = build_utf16_to_byte_map(text);

        while let Some(property) = components.next() {
            // The first part of the range represents the index in the format cache
            // the second part is the length of the range in UTF-16 code units
            if let Some(range) = as_type_length_pair(&property) {
                current_start = current_end;
                current_end = current_end.saturating_add(range.length as usize);
                current_range_id = range.type_index;

                let built = format_range_cache
                    .get(&current_range_id)
                    .cloned()
                    // Only text ranges are reusable; attachment ranges carry
                    // occurrence-specific metadata such as file-transfer GUIDs.
                    .and_then(|(mut cached, part)| {
                        if cached.attachment.is_none() {
                            cached.start = utf16_idx(text, current_start, &utf16_to_byte);
                            cached.end = utf16_idx(text, current_end, &utf16_to_byte);
                            return Some((cached, part));
                        }
                        None
                    })
                    .or_else(|| {
                        components
                            .next()
                            .as_ref()
                            .and_then(Property::as_dictionary)
                            .and_then(|dict| {
                                build_range(dict, text, current_start, current_end, &utf16_to_byte)
                                    .inspect(|built| {
                                        format_range_cache.insert(current_range_id, built.clone());
                                    })
                            })
                    });

                if let Some(built) = built {
                    ranges.push(built);
                }
            }
        }
    }

    // Group consecutive ranges that share a `__kIMMessagePartAttributeName`
    // index into a single `Run` (one message bubble). Parts are emitted in
    // contiguous, ascending order, so a `Run`'s position matches its part
    // index, which tapback/reply lookups and retracted-part insertion both
    // rely on. A negative part is the sentinel for a missing index; those never
    // coalesce.
    let mut out_v: Vec<BubbleComponent> = Vec::with_capacity(4);
    let mut current_part: Option<i64> = None;
    for (range, part) in ranges {
        match out_v.last_mut() {
            Some(BubbleComponent::Run(run)) if current_part == Some(part) && part >= 0 => {
                run.push(range);
            }
            _ => {
                out_v.push(BubbleComponent::Run(vec![range]));
                current_part = Some(part);
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
    // Return None only when nothing useful was extracted. `Some("")` is a
    // valid result.
    (!out_v.is_empty() || message_text.is_some()).then_some(ParseResult {
        components: out_v,
        text: message_text,
    })
}

/// Build a table so that `utf16_to_byte[n]` gives the byte offset
/// that corresponds to the *n*-th UTF-16 code-unit of `s`.
fn build_utf16_to_byte_map(s: &str) -> Vec<usize> {
    let mut map = Vec::with_capacity(s.len() + 1);
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

/// Builds a single [`AttributedRange`] from one typedstream range's
/// `NSDictionary`, walking *every* key so attachment metadata, text effects,
/// styles, and the inline-emoji hint are all captured on the same range.
///
/// Returns the range together with its `__kIMMessagePartAttributeName` index
/// (or `-1` when the attribute is absent), which the caller uses to group
/// ranges into bubbles.
fn build_range<'a>(
    dict: FoundationDict<'a, 'a>,
    text: &str,
    start: usize,
    end: usize,
    utf16_to_byte: &[usize],
) -> Option<(AttributedRange, i64)> {
    // The start and end indexes are based on the `UTF-16` char indexes of the text, so we need to convert them
    let range_start = utf16_idx(text, start, utf16_to_byte);
    let range_end = utf16_idx(text, end, utf16_to_byte);

    let mut effects = Vec::with_capacity(dict.len());
    let mut styles = Vec::new();
    let mut attachment: Option<AttachmentMeta> = None;
    let mut emoji_image = false;
    // `-1` sentinel: this range carries no part attribute.
    let mut message_part: i64 = -1;

    // Iterate over the key/value pairs in the `NSDictionary`; `as_dictionary`
    // already skipped the leading count group and yields complete pairs.
    for (key, value) in dict {
        // Keys are strings; skip a malformed non-string key rather than
        // discarding the rest of the range.
        let Some(key_name) = key.as_string() else {
            continue;
        };

        // Attachment-meta keys populate this range's `AttachmentMeta` in place.
        // We intentionally do not early-exit: sibling keys on the same range
        // (text effects, the emoji-image hint, the part index) are still read.
        if ATTACHMENT_META_KEYS.contains(key_name) {
            attachment
                .get_or_insert_with(AttachmentMeta::default)
                .set_from_key_value(key_name, &value);
            continue;
        }

        match key_name {
            "__kIMMessagePartAttributeName" => {
                if let Some(part) = value.as_i64() {
                    message_part = part;
                }
            }
            // Apple's inline-rendering hint; value `1` means "render inline".
            "__kIMEmojiImageAttributeName" => {
                emoji_image = value.as_i64() == Some(1);
            }
            _ => match get_text_effects(key_name, &value) {
                RangeResult::Effect(Some(text_effect)) => effects.push(text_effect),
                RangeResult::Style(style) => styles.push(style),
                _ => {}
            },
        }
    }

    // A text range with no effects still gets the explicit `Default` marker.
    // Attachment ranges keep an empty effects vec unless a real effect applied.
    if attachment.is_none() && effects.is_empty() && styles.is_empty() {
        effects.push(TextEffect::Default);
    }

    // Styles are represented as a single `TextEffect::Styles` entry.
    if !styles.is_empty() {
        effects.push(TextEffect::Styles(styles));
    }

    Some((
        AttributedRange {
            start: range_start,
            end: range_end,
            effects,
            attachment,
            emoji_image,
        },
        message_part,
    ))
}

/// Collect all text effects from the component attributes
fn get_text_effects<'a>(key_name: &'a str, value: &Property<'a, 'a>) -> RangeResult {
    match key_name {
        "__kIMMentionConfirmedMention" => {
            if let Some(mention_value) = value.as_string() {
                return RangeResult::Effect(Some(TextEffect::Mention(mention_value.to_string())));
            }
        }
        "__kIMLinkAttributeName" => {
            if let Some(url) = value.as_url() {
                return RangeResult::Effect(Some(TextEffect::Link(url.to_string())));
            }
        }
        "__kIMOneTimeCodeAttributeName" => {
            return RangeResult::Effect(Some(TextEffect::OTP));
        }
        "__kIMCalendarEventAttributeName" => {
            return RangeResult::Effect(Some(TextEffect::Conversion(Unit::Timezone)));
        }
        "__kIMDataDetectedAttributeName" => {
            // This attribute carries unrelated detector payloads. Per-type
            // markers let negative probes return before plist deserialization.
            return RangeResult::Effect(
                Unit::from_attribute(value)
                    .map(TextEffect::Conversion)
                    .or_else(|| ShipmentTracking::from_attribute(value).map(TextEffect::Tracking))
                    .or_else(|| Flight::from_attribute(value).map(TextEffect::Flight)),
            );
        }
        "__kIMMoneyAttributeName" => {
            return RangeResult::Effect(
                DetectedCurrency::from_attribute(value).map(TextEffect::Currency),
            );
        }
        "__kIMAddressAttributeName" => {
            return RangeResult::Effect(
                DetectedAddress::from_attribute(value)
                    .map(Box::new)
                    .map(TextEffect::Address),
            );
        }
        "__kIMTextEffectAttributeName" => {
            if let Some(effect_id) = value.as_i64() {
                return RangeResult::Effect(Some(TextEffect::Animated(Animation::from_id(
                    effect_id,
                ))));
            }
        }
        // Styles are collected and merged into one `TextEffect::Styles` value.
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

// MARK: Fallback
/// Parse body components from the plain message string.
pub(crate) fn parse_body_legacy(text: &Option<String>) -> Vec<BubbleComponent> {
    let mut out_v = vec![];
    // There are no part indexes in the fallback string path, so each text
    // segment and each attachment becomes its own single-range `Run`.
    match text {
        Some(text) => {
            let mut start: usize = 0;
            let mut end: usize = 0;

            for (idx, char) in text.char_indices() {
                if REPLACEMENT_CHARS.contains(&char) {
                    if start < end {
                        out_v.push(BubbleComponent::Run(vec![AttributedRange::text(
                            start,
                            idx,
                            vec![TextEffect::Default],
                        )]));
                    }
                    start = idx + 1;
                    end = idx;
                    match char {
                        ATTACHMENT_CHAR => {
                            out_v.push(BubbleComponent::Run(vec![AttributedRange::attachment(
                                idx,
                                idx + char.len_utf8(),
                                AttachmentMeta::default(),
                            )]));
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
                out_v.push(BubbleComponent::Run(vec![AttributedRange::text(
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

// MARK: TS Test
#[cfg(test)]
mod typedstream_tests {
    use std::{env::current_dir, fs::File, io::Read};

    use crabstep::TypedStreamDeserializer;

    use crate::{
        message_types::{
            edited::{EditStatus, EditedEvent, EditedMessage, EditedMessagePart},
            text_effects::{
                animation::Animation,
                detected::{
                    address::DetectedAddress, currency::DetectedCurrency, flight::Flight,
                    shipment_tracking::ShipmentTracking, unit::Unit,
                },
                style::Style,
                text_effect::TextEffect,
            },
        },
        tables::messages::{
            Message,
            body::{ParseResult, parse_body_typedstream},
            models::{AttachmentMeta, AttributedRange, BubbleComponent},
        },
    };

    /// Parse a `test_data/typedstream/` body fixture against `edited_parts`.
    fn parse_typedstream_fixture(name: &str, edited_parts: Option<&EditedMessage>) -> ParseResult {
        let typedstream_path = current_dir()
            .unwrap()
            .as_path()
            .join(format!("test_data/typedstream/{name}"));
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let mut parser = TypedStreamDeserializer::new(&bytes);
        let iter = parser.iter_root().unwrap();
        parse_body_typedstream(Some(iter), edited_parts).unwrap()
    }

    /// Build an [`AttachmentMeta`] carrying only a file-transfer GUID.
    fn meta(guid: &str) -> AttachmentMeta {
        AttachmentMeta {
            guid: Some(guid.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn can_get_message_body_memoji_only() {
        // A sticker-only message: a static Memoji with the `emoji_image` hint
        // parses as a single inline attachment.
        let ParseResult { text, components } = parse_typedstream_fixture("MemojiOnly", None);
        assert_eq!(text.as_deref(), Some("\u{FFFC}"));
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::inline_attachment(
                    0,
                    3,
                    meta("34D71074-FBCF-4E4A-BB53-54CE92660C22"),
                )
            ])]
        );
    }

    #[test]
    fn can_get_message_body_data_detected_conversions() {
        let ParseResult { text, components } =
            parse_typedstream_fixture("CurrencyTemperatureVolumeWeight", None);
        assert_eq!(
            text.as_deref(),
            Some("$100\n\n75℉\n\n1L of water\n\n225lbs")
        );
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 6, vec![TextEffect::Default]),
                AttributedRange::text(6, 11, vec![TextEffect::Conversion(Unit::Temperature)]),
                AttributedRange::text(11, 13, vec![TextEffect::Default]),
                AttributedRange::text(13, 15, vec![TextEffect::Conversion(Unit::Volume)]),
                AttributedRange::text(15, 26, vec![TextEffect::Default]),
                AttributedRange::text(26, 32, vec![TextEffect::Conversion(Unit::Weight)]),
            ])]
        );
    }

    #[test]
    fn can_get_message_body_detected_address() {
        let ParseResult { text, components } = parse_typedstream_fixture("Address", None);
        assert_eq!(
            text.as_deref(),
            Some("1 Apple Park Way, Cupertino, CA 95014")
        );
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![AttributedRange::text(
                0,
                37,
                vec![TextEffect::Address(Box::new(DetectedAddress {
                    full: "1 Apple Park Way, Cupertino, CA 95014".to_string(),
                    street: Some("1 Apple Park Way".to_string()),
                    street_number: Some("1".to_string()),
                    street_name: Some("Apple Park Way".to_string()),
                    city: Some("Cupertino".to_string()),
                    state: Some("CA".to_string()),
                    zip: Some("95014".to_string()),
                    country: None,
                    country_code: None,
                }))]
            )])]
        );
    }

    #[test]
    fn can_get_message_body_detected_currency() {
        let ParseResult { text, components } = parse_typedstream_fixture("Currency", None);
        assert_eq!(text.as_deref(), Some("My burrito was $16"));
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 15, vec![TextEffect::Default]),
                AttributedRange::text(
                    15,
                    18,
                    vec![TextEffect::Currency(DetectedCurrency {
                        symbol: "$".to_string(),
                        amount: "16".to_string(),
                    })]
                ),
            ])]
        );
    }

    #[test]
    fn can_get_message_body_detected_currency_money_amount() {
        let ParseResult { text, components } =
            parse_typedstream_fixture("CurrencyMoneyAmount", None);
        assert_eq!(text.as_deref(), Some("$15/mo"));
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(
                    0,
                    3,
                    vec![TextEffect::Currency(DetectedCurrency {
                        symbol: "$".to_string(),
                        amount: "15".to_string(),
                    })]
                ),
                AttributedRange::text(3, 6, vec![TextEffect::Default]),
            ])]
        );
    }

    #[test]
    fn can_get_message_body_detected_tracking() {
        let ParseResult { text, components } = parse_typedstream_fixture("Tracking", None);
        assert_eq!(text.as_deref(), Some("1Z999AA10123456784"));
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![AttributedRange::text(
                0,
                18,
                vec![TextEffect::Tracking(ShipmentTracking {
                    carrier: Some("UPS".to_string()),
                    number: "1Z999AA10123456784".to_string(),
                })]
            )])]
        );
    }

    #[test]
    fn can_get_message_body_detected_flight() {
        let ParseResult { text, components } = parse_typedstream_fixture("Flight", None);
        assert_eq!(text.as_deref(), Some("AS 1111"));
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![AttributedRange::text(
                0,
                7,
                vec![TextEffect::Flight(Flight {
                    airline: Some("AS".to_string()),
                    number: "1111".to_string(),
                })]
            )])]
        );
    }

    #[test]
    fn can_get_message_body_all_unit_conversions() {
        let ParseResult { text, components } = parse_typedstream_fixture("AllUnits", None);
        assert_eq!(
            text.as_deref(),
            Some(
                "40° angle\n120 sqft\n100 USD\n12 miles\n1 gallon\n2:00\n25 watts\n1000hp\n65 mph\n25 mpg\n12 bar\n70℉\n12 PDT\n225 lbs"
            )
        );
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(
                    0,
                    4,
                    vec![TextEffect::Conversion(Unit::Unknown(
                        "celsius-fahrenheit-degree".to_string()
                    ))]
                ), // "40°"
                AttributedRange::text(4, 11, vec![TextEffect::Default]), // " angle\n"
                AttributedRange::text(11, 19, vec![TextEffect::Conversion(Unit::Area)]), // "120 sqft"
                AttributedRange::text(19, 28, vec![TextEffect::Default]), // "\n100 USD\n"
                AttributedRange::text(28, 36, vec![TextEffect::Conversion(Unit::Distance)]), // "12 miles"
                AttributedRange::text(36, 37, vec![TextEffect::Default]),                    // "\n"
                AttributedRange::text(37, 45, vec![TextEffect::Conversion(Unit::Volume)]), // "1 gallon"
                AttributedRange::text(45, 46, vec![TextEffect::Default]),                  // "\n"
                AttributedRange::text(46, 50, vec![TextEffect::Conversion(Unit::Timezone)]), // "2:00"
                AttributedRange::text(50, 51, vec![TextEffect::Default]),                    // "\n"
                AttributedRange::text(51, 59, vec![TextEffect::Conversion(Unit::Power)]), // "25 watts"
                AttributedRange::text(59, 67, vec![TextEffect::Default]), // "\n1000hp\n"
                AttributedRange::text(67, 73, vec![TextEffect::Conversion(Unit::Speed)]), // "65 mph"
                AttributedRange::text(73, 74, vec![TextEffect::Default]),                 // "\n"
                AttributedRange::text(74, 80, vec![TextEffect::Conversion(Unit::FuelEfficiency)]), // "25 mpg"
                AttributedRange::text(80, 81, vec![TextEffect::Default]), // "\n"
                AttributedRange::text(81, 87, vec![TextEffect::Conversion(Unit::Pressure)]), // "12 bar"
                AttributedRange::text(87, 88, vec![TextEffect::Default]),                    // "\n"
                AttributedRange::text(88, 93, vec![TextEffect::Conversion(Unit::Temperature)]), // "70℉"
                AttributedRange::text(93, 101, vec![TextEffect::Default]), // "\n12 PDT\n"
                AttributedRange::text(101, 108, vec![TextEffect::Conversion(Unit::Weight)]), // "225 lbs"
            ])]
        );
    }

    #[test]
    fn can_get_message_body_memoji_with_text() {
        // Leading text followed by an inline Memoji at the end of the run.
        let ParseResult { text, components } = parse_typedstream_fixture("MemojiWithText", None);
        assert_eq!(text.as_deref(), Some("Memoji: \u{FFFC}"));
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 8, vec![TextEffect::Default]),
                AttributedRange::inline_attachment(
                    8,
                    11,
                    meta("31C086FD-E884-405D-8E0E-4BDA5D5E4E4A"),
                ),
            ])]
        );
    }

    #[test]
    fn can_get_message_body_animated_sticker_only() {
        // The negative case: an animated balloon sticker has *no* `emoji_image`
        // hint, so it parses as a block attachment (`emoji_image == false`), which is the
        // distinction the inline-vs-block classification hinges on.
        let ParseResult { text, components } =
            parse_typedstream_fixture("AnimatedStickerOnly", None);
        assert_eq!(text.as_deref(), Some("\u{FFFC}"));
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![AttributedRange::attachment(
                0,
                3,
                meta("92EA5F8F-059B-44E2-BE62-80CA706013F6"),
            )])]
        );
    }

    #[test]
    fn can_get_message_body_static_animoji_regular_emoji() {
        // "Check this out: ￼ 😀" bold text, an inline Memoji, an animated
        // space, and a trailing emoji, all in one run.
        let ParseResult { text, components } =
            parse_typedstream_fixture("StaticAnimojiRegularEmoji", None);
        assert_eq!(text.as_deref(), Some("Check this out: \u{FFFC} 😀"));
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 6, vec![TextEffect::Default]),
                AttributedRange::text(6, 10, vec![TextEffect::Styles(vec![Style::Bold])]),
                AttributedRange::text(10, 16, vec![TextEffect::Default]),
                AttributedRange::inline_attachment(
                    16,
                    19,
                    meta("F2C223DB-0140-4D49-B38A-C1A3553B4CBA"),
                ),
                AttributedRange::text(19, 20, vec![TextEffect::Animated(Animation::Jitter)]),
                AttributedRange::text(20, 24, vec![TextEffect::Default]),
            ])]
        );
    }

    #[test]
    fn can_get_message_body_multiple_types_inline() {
        // Two text segments each followed by three inline stickers; every
        // placeholder keeps its own GUID in body order (guards the ordering fix
        // at the parse layer).
        let ParseResult { text, components } =
            parse_typedstream_fixture("MultipleTypesInline", None);
        assert_eq!(
            text.as_deref(),
            Some("Some Genmoji: \u{FFFC}\u{FFFC}\u{FFFC}\nSome stickers: \u{FFFC}\u{FFFC}\u{FFFC}")
        );
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 14, vec![TextEffect::Default]),
                AttributedRange::inline_attachment(
                    14,
                    17,
                    meta("E8F4C3B6-1D70-429F-8DD2-4821A447F644")
                ),
                AttributedRange::inline_attachment(
                    17,
                    20,
                    meta("4B3FF592-4FB5-4B69-92F1-B3A545AFA834")
                ),
                AttributedRange::inline_attachment(
                    20,
                    23,
                    meta("1ABC2FCE-7BD7-45D3-90A3-5672555BEA91")
                ),
                AttributedRange::text(23, 39, vec![TextEffect::Default]),
                AttributedRange::inline_attachment(
                    39,
                    42,
                    meta("D832964D-33C5-4131-B8F8-9FF9F6CF454F")
                ),
                AttributedRange::inline_attachment(
                    42,
                    45,
                    meta("CE30DA18-B86D-4475-AF5E-46EC60C8E5E4")
                ),
                AttributedRange::inline_attachment(
                    45,
                    48,
                    meta("2E95ABBC-13F7-486B-8E8C-93AAD885E80A")
                ),
            ])]
        );
    }

    #[test]
    fn can_get_message_body_translated_with_inline_stickers_and_emoji() {
        // The *original* body of a translated message: text, two inline stickers
        // (a Memoji + a genmoji), and a trailing regular emoji. The body keeps its
        // stickers regardless of translation; the translation side (which can't
        // carry them) is covered by `translation::tests`.
        let ParseResult { text, components } =
            parse_typedstream_fixture("TranslatedWithInlineStickersAndEmoji", None);
        assert_eq!(text.as_deref(), Some("This is a test \u{FFFC}\u{FFFC}🫪"));
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 15, vec![TextEffect::Default]),
                AttributedRange::inline_attachment(
                    15,
                    18,
                    meta("D58F8CD8-CC3F-40C8-B8A3-7426E3DD6F27")
                ),
                AttributedRange::inline_attachment(
                    18,
                    21,
                    meta("15733840-BA54-4273-8B3B-0C1439C5CABD")
                ),
                AttributedRange::text(21, 25, vec![TextEffect::Default]),
            ])]
        );
    }

    #[test]
    fn can_get_message_body_formatted_inline_stickers() {
        // Inline stickers interleaved with formatted emoji ("￼🫪￼🙏"): the two
        // sticker ranges are inline, the emoji ranges carry their text effects.
        let ParseResult { text, components } =
            parse_typedstream_fixture("FormattedInlineStickers", None);
        assert_eq!(text.as_deref(), Some("\u{FFFC}🫪\u{FFFC}🙏"));
        assert_eq!(
            components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::inline_attachment(
                    0,
                    3,
                    meta("1E50CD07-D8F7-4CF1-A662-B23262B9D492")
                ),
                AttributedRange::text(3, 7, vec![TextEffect::Animated(Animation::Big)]),
                AttributedRange::inline_attachment(
                    7,
                    10,
                    meta("49769DA1-FF04-4381-8982-B4810F0EB971")
                ),
                AttributedRange::text(10, 14, vec![TextEffect::Default]),
            ])]
        );
    }

    #[test]
    fn can_get_message_body_simple() {
        let parsed = parse_typedstream_fixture("AttributedBodyTextOnly", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![AttributedRange::text(
                0,
                10,
                vec![TextEffect::Default]
            )])]
        );
    }

    #[test]
    fn can_get_message_body_app() {
        let parsed = parse_typedstream_fixture("AppMessage", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![AttributedRange::attachment(
                0,
                3,
                AttachmentMeta {
                    guid: Some("F0B18A15-E9A5-4B18-A38F-685B7B3FF037".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None
                }
            )])]
        );
    }

    #[test]
    fn can_get_message_body_simple_two() {
        let parsed = parse_typedstream_fixture("AttributedBodyTextOnly2", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![AttributedRange::text(
                0,
                6,
                vec![TextEffect::Default]
            )])]
        );
    }

    #[test]
    fn can_get_message_body_multi_part() {
        let parsed = parse_typedstream_fixture("MultiPart", None);
        assert_eq!(
            parsed.components,
            vec![
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    0,
                    3,
                    AttachmentMeta {
                        guid: Some("at_0_F0668F79-20C2-49C9-A87F-1B007ABB0CED".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(3, 9, vec![TextEffect::Default])]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    9,
                    12,
                    AttachmentMeta {
                        guid: Some("at_2_F0668F79-20C2-49C9-A87F-1B007ABB0CED".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(
                    12,
                    19,
                    vec![TextEffect::Default]
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    19,
                    22,
                    AttachmentMeta {
                        guid: Some("at_4_F0668F79-20C2-49C9-A87F-1B007ABB0CED".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(
                    22,
                    28,
                    vec![TextEffect::Default]
                )]),
            ]
        );
    }

    #[test]
    fn can_get_message_body_multi_part_deleted() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                    unsend_failed: false,
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                    unsend_failed: false,
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                    unsend_failed: false,
                },
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                    unsend_failed: false,
                },
            ],
            unsupported_handles: vec![],
        });

        let parsed = parse_typedstream_fixture("MultiPartWithDeleted", m.edited_parts.as_ref());
        assert_eq!(
            parsed.components,
            vec![
                BubbleComponent::Run(vec![AttributedRange::text(
                    0,
                    28,
                    vec![TextEffect::Default]
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    28,
                    31,
                    AttachmentMeta {
                        guid: Some("D0551D89-4E11-43D0-9A0E-06F19704E97B".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(
                    31,
                    63,
                    vec![TextEffect::Default]
                )]),
                BubbleComponent::Retracted
            ]
        );
    }

    #[test]
    fn can_get_message_body_multi_part_deleted_edited() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                    unsend_failed: false,
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                    unsend_failed: false,
                },
                EditedMessagePart {
                    status: EditStatus::Edited,
                    edit_history: vec![
                        EditedEvent::new(
                            743907435000000000,
                            "Second test".to_string(),
                            vec![],
                            None,
                        ),
                        EditedEvent::new(
                            743907448000000000,
                            "Second test was edited!".to_string(),
                            vec![],
                            None,
                        ),
                    ],
                    unsend_failed: false,
                },
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                    unsend_failed: false,
                },
            ],
            unsupported_handles: vec![],
        });

        let parsed = parse_typedstream_fixture("MultiPartWithDeleted", m.edited_parts.as_ref());
        assert_eq!(
            parsed.components,
            vec![
                BubbleComponent::Run(vec![AttributedRange::text(
                    0,
                    28,
                    vec![TextEffect::Default]
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    28,
                    31,
                    AttachmentMeta {
                        guid: Some("D0551D89-4E11-43D0-9A0E-06F19704E97B".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(
                    31,
                    63,
                    vec![TextEffect::Default]
                )]),
                BubbleComponent::Retracted,
            ]
        );
    }

    #[test]
    fn can_get_message_body_fully_unsent() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
                unsend_failed: false,
            }],
            unsupported_handles: vec![],
        });

        let parsed = parse_typedstream_fixture("Blank", m.edited_parts.as_ref());
        assert_eq!(parsed.components, vec![BubbleComponent::Retracted,]);
    }

    #[test]
    fn can_get_message_body_edited_with_formatting() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![
                    EditedEvent {
                        date: 758573156000000000,
                        text: "Test".to_string(),
                        components: vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            4,
                            vec![TextEffect::Default],
                        )])],
                        guid: None,
                    },
                    EditedEvent {
                        date: 758573166000000000,
                        text: "Test".to_string(),
                        components: vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            4,
                            vec![TextEffect::Styles(vec![Style::Strikethrough])],
                        )])],
                        guid: Some("76A466B8-D21E-4A20-AF62-FF2D3A20D31C".to_string()),
                    },
                ],
                unsend_failed: false,
            }],
            unsupported_handles: vec![],
        });

        let parsed = parse_typedstream_fixture("EditedWithFormatting", m.edited_parts.as_ref());
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![AttributedRange::text(
                0,
                4,
                vec![TextEffect::Styles(vec![Style::Strikethrough])]
            )]),]
        );
    }

    #[test]
    fn can_get_message_body_attachment() {
        let parsed = parse_typedstream_fixture("Attachment", None);
        assert_eq!(
            parsed.components,
            vec![
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    0,
                    3,
                    AttachmentMeta {
                        guid: Some("at_0_2E5F12C3-E649-48AA-954D-3EA67C016BCC".to_string()),
                        transcription: None,
                        height: Some(1139.0),
                        width: Some(952.0),
                        name: Some("Messages Image(785748029).png".to_string())
                    }
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(
                    3,
                    80,
                    vec![TextEffect::Default]
                )]),
            ]
        );
    }

    #[test]
    fn can_get_message_body_attachment_i16() {
        let parsed = parse_typedstream_fixture("AttachmentI16", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![AttributedRange::attachment(
                0,
                3,
                AttachmentMeta {
                    guid: Some("at_0_BE588799-C4BC-47DF-A56D-7EE90C74911D".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: Some("brilliant-kids-test-answers-32-93042.jpeg".to_string())
                }
            )])]
        );
    }

    #[test]
    fn can_get_message_body_url() {
        let parsed = parse_typedstream_fixture("URLMessage", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![AttributedRange::text(
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
        let parsed = parse_typedstream_fixture("Mention", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 5, vec![TextEffect::Default]),
                AttributedRange::text(5, 8, vec![TextEffect::Mention("+15558675309".to_string())]),
                AttributedRange::text(8, 9, vec![TextEffect::Default])
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_code() {
        let parsed = parse_typedstream_fixture("Code", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 6, vec![TextEffect::OTP]),
                AttributedRange::text(6, 52, vec![TextEffect::Default])
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_phone() {
        let parsed = parse_typedstream_fixture("PhoneNumber", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 11, vec![TextEffect::Default]),
                AttributedRange::text(11, 21, vec![TextEffect::Link("tel:0000000000".to_string())])
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_email() {
        let parsed = parse_typedstream_fixture("Email", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(
                    0,
                    20,
                    vec![TextEffect::Link("mailto:asdfghjklq@gmail.com".to_string())],
                ),
                AttributedRange::text(20, 31, vec![TextEffect::Default])
            ])]
        );
    }

    #[test]
    fn can_get_message_body_date() {
        let parsed = parse_typedstream_fixture("Date", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 17, vec![TextEffect::Default]),
                AttributedRange::text(17, 25, vec![TextEffect::Conversion(Unit::Timezone)]),
                AttributedRange::text(25, 26, vec![TextEffect::Default])
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_custom_tapback() {
        let parsed = parse_typedstream_fixture("CustomReaction", None);
        assert_eq!(
            parsed.components,
            vec![
                BubbleComponent::Run(vec![AttributedRange::text(
                    0,
                    79,
                    vec![TextEffect::Default]
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    79,
                    82,
                    AttachmentMeta {
                        guid: Some("41C4376E-397E-4C42-84E2-B16F7801F638".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )])
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
                unsend_failed: false,
            }],
            unsupported_handles: vec![],
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
        let parsed = parse_typedstream_fixture("TextStyles", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 4, vec![TextEffect::Styles(vec![Style::Bold])]),
                AttributedRange::text(4, 5, vec![TextEffect::Default]),
                AttributedRange::text(5, 14, vec![TextEffect::Styles(vec![Style::Underline])]),
                AttributedRange::text(14, 15, vec![TextEffect::Default]),
                AttributedRange::text(15, 21, vec![TextEffect::Styles(vec![Style::Italic])]),
                AttributedRange::text(21, 22, vec![TextEffect::Default]),
                AttributedRange::text(22, 35, vec![TextEffect::Styles(vec![Style::Strikethrough])]),
                AttributedRange::text(35, 40, vec![TextEffect::Default]),
                AttributedRange::text(
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
        let parsed = parse_typedstream_fixture("TextEffects", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 3, vec![TextEffect::Animated(Animation::Big)]),
                AttributedRange::text(3, 4, vec![TextEffect::Default]),
                AttributedRange::text(4, 10, vec![TextEffect::Animated(Animation::Small)]),
                AttributedRange::text(10, 15, vec![TextEffect::Animated(Animation::Shake)]),
                AttributedRange::text(15, 16, vec![TextEffect::Animated(Animation::Small)]),
                AttributedRange::text(16, 19, vec![TextEffect::Animated(Animation::Nod)]),
                AttributedRange::text(19, 20, vec![TextEffect::Animated(Animation::Small)]),
                AttributedRange::text(20, 28, vec![TextEffect::Animated(Animation::Explode)]),
                AttributedRange::text(28, 34, vec![TextEffect::Animated(Animation::Ripple)]),
                AttributedRange::text(34, 35, vec![TextEffect::Animated(Animation::Explode)]),
                AttributedRange::text(35, 40, vec![TextEffect::Animated(Animation::Bloom)]),
                AttributedRange::text(40, 41, vec![TextEffect::Animated(Animation::Explode)]),
                AttributedRange::text(41, 47, vec![TextEffect::Animated(Animation::Jitter)])
            ]),],
        );
    }

    #[test]
    fn can_get_message_body_text_effects_styles_mixed() {
        let parsed = parse_typedstream_fixture("TextStylesMixed", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 9, vec![TextEffect::Styles(vec![Style::Underline])]),
                AttributedRange::text(9, 17, vec![TextEffect::Default]),
                AttributedRange::text(17, 23, vec![TextEffect::Animated(Animation::Jitter)]),
                AttributedRange::text(23, 30, vec![TextEffect::Default])
            ]),],
        );
    }

    #[test]
    fn can_get_message_body_text_effects_styles_single_range() {
        let parsed = parse_typedstream_fixture("TextStylesSingleRange", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![AttributedRange::text(
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
        let parsed = parse_typedstream_fixture("Transcription", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![AttributedRange::attachment(
                0,
                3,
                AttachmentMeta {
                    guid: Some("4C339597-EBBB-4978-9B87-521C0471A848".to_string()),
                    transcription: Some("This is a test".to_string()),
                    height: None,
                    width: None,
                    name: None
                }
            )]),]
        );
    }

    #[test]
    fn can_get_message_body_apple_music_lyrics() {
        let parsed = parse_typedstream_fixture("AppleMusicLyrics", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![AttributedRange::text(
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
        assert_eq!(
            parse_typedstream_fixture("MultiAttachment", None).components[..5],
            vec![
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    0,
                    3,
                    AttachmentMeta {
                        guid: Some("at_0_48B9C973-3466-438C-BE72-E5B498D30772".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    3,
                    6,
                    AttachmentMeta {
                        guid: Some("at_1_48B9C973-3466-438C-BE72-E5B498D30772".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    6,
                    9,
                    AttachmentMeta {
                        guid: Some("at_2_48B9C973-3466-438C-BE72-E5B498D30772".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    9,
                    12,
                    AttachmentMeta {
                        guid: Some("at_3_48B9C973-3466-438C-BE72-E5B498D30772".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    12,
                    15,
                    AttachmentMeta {
                        guid: Some("at_4_48B9C973-3466-438C-BE72-E5B498D30772".to_string()),
                        transcription: None,
                        height: None,
                        width: None,
                        name: None
                    }
                )])
            ]
        );
    }

    #[test]
    fn can_get_message_body_text_styled_plain_link() {
        let parsed = parse_typedstream_fixture("StyledLink", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![AttributedRange::text(
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
        let parsed = parse_typedstream_fixture("EmojiBoldUnderline", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(0, 7, vec![TextEffect::Default]),
                AttributedRange::text(7, 11, vec![TextEffect::Styles(vec![Style::Bold])]),
                AttributedRange::text(11, 12, vec![TextEffect::Default]),
                AttributedRange::text(12, 21, vec![TextEffect::Styles(vec![Style::Underline])])
            ]),],
        );
    }

    #[test]
    fn can_get_message_body_text_overlapping_format_ranges() {
        let parsed = parse_typedstream_fixture("OverlappingFormat", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(
                    0,
                    1,
                    vec![
                        TextEffect::Conversion(Unit::Timezone),
                        TextEffect::Styles(vec![Style::Bold])
                    ]
                ),
                AttributedRange::text(1, 2, vec![TextEffect::Conversion(Unit::Timezone)]),
                AttributedRange::text(
                    2,
                    4,
                    vec![
                        TextEffect::Conversion(Unit::Timezone),
                        TextEffect::Styles(vec![Style::Underline])
                    ]
                ),
                AttributedRange::text(4, 5, vec![TextEffect::Conversion(Unit::Timezone)]),
                AttributedRange::text(
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
        let parsed = parse_typedstream_fixture("0123456789", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(
                    0,
                    6,
                    vec![
                        TextEffect::Link("tel:0123456789".to_string()),
                        TextEffect::Styles(vec![Style::Strikethrough])
                    ]
                ),
                AttributedRange::text(
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
        let parsed = parse_typedstream_fixture("35123", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(
                    0,
                    5,
                    vec![
                        TextEffect::Link("tel:0123456789".to_string()),
                        TextEffect::Styles(vec![Style::Underline])
                    ]
                ),
                AttributedRange::text(
                    5,
                    7,
                    vec![
                        TextEffect::Link("tel:0123456789".to_string()),
                        TextEffect::Styles(vec![Style::Underline, Style::Strikethrough])
                    ]
                ),
                AttributedRange::text(
                    7,
                    10,
                    vec![
                        TextEffect::Link("tel:0123456789".to_string()),
                        TextEffect::Styles(vec![Style::Strikethrough])
                    ]
                ),
                AttributedRange::text(10, 16, vec![TextEffect::Styles(vec![Style::Bold])]),
                AttributedRange::text(
                    16,
                    24,
                    vec![TextEffect::Styles(vec![Style::Bold, Style::Italic])]
                ),
                AttributedRange::text(
                    24,
                    34,
                    vec![TextEffect::Styles(vec![
                        Style::Bold,
                        Style::Underline,
                        Style::Italic
                    ])]
                ),
                AttributedRange::text(
                    34,
                    47,
                    vec![TextEffect::Styles(vec![
                        Style::Italic,
                        Style::Bold,
                        Style::Strikethrough,
                        Style::Underline
                    ])]
                ),
                AttributedRange::text(47, 51, vec![TextEffect::Default]),
                AttributedRange::text(51, 55, vec![TextEffect::Animated(Animation::Big)]),
                AttributedRange::text(55, 60, vec![TextEffect::Animated(Animation::Small)]),
                AttributedRange::text(60, 61, vec![TextEffect::Default]),
                AttributedRange::text(61, 66, vec![TextEffect::Animated(Animation::Shake)]),
                AttributedRange::text(66, 67, vec![TextEffect::Default]),
                AttributedRange::text(67, 70, vec![TextEffect::Animated(Animation::Nod)]),
                AttributedRange::text(70, 71, vec![TextEffect::Default]),
                AttributedRange::text(71, 78, vec![TextEffect::Animated(Animation::Explode)]),
                AttributedRange::text(78, 79, vec![TextEffect::Default]),
                AttributedRange::text(79, 85, vec![TextEffect::Animated(Animation::Ripple)]),
                AttributedRange::text(85, 86, vec![TextEffect::Default]),
                AttributedRange::text(86, 91, vec![TextEffect::Animated(Animation::Bloom)]),
                AttributedRange::text(91, 92, vec![TextEffect::Default]),
                AttributedRange::text(92, 98, vec![TextEffect::Animated(Animation::Jitter)])
            ]),]
        );
    }

    #[test]
    fn can_get_message_body_text_single_link() {
        let parsed = parse_typedstream_fixture("SingleLink", None);
        assert_eq!(
            parsed.components,
            vec![BubbleComponent::Run(vec![
                AttributedRange::text(
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

// MARK: Legacy Test
#[cfg(test)]
mod legacy_tests {
    use crate::{
        message_types::text_effects::text_effect::TextEffect,
        tables::messages::{
            Message,
            body::parse_body_legacy,
            models::{AttachmentMeta, AttributedRange, BubbleComponent},
        },
    };

    #[test]
    fn can_get_message_body_single_emoji() {
        let mut m = Message::blank();
        m.text = Some("🙈".to_string());
        assert_eq!(
            parse_body_legacy(&m.text),
            vec![BubbleComponent::Run(vec![AttributedRange::text(
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
            vec![BubbleComponent::Run(vec![AttributedRange::text(
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
            vec![BubbleComponent::Run(vec![AttributedRange::text(
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
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    0,
                    3,
                    AttachmentMeta::default()
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(
                    3,
                    14,
                    vec![TextEffect::Default]
                )])
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
                BubbleComponent::Run(vec![AttributedRange::text(
                    3,
                    14,
                    vec![TextEffect::Default]
                )])
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
                BubbleComponent::Run(vec![AttributedRange::text(0, 3, vec![TextEffect::Default])]),
                BubbleComponent::App,
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    6,
                    9,
                    AttachmentMeta::default()
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(
                    9,
                    12,
                    vec![TextEffect::Default]
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    12,
                    15,
                    AttachmentMeta::default()
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(
                    15,
                    20,
                    vec![TextEffect::Default]
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    20,
                    23,
                    AttachmentMeta::default()
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(
                    23,
                    27,
                    vec![TextEffect::Default]
                )]),
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
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    3,
                    6,
                    AttachmentMeta::default()
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(6, 9, vec![TextEffect::Default])]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    9,
                    12,
                    AttachmentMeta::default()
                )]),
                BubbleComponent::Run(vec![AttributedRange::text(
                    12,
                    17,
                    vec![TextEffect::Default]
                )]),
                BubbleComponent::Run(vec![AttributedRange::attachment(
                    17,
                    20,
                    AttachmentMeta::default()
                )]),
            ]
        );
    }
}
