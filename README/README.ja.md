<p align="center">
    <img src="https://alcomd3.cqmhv.com/assets/logo.png" alt="ALCOMD3 ロゴ" width="160">
</p>

<h1 align="center">ALCOMD3</h1>

<p align="center">
    VRChat Unity プロジェクトと VPM パッケージを管理する、オープンソースの VRChat Creator Companion 代替。
</p>

<p align="center">
    <a href="https://alcomd3.cqmhv.com/">ダウンロード</a> ·
    <a href="https://github.com/ALCOMD3/ALCOMD3/releases">リリース</a> ·
    <a href="../docs/README/README.ja.md">ドキュメント</a>
</p>

<p align="center">
    <a href="../README.md">English</a> · 日本語 ·
    <a href="./README.zh-TW.md">繁體中文</a> ·
    <a href="./README.zh-CN.md">简体中文</a>
</p>

## VRChat プロジェクトを一か所で管理

ALCOMD3 は、プロジェクトフォルダーやパッケージファイルを手作業で整理することなく、
VRChat Unity プロジェクトの日常作業をまとめて扱えるようにします。

- **プロジェクトを管理：**一つのデスクトップアプリでプロジェクトの作成、登録、
  コピー、バックアップ、復元を行えます。
- **VPM パッケージを操作：**リポジトリを参照し、各プロジェクトのパッケージを
  インストール、削除、更新できます。
- **使い慣れたリンクに対応：**VCC 互換の `vcc://` リンクを開き、同じワークフローで
  リポジトリを管理できます。
- **自分好みの外観：**Material Design 3 風の UI で、ライト、ダーク、
  カスタムテーマを選べます。
- **自分の AI ツールと連携：**必要に応じて、MCP 対応クライアントに ALCOMD3 の
  プロジェクト、パッケージ、リポジトリ、ログへの限定的なローカルアクセスを提供できます。

## はじめる

1. [ALCOMD3 公式サイト](https://alcomd3.cqmhv.com/)で stable または beta
   チャンネルを選びます。
2. 使用中の OS に合うパッケージをダウンロードし、インストールまたは起動します。
3. 既存の VRChat Unity プロジェクトを追加するか新しく作成し、ALCOMD3 で
   パッケージ、リポジトリ、バックアップを管理します。

公式ビルドは Windows x64、Apple Silicon 搭載 Mac、Linux x64 に対応しています。
公開済みの全バージョンは
[GitHub Releases](https://github.com/ALCOMD3/ALCOMD3/releases) でも確認できます。

アプリ内更新に対応している場合、ALCOMD3 は既定で起動時に署名済みアップデートを確認し、
利用可能な更新をダウンロードして次回起動前にインストールします。
この動作は設定で無効にできます。

## 任意のローカル MCP 連携

ALCOMD3 は、MCP 対応 AI クライアントを、範囲を限定したプロジェクト、
リポジトリ、パッケージ、環境設定、アクティビティ記録、技術ログのデータに接続できます。
また、限定的なプロジェクトおよびパッケージ操作も提供します。

MCP は既定で無効です。有効化後はローカル stdio bridge と private な loopback-only IPC
を使用します。GUI internal IPC は `127.0.0.1` のみ listen し、public network address
では listen しません。設定、利用可能な tools、権限境界については
[MCP ガイド](../docs/mcp/mcp.ja.md) を参照してください。

## プロジェクトとコミュニティ

ALCOMD3 は ALCOM/vrc-get を起源とし、現在は独立したオープンソースプロジェクトとして
保守されています。VRChat または VCC の公式製品ではありません。

- 不具合やアイデアがありますか？
  [Issue を作成](https://github.com/ALCOMD3/ALCOMD3/issues)してください。
- コントリビュートする場合は[コントリビューションガイド](../CONTRIBUTING/CONTRIBUTING.ja.md)を
  お読みください。
- 技術情報や保守資料は[ドキュメント索引](../docs/README/README.ja.md)から確認できます。
- バージョンごとの変更は
  [GitHub のリリースノート](https://github.com/ALCOMD3/ALCOMD3/releases)で確認できます。

## 開発に参加する

開発環境には Rust stable toolchain、Node.js、npm、および対象プラットフォームの
ビルドツールが必要です。

デスクトップアプリを開発モードで実行する場合：

```powershell
cd vrc-get-gui
npm run tauri dev
```

フロントエンドのみを開発する場合：

```powershell
cd vrc-get-gui
npm run dev
```

ビルド、テスト、保守、リリースに関する資料は
[ドキュメント索引](../docs/README/README.ja.md)にまとめています。

## ライセンス

主要なプロジェクトコードは GNU Affero General Public License v3.0 or later
（`AGPL-3.0-or-later`）でライセンスされています。[LICENSE](../LICENSE) と
[LICENSE-NOTES.md](../LICENSE-NOTES.md) を参照してください。

依存関係と第三者リソースの通知は、アプリ内の Licenses ページと
[vrc-get-gui/THIRD-PARTY.md](../vrc-get-gui/THIRD-PARTY.md) で確認できます。
