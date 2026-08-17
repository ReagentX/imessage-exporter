use std::{
    collections::{
        HashMap,
        hash_map::Entry::{Occupied, Vacant},
    },
    fs::File,
    io::{BufWriter, IsTerminal, Write, stderr},
    path::PathBuf,
};

use imessage_database::tables::{
    messages::Message,
    table::{ORPHANED, Table},
};
use rusqlite::Connection;

use crate::{
    app::{error::RuntimeError, progress::ExportProgress, runtime::Config},
    exporters::{
        formatter::{MessageFormatter, RenderContext},
        shared::date_span::{DateSpan, Timestamp},
    },
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
    /// Destination for messages that don't have a conversation route. Held in
    /// an [`Option`] so [`ExportState::finish`] can drop the writer before the
    /// file is stamped.
    pub orphaned: Option<BufWriter<File>>,
    /// Path to the orphaned file, retained for stamping after its writer drops.
    pub orphaned_path: PathBuf,
    /// Drives the on-screen progress indicator.
    pub pb: ExportProgress,
    /// Message date span per chat file, keyed like [`Self::files`]. Populated
    /// only when `use_message_times` is set.
    spans: HashMap<String, DateSpan>,
    /// Message date span for the orphaned file. The file opens eagerly, but
    /// stays unstamped when no message routes to it.
    orphaned_span: Option<DateSpan>,
}

impl ExportState {
    /// Open the orphaned file (creating it if missing) under
    /// `config.options.export_path` with the supplied extension, then build
    /// the empty file cache and progress bar.
    pub fn new(config: &Config, extension: &str) -> Result<Self, RuntimeError> {
        let mut orphaned_path = config.options.export_path.clone();
        orphaned_path.push(ORPHANED);
        orphaned_path.set_extension(extension);
        let file = File::options()
            .append(true)
            .create(true)
            .open(&orphaned_path)?;
        // `--no-progress` forces off; otherwise show only when stderr is a TTY
        // so headless invocations (CI, redirects to logfiles) stay clean.
        let pb_enabled = config.options.show_progress && stderr().is_terminal();
        Ok(Self {
            files: HashMap::new(),
            route: HashMap::new(),
            orphaned: Some(BufWriter::with_capacity(FILE_BUFFER_CAPACITY, file)),
            orphaned_path,
            pb: ExportProgress::new(pb_enabled),
            spans: HashMap::new(),
            orphaned_span: None,
        })
    }

    /// Widen the chat file's span to cover `at`, allocating the key only the
    /// first time a message lands in that file.
    fn record_chat(&mut self, filename: &str, at: Timestamp) {
        match self.spans.get_mut(filename) {
            Some(span) => span.extend(at),
            None => {
                self.spans.insert(filename.to_string(), DateSpan::new(at));
            }
        }
    }

    /// Widen the orphaned file's span to cover `at`.
    fn record_orphaned(&mut self, at: Timestamp) {
        match &mut self.orphaned_span {
            Some(span) => span.extend(at),
            None => self.orphaned_span = Some(DateSpan::new(at)),
        }
    }

    /// Drop every writer, then stamp each file with a recorded date span.
    ///
    /// Call-site details:
    /// - Write and flush footers first so buffered output cannot replace the
    ///   stamped modification time. Dropping a [`BufWriter`] discards flush
    ///   errors.
    /// - Files with no recorded span in the current export are left untouched,
    ///   including the orphaned file when no message routes to it.
    /// - Only files are stamped. Their parent directories remain unchanged.
    /// - No span is ever recorded while `use_message_times` is off, so this
    ///   stamps nothing in that case.
    fn finish(&mut self, config: &Config) {
        self.files.clear();
        self.orphaned = None;

        for (filename, span) in self.spans.drain() {
            span.apply(&config.options.export_path.join(filename));
        }
        if let Some(span) = self.orphaned_span.take() {
            span.apply(&self.orphaned_path);
        }
    }
}

/// Convert the message date into a file-span timestamp.
///
/// A date that [`Message::date`] cannot convert contributes nothing to the span.
fn message_timestamp(config: &Config, message: &Message) -> Option<Timestamp> {
    let date = message.date(config.offset).ok()?;
    Some((date.timestamp(), date.timestamp_subsec_nanos()))
}

/// Return the cached orphaned writer, reopening the file for append if absent.
fn orphaned_writer(state: &mut ExportState) -> Result<&mut BufWriter<File>, RuntimeError> {
    let writer = match state.orphaned.take() {
        Some(writer) => writer,
        None => {
            let file = File::options()
                .append(true)
                .create(true)
                .open(&state.orphaned_path)?;
            BufWriter::with_capacity(FILE_BUFFER_CAPACITY, file)
        }
    };
    Ok(state.orphaned.insert(writer))
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
///
/// With `use_message_times` set, this also extends the destination file's date
/// span. Callers invoke it only for rendered messages.
pub fn get_or_create_file_for<'a, 'b, W>(
    writer: &'b mut W,
    message: &Message,
) -> Result<&'b mut BufWriter<File>, RuntimeError>
where
    W: MessageWriter<'a>,
{
    let config = writer.config();
    // Skip date conversion when the export will not apply the resulting span.
    let at = config
        .options
        .use_message_times
        .then(|| message_timestamp(config, message))
        .flatten();
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
            if let Some(at) = at {
                state.record_chat(&filename, at);
            }
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
        None => {
            let state = writer.state_mut();
            if let Some(at) = at {
                state.record_orphaned(at);
            }
            orphaned_writer(state)
        }
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
/// With `use_message_times` set, teardown stamps each transcript that received
/// a message during the current export: creation time from its earliest
/// rendered message and modification time from its latest. Writers are dropped
/// first. Directories remain unchanged, and metadata failures do not fail the
/// export.
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

    W::write_file_header(orphaned_writer(writer.state_mut())?)?;

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
    let config = writer.config();
    let state = writer.state_mut();
    for file in state.files.values_mut() {
        W::write_file_footer(file)?;
        // Surface flush errors (disk full, quota, unmount, NFS hiccup) here
        // rather than letting `BufWriter::Drop` discard them silently.
        file.flush()?;
    }
    let orphaned = orphaned_writer(state)?;
    W::write_file_footer(orphaned)?;
    orphaned.flush()?;

    // Stamp only after every footer write and checked flush succeeds.
    state.finish(config);

    Ok(())
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use std::{
        fs::metadata,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use imessage_database::tables::{
        messages::Message,
        table::{ORPHANED, Table},
    };

    use crate::{
        Config, HTML, Options, TXT, app::export_type::ExportType,
        exporters::shared::driver::run_export,
    };

    /// Whole Unix seconds, so no assertion can trip over a filesystem that
    /// truncates sub-second precision.
    fn seconds(time: SystemTime) -> i64 {
        time.duration_since(UNIX_EPOCH)
            .expect("time is after the Unix epoch")
            .as_secs() as i64
    }

    /// Read the fixture's earliest and latest message dates through the same
    /// conversion path as the exporter.
    fn expected_span(config: &Config) -> (i64, i64) {
        let mut statement =
            Message::stream_rows(config.data_source.db(), &config.options.query_context).unwrap();
        let dates: Vec<i64> = Message::rows(&mut statement, [])
            .unwrap()
            .map(|message| message.unwrap().date(config.offset).unwrap().timestamp())
            .collect();
        (
            *dates.iter().min().expect("fixture has messages"),
            *dates.iter().max().expect("fixture has messages"),
        )
    }

    /// The fixture's `chat` and `chat_message_join` tables are empty, so every
    /// message routes here.
    fn orphaned_file(config: &Config, extension: &str) -> PathBuf {
        config
            .options
            .export_path
            .join(format!("{ORPHANED}.{extension}"))
    }

    #[test]
    fn can_stamp_export_with_message_times() {
        let mut options = Options::fake_options(ExportType::Txt);
        options.use_message_times = true;
        let config = Config::fake_app(options);

        let (earliest, latest) = expected_span(&config);
        assert_ne!(
            earliest, latest,
            "fixture must span two distinct dates to catch first-seen/last-seen bugs"
        );

        let mut exporter = TXT::new(&config).unwrap();
        run_export(&mut exporter).unwrap();

        let metadata = metadata(orphaned_file(&config, "txt")).unwrap();
        assert!(metadata.len() > 0, "export wrote no messages");
        assert_eq!(seconds(metadata.modified().unwrap()), latest);

        // Birth time is settable only on macOS and Windows.
        #[cfg(any(target_vendor = "apple", windows))]
        assert_eq!(seconds(metadata.created().unwrap()), earliest);
    }

    /// HTML writes a footer after the last message; stamping before that write
    /// would replace the expected modification time.
    #[test]
    fn can_stamp_html_export_with_message_times() {
        let mut options = Options::fake_options(ExportType::Html);
        options.use_message_times = true;
        let config = Config::fake_app(options);

        let (earliest, latest) = expected_span(&config);
        assert_ne!(
            earliest, latest,
            "fixture must span two distinct dates to catch first-seen/last-seen bugs"
        );

        let mut exporter = HTML::new(&config).unwrap();
        run_export(&mut exporter).unwrap();

        let metadata = metadata(orphaned_file(&config, "html")).unwrap();
        assert!(metadata.len() > 0, "export wrote no messages");
        assert_eq!(seconds(metadata.modified().unwrap()), latest);

        // Birth time is settable only on macOS and Windows.
        #[cfg(any(target_vendor = "apple", windows))]
        assert_eq!(seconds(metadata.created().unwrap()), earliest);
    }

    #[test]
    fn cannot_stamp_export_without_message_times() {
        let options = Options::fake_options(ExportType::Txt);
        assert!(!options.use_message_times);
        let config = Config::fake_app(options);

        let (_, latest) = expected_span(&config);
        let before = seconds(SystemTime::now());

        let mut exporter = TXT::new(&config).unwrap();
        run_export(&mut exporter).unwrap();

        let modified = seconds(
            metadata(orphaned_file(&config, "txt"))
                .unwrap()
                .modified()
                .unwrap(),
        );
        assert_ne!(modified, latest);
        assert!(
            modified >= before,
            "unstamped file should keep its wall-clock mtime: {modified} < {before}"
        );
    }

    #[test]
    fn cannot_stamp_file_without_messages() {
        let mut options = Options::fake_options(ExportType::Txt);
        options.use_message_times = true;
        // Exclude every fixture message; the orphaned file is still created.
        options.query_context.set_start("2100-01-01").unwrap();
        let config = Config::fake_app(options);

        let before = seconds(SystemTime::now());

        let mut exporter = TXT::new(&config).unwrap();
        run_export(&mut exporter).unwrap();

        let metadata = metadata(orphaned_file(&config, "txt")).unwrap();
        assert_eq!(metadata.len(), 0);
        assert!(
            seconds(metadata.modified().unwrap()) >= before,
            "an empty file must keep its real timestamps"
        );
    }
}
