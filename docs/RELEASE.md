# ALCOMD3 release workflow

Languages: English | [日本語](RELEASE/RELEASE.ja.md) |
[简体中文](RELEASE/RELEASE.zh-CN.md)

This is the release-day workflow for ALCOMD3. GitHub Actions is the default
release orchestrator. Local `release-build` runs default to unsigned,
single-platform temporary shards; `xtask` also retains an explicit,
exceptional path for building all platform shards, assembling signed release
artifacts, and manually publishing them to GitHub.

The release has three phases and two commits:

1. Source release commit: version metadata and release notes.
2. GitHub Release: ten platform-explicit assets for Windows x64, macOS Apple
   Silicon, and Linux x64.
3. Updater metadata commit: one generated updater JSON containing all three
   platforms after the public assets exist.

`Full-chain desktop smoke` builds ad-hoc signed macOS and unsigned Linux smoke
artifacts. Those artifacts are never release inputs. The formal
`Build release draft` workflow uses the required macOS ad-hoc signing
configuration in `alcomd3.config.json`; it signs the app and DMG without Apple
notarization. The three updater payloads receive separate ALCOMD3
Minisign signatures during trusted assembly. Its Windows shard also upgrades
the pinned migration baseline with the exact source-bound setup EXE before that
shard can be uploaded; unlike the Full-chain artifacts, this smoke result is a
formal release gate.

During the Windows identity-reset transition, a release must pass the installer
upgrade smoke proving that the old shared AppId is absent from HKCU and both
HKLM views, historical `ALCOM.exe`/`ALCOMD3.exe` installations are removed, the
known old Desktop and Start Menu shortcuts are removed, an existing desktop
shortcut choice is restored with the replacement targeting the new executable,
the GUI process, new shortcuts, template ProgID, and `vcc://` registration use
the configured `windowsAumid`,
the old `legacyTauriIdentifier` WebView directory is removed, the new
`windowsAppId` is the only registration, and the separate `ALCOMD3` user data is
preserved. Do not release an installer that bypasses this cleanup.
The migration baseline is resolved only from the configured repository. If
`legacyWindowsMigrationReleaseTag` is absent from a fresh repository, smoke runs
without a previous installer and never consults another repository.

### Agent execution semantics

An explicit audit request stays read-only. An explicit release request is an
end-to-end operation, not a readiness report: prepare and validate the source
release files, write complete release notes from the correct comparison base,
generate all seven localized updater summaries, commit and push the source
release, then dispatch and monitor the Draft workflow. Pause only at the manual
Draft publication gate. After the Draft is published, continue monitoring the
updater workflow, metadata commit, Cloudflare Pages deployment, and public
endpoint. The release is complete only after all of those checks pass.

### Version ownership

- Rust release version source: `[workspace.package].version` in `Cargo.toml`.
- Rust workspace members inherit it with `version.workspace = true`.
- `vrc-get-gui/package.json` and `website/package.json` are updated by
  `cargo xtask release-prepare`.
- `Cargo.lock` and `package-lock.json` are generated files.
- Updater JSON is regenerated from public Release assets by the published
  updater workflow and committed only after those assets pass verification.
- `release-notes/ALCOMD3_$Version.updater-notes.json` is the short localized
  update text shown in the in-app updater dialog.

### 1. Choose release inputs

Use exactly one channel.

| Channel | Version example | GitHub Release type | Updater JSON |
| --- | --- | --- | --- |
| Stable | `2.0.1` | normal release | `website/public/api/gui/tauri-updater.json` |
| Beta | `2.1.0-beta.1` | prerelease | `website/public/api/gui/tauri-updater-beta.json` |

The first formal multi-platform release was `2.1.2-beta.1`; `2.1.2` was the
first stable release to adopt the same contract. Its manifest is published, so
the stable catalog uses the three-platform manifest. Stable 2.1.1 remains an
immutable historical GitHub Release, and the website does not derive direct
links from its legacy assets. Beta remains a separately labeled choice, and the
website does not recommend a release channel. When the browser platform is
recognized, only the matching stable package card is visually emphasized; beta
packages are never emphasized.

Set variables in PowerShell:

```powershell
$Version = "2.0.1"
$Channel = "stable"
```

Stable versions must not contain prerelease metadata. Beta versions must contain
prerelease metadata.

Repository prerequisites:

- enable GitHub Immutable Releases;
- store `ALCOMD3_UPDATER_PRIVATE_KEY` and
  `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD` as repository Actions Secrets;
- keep automatic Cloudflare Pages production deployment enabled for `main`.

macOS releases support ad-hoc signing only and require no Apple account or Apple
Secrets. The signing command exposes no certificate identity or notarization
options. Platform-specific build, signing, installation, and update mechanisms
are technical contracts only. Release notes, updater notes, and the website use
one policy for every published platform: they do not add platform-only
disclosures, warnings, instructions, or help links solely because of those
mechanisms. Name a platform only for a user-visible change relevant to that
release.

Every updater payload and browser download declares an `updateMode` in
`alcomd3.config.json`: self-updating assets use `self-updater`, while the Linux
DEB uses `no-self-updater`. The build command consumes that value, and the
source-bound shard and combined manifest retain it with each asset.

Before dispatching the Draft workflow, verify Immutable Releases with an
administrator-authenticated `gh api` preflight. The job-scoped `GITHUB_TOKEN`
lacks the required repository Administration read permission, so the workflow
deliberately does not repeat this read; published updater metadata is still
gated by the immutable Release attestation.

A protected `release-signing` Environment is optional rather than a current
prerequisite. If the project later needs a separate approval boundary for
multiple maintainers, move the same two Secrets to that Environment, add the
job environment binding, and configure required reviewers at the same time.
Until then, tightly control who can change `main` and Actions workflows because
repository Secrets do not add a separate approval boundary.

### 2. Start clean

```powershell
git checkout main
git pull --ff-only
git status --short
gh auth status --hostname github.com
```

Expected result:

- `git status --short` prints nothing.
- GitHub CLI is authenticated for `ALCOMD3/ALCOMD3`.

Stop if the worktree is dirty.

### 3. Prepare source release files

```powershell
cargo xtask release-prepare --version $Version --channel $Channel
```

This command:

- updates `Cargo.toml` workspace version;
- refreshes `Cargo.lock` workspace package versions;
- updates GUI and website npm versions without creating tags;
- refreshes npm lockfiles;
- creates `release-notes/ALCOMD3_$Version.md` if missing;
- prints the resulting `git status --short`.

Now edit the release notes file and remove all placeholder text.

Release notes must use the correct comparison base:

- Stable releases compare against the previous stable release only.
- Beta releases compare against the immediately previous release, whether stable
  or beta.

Release notes also use one canonical localized structure. The title is exactly
`# ALCOMD3 v$Version`, followed by `## English`, `## 日本語`, and `## 中文` in
that order. Each locale starts with one summary paragraph and then retains exactly
four level-3 categories in this order: application updates, website updates,
installation and upgrade, and compatibility and security. Their localized titles
must be `Application updates` / `アプリの更新` / `应用更新`, `Website updates` /
`Web サイトの更新` / `网站更新`, `Installation and upgrade` /
`インストールとアップグレード` / `安装与升级`, and
`Compatibility and security` / `互換性とセキュリティ` / `兼容性与安全`.
Do not omit, reorder, rename, or add release-specific level-3 headings. Every
category must contain a non-empty bullet list; when a category has no user-visible
change, retain it and add a localized no-change statement. Level-4 headings,
fenced code blocks, indented ATX headings, and indented top-level bullets are not
permitted. Do not fill the fixed structure with routine platform disclosures.
`release-validate` enforces the exact headings and structure; release review must
confirm that localized bullets also have the same meaning and order.
Published notes through `2.1.3-beta.2` retain their historical headings and must
not be rewritten; the fixed four-category contract applies to later releases.

Also create or update `release-notes/ALCOMD3_$Version.updater-notes.json`.
This file is a short localized summary for the in-app updater dialog, not the
full GitHub Release notes. It must be a JSON object whose keys are limited to
`en`, `de`, `fr`, `ja`, `ko`, `zh_hans`, and `zh_hant`; values must be non-empty
strings. A normal release populates all seven keys. Missing languages still fall
back to the generated `notes` field for compatibility and recovery, but that
fallback is not the normal release-preparation outcome.

Commit and push the source release commit:

```powershell
git add Cargo.toml Cargo.lock
git add vrc-get-gui/package.json vrc-get-gui/package-lock.json
git add website/package.json website/package-lock.json
git add "release-notes/ALCOMD3_$Version.md"
git add "release-notes/ALCOMD3_$Version.updater-notes.json"
git status --short
git commit -m "release: prepare ALCOMD3 $Version"
git push origin main
```

This commit is the source state the GitHub Release tag should point to. It must
not include generated installer files, files under `target/`, files under
`artifacts/`, or updater JSON.

### 4. Run the Draft build workflow

From GitHub Actions, manually run **Build release draft**, or use:

```powershell
gh workflow run release-draft.yml --repo ALCOMD3/ALCOMD3 `
    -f version=$Version `
    -f channel=$Channel `
    -f replace_existing_draft=false
```

Use `replace_existing_draft=true` only when the same version already has a
Draft whose assets should be replaced. The workflow refuses to replace a
published Release or a Draft for the wrong channel. If the prepared source
commit changed after the Draft was created, for example to correct release
notes, explicit replacement retargets the Draft to the dispatched source commit
and replaces all ten assets built from that commit.

The workflow checks out the immutable `github.sha` captured when the workflow
was dispatched, records that source commit, validates the prepared source, and
runs `release-preflight` before building. Initial creation requires that the tag
does not exist; explicit replacement requires an existing compatible Draft with
no unexpected assets. API and authentication failures are not treated as a
missing Release.

The workflow DAG is `preflight` -> three platform build shards -> trusted
assembly -> Draft. Windows x64, macOS arm64, and Linux x64 build in separate
native runners from the same pinned source commit. Each `release-build
--platform ... --github-actions-release` invocation emits a platform shard that
does not yet have its updater Minisign signature, plus a source-bound shard
manifest. Before upload, the Windows shard looks for
`legacyWindowsMigrationReleaseTag` only in the configured repository. If present,
it verifies the setup ZIP, installs the pinned stable baseline, and upgrades it;
if absent, it runs the current installer smoke without a previous installer
with the setup EXE copied into `artifacts/release/v$Version/`. Baseline
validation checks only the historical contract; current AppId, AUMID, file
associations, shortcuts, migration cleanup, launch, and uninstall assertions run
after the upgrade. The macOS shard binds the required ad-hoc signing state into
its manifest. It signs the nested executables, application, and DMG with
identity `-`, omits secure timestamps and notarization, and verifies that each
final signature reports `Signature=adhoc` before upload.

After all three shards succeed, `release-assemble` verifies their source SHA,
allowlist, and digests. It then decrypts the updater key once, signs the Windows
installer, macOS app updater archive, and Linux AppImage updater archive, verifies
all three Minisign signatures, atomically builds the combined release manifest,
and only then allows `release-publish` to create the Draft. npm download caches
are keyed by lockfiles; Rust build outputs, signing material, and release assets
are not cached. The Rust toolchain and release asset patterns come from
`alcomd3.config.json`. Updater signing material comes from:

- `ALCOMD3_UPDATER_PRIVATE_KEY`
- `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`

The pinned Inno Setup installer is SHA-256 verified against its GitHub Release
asset digest before it is executed on the runner.

The workflow never publishes the Draft. Short-lived shard artifacts contain
only the release assets and source-bound shard manifests required by the
assembly job. Checkout credentials are not persisted, and signing Secrets are
exposed only to their required jobs or steps. After a successful Draft
creation, the job summary records the Release URL, source and target commits,
Draft/prerelease state, and all asset digests.

### 5. Inspect and publish the Draft

Before publishing, confirm:

- the tag is `v$Version` and points to the source release commit built by the
  workflow;
- the title is `Version $Version`;
- stable is a normal Release and beta is a prerelease;
- release notes are correct;
- exactly these ten assets exist:
    - `ALCOMD3_$Version_windows_x86_64_setup.exe`
    - `ALCOMD3_$Version_windows_x86_64_setup.exe.sig`
    - `ALCOMD3_$Version_windows_x86_64_setup.exe.zip`
    - `ALCOMD3_$Version_macos_aarch64.dmg`
    - `ALCOMD3_$Version_macos_aarch64.app.tar.gz`
    - `ALCOMD3_$Version_macos_aarch64.app.tar.gz.sig`
    - `ALCOMD3_$Version_linux_x86_64.AppImage`
    - `ALCOMD3_$Version_linux_x86_64.AppImage.tar.gz`
    - `ALCOMD3_$Version_linux_x86_64.AppImage.tar.gz.sig`
    - `ALCOMD3_$Version_linux_amd64.deb`
- no additional uploaded assets are present;

The platform and architecture are intentionally explicit in every new public
filename. Stable 2.1.1 remains an immutable historical GitHub Release; its
legacy Windows asset names are not part of the new catalog and are not
translated into direct website download links or compatibility aliases.

Publish the Draft manually in the GitHub UI. This is the release gate; the
default build workflow does not bypass it.

### 6. Let the published updater workflow finish

Publishing the Draft triggers **Publish updater metadata**. On a fresh runner,
the workflow:

- derives the version and stable/beta channel from the published Release;
- verifies the immutable Release attestation, downloads all ten public assets,
  and verifies every downloaded asset against its attestation;
- requires the Release target, tag commit, and root/GUI/Website versions to
  match exactly;
- verifies the three updater payloads and their Minisign signatures, each bound
  to its exact filename and authenticated `release` purpose;
- reads the localized sidecar from the Release tag, then atomically regenerates
  only the selected channel's updater JSON with Windows x64, macOS arm64, and
  Linux x64 entries, using the Release `publishedAt` as its fixed `pub_date`;
- verifies every generated platform entry, exact URL, signature filename,
  signature, and embedded public key before replacing the metadata file;
- rejects updater version rollback and permits same-version retries only when
  they regenerate byte-identical metadata;
- runs the website checks and build; the website derives the target channel's
  three-platform downloads from its manifest plus `alcomd3.config.json`, leaves
  the other channel unchanged, and never synthesizes links for legacy assets;
- commits that updater JSON and pushes `main` only after the website passes;
- waits for the public updater endpoint to expose the same version, three exact
  platform URLs, and signatures.

After the public endpoint passes, the job summary records the version, channel,
Release and source commit, whether a metadata commit was created, the final
`main` commit, and the verified endpoint.

The updater workflow does not receive the private signing key. Checkout does
not persist credentials; `GH_TOKEN` is step-scoped and removed from non-GitHub
cargo, npm, and git child processes. The push to `main` triggers the connected
Cloudflare Pages production deployment. Keep automatic production-branch
deployment enabled for `main`.

### 7. Confirm completion and handle retries

The release is complete only when **Publish updater metadata** and the public
endpoint check both pass. If Cloudflare Pages is still deploying, the workflow
retries the endpoint check for a bounded period. A timeout does not roll back
the already verified metadata commit; inspect the Pages deployment, fix the
deployment issue, and rerun the workflow. If the JSON is already current, the
same Release regenerates byte-identical JSON, so the rerun skips commit/push
and continues with the endpoint check.

### Local builds and exceptional manual publication

The local machine is not the planned release orchestrator. A normal local
`release-build` builds exactly one platform selected with `--platform` and emits
an **unsigned** shard under `artifacts/local-test/v$Version/`. It does not
Minisign the updater payload and, on macOS, does not perform certificate-based
signing, notarization, or stapling:

```powershell
cargo xtask release-build --version $Version --channel $Channel --platform windows-x86_64
```

The other platform keys are `darwin-aarch64` and `linux-x86_64`. These shards
are useful for package inspection but cannot be consumed by the official asset
publisher as-is.

If a maintainer explicitly needs to publish from a local machine, start from a
clean `main` that exactly matches `origin/main`, then build release-purpose
artifacts:

```powershell
cargo xtask release-validate --version $Version --channel $Channel
cargo xtask release-build --version $Version --channel $Channel --platform windows-x86_64 --release-artifacts
cargo xtask release-build --version $Version --channel $Channel --platform darwin-aarch64 --release-artifacts
cargo xtask release-build --version $Version --channel $Channel --platform linux-x86_64 --release-artifacts
$SourceSha = git rev-parse HEAD
cargo xtask release-assemble --version $Version --channel $Channel --source-sha $SourceSha
cargo xtask release-publish --version $Version --channel $Channel
```

Each explicit build emits a platform shard without its updater Minisign
signature. `release-assemble` is mandatory: it verifies the three source-bound
shard manifests, signs and verifies all three updater payloads with the
`release` purpose, checks the exact ten-file allowlist, and writes the combined
ignored `artifacts/release-state/v$Version.json` manifest. `release-publish`
rechecks that manifest before upload. A local macOS release shard must be
produced on macOS with the same required ad-hoc signing path as Actions, so its
app and DMG match the state recorded in the shard manifest before assembly. Add
`--replace-assets` only for an existing compatible Draft. Explicit replacement
retargets that Draft to the verified build source before post-upload validation.
Local publication still requires explicit authorization; add `--publish` only
when the user has authorized making the Release public. Publishing either an
Actions or local Draft triggers the same updater workflow. Updater metadata
publication is Actions-only so that attestation, source binding, monotonic
version, serialized queue, and public endpoint checks cannot be skipped.
During the Windows identity migration, exceptional local publication also
cannot bypass the GitHub-hosted formal installer upgrade smoke. Use the Draft
workflow if the exact local setup EXE cannot be proven by that gate.

Never commit `target/`, `artifacts/`, `.env`, or updater JSON generated before
the matching Release assets are public.

### Failure rules

Stop the release if:

- release notes still contain placeholder text or violate the canonical localized structure;
- updater notes sidecar is missing when expected or has invalid JSON, unsupported
  language keys, or empty values;
- validation fails;
- the source-bound Windows release installer upgrade smoke fails, is cancelled,
  does not run, or tests an installer other than the setup EXE in the Windows
  release shard;
- signing variables or signing key loader are missing;
- the macOS shard is not bound to the required ad-hoc signing configuration;
- ad-hoc signing, strict verification, or the `Signature=adhoc` check fails for
  the app, nested executables, updater archive contents, or DMG;
- any artifact is missing;
- updater JSON verification fails;
- release notes use the wrong comparison base;
- GitHub Release title is not `Version $Version`;
- GitHub Release assets are missing or misnamed;
- stable/beta flags are wrong;
- Immutable Releases are disabled or a Release/asset attestation fails;
- the Release target SHA, tag commit, or source versions disagree;
- a release signature is marked `local-test` or is bound to another filename;
- updater metadata would roll back the selected channel;
- an existing Release is published when the Draft workflow was asked to replace it;
- initial Draft creation finds an existing Release, or Draft replacement finds
  no compatible Draft or an unexpected asset;
- the updater signing key cannot be decrypted or does not match the embedded
  public key;
- Cloudflare Pages automatic production deployment for `main` is disabled;
- website updater JSON would be deployed before GitHub Release assets are public.
