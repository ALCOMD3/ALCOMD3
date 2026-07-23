use crate::utils;
use crate::utils::command::{CommandExt, create_command};
use crate::utils::{build_dir, build_target};
use anyhow::{Context, Result, bail};
use itertools::Itertools;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

const GUI_BINARY_NAME: &str = "ALCOMD3";

/// Builds the ALCOMD3 binary using `cargo build`.
///
/// When targeting `universal-apple-darwin`, both `aarch64-apple-darwin` and
/// `x86_64-apple-darwin` are compiled and then combined with `lipo`.
#[derive(clap::Parser)]
pub(super) struct Command {
    /// Target triple (e.g. `universal-apple-darwin`, `x86_64-unknown-linux-gnu`).
    ///
    /// Defaults to the host triple.
    #[arg(long)]
    target: Option<String>,

    #[command(flatten)]
    profile: utils::BuildProfile,

    /// Enable verbose cargo output.
    #[arg(long)]
    verbose: bool,

    #[command(flatten)]
    config: BuildConfig,
}

// feature flags for ALCOMD3

// see https://jwodder.github.io/kbits/posts/clap-bool-negate/ for negative option implementation
#[derive(clap::Args)]
struct BuildConfig {
    /// Enables the embedded WebDriver endpoint for isolated desktop E2E tests.
    /// This option is restricted to the dev profile.
    #[arg(long)]
    desktop_e2e_webdriver: bool,

    #[arg(long = "no-self-updater", action = clap::ArgAction::SetFalse, hide = true)]
    updater: bool,
    /// Enables self updater of ALCOMD3. Enabled by default. can be disabled with --no-self-updater.
    ///
    /// With this feature enabled, ALCOMD3 will try to update itself when newer version is released.
    /// When disabled with --no-self-updater, ALCOMD3 still checks for updates but just let user know
    /// newer version is published with optional instruction for your package manager specified with
    /// --updater-instruction-message build option.
    #[arg(long = "self-updater", overrides_with = "updater")]
    _inv_updater: bool,

    /// The message shown when a newer version of ALCOMD3 is published.
    ///
    /// This message will be displayed after the short message indicating that a new version of ALCOMD3 is available.
    /// As of writing, the short message is "A new version of ALCOMD3 is available."
    /// It is recommended to include instructions on how to update ALCOMD3 and how to ask the
    /// package maintainer to update the package.
    ///
    /// You can specify the locale of the message in the form "ja=brewで更新してください".
    /// If ALCOMD3 supports the locale, it will show the corresponding message when the user selects that locale.
    /// If no locale is specified, the message is assumed to be in English.
    /// If messages for other locales are specified, an English message must also be provided.
    #[arg(long, requires = "updater")]
    updater_instruction_message: Vec<String>,

    /// Enables devtools for ALCOMD3 frontend.
    ///
    /// This allows debugging frontend with distribution build of ALCOMD3, but this is only for debugging
    /// purposes. Please do not enable this feature for builds end-user uses.
    #[arg(long = "devtools", overrides_with = "_inv_devtools")]
    devtools: bool,
    #[arg(long = "no-devtools", hide = true)]
    _inv_devtools: bool,
}

impl crate::Command for Command {
    fn run(self) -> Result<i32> {
        let metadata = crate::utils::cargo::cargo_metadata();
        let workspace_root = metadata.workspace_root.as_std_path();

        let target_triple = build_target(self.target.as_deref());

        if self.config.desktop_e2e_webdriver && self.profile.name() != "dev" {
            bail!("--desktop-e2e-webdriver may only be used with the dev profile");
        }

        build_web(workspace_root)?;

        if target_triple == "universal-apple-darwin" {
            build_universal_macos(
                workspace_root,
                self.profile.name(),
                &self.config,
                self.verbose,
            )?;
        } else {
            build_cargo(
                workspace_root,
                self.target.as_deref(),
                self.profile.name(),
                &self.config,
                self.verbose,
            )?;
        }

        Ok(0)
    }
}

/// Run `npm run build` to build web part
fn build_web(workspace_root: &Path) -> Result<()> {
    create_command("npm")
        .arg("run")
        .arg("build")
        .current_dir(workspace_root.join("vrc-get-gui"))
        .run_checked("frontend of vrc-get-gui")
}

/// Run cargo builds for a single target triple.
fn build_cargo(
    workspace_root: &Path,
    target_triple: Option<&str>,
    profile: &str,
    config: &BuildConfig,
    verbose: bool,
) -> Result<()> {
    build_gui_cargo(workspace_root, target_triple, profile, config, verbose)?;
    build_mcp_cargo(workspace_root, target_triple, profile, verbose)
}

/// Run `cargo build -p vrc-get-gui` for a single target triple.
fn build_gui_cargo(
    workspace_root: &Path,
    target_triple: Option<&str>,
    profile: &str,
    config: &BuildConfig,
    verbose: bool,
) -> Result<()> {
    let mut cmd = ProcessCommand::new("cargo");
    cmd.current_dir(workspace_root)
        .arg("build")
        .arg("-p")
        .arg("vrc-get-gui")
        .arg("--profile")
        .arg(profile);

    let mut features = vec!["custom-protocol"];

    if !config.updater {
        features.push("no-self-updater");

        let mut locales = HashMap::new();

        for message in &config.updater_instruction_message {
            let (locale, message) = message.split_once('=').unwrap_or(("en", message));

            if message.trim().is_empty() {
                eprintln!("warning: message for {locale} for is blank. ignoring");
                continue;
            }

            fn normalize_locale(locale: &str) -> String {
                locale.to_ascii_lowercase().replace("_", "-")
            }
            match locales.entry(normalize_locale(locale)) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(message);
                }
                std::collections::hash_map::Entry::Occupied(_) => {
                    bail!("--updater-instruction-message specified for {locale} locale twice")
                }
            }
        }

        if locales.is_empty() {
            eprintln!(
                "warning: updater is disabled but no --updater-instruction-message flag was specified"
            );
        } else {
            if !locales.contains_key("en") {
                bail!("--updater-instruction-message specified some locale but not for en");
            }

            cmd.env(
                "ALCOMD3_UPDATER_DISABLED_MESSAGE",
                serde_json::to_string(&locales)?,
            );
        }
    }

    if config.devtools {
        features.push("devtools");
    }

    if config.desktop_e2e_webdriver {
        features.push("desktop-e2e-webdriver");
    }

    cmd.arg("--features").arg(features.iter().join(","));

    if let Some(target) = target_triple {
        cmd.arg("--target").arg(target);
    }

    if verbose {
        cmd.arg("--verbose");
    }

    cmd.run_checked(&format!(
        "building vrc-get-gui for {}",
        target_triple.unwrap_or("native target")
    ))
}

/// Run `cargo build -p alcomd3-mcp` for a single target triple.
fn build_mcp_cargo(
    workspace_root: &Path,
    target_triple: Option<&str>,
    profile: &str,
    verbose: bool,
) -> Result<()> {
    let mut cmd = ProcessCommand::new("cargo");
    cmd.current_dir(workspace_root)
        .arg("build")
        .arg("-p")
        .arg("alcomd3-mcp")
        .arg("--profile")
        .arg(profile);

    if let Some(target) = target_triple {
        cmd.arg("--target").arg(target);
    }

    if verbose {
        cmd.arg("--verbose");
    }

    cmd.run_checked(&format!(
        "building alcomd3-mcp for {}",
        target_triple.unwrap_or("native target")
    ))
}

/// Build a universal macOS binary by compiling for both x86_64 and aarch64 and
/// merging the results with `lipo`.
fn build_universal_macos(
    workspace_root: &Path,
    profile: &str,
    config: &BuildConfig,
    verbose: bool,
) -> Result<()> {
    build_cargo(
        workspace_root,
        Some("x86_64-apple-darwin"),
        profile,
        config,
        verbose,
    )?;
    build_cargo(
        workspace_root,
        Some("aarch64-apple-darwin"),
        profile,
        config,
        verbose,
    )?;

    // Combine the two single-arch binaries into one fat binary.
    let x86_bin = build_dir("x86_64-apple-darwin", profile).join(GUI_BINARY_NAME);
    let arm_bin = build_dir("aarch64-apple-darwin", profile).join(GUI_BINARY_NAME);
    let x86_mcp_bin = build_dir("x86_64-apple-darwin", profile).join("alcomd3-mcp");
    let arm_mcp_bin = build_dir("aarch64-apple-darwin", profile).join("alcomd3-mcp");

    let out_dir = build_dir("universal-apple-darwin", profile);
    fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    let out_bin = out_dir.join(GUI_BINARY_NAME);
    let out_mcp_bin = out_dir.join("alcomd3-mcp");

    lipo_create(&[&x86_bin, &arm_bin], &out_bin)?;
    lipo_create(&[&x86_mcp_bin, &arm_mcp_bin], &out_mcp_bin)?;

    println!("created universal binary: {}", out_bin.display());
    println!("created universal binary: {}", out_mcp_bin.display());
    Ok(())
}

/// Create a universal binary from a list of single-arch binaries using `lipo`.
fn lipo_create(inputs: &[&Path], output: &Path) -> Result<()> {
    let mut cmd = ProcessCommand::new("lipo");
    cmd.arg("-create").arg("-output").arg(output);
    for input in inputs {
        cmd.arg(input);
    }
    cmd.run_checked("lipo: creating universal binary")
}
