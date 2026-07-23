import {
    defaultRouteLocale as configDefaultRouteLocale,
    defaultUiLocale as configDefaultUiLocale,
    supportedRouteLocales as configSupportedRouteLocales,
    supportedUiLocales as configSupportedUiLocales,
} from "./i18n.config.mjs";

export const supportedUiLocales = configSupportedUiLocales as unknown as readonly [
    "zh-CN",
    "zh-TW",
    "ja-JP",
    "en-US",
];

export const supportedRouteLocales = configSupportedRouteLocales as unknown as readonly [
    "zh-cn",
    "zh-tw",
    "ja-jp",
    "en-us",
];

export const defaultUiLocale = configDefaultUiLocale as "en-US";

export const defaultRouteLocale = configDefaultRouteLocale as "en-us";

export type UiLocale = (typeof supportedUiLocales)[number];

export type RouteLocale = (typeof supportedRouteLocales)[number];

export function uiLocaleToRouteLocale(locale: UiLocale): RouteLocale {
    return locale.toLowerCase() as RouteLocale;
}

export function routeLocaleToUiLocale(locale: RouteLocale): UiLocale {
    return normalizeUiLocale(locale, supportedUiLocales) as UiLocale;
}

export function normalizeUiLocale(
    locale: string | null | undefined,
    supportedLocales: readonly string[] = supportedUiLocales,
): string | null {
    if (!locale) {
        return null;
    }

    const normalized = locale
        .trim()
        .replaceAll("_", "-")
        .toLowerCase();
    const supportedLocale = supportedLocales.find((candidate) => candidate.toLowerCase() === normalized);

    return supportedLocale ?? null;
}

export function normalizeRouteLocale(locale: string | null | undefined): RouteLocale | null {
    const uiLocale = normalizeUiLocale(locale);

    return uiLocale ? uiLocaleToRouteLocale(uiLocale as UiLocale) : null;
}

export function normalizeBrowserUiLocale(
    locale: string | null | undefined,
    supportedLocales: readonly string[] = supportedUiLocales,
): string | null {
    const exactLocale = normalizeUiLocale(locale, supportedLocales);

    if (exactLocale || !locale) {
        return exactLocale;
    }

    const normalized = locale
        .trim()
        .replaceAll("_", "-")
        .toLowerCase();

    if (normalized.startsWith("zh-tw") || normalized.startsWith("zh-hk") || normalized.startsWith("zh-mo")) {
        return normalizeUiLocale("zh-TW", supportedLocales);
    }

    if (normalized.startsWith("zh")) {
        return normalizeUiLocale("zh-CN", supportedLocales);
    }

    if (normalized.startsWith("ja")) {
        return normalizeUiLocale("ja-JP", supportedLocales);
    }

    if (normalized.startsWith("en")) {
        return normalizeUiLocale("en-US", supportedLocales);
    }

    return null;
}
