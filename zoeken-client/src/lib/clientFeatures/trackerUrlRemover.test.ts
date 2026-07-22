import { describe, expect, it } from "vitest";
import { mainResult, paperResult } from "./fixtures";
import {
	applyTrackerUrlRemover,
	stripTrackerParams,
} from "./trackerUrlRemover";

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
	it("cleans url and thumbnail on main results", () => {
		const out = applyTrackerUrlRemover([
			mainResult({
				url: "https://example.com/?utm_source=a",
				thumbnail: "https://cdn.example.com/t.png?gclid=2",
			}),
		]);
		expect(out[0].url).toBe("https://example.com/");
		expect(out[0].kind === "main" && out[0].thumbnail).toBe(
			"https://cdn.example.com/t.png",
		);
	});

	it("cleans img_src and thumbnail_src on image results", () => {
		const out = applyTrackerUrlRemover([
			{
				kind: "image",
				url: "https://example.com/?utm_source=a",
				title: "",
				content: "",
				engine: "",
				engines: [],
				score: 0,
				positions: [],
				priority: "",
				img_src: "https://cdn.example.com/i.png?fbclid=1",
				thumbnail_src: "https://cdn.example.com/t.png?gclid=2",
				resolution: "",
				img_format: "",
				source: "",
				filesize: "",
			},
		]);
		expect(out[0].url).toBe("https://example.com/");
		expect(out[0].kind === "image" && out[0].img_src).toBe(
			"https://cdn.example.com/i.png",
		);
		expect(out[0].kind === "image" && out[0].thumbnail_src).toBe(
			"https://cdn.example.com/t.png",
		);
	});

	it("cleans pdf_url and html_url on paper results", () => {
		const out = applyTrackerUrlRemover([
			paperResult({
				url: "https://arxiv.org/abs/1?utm_source=a",
				pdf_url: "https://arxiv.org/pdf/1?fbclid=1",
				html_url: "https://arxiv.org/html/1?gclid=2",
			}),
		]);
		expect(out[0].url).toBe("https://arxiv.org/abs/1");
		expect(out[0].kind === "paper" && out[0].pdf_url).toBe(
			"https://arxiv.org/pdf/1",
		);
		expect(out[0].kind === "paper" && out[0].html_url).toBe(
			"https://arxiv.org/html/1",
		);
	});
});
