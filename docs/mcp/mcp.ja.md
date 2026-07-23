# ALCOMD3 MCP ガイド

言語: [English](../mcp.md) | 日本語 | [简体中文](mcp.zh-CN.md) | [繁體中文](mcp.zh-TW.md)

このドキュメントは、ALCOMD3 の MCP 接続方法、利用可能な tools、ライフサイクル挙動、トラブルシューティングを説明します。

ALCOMD3 の現在の実装は、MCP `2025-11-25` specification の stdio transport に従います。MCP client は `alcomd3-mcp` を子プロセスとして起動し、stdin/stdout で JSON-RPC message を交換します。`alcomd3-mcp` は ALCOMD3 GUI が公開する local IPC endpoint を通してアプリケーションデータを要求します。

## クイックスタート

1. ALCOMD3 を起動し、sidebar の MCP page を開きます。
2. MCP を有効化します。
3. page に表示される Bridge Command をコピーします。
4. MCP 対応 client で、その command を stdio MCP server として追加します。
5. ALCOMD3 で MCP を有効にしたまま tool call を実行します。

bridge path を推測せず、GUI に表示される command を使用してください。設定例と
ライフサイクルの詳細は [MCP の有効化と client 設定](#mcp-の有効化と-client-設定)
を参照してください。

## 現在の境界

- MCP は既定で無効です。新しい tool call が ALCOMD3 data を読み書きするには、GUI で手動で有効化する必要があります。
- GUI が通常実行中の場合、local IPC endpoint が起動します。MCP の有効/無効は tool data access を制御するだけで、endpoint は停止しません。
- 現在は project、repository、package、environment、activity log、technical log の read-only tools と、限定的な write tools を提供します。write tools は project 作成、existing project 追加、VPM repository 追加、registered project の backup、registered project の copy、zip backup からの restore、registered project への package install/uninstall/reinstall です。repository 削除、repository 並べ替え、project 削除などの他の write operation は提供しません。
- `initialize` と `tools/list` は GUI を起動しません。
- 実際の tool call で GUI が必要なときに endpoint が存在しない、または接続できない場合、bridge は GUI の起動を試み、endpoint を待って 1 回だけ再試行します。
- GUI 起動に失敗した場合、または endpoint が利用できないままの場合、tool call は structured `alcomd3_unavailable` error を返し、MCP tool result に `isError: true` を付けます。
- MCP が無効な場合、新しい tool call は structured `mcp_disabled` error を返します。endpoint は停止せず、panic もしません。MCP tool result には `isError: true` が付きます。既に開始された project long task の `tasks/get`、`tasks/result`、`tasks/cancel` は cleanup 例外として、結果確認や cancel ができます。
- bridge は tool call に緩やかな local rate limit と concurrency protection を適用します。制限を超えた場合は structured `rate_limited` error を返し、MCP tool result に `isError: true` を付けます。
- GUI MCP page は、既知の tool call 実行中に該当 tool を highlight し、完了または失敗後も短時間 highlight を残します。
- GUI MCP page は tools を read-only、write、log 用途で group 表示し、正確な MCP name を保持します。tool name に hover すると localized readable name を表示します。
- GUI MCP page に表示される client は最近 activity があった client であり、live connection list ではありません。一定時間 activity がない record は自動で隠れます。
- MCP tool call は GUI の local activity log に記録されます。record には source、tool name、request id、client summary、started/completed/failed/cancelled state、安全処理済み target/details が含まれ、ユーザーは GUI の Activity page で Agent が何をしたか確認できます。
- GUI project management page と MCP package tools は backend の GUI-visible package catalog を共有します。pre-release、yanked、hidden repository、hidden local user package、同名 package の source 間 merge、default/user repository priority、Unity compatibility は backend が統一して処理します。
- すべての public MCP tool は GUI の既存 capability に mapping され、`vrc-get-gui/src/backend/` の shared backend service を通して business logic に入ります。MCP dispatch は enabled-state gate、argument parsing、task wrapping、error mapping、activity logging だけを担当し、GUI にない business capability を追加しません。
- stdio stdout は MCP JSON-RPC message だけを出力します。log は stderr にのみ書きます。
- GUI internal IPC は `127.0.0.1` のみ listen し、public network address では listen しません。

Activity record は raw MCP params、token-like field、HTTP header value、query 付き URL、URL userinfo credential を保存しません。local filesystem path は Unity、VPM、非 ASCII path の診断に必要なため完全な値を保持します。MCP access には GUI で MCP を有効化する必要があります。

## アーキテクチャ

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

外部 MCP transport は stdio です。GUI internal TCP IPC は bridge と desktop app の private channel であり、MCP client に直接公開される HTTP transport や MCP transport ではありません。

## MCP の有効化と client 設定

1. ALCOMD3 を起動します。
2. sidebar の MCP page を開きます。
3. MCP を有効化し、MCP tools が ALCOMD3 data を読めるようにします。
4. page の Bridge Command をコピーします。
5. stdio MCP server に対応する client で MCP server を追加し、コピーした `alcomd3-mcp` path を command に設定します。

一般的な設定形は次の通りです。正確な field name は MCP client に従ってください。

```json
{
    "mcpServers": {
        "alcomd3": {
            "command": "C:\\Path\\To\\ALCOMD3\\alcomd3-mcp.exe"
        }
    }
}
```

macOS と Linux では `.exe` suffix のない `alcomd3-mcp` を使います。GUI MCP page に表示される command を優先してください。

設定後、MCP client は ALCOMD3 が未起動でも `alcomd3-mcp` stdio connection を保持できます。最初の実際の tool call 時に、bridge は ALCOMD3 GUI の起動を試みます。GUI が起動して endpoint を公開した後、GUI で MCP が有効なら後続 call は data を返します。MCP が無効なら新しい tool call は `mcp_disabled` を返し、ユーザーが GUI で MCP を有効化した後に再試行できます。既に開始された project long task は task follow-up method で結果確認または cancel できます。

## パッケージ内の配置

- Windows: `alcomd3-mcp.exe` は main GUI executable と同じ install directory にあります。
- macOS: `alcomd3-mcp` は `.app/Contents/MacOS/` にあります。
- Linux: `alcomd3-mcp` は `/usr/bin/alcomd3-mcp` に install されます。
- AppImage: `alcomd3-mcp` は AppDir の `usr/bin/` にあります。

`cargo xtask build-alcom` は GUI main program と `alcomd3-mcp` を同時に build します。

## Endpoint metadata

GUI が通常実行中の場合、endpoint metadata を書き込みます。default path は ALCOMD3 local data directory 配下です。

```text
ALCOMD3/mcp/endpoint.json
```

test と development では環境変数で path を override できます。

```text
ALCOMD3_MCP_ENDPOINT_FILE
```

bridge が起動する GUI executable path も override できます。

```text
ALCOMD3_GUI_EXECUTABLE
```

metadata format:

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

`token` は bridge と GUI の local IPC authentication のみに使います。endpoint file を remote system に公開しないでください。

## Internal IPC

internal IPC は newline-delimited JSON を使います。request/response shape は次の通りです。

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

GUI は `protocolVersion` と `token` を検証します。検証失敗時は business error を返し、tool logic は実行しません。検証後、GUI で MCP が無効な場合、GUI は新しい tool data access と task startup に `mcp_disabled` を返し、project、repository、package などの data を読み取りません。既に開始された project long-task method の `project_task_get`、`project_task_list`、`project_task_cancel` は例外で、MCP 無効化後も既存 task の結果確認や cancel ができます。

## 利用可能な tools

すべての tools は JSON を返します。成功時は `ok: true` を含みます。business failure 時は `ok: false` と `error: { code, message, data? }` を含み、外側の MCP tool result には `isError: true` が付きます。

| Tool | Arguments | 説明 |
| --- | --- | --- |
| `alcomd3_list_projects` | `{}` | ALCOMD3 に登録済みの projects を列出します。 |
| `alcomd3_get_project_details` | `{ "project_path": string }` | 登録済み project の詳細と installed package summary を取得します。`project_path` は ALCOMD3 に登録済みの project と一致する必要があります。 |
| `alcomd3_list_repositories` | `{}` | 現在の remote repositories と表示設定を列出します。`repositories` には official default、Curated default、user repository が含まれます。`userRepositories` は user repository のみの互換 field です。 |
| `alcomd3_add_repository` | `{ "repository_url": string, "headers"?: object }` | 指定した VPM repository URL を download/validate し、user repository として追加し、package cache を clear します。成功時は追加された `repository` summary を返します。activity log は redacted URL と header count だけを保存し、header value は保存しません。 |
| `alcomd3_get_package_details` | `{ "package_name": string, "version"?: string, "repository_id"?: string, "repository_url"?: string }` | GUI-visible package の詳細 metadata を取得します。`package_name` は必須です。`version` と repository selection field で version/source を絞れます。 |
| `alcomd3_list_packages` | `{ "offset"?: number, "limit"?: number }` | GUI default-visible packages の lightweight summary を paging します。同一 source 内の同一 package name は最新 visible version のみ返し、`source.kind` は `officialDefault`、`curatedDefault`、`userRepository`、`localUser` のいずれかです。MCP は server-side search を行わないため、client/Agent 側で filter してください。 |
| `alcomd3_list_repository_packages` | `{ "repository_id"?: string, "repository_url"?: string, "offset"?: number, "limit"?: number }` | 指定 repository の GUI-visible package summary を paging します。同一 repository 内の同一 package name は最新 visible version のみ返します。先に `alcomd3_list_repositories` で `id` または `url` を取得し、どちらかを渡してください。 |
| `alcomd3_get_environment_settings` | `{}` | Unity installs、default Unity launch arguments、default project path、backup path を含む environment settings summary を読み取ります。 |
| `alcomd3_search_activity_logs` | `{ "search"?: string, "sources"?: string[], "kinds"?: string[], "statuses"?: string[], "visibility"?: "important" \| "primary" \| "secondary" \| "technical" \| "all", "operations"?: string[], "tool_names"?: string[], "request_id"?: string, "target"?: string, "since"?: string, "until"?: string, "offset"?: number, "limit"?: number, "order"?: "newest" \| "oldest" }` | user-readable activity log summary を paging search します。default は important activity のみ、`limit` default は 50、max は 200 です。 |
| `alcomd3_get_activity_log_entry` | `{ "id": string, "include_details"?: boolean }` | activity log entry を id で 1 件読み取ります。detail は raw MCP params、URL query、URL userinfo を含みません。local path は診断用に完全値を保持します。 |
| `alcomd3_summarize_activity_logs` | `alcomd3_search_activity_logs` arguments plus `{ "group_by"?: "source" \| "kind" \| "status" \| "operation" \| "tool_name" \| "client_name" \| "day" \| "hour" }` | source、kind、status、operation、tool、client、time で activity records を集計し、読むべき record を先に探します。 |
| `alcomd3_get_activity_log_context` | `{ "id": string, "before"?: number, "after"?: number, "include_details"?: boolean }` | entry の前後 activity を読み、operation chain を確認します。`before`/`after` は最大 50 です。 |
| `alcomd3_search_technical_logs` | `{ "search"?: string, "levels"?: string[], "targets"?: string[], "scope"?: "memory" \| "recent_files", "since"?: string, "until"?: string, "offset"?: number, "limit"?: number, "max_message_chars"?: number }` | technical log preview を paging search します。default は current process memory の `error`/`warn` のみ、`limit` default は 50、max は 100、message preview default は 300 chars です。 |
| `alcomd3_get_technical_log_entry` | `{ "id": string, "max_message_chars"?: number }` | technical log message を id で 1 件読み取ります。message は redacted され、最大 4000 chars です。 |
| `alcomd3_summarize_technical_logs` | `alcomd3_search_technical_logs` arguments plus `{ "group_by"?: "level" \| "target" \| "file" \| "hour" }` | level、target、file、hour で technical logs を集計し、error hotspot を探します。 |
| `alcomd3_create_project` | `{ "project_name": string, "base_path"?: string, "template_id"?: string, "unity_version"?: string }` | Unity project を作成し、project packages を解決し、ALCOMD3 に登録します。`project_name` は必須です。`base_path` 省略時は GUI default project path、`template_id` と `unity_version` 省略時は GUI の現在の template selection rule を使用します。成功時は `projectPath`、`templateId`、`unityVersion` を返します。 |
| `alcomd3_add_existing_project` | `{ "project_path": string }` | existing Unity project directory を ALCOMD3 に登録します。`project_path` は absolute path で、有効な Unity project として load できる必要があります。成功時は `projectPath` を返します。 |
| `alcomd3_backup_project` | `{ "project_path": string, "backup_name"?: string, "exclude_vpm_packages"?: boolean }` | registered project の zip backup を、GUI の現在の backup directory と backup format で作成します。`exclude_vpm_packages` が `true` の場合は installed VPM package contents を除外し、default は `false` です。`backup_name` 省略時は project name と timestamp から既定名を生成し、指定時は `.zip` を含まない archive file name を上書きします。成功時は `backupPath` を返します。 |
| `alcomd3_copy_project` | `{ "source_project_path": string, "new_project_path": string }` | registered project を存在しない新 directory に copy し、copy された project を ALCOMD3 に登録します。成功時は `projectPath` を返します。 |
| `alcomd3_restore_project_from_backup` | `{ "backup_path": string, "project_name"?: string }` | zip backup から GUI-configured default project directory に project を restore し、登録します。`project_name` 省略時は backup file name を使います。成功時は `projectPath` を返します。 |
| `alcomd3_install_project_package` | `{ "project_path": string, "package_name": string, "version_selector": { "type": "latest_gui_visible" } \| { "type": "exact", "version": string }, "source"?: { "repository_id"?: string, "repository_url"?: string }, "allow_conflicts"?: boolean }` | registered project に、project の Unity version と互換性がある GUI-visible package を 1 つ install します。`latest_gui_visible` は GUI backend と同じ visible package、source priority、pre-release settings を使います。`exact` も GUI-visible version と一致する必要があります。conflict または legacy file/folder deletion は default で block されます。 |
| `alcomd3_uninstall_project_package` | `{ "project_path": string, "package_name": string, "allow_conflicts"?: boolean }` | registered project から installed package を 1 つ uninstall します。conflict または legacy file/folder deletion は default で block されます。 |
| `alcomd3_reinstall_project_package` | `{ "project_path": string, "package_name": string, "allow_conflicts"?: boolean }` | registered project で installed package を 1 つ reinstall します。conflict または legacy file/folder deletion は default で block されます。 |

### Log query tools

Log tools は activity records と technical logs に分かれています。Agent が 1 つの問題を調査するためにすべての logs を context に取り込む必要を減らします。

- Activity records は user-readable、structured、redacted 済みの operation history です。`alcomd3_search_activity_logs` の default `visibility` は `important` で、write、failure、cancellation、重要な MCP/System behavior を返します。補助 record が必要な場合は `secondary`、`technical`、`all` を明示してください。
- Activity search result は summary fields のみ返します。id、time、source、kind、status、operation、target、duration、error summary が含まれます。detail は `alcomd3_get_activity_log_entry`、周辺 context は `alcomd3_get_activity_log_context` を呼びます。
- Technical logs は diagnostics 用です。default は current process memory の `error` と `warn` だけです。recent files を読むには `"scope": "recent_files"` を渡し、Info/Debug/Trace が必要なら `levels` を明示します。
- Technical log tools は無制限の raw text を返しません。search は `messagePreview` を返し、detail は `max_message_chars` で truncate され、token、secret、authorization、API key、`sk-` values、URL userinfo、query、fragment を redact します。
- Log tools 自体も MCP read activity として記録されます。成功した log read は Secondary、失敗は failed activity として表示されます。

### Project long tasks

Tasks は MCP `2025-11-25` で導入され、現在も実験的な capability です。
client ごとに対応状況が異なり、protocol behavior は将来の MCP version で
変更される可能性があります。

`alcomd3_create_project`、`alcomd3_backup_project`、`alcomd3_copy_project`、`alcomd3_restore_project_from_backup`、`alcomd3_install_project_package`、`alcomd3_uninstall_project_package`、`alcomd3_reinstall_project_package` は MCP task-aware call に対応し、`tools/list` で `execution.taskSupport: "optional"` を宣言します。

Tasks 対応 client は `tools/call` params に `task: {}` を含めることができます。

- `tools/call` はすぐに `CreateTaskResult` を返し、`task.taskId` を含みます。
- `tasks/get` は `working`、`completed`、`failed`、`cancelled` などの状態を query します。
- `tasks/result` は task completion 後に元 tool result shape を返します。例: `backupPath`、`projectPath`、package change summary。
- `tasks/result` の tool result は `_meta.io.modelcontextprotocol/related-task` に対応する `taskId` を記録します。
- `tasks/cancel` は underlying GUI backend task を cancel し、同種 project task lock を release します。
- `alcomd3_create_project` は formal registration 前に cancel された場合、または package resolve/apply に失敗した場合、MCP が作成した未登録 project directory を cleanup します。
- task 実行中にユーザーが MCP を無効化した場合、新しい tool call と新しい project task startup は `mcp_disabled` を返します。すでに `taskId` を得ている long task は `tasks/get`、`tasks/result`、`tasks/cancel` で cleanup できます。

`tools/call` の `_meta.progressToken` が存在する場合、bridge は標準 `notifications/progress` を送信します。`tasks/get` の `_meta` には、latest progress snapshot を polling するための `alcomd3/projectProgress` も含まれます。

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

task-aware call を使わない場合、これらの tools は completion まで通常の synchronous `tools/call` として実行されます。

### Path restrictions

`alcomd3_get_project_details`、`alcomd3_backup_project`、`alcomd3_copy_project`、project package install/uninstall/reinstall tools の source project path は、ALCOMD3 database に登録済みの project path のみ受け付けます。MCP client はこれらの tools で任意の local path を read/copy できません。

`alcomd3_get_environment_settings` は ALCOMD3 が保存した local paths を返します。例: Unity executable、default project directory、backup directory。この tool は Unity を起動せず、Unity Hub refresh も行わず、追加 disk path scan もしません。

`alcomd3_backup_project` の `backup_name` は path ではなく、合法な file name 1 つだけを受け付けます。自動追加される `.zip` extension は含めません。archive は常に GUI-configured backup directory に書き込まれ、既存 archive は上書きしません。

`alcomd3_copy_project` の `new_project_path` は absolute path、まだ存在しない directory path、かつ source project 内部ではない必要があります。tool は directory を作成し、project files を copy し、新 project を登録し、失敗時は新 directory を cleanup します。`alcomd3_restore_project_from_backup` の `backup_path` は absolute path で、GUI-configured default project directory にのみ restore します。`project_name` は合法な folder name 1 つだけで、path separator、root path、`..` は使えません。`alcomd3_create_project` の `project_name` も同じ single-folder-name restriction を受けます。明示的な `base_path` は absolute path である必要があります。`base_path` 省略時は GUI default project path を使います。`alcomd3_add_existing_project` の `project_path` は absolute path で、有効な Unity project として load できる必要があります。

### Package visibility and write limits

`alcomd3_list_packages` と `alcomd3_list_repository_packages` は GUI package page と同じ package-state load path を使い、force-refresh path は呼びません。results は GUI の pre-release、hidden repository、hidden local user package、yanked filters に従います。MCP tool call は server-side search を行いません。repository 追加には明示的な `alcomd3_add_repository` call が必要です。list tools は暗黙に repository を追加したり、repository refresh strategy を作り直したりしません。

GUI project-management package table は backend が同名 package merge logic から生成します。MCP package lists、package details、project package install selection は同じ backend rules を使います。

- "Show pre-release packages" が off の場合、GUI と MCP の GUI-visible results は pre-release versions を含みません。MCP `latest_gui_visible` も pre-release version を選択できません。underlying cache は pre-release data を保持できますが、setting を再有効化するまで visible results には入りません。
- Yanked package は visible candidate に入りません。installed package version が現在 yanked の場合、project package row は yanked marker を保持します。
- Hidden repositories と hidden local user packages は visible candidate にだけ影響します。hidden source は "existing source" information として表示されることがありますが、latest-version selection には参加しません。
- source 間の同名 packages は project management page で 1 行に merge されます。default repositories、local user packages、user repositories、unregistered repositories は backend order で merge されます。
- project package installation は、GUI-visible かつ project の Unity version と互換性がある candidate だけを選択します。

`alcomd3_install_project_package`、`alcomd3_uninstall_project_package`、`alcomd3_reinstall_project_package` は最初に pending project changes を生成します。結果に dependency conflicts または legacy file/folder deletion が含まれ、`"allow_conflicts": true` が渡されていない場合、tool は `project_package_conflicts` を返し、`error.data.changes` に change summary を含めます。この場合 project には適用されません。確認後、`"allow_conflicts": true` を設定して再試行すると apply します。

Package list tools は discovery/filter 用の summary fields だけを返します: `name`、`displayName`、`version`、`source`。`totalCount` と paging fields は aggregated summary rows から計算され、raw repository version list の length ではありません。description、keywords、dependencies、legacy packages、documentation URL、changelog URL、Unity version requirements を読むには、list から candidate を選び、`alcomd3_get_package_details` を呼びます。

Package list tools の default `offset` は `0`、`limit` は `200` です。`limit` max は `1000` です。paging response は `totalCount`、`offset`、`limit`、`returnedCount`、`hasMore`、`nextOffset` を含みます。complete list を読むには、`hasMore` が `true` の間 `nextOffset` で次ページを request します。package tools は `count` field を返しません。

## Lifecycle and multi-process behavior

`alcomd3-mcp` は通常、MCP client によって stdio server として起動されます。client や session が複数ある場合、複数の bridge processes が起動することがあります。これは MCP client management による正常な可能性です。

ALCOMD3 lifecycle boundaries:

- GUI 終了時、IPC listener を停止し endpoint file を削除します。
- `alcomd3-mcp` は GUI 終了時に自動終了しません。MCP stdio connection を保持し、Codex などの AI agents が client を再起動せずに GUI restart 後の call を回復できるようにします。
- GUI は bridge からの internal IPC request を観測できますが、MCP client と bridge の stdio session がまだ alive かを確実には判断できません。そのため GUI client area は recent activity を表示し、tool highlight が現在処理中の call を示します。
- GUI unavailable 中、tool call は structured `alcomd3_unavailable` を返します。
- GUI available だが MCP disabled の場合、新しい tool call は structured `mcp_disabled` を返します。既に開始された project long task の task follow-up methods は result query や cancel ができます。
- GUI restart 後、新しい endpoint metadata が同じ path に書き込まれ、後続 tool call は GUI に reconnect します。data を返すかどうかは GUI で MCP が enabled かに依存します。
- GUI が未起動の時に tool call が発生すると、bridge は GUI の起動を試みます。MCP client が config を load しただけで GUI が出ないように、`initialize` と `tools/list` は startup を trigger しません。

短時間に複数の `alcomd3-mcp.exe` が見える場合:

1. 複数の MCP clients または複数の client sessions が実行中か確認してください。
2. GUI を閉じた後も bridge が残ることは必ずしも stale ではありません。MCP client が stdio connection を保持している限り、GUI restart 後の call recovery のために実行を続けます。
3. MCP client が閉じているのに bridge が残る場合、通常は古い bridge version または client が stdio lifecycle を正しく管理していないことが原因です。該当 MCP client を閉じるか古い process を終了してから新 version で確認してください。

## Errors and troubleshooting

### `mcp_disabled`

MCP page が disabled です。endpoint が running と表示されることがありますが正常です。MCP を有効化して tool を再実行してください。既に開始された project long tasks は例外で、client は `tasks/get`、`tasks/result`、`tasks/cancel` で query/cancel できます。

### `rate_limited`

bridge が短時間に多すぎる tool calls を受信した、または既に多すぎる tool calls が実行中です。少し待ってから retry してください。

### `ALCOMD3 is not running or the MCP IPC endpoint is unavailable`

よくある原因:

- ALCOMD3 GUI が起動していない。
- endpoint file が存在しない、期限切れ、または削除された。
- client が旧 path の `alcomd3-mcp` を起動している。

対応:

1. ALCOMD3 を起動します。
2. MCP page で endpoint running を確認します。
3. Bridge Command を再コピーし、MCP client configuration を更新します。
4. MCP client を再起動します。

### `protocol mismatch`

bridge と GUI の internal IPC version が一致しません。通常は client が古い `alcomd3-mcp` を起動していることを示します。GUI MCP page に表示される command を再コピーし、current install directory を指していることを確認してください。

### stdout に非 JSON content が出る

これは bug です。stdio MCP server の stdout は JSON-RPC のみを含む必要があります。debug output は stderr に出してください。

## Development smoke test

repository root で bridge を build します。

```powershell
cargo build -p alcomd3-mcp
```

GUI 未起動時、存在しない endpoint で bridge が panic しないことを確認できます。

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

Expected result:

- `initialize` succeeds.
- `tools/list` returns current MCP tools.
- `tools/call` returns readable `ok: false` error and marks the MCP tool result with `isError: true`.
- stdout contains no non-JSON-RPC logs.

## Related source

- Bridge: `alcomd3-mcp/src/main.rs`
- Shared IPC protocol: `alcomd3-mcp-protocol/src/lib.rs`
- GUI shared backend services and MCP capability matrix: `vrc-get-gui/src/backend/`
- GUI IPC server and tool dispatch: `vrc-get-gui/src/mcp.rs`
- GUI Tauri commands: `vrc-get-gui/src/commands/mcp.rs`
- GUI MCP page: `vrc-get-gui/app/_main/mcp/index.tsx`
- Packaging logic: `xtask/src/build_alcom.rs`, `xtask/src/bundle_alcom*`

## References

- MCP Specification `2025-11-25`: <https://modelcontextprotocol.io/specification/2025-11-25>
- MCP stdio transport: <https://modelcontextprotocol.io/specification/2025-11-25/basic/transports>
- MCP lifecycle: <https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle>
- MCP tools: <https://modelcontextprotocol.io/specification/2025-11-25/server/tools>
- MCP tasks: <https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/tasks>
- MCP progress: <https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/progress>
- MCP cancellation: <https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/cancellation>
