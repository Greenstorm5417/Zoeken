import { expect, test } from "@playwright/test";

test("opt-in live homepage", async ({ page }) => {
	const live = process.env.ZOEK_LIVE_URL;
	test.skip(!live, "set ZOEK_LIVE_URL to run the live smoke");
	await page.goto(live ?? "/");
	await expect(page.getByPlaceholder("Search…")).toBeVisible();
});
