# ALCOMD3 v3.0.0-beta.2

## English

This beta separates website publishing from the application source repository and simplifies the updater metadata path while keeping the public website, update channels, and application behavior unchanged.

### Application updates

- No user-visible application changes in this release.

### Website updates

- Website source and deployment are now maintained in the dedicated ALCOMD3-Website repository without changing the public domain or download pages.

### Installation and upgrade

- Verified updater metadata is now published directly to the website repository; stable and beta update endpoint URLs remain unchanged.

### Compatibility and security

- No compatibility changes; updater metadata is still validated against the published release assets and signed updater payloads before publication.

## 日本語

このベータ版では、公開 Web サイト、更新チャンネル、アプリケーションの動作を変更せずに、Web サイトの公開処理をアプリケーションのソースリポジトリから分離し、updater metadata の公開経路を簡素化しました。

### アプリの更新

- このリリースにユーザー向けのアプリ変更はありません。

### Web サイトの更新

- 公開ドメインとダウンロードページを変更せず、Web サイトのソースとデプロイを専用の ALCOMD3-Website リポジトリで管理するようにしました。

### インストールとアップグレード

- 検証済み updater metadata を Web サイトリポジトリへ直接公開するようにしました。stable と beta の更新 endpoint URL は変更されません。

### 互換性とセキュリティ

- 互換性の変更はありません。updater metadata は公開前に、公開済み Release assets と署名済み updater payloads に対して引き続き検証されます。

## 中文

此 Beta 版本将网站发布流程从应用源码仓库中分离，并简化 updater 元数据发布路径，同时保持公开网站、更新通道和应用行为不变。

### 应用更新

- 本版本没有面向用户的应用变化。

### 网站更新

- 网站源码和部署现由独立的 ALCOMD3-Website 仓库维护，公开域名和下载页面保持不变。

### 安装与升级

- 经过验证的 updater 元数据现在直接发布到网站仓库；stable 和 beta 更新端点地址保持不变。

### 兼容性与安全

- 本版本没有兼容性变化；updater 元数据在发布前仍会根据已公开的 Release 资产和已签名 updater payload 进行验证。
