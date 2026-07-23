# ALCOMD3 v2.2.0-beta.3

## English

This beta completes ALCOMD3's Windows identity migration by restoring replacement shortcuts correctly, removing the former shared WebView profile, and giving ALCOMD3 a stable Windows Shell identity.

### Application updates

- This version has no additional user-visible application changes.

### Website updates

- This version has no user-visible website changes.

### Installation and upgrade

- Upgrading from the former shared Windows installer identity now preserves whether a desktop shortcut existed, removes shortcuts that target the previous installation, and creates replacement shortcuts that point to `ALCOMD3.exe`.
- The installer removes the legacy WebView profile used by the former shared Tauri identity before installing with ALCOMD3's dedicated identity.
- ALCOMD3 now uses the stable `CQMHV.ALCOMD3` Windows AppUserModelID for taskbar grouping, installer shortcuts, template files, and `vcc://` links.

### Compatibility and security

- Existing ALCOMD3 settings, projects, backups, and other application data remain preserved during the identity migration. Same-named shortcuts that do not target the previous installation are left unchanged.

## 日本語

このベータ版では、代替ショートカットを正しく復元し、以前の共有 WebView プロファイルを削除して、ALCOMD3 に安定した Windows Shell ID を設定することで、Windows の ID 移行を完了します。

### アプリの更新

- このバージョンには、その他のユーザーに見えるアプリの変更はありません。

### Web サイトの更新

- このバージョンには、ユーザーに見える Web サイトの変更はありません。

### インストールとアップグレード

- 以前の共有 Windows installer ID からアップグレードするとき、デスクトップショートカットが存在したかどうかを保持し、旧インストールを指すショートカットを削除して、`ALCOMD3.exe` を指す代替ショートカットを作成するようになりました。
- ALCOMD3 専用 ID でインストールする前に、以前の共有 Tauri ID が使用していた旧 WebView プロファイルを削除します。
- タスクバーのグループ化、installer shortcut、template file、`vcc://` link に、安定した Windows AppUserModelID `CQMHV.ALCOMD3` を使用するようになりました。

### 互換性とセキュリティ

- ID 移行中も既存の ALCOMD3 の設定、プロジェクト、バックアップ、その他のアプリデータを保持します。旧インストールを指していない同名のショートカットは変更しません。

## 中文

此测试版通过正确恢复替代快捷方式、删除原共享 WebView 配置目录并为 ALCOMD3 设置稳定的 Windows Shell 身份，完成 Windows 身份迁移。

### 应用更新

- 此版本没有其他用户可见的应用变化。

### 网站更新

- 此版本没有用户可见的网站变化。

### 安装与升级

- 从原共享 Windows 安装身份升级时，现在会保留此前是否存在桌面快捷方式的选择，删除指向旧安装的快捷方式，并创建指向 `ALCOMD3.exe` 的替代快捷方式。
- 使用 ALCOMD3 专属身份安装前，安装器会删除原共享 Tauri 身份使用的旧 WebView 配置目录。
- ALCOMD3 现在为任务栏分组、安装器快捷方式、模板文件和 `vcc://` 链接统一使用稳定的 Windows AppUserModelID `CQMHV.ALCOMD3`。

### 兼容性与安全

- 身份迁移期间会保留现有 ALCOMD3 设置、项目、备份及其他应用数据；名称相同但并未指向旧安装的快捷方式不会被修改。
