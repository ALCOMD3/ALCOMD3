import type {
	TauriPackage,
	TauriProjectPackageLatestInfo,
	TauriProjectPackageRows,
	TauriVersion,
} from "@/lib/bindings";
import { toVersionString } from "@/lib/version";

export type PackageLatestInfo =
	| {
			status: "none";
	  }
	| {
			status: "contains";
			pkg: TauriPackage;
			hasUnityIncompatibleLatest: boolean;
	  }
	| {
			status: "upgradable";
			pkg: TauriPackage;
			hasUnityIncompatibleLatest: boolean;
	  };

type UrlInfo = {
	// null source means URL comes from installed one which has the highest priority
	url: string;
	source: TauriVersion | null;
};

export interface PackageRowInfo {
	id: string;
	infoSource: TauriVersion;
	displayName: string;
	description: string;
	keywords: string[];
	unityCompatible: Map<string, TauriPackage>;
	unityIncompatible: Map<string, TauriPackage>;
	sources: Set<string>;
	isThereSource: boolean; // this will be true even if all sources are hidden
	visibleSources: Set<string>;
	installed: null | {
		version: TauriVersion;
		yanked: boolean;
	};
	latest: PackageLatestInfo;
	stableLatest: PackageLatestInfo;
	changelogUrl: null | UrlInfo;
	documentationUrl: null | UrlInfo;
}

export function collectPackageRowsFromBackend(
	response: TauriProjectPackageRows | null | undefined,
): PackageRowInfo[] {
	return (
		response?.packages.map((row) => ({
			id: row.id,
			infoSource: row.info_source,
			displayName: row.display_name,
			description: row.description,
			keywords: row.keywords,
			unityCompatible: packageMap(row.unity_compatible),
			unityIncompatible: packageMap(row.unity_incompatible),
			sources: new Set(row.sources),
			isThereSource: row.is_there_source,
			visibleSources: new Set(row.visible_sources),
			installed: row.installed,
			latest: latestInfo(row.latest),
			stableLatest: latestInfo(row.stable_latest),
			changelogUrl: urlInfo(row.changelog_url),
			documentationUrl: urlInfo(row.documentation_url),
		})) ?? []
	);
}

function packageMap(packages: TauriPackage[]): Map<string, TauriPackage> {
	return new Map(
		packages.map((pkg) => [toVersionString(pkg.version), pkg] as const),
	);
}

function latestInfo(latest: TauriProjectPackageLatestInfo): PackageLatestInfo {
	switch (latest.status) {
		case "none":
			return { status: "none" };
		case "contains":
			return {
				status: "contains",
				pkg: latest.pkg,
				hasUnityIncompatibleLatest: latest.has_unity_incompatible_latest,
			};
		case "upgradable":
			return {
				status: "upgradable",
				pkg: latest.pkg,
				hasUnityIncompatibleLatest: latest.has_unity_incompatible_latest,
			};
	}
}

function urlInfo(
	value: TauriProjectPackageRows["packages"][number]["changelog_url"],
): UrlInfo | null {
	if (value == null) return null;
	return {
		url: value.url,
		source: value.source,
	};
}
