use std::borrow::Cow;

use crate::{
    HTML,
    app::export_type::ExportType,
    exporters::{
        formatter::TextEffectFormatter,
        shared::test_support::{config, render, sent},
    },
};

use imessage_database::{
    message_types::text_effects::{
        animation::Animation,
        detected::{
            address::DetectedAddress, currency::DetectedCurrency, flight::Flight,
            shipment_tracking::ShipmentTracking, unit::Unit,
        },
        style::Style,
        text_effect::TextEffect,
    },
    tables::messages::models::{AttributedRange, BubbleComponent},
};

#[test]
fn can_format_html_default() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_effect("Chris", &TextEffect::Default);
    let expected = "Chris";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_mention() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_mention("Chris", "+15558675309");
    let expected = "<span title=\"+15558675309\"><b>Chris</b></span>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_link() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_link("chrissardegna.com", "https://chrissardegna.com");
    let expected = "<a href=\"https://chrissardegna.com\">chrissardegna.com</a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_otp() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_otp("123456");
    let expected = "<u>123456</u>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_style_single() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_styles("Bold", &[Style::Bold]);
    let expected = "<b>Bold</b>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_style_multiple() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_styles("Bold", &[Style::Bold, Style::Strikethrough]);
    let expected = "<s><b>Bold</b></s>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_style_all() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_styles(
        "Bold",
        &[
            Style::Bold,
            Style::Strikethrough,
            Style::Italic,
            Style::Underline,
        ],
    );
    let expected = "<u><i><s><b>Bold</b></s></i></u>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_conversion() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_conversion("100 Miles", &Unit::Distance);
    let expected = "<u>100 Miles</u>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_address() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let address = DetectedAddress {
        full: "1 Apple Park Way, Cupertino, CA 95014".to_string(),
        street: Some("1 Apple Park Way".to_string()),
        street_number: Some("1".to_string()),
        street_name: Some("Apple Park Way".to_string()),
        city: Some("Cupertino".to_string()),
        state: Some("CA".to_string()),
        zip: Some("95014".to_string()),
        country: None,
        country_code: None,
    };
    let actual = exporter.format_address("1 Apple Park Way, Cupertino, CA 95014", &address);
    let expected = "<u>1 Apple Park Way, Cupertino, CA 95014</u>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_currency() {
    // Create exporter
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let currency = DetectedCurrency {
        symbol: "$".to_string(),
        amount: "16".to_string(),
    };
    let actual = exporter.format_currency("$16", &currency);
    let expected = "<u>$16</u>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_tracking() {
    // Create exporter
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let tracking = ShipmentTracking {
        carrier: Some("UPS".to_string()),
        number: "1Z999AA10123456784".to_string(),
    };
    let actual = exporter.format_tracking("1Z999AA10123456784", &tracking);
    let expected = "<u>1Z999AA10123456784</u>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_flight() {
    // Create exporter
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let flight = Flight {
        airline: Some("AS".to_string()),
        number: "1111".to_string(),
    };
    let actual = exporter.format_flight("AS 1111", &flight);
    let expected = "<u>AS 1111</u>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_animated() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_animated("party", &Animation::Big);
    assert_eq!(actual, "<span class=\"animationBig\">party</span>");

    // Unknown(i64) round-trips its integer in the Debug form.
    let actual = exporter.format_animated("oops", &Animation::Unknown(42));
    assert_eq!(actual, "<span class=\"animationUnknown(42)\">oops</span>");
}

#[test]
fn format_effect_default_is_borrowed() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let owned_text = String::from("hello");
    let result = exporter.format_effect(&owned_text, &TextEffect::Default);
    assert!(
        matches!(result, Cow::Borrowed(_)),
        "Default arm must not allocate"
    );

    let owned_url = String::from("https://example.com");
    let link = TextEffect::Link(owned_url);
    let result = exporter.format_effect(&owned_text, &link);
    assert!(
        matches!(result, Cow::Owned(_)),
        "Link arm wraps in <a> and must own"
    );
}

#[test]
fn format_mention_escapes_name_to_prevent_attribute_injection() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_mention("Chris", "\"><script>alert(1)</script>");
    assert_eq!(
        actual,
        "<span title=\"&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;\"><b>Chris</b></span>"
    );
    assert!(
        !actual.contains("<script>"),
        "raw <script> must not survive"
    );
}

#[test]
fn format_link_escapes_url_to_prevent_attribute_injection() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let actual = exporter.format_link("click me", "https://x.test/?q=\"><script>");
    assert_eq!(
        actual,
        "<a href=\"https://x.test/?q=&quot;&gt;&lt;script&gt;\">click me</a>"
    );
    assert!(
        !actual.contains("<script>"),
        "raw <script> must not survive"
    );
}

#[test]
fn can_format_html_mention_end_to_end() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("Test Dad ");

    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 5, vec![TextEffect::Default]),
        AttributedRange::text(5, 8, vec![TextEffect::Mention("+15558675309".to_string())]),
        AttributedRange::text(8, 9, vec![TextEffect::Default]),
    ])];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Test <span title=\"+15558675309\"><b>Dad</b></span> </span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_otp_end_to_end() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("000123 is your security code. Don't share your code.");

    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 6, vec![TextEffect::OTP]),
        AttributedRange::text(6, 52, vec![TextEffect::Default]),
    ])];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><u>000123</u> is your security code. Don&apos;t share your code.</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_link_end_to_end() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("https://twitter.com/xxxxxxxxx/status/0000223300009216128");

    message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
        0,
        56,
        vec![TextEffect::Link(
            "https://twitter.com/xxxxxxxxx/status/0000223300009216128".to_string(),
        )],
    )])];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><a href=\"https://twitter.com/xxxxxxxxx/status/0000223300009216128\">https://twitter.com/xxxxxxxxx/status/0000223300009216128</a></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_conversion_end_to_end() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("Hi. Right now or tomorrow?");

    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 17, vec![TextEffect::Default]),
        AttributedRange::text(17, 25, vec![TextEffect::Conversion(Unit::Timezone)]),
        AttributedRange::text(25, 26, vec![TextEffect::Default]),
    ])];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hi. Right now or <u>tomorrow</u>?</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_text_effect_end_to_end() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("Big small shake nod explode ripple bloom jitter");

    message.components = vec![BubbleComponent::Run(vec![
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
        AttributedRange::text(41, 47, vec![TextEffect::Animated(Animation::Jitter)]),
    ])];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><span class=\"animationBig\">Big</span> <span class=\"animationSmall\">small </span><span class=\"animationShake\">shake</span><span class=\"animationSmall\"> </span><span class=\"animationNod\">nod</span><span class=\"animationSmall\"> </span><span class=\"animationExplode\">explode </span><span class=\"animationRipple\">ripple</span><span class=\"animationExplode\"> </span><span class=\"animationBloom\">bloom</span><span class=\"animationExplode\"> </span><span class=\"animationJitter\">jitter</span></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_text_styles_end_to_end() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("Bold underline italic strikethrough all four");

    message.components = vec![BubbleComponent::Run(vec![
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
                Style::Italic,
            ])],
        ),
    ])];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><b>Bold</b> <u>underline</u> <i>italic</i> <s>strikethrough</s> all <i><u><s><b>four</b></s></u></i></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_text_styles_mixed_end_to_end() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("Underline normal jitter normal");

    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 9, vec![TextEffect::Styles(vec![Style::Underline])]),
        AttributedRange::text(9, 17, vec![TextEffect::Default]),
        AttributedRange::text(17, 23, vec![TextEffect::Animated(Animation::Jitter)]),
        AttributedRange::text(23, 30, vec![TextEffect::Default]),
    ])];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><u>Underline</u> normal <span class=\"animationJitter\">jitter</span> normal</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_text_styled_plain_link() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

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
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><a href=\"https://github.com/ReagentX/imessage-exporter/discussions/553\"><span class=\"animationBig\">https://github.com/ReagentX/imessage-exporter/discussions/553</span></a></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_text_styled_emoji_bold_underline() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("🅱️Bold_Underline");

    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 7, vec![TextEffect::Default]),
        AttributedRange::text(7, 11, vec![TextEffect::Styles(vec![Style::Bold])]),
        AttributedRange::text(11, 12, vec![TextEffect::Default]),
        AttributedRange::text(12, 21, vec![TextEffect::Styles(vec![Style::Underline])]),
    ])];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">🅱\u{fe0f}<b>Bold</b>_<u>Underline</u></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_text_styled_overlapping_ranges() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

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
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><b><u>8</u></b><u>:</u><u><u>00</u></u><u> </u><i><u>pm</u></i></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}
