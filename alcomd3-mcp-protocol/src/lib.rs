use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

pub const IPC_PROTOCOL_VERSION: u32 = 2;
pub const IPC_IO_TIMEOUT: Duration = Duration::from_secs(120);
pub const IPC_MAX_LINE_BYTES: usize = 64 * 1024 * 1024;
pub const IPC_METHOD_PROJECT_TASK_START: &str = "project_task_start";
pub const IPC_METHOD_PROJECT_TASK_GET: &str = "project_task_get";
pub const IPC_METHOD_PROJECT_TASK_LIST: &str = "project_task_list";
pub const IPC_METHOD_PROJECT_TASK_CANCEL: &str = "project_task_cancel";
pub use alcomd3_app_paths::{
    MCP_DATA_DIR_NAME, MCP_ENDPOINT_FILE_ENV as ENDPOINT_FILE_ENV,
    MCP_ENDPOINT_FILE_NAME as ENDPOINT_FILE_NAME,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IpcTransport {
    Tcp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointMetadata {
    pub protocol_version: u32,
    pub transport: IpcTransport,
    pub host: String,
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientIdentity {
    pub session_id: Uuid,
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcRequest {
    pub protocol_version: u32,
    pub token: String,
    pub request_id: Uuid,
    pub client: ClientIdentity,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcResponse {
    pub request_id: Uuid,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl IpcResponse {
    pub fn success(request_id: Uuid, result: Value) -> Self {
        Self {
            request_id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(
        request_id: Uuid,
        code: impl Into<String>,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> Self {
        Self {
            request_id,
            ok: false,
            result: None,
            error: Some(IpcError {
                code: code.into(),
                message: message.into(),
                data,
            }),
        }
    }
}

pub fn endpoint_file_path() -> PathBuf {
    alcomd3_app_paths::mcp_endpoint_file_path()
}

pub fn endpoint_file_path_from_env(override_path: Option<OsString>) -> PathBuf {
    alcomd3_app_paths::mcp_endpoint_file_path_from_env(override_path)
}

pub fn default_endpoint_file_path(local_data_root: &Path) -> PathBuf {
    alcomd3_app_paths::mcp_endpoint_file_path_from_local_data_root(local_data_root)
}

pub fn local_data_root() -> PathBuf {
    alcomd3_app_paths::local_data_root()
}

pub fn local_data_root_from_env(
    local_app_data: Option<OsString>,
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> PathBuf {
    alcomd3_app_paths::local_data_root_from_parts(local_app_data, xdg_data_home, home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_file_path_uses_explicit_override() {
        let path = endpoint_file_path_from_env(Some(OsString::from("C:/tmp/alcom-endpoint.json")));
        assert_eq!(path, PathBuf::from("C:/tmp/alcom-endpoint.json"));
    }

    #[test]
    fn default_endpoint_file_lives_under_mcp_data_directory() {
        let path = default_endpoint_file_path(Path::new("/data"));
        assert_eq!(
            path,
            Path::new("/data")
                .join("ALCOMD3")
                .join("mcp")
                .join("endpoint.json")
        );
    }

    #[test]
    fn ipc_limits_match_current_transport_policy() {
        assert_eq!(IPC_IO_TIMEOUT, Duration::from_secs(120));
        assert_eq!(IPC_MAX_LINE_BYTES, 64 * 1024 * 1024);
    }

    #[test]
    fn response_serializes_camel_case_request_id_and_ok() {
        let request_id = Uuid::nil();
        let serialized = serde_json::to_value(IpcResponse::success(
            request_id,
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap();
        assert_eq!(serialized["requestId"], request_id.to_string());
        assert_eq!(serialized["ok"], true);
        assert!(serialized.get("result").is_some());
    }
}
