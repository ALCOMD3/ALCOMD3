import { createConnection } from "node:net";

export function callLocalIpc(endpoint, request, timeoutMilliseconds = 5_000) {
	return new Promise((resolve, reject) => {
		let requestLine;
		try {
			const serializedRequest = JSON.stringify(request);
			if (typeof serializedRequest !== "string") {
				throw new TypeError("MCP IPC request must be JSON serializable");
			}
			requestLine = `${serializedRequest}\n`;
		} catch (error) {
			reject(error);
			return;
		}

		const socket = createConnection({
			host: endpoint.host,
			port: endpoint.port,
		});
		let response = "";
		let settled = false;
		const fail = (error) => {
			if (settled) {
				return;
			}
			settled = true;
			socket.destroy();
			reject(error);
		};

		socket.setEncoding("utf8");
		socket.setTimeout(timeoutMilliseconds);
		socket.on("connect", () => socket.write(requestLine));
		socket.on("data", (chunk) => {
			if (settled) {
				return;
			}
			response += chunk;
			const newline = response.indexOf("\n");
			if (newline === -1) {
				return;
			}

			try {
				const result = JSON.parse(response.slice(0, newline));
				settled = true;
				socket.end();
				resolve(result);
			} catch (error) {
				fail(error);
			}
		});
		socket.on("timeout", () => fail(new Error("MCP IPC response timed out")));
		socket.on("error", fail);
		socket.on("close", () =>
			fail(new Error("MCP IPC socket closed before a complete response")),
		);
	});
}
