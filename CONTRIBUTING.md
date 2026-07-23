# Contributing

Languages: English | [日本語](CONTRIBUTING/CONTRIBUTING.ja.md) |
[简体中文](CONTRIBUTING/CONTRIBUTING.zh-CN.md)

### Project standard

ALCOMD3 is maintained as an independent project. The current repository,
documentation, release process, and user-facing behavior are the source of
truth.

External fixes can be useful, but they must be reviewed and adapted to
ALCOMD3's current architecture, especially the GUI/MCP shared operation model.

### Development scope

- GUI code lives in `vrc-get-gui/`.
- VPM package and project management code lives in `vrc-get-vpm/`.
- CLI compatibility code lives in `vrc-get/`.
- MCP bridge code lives in `alcomd3-mcp/`.
- MCP IPC protocol types live in `alcomd3-mcp-protocol/`.
- Release and packaging helpers live in `xtask/`.
- Website code lives in `website/`.

Some directory and package names still use `vrc-get` for compatibility and
historical reasons. Do not rename them unless the change is explicitly scoped
as a compatibility migration.

### Environment

Recommended tools:

- Rust stable toolchain.
- Node.js and npm.
- Windows build tools when building the Windows MSVC target.
- Inno Setup for local Windows installer work, or let `xtask` download/cache it
  when supported by the task.

Clone this repository directly:

```bash
git clone https://github.com/ALCOMD3/ALCOMD3.git
```

### Local development

Build and test Rust workspace members:

```bash
cargo check
cargo test
```

Run the desktop GUI in development mode:

```bash
cd vrc-get-gui
npm install
npm run tauri dev
```

Run the website:

```bash
cd website
npm install
npm run dev
```

### Release policy

ALCOMD3 owns its release flow. Use ALCOMD3 release automation, signing secrets,
updater metadata, and release naming.

Stable release versions use SemVer, for example `2.0.0`, `2.0.1`, and
`2.1.0`. Prerelease builds may use suffixes such as `2.1.0-beta.1`.

Windows release builds use the scripted release flow. For local artifact
validation, use:

```powershell
cargo xtask release-build --version 2.0.1 --channel stable
```

The complete release workflow is documented in `docs/RELEASE.md`. Updater key
and signature details are documented in `docs/ALCOMD3_UPDATER.md`.
Agent release procedures are documented in
`docs/skills/alcomd3-release/SKILL.md`.
Use `docs/README.md` as the documentation index when looking for maintenance,
release, MCP, format, and historical documentation.

### Contribution license

Unless explicitly marked otherwise and accepted by the maintainers,
contributions submitted to this repository are licensed under the project's
current main license, `AGPL-3.0-or-later`.

### External change intake

Treat changes from other repositories as selective intake, not as a merge
standard:

1. Identify the source commit, pull request, release note, or issue.
2. Check whether it affects security, data safety, VRChat/VPM compatibility, or
   a user-visible bug.
3. Adapt the change to ALCOMD3's architecture instead of blindly merging.
4. Verify affected Rust, GUI, MCP, and website code paths.
5. Document notable user-facing changes in `release-notes/`.

Changes touching package operations, project mutations, repository management,
operation cancellation, resource locking, or MCP visibility require extra care.
The GUI and MCP bridge should continue to share the same backend business logic
and safety checks.

### Compatibility rules

- Do not casually change the Tauri identifier, installed executable name,
  protocol names, user data paths, or `vrc-get` compatibility paths.
- Do not hardcode URLs, versions, colors, or public paths when a project config
  already owns that data.
- Keep ALCOMD3 updater metadata pointed at ALCOMD3-owned endpoints.
- Keep MCP disabled by default and local-only unless a separate design review
  explicitly changes that boundary.
- Preserve existing user data and migration paths.

### Pull request expectations

- Keep PRs focused.
- Explain user-visible behavior changes.
- Include validation results, or state clearly why validation was not run.
- Update docs and release notes when behavior, compatibility, packaging, or
  public configuration changes.
- Avoid unrelated formatting churn.

For website-specific work, also read `website/AGENTS.md`.
