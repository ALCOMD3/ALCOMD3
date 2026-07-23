---
name: alcomd3-release
description: 完成或审计 ALCOMD3 的 stable/beta 三平台发布与 release notes，包括版本准备、发布说明格式、Windows/macOS/Linux 资产、配置驱动的 macOS 签名策略、updater 签名、GitHub Release、官网多平台下载和自动更新 metadata。处理版本发布、发布审计、Draft、updater metadata 或新增/检查 release-notes/ALCOMD3_*.md 时使用。
---

# ALCOMD3 三平台发布

## 何时使用

处理以下任一任务时使用本 skill，并先阅读仓库根 `AGENTS.md` 与
`docs/RELEASE.md`：

- 准备或发布 stable/beta 版本；
- 创建、替换、检查或公开 GitHub Release Draft；
- 修改正式发布 workflow、资产命名、签名、公证或 updater metadata；
- 验证官网与 GitHub Release 的多平台下载；
- 审计发布失败、公开 updater endpoint 或 Cloudflare Pages 上线状态。

明确的审计请求保持只读。明确的发布请求应端到端推进，只在缺少外部凭据、需要新的
授权或安全门失败时停止。若用户已经明确授权公开指定版本，不要在 Draft 门重复询问。

## 完成标准

一次实际发布只有在以下条件全部满足后才算完成：

1. source release commit 已从 clean、同步的 `main` 创建并推送；
2. `Build release draft` 的 Windows shard 已通过安装、启动和卸载 smoke；配置仓库存在固定
   迁移基线时，还必须完成身份迁移和升级 smoke；
3. `Build release draft` 的三个平台 shard 与统一汇总全部成功；
4. GitHub Release 精确包含 10 个正式资产，Draft 已按授权公开；
5. `Publish updater metadata` 已验证公开 Release 与全部资产；
6. 目标 channel 的 updater JSON 已原子更新为三个平台并推送 `main`；
7. Cloudflare Pages 已部署该 commit；
8. public updater endpoint 与仓库 JSON 字节语义一致；
9. 官网只展示实际已发布的下载链接，Windows、macOS 和 Linux 链接均可访问；
10. 三个平台的 updater payload 与浏览器安装包均符合共享配置声明的更新模式。

Draft 创建成功不等于发布完成。定时 `Full-chain desktop smoke` 的临时产物不是正式发布输入；
`Build release draft` 对准确 Windows 正式安装器执行的 smoke 是发布门禁；配置仓库存在固定
迁移基线时，该门禁还必须包含升级 smoke。

## 不变量

- ALCOMD3 使用自己的仓库、endpoint、更新公钥与签名材料。
- 不复用其他项目的 Release、secret、updater metadata 或资产。
- 不修改迁移后的 `tauriIdentifier`、`windowsAppId`、`windowsAumid`、安装目录、主程序名、协议名、数据路径
  或 `vrc-get` 兼容路径。当前身份重置已经将旧身份固定为 `legacyTauriIdentifier` 和
  `legacyWindowsAppId`；过渡期发布必须保留安装前的无条件清理，且只能在另行审查确认共用
  迁移窗口结束后与 `legacyWindowsMigrationReleaseTag` 固定测试基线一起移除，不能再次更换
  `tauriIdentifier` 或 `windowsAppId`。
- MCP 默认关闭，只使用本地 stdio bridge 与本机 IPC；发布工作不得引入网络监听。
- 已发布的 `release-notes/ALCOMD3_*.md` 与 updater notes 是历史记录，不为后续版本改写。
- updater JSON 只能在对应 GitHub Release 已公开且全部验证通过后更新。
- stable 只更新 stable JSON；beta 只更新 beta JSON。
- 平台固有技术差异只约束构建与验证，不派生平台专属的用户文案：
    - Windows 安装器升级参数的内容与顺序保持不变；
    - macOS 按 `alcomd3.config.json` 的 `macosAdHocSigning` 使用固定 identity `-`，不提供
      可配置证书身份、安全时间戳、公证或 staple；
    - Linux AppImage 启用 self-updater，DEB 使用关闭 self-updater 的独立构建。
- 三个平台的 updater payload 都必须通过 release-purpose Minisign 验证；平台原生签名或打包
  检查不能替代 updater 签名验证。
- release notes、updater notes 与官网对所有已发布平台使用同一文案原则：不因某个平台的
  打包、签名、安装或更新机制增加专属披露、警告、操作说明或帮助链接。只有本版本确有该
  平台的用户可见变化时才点名说明，并使用同一套发布说明结构。
- 官网根据 updater manifest 与 `alcomd3.config.json` 生成链接，不能猜测尚不存在的资产。

## 三平台资产合约

命名真值在 `alcomd3.config.json` 的 `releasePlatforms`。每个平台有一个 updater payload、
一个 `.sig` 和至少一个浏览器下载资产。新 Release 的精确白名单为：

| 平台 | 角色 | 文件名 |
| --- | --- | --- |
| Windows x64 | updater/installer | `ALCOMD3_{version}_windows_x86_64_setup.exe` |
| Windows x64 | updater signature | `ALCOMD3_{version}_windows_x86_64_setup.exe.sig` |
| Windows x64 | browser download | `ALCOMD3_{version}_windows_x86_64_setup.exe.zip` |
| macOS Apple Silicon | browser download | `ALCOMD3_{version}_macos_aarch64.dmg` |
| macOS Apple Silicon | updater | `ALCOMD3_{version}_macos_aarch64.app.tar.gz` |
| macOS Apple Silicon | updater signature | `ALCOMD3_{version}_macos_aarch64.app.tar.gz.sig` |
| Linux x86_64 | browser download | `ALCOMD3_{version}_linux_x86_64.AppImage` |
| Linux x86_64 | updater | `ALCOMD3_{version}_linux_x86_64.AppImage.tar.gz` |
| Linux x86_64 | updater signature | `ALCOMD3_{version}_linux_x86_64.AppImage.tar.gz.sig` |
| Linux amd64 | browser download | `ALCOMD3_{version}_linux_amd64.deb` |

Updater platform key 固定为：

- `windows-x86_64` → Windows setup EXE，保留配置中的 5 个静默升级参数；
- `darwin-aarch64` → `.app.tar.gz`；
- `linux-x86_64` → `.AppImage.tar.gz`。

每个 updater payload 与浏览器下载资产还必须显式声明 `updateMode`：可应用内更新的资产使用
`self-updater`，包管理器接管更新的 Linux DEB 使用 `no-self-updater`。构建命令必须由该字段
决定是否传入 `--no-self-updater`，并把模式写入 source-bound shard 与统一 manifest。

## 发布前凭据

GitHub 仓库必须配置：

- `ALCOMD3_UPDATER_PRIVATE_KEY`
- `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`

macOS ad-hoc 签名不使用 Apple 账号、证书或 Apple Secrets，签名命令与 workflow 也不提供
相应参数或环境变量入口。Updater 私钥仅进入统一汇总 job；macOS build job 不读取 updater
私钥。

## 1. 选择版本和 channel

```powershell
$Version = "2.2.0-beta.1"
$Channel = "beta"
```

- stable 版本不能含 prerelease metadata；
- beta 版本必须含 prerelease metadata；
- stable release notes 对比上一个 stable；
- beta release notes 对比紧邻的上一个版本（stable 或 beta）。

## Release notes 固定格式

`release-notes/ALCOMD3_$Version.md` 必须使用以下骨架；不得照抄任意历史版本的临时结构：

```markdown
# ALCOMD3 v$Version

## English

一段英文摘要。

### Application updates

- 应用变化；没有时写明本版本没有此类用户可见变化。

### Website updates

- 网站变化；没有时写明本版本没有此类用户可见变化。

### Installation and upgrade

- 安装与升级变化；没有时写明本版本没有此类用户可见变化。

### Compatibility and security

- 兼容性、安全或已知问题；没有时写明本版本没有此类用户可见变化。

## 日本語

与英文语义一致的日文结构。

### アプリの更新

- 与英文对应的应用变化或无变化说明。

### Web サイトの更新

- 与英文对应的网站变化或无变化说明。

### インストールとアップグレード

- 与英文对应的安装与升级变化或无变化说明。

### 互換性とセキュリティ

- 与英文对应的兼容性、安全或已知问题，或无变化说明。

## 中文

与英文语义一致的中文结构。

### 应用更新

- 应用变化；没有时写明本版本没有此类用户可见变化。

### 网站更新

- 网站变化；没有时写明本版本没有此类用户可见变化。

### 安装与升级

- 安装与升级变化；没有时写明本版本没有此类用户可见变化。

### 兼容性与安全

- 兼容性、安全或已知问题；没有时写明本版本没有此类用户可见变化。
```

严格遵守：

- H1 必须精确为 `# ALCOMD3 v$Version`；H2 只能按顺序为 `English`、`日本語`、`中文`。
- 每种语言先写一段摘要，再严格按上述顺序保留四个 H3 固定分类；不得省略、重排、重命名
  或增加版本专属的 H3。
- 三种语言的固定分类分别对应应用、网站、安装与升级、兼容性与安全。软件包、项目、仓库、
  备份和 MCP 等功能变化归入应用；官网、下载页和用户文档归入网站；安装器、平台包、更新
  通道和数据迁移归入安装与升级；VRChat/VPM 兼容性、数据安全、权限边界、已知问题和重要
  限制归入兼容性与安全。
- 每个 H3 必须包含非空项目符号。某类没有用户可见变化时仍保留该 H3，并用对应语言明确写明
  本版本没有此类用户可见变化。
- 禁止用 `Changes`、`Fixes` 或“软件包列表可靠性”等动态标题包裹或替代固定分类，也禁止
  使用 H4 增加子主题。
- ATX 标题与顶层项目符号必须从行首开始，不得用缩进改变 Markdown 解析语义。
- 禁止 fenced code block；只允许在段落或项目符号中使用 inline code。
- 只写与本版本实际变化相关的内容；不得为了填充固定章节重复平台通用说明。
- 不例行添加任何平台的签名、打包、安装或更新机制说明；任何平台都不比其他平台多一段。
- 不写比较基准、commit/PR 清单、CI、workflow、权限、Secret 或仅供维护者使用的实现记录。
- `updater-notes.json` 是独立的七语言短摘要，不复用上述 Markdown 层级。
- `cargo xtask release-validate` 必须通过；不得跳过或以人工检查替代格式校验。
- `release-validate` 强制四类标题、顺序和结构一致性；三种语言的项目符号语义是否一致仍由
  发布审查人工确认。
- `2.1.3-beta.2` 及更早的已发布说明保留原有历史标题；不得为满足新格式改写。固定四类从
  后续版本开始强制执行。

## 2. 准备 source release commit

从 clean、同步的 `main` 开始：

```powershell
git checkout main
git pull --ff-only
git status --short
cargo xtask release-prepare --version $Version --channel $Channel
```

编辑 `release-notes/ALCOMD3_$Version.md`，删除全部 placeholder。正常发布同时创建
`release-notes/ALCOMD3_$Version.updater-notes.json`，并填写以下 7 个非空语言键：

- `en`
- `de`
- `fr`
- `ja`
- `ko`
- `zh_hans`
- `zh_hant`

发布说明只包含用户可见内容、兼容性和已知问题，不混入纯 CI、权限、Secret 或内部维护
记录。运行完整验证，再提交并推送 source release commit：

```powershell
cargo xtask release-validate --version $Version --channel $Channel
git add Cargo.toml Cargo.lock
git add vrc-get-gui/package.json vrc-get-gui/package-lock.json
git add website/package.json website/package-lock.json
git add "release-notes/ALCOMD3_$Version.md"
git add "release-notes/ALCOMD3_$Version.updater-notes.json"
git diff --cached --check
git commit -m "release: prepare ALCOMD3 $Version"
git push origin main
```

不要提交 `target/`、`artifacts/`、`.env` 或提前生成的 updater JSON。

## 3. 创建三平台 Draft

```powershell
gh workflow run release-draft.yml --repo ALCOMD3/ALCOMD3 `
    -f version=$Version `
    -f channel=$Channel `
    -f replace_existing_draft=false
```

只有同版本存在兼容 Draft 且明确要替换时才设
`replace_existing_draft=true`。workflow 必须固定使用 dispatch 时的 `github.sha`，执行：

1. `preflight`：版本、notes、channel、GitHub Release 状态与 source SHA；
2. `build-windows`：生成 setup EXE 与 ZIP，验证 ZIP 内容，只从当前配置仓库解析
   `legacyWindowsMigrationReleaseTag`；存在该版本时升级到本次正式 EXE，不存在时执行纯安装，
   并完成安装、启动、AUMID/AppId/关联/快捷方式检查和卸载 smoke；
3. `build-macos`：按共享配置对嵌套二进制与 `.app` 做 ad-hoc 签名，生成 updater/DMG，
   再对 DMG 做 ad-hoc 签名；不请求公证或 staple；
4. `build-linux`：带 self-updater 的 AppImage/updater archive，以及关闭 self-updater 的 DEB；
5. 每个平台生成 schema v4 shard，绑定 version/channel/source SHA/target/macOS ad-hoc
   signing 状态，以及每项资产的角色、更新模式、hash 和 size；
6. `assemble-draft` 验证三个 shard，统一给三个 updater payload 做 release-purpose minisign，
   生成精确 10 资产 manifest，再创建或替换 Draft。

任何 shard 的 source SHA、target、hash、size、名称或白名单不一致都必须失败。汇总前不要让
平台 job 直接写 GitHub Release。

监控到所有 job 结束。macOS job 还必须通过：

- 不使用 `codesign --deep` 进行签名；
- 嵌套 helper、主程序和 app 逐层签名；
- 固定使用 identity `-` 和 hardened runtime，不使用 secure timestamp；
- `codesign --verify --strict`；
- app、嵌套 helper、主程序、解压后的 updater app 和 DMG 均报告 `Signature=adhoc`；
- 命令与 workflow 中不存在可配置签名身份、公证选项或 Apple Secret 注入。

Windows job 还必须确认：

- 迁移基线只从当前配置仓库解析；新仓库不存在该 tag 时明确记录并跳过升级阶段；
- 存在迁移基线时，旧版阶段只验证旧版已有合约，不要求当前版本新增的 AUMID；
- 当前阶段测试的是 `artifacts/release/v$Version/` 中将进入 Draft 的 setup EXE；
- 存在迁移基线时，升级后当前 AppId/AUMID、模板 ProgID、`vcc://`、快捷方式和用户数据迁移
  全部通过；无基线时，当前安装器的对应安装合约全部通过；
- 卸载完成且 smoke 诊断日志在失败时已上传。

## 4. 检查并公开 Draft

确认：

- tag 为 `v$Version` 且目标是 source release commit；
- title 为 `Version $Version`；
- stable/beta flag 正确；
- release notes 完整；
- 10 个资产与上表完全相等，无缺失或额外项；
- Windows/macOS/Linux 文件名都含平台和架构；
- Draft workflow 的 source SHA 与三个 shard manifest 一致。

默认 workflow 不公开 Draft。只有得到明确公开授权后才发布。若用户已经要求“发布指定版本”
且未撤回，该指令就是公开授权。

## 5. 公开后自动发布 updater metadata

公开 Draft 会触发 `.github/workflows/release-updater.yml`。该 workflow 不读取私钥，必须：

1. 验证 Release title、channel、target SHA、tag commit 与版本文件；
2. 下载精确 10 个资产，并拒绝任何缺失或额外资产；
3. 用 GUI 内嵌公钥验证三个 updater payload 的 minisign、文件名绑定和
   `purpose:release`；
4. 从 tag 读取 updater notes，以 Release `publishedAt` 作为确定的 `pub_date`；
5. 一次性生成包含三个 platform key 的目标 channel JSON；
6. 拒绝版本回退；同版本重跑必须生成字节一致的 JSON；
7. 运行官网 check/build；
8. 只提交目标 channel updater JSON，并推送 `main`；
9. 等待 Cloudflare Pages 部署，轮询 public endpoint，直到与生成 JSON 完全一致。

不能在 Windows 资产成功但 macOS/Linux 失败时发布部分 metadata。三个平台必须原子上线。

## 6. 官网与用户路径验收

官网构建期从 stable/beta manifest 和共享配置生成下载卡片，不为发布通道添加推荐标记；浏览器
识别到当前系统时，只着重显示对应的 stable 平台卡片，Beta 平台卡片不着重显示。验收：

- 无 JavaScript 时，已发布平台仍有真实下载链接；
- Windows 指向 ZIP，macOS 指向 DMG，Linux 提供 AppImage 和 DEB；
- 官网和 GitHub Release notes 不因任一平台的打包、签名、安装或更新机制增加专属披露、
  警告、操作说明或帮助链接；
- 未出现在 manifest 的平台显示不可用，不生成猜测 URL；
- 不兼容新资产合约的 legacy stable（包括 2.1.1）只提供 GitHub Release 页面 fallback，
  不推导旧资产直链或增加兼容别名；目标 stable manifest 公开后必须切换为三平台直链；
- GitHub Release 页面与四个浏览器下载资产返回成功；
- updater JSON 中三个 URL 分别指向对应 updater payload，而不是浏览器包装资产；
- 每个浏览器安装包和 updater payload 都遵循共享配置声明的更新模式。

## 本地构建与 dry-run

`release-build` 现在按平台生成未签 updater 的 build shard。普通本地执行不是完整正式发布：

```powershell
cargo xtask release-build --version $Version --channel $Channel `
    --platform windows-x86_64
```

异常情况下需要本地组装正式资产时，必须在对应操作系统分别执行三个
`--release-artifacts` build，把三个 shard 与资产合并到同一 `artifacts/`，再运行：

```powershell
cargo xtask release-assemble --version $Version --channel $Channel `
    --source-sha $SourceSha
cargo xtask release-publish --version $Version --channel $Channel
```

本地 `release-publish --publish` 仍需要明确公开授权，并且只能从 clean、与
`origin/main` 完全一致的 `main` 执行。

只检查命令计划时使用：

```powershell
cargo xtask release-build --version $Version --channel $Channel `
    --platform windows-x86_64 --dry-run
cargo xtask release-assemble --version $Version --channel $Channel `
    --source-sha 0000000000000000000000000000000000000000 --dry-run
cargo xtask release-publish --version $Version --channel $Channel --dry-run
```

Dry-run 不证明 updater 私钥、实际 ad-hoc 签名、GitHub 状态或公开 endpoint。
身份迁移窗口内，配置仓库存在固定迁移基线时，例外的本地发布也不能绕过 GitHub-hosted
Windows 正式安装器升级 smoke；无法证明准确正式安装器通过适用门禁时，停止本地发布并改用
默认 Draft workflow。

## 必须验证

按影响范围至少运行：

```powershell
cargo fmt --all --check
cargo test -p xtask
cargo check --workspace --exclude windows-installer-wrapper --locked
cargo test --workspace --exclude windows-installer-wrapper --locked

Push-Location vrc-get-gui
npm.cmd run check
npm.cmd run lint
npm.cmd test
npm.cmd run build
Pop-Location

Push-Location website
npm.cmd run check
npm.cmd test
Pop-Location
```

修改 workflow 后还要让 PR 的 `Continuous integration` 与三平台
`Full-chain desktop smoke` 全部通过。当前真实 ad-hoc 签名链由 PR 的 macOS full-chain job
证明；正式 source-bound shard 与 Draft 汇总仍只能由 `release-draft.yml` 的 macOS/汇总
job 证明，正式 Windows 安装器升级链只能由 `release-draft.yml` 的 Windows job 证明。

## 停止条件

遇到以下任一条件停止发布并说明准确阻塞项：

- release notes/updater notes 缺失、含 placeholder、结构不符合固定格式或比较基准错误；
- worktree 不干净、source commit 与 `origin/main` 不一致；
- Windows 正式安装器 smoke 失败、取消、未执行，或测试的不是本次 source-bound shard 中的
  setup EXE；配置仓库存在固定迁移基线时，升级 smoke 同样不得失败、取消或省略；
- updater 签名凭据缺失、为空、无法解密或与公钥不匹配；
- macOS shard 未绑定共享配置要求的 ad-hoc signing；
- identity 不是 `-`，出现可配置签名身份、secure timestamp/公证参数，或 app、嵌套程序、updater
  app、DMG 的 ad-hoc 严格验证失败；
- 三个 shard 不同源、资产缺失/额外/篡改或 manifest 不匹配；
- GitHub Release title、channel、tag、target SHA 或 10 资产白名单不一致；
- updater signature、文件名绑定、purpose、URL 或平台 key 不一致；
- metadata 会回退版本或同版本重建不一致；
- 官网会生成不存在的资产 URL；
- 发布需要的外部权限或公开授权尚未取得。

公开 Release 之后，Cloudflare Pages 延迟或 endpoint 暂未更新不是回滚理由。修复部署问题并
重跑 updater workflow；不要改写已公开 Release 资产。

## 最终报告

报告至少包括：

- version/channel、source commit、tag 与 Release URL；
- 三个平台 build job、Windows 固定基线可用性与适用的正式安装器 smoke、各平台按共享配置
  完成的打包/签名/更新模式验证、统一汇总结果；
- 10 资产清单与 manifest/attestation 结果；
- updater metadata commit、目标 JSON 与三平台签名验证；
- Cloudflare Pages 与 public endpoint；
- 官网 Windows/macOS/Linux 下载链接；
- 未覆盖的真实平台测试或仍需人工检查的风险。
