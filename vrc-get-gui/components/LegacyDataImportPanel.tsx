"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
	Download,
	List,
	Package,
	Palette,
	Settings as SettingsIcon,
} from "lucide-react";
import type React from "react";
import { Button } from "@/components/ui/button";
import type {
	TauriLegacyDataImportCategory,
	TauriLegacyDataSourceKind,
} from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { tc, tt } from "@/lib/i18n";
import {
	importedLegacyDataItemCount,
	invalidateLegacyDataImportQueries,
	mergeLegacyDataImportResults,
} from "@/lib/legacy-data-import";
import { applyPersistedMaterialTheme } from "@/lib/material-theme";
import { toastSuccess, toastThrownError } from "@/lib/toast";
import { cn } from "@/lib/utils";

export function LegacyDataImportPanel({
	className,
	hideWhenUnavailable = false,
}: {
	className?: string;
	hideWhenUnavailable?: boolean;
}) {
	const queryClient = useQueryClient();
	const sources = useQuery({
		queryKey: ["environmentLegacyDataSources"],
		queryFn: commands.environmentLegacyDataSources,
	});
	const legacySources = sources.data ?? [];

	const importLegacyData = useMutation({
		mutationFn: async (category: TauriLegacyDataImportCategory) => {
			const results = [];
			for (const source of legacySources) {
				results.push(
					await commands.environmentImportLegacyData(source.kind, category),
				);
			}
			return mergeLegacyDataImportResults(results);
		},
		onError: (e) => {
			console.error(e);
			toastThrownError(e);
		},
		onSuccess: async (result) => {
			if (result.imported_gui_config) {
				await applyPersistedMaterialTheme();
			}

			const count = importedLegacyDataItemCount(result);
			toastSuccess(tc("legacy import:toast:imported", { count }));
		},
		onSettled: async () => {
			await invalidateLegacyDataImportQueries(queryClient);
		},
	});

	if (sources.isLoading) {
		if (hideWhenUnavailable) {
			return null;
		}

		return (
			<p className={cn("text-sm opacity-70 whitespace-normal", className)}>
				{tc("general:loading...")}
			</p>
		);
	}

	return (
		<div className={cn("flex flex-col gap-2", className)}>
			<div className="flex flex-col gap-2 rounded-md border border-border p-3">
				<div className="flex min-w-0 flex-col gap-1 border-b border-border pb-3">
					<h3 className="text-base">{tc("general:source")}</h3>
					<div className="flex min-w-0 flex-col gap-1">
						{legacySources.length === 0 ? (
							<p className="whitespace-normal text-sm opacity-70">
								{tc("legacy import:no sources")}
							</p>
						) : (
							legacySources.map((source) => (
								<p
									className="grid min-w-0 gap-x-2 gap-y-0.5 text-sm md:grid-cols-[max-content_minmax(0,1fr)]"
									key={source.kind}
								>
									<span>{sourceName(source.kind)}</span>
									<span className="break-all opacity-70">{source.path}</span>
								</p>
							))
						)}
					</div>
				</div>
				<div className="divide-y divide-border">
					{IMPORT_CATEGORIES.map((category) => (
						<LegacyImportCategoryRow
							key={category.kind}
							category={category}
							disabled={importLegacyData.isPending}
							onImport={() => importLegacyData.mutate(category.kind)}
						/>
					))}
				</div>
			</div>
		</div>
	);
}

const IMPORT_CATEGORIES: ReadonlyArray<{
	kind: TauriLegacyDataImportCategory;
	icon: React.ComponentType<{ className?: string }>;
	titleKey: string;
	descriptionKey: string;
	itemKeys: string[];
	buttonKey: string;
}> = [
	{
		kind: "Projects",
		icon: List,
		titleKey: "legacy import:category:projects",
		descriptionKey: "legacy import:category projects:description",
		itemKeys: [
			"legacy import:item:database",
			"legacy import:item:project settings",
		],
		buttonKey: "legacy import:button:projects",
	},
	{
		kind: "Resources",
		icon: Package,
		titleKey: "legacy import:category:resources",
		descriptionKey: "legacy import:category resources:description",
		itemKeys: [
			"legacy import:item:resource settings",
			"legacy import:item:repositories",
			"legacy import:item:vcc templates",
			"legacy import:item:alcom templates",
			"legacy import:item:vrc-get settings",
		],
		buttonKey: "legacy import:button:resources",
	},
	{
		kind: "Theme",
		icon: Palette,
		titleKey: "legacy import:category:theme",
		descriptionKey: "legacy import:category theme:description",
		itemKeys: ["legacy import:item:theme config"],
		buttonKey: "legacy import:button:theme",
	},
	{
		kind: "Settings",
		icon: SettingsIcon,
		titleKey: "legacy import:category:settings",
		descriptionKey: "legacy import:category settings:description",
		itemKeys: [
			"legacy import:item:settings preferences",
			"legacy import:item:gui settings",
		],
		buttonKey: "legacy import:button:settings",
	},
];

function LegacyImportCategoryRow({
	category,
	disabled,
	onImport,
}: {
	category: (typeof IMPORT_CATEGORIES)[number];
	disabled: boolean;
	onImport: () => void;
}) {
	const Icon = category.icon;
	const items = category.itemKeys.map((key) => tt(key));

	return (
		<div className="flex flex-col gap-2 py-3 first:pt-2 last:pb-0 md:flex-row md:items-start">
			<div className="flex min-w-0 grow gap-3">
				<Icon className="mt-1 size-4 shrink-0 text-muted-foreground" />
				<div className="flex min-w-0 flex-col gap-1">
					<h4 className="text-sm font-medium">{tc(category.titleKey)}</h4>
					<p className="whitespace-normal text-sm opacity-70">
						{tc(category.descriptionKey)}
					</p>
					<p className="whitespace-normal text-sm opacity-70">
						{items.join(tt("general:list separator"))}
					</p>
				</div>
			</div>
			<Button
				className="gap-2 self-start md:self-center"
				disabled={disabled}
				onClick={onImport}
				size="sm"
			>
				<Download className="size-4" />
				{tc(category.buttonKey)}
			</Button>
		</div>
	);
}

function sourceName(source: TauriLegacyDataSourceKind) {
	switch (source) {
		case "Vcc":
			return tc("legacy import:source:vcc");
		case "Alcom":
			return tc("legacy import:source:alcom");
		case "Alcomd3Beta":
			return tc("legacy import:source:alcomd3 beta");
	}
}
