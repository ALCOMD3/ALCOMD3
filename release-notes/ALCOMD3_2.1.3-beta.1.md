# ALCOMD3 v2.1.3-beta.1

## English

ALCOMD3 2.1.3-beta.1 is a compatibility-focused beta that improves installation
on clean Windows systems and AppImage startup on Fedora Linux.

### Fixes

- The Windows installer now explicitly extracts and launches the bundled
  Microsoft Edge WebView2 bootstrapper when the runtime is not already
  installed.
- The Linux AppImage now bundles the required `libjpeg` and `libbz2` shared
  libraries instead of assuming they are provided by the host, fixing
  missing-library startup failures observed on Fedora.

### Compatibility and notes

- This is a beta-channel release. The stable channel remains on ALCOMD3 2.1.2.
- The AppImage dependency fix does not change DEB update behavior. AppImage
  supports signed in-app updates; DEB continues to be updated through the
  system package manager.
- Intel Macs are not supported.
- The macOS build is ad-hoc signed but is not notarized by Apple. On first
  launch, macOS may require **Open Anyway** in System Settings → Privacy &
  Security after confirming the download came from the official ALCOMD3 website
  or GitHub Release. Follow [Apple's official instructions][apple-gatekeeper]
  for the system steps.
- Updater packages remain independently signed and verified; macOS ad-hoc code
  signing does not replace updater signature verification.
- No application data migration is required when upgrading from ALCOMD3 2.1.2
  or 2.1.2-beta.1.

## 日本語

ALCOMD3 2.1.3-beta.1 は、WebView2 が未導入の Windows 環境での
インストールと、Fedora Linux での AppImage 起動互換性を改善する beta release です。

### 修正

- Microsoft Edge WebView2 Runtime が未導入の場合、Windows installer が内蔵
  bootstrapper を明示的に展開して起動するよう修正しました。
- Linux AppImage がホスト側の提供を前提とせず、必要な `libjpeg` と `libbz2` の
  shared library を同梱するようにし、Fedora で確認された library 不足による
  起動失敗を修正しました。

### 互換性と注意事項

- これは beta channel のリリースです。stable channel は ALCOMD3 2.1.2 のままです。
- AppImage の依存関係修正によって DEB の更新方法は変わりません。AppImage は署名済み
  アプリ内アップデートに対応し、DEB は引き続き system package manager で更新します。
- Intel Mac はサポートされません。
- macOS 版は ad-hoc 署名済みですが、Apple の notarization は受けていません。公式
  ALCOMD3 Web サイトまたは GitHub Release からのダウンロードであることを確認した場合のみ、
  初回起動時に「システム設定 → プライバシーとセキュリティ」で「このまま開く」を
  選択してください。操作手順は [Apple 公式ガイド][apple-gatekeeper]を参照してください。
- updater package は別途署名・検証されます。macOS の ad-hoc code signing が updater
  signature verification の代わりになることはありません。
- ALCOMD3 2.1.2 または 2.1.2-beta.1 からのアップグレードにデータ移行は不要です。

## 中文

ALCOMD3 2.1.3-beta.1 是以兼容性修复为主的测试版，改善未预装 WebView2 的
Windows 环境安装流程，以及 Fedora Linux 上的 AppImage 启动兼容性。

### 修复

- Microsoft Edge WebView2 Runtime 尚未安装时，Windows 安装器现在会显式解压并启动
  内嵌的 bootstrapper。
- Linux AppImage 现在会包含所需的 `libjpeg` 和 `libbz2` 共享库，不再假设系统已提供
  这些依赖，修复 Fedora 上出现的缺少共享库启动失败问题。

### 兼容性与注意事项

- 这是 beta 通道版本；stable 通道继续保持 ALCOMD3 2.1.2。
- AppImage 依赖修复不会改变 DEB 的更新方式。AppImage 支持已签名的应用内更新；
  DEB 继续通过系统包管理器更新。
- 本版本不支持 Intel Mac。
- macOS 版经过 ad-hoc 签名，但未经 Apple 公证。请仅在确认下载来自 ALCOMD3 官方网站或
  GitHub Release 后，在首次启动时前往“系统设置 → 隐私与安全性”选择“仍要打开”；具体操作请
  参阅 [Apple 官方指南][apple-gatekeeper]。
- updater 软件包仍会独立进行签名和验证；macOS ad-hoc 代码签名不能替代 updater
  签名校验。
- 从 ALCOMD3 2.1.2 或 2.1.2-beta.1 升级无需迁移应用数据。

[apple-gatekeeper]: https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac
