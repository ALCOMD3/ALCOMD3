# ALCOMD3 updater signing

语言: [English](../ALCOMD3_UPDATER.md) | [日本語](ALCOMD3_UPDATER.ja.md) | 简体中文

本文档只覆盖 updater key material 和签名验证。完整发布流程使用
[RELEASE.zh-CN.md](../RELEASE/RELEASE.zh-CN.md)。

ALCOMD3 使用自己的 updater key pair 和 `xtask` 签名命令。ALCOMD3 发布使用 ALCOMD3 updater key 和 `ALCOMD3_UPDATER_*` 环境变量名。

### Key model

- updater public key 存放在 `vrc-get-gui/src/updater-public-key.txt`，并由 GUI updater include。
- private key 由 `xtask sign-alcom-updater` 从以下变量读取：
  - `ALCOMD3_UPDATER_PRIVATE_KEY`
  - `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`
- 单个 metadata 文件包含三个 platform entry，并验证以下 updater payload：
  - `windows-x86_64`：`ALCOMD3_{version}_windows_x86_64_setup.exe`；
  - `darwin-aarch64`：`ALCOMD3_{version}_macos_aarch64.app.tar.gz`；
  - `linux-x86_64`：`ALCOMD3_{version}_linux_x86_64.AppImage.tar.gz`。
- 浏览器下载是独立资产：Windows setup ZIP、macOS DMG、Linux AppImage 和 Linux DEB
  不能替代各自配置的 updater payload。
- AppImage build 启用应用内 self-update。DEB 使用 `--no-self-updater` 单独构建，使包管理器
  安装始终由包管理器管理。`releasePlatforms.*.updater.updateMode` 与
  `releasePlatforms.*.downloads[].updateMode` 驱动构建模式，并写入 release shard 与统一
  manifest。

Updater JSON 必须包含 artifact URL 和签名文件的实际内容，而不是签名文件 URL。

### 生成 key pair

仅在初始化或有意轮换 updater key 时生成 key pair：

```powershell
$bytes = [byte[]]::new(32)
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($bytes)
$rng.Dispose()
$env:ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD = [Convert]::ToBase64String($bytes)
cargo xtask generate-alcom-updater-key
```

生成文件位于 `artifacts/alcomd3-updater-key/`：

- `public-key-base64.txt`：复制到 `vrc-get-gui/src/updater-public-key.txt`。
- `private-key.env`：生成的可移植签名值；把其中的值复制到仓库根目录 `.env`，但不要
  提交该文件。
- `private-key.ps1`：生成的 PowerShell 签名值，仅用于安全备份或手动加载。

Private key 文件已 ignore，不能提交。请安全备份。如果 private key 丢失，使用已嵌入 public key 的 build 将无法自动更新到未来版本。

### 加载签名变量

把 `.env.example` 复制为仓库根目录 `.env` 并填写签名值。普通
`cargo xtask release-build --platform ...` 只生成未签名平台 shard，不读取 updater 私钥。
`release-assemble` 会在 signing variable 未设置时加载 `.env`，统一签名三个 updater
payload，并选择认证签名用途。GitHub Actions 使用同名 Secrets。

### GitHub Actions 发布方式

GitHub Actions 是默认发布路径。以下两个键必须配置为 repository Actions Secrets，并与
根目录 `.env` 保持同名：

- `ALCOMD3_UPDATER_PRIVATE_KEY`
- `ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD`

私钥和密码加载器会兼容复制 Secret 值时带入的开头 UTF-8 BOM 与末尾 CR/LF 换行，但会原样
保留其他内容。

`release-draft.yml` 构建三个 source-bound platform shards。只有可信组装 job 接收 updater
密钥：先确认私钥能解密且与 GUI 内嵌公钥匹配，再由 `release-assemble` 签名并验证 Windows
setup、macOS app updater archive 和 Linux AppImage updater archive，原子写入统一 release
manifest。Workflow 不创建或上传 `.env`。

人工发布 Draft 后，`release-updater.yml` 会下载并核对全部 10 项公开资产；随后验证三个
updater payload/signature pair，从 Release tag 读取
updater notes，并在全新 runner 上以 Release `publishedAt` 作为固定 `pub_date`，原子重建所选
channel 的三平台 updater JSON。它还会验证精确 tag/source SHA 与 source 版本、拒绝版本回退，
并要求认证的 `release` 签名用途，因此重跑结果稳定。Updater workflow 无权读取 private
signing key；checkout 不持久化凭据，GitHub token 也会从非 GitHub 子进程移除。

受保护的 `release-signing` Environment 可在以后作为独立审批边界加入，但当前 workflow
不以它为前置条件。

在 `release-build` 之外直接运行签名命令时，可将根目录 `.env` 加载到当前
PowerShell 进程：

```powershell
Get-Content .\.env | Where-Object {
    $_.Trim() -and -not $_.Trim().StartsWith('#')
} | ForEach-Object {
    $name, $value = $_ -split '=', 2
    [Environment]::SetEnvironmentVariable($name, $value, 'Process')
}
```

可以用以下命令验证已加载的私钥、密码和内嵌公钥，不写入签名或产物：

```powershell
cargo xtask verify-alcom-updater-key
```

该命令只在内存中签名固定挑战内容并立即验签，不会输出密钥或生成的签名。

### 签名 updater payload

```powershell
$Version = "2.1.2-beta.1"
$SetupDir = "target\x86_64-pc-windows-msvc\release\bundle\setup"
$Installer = "$SetupDir\ALCOMD3_${Version}_windows_x86_64_setup.exe"
cargo xtask sign-alcom-updater $Installer
```

直接签名默认写入认证的 `local-test` 用途，可用于诊断任一配置的 updater payload。需要
手动发布的资产时，必须构建三个 `release-build --release-artifacts` shard，再运行
`release-assemble`；不要临时直接指定 `--purpose release`。

输出：

```text
target/x86_64-pc-windows-msvc/release/bundle/setup/ALCOMD3_{version}_windows_x86_64_setup.exe.sig
```

Updater JSON generator 会读取 `.sig` 文件，并将内容写入 `signature` field。

### 验证 updater JSON

发布公开 metadata 前必须验证最终 updater JSON。默认的 published updater workflow 会使用下载的
公开 installer 自动完成；以下命令用于本地产物验证：

```powershell
$Assets = "artifacts\release-check\v2.1.2-beta.1"
cargo xtask verify-alcom-updater-json `
    --assets $Assets `
    --json "artifacts\release-updater\tauri-updater-beta.json" `
    --expected-signature-purpose release
```

本地诊断签名应使用匹配的资产目录和 `--expected-signature-purpose local-test`。

验证会检查 JSON 可解析，并同时包含 `windows-x86_64`、`darwin-aarch64` 和
`linux-x86_64`；每个配置 URL 与 updater 文件名精确匹配；每份签名存在，其认证 trusted
comment 绑定精确文件名和指定用途；三个 payload 都能使用
`vrc-get-gui/src/updater-public-key.txt` 中的 public key 验证。

### macOS 两层签名

macOS 发布路径只支持 ad-hoc 签名，使用身份 `-` 签署 `.app`、嵌套程序和 DMG，但不提供 Apple
平台信任或公证，因此 Gatekeeper 可能要求用户在首次启动时人工确认。ALCOMD3 updater
Minisign 使用 `ALCOMD3_UPDATER_*`，单独验证
`.app.tar.gz` updater payload；ad-hoc 代码签名不会削弱或取代 updater 签名验证。

### Key rotation rule

不要在普通发布中轮换 updater key。若无法避免，必须先发布一个由旧 key 签名且包含新 embedded public key 的 bridge release，之后的 installer 才能用新 private key 签名。
