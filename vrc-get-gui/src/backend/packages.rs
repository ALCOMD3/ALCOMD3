use indexmap::IndexSet;
use std::collections::BTreeMap;
use vrc_get_vpm::environment::{CURATED_REPOSITORY_ID, OFFICIAL_REPOSITORY_ID};
use vrc_get_vpm::repository::LocalCachedRepository;
use vrc_get_vpm::{PackageInfo, PackageManifest};

pub(crate) fn latest_package_infos_by_source<'package, 'env>(
    packages: impl IntoIterator<Item = &'package PackageInfo<'env>>,
) -> Vec<&'package PackageInfo<'env>>
where
    'env: 'package,
{
    let mut latest_by_source = BTreeMap::<String, &'package PackageInfo<'env>>::new();
    for package in packages {
        let key = package_source_key(package);
        latest_by_source
            .entry(key)
            .and_modify(|existing| {
                if package.version() > existing.version() {
                    *existing = package;
                }
            })
            .or_insert(package);
    }

    latest_by_source.into_values().collect()
}

fn package_source_key(package: &PackageInfo<'_>) -> String {
    let source = if let Some(repo) = package.repo().and_then(repository_id) {
        format!("remote:{repo}")
    } else {
        "local:user".to_string()
    };
    format!("{}\0{}", package.name(), source)
}

pub(crate) fn repository_id(repo: &LocalCachedRepository) -> Option<&str> {
    repo.id().or(repo.url().map(url::Url::as_str))
}

pub(crate) fn repository_kind(repo: &LocalCachedRepository) -> &'static str {
    match repository_id(repo) {
        Some(OFFICIAL_REPOSITORY_ID) => "officialDefault",
        Some(CURATED_REPOSITORY_ID) => "curatedDefault",
        _ => "user",
    }
}

pub(crate) fn package_source_kind(repo: &LocalCachedRepository) -> &'static str {
    match repository_kind(repo) {
        "officialDefault" => "officialDefault",
        "curatedDefault" => "curatedDefault",
        _ => "userRepository",
    }
}

pub(crate) fn repository_is_default(kind: &str) -> bool {
    matches!(kind, "officialDefault" | "curatedDefault")
}

pub(crate) fn package_is_available_for_display(
    package: &PackageInfo<'_>,
    show_prerelease_packages: bool,
) -> bool {
    package_manifest_is_available_for_display(package.package_json(), show_prerelease_packages)
}

pub(crate) fn package_manifest_is_available_for_display(
    package: &PackageManifest,
    show_prerelease_packages: bool,
) -> bool {
    (show_prerelease_packages || !package.version().is_pre()) && !package.is_yanked()
}

pub(crate) fn package_is_visible_with_gui_filters(
    package: &PackageInfo<'_>,
    hidden_user_repositories: &IndexSet<String>,
    hide_local_user_packages: bool,
    show_prerelease_packages: bool,
) -> bool {
    if !package_is_available_for_display(package, show_prerelease_packages) {
        return false;
    }

    let Some(repo) = package.repo() else {
        return !hide_local_user_packages;
    };
    let Some(repo_id) = repository_id(repo) else {
        return true;
    };
    !hidden_user_repositories
        .iter()
        .any(|hidden| hidden == repo_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use serde_json::{Value, json};
    use std::path::Path;
    use vrc_get_vpm::repository::RemoteRepository;

    #[test]
    fn package_manifest_availability_matches_prerelease_and_yanked_filters() {
        let stable = test_package_manifest(json!({
            "name": "com.example.stable",
            "version": "1.0.0",
        }));
        let prerelease = test_package_manifest(json!({
            "name": "com.example.prerelease",
            "version": "1.0.0-beta.1",
        }));
        let yanked = test_package_manifest(json!({
            "name": "com.example.yanked",
            "version": "1.0.0",
            "vrc-get": {
                "yanked": true
            }
        }));

        assert!(package_manifest_is_available_for_display(&stable, false));
        assert!(!package_manifest_is_available_for_display(
            &prerelease,
            false
        ));
        assert!(package_manifest_is_available_for_display(&prerelease, true));
        assert!(!package_manifest_is_available_for_display(&yanked, true));
    }

    #[test]
    fn package_visibility_respects_gui_local_user_package_filter() {
        let manifest = test_package_manifest(json!({
            "name": "com.example.local",
            "version": "1.0.0",
        }));
        let package_path = Path::new("Packages/com.example.local");
        let package = PackageInfo::local(&manifest, package_path);
        let hidden_user_repositories = IndexSet::new();

        assert!(package_is_visible_with_gui_filters(
            &package,
            &hidden_user_repositories,
            false,
            false
        ));
        assert!(!package_is_visible_with_gui_filters(
            &package,
            &hidden_user_repositories,
            true,
            false
        ));
    }

    #[test]
    fn package_visibility_respects_gui_hidden_repository_filter() {
        let manifest = test_package_manifest(json!({
            "name": "com.example.remote",
            "version": "1.0.0",
        }));
        let repository = test_cached_repository(json!({
            "id": "com.example.repo",
            "url": "https://example.com/index.json",
            "packages": {}
        }));
        let package = PackageInfo::remote(&manifest, &repository);
        let mut hidden_user_repositories = IndexSet::new();
        hidden_user_repositories.insert("com.example.repo".to_string());

        assert!(package_is_visible_with_gui_filters(
            &package,
            &IndexSet::new(),
            false,
            false
        ));
        assert!(!package_is_visible_with_gui_filters(
            &package,
            &hidden_user_repositories,
            false,
            false
        ));
    }

    fn test_package_manifest(value: Value) -> PackageManifest {
        serde_json::from_value(value).unwrap()
    }

    fn test_cached_repository(value: Value) -> LocalCachedRepository {
        let Value::Object(repository) = value else {
            panic!("expected repository object");
        };
        LocalCachedRepository::new(
            RemoteRepository::parse(repository).unwrap(),
            IndexMap::new(),
        )
    }
}
