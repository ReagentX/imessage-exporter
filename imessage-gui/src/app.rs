//! The eframe/egui application: all UI state and rendering.

use std::{
    cmp::Reverse,
    collections::{HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
        Arc,
    },
    time::Duration,
};

use eframe::egui;

use imessage_database::{tables::table::DEFAULT_PATH_IOS, util::dirs::home};

use crate::{
    backend::{self, Command, Event},
    model::*,
    settings,
    theme::{self, layout, progress, timing},
};

/// Maximum number of messages pulled into the in-app preview.
const PREVIEW_LIMIT: usize = 800;
/// Maximum number of call-history rows pulled into the in-app tab.
const CALL_LOG_LIMIT: usize = 1_000;
/// Maximum number of activity-log lines retained.
const LOG_LIMIT: usize = 300;
const CALL_LOG_CSV_FILE_NAME: &str = "call_logs.csv";
const CALL_LOG_CSV_HEADER: &str = "Started,Direction,Address,Duration,Service,Type\n";
const CSV_QUOTE: char = '"';
const CSV_COMMA: char = ',';
const CSV_NEWLINE: char = '\n';
const CSV_QUOTE_ESCAPE: &str = "\"\"";
const MACOS_CHAT_DB_FILE_NAME: &str = "chat.db";
const LOOSE_IOS_MESSAGES_DB_FILE_NAME: &str = "sms.db";
const DATABASE_SCAN_MAX_DEPTH: usize = 4;
const DATABASE_SCAN_MAX_ENTRIES: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveTab {
    Messages,
    CallLogs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedSource {
    path: PathBuf,
    platform: PlatformChoice,
    description: &'static str,
}

pub struct App {
    cmd_tx: Sender<Command>,
    evt_rx: Receiver<Event>,

    // Source selection
    backup_path: String,
    platform: PlatformChoice,
    password: String,
    show_password: bool,
    contacts_path: String,
    attachment_root: String,
    show_advanced_source: bool,

    // Loaded-state info
    opened: bool,
    opened_platform: Option<PlatformChoice>,
    summary: String,
    date_range: Option<(String, String)>,
    total_messages: usize,

    // Conversations
    conversations: Vec<ConversationSummary>,
    selected: HashSet<i32>,
    search: String,

    // Date/time range filters
    start_enabled: bool,
    start_date: String,
    start_time: String,
    end_enabled: bool,
    end_date: String,
    end_time: String,
    participant_filter: String,

    // Export options
    format: FormatChoice,
    copy_method: CopyMethod,
    no_lazy: bool,
    name_mode: NameMode,
    custom_name: String,
    ignore_disk_space: bool,
    export_path: String,

    // Preview
    active_tab: ActiveTab,
    preview: Vec<PreviewMessage>,
    preview_total: i64,
    preview_note: String,

    // Call logs
    call_logs: Vec<CallLogEntry>,
    call_log_total: i64,
    call_log_note: String,

    // Conversation list sorting
    sort_by_count: bool,

    // Progress / results / log
    busy: bool,
    busy_label: String,
    progress: Option<(u64, u64)>,
    log: Vec<String>,
    show_activity_log: bool,
    activity_log_close_requested: Arc<AtomicBool>,
    last_export: Option<PathBuf>,
    error: Option<String>,
    settings_msg: Option<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::configure(&cc.egui_ctx);

        let (cmd_tx, evt_rx) = backend::spawn(cc.egui_ctx.clone());

        // Load persisted settings (portable: stored next to the executable).
        let s = settings::Settings::load();

        let export_path = if s.export_path.trim().is_empty() {
            let mut p = PathBuf::from(home());
            p.push("imessage_export");
            p.display().to_string()
        } else {
            s.export_path.clone()
        };

        Self {
            cmd_tx,
            evt_rx,
            backup_path: s.backup_path,
            platform: s.platform.into(),
            password: String::new(),
            show_password: false,
            contacts_path: s.contacts_path,
            attachment_root: s.attachment_root,
            show_advanced_source: false,
            opened: false,
            opened_platform: None,
            summary: String::new(),
            date_range: None,
            total_messages: 0,
            conversations: Vec::new(),
            selected: HashSet::new(),
            search: String::new(),
            start_enabled: s.start_enabled,
            start_date: s.start_date,
            start_time: s.start_time,
            end_enabled: s.end_enabled,
            end_date: s.end_date,
            end_time: s.end_time,
            participant_filter: String::new(),
            format: s.format.into(),
            copy_method: s.copy_method.into(),
            no_lazy: s.no_lazy,
            name_mode: s.name_mode.into(),
            custom_name: s.custom_name,
            ignore_disk_space: s.ignore_disk_space,
            export_path,
            active_tab: ActiveTab::Messages,
            preview: Vec::new(),
            preview_total: 0,
            preview_note: String::new(),
            call_logs: Vec::new(),
            call_log_total: 0,
            call_log_note: String::new(),
            sort_by_count: s.sort_by_count,
            busy: false,
            busy_label: String::new(),
            progress: None,
            log: Vec::new(),
            show_activity_log: s.show_activity_log,
            activity_log_close_requested: Arc::new(AtomicBool::new(false)),
            last_export: None,
            error: None,
            settings_msg: None,
        }
    }

    /// Capture the current UI state as persistable settings.
    fn current_settings(&self) -> settings::Settings {
        settings::Settings {
            backup_path: self.backup_path.clone(),
            platform: self.platform.into(),
            contacts_path: self.contacts_path.clone(),
            attachment_root: self.attachment_root.clone(),
            format: self.format.into(),
            copy_method: self.copy_method.into(),
            name_mode: self.name_mode.into(),
            custom_name: self.custom_name.clone(),
            no_lazy: self.no_lazy,
            ignore_disk_space: self.ignore_disk_space,
            export_path: self.export_path.clone(),
            start_enabled: self.start_enabled,
            start_date: self.start_date.clone(),
            start_time: self.start_time.clone(),
            end_enabled: self.end_enabled,
            end_date: self.end_date.clone(),
            end_time: self.end_time.clone(),
            sort_by_count: self.sort_by_count,
            show_activity_log: self.show_activity_log,
        }
    }

    fn save_settings(&mut self) {
        match self.current_settings().save() {
            Ok(path) => self.settings_msg = Some(format!("Settings saved to {}", path.display())),
            Err(e) => self.settings_msg = Some(format!("Could not save settings: {e}")),
        }
    }

    fn push_log(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > LOG_LIMIT {
            let overflow = self.log.len() - LOG_LIMIT;
            self.log.drain(0..overflow);
        }
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.evt_rx.try_recv() {
            match event {
                Event::Status(s) => {
                    self.busy_label = s.clone();
                    self.push_log(s);
                }
                Event::Opened {
                    conversations,
                    date_range,
                    total_messages,
                    summary,
                    platform,
                } => {
                    self.conversations = conversations;
                    self.selected.clear();
                    self.date_range = date_range;
                    self.total_messages = total_messages;
                    self.summary = summary.clone();
                    self.opened = true;
                    self.opened_platform = Some(platform);
                    self.busy = false;
                    self.progress = None;
                    self.preview.clear();
                    self.preview_note.clear();
                    self.call_logs.clear();
                    self.call_log_total = 0;
                    self.call_log_note.clear();
                    self.push_log(format!("Opened: {summary}"));
                }
                Event::OpenFailed(e) => {
                    self.busy = false;
                    self.opened = false;
                    self.opened_platform = None;
                    self.call_logs.clear();
                    self.call_log_total = 0;
                    self.call_log_note.clear();
                    self.error = Some(e.clone());
                    self.push_log(format!("Open failed: {e}"));
                }
                Event::Preview { messages, total } => {
                    self.preview_total = total;
                    self.preview_note = format!(
                        "Showing {} of {} matching message{}",
                        messages.len(),
                        total,
                        if total == 1 { "" } else { "s" }
                    );
                    self.preview = messages;
                    self.busy = false;
                }
                Event::PreviewFailed(e) => {
                    self.busy = false;
                    self.error = Some(e.clone());
                    self.push_log(format!("Preview failed: {e}"));
                }
                Event::Progress { current, total } => {
                    self.progress = Some((current, total));
                }
                Event::ExportDone { path, summary } => {
                    self.busy = false;
                    self.progress = None;
                    self.last_export = Some(path);
                    self.error = None;
                    self.push_log(summary);
                }
                Event::ExportFailed(e) => {
                    self.busy = false;
                    self.progress = None;
                    self.error = Some(e.clone());
                    self.push_log(format!("Export failed: {e}"));
                }
                Event::HtmlPreviewReady(p) => {
                    self.busy = false;
                    self.push_log(format!("Opened HTML preview in browser: {}", p.display()));
                }
                Event::HtmlPreviewFailed(e) => {
                    self.busy = false;
                    self.error = Some(e.clone());
                    self.push_log(format!("HTML preview failed: {e}"));
                }
                Event::CallLogsLoaded {
                    entries,
                    total,
                    source,
                } => {
                    self.call_log_total = total;
                    self.call_log_note = format!(
                        "Showing {} of {} call{} from {}",
                        entries.len(),
                        total,
                        if total == 1 { "" } else { "s" },
                        source
                    );
                    self.call_logs = entries;
                    self.busy = false;
                    self.error = None;
                    self.push_log(format!("Loaded call logs: {}", self.call_log_note));
                }
                Event::CallLogsFailed(e) => {
                    self.busy = false;
                    self.error = Some(e.clone());
                    self.push_log(format!("Call logs failed: {e}"));
                }
            }
        }
    }

    // MARK: Command builders

    fn build_filters(&self) -> Result<Filters, String> {
        let mut selected_raw = Vec::new();
        if !self.selected.is_empty() {
            for c in &self.conversations {
                if self.selected.contains(&c.id) {
                    selected_raw.extend(c.raw_chat_ids.iter().copied());
                }
            }
        }

        let start_ns = if self.start_enabled {
            Some(parse_local_timestamp(&self.start_date, &self.start_time)?)
        } else {
            None
        };
        let end_ns = if self.end_enabled {
            Some(parse_local_timestamp(&self.end_date, &self.end_time)?)
        } else {
            None
        };

        let conversation_filter = {
            let t = self.participant_filter.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };

        Ok(Filters {
            selected_raw_chat_ids: selected_raw,
            conversation_filter,
            start_ns,
            end_ns,
        })
    }

    fn build_export_params(&self) -> Result<ExportParams, String> {
        let filters = self.build_filters()?;
        if self.export_path.trim().is_empty() {
            return Err("Choose an export folder first.".into());
        }
        let custom_name = if self.name_mode == NameMode::Custom {
            let n = self.custom_name.trim();
            if n.is_empty() {
                return Err("Enter a custom name, or choose a different name option.".into());
            }
            Some(n.to_string())
        } else {
            None
        };
        Ok(ExportParams {
            filters,
            format: self.format,
            export_path: PathBuf::from(self.export_path.trim()),
            copy_method: self.copy_method,
            no_lazy: self.no_lazy,
            custom_name,
            use_caller_id: self.name_mode == NameMode::CallerId,
            ignore_disk_space: self.ignore_disk_space,
        })
    }

    fn start_busy(&mut self, label: &str) {
        self.busy = true;
        self.busy_label = label.to_string();
        self.error = None;
    }

    fn do_open(&mut self) {
        if self.backup_path.trim().is_empty() {
            self.error = Some("Choose a backup folder, chat.db, or sms.db file first.".into());
            return;
        }
        let params = OpenParams {
            db_path: PathBuf::from(self.backup_path.trim()),
            platform: self.platform,
            password: {
                let p = self.password.trim();
                if p.is_empty() {
                    None
                } else {
                    Some(p.to_string())
                }
            },
            contacts_path: {
                let c = self.contacts_path.trim();
                if c.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(c))
                }
            },
            attachment_root: {
                let a = self.attachment_root.trim();
                if a.is_empty() {
                    None
                } else {
                    Some(a.to_string())
                }
            },
        };
        self.start_busy("Opening backup …");
        let _ = self.cmd_tx.send(Command::Open(params));
    }

    fn do_preview(&mut self) {
        match self.build_filters() {
            Ok(filters) => {
                self.start_busy("Loading preview …");
                let _ = self.cmd_tx.send(Command::Preview {
                    filters,
                    limit: PREVIEW_LIMIT,
                });
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn do_html_preview(&mut self) {
        match self.build_filters() {
            Ok(filters) => {
                self.start_busy("Rendering HTML preview …");
                let _ = self.cmd_tx.send(Command::HtmlPreview { filters });
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn do_export(&mut self) {
        match self.build_export_params() {
            Ok(params) => {
                self.start_busy("Exporting …");
                let _ = self.cmd_tx.send(Command::Export(params));
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn do_load_call_logs(&mut self) {
        if !self.opened {
            self.error = Some("Open an iOS backup before loading call logs.".into());
            return;
        }
        if self.opened_platform != Some(PlatformChoice::IOS) {
            self.error = Some("Call logs are available from iOS backup folders only.".into());
            return;
        }

        self.start_busy("Loading call logs ...");
        let _ = self.cmd_tx.send(Command::LoadCallLogs {
            limit: CALL_LOG_LIMIT,
        });
    }

    fn save_call_logs_csv(&mut self) {
        if self.call_logs.is_empty() {
            self.error = Some("Load call logs before saving CSV.".into());
            return;
        }

        let Some(path) = rfd::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .set_file_name(CALL_LOG_CSV_FILE_NAME)
            .save_file()
        else {
            return;
        };

        match fs::write(&path, call_logs_csv(&self.call_logs)) {
            Ok(()) => {
                self.error = None;
                self.push_log(format!("Saved call logs CSV: {}", path.display()));
            }
            Err(why) => {
                self.error = Some(format!("Could not save {}: {why}", path.display()));
            }
        }
    }

    // MARK: Panels

    fn top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("source")
            .frame(theme::panel_frame())
            .show(ctx, |ui| {
                theme::panel_header(ui, "iMessage Exporter", |ui| {
                    if self.opened {
                        ui.label(theme::muted_text(&self.summary));
                    } else {
                        ui.label(theme::muted_text("Select a source to begin"));
                    }
                });

                let mut open_from_enter = false;
                theme::control_row(ui, |ui| {
                    theme::field_label(ui, "Source");
                    let source_response = theme::add_text_field(
                        ui,
                        &mut self.backup_path,
                        layout::SOURCE_FIELD_WIDTH,
                        "iOS backup folder, chat.db, or sms.db",
                    );
                    open_from_enter |= !self.busy
                        && source_response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));

                    if theme::add_enabled_button(ui, !self.busy, "📁 Folder…").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            match resolve_source_folder(&dir) {
                                Ok(source) => {
                                    self.backup_path = source.path.display().to_string();
                                    self.platform = source.platform;
                                    self.push_log(format!(
                                        "Selected {}: {}",
                                        source.description,
                                        source.path.display()
                                    ));
                                    self.do_open();
                                }
                                Err(why) => {
                                    self.backup_path = dir.display().to_string();
                                    self.error = Some(why);
                                }
                            }
                        }
                    }
                    if theme::add_enabled_button(ui, !self.busy, "📄 chat.db…").clicked() {
                        if let Some(file) = rfd::FileDialog::new()
                            .add_filter("iMessage database", &["db"])
                            .add_filter("All files", &["*"])
                            .pick_file()
                        {
                            self.backup_path = file.display().to_string();
                            self.platform = PlatformChoice::MacOS;
                            self.do_open();
                        }
                    }
                });

                theme::control_row(ui, |ui| {
                    theme::field_label(ui, "Platform");
                    let platform_combo = egui::ComboBox::from_id_salt("platform")
                        .selected_text(self.platform.label())
                        .show_ui(ui, |ui| {
                            for choice in [
                                PlatformChoice::Auto,
                                PlatformChoice::MacOS,
                                PlatformChoice::IOS,
                            ] {
                                ui.selectable_value(&mut self.platform, choice, choice.label());
                            }
                        });
                    theme::paint_dropdown_border(ui, &platform_combo.response);

                    if self.platform != PlatformChoice::MacOS {
                        theme::field_label(ui, "Password");
                        let password_response = theme::add_password_field(
                            ui,
                            &mut self.password,
                            layout::PASSWORD_FIELD_WIDTH,
                            "if encrypted",
                            !self.show_password,
                        );
                        open_from_enter |= !self.busy
                            && password_response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        theme::checkbox(ui, &mut self.show_password, "show");
                    }
                });

                if open_from_enter {
                    self.do_open();
                }

                theme::gap(ui, layout::ADVANCED_SOURCE_TOP_GAP);
                theme::control_row(ui, |ui| {
                    theme::field_label(ui, "");
                    theme::checkbox(
                        ui,
                        &mut self.show_advanced_source,
                        "Advanced source options",
                    );
                });
                if self.show_advanced_source {
                    theme::control_row(ui, |ui| {
                        theme::field_label(ui, "Contacts DB");
                        theme::add_text_field(
                            ui,
                            &mut self.contacts_path,
                            layout::ADVANCED_SOURCE_FIELD_WIDTH,
                            "optional AddressBook database",
                        );
                        if theme::add_button(ui, "Browse…").clicked() {
                            if let Some(file) = rfd::FileDialog::new().pick_file() {
                                self.contacts_path = file.display().to_string();
                            }
                        }
                    });
                    theme::control_row(ui, |ui| {
                        theme::field_label(ui, "Attachment root");
                        theme::add_text_field(
                            ui,
                            &mut self.attachment_root,
                            layout::ADVANCED_SOURCE_FIELD_WIDTH,
                            "optional (macOS only)",
                        );
                        if theme::add_button(ui, "Browse…").clicked() {
                            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                self.attachment_root = dir.display().to_string();
                            }
                        }
                    });
                }
                theme::gap(ui, layout::ROW_GAP);
            });
    }

    fn left_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("conversations")
            .resizable(true)
            .default_width(layout::LEFT_PANEL_DEFAULT_WIDTH)
            .width_range(layout::LEFT_PANEL_MIN_WIDTH..=layout::LEFT_PANEL_MAX_WIDTH)
            .frame(theme::panel_frame())
            .show(ctx, |ui| {
                theme::panel_header(ui, "Conversations", |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(theme::muted_text(format!(
                            "{} selected",
                            self.selected.len()
                        )));
                    });
                });

                theme::control_row(ui, |ui| {
                    theme::field_label(ui, "Search");
                    theme::add_text_field(
                        ui,
                        &mut self.search,
                        layout::CONVERSATION_SEARCH_WIDTH,
                        "filter by name, number, or email",
                    );
                });

                let needle = self.search.trim().to_lowercase();
                let matches = |c: &ConversationSummary| {
                    needle.is_empty()
                        || c.title.to_lowercase().contains(&needle)
                        || c.participants.to_lowercase().contains(&needle)
                };

                theme::control_row(ui, |ui| {
                    theme::field_label(ui, "Selection");
                    if theme::add_small_button(ui, "Select all").clicked() {
                        for c in self.conversations.iter().filter(|c| matches(c)) {
                            self.selected.insert(c.id);
                        }
                    }
                    if theme::add_small_button(ui, "Clear").clicked() {
                        self.selected.clear();
                    }
                });
                theme::control_row(ui, |ui| {
                    theme::field_label(ui, "");
                    theme::checkbox(ui, &mut self.sort_by_count, "Sort by count");
                });
                if self.selected.is_empty() {
                    ui.label(theme::small_muted_text(
                        "(none selected = all conversations)",
                    ));
                }

                theme::inline_separator(ui);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.conversations.is_empty() {
                            ui.label(theme::muted_text("Open a source to list conversations."));
                        }
                        // Collect first to avoid borrowing self while mutating selection.
                        let mut visible: Vec<(i32, String, String, i64)> = self
                            .conversations
                            .iter()
                            .filter(|c| matches(c))
                            .map(|c| {
                                (
                                    c.id,
                                    c.title.clone(),
                                    c.participants.clone(),
                                    c.message_count,
                                )
                            })
                            .collect();
                        if self.sort_by_count {
                            visible.sort_by_key(|item| Reverse(item.3));
                        }
                        for (id, title, participants, count) in visible {
                            let mut checked = self.selected.contains(&id);
                            let label = format!("{title}  ·  {count}");
                            let resp = theme::checkbox(ui, &mut checked, label);
                            if !participants.is_empty() && participants != title {
                                resp.on_hover_text(format!("{participants}\n{count} messages"));
                            }
                            if checked {
                                self.selected.insert(id);
                            } else {
                                self.selected.remove(&id);
                            }
                        }
                    });

                if let Some((first, last)) = &self.date_range {
                    theme::group_gap(ui);
                    theme::group_header(ui, "Database date range");
                    ui.label(theme::small_muted_text(format!("{first}\n→ {last}")));
                }
            });
    }

    fn export_controls(&mut self, ui: &mut egui::Ui) {
        theme::panel_header(ui, "Export", |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                theme::checkbox(ui, &mut self.show_activity_log, "Activity log");
            });
        });

        theme::group_header(ui, "Filters");
        theme::control_row(ui, |ui| {
            theme::field_label(ui, "Date range");
            theme::checkbox(ui, &mut self.start_enabled, "From");
            ui.add_enabled_ui(self.start_enabled, |ui| {
                theme::add_text_field(
                    ui,
                    &mut self.start_date,
                    layout::DATE_FIELD_WIDTH,
                    "YYYY-MM-DD",
                );
                theme::add_text_field(ui, &mut self.start_time, layout::TIME_FIELD_WIDTH, "HH:MM");
            });
            theme::checkbox(ui, &mut self.end_enabled, "To");
            ui.add_enabled_ui(self.end_enabled, |ui| {
                theme::add_text_field(
                    ui,
                    &mut self.end_date,
                    layout::DATE_FIELD_WIDTH,
                    "YYYY-MM-DD",
                );
                theme::add_text_field(ui, &mut self.end_time, layout::TIME_FIELD_WIDTH, "HH:MM");
            });
        });
        theme::control_row(ui, |ui| {
            theme::field_label(ui, "Participants");
            theme::add_text_field(
                ui,
                &mut self.participant_filter,
                layout::PARTICIPANT_FILTER_WIDTH,
                "name,number,email",
            )
            .on_hover_text(
                "Comma-separated. Used only when no conversations are checked on the left.",
            );
        });

        theme::group_header(ui, "Output");
        theme::control_row(ui, |ui| {
            theme::field_label(ui, "Format");
            let format_combo = egui::ComboBox::from_id_salt("format")
                .width(layout::FORMAT_WIDTH)
                .selected_text(self.format.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.format, FormatChoice::Html, "HTML");
                    ui.selectable_value(&mut self.format, FormatChoice::Txt, "Text");
                    ui.selectable_value(&mut self.format, FormatChoice::Pdf, "PDF");
                });
            theme::paint_dropdown_border(ui, &format_combo.response);
            theme::field_label(ui, "Attachments");
            let copy_combo = egui::ComboBox::from_id_salt("copy")
                .width(layout::ATTACHMENT_MODE_WIDTH)
                .selected_text(self.copy_method.label())
                .show_ui(ui, |ui| {
                    for m in [
                        CopyMethod::Disabled,
                        CopyMethod::Clone,
                        CopyMethod::Basic,
                        CopyMethod::Full,
                    ] {
                        ui.selectable_value(&mut self.copy_method, m, m.label());
                    }
                });
            theme::paint_dropdown_border(ui, &copy_combo.response);
        });

        theme::control_row(ui, |ui| {
            theme::field_label(ui, "Owner");
            let name_combo = egui::ComboBox::from_id_salt("name")
                .width(layout::NAME_MODE_WIDTH)
                .selected_text(match self.name_mode {
                    NameMode::Me => "\"Me\"",
                    NameMode::Custom => "Custom",
                    NameMode::CallerId => "Caller ID",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.name_mode, NameMode::Me, "\"Me\"");
                    ui.selectable_value(&mut self.name_mode, NameMode::Custom, "Custom");
                    ui.selectable_value(&mut self.name_mode, NameMode::CallerId, "Caller ID");
                });
            theme::paint_dropdown_border(ui, &name_combo.response);
            if self.name_mode == NameMode::Custom {
                theme::add_text_field(
                    ui,
                    &mut self.custom_name,
                    layout::CUSTOM_NAME_WIDTH,
                    "custom name",
                );
            }
            theme::field_label(ui, "Options");
            if self.format == FormatChoice::Html {
                theme::checkbox(ui, &mut self.no_lazy, "No lazy images").on_hover_text(
                    "Disable loading=\"lazy\" so images render in headless captures.",
                );
            }
            theme::checkbox(ui, &mut self.ignore_disk_space, "Ignore disk check");
        });

        theme::group_header(ui, "Destination");
        theme::control_row(ui, |ui| {
            theme::field_label(ui, "Export to");
            theme::add_text_field(
                ui,
                &mut self.export_path,
                layout::EXPORT_PATH_WIDTH,
                "output folder",
            );
            if theme::add_button(ui, "Browse…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.export_path = dir.display().to_string();
                }
            }
        });

        theme::group_header(ui, "Actions");
        theme::control_row(ui, |ui| {
            theme::field_label(ui, "");
            let enabled = self.opened && !self.busy;
            if theme::add_enabled_button(ui, enabled, "👁 Preview").clicked() {
                self.do_preview();
            }
            if theme::add_enabled_button(
                ui,
                enabled && self.format == FormatChoice::Html,
                "🌐 Open HTML preview",
            )
            .on_hover_text("Render the current selection to HTML and open it in your browser")
            .clicked()
            {
                self.do_html_preview();
            }
            if theme::add_enabled_button(ui, enabled, "⬇ Export").clicked() {
                self.do_export();
            }
            if let Some(path) = self.last_export.clone() {
                if theme::add_button(ui, "📂 Open last export").clicked() {
                    let _ = open::that(&path);
                }
            }
            if theme::add_button(ui, "💾 Save settings")
                .on_hover_text("Save current options beside the app (portable)")
                .clicked()
            {
                self.save_settings();
            }
        });

        let has_status = self.busy
            || self.progress.is_some()
            || self.error.is_some()
            || self.settings_msg.is_some();
        if self.busy || self.progress.is_some() {
            theme::control_row(ui, |ui| {
                theme::group_label(ui, "Status");
                theme::field_label(ui, "Progress");
                if self.busy {
                    ui.spinner();
                    ui.label(&self.busy_label);
                }
                if let Some((cur, total)) = self.progress {
                    let frac = if total > 0 {
                        (cur as f32 / total as f32)
                            .clamp(progress::MIN_FRACTION, progress::MAX_FRACTION)
                    } else {
                        progress::MIN_FRACTION
                    };
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .desired_width(layout::PROGRESS_BAR_WIDTH)
                            .text(format!("{cur}/{total}")),
                    );
                }
            });
        }

        if let Some(err) = self.error.clone() {
            theme::control_row(ui, |ui| {
                theme::group_label(ui, if has_status { "Status" } else { "" });
                theme::field_label(ui, "Message");
                ui.label(theme::error_text(format!("⚠ {err}")));
            });
        }
        if let Some(msg) = self.settings_msg.clone() {
            theme::control_row(ui, |ui| {
                theme::group_label(ui, if has_status { "Status" } else { "" });
                theme::field_label(ui, "Message");
                ui.label(theme::small_muted_text(msg));
            });
        }

        theme::gap(ui, layout::ROW_GAP);
    }

    fn activity_log_viewport(&self, ctx: &egui::Context) {
        let log_lines = self.log.clone();
        let close_requested = self.activity_log_close_requested.clone();

        ctx.show_viewport_deferred(
            theme::activity_log_viewport_id(),
            theme::activity_log_viewport_builder(ctx),
            move |ctx, _class| {
                theme::configure(ctx);
                if ctx.input(|input| input.viewport().close_requested()) {
                    close_requested.store(true, Ordering::Relaxed);
                }

                theme::activity_log_viewport_panel(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            if log_lines.is_empty() {
                                ui.label(theme::muted_text("No activity yet."));
                            } else {
                                for line in &log_lines {
                                    ui.label(theme::log_text(line));
                                }
                            }
                        });
                });
            },
        );
    }

    fn central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(theme::content_frame())
            .show(ctx, |ui| {
                self.tab_bar(ui);
                match self.active_tab {
                    ActiveTab::Messages => self.messages_tab(ui),
                    ActiveTab::CallLogs => self.call_logs_tab(ui),
                }
            });
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        theme::control_row(ui, |ui| {
            if theme::add_tab_button(ui, self.active_tab == ActiveTab::Messages, "Messages")
                .clicked()
            {
                self.active_tab = ActiveTab::Messages;
            }
            if theme::add_tab_button(ui, self.active_tab == ActiveTab::CallLogs, "Call logs")
                .clicked()
            {
                self.active_tab = ActiveTab::CallLogs;
            }
        });
        theme::inline_separator(ui);
        theme::gap(ui, layout::ROW_GAP);
    }

    fn messages_tab(&mut self, ui: &mut egui::Ui) {
        let (preview_height, _export_height) = theme::preview_export_heights(ui.available_height());
        theme::fixed_height_area(ui, preview_height, |ui| {
            self.preview_contents(ui);
        });
        theme::inline_separator(ui);
        let export_height = ui.available_height();
        theme::scrollable_fixed_height_area(
            ui,
            export_height,
            theme::ids::EXPORT_CONTROLS_SCROLL,
            |ui| {
                self.export_controls(ui);
            },
        );
    }

    fn call_logs_tab(&mut self, ui: &mut egui::Ui) {
        theme::panel_header(ui, "Call Logs", |ui| {
            if !self.call_log_note.is_empty() {
                ui.label(theme::muted_text(&self.call_log_note));
            }
        });

        theme::control_row(ui, |ui| {
            theme::field_label(ui, "Actions");
            let load_enabled =
                self.opened && self.opened_platform == Some(PlatformChoice::IOS) && !self.busy;
            if theme::add_enabled_button(ui, load_enabled, "Load call logs").clicked() {
                self.do_load_call_logs();
            }
            if theme::add_enabled_button(ui, !self.call_logs.is_empty(), "Save CSV").clicked() {
                self.save_call_logs_csv();
            }
        });

        if self.opened && self.opened_platform != Some(PlatformChoice::IOS) {
            ui.label(theme::small_muted_text(
                "Open an iOS backup folder to read call history.",
            ));
        } else if !self.opened {
            ui.label(theme::small_muted_text(
                "Open an iOS backup folder, then load call logs.",
            ));
        }

        theme::inline_separator(ui);
        self.call_log_table(ui);
    }

    fn call_log_table(&mut self, ui: &mut egui::Ui) {
        theme::fixed_height_area(ui, ui.available_height(), |ui| {
            if self.call_logs.is_empty() {
                theme::centered_preview_message(ui, "No call logs loaded.");
                return;
            }

            egui::ScrollArea::both()
                .id_salt(theme::ids::CALL_LOGS_SCROLL)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new(theme::ids::CALL_LOGS_GRID)
                        .striped(true)
                        .spacing(egui::vec2(
                            layout::CALL_LOG_COLUMN_GAP,
                            layout::COMPACT_ROW_GAP,
                        ))
                        .min_col_width(layout::CALL_LOG_MIN_COLUMN_WIDTH)
                        .show(ui, |ui| {
                            ui.label(theme::strong_small_text("Started"));
                            ui.label(theme::strong_small_text("Direction"));
                            ui.label(theme::strong_small_text("Address"));
                            ui.label(theme::strong_small_text("Duration"));
                            ui.label(theme::strong_small_text("Service"));
                            ui.label(theme::strong_small_text("Type"));
                            ui.end_row();

                            for entry in &self.call_logs {
                                ui.label(&entry.started);
                                ui.label(entry.direction.label());
                                ui.label(&entry.address);
                                ui.label(&entry.duration);
                                ui.label(&entry.service);
                                ui.label(&entry.call_type);
                                ui.end_row();
                            }
                        });
                });
        });
    }

    fn preview_contents(&mut self, ui: &mut egui::Ui) {
        theme::panel_header(ui, "Preview", |ui| {
            if !self.preview_note.is_empty() {
                ui.label(theme::muted_text(&self.preview_note));
            }
        });

        theme::preview_tablet_area(ui, |ui| {
            if self.preview.is_empty() {
                if self.opened {
                    theme::centered_preview_message(
                        ui,
                        "Select conversations and/or a date range, then click Preview.",
                    );
                } else {
                    theme::centered_preview_message(
                        ui,
                        "Open an iOS backup folder, chat.db, or sms.db to get started.",
                    );
                }
            } else {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(false)
                    .show(ui, |ui| {
                        for m in &self.preview {
                            render_bubble(ui, m);
                            theme::gap(ui, layout::ROW_GAP);
                        }
                    });
            }
        });
    }
}

fn render_bubble(ui: &mut egui::Ui, m: &PreviewMessage) {
    let align = if m.is_from_me {
        egui::Align::Max
    } else {
        egui::Align::Min
    };

    let bubble_layout = if m.is_from_me {
        egui::Layout::right_to_left(egui::Align::TOP)
    } else {
        egui::Layout::left_to_right(egui::Align::TOP)
    };

    ui.with_layout(bubble_layout, |ui| {
        // Constrain bubble width so long threads stay readable.
        let max_w = (ui.available_width() * layout::PREVIEW_BUBBLE_MAX_FRACTION)
            .max(layout::PREVIEW_BUBBLE_MIN_WIDTH);
        theme::preview_bubble_frame(m.is_from_me).show(ui, |ui| {
            ui.set_max_width(max_w);
            ui.with_layout(egui::Layout::top_down(align), |ui| {
                ui.label(theme::preview_meta_text(
                    format!("{} · {}", m.sender, m.timestamp),
                    m.is_from_me,
                ));
                if !m.text.is_empty() {
                    ui.label(theme::preview_body_text(&m.text, m.is_from_me));
                }
                if !m.annotations.is_empty() {
                    ui.label(theme::preview_annotation_text(
                        m.annotations.join("   "),
                        m.is_from_me,
                    ));
                }
            });
        });
    });
}

fn call_logs_csv(entries: &[CallLogEntry]) -> String {
    let mut out = String::from(CALL_LOG_CSV_HEADER);
    for entry in entries {
        append_csv_row(
            &mut out,
            [
                entry.started.as_str(),
                entry.direction.label(),
                entry.address.as_str(),
                entry.duration.as_str(),
                entry.service.as_str(),
                entry.call_type.as_str(),
            ],
        );
    }
    out
}

fn append_csv_row<'a>(out: &mut String, fields: impl IntoIterator<Item = &'a str>) {
    let mut first = true;
    for field in fields {
        if !first {
            out.push(CSV_COMMA);
        }
        first = false;
        out.push_str(&csv_escape(field));
    }
    out.push(CSV_NEWLINE);
}

fn csv_escape(field: &str) -> String {
    let needs_quotes = field.contains(CSV_COMMA)
        || field.contains(CSV_QUOTE)
        || field.contains(CSV_NEWLINE)
        || field.starts_with(' ');
    if !needs_quotes {
        return field.to_string();
    }
    format!(
        "{CSV_QUOTE}{}{CSV_QUOTE}",
        field.replace(CSV_QUOTE, CSV_QUOTE_ESCAPE)
    )
}

fn resolve_source_folder(folder: &Path) -> Result<ResolvedSource, String> {
    if !folder.is_dir() {
        return Err(format!("{} is not a folder.", folder.display()));
    }

    let mut chat_db: Option<PathBuf> = None;
    let mut sms_db: Option<PathBuf> = None;
    let mut scanned_entries = 0usize;
    let mut queue = VecDeque::from([(folder.to_path_buf(), 0usize)]);

    while scanned_entries <= DATABASE_SCAN_MAX_ENTRIES {
        let Some((dir, depth)) = queue.pop_front() else {
            break;
        };

        if looks_like_ios_backup_root(&dir) {
            return Ok(ResolvedSource {
                path: dir,
                platform: PlatformChoice::IOS,
                description: "iOS backup root",
            });
        }

        let entries = match sorted_dir_entries(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries {
            scanned_entries += 1;
            if scanned_entries > DATABASE_SCAN_MAX_ENTRIES {
                break;
            }

            let path = entry.path();
            if path.is_dir() {
                if depth < DATABASE_SCAN_MAX_DEPTH {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }

            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if name.eq_ignore_ascii_case(MACOS_CHAT_DB_FILE_NAME) && chat_db.is_none() {
                chat_db = Some(path);
            } else if name.eq_ignore_ascii_case(LOOSE_IOS_MESSAGES_DB_FILE_NAME) && sms_db.is_none()
            {
                sms_db = Some(path);
            }
        }
    }

    if let Some(path) = chat_db {
        return Ok(ResolvedSource {
            path,
            platform: PlatformChoice::MacOS,
            description: "chat.db",
        });
    }

    if let Some(path) = sms_db {
        return Ok(ResolvedSource {
            path,
            platform: PlatformChoice::MacOS,
            description: "sms.db",
        });
    }

    Err(format!(
        "No iOS backup root, chat.db, or sms.db was found in {}.",
        folder.display()
    ))
}

fn looks_like_ios_backup_root(folder: &Path) -> bool {
    folder.join(DEFAULT_PATH_IOS).is_file()
}

fn sorted_dir_entries(dir: &Path) -> Result<Vec<fs::DirEntry>, std::io::Error> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    Ok(entries)
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        if self
            .activity_log_close_requested
            .swap(false, Ordering::Relaxed)
        {
            self.show_activity_log = false;
        }

        self.top_panel(ctx);
        self.left_panel(ctx);
        self.central_panel(ctx);
        if self.show_activity_log {
            self.activity_log_viewport(ctx);
        }

        // Keep polling the backend channel while a long operation runs.
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(timing::BUSY_REPAINT_MS));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Persist settings on exit (portable: beside the executable).
        let _ = self.current_settings().save();
        let _ = self.cmd_tx.send(Command::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("{name}-{}-{stamp}", std::process::id()));
        fs::create_dir(&dir).expect("create temp dir");
        dir
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, b"").expect("write temp file");
    }

    #[test]
    fn folder_resolver_prefers_ios_backup_root() {
        let dir = temp_dir("folder-resolver-ios");
        touch(&dir.join(DEFAULT_PATH_IOS));
        touch(&dir.join(LOOSE_IOS_MESSAGES_DB_FILE_NAME));

        let source = resolve_source_folder(&dir).expect("resolve folder");
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(source.path, dir);
        assert_eq!(source.platform, PlatformChoice::IOS);
    }

    #[test]
    fn folder_resolver_finds_nested_chat_db() {
        let dir = temp_dir("folder-resolver-chat");
        let chat_db = dir
            .join("Library")
            .join("Messages")
            .join(MACOS_CHAT_DB_FILE_NAME);
        touch(&chat_db);

        let source = resolve_source_folder(&dir).expect("resolve folder");
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(source.path, chat_db);
        assert_eq!(source.platform, PlatformChoice::MacOS);
    }

    #[test]
    fn folder_resolver_loads_loose_sms_db() {
        let dir = temp_dir("folder-resolver-sms");
        let sms_db = dir.join(LOOSE_IOS_MESSAGES_DB_FILE_NAME);
        touch(&sms_db);

        let source = resolve_source_folder(&dir).expect("resolve folder");
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(source.path, sms_db);
        assert_eq!(source.platform, PlatformChoice::MacOS);
    }

    #[test]
    fn folder_resolver_reports_missing_database() {
        let dir = temp_dir("folder-resolver-empty");

        let error = resolve_source_folder(&dir).expect_err("expected missing source");
        let _ = fs::remove_dir_all(&dir);

        assert!(error.contains("No iOS backup root"));
    }
}
