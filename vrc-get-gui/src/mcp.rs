use crate::activity_log::{
    ActivityDetail, ActivityImportance, ActivityInput, ActivityKind, ActivityLogContextParams,
    ActivityLogEntryParams, ActivityLogSearchParams, ActivityLogState, ActivityLogSummaryParams,
    ActivitySource, ActivityTracker, safe_detail_from_json, summarize_path, summarize_url,
    summarize_url_host, target_from_path,
};
use crate::backend::environment_settings;
use crate::backend::logs;
use crate::backend::mcp_capabilities::mcp_tool_capability_for_method;
use crate::backend::packages::{
    latest_package_infos_by_source, package_is_visible_with_gui_filters, package_source_kind,
    repository_id, repository_is_default, repository_kind,
};
use crate::backend::project_operations;
use crate::backend::projects::{
    ProjectDetailsSnapshot, load_project_details_snapshot, project_summary_snapshot,
};
use crate::backend::repository_operations;
use crate::commands::{
    DEFAULT_UNITY_ARGUMENTS, RustError, TauriPendingProjectChanges,
    build_project_package_row_accumulators, load_project, project_package_row_compatible_packages,
    project_package_row_incompatible_packages,
};
use crate::logging::{
    TechnicalLogEntryParams, TechnicalLogSearchParams, TechnicalLogSummaryParams,
};
use crate::state::{
    ChangesState, GuiConfigState, PackagesState, ProjectApplyState, ProjectBackupState,
    ProjectCopyState, ProjectRestoreState, SettingsState,
};
use alcomd3_mcp_protocol::{
    ClientIdentity, EndpointMetadata, IPC_IO_TIMEOUT, IPC_MAX_LINE_BYTES,
    IPC_METHOD_PROJECT_TASK_CANCEL, IPC_METHOD_PROJECT_TASK_GET, IPC_METHOD_PROJECT_TASK_LIST,
    IPC_METHOD_PROJECT_TASK_START, IPC_PROTOCOL_VERSION, IpcRequest, IpcResponse, IpcTransport,
    endpoint_file_path,
};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, oneshot};
use tokio::task::AbortHandle;
use uuid::Uuid;
use vrc_get_vpm::environment::{
    CURATED_REPOSITORY_ID, CURATED_URL_STR, OFFICIAL_REPOSITORY_ID, OFFICIAL_URL_STR, UserProject,
    VccDatabaseConnection,
};
use vrc_get_vpm::io::DefaultEnvironmentIo;
use vrc_get_vpm::repository::LocalCachedRepository;
use vrc_get_vpm::unity_project::AddPackageOperation;
use vrc_get_vpm::unity_project::PendingProjectChanges;
use vrc_get_vpm::version::{StrictEqVersion, Version};
use vrc_get_vpm::{
    AbortCheck, PackageInfo, PackageManifest, UserRepoSetting, is_valid_package_name,
};

pub const MCP_STATUS_CHANGED_EVENT: &str = "mcp-status-changed";
pub const MCP_TOOL_CALL_EVENT: &str = "mcp-tool-call";
const MAX_RECORDED_MCP_RECENT_CLIENTS: usize = 20;
const MCP_CLIENT_ACTIVITY_TTL_MS: u64 = 10 * 60 * 1_000;
const MCP_CLIENT_STATUS_EMIT_THROTTLE_MS: u64 = 1_000;
const MCP_PACKAGE_LIST_DEFAULT_LIMIT: usize = 200;
const MCP_PACKAGE_LIST_MAX_LIMIT: usize = 1_000;
const MCP_PROJECT_TASK_TTL_MS: u64 = 10 * 60 * 1_000;
const MCP_PROJECT_TASK_POLL_INTERVAL_MS: u64 = 500;
const MCP_DISABLED_MESSAGE: &str = "MCP is disabled in ALCOMD3 GUI";

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    enabled: bool,
    running: bool,
    protocol_version: u32,
    transport: String,
    host: Option<String>,
    port: Option<u16>,
    pid: u32,
    endpoint_file: String,
    bridge_command: String,
    recent_clients: Vec<McpRecentClientStatus>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct McpRecentClientStatus {
    session_id: String,
    name: String,
    version: Option<String>,
    last_seen_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallEvent {
    request_id: String,
    tool_name: String,
    phase: McpToolCallPhase,
}

#[derive(Debug, Clone, Serialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum McpToolCallPhase {
    Started,
    Finished,
    Failed,
}

#[derive(Debug, Clone)]
struct McpTrackedToolCall {
    request_id: Uuid,
    tool_name: String,
    activity: Option<ActivityTracker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum McpProjectTaskKind {
    Create,
    Backup,
    Copy,
    Restore,
    InstallPackage,
    UninstallPackage,
    ReinstallPackage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum McpProjectTaskStatus {
    Working,
    Completed,
    Failed,
    Cancelled,
}

impl McpProjectTaskStatus {
    fn is_terminal(self) -> bool {
        matches!(
            self,
            McpProjectTaskStatus::Completed
                | McpProjectTaskStatus::Failed
                | McpProjectTaskStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpProjectProgress {
    total: usize,
    proceed: usize,
    last_proceed: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpProjectTaskSnapshot {
    task_id: String,
    kind: McpProjectTaskKind,
    status: McpProjectTaskStatus,
    status_message: Option<String>,
    created_at: String,
    last_updated_at: String,
    ttl: Option<u64>,
    poll_interval: Option<u64>,
    progress: Option<McpProjectProgress>,
    result: Option<Value>,
    error: Option<McpIpcError>,
}

struct McpProjectTaskRecord {
    task_id: String,
    kind: McpProjectTaskKind,
    status: McpProjectTaskStatus,
    status_message: Option<String>,
    created_at: String,
    last_updated_at: String,
    ttl: Option<u64>,
    poll_interval: Option<u64>,
    progress: Option<McpProjectProgress>,
    result: Option<Value>,
    error: Option<McpIpcError>,
    cancel_handle: Option<McpProjectTaskCancelHandle>,
    cancel_requested: bool,
    tool_call: Option<McpTrackedToolCall>,
}

enum McpProjectTaskCancelHandle {
    AbortTask(AbortHandle),
    AbortProjectCreate(AbortCheck),
    AbortPackageApply(AbortCheck),
}

impl McpProjectTaskCancelHandle {
    fn cancel(&self) {
        match self {
            McpProjectTaskCancelHandle::AbortTask(abort) => abort.abort(),
            McpProjectTaskCancelHandle::AbortProjectCreate(abort) => abort.abort(),
            McpProjectTaskCancelHandle::AbortPackageApply(abort) => abort.abort(),
        }
    }
}

impl McpProjectTaskRecord {
    fn snapshot(&self) -> McpProjectTaskSnapshot {
        McpProjectTaskSnapshot {
            task_id: self.task_id.clone(),
            kind: self.kind,
            status: self.status,
            status_message: self.status_message.clone(),
            created_at: self.created_at.clone(),
            last_updated_at: self.last_updated_at.clone(),
            ttl: self.ttl,
            poll_interval: self.poll_interval,
            progress: self.progress.clone(),
            result: self.result.clone(),
            error: self.error.clone(),
        }
    }
}

#[derive(Default)]
struct McpProjectTaskStore {
    tasks: HashMap<String, McpProjectTaskRecord>,
}

impl McpProjectTaskStore {
    fn start(
        &mut self,
        task_id: String,
        kind: McpProjectTaskKind,
        tool_call: Option<McpTrackedToolCall>,
    ) -> McpProjectTaskSnapshot {
        self.prune_expired();
        let now = now_iso8601();
        let record = McpProjectTaskRecord {
            task_id: task_id.clone(),
            kind,
            status: McpProjectTaskStatus::Working,
            status_message: Some(project_task_status_message(kind, None)),
            created_at: now.clone(),
            last_updated_at: now,
            ttl: Some(MCP_PROJECT_TASK_TTL_MS),
            poll_interval: Some(MCP_PROJECT_TASK_POLL_INTERVAL_MS),
            progress: None,
            result: None,
            error: None,
            cancel_handle: None,
            cancel_requested: false,
            tool_call,
        };
        let snapshot = record.snapshot();
        self.tasks.insert(task_id, record);
        snapshot
    }

    fn set_cancel_handle(&mut self, task_id: &str, cancel_handle: McpProjectTaskCancelHandle) {
        if let Some(task) = self.tasks.get_mut(task_id)
            && task.status == McpProjectTaskStatus::Working
        {
            task.cancel_handle = Some(cancel_handle);
        }
    }

    fn list(&mut self) -> Vec<McpProjectTaskSnapshot> {
        self.prune_expired();
        self.tasks
            .values()
            .map(McpProjectTaskRecord::snapshot)
            .collect()
    }

    fn get(&mut self, task_id: &str) -> Option<McpProjectTaskSnapshot> {
        self.prune_expired();
        self.tasks.get(task_id).map(McpProjectTaskRecord::snapshot)
    }

    fn cancel(
        &mut self,
        task_id: &str,
    ) -> Result<(McpProjectTaskSnapshot, Option<McpTrackedToolCall>), McpIpcError> {
        self.prune_expired();
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Err(McpIpcError::new(
                "project_task_not_found",
                format!("MCP project task was not found: {task_id}"),
            ));
        };

        if task.status.is_terminal() {
            return Err(McpIpcError::new(
                "project_task_already_finished",
                format!("MCP project task already finished: {task_id}"),
            ));
        }

        if task.cancel_requested {
            return Ok((task.snapshot(), None));
        }

        if let Some(cancel_handle) = task.cancel_handle.take() {
            match cancel_handle {
                McpProjectTaskCancelHandle::AbortTask(abort) => abort.abort(),
                McpProjectTaskCancelHandle::AbortProjectCreate(abort)
                | McpProjectTaskCancelHandle::AbortPackageApply(abort) => {
                    abort.abort();
                    task.cancel_requested = true;
                    task.status_message = Some("Task cancellation requested".to_string());
                    task.last_updated_at = now_iso8601();
                    return Ok((task.snapshot(), None));
                }
            }
        }

        task.status = McpProjectTaskStatus::Cancelled;
        task.status_message = Some("Task canceled".to_string());
        task.last_updated_at = now_iso8601();
        task.error = Some(McpIpcError::new(
            "project_task_cancelled",
            "MCP project task was canceled",
        ));
        let tool_call = task.tool_call.take();
        Ok((task.snapshot(), tool_call))
    }

    fn update_progress(&mut self, task_id: &str, progress: McpProjectProgress) {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return;
        };
        if task.status != McpProjectTaskStatus::Working {
            return;
        }

        task.status_message = Some(if task.cancel_requested {
            "Task cancellation requested".to_string()
        } else {
            project_task_status_message(task.kind, Some(&progress))
        });
        task.progress = Some(progress);
        task.last_updated_at = now_iso8601();
    }

    fn finish_success(&mut self, task_id: &str, result: Value) -> Option<McpTrackedToolCall> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return None;
        };
        if task.status != McpProjectTaskStatus::Working {
            return None;
        }

        task.cancel_handle = None;
        task.cancel_requested = false;
        task.status = McpProjectTaskStatus::Completed;
        task.status_message = Some(project_task_completed_message(task.kind));
        task.result = Some(result);
        task.last_updated_at = now_iso8601();
        task.tool_call.take()
    }

    fn finish_error(&mut self, task_id: &str, error: McpIpcError) -> Option<McpTrackedToolCall> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return None;
        };
        if task.status != McpProjectTaskStatus::Working {
            return None;
        }

        task.cancel_handle = None;
        task.cancel_requested = false;
        task.status = McpProjectTaskStatus::Failed;
        task.status_message = Some(error.message.clone());
        task.error = Some(error);
        task.last_updated_at = now_iso8601();
        task.tool_call.take()
    }

    fn finish_cancelled(&mut self, task_id: &str) -> Option<McpTrackedToolCall> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return None;
        };
        if task.status != McpProjectTaskStatus::Working {
            return None;
        }

        task.cancel_handle = None;
        task.cancel_requested = false;
        task.status = McpProjectTaskStatus::Cancelled;
        task.status_message = Some("Task canceled".to_string());
        task.error = Some(McpIpcError::new(
            "project_task_cancelled",
            "MCP project task was canceled",
        ));
        task.last_updated_at = now_iso8601();
        task.tool_call.take()
    }

    fn cancel_requested(&self, task_id: &str) -> bool {
        self.tasks
            .get(task_id)
            .is_some_and(|task| task.cancel_requested)
    }

    fn cancel_if_working(&mut self, task_id: &str) -> Option<McpTrackedToolCall> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return None;
        };
        if task.status != McpProjectTaskStatus::Working {
            return None;
        }

        task.cancel_handle = None;
        task.cancel_requested = false;
        task.status = McpProjectTaskStatus::Cancelled;
        task.status_message = Some("Task canceled".to_string());
        task.error = Some(McpIpcError::new(
            "project_task_cancelled",
            "MCP project task was canceled",
        ));
        task.last_updated_at = now_iso8601();
        task.tool_call.take()
    }

    fn abort_all(&mut self) -> Vec<McpTrackedToolCall> {
        let mut tool_calls = Vec::new();
        for task in self.tasks.values_mut() {
            if let Some(cancel_handle) = task.cancel_handle.take() {
                cancel_handle.cancel();
            }
            if task.status == McpProjectTaskStatus::Working {
                task.status = McpProjectTaskStatus::Cancelled;
                task.status_message = Some("Task canceled".to_string());
                task.error = Some(McpIpcError::new(
                    "project_task_cancelled",
                    "MCP project task was canceled",
                ));
                task.last_updated_at = now_iso8601();
                if let Some(tool_call) = task.tool_call.take() {
                    tool_calls.push(tool_call);
                }
            }
        }
        self.tasks.clear();
        tool_calls
    }

    fn prune_expired(&mut self) {
        let now = now_unix_ms();
        self.tasks.retain(|_, task| {
            if !task.status.is_terminal() {
                return true;
            }

            let Ok(last_updated) = chrono::DateTime::parse_from_rfc3339(&task.last_updated_at)
            else {
                return true;
            };
            let last_updated_ms = last_updated.timestamp_millis();
            if last_updated_ms < 0 {
                return true;
            }

            now.saturating_sub(last_updated_ms as u64) < task.ttl.unwrap_or(MCP_PROJECT_TASK_TTL_MS)
        });
    }
}

fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn project_task_status_message(
    kind: McpProjectTaskKind,
    progress: Option<&McpProjectProgress>,
) -> String {
    let verb = match kind {
        McpProjectTaskKind::Create => "Creating project",
        McpProjectTaskKind::Backup => "Backing up project",
        McpProjectTaskKind::Copy => "Copying project",
        McpProjectTaskKind::Restore => "Restoring project",
        McpProjectTaskKind::InstallPackage => "Installing project package",
        McpProjectTaskKind::UninstallPackage => "Uninstalling project package",
        McpProjectTaskKind::ReinstallPackage => "Reinstalling project package",
    };

    let Some(progress) = progress else {
        return verb.to_string();
    };

    if progress.total == 0 {
        format!("{verb}: {}", progress.last_proceed)
    } else {
        format!(
            "{verb}: {}/{} {}",
            progress.proceed, progress.total, progress.last_proceed
        )
    }
}

fn project_task_completed_message(kind: McpProjectTaskKind) -> String {
    match kind {
        McpProjectTaskKind::Create => "Project creation completed",
        McpProjectTaskKind::Backup => "Project backup completed",
        McpProjectTaskKind::Copy => "Project copy completed",
        McpProjectTaskKind::Restore => "Project restore completed",
        McpProjectTaskKind::InstallPackage => "Project package install completed",
        McpProjectTaskKind::UninstallPackage => "Project package uninstall completed",
        McpProjectTaskKind::ReinstallPackage => "Project package reinstall completed",
    }
    .to_string()
}

pub struct McpState {
    inner: Mutex<McpStateInner>,
    project_tasks: Arc<StdMutex<McpProjectTaskStore>>,
}

struct McpStateInner {
    active: Option<ActiveMcpServer>,
    recent_clients: VecDeque<McpRecentClientStatus>,
    last_client_status_emit_unix_ms: u64,
}

struct ActiveMcpServer {
    metadata: EndpointMetadata,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl McpState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(McpStateInner {
                active: None,
                recent_clients: VecDeque::new(),
                last_client_status_emit_unix_ms: 0,
            }),
            project_tasks: Arc::new(StdMutex::new(McpProjectTaskStore::default())),
        }
    }

    pub async fn status(&self, enabled: bool) -> McpStatus {
        let inner = self.inner.lock().await;
        let metadata = inner.active.as_ref().map(|active| &active.metadata);
        let now_unix_ms = now_unix_ms();

        McpStatus {
            enabled,
            running: metadata.is_some(),
            protocol_version: IPC_PROTOCOL_VERSION,
            transport: "tcp".to_string(),
            host: metadata.map(|metadata| metadata.host.clone()),
            port: metadata.map(|metadata| metadata.port),
            pid: std::process::id(),
            endpoint_file: endpoint_file_path().display().to_string(),
            bridge_command: bridge_command(),
            recent_clients: inner
                .recent_clients
                .iter()
                .filter(|client| is_recent_client_activity(client.last_seen_unix_ms, now_unix_ms))
                .cloned()
                .collect(),
        }
    }

    pub async fn ensure_running(&self, app: AppHandle) -> io::Result<()> {
        self.start(app).await
    }

    pub async fn set_enabled(&self, app: AppHandle, enabled: bool) -> io::Result<()> {
        if let Err(e) = self.start(app.clone()).await {
            log::error!("failed to ensure MCP IPC endpoint while setting MCP access: {e}");
        }
        self.emit_status(app, enabled).await;
        Ok(())
    }

    async fn start(&self, app: AppHandle) -> io::Result<()> {
        let mut inner = self.inner.lock().await;
        if inner.active.is_some() {
            return Ok(());
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        let token = Uuid::new_v4().simple().to_string();
        let metadata = EndpointMetadata {
            protocol_version: IPC_PROTOCOL_VERSION,
            transport: IpcTransport::Tcp,
            host: "127.0.0.1".to_string(),
            port: address.port(),
            token: token.clone(),
            pid: std::process::id(),
        };

        write_endpoint_file(&endpoint_file_path(), &metadata).await?;

        let task = tauri::async_runtime::spawn(async move {
            accept_loop(app, listener, token).await;
        });

        inner.recent_clients.clear();
        inner.last_client_status_emit_unix_ms = 0;
        inner.active = Some(ActiveMcpServer { metadata, task });
        Ok(())
    }

    async fn stop(&self, app: Option<&AppHandle>) -> io::Result<()> {
        let mut inner = self.inner.lock().await;
        if let Some(active) = inner.active.take() {
            active.task.abort();
        }
        inner.recent_clients.clear();
        inner.last_client_status_emit_unix_ms = 0;
        let tool_calls = self.project_tasks.lock().unwrap().abort_all();
        if let Some(app) = app {
            for tool_call in tool_calls {
                cancel_tracked_mcp_tool_call_activity(app, &tool_call);
                emit_tracked_mcp_tool_call_event(app, &tool_call, McpToolCallPhase::Failed);
            }
        }
        remove_endpoint_file(&endpoint_file_path()).await
    }

    pub async fn shutdown(&self, app: &AppHandle) -> io::Result<()> {
        self.stop(Some(app)).await
    }

    async fn emit_status(&self, app: AppHandle, enabled: bool) {
        let status = self.status(enabled).await;
        if let Err(e) = app.emit(MCP_STATUS_CHANGED_EVENT, status) {
            log::error!("failed to emit MCP status change: {e}");
        }
    }

    async fn record_client(&self, client: &ClientIdentity) -> bool {
        let mut inner = self.inner.lock().await;
        let now_unix_ms = now_unix_ms();
        let McpStateInner {
            recent_clients,
            last_client_status_emit_unix_ms,
            ..
        } = &mut *inner;
        record_client_activity(
            recent_clients,
            client,
            now_unix_ms,
            last_client_status_emit_unix_ms,
        )
    }
}

async fn accept_loop(app: AppHandle, listener: TcpListener, token: String) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let app = app.clone();
                let token = token.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = handle_connection(app, stream, token).await {
                        log::error!("MCP IPC connection failed: {e}");
                    }
                });
            }
            Err(e) => {
                log::error!("MCP IPC listener failed: {e}");
                break;
            }
        }
    }
}

async fn handle_connection(app: AppHandle, stream: TcpStream, token: String) -> io::Result<()> {
    let mut reader = BufReader::new(stream);
    let line = with_ipc_io_timeout(
        "reading ALCOMD3 MCP IPC request",
        read_bounded_line(&mut reader, "ALCOMD3 MCP IPC request"),
    )
    .await?;

    let response = match serde_json::from_str::<IpcRequest>(&line) {
        Ok(request) => handle_request(app, request, &token).await,
        Err(e) => IpcResponse::error(
            Uuid::nil(),
            "invalid_request",
            format!("Failed to parse MCP IPC request: {e}"),
            None,
        ),
    };

    let stream = reader.get_mut();
    let bytes = serde_json::to_vec(&response).map_err(io::Error::other)?;
    with_ipc_io_timeout("writing ALCOMD3 MCP IPC response", stream.write_all(&bytes)).await?;
    with_ipc_io_timeout(
        "writing ALCOMD3 MCP IPC response delimiter",
        stream.write_all(b"\n"),
    )
    .await?;
    with_ipc_io_timeout("flushing ALCOMD3 MCP IPC response", stream.flush()).await
}

async fn with_ipc_io_timeout<T>(
    operation: &'static str,
    future: impl Future<Output = io::Result<T>>,
) -> io::Result<T> {
    tokio::time::timeout(IPC_IO_TIMEOUT, future)
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!("{operation} timed out after {:?}", IPC_IO_TIMEOUT),
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

async fn handle_request(app: AppHandle, request: IpcRequest, token: &str) -> IpcResponse {
    if request.protocol_version != IPC_PROTOCOL_VERSION {
        return IpcResponse::error(
            request.request_id,
            "protocol_version_mismatch",
            format!(
                "Unsupported MCP IPC protocol version {}; expected {}",
                request.protocol_version, IPC_PROTOCOL_VERSION
            ),
            None,
        );
    }

    if request.token != token {
        return IpcResponse::error(
            request.request_id,
            "unauthorized",
            "Invalid MCP IPC token",
            None,
        );
    }

    let mcp = app.state::<McpState>();
    let enabled = app.state::<GuiConfigState>().get().mcp_enabled;
    if mcp.record_client(&request.client).await {
        mcp.emit_status(app.clone(), enabled).await;
    }

    if !enabled && !mcp_request_allowed_when_disabled(&request.method) {
        record_disabled_mcp_tool_call(&app, &request);
        return mcp_disabled_response(request.request_id);
    }

    let mut tool_call =
        mcp_tool_call_for_request(&request.method, &request.params, request.request_id);
    if let Some(tool_call) = &tool_call {
        emit_tracked_mcp_tool_call_event(&app, tool_call, McpToolCallPhase::Started);
    }
    if let Some(tool_call) = &mut tool_call {
        tool_call.activity = start_mcp_tool_call_activity(&app, &request, tool_call);
    }

    let result = dispatch_gui_request(
        app.clone(),
        &request.method,
        request.params,
        tool_call.clone(),
    )
    .await;
    if let Some(tool_call) = &tool_call
        && mcp_tool_call_finishes_with_response(&request.method, &result)
    {
        finish_tracked_mcp_tool_call_activity(
            &app,
            tool_call,
            mcp_tool_call_finished_phase(&result),
            mcp_tool_call_error_message(&result),
        );
        emit_tracked_mcp_tool_call_event(&app, tool_call, mcp_tool_call_finished_phase(&result));
    }

    match result {
        Ok(result) => IpcResponse::success(request.request_id, result),
        Err(error) => IpcResponse::error(request.request_id, error.code, error.message, error.data),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct McpIpcError {
    pub code: String,
    pub message: String,
    pub data: Option<Value>,
}

impl McpIpcError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(code: impl Into<String>, message: impl Into<String>, data: Value) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            data: Some(data),
        }
    }

    fn from_error(code: impl Into<String>, error: impl std::error::Error) -> Self {
        Self::new(code, error.to_string())
    }

    fn from_rust_error(code: impl Into<String>, error: RustError) -> Self {
        Self::new(code, error.into_message())
    }
}

async fn dispatch_gui_request(
    app: AppHandle,
    method: &str,
    params: Value,
    tool_call: Option<McpTrackedToolCall>,
) -> Result<Value, McpIpcError> {
    match method {
        "list_projects" => list_projects(app).await,
        "get_project_details" => get_project_details(app, params).await,
        "list_repositories" => list_repositories(app).await,
        "add_repository" => add_repository(app, params).await,
        "get_package_details" => get_package_details(app, params).await,
        "list_packages" => list_packages(app, params).await,
        "list_repository_packages" => list_repository_packages(app, params).await,
        "get_environment_settings" => get_environment_settings(app).await,
        "search_activity_logs" => search_activity_logs(app, params).await,
        "get_activity_log_entry" => get_activity_log_entry(app, params).await,
        "summarize_activity_logs" => summarize_activity_logs(app, params).await,
        "get_activity_log_context" => get_activity_log_context(app, params).await,
        "search_technical_logs" => search_technical_logs(app, params).await,
        "get_technical_log_entry" => get_technical_log_entry(app, params).await,
        "summarize_technical_logs" => summarize_technical_logs(app, params).await,
        "create_project" => create_project(app, params).await,
        "add_existing_project" => add_existing_project(app, params).await,
        "backup_project" => backup_project(app, params).await,
        "copy_project" => copy_project(app, params).await,
        "restore_project_from_backup" => restore_project_from_backup(app, params).await,
        "install_project_package" => install_project_package(app, params).await,
        "uninstall_project_package" => uninstall_project_package(app, params).await,
        "reinstall_project_package" => reinstall_project_package(app, params).await,
        IPC_METHOD_PROJECT_TASK_START => project_task_start(app, params, tool_call).await,
        IPC_METHOD_PROJECT_TASK_GET => project_task_get(app, params).await,
        IPC_METHOD_PROJECT_TASK_LIST => project_task_list(app).await,
        IPC_METHOD_PROJECT_TASK_CANCEL => project_task_cancel(app, params).await,
        "search_packages" => list_packages(app, params).await,
        _ => Err(McpIpcError::new(
            "unknown_method",
            format!("MCP IPC method is not implemented: {method}"),
        )),
    }
}

async fn search_activity_logs(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<ActivityLogSearchParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let activity = app.state::<ActivityLogState>();
    let response = logs::search_activity_logs(&activity, params).map_err(mcp_activity_log_error)?;
    serde_json::to_value(response).map_err(|e| McpIpcError::from_error("serialization_error", e))
}

async fn get_activity_log_entry(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<ActivityLogEntryParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let activity = app.state::<ActivityLogState>();
    let response =
        logs::get_activity_log_entry(&activity, params).map_err(mcp_activity_log_error)?;
    serde_json::to_value(response).map_err(|e| McpIpcError::from_error("serialization_error", e))
}

async fn summarize_activity_logs(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<ActivityLogSummaryParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let activity = app.state::<ActivityLogState>();
    let response =
        logs::summarize_activity_logs(&activity, params).map_err(mcp_activity_log_error)?;
    serde_json::to_value(response).map_err(|e| McpIpcError::from_error("serialization_error", e))
}

async fn get_activity_log_context(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<ActivityLogContextParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let activity = app.state::<ActivityLogState>();
    let response =
        logs::get_activity_log_context(&activity, params).map_err(mcp_activity_log_error)?;
    serde_json::to_value(response).map_err(|e| McpIpcError::from_error("serialization_error", e))
}

async fn search_technical_logs(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<TechnicalLogSearchParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let io = app.state::<DefaultEnvironmentIo>();
    let folder = crate::logging::log_folder(&io);
    let response = logs::search_technical_logs(&folder, params).map_err(mcp_technical_log_error)?;
    serde_json::to_value(response).map_err(|e| McpIpcError::from_error("serialization_error", e))
}

async fn get_technical_log_entry(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<TechnicalLogEntryParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let io = app.state::<DefaultEnvironmentIo>();
    let folder = crate::logging::log_folder(&io);
    let response =
        logs::get_technical_log_entry(&folder, params).map_err(mcp_technical_log_error)?;
    serde_json::to_value(response).map_err(|e| McpIpcError::from_error("serialization_error", e))
}

async fn summarize_technical_logs(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<TechnicalLogSummaryParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let io = app.state::<DefaultEnvironmentIo>();
    let folder = crate::logging::log_folder(&io);
    let response =
        logs::summarize_technical_logs(&folder, params).map_err(mcp_technical_log_error)?;
    serde_json::to_value(response).map_err(|e| McpIpcError::from_error("serialization_error", e))
}

fn mcp_activity_log_error(error: crate::activity_log::ActivityLogQueryError) -> McpIpcError {
    McpIpcError::new(error.code(), error.message())
}

fn mcp_technical_log_error(error: crate::logging::TechnicalLogQueryError) -> McpIpcError {
    McpIpcError::new(error.code(), error.message())
}

fn mcp_request_allowed_when_disabled(method: &str) -> bool {
    matches!(
        method,
        IPC_METHOD_PROJECT_TASK_GET | IPC_METHOD_PROJECT_TASK_LIST | IPC_METHOD_PROJECT_TASK_CANCEL
    )
}

fn mcp_tool_call_for_request(
    method: &str,
    params: &Value,
    request_id: Uuid,
) -> Option<McpTrackedToolCall> {
    if matches!(
        method,
        IPC_METHOD_PROJECT_TASK_GET | IPC_METHOD_PROJECT_TASK_LIST
    ) {
        return None;
    }

    let tool_name = mcp_tool_name(method, params)
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("mcp.{method}"));
    Some(McpTrackedToolCall {
        request_id,
        tool_name,
        activity: None,
    })
}

fn mcp_tool_call_finishes_with_response(method: &str, result: &Result<Value, McpIpcError>) -> bool {
    method != IPC_METHOD_PROJECT_TASK_START || result.is_err()
}

fn mcp_tool_name(method: &str, params: &Value) -> Option<&'static str> {
    if method == IPC_METHOD_PROJECT_TASK_START {
        return params
            .get("method")
            .and_then(Value::as_str)
            .and_then(mcp_project_task_tool_name);
    }

    mcp_tool_name_for_method(method)
}

fn mcp_tool_name_for_method(method: &str) -> Option<&'static str> {
    if method == "search_packages" {
        return Some("alcomd3_list_packages");
    }

    mcp_tool_capability_for_method(method).map(|capability| capability.tool_name)
}

fn mcp_project_task_tool_name(method: &str) -> Option<&'static str> {
    mcp_tool_capability_for_method(method)
        .filter(|capability| !capability.read_only)
        .map(|capability| capability.tool_name)
}

fn mcp_tool_call_finished_phase<T, E>(result: &Result<T, E>) -> McpToolCallPhase {
    match result {
        Ok(_) => McpToolCallPhase::Finished,
        Err(_) => McpToolCallPhase::Failed,
    }
}

fn mcp_tool_call_error_message(result: &Result<Value, McpIpcError>) -> Option<String> {
    result.as_ref().err().map(|error| error.message.clone())
}

fn start_mcp_tool_call_activity(
    app: &AppHandle,
    request: &IpcRequest,
    tool_call: &McpTrackedToolCall,
) -> Option<ActivityTracker> {
    let activity = app.try_state::<ActivityLogState>()?;
    let input = ActivityInput::new(
        ActivitySource::Mcp,
        mcp_activity_kind(&request.method, &request.params),
        mcp_activity_importance(&request.method, &request.params),
        mcp_activity_operation(&tool_call.tool_name),
        format!("MCP started {}", tool_call.tool_name),
    )
    .target(
        mcp_activity_target(&request.method, &request.params)
            .unwrap_or_else(|| tool_call.tool_name.clone()),
    )
    .details(mcp_activity_details(&request.method, &request.params))
    .request_id(request.request_id.to_string())
    .tool_name(tool_call.tool_name.clone())
    .client_name(mcp_client_name(&request.client));
    Some(activity.start_activity(Some(app), input))
}

fn finish_tracked_mcp_tool_call_activity(
    app: &AppHandle,
    tool_call: &McpTrackedToolCall,
    phase: McpToolCallPhase,
    error: Option<String>,
) {
    let Some(activity) = app.try_state::<ActivityLogState>() else {
        return;
    };
    let Some(tracker) = &tool_call.activity else {
        return;
    };
    match phase {
        McpToolCallPhase::Started => {}
        McpToolCallPhase::Finished => {
            activity.finish_success(
                Some(app),
                tracker,
                format!("MCP completed {}", tool_call.tool_name),
                Vec::new(),
            );
        }
        McpToolCallPhase::Failed => {
            let error = error.unwrap_or_else(|| "MCP request failed".to_string());
            activity.finish_failed(
                Some(app),
                tracker,
                format!("MCP failed {}", tool_call.tool_name),
                Vec::new(),
                error,
            );
        }
    }
}

fn record_disabled_mcp_tool_call(app: &AppHandle, request: &IpcRequest) {
    let Some(mut tool_call) =
        mcp_tool_call_for_request(&request.method, &request.params, request.request_id)
    else {
        return;
    };
    emit_tracked_mcp_tool_call_event(app, &tool_call, McpToolCallPhase::Started);
    tool_call.activity = start_mcp_tool_call_activity(app, request, &tool_call);
    finish_tracked_mcp_tool_call_activity(
        app,
        &tool_call,
        McpToolCallPhase::Failed,
        Some(MCP_DISABLED_MESSAGE.to_string()),
    );
    emit_tracked_mcp_tool_call_event(app, &tool_call, McpToolCallPhase::Failed);
}

fn cancel_tracked_mcp_tool_call_activity(app: &AppHandle, tool_call: &McpTrackedToolCall) {
    let Some(activity) = app.try_state::<ActivityLogState>() else {
        return;
    };
    let Some(tracker) = &tool_call.activity else {
        return;
    };
    activity.finish_cancelled(
        Some(app),
        tracker,
        format!("MCP cancelled {}", tool_call.tool_name),
        Vec::new(),
    );
}

fn mcp_activity_operation(tool_name: &str) -> String {
    format!("mcp.{tool_name}")
}

fn mcp_activity_kind(method: &str, params: &Value) -> ActivityKind {
    if method == IPC_METHOD_PROJECT_TASK_CANCEL
        || mcp_tool_name_for_method(method)
            .and_then(crate::backend::mcp_capabilities::mcp_tool_capability_for_tool_name)
            .is_some_and(|capability| !capability.read_only)
        || (method == IPC_METHOD_PROJECT_TASK_START
            && params
                .get("method")
                .and_then(Value::as_str)
                .and_then(mcp_tool_name_for_method)
                .and_then(crate::backend::mcp_capabilities::mcp_tool_capability_for_tool_name)
                .is_some_and(|capability| !capability.read_only))
    {
        ActivityKind::Write
    } else {
        ActivityKind::Read
    }
}

fn mcp_activity_importance(method: &str, params: &Value) -> ActivityImportance {
    if mcp_activity_kind(method, params) == ActivityKind::Write {
        ActivityImportance::Primary
    } else {
        ActivityImportance::Secondary
    }
}

fn mcp_client_name(client: &ClientIdentity) -> String {
    match &client.version {
        Some(version) if !version.is_empty() => format!("{} {version}", client.name),
        _ => client.name.clone(),
    }
}

fn mcp_activity_target(method: &str, params: &Value) -> Option<String> {
    if method == IPC_METHOD_PROJECT_TASK_START {
        let inner_method = params
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or(method);
        return params
            .get("params")
            .and_then(|params| mcp_activity_target(inner_method, params));
    }

    for key in [
        "project_path",
        "projectPath",
        "source_project_path",
        "sourceProjectPath",
        "new_project_path",
        "newProjectPath",
        "backup_path",
        "backupPath",
    ] {
        if let Some(path) = params.get(key).and_then(Value::as_str) {
            return Some(target_from_path(path));
        }
    }

    if let Some(project_name) = params
        .get("project_name")
        .or_else(|| params.get("projectName"))
        .and_then(Value::as_str)
    {
        return Some(project_name.to_string());
    }

    if let Some(url) = params
        .get("repository_url")
        .or_else(|| params.get("repositoryUrl"))
        .and_then(Value::as_str)
    {
        return Some(summarize_url_host(url));
    }

    params
        .get("package_name")
        .or_else(|| params.get("packageName"))
        .or_else(|| params.get("package_id"))
        .or_else(|| params.get("packageId"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn mcp_activity_details(method: &str, params: &Value) -> Vec<ActivityDetail> {
    let params = if method == IPC_METHOD_PROJECT_TASK_START {
        params.get("params").unwrap_or(params)
    } else {
        params
    };
    let mut details = vec![ActivityDetail::new("method", method)];
    for key in [
        "project_path",
        "projectPath",
        "source_project_path",
        "sourceProjectPath",
        "new_project_path",
        "newProjectPath",
        "base_path",
        "basePath",
        "backup_path",
        "backupPath",
    ] {
        if let Some(path) = params.get(key).and_then(Value::as_str) {
            details.push(ActivityDetail::new(key, summarize_path(path)));
        }
    }
    for key in ["repository_url", "repositoryUrl"] {
        if let Some(url) = params.get(key).and_then(Value::as_str) {
            details.push(ActivityDetail::new(key, summarize_url(url)));
        }
    }
    if let Some(headers) = params.get("headers").and_then(Value::as_object) {
        details.push(ActivityDetail::new(
            "headers",
            format!("{} headers", headers.len()),
        ));
    }
    for key in [
        "limit",
        "query",
        "package_name",
        "packageName",
        "package_id",
        "packageId",
        "backup_name",
        "backupName",
        "exclude_vpm_packages",
        "excludeVpmPackages",
        "project_name",
        "projectName",
        "template_id",
        "templateId",
        "unity_version",
        "unityVersion",
        "version_selector",
        "versionSelector",
        "source",
        "allow_conflicts",
        "allowConflicts",
    ] {
        if let Some(value) = params.get(key) {
            details.push(safe_detail_from_json(key, value));
        }
    }
    details
}

fn emit_tracked_mcp_tool_call_event(
    app: &AppHandle,
    tool_call: &McpTrackedToolCall,
    phase: McpToolCallPhase,
) {
    emit_mcp_tool_call_event(app, tool_call.request_id, &tool_call.tool_name, phase);
}

fn emit_mcp_tool_call_event(
    app: &AppHandle,
    request_id: Uuid,
    tool_name: &str,
    phase: McpToolCallPhase,
) {
    let event = McpToolCallEvent {
        request_id: request_id.to_string(),
        tool_name: tool_name.to_string(),
        phase,
    };
    if let Err(e) = app.emit(MCP_TOOL_CALL_EVENT, event) {
        log::error!("failed to emit MCP tool call event: {e}");
    }
}

async fn list_projects(app: AppHandle) -> Result<Value, McpIpcError> {
    let io = app.state::<DefaultEnvironmentIo>();
    let connection = VccDatabaseConnection::connect(io.inner())
        .await
        .map_err(|e| McpIpcError::from_error("project_database_error", e))?;

    let projects = connection
        .get_projects()
        .iter()
        .filter_map(project_summary)
        .collect::<Vec<_>>();

    Ok(json!({
        "ok": true,
        "projects": projects,
    }))
}

#[derive(Deserialize)]
struct ProjectDetailsParams {
    project_path: String,
}

#[derive(Deserialize)]
struct BackupProjectParams {
    project_path: String,
    backup_name: Option<String>,
    #[serde(default)]
    exclude_vpm_packages: bool,
}

#[derive(Deserialize)]
struct CopyProjectParams {
    source_project_path: String,
    new_project_path: String,
}

#[derive(Deserialize)]
struct RestoreProjectFromBackupParams {
    backup_path: String,
    project_name: Option<String>,
}

#[derive(Deserialize)]
struct CreateProjectParams {
    project_name: String,
    base_path: Option<String>,
    template_id: Option<String>,
    unity_version: Option<String>,
}

#[derive(Deserialize)]
struct AddExistingProjectParams {
    project_path: String,
}

#[derive(Deserialize)]
struct AddRepositoryParams {
    repository_url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct InstallProjectPackageParams {
    project_path: String,
    package_name: String,
    version_selector: ProjectPackageVersionSelector,
    source: Option<ProjectPackageSourceParams>,
    #[serde(default)]
    allow_conflicts: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ProjectPackageParams {
    project_path: String,
    package_name: String,
    #[serde(default)]
    allow_conflicts: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ProjectPackageVersionSelector {
    LatestGuiVisible,
    Exact { version: String },
}

#[derive(Debug, Clone, Deserialize)]
struct ProjectPackageSourceParams {
    repository_id: Option<String>,
    repository_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTaskStartParams {
    task_id: String,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTaskIdParams {
    task_id: String,
}

async fn get_project_details(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<ProjectDetailsParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let io = app.state::<DefaultEnvironmentIo>();
    ensure_registered_project(io.inner(), &params.project_path).await?;

    let snapshot = load_project_details_snapshot(params.project_path.clone())
        .await
        .map_err(|e| McpIpcError::from_rust_error("project_load_error", e))?;

    let installed_packages = snapshot
        .installed_packages
        .into_iter()
        .map(|package| {
            json!({
                "id": package.id,
                "package": package_manifest_summary(&package.package),
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "ok": true,
        "project": {
            "path": params.project_path,
            "unity": {
                "major": snapshot.unity.0,
                "minor": snapshot.unity.1,
                "version": snapshot.unity_str,
                "revision": snapshot.unity_revision,
            },
            "shouldResolve": snapshot.should_resolve,
            "installedPackages": installed_packages,
        },
    }))
}

async fn create_project(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<CreateProjectParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;

    let packages = app.state::<PackagesState>();
    let settings = app.state::<SettingsState>();
    let config = app.state::<GuiConfigState>();
    let io = app.state::<DefaultEnvironmentIo>();
    let http = app.state::<reqwest::Client>();
    let project = project_operations::create_project(
        packages.inner(),
        settings.inner(),
        config.inner(),
        io.inner(),
        http.inner(),
        params.base_path,
        params.project_name,
        params.template_id,
        params.unity_version,
        None,
    )
    .await
    .map_err(|e| McpIpcError::from_rust_error("project_create_error", e))?;

    Ok(json!({
        "ok": true,
        "projectPath": project.project_path,
        "templateId": project.template_id,
        "unityVersion": project.unity_version,
    }))
}

async fn add_existing_project(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<AddExistingProjectParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;

    let settings = app.state::<SettingsState>();
    let io = app.state::<DefaultEnvironmentIo>();
    let project_path =
        project_operations::add_existing_project(settings.inner(), io.inner(), params.project_path)
            .await
            .map_err(|e| McpIpcError::from_rust_error("project_add_error", e))?;

    Ok(json!({
        "ok": true,
        "projectPath": project_path,
    }))
}

async fn backup_project(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<BackupProjectParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let io = app.state::<DefaultEnvironmentIo>();
    ensure_registered_project(io.inner(), &params.project_path).await?;

    let project_backup = app.state::<ProjectBackupState>();
    if !project_backup.try_start_uncancellable() {
        return Err(McpIpcError::new(
            "project_backup_already_running",
            "A project backup is already running",
        ));
    }

    let config = app.state::<GuiConfigState>();
    let settings = app.state::<SettingsState>();
    let result = project_operations::create_project_backup(
        config.inner(),
        settings.inner(),
        io.inner(),
        params.project_path,
        params.backup_name,
        params.exclude_vpm_packages,
        |_| {},
    )
    .await;
    project_backup.finish();

    let backup_path =
        result.map_err(|e| McpIpcError::from_rust_error("project_backup_error", e))?;
    Ok(json!({
        "ok": true,
        "backupPath": backup_path.display().to_string(),
    }))
}

async fn copy_project(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<CopyProjectParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let io = app.state::<DefaultEnvironmentIo>();
    ensure_registered_project(io.inner(), &params.source_project_path).await?;

    let project_copy = app.state::<ProjectCopyState>();
    if !project_copy.try_start_uncancellable() {
        return Err(McpIpcError::new(
            "project_copy_already_running",
            "A project copy is already running",
        ));
    }

    let settings = app.state::<SettingsState>();
    let result = project_operations::copy_registered_project(
        settings.inner(),
        io.inner(),
        params.source_project_path,
        params.new_project_path,
        |_| {},
    )
    .await;
    project_copy.finish();

    let project_path = result.map_err(|e| McpIpcError::from_rust_error("project_copy_error", e))?;
    Ok(json!({
        "ok": true,
        "projectPath": project_path,
    }))
}

async fn restore_project_from_backup(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<RestoreProjectFromBackupParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;

    let project_restore = app.state::<ProjectRestoreState>();
    if !project_restore.try_start_uncancellable() {
        return Err(McpIpcError::new(
            "project_restore_already_running",
            "A project restore is already running",
        ));
    }

    let settings = app.state::<SettingsState>();
    let io = app.state::<DefaultEnvironmentIo>();
    let result = project_operations::restore_project_from_backup(
        settings.inner(),
        io.inner(),
        params.backup_path,
        params.project_name,
        |_| {},
    )
    .await;
    project_restore.finish();

    let project_path =
        result.map_err(|e| McpIpcError::from_rust_error("project_restore_error", e))?;
    Ok(json!({
        "ok": true,
        "projectPath": project_path,
    }))
}

async fn install_project_package(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    install_project_package_with_abort(app, params, None).await
}

async fn install_project_package_with_abort(
    app: AppHandle,
    params: Value,
    prestarted_abort: Option<AbortCheck>,
) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<InstallProjectPackageParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let package_name = normalize_project_package_name(params.package_name)?;
    let source = parse_project_package_source(params.source)?;
    let io = app.state::<DefaultEnvironmentIo>();
    ensure_registered_project(io.inner(), &params.project_path).await?;

    let settings = app.state::<SettingsState>();
    let packages_state = app.state::<PackagesState>();
    let changes = app.state::<ChangesState>();
    let http = app.state::<reqwest::Client>();
    let settings = settings
        .load(io.inner())
        .await
        .map_err(|e| McpIpcError::from_error("settings_load_error", e))?;
    let show_prerelease_packages = settings.show_prerelease_packages();
    let defined_repository_ids = settings
        .get_user_repos()
        .iter()
        .filter_map(|repo| repo.id().or(repo.url().map(url::Url::as_str)))
        .map(str::to_string)
        .collect::<Vec<_>>();
    let default_repository_ids = vec![
        OFFICIAL_REPOSITORY_ID.to_string(),
        CURATED_REPOSITORY_ID.to_string(),
    ];
    packages_state
        .load(&settings, io.inner(), http.inner(), app.clone())
        .await
        .map_err(|e| McpIpcError::from_error("packages_load_error", e))?;
    let Some(packages) = packages_state.get() else {
        return Err(McpIpcError::new(
            "packages_load_error",
            "Internal Error: environment version mismatch",
        ));
    };
    let config = app.state::<GuiConfigState>().get();
    let hidden_user_repositories = config.gui_hidden_repositories.clone();
    let hide_local_user_packages = config.hide_local_user_packages;
    drop(config);

    let project_path = params.project_path.clone();
    let package_name_for_changes = package_name.clone();
    let version_selector = params.version_selector.clone();
    let source_for_changes = source.clone();
    let prepared = changes
        .build_changes(
            &packages,
            |collection, package_infos| async move {
                let unity_project = load_project(project_path.clone())
                    .await
                    .map_err(|e| McpIpcError::from_rust_error("project_load_error", e))?;
                let project = load_project_details_snapshot(project_path.clone())
                    .await
                    .map_err(|e| McpIpcError::from_rust_error("project_load_error", e))?;
                let installing_package = select_project_install_package(
                    package_infos,
                    &package_name_for_changes,
                    &version_selector,
                    source_for_changes.as_ref(),
                    &hidden_user_repositories,
                    hide_local_user_packages,
                    show_prerelease_packages,
                    &default_repository_ids,
                    &defined_repository_ids,
                    &project,
                )?;

                unity_project
                    .add_package_request(
                        collection,
                        &[installing_package],
                        AddPackageOperation::AutoDetected,
                        show_prerelease_packages,
                    )
                    .await
                    .map_err(|e| {
                        McpIpcError::from_rust_error("project_package_change_error", e.into())
                    })
            },
            |changes_version, changes| {
                prepared_project_package_changes(
                    "install",
                    params.project_path,
                    package_name,
                    changes_version,
                    changes,
                )
            },
        )
        .await?;

    apply_prepared_project_package_changes(app, prepared, params.allow_conflicts, prestarted_abort)
        .await
}

async fn uninstall_project_package(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    uninstall_project_package_with_abort(app, params, None).await
}

async fn uninstall_project_package_with_abort(
    app: AppHandle,
    params: Value,
    prestarted_abort: Option<AbortCheck>,
) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<ProjectPackageParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let package_name = normalize_project_package_name(params.package_name)?;
    let io = app.state::<DefaultEnvironmentIo>();
    ensure_registered_project(io.inner(), &params.project_path).await?;

    let changes = app.state::<ChangesState>();
    let unity_project = load_project(params.project_path.clone())
        .await
        .map_err(|e| McpIpcError::from_rust_error("project_load_error", e))?;
    ensure_project_package_installed(&unity_project, &package_name)?;
    let package_names = [package_name.as_str()];
    let project_changes = unity_project
        .remove_request(&package_names)
        .await
        .map_err(|e| McpIpcError::from_rust_error("project_package_change_error", e.into()))?;
    let prepared = changes.set(project_changes, |changes_version, changes| {
        prepared_project_package_changes(
            "uninstall",
            params.project_path,
            package_name,
            changes_version,
            changes,
        )
    });

    apply_prepared_project_package_changes(app, prepared, params.allow_conflicts, prestarted_abort)
        .await
}

async fn reinstall_project_package(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    reinstall_project_package_with_abort(app, params, None).await
}

async fn reinstall_project_package_with_abort(
    app: AppHandle,
    params: Value,
    prestarted_abort: Option<AbortCheck>,
) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<ProjectPackageParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let package_name = normalize_project_package_name(params.package_name)?;
    let io = app.state::<DefaultEnvironmentIo>();
    ensure_registered_project(io.inner(), &params.project_path).await?;

    let settings = app.state::<SettingsState>();
    let packages = app.state::<PackagesState>();
    let changes = app.state::<ChangesState>();
    let http = app.state::<reqwest::Client>();
    let settings = settings
        .load(io.inner())
        .await
        .map_err(|e| McpIpcError::from_error("settings_load_error", e))?;
    let packages = packages
        .load(&settings, io.inner(), http.inner(), app.clone())
        .await
        .map_err(|e| McpIpcError::from_error("packages_load_error", e))?;

    let project_path = params.project_path.clone();
    let package_name_for_changes = package_name.clone();
    let prepared = changes
        .build_changes_no_list(
            &packages,
            |collection| async move {
                let unity_project = load_project(project_path.clone())
                    .await
                    .map_err(|e| McpIpcError::from_rust_error("project_load_error", e))?;
                ensure_project_package_installed(&unity_project, &package_name_for_changes)?;
                let package_names = [package_name_for_changes.as_str()];
                unity_project
                    .reinstall_request(collection, &package_names)
                    .await
                    .map_err(|e| {
                        McpIpcError::from_rust_error("project_package_change_error", e.into())
                    })
            },
            |changes_version, changes| {
                prepared_project_package_changes(
                    "reinstall",
                    params.project_path,
                    package_name,
                    changes_version,
                    changes,
                )
            },
        )
        .await?;

    apply_prepared_project_package_changes(app, prepared, params.allow_conflicts, prestarted_abort)
        .await
}

struct PreparedProjectPackageChanges {
    operation: &'static str,
    project_path: String,
    package_name: String,
    changes_version: u32,
    changes: TauriPendingProjectChanges,
    requires_allow_conflicts: bool,
}

fn prepared_project_package_changes(
    operation: &'static str,
    project_path: String,
    package_name: String,
    changes_version: u32,
    changes: &PendingProjectChanges,
) -> PreparedProjectPackageChanges {
    PreparedProjectPackageChanges {
        operation,
        project_path,
        package_name,
        changes_version,
        changes: TauriPendingProjectChanges::new(changes_version, changes),
        requires_allow_conflicts: pending_project_changes_require_allow_conflicts(changes),
    }
}

async fn apply_prepared_project_package_changes(
    app: AppHandle,
    prepared: PreparedProjectPackageChanges,
    allow_conflicts: bool,
    prestarted_abort: Option<AbortCheck>,
) -> Result<Value, McpIpcError> {
    if prepared.requires_allow_conflicts && !allow_conflicts {
        app.state::<ChangesState>().clear_cache();
        let changes = serde_json::to_value(&prepared.changes)
            .map_err(|e| McpIpcError::from_error("serialization_error", e))?;
        return Err(McpIpcError::with_data(
            "project_package_conflicts",
            "Project package changes include conflicts or legacy file removals; retry with allow_conflicts=true to apply",
            json!({ "changes": changes }),
        ));
    }

    match prestarted_abort {
        Some(abort) => {
            project_operations::apply_project_changes_with_abort(
                app,
                prepared.project_path.clone(),
                prepared.changes_version,
                abort,
            )
            .await
        }
        None => {
            project_operations::apply_project_changes(
                app,
                prepared.project_path.clone(),
                prepared.changes_version,
            )
            .await
        }
    }
    .map_err(|e| McpIpcError::from_rust_error("project_package_apply_error", e))?;
    let changes = serde_json::to_value(&prepared.changes)
        .map_err(|e| McpIpcError::from_error("serialization_error", e))?;

    Ok(json!({
        "ok": true,
        "operation": prepared.operation,
        "projectPath": prepared.project_path,
        "packageName": prepared.package_name,
        "changes": changes,
    }))
}

fn pending_project_changes_require_allow_conflicts(changes: &PendingProjectChanges) -> bool {
    !changes.conflicts().is_empty()
        || !changes.remove_legacy_files().is_empty()
        || !changes.remove_legacy_folders().is_empty()
}

fn normalize_project_package_name(package_name: String) -> Result<String, McpIpcError> {
    let package_name = package_name.trim().to_string();
    if package_name.is_empty() {
        return Err(McpIpcError::new(
            "invalid_params",
            "package_name must be provided",
        ));
    }
    if !is_valid_package_name(&package_name) {
        return Err(McpIpcError::new(
            "invalid_params",
            "package_name must be a valid VPM package identifier",
        ));
    }
    Ok(package_name)
}

fn parse_project_package_source(
    source: Option<ProjectPackageSourceParams>,
) -> Result<Option<RepositorySelector>, McpIpcError> {
    let Some(source) = source else {
        return Ok(None);
    };
    let repository_id = normalize_optional_string(source.repository_id);
    let repository_url = normalize_optional_string(source.repository_url);
    if repository_id.is_none() && repository_url.is_none() {
        return Ok(None);
    }

    Ok(Some(RepositorySelector::from_values(
        repository_id,
        repository_url,
    )?))
}

fn ensure_project_package_installed(
    unity_project: &vrc_get_vpm::UnityProject,
    package_name: &str,
) -> Result<(), McpIpcError> {
    if unity_project
        .installed_packages()
        .any(|(id, _)| id == package_name)
    {
        Ok(())
    } else {
        Err(McpIpcError::new(
            "project_package_not_installed",
            "package_name must match an installed project package",
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn select_project_install_package<'package, 'env>(
    package_infos: &'package [PackageInfo<'env>],
    package_name: &str,
    version_selector: &ProjectPackageVersionSelector,
    source: Option<&RepositorySelector>,
    hidden_user_repositories: &IndexSet<String>,
    hide_local_user_packages: bool,
    show_prerelease_packages: bool,
    default_repository_ids: &[String],
    defined_repository_ids: &[String],
    project: &ProjectDetailsSnapshot,
) -> Result<PackageInfo<'env>, McpIpcError>
where
    'env: 'package,
{
    let exact_version = match version_selector {
        ProjectPackageVersionSelector::LatestGuiVisible => None,
        ProjectPackageVersionSelector::Exact { version } => Some(
            Version::from_str(version.trim())
                .map_err(|e| McpIpcError::from_error("invalid_package_version", e))?,
        ),
    };

    let rows = build_project_package_row_accumulators(
        package_infos.iter(),
        project,
        hidden_user_repositories,
        hide_local_user_packages,
        show_prerelease_packages,
        default_repository_ids,
        defined_repository_ids,
    );
    let Some(row) = rows.get(package_name) else {
        return Err(McpIpcError::new(
            "package_not_found",
            "package_name must match a GUI-visible ALCOMD3 project package row",
        ));
    };

    let source_matches = |package: PackageInfo<'env>| {
        source.is_none_or(|source| package_is_from_repository(&package, source))
    };
    let version_matches = |package: PackageInfo<'env>| {
        exact_version.as_ref().is_none_or(|exact_version| {
            StrictEqVersion(package.version()) == StrictEqVersion(exact_version)
        })
    };

    let selected = project_package_row_compatible_packages(row)
        .iter()
        .copied()
        .find(|package| source_matches(*package) && version_matches(*package));

    if let Some(package) = selected {
        return Ok(package);
    }

    let incompatible_visible_package_found = project_package_row_incompatible_packages(row)
        .iter()
        .copied()
        .any(|package| source_matches(package) && version_matches(package));

    if incompatible_visible_package_found {
        Err(McpIpcError::new(
            "project_package_unity_incompatible",
            "package_name matched a GUI-visible package, but no selected version is compatible with the project Unity version",
        ))
    } else {
        Err(McpIpcError::new(
            "package_not_found",
            "package_name must match a GUI-visible ALCOMD3 package",
        ))
    }
}

async fn project_task_start(
    app: AppHandle,
    params: Value,
    tool_call: Option<McpTrackedToolCall>,
) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<ProjectTaskStartParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;

    match params.method.as_str() {
        "create_project" => {
            start_create_project_task(app, params.task_id, params.params, tool_call).await
        }
        "backup_project" => {
            start_backup_project_task(app, params.task_id, params.params, tool_call).await
        }
        "copy_project" => {
            start_copy_project_task(app, params.task_id, params.params, tool_call).await
        }
        "restore_project_from_backup" => {
            start_restore_project_task(app, params.task_id, params.params, tool_call).await
        }
        "install_project_package" => {
            start_project_package_task(
                app,
                params.task_id,
                McpProjectTaskKind::InstallPackage,
                params.method,
                params.params,
                tool_call,
            )
            .await
        }
        "uninstall_project_package" => {
            start_project_package_task(
                app,
                params.task_id,
                McpProjectTaskKind::UninstallPackage,
                params.method,
                params.params,
                tool_call,
            )
            .await
        }
        "reinstall_project_package" => {
            start_project_package_task(
                app,
                params.task_id,
                McpProjectTaskKind::ReinstallPackage,
                params.method,
                params.params,
                tool_call,
            )
            .await
        }
        method => Err(McpIpcError::new(
            "unsupported_project_task_method",
            format!("MCP project task method is not supported: {method}"),
        )),
    }
}

async fn project_task_get(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<ProjectTaskIdParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let mcp = app.state::<McpState>();
    let snapshot = mcp
        .project_tasks
        .lock()
        .unwrap()
        .get(&params.task_id)
        .ok_or_else(|| {
            McpIpcError::new(
                "project_task_not_found",
                format!("MCP project task was not found: {}", params.task_id),
            )
        })?;

    task_snapshot_value(snapshot)
}

async fn project_task_list(app: AppHandle) -> Result<Value, McpIpcError> {
    let mcp = app.state::<McpState>();
    let tasks = mcp.project_tasks.lock().unwrap().list();
    Ok(json!({ "tasks": tasks }))
}

async fn project_task_cancel(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<ProjectTaskIdParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let mcp = app.state::<McpState>();
    let (snapshot, tool_call) = mcp.project_tasks.lock().unwrap().cancel(&params.task_id)?;
    if snapshot.status.is_terminal() {
        finish_project_task_kind(&app, snapshot.kind);
    }
    if let Some(tool_call) = tool_call {
        cancel_tracked_mcp_tool_call_activity(&app, &tool_call);
        emit_tracked_mcp_tool_call_event(&app, &tool_call, McpToolCallPhase::Failed);
    }

    task_snapshot_value(snapshot)
}

async fn start_create_project_task(
    app: AppHandle,
    task_id: String,
    params: Value,
    tool_call: Option<McpTrackedToolCall>,
) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<CreateProjectParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;

    let snapshot =
        start_project_task_record(&app, &task_id, McpProjectTaskKind::Create, tool_call)?;
    let tasks = app.state::<McpState>().project_tasks.clone();
    let create_abort = AbortCheck::new();
    let app_for_task = app.clone();
    let task_id_for_task = task_id.clone();
    let tasks_for_task = tasks.clone();
    let create_abort_for_task = create_abort.clone();
    let _task = tokio::spawn(async move {
        let packages = app_for_task.state::<PackagesState>();
        let settings = app_for_task.state::<SettingsState>();
        let config = app_for_task.state::<GuiConfigState>();
        let io = app_for_task.state::<DefaultEnvironmentIo>();
        let http = app_for_task.state::<reqwest::Client>();
        let result = project_operations::create_project(
            packages.inner(),
            settings.inner(),
            config.inner(),
            io.inner(),
            http.inner(),
            params.base_path,
            params.project_name,
            params.template_id,
            params.unity_version,
            Some(create_abort_for_task),
        )
        .await;

        match result {
            Ok(project) => {
                let tool_call = tasks_for_task.lock().unwrap().finish_success(
                    &task_id_for_task,
                    json!({
                        "ok": true,
                        "projectPath": project.project_path,
                        "templateId": project.template_id,
                        "unityVersion": project.unity_version,
                    }),
                );
                if let Some(tool_call) = tool_call {
                    finish_tracked_mcp_tool_call_activity(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Finished,
                        None,
                    );
                    emit_tracked_mcp_tool_call_event(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Finished,
                    );
                }
            }
            Err(error) => {
                let error = McpIpcError::from_rust_error("project_create_error", error);
                let error_message = error.message.clone();
                let cancel_requested = tasks_for_task
                    .lock()
                    .unwrap()
                    .cancel_requested(&task_id_for_task);
                if cancel_requested && is_project_create_abort_error(&error) {
                    let tool_call = tasks_for_task
                        .lock()
                        .unwrap()
                        .finish_cancelled(&task_id_for_task);
                    if let Some(tool_call) = tool_call {
                        cancel_tracked_mcp_tool_call_activity(&app_for_task, &tool_call);
                        emit_tracked_mcp_tool_call_event(
                            &app_for_task,
                            &tool_call,
                            McpToolCallPhase::Failed,
                        );
                    }
                    return;
                }
                let tool_call = tasks_for_task
                    .lock()
                    .unwrap()
                    .finish_error(&task_id_for_task, error);
                if let Some(tool_call) = tool_call {
                    finish_tracked_mcp_tool_call_activity(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Failed,
                        Some(error_message),
                    );
                    emit_tracked_mcp_tool_call_event(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Failed,
                    );
                }
            }
        }
    });

    tasks.lock().unwrap().set_cancel_handle(
        &task_id,
        McpProjectTaskCancelHandle::AbortProjectCreate(create_abort),
    );

    task_snapshot_value(tasks.lock().unwrap().get(&task_id).unwrap_or(snapshot))
}

async fn start_backup_project_task(
    app: AppHandle,
    task_id: String,
    params: Value,
    tool_call: Option<McpTrackedToolCall>,
) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<BackupProjectParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let io = app.state::<DefaultEnvironmentIo>();
    ensure_registered_project(io.inner(), &params.project_path).await?;

    let project_backup = app.state::<ProjectBackupState>().inner().clone();
    if !project_backup.try_start_uncancellable() {
        return Err(McpIpcError::new(
            "project_backup_already_running",
            "A project backup is already running",
        ));
    }

    let snapshot =
        start_project_task_record(&app, &task_id, McpProjectTaskKind::Backup, tool_call)?;
    let tasks = app.state::<McpState>().project_tasks.clone();
    let (start_tx, start_rx) = oneshot::channel();
    let app_for_task = app.clone();
    let task_id_for_task = task_id.clone();
    let tasks_for_task = tasks.clone();
    let project_backup_for_task = project_backup.clone();
    let handle = tokio::spawn(async move {
        if start_rx.await.is_err() {
            project_backup_for_task.finish();
            let tool_call = tasks_for_task
                .lock()
                .unwrap()
                .cancel_if_working(&task_id_for_task);
            if let Some(tool_call) = tool_call {
                cancel_tracked_mcp_tool_call_activity(&app_for_task, &tool_call);
                emit_tracked_mcp_tool_call_event(
                    &app_for_task,
                    &tool_call,
                    McpToolCallPhase::Failed,
                );
            }
            return;
        }

        let config = app_for_task.state::<GuiConfigState>();
        let settings = app_for_task.state::<SettingsState>();
        let io = app_for_task.state::<DefaultEnvironmentIo>();
        let progress_tasks = tasks_for_task.clone();
        let progress_task_id = task_id_for_task.clone();
        let result = project_operations::create_project_backup(
            config.inner(),
            settings.inner(),
            io.inner(),
            params.project_path,
            params.backup_name,
            params.exclude_vpm_packages,
            move |progress| {
                update_project_task_progress(&progress_tasks, &progress_task_id, progress);
            },
        )
        .await;

        project_backup_for_task.finish();
        match result {
            Ok(backup_path) => {
                let tool_call = tasks_for_task.lock().unwrap().finish_success(
                    &task_id_for_task,
                    json!({
                        "ok": true,
                        "backupPath": backup_path.display().to_string(),
                    }),
                );
                if let Some(tool_call) = tool_call {
                    finish_tracked_mcp_tool_call_activity(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Finished,
                        None,
                    );
                    emit_tracked_mcp_tool_call_event(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Finished,
                    );
                }
            }
            Err(error) => {
                let error = McpIpcError::from_rust_error("project_backup_error", error);
                let error_message = error.message.clone();
                let tool_call = tasks_for_task
                    .lock()
                    .unwrap()
                    .finish_error(&task_id_for_task, error);
                if let Some(tool_call) = tool_call {
                    finish_tracked_mcp_tool_call_activity(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Failed,
                        Some(error_message),
                    );
                    emit_tracked_mcp_tool_call_event(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Failed,
                    );
                }
            }
        }
    });

    let abort = handle.abort_handle();
    project_backup.start(abort.clone());
    tasks
        .lock()
        .unwrap()
        .set_cancel_handle(&task_id, McpProjectTaskCancelHandle::AbortTask(abort));
    if start_tx.send(()).is_err() {
        project_backup.finish();
        if let Some(tool_call) = tasks.lock().unwrap().cancel_if_working(&task_id) {
            cancel_tracked_mcp_tool_call_activity(&app, &tool_call);
            emit_tracked_mcp_tool_call_event(&app, &tool_call, McpToolCallPhase::Failed);
        }
    }

    task_snapshot_value(tasks.lock().unwrap().get(&task_id).unwrap_or(snapshot))
}

async fn start_copy_project_task(
    app: AppHandle,
    task_id: String,
    params: Value,
    tool_call: Option<McpTrackedToolCall>,
) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<CopyProjectParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let io = app.state::<DefaultEnvironmentIo>();
    ensure_registered_project(io.inner(), &params.source_project_path).await?;

    let project_copy = app.state::<ProjectCopyState>().inner().clone();
    if !project_copy.try_start_uncancellable() {
        return Err(McpIpcError::new(
            "project_copy_already_running",
            "A project copy is already running",
        ));
    }

    let snapshot = start_project_task_record(&app, &task_id, McpProjectTaskKind::Copy, tool_call)?;
    let tasks = app.state::<McpState>().project_tasks.clone();
    let (start_tx, start_rx) = oneshot::channel();
    let app_for_task = app.clone();
    let task_id_for_task = task_id.clone();
    let tasks_for_task = tasks.clone();
    let project_copy_for_task = project_copy.clone();
    let handle = tokio::spawn(async move {
        if start_rx.await.is_err() {
            project_copy_for_task.finish();
            let tool_call = tasks_for_task
                .lock()
                .unwrap()
                .cancel_if_working(&task_id_for_task);
            if let Some(tool_call) = tool_call {
                cancel_tracked_mcp_tool_call_activity(&app_for_task, &tool_call);
                emit_tracked_mcp_tool_call_event(
                    &app_for_task,
                    &tool_call,
                    McpToolCallPhase::Failed,
                );
            }
            return;
        }

        let settings = app_for_task.state::<SettingsState>();
        let io = app_for_task.state::<DefaultEnvironmentIo>();
        let progress_tasks = tasks_for_task.clone();
        let progress_task_id = task_id_for_task.clone();
        let result = project_operations::copy_registered_project(
            settings.inner(),
            io.inner(),
            params.source_project_path,
            params.new_project_path,
            move |progress| {
                update_project_task_progress(&progress_tasks, &progress_task_id, progress);
            },
        )
        .await;

        project_copy_for_task.finish();
        match result {
            Ok(project_path) => {
                let tool_call = tasks_for_task.lock().unwrap().finish_success(
                    &task_id_for_task,
                    json!({
                        "ok": true,
                        "projectPath": project_path,
                    }),
                );
                if let Some(tool_call) = tool_call {
                    finish_tracked_mcp_tool_call_activity(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Finished,
                        None,
                    );
                    emit_tracked_mcp_tool_call_event(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Finished,
                    );
                }
            }
            Err(error) => {
                let error = McpIpcError::from_rust_error("project_copy_error", error);
                let error_message = error.message.clone();
                let tool_call = tasks_for_task
                    .lock()
                    .unwrap()
                    .finish_error(&task_id_for_task, error);
                if let Some(tool_call) = tool_call {
                    finish_tracked_mcp_tool_call_activity(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Failed,
                        Some(error_message),
                    );
                    emit_tracked_mcp_tool_call_event(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Failed,
                    );
                }
            }
        }
    });

    let abort = handle.abort_handle();
    project_copy.start(abort.clone());
    tasks
        .lock()
        .unwrap()
        .set_cancel_handle(&task_id, McpProjectTaskCancelHandle::AbortTask(abort));
    if start_tx.send(()).is_err() {
        project_copy.finish();
        if let Some(tool_call) = tasks.lock().unwrap().cancel_if_working(&task_id) {
            cancel_tracked_mcp_tool_call_activity(&app, &tool_call);
            emit_tracked_mcp_tool_call_event(&app, &tool_call, McpToolCallPhase::Failed);
        }
    }

    task_snapshot_value(tasks.lock().unwrap().get(&task_id).unwrap_or(snapshot))
}

async fn start_restore_project_task(
    app: AppHandle,
    task_id: String,
    params: Value,
    tool_call: Option<McpTrackedToolCall>,
) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<RestoreProjectFromBackupParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;

    let project_restore = app.state::<ProjectRestoreState>().inner().clone();
    if !project_restore.try_start_uncancellable() {
        return Err(McpIpcError::new(
            "project_restore_already_running",
            "A project restore is already running",
        ));
    }

    let snapshot =
        start_project_task_record(&app, &task_id, McpProjectTaskKind::Restore, tool_call)?;
    let tasks = app.state::<McpState>().project_tasks.clone();
    let (start_tx, start_rx) = oneshot::channel();
    let app_for_task = app.clone();
    let task_id_for_task = task_id.clone();
    let tasks_for_task = tasks.clone();
    let project_restore_for_task = project_restore.clone();
    let handle = tokio::spawn(async move {
        if start_rx.await.is_err() {
            project_restore_for_task.finish();
            let tool_call = tasks_for_task
                .lock()
                .unwrap()
                .cancel_if_working(&task_id_for_task);
            if let Some(tool_call) = tool_call {
                cancel_tracked_mcp_tool_call_activity(&app_for_task, &tool_call);
                emit_tracked_mcp_tool_call_event(
                    &app_for_task,
                    &tool_call,
                    McpToolCallPhase::Failed,
                );
            }
            return;
        }

        let settings = app_for_task.state::<SettingsState>();
        let io = app_for_task.state::<DefaultEnvironmentIo>();
        let progress_tasks = tasks_for_task.clone();
        let progress_task_id = task_id_for_task.clone();
        let result = project_operations::restore_project_from_backup(
            settings.inner(),
            io.inner(),
            params.backup_path,
            params.project_name,
            move |progress| {
                update_project_task_progress(&progress_tasks, &progress_task_id, progress);
            },
        )
        .await;

        project_restore_for_task.finish();
        match result {
            Ok(project_path) => {
                let tool_call = tasks_for_task.lock().unwrap().finish_success(
                    &task_id_for_task,
                    json!({
                        "ok": true,
                        "projectPath": project_path,
                    }),
                );
                if let Some(tool_call) = tool_call {
                    finish_tracked_mcp_tool_call_activity(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Finished,
                        None,
                    );
                    emit_tracked_mcp_tool_call_event(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Finished,
                    );
                }
            }
            Err(error) => {
                let error = McpIpcError::from_rust_error("project_restore_error", error);
                let error_message = error.message.clone();
                let tool_call = tasks_for_task
                    .lock()
                    .unwrap()
                    .finish_error(&task_id_for_task, error);
                if let Some(tool_call) = tool_call {
                    finish_tracked_mcp_tool_call_activity(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Failed,
                        Some(error_message),
                    );
                    emit_tracked_mcp_tool_call_event(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Failed,
                    );
                }
            }
        }
    });

    let abort = handle.abort_handle();
    project_restore.start(abort.clone());
    tasks
        .lock()
        .unwrap()
        .set_cancel_handle(&task_id, McpProjectTaskCancelHandle::AbortTask(abort));
    if start_tx.send(()).is_err() {
        project_restore.finish();
        if let Some(tool_call) = tasks.lock().unwrap().cancel_if_working(&task_id) {
            cancel_tracked_mcp_tool_call_activity(&app, &tool_call);
            emit_tracked_mcp_tool_call_event(&app, &tool_call, McpToolCallPhase::Failed);
        }
    }

    task_snapshot_value(tasks.lock().unwrap().get(&task_id).unwrap_or(snapshot))
}

async fn start_project_package_task(
    app: AppHandle,
    task_id: String,
    kind: McpProjectTaskKind,
    method: String,
    params: Value,
    tool_call: Option<McpTrackedToolCall>,
) -> Result<Value, McpIpcError> {
    let package_abort = AbortCheck::new();
    let project_apply = app.state::<ProjectApplyState>();
    if !project_apply.try_start(package_abort.clone()) {
        return Err(McpIpcError::new(
            "project_package_apply_error",
            "project changes already applying",
        ));
    }

    let snapshot = match start_project_task_record(&app, &task_id, kind, tool_call) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            project_apply.finish();
            return Err(error);
        }
    };
    let tasks = app.state::<McpState>().project_tasks.clone();
    let app_for_task = app.clone();
    let task_id_for_task = task_id.clone();
    let tasks_for_task = tasks.clone();
    let package_abort_for_task = package_abort.clone();
    let _task = tokio::spawn(async move {
        let result = match method.as_str() {
            "install_project_package" => {
                install_project_package_with_abort(
                    app_for_task.clone(),
                    params,
                    Some(package_abort_for_task.clone()),
                )
                .await
            }
            "uninstall_project_package" => {
                uninstall_project_package_with_abort(
                    app_for_task.clone(),
                    params,
                    Some(package_abort_for_task.clone()),
                )
                .await
            }
            "reinstall_project_package" => {
                reinstall_project_package_with_abort(
                    app_for_task.clone(),
                    params,
                    Some(package_abort_for_task.clone()),
                )
                .await
            }
            method => Err(McpIpcError::new(
                "unsupported_project_task_method",
                format!("MCP project task method is not supported: {method}"),
            )),
        };
        app_for_task.state::<ProjectApplyState>().finish();

        match result {
            Ok(result) => {
                let tool_call = tasks_for_task
                    .lock()
                    .unwrap()
                    .finish_success(&task_id_for_task, result);
                if let Some(tool_call) = tool_call {
                    finish_tracked_mcp_tool_call_activity(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Finished,
                        None,
                    );
                    emit_tracked_mcp_tool_call_event(
                        &app_for_task,
                        &tool_call,
                        McpToolCallPhase::Finished,
                    );
                }
            }
            Err(error) => {
                let cancel_requested = tasks_for_task
                    .lock()
                    .unwrap()
                    .cancel_requested(&task_id_for_task);
                if cancel_requested && is_project_package_apply_abort_error(&error) {
                    let tool_call = tasks_for_task
                        .lock()
                        .unwrap()
                        .finish_cancelled(&task_id_for_task);
                    if let Some(tool_call) = tool_call {
                        cancel_tracked_mcp_tool_call_activity(&app_for_task, &tool_call);
                        emit_tracked_mcp_tool_call_event(
                            &app_for_task,
                            &tool_call,
                            McpToolCallPhase::Failed,
                        );
                    }
                } else {
                    let error_message = error.message.clone();
                    let tool_call = tasks_for_task
                        .lock()
                        .unwrap()
                        .finish_error(&task_id_for_task, error);
                    if let Some(tool_call) = tool_call {
                        finish_tracked_mcp_tool_call_activity(
                            &app_for_task,
                            &tool_call,
                            McpToolCallPhase::Failed,
                            Some(error_message),
                        );
                        emit_tracked_mcp_tool_call_event(
                            &app_for_task,
                            &tool_call,
                            McpToolCallPhase::Failed,
                        );
                    }
                }
            }
        }
    });

    tasks.lock().unwrap().set_cancel_handle(
        &task_id,
        McpProjectTaskCancelHandle::AbortPackageApply(package_abort),
    );

    task_snapshot_value(tasks.lock().unwrap().get(&task_id).unwrap_or(snapshot))
}

fn start_project_task_record(
    app: &AppHandle,
    task_id: &str,
    kind: McpProjectTaskKind,
    tool_call: Option<McpTrackedToolCall>,
) -> Result<McpProjectTaskSnapshot, McpIpcError> {
    let mcp = app.state::<McpState>();
    Ok(mcp
        .project_tasks
        .lock()
        .unwrap()
        .start(task_id.to_string(), kind, tool_call))
}

fn task_snapshot_value(snapshot: McpProjectTaskSnapshot) -> Result<Value, McpIpcError> {
    serde_json::to_value(snapshot)
        .map_err(|e| McpIpcError::from_error("project_task_serialization_error", e))
}

fn is_project_package_apply_abort_error(error: &McpIpcError) -> bool {
    error.code == "project_package_apply_error" && error.message == "Aborted"
}

fn is_project_create_abort_error(error: &McpIpcError) -> bool {
    error.code == "project_create_error" && error.message == "Aborted"
}

fn update_project_task_progress<P: Serialize>(
    tasks: &Arc<StdMutex<McpProjectTaskStore>>,
    task_id: &str,
    progress: P,
) {
    let Ok(value) = serde_json::to_value(progress) else {
        return;
    };
    let Some(progress) = project_progress_from_value(&value) else {
        return;
    };

    tasks.lock().unwrap().update_progress(task_id, progress);
}

fn project_progress_from_value(value: &Value) -> Option<McpProjectProgress> {
    let object = value.as_object()?;
    let total = object.get("total")?.as_u64()? as usize;
    let proceed = object.get("proceed")?.as_u64()? as usize;
    let last_proceed = object
        .get("lastProceed")
        .or_else(|| object.get("last_proceed"))?
        .as_str()?
        .to_string();

    Some(McpProjectProgress {
        total,
        proceed,
        last_proceed,
    })
}

fn finish_project_task_kind(app: &AppHandle, kind: McpProjectTaskKind) {
    match kind {
        McpProjectTaskKind::Backup => app.state::<ProjectBackupState>().finish(),
        McpProjectTaskKind::Copy => app.state::<ProjectCopyState>().finish(),
        McpProjectTaskKind::Restore => app.state::<ProjectRestoreState>().finish(),
        McpProjectTaskKind::Create
        | McpProjectTaskKind::InstallPackage
        | McpProjectTaskKind::UninstallPackage
        | McpProjectTaskKind::ReinstallPackage => {}
    }
}

async fn ensure_registered_project(
    io: &DefaultEnvironmentIo,
    project_path: &str,
) -> Result<(), McpIpcError> {
    let connection = VccDatabaseConnection::connect(io)
        .await
        .map_err(|e| McpIpcError::from_error("project_database_error", e))?;

    if connection
        .find_project(project_path)
        .map_err(|e| McpIpcError::from_error("project_database_error", e))?
        .is_some()
    {
        Ok(())
    } else {
        Err(McpIpcError::new(
            "project_not_registered",
            "project_path must match an ALCOMD3 registered project",
        ))
    }
}

async fn list_repositories(app: AppHandle) -> Result<Value, McpIpcError> {
    let io = app.state::<DefaultEnvironmentIo>();
    let settings = app.state::<SettingsState>();
    let config = app.state::<GuiConfigState>();
    let settings = settings
        .load(io.inner())
        .await
        .map_err(|e| McpIpcError::from_error("settings_load_error", e))?;
    let config = config.get();

    let user_repositories = settings
        .get_user_repos()
        .iter()
        .enumerate()
        .map(|(index, repo)| user_repository_summary(index, repo))
        .collect::<Vec<_>>();
    let mut repositories = Vec::new();
    if !settings.ignore_official_repository() {
        repositories.push(default_repository_summary(
            OFFICIAL_REPOSITORY_ID,
            OFFICIAL_URL_STR,
            "officialDefault",
        ));
    }
    if !settings.ignore_curated_repository() {
        repositories.push(default_repository_summary(
            CURATED_REPOSITORY_ID,
            CURATED_URL_STR,
            "curatedDefault",
        ));
    }
    repositories.extend(user_repositories.iter().cloned());

    Ok(json!({
        "ok": true,
        "repositories": repositories,
        "userRepositories": user_repositories,
        "hiddenUserRepositories": config.gui_hidden_repositories.iter().collect::<Vec<_>>(),
        "hideLocalUserPackages": config.hide_local_user_packages,
        "showPrereleasePackages": settings.show_prerelease_packages(),
    }))
}

async fn add_repository(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<AddRepositoryParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let url = params
        .repository_url
        .parse::<url::Url>()
        .map_err(|_| McpIpcError::new("invalid_params", "repository_url must be a valid URL"))?;
    let headers = params
        .headers
        .into_iter()
        .map(|(key, value)| (key.into_boxed_str(), value.into_boxed_str()))
        .collect::<IndexMap<_, _>>();
    let repository = repository_operations::add_repository(
        app.state::<SettingsState>().inner(),
        app.state::<PackagesState>().inner(),
        app.state::<DefaultEnvironmentIo>().inner(),
        app.state::<reqwest::Client>().inner(),
        url,
        headers,
    )
    .await
    .map_err(|e| McpIpcError::from_rust_error("repository_add_error", e))?;

    Ok(json!({
        "ok": true,
        "repository": {
            "index": repository.index,
            "id": repository.id,
            "url": repository.url,
            "displayName": repository.display_name,
            "kind": "user",
            "isDefaultRepository": false,
            "isUserRepository": true,
        },
    }))
}

fn default_repository_summary(id: &str, url: &str, kind: &str) -> Value {
    json!({
        "id": id,
        "url": url,
        "displayName": id,
        "kind": kind,
        "isDefaultRepository": true,
        "isUserRepository": false,
    })
}

fn user_repository_summary(index: usize, repo: &UserRepoSetting) -> Value {
    let id = repo
        .id()
        .or(repo.url().map(url::Url::as_str))
        .map(str::to_string);
    json!({
        "index": index,
        "id": id,
        "url": repo.url().map(ToString::to_string),
        "displayName": repo.name().map(str::to_string).or_else(|| id.clone()),
        "kind": "user",
        "isDefaultRepository": false,
        "isUserRepository": true,
    })
}

async fn list_packages(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = parse_package_list_params(params)?;
    let pagination = PackageListPagination::from_params(params);
    let io = app.state::<DefaultEnvironmentIo>();
    let settings = app.state::<SettingsState>();
    let packages = app.state::<PackagesState>();
    let http = app.state::<reqwest::Client>();

    let settings = settings
        .load(io.inner())
        .await
        .map_err(|e| McpIpcError::from_error("settings_load_error", e))?;
    let show_prerelease_packages = settings.show_prerelease_packages();
    let packages = packages
        .load(&settings, io.inner(), http.inner(), app.clone())
        .await
        .map_err(|e| McpIpcError::from_error("packages_load_error", e))?;
    let config = app.state::<GuiConfigState>().get();

    let results = packages
        .packages()
        .filter(|package| {
            package_is_visible_with_gui_filters(
                package,
                &config.gui_hidden_repositories,
                config.hide_local_user_packages,
                show_prerelease_packages,
            )
        })
        .collect::<Vec<_>>();
    let results = package_info_list_summaries(results);

    Ok(package_list_response(results, pagination))
}

#[derive(Debug, Default, Clone, Copy, Deserialize)]
struct PackageListParams {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PackageListPagination {
    offset: usize,
    limit: usize,
}

impl PackageListPagination {
    fn from_params(params: PackageListParams) -> Self {
        let limit = params
            .limit
            .unwrap_or(MCP_PACKAGE_LIST_DEFAULT_LIMIT)
            .clamp(1, MCP_PACKAGE_LIST_MAX_LIMIT);

        Self {
            offset: params.offset.unwrap_or(0),
            limit,
        }
    }
}

#[derive(Deserialize)]
struct RepositoryPackagesParams {
    repository_id: Option<String>,
    repository_url: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct PackageDetailsParams {
    package_name: String,
    version: Option<String>,
    repository_id: Option<String>,
    repository_url: Option<String>,
}

#[derive(Debug, Clone)]
struct RepositorySelector {
    repository_id: Option<String>,
    repository_url: Option<String>,
}

impl RepositorySelector {
    fn from_params(params: RepositoryPackagesParams) -> Result<Self, McpIpcError> {
        Self::from_values(params.repository_id, params.repository_url)
    }

    fn from_values(
        repository_id: Option<String>,
        repository_url: Option<String>,
    ) -> Result<Self, McpIpcError> {
        let repository_id = normalize_optional_string(repository_id);
        let repository_url = normalize_optional_string(repository_url);

        if repository_id.is_none() && repository_url.is_none() {
            return Err(McpIpcError::new(
                "invalid_params",
                "repository_id or repository_url must be provided",
            ));
        }

        Ok(Self {
            repository_id,
            repository_url,
        })
    }

    fn matches_repo(&self, repo: &LocalCachedRepository) -> bool {
        let id_matches = self
            .repository_id
            .as_deref()
            .is_some_and(|expected| repository_id(repo) == Some(expected));
        let url_matches = self.repository_url.as_deref().is_some_and(|expected| {
            repo.url()
                .is_some_and(|repository_url| repository_url.as_str() == expected)
        });

        id_matches || url_matches
    }
}

#[derive(Debug, Clone)]
struct PackageDetailsSelector {
    package_name: String,
    version: Option<String>,
    repository: Option<RepositorySelector>,
}

impl PackageDetailsSelector {
    fn from_params(params: PackageDetailsParams) -> Result<Self, McpIpcError> {
        let package_name = params.package_name.trim().to_string();
        if package_name.is_empty() {
            return Err(McpIpcError::new(
                "invalid_params",
                "package_name must be provided",
            ));
        }

        let version = normalize_optional_string(params.version);
        let has_repository = params
            .repository_id
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
            || params
                .repository_url
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty());
        let repository = if has_repository {
            Some(RepositorySelector::from_values(
                params.repository_id,
                params.repository_url,
            )?)
        } else {
            None
        };

        Ok(Self {
            package_name,
            version,
            repository,
        })
    }

    fn matches_package(&self, package: &PackageInfo<'_>) -> bool {
        let manifest = package.package_json();
        if manifest.name() != self.package_name {
            return false;
        }
        if self
            .version
            .as_deref()
            .is_some_and(|version| manifest.version().to_string() != version)
        {
            return false;
        }
        if let Some(repository) = &self.repository {
            return package_is_from_repository(package, repository);
        }

        true
    }
}

fn parse_package_list_params(params: Value) -> Result<PackageListParams, McpIpcError> {
    if params.is_null() {
        return Ok(PackageListParams::default());
    }

    serde_json::from_value::<PackageListParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))
}

fn package_list_response(packages: Vec<Value>, pagination: PackageListPagination) -> Value {
    let total_count = packages.len();
    let start = pagination.offset.min(total_count);
    let end = start.saturating_add(pagination.limit).min(total_count);
    let has_more = end < total_count;
    let page_packages = packages
        .into_iter()
        .skip(start)
        .take(pagination.limit)
        .collect::<Vec<_>>();
    let returned_count = page_packages.len();

    json!({
        "ok": true,
        "totalCount": total_count,
        "offset": pagination.offset,
        "limit": pagination.limit,
        "returnedCount": returned_count,
        "hasMore": has_more,
        "nextOffset": if has_more { json!(end) } else { Value::Null },
        "packages": page_packages,
    })
}

fn package_details_response(packages: Vec<Value>) -> Value {
    json!({
        "ok": true,
        "packages": packages,
    })
}

async fn list_repository_packages(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<RepositoryPackagesParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let pagination = PackageListPagination::from_params(PackageListParams {
        offset: params.offset,
        limit: params.limit,
    });
    let selector = RepositorySelector::from_params(params)?;
    let io = app.state::<DefaultEnvironmentIo>();
    let settings = app.state::<SettingsState>();
    let packages = app.state::<PackagesState>();
    let http = app.state::<reqwest::Client>();

    let settings = settings
        .load(io.inner())
        .await
        .map_err(|e| McpIpcError::from_error("settings_load_error", e))?;
    let show_prerelease_packages = settings.show_prerelease_packages();
    let packages = packages
        .load(&settings, io.inner(), http.inner(), app.clone())
        .await
        .map_err(|e| McpIpcError::from_error("packages_load_error", e))?;
    let config = app.state::<GuiConfigState>().get();
    let Some(repository) = packages
        .collection()
        .get_remote()
        .find(|repo| selector.matches_repo(repo))
    else {
        return Err(McpIpcError::new(
            "repository_not_found",
            "repository_id or repository_url must match an ALCOMD3 remote repository",
        ));
    };
    let repository = repository_summary(repository);

    let results = packages
        .packages()
        .filter(|package| package_is_from_repository(package, &selector))
        .filter(|package| {
            package_is_visible_with_gui_filters(
                package,
                &config.gui_hidden_repositories,
                config.hide_local_user_packages,
                show_prerelease_packages,
            )
        })
        .collect::<Vec<_>>();
    let results = package_info_list_summaries(results);
    let mut response = package_list_response(results, pagination);
    if let Value::Object(ref mut object) = response {
        object.insert("repository".to_string(), repository);
    }

    Ok(response)
}

async fn get_package_details(app: AppHandle, params: Value) -> Result<Value, McpIpcError> {
    let params = serde_json::from_value::<PackageDetailsParams>(params)
        .map_err(|e| McpIpcError::from_error("invalid_params", e))?;
    let selector = PackageDetailsSelector::from_params(params)?;
    let io = app.state::<DefaultEnvironmentIo>();
    let settings = app.state::<SettingsState>();
    let packages = app.state::<PackagesState>();
    let http = app.state::<reqwest::Client>();

    let settings = settings
        .load(io.inner())
        .await
        .map_err(|e| McpIpcError::from_error("settings_load_error", e))?;
    let show_prerelease_packages = settings.show_prerelease_packages();
    let packages = packages
        .load(&settings, io.inner(), http.inner(), app.clone())
        .await
        .map_err(|e| McpIpcError::from_error("packages_load_error", e))?;
    let config = app.state::<GuiConfigState>().get();

    let mut results = packages
        .packages()
        .filter(|package| selector.matches_package(package))
        .filter(|package| {
            package_is_visible_with_gui_filters(
                package,
                &config.gui_hidden_repositories,
                config.hide_local_user_packages,
                show_prerelease_packages,
            )
        })
        .map(package_info_details)
        .collect::<Vec<_>>();

    sort_package_summaries(&mut results);
    if results.is_empty() {
        return Err(McpIpcError::new(
            "package_not_found",
            "package_name must match a GUI-visible ALCOMD3 package",
        ));
    }
    Ok(package_details_response(results))
}

async fn get_environment_settings(app: AppHandle) -> Result<Value, McpIpcError> {
    let io = app.state::<DefaultEnvironmentIo>();
    let settings = app.state::<SettingsState>();
    let config = app.state::<GuiConfigState>();
    let response = environment_settings::load_environment_settings_snapshot(
        io.inner(),
        settings.inner(),
        config.inner(),
        DEFAULT_UNITY_ARGUMENTS,
    )
    .await
    .map_err(|e| McpIpcError::new(e.code(), e.message()))?;
    serde_json::to_value(response).map_err(|e| McpIpcError::from_error("serialization_error", e))
}

fn project_summary(project: &UserProject) -> Option<Value> {
    let snapshot = project_summary_snapshot(project)?;
    Some(json!({
        "name": snapshot.name,
        "path": snapshot.path,
        "projectType": format!("{:?}", snapshot.project_type),
        "unity": snapshot.unity,
        "unityRevision": snapshot.unity_revision,
        "lastModified": snapshot.last_modified,
        "createdAt": snapshot.created_at,
        "favorite": snapshot.favorite,
        "exists": snapshot.exists,
    }))
}

fn repository_summary(repo: &LocalCachedRepository) -> Value {
    let id = repository_id(repo).map(str::to_string);
    let kind = repository_kind(repo);
    let is_default_repository = repository_is_default(kind);
    json!({
        "id": id,
        "url": repo.url().map(ToString::to_string),
        "displayName": repo.name().map(str::to_string).or_else(|| id.clone()),
        "kind": kind,
        "isDefaultRepository": is_default_repository,
        "isUserRepository": !is_default_repository,
    })
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn package_source_summary(package: &PackageInfo<'_>) -> Value {
    if let Some(repo) = package.repo() {
        let id = repository_id(repo);
        let kind = package_source_kind(repo);
        let is_default_repository = repository_is_default(kind);
        json!({
            "type": "remote",
            "kind": kind,
            "id": id,
            "displayName": repo.name().or(id),
            "url": repo.url().map(ToString::to_string),
            "isDefaultRepository": is_default_repository,
            "isUserRepository": !is_default_repository,
        })
    } else {
        json!({
            "type": "localUser",
            "kind": "localUser",
            "isLocalUserPackage": true,
        })
    }
}

fn package_info_summary(package: &PackageInfo<'_>) -> Value {
    let manifest = package.package_json();
    json!({
        "name": manifest.name(),
        "displayName": manifest.display_name(),
        "version": manifest.version().to_string(),
        "source": package_source_summary(package),
    })
}

fn package_info_list_summaries<'package, 'env>(
    packages: impl IntoIterator<Item = &'package PackageInfo<'env>>,
) -> Vec<Value>
where
    'env: 'package,
{
    let mut results = latest_package_infos_by_source(packages)
        .into_iter()
        .map(|package| package_info_summary(package))
        .collect::<Vec<_>>();
    sort_package_summaries(&mut results);
    results
}

fn package_info_details(package: &PackageInfo<'_>) -> Value {
    let manifest = package.package_json();
    let mut value = package_manifest_summary(manifest);
    if let Value::Object(ref mut object) = value {
        object.insert("source".to_string(), package_source_summary(package));
    }
    value
}

fn package_is_from_repository(package: &PackageInfo<'_>, selector: &RepositorySelector) -> bool {
    package
        .repo()
        .is_some_and(|repo| selector.matches_repo(repo))
}

fn sort_package_summaries(results: &mut [Value]) {
    results.sort_by(|a, b| {
        let a_name = a["name"].as_str().unwrap_or_default();
        let b_name = b["name"].as_str().unwrap_or_default();
        let a_version = a["version"].as_str().unwrap_or_default();
        let b_version = b["version"].as_str().unwrap_or_default();
        a_name.cmp(b_name).then_with(|| a_version.cmp(b_version))
    });
}

fn package_manifest_summary(package: &PackageManifest) -> Value {
    json!({
        "name": package.name(),
        "displayName": package.display_name(),
        "description": package.description(),
        "version": package.version().to_string(),
        "unity": package.unity().map(|unity| {
            json!({
                "major": unity.major(),
                "minor": unity.minor(),
            })
        }),
        "keywords": package.keywords(),
        "aliases": package.aliases(),
        "vpmDependencies": package
            .vpm_dependencies()
            .keys()
            .map(|name| name.to_string())
            .collect::<Vec<_>>(),
        "legacyPackages": package.legacy_packages(),
        "changelogUrl": package.changelog_url().map(ToString::to_string),
        "documentationUrl": package.documentation_url().map(ToString::to_string),
        "isYanked": package.is_yanked(),
    })
}

async fn write_endpoint_file(path: &Path, metadata: &EndpointMetadata) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "endpoint file path has no parent",
        ));
    };
    tokio::fs::create_dir_all(parent).await?;
    let bytes = serde_json::to_vec_pretty(metadata).map_err(io::Error::other)?;
    let temp_path = temporary_endpoint_path(path);
    remove_endpoint_file(&temp_path).await?;
    tokio::fs::write(&temp_path, bytes).await?;
    remove_endpoint_file(path).await?;
    tokio::fs::rename(&temp_path, path).await
}

async fn remove_endpoint_file(path: &Path) -> io::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn temporary_endpoint_path(path: &Path) -> PathBuf {
    let mut file_name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "endpoint.json".into());
    file_name.push(format!(".tmp.{}", std::process::id()));
    path.with_file_name(file_name)
}

fn bridge_command() -> String {
    quote_command_path(&bridge_executable_path())
}

fn bridge_executable_path() -> PathBuf {
    let current = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ALCOMD3"));
    let directory = current.parent().unwrap_or_else(|| Path::new("."));
    #[cfg(windows)]
    {
        directory.join("alcomd3-mcp.exe")
    }
    #[cfg(not(windows))]
    {
        directory.join("alcomd3-mcp")
    }
}

fn quote_command_path(path: &Path) -> String {
    let value = path.display().to_string();
    if value.contains([' ', '\t', '"']) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn record_client_activity(
    clients: &mut VecDeque<McpRecentClientStatus>,
    client: &ClientIdentity,
    now_unix_ms: u64,
    last_emit_unix_ms: &mut u64,
) -> bool {
    let removed_stale_clients = retain_recent_client_activity(clients, now_unix_ms);
    let session_id = client.session_id.to_string();
    let existing = clients
        .iter()
        .find(|existing| existing.session_id == session_id);
    let should_emit = existing.is_none_or(|existing| {
        existing.name != client.name
            || existing.version != client.version
            || now_unix_ms.saturating_sub(*last_emit_unix_ms) >= MCP_CLIENT_STATUS_EMIT_THROTTLE_MS
    }) || removed_stale_clients;

    clients.retain(|existing| existing.session_id != session_id);
    clients.push_front(McpRecentClientStatus {
        session_id,
        name: client.name.clone(),
        version: client.version.clone(),
        last_seen_unix_ms: now_unix_ms,
    });
    while clients.len() > MAX_RECORDED_MCP_RECENT_CLIENTS {
        clients.pop_back();
    }

    if should_emit {
        *last_emit_unix_ms = now_unix_ms;
    }
    should_emit
}

fn retain_recent_client_activity(
    clients: &mut VecDeque<McpRecentClientStatus>,
    now_unix_ms: u64,
) -> bool {
    let before_len = clients.len();
    clients.retain(|client| is_recent_client_activity(client.last_seen_unix_ms, now_unix_ms));
    clients.len() != before_len
}

fn is_recent_client_activity(last_seen_unix_ms: u64, now_unix_ms: u64) -> bool {
    now_unix_ms.saturating_sub(last_seen_unix_ms) <= MCP_CLIENT_ACTIVITY_TTL_MS
}

fn mcp_disabled_response(request_id: Uuid) -> IpcResponse {
    IpcResponse::error(request_id, "mcp_disabled", MCP_DISABLED_MESSAGE, None)
}

#[allow(dead_code)]
fn ok_result(value: Value) -> Value {
    json!({
        "ok": true,
        "result": value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::packages::package_manifest_is_available_for_display;
    use indexmap::{IndexMap, IndexSet};
    use vrc_get_vpm::repository::RemoteRepository;

    #[test]
    fn record_client_activity_emits_for_new_client() {
        let mut clients = VecDeque::new();
        let mut last_emit_unix_ms = 0;
        let client = test_client("Codex", Some("1.0.0"));

        assert!(record_client_activity(
            &mut clients,
            &client,
            1_000,
            &mut last_emit_unix_ms
        ));
        assert_eq!(last_emit_unix_ms, 1_000);
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].name, "Codex");
        assert_eq!(clients[0].version.as_deref(), Some("1.0.0"));
        assert_eq!(clients[0].last_seen_unix_ms, 1_000);
    }

    #[test]
    fn record_client_activity_throttles_last_seen_only_updates() {
        let mut clients = VecDeque::new();
        let mut last_emit_unix_ms = 0;
        let client = test_client("Codex", Some("1.0.0"));

        assert!(record_client_activity(
            &mut clients,
            &client,
            1_000,
            &mut last_emit_unix_ms
        ));

        let before_throttle = 1_000 + MCP_CLIENT_STATUS_EMIT_THROTTLE_MS - 1;
        assert!(!record_client_activity(
            &mut clients,
            &client,
            before_throttle,
            &mut last_emit_unix_ms
        ));
        assert_eq!(last_emit_unix_ms, 1_000);
        assert_eq!(clients[0].last_seen_unix_ms, before_throttle);

        let after_throttle = 1_000 + MCP_CLIENT_STATUS_EMIT_THROTTLE_MS;
        assert!(record_client_activity(
            &mut clients,
            &client,
            after_throttle,
            &mut last_emit_unix_ms
        ));
        assert_eq!(last_emit_unix_ms, after_throttle);
        assert_eq!(clients[0].last_seen_unix_ms, after_throttle);
    }

    #[test]
    fn record_client_activity_emits_when_client_metadata_changes() {
        let mut clients = VecDeque::new();
        let mut last_emit_unix_ms = 0;
        let mut client = test_client("Codex", Some("1.0.0"));

        assert!(record_client_activity(
            &mut clients,
            &client,
            1_000,
            &mut last_emit_unix_ms
        ));

        client.version = Some("1.0.1".to_string());
        assert!(record_client_activity(
            &mut clients,
            &client,
            1_001,
            &mut last_emit_unix_ms
        ));
        assert_eq!(last_emit_unix_ms, 1_001);
        assert_eq!(clients[0].version.as_deref(), Some("1.0.1"));
    }

    #[test]
    fn record_client_activity_prunes_stale_clients() {
        let mut clients = VecDeque::new();
        let mut last_emit_unix_ms = 1_000;
        let stale_client = test_client("Stale", Some("1.0.0"));
        let fresh_client = test_client("Fresh", Some("1.0.0"));
        let next_client = test_client("Codex", Some("1.0.0"));

        clients.push_back(McpRecentClientStatus {
            session_id: stale_client.session_id.to_string(),
            name: stale_client.name,
            version: stale_client.version,
            last_seen_unix_ms: 1_000,
        });
        clients.push_back(McpRecentClientStatus {
            session_id: fresh_client.session_id.to_string(),
            name: fresh_client.name,
            version: fresh_client.version,
            last_seen_unix_ms: 1_000 + MCP_CLIENT_ACTIVITY_TTL_MS,
        });

        assert!(record_client_activity(
            &mut clients,
            &next_client,
            1_000 + MCP_CLIENT_ACTIVITY_TTL_MS + 1,
            &mut last_emit_unix_ms
        ));

        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].name, "Codex");
        assert_eq!(clients[1].name, "Fresh");
    }

    #[test]
    fn recent_client_activity_expires_after_ttl() {
        assert!(is_recent_client_activity(
            1_000,
            1_000 + MCP_CLIENT_ACTIVITY_TTL_MS
        ));
        assert!(!is_recent_client_activity(
            1_000,
            1_000 + MCP_CLIENT_ACTIVITY_TTL_MS + 1
        ));
    }

    #[test]
    fn bounded_line_rejects_oversized_lines() {
        tauri::async_runtime::block_on(async {
            let max_line_bytes = 32;
            let mut input = vec![b'a'; max_line_bytes + 1];
            input.push(b'\n');
            let mut reader = BufReader::new(input.as_slice());

            let error = read_bounded_line_with_limit(&mut reader, "test line", max_line_bytes)
                .await
                .unwrap_err();

            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        });
    }

    #[test]
    fn mcp_tool_name_maps_gui_methods_to_public_tool_names() {
        assert_eq!(
            mcp_tool_name("list_projects", &Value::Null),
            Some("alcomd3_list_projects")
        );
        assert_eq!(
            mcp_tool_name("get_project_details", &Value::Null),
            Some("alcomd3_get_project_details")
        );
        assert_eq!(
            mcp_tool_name("get_package_details", &Value::Null),
            Some("alcomd3_get_package_details")
        );
        assert_eq!(
            mcp_tool_name("list_repositories", &Value::Null),
            Some("alcomd3_list_repositories")
        );
        assert_eq!(
            mcp_tool_name("add_repository", &Value::Null),
            Some("alcomd3_add_repository")
        );
        assert_eq!(
            mcp_tool_name("list_packages", &Value::Null),
            Some("alcomd3_list_packages")
        );
        assert_eq!(
            mcp_tool_name("list_repository_packages", &Value::Null),
            Some("alcomd3_list_repository_packages")
        );
        assert_eq!(
            mcp_tool_name("get_environment_settings", &Value::Null),
            Some("alcomd3_get_environment_settings")
        );
        assert_eq!(
            mcp_tool_name("search_activity_logs", &Value::Null),
            Some("alcomd3_search_activity_logs")
        );
        assert_eq!(
            mcp_tool_name("get_activity_log_entry", &Value::Null),
            Some("alcomd3_get_activity_log_entry")
        );
        assert_eq!(
            mcp_tool_name("summarize_activity_logs", &Value::Null),
            Some("alcomd3_summarize_activity_logs")
        );
        assert_eq!(
            mcp_tool_name("get_activity_log_context", &Value::Null),
            Some("alcomd3_get_activity_log_context")
        );
        assert_eq!(
            mcp_tool_name("search_technical_logs", &Value::Null),
            Some("alcomd3_search_technical_logs")
        );
        assert_eq!(
            mcp_tool_name("get_technical_log_entry", &Value::Null),
            Some("alcomd3_get_technical_log_entry")
        );
        assert_eq!(
            mcp_tool_name("summarize_technical_logs", &Value::Null),
            Some("alcomd3_summarize_technical_logs")
        );
        assert_eq!(
            mcp_tool_name("create_project", &Value::Null),
            Some("alcomd3_create_project")
        );
        assert_eq!(
            mcp_tool_name("add_existing_project", &Value::Null),
            Some("alcomd3_add_existing_project")
        );
        assert_eq!(
            mcp_tool_name("backup_project", &Value::Null),
            Some("alcomd3_backup_project")
        );
        assert_eq!(
            mcp_tool_name("copy_project", &Value::Null),
            Some("alcomd3_copy_project")
        );
        assert_eq!(
            mcp_tool_name("restore_project_from_backup", &Value::Null),
            Some("alcomd3_restore_project_from_backup")
        );
        assert_eq!(
            mcp_tool_name("install_project_package", &Value::Null),
            Some("alcomd3_install_project_package")
        );
        assert_eq!(
            mcp_tool_name("uninstall_project_package", &Value::Null),
            Some("alcomd3_uninstall_project_package")
        );
        assert_eq!(
            mcp_tool_name("reinstall_project_package", &Value::Null),
            Some("alcomd3_reinstall_project_package")
        );
        assert_eq!(
            mcp_tool_name("search_packages", &Value::Null),
            Some("alcomd3_list_packages")
        );
        assert_eq!(mcp_tool_name("unknown", &Value::Null), None);
    }

    #[test]
    fn mcp_read_tools_are_secondary_activity_importance() {
        assert_eq!(
            mcp_activity_importance("search_activity_logs", &Value::Null),
            ActivityImportance::Secondary
        );
        assert_eq!(
            mcp_activity_importance("list_projects", &Value::Null),
            ActivityImportance::Secondary
        );
        assert_eq!(
            mcp_activity_importance("create_project", &Value::Null),
            ActivityImportance::Primary
        );
        assert_eq!(
            mcp_activity_importance("add_existing_project", &Value::Null),
            ActivityImportance::Primary
        );
        assert_eq!(
            mcp_activity_importance("add_repository", &Value::Null),
            ActivityImportance::Primary
        );
        assert_eq!(
            mcp_activity_importance("backup_project", &Value::Null),
            ActivityImportance::Primary
        );
        assert_eq!(
            mcp_activity_importance("install_project_package", &Value::Null),
            ActivityImportance::Primary
        );
    }

    #[test]
    fn mcp_capability_matrix_covers_public_tool_names() {
        let mut tool_names = std::collections::HashSet::new();
        let mut methods = std::collections::HashSet::new();

        for capability in crate::backend::mcp_capabilities::MCP_TOOL_CAPABILITIES {
            assert!(
                tool_names.insert(capability.tool_name),
                "duplicate MCP tool capability for {}",
                capability.tool_name
            );
            assert!(
                methods.insert(capability.ipc_method),
                "duplicate MCP method capability for {}",
                capability.ipc_method
            );
            assert!(
                !capability.gui_capability.trim().is_empty(),
                "MCP tool {} must map to a GUI capability",
                capability.tool_name
            );
            if capability.read_only {
                assert!(
                    !capability.destructive,
                    "read-only MCP tool {} must not be destructive",
                    capability.tool_name
                );
            }
            assert_eq!(
                mcp_tool_name(capability.ipc_method, &Value::Null),
                Some(capability.tool_name)
            );
            assert_eq!(
                crate::backend::mcp_capabilities::mcp_tool_capability_for_tool_name(
                    capability.tool_name
                ),
                Some(capability)
            );
        }

        assert_eq!(tool_names.len(), 23);
        assert_eq!(methods.len(), 23);
        assert!(
            crate::backend::mcp_capabilities::mcp_tool_capability_for_tool_name(
                "alcomd3_uninstall_project_package"
            )
            .is_some_and(|capability| capability.destructive)
        );
        assert_eq!(
            mcp_tool_name("search_packages", &Value::Null),
            Some("alcomd3_list_packages")
        );
    }

    #[test]
    fn mcp_task_start_tool_names_map_inner_project_methods() {
        assert_eq!(
            mcp_tool_name(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({ "method": "create_project" }),
            ),
            Some("alcomd3_create_project")
        );
        assert_eq!(
            mcp_tool_name(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({ "method": "backup_project" }),
            ),
            Some("alcomd3_backup_project")
        );
        assert_eq!(
            mcp_tool_name(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({ "method": "copy_project" }),
            ),
            Some("alcomd3_copy_project")
        );
        assert_eq!(
            mcp_tool_name(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({ "method": "restore_project_from_backup" }),
            ),
            Some("alcomd3_restore_project_from_backup")
        );
        assert_eq!(
            mcp_tool_name(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({ "method": "install_project_package" }),
            ),
            Some("alcomd3_install_project_package")
        );
        assert_eq!(
            mcp_tool_name(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({ "method": "uninstall_project_package" }),
            ),
            Some("alcomd3_uninstall_project_package")
        );
        assert_eq!(
            mcp_tool_name(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({ "method": "reinstall_project_package" }),
            ),
            Some("alcomd3_reinstall_project_package")
        );
        assert_eq!(
            mcp_tool_name(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({ "method": "list_projects" }),
            ),
            None
        );
        assert_eq!(
            mcp_tool_name(IPC_METHOD_PROJECT_TASK_START, &json!({})),
            None
        );
    }

    #[test]
    fn mcp_task_start_activity_target_uses_inner_params() {
        assert_eq!(
            mcp_activity_target(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({
                    "method": "create_project",
                    "params": {
                        "project_name": "New World",
                        "base_path": "C:/Users/test/Projects"
                    }
                }),
            ),
            Some("New World".to_string())
        );
        assert_eq!(
            mcp_activity_target(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({
                    "method": "backup_project",
                    "params": {
                        "project_path": "C:/Users/test/World"
                    }
                }),
            ),
            Some("World".to_string())
        );
        assert_eq!(
            mcp_activity_target(
                IPC_METHOD_PROJECT_TASK_START,
                &json!({
                    "method": "copy_project",
                    "params": {
                        "source_project_path": "C:/Users/test/Source",
                        "new_project_path": "C:/Users/test/Copy"
                    }
                }),
            ),
            Some("Source".to_string())
        );
    }

    #[test]
    fn mcp_repository_activity_target_and_details_sanitize_sensitive_values() {
        assert_eq!(
            mcp_activity_target(
                "add_repository",
                &json!({
                    "repository_url": "https://user:pass@example.com/index.json?token=secret",
                    "headers": {
                        "Authorization": "Bearer secret",
                    },
                }),
            ),
            Some("example.com".to_string())
        );

        let details = mcp_activity_details(
            "add_repository",
            &json!({
                "repository_url": "https://user:pass@example.com/index.json?token=secret",
                "headers": {
                    "Authorization": "Bearer secret",
                    "X-Token": "secret",
                },
            }),
        );

        assert!(details.contains(&ActivityDetail::new(
            "repository_url",
            "https://example.com/index.json",
        )));
        assert!(details.contains(&ActivityDetail::new("headers", "2 headers")));
        assert!(!serde_json::to_string(&details).unwrap().contains("secret"));
    }

    #[test]
    fn internal_project_task_polling_is_not_tracked_as_tool_call() {
        assert!(
            mcp_tool_call_for_request(IPC_METHOD_PROJECT_TASK_GET, &Value::Null, Uuid::new_v4())
                .is_none()
        );
        assert!(
            mcp_tool_call_for_request(IPC_METHOD_PROJECT_TASK_LIST, &Value::Null, Uuid::new_v4())
                .is_none()
        );
        assert!(
            mcp_tool_call_for_request(IPC_METHOD_PROJECT_TASK_CANCEL, &Value::Null, Uuid::new_v4())
                .is_some()
        );
    }

    #[test]
    fn project_task_start_tool_call_finishes_on_task_terminal() {
        let ok: Result<Value, McpIpcError> = Ok(json!({ "taskId": "task-1" }));
        let error: Result<Value, McpIpcError> =
            Err(McpIpcError::new("invalid_params", "invalid params"));

        assert!(!mcp_tool_call_finishes_with_response(
            IPC_METHOD_PROJECT_TASK_START,
            &ok
        ));
        assert!(mcp_tool_call_finishes_with_response(
            IPC_METHOD_PROJECT_TASK_START,
            &error
        ));
        assert!(mcp_tool_call_finishes_with_response("backup_project", &ok));
    }

    #[test]
    fn project_task_terminal_state_takes_tracked_tool_call_once() {
        let request_id = Uuid::new_v4();
        let mut tasks = McpProjectTaskStore::default();
        tasks.start(
            "task-1".to_string(),
            McpProjectTaskKind::Backup,
            Some(McpTrackedToolCall {
                request_id,
                tool_name: "alcomd3_backup_project".to_string(),
                activity: None,
            }),
        );

        let tool_call = tasks
            .finish_success("task-1", json!({ "ok": true }))
            .unwrap();

        assert_eq!(tool_call.request_id, request_id);
        assert_eq!(tool_call.tool_name, "alcomd3_backup_project");
        assert!(
            tasks
                .finish_error("task-1", McpIpcError::new("failed", "failed"))
                .is_none()
        );
    }

    #[test]
    fn project_task_cancel_takes_tracked_tool_call() {
        let request_id = Uuid::new_v4();
        let mut tasks = McpProjectTaskStore::default();
        tasks.start(
            "task-1".to_string(),
            McpProjectTaskKind::Copy,
            Some(McpTrackedToolCall {
                request_id,
                tool_name: "alcomd3_copy_project".to_string(),
                activity: None,
            }),
        );

        let (snapshot, tool_call) = tasks.cancel("task-1").unwrap();
        let tool_call = tool_call.unwrap();

        assert_eq!(snapshot.status, McpProjectTaskStatus::Cancelled);
        assert_eq!(tool_call.request_id, request_id);
        assert_eq!(tool_call.tool_name, "alcomd3_copy_project");
    }

    #[test]
    fn project_package_task_cancel_requests_abort_and_waits_for_worker() {
        let abort = AbortCheck::new();
        let request_id = Uuid::new_v4();
        let mut tasks = McpProjectTaskStore::default();
        tasks.start(
            "task-1".to_string(),
            McpProjectTaskKind::InstallPackage,
            Some(McpTrackedToolCall {
                request_id,
                tool_name: "alcomd3_install_project_package".to_string(),
                activity: None,
            }),
        );
        tasks.set_cancel_handle(
            "task-1",
            McpProjectTaskCancelHandle::AbortPackageApply(abort.clone()),
        );

        assert!(abort.check().is_ok());

        let (snapshot, _) = tasks.cancel("task-1").unwrap();

        assert_eq!(snapshot.status, McpProjectTaskStatus::Working);
        assert_eq!(
            snapshot.status_message.as_deref(),
            Some("Task cancellation requested")
        );
        assert!(abort.check().is_err());

        let tool_call = tasks
            .finish_success("task-1", json!({ "ok": true }))
            .unwrap();
        let snapshot = tasks.get("task-1").unwrap();

        assert_eq!(snapshot.status, McpProjectTaskStatus::Completed);
        assert_eq!(tool_call.request_id, request_id);
    }

    #[test]
    fn project_create_task_cancel_requests_abort_and_waits_for_worker() {
        let abort = AbortCheck::new();
        let request_id = Uuid::new_v4();
        let mut tasks = McpProjectTaskStore::default();
        tasks.start(
            "task-1".to_string(),
            McpProjectTaskKind::Create,
            Some(McpTrackedToolCall {
                request_id,
                tool_name: "alcomd3_create_project".to_string(),
                activity: None,
            }),
        );
        tasks.set_cancel_handle(
            "task-1",
            McpProjectTaskCancelHandle::AbortProjectCreate(abort.clone()),
        );

        assert!(abort.check().is_ok());

        let (snapshot, tool_call) = tasks.cancel("task-1").unwrap();

        assert_eq!(snapshot.status, McpProjectTaskStatus::Working);
        assert_eq!(
            snapshot.status_message.as_deref(),
            Some("Task cancellation requested")
        );
        assert!(tool_call.is_none());
        assert!(abort.check().is_err());

        let tool_call = tasks.finish_cancelled("task-1").unwrap();
        let snapshot = tasks.get("task-1").unwrap();

        assert_eq!(snapshot.status, McpProjectTaskStatus::Cancelled);
        assert_eq!(tool_call.request_id, request_id);
    }

    #[test]
    fn project_package_task_cancel_finishes_cancelled_after_apply_abort() {
        let abort = AbortCheck::new();
        let request_id = Uuid::new_v4();
        let mut tasks = McpProjectTaskStore::default();
        tasks.start(
            "task-1".to_string(),
            McpProjectTaskKind::InstallPackage,
            Some(McpTrackedToolCall {
                request_id,
                tool_name: "alcomd3_install_project_package".to_string(),
                activity: None,
            }),
        );
        tasks.set_cancel_handle(
            "task-1",
            McpProjectTaskCancelHandle::AbortPackageApply(abort.clone()),
        );

        let (_, tool_call) = tasks.cancel("task-1").unwrap();
        assert!(tool_call.is_none());

        let tool_call = tasks.finish_cancelled("task-1").unwrap();
        let snapshot = tasks.get("task-1").unwrap();

        assert_eq!(snapshot.status, McpProjectTaskStatus::Cancelled);
        assert_eq!(tool_call.request_id, request_id);
    }

    #[test]
    fn project_create_abort_detection_matches_abort_error_only() {
        assert!(is_project_create_abort_error(&McpIpcError::new(
            "project_create_error",
            "Aborted",
        )));
        assert!(!is_project_create_abort_error(&McpIpcError::new(
            "project_create_error",
            "disk write failed",
        )));
        assert!(!is_project_create_abort_error(&McpIpcError::new(
            "project_package_apply_error",
            "Aborted",
        )));
    }

    #[test]
    fn project_task_abort_all_takes_working_tracked_tool_calls() {
        let request_id = Uuid::new_v4();
        let mut tasks = McpProjectTaskStore::default();
        tasks.start(
            "task-1".to_string(),
            McpProjectTaskKind::Restore,
            Some(McpTrackedToolCall {
                request_id,
                tool_name: "alcomd3_restore_project".to_string(),
                activity: None,
            }),
        );

        let tool_calls = tasks.abort_all();

        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].request_id, request_id);
        assert_eq!(tool_calls[0].tool_name, "alcomd3_restore_project");
        assert!(tasks.tasks.is_empty());
    }

    #[test]
    fn disabled_mcp_still_allows_project_task_follow_up_methods() {
        assert!(mcp_request_allowed_when_disabled(
            IPC_METHOD_PROJECT_TASK_GET
        ));
        assert!(mcp_request_allowed_when_disabled(
            IPC_METHOD_PROJECT_TASK_LIST
        ));
        assert!(mcp_request_allowed_when_disabled(
            IPC_METHOD_PROJECT_TASK_CANCEL
        ));

        assert!(!mcp_request_allowed_when_disabled(
            IPC_METHOD_PROJECT_TASK_START
        ));
        assert!(!mcp_request_allowed_when_disabled("list_projects"));
        assert!(!mcp_request_allowed_when_disabled("backup_project"));
        assert!(!mcp_request_allowed_when_disabled(
            "install_project_package"
        ));
    }

    #[test]
    fn mcp_tool_call_finished_phase_tracks_success_and_failure() {
        let ok: Result<(), McpIpcError> = Ok(());
        let error: Result<(), McpIpcError> = Err(McpIpcError::new("failed", "failed"));

        assert_eq!(
            mcp_tool_call_finished_phase(&ok),
            McpToolCallPhase::Finished
        );
        assert_eq!(
            mcp_tool_call_finished_phase(&error),
            McpToolCallPhase::Failed
        );
    }

    #[test]
    fn mcp_tool_call_event_serializes_for_frontend_listener() {
        let request_id = Uuid::nil().to_string();
        let event = McpToolCallEvent {
            request_id: request_id.clone(),
            tool_name: "alcomd3_list_projects".to_string(),
            phase: McpToolCallPhase::Started,
        };

        let serialized = serde_json::to_value(event).unwrap();

        assert_eq!(
            serialized,
            json!({
                "requestId": request_id,
                "toolName": "alcomd3_list_projects",
                "phase": "started",
            })
        );
    }

    #[test]
    fn disabled_mcp_response_returns_business_error() {
        let request_id = Uuid::new_v4();

        let response = mcp_disabled_response(request_id);

        assert_eq!(response.request_id, request_id);
        assert!(!response.ok);
        let error = response.error.unwrap();
        assert_eq!(error.code, "mcp_disabled");
        assert!(error.message.contains("disabled"));
    }

    #[test]
    fn package_manifest_visibility_matches_gui_prerelease_and_yanked_filters() {
        let stable = test_package_manifest(json!({
            "name": "com.example.stable",
            "version": "1.0.0",
        }));
        let prerelease = test_package_manifest(json!({
            "name": "com.example.prerelease",
            "version": "1.0.0-beta.1",
        }));
        let yanked = test_package_manifest(json!({
            "name": "com.example.yanked",
            "version": "1.0.0",
            "vrc-get": {
                "yanked": true
            }
        }));

        assert!(package_manifest_is_available_for_display(&stable, false));
        assert!(!package_manifest_is_available_for_display(
            &prerelease,
            false
        ));
        assert!(package_manifest_is_available_for_display(&prerelease, true));
        assert!(!package_manifest_is_available_for_display(&yanked, true));
    }

    #[test]
    fn package_visibility_respects_gui_local_user_package_filter() {
        let manifest = test_package_manifest(json!({
            "name": "com.example.local",
            "version": "1.0.0",
        }));
        let package_path = Path::new("Packages/com.example.local");
        let package = PackageInfo::local(&manifest, package_path);
        let hidden_user_repositories = IndexSet::new();

        assert!(package_is_visible_with_gui_filters(
            &package,
            &hidden_user_repositories,
            false,
            false
        ));
        assert!(!package_is_visible_with_gui_filters(
            &package,
            &hidden_user_repositories,
            true,
            false
        ));
    }

    #[test]
    fn package_visibility_respects_gui_hidden_repository_filter() {
        let manifest = test_package_manifest(json!({
            "name": "com.example.remote",
            "version": "1.0.0",
        }));
        let repository = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));
        let package = PackageInfo::remote(&manifest, &repository);
        let mut hidden_user_repositories = IndexSet::new();
        hidden_user_repositories.insert("com.example.repo".to_string());

        assert!(package_is_visible_with_gui_filters(
            &package,
            &IndexSet::new(),
            false,
            false
        ));
        assert!(!package_is_visible_with_gui_filters(
            &package,
            &hidden_user_repositories,
            false,
            false
        ));
    }

    #[test]
    fn repository_summary_marks_default_and_user_repositories() {
        let official = test_cached_repository(json!({
            "id": "com.vrchat.repos.official",
            "url": "https://packages.vrchat.com/official?download",
            "packages": {}
        }));
        let curated = test_cached_repository(json!({
            "id": "com.vrchat.repos.curated",
            "url": "https://packages.vrchat.com/curated?download",
            "packages": {}
        }));
        let user = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));

        assert_eq!(repository_summary(&official)["kind"], "officialDefault");
        assert_eq!(repository_summary(&curated)["kind"], "curatedDefault");
        assert_eq!(repository_summary(&user)["kind"], "user");
        assert_eq!(repository_summary(&official)["isDefaultRepository"], true);
        assert_eq!(repository_summary(&user)["isUserRepository"], true);
    }

    #[test]
    fn package_summary_marks_local_user_and_remote_repository_kind() {
        let manifest = test_package_manifest(json!({
            "name": "com.example.package",
            "version": "1.0.0",
        }));
        let official = test_cached_repository(json!({
            "id": "com.vrchat.repos.official",
            "url": "https://packages.vrchat.com/official?download",
            "packages": {}
        }));
        let user = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));
        let official_package = PackageInfo::remote(&manifest, &official);
        let user_package = PackageInfo::remote(&manifest, &user);
        let local_package = PackageInfo::local(&manifest, Path::new("Packages/com.example.local"));

        assert_eq!(
            package_info_summary(&official_package)["source"]["kind"],
            "officialDefault"
        );
        assert_eq!(
            package_info_summary(&user_package)["source"]["kind"],
            "userRepository"
        );
        assert_eq!(
            package_info_summary(&local_package)["source"]["kind"],
            "localUser"
        );
    }

    #[test]
    fn package_summary_omits_detail_fields_from_list_items() {
        let manifest = test_package_manifest(json!({
            "name": "com.example.package",
            "displayName": "Example Package",
            "description": "Long package description",
            "version": "1.0.0",
            "keywords": ["avatar"],
            "vrc-get": {
                "aliases": ["example"]
            },
            "vpmDependencies": {
                "com.example.dependency": "1.x"
            },
            "legacyPackages": ["legacy-package"],
            "changelogUrl": "https://example.com/changelog",
            "documentationUrl": "https://example.com/docs"
        }));
        let repository = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));
        let package = PackageInfo::remote(&manifest, &repository);

        let summary = package_info_summary(&package);

        assert_eq!(summary["name"], "com.example.package");
        assert_eq!(summary["displayName"], "Example Package");
        assert_eq!(summary["version"], "1.0.0");
        assert!(summary["source"].is_object());
        assert!(summary.get("description").is_none());
        assert!(summary.get("keywords").is_none());
        assert!(summary.get("aliases").is_none());
        assert!(summary.get("vpmDependencies").is_none());
        assert!(summary.get("legacyPackages").is_none());
        assert!(summary.get("changelogUrl").is_none());
        assert!(summary.get("documentationUrl").is_none());
    }

    #[test]
    fn package_list_summaries_keep_latest_version_per_source() {
        let older = test_package_manifest(json!({
            "name": "com.example.package",
            "displayName": "Example Package",
            "version": "1.0.0",
        }));
        let newer = test_package_manifest(json!({
            "name": "com.example.package",
            "displayName": "Example Package",
            "version": "1.1.0",
        }));
        let other_repo_version = test_package_manifest(json!({
            "name": "com.example.package",
            "displayName": "Example Package",
            "version": "1.0.5",
        }));
        let repository = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));
        let other_repository = test_cached_repository(json!({
            "id": "com.example.other",
            "url": "https://example.com/other.json",
            "packages": {}
        }));
        let older = PackageInfo::remote(&older, &repository);
        let newer = PackageInfo::remote(&newer, &repository);
        let other_repo_package = PackageInfo::remote(&other_repo_version, &other_repository);

        let summaries = package_info_list_summaries([&older, &newer, &other_repo_package]);

        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0]["source"]["id"], "com.example.other");
        assert_eq!(summaries[0]["version"], "1.0.5");
        assert_eq!(summaries[1]["source"]["id"], "com.example.repo");
        assert_eq!(summaries[1]["version"], "1.1.0");
    }

    #[test]
    fn package_details_keep_manifest_detail_fields_and_source() {
        let manifest = test_package_manifest(json!({
            "name": "com.example.package",
            "displayName": "Example Package",
            "description": "Long package description",
            "version": "1.0.0",
            "keywords": ["avatar"],
            "vrc-get": {
                "aliases": ["example"]
            },
            "vpmDependencies": {
                "com.example.dependency": "1.x"
            },
            "legacyPackages": ["legacy-package"],
            "changelogUrl": "https://example.com/changelog",
            "documentationUrl": "https://example.com/docs"
        }));
        let repository = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));
        let package = PackageInfo::remote(&manifest, &repository);

        let details = package_info_details(&package);

        assert_eq!(details["description"], "Long package description");
        assert_eq!(details["keywords"], json!(["avatar"]));
        assert_eq!(details["aliases"], json!(["example"]));
        assert_eq!(
            details["vpmDependencies"],
            json!(["com.example.dependency"])
        );
        assert_eq!(details["legacyPackages"], json!(["legacy-package"]));
        assert_eq!(details["changelogUrl"], "https://example.com/changelog");
        assert_eq!(details["documentationUrl"], "https://example.com/docs");
        assert_eq!(details["source"]["kind"], "userRepository");
    }

    #[test]
    fn package_list_pagination_defaults_and_omits_count() {
        let page = package_list_response(
            test_package_list_items(MCP_PACKAGE_LIST_DEFAULT_LIMIT + 1),
            PackageListPagination::from_params(PackageListParams::default()),
        );

        assert_eq!(page["ok"], true);
        assert_eq!(page["totalCount"], MCP_PACKAGE_LIST_DEFAULT_LIMIT + 1);
        assert_eq!(page["offset"], 0);
        assert_eq!(page["limit"], MCP_PACKAGE_LIST_DEFAULT_LIMIT);
        assert_eq!(page["returnedCount"], MCP_PACKAGE_LIST_DEFAULT_LIMIT);
        assert_eq!(page["hasMore"], true);
        assert_eq!(page["nextOffset"], MCP_PACKAGE_LIST_DEFAULT_LIMIT);
        assert_eq!(
            page["packages"].as_array().unwrap().len(),
            MCP_PACKAGE_LIST_DEFAULT_LIMIT
        );
        assert!(page.get("count").is_none());
    }

    #[test]
    fn package_list_pagination_applies_offset_limit_and_reports_next_offset() {
        let page = package_list_response(
            test_package_list_items(5),
            PackageListPagination::from_params(PackageListParams {
                offset: Some(1),
                limit: Some(2),
            }),
        );

        assert_eq!(page["totalCount"], 5);
        assert_eq!(page["offset"], 1);
        assert_eq!(page["limit"], 2);
        assert_eq!(page["returnedCount"], 2);
        assert_eq!(page["hasMore"], true);
        assert_eq!(page["nextOffset"], 3);
        assert_eq!(page["packages"][0]["name"], "com.example.package001");
        assert_eq!(page["packages"][1]["name"], "com.example.package002");
    }

    #[test]
    fn package_list_pagination_clamps_limit_and_marks_last_page() {
        let page = package_list_response(
            test_package_list_items(MCP_PACKAGE_LIST_MAX_LIMIT + 2),
            PackageListPagination::from_params(PackageListParams {
                offset: Some(2),
                limit: Some(MCP_PACKAGE_LIST_MAX_LIMIT + 99),
            }),
        );

        assert_eq!(page["totalCount"], MCP_PACKAGE_LIST_MAX_LIMIT + 2);
        assert_eq!(page["offset"], 2);
        assert_eq!(page["limit"], MCP_PACKAGE_LIST_MAX_LIMIT);
        assert_eq!(page["returnedCount"], MCP_PACKAGE_LIST_MAX_LIMIT);
        assert_eq!(page["hasMore"], false);
        assert_eq!(page["nextOffset"], Value::Null);
        assert!(page.get("count").is_none());
    }

    #[test]
    fn package_list_pagination_returns_empty_page_for_offset_past_end() {
        let page = package_list_response(
            test_package_list_items(3),
            PackageListPagination::from_params(PackageListParams {
                offset: Some(99),
                limit: Some(2),
            }),
        );

        assert_eq!(page["totalCount"], 3);
        assert_eq!(page["offset"], 99);
        assert_eq!(page["limit"], 2);
        assert_eq!(page["returnedCount"], 0);
        assert_eq!(page["hasMore"], false);
        assert_eq!(page["nextOffset"], Value::Null);
        assert!(page["packages"].as_array().unwrap().is_empty());
    }

    #[test]
    fn package_details_response_omits_count() {
        let response = package_details_response(vec![json!({
            "name": "com.example.package",
            "version": "1.0.0",
        })]);

        assert_eq!(response["ok"], true);
        assert_eq!(response["packages"].as_array().unwrap().len(), 1);
        assert!(response.get("count").is_none());
    }

    #[test]
    fn repository_selector_matches_repository_id_or_url() {
        let repository = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));
        let other_repository = test_cached_repository(json!({
            "id": "com.example.other",
            "url": "https://example.com/other.json",
            "packages": {}
        }));

        let by_id = RepositorySelector::from_params(RepositoryPackagesParams {
            repository_id: Some("com.example.repo".to_string()),
            repository_url: None,
            offset: None,
            limit: None,
        })
        .unwrap();
        let by_url = RepositorySelector::from_params(RepositoryPackagesParams {
            repository_id: None,
            repository_url: Some("https://example.com/index.json".to_string()),
            offset: None,
            limit: None,
        })
        .unwrap();

        assert!(by_id.matches_repo(&repository));
        assert!(by_url.matches_repo(&repository));
        assert!(!by_id.matches_repo(&other_repository));
        assert!(!by_url.matches_repo(&other_repository));
    }

    #[test]
    fn repository_selector_rejects_empty_params() {
        let error = RepositorySelector::from_params(RepositoryPackagesParams {
            repository_id: None,
            repository_url: None,
            offset: None,
            limit: None,
        })
        .unwrap_err();

        assert_eq!(error.code, "invalid_params");
    }

    #[test]
    fn project_package_source_parser_treats_empty_source_as_omitted() {
        assert!(parse_project_package_source(None).unwrap().is_none());
        assert!(
            parse_project_package_source(Some(ProjectPackageSourceParams {
                repository_id: None,
                repository_url: None,
            }))
            .unwrap()
            .is_none()
        );
        assert!(
            parse_project_package_source(Some(ProjectPackageSourceParams {
                repository_id: Some(" ".to_string()),
                repository_url: Some(String::new()),
            }))
            .unwrap()
            .is_none()
        );

        let source = parse_project_package_source(Some(ProjectPackageSourceParams {
            repository_id: Some("com.example.repo".to_string()),
            repository_url: None,
        }))
        .unwrap()
        .unwrap();

        assert_eq!(source.repository_id.as_deref(), Some("com.example.repo"));
    }

    #[test]
    fn package_details_selector_matches_name_version_and_repository() {
        let manifest = test_package_manifest(json!({
            "name": "com.example.remote",
            "version": "1.0.0",
        }));
        let other_version = test_package_manifest(json!({
            "name": "com.example.remote",
            "version": "2.0.0",
        }));
        let repository = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));
        let other_repository = test_cached_repository(json!({
            "id": "com.example.other",
            "url": "https://example.com/other.json",
            "packages": {}
        }));
        let selector = PackageDetailsSelector::from_params(PackageDetailsParams {
            package_name: "com.example.remote".to_string(),
            version: Some("1.0.0".to_string()),
            repository_id: Some("com.example.repo".to_string()),
            repository_url: None,
        })
        .unwrap();
        let selected_package = PackageInfo::remote(&manifest, &repository);
        let other_version_package = PackageInfo::remote(&other_version, &repository);
        let other_repository_package = PackageInfo::remote(&manifest, &other_repository);

        assert!(selector.matches_package(&selected_package));
        assert!(!selector.matches_package(&other_version_package));
        assert!(!selector.matches_package(&other_repository_package));
    }

    #[test]
    fn package_repository_filter_matches_only_selected_remote_repo() {
        let manifest = test_package_manifest(json!({
            "name": "com.example.remote",
            "version": "1.0.0",
        }));
        let repository = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));
        let other_repository = test_cached_repository(json!({
            "id": "com.example.other",
            "url": "https://example.com/other.json",
            "packages": {}
        }));
        let selector = RepositorySelector::from_params(RepositoryPackagesParams {
            repository_id: Some("com.example.repo".to_string()),
            repository_url: None,
            offset: None,
            limit: None,
        })
        .unwrap();
        let selected_package = PackageInfo::remote(&manifest, &repository);
        let other_package = PackageInfo::remote(&manifest, &other_repository);
        let local_package = PackageInfo::local(&manifest, Path::new("Packages/com.example.local"));

        assert!(package_is_from_repository(&selected_package, &selector));
        assert!(!package_is_from_repository(&other_package, &selector));
        assert!(!package_is_from_repository(&local_package, &selector));
    }

    #[test]
    fn project_task_params_accept_bridge_camel_case_task_id() {
        let start: ProjectTaskStartParams = serde_json::from_value(json!({
            "taskId": "task-1",
            "method": "backup_project",
            "params": {
                "project_path": "C:/Projects/Example",
            },
        }))
        .unwrap();

        assert_eq!(start.task_id, "task-1");
        assert_eq!(start.method, "backup_project");
        assert_eq!(start.params["project_path"], "C:/Projects/Example");

        let id: ProjectTaskIdParams = serde_json::from_value(json!({
            "taskId": "task-1",
        }))
        .unwrap();

        assert_eq!(id.task_id, "task-1");
    }

    #[test]
    fn backup_project_params_default_to_including_vpm_packages() {
        let default_params: BackupProjectParams = serde_json::from_value(json!({
            "project_path": "C:/Projects/Example",
        }))
        .unwrap();
        assert!(!default_params.exclude_vpm_packages);

        let excluding_params: BackupProjectParams = serde_json::from_value(json!({
            "project_path": "C:/Projects/Example",
            "exclude_vpm_packages": true,
        }))
        .unwrap();
        assert!(excluding_params.exclude_vpm_packages);
    }

    #[test]
    fn normalize_project_package_name_rejects_invalid_identifiers() {
        for name in ["../evil", "evil/asset", "evil\\asset", ".evil", "com..evil"] {
            let error = normalize_project_package_name(name.to_string()).unwrap_err();
            assert_eq!(error.code, "invalid_params");
        }
    }

    #[test]
    fn normalize_project_package_name_accepts_trimmed_valid_identifier() {
        assert_eq!(
            normalize_project_package_name("  com.example.package  ".to_string()).unwrap(),
            "com.example.package"
        );
    }

    #[test]
    fn normalize_project_package_name_rejects_empty_identifier() {
        let error = normalize_project_package_name("   ".to_string()).unwrap_err();
        assert_eq!(error.code, "invalid_params");
    }

    fn test_package_manifest(value: Value) -> PackageManifest {
        serde_json::from_value(value).unwrap()
    }

    fn test_cached_repository(value: Value) -> LocalCachedRepository {
        let Value::Object(repository) = value else {
            panic!("expected repository object");
        };
        LocalCachedRepository::new(
            RemoteRepository::parse(repository).unwrap(),
            IndexMap::new(),
        )
    }

    fn test_package_list_items(count: usize) -> Vec<Value> {
        (0..count)
            .map(|index| {
                json!({
                    "name": format!("com.example.package{index:03}"),
                    "version": "1.0.0",
                })
            })
            .collect()
    }

    fn test_client(name: &str, version: Option<&str>) -> ClientIdentity {
        ClientIdentity {
            session_id: Uuid::new_v4(),
            name: name.to_string(),
            version: version.map(str::to_string),
        }
    }
}
