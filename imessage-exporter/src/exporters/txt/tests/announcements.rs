use crate::{
    Config, Options, TXT,
    app::{contacts::Name, export_type::ExportType},
    exporters::{
        formatter::MessageFormatter,
        shared::test_support::{SAMPLE_DATE, config, with_contact, with_me},
    },
};

use imessage_database::tables::{messages::Message, table::ME};

#[test]
fn can_format_txt_announcement() {
    let mut config = config(ExportType::Txt);
    config.participants.insert(0, Name::fake_name(ME));

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 2;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You named the conversation Hello world\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_announcement_custom_name() {
    let mut options = Options::fake_options(ExportType::Txt);
    options.custom_name = Some("Name".to_string());
    let mut config = Config::fake_app(options);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.item_type = 2;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM Name named the conversation Hello world\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_group_name_removed() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 2;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You removed the name from the conversation.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_announcement_unknown() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "Unable to format announcement!\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_group_removed() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 1;
    message.group_action_type = 1;
    message.other_handle = Some(1);

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You removed Other from the conversation.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_group_removed_other() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");
    with_contact(&mut config, 2, "Second");

    let exporter = TXT::new(&config).unwrap();

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
    let expected = "May 17, 2022  5:29:42 PM Other removed Second from the conversation.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_group_changed_number() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = TXT::new(&config).unwrap();

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
    let expected = "May 17, 2022  5:29:42 PM Other changed their phone number.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_group_added() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 1;
    message.group_action_type = 0;
    message.other_handle = Some(1);

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You added Other to the conversation.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_group_left() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 3;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You left the conversation.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_group_icon_removed() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 2;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You removed the group photo.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_group_icon_added() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 1;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You changed the group photo.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_chat_background_removed() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 6;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You removed the chat background.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_chat_background_added() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.group_title = Some("Hello world".to_string());
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 4;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You changed the chat background.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_chat_background_sensitive() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 5;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You set a chat background which may be sensitive.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_group_icon_added_hidden() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 3;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You changed the group photo.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_chat_background_added_hidden() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 7;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You changed the chat background.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_audio_message_kept() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 5;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You kept an audio message.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_other_kept_messages() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

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
        let expected = format!("May 17, 2022  5:29:42 PM You {wording}\n\n");

        assert_eq!(actual, expected, "{code}");
    }
}

#[test]
fn can_format_txt_transcript_sharing_stopped() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.handle_id = Some(1);
    message.item_type = 7;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM Other stopped sharing Emergency SOS transcript of this conversation with you.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_emergency_location_sent() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = TXT::new(&config).unwrap();

    for is_from_me in [false, true] {
        let mut message = Config::fake_message();
        message.date = SAMPLE_DATE;
        message.handle_id = Some(1);
        message.is_from_me = is_from_me;
        message.item_type = 8;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "May 17, 2022  5:29:42 PM Location sent to emergency services.\n\n";

        assert_eq!(actual, expected, "{is_from_me}");
    }
}

/// Group events with actorless forms and nil senders in Messages, paired
/// with the expected text for each event.
const UNATTRIBUTED: [(i32, i32, &str); 10] = [
    (1, 0, "Other was added to the conversation."),
    (1, 1, "Other was removed from the conversation."),
    (2, 0, "The conversation was named Hello world"),
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
fn can_format_txt_unattributed_group_events() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);
    with_contact(&mut config, 1, "Other");

    let exporter = TXT::new(&config).unwrap();

    for (item_type, action, text) in UNATTRIBUTED {
        let mut actual = String::new();
        exporter.format_announcement(&unattributed(item_type, action), &mut actual);
        let expected = format!("May 17, 2022  5:29:42 PM {text}\n\n");

        assert_eq!(actual, expected, "({item_type}, {action})");
    }
}

#[test]
fn can_format_txt_unattributed_participant_left() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = unattributed(3, 0);
    message.is_from_me = true;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM You left the conversation.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_unattributed_name_removed() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let mut message = unattributed(2, 0);
    message.group_title = None;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM The conversation name was removed.\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_unknown_group_event() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 3;
    message.group_action_type = 8;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected =
        "May 17, 2022  5:29:42 PM Unknown group event (item_type 3, group_action_type 8).\n\n";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_unknown_location_event() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

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
            "May 17, 2022  5:29:42 PM Unknown location sharing event (share_status {status}, share_direction {shown}).\n\n"
        );

        assert_eq!(actual, expected, "({status}, {direction:?})");
    }
}

#[test]
fn can_format_txt_unknown_item_type() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.item_type = 9;

    let mut actual = String::new();
    exporter.format_announcement(&message, &mut actual);
    let expected = "May 17, 2022  5:29:42 PM Unknown event (item_type 9).\n\n";

    assert_eq!(actual, expected);
}
