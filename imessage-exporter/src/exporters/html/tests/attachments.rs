use std::{env::current_dir, path::PathBuf};

use crate::{
    Config, HTML, Options,
    app::{compatibility::attachment_manager::AttachmentManagerMode, export_type::ExportType},
    exporters::{
        formatter::{AttachmentRender, MessageFormatter},
        shared::{
            attachment::ExportFile,
            test_support::{config, export_file},
        },
    },
};

use imessage_database::{
    tables::{attachment::MediaType, messages::models::AttachmentMeta},
    util::platform::Platform,
};

#[test]
fn can_format_html_attachment_macos() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let message = Config::fake_message();

    let attachment = Config::fake_attachment();

    let actual = exporter.format_attachment(&attachment, &message, &AttachmentMeta::default());

    assert_eq!(
        actual,
        AttachmentRender::Embedded("<img src=\"a/b/c/d.jpg\" loading=\"lazy\">".to_string())
    );
}

#[test]
fn can_format_html_attachment_macos_invalid_disabled() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    attachment.filename = None;
    attachment.transfer_name = None;

    let actual = exporter.format_attachment(&attachment, &message, &AttachmentMeta::default());

    assert_eq!(actual, AttachmentRender::MissingFilename);
}

#[test]
fn can_format_html_attachment_macos_invalid_clone() {
    let mut options = Options::fake_options(ExportType::Html);
    options.attachment_manager.mode = AttachmentManagerMode::Clone;

    let config = Config::fake_app(options);
    let exporter = HTML::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    attachment.filename = None;
    attachment.transfer_name = None;

    let actual = exporter.format_attachment(&attachment, &message, &AttachmentMeta::default());

    assert_eq!(actual, AttachmentRender::MissingFilename);
}

#[test]
fn can_format_html_attachment_ios() {
    let mut config = config(ExportType::Html);
    config.options.no_lazy = true;
    config.options.platform = Platform::iOS;
    let exporter = HTML::new(&config).unwrap();
    let message = Config::fake_message();

    let attachment = Config::fake_attachment();

    let AttachmentRender::Embedded(actual) =
        exporter.format_attachment(&attachment, &message, &AttachmentMeta::default())
    else {
        panic!("expected AttachmentRender::Embedded");
    };

    assert!(actual.ends_with("33/33c81da8ae3194fc5a0ea993ef6ffe0b048baedb\">"));
}

#[test]
fn can_format_html_attachment_folder() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut attachment = Config::fake_attachment();
    let folder_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/");
    attachment.mime_type = None;
    attachment.transfer_name = Some("test_data".to_string());
    let file = ExportFile {
        path: folder_path,
        media_type: MediaType::Unknown,
        name: "test_data",
    };

    let actual = exporter.render_attachment(&attachment, &file, &AttachmentMeta::default());

    let abs_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/");
    let expected = format!(
        "<p>\n    Folder: <i>test_data</i> (100.00 B)\n    <a href=\"{}\">Click to open</a>\n</p>",
        abs_path.display()
    );

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_attachment_text_download() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    // text/* → MediaType::Text(_) → AttachmentVariant::Download
    attachment.mime_type = Some("text/plain".to_string());
    attachment.filename = Some("notes.txt".to_string());
    attachment.transfer_name = Some("notes.txt".to_string());

    let actual = exporter.format_attachment(&attachment, &message, &AttachmentMeta::default());

    assert_eq!(
        actual,
        AttachmentRender::Embedded(
            "<a href=\"notes.txt\">Click to download notes.txt (100.00 B)</a>".to_string()
        )
    );
}

#[test]
fn can_format_html_attachment_application_download() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    // application/* → MediaType::Application(_) → AttachmentVariant::Download
    attachment.mime_type = Some("application/pdf".to_string());
    attachment.filename = Some("doc.pdf".to_string());
    attachment.transfer_name = Some("doc.pdf".to_string());

    let actual = exporter.format_attachment(&attachment, &message, &AttachmentMeta::default());

    assert_eq!(
        actual,
        AttachmentRender::Embedded(
            "<a href=\"doc.pdf\">Click to download doc.pdf (100.00 B)</a>".to_string()
        )
    );
}

#[test]
fn can_format_html_attachment_other_media_type() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let message = Config::fake_message();

    let mut attachment = Config::fake_attachment();
    // mime_type without a recognized prefix maps to MediaType::Other(full).
    attachment.mime_type = Some("model/gltf-binary".to_string());
    attachment.filename = Some("scene.glb".to_string());

    let actual = exporter.format_attachment(&attachment, &message, &AttachmentMeta::default());

    assert_eq!(
        actual,
        AttachmentRender::Embedded(
            "<p>Unable to embed model/gltf-binary attachments: scene.glb</p>".to_string()
        )
    );
}

#[test]
fn can_format_html_attachment_unknown() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut attachment = Config::fake_attachment();
    attachment.mime_type = None;
    attachment.transfer_name = Some("test_data".to_string());
    let file = ExportFile {
        path: PathBuf::from("Fake"),
        media_type: MediaType::Unknown,
        name: "test_data",
    };

    let actual = exporter.render_attachment(&attachment, &file, &AttachmentMeta::default());

    assert_eq!(
        actual,
        "<p>Unknown attachment type: Fake</p>\n<a href=\"Fake\">Download (100.00 B)</a>"
    );
}

#[test]
fn can_format_html_attachment_sticker() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut attachment = Config::fake_attachment();
    attachment.rowid = 3;
    attachment.is_sticker = true;
    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    attachment.filename = Some(sticker_path.to_string_lossy().to_string());
    let file = export_file(
        &config,
        &attachment,
        "imessage-database/test_data/stickers/outline.heic",
    );

    let actual = exporter.render_sticker(&attachment, &file);

    assert_eq!(
        actual,
        "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\"><div class=\"sticker_effect\">Sent with Outline effect</div>"
    );
}

#[test]
fn can_format_html_attachment_sticker_genmoji() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut attachment = Config::fake_attachment();
    attachment.rowid = 2;
    attachment.is_sticker = true;
    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    attachment.filename = Some(sticker_path.to_string_lossy().to_string());
    attachment.emoji_description = Some("pink poodle".to_string());
    let file = export_file(
        &config,
        &attachment,
        "imessage-database/test_data/stickers/outline.heic",
    );

    let actual = exporter.render_sticker(&attachment, &file);

    assert_eq!(
        actual,
        "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\"><div class=\"genmoji_prompt\">Genmoji prompt: pink poodle</div>"
    );
}

#[test]
fn can_format_html_attachment_sticker_app() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut attachment = Config::fake_attachment();
    attachment.rowid = 1;
    attachment.is_sticker = true;
    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    attachment.filename = Some(sticker_path.to_string_lossy().to_string());
    let file = export_file(
        &config,
        &attachment,
        "imessage-database/test_data/stickers/outline.heic",
    );

    let actual = exporter.render_sticker(&attachment, &file);

    assert_eq!(
        actual,
        "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\"><div class=\"sticker_name\">App: Free People</div>"
    );
}

#[test]
fn format_sticker_inline_renders_with_effect_title() {
    // Inline static stickers emit a bare `<img class="inline_sticker">`
    // with the decoration text carried on `title=` (rather than the
    // block-style `<div class="sticker_effect">`).
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut attachment = Config::fake_attachment();
    attachment.rowid = 3;
    attachment.is_sticker = true;
    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    attachment.filename = Some(sticker_path.to_string_lossy().to_string());
    let file = export_file(
        &config,
        &attachment,
        "imessage-database/test_data/stickers/outline.heic",
    );

    let actual = exporter.render_sticker_inline(&attachment, &file);
    let expected = "<img class=\"inline_sticker\" src=\"imessage-database/test_data/stickers/outline.heic\" alt=\"Sent with Outline effect\" title=\"Sent with Outline effect\" loading=\"lazy\">";
    assert_eq!(actual, expected);
}

#[test]
fn format_sticker_inline_renders_with_genmoji_title() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut attachment = Config::fake_attachment();
    attachment.rowid = 2;
    attachment.is_sticker = true;
    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    attachment.filename = Some(sticker_path.to_string_lossy().to_string());
    attachment.emoji_description = Some("pink poodle".to_string());
    let file = export_file(
        &config,
        &attachment,
        "imessage-database/test_data/stickers/outline.heic",
    );

    let actual = exporter.render_sticker_inline(&attachment, &file);
    let expected = "<img class=\"inline_sticker\" src=\"imessage-database/test_data/stickers/outline.heic\" alt=\"Genmoji prompt: pink poodle\" title=\"Genmoji prompt: pink poodle\" loading=\"lazy\">";
    assert_eq!(actual, expected);
}

#[test]
fn format_sticker_inline_renders_with_app_title() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut attachment = Config::fake_attachment();
    attachment.rowid = 1;
    attachment.is_sticker = true;
    let sticker_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/stickers/outline.heic");
    attachment.filename = Some(sticker_path.to_string_lossy().to_string());
    let file = export_file(
        &config,
        &attachment,
        "imessage-database/test_data/stickers/outline.heic",
    );

    let actual = exporter.render_sticker_inline(&attachment, &file);
    let expected = "<img class=\"inline_sticker\" src=\"imessage-database/test_data/stickers/outline.heic\" alt=\"App: Free People\" title=\"App: Free People\" loading=\"lazy\">";
    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_attachment_audio_transcript() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

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

    let actual = exporter.format_attachment(&attachment, &message, &meta);

    assert_eq!(
        actual,
        AttachmentRender::Embedded(
            "<div>\n    <audio controls src=\"Audio Message.caf\" type=\"x-caf; codecs=opus\"> </audio>\n</div>\n<hr>\n<span class=\"transcription\">Transcription: Test</span>".to_string()
        )
    );
}
