# Contributing

言語: [English](../CONTRIBUTING.md) | 日本語 | [简体中文](CONTRIBUTING.zh-CN.md)

最初に [project-wide contribution guide](../../CONTRIBUTING/CONTRIBUTING.ja.md) を読む。

### コントリビューションのライセンス

明示的に別の license が記載され、maintainer がそれを受け入れた場合を除き、
GUI contribution は ALCOMD3 の現在の main project license である
`AGPL-3.0-or-later` で提供されます。

### ローカライズ

ALCOMD3 は internationalized されている。ユーザーに見えるテキストを追加する場合は、
文字列をハードコードせず、既存の localization key を使うか新しい key を追加し、i18n を使う。

新しい localization key を追加する場合は、`locales/en.json5` に English の値を追加する。
他の言語を理解している場合は値を追加してよい。理解していない場合は、その言語の maintainer に任せる。

### 言語追加

新しい言語の追加を歓迎する。追加手順:

1. リポジトリを fork し、新しい言語用の branch を作る。
2. `locales/` に language code の JSON5 ファイルを作る。
   - 例: 日本語は `ja.json5`。
3. `lib/i18n.ts` で新しい JSON5 ファイルを import し、`languageResources` object に追加する。
4. draft pull request を作る。
5. 新しい言語追加を user-facing release coverage として記録する必要がある場合は、release notes または関連 documentation を更新する。
6. pull request を ready for review にする。
7. maintainer がその言語を保守できるか確認する。
   保守しない場合、別の maintainer が現れるまでその言語は merge されない。
8. localization tracking discussion や automation がそのタスクで保守されている場合、maintainer が更新する。
9. pull request で Allow edits from maintainers を有効にする。
10. review 後、maintainer が pull request を merge する。

### ローカライズ指針

- ALCOMD3 は cross-platform なので、OS に依存しない表現を優先する。
  - 必要な場合は "Folder" より "Directory" を優先する。
- 技術用語は常に翻訳する必要はない。
  - 分かりやすい場合は English term や一般的な transliteration を使ってよい。
- 逐語訳にこだわらず、ユーザーが理解しやすい表現を使う。
  - 意味と挙動を正しく理解できることを優先する。
  - 全言語で表現を変えるべき場合は、ALCOMD3 issue または pull request で提案する。
