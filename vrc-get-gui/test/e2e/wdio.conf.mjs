import {
	existsSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { isStrictChildPath } from "./path-safety.mjs";

const configDirectory = path.dirname(fileURLToPath(import.meta.url));
const workspaceRoot = path.resolve(configDirectory, "../../..");
const target = process.env.ALCOMD3_E2E_TARGET ?? "x86_64-pc-windows-msvc";
const profile = process.env.ALCOMD3_E2E_PROFILE ?? "debug";
const binaryName = process.platform === "win32" ? "ALCOMD3.exe" : "ALCOMD3";
const appBinaryPath =
	process.env.ALCOMD3_E2E_BINARY ??
	path.join(workspaceRoot, "target", target, profile, binaryName);
const suppliedDataRoot = process.env.ALCOMD3_TEST_LOCAL_DATA_ROOT;
const testRootInitialized =
	process.env.ALCOMD3_E2E_TEST_ROOT_INITIALIZED === "1";
const testDataRoot = suppliedDataRoot
	? path.resolve(suppliedDataRoot)
	: mkdtempSync(path.join(tmpdir(), "alcomd3-e2e-"));
const endpointFile = path.join(testDataRoot, "mcp-endpoint.json");
const driverProvider = process.env.ALCOMD3_E2E_DRIVER_PROVIDER ?? "external";
const embeddedPort = Number(
	process.env.ALCOMD3_E2E_EMBEDDED_PORT ??
		process.env.TAURI_WEBDRIVER_PORT ??
		process.env.WDIO_EMBEDDED_PORT ??
		4445,
);
const autoDownloadEdgeDriver =
	process.platform === "win32" || driverProvider === "external";

if (!["embedded", "external"].includes(driverProvider)) {
	throw new Error(
		`ALCOMD3_E2E_DRIVER_PROVIDER must be embedded or external, received: ${driverProvider}`,
	);
}

if (
	!Number.isInteger(embeddedPort) ||
	embeddedPort <= 0 ||
	embeddedPort > 65535
) {
	throw new Error(
		`ALCOMD3_E2E_EMBEDDED_PORT must be a TCP port number, received: ${embeddedPort}`,
	);
}

function assertSafeTestDataRoot() {
	const allowedRoots = [tmpdir(), path.join(workspaceRoot, "target")];
	if (process.env.GITHUB_ACTIONS === "true" && process.env.RUNNER_TEMP) {
		allowedRoots.push(process.env.RUNNER_TEMP);
	}
	if (!allowedRoots.some((root) => isStrictChildPath(root, testDataRoot))) {
		throw new Error(
			`Desktop E2E data root must be a child of a temporary or target directory: ${testDataRoot}`,
		);
	}
}

if (suppliedDataRoot && !testRootInitialized) {
	assertSafeTestDataRoot();
	if (existsSync(testDataRoot) && readdirSync(testDataRoot).length > 0) {
		throw new Error(`Desktop E2E data root must start empty: ${testDataRoot}`);
	}
}

mkdirSync(testDataRoot, { recursive: true });
if (suppliedDataRoot && !testRootInitialized) {
	assertSafeTestDataRoot();
}

const fixtureProjectName = "ALCOMD3 E2E Project";
if (profile === "debug" && !testRootInitialized) {
	const applicationDataRoot = path.join(testDataRoot, "ALCOMD3");
	const fixtureProjectRoot = path.join(
		testDataRoot,
		"fixtures",
		fixtureProjectName,
	);
	mkdirSync(path.join(fixtureProjectRoot, "Assets"), { recursive: true });
	mkdirSync(path.join(fixtureProjectRoot, "Packages"), { recursive: true });
	mkdirSync(path.join(fixtureProjectRoot, "ProjectSettings"), {
		recursive: true,
	});
	mkdirSync(applicationDataRoot, { recursive: true });
	writeFileSync(
		path.join(fixtureProjectRoot, "Packages", "vpm-manifest.json"),
		`${JSON.stringify({ dependencies: {}, locked: {} }, null, 4)}\n`,
	);
	writeFileSync(
		path.join(fixtureProjectRoot, "ProjectSettings", "ProjectVersion.txt"),
		"m_EditorVersion: 2022.3.22f1\n",
	);
	writeFileSync(
		path.join(applicationDataRoot, "settings.json"),
		`${JSON.stringify({ userProjects: [fixtureProjectRoot] }, null, 4)}\n`,
	);
	process.env.ALCOMD3_E2E_PROJECT_NAME = fixtureProjectName;
}

process.env.ALCOMD3_E2E_TEST_ROOT_INITIALIZED = "1";

if (profile === "release" && process.env.GITHUB_ACTIONS !== "true") {
	throw new Error(
		"Release-profile desktop E2E may only run on an ephemeral GitHub runner.",
	);
}

if (!existsSync(appBinaryPath)) {
	throw new Error(
		`Desktop E2E binary does not exist: ${appBinaryPath}. Build it before running this suite.`,
	);
}

process.env.ALCOMD3_TEST_LOCAL_DATA_ROOT = testDataRoot;
process.env.ALCOMD3_MCP_ENDPOINT_FILE = endpointFile;
process.env.ALCOMD3_TEST_DISABLE_SYSTEM_INTEGRATION = "1";

export const config = {
	runner: "local",
	specs: [path.join(configDirectory, "*.spec.mjs")],
	maxInstances: 1,
	...(driverProvider === "embedded"
		? {
				hostname: "127.0.0.1",
				port: embeddedPort,
			}
		: {}),
	capabilities: [
		{
			browserName: "tauri",
			"tauri:options": {
				application: appBinaryPath,
			},
		},
	],
	services: [
		[
			"@wdio/tauri-service",
			{
				appBinaryPath,
				driverProvider,
				embeddedPort,
				autoInstallTauriDriver: driverProvider === "external",
				autoDownloadEdgeDriver,
				env: {
					ALCOMD3_TEST_LOCAL_DATA_ROOT: testDataRoot,
					ALCOMD3_MCP_ENDPOINT_FILE: endpointFile,
					ALCOMD3_TEST_DISABLE_SYSTEM_INTEGRATION: "1",
				},
				startTimeout: 60_000,
				commandTimeout: 30_000,
			},
		],
	],
	logLevel: "info",
	outputDir: path.join(workspaceRoot, "target", "e2e-desktop-logs"),
	bail: 0,
	waitforTimeout: 20_000,
	connectionRetryTimeout: 120_000,
	connectionRetryCount: 2,
	framework: "mocha",
	reporters: ["spec"],
	mochaOpts: {
		ui: "bdd",
		timeout: 60_000,
	},
	onComplete(exitCode, _configuration, _capabilities, results) {
		const resultFile = process.env.ALCOMD3_E2E_RESULT_FILE;
		if (resultFile) {
			writeFileSync(
				resultFile,
				`${JSON.stringify(
					{
						exitCode,
						failed: results?.failed ?? null,
						passed: results?.passed ?? null,
					},
					null,
					4,
				)}\n`,
			);
		}
		if (!suppliedDataRoot) {
			const resolvedTempRoot = path.resolve(tmpdir());
			const resolvedTestRoot = path.resolve(testDataRoot);
			if (
				path.dirname(resolvedTestRoot) === resolvedTempRoot &&
				path.basename(resolvedTestRoot).startsWith("alcomd3-e2e-")
			) {
				rmSync(resolvedTestRoot, { recursive: true, force: true });
			}
		}
	},
};
