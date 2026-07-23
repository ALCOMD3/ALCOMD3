import { webcrypto } from "node:crypto";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, beforeAll } from "vitest";

beforeAll(() => {
	if (typeof window === "undefined") {
		return;
	}
	Object.defineProperty(window, "crypto", {
		configurable: true,
		value: webcrypto,
	});
});

afterEach(() => {
	if (typeof window === "undefined") {
		return;
	}
	clearMocks();
	localStorage.clear();
	document.head.replaceChildren();
	document.body.replaceChildren();
	document.documentElement.className = "";
	document.documentElement.removeAttribute("style");
});
