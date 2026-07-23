use crate::alcomd3_config::Alcomd3Config;
use crate::utils::{self, build_dir, build_target, target_os};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

mod app;
mod appimage;
mod deb;
mod dmg;
mod linux;
mod rpm;
mod setup_exe;

/// Individual bundle artifact that can be produced.
///
/// Pass one or more values to `--bundles` to produce only those artifacts.
/// If `--bundles` is not specified, all artifacts for the target platform are produced.
///
/// **macOS** artifacts:
/// - `app` - `ALCOMD3.app` application bundle
/// - `dmg` - `ALCOMD3_<version>_<arch>.dmg` disk image
/// - `app-updater` - `ALCOMD3.app.tar.gz` updater payload
///
/// **Linux** artifacts:
/// - `app-image` - `ALCOMD3_<version>_<arch>.AppImage`
/// - `app-image-updater` - `ALCOMD3_<version>_<arch>.AppImage.tar.gz` updater payload
/// - `deb` - `alcomd3_<version>_<arch>.deb` Debian package
/// - `rpm` - `alcomd3-<version>-1.<arch>.rpm` RPM package
/// - `buildroot` - The package manager independent buildroot for external package managers.
///
/// **Windows** artifacts:
/// - `setup-exe` - `-setup.exe` for first-time installation
/// - `setup-exe-zip` - `-setup.exe.zip` to workaround warning from browsers
/// - `exe-updater` - `-updater.exe` for the updater. This includes
#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BundleKind {
    // --- macOS ---
    /// ALCOMD3.app bundle
    App,
    /// Disk image (requires ALCOMD3.app to already exist in bundle dir)
    Dmg,
    /// ALCOMD3.app.tar.gz updater payload (requires ALCOMD3.app to already exist)
    AppUpdater,
    // --- Linux ---
    /// AppImage portable image
    #[value(name = "appimage")]
    AppImage,
    /// AppImage.tar.gz updater payload (requires AppImage to already exist)
    #[value(name = "appimage-updater")]
    AppImageUpdater,
    /// Package manager independent buildroot for external package managers
    ///
    /// Unlike dmg depends on app, deb/rpm doesn't depend on this bundle.
    Buildroot,
    /// Debian package
    Deb,
    /// RPM package
    Rpm,
    /// Windows setup.exe
    SetupExe,
    /// Windows setup.exe in zip (requires setup.exe to already exist in bundle dir)
    SetupExeZip,
    /// Windows setup.exe for updater
    ExeUpdater,
}

/// Bundles the ALCOM application for the target platform.
///
/// This reimplements the tauri bundler functionality so we do not need to depend on
/// `tauri bundle` / `tauri-apps/tauri-action` for creating distributable packages.
///
/// Outputs (relative to the profile build directory, e.g. `target/<triple>/release/`):
///
/// **macOS**
/// - `bundle/macos/ALCOMD3.app`                       – application bundle
/// - `bundle/macos/ALCOMD3.app.tar.gz`               – tarball used by the updater
/// - `bundle/dmg/ALCOMD3_<version>_universal.dmg`    – disk image for distribution
///
/// **Linux**
/// - `bundle/appimage/ALCOMD3_<version>_amd64.AppImage`                  – portable image
/// - `bundle/appimage/ALCOMD3_<version>_amd64.AppImage.tar.gz`           – tarball for updater
/// - `bundle/deb/alcomd3_<version>_amd64.deb`                            – Debian package
/// - `bundle/rpm/alcomd3-<version>-1.x86_64.rpm`                         – RPM package
#[derive(clap::Parser)]
pub(super) struct Command {
    /// Target triple (e.g. `universal-apple-darwin`, `x86_64-unknown-linux-gnu`).
    ///
    /// Defaults to the host triple.
    #[arg(long)]
    target: Option<String>,

    #[command(flatten)]
    profile: utils::BuildProfile,

    /// Specific bundle artifacts to produce (comma-separated or repeated).
    ///
    /// When not specified, all artifacts for the target platform are produced.
    /// Use this to split the bundling process - e.g. produce only `app` first,
    /// then sign it, then produce `dmg` and `app-updater`.
    #[arg(long, value_delimiter = ',')]
    bundles: Vec<BundleKind>,

    /// Output directory for buildroot
    ///
    /// Only valid for buildroot bundle
    #[arg(long)]
    buildroot: Option<PathBuf>,
}

impl crate::Command for Command {
    fn run(self) -> Result<i32> {
        let ctx = BundleContext::new(self.target.as_deref(), self.profile.name())?;

        let bundles = self.bundles.as_slice();

        if bundles.is_empty() {
            println!("Note: no bundles are specified");
        }

        if bundles.contains(&BundleKind::App) {
            app::create_app_bundle(&ctx)?;
        }

        if bundles.contains(&BundleKind::AppUpdater) {
            app::create_app_tar_gz(&ctx)?;
        }

        if bundles.contains(&BundleKind::Dmg) {
            dmg::create_dmg(&ctx)?;
        }

        if bundles.contains(&BundleKind::AppImage) {
            appimage::create_appimage(&ctx)?;
        }

        if bundles.contains(&BundleKind::AppImageUpdater) {
            appimage::create_appimage_tar_gz(&ctx)?;
        }

        if bundles.contains(&BundleKind::Buildroot) {
            linux::create_install_build_root(&ctx, self.buildroot.as_deref())?;
        }

        if bundles.contains(&BundleKind::Deb) {
            deb::create_deb(&ctx)?;
        }

        if bundles.contains(&BundleKind::Rpm) {
            rpm::create_rpm(&ctx)?;
        }

        if bundles.contains(&BundleKind::SetupExe) {
            setup_exe::create_setup_exe(&ctx)?;
        }

        if bundles.contains(&BundleKind::SetupExeZip) {
            setup_exe::create_setup_exe_zip(&ctx)?;
        }

        if bundles.contains(&BundleKind::ExeUpdater) {
            setup_exe::create_updater_exe(&ctx)?;
        }

        Ok(0)
    }
}

/// Shared context passed to every platform bundler.
pub(crate) struct BundleContext<'a> {
    #[allow(dead_code)]
    pub workspace_root: &'a Path,
    pub gui_dir: PathBuf,
    pub host_build_dir: &'a Path,
    pub build_dir: PathBuf,
    pub bundle_dir: PathBuf,
    pub target: Option<&'a str>,
    pub target_tuple: &'a str,
    pub profile: &'a str,
    version: String,
    config: Alcomd3Config,
}

impl<'a> BundleContext<'a> {
    pub fn new(target: Option<&'a str>, profile: &'a str) -> Result<Self> {
        let metadata = utils::cargo::cargo_metadata();
        let workspace_root = metadata.workspace_root.as_std_path();
        let config = Alcomd3Config::load_from_workspace(workspace_root)?;

        let target_tuple = build_target(target);
        let build_dir = build_dir(target, profile);

        let gui_dir = workspace_root.join("vrc-get-gui");

        let version = (metadata.packages.iter())
            .find(|p| p.name == "vrc-get-gui")
            .context("finding vrc-get-gui")?
            .version
            .to_string();

        let bundle_dir = build_dir.join("bundle");

        Ok(BundleContext {
            workspace_root,
            gui_dir,
            host_build_dir: metadata.target_directory.as_std_path(),
            build_dir,
            bundle_dir,
            target,
            target_tuple,
            profile,
            version,
            config,
        })
    }

    pub fn version(&self) -> &str {
        self.version.as_str()
    }

    pub fn short_description(&self) -> &str {
        &self.config.short_description
    }

    pub fn long_description(&self) -> &str {
        &self.config.long_description
    }

    /// Binary name without extension (e.g. `ALCOMD3`).
    pub fn binary_name(&self) -> &str {
        &self.config.main_binary_name
    }

    /// MCP bridge binary name without extension.
    pub fn mcp_binary_name(&self) -> &str {
        &self.config.mcp_binary_name
    }

    /// The lower-case package and Linux desktop identifier.
    pub fn package_name(&self) -> &str {
        &self.config.package_name
    }

    pub fn app_bundle_name(&self) -> String {
        format!("{}.app", self.product_name())
    }

    pub fn linux_desktop_file_name(&self) -> String {
        format!("{}.desktop", self.package_name())
    }

    pub fn linux_icon_name(&self) -> &str {
        self.package_name()
    }

    /// The human-readable product name.
    pub fn product_name(&self) -> &str {
        &self.config.product_name
    }

    /// The machine-readable identifier of the product.
    pub fn identifier(&self) -> &str {
        &self.config.tauri_identifier
    }

    pub fn legacy_tauri_identifier(&self) -> &str {
        &self.config.legacy_tauri_identifier
    }

    pub fn windows_app_id(&self) -> &str {
        &self.config.windows_app_id
    }

    pub fn windows_aumid(&self) -> &str {
        &self.config.windows_aumid
    }

    pub fn legacy_windows_app_id(&self) -> &str {
        &self.config.legacy_windows_app_id
    }

    /// The simplified copyright notice for this product
    pub fn copyright(&self) -> &str {
        &self.config.copyright
    }

    pub fn installer_file_name(&self) -> String {
        self.config.installer_file_name(self.version())
    }

    /// Path to the compiled binary in the build directory.
    pub fn binary_path(&self) -> PathBuf {
        if target_os(self.target_tuple) == "windows" {
            self.build_dir.join(format!("{}.exe", self.binary_name()))
        } else {
            self.build_dir.join(self.binary_name())
        }
    }

    /// Path to the compiled MCP bridge binary in the build directory.
    pub fn mcp_binary_path(&self) -> PathBuf {
        if target_os(self.target_tuple) == "windows" {
            self.build_dir
                .join(format!("{}.exe", self.mcp_binary_name()))
        } else {
            self.build_dir.join(self.mcp_binary_name())
        }
    }

    /// Resolved path of an icon file
    pub fn icon_path(&self, name: &str) -> PathBuf {
        let mut pathbuf = self.gui_dir.join("icons").join(name);
        if pathbuf.extension().is_none() {
            pathbuf.set_extension("png");
        }
        pathbuf
    }

    /// Linux only. Full AppImage bundling depends on Debian package metadata.
    pub fn is_debian_like(&self) -> bool {
        utils::dpkg::dpkg_apt_available()
    }

    pub fn debian_triple(&self) -> String {
        let arch = match utils::target_arch(self.target_tuple) {
            "i486" | "i586" | "i686" => "i386",
            "armv7" => "arm",
            default => default,
        };
        let abi = utils::target_abi(self.target_tuple);
        format!("{arch}-linux-{abi}")
    }

    pub fn find_library(&self, lib_name: &str) -> Option<Vec<String>> {
        let triple = self.debian_triple();

        for find_path in [
            "/usr/local/lib/{triple}/",
            "/lib/{triple}/",
            "/usr/lib/{triple}/",
            "/usr/local/lib/",
            "/lib/",
            "/usr/lib/",
            "/usr/{triple}/lib/",
        ] {
            let original_library_path = find_path.replace("{triple}", &triple);
            let Ok(canonical_library_dir) = fs::canonicalize(&original_library_path) else {
                continue;
            };
            let mut library_path = canonical_library_dir.to_string_lossy().into_owned();
            if !library_path.ends_with('/') {
                library_path.push('/');
            }
            library_path.push_str(lib_name);
            if Path::new(&library_path).exists() {
                let mut paths = vec![library_path.clone()];
                let original_path = format!("{original_library_path}{lib_name}");
                if original_path != library_path {
                    paths.push(original_path);
                }
                return Some(paths);
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a `.tar.gz` archive at `out_path` containing a single file `src`
/// whose name inside the archive is `archive_name`.
pub(crate) fn create_tar_gz(src: &Path, archive_name: &str, out_path: &Path) -> Result<()> {
    use flate2::Compression;
    use flate2::write::GzEncoder;

    fs::create_dir_all(out_path.parent().unwrap())?;

    let file =
        fs::File::create(out_path).with_context(|| format!("creating {}", out_path.display()))?;
    let gz = GzEncoder::new(file, Compression::best());
    let mut builder = tar::Builder::new(gz);
    builder.follow_symlinks(false);

    if src.is_dir() {
        builder
            .append_dir_all(archive_name, src)
            .with_context(|| format!("appending dir {}", src.display()))?;
    } else {
        builder
            .append_path_with_name(src, archive_name)
            .with_context(|| format!("appending file {}", src.display()))?;
    }

    let gz = builder.into_inner().context("finishing tar archive")?;
    gz.finish().context("finishing gzip stream")?;

    println!("created: {}", out_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_context_uses_alcomd3_binary_identity() -> Result<()> {
        let ctx = BundleContext::new(None, "release")?;

        assert_eq!(ctx.product_name(), "ALCOMD3");
        assert_eq!(ctx.binary_name(), "ALCOMD3");
        assert_eq!(ctx.package_name(), "alcomd3");
        assert_eq!(ctx.app_bundle_name(), "ALCOMD3.app");
        assert_eq!(ctx.linux_desktop_file_name(), "alcomd3.desktop");
        assert_eq!(ctx.linux_icon_name(), "alcomd3");

        Ok(())
    }
}
