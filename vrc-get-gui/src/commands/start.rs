use crate::commands::prelude::*;

use crate::activity_log::{
    ActivityDetail, ActivityImportance, ActivityInput, ActivityKind, ActivityLogState,
    ActivitySource, operations,
};
use crate::commands::environment::unity_hub::update_unity_paths_from_unity_hub;
use log::{error, info};
use std::io;
use tauri::async_runtime::spawn;
use tauri::{App, AppHandle, LogicalSize, Manager, State, WebviewWindow, WindowEvent};
use vrc_get_vpm::io::DefaultEnvironmentIo;

trait WindowExt {
    fn make_fullscreen_ish(&self) -> tauri::Result<()>;
    fn is_fullscreen_ish(&self) -> tauri::Result<bool>;
}

impl WindowExt for WebviewWindow {
    fn make_fullscreen_ish(&self) -> tauri::Result<()> {
        if !cfg!(target_os = "macos") {
            self.maximize()
        } else {
            self.set_fullscreen(true)
        }
    }

    fn is_fullscreen_ish(&self) -> tauri::Result<bool> {
        if !cfg!(target_os = "macos") {
            self.is_maximized()
        } else {
            self.is_fullscreen()
        }
    }
}

pub fn startup(app: &mut App, initial_args: Vec<String>) {
    crate::capture_initial_startup_args(app.handle(), &initial_args);
    let handle = app.handle().clone();
    spawn(async move {
        if let Err(e) = open_main(handle.clone()).await {
            error!("failed to open main window: {e}");
            crate::process_startup_requests(
                &handle,
                crate::finish_startup_request_capture(&handle),
            );
        }
    });

    async fn update_unity_hub(
        app: AppHandle,
        settings: State<'_, SettingsState>,
        config: State<'_, GuiConfigState>,
        io: State<'_, DefaultEnvironmentIo>,
    ) -> Result<(), io::Error> {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let activity_tracker = app.try_state::<ActivityLogState>().map(|activity| {
            activity.start_activity(
                Some(&app),
                ActivityInput::new(
                    ActivitySource::System,
                    ActivityKind::Passive,
                    ActivityImportance::Secondary,
                    operations::UNITY_HUB_REFRESH,
                    "Refreshing Unity paths from Unity Hub in background",
                ),
            )
        });

        let result = update_unity_paths_from_unity_hub(&settings, &config, &io).await;
        match &result {
            Ok(true) => {
                if let Some(activity_tracker) = &activity_tracker
                    && let Some(activity) = app.try_state::<ActivityLogState>()
                {
                    activity.finish_success(
                        Some(&app),
                        activity_tracker,
                        "Unity paths refreshed from Unity Hub",
                        vec![ActivityDetail::new("foundUnityHub", "true")],
                    );
                }
            }
            Ok(false) => {
                if let Some(activity_tracker) = &activity_tracker
                    && let Some(activity) = app.try_state::<ActivityLogState>()
                {
                    activity.finish_info(
                        Some(&app),
                        activity_tracker,
                        "Unity Hub was not found",
                        vec![ActivityDetail::new("foundUnityHub", "false")],
                    );
                }
            }
            Err(error) => {
                if let Some(activity_tracker) = &activity_tracker
                    && let Some(activity) = app.try_state::<ActivityLogState>()
                {
                    activity.finish_failed(
                        Some(&app),
                        activity_tracker,
                        "Unity Hub refresh failed",
                        Vec::new(),
                        error,
                    );
                }
            }
        }

        if result? {
            info!("finished updating unity from unity hub");
        } else {
            info!("Unity Hub not found");
        }

        Ok(())
    }

    async fn open_main(app: AppHandle) -> tauri::Result<()> {
        let io = app.state::<DefaultEnvironmentIo>();
        let config = GuiConfigState::new_load(io.inner()).await?;
        app.manage(config);
        let theme_config = ThemeConfigState::new_load(io.inner()).await?;
        app.manage(theme_config);

        let release_channel = {
            let config = app.state::<GuiConfigState>();
            let config = config.get();
            config.release_channel.clone()
        };
        let startup_requests_ready = match preserve_startup_request_snapshot(&app, io.inner()).await
        {
            Ok(()) => true,
            Err(error) => {
                error!(gui_toast = false; "failed to preserve startup requests before installing a downloaded update: {error}");
                false
            }
        };
        if startup_requests_ready {
            match crate::updater::install_staged_update(&app, io.inner(), &release_channel).await {
                Ok(true) => {
                    info!("installed staged update before opening the main window");
                    app.exit(0);
                    return Ok(());
                }
                Ok(false) => {}
                Err(error) => {
                    error!(gui_toast = false; "failed to install staged update before startup: {error}");
                }
            }
        }

        let mcp = app.state::<crate::mcp::McpState>();
        if let Err(e) = mcp.ensure_running(app.clone()).await {
            error!("failed to start MCP IPC endpoint: {e}");
        }

        let handle = app.clone();
        spawn(async move {
            let state = handle.state();
            let config = handle.state();
            let io = handle.state();
            if let Err(e) = update_unity_hub(handle.clone(), state, config, io).await {
                error!("failed to update unity from unity hub: {e}");
            }
        });

        let config = app.state::<GuiConfigState>();
        let config = config.get().clone();

        if crate::deep_link_support::should_install_deep_link(&app)
            && config.use_alcom_for_vcc_protocol
        {
            spawn(crate::deep_link_support::deep_link_install_vcc(app.clone()));
        }

        use super::environment::config::SetupPages;
        let start_page = SetupPages::pages(&app)
            .iter()
            .copied()
            .find(|page| !page.is_finished(config.setup_process_progress))
            .map(|x| x.path())
            .unwrap_or("/projects/");

        let initial_width = config.window_size.width.max(101);
        let initial_height = config.window_size.height.max(101);

        let window = tauri::WebviewWindowBuilder::new(
            &app,
            "main", /* the unique window label */
            tauri::WebviewUrl::App(start_page.into()),
        )
        .title("ALCOMD3")
        .resizable(true)
        .inner_size(initial_width as f64, initial_height as f64)
        .visible(false)
        .incognito(true) // this prevents the webview from saving data
        .on_navigation(|url| {
            if cfg!(debug_assertions) && url.host_str() == Some("localhost") {
                return true;
            }
            if cfg!(windows) {
                url.scheme() == "http" && url.host_str() == Some("tauri.localhost")
                    || url.host_str() == Some("vrc-get.localhost")
            } else {
                url.scheme() == "tauri" || url.scheme() == "vrc-get"
            }
        })
        .build()?;

        if config.fullscreen {
            window.make_fullscreen_ish()?;
        }

        let cloned = window.clone();

        let resize_debounce: std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>> =
            std::sync::Mutex::new(None);

        #[allow(clippy::single_match)]
        window.on_window_event(move |e| match e {
            WindowEvent::Resized(size) => {
                let logical = size
                    .to_logical::<u32>(cloned.current_monitor().unwrap().unwrap().scale_factor());

                if logical.width < 100 || logical.height < 100 {
                    // ignore too small sizes
                    // this is generally caused by the window being minimized
                    return;
                }

                let fullscreen = cloned.is_fullscreen_ish().unwrap();

                let mut resize_debounce = resize_debounce.lock().unwrap();

                if let Some(resize_debounce) = resize_debounce.as_ref() {
                    resize_debounce.abort();
                }

                let cloned = cloned.clone();

                *resize_debounce = Some(tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                    if let Err(e) = save_window_size(cloned, logical, fullscreen).await {
                        error!("failed to save window size: {e}");
                    }
                }));
            }
            _ => {}
        });

        dispatch_startup_requests(&app, io.inner()).await;

        async fn save_window_size(
            window: WebviewWindow,
            size: LogicalSize<u32>,
            fullscreen: bool,
        ) -> tauri::Result<()> {
            info!(
                "saving window size: {}x{}, full: {fullscreen}",
                size.width, size.height
            );
            let config = window.state::<GuiConfigState>();
            let mut config = config.load_mut().await?;
            if fullscreen {
                config.fullscreen = true;
            } else {
                config.fullscreen = false;
                config.window_size.width = size.width;
                config.window_size.height = size.height;
            }
            config.save().await?;
            Ok(())
        }

        Ok(())
    }

    async fn preserve_startup_request_snapshot(
        app: &AppHandle,
        io: &DefaultEnvironmentIo,
    ) -> io::Result<()> {
        let (_, requests) = crate::snapshot_startup_requests(app);
        crate::deep_link_support::preserve_pending_startup_requests(io, &requests).await
    }

    async fn dispatch_startup_requests(app: &AppHandle, io: &DefaultEnvironmentIo) {
        let pending = match crate::deep_link_support::take_pending_startup_args(io).await {
            Ok(pending) => pending,
            Err(error) => {
                error!(gui_toast = false; "failed to consume pending startup arguments: {error}");
                Vec::new()
            }
        };
        let mut requests = pending.clone();

        for request in crate::finish_startup_request_capture(app) {
            if !requests.contains(&request) {
                requests.push(request);
            }
        }

        crate::process_startup_requests(app, requests);
        if let Err(error) =
            crate::deep_link_support::acknowledge_pending_startup_args(io, &pending).await
        {
            error!(gui_toast = false; "failed to acknowledge pending startup arguments; requests may be retried: {error}");
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn ordinary_startup_has_no_external_data_import_hook() {
        let source = include_str!("start.rs");
        let forbidden_patterns = [
            concat!("new_", "legacy_vcc"),
            concat!("new_", "legacy_alcom"),
            concat!("legacy", "_import"),
            concat!("storage", "_migration"),
            concat!("run_startup", "_storage", "_migration"),
        ];

        for pattern in forbidden_patterns {
            assert!(
                !source.contains(pattern),
                "ordinary startup must not reference {pattern}"
            );
        }
    }

    #[test]
    fn staged_update_is_installed_before_services_and_main_window() {
        let source = include_str!("start.rs");
        let preserve_args = source
            .find("preserve_startup_request_snapshot(&app")
            .unwrap();
        let install = source.find("install_staged_update").unwrap();
        let mcp = source.find("mcp.ensure_running").unwrap();
        let main_window = source.find("WebviewWindowBuilder::new").unwrap();
        let dispatch_args = source.find("dispatch_startup_requests(&app").unwrap();

        assert!(preserve_args < install);
        assert!(install < mcp);
        assert!(install < main_window);
        assert!(install < dispatch_args);
    }

    #[test]
    fn downloaded_update_installation_does_not_depend_on_automatic_download_setting() {
        let source = include_str!("start.rs");
        let open_main = source.find("async fn open_main").unwrap();
        let install = source[open_main..].find("install_staged_update").unwrap() + open_main;
        let mcp = source[install..].find("mcp.ensure_running").unwrap() + install;

        assert!(!source[open_main..mcp].contains("automatic_update"));
        assert!(!source[open_main..mcp].contains("discard_staged_update"));
    }
}
