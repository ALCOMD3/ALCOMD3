import { describe, expect, it } from "vitest";

import { shouldInstallAfterDownload } from "@/lib/update-download-policy";

describe("update download policy", () => {
	it("waits for confirmation after an automatic download", () => {
		expect(shouldInstallAfterDownload(true)).toBe(false);
	});

	it("installs after a manually confirmed download", () => {
		expect(shouldInstallAfterDownload(false)).toBe(true);
	});
});
