# ALCOMD3 maintenance notes

言語: [English](../MAINTENANCE.md) | 日本語 | [简体中文](MAINTENANCE.zh-CN.md)

このメモは ALCOMD3 の現在の挙動、互換性境界、保守判断を記録する。ALCOMD3 の
コードを変更する場合や、外部ソースから修正を選択的に取り込む場合に使う。

### ブランディング

ALCOMD3 は独自ブランドを使いながら、既存インストールに必要な互換性ポイントを維持する。

- 製品名と表示名: `ALCOMD3`。
- Window title: `ALCOMD3`。
- Windows installer display name: `ALCOMD3`。
- Windows shortcut: `ALCOMD3`。
- Windows GUI process、installer が作成する shortcut、`.alcomtemplate` ProgID、`vcc://`
  protocol registration は、安定した明示的 AUMID `CQMHV.ALCOMD3` を共通で使う。version
  segment を含めず、upgrade 後も同じ Shell identity を維持する。
- Windows setup output name: `ALCOMD3_{version}_windows_x86_64_setup.exe`。
- Installed main executable: `ALCOMD3.exe`。
- macOS application bundle: `ALCOMD3.app`。
- Linux binary、desktop file、icon name、package metadata は `alcomd3` を使う。
- Public asset name は platform と architecture を明示する。macOS download は
  `ALCOMD3_{version}_macos_aarch64.dmg`、Linux downloads は
  `ALCOMD3_{version}_linux_x86_64.AppImage` と
  `ALCOMD3_{version}_linux_amd64.deb`。
- Windows installer は ALCOMD3 専用の新しい AppId を使う。移行期間中は、main
  executable が `ALCOM.exe` と `ALCOMD3.exe` のどちらでも、旧共有 AppId の Inno
  Setup installation を無条件で uninstall する。HKCU と HKLM の 32/64-bit view
  から旧 AppId が消えたことを確認してから新 AppId を登録し、2 つの ALCOMD3 が
  side-by-side で残ることを防ぐ。
- この identity migration は旧 AppId の installation record、既知の program file、
  Desktop と Start Menu にある既知の `ALCOM`/`ALCOMD3` shortcut を削除する。旧 install
  に desktop shortcut があった場合、新 installer は新しい ALCOMD3 desktop shortcut を
  既定で選択し、interactive install では今回の user choice を引き続き尊重する。legacy
  ALCOM NSIS record、無関係な shortcut、ALCOMD3 user data は削除しない。
- `vcc://` URL shortcut association は、明示的な user choice がない場合は既定で有効。
- repository add deep link は、repository metadata の download を開始する前に app 内確認を必須にする。

重要ファイル:

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

### 共有設定

`alcomd3.config.json` は ALCOMD3 の product identity と release metadata の共有元。
product、package、GUI binary、MCP binary、publisher、homepage、GitHub repository、
Windows AUMID、現在および移行期間用の旧 Windows AppId、固定 migration baseline Release tag、updater
manifest path、3-platform release catalog（target、bundle type、
updater payload、download asset、filename pattern）、`.alcomtemplate` association
metadata、description、copyright を管理する。

一部の値は、外部ツールがそれぞれの形式を直接読むため、以下の template に残す。

- `vrc-get-gui/Tauri.toml`
- `vrc-get-gui/bundle/windows-setup.iss`
- `vrc-get-gui/bundle/alcomd3.desktop`
- `vrc-get-gui/bundle/deb-control`
- `website/src/data/site.config.mjs`

これらのファイルは現在の場所に維持する。`cargo test -p xtask alcomd3_identity` が
`alcomd3.config.json` との一致を確認するため、生成ステップを増やさずに template drift
を検出できる。`repositories.txt` は repository list format workflow の example/import
data であり、runtime shared configuration ではない。

### Data directory と external app import

ALCOMD3 は既定で自身の runtime data を所有する。

- 既定の local data directory は platform local data root 配下の `ALCOMD3`
  （Windows では `%LOCALAPPDATA%\ALCOMD3`）。
- 既定の project と backup folder は user Documents 配下の
  `ALCOMD3/Projects` と `ALCOMD3/Backups`。
- ALCOMD3 は通常 startup で VCC または legacy ALCOM data を自動 migrate、move、
  import してはならない。
- ALCOMD3 の install、または 2.0.0 以前から 2.1.0 以降への update は、user が
  external VCC または legacy ALCOM installation から data を明示的に import するまで
  fresh ALCOMD3 data-root install として振る舞う。
- VCC または legacy ALCOM data は、first setup または settings で user が明示的に
  import した場合のみ読んでよい。
- External app import は VPM settings、LiteDB project/Unity data、repository cache、
  vrc-get settings、template data を ALCOMD3 data directory にコピーする。
- Repository index cache は `Repos/` に置く。Downloaded package zip cache は
  `PackageCache/` に置き、新規 download は `Repos/` に書き込まない。
- 新しい ALCOMD3-only file は `config/`、`state/`、`templates/`、
  `activity-logs/`、`technical-logs/` などの descriptive top-level folder を使い、
  `vrc-get/` 配下に新規 data を追加しない。
- Top-level `settings.json` と `vcc.liteDb` は VPM/VCC data-format compatibility
  file として残す。
- 新しい package cache file は `alcomd3-` prefix を使う。Legacy `vrc-get-`
  package cache file は manually imported cache compatibility のためにのみ
  read/clear 可能にする。
- Legacy `.alcomtemplate` file は ALCOMD3 template file として import する。Legacy
  VCC directory template は ALCOMD3 `.alcomtemplate` project archive file に変換し、
  `templates/` に保存する。
- External app import は imported repository cache path を ALCOMD3 data directory に書き換え、
  legacy package zip cache を `PackageCache/` に分離し、ALCOMD3 自身の既定
  project/backup path を維持する。
- External app import は古い MCP endpoint metadata や古い log folder をコピーしてはならない。

現在の ALCOMD3 data-root structure:

| Path | 保存内容と役割 |
| --- | --- |
| `settings.json` | VPM/VCC-compatible primary environment settings。project record、Unity path、user package folder、default path、default repository を含む。 |
| `vcc.liteDb` | VCC-compatible data format で保存する LiteDB project/Unity metadata。 |
| `config/gui-config.json` | language、setup progress、layout、backup format、update channel、URL protocol preference などの GUI preferences。 |
| `config/theme-config.json` | theme display mode、color scheme、保存済み theme color。 |
| `config/repository-settings.json` | ALCOMD3/vrc-get repository behavior と package-management settings。 |
| `state/vcc-settings-backup.json` | ALCOMD3-owned VPM settings backup/fallback snapshot。legacy source path ではない。 |
| `Repos/*.json` | repository index cache file。 |
| `PackageCache/<package-id>/alcomd3-*.zip` | downloaded package zip cache file。 |
| `PackageCache/<package-id>/alcomd3-*.zip.sha256` | downloaded package zip cache file の checksum。 |
| `templates/*.alcomtemplate` | custom/imported ALCOMD3 project template。 |
| `activity-logs/*.jsonl` | Log view に表示する operation activity history。 |
| `technical-logs/alcomd3-*.log` | technical application log。legacy `vrc-get-` log name はこの folder 内で引き続き readable。 |
| `mcp/endpoint.json` | local stdio MCP bridge endpoint の runtime metadata。current install が生成し、legacy data root から import しない。 |
| `Documents/ALCOMD3/Projects/` | local data root 外の default project creation folder。 |
| `Documents/ALCOMD3/Backups/` | local data root 外の default project backup folder。 |

`vrc-get/*` は ALCOMD3 2.1.0 以降の新しい runtime write location ではない。
user が manual import を明示的に開始した場合のみ external-app import source として使う。

### 起動とウィンドウ表示

保持する挙動:

- main window 作成時に保存済み初期サイズと maximized state を適用する。
- main window は hidden で開始し、frontend が frontend-ready command を呼んだ後に backend が表示する。
- `index.html` は first-frame background color を静的に保持し、CSS/JavaScript 読み込み前に白く点滅しないようにする。
- 通常 UI 起動前に、未対応の non-ASCII host name を拒否する。

### Material Design 3 UI

保持する挙動:

- Material Theme entry を side navigation に表示する。
- side navigation は BOOTH、VRChatAvatarLearn、version button を任意表示できる。`hide_sidebar_links` で非表示にできる。
- version button は `version: v{actual_version}` を表示し、`v{actual_version}` をコピーする。
- Toast は rounded MD3 style、MD3 semantic progress color、app base background color を使う。
- 重要な project/package action は ALCOMD3 emphasis button style を使う。
- setup と settings の文言では user-facing text に `ALCOMD3` を使う。
- `vrc-get-gui/locales/*.json5` のすべての supported locale は、`en.json5`
  のすべての translation key を cover しなければならない。`npm run check` は
  `scripts/check-locales.mjs` を実行し、missing locale key がある場合は失敗する。

重要領域:

- `vrc-get-gui/app`
- `vrc-get-gui/components`
- `vrc-get-gui/app/globals.css`
- `vrc-get-gui/lib/material-theme.ts`
- `vrc-get-gui/locales/*.json5`
- `vrc-get-gui/src/config.rs`

### Contributor data

app と Website は GitHub の public repository contributors REST endpoint を直接
request する。GitHub は commit count 順で response を返し、結果を数時間 cache
する場合がある。

- unauthenticated request のまま、先頭 100 contributors に制限する。
- generated snapshot、proxy、allowlist、手動管理の contributor data を追加しない。
- GitHub が利用できない場合や valid contributor を返さない場合は contributor
  section を非表示にする。

### アイコンと第三者表示

アプリアイコンは ALCOMD3 theme color `#6cb6ff` を使い、project の third-party asset notice で扱う。

- icon を再生成するときは full logo design を保持する。
- 明示的な design decision がない限り、小さい icon size を簡略版に置き換えない。
- `vrc-get-gui/THIRD-PARTY.md` を正確に保つ。
- app 内 Licenses page は `vrc-get-gui/scripts/vite-build-license-json.ts` が生成する。

重要アセット:

- `vrc-get-gui/app-icon.png`
- `vrc-get-gui/icons/*`
- `vrc-get-gui/icon-LICENSE`
- `vrc-get-gui/third-party/Anton-Regular-OFL.txt`
- `vrc-get-gui/third-party/NotoSans-OFL.txt`

### Package operation

ALCOMD3 は package operation progress と cancellation behavior を所有する。

保持する挙動:

- install/remove/reinstall package は progress dialog を表示する。
- dialog は per-package status、overall progress、success/failure count を表示する。
- failed package は retry できる。
- long-running operation は user が terminate できる。
- operation 中に main window を閉じると即終了ではなく termination を request する。
- termination は既に成功した package を failed item に変えない。
- 可能な限り parallel package work を継続し、1 つの package failure が無関係な package を止めない。

Backend rule:

- `AbortCheck` は frontend cancellation、window-close interception、package download、package extraction、package apply step で共有する。
- Download/cache verification と zip extraction は copy chunk 間で cancellation を確認する。
- Package install/remove/reinstall progress は `TauriProjectApplyProgress` で報告する。
- Partial package failure は収集して報告し、成功済み package は完了できるようにする。

重要領域:

- `vrc-get-gui/app/_main/projects/manage`
- `vrc-get-gui/src/commands/project.rs`
- `vrc-get-gui/src/commands/start.rs`
- `vrc-get-gui/src/state/project_apply.rs`
- `vrc-get-vpm/src/traits.rs`
- `vrc-get-vpm/src/environment/package_installer.rs`
- `vrc-get-vpm/src/unity_project/pending_project_changes.rs`
- `vrc-get-vpm/src/utils/extract_zip.rs`

### Project copy、backup、restore

保持する挙動:

- Project copy は progress を表示し、cancel できる。
- Backup と restore workflow は停止して見えないよう progress を報告する。
- GUI backup は開始前に archive name の確認または変更を求める。既定値は project name と timestamp、
  保存先は configured backup directory のままで、`.zip` を自動追加する。GUI precheck と backend の
  両方で file name と target conflict を検証し、既存 archive は上書きしない。
- GUI restore は最初に zip backup を選択し、restore 開始前に restore 後の project name を確認または
  変更できる。既定値は backup file stem、restore 先は configured default project directory のままとし、
  GUI precheck と backend の両方で folder name と target conflict を検証する。
- Name または confirmation prompt の完了後は、nested backup、copy、restore progress dialog を開始する前に
  DOM から detach する。非 active prompt が Escape を intercept し、active progress dialog の minimize を
  妨げてはならない。
- Project copy と project restore は別々の task lock を使い、互いに block しない。同種の operation は引き続き重複実行を拒否する。
- MCP の backup/copy/restore call は standard task-aware execution で queryable progress と cancellation を提供し、GUI operation と同じ backend task lock を再利用する。
- Main-window close handling は active long-running operation と協調し、silent interruption を避ける。

重要領域:

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

ALCOMD3 repository-management feature を保持する:

- Repository ordering / priority adjustment。
- Repository に含まれる package list の表示。

外部から package/repository management の変更を取り込む場合も、これらの機能を保持する。

重要領域:

- `vrc-get-gui/app/_main/packages/repositories`
- `vrc-get-gui/components/ReorderableList.tsx`

### Activity records と technical logs

ALCOMD3 は user-readable activity record と developer-oriented technical log
を分けて保持する。

保持する規則:

- `/log` route は既定で Activity Records を表示する。Technical logs は troubleshooting 用の secondary tab として残す。
- Activity records は meaningful な GUI、MCP、DeepLink、System operation を記録する。failure、cancellation、write operation、MCP tool call、重要な passive refresh を含める。
- Secondary activity record は既定で hidden にしてよいが、filter で queryable にする。
- Activity details には raw MCP params、token-like value、HTTP header value、URL query string、URL userinfo credential を記録しない。Unity、VPM、non-ASCII path issue の診断に必要なため、local filesystem path は full path を記録してよい。
- High-volume progress internals、Unity stdout line、per-file copy event、cache hit、rendering event、logger maintenance は technical logs に残すか記録しない。
- Activity JSONL files は既存 local data tree の `activity-logs/` に置き、bounded retention を維持する。
- Technical log files は `technical-logs/` に置く。New log files は `alcomd3-`
  prefix を使い、legacy `vrc-get-` log file name は引き続き readable にする。
- MCP log access は selective に保つ。activity log と technical log は separate tools を使い、summary/search result は paginated、detail は id 指定、technical log message は redacted かつ length-limited にする。Technical log redaction は token-like value、authorization/API key material、URL userinfo、query string、fragment を除去する。

重要領域:

- `vrc-get-gui/src/activity_log.rs`
- `vrc-get-gui/src/commands/activity.rs`
- `vrc-get-gui/app/_main/log/`
- `vrc-get-gui/src/logging.rs`

### MCP bridge

ALCOMD3 には optional local MCP bridge がある。将来の変更が review と UI approval flow により明示的に拡張しない限り、minimal local boundary を保持する。

保持する規則:

- MCP data access は既定で disabled。tool が ALCOMD3 data を返す前に GUI から有効化する必要がある。
- External MCP server は stdio 上の `alcomd3-mcp`。
- `alcomd3-mcp` の stdout は MCP JSON-RPC message のみにする。diagnostic は stderr に出す。
- GUI は起動中に private localhost TCP IPC endpoint を公開し、local data directory の `mcp/endpoint.json` に metadata を書く。
- GUI の MCP enable/disable control は tool data access のみを gate する。local endpoint を停止してはならない。disabled tool call は `mcp_disabled` を返す。
- `ALCOMD3_MCP_ENDPOINT_FILE` は development/test 用に endpoint metadata path を override できる。
- Tool は read-only access として list projects、get registered project details、list repositories、get GUI-visible package details、GUI-visible packages の一覧、selected repository 内の GUI-visible packages の一覧、environment settings の読み取り、activity/technical log の selective query を提供する。限定 write tool は registered project の backup、registered project の copy、zip backup からの restore、既存 GUI-visible project package rule による単一 package の install/uninstall/reinstall を提供する。
- Public MCP tool は GUI capability の adapter のままにする。すべての public tool は
  `vrc-get-gui/src/backend/mcp_capabilities.rs` に mapping を持ち、GUI capability が
  ない tool は test で失敗するようにする。
- GUI Tauri commands と MCP IPC/tool dispatch は、共有 backend logic について
  `vrc-get-gui/src/backend/` の shared service を呼ぶ。MCP-specific code は access
  gating、parameter/DTO mapping、task wrapping、error mapping、activity recording に限定する。
- Parity gap を埋めるために MCP-only business capability を追加しない。先に equivalent
  GUI backend capability を reuse/expose するか、design review のために停止する。
- `project_path` は details の読み取り、project backup、project copy の source として使う前に ALCOMD3 registered project と一致する必要がある。
- MCP copy target と restore backup path は absolute path でなければならない。
- Backup restore は GUI configured default project directory のみに書き込む。
- MCP package search は repository refresh を強制しない。
- すべての MCP tool call は local activity log に記録する。request id、tool name、利用可能な client summary、sanitized details を含める。
- 成功した MCP read tool（log query tool を含む）は Secondary activity record として記録する。failure と cancellation は既定で visible のままにする。
- `initialize` と `tools/list` は GUI を起動しない。
- Actual tool call は endpoint がない場合 GUI を起動してよい。bridge は packaged/sibling ALCOMD3 GUI executable または明示的な `ALCOMD3_GUI_EXECUTABLE` override のみを起動する。
- `alcomd3-mcp` は GUI を install/update/repair してはならない。
- GUI shutdown は endpoint file を削除する。

重要領域:

- `docs/mcp.md`
- `alcomd3-mcp/`
- `alcomd3-mcp-protocol/`
- `vrc-get-gui/src/backend/`
- `vrc-get-gui/src/mcp.rs`
- `vrc-get-gui/src/commands/mcp.rs`
- `vrc-get-gui/app/_main/mcp/index.tsx`
- `xtask/src/build_alcom.rs`
- `xtask/src/bundle_alcom.rs`

### Updater と release

ALCOMD3 は独自の update source と signing key を使う。

保持する規則:

- Stable endpoint: `https://alcomd3.cqmhv.com/api/gui/tauri-updater.json`。
- Beta endpoint: `https://alcomd3.cqmhv.com/api/gui/tauri-updater-beta.json`。
- Update-available dialog は `https://alcomd3.cqmhv.com/` を開く official website action を含む。
- Automatic update check failure は user には silent にする。
- Automatic update は既定で有効で、既存の startup check をそのまま使う。その check で
  install 可能な release が見つかった場合、automatic branch は確認 dialog を表示せず、
  manual update と共通の download、progress report、stage flow を開始する。Automatic
  download の failure は user に通知しない。Download 中は sidebar に progress を表示し、
  click すると共通の progress dialog を開く。設定の無効化は今後の確認なし download だけを
  停止し、download 済み update は破棄しない。
- User が manual に確認した update は、共通の download と stage flow の完了後に直ちに
  install する。Automatic download の update は download 完了画面で待機し、user は直ちに
  install するか次回起動まで残すことができる。次回起動時は MCP の起動や main window の
  作成前に staged package を再検証して install し、新しい binary を実行するための restart
  だけを行う。Channel 変更時は staged package を破棄する。
- Startup で restart の要否を判断している間、command-line、macOS の opened URL/file、
  single-instance request は同じ queue で capture する。Restart 前に queue を永続化し、
  at-least-once semantics で consume する。一時的な read error で queue を上書きまたは
  削除してはならない。Final handoff が有限回の retry 後も書き込めない場合は、installer を
  無期限に block せず、install 前の snapshot を使って exit を続行する。Final snapshot の
  検証と in-memory capture の終了は atomic に行い、遅れて到着した request が次の永続化
  snapshot または通常の live request path に進み、終了中の process memory だけに残らない
  ようにする。
- 決定的な automatic install failure が発生した場合、同じ version と channel の automatic
  retry を停止する。新しい version、channel または automatic-update setting の変更、manual
  update は引き続き install できる。Failure state の読み取り中に一時的な I/O error が発生した
  場合は state と staged package を保持し、再び読み取れるまで automatic install を延期する。
  Staged update の起動または適用中に一時的な I/O error が発生した場合も package を保持して
  次回起動時に retry し、決定的な install rejection のみ failed release として記録する。
- Manual update check は no-update と failed-update の両方で dialog を表示する。
- Version は `2.1.0-beta.1` のような SemVer string を使う。
- Updater public key は `vrc-get-gui/src/updater-public-key.txt` に置く。GUI はこの
  file を include し、`xtask` verifier も同じ file を読む。
- Updater private key は git に入れない。
- Updater installer に署名する場合は `docs/RELEASE/RELEASE.ja.md` と
  `docs/ALCOMD3_UPDATER/ALCOMD3_UPDATER.ja.md` に従う。
- Website updater JSON を公開する前に、serve される各 JSON file に対して
  `cargo xtask verify-alcom-updater-json --assets <directory>` を実行する。
- 1 updater manifest が `windows-x86_64`、`darwin-aarch64`、`linux-x86_64` を atomic
  に記述する。Updater payload はそれぞれ Windows setup executable、macOS
  `.app.tar.gz`、Linux `.AppImage.tar.gz`。
- Linux AppImage は self-updater を有効にする。DEB は `--no-self-updater` で別 build し、
  package-manager installation を package manager の管理下に保つ。各 updater/download asset
  は `releasePlatforms.*.updater.updateMode` または
  `releasePlatforms.*.downloads[].updateMode` でこの contract を宣言し、release shard は
  asset digest とともに mode を保持する。
- `alcomd3.config.json` で必須の `macosAdHocSigning` configuration は app と DMG を ad-hoc
  sign し、Apple notarization は行わないため、Gatekeeper で初回起動の手動承認が必要になる
  場合がある。ALCOMD3 updater Minisign は `.app.tar.gz` を別に authenticate し、ad-hoc
  code signing はその check を代替・弱体化しない。
- Website downloads は stable/beta manifests と shared platform catalog から導出する。
  `2.1.2-beta.1` が 3-platform beta catalog を導入し、stable `2.1.2` も同じ catalog を
  stable に採用する。Stable 2.1.1 は historical GitHub Release として保持し、legacy asset
  names から direct links や aliases を生成しない。

### Windows installer

保持する規則:

- Installer product name は `ALCOMD3`。
- Output setup file は `ALCOMD3_{version}_windows_x86_64_setup.exe`。
- `VersionInfoProductVersion` は Windows-compatible numeric version を使う。
- Setup icon は `vrc-get-gui/icons/icon.ico` を使う。
- Installed main executable は `ALCOMD3.exe`。
- install 前に `legacyWindowsAppId` の Inno Setup installation と、既知の
  `ALCOM.exe`、`ALCOMD3.exe`、`alcomd3-mcp.exe` を無条件で削除する。cleanup を
  完了できない場合は、新規 install を中止する。
- 旧 AppId registration の有無に依存せず、元の install user の
  `legacyTauriIdentifier` local WebView data directory を削除する。独立した
  `ALCOMD3` data directory は保持する。
- 旧 AppId が user/common Desktop と Start Menu に残した既知の `ALCOM`/`ALCOMD3`
  shortcut を削除する。旧 install の desktop shortcut selection を保持し、replacement
  shortcut は新しい `ALCOMD3.exe` を指すようにする。
- ALCOMD3 user data、および旧共有 AppId に属さない legacy ALCOM NSIS record や
  無関係な shortcut は削除しない。
- `tauriIdentifier`、`windowsAppId`、`windowsAumid` は移行後の長期 identity であり、再度変更しては
  ならない。`legacyTauriIdentifier` と `legacyWindowsAppId` の cleanup は、共通の移行期間
  終了を別途 review した後にのみ、固定 test baseline の
  `legacyWindowsMigrationReleaseTag` と同時に削除できる。

### GitHub configuration

ALCOMD3 repository-specific release automation は
`.github/workflows/release-draft.yml` と
`.github/workflows/release-updater.yml` で保守する。Workflows は共有の
`cargo xtask release-*` rules を orchestration し、generic inherited signing/release
action は再導入しない。詳細は `docs/RELEASE/RELEASE.ja.md`。

`.github/workflows/full-chain.yml` は cross-platform test workflow。関連 PR と `main`
push では Windows x64、macOS arm64、Linux x64 jobs を実行する。Scheduled / manual
run は 3 jobs を再実行し、public updater verification は Windows job 内だけで追加する。
この workflow の macOS smoke bundles は明示的に ad-hoc sign し、Linux bundles は unsigned
のままとする。どちらも ephemeral test artifacts で、Release assets として扱わない。

正式 Draft workflow は別の DAG を使う: preflight、3 native platform build shards、trusted
`release-assemble`、Draft creation。Upload 前に Windows shard は正確な source-bound
release installer で固定 migration baseline を upgrade し、その後にだけ current identity
を検証する。macOS shard は config-bound ad-hoc signing path を使い、Apple notarization
は行わない。Assembly は 3 updater payloads を sign/verify し、exact 10 public assets
だけを許可する。Publish 後、updater workflow は 10 assets すべての attestation を検証し、
selected channel の 3-platform metadata を atomic に生成する。

### Release notes と local build commands

- Release source commit の準備時に `release-prepare` が
  `release-notes/ALCOMD3_$Version.md` を作成する。
- 過去の release notes は `release-notes/` に保持する。
- 今後の release notes は user-visible ALCOMD3 changes を中心にする。

Unsigned local Windows shard build:

```powershell
$Version = "<version>"
$Channel = "<stable-or-beta>"
cargo xtask release-build --version $Version --channel $Channel --platform windows-x86_64
```

Expected outputs:

- `artifacts/local-test/v{version}/ALCOMD3_{version}_windows_x86_64_setup.exe`
- `artifacts/local-test/v{version}/ALCOMD3_{version}_windows_x86_64_setup.exe.zip`

通常の `release-build` は updater payload に署名しない。例外的な local manual
publication flow のみ `--release-artifacts` を追加して 3 platform shards を build し、
`release-assemble` で 3 Minisign signatures と combined release manifest を生成・検証して
から `release-publish` を実行する。

Known non-blocking warnings:

- `xtask/src/bundle_alcom/linux.rs` は unused `mode` variable warning を出す場合がある。
- Vite は `vrc-get://localhost/global-info.js` が `type="module"` なしでは bundle できないと warning を出す場合がある。
