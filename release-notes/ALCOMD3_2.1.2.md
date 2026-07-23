# ALCOMD3 v2.1.2

## English

ALCOMD3 2.1.2 brings the stable channel to Windows x64, Apple Silicon macOS,
and Linux x64, with signed updater packages and a dedicated multi-platform
download page.

### Multi-platform distribution

- Added stable packages for Windows x64, Apple Silicon macOS, and Linux x64.
- Release filenames include the operating system and architecture so packages
  can be distinguished before downloading.
- Windows is distributed as a ZIP containing the installer, macOS as an ad-hoc
  signed DMG, and Linux as AppImage and DEB packages.
- Windows installation continues to register the application as `ALCOMD3`
  without appending the version to its display name.

### Installation and updates

- Stable-channel in-app updates verify signed packages on Windows, Apple
  Silicon macOS, and Linux AppImage installations.
- Linux DEB installations remain under the system package manager's control and
  do not apply AppImage self-updates.
- Linux package handling now preserves ownership and semantics for files managed
  through `dpkg-divert`.

### Website

- Moved platform downloads to a dedicated localized download page while keeping
  the homepage focused on the product overview.
- Added operating-system recommendations while retaining all published package
  choices, and standardized the platform download button layout.
- Stable 2.1.2 downloads are derived from the signed stable updater manifest;
  the legacy 2.1.1 Release remains a historical page without filename aliases.

### Compatibility and notes

- Intel Macs are not supported in this release.
- The macOS build is ad-hoc signed but is not notarized by Apple. On first
  launch, macOS may require **Open Anyway** in System Settings → Privacy &
  Security after confirming the download came from the official ALCOMD3 website
  or GitHub Release. Follow [Apple's official instructions][apple-gatekeeper]
  for the system steps.
- Updater packages remain independently signed and verified; macOS ad-hoc code
  signing does not replace updater signature verification.
- No application data migration is required when upgrading from ALCOMD3 2.1.1
  or 2.1.2-beta.1.

## 日本語

ALCOMD3 2.1.2 では、stable channel が Windows x64、Apple Silicon macOS、
Linux x64 に対応し、署名済み updater package とマルチプラットフォーム専用の
ダウンロードページを利用できるようになりました。

### マルチプラットフォーム配布

- Windows x64、Apple Silicon macOS、Linux x64 向けの stable package を追加しました。
- ダウンロード前に区別できるよう、Release のファイル名に OS とアーキテクチャを
  含めました。
- Windows 版は installer を含む ZIP、macOS 版は ad-hoc 署名済み DMG、Linux 版は
  AppImage と DEB package として配布します。
- Windows の登録済みアプリ名は、バージョンを付けず `ALCOMD3` のまま表示されます。

### インストールとアップデート

- stable channel のアプリ内アップデートは、Windows、Apple Silicon macOS、Linux
  AppImage の署名済み package を検証します。
- Linux DEB 版は引き続き system package manager が管理し、AppImage の self-update
  は適用しません。
- Linux の package 処理で、`dpkg-divert` が管理するファイルの所有権と動作を維持する
  ようにしました。

### Web サイト

- 各言語の専用ダウンロードページに platform package を移動し、トップページは製品概要に
  集中する構成に戻しました。
- 公開済み package をすべて表示したまま OS に応じた推奨を行い、platform download
  button のレイアウトを統一しました。
- stable 2.1.2 のリンクは署名済み stable updater manifest から生成されます。旧 2.1.1
  Release はファイル名 alias を追加せず、履歴ページとして維持されます。

### 互換性と注意事項

- Intel Mac はこのリリースではサポートされません。
- macOS 版は ad-hoc 署名済みですが、Apple の notarization は受けていません。公式
  ALCOMD3 Web サイトまたは GitHub Release からのダウンロードであることを確認した場合のみ、
  初回起動時に「システム設定 → プライバシーとセキュリティ」で「このまま開く」を選択してください。
  操作手順は [Apple 公式ガイド][apple-gatekeeper]を参照してください。
- updater package は別途署名・検証されます。macOS の ad-hoc code signing が updater
  signature verification の代わりになることはありません。
- ALCOMD3 2.1.1 または 2.1.2-beta.1 からのアップグレードにデータ移行は不要です。

## 中文

ALCOMD3 2.1.2 将稳定通道扩展到 Windows x64、Apple Silicon macOS 和 Linux x64，
提供已签名的更新包与独立的多平台下载页面。

### 多平台分发

- 新增 Windows x64、Apple Silicon macOS 和 Linux x64 稳定版软件包。
- Release 文件名包含操作系统与架构，下载前即可区分不同平台的软件包。
- Windows 版以包含安装器的 ZIP 分发，macOS 版使用 ad-hoc 签名的 DMG，Linux 版提供
  AppImage 和 DEB 软件包。
- Windows 注册应用名称继续仅显示为 `ALCOMD3`，不会附加版本号。

### 安装与更新

- 稳定通道的应用内更新会验证 Windows、Apple Silicon macOS 和 Linux AppImage 的
  已签名更新包。
- Linux DEB 版继续由系统包管理器负责更新，不会应用 AppImage 自更新。
- Linux 软件包处理会保留由 `dpkg-divert` 管理的文件所有权与转移语义。

### 网站

- 将平台下载移至独立的多语言下载页面，主页恢复为以产品介绍为主的布局。
- 在保留全部已发布软件包选项的同时根据操作系统给出推荐，并统一各平台下载按钮样式。
- 稳定版 2.1.2 下载链接由已签名的 stable updater manifest 生成；旧 2.1.1 Release
  继续作为历史页面保留，不增加文件名兼容别名。

### 兼容性与注意事项

- 本版本不支持 Intel Mac。
- macOS 版经过 ad-hoc 签名，但未经 Apple 公证。请仅在确认下载来自 ALCOMD3 官方网站或
  GitHub Release 后，在首次启动时前往“系统设置 → 隐私与安全性”选择“仍要打开”；具体操作请
  参阅 [Apple 官方指南][apple-gatekeeper]。
- updater 软件包仍会独立进行签名和验证；macOS ad-hoc 代码签名不能替代 updater
  签名校验。
- 从 ALCOMD3 2.1.1 或 2.1.2-beta.1 升级无需迁移应用数据。

[apple-gatekeeper]: https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac
