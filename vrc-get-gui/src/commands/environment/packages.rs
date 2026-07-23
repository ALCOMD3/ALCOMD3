use crate::activity_log::{
    ActivityDetail, ActivityImportance, ActivityInput, ActivityKind, ActivityLogState,
    ActivitySource, operations, summarize_path, summarize_url, summarize_url_host,
    target_from_path,
};
use crate::backend::packages::{
    latest_package_infos_by_source, package_is_available_for_display,
    repository_id as cached_repository_id,
};
use crate::commands::async_command::{AsyncCallResult, With, async_command};
use crate::commands::prelude::*;
use futures::future::try_join_all;
use indexmap::IndexMap;
use itertools::Itertools;
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{AppHandle, Manager, State, Window};
use tauri_plugin_dialog::DialogExt;
use tokio::fs::write;
use url::Url;
use vrc_get_vpm::environment::{
    AddUserPackageResult, CURATED_REPOSITORY_ID, CURATED_URL_STR, OFFICIAL_REPOSITORY_ID,
    OFFICIAL_URL_STR, Settings, UserPackageCollection, add_remote_repo, clear_package_cache,
};
use vrc_get_vpm::io::{DefaultEnvironmentIo, IoTrait};
use vrc_get_vpm::repositories_file::RepositoriesFile;
use vrc_get_vpm::repository::RemoteRepository;
use vrc_get_vpm::{HttpClient, PackageInfo, UserRepoSetting, VersionSelector};

#[tauri::command]
#[specta::specta]
pub async fn environment_refetch_packages(
    app: AppHandle,
    packages: State<'_, PackagesState>,
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    activity
        .track_result(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Maintenance,
                ActivityImportance::Primary,
                operations::PACKAGES_REFRESH_CACHE,
                "Refreshing package cache",
            ),
            "Package cache refreshed",
            Vec::new(),
            async move {
                let settings = settings.load(io.inner()).await?;
                packages
                    .load_force(&settings, io.inner(), http.inner())
                    .await?;

                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_packages(
    app_handle: AppHandle,
    packages: State<'_, PackagesState>,
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
) -> Result<Vec<TauriPackage>, RustError> {
    let settings = settings.load(io.inner()).await?;
    let packages = packages
        .load(&settings, io.inner(), http.inner(), app_handle)
        .await?;

    Ok(packages
        .packages()
        .map(|value| TauriPackage::new(value))
        .collect::<Vec<_>>())
}

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriRepositoryPackageList {
    repository_id: String,
    packages: Vec<TauriBasePackageInfo>,
}

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriDefaultRepository {
    id: String,
    url: String,
    kind: String,
}

#[tauri::command]
#[specta::specta]
pub async fn environment_default_repositories() -> Result<Vec<TauriDefaultRepository>, RustError> {
    Ok(vec![
        TauriDefaultRepository {
            id: OFFICIAL_REPOSITORY_ID.to_string(),
            url: OFFICIAL_URL_STR.to_string(),
            kind: "officialDefault".to_string(),
        },
        TauriDefaultRepository {
            id: CURATED_REPOSITORY_ID.to_string(),
            url: CURATED_URL_STR.to_string(),
            kind: "curatedDefault".to_string(),
        },
    ])
}

#[tauri::command]
#[specta::specta]
pub async fn environment_repository_package_lists(
    app_handle: AppHandle,
    packages: State<'_, PackagesState>,
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
) -> Result<Vec<TauriRepositoryPackageList>, RustError> {
    let settings = settings.load(io.inner()).await?;
    let show_prerelease_packages = settings.show_prerelease_packages();
    let packages = packages
        .load(&settings, io.inner(), http.inner(), app_handle)
        .await?;

    Ok(repository_package_lists(
        packages.packages(),
        show_prerelease_packages,
    ))
}

fn repository_package_lists<'package, 'env>(
    packages: impl IntoIterator<Item = &'package PackageInfo<'env>>,
    show_prerelease_packages: bool,
) -> Vec<TauriRepositoryPackageList>
where
    'env: 'package,
{
    let latest_packages = latest_package_infos_by_source(
        packages
            .into_iter()
            .filter(|package| package.repo().is_some())
            .filter(|package| package_is_available_for_display(package, show_prerelease_packages)),
    );

    let mut packages_by_repository = BTreeMap::<String, Vec<TauriBasePackageInfo>>::new();
    for package in latest_packages {
        let Some(repository_id) = package.repo().and_then(cached_repository_id) else {
            continue;
        };
        packages_by_repository
            .entry(repository_id.to_string())
            .or_default()
            .push(TauriBasePackageInfo::new(package.package_json()));
    }

    packages_by_repository
        .into_iter()
        .map(|(repository_id, mut packages)| {
            sort_base_package_infos(&mut packages);
            TauriRepositoryPackageList {
                repository_id,
                packages,
            }
        })
        .collect()
}

fn sort_base_package_infos(packages: &mut [TauriBasePackageInfo]) {
    packages.sort_by(|a, b| {
        let a_name = a.display_name.as_deref().unwrap_or(&a.name);
        let b_name = b.display_name.as_deref().unwrap_or(&b.name);
        a_name
            .cmp(b_name)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.version.major.cmp(&b.version.major))
            .then_with(|| a.version.minor.cmp(&b.version.minor))
            .then_with(|| a.version.patch.cmp(&b.version.patch))
    });
}

#[derive(Serialize, specta::Type)]
struct TauriUserRepository {
    index: usize,
    id: String,
    url: Option<String>,
    display_name: String,
}

#[derive(Serialize, specta::Type)]
pub struct TauriRepositoriesInfo {
    user_repositories: Vec<TauriUserRepository>,
    hidden_user_repositories: Vec<String>,
    hide_local_user_packages: bool,
    show_prerelease_packages: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn environment_repositories_info(
    settings: State<'_, SettingsState>,
    config: State<'_, GuiConfigState>,
    io: State<'_, DefaultEnvironmentIo>,
) -> Result<TauriRepositoriesInfo, RustError> {
    let config = config.get();
    let hidden_user_repositories = config.gui_hidden_repositories.iter().cloned().collect();
    let hide_local_user_packages = config.hide_local_user_packages;
    drop(config);

    let settings = settings.load(io.inner()).await?;
    let user_repositories = settings
        .get_user_repos()
        .iter()
        .enumerate()
        .filter_map(|(index, x)| {
            let id = x.id().or(x.url().map(Url::as_str))?;
            Some(TauriUserRepository {
                index,
                id: id.to_string(),
                url: x.url().map(|x| x.to_string()),
                display_name: x.name().unwrap_or(id).to_string(),
            })
        })
        .collect();
    let show_prerelease_packages = settings.show_prerelease_packages();

    Ok(TauriRepositoriesInfo {
        user_repositories,
        hidden_user_repositories,
        hide_local_user_packages,
        show_prerelease_packages,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn environment_hide_repository(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    repository: String,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let target = repository_activity_target(&repository);
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::REPOSITORY_HIDE,
        "Hiding repository",
    )
    .target(target);
    activity
        .track_result(
            Some(&app),
            input,
            "Repository hidden",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.gui_hidden_repositories.insert(repository);
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_show_repository(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    repository: String,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let target = repository_activity_target(&repository);
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::REPOSITORY_SHOW,
        "Showing repository",
    )
    .target(target);
    activity
        .track_result(
            Some(&app),
            input,
            "Repository shown",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.gui_hidden_repositories.shift_remove(&repository);
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_hide_local_user_packages(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    value: bool,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::SETTINGS_SET,
        "Updating local user packages visibility",
    )
    .target("hideLocalUserPackages")
    .details(vec![ActivityDetail::new("value", value.to_string())]);
    activity
        .track_result(
            Some(&app),
            input,
            "Local user packages visibility updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.hide_local_user_packages = value;
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[derive(Serialize, specta::Type, Clone)]
pub struct TauriRemoteRepositoryInfo {
    display_name: String,
    id: String,
    url: String,
    packages: Vec<TauriBasePackageInfo>,
}

#[derive(Serialize, specta::Type, Clone)]
#[serde(tag = "type")]
pub enum TauriDownloadRepository {
    BadUrl,
    Duplicated {
        reason: TauriDuplicatedReason,
        // Default repository ids use vrc_get_vpm::environment constants.
        duplicated_name: String,
    },
    DownloadError {
        message: String,
    },
    Success {
        value: TauriRemoteRepositoryInfo,
    },
}

#[derive(Serialize, specta::Type, Clone)]
pub enum TauriDuplicatedReason {
    URLDuplicated,
    IDDuplicated,
}

#[tauri::command]
#[specta::specta]
pub async fn environment_download_repository(
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
    url: String,
    headers: IndexMap<Box<str>, Box<str>>,
) -> Result<TauriDownloadRepository, RustError> {
    let url: Url = match url.parse() {
        Err(_) => {
            return Ok(TauriDownloadRepository::BadUrl);
        }
        Ok(url) => url,
    };

    {
        let settings = settings.load(io.inner()).await?;
        let user_repo_urls = user_repo_urls(&settings);
        let user_repo_ids = user_repo_ids(&settings);

        download_one_repository(
            http.inner(),
            &url,
            &headers,
            &user_repo_urls,
            &user_repo_ids,
        )
        .await
    }
}

fn user_repo_urls(settings: &Settings) -> HashMap<String, String> {
    let mut user_repo_urls = settings
        .get_user_repos()
        .iter()
        .flat_map(|x| {
            x.url().map(|u| {
                (
                    u.to_string(),
                    x.name().or(x.id()).unwrap_or(u.as_str()).to_string(),
                )
            })
        })
        .collect::<HashMap<String, String>>();

    if !settings.ignore_curated_repository() {
        // should we check more urls?
        user_repo_urls.insert(
            CURATED_URL_STR.to_owned(),
            CURATED_REPOSITORY_ID.to_string(),
        );
    }

    if !settings.ignore_official_repository() {
        user_repo_urls.insert(
            OFFICIAL_URL_STR.to_owned(),
            OFFICIAL_REPOSITORY_ID.to_string(),
        );
    }

    user_repo_urls
}

fn user_repo_ids(settings: &Settings) -> HashMap<String, String> {
    let mut user_repo_ids = settings
        .get_user_repos()
        .iter()
        .flat_map(|x| {
            x.id()
                .map(|i| (i.to_string(), x.name().unwrap_or(i).to_string()))
        })
        .collect::<HashMap<String, String>>();

    if !settings.ignore_curated_repository() {
        user_repo_ids.insert(
            CURATED_REPOSITORY_ID.to_owned(),
            CURATED_REPOSITORY_ID.to_string(),
        );
    }

    if !settings.ignore_official_repository() {
        user_repo_ids.insert(
            OFFICIAL_REPOSITORY_ID.to_owned(),
            OFFICIAL_REPOSITORY_ID.to_string(),
        );
    }

    user_repo_ids
}

async fn download_one_repository(
    client: &impl HttpClient,
    repository_url: &Url,
    headers: &IndexMap<Box<str>, Box<str>>,
    user_repo_urls: &HashMap<String, String>,
    user_repo_ids: &HashMap<String, String>,
) -> Result<TauriDownloadRepository, RustError> {
    if let Some(name) = user_repo_urls.get(repository_url.as_str()) {
        return Ok(TauriDownloadRepository::Duplicated {
            reason: TauriDuplicatedReason::URLDuplicated,
            duplicated_name: name.to_string(),
        });
    }

    let repo = match RemoteRepository::download(client, repository_url, headers).await {
        Ok((repo, _)) => repo,
        Err(e) => {
            return Ok(TauriDownloadRepository::DownloadError {
                message: e.to_string(),
            });
        }
    };

    let url = repo.url().unwrap_or(repository_url).as_str();
    let id = repo.id().unwrap_or(url);

    if let Some(name) = user_repo_ids.get(id) {
        return Ok(TauriDownloadRepository::Duplicated {
            reason: TauriDuplicatedReason::IDDuplicated,
            duplicated_name: name.to_string(),
        });
    }

    Ok(TauriDownloadRepository::Success {
        value: TauriRemoteRepositoryInfo {
            id: id.to_string(),
            url: url.to_string(),
            display_name: repo.name().unwrap_or(id).to_string(),
            packages: repo
                .get_packages()
                .filter_map(|x| x.get_latest(VersionSelector::latest_for(None, true)))
                .filter(|x| !x.is_yanked())
                .map(TauriBasePackageInfo::new)
                .collect(),
        },
    })
}

#[derive(Serialize, specta::Type)]
pub enum TauriAddRepositoryResult {
    BadUrl,
    Success,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AddedRepositoryInfo {
    pub(crate) index: usize,
    pub(crate) id: Option<String>,
    pub(crate) url: String,
    pub(crate) display_name: Option<String>,
}

pub(crate) async fn add_repository_by_url(
    settings: &SettingsState,
    packages: &PackagesState,
    io: &DefaultEnvironmentIo,
    http: &reqwest::Client,
    url: Url,
    headers: IndexMap<Box<str>, Box<str>>,
) -> Result<AddedRepositoryInfo, RustError> {
    let repository_url = url.to_string();
    let mut settings = settings.load_mut(io).await?;
    let previous_repo_count = settings.get_user_repos().len();
    add_remote_repo(&mut settings, url, None, headers, io, http).await?;

    let user_repos = settings.get_user_repos();
    let (index, repository) = user_repos
        .get(previous_repo_count)
        .map(|repository| (previous_repo_count, repository))
        .or_else(|| {
            user_repos.iter().enumerate().find(|(_, repository)| {
                repository
                    .url()
                    .is_some_and(|url| url.as_str() == repository_url)
            })
        })
        .ok_or_else(|| RustError::unrecoverable_str("added repository was not found"))?;
    let id = repository
        .id()
        .or(repository.url().map(Url::as_str))
        .map(ToString::to_string);
    let display_name = repository
        .name()
        .map(ToString::to_string)
        .or_else(|| id.clone());
    let url = repository
        .url()
        .map(ToString::to_string)
        .unwrap_or(repository_url);
    let repository = AddedRepositoryInfo {
        index,
        id,
        url,
        display_name,
    };

    settings.save().await?;

    // force update repository
    packages.clear_cache();

    Ok(repository)
}

#[tauri::command]
#[specta::specta]
pub async fn environment_add_repository(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    io: State<'_, DefaultEnvironmentIo>,
    http: State<'_, reqwest::Client>,
    url: String,
    headers: IndexMap<Box<str>, Box<str>>,
) -> Result<TauriAddRepositoryResult, RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::REPOSITORY_ADD,
        "Adding repository",
    )
    .target(summarize_url_host(&url))
    .details(vec![ActivityDetail::new("url", summarize_url(&url))]);
    let url: Url = match url.parse() {
        Err(_) => {
            activity.record_failed(Some(&app), input, "Bad repository URL");
            return Ok(TauriAddRepositoryResult::BadUrl);
        }
        Ok(url) => url,
    };

    activity
        .track_result(
            Some(&app),
            input,
            "Repository added",
            Vec::new(),
            async move {
                add_repository_by_url(
                    settings.inner(),
                    packages.inner(),
                    io.inner(),
                    http.inner(),
                    url,
                    headers,
                )
                .await?;
                Ok(TauriAddRepositoryResult::Success)
            },
        )
        .await
}

// Verifies that the repo at `index` in the freshly-loaded settings still has
// the `expected_id` the frontend last saw. Guards against silent corruption
// from external writes to settings.json between fetch and mutation.
fn verify_repo_at_index(
    repos: &[UserRepoSetting],
    index: usize,
    expected_id: &str,
) -> Result<(), RustError> {
    let Some(repo) = repos.get(index) else {
        return Err(RustError::unrecoverable_str(format!(
            "Repository index {index} out of range (expected id {expected_id}). \
             settings.json was likely modified externally; please refresh."
        )));
    };
    let actual = repo.id().or(repo.url().map(Url::as_str));
    if actual != Some(expected_id) {
        return Err(RustError::unrecoverable_str(format!(
            "Repository at index {index} changed (expected id {expected_id}, found {actual:?}). \
             settings.json was likely modified externally; please refresh."
        )));
    }
    Ok(())
}

fn repository_activity_target(repository_id: &str) -> String {
    Url::parse(repository_id)
        .ok()
        .map(|_| summarize_url(repository_id))
        .unwrap_or_else(|| repository_id.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn environment_remove_repository(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    io: State<'_, DefaultEnvironmentIo>,
    index: usize,
    expected_id: String,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let target = repository_activity_target(&expected_id);
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::REPOSITORY_REMOVE,
        "Removing repository",
    )
    .target(target)
    .details(vec![ActivityDetail::new("index", index.to_string())]);
    activity
        .track_result(
            Some(&app),
            input,
            "Repository removed",
            Vec::new(),
            async move {
                let mut settings = settings.load_mut(io.inner()).await?;

                verify_repo_at_index(settings.get_user_repos(), index, &expected_id)?;

                let removed = settings.remove_repo_at_index(index);

                if let Some(repo) = &removed {
                    io.remove_file(repo.local_path()).await.ok();
                }

                settings.save().await?;

                packages.clear_cache();

                Ok(())
            },
        )
        .await
}

#[derive(Serialize, specta::Type)]
#[serde(tag = "type")]
pub enum TauriImportRepositoryPickResult {
    NoFilePicked,
    ParsedRepositories {
        repositories: Vec<TauriRepositoryDescriptor>,
        unparsable_lines: Vec<String>,
    },
}

// workaround bug in specta::Type derive macro
type Headers = IndexMap<Box<str>, Box<str>>;

#[derive(Serialize, Deserialize, specta::Type, Clone)]
pub struct TauriRepositoryDescriptor {
    pub url: Url,
    pub headers: Headers,
}

#[derive(Deserialize, specta::Type)]
pub struct TauriUserRepositoryRef {
    pub index: usize,
    pub id: String,
}

#[tauri::command]
#[specta::specta]
pub async fn environment_reorder_repositories(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    io: State<'_, DefaultEnvironmentIo>,
    repos: Vec<TauriUserRepositoryRef>,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let repo_count = repos.len();
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::REPOSITORY_REORDER,
        "Reordering repositories",
    )
    .details(vec![ActivityDetail::new(
        "repositories",
        repo_count.to_string(),
    )]);
    activity
        .track_result(
            Some(&app),
            input,
            "Repositories reordered",
            Vec::new(),
            async move {
                let mut settings = settings.load_mut(io.inner()).await?;
                log::debug!("reorder user repositories: {} entries", repos.len());

                let user_repos = settings.get_user_repos();
                for r in &repos {
                    verify_repo_at_index(user_repos, r.index, &r.id)?;
                }

                let indices: Vec<usize> = repos.into_iter().map(|r| r.index).collect();
                settings.reorder_user_repos_by_indices(&indices);
                settings.save().await?;
                packages.clear_cache();
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_import_repository_pick(
    window: Window,
) -> Result<TauriImportRepositoryPickResult, RustError> {
    let builder = window.dialog().file().set_parent(&window);

    let Some(repositories_path) = builder
        .blocking_pick_file()
        .map(|x| x.into_path_buf())
        .transpose()?
    else {
        return Ok(TauriImportRepositoryPickResult::NoFilePicked);
    };

    let repositories_file = tokio::fs::read_to_string(repositories_path).await?;

    let result = RepositoriesFile::parse(&repositories_file);

    Ok(TauriImportRepositoryPickResult::ParsedRepositories {
        repositories: result
            .parsed()
            .repositories()
            .iter()
            .map(|x| TauriRepositoryDescriptor {
                url: x.url().clone(),
                headers: x.headers().clone(),
            })
            .collect(),
        unparsable_lines: result.unparseable_lines().to_vec(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn environment_import_download_repositories(
    window: Window,
    channel: String,
    repositories: Vec<TauriRepositoryDescriptor>,
) -> Result<
    AsyncCallResult<usize, Vec<(TauriRepositoryDescriptor, TauriDownloadRepository)>>,
    RustError,
> {
    async_command(channel, window.clone(), async move {
        With::<usize>::continue_async(|ctx| async move {
            let settings = window.state::<SettingsState>();
            let io = window.state::<DefaultEnvironmentIo>();
            let settings = settings.load(io.inner()).await?;
            {
                let user_repo_urls = user_repo_urls(&settings);
                let mut user_repo_ids = user_repo_ids(&settings);
                drop(settings);

                info!("downloading {} repositories", repositories.len());

                let counter = AtomicUsize::new(0);

                let counter_ref = &counter;
                let user_repo_urls_ref = &user_repo_urls;
                let user_repo_ids_ref = &user_repo_ids;

                let http = window.state::<reqwest::Client>();
                let mut results = try_join_all(repositories.into_iter().map(|adding_repo| {
                    let ctx = ctx.clone();
                    let http = http.clone();
                    async move {
                        let downloaded = download_one_repository(
                            http.inner(),
                            &adding_repo.url,
                            &adding_repo.headers,
                            user_repo_urls_ref,
                            user_repo_ids_ref,
                        )
                        .await?;

                        info!("downloaded repository: {:?}", adding_repo.url);

                        let count = counter_ref.fetch_add(1, Ordering::Relaxed) + 1;
                        if let Err(e) = ctx.emit(count) {
                            log::error!("failed to emit repository download progress: {e}");
                        }

                        Ok::<_, RustError>((adding_repo, downloaded))
                    }
                }))
                .await?;

                for (_, downloaded) in results.as_mut_slice() {
                    if let TauriDownloadRepository::Success { value } = &downloaded {
                        if let Some(name) = user_repo_ids.get(&value.id) {
                            info!("duplicated repository in list: {}", value.url);
                            *downloaded = TauriDownloadRepository::Duplicated {
                                reason: TauriDuplicatedReason::IDDuplicated,
                                duplicated_name: name.to_string(),
                            };
                        } else {
                            user_repo_ids.insert(value.id.to_string(), value.display_name.clone());
                        }
                    }
                }

                Ok(results)
            }
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_import_add_repositories(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    http: State<'_, reqwest::Client>,
    io: State<'_, DefaultEnvironmentIo>,
    repositories: Vec<TauriRepositoryDescriptor>,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let repo_count = repositories.len();
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::REPOSITORY_IMPORT,
        "Importing repositories",
    )
    .details(vec![ActivityDetail::new(
        "repositories",
        repo_count.to_string(),
    )]);
    activity
        .track_result(
            Some(&app),
            input,
            "Repositories imported",
            Vec::new(),
            async move {
                let mut settings = settings.load_mut(io.inner()).await?;
                for adding_repo in repositories {
                    add_remote_repo(
                        &mut settings,
                        adding_repo.url,
                        None,
                        adding_repo.headers,
                        io.inner(),
                        http.inner(),
                    )
                    .await?;
                }
                settings.save().await?;

                // force update repository
                packages.clear_cache();

                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_export_repositories(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
    window: Window,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let Some(path) = window
        .dialog()
        .file()
        .set_parent(&window)
        .add_filter("Text", &["txt"])
        .set_file_name("repositories.txt")
        .blocking_save_file()
        .map(|x| x.into_path_buf())
        .transpose()?
    else {
        activity.record_info(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Open,
                ActivityImportance::Secondary,
                operations::REPOSITORY_EXPORT,
                "Repository export cancelled",
            ),
        );
        return Ok(());
    };

    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::REPOSITORY_EXPORT,
        "Exporting repositories",
    )
    .target(target_from_path(&path))
    .details(vec![ActivityDetail::new("path", summarize_path(&path))]);
    activity
        .track_result(
            Some(&app),
            input,
            "Repositories exported",
            Vec::new(),
            async move {
                let repositories = settings.load(io.inner()).await?.export_repositories();

                write(path, repositories).await?;

                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_clear_package_cache(
    app: AppHandle,
    packages: State<'_, PackagesState>,
    io: State<'_, DefaultEnvironmentIo>,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    activity
        .track_result(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Maintenance,
                ActivityImportance::Primary,
                operations::REPOSITORY_CLEAR_CACHE,
                "Clearing package cache",
            ),
            "Package cache cleared",
            Vec::new(),
            async move {
                clear_package_cache(io.inner()).await?;
                packages.clear_cache();

                Ok(())
            },
        )
        .await
}

#[derive(Serialize, specta::Type)]
pub struct TauriUserPackage {
    path: String,
    package: TauriBasePackageInfo,
}

#[tauri::command]
#[specta::specta]
pub async fn environment_get_user_packages(
    settings: State<'_, SettingsState>,
    io: State<'_, DefaultEnvironmentIo>,
) -> Result<Vec<TauriUserPackage>, RustError> {
    let settings = settings.load(io.inner()).await?;
    let packages = UserPackageCollection::load(&settings, io.inner()).await;

    Ok(packages
        .packages()
        .filter_map(|(path, json)| {
            let path = path.as_os_str().to_str()?;
            Some(TauriUserPackage {
                path: path.into(),
                package: TauriBasePackageInfo::new(json),
            })
        })
        .collect())
}

#[derive(Serialize, specta::Type)]
pub enum TauriAddUserPackageWithPickerResult {
    NoFolderSelected,
    InvalidSelection,
    AlreadyAdded,
    Successful,
}

#[tauri::command]
#[specta::specta]
pub async fn environment_add_user_package_with_picker(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    io: State<'_, DefaultEnvironmentIo>,
    window: Window,
) -> Result<TauriAddUserPackageWithPickerResult, RustError> {
    let activity = app.state::<ActivityLogState>();
    let Some(package_paths) = window
        .dialog()
        .file()
        .set_parent(&window)
        .blocking_pick_folders()
    else {
        activity.record_info(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Open,
                ActivityImportance::Secondary,
                operations::USER_PACKAGE_ADD,
                "User package selection cancelled",
            ),
        );
        return Ok(TauriAddUserPackageWithPickerResult::NoFolderSelected);
    };

    let Ok(package_paths) = package_paths
        .into_iter()
        .map(|x| x.into_path_buf().map_err(|_| ()))
        .map_ok(|x| x.into_os_string().into_string().map_err(|_| ()))
        .flatten_ok()
        .collect::<Result<Vec<_>, ()>>()
    else {
        activity.record_failed(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityImportance::Primary,
                operations::USER_PACKAGE_ADD,
                "Adding user packages",
            ),
            "Invalid user package selection",
        );
        return Ok(TauriAddUserPackageWithPickerResult::InvalidSelection);
    };

    let package_count = package_paths.len();
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::USER_PACKAGE_ADD,
        "Adding user packages",
    )
    .details(vec![ActivityDetail::new(
        "packages",
        package_count.to_string(),
    )]);
    let tracker = activity.start_activity(Some(&app), input);
    let result = async move {
        {
            let mut settings = settings.load_mut(io.inner()).await?;
            for package_path in package_paths {
                match settings
                    .add_user_package(package_path.as_ref(), io.inner())
                    .await
                {
                    AddUserPackageResult::Success => {}
                    AddUserPackageResult::NonAbsolute => unreachable!("absolute path"),
                    AddUserPackageResult::BadPackage => {
                        return Ok(TauriAddUserPackageWithPickerResult::InvalidSelection);
                    }
                    AddUserPackageResult::AlreadyAdded => {
                        return Ok(TauriAddUserPackageWithPickerResult::AlreadyAdded);
                    }
                }
            }

            settings.save().await?;
        }

        packages.clear_cache();

        Ok(TauriAddUserPackageWithPickerResult::Successful)
    }
    .await;
    match &result {
        Ok(TauriAddUserPackageWithPickerResult::Successful) => {
            activity.finish_success(Some(&app), &tracker, "User packages added", Vec::new());
        }
        Ok(TauriAddUserPackageWithPickerResult::InvalidSelection) => {
            activity.finish_failed(
                Some(&app),
                &tracker,
                "User package selection was invalid",
                Vec::new(),
                "selected folder did not contain a valid user package",
            );
        }
        Ok(TauriAddUserPackageWithPickerResult::AlreadyAdded) => {
            activity.finish_failed(
                Some(&app),
                &tracker,
                "User package was already added",
                Vec::new(),
                "selected user package was already added",
            );
        }
        Ok(TauriAddUserPackageWithPickerResult::NoFolderSelected) => {
            activity.finish_cancelled(
                Some(&app),
                &tracker,
                "User package selection cancelled",
                Vec::new(),
            );
        }
        Err(error) => {
            activity.finish_failed(
                Some(&app),
                &tracker,
                "User package add failed",
                Vec::new(),
                error,
            );
        }
    }
    result
}

#[tauri::command]
#[specta::specta]
pub async fn environment_remove_user_packages(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    packages: State<'_, PackagesState>,
    io: State<'_, DefaultEnvironmentIo>,
    path: String,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::USER_PACKAGE_REMOVE,
        "Removing user package",
    )
    .target(target_from_path(&path))
    .details(vec![ActivityDetail::new("path", summarize_path(&path))]);
    activity
        .track_result(
            Some(&app),
            input,
            "User package removed",
            Vec::new(),
            async move {
                {
                    let mut settings = settings.load_mut(io.inner()).await?;
                    settings.remove_user_package(Path::new(&path));
                    settings.save().await?;
                }

                packages.clear_cache();

                Ok(())
            },
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use vrc_get_vpm::PackageManifest;
    use vrc_get_vpm::repository::LocalCachedRepository;

    #[test]
    fn repository_package_lists_keep_latest_visible_version_per_package() {
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
        let repository = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));
        let older = PackageInfo::remote(&older, &repository);
        let newer = PackageInfo::remote(&newer, &repository);

        let lists = repository_package_lists([&older, &newer], false);

        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].repository_id, "com.example.repo");
        assert_eq!(lists[0].packages.len(), 1);
        assert_eq!(lists[0].packages[0].name, "com.example.package");
        assert_eq!(lists[0].packages[0].version.major, 1);
        assert_eq!(lists[0].packages[0].version.minor, 1);
        assert_eq!(lists[0].packages[0].version.patch, 0);
    }

    #[test]
    fn repository_activity_target_sanitizes_url_ids() {
        assert_eq!(
            repository_activity_target("https://user:pass@example.com/index.json?token=secret"),
            "https://example.com/index.json"
        );
        assert_eq!(
            repository_activity_target("com.example.repo"),
            "com.example.repo"
        );
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
