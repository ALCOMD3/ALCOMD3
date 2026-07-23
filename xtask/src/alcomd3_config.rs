use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alcomd3Config {
    pub product_name: String,
    pub main_binary_name: String,
    pub package_name: String,
    pub mcp_binary_name: String,
    pub publisher_name: String,
    pub homepage_url: String,
    pub repository: String,
    pub tauri_identifier: String,
    pub legacy_tauri_identifier: String,
    pub windows_app_id: String,
    pub windows_aumid: String,
    pub legacy_windows_app_id: String,
    pub legacy_windows_migration_release_tag: String,
    pub legacy_windows_executable_name: String,
    pub installer_file_pattern: String,
    pub release_platforms: IndexMap<String, ReleasePlatform>,
    pub short_description: String,
    pub long_description: String,
    pub copyright: String,
    pub updater_manifests: UpdaterManifests,
    pub release_automation: ReleaseAutomation,
    pub template_association: TemplateAssociation,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterManifests {
    pub stable: UpdaterManifest,
    pub beta: UpdaterManifest,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterManifest {
    pub workspace_path: String,
    pub public_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasePlatform {
    pub target: String,
    pub bundles: Vec<String>,
    pub macos_ad_hoc_signing: Option<ReleaseMacosAdHocSigning>,
    pub updater: ReleaseUpdaterAsset,
    pub downloads: Vec<ReleaseDownloadAsset>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseMacosAdHocSigning {}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseUpdaterAsset {
    pub asset_pattern: String,
    pub source_path_pattern: String,
    pub max_download_bytes: u64,
    pub update_mode: ReleaseUpdateMode,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDownloadAsset {
    pub id: String,
    pub format: String,
    pub asset_pattern: String,
    pub source_path_pattern: String,
    pub update_mode: ReleaseUpdateMode,
    pub primary: bool,
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseUpdateMode {
    SelfUpdater,
    NoSelfUpdater,
}

impl ReleaseUpdateMode {
    pub fn uses_self_updater(self) -> bool {
        matches!(self, Self::SelfUpdater)
    }
}

impl Display for ReleaseUpdateMode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::SelfUpdater => "self-updater",
            Self::NoSelfUpdater => "no-self-updater",
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseAutomation {
    pub rust_toolchain: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateAssociation {
    pub name: String,
    pub extension: String,
    pub key: String,
}

impl Alcomd3Config {
    pub fn load_from_workspace(workspace_root: &Path) -> Result<Self> {
        let path = workspace_root.join("alcomd3.config.json");
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let config: Self =
            serde_json::from_str(&source).with_context(|| format!("parsing {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn load() -> Result<Self> {
        let metadata = crate::utils::cargo::cargo_metadata();
        Self::load_from_workspace(metadata.workspace_root.as_std_path())
    }

    pub fn site_base_url(&self) -> &str {
        self.homepage_url.trim_end_matches('/')
    }

    pub fn release_download_base_url(&self, version: &str) -> String {
        format!(
            "https://github.com/{}/releases/download/v{version}",
            self.repository
        )
    }

    pub fn release_tag_url(&self, version: &str) -> String {
        format!(
            "https://github.com/{}/releases/tag/v{version}",
            self.repository
        )
    }

    pub fn installer_file_name(&self, version: &str) -> String {
        self.installer_file_pattern.replace("{version}", version)
    }

    pub fn release_platform(&self, platform: &str) -> Result<&ReleasePlatform> {
        self.release_platforms
            .get(platform)
            .with_context(|| format!("release platform is not configured: {platform}"))
    }

    pub fn release_asset_name(pattern: &str, version: &str) -> String {
        pattern.replace("{version}", version)
    }

    pub fn workspace_path(&self, relative: &str, workspace_root: &Path) -> PathBuf {
        workspace_root.join(relative)
    }

    pub fn stable_updater_manifest(&self) -> &UpdaterManifest {
        &self.updater_manifests.stable
    }

    pub fn beta_updater_manifest(&self) -> &UpdaterManifest {
        &self.updater_manifests.beta
    }

    fn validate(&self) -> Result<()> {
        ensure_non_empty("productName", &self.product_name)?;
        ensure_non_empty("mainBinaryName", &self.main_binary_name)?;
        ensure_non_empty("packageName", &self.package_name)?;
        ensure_non_empty("mcpBinaryName", &self.mcp_binary_name)?;
        ensure_non_empty("publisherName", &self.publisher_name)?;
        ensure_non_empty("homepageUrl", &self.homepage_url)?;
        ensure_non_empty("repository", &self.repository)?;
        ensure_non_empty("tauriIdentifier", &self.tauri_identifier)?;
        ensure_non_empty("legacyTauriIdentifier", &self.legacy_tauri_identifier)?;
        ensure_non_empty("windowsAppId", &self.windows_app_id)?;
        ensure_non_empty("windowsAumid", &self.windows_aumid)?;
        ensure_non_empty("legacyWindowsAppId", &self.legacy_windows_app_id)?;
        ensure_non_empty(
            "legacyWindowsMigrationReleaseTag",
            &self.legacy_windows_migration_release_tag,
        )?;
        ensure_non_empty(
            "legacyWindowsExecutableName",
            &self.legacy_windows_executable_name,
        )?;
        ensure_non_empty("installerFilePattern", &self.installer_file_pattern)?;
        ensure_non_empty("shortDescription", &self.short_description)?;
        ensure_non_empty("longDescription", &self.long_description)?;
        ensure_non_empty("copyright", &self.copyright)?;
        ensure_non_empty(
            "updaterManifests.stable.workspacePath",
            &self.updater_manifests.stable.workspace_path,
        )?;
        ensure_non_empty(
            "updaterManifests.stable.publicPath",
            &self.updater_manifests.stable.public_path,
        )?;
        ensure_non_empty(
            "updaterManifests.beta.workspacePath",
            &self.updater_manifests.beta.workspace_path,
        )?;
        ensure_non_empty(
            "updaterManifests.beta.publicPath",
            &self.updater_manifests.beta.public_path,
        )?;
        ensure_non_empty(
            "releaseAutomation.rustToolchain",
            &self.release_automation.rust_toolchain,
        )?;
        ensure_non_empty("templateAssociation.name", &self.template_association.name)?;
        ensure_non_empty(
            "templateAssociation.extension",
            &self.template_association.extension,
        )?;
        ensure_non_empty("templateAssociation.key", &self.template_association.key)?;

        if !self.installer_file_pattern.contains("{version}") {
            bail!("installerFilePattern must contain {{version}}");
        }
        validate_tauri_identifier("tauriIdentifier", &self.tauri_identifier)?;
        validate_tauri_identifier("legacyTauriIdentifier", &self.legacy_tauri_identifier)?;
        if self
            .tauri_identifier
            .eq_ignore_ascii_case(&self.legacy_tauri_identifier)
        {
            bail!("tauriIdentifier and legacyTauriIdentifier must be different");
        }
        validate_release_platforms(&self.release_platforms)?;
        if !self.repository.contains('/') {
            bail!("repository must be in OWNER/REPO form");
        }
        if !self.windows_app_id.starts_with('{') || !self.windows_app_id.ends_with('}') {
            bail!("windowsAppId must include surrounding braces");
        }
        validate_windows_aumid(&self.windows_aumid)?;
        if !self.legacy_windows_app_id.starts_with('{')
            || !self.legacy_windows_app_id.ends_with('}')
        {
            bail!("legacyWindowsAppId must include surrounding braces");
        }
        if self
            .windows_app_id
            .eq_ignore_ascii_case(&self.legacy_windows_app_id)
        {
            bail!("windowsAppId and legacyWindowsAppId must be different");
        }
        let migration_version = self
            .legacy_windows_migration_release_tag
            .strip_prefix('v')
            .context("legacyWindowsMigrationReleaseTag must start with 'v'")?;
        semver::Version::parse(migration_version)
            .context("legacyWindowsMigrationReleaseTag must contain a semantic version")?;
        if !self.template_association.extension.starts_with('.') {
            bail!("templateAssociation.extension must start with '.'");
        }

        Ok(())
    }
}

fn validate_windows_aumid(value: &str) -> Result<()> {
    let segments = value.split('.').collect::<Vec<_>>();
    if value.len() > 128
        || segments.len() < 2
        || segments.iter().any(|segment| {
            segment.is_empty()
                || !segment.chars().all(|ch| ch.is_ascii_alphanumeric())
                || !segment
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphabetic())
        })
    {
        bail!(
            "windowsAumid must be at most 128 characters with at least two dot-separated ASCII alphanumeric segments starting with letters"
        );
    }
    Ok(())
}

fn validate_tauri_identifier(name: &str, value: &str) -> Result<()> {
    let segments = value.split('.').collect::<Vec<_>>();
    if segments.len() < 2
        || segments.iter().any(|segment| {
            segment.is_empty()
                || !segment
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
                || !segment
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric())
                || !segment
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric())
        })
    {
        bail!("{name} must be a dot-separated identifier with alphanumeric segment boundaries");
    }
    Ok(())
}

fn validate_release_platforms(platforms: &IndexMap<String, ReleasePlatform>) -> Result<()> {
    let required_platforms = ["windows-x86_64", "darwin-aarch64", "linux-x86_64"];
    for platform in required_platforms {
        if !platforms.contains_key(platform) {
            bail!("releasePlatforms is missing required platform {platform}");
        }
    }

    let mut targets = HashSet::new();
    let mut asset_patterns = HashSet::new();
    let mut download_ids = HashSet::new();
    for (platform_key, platform) in platforms {
        ensure_non_empty(
            &format!("releasePlatforms.{platform_key}.target"),
            &platform.target,
        )?;
        if !targets.insert(platform.target.as_str()) {
            bail!("release platform target is duplicated: {}", platform.target);
        }
        if platform.bundles.is_empty() {
            bail!("releasePlatforms.{platform_key}.bundles must not be empty");
        }
        for bundle in &platform.bundles {
            ensure_non_empty(&format!("releasePlatforms.{platform_key}.bundles"), bundle)?;
        }
        validate_macos_ad_hoc_signing(platform_key, platform)?;
        validate_asset_pattern(
            &format!("releasePlatforms.{platform_key}.updater.assetPattern"),
            &platform.updater.asset_pattern,
        )?;
        validate_source_path_pattern(
            &format!("releasePlatforms.{platform_key}.updater.sourcePathPattern"),
            &platform.updater.source_path_pattern,
        )?;
        if platform.updater.max_download_bytes == 0 {
            bail!(
                "releasePlatforms.{platform_key}.updater.maxDownloadBytes must be greater than zero"
            );
        }
        if !asset_patterns.insert(platform.updater.asset_pattern.as_str()) {
            bail!(
                "release asset pattern is duplicated: {}",
                platform.updater.asset_pattern
            );
        }
        if platform.downloads.is_empty() {
            bail!("releasePlatforms.{platform_key}.downloads must not be empty");
        }
        let primary_count = platform
            .downloads
            .iter()
            .filter(|download| download.primary)
            .count();
        if primary_count != 1 {
            bail!(
                "releasePlatforms.{platform_key}.downloads must contain exactly one primary asset"
            );
        }
        for download in &platform.downloads {
            ensure_non_empty(
                &format!("releasePlatforms.{platform_key}.downloads.id"),
                &download.id,
            )?;
            ensure_non_empty(
                &format!("releasePlatforms.{platform_key}.downloads.format"),
                &download.format,
            )?;
            validate_asset_pattern(
                &format!("releasePlatforms.{platform_key}.downloads.assetPattern"),
                &download.asset_pattern,
            )?;
            validate_source_path_pattern(
                &format!("releasePlatforms.{platform_key}.downloads.sourcePathPattern"),
                &download.source_path_pattern,
            )?;
            if !download_ids.insert(download.id.as_str()) {
                bail!("release download id is duplicated: {}", download.id);
            }
            if !asset_patterns.insert(download.asset_pattern.as_str())
                && download.asset_pattern != platform.updater.asset_pattern
            {
                bail!(
                    "release asset pattern is duplicated: {}",
                    download.asset_pattern
                );
            }
            if download.asset_pattern == platform.updater.asset_pattern
                && download.update_mode != platform.updater.update_mode
            {
                bail!(
                    "releasePlatforms.{platform_key}.downloads.{}.updateMode must match the updater updateMode when both roles use the same asset",
                    download.id
                );
            }
        }
        validate_update_mode_recipe(platform_key, platform)?;
    }
    Ok(())
}

fn validate_update_mode_recipe(platform_key: &str, platform: &ReleasePlatform) -> Result<()> {
    require_update_mode(
        &format!("releasePlatforms.{platform_key}.updater.updateMode"),
        platform.updater.update_mode,
        ReleaseUpdateMode::SelfUpdater,
    )?;

    for download in &platform.downloads {
        let expected = match platform_key {
            "windows-x86_64" | "darwin-aarch64" => ReleaseUpdateMode::SelfUpdater,
            "linux-x86_64" => match download.format.as_str() {
                "appimage" => ReleaseUpdateMode::SelfUpdater,
                "deb" => ReleaseUpdateMode::NoSelfUpdater,
                other => bail!(
                    "releasePlatforms.{platform_key}.downloads.{} has no release build recipe for format {other}",
                    download.id
                ),
            },
            _ => continue,
        };
        require_update_mode(
            &format!(
                "releasePlatforms.{platform_key}.downloads.{}.updateMode",
                download.id
            ),
            download.update_mode,
            expected,
        )?;
    }
    Ok(())
}

fn require_update_mode(
    name: &str,
    actual: ReleaseUpdateMode,
    expected: ReleaseUpdateMode,
) -> Result<()> {
    if actual != expected {
        bail!("{name} must be {expected} for the configured release build recipe");
    }
    Ok(())
}

fn validate_macos_ad_hoc_signing(platform_key: &str, platform: &ReleasePlatform) -> Result<()> {
    let is_macos = platform.target.contains("apple-darwin");
    match (is_macos, &platform.macos_ad_hoc_signing) {
        (true, None) => bail!(
            "releasePlatforms.{platform_key}.macosAdHocSigning must explicitly configure ad-hoc signing"
        ),
        (false, Some(_)) => {
            bail!(
                "releasePlatforms.{platform_key}.macosAdHocSigning is only valid for a macOS target"
            )
        }
        (false, None) | (true, Some(_)) => Ok(()),
    }
}

fn validate_asset_pattern(name: &str, pattern: &str) -> Result<()> {
    ensure_non_empty(name, pattern)?;
    if !pattern.contains("{version}") {
        bail!("{name} must contain {{version}}");
    }
    if pattern.contains('/') || pattern.contains('\\') || pattern.contains("..") {
        bail!("{name} must be a file name without path traversal");
    }
    Ok(())
}

fn validate_source_path_pattern(name: &str, pattern: &str) -> Result<()> {
    ensure_non_empty(name, pattern)?;
    let path = Path::new(pattern);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("{name} must be a workspace-relative path without traversal");
    }
    Ok(())
}

fn ensure_non_empty(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{name} must not be empty");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_value() -> serde_json::Value {
        serde_json::from_str(include_str!("../../alcomd3.config.json")).unwrap()
    }

    fn parse_and_validate(value: serde_json::Value) -> Result<Alcomd3Config> {
        let config: Alcomd3Config = serde_json::from_value(value)?;
        config.validate()?;
        Ok(config)
    }

    #[test]
    fn macos_ad_hoc_signing_is_explicit() {
        let config = parse_and_validate(config_value()).unwrap();
        assert!(
            config
                .release_platforms
                .get("darwin-aarch64")
                .and_then(|platform| platform.macos_ad_hoc_signing.as_ref())
                .is_some()
        );
    }

    #[test]
    fn macos_target_rejects_an_implicit_signing_policy() {
        let mut value = config_value();
        value["releasePlatforms"]["darwin-aarch64"]
            .as_object_mut()
            .unwrap()
            .remove("macosAdHocSigning");

        let error = parse_and_validate(value).unwrap_err();
        assert!(error.to_string().contains("must explicitly configure"));
    }

    #[test]
    fn ad_hoc_policy_rejects_signing_mode_extensions() {
        let mut value = config_value();
        value["releasePlatforms"]["darwin-aarch64"]["macosAdHocSigning"]["mode"] =
            serde_json::Value::String("certificate".into());

        let error = parse_and_validate(value).unwrap_err();
        assert!(error.to_string().contains("unknown field `mode`"));
    }

    #[test]
    fn every_release_asset_requires_an_update_mode() {
        let mut value = config_value();
        value["releasePlatforms"]["windows-x86_64"]["updater"]
            .as_object_mut()
            .unwrap()
            .remove("updateMode");

        let error = parse_and_validate(value).unwrap_err();
        assert!(error.to_string().contains("missing field `updateMode`"));
    }

    #[test]
    fn updater_download_limit_must_be_positive() {
        let mut value = config_value();
        value["releasePlatforms"]["windows-x86_64"]["updater"]["maxDownloadBytes"] =
            serde_json::Value::Number(0.into());

        let error = parse_and_validate(value).unwrap_err();
        assert!(error.to_string().contains("maxDownloadBytes"));
    }

    #[test]
    fn update_mode_rejects_unknown_values() {
        let mut value = config_value();
        value["releasePlatforms"]["windows-x86_64"]["updater"]["updateMode"] =
            serde_json::Value::String("package-manager".into());

        let error = parse_and_validate(value).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unknown variant `package-manager`")
        );
    }

    #[test]
    fn release_update_modes_match_the_supported_build_recipes() {
        let config = parse_and_validate(config_value()).unwrap();
        for (platform_key, platform) in &config.release_platforms {
            assert_eq!(
                platform.updater.update_mode,
                ReleaseUpdateMode::SelfUpdater,
                "{platform_key} updater payload"
            );
        }
        let linux = &config.release_platforms["linux-x86_64"];
        assert_eq!(
            linux
                .downloads
                .iter()
                .find(|download| download.format == "appimage")
                .unwrap()
                .update_mode,
            ReleaseUpdateMode::SelfUpdater
        );
        assert_eq!(
            linux
                .downloads
                .iter()
                .find(|download| download.format == "deb")
                .unwrap()
                .update_mode,
            ReleaseUpdateMode::NoSelfUpdater
        );
    }

    #[test]
    fn release_recipe_rejects_a_non_self_updating_updater_payload() {
        let mut value = config_value();
        value["releasePlatforms"]["darwin-aarch64"]["updater"]["updateMode"] =
            serde_json::Value::String("no-self-updater".into());

        let error = parse_and_validate(value).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("updater.updateMode must be self-updater")
        );
    }

    #[test]
    fn release_recipe_rejects_a_self_updating_deb() {
        let mut value = config_value();
        value["releasePlatforms"]["linux-x86_64"]["downloads"][1]["updateMode"] =
            serde_json::Value::String("self-updater".into());

        let error = parse_and_validate(value).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("linux-deb.updateMode must be no-self-updater")
        );
    }

    #[test]
    fn windows_app_ids_must_be_different() {
        let mut value = config_value();
        value["legacyWindowsAppId"] = value["windowsAppId"].clone();

        let error = parse_and_validate(value).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("windowsAppId and legacyWindowsAppId must be different")
        );
    }

    #[test]
    fn tauri_identifiers_must_be_different() {
        let mut value = config_value();
        value["legacyTauriIdentifier"] = value["tauriIdentifier"].clone();

        let error = parse_and_validate(value).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("tauriIdentifier and legacyTauriIdentifier must be different")
        );
    }

    #[test]
    fn legacy_tauri_identifier_must_be_a_safe_path_segment() {
        let mut value = config_value();
        value["legacyTauriIdentifier"] = serde_json::Value::String("../ALCOMD3".into());

        let error = parse_and_validate(value).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("legacyTauriIdentifier must be a dot-separated identifier")
        );
    }

    #[test]
    fn windows_aumid_must_be_safe_and_stable() {
        let mut value = config_value();
        value["windowsAumid"] = serde_json::Value::String("CQMHV ALCOMD3".into());

        let error = parse_and_validate(value).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("windowsAumid must be at most 128 characters")
        );
    }

    #[test]
    fn legacy_windows_migration_release_tag_must_be_semantic() {
        let mut value = config_value();
        value["legacyWindowsMigrationReleaseTag"] = serde_json::Value::String("latest".into());

        let error = parse_and_validate(value).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("legacyWindowsMigrationReleaseTag must start with 'v'")
        );
    }
}
