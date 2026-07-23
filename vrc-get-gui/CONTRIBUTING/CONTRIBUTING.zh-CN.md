# Contributing

语言: [English](../CONTRIBUTING.md) | [日本語](CONTRIBUTING.ja.md) | 简体中文

先阅读[项目级贡献指南](../../CONTRIBUTING/CONTRIBUTING.zh-CN.md)。

### 贡献授权

除非贡献者明确标注其他许可证且维护者接受，GUI 贡献默认按 ALCOMD3 当前主项目许可证
`AGPL-3.0-or-later` 授权。

### 本地化

ALCOMD3 支持国际化。新增用户可见文本时，添加或复用已有 localization key，并使用 i18n，不要硬编码文本。

新增 localization key 时，必须在 `locales/en.json5` 添加英文值。如果你懂其他语言，可以同时添加对应值；如果不懂，留给对应语言维护者处理。

### 新增语言

欢迎新增语言。步骤：

1. Fork 仓库并为新语言创建分支。
2. 在 `locales/` 中用语言代码创建新的 JSON5 文件。
   - 例如日语使用 `ja.json5`。
3. 在 `lib/i18n.ts` 导入新的 JSON5 文件，并加入 `languageResources` object。
4. 创建 draft pull request。
5. 如果新增语言需要用户可见的发布说明覆盖，更新 release notes 或相关文档。
6. 将 pull request 标记为 ready for review。
7. 维护者会询问你是否能维护该语言。
   如果你不想维护，在其他维护者出现前，该语言不会合并。
8. 如果本任务维护 localization tracking discussion 或自动化，维护者会更新它们。
9. 为 pull request 开启 Allow edits from maintainers。
10. Review 后由维护者合并 pull request。

### 本地化指南

- ALCOMD3 是跨平台应用，优先使用不依赖具体 OS 的表述。
  - 需要区分时，优先使用 "Directory" 而不是 "Folder"。
- 技术词不一定必须翻译。
  - 更清楚时可以保留英文词或使用常见音译。
- 不必逐字翻译，应使用用户更容易理解的表达。
  - 优先确保用户正确理解含义和行为。
  - 如果某个表达应在所有语言中一起调整，请在 ALCOMD3 issue 或 pull request 中提出。
