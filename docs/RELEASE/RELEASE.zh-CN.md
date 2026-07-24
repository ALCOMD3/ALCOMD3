# ALCOMD3 release workflow

语言: [English](../RELEASE.md) | [日本語](RELEASE.ja.md) | 简体中文

这是 ALCOMD3 发布当天的流程。GitHub Actions 是默认发布编排入口。本地
`release-build` 默认只生成单平台未签名临时 shard；`xtask` 仍保留显式、例外的三平台
shard 构建、签名组装和 GitHub 手动发布路径。

发布分为三个阶段、两个提交：

1. Source release commit：版本 metadata 和 release notes。
2. GitHub Release：Windows x64、macOS Apple Silicon 和 Linux x64 的 10 项平台显式资产。
3. Updater metadata commit：公开 assets 存在后，原子生成并提交包含三个平台的 updater JSON。

`Full-chain desktop smoke` 构建 ad-hoc 签名的 macOS smoke 产物和未签名的 Linux smoke 产物，
它们绝不作为正式发布输入。正式 `Build release draft` workflow 读取
`alcomd3.config.json` 中要求的 macOS ad-hoc 签名配置；它对 app 和 DMG 进行 ad-hoc
签名且不进行 Apple 公证。三个平台的 updater payload 会在可信组装阶段使用独立的
ALCOMD3 Minisign 密钥签名。Windows shard 还会在上传前，使用准确的 source-bound setup
EXE 升级固定迁移基线；与 Full-chain 的临时产物不同，这项 smoke 结果是正式发布门禁。

身份重置过渡期的 Windows 发布必须通过安装/升级 smoke：旧共享 AppId 在 HKCU 与 HKLM
两个注册表视图中均被移除，`ALCOM.exe`/`ALCOMD3.exe` 历史安装被清理，旧桌面和开始菜单
快捷方式被移除，已有桌面快捷方式选择得到恢复且替代快捷方式指向新程序；新的
GUI 进程、新快捷方式、模板 ProgID 和 `vcc://` 注册统一使用配置的 `windowsAumid`；
`windowsAppId` 成为唯一注册项，旧 `legacyTauriIdentifier` WebView 目录被移除，并保留
独立的 `ALCOMD3` 用户数据。不得发布绕过这项清理的安装器。
迁移基线必须保持为 `legacyWindowsMigrationReleaseTag` 指定的最后一个旧 AppId 稳定版。

### Agent 执行语义

用户明确要求审计时保持只读。用户明确要求发布时，必须把它作为端到端操作，而不是只报告
“可以发布”：准备并验证 source release files，按正确比对基准生成完整 release notes，生成
全部 7 种语言的 updater 短说明，提交并推送 source release，然后触发并监控 Draft workflow。
只在 Draft 人工发布门暂停。Draft 公开后继续监控 updater workflow、metadata commit、
Cloudflare Pages deployment 和公开 endpoint；这些检查全部通过后，发布才算完成。

### Version ownership

- Rust 发布版本源：根 `Cargo.toml` 的 `[workspace.package].version`。
- Rust workspace 成员通过 `version.workspace = true` 继承。
- `vrc-get-gui/package.json` 由 `cargo xtask release-prepare` 更新。
- `Cargo.lock` 和 `package-lock.json` 是生成文件。
- Updater JSON 由发布后的 updater workflow 从公开 Release assets 重新生成并验证，
  只能在验证通过后提交。
- `release-notes/ALCOMD3_$Version.updater-notes.json` 是应用内更新弹窗使用的简短多语言更新说明。

### 1. 确定发布输入

只使用一个 channel。

| Channel | Version example | GitHub Release type | Updater JSON |
| --- | --- | --- | --- |
| Stable | `2.0.1` | normal release | 网站仓库 `public/api/gui/tauri-updater.json` |
| Beta | `2.1.0-beta.1` | prerelease | 网站仓库 `public/api/gui/tauri-updater-beta.json` |

首次正式多平台发布是 `2.1.2-beta.1`；`2.1.2` 是首个采用同一合约的 stable release。其
manifest 已经公开，因此 stable catalog 使用三平台 manifest。Stable 2.1.1 继续作为不可变的
历史 GitHub Release 保留，官网不从其旧资产推导直链。Beta 继续作为独立选项展示，官网不
推荐任何发布通道。浏览器能够识别当前系统时，只着重显示相应的 stable 平台包卡片，Beta
平台包不着重显示。

在 PowerShell 中设置变量：

```powershell
$Version = "2.0.1"
$Channel = "stable"
```

Stable version 不能包含 prerelease metadata。Beta version 必须包含 prerelease metadata。

仓库前置配置：

- 把 `ALCOMD3_UPDATER_PRIVATE_KEY`、`ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD` 和
  `ALCOMD3_WEBSITE_DEPLOY_KEY` 存为 repository Actions Secrets；
- 保持 Cloudflare Pages 对配置的网站仓库 `main` 分支的 production 自动部署开启。

macOS 发布只支持 ad-hoc 签名，不需要 Apple 账号或 Apple Secrets。签名命令不提供证书身份或
公证选项。平台特定的构建、签名、安装与更新机制只属于技术合约。Release notes、updater notes
和官网对所有已发布平台使用同一原则：不单纯因为这些机制增加平台专属披露、警告、操作说明或
帮助链接。只有本版本确有某个平台的用户可见变化时才点名说明。

每个 updater payload 和浏览器下载资产都在 `alcomd3.config.json` 中声明 `updateMode`：可应用内
更新的资产使用 `self-updater`，Linux DEB 使用 `no-self-updater`。构建命令直接使用该值，
source-bound shard 与统一 manifest 也会保留每项资产的模式。


受保护的 `release-signing` Environment 是可选加固项，不是当前前置条件。如果以后进入
多人维护并需要独立审批边界，再把同名 Secrets 迁移到该 Environment，同时给 workflow
增加 environment 绑定并配置 required reviewer。
在此之前，应严格控制 `main` 和 Actions workflow 的修改权限，因为 repository Secrets
本身不提供独立审批边界。

### 2. 从 clean state 开始

```powershell
git checkout main
git pull --ff-only
git status --short
gh auth status --hostname github.com
```

预期结果：

- `git status --short` 没有输出。
- GitHub CLI 已对 `ALCOMD3/ALCOMD3` 完成认证。

如果 worktree 不干净，停止。

### 3. 准备 source release files

```powershell
cargo xtask release-prepare --version $Version --channel $Channel
```

这个命令会：

- 更新 `Cargo.toml` workspace version；
- 刷新 `Cargo.lock` workspace package versions；
- 无 tag 更新 GUI 和 website 的 npm versions；
- 刷新 npm lockfiles；
- 如果不存在则创建 `release-notes/ALCOMD3_$Version.md`；
- 输出 `git status --short`。

现在编辑 release notes 文件，删除所有 placeholder text。

Release notes 必须使用正确的比对基准：

- Stable release 只和上一个 stable release 比对。
- Beta release 和紧邻的上一个 release 比对，不区分 stable 或 beta。

Release notes 还必须使用统一的多语言结构。标题精确为 `# ALCOMD3 v$Version`，随后按顺序
使用 `## English`、`## 日本語`、`## 中文`。每种语言先写一段摘要，再严格保留四个固定的
三级分类，顺序依次为应用更新、网站更新、安装与升级、兼容性与安全。对应标题必须分别为
`Application updates` / `アプリの更新` / `应用更新`、`Website updates` /
`Web サイトの更新` / `网站更新`、`Installation and upgrade` /
`インストールとアップグレード` / `安装与升级`、`Compatibility and security` /
`互換性とセキュリティ` / `兼容性与安全`。不得省略、重排、重命名或增加版本专属的三级
标题。每个分类必须包含非空项目符号；没有用户可见变化时仍保留分类，并用对应语言写明
本版本没有此类用户可见变化。禁止四级标题、fenced code block、缩进的 ATX 标题和缩进的
顶层项目符号。不要用任一平台的例行披露填充固定结构。`release-validate` 会强制校验固定
标题和结构；三种语言的项目符号语义和顺序仍由发布审查人工确认。
`2.1.3-beta.2` 及更早的已发布说明保留历史标题，不得为满足新格式改写；固定四类从后续版本
开始强制执行。

同时创建或更新 `release-notes/ALCOMD3_$Version.updater-notes.json`。
这个文件是应用内更新弹窗的简短多语言摘要，不是完整 GitHub Release notes。
它必须是 JSON object，key 只能是 `en`、`de`、`fr`、`ja`、`ko`、`zh_hans`、`zh_hant`，
value 必须是非空字符串。正常发布必须填写全部 7 个 key。缺失语言仍可为兼容和故障恢复
回退到生成的 `notes` 字段，但不应作为正常发布准备结果。

提交并推送 source release commit：

```powershell
git add Cargo.toml Cargo.lock
git add vrc-get-gui/package.json vrc-get-gui/package-lock.json
git add "release-notes/ALCOMD3_$Version.md"
git add "release-notes/ALCOMD3_$Version.updater-notes.json"
git status --short
git commit -m "release: prepare ALCOMD3 $Version"
git push origin main
```

这个提交是 GitHub Release tag 应该指向的 source state。它不能包含 generated installer、
`target/`、`artifacts/` 或 updater JSON。

### 4. 运行 Draft 构建 workflow

在 GitHub Actions 中手动运行 **Build release draft**，也可以使用：

```powershell
gh workflow run release-draft.yml --repo ALCOMD3/ALCOMD3 `
    -f version=$Version `
    -f channel=$Channel `
    -f replace_existing_draft=false
```

只有同一版本已存在、且明确需要替换其资产的 Draft 才使用
`replace_existing_draft=true`。Workflow 会拒绝覆盖已经发布的 Release 或 channel 不匹配的
Draft。如果 Draft 创建后 prepared source commit 发生变化，例如修正 release notes，显式替换
会把 Draft 重新指向本次 dispatch 的 source commit，并替换由该 commit 构建的全部 10 项资产。

Workflow 会固定 checkout 触发时记录的 `github.sha`，记录该 source commit，并在正式构建前
先校验准备好的 source state，再运行 `release-preflight`。首次创建要求目标 tag 不存在；显式
替换要求目标是 channel 匹配且没有额外资产的现有 Draft。认证、网络或 API 错误不会被当成
Release 不存在。

Workflow DAG 是 `preflight` → 三个平台构建 shard → 可信组装 → Draft。Windows x64、
macOS arm64、Linux x64 在各自原生 runner 上从同一个固定 source commit 构建。每次
`release-build --platform ... --github-actions-release` 生成一个尚未带 updater Minisign
签名的平台 shard 及绑定 source SHA 的 shard manifest。Windows shard 在上传前通过精确
Release tag endpoint 解析 `legacyWindowsMigrationReleaseTag`，验证 setup ZIP，安装固定
stable 基线，再用复制到 `artifacts/release/v$Version/` 的 setup EXE 升级。基线阶段只验证
历史版本已有合约；当前 AppId、AUMID、文件关联、快捷方式、迁移清理、启动与卸载断言全部在
升级后执行。macOS shard 还会把必需的 ad-hoc 签名状态写入 manifest。该路径使用身份 `-`
逐层签署嵌套程序、app 和 DMG，不请求安全时间戳或公证，并在上传前确认最终签名均报告
`Signature=adhoc`。

三个 shard 全部成功后，`release-assemble` 验证其 source SHA、允许列表和 digest；随后只
解密一次 updater 密钥，为 Windows installer、macOS app updater archive 和 Linux AppImage
updater archive 生成并验证三份 Minisign 签名，原子生成统一 release manifest，之后才允许
`release-publish` 创建 Draft。npm 下载缓存以 lockfile 为 key；Rust 构建输出、签名材料和
release assets 不进入缓存。Rust toolchain 和全部 release asset pattern 由
`alcomd3.config.json` 集中管理。Updater 签名材料来自：

- `ALCOMD3_UPDATER_PRIVATE_KEY`
- `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`

Runner 执行固定版本 Inno Setup installer 前，会先按其 GitHub Release asset digest
验证 SHA-256。

Workflow 不会发布 Draft。短期 shard artifact 只包含组装 job 所需的 release assets 和
source-bound shard manifests。Checkout 不持久化凭据，签名 Secrets 只暴露给所需 job 或
step。Draft 创建成功后，Job Summary 会记录 Release URL、source 和 target commit、
Draft/prerelease 状态及全部资产 digest。

### 5. 检查并人工发布 Draft

发布前确认：

- tag 是 `v$Version`，并指向 workflow 实际构建的 source release commit；
- title 是 `Version $Version`；
- stable 是普通 Release，beta 是 prerelease；
- release notes 正确；
- 以下 10 项资产恰好齐全：
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
- 不存在其他额外上传资产；

所有新公开文件名都显式包含平台和架构。Stable 2.1.1 继续作为不可变的历史 GitHub Release
保留；其旧 Windows 资产名不属于新 catalog，也不会被官网转换为直接下载链接或兼容别名。

在 GitHub UI 中人工发布 Draft。这是发布门，默认构建 workflow 不会绕过它。

### 6. 等待 updater workflow 和 Cloudflare Pages

发布 Draft 会触发 **Publish updater metadata**。它会在全新 runner 上：

如需故障恢复或复验，可使用已经发布的 Release 标签手动启动同一工作流。工作流会
直接从 GitHub 解析 Release 名称、通道和源提交。

- 从已发布 Release 推导版本和 stable/beta channel；
- 下载精确 10 项公开资产，并拒绝任何缺失或额外资产；
- 要求 Release target、tag commit、root/GUI 版本完全一致；
- 验证三个 updater payload 及其 Minisign 签名，确保每份签名绑定精确文件名和已认证的
  `release` 用途；
- 从 Release tag 读取多语言 sidecar，以 Release `publishedAt` 作为固定 `pub_date`，原子
  重建当前 channel 的 updater JSON，包含 Windows x64、macOS arm64 和 Linux x64；
- 替换 metadata 文件前，验证每个平台 entry 的版本、精确 URL、签名文件名、签名和内嵌公钥；
- 拒绝 updater 版本回退；同版本重跑仅允许生成逐字节相同的 metadata；
- 克隆配置的网站仓库，把当前 channel JSON 直接写入配置的仓库路径；
- 只在网站仓库中提交该 JSON，并推送网站仓库 `main`；
- 等待公开 updater endpoint 返回同一版本、三个精确平台 URL 和签名。

公开 endpoint 通过后，Job Summary 会记录 version、channel、Release 与 source commit、
metadata 是否创建新提交、最终网站仓库 commit 和已验证的 endpoint。

Updater workflow 不接收私钥。Checkout 不持久化凭据，`GH_TOKEN` 只在必要 step 可见，
且会从非 GitHub cargo、git 子进程移除。它对网站仓库 `main` 的 push 会触发该仓库连接的
Cloudflare Pages production deployment，因此应保持网站仓库 `main` 的自动 production
branch deployment 开启。

### 7. 确认完成与失败恢复

只有 **Publish updater metadata** 和公开 endpoint 检查都通过，发布才算完成。
Cloudflare Pages 尚在部署时，workflow 会在有限时间内重试。超时不会回滚已经验证并
推送的 metadata commit；应检查 Pages deployment，修复部署问题后重新运行 endpoint
workflow。同一 Release 会重建出逐字节相同的 JSON，重跑会跳过无变化的 commit/push
并继续检查 endpoint。

### 本地构建与例外手动发布

本机不是计划内的发布编排器。普通本地 `release-build` 每次只构建 `--platform` 指定的一个
平台，在 `artifacts/local-test/v$Version/` 下生成**未签名** shard；它不会对 updater
payload 做 Minisign 签名，macOS 普通本地构建也不会进行证书签名、公证或 staple：

```powershell
cargo xtask release-build --version $Version --channel $Channel --platform windows-x86_64
```

另外两个 platform key 是 `darwin-aarch64` 和 `linux-x86_64`。这些 shard 可用于检查打包
结果，但不能直接交给正式资产发布器。

如果维护者明确需要从本机发布，必须从 clean、与 `origin/main` 完全一致的 `main` 开始，
并显式构建正式用途产物：

```powershell
cargo xtask release-validate --version $Version --channel $Channel
cargo xtask release-build --version $Version --channel $Channel --platform windows-x86_64 --release-artifacts
cargo xtask release-build --version $Version --channel $Channel --platform darwin-aarch64 --release-artifacts
cargo xtask release-build --version $Version --channel $Channel --platform linux-x86_64 --release-artifacts
$SourceSha = git rev-parse HEAD
cargo xtask release-assemble --version $Version --channel $Channel --source-sha $SourceSha
cargo xtask release-publish --version $Version --channel $Channel
```

每次显式 build 生成的平台 shard 都还没有 updater Minisign 签名。必须运行
`release-assemble`：它会验证三个 source-bound shard manifests、用 `release` 用途签名并
验证三个 updater payload、检查精准 10 文件允许列表，并写入统一的、被忽略的
`artifacts/release-state/v$Version.json`。`release-publish` 会在上传前重新核对这份清单。
本地 macOS release shard 必须在 macOS 上使用与 Actions 相同的 ad-hoc 签名路径生成，使 app、
DMG 和 shard manifest 中记录的状态在组装前保持一致。仅在替换兼容 Draft 时追加
`--replace-assets`；显式替换会把 Draft 重新指向已验证的构建 source，并在上传后再次校验。
本地发布仍必须得到明确授权，只有用户授权公开 Release 时才追加 `--publish`。无论 Draft
来自 Actions 还是本地，公开后都会触发同一个 updater workflow。
Updater metadata 发布仅允许在 Actions 中运行，避免绕过 attestation、source 绑定、版本
单调性、串行队列和公开 endpoint 检查。
Windows 身份迁移期间，例外的本地发布也不能绕过 GitHub-hosted 正式安装器升级 smoke；
无法用该门禁证明准确的本地 setup EXE 时，应改用 Draft workflow。

不得提交 `target/`、`artifacts/`、`.env`，也不得提交匹配 Release assets 尚未公开时
生成的 updater JSON。

### Failure rules

以下情况必须停止发布：

- release notes 仍包含 placeholder text 或不符合统一的多语言结构；
- updater notes sidecar 在需要时缺失，或包含非法 JSON、不支持的语言 key、空字符串值；
- validation 失败；
- source-bound Windows 正式安装器升级 smoke 失败、取消、未执行，或测试的不是 Windows
  release shard 中的 setup EXE；
- signing variables 或 signing key loader 缺失；
- macOS shard 未绑定共享配置要求的 ad-hoc 签名；
- app、嵌套程序、updater 压缩包内容或 DMG 的 ad-hoc 签名、严格验证或
  `Signature=adhoc` 检查失败；
- artifact 缺失；
- updater JSON verification 失败；
- release notes 使用了错误的比对基准；
- GitHub Release title 不是 `Version $Version`；
- GitHub Release assets 缺失或命名错误；
- stable/beta flags 错误；
- Release target SHA、tag commit 或 source 版本不一致；
- release 签名被标记为 `local-test`，或绑定了其他文件名；
- updater metadata 会使所选 channel 版本回退；
- 要求替换 Draft 时发现目标 Release 已经发布；
- 首次创建 Draft 时目标 Release 已存在，或替换时找不到兼容 Draft、发现额外资产；
- updater 私钥无法解密，或与 GUI 内嵌公钥不匹配；
- Cloudflare Pages 对网站仓库 `main` 的 production 自动部署被关闭；
- GitHub Release assets 公开前试图部署 website updater JSON。
