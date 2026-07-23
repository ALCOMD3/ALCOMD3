import {
    defaultUiLocale,
    normalizeBrowserUiLocale,
    normalizeRouteLocale,
    normalizeUiLocale,
    routeLocaleToUiLocale,
    supportedUiLocales,
    uiLocaleToRouteLocale,
    type RouteLocale,
    type UiLocale,
} from "@/data/i18n";

export const uiLocaleStorageKey = "ui_locale";

export const themeModeStorageKey = "theme_mode";

// TODO: 接入登录系统后，账户 Cookie 中也使用同名键 ui_locale 和 theme_mode。
// 保持持久化偏好优先级为 localStorage > 账户 Cookie；URL 语言仍优先代表当前路由。
export const defaultThemeMode: ThemeModePreference = "system";

export type LocaleSource = "url" | "local" | "account" | "browser" | "default";

export type ThemeModeSource = "local" | "account" | "system" | "default";

export type ThemeModePreference = "light" | "dark" | "system";

export type ResolvedThemeMode = "light" | "dark";

export type ResolvedUiPreferences = {
    ui_locale: UiLocale;
    route_locale: RouteLocale;
    theme_mode: ThemeModePreference;
    resolved_theme_mode: ResolvedThemeMode;
    source: {
        ui_locale: LocaleSource;
        theme_mode: ThemeModeSource;
    };
};

export type ResolveUiPreferencesOptions = {
    urlLocale?: string | null;
    localLocale?: string | null;
    accountLocale?: string | null;
    browserLocale?: string | readonly string[] | null;
    localThemeMode?: string | null;
    accountThemeMode?: string | null;
    systemThemeMode?: ResolvedThemeMode;
    defaultLocale?: string;
    defaultThemeMode?: ThemeModePreference;
    supportedLocales?: readonly string[];
};

export function normalizeThemeMode(themeMode: string | null | undefined): ThemeModePreference | null {
    if (themeMode === "light" || themeMode === "dark" || themeMode === "system") {
        return themeMode;
    }

    return null;
}

export function resolveThemeMode(
    themeMode: ThemeModePreference,
    systemThemeMode: ResolvedThemeMode = "light",
): ResolvedThemeMode {
    return themeMode === "system" ? systemThemeMode : themeMode;
}

export function resolveUiPreferences(options: ResolveUiPreferencesOptions): ResolvedUiPreferences {
    const supportedLocales = options.supportedLocales ?? supportedUiLocales;
    const systemThemeMode = options.systemThemeMode ?? "light";
    const defaultLocale = normalizeUiLocale(options.defaultLocale ?? defaultUiLocale, supportedLocales)
        ?? supportedLocales[0]
        ?? defaultUiLocale;
    const defaultThemeModePreference = normalizeThemeMode(options.defaultThemeMode ?? defaultThemeMode)
        ?? defaultThemeMode;
    const browserLocales = Array.isArray(options.browserLocale)
        ? options.browserLocale
        : [options.browserLocale];

    const localeCandidates: Array<[LocaleSource, string | null | undefined]> = [
        ["url", options.urlLocale],
        ["local", options.localLocale],
        ["account", options.accountLocale],
        ...browserLocales.map((locale): [LocaleSource, string | null | undefined] => ["browser", locale]),
        ["default", defaultLocale],
    ];
    const localeResult = localeCandidates
        .map(([source, locale]) => ({
            source,
            uiLocale: source === "browser"
                ? normalizeBrowserUiLocale(locale, supportedLocales)
                : normalizeUiLocale(locale, supportedLocales),
        }))
        .find((candidate) => candidate.uiLocale);

    const themeModeCandidates: Array<[ThemeModeSource, string | null | undefined]> = [
        ["local", options.localThemeMode],
        ["account", options.accountThemeMode],
        ["default", defaultThemeModePreference],
    ];
    const themeModeResult = themeModeCandidates
        .map(([source, themeMode]) => ({
            source,
            themeMode: normalizeThemeMode(themeMode),
        }))
        .find((candidate) => candidate.themeMode);
    const themeMode = themeModeResult?.themeMode ?? defaultThemeModePreference;
    const uiLocale = (localeResult?.uiLocale ?? defaultLocale) as UiLocale;

    return {
        ui_locale: uiLocale,
        route_locale: uiLocaleToRouteLocale(uiLocale),
        theme_mode: themeMode,
        resolved_theme_mode: resolveThemeMode(themeMode, systemThemeMode),
        source: {
            ui_locale: localeResult?.source ?? "default",
            theme_mode: themeMode === "system" && themeModeResult?.source === "default"
                ? "system"
                : themeModeResult?.source ?? "default",
        },
    };
}

export function getRouteLocaleFromPath(pathname: string): RouteLocale | null {
    const segment = pathname.split("/").filter(Boolean)[0];
    const locale = normalizeRouteLocale(segment);

    return segment && locale === segment ? locale : null;
}

export {
    defaultUiLocale,
    normalizeBrowserUiLocale,
    normalizeRouteLocale,
    normalizeUiLocale,
    routeLocaleToUiLocale,
    supportedUiLocales,
    uiLocaleToRouteLocale,
};
