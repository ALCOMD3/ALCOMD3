use crate::alcomd3_config::ReleaseUpdateMode;
use crate::release_assets::{
    ReleaseManifestPaths, ResolvedReleaseAsset, ResolvedReleasePlatform, copy_platform_assets,
    write_release_build_shard,
};
use crate::release_common::{
    CmdRunner, ReleaseAutomation, ReleaseChannel, ReleaseContext, cargo, cargo_xtask,
    check_worktree_clean, current_head, default_repo, default_site_base_url,
    ensure_github_actions_context, git, npm, remove_updater_signing_env,
};
use anyhow::{Context, Result, bail};

/// Validate and build one platform shard for later signed release assembly.
#[derive(clap::Parser)]
pub struct Command {
    /// Release version, for example 2.2.0 or 2.2.0-beta.1.
    #[arg(long)]
    version: String,

    /// Release channel.
    #[arg(long, value_enum, default_value_t = ReleaseChannel::Stable)]
    channel: ReleaseChannel,

    /// Configured updater platform key.
    #[arg(long, default_value = "windows-x86_64")]
    platform: String,

    /// GitHub repository in OWNER/REPO form.
    #[arg(long, default_value_t = default_repo())]
    repo: String,

    /// Public website base URL.
    #[arg(long, default_value_t = default_site_base_url())]
    site_base_url: String,

    /// Skip cargo/npm validation commands.
    #[arg(long)]
    skip_validation: bool,

    /// Produce an official updater-unsigned platform shard for the trusted Draft workflow.
    #[arg(long, conflicts_with = "release_artifacts")]
    github_actions_release: bool,

    /// Produce a manually assemblable unsigned platform shard.
    #[arg(long)]
    release_artifacts: bool,

    /// Print planned commands without executing them.
    #[arg(long)]
    dry_run: bool,
}

impl crate::Command for Command {
    fn run(self) -> Result<i32> {
        let ctx = ReleaseContext::new(
            self.version,
            self.channel,
            Some(self.repo),
            Some(self.site_base_url),
            None,
        )?;
        let platform_config = ctx.config.release_platform(&self.platform)?;
        let platform = crate::release_assets::resolve_release_platform(
            &self.platform,
            platform_config,
            &ctx.workspace_root,
            &ctx.version,
        );
        let runner = CmdRunner::new(self.dry_run);
        let produces_release_artifacts = self.github_actions_release || self.release_artifacts;

        let github_source_sha = if self.github_actions_release {
            let source_sha = if self.dry_run {
                "<github-sha>".to_string()
            } else {
                std::env::var("GITHUB_SHA")
                    .context("GITHUB_SHA is required when --github-actions-release is selected")?
            };
            ensure_github_actions_context(
                &ctx,
                ReleaseAutomation::Draft,
                &source_sha,
                self.dry_run,
            )?;
            Some(source_sha)
        } else {
            None
        };

        let release_source_sha = if !self.dry_run && produces_release_artifacts {
            check_worktree_clean(&ctx)?;
            crate::release_validate::ensure_release_source_ready(&ctx)?;
            let source_sha = current_head(&ctx)?;
            if let Some(expected) = github_source_sha.as_deref() {
                anyhow::ensure!(
                    source_sha.eq_ignore_ascii_case(expected),
                    "checked out source commit does not match GITHUB_SHA"
                );
            }
            Some(source_sha)
        } else if !self.dry_run {
            crate::release_validate::ensure_release_source_versions_ready(&ctx)?;
            None
        } else {
            None
        };

        if !self.skip_validation {
            run_validation(&runner, &ctx)?;
        }
        run_platform_build(&runner, &ctx, &platform, produces_release_artifacts)?;

        let artifact_dir = if produces_release_artifacts {
            ctx.artifact_dir()
        } else {
            ctx.local_test_artifact_dir()
        };
        copy_platform_assets(&platform, &artifact_dir, self.dry_run)?;
        if let Some(source_sha) = release_source_sha.as_deref() {
            let shard_dir = ctx.release_build_shard_dir();
            let manifest_path = ctx.release_build_manifest();
            write_release_build_shard(
                &platform,
                &ReleaseManifestPaths {
                    version: &ctx.version,
                    channel: ctx.channel,
                    source_sha,
                    artifact_dir: &artifact_dir,
                    shard_dir: &shard_dir,
                    manifest_path: &manifest_path,
                },
            )?;
        }

        println!(
            "{} updater-unsigned build shard is ready: {}",
            platform.key,
            artifact_dir.display()
        );
        Ok(0)
    }
}

fn run_validation(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<()> {
    let mut cmd = cargo();
    cmd.arg("fmt")
        .arg("--all")
        .arg("--check")
        .current_dir(&ctx.workspace_root);
    remove_updater_signing_env(&mut cmd);
    runner.run(cmd, "cargo fmt")?;

    let mut cmd = cargo();
    cmd.arg("check")
        .arg("--workspace")
        .arg("--exclude")
        .arg("windows-installer-wrapper")
        .current_dir(&ctx.workspace_root);
    remove_updater_signing_env(&mut cmd);
    runner.run(cmd, "cargo check release workspace")?;

    let mut cmd = cargo();
    cmd.arg("test")
        .arg("--workspace")
        .arg("--exclude")
        .arg("windows-installer-wrapper")
        .current_dir(&ctx.workspace_root);
    remove_updater_signing_env(&mut cmd);
    runner.run(cmd, "cargo test release workspace")?;

    for (directory, script, what) in [
        ("vrc-get-gui", "check", "GUI check"),
        ("vrc-get-gui", "lint", "GUI lint"),
    ] {
        let mut cmd = npm();
        cmd.arg("run")
            .arg(script)
            .current_dir(ctx.workspace_root.join(directory));
        remove_updater_signing_env(&mut cmd);
        runner.run(cmd, what)?;
    }

    let mut cmd = git();
    cmd.arg("diff")
        .arg("--check")
        .current_dir(&ctx.workspace_root);
    remove_updater_signing_env(&mut cmd);
    runner.run(cmd, "git diff --check")
}

fn run_platform_build(
    runner: &CmdRunner,
    ctx: &ReleaseContext,
    platform: &ResolvedReleasePlatform,
    official_release: bool,
) -> Result<()> {
    match platform.key.as_str() {
        "windows-x86_64" => {
            let update_mode = shared_update_mode(platform, &["windows-installer"])?;
            run_build_alcom(runner, ctx, platform, update_mode)?;
            run_bundle_alcom(runner, ctx, platform, &platform.bundles)?;
        }
        "darwin-aarch64" => {
            let dmg = download_asset(platform, "macos-apple-silicon")?;
            let update_mode = shared_update_mode(platform, &["macos-apple-silicon"])?;
            run_build_alcom(runner, ctx, platform, update_mode)?;
            run_bundle_alcom(runner, ctx, platform, &["app".to_string()])?;
            if official_release {
                run_macos_ad_hoc_signing(runner, ctx, platform, None)?;
            }
            run_bundle_alcom(
                runner,
                ctx,
                platform,
                &["app-updater".to_string(), "dmg".to_string()],
            )?;
            if official_release {
                run_macos_ad_hoc_signing(runner, ctx, platform, Some(&dmg.source))?;
            }
        }
        "linux-x86_64" => {
            let appimage_mode = shared_update_mode(platform, &["linux-appimage"])?;
            run_build_alcom(runner, ctx, platform, appimage_mode)?;
            run_bundle_alcom(
                runner,
                ctx,
                platform,
                &["appimage".to_string(), "appimage-updater".to_string()],
            )?;
            let deb_mode = download_asset(platform, "linux-deb")?.update_mode;
            run_build_alcom(runner, ctx, platform, deb_mode)?;
            run_bundle_alcom(runner, ctx, platform, &["deb".to_string()])?;
        }
        other => bail!("release build recipe is not implemented for {other}"),
    }
    Ok(())
}

fn shared_update_mode(
    platform: &ResolvedReleasePlatform,
    download_ids: &[&str],
) -> Result<ReleaseUpdateMode> {
    let update_mode = platform.updater.update_mode;
    for id in download_ids {
        let download = download_asset(platform, id)?;
        anyhow::ensure!(
            download.update_mode == update_mode,
            "{} and updater must use the same update mode for the {} build recipe",
            id,
            platform.key
        );
    }
    Ok(update_mode)
}

fn download_asset<'a>(
    platform: &'a ResolvedReleasePlatform,
    id: &str,
) -> Result<&'a ResolvedReleaseAsset> {
    let role = format!("download:{id}");
    platform
        .downloads
        .iter()
        .find(|asset| asset.roles.iter().any(|candidate| candidate == &role))
        .with_context(|| {
            format!(
                "{} release build recipe requires configured download {id}",
                platform.key
            )
        })
}

fn run_macos_ad_hoc_signing(
    runner: &CmdRunner,
    ctx: &ReleaseContext,
    platform: &ResolvedReleasePlatform,
    dmg: Option<&std::path::Path>,
) -> Result<()> {
    anyhow::ensure!(
        platform.macos_ad_hoc_signed,
        "macOS release platform does not require ad-hoc signing"
    );
    let mut cmd = cargo_xtask();
    cmd.arg("sign-alcom-app");
    if let Some(dmg) = dmg {
        cmd.arg("--dmg").arg(dmg);
    } else {
        cmd.arg("--target").arg(&platform.target);
    }

    let what = if dmg.is_some() {
        "ad-hoc signing macOS DMG"
    } else {
        "ad-hoc signing macOS app"
    };
    cmd.current_dir(&ctx.workspace_root);
    remove_updater_signing_env(&mut cmd);
    runner.run(cmd, what)
}

fn run_build_alcom(
    runner: &CmdRunner,
    ctx: &ReleaseContext,
    platform: &ResolvedReleasePlatform,
    update_mode: ReleaseUpdateMode,
) -> Result<()> {
    let mut cmd = cargo_xtask();
    cmd.args(build_alcom_args(&platform.target, update_mode));
    cmd.current_dir(&ctx.workspace_root);
    remove_updater_signing_env(&mut cmd);
    runner.run(
        cmd,
        match update_mode {
            ReleaseUpdateMode::SelfUpdater => "building self-updating ALCOMD3",
            ReleaseUpdateMode::NoSelfUpdater => {
                "building package-manager ALCOMD3 without self-updater"
            }
        },
    )
}

fn build_alcom_args(target: &str, update_mode: ReleaseUpdateMode) -> Vec<String> {
    let mut args = vec![
        "build-alcom".to_string(),
        "--release".to_string(),
        "--target".to_string(),
        target.to_string(),
    ];
    if !update_mode.uses_self_updater() {
        args.push("--no-self-updater".to_string());
    }
    args
}

fn run_bundle_alcom(
    runner: &CmdRunner,
    ctx: &ReleaseContext,
    platform: &ResolvedReleasePlatform,
    bundles: &[String],
) -> Result<()> {
    let mut cmd = cargo_xtask();
    cmd.arg("bundle-alcom")
        .arg("--release")
        .arg("--target")
        .arg(&platform.target)
        .arg("--bundles")
        .arg(bundles.join(","))
        .current_dir(&ctx.workspace_root);
    remove_updater_signing_env(&mut cmd);
    runner.run(cmd, &format!("bundling {} release assets", platform.key))
}

#[cfg(test)]
mod tests {
    use super::{Command, build_alcom_args, download_asset, shared_update_mode};
    use crate::alcomd3_config::{Alcomd3Config, ReleaseUpdateMode};
    use crate::release_assets::resolve_release_platforms;
    use clap::Parser;

    #[test]
    fn release_build_defaults_to_windows_shard() {
        let command = Command::try_parse_from(["xtask", "--version", "2.1.1"]).unwrap();

        assert_eq!(command.platform, "windows-x86_64");
        assert!(!command.github_actions_release);
        assert!(!command.release_artifacts);
    }

    #[test]
    fn release_build_recipe_uses_each_assets_configured_update_mode() {
        let config = Alcomd3Config::load().unwrap();
        let workspace = crate::utils::cargo::cargo_metadata()
            .workspace_root
            .as_std_path()
            .to_path_buf();
        let platforms = resolve_release_platforms(&config, &workspace, "2.2.0");
        let windows = platforms
            .iter()
            .find(|platform| platform.key == "windows-x86_64")
            .unwrap();
        let macos = platforms
            .iter()
            .find(|platform| platform.key == "darwin-aarch64")
            .unwrap();
        let linux = platforms
            .iter()
            .find(|platform| platform.key == "linux-x86_64")
            .unwrap();

        assert_eq!(
            shared_update_mode(windows, &["windows-installer"]).unwrap(),
            ReleaseUpdateMode::SelfUpdater
        );
        assert_eq!(
            shared_update_mode(macos, &["macos-apple-silicon"]).unwrap(),
            ReleaseUpdateMode::SelfUpdater
        );
        assert_eq!(
            shared_update_mode(linux, &["linux-appimage"]).unwrap(),
            ReleaseUpdateMode::SelfUpdater
        );
        assert_eq!(
            download_asset(linux, "linux-deb").unwrap().update_mode,
            ReleaseUpdateMode::NoSelfUpdater
        );
    }

    #[test]
    fn release_build_recipe_rejects_assets_built_together_with_different_modes() {
        let config = Alcomd3Config::load().unwrap();
        let workspace = crate::utils::cargo::cargo_metadata()
            .workspace_root
            .as_std_path()
            .to_path_buf();
        let mut linux = resolve_release_platforms(&config, &workspace, "2.2.0")
            .into_iter()
            .find(|platform| platform.key == "linux-x86_64")
            .unwrap();
        download_asset(&linux, "linux-appimage").unwrap();
        linux
            .downloads
            .iter_mut()
            .find(|asset| asset.roles == ["download:linux-appimage"])
            .unwrap()
            .update_mode = ReleaseUpdateMode::NoSelfUpdater;

        let error = shared_update_mode(&linux, &["linux-appimage"]).unwrap_err();
        assert!(error.to_string().contains("must use the same update mode"));
    }

    #[test]
    fn build_alcom_args_apply_the_configured_update_mode() {
        let self_updating =
            build_alcom_args("x86_64-unknown-linux-gnu", ReleaseUpdateMode::SelfUpdater);
        let package_manager =
            build_alcom_args("x86_64-unknown-linux-gnu", ReleaseUpdateMode::NoSelfUpdater);

        assert!(!self_updating.iter().any(|arg| arg == "--no-self-updater"));
        assert!(package_manager.iter().any(|arg| arg == "--no-self-updater"));
    }
}
