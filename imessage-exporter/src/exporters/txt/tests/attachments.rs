use std::env::current_dir;

use crate::{
    Config, Options, TXT,
    app::{compatibility::attachment_manager::AttachmentManagerMode, export_type::ExportType},
    exporters::{
        formatter::{AttachmentRender, MessageFormatter},
        shared::test_support::{config, with_me},
    },
};

use imessage_database::{
    message_types::text_effects::text_effect::TextEffect,
    tables::messages::models::{AttachmentMeta, AttributedRange, BubbleComponent},
    util::platform::Platform,
};

#[test]
fn can_format_txt_attachment_macos() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();

    let actual = exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

    assert_eq!(
        actual,
        AttachmentRender::Embedded("a/b/c/d.jpg".to_string())
    );
}

#[test]
fn can_format_txt_attachment_macos_invalid_disabled() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    attachment.filename = None;
    attachment.transfer_name = None;

    let actual = exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

    assert_eq!(actual, AttachmentRender::MissingFilename);
}

#[test]
fn can_format_txt_attachment_macos_invalid_clone() {
    let mut options = Options::fake_options(ExportType::Txt);
    options.attachment_manager.mode = AttachmentManagerMode::Clone;

    let config = Config::fake_app(options);
    let exporter = TXT::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    attachment.filename = None;
    attachment.transfer_name = None;

    let actual = exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

    assert_eq!(actual, AttachmentRender::MissingFilename);
}

#[test]
fn can_format_txt_attachment_ios() {
    let mut config = config(ExportType::Txt);
    config.options.platform = Platform::iOS;
    let exporter = TXT::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();

    let AttachmentRender::Embedded(actual) =
        exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default())
    else {
        panic!("expected AttachmentRender::Embedded");
    };

    assert!(actual.ends_with("33/33c81da8ae3194fc5a0ea993ef6ffe0b048baedb"));
}

#[test]
fn can_format_txt_attachment_ios_invalid_clone() {
    let mut options = Options::fake_options(ExportType::Txt);
    options.attachment_manager.mode = AttachmentManagerMode::Clone;

    let mut config = Config::fake_app(options);
    config.options.platform = Platform::iOS;

    let exporter = TXT::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    attachment.filename = None;
    attachment.transfer_name = None;

    let actual = exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

    assert_eq!(actual, AttachmentRender::MissingFilename);
}

#[test]
fn can_format_txt_attachment_sticker() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    attachment.rowid = 3;
    attachment.is_sticker = true;
    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    attachment.filename = Some(sticker_path.to_string_lossy().to_string());
    attachment.copied_path = Some(
        config
            .options
            .export_path
            .join("imessage-database/test_data/stickers/outline.heic"),
    );

    let actual = exporter.format_sticker(&mut attachment, &message);

    assert_eq!(
        actual,
        "Outline Sticker from Me: imessage-database/test_data/stickers/outline.heic"
    );
}

#[test]
fn can_format_txt_attachment_sticker_genmoji() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    attachment.rowid = 2;
    attachment.is_sticker = true;
    attachment.emoji_description = Some("Example description".to_string());
    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    attachment.filename = Some(sticker_path.to_string_lossy().to_string());
    attachment.copied_path = Some(
        config
            .options
            .export_path
            .join("imessage-database/test_data/stickers/outline.heic"),
    );

    let actual = exporter.format_sticker(&mut attachment, &message);

    assert_eq!(
        actual,
        "Sticker from Me: imessage-database/test_data/stickers/outline.heic (Genmoji prompt: Example description)"
    );
}

#[test]
fn can_format_txt_attachment_sticker_app() {
    let mut config = config(ExportType::Txt);
    with_me(&mut config);

    let exporter = TXT::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    attachment.rowid = 1;
    attachment.is_sticker = true;
    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    attachment.filename = Some(sticker_path.to_string_lossy().to_string());
    attachment.copied_path = Some(
        config
            .options
            .export_path
            .join("imessage-database/test_data/stickers/outline.heic"),
    );

    let actual = exporter.format_sticker(&mut attachment, &message);

    assert_eq!(
        actual,
        "Sticker from Me: imessage-database/test_data/stickers/outline.heic (App: Free People)"
    );
}

#[test]
fn dispatch_part_body_advances_index_between_stickers() {
    // Regression: a message with two stickers must render each sticker
    // at its own slot. Prior to the fix, the sticker arm of
    // `dispatch_part_body` returned without advancing `attachment_index`,
    // so both parts re-resolved `attachments[0]` and the first sticker
    // rendered at every slot.
    use crate::exporters::{
        shared::part::{AttachmentResolver, dispatch_part_body},
        txt::view_model::PartBody,
    };

    let mut config = config(ExportType::Txt);
    with_me(&mut config);
    let exporter = TXT::new(&config).unwrap();
    let message = Config::fake_message();

    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");

    // rowid 3 → "Outline Sticker from Me: …"
    let mut sticker_a = Config::fake_attachment();
    sticker_a.rowid = 3;
    sticker_a.is_sticker = true;
    sticker_a.filename = Some(sticker_path.to_string_lossy().to_string());
    sticker_a.copied_path = Some(
        config
            .options
            .export_path
            .join("imessage-database/test_data/stickers/outline.heic"),
    );

    // rowid 1 → "Sticker from Me: … (App: Free People)"
    let mut sticker_b = Config::fake_attachment();
    sticker_b.rowid = 1;
    sticker_b.is_sticker = true;
    sticker_b.filename = Some(sticker_path.to_string_lossy().to_string());
    sticker_b.copied_path = Some(
        config
            .options
            .export_path
            .join("imessage-database/test_data/stickers/outline.heic"),
    );

    let mut attachments = vec![sticker_a, sticker_b];
    let part = BubbleComponent::Run(vec![AttributedRange::attachment(
        0,
        3,
        AttachmentMeta::default(),
    )]);
    let mut resolver = AttachmentResolver::new(&attachments);

    let first = dispatch_part_body(
        &exporter,
        &message,
        0,
        &part,
        &mut attachments,
        &mut resolver,
    );
    let PartBody::Line { text: first_text } = first else {
        panic!("expected PartBody::Line for sticker arm");
    };
    let expected_first =
        "Outline Sticker from Me: imessage-database/test_data/stickers/outline.heic";
    assert_eq!(first_text, expected_first);

    let second = dispatch_part_body(
        &exporter,
        &message,
        1,
        &part,
        &mut attachments,
        &mut resolver,
    );
    let PartBody::Line { text: second_text } = second else {
        panic!("expected PartBody::Line for sticker arm");
    };
    let expected_second =
        "Sticker from Me: imessage-database/test_data/stickers/outline.heic (App: Free People)";
    assert_eq!(second_text, expected_second);
}

#[test]
fn translated_mixed_run_keeps_translation() {
    use crate::exporters::{
        shared::part::{AttachmentResolver, dispatch_part_body},
        txt::view_model::PartBody,
    };

    let mut config = config(ExportType::Txt);
    config
        .translated_messages
        .insert("56FE94B9-2345-4A3C-A57F-949BDDDDF9FF".to_string());
    with_me(&mut config);
    let exporter = TXT::new(&config).unwrap();

    let mut message = Config::fake_message();
    message.guid = "56FE94B9-2345-4A3C-A57F-949BDDDDF9FF".to_string();
    message.rowid = 548216; // row carrying the translation in test.db
    message.text = Some("Look \u{FFFC}".to_string());

    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    let mut sticker = Config::fake_attachment();
    sticker.rowid = 3;
    sticker.is_sticker = true;
    sticker.filename = Some(sticker_path.to_string_lossy().to_string());
    sticker.copied_path = Some(
        config
            .options
            .export_path
            .join("imessage-database/test_data/stickers/outline.heic"),
    );

    let mut attachments = vec![sticker];
    let part = BubbleComponent::Run(vec![
        AttributedRange::text(0, 5, vec![TextEffect::Default]),
        AttributedRange::inline_attachment(5, 8, AttachmentMeta::default()),
    ]);
    let mut resolver = AttachmentResolver::new(&attachments);

    let body = dispatch_part_body(
        &exporter,
        &message,
        0,
        &part,
        &mut attachments,
        &mut resolver,
    );
    let PartBody::Translated {
        translated,
        original,
    } = body
    else {
        panic!("a translated mixed run must produce PartBody::Translated");
    };
    assert_eq!(translated, "Oh, il a traduit ce que j'ai envoyé !");
    assert!(
        original.contains("Look"),
        "original text must survive: {original}"
    );
    assert!(
        original.contains("Sticker from Me"),
        "the sticker line must be present in the original: {original}"
    );
}

#[test]
fn dispatch_part_body_advances_index_after_sticker() {
    // Regression: when a sticker precedes a non-sticker attachment, the
    // sticker arm must advance `attachment_index` so the next part
    // resolves to the non-sticker attachment. Prior to the fix the
    // second part would re-resolve the sticker.
    use crate::exporters::{
        shared::part::{AttachmentResolver, dispatch_part_body},
        txt::view_model::PartBody,
    };

    let mut config = config(ExportType::Txt);
    with_me(&mut config);
    let exporter = TXT::new(&config).unwrap();
    let message = Config::fake_message();

    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");

    let mut sticker = Config::fake_attachment();
    sticker.rowid = 3;
    sticker.is_sticker = true;
    sticker.filename = Some(sticker_path.to_string_lossy().to_string());
    sticker.copied_path = Some(
        config
            .options
            .export_path
            .join("imessage-database/test_data/stickers/outline.heic"),
    );

    // Default `fake_attachment` renders as `AttachmentRender::Embedded("a/b/c/d.jpg")`.
    let image = Config::fake_attachment();

    let mut attachments = vec![sticker, image];
    let part = BubbleComponent::Run(vec![AttributedRange::attachment(
        0,
        3,
        AttachmentMeta::default(),
    )]);
    let mut resolver = AttachmentResolver::new(&attachments);

    let first = dispatch_part_body(
        &exporter,
        &message,
        0,
        &part,
        &mut attachments,
        &mut resolver,
    );
    let PartBody::Line { text: first_text } = first else {
        panic!("expected PartBody::Line for sticker arm");
    };
    let expected_first =
        "Outline Sticker from Me: imessage-database/test_data/stickers/outline.heic";
    assert_eq!(first_text, expected_first);

    let second = dispatch_part_body(
        &exporter,
        &message,
        1,
        &part,
        &mut attachments,
        &mut resolver,
    );
    let PartBody::Line { text: second_text } = second else {
        panic!("expected PartBody::Line for attachment arm");
    };
    let expected_second = "a/b/c/d.jpg";
    assert_eq!(second_text, expected_second);
}

#[test]
fn can_format_txt_attachment_audio_transcript() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    attachment.uti = Some("com.apple.coreaudio-format".to_string());
    attachment.transfer_name = Some("Audio Message.caf".to_string());
    attachment.filename = Some("Audio Message.caf".to_string());
    attachment.mime_type = None;

    let meta = AttachmentMeta {
        transcription: Some("Test".to_string()),
        ..Default::default()
    };

    let actual = exporter.format_attachment(&mut attachment, &message, &meta);

    assert_eq!(
        actual,
        AttachmentRender::Embedded("Audio Message.caf\nTranscription: Test".to_string())
    );
}
