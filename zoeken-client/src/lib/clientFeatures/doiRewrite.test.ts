import { describe, expect, it } from "vitest";
import type { SearchResult } from "../api";
import { applyDoiRewrite, extractDoi } from "./doiRewrite";

function result(overrides: Partial<SearchResult> = {}): SearchResult {
	return { url: "", title: "", ...overrides };
}

describe("extractDoi", () => {
	it("extracts a DOI from the URL path", () => {
		expect(extractDoi("https://publisher.test/10.1234/abc.def")).toBe(
			"10.1234/abc.def",
		);
	});

	it("strips trailing suffixes like /full and .pdf", () => {
		expect(extractDoi("https://publisher.test/10.1234/abc/full")).toBe(
			"10.1234/abc",
		);
		expect(extractDoi("https://publisher.test/10.1234/abc.pdf")).toBe(
			"10.1234/abc",
		);
	});

	it("falls back to a query parameter value", () => {
		expect(extractDoi("https://publisher.test/view?doi=10.5555/xyz")).toBe(
			"10.5555/xyz",
		);
	});

	it("returns null when there's no DOI", () => {
		expect(extractDoi("https://example.test/article")).toBeNull();
	});

	it("returns null for unparseable URLs", () => {
		expect(extractDoi("not a url")).toBeNull();
	});
});

describe("applyDoiRewrite", () => {
	it("rewrites the URL to the resolver + DOI", () => {
		const input = result({ url: "https://publisher.test/10.1234/abc" });
		const out = applyDoiRewrite(input, "https://oadoi.org/");
		expect(out.url).toBe("https://oadoi.org/10.1234/abc");
	});

	it("sets result.doi for paper-template results without one", () => {
		const input = result({
			url: "https://publisher.test/10.1234/abc",
			template: "paper.html",
		});
		const out = applyDoiRewrite(input, "https://oadoi.org/");
		expect(out.doi).toBe("10.1234/abc");
	});

	it("does not overwrite an existing doi field", () => {
		const input = result({
			url: "https://publisher.test/10.1234/abc",
			template: "paper.html",
			doi: "10.9999/keep",
		});
		const out = applyDoiRewrite(input, "https://oadoi.org/");
		expect(out.doi).toBe("10.9999/keep");
	});

	it("leaves non-paper results' doi field untouched", () => {
		const input = result({ url: "https://publisher.test/10.1234/abc" });
		const out = applyDoiRewrite(input, "https://oadoi.org/");
		expect(out.doi).toBeUndefined();
	});

	it("passes through when no DOI is found", () => {
		const input = result({ url: "https://example.test/article" });
		expect(applyDoiRewrite(input, "https://oadoi.org/")).toEqual(input);
	});

	it("passes through overly long DOIs unchanged", () => {
		const longDoi = `10.1234/${"a".repeat(60)}`;
		const input = result({ url: `https://publisher.test/${longDoi}` });
		expect(applyDoiRewrite(input, "https://oadoi.org/")).toEqual(input);
	});
});
