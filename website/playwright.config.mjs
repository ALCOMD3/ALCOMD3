import { defineConfig, devices } from "@playwright/test";

const baseURL = process.env.PLAYWRIGHT_TEST_BASE_URL ?? "http://127.0.0.1:4322";
const previewUrl = new URL(baseURL);
const previewHost = previewUrl.hostname.replace(/^\[(.*)\]$/, "$1");

export default defineConfig({
    testDir: "./tests/e2e",
    outputDir: "../artifacts/website-playwright",
    fullyParallel: true,
    forbidOnly: Boolean(process.env.CI),
    retries: process.env.CI ? 2 : 0,
    workers: process.env.CI ? 2 : undefined,
    reporter: process.env.CI ? [["github"], ["line"]] : "line",
    use: {
        baseURL,
        locale: "en-US",
        serviceWorkers: "allow",
        trace: "on-first-retry",
        screenshot: "only-on-failure",
    },
    projects: [
        {
            name: "chromium",
            use: {
                ...devices["Desktop Chrome"],
            },
        },
    ],
    webServer: {
        command: `npm run preview -- --host ${previewHost} --port ${previewUrl.port}`,
        url: baseURL,
        reuseExistingServer: false,
        stdout: "pipe",
        stderr: "pipe",
    },
});
