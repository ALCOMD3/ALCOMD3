import { spawn } from "node:child_process";
import { existsSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform === "win32") {
	throw new Error("Use run-desktop-e2e.ps1 on Windows.");
}

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const guiDirectory = path.resolve(scriptDirectory, "..");
const wdio = path.join(
	guiDirectory,
	"node_modules",
	"@wdio",
	"cli",
	"bin",
	"wdio.js",
);
const configuration = path.join(guiDirectory, "test", "e2e", "wdio.conf.mjs");
const resultFile = path.join(
	process.env.RUNNER_TEMP ?? tmpdir(),
	`alcomd3-wdio-result-${process.pid}.json`,
);
const timeoutMilliseconds = 15 * 60 * 1000;

if (!existsSync(wdio)) {
	throw new Error("WebdriverIO is not installed. Run npm ci first.");
}

rmSync(resultFile, { force: true });
const child = spawn(process.execPath, [wdio, "run", configuration], {
	cwd: process.cwd(),
	detached: true,
	env: {
		...process.env,
		ALCOMD3_E2E_RESULT_FILE: resultFile,
	},
	stdio: "inherit",
});

let timedOut = false;
const timeout = setTimeout(() => {
	timedOut = true;
	terminateProcessGroup("SIGTERM");
	setTimeout(() => terminateProcessGroup("SIGKILL"), 10_000).unref();
}, timeoutMilliseconds);

function terminateProcessGroup(signal) {
	if (child.pid) {
		try {
			process.kill(-child.pid, signal);
		} catch (error) {
			if (error?.code !== "ESRCH") {
				throw error;
			}
		}
	}
}

function waitForExit() {
	return new Promise((resolve, reject) => {
		child.once("error", reject);
		child.once("exit", (code, signal) => resolve({ code, signal }));
	});
}

let exitCode = 1;
try {
	const result = await waitForExit();
	if (timedOut) {
		throw new Error("Desktop E2E exceeded its 15-minute timeout.");
	}
	if (!existsSync(resultFile)) {
		throw new Error(
			`WebdriverIO did not write a completion result (exit ${result.code}, signal ${result.signal ?? "none"}).`,
		);
	}
	const completion = JSON.parse(readFileSync(resultFile, "utf8"));
	if (
		completion.exitCode !== 0 ||
		completion.failed !== 0 ||
		!Number.isInteger(completion.passed) ||
		completion.passed < 1
	) {
		throw new Error(`Desktop E2E failed: ${JSON.stringify(completion)}`);
	}
	exitCode = 0;
} finally {
	clearTimeout(timeout);
	terminateProcessGroup("SIGTERM");
	rmSync(resultFile, { force: true });
}

process.exitCode = exitCode;
