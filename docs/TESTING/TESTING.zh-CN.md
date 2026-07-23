# ALCOMD3 测试链路

本文档定义持续维护的测试链路。正式发布步骤仍单独遵循
[RELEASE.zh-CN.md](../RELEASE/RELEASE.zh-CN.md)。

## 测试分层

| 层级 | 命令或 workflow | 主要覆盖 |
| --- | --- | --- |
| Rust | `cargo test --workspace --exclude windows-installer-wrapper --locked` | VPM、MCP stdio/IPC、updater 与发布工具 |
| GUI 单元测试 | 在 `vrc-get-gui` 运行 `npm test` | Tauri command 序列化、事件、取消、错误与导航状态 |
| Website 浏览器测试 | 在 `website` 运行 `npm test` | 本地静态预览、多语言、导航、下载、updater metadata 与移动布局 |
| 桌面 E2E | `Full-chain desktop smoke` | 在 Windows x64、macOS arm64 和 Linux x64 上真实启动 Tauri，验证首次配置、隔离项目发现、重启持久化和 MCP 默认禁用边界 |
| macOS bundles | `Full-chain desktop smoke` | `.app`、updater 压缩包和 DMG 结构、arm64 二进制、显式 ad-hoc 签名、复制安装后的启动与清理 |
| Linux packages | `Full-chain desktop smoke` | AppImage/updater 压缩包一致性、DEB metadata、全新安装、启动与卸载 |
| Windows 安装/升级 | `Full-chain desktop smoke` | 旧 stable 安装、旧共享 AppId/两种历史程序名/旧快捷方式清理、桌面快捷方式选择保留、新快捷方式目标与新 AppId 注册、用户数据保留、启动与卸载 |
| 发布 CLI 计划 | 使用 `--dry-run` 的 `release-build`、`release-assemble`、`release-publish` 和 `release-preflight` | 只检查命令构造，不签名、不发布、不写外部状态 |
| 已发布 updater | 定时或手动运行的 `Full-chain desktop smoke`，仅 Windows job | 比对线上 stable/beta manifest、下载安装包、校验精确 URL/文件绑定和 Minisign 签名；stable 还强制校验认证的 `release` purpose |
| 正式发布链 | `Build release draft` 和 `Publish updater metadata` | 绑定 source 的原生构建 shard、准确 Windows 安装器升级 smoke、精准 10 资产允许列表、绑定配置的 macOS ad-hoc 签名、三份 updater Minisign 签名、全部 10 项资产 attestation、原子三平台 metadata、Website 构建和公开 endpoint |

`Continuous integration` 在 PR 和 `main` push 上执行 Rust、GUI 单元测试及
Website 浏览器测试。`Full-chain desktop smoke` 的三个平台 job 会在相关 PR、相关
`main` push、每周一北京时间 03:00 和手动触发时执行。线上 updater 校验是 Windows
job 内的独立步骤，只在定时和手动执行时运行；它会获取公开 manifest、与检出的
manifest 做语义比对，再校验公开响应指向的安装包。

## 本地验证

使用 `alcomd3.config.json` 指定的 Rust 版本和 Node.js 24。

```powershell
cargo fmt --all --check
cargo clippy --workspace --exclude windows-installer-wrapper --all-targets --locked -- -D clippy::correctness
cargo check --workspace --exclude windows-installer-wrapper --locked
cargo test --workspace --exclude windows-installer-wrapper --locked

Push-Location vrc-get-gui
npm.cmd ci
npm.cmd run check
npm.cmd run lint
npm.cmd test
npm.cmd run build
Pop-Location

Push-Location website
npm.cmd ci
npx.cmd playwright install chromium
npm.cmd run check
npm.cmd test
Pop-Location
```

真实 Windows 桌面测试需要先构建调试程序：

```powershell
cargo xtask build-alcom --target x86_64-pc-windows-msvc
Push-Location vrc-get-gui
npm.cmd run test:e2e:desktop
Pop-Location
```

调试版桌面 E2E 会把 `ALCOMD3_TEST_LOCAL_DATA_ROOT` 和
`ALCOMD3_MCP_ENDPOINT_FILE` 指向临时路径。数据根目录覆盖只在启用 debug
assertions 时编译，Release 构建会忽略它。测试会禁用调试构建的系统集成，并快照当前
用户完整的 `vcc://` 注册表树；如果应用改写它，即使测试失败，也会在 `finally` 中恢复。
外部传入的
数据根必须为空，且位于临时目录、runner temp 或工作区 `target` 下。Runner 会把暂停
启动的 WebdriverIO 放入本次专用的 Windows Job Object，关闭它时只停止本次测试创建的
完整进程树。包装脚本设置 15 分钟超时，驱动卡死时仍会进入清理流程。

macOS 和 Linux job 使用 `--desktop-e2e-webdriver` 构建调试版 GUI，并运行
`npm run test:e2e:desktop:unix`。该构建参数只在 dev profile 中启用 loopback
内嵌 WebDriver，非 dev profile 会直接拒绝，因此不会进入正式包。Linux 在 Xvfb 下
运行。Unix 包装脚本持有隔离的进程组，并执行与 Windows 包装脚本相同的 15 分钟超时和
结果完整性检查。

## 安装与发布边界

安装/升级 smoke 被强制限制在 GitHub-hosted 的一次性 Windows runner，因为安装器
使用正式 AppId 和文件关联。Workflow 构建不带 updater 签名的测试安装包，在旧 stable
版本上升级，验证旧共享 AppId 从 HKCU、HKLM 32 位和 HKLM 64 位视图消失、历史
`ALCOM.exe` 及旧桌面/开始菜单快捷方式被清理、旧桌面快捷方式选择得到恢复，且两个新
快捷方式均指向 `ALCOMD3.exe`；同时验证新 AppId 被注册，新快捷方式、模板 ProgID 和
`vcc://` 注册统一使用配置的 Windows AUMID，既有用户数据得到保留，且同名但指向无关
程序的快捷方式不会被删除。随后验证 ZIP 内安装包、启动应用、确认 `vcc://`
命令、确认 MCP 默认拒绝工具调用且 endpoint 端口不存在
额外的非 loopback 监听，最后执行卸载并确认新快捷方式也被移除。迁移窗口内，旧版本由
`alcomd3.config.json` 的 `legacyWindowsMigrationReleaseTag` 固定，避免新 AppId 首次发布后
测试基线自动前移而失去旧 AppId 覆盖。

macOS package job 使用原生 `macos-15` arm64 runner，验证 ad-hoc 签名的 `.app`、updater
压缩包和 DMG，并对嵌套程序及解压后的 updater app 运行严格签名检查。Linux package job
使用 Ubuntu 22.04 x64 作为 AppImage 兼容性基线，
验证 AppImage、updater 压缩包和 DEB。它先用 `self-updater` 模式构建 AppImage，再按共享发布
配置用 `no-self-updater` 模式重建 DEB。打包后启动测试会把 `HOME`、XDG 目录和 MCP
endpoint metadata 隔离到 `RUNNER_TEMP`，确认 GUI 持续运行，验证 loopback MCP 的
默认禁用与错误 token 拒绝边界，再完成清理。Package smoke 辅助程序会拒绝在非
GitHub-hosted 的一次性 macOS/Linux runner 上运行。

这些 macOS/Linux 包只属于 CI 测试产物：不上传 GitHub Release、不生成公开 updater 条目，
也不宣称覆盖旧版本升级。正式三平台资产只来自独立的 `Build release draft` DAG；其 macOS
app 和 DMG 使用同一套显式 ad-hoc 策略，未经 Apple 公证。Windows shard 会用复制到
source-bound release shard 的准确 setup EXE 重复升级 smoke，且历史基线阶段不会要求当前
安装器才引入的身份字段。Linux 构建与 full-chain 使用相同的两种配置更新模式。

发布 CLI dry-run 只是命令计划 smoke，不证明 updater 签名材料、干净且已同步的 `main` 或线上
GitHub Release 状态。该 workflow 不读取 updater 签名 Secret，不创建 GitHub Release，不发布 updater
metadata，不推送提交，也不部署 Website。真实签名、Draft 创建和线上 updater 校验仍由
现有发布 workflow 按发布手册把关。

Website 浏览器测试会拒绝意外外部 HTTP 请求；Rust 集成测试只使用临时目录和 loopback
监听。测试不得依赖或修改维护者现有的 ALCOMD3、VCC 或 ALCOM 数据。
