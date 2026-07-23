use crate::release_common::{
    CmdRunner, ReleaseAutomation, ReleaseChannel, ReleaseContext, ensure_github_actions_context, gh,
};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::process::Command as ProcessCommand;

const RELEASE_QUERY: &str = r#"
query ReleasePreflight($owner: String!, $name: String!, $tag: String!) {
  repository(owner: $owner, name: $name) {
    release(tagName: $tag) {
      isDraft
      isPrerelease
      releaseAssets(first: 100) {
        totalCount
        nodes {
          name
        }
      }
    }
  }
}
"#;

/// Check whether the requested GitHub Release Draft operation is safe before building assets.
#[derive(clap::Parser)]
pub struct Command {
    /// Release version, for example 2.0.1 or 2.1.0-beta.1.
    #[arg(long)]
    version: String,

    /// Release channel.
    #[arg(long, value_enum, default_value_t = ReleaseChannel::Stable)]
    channel: ReleaseChannel,

    /// Exact source commit that the Draft workflow checked out.
    #[arg(long)]
    source_sha: String,

    /// Require an existing compatible Draft whose metadata and assets may be replaced.
    #[arg(long)]
    replace_existing_draft: bool,

    /// Print the planned checks without querying GitHub.
    #[arg(long)]
    dry_run: bool,
}

impl crate::Command for Command {
    fn run(self) -> Result<i32> {
        let ctx = ReleaseContext::new(self.version, self.channel, None, None, None)?;
        let runner = CmdRunner::new(self.dry_run);

        ensure_github_actions_context(
            &ctx,
            ReleaseAutomation::Draft,
            &self.source_sha,
            self.dry_run,
        )?;

        if self.dry_run {
            println!(
                "check GitHub Release {} for {} mode",
                ctx.tag,
                if self.replace_existing_draft {
                    "Draft replacement"
                } else {
                    "initial Draft creation"
                }
            );
            return Ok(0);
        }

        let release = query_release(&runner, &ctx)?;
        validate_preflight_state(&ctx, release.as_ref(), self.replace_existing_draft)?;
        println!(
            "release preflight passed: {} ({}, replace_existing_draft={})",
            ctx.version, ctx.channel, self.replace_existing_draft
        );
        Ok(0)
    }
}

fn query_release(runner: &CmdRunner, ctx: &ReleaseContext) -> Result<Option<ReleaseState>> {
    let output = runner.capture(release_query_command(ctx)?, "querying GitHub Release state")?;
    parse_release_query(&output)
}

fn release_query_command(ctx: &ReleaseContext) -> Result<ProcessCommand> {
    let (owner, name) = ctx
        .repo
        .split_once('/')
        .context("release repository must be in OWNER/REPO form")?;
    let mut cmd = gh();
    cmd.arg("api")
        .arg("graphql")
        .arg("-f")
        .arg(format!("query={RELEASE_QUERY}"))
        .arg("-F")
        .arg(format!("owner={owner}"))
        .arg("-F")
        .arg(format!("name={name}"))
        .arg("-F")
        .arg(format!("tag={}", ctx.tag));
    Ok(cmd)
}

fn parse_release_query(source: &str) -> Result<Option<ReleaseState>> {
    let response: ReleaseQueryResponse =
        serde_json::from_str(source).context("parsing GitHub Release preflight response")?;
    if !response.errors.is_empty() {
        let messages = response
            .errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        bail!("GitHub Release preflight query failed: {messages}");
    }

    let data = response
        .data
        .context("GitHub Release preflight response has no data")?;
    let repository = data
        .repository
        .context("GitHub Release preflight could not access the configured repository")?;
    Ok(repository.release)
}

fn validate_preflight_state(
    ctx: &ReleaseContext,
    release: Option<&ReleaseState>,
    replace_existing_draft: bool,
) -> Result<()> {
    if !replace_existing_draft {
        if let Some(release) = release {
            bail!(
                "GitHub Release {} already exists as {}; rerun only with --replace-existing-draft for a compatible Draft",
                ctx.tag,
                if release.is_draft {
                    "a Draft"
                } else {
                    "a published Release"
                }
            );
        }
        return Ok(());
    }

    let release = release
        .context("no existing GitHub Release Draft was found for --replace-existing-draft")?;
    if !release.is_draft {
        bail!("refusing to replace assets on a published GitHub Release");
    }
    if release.is_prerelease != ctx.channel.is_prerelease() {
        bail!(
            "refusing to replace Draft assets with a mismatched prerelease flag for channel {}",
            ctx.channel
        );
    }

    if release.release_assets.total_count != release.release_assets.nodes.len() {
        bail!(
            "GitHub Release asset query was incomplete: expected {}, got {}",
            release.release_assets.total_count,
            release.release_assets.nodes.len()
        );
    }

    let expected_assets = ctx.expected_public_asset_names();
    for asset in &release.release_assets.nodes {
        if !expected_assets
            .iter()
            .any(|expected| asset.name == *expected)
        {
            bail!(
                "existing GitHub Release Draft has an unexpected asset that replacement cannot remove: {}",
                asset.name
            );
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct ReleaseQueryResponse {
    data: Option<ReleaseQueryData>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

#[derive(Debug, Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseQueryData {
    repository: Option<ReleaseRepository>,
}

#[derive(Debug, Deserialize)]
struct ReleaseRepository {
    release: Option<ReleaseState>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseState {
    is_draft: bool,
    is_prerelease: bool,
    release_assets: ReleaseAssets,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseAssets {
    total_count: usize,
    nodes: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::{
        ReleaseAsset, ReleaseAssets, ReleaseState, parse_release_query, validate_preflight_state,
    };
    use crate::release_common::{ReleaseChannel, ReleaseContext};

    fn ctx(channel: ReleaseChannel) -> ReleaseContext {
        let version = match channel {
            ReleaseChannel::Stable => "9.0.0",
            ReleaseChannel::Beta => "9.0.0-beta.1",
        };
        ReleaseContext::new(version, channel, None, None, None).unwrap()
    }

    fn release(ctx: &ReleaseContext, is_draft: bool, is_prerelease: bool) -> ReleaseState {
        let assets = ctx
            .expected_public_asset_names()
            .into_iter()
            .map(|name| ReleaseAsset { name })
            .collect::<Vec<_>>();
        ReleaseState {
            is_draft,
            is_prerelease,
            release_assets: ReleaseAssets {
                total_count: assets.len(),
                nodes: assets,
            },
        }
    }

    #[test]
    fn initial_draft_requires_no_existing_release() {
        let ctx = ctx(ReleaseChannel::Stable);
        validate_preflight_state(&ctx, None, false).unwrap();

        let release = release(&ctx, true, false);
        let error = validate_preflight_state(&ctx, Some(&release), false).unwrap_err();
        assert!(error.to_string().contains("already exists"));
    }

    #[test]
    fn replacement_accepts_compatible_draft() {
        let ctx = ctx(ReleaseChannel::Stable);
        let release = release(&ctx, true, false);
        validate_preflight_state(&ctx, Some(&release), true).unwrap();
    }

    #[test]
    fn replacement_requires_an_existing_draft() {
        let ctx = ctx(ReleaseChannel::Stable);
        let error = validate_preflight_state(&ctx, None, true).unwrap_err();
        assert!(error.to_string().contains("no existing"));

        let release = release(&ctx, false, false);
        let error = validate_preflight_state(&ctx, Some(&release), true).unwrap_err();
        assert!(error.to_string().contains("published"));
    }

    #[test]
    fn replacement_rejects_channel_mismatch() {
        let ctx = ctx(ReleaseChannel::Beta);
        let release = release(&ctx, true, false);
        let error = validate_preflight_state(&ctx, Some(&release), true).unwrap_err();
        assert!(error.to_string().contains("prerelease flag"));
    }

    #[test]
    fn replacement_rejects_unexpected_assets() {
        let ctx = ctx(ReleaseChannel::Stable);
        let mut release = release(&ctx, true, false);
        release.release_assets.nodes.push(ReleaseAsset {
            name: "stale-build.zip".to_string(),
        });
        release.release_assets.total_count += 1;

        let error = validate_preflight_state(&ctx, Some(&release), true).unwrap_err();
        assert!(error.to_string().contains("unexpected asset"));
    }

    #[test]
    fn graphql_errors_are_not_treated_as_a_missing_release() {
        let error = parse_release_query(
            r#"{"data":null,"errors":[{"message":"Resource not accessible"}]}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("Resource not accessible"));
    }

    #[test]
    fn graphql_null_release_is_the_only_missing_release_state() {
        let release =
            parse_release_query(r#"{"data":{"repository":{"release":null}},"errors":[]}"#).unwrap();
        assert!(release.is_none());
    }
}
