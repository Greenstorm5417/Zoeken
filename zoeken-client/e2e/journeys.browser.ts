import { expect, test } from "@playwright/test";
import { e2eConfig, installMocks } from "./support";

test.beforeEach(async ({ page }) => {
	await installMocks(page).ready;
});

test("query, suggestion, submit, and result", async ({ page }) => {
	await page.goto("/");
	const box = page.getByPlaceholder("Search…");
	await box.click();
	await box.fill("ru");
	await expect(page.getByRole("option", { name: /rust language/ })).toBeVisible();
	await page.getByRole("option", { name: /rust language/ }).click();
	await expect(page.getByRole("link", { name: /Rust docs/ })).toBeVisible();
	await expect(page).toHaveURL(/q=rust(\+|%20)language/);
});

test("operator chip removal stays in the URL", async ({ page }) => {
	await page.goto("/search?q=filetype%3A.PDF+rust");
	await page.getByRole("link", { name: /filetype:pdf/i }).click();
	await expect(page).toHaveURL(/q=rust$/);
	await expect(page.getByRole("link", { name: /filetype:pdf/i })).toHaveCount(0);
});

test("infinite pagination keeps a result after a duplicate page", async ({ page }) => {
	await page.goto("/search?q=pages");
	await expect(page.getByRole("link", { name: "Alpha" })).toBeVisible();
	await expect(page.getByRole("link", { name: "Beta" })).toBeVisible();
	await expect(page.getByText("End of results")).toBeVisible();
});

test("local answers show while search is slow or failed", async ({ page }) => {
	await page.route("**/api/v1/search", async (route) => {
		await new Promise((resolve) => setTimeout(resolve, 1500));
		await route.abort();
	});
	await page.goto("/search?q=2%2B2");
	await expect(page.getByText("4", { exact: true }).first()).toBeVisible();
	await page.goto("/search?q=days%20until%202027-01-01");
	await expect(page.getByText(/days until 2027-01-01/).first()).toBeVisible();
});

test("failed, limited, and empty searches offer recovery", async ({ page }) => {
	await page.goto("/search?q=fail");
	await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();
	await page.goto("/search?q=limited");
	await expect(page.getByText("Please wait a moment and try again.")).toBeVisible();
	await page.goto("/search?q=nothing&time_range=week");
	await page.getByRole("link", { name: "Remove time filter" }).click();
	await expect(page).not.toHaveURL(/time_range/);
});

test("keyboard submits a query and opens a result", async ({ page }) => {
	await page.goto("/");
	const box = page.getByPlaceholder("Search…");
	await box.focus();
	await page.keyboard.type("pages");
	await page.keyboard.press("Enter");
	const result = page.getByRole("link", { name: "Alpha" });
	await expect(result).toBeVisible();
	await box.focus();
	for (let step = 0; step < 25; step += 1) {
		await page.keyboard.press("Tab");
		if (await result.evaluate((node) => node === document.activeElement)) break;
	}
	await expect(result).toBeFocused();
	await page.route("https://a.test/**", (route) =>
		route.fulfill({ status: 200, body: "opened" }),
	);
	await page.keyboard.press("Enter");
	await expect(page).toHaveURL(/a\.test/);
});

test("hide a site and undo", async ({ page }) => {
	await page.goto("/search?q=rust");
	await page.getByText("web").click();
	await page.getByRole("button", { name: "Hide docs.example.com" }).click();
	await expect(page.getByText("Hidden sites removed every result on this page.")).toBeVisible();
	await page.getByRole("button", { name: "Undo" }).click();
	await expect(page.getByRole("link", { name: /Rust docs/ })).toBeVisible();
});

test("image lightbox arrows and escape", async ({ page }) => {
	await page.goto("/search?q=images&categories=images");
	await page.getByRole("button", { name: "Red panda" }).click();
	await expect(page.getByRole("dialog")).toBeVisible();
	await page.keyboard.press("ArrowRight");
	await expect(page.getByText("2 / 2")).toBeVisible();
	await page.getByRole("button", { name: "Previous image" }).focus();
	await page.keyboard.press("Enter");
	await expect(page.getByRole("dialog")).toBeVisible();
	await page.keyboard.press("Escape");
	await expect(page.getByRole("dialog")).toHaveCount(0);
	await expect(page.getByRole("button", { name: "Red panda" })).toBeFocused();
});

test("map without coordinates keeps the list", async ({ page }) => {
	await page.goto("/search?q=map&categories=map");
	await expect(page.getByText(/don’t include coordinates/)).toBeVisible();
	await expect(page.getByRole("link", { name: /No coordinates/ })).toBeVisible();
});

test("a late response does not replace the newer query", async ({ page }) => {
	await page.goto("/search?q=slow");
	await page.goto("/search?q=fast");
	await expect(page.getByRole("link", { name: /Rust docs/ })).toBeVisible();
	await expect(page.getByText("slow", { exact: false })).toHaveCount(0);
});

test("numbered next loads the following page", async ({ page }) => {
	await page.route("**/config", (route) =>
		route.fulfill({
			status: 200,
			contentType: "application/json",
			body: JSON.stringify(e2eConfig(false)),
		}),
	);
	await page.goto("/search?q=pages");
	await expect(page.getByRole("link", { name: "Alpha" })).toBeVisible();
	await page.getByRole("link", { name: "Next ›" }).click();
	await expect(page).toHaveURL(/pageno=2/);
	await expect(page.getByRole("link", { name: "Alpha" })).toBeVisible();
	await page.getByRole("link", { name: "Next ›" }).click();
	await expect(page).toHaveURL(/pageno=3/);
	await expect(page.getByRole("link", { name: "Beta" })).toBeVisible();
});

test("back restores scroll and a loaded page", async ({ page }) => {
	await page.goto("/search?q=long");
	await expect(page.getByRole("link", { name: /› 0 Long result 1$/ })).toBeVisible();
	await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
	await expect(page.getByRole("link", { name: "Gamma" })).toBeVisible();
	const scrolled = await page.evaluate(() => window.scrollY);
	expect(scrolled).toBeGreaterThan(200);
	await page.getByPlaceholder("Search…").fill("rust");
	await page.locator("form").evaluate((form: HTMLFormElement) => form.requestSubmit());
	await expect(page.getByRole("link", { name: /Rust docs/ })).toBeVisible();
	await page.goBack();
	await expect(page.getByRole("link", { name: "Gamma" })).toBeVisible();
	await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(200);
});

test("hide and prefer survive a reload", async ({ page }) => {
	await page.goto("/search?q=rust");
	await page.getByText("web").click();
	await page.getByRole("button", { name: "Hide docs.example.com" }).click();
	await page.reload();
	await expect(page.getByText("Hidden sites removed every result on this page.")).toBeVisible();
	await page.evaluate(() => localStorage.removeItem("zoeken.source-prefs.v1"));
	await page.reload();
	await page.getByText("web").click();
	await page.getByRole("button", { name: "Prefer docs.example.com" }).click();
	await page.reload();
	const stored = await page.evaluate(() => localStorage.getItem("zoeken.source-prefs.v1"));
	expect(stored).toContain("docs.example.com");
	await expect(page.getByRole("link", { name: /Rust docs/ })).toBeVisible();
});

test("a time filter drops the engine that cannot honor it", async ({ page }) => {
	await page.goto("/search?q=rust");
	await page.getByRole("button", { name: "Time range" }).first().click();
	await page.getByRole("option", { name: "Past week" }).click();
	await expect(page).toHaveURL(/time_range=week/);
	await expect(page.getByText(/can apply the active filters/)).toBeVisible();
	await expect(page.getByText(/1 of 2/)).toBeVisible();
});

test("the more menu opens a secondary category", async ({ page }) => {
	await page.goto("/search?q=rust");
	await page.getByText("More", { exact: true }).click();
	await page.getByRole("link", { name: "science" }).click();
	await expect(page).toHaveURL(/categories=science/);
});

test("a newer search replaces a slow one in the same page", async ({ page }) => {
	await page.goto("/search?q=rust");
	const box = page.getByPlaceholder("Search…");
	await box.fill("slow");
	await page.locator("form").evaluate((form: HTMLFormElement) => form.requestSubmit());
	await box.fill("fast");
	await page.locator("form").evaluate((form: HTMLFormElement) => form.requestSubmit());
	await expect(page).toHaveURL(/q=fast/);
	await expect(page.getByText("fast", { exact: true })).toBeVisible();
	await page.waitForTimeout(1000);
	await expect(page.getByText("slow", { exact: true })).toHaveCount(0);
});

test("mobile dark layout keeps the search box", async ({ page }) => {
	await page.setViewportSize({ width: 390, height: 800 });
	await page.emulateMedia({ colorScheme: "dark" });
	await page.goto("/search?q=rust");
	const box = page.getByPlaceholder("Search…");
	await expect(box).toBeVisible();
	const boxWidth = await box.evaluate((node) => node.getBoundingClientRect().width);
	expect(boxWidth).toBeGreaterThan(120);
	expect(boxWidth).toBeLessThan(390);
});

test("a suggestion does not replace the typed query", async ({ page }) => {
	await page.route("**/autocompleter**", (route) =>
		route.fulfill({
			status: 200,
			contentType: "application/json",
			body: JSON.stringify([{ text: "python", subtext: "" }]),
		}),
	);
	await page.goto("/");
	const box = page.getByPlaceholder("Search…");
	await box.click();
	await box.fill("pythn");
	await expect(page.getByRole("option", { name: "python" })).toBeVisible();
	await expect(box).toHaveValue("pythn");
	await page.keyboard.press("Enter");
	await expect(page).toHaveURL(/q=pythn/);
});

test("a failed suggestion provider leaves the typed query", async ({ page }) => {
	await page.route("**/autocompleter**", (route) => route.abort());
	await page.goto("/");
	const box = page.getByPlaceholder("Search…");
	await box.click();
	await box.fill("pythn");
	await page.keyboard.press("Enter");
	await expect(page).toHaveURL(/q=pythn/);
});

test("preferred sites can be removed from preferences", async ({ page }) => {
	await page.goto("/search?q=rust");
	await page.getByText("web").click();
	await page.getByRole("button", { name: "Prefer docs.example.com" }).click();
	await page.goto("/preferences");
	await expect(page.getByText("Preferred · docs.example.com")).toBeVisible();
	await page.getByRole("button", { name: "Remove" }).click();
	await expect(page.getByText("No personal site rules yet.")).toBeVisible();
});

test("a paper row expands and can be hidden", async ({ page }) => {
	await page.goto("/search?q=paper");
	await expect(page.getByRole("link", { name: /papers\.example/ })).toBeVisible();
	await page.getByRole("button", { name: "Show more" }).click();
	await expect(page.getByText(/café/)).toBeVisible();
	await page.getByRole("button", { name: "Hide papers.example" }).click();
	await expect(page.getByText("Hidden sites removed every result on this page.")).toBeVisible();
});

test("a mapped result stays in the list", async ({ page }) => {
	await page.goto("/search?q=placed&categories=map");
	await expect(page.getByRole("img", { name: "Map of search results" })).toBeVisible();
	await expect(page.getByRole("link", { name: /Library/ })).toBeVisible();
});

test("the answer and the first result share a left edge", async ({ page }) => {
	const cases = [
		{ width: 1440, height: 900, scheme: "light" as const },
		{ width: 1440, height: 900, scheme: "dark" as const },
		{ width: 390, height: 800, scheme: "light" as const },
		{ width: 390, height: 800, scheme: "dark" as const },
	];
	for (const item of cases) {
		await page.setViewportSize({ width: item.width, height: item.height });
		await page.emulateMedia({ colorScheme: item.scheme });
		await page.goto("/search?q=2%2B2");
		const answer = page.locator(".zoeken-answer").first();
		const result = page.locator("a[data-result-link]").first();
		await expect(answer).toBeVisible();
		await expect(result).toBeVisible();
		const answerBox = await answer.boundingBox();
		const resultBox = await result.boundingBox();
		if (!answerBox || !resultBox) throw new Error("missing box");
		expect(Math.abs(answerBox.x - resultBox.x)).toBeLessThan(2);
		expect(Math.abs(answerBox.width - resultBox.width)).toBeLessThan(2);
		await page.screenshot({
			path: `test-results/align-${item.width}-${item.scheme}.png`,
		});
	}
});
