import alcomd3Config from "../../alcomd3.config.json";
import type { GlobalInfo as GlobalInfoBinding } from "./bindings.js";

type OsType = "Linux" | "Darwin" | "WindowsNT";
type Arch = "x86_64" | "aarch64";

type GlobalInfo = GlobalInfoBinding & {
	osType: OsType;
	arch: Arch;
};

const fallbackRepositoryUrl = `https://github.com/${alcomd3Config.repository}`;

const fallbackGlobalInfo: Readonly<GlobalInfo> = {
	language: "en",
	theme: "system",
	version: null,
	commitHash: null,
	osType: "WindowsNT",
	arch: "x86_64",
	osInfo: "unknown OS",
	webviewVersion: "unknown",
	appData: "",
	homepageUrl: alcomd3Config.homepageUrl,
	repositoryUrl: fallbackRepositoryUrl,
	defaultUnityArguments: [],
	vpmHomeFolder: "",
	checkForUpdates: false,
	shouldInstallDeepLink: false,
};

const globalInfo: Readonly<GlobalInfo> = load();

function load(): GlobalInfo {
	if ("vrcGetGlobalInfo" in globalThis) {
		console.log("found vrcGetGlobalInfo!");
		// @ts-expect-error
		const info = globalThis.vrcGetGlobalInfo as GlobalInfo;
		onload(info);
		return info;
	}
	return fallbackGlobalInfo;
}

function onload(info: Readonly<GlobalInfo>) {
	document.documentElement.setAttribute("lang", info.language);
}

export default globalInfo;

export function useGlobalInfo(): Readonly<GlobalInfo> {
	return globalInfo;
}

export function getHomepageUrl(): string {
	return globalInfo.homepageUrl.trim() || alcomd3Config.homepageUrl;
}

export function newRepositoryIssueUrl(): URL {
	const repositoryUrl =
		globalInfo.repositoryUrl.trim() || fallbackRepositoryUrl;
	return new URL("issues/new", `${repositoryUrl.replace(/\/+$/, "")}/`);
}
