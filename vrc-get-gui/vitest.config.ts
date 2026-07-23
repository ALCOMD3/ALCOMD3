import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
	resolve: {
		alias: {
			"@": fileURLToPath(new URL(".", import.meta.url)),
		},
	},
	test: {
		environment: "jsdom",
		include: ["test/**/*.test.{ts,tsx,mjs}"],
		setupFiles: ["./test/setup.ts"],
		clearMocks: true,
		restoreMocks: true,
	},
});
