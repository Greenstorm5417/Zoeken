import { describe, expect, it } from "vitest";
import { specializedTemplate } from "#/components/ResultTemplates";
import type { SearchResult } from "#/lib/api";
import { mainResult, paperResult } from "#/lib/clientFeatures/fixtures";

function fileResult(
	overrides: Partial<Extract<SearchResult, { kind: "file" }>> = {},
): SearchResult {
	return {
		kind: "file",
		url: "https://example.test/t",
		title: "T",
		content: "",
		engine: "piratebay",
		engines: ["piratebay"],
		score: 1,
		positions: [1],
		priority: "",
		filename: "T",
		size: "1 GiB",
		time: "2024-01-02",
		mimetype: "application/x-bittorrent",
		abstract: "",
		author: "uploader",
		embedded: "",
		mtype: "",
		subtype: "",
		filesize: "1 GiB",
		seed: 1,
		leech: 0,
		magnetlink: "magnet:?xt=urn:btih:abc",
		...overrides,
	};
}

function codeResult(): SearchResult {
	return {
		kind: "code",
		url: "https://github.com/a/b",
		title: "main",
		content: "",
		engine: "github_code",
		engines: ["github_code"],
		score: 1,
		positions: [1],
		priority: "",
		repository: "a/b",
		filename: "main.rs",
		code_language: "rust",
		codelines: [[1, "fn main() {}"]],
		hl_lines: [1],
	};
}

function keyValueResult(): SearchResult {
	return {
		kind: "key_value",
		url: "",
		title: "pkg",
		content: "",
		engine: "crates",
		engines: ["crates"],
		score: 1,
		positions: [1],
		priority: "",
		caption: "Meta",
		key_title: "K",
		value_title: "V",
		kvmap: [["license", "MIT"]],
	};
}

describe("specializedTemplate by kind", () => {
	it("routes each native kind", () => {
		expect(specializedTemplate(fileResult())?.name).toBe("TorrentResult");
		expect(specializedTemplate(paperResult())?.name).toBe("PaperResult");
		expect(specializedTemplate(codeResult())?.name).toBe("CodeResult");
		expect(specializedTemplate(keyValueResult())?.name).toBe("KeyValueResult");
		expect(
			specializedTemplate(mainResult({ category: "shopping" }))?.name,
		).toBe("ProductResult");
		expect(specializedTemplate(mainResult(), "shopping")?.name).toBe(
			"ProductResult",
		);
		expect(specializedTemplate(mainResult())).toBeNull();
	});

	it("paper fixtures expose restored citation fields", () => {
		const paper = paperResult({
			type: "preprint",
			volume: "1",
			pages: "1-10",
			number: "2",
			editor: "Ed",
			issn: ["1234-5678"],
			isbn: ["978-0"],
			comments: "15 pages",
			published_date: "2017-06-12T00:00:00Z",
		});
		expect(paper.kind).toBe("paper");
		if (paper.kind !== "paper") return;
		expect(paper.type).toBe("preprint");
		expect(paper.volume).toBe("1");
		expect(paper.pages).toBe("1-10");
		expect(paper.issn).toEqual(["1234-5678"]);
		expect(paper.comments).toBe("15 pages");
	});

	it("file fixtures expose time and author", () => {
		const file = fileResult();
		expect(file.kind).toBe("file");
		if (file.kind !== "file") return;
		expect(file.time).toBe("2024-01-02");
		expect(file.author).toBe("uploader");
	});
});
