import { encode } from "@msgpack/msgpack";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
	clearCookies,
	type Preferences,
	preferencesPost,
	type SearchResponse,
	search,
} from "./api";

const originalFetch = globalThis.fetch;

afterEach(() => {
	globalThis.fetch = originalFetch;
});

function stubFetch(mock: ReturnType<typeof vi.fn>) {
	globalThis.fetch = mock as unknown as typeof globalThis.fetch;
}

function emptyNativeResponse(
	overrides: Partial<SearchResponse> = {},
): SearchResponse {
	return {
		schema_version: 1,
		query: "rust",
		number_of_results: 0,
		results: [],
		answers: [],
		corrections: [],
		infoboxes: [],
		suggestions: [],
		unresponsive_engines: [],
		engine_data: {},
		...overrides,
	};
}

describe("API client", () => {
	it("posts JSON body and decodes msgpack native search", async () => {
		const payload = emptyNativeResponse({
			query: "rust search",
			number_of_results: 1,
			results: [
				{
					kind: "main",
					url: "https://www.rust-lang.org/",
					title: "Rust",
					content: "A language",
					engine: "duckduckgo",
					engines: ["duckduckgo"],
					category: "general",
					score: 1.2,
					positions: [1],
					priority: "",
					thumbnail: "",
					iframe_src: "",
					favicon: "/favicon_proxy?authority=www.rust-lang.org",
					pretty_url: "www.rust-lang.org",
					published_date: null,
				},
			],
		});
		const fetch = vi.fn().mockResolvedValue(
			new Response(encode(payload), {
				status: 200,
				headers: { "Content-Type": "application/msgpack" },
			}),
		);
		stubFetch(fetch);

		const response = await search({
			q: "rust search",
			pageno: 2,
			categories: "it",
			safesearch: 2,
		});

		expect(fetch).toHaveBeenCalledWith("/api/v1/search", expect.any(Object));
		const init = fetch.mock.calls[0]?.[1] as RequestInit;
		expect(init.method).toBe("POST");
		expect(init.headers).toMatchObject({
			Accept: "application/msgpack",
			"Content-Type": "application/json",
		});
		expect(JSON.parse(String(init.body))).toEqual({
			q: "rust search",
			pageno: 2,
			language: null,
			safesearch: 2,
			categories: "it",
			time_range: null,
			engines: null,
		});
		expect(response.schema_version).toBe(1);
		expect(response.query).toBe("rust search");
		expect(response.results[0]?.kind).toBe("main");
		if (response.results[0]?.kind === "main") {
			expect(response.results[0].title).toBe("Rust");
		}
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
