import type { QueryClient } from "@tanstack/react-query";
import type { TauriLegacyDataImportResult } from "@/lib/bindings";

export async function invalidateLegacyDataImportQueries(
	queryClient: QueryClient,
) {
	await Promise.all(
		[
			["environmentLegacyDataSources"],
			["environmentGetSettings"],
			["environmentProjects"],
			["environmentPackages"],
			["environmentRepositoriesInfo"],
			["environmentProjectCreationInformation"],
			["environmentTheme"],
		].map((queryKey) => queryClient.invalidateQueries({ queryKey })),
	);
}

export function importedLegacyDataItemCount(
	result: TauriLegacyDataImportResult,
) {
	return [
		result.imported_settings,
		result.imported_database,
		result.imported_repositories,
		result.imported_vcc_templates,
		result.imported_alcom_templates,
		result.imported_vrc_get_settings,
		result.imported_gui_config,
	].filter(Boolean).length;
}

export function mergeLegacyDataImportResults(
	results: TauriLegacyDataImportResult[],
): TauriLegacyDataImportResult {
	return {
		imported_settings: results.some((result) => result.imported_settings),
		imported_database: results.some((result) => result.imported_database),
		imported_repositories: results.some(
			(result) => result.imported_repositories,
		),
		imported_vcc_templates: results.some(
			(result) => result.imported_vcc_templates,
		),
		imported_alcom_templates: results.some(
			(result) => result.imported_alcom_templates,
		),
		imported_vrc_get_settings: results.some(
			(result) => result.imported_vrc_get_settings,
		),
		imported_gui_config: results.some((result) => result.imported_gui_config),
	};
}
