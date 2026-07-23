# ALCOMD3 updater signing

言語: [English](../ALCOMD3_UPDATER.md) | 日本語 | [简体中文](ALCOMD3_UPDATER.zh-CN.md)

この文書は updater key material と signature verification を扱う。完全な release
workflow は [RELEASE.ja.md](../RELEASE/RELEASE.ja.md) を使う。

ALCOMD3 は独自の updater key pair と `xtask` signing command を使う。ALCOMD3
release では ALCOMD3 updater key と `ALCOMD3_UPDATER_*` environment variable name を使う。

### Key model

- updater public key は `vrc-get-gui/src/updater-public-key.txt` に置き、GUI updater が include する。
- private key は `xtask sign-alcom-updater` が次から読み込む:
  - `ALCOMD3_UPDATER_PRIVATE_KEY`
  - `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`
- 1 metadata file は 3 platform entries を含み、次の updater payloads を検証する:
  - `windows-x86_64`: `ALCOMD3_{version}_windows_x86_64_setup.exe`。
  - `darwin-aarch64`: `ALCOMD3_{version}_macos_aarch64.app.tar.gz`。
  - `linux-x86_64`: `ALCOMD3_{version}_linux_x86_64.AppImage.tar.gz`。
- Browser downloads は別 assets。Windows setup ZIP、macOS DMG、Linux AppImage、Linux DEB
  を configured updater payload の代わりに使わない。
- AppImage build は in-app self-update を有効にする。DEB は `--no-self-updater` で別 build
  し、package-manager installation を package manager の管理下に保つ。
  `releasePlatforms.*.updater.updateMode` と
  `releasePlatforms.*.downloads[].updateMode` が build mode を決め、release shard と manifest
  に bind される。

Updater JSON には artifact URL と literal signature content を入れる。signature file URL は使わない。

`2.1.2-beta.1` が beta metadata にこの 3-platform contract を導入し、stable `2.1.2` も
同じ contract を採用する。Stable 2.1.1 は immutable historical Release として保持し、
Website は legacy filenames の direct links や compatibility aliases を合成しない。

### Key pair 生成

Key pair は bootstrap または意図的な updater key rotation の場合のみ生成する:

```powershell
$bytes = [byte[]]::new(32)
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($bytes)
$rng.Dispose()
$env:ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD = [Convert]::ToBase64String($bytes)
cargo xtask generate-alcom-updater-key
```

生成物は `artifacts/alcomd3-updater-key/` に書かれる:

- `public-key-base64.txt`: この値を `vrc-get-gui/src/updater-public-key.txt` にコピーする。
- `private-key.env`: 生成された portable signing value。値を repository root の
  `.env` にコピーし、その file は commit しない。
- `private-key.ps1`: secure backup または manual load 用に生成された PowerShell
  signing value。

Private key file は ignore され、commit してはならない。安全な場所に backup する。Private key を失うと、埋め込み public key を使う build は将来 version に auto-update できない。

### Signing variable の読み込み

`.env.example` を repository root の `.env` にコピーし、signing value を入力する。
通常の `cargo xtask release-build --platform ...` は unsigned platform shard だけを生成し、
updater private key を読まない。`release-assemble` は signing variable が未設定の場合に
`.env` を読み、3 updater payloads をまとめて sign して authenticated signature purpose
を選択する。GitHub Actions は同名の Secrets を使う。

### GitHub Actions release

GitHub Actions が既定の release path。次の 2 keys を repository Actions Secrets に設定し、
root `.env` と同じ name にする:

- `ALCOMD3_UPDATER_PRIVATE_KEY`
- `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`

private key / password loader は、copied Secret value の先頭 UTF-8 BOM と末尾 CR/LF を
許容しますが、他の content はそのまま保持します。

`release-draft.yml` は 3 source-bound platform shards を build する。Updater key は trusted
assembly job だけが受け取り、private key の復号と GUI embedded public key との一致を確認
した後、`release-assemble` が Windows setup、macOS app updater archive、Linux AppImage
updater archive を sign/verify し、combined release manifest を atomic に書く。`.env` は
作成・upload しない。

Draft の手動 publish 後、`release-updater.yml` は 10 public assets を正確に download し、
不足または追加された asset を拒否する。3 updater payload/signature pairs を
検証し、Release tag から updater notes を読み、Release `publishedAt` を固定 `pub_date`
として selected channel の 3-platform updater JSON を fresh runner 上で atomic に再生成する。
Exact tag/source SHA、source version、version non-rollback、authenticated `release` purpose
も検証するため、retry 結果は deterministic。Updater workflow は private signing key に
access せず、checkout credentials を persist せず、GitHub token を non-GitHub child process
から除去する。

Protected `release-signing` Environment は将来 optional approval boundary として追加できるが、
current workflow の prerequisite ではない。

`release-build` 以外で signing command を直接実行する場合は、root `.env` を現在の
PowerShell process に読み込む:

```powershell
Get-Content .\.env | Where-Object {
    $_.Trim() -and -not $_.Trim().StartsWith('#')
} | ForEach-Object {
    $name, $value = $_ -split '=', 2
    [Environment]::SetEnvironmentVariable($name, $value, 'Process')
}
```

Loaded private key、password、embedded public key を signature や artifact の書き込みなしで
検証する:

```powershell
cargo xtask verify-alcom-updater-key
```

この command は fixed challenge を memory 内で sign して直ちに verify し、key や生成した
signature を出力しない。

### Updater payload 署名

```powershell
$Version = "2.1.2-beta.1"
$SetupDir = "target\x86_64-pc-windows-msvc\release\bundle\setup"
$Installer = "$SetupDir\ALCOMD3_${Version}_windows_x86_64_setup.exe"
cargo xtask sign-alcom-updater $Installer
```

Direct signing の既定 purpose は authenticated `local-test` で、configured updater payload
1 つの診断に使える。Manually publishable assets は 3 つの
`release-build --release-artifacts` shards を build して `release-assemble` を実行する。
Ad hoc に `--purpose release` を指定しない。

出力:

```text
target/x86_64-pc-windows-msvc/release/bundle/setup/ALCOMD3_{version}_windows_x86_64_setup.exe.sig
```

Updater JSON generator は `.sig` file を読み、内容を `signature` field に入れる。

### Updater JSON 検証

Website 公開前に最終 updater JSON を必ず検証する。既定の published updater workflow
は download 済み public installer に対して自動検証する。次の command は local artifact
validation 用:

```powershell
$Assets = "artifacts\release-check\v2.1.2-beta.1"
cargo xtask verify-alcom-updater-json `
    --assets $Assets `
    --json "website\public\api\gui\tauri-updater-beta.json" `
    --expected-signature-purpose release
```

Local diagnostic signature では matching asset directory と
`--expected-signature-purpose local-test` を使う。

検証では JSON parse と `windows-x86_64`、`darwin-aarch64`、`linux-x86_64` の存在、各
configured URL / updater filename の exact match、signature の存在、authenticated trusted
comment の exact filename / requested purpose binding、
`vrc-get-gui/src/updater-public-key.txt` の public key による 3 payloads の verification を確認する。

### macOS の 2 つの signature layers

macOS release path は ad-hoc signing のみを support する。identity `-` で `.app`、nested
executables、DMG を sign するが、Apple platform trust や notarization は提供しないため、
初回起動時に Gatekeeper で手動承認が必要になる場合がある。ALCOMD3 updater Minisign は
`ALCOMD3_UPDATER_*` を使い、別の `.app.tar.gz` updater
payload を authenticate する。Ad-hoc code signing は updater signature verification を
弱めたり代替したりしない。

### Key rotation rule

通常 release 内で updater key を rotate しない。避けられない場合は、old key で署名され new embedded public key を含む bridge release を先に公開し、その後の installer を new private key で署名する。
