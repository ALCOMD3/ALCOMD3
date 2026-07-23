//! The vpm client library.
//!
//! TODO: documentation

#![deny(unsafe_code)]

use std::fmt::Display;
use std::path::Path;

use indexmap::IndexMap;
use serde_repr::{Deserialize_repr, Serialize_repr};

use version::{ReleaseType, UnityVersion, Version, VersionRange};

pub mod environment;
pub mod io;
mod package_manifest;
pub mod repository;
mod structs;
mod traits;
pub mod unity_project;
mod utils;
pub mod version;
mod version_selector;

pub mod repositories_file;

#[cfg(feature = "unity")]
pub mod unity;
#[cfg(feature = "unity-hub")]
pub mod unity_hub;

use crate::repository::local::LocalCachedRepository;

pub use package_manifest::PackageManifest;
pub use package_manifest::PartialUnityVersion;
pub use package_manifest::is_valid_package_name;
pub use structs::setting::UserRepoSetting;
pub use traits::AbortCheck;
pub use traits::HttpClient;
pub use traits::PackageCollection;
pub use traits::PackageInstallProgress;
pub use traits::PackageInstallProgressKind;
pub use traits::PackageInstaller;
pub use unity_project::UnityProject;
pub use version_selector::VersionSelector;

pub const VRCHAT_RECOMMENDED_2022_UNITY: UnityVersion = UnityVersion::new_f1(2022, 3, 22);
pub const VRCHAT_RECOMMENDED_2022_UNITY_HUB_LINK: &str = "unityhub://2022.3.22f1/887be4894c44";

#[derive(Copy, Clone)]
pub struct PackageInfo<'a> {
    inner: PackageInfoInner<'a>,
}

impl std::fmt::Debug for PackageInfo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[derive(Debug)]
        #[allow(dead_code)] // debug only struct
        enum SourceEnum<'a> {
            Local(&'a Path),
            Remote(&'a str),
        }

        let source = match self.inner {
            PackageInfoInner::Remote(_, repo) => SourceEnum::Remote(
                repo.id()
                    .or(repo.url().map(url::Url::as_str))
                    .unwrap_or("(unknown id)"),
            ),
            PackageInfoInner::Local(_, path) => SourceEnum::Local(path),
        };

        f.debug_struct("PackageInfo")
            .field("json", &self.package_json())
            .field("source", &source)
            .finish()
    }
}

#[derive(Copy, Clone)]
enum PackageInfoInner<'a> {
    Remote(&'a PackageManifest, &'a LocalCachedRepository),
    Local(&'a PackageManifest, &'a Path),
}

impl<'a> PackageInfo<'a> {
    pub fn package_json(self) -> &'a PackageManifest {
        // this match will be removed in the optimized code because package.json is exists at first
        match self.inner {
            PackageInfoInner::Remote(pkg, _) => pkg,
            PackageInfoInner::Local(pkg, _) => pkg,
        }
    }

    pub fn remote(json: &'a PackageManifest, repo: &'a LocalCachedRepository) -> Self {
        Self {
            inner: PackageInfoInner::Remote(json, repo),
        }
    }

    pub fn local(json: &'a PackageManifest, path: &'a Path) -> Self {
        Self {
            inner: PackageInfoInner::Local(json, path),
        }
    }

    pub fn repo(self) -> Option<&'a LocalCachedRepository> {
        match self.inner {
            PackageInfoInner::Remote(_, repo) => Some(repo),
            PackageInfoInner::Local(_, _) => None,
        }
    }

    #[allow(unused)]
    pub fn is_remote(self) -> bool {
        matches!(self.inner, PackageInfoInner::Remote(_, _))
    }

    #[allow(unused)]
    pub fn is_local(self) -> bool {
        matches!(self.inner, PackageInfoInner::Local(_, _))
    }

    pub fn name(self) -> &'a str {
        self.package_json().name()
    }

    pub fn version(self) -> &'a Version {
        self.package_json().version()
    }

    pub fn vpm_dependencies(self) -> &'a IndexMap<Box<str>, VersionRange> {
        self.package_json().vpm_dependencies()
    }

    pub fn legacy_packages(self) -> &'a [Box<str>] {
        self.package_json().legacy_packages()
    }

    pub fn unity(self) -> Option<&'a PartialUnityVersion> {
        self.package_json().unity()
    }

    pub fn is_yanked(self) -> bool {
        self.package_json().is_yanked()
    }

    pub fn display_name(self) -> Option<&'a str> {
        self.package_json().display_name()
    }

    pub fn aliases(self) -> &'a [Box<str>] {
        self.package_json().aliases()
    }
    pub fn keywords(self) -> &'a [Box<str>] {
        self.package_json().keywords()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum ProjectType {
    Unknown = 0,
    LegacySdk2 = 1,
    LegacyWorlds = 2,
    LegacyAvatars = 3,
    UpmWorlds = 4,
    UpmAvatars = 5,
    UpmStarter = 6,
    Worlds = 7,
    Avatars = 8,
    VpmStarter = 9,
}

impl ProjectType {
    pub fn from_i32(i: i32) -> Option<ProjectType> {
        match i {
            0 => Some(Self::Unknown),
            1 => Some(Self::LegacySdk2),
            2 => Some(Self::LegacyWorlds),
            3 => Some(Self::LegacyAvatars),
            4 => Some(Self::UpmWorlds),
            5 => Some(Self::UpmAvatars),
            6 => Some(Self::UpmStarter),
            7 => Some(Self::Worlds),
            8 => Some(Self::Avatars),
            9 => Some(Self::VpmStarter),
            _ => None,
        }
    }
}

impl Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => f.write_str("Unknown"),
            Self::LegacySdk2 => f.write_str("Legacy SDK2"),
            Self::LegacyWorlds => f.write_str("Legacy Worlds"),
            Self::LegacyAvatars => f.write_str("Legacy Avatars"),
            Self::UpmWorlds => f.write_str("UPM Worlds"),
            Self::UpmAvatars => f.write_str("UPM Avatars"),
            Self::UpmStarter => f.write_str("UPM Starter"),
            Self::Worlds => f.write_str("Worlds"),
            Self::Avatars => f.write_str("Avatars"),
            Self::VpmStarter => f.write_str("VPM Starter"),
        }
    }
}

pub fn package_manifest_is_unity_compatible(
    package: &PackageManifest,
    unity: UnityVersion,
) -> bool {
    fn is_vrcsdk_package(name: &str) -> bool {
        matches!(
            name,
            "com.vrchat.avatars" | "com.vrchat.worlds" | "com.vrchat.base"
        )
    }

    fn is_resolver_package(name: &str) -> bool {
        name == "com.vrchat.core.vpm-resolver"
    }

    fn is_vrcsdk_for_2019(version: &Version) -> bool {
        version.major == 3 && version.minor <= 4
    }

    fn is_resolver_for_2019(version: &Version) -> bool {
        version.major == 0 && version.minor == 1 && version.patch <= 26
    }

    fn matches_unity_minor(package: &PackageManifest, unity: UnityVersion) -> Option<bool> {
        let target_unity = package.unity()?;
        Some(target_unity.major() == unity.major() && target_unity.minor() == unity.minor())
    }

    match package.name() {
        name if is_vrcsdk_package(name) => {
            if is_vrcsdk_for_2019(package.version()) {
                // This version of VRCSDK is only for Unity 2019.
                unity.major() == 2019
            } else {
                matches_unity_minor(package, unity).unwrap_or(true)
            }
        }
        name if is_resolver_package(name) => {
            if is_resolver_for_2019(package.version()) {
                // This version of Resolver is only for Unity 2019.
                unity.major() == 2019
            } else {
                matches_unity_minor(package, unity).unwrap_or(true)
            }
        }
        _ => {
            // Otherwise, check based on package info.

            if let Some(min_unity) = package.unity() {
                unity
                    >= UnityVersion::new(
                        min_unity.major(),
                        min_unity.minor(),
                        0,
                        ReleaseType::Alpha,
                        0,
                    )
            } else {
                // if there are no info, satisfies for all unity versions
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_with_unity(name: &str, version: &str, unity: &str) -> PackageManifest {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "version": version,
            "unity": unity,
        }))
        .unwrap()
    }

    #[test]
    fn vrcsdk_requires_matching_unity_minor() {
        let package = package_with_unity("com.vrchat.avatars", "3.10.0", "2022.3");

        assert!(package_manifest_is_unity_compatible(
            &package,
            UnityVersion::parse("2022.3.22f1").unwrap()
        ));
        assert!(!package_manifest_is_unity_compatible(
            &package,
            UnityVersion::parse("6000.0.1f1").unwrap()
        ));
    }

    #[test]
    fn resolver_requires_matching_unity_minor_when_manifest_declares_unity() {
        let package = package_with_unity("com.vrchat.core.vpm-resolver", "0.1.30", "2022.3");

        assert!(package_manifest_is_unity_compatible(
            &package,
            UnityVersion::parse("2022.3.22f1").unwrap()
        ));
        assert!(!package_manifest_is_unity_compatible(
            &package,
            UnityVersion::parse("6000.0.1f1").unwrap()
        ));
    }

    #[test]
    fn normal_packages_keep_minimum_unity_behavior() {
        let package = package_with_unity("com.example.tool", "1.0.0", "2022.3");

        assert!(package_manifest_is_unity_compatible(
            &package,
            UnityVersion::parse("6000.0.1f1").unwrap()
        ));
    }
}
