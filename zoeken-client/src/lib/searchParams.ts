import type { SearchParams } from "./api";

export type SearchRouteParams = Pick<
	SearchParams,
	| "q"
	| "pageno"
	| "categories"
	| "language"
	| "safesearch"
	| "time_range"
	| "engines"
>;

const safeSearchValues = new Set([0, 1, 2]);

export function parseSearchParams(
	raw: Record<string, unknown>,
): SearchRouteParams {
	const pageno = Number(raw.pageno);
	const safesearch = Number(raw.safesearch);
	return {
		q: typeof raw.q === "string" ? raw.q : "",
		...(Number.isInteger(pageno) && pageno > 0 ? { pageno } : {}),
		...(typeof raw.categories === "string" && raw.categories
			? { categories: raw.categories }
			: {}),
		...(typeof raw.language === "string" && raw.language
			? { language: raw.language }
			: {}),
		...(safeSearchValues.has(safesearch)
			? { safesearch: safesearch as 0 | 1 | 2 }
			: {}),
		...(typeof raw.time_range === "string" && raw.time_range
			? { time_range: raw.time_range }
			: {}),
		...(typeof raw.engines === "string" && raw.engines
			? { engines: raw.engines }
			: {}),
	};
}

export function serializeSearchParams(params: SearchRouteParams) {
	return Object.fromEntries(
		Object.entries(params).filter(
			([, value]) => value !== undefined && value !== "",
		),
	) as SearchRouteParams;
}
