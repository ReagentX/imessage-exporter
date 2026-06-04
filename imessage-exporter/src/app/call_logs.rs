use std::{
    collections::HashSet,
    fs::{self, File, create_dir_all},
    io::copy,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crabapple::{Backup, error::BackupError};
use imessage_database::util::{
    dates::{TIMESTAMP_FACTOR, format as fmt_date, get_local_time, get_offset},
    platform::Platform,
};
use rusqlite::{Connection, OptionalExtension, Row, types::ValueRef};
use sha1::{Digest, Sha1};

use crate::app::{
    compatibility::backup::decrypt_backup, error::RuntimeError, options::Options, runtime::Config,
};

pub const DEFAULT_CALL_LOG_CSV_FILE_NAME: &str = "call_logs.csv";

const IOS_BACKUP_DOMAIN_SEPARATOR: &str = "-";
const IOS_BACKUP_HASH_FOLDER_LEN: usize = 2;
const HEX_CHARS_PER_BYTE: usize = 2;
const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 3_600;
const CALL_LOG_TEMP_DIR_PREFIX: &str = "imessage-exporter-callhistory";
const MANIFEST_PLIST_FILE_NAME: &str = "Manifest.plist";
const SQLITE_WAL_SUFFIX: &str = "-wal";
const SQLITE_SHM_SUFFIX: &str = "-shm";
const CALL_LOG_NO_VALUE: &str = "Unknown";
const CALL_LOG_TYPE_PREFIX: &str = "Type";
const CALL_LOG_CSV_HEADER: &str = "Started,Direction,Address,Duration,Service,Type\n";
const CALL_HISTORY_MISSING_UNENCRYPTED_HINT: &str = "This iOS backup is not encrypted, and its manifest does not list the Apple \
     Phone/FaceTime call-history database. Recent iOS backups often omit call \
     history unless backup encryption is enabled. Create an encrypted backup and \
     open that backup to load call logs.";
const CALL_ROW_ID_INDEX: usize = 0;
const CALL_ADDRESS_INDEX: usize = 1;
const CALL_DATE_INDEX: usize = 2;
const CALL_DURATION_INDEX: usize = 3;
const CALL_ORIGINATED_INDEX: usize = 4;
const CALL_ANSWERED_INDEX: usize = 5;
const CALL_TYPE_INDEX: usize = 6;
const CALL_SERVICE_INDEX: usize = 7;
const CALL_LEGACY_FLAGS_INDEX: usize = 8;
const CSV_QUOTE: char = '"';
const CSV_COMMA: char = ',';
const CSV_NEWLINE: char = '\n';
const CSV_QUOTE_ESCAPE: &str = "\"\"";

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallDirection {
    Incoming,
    Outgoing,
    Missed,
    Blocked,
    Unknown,
}

impl CallDirection {
    #[must_use]
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

pub struct CallLogLoadResult {
    pub entries: Vec<CallLogEntry>,
    pub total: i64,
    pub source: String,
}

#[derive(Clone, Copy, Debug)]
struct CallHistorySource {
    label: &'static str,
    domain: &'static str,
    relative_path: &'static str,
    sqlite_name: &'static str,
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

pub fn load(
    config: &Config,
    backup_root: &Path,
    limit: Option<usize>,
) -> Result<CallLogLoadResult, RuntimeError> {
    if config.options.platform != Platform::iOS {
        return Err(call_log_error(
            "Call logs are available from iOS backup folders only.",
        ));
    }

    load_from_backup(backup_root, config.data_source.backup.as_ref(), limit)
}

pub fn load_from_options(options: &Options) -> Result<CallLogLoadResult, RuntimeError> {
    if options.platform != Platform::iOS {
        return Err(call_log_error(
            "Call logs are available from iOS backup folders only.",
        ));
    }

    let backup = decrypt_backup(options)?;
    load_from_backup(&options.db_path, backup.as_ref(), options.call_log_limit)
}

fn load_from_backup(
    backup_root: &Path,
    backup: Option<&Backup>,
    limit: Option<usize>,
) -> Result<CallLogLoadResult, RuntimeError> {
    let materialized = materialize_call_history(backup_root, backup)?;
    let (entries, total) = collect_call_logs_from_db(&materialized.db_path, limit)?;

    Ok(CallLogLoadResult {
        entries,
        total,
        source: materialized.source_label.to_string(),
    })
}

pub fn export_csv_from_options(
    options: &Options,
) -> Result<(PathBuf, CallLogLoadResult), RuntimeError> {
    let path = call_log_csv_path(&options.export_path)?;
    let result = load_from_options(options)?;
    fs::write(&path, call_logs_csv(&result.entries))?;
    Ok((path, result))
}

pub fn export_csv(config: &Config) -> Result<(PathBuf, CallLogLoadResult), RuntimeError> {
    let path = call_log_csv_path(&config.options.export_path)?;
    let result = load(
        config,
        &config.options.db_path,
        config.options.call_log_limit,
    )?;
    fs::write(&path, call_logs_csv(&result.entries))?;
    Ok((path, result))
}

fn call_log_csv_path(export_path: &Path) -> Result<PathBuf, RuntimeError> {
    create_dir_all(export_path)?;
    let path = export_path.join(DEFAULT_CALL_LOG_CSV_FILE_NAME);

    if path.exists() {
        return Err(call_log_error(format!(
            "Call log export file {} already exists. Remove it or choose another export path.",
            path.display()
        )));
    }

    Ok(path)
}

#[must_use]
pub fn call_logs_csv(entries: &[CallLogEntry]) -> String {
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

fn materialize_call_history(
    backup_root: &Path,
    backup: Option<&Backup>,
) -> Result<MaterializedCallHistory, RuntimeError> {
    let tried = CALL_HISTORY_SOURCES
        .iter()
        .map(|candidate| format!("{}:{}", candidate.domain, candidate.relative_path))
        .collect::<Vec<_>>()
        .join(", ");

    for candidate in CALL_HISTORY_SOURCES {
        if let Some(materialized) =
            materialize_call_history_candidate(backup_root, backup, *candidate)?
        {
            return Ok(materialized);
        }
    }

    Err(call_history_missing_message(
        backup_root,
        backup.is_some(),
        &tried,
    ))
}

fn call_history_missing_message(
    backup_root: &Path,
    has_encrypted_backup: bool,
    tried: &str,
) -> RuntimeError {
    let base = format!("Could not find a call-history database in the backup. Tried: {tried}");
    if has_encrypted_backup {
        return call_log_error(base);
    }

    match backup_manifest_is_encrypted(backup_root) {
        Ok(Some(false)) => {
            call_log_error(format!("{base}\n\n{CALL_HISTORY_MISSING_UNENCRYPTED_HINT}"))
        }
        _ => call_log_error(base),
    }
}

fn backup_manifest_is_encrypted(backup_root: &Path) -> Result<Option<bool>, RuntimeError> {
    let path = backup_root.join(MANIFEST_PLIST_FILE_NAME);
    if !path.is_file() {
        return Ok(None);
    }

    let value = plist::Value::from_file(&path)
        .map_err(|why| call_log_error(format!("Could not read {}: {why}", path.display())))?;
    Ok(value
        .as_dictionary()
        .and_then(|dict| dict.get("IsEncrypted"))
        .and_then(plist::Value::as_boolean))
}

fn materialize_call_history_candidate(
    backup_root: &Path,
    backup: Option<&Backup>,
    candidate: CallHistorySource,
) -> Result<Option<MaterializedCallHistory>, RuntimeError> {
    let temp_dir = create_call_history_temp_dir(candidate.sqlite_name)?;
    let primary_target = temp_dir.join(candidate.sqlite_name);
    if !copy_backup_file_to(
        backup,
        backup_root,
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
            backup,
            backup_root,
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

fn create_call_history_temp_dir(sqlite_name: &str) -> Result<PathBuf, RuntimeError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!(
        "{CALL_LOG_TEMP_DIR_PREFIX}-{}-{stamp}-{sqlite_name}",
        std::process::id()
    ));
    fs::create_dir(&dir).map_err(|why| {
        call_log_error(format!(
            "Could not create temporary call-history folder {}: {why}",
            dir.display()
        ))
    })?;
    Ok(dir)
}

fn copy_backup_file_to(
    backup: Option<&Backup>,
    backup_root: &Path,
    domain: &str,
    relative_path: &str,
    target: &Path,
) -> Result<bool, RuntimeError> {
    let file_id = backup_file_id(domain, relative_path);
    if let Some(backup) = backup {
        let file = match backup.get_file(&file_id) {
            Ok(file) => file,
            Err(BackupError::FileNotFoundInBackup(_)) => return Ok(false),
            Err(why) => {
                return Err(call_log_error(format!(
                    "Could not resolve {domain}:{relative_path} in encrypted backup: {why}"
                )));
            }
        };
        let mut decrypted = backup.decrypt_entry_stream(&file).map_err(|why| {
            call_log_error(format!("Could not decrypt {domain}:{relative_path}: {why}"))
        })?;
        let mut out = File::create(target)?;
        copy(&mut decrypted, &mut out)?;
        return Ok(true);
    }

    let hashed_path = backup_root
        .join(&file_id[..IOS_BACKUP_HASH_FOLDER_LEN])
        .join(&file_id);
    if !hashed_path.is_file() {
        return Ok(false);
    }
    fs::copy(&hashed_path, target).map_err(|why| {
        call_log_error(format!(
            "Could not copy {} to {}: {why}",
            hashed_path.display(),
            target.display()
        ))
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
    limit: Option<usize>,
) -> Result<(Vec<CallLogEntry>, i64), RuntimeError> {
    let conn = Connection::open(db_path)
        .map_err(|why| call_log_error(format!("Could not open call-history database: {why}")))?;
    let schema = call_log_schema(&conn)?;
    let total = call_log_count(&conn, &schema)?;
    let limit_clause = if limit.is_some() { " LIMIT ?1" } else { "" };
    let sql = format!(
        "SELECT {id} AS row_id, {address} AS address, {date} AS started, \
         {duration} AS duration, {originated} AS originated, {answered} AS answered, \
         {call_type} AS call_type, {service} AS service, {flags} AS legacy_flags \
         FROM {table} ORDER BY {order_by} DESC{limit_clause}",
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
        .map_err(|why| call_log_error(format!("Could not prepare call-history query: {why}")))?;
    let mut rows = match limit {
        Some(limit) => stmt
            .query([limit as i64])
            .map_err(|why| call_log_error(format!("Could not query call history: {why}")))?,
        None => stmt
            .query([])
            .map_err(|why| call_log_error(format!("Could not query call history: {why}")))?,
    };

    let mut entries = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|why| call_log_error(format!("Could not read call-history row: {why}")))?
    {
        let raw = raw_call_log_row(row)
            .map_err(|why| call_log_error(format!("Could not read call-history row: {why}")))?;
        entries.push(call_log_entry_from_raw(raw));
    }

    Ok((entries, total))
}

fn call_log_schema(conn: &Connection) -> Result<CallLogSchema, RuntimeError> {
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

    Err(call_log_error(format!(
        "Call-history database does not contain {MODERN_CALL_TABLE} or {LEGACY_CALL_TABLE}."
    )))
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, RuntimeError> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        [table],
        |_| Ok(()),
    )
    .optional()
    .map(|row| row.is_some())
    .map_err(|why| call_log_error(format!("Could not inspect call-history tables: {why}")))
}

fn table_columns(conn: &Connection, table: &str) -> Result<HashSet<String>, RuntimeError> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|why| call_log_error(format!("Could not inspect {table} columns: {why}")))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|why| call_log_error(format!("Could not read {table} columns: {why}")))?;

    let mut columns = HashSet::new();
    for row in rows {
        columns.insert(
            row.map_err(|why| call_log_error(format!("Could not read {table} column: {why}")))?,
        );
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

fn call_log_count(conn: &Connection, schema: &CallLogSchema) -> Result<i64, RuntimeError> {
    let sql = format!("SELECT COUNT(*) FROM {}", schema.table);
    conn.query_row(&sql, [], |row| row.get::<_, i64>(0))
        .map_err(|why| call_log_error(format!("Could not count call-history rows: {why}")))
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
    if !seconds_since_reference.is_finite() {
        return CALL_LOG_NO_VALUE.to_string();
    }

    let nanos_since_reference = seconds_since_reference * TIMESTAMP_FACTOR as f64;
    if nanos_since_reference < i64::MIN as f64 || nanos_since_reference > i64::MAX as f64 {
        return CALL_LOG_NO_VALUE.to_string();
    }

    get_local_time(nanos_since_reference.round() as i64, get_offset())
        .map(|date| fmt_date(&date))
        .unwrap_or_else(|_| CALL_LOG_NO_VALUE.to_string())
}

fn call_log_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::CallLogError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_db(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("{name}-{}-{stamp}.db", std::process::id()))
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!("{name}-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).expect("create temp test dir");
        path
    }

    fn write_manifest_plist(root: &Path, is_encrypted: bool) {
        let value = if is_encrypted { "true" } else { "false" };
        fs::write(
            root.join(MANIFEST_PLIST_FILE_NAME),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>IsEncrypted</key>
  <{value}/>
</dict>
</plist>
"#
            ),
        )
        .expect("write manifest plist");
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
    fn reads_backup_manifest_encryption_state() {
        let root = temp_test_dir("backup-manifest-state");
        write_manifest_plist(&root, false);

        assert_eq!(backup_manifest_is_encrypted(&root).unwrap(), Some(false));

        write_manifest_plist(&root, true);
        assert_eq!(backup_manifest_is_encrypted(&root).unwrap(), Some(true));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_call_history_message_explains_unencrypted_backup() {
        let root = temp_test_dir("missing-call-history");
        write_manifest_plist(&root, false);

        let message = call_history_missing_message(&root, false, "HomeDomain:example").to_string();

        assert!(message.contains("Could not find a call-history database"));
        assert!(message.contains("HomeDomain:example"));
        assert!(message.contains("backup is not encrypted"));
        assert!(message.contains("Create an encrypted backup"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_call_history_message_keeps_encrypted_backup_generic() {
        let root = temp_test_dir("missing-call-history-encrypted");
        write_manifest_plist(&root, false);

        let message = call_history_missing_message(&root, true, "HomeDomain:example").to_string();

        assert!(message.contains("Could not find a call-history database"));
        assert!(!message.contains("backup is not encrypted"));

        let _ = fs::remove_dir_all(root);
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

        let (entries, total) = collect_call_logs_from_db(&path, Some(CALL_HISTORY_SOURCES.len()))
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

        let (entries, total) = collect_call_logs_from_db(&path, Some(CALL_HISTORY_SOURCES.len()))
            .expect("collect legacy call logs");
        let _ = fs::remove_file(&path);

        assert_eq!(total, 2);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].direction, CallDirection::Blocked);
        assert_eq!(entries[1].direction, CallDirection::Outgoing);
        assert_eq!(entries[1].duration, "1:00:05");
    }

    #[test]
    fn csv_quotes_fields_that_need_it() {
        let csv = call_logs_csv(&[CallLogEntry {
            id: 1,
            started: "Jun 04, 2026  1:02:03 PM".to_string(),
            direction: CallDirection::Incoming,
            address: " Smith, Jane ".to_string(),
            duration: "0:30".to_string(),
            service: "FaceTime".to_string(),
            call_type: "Type \"video\"".to_string(),
        }]);

        assert!(csv.starts_with(CALL_LOG_CSV_HEADER));
        assert!(csv.contains("\" Smith, Jane \""));
        assert!(csv.contains("\"Type \"\"video\"\"\""));
    }
}
