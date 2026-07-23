use crate::StartupRequest;
use crate::activity_log::{
    ActivityDetail, ActivityImportance, ActivityInput, ActivityKind, ActivityLogState,
    ActivitySource, operations, summarize_url, summarize_url_host,
};
use crate::commands::import_templates;
use arc_swap::ArcSwapOption;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
#[allow(unused_imports)] // Manager is used only on linux
use tauri::{AppHandle, Emitter, Manager};
use url::{Host, Url};
use vrc_get_vpm::io::{DefaultEnvironmentIo, IoTrait};

static APP_HANDLE: ArcSwapOption<AppHandle> = ArcSwapOption::const_empty();

const PENDING_STARTUP_ARGS_SCHEMA_VERSION: u32 = 2;
static PENDING_STARTUP_ARGS_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingStartupArgs {
    schema_version: u32,
    requests: Vec<StartupRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyPendingStartupArgs {
    requests: Vec<Vec<String>>,
}

#[derive(Debug)]
enum ReadPendingStartupArgsError {
    InvalidData(io::Error),
    Io(io::Error),
}

#[derive(Debug)]
pub(crate) enum TakePendingStartupArgsError {
    Read(io::Error),
    InvalidData(io::Error),
}

impl std::fmt::Display for TakePendingStartupArgsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(
                formatter,
                "failed to read pending startup arguments: {error}"
            ),
            Self::InvalidData(error) => write!(
                formatter,
                "pending startup arguments contain invalid data: {error}"
            ),
        }
    }
}

impl std::error::Error for TakePendingStartupArgsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(error) | Self::InvalidData(error) => Some(error),
        }
    }
}

#[cfg(debug_assertions)]
const TEST_DISABLE_SYSTEM_INTEGRATION_ENV: &str = "ALCOMD3_TEST_DISABLE_SYSTEM_INTEGRATION";

pub fn set_app_handle(handle: AppHandle) {
    APP_HANDLE.store(Some(Arc::new(handle)));
}

#[cfg(test)]
async fn preserve_pending_startup_args(
    io: &DefaultEnvironmentIo,
    args: &[String],
) -> io::Result<()> {
    let Some(request) = args.get(1..).filter(|request| !request.is_empty()) else {
        return Ok(());
    };
    preserve_pending_startup_requests(io, &[StartupRequest::Arguments(request.to_vec())]).await
}

pub(crate) async fn preserve_pending_startup_requests(
    io: &DefaultEnvironmentIo,
    requests: &[StartupRequest],
) -> io::Result<()> {
    if requests.is_empty() {
        return Ok(());
    }
    let _guard = PENDING_STARTUP_ARGS_LOCK.lock().await;

    let mut pending = match read_pending_startup_args(io).await {
        Ok(Some(pending)) => pending,
        Ok(None) => PendingStartupArgs {
            schema_version: PENDING_STARTUP_ARGS_SCHEMA_VERSION,
            requests: Vec::new(),
        },
        Err(ReadPendingStartupArgsError::InvalidData(error)) => {
            log::warn!(gui_toast = false; "discarding invalid pending startup arguments: {error}");
            remove_pending_startup_args(io).await?;
            PendingStartupArgs {
                schema_version: PENDING_STARTUP_ARGS_SCHEMA_VERSION,
                requests: Vec::new(),
            }
        }
        Err(ReadPendingStartupArgsError::Io(error)) => return Err(error),
    };

    for request in requests {
        if !request.is_empty() && !pending.requests.contains(request) {
            pending.requests.push(request.clone());
        }
    }

    io.create_dir_all(Path::new(crate::storage::STARTUP_ARGS_DIR))
        .await?;
    let bytes = serde_json::to_vec_pretty(&pending).map_err(invalid_startup_args_data)?;
    io.write_atomic(Path::new(crate::storage::STARTUP_ARGS_PENDING_PATH), &bytes)
        .await
}

pub(crate) async fn take_pending_startup_args(
    io: &DefaultEnvironmentIo,
) -> Result<Vec<StartupRequest>, TakePendingStartupArgsError> {
    let _guard = PENDING_STARTUP_ARGS_LOCK.lock().await;
    let pending = match read_pending_startup_args(io).await {
        Ok(Some(pending)) => pending,
        Ok(None) => return Ok(Vec::new()),
        Err(ReadPendingStartupArgsError::InvalidData(error)) => {
            if let Err(remove_error) = remove_pending_startup_args(io).await {
                log::warn!(gui_toast = false; "failed to remove unreadable pending startup arguments: {remove_error}");
            }
            return Err(TakePendingStartupArgsError::InvalidData(error));
        }
        Err(ReadPendingStartupArgsError::Io(error)) => {
            return Err(TakePendingStartupArgsError::Read(error));
        }
    };

    Ok(pending.requests)
}

pub(crate) async fn acknowledge_pending_startup_args(
    io: &DefaultEnvironmentIo,
    consumed: &[StartupRequest],
) -> io::Result<()> {
    if consumed.is_empty() {
        return Ok(());
    }
    let _guard = PENDING_STARTUP_ARGS_LOCK.lock().await;
    let mut pending = match read_pending_startup_args(io).await {
        Ok(Some(pending)) => pending,
        Ok(None) => return Ok(()),
        Err(ReadPendingStartupArgsError::InvalidData(error))
        | Err(ReadPendingStartupArgsError::Io(error)) => return Err(error),
    };
    pending
        .requests
        .retain(|request| !consumed.contains(request));
    if pending.requests.is_empty() {
        return remove_pending_startup_args(io).await;
    }

    let bytes = serde_json::to_vec_pretty(&pending).map_err(invalid_startup_args_data)?;
    io.write_atomic(Path::new(crate::storage::STARTUP_ARGS_PENDING_PATH), &bytes)
        .await
}

async fn read_pending_startup_args(
    io: &DefaultEnvironmentIo,
) -> Result<Option<PendingStartupArgs>, ReadPendingStartupArgsError> {
    let path = io.resolve(Path::new(crate::storage::STARTUP_ARGS_PENDING_PATH));
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ReadPendingStartupArgsError::Io(error)),
    };

    let schema: PendingStartupArgsSchema = serde_json::from_slice(&bytes)
        .map_err(invalid_startup_args_data)
        .map_err(ReadPendingStartupArgsError::InvalidData)?;
    let pending = match schema.schema_version {
        1 => {
            let legacy: LegacyPendingStartupArgs = serde_json::from_slice(&bytes)
                .map_err(invalid_startup_args_data)
                .map_err(ReadPendingStartupArgsError::InvalidData)?;
            PendingStartupArgs {
                schema_version: PENDING_STARTUP_ARGS_SCHEMA_VERSION,
                requests: legacy
                    .requests
                    .into_iter()
                    .map(StartupRequest::Arguments)
                    .collect(),
            }
        }
        PENDING_STARTUP_ARGS_SCHEMA_VERSION => serde_json::from_slice(&bytes)
            .map_err(invalid_startup_args_data)
            .map_err(ReadPendingStartupArgsError::InvalidData)?,
        _ => {
            return Err(ReadPendingStartupArgsError::InvalidData(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported pending startup arguments schema",
            )));
        }
    };
    if pending.requests.iter().any(StartupRequest::is_empty) {
        return Err(ReadPendingStartupArgsError::InvalidData(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported pending startup arguments",
        )));
    }
    Ok(Some(pending))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingStartupArgsSchema {
    schema_version: u32,
}

async fn remove_pending_startup_args(io: &DefaultEnvironmentIo) -> io::Result<()> {
    match io
        .remove_file(Path::new(crate::storage::STARTUP_ARGS_PENDING_PATH))
        .await
    {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn invalid_startup_args_data(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[derive(Debug, Eq, PartialEq)]
enum DeepLink {
    AddRepository(AddRepositoryInfo),
}

fn parse_deep_link(deep_link: Url) -> Option<DeepLink> {
    if deep_link.scheme() != "vcc" {
        log::error!("Invalid deep link: {deep_link}");
        return None;
    }

    if deep_link.host() != Some(Host::Domain("vpm")) {
        log::error!("Invalid deep link: {deep_link}");
        return None;
    }

    match deep_link.path() {
        "/addRepo" => {
            // add repo
            let mut url = None;
            let mut headers = IndexMap::new();
            for (key, value) in deep_link.query_pairs() {
                match key.as_ref() {
                    "url" => {
                        if url.is_some() {
                            log::error!("Duplicate url query parameter");
                            return None;
                        }
                        let Some(parsed) = Url::parse(&value)
                            .ok()
                            .filter(|x| x.scheme() == "http" || x.scheme() == "https")
                        else {
                            log::error!("Invalid to remove url: {value}");
                            return None;
                        };
                        url = Some(parsed);
                    }
                    "headers[]" => {
                        let (key, value) = value.split_once(':')?;
                        headers.insert(key.to_string(), value.to_string());
                    }
                    _ => {
                        log::error!("Unknown query parameter: {key}");
                    }
                }
            }

            Some(DeepLink::AddRepository(AddRepositoryInfo {
                url: url?,
                headers,
            }))
        }
        _ => {
            log::error!("Unknown deep link: {deep_link}");
            None
        }
    }
}

#[derive(specta::Type, serde::Serialize, Debug, Eq, PartialEq)]
pub struct AddRepositoryInfo {
    url: Url,
    headers: IndexMap<String, String>,
}

static PENDING_ADD_REPOSITORY: Mutex<Vec<AddRepositoryInfo>> = Mutex::new(Vec::new());

pub fn on_deep_link(deep_link: Url) {
    match parse_deep_link(deep_link) {
        None => {}
        Some(DeepLink::AddRepository(add_repository)) => {
            if let Some(handle) = APP_HANDLE.load().as_ref()
                && let Some(activity) = handle.try_state::<ActivityLogState>()
            {
                activity.record_info(
                    Some(handle),
                    ActivityInput::new(
                        ActivitySource::DeepLink,
                        ActivityKind::Open,
                        ActivityImportance::Primary,
                        operations::DEEP_LINK_ADD_REPOSITORY,
                        "Received add repository deep link",
                    )
                    .target(summarize_url_host(add_repository.url.as_str()))
                    .details(vec![
                        ActivityDetail::new(
                            "repositoryUrl",
                            summarize_url(add_repository.url.as_str()),
                        ),
                        ActivityDetail::new("headers", add_repository.headers.len().to_string()),
                    ]),
                );
            }
            PENDING_ADD_REPOSITORY.lock().unwrap().push(add_repository);
            APP_HANDLE
                .load()
                .as_ref()
                .map(|handle| handle.emit("deep-link-add-repository", ()));
        }
    }
}

#[allow(unused_variables)]
pub fn should_install_deep_link(app: &AppHandle) -> bool {
    #[cfg(debug_assertions)]
    if std::env::var_os(TEST_DISABLE_SYSTEM_INTEGRATION_ENV).is_some_and(|value| !value.is_empty())
    {
        return false;
    }

    #[cfg(target_os = "linux")]
    if app.env().appimage.is_some() {
        return true;
    }

    cfg!(target_os = "windows")
}

static IMPORTED_NON_TOASTED_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn process_files(app: &AppHandle, files: Vec<PathBuf>) {
    if files.is_empty() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let imported = import_templates(&app.state(), &files).await;
        app.emit("templates-imported", imported).ok();
        IMPORTED_NON_TOASTED_COUNT.fetch_add(1, Ordering::SeqCst);
    });
}

#[tauri::command]
#[specta::specta]
pub fn deep_link_has_add_repository() -> bool {
    !PENDING_ADD_REPOSITORY.lock().unwrap().is_empty()
}

#[tauri::command]
#[specta::specta]
pub fn deep_link_take_add_repository() -> Option<AddRepositoryInfo> {
    PENDING_ADD_REPOSITORY.lock().unwrap().pop()
}

#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn deep_link_install_vcc(_app: AppHandle) {
    // for macos, nothing to do!
    log::error!("deep_link_install_vcc is not supported on macos");
}

#[tauri::command]
#[specta::specta]
#[cfg(windows)]
pub async fn deep_link_install_vcc(_app: AppHandle) {
    // for windows, install to registry
    fn impl_() -> std::io::Result<()> {
        let exe = std::env::current_exe()?;
        let exe = exe.to_string_lossy();

        let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .create_subkey("Software\\Classes\\vcc")?;
        key.set_value("URL Protocol", &"")?;
        key.set_value("AppUserModelID", &crate::alcomd3_config::windows_aumid())?;
        let (default_icon, _) = key.create_subkey("DefaultIcon")?;
        default_icon.set_value("", &format!("\"{exe}\",0"))?;
        let (command, _) = key.create_subkey("shell\\open\\command")?;
        command.set_value("", &format!("\"{exe}\" link \"%1\""))?;
        Ok(())
    }

    if let Err(e) = impl_() {
        log::error!("Failed to install vcc deep link: {e}");
    }
}

#[tauri::command]
#[specta::specta]
#[cfg(target_os = "linux")]
pub async fn deep_link_install_vcc(app: AppHandle) {
    use tauri::Manager as _;
    // for linux, create a desktop entry
    // https://specifications.freedesktop.org/desktop-entry-spec/desktop-entry-spec-latest.html

    let Some(home_dir) = dirs_next::data_dir() else {
        log::error!("Failed to get XDG_DATA_HOME");
        return;
    };
    let applications_dir = home_dir.join("applications");
    let desktop_file = applications_dir.join(format!(
        "{app_id}.desktop",
        app_id = "com.anatawa12.vrc_get"
    ));

    let Some(appimage_path) = app.env().appimage.and_then(|x| x.into_string().ok()) else {
        log::error!("Failed to get appimage path");
        return;
    };

    let contents = format!(
        r#"[Desktop Entry]
Type=Application
Name=ALCOMD3
Exec="{appimage_path}" link %u
NoDisplay=true
Terminal=false
MimeType=x-scheme-handler/vcc
Categories=Utility;
"#,
        appimage_path = escape(&appimage_path)
    );

    if let Err(e) = tokio::fs::create_dir_all(&applications_dir).await {
        log::error!("Failed to create applications directory: {e}");
        return;
    }

    if let Err(e) = tokio::fs::write(&desktop_file, &contents).await {
        log::error!("Failed to write desktop file: {e}");
        return;
    }

    log::info!("Desktop file created: {}", desktop_file.display());

    if let Err(e) = tokio::process::Command::new("update-desktop-database")
        .arg(applications_dir)
        .status()
        .await
    {
        log::error!("Failed to call update-desktop-database: {e}");
    }

    fn escape(s: &str) -> String {
        s.replace('\\', r#"\\\\"#)
            .replace('`', r#"\\`"#)
            .replace('$', r#"\\$"#)
            .replace('"', r#"\\""#)
    }
}

#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn deep_link_uninstall_vcc(_app: AppHandle) {
    // for macos, nothing to do!
    log::error!("deep_link_uninstall_vcc is not supported on macos");
}

#[tauri::command]
#[specta::specta]
#[cfg(windows)]
pub async fn deep_link_uninstall_vcc(_app: AppHandle) {
    // for windows, install to registry
    fn impl_() -> std::io::Result<()> {
        winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .delete_subkey_all("Software\\Classes\\vcc")?;
        Ok(())
    }

    if let Err(e) = impl_() {
        log::error!("Failed to install vcc deep link: {e}");
    }
}

#[tauri::command]
#[specta::specta]
#[cfg(target_os = "linux")]
pub async fn deep_link_uninstall_vcc(_app: AppHandle) {
    // for linux, create a desktop entry
    // https://specifications.freedesktop.org/desktop-entry-spec/desktop-entry-spec-latest.html

    let Some(home_dir) = dirs_next::data_dir() else {
        log::error!("Failed to get XDG_DATA_HOME");
        return;
    };
    let applications_dir = home_dir.join("applications");
    let desktop_file = applications_dir.join(format!(
        "{app_id}.desktop",
        app_id = "com.anatawa12.vrc_get"
    ));

    match tokio::fs::remove_file(&desktop_file).await {
        Ok(()) => {
            log::info!("Desktop file removed: {}", desktop_file.display());
        }
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::info!("Desktop file was not found: {}", desktop_file.display());
            return;
        }
        Err(e) => {
            log::error!("Failed to remove desktop file: {e}");
            return;
        }
    }

    if let Err(e) = tokio::process::Command::new("update-desktop-database")
        .arg(applications_dir)
        .status()
        .await
    {
        log::error!("Failed to call update-desktop-database: {e}");
    }
}

#[tauri::command]
#[specta::specta]
pub fn deep_link_imported_clear_non_toasted_count() -> usize {
    IMPORTED_NON_TOASTED_COUNT.swap(0, Ordering::SeqCst)
}

#[tauri::command]
#[specta::specta]
pub fn deep_link_reduce_imported_clear_non_toasted_count(reduce: usize) {
    IMPORTED_NON_TOASTED_COUNT.fetch_sub(reduce, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    async fn take_requests(io: &DefaultEnvironmentIo) -> Vec<StartupRequest> {
        let requests = take_pending_startup_args(io).await.unwrap();
        acknowledge_pending_startup_args(io, &requests)
            .await
            .unwrap();
        requests
    }

    #[test]
    fn parse_add_repo() {
        let deep_link =
            parse_deep_link(Url::parse("vcc://vpm/addRepo?url=https://example.com").unwrap())
                .unwrap();
        assert_eq!(
            deep_link,
            DeepLink::AddRepository(AddRepositoryInfo {
                url: Url::parse("https://example.com").unwrap(),
                headers: IndexMap::new(),
            })
        );

        let deep_link = parse_deep_link(
            Url::parse("vcc://vpm/addRepo?url=https%3A%2F%2Fvpm.anatawa12.com%2Fvpm.json").unwrap(),
        )
        .unwrap();
        assert_eq!(
            deep_link,
            DeepLink::AddRepository(AddRepositoryInfo {
                url: Url::parse("https://vpm.anatawa12.com/vpm.json").unwrap(),
                headers: IndexMap::new(),
            })
        );

        let deep_link = parse_deep_link(
            Url::parse("vcc://vpm/addRepo?url=https%3A%2F%2Fvpm.anatawa12.com%2Fvpm.json&headers[]=Authorization:test").unwrap()).unwrap();
        assert_eq!(
            deep_link,
            DeepLink::AddRepository(AddRepositoryInfo {
                url: Url::parse("https://vpm.anatawa12.com/vpm.json").unwrap(),
                headers: {
                    let mut map = IndexMap::new();
                    map.insert("Authorization".to_string(), "test".to_string());
                    map
                },
            })
        );
    }

    #[test]
    fn pending_startup_args_survive_restart_without_duplicate_dispatch() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            let request = "vcc://vpm/addRepo?url=https://example.com";

            preserve_pending_startup_args(&io, &["old.exe".into(), request.into()])
                .await
                .unwrap();
            preserve_pending_startup_args(&io, &["new.exe".into(), request.into()])
                .await
                .unwrap();

            assert_eq!(
                take_requests(&io).await,
                vec![StartupRequest::Arguments(vec![request.to_string()])]
            );
            assert!(take_requests(&io).await.is_empty());
        });
    }

    #[test]
    fn opened_urls_survive_restart_with_their_original_encoding() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            let request = StartupRequest::OpenedUrls(vec![
                "file:///Users/test/%E6%A8%A1%E6%9D%BF.alcomtemplate".to_string(),
                "vcc://vpm/addRepo?url=https%3A%2F%2Fexample.com%2Fvpm.json".to_string(),
            ]);

            preserve_pending_startup_requests(&io, std::slice::from_ref(&request))
                .await
                .unwrap();
            assert_eq!(take_requests(&io).await, vec![request]);
        });
    }

    #[test]
    fn unreadable_pending_startup_args_keep_the_original_error() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            io.create_dir_all(Path::new(crate::storage::STARTUP_ARGS_DIR))
                .await
                .unwrap();
            let pending_path = io.resolve(Path::new(crate::storage::STARTUP_ARGS_PENDING_PATH));
            tokio::fs::write(&pending_path, b"{invalid json")
                .await
                .unwrap();

            let error = take_pending_startup_args(&io).await.unwrap_err();
            match error {
                TakePendingStartupArgsError::InvalidData(error) => {
                    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
                }
                TakePendingStartupArgsError::Read(error) => {
                    panic!("invalid data was reported as an I/O error: {error}");
                }
            }
            assert!(!pending_path.exists());
        });
    }

    #[test]
    fn concurrent_pending_startup_args_are_serialized_without_lost_updates() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            let first = ["app.exe".to_string(), "first.alcomtemplate".to_string()];
            let second = ["app.exe".to_string(), "second.alcomtemplate".to_string()];

            let (first_result, second_result) = futures::join!(
                preserve_pending_startup_args(&io, &first),
                preserve_pending_startup_args(&io, &second),
            );
            first_result.unwrap();
            second_result.unwrap();

            let mut pending = take_requests(&io).await;
            pending.sort_by_key(|request| format!("{request:?}"));
            assert_eq!(
                pending,
                vec![
                    StartupRequest::Arguments(vec!["first.alcomtemplate".to_string()]),
                    StartupRequest::Arguments(vec!["second.alcomtemplate".to_string()]),
                ]
            );
        });
    }

    #[test]
    fn concurrent_preserve_and_take_neither_lose_nor_duplicate_requests() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            let first = ["app.exe".to_string(), "first.alcomtemplate".to_string()];
            let second = ["app.exe".to_string(), "second.alcomtemplate".to_string()];
            preserve_pending_startup_args(&io, &first).await.unwrap();

            let (taken_during_preserve, preserve_result) = futures::join!(
                take_pending_startup_args(&io),
                preserve_pending_startup_args(&io, &second),
            );
            preserve_result.unwrap();

            let mut observed = taken_during_preserve.unwrap();
            acknowledge_pending_startup_args(&io, &observed)
                .await
                .unwrap();
            observed.extend(take_requests(&io).await);
            observed.sort_by_key(|request| format!("{request:?}"));
            assert_eq!(
                observed,
                vec![
                    StartupRequest::Arguments(vec!["first.alcomtemplate".to_string()]),
                    StartupRequest::Arguments(vec!["second.alcomtemplate".to_string()]),
                ]
            );
        });
    }

    #[test]
    fn pending_startup_args_io_errors_do_not_delete_or_replace_existing_data() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            io.create_dir_all(Path::new(crate::storage::STARTUP_ARGS_PENDING_PATH))
                .await
                .unwrap();

            let preserve_error = preserve_pending_startup_args(
                &io,
                &["app".to_string(), "request.alcomtemplate".to_string()],
            )
            .await
            .unwrap_err();
            assert_ne!(preserve_error.kind(), io::ErrorKind::InvalidData);
            assert!(
                io.resolve(Path::new(crate::storage::STARTUP_ARGS_PENDING_PATH))
                    .is_dir()
            );

            assert!(matches!(
                take_pending_startup_args(&io).await,
                Err(TakePendingStartupArgsError::Read(_))
            ));
            assert!(
                io.resolve(Path::new(crate::storage::STARTUP_ARGS_PENDING_PATH))
                    .is_dir()
            );
        });
    }

    #[test]
    fn pending_startup_args_remain_until_dispatch_is_acknowledged() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            let request = ["app".to_string(), "request.alcomtemplate".to_string()];
            preserve_pending_startup_args(&io, &request).await.unwrap();

            let first_read = take_pending_startup_args(&io).await.unwrap();
            assert_eq!(take_pending_startup_args(&io).await.unwrap(), first_read);
            acknowledge_pending_startup_args(&io, &first_read)
                .await
                .unwrap();
            assert!(take_pending_startup_args(&io).await.unwrap().is_empty());
        });
    }

    #[test]
    fn acknowledging_dispatch_keeps_requests_appended_after_the_read() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            let first = ["app".to_string(), "first.alcomtemplate".to_string()];
            let second = ["app".to_string(), "second.alcomtemplate".to_string()];
            preserve_pending_startup_args(&io, &first).await.unwrap();

            let dispatched = take_pending_startup_args(&io).await.unwrap();
            preserve_pending_startup_args(&io, &second).await.unwrap();
            acknowledge_pending_startup_args(&io, &dispatched)
                .await
                .unwrap();

            assert_eq!(
                take_requests(&io).await,
                vec![StartupRequest::Arguments(vec![
                    "second.alcomtemplate".to_string()
                ])]
            );
        });
    }

    #[test]
    fn legacy_pending_startup_args_are_migrated_on_read() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            io.create_dir_all(Path::new(crate::storage::STARTUP_ARGS_DIR))
                .await
                .unwrap();
            let pending_path = io.resolve(Path::new(crate::storage::STARTUP_ARGS_PENDING_PATH));
            tokio::fs::write(
                pending_path,
                br#"{"schemaVersion":1,"requests":[["legacy.alcomtemplate"]]}"#,
            )
            .await
            .unwrap();

            assert_eq!(
                take_requests(&io).await,
                vec![StartupRequest::Arguments(vec![
                    "legacy.alcomtemplate".to_string()
                ])]
            );
        });
    }
}
