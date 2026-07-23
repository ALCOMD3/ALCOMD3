# ALCOMD3 v2.2.0-beta.1

## English

This beta improves everyday project, package, repository, and activity-log workflows, and introduces resilient automatic updates that install on the next launch.

### Application updates

- Shift-click now selects a contiguous package range in Manage Packages, making bulk updates faster.
- Backup archives and restored projects can be named, and VPM package exclusion can be chosen separately for each backup.
- Repository order is now changed in an explicit sorting mode, with drag handles shown only while sorting.
- Extensions now has a dedicated sidebar entry, and Activity Records controls have been reorganized for easier filtering and folder access.

### Website updates

- Download cards now use platform icons and a more consistent layout. Only the matching stable-platform card receives visual emphasis; stable and beta remain explicit choices, and beta is presented neutrally.

### Installation and upgrade

- Automatic updates are enabled by default. Eligible signed updates download quietly and install on the next launch before the main window or MCP starts.
- Disabling automatic updates keeps the existing manual update prompt, and manually started updates remain available for immediate installation.
- Update recovery now preserves startup links and project-template requests across the restart, cleans orphaned temporary downloads, and uses bounded handoff retries.

### Compatibility and security

- Downloaded update assets are bound to the expected release version and Minisign metadata, with download-size limits and transactional staging protection.
- A failed installation of the same version and channel no longer causes a restart loop. Transient I/O failures preserve the staged package for the next launch, while newer versions, channel or setting changes, and manual retries remain eligible.

## 日本語

このベータ版では、プロジェクト、パッケージ、リポジトリ、アクティビティログの日常的な操作を改善し、次回起動時にインストールされる復旧性の高い自動更新を導入しました。

### アプリの更新

- 「パッケージを管理」で Shift クリックによる連続範囲選択に対応し、一括更新をすばやく行えるようになりました。
- バックアップアーカイブと復元後のプロジェクトに名前を指定できるようになり、VPM パッケージを除外するかどうかをバックアップごとに選択できます。
- リポジトリの順序変更を明示的な並べ替えモードで行うようにし、ドラッグハンドルは並べ替え中だけ表示されます。
- 「拡張機能」をサイドバーから直接開けるようになり、アクティビティ記録のフィルターとフォルダー操作を使いやすく整理しました。

### Web サイトの更新

- ダウンロードカードにプラットフォームアイコンを追加し、レイアウトを統一しました。利用中のプラットフォームに一致する安定版カードだけを強調し、安定版とベータ版は明示的に選択でき、ベータ版は中立的に表示されます。

### インストールとアップグレード

- 自動更新を既定で有効にしました。対象となる署名済み更新をバックグラウンドでダウンロードし、次回起動時にメインウィンドウや MCP より先にインストールします。
- 自動更新を無効にした場合は従来の手動更新ダイアログを維持し、手動で開始した更新は引き続きすぐにインストールできます。
- 更新後の再起動をまたいで起動時のリンクやプロジェクトテンプレート要求を保持し、孤立した一時ダウンロードを削除して、引き継ぎの再試行回数を制限するようにしました。

### 互換性とセキュリティ

- ダウンロードした更新ファイルを想定されたリリースバージョンと Minisign メタデータに結び付け、ダウンロードサイズ制限とトランザクション形式のステージング保護を追加しました。
- 同じバージョンとチャンネルのインストール失敗による再起動ループを防止しました。一時的な I/O エラーでは次回起動用に更新ファイルを保持し、新しいバージョン、チャンネルや設定の変更、手動再試行は引き続き実行できます。

## 中文

此测试版改进了日常的项目、软件包、仓库和活动日志操作，并引入了可在下次启动时安装、具备可靠恢复能力的自动更新。

### 应用更新

- “管理软件包”现在支持按住 Shift 单击来选择连续范围，可更快地执行批量更新。
- 备份归档和恢复后的项目均可命名，并可在每次备份时单独选择是否排除 VPM 软件包。
- 仓库顺序现在通过明确的排序模式调整，拖动手柄仅在排序期间显示。
- “扩展”现在拥有独立的侧边栏入口；“活动记录”的筛选和文件夹操作也经过重新整理，更易使用。

### 网站更新

- 下载卡片新增平台图标并统一布局。仅与当前平台匹配的稳定版卡片会获得视觉强调；稳定版和测试版仍需明确选择，测试版保持中性展示。

### 安装与升级

- 自动更新现在默认启用。符合条件且经过签名的更新会在后台下载，并在下次启动时先于主窗口或 MCP 安装。
- 关闭自动更新后仍使用现有的手动更新提示；手动发起的更新仍可立即安装。
- 更新恢复现在会跨重启保留启动链接和项目模板请求、清理遗留的临时下载，并限制交接重试次数。

### 兼容性与安全

- 下载的更新资源会与预期发布版本及 Minisign 元数据绑定，并受到下载大小限制和事务式暂存保护。
- 同一版本和通道安装失败后不再形成重启循环。遇到暂时性 I/O 错误时会保留暂存包并在下次启动重试；新版本、通道或设置变更以及手动重试仍可正常进行。
