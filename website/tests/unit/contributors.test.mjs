import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
    fetchContributors,
    normalizeContributor,
    validateSnapshot,
} from "../../../scripts/sync-contributors.mjs";
import {
    createGitHubContributorsUrl,
    fetchGitHubContributors,
    parseGitHubContributorFragment,
} from "../../functions/_shared/github-contributors.js";

const githubFragment = `
    <h2 class="h4">
        <a href="/ALCOMD3/ALCOMD3/graphs/contributors">
            Contributors <span title="2" class="Counter">2</span>
        </a>
    </h2>
    <ul class="list-style-none">
        <li class="mb-2 d-flex">
            <a href="https://github.com/CQMHV">
                <img
                    src="https://avatars.githubusercontent.com/u/185041488?s=64&amp;v=4"
                    alt="@CQMHV"
                />
            </a>
            <a href="https://github.com/CQMHV">
                <strong>CQMHV</strong>
                <span>才倾梦华V</span>
            </a>
        </li>
        <li class="mb-2 d-flex">
            <a href="https://github.com/apps/github-actions">
                <img
                    src="https://avatars.githubusercontent.com/in/15368?s=64&amp;v=4"
                    alt="@github-actions[bot]"
                />
            </a>
        </li>
    </ul>
`;
const expectedContributors = [
    {
        avatarUrl: "https://avatars.githubusercontent.com/u/185041488?s=64&v=4",
        name: "CQMHV",
        profileUrl: "https://github.com/CQMHV",
    },
    {
        avatarUrl: "https://avatars.githubusercontent.com/in/15368?s=64&v=4",
        name: "github-actions[bot]",
        profileUrl: "https://github.com/apps/github-actions",
    },
];

describe("contributor snapshot generation", () => {
    test("uses the exact contributor list and order rendered by GitHub", () => {
        assert.deepEqual(
            parseGitHubContributorFragment(githubFragment),
            expectedContributors,
        );
        assert.throws(
            () => parseGitHubContributorFragment("<p>not the GitHub fragment</p>"),
            /invalid contributor fragment/,
        );
        assert.throws(
            () => parseGitHubContributorFragment(
                githubFragment.replace(
                    /<li class="mb-2 d-flex">\s*<a href="https:\/\/github\.com\/apps\/github-actions">[\s\S]*?<\/li>/,
                    "",
                ),
            ),
            /incomplete contributor fragment/,
        );
    });

    test("build and runtime proxy fetch the GitHub repository fragment", async () => {
        const requestedUrls = [];
        const fetchImpl = async (input) => {
            requestedUrls.push(new URL(input).href);

            return new Response(githubFragment);
        };

        assert.deepEqual(
            await fetchContributors({
                fetchImpl,
                repository: "ALCOMD3/ALCOMD3",
                userAgent: "ALCOMD3-test",
            }),
            expectedContributors,
        );
        assert.deepEqual(
            await fetchGitHubContributors({
                fetchImpl,
                repository: "ALCOMD3/ALCOMD3",
            }),
            expectedContributors,
        );
        assert.deepEqual(requestedUrls, [
            "https://github.com/ALCOMD3/ALCOMD3/contributors_list?current_repository=ALCOMD3&deferred=true",
            "https://github.com/ALCOMD3/ALCOMD3/contributors_list?current_repository=ALCOMD3&deferred=true",
        ]);
    });

    test("derives GitHub URLs and validates snapshot data", () => {
        assert.equal(
            createGitHubContributorsUrl("ALCOMD3/ALCOMD3").href,
            "https://github.com/ALCOMD3/ALCOMD3/contributors_list?current_repository=ALCOMD3&deferred=true",
        );
        assert.throws(
            () => createGitHubContributorsUrl("invalid"),
            /repository is invalid/,
        );
        assert.equal(normalizeContributor({ name: "invalid" }), null);
        assert.throws(
            () => validateSnapshot(
                {
                    schemaVersion: 1,
                    repository: "upstream/ALCOM",
                    contributors: expectedContributors,
                },
                "ALCOMD3/ALCOMD3",
            ),
            /snapshot is invalid/,
        );
    });
});
