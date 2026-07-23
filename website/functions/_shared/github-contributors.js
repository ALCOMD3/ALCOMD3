const GITHUB_ORIGIN = "https://github.com";

function decodeHtmlAttribute(value) {
    return value.replace(
        /&(?:#(\d+)|#x([0-9a-f]+)|([a-z]+));/gi,
        (entity, decimal, hexadecimal, named) => {
            if (decimal) {
                return String.fromCodePoint(Number.parseInt(decimal, 10));
            }
            if (hexadecimal) {
                return String.fromCodePoint(Number.parseInt(hexadecimal, 16));
            }

            switch (named.toLowerCase()) {
                case "amp":
                    return "&";
                case "apos":
                    return "'";
                case "gt":
                    return ">";
                case "lt":
                    return "<";
                case "quot":
                    return "\"";
                default:
                    return entity;
            }
        },
    );
}

function readAttribute(tag, name) {
    const pattern = new RegExp(
        `\\b${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)')`,
        "i",
    );
    const match = tag.match(pattern);

    return match ? decodeHtmlAttribute(match[1] ?? match[2]) : null;
}

function normalizeHttpsUrl(value) {
    if (!value) {
        return null;
    }

    try {
        const url = new URL(value);

        return url.protocol === "https:" ? url.href : null;
    } catch {
        return null;
    }
}

export function createGitHubContributorsUrl(repository) {
    if (!/^[^/]+\/[^/]+$/.test(repository)) {
        throw new Error("The configured GitHub repository is invalid");
    }

    const [owner, name] = repository.split("/");
    const url = new URL(
        `/${encodeURIComponent(owner)}/${encodeURIComponent(name)}/contributors_list`,
        GITHUB_ORIGIN,
    );
    url.searchParams.set("current_repository", name);
    url.searchParams.set("deferred", "true");

    return url;
}

export async function fetchGitHubContributors({
    fetchImpl = fetch,
    repository,
    userAgent,
}) {
    const headers = {
        Accept: "text/html",
    };
    if (userAgent) {
        headers["User-Agent"] = userAgent;
    }

    const response = await fetchImpl(
        createGitHubContributorsUrl(repository),
        { headers },
    );
    if (!response.ok) {
        throw new Error(
            `GitHub contributor fragment request failed: ${response.status}`,
        );
    }

    return parseGitHubContributorFragment(await response.text());
}

export function parseGitHubContributorFragment(html) {
    if (typeof html !== "string") {
        throw new Error("GitHub returned an invalid contributor fragment");
    }

    const headingMatch = html.match(
        /<h2\b[^>]*>[\s\S]*?\bContributors\b[\s\S]*?<\/h2>/i,
    );
    const listMatch = html.match(
        /<ul\b[^>]*class\s*=\s*(?:"[^"]*\blist-style-none\b[^"]*"|'[^']*\blist-style-none\b[^']*')[^>]*>([\s\S]*?)<\/ul>/i,
    );
    if (!headingMatch || !listMatch) {
        throw new Error("GitHub returned an invalid contributor fragment");
    }

    const contributors = [];
    const seenProfiles = new Set();

    for (const itemMatch of listMatch[1].matchAll(/<li\b[^>]*>([\s\S]*?)<\/li>/gi)) {
        const item = itemMatch[1];
        const profileTag = item.match(/<a\b[^>]*\bhref\s*=\s*(?:"[^"]*"|'[^']*')[^>]*>/i)?.[0];
        const imageTag = item.match(/<img\b[^>]*>/i)?.[0];
        const profileUrl = normalizeHttpsUrl(
            profileTag ? readAttribute(profileTag, "href") : null,
        );
        const avatarUrl = normalizeHttpsUrl(
            imageTag ? readAttribute(imageTag, "src") : null,
        );
        const imageAlt = imageTag ? readAttribute(imageTag, "alt") : null;
        const name = imageAlt?.startsWith("@") ? imageAlt.slice(1) : null;

        if (
            !name
            || !profileUrl
            || !avatarUrl
            || seenProfiles.has(profileUrl)
        ) {
            throw new Error("GitHub returned an invalid contributor entry");
        }

        seenProfiles.add(profileUrl);
        contributors.push({
            avatarUrl,
            name,
            profileUrl,
        });
    }

    if (contributors.length === 0) {
        throw new Error("GitHub returned no visible contributors");
    }

    const counterTag = [...headingMatch[0].matchAll(/<span\b[^>]*>/gi)]
        .map((match) => match[0])
        .find((tag) => readAttribute(tag, "class")?.split(/\s+/).includes("Counter"));
    const counterTitle = counterTag ? readAttribute(counterTag, "title") : null;
    const hasAdditionalContributorsLink = (
        /<a\b[^>]*\bhref\s*=\s*(?:"[^"]*\/graphs\/contributors"|'[^']*\/graphs\/contributors')[^>]*>\s*\+\s*[\d,]+\+?\s+contributors?\s*<\/a>/i
    ).test(html);
    if (
        counterTitle
        && /^\d+$/.test(counterTitle)
        && !hasAdditionalContributorsLink
        && Number.parseInt(counterTitle, 10) !== contributors.length
    ) {
        throw new Error("GitHub returned an incomplete contributor fragment");
    }

    return contributors;
}
