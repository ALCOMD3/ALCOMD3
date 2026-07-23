import { readFile } from "node:fs/promises";
import { expect, test } from "@playwright/test";
import {
    defaultRouteLocale,
    supportedRouteLocales,
    supportedUiLocales,
} from "../../src/data/i18n.config.mjs";
import { siteConfig } from "../../src/data/site.config.mjs";

const projectConfig = JSON.parse(await readFile(
    new URL("../../../alcomd3.config.json", import.meta.url),
));
const stableUpdaterManifest = JSON.parse(await readFile(
    new URL("../../public/api/gui/tauri-updater.json", import.meta.url),
));
const betaUpdaterManifest = JSON.parse(await readFile(
    new URL("../../public/api/gui/tauri-updater-beta.json", import.meta.url),
));
const htmlLanguageByRoute = Object.fromEntries(
    supportedUiLocales.map((locale) => [locale.toLowerCase(), locale]),
);
const recommendationPatternByRoute = {
    "zh-cn": /推荐/,
    "zh-tw": /推薦/,
    "ja-jp": /おすすめ|推奨/,
    "en-us": /recommended/i,
};
const loopbackHostnames = new Set(["127.0.0.1", "localhost", "::1", "[::1]"]);
const externalRequestsByPage = new WeakMap();
const contributorApiUrl = new URL(siteConfig.contributorsApiUrl);
const contributorApiPattern = `${contributorApiUrl.origin}${contributorApiUrl.pathname}**`;
const mockedContributors = [
    {
        avatar_url: "https://avatars.example/cqmhv",
        html_url: "https://github.com/CQMHV",
        login: "CQMHV",
    },
];
const mockedContributorAvatarUrls = new Set(
    mockedContributors.map((contributor) => contributor.avatar_url),
);

function isExternalHttpRequest(requestUrl) {
    const url = new URL(requestUrl);

    return ["http:", "https:"].includes(url.protocol)
        && !loopbackHostnames.has(url.hostname);
}

function isMockedContributorRequest(requestUrl) {
    if (mockedContributorAvatarUrls.has(requestUrl)) {
        return true;
    }

    const url = new URL(requestUrl);

    return url.origin === contributorApiUrl.origin
        && url.pathname === contributorApiUrl.pathname;
}

async function mockContributorRequests(routingTarget) {
    await routingTarget.route(contributorApiPattern, async (route) => {
        await route.fulfill({
            contentType: "application/json",
            body: JSON.stringify(mockedContributors),
        });
    });
    for (const avatarUrl of mockedContributorAvatarUrls) {
        await routingTarget.route(avatarUrl, async (route) => {
            await route.fulfill({
                contentType: "image/svg+xml",
                body: "<svg xmlns='http://www.w3.org/2000/svg'/>",
            });
        });
    }
}

test("classifies IPv4 and IPv6 loopback requests as local", () => {
    for (const localUrl of [
        "http://127.0.0.1:4322/",
        "http://localhost:4322/",
        "http://[::1]:4322/",
    ]) {
        expect(isExternalHttpRequest(localUrl)).toBe(false);
    }
    expect(isExternalHttpRequest("https://example.com/")).toBe(true);
    expect(isExternalHttpRequest("data:text/plain,local")).toBe(false);
});

async function gotoLocalPage(page, path) {
    const response = await page.goto(path);

    expect(response?.ok(), `Expected ${path} to return a successful response`).toBe(true);
    await page.waitForLoadState("networkidle");
}

async function expectSafeBlankTarget(link) {
    const href = await link.getAttribute("href");
    const rel = (await link.getAttribute("rel"))?.split(/\s+/) ?? [];

    expect(href).toBeTruthy();
    expect(new URL(href).protocol).toBe("https:");
    expect(await link.getAttribute("target")).toBe("_blank");
    expect(rel).toEqual(expect.arrayContaining(["noopener", "noreferrer"]));
}

async function expectDownloadCatalog(page, catalog, { checkActionStyles = false } = {}) {
    const section = page.locator(`[data-download-channel-section="${catalog.channel}"]`);

    if (catalog.releasePageOnly) {
        await expect(section.locator("[data-download-platform-card]")).toHaveCount(0);
        await expect(section.locator("[data-download-channel-notice]")).toBeVisible();
        return;
    }

    await expect(section.locator("[data-download-platform-card]")).toHaveCount(catalog.platforms.length);
    for (const platform of catalog.platforms) {
        const card = section.locator(`[data-download-platform-card="${platform.key}"]`);
        const links = card.locator("[data-download-link]");
        const availableDownloads = platform.downloads.filter((download) => download.url);

        await expect(card).toBeVisible();
        if (platform.available) {
            await expect(links).toHaveCount(availableDownloads.length);
            for (const download of availableDownloads) {
                const link = card.locator(
                    `[data-download-link][data-download-action-label][href="${download.url}"]`,
                );
                await expect(link).toHaveCount(1);
                await expect(link).not.toHaveAttribute("aria-disabled", /.+/);
                if (checkActionStyles) {
                    await expect(link).toHaveClass(/\blarge\b/);
                    await expect(link).toHaveClass(/\bresponsive\b/);
                }
            }
            if (checkActionStyles) {
                const actionWidths = await links.evaluateAll((elements) => elements.map((element) => ({
                    button: element.getBoundingClientRect().width,
                    container: element.parentElement?.getBoundingClientRect().width ?? 0,
                })));
                for (const { button, container } of actionWidths) {
                    expect(Math.abs(button - container)).toBeLessThanOrEqual(1);
                }
            }
            await expect(card.locator("[data-download-unavailable]")).toHaveCount(0);
        } else {
            await expect(links).toHaveCount(0);
            await expect(card.locator("[data-download-unavailable]")).toBeVisible();
        }
    }
}

async function getLanguageRedirectConfig(request) {
    const response = await request.get("/");
    const html = await response.text();
    const configMatch = html.match(
        /<script[^>]+id="language-redirect-config"[^>]*>(?<config>.*?)<\/script>/s,
    );

    expect(response.ok()).toBe(true);
    expect(configMatch?.groups?.config).toBeTruthy();

    return JSON.parse(configMatch.groups.config);
}

test.beforeEach(async ({ page }) => {
    const externalRequests = [];

    await mockContributorRequests(page);

    externalRequestsByPage.set(page, externalRequests);
    page.on("request", (request) => {
        if (
            isExternalHttpRequest(request.url())
            && !isMockedContributorRequest(request.url())
        ) {
            externalRequests.push(request.url());
        }
    });
});

test.afterEach(async ({ page }) => {
    expect(externalRequestsByPage.get(page), "Website tests must not depend on external requests").toEqual([]);
});

for (const routeLocale of supportedRouteLocales) {
    test(`${routeLocale} home, download, and MCP routes expose localized semantics`, async ({ page }) => {
        await gotoLocalPage(page, `/${routeLocale}/`);

        await expect(page.locator("html")).toHaveAttribute("lang", htmlLanguageByRoute[routeLocale]);
        await expect(page.getByRole("heading", { level: 1 })).toHaveText("ALCOMD3");
        await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
            "href",
            new URL(`${routeLocale}/`, siteConfig.url).href,
        );
        await expect(page.locator('link[rel="alternate"]')).toHaveCount(supportedRouteLocales.length + 1);
        await expect(page.locator("#download-button")).toHaveAttribute(
            "href",
            `/${routeLocale}/${siteConfig.downloadPath}/`,
        );
        await expect(page.locator("[data-download-channel-section]")).toHaveCount(0);
        const contributorsSection = page.locator("[data-contributors-section]");
        await expect(contributorsSection).toHaveAttribute("data-contributors-live", "");
        await expect(contributorsSection.locator("[data-contributor-link]")).toHaveCount(
            mockedContributors.length,
        );
        await expect(
            contributorsSection.getByRole("link", { name: mockedContributors[0].login }),
        ).toHaveAttribute("href", mockedContributors[0].html_url);

        await gotoLocalPage(page, `/${routeLocale}/${siteConfig.downloadPath}/`);

        await expect(page.locator("html")).toHaveAttribute("lang", htmlLanguageByRoute[routeLocale]);
        await expect(page.getByRole("heading", { level: 1 })).toContainText("ALCOMD3");
        await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
            "href",
            new URL(`${routeLocale}/${siteConfig.downloadPath}/`, siteConfig.url).href,
        );
        await expect(page.locator('link[rel="alternate"]')).toHaveCount(supportedRouteLocales.length + 1);
        await expect(page.locator('[data-download-channel-section="stable"]')).toHaveCount(1);
        await expect(page.locator('[data-download-channel-section="beta"]')).toHaveCount(1);
        await expect(page.locator("body")).not.toContainText(recommendationPatternByRoute[routeLocale]);
        await expect(page.locator(
            "[data-recommended], [data-recommended-channel], [data-download-recommended-badge], "
            + "#download-recommended-release, #download-recommendation-status",
        )).toHaveCount(0);
        const platformMatch = page.locator("[data-download-platform-match]");
        await expect(platformMatch).toHaveCount(1);
        await expect(platformMatch).toHaveAttribute("data-download-channel", "stable");
        await expect(platformMatch).toHaveAttribute("aria-current", "true");
        await expect(page.locator(
            '[data-download-channel="beta"][data-download-platform-match]',
        )).toHaveCount(0);
        for (const catalog of siteConfig.downloadChannels) {
            await expect(page.locator(
                `[data-download-channel="${catalog.channel}"][data-download-platform-card]`,
            )).toHaveCount(catalog.platforms.length);
        }
        await expect(page.locator("[data-macos-installation-notice]")).toHaveCount(0);

        await gotoLocalPage(page, `/${routeLocale}/mcp/`);

        await expect(page.locator("html")).toHaveAttribute("lang", htmlLanguageByRoute[routeLocale]);
        await expect(page.locator("article.docs-content")).toBeVisible();
        await expect(page.getByRole("heading", { level: 1 })).toContainText("MCP");
        await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
            "href",
            new URL(`${routeLocale}/mcp/`, siteConfig.url).href,
        );
    });
}

test("home hides contributors when the GitHub request fails", async ({ page }) => {
    await page.unroute(contributorApiPattern);
    await page.route(contributorApiPattern, async (route) => {
        await route.fulfill({ status: 429 });
    });

    await gotoLocalPage(page, `/${defaultRouteLocale}/`);

    const contributorsSection = page.locator("[data-contributors-section]");
    await expect(contributorsSection).toBeHidden();
    await expect(contributorsSection.locator("[data-contributor-link]")).toHaveCount(0);
});

test("root route redirects from the browser locale", async ({ browser }) => {
    const context = await browser.newContext({ locale: "ja-JP" });
    const page = await context.newPage();
    const externalRequests = [];

    await mockContributorRequests(context);
    page.on("request", (request) => {
        if (
            isExternalHttpRequest(request.url())
            && !isMockedContributorRequest(request.url())
        ) {
            externalRequests.push(request.url());
        }
    });

    await page.goto("/");
    await expect(page).toHaveURL(/\/ja-jp\/$/);
    await expect(page.locator("html")).toHaveAttribute("lang", "ja-JP");
    expect(externalRequests).toEqual([]);

    await context.close();
});

test("stored locale overrides browser locale on the root route", async ({ page, request }) => {
    const { uiLocaleStorageKey } = await getLanguageRedirectConfig(request);

    await page.addInitScript(({ storageKey }) => {
        localStorage.setItem(storageKey, "zh-CN");
    }, { storageKey: uiLocaleStorageKey });

    await page.goto("/");
    await expect(page).toHaveURL(/\/zh-cn\/$/);
    await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
});

for (const nestedPagePath of [siteConfig.downloadPath, siteConfig.mcpDocsPath]) {
    test(`language navigation preserves the ${nestedPagePath} page and stores the selection`, async ({ page, request }) => {
        const { uiLocaleStorageKey } = await getLanguageRedirectConfig(request);

        await gotoLocalPage(page, `/en-us/${nestedPagePath}/`);

        await page.getByRole("button", { name: "Language" }).click();
        await page.getByRole("link", { name: "日本語" }).click();

        await expect(page).toHaveURL(new RegExp(`/ja-jp/${nestedPagePath}/$`));
        await expect(page.locator("html")).toHaveAttribute("lang", "ja-JP");
        await expect.poll(() => page.evaluate(
            (storageKey) => localStorage.getItem(storageKey),
            uiLocaleStorageKey,
        )).toBe("ja-JP");
    });
}

test("theme navigation applies and persists the selected mode", async ({ page, request }) => {
    const { themeModeStorageKey } = await getLanguageRedirectConfig(request);

    await gotoLocalPage(page, `/${defaultRouteLocale}/`);

    await page.getByRole("button", { name: "Appearance" }).click();
    await page.locator('[data-theme-mode-value="dark"]').click();

    await expect(page.locator("html")).toHaveAttribute("data-theme-mode", "dark");
    await expect(page.locator("body")).toHaveClass(/\bdark\b/);
    await expect.poll(() => page.evaluate(
        (storageKey) => localStorage.getItem(storageKey),
        themeModeStorageKey,
    )).toBe("dark");
});

test("download page and public links resolve without external page requests", async ({ page }) => {
    await gotoLocalPage(page, `/${defaultRouteLocale}/`);

    const downloadButton = page.locator("#download-button");
    const stableRelease = siteConfig.stableRelease;
    await expect(downloadButton).toHaveAttribute(
        "href",
        `/${defaultRouteLocale}/${siteConfig.downloadPath}/`,
    );
    await expect(downloadButton).not.toHaveAttribute("aria-disabled", /.+/);
    await expect(page.locator("#download-note")).toContainText(stableRelease.version);
    await expect(page.locator("[data-download-channel-section]")).toHaveCount(0);

    const repositoryLink = page.getByRole("link", { name: "Visit GitHub" });
    await expect(repositoryLink).toHaveAttribute("href", siteConfig.repositoryUrl);
    await expectSafeBlankTarget(repositoryLink);
    await expectSafeBlankTarget(page.getByRole("link", { name: "VRChatAvatarLearn" }));
    await expectSafeBlankTarget(page.getByRole("link", { name: "BOOTH" }));
    await expectSafeBlankTarget(page.getByRole("link", { name: "Gumroad" }));

    await gotoLocalPage(page, `/${defaultRouteLocale}/${siteConfig.downloadPath}/`);

    for (const catalog of siteConfig.downloadChannels) {
        const section = page.locator(`[data-download-channel-section="${catalog.channel}"]`);
        const releaseLink = page.locator(`[data-download-release-link="${catalog.channel}"]`);

        await expect(section).toBeVisible();
        await expect(section).toHaveAttribute("data-download-channel-version", catalog.version);
        await expect(releaseLink).toHaveAttribute("href", catalog.releaseUrl);
        await expectSafeBlankTarget(releaseLink);
        await expectDownloadCatalog(page, catalog, { checkActionStyles: true });
    }

    const structuredData = JSON.parse(
        await page.locator('script[type="application/ld+json"]').textContent(),
    );
    expect(structuredData[0].softwareVersion).toBe(stableRelease.version);
    if (stableRelease.operatingSystems.length > 0) {
        expect(structuredData[0].operatingSystem).toEqual(stableRelease.operatingSystems);
    } else {
        expect(structuredData[0]).not.toHaveProperty("operatingSystem");
    }
    if (stableRelease.downloadUrls.length > 0) {
        expect(structuredData[0].downloadUrl).toEqual(stableRelease.downloadUrls);
    } else {
        expect(structuredData[0]).not.toHaveProperty("downloadUrl");
    }

    await expectSafeBlankTarget(page.getByRole("link", { name: "Visit GitHub" }));
    await expectSafeBlankTarget(page.getByRole("link", { name: "VRChatAvatarLearn" }));
});

test("updater manifests are served unchanged with configured platform metadata", async ({ request }) => {
    const manifests = [
        ["stable", projectConfig.updaterManifests.stable.publicPath, stableUpdaterManifest],
        ["beta", projectConfig.updaterManifests.beta.publicPath, betaUpdaterManifest],
    ];

    for (const [channel, path, expectedManifest] of manifests) {
        const response = await request.get(path);
        const manifest = await response.json();
        expect(response.ok()).toBe(true);
        expect(response.headers()["content-type"]).toContain("application/json");
        expect(manifest).toEqual(expectedManifest);
        expect(manifest.version).toMatch(/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/);
        expect(Date.parse(manifest.pub_date)).not.toBeNaN();
        expect(Object.keys(manifest.platforms).length).toBeGreaterThan(0);

        for (const [platformKey, platform] of Object.entries(manifest.platforms)) {
            const platformConfig = projectConfig.releasePlatforms[platformKey];

            expect(platformConfig, `${platformKey} must exist in releasePlatforms`).toBeTruthy();
            const canonicalUpdaterUrl = (
                `https://github.com/${projectConfig.repository}/releases/download/v${manifest.version}/`
                + platformConfig.updater.assetPattern.replace("{version}", manifest.version)
            );

            expect(platform.signature).toBeTruthy();
            expect(platform.args ?? []).toEqual(platformConfig.updater.args);
            const catalog = siteConfig.downloadChannels.find((candidate) => candidate.channel === channel);
            if (channel === "beta" || !catalog?.releasePageOnly) {
                expect([
                    canonicalUpdaterUrl,
                    new URL("./", canonicalUpdaterUrl).href,
                ]).toContain(
                    platform.url === canonicalUpdaterUrl
                        ? platform.url
                        : new URL("./", platform.url).href,
                );
            }
        }
    }

    for (const [channel, manifest] of [
        ["stable", stableUpdaterManifest],
        ["beta", betaUpdaterManifest],
    ]) {
        const contractSatisfied = Object.entries(manifest.platforms).every(([platformKey, platform]) => {
            const platformConfig = projectConfig.releasePlatforms[platformKey];
            const canonicalUpdaterUrl = (
                `https://github.com/${projectConfig.repository}/releases/download/v${manifest.version}/`
                + platformConfig.updater.assetPattern.replace("{version}", manifest.version)
            );

            return platform.url === canonicalUpdaterUrl;
        });
        const catalog = siteConfig.downloadChannels.find((candidate) => candidate.channel === channel);

        if (channel === "stable" && !contractSatisfied) {
            expect(catalog.releasePageOnly).toBe(true);
            expect(catalog.platforms).toEqual([]);
            continue;
        }
        for (const platform of catalog.platforms) {
            expect(platform.available).toBe(
                contractSatisfied && Boolean(manifest.platforms[platform.key]),
            );
        }
    }
});

for (const platformCase of [
    {
        name: "Windows",
        userAgentDataPlatform: "Windows",
        navigatorPlatform: "Win32",
        userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
        expectedPlatform: "windows-x86_64",
    },
    {
        name: "macOS",
        userAgentDataPlatform: "macOS",
        navigatorPlatform: "MacIntel",
        userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        expectedPlatform: "darwin-aarch64",
    },
    {
        name: "Linux",
        userAgentDataPlatform: "Linux",
        navigatorPlatform: "Linux x86_64",
        userAgent: "Mozilla/5.0 (X11; Linux x86_64)",
        expectedPlatform: "linux-x86_64",
    },
    {
        name: "Android",
        userAgentDataPlatform: "Android",
        navigatorPlatform: "Linux armv8l",
        userAgent: "Mozilla/5.0 (Linux; Android 15)",
        expectedPlatform: null,
    },
]) {
    test(`download page emphasizes only the matching stable card on ${platformCase.name}`, async ({ page }) => {
        await page.addInitScript((profile) => {
            Object.defineProperties(navigator, {
                maxTouchPoints: {
                    configurable: true,
                    get: () => 0,
                },
                platform: {
                    configurable: true,
                    get: () => profile.navigatorPlatform,
                },
                userAgent: {
                    configurable: true,
                    get: () => profile.userAgent,
                },
                userAgentData: {
                    configurable: true,
                    get: () => ({ platform: profile.userAgentDataPlatform }),
                },
            });
        }, platformCase);

        await gotoLocalPage(page, `/${defaultRouteLocale}/${siteConfig.downloadPath}/`);

        const platformMatch = page.locator("[data-download-platform-match]");
        if (!platformCase.expectedPlatform) {
            await expect(platformMatch).toHaveCount(0);
            return;
        }

        await expect(platformMatch).toHaveCount(1);
        await expect(platformMatch).toHaveAttribute("data-download-channel", "stable");
        await expect(platformMatch).toHaveAttribute(
            "data-download-platform-card",
            platformCase.expectedPlatform,
        );
        await expect(platformMatch).toHaveAttribute("aria-current", "true");
        await expect(platformMatch).toHaveCSS("outline-style", "solid");
        await expect(page.locator(
            '[data-download-channel="beta"][data-download-platform-match]',
        )).toHaveCount(0);
    });
}

test("server-rendered downloads remain usable without JavaScript", async ({ browser }) => {
    const context = await browser.newContext({
        javaScriptEnabled: false,
        locale: "en-US",
    });
    await mockContributorRequests(context);
    const page = await context.newPage();
    const homepageResponse = await page.goto(`/${defaultRouteLocale}/`);

    expect(homepageResponse?.ok()).toBe(true);
    await expect(page.locator("[data-contributors-section]")).toHaveCount(1);
    await expect(page.locator("[data-contributors-section]")).toBeHidden();
    await expect(page.locator("[data-contributor-link]")).toHaveCount(0);
    await expect(page.locator("#download-button")).toHaveAttribute(
        "href",
        `/${defaultRouteLocale}/${siteConfig.downloadPath}/`,
    );
    await expect(page.locator("#download-button")).not.toHaveAttribute("aria-disabled", /.+/);
    await expect(page.locator("[data-download-channel-section]")).toHaveCount(0);

    const downloadResponse = await page.goto(`/${defaultRouteLocale}/${siteConfig.downloadPath}/`);

    expect(downloadResponse?.ok()).toBe(true);
    await expect(page.locator('[data-download-link][href="#"]')).toHaveCount(0);
    await expect(page.locator("[data-download-platform-match]")).toHaveCount(0);
    await expect(page.locator('[data-download-release-link="stable"]'))
        .toHaveAttribute("href", siteConfig.stableRelease.releaseUrl);
    await expect(page.locator("[data-macos-installation-notice]")).toHaveCount(0);

    for (const catalog of siteConfig.downloadChannels) {
        await expectDownloadCatalog(page, catalog);
    }

    await context.close();
});

test("download actions can expand for long localized labels on narrow screens", async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 844 });
    await gotoLocalPage(page, `/${defaultRouteLocale}/${siteConfig.downloadPath}/`);

    const action = page.locator("[data-download-link]").first();
    const label = action.locator("span");
    await label.evaluate((element) => {
        element.textContent = "Localized download format label ".repeat(4).trim();
    });

    const metrics = await action.evaluate((element) => {
        const style = getComputedStyle(element);

        return {
            blockSize: element.getBoundingClientRect().height,
            clientWidth: element.clientWidth,
            scrollWidth: element.scrollWidth,
            whiteSpace: style.whiteSpace,
        };
    });

    expect(metrics.blockSize).toBeGreaterThan(48);
    expect(metrics.scrollWidth).toBeLessThanOrEqual(metrics.clientWidth);
    expect(metrics.whiteSpace).toBe("normal");
});

test("download cards match the home MCP card width on wide screens", async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1080 });
    await gotoLocalPage(page, `/${defaultRouteLocale}/`);

    const homeMcpCardWidth = await page
        .locator('[aria-labelledby="mcp-section-title"] > article')
        .evaluate((element) => element.getBoundingClientRect().width);

    await gotoLocalPage(page, `/${defaultRouteLocale}/${siteConfig.downloadPath}/`);

    const metrics = await page.locator('[data-download-channel-section="stable"]').evaluate((element) => ({
        cardWidth: element.getBoundingClientRect().width,
        pageWidth: document.documentElement.scrollWidth,
        viewportWidth: document.documentElement.clientWidth,
    }));

    expect(metrics.cardWidth).toBeCloseTo(homeMcpCardWidth, 0);
    expect(metrics.pageWidth).toBeLessThanOrEqual(metrics.viewportWidth);
});

test("manifest, robots, sitemap, and service worker endpoints stay coherent", async ({ request }) => {
    const webManifestResponse = await request.get("/site.webmanifest");
    const webManifest = await webManifestResponse.json();

    expect(webManifestResponse.ok()).toBe(true);
    expect(webManifestResponse.headers()["content-type"]).toContain("application/manifest+json");
    expect(webManifest).toMatchObject({
        name: siteConfig.name,
        short_name: siteConfig.shortName,
        start_url: "/",
        scope: "/",
        display: "standalone",
    });
    expect(webManifest.icons).toEqual(expect.arrayContaining([
        expect.objectContaining({ src: siteConfig.pwaIcon192Path, sizes: "192x192" }),
        expect.objectContaining({ src: siteConfig.pwaIcon512Path, sizes: "512x512" }),
    ]));

    const robotsResponse = await request.get("/robots.txt");
    const robots = await robotsResponse.text();
    expect(robotsResponse.ok()).toBe(true);
    expect(robots).toContain("User-agent: *");
    expect(robots).toContain(`Sitemap: ${new URL("/sitemap-index.xml", siteConfig.url).href}`);

    const sitemapIndexResponse = await request.get("/sitemap-index.xml");
    const sitemapIndex = await sitemapIndexResponse.text();
    const sitemapResponse = await request.get("/sitemap-0.xml");
    const sitemap = await sitemapResponse.text();
    expect(sitemapIndexResponse.ok()).toBe(true);
    expect(sitemapIndex).toContain(new URL("/sitemap-0.xml", siteConfig.url).href);
    expect(sitemapResponse.ok()).toBe(true);
    for (const routeLocale of supportedRouteLocales) {
        expect(sitemap).toContain(new URL(`${routeLocale}/`, siteConfig.url).href);
        expect(sitemap).toContain(new URL(`${routeLocale}/${siteConfig.downloadPath}/`, siteConfig.url).href);
        expect(sitemap).toContain(new URL(`${routeLocale}/mcp/`, siteConfig.url).href);
    }
    expect(sitemap).toContain('hreflang="x-default"');

    const serviceWorkerResponse = await request.get("/sw.js");
    const serviceWorker = await serviceWorkerResponse.text();
    expect(serviceWorkerResponse.ok()).toBe(true);
    for (const routeLocale of supportedRouteLocales) {
        expect(serviceWorker).toContain(`"/${routeLocale}/"`);
    }
    expect(serviceWorker).toContain('url.pathname.startsWith("/api/")');
});

test("home and download pages keep basic accessible semantics on desktop and mobile", async ({ page }) => {
    for (const path of [
        `/${defaultRouteLocale}/`,
        `/${defaultRouteLocale}/${siteConfig.downloadPath}/`,
    ]) {
        await gotoLocalPage(page, path);

        await expect(page.locator("main")).toBeVisible();
        await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
        await expect(page.locator("img:not([alt])")).toHaveCount(0);

        const blankTargetLinks = page.locator('a[target="_blank"]');
        for (let index = 0; index < await blankTargetLinks.count(); index += 1) {
            await expectSafeBlankTarget(blankTargetLinks.nth(index));
        }

        await page.setViewportSize({ width: 390, height: 844 });
        await gotoLocalPage(page, path);

        const viewportMetrics = await page.evaluate(() => ({
            clientWidth: document.documentElement.clientWidth,
            scrollWidth: document.documentElement.scrollWidth,
        }));
        expect(viewportMetrics.scrollWidth).toBeLessThanOrEqual(viewportMetrics.clientWidth);
    }
});
