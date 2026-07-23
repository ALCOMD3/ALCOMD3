use super::{BundleContext, create_tar_gz};
use crate::bundle_alcom::linux::*;
use crate::utils;
use crate::utils::command::CommandExt;
use crate::utils::rustc::rustc_host_triple;
use crate::utils::{download_file_cached, make_executable, target_arch};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

// appimage versions we currently used
const APPIMAGETOOL_VERSION: &str = "1.9.1";
const APPIMAGETOOL_URL: &str = "https://github.com/AppImage/appimagetool/releases/download/{{version}}/appimagetool-{{arch}}.AppImage";

fn appimage_name(ctx: &BundleContext<'_>) -> Result<String> {
    Ok(format!(
        "{}_{}_{arch}.AppImage",
        ctx.product_name(),
        ctx.version(),
        arch = target_arch(ctx.target_tuple)
    ))
}

/// Builds the AppImage and returns the path to the created file.
pub fn create_appimage(ctx: &BundleContext<'_>) -> Result<PathBuf> {
    if !ctx.is_debian_like() {
        bail!(
            "building a full AppImage requires dpkg, dpkg-query, and apt-cache on a Debian-like system"
        );
    }

    let appdir = ctx
        .bundle_dir
        .join("appimage")
        .join(format!("{}.AppDir", ctx.product_name()));
    prepare_appdir(ctx, &appdir)?;

    let tool = ensure_appimagetool(ctx)?;
    make_executable(&tool)?;

    let name = format!(
        "{}_{}_{}.AppImage",
        ctx.product_name(),
        ctx.version(),
        target_arch(ctx.target_tuple)
    );
    let out_dir = ctx.bundle_dir.join("appimage");
    fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join(&name);

    // appimagetool requires ARCH to be set for non-native builds.

    ProcessCommand::new(&tool)
        .arg(&appdir)
        .arg(&out_path)
        .env("ARCH", target_arch(ctx.target_tuple))
        // Avoid FUSE requirement when running appimagetool (which is itself an AppImage)
        // in environments where FUSE is not available (e.g. GitHub Actions).
        .env("APPIMAGE_EXTRACT_AND_RUN", "1")
        .run_checked("creating AppImage")?;

    println!("created: {}", out_path.display());
    Ok(out_path)
}

pub fn create_appimage_tar_gz(ctx: &BundleContext<'_>) -> Result<()> {
    let appimage = ctx.bundle_dir.join("appimage").join(appimage_name(ctx)?);

    let archive_name = format!(
        "{}.tar.gz",
        appimage.file_name().and_then(|n| n.to_str()).unwrap()
    );
    let out = appimage.parent().unwrap().join(&archive_name);
    let inner_name = appimage.file_name().and_then(|n| n.to_str()).unwrap();

    create_tar_gz(&appimage, inner_name, &out)
}

/// Populate the AppDir structure.
fn prepare_appdir(ctx: &BundleContext<'_>, appdir: &Path) -> Result<()> {
    if appdir.exists() {
        fs::remove_dir_all(appdir)?;
    }

    fs::create_dir_all(appdir).context("creating AppDir root")?;
    prepare_system_libraries(ctx, appdir).context("copying system libraries into AppDir")?;

    let bin_dir = appdir.join("usr/bin");
    let share_apps = appdir.join("usr/share/applications");
    let icons_dir = appdir.join("usr/share/icons/hicolor");

    fs::create_dir_all(&bin_dir)?;
    fs::create_dir_all(&share_apps)?;

    // Binary
    let bin_name = ctx.binary_name();
    let bin_dst = bin_dir.join(bin_name);
    fs::copy(ctx.binary_path(), &bin_dst).context("copying binary to AppDir")?;
    make_executable(&bin_dst)?;

    let mcp_bin_dst = bin_dir.join(ctx.mcp_binary_name());
    fs::copy(ctx.mcp_binary_path(), &mcp_bin_dst).context("copying MCP bridge binary to AppDir")?;
    make_executable(&mcp_bin_dst)?;

    // AppRun (wrapper that executes the binary)
    let apprun_path = appdir.join("AppRun");
    fs::write(
        &apprun_path,
        fs::read_to_string(ctx.gui_dir.join("bundle/appimage/AppRun"))
            .context("reading AppRun template")?
            .replace("@debian_tuple@", &ctx.debian_triple())
            .replace("@binary_name@", bin_name),
    )?;
    make_executable(&apprun_path)?;

    // Desktop file
    let exec = format!("usr/bin/{bin_name}");
    let desktop_content = crate::bundle_alcom::linux::render_desktop_file(ctx, &exec)?;
    let desktop_name = ctx.linux_desktop_file_name();
    fs::write(appdir.join(&desktop_name), &desktop_content)?;
    fs::create_dir_all(&share_apps)?;
    fs::write(share_apps.join(&desktop_name), &desktop_content)?;

    // Icons
    install_icons(ctx, &icons_dir)?;

    // Also copy the 128x128 icon as the top-level .DirIcon and app icon for appimagetool.
    let icon_128 = ctx.icon_path("128x128");
    fs::copy(&icon_128, appdir.join(".DirIcon"))?;
    fs::copy(
        &icon_128,
        appdir.join(format!("{}.png", ctx.linux_icon_name())),
    )?;

    Ok(())
}

pub fn prepare_system_libraries(ctx: &BundleContext<'_>, appimage_root: &Path) -> Result<()> {
    let sysroot = Path::new("/");

    let system_packages = list_deps::collect_system_packages()?;
    let mut lib_names = Vec::new();
    for binary_path in [ctx.binary_path(), ctx.mcp_binary_path()] {
        for lib_name in list_deps::collect_dependency_libraries(&binary_path)
            .with_context(|| format!("collecting dependencies from {}", binary_path.display()))?
        {
            if !lib_names.contains(&lib_name) {
                lib_names.push(lib_name);
            }
        }
    }

    let file_names = lib_names
        .iter()
        .map(|lib_name| ctx.find_library(lib_name))
        .collect::<Vec<_>>();

    let packages = utils::dpkg::dpkg_query_search(file_names.iter().flatten().flatten())
        .context("querying packages for dependent libraries")?;
    for (lib, _) in lib_names
        .iter()
        .zip(file_names.iter())
        .filter(|(_, paths)| {
            !paths
                .as_ref()
                .is_some_and(|paths| paths.iter().any(|path| packages.contains_key(path)))
        })
    {
        eprintln!("Package of library {lib:?} not found");
    }

    let required_packages = list_deps::collect_dependency_packages(
        packages.values().flatten().map(|package| {
            if let Some(architecture) = &package.architecture {
                format!("{}:{architecture}", package.package_name)
            } else {
                package.package_name.clone()
            }
        }),
        &system_packages,
    )?;

    println!("Using the following libraries as system libraries:");
    for package in &system_packages {
        println!("  {package}");
    }
    println!("Bundling the following libraries:");
    for package in &required_packages {
        println!("  {package}");
    }

    let mut files = list_deps::collect_files_to_bundle(&required_packages)?;
    if let Some(i) = files.iter().position(|path| path == "usr") {
        files[0..=i].rotate_right(1);
    }
    if let Some(i) = files.iter().position(|path| path == "usr/lib") {
        files[1..=i].rotate_right(1);
    }

    for path in &files {
        let absolute = sysroot.join(path);

        let metadata = absolute
            .symlink_metadata()
            .with_context(|| format!("reading metadata for {}", absolute.display()))?;

        if metadata.is_symlink() {
            let link = fs::read_link(&absolute)
                .with_context(|| format!("reading symlink {}", absolute.display()))?;
            if link.is_absolute() {
                bail!("cannot copy absolute symlink {}", absolute.display());
            }
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&link, appimage_root.join(path))
                    .with_context(|| format!("copying symlink {path}"))?;
            }
            #[cfg(not(unix))]
            {
                let _ = link;
                bail!("copying symlinks is only supported on Unix hosts");
            }
        } else if metadata.is_dir() {
            match fs::create_dir(appimage_root.join(path)) {
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                result => result,
            }
            .with_context(|| format!("copying directory {path}"))?;
        } else {
            fs::copy(&absolute, appimage_root.join(path))
                .with_context(|| format!("copying file {path}"))?;
        }
    }

    let debian_triple = ctx.debian_triple();

    remove_dir_if_exists(appimage_root.join(format!("usr/lib/{debian_triple}/enchant-2")))
        .context("removing enchant providers")?;
    remove_dir_if_exists(appimage_root.join(format!("usr/lib/{debian_triple}/gstreamer-1.0")))
        .context("removing GStreamer plugins")?;
    remove_dir_if_exists(appimage_root.join(format!("usr/lib/{debian_triple}/gstreamer1.0")))
        .context("removing GStreamer helpers")?;
    remove_dir_if_exists(appimage_root.join("usr/share/lintian"))
        .context("removing lintian metadata")?;

    list_deps::patch_usr_lib(
        files
            .iter()
            .map(|file| appimage_root.join(file))
            .filter(|path| path.is_file())
            .filter(|path| {
                let name = path.file_name().unwrap().as_encoded_bytes();
                name.starts_with(b"libenchant") || name.starts_with(b"libwebkit")
            }),
    )?;

    Ok(())
}

fn remove_dir_if_exists(path: impl AsRef<Path>) -> Result<()> {
    match fs::remove_dir_all(path.as_ref()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("removing {}", path.as_ref().display())),
    }
}

pub fn install_icons(ctx: &BundleContext<'_>, icons_base: &Path) -> Result<()> {
    for size in LINUX_ICON_RESOLUTIONS {
        let icon_dir = icons_base.join(size).join("apps");
        fs::create_dir_all(&icon_dir)?;
        fs::copy(
            ctx.icon_path(size),
            icon_dir.join(format!("{}.png", ctx.linux_icon_name())),
        )
        .with_context(|| format!("copying icon {size}.png"))?;
    }
    Ok(())
}

/// Ensures `appimagetool` is available in the target cache directory.
fn ensure_appimagetool(ctx: &BundleContext<'_>) -> Result<PathBuf> {
    let arch = target_arch(rustc_host_triple());
    let cache_dir = ctx
        .host_build_dir
        .join("bundle/appimagetool")
        .join(APPIMAGETOOL_VERSION);
    let tool = cache_dir.join(format!("appimagetool-{arch}.AppImage"));

    download_file_cached(
        &APPIMAGETOOL_URL
            .replace("{{version}}", APPIMAGETOOL_VERSION)
            .replace("{{arch}}", arch),
        &tool,
        "downloading appimagetool",
    )?;
    Ok(tool)
}

mod list_deps {
    use crate::utils;
    use anyhow::Context;
    use itertools::Itertools;
    use object::Endianness;
    use object::elf::DT_NEEDED;
    use object::read::elf::ElfFile64;
    use std::borrow::Borrow;
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::hash::Hash;
    use std::io::{Read, Seek, Write};
    use std::path::Path;
    use std::{fs, io};

    static SYSTEM_LIBRARIES: &[&str] = &[
        "libatomic1",
        "libgles2",
        "libatk1.0-0",
        "libc6",
        "libcairo2",
        "libdrm2",
        "libepoxy0",
        "libexpat1",
        "libfontconfig1",
        "libfreetype6",
        "libgbm1",
        "libgcc-s1",
        "libgcrypt20",
        "libglib2.0-0",
        "libegl1",
        "libgl1",
        "libudev1",
        "libwayland-client0",
        "libwayland-cursor0",
        "libwayland-egl1",
        "libx11-6",
        "libxcb1",
        "zlib1g",
        "libgtk-3-0",
        "libharfbuzz0b",
        "libstdc++6",
        "liblcms2-2",
        "libpango-1.0-0",
        "libpng16-16",
        "libpng16-16t64",
        "libbrotli1",
        "libgssapi-krb5-2",
        "libnghttp2-14",
        "libpsl5",
        "libpsl5t64",
        "libsqlite3-0",
        "libsystemd0",
        "libtasn1-6",
        "libwayland-server0",
        "libwebp7",
    ];

    fn is_ignored_dep(dependant: &str, dependency: &str) -> bool {
        let dependant = dependant.split_once(':').unwrap_or((dependant, "")).0;
        let dependency = dependency.split_once(':').unwrap_or((dependency, "")).0;
        match (dependant, dependency) {
            (_, "iso-codes") => true,
            ("libgstreamer1.0-0", "libcap2-bin") => true,

            ("libwebkit2gtk-4.1-0", "gstreamer1.0-plugins-base") => true,
            ("libwebkit2gtk-4.1-0", "gstreamer1.0-plugins-good") => true,
            ("libwebkit2gtk-4.1-0", "bubblewrap") => true,
            ("libwebkit2gtk-4.1-0", "xdg-dbus-proxy") => true,

            ("libsecret-1-0", "libsecret-common") => true,

            ("libsoup-3.0-0", "glib-networking") => true,
            ("libsoup-3.0-0", "libsoup-3.0-common") => true,

            ("libenchant-2-2", "<ispell-dictionary>") => true,
            ("libenchant-2-2", "libaspell15") => true,
            ("libenchant-2-2", "libhunspell-1.7-0") => true,

            (_, dependency) if dependency.starts_with('<') => true,
            _ => false,
        }
    }

    fn collect_deps<'a>(
        deps: &'a HashMap<String, utils::dpkg::PackageDepends>,
        entry_point: impl IntoIterator<Item = &'a (impl AsRef<str> + 'a)>,
    ) -> impl Iterator<Item = &'a str> {
        let mut queue = VecDeque::<&'a str>::new();
        for package in entry_point {
            queue.push_back(package.as_ref());
        }
        let mut visited = HashSet::new();

        std::iter::from_fn(move || {
            loop {
                let front = queue.pop_front()?;
                if !visited.insert(front) {
                    continue;
                }
                if let Some(package_deps) = deps.get(front) {
                    for depends in &package_deps.depends {
                        if let [single] = depends.as_slice() {
                            queue.push_back(single);
                        }
                    }
                }
                return Some(front);
            }
        })
    }

    pub fn collect_system_packages() -> anyhow::Result<HashSet<String>> {
        Ok(SYSTEM_LIBRARIES
            .iter()
            .map(|&package| package.to_owned())
            .collect())
    }

    pub fn collect_dependency_libraries(path: &Path) -> anyhow::Result<Vec<String>> {
        let binary = fs::read(path).context("failed to read binary")?;
        let parsed =
            ElfFile64::<Endianness>::parse(binary.as_slice()).context("failed to parse binary")?;

        let dynamic_table = parsed.elf_dynamic_table()?;
        let mut lib_names = Vec::new();
        for import in dynamic_table.iter() {
            if import.tag == DT_NEEDED {
                let name_bytes = import.string(dynamic_table.strings())?;
                let Ok(lib_name) = std::str::from_utf8(name_bytes) else {
                    eprintln!(
                        "Warning: library name is not utf8: {}",
                        String::from_utf8_lossy(name_bytes)
                    );
                    continue;
                };

                lib_names.push(lib_name.to_owned());
            }
        }

        Ok(lib_names)
    }

    pub fn collect_dependency_packages(
        entry_point: impl IntoIterator<Item = impl AsRef<str> + Clone + Eq + Hash>,
        system_packages: &HashSet<impl Borrow<str> + Eq + Hash>,
    ) -> anyhow::Result<Vec<String>> {
        let dpkg_arch = utils::dpkg::dpkg_architecture().context("retrieving dpkg architecture")?;
        let mut required_packages = entry_point
            .into_iter()
            .filter(|package| !system_packages.contains(package.as_ref()))
            .filter(|package| {
                package
                    .as_ref()
                    .split_once(':')
                    .is_none_or(|(package, _arch)| !system_packages.contains(package))
            })
            .unique()
            .map(|package| {
                let package_name = package.as_ref();
                match package_name.split_once(':') {
                    Some((package, arch)) if arch == dpkg_arch => package.to_owned(),
                    _ => package_name.to_owned(),
                }
            })
            .collect::<HashSet<_>>();

        let mut new_required_packages = Vec::from_iter(required_packages.iter().cloned());

        while !new_required_packages.is_empty() {
            let mut deps_map = utils::dpkg::AptCacheDepends::default()
                .run(&new_required_packages)
                .context("retrieving dependencies of required packages")?;

            for (package, deps) in &mut deps_map {
                deps.depends.retain(|depends| {
                    if let [single] = depends.as_slice() {
                        !is_ignored_dep(package, single)
                    } else {
                        true
                    }
                });
            }

            let deps_of_new_required = collect_deps(&deps_map, &new_required_packages)
                .filter(|&package| !system_packages.contains(package))
                .filter(|package| {
                    package
                        .split_once(':')
                        .is_none_or(|(package, _arch)| !system_packages.contains(package))
                })
                .filter(|&package| !required_packages.contains(package))
                .unique()
                .map(str::to_owned)
                .collect::<Vec<_>>();

            required_packages.extend(deps_of_new_required.iter().cloned());
            new_required_packages = deps_of_new_required;
        }

        Ok(required_packages.into_iter().sorted().collect_vec())
    }

    pub fn collect_files_to_bundle(packages: &[String]) -> anyhow::Result<Vec<String>> {
        let mut files = utils::dpkg::dpkg_query_list_files(packages)
            .context("collecting files from packages")?;
        files.sort();
        files.dedup();

        files.retain(|path| !path.is_empty() && path != "/.");

        for absolute in &mut files {
            assert_eq!(absolute.as_bytes()[0], b'/');
            absolute.remove(0);
        }

        Ok(files)
    }

    pub fn patch_usr_lib(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> anyhow::Result<()> {
        let mut buffer = Vec::new();
        static USR_LIB: &[u8] = b"/usr/lib";
        static DOT_LIB: &[u8] = b"././/lib";
        const { assert!(USR_LIB.len() == DOT_LIB.len()) }

        for file in paths {
            let file = file.as_ref();
            buffer.clear();

            let mut fd = fs::File::options()
                .read(true)
                .write(true)
                .open(file)
                .with_context(|| format!("opening file for patching: {}", file.display()))?;
            fd.read_to_end(&mut buffer)
                .with_context(|| format!("reading file for patching: {}", file.display()))?;

            if buffer.len() < USR_LIB.len() {
                continue;
            }

            let mut changed = false;
            for i in 0..=(buffer.len() - USR_LIB.len()) {
                let range = &mut buffer[i..][..USR_LIB.len()];
                if range == USR_LIB {
                    changed = true;
                    fd.seek(io::SeekFrom::Start(i as u64))
                        .and_then(|_| fd.write_all(DOT_LIB))
                        .with_context(|| {
                            format!("writing file for patching: {}", file.display())
                        })?;
                }
            }

            if changed {
                println!("patched {}", file.display());
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::SYSTEM_LIBRARIES;

        #[test]
        fn bundles_libraries_with_incompatible_fedora_sonames() {
            assert!(!SYSTEM_LIBRARIES.contains(&"libbz2-1.0"));
            assert!(!SYSTEM_LIBRARIES.contains(&"libjpeg8"));
        }
    }
}
