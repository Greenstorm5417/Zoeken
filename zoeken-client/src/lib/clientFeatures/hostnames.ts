/** Rewrite/remove result URLs and adjust display priority by hostname rules from `/config`. */
import type { SearchResult } from "../api";

export type HostnamesRules = {
	replace: Record<string, string>;
	remove: string[];
	high_priority: string[];
	low_priority: string[];
};

function hasRules(rules: HostnamesRules): boolean {
	return (
		Object.keys(rules.replace).length > 0 ||
		rules.remove.length > 0 ||
		rules.high_priority.length > 0 ||
		rules.low_priority.length > 0
	);
}

function safeRegex(pattern: string): RegExp | null {
	try {
		return new RegExp(pattern, "i");
	} catch {
		return null;
	}
}

function anyMatch(host: string, patterns: string[]): boolean {
	return patterns.some((pattern) => safeRegex(pattern)?.test(host));
}

function hostOf(url: string): string | null {
	try {
		return new URL(url).hostname.toLowerCase();
	} catch {
		return null;
	}
}

function rewriteHost(
	url: string,
	pattern: string,
	replacement: string,
): string | null {
	const regex = safeRegex(pattern);
	if (!regex) return null;
	let parsed: URL;
	try {
		parsed = new URL(url);
	} catch {
		return null;
	}
	if (!regex.test(parsed.hostname)) return null;
	parsed.hostname = parsed.hostname.replace(regex, replacement);
	return parsed.toString();
}

function filterUrl(value: string, rules: HostnamesRules): string {
	if (!value) return value;
	const host = hostOf(value);
	if (host && anyMatch(host, rules.remove)) return "";
	let current = value;
	for (const [pattern, replacement] of Object.entries(rules.replace)) {
		const rewritten = rewriteHost(current, pattern, replacement);
		if (rewritten) current = rewritten;
	}
	return current;
}

function applyUrlRules(
	result: SearchResult,
	rules: HostnamesRules,
): SearchResult {
	const url = filterUrl(result.url, rules);
	switch (result.kind) {
		case "main":
			return {
				...result,
				url,
				thumbnail: filterUrl(result.thumbnail, rules),
			};
		case "image":
			return {
				...result,
				url,
				img_src: filterUrl(result.img_src, rules),
				thumbnail_src: filterUrl(result.thumbnail_src, rules),
			};
		case "paper":
			return {
				...result,
				url,
				pdf_url: filterUrl(result.pdf_url, rules),
				html_url: filterUrl(result.html_url, rules),
			};
		case "file":
			return {
				...result,
				url,
				magnetlink: filterUrl(result.magnetlink, rules),
			};
		default:
			return { ...result, url };
	}
}

export type PrioritizedResult = {
	result: SearchResult;
	priority: "high" | "low" | "normal";
};

/**
 * Apply hostname replace/remove and tag high/low priority. Only results
 * whose host is present and matches `remove` are dropped; results with no
 * parseable host pass through untouched.
 */
export function applyHostnames(
	results: SearchResult[],
	rules: HostnamesRules | undefined,
): PrioritizedResult[] {
	if (!rules || !hasRules(rules)) {
		return results.map((result) => ({ result, priority: "normal" }));
	}
	const out: PrioritizedResult[] = [];
	for (const result of results) {
		const host = result.url ? hostOf(result.url) : null;
		if (!host) {
			out.push({ result, priority: "normal" });
			continue;
		}
		if (anyMatch(host, rules.remove)) continue;

		const next = applyUrlRules(result, rules);

		let priority: PrioritizedResult["priority"] = "normal";
		if (anyMatch(host, rules.low_priority)) priority = "low";
		if (anyMatch(host, rules.high_priority)) priority = "high";
		out.push({ result: next, priority });
	}
	return out;
}

/** Stable-sort so high-priority results float up and low-priority sink down. */
export function sortByPriority(entries: PrioritizedResult[]): SearchResult[] {
	const rank = { high: 0, normal: 1, low: 2 } as const;
	return entries
		.map((entry, index) => ({ entry, index }))
		.sort(
			(a, b) =>
				rank[a.entry.priority] - rank[b.entry.priority] || a.index - b.index,
		)
		.map(({ entry }) => entry.result);
}
