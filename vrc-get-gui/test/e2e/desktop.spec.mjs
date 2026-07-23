import { randomUUID } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { browser, expect } from "@wdio/globals";
import { callLocalIpc } from "./local-ipc.mjs";

const requiredSetupRoutes = [
	"/setup/appearance/",
	"/setup/legacy-import/",
	"/setup/unity-hub/",
	"/setup/project-path/",
	"/setup/backups/",
];

async function currentPathname() {
	const pathname = new URL(await browser.getUrl()).pathname;
	return pathname.endsWith("/") ? pathname : `${pathname}/`;
}

async function clickLastVisibleButtonAndWaitForNavigation() {
	const previousPathname = await currentPathname();
	const clicked = await browser.execute(() => {
		const visibleButtons = [...document.querySelectorAll("button")].filter(
			(button) => {
				const bounds = button.getBoundingClientRect();
				return !button.disabled && bounds.width > 0 && bounds.height > 0;
			},
		);
		const button = visibleButtons.at(-1);
		button?.click();
		return button?.textContent?.trim() ?? null;
	});
	expect(clicked).toBeTruthy();
	await browser.waitUntil(
		async () => (await currentPathname()) !== previousPathname,
		{
			timeoutMsg: `Setup did not navigate away from ${previousPathname}`,
		},
	);
}

describe("ALCOMD3 desktop startup", () => {
	it("starts the real Tauri application with an interactive first-run page", async () => {
		await browser.waitUntil(
			async () =>
				(await browser.execute(() => document.body.innerText)).trim().length >
				0,
			{
				timeoutMsg: "ALCOMD3 did not render any visible content",
			},
		);

		const page = await browser.execute(() => ({
			title: document.title,
			bodyText: document.body.innerText,
			headingCount: document.querySelectorAll("h1, h2").length,
			buttonCount: document.querySelectorAll("button").length,
		}));
		expect(page.title).toContain("ALCOMD3");
		expect(page.bodyText.toLowerCase()).not.toContain("unrecoverable error");
		expect(page.headingCount).toBeGreaterThan(0);
		expect(page.buttonCount).toBeGreaterThan(0);
		expect(await browser.getUrl()).toMatch(
			/^(?:http:\/\/tauri\.localhost|tauri:\/\/localhost)\//,
		);
		expect(await currentPathname()).toBe(requiredSetupRoutes[0]);

		const testDataRoot = process.env.ALCOMD3_TEST_LOCAL_DATA_ROOT;
		expect(testDataRoot).toBeTruthy();
		const settingsFile = path.join(testDataRoot, "ALCOMD3", "settings.json");
		expect(existsSync(settingsFile)).toBe(true);
		const settings = JSON.parse(readFileSync(settingsFile, "utf8"));
		expect(settings.userProjects).toHaveLength(1);
	});

	it("keeps MCP access disabled and its IPC endpoint loopback-only by default", async () => {
		const endpointFile = process.env.ALCOMD3_MCP_ENDPOINT_FILE;
		expect(endpointFile).toBeTruthy();
		const endpoint = JSON.parse(readFileSync(endpointFile, "utf8"));

		expect(endpoint.protocolVersion).toBe(2);
		expect(endpoint.transport).toBe("tcp");
		expect(endpoint.host).toBe("127.0.0.1");
		expect(endpoint.port).toBeGreaterThan(0);
		expect(endpoint.token).toMatch(/^[0-9a-f]{32}$/);

		const requestId = randomUUID();
		const response = await callLocalIpc(endpoint, {
			protocolVersion: endpoint.protocolVersion,
			token: endpoint.token,
			requestId,
			client: {
				sessionId: randomUUID(),
				name: "alcomd3-desktop-e2e",
				version: "1",
			},
			method: "list_projects",
			params: {},
		});
		expect(response.requestId).toBe(requestId);
		expect(response.ok).toBe(false);
		expect(response.error.code).toBe("mcp_disabled");
	});

	it("completes first-run setup, discovers an isolated project, and persists setup", async () => {
		const visitedRoutes = [];
		for (let step = 0; step < 8; step += 1) {
			const pathname = await currentPathname();
			if (pathname === "/projects/") {
				break;
			}
			expect([
				...requiredSetupRoutes,
				"/setup/system-setting/",
				"/setup/finish/",
			]).toContain(pathname);
			expect(visitedRoutes).not.toContain(pathname);
			visitedRoutes.push(pathname);
			await clickLastVisibleButtonAndWaitForNavigation();
		}

		expect(await currentPathname()).toBe("/projects/");
		for (const route of requiredSetupRoutes) {
			expect(visitedRoutes).toContain(route);
		}
		expect(visitedRoutes).toContain("/setup/finish/");

		const fixtureProjectName = process.env.ALCOMD3_E2E_PROJECT_NAME;
		expect(fixtureProjectName).toBeTruthy();
		await browser.waitUntil(
			async () =>
				(await browser.execute(() => document.body.innerText)).includes(
					fixtureProjectName,
				),
			{ timeoutMsg: `Project list did not render ${fixtureProjectName}` },
		);

		const testDataRoot = process.env.ALCOMD3_TEST_LOCAL_DATA_ROOT;
		const guiConfigFile = path.join(
			testDataRoot,
			"ALCOMD3",
			"config",
			"gui-config.json",
		);
		expect(existsSync(guiConfigFile)).toBe(true);
		const guiConfig = JSON.parse(readFileSync(guiConfigFile, "utf8"));
		expect(guiConfig.mcpEnabled).toBe(false);
		expect(guiConfig.setupProcessProgress & 0x2f).toBe(0x2f);

		await browser.reloadSession();
		await browser.waitUntil(
			async () => (await currentPathname()) === "/projects/",
			{
				timeoutMsg:
					"Restarted application did not retain completed setup state",
			},
		);
		await browser.waitUntil(
			async () =>
				(await browser.execute(() => document.body.innerText)).includes(
					fixtureProjectName,
				),
			{
				timeoutMsg: `Restarted application did not retain ${fixtureProjectName}`,
			},
		);
	});
});
