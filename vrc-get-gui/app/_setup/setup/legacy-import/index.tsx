"use client";

import { createFileRoute } from "@tanstack/react-router";
import { LegacyDataImportPanel } from "@/components/LegacyDataImportPanel";
import { CardDescription } from "@/components/ui/card";
import { tc } from "@/lib/i18n";
import { SetupPageBase } from "../-setup-page-base";

export const Route = createFileRoute("/_setup/setup/legacy-import/")({
	component: Page,
});

function Page() {
	return (
		<SetupPageBase
			heading={tc("setup:legacy import:heading")}
			Body={Body}
			nextPage={"/setup/unity-hub"}
			prevPage={"/setup/appearance"}
			pageId={"LegacyImport"}
		/>
	);
}

function Body() {
	return (
		<>
			<CardDescription className="whitespace-normal">
				{tc("setup:legacy import:description")}
			</CardDescription>
			<LegacyDataImportPanel />
		</>
	);
}
