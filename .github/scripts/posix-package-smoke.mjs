import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import {
	closeSync,
	existsSync,
	mkdirSync,
	openSync,
	readdirSync,
	readFileSync,
	statSync,
} from "node:fs";
import path from "node:path";
import { callLocalIpc } from "../../vrc-get-gui/test/e2e/local-ipc.mjs";
import { isStrictChildPath } from "../../vrc-get-gui/test/e2e/path-safety.mjs";

const argumentsByName = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
	const name = process.argv[index];
	const value = process.argv[index + 1];
	if (!name?.startsWith("--") || !value) {
		throw new Error(`Invalid argument near ${name ?? "end of command"}`);
	}
	argumentsByName.set(name, value);
}

const binary = path.resolve(requiredArgument("--binary"));
const dataRoot = path.resolve(requiredArgument("--data-root"));
const runnerTempSource = process.env.RUNNER_TEMP;
const runnerTemp = runnerTempSource ? path.resolve(runnerTempSource) : null;
const label = argumentsByName.get("--label") ?? path.basename(binary);
const pidMode = argumentsByName.get("--pid-mode") ?? "exact";

if (
	process.env.GITHUB_ACTIONS !== "true" ||
	process.env.RUNNER_ENVIRONMENT !== "github-hosted" ||
	!["darwin", "linux"].includes(process.platform)
) {
	throw new Error(
		"Packaged application smoke tests may only run on an ephemeral GitHub-hosted macOS or Linux runner.",
	);
}
if (!runnerTemp || !isStrictChildPath(runnerTemp, dataRoot)) {
	throw new Error(`Data root must be a child of RUNNER_TEMP: ${dataRoot}`);
}
if (!["exact", "process-group"].includes(pidMode)) {
	throw new Error(`Unsupported --pid-mode: ${pidMode}`);
}
if (pidMode === "process-group" && process.platform !== "linux") {
	throw new Error("The process-group PID mode is only supported on Linux.");
}
if (!existsSync(binary)) {
	throw new Error(`Packaged application binary does not exist: ${binary}`);
}
if (existsSync(dataRoot) && readdirSync(dataRoot).length > 0) {
	throw new Error(
		`Packaged application data root must start empty: ${dataRoot}`,
	);
}

const home = path.join(dataRoot, "home");
const xdgDataHome = path.join(dataRoot, "xdg-data");
const xdgConfigHome = path.join(dataRoot, "xdg-config");
const xdgCacheHome = path.join(dataRoot, "xdg-cache");
const endpointFile = path.join(dataRoot, "mcp-endpoint.json");
const logFile = path.join(dataRoot, "application.log");
for (const directory of [home, xdgDataHome, xdgConfigHome, xdgCacheHome]) {
	mkdirSync(directory, { recursive: true });
}

const log = openSync(logFile, "w");
const child = spawn(binary, [], {
	detached: true,
	env: {
		...process.env,
		HOME: home,
		XDG_DATA_HOME: xdgDataHome,
		XDG_CONFIG_HOME: xdgConfigHome,
		XDG_CACHE_HOME: xdgCacheHome,
		ALCOMD3_MCP_ENDPOINT_FILE: endpointFile,
		APPIMAGE_EXTRACT_AND_RUN: "1",
		WEBKIT_DISABLE_COMPOSITING_MODE: "1",
	},
	stdio: ["ignore", log, log],
});
let spawnError;
child.once("error", (error) => {
	spawnError = error;
});

let smokeError;
try {
	const endpoint = await waitForEndpoint(
		endpointFile,
		child,
		() => spawnError,
		60_000,
	);
	if (
		endpoint.protocolVersion !== 2 ||
		endpoint.transport !== "tcp" ||
		endpoint.host !== "127.0.0.1" ||
		!Number.isInteger(endpoint.port) ||
		endpoint.port < 1 ||
		endpoint.port > 65_535 ||
		!/^[0-9a-f]{32}$/.test(endpoint.token) ||
		!Number.isInteger(endpoint.pid) ||
		endpoint.pid < 1 ||
		!endpointPidMatchesLaunch(endpoint.pid, child.pid, pidMode)
	) {
		throw new Error(
			`Invalid MCP endpoint metadata: ${JSON.stringify(endpoint)}`,
		);
	}

	const requestId = randomUUID();
	const response = await callLocalIpc(endpoint, {
		protocolVersion: endpoint.protocolVersion,
		token: endpoint.token,
		requestId,
		client: {
			sessionId: randomUUID(),
			name: "alcomd3-package-smoke",
			version: "1",
		},
		method: "list_projects",
		params: {},
	});
	if (
		response.requestId !== requestId ||
		response.ok !== false ||
		response.error?.code !== "mcp_disabled"
	) {
		throw new Error(
			`Unexpected default MCP response: ${JSON.stringify(response)}`,
		);
	}

	const unauthorizedId = randomUUID();
	const unauthorized = await callLocalIpc(endpoint, {
		protocolVersion: endpoint.protocolVersion,
		token: "0".repeat(32),
		requestId: unauthorizedId,
		client: {
			sessionId: randomUUID(),
			name: "alcomd3-package-smoke",
			version: "1",
		},
		method: "list_projects",
		params: {},
	});
	if (
		unauthorized.requestId !== unauthorizedId ||
		unauthorized.ok !== false ||
		unauthorized.error?.code !== "unauthorized"
	) {
		throw new Error(
			`Unexpected unauthorized MCP response: ${JSON.stringify(unauthorized)}`,
		);
	}

	const applicationDataDirectory = path.join(xdgDataHome, "ALCOMD3");
	if (
		!existsSync(applicationDataDirectory) ||
		!statSync(applicationDataDirectory).isDirectory()
	) {
		throw new Error(
			"The packaged application did not initialize its data directory.",
		);
	}
	console.log(`${label}: launch and local MCP boundary smoke passed.`);
} catch (error) {
	smokeError = error;
} finally {
	await terminateProcessGroup(child);
	closeSync(log);
}

if (smokeError) {
	let diagnostic = "";
	try {
		diagnostic = readFileSync(logFile, "utf8");
	} catch {}
	throw new Error(
		`${label}: packaged application smoke failed: ${smokeError.message}\nApplication log: ${logFile}\n${diagnostic}`,
		{ cause: smokeError },
	);
}

function requiredArgument(name) {
	const value = argumentsByName.get(name);
	if (!value) {
		throw new Error(`${name} is required`);
	}
	return value;
}

async function waitForEndpoint(
	endpointPath,
	application,
	getSpawnError,
	timeoutMilliseconds,
) {
	const deadline = Date.now() + timeoutMilliseconds;
	while (Date.now() < deadline) {
		if (getSpawnError()) {
			throw new Error(
				`Unable to start application: ${getSpawnError().message}`,
				{
					cause: getSpawnError(),
				},
			);
		}
		if (application.exitCode !== null || application.signalCode !== null) {
			throw new Error(
				`Application exited before startup completed (exit ${application.exitCode}, signal ${application.signalCode ?? "none"}).`,
			);
		}
		if (existsSync(endpointPath)) {
			try {
				return JSON.parse(readFileSync(endpointPath, "utf8"));
			} catch {}
		}
		await new Promise((resolve) => setTimeout(resolve, 250));
	}
	throw new Error(
		`Application did not create ${endpointPath} within ${timeoutMilliseconds / 1000} seconds.`,
	);
}

async function terminateProcessGroup(application) {
	if (!application.pid) {
		return;
	}
	try {
		process.kill(-application.pid, "SIGTERM");
	} catch (error) {
		if (error?.code !== "ESRCH") {
			throw error;
		}
	}
	await new Promise((resolve) => setTimeout(resolve, 2_000));
	if (processGroupExists(application.pid)) {
		try {
			process.kill(-application.pid, "SIGKILL");
		} catch (error) {
			if (error?.code !== "ESRCH") {
				throw error;
			}
		}
	}
}

function endpointPidMatchesLaunch(endpointPid, launcherPid, mode) {
	if (mode === "exact") {
		return endpointPid === launcherPid;
	}
	try {
		const stat = readFileSync(`/proc/${endpointPid}/stat`, "utf8");
		const fields = stat
			.slice(stat.lastIndexOf(")") + 1)
			.trim()
			.split(/\s+/);
		const processGroup = Number.parseInt(fields[2], 10);
		return Number.isInteger(processGroup) && processGroup === launcherPid;
	} catch {
		return false;
	}
}

function processGroupExists(processGroupId) {
	try {
		process.kill(-processGroupId, 0);
		return true;
	} catch (error) {
		if (error?.code === "ESRCH") {
			return false;
		}
		throw error;
	}
}
