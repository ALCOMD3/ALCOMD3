// @vitest-environment node

import {
	mkdirSync,
	mkdtempSync,
	realpathSync,
	rmSync,
	symlinkSync,
} from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { callLocalIpc } from "./local-ipc.mjs";
import { canonicalizePath, isStrictChildPath } from "./path-safety.mjs";

const temporaryDirectories = [];

afterEach(() => {
	for (const directory of temporaryDirectories.splice(0)) {
		rmSync(directory, { recursive: true, force: true });
	}
});

async function withTcpServer(connectionListener, callback) {
	const sockets = new Set();
	const server = createServer((socket) => {
		sockets.add(socket);
		socket.once("close", () => sockets.delete(socket));
		connectionListener(socket);
	});
	await new Promise((resolve, reject) => {
		server.once("error", reject);
		server.listen(0, "127.0.0.1", () => {
			server.off("error", reject);
			resolve();
		});
	});

	try {
		const address = server.address();
		if (!address || typeof address === "string") {
			throw new Error("TCP test server did not expose an IP endpoint");
		}
		return await callback(address.port);
	} finally {
		for (const socket of sockets) {
			socket.destroy();
		}
		await new Promise((resolve, reject) => {
			server.close((error) => (error ? reject(error) : resolve()));
		});
	}
}

describe("local IPC helper", () => {
	it("resolves a complete newline-delimited JSON response", async () => {
		await withTcpServer(
			(socket) => socket.end('{"ok":true}\n'),
			async (port) => {
				await expect(
					callLocalIpc({ host: "127.0.0.1", port }, { method: "ping" }),
				).resolves.toEqual({ ok: true });
			},
		);
	});

	it("rejects when the peer closes before a complete response", async () => {
		await withTcpServer(
			(socket) => socket.end('{"ok":'),
			async (port) => {
				await expect(
					callLocalIpc({ host: "127.0.0.1", port }, { method: "ping" }),
				).rejects.toThrow("closed before a complete response");
			},
		);
	});

	it("rejects malformed newline-delimited JSON", async () => {
		await withTcpServer(
			(socket) => socket.end("not-json\n"),
			async (port) => {
				await expect(
					callLocalIpc({ host: "127.0.0.1", port }, { method: "ping" }),
				).rejects.toThrow();
			},
		);
	});

	it("rejects an unresponsive peer at the configured timeout", async () => {
		await withTcpServer(
			() => {},
			async (port) => {
				await expect(
					callLocalIpc({ host: "127.0.0.1", port }, { method: "ping" }, 25),
				).rejects.toThrow("timed out");
			},
		);
	});

	it("rejects a non-serializable request before opening a socket", async () => {
		let connections = 0;
		await withTcpServer(
			() => {
				connections += 1;
			},
			async (port) => {
				const request = {};
				request.circular = request;
				await expect(
					callLocalIpc({ host: "127.0.0.1", port }, request),
				).rejects.toThrow("circular");
			},
		);
		expect(connections).toBe(0);
	});
});

describe("desktop E2E path safety", () => {
	it("accepts strict descendants and rejects siblings", () => {
		const root = mkdtempSync(path.join(tmpdir(), "alcomd3-path-safety-"));
		temporaryDirectories.push(root);
		const allowed = path.join(root, "allowed");
		mkdirSync(allowed);

		expect(isStrictChildPath(allowed, path.join(allowed, "child"))).toBe(true);
		expect(isStrictChildPath(allowed, allowed)).toBe(false);
		expect(isStrictChildPath(allowed, path.join(root, "allowed-sibling"))).toBe(
			false,
		);
	});

	it("rejects a missing child reached through a link outside the allowed root", () => {
		const root = mkdtempSync(path.join(tmpdir(), "alcomd3-path-safety-"));
		temporaryDirectories.push(root);
		const allowed = path.join(root, "allowed");
		const outside = path.join(root, "outside");
		const link = path.join(allowed, "linked-outside");
		mkdirSync(allowed);
		mkdirSync(outside);
		symlinkSync(
			outside,
			link,
			process.platform === "win32" ? "junction" : "dir",
		);

		expect(isStrictChildPath(allowed, path.join(link, "missing"))).toBe(false);
		expect(canonicalizePath(path.join(link, "missing"))).toBe(
			path.join(realpathSync.native(outside), "missing"),
		);
	});
});
