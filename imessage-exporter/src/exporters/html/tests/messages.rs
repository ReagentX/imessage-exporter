use std::env::current_dir;

use crate::{
    Config, HTML, Options,
    app::{compatibility::attachment_manager::AttachmentManagerMode, export_type::ExportType},
    exporters::{
        formatter::{MessageFormatter, RenderContext},
        shared::test_support::{
            SAMPLE_DATE, SAMPLE_DATE_LATER, config, received, render, sent, with_contact, with_me,
        },
    },
};

use imessage_database::{
    message_types::text_effects::text_effect::TextEffect,
    tables::{
        messages::{
            Message,
            models::{AttachmentMeta, AttributedRange, BubbleComponent},
        },
        table::FITNESS_RECEIVER,
    },
};

#[test]
fn can_format_html_from_me_normal() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let message = sent("Hello world");

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_fitness_receiver_rewrite() {
    // A Fitness transcript message whose body begins with the
    // `FITNESS_RECEIVER` sentinel must render as "You".
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let message = sent(&format!("{FITNESS_RECEIVER} closed all three rings"));

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">You closed all three rings</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_message_with_html() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let message = sent("<table></table>");

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">&lt;table&gt;&lt;/table&gt;</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_from_me_normal_deleted() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message_with_text("Hello world");
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.deleted_from = Some(0);

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        <span class=\"deleted\">This message was deleted from the conversation!</span>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_from_me_normal_read() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message_with_text("Hello world");
    message.date = SAMPLE_DATE;
    message.date_delivered = SAMPLE_DATE_LATER;
    message.is_from_me = true;

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                (Read by them after 1 hour, 49 seconds)\n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_from_them_normal() {
    let mut config = config(ExportType::Html);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = HTML::new(&config).unwrap();

    let message = received("Hello world", 999999);

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Sample Contact</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_from_them_normal_read() {
    let mut config = config(ExportType::Html);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = HTML::new(&config).unwrap();

    let mut message = received("Hello world", 999999);
    message.date_delivered = SAMPLE_DATE;
    message.date_read = SAMPLE_DATE_LATER;

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                (Read by you after 1 hour, 49 seconds)\n            </span>\n            \n            <span class=\"sender\">Sample Contact</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_from_them_custom_name_read() {
    let mut options = Options::fake_options(ExportType::Html);
    options.custom_name = Some("Name".to_string());
    let mut config = Config::fake_app(options);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = HTML::new(&config).unwrap();

    let mut message = received("Hello world", 999999);
    message.date_delivered = SAMPLE_DATE;
    message.date_read = SAMPLE_DATE_LATER;

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                (Read by Name after 1 hour, 49 seconds)\n            </span>\n            \n            <span class=\"sender\">Sample Contact</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_shareplay() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.item_type = 6;

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        <span class=\"shareplay\"><hr>SharePlay Message Ended</span>\n        \n        \n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_reply_top_level() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("hello");
    message.guid = "TOP-GUID".to_string();
    message.thread_originator_guid = Some("ORIG-GUID".to_string());
    message.thread_originator_part = Some("0:0:0".to_string());

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\" id=\"r-TOP-GUID\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=TOP-GUID\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            \n            <span class=\"reply_anchor\"><a title=\"View in thread\" href=\"#TOP-GUID\">⇱</a></span>\n            \n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">hello</span>\n    </div>\n\n        \n        \n        <span class=\"reply_context\">This message responded to an earlier message.</span>\n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_reply_in_thread() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("hello");
    message.guid = "INNER-GUID".to_string();
    message.thread_originator_guid = Some("ORIG-GUID".to_string());
    message.thread_originator_part = Some("0:0:0".to_string());

    let mut buf = String::with_capacity(2048);
    exporter
        .format_message_into(&message, RenderContext::Reply, &mut buf)
        .unwrap();
    let actual = buf;
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=INNER-GUID\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            \n            <span class=\"reply_anchor\"><a title=\"View in context\" href=\"#r-INNER-GUID\">⇲</a></span>\n            \n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">hello</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_non_reply_has_no_anchor() {
    // Sanity check: a regular message has no reply anchor and no anchor id.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("hello");
    message.guid = "PLAIN-GUID".to_string();

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=PLAIN-GUID\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">hello</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_part_body_attachment_missing_standalone() {
    // An attachment-range Run with no matching Attachment row →
    // PartBody::AttachmentMissing → "<span class=\"attachment_error\">Attachment does not exist!</span>"
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.rowid = i32::MAX; // unlikely to exist in fixture db
    message.is_from_me = true;
    message.chat_id = Some(0);
    message.components = vec![BubbleComponent::Run(vec![AttributedRange::attachment(
        0,
        3,
        AttachmentMeta::default(),
    )])];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"attachment_error\">Attachment does not exist!</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_url_message_without_payload_uses_text_fallback() {
    // Defensive path in dispatch_app_balloon: when a URL-balloon message
    // has no payload row but does carry `text`, the normal `format_url`
    // pipeline still produces a clickable link via its msg.text fallback.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("https://example.com");
    message.rowid = i32::MAX; // not in fixture db, so payload_data returns None
    message.balloon_bundle_id = Some("com.apple.messages.URLBalloonProvider".to_string());
    message.components = vec![BubbleComponent::App];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <div class=\"app\"><a href=\"https://example.com\"><div class=\"app_header\"><div class=\"name\">https://example.com</div></div></a></div>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_url_message_without_payload_escapes_text() {
    // The fallback flows msg.text through `format_url` and the Askama
    // template's auto-escaper; raw HTML in `text` must not survive.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("https://x.test/?q=<script>");
    message.rowid = i32::MAX;
    message.balloon_bundle_id = Some("com.apple.messages.URLBalloonProvider".to_string());
    message.components = vec![BubbleComponent::App];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <div class=\"app\"><a href=\"https://x.test/?q=&lt;script&gt;\"><div class=\"app_header\"><div class=\"name\">https://x.test/?q=&lt;script&gt;</div></div></a></div>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn expressive_renders_via_display_impl() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = sent("Hello world");
    message.expressive_send_style_id =
        Some("com.apple.messages.effect.CKConfettiEffect".to_string());

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n<span class=\"expressive\">Sent with Confetti</span>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn expressive_empty_unknown_renders_like_none() {
    // expressive_send_style_id = Some("") rows must render identically to
    // expressive_send_style_id = None: no stray empty `<span class="expressive">`
    // from the empty Unknown variant passing through the template's `Some` guard.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

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
fn can_format_html_part_body_app_error_on_normal_variant() {
    // BubbleComponent::App on a Variant::Normal message → format_app
    // returns WrongMessageType → PartBody::AppError, escaped via sanitize_html.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.rowid = i32::MAX;
    message.is_from_me = true;
    message.chat_id = Some(0);
    // Default fake_message is Variant::Normal (no balloon_bundle_id, AMT=0).
    // Adding a BubbleComponent::App forces format_app's else-branch.
    message.components = vec![BubbleComponent::App];

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <div class=\"app_error\">Unable to format Normal message: Failed to parse property list: Message is not an app message!</div>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn format_message_into_appends_to_existing_buffer() {
    // Mirrors the production hot path in `run_export`, which reuses a
    // single `String` across messages via `clear()` + `format_message_into`.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let message = sent("hello");

    let standalone = render(&exporter, &message);

    // Pre-fill the buffer with content the writer would have left in
    // (e.g. the previous message). format_message_into should leave that
    // content alone and append the new render after it.
    let prefix = "<!-- previous message -->\n";
    let mut buf = String::with_capacity(2048);
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
    // Capacity should not have shrunk; if anything it grows to fit the
    // new content.
    assert!(buf.capacity() >= cap_before);
}

#[test]
fn can_format_html_started_sharing_location_me() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.is_from_me = false;
    message.other_handle = Some(2);
    message.share_status = false;
    message.share_direction = Some(false);
    message.item_type = 4;

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  5:00:00 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        <span class=\"shared_location\"><hr>Started sharing location!</span>\n        \n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_stopped_sharing_location_me() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.is_from_me = false;
    message.other_handle = Some(2);
    message.share_status = true;
    message.share_direction = Some(false);
    message.item_type = 4;

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  5:00:00 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        <span class=\"shared_location\"><hr>Stopped sharing location!</span>\n        \n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_started_sharing_location_them() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.handle_id = None;
    message.is_from_me = false;
    message.other_handle = Some(0);
    message.share_status = false;
    message.share_direction = Some(false);
    message.item_type = 4;

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  5:00:00 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Unknown</span>\n        </p>\n        \n        \n        \n        \n        <span class=\"shared_location\"><hr>Started sharing location!</span>\n        \n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_stopped_sharing_location_them() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.handle_id = None;
    message.is_from_me = false;
    message.other_handle = Some(0);
    message.share_status = true;
    message.share_direction = Some(false);
    message.item_type = 4;

    let actual = render(&exporter, &message);
    let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  5:00:00 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Unknown</span>\n        </p>\n        \n        \n        \n        \n        <span class=\"shared_location\"><hr>Stopped sharing location!</span>\n        \n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}

// MARK: Walker / jumbomoji integration

fn render_parts<'a>(
    exporter: &'a HTML<'a>,
    message: &'a imessage_database::tables::messages::Message,
    ctx: &mut crate::exporters::shared::message::MessageContext<'a>,
) -> String {
    use crate::exporters::shared::render::render_template;
    let parts = exporter.build_message_parts(message, ctx).unwrap();
    parts
        .iter()
        .map(render_template)
        .collect::<Vec<_>>()
        .join("")
}

fn empty_ctx<'a>() -> crate::exporters::shared::message::MessageContext<'a> {
    use std::collections::HashMap;
    crate::exporters::shared::message::MessageContext {
        attachments: vec![],
        replies_map: HashMap::new(),
        expressive: None,
    }
}

fn make_static_sticker() -> imessage_database::tables::attachment::Attachment {
    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    let mut sticker = Config::fake_attachment();
    sticker.rowid = 3;
    sticker.is_sticker = true;
    sticker.mime_type = Some("image/heic".to_string());
    sticker.filename = Some(sticker_path.to_string_lossy().to_string());
    sticker
}

#[test]
fn inline_stickers_resolve_by_guid_in_body_order() {
    // The body lists stickers in display order via their file-transfer
    // GUIDs, but `Attachment::from_message` returns them in an unspecified
    // join order. The walker must pair each placeholder to its own
    // attachment by GUID so the images keep body order, not DB order.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    // A static sticker with a given GUID and a distinct relative `src`.
    let make = |guid: &str, file: &str| {
        let mut s = Config::fake_attachment();
        s.is_sticker = true;
        s.mime_type = Some("image/heic".to_string());
        s.guid = Some(guid.to_string());
        s.filename = Some(file.to_string());
        s
    };
    // An inline-sticker range carrying its file-transfer GUID.
    let range = |start: usize, guid: &str| AttributedRange {
        start,
        end: start + 3,
        effects: vec![],
        attachment: Some(AttachmentMeta {
            guid: Some(guid.to_string()),
            ..Default::default()
        }),
        emoji_image: true,
    };

    // Body order: A, B, C.
    let mut message = Config::fake_message();
    message.text = Some("\u{FFFC}\u{FFFC}\u{FFFC}".to_string());
    message.components = vec![BubbleComponent::Run(vec![
        range(0, "A"),
        range(3, "B"),
        range(6, "C"),
    ])];

    // Resolved attachments deliberately in a *different* (join) order: C, A, B.
    let mut ctx = empty_ctx();
    ctx.attachments = vec![
        make("C", "c.heic"),
        make("A", "a.heic"),
        make("B", "b.heic"),
    ];

    let actual = render_parts(&exporter, &message, &mut ctx);
    let pa = actual.find("a.heic").expect("a.heic in output");
    let pb = actual.find("b.heic").expect("b.heic in output");
    let pc = actual.find("c.heic").expect("c.heic in output");
    // Body order a < b < c, not the resolved order c, a, b (which positional
    // matching would have produced).
    assert!(
        pa < pb && pb < pc,
        "expected inline stickers in body order a<b<c, got: {actual}"
    );
}

#[test]
fn text_only_single_emoji_renders_jumbo_bubble() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.text = Some("🎉".to_string());
    message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
        0,
        "🎉".len(),
        vec![TextEffect::Default],
    )])];

    let mut ctx = empty_ctx();
    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        actual.contains(r#"<span class="bubble jumbo">🎉</span>"#),
        "expected jumbo class on single-emoji bubble, got: {actual}"
    );
}

#[test]
fn text_only_three_emoji_renders_medium_bubble() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    let text = "🎉🎊🎁".to_string();
    message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
        0,
        text.len(),
        vec![TextEffect::Default],
    )])];
    message.text = Some(text);

    let mut ctx = empty_ctx();
    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        actual.contains(r#"class="bubble medium""#),
        "expected medium class on three-emoji bubble, got: {actual}"
    );
}

#[test]
fn text_only_four_emoji_renders_normal_bubble() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    let text = "🎉🎊🎁🎀".to_string();
    message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
        0,
        text.len(),
        vec![TextEffect::Default],
    )])];
    message.text = Some(text);

    let mut ctx = empty_ctx();
    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        actual.contains(r#"class="bubble""#),
        "expected plain bubble class for 4+ emoji, got: {actual}"
    );
    assert!(
        !actual.contains("jumbo") && !actual.contains("medium"),
        "expected no size class for 4+ emoji, got: {actual}"
    );
}

#[test]
fn text_with_emoji_renders_normal_bubble() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    let text = "Hello 👋".to_string();
    message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
        0,
        text.len(),
        vec![TextEffect::Default],
    )])];
    message.text = Some(text);

    let mut ctx = empty_ctx();
    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        !actual.contains("jumbo") && !actual.contains("medium"),
        "non-pure-emoji text must not get a size class, got: {actual}"
    );
}

#[test]
fn inline_sticker_between_text_renders_one_bubble() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    // "Hello " = 6 bytes, "\u{FFFC}" = 3 bytes, " world" = 6 bytes
    message.text = Some("Hello \u{FFFC} world".to_string());
    // Inline sticker shares the text's part: one Run, ranges in order.
    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 6, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(6, 9, AttachmentMeta::default()),
        AttributedRange::text(9, 15, vec![TextEffect::Default]),
    ])];

    let mut ctx = empty_ctx();
    ctx.attachments = vec![make_static_sticker()];

    let actual = render_parts(&exporter, &message, &mut ctx);

    // The bubble wraps all three segments in one span.
    assert!(
        actual.contains("<span class=\"bubble\">Hello <img class=\"inline_sticker\""),
        "expected text + inline_sticker in the same bubble, got: {actual}"
    );
    // No block-level sticker div or effect suffix.
    assert!(
        !actual.contains("class=\"sticker\""),
        "inline sticker should not emit <div class=\"sticker\">, got: {actual}"
    );
    assert!(
        !actual.contains("sticker_effect"),
        "inline sticker should drop the effect suffix, got: {actual}"
    );
    // The decoration is preserved via title=.
    assert!(
        actual.contains("title=\"Sent with Outline effect\""),
        "inline sticker should carry decoration in title=, got: {actual}"
    );
}

#[test]
fn inline_memoji_among_text_renders_inline_and_keeps_text() {
    // A non-edited run with text, an inline (image/heic) Memoji carrying the
    // `emoji_image` hint, and a trailing emoji: `render_run` must interleave
    // the sticker as an inline <img> and keep the surrounding text/emoji,
    // given the Memoji is resolved in `ctx.attachments`.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let guid = "F2C223DB-0140-4D49-B38A-C1A3553B4CBA";
    let mut message = Config::fake_message();
    message.text = Some("Check this out: \u{FFFC} 😀".to_string());
    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 16, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(
            16,
            19,
            AttachmentMeta {
                guid: Some(guid.to_string()),
                ..Default::default()
            },
        ),
        AttributedRange::text(19, 24, vec![TextEffect::Default]),
    ])];

    // A static image/heic Memoji, matching the real attachment row.
    let mut memoji = make_static_sticker();
    memoji.guid = Some(guid.to_string());
    let mut ctx = empty_ctx();
    ctx.attachments = vec![memoji];

    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        actual.contains("<img class=\"inline_sticker\""),
        "inline Memoji should render an inline <img>, got: {actual}"
    );
    assert!(
        actual.contains("😀"),
        "surrounding text/emoji must survive alongside the sticker, got: {actual}"
    );
    assert!(
        !actual.contains("class=\"sticker\""),
        "Memoji must render inline, not as a block <div class=\"sticker\">, got: {actual}"
    );
}

#[test]
fn edited_message_inlines_memoji_sticker() {
    // Regression for the `MemojiEdited` fixture: an *edited* message with an
    // inline Memoji renders through `format_edited`, not `render_run`. It must
    // still inline the sticker as an <img> in every edit-history row rather
    // than leaking the bare `\u{FFFC}` placeholder.
    use imessage_database::message_types::edited::{
        EditStatus, EditedEvent, EditedMessage, EditedMessagePart,
    };

    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let guid = "F2C223DB-0140-4D49-B38A-C1A3553B4CBA";
    let mk_meta = || AttachmentMeta {
        guid: Some(guid.to_string()),
        ..Default::default()
    };

    // v1 ends with the Memoji; v2 adds " 😀" after it. Both carry the hint.
    let v1 = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 16, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(16, 19, mk_meta()),
    ])];
    let v2 = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 16, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(16, 19, mk_meta()),
        AttributedRange::text(19, 24, vec![TextEffect::Default]),
    ])];

    let mut message = Config::fake_message();
    message.text = Some("Check this out: \u{FFFC} 😀".to_string());
    message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
        0,
        1,
        vec![TextEffect::Default],
    )])];
    message.edited_parts = Some(EditedMessage {
        parts: vec![EditedMessagePart {
            status: EditStatus::Edited,
            edit_history: vec![
                EditedEvent {
                    date: 758573156000000000,
                    text: "Check this out: \u{FFFC}".to_string(),
                    components: v1,
                    guid: None,
                },
                EditedEvent {
                    date: 758573166000000000,
                    text: "Check this out: \u{FFFC} 😀".to_string(),
                    components: v2,
                    guid: None,
                },
            ],
        }],
    });

    let mut memoji = make_static_sticker();
    memoji.guid = Some(guid.to_string());
    let mut ctx = empty_ctx();
    ctx.attachments = vec![memoji];

    let actual = render_parts(&exporter, &message, &mut ctx);

    assert_eq!(
        actual.matches("<img class=\"inline_sticker\"").count(),
        2,
        "both edit versions should inline the Memoji as an <img>, got: {actual}"
    );
    assert!(
        actual.contains('😀'),
        "the trailing emoji must survive, got: {actual}"
    );
    assert!(
        !actual.contains('\u{FFFC}'),
        "no bare object-replacement placeholder should leak, got: {actual}"
    );
}

#[test]
fn missing_inline_sticker_row_renders_placeholder() {
    // An inline-sticker range that resolves to an absent attachment row (a
    // dangling placeholder / orphaned join → out-of-bounds index) must show
    // the same "missing" marker as the block path, not vanish silently.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.text = Some("\u{FFFC}".to_string());
    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::inline_attachment(0, 3, AttachmentMeta::default()),
    ])];

    // No resolved attachments → the range's index is out of bounds.
    let mut ctx = empty_ctx();
    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        actual.contains("Attachment does not exist!"),
        "missing inline sticker must render a placeholder, got: {actual}"
    );
}

#[test]
fn single_inline_sticker_renders_jumbo_bubble() {
    // A pure-sticker message (one static sticker, no text) should render
    // as a jumbo-sized inline bubble.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.text = Some("\u{FFFC}".to_string());
    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::inline_attachment(0, 3, AttachmentMeta::default()),
    ])];

    let mut ctx = empty_ctx();
    ctx.attachments = vec![make_static_sticker()];

    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        actual.contains("class=\"bubble jumbo\""),
        "single static sticker should be jumbo, got: {actual}"
    );
    assert!(
        actual.contains("<img class=\"inline_sticker\""),
        "expected inline_sticker img, got: {actual}"
    );
}

#[test]
fn multiple_inline_stickers_render_in_one_bubble() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    // 6 stickers separated by spaces, no surrounding text. Each \u{FFFC}
    // is 3 bytes; spaces are 1 byte. Components must reference the spaces.
    let text = "\u{FFFC} \u{FFFC} \u{FFFC} \u{FFFC} \u{FFFC} \u{FFFC}".to_string();
    message.text = Some(text);
    // All six inline stickers and the spaces between them share one part.
    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::inline_attachment(0, 3, AttachmentMeta::default()),
        AttributedRange::text(3, 4, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(4, 7, AttachmentMeta::default()),
        AttributedRange::text(7, 8, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(8, 11, AttachmentMeta::default()),
        AttributedRange::text(11, 12, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(12, 15, AttachmentMeta::default()),
        AttributedRange::text(15, 16, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(16, 19, AttachmentMeta::default()),
        AttributedRange::text(19, 20, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(20, 23, AttachmentMeta::default()),
    ])];

    let mut ctx = empty_ctx();
    ctx.attachments = (0..6).map(|_| make_static_sticker()).collect();

    let actual = render_parts(&exporter, &message, &mut ctx);
    let sticker_count = actual.matches("<img class=\"inline_sticker\"").count();
    assert_eq!(
        sticker_count, 6,
        "expected 6 inline stickers in a single bubble, got {sticker_count}: {actual}"
    );
    let bubble_open_count = actual.matches("<span class=\"bubble").count();
    assert_eq!(
        bubble_open_count, 1,
        "all stickers should share one bubble, found {bubble_open_count} bubbles: {actual}"
    );
    assert!(
        !actual.contains("class=\"sticker\""),
        "no block-level sticker divs expected: {actual}"
    );
}

#[test]
fn animated_sticker_renders_as_block() {
    // HEIC sequence stickers stay on the existing block path (their own
    // <div class="sticker"> with the "Sent with … effect" suffix), even
    // when surrounded by text.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.text = Some("Hi \u{FFFC} bye".to_string());
    // An animated sticker is its own part, so the text on either side stays
    // in its own Run: three parts in all.
    message.components = vec![
        BubbleComponent::Run(vec![AttributedRange::text(0, 3, vec![TextEffect::Default])]),
        BubbleComponent::Run(vec![AttributedRange::attachment(
            3,
            6,
            AttachmentMeta::default(),
        )]),
        BubbleComponent::Run(vec![AttributedRange::text(
            6,
            10,
            vec![TextEffect::Default],
        )]),
    ];

    let mut animated = make_static_sticker();
    animated.mime_type = Some("image/heic-sequence".to_string());
    let mut ctx = empty_ctx();
    ctx.attachments = vec![animated];

    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        actual.contains("<div class=\"sticker\">"),
        "animated sticker should keep block rendering, got: {actual}"
    );
    // Surrounding text should NOT have merged into an inline bubble.
    assert!(
        !actual.contains("class=\"inline_sticker\""),
        "animated sticker must not emit inline_sticker img: {actual}"
    );
}

#[test]
fn translated_message_with_inline_sticker_uses_block_path() {
    // When a message is translated, every component falls back to the
    // block-style rendering–including stickers–so the bubble's
    // semantics stay consistent (a translated text bubble alongside an
    // inline-style sticker would look incoherent).
    let mut config = config(ExportType::Html);
    let test_guid = "TRANSLATED-STICKER-GUID-0001".to_string();
    config.translated_messages.insert(test_guid.clone());
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.guid = test_guid;
    message.text = Some("\u{FFFC}".to_string());
    message.components = vec![BubbleComponent::Run(vec![AttributedRange::attachment(
        0,
        3,
        AttachmentMeta::default(),
    )])];

    let mut ctx = empty_ctx();
    ctx.attachments = vec![make_static_sticker()];

    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        actual.contains("<div class=\"sticker\">"),
        "translated message must keep sticker on the block path: {actual}"
    );
    assert!(
        !actual.contains("class=\"inline_sticker\""),
        "no inline_sticker img expected for a translated message: {actual}"
    );
}

#[test]
fn translated_message_with_inline_sticker_keeps_text_and_sticker() {
    // For a translated run of [text, inline Memoji, text], the text and the
    // sticker must both survive (interleaved as the translation's "original").
    // The translation itself is fetched from the DB via `get_translation`, so
    // with no translation row this renders as a plain bubble: the regression
    // being guarded is the lost text/sticker, not the translation lookup.
    let mut config = config(ExportType::Html);
    let test_guid = "TRANSLATED-INLINE-TEXT-0001".to_string();
    config.translated_messages.insert(test_guid.clone());
    let exporter = HTML::new(&config).unwrap();

    let sticker_guid = "F2C223DB-0140-4D49-B38A-C1A3553B4CBA";
    let mut message = Config::fake_message();
    message.guid = test_guid;
    message.text = Some("Look at this \u{FFFC} now".to_string());
    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 13, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(
            13,
            16,
            AttachmentMeta {
                guid: Some(sticker_guid.to_string()),
                ..Default::default()
            },
        ),
        AttributedRange::text(16, 20, vec![TextEffect::Default]),
    ])];

    let mut memoji = make_static_sticker();
    memoji.guid = Some(sticker_guid.to_string());
    let mut ctx = empty_ctx();
    ctx.attachments = vec![memoji];

    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        actual.contains("Look at this"),
        "leading text must survive on the translated path: {actual}"
    );
    assert!(
        actual.contains("now"),
        "trailing text must survive on the translated path: {actual}"
    );
    assert!(
        actual.contains("<img class=\"inline_sticker\""),
        "the inline sticker must still render: {actual}"
    );
    assert!(
        !actual.contains("class=\"sticker\""),
        "must not collapse to a lone block sticker: {actual}"
    );
}

#[test]
fn edited_text_part_flushes_block_but_sibling_sticker_inlines() {
    // Per-part edit semantics: an edited text part block-flushes via
    // `<div class="edited">`, but an adjacent attachment whose own part
    // index isn't marked Edited still goes through the inline path.
    // Edits to text never bleed into sibling stickers.
    use imessage_database::message_types::edited::{EditStatus, EditedMessage, EditedMessagePart};

    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.text = Some("hi \u{FFFC}".to_string());
    // An edit applies to a whole Run/part, so the edited text and the
    // sibling sticker live in separate parts; the edit can't bleed across.
    message.components = vec![
        BubbleComponent::Run(vec![AttributedRange::text(0, 3, vec![TextEffect::Default])]),
        BubbleComponent::Run(vec![AttributedRange::inline_attachment(
            3,
            6,
            AttachmentMeta::default(),
        )]),
    ];
    // Mark only the Text part (idx 0) as edited.
    message.edited_parts = Some(EditedMessage {
        parts: vec![
            EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![],
            },
            EditedMessagePart {
                status: EditStatus::Original,
                edit_history: vec![],
            },
        ],
    });
    message.date_edited = SAMPLE_DATE;

    let mut ctx = empty_ctx();
    ctx.attachments = vec![make_static_sticker()];

    let actual = render_parts(&exporter, &message, &mut ctx);
    // Edited text part block-flushes through dispatch_part_body's
    // PartBody::TextEdited arm.
    assert!(
        actual.contains("class=\"edited\"") || actual.contains("Edited"),
        "edited text part should not be inlined: {actual}"
    );
    // Sticker's own idx isn't Edited → still on the inline path.
    assert!(
        actual.contains("class=\"inline_sticker\""),
        "non-edited sticker should still go inline: {actual}"
    );
}

#[test]
fn missing_inline_sticker_renders_as_broken_image() {
    // A sticker with no filename should still produce a visible inline
    // `<img>` (which the browser renders as its broken-image glyph)
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.text = Some("hi \u{FFFC}".to_string());
    // Inline sticker shares the text's part: one Run.
    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 3, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(3, 6, AttachmentMeta::default()),
    ])];

    let mut sticker = Config::fake_attachment();
    sticker.rowid = 3;
    sticker.is_sticker = true;
    sticker.mime_type = Some("image/heic".to_string());
    // No filename → `AttachmentFailure::MissingFilename`.
    sticker.filename = None;
    sticker.transfer_name = None;

    let mut ctx = empty_ctx();
    ctx.attachments = vec![sticker];

    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        actual.contains("<img class=\"inline_sticker\" loading=\"lazy\">"),
        "missing sticker should emit a bare inline <img> (no src, broken-image glyph): {actual}"
    );
    assert!(
        !actual.contains("attachment_error"),
        "must not splice error text into a bubble: {actual}"
    );
}

#[test]
fn inline_bubble_carries_part_tapbacks() {
    // One inline Run (text + sticker) is a single part; both tapbacks
    // registered on that part index render on the one merged bubble.
    use std::collections::HashMap;

    let mut config = config(ExportType::Html);
    let parent_guid = "INLINE-TAPBACK-PARENT-0001".to_string();

    let mut tapback_a = Config::fake_message();
    tapback_a.associated_message_type = Some(2000); // Loved
    tapback_a.associated_message_guid = Some(parent_guid.clone());
    let mut tapback_b = Config::fake_message();
    tapback_b.associated_message_type = Some(2001); // Liked
    tapback_b.associated_message_guid = Some(parent_guid.clone());

    let mut by_idx: HashMap<usize, Vec<imessage_database::tables::messages::Message>> =
        HashMap::new();
    // Both tapbacks belong to the single inline part (index 0).
    by_idx.insert(0, vec![tapback_a, tapback_b]);
    config.tapbacks.insert(parent_guid.clone(), by_idx);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.guid = parent_guid;
    message.text = Some("hi \u{FFFC}".to_string());
    // Text and inline sticker share one part (one Run).
    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 3, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(3, 6, AttachmentMeta::default()),
    ])];

    let mut ctx = empty_ctx();
    ctx.attachments = vec![make_static_sticker()];

    let actual = render_parts(&exporter, &message, &mut ctx);
    let bubble_count = actual.matches("<span class=\"bubble\"").count();
    assert_eq!(bubble_count, 1, "expected one merged bubble: {actual}");
    let tapback_count = actual.matches("<span class=\"tapback\">").count();
    assert_eq!(
        tapback_count, 2,
        "expected both of the part's tapbacks on the merged bubble, got {tapback_count}: {actual}"
    );
}

#[test]
fn inline_bubble_carries_part_replies() {
    // One inline Run (text + sticker) is a single part; both reply threads
    // registered on that part index appear under the one merged bubble.
    use std::collections::HashMap;

    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.text = Some("hi \u{FFFC}".to_string());
    // Text and inline sticker share one part (one Run).
    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 3, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(3, 6, AttachmentMeta::default()),
    ])];

    let mut reply_a = Config::fake_message();
    reply_a.guid = "REPLY-A-GUID".to_string();
    reply_a.text = Some("re: text part".to_string());
    reply_a.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
        0,
        13,
        vec![TextEffect::Default],
    )])];

    let mut reply_b = Config::fake_message();
    reply_b.guid = "REPLY-B-GUID".to_string();
    reply_b.text = Some("re: sticker".to_string());
    reply_b.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
        0,
        11,
        vec![TextEffect::Default],
    )])];

    let mut replies_map: HashMap<usize, Vec<imessage_database::tables::messages::Message>> =
        HashMap::new();
    // Both replies belong to the single inline part (index 0).
    replies_map.insert(0, vec![reply_a, reply_b]);

    let mut ctx = empty_ctx();
    ctx.attachments = vec![make_static_sticker()];
    ctx.replies_map = replies_map;

    let actual = render_parts(&exporter, &message, &mut ctx);
    let bubble_count = actual.matches("<span class=\"bubble\"").count();
    // Replies render their own bubbles too. The outer merged bubble plus
    // one bubble per reply means at least 3. What we're verifying is
    // that the two reply bodies both appear in the merged output.
    assert!(
        bubble_count >= 3,
        "expected merged bubble + at least 2 reply bubbles, got {bubble_count}: {actual}"
    );
    assert!(
        actual.contains("re: text part"),
        "reply A missing from merged bubble: {actual}"
    );
    assert!(
        actual.contains("re: sticker"),
        "reply B missing from merged bubble: {actual}"
    );
}

#[test]
fn expressive_renders_once_on_merged_inline_bubble() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.text = Some("hi \u{FFFC}".to_string());
    // Text and inline sticker share one part (one Run).
    message.components = vec![BubbleComponent::Run(vec![
        AttributedRange::text(0, 3, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(3, 6, AttachmentMeta::default()),
    ])];
    message.expressive_send_style_id =
        Some("com.apple.MobileSMS.expressivesend.confetti".to_string());

    let mut ctx = empty_ctx();
    ctx.attachments = vec![make_static_sticker()];
    ctx.expressive = match message.get_expressive() {
        imessage_database::message_types::expressives::Expressive::None
        | imessage_database::message_types::expressives::Expressive::Unknown("") => None,
        other => Some(other),
    };

    let actual = render_parts(&exporter, &message, &mut ctx);
    let expressive_count = actual.matches("<span class=\"expressive\">").count();
    assert_eq!(
        expressive_count, 1,
        "merged inline bubble should fire the expressive marker once, got {expressive_count}: {actual}"
    );
}

#[test]
fn animated_sticker_alone_is_not_jumbo() {
    // A single animated sticker stays as a block-style sticker; the
    // jumbomoji bucketing only applies to inline-eligible content.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.text = Some("\u{FFFC}".to_string());
    message.components = vec![BubbleComponent::Run(vec![AttributedRange::attachment(
        0,
        3,
        AttachmentMeta::default(),
    )])];

    let mut animated = make_static_sticker();
    animated.mime_type = Some("image/heic-sequence".to_string());
    let mut ctx = empty_ctx();
    ctx.attachments = vec![animated];

    let actual = render_parts(&exporter, &message, &mut ctx);
    assert!(
        !actual.contains("jumbo") && !actual.contains("medium"),
        "animated sticker must not trigger jumbomoji sizing: {actual}"
    );
    assert!(
        actual.contains("<div class=\"sticker\">"),
        "expected block sticker rendering: {actual}"
    );
}

#[test]
fn can_format_html_single_url_no_bundle_id() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

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
        "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=FAKEGUID-D0C8-4212-AA87-DD8AE4FD1203\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Unknown</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <div class=\"app\"><a href=\"https://www.ghacks.net/2020/01/23/lastpass-no-longer-listed-on-the-chrome-web-store/\"><div class=\"app_header\"><div class=\"name\">gHacks Technology News</div></div><div class=\"app_footer\"><div class=\"caption\">LastPass no longer listed on the Chrome Web Store - gHacks Tech News</div><div class=\"subcaption\">LastPass customers and new users searching for password managers on Google&apos;s Chrome Web Store may have noticed that the LastPass extension for Google Chrome is currently no longer listed on the store.</div></div></a></div>\n    </div>\n\n        \n        \n    </div>\n</div>\n"
    );
}

#[test]
fn can_format_html_translated_message() {
    let mut options = Options::fake_options(ExportType::Html);
    options.attachment_manager.mode = AttachmentManagerMode::Clone;

    let mut config = Config::fake_app(options);
    config
        .translated_messages
        .insert("56FE94B9-2345-4A3C-A57F-949BDDDDF9FF".to_string());

    let exporter = HTML::new(&config).unwrap();

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
    let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=56FE94B9-2345-4A3C-A57F-949BDDDDF9FF\">Dec 31, 2000  5:00:00 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Unknown</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Oh, il a traduit ce que j&apos;ai envoyé !</span>\n    <div class=\"translated\"><span class=\"bubble\">Oh it translated what I sent!</span></div>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

    assert_eq!(actual, expected);
}
