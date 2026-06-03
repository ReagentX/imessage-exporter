//! Plain data types shared between the UI thread and the backend worker.
//!
//! Nothing here borrows the database or holds a `Config`; these are owned,
//! `Send` values that move across the command/event channels.

use std::path::PathBuf;

use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use imessage_database::util::dates::{get_offset, TIMESTAMP_FACTOR};

/// Which platform the source database came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlatformChoice {
    /// Detect automatically from the provided path.
    Auto,
    /// A macOS `chat.db` file.
    MacOS,
    /// The root of an iOS device backup directory.
    #[allow(clippy::upper_case_acronyms)]
    IOS,
}

impl PlatformChoice {
    pub fn label(self) -> &'static str {
        match self {
            PlatformChoice::Auto => "Auto-detect",
            PlatformChoice::MacOS => "macOS (chat.db)",
            PlatformChoice::IOS => "iOS backup folder",
        }
    }
}

/// Output format for an export.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FormatChoice {
    Html,
    Txt,
    /// Portable PDF, rendered in-process from the text export (no browser).
    Pdf,
}

impl FormatChoice {
    pub fn label(self) -> &'static str {
        match self {
            FormatChoice::Html => "HTML",
            FormatChoice::Txt => "Text",
            FormatChoice::Pdf => "PDF",
        }
    }
}

/// How the database owner's name is rendered.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NameMode {
    /// Literal "Me".
    Me,
    /// A user-provided custom name.
    Custom,
    /// The database owner's caller ID.
    CallerId,
}

/// Attachment copy/conversion strategy (mirrors the CLI `--copy-method`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CopyMethod {
    Disabled,
    Clone,
    Basic,
    Full,
}

impl CopyMethod {
    pub fn label(self) -> &'static str {
        match self {
            CopyMethod::Disabled => "disabled (reference originals)",
            CopyMethod::Clone => "clone (copy, no conversion)",
            CopyMethod::Basic => "basic (copy + HEIC→JPEG)",
            CopyMethod::Full => "full (copy + convert media)",
        }
    }
}

/// Parameters needed to open a source database/backup.
#[derive(Clone, Debug)]
pub struct OpenParams {
    pub db_path: PathBuf,
    pub platform: PlatformChoice,
    pub password: Option<String>,
    pub contacts_path: Option<PathBuf>,
    pub attachment_root: Option<String>,
}

/// A deduplicated conversation as shown in the conversation list.
#[derive(Clone, Debug)]
pub struct ConversationSummary {
    /// Stable, deduplicated id used as the selection key.
    pub id: i32,
    /// Every underlying `chat.ROWID` that maps to this conversation.
    pub raw_chat_ids: Vec<i32>,
    /// A human title (group name or participant list).
    pub title: String,
    /// Participant detail line.
    pub participants: String,
    /// Total number of messages across the underlying chats.
    pub message_count: i64,
}

/// One message rendered for the preview pane.
#[derive(Clone, Debug)]
pub struct PreviewMessage {
    pub is_from_me: bool,
    pub sender: String,
    pub timestamp: String,
    pub text: String,
    /// Total attachment rows referenced by the message.
    pub attachment_count: usize,
    /// Image attachments resolved for PDF embedding.
    pub attachments: Vec<PreviewAttachment>,
    /// Short badges such as "📎 2 attachments", "↪ reply", "✎ edited".
    pub annotations: Vec<String>,
}

/// A raster image attachment available to render into a PDF bubble.
#[derive(Clone, Debug)]
pub struct PreviewAttachment {
    pub path: PathBuf,
    pub name: String,
}

/// Direction/status for a call-history row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallDirection {
    Incoming,
    Outgoing,
    Missed,
    Blocked,
    Unknown,
}

impl CallDirection {
    pub fn label(self) -> &'static str {
        match self {
            CallDirection::Incoming => "Incoming",
            CallDirection::Outgoing => "Outgoing",
            CallDirection::Missed => "Missed",
            CallDirection::Blocked => "Blocked",
            CallDirection::Unknown => "Unknown",
        }
    }
}

/// One row from the iOS call-history database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallLogEntry {
    pub id: i64,
    pub started: String,
    pub direction: CallDirection,
    pub address: String,
    pub duration: String,
    pub service: String,
    pub call_type: String,
}

/// Filters applied to a preview or export.
#[derive(Clone, Debug, Default)]
pub struct Filters {
    /// Underlying raw chat ids to include. Empty = no chat-id filter.
    pub selected_raw_chat_ids: Vec<i32>,
    /// CLI-style participant text filter, used only when no conversations are
    /// explicitly selected.
    pub conversation_filter: Option<String>,
    /// Inclusive start timestamp in iMessage nanoseconds.
    pub start_ns: Option<i64>,
    /// Exclusive end timestamp in iMessage nanoseconds.
    pub end_ns: Option<i64>,
}

/// Everything needed to perform an export.
#[derive(Clone, Debug)]
pub struct ExportParams {
    pub filters: Filters,
    pub format: FormatChoice,
    pub export_path: PathBuf,
    pub copy_method: CopyMethod,
    pub no_lazy: bool,
    pub custom_name: Option<String>,
    pub use_caller_id: bool,
    pub ignore_disk_space: bool,
}

/// Convert a local date (`YYYY-MM-DD`) and optional time (`HH:MM` or
/// `HH:MM:SS`) into an iMessage nanosecond timestamp, matching the offset
/// math used by [`imessage_database::util::query_context::QueryContext`].
pub fn parse_local_timestamp(date: &str, time: &str) -> Result<i64, String> {
    let date = date.trim();
    let time = time.trim();
    let naive_date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date '{date}' (expected YYYY-MM-DD): {e}"))?;
    let naive_time = if time.is_empty() {
        NaiveTime::from_hms_opt(0, 0, 0).unwrap()
    } else {
        NaiveTime::parse_from_str(time, "%H:%M:%S")
            .or_else(|_| NaiveTime::parse_from_str(time, "%H:%M"))
            .map_err(|e| format!("Invalid time '{time}' (expected HH:MM or HH:MM:SS): {e}"))?
    };
    let naive = NaiveDateTime::new(naive_date, naive_time);
    let local = Local
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| format!("Ambiguous or invalid local time: {naive}"))?;
    let stamp = local
        .timestamp_nanos_opt()
        .ok_or_else(|| "Timestamp out of representable range".to_string())?;
    Ok(stamp - (get_offset() * TIMESTAMP_FACTOR))
}
