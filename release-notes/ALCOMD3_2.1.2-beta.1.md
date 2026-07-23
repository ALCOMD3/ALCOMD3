# ALCOMD3 v2.1.2-beta.1

## English

### Changes

#### Multi-platform distribution

- ALCOMD3 is now available for Windows x64, Apple Silicon macOS, and Linux x64.
- Release filenames now include the operating system and architecture so packages
  can be distinguished before downloading.
- The website can recommend the appropriate package for the visitor's operating
  system while keeping every available package visible.
- Windows installation continues to display the application as `ALCOMD3` without
  including the version in the registered application name.

#### Installation and updates

- Windows is distributed as a ZIP containing the signed installer.
- Apple Silicon macOS is distributed as an ad-hoc signed DMG. It is not
  notarized by Apple.
- Linux x64 is available as an AppImage or DEB package.
- Beta-channel in-app updates verify signed updater packages on Windows, Apple
  Silicon macOS, and Linux AppImage installations. DEB installations remain under
  the system package manager's control.

### Notes

- This is a beta-channel release. The stable channel remains on ALCOMD3 2.1.1
  until the next stable version is published.
- Intel Macs are not supported by this beta.
- On first launch, macOS may block the application. Continue with **Open
  Anyway** in System Settings → Privacy & Security only after confirming the
  download came from the official ALCOMD3 website or GitHub Release.
- ALCOMD3 updater packages remain separately signed and verified; ad-hoc macOS
  code signing does not disable updater signature verification.
- No application data migration is required when upgrading from ALCOMD3 2.1.1.

## 日本語

### 変更点

#### マルチプラットフォーム配布

- ALCOMD3 を Windows x64、Apple Silicon macOS、Linux x64 で利用できるように
  なりました。
- ダウンロード前にパッケージを区別できるよう、Release のファイル名に OS と
  アーキテクチャを含めるようにしました。
- Web サイトは、すべてのパッケージを表示したまま、閲覧環境に適したパッケージを
  推奨できるようになりました。
- Windows の登録済みアプリ名は、バージョンを含めず引き続き `ALCOMD3` と表示されます。

#### インストールとアップデート

- Windows 版は、署名済みインストーラーを含む ZIP として配布されます。
- Apple Silicon macOS 版は ad-hoc 署名済み DMG として配布され、Apple の
  notarization は受けていません。
- Linux x64 版は AppImage と DEB パッケージを選択できます。
- beta channel のアプリ内アップデートは、Windows、Apple Silicon macOS、Linux
  AppImage の署名済み updater package を検証します。DEB 版の更新は引き続き
  system package manager が管理します。

### 注意事項

- これは beta channel のリリースです。次の stable version が公開されるまで、stable
  channel は ALCOMD3 2.1.1 のままです。
- この beta は Intel Mac をサポートしていません。
- 初回起動時は macOS にブロックされる場合があります。ALCOMD3 公式 Web サイトまたは
  GitHub Release からのダウンロードであることを確認した場合のみ、「システム設定 →
  プライバシーとセキュリティ」で「このまま開く」を選択してください。
- ALCOMD3 updater package は別途署名・検証されます。macOS の ad-hoc code signing に
  よって updater signature verification が無効になることはありません。
- ALCOMD3 2.1.1 からのアップグレードにデータ移行は不要です。

## 中文

### 变化

#### 多平台分发

- ALCOMD3 现在可用于 Windows x64、Apple Silicon macOS 和 Linux x64。
- Release 文件名现在包含操作系统和架构，下载前即可区分不同平台的软件包。
- 网站可以根据访问者的操作系统推荐合适的软件包，同时仍然显示全部可用软件包。
- Windows 注册应用名称继续仅显示为 `ALCOMD3`，不会附加版本号。

#### 安装与更新

- Windows 版以包含已签名安装器的 ZIP 形式分发。
- Apple Silicon macOS 版以 ad-hoc 签名的 DMG 形式分发，未经 Apple 公证。
- Linux x64 版提供 AppImage 和 DEB 两种软件包。
- beta 通道的应用内更新会在 Windows、Apple Silicon macOS 和 Linux AppImage
  安装中验证已签名的 updater 软件包；DEB 安装仍由系统包管理器负责更新。

### 注意事项

- 这是 beta 通道版本；在下一个稳定版发布前，stable 通道继续保持 ALCOMD3 2.1.1。
- 此 beta 暂不支持 Intel Mac。
- 首次启动时，macOS 可能阻止应用运行。请仅在确认下载来自 ALCOMD3 官方网站或
  GitHub Release 后，前往“系统设置 → 隐私与安全性”选择“仍要打开”。
- ALCOMD3 updater 软件包仍会单独进行签名和验证；macOS ad-hoc 代码签名不会关闭
  updater 签名校验。
- 从 ALCOMD3 2.1.1 升级无需迁移应用数据。
