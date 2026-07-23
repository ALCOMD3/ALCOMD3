# ALCOMD3 v2.1.1

## English

ALCOMD3 2.1.1 is a maintenance release that updates desktop and local MCP
compatibility, refines the default interface, and improves the website's MCP
documentation and download experience.

### Application updates

- Updated the Windows desktop runtime and supporting components for compatibility
  with current Tauri and MCP SDK releases.
- Updated local MCP task handling while retaining task listing, status and result
  queries, cancellation, progress reporting, and lifecycle behavior.
- Increased the default desktop window width from 1300 to 1400 pixels.
- Replaced the generic network icon in the sidebar with the MCP mark so the MCP
  entry is easier to identify.

### Website updates

- Added complete MCP guide pages in Simplified Chinese, Traditional Chinese,
  Japanese, and English. The guides cover setup, client configuration, available
  tools, lifecycle behavior, permission boundaries, and troubleshooting.
- Improved website navigation and language switching for documentation pages, and
  clarified the homepage description of ALCOMD3, VPM management, and optional
  local MCP capabilities.
- Improved search and social-sharing metadata so localized ALCOMD3 pages and MCP
  documentation are easier to discover.

### Installation and upgrade

- The website download button now provides the Release ZIP corresponding to the
  signed Windows installer. In-app updates continue to download and verify the
  signed installer directly.
- Upgrading from ALCOMD3 2.1.0 requires no data migration. Existing application
  data paths and updater identity remain unchanged.

### Compatibility and security

- MCP remains disabled by default and continues to communicate only through the
  local stdio bridge and local IPC; this release does not add a network listener.
- Release assets and updater metadata are verified against the tagged source,
  exact version and filenames, cryptographic signatures, and immutable Release
  attestations before updater metadata is distributed.

## 日本語

ALCOMD3 2.1.1 は、デスクトップアプリとローカル MCP の互換性を更新し、既定の画面構成を
調整するとともに、Web サイトの MCP ドキュメントとダウンロード体験を改善する
メンテナンスリリースです。

### アプリの更新

- Windows デスクトップランタイムと関連コンポーネントを更新し、現行の Tauri および
  MCP SDK との互換性を確保しました。
- ローカル MCP のタスク処理を更新し、タスク一覧、状態と結果の取得、キャンセル、進捗通知、
  ライフサイクルの動作を維持しました。
- デスクトップウィンドウの既定幅を 1300 ピクセルから 1400 ピクセルに広げました。
- サイドバーの汎用ネットワークアイコンを MCP マークに変更し、MCP 項目を見つけやすく
  しました。

### Web サイトの更新

- 簡体字中国語、繁体字中国語、日本語、英語の完全な MCP ガイドページを追加しました。
  有効化手順、クライアント設定、利用可能なツール、ライフサイクル、権限範囲、
  トラブルシューティングを確認できます。
- ドキュメントページのナビゲーションと言語切り替えを改善し、トップページにおける
  ALCOMD3、VPM 管理、任意のローカル MCP 機能の説明を明確にしました。
- 検索およびソーシャル共有用のメタデータを改善し、各言語の ALCOMD3 ページと MCP
  ドキュメントを見つけやすくしました。

### インストールとアップグレード

- Web サイトのダウンロードボタンは、署名済み Windows インストーラーに対応する
  Release ZIP を提供するようになりました。アプリ内更新は引き続き署名済み
  インストーラーを直接ダウンロードして検証します。
- ALCOMD3 2.1.0 からのアップグレードにデータ移行は不要です。既存のアプリデータパスと
  updater identity は変更されません。

### 互換性とセキュリティ

- MCP は引き続き既定で無効で、ローカル stdio bridge とローカル IPC のみを使用します。
  このリリースでネットワークリスナーが追加されることはありません。
- updater metadata の配布前に、Release assets と updater metadata がタグ付きソース、
  正確なバージョンとファイル名、暗号署名、Immutable Release attestation と一致することを
  検証します。

## 中文

ALCOMD3 2.1.1 是一个维护版本，更新桌面端与本地 MCP 的兼容性，调整默认界面，并改进
网站的 MCP 文档和下载体验。

### 应用更新

- 更新 Windows 桌面运行时和相关组件，以兼容当前的 Tauri 与 MCP SDK 版本。
- 更新本地 MCP 任务处理，同时保留任务列表、状态与结果查询、取消、进度报告和生命周期行为。
- 桌面窗口默认宽度由 1300 像素调整为 1400 像素。
- 将侧栏中的通用网络图标替换为 MCP 标志，使 MCP 入口更容易识别。

### 网站更新

- 新增简体中文、繁体中文、日语和英语的完整 MCP 指南页面，涵盖启用方式、客户端配置、
  可用工具、生命周期行为、权限边界和故障排除。
- 改进文档页面的导航与语言切换，并更清楚地介绍 ALCOMD3、VPM 管理和可选的本地 MCP 能力。
- 改进搜索和社交分享元数据，使各语言的 ALCOMD3 页面与 MCP 文档更容易被找到。

### 安装与升级

- 网站下载按钮现在提供与已签名 Windows 安装器对应的 Release ZIP；应用内更新仍会直接
  下载并验证已签名安装器。
- 从 ALCOMD3 2.1.0 升级无需迁移数据，现有应用数据路径和 updater 标识保持不变。

### 兼容性与安全

- MCP 仍默认关闭，并继续仅通过本地 stdio bridge 和本机 IPC 通信；本版本不会新增网络监听。
- 在分发 updater metadata 前，会核验 Release 资产和 updater metadata 是否与 tag 源码、
  精确版本及文件名、加密签名和 Immutable Release attestation 一致。
