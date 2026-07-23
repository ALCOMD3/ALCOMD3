use crate::release_common::{
    CmdRunner, ReleaseChannel, ReleaseContext, cargo, check_worktree_clean,
    create_release_notes_if_missing, default_repo, default_site_base_url, default_target, git, npm,
    update_workspace_version,
};
use crate::utils::command::CommandExt;
use anyhow::Result;

/// Prepare release source files: workspace version, npm versions, lockfiles, and release notes.
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

    /// Build target used by later release steps.
    #[arg(long, default_value_t = default_target())]
    target: String,

    /// Do not fail when the worktree already contains changes.
    #[arg(long)]
    allow_dirty: bool,

    /// Do not refresh npm lockfiles.
    #[arg(long)]
    skip_lockfile: bool,

    /// Print planned commands and file changes without executing them.
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

        if !self.allow_dirty && !self.dry_run {
            check_worktree_clean(&ctx)?;
        }

        update_workspace_version(&ctx, self.dry_run)?;
        refresh_cargo_lockfile(&runner, &ctx)?;
        update_npm_package(&runner, &ctx, "vrc-get-gui")?;
        update_npm_package(&runner, &ctx, "website")?;

        if !self.skip_lockfile {
            refresh_npm_lockfile(&runner, &ctx, "vrc-get-gui")?;
            refresh_npm_lockfile(&runner, &ctx, "website")?;
        }

        create_release_notes_if_missing(&ctx, self.dry_run)?;
        print_git_status(&ctx)?;

        println!(
            "release source prepared for {} ({})",
            ctx.version, ctx.channel
        );
        println!("next: edit {}", ctx.release_notes.display());
        Ok(0)
    }
}

fn update_npm_package(runner: &CmdRunner, ctx: &ReleaseContext, dir: &str) -> Result<()> {
    let mut cmd = npm();
    cmd.arg("version")
        .arg(&ctx.version)
        .arg("--no-git-tag-version")
        .current_dir(ctx.workspace_root.join(dir));
    runner.run(cmd, &format!("updating npm version in {dir}"))
}

fn refresh_cargo_lockfile(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<()> {
    let mut cmd = cargo();
    cmd.arg("update")
        .arg("--workspace")
        .arg("--offline")
        .current_dir(&ctx.workspace_root);
    runner.run(cmd, "refreshing Cargo.lock workspace package versions")
}

fn refresh_npm_lockfile(runner: &CmdRunner, ctx: &ReleaseContext, dir: &str) -> Result<()> {
    let mut cmd = npm();
    cmd.arg("install")
        .arg("--package-lock-only")
        .current_dir(ctx.workspace_root.join(dir));
    runner.run(cmd, &format!("refreshing npm lockfile in {dir}"))
}

fn print_git_status(ctx: &ReleaseContext) -> Result<()> {
    let mut cmd = git();
    cmd.arg("status")
        .arg("--short")
        .current_dir(&ctx.workspace_root);
    let status = cmd.run_capture_checked("checking release prepare diff")?;
    if status.trim().is_empty() {
        println!("git status: clean");
    } else {
        println!("{status}");
    }
    Ok(())
}
