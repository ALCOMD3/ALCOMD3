import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, test, vi } from "vitest";
import { commands } from "@/lib/bindings";

describe("Tauri command bindings", () => {
	test("serializes settings commands with the backend field names", async () => {
		const ipc = vi.fn(() => null);
		mockIPC(ipc);

		await commands.environmentSetLanguage("zh-CN");
		await commands.environmentSetGuiAnimation(false);
		await commands.environmentSetAutomaticUpdate(false);

		expect(ipc).toHaveBeenNthCalledWith(1, "environment_set_language", {
			language: "zh-CN",
		});
		expect(ipc).toHaveBeenNthCalledWith(2, "environment_set_gui_animation", {
			guiAnimation: false,
		});
		expect(ipc).toHaveBeenNthCalledWith(3, "environment_set_automatic_update", {
			automaticUpdate: false,
		});
	});

	test("keeps destructive project arguments explicit", async () => {
		const ipc = vi.fn(() => null);
		mockIPC(ipc);

		await commands.environmentRemoveProjectByPath(
			"C:\\Projects\\Avatar",
			false,
		);

		expect(ipc).toHaveBeenCalledWith("environment_remove_project_by_path", {
			projectPath: "C:\\Projects\\Avatar",
			directory: false,
		});
	});

	test("passes async channel and update download options without renaming", async () => {
		const ipc = vi.fn(() => ({ type: "Started" as const }));
		mockIPC(ipc);

		await commands.utilDownloadUpdate("async_call:test", true, 42);

		expect(ipc).toHaveBeenCalledWith("util_download_update", {
			channel: "async_call:test",
			automatic: true,
			version: 42,
		});
	});
});
