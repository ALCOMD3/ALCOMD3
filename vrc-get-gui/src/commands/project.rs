use crate::activity_log::{
    ActivityDetail, ActivityImportance, ActivityInput, ActivityKind, ActivityLogState,
    ActivitySource, operations, summarize_path, target_from_path,
};
use crate::backend::packages::{
    package_is_available_for_display, repository_id as cached_repository_id,
};
use crate::backend::project_archive::{
    create_project_backup_archive, default_project_backup_name, normalize_project_backup_name,
    project_backup_archive_path,
};
use crate::backend::projects::load_project_details_snapshot;
use crate::commands::DEFAULT_UNITY_ARGUMENTS;
use crate::commands::async_command::*;
use crate::commands::prelude::*;
use crate::compressor::TauriCreateBackupProgress;
use crate::utils::project_backup_path;
use indexmap::{IndexMap, IndexSet};
use log::error;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::str::FromStr;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use vrc_get_vpm::AbortCheck;
use vrc_get_vpm::environment::{
    CURATED_REPOSITORY_ID, OFFICIAL_REPOSITORY_ID, PackageInstallProgressKind, PackageInstaller,
    VccDatabaseConnection,
};
use vrc_get_vpm::io::DefaultEnvironmentIo;
use vrc_get_vpm::unity_project::pending_project_changes::{
    ConflictInfo, PackageChange, RemoveReason,
};
use vrc_get_vpm::unity_project::{AddPackageOperation, PendingProjectChanges};
use vrc_get_vpm::version::{StrictEqVersion, Version};
use vrc_get_vpm::{PackageInfo, PackageManifest, package_manifest_is_unity_compatible};

pub const PROJECT_PACKAGE_CHANGED_EVENT: &str = "project-package-changed";

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriProjectDetails {
    unity: (u16, u8),
    unity_str: String,
    unity_revision: Option<String>,
    installed_packages: Vec<(String, TauriBasePackageInfo)>,
    should_resolve: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectPackageChangedEvent {
    pub(crate) project_path: String,
}

#[derive(Serialize, specta::Type, Clone)]
#[serde(tag = "type")]
pub enum TauriProjectApplyProgress {
    DownloadStarted {
        package_name: String,
    },
    DownloadFinished {
        package_name: String,
    },
    ExtractStarted {
        package_name: String,
    },
    ExtractFinished {
        package_name: String,
    },
    RemoveStarted {
        package_name: String,
    },
    RemoveFinished {
        package_name: String,
    },
    InstallStarted {
        package_name: String,
    },
    InstallFinished {
        package_name: String,
    },
    Failed {
        package_name: String,
        message: String,
    },
}

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriProjectPackageRows {
    project: TauriProjectDetails,
    packages: Vec<TauriProjectPackageRow>,
}

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriProjectPackageRow {
    id: String,
    info_source: TauriVersion,
    display_name: String,
    description: String,
    keywords: Vec<String>,
    unity_compatible: Vec<TauriPackage>,
    unity_incompatible: Vec<TauriPackage>,
    sources: Vec<String>,
    is_there_source: bool,
    visible_sources: Vec<String>,
    installed: Option<TauriProjectPackageInstalled>,
    latest: TauriProjectPackageLatestInfo,
    stable_latest: TauriProjectPackageLatestInfo,
    changelog_url: Option<TauriProjectPackageUrlInfo>,
    documentation_url: Option<TauriProjectPackageUrlInfo>,
}

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriProjectPackageInstalled {
    version: TauriVersion,
    yanked: bool,
}

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriProjectPackageUrlInfo {
    url: String,
    source: Option<TauriVersion>,
}

#[derive(Serialize, specta::Type, Clone)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum TauriProjectPackageLatestInfo {
    None,
    Contains {
        pkg: TauriPackage,
        has_unity_incompatible_latest: bool,
    },
    Upgradable {
        pkg: TauriPackage,
        has_unity_incompatible_latest: bool,
    },
}

#[tauri::command]
#[specta::specta]
pub async fn project_details(project_path: String) -> Result<TauriProjectDetails, RustError> {
    let snapshot = load_project_details_snapshot(project_path).await?;

    Ok(tauri_project_details_from_snapshot(snapshot))
}

#[tauri::command]
#[specta::specta]
pub async fn project_package_rows(
    app_handle: AppHandle,
    packages: State<'_, PackagesState>,
    settings: State<'_, SettingsState>,
    config: State<'_, GuiConfigState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
    project_path: String,
) -> Result<TauriProjectPackageRows, RustError> {
    let settings = settings.load(io.inner()).await?;
    let packages = packages
        .load(&settings, io.inner(), http.inner(), app_handle)
        .await?;
    let config = config.get();
    let hidden_user_repositories = config.gui_hidden_repositories.clone();
    let hide_local_user_packages = config.hide_local_user_packages;
    drop(config);

    let snapshot = load_project_details_snapshot(project_path).await?;
    let project = tauri_project_details_from_snapshot(snapshot.clone());
    let defined_repository_ids = settings
        .get_user_repos()
        .iter()
        .filter_map(|repo| repo.id().or(repo.url().map(url::Url::as_str)))
        .map(str::to_string)
        .collect::<Vec<_>>();

    Ok(TauriProjectPackageRows {
        packages: build_project_package_rows(
            packages.packages(),
            &snapshot,
            &hidden_user_repositories,
            hide_local_user_packages,
            settings.show_prerelease_packages(),
            &[
                OFFICIAL_REPOSITORY_ID.to_string(),
                CURATED_REPOSITORY_ID.to_string(),
            ],
            &defined_repository_ids,
        ),
        project,
    })
}

fn tauri_project_details_from_snapshot(
    snapshot: crate::backend::projects::ProjectDetailsSnapshot,
) -> TauriProjectDetails {
    TauriProjectDetails {
        unity: snapshot.unity,
        unity_str: snapshot.unity_str,
        unity_revision: snapshot.unity_revision,
        installed_packages: snapshot
            .installed_packages
            .into_iter()
            .map(|package| (package.id, TauriBasePackageInfo::new(&package.package)))
            .collect(),
        should_resolve: snapshot.should_resolve,
    }
}

#[derive(Clone)]
pub(crate) struct ProjectPackageRowAccumulator<'env> {
    id: String,
    info_source: Version,
    display_name: String,
    description: String,
    keywords: Vec<String>,
    unity_compatible: Vec<PackageInfo<'env>>,
    unity_incompatible: Vec<PackageInfo<'env>>,
    sources: IndexSet<String>,
    is_there_source: bool,
    visible_sources: IndexSet<String>,
    installed: Option<TauriProjectPackageInstalled>,
    latest: ProjectPackageLatestAccumulator<'env>,
    stable_latest: ProjectPackageLatestAccumulator<'env>,
    changelog_url: Option<ProjectPackageUrlAccumulator>,
    documentation_url: Option<ProjectPackageUrlAccumulator>,
}

#[derive(Clone, Copy)]
enum ProjectPackageLatestAccumulator<'env> {
    None,
    Contains {
        package: PackageInfo<'env>,
        has_unity_incompatible_latest: bool,
    },
    Upgradable {
        package: PackageInfo<'env>,
        has_unity_incompatible_latest: bool,
    },
}

#[derive(Clone)]
struct ProjectPackageUrlAccumulator {
    url: String,
    source: Option<Version>,
}

fn build_project_package_rows<'package, 'env>(
    packages: impl IntoIterator<Item = &'package PackageInfo<'env>>,
    project: &crate::backend::projects::ProjectDetailsSnapshot,
    hidden_user_repositories: &IndexSet<String>,
    hide_local_user_packages: bool,
    show_prerelease_packages: bool,
    default_repository_ids: &[String],
    defined_repository_ids: &[String],
) -> Vec<TauriProjectPackageRow>
where
    'env: 'package,
{
    let rows = build_project_package_row_accumulators(
        packages,
        project,
        hidden_user_repositories,
        hide_local_user_packages,
        show_prerelease_packages,
        default_repository_ids,
        defined_repository_ids,
    );
    let mut rows = rows
        .into_values()
        .map(project_package_row_from_accumulator)
        .collect::<Vec<_>>();
    rows.sort_by(
        |a, b| match (a.installed.is_some(), b.installed.is_some()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => Ordering::Equal,
        },
    );
    rows
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_project_package_row_accumulators<'package, 'env>(
    packages: impl IntoIterator<Item = &'package PackageInfo<'env>>,
    project: &crate::backend::projects::ProjectDetailsSnapshot,
    hidden_user_repositories: &IndexSet<String>,
    hide_local_user_packages: bool,
    show_prerelease_packages: bool,
    default_repository_ids: &[String],
    defined_repository_ids: &[String],
) -> IndexMap<String, ProjectPackageRowAccumulator<'env>>
where
    'env: 'package,
{
    let mut yanked_versions = HashSet::<(String, String)>::new();
    let mut packages_per_repository = IndexMap::<String, Vec<PackageInfo<'env>>>::new();
    let mut hidden_packages_per_repository = IndexMap::<String, Vec<PackageInfo<'env>>>::new();
    let mut user_packages = Vec::<PackageInfo<'env>>::new();
    let mut hidden_user_packages = Vec::<PackageInfo<'env>>::new();

    for package in packages {
        if !package_is_available_for_display(package, show_prerelease_packages) {
            if package.package_json().is_yanked()
                && (show_prerelease_packages || !package.version().is_pre())
            {
                yanked_versions.insert((package.name().to_string(), package.version().to_string()));
            }
            continue;
        }

        let package = *package;
        let Some(repo) = package.repo() else {
            if hide_local_user_packages {
                hidden_user_packages.push(package);
            } else {
                user_packages.push(package);
            }
            continue;
        };
        let Some(repo_id) = cached_repository_id(repo).map(str::to_string) else {
            packages_per_repository
                .entry(repo.url().map(ToString::to_string).unwrap_or_default())
                .or_default()
                .push(package);
            continue;
        };

        if hidden_user_repositories
            .iter()
            .any(|hidden| hidden == &repo_id)
        {
            hidden_packages_per_repository
                .entry(repo_id)
                .or_default()
                .push(package);
        } else {
            packages_per_repository
                .entry(repo_id)
                .or_default()
                .push(package);
        }
    }

    let mut rows = IndexMap::<String, ProjectPackageRowAccumulator<'env>>::new();
    for repository_id in default_repository_ids {
        if let Some(packages) = packages_per_repository.get(repository_id) {
            for package in packages {
                add_visible_package_row(&mut rows, *package, project);
            }
        }
    }
    for package in user_packages {
        add_visible_package_row(&mut rows, package, project);
    }
    for package in hidden_user_packages {
        add_hidden_package_source(&mut rows, package);
    }
    for repository_id in default_repository_ids {
        packages_per_repository.shift_remove(repository_id);
    }

    for repository_id in defined_repository_ids {
        if let Some(packages) = packages_per_repository.shift_remove(repository_id) {
            for package in packages {
                add_visible_package_row(&mut rows, package, project);
            }
        }
    }

    for packages in packages_per_repository.into_values() {
        for package in packages {
            add_visible_package_row(&mut rows, package, project);
        }
    }

    for packages in hidden_packages_per_repository.into_values() {
        for package in packages {
            add_hidden_package_source(&mut rows, package);
        }
    }

    for row in rows.values_mut() {
        sort_package_versions(&mut row.unity_compatible);
        sort_package_versions(&mut row.unity_incompatible);
        row.latest =
            project_package_latest(&row.unity_compatible, &row.unity_incompatible, None, false);
        row.stable_latest =
            project_package_latest(&row.unity_compatible, &row.unity_incompatible, None, true);
    }

    for installed_package in &project.installed_packages {
        let row = get_project_package_row(&mut rows, &installed_package.package);
        set_project_package_url_info(
            &mut row.changelog_url,
            installed_package
                .package
                .changelog_url()
                .take_if(|url| safe_url(url)),
            None,
        );
        set_project_package_url_info(
            &mut row.documentation_url,
            installed_package
                .package
                .documentation_url()
                .take_if(|url| safe_url(url)),
            None,
        );

        row.display_name = installed_package
            .package
            .display_name()
            .unwrap_or(installed_package.package.name())
            .to_string();
        row.keywords = installed_package
            .package
            .aliases()
            .iter()
            .chain(installed_package.package.keywords())
            .map(|value| value.to_string())
            .chain(row.keywords.iter().cloned())
            .collect();
        row.installed = Some(TauriProjectPackageInstalled {
            version: installed_package.package.version().into(),
            yanked: installed_package.package.is_yanked()
                || yanked_versions.contains(&(
                    installed_package.package.name().to_string(),
                    installed_package.package.version().to_string(),
                )),
        });
        row.is_there_source = true;
        row.latest = project_package_latest(
            &row.unity_compatible,
            &row.unity_incompatible,
            Some(&installed_package.package),
            false,
        );
        row.stable_latest = project_package_latest(
            &row.unity_compatible,
            &row.unity_incompatible,
            Some(&installed_package.package),
            true,
        );
    }

    remove_other_vrchat_sdk_rows(&mut rows);

    for installed_package in &project.installed_packages {
        for legacy_package in installed_package.package.legacy_packages() {
            if rows
                .get(legacy_package.as_ref())
                .is_some_and(|row| row.installed.is_some())
            {
                continue;
            }
            rows.shift_remove(legacy_package.as_ref());
        }
    }

    rows
}

pub(crate) fn project_package_row_compatible_packages<'row, 'env>(
    row: &'row ProjectPackageRowAccumulator<'env>,
) -> &'row [PackageInfo<'env>] {
    &row.unity_compatible
}

pub(crate) fn project_package_row_incompatible_packages<'row, 'env>(
    row: &'row ProjectPackageRowAccumulator<'env>,
) -> &'row [PackageInfo<'env>] {
    &row.unity_incompatible
}

fn get_project_package_row<'rows, 'env>(
    rows: &'rows mut IndexMap<String, ProjectPackageRowAccumulator<'env>>,
    package: &PackageManifest,
) -> &'rows mut ProjectPackageRowAccumulator<'env> {
    rows.entry(package.name().to_string())
        .or_insert_with(|| ProjectPackageRowAccumulator {
            id: package.name().to_string(),
            info_source: package.version().clone(),
            display_name: package.display_name().unwrap_or(package.name()).to_string(),
            description: package.description().unwrap_or_default().to_string(),
            keywords: package
                .aliases()
                .iter()
                .chain(package.keywords())
                .map(|value| value.to_string())
                .collect(),
            unity_compatible: Vec::new(),
            unity_incompatible: Vec::new(),
            sources: IndexSet::new(),
            is_there_source: false,
            visible_sources: IndexSet::new(),
            installed: None,
            latest: ProjectPackageLatestAccumulator::None,
            stable_latest: ProjectPackageLatestAccumulator::None,
            changelog_url: None,
            documentation_url: None,
        })
}

fn add_visible_package_row<'env>(
    rows: &mut IndexMap<String, ProjectPackageRowAccumulator<'env>>,
    package: PackageInfo<'env>,
    project: &crate::backend::projects::ProjectDetailsSnapshot,
) {
    let row = get_project_package_row(rows, package.package_json());
    row.is_there_source = true;

    set_project_package_url_info(
        &mut row.changelog_url,
        package
            .package_json()
            .changelog_url()
            .take_if(|url| safe_url(url)),
        Some(package.version()),
    );
    set_project_package_url_info(
        &mut row.documentation_url,
        package
            .package_json()
            .documentation_url()
            .take_if(|url| safe_url(url)),
        Some(package.version()),
    );

    if package.version() > &row.info_source {
        row.info_source = package.version().clone();
        row.display_name = package
            .package_json()
            .display_name()
            .unwrap_or(package.name())
            .to_string();
        if let Some(description) = package.package_json().description() {
            row.description = description.to_string();
        }
        row.keywords = package
            .package_json()
            .aliases()
            .iter()
            .chain(package.package_json().keywords())
            .map(|value| value.to_string())
            .collect();
    }

    if package_manifest_is_unity_compatible(package.package_json(), project.unity_version) {
        row.unity_compatible.push(package);
    } else {
        row.unity_incompatible.push(package);
    }

    let source_name = project_package_source_name(package);
    row.sources.insert(source_name.clone());
    row.visible_sources.insert(source_name);
}

fn add_hidden_package_source<'env>(
    rows: &mut IndexMap<String, ProjectPackageRowAccumulator<'env>>,
    package: PackageInfo<'env>,
) {
    let row = get_project_package_row(rows, package.package_json());
    row.is_there_source = true;
    row.sources.insert(project_package_source_name(package));
}

fn sort_package_versions(packages: &mut Vec<PackageInfo<'_>>) {
    let mut by_version = IndexMap::<String, PackageInfo<'_>>::new();
    for package in packages.drain(..) {
        by_version.insert(package.version().to_string(), package);
    }
    packages.extend(by_version.into_values());
    packages.sort_by(|a, b| b.version().cmp(a.version()));
}

fn project_package_latest<'env>(
    compatible: &[PackageInfo<'env>],
    incompatible: &[PackageInfo<'env>],
    installed: Option<&PackageManifest>,
    stable_only: bool,
) -> ProjectPackageLatestAccumulator<'env> {
    let Some(package) = first_project_package_version(compatible, stable_only) else {
        return ProjectPackageLatestAccumulator::None;
    };

    let has_unity_incompatible_latest = first_project_package_version(incompatible, stable_only)
        .is_some_and(|incompatible| incompatible.version() > package.version());

    if installed.is_some_and(|installed| installed.version() < package.version()) {
        ProjectPackageLatestAccumulator::Upgradable {
            package,
            has_unity_incompatible_latest,
        }
    } else {
        ProjectPackageLatestAccumulator::Contains {
            package,
            has_unity_incompatible_latest,
        }
    }
}

fn first_project_package_version<'env>(
    packages: &[PackageInfo<'env>],
    stable_only: bool,
) -> Option<PackageInfo<'env>> {
    packages
        .iter()
        .copied()
        .find(|package| !stable_only || !package.version().is_pre())
}

fn set_project_package_url_info(
    current: &mut Option<ProjectPackageUrlAccumulator>,
    url: Option<&url::Url>,
    version: Option<&Version>,
) {
    let Some(url) = url else {
        return;
    };
    let next = ProjectPackageUrlAccumulator {
        url: url.to_string(),
        source: version.cloned(),
    };
    let Some(current_value) = current else {
        *current = Some(next);
        return;
    };

    match (&current_value.source, version) {
        (_, None) => *current = Some(next),
        (None, Some(_)) => {}
        (Some(current_version), Some(version)) if current_version < version => {
            *current = Some(next);
        }
        _ => {}
    }
}

fn project_package_source_name(package: PackageInfo<'_>) -> String {
    if let Some(repo) = package.repo() {
        let id = cached_repository_id(repo);
        repo.name().or(id).unwrap_or("Unknown").to_string()
    } else {
        "User".to_string()
    }
}

fn remove_other_vrchat_sdk_rows(rows: &mut IndexMap<String, ProjectPackageRowAccumulator<'_>>) {
    let is_avatars_sdk_installed = rows
        .get("com.vrchat.avatars")
        .is_some_and(|row| row.installed.is_some());
    let is_worlds_sdk_installed = rows
        .get("com.vrchat.worlds")
        .is_some_and(|row| row.installed.is_some());
    if is_avatars_sdk_installed == is_worlds_sdk_installed {
        return;
    }

    let mut dependant_packages = HashMap::<String, IndexSet<String>>::new();
    for row in rows.values() {
        let Some(package) = project_package_latest_package(row.latest) else {
            continue;
        };
        for dependency in package.package_json().vpm_dependencies().keys() {
            dependant_packages
                .entry(dependency.to_string())
                .or_default()
                .insert(row.id.clone());
        }
    }

    let mut to_remove = IndexSet::<String>::new();
    if is_avatars_sdk_installed {
        to_remove.insert("com.vrchat.worlds".to_string());
    } else if is_worlds_sdk_installed {
        to_remove.insert("com.vrchat.avatars".to_string());
    }

    while let Some(package_id) = to_remove.pop() {
        if rows
            .get(&package_id)
            .is_some_and(|row| row.installed.is_some())
        {
            continue;
        }
        if rows.shift_remove(&package_id).is_none() {
            continue;
        }
        if let Some(dependants) = dependant_packages.get(&package_id) {
            for dependant in dependants {
                to_remove.insert(dependant.clone());
            }
        }
    }
}

fn project_package_latest_package<'env>(
    latest: ProjectPackageLatestAccumulator<'env>,
) -> Option<PackageInfo<'env>> {
    match latest {
        ProjectPackageLatestAccumulator::None => None,
        ProjectPackageLatestAccumulator::Contains { package, .. }
        | ProjectPackageLatestAccumulator::Upgradable { package, .. } => Some(package),
    }
}

fn project_package_row_from_accumulator(
    value: ProjectPackageRowAccumulator<'_>,
) -> TauriProjectPackageRow {
    TauriProjectPackageRow {
        id: value.id,
        info_source: (&value.info_source).into(),
        display_name: value.display_name,
        description: value.description,
        keywords: value.keywords,
        unity_compatible: value
            .unity_compatible
            .into_iter()
            .map(|package| TauriPackage::new(&package))
            .collect(),
        unity_incompatible: value
            .unity_incompatible
            .into_iter()
            .map(|package| TauriPackage::new(&package))
            .collect(),
        sources: value.sources.into_iter().collect(),
        is_there_source: value.is_there_source,
        visible_sources: value.visible_sources.into_iter().collect(),
        installed: value.installed,
        latest: project_package_latest_from_accumulator(value.latest),
        stable_latest: project_package_latest_from_accumulator(value.stable_latest),
        changelog_url: value
            .changelog_url
            .map(project_package_url_from_accumulator),
        documentation_url: value
            .documentation_url
            .map(project_package_url_from_accumulator),
    }
}

fn project_package_latest_from_accumulator(
    value: ProjectPackageLatestAccumulator<'_>,
) -> TauriProjectPackageLatestInfo {
    match value {
        ProjectPackageLatestAccumulator::None => TauriProjectPackageLatestInfo::None,
        ProjectPackageLatestAccumulator::Contains {
            package,
            has_unity_incompatible_latest,
        } => TauriProjectPackageLatestInfo::Contains {
            pkg: TauriPackage::new(&package),
            has_unity_incompatible_latest,
        },
        ProjectPackageLatestAccumulator::Upgradable {
            package,
            has_unity_incompatible_latest,
        } => TauriProjectPackageLatestInfo::Upgradable {
            pkg: TauriPackage::new(&package),
            has_unity_incompatible_latest,
        },
    }
}

fn project_package_url_from_accumulator(
    value: ProjectPackageUrlAccumulator,
) -> TauriProjectPackageUrlInfo {
    TauriProjectPackageUrlInfo {
        url: value.url,
        source: value.source.as_ref().map(|version| version.into()),
    }
}

#[derive(Serialize, specta::Type)]
pub struct TauriPendingProjectChanges {
    changes_version: u32,
    package_changes: Vec<(String, TauriPackageChange)>,

    remove_legacy_files: Vec<String>,
    remove_legacy_folders: Vec<String>,

    conflicts: Vec<(String, TauriConflictInfo)>,
}

impl TauriPendingProjectChanges {
    pub fn new(version: u32, changes: &PendingProjectChanges) -> Self {
        TauriPendingProjectChanges {
            changes_version: version,
            package_changes: changes
                .package_changes()
                .iter()
                .filter_map(|(name, change)| Some((name.to_string(), change.try_into().ok()?)))
                .collect(),
            remove_legacy_files: changes
                .remove_legacy_files()
                .iter()
                .map(|(x, _)| x.to_string_lossy().into_owned())
                .collect(),
            remove_legacy_folders: changes
                .remove_legacy_folders()
                .iter()
                .map(|(x, _)| x.to_string_lossy().into_owned())
                .collect(),
            conflicts: changes
                .conflicts()
                .iter()
                .map(|(name, info)| (name.to_string(), info.into()))
                .collect(),
        }
    }
}

#[derive(Serialize, specta::Type)]
enum TauriPackageChange {
    InstallNew(Box<TauriBasePackageInfo>),
    Remove(TauriRemoveReason),
}

impl TryFrom<&PackageChange<'_>> for TauriPackageChange {
    type Error = ();

    fn try_from(value: &PackageChange) -> Result<Self, ()> {
        Ok(match value {
            PackageChange::Install(install) => TauriPackageChange::InstallNew(
                TauriBasePackageInfo::new(install.install_package().ok_or(())?.package_json())
                    .into(),
            ),
            PackageChange::Remove(remove) => TauriPackageChange::Remove(remove.reason().into()),
        })
    }
}

#[derive(Serialize, specta::Type)]
enum TauriRemoveReason {
    Requested,
    Legacy,
    Unused,
}

impl From<RemoveReason> for TauriRemoveReason {
    fn from(value: RemoveReason) -> Self {
        match value {
            RemoveReason::Requested => Self::Requested,
            RemoveReason::Legacy => Self::Legacy,
            RemoveReason::Unused => Self::Unused,
        }
    }
}

#[derive(Serialize, specta::Type)]
struct TauriConflictInfo {
    packages: Vec<String>,
    unity_conflict: bool,
    unlocked_names: Vec<String>,
}

impl From<&ConflictInfo> for TauriConflictInfo {
    fn from(value: &ConflictInfo) -> Self {
        Self {
            packages: value
                .conflicting_packages()
                .iter()
                .map(|x| x.to_string())
                .collect(),
            unity_conflict: value.conflicts_with_unity(),
            unlocked_names: value
                .unlocked_names()
                .iter()
                .map(|x| x.to_string())
                .collect(),
        }
    }
}

macro_rules! changes {
    ($packages_ref: ident, $changes: ident, |$collection: pat_param, $packages: pat_param| $body: expr) => {{
        $changes
            .build_changes(
                &$packages_ref,
                |$collection, $packages| async { Ok($body) },
                TauriPendingProjectChanges::new,
            )
            .await
    }};
    ($packages_ref: ident, $changes: ident, |$collection: pat_param| $body: expr) => {{
        $changes
            .build_changes_no_list(
                &$packages_ref,
                |$collection| async { Ok($body) },
                TauriPendingProjectChanges::new,
            )
            .await
    }};
}

fn project_activity(
    operation: &'static str,
    summary: &'static str,
    project_path: &str,
) -> ActivityInput {
    project_activity_with_source(ActivitySource::Gui, operation, summary, project_path)
}

fn project_activity_with_source(
    source: ActivitySource,
    operation: &'static str,
    summary: &'static str,
    project_path: &str,
) -> ActivityInput {
    ActivityInput::new(
        source,
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
pub async fn project_install_packages(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    changes: State<'_, ChangesState>,
    io: State<'_, DefaultEnvironmentIo>,
    project_path: String,
    installs: Vec<(String, String)>,
) -> Result<TauriPendingProjectChanges, RustError> {
    let activity = app.state::<ActivityLogState>();
    let package_count = installs.len();
    let input = project_activity(
        operations::PROJECT_INSTALL_PACKAGES,
        "Preparing package install changes",
        &project_path,
    )
    .add_detail(ActivityDetail::new("packages", package_count.to_string()));
    activity
        .track_result(
            Some(&app),
            input,
            "Package install changes prepared",
            Vec::new(),
            async move {
                let settings = settings.load(io.inner()).await?;
                let Some(packages) = packages.get() else {
                    return Err(RustError::unrecoverable_str(
                        "Internal Error: environment version mismatch",
                    ));
                };
                let Some(installs) = installs
                    .into_iter()
                    .map(|(id, v)| Some((id, Version::from_str(&v).ok()?)))
                    .collect::<Option<Vec<_>>>()
                else {
                    return Err(RustError::unrecoverable_str("bad version file"));
                };

                changes!(packages, changes, |collection, packages| {
                    let Some(installing_packages) = installs
                        .iter()
                        .map(|(id, version)| {
                            packages
                                .iter()
                                .find(|&p| {
                                    p.name() == id
                                        && StrictEqVersion(p.version()) == StrictEqVersion(version)
                                })
                                .copied()
                        })
                        .collect::<Option<Vec<_>>>()
                    else {
                        return Err(RustError::unrecoverable_str("some packages not found"));
                    };

                    let unity_project = load_project(project_path).await?;

                    let allow_prerelease = settings.show_prerelease_packages();

                    unity_project
                        .add_package_request(
                            collection,
                            &installing_packages,
                            AddPackageOperation::AutoDetected,
                            allow_prerelease,
                        )
                        .await?
                })
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn project_reinstall_packages(
    app_handle: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    changes: State<'_, ChangesState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
    project_path: String,
    package_ids: Vec<String>,
) -> Result<TauriPendingProjectChanges, RustError> {
    let app_for_activity = app_handle.clone();
    let activity = app_for_activity.state::<ActivityLogState>();
    let package_count = package_ids.len();
    let input = project_activity(
        operations::PROJECT_REINSTALL_PACKAGES,
        "Preparing package reinstall changes",
        &project_path,
    )
    .add_detail(ActivityDetail::new("packages", package_count.to_string()));
    activity
        .track_result(
            Some(&app_for_activity),
            input,
            "Package reinstall changes prepared",
            Vec::new(),
            async move {
                let settings = settings.load(&io).await?;
                let packages = packages.load(&settings, &io, &http, app_handle).await?;

                changes!(packages, changes, |collection| {
                    let unity_project = load_project(project_path).await?;

                    let package_ids = package_ids.iter().map(|x| x.as_str()).collect::<Vec<_>>();

                    unity_project
                        .reinstall_request(collection, &package_ids)
                        .await?
                })
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn project_resolve(
    app_handle: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    changes: State<'_, ChangesState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
    project_path: String,
) -> Result<TauriPendingProjectChanges, RustError> {
    let app_for_activity = app_handle.clone();
    let activity = app_for_activity.state::<ActivityLogState>();
    let input = project_activity(
        operations::PROJECT_RESOLVE_PACKAGES,
        "Preparing package resolve changes",
        &project_path,
    );
    activity
        .track_result(
            Some(&app_for_activity),
            input,
            "Package resolve changes prepared",
            Vec::new(),
            async move {
                let settings = settings.load(&io).await?;
                let packages = packages.load(&settings, &io, &http, app_handle).await?;
                changes!(packages, changes, |collection| {
                    let unity_project = load_project(project_path).await?;

                    unity_project.resolve_request(collection).await?
                })
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn project_remove_packages(
    app: AppHandle,
    changes_state: State<'_, ChangesState>,
    project_path: String,
    names: Vec<String>,
) -> Result<TauriPendingProjectChanges, RustError> {
    let activity = app.state::<ActivityLogState>();
    let package_count = names.len();
    let input = project_activity(
        operations::PROJECT_REMOVE_PACKAGES,
        "Preparing package remove changes",
        &project_path,
    )
    .add_detail(ActivityDetail::new("packages", package_count.to_string()));
    activity
        .track_result(
            Some(&app),
            input,
            "Package remove changes prepared",
            Vec::new(),
            async move {
                let unity_project = load_project(project_path).await?;

                let names = names.iter().map(|x| x.as_str()).collect::<Vec<_>>();

                let changes = unity_project.remove_request(&names).await?;

                Ok(changes_state.set(changes, TauriPendingProjectChanges::new))
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn project_apply_pending_changes(
    changes: State<'_, ChangesState>,
    project_apply: State<'_, ProjectApplyState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
    window: Window,
    channel: String,
    project_path: String,
    changes_version: u32,
) -> Result<(), RustError> {
    let app = window.app_handle().clone();
    let activity = app.state::<ActivityLogState>();
    let activity_tracker = activity.start_activity(
        Some(&app),
        project_activity(
            operations::PROJECT_APPLY_CHANGES,
            "Applying pending project package changes",
            &project_path,
        ),
    );
    let abort = AbortCheck::new();
    if !project_apply.try_start(abort.clone()) {
        let error = RustError::unrecoverable_str("project changes already applying");
        activity.finish_failed(
            Some(&app),
            &activity_tracker,
            "Project package changes failed",
            Vec::new(),
            &error,
        );
        return Err(error);
    }

    let installer =
        PackageInstaller::new(io.inner(), Some(http.inner())).with_progress(move |progress| {
            let package_name = progress.package_name.to_string();
            let event = match progress.kind {
                PackageInstallProgressKind::DownloadStarted => {
                    TauriProjectApplyProgress::DownloadStarted { package_name }
                }
                PackageInstallProgressKind::DownloadFinished => {
                    TauriProjectApplyProgress::DownloadFinished { package_name }
                }
                PackageInstallProgressKind::ExtractStarted => {
                    TauriProjectApplyProgress::ExtractStarted { package_name }
                }
                PackageInstallProgressKind::ExtractFinished => {
                    TauriProjectApplyProgress::ExtractFinished { package_name }
                }
                PackageInstallProgressKind::RemoveStarted => {
                    TauriProjectApplyProgress::RemoveStarted { package_name }
                }
                PackageInstallProgressKind::RemoveFinished => {
                    TauriProjectApplyProgress::RemoveFinished { package_name }
                }
                PackageInstallProgressKind::InstallStarted => {
                    TauriProjectApplyProgress::InstallStarted { package_name }
                }
                PackageInstallProgressKind::InstallFinished => {
                    TauriProjectApplyProgress::InstallFinished { package_name }
                }
                PackageInstallProgressKind::Failed { message } => {
                    TauriProjectApplyProgress::Failed {
                        package_name,
                        message: message.to_string(),
                    }
                }
            };
            window.emit(&channel, event).ok();
        });

    let result = async {
        let Some(mut changes) = changes.get_versioned(changes_version) else {
            return Err(RustError::unrecoverable_str("changes version mismatch"));
        };

        let changes = changes.take_changes();
        let mut unity_project = load_project(project_path).await?;

        unity_project
            .apply_pending_changes_with_abort(&installer, changes, &abort)
            .await?;

        update_project_last_modified(&io, unity_project.project_dir()).await;
        Ok(())
    }
    .await;
    project_apply.finish();
    match &result {
        Ok(()) => {
            activity.finish_success(
                Some(&app),
                &activity_tracker,
                "Project package changes applied",
                Vec::new(),
            );
        }
        Err(error) => {
            if is_project_apply_abort(error) {
                activity.finish_cancelled(
                    Some(&app),
                    &activity_tracker,
                    "Project package changes cancelled",
                    Vec::new(),
                );
            } else {
                activity.finish_failed(
                    Some(&app),
                    &activity_tracker,
                    "Project package changes failed",
                    Vec::new(),
                    error,
                );
            }
        }
    }
    result
}

pub(crate) async fn project_apply_pending_changes_from_prepared(
    app: AppHandle,
    project_path: String,
    changes_version: u32,
) -> Result<(), RustError> {
    project_apply_pending_changes_from_prepared_inner(app, project_path, changes_version, None)
        .await
}

pub(crate) async fn project_apply_pending_changes_from_prepared_with_abort(
    app: AppHandle,
    project_path: String,
    changes_version: u32,
    abort: AbortCheck,
) -> Result<(), RustError> {
    project_apply_pending_changes_from_prepared_inner(
        app,
        project_path,
        changes_version,
        Some(abort),
    )
    .await
}

async fn project_apply_pending_changes_from_prepared_inner(
    app: AppHandle,
    project_path: String,
    changes_version: u32,
    prestarted_abort: Option<AbortCheck>,
) -> Result<(), RustError> {
    let changes = app.state::<ChangesState>();
    let project_apply = app.state::<ProjectApplyState>();
    let io = app.state::<DefaultEnvironmentIo>();
    let http = app.state::<reqwest::Client>();
    let activity = app.state::<ActivityLogState>();
    let activity_tracker = activity.start_activity(
        Some(&app),
        project_activity_with_source(
            ActivitySource::Mcp,
            operations::PROJECT_APPLY_CHANGES,
            "Applying pending project package changes",
            &project_path,
        ),
    );
    let started_here = prestarted_abort.is_none();
    let abort = prestarted_abort.unwrap_or_else(AbortCheck::new);
    if started_here && !project_apply.try_start(abort.clone()) {
        let error = RustError::unrecoverable_str("project changes already applying");
        activity.finish_failed(
            Some(&app),
            &activity_tracker,
            "Project package changes failed",
            Vec::new(),
            &error,
        );
        return Err(error);
    }

    let installer = PackageInstaller::new(io.inner(), Some(http.inner()));
    let project_path_for_event = project_path.clone();
    let result = async {
        let Some(mut changes) = changes.get_versioned(changes_version) else {
            return Err(RustError::unrecoverable_str("changes version mismatch"));
        };

        let changes = changes.take_changes();
        let mut unity_project = load_project(project_path).await?;

        unity_project
            .apply_pending_changes_with_abort(&installer, changes, &abort)
            .await?;

        update_project_last_modified(io.inner(), unity_project.project_dir()).await;
        Ok(())
    }
    .await;
    if started_here {
        project_apply.finish();
    }
    match &result {
        Ok(()) => {
            let _ = app.emit(
                PROJECT_PACKAGE_CHANGED_EVENT,
                ProjectPackageChangedEvent {
                    project_path: project_path_for_event,
                },
            );
            activity.finish_success(
                Some(&app),
                &activity_tracker,
                "Project package changes applied",
                Vec::new(),
            );
        }
        Err(error) => {
            if is_project_apply_abort(error) {
                activity.finish_cancelled(
                    Some(&app),
                    &activity_tracker,
                    "Project package changes cancelled",
                    Vec::new(),
                );
            } else {
                activity.finish_failed(
                    Some(&app),
                    &activity_tracker,
                    "Project package changes failed",
                    Vec::new(),
                    error,
                );
            }
        }
    }
    result
}

fn is_project_apply_abort(error: &RustError) -> bool {
    matches!(error, RustError::Unrecoverable { message } if message == "Aborted")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::projects::{ProjectDetailsSnapshot, ProjectInstalledPackageSnapshot};
    use serde_json::{Value, json};
    use vrc_get_vpm::repository::{LocalCachedRepository, RemoteRepository};
    use vrc_get_vpm::version::UnityVersion;

    #[test]
    fn project_apply_abort_detection_matches_abort_error_only() {
        assert!(is_project_apply_abort(&RustError::Unrecoverable {
            message: "Aborted".to_string(),
        }));
        assert!(!is_project_apply_abort(&RustError::Unrecoverable {
            message: "disk write failed".to_string(),
        }));
    }

    #[test]
    fn project_package_rows_keep_later_source_for_same_version() {
        let official_manifest = test_package_manifest(json!({
            "name": "com.example.same",
            "displayName": "Official Package",
            "version": "1.0.0",
        }));
        let user_manifest = test_package_manifest(json!({
            "name": "com.example.same",
            "displayName": "User Package",
            "version": "1.0.0",
        }));
        let official_repository = test_cached_repository(json!({
            "id": OFFICIAL_REPOSITORY_ID,
            "url": "https://example.com/official.json",
            "packages": {},
        }));
        let user_repository = test_cached_repository(json!({
            "id": "com.example.user",
            "url": "https://example.com/user.json",
            "packages": {},
        }));
        let official_package = PackageInfo::remote(&official_manifest, &official_repository);
        let user_package = PackageInfo::remote(&user_manifest, &user_repository);
        let project = test_project_details_snapshot();
        let rows = build_project_package_row_accumulators(
            [&official_package, &user_package],
            &project,
            &IndexSet::new(),
            false,
            false,
            &[OFFICIAL_REPOSITORY_ID.to_string()],
            &["com.example.user".to_string()],
        );

        let row = rows.get("com.example.same").unwrap();
        let compatible = project_package_row_compatible_packages(row);

        assert_eq!(compatible.len(), 1);
        assert_eq!(
            compatible[0].package_json().display_name(),
            Some("User Package")
        );
        assert_eq!(
            compatible[0].repo().and_then(cached_repository_id),
            Some("com.example.user")
        );
    }

    #[test]
    fn project_package_rows_keep_installed_legacy_packages() {
        let modern_manifest = test_package_manifest(json!({
            "name": "com.example.modern",
            "version": "2.0.0",
            "legacyPackages": [
                "com.example.legacy-installed",
                "com.example.legacy-available"
            ],
        }));
        let installed_legacy_manifest = test_package_manifest(json!({
            "name": "com.example.legacy-installed",
            "version": "1.0.0",
        }));
        let available_legacy_manifest = test_package_manifest(json!({
            "name": "com.example.legacy-available",
            "version": "1.0.0",
        }));
        let repository = test_cached_repository(json!({
            "id": OFFICIAL_REPOSITORY_ID,
            "url": "https://example.com/official.json",
            "packages": {},
        }));
        let modern_package = PackageInfo::remote(&modern_manifest, &repository);
        let available_legacy_package = PackageInfo::remote(&available_legacy_manifest, &repository);
        let mut project = test_project_details_snapshot();
        project.installed_packages = vec![
            test_installed_package(modern_manifest.clone()),
            test_installed_package(installed_legacy_manifest),
        ];

        let rows = build_project_package_row_accumulators(
            [&modern_package, &available_legacy_package],
            &project,
            &IndexSet::new(),
            false,
            false,
            &[OFFICIAL_REPOSITORY_ID.to_string()],
            &[],
        );

        assert!(
            rows.get("com.example.legacy-installed")
                .is_some_and(|row| row.installed.is_some())
        );
        assert!(!rows.contains_key("com.example.legacy-available"));
    }

    #[test]
    fn remove_other_vrchat_sdk_rows_keeps_installed_dependants() {
        let avatars_manifest = test_package_manifest(json!({
            "name": "com.vrchat.avatars",
            "version": "3.7.0",
        }));
        let worlds_manifest = test_package_manifest(json!({
            "name": "com.vrchat.worlds",
            "version": "3.7.0",
        }));
        let dependant_manifest = test_package_manifest(json!({
            "name": "com.example.worlds-tool",
            "version": "1.0.0",
            "vpmDependencies": {
                "com.vrchat.worlds": ">=3.0.0"
            },
        }));
        let repository = test_cached_repository(json!({
            "id": OFFICIAL_REPOSITORY_ID,
            "url": "https://example.com/official.json",
            "packages": {},
        }));
        let worlds_package = PackageInfo::remote(&worlds_manifest, &repository);
        let dependant_package = PackageInfo::remote(&dependant_manifest, &repository);
        let mut project = test_project_details_snapshot();
        project.installed_packages = vec![
            test_installed_package(avatars_manifest),
            test_installed_package(dependant_manifest.clone()),
        ];

        let rows = build_project_package_row_accumulators(
            [&worlds_package, &dependant_package],
            &project,
            &IndexSet::new(),
            false,
            false,
            &[OFFICIAL_REPOSITORY_ID.to_string()],
            &[],
        );

        assert!(!rows.contains_key("com.vrchat.worlds"));
        assert!(
            rows.get("com.example.worlds-tool")
                .is_some_and(|row| row.installed.is_some())
        );
    }

    fn test_project_details_snapshot() -> ProjectDetailsSnapshot {
        let unity_version = UnityVersion::new_f1(2022, 3, 22);
        ProjectDetailsSnapshot {
            unity: (2022, 3),
            unity_version,
            unity_str: unity_version.to_string(),
            unity_revision: None,
            installed_packages: Vec::new(),
            should_resolve: false,
        }
    }

    fn test_installed_package(package: PackageManifest) -> ProjectInstalledPackageSnapshot {
        ProjectInstalledPackageSnapshot {
            id: package.name().to_string(),
            package,
        }
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
}

#[tauri::command]
#[specta::specta]
pub async fn project_cancel_apply_pending_changes(
    app: AppHandle,
    project_apply: State<'_, ProjectApplyState>,
) -> Result<bool, RustError> {
    let activity = app.state::<ActivityLogState>();
    let aborted = project_apply.abort();
    activity.record_info(
        Some(&app),
        ActivityInput::new(
            ActivitySource::Gui,
            ActivityKind::Write,
            ActivityImportance::Primary,
            operations::PROJECT_APPLY_CHANGES,
            if aborted {
                "Cancelled project package changes"
            } else {
                "Requested project package change cancellation"
            },
        ),
    );
    Ok(aborted)
}

#[tauri::command]
#[specta::specta]
pub async fn project_clear_pending_changes(
    changes: State<'_, ChangesState>,
) -> Result<(), RustError> {
    changes.clear_cache();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn project_migrate_project_to_2022(
    app_handle: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
    project_path: String,
) -> Result<(), RustError> {
    {
        let settings = settings.load(io.inner()).await?;
        let packages = packages.load(&settings, &io, &http, app_handle).await?;
        let mut unity_project = load_project(project_path).await?;

        let installer = PackageInstaller::new(io.inner(), Some(http.inner()));

        unity_project
            .migrate_unity_2022(packages.collection(), &installer)
            .await?;

        update_project_last_modified(&io, unity_project.project_dir()).await;

        Ok(())
    }
}

#[derive(Serialize, specta::Type, Clone)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub enum TauriCallUnityForMigrationResult {
    ExistsWithNonZero { status: String },
    FinishedSuccessfully,
}

#[allow(dead_code)]
#[tauri::command]
#[specta::specta]
pub async fn project_call_unity_for_migration(
    window: Window,
    channel: String,
    project_path: String,
    unity_path: String,
) -> Result<AsyncCallResult<String, TauriCallUnityForMigrationResult>, RustError> {
    async_command(channel, window, async {
        let unity_project = load_project(project_path).await?;

        With::<String>::continue_async(move |context| async move {
            let mut child = Command::new(unity_path)
                .args([
                    "-quit".as_ref(),
                    "-batchmode".as_ref(),
                    "-ignorecompilererrors".as_ref(),
                    // https://docs.unity3d.com/Manual/EditorCommandLineArguments.html
                    "-logFile".as_ref(),
                    "-".as_ref(),
                    "-projectPath".as_ref(),
                    unity_project.project_dir().as_os_str(),
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null())
                .spawn()?;

            // stdout and stderr
            tokio::spawn(send_lines(child.stdout.take().unwrap(), context.clone()));
            tokio::spawn(send_lines(child.stderr.take().unwrap(), context.clone()));

            // process end
            let status = child.wait().await?;

            return if status.success() {
                Ok(TauriCallUnityForMigrationResult::FinishedSuccessfully)
            } else {
                Ok(TauriCallUnityForMigrationResult::ExistsWithNonZero {
                    status: status.to_string(),
                })
            };

            async fn send_lines(
                stdout: impl tokio::io::AsyncRead + Unpin,
                context: AsyncCommandContext<String>,
            ) {
                let stdout = BufReader::new(stdout);
                let mut stdout = stdout.lines();
                loop {
                    match stdout.next_line().await {
                        Err(e) => {
                            error!("error reading unity output: {e}");
                            break;
                        }
                        Ok(None) => break,
                        Ok(Some(line)) => {
                            log::debug!(target: "vrc_get_gui::unity", "{line}");
                            let line = line.trim().to_string();
                            if let Err(e) = context.emit(line) {
                                error!("error sending stdout: {e}")
                            }
                        }
                    }
                }
            }
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn project_migrate_project_to_vpm(
    app_handle: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
    project_path: String,
) -> Result<(), RustError> {
    let settings = settings.load(&io).await?;
    let packages = packages.load(&settings, &io, &http, app_handle).await?;

    let mut unity_project = load_project(project_path).await?;
    let installer = PackageInstaller::new(io.inner(), Some(http.inner()));

    unity_project
        .migrate_vpm(
            packages.collection(),
            &installer,
            settings.show_prerelease_packages(),
        )
        .await?;

    update_project_last_modified(&io, unity_project.project_dir()).await;

    Ok(())
}

fn is_unity_running(project_path: impl AsRef<Path>) -> bool {
    crate::os::is_locked(&project_path.as_ref().join("Temp/UnityLockFile")).unwrap_or(false)
}

#[tauri::command]
#[specta::specta]
pub async fn project_open_unity(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    io: State<'_, DefaultEnvironmentIo>,
    project_path: String,
    unity_path: String,
) -> Result<bool, RustError> {
    let activity = app.state::<ActivityLogState>();
    let activity_tracker = activity.start_activity(
        Some(&app),
        project_activity(
            operations::PROJECT_OPEN_UNITY,
            "Opening Unity project",
            &project_path,
        )
        .add_details(vec![ActivityDetail::new(
            "unityPath",
            summarize_path(&unity_path),
        )]),
    );
    if is_unity_running(&project_path) {
        // it looks unity is running. returning false
        activity.finish_success(
            Some(&app),
            &activity_tracker,
            "Unity project is already running",
            Vec::new(),
        );
        return Ok(false);
    }

    // Check if the project is on a noexec filesystem (Linux/macOS only)
    // This causes shader compilation failures, resulting in non-stereoscopic rendering
    let project_path_ref = Path::new(&project_path);
    for subdir in &["Assets", "Packages", "Library"] {
        let dir = project_path_ref.join(subdir);
        if crate::os::is_noexec(&dir) {
            let error = localizable_error!("projects:error:noexec filesystem");
            activity.finish_failed(
                Some(&app),
                &activity_tracker,
                "Unity project launch failed",
                Vec::new(),
                &error,
            );
            return Err(error);
        }
    }

    let custom_args = match async {
        let mut connection = VccDatabaseConnection::connect(io.inner()).await?;
        let mut custom_args: Option<Vec<String>> = None;
        if let Some(project) = connection.find_project(project_path.as_ref())? {
            custom_args = project
                .custom_unity_args()
                .map(|x| Vec::from_iter(x.iter().map(ToOwned::to_owned)));
        }
        connection.update_project_last_modified(project_path.as_ref())?;
        connection.save(io.inner()).await?;
        Ok::<_, RustError>(custom_args)
    }
    .await
    {
        Ok(custom_args) => custom_args,
        Err(error) => {
            activity.finish_failed(
                Some(&app),
                &activity_tracker,
                "Unity project launch failed",
                Vec::new(),
                &error,
            );
            return Err(error);
        }
    };

    let unity_args = custom_args.or_else(|| config.get().default_unity_arguments.clone());
    let activity_app = app.clone();
    let start_activity_tracker = activity_tracker.clone();
    tokio::spawn(async move {
        let mut args = vec!["-projectPath".as_ref(), OsStr::new(project_path.as_str())];

        if let Some(unity_args) = &unity_args {
            args.extend(unity_args.iter().map(OsStr::new));
        } else {
            args.extend(DEFAULT_UNITY_ARGUMENTS.iter().map(OsStr::new));
        }

        if let Err(e) = crate::os::start_command("Unity".as_ref(), unity_path.as_ref(), &args).await
        {
            log::error!("Launching Unity: {e}");
            if let Some(activity) = activity_app.try_state::<ActivityLogState>() {
                activity.finish_failed(
                    Some(&activity_app),
                    &start_activity_tracker,
                    "Unity project launch failed",
                    Vec::new(),
                    e,
                );
            }
        } else if let Some(activity) = activity_app.try_state::<ActivityLogState>() {
            activity.finish_success(
                Some(&activity_app),
                &start_activity_tracker,
                "Unity project launch started",
                Vec::new(),
            );
        }
    });

    Ok(true)
}

#[tauri::command]
#[specta::specta]
pub fn project_is_unity_launching(project_path: String) -> bool {
    is_unity_running(project_path)
}

pub(crate) async fn create_project_backup_with_settings(
    config: &GuiConfigState,
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    project_path: String,
    backup_name: Option<String>,
    exclude_vpm: bool,
    progress: impl Fn(TauriCreateBackupProgress) + Clone + Send + Sync + 'static,
) -> Result<PathBuf, RustError> {
    let options = resolve_project_backup_options(config, settings, io, exclude_vpm).await?;
    create_project_backup_with_options(project_path, backup_name, options, progress).await
}

struct ProjectBackupOptions {
    backup_dir: String,
    backup_format: String,
    exclude_vpm: bool,
}

async fn resolve_project_backup_options(
    config: &GuiConfigState,
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
    exclude_vpm: bool,
) -> Result<ProjectBackupOptions, RustError> {
    let backup_format = config.get().backup_format.to_ascii_lowercase();

    let backup_dir = resolve_project_backup_directory(settings, io).await?;

    Ok(ProjectBackupOptions {
        backup_dir,
        backup_format,
        exclude_vpm,
    })
}

async fn resolve_project_backup_directory(
    settings: &SettingsState,
    io: &DefaultEnvironmentIo,
) -> Result<String, RustError> {
    let mut settings = settings.load_mut(io).await?;
    let backup_dir = project_backup_path(&mut settings).to_string();
    settings.maybe_save().await?;
    Ok(backup_dir)
}

async fn create_project_backup_with_options(
    project_path: String,
    backup_name: Option<String>,
    options: ProjectBackupOptions,
    progress: impl Fn(TauriCreateBackupProgress) + Clone + Send + Sync + 'static,
) -> Result<PathBuf, RustError> {
    create_project_backup_archive(
        project_path,
        options.backup_dir,
        backup_name,
        options.backup_format,
        options.exclude_vpm,
        progress,
    )
    .await
}

#[derive(Serialize, specta::Type)]
pub struct TauriProjectBackupCreationInformation {
    backup_directory: String,
    default_backup_name: String,
}

#[derive(Serialize, specta::Type)]
pub enum TauriBackupNameCheckResult {
    InvalidNameForFileName,
    AlreadyExists,
    Ok,
}

#[tauri::command]
#[specta::specta]
pub async fn project_backup_creation_information(
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    project_path: String,
) -> Result<TauriProjectBackupCreationInformation, RustError> {
    let backup_directory = resolve_project_backup_directory(settings.inner(), io.inner()).await?;
    let default_backup_name = default_project_backup_name(&project_path)?;
    Ok(TauriProjectBackupCreationInformation {
        backup_directory,
        default_backup_name,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn project_check_backup_name(
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    backup_name: String,
) -> Result<TauriBackupNameCheckResult, RustError> {
    let backup_name = match normalize_project_backup_name(&backup_name) {
        Ok(backup_name) => backup_name,
        Err(_) => return Ok(TauriBackupNameCheckResult::InvalidNameForFileName),
    };
    let backup_directory = resolve_project_backup_directory(settings.inner(), io.inner()).await?;
    let backup_path = project_backup_archive_path(&backup_directory, &backup_name);
    if tokio::fs::try_exists(backup_path).await? {
        Ok(TauriBackupNameCheckResult::AlreadyExists)
    } else {
        Ok(TauriBackupNameCheckResult::Ok)
    }
}

#[tauri::command]
#[specta::specta]
pub async fn project_create_backup(
    config: State<'_, GuiConfigState>,
    settings: State<'_, SettingsState>,
    project_backup: State<'_, ProjectBackupState>,
    io: State<'_, DefaultEnvironmentIo>,
    window: Window,
    channel: String,
    project_path: String,
    backup_name: Option<String>,
    exclude_vpm: bool,
) -> Result<AsyncCallResult<TauriCreateBackupProgress, ()>, RustError> {
    let app = window.app_handle().clone();
    let activity = app.state::<ActivityLogState>();
    let activity_tracker = activity.start_activity(
        Some(&app),
        project_activity(
            operations::PROJECT_BACKUP,
            "Starting project backup",
            &project_path,
        ),
    );
    let project_backup = project_backup.inner().clone();
    if !project_backup.try_start_uncancellable() {
        let error = localizable_error!("projects:toast:backup already running");
        activity.finish_failed(
            Some(&app),
            &activity_tracker,
            "Project backup failed to start",
            Vec::new(),
            &error,
        );
        return Err(error);
    }

    let project_backup_start = project_backup.clone();
    let project_backup_finish = project_backup.clone();
    let async_activity_tracker = activity_tracker.clone();
    let async_app = app.clone();
    let finish_activity_tracker = activity_tracker.clone();
    let finish_app = app.clone();
    let result = async_command_with_cancel_state(
        channel,
        window,
        async {
            let options = resolve_project_backup_options(
                config.inner(),
                settings.inner(),
                io.inner(),
                exclude_vpm,
            )
            .await?;
            With::<TauriCreateBackupProgress>::continue_async(move |ctx| async move {
                let progress_ctx = ctx.clone();
                let outcome = create_project_backup_with_options(
                    project_path,
                    backup_name,
                    options,
                    move |progress| {
                        let _ = progress_ctx.emit(progress);
                    },
                )
                .await;

                if let Some(activity) = async_app.try_state::<ActivityLogState>() {
                    match &outcome {
                        Ok(backup_path) => {
                            let details = vec![ActivityDetail::new(
                                "backupArchive",
                                summarize_path(backup_path),
                            )];
                            activity.finish_success(
                                Some(&async_app),
                                &async_activity_tracker,
                                "Project backup completed",
                                details,
                            );
                        }
                        Err(error) => {
                            activity.finish_failed(
                                Some(&async_app),
                                &async_activity_tracker,
                                "Project backup failed",
                                Vec::new(),
                                error,
                            );
                        }
                    }
                }

                outcome.map(|_| ())
            })
        },
        move |abort| project_backup_start.start(abort),
        move || {
            project_backup_finish.finish();
            if let Some(activity) = finish_app.try_state::<ActivityLogState>() {
                activity.finish_cancelled(
                    Some(&finish_app),
                    &finish_activity_tracker,
                    "Project backup cancelled",
                    Vec::new(),
                );
            }
        },
    )
    .await;
    if result.is_err() {
        project_backup.finish();
    }
    match &result {
        Ok(_) => {}
        Err(error) => {
            activity.finish_failed(
                Some(&app),
                &activity_tracker,
                "Project backup failed to start",
                Vec::new(),
                error,
            );
        }
    }
    result
}

#[tauri::command]
#[specta::specta]
pub async fn project_get_custom_unity_args(
    io: State<'_, DefaultEnvironmentIo>,
    project_path: String,
) -> Result<Option<Vec<String>>, RustError> {
    let connection = VccDatabaseConnection::connect(io.inner()).await?;
    if let Some(project) = connection.find_project(project_path.as_ref())? {
        Ok(project
            .custom_unity_args()
            .map(|x| x.iter().map(ToOwned::to_owned).collect()))
    } else {
        Ok(None)
    }
}

#[tauri::command]
#[specta::specta]
pub async fn project_set_custom_unity_args(
    app: AppHandle,
    io: State<'_, DefaultEnvironmentIo>,
    project_path: String,
    args: Option<Vec<String>>,
) -> Result<bool, RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = project_activity(
        operations::PROJECT_SET_CUSTOM_UNITY_ARGS,
        "Updating custom Unity arguments",
        &project_path,
    )
    .add_detail(ActivityDetail::new(
        "args",
        args.as_ref()
            .map(|args| args.len().to_string())
            .unwrap_or_else(|| "default".to_string()),
    ));
    activity
        .track_result(
            Some(&app),
            input,
            "Custom Unity arguments updated",
            Vec::new(),
            async move {
                let mut connection = VccDatabaseConnection::connect(io.inner()).await?;
                if let Some(mut project) = connection.find_project(project_path.as_ref())? {
                    if let Some(args) = args {
                        project.set_custom_unity_args(args);
                    } else {
                        project.clear_custom_unity_args();
                    }
                    connection.update_project(&project);
                    connection.save(io.inner()).await?;
                    Ok(true)
                } else {
                    Err(RustError::unrecoverable_str("project not found"))
                }
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn project_get_unity_path(
    io: State<'_, DefaultEnvironmentIo>,
    project_path: String,
) -> Result<Option<String>, RustError> {
    let connection = VccDatabaseConnection::connect(io.inner()).await?;
    if let Some(project) = connection.find_project(project_path.as_ref())? {
        Ok(project.unity_path().map(ToOwned::to_owned))
    } else {
        Ok(None)
    }
}

#[tauri::command]
#[specta::specta]
pub async fn project_set_unity_path(
    app: AppHandle,
    io: State<'_, DefaultEnvironmentIo>,
    project_path: String,
    unity_path: Option<String>,
) -> Result<bool, RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = project_activity(
        operations::PROJECT_SET_UNITY_PATH,
        "Updating Unity path",
        &project_path,
    )
    .add_detail(ActivityDetail::new(
        "unityPath",
        unity_path
            .as_deref()
            .map(summarize_path)
            .unwrap_or_else(|| "auto".to_string()),
    ));
    activity
        .track_result(
            Some(&app),
            input,
            "Unity path updated",
            Vec::new(),
            async move {
                let mut connection = VccDatabaseConnection::connect(io.inner()).await?;
                if let Some(mut project) = connection.find_project(project_path.as_ref())? {
                    if let Some(unity_path) = unity_path {
                        project.set_unity_path(unity_path);
                    } else {
                        project.clear_unity_path();
                    }
                    connection.update_project(&project);
                    connection.save(io.inner()).await?;
                    Ok(true)
                } else {
                    Err(RustError::unrecoverable_str("project not found"))
                }
            },
        )
        .await
}
