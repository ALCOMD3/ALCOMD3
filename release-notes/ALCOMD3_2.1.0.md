# ALCOMD3 v2.1.0

## English

ALCOMD3 2.1.0 is the first stable release of the independent ALCOMD3 product
line. It is a desktop tool for managing VRChat projects and VPM packages, with
project workflows, repositories, templates, backups, logs, updates, and
optional local MCP integration.

### Projects and packages

- Create VRChat projects, register existing projects, and organize them in list
  or grid views with their added dates.
- Copy projects and create or restore backups with visible progress and
  cancellation controls.
- Manage VPM repositories and packages with compatibility checks based on the
  Unity major and minor versions declared by each package.
- Project and repository readers isolate malformed entries so the rest of the
  collection stays available.
- Built-in Unity 2022 Worlds templates include the `UDON` scripting define, and
  custom templates can be duplicated and managed from the template interface.

### Data and import

- ALCOMD3 uses its own data root, with `config/`, `state/`, `templates/`,
  `activity-logs/`, and `technical-logs/` directories for distinct types of
  application data.
- Top-level `settings.json` and `vcc.liteDb` files provide VPM/VCC-compatible
  environment and project data.
- First setup and Settings offer optional manual import from VCC, ALCOM, and
  ALCOMD3 2.1.0 beta data layouts.
- Settings provides shortcuts to the active configuration, log, and template
  locations.

### Logs and diagnostics

- Activity records provide a user-readable history, with persistent display
  preferences and efficient rendering for longer histories.
- Multi-line technical log entries are collapsed by default for easier
  inspection.
- Log queries and returned log text redact token, password, and related
  sensitive fields.

### Local MCP integration

- MCP access is disabled by default and is limited to a local stdio bridge and
  local IPC.
- MCP tools cover project and repository registration; project, repository,
  package, activity-record, and technical-log queries; and selected project
  backup, copy, restore, and package operations.
- Project creation supports cancellation and rollback to avoid leaving partial
  state after an unsuccessful operation.
- The MCP page groups tools by purpose and shows active tool activity.

### Desktop experience, updates, and languages

- The Material Design 3-style interface supports theme customization.
- Stable and beta update channels are available separately.
- The update dialog supports localized summaries, a 7-day remind-later action,
  and a GitHub Release link for the complete release notes.
- The interface uses the configured ALCOMD3 product name and includes the full
  supported locale key set.

### Compatibility and licensing

- Templates, repositories, project configuration, and the `vcc://` workflow
  are compatible with the VPM/VCC ecosystem.
- ALCOMD3 2.1.0 is licensed under AGPL-3.0-or-later.
- Built-in templates are licensed under the MIT License. Third-party resources,
  dependencies, and template assets retain their respective licenses.

### Getting started

- First setup guides the choice of project and backup locations and offers
  optional data import.
- Settings can be used at any time to review paths, templates, repositories,
  update channels, and local MCP access.

## 日本語

ALCOMD3 2.1.0 は、独立した ALCOMD3 製品ラインの最初の安定版です。VRChat
プロジェクトと VPM パッケージを管理するデスクトップツールとして、プロジェクト操作、
リポジトリ、テンプレート、バックアップ、ログ、更新、任意で利用できるローカル MCP
連携を備えています。

### プロジェクトとパッケージ

- VRChat プロジェクトの作成、既存プロジェクトの登録に対応し、追加日時付きのリスト表示
  またはグリッド表示で整理できます。
- プロジェクトのコピー、バックアップの作成と復元に対応し、進行状況の表示とキャンセル操作を
  利用できます。
- 各パッケージが宣言する Unity のメジャーおよびマイナーバージョンを基に互換性を確認し、
  VPM リポジトリとパッケージを管理できます。
- プロジェクトやリポジトリに不正な項目が含まれていても、その項目を分離し、残りの一覧を
  利用できます。
- Unity 2022 Worlds の組み込みテンプレートには `UDON` スクリプト定義が含まれ、
  カスタムテンプレートはテンプレート画面から複製および管理できます。

### データとインポート

- ALCOMD3 は専用のデータルートを使用し、アプリケーションデータの種類ごとに
  `config/`、`state/`、`templates/`、`activity-logs/`、`technical-logs/`
  ディレクトリへ保存します。
- トップレベルの `settings.json` と `vcc.liteDb` は、VPM/VCC 互換の環境データと
  プロジェクトデータを提供します。
- 初回セットアップと設定画面から、VCC、ALCOM、ALCOMD3 2.1.0 ベータ版のデータ配置を
  任意で手動インポートできます。
- 設定画面には、使用中の設定、ログ、テンプレートの保存場所を開くショートカットがあります。

### ログと診断

- アクティビティ記録ではユーザーが読める操作履歴を確認でき、表示設定は保存され、長い履歴も
  効率よく表示されます。
- 複数行の技術ログは既定で折りたたまれ、確認しやすくなっています。
- ログの検索結果と取得したログ本文では、トークン、パスワードなどの機密フィールドが
  マスキングされます。

### ローカル MCP 連携

- MCP アクセスは既定で無効で、ローカルの stdio ブリッジとローカル IPC のみに限定されます。
- MCP ツールは、プロジェクトとリポジトリの登録、プロジェクト、リポジトリ、パッケージ、
  アクティビティ記録、技術ログの照会、および一部のプロジェクトのバックアップ、コピー、
  復元、パッケージ操作に対応します。
- プロジェクト作成はキャンセルとロールバックに対応し、処理に失敗した場合に不完全な状態が
  残ることを防ぎます。
- MCP 画面ではツールを用途別に分類し、実行中のツールを表示します。

### デスクトップ体験、更新、言語

- Material Design 3 スタイルの画面でテーマをカスタマイズできます。
- 安定版とベータ版の更新チャンネルを個別に選択できます。
- 更新ダイアログは、言語別の概要、7 日後に再通知する操作、完全なリリースノートを開く
  GitHub Release リンクに対応します。
- 画面全体で設定された ALCOMD3 製品名を使用し、対応する全ロケールのキーを収録しています。

### 互換性とライセンス

- テンプレート、リポジトリ、プロジェクト設定、`vcc://` ワークフローは VPM/VCC
  エコシステムと互換性があります。
- ALCOMD3 2.1.0 は AGPL-3.0-or-later で提供されます。
- 組み込みテンプレートは MIT License で提供されます。サードパーティーのリソース、依存関係、
  テンプレート素材には、それぞれのライセンスが適用されます。

### はじめに

- 初回セットアップでは、プロジェクトとバックアップの保存場所を選択し、必要に応じてデータを
  インポートできます。
- 設定画面では、パス、テンプレート、リポジトリ、更新チャンネル、ローカル MCP アクセスを
  いつでも確認できます。

## 中文

ALCOMD3 2.1.0 是 ALCOMD3 独立产品线的首个稳定版本。它是一款面向 VRChat 项目与 VPM
软件包管理的桌面工具，提供项目工作流、仓库、模板、备份、日志、更新以及可选的本地 MCP
集成。

### 项目与软件包

- 支持创建 VRChat 项目、登记已有项目，并通过列表或网格视图按添加时间整理项目。
- 支持复制项目以及创建或恢复备份，并提供进度显示和取消操作。
- 支持管理 VPM 仓库与软件包，并根据软件包声明的 Unity 主版本和次版本检查兼容性。
- 项目或仓库中存在异常条目时，会将其单独隔离，其他内容仍可正常使用。
- Unity 2022 Worlds 内置模板包含 `UDON` 脚本定义；自定义模板可在模板界面中复制和管理。

### 数据与导入

- ALCOMD3 使用独立的数据根目录，并通过 `config/`、`state/`、`templates/`、
  `activity-logs/` 和 `technical-logs/` 分别存放不同类型的应用数据。
- 顶层 `settings.json` 和 `vcc.liteDb` 提供与 VPM/VCC 兼容的环境和项目数据。
- 首次设置和设置页面可按需手动导入 VCC、ALCOM 以及 ALCOMD3 2.1.0 beta 布局的数据。
- 设置页面提供当前配置、日志和模板位置的快捷入口。

### 日志与诊断

- 活动记录提供用户可读的操作历史，支持持久化显示偏好，并可高效展示较长的历史记录。
- 多行技术日志默认折叠，便于检查和浏览。
- 日志查询结果与返回的日志文本会对 token、password 等敏感字段进行脱敏。

### 本地 MCP 集成

- MCP 默认关闭，仅通过本地 stdio bridge 和本机 IPC 工作。
- MCP 工具支持登记项目与仓库，查询项目、仓库、软件包、活动记录和技术日志，以及执行部分
  项目备份、复制、恢复与软件包操作。
- 项目创建支持取消和回滚，避免失败操作留下不完整状态。
- MCP 页面按用途对工具分组，并显示正在活动的工具。

### 桌面体验、更新与语言

- Material Design 3 风格的界面支持主题自定义。
- stable 与 beta 更新渠道可分别选择。
- 更新弹窗支持多语言摘要、“7 天后再提醒”，并提供 GitHub Release 链接以查看完整发布说明。
- 界面统一使用配置中的 ALCOMD3 产品名，并覆盖全部受支持语言的 locale key。

### 兼容性与许可证

- 模板、仓库、项目配置以及 `vcc://` 工作流与 VPM/VCC 生态兼容。
- ALCOMD3 2.1.0 使用 AGPL-3.0-or-later 许可证。
- 内置模板使用 MIT License；第三方资源、依赖和模板素材分别遵循各自的许可证。

### 开始使用

- 首次设置会引导选择项目和备份位置，并可按需导入数据。
- 设置页面可随时检查路径、模板、仓库、更新渠道和本地 MCP 访问状态。
