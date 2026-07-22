/** Small presentational helpers shared by the search route's result views. */
import type { NativeCorrection, NativeSuggestion, SearchResult } from "./api";
import type { SearchRouteParams } from "./searchParams";
import { serializeSearchParams } from "./searchParams";

/** Align with `categories_as_tabs` in default.config.yml when engines exist. */
export const DEFAULT_CATEGORIES = [
	"general",
	"images",
	"videos",
	"news",
	"map",
	"science",
	"it",
	"files",
	"music",
	"shopping",
] as const;

export function suggestionText(s: NativeSuggestion) {
	return s.suggestion;
}

export function correctionText(c: NativeCorrection) {
	return c.correction;
}

export function hostnameOf(url: string) {
	try {
		return new URL(url).hostname.replace(/^www\./, "");
	} catch {
		return url;
	}
}

export function pathOf(url: string) {
	try {
		const u = new URL(url);
		const path = u.pathname.replace(/\/$/, "") || "";
		return path === "/" ? "" : path;
	} catch {
		return "";
	}
}

export function engineNames(result: SearchResult): string[] {
	const names =
		result.engines && result.engines.length > 0
			? result.engines
			: result.engine
				? [result.engine]
				: [];
	return [...new Set(names.filter(Boolean))];
}

export function formatEngineLabel(name: string) {
	return name.replace(/[_-]+/g, " ");
}

export function wikidataId(id: string | null | undefined): string | null {
	if (!id) return null;
	const match = id.match(/\/(Q\d+)\s*$/i) || id.match(/^(Q\d+)$/i);
	return match ? match[1].toUpperCase() : null;
}

export function searchLink(
	search: SearchRouteParams,
	updates: Partial<SearchRouteParams>,
) {
	return serializeSearchParams({ ...search, ...updates });
}

/** Sliding window of page numbers (SearXNG-style): 1–10, then centered on current. */
export function pageNumbers(pageno: number): number[] {
	const start = pageno > 5 ? pageno - 4 : 1;
	return Array.from({ length: 10 }, (_, i) => start + i);
}
