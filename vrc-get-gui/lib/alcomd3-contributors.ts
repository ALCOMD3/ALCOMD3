import alcomd3Config from "../../alcomd3.config.json";

const CONTRIBUTORS_API_URL = new URL(
	`/repos/${alcomd3Config.repository}/contributors`,
	"https://api.github.com",
);
CONTRIBUTORS_API_URL.searchParams.set("per_page", "100");

export type Alcomd3Contributor = {
	avatarUrl: string;
	name: string;
	profileUrl: string;
};

function isHttpsUrl(value: unknown): value is string {
	if (typeof value !== "string") {
		return false;
	}

	try {
		return new URL(value).protocol === "https:";
	} catch {
		return false;
	}
}

function normalizeContributor(value: unknown): Alcomd3Contributor | null {
	if (
		typeof value !== "object" ||
		value === null ||
		!("login" in value) ||
		!("avatar_url" in value) ||
		!("html_url" in value) ||
		typeof value.login !== "string" ||
		value.login.length === 0 ||
		!isHttpsUrl(value.avatar_url) ||
		!isHttpsUrl(value.html_url)
	) {
		return null;
	}

	return {
		avatarUrl: value.avatar_url,
		name: value.login,
		profileUrl: value.html_url,
	};
}

export async function fetchAlcomd3Contributors(
	fetchImpl: typeof fetch = fetch,
): Promise<Alcomd3Contributor[]> {
	const response = await fetchImpl(CONTRIBUTORS_API_URL, {
		headers: {
			Accept: "application/vnd.github+json",
		},
	});
	if (!response.ok) {
		throw new Error(`ALCOMD3 contributors request failed: ${response.status}`);
	}

	const responseData = (await response.json()) as unknown;
	if (!Array.isArray(responseData)) {
		throw new Error("ALCOMD3 contributors response is invalid");
	}

	return responseData
		.map(normalizeContributor)
		.filter(
			(contributor): contributor is Alcomd3Contributor => contributor !== null,
		);
}

export async function loadAlcomd3Contributors(
	fetchImpl: typeof fetch = fetch,
): Promise<Alcomd3Contributor[]> {
	try {
		return await fetchAlcomd3Contributors(fetchImpl);
	} catch (error) {
		console.warn("Failed to load ALCOMD3 contributors from GitHub.", error);
		return [];
	}
}
