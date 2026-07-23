use crate::release_common::{
    CmdRunner, ReleaseAutomation, ReleaseChannel, ReleaseContext, UpdaterSignaturePurpose,
    check_worktree_clean, default_repo, default_site_base_url, default_target,
    ensure_github_actions_context, ensure_github_release_is_draft, gh, git, remove_github_auth_env,
    remove_updater_signing_env, validate_full_git_sha, verify_github_release,
};
use anyhow::{Result, bail};
use std::process::Command as ProcessCommand;

/// Create or update a GitHub Release and optionally publish the draft.
#[derive(clap::Parser)]
pub struct Command {
    /// Release version, for example 2.0.1 or 2.1.0-beta.1.
    #[arg(long)]
    version: String,

    /// Release channel.
    #[arg(long, value_enum, default_value_t = ReleaseChannel::Stable)]
    channel: ReleaseChannel,

    /// GitHub repository in OWNER/REPO form.
    #[arg(long, default_value_t = default_repo())]
    repo: String,

    /// Public website base URL.
    #[arg(long, default_value_t = default_site_base_url())]
    site_base_url: String,

    /// Build target.
    #[arg(long, default_value_t = default_target())]
    target: String,

    /// Exact source commit used to build the artifacts. Defaults to the current HEAD locally.
    #[arg(long)]
    source_sha: Option<String>,

    /// Replace assets on an existing release draft instead of creating a new release.
    #[arg(long)]
    replace_assets: bool,

    /// Publish the release draft after creating or updating it.
    #[arg(long)]
    publish: bool,

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
            Some(self.target),
        )?;
        let runner = CmdRunner::new(self.dry_run);
        let platforms = ctx.resolved_release_platforms();
        let artifact_dir = ctx.artifact_dir();
        let shard_dir = ctx.release_build_shard_dir();
        let manifest_path = ctx.release_build_manifest();
        let target_commit = current_head(&runner, &ctx)?;
        let source_sha = self.source_sha.as_deref().unwrap_or(target_commit.as_str());

        let trusted_draft_automation = if !self.dry_run {
            validate_full_git_sha(source_sha)?;
            if !target_commit.eq_ignore_ascii_case(source_sha) {
                bail!(
                    "release artifacts were not built from the requested source commit: expected {source_sha}, got {target_commit}"
                );
            }

            let trusted_draft_automation = if std::env::var("GITHUB_ACTIONS").as_deref()
                == Ok("true")
            {
                ensure_github_actions_context(&ctx, ReleaseAutomation::Draft, source_sha, false)?;
                if self.publish {
                    bail!("the GitHub Actions Draft workflow is not allowed to publish a release");
                }
                true
            } else {
                ensure_local_manual_publish_source(&runner, &ctx, source_sha)?;
                false
            };

            crate::release_assets::verify_artifact_directory_allowlist(
                &artifact_dir,
                &ctx.expected_public_asset_names(),
            )?;
            crate::release_assets::verify_release_build_manifest(
                &platforms,
                &crate::release_assets::ReleaseManifestPaths {
                    version: &ctx.version,
                    channel: ctx.channel,
                    source_sha,
                    artifact_dir: &artifact_dir,
                    shard_dir: &shard_dir,
                    manifest_path: &manifest_path,
                },
            )?;
            let public_key = ctx
                .workspace_root
                .join("vrc-get-gui/src/updater-public-key.txt");
            for platform in &platforms {
                crate::verify_alcom_updater_json::verify_updater_signature_file(
                    &ctx.artifact_path(&platform.updater.name),
                    &ctx.artifact_path(&platform.updater_signature_name()),
                    &public_key,
                    Some(UpdaterSignaturePurpose::Release),
                )?;
            }
            trusted_draft_automation
        } else {
            false
        };

        // GitHub Actions' job-scoped token cannot read the Administration-only
        // immutable-releases setting. The trusted Draft workflow is checked by
        // an administrator-authenticated preflight before dispatch, while local
        // manual publishing continues to verify the setting directly.
        if !trusted_draft_automation {
            ensure_immutable_releases_enabled(&runner, &ctx)?;
        }

        if self.replace_assets {
            ensure_github_release_is_draft(&ctx, &runner)?;
            update_release_metadata(&runner, &ctx, source_sha)?;
            upload_assets(&runner, &ctx)?;
        } else {
            create_release(&runner, &ctx, source_sha)?;
        }

        verify_github_release(&ctx, &runner, Some(true), Some(source_sha))?;

        if self.publish {
            let mut cmd = gh();
            cmd.arg("release")
                .arg("edit")
                .arg(&ctx.tag)
                .arg("--repo")
                .arg(&ctx.repo)
                .arg("--draft=false");
            runner.run(cmd, "publishing GitHub Release draft")?;
            verify_github_release(&ctx, &runner, Some(false), Some(source_sha))?;
        }

        Ok(0)
    }
}

fn ensure_immutable_releases_enabled(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<()> {
    let mut cmd = gh();
    cmd.arg("api")
        .arg("--header")
        .arg("X-GitHub-Api-Version: 2026-03-10")
        .arg(format!("repos/{}/immutable-releases", ctx.repo));
    let output = runner.capture(cmd, "checking immutable GitHub Releases")?;
    if runner.dry_run() {
        return Ok(());
    }

    let state: serde_json::Value =
        serde_json::from_str(&output).map_err(|error| anyhow::anyhow!(error))?;
    let enabled = state
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !enabled {
        bail!("GitHub immutable releases must be enabled before creating a release Draft");
    }
    Ok(())
}

fn create_release(runner: &CmdRunner, ctx: &ReleaseContext, target_commit: &str) -> Result<()> {
    runner.run(
        create_release_command(ctx, target_commit),
        "creating GitHub Release draft",
    )
}

fn current_head(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<String> {
    let mut cmd = git();
    cmd.arg("rev-parse")
        .arg("--verify")
        .arg("HEAD")
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    remove_updater_signing_env(&mut cmd);
    let output = runner.capture(cmd, "resolving release source commit")?;
    if runner.dry_run() {
        return Ok("<current-HEAD>".to_string());
    }

    let target_commit = output.trim();
    anyhow::ensure!(
        !target_commit.is_empty(),
        "git rev-parse HEAD returned no commit"
    );
    Ok(target_commit.to_string())
}

fn ensure_local_manual_publish_source(
    runner: &CmdRunner,
    ctx: &ReleaseContext,
    source_sha: &str,
) -> Result<()> {
    check_worktree_clean(ctx)?;
    crate::release_validate::ensure_release_source_ready(ctx)?;

    let mut cmd = git();
    cmd.arg("branch")
        .arg("--show-current")
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    remove_updater_signing_env(&mut cmd);
    let branch = runner.capture(cmd, "checking local release branch")?;
    if branch.trim() != "main" {
        bail!(
            "local manual publication must run from main, got `{}`",
            branch.trim()
        );
    }

    let mut cmd = git();
    cmd.arg("rev-parse")
        .arg("--verify")
        .arg("refs/remotes/origin/main")
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    remove_updater_signing_env(&mut cmd);
    let origin_main = runner.capture(cmd, "checking origin/main release source")?;
    if !origin_main.trim().eq_ignore_ascii_case(source_sha) {
        bail!(
            "local release source must exactly match origin/main: source={source_sha}, origin/main={}",
            origin_main.trim()
        );
    }
    Ok(())
}

fn create_release_command(ctx: &ReleaseContext, target_commit: &str) -> ProcessCommand {
    let mut cmd = gh();
    cmd.arg("release").arg("create").arg(&ctx.tag);
    for asset in ctx.expected_public_asset_names() {
        cmd.arg(ctx.artifact_path(&asset));
    }
    cmd.arg("--repo")
        .arg(&ctx.repo)
        .arg("--target")
        .arg(target_commit)
        .arg("--title")
        .arg(ctx.release_title())
        .arg("--notes-file")
        .arg(&ctx.release_notes)
        .arg("--draft");

    if ctx.channel.is_prerelease() {
        cmd.arg("--prerelease");
    }

    cmd
}

fn update_release_metadata(
    runner: &CmdRunner,
    ctx: &ReleaseContext,
    target_commit: &str,
) -> Result<()> {
    runner.run(
        update_release_metadata_command(ctx, target_commit),
        "updating GitHub Release metadata",
    )
}

fn update_release_metadata_command(ctx: &ReleaseContext, target_commit: &str) -> ProcessCommand {
    let mut cmd = gh();
    cmd.arg("release")
        .arg("edit")
        .arg(&ctx.tag)
        .arg("--repo")
        .arg(&ctx.repo)
        .arg("--title")
        .arg(ctx.release_title())
        .arg("--target")
        .arg(target_commit)
        .arg("--notes-file")
        .arg(&ctx.release_notes);
    cmd
}

fn upload_assets(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<()> {
    let mut cmd = gh();
    cmd.arg("release").arg("upload").arg(&ctx.tag);
    for asset in ctx.expected_public_asset_names() {
        cmd.arg(ctx.artifact_path(&asset));
    }
    cmd.arg("--repo").arg(&ctx.repo).arg("--clobber");
    runner.run(cmd, "uploading GitHub Release assets")
}

#[cfg(test)]
mod tests {
    use super::{create_release_command, update_release_metadata_command};
    use crate::release_common::{ReleaseChannel, ReleaseContext};
    use std::ffi::OsStr;

    #[test]
    fn release_draft_targets_the_built_commit() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let command = create_release_command(&ctx, "0123456789abcdef");
        let args = command.get_args().collect::<Vec<_>>();
        let target_position = args
            .iter()
            .position(|argument| *argument == OsStr::new("--target"))
            .unwrap();

        assert_eq!(args[target_position + 1], OsStr::new("0123456789abcdef"));
    }

    #[test]
    fn replacement_draft_retargets_the_built_commit() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let command = update_release_metadata_command(&ctx, "fedcba9876543210");
        let args = command.get_args().collect::<Vec<_>>();
        let target_position = args
            .iter()
            .position(|argument| *argument == OsStr::new("--target"))
            .unwrap();

        assert_eq!(args[target_position + 1], OsStr::new("fedcba9876543210"));
    }
}
