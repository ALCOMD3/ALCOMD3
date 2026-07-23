use crate::log_sanitization::{sanitize_log_text, sanitize_url};
use chrono::{DateTime, Datelike, SecondsFormat, Timelike};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::VecDeque;
use std::fmt::Display;
use std::future::Future;
use std::io::{BufRead, BufReader, Read as _, Seek, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Emitter};
use url::Url;
use uuid::Uuid;
use vrc_get_vpm::io::DefaultEnvironmentIo;

pub const ACTIVITY_LOG_EVENT: &str = "activity-log-entry";
pub const ACTIVITY_LOG_RELATIVE_FOLDER: &str = crate::storage::ACTIVITY_LOG_DIR;
pub const MAX_ACTIVITY_LOG_FILES: usize = 30;
pub const MAX_ACTIVITY_LOG_ENTRIES: usize = 1000;
pub const MCP_ACTIVITY_LOG_DEFAULT_LIMIT: usize = 50;
pub const MCP_ACTIVITY_LOG_MAX_LIMIT: usize = 200;
pub const MCP_ACTIVITY_LOG_DEFAULT_CONTEXT: usize = 5;
pub const MCP_ACTIVITY_LOG_MAX_CONTEXT: usize = 50;
const MCP_ACTIVITY_LOG_RECENT_FILE_MAX_BYTES: u64 = 1024 * 1024;

pub mod operations {
    pub const DEEP_LINK_ADD_REPOSITORY: &str = "deep_link.add_repository";
    pub const GUI_OPEN_PATH: &str = "gui.open_path";
    pub const GUI_OPEN_URL: &str = "gui.open_url";
    pub const LEGACY_IMPORT: &str = "legacy.import";
    pub const MCP_SET_ENABLED: &str = "mcp.set_enabled";
    pub const PACKAGES_REFRESH_CACHE: &str = "packages.refresh_cache";
    pub const PROJECTS_SYNC_REAL_INFO: &str = "projects.sync_real_info";
    pub const PROJECT_ADD: &str = "project.add";
    pub const PROJECT_APPLY_CHANGES: &str = "project.apply_changes";
    pub const PROJECT_BACKUP: &str = "project.backup";
    pub const PROJECT_COPY: &str = "project.copy";
    pub const PROJECT_CREATE: &str = "project.create";
    pub const PROJECT_INSTALL_PACKAGES: &str = "project.install_packages";
    pub const PROJECT_OPEN_UNITY: &str = "project.open_unity";
    pub const PROJECT_REMOVE: &str = "project.remove";
    pub const PROJECT_REMOVE_PACKAGES: &str = "project.remove_packages";
    pub const PROJECT_REINSTALL_PACKAGES: &str = "project.reinstall_packages";
    pub const PROJECT_RESTORE: &str = "project.restore";
    pub const PROJECT_RESOLVE_PACKAGES: &str = "project.resolve_packages";
    pub const PROJECT_SET_CUSTOM_UNITY_ARGS: &str = "project.set_custom_unity_args";
    pub const PROJECT_SET_FAVORITE: &str = "project.set_favorite";
    pub const PROJECT_SET_UNITY_PATH: &str = "project.set_unity_path";
    pub const REPOSITORY_ADD: &str = "repository.add";
    pub const REPOSITORY_CLEAR_CACHE: &str = "repository.clear_package_cache";
    pub const REPOSITORY_EXPORT: &str = "repository.export";
    pub const REPOSITORY_HIDE: &str = "repository.hide";
    pub const REPOSITORY_IMPORT: &str = "repository.import";
    pub const REPOSITORY_REMOVE: &str = "repository.remove";
    pub const REPOSITORY_REORDER: &str = "repository.reorder";
    pub const REPOSITORY_SHOW: &str = "repository.show";
    pub const SIDEBAR_EXTENSION_INSTALLED: &str = "sidebar_extension.installed";
    pub const SIDEBAR_EXTENSION_REORDER: &str = "sidebar_extension.reorder";
    pub const SIDEBAR_EXTENSION_VISIBLE: &str = "sidebar_extension.visible";
    pub const SETTINGS_SET: &str = "settings.set";
    pub const TEMPLATE_EXPORT: &str = "template.export";
    pub const TEMPLATE_IMPORT: &str = "template.import";
    pub const TEMPLATE_REMOVE: &str = "template.remove";
    pub const TEMPLATE_SAVE: &str = "template.save";
    pub const UNITY_HUB_REFRESH: &str = "unity_hub.refresh_paths";
    pub const UPDATE_CHECK: &str = "update.check";
    pub const UPDATE_DOWNLOAD: &str = "update.download";
    pub const UPDATE_INSTALL: &str = "update.install";
    pub const USER_PACKAGE_ADD: &str = "user_package.add";
    pub const USER_PACKAGE_REMOVE: &str = "user_package.remove";
}

#[derive(
    Serialize, Deserialize, specta::Type, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum ActivitySource {
    #[serde(alias = "gui", alias = "GUI")]
    Gui,
    #[serde(alias = "mcp", alias = "MCP")]
    Mcp,
    #[serde(alias = "deep_link", alias = "deeplink", alias = "Deep Link")]
    DeepLink,
    #[serde(alias = "system", alias = "SYSTEM")]
    System,
}

#[derive(
    Serialize, Deserialize, specta::Type, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum ActivityKind {
    #[serde(alias = "read")]
    Read,
    #[serde(alias = "write")]
    Write,
    #[serde(alias = "passive")]
    Passive,
    #[serde(alias = "open")]
    Open,
    #[serde(alias = "maintenance")]
    Maintenance,
}

#[derive(
    Serialize, Deserialize, specta::Type, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum ActivityStatus {
    #[serde(alias = "started", alias = "running")]
    Started,
    #[serde(alias = "succeeded", alias = "success", alias = "completed")]
    Succeeded,
    #[serde(alias = "failed", alias = "failure", alias = "error")]
    Failed,
    #[serde(alias = "cancelled", alias = "canceled")]
    Cancelled,
    #[serde(alias = "info")]
    Info,
}

#[derive(
    Serialize, Deserialize, specta::Type, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum ActivityImportance {
    #[serde(alias = "primary")]
    Primary,
    #[serde(alias = "secondary")]
    Secondary,
    #[serde(alias = "technical")]
    Technical,
}

#[derive(Serialize, Deserialize, specta::Type, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDetail {
    key: String,
    value: String,
}

impl ActivityDetail {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

#[derive(Serialize, Deserialize, specta::Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    id: String,
    source: ActivitySource,
    kind: ActivityKind,
    status: ActivityStatus,
    importance: ActivityImportance,
    operation: String,
    summary: String,
    target: Option<String>,
    details: Vec<ActivityDetail>,
    request_id: Option<String>,
    tool_name: Option<String>,
    client_name: Option<String>,
    started_at: String,
    finished_at: Option<String>,
    duration_ms: Option<u64>,
    error: Option<String>,
}

impl ActivityEntry {
    fn matches_search(&self, search: &str) -> bool {
        if search.is_empty() {
            return true;
        }

        let search = search.to_lowercase();
        self.operation.to_lowercase().contains(&search)
            || self.summary.to_lowercase().contains(&search)
            || self
                .target
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&search)
            || self
                .tool_name
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&search)
            || self
                .client_name
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&search)
            || self
                .error
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&search)
            || self.details.iter().any(|detail| {
                detail.key.to_lowercase().contains(&search)
                    || detail.value.to_lowercase().contains(&search)
            })
    }
}

#[derive(Deserialize, specta::Type, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntryFilter {
    #[serde(default)]
    source: Option<ActivitySource>,
    #[serde(default)]
    kind: Option<ActivityKind>,
    #[serde(default)]
    status: Option<ActivityStatus>,
    #[serde(default)]
    include_secondary: bool,
    #[serde(default)]
    include_technical: bool,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityLogVisibility {
    #[default]
    Important,
    Primary,
    Secondary,
    Technical,
    All,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityLogOrder {
    #[default]
    Newest,
    Oldest,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityLogGroupBy {
    #[default]
    Source,
    Kind,
    Status,
    Operation,
    ToolName,
    ClientName,
    Day,
    Hour,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ActivityLogSearchParams {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub sources: Option<Vec<ActivitySource>>,
    #[serde(default)]
    pub kinds: Option<Vec<ActivityKind>>,
    #[serde(default)]
    pub statuses: Option<Vec<ActivityStatus>>,
    #[serde(default)]
    pub visibility: ActivityLogVisibility,
    #[serde(default)]
    pub operations: Option<Vec<String>>,
    #[serde(default)]
    pub tool_names: Option<Vec<String>>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub order: ActivityLogOrder,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ActivityLogSummaryParams {
    #[serde(flatten)]
    pub filter: ActivityLogSearchParams,
    #[serde(default)]
    pub group_by: ActivityLogGroupBy,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ActivityLogEntryParams {
    pub id: String,
    #[serde(default = "default_true")]
    pub include_details: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ActivityLogContextParams {
    pub id: String,
    #[serde(default = "default_activity_log_context")]
    pub before: usize,
    #[serde(default = "default_activity_log_context")]
    pub after: usize,
    #[serde(default)]
    pub include_details: bool,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLogEntrySummary {
    id: String,
    started_at: String,
    finished_at: Option<String>,
    source: ActivitySource,
    kind: ActivityKind,
    status: ActivityStatus,
    importance: ActivityImportance,
    operation: String,
    summary: String,
    target: Option<String>,
    duration_ms: Option<u64>,
    request_id: Option<String>,
    tool_name: Option<String>,
    client_name: Option<String>,
    detail_count: usize,
    has_error: bool,
    error_summary: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLogSearchResponse {
    ok: bool,
    total_count: usize,
    offset: usize,
    limit: usize,
    returned_count: usize,
    has_more: bool,
    next_offset: Option<usize>,
    entries: Vec<ActivityLogEntrySummary>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLogSummaryGroup {
    key: String,
    count: usize,
    failed_count: usize,
    cancelled_count: usize,
    latest_entry_id: Option<String>,
    latest_started_at: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLogSummaryResponse {
    ok: bool,
    group_by: ActivityLogGroupBy,
    total_count: usize,
    total_group_count: usize,
    offset: usize,
    limit: usize,
    returned_count: usize,
    has_more: bool,
    next_offset: Option<usize>,
    groups: Vec<ActivityLogSummaryGroup>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLogContextResponse {
    ok: bool,
    entry: ActivityEntry,
    before: Vec<ActivityEntry>,
    after: Vec<ActivityEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityLogQueryError {
    code: &'static str,
    message: String,
}

impl ActivityLogQueryError {
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

fn default_true() -> bool {
    true
}

fn default_activity_log_context() -> usize {
    MCP_ACTIVITY_LOG_DEFAULT_CONTEXT
}

#[derive(Debug, Clone)]
pub struct ActivityInput {
    source: ActivitySource,
    kind: ActivityKind,
    importance: ActivityImportance,
    operation: String,
    summary: String,
    target: Option<String>,
    details: Vec<ActivityDetail>,
    request_id: Option<String>,
    tool_name: Option<String>,
    client_name: Option<String>,
}

impl ActivityInput {
    pub fn new(
        source: ActivitySource,
        kind: ActivityKind,
        importance: ActivityImportance,
        operation: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            source,
            kind,
            importance,
            operation: operation.into(),
            summary: summary.into(),
            target: None,
            details: Vec::new(),
            request_id: None,
            tool_name: None,
            client_name: None,
        }
    }

    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn details(mut self, details: Vec<ActivityDetail>) -> Self {
        self.details = details;
        self
    }

    pub fn add_detail(mut self, detail: ActivityDetail) -> Self {
        self.details.push(detail);
        self
    }

    pub fn add_details(mut self, mut details: Vec<ActivityDetail>) -> Self {
        self.details.append(&mut details);
        self
    }

    pub fn request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    pub fn tool_name(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }

    pub fn client_name(mut self, client_name: impl Into<String>) -> Self {
        self.client_name = Some(client_name.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct ActivityTracker {
    id: String,
    source: ActivitySource,
    kind: ActivityKind,
    importance: ActivityImportance,
    operation: String,
    target: Option<String>,
    details: Vec<ActivityDetail>,
    request_id: Option<String>,
    tool_name: Option<String>,
    client_name: Option<String>,
    started_at: DateTime<chrono::Local>,
    completed: Arc<AtomicBool>,
}

pub struct ActivityLogState {
    folder: PathBuf,
    entries: Mutex<VecDeque<ActivityEntry>>,
}

impl ActivityLogState {
    pub fn new(io: &DefaultEnvironmentIo) -> Self {
        let folder = io.resolve(Path::new(ACTIVITY_LOG_RELATIVE_FOLDER));
        Self::new_with_folder(folder)
    }

    fn new_with_folder(folder: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&folder) {
            log::error!("failed to create activity log folder: {e}");
        }
        remove_old_activity_logs(&folder);
        let entries = load_recent_activity_entries(&folder);
        let mut buffer = VecDeque::with_capacity(MAX_ACTIVITY_LOG_ENTRIES.min(entries.len()));
        for entry in entries {
            buffer.push_back(entry);
        }
        Self {
            folder,
            entries: Mutex::new(buffer),
        }
    }

    pub fn log_folder(&self) -> &Path {
        &self.folder
    }

    pub fn record_info(&self, app: Option<&AppHandle>, input: ActivityInput) -> ActivityEntry {
        let now = chrono::Local::now();
        let entry = build_entry(
            Uuid::new_v4().to_string(),
            input,
            ActivityStatus::Info,
            now,
            None,
            None,
        );
        self.record_entry(app, entry.clone());
        entry
    }

    pub fn record_failed(
        &self,
        app: Option<&AppHandle>,
        input: ActivityInput,
        error: impl Display,
    ) -> ActivityEntry {
        let now = chrono::Local::now();
        let entry = build_entry(
            Uuid::new_v4().to_string(),
            input,
            ActivityStatus::Failed,
            now,
            Some(now),
            Some(sanitize_log_text(&error.to_string())),
        );
        self.record_entry(app, entry.clone());
        entry
    }

    pub fn start_activity(&self, app: Option<&AppHandle>, input: ActivityInput) -> ActivityTracker {
        let id = Uuid::new_v4().to_string();
        let started_at = chrono::Local::now();
        let entry = build_entry(
            id.clone(),
            input.clone(),
            ActivityStatus::Started,
            started_at,
            None,
            None,
        );
        self.record_entry(app, entry);
        ActivityTracker {
            id,
            source: input.source,
            kind: input.kind,
            importance: input.importance,
            operation: input.operation,
            target: input.target,
            details: input.details,
            request_id: input.request_id,
            tool_name: input.tool_name,
            client_name: input.client_name,
            started_at,
            completed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn track_result<T, E, Fut>(
        &self,
        app: Option<&AppHandle>,
        input: ActivityInput,
        success_summary: impl Into<String>,
        success_details: Vec<ActivityDetail>,
        future: Fut,
    ) -> Result<T, E>
    where
        E: Display,
        Fut: Future<Output = Result<T, E>>,
    {
        let tracker = self.start_activity(app, input);
        let result = future.await;
        match &result {
            Ok(_) => {
                self.finish_success(app, &tracker, success_summary, success_details);
            }
            Err(error) => {
                self.finish_failed(app, &tracker, "Activity failed", Vec::new(), error);
            }
        }
        result
    }

    pub fn finish_success(
        &self,
        app: Option<&AppHandle>,
        tracker: &ActivityTracker,
        summary: impl Into<String>,
        details: Vec<ActivityDetail>,
    ) -> Option<ActivityEntry> {
        self.finish_activity(
            app,
            tracker,
            ActivityStatus::Succeeded,
            summary.into(),
            details,
            None,
        )
    }

    pub fn finish_failed(
        &self,
        app: Option<&AppHandle>,
        tracker: &ActivityTracker,
        summary: impl Into<String>,
        details: Vec<ActivityDetail>,
        error: impl Display,
    ) -> Option<ActivityEntry> {
        self.finish_activity(
            app,
            tracker,
            ActivityStatus::Failed,
            summary.into(),
            details,
            Some(sanitize_log_text(&error.to_string())),
        )
    }

    pub fn finish_info(
        &self,
        app: Option<&AppHandle>,
        tracker: &ActivityTracker,
        summary: impl Into<String>,
        details: Vec<ActivityDetail>,
    ) -> Option<ActivityEntry> {
        self.finish_activity(
            app,
            tracker,
            ActivityStatus::Info,
            summary.into(),
            details,
            None,
        )
    }

    pub fn finish_cancelled(
        &self,
        app: Option<&AppHandle>,
        tracker: &ActivityTracker,
        summary: impl Into<String>,
        details: Vec<ActivityDetail>,
    ) -> Option<ActivityEntry> {
        self.finish_activity(
            app,
            tracker,
            ActivityStatus::Cancelled,
            summary.into(),
            details,
            None,
        )
    }

    fn finish_activity(
        &self,
        app: Option<&AppHandle>,
        tracker: &ActivityTracker,
        status: ActivityStatus,
        summary: String,
        mut details: Vec<ActivityDetail>,
        error: Option<String>,
    ) -> Option<ActivityEntry> {
        if tracker.completed.swap(true, Ordering::SeqCst) {
            return None;
        }

        if details.is_empty() {
            details = tracker.details.clone();
        }

        let finished_at = chrono::Local::now();
        let duration_ms = finished_at
            .signed_duration_since(tracker.started_at)
            .num_milliseconds()
            .try_into()
            .ok();
        let input = ActivityInput {
            source: tracker.source,
            kind: tracker.kind,
            importance: tracker.importance,
            operation: tracker.operation.clone(),
            summary,
            target: tracker.target.clone(),
            details,
            request_id: tracker.request_id.clone(),
            tool_name: tracker.tool_name.clone(),
            client_name: tracker.client_name.clone(),
        };
        let entry = build_entry(
            tracker.id.clone(),
            input,
            status,
            tracker.started_at,
            Some(finished_at),
            error,
        );
        self.record_entry(app, entry.clone());
        Some(ActivityEntry {
            duration_ms,
            ..entry
        })
    }

    pub fn get_entries(&self, filter: ActivityEntryFilter) -> Vec<ActivityEntry> {
        let entries = self.entries.lock().unwrap().iter().cloned().collect();
        filter_entries(entries, filter)
    }

    pub fn search_entries(
        &self,
        params: ActivityLogSearchParams,
    ) -> Result<ActivityLogSearchResponse, ActivityLogQueryError> {
        let entries = query_activity_entries_from_files(&self.folder, &params)?;
        let offset = params.offset.unwrap_or(0);
        let limit = activity_log_limit(params.limit);
        let total_count = entries.len();
        let entries = entries
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(ActivityLogEntrySummary::from_entry)
            .collect::<Vec<_>>();
        let returned_count = entries.len();
        let next_offset = offset
            .checked_add(returned_count)
            .filter(|next| *next < total_count);

        Ok(ActivityLogSearchResponse {
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

    pub fn get_entry(
        &self,
        params: ActivityLogEntryParams,
    ) -> Result<ActivityEntry, ActivityLogQueryError> {
        let entry = all_coalesced_activity_entries(&self.folder)
            .into_iter()
            .find(|entry| entry.id == params.id)
            .ok_or_else(|| {
                ActivityLogQueryError::not_found(format!(
                    "Activity log entry not found: {}",
                    params.id
                ))
            })?;
        Ok(activity_entry_with_details(entry, params.include_details))
    }

    pub fn summarize_entries(
        &self,
        params: ActivityLogSummaryParams,
    ) -> Result<ActivityLogSummaryResponse, ActivityLogQueryError> {
        let entries = query_activity_entries_from_files(&self.folder, &params.filter)?;
        let total_count = entries.len();
        let mut groups = IndexMap::<String, ActivityLogSummaryGroup>::new();

        for entry in entries {
            let key = activity_group_key(&entry, params.group_by);
            let group = groups
                .entry(key.clone())
                .or_insert_with(|| ActivityLogSummaryGroup {
                    key,
                    count: 0,
                    failed_count: 0,
                    cancelled_count: 0,
                    latest_entry_id: None,
                    latest_started_at: None,
                });
            group.count += 1;
            if entry.status == ActivityStatus::Failed {
                group.failed_count += 1;
            }
            if entry.status == ActivityStatus::Cancelled {
                group.cancelled_count += 1;
            }
            if activity_started_at_is_newer(&entry, group.latest_started_at.as_deref()) {
                group.latest_entry_id = Some(entry.id.clone());
                group.latest_started_at = Some(entry.started_at.clone());
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
        let limit = activity_log_limit(params.filter.limit);
        let groups = groups
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        let returned_count = groups.len();
        let next_offset = offset
            .checked_add(returned_count)
            .filter(|next| *next < total_group_count);

        Ok(ActivityLogSummaryResponse {
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

    pub fn entry_context(
        &self,
        params: ActivityLogContextParams,
    ) -> Result<ActivityLogContextResponse, ActivityLogQueryError> {
        let entries = all_coalesced_activity_entries(&self.folder);
        let Some(index) = entries.iter().position(|entry| entry.id == params.id) else {
            return Err(ActivityLogQueryError::not_found(format!(
                "Activity log entry not found: {}",
                params.id
            )));
        };

        let before_count = params.before.min(MCP_ACTIVITY_LOG_MAX_CONTEXT);
        let after_count = params.after.min(MCP_ACTIVITY_LOG_MAX_CONTEXT);
        let before_start = index.saturating_sub(before_count);
        let before = entries[before_start..index]
            .iter()
            .cloned()
            .map(|entry| activity_entry_with_details(entry, params.include_details))
            .collect();
        let after_end = (index + 1 + after_count).min(entries.len());
        let after = entries[index + 1..after_end]
            .iter()
            .cloned()
            .map(|entry| activity_entry_with_details(entry, params.include_details))
            .collect();
        let entry = activity_entry_with_details(entries[index].clone(), params.include_details);

        Ok(ActivityLogContextResponse {
            ok: true,
            entry,
            before,
            after,
        })
    }

    fn record_entry(&self, app: Option<&AppHandle>, entry: ActivityEntry) {
        {
            let mut entries = self.entries.lock().unwrap();
            if let Err(e) = append_entry(&self.folder, &entry) {
                log::error!("failed to write activity log: {e}");
            }
            entries.push_back(entry.clone());
            while entries.len() > MAX_ACTIVITY_LOG_ENTRIES {
                entries.pop_front();
            }
        }

        if let Some(app) = app
            && let Err(e) = app.emit(ACTIVITY_LOG_EVENT, entry)
        {
            log::error!("failed to emit activity log event: {e}");
        }
    }
}

fn build_entry(
    id: String,
    input: ActivityInput,
    status: ActivityStatus,
    started_at: DateTime<chrono::Local>,
    finished_at: Option<DateTime<chrono::Local>>,
    error: Option<String>,
) -> ActivityEntry {
    let duration_ms = finished_at.and_then(|finished_at| {
        finished_at
            .signed_duration_since(started_at)
            .num_milliseconds()
            .try_into()
            .ok()
    });
    ActivityEntry {
        id,
        source: input.source,
        kind: input.kind,
        status,
        importance: input.importance,
        operation: input.operation,
        summary: input.summary,
        target: input.target,
        details: input.details,
        request_id: input.request_id,
        tool_name: input.tool_name,
        client_name: input.client_name,
        started_at: format_time(started_at),
        finished_at: finished_at.map(format_time),
        duration_ms,
        error,
    }
}

fn format_time(time: DateTime<chrono::Local>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, false)
}

fn filter_entries(entries: Vec<ActivityEntry>, filter: ActivityEntryFilter) -> Vec<ActivityEntry> {
    let mut coalesced = IndexMap::<String, ActivityEntry>::new();
    for entry in entries {
        coalesced.insert(entry.id.clone(), entry);
    }

    let search = filter.search.unwrap_or_default();
    let mut entries = coalesced
        .into_values()
        .filter(|entry| {
            filter.source.is_none_or(|source| entry.source == source)
                && filter.kind.is_none_or(|kind| entry.kind == kind)
                && filter.status.is_none_or(|status| entry.status == status)
                && importance_visible(entry, filter.include_secondary, filter.include_technical)
                && entry.matches_search(&search)
        })
        .collect::<Vec<_>>();
    entries.reverse();
    entries.truncate(filter.limit.unwrap_or(500).min(2000));
    entries
}

fn importance_visible(
    entry: &ActivityEntry,
    include_secondary: bool,
    include_technical: bool,
) -> bool {
    match entry.importance {
        ActivityImportance::Primary => true,
        ActivityImportance::Secondary => {
            include_secondary
                || matches!(
                    entry.status,
                    ActivityStatus::Failed | ActivityStatus::Cancelled
                )
        }
        ActivityImportance::Technical => include_technical,
    }
}

fn query_activity_entries_from_files(
    folder: &Path,
    params: &ActivityLogSearchParams,
) -> Result<Vec<ActivityEntry>, ActivityLogQueryError> {
    let since = parse_optional_activity_time("since", params.since.as_deref())?;
    let until = parse_optional_activity_time("until", params.until.as_deref())?;
    if let (Some(since), Some(until)) = (since, until)
        && since > until
    {
        return Err(ActivityLogQueryError::invalid_params(
            "since must be before or equal to until",
        ));
    }

    let search = params.search.as_deref().unwrap_or_default();
    let target = params.target.as_deref().map(str::to_lowercase);
    let operations = normalized_string_set(params.operations.as_deref());
    let tool_names = normalized_string_set(params.tool_names.as_deref());

    let mut entries = all_coalesced_activity_entries(folder)
        .into_iter()
        .filter(|entry| activity_matches_visibility(entry, params.visibility))
        .filter(|entry| {
            params
                .sources
                .as_ref()
                .is_none_or(|sources| sources.contains(&entry.source))
        })
        .filter(|entry| {
            params
                .kinds
                .as_ref()
                .is_none_or(|kinds| kinds.contains(&entry.kind))
        })
        .filter(|entry| {
            params
                .statuses
                .as_ref()
                .is_none_or(|statuses| statuses.contains(&entry.status))
        })
        .filter(|entry| {
            operations
                .as_ref()
                .is_none_or(|operations| operations.contains(&entry.operation.to_lowercase()))
        })
        .filter(|entry| {
            tool_names.as_ref().is_none_or(|tool_names| {
                entry
                    .tool_name
                    .as_deref()
                    .is_some_and(|tool_name| tool_names.contains(&tool_name.to_lowercase()))
            })
        })
        .filter(|entry| {
            params
                .request_id
                .as_deref()
                .is_none_or(|request_id| entry.request_id.as_deref() == Some(request_id))
        })
        .filter(|entry| {
            target.as_deref().is_none_or(|target| {
                entry
                    .target
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(target)
            })
        })
        .filter(|entry| entry.matches_search(search))
        .filter(|entry| activity_time_is_in_range(entry, since, until))
        .collect::<Vec<_>>();

    match params.order {
        ActivityLogOrder::Newest => entries.reverse(),
        ActivityLogOrder::Oldest => {}
    }

    Ok(entries)
}

fn all_coalesced_activity_entries(folder: &Path) -> Vec<ActivityEntry> {
    coalesce_activity_entries(load_activity_entries_from_recent_files(folder))
}

fn coalesce_activity_entries(entries: Vec<ActivityEntry>) -> Vec<ActivityEntry> {
    let mut coalesced = IndexMap::<String, ActivityEntry>::new();
    for entry in entries {
        coalesced.insert(entry.id.clone(), entry);
    }
    coalesced.into_values().collect()
}

fn activity_log_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(MCP_ACTIVITY_LOG_DEFAULT_LIMIT)
        .min(MCP_ACTIVITY_LOG_MAX_LIMIT)
        .max(1)
}

fn activity_matches_visibility(entry: &ActivityEntry, visibility: ActivityLogVisibility) -> bool {
    match visibility {
        ActivityLogVisibility::Important => importance_visible(entry, false, false),
        ActivityLogVisibility::Primary => entry.importance == ActivityImportance::Primary,
        ActivityLogVisibility::Secondary => entry.importance == ActivityImportance::Secondary,
        ActivityLogVisibility::Technical => entry.importance == ActivityImportance::Technical,
        ActivityLogVisibility::All => true,
    }
}

fn parse_optional_activity_time(
    name: &'static str,
    value: Option<&str>,
) -> Result<Option<DateTime<chrono::FixedOffset>>, ActivityLogQueryError> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value).map_err(|e| {
                ActivityLogQueryError::invalid_params(format!(
                    "Invalid RFC3339 {name} timestamp: {e}"
                ))
            })
        })
        .transpose()
}

fn activity_time_is_in_range(
    entry: &ActivityEntry,
    since: Option<DateTime<chrono::FixedOffset>>,
    until: Option<DateTime<chrono::FixedOffset>>,
) -> bool {
    let Some(time) = parse_activity_entry_time(entry) else {
        return true;
    };
    since.is_none_or(|since| time >= since) && until.is_none_or(|until| time <= until)
}

fn parse_activity_entry_time(entry: &ActivityEntry) -> Option<DateTime<chrono::FixedOffset>> {
    DateTime::parse_from_rfc3339(&entry.started_at).ok()
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

fn activity_entry_with_details(mut entry: ActivityEntry, include_details: bool) -> ActivityEntry {
    if !include_details {
        entry.details.clear();
    }
    entry
}

impl ActivityLogEntrySummary {
    fn from_entry(entry: ActivityEntry) -> Self {
        Self {
            id: entry.id,
            started_at: entry.started_at,
            finished_at: entry.finished_at,
            source: entry.source,
            kind: entry.kind,
            status: entry.status,
            importance: entry.importance,
            operation: entry.operation,
            summary: entry.summary,
            target: entry.target,
            duration_ms: entry.duration_ms,
            request_id: entry.request_id,
            tool_name: entry.tool_name,
            client_name: entry.client_name,
            detail_count: entry.details.len(),
            has_error: entry.error.is_some(),
            error_summary: entry.error.map(truncate_activity_error),
        }
    }
}

fn truncate_activity_error(error: String) -> String {
    const MAX_ERROR_SUMMARY_CHARS: usize = 300;
    truncate_chars(&error, MAX_ERROR_SUMMARY_CHARS).0
}

fn activity_group_key(entry: &ActivityEntry, group_by: ActivityLogGroupBy) -> String {
    match group_by {
        ActivityLogGroupBy::Source => format!("{:?}", entry.source),
        ActivityLogGroupBy::Kind => format!("{:?}", entry.kind),
        ActivityLogGroupBy::Status => format!("{:?}", entry.status),
        ActivityLogGroupBy::Operation => entry.operation.clone(),
        ActivityLogGroupBy::ToolName => entry.tool_name.clone().unwrap_or_else(|| "-".to_string()),
        ActivityLogGroupBy::ClientName => {
            entry.client_name.clone().unwrap_or_else(|| "-".to_string())
        }
        ActivityLogGroupBy::Day => parse_activity_entry_time(entry)
            .map(|time| format!("{:04}-{:02}-{:02}", time.year(), time.month(), time.day()))
            .unwrap_or_else(|| "-".to_string()),
        ActivityLogGroupBy::Hour => parse_activity_entry_time(entry)
            .map(|time| {
                format!(
                    "{:04}-{:02}-{:02}T{:02}:00",
                    time.year(),
                    time.month(),
                    time.day(),
                    time.hour()
                )
            })
            .unwrap_or_else(|| "-".to_string()),
    }
}

fn activity_started_at_is_newer(entry: &ActivityEntry, current: Option<&str>) -> bool {
    let Some(current) = current else {
        return true;
    };
    let Some(entry_time) = parse_activity_entry_time(entry) else {
        return false;
    };
    DateTime::parse_from_rfc3339(current).is_ok_and(|current_time| entry_time > current_time)
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

fn append_entry(folder: &Path, entry: &ActivityEntry) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let path = folder.join(format!(
        "activity-{}.jsonl",
        chrono::Local::now().format("%Y-%m-%d")
    ));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    serde_json::to_writer(&mut file, entry)?;
    writeln!(file)?;
    Ok(())
}

fn load_recent_activity_entries(folder: &Path) -> Vec<ActivityEntry> {
    load_activity_entries_from_recent_files(folder)
        .into_iter()
        .rev()
        .take(MAX_ACTIVITY_LOG_ENTRIES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn load_activity_entries_from_recent_files(folder: &Path) -> Vec<ActivityEntry> {
    let Ok(read_dir) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files = read_dir
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            is_activity_log_file_name(&name).then_some((name, entry.path()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(name, _)| name.clone());

    let mut entries = Vec::new();
    for (_, path) in files.into_iter().rev().take(MAX_ACTIVITY_LOG_FILES).rev() {
        entries.extend(read_activity_entries_from_file(&path));
    }
    entries
}

fn read_activity_entries_from_file(path: &Path) -> Vec<ActivityEntry> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(file_len) = file.metadata().map(|metadata| metadata.len()) else {
        return Vec::new();
    };
    let start_offset = file_len.saturating_sub(MCP_ACTIVITY_LOG_RECENT_FILE_MAX_BYTES);
    let mut read_offset = start_offset;

    if read_offset > 0 && !activity_file_offset_is_line_boundary(&mut file, read_offset) {
        if file.seek(SeekFrom::Start(read_offset)).is_err() {
            return Vec::new();
        }
        let mut reader = BufReader::new(file);
        let mut skipped = Vec::new();
        let Ok(skipped_bytes) = reader.read_until(b'\n', &mut skipped) else {
            return Vec::new();
        };
        read_offset += skipped_bytes as u64;
        return read_activity_entries_from_reader(reader, read_offset);
    }

    if file.seek(SeekFrom::Start(read_offset)).is_err() {
        return Vec::new();
    }
    read_activity_entries_from_reader(BufReader::new(file), read_offset)
}

fn activity_file_offset_is_line_boundary(file: &mut std::fs::File, offset: u64) -> bool {
    if offset == 0 {
        return true;
    }
    if file.seek(SeekFrom::Start(offset - 1)).is_err() {
        return false;
    }
    let mut previous = [0];
    file.read_exact(&mut previous).is_ok() && previous[0] == b'\n'
}

fn read_activity_entries_from_reader<R: BufRead>(
    mut reader: R,
    mut byte_offset: u64,
) -> Vec<ActivityEntry> {
    let mut entries = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        let Ok(bytes_read) = reader.read_line(&mut line) else {
            break;
        };
        if bytes_read == 0 {
            break;
        }
        byte_offset += bytes_read as u64;

        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<ActivityEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                log::debug!("skipping invalid activity log line at byte {byte_offset}: {e}");
            }
        };
    }
    entries
}

fn remove_old_activity_logs(folder: &Path) {
    let Ok(read_dir) = std::fs::read_dir(folder) else {
        return;
    };
    let mut files = read_dir
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            is_activity_log_file_name(&name).then_some((name, entry.path()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(name, _)| Reverse(name.clone()));

    for (_, path) in files.into_iter().skip(MAX_ACTIVITY_LOG_FILES) {
        if let Err(e) = std::fs::remove_file(&path) {
            log::debug!("failed to remove old activity log {}: {e}", path.display());
        }
    }
}

fn is_activity_log_file_name(name: &str) -> bool {
    if name.len() != "activity-yyyy-mm-dd.jsonl".len() {
        return false;
    }
    let Some(name) = name.strip_prefix("activity-") else {
        return false;
    };
    let Some(name) = name.strip_suffix(".jsonl") else {
        return false;
    };
    let bytes = name.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

pub fn summarize_url(value: &str) -> String {
    sanitize_url(value)
}

pub fn summarize_url_host(value: &str) -> String {
    Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(ToString::to_string))
        .unwrap_or_else(|| summarize_url(value))
}

pub fn summarize_path(value: impl AsRef<Path>) -> String {
    let path = value.as_ref();
    if let Some(home) = dirs_next::home_dir()
        && let Ok(stripped) = path.strip_prefix(&home)
    {
        return format!("~{}{}", std::path::MAIN_SEPARATOR, stripped.display());
    }
    path.display().to_string()
}

pub fn target_from_path(value: impl AsRef<Path>) -> String {
    let path = value.as_ref();
    path.file_name()
        .and_then(|value| value.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| summarize_path(path))
}

pub fn safe_detail_from_json(key: impl Into<String>, value: &Value) -> ActivityDetail {
    ActivityDetail::new(key, summarize_json_value(value))
}

fn summarize_json_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => {
            if looks_sensitive(value) {
                "<redacted>".to_string()
            } else if Url::parse(value).is_ok() {
                summarize_url(value)
            } else {
                value.chars().take(120).collect()
            }
        }
        Value::Array(values) => format!("{} items", values.len()),
        Value::Object(values) => format!("{} fields", values.len()),
    }
}

fn looks_sensitive(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("authorization")
        || lower.starts_with("sk-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        source: ActivitySource,
        status: ActivityStatus,
        importance: ActivityImportance,
    ) -> ActivityEntry {
        build_entry(
            Uuid::new_v4().to_string(),
            ActivityInput::new(
                source,
                ActivityKind::Write,
                importance,
                "test.operation",
                "summary",
            ),
            status,
            chrono::Local::now(),
            Some(chrono::Local::now()),
            None,
        )
    }

    fn activity_entry(
        source: ActivitySource,
        kind: ActivityKind,
        status: ActivityStatus,
        importance: ActivityImportance,
        operation: &str,
        summary: &str,
    ) -> ActivityEntry {
        build_entry(
            Uuid::new_v4().to_string(),
            ActivityInput::new(source, kind, importance, operation, summary)
                .target("Example")
                .details(vec![ActivityDetail::new("detail", "value")])
                .request_id("request-1")
                .tool_name("alcomd3_test_tool")
                .client_name("test-client"),
            status,
            chrono::Local::now(),
            Some(chrono::Local::now()),
            None,
        )
    }

    #[test]
    fn activity_log_file_name_validation_is_strict() {
        assert!(is_activity_log_file_name("activity-2026-06-27.jsonl"));
        assert!(!is_activity_log_file_name("activity-2026-06-27.log"));
        assert!(!is_activity_log_file_name("other-2026-06-27.jsonl"));
        assert!(!is_activity_log_file_name("activity-2026-6-27.jsonl"));
    }

    #[test]
    fn activity_log_state_keeps_cache_off_the_stack() {
        assert!(std::mem::size_of::<ActivityLogState>() < 1024);
    }

    #[test]
    fn filter_hides_secondary_success_by_default_but_keeps_failures() {
        let entries = vec![
            entry(
                ActivitySource::Gui,
                ActivityStatus::Succeeded,
                ActivityImportance::Secondary,
            ),
            entry(
                ActivitySource::Gui,
                ActivityStatus::Failed,
                ActivityImportance::Secondary,
            ),
            entry(
                ActivitySource::Gui,
                ActivityStatus::Succeeded,
                ActivityImportance::Primary,
            ),
        ];
        let filtered = filter_entries(entries, ActivityEntryFilter::default());
        assert_eq!(filtered.len(), 2);
        assert!(
            filtered
                .iter()
                .any(|entry| entry.status == ActivityStatus::Failed)
        );
    }

    #[test]
    fn filter_coalesces_started_and_finished_by_id() {
        let id = Uuid::new_v4().to_string();
        let started = build_entry(
            id.clone(),
            ActivityInput::new(
                ActivitySource::Mcp,
                ActivityKind::Read,
                ActivityImportance::Primary,
                "mcp.list",
                "started",
            ),
            ActivityStatus::Started,
            chrono::Local::now(),
            None,
            None,
        );
        let finished = ActivityEntry {
            status: ActivityStatus::Succeeded,
            summary: "finished".to_string(),
            finished_at: Some(format_time(chrono::Local::now())),
            ..started.clone()
        };
        let filtered = filter_entries(vec![started, finished], ActivityEntryFilter::default());
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].status, ActivityStatus::Succeeded);
        assert_eq!(filtered[0].summary, "finished");
    }

    #[test]
    fn jsonl_loader_skips_invalid_lines() {
        let temp = tempfile::tempdir().unwrap();
        let valid = entry(
            ActivitySource::Gui,
            ActivityStatus::Succeeded,
            ActivityImportance::Primary,
        );
        let path = temp.path().join("activity-2026-06-27.jsonl");
        std::fs::write(
            path,
            format!("{{bad json}}\n{}\n", serde_json::to_string(&valid).unwrap()),
        )
        .unwrap();
        let entries = load_recent_activity_entries(temp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, valid.id);
    }

    #[test]
    fn jsonl_loader_reads_large_recent_files_from_tail() {
        let temp = tempfile::tempdir().unwrap();
        let old = activity_entry(
            ActivitySource::Gui,
            ActivityKind::Write,
            ActivityStatus::Succeeded,
            ActivityImportance::Primary,
            "project.old",
            "old",
        );
        let recent = activity_entry(
            ActivitySource::Mcp,
            ActivityKind::Read,
            ActivityStatus::Succeeded,
            ActivityImportance::Secondary,
            "mcp.recent",
            "recent",
        );
        let old_line = format!("{}\n", serde_json::to_string(&old).unwrap());
        let recent_line = format!("{}\n", serde_json::to_string(&recent).unwrap());
        let filler_len =
            MCP_ACTIVITY_LOG_RECENT_FILE_MAX_BYTES as usize - 2 - 1 - recent_line.len();
        let mut content = Vec::new();
        content.extend_from_slice(old_line.as_bytes());
        content.extend_from_slice("项".as_bytes());
        content.extend_from_slice("x".repeat(filler_len).as_bytes());
        content.push(b'\n');
        content.extend_from_slice(recent_line.as_bytes());
        std::fs::write(temp.path().join("activity-2026-06-27.jsonl"), content).unwrap();

        let entries = load_recent_activity_entries(temp.path());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, recent.id);
    }

    #[test]
    fn mcp_activity_search_filters_and_paginates_file_entries() {
        let temp = tempfile::tempdir().unwrap();
        let matching = activity_entry(
            ActivitySource::Mcp,
            ActivityKind::Read,
            ActivityStatus::Succeeded,
            ActivityImportance::Secondary,
            "mcp.alcomd3_search_activity_logs",
            "MCP completed alcomd3_search_activity_logs",
        );
        let hidden = activity_entry(
            ActivitySource::Gui,
            ActivityKind::Write,
            ActivityStatus::Succeeded,
            ActivityImportance::Primary,
            "project.backup",
            "Project backup completed",
        );
        append_entry(temp.path(), &hidden).unwrap();
        append_entry(temp.path(), &matching).unwrap();
        let state = ActivityLogState::new_with_folder(temp.path().to_path_buf());

        let response = state
            .search_entries(ActivityLogSearchParams {
                sources: Some(vec![ActivitySource::Mcp]),
                visibility: ActivityLogVisibility::All,
                limit: Some(1),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(response.total_count, 1);
        assert_eq!(response.returned_count, 1);
        assert_eq!(response.entries[0].id, matching.id);
        assert_eq!(response.entries[0].detail_count, 1);
    }

    #[test]
    fn mcp_activity_search_clamps_zero_limit_to_one() {
        let temp = tempfile::tempdir().unwrap();
        let entry = activity_entry(
            ActivitySource::Mcp,
            ActivityKind::Read,
            ActivityStatus::Succeeded,
            ActivityImportance::Secondary,
            "mcp.alcomd3_search_activity_logs",
            "MCP completed alcomd3_search_activity_logs",
        );
        append_entry(temp.path(), &entry).unwrap();
        let state = ActivityLogState::new_with_folder(temp.path().to_path_buf());

        let response = state
            .search_entries(ActivityLogSearchParams {
                visibility: ActivityLogVisibility::All,
                limit: Some(0),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(response.limit, 1);
        assert_eq!(response.returned_count, 1);
        assert_eq!(response.next_offset, None);
    }

    #[test]
    fn mcp_activity_summary_paginates_groups() {
        let temp = tempfile::tempdir().unwrap();
        for operation in ["project.alpha", "project.beta", "project.gamma"] {
            let entry = activity_entry(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityStatus::Succeeded,
                ActivityImportance::Primary,
                operation,
                "finished",
            );
            append_entry(temp.path(), &entry).unwrap();
        }
        let state = ActivityLogState::new_with_folder(temp.path().to_path_buf());

        let response = state
            .summarize_entries(ActivityLogSummaryParams {
                group_by: ActivityLogGroupBy::Operation,
                filter: ActivityLogSearchParams {
                    visibility: ActivityLogVisibility::All,
                    offset: Some(1),
                    limit: Some(1),
                    ..Default::default()
                },
            })
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
    fn mcp_activity_summary_clamps_zero_limit_to_one() {
        let temp = tempfile::tempdir().unwrap();
        let entry = activity_entry(
            ActivitySource::Gui,
            ActivityKind::Write,
            ActivityStatus::Succeeded,
            ActivityImportance::Primary,
            "project.alpha",
            "finished",
        );
        append_entry(temp.path(), &entry).unwrap();
        let state = ActivityLogState::new_with_folder(temp.path().to_path_buf());

        let response = state
            .summarize_entries(ActivityLogSummaryParams {
                group_by: ActivityLogGroupBy::Operation,
                filter: ActivityLogSearchParams {
                    visibility: ActivityLogVisibility::All,
                    limit: Some(0),
                    ..Default::default()
                },
            })
            .unwrap();

        assert_eq!(response.limit, 1);
        assert_eq!(response.returned_count, 1);
        assert_eq!(response.next_offset, None);
    }

    #[test]
    fn mcp_activity_search_reads_entries_outside_memory_cache() {
        let temp = tempfile::tempdir().unwrap();
        let oldest = activity_entry(
            ActivitySource::Mcp,
            ActivityKind::Read,
            ActivityStatus::Succeeded,
            ActivityImportance::Primary,
            "mcp.oldest",
            "oldest",
        );
        append_entry(temp.path(), &oldest).unwrap();
        for index in 0..MAX_ACTIVITY_LOG_ENTRIES {
            let entry = activity_entry(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityStatus::Succeeded,
                ActivityImportance::Primary,
                &format!("project.generated_{index}"),
                "generated",
            );
            append_entry(temp.path(), &entry).unwrap();
        }
        let state = ActivityLogState::new_with_folder(temp.path().to_path_buf());

        assert!(
            state
                .get_entries(ActivityEntryFilter {
                    search: Some("oldest".to_string()),
                    include_secondary: true,
                    include_technical: true,
                    limit: Some(2000),
                    ..Default::default()
                })
                .is_empty()
        );
        let response = state
            .search_entries(ActivityLogSearchParams {
                search: Some("oldest".to_string()),
                order: ActivityLogOrder::Oldest,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(response.total_count, 1);
        assert_eq!(response.entries[0].id, oldest.id);
    }

    #[test]
    fn mcp_activity_get_entry_can_omit_details() {
        let temp = tempfile::tempdir().unwrap();
        let entry = activity_entry(
            ActivitySource::Gui,
            ActivityKind::Write,
            ActivityStatus::Succeeded,
            ActivityImportance::Primary,
            "project.backup",
            "Project backup completed",
        );
        append_entry(temp.path(), &entry).unwrap();
        let state = ActivityLogState::new_with_folder(temp.path().to_path_buf());

        let without_details = state
            .get_entry(ActivityLogEntryParams {
                id: entry.id.clone(),
                include_details: false,
            })
            .unwrap();
        let with_details = state
            .get_entry(ActivityLogEntryParams {
                id: entry.id,
                include_details: true,
            })
            .unwrap();

        assert!(without_details.details.is_empty());
        assert_eq!(with_details.details.len(), 1);
    }

    #[test]
    fn mcp_activity_search_rejects_invalid_time() {
        let temp = tempfile::tempdir().unwrap();
        let state = ActivityLogState::new_with_folder(temp.path().to_path_buf());
        let error = state
            .search_entries(ActivityLogSearchParams {
                since: Some("not-a-time".to_string()),
                ..Default::default()
            })
            .unwrap_err();

        assert_eq!(error.code(), "invalid_params");
    }

    #[test]
    fn url_summary_removes_query_and_fragment() {
        assert_eq!(
            summarize_url("https://example.com/path?token=secret#frag"),
            "https://example.com/path"
        );
    }

    #[test]
    fn url_summary_removes_userinfo_credentials() {
        assert_eq!(
            summarize_url("https://user:pass@example.com/path?token=secret#frag"),
            "https://example.com/path"
        );
    }

    #[test]
    fn activity_error_sanitizes_url_credentials_query_and_tokens() {
        let temp = tempfile::tempdir().unwrap();
        let state = ActivityLogState::new_with_folder(temp.path().to_path_buf());
        let entry = state.record_failed(
            None,
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityImportance::Primary,
                "repository.add",
                "Repository add failed",
            ),
            "failed https://user:pass@example.com/index.json?token=secret Authorization: Bearer abcdefghijklmnopqrstuvwxyz",
        );
        let error = entry.error.unwrap();

        assert!(error.contains("https://example.com/index.json"));
        assert!(!error.contains("user:pass"));
        assert!(!error.contains("token=secret"));
        assert!(!error.contains("abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn path_summary_preserves_absolute_paths_outside_home_for_diagnostics() {
        let outside_home = if cfg!(windows) {
            PathBuf::from(r"D:\项目\中文世界")
        } else {
            PathBuf::from("/srv/项目/中文世界")
        };
        let expected = outside_home.display().to_string();

        assert_eq!(summarize_path(outside_home), expected);
    }

    #[test]
    fn tracker_finishes_only_once() {
        let temp = tempfile::tempdir().unwrap();
        let state = ActivityLogState::new_with_folder(temp.path().to_path_buf());
        let tracker = state.start_activity(
            None,
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityImportance::Primary,
                "x",
                "started",
            ),
        );
        assert!(
            state
                .finish_success(None, &tracker, "done", Vec::new())
                .is_some()
        );
        assert!(
            state
                .finish_failed(None, &tracker, "failed", Vec::new(), "err")
                .is_none()
        );
        let entries = state.get_entries(ActivityEntryFilter::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, ActivityStatus::Succeeded);
    }

    #[test]
    fn tracker_can_finish_as_info() {
        let temp = tempfile::tempdir().unwrap();
        let state = ActivityLogState::new_with_folder(temp.path().to_path_buf());
        let tracker = state.start_activity(
            None,
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityImportance::Primary,
                "x",
                "started",
            ),
        );

        state.finish_info(None, &tracker, "no changes", Vec::new());

        let entries = state.get_entries(ActivityEntryFilter::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, ActivityStatus::Info);
        assert_eq!(entries[0].summary, "no changes");
        assert!(entries[0].finished_at.is_some());
    }
}
