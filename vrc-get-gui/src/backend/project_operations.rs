use crate::commands::{CreatedProjectInfo, RustError};
use crate::state::{GuiConfigState, PackagesState, SettingsState};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use tauri::AppHandle;
use vrc_get_vpm::AbortCheck;
use vrc_get_vpm::io::DefaultEnvironmentIo;

pub(crate) async fn create_project_backup(
    config: &GuiConfigState,
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    project_path: String,
    backup_name: Option<String>,
    exclude_vpm: bool,
    progress: impl Fn(Value) + Clone + Send + Sync + 'static,
) -> Result<PathBuf, RustError> {
    crate::commands::create_project_backup_with_settings(
        config,
        settings,
        io,
        project_path,
        backup_name,
        exclude_vpm,
        move |snapshot| emit_progress(&progress, snapshot),
    )
    .await
}

pub(crate) async fn copy_registered_project(
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    source_project_path: String,
    new_project_path: String,
    progress: impl Fn(Value) + Clone + Send + Sync + 'static,
) -> Result<String, RustError> {
    crate::commands::copy_registered_project_to_path(
        settings,
        io,
        source_project_path,
        new_project_path,
        move |snapshot| emit_progress(&progress, snapshot),
    )
    .await
}

pub(crate) async fn restore_project_from_backup(
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    backup_path: String,
    project_name: Option<String>,
    progress: impl Fn(Value) + Clone + Send + Sync + 'static,
) -> Result<String, RustError> {
    crate::commands::restore_project_from_zip_backup(
        settings,
        io,
        backup_path,
        project_name,
        move |snapshot| emit_progress(&progress, snapshot),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_project(
    packages: &PackagesState,
    settings: &SettingsState,
    config: &GuiConfigState,
    io: &DefaultEnvironmentIo,
    http: &reqwest::Client,
    base_path: Option<String>,
    project_name: String,
    template_id: Option<String>,
    unity_version: Option<String>,
    abort: Option<AbortCheck>,
) -> Result<CreatedProjectInfo, RustError> {
    crate::commands::create_project_with_defaults(
        packages,
        settings,
        config,
        io,
        http,
        base_path,
        project_name,
        template_id,
        unity_version,
        abort.as_ref(),
    )
    .await
}

pub(crate) async fn add_existing_project(
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    project_path: String,
) -> Result<String, RustError> {
    crate::commands::add_existing_project_by_path(settings, io, project_path).await
}

pub(crate) async fn apply_project_changes(
    app: AppHandle,
    project_path: String,
    changes_version: u32,
) -> Result<(), RustError> {
    crate::commands::project_apply_pending_changes_from_prepared(app, project_path, changes_version)
        .await
}

pub(crate) async fn apply_project_changes_with_abort(
    app: AppHandle,
    project_path: String,
    changes_version: u32,
    abort: AbortCheck,
) -> Result<(), RustError> {
    crate::commands::project_apply_pending_changes_from_prepared_with_abort(
        app,
        project_path,
        changes_version,
        abort,
    )
    .await
}

fn emit_progress<P>(progress: &impl Fn(Value), snapshot: P)
where
    P: Serialize,
{
    if let Ok(value) = serde_json::to_value(snapshot) {
        progress(value);
    }
}
