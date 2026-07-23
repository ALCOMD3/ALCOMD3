import alcomd3Config from "../../../alcomd3.config.json" with { type: "json" };
import stableUpdaterManifest from "../../public/api/gui/tauri-updater.json" with { type: "json" };
import betaUpdaterManifest from "../../public/api/gui/tauri-updater-beta.json" with { type: "json" };
import {
    createDownloadCatalog,
    createStableDownloadCatalog,
} from "./downloads.mjs";

const homepageUrl = alcomd3Config.homepageUrl.replace(/\/$/, "");
const repositoryUrl = `https://github.com/${alcomd3Config.repository}`;
const contributorsApiUrl = new URL(
    `/repos/${alcomd3Config.repository}/contributors`,
    "https://api.github.com",
);
contributorsApiUrl.searchParams.set("per_page", "100");
const publisherRepositoryOwner = alcomd3Config.repository.split("/")[0];
const stableRelease = createStableDownloadCatalog(alcomd3Config, stableUpdaterManifest);
const betaDownloads = createDownloadCatalog(alcomd3Config, betaUpdaterManifest, {
    allowEmpty: true,
    channel: "beta",
    unavailableOnContractMismatch: true,
});
const downloadChannels = [stableRelease, betaDownloads];

export const siteConfig = {
    url: homepageUrl,
    name: alcomd3Config.productName,
    shortName: alcomd3Config.productName,
    authorName: alcomd3Config.publisherName,
    authorUrl: `https://github.com/${publisherRepositoryOwner}`,
    repositoryUrl,
    contributorsApiUrl: contributorsApiUrl.href,
    downloadPath: "download",
    mcpDocsPath: "mcp",
    ogImagePath: "/assets/og-banner.png",
    ogImageWidth: 2048,
    ogImageHeight: 2048,
    ogImageType: "image/png",
    logoPath: "/assets/logo.png",
    pwaIcon192Path: "/assets/icon-192.png",
    pwaIcon512Path: "/assets/icon-512.png",
    stableRelease,
    betaDownloads,
    downloadChannels,
    themeColorLight: "#6cb6ff",
    themeColorDark: "#6cb6ff",
};

export const serviceWorkerCacheName = "alcomd3-site-v1";
