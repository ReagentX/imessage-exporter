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

/// Every group event with a form without an actor, as Messages writes it with
/// a nil sender, paired with its rendered text.
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

/// Group event at `SAMPLE_DATE` that `message_summary_info` marks unattributed.
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
