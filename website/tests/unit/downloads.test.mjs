import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
    createDownloadCatalog,
    createReleaseSummary,
    createStableDownloadCatalog,
} from "../../src/data/downloads.mjs";
import { siteConfig } from "../../src/data/site.config.mjs";

const releasePlatforms = {
    "windows-x86_64": {
        target: "x86_64-pc-windows-msvc",
        updater: {
            assetPattern: "ALCOMD3_{version}_windows_x86_64_setup.exe",
        },
        downloads: [
            {
                id: "windows-installer",
                format: "zip",
                assetPattern: "ALCOMD3_{version}_windows_x86_64_setup.exe.zip",
                primary: true,
            },
        ],
    },
    "darwin-aarch64": {
        target: "aarch64-apple-darwin",
        macosAdHocSigning: {},
        updater: {
            assetPattern: "ALCOMD3_{version}_macos_aarch64.app.tar.gz",
        },
        downloads: [
            {
                id: "macos-apple-silicon",
                format: "dmg",
                assetPattern: "ALCOMD3_{version}_macos_aarch64.dmg",
                primary: true,
            },
        ],
    },
    "linux-x86_64": {
        target: "x86_64-unknown-linux-gnu",
        updater: {
            assetPattern: "ALCOMD3_{version}_linux_x86_64.AppImage.tar.gz",
        },
        downloads: [
            {
                id: "linux-appimage",
                format: "appimage",
                assetPattern: "ALCOMD3_{version}_linux_x86_64.AppImage",
                primary: true,
            },
            {
                id: "linux-deb",
                format: "deb",
                assetPattern: "ALCOMD3_{version}_linux_amd64.deb",
                primary: false,
            },
        ],
    },
};

function createConfig() {
    return {
        repository: "ALCOMD3/ALCOMD3",
        releasePlatforms: structuredClone(releasePlatforms),
    };
}

function createManifest(platformKeys = Object.keys(releasePlatforms), version = "3.4.5-beta.1") {
    return {
        version,
        platforms: Object.fromEntries(platformKeys.map((platformKey) => {
            const updaterFileName = releasePlatforms[platformKey].updater.assetPattern
                .replace("{version}", version);

            return [
                platformKey,
                {
                    url: `https://github.com/ALCOMD3/ALCOMD3/releases/download/v${version}/${updaterFileName}`,
                    signature: "test-signature",
                },
            ];
        })),
    };
}

describe("createDownloadCatalog", () => {
    test("derives all canonical beta downloads from the matching updater release directory", () => {
        const catalog = createDownloadCatalog(createConfig(), createManifest(), { channel: "beta" });

        assert.equal(catalog.version, "3.4.5-beta.1");
        assert.equal(catalog.releaseUrl, "https://github.com/ALCOMD3/ALCOMD3/releases/tag/v3.4.5-beta.1");
        assert.equal(catalog.hasDownloads, true);
        assert.deepEqual(catalog.operatingSystems, ["Windows", "macOS", "Linux"]);
        assert.deepEqual(
            catalog.platforms.find((platform) => platform.key === "darwin-aarch64"),
            {
                key: "darwin-aarch64",
                target: "aarch64-apple-darwin",
                operatingSystem: "macOS",
                available: true,
                downloads: [
                    {
                        id: "macos-apple-silicon",
                        format: "dmg",
                        assetName: "ALCOMD3_3.4.5-beta.1_macos_aarch64.dmg",
                        primary: true,
                        url: "https://github.com/ALCOMD3/ALCOMD3/releases/download/v3.4.5-beta.1/ALCOMD3_3.4.5-beta.1_macos_aarch64.dmg",
                    },
                ],
            },
        );
        assert.deepEqual(catalog.downloadUrls, [
            "https://github.com/ALCOMD3/ALCOMD3/releases/download/v3.4.5-beta.1/ALCOMD3_3.4.5-beta.1_windows_x86_64_setup.exe.zip",
            "https://github.com/ALCOMD3/ALCOMD3/releases/download/v3.4.5-beta.1/ALCOMD3_3.4.5-beta.1_macos_aarch64.dmg",
            "https://github.com/ALCOMD3/ALCOMD3/releases/download/v3.4.5-beta.1/ALCOMD3_3.4.5-beta.1_linux_x86_64.AppImage",
            "https://github.com/ALCOMD3/ALCOMD3/releases/download/v3.4.5-beta.1/ALCOMD3_3.4.5-beta.1_linux_amd64.deb",
        ]);
    });

    test("marks the beta catalog unpublished when an asset does not satisfy the release contract", () => {
        const manifest = createManifest();
        manifest.platforms["windows-x86_64"].url =
            "https://github.com/ALCOMD3/ALCOMD3/releases/download/v3.4.5-beta.1/alcomd3-3.4.5-beta.1-setup.exe";
        const catalog = createDownloadCatalog(createConfig(), manifest, {
            allowEmpty: true,
            channel: "beta",
            unavailableOnContractMismatch: true,
        });

        assert.equal(catalog.hasDownloads, false);
        assert.deepEqual(catalog.downloadUrls, []);
        assert.ok(catalog.platforms.every((platform) => !platform.available));
        assert.ok(catalog.platforms.every((platform) => platform.downloads.every((download) => !download.url)));
    });

    test("marks configured platforms unavailable when they are absent from a canonical beta manifest", () => {
        const catalog = createDownloadCatalog(
            createConfig(),
            createManifest(["windows-x86_64"]),
            { channel: "beta" },
        );

        assert.deepEqual(catalog.platforms.map(({ key, available }) => ({ key, available })), [
            { key: "windows-x86_64", available: true },
            { key: "darwin-aarch64", available: false },
            { key: "linux-x86_64", available: false },
        ]);
        assert.equal(catalog.downloadUrls.length, 1);
    });

    test("rejects updater URLs that point at another repository in contract-unavailable mode", () => {
        const manifest = createManifest(["windows-x86_64"]);
        manifest.platforms["windows-x86_64"].url =
            "https://github.com/example/other/releases/download/v3.4.5-beta.1/ALCOMD3_3.4.5-beta.1_windows_x86_64_setup.exe";

        assert.throws(
            () => createDownloadCatalog(createConfig(), manifest, {
                allowEmpty: true,
                channel: "beta",
                unavailableOnContractMismatch: true,
            }),
            /URL does not match the canonical release asset/,
        );
    });

    test("rejects release asset patterns that could escape the release directory", () => {
        const config = createConfig();
        config.releasePlatforms["windows-x86_64"].downloads[0].assetPattern = "../ALCOMD3_{version}.zip";

        assert.throws(
            () => createDownloadCatalog(config, createManifest(["windows-x86_64"]), { channel: "beta" }),
            /invalid release asset name/,
        );
    });

    test("rejects macOS downloads without the required ad-hoc signing configuration", () => {
        const config = createConfig();
        delete config.releasePlatforms["darwin-aarch64"].macosAdHocSigning;

        assert.throws(
            () => createDownloadCatalog(config, createManifest(), { channel: "beta" }),
            /macosAdHocSigning must be configured/,
        );
    });

    test("rejects attempts to add a configurable macOS signing mode", () => {
        const config = createConfig();
        config.releasePlatforms["darwin-aarch64"].macosAdHocSigning.mode = "certificate";

        assert.throws(
            () => createDownloadCatalog(config, createManifest(), { channel: "beta" }),
            /macosAdHocSigning contains unsupported fields: mode/,
        );
    });

    test("rejects manifest platforms that have no shared release configuration", () => {
        const manifest = createManifest(["windows-x86_64"]);
        manifest.platforms["future-platform"] = {
            url: "https://github.com/ALCOMD3/ALCOMD3/releases/download/v3.4.5-beta.1/future.tar.gz",
        };

        assert.throws(
            () => createDownloadCatalog(createConfig(), manifest, { channel: "beta" }),
            /unconfigured platform future-platform/,
        );
    });
});

describe("stable release handling", () => {
    test("creates a stable release summary without validating or exposing assets", () => {
        const manifest = {
            version: "2.1.1",
            platforms: {
                "windows-x86_64": {
                    url: "https://example.invalid/legacy-installer.exe",
                },
            },
        };
        const summary = createReleaseSummary(createConfig(), manifest, "stable");

        assert.equal(summary.releasePageOnly, true);
        assert.equal(summary.hasDownloads, false);
        assert.deepEqual(summary.platforms, []);
        assert.deepEqual(summary.downloadUrls, []);
    });

    test("keeps legacy stable assets on the release page without asset aliases", () => {
        const manifest = createManifest(["windows-x86_64"], "2.1.1");
        manifest.platforms["windows-x86_64"].url =
            "https://github.com/CQMHV/ALCOMD3/releases/download/v2.1.1/alcomd3-2.1.1-setup.exe";

        const catalog = createStableDownloadCatalog(createConfig(), manifest);

        assert.equal(catalog.version, "2.1.1");
        assert.equal(catalog.releasePageOnly, true);
        assert.equal(catalog.hasDownloads, false);
        assert.deepEqual(catalog.platforms, []);
    });

    test("exposes canonical stable platform downloads without compatibility aliases", () => {
        const catalog = createStableDownloadCatalog(createConfig(), createManifest(undefined, "2.1.2"));

        assert.equal(catalog.version, "2.1.2");
        assert.equal(catalog.releasePageOnly, false);
        assert.equal(catalog.hasDownloads, true);
        assert.deepEqual(catalog.operatingSystems, ["Windows", "macOS", "Linux"]);
        assert.equal(catalog.downloadUrls.length, 4);
        assert.ok(catalog.downloadUrls.every((url) => url.includes("/releases/download/v2.1.2/ALCOMD3_2.1.2_")));
    });
});

test("the current website exposes only canonical downloads for each release channel", () => {
    for (const catalog of siteConfig.downloadChannels) {
        if (catalog.releasePageOnly) {
            assert.equal(catalog.hasDownloads, false);
            assert.deepEqual(catalog.platforms, []);
            assert.deepEqual(catalog.downloadUrls, []);
            continue;
        }

        for (const platform of catalog.platforms) {
            for (const download of platform.downloads) {
                assert.equal(Boolean(download.url), platform.available);
            }
        }
    }
});
