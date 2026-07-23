use std::path::Path;

use crate::activity_log::{
    ActivityDetail, ActivityImportance, ActivityInput, ActivityKind, ActivityLogState,
    ActivitySource, operations, summarize_path, summarize_url, summarize_url_host,
    target_from_path,
};
use crate::commands::async_command::{AsyncCallResult, With, async_command};
use crate::commands::environment::settings::TauriPickProjectDefaultPathResult;
use crate::commands::prelude::*;
use crate::commands::safe_url;
use crate::logging::LogEntry;
use crate::os::{open_that, open_url};
use crate::updater::{self, Update};
use crate::utils::find_existing_parent_dir_or_home;
use tauri::{AppHandle, Manager, State, Window};
use tauri_plugin_dialog::DialogExt;
use url::Url;
use vrc_get_vpm::io::DefaultEnvironmentIo;

pub const ALCOMD3_DISPLAY_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(serde::Deserialize, specta::Type)]
#[allow(clippy::enum_variant_names)]
pub enum OpenOptions {
    ErrorIfNotExists,
    CreateFolderIfNotExists,
    OpenParentIfNotExists,
}

#[tauri::command]
#[specta::specta]
pub async fn util_open(
    app: AppHandle,
    path: String,
    if_not_exists: OpenOptions,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Open,
        ActivityImportance::Secondary,
        operations::GUI_OPEN_PATH,
        "Opening local path",
    )
    .target(target_from_path(&path))
    .details(vec![ActivityDetail::new("path", summarize_path(&path))]);
    activity
        .track_result(
            Some(&app),
            input,
            "Local path opened",
            Vec::new(),
            async move {
                let path = Path::new(&path);
                if !path.exists() {
                    match if_not_exists {
                        OpenOptions::ErrorIfNotExists => {
                            return Err(RustError::unrecoverable_str("Path does not exist"));
                        }
                        OpenOptions::CreateFolderIfNotExists => {
                            super::create_dir_all_with_err(&path).await?;
                            open_that(path)?;
                        }
                        OpenOptions::OpenParentIfNotExists => {
                            open_that(find_existing_parent_dir_or_home(path).as_os_str())?;
                        }
                    }
                } else {
                    open_that(path)?;
                }
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn util_open_url_nocheck(app: AppHandle, url: String) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = open_url_activity(&url);
    activity
        .track_result(Some(&app), input, "URL opened", Vec::new(), async move {
            open_that(url)?;
            Ok(())
        })
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn util_open_url(app: AppHandle, url: String) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = open_url_activity(&url);
    activity
        .track_result(Some(&app), input, "URL opened", Vec::new(), async move {
            if !Url::parse(&url).is_ok_and(|x| safe_url(&x)) {
                return Err(RustError::unrecoverable_str("Bad URL or bad scheme"));
            }
            open_url(url)?;
            Ok(())
        })
        .await
}

fn open_url_activity(url: &str) -> ActivityInput {
    ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Open,
        ActivityImportance::Secondary,
        operations::GUI_OPEN_URL,
        "Opening URL",
    )
    .target(summarize_url_host(url))
    .details(vec![ActivityDetail::new("url", summarize_url(url))])
}

#[tauri::command]
#[specta::specta]
pub fn util_get_log_entries() -> Vec<LogEntry> {
    crate::logging::get_log_entries()
}

#[tauri::command]
#[specta::specta]
pub fn util_get_version() -> String {
    ALCOMD3_DISPLAY_VERSION.to_string()
}

#[tauri::command]
#[specta::specta]
pub fn util_frontend_ready(app_handle: AppHandle) -> Result<(), RustError> {
    let Some(window) = app_handle.get_webview_window("main") else {
        return Err(RustError::unrecoverable_str("main window not found"));
    };
    window.show().map_err(RustError::unrecoverable)?;
    window.set_focus().map_err(RustError::unrecoverable)?;
    Ok(())
}

pub async fn check_for_update(
    app_handle: AppHandle,
    stable: bool,
) -> updater::Result<Option<Update>> {
    let endpoint = if let Ok(env) = std::env::var(
        "___ALCOMD3_UPDATER_URL_OVERRIDE_DEBUG_ONLY_FEATURE_YOU_SHOULD_NOT_USE_THIS___",
    ) {
        Url::parse(&env).unwrap()
    } else if stable {
        Url::parse(&crate::alcomd3_config::updater_endpoint(true)).unwrap()
    } else {
        Url::parse(&crate::alcomd3_config::updater_endpoint(false)).unwrap()
    };
    updater::check_for_update(&app_handle, endpoint).await
}

#[derive(serde::Serialize, specta::Type)]
pub struct CheckForUpdateResponse {
    version: u32,
    current_version: String,
    latest_version: String,
    updater_status: updater::UpdaterStatus,
    automatic_update_handled: bool,
    automatic_download: bool,
    update_downloaded: bool,
    update_description: Option<String>,
    update_description_localizations: Option<std::collections::HashMap<String, String>>,
    updater_disabled_messages: Option<indexmap::IndexMap<String, String>>,
}

fn store_checked_update(
    updater_state: &UpdaterState,
    response: Update,
    automatic_update_handled: bool,
    automatic_download: bool,
    update_downloaded: bool,
    updater_disabled_messages: Option<indexmap::IndexMap<String, String>>,
) -> CheckForUpdateResponse {
    let current_version = response.current_version.clone();
    let latest_version = response.version.clone();
    let updater_status = response.updater_status;
    let update_description = response.body.clone();
    let update_description_localizations = response.body_i18n.clone();
    let version = updater_state.set(response);

    CheckForUpdateResponse {
        version,
        current_version,
        latest_version,
        updater_status,
        automatic_update_handled,
        automatic_download,
        update_downloaded,
        update_description,
        update_description_localizations,
        updater_disabled_messages,
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum CheckedUpdateAction {
    Present,
    AutomaticDownload,
    Discard,
}

fn checked_update_action(
    manual: bool,
    automatic_update: bool,
    updater_status: updater::UpdaterStatus,
    checked_channel: &str,
    current_channel: &str,
) -> CheckedUpdateAction {
    if !manual && checked_channel != current_channel {
        CheckedUpdateAction::Discard
    } else if !manual && automatic_update && updater_status == updater::UpdaterStatus::Updatable {
        CheckedUpdateAction::AutomaticDownload
    } else {
        CheckedUpdateAction::Present
    }
}

#[tauri::command]
#[specta::specta]
pub async fn util_check_for_update(
    app_handle: AppHandle,
    updater_state: State<'_, UpdaterState>,
    config: State<'_, GuiConfigState>,
    io: State<'_, DefaultEnvironmentIo>,
    manual: bool,
) -> Result<Option<CheckForUpdateResponse>, RustError> {
    let source = if manual {
        ActivitySource::Gui
    } else {
        ActivitySource::System
    };
    let importance = if manual {
        ActivityImportance::Primary
    } else {
        ActivityImportance::Secondary
    };
    let app_for_activity = app_handle.clone();
    let activity = app_for_activity.state::<ActivityLogState>();
    let tracker = activity.start_activity(
        Some(&app_for_activity),
        ActivityInput::new(
            source,
            ActivityKind::Maintenance,
            importance,
            operations::UPDATE_CHECK,
            if manual {
                "Checking for updates"
            } else {
                "Checking for updates automatically"
            },
        )
        .details(vec![ActivityDetail::new("manual", manual.to_string())]),
    );

    let release_channel = config.get().release_channel.clone();
    let stable = release_channel == "stable";
    let response = match check_for_update(app_handle, stable).await {
        Ok(response) => response,
        Err(e) if !manual => {
            log::debug!("automatic update check failed silently: {e}");
            activity.finish_info(
                Some(&app_for_activity),
                &tracker,
                "Automatic update check failed silently",
                vec![ActivityDetail::new("manual", "false")],
            );
            return Ok(None);
        }
        Err(e) => {
            let error: RustError = e.into();
            activity.finish_failed(
                Some(&app_for_activity),
                &tracker,
                "Update check failed",
                vec![ActivityDetail::new("manual", manual.to_string())],
                &error,
            );
            return Err(error);
        }
    };

    let Some(response) = response else {
        activity.finish_success(
            Some(&app_for_activity),
            &tracker,
            "Update check completed",
            vec![
                ActivityDetail::new("manual", manual.to_string()),
                ActivityDetail::new("updateAvailable", "false"),
            ],
        );
        return Ok(None);
    };
    let current_version = response.current_version.clone();
    let latest_version = response.version.clone();
    let updater_status = response.updater_status;
    let updater_disabled_messages = if cfg!(feature = "no-self-updater") {
        option_env!("ALCOMD3_UPDATER_DISABLED_MESSAGE").and_then(|x| serde_json::from_str(x).ok())
    } else {
        None
    };

    let checked_update_action = {
        let current_config = config.get();
        checked_update_action(
            manual,
            current_config.automatic_update,
            updater_status,
            &release_channel,
            &current_config.release_channel,
        )
    };
    if checked_update_action == CheckedUpdateAction::Discard {
        activity.finish_info(
            Some(&app_for_activity),
            &tracker,
            "Automatic update check result was discarded after the release channel changed",
            vec![
                ActivityDetail::new("manual", "false"),
                ActivityDetail::new("updateAvailable", "true"),
                ActivityDetail::new("latestVersion", latest_version),
            ],
        );
        return Ok(None);
    }

    let update_downloaded = updater_status == updater::UpdaterStatus::Updatable
        && updater::staged_update_satisfies(io.inner(), &release_channel, &latest_version).await;

    if checked_update_action == CheckedUpdateAction::AutomaticDownload {
        let automatic_download = !update_downloaded
            && !updater::automatic_update_installation_failed(
                io.inner(),
                &release_channel,
                &latest_version,
            )
            .await;
        activity.finish_success(
            Some(&app_for_activity),
            &tracker,
            if update_downloaded {
                "Automatic update is already downloaded"
            } else if automatic_download {
                "Automatic update is ready to download"
            } else {
                "Automatic update download is paused after an installation failure"
            },
            vec![
                ActivityDetail::new("manual", "false"),
                ActivityDetail::new("updateAvailable", "true"),
                ActivityDetail::new("currentVersion", current_version),
                ActivityDetail::new("latestVersion", latest_version),
                ActivityDetail::new("downloaded", update_downloaded.to_string()),
                ActivityDetail::new("automaticDownload", automatic_download.to_string()),
            ],
        );
        return Ok(Some(store_checked_update(
            &updater_state,
            response,
            true,
            automatic_download,
            update_downloaded,
            updater_disabled_messages,
        )));
    }

    activity.finish_success(
        Some(&app_for_activity),
        &tracker,
        "Update check completed",
        vec![
            ActivityDetail::new("manual", manual.to_string()),
            ActivityDetail::new("updateAvailable", "true"),
            ActivityDetail::new("currentVersion", current_version.clone()),
            ActivityDetail::new("latestVersion", latest_version.clone()),
        ],
    );
    Ok(Some(store_checked_update(
        &updater_state,
        response,
        false,
        false,
        update_downloaded,
        updater_disabled_messages,
    )))
}

#[derive(serde::Serialize, specta::Type, Clone)]
#[serde(tag = "type")]
pub enum UpdateDownloadProgress {
    DownloadProgress { received: usize, total: Option<u64> },
    DownloadComplete,
}

#[tauri::command]
#[specta::specta]
pub async fn util_download_update(
    updater_state: State<'_, UpdaterState>,
    app_handle: AppHandle,
    config: State<'_, GuiConfigState>,
    io: State<'_, DefaultEnvironmentIo>,
    window: Window,
    channel: String,
    automatic: bool,
    version: u32,
) -> Result<AsyncCallResult<UpdateDownloadProgress, ()>, RustError> {
    let activity = app_handle.state::<ActivityLogState>();
    let tracker = activity.start_activity(
        Some(&app_handle),
        ActivityInput::new(
            if automatic {
                ActivitySource::System
            } else {
                ActivitySource::Gui
            },
            ActivityKind::Write,
            if automatic {
                ActivityImportance::Secondary
            } else {
                ActivityImportance::Primary
            },
            operations::UPDATE_DOWNLOAD,
            "Starting update download",
        )
        .details(vec![
            ActivityDetail::new("version", version.to_string()),
            ActivityDetail::new("automatic", automatic.to_string()),
        ]),
    );
    let app_for_async = app_handle.clone();
    let tracker_for_async = tracker.clone();
    let release_channel = config.get().release_channel.clone();
    let io = io.inner().clone();

    let result = async_command(channel, window, async move {
        let Some(response) = updater_state.take() else {
            return Err(RustError::unrecoverable_str("No update response found"));
        };

        if response.version() != version {
            return Err(RustError::unrecoverable_str("Update data version mismatch"));
        }

        With::<UpdateDownloadProgress>::continue_async(move |ctx| {
            let app_for_async = app_for_async.clone();
            let tracker_for_async = tracker_for_async.clone();
            async move {
                let outcome: Result<(), RustError> = async {
                    let downloaded = response
                        .into_data()
                        .download_for_staging(|received, total| {
                            ctx.emit(UpdateDownloadProgress::DownloadProgress { received, total })
                                .ok();
                        })
                        .await?;

                    let current_channel = app_for_async
                        .state::<GuiConfigState>()
                        .get()
                        .release_channel
                        .clone();
                    if current_channel != release_channel {
                        return Err(RustError::unrecoverable_str(
                            "The release channel changed while downloading the update",
                        ));
                    }

                    let staged_receipt = downloaded.persist(&io, &release_channel).await?;
                    let current_channel = app_for_async
                        .state::<GuiConfigState>()
                        .get()
                        .release_channel
                        .clone();
                    if current_channel != release_channel {
                        updater::discard_staged_update_if_matches(&io, staged_receipt).await?;
                        return Err(RustError::unrecoverable_str(
                            "The release channel changed while staging the update",
                        ));
                    }
                    ctx.emit(UpdateDownloadProgress::DownloadComplete).ok();

                    Ok(())
                }
                .await;

                if let Some(activity) = app_for_async.try_state::<ActivityLogState>() {
                    match &outcome {
                        Ok(()) => {
                            activity.finish_success(
                                Some(&app_for_async),
                                &tracker_for_async,
                                "Update download completed",
                                Vec::new(),
                            );
                        }
                        Err(error) => {
                            activity.finish_failed(
                                Some(&app_for_async),
                                &tracker_for_async,
                                "Update download failed",
                                Vec::new(),
                                error,
                            );
                        }
                    }
                }

                outcome
            }
        })
    })
    .await;

    if let Err(error) = &result {
        activity.finish_failed(
            Some(&app_handle),
            &tracker,
            "Update download failed to start",
            Vec::new(),
            error,
        );
    }

    result
}

#[tauri::command]
#[specta::specta]
pub async fn util_install_downloaded_update(
    app_handle: AppHandle,
    config: State<'_, GuiConfigState>,
    io: State<'_, DefaultEnvironmentIo>,
) -> Result<(), RustError> {
    let activity = app_handle.state::<ActivityLogState>();
    let tracker = activity.start_activity(
        Some(&app_handle),
        ActivityInput::new(
            ActivitySource::Gui,
            ActivityKind::Write,
            ActivityImportance::Primary,
            operations::UPDATE_INSTALL,
            "Installing downloaded update",
        ),
    );
    let release_channel = config.get().release_channel.clone();

    match updater::install_staged_update(&app_handle, io.inner(), &release_channel).await {
        Ok(true) => {
            activity.finish_success(
                Some(&app_handle),
                &tracker,
                "Downloaded update installation started",
                Vec::new(),
            );
            app_handle.exit(0);
            Ok(())
        }
        Ok(false) => {
            let error = RustError::unrecoverable_str("No downloaded update is ready to install");
            activity.finish_failed(
                Some(&app_handle),
                &tracker,
                "Downloaded update was not available",
                Vec::new(),
                &error,
            );
            Err(error)
        }
        Err(error) => {
            let error: RustError = error.into();
            activity.finish_failed(
                Some(&app_handle),
                &tracker,
                "Downloaded update installation failed",
                Vec::new(),
                &error,
            );
            Err(error)
        }
    }
}

#[cfg(windows)]
#[tauri::command]
#[specta::specta]
pub async fn util_is_bad_hostname() -> Result<bool, RustError> {
    unsafe {
        use windows::Win32::NetworkManagement::IpHelper::{FIXED_INFO_W2KSP1, GetNetworkParams};
        let mut len = 0;
        // ignore error since expecting ERROR_BUFFER_OVERFLOW
        GetNetworkParams(None, &mut len).ok().ok();
        let memory = vec![0u8; len as usize];
        let ptr = memory.as_ptr() as *mut FIXED_INFO_W2KSP1;
        GetNetworkParams(Some(ptr), &mut len)
            .ok()
            .map_err(RustError::unrecoverable)?;
        let info = &*ptr;
        Ok(info
            .HostName
            .iter()
            .take_while(|&&c| c != 0)
            .any(|&c| c < 0))
    }
}

#[cfg(not(windows))]
#[tauri::command]
#[specta::specta]
pub async fn util_is_bad_hostname() -> Result<bool, RustError> {
    Ok(false)
}

#[tauri::command]
#[specta::specta]
pub async fn util_pick_directory(
    window: Window,
    current: String,
) -> Result<TauriPickProjectDefaultPathResult, RustError> {
    let Some(dir) = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_directory(find_existing_parent_dir_or_home(current.as_ref()))
        .blocking_pick_folder()
        .map(|x| x.into_path_buf())
        .transpose()?
    else {
        return Ok(TauriPickProjectDefaultPathResult::NoFolderSelected);
    };

    let Ok(dir) = dir.into_os_string().into_string() else {
        return Ok(TauriPickProjectDefaultPathResult::InvalidSelection);
    };

    Ok(TauriPickProjectDefaultPathResult::Successful { new_path: dir })
}

#[cfg(test)]
mod tests {
    use super::{CheckedUpdateAction, checked_update_action, store_checked_update};
    use crate::state::UpdaterState;
    use crate::updater::{Update, UpdaterStatus};

    fn available_update() -> Update {
        Update {
            current_version: "1.0.0".to_string(),
            version: "1.1.0".to_string(),
            body: Some("Changes".to_string()),
            body_i18n: None,
            updater_status: UpdaterStatus::Updatable,
            updater: None,
        }
    }

    #[test]
    fn manual_check_preserves_the_dialog_flow() {
        assert_eq!(
            checked_update_action(true, true, UpdaterStatus::Updatable, "stable", "beta"),
            CheckedUpdateAction::Present
        );
    }

    #[test]
    fn automatic_updatable_check_starts_the_shared_download_flow() {
        assert_eq!(
            checked_update_action(false, true, UpdaterStatus::Updatable, "stable", "stable"),
            CheckedUpdateAction::AutomaticDownload
        );
    }

    #[test]
    fn automatic_non_updatable_check_preserves_the_dialog_flow() {
        for updater_status in [
            UpdaterStatus::NoPlatform,
            UpdaterStatus::NotUpdatable,
            UpdaterStatus::UpdaterDisabled,
        ] {
            assert_eq!(
                checked_update_action(false, true, updater_status, "stable", "stable"),
                CheckedUpdateAction::Present
            );
        }
    }

    #[test]
    fn background_check_is_discarded_after_channel_changes() {
        assert_eq!(
            checked_update_action(false, true, UpdaterStatus::Updatable, "stable", "beta"),
            CheckedUpdateAction::Discard
        );
        assert_eq!(
            checked_update_action(false, false, UpdaterStatus::NoPlatform, "stable", "beta"),
            CheckedUpdateAction::Discard
        );
    }

    #[test]
    fn handled_automatic_update_remains_available_to_the_frontend() {
        let updater_state = UpdaterState::new();

        let response =
            store_checked_update(&updater_state, available_update(), true, true, false, None);

        assert!(response.automatic_update_handled);
        assert!(response.automatic_download);
        assert!(!response.update_downloaded);
        assert_eq!(response.current_version, "1.0.0");
        assert_eq!(response.latest_version, "1.1.0");
        assert_eq!(response.update_description.as_deref(), Some("Changes"));
        assert!(updater_state.take().is_some());
    }

    #[test]
    fn manual_and_automatic_updates_share_the_staging_command() {
        let source = include_str!("util.rs");
        let check = source.find("pub async fn util_check_for_update").unwrap();
        let download = source.find("pub async fn util_download_update").unwrap();
        let install = source
            .find("pub async fn util_install_downloaded_update")
            .unwrap();

        assert!(!source[check..download].contains("download_for_staging"));
        assert!(source[download..install].contains("download_for_staging"));
        assert!(source[download..install].contains("downloaded.persist"));
        assert!(source[install..].contains("install_staged_update"));
    }
}
