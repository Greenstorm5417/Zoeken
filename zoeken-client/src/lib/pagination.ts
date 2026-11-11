export type PageSlice = {
	continuation?: string;
	results: Array<{ url: string }>;
};

/**
 * Next page, or undefined when continuation is exhausted or recovery is used up.
 * `more` keeps going through a duplicate-only page. `unknown` allows one empty
 * or duplicate page so a later page is not discarded. Numbered pagination uses
 * the same function with whatever pages it has loaded.
 */
export function nextPageNumber(
	pages: PageSlice[],
	startPage: number,
): number | undefined {
	const last = pages.at(-1);
	if (!last) return undefined;
	if (last.continuation === "exhausted") return undefined;
	if (last.continuation === "more") {
		if (consecutiveRepeats(pages) >= 2) return undefined;
		return startPage + pages.length;
	}
	if (last.results.length === 0) {
		if (consecutiveEmpty(pages) >= 2) return undefined;
		return startPage + pages.length;
	}
	if (consecutiveRepeats(pages) >= 2) return undefined;
	return startPage + pages.length;
}

export function showNumberedNext(
	page: PageSlice | undefined,
	previous: PageSlice[] = [],
): boolean {
	if (!page) return false;
	return nextPageNumber([...previous, page], 1) !== undefined;
}

function consecutiveRepeats(pages: PageSlice[]): number {
	let count = 0;
	const seen = new Set<string>();
	for (const page of pages) {
		const urls = page.results.map((result) => result.url);
		const novel = urls.some((url) => !seen.has(url));
		for (const url of urls) seen.add(url);
		if (urls.length > 0 && !novel) count += 1;
		else count = 0;
	}
	return count;
}

function consecutiveEmpty(pages: PageSlice[]): number {
	let count = 0;
	for (const page of pages) {
		if (page.results.length === 0) count += 1;
		else count = 0;
	}
	return count;
}
