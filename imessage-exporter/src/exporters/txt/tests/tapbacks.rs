use crate::{
    Config, TXT,
    app::export_type::ExportType,
    exporters::{
        formatter::MessageFormatter,
        shared::test_support::{SAMPLE_DATE, config, with_contact, with_me},
    },
};

use imessage_database::util::dirs::home;

#[test]
fn can_format_txt_tapback_me() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(2000);
    message.associated_message_guid = Some("fake_guid".to_string());

    let actual = exporter.format_tapback(&message).unwrap();
    let expected = "Loved by Me";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_tapback_them() {
    let mut config = config(ExportType::Txt);
    with_contact(&mut config, 999999, "Sample Contact");

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(2000);
    message.associated_message_guid = Some("fake_guid".to_string());
    message.handle_id = Some(999999);

    let actual = exporter.format_tapback(&message).unwrap();
    let expected = "Loved by Sample Contact";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_tapback_custom_emoji() {
    let mut config = config(ExportType::Txt);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(2006);
    message.associated_message_guid = Some("fake_guid".to_string());
    message.handle_id = Some(999999);
    message.associated_message_emoji = Some("☕️".to_string());

    let actual = exporter.format_tapback(&message).unwrap();
    let expected = "☕️ by Sample Contact";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_tapback_custom_sticker() {
    let mut config = config(ExportType::Txt);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(2007);
    message.associated_message_guid = Some("fake_guid".to_string());
    message.handle_id = Some(999999);
    message.num_attachments = 1;

    let actual = exporter.format_tapback(&message).unwrap();
    let expected = "Sticker from Sample Contact not found!";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_tapback_custom_sticker_exists() {
    let mut config = config(ExportType::Txt);
    with_contact(&mut config, 999999, "Sample Contact");

    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(2007);
    message.associated_message_guid = Some("fake_guid".to_string());
    message.handle_id = Some(999999);
    message.num_attachments = 1;
    message.rowid = 452567;

    let actual = exporter.format_tapback(&message).unwrap();
    let expected = format!(
        "Sticker from Sample Contact: {}/Library/Messages/StickerCache/8e682c381ab52ec2-289D9E83-33EE-4153-AF13-43DB31792C6F/289D9E83-33EE-4153-AF13-43DB31792C6F.heic (App: Free People) from Sample Contact",
        home()
    );

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_tapback_custom_sticker_removed() {
    let mut config = config(ExportType::Txt);
    with_contact(&mut config, 999999, "Sample Contact");
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.date = SAMPLE_DATE;
    message.associated_message_type = Some(3007);
    message.associated_message_guid = Some("fake_guid".to_string());
    message.handle_id = Some(999999);
    message.num_attachments = 1;
    message.rowid = 452567;

    let actual = exporter.format_tapback(&message).unwrap();
    let expected = "";

    assert_eq!(actual, expected);
}

#[test]
fn tapbacks_render_with_inner_inset_only() {
    // The 4-space inset on each tapback line is a visual cue for what's
    // being reacted to. The outer reply indent is applied by
    // `push_indented`, not by the template.
    use crate::exporters::txt::view_model::TapbacksVM;
    use askama::Template;

    let vm = TapbacksVM {
        tapbacks: vec!["Loved by Me".to_string(), "Liked by Sample".to_string()],
    };

    let rendered = vm.render().unwrap();
    assert_eq!(
        rendered,
        "Tapbacks:\n    Loved by Me\n    Liked by Sample\n",
    );
}

#[test]
fn push_indented_stacks_on_tapback_inner_inset() {
    // When the reply indent is stacked on top of `tapbacks.txt`'s inner
    // 4-space inset, the resulting tapback lines sit at 8 spaces.
    use crate::exporters::txt::view_model::TapbacksVM;
    use askama::Template;

    let vm = TapbacksVM {
        tapbacks: vec!["Loved by Me".to_string(), "Liked by Sample".to_string()],
    };
    let rendered = vm.render().unwrap();
    let mut indented = String::new();
    TXT::push_indented(&mut indented, &rendered, "    ");
    assert_eq!(
        indented,
        "    Tapbacks:\n        Loved by Me\n        Liked by Sample\n",
    );
}
