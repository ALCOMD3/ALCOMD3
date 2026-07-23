use crate::release_assets::{
    ReleaseManifestPaths, assemble_release_build_manifest, expected_public_asset_names,
    verify_artifact_directory_allowlist, verify_release_build_manifest,
    verify_release_build_shards,
};
use crate::release_common::{
    CmdRunner, ReleaseAutomation, ReleaseChannel, ReleaseContext, UpdaterSignaturePurpose,
    cargo_xtask, check_worktree_clean, current_head, default_repo, default_site_base_url,
    ensure_github_actions_context, remove_updater_signing_env, run_sign_updater_asset,
    validate_full_git_sha,
};
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Verify platform build shards, sign updater payloads, and assemble the exact release manifest.
#[derive(clap::Parser)]
pub struct Command {
    /// Release version, for example 2.2.0 or 2.2.0-beta.1.
    #[arg(long)]
    version: String,

    /// Release channel.
    #[arg(long, value_enum, default_value_t = ReleaseChannel::Stable)]
    channel: ReleaseChannel,

    /// Exact source commit used by every platform build.
    #[arg(long)]
    source_sha: String,

    /// GitHub repository in OWNER/REPO form.
    #[arg(long, default_value_t = default_repo())]
    repo: String,

    /// Public website base URL.
    #[arg(long, default_value_t = default_site_base_url())]
    site_base_url: String,

    /// Updater private key loader used outside GitHub Actions.
    #[arg(long = "key-loader", alias = "key-script", default_value = ".env")]
    key_loader: PathBuf,

    /// Restrict assembly to the trusted release Draft workflow.
    #[arg(long)]
    github_actions_release: bool,

    /// Print planned signing and verification commands without executing them.
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
        let runner = CmdRunner::new(self.dry_run);
        let platforms = ctx.resolved_release_platforms();
        let artifact_dir = ctx.artifact_dir();
        let shard_dir = ctx.release_build_shard_dir();
        let manifest_path = ctx.release_build_manifest();
        let paths = ReleaseManifestPaths {
            version: &ctx.version,
            channel: ctx.channel,
            source_sha: &self.source_sha,
            artifact_dir: &artifact_dir,
            shard_dir: &shard_dir,
            manifest_path: &manifest_path,
        };

        if !self.dry_run {
            validate_full_git_sha(&self.source_sha)?;
            check_worktree_clean(&ctx)?;
            crate::release_validate::ensure_release_source_ready(&ctx)?;
            let head = current_head(&ctx)?;
            anyhow::ensure!(
                head.eq_ignore_ascii_case(&self.source_sha),
                "release assembly source does not match the checked out commit"
            );
            verify_release_build_shards(&platforms, &paths)?;
        }
        if self.github_actions_release {
            ensure_github_actions_context(
                &ctx,
                ReleaseAutomation::Draft,
                &self.source_sha,
                self.dry_run,
            )?;
        }

        let key_loader = ctx.workspace_root.join(self.key_loader);
        let public_key = ctx
            .workspace_root
            .join("vrc-get-gui/src/updater-public-key.txt");
        for platform in &platforms {
            let updater = ctx.artifact_path(&platform.updater.name);
            run_sign_updater_asset(
                &ctx.workspace_root,
                &updater,
                &runner,
                &key_loader,
                UpdaterSignaturePurpose::Release,
            )?;
            if !self.dry_run {
                crate::verify_alcom_updater_json::verify_updater_signature_file(
                    &updater,
                    &ctx.artifact_path(&platform.updater_signature_name()),
                    &public_key,
                    Some(UpdaterSignaturePurpose::Release),
                )?;
            }
        }

        let verification_json = ctx
            .release_build_shard_dir()
            .join("updater-verification.json");
        if !self.dry_run {
            let parent = verification_json
                .parent()
                .context("updater verification JSON has no parent")?;
            std::fs::create_dir_all(parent)?;
        }
        let mut cmd = cargo_xtask();
        cmd.arg("alcom-updater-json")
            .arg("--assets")
            .arg(ctx.artifact_dir())
            .arg("--version")
            .arg(&ctx.version)
            .arg("--updater-notes")
            .arg(ctx.updater_notes())
            .arg(&verification_json)
            .current_dir(&ctx.workspace_root);
        remove_updater_signing_env(&mut cmd);
        runner.run(cmd, "generating three-platform updater verification JSON")?;

        let mut cmd = cargo_xtask();
        cmd.arg("verify-alcom-updater-json")
            .arg("--assets")
            .arg(ctx.artifact_dir())
            .arg("--json")
            .arg(&verification_json)
            .arg("--expected-signature-purpose")
            .arg(UpdaterSignaturePurpose::Release.to_string())
            .current_dir(&ctx.workspace_root);
        remove_updater_signing_env(&mut cmd);
        runner.run(cmd, "verifying three-platform updater payloads")?;

        if !self.dry_run {
            let expected = expected_public_asset_names(&platforms);
            verify_artifact_directory_allowlist(&ctx.artifact_dir(), &expected)?;
            assemble_release_build_manifest(&platforms, &paths)?;
            verify_release_build_manifest(&platforms, &paths)?;
        }

        println!(
            "assembled release assets are ready: {}",
            ctx.artifact_dir().display()
        );
        Ok(0)
    }
}
