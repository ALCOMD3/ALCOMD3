import { supportedRouteLocales, type RouteLocale, type UiLocale } from "@/data/i18n";
import { siteConfig as sharedSiteConfig } from "@/data/site.config.mjs";

export const languageKeys = supportedRouteLocales;

export type LanguageKey = RouteLocale;

export const languageNames: Record<LanguageKey, string> = {
    "zh-cn": "简体中文",
    "zh-tw": "繁體中文",
    "ja-jp": "日本語",
    "en-us": "English",
};

export const siteConfig = sharedSiteConfig;

type Advantage = {
    title: string;
    body: string;
};

export type DownloadPlatformKey = "windows-x86_64" | "darwin-aarch64" | "linux-x86_64";
export type DownloadChannel = "stable" | "beta";

type DownloadPlatformContent = {
    name: string;
    description: string;
};

type DownloadChannelContent = {
    name: string;
    description: string;
};

type DownloadContent = {
    sectionTitle: string;
    sectionDescription: string;
    channelVersion: string;
    unavailable: string;
    stableReleaseOnly: string;
    betaCanonicalUnavailable: string;
    releasePage: string;
    downloadAction: string;
    channels: Record<DownloadChannel, DownloadChannelContent>;
    platforms: Record<DownloadPlatformKey, DownloadPlatformContent>;
    formats: Record<string, string>;
};

export type LocaleContent = {
    lang: LanguageKey;
    uiLocale: UiLocale;
    htmlLang: string;
    ogLocale: string;
    languageLabel: string;
    githubLabel: string;
    contributorsTitle: string;
    mcpDocsLabel: string;
    mcpDocsPageTitle: string;
    mcpDocsPageDescription: string;
    mcpSectionTitle: string;
    mcpSectionDescription: string;
    appAdvantagesTitle: string;
    appearanceLabel: string;
    appearanceSystem: string;
    appearanceLight: string;
    appearanceDark: string;
    title: string;
    description: string;
    twitterDescription: string;
    structuredDescription: string;
    seoKeywords: string[];
    ogImageAlt: string;
    softwareFeatures: string[];
    productSubtitle: string;
    tagline: string;
    positioningTitle: string;
    positioningBody: string;
    positioningChips: string[];
    downloads: DownloadContent;
    booth: string;
    gumroad: string;
    advantages: Advantage[];
    footerDescription: string;
    footerCopyright: string;
    footerRelated: string;
    vrcalLink: string;
};

export const locales: Record<LanguageKey, LocaleContent> = {
    "zh-cn": {
        lang: "zh-cn",
        uiLocale: "zh-CN",
        htmlLang: "zh-CN",
        ogLocale: "zh_CN",
        languageLabel: "语言",
        githubLabel: "访问 GitHub",
        contributorsTitle: "贡献者",
        mcpDocsLabel: "MCP 文档",
        mcpDocsPageTitle: "ALCOMD3 MCP 说明",
        mcpDocsPageDescription: "阅读 ALCOMD3 本地 MCP bridge 的启用方式、客户端配置、可用工具、生命周期行为、权限边界和排障说明。",
        mcpSectionTitle: "可选本地 MCP 集成",
        mcpSectionDescription: "MCP 文档已经开放；本节概括 stdio bridge、本机 IPC、默认关闭的数据访问、日志查询，以及项目和仓库相关的受限写工具。",
        appAdvantagesTitle: "桌面端体验与核心优势",
        appearanceLabel: "外观",
        appearanceSystem: "跟随系统",
        appearanceLight: "浅色",
        appearanceDark: "深色",
        title: "ALCOMD3 - 开源 VCC 替代品与 VRChat Unity 项目管理工具",
        description: "ALCOMD3 是起源于 ALCOM/vrc-get 的开源 VCC 替代品，用于管理 VRChat Unity 项目、VPM 软件包、仓库、备份、更新与可选本地 MCP 集成。",
        twitterDescription: "开源 VRChat Creator Companion（VCC）替代品，面向 VRChat Unity 项目与 VPM 软件包管理，提供仓库、备份、更新和可选本地 MCP bridge。",
        structuredDescription: "ALCOMD3 是起源于 ALCOM/vrc-get 的独立开源桌面工具，可作为 VRChat Creator Companion（VCC）替代品管理 VRChat Unity 项目、VPM 仓库和软件包，提供备份、更新、vcc:// 关联以及可选本地 MCP bridge。MCP 通过 stdio 与本机 IPC 提供项目、仓库、软件包、环境设置、活动记录和技术日志可见性，并支持项目创建、已有项目登记、VPM 仓库登记、备份、复制、恢复和单包安装、卸载、重装等受限操作。",
        seoKeywords: [
            "ALCOMD3",
            "VCC替代品",
            "VRChat Creator Companion 替代品",
            "VRChat Unity 项目管理",
            "ALCOM",
            "vrc-get",
            "VPM 软件包管理",
            "VRChat 软件包管理",
        ],
        ogImageAlt: "ALCOMD3 开源 VCC 替代品与 VRChat Unity 项目管理工具",
        softwareFeatures: [
            "VRChat Unity 项目管理",
            "VPM 软件包管理",
            "VPM 仓库管理",
            "项目备份与恢复",
            "ALCOM/vrc-get 兼容起源",
            "可选本地 MCP bridge",
        ],
        productSubtitle: "VRChat 项目与 VPM 软件包管理",
        tagline: "管理项目、软件包、备份、更新与可选本地 MCP 接入",
        positioningTitle: "开源 VCC 替代品，面向 VRChat Unity 项目",
        positioningBody: "ALCOMD3 起源于 ALCOM/vrc-get，是用于管理 VRChat Unity 项目和 VPM 软件包的独立桌面工具，可作为 VRChat Creator Companion（VCC）的开源替代品，覆盖项目创建、仓库、软件包、备份、更新和可选本地 MCP 接入。",
        positioningChips: [
            "VCC替代品",
            "VRChat Unity",
            "ALCOM/vrc-get 起源",
            "VPM 软件包管理",
            "开源桌面工具",
        ],
        downloads: {
            sectionTitle: "下载 ALCOMD3",
            sectionDescription: "可手动选择稳定版或测试版，再下载与操作系统和处理器架构匹配的安装包。这里只提供实际发布的链接。",
            channelVersion: "{channel}：{version}",
            unavailable: "此通道暂未提供",
            stableReleaseOnly: "稳定版暂不接入新的多平台直链；可前往 GitHub Release 查看当前稳定版本。",
            betaCanonicalUnavailable: "采用新平台命名标准的测试版尚未发布；旧测试版资产不会作为新的多平台下载入口。",
            releasePage: "在 GitHub 查看 {channel} {version}",
            downloadAction: "下载 {channel} {version} · {platform}（{format}）",
            channels: {
                stable: {
                    name: "稳定版",
                    description: "适合日常使用，并经过稳定发布流程验证。",
                },
                beta: {
                    name: "测试版（Beta）",
                    description: "用于提前体验新功能，稳定性可能低于稳定版。",
                },
            },
            platforms: {
                "windows-x86_64": {
                    name: "Windows x64",
                    description: "Windows 10/11 · x64",
                },
                "darwin-aarch64": {
                    name: "macOS Apple 芯片",
                    description: "M1 或更新机型 · 暂不支持 Intel Mac",
                },
                "linux-x86_64": {
                    name: "Linux x86_64",
                    description: "通用 AppImage，另提供 Debian / Ubuntu DEB 软件包",
                },
            },
            formats: {
                "windows-installer": "ZIP 安装包",
                "macos-apple-silicon": "DMG",
                "linux-appimage": "AppImage",
                "linux-deb": "DEB 软件包",
            },
        },
        booth: "BOOTH",
        gumroad: "Gumroad",
        advantages: [
            {
                title: "可选本地 MCP bridge",
                body: "通过 stdio MCP server 接入支持 MCP 的 AI 客户端，再由本机 IPC 读取 ALCOMD3 数据；MCP 默认停用，需在 GUI 中启用后新的工具调用才会返回数据。",
            },
            {
                title: "可见性与受限写工具",
                body: "MCP 提供项目、仓库、软件包、环境设置、活动记录和技术日志查询，并开放新建项目、添加已有项目文件夹、添加 VPM 仓库、备份、复制、从 zip 备份恢复，以及对已登记项目安装、卸载、重装单个 GUI 可见软件包等受限写工具。",
            },
            {
                title: "项目、软件包与日志可见性",
                body: "AI Agent 可以列出已登记项目、查看项目包摘要、读取仓库和 GUI 可见软件包、获取 Unity/默认路径/备份路径摘要，并按条件搜索已脱敏活动记录和技术日志。",
            },
            {
                title: "Material Design 3 风格界面",
                body: "更现代的视觉层级、状态反馈、主题色自定义和首次设置入口，让日常管理 VRChat 项目时更清晰、舒适。",
            },
            {
                title: "更透明的操作进度",
                body: "包操作、项目复制和备份流程提供更细的进度显示，包含逐包状态、重试、取消和终止等实用控制。",
            },
            {
                title: "ALCOMD3 发布与更新",
                body: "使用 ALCOMD3 自有更新源、明确的平台可用状态和带版本号的安装包命名，让 Windows、macOS 与 Linux 的发布体验保持清晰一致。",
            },
            {
                title: "更好的桌面端集成",
                body: "ALCOMD3 作为一个独立的桌面应用程序，可以更好地与操作系统集成，提供更流畅的用户体验。",
            },
        ],
        footerDescription: "ALCOMD3 是起源于 vrc-get/ALCOM 的独立开源桌面工具。",
        footerCopyright: "© CQMHV. ALCOMD3 为开源软件项目，保留原项目及其贡献者的相关权利。",
        footerRelated: "相关项目",
        vrcalLink: "VRChatAvatarLearn",
    },
    "zh-tw": {
        lang: "zh-tw",
        uiLocale: "zh-TW",
        htmlLang: "zh-TW",
        ogLocale: "zh_TW",
        languageLabel: "語言",
        githubLabel: "前往 GitHub",
        contributorsTitle: "貢獻者",
        mcpDocsLabel: "MCP 文件",
        mcpDocsPageTitle: "ALCOMD3 MCP 說明",
        mcpDocsPageDescription: "閱讀 ALCOMD3 本機 MCP bridge 的啟用方式、客戶端設定、可用工具、生命週期行為、權限邊界和疑難排解說明。",
        mcpSectionTitle: "可選本機 MCP 整合",
        mcpSectionDescription: "MCP 文件已開放；本節概括 stdio bridge、本機 IPC、預設停用的資料存取、日誌查詢，以及專案和倉庫相關的受限寫入工具。",
        appAdvantagesTitle: "桌面端體驗與核心優勢",
        appearanceLabel: "外觀",
        appearanceSystem: "跟隨系統",
        appearanceLight: "淺色",
        appearanceDark: "深色",
        title: "ALCOMD3 - 開源 VCC 替代品與 VRChat Unity 專案管理工具",
        description: "ALCOMD3 是起源於 ALCOM/vrc-get 的開源 VCC 替代品，用於管理 VRChat Unity 專案、VPM 套件、儲存庫、備份、更新與可選本機 MCP 整合。",
        twitterDescription: "開源 VRChat Creator Companion（VCC）替代品，面向 VRChat Unity 專案與 VPM 套件管理，提供儲存庫、備份、更新和可選本機 MCP bridge。",
        structuredDescription: "ALCOMD3 是起源於 ALCOM/vrc-get 的獨立開源桌面工具，可作為 VRChat Creator Companion（VCC）替代品管理 VRChat Unity 專案、VPM 儲存庫和套件，提供備份、更新、vcc:// 關聯以及可選本機 MCP bridge。MCP 透過 stdio 與本機 IPC 提供專案、儲存庫、套件、環境設定、活動記錄和技術日誌可見性，並支援專案建立、既有專案登記、VPM 倉庫登記、備份、複製、還原和單套件安裝、解除安裝、重裝等受限操作。",
        seoKeywords: [
            "ALCOMD3",
            "VCC替代品",
            "VRChat Creator Companion 替代品",
            "VRChat Unity 專案管理",
            "ALCOM",
            "vrc-get",
            "VPM 套件管理",
            "VRChat 套件管理",
        ],
        ogImageAlt: "ALCOMD3 開源 VCC 替代品與 VRChat Unity 專案管理工具",
        softwareFeatures: [
            "VRChat Unity 專案管理",
            "VPM 套件管理",
            "VPM 儲存庫管理",
            "專案備份與還原",
            "ALCOM/vrc-get 相容起源",
            "可選本機 MCP bridge",
        ],
        productSubtitle: "VRChat 專案與 VPM 套件管理",
        tagline: "管理專案、套件、備份、更新與可選本機 MCP 接入",
        positioningTitle: "開源 VCC 替代品，面向 VRChat Unity 專案",
        positioningBody: "ALCOMD3 起源於 ALCOM/vrc-get，是用於管理 VRChat Unity 專案和 VPM 套件的獨立桌面工具，可作為 VRChat Creator Companion（VCC）的開源替代品，覆蓋專案建立、儲存庫、套件、備份、更新和可選本機 MCP 接入。",
        positioningChips: [
            "VCC替代品",
            "VRChat Unity",
            "ALCOM/vrc-get 起源",
            "VPM 套件管理",
            "開源桌面工具",
        ],
        downloads: {
            sectionTitle: "下載 ALCOMD3",
            sectionDescription: "可手動選擇穩定版或測試版，再下載符合作業系統與處理器架構的安裝包。這裡只提供實際發佈的連結。",
            channelVersion: "{channel}：{version}",
            unavailable: "此通道暫未提供",
            stableReleaseOnly: "穩定版暫不接入新的多平台直連；可前往 GitHub Release 查看目前穩定版本。",
            betaCanonicalUnavailable: "採用新平台命名標準的測試版尚未發佈；舊測試版資產不會作為新的多平台下載入口。",
            releasePage: "在 GitHub 查看 {channel} {version}",
            downloadAction: "下載 {channel} {version} · {platform}（{format}）",
            channels: {
                stable: {
                    name: "穩定版",
                    description: "適合日常使用，並經過穩定發佈流程驗證。",
                },
                beta: {
                    name: "測試版（Beta）",
                    description: "用於提前體驗新功能，穩定性可能低於穩定版。",
                },
            },
            platforms: {
                "windows-x86_64": {
                    name: "Windows x64",
                    description: "Windows 10/11 · x64",
                },
                "darwin-aarch64": {
                    name: "macOS Apple 晶片",
                    description: "M1 或更新機型 · 暫不支援 Intel Mac",
                },
                "linux-x86_64": {
                    name: "Linux x86_64",
                    description: "通用 AppImage，另提供 Debian / Ubuntu DEB 軟體包",
                },
            },
            formats: {
                "windows-installer": "ZIP 安裝包",
                "macos-apple-silicon": "DMG",
                "linux-appimage": "AppImage",
                "linux-deb": "DEB 軟體包",
            },
        },
        booth: "BOOTH",
        gumroad: "Gumroad",
        advantages: [
            {
                title: "可選本機 MCP bridge",
                body: "透過 stdio MCP server 接入支援 MCP 的 AI 客戶端，再由本機 IPC 讀取 ALCOMD3 資料；MCP 預設停用，需在 GUI 中啟用後新的工具呼叫才會傳回資料。",
            },
            {
                title: "可見性與受限寫入工具",
                body: "MCP 提供專案、儲存庫、套件、環境設定、活動記錄和技術日誌查詢，並開放新建專案、新增既有專案資料夾、新增 VPM 倉庫、備份、複製、從 zip 備份還原，以及對已登記專案安裝、解除安裝、重裝單一 GUI 可見套件等受限寫入工具。",
            },
            {
                title: "專案、套件與日誌可見性",
                body: "AI agent 可以列出已登記專案、查看專案套件摘要、讀取儲存庫和 GUI 可見套件、取得 Unity/預設路徑/備份路徑摘要，並按條件搜尋已脫敏活動記錄和技術日誌。",
            },
            {
                title: "Material Design 3 風格介面",
                body: "更現代的視覺層級、狀態回饋、主題色自訂與初次設定入口，讓日常管理 VRChat 專案時更清晰、舒適。",
            },
            {
                title: "更透明的操作進度",
                body: "套件操作、專案複製與備份流程提供更細的進度顯示，包含逐套件狀態、重試、取消與終止等實用控制。",
            },
            {
                title: "ALCOMD3 發行與更新",
                body: "使用 ALCOMD3 自有更新來源、明確的平台可用狀態和帶版本號的安裝包命名，讓 Windows、macOS 與 Linux 的發行體驗保持清晰一致。",
            },
            {
                title: "更好的桌面端整合",
                body: "ALCOMD3 作為一個獨立的桌面應用程式，可以更好地與作業系統整合，提供更流暢的使用者體驗。",
            },
        ],
        footerDescription: "ALCOMD3 是起源於 vrc-get/ALCOM 的獨立開源桌面工具。",
        footerCopyright: "© CQMHV. ALCOMD3 為開源軟體專案，保留原專案及其貢獻者的相關權利。",
        footerRelated: "相關專案",
        vrcalLink: "VRChatAvatarLearn",
    },
    "ja-jp": {
        lang: "ja-jp",
        uiLocale: "ja-JP",
        htmlLang: "ja-JP",
        ogLocale: "ja_JP",
        languageLabel: "言語",
        githubLabel: "GitHub を開く",
        contributorsTitle: "コントリビューター",
        mcpDocsLabel: "MCP ドキュメント",
        mcpDocsPageTitle: "ALCOMD3 MCP ガイド",
        mcpDocsPageDescription: "ALCOMD3 のローカル MCP bridge の有効化、クライアント設定、利用可能な tools、ライフサイクル挙動、権限境界、トラブルシューティングを確認できます。",
        mcpSectionTitle: "任意のローカル MCP 連携",
        mcpSectionDescription: "MCP ドキュメントは公開済みです。このセクションでは stdio bridge、ローカル IPC、既定で無効なデータアクセス、ログ検索、プロジェクトとリポジトリ向けの限定的な書き込み tools を要約します。",
        appAdvantagesTitle: "デスクトップ体験と主な強み",
        appearanceLabel: "外観",
        appearanceSystem: "システム設定に従う",
        appearanceLight: "ライト",
        appearanceDark: "ダーク",
        title: "ALCOMD3 - VRChat Unity プロジェクト向けのオープンソース VCC 代替",
        description: "ALCOMD3 は ALCOM/vrc-get を起源とするオープンソースの VCC 代替ツールで、VRChat Unity プロジェクト、VPM パッケージ、リポジトリ、バックアップ、更新、任意のローカル MCP 連携を管理します。",
        twitterDescription: "VRChat Creator Companion（VCC）のオープンソース代替。VRChat Unity プロジェクトと VPM パッケージ管理、リポジトリ、バックアップ、更新、任意のローカル MCP bridge に対応します。",
        structuredDescription: "ALCOMD3 は ALCOM/vrc-get を起源とする独立したオープンソースデスクトップツールで、VRChat Creator Companion（VCC）の代替として VRChat Unity プロジェクト、VPM リポジトリ、パッケージを管理し、バックアップ、更新、vcc:// 関連付け、任意のローカル MCP bridge を提供します。MCP は stdio とローカル IPC により、プロジェクト、リポジトリ、パッケージ、環境設定、アクティビティ記録、技術ログの visibility と、プロジェクト作成、既存プロジェクト登録、VPM リポジトリ登録、バックアップ、コピー、復元、単一パッケージのインストール、アンインストール、再インストールなど限定的な操作を提供します。",
        seoKeywords: [
            "ALCOMD3",
            "VCC 代替",
            "VRChat Creator Companion 代替",
            "VRChat Unity プロジェクト管理",
            "ALCOM",
            "vrc-get",
            "VPM パッケージ管理",
            "VRChat パッケージ管理",
        ],
        ogImageAlt: "ALCOMD3 VRChat Unity プロジェクト向けオープンソース VCC 代替",
        softwareFeatures: [
            "VRChat Unity プロジェクト管理",
            "VPM パッケージ管理",
            "VPM リポジトリ管理",
            "プロジェクトのバックアップと復元",
            "ALCOM/vrc-get 由来の互換性",
            "任意のローカル MCP bridge",
        ],
        productSubtitle: "VRChat プロジェクトと VPM パッケージ管理",
        tagline: "プロジェクト、パッケージ、バックアップ、更新、任意のローカル MCP 連携を管理",
        positioningTitle: "VRChat Unity プロジェクト向けのオープンソース VCC 代替",
        positioningBody: "ALCOMD3 は ALCOM/vrc-get を起源とする独立したデスクトップツールです。VRChat Unity プロジェクトと VPM パッケージ管理に向けた VRChat Creator Companion（VCC）のオープンソース代替として、プロジェクト作成、リポジトリ、パッケージ、バックアップ、更新、任意のローカル MCP 連携を扱えます。",
        positioningChips: [
            "VCC 代替",
            "VRChat Unity",
            "ALCOM/vrc-get 由来",
            "VPM パッケージ管理",
            "オープンソースデスクトップツール",
        ],
        downloads: {
            sectionTitle: "ALCOMD3 をダウンロード",
            sectionDescription: "安定版またはベータ版を手動で選び、OS とプロセッサのアーキテクチャに合うパッケージをダウンロードできます。実際に公開されたリンクだけを表示します。",
            channelVersion: "{channel}：{version}",
            unavailable: "このチャンネルでは未提供",
            stableReleaseOnly: "安定版は新しいマルチプラットフォーム直接リンクの対象外です。現在の安定版は GitHub Release で確認できます。",
            betaCanonicalUnavailable: "新しいプラットフォーム命名規則を使用するベータ版はまだ公開されていません。旧ベータ資産は新しいマルチプラットフォーム入口には使用しません。",
            releasePage: "{channel} {version} を GitHub で表示",
            downloadAction: "{channel} {version} · {platform} をダウンロード（{format}）",
            channels: {
                stable: {
                    name: "安定版",
                    description: "日常利用向け。安定リリース手順で検証されたバージョンです。",
                },
                beta: {
                    name: "ベータ版",
                    description: "新機能を早期に試すためのテスト版で、安定版より不安定な場合があります。",
                },
            },
            platforms: {
                "windows-x86_64": {
                    name: "Windows x64",
                    description: "Windows 10/11 · x64",
                },
                "darwin-aarch64": {
                    name: "macOS Apple シリコン",
                    description: "M1 以降 · Intel Mac は未対応",
                },
                "linux-x86_64": {
                    name: "Linux x86_64",
                    description: "汎用 AppImage と Debian / Ubuntu 向け DEB パッケージ",
                },
            },
            formats: {
                "windows-installer": "ZIP インストーラー",
                "macos-apple-silicon": "DMG",
                "linux-appimage": "AppImage",
                "linux-deb": "DEB パッケージ",
            },
        },
        booth: "BOOTH",
        gumroad: "Gumroad",
        advantages: [
            {
                title: "任意のローカル MCP bridge",
                body: "stdio MCP server で MCP 対応 AI クライアントに接続し、ローカル IPC 経由で ALCOMD3 データを読み取ります。MCP は既定で無効で、GUI で有効にした後の新しい tool call だけがデータを返します。",
            },
            {
                title: "Visibility と限定書き込み tools",
                body: "MCP はプロジェクト、リポジトリ、パッケージ、環境設定、アクティビティ記録、技術ログの query に加えて、プロジェクト作成、既存プロジェクトフォルダー追加、VPM リポジトリ追加、バックアップ、コピー、zip バックアップからの復元、登録済みプロジェクトへの GUI-visible な単一パッケージのインストール、アンインストール、再インストールを限定的に提供します。",
            },
            {
                title: "プロジェクト、パッケージ、ログ visibility",
                body: "AI agent は登録済みプロジェクト、プロジェクト内パッケージ概要、リポジトリと GUI-visible パッケージ、Unity/既定パス/バックアップパス概要を取得し、redacted 済みのアクティビティ記録と技術ログを条件検索できます。",
            },
            {
                title: "Material Design 3 スタイルの UI",
                body: "より現代的な視覚階層、状態フィードバック、テーマカラー調整、初期設定入口により、VRChat プロジェクトの日常管理が見やすく快適になります。",
            },
            {
                title: "より透明な操作進捗",
                body: "パッケージ操作、プロジェクトコピー、バックアップ処理で細かな進捗を表示し、パッケージごとの状態、再試行、キャンセル、停止などの実用的な操作を提供します。",
            },
            {
                title: "ALCOMD3 リリースと更新",
                body: "ALCOMD3 独自の更新ソース、明確なプラットフォーム提供状況、バージョン付きパッケージ名により、Windows、macOS、Linux のリリース体験を分かりやすく一貫させます。",
            },
            {
                title: "より良いデスクトップ統合",
                body: "ALCOMD3 は独立したデスクトップアプリケーションとして、オペレーティングシステムとより良く統合し、よりスムーズなユーザーエクスペリエンスを提供します。",
            },
        ],
        footerDescription: "ALCOMD3 は vrc-get/ALCOM を起源とする独立したオープンソースデスクトップツールです。",
        footerCopyright: "© CQMHV. ALCOMD3 はオープンソースソフトウェアプロジェクトであり、元プロジェクトおよび貢献者の権利を尊重します。",
        footerRelated: "関連プロジェクト",
        vrcalLink: "VRChatAvatarLearn",
    },
    "en-us": {
        lang: "en-us",
        uiLocale: "en-US",
        htmlLang: "en-US",
        ogLocale: "en_US",
        languageLabel: "Language",
        githubLabel: "Visit GitHub",
        contributorsTitle: "Contributors",
        mcpDocsLabel: "MCP Docs",
        mcpDocsPageTitle: "ALCOMD3 MCP Guide",
        mcpDocsPageDescription: "Read how to enable the ALCOMD3 local MCP bridge, configure clients, use available tools, understand lifecycle behavior, review permission boundaries, and troubleshoot setup issues.",
        mcpSectionTitle: "Optional Local MCP Integration",
        mcpSectionDescription: "The MCP docs are available now; this section summarizes the stdio bridge, local IPC, disabled-by-default data access, log queries, and scoped project/repository write tools.",
        appAdvantagesTitle: "Desktop Experience and Core Strengths",
        appearanceLabel: "Appearance",
        appearanceSystem: "Follow system",
        appearanceLight: "Light",
        appearanceDark: "Dark",
        title: "ALCOMD3 - Open-source VCC alternative for VRChat Unity projects",
        description: "ALCOMD3 is an open-source VRChat Creator Companion (VCC) alternative that originated from ALCOM/vrc-get for managing VRChat Unity projects, VPM packages, repositories, backups, updates, and optional local MCP integration.",
        twitterDescription: "An open-source VRChat Creator Companion (VCC) alternative for VRChat Unity project and VPM package management, with repositories, backups, updates, and optional local MCP bridge.",
        structuredDescription: "ALCOMD3 is an independent open-source desktop tool that originated from ALCOM/vrc-get. It serves as a VRChat Creator Companion (VCC) alternative for managing VRChat Unity projects, VPM repositories, and packages, with backups, updates, vcc:// association, and optional local MCP bridge. MCP uses stdio and local IPC to give AI agents visibility into projects, repositories, packages, environment settings, activity records, and technical logs, plus scoped operations for project creation, existing project registration, VPM repository registration, backup, copy, restore, and single-package install, uninstall, and reinstall.",
        seoKeywords: [
            "ALCOMD3",
            "VCC alternative",
            "VRChat Creator Companion alternative",
            "VRChat Unity project management",
            "ALCOM",
            "vrc-get",
            "VPM package management",
            "VRChat package manager",
        ],
        ogImageAlt: "ALCOMD3 open-source VCC alternative for VRChat Unity project management",
        softwareFeatures: [
            "VRChat Unity project management",
            "VPM package management",
            "VPM repository management",
            "Project backup and restore",
            "ALCOM/vrc-get origin and compatibility",
            "Optional local MCP bridge",
        ],
        productSubtitle: "VRChat project and VPM package management",
        tagline: "Manage projects, packages, backups, updates, and optional local MCP access",
        positioningTitle: "An open-source VCC alternative for VRChat Unity projects",
        positioningBody: "ALCOMD3 originated from ALCOM/vrc-get and is an independent desktop app for VRChat Unity project and VPM package management. It serves as an open-source VRChat Creator Companion (VCC) alternative with project creation, repositories, packages, backups, updates, and optional local MCP access.",
        positioningChips: [
            "VCC alternative",
            "VRChat Creator Companion alternative",
            "VRChat Unity",
            "ALCOM/vrc-get origin",
            "VPM package management",
        ],
        downloads: {
            sectionTitle: "Download ALCOMD3",
            sectionDescription: "Choose the stable or beta channel, then download a package that matches your operating system and processor architecture. Only actually published links are available here.",
            channelVersion: "{channel}: {version}",
            unavailable: "Not yet available in this channel",
            stableReleaseOnly: "Stable releases are not yet connected to the new multi-platform direct downloads. View the current stable release on GitHub.",
            betaCanonicalUnavailable: "A beta using the new platform-specific asset names has not been published yet. Assets from earlier beta releases are not exposed as new multi-platform downloads.",
            releasePage: "View {channel} {version} on GitHub",
            downloadAction: "Download {channel} {version} · {platform} ({format})",
            channels: {
                stable: {
                    name: "Stable",
                    description: "Intended for everyday use and verified through the stable release process.",
                },
                beta: {
                    name: "Beta",
                    description: "Preview new features early. Beta builds may be less stable than stable releases.",
                },
            },
            platforms: {
                "windows-x86_64": {
                    name: "Windows x64",
                    description: "Windows 10/11 · x64",
                },
                "darwin-aarch64": {
                    name: "macOS Apple silicon",
                    description: "M1 or later · Intel Macs are not supported",
                },
                "linux-x86_64": {
                    name: "Linux x86_64",
                    description: "Portable AppImage, plus a DEB package for Debian and Ubuntu",
                },
            },
            formats: {
                "windows-installer": "ZIP installer",
                "macos-apple-silicon": "DMG",
                "linux-appimage": "AppImage",
                "linux-deb": "DEB package",
            },
        },
        booth: "BOOTH",
        gumroad: "Gumroad",
        advantages: [
            {
                title: "Optional Local MCP Bridge",
                body: "Connect MCP-capable AI clients through a stdio MCP server, then read ALCOMD3 data through local IPC. MCP is disabled by default, and new tool calls return data only after it is enabled in the GUI.",
            },
            {
                title: "Visibility and Scoped Write Tools",
                body: "MCP exposes project, repository, package, environment, activity record, and technical log queries, plus scoped write tools for creating projects, adding existing project folders, adding VPM repositories, backup, copy, restore from zip backup, and install, uninstall, or reinstall of a single GUI-visible package in a registered project.",
            },
            {
                title: "Project, Package, and Log Visibility",
                body: "AI agents can list registered projects, inspect project package summaries, read repositories and GUI-visible packages, get Unity/default path/backup path summaries, and search redacted activity records and technical logs.",
            },
            {
                title: "Material Design 3 Interface",
                body: "A modern visual hierarchy, state feedback, theme color customization, and onboarding entry points make daily VRChat project management clearer and more comfortable.",
            },
            {
                title: "Clearer Operation Progress",
                body: "Package operations, project copy, and backups provide more detailed progress, including per-package status, retry, cancel, and terminate controls.",
            },
            {
                title: "ALCOMD3 Releases and Updates",
                body: "Use ALCOMD3-owned update sources, explicit platform availability, and versioned package names for a clear, consistent release experience across Windows, macOS, and Linux.",
            },
            {
                title: "Better Desktop Integration",
                body: "As a standalone desktop application, ALCOMD3 can better integrate with the operating system to provide a smoother user experience.",
            },
        ],
        footerDescription: "ALCOMD3 is an independent open-source desktop tool that originated from vrc-get/ALCOM.",
        footerCopyright: "© CQMHV. ALCOMD3 is an open-source software project. Rights of the upstream projects and contributors are respected.",
        footerRelated: "Related Project",
        vrcalLink: "VRChatAvatarLearn",
    },
};
