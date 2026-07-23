declare module "beercss/dist/cdn/beer.min.js" {
    export interface BeerCssTheme {
        dark: string;
        light: string;
    }

    export default function ui(
        selector?: string | Element,
        options?: string | number | BeerCssTheme,
    ): string | undefined | Promise<BeerCssTheme>;
}
