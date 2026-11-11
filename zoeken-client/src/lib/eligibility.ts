import type { EngineInfo } from "./api";

export type FilterState = {
	category: string;
	timeRange: string;
	language: string;
	safesearch: 0 | 1 | 2;
	engines: string;
	pageno?: number;
};

export type Eligibility = {
	/** Enabled engines in this category before the active filters. */
	inCategory: number;
	/** Engines that can run with the current filters. */
	eligible: number;
	timeReduced: boolean;
	safeReduced: boolean;
	languageReduced: boolean;
	explicitSkipped: string[];
};

/**
 * Same eligibility the backend uses: category membership, then SafeSearch,
 * time range, and language. General is a category, not "every engine".
 * Explicit engine picks are kept only when they can honor the filters.
 */
export function engineEligibility(
	engines: EngineInfo[],
	filters: FilterState,
): Eligibility {
	const category = filters.category || "general";
	const named = filters.engines
		.split(",")
		.map((name) => name.trim().toLowerCase())
		.filter(Boolean);
	const inCategoryOf = (engine: EngineInfo) =>
		engine.categories.some((item) => item.toLowerCase() === category);
	const namedHit = (engine: EngineInfo) =>
		named.includes(engine.name.toLowerCase()) ||
		(engine.shortcut !== "" && named.includes(engine.shortcut.toLowerCase()));
	const inCategory = engines.filter(
		(engine) =>
			inCategoryOf(engine) &&
			(engine.enabled || (named.length > 0 && namedHit(engine))),
	);
	const pool = named.length
		? inCategory.filter(namedHit)
		: inCategory.filter((engine) => engine.enabled);
	const explicitSkipped = named.filter(
		(name) =>
			!inCategory.some(
				(engine) =>
					engine.name.toLowerCase() === name ||
					engine.shortcut.toLowerCase() === name,
			),
	);
	let eligible = pool;
	const beforeTime = eligible.length;
	if (filters.timeRange) {
		const skipped = eligible.filter((engine) => !engine.time_range_support);
		explicitSkipped.push(
			...skipped
				.map((engine) => engine.name)
				.filter((name) => named.includes(name.toLowerCase())),
		);
		eligible = eligible.filter((engine) => engine.time_range_support);
	}
	const beforeSafe = eligible.length;
	if (filters.safesearch > 0) {
		const skipped = eligible.filter((engine) => !engine.safesearch);
		explicitSkipped.push(
			...skipped
				.map((engine) => engine.name)
				.filter((name) => named.includes(name.toLowerCase())),
		);
		eligible = eligible.filter((engine) => engine.safesearch);
	}
	const beforeLang = eligible.length;
	if (
		filters.language &&
		filters.language !== "all" &&
		filters.language !== "auto"
	) {
		eligible = eligible.filter((engine) => engine.language_support);
	}
	if ((filters.pageno ?? 1) > 1) {
		eligible = eligible.filter((engine) => engine.paging);
	}
	return {
		inCategory: inCategory.length,
		eligible: eligible.length,
		timeReduced: Boolean(filters.timeRange) && eligible.length < beforeTime,
		safeReduced: filters.safesearch > 0 && eligible.length < beforeSafe,
		languageReduced: Boolean(filters.language) && eligible.length < beforeLang,
		explicitSkipped: [...new Set(explicitSkipped)],
	};
}
