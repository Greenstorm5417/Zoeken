import { describe, expect, it } from "vitest";
import type { SearchResult } from "../api";
import { mainResult, paperResult } from "./fixtures";
import {
	applyHostnames,
	type HostnamesRules,
	sortByPriority,
} from "./hostnames";

function result(url: string): SearchResult {
	return mainResult({ url });
}

function imageResult(
	url: string,
	img_src: string,
	thumbnail_src: string,
): SearchResult {
	return {
		kind: "image",
		url,
		title: "",
		content: "",
		engine: "",
		engines: [],
		score: 0,
		positions: [],
		priority: "",
		img_src,
		thumbnail_src,
		resolution: "",
		img_format: "",
		source: "",
		filesize: "",
	};
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
		const input = result("not a url");
		const out = applyHostnames([input], rules({ remove: [".*"] }));
		expect(out).toEqual([{ result: input, priority: "normal" }]);
	});

	it("rewrites hostnames per the replace map", () => {
		const out = applyHostnames(
			[result("https://old.example.com/a")],
			rules({ replace: { "^old\\.example\\.com$": "new.example.com" } }),
		);
		expect(out[0].result.url).toBe("https://new.example.com/a");
	});

	it("rewrites img_src and thumbnail_src alongside url", () => {
		const out = applyHostnames(
			[
				imageResult(
					"https://old.example.com/a",
					"https://old.example.com/i.png",
					"https://old.example.com/t.png",
				),
			],
			rules({ replace: { "^old\\.example\\.com$": "new.example.com" } }),
		);
		const r = out[0].result;
		expect(r.kind).toBe("image");
		if (r.kind === "image") {
			expect(r.img_src).toBe("https://new.example.com/i.png");
			expect(r.thumbnail_src).toBe("https://new.example.com/t.png");
		}
	});

	it("rewrites paper pdf_url and html_url alongside url", () => {
		const out = applyHostnames(
			[
				paperResult({
					url: "https://old.example.com/abs",
					pdf_url: "https://old.example.com/paper.pdf",
					html_url: "https://old.example.com/html",
				}),
			],
			rules({ replace: { "^old\\.example\\.com$": "new.example.com" } }),
		);
		const r = out[0].result;
		expect(r.kind).toBe("paper");
		if (r.kind === "paper") {
			expect(r.url).toBe("https://new.example.com/abs");
			expect(r.pdf_url).toBe("https://new.example.com/paper.pdf");
			expect(r.html_url).toBe("https://new.example.com/html");
		}
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
