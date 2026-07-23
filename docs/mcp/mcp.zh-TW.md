# ALCOMD3 MCP 說明

語言: [English](../mcp.md) | [日本語](mcp.ja.md) | [簡體中文](mcp.zh-CN.md) | 繁體中文

本文件說明 ALCOMD3 的 MCP 接入方式、可用工具、生命週期行為和疑難排解方法。

ALCOMD3 目前實作遵循 MCP `2025-11-25` 規範的 stdio transport。MCP 客戶端啟動
`alcomd3-mcp` 作為子行程，透過 stdin/stdout 交換 JSON-RPC 訊息。`alcomd3-mcp`
再透過 ALCOMD3 GUI 暴露的本機 IPC endpoint 請求應用資料。

## 快速開始

1. 啟動 ALCOMD3，開啟側邊欄中的 MCP 頁面。
2. 啟用 MCP。
3. 複製頁面顯示的 Bridge Command。
4. 在支援 MCP 的客戶端中，將該命令新增為 stdio MCP server。
5. 保持 ALCOMD3 中的 MCP 為啟用狀態，然後執行工具呼叫。

請直接使用 GUI 顯示的命令，不要自行猜測 bridge 路徑。設定範例和生命週期詳情請參閱
[啟用與客戶端設定](#啟用與客戶端設定)。

## 目前邊界

- MCP 功能預設停用，需要在 GUI 中手動啟用後才允許新的工具呼叫讀取或寫入 ALCOMD3 資料。
- GUI 正常執行時會啟動本機 IPC endpoint；啟用/停用 MCP 只控制工具資料存取，不關閉
  endpoint。
- 目前提供專案、倉庫、軟體包和環境設定唯讀工具，以及有限寫工具：新建專案、
  新增既有專案、新增 VPM 倉庫、備份已登錄專案、複製已登錄專案、從 zip 備份還原專案、
  為已登錄專案安裝/解除安裝/重新安裝單一軟體包。不提供倉庫刪除、倉庫重新排序、專案刪除等其他寫操作。
- `initialize` 和 `tools/list` 不會啟動 GUI。
- 實際 tool call 需要 GUI 時，如果 endpoint 缺失或不可連接，bridge 會嘗試啟動 GUI、
  等待 endpoint 出現並重試一次。
- GUI 啟動失敗或 endpoint 仍不可用時，tool call 傳回結構化 `alcomd3_unavailable` 錯誤，
  並在 MCP tool result 上標記 `isError: true`。
- MCP 停用時，新的 tool call 傳回結構化 `mcp_disabled` 錯誤，不關閉 endpoint、不 panic，
  並在 MCP tool result 上標記 `isError: true`。已啟動專案長任務的 `tasks/get`、
  `tasks/result` 和 `tasks/cancel` 是收尾例外，可繼續查詢結果或取消該任務。
- bridge 對 tool call 做寬鬆的本機速率限制和並行保護；超過限制時傳回結構化
  `rate_limited` 錯誤，並在 MCP tool result 上標記 `isError: true`。
- GUI MCP 頁面會在已知 tool call 執行時醒目標示對應工具，並在完成或失敗後短暫保留醒目標示，
  便於觀察很快完成的呼叫。
- GUI MCP 頁面按唯讀、寫入和日誌用途分組顯示工具，並保留工具的精確 MCP 名稱；滑鼠懸停在工具名稱上會顯示在地化的可讀名稱。
- GUI MCP 頁面顯示的是最近活動過的客戶端，不是即時連線清單；超過一段時間沒有活動的記錄
  會自動隱藏。
- MCP tool call 會寫入 GUI 的本機活動記錄。記錄包含來源、工具名稱、request id、客戶端摘要、
  開始/完成/失敗/取消狀態和經過安全處理的目標/詳細資料，便於使用者在 GUI 的“活動記錄”頁回溯 Agent 做了什麼。
- GUI 專案管理頁和 MCP 包工具共用後端的 GUI-visible package catalog。預發布、yanked、
  隱藏倉庫、隱藏本機使用者包、同名包跨來源合併、預設/使用者倉庫優先順序和 Unity 相容性判斷由後端統一執行。
- 每個公開 MCP tool 都必須映射到 GUI 已有 capability，並透過 `vrc-get-gui/src/backend/`
  中的共享後端服務進入業務邏輯。MCP dispatch 只負責啟用狀態 gate、參數解析、任務封裝、
  錯誤映射和活動記錄，不應新增 GUI 不具備的業務能力。
- stdio stdout 只輸出 MCP JSON-RPC 訊息；日誌只能寫 stderr。
- GUI 內部 IPC 只監聽 `127.0.0.1`，不監聽公網位址。

活動記錄不會儲存原始 MCP params、token-like 欄位、HTTP header 值、帶 query 的 URL 或 URL userinfo 憑證。
本機檔案系統路徑會保留完整值，用於排查 Unity、VPM 和中文路徑等問題；MCP access 仍需先在 GUI 中啟用。

## 架構

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

對外 MCP transport 是 stdio。GUI 內部 TCP IPC 是 bridge 與桌面應用之間的私有通道，
不是對 MCP client 直接暴露的 HTTP 或 MCP transport。

## 啟用與客戶端設定

1. 啟動 ALCOMD3。
2. 打開側邊欄中的 MCP 頁面。
3. 點擊啟用，允許 MCP 工具讀取 ALCOMD3 資料。
4. 複製頁面中的 Bridge Command。
5. 在支援 stdio MCP server 的客戶端中新增一個 MCP server，命令使用複製到的
   `alcomd3-mcp` 路徑。

通用設定形態如下，具體欄位名稱以 MCP 客戶端為準：

```json
{
    "mcpServers": {
        "alcomd3": {
            "command": "C:\\Path\\To\\ALCOMD3\\alcomd3-mcp.exe"
        }
    }
}
```

macOS 和 Linux 使用無 `.exe` 後綴的 `alcomd3-mcp`。以 GUI MCP 頁面顯示的命令為準。

設定完成後，MCP 客戶端可以在 ALCOMD3 未執行時保留 `alcomd3-mcp` stdio 連接。第一次
實際呼叫工具時，bridge 會嘗試啟動 ALCOMD3 GUI。GUI 啟動後會暴露 endpoint；如果 GUI
中 MCP 已啟用，後續呼叫會傳回資料；如果 MCP 已停用，新的工具呼叫會傳回
`mcp_disabled`，使用者可在 GUI 中啟用 MCP 後重試工具呼叫。已啟動的專案長任務仍可透過
task 後續方法查詢結果或取消。

## 打包位置

- Windows：`alcomd3-mcp.exe` 與主 GUI 可執行檔位於同一安裝目錄。
- macOS：`alcomd3-mcp` 位於 `.app/Contents/MacOS/`。
- Linux：`alcomd3-mcp` 安裝到 `/usr/bin/alcomd3-mcp`。
- AppImage：`alcomd3-mcp` 位於 AppDir 的 `usr/bin/` 內。

`cargo xtask build-alcom` 會同時構建 GUI 主程式和 `alcomd3-mcp`。

## Endpoint metadata

GUI 正常執行時會寫入 endpoint metadata。預設路徑位於 ALCOMD3 本機資料目錄：

```text
ALCOMD3/mcp/endpoint.json
```

測試和開發可透過環境變數覆寫路徑：

```text
ALCOMD3_MCP_ENDPOINT_FILE
```

測試和開發也可覆寫 bridge 用來啟動 GUI 的可執行檔路徑：

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

`token` 只用於 bridge 與 GUI 的本機 IPC 鑑權。不要將 endpoint 檔案暴露給遠端系統。

## 內部 IPC

內部 IPC 使用換行分隔 JSON。請求和回應結構如下：

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

GUI 會驗證 `protocolVersion` 和 `token`。驗證失敗會傳回業務錯誤，不會執行工具邏輯。
驗證透過後，如果 GUI 中 MCP 處於停用狀態，GUI 會對新的工具資料存取和任務啟動傳回
`mcp_disabled`，不會讀取或傳回專案、倉庫、包等資料。已啟動專案長任務的
`project_task_get`、`project_task_list` 和 `project_task_cancel` 是例外，用於讓客戶端在
停用後繼續查詢結果或取消已執行任務。

## 可用工具

所有工具都傳回 JSON。成功時包含 `ok: true`；業務失敗時包含
`ok: false` 和 `error: { code, message, data? }`，並且 MCP tool result 外層會包含
`isError: true`。

| Tool | 參數 | 說明 |
| --- | --- | --- |
| `alcomd3_list_projects` | `{}` | 列出 ALCOMD3 已登錄專案。 |
| `alcomd3_get_project_details` | `{ "project_path": string }` | 取得已登錄專案詳細資料和已安裝包摘要。`project_path` 必須符合 ALCOMD3 已登錄專案。 |
| `alcomd3_list_repositories` | `{}` | 列出 ALCOMD3 目前遠端倉庫和相關顯示設定。`repositories` 包含官方預設、Curated 預設和使用者倉庫；`userRepositories` 保留為僅使用者倉庫的相容欄位。 |
| `alcomd3_add_repository` | `{ "repository_url": string, "headers"?: object }` | 下載並驗證指定 VPM 倉庫 URL，成功後作為使用者倉庫加入 ALCOMD3，並清除軟體包快取以便後續重新載入。成功時傳回新增的 `repository` 摘要；活動記錄只儲存脫敏 URL 和 header 數量，不儲存 header 值。 |
| `alcomd3_get_package_details` | `{ "package_name": string, "version"?: string, "repository_id"?: string, "repository_url"?: string }` | 取得 GUI 可見軟體包的詳細元資料。`package_name` 必填；`version` 和倉庫選擇欄位可用於縮小到某個具體包版本或來源。 |
| `alcomd3_list_packages` | `{ "offset"?: number, "limit"?: number }` | 分頁列出 GUI 預設可見的軟體包輕量摘要；同一來源內同一包名只傳回最新可見版本，並在 `source.kind` 中標明 `officialDefault`、`curatedDefault`、`userRepository` 或 `localUser`。MCP 不做伺服器端搜尋，client 或 Agent 應自行篩選傳回結果。 |
| `alcomd3_list_repository_packages` | `{ "repository_id"?: string, "repository_url"?: string, "offset"?: number, "limit"?: number }` | 依指定倉庫分頁列出 GUI 可見的軟體包輕量摘要；同一倉庫內同一包名只傳回最新可見版本。先呼叫 `alcomd3_list_repositories` 取得倉庫 `id` 或 `url`，再傳入其中一個欄位。 |
| `alcomd3_get_environment_settings` | `{}` | 讀取 ALCOMD3 環境設定摘要，包括已新增的 Unity 安裝、預設 Unity 啟動參數、預設專案路徑和備份路徑。 |
| `alcomd3_search_activity_logs` | `{ "search"?: string, "sources"?: string[], "kinds"?: string[], "statuses"?: string[], "visibility"?: "important" \| "primary" \| "secondary" \| "technical" \| "all", "operations"?: string[], "tool_names"?: string[], "request_id"?: string, "target"?: string, "since"?: string, "until"?: string, "offset"?: number, "limit"?: number, "order"?: "newest" \| "oldest" }` | 分頁搜尋使用者可讀活動記錄摘要。預設只傳回關鍵活動，`limit` 預設 50、最大 200。 |
| `alcomd3_get_activity_log_entry` | `{ "id": string, "include_details"?: boolean }` | 依活動記錄 id 讀取單筆完整活動記錄。詳細資料不包含 MCP 原始 params、URL query 或 URL userinfo；本機路徑會保留完整值以便排障。 |
| `alcomd3_summarize_activity_logs` | `alcomd3_search_activity_logs` 參數加 `{ "group_by"?: "source" \| "kind" \| "status" \| "operation" \| "tool_name" \| "client_name" \| "day" \| "hour" }` | 依來源、類型、狀態、操作、工具、客戶端或時間彙總活動記錄，用於先定位需要查看的記錄。 |
| `alcomd3_get_activity_log_context` | `{ "id": string, "before"?: number, "after"?: number, "include_details"?: boolean }` | 讀取某條活動記錄前後的相鄰活動，用於回溯操作鏈路；`before`/`after` 最大 50。 |
| `alcomd3_search_technical_logs` | `{ "search"?: string, "levels"?: string[], "targets"?: string[], "scope"?: "memory" \| "recent_files", "since"?: string, "until"?: string, "offset"?: number, "limit"?: number, "max_message_chars"?: number }` | 分頁搜尋技術日誌預覽。預設只查目前行程記憶體中的 `error`/`warn`，`limit` 預設 50、最大 100，訊息預覽預設最多 300 字元。 |
| `alcomd3_get_technical_log_entry` | `{ "id": string, "max_message_chars"?: number }` | 依技術日誌 id 讀取單筆日誌訊息；訊息會脫敏並最多傳回 4000 字元。 |
| `alcomd3_summarize_technical_logs` | `alcomd3_search_technical_logs` 參數加 `{ "group_by"?: "level" \| "target" \| "file" \| "hour" }` | 依層級、target、檔案或小時彙總技術日誌，用於定位錯誤熱點。 |
| `alcomd3_create_project` | `{ "project_name": string, "base_path"?: string, "template_id"?: string, "unity_version"?: string }` | 新建 Unity 專案、解析專案軟體包並登錄到 ALCOMD3。`project_name` 必填；`base_path` 省略時使用 GUI 預設專案路徑；`template_id` 和 `unity_version` 省略時使用 GUI 目前範本選擇規則。成功時傳回 `projectPath`、`templateId` 和 `unityVersion`。 |
| `alcomd3_add_existing_project` | `{ "project_path": string }` | 將既有 Unity 專案目錄登錄到 ALCOMD3。`project_path` 必須是絕對路徑並指向有效 Unity 專案。成功時傳回 `projectPath`。 |
| `alcomd3_backup_project` | `{ "project_path": string, "backup_name"?: string, "exclude_vpm_packages"?: boolean }` | 為已登錄專案建立 zip 備份，使用 GUI 目前備份目錄和備份格式。`exclude_vpm_packages` 為 `true` 時排除已安裝 VPM 軟體包的內容，預設為 `false`。未傳 `backup_name` 時產生「專案名稱加時間戳」的預設名稱；傳入時可覆寫不含 `.zip` 的封存檔名。成功時傳回 `backupPath`。 |
| `alcomd3_copy_project` | `{ "source_project_path": string, "new_project_path": string }` | 將已登錄專案複製到新的不存在目錄，並把複製出的專案登錄到 ALCOMD3。成功時傳回 `projectPath`。 |
| `alcomd3_restore_project_from_backup` | `{ "backup_path": string, "project_name"?: string }` | 從 zip 備份還原專案到 GUI 設定的預設專案目錄，並登錄還原出的專案。未傳 `project_name` 時使用備份檔名。成功時傳回 `projectPath`。 |
| `alcomd3_install_project_package` | `{ "project_path": string, "package_name": string, "version_selector": { "type": "latest_gui_visible" } \| { "type": "exact", "version": string }, "source"?: { "repository_id"?: string, "repository_url"?: string }, "allow_conflicts"?: boolean }` | 向已登錄專案安裝一個 GUI 可見且與專案 Unity 版本相容的軟體包。`latest_gui_visible` 使用後端與 GUI 相同的可見包、來源優先順序和預發布設定；`exact` 仍必須符合 GUI 可見版本。衝突或 legacy 檔案/資料夾刪除預設阻擋。 |
| `alcomd3_uninstall_project_package` | `{ "project_path": string, "package_name": string, "allow_conflicts"?: boolean }` | 從已登錄專案解除安裝一個已安裝軟體包。衝突或 legacy 檔案/資料夾刪除預設阻擋。 |
| `alcomd3_reinstall_project_package` | `{ "project_path": string, "package_name": string, "allow_conflicts"?: boolean }` | 在已登錄專案中重新安裝一個已安裝軟體包。衝突或 legacy 檔案/資料夾刪除預設阻擋。 |

### 日誌查詢工具

日誌工具按用途分為“活動記錄”和“技術日誌”兩套，避免 Agent 為了排查一個問題把全部日誌拉入上下文。

- 活動記錄是使用者可讀、結構化、已脫敏的操作歷史。`alcomd3_search_activity_logs`
  預設 `visibility` 為 `important`，會傳回寫操作、失敗、取消和重要 MCP/System 行為等關鍵活動；
  需要輔助記錄時明確傳入 `secondary`、`technical` 或 `all`。
- 活動日誌搜尋結果只傳回摘要欄位，包括 id、時間、來源、類型、狀態、操作、物件、耗時和錯誤摘要。
  需要詳細資料時再呼叫 `alcomd3_get_activity_log_entry`，需要上下文時呼叫
  `alcomd3_get_activity_log_context`。
- 技術日誌是排錯入口，預設只查目前行程記憶體裡的 `error` 和 `warn`。需要讀取近期檔案時明確傳入
  `"scope": "recent_files"`；需要 Info/Debug/Trace 時明確傳入 `levels`。
- 技術日誌工具不會傳回無限制原文。搜尋只傳回 `messagePreview`，詳細資料按 `max_message_chars`
  截斷，並會脫敏 token、secret、authorization、API key、`sk-` 開頭的值，以及 URL userinfo、query 和 fragment。
- 日誌工具本身也會被記錄為 MCP read activity。成功讀取日誌屬於 Secondary，失敗仍會作為失敗活動預設可見。

### 專案長任務

Tasks 在 MCP `2025-11-25` 中引入，目前仍屬於實驗性能力。不同客戶端的支援程度可能不同，
其協定行為也可能在後續 MCP 版本中演進。

`alcomd3_create_project`、`alcomd3_backup_project`、`alcomd3_copy_project`、
`alcomd3_restore_project_from_backup`、`alcomd3_install_project_package`、
`alcomd3_uninstall_project_package` 和 `alcomd3_reinstall_project_package` 支援 MCP task-aware 呼叫，並在
`tools/list` 中宣告 `execution.taskSupport: "optional"`。

支援 Tasks 的客戶端可以在 `tools/call` 參數中加入 `task: {}`：

- `tools/call` 會立即傳回 `CreateTaskResult`，其中包含 `task.taskId`。
- `tasks/get` 可查詢 `working`、`completed`、`failed`、`cancelled` 等狀態。
- `tasks/result` 在任務完成後傳回原工具結果形狀，例如 `backupPath`、`projectPath` 或包變更摘要。
- `tasks/result` 傳回的工具結果會在 `_meta.io.modelcontextprotocol/related-task` 中標記對應 `taskId`。
- `tasks/cancel` 會取消底層 GUI 後端任務，並釋放同類專案任務鎖。
- `alcomd3_create_project` 在專案正式登錄前收到取消或包解析/套用失敗時，會清理 MCP 建立出的未登錄專案目錄。
- 如果任務執行期間使用者停用 MCP，新的工具呼叫和新的專案任務啟動仍會傳回
  `mcp_disabled`；已經獲得 `taskId` 的專案長任務仍可用 `tasks/get`、`tasks/result`
  和 `tasks/cancel` 收尾。

如果 `tools/call` 的 `_meta.progressToken` 存在，bridge 會發送標準
`notifications/progress`。`tasks/get` 的 `_meta` 也會包含
`alcomd3/projectProgress`，用於輪詢讀取最近一次進度快照：

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

不使用 task-aware 呼叫時，這些工具仍按一般同步 `tools/call` 執行，直到完成後傳回結果。

### 路徑限制

`alcomd3_get_project_details`、`alcomd3_backup_project`、`alcomd3_copy_project`
以及專案包安裝/解除安裝/重新安裝工具的來源專案路徑只允許使用 ALCOMD3 資料庫中已登記的專案路徑。MCP client 不能透過這些工具
讀取或複製任意本機路徑。

`alcomd3_get_environment_settings` 會傳回 ALCOMD3 已儲存的本機路徑，例如 Unity 可執行檔、
預設專案目錄和備份目錄。該工具不啟動 Unity、不呼叫 Unity Hub 刷新、不掃描額外磁碟路徑。

`alcomd3_backup_project` 的 `backup_name` 只允許是單一合法檔名，不能是路徑，且不包含自動附加的
`.zip` 副檔名。封存檔始終寫入 GUI 設定的備份目錄，且不會覆寫現有封存檔。

`alcomd3_copy_project` 的 `new_project_path` 必須是絕對路徑、尚不存在的目錄路徑，且不能位於
來源專案目錄內部；工具會建立該目錄，複製專案檔案後登記新專案，失敗時會清理新建目錄。
`alcomd3_restore_project_from_backup` 的 `backup_path` 必須是絕對路徑，並且只從 zip 備份還原到
GUI 設定的預設專案目錄。`project_name` 只允許是單一合法資料夾名稱，不能包含路徑分隔符、
根路徑或 `..`。
`alcomd3_create_project` 的 `project_name` 使用同樣的單一資料夾名稱限制；明確傳入的 `base_path`
必須是絕對路徑。未傳 `base_path` 時使用 GUI 預設專案路徑。`alcomd3_add_existing_project`
的 `project_path` 必須是絕對路徑，並且必須能按 Unity 專案載入。

### 軟體包可見性與寫入限制

`alcomd3_list_packages` 和 `alcomd3_list_repository_packages` 使用與 GUI 軟體包頁相同的包狀態載入路徑，不呼叫強制重新整理路徑。
傳回結果會遵循 GUI 中的預發布、隱藏倉庫、隱藏本機使用者包和 yanked 篩選規則。MCP tool call
不做伺服器端搜尋。新增倉庫必須明確呼叫 `alcomd3_add_repository`；列表工具不會隱式新增倉庫或重構倉庫重新整理策略。

GUI 專案管理頁的軟體包表由後端合併同名包產生。MCP 的包列表、包詳細資料和專案包安裝選擇使用同一套後端規則：

- 關閉“顯示預發布軟體包”後，GUI 和 MCP 的 GUI-visible 結果都不會包含預發布版本；MCP `latest_gui_visible`
  也無法選擇預發布版本。底層快取仍可儲存預發布資料，重新開啟後才會進入可見結果。
- yanked 包不會進入可見候選。已安裝包如果目前版本 yanked，會在專案包行中保留 yanked 標記。
- 隱藏倉庫和隱藏本機使用者包只影響可見候選；隱藏來源仍可作為“存在來源”資訊顯示，但不參與最新版本選擇。
- 同名包跨來源在專案管理頁合併成一行，預設倉庫、本機使用者包、使用者倉庫和未登錄倉庫依後端順序合併。
- 專案包安裝只會從 GUI-visible 且與專案 Unity 版本相容的候選中選擇版本。

`alcomd3_install_project_package`、`alcomd3_uninstall_project_package` 和
`alcomd3_reinstall_project_package` 會先產生 pending project changes。若結果包含相依性衝突或 legacy
檔案/資料夾刪除，且未傳入 `"allow_conflicts": true`，工具會傳回
`project_package_conflicts`，並在 `error.data.changes` 中附帶變更摘要；此時不會套用到專案。
確認後重試並設定 `"allow_conflicts": true` 才會繼續 apply。

包列表工具只傳回適合發現和篩選的摘要欄位：`name`、`displayName`、`version` 和 `source`。
列表中的 `totalCount` 和分頁欄位按彙總後的摘要條目计算，不是倉庫原始版本清單的長度。
需要讀取描述、關鍵字、相依性、legacy 包、文檔 URL、變更日誌 URL 或 Unity 版本要求時，應先從列表中選出候選包，
再呼叫 `alcomd3_get_package_details` 取得詳細元資料。

包列表工具預設 `offset` 為 `0`、`limit` 為 `200`；`limit` 最大為 `1000`，超過時會被限制到最大值。
分頁回應包含 `totalCount`、`offset`、`limit`、`returnedCount`、`hasMore` 和 `nextOffset`。
需要讀取完整清單時，應在 `hasMore` 為 `true` 時使用 `nextOffset` 繼續請求下一頁。
包相關工具不再傳回 `count` 欄位。

## 生命週期和多行程行為

`alcomd3-mcp` 通常由 MCP 客戶端按 stdio server 方式啟動。不同客戶端可能為不同會話
啟動多個 bridge 行程，這是 MCP 客戶端管理方式導致的正常可能性。

ALCOMD3 的生命週期邊界：

- GUI 結束時會停止 IPC listener，並刪除 endpoint 檔案。
- `alcomd3-mcp` 不會因為 GUI 結束而主動結束。它繼續保持 MCP stdio 連接，讓 Codex
  等 AI 代理不需要重啟就能在 GUI 重新啟動後恢復呼叫。
- GUI 只能觀察 bridge 發來的內部 IPC 請求，不能可靠判斷 MCP client 與 bridge 的 stdio
  會話是否仍然存活。因此 GUI 中的客戶端區域按“最近活動”展示，工具醒目標示才表示目前正在處理的呼叫。
- GUI 不可用期間，tool call 傳回結構化 `alcomd3_unavailable` 錯誤。
- GUI 可用但 MCP 停用期間，新的 tool call 傳回結構化 `mcp_disabled` 錯誤；已啟動
  專案長任務的 task 後續方法仍可查詢結果或取消任務。
- GUI 重新啟動後，新的 endpoint metadata 寫入同一路徑，後續 tool call 會重新連線到
  GUI；是否傳回資料取決於 GUI 中 MCP 是否啟用。
- 如果 tool call 發生時 GUI 未執行，bridge 會嘗試啟動 GUI。為避免 MCP 客戶端載入設定
  時彈出 GUI，`initialize` 和 `tools/list` 不觸發啟動。

如果看到短時間內存在多個 `alcomd3-mcp.exe`：

1. 先確認是否有多個 MCP 客戶端或多個客戶端工作階段正在執行。
2. GUI 關閉後 bridge 仍存在並不一定是殘留；只要 MCP 客戶端還保持 stdio 連接，它就會
   繼續運行以便 GUI 重啟後恢復。
3. 如果確認 MCP 客戶端已經關閉但 bridge 仍存在，通常是舊版本 bridge 或客戶端未按
   stdio 生命週期管理導致；關閉對應 MCP 客戶端或手動結束舊進程後再使用新版本驗證。

## 錯誤與疑難排解

### `mcp_disabled`

MCP 頁面處於停用狀態。endpoint 仍可能顯示執行中，這是正常狀態；啟用 MCP 後重新呼叫
工具即可傳回資料。已經啟動的專案長任務是例外，客戶端仍可使用 `tasks/get`、
`tasks/result` 和 `tasks/cancel` 查詢結果或取消任務。

### `rate_limited`

bridge 在短時間內收到過多 tool call，或已有過多 tool call 正在執行。稍後重試即可。

### `ALCOMD3 is not running or the MCP IPC endpoint is unavailable`

常見原因：

- ALCOMD3 GUI 未執行。
- endpoint 檔案不存在、已過期或被刪除。
- 客戶端啟動的是舊路徑中的 `alcomd3-mcp`。

處理方式：

1. 啟動 ALCOMD3。
2. 在 MCP 頁面確認 endpoint running。
3. 重新複製 Bridge Command，更新 MCP 客戶端設定。
4. 重啟 MCP 客戶端。

### `protocol mismatch`

bridge 與 GUI 的內部 IPC 版本不一致。通常表示客戶端啟動了舊版本 `alcomd3-mcp`。
重新複製 GUI MCP 頁面顯示的命令，並確認它指向目前安裝目錄。

### stdout 出現非 JSON 內容

這是錯誤行為。stdio MCP server 的 stdout 只能寫 JSON-RPC。除錯輸出必須寫 stderr。

## 開發 smoke test

在倉庫根目錄構建 bridge：

```powershell
cargo build -p alcomd3-mcp
```

GUI 未執行時，可以用不存在的 endpoint 驗證 bridge 不 panic：

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

預期結果：

- `initialize` 成功。
- `tools/list` 傳回目前可用的 MCP 工具。
- `tools/call` 傳回 `ok: false` 的可讀錯誤，並在 MCP tool result 上標記
  `isError: true`。
- stdout 中沒有非 JSON-RPC 日誌。

## 相關原始碼

- Bridge：`alcomd3-mcp/src/main.rs`
- 共享 IPC 協議：`alcomd3-mcp-protocol/src/lib.rs`
- GUI 共享後端服務和 MCP capability 矩陣：`vrc-get-gui/src/backend/`
- GUI IPC server 和 tool dispatch：`vrc-get-gui/src/mcp.rs`
- GUI Tauri commands：`vrc-get-gui/src/commands/mcp.rs`
- GUI MCP 頁面：`vrc-get-gui/app/_main/mcp/index.tsx`
- 打包邏輯：`xtask/src/build_alcom.rs`、`xtask/src/bundle_alcom*`

## 參考

- MCP Specification `2025-11-25`: <https://modelcontextprotocol.io/specification/2025-11-25>
- MCP stdio transport: <https://modelcontextprotocol.io/specification/2025-11-25/basic/transports>
- MCP lifecycle: <https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle>
- MCP tools: <https://modelcontextprotocol.io/specification/2025-11-25/server/tools>
- MCP tasks: <https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/tasks>
- MCP progress: <https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/progress>
- MCP cancellation: <https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/cancellation>
