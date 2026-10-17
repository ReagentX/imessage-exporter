use crate::{
    TXT,
    app::export_type::ExportType,
    exporters::shared::test_support::{config, render, sent},
};

use imessage_database::{
    message_types::text_effects::{
        animation::Animation,
        detected::{
            currency::DetectedCurrency, flight::Flight, shipment_tracking::ShipmentTracking,
            unit::Unit,
        },
        style::Style,
        text_effect::TextEffect,
    },
    tables::messages::models::{AttributedRange, BubbleComponent},
};

#[test]
fn can_format_txt_text_styles_mixed_end_to_end() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = sent("Underline normal jitter normal");

    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 9, vec![TextEffect::Styles(vec![Style::Underline])]),
        AttributedRange::text(9, 17, vec![TextEffect::Default]),
        AttributedRange::text(17, 23, vec![TextEffect::Animated(Animation::Jitter)]),
        AttributedRange::text(23, 30, vec![TextEffect::Default]),
    ])];

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\nUnderline normal jitter normal\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_text_styled_plain_link() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = sent("https://github.com/ReagentX/imessage-exporter/discussions/553");

    message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
        0,
        61,
        vec![
            TextEffect::Animated(Animation::Big),
            TextEffect::Link(
                "https://github.com/ReagentX/imessage-exporter/discussions/553".to_string(),
            ),
        ],
    )])];

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\nhttps://github.com/ReagentX/imessage-exporter/discussions/553\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_text_styled_emoji_bold_underline() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = sent("🅱️Bold_Underline");

    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 7, vec![TextEffect::Default]),
        AttributedRange::text(7, 11, vec![TextEffect::Styles(vec![Style::Bold])]),
        AttributedRange::text(11, 12, vec![TextEffect::Default]),
        AttributedRange::text(12, 21, vec![TextEffect::Styles(vec![Style::Underline])]),
    ])];

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\n🅱️Bold_Underline\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_text_styled_overlapping_ranges() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = sent("8:00 pm");

    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(
            0,
            1,
            vec![
                TextEffect::Conversion(Unit::Timezone),
                TextEffect::Styles(vec![Style::Bold]),
            ],
        ),
        AttributedRange::text(1, 2, vec![TextEffect::Conversion(Unit::Timezone)]),
        AttributedRange::text(
            2,
            4,
            vec![
                TextEffect::Conversion(Unit::Timezone),
                TextEffect::Styles(vec![Style::Underline]),
            ],
        ),
        AttributedRange::text(4, 5, vec![TextEffect::Conversion(Unit::Timezone)]),
        AttributedRange::text(
            5,
            7,
            vec![
                TextEffect::Conversion(Unit::Timezone),
                TextEffect::Styles(vec![Style::Italic]),
            ],
        ),
    ])];

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\n8:00 pm\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_detected_effects_are_unformatted() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = sent("$16 1Z999AA10123456784 AS 1111");

    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(
            0,
            3,
            vec![TextEffect::Currency(DetectedCurrency {
                symbol: "$".to_string(),
                amount: "16".to_string(),
            })],
        ),
        AttributedRange::text(3, 4, vec![TextEffect::Default]),
        AttributedRange::text(
            4,
            22,
            vec![TextEffect::Tracking(ShipmentTracking {
                carrier: Some("UPS".to_string()),
                number: "1Z999AA10123456784".to_string(),
            })],
        ),
        AttributedRange::text(22, 23, vec![TextEffect::Default]),
        AttributedRange::text(
            23,
            30,
            vec![TextEffect::Flight(Flight {
                airline: Some("AS".to_string()),
                number: "1111".to_string(),
            })],
        ),
    ])];

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\n$16 1Z999AA10123456784 AS 1111\n\n";

    assert_eq!(actual, expected);
}
