import { describe, expect, test, vi } from "vitest";
import {
	bundledAlcomd3Contributors,
	fetchAlcomd3Contributors,
	loadAlcomd3Contributors,
} from "@/lib/alcomd3-contributors";

describe("ALCOMD3 contributors", () => {
	test("loads the complete GitHub homepage list, including new PR contributors", async () => {
		const fetchMock = vi.fn(async (_input: RequestInfo | URL) => {
			return Response.json({
				schemaVersion: 1,
				repository: "ALCOMD3/ALCOMD3",
				contributors: [
					{
						avatarUrl: "https://avatars.example/cqmhv",
						name: "CQMHV",
						profileUrl: "https://github.com/CQMHV",
					},
					{
						avatarUrl: "https://avatars.example/pr-contributor",
						name: "pr-contributor",
						profileUrl: "https://github.com/pr-contributor",
					},
					{
						avatarUrl: "https://avatars.example/github-actions",
						name: "github-actions[bot]",
						profileUrl: "https://github.com/apps/github-actions",
					},
				],
			});
		});

		const contributors = await fetchAlcomd3Contributors(fetchMock);

		expect(contributors).toEqual([
			{
				avatarUrl: "https://avatars.example/cqmhv",
				name: "CQMHV",
				profileUrl: "https://github.com/CQMHV",
			},
			{
				avatarUrl: "https://avatars.example/pr-contributor",
				name: "pr-contributor",
				profileUrl: "https://github.com/pr-contributor",
			},
			{
				avatarUrl: "https://avatars.example/github-actions",
				name: "github-actions[bot]",
				profileUrl: "https://github.com/apps/github-actions",
			},
		]);
		expect(fetchMock).toHaveBeenCalledTimes(1);
		const requestUrl = new URL(String(fetchMock.mock.calls[0]?.[0]));
		expect(requestUrl.href).toBe("https://alcomd3.cqmhv.com/api/contributors");
	});

	test("rejects incomplete responses so callers can use the build snapshot", async () => {
		const fetchMock = vi.fn(async (_input: RequestInfo | URL) => {
			return Response.json({
				schemaVersion: 1,
				repository: "ALCOMD3/ALCOMD3",
				contributors: [
					{
						name: "missing-profile-data",
					},
				],
			});
		});

		await expect(fetchAlcomd3Contributors(fetchMock)).rejects.toThrow(
			"ALCOMD3 contributors response is invalid",
		);
	});

	test("uses the bundled build snapshot when the live refresh fails", async () => {
		const fetchMock = vi.fn(
			async (_input: RequestInfo | URL) => new Response(null, { status: 429 }),
		);
		const consoleWarn = vi
			.spyOn(console, "warn")
			.mockImplementation(() => undefined);

		await expect(loadAlcomd3Contributors(fetchMock)).resolves.toEqual(
			bundledAlcomd3Contributors,
		);
		expect(consoleWarn).toHaveBeenCalledOnce();

		consoleWarn.mockRestore();
	});
});
