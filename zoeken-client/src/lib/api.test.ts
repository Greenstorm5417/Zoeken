import { afterEach, describe, expect, it, vi } from "vitest";
import { clearCookies, type Preferences, preferencesPost } from "./api";

const originalFetch = globalThis.fetch;

afterEach(() => {
	globalThis.fetch = originalFetch;
});

function stubFetch(mock: ReturnType<typeof vi.fn>) {
	globalThis.fetch = mock as unknown as typeof globalThis.fetch;
}

describe("API client", () => {
	it("posts all preference fields and plugin choices", async () => {
		const fetch = vi
			.fn()
			.mockResolvedValue(new Response(JSON.stringify({}), { status: 200 }));
		stubFetch(fetch);
		const preferences: Preferences = {
			locale: "en-US",
			language: "en",
			categories: ["general", "it"],
			engines: ["duckduckgo"],
			safesearch: "Strict",
			autocomplete: "duckduckgo",
			image_proxy: true,
			method: "POST",
			plugins: { calculator: true },
		};

		await preferencesPost(preferences);
		const init = fetch.mock.calls[0]?.[1] as RequestInit;
		const body = init.body as URLSearchParams;
		expect(init.method).toBe("POST");
		expect(body.get("safesearch")).toBe("2");
		expect(body.get("plugin_calculator")).toBe("1");
	});

	it("clears cookies through the redirecting GET route", async () => {
		const fetch = vi
			.fn()
			.mockResolvedValue(new Response(null, { status: 200 }));
		stubFetch(fetch);
		await clearCookies();
		expect(fetch).toHaveBeenCalledWith("/clear_cookies", {
			method: "GET",
			credentials: "same-origin",
		});
	});
});
