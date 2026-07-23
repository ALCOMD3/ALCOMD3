import { getAbsoluteLocaleUrl } from "astro:i18n";
import {
    defaultRouteLocale,
    routeLocaleToUiLocale,
    supportedRouteLocales,
    type RouteLocale,
    type UiLocale,
} from "@/data/i18n";

export type AlternateLocaleLink = {
    hreflang: UiLocale | "x-default";
    href: string;
};

export function getCanonicalUrl(locale: RouteLocale, path = ""): string {
    return getAbsoluteLocaleUrl(locale, path);
}

export function getAlternateLocaleLinks(path = ""): AlternateLocaleLink[] {
    return [
        ...supportedRouteLocales.map((locale) => ({
            hreflang: routeLocaleToUiLocale(locale),
            href: getCanonicalUrl(locale, path),
        })),
        {
            hreflang: "x-default",
            href: getCanonicalUrl(defaultRouteLocale, path),
        },
    ];
}
