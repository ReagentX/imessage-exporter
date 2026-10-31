use crate::{
    Config, Options, TXT,
    app::{compatibility::attachment_manager::AttachmentManagerMode, export_type::ExportType},
    exporters::{
        formatter::{MessageFormatter, RenderContext},
        shared::test_support::{
            SAMPLE_DATE, SAMPLE_DATE_LATER, config, received, render, sent, with_contact, with_me,
        },
    },
};

use imessage_database::tables::{
    messages::{
        Message,
        models::{BubbleComponent, ConversationAVMode},
    },
    table::FITNESS_RECEIVER,
};

#[test]
fn can_get_time_invalid() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE_LATER;
    message.date_delivered = SAMPLE_DATE_LATER;
    message.date_read = SAMPLE_DATE;
    assert_eq!(exporter.get_time(&message), "May 17, 2022  6:30:31 PM");
}

#[test]
fn can_format_txt_from_me_normal() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let message = sent("Hello world");

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\nHello world\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_fitness_receiver_rewrite() {
    // A Fitness transcript message whose body begins with the
    // `FITNESS_RECEIVER` sentinel must render as "You", matching HTML.
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let message = sent(&format!("{FITNESS_RECEIVER} closed all three rings"));

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\nYou closed all three rings\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn expressive_renders_via_display_impl() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = sent("Hello world");
    message.expressive_send_style_id =
        Some("com.apple.messages.effect.CKConfettiEffect".to_string());

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\nHello world\nSent with Confetti\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn expressive_empty_unknown_renders_like_none() {
    // expressive_send_style_id = Some("") rows must render identically to
    // expressive_send_style_id = None: no stray blank line from the empty
    // Unknown variant passing through the template's `Some` guard.
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let build = |expressive: Option<String>| {
        let mut m = sent("Hello world");
        m.expressive_send_style_id = expressive;
        m
    };

    let baseline = render(&exporter, &build(None));

    let actual = render(&exporter, &build(Some(String::new())));

    assert_eq!(actual, baseline);
}

#[test]
fn can_format_txt_from_me_normal_deleted() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message_with_text("Hello world");
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.deleted_from = Some(0);

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\nThis message was deleted from the conversation!\nHello world\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_from_me_normal_read() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message_with_text("Hello world");
    message.date = SAMPLE_DATE;
    message.date_delivered = SAMPLE_DATE_LATER;
    message.is_from_me = true;

    let actual = render(&exporter, &message);
    let expected =
        "May 17, 2022  5:29:42 PM (Read by them after 1 hour, 49 seconds)\nMe\nHello world\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_from_them_normal() {
    let mut config = config(ExportType::Txt);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = TXT::new(&config).unwrap();

    let message = received("Hello world", 999999);

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nSample Contact\nHello world\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_from_them_normal_read() {
    let mut config = config(ExportType::Txt);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = TXT::new(&config).unwrap();

    let mut message = received("Hello world", 999999);
    message.date_delivered = SAMPLE_DATE;
    message.date_read = SAMPLE_DATE_LATER;

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM (Read by you after 1 hour, 49 seconds)\nSample Contact\nHello world\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_from_them_custom_name_read() {
    let mut options = Options::fake_options(ExportType::Txt);
    options.custom_name = Some("Name".to_string());
    let mut config = Config::fake_app(options);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = TXT::new(&config).unwrap();

    let mut message = received("Hello world", 999999);
    message.date_delivered = SAMPLE_DATE;
    message.date_read = SAMPLE_DATE_LATER;

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM (Read by Name after 1 hour, 49 seconds)\nSample Contact\nHello world\n\n";

    assert_eq!(actual, expected);
}

/// FaceTime or SharePlay row at `SAMPLE_DATE` with `mode`.
fn conversation(mode: Option<ConversationAVMode>) -> Message {
    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.item_type = 6;
    message.conversation_av_mode = mode;
    message
}

#[test]
fn can_format_txt_shareplay() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let message = conversation(Some(ConversationAVMode::AVLess));

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\nSharePlay Message\nEnded\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_facetime() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let expected = "May 17, 2022  5:29:42 PM\nMe\nFaceTime\nCall Ended\n\n";
    for mode in [
        Some(ConversationAVMode::Audio),
        Some(ConversationAVMode::Video),
        Some(ConversationAVMode::Unknown(3)),
        None,
    ] {
        assert_eq!(render(&exporter, &conversation(mode)), expected, "{mode:?}");
    }
}

#[test]
fn format_message_into_appends_to_existing_buffer() {
    // Mirrors the production hot path in `run_export`, which reuses a
    // single `String` across messages via `clear()` + `format_message_into`.
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let message = sent("hello");

    let standalone = render(&exporter, &message);

    let prefix = "PREV MSG\n\n";
    let mut buf = String::with_capacity(1024);
    buf.push_str(prefix);
    let cap_before = buf.capacity();

    exporter
        .format_message_into(&message, RenderContext::TopLevel, &mut buf)
        .unwrap();

    assert!(
        buf.starts_with(prefix),
        "format_message_into must not overwrite existing buffer content"
    );
    assert_eq!(&buf[prefix.len()..], standalone);
    assert!(buf.capacity() >= cap_before);
}

#[test]
fn can_format_txt_started_sharing_location_me() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.is_from_me = false;
    message.other_handle = Some(2);
    message.share_status = 0;
    message.share_direction = Some(0);
    message.item_type = 4;

    let actual = render(&exporter, &message);
    let expected = "Dec 31, 2000  5:00:00 PM\nMe\nStarted sharing location!\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_stopped_sharing_location_me() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.is_from_me = false;
    message.other_handle = Some(2);
    message.share_status = 1;
    message.share_direction = Some(0);
    message.item_type = 4;

    let actual = render(&exporter, &message);
    let expected = "Dec 31, 2000  5:00:00 PM\nMe\nStopped sharing location!\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_started_sharing_location_them() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.handle_id = None;
    message.is_from_me = false;
    message.other_handle = Some(0);
    message.share_status = 0;
    message.share_direction = Some(0);
    message.item_type = 4;

    let actual = render(&exporter, &message);
    let expected = "Dec 31, 2000  5:00:00 PM\nUnknown\nStarted sharing location!\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_stopped_sharing_location_them() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.handle_id = None;
    message.is_from_me = false;
    message.other_handle = Some(0);
    message.share_status = 1;
    message.share_direction = Some(0);
    message.item_type = 4;

    let actual = render(&exporter, &message);
    let expected = "Dec 31, 2000  5:00:00 PM\nUnknown\nStopped sharing location!\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_single_url_no_bundle_id() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let parsed = Message::from_guid(
        "FAKEGUID-D0C8-4212-AA87-DD8AE4FD1203",
        config.data_source.db(),
        &config.data_source.capabilities,
    )
    .unwrap();
    let message = Message {
        guid: parsed.guid,
        rowid: parsed.rowid,
        date: SAMPLE_DATE,
        text: parsed.text,
        components: parsed.components,
        balloon_bundle_id: parsed.balloon_bundle_id,
        ..Config::fake_message()
    };

    let actual = render(&exporter, &message);

    assert_eq!(
        actual,
        "May 17, 2022  5:29:42 PM\nUnknown\nhttps://www.ghacks.net/2020/01/23/lastpass-no-longer-listed-on-the-chrome-web-store/\nLastPass no longer listed on the Chrome Web Store - gHacks Tech News\nLastPass customers and new users searching for password managers on Google's Chrome Web Store may have noticed that the LastPass extension for Google Chrome is currently no longer listed on the store.\n\n"
    );
}

#[test]
fn can_format_txt_translated_message() {
    let mut options = Options::fake_options(ExportType::Txt);
    options.attachment_manager.mode = AttachmentManagerMode::Clone;

    let mut config = Config::fake_app(options);
    config
        .translated_messages
        .insert("56FE94B9-2345-4A3C-A57F-949BDDDDF9FF".to_string());

    let exporter = TXT::new(&config).unwrap();

    let parsed = Message::from_guid(
        "56FE94B9-2345-4A3C-A57F-949BDDDDF9FF",
        config.data_source.db(),
        &config.data_source.capabilities,
    )
    .unwrap();
    let message = Message {
        guid: parsed.guid,
        rowid: parsed.rowid,
        text: parsed.text,
        components: parsed.components,
        ..Config::fake_message()
    };

    let actual = render(&exporter, &message);
    let expected = "Dec 31, 2000  5:00:00 PM\nUnknown\nOh, il a traduit ce que j'ai envoyé !\nTranslated from:\nOh it translated what I sent!\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn balloon_indent_matches_header_in_reply_context() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut msg = sent("https://example.com");
    msg.balloon_bundle_id = Some("com.apple.messages.URLBalloonProvider".to_string());
    msg.rowid = i32::MAX;
    msg.components = vec![BubbleComponent::App];

    let mut out = String::new();
    exporter
        .format_message_into(&msg, RenderContext::Reply, &mut out)
        .unwrap();

    // Every non-blank line should start with exactly four spaces.
    for line in out.lines().filter(|l| !l.is_empty()) {
        let leading = line.chars().take_while(|c| *c == ' ').count();
        assert_eq!(
            leading, 4,
            "expected 4-space indent on every non-blank line, got {leading} on: {line:?}\nfull output:\n{out}"
        );
    }
}

#[test]
fn message_part_emits_one_output_line_per_source_line() {
    // Locks in the `text.lines()` iteration in message_part.txt: a
    // PartBody::Line carrying multi-line text must emit one output line
    // per source line. Indentation is applied later by `push_indented`.
    use crate::exporters::txt::view_model::{MessagePartVM, PartBody};
    use askama::Template;

    let vm = MessagePartVM {
        body: PartBody::Line {
            text: "line1\nline2\nline3".to_string(),
        },
        expressive: None,
        tapbacks: None,
        replies: None,
    };

    let rendered = vm.render().unwrap();
    assert_eq!(rendered, "line1\nline2\nline3\n");
}

#[test]
fn replies_vm_separates_siblings_with_blank_line() {
    use crate::exporters::{shared::reply::ReplyEntry, txt::view_model::RepliesVM};
    use askama::Template;

    let vm = RepliesVM {
        replies: vec![
            ReplyEntry {
                guid: "one".to_string(),
                body: "reply one\n".to_string(),
            },
            ReplyEntry {
                guid: "two".to_string(),
                body: "reply two\n".to_string(),
            },
        ],
    };
    let rendered = vm.render().unwrap();
    assert_eq!(rendered, "reply one\n\nreply two\n\n");
}

#[test]
fn push_indented_skips_blank_lines() {
    let mut out = String::new();
    TXT::push_indented(&mut out, "a\n\nb\n", "    ");
    assert_eq!(out, "    a\n\n    b\n");
}

#[test]
fn can_format_txt_url_message_without_payload_uses_text_fallback() {
    // Defensive path in dispatch_app_balloon: when a URL-balloon message
    // has no payload row but does carry `text`, the normal `format_url`
    // pipeline still produces the raw URL via its msg.text fallback.
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = sent("https://example.com");
    message.rowid = i32::MAX;
    message.balloon_bundle_id = Some("com.apple.messages.URLBalloonProvider".to_string());
    message.components = vec![BubbleComponent::App];

    let actual = render(&exporter, &message);
    let expected = "May 17, 2022  5:29:42 PM\nMe\nhttps://example.com\n\n";

    assert_eq!(actual, expected);
}
