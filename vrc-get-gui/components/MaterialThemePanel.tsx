"use client";

import { Palette, RotateCcw } from "lucide-react";
import type React from "react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { DialogFooter, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { type DialogContext, openSingleDialog } from "@/lib/dialog";
import { tc, tt } from "@/lib/i18n";
import {
	applyMaterialTheme,
	DEFAULT_THEME_MODE,
	DEFAULT_THEME_SCHEME,
	DEFAULT_THEME_SOURCE_HEX,
	getPersistedMaterialTheme,
	getStoredMaterialTheme,
	hctFromHex,
	hexFromHctInputs,
	normalizeHexColor,
	savePersistedMaterialTheme,
	THEME_SCHEME_LABELS,
	THEME_SCHEME_NAMES,
	type ThemeMode,
	type ThemeSchemeName,
	USER_THEME_MODE_KEY,
	USER_THEME_SCHEME_KEY,
	USER_THEME_SOURCE_KEY,
} from "@/lib/material-theme";
import { cn } from "@/lib/utils";

export function openMaterialThemeDialog() {
	return openSingleDialog(MaterialThemeDialog, {}, "theme-dialog-content");
}

export function MaterialThemeButton({ className }: { className?: string }) {
	return (
		<Button
			variant="ghost"
			className={cn(
				"justify-start h-12 px-4 rounded-full text-[var(--md-sys-color-on-surface)] hover:bg-[var(--md-sys-color-surface-container-highest)] hover:text-[var(--md-sys-color-on-surface)]",
				className,
			)}
			onClick={() => void openMaterialThemeDialog()}
		>
			<div className="mr-4 compact:mr-0">
				<Palette className="h-5 w-5" />
			</div>
			<span className="compact:hidden">{tc("settings:theme")}</span>
		</Button>
	);
}

export function MaterialThemeDialog({
	dialog,
}: {
	dialog: DialogContext<void>;
}) {
	return (
		<>
			<ScrollArea
				type="scroll"
				className="max-h-[min(680px,calc(100dvh-12rem))] w-full"
				scrollBarClassName="bg-transparent py-2.5"
			>
				<div className="pr-4">
					<MaterialThemePanel />
				</div>
			</ScrollArea>
			<DialogFooter>
				<Button onClick={() => dialog.close()}>
					{tc("general:button:close")}
				</Button>
			</DialogFooter>
		</>
	);
}

export function MaterialThemePanel({
	showTitle = true,
	titleInDialog = true,
}: {
	showTitle?: boolean;
	titleInDialog?: boolean;
}) {
	const initialTheme = useState(getStoredMaterialTheme)[0];
	const [persistedThemeLoaded, setPersistedThemeLoaded] = useState(false);
	const [sourceHex, setSourceHex] = useState(initialTheme.sourceHex);
	const [hexText, setHexText] = useState(initialTheme.sourceHex);
	const [mode, setMode] = useState<ThemeMode>(initialTheme.mode);
	const [scheme, setScheme] = useState<ThemeSchemeName>(initialTheme.scheme);
	const [hct, setHct] = useState(() => hctFromHex(initialTheme.sourceHex));

	useEffect(() => {
		let cancelled = false;
		void getPersistedMaterialTheme().then((theme) => {
			if (cancelled) return;
			const themeChanged =
				theme.sourceHex !== initialTheme.sourceHex ||
				theme.mode !== initialTheme.mode ||
				theme.scheme !== initialTheme.scheme;
			if (themeChanged) {
				setSourceHex(theme.sourceHex);
				setHexText(theme.sourceHex);
				setMode(theme.mode);
				setScheme(theme.scheme);
				setHct(hctFromHex(theme.sourceHex));
				applyMaterialTheme(theme.sourceHex, theme.mode, theme.scheme);
			}
			setPersistedThemeLoaded(true);
		});
		return () => {
			cancelled = true;
		};
	}, [initialTheme]);

	useEffect(() => {
		if (!persistedThemeLoaded) return;
		applyMaterialTheme(sourceHex, mode, scheme);
		void savePersistedMaterialTheme({ sourceHex, mode, scheme });
	}, [sourceHex, mode, scheme, persistedThemeLoaded]);

	useEffect(() => {
		if (mode !== "auto") return;

		const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
		const refreshTheme = () => applyMaterialTheme(sourceHex, mode, scheme);
		mediaQuery.addEventListener("change", refreshTheme);
		return () => mediaQuery.removeEventListener("change", refreshTheme);
	}, [sourceHex, mode, scheme]);

	const updateSourceHex = (nextHex: string, syncHct = true) => {
		const normalizedHex = normalizeHexColor(nextHex);
		if (!normalizedHex) return;
		setSourceHex(normalizedHex);
		setHexText(normalizedHex);
		if (syncHct) setHct(hctFromHex(normalizedHex));
	};

	const updateHct = (key: keyof typeof hct, value: number) => {
		const nextHct = { ...hct, [key]: value };
		setHct(nextHct);
		updateSourceHex(
			hexFromHctInputs(nextHct.hue, nextHct.chroma, nextHct.tone),
			false,
		);
	};

	const resetTheme = () => {
		localStorage.removeItem(USER_THEME_SOURCE_KEY);
		localStorage.removeItem(USER_THEME_MODE_KEY);
		localStorage.removeItem(USER_THEME_SCHEME_KEY);
		setMode(DEFAULT_THEME_MODE);
		setScheme(DEFAULT_THEME_SCHEME);
		updateSourceHex(DEFAULT_THEME_SOURCE_HEX);
	};

	return (
		<div className="theme-panel__content">
			<div className="theme-panel__title-row">
				{showTitle && titleInDialog && (
					<DialogTitle className="theme-panel__title pb-0">
						{tc("settings:theme")}
					</DialogTitle>
				)}
				{showTitle && !titleInDialog && (
					<h2 className="theme-panel__title pb-0">{tc("settings:theme")}</h2>
				)}
				<Button
					variant="ghost"
					size="icon"
					className={!showTitle ? "ml-auto" : undefined}
					onClick={resetTheme}
					aria-label={tt("settings:theme:reset")}
				>
					<RotateCcw className="size-4" />
				</Button>
			</div>

			<div className="theme-panel__main">
				<div className="theme-panel__column">
					<div className="theme-card">
						<Label className="theme-card__label" htmlFor="theme-color-input">
							{tc("settings:theme:source color")}
						</Label>
						<div className="theme-card__color-row">
							<input
								id="theme-color-input"
								type="color"
								className="theme-card__picker"
								value={sourceHex}
								onChange={(event) => updateSourceHex(event.target.value)}
							/>
							<Input
								className="min-w-0 flex-1"
								value={hexText}
								onChange={(event) => setHexText(event.target.value)}
								onBlur={() => {
									const normalizedHex = normalizeHexColor(hexText);
									if (normalizedHex) {
										updateSourceHex(normalizedHex);
									} else {
										setHexText(sourceHex);
									}
								}}
							/>
						</div>
					</div>

					<div className="theme-sliders">
						<ThemeSlider
							label={tc("settings:theme:hue")}
							value={hct.hue}
							min={0}
							max={360}
							trackClassName="theme-slider__track--hue"
							onChange={(value) => updateHct("hue", value)}
						/>
						<ThemeSlider
							label={tc("settings:theme:chroma")}
							value={hct.chroma}
							min={0}
							max={150}
							trackClassName="theme-slider__track--chroma"
							onChange={(value) => updateHct("chroma", value)}
						/>
						<ThemeSlider
							label={tc("settings:theme:tone")}
							value={hct.tone}
							min={0}
							max={100}
							trackClassName="theme-slider__track--tone"
							onChange={(value) => updateHct("tone", value)}
						/>
					</div>
				</div>

				<div className="theme-panel__column theme-panel__column--options">
					<div className="theme-mode-toggle">
						<ThemeModeButton mode={mode} value="auto" onChange={setMode}>
							{tc("settings:theme:system")}
						</ThemeModeButton>
						<ThemeModeButton mode={mode} value="light" onChange={setMode}>
							{tc("settings:theme:light")}
						</ThemeModeButton>
						<ThemeModeButton mode={mode} value="dark" onChange={setMode}>
							{tc("settings:theme:dark")}
						</ThemeModeButton>
					</div>

					<div className="theme-schemes">
						<p className="theme-schemes__title">
							{tc("settings:theme:scheme")}
						</p>
						<div className="theme-schemes-list">
							{THEME_SCHEME_NAMES.map((schemeName) => (
								<label className="theme-scheme-option" key={schemeName}>
									<input
										type="radio"
										name="theme-scheme"
										value={schemeName}
										checked={scheme === schemeName}
										onChange={() => setScheme(schemeName)}
									/>
									{tc(THEME_SCHEME_LABELS[schemeName])}
								</label>
							))}
						</div>
					</div>
				</div>
			</div>
		</div>
	);
}

function ThemeModeButton({
	mode,
	value,
	onChange,
	children,
}: {
	mode: ThemeMode;
	value: ThemeMode;
	onChange: (value: ThemeMode) => void;
	children: React.ReactNode;
}) {
	return (
		<button
			type="button"
			className={cn("theme-mode-toggle__button", mode === value && "selected")}
			onClick={() => onChange(value)}
		>
			{children}
		</button>
	);
}

function ThemeSlider({
	label,
	value,
	min,
	max,
	trackClassName,
	onChange,
}: {
	label: React.ReactNode;
	value: number;
	min: number;
	max: number;
	trackClassName: string;
	onChange: (value: number) => void;
}) {
	return (
		<label className="theme-slider">
			<span>
				{label}：{Math.round(value)}
			</span>
			<input
				type="range"
				min={min}
				max={max}
				value={value}
				onChange={(event) => onChange(Number(event.target.value))}
			/>
			<i className={cn("theme-slider__track", trackClassName)} />
		</label>
	);
}
