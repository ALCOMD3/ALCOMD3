use crate::log_sanitization::sanitize_log_text;
use arc_swap::ArcSwapOption;
use chrono::{DateTime, Datelike, Timelike};
use indexmap::IndexMap;
use log::{Log, Metadata, Record};
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::fmt::{Display, Formatter};
use std::io::{BufRead, BufReader, Read as _, Seek, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use tauri::{AppHandle, Emitter};
use vrc_get_vpm::io::DefaultEnvironmentIo;

static APP_HANDLE: ArcSwapOption<AppHandle> = ArcSwapOption::const_empty();
const TECHNICAL_LOG_RELATIVE_FOLDER: &str = crate::storage::TECHNICAL_LOG_DIR;
const MAX_LOGS: usize = 30;
const MCP_TECHNICAL_LOG_DEFAULT_LIMIT: usize = 50;
const MCP_TECHNICAL_LOG_MAX_LIMIT: usize = 100;
const MCP_TECHNICAL_LOG_DEFAULT_MESSAGE_CHARS: usize = 300;
const MCP_TECHNICAL_LOG_DETAIL_MESSAGE_CHARS: usize = 4000;
const MCP_TECHNICAL_LOG_RECENT_FILE_MAX_BYTES: u64 = 1024 * 1024;
static NEXT_LOG_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub fn set_app_handle(handle: AppHandle) {
    APP_HANDLE.store(Some(Arc::new(handle)));
}

pub fn initialize_logger() -> DefaultEnvironmentIo {
    let (sender, receiver) = mpsc::channel::<LogChannelMessage>();
    let logger = Logger { sender };

    log::set_max_level(log::LevelFilter::Debug);
    log::set_boxed_logger(Box::new(logger)).expect("error while setting logger");

    let io = DefaultEnvironmentIo::new_default();

    start_logging_thread(receiver, &io);

    io
}

fn start_logging_thread(receiver: mpsc::Receiver<LogChannelMessage>, io: &DefaultEnvironmentIo) {
    let new_log_folder = log_folder(io);
    if !new_log_folder.exists() {
        std::fs::create_dir_all(&new_log_folder).ok();
    }
    let timestamp = chrono::Utc::now()
        .format("%Y-%m-%d_%H-%M-%S.%6f")
        .to_string();
    let log_file = new_log_folder.join(format!(
        "{}{timestamp}.log",
        crate::storage::TECHNICAL_LOG_FILE_PREFIX
    ));

    let log_file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        Ok(file) => {
            log::info!("logging to file {}", log_file.display());
            Some(file)
        }
        Err(e) => {
            log::error!("error while opening log file: {e}");
            None
        }
    };

    std::thread::Builder::new()
        .name("logging".to_string())
        .spawn(move || {
            logging_thread_main(receiver, log_file);
        })
        .expect("error while starting logging thread");

    std::thread::Builder::new()
        .name("remove-old-logs".to_string())
        .spawn(move || remove_old_logs(new_log_folder))
        .expect("error while starting remove-old-logs thread");
}

fn is_log_file_name(name: &str) -> bool {
    // alcomd3-yyyy-mm-dd_hh-mm-ss.ssssss.log, or the pre-2.1.0 vrc-get prefix.
    let Some(name) = name
        .strip_prefix(crate::storage::TECHNICAL_LOG_FILE_PREFIX)
        .or_else(|| name.strip_prefix(crate::storage::LEGACY_TECHNICAL_LOG_FILE_PREFIX))
    else {
        return false;
    };
    let Some(name) = name.strip_suffix(".log") else {
        return false;
    };

    //              00000000001111111111222222
    //              01234567890123456789012345
    // now, name is yyyy-mm-dd_hh-mm-ss.ssssss
    let name = name.as_bytes();
    let Ok(name) = <&[u8; 26]>::try_from(name) else {
        return false;
    };

    if name[4] != b'-'
        || name[7] != b'-'
        || name[10] != b'_'
        || name[13] != b'-'
        || name[16] != b'-'
        || name[19] != b'.'
    {
        return false;
    }

    name[0..4].iter().all(u8::is_ascii_digit)
        && name[5..7].iter().all(u8::is_ascii_digit)
        && name[8..10].iter().all(u8::is_ascii_digit)
        && name[11..13].iter().all(u8::is_ascii_digit)
        && name[14..16].iter().all(u8::is_ascii_digit)
        && name[17..19].iter().all(u8::is_ascii_digit)
        && name[20..26].iter().all(u8::is_ascii_digit)
}

fn remove_old_logs(log_folder: std::path::PathBuf) {
    let read_dir = match std::fs::read_dir(&log_folder) {
        Ok(read_dir) => read_dir,
        Err(e) => {
            log::error!("error while reading log folder: {e}");
            return;
        }
    };

    let entries = match read_dir.collect::<Result<Vec<_>, _>>() {
        Ok(entries) => entries,
        Err(e) => {
            log::error!("error while reading log folder: {e}");
            return;
        }
    };

    let mut log_files = entries
        .into_iter()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if is_log_file_name(&name) {
                Some((name, entry))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    log_files.sort_by_key(|(name, _)| Reverse(name.clone()));

    for (name, _) in log_files.iter().take(MAX_LOGS) {
        log::debug!("log to keep: {name}");
    }

    for (name, _) in log_files.iter().skip(MAX_LOGS) {
        match std::fs::remove_file(log_folder.join(name)) {
            Ok(()) => log::debug!("removed old log: {name}"),
            Err(e) => log::debug!("error while removing old log: {name}: {e}"),
        }
    }
}

fn logging_thread_main(
    receiver: mpsc::Receiver<LogChannelMessage>,
    mut log_file: Option<std::fs::File>,
) {
    for message in receiver {
        match message {
            LogChannelMessage::Log(entry) => {
                let sequence = NEXT_LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed);
                let message = format!("{entry}");
                // log to console
                eprintln!("{message}");

                // log to file
                if let Some(log_file) = log_file.as_mut() {
                    log_err(writeln!(log_file, "{message}"));
                }

                // add to buffer
                {
                    let mut buffer = LOG_BUFFER.lock().unwrap();
                    buffer.enqueue(entry.clone());
                }
                {
                    let mut buffer = MCP_LOG_BUFFER.lock().unwrap();
                    buffer.enqueue(TechnicalMemoryLogEntry {
                        sequence,
                        entry: entry.clone(),
                    });
                }

                // send to tauri
                if let Some(app_handle) = APP_HANDLE.load().as_ref() {
                    app_handle
                        .emit("log", Some(entry))
                        .expect("error while emitting log event");
                }
            }
            LogChannelMessage::Flush(sync) => {
                if let Some(log_file) = log_file.as_mut() {
                    log_err(log_file.flush());
                    sync.send(()).ok();
                }
            }
        }
    }
}

enum LogChannelMessage {
    Log(LogEntry),
    Flush(mpsc::Sender<()>),
}

pub(crate) fn get_log_entries() -> Vec<LogEntry> {
    LOG_BUFFER.lock().unwrap().to_vec()
}

pub(crate) fn log_folder(io: &DefaultEnvironmentIo) -> PathBuf {
    io.resolve(TECHNICAL_LOG_RELATIVE_FOLDER.as_ref())
}

#[derive(Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TechnicalLogScope {
    #[default]
    Memory,
    RecentFiles,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TechnicalLogGroupBy {
    #[default]
    Level,
    Target,
    File,
    Hour,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct TechnicalLogSearchParams {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub levels: Option<Vec<LogLevel>>,
    #[serde(default)]
    pub targets: Option<Vec<String>>,
    #[serde(default)]
    pub scope: TechnicalLogScope,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub max_message_chars: Option<usize>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct TechnicalLogSummaryParams {
    #[serde(flatten)]
    pub filter: TechnicalLogSearchParams,
    #[serde(default)]
    pub group_by: TechnicalLogGroupBy,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TechnicalLogEntryParams {
    pub id: String,
    #[serde(default)]
    pub max_message_chars: Option<usize>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalLogEntrySummary {
    id: String,
    time: String,
    level: LogLevel,
    target: String,
    message_preview: String,
    truncated: bool,
    source: TechnicalLogSource,
    file_name: Option<String>,
    line_number: Option<usize>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalLogEntryDetails {
    id: String,
    time: String,
    level: LogLevel,
    target: String,
    message: String,
    truncated: bool,
    source: TechnicalLogSource,
    file_name: Option<String>,
    line_number: Option<usize>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TechnicalLogSource {
    Memory,
    File,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalLogSearchResponse {
    ok: bool,
    total_count: usize,
    offset: usize,
    limit: usize,
    returned_count: usize,
    has_more: bool,
    next_offset: Option<usize>,
    entries: Vec<TechnicalLogEntrySummary>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalLogSummaryGroup {
    key: String,
    count: usize,
    error_count: usize,
    warn_count: usize,
    latest_entry_id: Option<String>,
    latest_time: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalLogSummaryResponse {
    ok: bool,
    group_by: TechnicalLogGroupBy,
    total_count: usize,
    total_group_count: usize,
    offset: usize,
    limit: usize,
    returned_count: usize,
    has_more: bool,
    next_offset: Option<usize>,
    groups: Vec<TechnicalLogSummaryGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalLogQueryError {
    code: &'static str,
    message: String,
}

impl TechnicalLogQueryError {
    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_params",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "not_found",
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(
    Serialize, Deserialize, specta::Type, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum LogLevel {
    #[serde(alias = "error", alias = "ERROR")]
    Error = 1,
    #[serde(alias = "warn", alias = "warning", alias = "WARN")]
    Warn,
    #[serde(alias = "info", alias = "INFO")]
    Info,
    #[serde(alias = "debug", alias = "DEBUG")]
    Debug,
    #[serde(alias = "trace", alias = "TRACE")]
    Trace,
}

impl Display for LogLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Error => "ERROR".fmt(f),
            LogLevel::Warn => "WARN".fmt(f),
            LogLevel::Info => "INFO".fmt(f),
            LogLevel::Debug => "DEBUG".fmt(f),
            LogLevel::Trace => "TRACE".fmt(f),
        }
    }
}

impl From<log::Level> for LogLevel {
    fn from(value: log::Level) -> Self {
        match value {
            log::Level::Error => LogLevel::Error,
            log::Level::Warn => LogLevel::Warn,
            log::Level::Info => LogLevel::Info,
            log::Level::Debug => LogLevel::Debug,
            log::Level::Trace => LogLevel::Trace,
        }
    }
}

#[derive(Serialize, specta::Type, Clone)]
pub(crate) struct LogEntry {
    #[serde(serialize_with = "to_rfc3339_micros")]
    #[specta(type = &str)]
    time: chrono::DateTime<chrono::Local>,
    level: LogLevel,
    target: String,
    message: String,
    gui_toast: Option<bool>,
}

fn to_rfc3339_micros<S>(
    time: &chrono::DateTime<chrono::Local>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    time.to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
        .serialize(serializer)
}

impl LogEntry {
    pub fn new(record: &Record) -> Self {
        let gui_toast = record
            .key_values()
            .get("gui_toast".into())
            .and_then(|x| x.to_bool());
        LogEntry {
            time: chrono::Local::now(),
            level: record.level().into(),
            target: record.target().to_string(),
            message: format!("{}", record.args()),
            gui_toast,
        }
    }
}

impl Display for LogEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} [{: >5}] {}: {}",
            self.time
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, false),
            self.level,
            self.target,
            self.message
        )
    }
}

#[derive(Clone)]
struct TechnicalMemoryLogEntry {
    sequence: u64,
    entry: LogEntry,
}

#[derive(Debug, Clone)]
struct TechnicalLogRecord {
    id: String,
    time: chrono::DateTime<chrono::FixedOffset>,
    level: LogLevel,
    target: String,
    message: String,
    source: TechnicalLogSource,
    file_name: Option<String>,
    line_number: Option<usize>,
}

pub(crate) fn search_technical_logs(
    log_folder: &Path,
    params: TechnicalLogSearchParams,
) -> Result<TechnicalLogSearchResponse, TechnicalLogQueryError> {
    let message_limit = technical_log_message_limit(
        params.max_message_chars,
        MCP_TECHNICAL_LOG_DEFAULT_MESSAGE_CHARS,
    );
    let offset = params.offset.unwrap_or(0);
    let limit = technical_log_limit(params.limit);
    let entries = query_technical_log_records(log_folder, &params)?;
    let total_count = entries.len();
    let entries = entries
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|record| TechnicalLogEntrySummary::from_record(record, message_limit))
        .collect::<Vec<_>>();
    let returned_count = entries.len();
    let next_offset = offset
        .checked_add(returned_count)
        .filter(|next| *next < total_count);

    Ok(TechnicalLogSearchResponse {
        ok: true,
        total_count,
        offset,
        limit,
        returned_count,
        has_more: next_offset.is_some(),
        next_offset,
        entries,
    })
}

pub(crate) fn get_technical_log_entry(
    log_folder: &Path,
    params: TechnicalLogEntryParams,
) -> Result<TechnicalLogEntryDetails, TechnicalLogQueryError> {
    let message_limit = technical_log_message_limit(
        params.max_message_chars,
        MCP_TECHNICAL_LOG_DETAIL_MESSAGE_CHARS,
    );
    let record = technical_log_record_by_id(log_folder, &params.id)?.ok_or_else(|| {
        TechnicalLogQueryError::not_found(format!("Technical log entry not found: {}", params.id))
    })?;

    Ok(TechnicalLogEntryDetails::from_record(record, message_limit))
}

pub(crate) fn summarize_technical_logs(
    log_folder: &Path,
    params: TechnicalLogSummaryParams,
) -> Result<TechnicalLogSummaryResponse, TechnicalLogQueryError> {
    let entries = query_technical_log_records(log_folder, &params.filter)?;
    let total_count = entries.len();
    let mut groups = IndexMap::<String, TechnicalLogSummaryGroup>::new();

    for entry in entries {
        let key = technical_log_group_key(&entry, params.group_by);
        let group = groups
            .entry(key.clone())
            .or_insert_with(|| TechnicalLogSummaryGroup {
                key,
                count: 0,
                error_count: 0,
                warn_count: 0,
                latest_entry_id: None,
                latest_time: None,
            });
        group.count += 1;
        if entry.level == LogLevel::Error {
            group.error_count += 1;
        }
        if entry.level == LogLevel::Warn {
            group.warn_count += 1;
        }
        if technical_log_time_is_newer(&entry, group.latest_time.as_deref()) {
            group.latest_entry_id = Some(entry.id.clone());
            group.latest_time = Some(format_technical_time(entry.time));
        }
    }

    let mut groups = groups.into_values().collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.key.cmp(&right.key))
    });
    let total_group_count = groups.len();
    let offset = params.filter.offset.unwrap_or(0);
    let limit = technical_log_limit(params.filter.limit);
    let groups = groups
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let returned_count = groups.len();
    let next_offset = offset
        .checked_add(returned_count)
        .filter(|next| *next < total_group_count);

    Ok(TechnicalLogSummaryResponse {
        ok: true,
        group_by: params.group_by,
        total_count,
        total_group_count,
        offset,
        limit,
        returned_count,
        has_more: next_offset.is_some(),
        next_offset,
        groups,
    })
}

fn query_technical_log_records(
    log_folder: &Path,
    params: &TechnicalLogSearchParams,
) -> Result<Vec<TechnicalLogRecord>, TechnicalLogQueryError> {
    let since = parse_optional_technical_time("since", params.since.as_deref())?;
    let until = parse_optional_technical_time("until", params.until.as_deref())?;
    if let (Some(since), Some(until)) = (since, until)
        && since > until
    {
        return Err(TechnicalLogQueryError::invalid_params(
            "since must be before or equal to until",
        ));
    }

    let search = params.search.as_deref().unwrap_or_default().to_lowercase();
    let levels = params
        .levels
        .clone()
        .unwrap_or_else(default_technical_levels);
    let targets = normalized_string_set(params.targets.as_deref());
    let mut records = match params.scope {
        TechnicalLogScope::Memory => memory_technical_log_records(),
        TechnicalLogScope::RecentFiles => recent_file_technical_log_records(log_folder),
    };

    records.retain(|record| {
        levels.contains(&record.level)
            && targets.as_ref().is_none_or(|targets| {
                targets
                    .iter()
                    .any(|target| record.target.to_lowercase().contains(target))
            })
            && (search.is_empty()
                || record.target.to_lowercase().contains(&search)
                || record.message.to_lowercase().contains(&search))
            && since.is_none_or(|since| record.time >= since)
            && until.is_none_or(|until| record.time <= until)
    });
    records.sort_by(|left, right| {
        right
            .time
            .cmp(&left.time)
            .then_with(|| right.id.cmp(&left.id))
    });
    Ok(records)
}

fn technical_log_record_by_id(
    log_folder: &Path,
    id: &str,
) -> Result<Option<TechnicalLogRecord>, TechnicalLogQueryError> {
    if let Some(sequence) = id.strip_prefix("memory:") {
        let Ok(sequence) = sequence.parse::<u64>() else {
            return Err(TechnicalLogQueryError::invalid_params(format!(
                "Invalid memory technical log id: {id}"
            )));
        };
        return Ok(memory_technical_log_records()
            .into_iter()
            .find(|record| record.id == format!("memory:{sequence}")));
    }

    let Some(file_id) = id.strip_prefix("file:") else {
        return Err(TechnicalLogQueryError::invalid_params(format!(
            "Invalid technical log id: {id}"
        )));
    };
    if let Some((file_name, byte_offset)) = file_id.split_once(":offset:") {
        let Ok(byte_offset) = byte_offset.parse::<u64>() else {
            return Err(TechnicalLogQueryError::invalid_params(format!(
                "Invalid file technical log byte offset: {id}"
            )));
        };
        if !is_log_file_name(file_name) {
            return Err(TechnicalLogQueryError::invalid_params(format!(
                "Invalid technical log file name: {file_name}"
            )));
        }

        return Ok(read_technical_log_record_at_offset(
            &log_folder.join(file_name),
            file_name,
            byte_offset,
        ));
    }

    let Some((file_name, line_number)) = file_id.rsplit_once(':') else {
        return Err(TechnicalLogQueryError::invalid_params(format!(
            "Invalid file technical log id: {id}"
        )));
    };
    let Ok(line_number) = line_number.parse::<usize>() else {
        return Err(TechnicalLogQueryError::invalid_params(format!(
            "Invalid file technical log line number: {id}"
        )));
    };
    if !is_log_file_name(file_name) {
        return Err(TechnicalLogQueryError::invalid_params(format!(
            "Invalid technical log file name: {file_name}"
        )));
    }

    Ok(read_technical_log_record_at_line(
        &log_folder.join(file_name),
        file_name,
        line_number,
    ))
}

fn memory_technical_log_records() -> Vec<TechnicalLogRecord> {
    MCP_LOG_BUFFER
        .lock()
        .unwrap()
        .to_vec()
        .into_iter()
        .map(|entry| TechnicalLogRecord {
            id: format!("memory:{}", entry.sequence),
            time: entry.entry.time.fixed_offset(),
            level: entry.entry.level,
            target: entry.entry.target,
            message: entry.entry.message,
            source: TechnicalLogSource::Memory,
            file_name: None,
            line_number: None,
        })
        .collect()
}

fn recent_file_technical_log_records(log_folder: &Path) -> Vec<TechnicalLogRecord> {
    recent_log_files(log_folder)
        .into_iter()
        .flat_map(|(file_name, path)| read_technical_log_file(&path, &file_name))
        .collect()
}

fn recent_log_files(log_folder: &Path) -> Vec<(String, PathBuf)> {
    let Ok(read_dir) = std::fs::read_dir(log_folder) else {
        return Vec::new();
    };
    let mut files = read_dir
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            is_log_file_name(&name).then_some((name, entry.path()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(name, _)| Reverse(name.clone()));
    files.truncate(MAX_LOGS);
    files
}

fn read_technical_log_file(path: &Path, file_name: &str) -> Vec<TechnicalLogRecord> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(file_len) = file.metadata().map(|metadata| metadata.len()) else {
        return Vec::new();
    };
    let start_offset = file_len.saturating_sub(MCP_TECHNICAL_LOG_RECENT_FILE_MAX_BYTES);
    let line_numbers_known = start_offset == 0;
    let mut read_offset = start_offset;

    if read_offset > 0 && !file_offset_is_line_boundary(&mut file, read_offset) {
        if file.seek(SeekFrom::Start(read_offset)).is_err() {
            return Vec::new();
        }
        let mut reader = BufReader::new(file);
        let mut skipped = Vec::new();
        let Ok(skipped_bytes) = reader.read_until(b'\n', &mut skipped) else {
            return Vec::new();
        };
        read_offset += skipped_bytes as u64;
        return read_technical_log_records_from_reader(reader, file_name, None, read_offset);
    }

    if file.seek(SeekFrom::Start(read_offset)).is_err() {
        return Vec::new();
    }
    read_technical_log_records_from_reader(
        BufReader::new(file),
        file_name,
        line_numbers_known.then_some(1),
        read_offset,
    )
}

fn file_offset_is_line_boundary(file: &mut std::fs::File, offset: u64) -> bool {
    if offset == 0 {
        return true;
    }
    if file.seek(SeekFrom::Start(offset - 1)).is_err() {
        return false;
    }
    let mut previous = [0];
    file.read_exact(&mut previous).is_ok() && previous[0] == b'\n'
}

fn read_technical_log_records_from_reader<R: BufRead>(
    mut reader: R,
    file_name: &str,
    mut line_number: Option<usize>,
    mut byte_offset: u64,
) -> Vec<TechnicalLogRecord> {
    let mut records = Vec::new();
    let mut current = None::<TechnicalLogRecord>;
    let mut line = String::new();
    loop {
        line.clear();
        let Ok(bytes_read) = reader.read_line(&mut line) else {
            break;
        };
        if bytes_read == 0 {
            break;
        }

        let line_start = byte_offset;
        byte_offset += bytes_read as u64;
        let line_text = line.trim_end_matches(|ch| matches!(ch, '\r' | '\n'));
        if let Some(record) =
            parse_technical_log_line(file_name, line_number, line_start, line_text)
        {
            if let Some(previous) = current.replace(record) {
                records.push(previous);
            }
        } else if let Some(record) = &mut current {
            record.message.push('\n');
            record.message.push_str(line_text);
        }
        if let Some(line_number) = &mut line_number {
            *line_number += 1;
        }
    }
    if let Some(record) = current {
        records.push(record);
    }
    records
}

fn read_technical_log_record_at_line(
    path: &Path,
    file_name: &str,
    target_line_number: usize,
) -> Option<TechnicalLogRecord> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut line_number = 1;
    let mut byte_offset = 0;

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).ok()?;
        if bytes_read == 0 {
            return None;
        }

        let line_start = byte_offset;
        byte_offset += bytes_read as u64;
        let line_text = line.trim_end_matches(|ch| matches!(ch, '\r' | '\n'));
        if line_number == target_line_number {
            let mut record =
                parse_technical_log_line(file_name, Some(line_number), line_start, line_text)?;
            append_technical_log_continuation_lines(
                &mut reader,
                file_name,
                &mut record,
                Some(line_number + 1),
                &mut byte_offset,
            );
            return Some(record);
        }

        line_number += 1;
    }
}

fn read_technical_log_record_at_offset(
    path: &Path,
    file_name: &str,
    byte_offset: u64,
) -> Option<TechnicalLogRecord> {
    let mut file = std::fs::File::open(path).ok()?;
    if byte_offset > file.metadata().ok()?.len() {
        return None;
    }
    file.seek(SeekFrom::Start(byte_offset)).ok()?;

    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).ok()?;
    if bytes_read == 0 {
        return None;
    }

    let mut next_offset = byte_offset + bytes_read as u64;
    let line_text = line.trim_end_matches(|ch| matches!(ch, '\r' | '\n'));
    let mut record = parse_technical_log_line(file_name, None, byte_offset, line_text)?;
    append_technical_log_continuation_lines(
        &mut reader,
        file_name,
        &mut record,
        None,
        &mut next_offset,
    );
    Some(record)
}

fn append_technical_log_continuation_lines<R: BufRead>(
    reader: &mut R,
    file_name: &str,
    record: &mut TechnicalLogRecord,
    mut line_number: Option<usize>,
    byte_offset: &mut u64,
) {
    let mut line = String::new();
    loop {
        line.clear();
        let Ok(bytes_read) = reader.read_line(&mut line) else {
            break;
        };
        if bytes_read == 0 {
            break;
        }

        let line_start = *byte_offset;
        *byte_offset += bytes_read as u64;
        let line_text = line.trim_end_matches(|ch| matches!(ch, '\r' | '\n'));
        if parse_technical_log_line(file_name, line_number, line_start, line_text).is_some() {
            break;
        }

        record.message.push('\n');
        record.message.push_str(line_text);
        if let Some(line_number) = &mut line_number {
            *line_number += 1;
        }
    }
}

fn parse_technical_log_line(
    file_name: &str,
    line_number: Option<usize>,
    byte_offset: u64,
    line: &str,
) -> Option<TechnicalLogRecord> {
    let (time, rest) = line.split_once(" [")?;
    let (level, rest) = rest.split_once("] ")?;
    let (target, message) = rest.split_once(": ")?;
    let level = parse_log_level(level.trim())?;
    let time = DateTime::parse_from_rfc3339(time).ok()?;

    let id = line_number.map_or_else(
        || format!("file:{file_name}:offset:{byte_offset}"),
        |line_number| format!("file:{file_name}:{line_number}"),
    );
    Some(TechnicalLogRecord {
        id,
        time,
        level,
        target: target.to_string(),
        message: message.to_string(),
        source: TechnicalLogSource::File,
        file_name: Some(file_name.to_string()),
        line_number,
    })
}

fn parse_log_level(value: &str) -> Option<LogLevel> {
    match value {
        "ERROR" => Some(LogLevel::Error),
        "WARN" => Some(LogLevel::Warn),
        "INFO" => Some(LogLevel::Info),
        "DEBUG" => Some(LogLevel::Debug),
        "TRACE" => Some(LogLevel::Trace),
        _ => None,
    }
}

impl TechnicalLogEntrySummary {
    fn from_record(record: TechnicalLogRecord, max_message_chars: usize) -> Self {
        let (message_preview, truncated) =
            truncate_technical_message(&record.message, max_message_chars);
        Self {
            id: record.id,
            time: format_technical_time(record.time),
            level: record.level,
            target: record.target,
            message_preview,
            truncated,
            source: record.source,
            file_name: record.file_name,
            line_number: record.line_number,
        }
    }
}

impl TechnicalLogEntryDetails {
    fn from_record(record: TechnicalLogRecord, max_message_chars: usize) -> Self {
        let (message, truncated) = truncate_technical_message(&record.message, max_message_chars);
        Self {
            id: record.id,
            time: format_technical_time(record.time),
            level: record.level,
            target: record.target,
            message,
            truncated,
            source: record.source,
            file_name: record.file_name,
            line_number: record.line_number,
        }
    }
}

fn technical_log_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(MCP_TECHNICAL_LOG_DEFAULT_LIMIT)
        .min(MCP_TECHNICAL_LOG_MAX_LIMIT)
        .max(1)
}

fn technical_log_message_limit(limit: Option<usize>, default_limit: usize) -> usize {
    limit.unwrap_or(default_limit).min(default_limit)
}

fn default_technical_levels() -> Vec<LogLevel> {
    vec![LogLevel::Error, LogLevel::Warn]
}

fn parse_optional_technical_time(
    name: &'static str,
    value: Option<&str>,
) -> Result<Option<DateTime<chrono::FixedOffset>>, TechnicalLogQueryError> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value).map_err(|e| {
                TechnicalLogQueryError::invalid_params(format!(
                    "Invalid RFC3339 {name} timestamp: {e}"
                ))
            })
        })
        .transpose()
}

fn normalized_string_set(values: Option<&[String]>) -> Option<Vec<String>> {
    values.map(|values| {
        values
            .iter()
            .filter_map(|value| {
                let value = value.trim();
                (!value.is_empty()).then(|| value.to_lowercase())
            })
            .collect()
    })
}

fn truncate_technical_message(message: &str, max_chars: usize) -> (String, bool) {
    let redacted = sanitize_log_text(message);
    truncate_chars(&redacted, max_chars)
}

#[cfg(test)]
fn redact_sensitive_text(message: &str) -> String {
    sanitize_log_text(message)
}

fn truncate_chars(value: &str, max_chars: usize) -> (String, bool) {
    let mut result = String::new();
    let mut truncated = false;
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            truncated = true;
            break;
        }
        result.push(ch);
    }
    (result, truncated)
}

fn technical_log_group_key(entry: &TechnicalLogRecord, group_by: TechnicalLogGroupBy) -> String {
    match group_by {
        TechnicalLogGroupBy::Level => format!("{:?}", entry.level),
        TechnicalLogGroupBy::Target => entry.target.clone(),
        TechnicalLogGroupBy::File => entry
            .file_name
            .clone()
            .unwrap_or_else(|| "memory".to_string()),
        TechnicalLogGroupBy::Hour => format!(
            "{:04}-{:02}-{:02}T{:02}:00",
            entry.time.year(),
            entry.time.month(),
            entry.time.day(),
            entry.time.hour()
        ),
    }
}

fn technical_log_time_is_newer(entry: &TechnicalLogRecord, current: Option<&str>) -> bool {
    let Some(current) = current else {
        return true;
    };
    DateTime::parse_from_rfc3339(current).is_ok_and(|current_time| entry.time > current_time)
}

fn format_technical_time(time: DateTime<chrono::FixedOffset>) -> String {
    time.to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

static LOG_BUFFER: Mutex<ConstGenericRingBuffer<LogEntry, 256>> =
    Mutex::new(ConstGenericRingBuffer::new());
static MCP_LOG_BUFFER: Mutex<ConstGenericRingBuffer<TechnicalMemoryLogEntry, 256>> =
    Mutex::new(ConstGenericRingBuffer::new());

struct Logger {
    sender: mpsc::Sender<LogChannelMessage>,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // TODO: configurable
        metadata.level() <= log::Level::Info
            || metadata.target().starts_with("vrc_get") && metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let entry = LogEntry::new(record);
        self.sender.send(LogChannelMessage::Log(entry)).ok();
    }

    fn flush(&self) {
        let (sync, receiver) = mpsc::channel();
        self.sender.send(LogChannelMessage::Flush(sync)).ok();
        receiver.recv().ok();
    }
}

fn log_err<T>(result: Result<T, impl Display>) {
    if let Err(e) = result {
        eprintln!("Error while logging: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    static TEST_LOG_LOCK: StdMutex<()> = StdMutex::new(());

    fn reset_mcp_log_buffer() {
        *MCP_LOG_BUFFER.lock().unwrap() = ConstGenericRingBuffer::new();
        NEXT_LOG_SEQUENCE.store(1, Ordering::Relaxed);
    }

    fn push_memory_log(level: LogLevel, target: &str, message: &str) {
        let sequence = NEXT_LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        MCP_LOG_BUFFER
            .lock()
            .unwrap()
            .enqueue(TechnicalMemoryLogEntry {
                sequence,
                entry: LogEntry {
                    time: chrono::Local::now(),
                    level,
                    target: target.to_string(),
                    message: message.to_string(),
                    gui_toast: None,
                },
            });
    }

    #[test]
    fn technical_log_search_defaults_to_error_and_warn() {
        let _guard = TEST_LOG_LOCK.lock().unwrap();
        reset_mcp_log_buffer();
        let temp = tempfile::tempdir().unwrap();
        push_memory_log(LogLevel::Info, "test::info", "info message");
        push_memory_log(LogLevel::Warn, "test::warn", "warn message");
        push_memory_log(LogLevel::Error, "test::error", "error message");

        let response =
            search_technical_logs(temp.path(), TechnicalLogSearchParams::default()).unwrap();

        assert_eq!(response.total_count, 2);
        assert!(
            response
                .entries
                .iter()
                .all(|entry| matches!(entry.level, LogLevel::Error | LogLevel::Warn))
        );
    }

    #[test]
    fn technical_log_search_clamps_zero_limit_to_one() {
        let _guard = TEST_LOG_LOCK.lock().unwrap();
        reset_mcp_log_buffer();
        let temp = tempfile::tempdir().unwrap();
        push_memory_log(LogLevel::Warn, "test::warn", "warn message");
        push_memory_log(LogLevel::Error, "test::error", "error message");

        let response = search_technical_logs(
            temp.path(),
            TechnicalLogSearchParams {
                limit: Some(0),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(response.limit, 1);
        assert_eq!(response.returned_count, 1);
        assert_eq!(response.next_offset, Some(1));
    }

    #[test]
    fn technical_log_summary_paginates_groups() {
        let _guard = TEST_LOG_LOCK.lock().unwrap();
        reset_mcp_log_buffer();
        let temp = tempfile::tempdir().unwrap();
        push_memory_log(LogLevel::Warn, "test::alpha", "alpha");
        push_memory_log(LogLevel::Warn, "test::beta", "beta");
        push_memory_log(LogLevel::Warn, "test::gamma", "gamma");

        let response = summarize_technical_logs(
            temp.path(),
            TechnicalLogSummaryParams {
                group_by: TechnicalLogGroupBy::Target,
                filter: TechnicalLogSearchParams {
                    offset: Some(1),
                    limit: Some(1),
                    ..Default::default()
                },
            },
        )
        .unwrap();

        assert_eq!(response.total_count, 3);
        assert_eq!(response.total_group_count, 3);
        assert_eq!(response.offset, 1);
        assert_eq!(response.limit, 1);
        assert_eq!(response.returned_count, 1);
        assert!(response.has_more);
        assert_eq!(response.next_offset, Some(2));
        assert_eq!(response.groups.len(), 1);
    }

    #[test]
    fn technical_log_summary_clamps_zero_limit_to_one() {
        let _guard = TEST_LOG_LOCK.lock().unwrap();
        reset_mcp_log_buffer();
        let temp = tempfile::tempdir().unwrap();
        push_memory_log(LogLevel::Warn, "test::warn", "warn message");

        let response = summarize_technical_logs(
            temp.path(),
            TechnicalLogSummaryParams {
                group_by: TechnicalLogGroupBy::Target,
                filter: TechnicalLogSearchParams {
                    limit: Some(0),
                    ..Default::default()
                },
            },
        )
        .unwrap();

        assert_eq!(response.limit, 1);
        assert_eq!(response.returned_count, 1);
        assert_eq!(response.next_offset, None);
    }

    #[test]
    fn technical_log_search_can_include_info_and_redacts_messages() {
        let _guard = TEST_LOG_LOCK.lock().unwrap();
        reset_mcp_log_buffer();
        let temp = tempfile::tempdir().unwrap();
        push_memory_log(
            LogLevel::Info,
            "test::info",
            "token=secret sk-project Authorization: Bearer abcdefghijklmnopqrstuvwxyz",
        );

        let response = search_technical_logs(
            temp.path(),
            TechnicalLogSearchParams {
                levels: Some(vec![LogLevel::Info]),
                max_message_chars: Some(24),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(response.total_count, 1);
        assert!(response.entries[0].truncated);
        assert!(!response.entries[0].message_preview.contains("secret"));
        assert!(!response.entries[0].message_preview.contains("sk-project"));
        assert!(
            !response.entries[0]
                .message_preview
                .contains("abcdefghijklmnopqrstuvwxyz")
        );

        let details = get_technical_log_entry(
            temp.path(),
            TechnicalLogEntryParams {
                id: response.entries[0].id.clone(),
                max_message_chars: Some(200),
            },
        )
        .unwrap();

        assert!(!details.message.contains("secret"));
        assert!(!details.message.contains("sk-project"));
        assert!(!details.message.contains("abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn technical_log_redaction_preserves_multiline_formatting() {
        let message = "first line\n    token=secret  value\n\tBearer hidden";

        let redacted = redact_sensitive_text(message);

        assert!(redacted.contains("first line\n    token=<redacted>  value\n\tBearer <redacted>"));
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("hidden"));
    }

    #[test]
    fn technical_log_redaction_covers_json_colon_and_quoted_tokens() {
        let message =
            r#"{"token":"json-secret"} api_key: plain-secret "sk-project" secret='quoted-secret'"#;

        let redacted = redact_sensitive_text(message);

        assert!(!redacted.contains("json-secret"));
        assert!(!redacted.contains("plain-secret"));
        assert!(!redacted.contains("sk-project"));
        assert!(!redacted.contains("quoted-secret"));
        assert!(redacted.contains("<redacted>"));
    }

    #[test]
    fn technical_log_redaction_covers_json_authorization_bearer_with_space() {
        let message = r#"{"Authorization":"Bearer abcdefghijklmnopqrstuvwxyz"} next"#;

        let redacted = redact_sensitive_text(message);

        assert!(!redacted.contains("abcdefghijklmnopqrstuvwxyz"));
        assert!(redacted.contains(r#""Authorization":<redacted> <redacted>"}"#));
        assert!(redacted.contains("next"));
    }

    #[test]
    fn technical_log_redaction_removes_url_credentials_and_query() {
        let message =
            r#"failed url="https://user:pass@example.com/index.json?token=secret#frag") next"#;

        let redacted = redact_sensitive_text(message);

        assert_eq!(
            redacted,
            r#"failed url="https://example.com/index.json") next"#
        );
    }

    #[test]
    fn technical_log_file_search_reads_matching_log_files_only() {
        let _guard = TEST_LOG_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("vrc-get-2026-06-27_10-00-00.000000.log"),
            "2026-06-27T10:00:00.000000+08:00 [ INFO] test::file: visible\n\
             not a log line\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("other-2026-06-27_10-00-00.000000.log"),
            "2026-06-27T10:00:00.000000+08:00 [ERROR] test::file: hidden\n",
        )
        .unwrap();

        let response = search_technical_logs(
            temp.path(),
            TechnicalLogSearchParams {
                levels: Some(vec![LogLevel::Info]),
                scope: TechnicalLogScope::RecentFiles,
                ..Default::default()
            },
        )
        .unwrap();
        let details = get_technical_log_entry(
            temp.path(),
            TechnicalLogEntryParams {
                id: response.entries[0].id.clone(),
                max_message_chars: None,
            },
        )
        .unwrap();

        assert_eq!(response.total_count, 1);
        assert_eq!(response.entries[0].line_number, Some(1));
        assert_eq!(details.message, "visible\nnot a log line");
    }

    #[test]
    fn technical_log_file_search_preserves_and_redacts_continuation_lines() {
        let _guard = TEST_LOG_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("vrc-get-2026-06-27_10-00-00.000000.log"),
            "2026-06-27T10:00:00.000000+08:00 [ERROR] test::file: failed\n\
             at C:\\项目\\世界\n\
             url=https://user:pass@example.com/index.json?token=secret\n",
        )
        .unwrap();

        let response = search_technical_logs(
            temp.path(),
            TechnicalLogSearchParams {
                scope: TechnicalLogScope::RecentFiles,
                max_message_chars: Some(1000),
                ..Default::default()
            },
        )
        .unwrap();
        let details = get_technical_log_entry(
            temp.path(),
            TechnicalLogEntryParams {
                id: response.entries[0].id.clone(),
                max_message_chars: None,
            },
        )
        .unwrap();

        assert_eq!(response.total_count, 1);
        assert!(details.message.contains("C:\\项目\\世界"));
        assert!(details.message.contains("https://example.com/index.json"));
        assert!(!details.message.contains("user:pass"));
        assert!(!details.message.contains("token=secret"));
    }

    #[test]
    fn technical_log_file_search_reads_large_recent_files_from_tail() {
        let _guard = TEST_LOG_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let padding = "x".repeat(MCP_TECHNICAL_LOG_RECENT_FILE_MAX_BYTES as usize + 128);
        std::fs::write(
            temp.path().join("vrc-get-2026-06-27_10-00-00.000000.log"),
            format!(
                "2026-06-27T10:00:00.000000+08:00 [ERROR] test::file: old\n\
                 {padding}\n\
                 2026-06-27T11:00:00.000000+08:00 [ERROR] test::file: recent x-api-key=secret\n\
                 at C:\\项目\\世界\n",
            ),
        )
        .unwrap();

        let response = search_technical_logs(
            temp.path(),
            TechnicalLogSearchParams {
                scope: TechnicalLogScope::RecentFiles,
                max_message_chars: Some(1000),
                ..Default::default()
            },
        )
        .unwrap();
        let entry = &response.entries[0];
        let details = get_technical_log_entry(
            temp.path(),
            TechnicalLogEntryParams {
                id: entry.id.clone(),
                max_message_chars: None,
            },
        )
        .unwrap();

        assert_eq!(response.total_count, 1);
        assert!(entry.id.contains(":offset:"));
        assert_eq!(entry.line_number, None);
        assert!(entry.message_preview.contains("recent"));
        assert!(!entry.message_preview.contains("secret"));
        assert!(!entry.message_preview.contains("old"));
        assert!(details.message.contains("recent"));
        assert!(details.message.contains("C:\\项目\\世界"));
        assert!(!details.message.contains("secret"));
    }

    #[test]
    fn technical_log_file_search_skips_truncated_utf8_prefix_by_bytes() {
        let _guard = TEST_LOG_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let recent = "2026-06-27T11:00:00.000000+08:00 [ERROR] test::file: recent\n";
        let filler_len = MCP_TECHNICAL_LOG_RECENT_FILE_MAX_BYTES as usize - 2 - 1 - recent.len();
        let mut content = Vec::new();
        content.extend_from_slice(b"2026-06-27T10:00:00.000000+08:00 [ERROR] test::file: old\n");
        content.extend_from_slice("项".as_bytes());
        content.extend_from_slice("x".repeat(filler_len).as_bytes());
        content.push(b'\n');
        content.extend_from_slice(recent.as_bytes());
        std::fs::write(
            temp.path().join("vrc-get-2026-06-27_10-00-00.000000.log"),
            content,
        )
        .unwrap();

        let response = search_technical_logs(
            temp.path(),
            TechnicalLogSearchParams {
                scope: TechnicalLogScope::RecentFiles,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(response.total_count, 1);
        assert!(response.entries[0].message_preview.contains("recent"));
        assert!(!response.entries[0].message_preview.contains("old"));
    }
}
