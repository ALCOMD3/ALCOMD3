<p align="center">
    <img src="https://alcomd3.cqmhv.com/assets/logo.png" alt="ALCOMD3 logo" width="160">
</p>

<h1 align="center">ALCOMD3</h1>

<p align="center">
    An open-source alternative to VRChat Creator Companion for managing VRChat Unity projects and VPM packages.
</p>

<p align="center">
    <a href="https://alcomd3.cqmhv.com/">Download</a> ·
    <a href="https://github.com/ALCOMD3/ALCOMD3/releases">Releases</a> ·
    <a href="./docs/README.md">Documentation</a>
</p>

<p align="center">
    English · <a href="./README/README.ja.md">日本語</a> ·
    <a href="./README/README.zh-TW.md">繁體中文</a> ·
    <a href="./README/README.zh-CN.md">简体中文</a>
</p>

## Your VRChat projects, in one place

ALCOMD3 helps creators handle the routine work around VRChat Unity projects
without juggling project folders and package files by hand.

- **Manage projects:** create, register, copy, back up, and restore projects
  from one desktop app.
- **Control VPM packages:** browse repositories and install, remove, or update
  the packages used by each project.
- **Work with familiar links:** open VCC-compatible `vcc://` links and manage
  repositories from the same workflow.
- **Make it yours:** use a Material Design 3-style interface with light, dark,
  and customizable themes.
- **Bring your own AI tools:** optionally give MCP-capable clients scoped local
  access to ALCOMD3 projects, packages, repositories, and logs.

## Get started

1. Visit the [ALCOMD3 website](https://alcomd3.cqmhv.com/) and choose the stable
   or beta channel.
2. Download the package for your operating system and install or launch it.
3. Add an existing VRChat Unity project or create a new one, then manage its
   packages, repositories, and backups from ALCOMD3.

Official builds are available for Windows x64, macOS on Apple Silicon, and
Linux x64. You can also browse every published version on
[GitHub Releases](https://github.com/ALCOMD3/ALCOMD3/releases).

When self-updating is available, ALCOMD3 checks for signed updates at startup
by default, downloads an available update, and installs it before the next
launch. This behavior can be disabled in Settings.

## Optional local MCP integration

ALCOMD3 can connect MCP-capable AI clients to selected project, repository,
package, environment, activity, and technical log data. It also provides a
limited set of project and package operations.

MCP is disabled by default. When enabled, it uses a local stdio bridge and
private loopback-only IPC. The GUI's internal IPC listens on `127.0.0.1`, not
on a public network address. See the [MCP guide](./docs/mcp.md) for setup,
available tools, and permission boundaries.

## Project and community

ALCOMD3 originated from ALCOM/vrc-get and is now maintained as an independent
open-source project. It is not an official product of VRChat or VCC.

- Found a bug or have an idea? [Open an issue](https://github.com/ALCOMD3/ALCOMD3/issues).
- Want to contribute? Read the [contribution guide](./CONTRIBUTING.md).
- Looking for technical or maintainer information? Start at the
  [documentation index](./docs/README.md).
- Want to see what changed? Browse the
  [release notes](https://github.com/ALCOMD3/ALCOMD3/releases).

## Development

You will need the stable Rust toolchain, Node.js, npm, and the build tools for
your target platform.

Run the desktop app in development mode:

```powershell
cd vrc-get-gui
npm run tauri dev
```

For frontend-only development:

```powershell
cd vrc-get-gui
npm run dev
```

Build, test, maintenance, and release information lives in the
[documentation index](./docs/README.md).

## License

The main project code is licensed under the GNU Affero General Public License
v3.0 or later (`AGPL-3.0-or-later`). See [LICENSE](./LICENSE) and
[LICENSE-NOTES.md](./LICENSE-NOTES.md).

Dependency and third-party resource notices are available from the in-app
Licenses page and [vrc-get-gui/THIRD-PARTY.md](./vrc-get-gui/THIRD-PARTY.md).
