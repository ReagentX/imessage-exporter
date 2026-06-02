use std::{
    collections::{
        HashMap,
        hash_map::Entry::{Occupied, Vacant},
    },
    fs::File,
    io::{BufWriter, IsTerminal, Write, stderr},
};

use imessage_database::tables::{
    messages::Message,
    table::{ORPHANED, Table},
};
use rusqlite::Connection;

use crate::{
    app::{error::RuntimeError, progress::ExportProgress, runtime::Config},
    exporters::formatter::{MessageFormatter, RenderContext},
};

/// Capacity for each chat file's [`BufWriter`].
const FILE_BUFFER_CAPACITY: usize = 64 * 1024;

/// Shared per-export mutable state held by every concrete `MessageWriter`.
/// Holds the file cache (one [`BufWriter`] per chatroom), the writer for
/// messages that don't belong to a chat, and the progress bar. The owning
/// formatter struct adds only the `&'a Config` reference and the
/// format-specific hooks declared on [`MessageWriter`].
pub struct ExportState {
    /// One open [`BufWriter`] per resolved chat filename.
    pub files: HashMap<String, BufWriter<File>>,
    /// Cache each chat's resolved filename by chat rowid
    pub route: HashMap<i32, String>,
    /// Destination for messages that don't have a conversation route.
    pub orphaned: BufWriter<File>,
    /// Drives the on-screen progress indicator.
    pub pb: ExportProgress,
}

impl ExportState {
    /// Open the orphaned file (creating it if missing) under
    /// `config.options.export_path` with the supplied extension, then build
    /// the empty file cache and progress bar.
    pub fn new(config: &Config, extension: &str) -> Result<Self, RuntimeError> {
        let mut orphaned = config.options.export_path.clone();
        orphaned.push(ORPHANED);
        orphaned.set_extension(extension);
        let file = File::options().append(true).create(true).open(&orphaned)?;
        // `--no-progress` forces off; otherwise show only when stderr is a TTY
        // so headless invocations (CI, redirects to logfiles) stay clean.
        let pb_enabled = config.options.show_progress && stderr().is_terminal();
        Ok(Self {
            files: HashMap::new(),
            route: HashMap::new(),
            orphaned: BufWriter::with_capacity(FILE_BUFFER_CAPACITY, file),
            pb: ExportProgress::new(pb_enabled, config.progress_callback.clone()),
        })
    }
}

/// Decode the message's body via [`Message::parse_body`] and apply it.
/// `parse_body` failures are non-fatal: they leave the message's
/// `components` empty, which downstream formatters already treat as
/// "nothing to render".
pub fn apply_body(msg: &mut Message, db: &Connection) {
    if let Ok(body) = msg.parse_body(db) {
        msg.apply_body(body);
    }
}

/// Format-specific hooks consumed by [`run_export`] and
/// [`get_or_create_file_for`]. Implementers hold the [`&'a Config`] and an
/// [`ExportState`], plus a small set of format-specific constants and
/// header/footer hooks.
pub trait MessageWriter<'a>: MessageFormatter<'a> {
    /// Format label substituted into the `"Exporting to … as <label>…"`
    /// status line (e.g. the file-extension name of the output format).
    const LABEL: &'static str;
    /// Initial capacity for the per-message buffer reused across iterations.
    const BUFFER_CAPACITY: usize;

    /// Access the per-export config (route resolution, database, options).
    fn config(&self) -> &'a Config;

    /// Borrow the shared per-export mutable state.
    fn state(&self) -> &ExportState;

    /// Mutably borrow the shared per-export mutable state.
    fn state_mut(&mut self) -> &mut ExportState;

    /// Write a per-file header. Called once for the orphaned file at the
    /// start of [`run_export`] and once when each chat file is first opened
    /// (skipped if the file already exists on disk, to avoid duplicate
    /// headers on group-name collisions). Return `Ok(())` to emit nothing.
    fn write_file_header(file: &mut BufWriter<File>) -> Result<(), RuntimeError>;

    /// Write a per-file footer. Called for every cached chat file and the
    /// orphaned file after iteration ends. Return `Ok(())` to emit nothing.
    fn write_file_footer(file: &mut BufWriter<File>) -> Result<(), RuntimeError>;

    /// Optional notice printed once before per-file footers are written.
    /// Return `None` to suppress the notice.
    fn footer_notice() -> Option<&'static str>;
}

/// Resolve the `BufWriter` for `message`, creating the chat file (and writing
/// its header) on first sight. Messages without a conversation route to the
/// shared orphaned writer.
pub fn get_or_create_file_for<'a, 'b, W>(
    writer: &'b mut W,
    message: &Message,
) -> Result<&'b mut BufWriter<File>, RuntimeError>
where
    W: MessageWriter<'a>,
{
    let config = writer.config();
    match config.conversation(message) {
        Some((chatroom, _)) => {
            let chatroom_rowid = chatroom.rowid;
            let state = writer.state_mut();
            // Reuse the chat's filename if we've already resolved it; otherwise
            // compute it once and memoize. `config`/`chatroom` are `&'a` and
            // independent of the `state` borrow.
            let filename = match state.route.get(&chatroom_rowid) {
                Some(name) => name.clone(),
                None => {
                    let name = config.filename(chatroom);
                    state.route.insert(chatroom_rowid, name.clone());
                    name
                }
            };
            match state.files.entry(filename) {
                Occupied(entry) => Ok(entry.into_mut()),
                Vacant(entry) => {
                    let mut path = config.options.export_path.clone();
                    path.push(entry.key());
                    // If the file already exists, don't write the headers again.
                    // This can happen if multiple chats use the same group name.
                    let file_exists = path.exists();
                    let file = File::options().append(true).create(true).open(&path)?;
                    let mut buf = BufWriter::with_capacity(FILE_BUFFER_CAPACITY, file);
                    if !file_exists {
                        W::write_file_header(&mut buf)?;
                    }
                    Ok(entry.insert(buf))
                }
            }
        }
        None => Ok(&mut writer.state_mut().orphaned),
    }
}

fn advance_progress(pb: &ExportProgress, current_message: &mut u64) {
    *current_message += 1;
    if current_message.is_multiple_of(99) {
        pb.set_position(*current_message);
    }
}

/// Stream every message in the database, dispatching announcements and
/// regular messages to `writer.format_announcement` /
/// `writer.format_message_into`. Tapbacks, poll votes and poll updates are
/// rendered in context by their parent messages, so they're skipped here.
/// Duplicate ROWIDs are dropped (see [issue #135]).
///
/// Per-message formatting errors (corrupt edited blobs, unparseable balloons,
/// etc.) are caught, logged to `stderr` (with `rowid` + `guid`), and tallied. The
/// export continues for the remaining messages. A one-line summary is emitted
/// after the progress bar only when one or more messages were skipped.
/// Row-deserialization errors and I/O errors remain fatal.
///
/// [issue #135]: https://github.com/ReagentX/imessage-exporter/issues/135
pub fn run_export<'a, W>(writer: &mut W) -> Result<(), RuntimeError>
where
    W: MessageWriter<'a>,
{
    eprintln!(
        "Exporting to {} as {}...",
        writer.config().options.export_path.display(),
        W::LABEL,
    );

    W::write_file_header(&mut writer.state_mut().orphaned)?;

    let mut current_message_row = -1;
    let mut current_message = 0;
    let mut failures: u64 = 0;
    let total_messages = Message::get_count(
        writer.config().data_source.db(),
        &writer.config().options.query_context,
    )?;
    writer.state().pb.start(total_messages);

    let mut statement = Message::stream_rows(
        writer.config().data_source.db(),
        &writer.config().options.query_context,
    )?;

    // Reused across iterations so each message doesn't allocate a fresh
    // output buffer. Capacity grows naturally to fit the largest message
    // and `clear()` retains it.
    let mut msg_buf = String::with_capacity(W::BUFFER_CAPACITY);
    for message in Message::rows(&mut statement, [])? {
        let mut msg = message?;

        // Early escape if we try and render the same message GUID twice
        // See https://github.com/ReagentX/imessage-exporter/issues/135
        if msg.rowid == current_message_row {
            advance_progress(&writer.state().pb, &mut current_message);
            continue;
        }
        current_message_row = msg.rowid;

        // Tapbacks, poll votes, and poll updates are rendered in context by
        // their parent messages, never at the top level, so we can skip them here
        if !msg.is_edited() && (msg.is_tapback() || msg.is_poll_vote() || msg.is_poll_update()) {
            advance_progress(&writer.state().pb, &mut current_message);
            continue;
        }

        apply_body(&mut msg, writer.config().data_source.db());

        if msg.is_announcement() {
            msg_buf.clear();
            writer.format_announcement(&msg, &mut msg_buf);
            let file = get_or_create_file_for(writer, &msg)?;
            file.write_all(msg_buf.as_bytes())?;
        }
        // Message tapbacks and poll votes are rendered in context, so no need to render them separately
        else if !msg.is_tapback() && !msg.is_poll_vote() && !msg.is_poll_update() {
            msg_buf.clear();
            match writer.format_message_into(&msg, RenderContext::TopLevel, &mut msg_buf) {
                Ok(()) => {
                    let file = get_or_create_file_for(writer, &msg)?;
                    file.write_all(msg_buf.as_bytes())?;
                }
                Err(why) => {
                    failures += 1;
                    eprintln!(
                        "Skipping message (rowid={}, guid={}): {}",
                        msg.rowid, msg.guid, why
                    );
                }
            }
        }
        advance_progress(&writer.state().pb, &mut current_message);
    }
    writer.state().pb.finish();

    if failures > 0 {
        eprintln!("{failures} messages skipped due to formatting errors.");
    }

    if let Some(notice) = W::footer_notice() {
        eprintln!("{notice}");
    }
    let state = writer.state_mut();
    for file in state.files.values_mut() {
        W::write_file_footer(file)?;
        // Surface flush errors (disk full, quota, unmount, NFS hiccup) here
        // rather than letting `BufWriter::Drop` discard them silently.
        file.flush()?;
    }
    W::write_file_footer(&mut state.orphaned)?;
    state.orphaned.flush()?;

    Ok(())
}
