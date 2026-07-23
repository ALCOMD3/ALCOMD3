<p align="center">
    <img src="../website/public/assets/logo.png" alt="ALCOMD3 標誌" width="160">
</p>

<h1 align="center">ALCOMD3</h1>

<p align="center">
    面向 VRChat Unity 專案與 VPM 套件管理的開源 VRChat Creator Companion 替代品。
</p>

<p align="center">
    <a href="https://alcomd3.cqmhv.com/">下載</a> ·
    <a href="https://github.com/ALCOMD3/ALCOMD3/releases">發行版本</a> ·
    <a href="../docs/README.md">文件</a>
</p>

<p align="center">
    <a href="../README.md">English</a> ·
    <a href="./README.ja.md">日本語</a> · 繁體中文 ·
    <a href="./README.zh-CN.md">简体中文</a>
</p>

## 在一個地方管理你的 VRChat 專案

ALCOMD3 協助創作者集中處理 VRChat Unity 專案的日常工作，不必再手動整理專案資料夾與套件檔案。

- **管理專案：**在一個桌面應用程式中建立、登記、複製、備份和還原專案。
- **控制 VPM 套件：**瀏覽儲存庫，為每個專案安裝、移除或更新套件。
- **沿用熟悉的連結：**開啟與 VCC 相容的 `vcc://` 連結，並在同一套流程中管理儲存庫。
- **打造自己的介面：**使用 Material Design 3 風格介面，並可選擇淺色、深色和自訂主題。
- **接入自己的 AI 工具：**視需要允許支援 MCP 的客戶端在限定範圍內存取 ALCOMD3
  的專案、套件、儲存庫和日誌。

## 開始使用

1. 前往 [ALCOMD3 官方網站](https://alcomd3.cqmhv.com/)，選擇 stable 或 beta 頻道。
2. 下載適合目前作業系統的套件，然後安裝或啟動應用程式。
3. 新增既有 VRChat Unity 專案或建立新專案，接著即可在 ALCOMD3 中管理套件、
   儲存庫和備份。

官方版本支援 Windows x64、搭載 Apple 晶片的 macOS，以及 Linux x64。你也可以在
[GitHub Releases](https://github.com/ALCOMD3/ALCOMD3/releases) 中查看所有已發行版本。

支援應用程式內更新時，ALCOMD3 預設在啟動時檢查已簽署更新，發現更新後下載，
並在下次啟動前安裝。可在「設定」中關閉此行為。

## 可選的本機 MCP 整合

ALCOMD3 可將支援 MCP 的 AI 客戶端連接到經過限定的專案、儲存庫、套件、環境設定、
活動記錄和技術日誌資料，並提供一組有限的專案與套件操作。

MCP 預設關閉。啟用後，它只使用本機 stdio bridge 與僅限本機回送的私有 IPC。
GUI 內部 IPC 只監聽 `127.0.0.1` 本機回送位址，不監聽公網位址。設定方式、可用工具和
權限邊界請參閱 [MCP 指南](../docs/mcp/mcp.zh-TW.md)。

## 專案與社群

ALCOMD3 起源於 ALCOM/vrc-get，目前作為獨立的開源專案維護。
它不是 VRChat 或 VCC 的官方產品。

- 遇到問題或有新想法？[提交 Issue](https://github.com/ALCOMD3/ALCOMD3/issues)。
- 想參與貢獻？請閱讀[貢獻指南](../CONTRIBUTING.md)。
- 需要技術或維護資料？從[文件索引](../docs/README.md)開始。
- 想了解版本變化？查看 [GitHub 發行說明](https://github.com/ALCOMD3/ALCOMD3/releases)。

## 參與開發

開發環境需要 Rust stable toolchain、Node.js、npm，以及目標平台所需的建置工具。

以開發模式執行桌面應用程式：

```powershell
cd vrc-get-gui
npm run tauri dev
```

僅開發前端：

```powershell
cd vrc-get-gui
npm run dev
```

建置、測試、維護和發布資料統一收錄在[文件索引](../docs/README.md)中。

## 授權

主要專案程式碼使用 GNU Affero General Public License v3.0 or later
（`AGPL-3.0-or-later`）。請參閱 [LICENSE](../LICENSE) 和
[LICENSE-NOTES.md](../LICENSE-NOTES.md)。

依賴項和第三方資源聲明可在應用程式內的 Licenses 頁面和
[vrc-get-gui/THIRD-PARTY.md](../vrc-get-gui/THIRD-PARTY.md) 中查看。
