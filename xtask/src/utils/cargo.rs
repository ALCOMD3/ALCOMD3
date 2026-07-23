use cargo_metadata::Metadata;
use cargo_metadata::semver::Version;
use std::sync::OnceLock;

use crate::release_common::{
    GH_TOKEN_ENV, GITHUB_TOKEN_ENV, UPDATER_PRIVATE_KEY_ENV, UPDATER_PRIVATE_KEY_PASSWORD_ENV,
};

#[allow(dead_code)]
pub fn cargo_metadata() -> &'static Metadata {
    static CACHE: OnceLock<Metadata> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut command = ::cargo_metadata::MetadataCommand::new();
        for name in [
            GH_TOKEN_ENV,
            GITHUB_TOKEN_ENV,
            UPDATER_PRIVATE_KEY_ENV,
            UPDATER_PRIVATE_KEY_PASSWORD_ENV,
        ] {
            command.env_remove(name);
        }
        command.exec().expect("cargo metadata failed")
    })
}

pub fn gui_version() -> &'static Version {
    cargo_metadata()
        .packages
        .iter()
        .find(|p| p.name == "vrc-get-gui")
        .map(|p| &p.version)
        .expect("vrc-get-gui metadata not found")
}
