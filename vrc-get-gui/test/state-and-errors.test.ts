import { describe, expect, test, vi } from "vitest";
import { isHandleable } from "@/lib/errors";

const { useLocationMock } = vi.hoisted(() => ({
	useLocationMock: vi.fn(),
}));

vi.mock("@tanstack/react-router", async (importOriginal) => {
	const original =
		await importOriginal<typeof import("@tanstack/react-router")>();
	return {
		...original,
		useLocation: useLocationMock,
	};
});

describe("isHandleable", () => {
	test("accepts the structured error contract exposed by Rust", () => {
		expect(
			isHandleable({
				type: "Handleable",
				message: "Project is locked",
				body: { type: "ProjectIsRunning" },
			}),
		).toBe(true);
	});

	test.each([
		null,
		"Handleable",
		{ type: "Handleable", message: "missing body" },
		{ type: "Handleable", message: 1, body: { type: "Failure" } },
		{ type: "Handleable", message: "wrong body", body: null },
		{ type: "Unrecoverable", message: "wrong type", body: { type: "Failure" } },
	])("rejects malformed errors: %j", (value) => {
		expect(isHandleable(value)).toBe(false);
	});
});

describe("previous route state", () => {
	test("tracks navigation and ignores repeated renders on the same route", async () => {
		vi.resetModules();
		const { updateCurrentPath, usePrevPathName } = await import(
			"@/lib/prev-page"
		);
		useLocationMock
			.mockReset()
			.mockReturnValue({ pathname: "/settings" })
			.mockReturnValueOnce({ pathname: "/projects" })
			.mockReturnValueOnce({ pathname: "/projects" });

		expect(usePrevPathName()).toBe("");
		expect(usePrevPathName()).toBe("");
		expect(usePrevPathName()).toBe("/projects");

		updateCurrentPath("/settings");
		expect(usePrevPathName()).toBe("/projects");
	});
});
