# ALCOMD3 release workflow

言語: [English](../RELEASE.md) | 日本語 | [简体中文](RELEASE.zh-CN.md)

これは ALCOMD3 の release-day workflow。GitHub Actions を既定の release orchestrator
とする。Local `release-build` は既定で single-platform unsigned temporary shard だけを
生成する。`xtask` は明示的・例外的な 3-platform shard build、signed release assembly、
GitHub manual publish path も保持する。

Release は 3 phases と 2 commits:

1. Source release commit: version metadata と release notes。
2. GitHub Release: Windows x64、macOS Apple Silicon、Linux x64 向けの
   platform を明示した 10 assets。
3. Updater metadata commit: public assets 作成後、3 platforms を含む updater JSON
   を atomic に生成して commit する。

`Full-chain desktop smoke` は ad-hoc signed macOS smoke artifact と unsigned Linux smoke
artifact を build するが、release input には使わない。正式な `Build release draft`
workflow は `alcomd3.config.json` で必須の macOS ad-hoc signing configuration を使う。
app と DMG を ad-hoc sign し、Apple notarization は行わない。3 platforms の updater
payload は trusted assembly で別途 ALCOMD3 Minisign key により署名する。Windows shard
は upload 前に、正確な source-bound setup EXE で固定 migration baseline を upgrade
する。Full-chain の一時 artifact と異なり、この smoke result は正式 release gate である。

Windows identity reset の移行期間中、release は installer upgrade smoke により、旧共有
AppId が HKCU と HKLM の両 view から消え、`ALCOM.exe`/`ALCOMD3.exe` の historical
installation と既知の旧 Desktop/Start Menu shortcut が削除され、既存の desktop shortcut
choice が新 executable を指す replacement として復元され、新しい `windowsAppId` だけが
登録され、GUI process、新 shortcut、template ProgID、`vcc://` registration が設定済みの
`windowsAumid` を共通で使い、旧 `legacyTauriIdentifier` WebView directory が削除され、独立した `ALCOMD3`
user data が保持されることを確認しなければならない。この cleanup を迂回する installer
は release しない。
Migration baseline は `legacyWindowsMigrationReleaseTag` が指定する最後の旧 AppId stable
release に固定する。

### Agent execution semantics

明示的な audit request は read-only のまま扱う。明示的な release request は readiness
report ではなく end-to-end operation とする。正しい comparison base から完全な release
notes を生成し、7 languages すべての updater short summary を作成し、source release files
を validate、commit、push した後、Draft workflow を dispatch して監視する。Manual Draft
publish gate でのみ停止する。Draft publish 後は updater workflow、metadata commit、
public endpoint の監視を続け、すべて成功した場合だけ release complete とする。

### Version ownership

- Rust release version source: root `Cargo.toml` の `[workspace.package].version`。
- Rust workspace members は `version.workspace = true` で継承する。
- `vrc-get-gui/package.json` は `cargo xtask release-prepare` が更新する。
- `Cargo.lock` と `package-lock.json` は generated files。
- Updater JSON は published updater workflow が public Release assets から再生成・検証し、
  検証成功後だけ commit する。
- `release-notes/ALCOMD3_$Version.updater-notes.json` は in-app updater dialog に表示する短い localized update text。

### 1. Release inputs を決める

Channel は 1 つだけ使う。

| Channel | Version example | GitHub Release type | Updater JSON |
| --- | --- | --- | --- |
| Stable | `3.0.0` | normal release | Metadata repository `public/api/gui/tauri-updater.json` |
| Beta | `3.1.0-beta.1` | prerelease | Metadata repository `public/api/gui/tauri-updater-beta.json` |

この repository で最初に visible となる release は `3.0.0`。

PowerShell で変数を設定する:

```powershell
$Version = "3.0.0"
$Channel = "stable"
```

Stable version は prerelease metadata を含めない。Beta version は prerelease metadata を含める。

Repository prerequisites:

- `ALCOMD3_UPDATER_PRIVATE_KEY`、`ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`、
  `ALCOMD3_WEBSITE_DEPLOY_KEY` を repository Actions Secrets として保存する。

macOS release は ad-hoc signing のみを support し、Apple account や Apple Secrets は不要。
signing command は certificate identity や notarization option を提供しない。Platform 固有の
build、signing、installation、update mechanism は technical contract に限る。Release notes、
updater notes、Website はすべての公開 platform に同じ方針を適用し、これらの mechanism だけを
理由に platform 専用の disclosure、warning、手順、help link を追加しない。この release に
関係する user-visible change がある場合だけ platform 名を記載する。

各 updater payload と browser download は `alcomd3.config.json` で `updateMode` を宣言する。
Self-update する asset は `self-updater`、Linux DEB は `no-self-updater` を使う。Build command は
この値を使用し、source-bound shard と combined manifest は asset ごとの mode を保持する。


Protected `release-signing` Environment は optional hardening であり、現在の prerequisite
ではない。将来 multiple maintainers による separate approval boundary が必要になったら、
同名 Secrets をその Environment に移し、workflow の environment binding と required
reviewer を同時に設定する。
それまでは repository Secrets 自体に separate approval boundary がないため、`main` と
Actions workflow の変更権限を厳しく管理する。

### 2. Clean state から開始する

```powershell
git checkout main
git pull --ff-only
git status --short
gh auth status --hostname github.com
```

Expected result:

- `git status --short` は何も出力しない。
- GitHub CLI が `ALCOMD3/ALCOMD3` に対して authenticated。

Worktree が dirty の場合は止める。

### 3. Source release files を準備する

```powershell
cargo xtask release-prepare --version $Version --channel $Channel
```

この command は:

- `Cargo.toml` workspace version を更新する。
- `Cargo.lock` workspace package versions を更新する。
- GUI の npm version を tag なしで更新する。
- npm lockfiles を更新する。
- `release-notes/ALCOMD3_$Version.md` がなければ作成する。
- `git status --short` を表示する。

ここで release notes file を編集し、placeholder text をすべて削除する。

Release notes は正しい comparison base を使う:

- Stable release は前回の stable release だけと比較する。
- Beta release は直前の release と比較する。直前が stable でも beta でも同じ。

Release notes は統一された多言語構造も使用する。Title は正確に
`# ALCOMD3 v$Version` とし、その後に `## English`、`## 日本語`、`## 中文` をこの順で
置く。各 locale は 1 段落の概要から始め、アプリの更新、インストールとアップグレード、
互換性とセキュリティの 3 つの level-3 category をこの順で必ず保持する。Localized title
はそれぞれ `Application updates` / `アプリの更新` / `应用更新`、`Installation and upgrade` /
`インストールとアップグレード` / `安装与升级`、`Compatibility and security` /
`互換性とセキュリティ` / `兼容性与安全` とする。Release 固有の level-3 heading を追加したり、
固定 heading を省略、並べ替え、改名したりしない。各 category には空でない bullet list を
含め、user-visible な変更がない category も保持して、その旨を各言語で明記する。Level-4
heading、fenced code block、indent された ATX heading と top-level bullet は使用しない。
いずれの platform についても定型 disclosure で固定構造を埋めない。`release-validate` は
固定 heading と構造を検証し、localized bullet の意味と順序が一致していることは release
review で確認する。
最初の visible release は `3.0.0` で、この release から固定 3-category contract を適用する。

同時に `release-notes/ALCOMD3_$Version.updater-notes.json` を作成または更新する。
これは in-app updater dialog 用の短い localized summary であり、完全な GitHub
Release notes ではない。JSON object とし、key は `en`、`de`、`fr`、`ja`、`ko`、
`zh_hans`、`zh_hant` のみ。Value は非空 string。欠けている language は generated
`notes` field に fallback する。Normal release では 7 keys すべてを生成する。Fallback は
compatibility / recovery 用に残すが、通常の release preparation outcome にはしない。

Source release commit を commit/push する:

```powershell
git add Cargo.toml Cargo.lock
git add vrc-get-gui/package.json vrc-get-gui/package-lock.json
git add "release-notes/ALCOMD3_$Version.md"
git add "release-notes/ALCOMD3_$Version.updater-notes.json"
git status --short
git commit -m "release: prepare ALCOMD3 $Version"
git push origin main
```

この commit は GitHub Release tag が指す source state。Generated installer、
`target/`、`artifacts/`、updater JSON を含めない。

### 4. Draft build workflow を実行する

GitHub Actions で **Build release draft** を手動実行する。または:

```powershell
gh workflow run release-draft.yml --repo ALCOMD3/ALCOMD3 `
    -f version=$Version `
    -f channel=$Channel `
    -f replace_existing_draft=false
```

同じ version の Draft が既にあり、その assets を明示的に置換する場合だけ
`replace_existing_draft=true` を使う。Published Release または channel が一致しない Draft
の上書きは拒否される。Release notes の修正などにより Draft 作成後に prepared source commit
が変わった場合、明示的な置換は Draft を dispatch 対象の source commit に付け替え、その
commit から build した 10 assets をすべて置換する。

Workflow は dispatch 時に記録された immutable `github.sha` を checkout して source commit
を記録し、prepared source を検証した後、build 前に `release-preflight` を実行する。Initial
creation では target tag が存在しないこと、explicit replacement では channel が一致し、
unexpected asset のない既存 Draft であることを要求する。Authentication、network、API error
を Release missing として扱わない。

Workflow DAG は `preflight` -> 3 platform build shards -> trusted assembly -> Draft。
Windows x64、macOS arm64、Linux x64 は同じ pinned source commit から、それぞれ native
runner で build する。各 `release-build --platform ... --github-actions-release` は updater
Minisign signature がまだない platform shard と source-bound shard manifest を生成する。
Windows shard は upload 前に exact Release tag endpoint から
`legacyWindowsMigrationReleaseTag` を解決し、setup ZIP を検証し、固定 stable baseline
を install して `artifacts/release/v$Version/` にコピーされた setup EXE へ upgrade する。
Baseline validation は historical contract のみを確認し、current AppId、AUMID、file
association、shortcut、migration cleanup、launch、uninstall assertion は upgrade 後に
実行する。macOS shard は required ad-hoc signing state も manifest に bind する。
identity `-` で nested executables、app、DMG を sign し、secure timestamp と notarization
は行わず、upload 前に final signatures が `Signature=adhoc` を報告することを検証する。

3 shards がすべて成功した後、`release-assemble` が source SHA、allowlist、digest を検証する。
続いて updater key を一度だけ復号し、Windows installer、macOS app updater archive、Linux
AppImage updater archive に署名して 3 Minisign signatures を検証し、combined release
manifest を atomic に生成する。その後にだけ `release-publish` が Draft を作成できる。
npm download cache は lockfile で keying し、Rust build output、signing material、release
asset は cache しない。Rust toolchain と release asset pattern は `alcomd3.config.json`
から取得する。Updater signing material:

- `ALCOMD3_UPDATER_PRIVATE_KEY`
- `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`

Runner で固定 version の Inno Setup installer を実行する前に、GitHub Release asset
digest に対して SHA-256 を検証する。

Workflow は Draft を publish しない。短期 shard artifacts には assembly job が必要とする
release assets と source-bound shard manifests だけを含める。Checkout credentials は
persist せず、signing Secrets は必要な jobs / steps にだけ公開する。Draft 作成成功後、
Job Summary に Release URL、source / target commit、Draft/prerelease state、すべての asset
digest を記録する。

### 5. Draft を確認して手動 publish する

Publish 前に確認する:

- tag が `v$Version` で、workflow が build した source release commit を指す。
- title が `Version $Version`。
- stable は normal Release、beta は prerelease。
- release notes が正しい。
- 次の 10 assets が正確に揃っている:
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
- 追加の upload asset がない。

すべての new public filename は platform と architecture を明示する。Historical Release
assets は rename も rewrite もしない。

GitHub UI で Draft を手動 publish する。これは release gate であり、既定の build
workflow は bypass しない。

### 6. Updater workflow を待つ

Draft の publish により **Publish updater metadata** が起動する。Fresh runner 上で:

- Published Release から version と stable/beta channel を導出する。
- 10 public assets を正確に download し、asset の不足や追加を拒否する。
- Release target、tag commit、root/GUI version の完全一致を要求する。
- 3 updater payloads と Minisign signatures を検証し、各 signature が exact filename と
  authenticated `release` purpose に bind されていることを確認する。
- Release tag から localized sidecar を読み、Release `publishedAt` を固定 `pub_date` として、
  Windows x64、macOS arm64、Linux x64 を含む current channel updater JSON を atomic に再生成する。
- Metadata file を置換する前に、各 platform entry の version、exact URL、signature filename、
  signature、embedded public key を検証する。
- Updater version rollback を拒否し、same-version retry は byte-identical metadata のみ許可する。
- 設定された metadata repository を clone し、selected channel JSON を設定済み repository
  path に直接書き込む。
- Metadata repository ではその JSON だけを commit し、`main` を push する。
- Public updater endpoint が同じ version、3 exact platform URLs、signatures を返すまで待つ。

Public endpoint 成功後、Job Summary に version、channel、Release / source commit、metadata
commit 作成有無、final metadata repository commit、verified endpoint を記録する。

Updater workflow は private key を受け取らない。Checkout credentials は persist せず、
`GH_TOKEN` は必要な steps だけに公開し、non-GitHub cargo/git child process から除去する。

### 7. 完了と retry を確認する

**Publish updater metadata** と public endpoint check の両方が成功したときだけ release
complete。Workflow は有限回 retry する。Timeout は検証済み metadata commit を rollback
しない。Metadata hosting を修復し、updater workflow を再実行する。同じ Release は
byte-identical JSON を再生成するため、変更なしの
commit/push を skip して endpoint check を続行する。

### Local build と例外的な manual publication

Local machine は計画上の release orchestrator ではない。通常の local `release-build` は
`--platform` で選んだ 1 platform だけを build し、`artifacts/local-test/v$Version/` に
**unsigned** shard を生成する。Updater payload の Minisign 署名は行わず、macOS の通常
local build は certificate-based signing、notarization、staple も行わない:

```powershell
cargo xtask release-build --version $Version --channel $Channel --platform windows-x86_64
```

他の platform keys は `darwin-aarch64` と `linux-x86_64`。これらの shard は package
inspection には使えるが、そのまま official asset publisher に渡せない。

Maintainer が local machine からの publish を明示的に必要とする場合は、clean で
`origin/main` と完全一致する `main` から release-purpose artifacts を build する:

```powershell
cargo xtask release-validate --version $Version --channel $Channel
cargo xtask release-build --version $Version --channel $Channel --platform windows-x86_64 --release-artifacts
cargo xtask release-build --version $Version --channel $Channel --platform darwin-aarch64 --release-artifacts
cargo xtask release-build --version $Version --channel $Channel --platform linux-x86_64 --release-artifacts
$SourceSha = git rev-parse HEAD
cargo xtask release-assemble --version $Version --channel $Channel --source-sha $SourceSha
cargo xtask release-publish --version $Version --channel $Channel
```

各 explicit build が生成する platform shard には updater Minisign signature がまだない。
`release-assemble` は必須で、3 source-bound shard manifests を検証し、3 updater payloads
を `release` purpose で sign/verify し、exact 10-file allowlist を確認して、combined ignored
`artifacts/release-state/v$Version.json` manifest を作る。`release-publish` は upload 前に
再検証する。Local macOS release shard は Actions と同じ required ad-hoc signing path を
使って macOS 上で生成し、app、DMG、shard manifest の state を assembly 前に一致させる。
Compatible Draft の置換時だけ `--replace-assets` を追加する。明示的な置換では
Draft を検証済み build source に付け替え、upload 後に再検証する。Local publication にも
明示的な authorization が必要で、Release public が許可された場合だけ `--publish` を追加する。
Actions / local のどちらで作った Draft も publish 後は同じ updater workflow を起動する。
障害復旧または再検証では、公開済み Release のタグを指定して同じ workflow を手動実行
できる。Release 名、チャンネル、ソースコミットは GitHub から解決される。
Updater metadata publication は Actions-only とし、source binding、monotonic
version、serialized queue、public endpoint check の bypass を防ぐ。
Windows identity migration 中は、例外の local publication も GitHub-hosted の正式
installer upgrade smoke を bypass できない。正確な local setup EXE をこの gate で証明
できない場合は Draft workflow を使う。

`target/`、`artifacts/`、`.env`、または matching Release assets の公開前に生成した updater
JSON は commit しない。

### Failure rules

以下の場合は release を止める:

- release notes に placeholder text が残っている、または統一された多言語構造に違反している。
- updater notes sidecar が必要なのに不足している、または JSON、language key、空 value が不正。
- validation が失敗する。
- source-bound Windows release installer upgrade smoke が失敗、cancel、未実行、または
  Windows release shard 内の setup EXE 以外を test している。
- signing variables または signing key loader がない。
- macOS shard が shared config 必須の ad-hoc signing に bind されていない。
- app、nested executables、updater archive contents、DMG の ad-hoc signing、strict
  verification、`Signature=adhoc` check のいずれかが失敗する。
- artifact が不足している。
- updater JSON verification が失敗する。
- release notes の comparison base が誤っている。
- GitHub Release title が `Version $Version` ではない。
- GitHub Release assets が不足または名前不一致。
- stable/beta flags が誤っている。
- Release target SHA、tag commit、source versions が一致しない。
- Release signature が `local-test`、または別の filename に bind されている。
- Updater metadata が selected channel を rollback する。
- Draft 置換時に対象 Release がすでに published。
- initial Draft creation で Release がすでに存在する、または replacement 対象の compatible
  Draft がない、もしくは unexpected asset がある。
- updater private key を復号できない、または GUI embedded public key と一致しない。
- GitHub Release assets が public になる前に updater JSON を publish しようとしている。
