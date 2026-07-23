use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

static CONFIG_SOURCE: &str = include_str!("../../alcomd3.config.json");
static CONFIG: OnceLock<Alcomd3Config> = OnceLock::new();

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Alcomd3Config {
    homepage_url: String,
    repository: String,
    windows_app_id: String,
    windows_aumid: String,
    updater_manifests: UpdaterManifests,
    release_platforms: HashMap<String, ReleasePlatform>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdaterManifests {
    stable: UpdaterManifest,
    beta: UpdaterManifest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdaterManifest {
    public_path: String,
}

#[derive(Deserialize)]
struct ReleasePlatform {
    updater: ReleasePlatformUpdater,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleasePlatformUpdater {
    asset_pattern: String,
    max_download_bytes: u64,
    args: Vec<String>,
}

pub fn homepage_url() -> &'static str {
    &config().homepage_url
}

pub fn updater_endpoint(stable: bool) -> String {
    let config = config();
    let manifest = if stable {
        &config.updater_manifests.stable
    } else {
        &config.updater_manifests.beta
    };
    join_url_path(&config.homepage_url, &manifest.public_path)
}

pub fn repository_url() -> String {
    format!("https://github.com/{}", config().repository)
}

pub fn windows_app_id() -> &'static str {
    &config().windows_app_id
}

pub fn windows_aumid() -> &'static str {
    &config().windows_aumid
}

pub fn updater_asset_name(version: &str) -> Option<String> {
    let updater = updater_platform()?.updater.asset_pattern.as_str();
    updater
        .contains("{version}")
        .then(|| updater.replace("{version}", version))
}

pub fn updater_args() -> Option<&'static [String]> {
    Some(&updater_platform()?.updater.args)
}

pub fn updater_max_download_bytes() -> Option<u64> {
    Some(updater_platform()?.updater.max_download_bytes)
}

fn updater_platform() -> Option<&'static ReleasePlatform> {
    let key = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x86_64"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "darwin-aarch64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x86_64"
    } else {
        return None;
    };

    config().release_platforms.get(key)
}

fn config() -> &'static Alcomd3Config {
    CONFIG.get_or_init(|| {
        serde_json::from_str(CONFIG_SOURCE).expect("failed to parse alcomd3.config.json")
    })
}

fn join_url_path(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_updater_asset_uses_requested_version() {
        if let Some(asset_name) = updater_asset_name("9.8.7-beta.6") {
            assert!(asset_name.contains("9.8.7-beta.6"));
            assert!(!asset_name.contains("{version}"));
        }
    }

    #[test]
    fn configured_updater_download_limit_is_positive() {
        if let Some(max_download_bytes) = updater_max_download_bytes() {
            assert!(max_download_bytes > 0);
        }
    }

    #[test]
    fn configured_windows_app_id_is_braced() {
        assert!(windows_app_id().starts_with('{'));
        assert!(windows_app_id().ends_with('}'));
    }

    #[test]
    fn configured_windows_aumid_is_stable() {
        assert_eq!(windows_aumid(), "CQMHV.ALCOMD3");
    }
}
