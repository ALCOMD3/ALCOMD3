use alcomd3_mcp_protocol::{
    ClientIdentity, EndpointMetadata, IPC_IO_TIMEOUT, IPC_MAX_LINE_BYTES,
    IPC_METHOD_PROJECT_TASK_CANCEL, IPC_METHOD_PROJECT_TASK_GET, IPC_METHOD_PROJECT_TASK_LIST,
    IPC_METHOD_PROJECT_TASK_START, IPC_PROTOCOL_VERSION, IpcRequest, IpcResponse, IpcTransport,
    endpoint_file_path,
};
use anyhow::{Context, Result, bail};
use rmcp::{
    ErrorData as McpError, Json, Peer, RoleServer, ServerHandler, ServiceExt,
    handler::server::tool::IntoCallToolResult,
    handler::server::wrapper::Parameters,
    model::{
        CallToolRequestParams, CancelTaskParams, CancelTaskResult, CreateTaskResult, GetTaskParams,
        GetTaskPayloadParams, GetTaskPayloadResult, GetTaskResult, Implementation, JsonObject,
        ListTasksResult, Meta, PaginatedRequestParams, ProgressNotificationParam, ProgressToken,
        RequestParamsMeta, ServerCapabilities, ServerInfo, Task, TaskStatus, TasksCapability,
    },
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use uuid::Uuid;

const GUI_EXECUTABLE_ENV: &str = "ALCOMD3_GUI_EXECUTABLE";
#[cfg(windows)]
const GUI_EXECUTABLE_NAMES: &[&str] = &["ALCOMD3.exe"];
#[cfg(not(windows))]
const GUI_EXECUTABLE_NAMES: &[&str] = &["ALCOMD3", "alcomd3"];
const GUI_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const GUI_STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(250);
const PROJECT_TOOL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const TOOL_INVOCATION_MAX_CONCURRENT: usize = 64;
const TOOL_INVOCATION_MAX_STARTED_PER_WINDOW: usize = 600;
const TOOL_INVOCATION_RATE_WINDOW: Duration = Duration::from_secs(60);
const PROJECT_TASK_DEFAULT_POLL_INTERVAL_MS: u64 = 500;
const PROJECT_TASK_MIN_POLL_INTERVAL_MS: u64 = 100;
const TASK_RESULT_POLL_INTERVAL: Duration =
    Duration::from_millis(PROJECT_TASK_MIN_POLL_INTERVAL_MS);
const TASK_PROGRESS_META_KEY: &str = "alcomd3/projectProgress";
const TASK_RELATED_META_KEY: &str = "io.modelcontextprotocol/related-task";

type McpJsonResult = std::result::Result<Json<JsonObject>, Json<JsonObject>>;

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct ProjectDetailsArgs {
    project_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct BackupProjectArgs {
    project_path: String,
    backup_name: Option<String>,
    #[serde(default)]
    exclude_vpm_packages: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct CopyProjectArgs {
    source_project_path: String,
    new_project_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct RestoreProjectFromBackupArgs {
    backup_path: String,
    project_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct CreateProjectArgs {
    project_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    template_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unity_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct AddExistingProjectArgs {
    project_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct InstallProjectPackageArgs {
    project_path: String,
    package_name: String,
    version_selector: ProjectPackageVersionSelectorArg,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<ProjectPackageSourceArg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_conflicts: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct ProjectPackageArgs {
    project_path: String,
    package_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_conflicts: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ProjectPackageVersionSelectorArg {
    LatestGuiVisible,
    Exact { version: String },
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct ProjectPackageSourceArg {
    repository_id: Option<String>,
    repository_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTaskSnapshot {
    task_id: String,
    #[allow(dead_code)]
    kind: ProjectTaskKind,
    status: ProjectTaskStatus,
    status_message: Option<String>,
    created_at: String,
    last_updated_at: String,
    ttl: Option<u64>,
    poll_interval: Option<u64>,
    progress: Option<ProjectTaskProgress>,
    result: Option<Value>,
    error: Option<ProjectTaskError>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProjectTaskKind {
    Create,
    Backup,
    Copy,
    Restore,
    InstallPackage,
    UninstallPackage,
    ReinstallPackage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProjectTaskStatus {
    Working,
    Completed,
    Failed,
    Cancelled,
}

impl ProjectTaskStatus {
    fn is_terminal(self) -> bool {
        matches!(
            self,
            ProjectTaskStatus::Completed | ProjectTaskStatus::Failed | ProjectTaskStatus::Cancelled
        )
    }

    fn to_mcp(self) -> TaskStatus {
        match self {
            ProjectTaskStatus::Working => TaskStatus::Working,
            ProjectTaskStatus::Completed => TaskStatus::Completed,
            ProjectTaskStatus::Failed => TaskStatus::Failed,
            ProjectTaskStatus::Cancelled => TaskStatus::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTaskProgress {
    total: usize,
    proceed: usize,
    last_proceed: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTaskError {
    code: String,
    message: String,
    data: Option<Value>,
}

impl ProjectTaskSnapshot {
    fn to_task(&self) -> Task {
        let mut task = Task::new(
            self.task_id.clone(),
            self.status.to_mcp(),
            self.created_at.clone(),
            self.last_updated_at.clone(),
        );
        if let Some(message) = &self.status_message {
            task = task.with_status_message(message.clone());
        }
        if let Some(ttl) = self.ttl {
            task = task.with_ttl(ttl);
        }
        if let Some(poll_interval) = self.poll_interval {
            task = task.with_poll_interval(poll_interval);
        }
        task
    }

    fn meta(&self) -> Option<Meta> {
        let progress = self.progress.as_ref()?;
        let mut meta = Meta::new();
        meta.insert(TASK_PROGRESS_META_KEY.to_string(), json!(progress));
        Some(meta)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTaskListResponse {
    tasks: Vec<ProjectTaskSnapshot>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct PackageListArgs {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct RepositoryPackagesArgs {
    repository_id: Option<String>,
    repository_url: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct AddRepositoryArgs {
    repository_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct PackageDetailsArgs {
    package_name: String,
    version: Option<String>,
    repository_id: Option<String>,
    repository_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ActivityLogSourceArg {
    Gui,
    Mcp,
    DeepLink,
    System,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ActivityLogKindArg {
    Read,
    Write,
    Passive,
    Open,
    Maintenance,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ActivityLogStatusArg {
    Started,
    Succeeded,
    Failed,
    Cancelled,
    Info,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ActivityLogVisibilityArg {
    Important,
    Primary,
    Secondary,
    Technical,
    All,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ActivityLogOrderArg {
    Newest,
    Oldest,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ActivityLogGroupByArg {
    Source,
    Kind,
    Status,
    Operation,
    ToolName,
    ClientName,
    Day,
    Hour,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct ActivityLogSearchArgs {
    search: Option<String>,
    sources: Option<Vec<ActivityLogSourceArg>>,
    kinds: Option<Vec<ActivityLogKindArg>>,
    statuses: Option<Vec<ActivityLogStatusArg>>,
    visibility: Option<ActivityLogVisibilityArg>,
    operations: Option<Vec<String>>,
    tool_names: Option<Vec<String>>,
    request_id: Option<String>,
    target: Option<String>,
    since: Option<String>,
    until: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    order: Option<ActivityLogOrderArg>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct ActivityLogEntryArgs {
    id: String,
    include_details: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct ActivityLogSummaryArgs {
    search: Option<String>,
    sources: Option<Vec<ActivityLogSourceArg>>,
    kinds: Option<Vec<ActivityLogKindArg>>,
    statuses: Option<Vec<ActivityLogStatusArg>>,
    visibility: Option<ActivityLogVisibilityArg>,
    operations: Option<Vec<String>>,
    tool_names: Option<Vec<String>>,
    request_id: Option<String>,
    target: Option<String>,
    since: Option<String>,
    until: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    order: Option<ActivityLogOrderArg>,
    group_by: Option<ActivityLogGroupByArg>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct ActivityLogContextArgs {
    id: String,
    before: Option<usize>,
    after: Option<usize>,
    include_details: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum TechnicalLogLevelArg {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum TechnicalLogScopeArg {
    Memory,
    RecentFiles,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum TechnicalLogGroupByArg {
    Level,
    Target,
    File,
    Hour,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct TechnicalLogSearchArgs {
    search: Option<String>,
    levels: Option<Vec<TechnicalLogLevelArg>>,
    targets: Option<Vec<String>>,
    scope: Option<TechnicalLogScopeArg>,
    since: Option<String>,
    until: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    max_message_chars: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct TechnicalLogEntryArgs {
    id: String,
    max_message_chars: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct TechnicalLogSummaryArgs {
    search: Option<String>,
    levels: Option<Vec<TechnicalLogLevelArg>>,
    targets: Option<Vec<String>>,
    scope: Option<TechnicalLogScopeArg>,
    since: Option<String>,
    until: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    max_message_chars: Option<usize>,
    group_by: Option<TechnicalLogGroupByArg>,
}

#[derive(Debug)]
enum InvokeOutcome {
    Success(Value),
    ToolError(Value),
}

#[derive(Debug, Clone, Copy)]
struct ToolInvocationLimits {
    max_concurrent: usize,
    max_started_per_window: usize,
    window: Duration,
}

impl ToolInvocationLimits {
    fn production() -> Self {
        Self {
            max_concurrent: TOOL_INVOCATION_MAX_CONCURRENT,
            max_started_per_window: TOOL_INVOCATION_MAX_STARTED_PER_WINDOW,
            window: TOOL_INVOCATION_RATE_WINDOW,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolRateLimitReason {
    TooManyConcurrent,
    TooManyStartedInWindow,
}

impl ToolRateLimitReason {
    fn message(self) -> &'static str {
        match self {
            ToolRateLimitReason::TooManyConcurrent => {
                "Too many ALCOMD3 MCP tool calls are already running"
            }
            ToolRateLimitReason::TooManyStartedInWindow => {
                "ALCOMD3 MCP tool call rate limit exceeded"
            }
        }
    }
}

#[derive(Clone)]
struct ToolInvocationLimiter {
    inner: Arc<ToolInvocationLimiterInner>,
}

struct ToolInvocationLimiterInner {
    limits: ToolInvocationLimits,
    state: Mutex<ToolInvocationLimiterState>,
}

#[derive(Default)]
struct ToolInvocationLimiterState {
    active: usize,
    started_at: VecDeque<Instant>,
}

struct ToolInvocationPermit {
    inner: Arc<ToolInvocationLimiterInner>,
}

impl ToolInvocationLimiter {
    fn new(limits: ToolInvocationLimits) -> Self {
        assert!(limits.max_concurrent > 0);
        assert!(limits.max_started_per_window > 0);
        assert!(!limits.window.is_zero());

        Self {
            inner: Arc::new(ToolInvocationLimiterInner {
                limits,
                state: Mutex::new(ToolInvocationLimiterState::default()),
            }),
        }
    }

    fn try_start(
        &self,
        now: Instant,
    ) -> std::result::Result<ToolInvocationPermit, ToolRateLimitReason> {
        let mut state = self.inner.state.lock().unwrap();
        prune_started_at(&mut state.started_at, now, self.inner.limits.window);

        if state.active >= self.inner.limits.max_concurrent {
            return Err(ToolRateLimitReason::TooManyConcurrent);
        }
        if state.started_at.len() >= self.inner.limits.max_started_per_window {
            return Err(ToolRateLimitReason::TooManyStartedInWindow);
        }

        state.active += 1;
        state.started_at.push_back(now);

        Ok(ToolInvocationPermit {
            inner: Arc::clone(&self.inner),
        })
    }
}

impl Drop for ToolInvocationPermit {
    fn drop(&mut self) {
        let mut state = self.inner.state.lock().unwrap();
        state.active = state.active.saturating_sub(1);
    }
}

fn prune_started_at(started_at: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while started_at
        .front()
        .and_then(|started| now.checked_duration_since(*started))
        .is_some_and(|elapsed| elapsed >= window)
    {
        started_at.pop_front();
    }
}

#[derive(Clone)]
struct Alcomd3Mcp {
    client: Arc<Mutex<ClientIdentity>>,
    limiter: ToolInvocationLimiter,
}

impl Alcomd3Mcp {
    fn new() -> Self {
        Self {
            client: Arc::new(Mutex::new(ClientIdentity {
                session_id: Uuid::new_v4(),
                name: "MCP client".to_string(),
                version: None,
            })),
            limiter: ToolInvocationLimiter::new(ToolInvocationLimits::production()),
        }
    }

    fn update_client(&self, peer: &Peer<RoleServer>) {
        let Some(info) = peer.peer_info() else {
            return;
        };
        let mut client = self.client.lock().unwrap();
        client.name = info.client_info.name.clone();
        client.version = Some(info.client_info.version.clone());
    }

    fn client_for_peer(&self, peer: &Peer<RoleServer>) -> ClientIdentity {
        self.update_client(peer);
        self.client.lock().unwrap().clone()
    }

    async fn invoke<T: Serialize>(
        &self,
        method: &str,
        params: T,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        let _permit = match self.limiter.try_start(Instant::now()) {
            Ok(permit) => permit,
            Err(reason) => return format_rate_limit_result(reason),
        };
        self.update_client(&peer);
        let client = self.client.lock().unwrap().clone();
        let mut params = serde_json::to_value(params).unwrap_or(Value::Null);
        remove_null_object_fields(&mut params);
        format_invoke_result(invoke_gui(method, params, &client).await)
    }

    async fn invoke_task_ipc<T: Serialize>(
        &self,
        method: &str,
        params: T,
        peer: &Peer<RoleServer>,
    ) -> std::result::Result<Value, McpError> {
        let client = self.client_for_peer(peer);
        let mut params = serde_json::to_value(params).unwrap_or(Value::Null);
        remove_null_object_fields(&mut params);
        invoke_gui_value(method, params, &client).await
    }

    async fn fetch_project_task(
        &self,
        task_id: &str,
        peer: &Peer<RoleServer>,
    ) -> std::result::Result<ProjectTaskSnapshot, McpError> {
        let value = self
            .invoke_task_ipc(
                IPC_METHOD_PROJECT_TASK_GET,
                json!({ "taskId": task_id }),
                peer,
            )
            .await?;
        project_task_snapshot_from_value(value)
    }

    async fn invoke_project_tool_sync<T: Serialize>(
        &self,
        method: &str,
        params: T,
        context: RequestContext<RoleServer>,
    ) -> McpJsonResult {
        let _permit = match self.limiter.try_start(Instant::now()) {
            Ok(permit) => permit,
            Err(reason) => return format_rate_limit_result(reason),
        };

        let client = self.client_for_peer(&context.peer);
        let task_id = Uuid::new_v4().to_string();
        let params = serde_json::to_value(params).unwrap_or(Value::Null);
        let mut snapshot = match invoke_gui_value(
            IPC_METHOD_PROJECT_TASK_START,
            json!({
                "taskId": task_id,
                "method": method,
                "params": params,
            }),
            &client,
        )
        .await
        .and_then(project_task_snapshot_from_value)
        {
            Ok(snapshot) => snapshot,
            Err(error) => return format_mcp_error_result(error),
        };

        let progress_token = context.meta.get_progress_token();
        let mut last_progress = -1.0;
        notify_project_progress_if_needed(
            &context.peer,
            &snapshot,
            progress_token.clone(),
            &mut last_progress,
        )
        .await;

        loop {
            match snapshot.status {
                ProjectTaskStatus::Working => {
                    tokio::select! {
                        _ = context.ct.cancelled() => {
                            let cancelled = invoke_gui_value(
                                IPC_METHOD_PROJECT_TASK_CANCEL,
                                json!({ "taskId": snapshot.task_id }),
                                &client,
                            )
                            .await
                            .and_then(project_task_snapshot_from_value);
                            return match cancelled {
                                Ok(snapshot) => project_task_snapshot_to_tool_result(snapshot),
                                Err(error) => format_mcp_error_result(error),
                            };
                        }
                        _ = tokio::time::sleep(project_task_poll_interval(&snapshot)) => {}
                    }

                    snapshot = match invoke_gui_value(
                        IPC_METHOD_PROJECT_TASK_GET,
                        json!({ "taskId": snapshot.task_id }),
                        &client,
                    )
                    .await
                    .and_then(project_task_snapshot_from_value)
                    {
                        Ok(snapshot) => snapshot,
                        Err(error) => return format_mcp_error_result(error),
                    };
                    notify_project_progress_if_needed(
                        &context.peer,
                        &snapshot,
                        progress_token.clone(),
                        &mut last_progress,
                    )
                    .await;
                }
                ProjectTaskStatus::Completed
                | ProjectTaskStatus::Failed
                | ProjectTaskStatus::Cancelled => {
                    return project_task_snapshot_to_tool_result(snapshot);
                }
            }
        }
    }
}

fn remove_null_object_fields(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|_, value| {
                remove_null_object_fields(value);
                !value.is_null()
            });
        }
        Value::Array(values) => {
            for value in values {
                remove_null_object_fields(value);
            }
        }
        _ => {}
    }
}

fn format_rate_limit_result(reason: ToolRateLimitReason) -> McpJsonResult {
    Err(Json(value_as_object(json!({
        "ok": false,
        "error": {
            "code": "rate_limited",
            "message": reason.message(),
        }
    }))))
}

fn rate_limit_mcp_error(reason: ToolRateLimitReason) -> McpError {
    McpError::invalid_request(
        reason.message(),
        Some(json!({
            "code": "rate_limited",
        })),
    )
}

fn format_mcp_error_result(error: McpError) -> McpJsonResult {
    let data = error.data;
    if let Some(value) = data.as_ref()
        && value.pointer("/ok") == Some(&Value::Bool(false))
        && value.pointer("/error").is_some()
    {
        return Err(Json(value_as_object(value.clone())));
    }

    Err(Json(value_as_object(json!({
        "ok": false,
        "error": {
            "code": "mcp_task_error",
            "message": error.message,
            "data": data,
        }
    }))))
}

fn project_task_snapshot_to_tool_result(snapshot: ProjectTaskSnapshot) -> McpJsonResult {
    match snapshot.status {
        ProjectTaskStatus::Completed => {
            let result = snapshot.result.unwrap_or_else(|| json!({ "ok": true }));
            Ok(Json(value_as_object(result)))
        }
        ProjectTaskStatus::Failed | ProjectTaskStatus::Cancelled => {
            let error = snapshot.error.unwrap_or(ProjectTaskError {
                code: "project_task_error".to_string(),
                message: "MCP project task did not complete successfully".to_string(),
                data: None,
            });
            Err(Json(value_as_object(json!({
                "ok": false,
                "error": error,
            }))))
        }
        ProjectTaskStatus::Working => Err(Json(value_as_object(json!({
            "ok": false,
            "error": {
                "code": "project_task_incomplete",
                "message": "MCP project task is still running",
            }
        })))),
    }
}

fn project_tool_method(tool_name: &str) -> std::result::Result<&'static str, McpError> {
    match tool_name {
        "alcomd3_create_project" => Ok("create_project"),
        "alcomd3_backup_project" => Ok("backup_project"),
        "alcomd3_copy_project" => Ok("copy_project"),
        "alcomd3_restore_project_from_backup" => Ok("restore_project_from_backup"),
        "alcomd3_install_project_package" => Ok("install_project_package"),
        "alcomd3_uninstall_project_package" => Ok("uninstall_project_package"),
        "alcomd3_reinstall_project_package" => Ok("reinstall_project_package"),
        _ => Err(McpError::invalid_params(
            format!("tool does not support task-based invocation: {tool_name}"),
            None,
        )),
    }
}

fn project_task_snapshot_from_value(
    value: Value,
) -> std::result::Result<ProjectTaskSnapshot, McpError> {
    serde_json::from_value(value)
        .map_err(|e| McpError::internal_error(format!("invalid task response: {e}"), None))
}

fn project_task_poll_interval(snapshot: &ProjectTaskSnapshot) -> Duration {
    Duration::from_millis(
        snapshot
            .poll_interval
            .unwrap_or(PROJECT_TASK_DEFAULT_POLL_INTERVAL_MS)
            .max(PROJECT_TASK_MIN_POLL_INTERVAL_MS),
    )
}

fn project_task_payload_result(
    snapshot: ProjectTaskSnapshot,
) -> std::result::Result<GetTaskPayloadResult, McpError> {
    let mut call_tool_result = match snapshot.status {
        ProjectTaskStatus::Completed => {
            let result = snapshot.result.ok_or_else(|| {
                McpError::internal_error(
                    format!("task completed without a result: {}", snapshot.task_id),
                    None,
                )
            })?;
            Json(value_as_object(result)).into_call_tool_result()?
        }
        ProjectTaskStatus::Failed | ProjectTaskStatus::Cancelled => {
            let error = snapshot.error.unwrap_or(ProjectTaskError {
                code: "project_task_error".to_string(),
                message: "MCP project task did not complete successfully".to_string(),
                data: None,
            });
            let mut result = Json(value_as_object(json!({
                "ok": false,
                "error": error,
            })))
            .into_call_tool_result()?;
            result.is_error = Some(true);
            result
        }
        ProjectTaskStatus::Working => {
            return Err(McpError::invalid_request(
                format!("project task is still running: {}", snapshot.task_id),
                None,
            ));
        }
    };
    call_tool_result = call_tool_result.with_meta(Some(related_task_meta(&snapshot.task_id)));
    let value = serde_json::to_value(call_tool_result).map_err(|e| {
        McpError::internal_error(format!("failed to serialize task result: {e}"), None)
    })?;
    Ok(GetTaskPayloadResult::new(value))
}

fn related_task_meta(task_id: &str) -> Meta {
    let mut meta = Meta::new();
    meta.insert(
        TASK_RELATED_META_KEY.to_string(),
        json!({
            "taskId": task_id,
        }),
    );
    meta
}

async fn invoke_gui_value(
    method: &str,
    params: Value,
    client: &ClientIdentity,
) -> std::result::Result<Value, McpError> {
    match invoke_gui(method, params, client).await {
        Ok(InvokeOutcome::Success(value)) => Ok(value),
        Ok(InvokeOutcome::ToolError(value)) => Err(mcp_error_from_tool_error(value)),
        Err(error) => Err(McpError::internal_error(
            format!("ALCOMD3 is not running or the MCP IPC endpoint is unavailable: {error}"),
            None,
        )),
    }
}

fn mcp_error_from_tool_error(value: Value) -> McpError {
    let message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("ALCOMD3 returned a tool error")
        .to_string();
    let code = value.pointer("/error/code").and_then(Value::as_str);
    match code {
        Some("invalid_params" | "project_task_not_found" | "project_task_already_finished") => {
            McpError::invalid_params(message, Some(value))
        }
        Some("unknown_method" | "unsupported_project_task_method") => {
            McpError::invalid_request(message, Some(value))
        }
        _ => McpError::invalid_request(message, Some(value)),
    }
}

fn spawn_project_progress_poller(
    task_id: String,
    progress_token: ProgressToken,
    peer: Peer<RoleServer>,
    client: ClientIdentity,
) {
    tokio::spawn(async move {
        let mut last_progress = -1.0;
        loop {
            let value = invoke_gui_value(
                IPC_METHOD_PROJECT_TASK_GET,
                json!({ "taskId": task_id }),
                &client,
            )
            .await;
            let snapshot = match value.and_then(project_task_snapshot_from_value) {
                Ok(snapshot) => snapshot,
                Err(_) => break,
            };

            if let Some(notification) =
                project_progress_notification(&snapshot, progress_token.clone(), &mut last_progress)
            {
                if peer.notify_progress(notification).await.is_err() {
                    break;
                }
            }

            if snapshot.status.is_terminal() {
                break;
            }

            tokio::time::sleep(project_task_poll_interval(&snapshot)).await;
        }
    });
}

async fn notify_project_progress_if_needed(
    peer: &Peer<RoleServer>,
    snapshot: &ProjectTaskSnapshot,
    progress_token: Option<ProgressToken>,
    last_progress: &mut f64,
) {
    let Some(progress_token) = progress_token else {
        return;
    };
    let Some(notification) = project_progress_notification(snapshot, progress_token, last_progress)
    else {
        return;
    };

    let _ = peer.notify_progress(notification).await;
}

fn project_progress_notification(
    snapshot: &ProjectTaskSnapshot,
    progress_token: ProgressToken,
    last_progress: &mut f64,
) -> Option<ProgressNotificationParam> {
    let progress = snapshot.progress.as_ref()?;
    let current = progress.proceed as f64;
    if current <= *last_progress {
        return None;
    }

    *last_progress = current;
    let mut notification = ProgressNotificationParam::new(progress_token, current);
    if progress.total > 0 {
        notification = notification.with_total(progress.total as f64);
    }
    if let Some(message) = &snapshot.status_message {
        notification = notification.with_message(message.clone());
    }

    Some(notification)
}

#[tool_router]
impl Alcomd3Mcp {
    #[tool(
        description = "List Unity projects registered in ALCOMD3",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_list_projects(&self, peer: Peer<RoleServer>) -> McpJsonResult {
        self.invoke("list_projects", json!({}), peer).await
    }

    #[tool(
        description = "Get details for a project registered in ALCOMD3. project_path must match an ALCOMD3 registered project path.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_get_project_details(
        &self,
        Parameters(_args): Parameters<ProjectDetailsArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("get_project_details", _args, peer).await
    }

    #[tool(
        description = "List VPM repositories available in ALCOMD3, including default and user repositories.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_list_repositories(&self, peer: Peer<RoleServer>) -> McpJsonResult {
        self.invoke("list_repositories", json!({}), peer).await
    }

    #[tool(
        description = "Add a VPM repository URL to ALCOMD3 and refresh package cache visibility. repository_url must be a valid repository URL. headers can provide optional HTTP headers for the repository request.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn alcomd3_add_repository(
        &self,
        Parameters(args): Parameters<AddRepositoryArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("add_repository", args, peer).await
    }

    #[tool(
        description = "Get detailed package metadata for GUI-visible ALCOMD3 packages selected by package_name, optional version, and optional repository_id or repository_url.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_get_package_details(
        &self,
        Parameters(args): Parameters<PackageDetailsArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("get_package_details", args, peer).await
    }

    #[tool(
        description = "List lightweight package summaries visible in the ALCOMD3 GUI package list. Use alcomd3_get_package_details for dependencies, description, keywords, and URLs.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_list_packages(
        &self,
        Parameters(args): Parameters<PackageListArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("list_packages", args, peer).await
    }

    #[tool(
        description = "List lightweight package summaries from one ALCOMD3 remote repository selected by repository_id or repository_url. Use alcomd3_list_repositories to discover repository identifiers.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_list_repository_packages(
        &self,
        Parameters(args): Parameters<RepositoryPackagesArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("list_repository_packages", args, peer).await
    }

    #[tool(
        description = "Get ALCOMD3 environment settings including registered Unity installations, default Unity launch arguments, and default project and backup paths.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_get_environment_settings(&self, peer: Peer<RoleServer>) -> McpJsonResult {
        self.invoke("get_environment_settings", json!({}), peer)
            .await
    }

    #[tool(
        description = "Search ALCOMD3 user-readable activity logs with bounded filters and pagination. Use this before alcomd3_get_activity_log_entry; do not raise limit to pull all logs.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_search_activity_logs(
        &self,
        Parameters(args): Parameters<ActivityLogSearchArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("search_activity_logs", args, peer).await
    }

    #[tool(
        description = "Get one ALCOMD3 activity log entry by id. Obtain ids from alcomd3_search_activity_logs or alcomd3_summarize_activity_logs.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_get_activity_log_entry(
        &self,
        Parameters(args): Parameters<ActivityLogEntryArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("get_activity_log_entry", args, peer).await
    }

    #[tool(
        description = "Summarize ALCOMD3 activity logs by source, kind, status, operation, tool name, client name, day, or hour. Use this to decide which activity log details to inspect.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_summarize_activity_logs(
        &self,
        Parameters(args): Parameters<ActivityLogSummaryArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("summarize_activity_logs", args, peer).await
    }

    #[tool(
        description = "Get nearby ALCOMD3 activity log entries around one activity id to reconstruct an operation chain without reading all logs.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_get_activity_log_context(
        &self,
        Parameters(args): Parameters<ActivityLogContextArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("get_activity_log_context", args, peer).await
    }

    #[tool(
        description = "Search ALCOMD3 technical logs with bounded filters and previews. Defaults to Error/Warn memory logs; use alcomd3_get_technical_log_entry for a selected id.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_search_technical_logs(
        &self,
        Parameters(args): Parameters<TechnicalLogSearchArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("search_technical_logs", args, peer).await
    }

    #[tool(
        description = "Get one ALCOMD3 technical log entry by id with message redaction and truncation. Obtain ids from alcomd3_search_technical_logs.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_get_technical_log_entry(
        &self,
        Parameters(args): Parameters<TechnicalLogEntryArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("get_technical_log_entry", args, peer).await
    }

    #[tool(
        description = "Summarize ALCOMD3 technical logs by level, target, file, or hour. Use this before inspecting individual technical log entries.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn alcomd3_summarize_technical_logs(
        &self,
        Parameters(args): Parameters<TechnicalLogSummaryArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("summarize_technical_logs", args, peer).await
    }

    #[tool(
        description = "Create a new Unity project, register it in ALCOMD3, and resolve project packages. project_name is required. base_path defaults to the ALCOMD3 default project path. template_id and unity_version default to the current GUI template selection rules when omitted.",
        execution(task_support = "optional"),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn alcomd3_create_project(
        &self,
        Parameters(args): Parameters<CreateProjectArgs>,
        context: RequestContext<RoleServer>,
    ) -> McpJsonResult {
        self.invoke_project_tool_sync("create_project", args, context)
            .await
    }

    #[tool(
        description = "Add an existing Unity project folder to ALCOMD3. project_path must be an absolute path to a valid Unity project directory.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn alcomd3_add_existing_project(
        &self,
        Parameters(args): Parameters<AddExistingProjectArgs>,
        peer: Peer<RoleServer>,
    ) -> McpJsonResult {
        self.invoke("add_existing_project", args, peer).await
    }

    #[tool(
        description = "Create a zip backup archive for a Unity project registered in ALCOMD3. project_path must match an ALCOMD3 registered project path. backup_name optionally overrides the generated archive name without the .zip extension. exclude_vpm_packages omits installed VPM package contents when true and defaults to false.",
        execution(task_support = "optional"),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn alcomd3_backup_project(
        &self,
        Parameters(args): Parameters<BackupProjectArgs>,
        context: RequestContext<RoleServer>,
    ) -> McpJsonResult {
        self.invoke_project_tool_sync("backup_project", args, context)
            .await
    }

    #[tool(
        description = "Copy a Unity project registered in ALCOMD3 to a new project directory and register the copied project. source_project_path must match an ALCOMD3 registered project path, and new_project_path must not already exist.",
        execution(task_support = "optional"),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn alcomd3_copy_project(
        &self,
        Parameters(args): Parameters<CopyProjectArgs>,
        context: RequestContext<RoleServer>,
    ) -> McpJsonResult {
        self.invoke_project_tool_sync("copy_project", args, context)
            .await
    }

    #[tool(
        description = "Restore a Unity project from an ALCOMD3 zip backup archive into the configured default project directory and register the restored project. project_name optionally overrides the restored folder name.",
        execution(task_support = "optional"),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn alcomd3_restore_project_from_backup(
        &self,
        Parameters(args): Parameters<RestoreProjectFromBackupArgs>,
        context: RequestContext<RoleServer>,
    ) -> McpJsonResult {
        self.invoke_project_tool_sync("restore_project_from_backup", args, context)
            .await
    }

    #[tool(
        description = "Install one GUI-visible VPM package into a Unity project registered in ALCOMD3. project_path must match a registered project path. version_selector is required: use {\"type\":\"latest_gui_visible\"} to install the same latest compatible version the GUI exposes, or {\"type\":\"exact\",\"version\":\"x.y.z\"}. Optional source selects a remote repository by repository_id or repository_url. Conflicts or legacy file removals are blocked unless allow_conflicts is true.",
        execution(task_support = "optional"),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn alcomd3_install_project_package(
        &self,
        Parameters(args): Parameters<InstallProjectPackageArgs>,
        context: RequestContext<RoleServer>,
    ) -> McpJsonResult {
        self.invoke_project_tool_sync("install_project_package", args, context)
            .await
    }

    #[tool(
        description = "Uninstall one installed package from a Unity project registered in ALCOMD3. project_path must match a registered project path. Conflicts or legacy file removals are blocked unless allow_conflicts is true.",
        execution(task_support = "optional"),
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn alcomd3_uninstall_project_package(
        &self,
        Parameters(args): Parameters<ProjectPackageArgs>,
        context: RequestContext<RoleServer>,
    ) -> McpJsonResult {
        self.invoke_project_tool_sync("uninstall_project_package", args, context)
            .await
    }

    #[tool(
        description = "Reinstall one installed package in a Unity project registered in ALCOMD3. project_path must match a registered project path. Conflicts or legacy file removals are blocked unless allow_conflicts is true.",
        execution(task_support = "optional"),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn alcomd3_reinstall_project_package(
        &self,
        Parameters(args): Parameters<ProjectPackageArgs>,
        context: RequestContext<RoleServer>,
    ) -> McpJsonResult {
        self.invoke_project_tool_sync("reinstall_project_package", args, context)
            .await
    }
}

#[tool_handler]
impl ServerHandler for Alcomd3Mcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tasks_with(TasksCapability::server_default())
                .build(),
        )
            .with_server_info(Implementation::new(
                "alcomd3-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Use ALCOMD3 tools through the local GUI IPC endpoint. Some tools create or add projects, add repositories, create project backups, copies, restores, or package changes. Tool calls may start the ALCOMD3 GUI if it is not running.",
            )
    }

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CreateTaskResult, McpError> {
        let _permit = match self.limiter.try_start(Instant::now()) {
            Ok(permit) => permit,
            Err(reason) => return Err(rate_limit_mcp_error(reason)),
        };
        let method = project_tool_method(&request.name)?;
        let task_id = Uuid::new_v4().to_string();
        let progress_token = request.progress_token();
        let params = Value::Object(request.arguments.unwrap_or_default());

        let value = self
            .invoke_task_ipc(
                IPC_METHOD_PROJECT_TASK_START,
                json!({
                    "taskId": task_id,
                    "method": method,
                    "params": params,
                }),
                &context.peer,
            )
            .await?;
        let snapshot = project_task_snapshot_from_value(value)?;

        if let Some(progress_token) = progress_token {
            let client = self.client_for_peer(&context.peer);
            spawn_project_progress_poller(
                snapshot.task_id.clone(),
                progress_token,
                context.peer.clone(),
                client,
            );
        }

        Ok(CreateTaskResult::new(snapshot.to_task()))
    }

    async fn list_tasks(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListTasksResult, McpError> {
        let value = self
            .invoke_task_ipc(IPC_METHOD_PROJECT_TASK_LIST, json!({}), &context.peer)
            .await?;
        let response = serde_json::from_value::<ProjectTaskListResponse>(value).map_err(|e| {
            McpError::internal_error(format!("invalid task list response: {e}"), None)
        })?;
        Ok(ListTasksResult::new(
            response
                .tasks
                .iter()
                .map(ProjectTaskSnapshot::to_task)
                .collect(),
        ))
    }

    async fn get_task_info(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<GetTaskResult, McpError> {
        let snapshot = self
            .fetch_project_task(&request.task_id, &context.peer)
            .await?;
        let mut result = GetTaskResult::new(snapshot.to_task());
        result.meta = snapshot.meta();
        Ok(result)
    }

    async fn get_task_result(
        &self,
        request: GetTaskPayloadParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<GetTaskPayloadResult, McpError> {
        loop {
            let snapshot = self
                .fetch_project_task(&request.task_id, &context.peer)
                .await?;
            match snapshot.status {
                ProjectTaskStatus::Working => {
                    tokio::time::sleep(TASK_RESULT_POLL_INTERVAL).await;
                }
                ProjectTaskStatus::Completed
                | ProjectTaskStatus::Failed
                | ProjectTaskStatus::Cancelled => return project_task_payload_result(snapshot),
            }
        }
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CancelTaskResult, McpError> {
        let value = self
            .invoke_task_ipc(
                IPC_METHOD_PROJECT_TASK_CANCEL,
                json!({ "taskId": request.task_id }),
                &context.peer,
            )
            .await?;
        let snapshot = project_task_snapshot_from_value(value)?;
        let mut result = CancelTaskResult::new(snapshot.to_task());
        result.meta = snapshot.meta();
        Ok(result)
    }
}

async fn invoke_gui(method: &str, params: Value, client: &ClientIdentity) -> Result<InvokeOutcome> {
    match invoke_gui_once(method, params.clone(), client).await {
        Ok(value) => Ok(value),
        Err(first_error) if should_try_start_gui(&first_error) => {
            start_alcom_gui().with_context(|| {
                format!("starting ALCOMD3 GUI after MCP IPC became unavailable: {first_error:#}")
            })?;
            wait_for_gui_and_invoke(method, params, client, first_error).await
        }
        Err(error) => Err(error),
    }
}

async fn invoke_gui_once(
    method: &str,
    params: Value,
    client: &ClientIdentity,
) -> Result<InvokeOutcome> {
    let metadata = read_endpoint().await?;
    validate_endpoint_metadata(&metadata)?;
    if metadata.protocol_version != IPC_PROTOCOL_VERSION {
        bail!(
            "ALCOMD3 IPC protocol mismatch: bridge={}, GUI={}",
            IPC_PROTOCOL_VERSION,
            metadata.protocol_version
        );
    }

    let request_id = Uuid::new_v4();
    let request = IpcRequest {
        protocol_version: IPC_PROTOCOL_VERSION,
        token: metadata.token.clone(),
        request_id,
        client: client.clone(),
        method: method.to_string(),
        params,
    };

    let response = invoke_tcp(
        &metadata,
        &request,
        response_timeout_for_method(&request.method),
    )
    .await?;
    if response.request_id != request_id {
        bail!("ALCOMD3 returned a response for a different request");
    }

    Ok(response_to_tool_outcome(response))
}

async fn wait_for_gui_and_invoke(
    method: &str,
    params: Value,
    client: &ClientIdentity,
    first_error: anyhow::Error,
) -> Result<InvokeOutcome> {
    let started = Instant::now();
    let mut last_error = first_error;

    while started.elapsed() < GUI_STARTUP_TIMEOUT {
        tokio::time::sleep(GUI_STARTUP_POLL_INTERVAL).await;
        match invoke_gui_once(method, params.clone(), client).await {
            Ok(value) => return Ok(value),
            Err(error) if should_try_start_gui(&error) => last_error = error,
            Err(error) => {
                return Err(error)
                    .context("waiting for ALCOMD3 GUI MCP endpoint after starting GUI");
            }
        }
    }

    Err(last_error).context("waiting for ALCOMD3 GUI MCP endpoint after starting GUI")
}

async fn read_endpoint() -> Result<EndpointMetadata> {
    let path = endpoint_file_path();
    let bytes = tokio::fs::read(&path).await.with_context(|| {
        format!(
            "reading ALCOMD3 MCP endpoint metadata at {}",
            path.display()
        )
    })?;
    serde_json::from_slice(&bytes).context("parsing ALCOMD3 MCP endpoint metadata")
}

async fn invoke_tcp(
    metadata: &EndpointMetadata,
    request: &IpcRequest,
    response_timeout: Duration,
) -> Result<IpcResponse> {
    let stream = with_ipc_io_timeout(
        "connecting to ALCOMD3 MCP IPC endpoint",
        TcpStream::connect((metadata.host.as_str(), metadata.port)),
    )
    .await
    .with_context(|| {
        format!(
            "connecting to ALCOMD3 MCP IPC endpoint {}:{}",
            metadata.host, metadata.port
        )
    })?;

    write_request_and_read_response(stream, request, response_timeout).await
}

async fn write_request_and_read_response(
    stream: TcpStream,
    request: &IpcRequest,
    response_timeout: Duration,
) -> Result<IpcResponse> {
    let (read_half, mut write_half) = tokio::io::split(stream);
    let line = serde_json::to_vec(request)?;
    with_ipc_io_timeout(
        "writing ALCOMD3 MCP IPC request",
        write_half.write_all(&line),
    )
    .await?;
    with_ipc_io_timeout(
        "writing ALCOMD3 MCP IPC request delimiter",
        write_half.write_all(b"\n"),
    )
    .await?;
    with_ipc_io_timeout("flushing ALCOMD3 MCP IPC request", write_half.flush()).await?;

    let mut reader = BufReader::new(read_half);
    let response = with_ipc_io_timeout_for(
        "reading ALCOMD3 MCP IPC response",
        response_timeout,
        read_bounded_line(&mut reader, "ALCOMD3 MCP IPC response"),
    )
    .await?;
    serde_json::from_str(&response).context("parsing ALCOMD3 MCP IPC response")
}

fn response_timeout_for_method(method: &str) -> Duration {
    match method {
        "backup_project"
        | "copy_project"
        | "restore_project_from_backup"
        | "install_project_package"
        | "uninstall_project_package"
        | "reinstall_project_package" => PROJECT_TOOL_RESPONSE_TIMEOUT,
        _ => IPC_IO_TIMEOUT,
    }
}

fn response_to_tool_outcome(response: IpcResponse) -> InvokeOutcome {
    if response.ok {
        let value = match response.result {
            Some(Value::Object(mut object)) => {
                object.insert("ok".to_string(), Value::Bool(true));
                Value::Object(object)
            }
            Some(value) => json!({
                "ok": true,
                "result": value,
            }),
            None => json!({ "ok": true }),
        };
        InvokeOutcome::Success(value)
    } else {
        InvokeOutcome::ToolError(json!({
            "ok": false,
            "error": response.error,
        }))
    }
}

fn validate_endpoint_metadata(metadata: &EndpointMetadata) -> Result<()> {
    if metadata.transport != IpcTransport::Tcp {
        bail!(
            "ALCOMD3 MCP IPC endpoint uses unsupported transport {:?}",
            metadata.transport
        );
    }

    let host: IpAddr = metadata.host.parse().with_context(|| {
        format!(
            "ALCOMD3 MCP IPC endpoint host must be a loopback IP literal: {}",
            metadata.host
        )
    })?;
    if !host.is_loopback() {
        bail!(
            "ALCOMD3 MCP IPC endpoint host must be loopback, got {}",
            metadata.host
        );
    }

    Ok(())
}

async fn with_ipc_io_timeout<T>(
    operation: &'static str,
    future: impl Future<Output = io::Result<T>>,
) -> io::Result<T> {
    with_ipc_io_timeout_for(operation, IPC_IO_TIMEOUT, future).await
}

async fn with_ipc_io_timeout_for<T>(
    operation: &'static str,
    timeout: Duration,
    future: impl Future<Output = io::Result<T>>,
) -> io::Result<T> {
    tokio::time::timeout(timeout, future).await.map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            format!("{operation} timed out after {timeout:?}"),
        )
    })?
}

async fn read_bounded_line<R>(reader: &mut R, description: &'static str) -> io::Result<String>
where
    R: AsyncBufRead + Unpin,
{
    read_bounded_line_with_limit(reader, description, IPC_MAX_LINE_BYTES).await
}

async fn read_bounded_line_with_limit<R>(
    reader: &mut R,
    description: &'static str,
    max_line_bytes: usize,
) -> io::Result<String>
where
    R: AsyncBufRead + Unpin,
{
    let mut bytes = Vec::new();
    let mut limited = reader.take((max_line_bytes + 1) as u64);
    let read = limited.read_until(b'\n', &mut bytes).await?;
    drop(limited);

    if read == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("{description} closed without a line"),
        ));
    }
    if bytes.len() > max_line_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{description} exceeded {} bytes", max_line_bytes),
        ));
    }
    if !bytes.ends_with(b"\n") {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("{description} closed before newline delimiter"),
        ));
    }

    String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn format_invoke_result(result: Result<InvokeOutcome>) -> McpJsonResult {
    match result {
        Ok(InvokeOutcome::Success(value)) => Ok(Json(value_as_object(value))),
        Ok(InvokeOutcome::ToolError(value)) => Err(Json(value_as_object(value))),
        Err(error) => Err(Json(value_as_object(json!({
            "ok": false,
            "error": {
                "code": "alcomd3_unavailable",
                "message": format!(
                    "ALCOMD3 is not running or the MCP IPC endpoint is unavailable: {error}"
                ),
            }
        })))),
    }
}

fn value_as_object(value: Value) -> JsonObject {
    match value {
        Value::Object(object) => object,
        value => {
            let mut object = JsonObject::new();
            object.insert("result".to_string(), value);
            object
        }
    }
}

fn should_try_start_gui(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<io::Error>().is_some_and(|io_error| {
            matches!(
                io_error.kind(),
                io::ErrorKind::NotFound
                    | io::ErrorKind::ConnectionRefused
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::TimedOut
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::UnexpectedEof
            )
        })
    })
}

fn start_alcom_gui() -> Result<PathBuf> {
    let path = gui_executable_path().context("locating ALCOMD3 GUI executable")?;
    let mut child = ProcessCommand::new(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("starting ALCOMD3 GUI at {}", path.display()))?;

    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(path)
}

fn gui_executable_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(GUI_EXECUTABLE_ENV) {
        return Some(PathBuf::from(path));
    }

    let current = std::env::current_exe().ok()?;
    gui_executable_candidates(&current)
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| gui_executable_candidates(&current).into_iter().next())
}

fn gui_executable_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let directory = current_exe.parent().unwrap_or_else(|| Path::new("."));
    GUI_EXECUTABLE_NAMES
        .iter()
        .map(|name| directory.join(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::tool::IntoCallToolResult;
    use rmcp::model::{ErrorCode, TaskSupport};

    fn test_metadata(host: &str) -> EndpointMetadata {
        EndpointMetadata {
            protocol_version: IPC_PROTOCOL_VERSION,
            transport: IpcTransport::Tcp,
            host: host.to_string(),
            port: 12345,
            token: "test-token".to_string(),
            pid: 1,
        }
    }

    #[test]
    fn endpoint_metadata_accepts_loopback_hosts() {
        validate_endpoint_metadata(&test_metadata("127.0.0.1")).unwrap();
        validate_endpoint_metadata(&test_metadata("::1")).unwrap();
    }

    #[test]
    fn endpoint_metadata_rejects_non_loopback_hosts() {
        let error = validate_endpoint_metadata(&test_metadata("192.168.1.10")).unwrap_err();
        assert!(error.to_string().contains("loopback"));
    }

    #[test]
    fn endpoint_metadata_rejects_hostname_aliases() {
        let error = validate_endpoint_metadata(&test_metadata("localhost")).unwrap_err();
        assert!(error.to_string().contains("loopback IP literal"));
    }

    #[tokio::test]
    async fn bounded_line_rejects_oversized_lines() {
        let max_line_bytes = 32;
        let mut input = vec![b'a'; max_line_bytes + 1];
        input.push(b'\n');
        let mut reader = BufReader::new(input.as_slice());

        let error = read_bounded_line_with_limit(&mut reader, "test line", max_line_bytes)
            .await
            .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn format_success_result_is_mcp_success() {
        let result = format_invoke_result(Ok(InvokeOutcome::Success(json!({
            "ok": true,
            "value": 1,
        }))));

        let call_result = result.into_call_tool_result().unwrap();

        assert_eq!(call_result.is_error, Some(false));
        let structured_content = call_result.structured_content.unwrap();
        assert_eq!(structured_content["ok"], true);
        assert_eq!(structured_content["value"], 1);
    }

    #[test]
    fn gui_business_error_is_mcp_tool_error() {
        let request_id = Uuid::new_v4();
        let result = format_invoke_result(Ok(response_to_tool_outcome(IpcResponse::error(
            request_id,
            "mcp_disabled",
            "MCP is disabled",
            None,
        ))));

        let call_result = result.into_call_tool_result().unwrap();

        assert_eq!(call_result.is_error, Some(true));
        let structured_content = call_result.structured_content.unwrap();
        assert_eq!(structured_content["ok"], false);
        assert_eq!(structured_content["error"]["code"], "mcp_disabled");
    }

    #[test]
    fn bridge_unavailable_error_is_mcp_tool_error() {
        let result = format_invoke_result(Err(anyhow::anyhow!("missing endpoint")));

        let call_result = result.into_call_tool_result().unwrap();

        assert_eq!(call_result.is_error, Some(true));
        let structured_content = call_result.structured_content.unwrap();
        assert_eq!(structured_content["ok"], false);
        assert_eq!(structured_content["error"]["code"], "alcomd3_unavailable");
    }

    #[test]
    fn rate_limited_result_is_mcp_tool_error() {
        let result = format_rate_limit_result(ToolRateLimitReason::TooManyConcurrent);

        let call_result = result.into_call_tool_result().unwrap();

        assert_eq!(call_result.is_error, Some(true));
        let structured_content = call_result.structured_content.unwrap();
        assert_eq!(structured_content["ok"], false);
        assert_eq!(structured_content["error"]["code"], "rate_limited");
    }

    #[test]
    fn mcp_tool_error_result_preserves_gui_error_payload() {
        let result = format_mcp_error_result(McpError::internal_error(
            "A project backup is already running",
            Some(json!({
                "ok": false,
                "error": {
                    "code": "project_backup_already_running",
                    "message": "A project backup is already running"
                }
            })),
        ));

        let call_result = result.into_call_tool_result().unwrap();

        assert_eq!(call_result.is_error, Some(true));
        let structured_content = call_result.structured_content.unwrap();
        assert_eq!(
            structured_content["error"]["code"],
            "project_backup_already_running"
        );
    }

    #[test]
    fn remove_null_object_fields_keeps_gui_defaults_available() {
        let mut value = json!({
            "visibility": null,
            "limit": 10,
            "nested": {
                "order": null,
                "search": "mcp"
            },
            "items": [
                {
                    "scope": null,
                    "levels": ["error"]
                }
            ]
        });

        remove_null_object_fields(&mut value);

        assert!(value.get("visibility").is_none());
        assert_eq!(value["limit"], 10);
        assert!(value["nested"].get("order").is_none());
        assert_eq!(value["nested"]["search"], "mcp");
        assert!(value["items"][0].get("scope").is_none());
        assert_eq!(value["items"][0]["levels"], json!(["error"]));
    }

    #[test]
    fn task_lookup_gui_errors_map_to_invalid_params() {
        let error = mcp_error_from_tool_error(json!({
            "ok": false,
            "error": {
                "code": "project_task_not_found",
                "message": "MCP project task was not found: task-1"
            }
        }));

        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
    }

    #[test]
    fn server_advertises_task_capabilities() {
        let info = Alcomd3Mcp::new().get_info();
        let tasks = info.capabilities.tasks.unwrap();

        assert!(tasks.supports_tools_call());
        assert!(tasks.supports_list());
        assert!(tasks.supports_cancel());
    }

    #[test]
    fn project_write_tools_optionally_support_tasks() {
        let server = Alcomd3Mcp::new();
        for tool_name in [
            "alcomd3_create_project",
            "alcomd3_backup_project",
            "alcomd3_copy_project",
            "alcomd3_restore_project_from_backup",
            "alcomd3_install_project_package",
            "alcomd3_uninstall_project_package",
            "alcomd3_reinstall_project_package",
        ] {
            let tool = server.get_tool(tool_name).unwrap();
            assert_eq!(tool.task_support(), TaskSupport::Optional);
        }
    }

    #[test]
    fn project_task_snapshot_maps_to_mcp_task_and_progress_meta() {
        let snapshot = project_task_snapshot_from_value(json!({
            "taskId": "task-1",
            "kind": "backup",
            "status": "working",
            "statusMessage": "Backing up project: 1/2 Assets/a.cs",
            "createdAt": "2026-06-25T00:00:00Z",
            "lastUpdatedAt": "2026-06-25T00:00:01Z",
            "ttl": 600000,
            "pollInterval": 500,
            "progress": {
                "total": 2,
                "proceed": 1,
                "lastProceed": "Assets/a.cs"
            }
        }))
        .unwrap();

        let task = snapshot.to_task();
        assert_eq!(task.task_id, "task-1");
        assert_eq!(task.status, TaskStatus::Working);
        assert_eq!(task.ttl, Some(600000));
        assert_eq!(task.poll_interval, Some(500));

        let meta = snapshot.meta().unwrap();
        assert_eq!(meta[TASK_PROGRESS_META_KEY]["total"], 2);
        assert_eq!(meta[TASK_PROGRESS_META_KEY]["proceed"], 1);
        assert_eq!(meta[TASK_PROGRESS_META_KEY]["lastProceed"], "Assets/a.cs");
    }

    #[test]
    fn project_task_poll_interval_defaults_and_clamps_low_values() {
        let mut snapshot = project_task_snapshot_from_value(json!({
            "taskId": "task-1",
            "kind": "backup",
            "status": "working",
            "createdAt": "2026-06-25T00:00:00Z",
            "lastUpdatedAt": "2026-06-25T00:00:01Z"
        }))
        .unwrap();

        assert_eq!(
            project_task_poll_interval(&snapshot),
            Duration::from_millis(PROJECT_TASK_DEFAULT_POLL_INTERVAL_MS)
        );

        snapshot.poll_interval = Some(0);
        assert_eq!(
            project_task_poll_interval(&snapshot),
            Duration::from_millis(PROJECT_TASK_MIN_POLL_INTERVAL_MS)
        );

        snapshot.poll_interval = Some(PROJECT_TASK_MIN_POLL_INTERVAL_MS - 1);
        assert_eq!(
            project_task_poll_interval(&snapshot),
            Duration::from_millis(PROJECT_TASK_MIN_POLL_INTERVAL_MS)
        );

        snapshot.poll_interval = Some(PROJECT_TASK_DEFAULT_POLL_INTERVAL_MS + 250);
        assert_eq!(
            project_task_poll_interval(&snapshot),
            Duration::from_millis(PROJECT_TASK_DEFAULT_POLL_INTERVAL_MS + 250)
        );
    }

    #[test]
    fn project_task_result_payload_includes_related_task_meta() {
        let snapshot = project_task_snapshot_from_value(json!({
            "taskId": "task-1",
            "kind": "backup",
            "status": "completed",
            "createdAt": "2026-06-25T00:00:00Z",
            "lastUpdatedAt": "2026-06-25T00:00:01Z",
            "ttl": 600000,
            "pollInterval": 500,
            "result": {
                "ok": true,
                "backupPath": "C:\\\\backup.zip"
            }
        }))
        .unwrap();

        let payload = project_task_payload_result(snapshot).unwrap().0;

        assert_eq!(payload["isError"], false);
        assert_eq!(payload["structuredContent"]["ok"], true);
        assert_eq!(payload["_meta"][TASK_RELATED_META_KEY]["taskId"], "task-1");
    }

    #[test]
    fn failed_project_task_result_payload_is_tool_error() {
        let snapshot = project_task_snapshot_from_value(json!({
            "taskId": "task-2",
            "kind": "copy",
            "status": "failed",
            "createdAt": "2026-06-25T00:00:00Z",
            "lastUpdatedAt": "2026-06-25T00:00:01Z",
            "ttl": 600000,
            "pollInterval": 500,
            "error": {
                "code": "project_copy_error",
                "message": "copy failed"
            }
        }))
        .unwrap();

        let payload = project_task_payload_result(snapshot).unwrap().0;

        assert_eq!(payload["isError"], true);
        assert_eq!(
            payload["structuredContent"]["error"]["code"],
            "project_copy_error"
        );
        assert_eq!(payload["_meta"][TASK_RELATED_META_KEY]["taskId"], "task-2");
    }

    #[test]
    fn project_write_methods_use_long_response_timeout() {
        assert_eq!(
            response_timeout_for_method("backup_project"),
            PROJECT_TOOL_RESPONSE_TIMEOUT
        );
        assert_eq!(
            response_timeout_for_method("copy_project"),
            PROJECT_TOOL_RESPONSE_TIMEOUT
        );
        assert_eq!(
            response_timeout_for_method("restore_project_from_backup"),
            PROJECT_TOOL_RESPONSE_TIMEOUT
        );
        assert_eq!(
            response_timeout_for_method("install_project_package"),
            PROJECT_TOOL_RESPONSE_TIMEOUT
        );
        assert_eq!(
            response_timeout_for_method("uninstall_project_package"),
            PROJECT_TOOL_RESPONSE_TIMEOUT
        );
        assert_eq!(
            response_timeout_for_method("reinstall_project_package"),
            PROJECT_TOOL_RESPONSE_TIMEOUT
        );
        assert_eq!(response_timeout_for_method("list_projects"), IPC_IO_TIMEOUT);
    }

    #[test]
    fn gui_executable_candidates_match_current_alcomd3_binary_names() {
        let candidates = gui_executable_candidates(Path::new("/install/alcomd3-mcp"));
        let file_names = candidates
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(file_names, GUI_EXECUTABLE_NAMES);
    }

    #[test]
    fn tool_invocation_limiter_rejects_calls_after_window_capacity() {
        let limiter = ToolInvocationLimiter::new(ToolInvocationLimits {
            max_concurrent: 8,
            max_started_per_window: 2,
            window: Duration::from_secs(60),
        });
        let now = Instant::now();

        drop(limiter.try_start(now).unwrap());
        drop(limiter.try_start(now + Duration::from_secs(1)).unwrap());

        let limited = limiter.try_start(now + Duration::from_secs(2));
        assert!(matches!(
            limited,
            Err(ToolRateLimitReason::TooManyStartedInWindow)
        ));
        assert!(limiter.try_start(now + Duration::from_secs(61)).is_ok());
    }

    #[test]
    fn tool_invocation_limiter_rejects_excess_concurrent_calls() {
        let limiter = ToolInvocationLimiter::new(ToolInvocationLimits {
            max_concurrent: 2,
            max_started_per_window: 16,
            window: Duration::from_secs(60),
        });
        let now = Instant::now();

        let first = limiter.try_start(now).unwrap();
        let _second = limiter.try_start(now).unwrap();

        let limited = limiter.try_start(now);
        assert!(matches!(
            limited,
            Err(ToolRateLimitReason::TooManyConcurrent)
        ));

        drop(first);
        assert!(limiter.try_start(now).is_ok());
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let service = Alcomd3Mcp::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
