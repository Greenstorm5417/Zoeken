import { describe, expect, it } from "vitest";
import type { SearchResult } from "../api";
import {
	applyTrackerUrlRemover,
	stripTrackerParams,
} from "./trackerUrlRemover";

function result(
	url: string,
	overrides: Partial<SearchResult> = {},
): SearchResult {
	return { url, title: "", ...overrides };
}

describe("stripTrackerParams", () => {
	it("strips global utm_source while keeping other params", () => {
		const out = stripTrackerParams("https://example.com/a?utm_source=x&keep=1");
		expect(out).not.toContain("utm_source");
		expect(out).toContain("keep=1");
	});

	it("strips fbclid", () => {
		expect(stripTrackerParams("https://example.com/?fbclid=abc&q=1")).toBe(
			"https://example.com/?q=1",
		);
	});

	it("returns the original string when the URL is unparseable", () => {
		expect(stripTrackerParams("not a url")).toBe("not a url");
	});

	it("leaves URLs without a query string alone", () => {
		expect(stripTrackerParams("https://example.com/path")).toBe(
			"https://example.com/path",
		);
	});
});

describe("applyTrackerUrlRemover", () => {
	it("cleans url, img_src, and thumbnail", () => {
		const out = applyTrackerUrlRemover([
			result("https://example.com/?utm_source=a", {
				img_src: "https://cdn.example.com/i.png?fbclid=1",
				thumbnail: "https://cdn.example.com/t.png?gclid=2",
			}),
		]);
		expect(out[0].url).toBe("https://example.com/");
		expect(out[0].img_src).toBe("https://cdn.example.com/i.png");
		expect(out[0].thumbnail).toBe("https://cdn.example.com/t.png");
	});
});
