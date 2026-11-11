/**
 * Browser-local site preferences. Exact host or its subdomains.
 * `example.com` matches `example.com` and `news.example.com`.
 * `news.example.com` does not match `example.com`.
 * These never leave the browser.
 */

import type { SearchResult } from "./api";

const KEY = "zoeken.source-prefs.v1";

export type SourcePrefs = {
	hidden: string[];
	preferred: string[];
};

export type SourceAction = {
	kind: "hide" | "prefer";
	domain: string;
};

export function emptySourcePrefs(): SourcePrefs {
	return { hidden: [], preferred: [] };
}

export function loadSourcePrefs(): SourcePrefs {
	if (typeof localStorage === "undefined") return emptySourcePrefs();
	try {
		const raw = localStorage.getItem(KEY);
		if (!raw) return emptySourcePrefs();
		const parsed = JSON.parse(raw) as Partial<SourcePrefs>;
		return {
			hidden: cleanList(parsed.hidden),
			preferred: cleanList(parsed.preferred),
		};
	} catch {
		return emptySourcePrefs();
	}
}

export function saveSourcePrefs(prefs: SourcePrefs) {
	if (typeof localStorage === "undefined") return;
	localStorage.setItem(
		KEY,
		JSON.stringify({
			hidden: cleanList(prefs.hidden),
			preferred: cleanList(prefs.preferred),
		}),
	);
}

export function normalizeDomain(value: string): string {
	return value
		.trim()
		.toLowerCase()
		.replace(/^https?:\/\//, "")
		.replace(/^www\./, "")
		.split("/")[0]
		.replace(/\.$/, "");
}

/** `rule` matches itself and its subdomains. */
export function domainMatches(host: string, rule: string): boolean {
	const h = normalizeDomain(host);
	const r = normalizeDomain(rule);
	if (!h || !r) return false;
	return h === r || h.endsWith(`.${r}`);
}

export function hostOf(url: string): string {
	try {
		return normalizeDomain(new URL(url).hostname);
	} catch {
		return "";
	}
}

export function applySourcePrefs(
	results: SearchResult[],
	prefs: SourcePrefs,
): {
	results: SearchResult[];
	hiddenCount: number;
} {
	const hidden = results.filter((result) =>
		prefs.hidden.some((rule) => domainMatches(hostOf(result.url), rule)),
	);
	const visible = results.filter((result) => !hidden.includes(result));
	const preferred: SearchResult[] = [];
	const rest: SearchResult[] = [];
	for (const result of visible) {
		if (
			prefs.preferred.some((rule) => domainMatches(hostOf(result.url), rule))
		) {
			preferred.push(result);
		} else {
			rest.push(result);
		}
	}
	return { results: [...preferred, ...rest], hiddenCount: hidden.length };
}

export function toggleHidden(prefs: SourcePrefs, domain: string): SourcePrefs {
	const normalized = normalizeDomain(domain);
	const hidden = prefs.hidden.includes(normalized)
		? prefs.hidden.filter((item) => item !== normalized)
		: [...prefs.hidden, normalized];
	return {
		hidden,
		preferred: prefs.preferred.filter((item) => item !== normalized),
	};
}

export function togglePreferred(
	prefs: SourcePrefs,
	domain: string,
): SourcePrefs {
	const normalized = normalizeDomain(domain);
	const preferred = prefs.preferred.includes(normalized)
		? prefs.preferred.filter((item) => item !== normalized)
		: [...prefs.preferred, normalized];
	return {
		preferred,
		hidden: prefs.hidden.filter((item) => item !== normalized),
	};
}

function cleanList(value: unknown): string[] {
	if (!Array.isArray(value)) return [];
	const seen = new Set<string>();
	const out: string[] = [];
	for (const item of value) {
		if (typeof item !== "string") continue;
		const domain = normalizeDomain(item);
		if (!domain || seen.has(domain) || domain.includes("*")) continue;
		seen.add(domain);
		out.push(domain);
	}
	return out;
}
