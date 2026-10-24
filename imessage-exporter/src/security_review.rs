use crate::app::{
    compatibility::attachment_manager::{AttachmentManager, AttachmentManagerMode},
    export_type::ExportType,
    test_dir::unique_test_dir,
};
use crate::exporters::formatter::{BalloonFormatter, TextEffectFormatter};
use crate::{Config, HTML, Options};
use imessage_database::message_types::url::URLMessage;
fn config() -> Config {
    let mut options = Options::fake_options(ExportType::Html);
    options.contacts_path = Some(options.export_path.join("no-contacts.db"));
    options.attachment_manager = AttachmentManager::from(AttachmentManagerMode::Clone);
    Config::fake_app(options)
}
#[test]
fn security_review_rejects_active_link() {
    let c = config();
    let h = HTML::new(&c).unwrap();
    let output = h.format_link("click", "javascript:alert(1)");
    assert_eq!(output, "click");
}
#[test]
fn security_review_omits_remote_preview() {
    let c = config();
    let h = HTML::new(&c).unwrap();
    let b = URLMessage {
        url: Some("https://example.invalid"),
        images: vec!["https://example.invalid/tracker.png"],
        ..Default::default()
    };
    let output = h.format_url(&Config::fake_message(), &b);
    assert!(!output.contains("src="));
}
#[test]
fn security_review_failed_copy_returns_error() {
    let c = config();
    let d = unique_test_dir("security-copy");
    let source = d.join("source.bin");
    std::fs::write(&source, b"synthetic").unwrap();
    // Export's attachment root is a regular file, so copying must fail.
    std::fs::write(c.attachment_path(), b"block directory creation").unwrap();
    let mut a = Config::fake_attachment();
    a.filename = Some(source.to_string_lossy().into());
    a.rowid = 7;
    let m = Config::fake_message();
    let manager = &c.options.attachment_manager;
    assert!(manager.plan(&m, &a, &c).is_err());
}
#[test]
fn security_review_repairs_corrupt_copy() {
    let c = config();
    let d = unique_test_dir("security-corrupt");
    let source = d.join("source.bin");
    std::fs::write(&source, b"complete source").unwrap();
    let mut a = Config::fake_attachment();
    a.filename = Some(source.to_string_lossy().into());
    a.rowid = 7;
    let m = Config::fake_message();
    let manager = &c.options.attachment_manager;
    let dir = c
        .attachment_path()
        .join(c.conversation_attachment_path(m.chat_rowid()));
    std::fs::create_dir_all(&dir).unwrap();
    let destination = dir.join("7.bin");
    std::fs::write(&destination, b"").unwrap();
    let plan = manager.plan(&m, &a, &c).unwrap();
    let result = manager.execute(plan, &m, &a, &c).unwrap().unwrap();
    assert_eq!(std::fs::read(result.path).unwrap(), b"complete source");
}
fn handwriting_with_id(
    id: &str,
) -> imessage_database::message_types::handwriting::HandwrittenMessage {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let mut payload =
        std::fs::read(root.join("imessage-database/test_data/handwritten_message/handwriting.bin"))
            .unwrap();
    // Protobuf field 3 is the string ID. Last occurrence wins.
    payload.push(0x1a);
    let mut len = id.len();
    while len >= 128 {
        payload.push(((len & 127) as u8) | 128);
        len >>= 7;
    }
    payload.push(len as u8);
    payload.extend_from_slice(id.as_bytes());
    imessage_database::message_types::handwriting::HandwrittenMessage::from_payload(&payload)
        .unwrap()
}
#[test]
fn security_review_escapes_handwriting() {
    let id =
        "</title><script>document.documentElement.dataset.securityReview='yes'</script><title>";
    let handwriting = handwriting_with_id(id);
    let c = config();
    let h = HTML::new(&c).unwrap();
    let output = h.format_handwriting(&Config::fake_message(), &handwriting);
    assert!(!output.contains("<script>"));
    assert!(output.contains("&lt;script&gt;"));
}
#[test]
fn security_review_contains_handwriting_path() {
    let c = config();
    let outside = unique_test_dir("security-outside").join("proof.svg");
    assert!(!outside.starts_with(&c.options.export_path));
    let handwriting = handwriting_with_id(outside.to_str().unwrap());
    let actual = c
        .options
        .attachment_manager
        .handle_handwriting(&Config::fake_message(), &handwriting, &c)
        .unwrap()
        .unwrap();
    assert!(actual.starts_with(&c.options.export_path));
    assert!(actual.is_file());
    assert!(!outside.exists());
}

#[test]
fn security_review_compares_contents_not_just_length() {
    use crate::app::compatibility::converters::common::files_equal;
    let d = unique_test_dir("equal-length");
    let a = d.join("a");
    let b = d.join("b");
    std::fs::write(&a, b"abcd").unwrap();
    std::fs::write(&b, b"abce").unwrap();
    assert!(!files_equal(&a, &b).unwrap());
    std::fs::write(&b, b"abcd").unwrap();
    assert!(files_equal(&a, &b).unwrap());
}
#[test]
fn security_review_compares_directory_trees() {
    use crate::app::compatibility::converters::common::{copy_raw, files_equal};
    let d = unique_test_dir("trees");
    let a = d.join("a");
    let b = d.join("b");
    std::fs::create_dir_all(a.join("nested")).unwrap();
    std::fs::write(a.join("nested/data"), b"original").unwrap();
    copy_raw(&a, &b).unwrap();
    assert!(files_equal(&a, &b).unwrap());
    std::fs::write(b.join("extra"), b"stale").unwrap();
    assert!(!files_equal(&a, &b).unwrap());
}
#[cfg(unix)]
#[test]
fn security_review_rejects_symlink_output_directory() {
    let c = config();
    let outside = unique_test_dir("symlink-target");
    std::os::unix::fs::symlink(&outside, c.attachment_path()).unwrap();
    let h = handwriting_with_id("ordinary-id");
    assert!(
        c.options
            .attachment_manager
            .handle_handwriting(&Config::fake_message(), &h, &c)
            .is_err()
    );
    assert_eq!(std::fs::read_dir(outside).unwrap().count(), 0);
}
#[test]
fn security_review_link_scheme_variants() {
    use crate::app::sanitizers::safe_link;
    for bad in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "data:text/html,hi",
        "file:///tmp/a",
        "java\nscript:hi",
        " https://example.com",
        "//example.com/a",
    ] {
        assert!(safe_link(bad).is_none(), "{bad}");
    }
    for good in [
        "https://example.com/?a=1&b=2",
        "HTTP://example.com",
        "mailto:test@example.com",
        "tel:+15551234567",
    ] {
        assert_eq!(safe_link(good), Some(good));
    }
}
