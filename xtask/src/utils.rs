#![allow(dead_code)]

use crate::utils;
use crate::utils::rustc::rustc_host_triple;
use anyhow::Context;
use sha2::{Digest, Sha256};
use std::io::IoSlice;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{fs, io};

pub mod cargo;
pub mod command;
pub mod dpkg;
pub mod ds_store;
pub mod rustc;
pub mod tar;

pub fn ureq() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();

    AGENT.get_or_init(|| {
        ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .user_agent(&xtask_user_agent())
                .build(),
        )
    })
}

fn xtask_user_agent() -> String {
    crate::alcomd3_config::Alcomd3Config::load()
        .map(|config| {
            format!(
                "cargo-xtask of {} (https://github.com/{})",
                config.product_name, config.repository
            )
        })
        .unwrap_or_else(|_| "cargo-xtask of ALCOMD3".to_string())
}

pub trait MayOption<T> {
    fn into_option(self) -> Option<T>;
}

impl<T> MayOption<T> for Option<T> {
    fn into_option(self) -> Option<T> {
        self
    }
}

impl<T> MayOption<T> for T {
    fn into_option(self) -> Option<T> {
        Some(self)
    }
}

pub fn build_target<'a>(target: impl MayOption<&'a str>) -> &'a str {
    let host_triple = rustc_host_triple();
    target.into_option().unwrap_or(host_triple)
}

pub fn build_dir<'a>(target: impl MayOption<&'a str>, profile: &str) -> PathBuf {
    let metadata = cargo::cargo_metadata();
    let target_dir = metadata.target_directory.as_std_path();
    // https://github.com/rust-lang/cargo/blob/b54fe551a982d75d299e0d54daeac70cb854eef0/src/cargo/core/profiles.rs#L119
    // built-in profiles have different dir name
    let profile_dir = match profile {
        "dev" => "debug",
        "test" => "debug",
        "bench" => "release",
        _ => profile,
    };

    match target.into_option() {
        None => target_dir.join(profile_dir),
        Some(target) => target_dir.join(target).join(profile_dir),
    }
}

#[derive(clap::Args)]
#[command(group(
    clap::ArgGroup::new("profile_select")
        .args(["release", "profile"])
))]
pub struct BuildProfile {
    /// Alias for --profile release
    #[arg(long)]
    release: bool,

    /// Builds for specified profile. dev profile is used by defaykt
    #[arg(long)]
    profile: Option<String>,
}

impl BuildProfile {
    pub fn with_default<'a>(&'a self, default: &'a str) -> &'a str {
        if self.release {
            "release"
        } else if let Some(profile) = &self.profile {
            profile
        } else {
            default
        }
    }

    pub fn name(&self) -> &str {
        self.with_default("dev")
    }
}

/// Make a file executable (mode 755).
#[cfg(unix)]
pub fn make_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("chmod 755 {}", path.display()))
}

#[cfg(not(unix))]
pub fn make_executable(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

pub fn estimated_dir_size(path: &Path) -> Option<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(path).ok()? {
        let Ok(entry) = entry else {
            continue;
        };
        if let Ok(meta) = entry.metadata() {
            if meta.is_dir() {
                total += estimated_dir_size(&entry.path()).unwrap_or(0);
            } else {
                total += meta.len();
            }
        }
    }
    Some(total)
}

pub struct CountingIo<T> {
    count: u64,
    inner: T,
}

impl<T> CountingIo<T> {
    pub fn new(inner: T) -> Self {
        Self { count: 0, inner }
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: io::Write> io::Write for CountingIo<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf).inspect(|&x| {
            self.count += x as u64;
        })
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.inner.write_vectored(bufs).inspect(|&x| {
            self.count += x as u64;
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Download a file from `url` to `dest`, skipping if the file already exists.
pub fn download_file_cached(url: &str, dest: &Path, what: &str) -> anyhow::Result<()> {
    if dest.is_file() {
        println!("cached: {}", dest.display());
        return Ok(());
    }
    fs::create_dir_all(dest.parent().unwrap())?;

    let mut response = utils::ureq()
        .get(url)
        .call()
        .with_context(|| format!("{what}: downloading {url}"))?;

    std::io::copy(
        &mut response.body_mut().as_reader(),
        &mut fs::File::create(dest)
            .with_context(|| format!("{what}: creating {}", dest.display()))?,
    )
    .with_context(|| format!("{what}: saving {url}"))?;

    println!("downloaded: {}", dest.display());
    Ok(())
}

pub fn download_file_cached_sha256(
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    what: &str,
) -> anyhow::Result<()> {
    if dest.is_file() {
        if verify_file_sha256(dest, expected_sha256).is_ok() {
            println!("cached and verified: {}", dest.display());
            return Ok(());
        }
        eprintln!("cached file failed SHA-256 verification; downloading again");
        fs::remove_file(dest)
            .with_context(|| format!("{what}: removing invalid cache {}", dest.display()))?;
    }

    download_file_cached(url, dest, what)?;
    if let Err(error) = verify_file_sha256(dest, expected_sha256) {
        let _ = fs::remove_file(dest);
        return Err(error).with_context(|| format!("{what}: verifying downloaded file"));
    }
    Ok(())
}

pub fn verify_file_sha256(path: &Path, expected_sha256: &str) -> anyhow::Result<()> {
    let expected = expected_sha256.trim().to_ascii_lowercase();
    anyhow::ensure!(
        expected.len() == 64 && expected.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid expected SHA-256 digest"
    );

    let actual = file_sha256(path)?;
    anyhow::ensure!(
        actual == expected,
        "SHA-256 mismatch for {}: expected {expected}, got {actual}",
        path.display()
    );
    Ok(())
}

pub fn file_sha256(path: &Path) -> anyhow::Result<String> {
    use std::io::Read as _;

    let mut file =
        fs::File::open(path).with_context(|| format!("opening {} for SHA-256", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("reading {} for SHA-256", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[allow(clippy::iter_nth_zero)] // actually we're accessing 0th element
pub fn target_arch(target: &str) -> &str {
    target.split('-').nth(0).unwrap()
}

pub fn target_vendor(target: &str) -> &str {
    target.split('-').nth(1).unwrap_or("unknown")
}

pub fn target_os(target: &str) -> &str {
    target.split('-').nth(2).unwrap_or("none")
}

pub fn target_abi(target: &str) -> &str {
    target.split('-').nth(3).unwrap_or("none")
}

#[cfg(test)]
mod tests {
    use super::verify_file_sha256;

    #[test]
    fn sha256_verification_accepts_known_content_and_rejects_mismatch() {
        let path = std::env::temp_dir().join(format!("alcomd3-sha256-test-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();

        verify_file_sha256(
            &path,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .unwrap();
        let error = verify_file_sha256(
            &path,
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap_err();
        let _ = std::fs::remove_file(path);

        assert!(error.to_string().contains("SHA-256 mismatch"));
    }
}
