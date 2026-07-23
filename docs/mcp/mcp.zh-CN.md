# ALCOMD3 MCP 说明

语言: [English](../mcp.md) | [日本語](mcp.ja.md) | 简体中文 | [繁體中文](mcp.zh-TW.md)

本文档说明 ALCOMD3 的 MCP 接入方式、可用工具、生命周期行为和排障方法。

ALCOMD3 当前实现遵循 MCP `2025-11-25` 规范的 stdio transport。MCP 客户端启动
`alcomd3-mcp` 作为子进程，通过 stdin/stdout 交换 JSON-RPC 消息。`alcomd3-mcp`
再通过 ALCOMD3 GUI 暴露的本机 IPC endpoint 请求应用数据。

## 快速开始

1. 启动 ALCOMD3，打开侧边栏中的 MCP 页面。
2. 启用 MCP。
3. 复制页面显示的 Bridge Command。
4. 在支持 MCP 的客户端中，将该命令添加为 stdio MCP server。
5. 保持 ALCOMD3 中的 MCP 为启用状态，然后运行工具调用。

请直接使用 GUI 显示的命令，不要自行猜测 bridge 路径。配置示例和生命周期详情请参阅
[启用和客户端配置](#启用和客户端配置)。

## 当前边界

- MCP 功能默认停用，需要在 GUI 中手动启用后才允许新的工具调用读取或写入 ALCOMD3 数据。
- GUI 正常运行时会启动本机 IPC endpoint；启用/停用 MCP 只控制工具数据访问，不关闭
  endpoint。
- 当前提供项目、仓库、软件包和环境设置只读工具，以及有限写工具：新建项目、
  添加已有项目、添加 VPM 仓库、备份已登记项目、复制已登记项目、从 zip 备份恢复项目、
  为已登记项目安装/卸载/重装单个软件包。不提供仓库删除、仓库重排、项目删除等其他写操作。
- `initialize` 和 `tools/list` 不会启动 GUI。
- 实际 tool call 需要 GUI 时，如果 endpoint 缺失或不可连接，bridge 会尝试启动 GUI、
  等待 endpoint 出现并重试一次。
- GUI 启动失败或 endpoint 仍不可用时，tool call 返回结构化 `alcomd3_unavailable` 错误，
  并在 MCP tool result 上标记 `isError: true`。
- MCP 停用时，新的 tool call 返回结构化 `mcp_disabled` 错误，不关闭 endpoint、不 panic，
  并在 MCP tool result 上标记 `isError: true`。已启动项目长任务的 `tasks/get`、
  `tasks/result` 和 `tasks/cancel` 是收尾例外，可继续查询结果或取消该任务。
- bridge 对 tool call 做宽松的本地限流和并发保护；超过限制时返回结构化
  `rate_limited` 错误，并在 MCP tool result 上标记 `isError: true`。
- GUI MCP 页面会在已知 tool call 执行时高亮对应工具，并在完成或失败后短暂保留高亮，
  便于观察很快完成的调用。
- GUI MCP 页面按只读、写入和日志用途分组显示工具，并保留工具的精确 MCP 名称；鼠标悬停在工具名上会显示本地化的可读名称。
- GUI MCP 页面显示的是最近活动过的客户端，不是实时连接列表；超过一段时间没有活动的记录
  会自动隐藏。
- MCP tool call 会写入 GUI 的本地活动记录。记录包含来源、工具名、request id、客户端摘要、
  开始/完成/失败/取消状态和经过安全处理的目标/详情，便于用户在 GUI 的“活动记录”页回溯 Agent 做了什么。
- GUI 项目管理页和 MCP 包工具共用后端的 GUI-visible package catalog。预发布、yanked、
  隐藏仓库、隐藏本地用户包、同名包跨来源合并、默认/用户仓库优先级和 Unity 兼容性判断由后端统一执行。
- 每个公开 MCP tool 都必须映射到 GUI 已有 capability，并通过 `vrc-get-gui/src/backend/`
  中的共享后端服务进入业务逻辑。MCP dispatch 只负责启用状态 gate、参数解析、任务封装、
  错误映射和活动记录，不应新增 GUI 不具备的业务能力。
- stdio stdout 只输出 MCP JSON-RPC 消息；日志只能写 stderr。
- GUI 内部 IPC 只监听 `127.0.0.1`，不监听公网地址。

活动记录不会保存原始 MCP params、token-like 字段、HTTP header 值、带 query 的 URL 或 URL userinfo 凭据。
本地文件系统路径会保留完整值，用于排查 Unity、VPM 和中文路径等问题；MCP access 仍需先在 GUI 中启用。

## 架构

```text
MCP Host / Client
        |
        | stdio JSON-RPC
        v
alcomd3-mcp
        |
        | reads endpoint metadata
        v
ALCOMD3 data dir / mcp / endpoint.json
        |
        | localhost TCP, newline-delimited JSON
        v
ALCOMD3 GUI IPC server
```

对外 MCP transport 是 stdio。GUI 内部 TCP IPC 是 bridge 与桌面应用之间的私有通道，
不是对 MCP client 直接暴露的 HTTP 或 MCP transport。

## 启用和客户端配置

1. 启动 ALCOMD3。
2. 打开侧边栏中的 MCP 页面。
3. 点击启用，允许 MCP 工具读取 ALCOMD3 数据。
4. 复制页面中的 Bridge Command。
5. 在支持 stdio MCP server 的客户端中添加一个 MCP server，命令使用复制到的
   `alcomd3-mcp` 路径。

通用配置形态如下，具体字段名以 MCP 客户端为准：

```json
{
    "mcpServers": {
        "alcomd3": {
            "command": "C:\\Path\\To\\ALCOMD3\\alcomd3-mcp.exe"
        }
    }
}
```

macOS 和 Linux 使用无 `.exe` 后缀的 `alcomd3-mcp`。以 GUI MCP 页面显示的命令为准。

配置完成后，MCP 客户端可以在 ALCOMD3 未运行时保留 `alcomd3-mcp` stdio 连接。第一次
实际调用工具时，bridge 会尝试启动 ALCOMD3 GUI。GUI 启动后会暴露 endpoint；如果 GUI
中 MCP 已启用，后续调用会返回数据；如果 MCP 已停用，新的工具调用会返回
`mcp_disabled`，用户可在 GUI 中启用 MCP 后重试工具调用。已启动的项目长任务仍可通过
task 后续方法查询结果或取消。

## 打包位置

- Windows：`alcomd3-mcp.exe` 与主 GUI 可执行文件位于同一安装目录。
- macOS：`alcomd3-mcp` 位于 `.app/Contents/MacOS/`。
- Linux：`alcomd3-mcp` 安装到 `/usr/bin/alcomd3-mcp`。
- AppImage：`alcomd3-mcp` 位于 AppDir 的 `usr/bin/` 内。

`cargo xtask build-alcom` 会同时构建 GUI 主程序和 `alcomd3-mcp`。

## Endpoint metadata

GUI 正常运行时会写入 endpoint metadata。默认路径位于 ALCOMD3 本地数据目录：

```text
ALCOMD3/mcp/endpoint.json
```

测试和开发可通过环境变量覆盖路径：

```text
ALCOMD3_MCP_ENDPOINT_FILE
```

测试和开发也可覆盖 bridge 用来启动 GUI 的可执行文件路径：

```text
ALCOMD3_GUI_EXECUTABLE
```

metadata 格式：

```json
{
    "protocolVersion": 2,
    "transport": "tcp",
    "host": "127.0.0.1",
    "port": 49152,
    "token": "opaque-random-token",
    "pid": 12345
}
```

`token` 只用于 bridge 与 GUI 的本机 IPC 鉴权。不要把 endpoint 文件暴露给远程系统。

## 内部 IPC

内部 IPC 使用换行分隔 JSON。请求和响应结构如下：

```text
IpcRequest {
    protocolVersion,
    token,
    requestId,
    client,
    method,
    params
}

IpcResponse {
    requestId,
    ok,
    result?,
    error?
}
```

GUI 会校验 `protocolVersion` 和 `token`。校验失败会返回业务错误，不会执行工具逻辑。
校验通过后，如果 GUI 中 MCP 处于停用状态，GUI 会对新的工具数据访问和任务启动返回
`mcp_disabled`，不会读取或返回项目、仓库、包等数据。已启动项目长任务的
`project_task_get`、`project_task_list` 和 `project_task_cancel` 是例外，用于让客户端在
停用后继续查询结果或取消已运行任务。

## 可用工具

所有工具都返回 JSON。成功时包含 `ok: true`；业务失败时包含
`ok: false` 和 `error: { code, message, data? }`，并且 MCP tool result 外层会包含
`isError: true`。

| Tool | 参数 | 说明 |
| --- | --- | --- |
| `alcomd3_list_projects` | `{}` | 列出 ALCOMD3 已登记项目。 |
| `alcomd3_get_project_details` | `{ "project_path": string }` | 获取已登记项目详情和已安装包摘要。`project_path` 必须匹配 ALCOMD3 已登记项目。 |
| `alcomd3_list_repositories` | `{}` | 列出 ALCOMD3 当前远程仓库和相关显示设置。`repositories` 包含官方默认、Curated 默认和用户仓库；`userRepositories` 保留为仅用户仓库的兼容字段。 |
| `alcomd3_add_repository` | `{ "repository_url": string, "headers"?: object }` | 下载并校验指定 VPM 仓库 URL，成功后作为用户仓库加入 ALCOMD3，并清除软件包缓存以便后续重新加载。成功时返回新增的 `repository` 摘要；活动记录只保存脱敏 URL 和 header 数量，不保存 header 值。 |
| `alcomd3_get_package_details` | `{ "package_name": string, "version"?: string, "repository_id"?: string, "repository_url"?: string }` | 获取 GUI 可见软件包的详细元数据。`package_name` 必填；`version` 和仓库选择字段可用于缩小到某个具体包版本或来源。 |
| `alcomd3_list_packages` | `{ "offset"?: number, "limit"?: number }` | 分页列出 GUI 默认可见的软件包轻量摘要；同一来源内同一包名只返回最新可见版本，并在 `source.kind` 中标明 `officialDefault`、`curatedDefault`、`userRepository` 或 `localUser`。MCP 不做服务端搜索，client 或 Agent 应自行筛选返回结果。 |
| `alcomd3_list_repository_packages` | `{ "repository_id"?: string, "repository_url"?: string, "offset"?: number, "limit"?: number }` | 按指定仓库分页列出 GUI 可见的软件包轻量摘要；同一仓库内同一包名只返回最新可见版本。先调用 `alcomd3_list_repositories` 获取仓库 `id` 或 `url`，再传入其中一个字段。 |
| `alcomd3_get_environment_settings` | `{}` | 读取 ALCOMD3 环境设置摘要，包括已添加的 Unity 安装、默认 Unity 启动参数、默认项目路径和备份路径。 |
| `alcomd3_search_activity_logs` | `{ "search"?: string, "sources"?: string[], "kinds"?: string[], "statuses"?: string[], "visibility"?: "important" \| "primary" \| "secondary" \| "technical" \| "all", "operations"?: string[], "tool_names"?: string[], "request_id"?: string, "target"?: string, "since"?: string, "until"?: string, "offset"?: number, "limit"?: number, "order"?: "newest" \| "oldest" }` | 分页搜索用户可读活动记录摘要。默认只返回关键活动，`limit` 默认 50、最大 200。 |
| `alcomd3_get_activity_log_entry` | `{ "id": string, "include_details"?: boolean }` | 按活动记录 id 读取单条完整活动记录。详情不包含 MCP 原始 params、URL query 或 URL userinfo；本地路径会保留完整值以便排障。 |
| `alcomd3_summarize_activity_logs` | `alcomd3_search_activity_logs` 参数加 `{ "group_by"?: "source" \| "kind" \| "status" \| "operation" \| "tool_name" \| "client_name" \| "day" \| "hour" }` | 按来源、类型、状态、操作、工具、客户端或时间聚合活动记录，用于先定位需要查看的记录。 |
| `alcomd3_get_activity_log_context` | `{ "id": string, "before"?: number, "after"?: number, "include_details"?: boolean }` | 读取某条活动记录前后的相邻活动，用于回溯操作链路；`before`/`after` 最大 50。 |
| `alcomd3_search_technical_logs` | `{ "search"?: string, "levels"?: string[], "targets"?: string[], "scope"?: "memory" \| "recent_files", "since"?: string, "until"?: string, "offset"?: number, "limit"?: number, "max_message_chars"?: number }` | 分页搜索技术日志预览。默认只查当前进程内存中的 `error`/`warn`，`limit` 默认 50、最大 100，消息预览默认最多 300 字符。 |
| `alcomd3_get_technical_log_entry` | `{ "id": string, "max_message_chars"?: number }` | 按技术日志 id 读取单条日志消息；消息会脱敏并最多返回 4000 字符。 |
| `alcomd3_summarize_technical_logs` | `alcomd3_search_technical_logs` 参数加 `{ "group_by"?: "level" \| "target" \| "file" \| "hour" }` | 按级别、target、文件或小时聚合技术日志，用于定位错误热点。 |
| `alcomd3_create_project` | `{ "project_name": string, "base_path"?: string, "template_id"?: string, "unity_version"?: string }` | 新建 Unity 项目、解析项目软件包并登记到 ALCOMD3。`project_name` 必填；`base_path` 省略时使用 GUI 默认项目路径；`template_id` 和 `unity_version` 省略时使用 GUI 当前模板选择规则。成功时返回 `projectPath`、`templateId` 和 `unityVersion`。 |
| `alcomd3_add_existing_project` | `{ "project_path": string }` | 将已有 Unity 项目目录登记到 ALCOMD3。`project_path` 必须是绝对路径并指向有效 Unity 项目。成功时返回 `projectPath`。 |
| `alcomd3_backup_project` | `{ "project_path": string, "backup_name"?: string, "exclude_vpm_packages"?: boolean }` | 为已登记项目创建 zip 备份，使用 GUI 当前备份目录和备份格式。`exclude_vpm_packages` 为 `true` 时排除已安装 VPM 软件包的内容，默认为 `false`。未传 `backup_name` 时生成“项目名称加时间戳”的默认名称；传入时可覆盖不含 `.zip` 的归档文件名。成功时返回 `backupPath`。 |
| `alcomd3_copy_project` | `{ "source_project_path": string, "new_project_path": string }` | 将已登记项目复制到新的不存在目录，并把复制出的项目登记到 ALCOMD3。成功时返回 `projectPath`。 |
| `alcomd3_restore_project_from_backup` | `{ "backup_path": string, "project_name"?: string }` | 从 zip 备份恢复项目到 GUI 配置的默认项目目录，并登记恢复出的项目。未传 `project_name` 时使用备份文件名。成功时返回 `projectPath`。 |
| `alcomd3_install_project_package` | `{ "project_path": string, "package_name": string, "version_selector": { "type": "latest_gui_visible" } \| { "type": "exact", "version": string }, "source"?: { "repository_id"?: string, "repository_url"?: string }, "allow_conflicts"?: boolean }` | 向已登记项目安装一个 GUI 可见且与项目 Unity 版本兼容的软件包。`latest_gui_visible` 使用后端与 GUI 相同的可见包、来源优先级和预发布设置；`exact` 仍必须匹配 GUI 可见版本。冲突或 legacy 文件/文件夹删除默认阻断。 |
| `alcomd3_uninstall_project_package` | `{ "project_path": string, "package_name": string, "allow_conflicts"?: boolean }` | 从已登记项目卸载一个已安装软件包。冲突或 legacy 文件/文件夹删除默认阻断。 |
| `alcomd3_reinstall_project_package` | `{ "project_path": string, "package_name": string, "allow_conflicts"?: boolean }` | 在已登记项目中重装一个已安装软件包。冲突或 legacy 文件/文件夹删除默认阻断。 |

### 日志查询工具

日志工具按用途分为“活动记录”和“技术日志”两套，避免 Agent 为了排查一个问题把全部日志拉入上下文。

- 活动记录是用户可读、结构化、已脱敏的操作历史。`alcomd3_search_activity_logs`
  默认 `visibility` 为 `important`，会返回写操作、失败、取消和重要 MCP/System 行为等关键活动；
  需要辅助记录时显式传入 `secondary`、`technical` 或 `all`。
- 活动日志搜索结果只返回摘要字段，包括 id、时间、来源、类型、状态、操作、对象、耗时和错误摘要。
  需要详情时再调用 `alcomd3_get_activity_log_entry`，需要上下文时调用
  `alcomd3_get_activity_log_context`。
- 技术日志是排错入口，默认只查当前进程内存里的 `error` 和 `warn`。需要读取近期文件时显式传入
  `"scope": "recent_files"`；需要 Info/Debug/Trace 时显式传入 `levels`。
- 技术日志工具不会返回无限制原文。搜索只返回 `messagePreview`，详情按 `max_message_chars`
  截断，并会脱敏 token、secret、authorization、API key、`sk-` 开头的值，以及 URL userinfo、query 和 fragment。
- 日志工具本身也会被记录为 MCP read activity。成功读取日志属于 Secondary，失败仍会作为失败活动默认可见。

### 项目长任务

Tasks 在 MCP `2025-11-25` 中引入，目前仍属于实验性能力。不同客户端的支持程度可能不同，
其协议行为也可能在后续 MCP 版本中演进。

`alcomd3_create_project`、`alcomd3_backup_project`、`alcomd3_copy_project`、
`alcomd3_restore_project_from_backup`、`alcomd3_install_project_package`、
`alcomd3_uninstall_project_package` 和 `alcomd3_reinstall_project_package` 支持 MCP task-aware 调用，并在
`tools/list` 中声明 `execution.taskSupport: "optional"`。

支持 Tasks 的客户端可以在 `tools/call` 参数中加入 `task: {}`：

- `tools/call` 会立即返回 `CreateTaskResult`，其中包含 `task.taskId`。
- `tasks/get` 可查询 `working`、`completed`、`failed`、`cancelled` 等状态。
- `tasks/result` 在任务完成后返回原工具结果形状，例如 `backupPath`、`projectPath` 或包变更摘要。
- `tasks/result` 返回的工具结果会在 `_meta.io.modelcontextprotocol/related-task` 中标记对应 `taskId`。
- `tasks/cancel` 会取消底层 GUI 后端任务，并释放同类项目任务锁。
- `alcomd3_create_project` 在项目正式登记前收到取消或包解析/应用失败时，会清理 MCP 创建出的未登记项目目录。
- 如果任务运行期间用户停用 MCP，新的工具调用和新的项目任务启动仍会返回
  `mcp_disabled`；已经获得 `taskId` 的项目长任务仍可用 `tasks/get`、`tasks/result`
  和 `tasks/cancel` 收尾。

如果 `tools/call` 的 `_meta.progressToken` 存在，bridge 会发送标准
`notifications/progress`。`tasks/get` 的 `_meta` 也会包含
`alcomd3/projectProgress`，用于轮询读取最近一次进度快照：

```json
{
  "_meta": {
    "alcomd3/projectProgress": {
      "total": 120,
      "proceed": 42,
      "lastProceed": "Assets/example.prefab"
    }
  }
}
```

不使用 task-aware 调用时，这些工具仍按普通同步 `tools/call` 执行，直到完成后返回结果。

### 路径限制

`alcomd3_get_project_details`、`alcomd3_backup_project`、`alcomd3_copy_project`
以及项目包安装/卸载/重装工具的源项目路径只允许使用 ALCOMD3 数据库中已登记的项目路径。MCP client 不能通过这些工具
读取或复制任意本地路径。

`alcomd3_get_environment_settings` 会返回 ALCOMD3 已保存的本机路径，例如 Unity 可执行文件、
默认项目目录和备份目录。该工具不启动 Unity、不调用 Unity Hub 刷新、不扫描额外磁盘路径。

`alcomd3_backup_project` 的 `backup_name` 只允许是单一合法文件名，不能是路径，并且不包含自动追加的
`.zip` 扩展名。归档始终写入 GUI 配置的备份目录，且不会覆盖现有归档。

`alcomd3_copy_project` 的 `new_project_path` 必须是绝对路径、尚不存在的目录路径，且不能位于
源项目目录内部；工具会创建该目录，复制项目文件后登记新项目，失败时会清理新建目录。
`alcomd3_restore_project_from_backup` 的 `backup_path` 必须是绝对路径，并且只从 zip 备份恢复到
GUI 配置的默认项目目录。`project_name` 只允许是单个合法文件夹名，不能包含路径分隔符、
根路径或 `..`。
`alcomd3_create_project` 的 `project_name` 使用同样的单文件夹名限制；显式传入的 `base_path`
必须是绝对路径。未传 `base_path` 时使用 GUI 默认项目路径。`alcomd3_add_existing_project`
的 `project_path` 必须是绝对路径，并且必须能按 Unity 项目加载。

### 软件包可见性和写入限制

`alcomd3_list_packages` 和 `alcomd3_list_repository_packages` 使用与 GUI 软件包页相同的包状态加载路径，不调用强制刷新路径。
返回结果会遵循 GUI 中的预发布、隐藏仓库、隐藏本地用户包和 yanked 过滤规则。MCP tool call
不做服务端搜索。添加仓库必须显式调用 `alcomd3_add_repository`；列表工具不会隐式添加仓库或重构仓库刷新策略。

GUI 项目管理页的软件包表由后端合并同名包生成。MCP 的包列表、包详情和项目包安装选择使用同一套后端规则：

- 关闭“显示预发布软件包”后，GUI 和 MCP 的 GUI-visible 结果都不会包含预发布版本；MCP `latest_gui_visible`
  也无法选择预发布版本。底层缓存仍可保存预发布数据，重新开启后才会进入可见结果。
- yanked 包不会进入可见候选。已安装包如果当前版本 yanked，会在项目包行中保留 yanked 标记。
- 隐藏仓库和隐藏本地用户包只影响可见候选；隐藏来源仍可作为“存在来源”信息显示，但不参与最新版本选择。
- 同名包跨来源在项目管理页合并成一行，默认仓库、本地用户包、用户仓库和未登记仓库按后端顺序合并。
- 项目包安装只会从 GUI-visible 且与项目 Unity 版本兼容的候选中选择版本。

`alcomd3_install_project_package`、`alcomd3_uninstall_project_package` 和
`alcomd3_reinstall_project_package` 会先生成 pending project changes。若结果包含依赖冲突或 legacy
文件/文件夹删除，且未传入 `"allow_conflicts": true`，工具会返回
`project_package_conflicts`，并在 `error.data.changes` 中附带变更摘要；此时不会应用到项目。
确认后重试并设置 `"allow_conflicts": true` 才会继续 apply。

包列表工具只返回适合发现和筛选的摘要字段：`name`、`displayName`、`version` 和 `source`。
列表中的 `totalCount` 和分页字段按聚合后的摘要条目计算，不是仓库原始版本清单的长度。
需要读取描述、关键词、依赖、legacy 包、文档 URL、变更日志 URL 或 Unity 版本要求时，应先从列表中选出候选包，
再调用 `alcomd3_get_package_details` 获取详细元数据。

包列表工具默认 `offset` 为 `0`、`limit` 为 `200`；`limit` 最大为 `1000`，超过时会被限制到最大值。
分页响应包含 `totalCount`、`offset`、`limit`、`returnedCount`、`hasMore` 和 `nextOffset`。
需要读取完整列表时，应在 `hasMore` 为 `true` 时使用 `nextOffset` 继续请求下一页。
包相关工具不再返回 `count` 字段。

## 生命周期和多进程行为

`alcomd3-mcp` 通常由 MCP 客户端按 stdio server 方式启动。不同客户端可能为不同会话
启动多个 bridge 进程，这是 MCP 客户端管理方式导致的正常可能性。

ALCOMD3 的生命周期边界：

- GUI 退出时会停止 IPC listener，并删除 endpoint 文件。
- `alcomd3-mcp` 不会因为 GUI 退出而主动退出。它继续保持 MCP stdio 连接，让 Codex
  等 AI 代理不需要重启就能在 GUI 重新启动后恢复调用。
- GUI 只能观察 bridge 发来的内部 IPC 请求，不能可靠判断 MCP client 与 bridge 的 stdio
  会话是否仍然存活。因此 GUI 中的客户端区域按“最近活动”展示，工具高亮才表示当前正在处理的调用。
- GUI 不可用期间，tool call 返回结构化 `alcomd3_unavailable` 错误。
- GUI 可用但 MCP 停用期间，新的 tool call 返回结构化 `mcp_disabled` 错误；已启动
  项目长任务的 task 后续方法仍可查询结果或取消任务。
- GUI 重新启动后，新的 endpoint metadata 写入同一路径，后续 tool call 会重新连接到
  GUI；是否返回数据取决于 GUI 中 MCP 是否启用。
- 如果 tool call 发生时 GUI 未运行，bridge 会尝试启动 GUI。为避免 MCP 客户端加载配置
  时弹出 GUI，`initialize` 和 `tools/list` 不触发启动。

如果看到短时间内存在多个 `alcomd3-mcp.exe`：

1. 先确认是否有多个 MCP 客户端或多个客户端会话正在运行。
2. GUI 关闭后 bridge 仍存在并不一定是残留；只要 MCP 客户端还保持 stdio 连接，它就会
   继续运行以便 GUI 重启后恢复。
3. 如果确认 MCP 客户端已经关闭但 bridge 仍存在，通常是旧版本 bridge 或客户端未按
   stdio 生命周期管理导致；关闭对应 MCP 客户端或手动结束旧进程后再使用新版本验证。

## 错误和排障

### `mcp_disabled`

MCP 页面处于停用状态。endpoint 仍可能显示运行中，这是正常状态；启用 MCP 后重新调用
工具即可返回数据。已经启动的项目长任务是例外，客户端仍可使用 `tasks/get`、
`tasks/result` 和 `tasks/cancel` 查询结果或取消任务。

### `rate_limited`

bridge 在短时间内收到过多 tool call，或已有过多 tool call 正在执行。稍后重试即可。

### `ALCOMD3 is not running or the MCP IPC endpoint is unavailable`

常见原因：

- ALCOMD3 GUI 未运行。
- endpoint 文件不存在、已过期或被删除。
- 客户端启动的是旧路径中的 `alcomd3-mcp`。

处理方式：

1. 启动 ALCOMD3。
2. 在 MCP 页面确认 endpoint running。
3. 重新复制 Bridge Command，更新 MCP 客户端配置。
4. 重启 MCP 客户端。

### `protocol mismatch`

bridge 与 GUI 的内部 IPC 版本不一致。通常说明客户端启动了旧版本 `alcomd3-mcp`。
重新复制 GUI MCP 页面显示的命令，并确认它指向当前安装目录。

### stdout 出现非 JSON 内容

这是错误行为。stdio MCP server 的 stdout 只能写 JSON-RPC。调试输出必须写 stderr。

## 开发 smoke test

在仓库根目录构建 bridge：

```powershell
cargo build -p alcomd3-mcp
```

GUI 未运行时，可以用不存在的 endpoint 验证 bridge 不 panic：

```powershell
$env:ALCOMD3_MCP_ENDPOINT_FILE = Join-Path $env:TEMP "missing-alcomd3-mcp-endpoint.json"
$payload = @'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"smoke","version":"0.0.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"alcomd3_list_projects","arguments":{}}}
'@
$payload | .\target\debug\alcomd3-mcp.exe
```

预期结果：

- `initialize` 成功。
- `tools/list` 返回当前可用的 MCP 工具。
- `tools/call` 返回 `ok: false` 的可读错误，并在 MCP tool result 上标记
  `isError: true`。
- stdout 中没有非 JSON-RPC 日志。

## 相关源码

- Bridge：`alcomd3-mcp/src/main.rs`
- 共享 IPC 协议：`alcomd3-mcp-protocol/src/lib.rs`
- GUI 共享后端服务和 MCP capability 矩阵：`vrc-get-gui/src/backend/`
- GUI IPC server 和 tool dispatch：`vrc-get-gui/src/mcp.rs`
- GUI Tauri commands：`vrc-get-gui/src/commands/mcp.rs`
- GUI MCP 页面：`vrc-get-gui/app/_main/mcp/index.tsx`
- 打包逻辑：`xtask/src/build_alcom.rs`、`xtask/src/bundle_alcom*`

## 参考

- MCP Specification `2025-11-25`: <https://modelcontextprotocol.io/specification/2025-11-25>
- MCP stdio transport: <https://modelcontextprotocol.io/specification/2025-11-25/basic/transports>
- MCP lifecycle: <https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle>
- MCP tools: <https://modelcontextprotocol.io/specification/2025-11-25/server/tools>
- MCP tasks: <https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/tasks>
- MCP progress: <https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/progress>
- MCP cancellation: <https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/cancellation>
