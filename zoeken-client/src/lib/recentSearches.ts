/** Opt-in recent queries — localStorage only, default off. */

const ENABLED_KEY = "zoeken.recentSearchesEnabled";
const LIST_KEY = "zoeken.recentSearches";
const MAX = 8;

export function recentSearchesEnabled(): boolean {
	if (typeof localStorage === "undefined") return false;
	return localStorage.getItem(ENABLED_KEY) === "1";
}

export function setRecentSearchesEnabled(on: boolean) {
	if (typeof localStorage === "undefined") return;
	localStorage.setItem(ENABLED_KEY, on ? "1" : "0");
	if (!on) clearRecentSearches();
}

export function getRecentSearches(): string[] {
	if (typeof localStorage === "undefined" || !recentSearchesEnabled()) {
		return [];
	}
	try {
		const raw = localStorage.getItem(LIST_KEY);
		if (!raw) return [];
		const parsed = JSON.parse(raw) as unknown;
		if (!Array.isArray(parsed)) return [];
		return parsed
			.filter((q): q is string => typeof q === "string" && q.trim() !== "")
			.slice(0, MAX);
	} catch {
		return [];
	}
}

export function rememberRecentSearch(query: string) {
	if (!recentSearchesEnabled()) return;
	const q = query.trim();
	if (!q) return;
	const next = [q, ...getRecentSearches().filter((item) => item !== q)].slice(
		0,
		MAX,
	);
	localStorage.setItem(LIST_KEY, JSON.stringify(next));
}

export function clearRecentSearches() {
	if (typeof localStorage === "undefined") return;
	localStorage.removeItem(LIST_KEY);
}
