import { afterEach, describe, expect, it, vi } from "vitest";
import { clearCookies, type Preferences, preferencesPost, search } from "./api";

const originalFetch = globalThis.fetch;

afterEach(() => {
	globalThis.fetch = originalFetch;
});

function stubFetch(mock: ReturnType<typeof vi.fn>) {
	globalThis.fetch = mock as unknown as typeof globalThis.fetch;
}

describe("API client", () => {
	it("serializes search filters", async () => {
		const fetch = vi.fn().mockResolvedValue(
			new Response(
				JSON.stringify({
					query: "rust",
					results: [],
					answers: [],
					corrections: [],
					infoboxes: [],
					suggestions: [],
					unresponsive_engines: [],
				}),
			),
		);
		stubFetch(fetch);

		await search({
			q: "rust search",
			pageno: 2,
			categories: "it",
			safesearch: 2,
		});

		expect(fetch).toHaveBeenCalledWith("/search", expect.any(Object));
		const init = fetch.mock.calls[0]?.[1] as RequestInit;
		expect(init.method).toBe("POST");
		expect(String(init.body)).toBe(
			"q=rust+search&format=json&pageno=2&safesearch=2&categories=it",
		);
	});

	it("surfaces non-success responses as ApiError", async () => {
		stubFetch(
			vi.fn().mockResolvedValue(new Response("limited", { status: 429 })),
		);
		await expect(search({ q: "rust" })).rejects.toEqual(
			expect.objectContaining({ status: 429 }),
		);
	});

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
