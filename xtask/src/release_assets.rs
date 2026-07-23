use crate::alcomd3_config::{Alcomd3Config, ReleasePlatform, ReleaseUpdateMode};
use crate::release_common::{ReleaseChannel, UpdaterSignaturePurpose, validate_full_git_sha};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const RELEASE_BUILD_MANIFEST_SCHEMA_VERSION: u32 = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedReleaseAsset {
    pub name: String,
    pub source: PathBuf,
    pub roles: Vec<String>,
    pub update_mode: ReleaseUpdateMode,
}

#[derive(Clone, Debug)]
pub struct ResolvedReleasePlatform {
    pub key: String,
    pub target: String,
    pub bundles: Vec<String>,
    pub macos_ad_hoc_signed: bool,
    pub updater: ResolvedReleaseAsset,
    pub downloads: Vec<ResolvedReleaseAsset>,
}

impl ResolvedReleasePlatform {
    pub fn unsigned_assets(&self) -> Vec<ResolvedReleaseAsset> {
        let mut assets = vec![self.updater.clone()];
        for download in &self.downloads {
            if let Some(existing) = assets.iter_mut().find(|asset| asset.name == download.name) {
                debug_assert_eq!(existing.update_mode, download.update_mode);
                for role in &download.roles {
                    if !existing.roles.contains(role) {
                        existing.roles.push(role.clone());
                    }
                }
            } else {
                assets.push(download.clone());
            }
        }
        assets
    }

    pub fn updater_signature_name(&self) -> String {
        format!("{}.sig", self.updater.name)
    }
}

pub fn resolve_release_platforms(
    config: &Alcomd3Config,
    workspace_root: &Path,
    version: &str,
) -> Vec<ResolvedReleasePlatform> {
    config
        .release_platforms
        .iter()
        .map(|(key, platform)| resolve_release_platform(key, platform, workspace_root, version))
        .collect()
}

pub fn resolve_release_platform(
    key: &str,
    platform: &ReleasePlatform,
    workspace_root: &Path,
    version: &str,
) -> ResolvedReleasePlatform {
    let release_dir = workspace_root
        .join("target")
        .join(&platform.target)
        .join("release");
    let resolve =
        |pattern: &str| release_dir.join(Alcomd3Config::release_asset_name(pattern, version));
    let updater = ResolvedReleaseAsset {
        name: Alcomd3Config::release_asset_name(&platform.updater.asset_pattern, version),
        source: resolve(&platform.updater.source_path_pattern),
        roles: vec!["updater".to_string()],
        update_mode: platform.updater.update_mode,
    };
    let downloads = platform
        .downloads
        .iter()
        .map(|download| ResolvedReleaseAsset {
            name: Alcomd3Config::release_asset_name(&download.asset_pattern, version),
            source: resolve(&download.source_path_pattern),
            roles: vec![format!("download:{}", download.id)],
            update_mode: download.update_mode,
        })
        .collect();

    ResolvedReleasePlatform {
        key: key.to_string(),
        target: platform.target.clone(),
        bundles: platform.bundles.clone(),
        macos_ad_hoc_signed: platform.macos_ad_hoc_signing.is_some(),
        updater,
        downloads,
    }
}

pub fn expected_public_asset_names(platforms: &[ResolvedReleasePlatform]) -> Vec<String> {
    let mut names = Vec::new();
    for platform in platforms {
        for asset in platform.unsigned_assets() {
            if !names.contains(&asset.name) {
                names.push(asset.name);
            }
        }
        names.push(platform.updater_signature_name());
    }
    names
}

pub fn copy_platform_assets(
    platform: &ResolvedReleasePlatform,
    artifact_dir: &Path,
    dry_run: bool,
) -> Result<()> {
    println!(
        "copy {} build artifacts to {}",
        platform.key,
        artifact_dir.display()
    );
    if dry_run {
        return Ok(());
    }

    fs::create_dir_all(artifact_dir)
        .with_context(|| format!("creating {}", artifact_dir.display()))?;
    for asset in platform.unsigned_assets() {
        if !asset.source.is_file() {
            bail!(
                "{} release asset does not exist: {}",
                platform.key,
                asset.source.display()
            );
        }
        fs::copy(&asset.source, artifact_dir.join(&asset.name)).with_context(|| {
            format!(
                "copying {} release asset {}",
                platform.key,
                asset.source.display()
            )
        })?;
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReleaseBuildShard {
    schema_version: u32,
    version: String,
    channel: String,
    source_sha: String,
    platform: String,
    target: String,
    macos_ad_hoc_signed: bool,
    assets: Vec<ManifestAsset>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestPlatform {
    key: String,
    target: String,
    macos_ad_hoc_signed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestAsset {
    name: String,
    sha256: String,
    size: u64,
    roles: Vec<String>,
    platform: String,
    update_mode: ReleaseUpdateMode,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReleaseBuildManifest {
    schema_version: u32,
    version: String,
    channel: String,
    source_sha: String,
    signature_purpose: String,
    platforms: Vec<ManifestPlatform>,
    assets: Vec<ManifestAsset>,
}

pub struct ReleaseManifestPaths<'a> {
    pub version: &'a str,
    pub channel: ReleaseChannel,
    pub source_sha: &'a str,
    pub artifact_dir: &'a Path,
    pub shard_dir: &'a Path,
    pub manifest_path: &'a Path,
}

pub fn write_release_build_shard(
    platform: &ResolvedReleasePlatform,
    paths: &ReleaseManifestPaths<'_>,
) -> Result<PathBuf> {
    validate_full_git_sha(paths.source_sha)?;
    let assets = platform
        .unsigned_assets()
        .into_iter()
        .map(|asset| manifest_asset(&platform.key, &asset, paths.artifact_dir))
        .collect::<Result<Vec<_>>>()?;
    let shard = ReleaseBuildShard {
        schema_version: RELEASE_BUILD_MANIFEST_SCHEMA_VERSION,
        version: paths.version.to_string(),
        channel: paths.channel.to_string(),
        source_sha: paths.source_sha.to_string(),
        platform: platform.key.clone(),
        target: platform.target.clone(),
        macos_ad_hoc_signed: platform.macos_ad_hoc_signed,
        assets,
    };
    fs::create_dir_all(paths.shard_dir)
        .with_context(|| format!("creating {}", paths.shard_dir.display()))?;
    let path = paths.shard_dir.join(format!("{}.json", platform.key));
    write_json(&path, &shard)?;
    println!("release build shard is ready: {}", path.display());
    Ok(path)
}

pub fn assemble_release_build_manifest(
    platforms: &[ResolvedReleasePlatform],
    paths: &ReleaseManifestPaths<'_>,
) -> Result<()> {
    validate_full_git_sha(paths.source_sha)?;
    let mut recorded_unsigned = load_and_verify_release_build_shards(platforms, paths)?;
    let mut assets = Vec::new();
    for platform in platforms {
        for expected in platform.unsigned_assets() {
            let asset = recorded_unsigned.remove(&expected.name).with_context(|| {
                format!("release build shards are missing asset {}", expected.name)
            })?;
            assets.push(asset);
        }
        let signature_name = platform.updater_signature_name();
        let signature = ResolvedReleaseAsset {
            name: signature_name,
            source: PathBuf::new(),
            roles: vec![format!("signature:{}", platform.updater.name)],
            update_mode: platform.updater.update_mode,
        };
        assets.push(manifest_asset(
            &platform.key,
            &signature,
            paths.artifact_dir,
        )?);
    }
    if !recorded_unsigned.is_empty() {
        bail!("release build shards contain unexpected assets");
    }

    let manifest = ReleaseBuildManifest {
        schema_version: RELEASE_BUILD_MANIFEST_SCHEMA_VERSION,
        version: paths.version.to_string(),
        channel: paths.channel.to_string(),
        source_sha: paths.source_sha.to_string(),
        signature_purpose: UpdaterSignaturePurpose::Release.to_string(),
        platforms: platforms.iter().map(manifest_platform).collect(),
        assets,
    };
    if let Some(parent) = paths.manifest_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    write_json(paths.manifest_path, &manifest)?;
    println!(
        "release build manifest is ready: {}",
        paths.manifest_path.display()
    );
    Ok(())
}

pub fn verify_release_build_shards(
    platforms: &[ResolvedReleasePlatform],
    paths: &ReleaseManifestPaths<'_>,
) -> Result<()> {
    load_and_verify_release_build_shards(platforms, paths).map(|_| ())
}

fn load_and_verify_release_build_shards(
    platforms: &[ResolvedReleasePlatform],
    paths: &ReleaseManifestPaths<'_>,
) -> Result<HashMap<String, ManifestAsset>> {
    validate_full_git_sha(paths.source_sha)?;
    let mut recorded_unsigned = HashMap::new();
    for platform in platforms {
        let shard_path = paths.shard_dir.join(format!("{}.json", platform.key));
        let source = fs::read_to_string(&shard_path)
            .with_context(|| format!("reading release build shard: {}", shard_path.display()))?;
        let shard: ReleaseBuildShard = serde_json::from_str(&source)
            .with_context(|| format!("parsing release build shard: {}", shard_path.display()))?;
        validate_shard(platform, paths, &shard)?;
        for asset in shard.assets {
            if recorded_unsigned
                .insert(asset.name.clone(), asset)
                .is_some()
            {
                bail!("release build shards contain a duplicate asset");
            }
        }
    }
    for platform in platforms {
        for expected in platform.unsigned_assets() {
            let asset = recorded_unsigned.get(&expected.name).with_context(|| {
                format!("release build shards are missing asset {}", expected.name)
            })?;
            verify_manifest_asset(&asset, &paths.artifact_dir.join(&asset.name))?;
        }
    }
    Ok(recorded_unsigned)
}

pub fn verify_artifact_directory_allowlist(
    artifact_dir: &Path,
    expected_names: &[String],
) -> Result<()> {
    let expected = expected_names.iter().cloned().collect::<HashSet<_>>();
    let mut actual = HashSet::new();
    for entry in
        fs::read_dir(artifact_dir).with_context(|| format!("reading {}", artifact_dir.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_file() {
            bail!(
                "release artifact directory contains a non-file entry: {}",
                entry.path().display()
            );
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("release asset name is not valid UTF-8"))?;
        if !actual.insert(name.clone()) {
            bail!("release artifact directory contains duplicate asset {name}");
        }
    }
    if actual != expected {
        bail!("release artifact directory does not match the exact public asset allowlist");
    }
    Ok(())
}

pub fn verify_release_build_manifest(
    platforms: &[ResolvedReleasePlatform],
    paths: &ReleaseManifestPaths<'_>,
) -> Result<()> {
    validate_full_git_sha(paths.source_sha)?;
    let source = fs::read_to_string(paths.manifest_path).with_context(|| {
        format!(
            "reading release build manifest: {}",
            paths.manifest_path.display()
        )
    })?;
    let manifest: ReleaseBuildManifest = serde_json::from_str(&source).with_context(|| {
        format!(
            "parsing release build manifest: {}",
            paths.manifest_path.display()
        )
    })?;
    if manifest.schema_version != RELEASE_BUILD_MANIFEST_SCHEMA_VERSION
        || manifest.version != paths.version
        || manifest.channel != paths.channel.to_string()
        || !manifest.source_sha.eq_ignore_ascii_case(paths.source_sha)
        || manifest.signature_purpose != UpdaterSignaturePurpose::Release.to_string()
    {
        bail!("release build manifest metadata does not match the requested release");
    }
    let expected_platforms = platforms.iter().map(manifest_platform).collect::<Vec<_>>();
    if manifest.platforms != expected_platforms {
        bail!("release build manifest platform policies do not match the requested release");
    }

    let expected_assets = expected_public_assets(platforms);
    if manifest.assets.len() != expected_assets.len() {
        bail!(
            "release build manifest asset count mismatch: expected {}, got {}",
            expected_assets.len(),
            manifest.assets.len()
        );
    }
    let mut assets = manifest
        .assets
        .into_iter()
        .map(|asset| (asset.name.clone(), asset))
        .collect::<HashMap<_, _>>();
    if assets.len() != expected_assets.len() {
        bail!("release build manifest contains duplicate assets");
    }
    for (platform_key, expected) in expected_assets {
        let asset = assets.remove(&expected.name).with_context(|| {
            format!("release build manifest is missing asset {}", expected.name)
        })?;
        validate_manifest_asset_contract(&asset, &platform_key, &expected)?;
        verify_manifest_asset(&asset, &paths.artifact_dir.join(&expected.name))?;
    }
    if !assets.is_empty() {
        bail!("release build manifest contains unexpected assets");
    }
    Ok(())
}

fn validate_shard(
    platform: &ResolvedReleasePlatform,
    paths: &ReleaseManifestPaths<'_>,
    shard: &ReleaseBuildShard,
) -> Result<()> {
    if shard.schema_version != RELEASE_BUILD_MANIFEST_SCHEMA_VERSION
        || shard.version != paths.version
        || shard.channel != paths.channel.to_string()
        || !shard.source_sha.eq_ignore_ascii_case(paths.source_sha)
        || shard.platform != platform.key
        || shard.target != platform.target
        || shard.macos_ad_hoc_signed != platform.macos_ad_hoc_signed
    {
        bail!(
            "release build shard metadata does not match platform {}",
            platform.key
        );
    }
    let mut expected = platform
        .unsigned_assets()
        .into_iter()
        .map(|asset| (asset.name.clone(), asset))
        .collect::<HashMap<_, _>>();
    if shard.assets.len() != expected.len() {
        bail!(
            "release build shard asset allowlist mismatch for {}",
            platform.key
        );
    }
    for asset in &shard.assets {
        let expected_asset = expected.remove(&asset.name).with_context(|| {
            format!(
                "release build shard contains unexpected asset {} for {}",
                asset.name, platform.key
            )
        })?;
        validate_manifest_asset_contract(asset, &platform.key, &expected_asset)?;
    }
    if !expected.is_empty() {
        bail!(
            "release build shard asset allowlist mismatch for {}",
            platform.key
        );
    }
    Ok(())
}

fn expected_public_assets(
    platforms: &[ResolvedReleasePlatform],
) -> Vec<(String, ResolvedReleaseAsset)> {
    let mut assets = Vec::new();
    for platform in platforms {
        assets.extend(
            platform
                .unsigned_assets()
                .into_iter()
                .map(|asset| (platform.key.clone(), asset)),
        );
        assets.push((
            platform.key.clone(),
            ResolvedReleaseAsset {
                name: platform.updater_signature_name(),
                source: PathBuf::new(),
                roles: vec![format!("signature:{}", platform.updater.name)],
                update_mode: platform.updater.update_mode,
            },
        ));
    }
    assets
}

fn validate_manifest_asset_contract(
    actual: &ManifestAsset,
    expected_platform: &str,
    expected: &ResolvedReleaseAsset,
) -> Result<()> {
    if actual.name != expected.name
        || actual.platform != expected_platform
        || actual.roles != expected.roles
        || actual.update_mode != expected.update_mode
    {
        bail!(
            "release asset contract does not match {} for platform {expected_platform}",
            expected.name
        );
    }
    Ok(())
}

fn manifest_platform(platform: &ResolvedReleasePlatform) -> ManifestPlatform {
    ManifestPlatform {
        key: platform.key.clone(),
        target: platform.target.clone(),
        macos_ad_hoc_signed: platform.macos_ad_hoc_signed,
    }
}

fn manifest_asset(
    platform: &str,
    asset: &ResolvedReleaseAsset,
    artifact_dir: &Path,
) -> Result<ManifestAsset> {
    let path = artifact_dir.join(&asset.name);
    let metadata = fs::metadata(&path)
        .with_context(|| format!("reading release asset metadata: {}", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        bail!("release asset must be a non-empty file: {}", path.display());
    }
    Ok(ManifestAsset {
        name: asset.name.clone(),
        sha256: crate::utils::file_sha256(&path)?,
        size: metadata.len(),
        roles: asset.roles.clone(),
        platform: platform.to_string(),
        update_mode: asset.update_mode,
    })
}

fn verify_manifest_asset(asset: &ManifestAsset, path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("reading release asset metadata: {}", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        bail!("release asset must be a non-empty file: {}", path.display());
    }
    if metadata.len() != asset.size {
        bail!("release asset size mismatch: {}", path.display());
    }
    crate::utils::verify_file_sha256(path, &asset.sha256)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    fs::write(path, json).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{
        ReleaseManifestPaths, assemble_release_build_manifest, expected_public_asset_names,
        resolve_release_platforms, verify_artifact_directory_allowlist,
        verify_release_build_manifest, verify_release_build_shards, write_release_build_shard,
    };
    use crate::alcomd3_config::{Alcomd3Config, ReleaseUpdateMode};
    use crate::release_common::ReleaseChannel;
    use std::fs;
    use std::process::id;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn release_asset_contract_resolves_exact_three_platform_allowlist() {
        let config = Alcomd3Config::load().unwrap();
        let workspace = crate::utils::cargo::cargo_metadata()
            .workspace_root
            .as_std_path()
            .to_path_buf();
        let platforms = resolve_release_platforms(&config, &workspace, "2.2.0");
        let macos = platforms
            .iter()
            .find(|platform| platform.key == "darwin-aarch64")
            .unwrap();

        assert!(macos.macos_ad_hoc_signed);
        let linux = platforms
            .iter()
            .find(|platform| platform.key == "linux-x86_64")
            .unwrap();
        assert_eq!(linux.updater.update_mode, ReleaseUpdateMode::SelfUpdater);
        assert_eq!(
            linux
                .downloads
                .iter()
                .find(|asset| asset.roles == ["download:linux-appimage"])
                .unwrap()
                .update_mode,
            ReleaseUpdateMode::SelfUpdater
        );
        assert_eq!(
            linux
                .downloads
                .iter()
                .find(|asset| asset.roles == ["download:linux-deb"])
                .unwrap()
                .update_mode,
            ReleaseUpdateMode::NoSelfUpdater
        );

        assert_eq!(
            expected_public_asset_names(&platforms),
            [
                "ALCOMD3_2.2.0_windows_x86_64_setup.exe",
                "ALCOMD3_2.2.0_windows_x86_64_setup.exe.zip",
                "ALCOMD3_2.2.0_windows_x86_64_setup.exe.sig",
                "ALCOMD3_2.2.0_macos_aarch64.app.tar.gz",
                "ALCOMD3_2.2.0_macos_aarch64.dmg",
                "ALCOMD3_2.2.0_macos_aarch64.app.tar.gz.sig",
                "ALCOMD3_2.2.0_linux_x86_64.AppImage.tar.gz",
                "ALCOMD3_2.2.0_linux_x86_64.AppImage",
                "ALCOMD3_2.2.0_linux_amd64.deb",
                "ALCOMD3_2.2.0_linux_x86_64.AppImage.tar.gz.sig",
            ]
        );
    }

    #[test]
    fn release_manifest_binds_all_shards_and_rejects_tampering() {
        let config = Alcomd3Config::load().unwrap();
        let root = std::env::temp_dir().join(format!(
            "alcomd3-release-assets-{}-{}",
            id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let artifact_dir = root.join("release");
        let shard_dir = root.join("state/v2.2.0");
        let manifest_path = root.join("state/v2.2.0.json");
        fs::create_dir_all(&artifact_dir).unwrap();
        let platforms = resolve_release_platforms(&config, &root, "2.2.0");
        let paths = ReleaseManifestPaths {
            version: "2.2.0",
            channel: ReleaseChannel::Stable,
            source_sha: "0123456789abcdef0123456789abcdef01234567",
            artifact_dir: &artifact_dir,
            shard_dir: &shard_dir,
            manifest_path: &manifest_path,
        };

        for platform in &platforms {
            for asset in platform.unsigned_assets() {
                fs::write(artifact_dir.join(&asset.name), asset.name.as_bytes()).unwrap();
            }
            write_release_build_shard(platform, &paths).unwrap();
            fs::write(
                artifact_dir.join(platform.updater_signature_name()),
                b"signature",
            )
            .unwrap();
        }

        verify_release_build_shards(&platforms, &paths).unwrap();
        let windows_shard_path = shard_dir.join("windows-x86_64.json");
        let original_windows_shard = fs::read_to_string(&windows_shard_path).unwrap();
        let shard: serde_json::Value = serde_json::from_str(&original_windows_shard).unwrap();
        assert_eq!(shard["schemaVersion"], 4);
        assert_eq!(shard["assets"][0]["updateMode"], "self-updater");

        for (field, value) in [
            ("name", serde_json::json!("unexpected.exe")),
            ("platform", serde_json::json!("darwin-aarch64")),
            ("roles", serde_json::json!(["download:wrong"])),
            ("updateMode", serde_json::json!("no-self-updater")),
            ("size", serde_json::json!(999_u64)),
            ("sha256", serde_json::json!("0".repeat(64))),
        ] {
            let mut tampered = shard.clone();
            tampered["assets"][0][field] = value;
            fs::write(
                &windows_shard_path,
                serde_json::to_string_pretty(&tampered).unwrap(),
            )
            .unwrap();
            assert!(
                verify_release_build_shards(&platforms, &paths).is_err(),
                "{field}"
            );
        }
        fs::write(&windows_shard_path, original_windows_shard).unwrap();
        verify_release_build_shards(&platforms, &paths).unwrap();

        let expected = expected_public_asset_names(&platforms);
        verify_artifact_directory_allowlist(&artifact_dir, &expected).unwrap();
        assemble_release_build_manifest(&platforms, &paths).unwrap();
        verify_release_build_manifest(&platforms, &paths).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["schemaVersion"], 4);
        assert_eq!(manifest["platforms"][1]["macosAdHocSigned"], true);
        assert_eq!(manifest["assets"][0]["updateMode"], "self-updater");
        assert_eq!(
            manifest["assets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|asset| asset["name"] == "ALCOMD3_2.2.0_linux_amd64.deb")
                .unwrap()["updateMode"],
            "no-self-updater"
        );

        let original_manifest = fs::read_to_string(&manifest_path).unwrap();
        for (field, value) in [
            ("name", serde_json::json!("unexpected.exe")),
            ("platform", serde_json::json!("darwin-aarch64")),
            ("roles", serde_json::json!(["download:wrong"])),
            ("updateMode", serde_json::json!("no-self-updater")),
            ("size", serde_json::json!(999_u64)),
            ("sha256", serde_json::json!("0".repeat(64))),
        ] {
            let mut tampered = manifest.clone();
            tampered["assets"][0][field] = value;
            fs::write(
                &manifest_path,
                serde_json::to_string_pretty(&tampered).unwrap(),
            )
            .unwrap();
            assert!(
                verify_release_build_manifest(&platforms, &paths).is_err(),
                "{field}"
            );
        }
        fs::write(&manifest_path, original_manifest).unwrap();
        verify_release_build_manifest(&platforms, &paths).unwrap();

        fs::remove_dir_all(root).unwrap();
    }
}
