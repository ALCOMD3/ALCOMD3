import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
    fetchGitHubContributors,
} from "../website/functions/_shared/github-contributors.js";

const workspaceRoot = fileURLToPath(new URL("../", import.meta.url));
const projectConfigPath = path.join(workspaceRoot, "alcomd3.config.json");
const snapshotPath = path.join(
    workspaceRoot,
    "generated",
    "alcomd3-contributors.json",
);

function isHttpsUrl(value) {
    if (typeof value !== "string") {
        return false;
    }

    try {
        return new URL(value).protocol === "https:";
    } catch {
        return false;
    }
}

export function normalizeContributor(value) {
    if (
        typeof value?.name !== "string"
        || value.name.length === 0
        || !isHttpsUrl(value.avatarUrl)
        || !isHttpsUrl(value.profileUrl)
    ) {
        return null;
    }

    return {
        avatarUrl: value.avatarUrl,
        name: value.name,
        profileUrl: value.profileUrl,
    };
}

export async function fetchContributors({
    fetchImpl = fetch,
    repository,
    userAgent,
}) {
    return fetchGitHubContributors({
        fetchImpl,
        repository,
        userAgent,
    });
}

export function validateSnapshot(value, repository) {
    if (
        value?.schemaVersion !== 1
        || value.repository !== repository
        || !Array.isArray(value.contributors)
        || value.contributors.length === 0
    ) {
        throw new Error("The bundled contributor snapshot is invalid");
    }

    const contributors = value.contributors.map((contributor) => {
        const normalized = normalizeContributor(contributor);
        if (!normalized) {
            throw new Error("The bundled contributor snapshot is invalid");
        }

        return normalized;
    });
    const profileUrls = new Set(
        contributors.map((contributor) => contributor.profileUrl),
    );
    if (profileUrls.size !== contributors.length) {
        throw new Error("The bundled contributor snapshot is invalid");
    }

    return {
        schemaVersion: 1,
        repository,
        contributors,
    };
}

async function readProjectConfig() {
    return JSON.parse(await readFile(projectConfigPath, "utf8"));
}

async function readExistingSnapshot(repository) {
    const snapshot = JSON.parse(await readFile(snapshotPath, "utf8"));

    return validateSnapshot(snapshot, repository);
}

async function writeSnapshot(snapshot) {
    const serialized = `${JSON.stringify(snapshot, null, 4)}\n`;
    let current = null;

    try {
        current = await readFile(snapshotPath, "utf8");
    } catch (error) {
        if (error?.code !== "ENOENT") {
            throw error;
        }
    }

    if (current === serialized) {
        console.log("ALCOMD3 contributor snapshot is already current.");
        return;
    }

    await mkdir(path.dirname(snapshotPath), { recursive: true });
    await writeFile(snapshotPath, serialized, "utf8");
    console.log("Updated the ALCOMD3 contributor snapshot from GitHub.");
}

export async function syncContributorSnapshot() {
    const config = await readProjectConfig();
    let contributors;

    try {
        contributors = await fetchContributors({
            repository: config.repository,
            userAgent: `${config.productName}-contributor-snapshot-builder`,
        });
    } catch (error) {
        await readExistingSnapshot(config.repository);
        console.warn(
            "Failed to refresh the ALCOMD3 contributor snapshot; using the existing snapshot.",
            error,
        );
        return;
    }

    await writeSnapshot({
        schemaVersion: 1,
        repository: config.repository,
        contributors,
    });
}

const entryPoint = process.argv[1]
    ? pathToFileURL(path.resolve(process.argv[1])).href
    : null;

if (entryPoint === import.meta.url) {
    await syncContributorSnapshot();
}
