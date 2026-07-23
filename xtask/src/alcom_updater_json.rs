use crate::alcomd3_config::Alcomd3Config;
use anyhow::*;
use chrono::{DateTime, Timelike, Utc};
use indexmap::IndexMap;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::result::Result::Ok;

/// Generates json for tauri updater.
#[derive(clap::Parser)]
pub struct Command {
    #[clap(long = "assets", default_value = "assets")]
    assets_dir: PathBuf,
    #[clap(long = "version")]
    version: String,
    #[clap(long = "updater-notes")]
    updater_notes: Option<PathBuf>,
    #[clap(long = "pub-date")]
    pub_date: Option<DateTime<Utc>>,
    out_path: PathBuf,
}

impl crate::Command for Command {
    fn run(self) -> Result<i32> {
        match (self.updater_notes, self.pub_date) {
            (Some(updater_notes), pub_date) => create_alcom_updater_json_with_options(
                &self.assets_dir,
                &self.version,
                &self.out_path,
                &updater_notes,
                false,
                pub_date,
            )?,
            (None, Some(pub_date)) => {
                let updater_notes = default_updater_notes_path(&self.version);
                create_alcom_updater_json_with_options(
                    &self.assets_dir,
                    &self.version,
                    &self.out_path,
                    &updater_notes,
                    true,
                    Some(pub_date),
                )?;
            }
            (None, None) => {
                let updater_notes = default_updater_notes_path(&self.version);
                create_alcom_updater_json(
                    &self.assets_dir,
                    &self.version,
                    &self.out_path,
                    &updater_notes,
                )?;
            }
        }
        Ok(0)
    }
}

#[derive(Serialize)]
struct UpdaterJson<'a> {
    version: &'a str,
    notes: String,
    notes_i18n: IndexMap<String, String>,
    pub_date: chrono::DateTime<Utc>,
    platforms: IndexMap<String, Platform>,
}

#[derive(serde::Serialize)]
struct Platform {
    signature: String,
    url: String,
    args: Vec<String>,
}

pub fn create_alcom_updater_json(
    assets_dir: &Path,
    version: &str,
    out_path: &Path,
    updater_notes_path: &Path,
) -> Result<()> {
    create_alcom_updater_json_with_options(
        assets_dir,
        version,
        out_path,
        updater_notes_path,
        true,
        None,
    )
}

fn create_alcom_updater_json_with_options(
    assets_dir: &Path,
    version: &str,
    out_path: &Path,
    updater_notes_path: &Path,
    allow_missing_updater_notes_fallback: bool,
    pub_date: Option<DateTime<Utc>>,
) -> Result<()> {
    let config = Alcomd3Config::load()?;
    let base_url = config.release_download_base_url(version);

    // create platforms info
    let mut platforms = IndexMap::new();
    for (platform_key, release_platform) in &config.release_platforms {
        let file_name =
            Alcomd3Config::release_asset_name(&release_platform.updater.asset_pattern, version);

        std::fs::metadata(assets_dir.join(&file_name)).with_context(|| file_name.clone())?;

        let sig_name = format!("{file_name}.sig");
        let signature = std::fs::read_to_string(assets_dir.join(&sig_name))
            .with_context(|| sig_name.clone())?;

        let url = format!("{base_url}/{file_name}");
        platforms.insert(
            platform_key.to_string(),
            Platform {
                signature,
                url,
                args: release_platform.updater.args.clone(),
            },
        );
    }

    let release_url = config.release_tag_url(version);
    let notes = format!(
        "{} v{version}. See {release_url} for details.",
        config.product_name
    );
    let notes_i18n = read_updater_notes_i18n(
        updater_notes_path,
        version,
        allow_missing_updater_notes_fallback,
        &config,
    )?;

    let updater = UpdaterJson {
        version,
        notes,
        notes_i18n,
        pub_date: pub_date.unwrap_or_else(|| Utc::now().with_nanosecond(0).unwrap()),
        platforms,
    };

    let json = serde_json::to_string_pretty(&updater)?;
    std::fs::write(out_path, json).context("write updater.json")?;

    Ok(())
}

fn default_updater_notes_path(version: &str) -> PathBuf {
    PathBuf::from("release-notes").join(format!("ALCOMD3_{version}.updater-notes.json"))
}

fn read_updater_notes_i18n(
    path: &Path,
    version: &str,
    allow_missing_fallback: bool,
    config: &Alcomd3Config,
) -> Result<IndexMap<String, String>> {
    let mut notes = if path.exists() {
        read_updater_notes_i18n_file(path)
    } else if allow_missing_fallback {
        Ok(fallback_updater_notes_i18n(version, config))
    } else {
        bail!("updater notes file does not exist: {}", path.display())
    }?;

    let release_url = config.release_tag_url(version);
    for note in notes.values_mut() {
        if !note.contains(&release_url) {
            note.push_str("\n\n");
            note.push_str(&release_url);
        }
    }

    Ok(notes)
}

pub(crate) fn validate_updater_notes_file(path: &Path) -> Result<()> {
    read_updater_notes_i18n_file(path).map(|_| ())
}

fn read_updater_notes_i18n_file(path: &Path) -> Result<IndexMap<String, String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading updater notes: {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing updater notes JSON: {}", path.display()))?;

    let object = value
        .as_object()
        .with_context(|| format!("updater notes must be a JSON object: {}", path.display()))?;

    let mut notes = IndexMap::new();
    for (language, value) in object {
        ensure!(
            is_supported_updater_notes_language(language),
            "unsupported updater notes language `{language}` in {}",
            path.display()
        );

        let value = value.as_str().with_context(|| {
            format!(
                "updater notes value for `{language}` must be a string in {}",
                path.display()
            )
        })?;
        let value = value.trim();
        ensure!(
            !value.is_empty(),
            "updater notes value for `{language}` must not be empty in {}",
            path.display()
        );
        notes.insert(language.clone(), value.to_string());
    }

    ensure!(
        !notes.is_empty(),
        "updater notes must contain at least one language in {}",
        path.display()
    );

    Ok(notes)
}

fn is_supported_updater_notes_language(language: &str) -> bool {
    matches!(
        language,
        "en" | "de" | "fr" | "ja" | "ko" | "zh_hans" | "zh_hant"
    )
}

fn fallback_updater_notes_i18n(version: &str, config: &Alcomd3Config) -> IndexMap<String, String> {
    let release_url = config.release_tag_url(version);
    let product_name = &config.product_name;
    [
        (
            "en",
            format!("{product_name} v{version}. See {release_url} for details."),
        ),
        (
            "zh_hans",
            format!("{product_name} v{version}。详情请查看 {release_url}。"),
        ),
        (
            "zh_hant",
            format!("{product_name} v{version}。詳情請查看 {release_url}。"),
        ),
        (
            "ja",
            format!("{product_name} v{version}。詳細は {release_url} を確認してください。"),
        ),
        (
            "ko",
            format!("{product_name} v{version}. 자세한 내용은 {release_url} 를 확인하세요."),
        ),
        (
            "fr",
            format!("{product_name} v{version}. Consultez {release_url} pour plus de détails."),
        ),
        (
            "de",
            format!("{product_name} v{version}. Details unter {release_url}."),
        ),
    ]
    .into_iter()
    .map(|(language, notes)| (language.to_string(), notes))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "alcomd3-updater-json-test-{}-{nanos}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn updater_json_accepts_explicit_release_pub_date() {
        let command = Command::try_parse_from([
            "xtask",
            "--assets",
            "assets",
            "--version",
            "2.1.1",
            "--pub-date",
            "2026-07-12T01:02:03Z",
            "updater.json",
        ])
        .unwrap();

        assert_eq!(
            command.pub_date.unwrap().to_rfc3339(),
            "2026-07-12T01:02:03+00:00"
        );
    }

    #[test]
    fn updater_json_writes_explicit_release_pub_date() {
        let root = temp_dir("pub-date");
        let assets = root.join("assets");
        let version = "2.1.1";
        create_assets(&assets, version);
        let notes = root.join("notes.json");
        fs::write(&notes, r#"{ "en": "Maintenance release." }"#).unwrap();
        let out = root.join("updater.json");
        let pub_date = "2026-07-12T01:02:03Z".parse::<DateTime<Utc>>().unwrap();

        create_alcom_updater_json_with_options(
            &assets,
            version,
            &out,
            &notes,
            false,
            Some(pub_date),
        )
        .unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        let _ = fs::remove_dir_all(root);

        assert_eq!(value["pub_date"], "2026-07-12T01:02:03Z");
    }

    #[test]
    fn updater_json_is_deterministic_for_release_pub_date() {
        let root = temp_dir("deterministic-pub-date");
        let assets = root.join("assets");
        let version = "2.1.1";
        create_assets(&assets, version);
        let notes = root.join("notes.json");
        fs::write(&notes, r#"{ "en": "Maintenance release." }"#).unwrap();
        let first = root.join("first.json");
        let second = root.join("second.json");
        let pub_date = "2026-07-12T01:02:03Z".parse::<DateTime<Utc>>().unwrap();

        create_alcom_updater_json_with_options(
            &assets,
            version,
            &first,
            &notes,
            false,
            Some(pub_date),
        )
        .unwrap();
        create_alcom_updater_json_with_options(
            &assets,
            version,
            &second,
            &notes,
            false,
            Some(pub_date),
        )
        .unwrap();
        let first = fs::read_to_string(first).unwrap();
        let second = fs::read_to_string(second).unwrap();
        let _ = fs::remove_dir_all(root);

        assert_eq!(first, second);
    }

    fn create_assets(dir: &Path, version: &str) {
        fs::create_dir_all(dir).unwrap();
        let config = Alcomd3Config::load().unwrap();
        for platform in config.release_platforms.values() {
            let name = Alcomd3Config::release_asset_name(&platform.updater.asset_pattern, version);
            fs::write(dir.join(&name), "updater").unwrap();
            fs::write(dir.join(format!("{name}.sig")), "signature").unwrap();
        }
    }

    fn generate(version: &str, updater_notes: &Path) -> serde_json::Value {
        let root = temp_dir("generate");
        let assets = root.join("assets");
        create_assets(&assets, version);
        let out = root.join("updater.json");

        create_alcom_updater_json(&assets, version, &out, updater_notes).unwrap();
        let json = fs::read_to_string(out).unwrap();
        let _ = fs::remove_dir_all(root);
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn missing_updater_notes_generates_fallback_i18n() {
        let version = "2.1.0-beta.3";
        let value = generate(version, Path::new("missing-updater-notes.json"));
        let config = Alcomd3Config::load().unwrap();
        let release_url = config.release_tag_url(version);

        assert_eq!(
            value["notes"],
            format!(
                "{} v{version}. See {release_url} for details.",
                config.product_name
            )
        );
        assert_eq!(
            value["notes_i18n"]["zh_hans"],
            format!(
                "{} v{version}。详情请查看 {release_url}。",
                config.product_name
            )
        );
        assert!(
            value["notes_i18n"]["en"]
                .as_str()
                .unwrap()
                .contains(version)
        );
    }

    #[test]
    fn generated_windows_url_uses_configured_release_asset() {
        let version = "2.1.0-beta.3";
        let value = generate(version, Path::new("missing-updater-notes.json"));
        let config = Alcomd3Config::load().unwrap();
        let expected_url = format!(
            "{}/{}",
            config.release_download_base_url(version),
            config.installer_file_name(version)
        );

        assert_eq!(value["platforms"]["windows-x86_64"]["url"], expected_url);
    }

    #[test]
    fn explicit_missing_updater_notes_is_rejected() {
        let root = temp_dir("explicit-missing");
        let assets = root.join("assets");
        let version = "2.1.0-beta.3";
        create_assets(&assets, version);
        let out = root.join("updater.json");
        let missing_notes = root.join("missing-updater-notes.json");

        let error = create_alcom_updater_json_with_options(
            &assets,
            version,
            &out,
            &missing_notes,
            false,
            None,
        )
        .unwrap_err();
        let _ = fs::remove_dir_all(root);

        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn existing_updater_notes_keep_release_url() {
        let root = temp_dir("sidecar");
        let notes_path = root.join("notes.json");
        fs::write(
            &notes_path,
            r#"{
                "en": "Short English update summary.",
                "zh_hans": "简短中文更新摘要。"
            }"#,
        )
        .unwrap();

        let value = generate("2.1.0-beta.3", &notes_path);
        let _ = fs::remove_dir_all(root);
        let config = Alcomd3Config::load().unwrap();
        let release_url = config.release_tag_url("2.1.0-beta.3");

        assert_eq!(
            value["notes_i18n"]["en"],
            format!("Short English update summary.\n\n{release_url}")
        );
        assert_eq!(
            value["notes_i18n"]["zh_hans"],
            format!("简短中文更新摘要。\n\n{release_url}")
        );
        assert!(value["notes_i18n"]["ja"].is_null());
    }

    #[test]
    fn updater_notes_reject_unsupported_language() {
        let root = temp_dir("unsupported-language");
        let notes_path = root.join("notes.json");
        fs::write(&notes_path, r#"{ "es": "Resumen." }"#).unwrap();

        let error = read_updater_notes_i18n_file(&notes_path).unwrap_err();
        let _ = fs::remove_dir_all(root);

        assert!(
            error
                .to_string()
                .contains("unsupported updater notes language")
        );
    }

    #[test]
    fn updater_notes_reject_empty_string() {
        let root = temp_dir("empty-string");
        let notes_path = root.join("notes.json");
        fs::write(&notes_path, r#"{ "en": "   " }"#).unwrap();

        let error = read_updater_notes_i18n_file(&notes_path).unwrap_err();
        let _ = fs::remove_dir_all(root);

        assert!(error.to_string().contains("must not be empty"));
    }

    #[test]
    fn updater_notes_reject_non_object_json() {
        let root = temp_dir("non-object");
        let notes_path = root.join("notes.json");
        fs::write(&notes_path, r#"["not an object"]"#).unwrap();

        let error = read_updater_notes_i18n_file(&notes_path).unwrap_err();
        let _ = fs::remove_dir_all(root);

        assert!(error.to_string().contains("must be a JSON object"));
    }
}
