import { describe, expect, it } from "vitest";
import type { SearchResult } from "../api";
import {
	applyHostnames,
	type HostnamesRules,
	sortByPriority,
} from "./hostnames";

function result(
	url: string,
	overrides: Partial<SearchResult> = {},
): SearchResult {
	return { url, title: "", ...overrides };
}

function rules(overrides: Partial<HostnamesRules> = {}): HostnamesRules {
	return {
		replace: {},
		remove: [],
		high_priority: [],
		low_priority: [],
		...overrides,
	};
}

describe("applyHostnames", () => {
	it("passes results through untouched when there are no rules", () => {
		const results = [result("https://example.com/a")];
		const out = applyHostnames(results, rules());
		expect(out).toEqual([{ result: results[0], priority: "normal" }]);
	});

	it("passes results through untouched when rules are undefined", () => {
		const results = [result("https://example.com/a")];
		expect(applyHostnames(results, undefined)).toEqual([
			{ result: results[0], priority: "normal" },
		]);
	});

	it("drops results whose host matches a remove pattern", () => {
		const out = applyHostnames(
			[
				result("https://spam.example.com/a"),
				result("https://good.example.com/b"),
			],
			rules({ remove: ["^spam\\."] }),
		);
		expect(out).toHaveLength(1);
		expect(out[0].result.url).toBe("https://good.example.com/b");
	});

	it("keeps results with no parseable host untouched", () => {
		const out = applyHostnames(
			[result("not a url")],
			rules({ remove: [".*"] }),
		);
		expect(out).toEqual([
			{ result: { url: "not a url", title: "" }, priority: "normal" },
		]);
	});

	it("rewrites hostnames per the replace map", () => {
		const out = applyHostnames(
			[result("https://old.example.com/a")],
			rules({ replace: { "^old\\.example\\.com$": "new.example.com" } }),
		);
		expect(out[0].result.url).toBe("https://new.example.com/a");
	});

	it("rewrites img_src and thumbnail alongside url", () => {
		const out = applyHostnames(
			[
				result("https://old.example.com/a", {
					img_src: "https://old.example.com/i.png",
					thumbnail: "https://old.example.com/t.png",
				}),
			],
			rules({ replace: { "^old\\.example\\.com$": "new.example.com" } }),
		);
		expect(out[0].result.img_src).toBe("https://new.example.com/i.png");
		expect(out[0].result.thumbnail).toBe("https://new.example.com/t.png");
	});

	it("tags high and low priority by host pattern", () => {
		const out = applyHostnames(
			[
				result("https://good.example.com/a"),
				result("https://bad.example.com/b"),
			],
			rules({ high_priority: ["^good\\."], low_priority: ["^bad\\."] }),
		);
		expect(out.find((e) => e.result.url.includes("good"))?.priority).toBe(
			"high",
		);
		expect(out.find((e) => e.result.url.includes("bad"))?.priority).toBe("low");
	});
});

describe("sortByPriority", () => {
	it("floats high priority up and sinks low priority down, preserving order within groups", () => {
		const a = result("https://a.test/");
		const b = result("https://b.test/");
		const c = result("https://c.test/");
		const d = result("https://d.test/");
		const sorted = sortByPriority([
			{ result: a, priority: "normal" },
			{ result: b, priority: "low" },
			{ result: c, priority: "high" },
			{ result: d, priority: "normal" },
		]);
		expect(sorted).toEqual([c, a, d, b]);
	});
});
