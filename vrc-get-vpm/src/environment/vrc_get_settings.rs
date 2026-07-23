use super::VRC_GET_SETTINGS_PATH;
use crate::io;
use crate::io::{DefaultEnvironmentIo, IoTrait};
use crate::utils::{parse_json_file, read_to_end};
use serde::{Deserialize, Serialize};

/// since this file is vrc-get specific, additional keys can be removed
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AsJson {
    #[serde(default)]
    ignore_official_repository: bool,
    #[serde(default)]
    ignore_curated_repository: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct VrcGetSettings {
    parsed: AsJson,
}

impl VrcGetSettings {
    pub async fn load(io: &DefaultEnvironmentIo) -> io::Result<Self> {
        //let parsed = load_json_or_default(io, JSON_PATH.as_ref()).await?;

        let parsed = Self::load_inner(io, VRC_GET_SETTINGS_PATH)
            .await?
            .unwrap_or_default();

        Ok(Self { parsed })
    }

    async fn load_inner(io: &DefaultEnvironmentIo, path: &str) -> io::Result<Option<AsJson>> {
        match io.open(path.as_ref()).await {
            Ok(file) => Ok(Some(match read_to_end(file).await? {
                vec if vec.is_empty() => Default::default(),
                vec => parse_json_file(&vec, path.as_ref())?,
            })),
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => return Err(e),
        }
    }

    pub fn ignore_official_repository(&self) -> bool {
        self.parsed.ignore_official_repository
    }

    pub fn ignore_curated_repository(&self) -> bool {
        self.parsed.ignore_curated_repository
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::LEGACY_VRC_GET_SETTINGS_PATH;
    use std::path::{Path, PathBuf};

    fn temp_environment() -> (DefaultEnvironmentIo, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "alcomd3-vrc-get-settings-test-{}",
            uuid::Uuid::new_v4().as_simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        (
            DefaultEnvironmentIo::new(root.clone().into_boxed_path()),
            root,
        )
    }

    async fn remove_temp_environment(root: PathBuf) {
        tokio::fs::remove_dir_all(root).await.ok();
    }

    #[tokio::test]
    async fn load_ignores_legacy_vrc_get_settings_path() {
        let (io, root) = temp_environment();
        let legacy_path = io.resolve(Path::new(LEGACY_VRC_GET_SETTINGS_PATH));
        tokio::fs::create_dir_all(legacy_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &legacy_path,
            br#"{"ignoreOfficialRepository":true,"ignoreCuratedRepository":true}"#,
        )
        .await
        .unwrap();

        let settings = VrcGetSettings::load(&io).await.unwrap();

        assert!(!settings.ignore_official_repository());
        assert!(!settings.ignore_curated_repository());
        remove_temp_environment(root).await;
    }

    #[tokio::test]
    async fn load_reads_current_vrc_get_settings_path() {
        let (io, root) = temp_environment();
        let current_path = io.resolve(Path::new(VRC_GET_SETTINGS_PATH));
        tokio::fs::create_dir_all(current_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&current_path, br#"{"ignoreOfficialRepository":true}"#)
            .await
            .unwrap();

        let settings = VrcGetSettings::load(&io).await.unwrap();

        assert!(settings.ignore_official_repository());
        assert!(!settings.ignore_curated_repository());
        remove_temp_environment(root).await;
    }
}
