# Contributing

Languages: English | [日本語](CONTRIBUTING/CONTRIBUTING.ja.md) |
[简体中文](CONTRIBUTING/CONTRIBUTING.zh-CN.md)

Read [../CONTRIBUTING.md](../CONTRIBUTING.md) first.

### Contribution license

Unless explicitly marked otherwise and accepted by the maintainers, GUI
contributions are licensed under ALCOMD3's current main project license,
`AGPL-3.0-or-later`.

### Localizing

ALCOMD3 is internationalized. When adding user-facing text, add or reuse an
existing localization key and use i18n instead of hardcoding text.

When adding a new localization key, add an English value in `locales/en.json5`.
If you understand other languages, you may add values for them. If you do not,
leave them for the relevant language maintainers.

### Adding languages

New languages are welcome. To add one:

1. Fork the repository and create a branch for the new language.
2. Create a new JSON5 file in `locales/` using the language code.
   - For example, Japanese uses `ja.json5`.
3. Import the new JSON5 file in `lib/i18n.ts` and add it to the `languageResources` object.
4. Create a draft pull request.
5. Update release notes or the relevant documentation when the language addition needs user-facing release coverage.
6. Mark the pull request as ready for review.
7. A maintainer will ask whether you can maintain the language.
   If you do not want to maintain it, the language will not be merged until another maintainer volunteers.
8. If localization tracking discussions or automation are maintained for the task, a maintainer will update them.
9. Allow edits from maintainers for the pull request.
10. A maintainer will merge the pull request after review.

### Localization guidelines

- ALCOMD3 is cross-platform, so prefer OS-independent wording.
  - For example, prefer "Directory" over "Folder" when the distinction matters.
- Technical terms do not always need to be translated.
  - English terms or common transliterations are acceptable when they are clearer.
- Do not translate word by word when another expression is easier for users to understand.
  - Prioritize correct user understanding of meaning and behavior.
  - If an expression should change across all languages, propose it in an ALCOMD3 issue or pull request.
