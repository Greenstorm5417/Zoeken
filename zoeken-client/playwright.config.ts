import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
	testDir: "./e2e",
	testMatch: "**/*.browser.ts",
	fullyParallel: false,
	workers: 1,
	retries: 0,
	use: {
		baseURL: "http://127.0.0.1:3010",
		...devices["Desktop Chrome"],
	},
	webServer: {
		command: "ZOEK_E2E=1 bunx vite dev --port 3010 --strictPort",
		url: "http://127.0.0.1:3010",
		reuseExistingServer: false,
		timeout: 120_000,
	},
});
