use crate::utils::build_dir;
use crate::utils::command::CommandExt;
use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

const AD_HOC_SIGNING_IDENTITY: &str = "-";

/// Ad-hoc signs a macOS application bundle or final DMG.
///
/// The application mode is intended to run **after** `bundle-alcom --bundles app`
/// and **before** `bundle-alcom --bundles dmg,app-updater`. It signs every helper
/// executable in `Contents/MacOS`, then the main executable, and finally the app
/// bundle. This inside-out ordering ensures that the DMG and updater payload are
/// built from an already signed application.
///
/// Run the command a second time with `--dmg <path>` after creating the final DMG.
/// DMG mode signs and verifies only that disk image. This command intentionally
/// has no certificate, identity, secure timestamp, or notarization options.
#[derive(clap::Parser)]
pub(super) struct Command {
    /// Target triple (e.g. `aarch64-apple-darwin`).
    ///
    /// Defaults to the host triple. Ignored in DMG mode.
    #[arg(long, conflicts_with = "dmg")]
    target: Option<String>,

    /// Build profile (default: `release`). Ignored in DMG mode.
    #[arg(long, default_value = "release", conflicts_with = "dmg")]
    profile: String,

    /// Path to a custom entitlements `.plist` file.
    ///
    /// If not provided, a default set of entitlements is generated automatically.
    #[arg(long, conflicts_with = "dmg")]
    entitlements: Option<PathBuf>,

    /// Sign and verify this already-built final DMG instead of the application.
    #[arg(long, value_name = "PATH")]
    dmg: Option<PathBuf>,
}

impl crate::Command for Command {
    fn run(self) -> Result<i32> {
        if let Some(dmg_path) = &self.dmg {
            validate_dmg_path(dmg_path)?;
            sign_dmg(dmg_path)?;
            verify_dmg_signature(dmg_path)?;
            return Ok(0);
        }

        let build_dir = build_dir(self.target.as_deref(), &self.profile);
        let app_path = find_app_bundle(&build_dir.join("bundle/macos"))?;

        let temporary_files = TemporaryDirectory::create("alcomd3-signing-files")?;
        let entitlements_path = match &self.entitlements {
            Some(path) => {
                ensure_regular_file(path, "entitlements plist")?;
                path.clone()
            }
            None => {
                let path = temporary_files.path().join("entitlements.plist");
                write_default_entitlements(&path)?;
                path
            }
        };

        sign_app(&app_path, &entitlements_path)?;
        verify_app_signature(&app_path)?;

        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Temporary files
// ---------------------------------------------------------------------------

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    /// Uses the platform `mktemp` implementation so directory names are generated
    /// from operating-system randomness instead of a predictable process ID.
    fn create(prefix: &str) -> Result<Self> {
        let template = std::env::temp_dir().join(format!("{prefix}.XXXXXXXX"));
        let output = ProcessCommand::new("mktemp")
            .arg("-d")
            .arg(&template)
            .run_capture_checked("creating a private temporary directory")?;
        let path = PathBuf::from(output.trim());
        if path.as_os_str().is_empty() {
            anyhow::bail!("mktemp returned an empty temporary directory path");
        }

        let path = fs::canonicalize(&path)
            .with_context(|| format!("resolving temporary directory {}", path.display()))?;
        let parent = fs::canonicalize(std::env::temp_dir())
            .context("resolving the operating-system temporary directory")?;
        if !path.starts_with(&parent) {
            let _ = fs::remove_dir_all(&path);
            anyhow::bail!(
                "mktemp created a directory outside the operating-system temporary directory: {}",
                path.display()
            );
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).with_context(|| {
                format!(
                    "restricting temporary directory permissions: {}",
                    path.display()
                )
            })?;
        }

        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path)
            && self.path.exists()
        {
            eprintln!(
                "warning: failed to remove temporary signing directory {}: {error:#}",
                self.path.display()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Entitlements
// ---------------------------------------------------------------------------

/// Write a default entitlements plist suitable for a hardened-runtime macOS app.
fn write_default_entitlements(path: &Path) -> Result<()> {
    let mut dict = plist::Dictionary::new();
    dict.insert(
        "com.apple.security.cs.allow-jit".into(),
        plist::Value::Boolean(false),
    );
    dict.insert(
        "com.apple.security.cs.allow-unsigned-executable-memory".into(),
        plist::Value::Boolean(false),
    );
    dict.insert(
        "com.apple.security.cs.disable-library-validation".into(),
        plist::Value::Boolean(false),
    );
    dict.insert(
        "com.apple.security.cs.allow-dyld-environment-variables".into(),
        plist::Value::Boolean(false),
    );
    dict.insert(
        "com.apple.security.network.client".into(),
        plist::Value::Boolean(true),
    );

    plist::to_file_xml(path, &plist::Value::Dictionary(dict))
        .with_context(|| format!("writing entitlements to {}", path.display()))
}

// ---------------------------------------------------------------------------
// Application signing
// ---------------------------------------------------------------------------

struct AppSigningTargets {
    nested_executables: Vec<PathBuf>,
    main_executable: PathBuf,
}

impl AppSigningTargets {
    fn discover(app_path: &Path) -> Result<Self> {
        let contents = app_path.join("Contents");
        let info_plist = contents.join("Info.plist");
        let plist: plist::Value = plist::from_file(&info_plist)
            .with_context(|| format!("reading {}", info_plist.display()))?;
        let main_executable_name = plist
            .as_dictionary()
            .and_then(|dictionary| dictionary.get("CFBundleExecutable"))
            .and_then(plist::Value::as_string)
            .context("Info.plist does not contain a string CFBundleExecutable")?;
        if main_executable_name.is_empty()
            || main_executable_name == "."
            || main_executable_name == ".."
            || main_executable_name.contains('/')
            || main_executable_name.contains('\\')
        {
            anyhow::bail!(
                "Info.plist contains an unsafe CFBundleExecutable value: {main_executable_name:?}"
            );
        }

        let macos_directory = contents.join("MacOS");
        let main_executable = macos_directory.join(main_executable_name);
        ensure_regular_file(&main_executable, "main macOS executable")?;

        let mut nested_executables = Vec::new();
        for entry in fs::read_dir(&macos_directory)
            .with_context(|| format!("reading {}", macos_directory.display()))?
        {
            let entry =
                entry.with_context(|| format!("reading {} entry", macos_directory.display()))?;
            let path = entry.path();
            if path == main_executable {
                continue;
            }
            let file_type = entry
                .file_type()
                .with_context(|| format!("reading file type for {}", path.display()))?;
            if !file_type.is_file() {
                anyhow::bail!(
                    "unsupported nested item in Contents/MacOS; expected a regular helper executable: {}",
                    path.display()
                );
            }
            nested_executables.push(path);
        }
        nested_executables.sort();
        if nested_executables.is_empty() {
            anyhow::bail!(
                "no nested helper executable was found in {}; the ALCOMD3 MCP helper must be bundled and signed separately",
                macos_directory.display()
            );
        }

        Ok(Self {
            nested_executables,
            main_executable,
        })
    }
}

fn find_app_bundle(bundle_directory: &Path) -> Result<PathBuf> {
    let mut app_bundles = Vec::new();
    for entry in fs::read_dir(bundle_directory).with_context(|| {
        format!(
            "reading macOS bundle directory {}",
            bundle_directory.display()
        )
    })? {
        let entry =
            entry.with_context(|| format!("reading {} entry", bundle_directory.display()))?;
        let path = entry.path();
        if entry
            .file_type()
            .with_context(|| format!("reading file type for {}", path.display()))?
            .is_dir()
            && path.extension() == Some(OsStr::new("app"))
        {
            app_bundles.push(path);
        }
    }
    app_bundles.sort();

    match app_bundles.as_slice() {
        [app_path] => Ok(app_path.clone()),
        [] => anyhow::bail!(
            "no .app bundle found in {}; run `bundle-alcom --bundles app` first",
            bundle_directory.display()
        ),
        _ => anyhow::bail!(
            "expected exactly one .app bundle in {}, found {}",
            bundle_directory.display(),
            app_bundles.len()
        ),
    }
}

fn sign_app(app_path: &Path, entitlements: &Path) -> Result<()> {
    let targets = AppSigningTargets::discover(app_path)?;

    for helper in &targets.nested_executables {
        sign_code(helper, None, true)
            .with_context(|| format!("signing nested helper executable {}", helper.display()))?;
        verify_code_signature(helper)?;
    }

    sign_code(&targets.main_executable, Some(entitlements), true).with_context(|| {
        format!(
            "signing main application executable {}",
            targets.main_executable.display()
        )
    })?;
    verify_code_signature(&targets.main_executable)?;

    sign_code(app_path, Some(entitlements), true)
        .with_context(|| format!("signing application bundle {}", app_path.display()))?;

    println!("signed application inside-out: {}", app_path.display());
    Ok(())
}

fn sign_code(path: &Path, entitlements: Option<&Path>, hardened_runtime: bool) -> Result<()> {
    codesign_command(path, entitlements, hardened_runtime)
        .run_checked(&format!("codesigning {}", path.display()))
}

fn codesign_command(
    path: &Path,
    entitlements: Option<&Path>,
    hardened_runtime: bool,
) -> ProcessCommand {
    let mut command = ProcessCommand::new("codesign");
    command.arg("--force");
    if hardened_runtime {
        command.args(["--options", "runtime"]);
    }
    command.arg("--sign").arg(AD_HOC_SIGNING_IDENTITY);
    if let Some(entitlements) = entitlements {
        command.arg("--entitlements").arg(entitlements);
    }
    command.arg(path);
    command
}

fn verify_code_signature(path: &Path) -> Result<()> {
    ProcessCommand::new("codesign")
        .arg("--verify")
        .args(["--strict", "--verbose=4"])
        .arg(path)
        .run_checked(&format!("verifying code signature for {}", path.display()))
}

fn verify_app_signature(app_path: &Path) -> Result<()> {
    let targets = AppSigningTargets::discover(app_path)?;
    for helper in &targets.nested_executables {
        verify_code_signature(helper)?;
        verify_ad_hoc_signature(helper, "nested helper")?;
    }
    verify_code_signature(&targets.main_executable)?;
    verify_ad_hoc_signature(&targets.main_executable, "main executable")?;
    verify_code_signature(app_path)?;
    verify_ad_hoc_signature(app_path, "application")
}

fn verify_ad_hoc_signature(path: &Path, kind: &str) -> Result<()> {
    let output = ProcessCommand::new("codesign")
        .args(["--display", "--verbose=4"])
        .arg(path)
        .output()
        .with_context(|| format!("displaying the {kind} signature for {}", path.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "displaying the {kind} signature failed with {}: {}",
            output.status,
            path.display()
        );
    }
    let details = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{details}");
    if !details.lines().any(|line| line.trim() == "Signature=adhoc") {
        anyhow::bail!("{kind} is not ad-hoc signed: {}", path.display());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// DMG signing
// ---------------------------------------------------------------------------

fn validate_dmg_path(path: &Path) -> Result<()> {
    ensure_regular_file(path, "DMG")?;
    let is_dmg = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dmg"));
    if !is_dmg {
        anyhow::bail!("DMG path must end with .dmg: {}", path.display());
    }
    Ok(())
}

fn sign_dmg(path: &Path) -> Result<()> {
    sign_code(path, None, false)
        .with_context(|| format!("signing final DMG {}", path.display()))?;
    println!("signed final DMG: {}", path.display());
    Ok(())
}

fn verify_dmg_signature(path: &Path) -> Result<()> {
    verify_code_signature(path)?;
    verify_ad_hoc_signature(path, "DMG")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ensure_regular_file(path: &Path, kind: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("reading {kind} metadata at {}", path.display()))?;
    if !metadata.file_type().is_file() {
        anyhow::bail!("{kind} must be a regular file: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create() -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "alcomd3-sign-test-{}-{nonce}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn command_arguments(command: &ProcessCommand) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    fn create_test_app() -> (TestDirectory, PathBuf) {
        let directory = TestDirectory::create();
        let app_path = directory.path().join("Configured Product.app");
        let contents = app_path.join("Contents");
        let macos = contents.join("MacOS");
        fs::create_dir_all(&macos).unwrap();
        fs::write(macos.join("configured-main"), b"main").unwrap();
        fs::write(macos.join("alcomd3-mcp"), b"mcp").unwrap();

        let mut dictionary = plist::Dictionary::new();
        dictionary.insert(
            "CFBundleExecutable".into(),
            plist::Value::String("configured-main".into()),
        );
        plist::to_file_xml(
            contents.join("Info.plist"),
            &plist::Value::Dictionary(dictionary),
        )
        .unwrap();

        (directory, app_path)
    }

    #[test]
    fn dmg_mode_accepts_the_default_profile() {
        let command = Command::try_parse_from(["sign-alcom-app", "--dmg", "Product.dmg"]).unwrap();
        assert_eq!(command.dmg, Some(PathBuf::from("Product.dmg")));
    }

    #[test]
    fn discovers_nested_helper_before_configured_main_executable() {
        let (_directory, app_path) = create_test_app();
        let targets = AppSigningTargets::discover(&app_path).unwrap();
        assert_eq!(
            targets.main_executable,
            app_path.join("Contents/MacOS/configured-main")
        );
        assert_eq!(
            targets.nested_executables,
            vec![app_path.join("Contents/MacOS/alcomd3-mcp")]
        );
    }

    #[test]
    fn rejects_unsafe_main_executable_path_from_plist() {
        let (_directory, app_path) = create_test_app();
        let plist_path = app_path.join("Contents/Info.plist");
        let mut dictionary = plist::Dictionary::new();
        dictionary.insert(
            "CFBundleExecutable".into(),
            plist::Value::String("../outside".into()),
        );
        plist::to_file_xml(&plist_path, &plist::Value::Dictionary(dictionary)).unwrap();

        let error = AppSigningTargets::discover(&app_path).err().unwrap();
        assert!(error.to_string().contains("unsafe CFBundleExecutable"));
    }

    #[test]
    fn dmg_codesign_is_ad_hoc_without_runtime_timestamp_keychain_or_deep() {
        let command = codesign_command(Path::new("Product.dmg"), None, false);
        let arguments = command_arguments(&command);

        assert!(
            arguments
                .windows(2)
                .any(|arguments| { arguments == ["--sign".to_owned(), "-".to_owned()] })
        );
        assert!(!arguments.iter().any(|argument| argument == "--timestamp"));
        assert!(!arguments.iter().any(|argument| argument == "runtime"));
        assert!(!arguments.iter().any(|argument| argument == "--keychain"));
        assert!(!arguments.iter().any(|argument| argument == "--deep"));
    }

    #[test]
    fn ad_hoc_codesign_uses_runtime_without_timestamp_or_keychain() {
        let command = codesign_command(
            Path::new("Product.app"),
            Some(Path::new("entitlements.plist")),
            true,
        );
        let arguments = command_arguments(&command);

        assert!(!arguments.iter().any(|argument| argument == "--timestamp"));
        assert!(arguments.iter().any(|argument| argument == "runtime"));
        assert!(
            arguments
                .windows(2)
                .any(|arguments| { arguments == ["--sign".to_owned(), "-".to_owned()] })
        );
        assert!(!arguments.iter().any(|argument| argument == "--keychain"));
        assert!(!arguments.iter().any(|argument| argument == "--deep"));
    }

    #[test]
    fn signing_command_rejects_configurable_identity_and_notarization_options() {
        for forbidden_option in ["--identity", "--ad-hoc", "--notarize"] {
            let result = Command::try_parse_from(["sign-alcom-app", forbidden_option]);
            assert!(result.is_err(), "{forbidden_option} must not be accepted");
        }
    }
}
