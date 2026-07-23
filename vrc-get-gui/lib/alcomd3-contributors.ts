import alcomd3Config from "../../alcomd3.config.json";
import contributorSnapshot from "../../generated/alcomd3-contributors.json";

const CONTRIBUTORS_API_URL = new URL(
	"/api/contributors",
	alcomd3Config.homepageUrl,
);

export type Alcomd3Contributor = {
	avatarUrl: string;
	name: string;
	profileUrl: string;
};

type ContributorsResponse = {
	contributors?: unknown;
	repository?: unknown;
	schemaVersion?: unknown;
};

export const bundledAlcomd3Contributors: Alcomd3Contributor[] =
	contributorSnapshot.contributors;

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
		!("name" in value) ||
		!("avatarUrl" in value) ||
		!("profileUrl" in value) ||
		typeof value.name !== "string" ||
		value.name.length === 0 ||
		!isHttpsUrl(value.avatarUrl) ||
		!isHttpsUrl(value.profileUrl)
	) {
		return null;
	}

	return {
		avatarUrl: value.avatarUrl,
		name: value.name,
		profileUrl: value.profileUrl,
	};
}

export async function fetchAlcomd3Contributors(
	fetchImpl: typeof fetch = fetch,
): Promise<Alcomd3Contributor[]> {
	const response = await fetchImpl(CONTRIBUTORS_API_URL, {
		headers: {
			Accept: "application/json",
		},
	});
	if (!response.ok) {
		throw new Error(`ALCOMD3 contributors request failed: ${response.status}`);
	}

	const responseData = (await response.json()) as ContributorsResponse;
	if (
		responseData.schemaVersion !== 1 ||
		responseData.repository !== alcomd3Config.repository ||
		!Array.isArray(responseData.contributors)
	) {
		throw new Error("ALCOMD3 contributors response is invalid");
	}

	const contributors = responseData.contributors.map(normalizeContributor);
	if (
		contributors.length === 0 ||
		contributors.some((contributor) => contributor === null)
	) {
		throw new Error("ALCOMD3 contributors response is invalid");
	}

	const validContributors = contributors as Alcomd3Contributor[];
	const profileUrls = new Set(
		validContributors.map((contributor) => contributor.profileUrl),
	);
	if (profileUrls.size !== validContributors.length) {
		throw new Error("ALCOMD3 contributors response is invalid");
	}

	return validContributors;
}

export async function loadAlcomd3Contributors(
	fetchImpl: typeof fetch = fetch,
): Promise<Alcomd3Contributor[]> {
	try {
		return await fetchAlcomd3Contributors(fetchImpl);
	} catch (error) {
		console.warn(
			"Failed to refresh ALCOMD3 contributors; using the bundled build snapshot.",
			error,
		);
		return bundledAlcomd3Contributors;
	}
}
