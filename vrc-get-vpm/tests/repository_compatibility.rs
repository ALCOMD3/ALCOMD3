use serde_json::json;
use vrc_get_vpm::repositories_file::RepositoriesFile;
use vrc_get_vpm::repository::RemoteRepository;
use vrc_get_vpm::version::Version;

#[test]
fn repositories_file_parses_direct_and_vcc_links_with_headers() {
    let input = r#"
        # Direct repository
        https://example.com/direct.json

        # VCC deep link with percent-encoded URL and headers
        vcc://vpm/addRepo?url=https%3A%2F%2Fexample.com%2Fprivate.json&headers%5B%5D=Authorization%3ABearer%20test-token&headers%5B%5D=X-Channel%3Abeta

        ftp://example.com/unsupported.json
        vcc://other/addRepo?url=https%3A%2F%2Fexample.com%2Fwrong-host.json
        vcc://vpm/addRepo?url=https%3A%2F%2Fexample.com%2Fone.json&url=https%3A%2F%2Fexample.com%2Ftwo.json
        not a URL
    "#;

    let result = RepositoriesFile::parse(input);
    let repositories = result.parsed().repositories();

    assert_eq!(repositories.len(), 2);
    assert_eq!(
        repositories[0].url().as_str(),
        "https://example.com/direct.json"
    );
    assert!(repositories[0].headers().is_empty());
    assert_eq!(
        repositories[1].url().as_str(),
        "https://example.com/private.json"
    );
    assert_eq!(
        repositories[1]
            .headers()
            .get("Authorization")
            .map(AsRef::as_ref),
        Some("Bearer test-token")
    );
    assert_eq!(
        repositories[1]
            .headers()
            .get("X-Channel")
            .map(AsRef::as_ref),
        Some("beta")
    );
    assert_eq!(result.unparseable_lines().len(), 4);
}

#[test]
fn remote_repository_keeps_valid_versions_when_one_manifest_is_malformed() {
    let repository = RemoteRepository::parse(
        json!({
            "name": "Compatibility Test Repository",
            "id": "com.example.compatibility",
            "url": "https://example.com/vpm.json",
            "packages": {
                "com.example.package": {
                    "versions": {
                        "1.0.0": {
                            "name": "com.example.package",
                            "version": "1.0.0"
                        },
                        "1.1.0": {
                            "name": "../invalid-package-name",
                            "version": "1.1.0"
                        }
                    }
                }
            }
        })
        .as_object()
        .unwrap()
        .clone(),
    )
    .unwrap();

    let versions = repository
        .get_versions_of("com.example.package")
        .collect::<Vec<_>>();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].name(), "com.example.package");
    assert_eq!(versions[0].version(), &Version::new(1, 0, 0));
    assert!(
        repository
            .get_package_version("com.example.package", &Version::new(1, 1, 0))
            .is_none()
    );
}
