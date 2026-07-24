# ALCOMD3 maintenance notes

Languages: English | [日本語](MAINTENANCE/MAINTENANCE.ja.md) |
[简体中文](MAINTENANCE/MAINTENANCE.zh-CN.md)

These notes document ALCOMD3's current project behavior, compatibility
boundaries, and maintenance decisions. Use them when changing ALCOMD3 code or
when selectively importing fixes from any external source.

### Branding

ALCOMD3 uses its own branding while keeping compatibility points required by
existing installations.

- Product and visible app name: `ALCOMD3`.
- Window title: `ALCOMD3`.
- Windows installer display name: `ALCOMD3`.
- Windows shortcuts: `ALCOMD3`.
- Windows uses the stable explicit AUMID `CQMHV.ALCOMD3` for the GUI process,
  installer-created shortcuts, `.alcomtemplate` ProgID, and `vcc://` protocol
  registration. It has no version segment so upgrades retain one Shell identity.
- Windows setup output name: `ALCOMD3_{version}_windows_x86_64_setup.exe`.
- Installed main executable: `ALCOMD3.exe`.
- macOS application bundle: `ALCOMD3.app`.
- Linux binary, desktop file, icon name, and package metadata use `alcomd3`.
- Public asset names include the platform and architecture. The macOS download
  is `ALCOMD3_{version}_macos_aarch64.dmg`; Linux downloads are
  `ALCOMD3_{version}_linux_x86_64.AppImage` and
  `ALCOMD3_{version}_linux_amd64.deb`.
- The Windows installer uses a new ALCOMD3-only AppId. During the transition,
  it unconditionally uninstalls Inno Setup installations under the old shared
  AppId, whether their main executable is `ALCOM.exe` or `ALCOMD3.exe`. It
  registers the new AppId only after the old AppId is absent from HKCU and both
  HKLM registry views, preventing two ALCOMD3 installations from coexisting.
- This identity migration removes the old AppId's installation records, known
  program files, and known `ALCOM`/`ALCOMD3` shortcuts from the Desktop and
  Start Menu. If the previous installation had a desktop shortcut, the new
  installer selects the ALCOMD3 desktop shortcut by default while still
  respecting an interactive user's current choice. It does not clean legacy
  ALCOM NSIS records, unrelated shortcuts, or ALCOMD3 user data.
- The `vcc://` URL shortcut association defaults to enabled when no explicit
  user choice exists.
- Repository-add deep links must require an in-app confirmation before any
  repository metadata download is started.

Important files:

- `alcomd3.config.json`
- `vrc-get-gui/Tauri.toml`
- `vrc-get-gui/src/commands/start.rs`
- `vrc-get-gui/src/commands/util.rs`
- `vrc-get-gui/src/commands/environment/legacy_import.rs`
- `vrc-get-gui/src/config.rs`
- `vrc-get-gui/src/utils.rs`
- `vrc-get-gui/bundle/windows-setup.iss`
- `vrc-get-vpm/src/io/tokio.rs`
- `alcomd3-mcp-protocol/src/lib.rs`
- `xtask/src/bundle_alcom.rs`
- `xtask/src/bundle_alcom/setup_exe.rs`
- `xtask/tests/alcomd3_identity.rs`

### Shared configuration

`alcomd3.config.json` is the shared source for ALCOMD3 product identity and
release metadata. It manages product, package, GUI binary, MCP binary,
publisher, homepage, GitHub repository, Windows AUMID, current and transitional
legacy Windows AppIds, the pinned migration-baseline Release tag, updater manifest paths,
the three-platform release catalog (targets, bundle types, updater payloads,
download assets, and filename patterns), `.alcomtemplate` association metadata,
descriptions, and copyright text.

Some values still appear in external tool templates because those tools consume
their own file formats directly:

- `vrc-get-gui/Tauri.toml`
- `vrc-get-gui/bundle/windows-setup.iss`
- `vrc-get-gui/bundle/alcomd3.desktop`
- `vrc-get-gui/bundle/deb-control`

Keep those files in place. `cargo test -p xtask alcomd3_identity` checks them
against `alcomd3.config.json` so template drift is caught without adding a
generation step. `repositories.txt` is example/import data for repository list
format workflows, not runtime shared configuration.

### Data directories and external app import

ALCOMD3 owns its runtime data by default.

- The default local data directory is the platform local data root plus
  `ALCOMD3` (for example, `%LOCALAPPDATA%\ALCOMD3` on Windows).
- Default project and backup folders are under the user's Documents folder:
  `ALCOMD3/Projects` and `ALCOMD3/Backups`.
- ALCOMD3 must not automatically migrate, move, or import VCC or legacy ALCOM
  data on ordinary startup.
- Installing ALCOMD3 or updating from 2.0.0 or earlier to 2.1.0 or later should
  behave as a fresh ALCOMD3 data-root install until the user explicitly imports
  data from an external VCC or legacy ALCOM installation.
- VCC or legacy ALCOM data may be read only when the user explicitly imports it
  during first setup or later from settings.
- External app import copies VPM settings, LiteDB project/Unity data,
  repository caches, vrc-get settings, and template data into the ALCOMD3 data
  directory.
- Repository index caches live under `Repos/`. Downloaded package zip caches
  live under `PackageCache/`; new downloads must not be written under `Repos/`.
- New ALCOMD3-only files should use descriptive top-level folders such as
  `config/`, `state/`, `templates/`, `activity-logs/`, and `technical-logs/`
  instead of creating new data under `vrc-get/`.
- Top-level `settings.json` and `vcc.liteDb` remain as VPM/VCC data-format
  compatibility files.
- New package cache files use the `alcomd3-` prefix. Legacy `vrc-get-` package
  cache files remain readable and clearable for manually imported cache
  compatibility.
- Legacy `.alcomtemplate` files are imported as ALCOMD3 template files. Legacy
  VCC directory templates are converted into ALCOMD3 `.alcomtemplate` project
  archive files under `templates/`.
- External app import rewrites imported repository cache paths to the ALCOMD3
  data directory, splits legacy package zip caches into `PackageCache/`, and
  keeps ALCOMD3's own default project and backup paths.
- External app import must not copy old MCP endpoint metadata or old log folders.

Current ALCOMD3 data-root structure:

| Path | Stored content and role |
| --- | --- |
| `settings.json` | VPM/VCC-compatible primary environment settings, including project records, Unity paths, user package folders, default paths, and default repositories. |
| `vcc.liteDb` | LiteDB project and Unity metadata kept in the VCC-compatible data format. |
| `config/gui-config.json` | GUI preferences such as language, setup progress, layout, backup format, update channel, and URL protocol preference. |
| `config/theme-config.json` | Theme display mode, color scheme, and saved theme colors. |
| `config/repository-settings.json` | ALCOMD3/vrc-get repository behavior and package-management settings. |
| `state/vcc-settings-backup.json` | ALCOMD3-owned VPM settings backup/fallback snapshot, not a legacy source path. |
| `Repos/*.json` | Repository index cache files. |
| `PackageCache/<package-id>/alcomd3-*.zip` | Downloaded package zip cache files. |
| `PackageCache/<package-id>/alcomd3-*.zip.sha256` | Checksums for downloaded package zip cache files. |
| `templates/*.alcomtemplate` | Custom and imported ALCOMD3 project templates. |
| `activity-logs/*.jsonl` | Operation activity history shown in the Log view. |
| `technical-logs/alcomd3-*.log` | Technical application logs. Legacy `vrc-get-` log names remain readable in this folder. |
| `mcp/endpoint.json` | Runtime metadata for the local stdio MCP bridge endpoint; generated for the current install and never imported from legacy data roots. |
| `Documents/ALCOMD3/Projects/` | Default project creation folder outside the local data root. |
| `Documents/ALCOMD3/Backups/` | Default project backup folder outside the local data root. |

`vrc-get/*` is not a new runtime write location for ALCOMD3 2.1.0 or later.
It is only an external-app import source when the user explicitly starts a
manual import.

### Startup and window display

Preserve these behaviors:

- Saved initial window size and maximized state are applied when the main window
  is created.
- The main window starts hidden and is shown by the backend after the frontend
  calls the frontend-ready command.
- `index.html` keeps a static first-frame background color so startup does not
  flash pure white before CSS and JavaScript load.
- Startup checks reject unsupported non-ASCII host names before normal UI startup.

### Material Design 3 UI

Preserve these behaviors:

- The Material Theme entry is visible in the side navigation.
- The side navigation can show optional BOOTH, VRChatAvatarLearn, and version
  buttons. They can be hidden with `hide_sidebar_links`.
- The version button displays `version: v{actual_version}` and copies
  `v{actual_version}`.
- Toasts use rounded MD3 styling, MD3 semantic progress colors, and the app base
  background color.
- Important project/package actions use ALCOMD3 emphasis button styling.
- Setup and settings copy use `ALCOMD3` in user-facing text.
- Every supported locale in `vrc-get-gui/locales/*.json5` must cover every
  translation key from `en.json5`. `npm run check` runs
  `scripts/check-locales.mjs` and fails on missing locale keys.

Important areas:

- `vrc-get-gui/app`
- `vrc-get-gui/components`
- `vrc-get-gui/app/globals.css`
- `vrc-get-gui/lib/material-theme.ts`
- `vrc-get-gui/locales/*.json5`
- `vrc-get-gui/src/config.rs`

### Contributor data

The app requests GitHub's public repository contributors REST endpoint directly.
GitHub orders this response by commit count and may cache it for several hours.

- Keep the request unauthenticated and limited to the first 100 contributors.
- Do not add a generated snapshot, proxy, allowlist, or manually maintained
  contributor data.
- Hide the contributor section when GitHub is unavailable or returns no valid
  contributors.

### Icons and third-party notices

The app icon uses the ALCOMD3 theme color `#6cb6ff` and is covered by the
project's third-party asset notices.

- Keep the full logo design when regenerating icons.
- Do not replace small icon sizes with a simplified variant unless that is an
  explicit design decision.
- Keep `vrc-get-gui/THIRD-PARTY.md` accurate.
- The in-app Licenses page is generated by
  `vrc-get-gui/scripts/vite-build-license-json.ts`.

Important assets:

- `vrc-get-gui/app-icon.png`
- `vrc-get-gui/icons/*`
- `vrc-get-gui/icon-LICENSE`
- `vrc-get-gui/third-party/Anton-Regular-OFL.txt`
- `vrc-get-gui/third-party/NotoSans-OFL.txt`

### Package operations

ALCOMD3 owns package operation progress and cancellation behavior.

Preserve these behaviors:

- Installing, removing, and reinstalling packages show a progress dialog.
- The dialog shows per-package status, overall progress, and success/failure counts.
- Failed packages can be retried.
- Long-running operations can be terminated by the user.
- Closing the main window during an operation requests termination instead of
  immediately closing the app.
- Termination must not turn already successful packages into failed items.
- Parallel package work should continue where possible; one package failure
  should not unnecessarily stop unrelated packages.

Backend rules:

- `AbortCheck` is shared between frontend cancellation, window-close
  interception, package download, package extraction, and package apply steps.
- Download/cache verification and zip extraction check cancellation between copy chunks.
- Package install/remove/reinstall progress is reported through
  `TauriProjectApplyProgress`.
- Partial package failures are collected and reported while successful packages
  can still finish.

Important areas:

- `vrc-get-gui/app/_main/projects/manage`
- `vrc-get-gui/src/commands/project.rs`
- `vrc-get-gui/src/commands/start.rs`
- `vrc-get-gui/src/state/project_apply.rs`
- `vrc-get-vpm/src/traits.rs`
- `vrc-get-vpm/src/environment/package_installer.rs`
- `vrc-get-vpm/src/unity_project/pending_project_changes.rs`
- `vrc-get-vpm/src/utils/extract_zip.rs`

### Project copy, backup, and restore

Preserve these behaviors:

- Project copy shows progress and can be canceled.
- Backup and restore workflows report progress instead of appearing stalled.
- GUI backup asks the user to confirm or change the archive name before
  starting. The default is the project name plus a timestamp, the configured
  backup directory remains the destination, and `.zip` is appended
  automatically. Both the GUI precheck and backend enforce file-name and
  target-conflict rules without overwriting an existing archive.
- GUI restore selects a zip archive first, then asks the user to confirm or
  change the restored project name before starting. The archive file stem is
  the default name, the configured default project directory remains the
  destination, and both the GUI precheck and backend enforce folder-name and
  target-conflict rules.
- A completed name or confirmation prompt must detach before a nested backup,
  copy, or restore progress dialog starts. Otherwise the inactive prompt can
  intercept Escape and prevent the active progress dialog from minimizing.
- Project copy and project restore use separate task locks. They should not
  block each other, while each operation still rejects another task of the same
  kind.
- MCP backup/copy/restore calls use standard task-aware execution for queryable
  progress and cancellation, while reusing the same backend task locks as GUI
  operations.
- Main-window close handling coordinates with active long-running operations
  instead of allowing silent interruption.

Important areas:

- `vrc-get-gui/components/BackupProjectDialog.tsx`
- `vrc-get-gui/components/RestoreProjectFromBackupDialog.tsx`
- `vrc-get-gui/app/_main/projects`
- `vrc-get-gui/src/commands/environment/projects.rs`
- `vrc-get-gui/src/commands/project.rs`
- `vrc-get-gui/src/commands/start.rs`
- `vrc-get-gui/src/backend/project_archive.rs`
- `vrc-get-gui/src/state/project_backup.rs`
- `vrc-get-gui/src/state/project_copy.rs`
- `vrc-get-gui/src/state/project_restore.rs`

### Repository management

Preserve ALCOMD3 repository-management features:

- Repository ordering / priority adjustment.
- Viewing the package list contained in a repository.

When importing package or repository-management changes from any source,
preserve these features.

Important areas:

- `vrc-get-gui/app/_main/packages/repositories`
- `vrc-get-gui/components/ReorderableList.tsx`

### Activity records and technical logs

ALCOMD3 keeps user-readable activity records separate from developer-oriented
technical logs.

Preserve these rules:

- The `/log` route defaults to Activity Records. Technical logs remain available
  as a secondary tab for troubleshooting.
- Activity records must include meaningful GUI, MCP, DeepLink, and System
  operations, including failures, cancellations, write operations, MCP tool
  calls, and important passive refreshes.
- Secondary activity records may be hidden by default, but they must stay
  queryable through filters.
- Do not record raw MCP params, token-like values, HTTP header values, URL query
  strings, or URL userinfo credentials in activity details. Local filesystem
  paths may be recorded in full because they are often required to diagnose
  Unity, VPM, and non-ASCII path issues.
- High-volume progress internals, Unity stdout lines, per-file copy events,
  cache hits, rendering events, and logger maintenance belong in technical logs
  or should stay unrecorded.
- Activity JSONL files live under the existing local data tree at
  `activity-logs/` and should keep bounded retention.
- Technical log files live under `technical-logs/`. New log files use the
  `alcomd3-` prefix; legacy `vrc-get-` log file names remain readable.
- MCP log access must stay selective: activity and technical logs use separate
  tools, summary/search results stay paginated, details require an id, and
  technical log messages remain redacted and length-limited. Technical log
  redaction must remove token-like values, authorization/API key material, and
  URL userinfo, query strings, and fragments.

Important areas:

- `vrc-get-gui/src/activity_log.rs`
- `vrc-get-gui/src/commands/activity.rs`
- `vrc-get-gui/app/_main/log/`
- `vrc-get-gui/src/logging.rs`

### MCP bridge

ALCOMD3 includes an optional local MCP bridge. Preserve the minimal local
boundary unless a future change explicitly expands it with review and UI
approval flows.

Preserve these rules:

- MCP data access is disabled by default and must be enabled from the GUI before
  tools return ALCOMD3 data.
- The external MCP server is `alcomd3-mcp` over stdio.
- stdout from `alcomd3-mcp` must contain only MCP JSON-RPC messages; diagnostics
  belong on stderr.
- The GUI exposes a private localhost TCP IPC endpoint while running and writes
  metadata under the local data directory's `mcp/endpoint.json`.
- The GUI MCP enable/disable control only gates tool data access. It must not
  stop the local endpoint; disabled tool calls return `mcp_disabled`.
- `ALCOMD3_MCP_ENDPOINT_FILE` may override the endpoint metadata path for
  development and tests.
- Tools include read-only access for listing projects, reading registered
  project details, listing repositories, reading GUI-visible package details,
  listing GUI-visible packages, listing GUI-visible packages in a selected
  repository, reading environment settings, and selectively querying activity
  and technical logs. Limited write tools may back up registered projects, copy
  registered projects, restore projects from zip backups, and install,
  uninstall, or reinstall a single package through the existing GUI-visible
  project package rules.
- Public MCP tools must remain adapters over GUI capabilities. Every public tool
  must have a mapping in `vrc-get-gui/src/backend/mcp_capabilities.rs`, and
  tests should fail if a tool lacks a GUI capability.
- GUI Tauri commands and MCP IPC/tool dispatch should call shared services under
  `vrc-get-gui/src/backend/` for shared backend logic. MCP-specific code should
  stay limited to access gating, parameter/DTO mapping, task wrapping, error
  mapping, and activity recording.
- Do not add MCP-only business capabilities to close parity gaps. First reuse or
  expose the equivalent GUI backend capability, or stop for design review.
- `project_path` must match an ALCOMD3 registered project before details are read
  or project backup/copy operations use it as a source.
- MCP copy targets and restore backup paths must be absolute paths.
- Restore from backup writes only into the GUI configured default project
  directory.
- MCP package search must not force repository refreshes.
- Every MCP tool call must be recorded in the local activity log with request id,
  tool name, client summary when available, and sanitized details.
- Successful MCP read tools, including log query tools, should be Secondary
  activity records; failures and cancellations remain visible by default.
- `initialize` and `tools/list` must not start the GUI.
- Actual tool calls may start the GUI when the endpoint is unavailable. The
  bridge should start only the packaged/sibling ALCOMD3 GUI executable or the
  explicit `ALCOMD3_GUI_EXECUTABLE` override.
- `alcomd3-mcp` must not install, update, or repair the GUI.
- GUI shutdown should remove the endpoint file.

Important areas:

- `docs/mcp.md`
- `alcomd3-mcp/`
- `alcomd3-mcp-protocol/`
- `vrc-get-gui/src/backend/`
- `vrc-get-gui/src/mcp.rs`
- `vrc-get-gui/src/commands/mcp.rs`
- `vrc-get-gui/app/_main/mcp/index.tsx`
- `xtask/src/build_alcom.rs`
- `xtask/src/bundle_alcom.rs`

### Updater and release

ALCOMD3 uses its own update source and signing key.

Preserve these rules:

- Stable endpoint: `https://alcomd3.cqmhv.com/api/gui/tauri-updater.json`.
- Beta endpoint: `https://alcomd3.cqmhv.com/api/gui/tauri-updater-beta.json`.
- Update-available dialogs include an official website action that opens
  `https://alcomd3.cqmhv.com/`.
- Automatic update check failures stay silent for users.
- Automatic updates are enabled by default and reuse the existing startup
  check. When that check finds an updatable release, the automatic branch skips
  the confirmation dialog and starts the same progress-reporting download and
  staging flow used by manual updates. Automatic download failures stay silent
  for users. The sidebar shows active download progress and opens the shared
  progress dialog when clicked. Disabling the setting only stops future
  confirmation-free downloads; it does not discard an already downloaded update.
- A manually confirmed update installs immediately after the shared download and
  staging flow completes. An automatically downloaded update instead remains on
  the completion dialog, where the user can install it immediately or leave it
  for the next launch. On the next launch, the backend re-verifies and installs
  the staged package before starting MCP or creating the main window, then
  performs only the restart needed to run the new binary. Changing channels
  discards a staged package.
- Command-line, macOS opened-URL/file, and single-instance startup requests are
  captured through one queue while startup is deciding whether to restart. The
  queue is persisted before restart and consumed with at-least-once semantics;
  transient read errors must not overwrite or remove it. If the final handoff
  remains unwritable after bounded retries, startup continues exiting with the
  pre-install snapshot instead of blocking the installer indefinitely. The
  final snapshot check and the end of in-memory capture are atomic, so a late
  request either joins another persisted snapshot or follows the normal live
  request path instead of remaining only in the exiting process.
- A deterministic automatic installation failure suppresses another automatic
  attempt for the same version and channel. A newer version, a channel or
  automatic-update setting change, or a manual update remains eligible. A
  transient I/O error while reading the failure state must preserve that state
  and the staged package, deferring automatic installation until the state can
  be read again. A transient I/O error while launching or applying the staged
  update also keeps the package retryable on the next startup; only a
  deterministic installation rejection records a failed release.
- Manual update checks show dialogs for both no-update and failed-update results.
- Versions use SemVer strings such as `3.0.0-beta.3`.
- The updater public key lives in `vrc-get-gui/src/updater-public-key.txt`; the
  GUI includes that file and the `xtask` verifier reads the same file.
- The updater private key must stay out of git.
- Follow `docs/RELEASE.md` and `docs/ALCOMD3_UPDATER.md` when signing updater
  installers.
- Before publishing public updater JSON, run `cargo xtask
  verify-alcom-updater-json --assets <directory>` for each JSON file that will
  be served.
- One updater manifest atomically describes `windows-x86_64`,
  `darwin-aarch64`, and `linux-x86_64`. Their updater payloads are respectively
  the Windows setup executable, the macOS `.app.tar.gz`, and the Linux
  `.AppImage.tar.gz`.
- The Linux AppImage includes the self-updater. The DEB is built separately
  with `--no-self-updater`, so package-manager installations remain managed by
  the package manager. Each updater/download asset declares this contract with
  `releasePlatforms.*.updater.updateMode` or
  `releasePlatforms.*.downloads[].updateMode`; release shards retain the declared
  mode next to the asset digest.
- The required `macosAdHocSigning` configuration in `alcomd3.config.json`
  ad-hoc signs the app and DMG without Apple notarization, so Gatekeeper can require
  manual first-launch approval. ALCOMD3 updater Minisign separately authenticates
  the `.app.tar.gz`; ad-hoc code signing does not replace or weaken that check.

### Windows installer

Preserve these rules:

- Installer product name is `ALCOMD3`.
- Output setup file is `ALCOMD3_{version}_windows_x86_64_setup.exe`.
- `VersionInfoProductVersion` uses a Windows-compatible numeric version.
- Setup icon uses `vrc-get-gui/icons/icon.ico`.
- Installed main executable is `ALCOMD3.exe`.
- Before installation, unconditionally remove the Inno Setup installation under
  `legacyWindowsAppId` and its known `ALCOM.exe`, `ALCOMD3.exe`, and
  `alcomd3-mcp.exe` files. Abort the new installation if cleanup cannot finish.
- Remove the original user's local WebView data directory named by
  `legacyTauriIdentifier` without gating it on the old AppId registration.
  Preserve the separate `ALCOMD3` data directory.
- Remove known `ALCOM`/`ALCOMD3` shortcuts left by that old AppId from the user
  and common Desktop and Start Menu locations. Preserve whether a desktop
  shortcut was selected and retarget the replacement to the new `ALCOMD3.exe`.
- Do not delete ALCOMD3 user data or clean legacy ALCOM NSIS records and
  unrelated shortcuts that do not belong to the old shared AppId.
- `tauriIdentifier`, `windowsAppId`, and `windowsAumid` are the long-term post-migration
  identities and must not change again. Remove the `legacyTauriIdentifier` and
  `legacyWindowsAppId` cleanups only after a separately reviewed decision that
  the shared migration window has ended; remove the pinned
  `legacyWindowsMigrationReleaseTag` test baseline at the same time.

### GitHub configuration

ALCOMD3 maintains repository-specific release automation in
`.github/workflows/release-draft.yml` and
`.github/workflows/release-updater.yml`. The workflows orchestrate the shared
`cargo xtask release-*` rules; do not reintroduce generic inherited signing or
release actions. See `docs/RELEASE.md`.

`.github/workflows/full-chain.yml` is the cross-platform test workflow. Relevant
pull requests and `main` pushes run Windows x64, macOS arm64 and Linux x64 jobs.
Scheduled/manual runs repeat those jobs and add public updater verification only
inside the Windows job. The macOS smoke bundles are explicitly ad-hoc signed;
Linux bundles remain unsigned. Both are ephemeral test artifacts and must not be
treated as release assets.

The formal Draft workflow is a separate DAG: preflight, three native platform
build shards, trusted `release-assemble`, then Draft creation. Before upload,
the Windows shard upgrades the pinned migration baseline with the exact
source-bound release installer and validates the current identity only after
that upgrade. The macOS shard uses the config-bound ad-hoc signing path and is
not Apple-notarized. Assembly signs and verifies the three updater payloads and
admits exactly ten public assets. After publication, the updater workflow
checks all ten assets and atomically generates the selected
channel's three-platform metadata.

### Release notes and local build commands

- `release-prepare` creates `release-notes/ALCOMD3_$Version.md` when the release
  source commit is prepared.
- Previous release notes are kept under `release-notes/`.
- Future release notes should focus on user-visible ALCOMD3 changes.

Unsigned local Windows shard build:

```powershell
$Version = "<version>"
$Channel = "<stable-or-beta>"
cargo xtask release-build --version $Version --channel $Channel --platform windows-x86_64
```

Expected outputs:

- `artifacts/local-test/v{version}/ALCOMD3_{version}_windows_x86_64_setup.exe`
- `artifacts/local-test/v{version}/ALCOMD3_{version}_windows_x86_64_setup.exe.zip`

Normal `release-build` does not sign updater payloads. Add `--release-artifacts`
only for the exceptional local manual publication flow, build all three
platform shards, then run `release-assemble` to create and verify the three
Minisign signatures and combined release manifest before `release-publish`.

Known non-blocking warnings:

- `xtask/src/bundle_alcom/linux.rs` may warn about an unused `mode` variable.
- Vite may warn that `vrc-get://localhost/global-info.js` cannot be bundled
  without `type="module"`.
