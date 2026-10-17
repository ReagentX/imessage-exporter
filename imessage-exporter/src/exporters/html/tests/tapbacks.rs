use crate::{
    Config, HTML,
    app::export_type::ExportType,
    exporters::{
        formatter::MessageFormatter,
        shared::test_support::{SAMPLE_DATE, config, with_contact, with_me},
    },
};

use imessage_database::util::dirs::home;

#[test]
fn can_format_html_tapback_me() {
    let mut config = config(ExportType::Html);
    with_me(&mut config);

    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(2000);
    message.associated_message_guid = Some("fake_guid".to_string());

    let actual = exporter.format_tapback(&message).unwrap();
    let expected = "<span class=\"tapback\"><b>Loved</b> by Me</span>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_tapback_them() {
    let mut config = config(ExportType::Html);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(2000);
    message.associated_message_guid = Some("fake_guid".to_string());
    message.handle_id = Some(999999);

    let actual = exporter.format_tapback(&message).unwrap();
    let expected = "<span class=\"tapback\"><b>Loved</b> by Sample Contact</span>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_tapback_custom_emoji() {
    let mut config = config(ExportType::Html);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(2006);
    message.associated_message_guid = Some("fake_guid".to_string());
    message.handle_id = Some(999999);
    message.associated_message_emoji = Some("☕️".to_string());

    let actual = exporter.format_tapback(&message).unwrap();
    // The result contains `&nbsp;`
    let expected = "<span class=\"tapback\"><b>☕\u{fe0f}</b> by Sample Contact</span>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_tapback_custom_sticker() {
    let mut config = config(ExportType::Html);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(2007);
    message.associated_message_guid = Some("fake_guid".to_string());
    message.handle_id = Some(999999);
    message.num_attachments = 1;

    let actual = exporter.format_tapback(&message).unwrap();
    let expected = "<span class=\"tapback\">Sticker from Sample Contact not found!</span>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_tapback_custom_sticker_exists() {
    let mut config = config(ExportType::Html);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = HTML::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(2007);
    message.associated_message_guid = Some("fake_guid".to_string());
    message.handle_id = Some(999999);
    message.num_attachments = 1;
    message.rowid = 452567;

    let actual = exporter.format_tapback(&message).unwrap();
    let expected = format!(
        "<img src=\"{}/Library/Messages/StickerCache/8e682c381ab52ec2-289D9E83-33EE-4153-AF13-43DB31792C6F/289D9E83-33EE-4153-AF13-43DB31792C6F.heic\" loading=\"lazy\"><div class=\"sticker_name\">App: Free People</div><div class=\"sticker_tapback\">&nbsp;by Sample Contact</div>",
        home()
    );

    assert_eq!(actual, expected);
}
