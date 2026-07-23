import alcomd3Config from "../../../alcomd3.config.json";
import { fetchGitHubContributors } from "../_shared/github-contributors.js";

const responseHeaders = {
    "Access-Control-Allow-Origin": "*",
    "Cache-Control": "public, max-age=60, s-maxage=60, stale-while-revalidate=300",
    "Content-Type": "application/json; charset=utf-8",
    "X-Content-Type-Options": "nosniff",
};

export async function onRequestGet() {
    try {
        const contributors = await fetchGitHubContributors({
            repository: alcomd3Config.repository,
        });

        return new Response(
            JSON.stringify({
                schemaVersion: 1,
                repository: alcomd3Config.repository,
                contributors,
            }),
            {
                headers: responseHeaders,
            },
        );
    } catch (error) {
        console.error("Failed to proxy GitHub contributors.", error);

        return new Response(
            JSON.stringify({ error: "Contributor data is temporarily unavailable" }),
            {
                headers: {
                    ...responseHeaders,
                    "Cache-Control": "no-store",
                },
                status: 502,
            },
        );
    }
}
