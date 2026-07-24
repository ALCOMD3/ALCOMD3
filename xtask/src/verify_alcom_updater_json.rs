use crate::alcomd3_config::Alcomd3Config;
use crate::release_common::UpdaterSignaturePurpose;
use anyhow::{Context, Result, bail};
use base64::Engine;
use minisign::{PublicKeyBox, SignatureBox};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Verifies updater JSON before publishing the website.
#[derive(clap::Parser)]
pub struct Command {
    #[arg(long)]
    assets: PathBuf,
    #[arg(long, default_value = "artifacts/release-updater/tauri-updater.json")]
    json: PathBuf,
    #[arg(long, default_value = "vrc-get-gui/src/updater-public-key.txt")]
    public_key: PathBuf,

    /// Require the authenticated signature comment to identify this artifact purpose.
    #[arg(long, value_enum)]
    expected_signature_purpose: Option<UpdaterSignaturePurpose>,

    /// Verify a previously published platform subset using asset names bound by its URLs.
    /// This read-only compatibility mode must not be used before publishing new metadata.
    #[arg(long)]
    published_manifest: bool,
}

impl crate::Command for Command {
    fn run(self) -> Result<i32> {
        verify_alcom_updater_json(
            &self.assets,
            &self.json,
            &self.public_key,
            self.expected_signature_purpose,
            self.published_manifest,
        )?;
        println!("Updater JSON verification passed: {}", self.json.display());
        Ok(0)
    }
}

#[derive(Deserialize)]
struct UpdaterJson {
    version: String,
    platforms: HashMap<String, Platform>,
}

#[derive(Deserialize)]
struct Platform {
    signature: String,
    url: String,
    #[serde(default)]
    args: Vec<String>,
}

fn verify_alcom_updater_json(
    assets_dir: &Path,
    json_path: &Path,
    public_key_path: &Path,
    expected_signature_purpose: Option<UpdaterSignaturePurpose>,
    published_manifest: bool,
) -> Result<()> {
    let json = fs::read_to_string(json_path)
        .with_context(|| format!("failed to read updater JSON: {}", json_path.display()))?;
    let updater: UpdaterJson = serde_json::from_str(&json)
        .with_context(|| format!("failed to parse updater JSON: {}", json_path.display()))?;
    let config = Alcomd3Config::load()?;
    let expected_platforms = config
        .release_platforms
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    let actual_platforms = updater.platforms.keys().cloned().collect::<HashSet<_>>();
    if (!published_manifest && actual_platforms != expected_platforms)
        || (published_manifest && !actual_platforms.is_subset(&expected_platforms))
    {
        bail!("updater JSON platform allowlist does not match releasePlatforms");
    }

    for platform_key in &actual_platforms {
        let release_platform = config.release_platform(platform_key)?;
        let platform = updater
            .platforms
            .get(platform_key)
            .with_context(|| format!("missing platforms.{platform_key}"))?;
        if platform.signature.contains("REPLACE_WITH") || platform.signature.trim().is_empty() {
            bail!("updater JSON has a placeholder or empty signature for {platform_key}");
        }
        if platform.args != release_platform.updater.args {
            bail!("updater args mismatch for {platform_key}");
        }
        let release_base = format!("{}/", config.release_download_base_url(&updater.version));
        let asset_name = if published_manifest {
            published_asset_name(&platform.url, &release_base, platform_key)?
        } else {
            let asset_name = Alcomd3Config::release_asset_name(
                &release_platform.updater.asset_pattern,
                &updater.version,
            );
            let expected_url = format!("{release_base}{asset_name}");
            if platform.url != expected_url {
                bail!(
                    "updater URL mismatch for {platform_key}: expected {expected_url}, got {}",
                    platform.url,
                );
            }
            asset_name
        };
        let asset_path = assets_dir.join(&asset_name);
        verify_signature_value(
            &asset_path,
            &platform.signature,
            public_key_path,
            expected_signature_purpose,
        )?;
    }

    Ok(())
}

fn published_asset_name(url: &str, release_base: &str, platform_key: &str) -> Result<String> {
    let name = url.strip_prefix(release_base).with_context(|| {
        format!("published updater URL is outside the configured release for {platform_key}")
    })?;
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains('?')
        || name.contains('#')
        || name.contains('%')
    {
        bail!("published updater URL has an unsafe asset name for {platform_key}");
    }
    Ok(name.to_string())
}

pub(crate) fn verify_updater_signature_file(
    installer_path: &Path,
    signature_path: &Path,
    public_key_path: &Path,
    expected_signature_purpose: Option<UpdaterSignaturePurpose>,
) -> Result<()> {
    let signature = fs::read_to_string(signature_path).with_context(|| {
        format!(
            "failed to read updater signature: {}",
            signature_path.display()
        )
    })?;
    verify_signature_value(
        installer_path,
        &signature,
        public_key_path,
        expected_signature_purpose,
    )
}

fn verify_signature_value(
    installer_path: &Path,
    signature_base64: &str,
    public_key_path: &Path,
    expected_signature_purpose: Option<UpdaterSignaturePurpose>,
) -> Result<()> {
    let installer_name = installer_path
        .file_name()
        .context("installer path has no file name")?
        .to_string_lossy();
    let public_key = read_public_key(public_key_path)?;
    let signature = decode_signature(signature_base64)?;
    let mut installer = fs::File::open(installer_path)
        .with_context(|| format!("failed to open installer: {}", installer_path.display()))?;

    minisign::verify(&public_key, &signature, &mut installer, true, false, true)
        .context("signature does not verify with the embedded updater public key")?;
    validate_signature_trusted_comment(&signature, &installer_name, expected_signature_purpose)
}

fn validate_signature_trusted_comment(
    signature: &SignatureBox,
    expected_installer_name: &str,
    expected_signature_purpose: Option<UpdaterSignaturePurpose>,
) -> Result<()> {
    let trusted_comment = signature
        .trusted_comment()
        .context("updater signature has no authenticated trusted comment")?;
    let signed_file = trusted_comment_value(&trusted_comment, "file")
        .context("updater signature trusted comment has no file field")?;
    if signed_file != expected_installer_name {
        bail!(
            "updater signature file binding mismatch: expected {expected_installer_name}, got {signed_file}"
        );
    }

    if let Some(expected_purpose) = expected_signature_purpose {
        let actual_purpose = trusted_comment_value(&trusted_comment, "purpose")
            .context("updater signature trusted comment has no purpose field")?;
        if actual_purpose != expected_purpose.to_string() {
            bail!(
                "updater signature purpose mismatch: expected {expected_purpose}, got {actual_purpose}"
            );
        }
    }
    Ok(())
}

fn trusted_comment_value<'a>(trusted_comment: &'a str, key: &str) -> Option<&'a str> {
    trusted_comment
        .split('\t')
        .filter_map(|field| field.split_once(':'))
        .find_map(|(field_key, value)| (field_key == key).then_some(value))
}

pub(crate) fn read_public_key(public_key_path: &Path) -> Result<minisign::PublicKey> {
    let public_key_base64 = fs::read_to_string(public_key_path).with_context(|| {
        format!(
            "failed to read updater public key: {}",
            public_key_path.display()
        )
    })?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(public_key_base64.trim())
        .context("failed to base64-decode updater public key")?;
    let decoded = String::from_utf8(decoded).context("updater public key is not valid UTF-8")?;
    PublicKeyBox::from_string(&decoded)
        .context("failed to parse updater public key as minisign public key")?
        .into_public_key()
        .context("failed to convert updater public key to minisign public key")
}

fn decode_signature(signature_base64: &str) -> Result<SignatureBox> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(signature_base64)
        .context("failed to base64-decode updater signature")?;
    let decoded = String::from_utf8(decoded).context("updater signature is not valid UTF-8")?;
    SignatureBox::from_string(&decoded).context("failed to parse updater signature")
}

#[cfg(test)]
mod tests {
    use super::{published_asset_name, trusted_comment_value, validate_signature_trusted_comment};
    use crate::release_common::UpdaterSignaturePurpose;
    use minisign::KeyPair;

    #[test]
    fn trusted_comment_fields_bind_file_and_purpose() {
        let comment =
            "timestamp:1783496553\tfile:ALCOMD3_2.2.0_windows_x86_64_setup.exe\tpurpose:release";

        assert_eq!(
            trusted_comment_value(comment, "file"),
            Some("ALCOMD3_2.2.0_windows_x86_64_setup.exe")
        );
        assert_eq!(trusted_comment_value(comment, "purpose"), Some("release"));
        assert_eq!(trusted_comment_value(comment, "missing"), None);
    }

    #[test]
    fn published_manifest_mode_accepts_only_assets_from_the_exact_release_directory() {
        let base = "https://github.com/ALCOMD3/ALCOMD3/releases/download/v2.1.1/";
        assert_eq!(
            published_asset_name(
                "https://github.com/ALCOMD3/ALCOMD3/releases/download/v2.1.1/alcomd3-2.1.1-setup.exe",
                base,
                "windows-x86_64",
            )
            .unwrap(),
            "alcomd3-2.1.1-setup.exe"
        );
        assert!(
            published_asset_name(
                "https://example.test/alcomd3-2.1.1-setup.exe",
                base,
                "windows-x86_64",
            )
            .is_err()
        );
        assert!(
            published_asset_name(
                "https://github.com/ALCOMD3/ALCOMD3/releases/download/v2.1.1/../asset",
                base,
                "windows-x86_64",
            )
            .is_err()
        );
    }

    #[test]
    fn authenticated_signature_comment_rejects_the_wrong_purpose_or_file() {
        let path = std::env::temp_dir().join(format!(
            "alcomd3-{}-signed-test-setup.exe",
            std::process::id()
        ));
        std::fs::write(&path, b"signed test installer").unwrap();
        let key_pair = KeyPair::generate_unencrypted_keypair().unwrap();
        let signature = crate::sign_alcom_updater::sign_file(
            &key_pair.sk,
            &path,
            UpdaterSignaturePurpose::LocalTest,
        )
        .unwrap();
        let file_name = path.file_name().unwrap().to_string_lossy();

        validate_signature_trusted_comment(
            &signature,
            &file_name,
            Some(UpdaterSignaturePurpose::LocalTest),
        )
        .unwrap();
        assert!(
            validate_signature_trusted_comment(
                &signature,
                &file_name,
                Some(UpdaterSignaturePurpose::Release),
            )
            .unwrap_err()
            .to_string()
            .contains("purpose mismatch")
        );
        assert!(
            validate_signature_trusted_comment(
                &signature,
                "different-installer.exe",
                Some(UpdaterSignaturePurpose::LocalTest),
            )
            .unwrap_err()
            .to_string()
            .contains("file binding mismatch")
        );

        let _ = std::fs::remove_file(path);
    }
}
