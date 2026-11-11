import { describe, expect, it } from "vitest";
import type { SearchResult } from "./api";
import { applySourcePrefs, domainMatches, toggleHidden } from "./sourcePrefs";

function result(url: string): SearchResult {
	return {
		kind: "main",
		url,
		title: url,
		content: "",
		engine: "a",
		engines: ["a"],
		category: "general",
		score: 1,
		positions: [1],
		priority: "",
		thumbnail: "",
		iframe_src: "",
		favicon: "",
		pretty_url: "",
		published_date: null,
		length: "",
		author: "",
	};
}

describe("source preferences", () => {
	it("hides a domain and its subdomains, not a sibling registrable domain", () => {
		expect(domainMatches("news.example.com", "example.com")).toBe(true);
		expect(domainMatches("example.com", "news.example.com")).toBe(false);
		const applied = applySourcePrefs(
			[result("https://news.example.com/a"), result("https://other.test/")],
			{ hidden: ["example.com"], preferred: [] },
		);
		expect(applied.hiddenCount).toBe(1);
		expect(applied.results.map((item) => item.url)).toEqual([
			"https://other.test/",
		]);
	});

	it("moves preferred hosts first and undo restores the list", () => {
		const applied = applySourcePrefs(
			[result("https://a.test/"), result("https://b.test/")],
			{ hidden: [], preferred: ["b.test"] },
		);
		expect(applied.results[0]?.url).toBe("https://b.test/");
		const hidden = toggleHidden(
			{ hidden: [], preferred: ["b.test"] },
			"b.test",
		);
		expect(hidden.preferred).toEqual([]);
		expect(hidden.hidden).toEqual(["b.test"]);
		expect(toggleHidden(hidden, "b.test").hidden).toEqual([]);
	});
});
