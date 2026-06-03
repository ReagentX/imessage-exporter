//! Backend worker thread.
//!
//! The iMessage database connection (and the `Config` that owns it) is not
//! `Sync`, so all database work happens on a single dedicated thread. The UI
//! thread communicates with it exclusively through [`Command`]/[`Event`]
//! channels and never touches the connection directly.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, File},
    io::copy,
    path::{Path, PathBuf},
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{Local, TimeZone};
use crabapple::error::BackupError;
use eframe::egui;
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, Row};
use sha1::{Digest, Sha1};

use imessage_database::{
    tables::{
        attachment::{Attachment, MediaType},
        messages::{models::BubbleComponent, Message},
        table::Table,
    },
    util::{
        dates::{format as fmt_date, get_local_time},
        platform::Platform,
        query_context::QueryContext,
    },
};
use imessage_exporter::{
    app::{
        compatibility::attachment_manager::{AttachmentManager, AttachmentManagerMode},
        export_type::ExportType,
        runtime::ProgressCallback,
    },
    Config, Options,
};

use crate::model::*;

const IOS_BACKUP_DOMAIN_SEPARATOR: &str = "-";
const IOS_BACKUP_HASH_FOLDER_LEN: usize = 2;
const HEX_CHARS_PER_BYTE: usize = 2;
const APPLE_UNIX_OFFSET_SECONDS: f64 = 978_307_200.0;
const NANOS_PER_SECOND: f64 = 1_000_000_000.0;
const MAX_NANOSECOND_PART: u32 = 999_999_999;
const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 3_600;
const CALL_LOG_TEMP_DIR_PREFIX: &str = "imessage-gui-callhistory";
const SQLITE_WAL_SUFFIX: &str = "-wal";
const SQLITE_SHM_SUFFIX: &str = "-shm";
const CALL_LOG_NO_VALUE: &str = "Unknown";
const CALL_LOG_TYPE_PREFIX: &str = "Type";
const CALL_ROW_ID_INDEX: usize = 0;
const CALL_ADDRESS_INDEX: usize = 1;
const CALL_DATE_INDEX: usize = 2;
const CALL_DURATION_INDEX: usize = 3;
const CALL_ORIGINATED_INDEX: usize = 4;
const CALL_ANSWERED_INDEX: usize = 5;
const CALL_TYPE_INDEX: usize = 6;
const CALL_SERVICE_INDEX: usize = 7;
const CALL_LEGACY_FLAGS_INDEX: usize = 8;

const CALL_HISTORY_MODERN_SOURCE: CallHistorySource = CallHistorySource {
    label: "CallHistory.storedata",
    domain: "HomeDomain",
    relative_path: "Library/CallHistoryDB/CallHistory.storedata",
    sqlite_name: "CallHistory.storedata",
};

const CALL_HISTORY_LEGACY_SOURCE: CallHistorySource = CallHistorySource {
    label: "call_history.db",
    domain: "WirelessDomain",
    relative_path: "Library/CallHistory/call_history.db",
    sqlite_name: "call_history.db",
};

const CALL_HISTORY_SOURCES: &[CallHistorySource] =
    &[CALL_HISTORY_MODERN_SOURCE, CALL_HISTORY_LEGACY_SOURCE];

const MODERN_CALL_TABLE: &str = "ZCALLRECORD";
const MODERN_ID_COLUMN: &str = "Z_PK";
const MODERN_ADDRESS_COLUMN: &str = "ZADDRESS";
const MODERN_DATE_COLUMN: &str = "ZDATE";
const MODERN_DURATION_COLUMN: &str = "ZDURATION";
const MODERN_ORIGINATED_COLUMN: &str = "ZORIGINATED";
const MODERN_ANSWERED_COLUMN: &str = "ZANSWERED";
const MODERN_CALL_TYPE_COLUMN: &str = "ZCALLTYPE";
const MODERN_SERVICE_COLUMN: &str = "ZSERVICE_PROVIDER";

const LEGACY_CALL_TABLE: &str = "call";
const LEGACY_ID_COLUMN: &str = "ROWID";
const LEGACY_ADDRESS_COLUMN: &str = "address";
const LEGACY_DATE_COLUMN: &str = "date";
const LEGACY_DURATION_COLUMN: &str = "duration";
const LEGACY_FLAGS_COLUMN: &str = "flags";
const LEGACY_INCOMING_FLAG: i64 = 4;
const LEGACY_OUTGOING_FLAG: i64 = 5;
const LEGACY_BLOCKED_FLAG: i64 = 8;

#[derive(Clone, Copy, Debug)]
struct CallHistorySource {
    label: &'static str,
    domain: &'static str,
    relative_path: &'static str,
    sqlite_name: &'static str,
}

#[derive(Clone, Debug)]
struct OpenedSource {
    root_path: PathBuf,
    platform: PlatformChoice,
}

struct MaterializedCallHistory {
    db_path: PathBuf,
    temp_dir: PathBuf,
    source_label: &'static str,
}

impl Drop for MaterializedCallHistory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

#[derive(Clone, Debug)]
struct CallLogSchema {
    table: &'static str,
    id_expr: &'static str,
    address_expr: &'static str,
    date_expr: &'static str,
    duration_expr: &'static str,
    originated_expr: &'static str,
    answered_expr: &'static str,
    call_type_expr: &'static str,
    service_expr: &'static str,
    legacy_flags_expr: &'static str,
    order_expr: &'static str,
}

#[derive(Debug)]
struct RawCallLogRow {
    id: i64,
    address: Option<String>,
    date: Option<f64>,
    duration: Option<f64>,
    originated: Option<i64>,
    answered: Option<i64>,
    call_type: Option<i64>,
    service: Option<String>,
    legacy_flags: Option<i64>,
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
    collect_messages(config, qc, limit, false)
}

fn collect_pdf_messages(
    config: &Config,
    qc: &QueryContext,
    limit: usize,
) -> Result<Vec<PreviewMessage>, String> {
    collect_messages(config, qc, limit, true)
}

fn collect_messages(
    config: &Config,
    qc: &QueryContext,
    limit: usize,
    include_image_attachments: bool,
) -> Result<Vec<PreviewMessage>, String> {
    let db = config.db();
    let mut statement = Message::stream_rows(db, qc).map_err(|e| format!("{e}"))?;
    let mut out: Vec<PreviewMessage> = Vec::new();

    for row in Message::rows(&mut statement, []).map_err(|e| format!("{e}"))? {
        let mut msg = row.map_err(|e| format!("{e}"))?;

        // Tapbacks / poll votes are rendered in context elsewhere; skip them.
        if !msg.is_edited() && (msg.is_tapback() || msg.is_poll_vote() || msg.is_poll_update()) {
            continue;
        }

        if let Ok(body) = msg.parse_body(db) {
            msg.apply_body(body);
        }

        let sender = config
            .who(msg.handle_id, msg.is_from_me, &msg.destination_caller_id)
            .to_string();
        let timestamp = msg
            .date(config.offset)
            .map(|d| fmt_date(&d))
            .unwrap_or_default();
        let attachment_count = msg.num_attachments.max(0) as usize;
        let attachments = if include_image_attachments {
            collect_pdf_image_attachments(config, &msg)?
        } else {
            Vec::new()
        };

        out.push(PreviewMessage {
            is_from_me: msg.is_from_me,
            sender,
            timestamp,
            text: preview_text(&msg),
            attachment_count,
            attachments,
            annotations: preview_annotations(&msg),
        });

        if out.len() >= limit {
            break;
        }
    }

    Ok(out)
}

fn collect_pdf_image_attachments(
    config: &Config,
    msg: &Message,
) -> Result<Vec<PreviewAttachment>, String> {
    let attachments = Attachment::from_message(config.db(), msg)
        .map_err(|e| format!("Attachment query failed: {e}"))?;
    let mut out = Vec::new();

    for mut attachment in ordered_attachments_for_message(msg, attachments) {
        if !is_pdf_image_candidate(&attachment) {
            continue;
        }

        if let Err(why) =
            config
                .options
                .attachment_manager
                .handle_attachment(msg, &mut attachment, config)
        {
            eprintln!(
                "Skipping PDF image attachment rowid={} for message rowid={}: {why}",
                attachment.rowid, msg.rowid
            );
            continue;
        }

        let path = attachment.copied_path.clone().or_else(|| {
            attachment
                .resolved_attachment_path(
                    &config.options.platform,
                    &config.options.db_path,
                    config.options.attachment_root.as_deref(),
                )
                .map(PathBuf::from)
        });
        let Some(path) = path.filter(|p| p.is_file()) else {
            continue;
        };

        let name = attachment
            .filename()
            .map(ToOwned::to_owned)
            .or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "image attachment".to_string());

        out.push(PreviewAttachment { path, name });
    }

    Ok(out)
}

fn ordered_attachments_for_message(msg: &Message, attachments: Vec<Attachment>) -> Vec<Attachment> {
    if attachments.len() <= 1 {
        return attachments;
    }

    let by_guid: HashMap<String, usize> = attachments
        .iter()
        .enumerate()
        .filter_map(|(idx, attachment)| attachment.guid.clone().map(|guid| (guid, idx)))
        .collect();
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    let mut next_positional = 0usize;

    for component in &msg.components {
        let BubbleComponent::Run(ranges) = component else {
            continue;
        };
        for range in ranges {
            if let Some(meta) = &range.attachment {
                let idx = meta
                    .guid
                    .as_deref()
                    .and_then(|guid| by_guid.get(guid).copied())
                    .unwrap_or_else(|| {
                        let idx = next_positional;
                        next_positional += 1;
                        idx
                    });
                if idx < attachments.len() && seen.insert(idx) {
                    order.push(idx);
                }
            }
        }
    }

    if order.is_empty() {
        return attachments;
    }

    for idx in 0..attachments.len() {
        if seen.insert(idx) {
            order.push(idx);
        }
    }

    let mut slots: Vec<Option<Attachment>> = attachments.into_iter().map(Some).collect();
    order
        .into_iter()
        .filter_map(|idx| slots.get_mut(idx).and_then(Option::take))
        .collect()
}

fn is_pdf_image_candidate(attachment: &Attachment) -> bool {
    if matches!(attachment.mime_type(), MediaType::Image(_)) {
        return true;
    }

    attachment.extension().is_some_and(|ext| {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "gif" | "jpg" | "jpeg" | "png"
        )
    })
}

fn preview_text(msg: &Message) -> String {
    let raw = msg.text.clone().unwrap_or_default();
    // Strip the object-replacement / replacement chars iMessage uses as inline
    // attachment placeholders so the preview reads cleanly.
    let cleaned = raw.replace(['\u{FFFC}', '\u{FFFD}'], " ");
    let cleaned = cleaned.trim();
    if !cleaned.is_empty() {
        return cleaned.to_string();
    }
    if msg.is_url() {
        "🔗 Link".to_string()
    } else {
        String::new()
    }
}

fn preview_annotations(msg: &Message) -> Vec<String> {
    let mut a = Vec::new();
    if msg.has_attachments() {
        let n = msg.num_attachments;
        a.push(format!(
            "📎 {n} attachment{}",
            if n == 1 { "" } else { "s" }
        ));
    }
    if msg.is_reply() {
        a.push("↪ reply".to_string());
    }
    if msg.has_replies() {
        let n = msg.num_replies;
        a.push(format!("💬 {n} repl{}", if n == 1 { "y" } else { "ies" }));
    }
    if msg.is_edited() {
        a.push("✎ edited".to_string());
    }
    if msg.is_expressive() {
        a.push("✨ effect".to_string());
    }
    a
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

    let materialized = match materialize_call_history(config, source) {
        Ok(materialized) => materialized,
        Err(why) => {
            send(evt_tx, ctx, Event::CallLogsFailed(why));
            return;
        }
    };

    match collect_call_logs_from_db(&materialized.db_path, limit) {
        Ok((entries, total)) => send(
            evt_tx,
            ctx,
            Event::CallLogsLoaded {
                entries,
                total,
                source: materialized.source_label.to_string(),
            },
        ),
        Err(why) => send(evt_tx, ctx, Event::CallLogsFailed(why)),
    }
}

fn materialize_call_history(
    config: &Config,
    source: &OpenedSource,
) -> Result<MaterializedCallHistory, String> {
    let tried = CALL_HISTORY_SOURCES
        .iter()
        .map(|candidate| format!("{}:{}", candidate.domain, candidate.relative_path))
        .collect::<Vec<_>>()
        .join(", ");

    for candidate in CALL_HISTORY_SOURCES {
        if let Some(materialized) = materialize_call_history_candidate(config, source, *candidate)?
        {
            return Ok(materialized);
        }
    }

    Err(format!(
        "Could not find a call-history database in the backup. Tried: {tried}"
    ))
}

fn materialize_call_history_candidate(
    config: &Config,
    source: &OpenedSource,
    candidate: CallHistorySource,
) -> Result<Option<MaterializedCallHistory>, String> {
    let temp_dir = create_call_history_temp_dir(candidate.sqlite_name)?;
    let primary_target = temp_dir.join(candidate.sqlite_name);
    if !copy_backup_file_to(
        config,
        &source.root_path,
        candidate.domain,
        candidate.relative_path,
        &primary_target,
    )? {
        let _ = fs::remove_dir_all(&temp_dir);
        return Ok(None);
    }

    for suffix in [SQLITE_WAL_SUFFIX, SQLITE_SHM_SUFFIX] {
        let relative_path = format!("{}{}", candidate.relative_path, suffix);
        let target = temp_dir.join(format!("{}{}", candidate.sqlite_name, suffix));
        let _ = copy_backup_file_to(
            config,
            &source.root_path,
            candidate.domain,
            &relative_path,
            &target,
        )?;
    }

    Ok(Some(MaterializedCallHistory {
        db_path: primary_target,
        temp_dir,
        source_label: candidate.label,
    }))
}

fn create_call_history_temp_dir(sqlite_name: &str) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!(
        "{CALL_LOG_TEMP_DIR_PREFIX}-{}-{stamp}-{sqlite_name}",
        std::process::id()
    ));
    fs::create_dir(&dir).map_err(|why| {
        format!(
            "Could not create temporary call-history folder {}: {why}",
            dir.display()
        )
    })?;
    Ok(dir)
}

fn copy_backup_file_to(
    config: &Config,
    backup_root: &Path,
    domain: &str,
    relative_path: &str,
    target: &Path,
) -> Result<bool, String> {
    let file_id = backup_file_id(domain, relative_path);
    if let Some(backup) = config.data_source.backup.as_ref() {
        let file = match backup.get_file(&file_id) {
            Ok(file) => file,
            Err(BackupError::FileNotFoundInBackup(_)) => return Ok(false),
            Err(why) => {
                return Err(format!(
                    "Could not resolve {domain}:{relative_path} in encrypted backup: {why}"
                ));
            }
        };
        let mut decrypted = backup
            .decrypt_entry_stream(&file)
            .map_err(|why| format!("Could not decrypt {domain}:{relative_path}: {why}"))?;
        let mut out = File::create(target)
            .map_err(|why| format!("Could not create {}: {why}", target.display()))?;
        copy(&mut decrypted, &mut out)
            .map_err(|why| format!("Could not write {}: {why}", target.display()))?;
        return Ok(true);
    }

    let hashed_path = backup_root
        .join(&file_id[..IOS_BACKUP_HASH_FOLDER_LEN])
        .join(&file_id);
    if !hashed_path.is_file() {
        return Ok(false);
    }
    fs::copy(&hashed_path, target).map_err(|why| {
        format!(
            "Could not copy {} to {}: {why}",
            hashed_path.display(),
            target.display()
        )
    })?;
    Ok(true)
}

fn backup_file_id(domain: &str, relative_path: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(domain.as_bytes());
    hasher.update(IOS_BACKUP_DOMAIN_SEPARATOR.as_bytes());
    hasher.update(relative_path.as_bytes());
    let digest = hasher.finalize();
    hex_digest(&digest)
}

fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(bytes.len() * HEX_CHARS_PER_BYTE);
    for byte in bytes {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn collect_call_logs_from_db(
    db_path: &Path,
    limit: usize,
) -> Result<(Vec<CallLogEntry>, i64), String> {
    let conn = Connection::open(db_path)
        .map_err(|why| format!("Could not open call-history database: {why}"))?;
    let schema = call_log_schema(&conn)?;
    let total = call_log_count(&conn, &schema)?;
    let sql = format!(
        "SELECT {id} AS row_id, {address} AS address, {date} AS started, \
         {duration} AS duration, {originated} AS originated, {answered} AS answered, \
         {call_type} AS call_type, {service} AS service, {flags} AS legacy_flags \
         FROM {table} ORDER BY {order_by} DESC LIMIT ?1",
        id = schema.id_expr,
        address = schema.address_expr,
        date = schema.date_expr,
        duration = schema.duration_expr,
        originated = schema.originated_expr,
        answered = schema.answered_expr,
        call_type = schema.call_type_expr,
        service = schema.service_expr,
        flags = schema.legacy_flags_expr,
        table = schema.table,
        order_by = schema.order_expr,
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|why| format!("Could not prepare call-history query: {why}"))?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            raw_call_log_row(row).map(call_log_entry_from_raw)
        })
        .map_err(|why| format!("Could not query call history: {why}"))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|why| format!("Could not read call-history row: {why}"))?);
    }

    Ok((entries, total))
}

fn call_log_schema(conn: &Connection) -> Result<CallLogSchema, String> {
    if table_exists(conn, MODERN_CALL_TABLE)? {
        let columns = table_columns(conn, MODERN_CALL_TABLE)?;
        return Ok(CallLogSchema {
            table: MODERN_CALL_TABLE,
            id_expr: column_or_rowid(&columns, MODERN_ID_COLUMN),
            address_expr: column_or_null(&columns, MODERN_ADDRESS_COLUMN),
            date_expr: column_or_null(&columns, MODERN_DATE_COLUMN),
            duration_expr: column_or_null(&columns, MODERN_DURATION_COLUMN),
            originated_expr: column_or_null(&columns, MODERN_ORIGINATED_COLUMN),
            answered_expr: column_or_null(&columns, MODERN_ANSWERED_COLUMN),
            call_type_expr: column_or_null(&columns, MODERN_CALL_TYPE_COLUMN),
            service_expr: column_or_null(&columns, MODERN_SERVICE_COLUMN),
            legacy_flags_expr: "NULL",
            order_expr: column_or_rowid(&columns, MODERN_DATE_COLUMN),
        });
    }

    if table_exists(conn, LEGACY_CALL_TABLE)? {
        let columns = table_columns(conn, LEGACY_CALL_TABLE)?;
        return Ok(CallLogSchema {
            table: LEGACY_CALL_TABLE,
            id_expr: column_or_rowid(&columns, LEGACY_ID_COLUMN),
            address_expr: column_or_null(&columns, LEGACY_ADDRESS_COLUMN),
            date_expr: column_or_null(&columns, LEGACY_DATE_COLUMN),
            duration_expr: column_or_null(&columns, LEGACY_DURATION_COLUMN),
            originated_expr: "NULL",
            answered_expr: "NULL",
            call_type_expr: "NULL",
            service_expr: "NULL",
            legacy_flags_expr: column_or_null(&columns, LEGACY_FLAGS_COLUMN),
            order_expr: column_or_rowid(&columns, LEGACY_DATE_COLUMN),
        });
    }

    Err(format!(
        "Call-history database does not contain {MODERN_CALL_TABLE} or {LEGACY_CALL_TABLE}."
    ))
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        [table],
        |_| Ok(()),
    )
    .optional()
    .map(|row| row.is_some())
    .map_err(|why| format!("Could not inspect call-history tables: {why}"))
}

fn table_columns(conn: &Connection, table: &str) -> Result<HashSet<String>, String> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|why| format!("Could not inspect {table} columns: {why}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|why| format!("Could not read {table} columns: {why}"))?;

    let mut columns = HashSet::new();
    for row in rows {
        columns.insert(row.map_err(|why| format!("Could not read {table} column: {why}"))?);
    }
    Ok(columns)
}

fn column_or_null(columns: &HashSet<String>, column: &'static str) -> &'static str {
    if columns.contains(column) {
        column
    } else {
        "NULL"
    }
}

fn column_or_rowid(columns: &HashSet<String>, column: &'static str) -> &'static str {
    if columns.contains(column) {
        column
    } else {
        "ROWID"
    }
}

fn call_log_count(conn: &Connection, schema: &CallLogSchema) -> Result<i64, String> {
    let sql = format!("SELECT COUNT(*) FROM {}", schema.table);
    conn.query_row(&sql, [], |row| row.get::<_, i64>(0))
        .map_err(|why| format!("Could not count call-history rows: {why}"))
}

fn raw_call_log_row(row: &Row<'_>) -> rusqlite::Result<RawCallLogRow> {
    Ok(RawCallLogRow {
        id: optional_i64(row, CALL_ROW_ID_INDEX)?.unwrap_or_default(),
        address: optional_string(row, CALL_ADDRESS_INDEX)?,
        date: optional_f64(row, CALL_DATE_INDEX)?,
        duration: optional_f64(row, CALL_DURATION_INDEX)?,
        originated: optional_i64(row, CALL_ORIGINATED_INDEX)?,
        answered: optional_i64(row, CALL_ANSWERED_INDEX)?,
        call_type: optional_i64(row, CALL_TYPE_INDEX)?,
        service: optional_string(row, CALL_SERVICE_INDEX)?,
        legacy_flags: optional_i64(row, CALL_LEGACY_FLAGS_INDEX)?,
    })
}

fn optional_string(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<String>> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        ValueRef::Text(value) => Ok(Some(String::from_utf8_lossy(value).to_string())),
        ValueRef::Integer(value) => Ok(Some(value.to_string())),
        ValueRef::Real(value) => Ok(Some(value.to_string())),
        ValueRef::Blob(value) => Ok(Some(format!("{} bytes", value.len()))),
    }
}

fn optional_i64(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<i64>> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(value) => Ok(Some(value)),
        ValueRef::Real(value) => Ok(Some(value.round() as i64)),
        ValueRef::Text(value) => Ok(std::str::from_utf8(value)
            .ok()
            .and_then(|text| text.trim().parse::<i64>().ok())),
        ValueRef::Blob(_) => Ok(None),
    }
}

fn optional_f64(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<f64>> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(value) => Ok(Some(value as f64)),
        ValueRef::Real(value) => Ok(Some(value)),
        ValueRef::Text(value) => Ok(std::str::from_utf8(value)
            .ok()
            .and_then(|text| text.trim().parse::<f64>().ok())),
        ValueRef::Blob(_) => Ok(None),
    }
}

fn call_log_entry_from_raw(raw: RawCallLogRow) -> CallLogEntry {
    CallLogEntry {
        id: raw.id,
        started: format_apple_call_timestamp(raw.date),
        direction: call_direction(raw.originated, raw.answered, raw.legacy_flags),
        address: call_address(raw.address),
        duration: format_call_duration(raw.duration),
        service: service_label(raw.service),
        call_type: call_type_label(raw.call_type),
    }
}

fn call_direction(
    originated: Option<i64>,
    answered: Option<i64>,
    legacy_flags: Option<i64>,
) -> CallDirection {
    if let Some(flags) = legacy_flags {
        return match flags {
            LEGACY_INCOMING_FLAG => CallDirection::Incoming,
            LEGACY_OUTGOING_FLAG => CallDirection::Outgoing,
            LEGACY_BLOCKED_FLAG => CallDirection::Blocked,
            _ => CallDirection::Unknown,
        };
    }

    match (originated, answered) {
        (Some(1), _) => CallDirection::Outgoing,
        (Some(0), Some(0)) => CallDirection::Missed,
        (Some(0), _) => CallDirection::Incoming,
        _ => CallDirection::Unknown,
    }
}

fn call_address(address: Option<String>) -> String {
    address
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| CALL_LOG_NO_VALUE.to_string())
}

fn service_label(service: Option<String>) -> String {
    let Some(service) = service
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return CALL_LOG_NO_VALUE.to_string();
    };

    let normalized = service.to_ascii_lowercase();
    if normalized.contains("facetime") {
        "FaceTime".to_string()
    } else if normalized.contains("telephony") || normalized.contains("phone") {
        "Phone".to_string()
    } else {
        service
    }
}

fn call_type_label(call_type: Option<i64>) -> String {
    call_type
        .map(|value| format!("{CALL_LOG_TYPE_PREFIX} {value}"))
        .unwrap_or_else(|| CALL_LOG_NO_VALUE.to_string())
}

fn format_call_duration(duration: Option<f64>) -> String {
    let Some(duration) = duration else {
        return CALL_LOG_NO_VALUE.to_string();
    };
    let seconds = duration.max(0.0).round() as i64;
    let hours = seconds / SECONDS_PER_HOUR;
    let minutes = (seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    let seconds = seconds % SECONDS_PER_MINUTE;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn format_apple_call_timestamp(seconds_since_reference: Option<f64>) -> String {
    let Some(seconds_since_reference) = seconds_since_reference else {
        return CALL_LOG_NO_VALUE.to_string();
    };
    let unix_seconds = seconds_since_reference + APPLE_UNIX_OFFSET_SECONDS;
    if !unix_seconds.is_finite() {
        return CALL_LOG_NO_VALUE.to_string();
    }

    let whole = unix_seconds.trunc() as i64;
    let nanos = ((unix_seconds.fract().abs()) * NANOS_PER_SECOND).round() as u32;
    Local
        .timestamp_opt(whole, nanos.min(MAX_NANOSECOND_PART))
        .single()
        .map(|date| fmt_date(&date))
        .unwrap_or_else(|| CALL_LOG_NO_VALUE.to_string())
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
    config.options.query_context = qc;
}

fn apply_pdf_export_options(config: &mut Config, params: &ExportParams) {
    config.options.export_type = None;
    config.options.export_path = params.export_path.clone();
    config.options.attachment_manager =
        AttachmentManager::from(attachment_manager_mode(params.copy_method));
    config.options.no_lazy = params.no_lazy;
    config.options.custom_name = params.custom_name.clone();
    config.options.use_caller_id = params.use_caller_id;
    config.options.ignore_disk_space = params.ignore_disk_space;
    config.options.conversation_filter = None;
    config.options.show_progress = false;
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

    // PDF is rendered in-process per conversation (no exporter file writers).
    if params.format == FormatChoice::Pdf {
        export_pdf(config, &params, evt_tx, ctx);
        return;
    }

    let export_type = match params.format {
        FormatChoice::Html => ExportType::Html,
        FormatChoice::Txt => ExportType::Txt,
        FormatChoice::Pdf => unreachable!("handled above"),
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

/// Sanitize a conversation title into a safe file name stem.
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    let trimmed = if trimmed.is_empty() {
        "conversation"
    } else {
        trimmed
    };
    trimmed.chars().take(120).collect()
}

/// Render one styled PDF per selected conversation, fully in-process.
fn export_pdf(
    config: &mut Config,
    params: &ExportParams,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let export_path = params.export_path.clone();
    if let Err(why) = std::fs::create_dir_all(&export_path) {
        send(
            evt_tx,
            ctx,
            Event::ExportFailed(format!("Cannot create {}: {why}", export_path.display())),
        );
        return;
    }
    apply_pdf_export_options(config, params);

    // Determine which conversations to render.
    let all = build_conversations(config, &HashMap::new());
    let selected: HashSet<i32> = params
        .filters
        .selected_raw_chat_ids
        .iter()
        .copied()
        .collect();
    let chosen: Vec<&ConversationSummary> = all
        .iter()
        .filter(|c| selected.is_empty() || c.raw_chat_ids.iter().any(|id| selected.contains(id)))
        .collect();

    if chosen.is_empty() {
        send(
            evt_tx,
            ctx,
            Event::ExportFailed("No conversations matched the current selection.".into()),
        );
        return;
    }

    let total_convs = chosen.len();
    let mut produced = 0usize;
    let mut total_messages = 0usize;
    let mut used_names: HashSet<String> = HashSet::new();

    for (idx, conv) in chosen.iter().enumerate() {
        send(
            evt_tx,
            ctx,
            Event::Progress {
                current: idx as u64,
                total: total_convs as u64,
            },
        );

        // Build a query context scoped to this conversation + date range.
        let mut qc = QueryContext {
            start: params.filters.start_ns,
            end: params.filters.end_ns,
            ..Default::default()
        };
        qc.set_selected_chat_ids(conv.raw_chat_ids.iter().copied().collect());

        let messages = match collect_pdf_messages(config, &qc, usize::MAX) {
            Ok(m) => m,
            Err(why) => {
                send(evt_tx, ctx, Event::ExportFailed(why));
                return;
            }
        };
        if messages.is_empty() {
            continue;
        }
        total_messages += messages.len();

        // Ensure a unique file name.
        let mut stem = sanitize_filename(&conv.title);
        let base = stem.clone();
        let mut n = 1;
        while !used_names.insert(stem.clone()) {
            n += 1;
            stem = format!("{base} ({n})");
        }
        let pdf_path = export_path.join(format!("{stem}.pdf"));

        if let Err(why) = crate::pdf::render(&messages, &conv.title, &pdf_path) {
            send(evt_tx, ctx, Event::ExportFailed(why));
            return;
        }
        produced += 1;
    }

    send(
        evt_tx,
        ctx,
        Event::Progress {
            current: total_convs as u64,
            total: total_convs as u64,
        },
    );

    if produced == 0 {
        send(
            evt_tx,
            ctx,
            Event::ExportFailed("No messages matched the current filters.".into()),
        );
    } else {
        send(
            evt_tx,
            ctx,
            Event::ExportDone {
                path: export_path.clone(),
                summary: format!(
                    "Exported {produced} PDF file{} ({total_messages} messages) to {}",
                    if produced == 1 { "" } else { "s" },
                    export_path.display()
                ),
            },
        );
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
        };
        Config::new(options).expect("failed to open bundled test database")
    }

    fn temp_test_db(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("{name}-{}-{stamp}.db", std::process::id()))
    }

    #[test]
    fn call_history_backup_hash_matches_ios_manifest_id() {
        assert_eq!(
            backup_file_id(
                CALL_HISTORY_MODERN_SOURCE.domain,
                CALL_HISTORY_MODERN_SOURCE.relative_path
            ),
            "5a4935c78a5255723f707230a451d79c540d2741"
        );
        assert_eq!(
            backup_file_id(
                CALL_HISTORY_LEGACY_SOURCE.domain,
                CALL_HISTORY_LEGACY_SOURCE.relative_path
            ),
            "2b2b0084a1bc3a5ac8c27afdf14afb42c61a19ca"
        );
    }

    #[test]
    fn parses_modern_call_history_rows() {
        let path = temp_test_db("modern-call-history");
        let conn = Connection::open(&path).expect("create call-history db");
        conn.execute_batch(
            "
            CREATE TABLE ZCALLRECORD (
                Z_PK INTEGER PRIMARY KEY,
                ZADDRESS TEXT,
                ZDATE REAL,
                ZDURATION INTEGER,
                ZORIGINATED INTEGER,
                ZANSWERED INTEGER,
                ZCALLTYPE INTEGER,
                ZSERVICE_PROVIDER TEXT
            );
            INSERT INTO ZCALLRECORD
                (Z_PK, ZADDRESS, ZDATE, ZDURATION, ZORIGINATED, ZANSWERED, ZCALLTYPE, ZSERVICE_PROVIDER)
            VALUES
                (1, '+15551230000', 700000000, 65, 1, 1, 1, 'com.apple.Telephony'),
                (2, '+15557650000', 700000010, 0, 0, 0, 8, 'com.apple.facetime');
            ",
        )
        .expect("seed call-history db");
        drop(conn);

        let (entries, total) = collect_call_logs_from_db(&path, CALL_HISTORY_SOURCES.len())
            .expect("collect call logs");
        let _ = fs::remove_file(&path);

        assert_eq!(total, 2);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].address, "+15557650000");
        assert_eq!(entries[0].direction, CallDirection::Missed);
        assert_eq!(entries[0].duration, "0:00");
        assert_eq!(entries[0].service, "FaceTime");
        assert_eq!(entries[1].direction, CallDirection::Outgoing);
        assert_eq!(entries[1].duration, "1:05");
        assert_eq!(entries[1].service, "Phone");
    }

    #[test]
    fn parses_legacy_call_history_rows() {
        let path = temp_test_db("legacy-call-history");
        let conn = Connection::open(&path).expect("create legacy call-history db");
        conn.execute_batch(
            "
            CREATE TABLE call (
                ROWID INTEGER PRIMARY KEY,
                address TEXT,
                date INTEGER,
                duration INTEGER,
                flags INTEGER
            );
            INSERT INTO call (ROWID, address, date, duration, flags)
            VALUES
                (1, '+15551230000', 600000000, 3605, 5),
                (2, '+15557650000', 600000010, 0, 8);
            ",
        )
        .expect("seed legacy call-history db");
        drop(conn);

        let (entries, total) = collect_call_logs_from_db(&path, CALL_HISTORY_SOURCES.len())
            .expect("collect legacy call logs");
        let _ = fs::remove_file(&path);

        assert_eq!(total, 2);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].direction, CallDirection::Blocked);
        assert_eq!(entries[1].direction, CallDirection::Outgoing);
        assert_eq!(entries[1].duration, "1:00:05");
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
