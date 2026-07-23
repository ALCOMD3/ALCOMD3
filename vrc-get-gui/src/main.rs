// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const STARTUP_REQUEST_HANDOFF_MAX_ATTEMPTS: u8 = 3;
const STARTUP_REQUEST_HANDOFF_RETRY_DELAY: std::time::Duration =
    std::time::Duration::from_millis(250);

mod activity_log;
mod alcomd3_config;
mod backend;
mod commands;
mod compressor;
mod config;
mod deep_link_support;
mod log_sanitization;
mod logging;
mod mcp;
mod storage;
mod templates;

#[cfg_attr(windows, path = "os_windows.rs")]
#[cfg_attr(not(windows), path = "os_posix.rs")]
mod os;
mod state;
mod updater;
mod utils;

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
enum StartupRequest {
    Arguments(Vec<String>),
    OpenedUrls(Vec<String>),
}

impl StartupRequest {
    fn is_empty(&self) -> bool {
        match self {
            Self::Arguments(arguments) | Self::OpenedUrls(arguments) => arguments.is_empty(),
        }
    }
}

struct StartupRequestState {
    inner: Mutex<StartupRequestStateInner>,
}

struct StartupRequestStateInner {
    capturing: bool,
    generation: u64,
    handoff: StartupRequestHandoff,
    requests: Vec<StartupRequest>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupRequestHandoff {
    Inactive,
    Armed { restart: bool, attempts: u8 },
    Finalizing { restart: bool, attempt: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StartupRequestHandoffFinalization {
    restart: bool,
    attempt: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupRequestHandoffFailure {
    Retry,
    ContinueExit { restart: bool },
}

impl Default for StartupRequestState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(StartupRequestStateInner {
                capturing: true,
                generation: 0,
                handoff: StartupRequestHandoff::Inactive,
                requests: Vec::new(),
            }),
        }
    }
}

impl StartupRequestState {
    fn capture(&self, request: StartupRequest) -> bool {
        if request.is_empty() {
            return true;
        }

        let mut inner = self.inner.lock().unwrap();
        if !inner.capturing {
            return false;
        }
        if !inner.requests.contains(&request) {
            inner.requests.push(request);
            inner.generation = inner.generation.wrapping_add(1);
        }
        true
    }

    fn snapshot(&self) -> (u64, Vec<StartupRequest>) {
        let inner = self.inner.lock().unwrap();
        (inner.generation, inner.requests.clone())
    }

    fn complete_handoff_finalization(&self, generation: u64) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if !matches!(inner.handoff, StartupRequestHandoff::Finalizing { .. })
            || inner.generation != generation
        {
            return false;
        }
        inner.capturing = false;
        inner.handoff = StartupRequestHandoff::Inactive;
        true
    }

    fn arm_handoff(&self, restart: bool) {
        self.inner.lock().unwrap().handoff = StartupRequestHandoff::Armed {
            restart,
            attempts: 0,
        };
    }

    fn disarm_handoff(&self) {
        let mut inner = self.inner.lock().unwrap();
        if matches!(inner.handoff, StartupRequestHandoff::Armed { .. }) {
            inner.handoff = StartupRequestHandoff::Inactive;
        }
    }

    fn begin_handoff_finalization(&self) -> Option<StartupRequestHandoffFinalization> {
        let mut inner = self.inner.lock().unwrap();
        let StartupRequestHandoff::Armed { restart, attempts } = inner.handoff else {
            return None;
        };
        let attempt = attempts.saturating_add(1);
        inner.handoff = StartupRequestHandoff::Finalizing { restart, attempt };
        Some(StartupRequestHandoffFinalization { restart, attempt })
    }

    fn handoff_finalization_failed(&self) -> StartupRequestHandoffFailure {
        let mut inner = self.inner.lock().unwrap();
        let StartupRequestHandoff::Finalizing { restart, attempt } = inner.handoff else {
            return StartupRequestHandoffFailure::ContinueExit { restart: false };
        };
        if attempt < STARTUP_REQUEST_HANDOFF_MAX_ATTEMPTS {
            inner.handoff = StartupRequestHandoff::Armed {
                restart,
                attempts: attempt,
            };
            StartupRequestHandoffFailure::Retry
        } else {
            inner.capturing = false;
            inner.handoff = StartupRequestHandoff::Inactive;
            StartupRequestHandoffFailure::ContinueExit { restart }
        }
    }

    fn finish_capture(&self) -> Vec<StartupRequest> {
        let mut inner = self.inner.lock().unwrap();
        inner.capturing = false;
        std::mem::take(&mut inner.requests)
    }
}

// for clippy compatibility
#[cfg(not(clippy))]
fn tauri_context() -> tauri::Context {
    tauri::generate_context!()
}

#[cfg(clippy)]
fn tauri_context() -> tauri::Context {
    panic!()
}

fn main() {
    #[cfg(target_os = "macos")]
    updater::macos::try_run_updater_helper(); // This file can be updater helper.

    let io = logging::initialize_logger();
    let initial_args = std::env::args().collect::<Vec<_>>();

    // logger is now initialized, we can use log for panics
    log_panics::init();

    #[cfg(windows)]
    os::set_current_process_app_user_model_id(alcomd3_config::windows_aumid())
        .expect("failed to set the Windows AppUserModelID");

    // prevent errors caused by hitting the file descriptor limit during project backup creation
    #[cfg(target_os = "macos")]
    if let Err(e) = rlimit::increase_nofile_limit(4096) {
        log::error!("error while increasing nofile limit: {e}");
    }

    #[cfg(dev)]
    commands::export_ts();

    let activity_log_state = activity_log::ActivityLogState::new(&io);

    let builder = tauri::Builder::default();
    #[cfg(feature = "desktop-e2e-webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());

    let app = builder
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            log::info!("single instance remote procedure, {argv:?}, {cwd}");
            focus_main_window(app);
            route_startup_args(app, &argv);
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(io.clone())
        .manage(StartupRequestState::default())
        .manage(state::new_http_client())
        .manage(state::SettingsState::new())
        .manage(state::UpdaterState::new())
        .manage(state::PackagesState::new())
        .manage(state::ChangesState::new())
        .manage(state::ProjectApplyState::new())
        .manage(state::ProjectBackupState::new())
        .manage(state::ProjectCopyState::new())
        .manage(state::ProjectRestoreState::new())
        .manage(state::TemplatesState::new())
        .manage(activity_log_state)
        .manage(mcp::McpState::new())
        .register_uri_scheme_protocol("vrc-get", commands::handle_vrc_get_scheme)
        .invoke_handler(commands::handlers())
        .setup(move |app| {
            deep_link_support::set_app_handle(app.handle().clone());
            commands::startup(app, initial_args);
            Ok(())
        })
        .build(tauri_context())
        .expect("error while building tauri application");

    os::initialize(app.handle().clone());

    deep_link_support::set_app_handle(app.handle().clone());

    logging::set_app_handle(app.handle().clone());
    #[allow(unused_variables)]
    app.run(|app, event| match event {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        tauri::RunEvent::Opened { urls } => {
            route_opened_urls(app, urls);
        }
        tauri::RunEvent::ExitRequested { api, .. } => {
            if let Some(finalization) = begin_startup_request_handoff_finalization(app) {
                api.prevent_exit();
                match finalize_startup_request_handoff(app) {
                    Ok(()) if finalization.restart => app.restart(),
                    Ok(()) => app.exit(0),
                    Err(error) => {
                        match app
                            .state::<StartupRequestState>()
                            .handoff_finalization_failed()
                        {
                            StartupRequestHandoffFailure::Retry => {
                                log::error!(gui_toast = false; "failed to finalize startup request handoff on attempt {}/{}; retrying: {error}", finalization.attempt, STARTUP_REQUEST_HANDOFF_MAX_ATTEMPTS);
                                let app = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    tokio::time::sleep(STARTUP_REQUEST_HANDOFF_RETRY_DELAY).await;
                                    app.exit(0);
                                });
                            }
                            StartupRequestHandoffFailure::ContinueExit { restart } => {
                                log::error!(gui_toast = false; "failed to finalize startup request handoff after {} attempts; continuing with the pre-install snapshot: {error}", finalization.attempt);
                                if restart {
                                    app.restart();
                                } else {
                                    app.exit(0);
                                }
                            }
                        }
                    }
                }
            }
        }
        tauri::RunEvent::Exit => {
            if let Some(mcp) = app.try_state::<mcp::McpState>()
                && let Err(e) = tauri::async_runtime::block_on(mcp.shutdown(app))
            {
                log::error!("failed to shut down MCP IPC endpoint: {e}");
            }
        }
        _ => {}
    });
}

fn process_opened_urls(app: &AppHandle, urls: Vec<url::Url>) {
    let mut files = vec![];
    for url in urls {
        if let Ok(file) = url.to_file_path() {
            files.push(file)
        } else {
            deep_link_support::on_deep_link(url);
        }
    }
    deep_link_support::process_files(app, files);
}

fn route_startup_args(app: &AppHandle, args: &[String]) {
    let request = StartupRequest::Arguments(args.get(1..).unwrap_or_default().to_vec());
    if app.state::<StartupRequestState>().capture(request) {
        return;
    }
    process_args(app, args);
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn route_opened_urls(app: &AppHandle, urls: Vec<url::Url>) {
    let request = StartupRequest::OpenedUrls(urls.iter().map(ToString::to_string).collect());
    if app.state::<StartupRequestState>().capture(request) {
        return;
    }
    process_opened_urls(app, urls);
}

fn capture_initial_startup_args(app: &AppHandle, args: &[String]) {
    let request = StartupRequest::Arguments(args.get(1..).unwrap_or_default().to_vec());
    let captured = app.state::<StartupRequestState>().capture(request);
    debug_assert!(captured);
}

fn snapshot_startup_requests(app: &AppHandle) -> (u64, Vec<StartupRequest>) {
    app.state::<StartupRequestState>().snapshot()
}

fn complete_startup_request_handoff_finalization(app: &AppHandle, generation: u64) -> bool {
    app.state::<StartupRequestState>()
        .complete_handoff_finalization(generation)
}

fn arm_startup_request_handoff(app: &AppHandle) {
    app.state::<StartupRequestState>()
        .arm_handoff(!cfg!(windows));
}

fn disarm_startup_request_handoff(app: &AppHandle) {
    app.state::<StartupRequestState>().disarm_handoff();
}

fn begin_startup_request_handoff_finalization(
    app: &AppHandle,
) -> Option<StartupRequestHandoffFinalization> {
    app.state::<StartupRequestState>()
        .begin_handoff_finalization()
}

fn finalize_startup_request_handoff(app: &AppHandle) -> std::io::Result<()> {
    let io = app.state::<vrc_get_vpm::io::DefaultEnvironmentIo>();
    loop {
        let (generation, requests) = snapshot_startup_requests(app);
        tauri::async_runtime::block_on(deep_link_support::preserve_pending_startup_requests(
            io.inner(),
            &requests,
        ))?;
        if complete_startup_request_handoff_finalization(app, generation) {
            return Ok(());
        }
    }
}

fn finish_startup_request_capture(app: &AppHandle) -> Vec<StartupRequest> {
    app.state::<StartupRequestState>().finish_capture()
}

fn focus_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Err(e) = window.unminimize() {
        log::error!("error while unminimize: {e}");
    }
    if let Err(e) = window.set_focus() {
        log::error!("error while setting focus: {e}");
    }
}

fn process_startup_requests(app: &AppHandle, requests: Vec<StartupRequest>) {
    for request in requests {
        match request {
            StartupRequest::Arguments(arguments) => {
                let mut args = Vec::with_capacity(arguments.len() + 1);
                args.push(String::new());
                args.extend(arguments);
                process_args(app, &args);
            }
            StartupRequest::OpenedUrls(urls) => {
                let urls = urls
                    .into_iter()
                    .filter_map(|url| match url::Url::parse(&url) {
                        Ok(url) => Some(url),
                        Err(error) => {
                            log::error!("Invalid pending opened URL {url:?}: {error}");
                            None
                        }
                    })
                    .collect();
                process_opened_urls(app, urls);
            }
        }
    }
}

fn process_args(app: &AppHandle, args: &[String]) {
    if args.len() <= 1 {
        // no additional args
        return;
    }

    if args.len() == 2 {
        // we have a single argument. it might be a deep link
        let arg = &args[1];
        if is_deep_link(arg) {
            process_deep_link_string(app, arg);
            return;
        }
    }

    match args[1].as_str() {
        "link" => {
            let Some(url) = args.get(2) else {
                log::error!("link command requires a URL argument");
                return;
            };
            process_deep_link_string(app, url);
        }
        _ => {
            log::error!("Unknown command: {}", args[1]);
        }
    }

    fn is_deep_link(url: &str) -> bool {
        url.starts_with("vcc://") || url.ends_with(".alcomtemplate")
    }

    fn process_deep_link_string(app: &AppHandle, url: &str) {
        if let Some(url) = url::Url::parse(url)
            .ok()
            .take_if(|url| url.scheme() == "vcc")
        {
            deep_link_support::on_deep_link(url);
            return;
        }
        if std::fs::exists(url).unwrap_or(false) {
            deep_link_support::process_files(app, vec![PathBuf::from(url)]);
            return;
        }
        log::error!("Invalid deep link: {url}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_request_capture_deduplicates_all_request_sources() {
        let state = StartupRequestState::default();
        let arguments = StartupRequest::Arguments(vec!["request.alcomtemplate".to_string()]);
        let opened = StartupRequest::OpenedUrls(vec!["file:///request.alcomtemplate".to_string()]);

        assert!(state.capture(arguments.clone()));
        assert!(state.capture(arguments.clone()));
        assert!(state.capture(opened.clone()));
        let (generation, requests) = state.snapshot();

        assert_eq!(generation, 2);
        assert_eq!(requests, vec![arguments.clone(), opened.clone()]);
        assert_eq!(state.finish_capture(), vec![arguments, opened]);
        assert!(!state.capture(StartupRequest::Arguments(vec![
            "later.alcomtemplate".to_string()
        ])));
    }

    #[test]
    fn startup_request_handoff_can_retry_without_holding_the_request_lock() {
        let state = StartupRequestState::default();
        state.arm_handoff(true);

        assert_eq!(
            state.begin_handoff_finalization(),
            Some(StartupRequestHandoffFinalization {
                restart: true,
                attempt: 1
            })
        );
        assert_eq!(state.begin_handoff_finalization(), None);
        assert_eq!(
            state.handoff_finalization_failed(),
            StartupRequestHandoffFailure::Retry
        );
        assert_eq!(
            state.begin_handoff_finalization(),
            Some(StartupRequestHandoffFinalization {
                restart: true,
                attempt: 2
            })
        );
    }

    #[test]
    fn completed_startup_request_handoff_closes_memory_only_capture_atomically() {
        let state = StartupRequestState::default();
        state.arm_handoff(false);
        assert!(state.begin_handoff_finalization().is_some());

        let (generation, _) = state.snapshot();
        let late_request = StartupRequest::Arguments(vec!["late.alcomtemplate".to_string()]);
        assert!(state.capture(late_request));
        assert!(!state.complete_handoff_finalization(generation));

        let (generation, requests) = state.snapshot();
        assert_eq!(requests.len(), 1);
        assert!(state.complete_handoff_finalization(generation));
        assert!(!state.capture(StartupRequest::Arguments(vec![
            "after-finalization.alcomtemplate".to_string()
        ])));
        assert_eq!(state.begin_handoff_finalization(), None);
    }

    #[test]
    fn failed_install_disarms_startup_request_handoff() {
        let state = StartupRequestState::default();
        state.arm_handoff(false);
        state.disarm_handoff();

        assert_eq!(state.begin_handoff_finalization(), None);
    }

    #[test]
    fn startup_request_handoff_continues_exit_after_bounded_retries() {
        let state = StartupRequestState::default();
        state.arm_handoff(false);

        for attempt in 1..STARTUP_REQUEST_HANDOFF_MAX_ATTEMPTS {
            assert_eq!(state.begin_handoff_finalization().unwrap().attempt, attempt);
            assert_eq!(
                state.handoff_finalization_failed(),
                StartupRequestHandoffFailure::Retry
            );
        }

        assert_eq!(
            state.begin_handoff_finalization().unwrap().attempt,
            STARTUP_REQUEST_HANDOFF_MAX_ATTEMPTS
        );
        assert_eq!(
            state.handoff_finalization_failed(),
            StartupRequestHandoffFailure::ContinueExit { restart: false }
        );
        assert!(!state.capture(StartupRequest::Arguments(vec![
            "after-failed-finalization.alcomtemplate".to_string()
        ])));
        assert_eq!(state.begin_handoff_finalization(), None);
    }
}
