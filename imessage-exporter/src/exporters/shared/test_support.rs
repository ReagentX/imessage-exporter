use imessage_database::tables::{attachment::Attachment, messages::Message, table::ME};

use crate::{
    Config, Options,
    app::{contacts::Name, export_type::ExportType},
    exporters::{
        formatter::{MessageFormatter, RenderContext},
        shared::attachment::ExportFile,
    },
};

/// May 17, 2022  8:29:42 PM
pub(crate) const SAMPLE_DATE: i64 = 674526582885055488;
/// May 17, 2022  9:30:31 PM
pub(crate) const SAMPLE_DATE_LATER: i64 = 674530231992568192;

/// Config for the given export type with no participants.
pub(crate) fn config(export_type: ExportType) -> Config {
    Config::fake_app(Options::fake_options(export_type))
}

/// Register the local user as participant 0.
pub(crate) fn with_me(config: &mut Config) {
    config.participants.insert(0, Name::fake_name(ME));
    config.real_participants.insert(0, 0);
}

/// Register a contact under `handle` with its own dedup id.
pub(crate) fn with_contact(config: &mut Config, handle: i32, name: &str) {
    config.participants.insert(handle, Name::fake_name(name));
    config.real_participants.insert(handle, handle);
}

/// A message from the local user at `SAMPLE_DATE` in chat 0 with one plain-text body component.
pub(crate) fn sent(text: &str) -> Message {
    let mut message = Config::fake_message_with_text(text);
    message.date = SAMPLE_DATE;
    message.is_from_me = true;
    message.chat_id = Some(0);
    message
}

/// A message from `handle` at `SAMPLE_DATE` with one plain-text body component.
pub(crate) fn received(text: &str, handle: i32) -> Message {
    let mut message = Config::fake_message_with_text(text);
    message.date = SAMPLE_DATE;
    message.handle_id = Some(handle);
    message
}

/// The file a render half would receive for `attachment` after a copy to
/// `relative` under the export root, with the row's own media type.
pub(crate) fn export_file<'a>(
    config: &Config,
    attachment: &'a Attachment,
    relative: &str,
) -> ExportFile<'a> {
    ExportFile {
        path: config.options.export_path.join(relative),
        media_type: attachment.mime_type(),
        name: attachment.filename().unwrap(),
    }
}

/// Render a top-level message to a string.
pub(crate) fn render<'a>(exporter: &impl MessageFormatter<'a>, message: &Message) -> String {
    let mut out = String::new();
    exporter
        .format_message_into(message, RenderContext::TopLevel, &mut out)
        .unwrap();
    out
}
