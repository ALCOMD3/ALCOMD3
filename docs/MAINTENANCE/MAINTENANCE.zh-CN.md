# ALCOMD3 maintenance notes

语言: [English](../MAINTENANCE.md) | [日本語](MAINTENANCE.ja.md) | 简体中文

这些笔记记录 ALCOMD3 当前项目行为、兼容性边界和维护决策。修改 ALCOMD3 代码或从任何外部来源选择性引入修复时使用。

### 品牌

ALCOMD3 使用自己的品牌，同时保留既有安装所需的兼容性点。

- 产品和可见应用名：`ALCOMD3`。
- 窗口标题：`ALCOMD3`。
- Windows 安装器显示名称：`ALCOMD3`。
- Windows 快捷方式：`ALCOMD3`。
- Windows GUI 进程、安装器创建的快捷方式、`.alcomtemplate` ProgID 和 `vcc://` 协议注册
  统一使用稳定的显式 AUMID `CQMHV.ALCOMD3`。该值不含版本号，升级后仍保持同一个 Shell 身份。
- Windows setup 输出名：`ALCOMD3_{version}_windows_x86_64_setup.exe`。
- 已安装主程序：`ALCOMD3.exe`。
- macOS application bundle：`ALCOMD3.app`。
- Linux binary、desktop file、icon name 和 package metadata 使用 `alcomd3`。
- 公开资产名显式包含平台和架构。macOS 下载名为
  `ALCOMD3_{version}_macos_aarch64.dmg`；Linux 下载名为
  `ALCOMD3_{version}_linux_x86_64.AppImage` 和
  `ALCOMD3_{version}_linux_amd64.deb`。
- Windows 安装器使用 ALCOMD3 专属的新 AppId。过渡期安装器会无条件卸载旧共享 AppId
  下的 Inno Setup 安装，无论主程序名是 `ALCOM.exe` 还是 `ALCOMD3.exe`；确认旧 AppId
  在 HKCU、HKLM 32 位和 HKLM 64 位视图均无残留后，才注册新 AppId，避免并排出现两个
  ALCOMD3。
- 这项身份迁移会清理旧 AppId 对应的安装记录、已知程序文件，以及桌面和开始菜单中的
  已知 `ALCOM`/`ALCOMD3` 快捷方式。旧安装存在桌面快捷方式时，新安装器默认选中新的
  ALCOMD3 桌面快捷方式，同时仍尊重交互安装中用户本次作出的选择；不会清理 legacy
  ALCOM NSIS 记录、无关快捷方式或 ALCOMD3 用户数据。
- 当没有明确用户选择时，`vcc://` URL 快捷方式关联默认启用。
- 通过 deep link 添加存储库时，必须先在应用内确认，再开始下载存储库 metadata。

重要文件：

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

### 共享配置

`alcomd3.config.json` 是 ALCOMD3 产品身份和发布 metadata 的共享来源。它管理产品名、
包名、GUI 二进制名、MCP 二进制名、发布者、官网、GitHub repository、Windows AUMID、
当前及过渡期旧 Windows AppId，以及过渡期固定迁移基线 Release tag、
updater manifest 路径、三平台发布 catalog（target、bundle type、updater payload、下载资产
和文件名模式）、`.alcomtemplate` 关联 metadata、描述和 copyright 文本。

部分值仍保留在外部工具模板中，因为这些工具会直接消费自己的文件格式：

- `vrc-get-gui/Tauri.toml`
- `vrc-get-gui/bundle/windows-setup.iss`
- `vrc-get-gui/bundle/alcomd3.desktop`
- `vrc-get-gui/bundle/deb-control`
- `website/src/data/site.config.mjs`

这些文件仍保留原位置。`cargo test -p xtask alcomd3_identity` 会校验它们是否与
`alcomd3.config.json` 一致，从而在不新增生成步骤的情况下发现模板漂移。`repositories.txt`
是 repository list 格式流程的示例/导入数据，不是运行时共享配置。

### 数据目录和外部应用导入

ALCOMD3 默认拥有自己的运行数据。

- 默认本地数据目录为平台 local data root 下的 `ALCOMD3`，例如 Windows 上的
  `%LOCALAPPDATA%\ALCOMD3`。
- 默认项目和备份目录位于用户 Documents 文件夹下：`ALCOMD3/Projects` 和
  `ALCOMD3/Backups`。
- ALCOMD3 普通启动时不得自动迁移、移动或导入 VCC 或 legacy ALCOM 数据。
- 安装 ALCOMD3，或从 2.0.0 及以前更新到 2.1.0 及以后时，在用户显式从外部
  VCC 或 legacy ALCOM 安装导入数据之前，应表现为全新的 ALCOMD3 数据根安装。
- 仅当用户在首次设置或之后的设置界面明确导入时，才可以读取 VCC 或 legacy ALCOM
  数据。
- 外部应用导入会把 VPM settings、LiteDB 项目/Unity 数据、仓库缓存、vrc-get
  settings 和模板数据复制到 ALCOMD3 数据目录。
- 仓库索引缓存位于 `Repos/`。已下载的软件包 zip 缓存位于 `PackageCache/`；
  新下载不得写入 `Repos/`。
- 新的 ALCOMD3 专属文件应使用 `config/`、`state/`、`templates/`、
  `activity-logs/`、`technical-logs/` 等语义化顶层目录，不再在 `vrc-get/`
  下新增数据。
- 顶层 `settings.json` 和 `vcc.liteDb` 继续作为 VPM/VCC 数据格式兼容文件保留。
- 新的软件包缓存文件使用 `alcomd3-` 前缀。legacy `vrc-get-` 软件包缓存仅作为手动导入后的
  缓存兼容数据读取、清理。
- legacy `.alcomtemplate` 文件会作为 ALCOMD3 模板文件导入。legacy VCC 目录模板会被
  转换为 ALCOMD3 `.alcomtemplate` project archive 文件，并保存到 `templates/`。
- 外部应用导入会把导入的仓库缓存路径改写到 ALCOMD3 数据目录，并保留 ALCOMD3
  自己的默认项目和备份路径；legacy package zip 缓存会拆分到 `PackageCache/`。
- 外部应用导入不得复制旧 MCP endpoint metadata 或旧日志目录。

当前 ALCOMD3 数据根结构：

| 路径 | 存放内容和作用 |
| --- | --- |
| `settings.json` | VPM/VCC 兼容的主环境设置，包括项目记录、Unity 路径、用户包目录、默认路径和默认仓库。 |
| `vcc.liteDb` | 使用 VCC 兼容数据格式保存的 LiteDB 项目和 Unity 元数据。 |
| `config/gui-config.json` | GUI 偏好，例如语言、设置流程进度、布局、备份格式、更新频道和 URL 协议偏好。 |
| `config/theme-config.json` | 主题显示模式、配色方案和已保存主题色。 |
| `config/repository-settings.json` | ALCOMD3/vrc-get 仓库行为和包管理设置。 |
| `state/vcc-settings-backup.json` | ALCOMD3 自有的 VPM settings 备份/回退快照，不是 legacy 来源路径。 |
| `Repos/*.json` | 仓库索引缓存文件。 |
| `PackageCache/<package-id>/alcomd3-*.zip` | 已下载的软件包 zip 缓存文件。 |
| `PackageCache/<package-id>/alcomd3-*.zip.sha256` | 已下载软件包 zip 缓存文件的校验和。 |
| `templates/*.alcomtemplate` | 自定义和导入的 ALCOMD3 项目模板。 |
| `activity-logs/*.jsonl` | 日志页面展示的操作活动历史。 |
| `technical-logs/alcomd3-*.log` | 技术应用日志。legacy `vrc-get-` 日志文件名在此目录内仍可读取。 |
| `mcp/endpoint.json` | 本地 stdio MCP bridge endpoint 的运行时 metadata；由当前安装生成，绝不从 legacy 数据根导入。 |
| `Documents/ALCOMD3/Projects/` | 本地数据根之外的默认项目创建目录。 |
| `Documents/ALCOMD3/Backups/` | 本地数据根之外的默认项目备份目录。 |

`vrc-get/*` 不是 ALCOMD3 2.1.0 及以后新的运行时写入位置。只有用户显式开始手动导入时，
它才作为外部应用导入来源使用。

### 启动和窗口显示

保留这些行为：

- 创建主窗口时应用保存的初始窗口大小和最大化状态。
- 主窗口以隐藏状态启动，在前端调用 frontend-ready command 后由后端显示。
- `index.html` 保持静态首帧背景色，避免 CSS 和 JavaScript 加载前出现纯白闪烁。
- 正常 UI 启动前拒绝不支持的非 ASCII 主机名。

### Material Design 3 UI

保留这些行为：

- Material Theme 入口显示在侧边导航中。
- 侧边导航可显示可选 BOOTH、VRChatAvatarLearn 和版本按钮，可通过 `hide_sidebar_links` 隐藏。
- 版本按钮显示 `version: v{actual_version}` 并复制 `v{actual_version}`。
- Toast 使用圆角 MD3 样式、MD3 语义进度颜色和应用基础背景色。
- 重要项目/包操作使用 ALCOMD3 emphasis button 样式。
- setup 和 settings 文案中的用户可见文本使用 `ALCOMD3`。
- `vrc-get-gui/locales/*.json5` 中每个支持的 locale 都必须覆盖 `en.json5`
  的所有 translation key。`npm run check` 会运行 `scripts/check-locales.mjs`，
  并在缺少 locale key 时失败。

重要区域：

- `vrc-get-gui/app`
- `vrc-get-gui/components`
- `vrc-get-gui/app/globals.css`
- `vrc-get-gui/lib/material-theme.ts`
- `vrc-get-gui/locales/*.json5`
- `vrc-get-gui/src/config.rs`

### 贡献者数据

应用和网站共享 `generated/alcomd3-contributors.json` 作为构建时贡献者快照。
两个前端构建命令都会在打包前运行 `scripts/sync-contributors.mjs`。该脚本读取
GitHub 在 ALCOMD3 仓库首页呈现的同一个 `contributors_list` 片段，保持 GitHub
显示的名单和顺序，不再单独维护提交历史判断规则。

- 不要手工编辑生成的快照。
- GitHub 暂时不可用时，构建保留最后一份有效快照。
- `website/functions/api/contributors.js` 将 GitHub 片段转换为经过校验、短期缓存的
  JSON 响应。应用和网站使用它实时刷新，失败时回退到打包的快照。
- PR 作者会在 GitHub 将其计入仓库首页贡献者名单后显示。不要另加允许名单或继承
  历史过滤规则。

### 图标和第三方声明

应用图标使用 ALCOMD3 主题色 `#6cb6ff`，并由项目第三方资源声明覆盖。

- 重新生成图标时保留完整 logo 设计。
- 除非有明确设计决策，不要把小尺寸图标替换为简化版本。
- 保持 `vrc-get-gui/THIRD-PARTY.md` 准确。
- 应用内 Licenses 页面由 `vrc-get-gui/scripts/vite-build-license-json.ts` 生成。

重要资源：

- `vrc-get-gui/app-icon.png`
- `vrc-get-gui/icons/*`
- `vrc-get-gui/icon-LICENSE`
- `vrc-get-gui/third-party/Anton-Regular-OFL.txt`
- `vrc-get-gui/third-party/NotoSans-OFL.txt`

### 软件包操作

ALCOMD3 拥有 package operation progress 和 cancellation behavior。

保留这些行为：

- 安装、移除、重装软件包时显示进度弹窗。
- 弹窗显示逐包状态、整体进度和成功/失败数量。
- 失败的软件包可以重试。
- 用户可以终止长时间操作。
- 操作期间关闭主窗口时请求终止，而不是立即关闭应用。
- 终止不应把已经成功的软件包变成失败项。
- 可以并行时继续执行并行软件包工作；一个软件包失败不应无谓停止无关软件包。

后端规则：

- `AbortCheck` 在前端取消、窗口关闭拦截、包下载、包解压和包应用步骤之间共享。
- Download/cache verification 和 zip extraction 在 copy chunk 之间检查取消。
- 包安装/移除/重装进度通过 `TauriProjectApplyProgress` 上报。
- 收集并报告部分包失败，同时允许成功包继续完成。

重要区域：

- `vrc-get-gui/app/_main/projects/manage`
- `vrc-get-gui/src/commands/project.rs`
- `vrc-get-gui/src/commands/start.rs`
- `vrc-get-gui/src/state/project_apply.rs`
- `vrc-get-vpm/src/traits.rs`
- `vrc-get-vpm/src/environment/package_installer.rs`
- `vrc-get-vpm/src/unity_project/pending_project_changes.rs`
- `vrc-get-vpm/src/utils/extract_zip.rs`

### 项目复制、备份和恢复

保留这些行为：

- 项目复制显示进度并可取消。
- 备份和恢复 workflow 上报进度，避免看起来卡住。
- GUI 备份会在开始前让用户确认或修改归档名称。默认名称由项目名称和时间戳组成，目标仍是已配置的
  备份目录，并自动追加 `.zip`；GUI 预检和后端都会执行文件名称与目标冲突校验，且不会覆盖现有归档。
- GUI 恢复会先选择 zip 备份，再让用户确认或修改恢复后的项目名称，然后才开始恢复。默认名称取自
  备份文件名，目标仍固定为已配置的默认项目目录；GUI 预检和后端都会执行目录名称与目标冲突校验。
- 名称或确认弹窗完成后，必须先从 DOM 脱离，再启动嵌套的备份、复制或恢复进度弹窗；否则非活动弹窗
  可能截获 Esc，导致当前进度弹窗无法最小化。
- 项目复制和项目恢复使用独立任务锁，彼此不应互相阻塞，但同类操作仍要拒绝重复运行。
- MCP 备份、复制和恢复调用使用标准 task-aware execution 提供可查询进度和取消，并复用与 GUI 操作相同的后端任务锁。
- MCP 停用后必须拒绝新的工具调用和新的项目任务启动，但已启动项目任务的查询、结果读取和取消是收尾例外。
- 主窗口关闭处理与正在运行的长任务协调，避免静默中断。

重要区域：

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

### 仓库管理

保留 ALCOMD3 仓库管理功能：

- 仓库排序 / 优先级调整。
- 查看仓库包含的软件包列表。

从任何来源引入包或仓库管理改动时，都要保留这些功能。

重要区域：

- `vrc-get-gui/app/_main/packages/repositories`
- `vrc-get-gui/components/ReorderableList.tsx`

### 活动记录和技术日志

ALCOMD3 将用户可读的活动记录与偏开发者排错的技术日志分开维护。

保留这些规则：

- `/log` 路由默认显示活动记录；技术日志保留为次级 tab，用于排障。
- 活动记录必须覆盖有意义的 GUI、MCP、DeepLink 和 System 操作，包括失败、取消、写操作、MCP tool call，以及重要的被动刷新。
- Secondary 活动记录可以默认隐藏，但必须能通过筛选查询。
- 活动详情不得记录 MCP 原始 params、疑似 token 的值、HTTP header 值、URL query string 或 URL userinfo 凭据。本地文件系统路径可以完整记录，因为排查 Unity、VPM 和非 ASCII 路径问题时通常需要完整路径。
- 高频内部进度、Unity stdout 行、逐文件复制事件、cache hit、渲染事件和 logger 维护行为应保留在技术日志或不记录。
- 活动 JSONL 文件放在现有本地数据树 `activity-logs/` 下，并保持有界保留策略。
- 技术日志文件放在 `technical-logs/` 下。新日志文件使用 `alcomd3-` 前缀；legacy
  `vrc-get-` 日志文件名仍可读取。
- MCP 日志访问必须保持选择性：活动记录和技术日志使用两套工具，摘要/搜索结果必须分页，详情必须按 id 获取，技术日志消息必须继续脱敏并限制长度。技术日志脱敏必须移除疑似 token 的值、authorization/API key 材料，以及 URL userinfo、query string 和 fragment。

重要区域：

- `vrc-get-gui/src/activity_log.rs`
- `vrc-get-gui/src/commands/activity.rs`
- `vrc-get-gui/app/_main/log/`
- `vrc-get-gui/src/logging.rs`

### MCP bridge

ALCOMD3 包含可选本地 MCP bridge。除非未来变更通过 review 和 UI approval flow 明确扩展，否则保留最小只读边界。

保留这些规则：

- MCP data access 默认关闭，工具返回 ALCOMD3 数据前必须从 GUI 启用。
- 外部 MCP server 是通过 stdio 运行的 `alcomd3-mcp`。
- `alcomd3-mcp` 的 stdout 只能包含 MCP JSON-RPC 消息；诊断输出写入 stderr。
- GUI 运行时暴露 private localhost TCP IPC endpoint，并在本地数据目录的 `mcp/endpoint.json` 写入 metadata。
- GUI MCP enable/disable control 只 gate 新的工具数据访问和任务启动，不应停止 local endpoint；禁用时新的工具调用返回 `mcp_disabled`，已启动项目任务的 get/list/cancel 作为收尾例外保留。
- `ALCOMD3_MCP_ENDPOINT_FILE` 可为开发和测试覆盖 endpoint metadata path。
- MCP 工具：只读工具包括 list projects、get registered project details、list repositories、get GUI-visible package details、列出 GUI 可见软件包、列出指定存储库中的 GUI 可见软件包、读取环境设置，以及选择性查询活动记录和技术日志；有限写工具包括备份已登记项目、复制已登记项目、从 zip 备份恢复项目。
- 有限写工具还可以通过既有 GUI 可见项目包规则，为已登记项目安装、卸载或重装单个软件包。
- 公开 MCP 工具必须继续只是 GUI capability 的适配层。每个公开 tool 都必须在
  `vrc-get-gui/src/backend/mcp_capabilities.rs` 中有映射，缺少 GUI capability 映射时
  测试应失败。
- GUI Tauri commands 和 MCP IPC/tool dispatch 应通过 `vrc-get-gui/src/backend/`
  下的共享服务调用共同后端逻辑。MCP-specific 代码应只保留 access gate、参数/DTO 映射、
  task 封装、错误映射和活动记录。
- 不要为了补 parity 新增 MCP-only 业务能力。应先复用或暴露等价 GUI 后端能力，无法映射时停止并做设计 review。
- 读取详情前，`project_path` 必须匹配 ALCOMD3 registered project。
- 备份和复制的源项目路径必须匹配 ALCOMD3 registered project；MCP 复制目标和恢复备份路径必须是绝对路径；从备份恢复只写入 GUI 配置的默认项目目录。
- MCP package search 不得强制 repository refresh。
- 每次 MCP tool call 都必须写入本地活动记录，包含 request id、tool name、可用时的 client 摘要，以及脱敏后的 details。
- 成功的 MCP 读取工具（包括日志查询工具）应记录为 Secondary 活动；失败和取消仍需默认可见。
- `initialize` 和 `tools/list` 不得启动 GUI。
- endpoint 不可用时，实际 tool call 可以启动 GUI。bridge 只能启动 packaged/sibling ALCOMD3 GUI executable 或明确的 `ALCOMD3_GUI_EXECUTABLE` override。
- `alcomd3-mcp` 不得 install、update 或 repair GUI。
- GUI shutdown 应删除 endpoint file。

重要区域：

- `docs/mcp.md`
- `alcomd3-mcp/`
- `alcomd3-mcp-protocol/`
- `vrc-get-gui/src/backend/`
- `vrc-get-gui/src/mcp.rs`
- `vrc-get-gui/src/commands/mcp.rs`
- `vrc-get-gui/app/_main/mcp/index.tsx`
- `xtask/src/build_alcom.rs`
- `xtask/src/bundle_alcom.rs`

### 更新器和发布

ALCOMD3 使用自己的 update source 和 signing key。

保留这些规则：

- Stable endpoint：`https://alcomd3.cqmhv.com/api/gui/tauri-updater.json`。
- Beta endpoint：`https://alcomd3.cqmhv.com/api/gui/tauri-updater-beta.json`。
- update-available dialog 包含打开 `https://alcomd3.cqmhv.com/` 的官网动作。
- 自动检查更新失败时对用户静默。
- 自动更新默认开启，并沿用原有的启动检查。检查到可安装版本后，自动分支不显示确认框，
  直接进入与手动更新共用的下载、进度上报和暂存流程；自动下载失败时仍不通知用户。下载时
  侧边栏显示进度，点击后打开同一个进度弹窗。关闭开关只停止今后的免确认下载，不会丢弃已经
  下载的更新。
- 用户手动确认的更新在共用下载和暂存流程完成后立即安装。自动下载的更新则停留在下载完成页，
  用户可以立即安装，也可以留到下次启动。下次启动时，后端会在启动 MCP 或创建主窗口前重新
  验证并安装暂存包，然后只执行一次用于加载新二进制的重启。切换通道会丢弃暂存包。
- 启动阶段判断是否重启期间，命令行、macOS 打开的 URL/文件和 single-instance 请求统一进入
  同一个队列。重启前必须持久化该队列，并按 at-least-once 语义消费；瞬时读取错误不得覆盖或
  删除队列。如果最终 handoff 在有限次数重试后仍不可写，应使用安装前快照继续退出，而不是
  无限期阻塞安装器。最终快照校验与结束内存捕获必须原子完成，使晚到请求要么进入下一次持久化
  快照，要么走正常的实时请求路径，而不会仅停留在即将退出的进程内存中。
- 确定性的自动安装失败会暂停同一版本和通道的后续自动尝试。出现新版本、切换通道或自动更新
  设置，或者用户手动更新时，仍允许继续安装。读取失败状态时若发生瞬时 I/O 错误，必须保留
  该状态和暂存更新包，并暂停自动安装直到失败状态重新可读。启动或应用暂存更新时若发生瞬时
  I/O 错误，也必须保留更新包以便下次启动重试；只有确定性的安装拒绝才记录为失败版本。
- 手动检查更新时，无更新和更新失败都显示 dialog。
- 版本使用 `2.1.0-beta.1` 这类 SemVer 字符串。
- updater public key 位于 `vrc-get-gui/src/updater-public-key.txt`；GUI include 这个文件，
  `xtask` 验证器也读取同一文件。
- updater private key 不得进入 git。
- 签名 updater installer 时遵循 `docs/RELEASE/RELEASE.zh-CN.md` 和
  `docs/ALCOMD3_UPDATER/ALCOMD3_UPDATER.zh-CN.md`。
- 发布 website updater JSON 前，对每个将被服务的 JSON 文件运行
  `cargo xtask verify-alcom-updater-json --assets <directory>`。
- 单个 updater manifest 原子描述 `windows-x86_64`、`darwin-aarch64` 和
  `linux-x86_64`；对应 updater payload 分别是 Windows setup executable、macOS
  `.app.tar.gz` 和 Linux `.AppImage.tar.gz`。
- Linux AppImage 启用 self-updater。DEB 使用 `--no-self-updater` 单独构建，使包管理器安装
  始终由包管理器管理。每个 updater/download 资产通过
  `releasePlatforms.*.updater.updateMode` 或
  `releasePlatforms.*.downloads[].updateMode` 声明该合约，release shard 会将模式与资产
  digest 一同绑定。
- `alcomd3.config.json` 中必需的 `macosAdHocSigning` 配置对 app 和 DMG 进行 ad-hoc 签名，
  不进行 Apple 公证，因此 Gatekeeper 可能要求用户在首次启动时人工确认。ALCOMD3 updater
  Minisign 单独验证 `.app.tar.gz`；ad-hoc 代码签名不会取代或削弱该校验。
- 官网从 stable/beta manifests 与共享平台 catalog 派生下载。`2.1.2-beta.1` 首次启用
  三平台 beta catalog；stable `2.1.2` 将同一 catalog 用于 stable。Stable 2.1.1 继续作为
  历史 GitHub Release 保留，其旧资产名不会生成直接下载链接或别名。

### Windows 安装器

保留这些规则：

- Installer product name 是 `ALCOMD3`。
- Output setup file 是 `ALCOMD3_{version}_windows_x86_64_setup.exe`。
- `VersionInfoProductVersion` 使用 Windows-compatible numeric version。
- Setup icon 使用 `vrc-get-gui/icons/icon.ico`。
- 已安装主程序是 `ALCOMD3.exe`。
- 安装前无条件移除 `legacyWindowsAppId` 下的 Inno Setup 安装及其已知
  `ALCOM.exe`、`ALCOMD3.exe` 和 `alcomd3-mcp.exe` 文件；清理失败时必须中止新安装。
- 不依赖旧 AppId 是否存在，无条件清理原始安装用户的 `legacyTauriIdentifier` 本地
  WebView 数据目录；保留独立的 `ALCOMD3` 数据目录。
- 清理旧 AppId 在用户/公共桌面和开始菜单中留下的已知 `ALCOM`/`ALCOMD3` 快捷方式；
  保留旧安装是否选择桌面快捷方式的状态，并让替代快捷方式指向新的 `ALCOMD3.exe`。
- 迁移不得删除 ALCOMD3 用户数据，也不得清理不属于旧共享 AppId 的 legacy ALCOM
  NSIS 记录或无关快捷方式。
- `tauriIdentifier`、`windowsAppId` 和 `windowsAumid` 是迁移后的长期身份，不得再次变更。只有在单独审查
  并确认共用迁移窗口结束后，才能同时移除 `legacyTauriIdentifier`、
  `legacyWindowsAppId` 清理逻辑和 `legacyWindowsMigrationReleaseTag` 固定迁移测试基线。

### GitHub 配置

ALCOMD3 在 `.github/workflows/release-draft.yml` 和
`.github/workflows/release-updater.yml` 维护仓库专用发布自动化。Workflow 只负责编排共享的
`cargo xtask release-*` 规则；不要重新引入通用继承的签名或发布 action。详见
`docs/RELEASE/RELEASE.zh-CN.md`。

`.github/workflows/full-chain.yml` 是跨平台测试 workflow。相关 PR 和 `main` push 会运行
Windows x64、macOS arm64 与 Linux x64 job；定时或手动运行会重复这三个 job，并且只在
Windows job 内额外验证已公开 updater。该 workflow 的 macOS smoke bundles 会显式执行
ad-hoc 签名，Linux bundles 保持未签名；它们都是一次性测试产物，不得作为正式 Release
assets 使用。

正式 Draft workflow 使用独立 DAG：preflight、三个原生平台 build shards、可信
`release-assemble`、创建 Draft。上传前，Windows shard 会使用准确的 source-bound 正式
安装器升级固定迁移基线，并且只在升级后验证当前身份。macOS shard 使用绑定配置的 ad-hoc
签名路径且未经 Apple 公证；组装阶段签名并验证三个 updater payload，只允许恰好 10 项公开
资产。公开后，updater workflow 验证全部 10 项资产的 attestation，并原子生成所选 channel
的三平台 metadata。

### Release notes 和本地构建命令

- 准备发布源提交时，`release-prepare` 创建
  `release-notes/ALCOMD3_$Version.md`。
- 过去 release notes 保留在 `release-notes/` 下。
- 未来 release notes 应聚焦用户可见的 ALCOMD3 变化。

未签名的本地 Windows shard 构建：

```powershell
$Version = "<version>"
$Channel = "<stable-or-beta>"
cargo xtask release-build --version $Version --channel $Channel --platform windows-x86_64
```

预期输出：

- `artifacts/local-test/v{version}/ALCOMD3_{version}_windows_x86_64_setup.exe`
- `artifacts/local-test/v{version}/ALCOMD3_{version}_windows_x86_64_setup.exe.zip`

普通 `release-build` 不为 updater payload 签名。仅在例外的本地手动发布流程中追加
`--release-artifacts`，构建三个平台 shard，再运行 `release-assemble` 生成并验证三份
Minisign 签名和统一 release manifest，之后才能运行 `release-publish`。

已知非阻塞警告：

- `xtask/src/bundle_alcom/linux.rs` 可能出现 unused `mode` variable warning。
- Vite 可能提示 `vrc-get://localhost/global-info.js` 没有 `type="module"` 时无法 bundle。
