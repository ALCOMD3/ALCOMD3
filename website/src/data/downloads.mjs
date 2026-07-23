const SEMVER_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
const REPOSITORY_PATTERN = /^[0-9A-Za-z_.-]+\/[0-9A-Za-z_.-]+$/;

function validateSemVer(version) {
    const match = SEMVER_PATTERN.exec(version);

    if (!match) {
        throw new Error(`invalid semantic version: ${version}`);
    }

    const prerelease = match[4]?.split(".") ?? [];
    if (prerelease.some((identifier) => /^\d+$/.test(identifier) && identifier.length > 1 && identifier.startsWith("0"))) {
        throw new Error(`invalid semantic version prerelease: ${version}`);
    }
}

function expandAssetPattern(pattern, version, fieldName) {
    if (typeof pattern !== "string" || !pattern.includes("{version}")) {
        throw new Error(`${fieldName} must contain {version}`);
    }

    const fileName = pattern.replaceAll("{version}", version);

    if (
        fileName.includes("{")
        || fileName.includes("}")
        || fileName.includes("/")
        || fileName.includes("\\")
        || fileName === "."
        || fileName === ".."
    ) {
        throw new Error(`${fieldName} produced an invalid release asset name`);
    }

    return fileName;
}

function getOperatingSystem(target, platformKey) {
    if (target.includes("windows")) {
        return "Windows";
    }
    if (target.includes("apple-darwin")) {
        return "macOS";
    }
    if (target.includes("linux")) {
        return "Linux";
    }

    throw new Error(`releasePlatforms.${platformKey}.target does not identify a supported operating system`);
}

function validateMacosAdHocSigning(platformConfig, platformKey) {
    const isMacos = platformConfig.target.includes("apple-darwin");
    const signing = platformConfig.macosAdHocSigning;

    if (!isMacos) {
        if (signing !== undefined) {
            throw new Error(`releasePlatforms.${platformKey}.macosAdHocSigning is only valid for macOS`);
        }
        return;
    }
    if (!signing || typeof signing !== "object" || Array.isArray(signing)) {
        throw new Error(`releasePlatforms.${platformKey}.macosAdHocSigning must be configured`);
    }
    const unsupportedSigningFields = Object.keys(signing);
    if (unsupportedSigningFields.length > 0) {
        throw new Error(
            `releasePlatforms.${platformKey}.macosAdHocSigning contains unsupported fields: ${unsupportedSigningFields.join(", ")}`,
        );
    }
}

function createExpectedReleaseAssetUrl(repository, version, fileName) {
    return new URL(
        `${encodeURIComponent(fileName)}`,
        `https://github.com/${repository}/releases/download/v${version}/`,
    ).href;
}

export function createReleaseSummary(config, manifest, channel) {
    const version = manifest?.version;
    const repository = config?.repository;

    if (typeof version !== "string") {
        throw new Error("updater manifest has an invalid version");
    }
    validateSemVer(version);
    if (channel !== "stable" && channel !== "beta") {
        throw new Error(`unsupported download channel: ${channel}`);
    }
    if (typeof repository !== "string" || !REPOSITORY_PATTERN.test(repository)) {
        throw new Error("repository must use OWNER/REPOSITORY format");
    }

    return {
        channel,
        version,
        releaseUrl: `https://github.com/${repository}/releases/tag/v${version}`,
        platforms: [],
        operatingSystems: [],
        downloadUrls: [],
        hasDownloads: false,
        releasePageOnly: true,
    };
}

export function createStableDownloadCatalog(config, manifest) {
    const catalog = createDownloadCatalog(config, manifest, {
        allowEmpty: true,
        channel: "stable",
        unavailableOnContractMismatch: true,
    });

    return catalog.hasDownloads
        ? catalog
        : createReleaseSummary(config, manifest, "stable");
}

function validateUpdaterUrl(
    repository,
    version,
    platformKey,
    platformConfig,
    manifestPlatform,
    unavailableOnContractMismatch,
) {
    if (!manifestPlatform || typeof manifestPlatform.url !== "string") {
        throw new Error(`updater manifest platform ${platformKey} has no URL`);
    }

    const canonicalUpdaterAssetName = expandAssetPattern(
        platformConfig.updater?.assetPattern,
        version,
        `releasePlatforms.${platformKey}.updater.assetPattern`,
    );
    const actualUrl = new URL(manifestPlatform.url).href;
    const canonicalUrl = createExpectedReleaseAssetUrl(repository, version, canonicalUpdaterAssetName);

    if (actualUrl === canonicalUrl) {
        return new URL("./", actualUrl);
    }

    const expectedReleaseDirectory = new URL("./", canonicalUrl);
    const actualReleaseDirectory = new URL("./", actualUrl);
    if (unavailableOnContractMismatch && actualReleaseDirectory.href === expectedReleaseDirectory.href) {
        return null;
    }

    throw new Error(`updater manifest platform ${platformKey} URL does not match the canonical release asset`);
}

export function createDownloadCatalog(config, manifest, options = {}) {
    const {
        allowEmpty = false,
        channel = "beta",
        unavailableOnContractMismatch = false,
    } = options;
    const version = manifest?.version;
    const repository = config?.repository;
    const releasePlatforms = config?.releasePlatforms;

    if (typeof version !== "string") {
        throw new Error("updater manifest has an invalid version");
    }
    validateSemVer(version);
    if (channel !== "stable" && channel !== "beta") {
        throw new Error(`unsupported download channel: ${channel}`);
    }
    if (typeof repository !== "string" || !REPOSITORY_PATTERN.test(repository)) {
        throw new Error("repository must use OWNER/REPOSITORY format");
    }
    if (!releasePlatforms || typeof releasePlatforms !== "object" || Array.isArray(releasePlatforms)) {
        throw new Error("releasePlatforms must be configured");
    }
    if (!manifest.platforms || typeof manifest.platforms !== "object" || Array.isArray(manifest.platforms)) {
        throw new Error("updater manifest has no platforms object");
    }

    for (const platformKey of Object.keys(manifest.platforms)) {
        if (!releasePlatforms[platformKey]) {
            throw new Error(`updater manifest contains unconfigured platform ${platformKey}`);
        }
    }

    const releaseDirectoryUrls = new Map();
    let hasContractMismatch = false;
    for (const [platformKey, manifestPlatform] of Object.entries(manifest.platforms)) {
        const releaseDirectoryUrl = validateUpdaterUrl(
            repository,
            version,
            platformKey,
            releasePlatforms[platformKey],
            manifestPlatform,
            unavailableOnContractMismatch,
        );

        releaseDirectoryUrls.set(platformKey, releaseDirectoryUrl);
        hasContractMismatch ||= releaseDirectoryUrl === null;
    }
    const downloadsUnavailable = unavailableOnContractMismatch && hasContractMismatch;

    const seenDownloadIds = new Set();
    const platforms = Object.entries(releasePlatforms).map(([platformKey, platformConfig]) => {
        if (!platformConfig || typeof platformConfig !== "object") {
            throw new Error(`releasePlatforms.${platformKey} must be an object`);
        }
        if (typeof platformConfig.target !== "string" || platformConfig.target.length === 0) {
            throw new Error(`releasePlatforms.${platformKey}.target must be configured`);
        }
        if (!Array.isArray(platformConfig.downloads) || platformConfig.downloads.length === 0) {
            throw new Error(`releasePlatforms.${platformKey}.downloads must not be empty`);
        }

        const releaseDirectoryUrl = downloadsUnavailable
            ? null
            : releaseDirectoryUrls.get(platformKey) ?? null;
        const downloads = platformConfig.downloads.map((download, index) => {
            const fieldName = `releasePlatforms.${platformKey}.downloads[${index}]`;

            if (!download || typeof download !== "object") {
                throw new Error(`${fieldName} must be an object`);
            }
            if (typeof download.id !== "string" || download.id.length === 0) {
                throw new Error(`${fieldName}.id must be configured`);
            }
            if (typeof download.format !== "string" || download.format.length === 0) {
                throw new Error(`${fieldName}.format must be configured`);
            }
            if (seenDownloadIds.has(download.id)) {
                throw new Error(`release download id ${download.id} is duplicated`);
            }
            seenDownloadIds.add(download.id);

            const assetName = expandAssetPattern(download.assetPattern, version, `${fieldName}.assetPattern`);

            return {
                id: download.id,
                format: download.format,
                assetName,
                primary: download.primary === true,
                url: releaseDirectoryUrl
                    ? new URL(encodeURIComponent(assetName), releaseDirectoryUrl).href
                    : null,
            };
        });

        const available = downloads.some((download) => download.url);
        if (available && downloads.filter((download) => download.primary && download.url).length !== 1) {
            throw new Error(`available platform ${platformKey} must have exactly one primary download`);
        }
        validateMacosAdHocSigning(platformConfig, platformKey);

        return {
            key: platformKey,
            target: platformConfig.target,
            operatingSystem: getOperatingSystem(platformConfig.target, platformKey),
            available,
            downloads,
        };
    });
    const availablePlatforms = platforms.filter((platform) => platform.available);

    if (availablePlatforms.length === 0 && !allowEmpty) {
        throw new Error("updater manifest has no configured release platform");
    }

    return {
        channel,
        version,
        releaseUrl: `https://github.com/${repository}/releases/tag/v${version}`,
        platforms,
        operatingSystems: [...new Set(availablePlatforms.map((platform) => platform.operatingSystem))],
        downloadUrls: availablePlatforms.flatMap((platform) => (
            platform.downloads.flatMap((download) => download.url ? [download.url] : [])
        )),
        hasDownloads: availablePlatforms.length > 0,
        releasePageOnly: false,
    };
}
