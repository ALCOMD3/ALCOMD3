"use client";

import { createFileRoute } from "@tanstack/react-router";
import { Palette } from "lucide-react";
import {
	GuiAnimationSwitch,
	GuiCompactSwitch,
	LanguageSelector,
} from "@/components/common-setting-parts";
import { openMaterialThemeDialog } from "@/components/MaterialThemePanel";
import { Button } from "@/components/ui/button";
import { CardDescription } from "@/components/ui/card";
import { tc } from "@/lib/i18n";
import { SetupPageBase } from "../-setup-page-base";

export const Route = createFileRoute("/_setup/setup/appearance/")({
	component: Page,
});

function Page() {
	return (
		<SetupPageBase
			heading={tc("setup:entry:welcome")}
			Body={Body}
			nextPage={"/setup/legacy-import"}
			prevPage={null}
			pageId={"Appearance"}
			withoutSteps
		/>
	);
}

function Body() {
	return (
		<>
			<CardDescription className={"whitespace-normal text-lg text-center"}>
				{tc("setup:entry:welcome description")}
			</CardDescription>
			<div className={"pb-3"} />
			<CardDescription className={"whitespace-normal"}>
				{tc("setup:appearance:description")}
			</CardDescription>
			<LanguageSelector />
			<GuiAnimationSwitch />
			<GuiCompactSwitch />
			<div className="flex flex-col items-center gap-2 pt-2 text-center">
				<CardDescription className="whitespace-normal">
					{tc("setup:appearance:theme description")}
				</CardDescription>
				<Button
					className="gap-2"
					onClick={() => void openMaterialThemeDialog()}
				>
					<Palette className="size-4" />
					{tc("settings:theme")}
				</Button>
			</div>
		</>
	);
}
