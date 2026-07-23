import { emit, listen } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, test, vi } from "vitest";
import { callAsyncCommand } from "@/lib/call-async-command";

function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((resolvePromise) => {
		resolve = resolvePromise;
	});
	return { promise, resolve };
}

describe("callAsyncCommand", () => {
	test("returns an immediate result without waiting for events", async () => {
		mockIPC(() => undefined, { shouldMockEvents: true });
		const progress = vi.fn();
		const command = vi.fn(async (_channel: string, value: number) => ({
			type: "Result" as const,
			value: value * 2,
		}));

		const [, result] = callAsyncCommand(command, [21], progress);

		await expect(result).resolves.toBe(42);
		expect(command).toHaveBeenCalledOnce();
		expect(progress).not.toHaveBeenCalled();
		const channel = command.mock.calls[0][0];
		await emit(`${channel}:progress`, { current: 1 });
		expect(progress).not.toHaveBeenCalled();
	});

	test("forwards progress and resolves a finished operation", async () => {
		mockIPC(() => undefined, { shouldMockEvents: true });
		const started = deferred<string>();
		const progress = vi.fn();
		const command = vi.fn(async (channel: string) => {
			started.resolve(channel);
			return { type: "Started" as const };
		});

		const [, result] = callAsyncCommand(command, [], progress);
		const channel = await started.promise;
		await emit(`${channel}:progress`, { current: 2, total: 5 });
		await emit(`${channel}:finished`, {
			type: "Success",
			value: "done",
		});

		expect(progress).toHaveBeenCalledOnce();
		expect(progress).toHaveBeenCalledWith({ current: 2, total: 5 });
		await expect(result).resolves.toBe("done");
		await emit(`${channel}:progress`, { current: 3, total: 5 });
		expect(progress).toHaveBeenCalledOnce();
	});

	test("rejects a failed operation and removes all event listeners", async () => {
		mockIPC(() => undefined, { shouldMockEvents: true });
		const started = deferred<string>();
		const progress = vi.fn();
		const command = vi.fn(async (channel: string) => {
			started.resolve(channel);
			return { type: "Started" as const };
		});

		const [, result] = callAsyncCommand(command, [], progress);
		const channel = await started.promise;
		await emit(`${channel}:finished`, {
			type: "Failed",
			value: "backend failed",
		});

		await expect(result).rejects.toBe("backend failed");
		await emit(`${channel}:progress`, { current: 1 });
		expect(progress).not.toHaveBeenCalled();
	});

	test("delivers cancellation requested before the backend is ready", async () => {
		mockIPC(() => undefined, { shouldMockEvents: true });
		const started = deferred<string>();
		const cancelReceived = deferred<void>();
		const command = vi.fn(async (channel: string) => {
			await listen(`${channel}:cancel`, () => cancelReceived.resolve());
			started.resolve(channel);
			return { type: "Started" as const };
		});

		const [cancel, result] = callAsyncCommand(command, [], vi.fn());
		cancel();
		const channel = await started.promise;
		await cancelReceived.promise;
		await emit(`${channel}:cancelled`);

		await expect(result).resolves.toBe("cancelled");
	});

	test("cleans listeners when invoking the command throws", async () => {
		mockIPC(() => undefined, { shouldMockEvents: true });
		const started = deferred<string>();
		const progress = vi.fn();
		const command = vi.fn(async (channel: string) => {
			started.resolve(channel);
			throw new Error("invoke failed");
		});

		const [, result] = callAsyncCommand(command, [], progress);
		const channel = await started.promise;

		await expect(result).rejects.toThrow("invoke failed");
		await emit(`${channel}:progress`, { current: 1 });
		expect(progress).not.toHaveBeenCalled();
	});
});
