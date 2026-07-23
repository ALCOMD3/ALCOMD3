mod repo_holder;
mod repo_source;
mod uesr_package_collection;
mod vpm_settings;
mod vrc_get_settings;

#[cfg(feature = "vrc-get-litedb")]
mod litedb;
mod package_collection;
mod package_installer;
#[cfg(feature = "experimental-project-management")]
mod project_management;
mod settings;
#[cfg(feature = "experimental-unity-management")]
mod unity_management;

use crate::io;
use crate::repository::RemoteRepository;
use crate::repository::local::LocalCachedRepository;
use crate::traits::HttpClient;
use crate::utils::to_vec_pretty_os_eol;
use futures::prelude::*;
use indexmap::IndexMap;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt;
use std::path::Path;
use url::Url;

use crate::io::{DefaultEnvironmentIo, DirEntry, IoTrait};
#[cfg(feature = "experimental-project-management")]
pub use project_management::*;
pub(crate) use repo_holder::RepoHolder;
pub(crate) use repo_source::RepoSource;
#[cfg(feature = "experimental-unity-management")]
pub use unity_management::*;

pub use crate::{PackageInstallProgress, PackageInstallProgressKind};
#[cfg(feature = "vrc-get-litedb")]
pub use litedb::VccDatabaseConnection;
pub use package_collection::PackageCollection;
pub use package_installer::PackageInstaller;
pub use settings::Settings;
pub use uesr_package_collection::UserPackageCollection;

pub const OFFICIAL_REPOSITORY_ID: &str = "com.vrchat.repos.official";
pub const CURATED_REPOSITORY_ID: &str = "com.vrchat.repos.curated";
pub const OFFICIAL_URL_STR: &str = "https://packages.vrchat.com/official?download";
pub const CURATED_URL_STR: &str = "https://packages.vrchat.com/curated?download";
pub const REPO_CACHE_FOLDER: &str = "Repos";
pub const PACKAGE_CACHE_FOLDER: &str = "PackageCache";
pub const PACKAGE_CACHE_FILE_PREFIX: &str = "alcomd3-";
pub const LEGACY_PACKAGE_CACHE_FILE_PREFIX: &str = "vrc-get-";
pub const VRC_GET_SETTINGS_PATH: &str = "config/repository-settings.json";
pub const LEGACY_VRC_GET_SETTINGS_PATH: &str = "vrc-get/settings.json";
pub const VPM_SETTINGS_BACKUP_PATH: &str = "state/vcc-settings-backup.json";
const LOCAL_OFFICIAL_FILE: &str = "vrc-official.json";
const LOCAL_CURATED_FILE: &str = "vrc-curated.json";
const VCC_PACKAGE_CACHE_FILE: &str = "package-cache.json";

pub async fn add_remote_repo(
    settings: &mut Settings,
    url: Url,
    name: Option<&str>,
    headers: IndexMap<Box<str>, Box<str>>,
    io: &DefaultEnvironmentIo,
    http: &impl HttpClient,
) -> Result<(), AddRepositoryErr> {
    let (remote_repo, etag) = RemoteRepository::download(http, &url, &headers).await?;

    if !settings.can_add_remote_repo(&url, &remote_repo) {
        return Err(AddRepositoryErr::AlreadyAdded);
    }

    let mut local_cache = LocalCachedRepository::new(remote_repo, headers.clone());
    if let Some(etag) = etag {
        local_cache
            .vrc_get
            .get_or_insert_with(Default::default)
            .etag = etag;
    }

    io.create_dir_all(REPO_CACHE_FOLDER.as_ref()).await?;
    let file_name = write_new_repo(&local_cache, io).await?;
    let cache_path = Path::new(REPO_CACHE_FOLDER).join(&file_name);
    let repo_path = io.resolve(&cache_path);

    if !settings.add_remote_repo(&url, name, headers, local_cache.repo(), &repo_path) {
        io.remove_file(&cache_path).await.ok();
        return Err(AddRepositoryErr::AlreadyAdded);
    }

    Ok(())
}

pub async fn cleanup_repos_folder(
    settings: &Settings,
    io: &DefaultEnvironmentIo,
) -> io::Result<()> {
    let mut uesr_repo_file_names = HashSet::<OsString>::from_iter([
        OsString::from(LOCAL_OFFICIAL_FILE),
        OsString::from(LOCAL_CURATED_FILE),
        // package cache management file used by VCC but not used by vrc-get
        OsString::from(VCC_PACKAGE_CACHE_FILE),
    ]);
    let repos_base = io.resolve(REPO_CACHE_FOLDER.as_ref());

    for x in settings.get_user_repos() {
        if let Ok(relative) = x.local_path().strip_prefix(&repos_base)
            && let Some(file_name) = relative.file_name()
            && relative
                .parent()
                .map(|x| x.as_os_str().is_empty())
                .unwrap_or(true)
        {
            // the file must be in direct child of
            uesr_repo_file_names.insert(file_name.to_owned());
        }
    }

    let mut entry = io.read_dir(REPO_CACHE_FOLDER.as_ref()).await?;
    while let Some(entry) = entry.try_next().await? {
        let file_name: OsString = entry.file_name();
        if file_name.as_encoded_bytes().ends_with(b".json")
            && !uesr_repo_file_names.contains(&file_name)
            && entry.metadata().await.map(|x| x.is_file()).unwrap_or(false)
        {
            let path = Path::new(REPO_CACHE_FOLDER).join(Path::new(&file_name));
            io.remove_file(&path).await?;
        }
    }

    Ok(())
}

async fn write_new_repo(
    local_cache: &LocalCachedRepository,
    io: &DefaultEnvironmentIo,
) -> io::Result<String> {
    io.create_dir_all(REPO_CACHE_FOLDER.as_ref()).await?;

    // [0-9a-zA-Z._-]+
    fn is_id_name_for_file(id: &str) -> bool {
        !id.is_empty()
            && id
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'.' | b'_' | b'-'))
    }

    // try id.json
    let id_names = local_cache
        .id()
        .filter(|id| is_id_name_for_file(id))
        .map(|id| format!("{id}.json"))
        .into_iter();

    // finally generate with uuid v4.
    // note: this iterator is endless. Consumes uuidv4 infinitely.
    let guid_names = std::iter::from_fn(|| Some(format!("{}.json", uuid::Uuid::new_v4())));

    for file_name in id_names.chain(guid_names) {
        let path = Path::new(REPO_CACHE_FOLDER).join(&file_name);
        match io.create_new(&path).await {
            Ok(mut file) => {
                file.write_all(&to_vec_pretty_os_eol(&local_cache)?).await?;
                file.flush().await?;

                return Ok(file_name);
            }
            Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }

    unreachable!();
}

pub async fn clear_package_cache(io: &DefaultEnvironmentIo) -> io::Result<()> {
    clear_package_cache_folder(io, PACKAGE_CACHE_FOLDER).await?;
    clear_package_cache_folder(io, REPO_CACHE_FOLDER).await
}

async fn clear_package_cache_folder(io: &DefaultEnvironmentIo, folder: &str) -> io::Result<()> {
    let repo_folder_stream = match io.read_dir(folder.as_ref()).await {
        Ok(stream) => stream,
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    let pkg_folder_entries = repo_folder_stream.try_filter_map(|pkg_entry| async move {
        if pkg_entry.file_type().await?.is_dir() {
            return Ok(Some(pkg_entry));
        }
        Ok(None)
    });

    pkg_folder_entries
        .try_for_each_concurrent(None, |pkg_folder_entry| async move {
            let pkg_name = pkg_folder_entry.file_name();

            let pkg_folder_stream = io
                .read_dir(&Path::new(folder).join(pkg_folder_entry.file_name()))
                .await?
                .map_ok(move |inner| (pkg_name.clone(), inner));

            let cache_file_entries =
                pkg_folder_stream.try_filter_map(|(pkg_id, cache_entry)| async move {
                    let name = cache_entry.file_name();
                    let name = name.as_encoded_bytes();
                    if (name.starts_with(PACKAGE_CACHE_FILE_PREFIX.as_bytes())
                        || name.starts_with(LEGACY_PACKAGE_CACHE_FILE_PREFIX.as_bytes()))
                        && (name.ends_with(b".zip") || name.ends_with(b".zip.sha256"))
                        && cache_entry.file_type().await?.is_file()
                    {
                        return Ok(Some((pkg_id, cache_entry)));
                    }
                    Ok(None)
                });

            cache_file_entries
                .try_for_each_concurrent(None, |(pkg_id, cache_entry)| async move {
                    let file_path = Path::new(folder).join(pkg_id).join(cache_entry.file_name());
                    io.remove_file(&file_path).await?;
                    Ok(())
                })
                .await?;

            Ok(())
        })
        .await?;

    Ok(())
}

pub enum AddUserPackageResult {
    Success,
    NonAbsolute,
    BadPackage,
    AlreadyAdded,
}

#[derive(Debug)]
pub enum AddRepositoryErr {
    Io(io::Error),
    AlreadyAdded,
    OfflineMode,
}

impl fmt::Display for AddRepositoryErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddRepositoryErr::Io(ioerr) => fmt::Display::fmt(ioerr, f),
            AddRepositoryErr::AlreadyAdded => f.write_str("already repository added"),
            AddRepositoryErr::OfflineMode => {
                f.write_str("you can't add remote repo in offline mode")
            }
        }
    }
}

impl std::error::Error for AddRepositoryErr {}

impl From<io::Error> for AddRepositoryErr {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for AddRepositoryErr {
    fn from(value: serde_json::Error) -> Self {
        Self::Io(value.into())
    }
}
