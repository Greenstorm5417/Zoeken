/** Strip ClearURLs-style tracker query params from result URLs. */
import type { SearchResult } from "../api";
import trackerPatterns from "../generated/tracker_patterns.json" with {
	type: "json",
};

type TrackerRule = {
	url: string;
	exceptions: string[];
	rules: string[];
};

type CompiledRule = {
	url: RegExp;
	exceptions: RegExp[];
	params: RegExp[];
};

const PATTERNS = trackerPatterns as TrackerRule[];
let compiled: CompiledRule[] | null = null;

function compileRules(): CompiledRule[] {
	if (compiled) return compiled;
	compiled = PATTERNS.flatMap((rule) => {
		try {
			return [
				{
					url: new RegExp(rule.url),
					exceptions: rule.exceptions.flatMap((pattern) => {
						try {
							return [new RegExp(pattern)];
						} catch {
							return [];
						}
					}),
					params: rule.rules.flatMap((pattern) => {
						try {
							return [new RegExp(`^${pattern}$`)];
						} catch {
							return [];
						}
					}),
				},
			];
		} catch {
			return [];
		}
	});
	return compiled;
}

/** Remove matching tracker query args from a single URL. */
export function stripTrackerParams(
	raw: string,
	rules: CompiledRule[] = compileRules(),
): string {
	let parsed: URL;
	try {
		parsed = new URL(raw);
	} catch {
		return raw;
	}
	if (!parsed.search) return raw;

	let current = raw;
	for (const rule of rules) {
		if (!parsed.search) break;
		if (!rule.url.test(current)) continue;
		if (rule.exceptions.some((re) => re.test(current))) continue;

		const kept = [...parsed.searchParams].filter(
			([name]) => !rule.params.some((re) => re.test(name)),
		);
		parsed.search = "";
		for (const [name, value] of kept) {
			parsed.searchParams.append(name, value);
		}
		current = parsed.toString();
	}
	return current;
}

function cleanField(
	result: SearchResult,
	field: "url" | "img_src" | "thumbnail",
	rules: CompiledRule[],
): void {
	const value = result[field];
	if (!value) return;
	result[field] = stripTrackerParams(value, rules);
}

/** Strip trackers from `url` / `img_src` / `thumbnail` on each result. */
export function applyTrackerUrlRemover(
	results: SearchResult[],
): SearchResult[] {
	const rules = compileRules();
	return results.map((result) => {
		const next = { ...result };
		cleanField(next, "url", rules);
		cleanField(next, "img_src", rules);
		cleanField(next, "thumbnail", rules);
		return next;
	});
}
