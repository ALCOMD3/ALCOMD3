use crate::release_common::{
    CmdRunner, ReleaseAutomation, ReleaseChannel, ReleaseContext, UpdaterSignaturePurpose,
    cargo_xtask, check_public_updater_endpoint, check_worktree_clean, default_repo,
    default_site_base_url, default_target, ensure_github_actions_context, gh, git, npm,
    remove_github_auth_env, validate_full_git_sha, validate_release_source_versions,
    verify_github_release,
};
use anyhow::{Context, Result};
use semver::Version;
use serde::Deserialize;
use std::process::Command as ProcessCommand;
use std::time::Duration;
use toml_edit::DocumentMut;

/// Verify public release assets, commit updater metadata, build the website, and optionally check the public endpoint.
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

    /// Exact source commit targeted by the published GitHub Release.
    #[arg(long)]
    source_sha: Option<String>,

    /// Commit the selected updater JSON after verification.
    #[arg(long)]
    commit: bool,

    /// Push main after committing updater metadata. Requires --commit.
    #[arg(long, requires = "commit")]
    push: bool,

    /// Skip local website check/build.
    #[arg(long)]
    skip_website_build: bool,

    /// Check the public updater endpoint after deployment.
    #[arg(long)]
    check_public: bool,

    /// Maximum public endpoint checks after deployment.
    #[arg(long, default_value_t = 1)]
    public_check_attempts: u32,

    /// Delay between public endpoint checks.
    #[arg(long, default_value_t = 0)]
    public_check_delay_seconds: u64,

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
        let source_sha = self.source_sha.as_deref().unwrap_or("<source-sha>");

        anyhow::ensure!(
            self.public_check_attempts > 0,
            "public-check-attempts must be greater than zero"
        );
        ensure_github_actions_context(&ctx, ReleaseAutomation::Updater, source_sha, self.dry_run)?;
        if !self.dry_run {
            validate_full_git_sha(source_sha)?;
            check_worktree_clean(&ctx)?;
        }
        if self.push {
            ensure_release_branch(&runner, &ctx)?;
        }

        let published_at = verify_github_release(&ctx, &runner, Some(false), Some(source_sha))?;
        let published_at = if runner.dry_run() {
            published_at.unwrap_or_else(|| "<release-published-at>".to_string())
        } else {
            published_at.context("published GitHub Release has no publishedAt timestamp")?
        };
        verify_release_tag_source(&runner, &ctx, source_sha)?;
        verify_immutable_release(&runner, &ctx)?;
        let previous_updater = if self.dry_run {
            None
        } else {
            Some(std::fs::read_to_string(&ctx.updater_json).with_context(|| {
                format!(
                    "reading current updater JSON: {}",
                    ctx.updater_json.display()
                )
            })?)
        };
        let same_version = previous_updater
            .as_deref()
            .map(|source| validate_updater_version_progression(source, &ctx.version))
            .transpose()?
            .unwrap_or(false);
        download_public_release_assets(&runner, &ctx)?;
        verify_downloaded_release_attestations(&runner, &ctx)?;
        verify_downloaded_updater_signatures(&ctx)?;
        let updater_notes = load_release_updater_notes(&runner, &ctx)?;
        generate_updater_json(&runner, &ctx, &updater_notes, &published_at)?;
        verify_with_downloaded_updaters(&runner, &ctx)?;
        if same_version {
            let generated = std::fs::read_to_string(&ctx.updater_json).with_context(|| {
                format!(
                    "reading generated updater JSON: {}",
                    ctx.updater_json.display()
                )
            })?;
            anyhow::ensure!(
                previous_updater.as_deref() == Some(generated.as_str()),
                "same-version updater regeneration changed metadata; explicit recovery is required"
            );
        }

        if !self.skip_website_build {
            build_website(&runner, &ctx)?;
        }

        if self.commit {
            let committed = commit_updater_metadata(&runner, &ctx)?;
            if self.push && committed {
                let mut cmd = git();
                cmd.arg("push")
                    .arg("origin")
                    .arg("main")
                    .current_dir(&ctx.workspace_root);
                runner.run(cmd, "pushing updater metadata")?;
            } else if self.push {
                println!("updater metadata is unchanged; nothing to push");
            }
        }

        if self.check_public {
            if self.dry_run {
                println!(
                    "check public updater endpoint {} up to {} time(s), waiting {} second(s) between attempts",
                    ctx.updater_endpoint,
                    self.public_check_attempts,
                    self.public_check_delay_seconds
                );
            } else {
                retry_public_check(
                    self.public_check_attempts,
                    Duration::from_secs(self.public_check_delay_seconds),
                    || check_public_updater_endpoint(&ctx),
                    std::thread::sleep,
                )?;
            }
        } else {
            println!(
                "deploy website/dist for {} before public endpoint verification",
                ctx.site_base_url
            );
            println!("public updater endpoint: {}", ctx.updater_endpoint);
        }

        Ok(0)
    }
}

fn download_public_release_assets(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<()> {
    if !runner.dry_run() {
        std::fs::create_dir_all(ctx.release_check_dir())?;
    }

    runner.run(
        download_public_release_assets_command(ctx),
        "downloading public release assets",
    )
}

fn download_public_release_assets_command(ctx: &ReleaseContext) -> ProcessCommand {
    let mut cmd = gh();
    cmd.arg("release")
        .arg("download")
        .arg(&ctx.tag)
        .arg("--repo")
        .arg(&ctx.repo);
    for asset in ctx.expected_public_asset_names() {
        cmd.arg("--pattern").arg(asset);
    }
    cmd.arg("--dir")
        .arg(ctx.release_check_dir())
        .arg("--clobber");
    cmd
}

fn verify_immutable_release(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<()> {
    let mut cmd = gh();
    cmd.arg("release")
        .arg("verify")
        .arg(&ctx.tag)
        .arg("--repo")
        .arg(&ctx.repo);
    runner.run(cmd, "verifying immutable GitHub Release attestation")
}

fn verify_downloaded_release_attestations(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<()> {
    for name in ctx.expected_public_asset_names() {
        let asset = ctx.release_check_dir().join(name);
        let mut cmd = gh();
        cmd.arg("release")
            .arg("verify-asset")
            .arg(&ctx.tag)
            .arg(&asset)
            .arg("--repo")
            .arg(&ctx.repo);
        runner.run(cmd, "verifying downloaded release asset attestation")?;
    }
    Ok(())
}

fn verify_downloaded_updater_signatures(ctx: &ReleaseContext) -> Result<()> {
    let public_key = ctx
        .workspace_root
        .join("vrc-get-gui/src/updater-public-key.txt");
    for platform in ctx.resolved_release_platforms() {
        crate::verify_alcom_updater_json::verify_updater_signature_file(
            &ctx.release_check_dir().join(&platform.updater.name),
            &ctx.release_check_dir()
                .join(platform.updater_signature_name()),
            &public_key,
            Some(UpdaterSignaturePurpose::Release),
        )?;
    }
    Ok(())
}

fn verify_release_tag_source(
    runner: &CmdRunner,
    ctx: &ReleaseContext,
    source_sha: &str,
) -> Result<()> {
    let mut cmd = git();
    cmd.arg("rev-parse")
        .arg("--verify")
        .arg(format!("{}^{{commit}}", ctx.tag))
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    let tag_commit = runner.capture(cmd, "resolving release tag source commit")?;

    let cargo_toml = read_tag_file(runner, ctx, "Cargo.toml")?;
    let gui_package = read_tag_file(runner, ctx, "vrc-get-gui/package.json")?;
    let website_package = read_tag_file(runner, ctx, "website/package.json")?;
    if runner.dry_run() {
        return Ok(());
    }

    if !tag_commit.trim().eq_ignore_ascii_case(source_sha) {
        anyhow::bail!(
            "release tag source mismatch: expected {source_sha}, got {}",
            tag_commit.trim()
        );
    }

    validate_release_tag_source_versions(ctx, &cargo_toml, &gui_package, &website_package)
}

fn read_tag_file(runner: &CmdRunner, ctx: &ReleaseContext, path: &str) -> Result<String> {
    let mut cmd = git();
    cmd.arg("show")
        .arg(format!("{}:{path}", ctx.tag))
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    runner.capture(cmd, &format!("reading {path} from release tag"))
}

#[derive(Deserialize)]
struct TaggedPackageJson {
    version: String,
}

fn validate_release_tag_source_versions(
    ctx: &ReleaseContext,
    cargo_toml: &str,
    gui_package: &str,
    website_package: &str,
) -> Result<()> {
    let cargo = cargo_toml
        .parse::<DocumentMut>()
        .context("parsing Cargo.toml from release tag")?;
    let workspace_version = cargo["workspace"]["package"]["version"]
        .as_str()
        .context("release tag Cargo.toml has no workspace.package.version")?;
    let gui: TaggedPackageJson =
        serde_json::from_str(gui_package).context("parsing GUI package.json from release tag")?;
    let website: TaggedPackageJson = serde_json::from_str(website_package)
        .context("parsing Website package.json from release tag")?;

    validate_release_source_versions(
        &ctx.version,
        &[workspace_version.to_string()],
        &gui.version,
        &website.version,
    )
}

fn validate_updater_version_progression(current_json: &str, target_version: &str) -> Result<bool> {
    let current: serde_json::Value =
        serde_json::from_str(current_json).context("parsing current updater JSON")?;
    let current_version = current
        .get("version")
        .and_then(serde_json::Value::as_str)
        .context("current updater JSON has no string version")?;
    if current_version == target_version {
        return Ok(true);
    }

    let current = Version::parse(current_version)
        .with_context(|| format!("current updater version is not SemVer: {current_version}"))?;
    let target = Version::parse(target_version)
        .with_context(|| format!("target updater version is not SemVer: {target_version}"))?;
    anyhow::ensure!(
        target > current,
        "refusing updater version rollback: current={current_version}, target={target_version}"
    );
    Ok(false)
}

fn load_release_updater_notes(
    runner: &CmdRunner,
    ctx: &ReleaseContext,
) -> Result<std::path::PathBuf> {
    let destination = ctx.release_check_dir().join("updater-notes.json");
    let notes = runner.capture(
        release_updater_notes_command(ctx),
        "reading updater notes from release tag",
    )?;
    if !runner.dry_run() {
        std::fs::write(&destination, notes)
            .with_context(|| format!("writing {}", destination.display()))?;
    }
    Ok(destination)
}

fn release_updater_notes_command(ctx: &ReleaseContext) -> ProcessCommand {
    let mut cmd = git();
    cmd.arg("show")
        .arg(format!(
            "{}:release-notes/ALCOMD3_{}.updater-notes.json",
            ctx.tag, ctx.version
        ))
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    cmd
}

fn generate_updater_json(
    runner: &CmdRunner,
    ctx: &ReleaseContext,
    updater_notes: &std::path::Path,
    published_at: &str,
) -> Result<()> {
    runner.run(
        generate_updater_json_command(ctx, updater_notes, published_at),
        "generating updater JSON from public release assets",
    )
}

fn generate_updater_json_command(
    ctx: &ReleaseContext,
    updater_notes: &std::path::Path,
    published_at: &str,
) -> ProcessCommand {
    let mut cmd = cargo_xtask();
    cmd.arg("alcom-updater-json")
        .arg("--assets")
        .arg(ctx.release_check_dir())
        .arg("--version")
        .arg(&ctx.version)
        .arg("--updater-notes")
        .arg(updater_notes)
        .arg("--pub-date")
        .arg(published_at)
        .arg(&ctx.updater_json);
    remove_github_auth_env(&mut cmd);
    cmd
}

fn verify_with_downloaded_updaters(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<()> {
    let mut cmd = cargo_xtask();
    cmd.arg("verify-alcom-updater-json")
        .arg("--assets")
        .arg(ctx.release_check_dir())
        .arg("--json")
        .arg(&ctx.updater_json)
        .arg("--expected-signature-purpose")
        .arg(UpdaterSignaturePurpose::Release.to_string());
    remove_github_auth_env(&mut cmd);
    runner.run(cmd, "verifying updater JSON against public updater assets")
}

fn ensure_release_branch(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<()> {
    let mut cmd = git();
    cmd.arg("branch")
        .arg("--show-current")
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    let branch = runner.capture(cmd, "checking updater metadata branch")?;
    if runner.dry_run() {
        return Ok(());
    }
    validate_release_branch(branch.trim())
}

fn validate_release_branch(branch: &str) -> Result<()> {
    anyhow::ensure!(
        branch == "main",
        "updater metadata must be committed and pushed from main, got `{branch}`"
    );
    Ok(())
}

fn commit_updater_metadata(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<bool> {
    let mut cmd = git();
    cmd.arg("add")
        .arg(&ctx.updater_json)
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    runner.run(cmd, "staging updater metadata")?;

    let mut cmd = git();
    cmd.arg("diff")
        .arg("--cached")
        .arg("--name-only")
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    let staged = runner.capture(cmd, "checking staged updater metadata")?;
    let expected = ctx
        .updater_json
        .strip_prefix(&ctx.workspace_root)
        .map_err(|_| anyhow::anyhow!("updater JSON is outside the workspace"))?
        .to_string_lossy()
        .replace('\\', "/");
    if !runner.dry_run() && !validate_staged_updater_paths(&expected, &staged)? {
        println!("updater metadata is already current");
        return Ok(false);
    }

    let mut cmd = git();
    cmd.arg("status")
        .arg("--short")
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    runner.run(cmd, "showing staged updater metadata")?;

    let mut cmd = git();
    cmd.arg("commit")
        .arg("-m")
        .arg(format!(
            "release: publish ALCOMD3 {} updater metadata",
            ctx.version
        ))
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    runner.run(cmd, "committing updater metadata")?;
    Ok(true)
}

fn validate_staged_updater_paths(expected: &str, output: &str) -> Result<bool> {
    let staged = output
        .lines()
        .map(|path| path.trim().replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    if staged.is_empty() {
        return Ok(false);
    }

    for path in staged {
        if path != expected {
            anyhow::bail!("unexpected staged file while publishing updater metadata: {path}");
        }
    }
    Ok(true)
}

fn build_website(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<()> {
    let mut cmd = npm();
    cmd.arg("run")
        .arg("check")
        .current_dir(ctx.workspace_root.join("website"));
    remove_github_auth_env(&mut cmd);
    runner.run(cmd, "website check")?;

    let mut cmd = npm();
    cmd.arg("run")
        .arg("build")
        .current_dir(ctx.workspace_root.join("website"));
    remove_github_auth_env(&mut cmd);
    runner.run(cmd, "website build")
}

fn retry_public_check<F, S>(
    attempts: u32,
    delay: Duration,
    mut check: F,
    mut sleep: S,
) -> Result<()>
where
    F: FnMut() -> Result<()>,
    S: FnMut(Duration),
{
    anyhow::ensure!(
        attempts > 0,
        "public check attempts must be greater than zero"
    );

    for attempt in 1..=attempts {
        match check() {
            Ok(()) => return Ok(()),
            Err(error) if attempt == attempts => return Err(error),
            Err(error) => {
                eprintln!("public updater check {attempt}/{attempts} failed: {error:#}; retrying");
                sleep(delay);
            }
        }
    }

    unreachable!("positive public check attempt count must return from the loop")
}

#[cfg(test)]
mod tests {
    use super::{
        download_public_release_assets_command, generate_updater_json_command,
        release_updater_notes_command, retry_public_check, validate_release_branch,
        validate_release_tag_source_versions, validate_staged_updater_paths,
        validate_updater_version_progression,
    };
    use crate::release_common::{GH_TOKEN_ENV, GITHUB_TOKEN_ENV, ReleaseChannel, ReleaseContext};
    use anyhow::{Result, bail};
    use std::cell::Cell;
    use std::ffi::OsStr;
    use std::time::Duration;

    #[test]
    fn release_updater_downloads_exact_release_asset_allowlist() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let command = download_public_release_assets_command(&ctx);
        let args = command.get_args().collect::<Vec<_>>();

        for asset in ctx.expected_public_asset_names() {
            assert!(args.contains(&OsStr::new(&asset)));
        }
    }

    #[test]
    fn release_updater_uses_tag_notes_and_published_date() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let notes_command = release_updater_notes_command(&ctx);
        let notes_args = notes_command.get_args().collect::<Vec<_>>();
        assert!(notes_args.contains(&OsStr::new(
            "v2.1.1:release-notes/ALCOMD3_2.1.1.updater-notes.json"
        )));

        let notes_path = ctx.release_check_dir().join("updater-notes.json");
        let generation = generate_updater_json_command(&ctx, &notes_path, "2026-07-12T01:02:03Z");
        let generation_args = generation.get_args().collect::<Vec<_>>();
        assert!(generation_args.contains(&OsStr::new("--pub-date")));
        assert!(generation_args.contains(&OsStr::new("2026-07-12T01:02:03Z")));
        assert!(generation_args.contains(&notes_path.as_os_str()));

        for command in [notes_command, generation] {
            let removed = command
                .get_envs()
                .filter(|(_, value)| value.is_none())
                .map(|(name, _)| name.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert!(removed.contains(&GH_TOKEN_ENV.to_string()));
            assert!(removed.contains(&GITHUB_TOKEN_ENV.to_string()));
        }
    }

    #[test]
    fn release_updater_public_check_retries_until_success() {
        let attempts = Cell::new(0);

        retry_public_check(
            3,
            Duration::ZERO,
            || -> Result<()> {
                attempts.set(attempts.get() + 1);
                if attempts.get() < 3 {
                    bail!("not deployed")
                }
                Ok(())
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(attempts.get(), 3);
    }

    #[test]
    fn release_updater_public_check_returns_final_error() {
        let attempts = Cell::new(0);

        let error = retry_public_check(
            2,
            Duration::ZERO,
            || -> Result<()> {
                attempts.set(attempts.get() + 1);
                bail!("still stale")
            },
            |_| {},
        )
        .unwrap_err();

        assert_eq!(attempts.get(), 2);
        assert!(error.to_string().contains("still stale"));
    }

    #[test]
    fn unchanged_updater_metadata_needs_no_commit() {
        assert!(
            !validate_staged_updater_paths("website/public/api/gui/tauri-updater.json", "",)
                .unwrap()
        );
    }

    #[test]
    fn updater_metadata_commit_rejects_unrelated_staged_files() {
        let error = validate_staged_updater_paths(
            "website/public/api/gui/tauri-updater.json",
            "Cargo.toml\nwebsite/public/api/gui/tauri-updater.json\n",
        )
        .unwrap_err();

        assert!(error.to_string().contains("unexpected staged file"));
    }

    #[test]
    fn updater_metadata_push_requires_main_branch() {
        let error = validate_release_branch("feature/release-test").unwrap_err();

        assert!(error.to_string().contains("main"));
    }

    #[test]
    fn updater_version_progression_accepts_newer_or_identical_versions() {
        assert!(validate_updater_version_progression(r#"{"version":"2.1.1"}"#, "2.1.1").unwrap());
        assert!(!validate_updater_version_progression(r#"{"version":"2.1.1"}"#, "2.2.0").unwrap());
    }

    #[test]
    fn updater_version_progression_rejects_rollback() {
        let error =
            validate_updater_version_progression(r#"{"version":"2.1.1"}"#, "2.1.0").unwrap_err();

        assert!(error.to_string().contains("rollback"));
    }

    #[test]
    fn release_tag_source_versions_must_match_release_version() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let cargo_toml = r#"
[workspace]
[workspace.package]
version = "2.1.1"
"#;

        validate_release_tag_source_versions(
            &ctx,
            cargo_toml,
            r#"{"version":"2.1.1"}"#,
            r#"{"version":"2.1.1"}"#,
        )
        .unwrap();

        let error = validate_release_tag_source_versions(
            &ctx,
            cargo_toml,
            r#"{"version":"2.1.0"}"#,
            r#"{"version":"2.1.1"}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("vrc-get-gui"));
    }
}
