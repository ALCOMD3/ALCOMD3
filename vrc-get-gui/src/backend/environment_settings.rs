use crate::state::{GuiConfigState, SettingsState};
use crate::utils;
use serde::Serialize;
use std::fmt::{Display, Formatter};
use vrc_get_vpm::environment::VccDatabaseConnection;
use vrc_get_vpm::io::DefaultEnvironmentIo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EnvironmentSettingsSnapshot {
    pub(crate) ok: bool,
    pub(crate) unity_installations: Vec<UnityInstallationSnapshot>,
    pub(crate) unity_launch_arguments: UnityArgumentsSnapshot,
    pub(crate) paths: EnvironmentPathsSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnityInstallationSnapshot {
    pub(crate) path: String,
    pub(crate) version: String,
    pub(crate) loaded_from_hub: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnityArgumentsSnapshot {
    pub(crate) configured: Option<Vec<String>>,
    pub(crate) builtin_default: Vec<String>,
    pub(crate) effective: Vec<String>,
    pub(crate) uses_builtin_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EnvironmentPathsSnapshot {
    pub(crate) default_project_path: String,
    pub(crate) project_backup_path: String,
}

#[derive(Debug, Clone)]
pub(crate) struct EnvironmentSettingsQueryError {
    code: &'static str,
    message: String,
}

impl EnvironmentSettingsQueryError {
    fn new(code: &'static str, error: impl Display) -> Self {
        Self {
            code,
            message: error.to_string(),
        }
    }

    pub(crate) fn code(&self) -> &'static str {
        self.code
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl Display for EnvironmentSettingsQueryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}

pub(crate) async fn load_environment_settings_snapshot(
    io: &DefaultEnvironmentIo,
    settings: &SettingsState,
    config: &GuiConfigState,
    builtin_unity_arguments: &[&str],
) -> Result<EnvironmentSettingsSnapshot, EnvironmentSettingsQueryError> {
    let configured_unity_arguments = config.get().default_unity_arguments.clone();
    let connection = VccDatabaseConnection::connect(io)
        .await
        .map_err(|e| EnvironmentSettingsQueryError::new("project_database_error", e))?;
    let unity_installations = connection
        .get_unity_installations()
        .iter()
        .filter_map(|unity| {
            Some(UnityInstallationSnapshot {
                path: unity.path()?.to_string(),
                version: unity.version()?.to_string(),
                loaded_from_hub: unity.loaded_from_hub(),
            })
        })
        .collect::<Vec<_>>();

    let settings = settings
        .load(io)
        .await
        .map_err(|e| EnvironmentSettingsQueryError::new("settings_load_error", e))?;
    let default_project_path = settings
        .default_project_path()
        .map(str::to_string)
        .unwrap_or_else(utils::default_default_project_path);
    let project_backup_path = settings
        .project_backup_path()
        .map(str::to_string)
        .unwrap_or_else(utils::default_backup_path);
    let builtin_default = builtin_unity_arguments
        .iter()
        .copied()
        .map(String::from)
        .collect::<Vec<_>>();
    Ok(environment_settings_snapshot(
        unity_installations,
        configured_unity_arguments,
        builtin_default,
        default_project_path,
        project_backup_path,
    ))
}

pub(crate) fn environment_settings_snapshot(
    unity_installations: Vec<UnityInstallationSnapshot>,
    configured_unity_arguments: Option<Vec<String>>,
    builtin_default: Vec<String>,
    default_project_path: String,
    project_backup_path: String,
) -> EnvironmentSettingsSnapshot {
    let uses_builtin_default = configured_unity_arguments.is_none();
    let effective = configured_unity_arguments
        .clone()
        .unwrap_or_else(|| builtin_default.clone());

    EnvironmentSettingsSnapshot {
        ok: true,
        unity_installations,
        unity_launch_arguments: UnityArgumentsSnapshot {
            configured: configured_unity_arguments,
            builtin_default,
            effective,
            uses_builtin_default,
        },
        paths: EnvironmentPathsSnapshot {
            default_project_path,
            project_backup_path,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn environment_settings_snapshot_reports_configured_unity_arguments_and_paths() {
        let snapshot = environment_settings_snapshot(
            vec![UnityInstallationSnapshot {
                path: "C:/Unity/Editor/Unity.exe".to_string(),
                version: "2022.3.22f1".to_string(),
                loaded_from_hub: true,
            }],
            Some(vec!["-projectPath".to_string()]),
            vec!["-default".to_string()],
            "C:/Projects".to_string(),
            "C:/Backups".to_string(),
        );
        let summary = serde_json::to_value(snapshot).unwrap();

        assert_eq!(summary["ok"], true);
        assert_eq!(
            summary["unityInstallations"][0]["path"],
            "C:/Unity/Editor/Unity.exe"
        );
        assert_eq!(
            summary["unityLaunchArguments"]["configured"],
            json!(["-projectPath"])
        );
        assert_eq!(
            summary["unityLaunchArguments"]["builtinDefault"],
            json!(["-default"])
        );
        assert_eq!(
            summary["unityLaunchArguments"]["effective"],
            json!(["-projectPath"])
        );
        assert_eq!(summary["unityLaunchArguments"]["usesBuiltinDefault"], false);
        assert_eq!(summary["paths"]["defaultProjectPath"], "C:/Projects");
        assert_eq!(summary["paths"]["projectBackupPath"], "C:/Backups");
    }

    #[test]
    fn environment_settings_snapshot_uses_builtin_unity_arguments_when_unconfigured() {
        let snapshot = environment_settings_snapshot(
            Vec::new(),
            None,
            vec!["-default".to_string()],
            "C:/Projects".to_string(),
            "C:/Backups".to_string(),
        );
        let summary = serde_json::to_value(snapshot).unwrap();

        assert_eq!(summary["unityLaunchArguments"]["configured"], Value::Null);
        assert_eq!(
            summary["unityLaunchArguments"]["effective"],
            json!(["-default"])
        );
        assert_eq!(summary["unityLaunchArguments"]["usesBuiltinDefault"], true);
    }
}
