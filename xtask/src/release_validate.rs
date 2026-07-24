use crate::release_common::{
    ReleaseChannel, ReleaseContext, default_repo, default_site_base_url, default_target,
    ensure_release_notes_ready, validate_release_source_versions,
};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// Validate the prepared source state before building a release.
#[derive(clap::Parser)]
pub struct Command {
    /// Release version, for example 3.0.0 or 3.1.0-beta.1.
    #[arg(long)]
    version: String,

    /// Release channel.
    #[arg(long, value_enum, default_value_t = ReleaseChannel::Stable)]
    channel: ReleaseChannel,

    /// GitHub repository in OWNER/REPO form.
    #[arg(long, default_value_t = default_repo())]
    repo: String,

    /// Public updater endpoint base URL.
    #[arg(long, default_value_t = default_site_base_url())]
    site_base_url: String,

    /// Build target.
    #[arg(long, default_value_t = default_target())]
    target: String,
}

impl crate::Command for Command {
    fn run(self) -> Result<i32> {
        let ctx = ReleaseContext::new(
            self.version,
            self.channel,
            Some(self.repo),
            Some(self.site_base_url),
            Some(self.target),
        )?;
        ensure_release_source_ready(&ctx)?;
        println!("release source is ready: {} ({})", ctx.version, ctx.channel);
        Ok(0)
    }
}

#[derive(Deserialize)]
struct PackageJson {
    version: String,
}

pub fn ensure_release_source_ready(ctx: &ReleaseContext) -> Result<()> {
    ensure_release_source_versions_ready(ctx)?;
    ensure_release_notes_ready(ctx)?;
    crate::alcom_updater_json::validate_updater_notes_file(&ctx.updater_notes())?;
    Ok(())
}

/// Validates version-bearing source files without requiring release notes.
/// This is the boundary used by local signed test builds.
pub fn ensure_release_source_versions_ready(ctx: &ReleaseContext) -> Result<()> {
    let metadata = crate::utils::cargo::cargo_metadata();
    let workspace_members = metadata.workspace_members.iter().collect::<HashSet<_>>();
    let workspace_versions = metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.id))
        .map(|package| package.version.to_string())
        .collect::<Vec<_>>();
    let gui_version = read_package_version(&ctx.workspace_root.join("vrc-get-gui/package.json"))?;

    validate_release_source_versions(&ctx.version, &workspace_versions, &gui_version)?;
    Ok(())
}

fn read_package_version(path: &Path) -> Result<String> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let package: PackageJson =
        serde_json::from_str(&source).with_context(|| format!("parsing {}", path.display()))?;
    Ok(package.version)
}
