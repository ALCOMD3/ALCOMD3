# Repository List 文件格式

语言: [English](../repository-list-file-format.md) | [日本語](repository-list-file-format.ja.md) | 简体中文

本文档说明 ALCOMD3 使用的 repository list 文件格式。

## 文件格式

1. 文件是 UTF-8 编码的文本文件。
2. 每一行中，`#` 之后的文本会作为注释忽略。
3. 每一行包含一个 repository URL，或为空。
4. 每一行都会在处理前 trim。也就是说，只包含空格的行会被忽略。
5. Repository 行只能包含一个有效 URL。
6. URL scheme 必须是 `http`、`https` 或 `vcc`。\
   其他 scheme 会被忽略，并且将来可能被识别为其他用途。
7. 如果 URL 是 `http` 或 `https` URL，该 URL 表示一个不带 headers 的 VPM repository。
8. 如果 URL 是 `vcc` URL，该 URL 应为下文说明的 VPM repository 添加用 VCC URL。\
   这种表示法用于表达带 headers 的 repository。

## VCC URL 格式

用于添加 VPM Repository 的 VCC URL 是符合以下格式的有效 URL：

- scheme 必须是 `vcc`
- host 部分必须是 `vpm`
- path 部分必须是 `/addRepo`
- query 部分必须包含一个 `url` 参数，表示要添加的 repository URL
- query 部分可以包含 `headers[]` 参数，表示该 repository 的 HTTP headers
  - query value 会按 `:` 分割，前半部分是 header name，其余部分是 header value。

## 示例

```text
# This is a comment
http://example.com/repo
https://example.com/repo

vcc://vpm/addRepo?url=http://example.com/repo&headers[]=header-name:header-value
```

该文件表示一个包含以下 repositories 的 repository list：

- `http://example.com/repo`
- `https://example.com/repo`
- 带有自定义 header `header-name:header-value` 的 `http://example.com/repo`

另一个示例位于本仓库根目录的 `repositories.txt`。
