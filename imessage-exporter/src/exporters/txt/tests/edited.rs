use crate::{
    Config, TXT,
    app::export_type::ExportType,
    exporters::{
        formatter::MessageFormatter,
        shared::test_support::{
            SAMPLE_DATE, SAMPLE_DATE_LATER, config, received, render, sent, with_contact,
        },
    },
};

use imessage_database::{
    message_types::{
        edited::{EditStatus, EditedMessage, EditedMessagePart},
        text_effects::text_effect::TextEffect,
    },
    tables::messages::models::{AttachmentMeta, AttributedRange, BubbleComponent},
};

#[test]
fn edited_history_renders_unindented() {
    // format_edited returns rows separated by `\n` with no indent baked
    // in. `push_indented` applies the reply prefix uniformly across all
    // lines downstream.
    use crate::exporters::{
        shared::edited::Edit,
        txt::view_model::{EditedRow, EditedVM},
    };
    use askama::Template;

    let vm = EditedVM {
        kind: Edit::Edited {
            rows: vec![
                EditedRow {
                    timestamp_prefix: "5:29:42 PM ".to_string(),
                    text: "first",
                },
                EditedRow {
                    timestamp_prefix: "Edited 30 seconds later: ".to_string(),
                    text: "second",
                },
            ],
        },
    };

    let rendered = vm.render().unwrap();
    assert_eq!(
        rendered,
        "5:29:42 PM first\nEdited 30 seconds later: second\n",
    );

    // Wrapping in PartBody::Line, rendering, and applying push_indented
    // produces a uniformly prefixed block.
    use crate::exporters::txt::view_model::{MessagePartVM, PartBody};
    let part = MessagePartVM {
        body: PartBody::Line {
            text: rendered.trim_end_matches('\n').to_string(),
        },
        expressive: None,
        tapbacks: None,
        replies: None,
    };
    let unindented = part.render().unwrap();
    let mut full = String::new();
    TXT::push_indented(&mut full, &unindented, "    ");
    assert_eq!(
        full,
        "    5:29:42 PM first\n    Edited 30 seconds later: second\n",
    );
}

#[test]
fn can_format_txt_conversion_final_unsent() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message =
        sent("From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r");
    message.date_edited = SAMPLE_DATE_LATER;
    message.edited_parts = Some(EditedMessage {
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
    let expected = "May 17, 2022  5:29:42 PM\nMe\nFrom arbitrary byte stream:\r\nAttachment missing!\nTo native Rust data structures:\r\nYou unsent this message part 1 hour, 49 seconds after sending!\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_unsent_from_them_resolves_contact_name() {
    // When someone else unsends a message part, the resolved contact
    // name must be rendered, matching the behavior of TXT's tapback,
    // announcement, and message-header rendering.
    let mut config = config(ExportType::Txt);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = TXT::new(&config).unwrap();

    let mut message = received("hello", 999999);
    message.date_edited = SAMPLE_DATE_LATER;
    message.edited_parts = Some(EditedMessage {
        parts: vec![EditedMessagePart {
            status: EditStatus::Unsent,
            edit_history: vec![],
        }],
    });
    message.components = vec![BubbleComponent::Retracted];

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nSample Contact\nSample Contact unsent this message part 1 hour, 49 seconds after sending!\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_conversion_no_edits() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
        BubbleComponent::Retracted,
    ];

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\nFrom arbitrary byte stream:\r\nAttachment missing!\nTo native Rust data structures:\r\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_conversion_fully_unsent() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
        }],
    });

    message.components = vec![BubbleComponent::Retracted];

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You unsent a message!\n\n";

    assert_eq!(actual, expected);
}
