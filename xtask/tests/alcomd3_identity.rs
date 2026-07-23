use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a workspace parent")
        .to_path_buf()
}

fn read_workspace_file(path: &str) -> String {
    fs::read_to_string(workspace_root().join(path)).expect("workspace file is readable")
}

fn parse_toml(path: &str) -> DocumentMut {
    read_workspace_file(path)
        .parse::<DocumentMut>()
        .expect("workspace TOML is valid")
}

fn alcomd3_config() -> serde_json::Value {
    serde_json::from_str(&read_workspace_file("alcomd3.config.json"))
        .expect("alcomd3.config.json is valid JSON")
}

fn config_str<'a>(config: &'a serde_json::Value, key: &str) -> &'a str {
    config
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("alcomd3.config.json has string `{key}`"))
}

fn config_nested_str<'a>(config: &'a serde_json::Value, parent: &str, child: &str) -> &'a str {
    config
        .get(parent)
        .and_then(|value| value.get(child))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("alcomd3.config.json has string `{parent}.{child}`"))
}

fn parse_key_value_lines(contents: &str, separator: char) -> HashMap<String, String> {
    contents
        .lines()
        .filter_map(|line| line.split_once(separator))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

#[test]
fn gui_cargo_and_tauri_configs_use_alcomd3_binary_name() {
    let config = alcomd3_config();
    let product_name = config_str(&config, "productName");
    let main_binary_name = config_str(&config, "mainBinaryName");
    let tauri_identifier = config_str(&config, "tauriIdentifier");

    let cargo = parse_toml("vrc-get-gui/Cargo.toml");
    let bins = cargo["bin"]
        .as_array_of_tables()
        .expect("vrc-get-gui declares a bin target");
    let gui_bin = bins
        .iter()
        .find(|bin| bin["path"].as_str() == Some("src/main.rs"))
        .expect("vrc-get-gui bin target exists");

    assert_eq!(gui_bin["name"].as_str(), Some(main_binary_name));

    let tauri = parse_toml("vrc-get-gui/Tauri.toml");
    assert_eq!(tauri["productName"].as_str(), Some(product_name));
    assert_eq!(tauri["mainBinaryName"].as_str(), Some(main_binary_name));
    assert_eq!(tauri["identifier"].as_str(), Some(tauri_identifier));
}

#[test]
fn platform_install_metadata_uses_alcomd3_identity() {
    let config = alcomd3_config();
    let product_name = config_str(&config, "productName");
    let main_binary_name = config_str(&config, "mainBinaryName");
    let package_name = config_str(&config, "packageName");
    let mcp_binary_name = config_str(&config, "mcpBinaryName");
    let publisher_name = config_str(&config, "publisherName");
    let homepage_url = config_str(&config, "homepageUrl");
    let windows_app_id = config_str(&config, "windowsAppId");
    let windows_aumid = config_str(&config, "windowsAumid");
    let legacy_windows_app_id = config_str(&config, "legacyWindowsAppId");
    let legacy_tauri_identifier = config_str(&config, "legacyTauriIdentifier");
    let legacy_windows_migration_release_tag =
        config_str(&config, "legacyWindowsMigrationReleaseTag");
    let legacy_windows_executable_name = config_str(&config, "legacyWindowsExecutableName");
    let short_description = config_str(&config, "shortDescription");
    let long_description = config_str(&config, "longDescription");
    let template_name = config_nested_str(&config, "templateAssociation", "name");
    let template_extension = config_nested_str(&config, "templateAssociation", "extension");
    let template_key = config_nested_str(&config, "templateAssociation", "key");

    let windows_setup = read_workspace_file("vrc-get-gui/bundle/windows-setup.iss");
    assert!(windows_setup.contains(&format!(r#"#define MyAppName "{product_name}""#)));
    assert!(windows_setup.contains(r#"#define LegacyAppName "ALCOM""#));
    assert!(windows_setup.contains(&format!(r#"#define MyAppPublisher "{publisher_name}""#)));
    assert!(windows_setup.contains(&format!(r#"#define MyAppURL "{homepage_url}""#)));
    assert!(windows_setup.contains(&format!(r#"#define MyAppExeName "{main_binary_name}.exe""#)));
    assert!(windows_setup.contains(r#"UninstallDisplayName={#MyAppName}"#));
    assert!(!windows_setup.contains(r#"#define MyAppExeName "ALCOM.exe""#));
    assert!(windows_setup.contains(&format!(
        r#"#define LegacyAlcomd3ExeName "{legacy_windows_executable_name}""#
    )));
    assert!(windows_setup.contains(&format!(r#"#define MyMcpExeName "{mcp_binary_name}.exe""#)));
    assert!(windows_setup.contains(&format!(r#"#define MyAppAssocName "{template_name}""#)));
    assert!(windows_setup.contains(&format!(r#"#define MyAppAssocExt "{template_extension}""#)));
    assert!(windows_setup.contains(&format!(r#"#define MyAppAssocKey "{template_key}""#)));
    assert!(windows_setup.contains(r#"AppId={#WindowsAppId}"#));
    assert!(windows_setup.contains(r#"AppUserModelID: "{#WindowsAumid}""#));
    assert!(windows_setup.contains(r#"ValueName: "AppUserModelID"; ValueData: "{#WindowsAumid}""#));
    assert!(!windows_setup.contains(windows_app_id.trim_matches(['{', '}'])));
    assert!(!windows_setup.contains(windows_aumid));
    assert!(!windows_setup.contains(legacy_windows_app_id.trim_matches(['{', '}'])));
    assert!(!windows_setup.contains(legacy_tauri_identifier));
    assert!(windows_setup.contains(r#"LegacyUninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#LegacyWindowsAppId}_is1'"#));
    assert!(
        windows_setup.contains("function PrepareToInstall(var NeedsRestart: Boolean): string;")
    );
    assert!(windows_setup.contains("'QuietUninstallString'"));
    assert!(windows_setup.contains("'UninstallString'"));
    assert!(windows_setup.contains("'Inno Setup: App Path'"));
    assert!(windows_setup.contains("RegDeleteKeyIncludingSubkeys(RootKey, LegacyUninstallKey)"));
    assert!(windows_setup.contains("function HasLegacyRegistration: Boolean;"));
    assert!(windows_setup.contains("function HasLegacyDesktopShortcut: Boolean;"));
    assert!(windows_setup.contains("CreateOleObject('WScript.Shell')"));
    assert!(windows_setup.contains("Shortcut.TargetPath"));
    assert!(windows_setup.contains("IsLegacyExecutableTarget"));
    assert!(windows_setup.contains("procedure ApplyLegacyDesktopShortcutDefault;"));
    assert!(windows_setup.contains("WizardSelectTasks('desktopicon')"));
    assert!(windows_setup.contains("procedure CurPageChanged(CurPageID: Integer);"));
    assert!(windows_setup.contains("CurPageID = wpSelectTasks"));
    assert!(windows_setup.contains("if WizardSilent then"));
    assert!(windows_setup.contains("function CleanupLegacyShortcuts: string;"));
    assert!(windows_setup.contains("ExpandConstant('{userdesktop}')"));
    assert!(windows_setup.contains("ExpandConstant('{commondesktop}')"));
    assert!(windows_setup.contains("ExpandConstant('{userprograms}')"));
    assert!(windows_setup.contains("ExpandConstant('{commonprograms}')"));
    assert!(windows_setup.contains("CleanupLegacyRegistration(HKCU"));
    assert!(windows_setup.contains("HKLM64,"));
    assert!(windows_setup.contains("HKLM32,"));
    assert!(windows_setup.contains("ExecAsOriginalUser("));
    assert!(windows_setup.contains("DeleteLegacyExecutableAsOriginalUser"));
    assert!(windows_setup.contains("'/D /V:OFF /C DEL /F /Q \"' + FilePath + '\"'"));
    assert!(windows_setup.contains("function CleanupLegacyTauriData: string;"));
    assert!(windows_setup.contains("RMDIR /S /Q \"%LOCALAPPDATA%\\{#LegacyTauriIdentifier}\""));
    assert!(windows_setup.contains("Result := CleanupLegacyTauriData;"));
    assert!(windows_setup.contains("RunAsOriginalUser and IsAdmin"));
    assert!(windows_setup.contains(r#"Type: files; Name: "{app}\{#LegacyAlcomd3ExeName}""#));
    assert!(windows_setup.contains(r#"CloseApplications=yes"#));
    assert!(windows_setup.contains(
        r#"CloseApplicationsFilter={#MyAppExeName},{#LegacyAlcomd3ExeName},{#MyMcpExeName}"#
    ));
    assert!(windows_setup.contains(r#"Parameters: "/F /T /IM ""{#MyAppExeName}"""#));
    assert!(windows_setup.contains(r#"Parameters: "/F /T /IM ""{#LegacyAlcomd3ExeName}"""#));
    assert!(windows_setup.contains(r#"Parameters: "/F /T /IM ""{#MyMcpExeName}"""#));
    assert!(windows_setup.contains(r#"Type: dirifempty; Name: "{app}""#));
    assert!(!windows_setup.contains(r#"{autoprograms}\ALCOM.lnk"#));
    assert!(!windows_setup.contains(r#"{autodesktop}\ALCOM.lnk"#));
    assert!(!windows_setup.contains(r#"Software\anatawa12\vrc-get-gui"#));
    assert!(!windows_setup.contains(r#"Uninstall\ALCOM"#));

    let setup_builder = read_workspace_file("xtask/src/bundle_alcom/setup_exe.rs");
    assert!(setup_builder.contains("-DWindowsAppId={windows_app_id}"));
    assert!(setup_builder.contains("-DWindowsAumid={}"));
    assert!(setup_builder.contains("-DLegacyWindowsAppId={}"));
    assert!(setup_builder.contains("-DLegacyTauriIdentifier={}"));

    let updater = read_workspace_file("vrc-get-gui/src/updater.rs");
    assert!(updater.contains("crate::alcomd3_config::windows_app_id()"));
    assert!(!updater.contains(windows_app_id.trim_matches(['{', '}'])));
    assert!(!updater.contains(legacy_windows_app_id.trim_matches(['{', '}'])));

    let full_chain = read_workspace_file(".github/workflows/full-chain.yml");
    assert!(!full_chain.contains(legacy_windows_migration_release_tag));
    assert!(full_chain.contains(".github/scripts/resolve-windows-migration-installer.ps1"));
    assert!(full_chain.contains(".github/scripts/verify-installer-zip.ps1"));
    assert!(full_chain.contains("cargo xtask release-assemble"));

    let installer_smoke = read_workspace_file(".github/scripts/installer-smoke.ps1");
    assert!(installer_smoke.contains("[switch] $CreateDesktopIcon"));
    assert!(installer_smoke.contains("function New-TestShortcut"));
    assert!(installer_smoke.contains("function Assert-ShortcutTarget"));
    assert!(installer_smoke.contains("function Assert-ShortcutAppUserModelId"));
    assert!(installer_smoke.contains("function Assert-ShortcutRemoved"));
    assert!(installer_smoke.contains("$config.windowsAumid"));
    assert!(installer_smoke.contains("$config.legacyTauriIdentifier"));
    assert!(installer_smoke.contains("Legacy Tauri data directory was not removed"));
    assert!(installer_smoke.contains("[ValidateSet('Baseline', 'Current')]"));
    assert_eq!(installer_smoke.matches("-Phase Baseline").count(), 1);
    assert_eq!(installer_smoke.matches("-Phase Current").count(), 1);
    let baseline_validation = installer_smoke.find("-Phase Baseline").unwrap();
    let current_validation = installer_smoke.find("-Phase Current").unwrap();
    assert!(baseline_validation < current_validation);

    let migration_installer =
        read_workspace_file(".github/scripts/resolve-windows-migration-installer.ps1");
    assert!(migration_installer.contains("legacyWindowsMigrationReleaseTag"));
    assert!(!migration_installer.contains(legacy_windows_migration_release_tag));
    assert!(migration_installer.contains("releases/tags/$encodedTag"));
    assert!(!migration_installer.contains("per_page=100"));

    let release_draft = read_workspace_file(".github/workflows/release-draft.yml");
    assert!(release_draft.contains(".github/scripts/resolve-windows-migration-installer.ps1"));
    assert!(release_draft.contains(".github/scripts/verify-installer-zip.ps1"));
    let release_smoke = release_draft
        .find(".github/scripts/installer-smoke.ps1")
        .unwrap();
    let upload_windows_shard = release_draft.find("Upload Windows shard").unwrap();
    assert!(release_smoke < upload_windows_shard);

    let gui_main = read_workspace_file("vrc-get-gui/src/main.rs");
    assert!(gui_main.contains("set_current_process_app_user_model_id"));
    assert!(gui_main.contains("alcomd3_config::windows_aumid()"));

    let deep_link_support = read_workspace_file("vrc-get-gui/src/deep_link_support.rs");
    assert!(deep_link_support.contains(r#"set_value("AppUserModelID""#));
    assert!(deep_link_support.contains("alcomd3_config::windows_aumid()"));

    let desktop = parse_key_value_lines(
        &read_workspace_file("vrc-get-gui/bundle/alcomd3.desktop"),
        '=',
    );
    assert_eq!(desktop.get("Name").map(String::as_str), Some(product_name));
    assert_eq!(desktop.get("Icon").map(String::as_str), Some(package_name));
    assert_eq!(
        desktop.get("Comment").map(String::as_str),
        Some(short_description)
    );

    let deb_control =
        parse_key_value_lines(&read_workspace_file("vrc-get-gui/bundle/deb-control"), ':');
    assert_eq!(
        deb_control.get("Package").map(String::as_str),
        Some(package_name)
    );
    assert_eq!(
        deb_control.get("Homepage").map(String::as_str),
        Some(homepage_url)
    );
    assert_eq!(
        deb_control.get("Description").map(String::as_str),
        Some(short_description)
    );
    let deb_dependencies = deb_control
        .get("Depends")
        .expect("DEB control should declare runtime dependencies");
    assert!(deb_dependencies.contains("libc6 (>= {{libc_version}})"));
    assert!(deb_dependencies.contains("libgcc-s1 (>= {{libgcc_version}})"));
    assert_eq!(deb_dependencies.matches("libc6 (").count(), 1);
    assert!(read_workspace_file("vrc-get-gui/bundle/deb-control").contains(long_description));
}

#[test]
fn windows_setup_extracts_webview2_before_installing_it() {
    let windows_setup = read_workspace_file("vrc-get-gui/bundle/windows-setup.iss");

    assert!(windows_setup.contains(
        r#"Source: "{#WebView2SetupPath}"; DestName: "MicrosoftEdgeWebView2Setup.exe"; Flags: dontcopy noencryption"#
    ));
    let extract_position = windows_setup
        .find("ExtractTemporaryFile('MicrosoftEdgeWebView2Setup.exe');")
        .expect("WebView2 bootstrapper is explicitly extracted");
    let exec_position = windows_setup
        .find("if not Exec(")
        .expect("WebView2 bootstrapper is executed");
    assert!(
        extract_position < exec_position,
        "WebView2 bootstrapper must be extracted before it is executed"
    );
    assert!(!windows_setup.contains(
        r#"DestName: "MicrosoftEdgeWebView2Setup.exe"; DestDir: "{tmp}"; Flags: deleteafterinstall"#
    ));
}

#[test]
fn website_site_config_uses_shared_alcomd3_config() {
    let site_config = read_workspace_file("website/src/data/site.config.mjs");
    let downloads = read_workspace_file("website/src/data/downloads.mjs");

    for expected in [
        "alcomd3.config.json",
        "homepageUrl",
        "productName",
        "publisherName",
        "repository",
        "createDownloadCatalog",
        "createStableDownloadCatalog",
        "stableRelease",
        "betaDownloads",
    ] {
        assert!(
            site_config.contains(expected),
            "website site.config.mjs should reference shared config field `{expected}`"
        );
    }

    for expected in ["releasePlatforms", "assetPattern"] {
        assert!(
            downloads.contains(expected),
            "website downloads.mjs should consume shared release field `{expected}`"
        );
    }
}

#[test]
fn gui_and_mcp_use_shared_application_paths() {
    let environment_io = read_workspace_file("vrc-get-vpm/src/io/tokio.rs");
    assert!(
        environment_io.contains("alcomd3_app_paths::alcomd3_data_dir()"),
        "DefaultEnvironmentIo must use the shared ALCOMD3 data directory resolver"
    );

    let mcp_protocol = read_workspace_file("alcomd3-mcp-protocol/src/lib.rs");
    assert!(
        mcp_protocol.contains("alcomd3_app_paths::mcp_endpoint_file_path()"),
        "MCP endpoint lookup must use the shared ALCOMD3 data directory resolver"
    );
}

#[test]
fn mcp_docs_reference_current_official_protocol_version() {
    let docs = read_workspace_file("docs/mcp.md");

    assert!(
        docs.contains("2025-11-25"),
        "MCP docs should reference the current official protocol version"
    );
    assert!(
        !docs.contains("2025-06-18"),
        "MCP docs should not keep the previous protocol version as the active target"
    );
}
