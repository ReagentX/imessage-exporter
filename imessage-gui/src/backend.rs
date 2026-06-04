//! Backend worker thread.
//!
//! The iMessage database connection (and the `Config` that owns it) is not
//! `Sync`, so all database work happens on a single dedicated thread. The UI
//! thread communicates with it exclusively through [`Command`]/[`Event`]
//! channels and never touches the connection directly.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    thread,
};

use eframe::egui;

use imessage_database::{
    tables::messages::Message,
    util::{
        dates::{format as fmt_date, get_local_time},
        platform::Platform,
        query_context::QueryContext,
    },
};
use imessage_exporter::{
    app::{
        call_logs,
        compatibility::attachment_manager::{AttachmentManager, AttachmentManagerMode},
        export_type::ExportType,
        runtime::ProgressCallback,
    },
    exporters::pdf,
    Config, Options,
};

use crate::model::*;

#[derive(Clone, Debug)]
struct OpenedSource {
    root_path: PathBuf,
    platform: PlatformChoice,
}

/// Messages sent from the UI to the backend.
pub enum Command {
    Open(OpenParams),
    Preview { filters: Filters, limit: usize },
    Export(ExportParams),
    HtmlPreview { filters: Filters },
    LoadCallLogs { limit: usize },
    Shutdown,
}

/// Messages sent from the backend to the UI.
pub enum Event {
    Status(String),
    Opened {
        conversations: Vec<ConversationSummary>,
        date_range: Option<(String, String)>,
        total_messages: usize,
        summary: String,
        platform: PlatformChoice,
    },
    OpenFailed(String),
    Preview {
        messages: Vec<PreviewMessage>,
        total: i64,
    },
    PreviewFailed(String),
    Progress {
        current: u64,
        total: u64,
    },
    ExportDone {
        path: PathBuf,
        summary: String,
    },
    ExportFailed(String),
    HtmlPreviewReady(PathBuf),
    HtmlPreviewFailed(String),
    CallLogsLoaded {
        entries: Vec<CallLogEntry>,
        total: i64,
        source: String,
    },
    CallLogsFailed(String),
}

/// Spawn the backend worker, returning the command sender and event receiver.
pub fn spawn(ctx: egui::Context) -> (Sender<Command>, Receiver<Event>) {
    let (cmd_tx, cmd_rx) = channel::<Command>();
    let (evt_tx, evt_rx) = channel::<Event>();
    thread::Builder::new()
        .name("imessage-backend".into())
        .spawn(move || backend_loop(&cmd_rx, &evt_tx, &ctx))
        .expect("failed to spawn backend thread");
    (cmd_tx, evt_rx)
}

fn send(evt_tx: &Sender<Event>, ctx: &egui::Context, event: Event) {
    let _ = evt_tx.send(event);
    ctx.request_repaint();
}

fn backend_loop(cmd_rx: &Receiver<Command>, evt_tx: &Sender<Event>, ctx: &egui::Context) {
    let mut config: Option<Config> = None;
    let mut opened_source: Option<OpenedSource> = None;
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            Command::Shutdown => break,
            Command::Open(params) => {
                handle_open(&mut config, &mut opened_source, params, evt_tx, ctx)
            }
            Command::Preview { filters, limit } => {
                handle_preview(config.as_ref(), &filters, limit, evt_tx, ctx)
            }
            Command::Export(params) => handle_export(config.as_mut(), params, evt_tx, ctx),
            Command::HtmlPreview { filters } => {
                handle_html_preview(config.as_mut(), &filters, evt_tx, ctx)
            }
            Command::LoadCallLogs { limit } => {
                handle_load_call_logs(config.as_ref(), opened_source.as_ref(), limit, evt_tx, ctx)
            }
        }
    }
}

// MARK: Open

fn handle_open(
    config_slot: &mut Option<Config>,
    source_slot: &mut Option<OpenedSource>,
    params: OpenParams,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    // Drop any previously open backup first so we release file handles.
    *config_slot = None;
    *source_slot = None;

    send(
        evt_tx,
        ctx,
        Event::Status(format!("Opening {} …", params.db_path.display())),
    );

    let platform = match params.platform {
        PlatformChoice::Auto => match Platform::determine(&params.db_path) {
            Ok(p) => p,
            Err(why) => {
                send(
                    evt_tx,
                    ctx,
                    Event::OpenFailed(format!("Could not determine platform: {why}")),
                );
                return;
            }
        },
        PlatformChoice::MacOS => Platform::macOS,
        PlatformChoice::IOS => Platform::iOS,
    };
    let is_ios = matches!(platform, Platform::iOS);
    let opened_platform = if is_ios {
        PlatformChoice::IOS
    } else {
        PlatformChoice::MacOS
    };

    send(
        evt_tx,
        ctx,
        Event::Status("Building cache (this can take a moment on large databases) …".into()),
    );

    let options = Options {
        db_path: params.db_path.clone(),
        attachment_root: params.attachment_root.clone(),
        attachment_manager: AttachmentManager::from(AttachmentManagerMode::Disabled),
        diagnostic: false,
        export_type: None,
        export_path: std::env::temp_dir().join("imessage-gui-placeholder"),
        query_context: QueryContext::default(),
        no_lazy: false,
        custom_name: None,
        use_caller_id: false,
        platform,
        ignore_disk_space: true,
        conversation_filter: None,
        cleartext_password: if is_ios {
            params.password.clone()
        } else {
            None
        },
        contacts_path: params.contacts_path.clone(),
        show_progress: false,
        export_call_logs: false,
        call_log_limit: None,
    };

    let config = match Config::new(options) {
        Ok(config) => config,
        Err(why) => {
            send(evt_tx, ctx, Event::OpenFailed(format!("{why}")));
            return;
        }
    };

    // Build the conversation list and summary stats.
    let counts = config.message_counts_by_chat();
    let conversations = build_conversations(&config, &counts);

    let (date_range, total_messages) = match Message::run_diagnostic(config.db()) {
        Ok(diag) => {
            let range = match (diag.first_message_date, diag.last_message_date) {
                (Some(first), Some(last)) => {
                    match (
                        get_local_time(first, config.offset),
                        get_local_time(last, config.offset),
                    ) {
                        (Ok(f), Ok(l)) => Some((fmt_date(&f), fmt_date(&l))),
                        _ => None,
                    }
                }
                _ => None,
            };
            (range, diag.total_messages)
        }
        Err(_) => (None, 0),
    };

    let summary = format!(
        "{} • {} conversations • {} messages",
        if is_ios {
            "iOS backup"
        } else {
            "macOS database"
        },
        conversations.len(),
        total_messages
    );

    *source_slot = Some(OpenedSource {
        root_path: params.db_path,
        platform: opened_platform,
    });
    *config_slot = Some(config);

    send(
        evt_tx,
        ctx,
        Event::Opened {
            conversations,
            date_range,
            total_messages,
            summary,
            platform: opened_platform,
        },
    );
}

/// Resolve a handle id to a display name using the same maps the exporter uses.
fn handle_name(config: &Config, handle_id: i32) -> Option<String> {
    config
        .real_participants
        .get(&handle_id)
        .and_then(|internal| config.participants.get(internal))
        .map(|name| name.get_display_name().to_string())
}

/// Build a deduplicated conversation list from the cached `Config`.
fn build_conversations(
    config: &Config,
    counts: &std::collections::HashMap<i32, i64>,
) -> Vec<ConversationSummary> {
    // Group raw chat rowids by their deduplicated id.
    let mut groups: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
    for raw in config.chatrooms.keys() {
        let real = *config.real_chatrooms.get(raw).unwrap_or(raw);
        groups.entry(real).or_default().push(*raw);
    }

    let mut out: Vec<ConversationSummary> = Vec::with_capacity(groups.len());
    for (real, raw_chat_ids) in groups {
        // Prefer an explicit display name from any underlying chat.
        let display_name = raw_chat_ids
            .iter()
            .filter_map(|id| config.chatrooms.get(id))
            .find_map(|chat| chat.display_name().map(|s| s.to_string()));

        // Union of participant handle ids across the grouped chats.
        let mut handle_ids: BTreeSet<i32> = BTreeSet::new();
        for id in &raw_chat_ids {
            if let Some(parts) = config.chatroom_participants.get(id) {
                handle_ids.extend(parts.iter().copied());
            }
        }

        let mut names: Vec<String> = handle_ids
            .iter()
            .map(|h| handle_name(config, *h).unwrap_or_else(|| "Unknown".to_string()))
            .collect();
        names.sort();
        names.dedup();
        let participants = if names.is_empty() {
            raw_chat_ids
                .iter()
                .filter_map(|id| config.chatrooms.get(id))
                .map(|chat| chat.chat_identifier.clone())
                .next()
                .unwrap_or_else(|| "Unknown".to_string())
        } else {
            names.join(", ")
        };

        let title = display_name.unwrap_or_else(|| participants.clone());
        let message_count: i64 = raw_chat_ids
            .iter()
            .map(|id| counts.get(id).copied().unwrap_or(0))
            .sum();

        out.push(ConversationSummary {
            id: real,
            raw_chat_ids,
            title,
            participants,
            message_count,
        });
    }

    out.sort_by_key(|a| a.title.to_lowercase());
    out
}

// MARK: Filters

/// Build a [`QueryContext`] from UI filters. The text participant filter is
/// resolved against the cached participant/chat maps exactly as
/// `Config::resolve_filtered_handles` does.
fn build_query_context(config: &Config, filters: &Filters) -> QueryContext {
    let mut qc = QueryContext {
        start: filters.start_ns,
        end: filters.end_ns,
        ..Default::default()
    };

    if !filters.selected_raw_chat_ids.is_empty() {
        qc.set_selected_chat_ids(filters.selected_raw_chat_ids.iter().copied().collect());
    } else if let Some(text) = filters
        .conversation_filter
        .as_ref()
        .filter(|t| !t.trim().is_empty())
    {
        let parsed: Vec<&str> = text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        let mut handles: BTreeSet<i32> = BTreeSet::new();
        for name in config.participants.values() {
            for needle in &parsed {
                if name.contains(needle) {
                    handles.extend(name.handle_ids.iter().copied());
                }
            }
        }
        let mut chats: BTreeSet<i32> = BTreeSet::new();
        for (chat_id, parts) in &config.chatroom_participants {
            if !parts.is_disjoint(&handles) {
                chats.insert(*chat_id);
            }
        }
        qc.set_selected_handle_ids(handles);
        qc.set_selected_chat_ids(chats);
    }

    qc
}

// MARK: Preview

fn handle_preview(
    config: Option<&Config>,
    filters: &Filters,
    limit: usize,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let Some(config) = config else {
        send(
            evt_tx,
            ctx,
            Event::PreviewFailed("Open a backup before previewing.".into()),
        );
        return;
    };

    let qc = build_query_context(config, filters);

    match collect_preview(config, &qc, limit) {
        Ok(messages) => {
            let total = Message::get_count(config.db(), &qc).unwrap_or(messages.len() as i64);
            send(evt_tx, ctx, Event::Preview { messages, total });
        }
        Err(why) => send(evt_tx, ctx, Event::PreviewFailed(why)),
    }
}

fn collect_preview(
    config: &Config,
    qc: &QueryContext,
    limit: usize,
) -> Result<Vec<PreviewMessage>, String> {
    pdf::collect_messages(config, qc, limit, false).map_err(|why| format!("{why}"))
}

// MARK: Call logs

fn handle_load_call_logs(
    config: Option<&Config>,
    source: Option<&OpenedSource>,
    limit: usize,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let Some(config) = config else {
        send(
            evt_tx,
            ctx,
            Event::CallLogsFailed("Open an iOS backup before loading call logs.".into()),
        );
        return;
    };
    let Some(source) = source else {
        send(
            evt_tx,
            ctx,
            Event::CallLogsFailed("Open an iOS backup before loading call logs.".into()),
        );
        return;
    };
    if source.platform != PlatformChoice::IOS {
        send(
            evt_tx,
            ctx,
            Event::CallLogsFailed("Call logs are available from iOS backup folders only.".into()),
        );
        return;
    }

    send(evt_tx, ctx, Event::Status("Loading call logs ...".into()));

    match call_logs::load(config, &source.root_path, Some(limit)) {
        Ok(result) => send(
            evt_tx,
            ctx,
            Event::CallLogsLoaded {
                entries: result.entries,
                total: result.total,
                source: result.source,
            },
        ),
        Err(why) => send(evt_tx, ctx, Event::CallLogsFailed(format!("{why}"))),
    }
}

// MARK: Export

fn attachment_manager_mode(copy_method: CopyMethod) -> AttachmentManagerMode {
    match copy_method {
        CopyMethod::Disabled => AttachmentManagerMode::Disabled,
        CopyMethod::Clone => AttachmentManagerMode::Clone,
        CopyMethod::Basic => AttachmentManagerMode::Basic,
        CopyMethod::Full => AttachmentManagerMode::Full,
    }
}

fn apply_export_options(
    config: &mut Config,
    params: &ExportParams,
    export_type: ExportType,
    qc: QueryContext,
) {
    config.options.export_type = Some(export_type);
    config.options.export_path = params.export_path.clone();
    config.options.attachment_manager =
        AttachmentManager::from(attachment_manager_mode(params.copy_method));
    config.options.no_lazy = params.no_lazy;
    config.options.custom_name = params.custom_name.clone();
    config.options.use_caller_id = params.use_caller_id;
    config.options.ignore_disk_space = params.ignore_disk_space;
    // Filtering is fully resolved into the query context, so no text filter.
    config.options.conversation_filter = None;
    config.options.show_progress = false;
    config.options.export_call_logs = false;
    config.options.call_log_limit = None;
    config.options.query_context = qc;
}

fn handle_export(
    config: Option<&mut Config>,
    params: ExportParams,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let Some(config) = config else {
        send(
            evt_tx,
            ctx,
            Event::ExportFailed("Open a backup before exporting.".into()),
        );
        return;
    };

    let export_path = params.export_path.clone();
    let extension = match params.format {
        FormatChoice::Html => "html",
        FormatChoice::Txt => "txt",
        FormatChoice::Pdf => "pdf",
    };

    // Guard against appending into an export that already exists.
    if let Err(why) = ensure_export_dir_clear(&export_path, extension) {
        send(evt_tx, ctx, Event::ExportFailed(why));
        return;
    }

    let export_type = match params.format {
        FormatChoice::Html => ExportType::Html,
        FormatChoice::Txt => ExportType::Txt,
        FormatChoice::Pdf => ExportType::Pdf,
    };

    let qc = build_query_context(config, &params.filters);
    let total = Message::get_count(config.db(), &qc).unwrap_or(0);
    apply_export_options(config, &params, export_type, qc);

    // Install a progress callback that forwards to the UI.
    let etx = evt_tx.clone();
    let cctx = ctx.clone();
    let callback: ProgressCallback = Arc::new(move |current, total| {
        let _ = etx.send(Event::Progress { current, total });
        cctx.request_repaint();
    });
    config.progress_callback = Some(callback);

    send(
        evt_tx,
        ctx,
        Event::Status(format!(
            "Exporting {total} messages to {} …",
            export_path.display()
        )),
    );

    let result = config.start();
    config.progress_callback = None;

    match result {
        Ok(()) => send(
            evt_tx,
            ctx,
            Event::ExportDone {
                path: export_path.clone(),
                summary: format!("Exported {total} messages to {}", export_path.display()),
            },
        ),
        Err(why) => send(evt_tx, ctx, Event::ExportFailed(format!("{why}"))),
    }
}

/// Return an error if `dir` already contains files with the target extension.
fn ensure_export_dir_clear(dir: &Path, extension: &str) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Cannot read export folder {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        if entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(extension))
        {
            return Err(format!(
                "Export folder {} already contains .{extension} files. Choose an empty folder to avoid mixing exports.",
                dir.display()
            ));
        }
    }
    Ok(())
}

// MARK: HTML preview

fn handle_html_preview(
    config: Option<&mut Config>,
    filters: &Filters,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let Some(config) = config else {
        send(
            evt_tx,
            ctx,
            Event::HtmlPreviewFailed("Open a backup before previewing.".into()),
        );
        return;
    };

    // Unique temp directory for this preview render.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("imessage-gui-preview-{stamp}"));
    if let Err(why) = std::fs::create_dir_all(&dir) {
        send(
            evt_tx,
            ctx,
            Event::HtmlPreviewFailed(format!("Could not create preview folder: {why}")),
        );
        return;
    }

    let preview_params = ExportParams {
        filters: filters.clone(),
        format: FormatChoice::Html,
        export_path: dir.clone(),
        copy_method: CopyMethod::Disabled,
        no_lazy: true,
        custom_name: config.options.custom_name.clone(),
        use_caller_id: config.options.use_caller_id,
        ignore_disk_space: true,
    };

    let qc = build_query_context(config, filters);
    apply_export_options(config, &preview_params, ExportType::Html, qc);
    config.progress_callback = None;

    send(
        evt_tx,
        ctx,
        Event::Status("Rendering HTML preview …".into()),
    );

    if let Err(why) = config.start() {
        send(evt_tx, ctx, Event::HtmlPreviewFailed(format!("{why}")));
        return;
    }

    match largest_html_file(&dir) {
        Some(file) => {
            if let Err(why) = open::that(&file) {
                send(
                    evt_tx,
                    ctx,
                    Event::HtmlPreviewFailed(format!("Could not open preview: {why}")),
                );
            } else {
                send(evt_tx, ctx, Event::HtmlPreviewReady(file));
            }
        }
        None => send(
            evt_tx,
            ctx,
            Event::HtmlPreviewFailed("No messages matched the current filters.".into()),
        ),
    }
}

/// Find the largest `.html` file in `dir` (the conversation file, as opposed to
/// the near-empty `orphaned.html`).
fn largest_html_file(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("html") {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if best.as_ref().is_none_or(|(b, _)| size > *b) {
                best = Some((size, path));
            }
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `Config` against the bundled synthetic test database, mirroring
    /// how `handle_open` constructs one.
    fn test_config() -> Config {
        let db_path = std::env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let options = Options {
            db_path,
            attachment_root: None,
            attachment_manager: AttachmentManager::from(AttachmentManagerMode::Disabled),
            diagnostic: false,
            export_type: None,
            export_path: std::env::temp_dir().join("imessage-gui-test"),
            query_context: QueryContext::default(),
            no_lazy: false,
            custom_name: None,
            use_caller_id: false,
            platform: Platform::macOS,
            ignore_disk_space: true,
            conversation_filter: None,
            cleartext_password: None,
            contacts_path: None,
            show_progress: false,
            export_call_logs: false,
            call_log_limit: None,
        };
        Config::new(options).expect("failed to open bundled test database")
    }

    #[test]
    fn builds_conversation_list_without_panicking() {
        let config = test_config();
        // The synthetic fixture has no chats; this must still succeed cleanly.
        let counts = config.message_counts_by_chat();
        let _ = build_conversations(&config, &counts);
    }

    #[test]
    fn previews_messages_from_test_db() {
        let config = test_config();
        let messages =
            collect_preview(&config, &QueryContext::default(), 100).expect("preview failed");
        assert!(
            !messages.is_empty(),
            "expected at least one preview message"
        );
        for message in &messages {
            assert!(
                !message.timestamp.is_empty(),
                "every preview message should have a timestamp"
            );
            assert!(
                !message.sender.is_empty(),
                "every preview message should have a sender"
            );
        }
    }

    #[test]
    fn date_filter_narrows_results() {
        let config = test_config();
        let all = Message::get_count(config.db(), &QueryContext::default()).unwrap_or(0);
        // A start date far in the future should exclude everything.
        let future = QueryContext {
            start: crate::model::parse_local_timestamp("2099-01-01", "00:00").ok(),
            ..Default::default()
        };
        let none = collect_preview(&config, &future, 100).unwrap();
        assert!(all >= 1);
        assert!(
            none.is_empty(),
            "future start date should exclude all messages"
        );
    }
}
