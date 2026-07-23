use crate::activity_log::{
    ActivityDetail, ActivityImportance, ActivityInput, ActivityKind, ActivityLogState,
    ActivitySource, operations, summarize_path, target_from_path,
};
use crate::backend::projects::project_summary_snapshot;
use crate::commands::prelude::*;
use std::cmp::Reverse;

use crate::commands::async_command::{
    AsyncCallResult, AsyncCommandContext, With, async_command_with_cancel_state,
};
use crate::templates;
use crate::templates::{CreateProjectErr, ProjectTemplateInfo};
use crate::utils::{
    FileSystemTree, collect_notable_project_files_tree, default_project_path,
    find_existing_parent_dir_or_home, project_backup_path, trash_delete,
};
use async_zip::base::read::seek::ZipFileReader;
use futures::future::{join_all, try_join_all};
use futures::prelude::*;
use itertools::Itertools;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::atomic::AtomicUsize;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use tauri_plugin_dialog::DialogExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Semaphore;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use vrc_get_vpm::AbortCheck;
use vrc_get_vpm::ProjectType;
use vrc_get_vpm::environment::{
    InvalidRealProjectInformation, PackageInstaller, RealProjectInformation, Settings, UserProject,
    ValidRealProjectInformation, VccDatabaseConnection,
};
use vrc_get_vpm::io::{DefaultEnvironmentIo, DefaultProjectIo};
use vrc_get_vpm::version::UnityVersion;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct TauriProject {
    // projet information
    name: String,
    path: String,
    project_type: TauriProjectType,
    unity: String,
    unity_revision: Option<String>,
    last_modified: i64,
    created_at: i64,
    favorite: bool,
    is_exists: bool,
    is_valid: Option<bool>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct TauriUpdatedRealProjectInfo {
    // project information
    path: String,
    is_valid: bool,
    project_type: TauriProjectType,
    unity: String,
    unity_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
enum TauriProjectType {
    Unknown,
    LegacySdk2,
    LegacyWorlds,
    LegacyAvatars,
    UpmWorlds,
    UpmAvatars,
    UpmStarter,
    Worlds,
    Avatars,
    VpmStarter,
}

impl From<ProjectType> for TauriProjectType {
    fn from(value: ProjectType) -> Self {
        match value {
            ProjectType::Unknown => Self::Unknown,
            ProjectType::LegacySdk2 => Self::LegacySdk2,
            ProjectType::LegacyWorlds => Self::LegacyWorlds,
            ProjectType::LegacyAvatars => Self::LegacyAvatars,
            ProjectType::UpmWorlds => Self::UpmWorlds,
            ProjectType::UpmAvatars => Self::UpmAvatars,
            ProjectType::UpmStarter => Self::UpmStarter,
            ProjectType::Worlds => Self::Worlds,
            ProjectType::Avatars => Self::Avatars,
            ProjectType::VpmStarter => Self::VpmStarter,
        }
    }
}

impl TauriProject {
    fn new(project: &UserProject) -> Self {
        let snapshot = project_summary_snapshot(project)
            .expect("environment_projects filters out projects without path");
        Self {
            name: snapshot
                .name
                .expect("environment_projects expects projects to have a name"),
            path: snapshot.path,
            project_type: snapshot.project_type.into(),
            unity: snapshot.unity.unwrap_or_else(|| "unknown".into()),
            unity_revision: snapshot.unity_revision,
            last_modified: snapshot.last_modified.unwrap_or(0),
            created_at: snapshot.created_at.unwrap_or(0),
            favorite: snapshot.favorite,
            is_exists: snapshot.exists,
            is_valid: snapshot.is_valid,
        }
    }
}

impl TauriUpdatedRealProjectInfo {
    fn new(project: &ValidRealProjectInformation) -> Self {
        Self {
            path: project.path().into(),
            is_valid: true,
            project_type: project.project_type().into(),
            unity: project.unity_version().to_string(),
            unity_revision: project.unity_revision().map(Into::into),
        }
    }

    fn new_invalid(path: String) -> Self {
        Self {
            path,
            is_valid: false,
            project_type: TauriProjectType::Unknown,
            unity: String::new(),
            unity_revision: None,
        }
    }
}

async fn migrate_sanitize_projects(
    connection: &mut VccDatabaseConnection,
    io: &DefaultEnvironmentIo,
    settings: &Settings,
) -> io::Result<()> {
    info!("migrating projects from settings.json");
    // migrate from settings json
    connection.migrate(settings, io).await?;
    connection.dedup_projects();
    connection.normalize_path();
    Ok(())
}

fn sync_with_real_project_background(projects: &[UserProject], app: &AppHandle) {
    static LAST_UPDATE: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
    // update after one minutes.
    let mut lock = LAST_UPDATE.lock().unwrap_or_else(|mut e| {
        **e.get_mut() = None;
        e.into_inner()
    });
    if lock
        .map(|x| x.elapsed() > std::time::Duration::from_secs(60))
        .unwrap_or(true)
    {
        *lock = Some(Instant::now());
        // start update thread
        log::info!("starting sync with real project...");
        tauri::async_runtime::spawn(sync_with_real_project(
            projects
                .iter()
                .map(|x| x.path().unwrap().to_string())
                .collect(),
            app.clone(),
        ));
    } else {
        log::info!("skipped sync with real project since last update was less than 1 minute ago");
    }

    async fn sync_with_real_project(projects: Vec<String>, app: AppHandle) {
        app.emit("projects-update-in-progress", true).ok();
        let project_count = projects.len();
        let activity_tracker = app.try_state::<ActivityLogState>().map(|activity| {
            activity.start_activity(
                Some(&app),
                ActivityInput::new(
                    ActivitySource::System,
                    ActivityKind::Passive,
                    ActivityImportance::Secondary,
                    operations::PROJECTS_SYNC_REAL_INFO,
                    "Synchronizing project information in background",
                )
                .details(vec![ActivityDetail::new(
                    "projects",
                    project_count.to_string(),
                )]),
            )
        });

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        info!(
            "loading real project information of {} projects",
            projects.len()
        );

        let io = app.state::<DefaultEnvironmentIo>();

        let projects = join_all(projects.into_iter().map(async |project| {
            match ValidRealProjectInformation::load_from_fs(&io, project.to_owned()).await {
                Ok(Some(project)) => {
                    app.emit(
                        "projects-updated",
                        TauriUpdatedRealProjectInfo::new(&project),
                    )
                    .ok();
                    RealProjectInformation::Valid(project)
                }
                Ok(None) => {
                    app.emit(
                        "projects-updated",
                        TauriUpdatedRealProjectInfo::new_invalid(project.clone()),
                    )
                    .ok();
                    RealProjectInformation::Invalid(InvalidRealProjectInformation::new(project))
                }
                Err(err) => {
                    app.emit(
                        "projects-updated",
                        TauriUpdatedRealProjectInfo::new_invalid(project.clone()),
                    )
                    .ok();
                    error!(gui_toast = false; "Error updating project information of {project}: {err}");
                    RealProjectInformation::Invalid(InvalidRealProjectInformation::new(project))
                }
            }
        }))
        .await;
        app.emit("projects-update-in-progress", false).ok();
        let invalid_count = projects
            .iter()
            .filter(|project| matches!(project, RealProjectInformation::Invalid(_)))
            .count();

        info!(
            "updating database real project information of {} projects",
            projects.len()
        );

        let mut connection = match VccDatabaseConnection::connect(io.inner()).await {
            Ok(connection) => connection,
            Err(e) => {
                error!("Error opening database: {e}");
                if let Some(activity_tracker) = &activity_tracker
                    && let Some(activity) = app.try_state::<ActivityLogState>()
                {
                    activity.finish_failed(
                        Some(&app),
                        activity_tracker,
                        "Project information synchronization failed",
                        Vec::new(),
                        e,
                    );
                }
                return;
            }
        };
        connection.sync_with_real_projects_information(projects);
        match connection.save(io.inner()).await {
            Ok(()) => {}
            Err(e) => {
                error!("Error updating database: {e}");
                if let Some(activity_tracker) = &activity_tracker
                    && let Some(activity) = app.try_state::<ActivityLogState>()
                {
                    activity.finish_failed(
                        Some(&app),
                        activity_tracker,
                        "Project information synchronization failed",
                        Vec::new(),
                        e,
                    );
                }
                return;
            }
        }

        if let Some(activity_tracker) = &activity_tracker
            && let Some(activity) = app.try_state::<ActivityLogState>()
        {
            activity.finish_success(
                Some(&app),
                activity_tracker,
                "Project information synchronized",
                vec![
                    ActivityDetail::new("projects", project_count.to_string()),
                    ActivityDetail::new("invalidProjects", invalid_count.to_string()),
                ],
            );
        }
        info!("updated database based on real project information");
    }
}

#[tauri::command]
#[specta::specta]
pub async fn environment_projects(
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    app: AppHandle,
) -> Result<Vec<TauriProject>, RustError> {
    let mut settings = settings.load_mut(io.inner()).await?;
    let mut connection = VccDatabaseConnection::connect(io.inner()).await?;

    migrate_sanitize_projects(&mut connection, io.inner(), &settings).await?;
    settings.load_from_db(&connection)?;
    connection.save(io.inner()).await?;
    settings.save().await?;

    info!("fetching projects");

    let mut projects = connection.get_projects();
    projects.retain(|x| x.path().is_some());

    sync_with_real_project_background(&projects, &app);

    let vec = projects.iter().map(TauriProject::new).collect::<Vec<_>>();

    Ok(vec)
}

#[derive(Serialize, specta::Type)]
pub enum TauriAddProjectWithPickerResult {
    NoFolderSelected,
    InvalidSelection,
    AlreadyAdded,
    Successful,
}

#[derive(Serialize, specta::Type, Clone)]
pub enum TauriRestoreProjectFromBackupResult {
    InvalidSelection,
    AlreadyExists,
    AlreadyAdded,
    Successful,
}

#[derive(Serialize, specta::Type)]
#[serde(tag = "type")]
pub enum TauriPickProjectBackupForRestoreResult {
    NoFileSelected,
    InvalidSelection,
    Successful {
        backup_path: String,
        project_name: String,
        project_location: String,
    },
}

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriRestoreProjectFromBackupProgress {
    total: usize,
    proceed: usize,
    last_proceed: String,
}

fn gui_project_activity(
    operation: &'static str,
    summary: impl Into<String>,
    project_path: impl AsRef<Path>,
) -> ActivityInput {
    let project_path = project_path.as_ref();
    ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operation,
        summary,
    )
    .target(target_from_path(project_path))
    .details(vec![ActivityDetail::new(
        "projectPath",
        summarize_path(project_path),
    )])
}

#[tauri::command]
#[specta::specta]
pub async fn environment_add_project_with_picker(
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    window: Window,
) -> Result<TauriAddProjectWithPickerResult, RustError> {
    let app = window.app_handle().clone();
    let activity = app.state::<ActivityLogState>();
    let tracker = activity.start_activity(
        Some(&app),
        ActivityInput::new(
            ActivitySource::Gui,
            ActivityKind::Write,
            ActivityImportance::Primary,
            operations::PROJECT_ADD,
            "Adding project from folder picker",
        ),
    );

    let setup_result = async {
        let mut environment_settings = settings.load_mut(io.inner()).await?;
        let project_dir = default_project_path(&mut environment_settings).to_string();
        environment_settings.maybe_save().await?;
        Ok::<_, RustError>(project_dir)
    }
    .await;
    let project_dir = match setup_result {
        Ok(project_dir) => project_dir,
        Err(error) => {
            activity.finish_failed(
                Some(&app),
                &tracker,
                "Project add setup failed",
                Vec::new(),
                &error,
            );
            return Err(error);
        }
    };

    let Some(project_paths) = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_directory(find_existing_parent_dir_or_home(Path::new(&project_dir)))
        .blocking_pick_folders()
    else {
        activity.finish_cancelled(Some(&app), &tracker, "Project add cancelled", Vec::new());
        return Ok(TauriAddProjectWithPickerResult::NoFolderSelected);
    };

    let Ok(project_paths) = project_paths
        .into_iter()
        .map(|x| x.into_path_buf().map_err(|_| ()))
        .map_ok(|x| x.into_os_string().into_string().map_err(|_| ()))
        .flatten_ok()
        .collect::<Result<Vec<_>, ()>>()
    else {
        activity.finish_failed(
            Some(&app),
            &tracker,
            "Selected project folder was invalid",
            Vec::new(),
            "selected path is not valid unicode",
        );
        return Ok(TauriAddProjectWithPickerResult::InvalidSelection);
    };

    let selected_count = project_paths.len();
    let unity_projects = match try_join_all(
        project_paths
            .into_iter()
            .map(|path| UnityProject::load(DefaultProjectIo::new(PathBuf::from(path).into()))),
    )
    .await
    {
        Ok(unity_projects) => unity_projects,
        Err(e) => {
            error!(gui_toast = false; "Error loading project: {e}");
            activity.finish_failed(
                Some(&app),
                &tracker,
                "Selected project folder was invalid",
                Vec::new(),
                e,
            );
            return Ok(TauriAddProjectWithPickerResult::InvalidSelection);
        }
    };

    let result = async {
        let mut settings = settings.load_mut(io.inner()).await?;
        let mut connection = VccDatabaseConnection::connect(io.inner()).await?;
        migrate_sanitize_projects(&mut connection, io.inner(), &settings).await?;

        let projects = connection.get_projects();
        if (projects.iter().cartesian_product(unity_projects.iter()))
            .any(|(in_db, adding)| in_db.path().map(Path::new) == Some(adding.project_dir()))
        {
            return Ok(TauriAddProjectWithPickerResult::AlreadyAdded);
        }
        for unity_project in unity_projects {
            connection.add_project(&unity_project).await?;
        }
        connection.save(io.inner()).await?;
        settings.load_from_db(&connection)?;
        settings.save().await?;

        Ok(TauriAddProjectWithPickerResult::Successful)
    }
    .await;

    match &result {
        Ok(TauriAddProjectWithPickerResult::Successful) => {
            activity.finish_success(
                Some(&app),
                &tracker,
                "Project added",
                vec![ActivityDetail::new(
                    "selectedFolders",
                    selected_count.to_string(),
                )],
            );
        }
        Ok(TauriAddProjectWithPickerResult::AlreadyAdded) => {
            activity.finish_info(
                Some(&app),
                &tracker,
                "Project was already in the project list",
                vec![ActivityDetail::new(
                    "selectedFolders",
                    selected_count.to_string(),
                )],
            );
        }
        Ok(TauriAddProjectWithPickerResult::NoFolderSelected) => {}
        Ok(TauriAddProjectWithPickerResult::InvalidSelection) => {}
        Err(error) => {
            activity.finish_failed(
                Some(&app),
                &tracker,
                "Project add failed",
                vec![ActivityDetail::new(
                    "selectedFolders",
                    selected_count.to_string(),
                )],
                error,
            );
        }
    }

    result
}

#[tauri::command]
#[specta::specta]
pub async fn environment_pick_project_backup_for_restore(
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    window: Window,
) -> Result<TauriPickProjectBackupForRestoreResult, RustError> {
    let app = window.app_handle().clone();
    let activity = app.state::<ActivityLogState>();
    let mut environment_settings = settings.load_mut(io.inner()).await?;
    let backup_dir = project_backup_path(&mut environment_settings).to_string();
    let project_dir = default_project_path(&mut environment_settings).to_string();
    environment_settings.maybe_save().await?;

    let Some(backup_path) = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_directory(find_existing_parent_dir_or_home(Path::new(&backup_dir)))
        .add_filter("Zip Archive", &["zip"])
        .blocking_pick_file()
        .map(|x| x.into_path_buf())
        .transpose()?
    else {
        activity.record_info(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Open,
                ActivityImportance::Secondary,
                operations::PROJECT_RESTORE,
                "Project restore file picker cancelled",
            ),
        );
        return Ok(TauriPickProjectBackupForRestoreResult::NoFileSelected);
    };

    let Some(project_name) = backup_project_name(&backup_path) else {
        activity.record_failed(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityImportance::Primary,
                operations::PROJECT_RESTORE,
                "Selected backup archive was invalid",
            )
            .target(target_from_path(&backup_path))
            .details(vec![ActivityDetail::new(
                "backupArchive",
                summarize_path(&backup_path),
            )]),
            "backup archive path has no valid file name",
        );
        return Ok(TauriPickProjectBackupForRestoreResult::InvalidSelection);
    };

    let backup_path = match backup_path.into_os_string().into_string() {
        Ok(path) => path,
        Err(_) => {
            activity.record_failed(
                Some(&app),
                ActivityInput::new(
                    ActivitySource::Gui,
                    ActivityKind::Open,
                    ActivityImportance::Secondary,
                    operations::PROJECT_RESTORE,
                    "Selected backup archive path was invalid",
                ),
                "backup archive path is not valid unicode",
            );
            return Ok(TauriPickProjectBackupForRestoreResult::InvalidSelection);
        }
    };

    Ok(TauriPickProjectBackupForRestoreResult::Successful {
        backup_path,
        project_name,
        project_location: project_dir,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn environment_restore_project_from_backup(
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    project_restore: State<'_, ProjectRestoreState>,
    window: Window,
    channel: String,
    backup_path: String,
    project_name: String,
) -> Result<
    AsyncCallResult<TauriRestoreProjectFromBackupProgress, TauriRestoreProjectFromBackupResult>,
    RustError,
> {
    let app = window.app_handle().clone();
    let activity = app.state::<ActivityLogState>();
    let mut environment_settings = settings.load_mut(io.inner()).await?;
    let project_dir = default_project_path(&mut environment_settings).to_string();
    environment_settings.maybe_save().await?;

    let backup_path = PathBuf::from(backup_path);
    let project_name = validate_project_folder_name("project_name", &project_name)?;
    let restore_path = Path::new(&project_dir).join(&project_name);
    let tracker = activity.start_activity(
        Some(&app),
        ActivityInput::new(
            ActivitySource::Gui,
            ActivityKind::Write,
            ActivityImportance::Primary,
            operations::PROJECT_RESTORE,
            "Restoring project from backup",
        )
        .target(target_from_path(&restore_path))
        .details(vec![
            ActivityDetail::new("projectPath", summarize_path(&restore_path)),
            ActivityDetail::new("backupArchive", summarize_path(&backup_path)),
        ]),
    );

    match tokio::fs::try_exists(&restore_path).await {
        Ok(true) => {
            activity.finish_failed(
                Some(&app),
                &tracker,
                "Project restore target already exists",
                Vec::new(),
                "restore target already exists",
            );
            return Ok(AsyncCallResult::Result {
                value: TauriRestoreProjectFromBackupResult::AlreadyExists,
            });
        }
        Ok(false) => {}
        Err(error) => {
            activity.finish_failed(
                Some(&app),
                &tracker,
                "Project restore precheck failed",
                Vec::new(),
                &error,
            );
            return Err(error.into());
        }
    }

    let restore_path_string = match restore_path.as_os_str().to_str() {
        Some(path) => path.to_string(),
        None => {
            let error = RustError::unrecoverable_str("restore path is not a valid unicode string");
            activity.finish_failed(
                Some(&app),
                &tracker,
                "Project restore precheck failed",
                Vec::new(),
                &error,
            );
            return Err(error);
        }
    };

    let project_restore = project_restore.inner().clone();
    if !project_restore.try_start_uncancellable() {
        activity.finish_failed(
            Some(&app),
            &tracker,
            "Project restore could not start",
            Vec::new(),
            "project restore is already running",
        );
        return Err(localizable_error!("projects:toast:restore already running"));
    }

    if let Err(error) = super::super::create_dir_all_with_err(&project_dir).await {
        project_restore.finish();
        activity.finish_failed(
            Some(&app),
            &tracker,
            "Project restore could not create parent folder",
            Vec::new(),
            &error,
        );
        return Err(error);
    }
    if let Err(error) = tokio::fs::create_dir(&restore_path).await {
        project_restore.finish();
        activity.finish_failed(
            Some(&app),
            &tracker,
            "Project restore could not create target folder",
            Vec::new(),
            &error,
        );
        return Err(error.into());
    }

    let project_restore_start = project_restore.clone();
    let project_restore_finish = project_restore.clone();
    let async_tracker = tracker.clone();
    let async_app = app.clone();
    let finish_tracker = tracker.clone();
    let finish_app = app.clone();
    let result = async_command_with_cancel_state(
        channel,
        window,
        async {
            With::<TauriRestoreProjectFromBackupProgress>::continue_async(move |ctx| async move {
                let backup_path = PathBuf::from(backup_path);
                let restore_path = PathBuf::from(restore_path_string);
                let remove_on_drop = RemoveDirOnDrop::new(&restore_path);

                let outcome: Result<TauriRestoreProjectFromBackupResult, RustError> = async {
                    extract_backup_zip(&backup_path, &restore_path, &ctx).await?;

                    let unity_project = match UnityProject::load(DefaultProjectIo::new(
                        restore_path.clone().into_boxed_path(),
                    ))
                    .await
                    {
                        Ok(unity_project) => unity_project,
                        Err(e) => {
                            error!(gui_toast = false; "Error loading restored project: {e}");
                            return Ok(TauriRestoreProjectFromBackupResult::InvalidSelection);
                        }
                    };

                    let settings = ctx.state::<SettingsState>();
                    let io = ctx.state::<DefaultEnvironmentIo>();

                    {
                        let mut settings = settings.load_mut(io.inner()).await?;
                        let mut connection = VccDatabaseConnection::connect(io.inner()).await?;
                        migrate_sanitize_projects(&mut connection, io.inner(), &settings).await?;

                        let projects = connection.get_projects();
                        if projects.iter().any(|in_db| {
                            in_db.path().map(Path::new) == Some(unity_project.project_dir())
                        }) {
                            return Ok(TauriRestoreProjectFromBackupResult::AlreadyAdded);
                        }

                        connection.add_project(&unity_project).await?;
                        connection.save(io.inner()).await?;
                        settings.load_from_db(&connection)?;
                        settings.save().await?;
                    }

                    remove_on_drop.forget();
                    Ok(TauriRestoreProjectFromBackupResult::Successful)
                }
                .await;

                if let Some(activity) = async_app.try_state::<ActivityLogState>() {
                    match &outcome {
                        Ok(TauriRestoreProjectFromBackupResult::Successful) => {
                            activity.finish_success(
                                Some(&async_app),
                                &async_tracker,
                                "Project restored from backup",
                                Vec::new(),
                            );
                        }
                        Ok(TauriRestoreProjectFromBackupResult::AlreadyAdded) => {
                            activity.finish_failed(
                                Some(&async_app),
                                &async_tracker,
                                "Restored project was already in the project list",
                                Vec::new(),
                                "restored project is already added",
                            );
                        }
                        Ok(TauriRestoreProjectFromBackupResult::InvalidSelection) => {
                            activity.finish_failed(
                                Some(&async_app),
                                &async_tracker,
                                "Restored project was invalid",
                                Vec::new(),
                                "restored files did not contain a valid Unity project",
                            );
                        }
                        Ok(TauriRestoreProjectFromBackupResult::AlreadyExists) => {}
                        Err(error) => {
                            activity.finish_failed(
                                Some(&async_app),
                                &async_tracker,
                                "Project restore failed",
                                Vec::new(),
                                error,
                            );
                        }
                    }
                }

                outcome
            })
        },
        move |abort| project_restore_start.start(abort),
        move || {
            project_restore_finish.finish();
            if let Some(activity) = finish_app.try_state::<ActivityLogState>() {
                activity.finish_cancelled(
                    Some(&finish_app),
                    &finish_tracker,
                    "Project restore cancelled",
                    Vec::new(),
                );
            }
        },
    )
    .await;
    if result.is_err() {
        project_restore.finish();
        if let Err(error) = &result {
            activity.finish_failed(
                Some(&app),
                &tracker,
                "Project restore failed to start",
                Vec::new(),
                error,
            );
        }
    }
    result
}

struct RemoveDirOnDrop(PathBuf);

impl RemoveDirOnDrop {
    fn new(path: impl AsRef<Path>) -> Self {
        Self(path.as_ref().to_path_buf())
    }

    fn forget(self) {
        std::mem::forget(self);
    }
}

impl Drop for RemoveDirOnDrop {
    fn drop(&mut self) {
        let path = self.0.clone();
        std::thread::spawn(move || {
            let _ = std::fs::remove_dir_all(path);
        });
    }
}

fn backup_project_name(backup_path: &Path) -> Option<String> {
    Some(backup_path.file_stem()?.to_str()?.to_string())
}

async fn extract_backup_zip(
    backup_path: &Path,
    restore_path: &Path,
    ctx: &AsyncCommandContext<TauriRestoreProjectFromBackupProgress>,
) -> Result<(), RustError> {
    extract_backup_zip_with_progress(backup_path, restore_path, move |progress| {
        let _ = ctx.emit(progress);
    })
    .await
}

async fn extract_backup_zip_with_progress<F>(
    backup_path: &Path,
    restore_path: &Path,
    progress: F,
) -> Result<(), RustError>
where
    F: Fn(TauriRestoreProjectFromBackupProgress) + Clone + Send + Sync,
{
    let archive = tokio::fs::File::open(backup_path).await?;
    let archive = BufReader::new(archive).compat();
    let mut reader = ZipFileReader::new(archive).await?;
    let total = reader.file().entries().len();

    progress(TauriRestoreProjectFromBackupProgress {
        total,
        proceed: 0,
        last_proceed: "Reading backup archive".to_string(),
    });

    for index in 0..total {
        let entry = &reader.file().entries()[index];
        let Some(filename) = entry.filename().as_str().ok() else {
            return Err(RustError::unrecoverable_str(
                "path in backup archive is not utf8",
            ));
        };
        let filename = fix_zip_path_separator(filename);
        let last_proceed = filename.to_string();
        let entry_is_dir = entry.dir()?;
        let filename_path = Path::new(filename.as_ref());
        if !is_complete_relative(filename_path) {
            return Err(RustError::unrecoverable_str(
                "directory traversal detected in backup archive",
            ));
        }

        let path = restore_path.join(filename_path);
        if entry_is_dir {
            tokio::fs::create_dir_all(&path).await?;
        } else {
            let parent = path.parent().ok_or_else(|| {
                RustError::unrecoverable_str("file entry in backup archive has no parent")
            })?;
            tokio::fs::create_dir_all(parent).await?;
            let mut entry_reader = reader.reader_without_entry(index).await?;
            let writer = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .await?;
            futures::io::copy(&mut entry_reader, &mut writer.compat_write()).await?;
        }

        progress(TauriRestoreProjectFromBackupProgress {
            total,
            proceed: index + 1,
            last_proceed,
        });
    }

    Ok(())
}

pub(crate) async fn restore_project_from_zip_backup(
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    backup_path: String,
    project_name: Option<String>,
    progress: impl Fn(TauriRestoreProjectFromBackupProgress) + Clone + Send + Sync,
) -> Result<String, RustError> {
    let backup_path = PathBuf::from(backup_path);
    ensure_mcp_absolute_path("backup_path", &backup_path)?;
    if backup_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("zip"))
    {
        return Err(RustError::unrecoverable_str(
            "backup_path must point to a zip archive",
        ));
    }

    let mut environment_settings = settings.load_mut(io).await?;
    let project_dir = default_project_path(&mut environment_settings).to_string();
    environment_settings.maybe_save().await?;

    let project_name = match project_name {
        Some(project_name) => project_name,
        None => backup_project_name(&backup_path).ok_or_else(|| {
            RustError::unrecoverable_str("backup archive path has no valid file name")
        })?,
    };
    let project_name = validate_mcp_restore_project_name(&project_name)?;

    let restore_path = Path::new(&project_dir).join(project_name);
    if tokio::fs::try_exists(&restore_path).await? {
        return Err(RustError::unrecoverable_str(
            "restore target already exists",
        ));
    }

    super::super::create_dir_all_with_err(&project_dir).await?;
    tokio::fs::create_dir(&restore_path).await?;

    let remove_on_drop = RemoveDirOnDrop::new(&restore_path);
    extract_backup_zip_with_progress(&backup_path, &restore_path, progress).await?;

    let unity_project = UnityProject::load(DefaultProjectIo::new(
        restore_path.clone().into_boxed_path(),
    ))
    .await
    .map_err(|e| {
        error!(gui_toast = false; "Error loading restored project: {e}");
        RustError::unrecoverable_str("restored backup is not a valid Unity project")
    })?;

    add_restored_or_copied_project(settings, io, &unity_project).await?;
    remove_on_drop.forget();

    restore_path
        .into_os_string()
        .into_string()
        .map_err(|_| RustError::unrecoverable_str("restore path is not a valid unicode string"))
}

fn validate_mcp_restore_project_name(project_name: &str) -> Result<String, RustError> {
    validate_project_folder_name("project_name", project_name)
}

fn validate_project_folder_name(
    parameter_name: &str,
    project_name: &str,
) -> Result<String, RustError> {
    let project_name = project_name.trim();
    let project_name_upper = project_name.to_ascii_uppercase();
    let mut components = Path::new(project_name).components();
    let valid_single_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();

    if project_name.is_empty()
        || project_name.len() > 255
        || !valid_single_component
        || WINDOWS_RESERVED_NAMES.contains(&project_name_upper.as_str())
        || project_name.contains(WINDOWS_RESERVED_CHARS)
    {
        return Err(RustError::unrecoverable_str(format!(
            "{parameter_name} is not a valid project folder name"
        )));
    }

    Ok(project_name.to_string())
}

fn ensure_mcp_absolute_path(parameter_name: &str, path: &Path) -> Result<(), RustError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(RustError::unrecoverable_str(format!(
            "{parameter_name} must be an absolute path"
        )))
    }
}

async fn add_restored_or_copied_project(
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    unity_project: &UnityProject,
) -> Result<(), RustError> {
    let mut settings = settings.load_mut(io).await?;
    let mut connection = VccDatabaseConnection::connect(io).await?;
    migrate_sanitize_projects(&mut connection, io, &settings).await?;

    let projects = connection.get_projects();
    if projects
        .iter()
        .any(|in_db| in_db.path().map(Path::new) == Some(unity_project.project_dir()))
    {
        return Err(RustError::unrecoverable_str(
            "project is already registered",
        ));
    }

    connection.add_project(unity_project).await?;
    connection.save(io).await?;
    settings.load_from_db(&connection)?;
    settings.save().await?;
    Ok(())
}

pub(crate) async fn add_existing_project_by_path(
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    project_path: String,
) -> Result<String, RustError> {
    let project_path = PathBuf::from(project_path);
    ensure_mcp_absolute_path("project_path", &project_path)?;
    let unity_project =
        UnityProject::load(DefaultProjectIo::new(project_path.into_boxed_path())).await?;
    let registered_path = unity_project
        .project_dir()
        .as_os_str()
        .to_str()
        .ok_or_else(|| RustError::unrecoverable_str("project_path is not valid unicode"))?
        .to_string();

    add_restored_or_copied_project(settings, io, &unity_project).await?;
    Ok(registered_path)
}

fn fix_zip_path_separator(path: &str) -> std::borrow::Cow<'_, str> {
    if cfg!(windows) || !path.contains('\\') {
        std::borrow::Cow::Borrowed(path)
    } else {
        std::borrow::Cow::Owned(path.replace('\\', "/"))
    }
}

fn is_complete_relative(path: &Path) -> bool {
    for component in path.components() {
        match component {
            Component::Prefix(_) => return false,
            Component::RootDir => return false,
            Component::ParentDir => return false,
            Component::CurDir => {}
            Component::Normal(_) => {}
        }
    }
    true
}

#[tauri::command]
#[specta::specta]
pub async fn environment_remove_project_by_path(
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    app: AppHandle,
    project_path: String,
    directory: bool,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = gui_project_activity(
        operations::PROJECT_REMOVE,
        if directory {
            "Removing project and moving folder to trash"
        } else {
            "Removing project from project list"
        },
        &project_path,
    )
    .details(vec![
        ActivityDetail::new("projectPath", summarize_path(&project_path)),
        ActivityDetail::new("removeDirectory", directory.to_string()),
    ]);

    activity
        .track_result(
            Some(&app),
            input,
            if directory {
                "Project removed and folder moved to trash"
            } else {
                "Project removed from project list"
            },
            vec![
                ActivityDetail::new("projectPath", summarize_path(&project_path)),
                ActivityDetail::new("removeDirectory", directory.to_string()),
            ],
            async {
                let mut settings = settings.load_mut(io.inner()).await?;
                let mut connection = VccDatabaseConnection::connect(io.inner()).await?;
                migrate_sanitize_projects(&mut connection, io.inner(), &settings).await?;
                let Some(project) = connection.find_project(&project_path).unwrap() else {
                    return Err(RustError::unrecoverable_str("project not found"));
                };

                if directory {
                    let path = project.path().unwrap();
                    info!("removing project directory: {path}");

                    if let Err(err) = trash_delete(PathBuf::from(path)).await {
                        error!("failed to remove project directory: {err}");
                        return Err(localizable_error!(
                            "projects:toast:remove directory failed by lock",
                            err => err
                        ));
                    }
                    info!("removed project directory: {path}");
                }

                connection.remove_project(&project);
                connection.save(io.inner()).await?;
                settings.load_from_db(&connection)?;
                settings.save().await?;

                Ok(())
            },
        )
        .await
}

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriCopyProjectProgress {
    total: usize,
    proceed: usize,
    last_proceed: String,
}

#[tauri::command]
#[specta::specta]
pub async fn environment_copy_project_for_migration(
    project_copy: State<'_, ProjectCopyState>,
    window: Window,
    channel: String,
    source_path: String,
) -> Result<AsyncCallResult<TauriCopyProjectProgress, String>, RustError> {
    async fn create_folder(source_path: PathBuf) -> Option<PathBuf> {
        let folder = source_path.parent().unwrap();
        let name = source_path.file_name().unwrap();

        let name = name.to_str().unwrap();
        // first, try `-Migrated`
        let new_path = folder.join(format!("{name}-Migrated"));
        if let Ok(()) = tokio::fs::create_dir(&new_path).await {
            return Some(new_path);
        }

        for i in 1..100 {
            let new_path = folder.join(format!("{name}-Migrated-{i}"));
            if let Ok(()) = tokio::fs::create_dir(&new_path).await {
                return Some(new_path);
            }
        }

        None
    }

    copy_project(
        project_copy.inner().clone(),
        window,
        channel,
        source_path,
        create_folder,
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_copy_project(
    project_copy: State<'_, ProjectCopyState>,
    window: Window,
    channel: String,
    source_path: String,
    new_path: String,
) -> Result<AsyncCallResult<TauriCopyProjectProgress, String>, RustError> {
    copy_project(
        project_copy.inner().clone(),
        window,
        channel,
        source_path,
        async move |_| {
            if let Ok(()) = tokio::fs::create_dir(&new_path).await {
                Some(PathBuf::from(new_path))
            } else {
                None
            }
        },
    )
    .await
}

pub async fn copy_project<F, Fut>(
    project_copy: ProjectCopyState,
    window: Window,
    channel: String,
    source_path: String,
    create_folder: F,
) -> Result<AsyncCallResult<TauriCopyProjectProgress, String>, RustError>
where
    F: FnOnce(PathBuf) -> Fut + Send + Sync,
    Fut: Future<Output = Option<PathBuf>> + Send + Sync,
{
    let app = window.app_handle().clone();
    let activity_tracker = app.try_state::<ActivityLogState>().map(|activity| {
        activity.start_activity(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityImportance::Primary,
                operations::PROJECT_COPY,
                "Copying project",
            )
            .target(target_from_path(&source_path))
            .details(vec![ActivityDetail::new(
                "sourceProjectPath",
                summarize_path(&source_path),
            )]),
        )
    });

    if !project_copy.try_start_uncancellable() {
        if let Some(activity_tracker) = &activity_tracker
            && let Some(activity) = app.try_state::<ActivityLogState>()
        {
            activity.finish_failed(
                Some(&app),
                activity_tracker,
                "Project copy could not start",
                Vec::new(),
                "project copy is already running",
            );
        }
        return Err(localizable_error!("projects:toast:copy already running"));
    }

    let project_copy_start = project_copy.clone();
    let project_copy_finish = project_copy.clone();
    let async_activity_tracker = activity_tracker.clone();
    let async_app = app.clone();
    let finish_activity_tracker = activity_tracker.clone();
    let finish_app = app.clone();
    let result = async_command_with_cancel_state(
        channel,
        window,
        async {
            let source_path_str = source_path;
            let source_path = Path::new(&source_path_str);

            let Some(new_path) = create_folder(source_path.into()).await else {
                if let Some(activity_tracker) = &activity_tracker
                    && let Some(activity) = app.try_state::<ActivityLogState>()
                {
                    activity.finish_failed(
                        Some(&app),
                        activity_tracker,
                        "Project copy target could not be created",
                        Vec::new(),
                        "failed to create a new project folder",
                    );
                }
                return Err(RustError::unrecoverable_str(
                    "failed to create a new folder for migration",
                ));
            };
            let new_path_str = new_path.into_os_string().into_string().unwrap();

            With::<TauriCopyProjectProgress>::continue_async(move |ctx| async move {
                let source_path = Path::new(&source_path_str);
                let new_path_buf = PathBuf::from(&new_path_str);
                let new_path = new_path_buf.as_path();
                let remove_on_drop = RemoveDirOnDrop::new(new_path);

                info!("copying project for migration: {source_path_str} -> {new_path_str}");

                let progress_ctx = ctx.clone();
                let outcome: Result<String, RustError> = async {
                    copy_project_files(source_path, new_path, move |progress| {
                        let _ = progress_ctx.emit(progress);
                    })
                    .await?;

                    info!("copied project for migration. adding to project list");

                    let unity_project = load_project(new_path_str.clone()).await?;

                    let settings = ctx.state::<SettingsState>();
                    let io = ctx.state::<DefaultEnvironmentIo>();

                    {
                        let mut settings = settings.load_mut(io.inner()).await?;
                        let mut connection = VccDatabaseConnection::connect(io.inner()).await?;
                        migrate_sanitize_projects(&mut connection, io.inner(), &settings).await?;
                        connection.add_project(&unity_project).await?;
                        connection.save(io.inner()).await?;
                        settings.load_from_db(&connection)?;
                        settings.save().await?;
                    }

                    remove_on_drop.forget();
                    Ok(new_path_str)
                }
                .await;

                if let Some(activity_tracker) = &async_activity_tracker
                    && let Some(activity) = async_app.try_state::<ActivityLogState>()
                {
                    match &outcome {
                        Ok(new_path) => {
                            activity.finish_success(
                                Some(&async_app),
                                activity_tracker,
                                "Project copied",
                                vec![
                                    ActivityDetail::new(
                                        "sourceProjectPath",
                                        summarize_path(source_path),
                                    ),
                                    ActivityDetail::new("newProjectPath", summarize_path(new_path)),
                                ],
                            );
                        }
                        Err(error) => {
                            activity.finish_failed(
                                Some(&async_app),
                                activity_tracker,
                                "Project copy failed",
                                Vec::new(),
                                error,
                            );
                        }
                    }
                }

                outcome
            })
        },
        move |abort| project_copy_start.start(abort),
        move || {
            project_copy_finish.finish();
            if let Some(activity_tracker) = &finish_activity_tracker
                && let Some(activity) = finish_app.try_state::<ActivityLogState>()
            {
                activity.finish_cancelled(
                    Some(&finish_app),
                    activity_tracker,
                    "Project copy cancelled",
                    Vec::new(),
                );
            }
        },
    )
    .await;
    if result.is_err() {
        project_copy.finish();
        if let Some(activity_tracker) = &activity_tracker
            && let Some(activity) = app.try_state::<ActivityLogState>()
            && let Err(error) = &result
        {
            activity.finish_failed(
                Some(&app),
                activity_tracker,
                "Project copy failed to start",
                Vec::new(),
                error,
            );
        }
    }
    result
}

async fn copy_file_cancellable(source: &Path, destination: &Path) -> io::Result<()> {
    let mut source = tokio::fs::File::open(source).await?;
    let mut destination = tokio::fs::File::create(destination).await?;
    let mut buffer = vec![0; 64 * 1024];

    loop {
        let len = source.read(&mut buffer).await?;
        if len == 0 {
            break;
        }
        destination.write_all(&buffer[..len]).await?;
    }

    destination.flush().await
}

pub(crate) async fn copy_registered_project_to_path(
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    source_path: String,
    new_path: String,
    progress: impl Fn(TauriCopyProjectProgress) + Clone + Send + Sync + 'static,
) -> Result<String, RustError> {
    let source_path = PathBuf::from(source_path);
    let new_path = PathBuf::from(new_path);
    ensure_mcp_absolute_path("new_project_path", &new_path)?;
    tokio::fs::create_dir(&new_path).await?;
    let remove_on_drop = RemoveDirOnDrop::new(&new_path);

    ensure_copy_target_not_inside_source(&source_path, &new_path).await?;
    copy_project_files(&source_path, &new_path, progress).await?;

    let new_path_str = new_path
        .clone()
        .into_os_string()
        .into_string()
        .map_err(|_| RustError::unrecoverable_str("new project path is not valid unicode"))?;
    let unity_project = load_project(new_path_str.clone()).await?;

    add_restored_or_copied_project(settings, io, &unity_project).await?;
    remove_on_drop.forget();
    Ok(new_path_str)
}

async fn ensure_copy_target_not_inside_source(
    source_path: &Path,
    new_path: &Path,
) -> Result<(), RustError> {
    let source_path = tokio::fs::canonicalize(source_path).await?;
    let new_path = tokio::fs::canonicalize(new_path).await?;

    if new_path.starts_with(&source_path) {
        Err(RustError::unrecoverable_str(
            "new_project_path must not be inside source_project_path",
        ))
    } else {
        Ok(())
    }
}

async fn copy_project_files<F>(
    source_path: &Path,
    new_path: &Path,
    progress: F,
) -> Result<(), RustError>
where
    F: Fn(TauriCopyProjectProgress) + Clone + Send + Sync + 'static,
{
    info!(
        "copying project: {} -> {}",
        source_path.display(),
        new_path.display()
    );

    let file_tree =
        collect_notable_project_files_tree(PathBuf::from(source_path), false, false).await?;
    let total_files = file_tree.count_all();

    info!("collecting files for copy finished, total files: {total_files}");

    struct CopyFileContext<'a, F> {
        proceed: AtomicUsize,
        total_files: usize,
        new_path: &'a Path,
        semaphore: Semaphore,
        progress: &'a F,
    }

    impl<F> CopyFileContext<'_, F>
    where
        F: Fn(TauriCopyProjectProgress) + Clone + Send + Sync + 'static,
    {
        fn on_finish(&self, entry: &FileSystemTree) {
            let proceed = self
                .proceed
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let last_proceed = entry.relative_path().to_string();

            (self.progress)(TauriCopyProjectProgress {
                total: self.total_files,
                proceed: proceed + 1,
                last_proceed,
            });
        }

        async fn process(&self, entry: &FileSystemTree) -> io::Result<()> {
            let new_entry = self.new_path.join(entry.relative_path());

            if entry.is_dir() {
                let permission = self.semaphore.acquire().await.unwrap();
                if let Err(e) = tokio::fs::create_dir(&new_entry).await
                    && e.kind() != io::ErrorKind::AlreadyExists
                {
                    return Err(e);
                }
                drop(permission);

                try_join_all(entry.iter().map(|x| self.process(x))).await?;
            } else {
                let permission = self.semaphore.acquire().await.unwrap();
                copy_file_cancellable(entry.absolute_path(), &new_entry).await?;
                drop(permission);

                self.on_finish(entry);
            }

            Ok(())
        }
    }

    let parallelism = std::thread::available_parallelism()
        .map(|x| x.get() * 2)
        .unwrap_or(4);

    info!("Copying project with parallelism: {parallelism}");

    CopyFileContext {
        proceed: AtomicUsize::new(0),
        total_files,
        new_path,
        semaphore: Semaphore::new(parallelism),
        progress: &progress,
    }
    .process(&file_tree)
    .await?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_favorite_project(
    io: State<'_, DefaultEnvironmentIo>,
    app: AppHandle,
    project_path: String,
    favorite: bool,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = gui_project_activity(
        operations::PROJECT_SET_FAVORITE,
        if favorite {
            "Marking project as favorite"
        } else {
            "Removing project from favorites"
        },
        &project_path,
    )
    .details(vec![
        ActivityDetail::new("projectPath", summarize_path(&project_path)),
        ActivityDetail::new("favorite", favorite.to_string()),
    ]);

    activity
        .track_result(
            Some(&app),
            input,
            if favorite {
                "Project marked as favorite"
            } else {
                "Project removed from favorites"
            },
            vec![
                ActivityDetail::new("projectPath", summarize_path(&project_path)),
                ActivityDetail::new("favorite", favorite.to_string()),
            ],
            async {
                let mut connection = VccDatabaseConnection::connect(io.inner()).await?;
                let Some(mut project) = connection.find_project(&project_path).unwrap() else {
                    return Err(RustError::unrecoverable_str("project not found"));
                };
                project.set_favorite(favorite);
                connection.update_project(&project);
                connection.save(io.inner()).await?;
                Ok(())
            },
        )
        .await
}

#[derive(Serialize, Deserialize, specta::Type)]
pub struct TauriProjectTemplateInfo {
    pub display_name: String,
    pub id: String,
    pub unity_versions: Vec<String>,
    pub update_date: Option<String>,
    pub has_unitypackage: bool,
    pub has_project_archive: bool,
    pub source_path: Option<String>,
    pub available: bool,
}

impl From<&ProjectTemplateInfo> for TauriProjectTemplateInfo {
    fn from(info: &ProjectTemplateInfo) -> Self {
        Self {
            display_name: info.display_name.clone(),
            id: info.id.clone(),
            unity_versions: info
                .unity_versions
                .iter()
                .sorted_by_key(|&&x| Reverse(x))
                .map(|x| x.to_string())
                .unique()
                .collect(),
            update_date: info.update_date.map(|x| x.to_rfc3339()),
            has_unitypackage: info
                .alcom_template
                .as_ref()
                .map(|x| !x.unity_packages.is_empty())
                .unwrap_or(false),
            has_project_archive: info
                .alcom_template
                .as_ref()
                .map(|x| x.is_project_archive())
                .unwrap_or(false),
            source_path: info
                .source_path
                .as_ref()
                .map(|x| x.to_string_lossy().into_owned()),
            available: info.available,
        }
    }
}

#[derive(Serialize, specta::Type)]
pub struct TauriProjectCreationInformation {
    templates: Vec<TauriProjectTemplateInfo>,
    recent_project_locations: Vec<String>,
    favorite_templates: Vec<String>,
    last_used_template: Option<String>,
    templates_version: u32,
    default_path: String,
}

#[tauri::command]
#[specta::specta]
pub async fn environment_project_creation_information(
    settings: State<'_, SettingsState>,
    templates: State<'_, TemplatesState>,
    config: State<'_, GuiConfigState>,
    io: State<'_, DefaultEnvironmentIo>,
) -> Result<TauriProjectCreationInformation, RustError> {
    let unity_paths = {
        let connection = VccDatabaseConnection::connect(io.inner()).await?;

        connection
            .get_unity_installations()
            .iter()
            .filter_map(|unity| unity.version())
            .collect::<Vec<_>>()
    };

    let recent_project_locations = config.get().recent_project_locations.clone();
    let last_used_template = config.get().last_used_template.clone();
    let favorite_templates = config.get().favorite_templates.clone();

    let templates = templates.save(templates::load_resolve_all_templates(&io, &unity_paths).await?);

    let mut settings = settings.load_mut(io.inner()).await?;
    let default_path = default_project_path(&mut settings).to_string();
    settings.maybe_save().await?;

    Ok(TauriProjectCreationInformation {
        templates: templates.iter().map(Into::into).collect(),
        recent_project_locations,
        templates_version: templates.version(),
        default_path,
        last_used_template,
        favorite_templates,
    })
}

#[derive(Serialize, specta::Type)]
pub enum TauriProjectDirCheckResult {
    // path related
    InvalidNameForFolderName,
    MayCompatibilityProblem,
    WideChar,

    AlreadyExists,
    Ok,
}

static WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

static WINDOWS_RESERVED_CHARS: &[char] = &['/', '\\', '<', '>', ':', '"', '|', '?', '*'];

#[tauri::command]
#[specta::specta]
pub async fn environment_check_project_name(
    base_path: String,
    project_name: String,
) -> Result<TauriProjectDirCheckResult, RustError> {
    let project_name = project_name.trim();
    let project_name_upper = project_name.to_ascii_uppercase();

    if project_name.is_empty()
        || project_name.len() > 255
        || WINDOWS_RESERVED_NAMES.contains(&project_name_upper.as_str())
        || project_name.contains(WINDOWS_RESERVED_CHARS)
    {
        return Ok(TauriProjectDirCheckResult::InvalidNameForFolderName);
    }

    let path = Path::new(&base_path).join(project_name);
    if path.exists() {
        return Ok(TauriProjectDirCheckResult::AlreadyExists);
    }

    if cfg!(target_os = "windows") {
        if project_name.contains('%') {
            return Ok(TauriProjectDirCheckResult::MayCompatibilityProblem);
        }

        if project_name.chars().any(|c| c as u32 > 0x7F) {
            return Ok(TauriProjectDirCheckResult::WideChar);
        }
    }

    Ok(TauriProjectDirCheckResult::Ok)
}

#[derive(Serialize, specta::Type)]
pub enum TauriCreateProjectResult {
    AlreadyExists,
    TemplateNotFound,
    Successful,
}

pub(crate) struct CreatedProjectInfo {
    pub(crate) project_path: String,
    pub(crate) template_id: String,
    pub(crate) unity_version: String,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_project_with_defaults(
    packages_state: &PackagesState,
    settings: &SettingsState,
    config: &GuiConfigState,
    io: &DefaultEnvironmentIo,
    http: &reqwest::Client,
    base_path: Option<String>,
    project_name: String,
    template_id: Option<String>,
    unity_version: Option<String>,
    abort: Option<&AbortCheck>,
) -> Result<CreatedProjectInfo, RustError> {
    let project_name = validate_project_folder_name("project_name", &project_name)?;
    let base_path = match base_path {
        Some(base_path) => PathBuf::from(base_path),
        None => {
            let mut environment_settings = settings.load_mut(io).await?;
            let default_path = default_project_path(&mut environment_settings).to_string();
            environment_settings.maybe_save().await?;
            PathBuf::from(default_path)
        }
    };
    ensure_mcp_absolute_path("base_path", &base_path)?;

    check_project_create_abort(abort)?;
    let template_infos = load_current_project_templates_for_mcp(io).await?;
    let template = select_project_template(&template_infos, config, template_id.as_deref())?;
    let template_id = template.id.clone();
    let unity_version = select_project_unity_version(template, unity_version.as_deref())?;
    let unity_version_label = unity_version.to_string();
    let project_path = base_path.join(&project_name);

    match create_project_from_template(
        packages_state,
        settings,
        config,
        &template_infos,
        io,
        http,
        &base_path,
        &project_name,
        &template_id,
        unity_version,
        abort,
        true,
    )
    .await?
    {
        TauriCreateProjectResult::Successful => Ok(CreatedProjectInfo {
            project_path: project_path
                .into_os_string()
                .into_string()
                .map_err(|_| RustError::unrecoverable_str("project path is not valid unicode"))?,
            template_id,
            unity_version: unity_version_label,
        }),
        TauriCreateProjectResult::AlreadyExists => Err(RustError::unrecoverable_str(
            "project folder already exists",
        )),
        TauriCreateProjectResult::TemplateNotFound => Err(RustError::unrecoverable_str(
            "project template was not found",
        )),
    }
}

async fn load_current_project_templates_for_mcp(
    io: &DefaultEnvironmentIo,
) -> Result<Vec<ProjectTemplateInfo>, RustError> {
    let unity_paths = {
        let connection = VccDatabaseConnection::connect(io).await?;
        connection
            .get_unity_installations()
            .iter()
            .filter_map(|unity| unity.version())
            .collect::<Vec<_>>()
    };

    Ok(templates::load_resolve_all_templates(io, &unity_paths).await?)
}

fn select_project_template<'a>(
    templates: &'a [ProjectTemplateInfo],
    config: &GuiConfigState,
    template_id: Option<&str>,
) -> Result<&'a ProjectTemplateInfo, RustError> {
    let is_usable_template =
        |template: &&ProjectTemplateInfo| template.available && !template.unity_versions.is_empty();
    let template = match template_id {
        Some(template_id) => templates.iter().find(|template| template.id == template_id),
        None => {
            let last_used_template = config.get().last_used_template.clone();
            last_used_template
                .as_deref()
                .and_then(|last_used_template| {
                    templates.iter().find(|template| {
                        template.id == last_used_template && is_usable_template(template)
                    })
                })
                .or_else(|| templates.iter().find(is_usable_template))
        }
    };

    let Some(template) = template else {
        return Err(RustError::unrecoverable_str(
            "template_id must match an available project template",
        ));
    };
    if !template.available || template.unity_versions.is_empty() {
        return Err(RustError::unrecoverable_str(
            "template_id must match an available project template with Unity versions",
        ));
    }
    Ok(template)
}

fn select_project_unity_version(
    template: &ProjectTemplateInfo,
    unity_version: Option<&str>,
) -> Result<UnityVersion, RustError> {
    match unity_version {
        Some(unity_version) => {
            let unity_version = UnityVersion::parse(unity_version)
                .ok_or_else(|| RustError::unrecoverable_str("unity_version is not valid"))?;
            if template.unity_versions.contains(&unity_version) {
                Ok(unity_version)
            } else {
                Err(RustError::unrecoverable_str(
                    "unity_version must be available for the selected template",
                ))
            }
        }
        None => template
            .unity_versions
            .iter()
            .max()
            .copied()
            .ok_or_else(|| RustError::unrecoverable_str("selected template has no Unity versions")),
    }
}

fn normalize_gui_project_base_path(base_path: &Path) -> PathBuf {
    if !base_path.has_root() {
        let mut components = base_path.components().collect::<Vec<_>>();

        match (components.first(), components.get(1)) {
            (Some(Component::Prefix(_)), Some(Component::RootDir)) => {
                // starts with 'C:/', good!
            }
            (Some(Component::Prefix(prefix)), _) => {
                if matches!(prefix.kind(), Prefix::Disk(_)) {
                    // starts with 'C:yourpath', we should insert / after prefix
                    components.insert(1, Component::RootDir);
                } else {
                    // starts with '\\?\', no problem
                }
            }
            (Some(Component::RootDir), _) => {
                // starts with '/', good!
            }
            (Some(_), _) => {
                // starts with 'yourpath', insert '/'
                components.insert(0, Component::RootDir);
            }
            _ => {}
        }

        components.iter().collect()
    } else {
        base_path.to_path_buf()
    }
}

fn check_project_create_abort(abort: Option<&AbortCheck>) -> Result<(), RustError> {
    if let Some(abort) = abort {
        abort.check()?;
    }
    Ok(())
}

async fn register_created_project(
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    unity_project: &UnityProject,
) -> Result<(), RustError> {
    let mut settings = settings.load_mut(io).await?;
    let mut connection = VccDatabaseConnection::connect(io).await?;
    migrate_sanitize_projects(&mut connection, io, &settings).await?;
    connection.add_project(unity_project).await?;
    connection.save(io).await?;
    settings.load_from_db(&connection)?;
    settings.save().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_project_from_template(
    packages_state: &PackagesState,
    settings: &SettingsState,
    config: &GuiConfigState,
    templates: &[ProjectTemplateInfo],
    io: &DefaultEnvironmentIo,
    http: &reqwest::Client,
    base_path: &Path,
    project_name: &str,
    template_id: &str,
    unity_version: UnityVersion,
    abort: Option<&AbortCheck>,
    cleanup_before_registration: bool,
) -> Result<TauriCreateProjectResult, RustError> {
    check_project_create_abort(abort)?;
    {
        let mut config = config.load_mut().await?;
        let base_path_str = base_path
            .as_os_str()
            .to_str()
            .ok_or_else(|| RustError::unrecoverable_str("base_path is not valid unicode"))?;
        if let Some(path_index) = config
            .recent_project_locations
            .iter()
            .position(|x| x == base_path_str)
        {
            let base_path = config.recent_project_locations.remove(path_index);
            config.recent_project_locations.push(base_path);
        } else {
            let to_remove = config.recent_project_locations.len().saturating_sub(8 - 1);
            config.recent_project_locations.drain(0..to_remove);
            config
                .recent_project_locations
                .push(base_path_str.to_string());
        }
        config.last_used_template = Some(template_id.to_string());
        config.save().await?;
    }

    check_project_create_abort(abort)?;
    super::super::create_dir_all_with_err(base_path).await?;
    let path = base_path.join(project_name);
    match tokio::fs::create_dir(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return Ok(TauriCreateProjectResult::AlreadyExists);
        }
        Err(e) => {
            return Err(e.into());
        }
    }
    let remove_on_drop = if cleanup_before_registration {
        Some(RemoveDirOnDrop::new(&path))
    } else {
        None
    };

    check_project_create_abort(abort)?;
    let mut unity_project = match templates::create_project(
        io,
        templates,
        template_id,
        &path,
        project_name,
        unity_version,
    )
    .await
    {
        Ok(unity_project) => unity_project,
        Err(CreateProjectErr::Io(e)) => return Err(e.into()),
        Err(CreateProjectErr::NoSuchTemplate) => {
            return Ok(TauriCreateProjectResult::TemplateNotFound);
        }
    };

    check_project_create_abort(abort)?;
    let packages = {
        let settings = settings.load(io).await?;
        packages_state.load_fully(&settings, io, http).await?
    };

    if !cleanup_before_registration {
        register_created_project(settings, io, &unity_project).await?;
    }

    {
        check_project_create_abort(abort)?;
        let installer = PackageInstaller::new(io, Some(http));
        let request = unity_project.resolve_request(packages.collection()).await?;
        check_project_create_abort(abort)?;
        if let Some(abort) = abort {
            unity_project
                .apply_pending_changes_with_abort(&installer, request, abort)
                .await?;
        } else {
            unity_project
                .apply_pending_changes(&installer, request)
                .await?;
        }
    }

    check_project_create_abort(abort)?;
    if cleanup_before_registration {
        register_created_project(settings, io, &unity_project).await?;
        if let Some(remove_on_drop) = remove_on_drop {
            remove_on_drop.forget();
        }
    }

    Ok(TauriCreateProjectResult::Successful)
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn environment_create_project(
    app_handle: AppHandle,
    base_path: String,
    project_name: String,
    template_id: String,
    template_version: u32,
    unity_version: String,
) -> Result<TauriCreateProjectResult, RustError> {
    let packages_state: State<'_, PackagesState> = app_handle.state();
    let settings: State<'_, SettingsState> = app_handle.state();
    let config: State<'_, GuiConfigState> = app_handle.state();
    let templates: State<'_, TemplatesState> = app_handle.state();
    let io: State<'_, DefaultEnvironmentIo> = app_handle.state();
    let http: State<'_, reqwest::Client> = app_handle.state();
    let activity = app_handle.state::<ActivityLogState>();

    let templates = templates
        .get_versioned(template_version)
        .ok_or_else(|| RustError::unrecoverable_str("Templates info version mismatch (bug)"))?;

    let unity_version = UnityVersion::parse(&unity_version)
        .ok_or_else(|| RustError::unrecoverable_str("Bad Unity Version (unparsable)"))?;
    let unity_version_label = unity_version.to_string();

    let base_path = normalize_gui_project_base_path(Path::new(&base_path));
    let path = base_path.join(&project_name);
    let activity_input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::PROJECT_CREATE,
        "Creating project",
    )
    .target(project_name.clone())
    .details(vec![
        ActivityDetail::new("projectPath", summarize_path(&path)),
        ActivityDetail::new("template", template_id.clone()),
        ActivityDetail::new("unityVersion", unity_version_label.clone()),
    ]);
    let tracker = activity.start_activity(Some(&app_handle), activity_input);

    let result = create_project_from_template(
        packages_state.inner(),
        settings.inner(),
        config.inner(),
        &templates,
        io.inner(),
        http.inner(),
        &base_path,
        &project_name,
        &template_id,
        unity_version,
        None,
        false,
    )
    .await;

    match &result {
        Ok(TauriCreateProjectResult::Successful) => {
            activity.finish_success(
                Some(&app_handle),
                &tracker,
                "Project created",
                vec![
                    ActivityDetail::new("projectPath", summarize_path(&path)),
                    ActivityDetail::new("template", template_id),
                    ActivityDetail::new("unityVersion", unity_version_label),
                ],
            );
        }
        Ok(TauriCreateProjectResult::AlreadyExists) => {
            activity.finish_failed(
                Some(&app_handle),
                &tracker,
                "Project folder already exists",
                Vec::new(),
                "project folder already exists",
            );
        }
        Ok(TauriCreateProjectResult::TemplateNotFound) => {
            activity.finish_failed(
                Some(&app_handle),
                &tracker,
                "Project template was not found",
                Vec::new(),
                "project template was not found",
            );
        }
        Err(error) => {
            activity.finish_failed(
                Some(&app_handle),
                &tracker,
                "Project creation failed",
                Vec::new(),
                error,
            );
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{backup_project_name, validate_project_folder_name};
    use std::path::Path;

    #[test]
    fn backup_file_stem_is_used_as_default_restore_name() {
        assert_eq!(
            backup_project_name(Path::new("My Project.zip")),
            Some("My Project".to_string())
        );
    }

    #[test]
    fn restore_project_name_is_trimmed_and_restricted_to_one_folder() {
        assert_eq!(
            validate_project_folder_name("project_name", "  Restored Project  ").unwrap(),
            "Restored Project"
        );
        assert!(validate_project_folder_name("project_name", "..").is_err());
        assert!(validate_project_folder_name("project_name", "nested/project").is_err());
    }
}
