<p align="center">
    <img src="../website/public/assets/logo.png" alt="ALCOMD3 标志" width="160">
</p>

<h1 align="center">ALCOMD3</h1>

<p align="center">
    面向 VRChat Unity 项目与 VPM 软件包管理的开源 VRChat Creator Companion 替代品。
</p>

<p align="center">
    <a href="https://alcomd3.cqmhv.com/">下载</a> ·
    <a href="https://github.com/ALCOMD3/ALCOMD3/releases">发布版本</a> ·
    <a href="../docs/README/README.zh-CN.md">文档</a>
</p>

<p align="center">
    <a href="../README.md">English</a> ·
    <a href="./README.ja.md">日本語</a> ·
    <a href="./README.zh-TW.md">繁體中文</a> · 简体中文
</p>

## 在一个地方管理你的 VRChat 项目

ALCOMD3 帮助创作者集中处理 VRChat Unity 项目的日常工作，无需再手动整理项目文件夹和软件包文件。

- **管理项目：**在一个桌面应用中创建、登记、复制、备份和恢复项目。
- **控制 VPM 软件包：**浏览软件仓库，为每个项目安装、移除或更新软件包。
- **沿用熟悉的链接：**打开与 VCC 兼容的 `vcc://` 链接，并在同一套流程中管理软件仓库。
- **打造自己的界面：**使用 Material Design 3 风格界面，并可选择浅色、深色和自定义主题。
- **接入自己的 AI 工具：**按需允许支持 MCP 的客户端在受限范围内访问 ALCOMD3 的项目、
  软件包、仓库和日志。

## 开始使用

1. 前往 [ALCOMD3 官网](https://alcomd3.cqmhv.com/)，选择 stable 或 beta 通道。
2. 下载适合当前操作系统的软件包，然后安装或启动应用。
3. 添加已有 VRChat Unity 项目或创建新项目，随后即可在 ALCOMD3 中管理软件包、
   仓库和备份。

官方版本支持 Windows x64、搭载 Apple 芯片的 macOS，以及 Linux x64。你也可以在
[GitHub Releases](https://github.com/ALCOMD3/ALCOMD3/releases) 中查看所有已发布版本。

支持应用内更新时，ALCOMD3 默认在启动时检查已签名更新，发现更新后下载，
并在下次启动前安装。可在“设置”中关闭此行为。

## 可选的本地 MCP 集成

ALCOMD3 可将支持 MCP 的 AI 客户端连接到经过限定的项目、仓库、软件包、环境设置、
活动记录和技术日志数据，并提供一组有限的项目与软件包操作。

MCP 默认关闭。启用后，它仅使用本地 stdio bridge 和私有的本机回环 IPC。
GUI 内部 IPC 只监听 `127.0.0.1`，不监听公网地址。配置方式、可用工具和权限边界请参阅
[MCP 指南](../docs/mcp/mcp.zh-CN.md)。

## 项目与社区

ALCOMD3 起源于 ALCOM/vrc-get，目前作为独立的开源项目维护。
它不是 VRChat 或 VCC 的官方产品。

- 遇到问题或有新想法？[提交 Issue](https://github.com/ALCOMD3/ALCOMD3/issues)。
- 想参与贡献？请阅读[贡献指南](../CONTRIBUTING/CONTRIBUTING.zh-CN.md)。
- 需要技术或维护资料？从[文档索引](../docs/README/README.zh-CN.md)开始。
- 想了解版本变化？查看 [GitHub 发布说明](https://github.com/ALCOMD3/ALCOMD3/releases)。

## 参与开发

开发环境需要 Rust stable toolchain、Node.js、npm，以及目标平台所需的构建工具。

以开发模式运行桌面应用：

```powershell
cd vrc-get-gui
npm run tauri dev
```

仅开发前端：

```powershell
cd vrc-get-gui
npm run dev
```

构建、测试、维护和发布资料统一收录在[文档索引](../docs/README/README.zh-CN.md)中。

## 许可证

主要项目代码使用 GNU Affero General Public License v3.0 or later
（`AGPL-3.0-or-later`）。请参阅 [LICENSE](../LICENSE) 和
[LICENSE-NOTES.md](../LICENSE-NOTES.md)。

依赖项和第三方资源声明可在应用内的 Licenses 页面和
[vrc-get-gui/THIRD-PARTY.md](../vrc-get-gui/THIRD-PARTY.md) 中查看。
