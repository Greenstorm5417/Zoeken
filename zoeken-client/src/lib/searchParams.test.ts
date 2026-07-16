import { describe, expect, it } from "vitest";
import { parseSearchParams, serializeSearchParams } from "./searchParams";

describe("searchParams", () => {
	it("round-trips supported search parameters", () => {
		const params = {
			q: "green energy",
			pageno: 3,
			categories: "images",
			language: "en",
			safesearch: 2 as const,
			time_range: "month",
			engines: "brave,duckduckgo",
		};
		expect(parseSearchParams(serializeSearchParams(params))).toEqual(params);
	});

	it("drops malformed optional values", () => {
		expect(
			parseSearchParams({ q: "test", pageno: "0", safesearch: "4" }),
		).toEqual({ q: "test" });
	});
});
