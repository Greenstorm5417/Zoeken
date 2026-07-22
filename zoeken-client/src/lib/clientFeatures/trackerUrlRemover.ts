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

function clean(value: string, rules: CompiledRule[]): string {
	return value ? stripTrackerParams(value, rules) : value;
}

/** Strip trackers from `url` / image fields on each result. */
export function applyTrackerUrlRemover(
	results: SearchResult[],
): SearchResult[] {
	const rules = compileRules();
	return results.map((result) => {
		const url = clean(result.url, rules);
		switch (result.kind) {
			case "main":
				return {
					...result,
					url,
					thumbnail: clean(result.thumbnail, rules),
				};
			case "image":
				return {
					...result,
					url,
					img_src: clean(result.img_src, rules),
					thumbnail_src: clean(result.thumbnail_src, rules),
				};
			case "paper":
				return {
					...result,
					url,
					pdf_url: clean(result.pdf_url, rules),
					html_url: clean(result.html_url, rules),
				};
			case "file":
				return {
					...result,
					url,
					magnetlink: clean(result.magnetlink, rules),
				};
			default:
				return { ...result, url };
		}
	});
}
