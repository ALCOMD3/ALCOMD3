# ALCOMD3 v2.2.0-beta.2

## English

This beta corrects repository sorting interactions and gives ALCOMD3 a dedicated Windows installer identity with a clean migration from installations that used the former shared identity.

### Application updates

- Repository order is now saved when sorting mode is closed instead of after every drag. The drag preview retains package details, and temporary sorting rows no longer expose actions that cannot be completed.

### Website updates

- This version has no user-visible website changes.

### Installation and upgrade

- On Windows, upgrading removes the installation registered under the former shared AppId—whether its main executable is `ALCOM.exe` or `ALCOMD3.exe`—before installing ALCOMD3 under its new dedicated AppId.
- Existing ALCOMD3 user data is preserved during this installer identity migration.

### Compatibility and security

- The new installation is stopped if the old AppId registration or known legacy executables cannot be removed, preventing two independently managed ALCOMD3 installations from being left behind.

## 日本語

このベータ版では、リポジトリの並べ替え操作を修正し、ALCOMD3 に Windows 専用のインストーラー ID を導入して、以前の共有 ID を使用するインストールから確実に移行します。

### アプリの更新

- リポジトリの順序はドラッグのたびではなく、並べ替えモードを終了したときに保存されます。ドラッグ中のプレビューにもパッケージ情報を保持し、一時的な行では完了できない操作を無効にしました。

### Web サイトの更新

- このバージョンには、ユーザーに見える Web サイトの変更はありません。

### インストールとアップグレード

- Windows では、メイン実行ファイルが `ALCOM.exe` と `ALCOMD3.exe` のどちらでも、以前の共有 AppId に登録されたインストールを削除してから、ALCOMD3 を新しい専用 AppId でインストールします。
- このインストーラー ID の移行では、既存の ALCOMD3 ユーザーデータを保持します。

### 互換性とセキュリティ

- 旧 AppId の登録または既知の旧実行ファイルを削除できない場合は新規インストールを中止し、個別に管理される 2 つの ALCOMD3 が残らないようにします。

## 中文

此测试版修复了仓库排序交互，并为 ALCOMD3 启用独立的 Windows 安装身份，可从使用旧共享身份的安装中干净迁移。

### 应用更新

- 仓库顺序现在会在退出排序模式时保存，而不是每次拖动后立即写入。拖动预览会保留软件包信息，临时排序行也不再提供无法完成的操作。

### 网站更新

- 此版本没有用户可见的网站变化。

### 安装与升级

- 在 Windows 上升级时，会先卸载注册在旧共享 AppId 下的安装——无论其主程序是 `ALCOM.exe` 还是 `ALCOMD3.exe`——再使用新的 ALCOMD3 专属 AppId 安装。
- 安装身份迁移会保留现有 ALCOMD3 用户数据。

### 兼容性与安全

- 如果旧 AppId 注册项或已知旧程序文件无法删除，新安装会中止，避免留下两个需要独立管理的 ALCOMD3。
