# Repository List file format

言語: [English](../repository-list-file-format.md) | 日本語 | [简体中文](repository-list-file-format.zh-CN.md)

このドキュメントは、ALCOMD3 が使用する repository list file format を説明します。

## File format

1. ファイルは UTF-8 encoded text file です。
2. 各行では、`#` 以降の text は comment として無視されます。
3. 各行には repository URL、または空行を記述します。
4. 各行は処理前に trim されます。つまり spaces だけの行は無視されます。
5. Repository 行には valid URL だけを記述します。
6. URL scheme は `http`、`https`、または `vcc` である必要があります。\
   その他の scheme は無視され、将来別の用途として認識される可能性があります。
7. URL が `http` または `https` URL の場合、その URL は headers なしの VPM repository を表します。
8. URL が `vcc` URL の場合、URL は下記で説明する VPM repository 追加用の VCC URL である必要があります。\
   この notation は headers 付き repository を表すために使用します。

## VCC URL format

VPM Repository 追加用の VCC URL は、次の format の valid URL です。

- scheme は `vcc`
- host part は `vpm`
- path part は `/addRepo`
- query part には、追加する repository URL を表す single `url` parameter が必要
- query part には、repository の HTTP headers を表す `headers[]` parameter を含めることが可能
  - query value は `:` で split され、前半が header name、残りが header value になります。

## Examples

```text
# This is a comment
http://example.com/repo
https://example.com/repo

vcc://vpm/addRepo?url=http://example.com/repo&headers[]=header-name:header-value
```

この file は次の repositories を持つ repository list を表します。

- `http://example.com/repo`
- `https://example.com/repo`
- custom header `header-name:header-value` 付きの `http://example.com/repo`

別の example は、この repository root の `repositories.txt` にあります。
