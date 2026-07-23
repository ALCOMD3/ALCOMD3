use std::io;
use std::path::{Path, PathBuf};

use log::error;
use serde::Serialize;
pub use start::startup;
use tauri::generate_handler;
use tauri::ipc::Invoke;
pub use uri_custom_scheme::handle_vrc_get_scheme;
use vrc_get_vpm::environment::VccDatabaseConnection;
use vrc_get_vpm::io::{DefaultEnvironmentIo, DefaultProjectIo};
use vrc_get_vpm::unity_project::{
    AddPackageErr, MigrateUnity2022Error, MigrateVpmError, ReinstalPackagesError, ResolvePackageErr,
};
use vrc_get_vpm::version::{Version, VersionRange};
use vrc_get_vpm::{PackageInfo, PackageManifest, UnityProject};

// common macro for commands so put it here
#[allow(unused_macros)]
macro_rules! localizable_error {
    ($id:literal $(,)?) => {
        $crate::commands::RustError::Localizable(::std::boxed::Box::new($crate::commands::LocalizableRustError {
            id: $id.to_string(),
            args: indexmap::IndexMap::new(),
        }))
    };

    ($id:literal, $($key:ident => $value:expr), * $(,)?) => {
        $crate::commands::RustError::Localizable(::std::boxed::Box::new($crate::commands::LocalizableRustError {
            id: $id.to_string(),
            args: indexmap::indexmap! {
                $(::std::stringify!($key).to_string() => $value.to_string()),*
            }
        }))
    };
}

mod activity;
mod async_command;
mod environment;
mod mcp;
mod project;
mod start;
mod uri_custom_scheme;
mod util;

pub(crate) use environment::packages::{AddedRepositoryInfo, add_repository_by_url};
pub(crate) use environment::projects::{
    CreatedProjectInfo, add_existing_project_by_path, copy_registered_project_to_path,
    create_project_with_defaults, restore_project_from_zip_backup,
};
pub use environment::templates::import_templates;
pub(crate) use project::{
    TauriPendingProjectChanges, build_project_package_row_accumulators,
    create_project_backup_with_settings, project_apply_pending_changes_from_prepared,
    project_apply_pending_changes_from_prepared_with_abort,
    project_package_row_compatible_packages, project_package_row_incompatible_packages,
};

#[allow(unused_imports)]
mod prelude {
    pub(super) use super::{
        IntoPathBuf as _, RustError, TauriBasePackageInfo, TauriPackage, TauriVersion,
        UnityProject, load_project, safe_url, update_project_last_modified,
    };
    pub use crate::state::*;
}

// Note: remember to change similar in typescript
pub(crate) static DEFAULT_UNITY_ARGUMENTS: &[&str] = &[];

pub(crate) fn handlers() -> impl Fn(Invoke) -> bool + Send + Sync + 'static {
    generate_handler![
        activity::activity_get_entries,
        activity::activity_open_log_folder,
        environment::config::environment_language,
        environment::config::environment_set_language,
        environment::config::environment_theme,
        environment::config::environment_set_theme,
        environment::config::environment_get_project_sorting,
        environment::config::environment_set_project_sorting,
        environment::config::environment_get_sidebar_extensions,
        environment::config::environment_set_sidebar_extension_installed,
        environment::config::environment_set_sidebar_extension_order,
        environment::config::environment_set_sidebar_extension_visible,
        environment::config::environment_get_finished_setup_pages,
        environment::config::environment_finished_setup_page,
        environment::config::environment_clear_setup_process,
        environment::config::environment_logs_level,
        environment::config::environment_set_logs_level,
        environment::config::environment_gui_animation,
        environment::config::environment_set_gui_animation,
        environment::config::environment_gui_compact,
        environment::config::environment_set_gui_compact,
        environment::config::environment_project_view_mode,
        environment::config::environment_set_project_view_mode,
        environment::config::environment_set_unity_hub_access_method,
        environment::config::environment_set_template_favorite,
        environment::config::environment_update_reminder,
        environment::config::environment_set_update_reminder,
        environment::legacy_import::environment_legacy_data_sources,
        environment::legacy_import::environment_import_legacy_data,
        mcp::mcp_status,
        mcp::mcp_set_enabled,
        environment::projects::environment_projects,
        environment::projects::environment_add_project_with_picker,
        environment::projects::environment_pick_project_backup_for_restore,
        environment::projects::environment_restore_project_from_backup,
        environment::projects::environment_remove_project_by_path,
        environment::projects::environment_copy_project_for_migration,
        environment::projects::environment_copy_project,
        environment::projects::environment_set_favorite_project,
        environment::projects::environment_project_creation_information,
        environment::projects::environment_check_project_name,
        environment::projects::environment_create_project,
        environment::packages::environment_refetch_packages,
        environment::packages::environment_packages,
        environment::packages::environment_default_repositories,
        environment::packages::environment_repository_package_lists,
        environment::packages::environment_repositories_info,
        environment::packages::environment_hide_repository,
        environment::packages::environment_show_repository,
        environment::packages::environment_set_hide_local_user_packages,
        environment::packages::environment_download_repository,
        environment::packages::environment_add_repository,
        environment::packages::environment_remove_repository,
        environment::packages::environment_reorder_repositories,
        environment::packages::environment_import_repository_pick,
        environment::packages::environment_import_download_repositories,
        environment::packages::environment_import_add_repositories,
        environment::packages::environment_export_repositories,
        environment::packages::environment_clear_package_cache,
        environment::packages::environment_get_user_packages,
        environment::packages::environment_add_user_package_with_picker,
        environment::packages::environment_remove_user_packages,
        environment::settings::environment_unity_versions,
        environment::settings::environment_get_settings,
        environment::settings::environment_pick_unity_hub,
        environment::settings::environment_pick_unity,
        environment::settings::environment_pick_project_default_path,
        environment::settings::environment_pick_project_backup_path,
        environment::settings::environment_set_show_prerelease_packages,
        environment::settings::environment_set_backup_format,
        environment::settings::environment_set_release_channel,
        environment::settings::environment_set_automatic_update,
        environment::settings::environment_set_use_alcom_for_vcc_protocol,
        environment::settings::environment_get_default_unity_arguments,
        environment::settings::environment_set_default_unity_arguments,
        environment::templates::environment_export_template,
        environment::templates::environment_get_alcom_template,
        environment::templates::environment_pick_unity_packages,
        environment::templates::environment_pick_unity_package,
        environment::templates::environment_save_template,
        environment::templates::environment_remove_template,
        environment::templates::environment_import_template,
        environment::templates::environment_import_template_override,
        environment::unity_hub::environment_update_unity_paths_from_unity_hub,
        environment::unity_hub::environment_is_loading_from_unity_hub_in_progress,
        environment::unity_hub::environment_wait_for_unity_hub_update,
        project::project_details,
        project::project_package_rows,
        project::project_install_packages,
        project::project_reinstall_packages,
        project::project_resolve,
        project::project_remove_packages,
        project::project_apply_pending_changes,
        project::project_cancel_apply_pending_changes,
        project::project_clear_pending_changes,
        project::project_migrate_project_to_2022,
        project::project_call_unity_for_migration,
        project::project_migrate_project_to_vpm,
        project::project_open_unity,
        project::project_is_unity_launching,
        project::project_backup_creation_information,
        project::project_check_backup_name,
        project::project_create_backup,
        project::project_get_custom_unity_args,
        project::project_set_custom_unity_args,
        project::project_get_unity_path,
        project::project_set_unity_path,
        util::util_open,
        util::util_open_url,
        util::util_open_url_nocheck,
        util::util_get_log_entries,
        util::util_get_version,
        util::util_frontend_ready,
        util::util_check_for_update,
        util::util_download_update,
        util::util_install_downloaded_update,
        util::util_is_bad_hostname,
        util::util_pick_directory,
        crate::deep_link_support::deep_link_has_add_repository,
        crate::deep_link_support::deep_link_take_add_repository,
        crate::deep_link_support::deep_link_install_vcc,
        crate::deep_link_support::deep_link_imported_clear_non_toasted_count,
        crate::deep_link_support::deep_link_reduce_imported_clear_non_toasted_count,
    ]
}

#[cfg(dev)]
pub(crate) fn export_ts() {
    let export_path = Path::new("lib").join("bindings.ts");
    if !should_export_ts_bindings(Path::new(".")) {
        log::warn!("skipping TypeScript bindings export outside the source tree");
        return;
    }

    if let Err(err) = tauri_specta::Builder::new()
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        .commands(tauri_specta::collect_commands![
            activity::activity_get_entries,
            activity::activity_open_log_folder,
            environment::config::environment_language,
            environment::config::environment_set_language,
            environment::config::environment_theme,
            environment::config::environment_set_theme,
            environment::config::environment_get_project_sorting,
            environment::config::environment_set_project_sorting,
            environment::config::environment_get_sidebar_extensions,
            environment::config::environment_set_sidebar_extension_installed,
            environment::config::environment_set_sidebar_extension_order,
            environment::config::environment_set_sidebar_extension_visible,
            environment::config::environment_get_finished_setup_pages,
            environment::config::environment_finished_setup_page,
            environment::config::environment_clear_setup_process,
            environment::config::environment_logs_level,
            environment::config::environment_set_logs_level,
            environment::config::environment_gui_animation,
            environment::config::environment_set_gui_animation,
            environment::config::environment_gui_compact,
            environment::config::environment_set_gui_compact,
            environment::config::environment_project_view_mode,
            environment::config::environment_set_project_view_mode,
            environment::config::environment_set_unity_hub_access_method,
            environment::config::environment_set_template_favorite,
            environment::config::environment_update_reminder,
            environment::config::environment_set_update_reminder,
            environment::legacy_import::environment_legacy_data_sources,
            environment::legacy_import::environment_import_legacy_data,
            mcp::mcp_status,
            mcp::mcp_set_enabled,
            environment::projects::environment_projects,
            environment::projects::environment_add_project_with_picker,
            environment::projects::environment_pick_project_backup_for_restore,
            environment::projects::environment_restore_project_from_backup,
            environment::projects::environment_remove_project_by_path,
            environment::projects::environment_copy_project_for_migration,
            environment::projects::environment_copy_project,
            environment::projects::environment_set_favorite_project,
            environment::projects::environment_project_creation_information,
            environment::projects::environment_check_project_name,
            environment::projects::environment_create_project,
            environment::packages::environment_refetch_packages,
            environment::packages::environment_packages,
            environment::packages::environment_default_repositories,
            environment::packages::environment_repository_package_lists,
            environment::packages::environment_repositories_info,
            environment::packages::environment_hide_repository,
            environment::packages::environment_show_repository,
            environment::packages::environment_set_hide_local_user_packages,
            environment::packages::environment_download_repository,
            environment::packages::environment_add_repository,
            environment::packages::environment_remove_repository,
            environment::packages::environment_reorder_repositories,
            environment::packages::environment_import_repository_pick,
            environment::packages::environment_import_download_repositories,
            environment::packages::environment_import_add_repositories,
            environment::packages::environment_export_repositories,
            environment::packages::environment_clear_package_cache,
            environment::packages::environment_get_user_packages,
            environment::packages::environment_add_user_package_with_picker,
            environment::packages::environment_remove_user_packages,
            environment::settings::environment_unity_versions,
            environment::settings::environment_get_settings,
            environment::settings::environment_pick_unity_hub,
            environment::settings::environment_pick_unity,
            environment::settings::environment_pick_project_default_path,
            environment::settings::environment_pick_project_backup_path,
            environment::settings::environment_set_show_prerelease_packages,
            environment::settings::environment_set_backup_format,
            environment::settings::environment_set_release_channel,
            environment::settings::environment_set_automatic_update,
            environment::settings::environment_set_use_alcom_for_vcc_protocol,
            environment::settings::environment_get_default_unity_arguments,
            environment::settings::environment_set_default_unity_arguments,
            environment::templates::environment_export_template,
            environment::templates::environment_get_alcom_template,
            environment::templates::environment_pick_unity_packages,
            environment::templates::environment_pick_unity_package,
            environment::templates::environment_save_template,
            environment::templates::environment_remove_template,
            environment::templates::environment_import_template,
            environment::templates::environment_import_template_override,
            environment::unity_hub::environment_update_unity_paths_from_unity_hub,
            environment::unity_hub::environment_is_loading_from_unity_hub_in_progress,
            environment::unity_hub::environment_wait_for_unity_hub_update,
            project::project_details,
            project::project_package_rows,
            project::project_install_packages,
            project::project_reinstall_packages,
            project::project_resolve,
            project::project_remove_packages,
            project::project_apply_pending_changes,
            project::project_cancel_apply_pending_changes,
            project::project_clear_pending_changes,
            project::project_migrate_project_to_2022,
            project::project_call_unity_for_migration,
            project::project_migrate_project_to_vpm,
            project::project_open_unity,
            project::project_is_unity_launching,
            project::project_backup_creation_information,
            project::project_check_backup_name,
            project::project_create_backup,
            project::project_get_custom_unity_args,
            project::project_set_custom_unity_args,
            project::project_get_unity_path,
            project::project_set_unity_path,
            util::util_open,
            util::util_open_url,
            util::util_open_url_nocheck,
            util::util_get_log_entries,
            util::util_get_version,
            util::util_frontend_ready,
            util::util_check_for_update,
            util::util_download_update,
            util::util_install_downloaded_update,
            util::util_is_bad_hostname,
            util::util_pick_directory,
            crate::deep_link_support::deep_link_has_add_repository,
            crate::deep_link_support::deep_link_take_add_repository,
            crate::deep_link_support::deep_link_install_vcc,
            crate::deep_link_support::deep_link_imported_clear_non_toasted_count,
            crate::deep_link_support::deep_link_reduce_imported_clear_non_toasted_count,
        ])
        .typ::<uri_custom_scheme::GlobalInfo>()
        .typ::<crate::mcp::McpToolCallEvent>()
        .typ::<crate::activity_log::ActivityEntry>()
        .typ::<crate::activity_log::ActivityEntryFilter>()
        .typ::<environment::projects::TauriUpdatedRealProjectInfo>()
        .typ::<project::TauriProjectApplyProgress>()
        .export(specta_typescript::Typescript::default(), &export_path)
    {
        log::warn!(
            "failed to export TypeScript bindings to {}: {err}",
            export_path.display()
        );
    }
}

#[cfg(any(dev, test))]
fn should_export_ts_bindings(cwd: &Path) -> bool {
    cwd.join("Tauri.toml").is_file() && cwd.join("lib").is_dir()
}

#[cfg(test)]
mod tests {
    use super::should_export_ts_bindings;

    #[test]
    fn typescript_binding_export_requires_source_layout() {
        let temp = tempfile::tempdir().unwrap();
        assert!(!should_export_ts_bindings(temp.path()));

        std::fs::write(temp.path().join("Tauri.toml"), "").unwrap();
        assert!(!should_export_ts_bindings(temp.path()));

        std::fs::create_dir(temp.path().join("lib")).unwrap();
        assert!(should_export_ts_bindings(temp.path()));
    }
}

async fn update_project_last_modified(io: &DefaultEnvironmentIo, project_dir: &Path) {
    async fn inner(io: &DefaultEnvironmentIo, project_dir: &Path) -> Result<(), io::Error> {
        let mut connection = VccDatabaseConnection::connect(io).await?;
        connection.update_project_last_modified(&project_dir.to_string_lossy())?;
        connection.save(io).await?;
        Ok(())
    }

    if let Err(err) = inner(io, project_dir).await {
        eprintln!("error updating project updated_at on vcc: {err}");
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[specta(collect)]
#[serde(tag = "type")]
pub(crate) enum RustError {
    Unrecoverable {
        message: String,
    },
    #[allow(dead_code)]
    #[specta(type = LocalizableRustError)]
    Localizable(Box<LocalizableRustError>),
    Handleable {
        message: String,
        body: HandleableRustError,
    },
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub(crate) struct LocalizableRustError {
    id: String,
    args: indexmap::IndexMap<String, String>,
}

/// Errors that is expected to be handled on the GUI side
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "type")]
pub(crate) enum HandleableRustError {
    MissingDependencies {
        dependencies: Vec<(Box<str>, Box<str>)>,
    },
}

impl RustError {
    fn unrecoverable<T: std::error::Error>(value: T) -> Self {
        let message = Self::display_error(&value);
        error!("{message}");
        Self::Unrecoverable { message }
    }

    pub(crate) fn unrecoverable_str<T: Into<String>>(value: T) -> Self {
        let message = value.into();
        error!("{message}");
        Self::Unrecoverable { message }
    }

    fn handleable(message: String, body: HandleableRustError) -> Self {
        error!(gui_toast = false; "{message}");
        Self::Handleable { message, body }
    }

    // formats the error but with inner error message included
    fn display_error<T: std::error::Error>(e: T) -> String {
        let mut message = format!("{e}");

        let mut cur = e.source();
        while let Some(src) = cur {
            let src_msg = format!("{src}");

            if !message.contains(&src_msg) {
                message.push_str(": ");
                message.push_str(src_msg.as_str());
            }

            cur = src.source();
        }

        message
    }

    fn handleable_missing_dependencies(
        message: String,
        dependencies: Vec<(Box<str>, VersionRange)>,
    ) -> Self {
        Self::handleable(
            message,
            HandleableRustError::MissingDependencies {
                dependencies: dependencies
                    .into_iter()
                    .map(|(pkg, range)| (pkg, range.to_string().into()))
                    .collect(),
            },
        )
    }

    pub(crate) fn into_message(self) -> String {
        match self {
            Self::Unrecoverable { message } => message,
            Self::Localizable(error) => error.id,
            Self::Handleable { message, .. } => message,
        }
    }
}

impl std::fmt::Display for RustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unrecoverable { message } => message.fmt(f),
            Self::Localizable(error) => error.id.fmt(f),
            Self::Handleable { message, .. } => message.fmt(f),
        }
    }
}

macro_rules! impl_from_error {
    ($($error:ty),* $(,)?) => {
        $(
            impl From<$error> for RustError {
                fn from(value: $error) -> Self {
                    RustError::unrecoverable(value)
                }
            }
        )*
    };
}

impl_from_error!(
    io::Error,
    async_zip::error::ZipError,
    vrc_get_vpm::environment::AddRepositoryErr,
    vrc_get_vpm::unity_project::RemovePackageErr,
    fs_extra::error::Error,
);

impl From<String> for RustError {
    fn from(value: String) -> Self {
        RustError::unrecoverable_str(value)
    }
}

impl From<crate::compressor::CompressError> for RustError {
    fn from(value: crate::compressor::CompressError) -> Self {
        match value {
            crate::compressor::CompressError::Io(e) => e.into(),
            crate::compressor::CompressError::Zip(e) => e.into(),
            crate::compressor::CompressError::TaskJoin(e) => RustError::unrecoverable(e),
            crate::compressor::CompressError::Semaphore(e) => RustError::unrecoverable(e),
        }
    }
}

impl From<crate::updater::Error> for RustError {
    fn from(_value: crate::updater::Error) -> Self {
        localizable_error!("check update:toast:failed to load latest release")
    }
}

impl From<MigrateVpmError> for RustError {
    fn from(value: MigrateVpmError) -> Self {
        match value {
            MigrateVpmError::AddPackageErr(add_err) => add_err.into(),
            value => RustError::unrecoverable(value),
        }
    }
}

impl From<MigrateUnity2022Error> for RustError {
    fn from(value: MigrateUnity2022Error) -> Self {
        match value {
            MigrateUnity2022Error::AddPackageErr(add_err) => add_err.into(),
            value => RustError::unrecoverable(value),
        }
    }
}

impl From<ReinstalPackagesError> for RustError {
    fn from(value: ReinstalPackagesError) -> Self {
        let message = value.to_string();
        match value {
            ReinstalPackagesError::DependenciesNotFound { dependencies } => {
                RustError::handleable_missing_dependencies(message, dependencies)
            }
            _ => RustError::unrecoverable(value),
        }
    }
}

impl From<AddPackageErr> for RustError {
    fn from(value: AddPackageErr) -> Self {
        let message = value.to_string();
        match value {
            AddPackageErr::DependenciesNotFound { dependencies } => {
                RustError::handleable_missing_dependencies(message, dependencies)
            }
            _ => RustError::unrecoverable(value),
        }
    }
}

impl From<ResolvePackageErr> for RustError {
    fn from(value: ResolvePackageErr) -> Self {
        let message = value.to_string();
        match value {
            ResolvePackageErr::DependenciesNotFound { dependencies } => {
                RustError::handleable_missing_dependencies(message, dependencies)
            }
            _ => RustError::unrecoverable(value),
        }
    }
}

#[derive(Serialize, specta::Type, Clone)]
struct TauriVersion {
    major: u64,
    minor: u64,
    patch: u64,
    pre: String,
    build: String,
}

impl From<&Version> for TauriVersion {
    fn from(value: &Version) -> Self {
        Self {
            major: value.major,
            minor: value.minor,
            patch: value.patch,
            pre: value.pre.as_str().to_string(),
            build: value.build.as_str().to_string(),
        }
    }
}

#[derive(Serialize, specta::Type, Clone)]
struct TauriBasePackageInfo {
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    keywords: Vec<String>,
    version: TauriVersion,
    unity: Option<(u16, u8)>,
    changelog_url: Option<String>,
    documentation_url: Option<String>,
    vpm_dependencies: Vec<String>,
    legacy_packages: Vec<String>,
    is_yanked: bool,
}

fn safe_url(url: &url::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

impl TauriBasePackageInfo {
    fn new(package: &PackageManifest) -> Self {
        Self {
            name: package.name().to_string(),
            display_name: package.display_name().map(|v| v.to_string()),
            description: package.description().map(|v| v.to_string()),
            keywords: (package.aliases().iter().chain(package.keywords()))
                .map(|v| v.to_string())
                .collect(),
            version: package.version().into(),
            unity: package.unity().map(|v| (v.major(), v.minor())),
            changelog_url: package
                .changelog_url()
                .take_if(|x| safe_url(x))
                .map(|v| v.to_string()),
            documentation_url: package
                .documentation_url()
                .take_if(|x| safe_url(x))
                .map(|v| v.to_string()),
            vpm_dependencies: package
                .vpm_dependencies()
                .keys()
                .map(|x| x.to_string())
                .collect(),
            legacy_packages: package
                .legacy_packages()
                .iter()
                .map(|x| x.to_string())
                .collect(),
            is_yanked: package.is_yanked(),
        }
    }
}

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriPackage {
    #[serde(flatten)]
    base: TauriBasePackageInfo,

    source: TauriPackageSource,
}

#[derive(Serialize, specta::Type, Clone)]
enum TauriPackageSource {
    LocalUser,
    Remote { id: String, display_name: String },
}

impl TauriPackage {
    pub fn new(package: &PackageInfo) -> Self {
        let source = if let Some((repo, id)) = package.repo().and_then(|repo| {
            repo.id()
                .or(repo.url().map(|x| x.as_str()))
                .map(|id| (repo, id))
        }) {
            TauriPackageSource::Remote {
                id: id.to_string(),
                display_name: repo.name().unwrap_or(id).to_string(),
            }
        } else {
            TauriPackageSource::LocalUser
        };

        Self {
            base: TauriBasePackageInfo::new(package.package_json()),
            source,
        }
    }
}

pub(crate) async fn load_project(project_path: String) -> Result<UnityProject, RustError> {
    Ok(UnityProject::load(DefaultProjectIo::new(PathBuf::from(project_path).into())).await?)
}

trait IntoPathBuf {
    fn into_path_buf(self) -> Result<PathBuf, RustError>;
}

impl IntoPathBuf for tauri_plugin_dialog::FilePath {
    fn into_path_buf(self) -> Result<PathBuf, RustError> {
        match self {
            Self::Url(url) => url
                .to_file_path()
                .map_err(|_| RustError::unrecoverable_str("internal error: bad file url")),
            Self::Path(p) => Ok(p),
        }
    }
}

pub(crate) async fn create_dir_all_with_err(path: impl AsRef<Path>) -> Result<(), RustError> {
    async fn _create_dir_all_with_err(path: &Path) -> Result<(), RustError> {
        if let Err(e) = tokio::fs::create_dir_all(&path).await {
            log::error!(gui_toast = false; "failed to create dir: {e} (creating {path})", path = path.display());
            return if root_dir(path).exists() {
                // Drive exists, failed to create dir
                Err(localizable_error!("general:error:failed to create dir", err => path.display()))
            } else {
                // Drive does not exist
                Err(localizable_error!(
                    "general:error:failed to create dir missing drive"
                ))
            };
        }
        Ok(())
    }

    _create_dir_all_with_err(path.as_ref()).await
}

fn root_dir(mut path: &Path) -> &Path {
    while let Some(parent) = path.parent() {
        path = parent;
    }

    path
}
