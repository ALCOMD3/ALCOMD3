//! Updater module
//!
//! This module reimplements the auto-update logic previously provided by
//! tauri-plugin-updater in order to fix several issues:
//! - macOS: Extract to a directory on the same filesystem as the app bundle to
//!   avoid cross-device rename errors when the app is installed on a non-default
//!   volume.
//! - Windows: Support custom installer types beyond NSIS and run them in a way
//!   that correctly triggers UAC elevation.
//! - Check-for-update-only mode: Because checking and installing are separate
//!   functions, callers can check without installing (useful when ALCOMD3 is
//!   managed by a package manager).
//!
//! This is based heavily on the tauri-plugin-updater source code.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use base64::Engine as _;
use futures::StreamExt as _;
use minisign_verify::{PublicKey, Signature};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{StatusCode, header};
use semver::Version;
use serde::{Deserialize, Deserializer, de::Error as DeError};
use tauri::{AppHandle, Env, Manager as _, Runtime};
use url::Url;
use uuid::Uuid;
use vrc_get_vpm::io::{DefaultEnvironmentIo, IoTrait};

// ---------------------------------------------------------------------------
// constants
// ---------------------------------------------------------------------------

static PUBLIC_KEY: &str = include_str!("updater-public-key.txt");

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum Error {
    FailedToDetermineExtractPath,
    BinaryNotFoundInArchive,
    TempDirNotOnSameMountPoint,
    Network(String),
    Signature(String),
    SignatureUtf8(String),
    InvalidBase64(base64::DecodeError),
    Json(serde_json::Error),
    Io(std::io::Error),
    Reqwest(reqwest::Error),
    Url(url::ParseError),
    InvalidStagedUpdate(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedToDetermineExtractPath => {
                write!(f, "failed to determine extract path")
            }
            Self::BinaryNotFoundInArchive => write!(f, "binary not found in archive"),
            Self::TempDirNotOnSameMountPoint => {
                write!(f, "temp dir not on same mount point")
            }
            Self::Network(s) => write!(f, "network error: {s}"),
            Self::Signature(s) => write!(f, "signature error: {s}"),
            Self::SignatureUtf8(s) => write!(f, "signature utf8 error: {s}"),
            Self::InvalidBase64(e) => write!(f, "base64 decode error: {e}"),
            Self::Json(e) => write!(f, "json error: {e}"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Reqwest(e) => write!(f, "reqwest error: {e}"),
            Self::Url(e) => write!(f, "url parse error: {e}"),
            Self::InvalidStagedUpdate(e) => write!(f, "invalid staged update: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<base64::DecodeError> for Error {
    fn from(e: base64::DecodeError) -> Self {
        Self::InvalidBase64(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Self::Reqwest(e)
    }
}

impl From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Self {
        Self::Url(e)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

// ---------------------------------------------------------------------------
// Signature verification
// ---------------------------------------------------------------------------

fn verify_signature(
    data: &[u8],
    release_signature: &str,
    pub_key: &str,
    expected_file_name: &str,
) -> Result<bool> {
    if std::env::var(
        "___ALCOMD3_UPDATER_DISABLE_SIGNATURE_VERIFICATION_DEBUG_ONLY_FEATURE_DO_NOT_USE_THIS_OR_YOU_WILL_BE_HACKED___",
    )
    .as_deref()
        == Ok("YES_I_WANT_TO_BE_HACKED")
    {
        return Ok(true);
    }
    let pub_key_decoded = base64_to_string(pub_key)?;
    let public_key =
        PublicKey::decode(&pub_key_decoded).map_err(|e| Error::Signature(e.to_string()))?;
    let sig_decoded = base64_to_string(release_signature)?;
    let signature = Signature::decode(&sig_decoded).map_err(|e| Error::Signature(e.to_string()))?;
    public_key
        .verify(data, &signature, true)
        .map_err(|e| Error::Signature(e.to_string()))?;
    validate_trusted_comment(signature.trusted_comment(), expected_file_name)?;
    Ok(true)
}

fn validate_trusted_comment(trusted_comment: &str, expected_file_name: &str) -> Result<()> {
    if trusted_comment_value(trusted_comment, "file") != Some(expected_file_name) {
        return Err(Error::Signature(
            "trusted comment does not match the update asset name".to_string(),
        ));
    }
    if trusted_comment_value(trusted_comment, "purpose") != Some("release") {
        return Err(Error::Signature(
            "trusted comment does not identify a release update".to_string(),
        ));
    }

    Ok(())
}

fn trusted_comment_value<'a>(trusted_comment: &'a str, key: &str) -> Option<&'a str> {
    trusted_comment
        .split('\t')
        .filter_map(|field| field.split_once(':'))
        .find_map(|(field_key, value)| (field_key == key).then_some(value))
}

fn updater_public_key() -> &'static str {
    PUBLIC_KEY.trim()
}

fn base64_to_string(base64_string: &str) -> Result<String> {
    let decoded = base64::engine::general_purpose::STANDARD.decode(base64_string)?;
    std::str::from_utf8(&decoded)
        .map(|s| s.to_string())
        .map_err(|_| Error::SignatureUtf8(base64_string.into()))
}

// ---------------------------------------------------------------------------
// Remote release structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseManifestPlatform {
    pub url: Url,
    pub signature: String,
}

#[derive(Deserialize)]
struct RemoteRelease {
    #[serde(alias = "name", deserialize_with = "parse_version")]
    version: Version,
    notes: Option<String>,
    notes_i18n: Option<HashMap<String, String>>,
    platforms: HashMap<String, ReleaseManifestPlatform>,
}

fn parse_version<'de, D>(deserializer: D) -> std::result::Result<Version, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;

    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Version;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a semver version")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Version::from_str(v.trim_start_matches('v'))
                .map_err(|_| DeError::invalid_value(serde::de::Unexpected::Str(v), &self))
        }
    }

    deserializer.deserialize_str(Visitor)
}

// ---------------------------------------------------------------------------
// OS / arch helpers  (always compiled for all targets – use cfg!())
// ---------------------------------------------------------------------------

fn updater_os() -> Option<&'static str> {
    if cfg!(target_os = "linux") {
        Some("linux")
    } else if cfg!(target_os = "macos") {
        Some("darwin")
    } else if cfg!(target_os = "windows") {
        Some("windows")
    } else {
        None
    }
}

fn updater_arch() -> Option<&'static str> {
    if cfg!(target_arch = "x86") {
        Some("i686")
    } else if cfg!(target_arch = "x86_64") {
        Some("x86_64")
    } else if cfg!(target_arch = "arm") {
        Some("armv7")
    } else if cfg!(target_arch = "aarch64") {
        Some("aarch64")
    } else if cfg!(target_arch = "riscv64") {
        Some("riscv64")
    } else {
        None
    }
}

/// Determine the "extract path" – the path that the updater should replace.
///
/// - Linux: the AppImage binary itself.
/// - macOS: the `.app` bundle (2 parents above the binary).
/// - Windows: the directory containing the binary.
pub fn extract_path_from_executable(executable_path: &std::path::Path) -> Option<&std::path::Path> {
    if cfg!(target_os = "linux") {
        return Some(executable_path);
    }
    if cfg!(target_os = "macos") {
        let macos = executable_path.parent()?;
        if macos.ends_with("Contents/MacOS") {
            return Some(macos.parent().unwrap().parent().unwrap());
        }
        return None;
    }
    if cfg!(target_os = "windows") {
        let extract_path = executable_path.parent()?;
        return Some(extract_path);
    }
    None
}

// ---------------------------------------------------------------------------
// check_for_update
// ---------------------------------------------------------------------------

/// Check whether a newer version is available at `endpoint`.
///
/// Returns `Ok(None)` when the current version is already up to date.
pub async fn check_for_update<R: Runtime>(
    app: &AppHandle<R>,
    endpoint: Url,
) -> Result<Option<Update>> {
    let current_version = app.package_info().version.clone();

    // Build URL with template variables replaced
    let url: Url = endpoint;

    log::debug!("checking for updates: {url}");

    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        "X-Alcom-Version",
        HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
    );
    headers.insert(
        "X-Alcom-OS",
        HeaderValue::from_static(updater_os().unwrap_or("unknown")),
    );
    headers.insert(
        "X-Alcom-Arch",
        HeaderValue::from_static(updater_arch().unwrap_or("unknown")),
    );

    let client = app.state::<reqwest::Client>();

    let response = client.get(url).headers(headers).send().await.map_err(|e| {
        log::error!(gui_toast = false; "failed to check for updates: {e}");
        Error::Reqwest(e)
    })?;

    if StatusCode::NO_CONTENT == response.status() {
        log::debug!("no update available (204 No Content)");
        return Ok(None);
    }

    if !response.status().is_success() {
        let status = response.status();
        log::error!(gui_toast = false; "update endpoint returned {status}");
        return Err(Error::Network(format!(
            "update endpoint returned status {status}"
        )));
    }

    let release: RemoteRelease = response.json().await?;
    log::debug!("parsed release version: {}", release.version);

    let should_update = release.version > current_version;
    if !should_update {
        return Ok(None);
    }

    let updater = updater_information(&app.env(), &client, &release);
    let updater_status = updater
        .as_ref()
        .err()
        .copied()
        .unwrap_or(UpdaterStatus::Updatable);

    Ok(Some(Update {
        current_version: current_version.to_string(),
        version: release.version.to_string(),
        body: release.notes.clone(),
        body_i18n: release.notes_i18n.clone(),
        updater_status,
        updater: updater.ok(),
    }))
}

fn updater_information(
    env: &Env,
    client: &reqwest::Client,
    release: &RemoteRelease,
) -> Result<UpdaterInformation, UpdaterStatus> {
    if cfg!(feature = "no-self-updater") {
        return Err(UpdaterStatus::UpdaterDisabled);
    }

    let arch = updater_arch().ok_or(UpdaterStatus::NoPlatform)?;
    let os = updater_os().ok_or(UpdaterStatus::NoPlatform)?;

    let platform = (release.platforms.get(&format!("{os}-{arch}")).cloned())
        .ok_or(UpdaterStatus::NoPlatform)?;
    let version = release.version.to_string();
    let expected_asset_name =
        crate::alcomd3_config::updater_asset_name(&version).ok_or(UpdaterStatus::NoPlatform)?;
    let max_download_bytes = crate::alcomd3_config::updater_max_download_bytes()
        .filter(|limit| *limit > 0)
        .ok_or(UpdaterStatus::NoPlatform)?;
    let args = crate::alcomd3_config::updater_args()
        .ok_or(UpdaterStatus::NoPlatform)?
        .to_vec();

    installer_information(env, args)?;

    Ok(UpdaterInformation {
        client: client.clone(),
        url: platform.url,
        signature: platform.signature,
        expected_asset_name,
        max_download_bytes,
    })
}

fn installer_information(
    env: &Env,
    args: Vec<String>,
) -> Result<InstallerInformation, UpdaterStatus> {
    if cfg!(feature = "no-self-updater") {
        return Err(UpdaterStatus::UpdaterDisabled);
    }

    let executable_path = current_exe(env).ok_or(UpdaterStatus::NotUpdatable)?;
    let executable_path = try_read_link(executable_path);
    let extract_path =
        extract_path_from_executable(&executable_path).ok_or(UpdaterStatus::NotUpdatable)?;

    fn current_exe(_env: &Env) -> Option<PathBuf> {
        #[cfg(target_os = "linux")]
        {
            _env.appimage.as_ref().map(PathBuf::from)
        }
        #[cfg(not(target_os = "linux"))]
        {
            std::env::current_exe().ok()
        }
    }

    #[allow(unused_mut)]
    let mut current_install = CurrentInstallMode::Unrelated;

    if cfg!(windows) {
        // This version of ALCOMD3 is installed with Inno Setup.
        let current_user = find_install(true).map(PathBuf::from).map(try_read_link);
        let local_machine = find_install(false).map(PathBuf::from).map(try_read_link);

        if current_user.as_deref() == Some(extract_path) {
            current_install = CurrentInstallMode::UserInstall
        } else if local_machine.as_deref() == Some(extract_path) {
            current_install = CurrentInstallMode::MachineInstall
        } else {
            // None of two installation path matches current installation
            return Err(UpdaterStatus::NotUpdatable);
        }

        #[cfg(not(windows))]
        fn find_install(_is_user: bool) -> Option<OsString> {
            None
        }

        #[cfg(windows)]
        fn find_install(is_user: bool) -> Option<OsString> {
            use winreg::enums::HKEY_CURRENT_USER;
            use winreg::enums::HKEY_LOCAL_MACHINE;

            static REG_VALUE: &str = "Inno Setup: App Path";
            let reg_key = format!(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}_is1",
                crate::alcomd3_config::windows_app_id()
            );

            let root = if is_user {
                HKEY_CURRENT_USER
            } else {
                HKEY_LOCAL_MACHINE
            };

            winreg::RegKey::predef(root)
                .open_subkey(reg_key)
                .ok()?
                .get_value(REG_VALUE)
                .ok()
        }
    }

    return Ok(InstallerInformation {
        args,
        extract_path: extract_path.to_path_buf(),
        current_install,
    });

    fn try_read_link(path: PathBuf) -> PathBuf {
        std::fs::read_link(&path).unwrap_or(path)
    }
}

// ---------------------------------------------------------------------------
// Update – public handle returned by check_for_update
// ---------------------------------------------------------------------------

/// Represents an available update that can be downloaded and installed.
#[derive(Clone)]
pub struct Update {
    /// The version currently installed.
    pub current_version: String,
    /// The version available for download.
    pub version: String,
    /// Release notes from the update server.
    pub body: Option<String>,
    /// Localized release notes from the update server.
    pub body_i18n: Option<HashMap<String, String>>,
    /// The status of the updater describes if auto update is possible, or reason why impossible
    /// if auto update is not possible.
    pub updater_status: UpdaterStatus,
    /// The information for updating application. only available if updater_status is Updatable
    pub updater: Option<UpdaterInformation>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize, specta::Type)]
pub enum UpdaterStatus {
    // NoUpdate: Expressed as None
    /// Update is found and can be updated automatically. UpdaterInformation is available
    ///
    /// User will proceed update.
    Updatable,
    /// Update is found, but installer or package for current architecture does not found.
    /// This can happen if platform support is removed.
    /// x86_64 macOS will become this state in near future, but other platforms may if new arch is expanded enough.
    ///
    /// Inform only
    NoPlatform,
    /// Update is found and installer is found, but current installation is different from
    /// the previous (detected) installation, or we failed to detect current installation path.
    ///
    /// Inform user to install update manually to prevent problem.
    NotUpdatable,
    /// Updater is disabled at build time. generally the installation is managed by package manager.
    ///
    /// Inform user to upgrade through package manager.
    /// Packager may customize information message by defining
    /// `VRC_GET_GUI_UPDATER_UPDATE_SUGGESTION_MESSAGE` environment variable at build time.
    UpdaterDisabled,
}

#[derive(Debug, Clone)]
pub struct UpdaterInformation {
    client: reqwest::Client,
    url: Url,
    signature: String,
    expected_asset_name: String,
    max_download_bytes: u64,
}

#[derive(Debug, Clone)]
struct InstallerInformation {
    args: Vec<String>,
    /// Path to replace during installation (app bundle on macOS, binary on Linux).
    extract_path: PathBuf,
    #[allow(dead_code)] // no meaning on non-windows
    current_install: CurrentInstallMode,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[allow(dead_code)] // no meaning on non-windows
enum CurrentInstallMode {
    Unrelated, // Non windows
    UserInstall,
    MachineInstall,
}

impl UpdaterInformation {
    async fn download(&self, on_chunk: &mut impl FnMut(usize, Option<u64>)) -> Result<Vec<u8>> {
        validate_update_asset_name(&self.url, &self.expected_asset_name)?;

        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/octet-stream"),
        );

        let response = (self.client)
            .get(self.url.clone())
            .headers(headers)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::Network(format!(
                "download request failed with status: {}",
                response.status()
            )));
        }

        let content_length = parse_content_length(response.headers())?;
        validate_download_size(content_length, 0, self.max_download_bytes)?;

        let mut buffer = Vec::new();
        let mut downloaded_bytes = 0u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded_bytes = downloaded_bytes
                .checked_add(u64::try_from(chunk.len()).map_err(|_| {
                    Error::Network("update download size does not fit in u64".to_string())
                })?)
                .ok_or_else(|| Error::Network("update download size overflowed".to_string()))?;
            validate_download_size(content_length, downloaded_bytes, self.max_download_bytes)?;
            on_chunk(chunk.len(), content_length);
            buffer.extend_from_slice(&chunk);
        }

        verify_signature(
            &buffer,
            &self.signature,
            updater_public_key(),
            &self.expected_asset_name,
        )?;

        Ok(buffer)
    }
}

fn parse_content_length(headers: &HeaderMap) -> Result<Option<u64>> {
    let mut values = headers.get_all(header::CONTENT_LENGTH).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    let expected = value
        .to_str()
        .map_err(|_| Error::Network("update Content-Length is not valid ASCII".to_string()))?
        .parse()
        .map_err(|_| {
            Error::Network("update Content-Length is not a valid byte count".to_string())
        })?;
    for value in values {
        let actual = value
            .to_str()
            .map_err(|_| Error::Network("update Content-Length is not valid ASCII".to_string()))?
            .parse::<u64>()
            .map_err(|_| {
                Error::Network("update Content-Length is not a valid byte count".to_string())
            })?;
        if actual != expected {
            return Err(Error::Network(
                "update response contains conflicting Content-Length values".to_string(),
            ));
        }
    }
    Ok(Some(expected))
}

fn validate_update_asset_name(url: &Url, expected_asset_name: &str) -> Result<()> {
    let actual_asset_name = asset_name_from_url(url)?;
    if actual_asset_name != expected_asset_name {
        return Err(Error::Network(format!(
            "update asset {actual_asset_name:?} does not match configured asset {expected_asset_name:?}"
        )));
    }
    Ok(())
}

fn validate_download_size(
    content_length: Option<u64>,
    downloaded_bytes: u64,
    max_download_bytes: u64,
) -> Result<()> {
    if content_length.is_some_and(|length| length > max_download_bytes) {
        return Err(Error::Network(format!(
            "update Content-Length exceeds the configured {max_download_bytes}-byte limit"
        )));
    }
    if downloaded_bytes > max_download_bytes {
        return Err(Error::Network(format!(
            "update download exceeds the configured {max_download_bytes}-byte limit"
        )));
    }
    Ok(())
}

fn asset_name_from_url(url: &Url) -> Result<&str> {
    url.path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .ok_or_else(|| Error::Network("update URL does not contain an asset name".to_string()))
}

impl InstallerInformation {
    // ------------------------------------------------------------------
    // install_inner – dispatches by OS using cfg!() rather than #[cfg]
    // ------------------------------------------------------------------

    fn install_inner_for_startup(&self, bytes: &[u8]) -> Result<()> {
        self.install_inner_with_windows_exit(bytes, false)
    }

    fn install_inner_with_windows_exit(
        &self,
        bytes: &[u8],
        exit_after_windows_launch: bool,
    ) -> Result<()> {
        if cfg!(feature = "no-self-updater") {
            panic!("updater is disabled")
        }
        if cfg!(target_os = "macos") {
            self.install_macos(bytes)
        } else if cfg!(windows) {
            self.install_windows(bytes, exit_after_windows_launch)
        } else if cfg!(target_os = "linux") {
            self.install_linux(bytes)
        } else {
            panic!("Unsupported OS")
        }
    }
}

const STAGED_UPDATE_SCHEMA_VERSION: u32 = 1;
const FAILED_AUTOMATIC_UPDATE_SCHEMA_VERSION: u32 = 1;
static STAGED_UPDATE_WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StagedUpdate {
    schema_version: u32,
    version: String,
    channel: String,
    package_id: String,
    signature: String,
}

#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FailedAutomaticUpdate {
    schema_version: u32,
    version: String,
    channel: String,
}

pub struct DownloadedUpdate {
    version: String,
    signature: String,
    bytes: Vec<u8>,
}

pub struct StagedUpdateReceipt {
    package_id: Uuid,
    version: Version,
    channel: String,
}

impl Update {
    pub async fn download_for_staging(
        self,
        mut on_chunk: impl FnMut(usize, Option<u64>),
    ) -> Result<DownloadedUpdate> {
        let updater = self.updater.ok_or_else(|| {
            Error::InvalidStagedUpdate(
                "the available update cannot be installed automatically".to_string(),
            )
        })?;
        let signature = updater.signature.clone();
        let bytes = updater.download(&mut on_chunk).await?;

        Ok(DownloadedUpdate {
            version: self.version,
            signature,
            bytes,
        })
    }
}

impl DownloadedUpdate {
    pub async fn persist(
        self,
        io: &DefaultEnvironmentIo,
        channel: &str,
    ) -> Result<StagedUpdateReceipt> {
        let _guard = STAGED_UPDATE_WRITE_LOCK.lock().await;
        self.persist_locked(io, channel).await
    }

    async fn persist_locked(
        self,
        io: &DefaultEnvironmentIo,
        channel: &str,
    ) -> Result<StagedUpdateReceipt> {
        validate_channel(channel)?;
        let version = Version::parse(&self.version).map_err(|error| {
            Error::InvalidStagedUpdate(format!("invalid update version: {error}"))
        })?;
        validate_version_for_channel(&version, channel)?;

        io.create_dir_all(Path::new(crate::storage::AUTOMATIC_UPDATER_PACKAGES_DIR))
            .await?;
        cleanup_orphaned_staged_packages_locked(io).await?;

        let previous = read_staged_update(io).await.ok().flatten();
        if let Some(previous) = previous.as_ref()
            && let Some(previous_version) = effective_staged_version(io, previous).await
            && version < previous_version
        {
            return Err(Error::InvalidStagedUpdate(format!(
                "update version {version} cannot replace newer staged version {previous_version}"
            )));
        }
        let package_id = Uuid::new_v4();
        let staged = StagedUpdate {
            schema_version: STAGED_UPDATE_SCHEMA_VERSION,
            version: self.version,
            channel: channel.to_string(),
            package_id: package_id.to_string(),
            signature: self.signature,
        };
        let package_path = staged_package_path(&staged)?;
        io.write_atomic(&package_path, &self.bytes).await?;

        let metadata = serde_json::to_vec_pretty(&staged)?;
        if let Err(error) = io
            .write_atomic(
                Path::new(crate::storage::AUTOMATIC_UPDATER_PENDING_PATH),
                &metadata,
            )
            .await
        {
            if let Err(cleanup_error) = remove_file_if_exists(io, &package_path).await {
                log::warn!(gui_toast = false; "failed to clean up an incomplete staged update package: {cleanup_error}");
            }
            return Err(error.into());
        }

        if let Err(error) = cleanup_orphaned_staged_packages_locked(io).await {
            log::warn!(gui_toast = false; "failed to clean up superseded staged update packages: {error}");
        }
        if let Err(error) = discard_failed_automatic_update_locked(io).await {
            log::warn!(gui_toast = false; "failed to clear the previous automatic update failure: {error}");
        }

        Ok(StagedUpdateReceipt {
            package_id,
            version,
            channel: channel.to_string(),
        })
    }
}

#[cfg(test)]
async fn staged_update_exists(io: &DefaultEnvironmentIo) -> bool {
    let _guard = STAGED_UPDATE_WRITE_LOCK.lock().await;
    effective_staged_update_locked(io).await.is_some()
}

pub async fn staged_update_satisfies(
    io: &DefaultEnvironmentIo,
    expected_channel: &str,
    minimum_version: &str,
) -> bool {
    if let Err(error) = validate_channel(expected_channel) {
        log::debug!(gui_toast = false; "cannot match a staged update to an invalid channel: {error}");
        return false;
    }
    let minimum_version = match Version::parse(minimum_version) {
        Ok(version) => version,
        Err(error) => {
            log::debug!(gui_toast = false; "cannot match a staged update to an invalid version: {error}");
            return false;
        }
    };

    let _guard = STAGED_UPDATE_WRITE_LOCK.lock().await;
    effective_staged_update_locked(io)
        .await
        .is_some_and(|(staged, staged_version)| {
            staged.channel == expected_channel && staged_version >= minimum_version
        })
}

pub async fn automatic_update_installation_failed(
    io: &DefaultEnvironmentIo,
    expected_channel: &str,
    expected_version: &str,
) -> bool {
    if validate_channel(expected_channel).is_err() {
        return false;
    }
    let Ok(expected_version) = Version::parse(expected_version) else {
        return false;
    };

    let _guard = STAGED_UPDATE_WRITE_LOCK.lock().await;
    match failed_automatic_update_matches_locked(io, expected_channel, &expected_version).await {
        Ok(failed) => failed,
        Err(error) => {
            log::warn!(gui_toast = false; "failed to inspect automatic update failure state; suppressing automatic installation until it can be read: {error}");
            true
        }
    }
}

async fn failed_automatic_update_matches_locked(
    io: &DefaultEnvironmentIo,
    expected_channel: &str,
    expected_version: &Version,
) -> Result<bool> {
    match read_failed_automatic_update(io).await {
        Ok(Some(failed)) => Ok(
            failed.schema_version == FAILED_AUTOMATIC_UPDATE_SCHEMA_VERSION
                && failed.channel == expected_channel
                && Version::parse(&failed.version)
                    .is_ok_and(|version| version == *expected_version),
        ),
        Ok(None) => Ok(false),
        Err(Error::Json(error)) => {
            log::warn!(gui_toast = false; "discarding invalid automatic update failure state: {error}");
            if let Err(error) = discard_failed_automatic_update_locked(io).await {
                log::warn!(gui_toast = false; "failed to discard invalid automatic update failure state: {error}");
            }
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

pub async fn clear_failed_automatic_update(io: &DefaultEnvironmentIo) -> Result<()> {
    let _guard = STAGED_UPDATE_WRITE_LOCK.lock().await;
    discard_failed_automatic_update_locked(io).await
}

async fn effective_staged_update_locked(
    io: &DefaultEnvironmentIo,
) -> Option<(StagedUpdate, Version)> {
    if let Err(error) = cleanup_orphaned_staged_packages_locked(io).await {
        log::warn!(gui_toast = false; "failed to clean up orphaned staged update packages: {error}");
    }

    let effective = match read_staged_update(io).await {
        Ok(Some(staged)) => effective_staged_version(io, &staged)
            .await
            .map(|version| (staged, version)),
        Ok(None) => None,
        Err(error) => {
            log::warn!(gui_toast = false; "failed to read staged update metadata: {error}");
            None
        }
    };
    if effective.is_none()
        && let Err(error) = discard_staged_update_locked(io).await
    {
        log::warn!(gui_toast = false; "failed to discard invalid staged update metadata: {error}");
    }
    effective
}

pub async fn discard_staged_update(io: &DefaultEnvironmentIo) -> Result<()> {
    let _guard = STAGED_UPDATE_WRITE_LOCK.lock().await;
    discard_staged_update_locked(io).await
}

pub async fn discard_staged_update_if_matches(
    io: &DefaultEnvironmentIo,
    receipt: StagedUpdateReceipt,
) -> Result<bool> {
    let _guard = STAGED_UPDATE_WRITE_LOCK.lock().await;
    let Some(staged) = read_staged_update(io).await? else {
        return Ok(false);
    };
    if !receipt.matches(&staged) {
        return Ok(false);
    }
    discard_staged_update_locked(io).await?;
    Ok(true)
}

impl StagedUpdateReceipt {
    fn matches(&self, staged: &StagedUpdate) -> bool {
        staged.schema_version == STAGED_UPDATE_SCHEMA_VERSION
            && Uuid::parse_str(&staged.package_id)
                .is_ok_and(|package_id| package_id == self.package_id)
            && Version::parse(&staged.version).is_ok_and(|version| version == self.version)
            && staged.channel == self.channel
    }
}

async fn discard_staged_update_locked(io: &DefaultEnvironmentIo) -> Result<()> {
    remove_file_if_exists(
        io,
        Path::new(crate::storage::AUTOMATIC_UPDATER_PENDING_PATH),
    )
    .await?;
    cleanup_orphaned_staged_packages_locked(io).await
}

pub async fn install_staged_update(
    app: &AppHandle,
    io: &DefaultEnvironmentIo,
    expected_channel: &str,
) -> Result<bool> {
    let _guard = STAGED_UPDATE_WRITE_LOCK.lock().await;
    cleanup_orphaned_staged_packages_locked(io).await?;

    if cfg!(feature = "no-self-updater") {
        discard_staged_update_locked(io).await?;
        return Ok(false);
    }

    validate_channel(expected_channel)?;
    let staged = match read_staged_update(io).await {
        Ok(Some(staged)) => staged,
        Ok(None) => return Ok(false),
        Err(error) => {
            discard_staged_update_locked(io).await?;
            return Err(error);
        }
    };

    let staged_version = match Version::parse(&staged.version) {
        Ok(version) => version,
        Err(error) => {
            discard_staged_update_locked(io).await?;
            return Err(Error::InvalidStagedUpdate(format!(
                "invalid staged version: {error}"
            )));
        }
    };
    if let Err(error) = validate_version_for_channel(&staged_version, expected_channel) {
        discard_staged_update_locked(io).await?;
        return Err(error);
    }
    if staged.schema_version != STAGED_UPDATE_SCHEMA_VERSION
        || staged.channel != expected_channel
        || staged_version <= app.package_info().version
    {
        discard_staged_update_locked(io).await?;
        return Ok(false);
    }

    match failed_automatic_update_matches_locked(io, expected_channel, &staged_version).await {
        Ok(true) => {
            discard_staged_update_locked(io).await?;
            return Ok(false);
        }
        Ok(false) => {}
        Err(error) => {
            log::warn!(gui_toast = false; "deferring staged automatic update because the failure state cannot be read: {error}");
            return Err(error);
        }
    }

    let expected_asset_name = match crate::alcomd3_config::updater_asset_name(&staged.version) {
        Some(asset_name) => asset_name,
        None => {
            discard_staged_update_locked(io).await?;
            return Err(Error::InvalidStagedUpdate(
                "the current platform has no configured updater asset".to_string(),
            ));
        }
    };
    let package_path = match staged_package_path(&staged) {
        Ok(path) => path,
        Err(error) => {
            discard_staged_update_locked(io).await?;
            return Err(error);
        }
    };
    let max_download_bytes = match crate::alcomd3_config::updater_max_download_bytes() {
        Some(limit) if limit > 0 => limit,
        _ => {
            discard_staged_update_locked(io).await?;
            return Err(Error::InvalidStagedUpdate(
                "the current platform has no configured updater download limit".to_string(),
            ));
        }
    };
    let bytes = match read_staged_package(io, &package_path, max_download_bytes).await {
        Ok(bytes) => bytes,
        Err(error) => {
            discard_staged_update_locked(io).await?;
            return Err(error);
        }
    };
    if let Err(error) = verify_signature(
        &bytes,
        &staged.signature,
        updater_public_key(),
        &expected_asset_name,
    ) {
        discard_staged_update_locked(io).await?;
        return Err(error);
    }

    let args = match crate::alcomd3_config::updater_args() {
        Some(args) => args.to_vec(),
        None => {
            let error = Error::InvalidStagedUpdate(
                "the current platform has no configured updater arguments".to_string(),
            );
            reject_staged_automatic_update_locked(io, &staged.version, expected_channel, &error)
                .await;
            return Err(error);
        }
    };
    let installer = match installer_information(&app.env(), args) {
        Ok(installer) => installer,
        Err(status) => {
            let error = Error::InvalidStagedUpdate(format!(
                "the staged update cannot be installed: {status:?}"
            ));
            reject_staged_automatic_update_locked(io, &staged.version, expected_channel, &error)
                .await;
            return Err(error);
        }
    };

    crate::arm_startup_request_handoff(app);
    if let Err(error) = installer.install_inner_for_startup(&bytes) {
        crate::disarm_startup_request_handoff(app);
        if automatic_installation_error_is_transient(&error) {
            log::warn!(gui_toast = false; "preserving staged automatic update after transient installation failure: {error}");
        } else {
            reject_staged_automatic_update_locked(io, &staged.version, expected_channel, &error)
                .await;
        }
        return Err(error);
    }
    if let Err(error) = discard_staged_update_locked(io).await {
        log::error!(gui_toast = false; "failed to discard installed staged update: {error}");
    }

    Ok(true)
}

fn automatic_installation_error_is_transient(error: &Error) -> bool {
    match error {
        Error::Io(error) => !matches!(
            error.kind(),
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof
        ),
        _ => false,
    }
}

async fn read_staged_update(io: &DefaultEnvironmentIo) -> Result<Option<StagedUpdate>> {
    let path = io.resolve(Path::new(crate::storage::AUTOMATIC_UPDATER_PENDING_PATH));
    match tokio::fs::read(path).await {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn read_failed_automatic_update(
    io: &DefaultEnvironmentIo,
) -> Result<Option<FailedAutomaticUpdate>> {
    let path = io.resolve(Path::new(crate::storage::AUTOMATIC_UPDATER_FAILED_PATH));
    match tokio::fs::read(path).await {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn record_failed_automatic_update_locked(
    io: &DefaultEnvironmentIo,
    version: &str,
    channel: &str,
) -> Result<()> {
    let failed = FailedAutomaticUpdate {
        schema_version: FAILED_AUTOMATIC_UPDATE_SCHEMA_VERSION,
        version: version.to_string(),
        channel: channel.to_string(),
    };
    let path = Path::new(crate::storage::AUTOMATIC_UPDATER_FAILED_PATH);
    if let Some(parent) = path.parent() {
        io.create_dir_all(parent).await?;
    }
    io.write_atomic(path, &serde_json::to_vec_pretty(&failed)?)
        .await?;
    Ok(())
}

async fn reject_staged_automatic_update_locked(
    io: &DefaultEnvironmentIo,
    version: &str,
    channel: &str,
    original_error: &Error,
) {
    if let Err(error) = record_failed_automatic_update_locked(io, version, channel).await {
        log::error!(gui_toast = false; "failed to record rejected automatic update after {original_error}: {error}");
    }
    if let Err(error) = discard_staged_update_locked(io).await {
        log::error!(gui_toast = false; "failed to discard rejected automatic update after {original_error}: {error}");
    }
}

async fn discard_failed_automatic_update_locked(io: &DefaultEnvironmentIo) -> Result<()> {
    remove_file_if_exists(io, Path::new(crate::storage::AUTOMATIC_UPDATER_FAILED_PATH)).await
}

async fn read_staged_package(
    io: &DefaultEnvironmentIo,
    package_path: &Path,
    max_download_bytes: u64,
) -> Result<Vec<u8>> {
    let resolved = io.resolve(package_path);
    let metadata = tokio::fs::metadata(&resolved).await?;
    validate_download_size(Some(metadata.len()), 0, max_download_bytes)?;
    let bytes = tokio::fs::read(resolved).await?;
    validate_download_size(
        None,
        u64::try_from(bytes.len()).map_err(|_| {
            Error::InvalidStagedUpdate("staged package size does not fit in u64".to_string())
        })?,
        max_download_bytes,
    )?;
    Ok(bytes)
}

async fn effective_staged_version(
    io: &DefaultEnvironmentIo,
    staged: &StagedUpdate,
) -> Option<Version> {
    let package_path = referenced_staged_package_path(staged)?;
    let metadata = tokio::fs::metadata(io.resolve(&package_path)).await.ok()?;
    metadata
        .is_file()
        .then(|| Version::parse(&staged.version).ok())
        .flatten()
}

fn referenced_staged_package_path(staged: &StagedUpdate) -> Option<PathBuf> {
    if staged.schema_version != STAGED_UPDATE_SCHEMA_VERSION || staged.signature.trim().is_empty() {
        return None;
    }
    let version = Version::parse(&staged.version).ok()?;
    validate_version_for_channel(&version, &staged.channel).ok()?;
    staged_package_path(staged).ok()
}

async fn cleanup_orphaned_staged_packages_locked(io: &DefaultEnvironmentIo) -> Result<()> {
    let referenced_package = match read_staged_update(io).await {
        Ok(Some(staged)) => referenced_staged_package_path(&staged),
        Ok(None) | Err(Error::Json(_)) => None,
        Err(error) => return Err(error),
    };
    let packages_dir = Path::new(crate::storage::AUTOMATIC_UPDATER_PACKAGES_DIR);
    let mut entries = match tokio::fs::read_dir(io.resolve(packages_dir)).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        if !is_staged_package_artifact(&file_name) {
            continue;
        }
        let relative_path = packages_dir.join(file_name);
        if referenced_package.as_deref() == Some(relative_path.as_path()) {
            continue;
        }
        if entry.file_type().await?.is_dir() {
            continue;
        }
        remove_file_if_exists(io, &relative_path).await?;
    }

    Ok(())
}

fn is_staged_package_artifact(file_name: &OsStr) -> bool {
    let path = Path::new(file_name);
    if path.extension() == Some(OsStr::new("bin")) {
        return true;
    }

    let Some(file_name) = file_name.to_str() else {
        return false;
    };
    let Some((package_id, temporary_index)) = file_name.split_once(".bin.temp.") else {
        return false;
    };
    Uuid::parse_str(package_id).is_ok()
        && !temporary_index.is_empty()
        && temporary_index.bytes().all(|byte| byte.is_ascii_digit())
}

fn staged_package_path(staged: &StagedUpdate) -> Result<PathBuf> {
    let package_id = Uuid::parse_str(&staged.package_id).map_err(|error| {
        Error::InvalidStagedUpdate(format!("invalid staged package id: {error}"))
    })?;
    Ok(Path::new(crate::storage::AUTOMATIC_UPDATER_PACKAGES_DIR).join(format!("{package_id}.bin")))
}

async fn remove_file_if_exists(io: &DefaultEnvironmentIo, path: &Path) -> Result<()> {
    match io.remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_channel(channel: &str) -> Result<()> {
    if matches!(channel, "stable" | "beta") {
        Ok(())
    } else {
        Err(Error::InvalidStagedUpdate(format!(
            "unsupported update channel {channel:?}"
        )))
    }
}

fn validate_version_for_channel(version: &Version, channel: &str) -> Result<()> {
    validate_channel(channel)?;
    if channel == "stable" && !version.pre.is_empty() {
        return Err(Error::InvalidStagedUpdate(
            "a prerelease package cannot be staged for the stable channel".to_string(),
        ));
    }
    Ok(())
}

pub(crate) mod macos {
    use super::unix::*;
    use super::*;
    use flate2::read::GzDecoder;
    use sha2::Digest;
    use std::ffi::{OsStr, OsString};
    use std::io;
    use std::io::{Read, Write};

    static UPDATE_HELPER_MARKER: &str = "--private-alcomd3-updater-helper";

    impl InstallerInformation {
        // ------------------------------------------------------------------
        // macOS – extract .app.tar.gz into the app's own parent directory so
        // that rename() never crosses a filesystem boundary.
        // ------------------------------------------------------------------

        /// ### Expected archive structure:
        /// ```text
        /// ├── [AppName]_[version]_aarch64.app.tar.gz
        /// │   └── ALCOMD3.app
        /// │       └── Contents/…
        /// └── …
        /// ```
        pub(super) fn install_macos(&self, bytes: &[u8]) -> Result<()> {
            let app_path = &self.extract_path;
            let app_parent = (app_path.parent()).ok_or(Error::FailedToDetermineExtractPath)?;

            let app_metadata = app_path.metadata()?;
            let parent_metadata = app_parent.metadata()?;

            // if .app itself is a mount point, we cannot support updating application.
            if !same_device(&parent_metadata, &app_metadata) {
                return Err(Error::TempDirNotOnSameMountPoint);
            }

            // tries to create temporal directory for temporally extracting app next to the app.
            // If it was restricted with permission, we need to extract to temp dir
            // and (copy and) move from apple_script

            match tempfile::Builder::new()
                .prefix(".ALCOMD3-extracting")
                .tempdir_in(app_parent)
            {
                Ok(temp_dir) => {
                    // we have sufficient permission.
                    // We install with current permission
                    extract_update_app(temp_dir.path(), app_path, bytes)?;
                }
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                    // Insufficient permission.
                    log::debug!(
                        "update requires admin privileges. calling installer helper with osascript"
                    );

                    update_elevated_permission(app_path, bytes)?;
                }
                Err(e) => return Err(e.into()),
            }

            Ok(())
        }
    }

    fn update_elevated_permission(app_path: &Path, bytes: &[u8]) -> Result<()> {
        let mut tgz_file = tempfile::Builder::new()
            .prefix(".ALCOMD3-installing")
            .tempfile()?;

        tgz_file.write_all(bytes)?;
        tgz_file.flush()?;

        std::thread::sleep(std::time::Duration::from_millis(100));

        let apple_script = concat!(
            "on run argv\n",
            "    set exe to quoted form of item 1 of argv\n",
            "    set cmd to exe\n",
            "    repeat with i from 2 to count of argv\n",
            "        set cmd to cmd & \" \" & quoted form of item i of argv\n",
            "    end repeat\n",
            "    do shell script cmd with administrator privileges\n",
            "end run",
        );

        // Note for AI: we can do inject with following code.

        let updater = std::env::current_exe()?;
        let hash = hex::encode(sha2::Sha256::digest(bytes));

        let output = std::process::Command::new("osascript")
            .args(["-e", apple_script])
            .arg(updater)
            .arg(UPDATE_HELPER_MARKER)
            .arg(tgz_file.path())
            .arg(app_path)
            .arg(hash)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("failed to install update with admin privileges: {stderr}"),
            )));
        }

        Ok(())
    }

    #[allow(dead_code)] // only called on macos
    #[cfg_attr(feature = "no-self-updater", inline)]
    pub fn try_run_updater_helper() {
        if cfg!(feature = "no-self-updater") {
            return;
        }
        let Some((tgz_path, app_path, hash)) = parser() else {
            return;
        };

        if let Err(e) = main(&tgz_path, &app_path, &hash) {
            eprintln!("{}", e);
            std::process::exit(1);
        }

        std::process::exit(0);

        fn parser() -> Option<(OsString, OsString, OsString)> {
            let mut args = std::env::args_os();
            let _executable = args.next()?;
            let command = args.next()?;
            if command != OsStr::new(UPDATE_HELPER_MARKER) {
                return None;
            }
            let tgz_path = args.next()?;
            let app_path = args.next()?;
            let hash = args.next()?;
            if args.next().is_some() {
                return None;
            }

            Some((tgz_path, app_path, hash))
        }

        fn main(tgz_path: &OsStr, app_path: &OsStr, hash: &OsStr) -> io::Result<()> {
            let tgz = {
                let mut tgz_file = std::fs::File::open(tgz_path)?;
                let len = tgz_file.metadata()?.len();
                let mut vec = Vec::with_capacity(len as usize);
                tgz_file.read_to_end(&mut vec)?;
                vec
            };
            let app_path = Path::new(app_path);
            let hash = hex::decode(hash.as_encoded_bytes())
                .map_err(|x| io::Error::new(io::ErrorKind::InvalidData, x))?;
            let tgz_digit = sha2::Sha256::digest(&tgz);
            if hash != tgz_digit.as_slice() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid hash"));
            }

            let app_parent = app_path.parent().ok_or(io::ErrorKind::NotFound)?;

            let tmpdir = tempfile::Builder::new()
                .prefix(".ALCOMD3-extracting")
                .tempdir_in(app_parent)?;

            extract_update_app(tmpdir.path(), app_path, &tgz)?;

            drop(tmpdir);

            Ok(())
        }
    }

    fn extract_update_app(temp_dir: &Path, app_path: &Path, bytes: &[u8]) -> io::Result<()> {
        // we extract the tar.gz to the dir, swap, and remove old one.

        let new_tmp_app = temp_dir.join("new.app");

        std::fs::create_dir(&new_tmp_app)?;

        let decoder = GzDecoder::new(Cursor::new(bytes));
        let mut archive = tar::Archive::new(decoder);
        for entry in archive.entries()? {
            let mut entry = entry?;
            let tar_path = entry.path()?;
            let fs_path = {
                let mut iter = tar_path.iter();
                iter.next();
                iter.as_path()
            };
            log::info!("{} as {}", tar_path.display(), fs_path.display());
            if fs_path.as_os_str().is_empty() {
                continue;
            }
            let dest = new_tmp_app.join(fs_path);
            entry.unpack(&dest)?;
        }

        // tries to swap the app.
        if !swap_fs_entry(&new_tmp_app, app_path)? {
            // swapping failed, we do 3-way sapping.
            // less atomic but works well for most filesystem.
            let old_app = temp_dir.join("old.app");
            std::fs::rename(app_path, &old_app)?;
            std::fs::rename(new_tmp_app, app_path)?;
        }

        // Update mtime to Notify Finder that the bundle has changed.
        let _ = std::process::Command::new("touch")
            .arg("--")
            .arg(app_path)
            .status();

        Ok(())
    }
}

mod windows {
    use super::*;

    impl InstallerInformation {
        // ------------------------------------------------------------------
        // Windows – write installer to temp file and launch with ShellExecute
        // so UAC elevation works correctly.
        // ------------------------------------------------------------------

        /// ### Expected format:
        /// A plain `.exe` installer (the ALCOMD3 setup wrapper, which bundles the
        /// InnoSetup installer inside it).
        pub(super) fn install_windows(&self, bytes: &[u8], exit_after_launch: bool) -> Result<()> {
            // The actual Windows-API code lives in a #[cfg(windows)] block so it
            // only compiles on Windows.  The dispatch to this function already
            // happens under `if cfg!(windows)` in install_inner, so on every
            // other platform this branch is dead but still compiled.
            self.install_windows_impl(bytes, exit_after_launch)
        }

        fn install_windows_impl(&self, bytes: &[u8], exit_after_launch: bool) -> Result<()> {
            // Write the installer bytes to a persistent temp file.
            let mut tempfile = tempfile::Builder::new()
                .prefix("alcomd3_updater")
                .suffix(".exe")
                .tempfile()?;
            let installer_path = tempfile.path();
            std::fs::write(installer_path, bytes)?;

            fn wide_null(s: &str) -> Vec<u16> {
                s.encode_utf16().chain(std::iter::once(0)).collect()
            }

            let op = wide_null("open");
            let file = wide_null(&installer_path.to_string_lossy());
            let params = build_updater_args(&self.args, self.current_install);

            tempfile.disable_cleanup(true);
            drop(tempfile);
            start_installer(op, file, params)?;

            if exit_after_launch {
                std::process::exit(0);
            }
            Ok(())
        }
    }

    fn build_updater_args(args: &[String], current_install_mode: CurrentInstallMode) -> Vec<u16> {
        let mut result = Vec::new();

        for arg in args {
            let arg = if arg.starts_with('!') {
                let Some((name, value)) = arg.split_once(':') else {
                    continue; // failed to parse '!' arg
                };

                match name {
                    "!peruser" if current_install_mode == CurrentInstallMode::UserInstall => value,
                    "!machine" if current_install_mode == CurrentInstallMode::MachineInstall => {
                        value
                    }
                    _ => continue,
                }
            } else {
                arg.as_str()
            };

            result.push('"' as u16);

            let mut backslash = 0;
            for x in arg.encode_utf16() {
                if x == '"' as u16 {
                    for _ in 0..backslash {
                        result.push('\\' as u16);
                    }
                    result.push('\\' as u16);
                }

                if x == '\\' as u16 {
                    backslash += 1;
                } else {
                    backslash = 0;
                }
                result.push(x);
            }

            for _ in 0..backslash {
                result.push('\\' as u16);
            }

            result.push('"' as u16);
            result.push(' ' as u16);
        }

        result.push(0); // trailing null

        result
    }

    #[test]
    fn build_updater_args_test() {
        #[track_caller]
        fn tester(current_install_mode: CurrentInstallMode, args: &[&str], expected: &str) {
            let args = args.iter().copied().map(String::from).collect::<Vec<_>>();
            let result = build_updater_args(&args[..], current_install_mode);
            let expected_encoded = expected.encode_utf16().chain([0]).collect::<Vec<_>>();
            assert_eq!(
                result,
                expected_encoded,
                "\nleft:  {left_str:?}\nright: {right_str:?}",
                left_str = String::from_utf16_lossy(result.as_slice()),
                right_str = expected,
            );
        }

        // basic escaping test
        tester(
            CurrentInstallMode::UserInstall,
            &[r##""hello""##, r##"\"hello\""##, r##"hello\"##],
            r###""\"hello\"" "\\\"hello\\\"" "hello\\" "###,
        );

        // conditional test
        let args = &[
            "/normal",
            "!peruser:/peruser-only",
            "!machine:/machine",
            "/normal2",
            "!unknown:/unknown1",
            "!unknown2",
        ];

        tester(
            CurrentInstallMode::UserInstall,
            args,
            r##""/normal" "/peruser-only" "/normal2" "##,
        );

        tester(
            CurrentInstallMode::MachineInstall,
            args,
            r##""/normal" "/machine" "/normal2" "##,
        );
    }

    // os specific call
    #[cfg(windows)]
    fn start_installer(op: Vec<u16>, file: Vec<u16>, params: Vec<u16>) -> Result<()> {
        use ::windows::Win32::UI::Shell::ShellExecuteW;
        use ::windows::Win32::UI::WindowsAndMessaging::SW_SHOW;
        use ::windows::core::PCWSTR;

        unsafe {
            // SAFETY: all pointers remain valid for the duration of the call, since owned vec is passed
            let response = ShellExecuteW(
                None,
                PCWSTR(op.as_ptr()),
                PCWSTR(file.as_ptr()),
                PCWSTR(params.as_ptr()),
                PCWSTR(std::ptr::null()),
                SW_SHOW,
            );

            let response = response.0 as u32;
            if response > 32 {
                Ok(())
            } else {
                // Map the error code (<= 32) to an IO Error
                Err(std::io::Error::from_raw_os_error(response as i32).into())
            }
        }
    }

    #[cfg(not(windows))]
    fn start_installer(_op: Vec<u16>, _file: Vec<u16>, _params: Vec<u16>) -> Result<()> {
        unreachable!("install_windows_impl called on a non-Windows platform")
    }
}

mod linux {
    use super::unix::*;
    use super::*;
    use std::io::{Read, Write};

    impl InstallerInformation {
        // ------------------------------------------------------------------
        // Linux – replace the AppImage in-place, keeping the same filesystem.
        // ------------------------------------------------------------------

        /// ### Expected archive structure:
        /// ```text
        /// ├── [AppName]_[version]_amd64.AppImage.tar.gz
        /// │   └── [AppName]_[version]_amd64.AppImage
        /// └── …
        /// ```
        pub(super) fn install_linux(&self, bytes: &[u8]) -> Result<()> {
            self.install_appimage(bytes)
        }

        fn install_appimage(&self, bytes: &[u8]) -> Result<()> {
            let extract_metadata = self.extract_path.metadata()?;

            // Try multiple candidate temp directories until we find one on the
            // same device as the AppImage (rename requires same filesystem).
            let candidates: Vec<Box<dyn FnOnce() -> Option<PathBuf>>> = vec![
                // normal $TMPDIR (or os specific TMPDIR) typically at `/tmp` but can be tmpfs.
                Box::new(|| Some(std::env::temp_dir())),
                // $XDG_CACHE_HOME likely under $HOME.
                // if vrc-get-gui is installed under home directory, this likely to succeed but
                // if user installed ALCOMD3 to external HDD, not working.
                Box::new(|| {
                    std::env::var_os("XDG_CACHE_HOME")
                        .map(PathBuf::from)
                        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".cache")))
                }),
                // As a final fallback, the parent dir of extract path is used.
                // This will leave invisible file in user directory in case of abort so
                // not recommended, but likely to work in most case
                Box::new(|| self.extract_path.parent().map(|p| p.to_path_buf())),
            ];

            for candidate_fn in candidates {
                let Some(candidate) = candidate_fn() else {
                    continue;
                };
                let Ok(tmp_dir) = tempfile::Builder::new()
                    .prefix("alcomd3_update_app")
                    .tempdir_in(&candidate)
                else {
                    // can be EPERM
                    continue;
                };

                let Ok(tmp_metadata) = tmp_dir.path().metadata() else {
                    continue;
                };

                // Check that both paths are on the same device.
                let same_device = same_device(&extract_metadata, &tmp_metadata);
                if !same_device {
                    continue;
                }

                if !try_install_appimage(bytes, tmp_dir.path(), &self.extract_path)? {
                    continue;
                }

                return Ok(());
            }

            Err(Error::TempDirNotOnSameMountPoint)
        }
    }

    fn try_install_appimage(bytes: &[u8], tmp_dir: &Path, extract_path: &Path) -> Result<bool> {
        //Set permissions on the temp dir.
        set_temp_dir_permissions(tmp_dir).ok(); // not mandatory

        let tmp_app = tmp_dir.join("alcomd3-installing.AppImage");
        let original_perms = std::fs::metadata(extract_path)?.permissions();

        // Write new AppImage (may be raw bytes or a tar.gz).
        if looks_like_gz(bytes) {
            extract_appimage_from_gz_or_tar_gz(bytes, &tmp_app)?
        } else if looks_like_appimage(bytes) {
            std::fs::write(&tmp_app, bytes)?;
            std::fs::set_permissions(&tmp_app, original_perms)?;
        } else {
            return Err(Error::BinaryNotFoundInArchive);
        }

        match std::fs::rename(tmp_app, extract_path) {
            Ok(()) => Ok(true),
            Err(ref e) if e.kind() == std::io::ErrorKind::CrossesDevices => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    fn extract_appimage_from_gz_or_tar_gz(bytes: &[u8], tmp_app: &Path) -> Result<()> {
        use flate2::read::GzDecoder;

        let mut decoder = GzDecoder::new(Cursor::new(bytes));

        // read 512 bytes for fining header
        let mut header_viewer = [0u8; 512];
        decoder.read_exact(&mut header_viewer[..])?;
        let mut decoder = GzDecoder::new(Cursor::new(bytes));

        if looks_like_tar(header_viewer.as_ref()) {
            let mut archive = tar::Archive::new(decoder);
            for entry in archive.entries()? {
                let mut entry = entry?;
                if let Ok(path) = entry.path()
                    && path.extension().and_then(|e| e.to_str()) == Some("AppImage")
                {
                    entry.unpack(tmp_app)?;
                    return Ok(());
                }
            }
        } else if looks_like_appimage(header_viewer.as_ref()) {
            let mut out = std::fs::File::create(tmp_app)?;
            std::io::copy(&mut decoder, &mut out)?;
            out.flush()?;
            return Ok(());
        }

        Err(Error::BinaryNotFoundInArchive)
    }

    fn looks_like_gz(bytes: &[u8]) -> bool {
        bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
    }

    fn looks_like_appimage(bytes: &[u8]) -> bool {
        bytes.len() >= 16 && bytes[8] == b'A' && bytes[9] == b'I'
    }

    fn looks_like_tar(bytes: &[u8]) -> bool {
        bytes.len() >= 512 && (&bytes[257..][..6] == b"ustar\0" || &bytes[257..][..6] == b"ustar ")
    }
}

mod unix {
    // Helper: compare device IDs on Unix to prevent EXDEV
    // This is not perfect since same device can be mounted to multiple location but
    // I hope precheck can improve update speed.

    use std::io;
    use std::path::Path;

    pub(super) fn same_device(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            a.dev() == b.dev()
        }
        #[cfg(not(unix))]
        {
            let (_, _) = (a, b);
            false
        }
    }

    pub(super) fn set_temp_dir_permissions(path: &Path) -> crate::updater::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mut perms = path.metadata()?.permissions();
            perms.set_mode(0o700);
            std::fs::set_permissions(path, perms)?;
        }
        #[cfg(not(unix))]
        {
            let _ = path;
        }
        Ok(())
    }

    // returns false for unsupported platforms or filesystem
    pub(super) fn swap_fs_entry(file1: &Path, file2: &Path) -> io::Result<bool> {
        #[cfg(target_os = "macos")]
        {
            use std::ffi::CString;

            let file1 = CString::new(file1.as_os_str().as_encoded_bytes())
                .map_err(|_| io::ErrorKind::InvalidInput)?;
            let file2 = CString::new(file2.as_os_str().as_encoded_bytes())
                .map_err(|_| io::ErrorKind::InvalidInput)?;
            let result = unsafe {
                nix::libc::renamex_np(file1.as_ptr(), file2.as_ptr(), nix::libc::RENAME_SWAP)
            };
            if result == 0 {
                return Ok(true);
            }
            let last_err = io::Error::last_os_error();
            if last_err.raw_os_error() == Some(nix::libc::ENOTSUP) {
                return Ok(false);
            }
            Err(last_err)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = file1;
            let _ = file2;
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_asset_name_is_bound_to_the_manifest_version() {
        let expected = "ALCOMD3_9.8.7_windows_x86_64_setup.exe";
        validate_update_asset_name(
            &Url::parse(&format!("https://example.invalid/releases/{expected}")).unwrap(),
            expected,
        )
        .unwrap();

        let replayed =
            Url::parse("https://example.invalid/releases/ALCOMD3_9.8.6_windows_x86_64_setup.exe")
                .unwrap();
        assert!(validate_update_asset_name(&replayed, expected).is_err());
    }

    #[test]
    fn download_limit_checks_headers_and_accumulated_bytes() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("10"));
        assert_eq!(parse_content_length(&headers).unwrap(), Some(10));
        headers.append(header::CONTENT_LENGTH, HeaderValue::from_static("11"));
        assert!(parse_content_length(&headers).is_err());
        headers = HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("invalid"));
        assert!(parse_content_length(&headers).is_err());

        validate_download_size(Some(10), 10, 10).unwrap();
        assert!(validate_download_size(Some(11), 0, 10).is_err());
        assert!(validate_download_size(Some(1), 11, 10).is_err());
        assert!(validate_download_size(None, 11, 10).is_err());
    }

    #[test]
    fn release_signature_comment_must_bind_file_and_purpose() {
        let file_name = "ALCOMD3_9.8.7_windows_x86_64_setup.exe";
        validate_trusted_comment(
            &format!("timestamp:123\tfile:{file_name}\tpurpose:release"),
            file_name,
        )
        .unwrap();

        assert!(
            validate_trusted_comment(
                "timestamp:123\tfile:another.exe\tpurpose:release",
                file_name,
            )
            .is_err()
        );
        assert!(
            validate_trusted_comment(
                &format!("timestamp:123\tfile:{file_name}\tpurpose:local-test"),
                file_name,
            )
            .is_err()
        );
        assert_eq!(
            trusted_comment_value("file:first\tfile:second\tpurpose:release", "file"),
            Some("first")
        );
    }

    #[test]
    fn staged_package_paths_do_not_use_untrusted_path_components() {
        let staged = StagedUpdate {
            schema_version: STAGED_UPDATE_SCHEMA_VERSION,
            version: "9.8.7".to_string(),
            channel: "stable".to_string(),
            package_id: Uuid::nil().to_string(),
            signature: "signature".to_string(),
        };
        assert_eq!(
            staged_package_path(&staged).unwrap(),
            Path::new(crate::storage::AUTOMATIC_UPDATER_PACKAGES_DIR)
                .join("00000000-0000-0000-0000-000000000000.bin")
        );

        let mut invalid = staged;
        invalid.package_id = "../outside".to_string();
        assert!(staged_package_path(&invalid).is_err());
    }

    #[test]
    fn stable_channel_rejects_prerelease_packages() {
        let prerelease = Version::parse("9.8.7-beta.1").unwrap();
        assert!(validate_version_for_channel(&prerelease, "stable").is_err());
        validate_version_for_channel(&prerelease, "beta").unwrap();

        let stable = Version::parse("9.8.7").unwrap();
        validate_version_for_channel(&stable, "stable").unwrap();
        validate_version_for_channel(&stable, "beta").unwrap();
    }

    #[test]
    fn failed_automatic_update_blocks_only_the_same_release() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());

            record_failed_automatic_update_locked(&io, "9.8.7", "stable")
                .await
                .unwrap();
            assert!(automatic_update_installation_failed(&io, "stable", "9.8.7").await);
            assert!(!automatic_update_installation_failed(&io, "stable", "9.8.8").await);
            assert!(!automatic_update_installation_failed(&io, "beta", "9.8.7").await);

            DownloadedUpdate {
                version: "9.8.8".to_string(),
                signature: "signature".to_string(),
                bytes: b"new package".to_vec(),
            }
            .persist(&io, "stable")
            .await
            .unwrap();
            assert!(!automatic_update_installation_failed(&io, "stable", "9.8.7").await);
        });
    }

    #[test]
    fn unreadable_automatic_update_failure_state_is_preserved_and_suppresses_installation() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            DownloadedUpdate {
                version: "9.8.7".to_string(),
                signature: "signature".to_string(),
                bytes: b"staged package".to_vec(),
            }
            .persist(&io, "stable")
            .await
            .unwrap();
            let failed_path = io.resolve(Path::new(crate::storage::AUTOMATIC_UPDATER_FAILED_PATH));
            tokio::fs::create_dir_all(&failed_path).await.unwrap();

            assert!(automatic_update_installation_failed(&io, "stable", "9.8.7").await);
            let _guard = STAGED_UPDATE_WRITE_LOCK.lock().await;
            assert!(
                failed_automatic_update_matches_locked(
                    &io,
                    "stable",
                    &Version::parse("9.8.7").unwrap()
                )
                .await
                .is_err()
            );
            assert!(read_staged_update(&io).await.unwrap().is_some());
            assert!(failed_path.is_dir());
        });
    }

    #[test]
    fn invalid_automatic_update_failure_state_is_discarded() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            let failed_path = io.resolve(Path::new(crate::storage::AUTOMATIC_UPDATER_FAILED_PATH));
            tokio::fs::create_dir_all(failed_path.parent().unwrap())
                .await
                .unwrap();
            tokio::fs::write(&failed_path, b"not json").await.unwrap();

            assert!(!automatic_update_installation_failed(&io, "stable", "9.8.7").await);
            assert!(!failed_path.exists());
        });
    }

    #[test]
    fn automatic_install_arms_handoff_before_launching_the_installer() {
        let source = include_str!("updater.rs");
        let install = source.find("pub async fn install_staged_update").unwrap();
        let source = &source[install..];
        let arm = source
            .find("crate::arm_startup_request_handoff(app)")
            .unwrap();
        let launch = source
            .find("installer.install_inner_for_startup(&bytes)")
            .unwrap();

        assert!(arm < launch);
        assert!(source[launch..].contains("discard_staged_update_locked(io).await"));
    }

    #[test]
    fn transient_automatic_installation_errors_remain_retryable() {
        assert!(automatic_installation_error_is_transient(&Error::Io(
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "temporarily locked")
        )));
        assert!(!automatic_installation_error_is_transient(&Error::Io(
            std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid archive")
        )));
        assert!(!automatic_installation_error_is_transient(
            &Error::BinaryNotFoundInArchive
        ));
    }

    #[test]
    fn windows_startup_installer_can_return_to_the_handoff() {
        let source = include_str!("updater.rs");
        let windows_install = source.find("fn install_windows_impl").unwrap();
        let build_args = source[windows_install..]
            .find("fn build_updater_args")
            .unwrap();
        let source = &source[windows_install..windows_install + build_args];

        assert!(source.contains("start_installer(op, file, params)?"));
        assert!(source.contains("if exit_after_launch"));
        assert!(source.contains("process::exit"));
    }

    #[test]
    fn rejected_staged_update_is_discarded_and_suppressed() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            DownloadedUpdate {
                version: "9.8.7".to_string(),
                signature: "signature".to_string(),
                bytes: b"new package".to_vec(),
            }
            .persist(&io, "stable")
            .await
            .unwrap();

            {
                let _guard = STAGED_UPDATE_WRITE_LOCK.lock().await;
                reject_staged_automatic_update_locked(
                    &io,
                    "9.8.7",
                    "stable",
                    &Error::InvalidStagedUpdate("not installable".to_string()),
                )
                .await;
                assert!(read_staged_update(&io).await.unwrap().is_none());
            }

            assert!(automatic_update_installation_failed(&io, "stable", "9.8.7").await);
        });
    }

    #[test]
    fn downloaded_update_is_persisted_and_discarded() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            let packages_dir =
                io.resolve(Path::new(crate::storage::AUTOMATIC_UPDATER_PACKAGES_DIR));
            tokio::fs::create_dir_all(&packages_dir).await.unwrap();
            tokio::fs::write(packages_dir.join("interrupted.bin"), b"orphan")
                .await
                .unwrap();
            let interrupted_temporary_package =
                packages_dir.join(format!("{}.bin.temp.0", Uuid::new_v4()));
            tokio::fs::write(&interrupted_temporary_package, b"temporary orphan")
                .await
                .unwrap();
            let unrelated_temporary_file = packages_dir.join("keep.bin.temp.0");
            tokio::fs::write(&unrelated_temporary_file, b"unrelated temporary file")
                .await
                .unwrap();
            tokio::fs::write(packages_dir.join("keep.txt"), b"unrelated")
                .await
                .unwrap();
            let downloaded = DownloadedUpdate {
                version: "9.8.7".to_string(),
                signature: "signature".to_string(),
                bytes: b"signed package placeholder".to_vec(),
            };

            let receipt = downloaded.persist(&io, "stable").await.unwrap();
            assert!(staged_update_exists(&io).await);
            assert!(staged_update_satisfies(&io, "stable", "9.8.7").await);
            assert!(!staged_update_satisfies(&io, "stable", "9.8.8").await);
            assert!(!staged_update_satisfies(&io, "beta", "9.8.7").await);

            let staged = read_staged_update(&io).await.unwrap().unwrap();
            let package_path = staged_package_path(&staged).unwrap();
            assert_eq!(
                tokio::fs::read(io.resolve(&package_path)).await.unwrap(),
                b"signed package placeholder"
            );
            assert!(!packages_dir.join("interrupted.bin").exists());
            assert!(!interrupted_temporary_package.exists());
            assert!(unrelated_temporary_file.exists());

            tokio::fs::write(packages_dir.join("second-interruption.bin"), b"orphan")
                .await
                .unwrap();
            assert!(staged_update_exists(&io).await);
            assert!(!packages_dir.join("second-interruption.bin").exists());
            assert!(io.resolve(&package_path).exists());

            assert!(
                discard_staged_update_if_matches(&io, receipt)
                    .await
                    .unwrap()
            );
            assert!(!staged_update_exists(&io).await);
            assert!(!io.resolve(&package_path).exists());
            assert!(packages_dir.join("keep.txt").exists());
            assert!(unrelated_temporary_file.exists());
        });
    }

    #[test]
    fn older_download_cannot_replace_a_newer_staged_update() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            let newer = DownloadedUpdate {
                version: "9.8.8".to_string(),
                signature: "new signature".to_string(),
                bytes: b"new package".to_vec(),
            };
            let older = DownloadedUpdate {
                version: "9.8.7".to_string(),
                signature: "old signature".to_string(),
                bytes: b"old package".to_vec(),
            };

            let (newer_result, older_result) =
                futures::future::join(newer.persist(&io, "stable"), older.persist(&io, "stable"))
                    .await;
            newer_result.unwrap();
            let error = match older_result {
                Ok(_) => panic!("older staged update unexpectedly replaced the newer update"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("newer staged version"));

            let staged = read_staged_update(&io).await.unwrap().unwrap();
            assert_eq!(staged.version, "9.8.8");
            assert_eq!(
                tokio::fs::read(io.resolve(&staged_package_path(&staged).unwrap()))
                    .await
                    .unwrap(),
                b"new package"
            );
        });
    }

    #[test]
    fn conditional_rollback_does_not_discard_a_superseding_update() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            let first_receipt = DownloadedUpdate {
                version: "9.8.7".to_string(),
                signature: "first signature".to_string(),
                bytes: b"first package".to_vec(),
            }
            .persist(&io, "stable")
            .await
            .unwrap();
            let superseding = DownloadedUpdate {
                version: "9.8.8".to_string(),
                signature: "superseding signature".to_string(),
                bytes: b"superseding package".to_vec(),
            };

            let (superseding_result, rollback_result) = futures::future::join(
                superseding.persist(&io, "stable"),
                discard_staged_update_if_matches(&io, first_receipt),
            )
            .await;
            let superseding_receipt = superseding_result.unwrap();
            assert!(!rollback_result.unwrap());

            let staged = read_staged_update(&io).await.unwrap().unwrap();
            assert!(superseding_receipt.matches(&staged));
            assert_eq!(
                tokio::fs::read(io.resolve(&staged_package_path(&staged).unwrap()))
                    .await
                    .unwrap(),
                b"superseding package"
            );
        });
    }

    #[test]
    fn discarded_update_can_be_restored_with_a_fresh_transaction() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let io = DefaultEnvironmentIo::new(temp.path().into());
            DownloadedUpdate {
                version: "9.8.7".to_string(),
                signature: "signature".to_string(),
                bytes: b"package".to_vec(),
            }
            .persist(&io, "stable")
            .await
            .unwrap();
            let first = read_staged_update(&io).await.unwrap().unwrap();

            discard_staged_update(&io).await.unwrap();
            DownloadedUpdate {
                version: first.version,
                signature: first.signature,
                bytes: b"package".to_vec(),
            }
            .persist(&io, "stable")
            .await
            .unwrap();

            let restored = read_staged_update(&io).await.unwrap().unwrap();
            assert_ne!(restored.package_id, first.package_id);
            assert_eq!(
                tokio::fs::read(io.resolve(&staged_package_path(&restored).unwrap()))
                    .await
                    .unwrap(),
                b"package"
            );
        });
    }
}
