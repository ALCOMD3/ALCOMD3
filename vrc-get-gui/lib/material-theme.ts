import {
	argbFromHex,
	type DynamicColor,
	Hct,
	hexFromArgb,
	MaterialDynamicColors,
	SchemeContent,
	SchemeExpressive,
	SchemeFidelity,
	SchemeFruitSalad,
	SchemeMonochrome,
	SchemeNeutral,
	SchemeRainbow,
	SchemeTonalSpot,
	SchemeVibrant,
} from "@material/material-color-utilities";
import { commands } from "@/lib/bindings";

export const DEFAULT_THEME_SOURCE_HEX = "#6cb6ff";
export const DEFAULT_THEME_MODE = "auto";
export const DEFAULT_THEME_SCHEME = "vibrant";

export const USER_THEME_SOURCE_KEY = "user_theme_source";
export const USER_THEME_MODE_KEY = "user_theme_mode";
export const USER_THEME_SCHEME_KEY = "user_theme_scheme";
export const USER_THEME_STYLE_ID = "user-theme-style";
const USER_THEME_CONFIG_PREFIX = "material:";

export const THEME_SCHEME_LABELS = {
	"tonal-spot": "settings:theme:scheme:tonal-spot",
	fidelity: "settings:theme:scheme:fidelity",
	monochrome: "settings:theme:scheme:monochrome",
	neutral: "settings:theme:scheme:neutral",
	vibrant: "settings:theme:scheme:vibrant",
	expressive: "settings:theme:scheme:expressive",
	content: "settings:theme:scheme:content",
	rainbow: "settings:theme:scheme:rainbow",
	"fruit-salad": "settings:theme:scheme:fruit-salad",
} as const;

export type ThemeSchemeName = keyof typeof THEME_SCHEME_LABELS;
export type ThemeMode = "auto" | "light" | "dark";
export type MaterialThemeSettings = {
	sourceHex: string;
	mode: ThemeMode;
	scheme: ThemeSchemeName;
};

export const THEME_SCHEME_NAMES = Object.keys(
	THEME_SCHEME_LABELS,
) as ThemeSchemeName[];

const FULL_THEME_ROLE_NAMES = [
	"primary",
	"surface-tint",
	"on-primary",
	"primary-container",
	"on-primary-container",
	"secondary",
	"on-secondary",
	"secondary-container",
	"on-secondary-container",
	"tertiary",
	"on-tertiary",
	"tertiary-container",
	"on-tertiary-container",
	"error",
	"on-error",
	"error-container",
	"on-error-container",
	"background",
	"on-background",
	"surface",
	"on-surface",
	"surface-variant",
	"on-surface-variant",
	"outline",
	"outline-variant",
	"shadow",
	"scrim",
	"inverse-surface",
	"inverse-on-surface",
	"inverse-primary",
	"primary-fixed",
	"on-primary-fixed",
	"primary-fixed-dim",
	"on-primary-fixed-variant",
	"secondary-fixed",
	"on-secondary-fixed",
	"secondary-fixed-dim",
	"on-secondary-fixed-variant",
	"tertiary-fixed",
	"on-tertiary-fixed",
	"tertiary-fixed-dim",
	"on-tertiary-fixed-variant",
	"surface-dim",
	"surface-bright",
	"surface-container-lowest",
	"surface-container-low",
	"surface-container",
	"surface-container-high",
	"surface-container-highest",
] as const;

export function clamp(value: number, min: number, max: number) {
	return Math.min(max, Math.max(min, value));
}

export function normalizeHexColor(value: unknown) {
	if (typeof value !== "string") return null;
	const normalized = value.trim().toLowerCase();
	return /^#[0-9a-fA-F]{6}$/.test(normalized) ? normalized : null;
}

export function normalizeThemeMode(value: unknown): ThemeMode {
	return value === "light" || value === "dark" || value === "auto"
		? value
		: DEFAULT_THEME_MODE;
}

export function normalizeThemeScheme(value: unknown): ThemeSchemeName {
	return typeof value === "string" &&
		THEME_SCHEME_NAMES.includes(value as ThemeSchemeName)
		? (value as ThemeSchemeName)
		: DEFAULT_THEME_SCHEME;
}

export function getStoredMaterialTheme(): MaterialThemeSettings {
	if (typeof window === "undefined") {
		return {
			sourceHex: DEFAULT_THEME_SOURCE_HEX,
			mode: DEFAULT_THEME_MODE,
			scheme: DEFAULT_THEME_SCHEME,
		};
	}

	return {
		sourceHex:
			normalizeHexColor(localStorage.getItem(USER_THEME_SOURCE_KEY)) ??
			DEFAULT_THEME_SOURCE_HEX,
		mode: normalizeThemeMode(localStorage.getItem(USER_THEME_MODE_KEY)),
		scheme: normalizeThemeScheme(localStorage.getItem(USER_THEME_SCHEME_KEY)),
	};
}

function parseStoredMaterialTheme(
	value: unknown,
): MaterialThemeSettings | null {
	if (typeof value !== "string") return null;
	if (!value.startsWith(USER_THEME_CONFIG_PREFIX)) return null;

	try {
		const parsed = JSON.parse(value.slice(USER_THEME_CONFIG_PREFIX.length));
		return {
			sourceHex:
				normalizeHexColor(parsed?.sourceHex) ?? DEFAULT_THEME_SOURCE_HEX,
			mode: normalizeThemeMode(parsed?.mode),
			scheme: normalizeThemeScheme(parsed?.scheme),
		};
	} catch (error) {
		console.warn("failed to parse material theme config", error);
		return null;
	}
}

function serializeMaterialTheme(settings: MaterialThemeSettings) {
	return `${USER_THEME_CONFIG_PREFIX}${JSON.stringify(settings)}`;
}

export function saveMaterialTheme({
	sourceHex,
	mode,
	scheme,
}: MaterialThemeSettings) {
	localStorage.setItem(USER_THEME_SOURCE_KEY, sourceHex);
	localStorage.setItem(USER_THEME_MODE_KEY, mode);
	localStorage.setItem(USER_THEME_SCHEME_KEY, scheme);
}

export async function getPersistedMaterialTheme() {
	try {
		const configTheme = await commands.environmentTheme();
		const parsed = parseStoredMaterialTheme(configTheme);
		if (parsed) {
			saveMaterialTheme(parsed);
			return parsed;
		}
	} catch (error) {
		console.warn("failed to load material theme config", error);
	}

	return getStoredMaterialTheme();
}

export async function savePersistedMaterialTheme(
	settings: MaterialThemeSettings,
) {
	saveMaterialTheme(settings);
	try {
		await commands.environmentSetTheme(serializeMaterialTheme(settings));
	} catch (error) {
		console.warn("failed to save material theme config", error);
	}
}

export function applyStoredMaterialTheme() {
	const settings = getStoredMaterialTheme();
	applyMaterialTheme(settings.sourceHex, settings.mode, settings.scheme);
	return settings;
}

export async function applyPersistedMaterialTheme() {
	const settings = await getPersistedMaterialTheme();
	applyMaterialTheme(settings.sourceHex, settings.mode, settings.scheme);
	return settings;
}

export function hctFromHex(sourceHex: string) {
	const hct = Hct.fromInt(argbFromHex(sourceHex));
	return {
		hue: Math.round(hct.hue),
		chroma: Math.round(clamp(hct.chroma, 0, 150)),
		tone: Math.round(clamp(hct.tone, 0, 100)),
	};
}

export function hexFromHctInputs(hue: number, chroma: number, tone: number) {
	return hexFromArgb(
		Hct.from(
			clamp(hue, 0, 360),
			clamp(chroma, 0, 150),
			clamp(tone, 0, 100),
		).toInt(),
	);
}

function kebabToCamelCase(value: string) {
	return value.replace(/-([a-z])/g, (_, letter: string) =>
		letter.toUpperCase(),
	);
}

function getScheme(
	sourceHct: Hct,
	isDark: boolean,
	schemeName: ThemeSchemeName,
) {
	switch (schemeName) {
		case "fidelity":
			return new SchemeFidelity(sourceHct, isDark, 0);
		case "monochrome":
			return new SchemeMonochrome(sourceHct, isDark, 0);
		case "neutral":
			return new SchemeNeutral(sourceHct, isDark, 0);
		case "vibrant":
			return new SchemeVibrant(sourceHct, isDark, 0);
		case "expressive":
			return new SchemeExpressive(sourceHct, isDark, 0);
		case "content":
			return new SchemeContent(sourceHct, isDark, 0);
		case "rainbow":
			return new SchemeRainbow(sourceHct, isDark, 0);
		case "fruit-salad":
			return new SchemeFruitSalad(sourceHct, isDark, 0);
		case "tonal-spot":
			return new SchemeTonalSpot(sourceHct, isDark, 0);
	}
}

export function buildFullMaterialThemeCss(
	sourceHex: string,
	isDark: boolean,
	schemeName: ThemeSchemeName,
) {
	const sourceHct = Hct.fromInt(argbFromHex(sourceHex));
	const scheme = getScheme(sourceHct, isDark, schemeName);
	const colors = new MaterialDynamicColors();

	const declarations = FULL_THEME_ROLE_NAMES.map((roleName) => {
		const accessorName = kebabToCamelCase(roleName);
		const dynamicColorFactory = (
			colors as unknown as Record<string, () => DynamicColor | undefined>
		)[accessorName];
		const dynamicColor = dynamicColorFactory?.();
		if (!dynamicColor) return null;
		const value = hexFromArgb(dynamicColor.getArgb(scheme));
		return `--md-sys-color-${roleName}: ${value};`;
	}).filter(Boolean);

	return `:root, .light, .dark, .system {\n    ${declarations.join("\n    ")}\n}`;
}

export function isDarkThemeActive(mode: ThemeMode) {
	if (mode === "dark") return true;
	if (mode === "light") return false;

	return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function applyMaterialTheme(
	sourceHex: string,
	mode: ThemeMode,
	schemeName: ThemeSchemeName,
) {
	const normalizedHex = normalizeHexColor(sourceHex);
	if (!normalizedHex) return false;

	let styleEl = document.getElementById(USER_THEME_STYLE_ID);
	if (!styleEl) {
		styleEl = document.createElement("style");
		styleEl.id = USER_THEME_STYLE_ID;
		document.head.appendChild(styleEl);
	}
	const darkThemeActive = isDarkThemeActive(mode);
	document.documentElement.classList.remove("light", "dark", "system");
	document.documentElement.classList.add(darkThemeActive ? "dark" : "light");
	styleEl.textContent = buildFullMaterialThemeCss(
		normalizedHex,
		darkThemeActive,
		schemeName,
	);
	document.documentElement.style.setProperty(
		"--theme-hue",
		`${hctFromHex(normalizedHex).hue}`,
	);
	return true;
}
