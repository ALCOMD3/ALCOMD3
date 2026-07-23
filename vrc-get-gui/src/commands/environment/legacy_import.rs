use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager, State};
use vrc_get_vpm::environment::{
    LEGACY_VRC_GET_SETTINGS_PATH, PACKAGE_CACHE_FOLDER, REPO_CACHE_FOLDER,
    VPM_SETTINGS_BACKUP_PATH, VRC_GET_SETTINGS_PATH,
};
use vrc_get_vpm::io::{DefaultEnvironmentIo, IoTrait};

use crate::activity_log::{
    ActivityDetail, ActivityImportance, ActivityInput, ActivityKind, ActivityLogState,
    ActivitySource, operations,
};
use crate::commands::prelude::*;
use crate::config::{GuiConfig, ThemeConfig};
use crate::templates;

const SETTINGS_PATH: &str = "settings.json";
const VCC_DATABASE_PATH: &str = "vcc.liteDb";
const VCC_TEMPLATES_PATH: &str = "Templates";
const BETA_THEME_CONFIG_PATH: &str = "vrc-get/theme-config.json";
const PROJECT_SETTINGS_KEYS: &[&str] = &[
    "userProjects",
    "unityEditors",
    "preferredUnityEditors",
    "pathToUnityExe",
    "pathToUnityHub",
    "lastSelectedProject",
];
const RESOURCE_SETTINGS_KEYS: &[&str] = &["userRepos", "userPackageFolders"];
const APP_SETTINGS_KEYS: &[&str] = &[
    "defaultProjectPath",
    "projectBackupPath",
    "pathToUnityExe",
    "pathToUnityHub",
    "unityEditors",
    "preferredUnityEditors",
    "showPrereleasePackages",
    "trackCommunityRepos",
    "selectedProviders",
    "skipUnityAutoFind",
    "skipRequirements",
    "allowPii",
];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, specta::Type)]
pub enum TauriLegacyDataSourceKind {
    Vcc,
    Alcom,
    Alcomd3Beta,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, specta::Type)]
pub enum TauriLegacyDataImportCategory {
    Projects,
    Resources,
    Theme,
    Settings,
}

#[derive(Debug, Serialize, specta::Type)]
pub struct TauriLegacyDataSource {
    kind: TauriLegacyDataSourceKind,
    display_name: String,
    path: String,
}

#[derive(Default, Debug, Serialize, specta::Type)]
pub struct TauriLegacyDataImportResult {
    imported_settings: bool,
    imported_database: bool,
    imported_repositories: bool,
    imported_vcc_templates: bool,
    imported_alcom_templates: bool,
    imported_vrc_get_settings: bool,
    imported_gui_config: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn environment_legacy_data_sources(
    io: State<'_, DefaultEnvironmentIo>,
) -> Result<Vec<TauriLegacyDataSource>, RustError> {
    let target_root = io.resolve(Path::new(""));
    let mut sources = Vec::new();

    for kind in [
        TauriLegacyDataSourceKind::Vcc,
        TauriLegacyDataSourceKind::Alcom,
    ] {
        let source_io = legacy_source_io(kind);
        let source_root = source_io.resolve(Path::new(""));
        if source_root == target_root {
            continue;
        }

        sources.push(describe_source(kind, &source_io).await?);
    }

    if alcomd3_beta_source_has_data(&io).await? {
        sources.push(describe_source(TauriLegacyDataSourceKind::Alcomd3Beta, &io).await?);
    }

    Ok(sources)
}

#[tauri::command]
#[specta::specta]
pub async fn environment_import_legacy_data(
    app: AppHandle,
    io: State<'_, DefaultEnvironmentIo>,
    settings: State<'_, SettingsState>,
    config: State<'_, GuiConfigState>,
    theme_config: State<'_, ThemeConfigState>,
    packages: State<'_, PackagesState>,
    templates: State<'_, TemplatesState>,
    source: TauriLegacyDataSourceKind,
    category: TauriLegacyDataImportCategory,
) -> Result<TauriLegacyDataImportResult, RustError> {
    let activity = app.state::<ActivityLogState>();
    let tracker = activity.start_activity(Some(&app), legacy_import_activity(source, category));
    let result = import_legacy_data(
        io.inner(),
        settings.inner(),
        config.inner(),
        theme_config.inner(),
        packages.inner(),
        templates.inner(),
        source,
        category,
    )
    .await;

    match &result {
        Ok(result) => {
            let imported_count = result.imported_count();
            let summary = if imported_count == 0 {
                "External app data import completed with no changes"
            } else {
                "External app data import completed"
            };
            activity.finish_success(
                Some(&app),
                &tracker,
                summary,
                legacy_import_result_details(source, category, result),
            );
        }
        Err(error) => {
            activity.finish_failed(
                Some(&app),
                &tracker,
                "External app data import failed",
                legacy_import_details(source, category),
                error,
            );
        }
    }

    result
}

async fn import_legacy_data(
    io: &DefaultEnvironmentIo,
    settings: &SettingsState,
    config: &GuiConfigState,
    theme_config: &ThemeConfigState,
    packages: &PackagesState,
    templates: &TemplatesState,
    source: TauriLegacyDataSourceKind,
    category: TauriLegacyDataImportCategory,
) -> Result<TauriLegacyDataImportResult, RustError> {
    let source_io = match source {
        TauriLegacyDataSourceKind::Alcomd3Beta => io.clone(),
        _ => legacy_source_io(source),
    };
    let source_root = source_io.resolve(Path::new(""));
    let target_root = io.resolve(Path::new(""));
    if source_root == target_root && !matches!(source, TauriLegacyDataSourceKind::Alcomd3Beta) {
        return Err(RustError::unrecoverable_str(
            "legacy source is the current ALCOMD3 data directory",
        ));
    }

    if matches!(source, TauriLegacyDataSourceKind::Alcomd3Beta) {
        let result = import_alcomd3_beta_data(io, config, theme_config, category).await?;
        settings.clear_cache();
        packages.clear_cache();
        templates.clear_cache();
        return Ok(result);
    }

    let mut result = TauriLegacyDataImportResult::default();

    match category {
        TauriLegacyDataImportCategory::Projects => {
            result.imported_settings =
                import_settings_subset(&source_io, &io, LegacySettingsCategory::Projects).await?;
            result.imported_database =
                copy_file_if_exists(&source_io, &io, Path::new(VCC_DATABASE_PATH)).await?;
        }
        TauriLegacyDataImportCategory::Resources => {
            result.imported_settings =
                import_settings_subset(&source_io, &io, LegacySettingsCategory::Resources).await?;
            result.imported_repositories = import_repositories(&source_io, &io).await?;
            result.imported_vcc_templates = import_vcc_templates(&source_io, &io).await?;
            result.imported_alcom_templates = copy_dir_if_exists_to(
                &source_io,
                &io,
                Path::new(crate::storage::LEGACY_TEMPLATE_DIR),
                Path::new(crate::storage::TEMPLATE_DIR),
            )
            .await?;
            result.imported_vrc_get_settings = copy_file_if_exists_to(
                &source_io,
                &io,
                Path::new(LEGACY_VRC_GET_SETTINGS_PATH),
                Path::new(VRC_GET_SETTINGS_PATH),
            )
            .await?;
        }
        TauriLegacyDataImportCategory::Theme => {
            result.imported_gui_config = import_gui_config(&source_io, &theme_config).await?;
        }
        TauriLegacyDataImportCategory::Settings => {
            result.imported_settings =
                import_settings_subset(&source_io, &io, LegacySettingsCategory::Settings).await?;
            result.imported_gui_config = import_gui_settings(&source_io, &config).await?;
        }
    }

    settings.clear_cache();
    packages.clear_cache();
    templates.clear_cache();

    Ok(result)
}

fn legacy_import_activity(
    source: TauriLegacyDataSourceKind,
    category: TauriLegacyDataImportCategory,
) -> ActivityInput {
    ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::LEGACY_IMPORT,
        "Importing external app data",
    )
    .target(legacy_import_category_label(category))
    .details(legacy_import_details(source, category))
}

fn legacy_import_details(
    source: TauriLegacyDataSourceKind,
    category: TauriLegacyDataImportCategory,
) -> Vec<ActivityDetail> {
    vec![
        ActivityDetail::new("source", legacy_import_source_label(source)),
        ActivityDetail::new("category", legacy_import_category_label(category)),
    ]
}

fn legacy_import_result_details(
    source: TauriLegacyDataSourceKind,
    category: TauriLegacyDataImportCategory,
    result: &TauriLegacyDataImportResult,
) -> Vec<ActivityDetail> {
    let mut details = legacy_import_details(source, category);
    details.push(ActivityDetail::new(
        "imported",
        result.imported_count().to_string(),
    ));
    details
}

fn legacy_import_source_label(source: TauriLegacyDataSourceKind) -> &'static str {
    match source {
        TauriLegacyDataSourceKind::Vcc => "VCC",
        TauriLegacyDataSourceKind::Alcom => "ALCOM",
        TauriLegacyDataSourceKind::Alcomd3Beta => "ALCOMD3 2.1.0 beta",
    }
}

fn legacy_import_category_label(category: TauriLegacyDataImportCategory) -> &'static str {
    match category {
        TauriLegacyDataImportCategory::Projects => "Projects",
        TauriLegacyDataImportCategory::Resources => "Resources",
        TauriLegacyDataImportCategory::Theme => "Theme",
        TauriLegacyDataImportCategory::Settings => "Settings",
    }
}

impl TauriLegacyDataImportResult {
    fn imported_count(&self) -> usize {
        [
            self.imported_settings,
            self.imported_database,
            self.imported_repositories,
            self.imported_vcc_templates,
            self.imported_alcom_templates,
            self.imported_vrc_get_settings,
            self.imported_gui_config,
        ]
        .into_iter()
        .filter(|imported| *imported)
        .count()
    }
}

fn legacy_source_io(source: TauriLegacyDataSourceKind) -> DefaultEnvironmentIo {
    match source {
        TauriLegacyDataSourceKind::Vcc => DefaultEnvironmentIo::new_legacy_vcc(),
        TauriLegacyDataSourceKind::Alcom => DefaultEnvironmentIo::new_legacy_alcom(),
        TauriLegacyDataSourceKind::Alcomd3Beta => DefaultEnvironmentIo::new_default(),
    }
}

async fn describe_source(
    kind: TauriLegacyDataSourceKind,
    source_io: &DefaultEnvironmentIo,
) -> io::Result<TauriLegacyDataSource> {
    let source_root = source_io.resolve(Path::new(""));

    Ok(TauriLegacyDataSource {
        kind,
        display_name: match kind {
            TauriLegacyDataSourceKind::Vcc => "VRChat Creator Companion / legacy ALCOM".to_string(),
            TauriLegacyDataSourceKind::Alcom => "Legacy ALCOM".to_string(),
            TauriLegacyDataSourceKind::Alcomd3Beta => "ALCOMD3 2.1.0 beta".to_string(),
        },
        path: source_root.to_string_lossy().into_owned(),
    })
}

async fn alcomd3_beta_source_has_data(io: &DefaultEnvironmentIo) -> io::Result<bool> {
    Ok(
        path_is_file(io.resolve(Path::new(BETA_THEME_CONFIG_PATH))).await?
            || path_is_file(io.resolve(Path::new(crate::storage::LEGACY_GUI_CONFIG_PATH))).await?
            || path_is_file(io.resolve(Path::new(LEGACY_VRC_GET_SETTINGS_PATH))).await?
            || path_is_dir(io.resolve(Path::new(crate::storage::LEGACY_TEMPLATE_DIR))).await?,
    )
}

async fn import_alcomd3_beta_data(
    io: &DefaultEnvironmentIo,
    config: &GuiConfigState,
    theme_config: &ThemeConfigState,
    category: TauriLegacyDataImportCategory,
) -> io::Result<TauriLegacyDataImportResult> {
    let mut result = TauriLegacyDataImportResult::default();

    match category {
        TauriLegacyDataImportCategory::Projects => {}
        TauriLegacyDataImportCategory::Resources => {
            result.imported_alcom_templates = copy_dir_if_exists_to(
                io,
                io,
                Path::new(crate::storage::LEGACY_TEMPLATE_DIR),
                Path::new(crate::storage::TEMPLATE_DIR),
            )
            .await?;
            result.imported_vrc_get_settings = copy_file_if_exists_to(
                io,
                io,
                Path::new(LEGACY_VRC_GET_SETTINGS_PATH),
                Path::new(VRC_GET_SETTINGS_PATH),
            )
            .await?;
        }
        TauriLegacyDataImportCategory::Theme => {
            result.imported_gui_config = import_alcomd3_beta_theme_config(io, theme_config).await?;
        }
        TauriLegacyDataImportCategory::Settings => {
            result.imported_gui_config = import_gui_settings(io, config).await?;
        }
    }

    Ok(result)
}

async fn read_legacy_gui_config_as_value(source_io: &DefaultEnvironmentIo) -> io::Result<Value> {
    let source_path = source_io.resolve(Path::new(crate::storage::LEGACY_GUI_CONFIG_PATH));
    let source = tokio::fs::read(&source_path).await?;
    serde_json::from_slice::<Value>(&source).map_err(Into::into)
}

#[derive(Clone, Copy)]
enum LegacySettingsCategory {
    Projects,
    Resources,
    Settings,
}

async fn import_settings_subset(
    source_io: &DefaultEnvironmentIo,
    target_io: &DefaultEnvironmentIo,
    category: LegacySettingsCategory,
) -> io::Result<bool> {
    let source_path = source_io.resolve(Path::new(SETTINGS_PATH));
    if !path_is_file(&source_path).await? {
        return Ok(false);
    }

    let source = tokio::fs::read(&source_path).await?;
    let source = serde_json::from_slice::<Value>(&source)?;
    let target = read_settings_or_default(target_io).await?;
    let (imported, changed) = merge_legacy_settings_for_import(
        source,
        target,
        category,
        &source_io.resolve(Path::new("")),
        &target_io.resolve(Path::new("")),
    );
    if !changed {
        return Ok(false);
    }

    let imported = serde_json::to_vec_pretty(&imported)?;

    target_io.create_dir_all(Path::new("")).await?;
    target_io
        .write_atomic(Path::new(SETTINGS_PATH), &imported)
        .await?;
    target_io.create_dir_all(Path::new("state")).await?;
    target_io
        .write_atomic(Path::new(VPM_SETTINGS_BACKUP_PATH), &imported)
        .await?;

    Ok(true)
}

async fn read_settings_or_default(target_io: &DefaultEnvironmentIo) -> io::Result<Value> {
    let target_path = target_io.resolve(Path::new(SETTINGS_PATH));
    if !path_is_file(&target_path).await? {
        return Ok(Value::Object(Map::new()));
    }

    let target = tokio::fs::read(&target_path).await?;
    Ok(serde_json::from_slice::<Value>(&target)?)
}

fn merge_legacy_settings_for_import(
    source: Value,
    mut target: Value,
    category: LegacySettingsCategory,
    legacy_root: &Path,
    target_root: &Path,
) -> (Value, bool) {
    let Some(source_object) = source.as_object() else {
        return (target, false);
    };

    if !target.is_object() {
        target = Value::Object(Map::new());
    }

    let target_object = target.as_object_mut().unwrap();
    let mut changed = false;

    match category {
        LegacySettingsCategory::Projects => {
            changed |= copy_settings_keys(source_object, target_object, PROJECT_SETTINGS_KEYS);
        }
        LegacySettingsCategory::Resources => {
            changed |= copy_settings_keys_with(
                source_object,
                target_object,
                RESOURCE_SETTINGS_KEYS,
                |key, mut value| {
                    if key == "userRepos" {
                        rewrite_user_repo_paths(&mut value, legacy_root, target_root);
                    }
                    value
                },
            );
        }
        LegacySettingsCategory::Settings => {
            changed |= copy_settings_keys(source_object, target_object, APP_SETTINGS_KEYS);
        }
    }

    (target, changed)
}

fn copy_settings_keys(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    keys: &[&str],
) -> bool {
    copy_settings_keys_with(source, target, keys, |_key, value| value)
}

fn copy_settings_keys_with(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    keys: &[&str],
    mut transform: impl FnMut(&str, Value) -> Value,
) -> bool {
    let mut changed = false;

    for key in keys {
        let Some(value) = source.get(*key).cloned() else {
            continue;
        };

        target.insert((*key).to_string(), transform(key, value));
        changed = true;
    }

    changed
}

fn rewrite_user_repo_paths(value: &mut Value, legacy_root: &Path, target_root: &Path) {
    let Some(repos) = value.as_array_mut() else {
        return;
    };

    for repo in repos {
        let Some(repo) = repo.as_object_mut() else {
            continue;
        };
        let Some(local_path) = repo.get("localPath").and_then(Value::as_str) else {
            continue;
        };
        if let Some(imported_path) = rewrite_path_under_root(local_path, legacy_root, target_root) {
            repo.insert("localPath".to_string(), Value::String(imported_path));
        }
    }
}

fn rewrite_path_under_root(path: &str, legacy_root: &Path, target_root: &Path) -> Option<String> {
    let path = PathBuf::from(path);
    let relative = path.strip_prefix(legacy_root).ok()?;
    Some(target_root.join(relative).to_string_lossy().into_owned())
}

async fn import_gui_config(
    source_io: &DefaultEnvironmentIo,
    config: &ThemeConfigState,
) -> io::Result<bool> {
    let source_path = source_io.resolve(Path::new(crate::storage::LEGACY_GUI_CONFIG_PATH));
    if !path_is_file(&source_path).await? {
        return Ok(false);
    }

    let source = read_legacy_gui_config_as_value(source_io).await?;
    if source.get("theme").is_none() {
        return Ok(false);
    }
    let mut source = serde_json::from_value::<ThemeConfig>(source)?;
    source.fix_defaults();

    let mut target = config.load_mut().await?;
    target.theme = source.theme;
    target.save().await?;
    Ok(true)
}

async fn import_alcomd3_beta_theme_config(
    source_io: &DefaultEnvironmentIo,
    config: &ThemeConfigState,
) -> io::Result<bool> {
    if import_theme_config_file(source_io, config, Path::new(BETA_THEME_CONFIG_PATH)).await? {
        return Ok(true);
    }

    import_gui_config(source_io, config).await
}

async fn import_theme_config_file(
    source_io: &DefaultEnvironmentIo,
    config: &ThemeConfigState,
    source_relative_path: &Path,
) -> io::Result<bool> {
    let source_path = source_io.resolve(source_relative_path);
    if !path_is_file(&source_path).await? {
        return Ok(false);
    }

    let source = tokio::fs::read(&source_path).await?;
    let mut source = serde_json::from_slice::<ThemeConfig>(&source)?;
    source.fix_defaults();

    let mut target = config.load_mut().await?;
    target.theme = source.theme;
    target.save().await?;
    Ok(true)
}

async fn import_gui_settings(
    source_io: &DefaultEnvironmentIo,
    config: &GuiConfigState,
) -> io::Result<bool> {
    let source_path = source_io.resolve(Path::new(crate::storage::LEGACY_GUI_CONFIG_PATH));
    if !path_is_file(&source_path).await? {
        return Ok(false);
    }

    let source = tokio::fs::read(&source_path).await?;
    let mut source = serde_json::from_slice::<GuiConfig>(&source)?;
    source.fix_defaults();

    let mut target = config.load_mut().await?;
    target.language = source.language;
    target.backup_format = source.backup_format;
    target.release_channel = source.release_channel;
    target.use_alcom_for_vcc_protocol = source.use_alcom_for_vcc_protocol;
    target.default_unity_arguments = source.default_unity_arguments;
    target.gui_animation = source.gui_animation;
    target.gui_compact = source.gui_compact;
    target.unity_hub_access_method = source.unity_hub_access_method;
    target.save().await?;
    Ok(true)
}

async fn copy_file_if_exists(
    source_io: &DefaultEnvironmentIo,
    target_io: &DefaultEnvironmentIo,
    relative_path: &Path,
) -> io::Result<bool> {
    copy_file_if_exists_to(source_io, target_io, relative_path, relative_path).await
}

async fn copy_file_if_exists_to(
    source_io: &DefaultEnvironmentIo,
    target_io: &DefaultEnvironmentIo,
    source_relative_path: &Path,
    target_relative_path: &Path,
) -> io::Result<bool> {
    let source_path = source_io.resolve(source_relative_path);
    if !path_is_file(&source_path).await? {
        return Ok(false);
    }

    let target_path = target_io.resolve(target_relative_path);
    create_parent_dir(&target_path).await?;
    tokio::fs::copy(source_path, target_path).await?;
    Ok(true)
}

async fn import_repositories(
    source_io: &DefaultEnvironmentIo,
    target_io: &DefaultEnvironmentIo,
) -> io::Result<bool> {
    let source_path = source_io.resolve(Path::new(REPO_CACHE_FOLDER));
    if !path_is_dir(&source_path).await? {
        return Ok(false);
    }

    let mut imported = false;
    let mut entries = tokio::fs::read_dir(source_path).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        let file_name = entry.file_name();

        if file_type.is_file() {
            if !is_repository_cache_file(&file_name) {
                continue;
            }

            let target_path = target_io.resolve(&Path::new(REPO_CACHE_FOLDER).join(&file_name));
            create_parent_dir(&target_path).await?;
            tokio::fs::copy(entry.path(), target_path).await?;
            imported = true;
        } else if file_type.is_dir() {
            imported |=
                import_legacy_package_cache_dir(&entry.path(), target_io, &file_name).await?;
        }
    }

    Ok(imported)
}

fn is_repository_cache_file(file_name: &OsStr) -> bool {
    let file_name = file_name.to_string_lossy();
    file_name.ends_with(".json") && !file_name.eq_ignore_ascii_case("package-cache.json")
}

async fn import_legacy_package_cache_dir(
    source_dir: &Path,
    target_io: &DefaultEnvironmentIo,
    package_folder_name: &OsStr,
) -> io::Result<bool> {
    let mut imported = false;
    let mut entries = tokio::fs::read_dir(source_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        let file_name = entry.file_name();
        if !file_type.is_file() || !is_package_cache_file(&file_name) {
            continue;
        }

        let target_path = target_io.resolve(
            &Path::new(PACKAGE_CACHE_FOLDER)
                .join(package_folder_name)
                .join(&file_name),
        );
        create_parent_dir(&target_path).await?;
        tokio::fs::copy(entry.path(), target_path).await?;
        imported = true;
    }

    Ok(imported)
}

fn is_package_cache_file(file_name: &OsStr) -> bool {
    let file_name = file_name.as_encoded_bytes();
    file_name.starts_with(b"vrc-get-")
        && (file_name.ends_with(b".zip") || file_name.ends_with(b".zip.sha256"))
}

async fn copy_dir_if_exists_to(
    source_io: &DefaultEnvironmentIo,
    target_io: &DefaultEnvironmentIo,
    source_relative_path: &Path,
    target_relative_path: &Path,
) -> io::Result<bool> {
    let source_path = source_io.resolve(source_relative_path);
    if !path_is_dir(&source_path).await? {
        return Ok(false);
    }

    copy_dir_recursive(&source_path, &target_io.resolve(target_relative_path)).await?;
    Ok(true)
}

async fn import_vcc_templates(
    source_io: &DefaultEnvironmentIo,
    target_io: &DefaultEnvironmentIo,
) -> io::Result<bool> {
    let source_templates_path = source_io.resolve(Path::new(VCC_TEMPLATES_PATH));
    if !path_is_dir(&source_templates_path).await? {
        return Ok(false);
    }

    let mut imported = false;
    let mut entries = tokio::fs::read_dir(source_templates_path).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        if !file_type.is_dir() {
            continue;
        }

        let Ok(template_name) = entry.file_name().into_string() else {
            log::warn!(
                "skipping legacy VCC template with non-utf8 name: {}",
                entry.path().display()
            );
            continue;
        };

        if !path_is_file(entry.path().join("package.json")).await? {
            continue;
        }

        let id = templates::imported_vcc_template_id(&template_name);
        let template =
            match templates::create_project_archive_template(&entry.path(), &template_name, id)
                .await
            {
                Ok(template) => template,
                Err(err) => {
                    log::warn!(
                        "failed to import legacy VCC template {}: {err}",
                        entry.path().display()
                    );
                    continue;
                }
            };

        let file_name = format!(
            "legacy-vcc-{}",
            templates::sanitize_template_file_stem(&template_name)
        );
        let target_path = Path::new(crate::storage::TEMPLATE_DIR)
            .join(file_name)
            .with_extension("alcomtemplate");
        target_io
            .create_dir_all(Path::new(crate::storage::TEMPLATE_DIR))
            .await?;
        target_io.write_atomic(&target_path, &template).await?;
        imported = true;
    }

    Ok(imported)
}

async fn copy_dir_recursive(source: &Path, target: &Path) -> io::Result<()> {
    tokio::fs::create_dir_all(target).await?;

    let mut pending = vec![(source.to_path_buf(), target.to_path_buf())];
    while let Some((source_dir, target_dir)) = pending.pop() {
        let mut entries = tokio::fs::read_dir(&source_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            let target_path = target_dir.join(entry.file_name());
            if file_type.is_dir() {
                tokio::fs::create_dir_all(&target_path).await?;
                pending.push((entry.path(), target_path));
            } else if file_type.is_file() {
                create_parent_dir(&target_path).await?;
                tokio::fs::copy(entry.path(), target_path).await?;
            }
        }
    }

    Ok(())
}

async fn create_parent_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    Ok(())
}

async fn path_is_file(path: impl AsRef<Path>) -> io::Result<bool> {
    Ok(tokio::fs::metadata(path)
        .await
        .map(|metadata| metadata.is_file())
        .unwrap_or(false))
}

async fn path_is_dir(path: impl AsRef<Path>) -> io::Result<bool> {
    Ok(tokio::fs::metadata(path)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vrc_get_vpm::environment::{PACKAGE_CACHE_FOLDER, REPO_CACHE_FOLDER};

    #[test]
    fn legacy_source_description_does_not_require_existing_directory() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(legacy_source_description_does_not_require_existing_directory_inner());
    }

    async fn legacy_source_description_does_not_require_existing_directory_inner() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("missing");
        let source_io = DefaultEnvironmentIo::new(source_root.clone().into_boxed_path());

        let source = describe_source(TauriLegacyDataSourceKind::Vcc, &source_io)
            .await
            .unwrap();
        assert_eq!(PathBuf::from(source.path), source_root);
    }

    #[test]
    fn alcomd3_beta_source_requires_beta_layout() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(alcomd3_beta_source_requires_beta_layout_inner());
    }

    async fn alcomd3_beta_source_requires_beta_layout_inner() {
        let temp = tempfile::tempdir().unwrap();
        let source_io = DefaultEnvironmentIo::new(temp.path().join("source").into_boxed_path());

        assert!(!alcomd3_beta_source_has_data(&source_io).await.unwrap());

        source_io
            .create_dir_all(Path::new("vrc-get"))
            .await
            .unwrap();
        source_io
            .write(Path::new(BETA_THEME_CONFIG_PATH), br#"{"theme":"system"}"#)
            .await
            .unwrap();

        assert!(alcomd3_beta_source_has_data(&source_io).await.unwrap());
    }

    #[test]
    fn alcomd3_beta_imports_old_runtime_layout() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(alcomd3_beta_imports_old_runtime_layout_inner());
    }

    async fn alcomd3_beta_imports_old_runtime_layout_inner() {
        let temp = tempfile::tempdir().unwrap();
        let io = DefaultEnvironmentIo::new(temp.path().join("alcomd3").into_boxed_path());
        let settings = SettingsState::new();
        let config = GuiConfigState::new_load(&io).await.unwrap();
        let theme_config = ThemeConfigState::new_load(&io).await.unwrap();
        let packages = PackagesState::new();
        let templates = TemplatesState::new();

        source_beta_runtime_layout(&io).await;

        let resources = import_legacy_data(
            &io,
            &settings,
            &config,
            &theme_config,
            &packages,
            &templates,
            TauriLegacyDataSourceKind::Alcomd3Beta,
            TauriLegacyDataImportCategory::Resources,
        )
        .await
        .unwrap();

        assert!(resources.imported_alcom_templates);
        assert!(resources.imported_vrc_get_settings);
        assert!(
            io.resolve(
                Path::new(crate::storage::TEMPLATE_DIR)
                    .join("custom.alcomtemplate")
                    .as_path()
            )
            .is_file()
        );
        assert!(io.resolve(Path::new(VRC_GET_SETTINGS_PATH)).is_file());

        let theme = import_legacy_data(
            &io,
            &settings,
            &config,
            &theme_config,
            &packages,
            &templates,
            TauriLegacyDataSourceKind::Alcomd3Beta,
            TauriLegacyDataImportCategory::Theme,
        )
        .await
        .unwrap();

        assert!(theme.imported_gui_config);
        assert_eq!(
            theme_config.get().theme,
            r##"material:{"sourceHex":"#112233"}"##
        );

        let gui_settings = import_legacy_data(
            &io,
            &settings,
            &config,
            &theme_config,
            &packages,
            &templates,
            TauriLegacyDataSourceKind::Alcomd3Beta,
            TauriLegacyDataImportCategory::Settings,
        )
        .await
        .unwrap();

        assert!(gui_settings.imported_gui_config);
        assert_eq!(config.get().language, "ja");
        assert!(config.get().gui_compact);
    }

    async fn source_beta_runtime_layout(io: &DefaultEnvironmentIo) {
        io.create_dir_all(Path::new(crate::storage::LEGACY_TEMPLATE_DIR))
            .await
            .unwrap();
        io.write(
            Path::new(BETA_THEME_CONFIG_PATH),
            br##"{"theme":"material:{\"sourceHex\":\"#112233\"}"}"##,
        )
        .await
        .unwrap();
        io.write(
            Path::new(crate::storage::LEGACY_GUI_CONFIG_PATH),
            br#"{"language":"ja","guiCompact":true}"#,
        )
        .await
        .unwrap();
        io.write(
            Path::new(LEGACY_VRC_GET_SETTINGS_PATH),
            br#"{"ignoreOfficialRepository":true}"#,
        )
        .await
        .unwrap();
        io.write(
            &Path::new(crate::storage::LEGACY_TEMPLATE_DIR).join("custom.alcomtemplate"),
            b"template",
        )
        .await
        .unwrap();
    }

    #[test]
    fn legacy_vcc_templates_are_imported_as_alcomd3_templates() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(legacy_vcc_templates_are_imported_as_alcomd3_templates_inner());
    }

    async fn legacy_vcc_templates_are_imported_as_alcomd3_templates_inner() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("source");
        let target_root = temp.path().join("target");
        let template_root = source_root.join("Templates/Legacy Template");
        tokio::fs::create_dir_all(template_root.join("Assets"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(template_root.join("Packages"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(template_root.join("ProjectSettings"))
            .await
            .unwrap();
        tokio::fs::write(template_root.join("package.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(template_root.join("Assets/Keep.txt"), "kept")
            .await
            .unwrap();
        tokio::fs::write(
            template_root.join("ProjectSettings/ProjectVersion.txt"),
            "m_EditorVersion: 2022.3.22f1\n",
        )
        .await
        .unwrap();

        let source_io = DefaultEnvironmentIo::new(source_root.into_boxed_path());
        let target_io = DefaultEnvironmentIo::new(target_root.into_boxed_path());

        assert!(import_vcc_templates(&source_io, &target_io).await.unwrap());
        assert!(
            !target_io
                .resolve(Path::new(crate::storage::LEGACY_TEMPLATE_DIR))
                .exists()
        );

        let mut entries =
            tokio::fs::read_dir(target_io.resolve(Path::new(crate::storage::TEMPLATE_DIR)))
                .await
                .unwrap();
        let entry = entries.next_entry().await.unwrap().unwrap();
        assert!(entries.next_entry().await.unwrap().is_none());

        let imported = tokio::fs::read(entry.path()).await.unwrap();
        let imported = templates::parse_alcom_template(&imported).unwrap();
        assert!(imported.is_project_archive());
    }

    #[test]
    fn legacy_repository_import_splits_package_cache_from_repositories() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(legacy_repository_import_splits_package_cache_from_repositories_inner());
    }

    async fn legacy_repository_import_splits_package_cache_from_repositories_inner() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("source");
        let target_root = temp.path().join("target");
        let source_io = DefaultEnvironmentIo::new(source_root.clone().into_boxed_path());
        let target_io = DefaultEnvironmentIo::new(target_root.clone().into_boxed_path());
        let repo_file = Path::new(REPO_CACHE_FOLDER).join("community.json");
        let legacy_cache_file = Path::new(REPO_CACHE_FOLDER)
            .join("com.example.package")
            .join("vrc-get-com.example.package-1.0.0.zip");
        let legacy_cache_sha = legacy_cache_file.with_extension("zip.sha256");

        source_io
            .create_dir_all(legacy_cache_file.parent().unwrap())
            .await
            .unwrap();
        source_io.write(&repo_file, b"{}").await.unwrap();
        source_io.write(&legacy_cache_file, b"cache").await.unwrap();
        source_io.write(&legacy_cache_sha, b"sha").await.unwrap();

        assert!(import_repositories(&source_io, &target_io).await.unwrap());

        assert!(target_io.resolve(&repo_file).is_file());
        assert!(!target_io.resolve(&legacy_cache_file).exists());
        assert!(
            target_io
                .resolve(
                    &Path::new(PACKAGE_CACHE_FOLDER)
                        .join("com.example.package")
                        .join("vrc-get-com.example.package-1.0.0.zip")
                )
                .is_file()
        );
    }

    #[test]
    fn legacy_gui_config_imports_theme_only() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(legacy_gui_config_imports_theme_only_inner());
    }

    async fn legacy_gui_config_imports_theme_only_inner() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("source");
        let target_root = temp.path().join("target");
        let source_io = DefaultEnvironmentIo::new(source_root.into_boxed_path());
        let target_io = DefaultEnvironmentIo::new(target_root.into_boxed_path());
        let config = GuiConfigState::new_load(&target_io).await.unwrap();
        let theme_config = ThemeConfigState::new_load(&target_io).await.unwrap();

        source_io
            .create_dir_all(Path::new("vrc-get"))
            .await
            .unwrap();
        source_io
            .write(
                Path::new(crate::storage::LEGACY_GUI_CONFIG_PATH),
                br##"{
                    "theme": "material:{\"sourceHex\":\"#ff3366\",\"mode\":\"dark\",\"scheme\":\"expressive\"}",
                    "setupProcessProgress": 31,
                    "mcpEnabled": true
                }"##,
            )
            .await
            .unwrap();

        assert!(import_gui_config(&source_io, &theme_config).await.unwrap());

        let imported = theme_config.get();
        assert_eq!(
            imported.theme,
            r##"material:{"sourceHex":"#ff3366","mode":"dark","scheme":"expressive"}"##
        );
        let gui_config = config.get();
        assert_eq!(gui_config.setup_process_progress, 0);
        assert!(!gui_config.mcp_enabled);
    }

    #[test]
    fn legacy_gui_config_without_theme_is_not_theme_importable() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(legacy_gui_config_without_theme_is_not_theme_importable_inner());
    }

    async fn legacy_gui_config_without_theme_is_not_theme_importable_inner() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("source");
        let target_root = temp.path().join("target");
        let source_io = DefaultEnvironmentIo::new(source_root.into_boxed_path());
        let target_io = DefaultEnvironmentIo::new(target_root.into_boxed_path());
        let theme_config = ThemeConfigState::new_load(&target_io).await.unwrap();

        source_io
            .create_dir_all(Path::new("vrc-get"))
            .await
            .unwrap();
        source_io
            .write(
                Path::new(crate::storage::LEGACY_GUI_CONFIG_PATH),
                br#"{
                    "language": "ja",
                    "guiCompact": true
                }"#,
            )
            .await
            .unwrap();

        assert!(!import_gui_config(&source_io, &theme_config).await.unwrap());
        assert_eq!(theme_config.get().theme, "system");
    }

    #[test]
    fn legacy_gui_settings_imports_settings_only() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(legacy_gui_settings_imports_settings_only_inner());
    }

    async fn legacy_gui_settings_imports_settings_only_inner() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("source");
        let target_root = temp.path().join("target");
        let source_io = DefaultEnvironmentIo::new(source_root.into_boxed_path());
        let target_io = DefaultEnvironmentIo::new(target_root.into_boxed_path());
        let config = GuiConfigState::new_load(&target_io).await.unwrap();
        let theme_config = ThemeConfigState::new_load(&target_io).await.unwrap();

        source_io
            .create_dir_all(Path::new("vrc-get"))
            .await
            .unwrap();
        source_io
            .write(
                Path::new(crate::storage::LEGACY_GUI_CONFIG_PATH),
                br##"{
                    "language": "ja",
                    "theme": "material:{\"sourceHex\":\"#ff3366\",\"mode\":\"dark\",\"scheme\":\"expressive\"}",
                    "backupFormat": "zip-best-compression",
                    "releaseChannel": "beta",
                    "useAlcomForVccProtocol": false,
                    "defaultUnityArguments": ["-custom-argument"],
                    "guiAnimation": false,
                    "guiCompact": true,
                    "mcpEnabled": true,
                    "setupProcessProgress": 31,
                    "unityHubAccessMethod": "CallHub",
                    "excludeVpmPackagesFromBackup": true
                }"##,
            )
            .await
            .unwrap();

        assert!(import_gui_settings(&source_io, &config).await.unwrap());

        let imported = config.get();
        assert_eq!(imported.language, "ja");
        assert_eq!(imported.backup_format, "zip-best-compression");
        assert_eq!(imported.release_channel, "beta");
        assert!(!imported.use_alcom_for_vcc_protocol);
        assert_eq!(
            imported.default_unity_arguments,
            Some(vec!["-custom-argument".to_string()])
        );
        assert!(!imported.gui_animation);
        assert!(imported.gui_compact);
        assert!(matches!(
            imported.unity_hub_access_method,
            crate::config::UnityHubAccessMethod::CallHub
        ));
        assert_eq!(imported.setup_process_progress, 0);
        assert!(!imported.mcp_enabled);

        let imported_theme = theme_config.get();
        assert_eq!(imported_theme.theme, "system");
    }

    #[test]
    fn legacy_resource_settings_import_rewrites_repo_paths_only() {
        let legacy_root = Path::new("C:/Users/Example/AppData/Local/VRChatCreatorCompanion");
        let target_root = Path::new("C:/Users/Example/AppData/Local/ALCOMD3");
        let source = serde_json::json!({
            "defaultProjectPath": "C:/Users/Example/ALCOM/Projects",
            "projectBackupPath": "C:/Users/Example/ALCOM/Backups",
            "userRepos": [
                {
                    "localPath": "C:/Users/Example/AppData/Local/VRChatCreatorCompanion/Repos/community.json",
                    "url": "https://example.com/index.json",
                    "id": "community"
                },
                {
                    "localPath": "D:/Repos/local.json",
                    "id": "local"
                }
            ],
            "userPackageFolders": ["D:/LocalPackages"]
        });

        let (imported, changed) = merge_legacy_settings_for_import(
            source,
            Value::Object(Map::new()),
            LegacySettingsCategory::Resources,
            legacy_root,
            target_root,
        );

        assert!(changed);
        assert!(imported.get("defaultProjectPath").is_none());
        assert!(imported.get("projectBackupPath").is_none());
        assert_eq!(
            PathBuf::from(imported["userRepos"][0]["localPath"].as_str().unwrap()),
            target_root.join(REPO_CACHE_FOLDER).join("community.json")
        );
        assert_eq!(imported["userRepos"][1]["localPath"], "D:/Repos/local.json");
        assert_eq!(imported["userPackageFolders"][0], "D:/LocalPackages");
    }

    #[test]
    fn legacy_app_settings_imports_settings_tab_paths_and_keeps_resources() {
        let legacy_root = Path::new("C:/Users/Example/AppData/Local/VRChatCreatorCompanion");
        let target_root = Path::new("C:/Users/Example/AppData/Local/ALCOMD3");
        let source = serde_json::json!({
            "defaultProjectPath": "C:/Users/Example/ALCOM/Projects",
            "projectBackupPath": "C:/Users/Example/ALCOM/Backups",
            "showPrereleasePackages": true,
            "skipRequirements": true,
            "userRepos": [
                {
                    "localPath": "C:/Users/Example/AppData/Local/VRChatCreatorCompanion/Repos/community.json",
                    "url": "https://example.com/index.json"
                }
            ]
        });
        let target = serde_json::json!({
            "userRepos": [
                {
                    "localPath": "C:/Users/Example/AppData/Local/ALCOMD3/Repos/existing.json",
                    "url": "https://example.com/existing.json"
                }
            ]
        });

        let (imported, changed) = merge_legacy_settings_for_import(
            source,
            target,
            LegacySettingsCategory::Settings,
            legacy_root,
            target_root,
        );

        assert!(changed);
        assert_eq!(
            imported["defaultProjectPath"],
            "C:/Users/Example/ALCOM/Projects"
        );
        assert_eq!(
            imported["projectBackupPath"],
            "C:/Users/Example/ALCOM/Backups"
        );
        assert_eq!(imported["showPrereleasePackages"], true);
        assert_eq!(imported["skipRequirements"], true);
        assert_eq!(
            imported["userRepos"][0]["localPath"],
            "C:/Users/Example/AppData/Local/ALCOMD3/Repos/existing.json"
        );
    }

    #[test]
    fn legacy_project_settings_import_skips_resource_settings() {
        let legacy_root = Path::new("C:/Users/Example/AppData/Local/VRChatCreatorCompanion");
        let target_root = Path::new("C:/Users/Example/AppData/Local/ALCOMD3");
        let source = serde_json::json!({
            "userProjects": ["C:/Users/Example/Documents/Avatar"],
            "unityEditors": ["C:/Program Files/Unity/Editor/Unity.exe"],
            "pathToUnityHub": "C:/Program Files/Unity Hub/Unity Hub.exe",
            "userRepos": [
                {
                    "localPath": "C:/Users/Example/AppData/Local/VRChatCreatorCompanion/Repos/community.json"
                }
            ]
        });

        let (imported, changed) = merge_legacy_settings_for_import(
            source,
            Value::Object(Map::new()),
            LegacySettingsCategory::Projects,
            legacy_root,
            target_root,
        );

        assert!(changed);
        assert_eq!(
            imported["userProjects"][0],
            "C:/Users/Example/Documents/Avatar"
        );
        assert_eq!(
            imported["unityEditors"][0],
            "C:/Program Files/Unity/Editor/Unity.exe"
        );
        assert_eq!(
            imported["pathToUnityHub"],
            "C:/Program Files/Unity Hub/Unity Hub.exe"
        );
        assert!(imported.get("userRepos").is_none());
    }

    #[test]
    fn legacy_import_result_counts_imported_sections() {
        let result = TauriLegacyDataImportResult {
            imported_settings: true,
            imported_database: false,
            imported_repositories: true,
            imported_vcc_templates: false,
            imported_alcom_templates: false,
            imported_vrc_get_settings: false,
            imported_gui_config: true,
        };

        assert_eq!(result.imported_count(), 3);
    }

    #[test]
    fn legacy_import_activity_describes_source_and_category() {
        let details = legacy_import_result_details(
            TauriLegacyDataSourceKind::Vcc,
            TauriLegacyDataImportCategory::Projects,
            &TauriLegacyDataImportResult::default(),
        );

        assert_eq!(
            legacy_import_source_label(TauriLegacyDataSourceKind::Vcc),
            "VCC"
        );
        assert_eq!(
            legacy_import_category_label(TauriLegacyDataImportCategory::Projects),
            "Projects"
        );
        assert!(details.contains(&ActivityDetail::new("source", "VCC")));
        assert!(details.contains(&ActivityDetail::new("category", "Projects")));
        assert!(details.contains(&ActivityDetail::new("imported", "0")));
    }
}
