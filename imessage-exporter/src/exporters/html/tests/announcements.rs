use imessage_database::tables::messages::Message;

use crate::{
    Config, HTML, Options,
    app::export_type::ExportType,
    exporters::{
        formatter::MessageFormatter,
        shared::test_support::{SAMPLE_DATE, config, with_contact, with_me},
    },
};

#[test]
fn can_format_html_announcement() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 2;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You named the conversation <b>Hello world</b></p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_announcement_custom_name() {
    let mut options = Options::fake_options(ExportType::Html);
    options.custom_name = Some("Name".to_string());
    let mut config = Config::fake_app(options);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.item_type = 2;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Name named the conversation <b>Hello world</b></p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_group_name_removed() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 2;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You removed the name from the conversation.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn html_announcement_who_is_escaped_once() {
    // Regression: `who` is rendered with the default escaper in the
    // template, so the formatter must not pre-escape it. Pre-escaping
    // would produce `&amp;amp;` for an `&` in the name.
    let mut options = Options::fake_options(ExportType::Html);
    options.custom_name = Some("Bob & <Alice>".to_string());
    let mut config = Config::fake_app(options);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 3; // ParticipantLeft

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Bob &amp; &lt;Alice&gt; left the conversation.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_announcement_unknown() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected =
        "\n<div class=\"announcement\">\n    <p>Unable to format announcement!</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_group_removed() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 1;
    message.group_action_type = 1;
    message.other_handle = Some(1);

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You removed Other from the conversation.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_group_removed_other() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");
    with_contact(&mut config, 2, "Second");

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = false;
    message.handle_id = Some(1);
    message.item_type = 1;
    message.group_action_type = 1;
    message.other_handle = Some(2);

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Other removed Second from the conversation.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_group_changed_number() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = false;
    message.handle_id = Some(1);
    message.item_type = 1;
    message.group_action_type = 0;
    message.other_handle = Some(1);

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Other changed their phone number.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_group_added() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 1;
    message.group_action_type = 0;
    message.other_handle = Some(1);

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You added Other to the conversation.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_group_left() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 3;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You left the conversation.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_group_icon_removed() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 2;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You removed the group photo.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_group_icon_added() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 1;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You changed the group photo.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_chat_background_removed() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 6;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You removed the chat background.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_chat_background_added() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 4;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You changed the chat background.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_chat_background_sensitive() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 5;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You set a chat background which may be sensitive.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_group_icon_added_hidden() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 3;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You changed the group photo.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_chat_background_added_hidden() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 7;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You changed the chat background.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_audio_message_kept() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 5;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You kept an audio message.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_other_kept_messages() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    for (code, wording) in [
        (1, "kept a video message."),
        (2, "kept a message."),
        (3, "kept a message."),
    ] {
        let mut message = Config::fake_message();
        message.date = SAMPLE_DATE;
        message.is_from_me = true;
        message.item_type = 5;
        message.message_action_type = code;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = format!(
            "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You {wording}</p>\n</div>\n"
        );

        assert_eq!(actual, expected, "{code}");
    }
}

#[test]
fn can_format_html_transcript_sharing_stopped() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.handle_id = Some(1);
    message.item_type = 7;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Other stopped sharing Emergency SOS transcript of this conversation with you.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_emergency_location_sent() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = HTML::new(&config).unwrap();

    for is_from_me in [false, true] {
        let mut message = Config::fake_message();
        message.date = SAMPLE_DATE;
        message.handle_id = Some(1);
        message.is_from_me = is_from_me;
        message.item_type = 8;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Location sent to emergency services.</p>\n</div>\n";

        assert_eq!(actual, expected, "{is_from_me}");
    }
}

/// Group events with actorless forms and nil senders in Messages, paired
/// with the expected text for each event.
const UNATTRIBUTED: [(i32, i32, &str); 10] = [
    (1, 0, "Other was added to the conversation."),
    (1, 1, "Other was removed from the conversation."),
    (2, 0, "The conversation was named <b>Hello world</b>"),
    (3, 1, "The group photo was changed."),
    (3, 2, "The group photo was removed."),
    (3, 3, "The group photo was changed."),
    (3, 4, "The chat background was changed."),
    (3, 5, "A chat background was set which may be sensitive."),
    (3, 6, "The chat background was removed."),
    (3, 7, "The chat background was changed."),
];

/// Group event at `SAMPLE_DATE`, marked as unattributed in `message_summary_info`.
fn unattributed(item_type: i32, group_action_type: i32) -> Message {
    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.handle_id = Some(0);
    message.item_type = item_type;
    message.group_action_type = group_action_type;
    message.other_handle = Some(1);
    message.group_title = Some("Hello world".to_string());
    message.is_unattributed = true;
    message
}

#[test]
fn can_format_html_unattributed_group_events() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = HTML::new(&config).unwrap();

    for (item_type, action, text) in UNATTRIBUTED {
        let mut actual = String::new();
        exporter.format_announcement(&unattributed(item_type, action), &mut actual);
        let expected = format!(
            "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> {text}</p>\n</div>\n"
        );

        assert_eq!(actual, expected, "({item_type}, {action})");
    }
}

#[test]
fn can_format_html_unattributed_participant_left() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = unattributed(3, 0);
    message.is_from_me = true;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You left the conversation.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_unattributed_name_removed() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut message = unattributed(2, 0);
    message.group_title = None;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> The conversation name was removed.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_unknown_group_event() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 8;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Unknown group event (item_type 3, group_action_type 8).</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_unknown_location_event() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    for (status, direction, shown) in [(2, None, "NULL"), (0, Some(2), "2")] {
        let mut message = Config::fake_message();
        message.date = SAMPLE_DATE;
        message.is_from_me = true;
        message.item_type = 4;
        message.share_status = status;
        message.share_direction = direction;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = format!(
            "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Unknown location sharing event (share_status {status}, share_direction {shown}).</p>\n</div>\n"
        );

        assert_eq!(actual, expected, "({status}, {direction:?})");
    }
}

#[test]
fn can_format_html_unknown_item_type() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 9;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Unknown event (item_type 9).</p>\n</div>\n";

    assert_eq!(actual, expected);
}

/// Group photo removal by the database owner at `SAMPLE_DATE` with `error` set.
fn group_icon_removed(error: i64) -> Message {
    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 2;
    message.error = error;
    message
}

#[test]
fn can_format_html_failed_group_event() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut actual = String::new();
    exporter.format_announcement(&group_icon_removed(1), &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM (Failed)</span> You removed the group photo.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_failed_unattributed_group_event() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = unattributed(3, 2);
    message.error = 1;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM (Failed)</span> The group photo was removed.</p>\n</div>\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_unfailed_group_event() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    for error in [0, 1 << 32] {
        let mut actual = String::new();
        exporter.format_announcement(&group_icon_removed(error), &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You removed the group photo.</p>\n</div>\n";

        assert_eq!(actual, expected, "{error}");
    }
}
