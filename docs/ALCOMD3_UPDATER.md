# ALCOMD3 updater signing

Languages: English | [日本語](ALCOMD3_UPDATER/ALCOMD3_UPDATER.ja.md) |
[简体中文](ALCOMD3_UPDATER/ALCOMD3_UPDATER.zh-CN.md)

This document covers updater key material and signature verification. For the
complete release workflow, use [RELEASE.md](./RELEASE.md).

ALCOMD3 uses its own updater key pair and `xtask` signing commands. Use the
ALCOMD3 updater key and `ALCOMD3_UPDATER_*` environment variable names for
ALCOMD3 releases.

### Key model

- The updater public key is stored in
  `vrc-get-gui/src/updater-public-key.txt` and included by the GUI updater.
- The private key is loaded by `xtask sign-alcom-updater` from:
  - `ALCOMD3_UPDATER_PRIVATE_KEY`
  - `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`
- One metadata file contains three platform entries and verifies these updater
  payloads:
  - `windows-x86_64`:
    `ALCOMD3_{version}_windows_x86_64_setup.exe`;
  - `darwin-aarch64`:
    `ALCOMD3_{version}_macos_aarch64.app.tar.gz`;
  - `linux-x86_64`:
    `ALCOMD3_{version}_linux_x86_64.AppImage.tar.gz`.
- Browser downloads are separate assets: the Windows setup ZIP, macOS DMG,
  Linux AppImage, and Linux DEB are not substituted for their configured updater
  payloads.
- The AppImage build enables in-app self-update. The DEB is built separately
  with `--no-self-updater`, so package-manager installations remain managed by
  the package manager. `releasePlatforms.*.updater.updateMode` and
  `releasePlatforms.*.downloads[].updateMode` drive the build mode and are bound
  into the release shard and manifest.

The updater JSON must contain the artifact URL and literal signature content,
not a signature file URL.

`2.1.2-beta.1` introduced this three-platform contract for beta; stable `2.1.2`
adopts the same contract. Stable 2.1.1 remains an immutable historical Release,
and the website never synthesizes direct links or compatibility aliases for its
legacy filenames.

### Generate a key pair

Generate a key pair only when bootstrapping or intentionally rotating the updater key:

```powershell
$bytes = [byte[]]::new(32)
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($bytes)
$rng.Dispose()
$env:ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD = [Convert]::ToBase64String($bytes)
cargo xtask generate-alcom-updater-key
```

Generated files are written under `artifacts/alcomd3-updater-key/`:

- `public-key-base64.txt`: copy this value into
  `vrc-get-gui/src/updater-public-key.txt`.
- `private-key.env`: generated portable signing values; copy its values into
  the repository-root `.env` without committing that file.
- `private-key.ps1`: generated PowerShell signing values for secure backup or
  manual loading.

The private key files are ignored and must not be committed. Back them up in a
secure location. If the private key is lost, builds using the embedded public
key cannot auto-update to future versions.

### Load signing variables

Copy `.env.example` to the repository-root `.env` and fill in the signing
values. A normal `cargo xtask release-build --platform ...` creates an unsigned
platform shard and does not read the updater private key. `release-assemble`
loads `.env` when signing variables are not already set, signs all three updater
payloads together, and selects the authenticated signature purpose. GitHub
Actions uses Secrets with the same names.

### GitHub Actions release use

GitHub Actions is the default release path. Configure these repository Actions
Secrets with names identical to the root `.env` keys:

- `ALCOMD3_UPDATER_PRIVATE_KEY`
- `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`

The private-key and password loaders accept a leading UTF-8 BOM and trailing
CR/LF line ending from copied Secret values, but preserve all other content.

`release-draft.yml` builds three source-bound platform shards. Only the trusted
assembly job receives the updater key: it first checks that the key decrypts and
matches the GUI's embedded public key, then `release-assemble` signs and verifies
the Windows setup, macOS app updater archive, and Linux AppImage updater archive
and atomically writes the combined release manifest. It does not create or
upload an `.env` file.

After the Draft is manually published, `release-updater.yml` downloads the exact
ten public assets and rejects an incomplete or unexpected asset set. It verifies
the three updater payload/signature pairs, reads updater notes from the Release
tag, and atomically regenerates the selected channel's three-platform updater
JSON on a fresh runner using the Release `publishedAt` as a fixed `pub_date`.
It also verifies the exact tag/source SHA and source versions, rejects version
rollback, and requires an authenticated `release` signature purpose. This makes
retries deterministic. The updater workflow has no access to the private signing
key; checkout credentials are not persisted, and GitHub tokens are removed from
non-GitHub child processes.

A protected `release-signing` Environment can be added later as an optional
approval boundary, but it is not required by the current workflow.

For direct signing commands outside `release-build`, load the root `.env` into
the current PowerShell process:

```powershell
Get-Content .\.env | Where-Object {
    $_.Trim() -and -not $_.Trim().StartsWith('#')
} | ForEach-Object {
    $name, $value = $_ -split '=', 2
    [Environment]::SetEnvironmentVariable($name, $value, 'Process')
}
```

Verify the loaded private key, password, and embedded public key without writing
a signature or artifact:

```powershell
cargo xtask verify-alcom-updater-key
```

This command signs a fixed challenge in memory and verifies it immediately. It
does not print the key or generated signature.

### Sign an updater payload

```powershell
$Version = "2.1.2-beta.1"
$SetupDir = "target\x86_64-pc-windows-msvc\release\bundle\setup"
$Installer = "$SetupDir\ALCOMD3_${Version}_windows_x86_64_setup.exe"
cargo xtask sign-alcom-updater $Installer
```

Direct signing defaults to the authenticated `local-test` purpose. This command
can sign any one configured updater payload for diagnostics. Manually publishable
assets require all three `release-build --release-artifacts` shards followed by
`release-assemble`; do not set `--purpose release` ad hoc.

This writes:

```text
target/x86_64-pc-windows-msvc/release/bundle/setup/ALCOMD3_{version}_windows_x86_64_setup.exe.sig
```

The updater JSON generator reads the `.sig` file and inserts its content into
the `signature` field.

### Verify updater JSON

Always verify the final updater JSON before publishing the website. The default
published updater workflow performs this against the downloaded public
installer; use this command for local artifact verification:

```powershell
$Assets = "artifacts\release-check\v2.1.2-beta.1"
cargo xtask verify-alcom-updater-json `
    --assets $Assets `
    --json "website\public\api\gui\tauri-updater-beta.json" `
    --expected-signature-purpose release
```

For local diagnostic signatures, use the matching asset directory and
`--expected-signature-purpose local-test`.

Verification checks that the JSON parses and contains `windows-x86_64`,
`darwin-aarch64`, and `linux-x86_64`; every configured URL and updater filename
matches exactly; each signature is present and its authenticated trusted comment
binds the exact filename and requested purpose; and all three payloads verify
with the public key in `vrc-get-gui/src/updater-public-key.txt`.

### macOS signature layers

The macOS release path supports ad-hoc signing only. It signs the `.app`, its
nested executables, and the DMG with identity `-`, but it does not provide Apple
platform trust or notarization; Gatekeeper can therefore require manual approval
on first launch. The ALCOMD3 updater Minisign
signature separately authenticates the `.app.tar.gz` updater payload with
`ALCOMD3_UPDATER_*`. Ad-hoc code signing does not weaken or replace updater
signature verification.

### Key rotation rule

Do not rotate the updater key inside a normal release. If rotation is
unavoidable, publish a bridge release signed by the old key and containing the
new embedded public key before signing future installers with the new private key.
