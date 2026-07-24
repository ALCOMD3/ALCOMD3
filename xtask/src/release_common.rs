use crate::alcomd3_config::{Alcomd3Config, UpdaterManifest};
use crate::utils::command::{CommandExt, create_command};
use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use semver::Version;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use toml_edit::{DocumentMut, value};

const DEFAULT_TARGET: &str = "x86_64-pc-windows-msvc";
pub const GH_TOKEN_ENV: &str = "GH_TOKEN";
pub const GITHUB_TOKEN_ENV: &str = "GITHUB_TOKEN";
pub const UPDATER_PRIVATE_KEY_ENV: &str = "ALCOMD3_UPDATER_PRIVATE_KEY";
pub const UPDATER_PRIVATE_KEY_PASSWORD_ENV: &str = "ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD";
const GITHUB_ACTIONS_ENV: &str = "GITHUB_ACTIONS";
const GITHUB_EVENT_NAME_ENV: &str = "GITHUB_EVENT_NAME";
const GITHUB_REF_ENV: &str = "GITHUB_REF";
const GITHUB_REPOSITORY_ENV: &str = "GITHUB_REPOSITORY";
const GITHUB_SHA_ENV: &str = "GITHUB_SHA";
const GITHUB_WORKFLOW_REF_ENV: &str = "GITHUB_WORKFLOW_REF";
const RELEASE_DRAFT_WORKFLOW: &str = ".github/workflows/release-draft.yml";
const RELEASE_UPDATER_WORKFLOW: &str = ".github/workflows/release-updater.yml";

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ReleaseChannel {
    Stable,
    Beta,
}

impl ReleaseChannel {
    pub fn is_prerelease(self) -> bool {
        matches!(self, Self::Beta)
    }

    pub fn updater_manifest<'a>(self, config: &'a Alcomd3Config) -> &'a UpdaterManifest {
        match self {
            Self::Stable => config.stable_updater_manifest(),
            Self::Beta => config.beta_updater_manifest(),
        }
    }

    pub fn updater_endpoint(self, config: &Alcomd3Config, site_base_url: &str) -> String {
        let suffix = &self.updater_manifest(config).public_path;
        format!(
            "{}/{}",
            site_base_url.trim_end_matches('/'),
            suffix.trim_start_matches('/')
        )
    }
}

impl fmt::Display for ReleaseChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stable => f.write_str("stable"),
            Self::Beta => f.write_str("beta"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum UpdaterSignaturePurpose {
    LocalTest,
    Release,
}

impl fmt::Display for UpdaterSignaturePurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalTest => f.write_str("local-test"),
            Self::Release => f.write_str("release"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAutomation {
    Draft,
    Updater,
}

impl ReleaseAutomation {
    fn event_name(self) -> &'static str {
        match self {
            Self::Draft => "workflow_dispatch",
            Self::Updater => "release",
        }
    }

    fn workflow_path(self) -> &'static str {
        match self {
            Self::Draft => RELEASE_DRAFT_WORKFLOW,
            Self::Updater => RELEASE_UPDATER_WORKFLOW,
        }
    }

    fn expected_ref(self, ctx: &ReleaseContext) -> String {
        match self {
            Self::Draft => "refs/heads/main".to_string(),
            Self::Updater => format!("refs/tags/{}", ctx.tag),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReleaseContext {
    pub version: String,
    pub channel: ReleaseChannel,
    pub repo: String,
    pub workspace_root: PathBuf,
    pub tag: String,
    pub release_notes: PathBuf,
    pub updater_json: PathBuf,
    pub updater_endpoint: String,
    pub config: Alcomd3Config,
}

impl ReleaseContext {
    pub fn new(
        version: impl Into<String>,
        channel: ReleaseChannel,
        repo: Option<String>,
        site_base_url: Option<String>,
        _target: Option<String>,
    ) -> Result<Self> {
        let version = version.into();
        validate_version_for_channel(&version, channel)?;

        let metadata = crate::utils::cargo::cargo_metadata();
        let workspace_root = metadata.workspace_root.as_std_path().to_path_buf();
        let config = Alcomd3Config::load_from_workspace(&workspace_root)?;

        let repo = repo.unwrap_or_else(|| config.repository.clone());
        let site_base_url = site_base_url.unwrap_or_else(|| config.site_base_url().to_string());
        let tag = format!("v{version}");
        let release_notes = workspace_root
            .join("release-notes")
            .join(format!("ALCOMD3_{version}.md"));
        let updater_json = config.workspace_path(
            &channel.updater_manifest(&config).output_path,
            &workspace_root,
        );
        let updater_endpoint = channel.updater_endpoint(&config, &site_base_url);

        Ok(Self {
            version,
            channel,
            repo,
            workspace_root,
            tag,
            release_notes,
            updater_json,
            updater_endpoint,
            config,
        })
    }

    pub fn artifact_dir(&self) -> PathBuf {
        self.workspace_root
            .join("artifacts")
            .join("release")
            .join(&self.tag)
    }

    pub fn local_test_artifact_dir(&self) -> PathBuf {
        self.workspace_root
            .join("artifacts")
            .join("local-test")
            .join(&self.tag)
    }

    pub fn release_build_manifest(&self) -> PathBuf {
        self.workspace_root
            .join("artifacts")
            .join("release-state")
            .join(format!("{}.json", self.tag))
    }

    pub fn release_build_shard_dir(&self) -> PathBuf {
        self.workspace_root
            .join("artifacts")
            .join("release-state")
            .join(&self.tag)
    }

    pub fn resolved_release_platforms(
        &self,
    ) -> Vec<crate::release_assets::ResolvedReleasePlatform> {
        crate::release_assets::resolve_release_platforms(
            &self.config,
            &self.workspace_root,
            &self.version,
        )
    }

    pub fn expected_public_asset_names(&self) -> Vec<String> {
        crate::release_assets::expected_public_asset_names(&self.resolved_release_platforms())
    }

    pub fn artifact_path(&self, name: &str) -> PathBuf {
        self.artifact_dir().join(name)
    }

    pub fn release_check_dir(&self) -> PathBuf {
        self.workspace_root
            .join("artifacts")
            .join("release-check")
            .join(&self.tag)
    }

    pub fn release_title(&self) -> String {
        format!("Version {}", self.version)
    }

    pub fn updater_notes(&self) -> PathBuf {
        self.workspace_root
            .join("release-notes")
            .join(format!("ALCOMD3_{}.updater-notes.json", self.version))
    }

    pub fn release_notes_comparison_base(&self) -> &'static str {
        match self.channel {
            ReleaseChannel::Stable => {
                "compare this stable release against the previous stable release"
            }
            ReleaseChannel::Beta => {
                "compare this beta release against the immediately previous release, stable or beta"
            }
        }
    }
}

pub struct CmdRunner {
    dry_run: bool,
}

impl CmdRunner {
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }

    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    pub fn run(&self, mut cmd: ProcessCommand, what: &str) -> Result<()> {
        println!("$ {}", cmd.display_command());
        if self.dry_run {
            return Ok(());
        }
        cmd.run_checked(what)
    }

    pub fn capture(&self, mut cmd: ProcessCommand, what: &str) -> Result<String> {
        println!("$ {}", cmd.display_command());
        if self.dry_run {
            return Ok(String::new());
        }
        cmd.run_capture_checked(what)
    }
}

pub fn default_repo() -> String {
    Alcomd3Config::load()
        .map(|config| config.repository)
        .unwrap_or_else(|error| panic!("failed to load alcomd3.config.json: {error:#}"))
}

pub fn default_site_base_url() -> String {
    Alcomd3Config::load()
        .map(|config| config.site_base_url().to_string())
        .unwrap_or_else(|error| panic!("failed to load alcomd3.config.json: {error:#}"))
}

pub fn validate_release_source_versions(
    expected: &str,
    workspace_versions: &[String],
    gui_version: &str,
) -> Result<()> {
    for version in workspace_versions {
        if version != expected {
            bail!("workspace package version mismatch: expected {expected}, got {version}");
        }
    }
    if gui_version != expected {
        bail!("vrc-get-gui package version mismatch: expected {expected}, got {gui_version}");
    }
    Ok(())
}

pub fn default_target() -> String {
    DEFAULT_TARGET.to_string()
}

pub fn validate_version_for_channel(version: &str, channel: ReleaseChannel) -> Result<()> {
    let parsed = Version::parse(version).with_context(|| format!("invalid SemVer: {version}"))?;

    match channel {
        ReleaseChannel::Stable if !parsed.pre.is_empty() => {
            bail!("stable release version must not contain prerelease metadata: {version}")
        }
        ReleaseChannel::Beta if parsed.pre.is_empty() => {
            bail!("beta release version must contain prerelease metadata: {version}")
        }
        _ => Ok(()),
    }
}

pub fn validate_full_git_sha(source_sha: &str) -> Result<()> {
    if source_sha.len() != 40 || !source_sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("release source SHA must be a full 40-character hexadecimal commit ID");
    }
    Ok(())
}

pub fn ensure_github_actions_context(
    ctx: &ReleaseContext,
    automation: ReleaseAutomation,
    source_sha: &str,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        println!(
            "require GitHub Actions context: {} on {} at {source_sha}",
            automation.workflow_path(),
            automation.event_name()
        );
        return Ok(());
    }

    let github_actions = required_env(GITHUB_ACTIONS_ENV)?;
    let event_name = required_env(GITHUB_EVENT_NAME_ENV)?;
    let github_ref = required_env(GITHUB_REF_ENV)?;
    let repository = required_env(GITHUB_REPOSITORY_ENV)?;
    let github_sha = required_env(GITHUB_SHA_ENV)?;
    let workflow_ref = required_env(GITHUB_WORKFLOW_REF_ENV)?;

    validate_github_actions_context(
        ctx,
        automation,
        source_sha,
        &github_actions,
        &event_name,
        &github_ref,
        &repository,
        &github_sha,
        &workflow_ref,
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_github_actions_context(
    ctx: &ReleaseContext,
    automation: ReleaseAutomation,
    source_sha: &str,
    github_actions: &str,
    event_name: &str,
    github_ref: &str,
    repository: &str,
    github_sha: &str,
    workflow_ref: &str,
) -> Result<()> {
    validate_full_git_sha(source_sha)?;
    if github_actions != "true" {
        bail!("release publication commands may only run in GitHub Actions");
    }
    let updater_dispatch =
        automation == ReleaseAutomation::Updater && event_name == "workflow_dispatch";
    if event_name != automation.event_name() && !updater_dispatch {
        let expected_event = if automation == ReleaseAutomation::Updater {
            "release or workflow_dispatch"
        } else {
            automation.event_name()
        };
        bail!("unexpected GitHub Actions event: expected {expected_event}, got {event_name}");
    }

    let expected_ref = if updater_dispatch {
        "refs/heads/main".to_string()
    } else {
        automation.expected_ref(ctx)
    };
    if github_ref != expected_ref {
        bail!("unexpected GitHub ref: expected {expected_ref}, got {github_ref}");
    }
    if !repository.eq_ignore_ascii_case(&ctx.repo) {
        bail!(
            "unexpected GitHub repository: expected {}, got {repository}",
            ctx.repo
        );
    }
    validate_full_git_sha(github_sha)?;
    if !updater_dispatch && !github_sha.eq_ignore_ascii_case(source_sha) {
        bail!("GitHub event SHA does not match the requested release source SHA");
    }

    let expected_workflow_ref = format!("{}/{}@", ctx.repo, automation.workflow_path());
    if !workflow_ref
        .to_ascii_lowercase()
        .starts_with(&expected_workflow_ref.to_ascii_lowercase())
    {
        bail!(
            "unexpected GitHub workflow: expected {}, got {workflow_ref}",
            automation.workflow_path()
        );
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String> {
    std::env::var(name)
        .with_context(|| format!("{name} is required for GitHub Actions release automation"))
}

pub fn cargo_xtask() -> ProcessCommand {
    ProcessCommand::new(std::env::current_exe().expect("failed to locate current xtask executable"))
}

pub fn cargo() -> ProcessCommand {
    create_command("cargo")
}

pub fn npm() -> ProcessCommand {
    create_command("npm")
}

pub fn git() -> ProcessCommand {
    create_command("git")
}

pub fn gh() -> ProcessCommand {
    create_command("gh")
}

pub fn remove_updater_signing_env(cmd: &mut ProcessCommand) {
    cmd.env_remove(UPDATER_PRIVATE_KEY_ENV)
        .env_remove(UPDATER_PRIVATE_KEY_PASSWORD_ENV);
}

pub fn remove_github_auth_env(cmd: &mut ProcessCommand) {
    cmd.env_remove(GH_TOKEN_ENV).env_remove(GITHUB_TOKEN_ENV);
}

pub fn check_worktree_clean(ctx: &ReleaseContext) -> Result<()> {
    let mut cmd = git();
    cmd.arg("status")
        .arg("--short")
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    remove_updater_signing_env(&mut cmd);
    let output = cmd.run_capture_checked("checking git status")?;
    if !output.trim().is_empty() {
        bail!("worktree is not clean:\n{output}");
    }
    Ok(())
}

pub fn current_head(ctx: &ReleaseContext) -> Result<String> {
    let mut cmd = git();
    cmd.arg("rev-parse")
        .arg("--verify")
        .arg("HEAD")
        .current_dir(&ctx.workspace_root);
    remove_github_auth_env(&mut cmd);
    remove_updater_signing_env(&mut cmd);
    let output = cmd.run_capture_checked("resolving release source commit")?;
    let source_sha = output.trim();
    validate_full_git_sha(source_sha)?;
    Ok(source_sha.to_string())
}

pub fn update_workspace_version(ctx: &ReleaseContext, dry_run: bool) -> Result<()> {
    let cargo_toml = ctx.workspace_root.join("Cargo.toml");
    let source = fs::read_to_string(&cargo_toml)
        .with_context(|| format!("reading {}", cargo_toml.display()))?;
    let mut doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("parsing {}", cargo_toml.display()))?;
    doc["workspace"]["package"]["version"] = value(&ctx.version);
    let rendered = doc.to_string();

    if source == rendered {
        println!("version unchanged: {}", cargo_toml.display());
        return Ok(());
    }

    println!(
        "update workspace package version: {} -> {}",
        cargo_toml.display(),
        ctx.version
    );
    if !dry_run {
        fs::write(&cargo_toml, rendered)
            .with_context(|| format!("writing {}", cargo_toml.display()))?;
    }
    Ok(())
}

pub fn create_release_notes_if_missing(ctx: &ReleaseContext, dry_run: bool) -> Result<()> {
    if ctx.release_notes.exists() {
        println!(
            "release notes already exist: {}",
            ctx.release_notes.display()
        );
        return Ok(());
    }

    println!("create release notes: {}", ctx.release_notes.display());
    if dry_run {
        return Ok(());
    }

    let parent = ctx
        .release_notes
        .parent()
        .context("release notes path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    fs::write(
        &ctx.release_notes,
        format!(
            "# ALCOMD3 v{version}\n\n<!-- Release note comparison base: {comparison_base}. Remove all HTML comments before release. -->\n\n## English\n\n<!-- Summarize this release in one paragraph. -->\n\n### Application updates\n\n<!-- List user-visible application changes, or state that there are none. -->\n\n### Website updates\n\n<!-- List user-visible website changes, or state that there are none. -->\n\n### Installation and upgrade\n\n<!-- List installation and upgrade changes, or state that there are none. -->\n\n### Compatibility and security\n\n<!-- List compatibility, security, or known-issue changes, or state that there are none. -->\n\n## 日本語\n\n<!-- このリリースの概要を 1 段落で記述してください。 -->\n\n### アプリの更新\n\n<!-- ユーザーに見えるアプリの変更、または変更がないことを記述してください。 -->\n\n### Web サイトの更新\n\n<!-- ユーザーに見える Web サイトの変更、または変更がないことを記述してください。 -->\n\n### インストールとアップグレード\n\n<!-- インストールとアップグレードの変更、または変更がないことを記述してください。 -->\n\n### 互換性とセキュリティ\n\n<!-- 互換性、セキュリティ、既知の問題の変更、または変更がないことを記述してください。 -->\n\n## 中文\n\n<!-- 用一个段落概述此版本。 -->\n\n### 应用更新\n\n<!-- 列出面向用户的应用变化，或说明没有此类变化。 -->\n\n### 网站更新\n\n<!-- 列出面向用户的网站变化，或说明没有此类变化。 -->\n\n### 安装与升级\n\n<!-- 列出安装与升级变化，或说明没有此类变化。 -->\n\n### 兼容性与安全\n\n<!-- 列出兼容性、安全或已知问题变化，或说明没有此类变化。 -->\n",
            version = ctx.version,
            comparison_base = ctx.release_notes_comparison_base(),
        ),
    )
    .with_context(|| format!("writing {}", ctx.release_notes.display()))?;
    Ok(())
}

pub fn ensure_release_notes_ready(ctx: &ReleaseContext) -> Result<()> {
    let notes = fs::read_to_string(&ctx.release_notes)
        .with_context(|| format!("reading release notes: {}", ctx.release_notes.display()))?;
    if notes.contains("<!--") || notes.contains("-->") {
        bail!("release notes still contain an HTML comment or placeholder");
    }
    validate_release_notes_format(&ctx.version, &notes)?;
    Ok(())
}

fn validate_release_notes_format(version: &str, notes: &str) -> Result<()> {
    const LEGACY_DYNAMIC_RELEASE_NOTES_THROUGH: &str = "2.1.3-beta.2";
    const LOCALES: [(&str, [&str; 4]); 3] = [
        (
            "English",
            [
                "Application updates",
                "Website updates",
                "Installation and upgrade",
                "Compatibility and security",
            ],
        ),
        (
            "日本語",
            [
                "アプリの更新",
                "Web サイトの更新",
                "インストールとアップグレード",
                "互換性とセキュリティ",
            ],
        ),
        (
            "中文",
            ["应用更新", "网站更新", "安装与升级", "兼容性与安全"],
        ),
    ];

    let lines = notes.lines().collect::<Vec<_>>();
    let expected_title = format!("# ALCOMD3 v{version}");
    if lines.first().copied() != Some(expected_title.as_str()) {
        bail!("release notes title must be exactly: {expected_title}");
    }

    if lines.iter().skip(1).any(|line| line.starts_with("# ")) {
        bail!("release notes must contain exactly one level-1 heading");
    }
    if lines.iter().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("```") || trimmed.starts_with("~~~")
    }) {
        bail!("release notes must not contain fenced code blocks");
    }
    if lines.iter().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with('#') && trimmed.len() != line.len()
    }) {
        bail!("release notes headings must not be indented");
    }
    if lines.iter().any(|line| line.starts_with("####")) {
        bail!("release notes must not use level-4 or deeper headings");
    }
    if lines.iter().any(|line| {
        line.starts_with('#')
            && !line.starts_with("# ")
            && !line.starts_with("## ")
            && !line.starts_with("### ")
    }) {
        bail!("release notes headings must use one space after the heading marker");
    }

    let locale_sections = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.strip_prefix("## ").map(|name| (index, name)))
        .collect::<Vec<_>>();
    let locale_names = locale_sections
        .iter()
        .map(|(_, name)| *name)
        .collect::<Vec<_>>();
    let expected_locale_names = LOCALES.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    if locale_names != expected_locale_names {
        bail!(
            "release notes locale headings must be exactly and in order: {}",
            expected_locale_names.join(", ")
        );
    }
    if lines[1..locale_sections[0].0]
        .iter()
        .any(|line| !line.trim().is_empty())
    {
        bail!("release notes must not contain content before the English locale section");
    }

    let release_version = Version::parse(version)
        .with_context(|| format!("invalid release notes SemVer: {version}"))?;
    let legacy_cutoff = Version::parse(LEGACY_DYNAMIC_RELEASE_NOTES_THROUGH)
        .expect("legacy release notes cutoff must be valid SemVer");
    let requires_fixed_categories = release_version > legacy_cutoff;
    let mut legacy_topic_count = None;

    for (locale_index, ((locale_name, expected_topic_headings), (start, _))) in
        LOCALES.iter().zip(locale_sections.iter()).enumerate()
    {
        let end = locale_sections
            .get(locale_index + 1)
            .map(|(index, _)| *index)
            .unwrap_or(lines.len());
        let topic_sections = lines[*start + 1..end]
            .iter()
            .enumerate()
            .filter_map(|(offset, line)| {
                line.strip_prefix("### ")
                    .map(|heading| (*start + 1 + offset, heading))
            })
            .collect::<Vec<_>>();

        if topic_sections
            .iter()
            .any(|(_, heading)| heading.trim().is_empty() || heading.trim() != *heading)
        {
            bail!(
                "release notes locale {locale_name} contains an empty or malformed topic heading"
            );
        }
        let topic_headings = topic_sections
            .iter()
            .map(|(_, heading)| *heading)
            .collect::<Vec<_>>();
        if requires_fixed_categories && topic_headings.as_slice() != expected_topic_headings {
            bail!(
                "release notes locale {locale_name} level-3 headings must be exactly and in order: {}",
                expected_topic_headings.join(", ")
            );
        }
        if !requires_fixed_categories {
            if topic_sections.is_empty() {
                bail!("release notes locale {locale_name} must contain at least one change topic");
            }
            match legacy_topic_count {
                Some(count) if count != topic_sections.len() => {
                    bail!(
                        "legacy release notes locale sections must have the same number of level-3 topics"
                    )
                }
                None => legacy_topic_count = Some(topic_sections.len()),
                _ => {}
            }
        }

        let summary = &lines[*start + 1..topic_sections[0].0];
        let first_summary_line = summary.iter().position(|line| !line.trim().is_empty());
        let last_summary_line = summary.iter().rposition(|line| !line.trim().is_empty());
        let (Some(first_summary_line), Some(last_summary_line)) =
            (first_summary_line, last_summary_line)
        else {
            bail!("release notes locale {locale_name} must start with a summary paragraph");
        };
        if summary[first_summary_line..=last_summary_line]
            .iter()
            .any(|line| {
                let line = line.trim();
                line.is_empty()
                    || line.starts_with("#")
                    || line.starts_with("- ")
                    || line.starts_with("* ")
                    || line.starts_with("+ ")
                    || line.starts_with("> ")
                    || line.split_once(". ").is_some_and(|(prefix, _)| {
                        !prefix.is_empty()
                            && prefix.chars().all(|character| character.is_ascii_digit())
                    })
            })
        {
            bail!(
                "release notes locale {locale_name} must start with exactly one summary paragraph"
            );
        }

        for (topic_index, (topic_start, topic_heading)) in topic_sections.iter().enumerate() {
            let topic_end = topic_sections
                .get(topic_index + 1)
                .map(|(index, _)| *index)
                .unwrap_or(end);
            let mut has_non_empty_bullet = false;
            for line in &lines[*topic_start + 1..topic_end] {
                let Some(bullet) = line.strip_prefix("- ") else {
                    continue;
                };
                if bullet.trim().is_empty() {
                    bail!(
                        "release notes section {locale_name} / {topic_heading} must not contain an empty bullet"
                    );
                }
                has_non_empty_bullet = true;
            }
            if !has_non_empty_bullet {
                bail!(
                    "release notes section {locale_name} / {topic_heading} must contain at least one non-empty bullet"
                );
            }
        }
    }

    Ok(())
}

pub fn run_sign_updater_asset(
    workspace_root: &Path,
    asset: &Path,
    runner: &CmdRunner,
    key_loader: &Path,
    purpose: UpdaterSignaturePurpose,
) -> Result<()> {
    if runner.dry_run() {
        let mut cmd = cargo_xtask();
        cmd.arg("sign-alcom-updater")
            .arg("--purpose")
            .arg(purpose.to_string())
            .arg(asset);
        return runner.run(cmd, "signing updater asset");
    }

    let has_key = std::env::var_os(UPDATER_PRIVATE_KEY_ENV).is_some_and(|value| !value.is_empty());
    let has_password =
        std::env::var_os(UPDATER_PRIVATE_KEY_PASSWORD_ENV).is_some_and(|value| !value.is_empty());

    if has_key && has_password {
        let mut cmd = cargo_xtask();
        cmd.arg("sign-alcom-updater")
            .arg("--purpose")
            .arg(purpose.to_string())
            .arg(asset)
            .current_dir(workspace_root);
        return runner.run(cmd, "signing updater asset");
    }

    let key_loader = resolve_key_loader(key_loader)?;
    match key_loader_format(&key_loader) {
        Some(KeyLoaderFormat::PowerShell) => {
            run_sign_updater_with_ps1_loader(workspace_root, asset, runner, &key_loader, purpose)
        }
        Some(KeyLoaderFormat::DotEnv) => {
            run_sign_updater_with_env_loader(workspace_root, asset, runner, &key_loader, purpose)
        }
        _ => bail!("unsupported updater key loader: {}", key_loader.display()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyLoaderFormat {
    PowerShell,
    DotEnv,
}

fn key_loader_format(path: &Path) -> Option<KeyLoaderFormat> {
    let file_name = path.file_name()?.to_str()?;
    let extension = path.extension().and_then(|extension| extension.to_str());

    if file_name.eq_ignore_ascii_case(".env")
        || extension.is_some_and(|extension| extension.eq_ignore_ascii_case("env"))
    {
        Some(KeyLoaderFormat::DotEnv)
    } else if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("ps1")) {
        Some(KeyLoaderFormat::PowerShell)
    } else {
        None
    }
}

fn run_sign_updater_with_ps1_loader(
    workspace_root: &Path,
    asset: &Path,
    runner: &CmdRunner,
    key_script: &Path,
    purpose: UpdaterSignaturePurpose,
) -> Result<()> {
    let command = format!(
        ". '{}'; cargo xtask sign-alcom-updater --purpose '{}' '{}'",
        ps_quote(key_script),
        purpose,
        ps_quote(asset),
    );

    let mut cmd = if cfg!(windows) {
        create_command("powershell")
    } else {
        create_command("pwsh")
    };
    cmd.arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(command)
        .current_dir(workspace_root);
    runner.run(cmd, "signing updater asset with key loader")
}

fn run_sign_updater_with_env_loader(
    workspace_root: &Path,
    asset: &Path,
    runner: &CmdRunner,
    key_env: &Path,
    purpose: UpdaterSignaturePurpose,
) -> Result<()> {
    let env = read_key_env_loader(key_env)?;
    let private_key = env
        .get(UPDATER_PRIVATE_KEY_ENV)
        .with_context(|| format!("{UPDATER_PRIVATE_KEY_ENV} is missing from updater key loader"))?;
    let password = env.get(UPDATER_PRIVATE_KEY_PASSWORD_ENV).with_context(|| {
        format!("{UPDATER_PRIVATE_KEY_PASSWORD_ENV} is missing from updater key loader")
    })?;

    let mut cmd = cargo_xtask();
    cmd.arg("sign-alcom-updater")
        .arg("--purpose")
        .arg(purpose.to_string())
        .arg(asset)
        .env(UPDATER_PRIVATE_KEY_ENV, private_key)
        .env(UPDATER_PRIVATE_KEY_PASSWORD_ENV, password)
        .current_dir(workspace_root);
    runner.run(cmd, "signing updater asset with env key loader")
}

fn resolve_key_loader(key_loader: &Path) -> Result<PathBuf> {
    if key_loader.is_file() {
        return Ok(key_loader.to_path_buf());
    }

    if key_loader.is_dir() {
        for file_name in ["private-key.ps1", "private-key.env"] {
            let candidate = key_loader.join(file_name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    bail!(
        "updater signing variables are not set and key loader does not exist: {}",
        key_loader.display()
    )
}

fn read_key_env_loader(key_env: &Path) -> Result<HashMap<String, String>> {
    let source =
        fs::read_to_string(key_env).with_context(|| format!("reading {}", key_env.display()))?;
    let mut env = HashMap::new();

    for (index, line) in source.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (name, value) = line.split_once('=').with_context(|| {
            format!(
                "invalid updater key loader line {} in {}",
                index + 1,
                key_env.display()
            )
        })?;
        env.insert(name.trim().to_string(), value.trim().to_string());
    }

    Ok(env)
}

fn capture_github_release(
    ctx: &ReleaseContext,
    runner: &CmdRunner,
) -> Result<Option<(ReleaseState, String)>> {
    let mut cmd = gh();
    cmd.arg("release")
        .arg("view")
        .arg(&ctx.tag)
        .arg("--repo")
        .arg(&ctx.repo)
        .arg("--json")
        .arg("tagName,name,isDraft,isPrerelease,targetCommitish,publishedAt,assets");

    let output = runner.capture(cmd, "viewing GitHub Release")?;
    if runner.dry_run() {
        return Ok(None);
    }

    let state: ReleaseState =
        serde_json::from_str(&output).context("parsing GitHub Release JSON")?;
    Ok(Some((state, output)))
}

pub fn ensure_github_release_is_draft(ctx: &ReleaseContext, runner: &CmdRunner) -> Result<()> {
    let Some((state, _)) = capture_github_release(ctx, runner)? else {
        return Ok(());
    };

    validate_github_release_is_replaceable(ctx, &state)
}

fn validate_github_release_is_replaceable(
    ctx: &ReleaseContext,
    state: &ReleaseState,
) -> Result<()> {
    if !state.is_draft {
        bail!("refusing to replace assets on a published GitHub Release");
    }
    if state.is_prerelease != ctx.channel.is_prerelease() {
        bail!(
            "refusing to replace Draft assets with a mismatched prerelease flag for channel {}",
            ctx.channel
        );
    }
    Ok(())
}

pub fn verify_github_release(
    ctx: &ReleaseContext,
    runner: &CmdRunner,
    expected_draft: Option<bool>,
    expected_target_commit: Option<&str>,
) -> Result<Option<String>> {
    let Some((state, output)) = capture_github_release(ctx, runner)? else {
        return Ok(None);
    };

    validate_github_release_state(ctx, &state, expected_draft, expected_target_commit)?;
    let published_at = state.published_at.clone();
    println!("{output}");
    Ok(published_at)
}

fn validate_github_release_state(
    ctx: &ReleaseContext,
    state: &ReleaseState,
    expected_draft: Option<bool>,
    expected_target_commit: Option<&str>,
) -> Result<()> {
    let expected_title = ctx.release_title();
    let expected_assets = ctx.expected_public_asset_names();

    if state.name != expected_title {
        bail!(
            "GitHub Release title mismatch: expected {expected_title}, got {}",
            state.name
        );
    }

    for expected in &expected_assets {
        if !state.assets.iter().any(|asset| asset.name == *expected) {
            bail!("GitHub Release asset is missing: {expected}");
        }
    }
    for asset in &state.assets {
        if !expected_assets
            .iter()
            .any(|expected| asset.name == *expected)
        {
            bail!("GitHub Release has unexpected asset: {}", asset.name);
        }
    }
    if state.assets.len() != expected_assets.len() {
        bail!(
            "GitHub Release asset count mismatch: expected {}, got {}",
            expected_assets.len(),
            state.assets.len()
        );
    }

    if state.is_prerelease != ctx.channel.is_prerelease() {
        bail!(
            "GitHub Release prerelease flag does not match channel {}",
            ctx.channel
        );
    }
    if let Some(expected_draft) = expected_draft
        && state.is_draft != expected_draft
    {
        bail!(
            "GitHub Release draft state mismatch: expected {expected_draft}, got {}",
            state.is_draft
        );
    }
    if expected_draft == Some(false) && state.published_at.is_none() {
        bail!("published GitHub Release has no publishedAt timestamp");
    }
    if let Some(expected_target_commit) = expected_target_commit
        && state.target_commitish != expected_target_commit
    {
        bail!(
            "GitHub Release target commit mismatch: expected {expected_target_commit}, got {}",
            state.target_commitish
        );
    }
    Ok(())
}

pub fn check_public_updater_endpoint(ctx: &ReleaseContext) -> Result<()> {
    let mut response = crate::utils::ureq()
        .get(&ctx.updater_endpoint)
        .call()
        .with_context(|| format!("requesting {}", ctx.updater_endpoint))?;
    let mut body = String::new();
    response
        .body_mut()
        .as_reader()
        .read_to_string(&mut body)
        .with_context(|| format!("reading {}", ctx.updater_endpoint))?;

    let json: serde_json::Value =
        serde_json::from_str(&body).context("parsing public updater JSON")?;
    let expected_source = std::fs::read_to_string(&ctx.updater_json)
        .with_context(|| format!("reading {}", ctx.updater_json.display()))?;
    let expected_json: serde_json::Value = serde_json::from_str(&expected_source)
        .with_context(|| format!("parsing {}", ctx.updater_json.display()))?;
    let expected_url = updater_url(&expected_json)?;
    let expected_signature = updater_signature(&expected_json)?;

    validate_public_updater_document(&ctx.version, expected_url, expected_signature, &json)?;
    validate_public_updater_matches_expected(&expected_json, &json)?;

    println!("public updater endpoint passed: {}", ctx.updater_endpoint);
    Ok(())
}

pub fn validate_public_updater_document(
    expected_version: &str,
    expected_url: &str,
    expected_signature: &str,
    json: &serde_json::Value,
) -> Result<()> {
    let version = json
        .get("version")
        .and_then(|value| value.as_str())
        .context("public updater JSON has no string version")?;
    if version != expected_version {
        bail!("public updater version mismatch: expected {expected_version}, got {version}");
    }

    let url = updater_url(json)?;
    if url != expected_url {
        bail!("public updater URL mismatch: expected {expected_url}, got {url}");
    }
    let signature = updater_signature(json)?;
    if signature != expected_signature {
        bail!("public updater signature mismatch");
    }

    Ok(())
}

pub fn validate_public_updater_matches_expected(
    expected: &serde_json::Value,
    actual: &serde_json::Value,
) -> Result<()> {
    if actual != expected {
        bail!("public updater JSON does not match generated updater JSON");
    }
    Ok(())
}

fn updater_url(json: &serde_json::Value) -> Result<&str> {
    json.pointer("/platforms/windows-x86_64/url")
        .and_then(|value| value.as_str())
        .context("updater JSON has no platforms.windows-x86_64.url")
}

fn updater_signature(json: &serde_json::Value) -> Result<&str> {
    json.pointer("/platforms/windows-x86_64/signature")
        .and_then(|value| value.as_str())
        .context("updater JSON has no platforms.windows-x86_64.signature")
}

fn ps_quote(path: &Path) -> String {
    path.as_os_str().to_string_lossy().replace('\'', "''")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseState {
    name: String,
    is_draft: bool,
    is_prerelease: bool,
    published_at: Option<String>,
    target_commitish: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::{
        GH_TOKEN_ENV, GITHUB_TOKEN_ENV, KeyLoaderFormat, ReleaseAsset, ReleaseAutomation,
        ReleaseChannel, ReleaseContext, ReleaseState, UPDATER_PRIVATE_KEY_ENV,
        UPDATER_PRIVATE_KEY_PASSWORD_ENV, key_loader_format, remove_github_auth_env,
        remove_updater_signing_env, validate_github_actions_context,
        validate_github_release_is_replaceable, validate_github_release_state,
        validate_public_updater_document, validate_public_updater_matches_expected,
        validate_release_notes_format, validate_release_source_versions,
    };
    use serde_json::json;
    use std::process::Command as ProcessCommand;

    fn expected_release_assets(ctx: &ReleaseContext) -> Vec<ReleaseAsset> {
        ctx.expected_public_asset_names()
            .into_iter()
            .map(|name| ReleaseAsset { name })
            .collect()
    }

    fn valid_release_notes() -> String {
        r#"# ALCOMD3 v2.1.3-beta.3

## English

This beta fixes a user-visible compatibility issue.

### Application updates

- Fixed the compatibility issue.

### Website updates

- No user-visible website changes in this release.

### Installation and upgrade

- No installation or upgrade changes in this release.

### Compatibility and security

- No data migration is required.

## 日本語

この beta ではユーザーに影響する互換性の問題を修正しました。

### アプリの更新

- 互換性の問題を修正しました。

### Web サイトの更新

- このリリースにユーザー向けの Web サイトの変更はありません。

### インストールとアップグレード

- このリリースにインストールまたはアップグレードの変更はありません。

### 互換性とセキュリティ

- データ移行は不要です。

## 中文

此测试版修复了一项影响用户的兼容性问题。

### 应用更新

- 修复兼容性问题。

### 网站更新

- 本版本没有网站方面的用户可见变化。

### 安装与升级

- 本版本没有安装或升级方面的变化。

### 兼容性与安全

- 无需迁移数据。
"#
        .to_string()
    }

    fn legacy_release_notes() -> String {
        r#"# ALCOMD3 v2.1.3-beta.2

## English

This beta fixes package list behavior.

### Package list reliability

- Fixed package list behavior.

## 日本語

この beta ではパッケージ一覧の動作を修正しました。

### パッケージ一覧の信頼性

- パッケージ一覧の動作を修正しました。

## 中文

此测试版修复了软件包列表行为。

### 软件包列表可靠性

- 修复软件包列表行为。
"#
        .to_string()
    }

    #[test]
    fn release_notes_accept_canonical_fixed_categories() {
        validate_release_notes_format("2.1.3-beta.3", &valid_release_notes()).unwrap();
    }

    #[test]
    fn release_notes_accept_published_legacy_categories_through_beta_2() {
        validate_release_notes_format("2.1.3-beta.2", &legacy_release_notes()).unwrap();
    }

    #[test]
    fn release_notes_reject_level_four_topics() {
        let notes = valid_release_notes().replace(
            "### Application updates",
            "### Application updates\n\n#### Fixes",
        );
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(error.to_string().contains("level-4"));
    }

    #[test]
    fn release_notes_reject_title_with_trailing_whitespace() {
        let notes = valid_release_notes().replacen(
            "# ALCOMD3 v2.1.3-beta.3",
            "# ALCOMD3 v2.1.3-beta.3 ",
            1,
        );
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(error.to_string().contains("title must be exactly"));
    }

    #[test]
    fn release_notes_reject_empty_topic_heading() {
        let notes = valid_release_notes().replacen("### Application updates", "### ", 1);
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("empty or malformed topic heading")
        );
    }

    #[test]
    fn release_notes_reject_multiple_summary_paragraphs() {
        let notes = valid_release_notes().replacen(
            "This beta fixes a user-visible compatibility issue.",
            "This beta fixes a user-visible compatibility issue.\n\nThis is a second paragraph.",
            1,
        );
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(error.to_string().contains("exactly one summary paragraph"));
    }

    #[test]
    fn release_notes_reject_list_instead_of_summary_paragraph() {
        let notes = valid_release_notes().replacen(
            "This beta fixes a user-visible compatibility issue.",
            "- This is not a summary paragraph.",
            1,
        );
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(error.to_string().contains("exactly one summary paragraph"));
    }

    #[test]
    fn release_notes_reject_empty_bullet() {
        let notes = valid_release_notes().replacen("- Fixed the compatibility issue.", "- ", 1);
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("must not contain an empty bullet")
        );
    }

    #[test]
    fn release_notes_reject_indented_bullet_as_the_only_topic_content() {
        let notes = valid_release_notes().replacen(
            "- Fixed the compatibility issue.",
            "   - Fixed the compatibility issue.",
            1,
        );
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(error.to_string().contains("non-empty bullet"));
    }

    #[test]
    fn release_notes_reject_fenced_code_as_topic_content() {
        let notes = valid_release_notes().replacen(
            "- Fixed the compatibility issue.",
            "```text\n- Fixed the compatibility issue.\n```",
            1,
        );
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(error.to_string().contains("fenced code"));
    }

    #[test]
    fn release_notes_reject_indented_heading() {
        let notes = valid_release_notes().replacen(
            "### Application updates",
            "   ### Application updates",
            1,
        );
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(error.to_string().contains("headings must not be indented"));
    }

    #[test]
    fn release_notes_reject_release_specific_topic_heading() {
        let notes = valid_release_notes().replacen(
            "### Application updates",
            "### Package list reliability",
            1,
        );
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(error.to_string().contains("exactly and in order"));
    }

    #[test]
    fn release_notes_reject_missing_fixed_category() {
        let notes = valid_release_notes().replace(
            "### Website updates\n\n- No user-visible website changes in this release.\n\n",
            "",
        );
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(error.to_string().contains("exactly and in order"));
    }

    #[test]
    fn release_notes_reject_reordered_fixed_categories() {
        let notes = valid_release_notes().replace(
            "### Website updates\n\n- No user-visible website changes in this release.\n\n### Installation and upgrade\n\n- No installation or upgrade changes in this release.",
            "### Installation and upgrade\n\n- No installation or upgrade changes in this release.\n\n### Website updates\n\n- No user-visible website changes in this release.",
        );
        let error = validate_release_notes_format("2.1.3-beta.3", &notes).unwrap_err();

        assert!(error.to_string().contains("exactly and in order"));
    }

    #[test]
    fn release_source_versions_accept_matching_versions() {
        validate_release_source_versions("2.1.1", &["2.1.1".to_string()], "2.1.1").unwrap();
    }

    #[test]
    fn updater_key_loader_recognizes_dotenv_and_named_env_files() {
        assert_eq!(
            key_loader_format(std::path::Path::new(".env")),
            Some(KeyLoaderFormat::DotEnv)
        );
        assert_eq!(
            key_loader_format(std::path::Path::new("private-key.env")),
            Some(KeyLoaderFormat::DotEnv)
        );
        assert_eq!(
            key_loader_format(std::path::Path::new("private-key.ps1")),
            Some(KeyLoaderFormat::PowerShell)
        );
    }

    #[test]
    fn release_source_versions_reject_workspace_mismatch() {
        let error = validate_release_source_versions(
            "2.1.1",
            &["2.1.0".to_string(), "2.1.1".to_string()],
            "2.1.1",
        )
        .unwrap_err();

        assert!(error.to_string().contains("workspace package version"));
    }

    #[test]
    fn release_source_versions_reject_npm_mismatch() {
        let error =
            validate_release_source_versions("2.1.1", &["2.1.1".to_string()], "2.1.0").unwrap_err();

        assert!(error.to_string().contains("vrc-get-gui package version"));
    }

    #[test]
    fn public_updater_document_rejects_stale_signature() {
        let document = json!({
            "version": "2.1.1",
            "platforms": {
                "windows-x86_64": {
                    "url": "https://example.test/alcomd3-2.1.1-setup.exe",
                    "signature": "old-signature"
                }
            }
        });

        let error = validate_public_updater_document(
            "2.1.1",
            "https://example.test/alcomd3-2.1.1-setup.exe",
            "new-signature",
            &document,
        )
        .unwrap_err();

        assert!(error.to_string().contains("signature mismatch"));
    }

    #[test]
    fn public_updater_document_rejects_unexpected_url() {
        let document = json!({
            "version": "2.1.1",
            "platforms": {
                "windows-x86_64": {
                    "url": "https://wrong.example/alcomd3-2.1.1-setup.exe",
                    "signature": "signature"
                }
            }
        });

        let error = validate_public_updater_document(
            "2.1.1",
            "https://example.test/alcomd3-2.1.1-setup.exe",
            "signature",
            &document,
        )
        .unwrap_err();

        assert!(error.to_string().contains("URL mismatch"));
    }

    #[test]
    fn public_updater_document_rejects_stale_notes() {
        let expected = json!({
            "version": "2.1.1",
            "notes": "Current notes",
            "platforms": {
                "windows-x86_64": {
                    "url": "https://example.test/alcomd3-2.1.1-setup.exe",
                    "signature": "signature"
                }
            }
        });
        let mut actual = expected.clone();
        actual["notes"] = json!("Stale notes");

        let error = validate_public_updater_matches_expected(&expected, &actual).unwrap_err();

        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn github_release_state_rejects_published_release_when_draft_is_required() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let state = ReleaseState {
            name: ctx.release_title(),
            is_draft: false,
            is_prerelease: false,
            published_at: Some("2026-07-12T01:02:03Z".to_string()),
            target_commitish: "0123456789abcdef".to_string(),
            assets: expected_release_assets(&ctx),
        };

        let error = validate_github_release_state(&ctx, &state, Some(true), None).unwrap_err();

        assert!(error.to_string().contains("draft state mismatch"));
    }

    #[test]
    fn published_release_state_requires_published_at() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let state = ReleaseState {
            name: ctx.release_title(),
            is_draft: false,
            is_prerelease: false,
            published_at: None,
            target_commitish: "0123456789abcdef".to_string(),
            assets: expected_release_assets(&ctx),
        };

        let error = validate_github_release_state(&ctx, &state, Some(false), None).unwrap_err();

        assert!(error.to_string().contains("publishedAt"));
    }

    #[test]
    fn github_release_state_rejects_unexpected_asset() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let state = ReleaseState {
            name: ctx.release_title(),
            is_draft: true,
            is_prerelease: false,
            published_at: None,
            target_commitish: "0123456789abcdef".to_string(),
            assets: {
                let mut assets = expected_release_assets(&ctx);
                assets.push(ReleaseAsset {
                    name: "stale-build.zip".to_string(),
                });
                assets
            },
        };

        let error = validate_github_release_state(&ctx, &state, Some(true), None).unwrap_err();

        assert!(error.to_string().contains("unexpected asset"));
    }

    #[test]
    fn github_release_state_rejects_wrong_source_commit() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let state = ReleaseState {
            name: ctx.release_title(),
            is_draft: true,
            is_prerelease: false,
            published_at: None,
            target_commitish: "wrong-commit".to_string(),
            assets: expected_release_assets(&ctx),
        };

        let error =
            validate_github_release_state(&ctx, &state, Some(true), Some("0123456789abcdef"))
                .unwrap_err();

        assert!(error.to_string().contains("target commit"));
    }

    #[test]
    fn replacement_draft_may_target_an_older_source_commit() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let state = ReleaseState {
            name: ctx.release_title(),
            is_draft: true,
            is_prerelease: false,
            published_at: None,
            target_commitish: "older-source-commit".to_string(),
            assets: vec![],
        };

        validate_github_release_is_replaceable(&ctx, &state).unwrap();
    }

    #[test]
    fn replacement_refuses_published_release() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let state = ReleaseState {
            name: ctx.release_title(),
            is_draft: false,
            is_prerelease: false,
            published_at: Some("2026-07-13T00:00:00Z".to_string()),
            target_commitish: "0123456789abcdef".to_string(),
            assets: vec![],
        };

        let error = validate_github_release_is_replaceable(&ctx, &state).unwrap_err();
        assert!(error.to_string().contains("published GitHub Release"));
    }

    #[test]
    fn non_signing_commands_remove_updater_secrets() {
        let mut command = ProcessCommand::new("test");
        command
            .env(UPDATER_PRIVATE_KEY_ENV, "private")
            .env(UPDATER_PRIVATE_KEY_PASSWORD_ENV, "password");

        remove_updater_signing_env(&mut command);

        let removed = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(removed.contains(&UPDATER_PRIVATE_KEY_ENV.to_string()));
        assert!(removed.contains(&UPDATER_PRIVATE_KEY_PASSWORD_ENV.to_string()));
    }

    #[test]
    fn non_github_commands_remove_github_tokens() {
        let mut command = ProcessCommand::new("test");
        command
            .env(GH_TOKEN_ENV, "gh-token")
            .env(GITHUB_TOKEN_ENV, "github-token");

        remove_github_auth_env(&mut command);

        let removed = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(removed.contains(&GH_TOKEN_ENV.to_string()));
        assert!(removed.contains(&GITHUB_TOKEN_ENV.to_string()));
    }

    #[test]
    fn draft_automation_accepts_only_the_pinned_dispatch_context() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let source_sha = "0123456789abcdef0123456789abcdef01234567";

        validate_github_actions_context(
            &ctx,
            ReleaseAutomation::Draft,
            source_sha,
            "true",
            "workflow_dispatch",
            "refs/heads/main",
            &ctx.repo,
            source_sha,
            &format!(
                "{}/.github/workflows/release-draft.yml@refs/heads/main",
                ctx.repo
            ),
        )
        .unwrap();
    }

    #[test]
    fn updater_automation_rejects_the_wrong_workflow() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let source_sha = "0123456789abcdef0123456789abcdef01234567";

        let error = validate_github_actions_context(
            &ctx,
            ReleaseAutomation::Updater,
            source_sha,
            "true",
            "release",
            "refs/tags/v2.1.1",
            &ctx.repo,
            source_sha,
            &format!(
                "{}/.github/workflows/release-draft.yml@refs/heads/main",
                ctx.repo
            ),
        )
        .unwrap_err();

        assert!(error.to_string().contains("workflow"));
    }

    #[test]
    fn updater_automation_accepts_a_main_branch_recovery_dispatch() {
        let ctx = ReleaseContext::new("2.1.1", ReleaseChannel::Stable, None, None, None).unwrap();
        let source_sha = "0123456789abcdef0123456789abcdef01234567";
        let workflow_sha = "89abcdef0123456789abcdef0123456789abcdef";

        validate_github_actions_context(
            &ctx,
            ReleaseAutomation::Updater,
            source_sha,
            "true",
            "workflow_dispatch",
            "refs/heads/main",
            &ctx.repo,
            workflow_sha,
            &format!(
                "{}/.github/workflows/release-updater.yml@refs/heads/main",
                ctx.repo
            ),
        )
        .unwrap();
    }
}
