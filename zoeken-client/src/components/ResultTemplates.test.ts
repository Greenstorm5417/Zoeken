import { describe, expect, it } from "vitest";
import { specializedTemplate } from "#/components/ResultTemplates";
import type { SearchResult } from "#/lib/api";
import { mainResult, paperResult } from "#/lib/clientFeatures/fixtures";

function fileResult(): SearchResult {
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
		author: "",
		embedded: "",
		mtype: "",
		subtype: "",
		filesize: "1 GiB",
		seed: 1,
		leech: 0,
		magnetlink: "magnet:?xt=urn:btih:abc",
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
});
