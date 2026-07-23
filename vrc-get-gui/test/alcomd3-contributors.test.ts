import { describe, expect, test, vi } from "vitest";
import {
	fetchAlcomd3Contributors,
	loadAlcomd3Contributors,
} from "@/lib/alcomd3-contributors";

describe("ALCOMD3 contributors", () => {
	test("loads repository contributors directly from the GitHub REST API", async () => {
		const fetchMock = vi.fn(async (_input: RequestInfo | URL) => {
			return Response.json([
				{
					avatar_url: "https://avatars.example/cqmhv",
					html_url: "https://github.com/CQMHV",
					login: "CQMHV",
				},
				{
					avatar_url: "https://avatars.example/contributor",
					html_url: "https://github.com/contributor",
					login: "contributor",
				},
			]);
		});

		const contributors = await fetchAlcomd3Contributors(fetchMock);

		expect(contributors).toEqual([
			{
				avatarUrl: "https://avatars.example/cqmhv",
				name: "CQMHV",
				profileUrl: "https://github.com/CQMHV",
			},
			{
				avatarUrl: "https://avatars.example/contributor",
				name: "contributor",
				profileUrl: "https://github.com/contributor",
			},
		]);
		expect(fetchMock).toHaveBeenCalledTimes(1);
		const requestUrl = new URL(String(fetchMock.mock.calls[0]?.[0]));
		expect(requestUrl.href).toBe(
			"https://api.github.com/repos/ALCOMD3/ALCOMD3/contributors?per_page=100",
		);
	});

	test("ignores incomplete GitHub contributor entries", async () => {
		const fetchMock = vi.fn(async (_input: RequestInfo | URL) => {
			return Response.json([
				{
					login: "missing-profile-data",
				},
			]);
		});

		await expect(fetchAlcomd3Contributors(fetchMock)).resolves.toEqual([]);
	});

	test("returns an empty list when GitHub is unavailable", async () => {
		const fetchMock = vi.fn(
			async (_input: RequestInfo | URL) => new Response(null, { status: 429 }),
		);
		const consoleWarn = vi
			.spyOn(console, "warn")
			.mockImplementation(() => undefined);

		await expect(loadAlcomd3Contributors(fetchMock)).resolves.toEqual([]);
		expect(consoleWarn).toHaveBeenCalledOnce();

		consoleWarn.mockRestore();
	});
});
