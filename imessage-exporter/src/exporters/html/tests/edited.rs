use crate::{
    Config, HTML,
    app::export_type::ExportType,
    exporters::{
        formatter::MessageFormatter,
        shared::test_support::{SAMPLE_DATE, SAMPLE_DATE_LATER, config, render, sent},
    },
};

use imessage_database::{
    message_types::{
        edited::{EditStatus, EditedEvent, EditedMessage, EditedMessagePart},
        text_effects::{style::Style, text_effect::TextEffect},
    },
    tables::messages::models::{AttachmentMeta, AttributedRange, BubbleComponent},
};

#[test]
fn can_format_html_edited_with_formatting() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let edited_message = EditedMessage {
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
    };

    let mut message = sent("Test");
    message.date_edited = SAMPLE_DATE_LATER;
    message.edited_parts = Some(edited_message);

    message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
        0,
        4,
        vec![TextEffect::Styles(vec![Style::Strikethrough])],
    )])];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <div class=\"edited\"><table><tbody>\n        <tr>\n            <td><span class=\"timestamp\"></span></td>\n            <td>Test</td>\n        </tr>\n    </tbody><tfoot>\n        <tr>\n            <td><span class=\"timestamp\">Edited 10 seconds later</span></td>\n            <td><s>Test</s></td>\n        </tr>\n    </tfoot></table></div>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_conversion_final_unsent() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message =
        sent("From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r");
    message.date_edited = SAMPLE_DATE_LATER;
    message.edited_parts = Some(EditedMessage {
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

    message.components = vec![
        BubbleComponent::Run(vec![AttributedRange::text(
            0,
            28,
            vec![TextEffect::Default],
        )]),
        BubbleComponent::Run(vec![AttributedRange::attachment(
            28,
            31,
            AttachmentMeta {
                guid: Some("D0551D89-4E11-43D0-9A0E-06F19704E97B".to_string()),
                transcription: None,
                height: None,
                width: None,
                name: None,
            },
        )]),
        BubbleComponent::Run(vec![AttributedRange::text(
            31,
            63,
            vec![TextEffect::Default],
        )]),
        BubbleComponent::Retracted,
    ];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">From arbitrary byte stream:\r</span>\n    </div>\n\n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"attachment_error\">Attachment does not exist!</span>\n    </div>\n\n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">To native Rust data structures:\r</span>\n    </div>\n\n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"unsent\"><span class=\"unsent\">You unsent this message part 1 hour, 49 seconds after sending!</span></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_conversion_no_edits() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message =
        sent("From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r");

    message.components = vec![
        BubbleComponent::Run(vec![AttributedRange::text(
            0,
            28,
            vec![TextEffect::Default],
        )]),
        BubbleComponent::Run(vec![AttributedRange::attachment(
            28,
            31,
            AttachmentMeta {
                guid: Some("D0551D89-4E11-43D0-9A0E-06F19704E97B".to_string()),
                transcription: None,
                height: None,
                width: None,
                name: None,
            },
        )]),
        BubbleComponent::Run(vec![AttributedRange::text(
            31,
            63,
            vec![TextEffect::Default],
        )]),
    ];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">From arbitrary byte stream:\r</span>\n    </div>\n\n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"attachment_error\">Attachment does not exist!</span>\n    </div>\n\n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">To native Rust data structures:\r</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_conversion_fully_unsent() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.date_edited = SAMPLE_DATE_LATER;
    message.text = None;
    message.is_from_me = true;
    message.chat_id = Some(0);
    message.edited_parts = Some(EditedMessage {
        parts: vec![EditedMessagePart {
            status: EditStatus::Unsent,
            edit_history: vec![],
            unsend_failed: false,
        }],
        unsupported_handles: vec![],
    });

    message.components = vec![];

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You unsent a message.</p>\n</div>";

    assert_eq!(actual, expected);
}
