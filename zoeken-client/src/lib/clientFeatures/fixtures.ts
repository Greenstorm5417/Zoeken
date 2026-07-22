/** Shared native result fixtures for client-feature tests. */
import type { SearchResult } from "../api";

export function mainResult(
	overrides: Partial<Extract<SearchResult, { kind: "main" }>> & {
		url?: string;
	} = {},
): SearchResult {
	return {
		kind: "main",
		url: overrides.url ?? "https://example.test/",
		title: overrides.title ?? "",
		content: overrides.content ?? "",
		engine: overrides.engine ?? "",
		engines: overrides.engines ?? [],
		category: overrides.category ?? "general",
		score: overrides.score ?? 0,
		positions: overrides.positions ?? [],
		priority: overrides.priority ?? "",
		thumbnail: overrides.thumbnail ?? "",
		iframe_src: overrides.iframe_src ?? "",
		favicon: overrides.favicon ?? "",
		pretty_url: overrides.pretty_url ?? "",
		published_date: overrides.published_date ?? null,
		length: overrides.length ?? "",
		author: overrides.author ?? "",
	};
}

export function paperResult(
	overrides: Partial<Extract<SearchResult, { kind: "paper" }>> & {
		url?: string;
	} = {},
): SearchResult {
	return {
		kind: "paper",
		url: overrides.url ?? "https://example.test/paper",
		title: overrides.title ?? "",
		content: overrides.content ?? "",
		engine: overrides.engine ?? "",
		engines: overrides.engines ?? [],
		score: overrides.score ?? 0,
		positions: overrides.positions ?? [],
		priority: overrides.priority ?? "",
		authors: overrides.authors ?? [],
		doi: overrides.doi ?? "",
		journal: overrides.journal ?? "",
		published_date: overrides.published_date ?? null,
		publisher: overrides.publisher ?? "",
		editor: overrides.editor ?? "",
		volume: overrides.volume ?? "",
		pages: overrides.pages ?? "",
		number: overrides.number ?? "",
		type: overrides.type ?? "",
		tags: overrides.tags ?? [],
		issn: overrides.issn ?? [],
		isbn: overrides.isbn ?? [],
		pdf_url: overrides.pdf_url ?? "",
		html_url: overrides.html_url ?? "",
		comments: overrides.comments ?? "",
	};
}
