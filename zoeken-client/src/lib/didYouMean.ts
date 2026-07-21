/** Lightweight “Did you mean?” when engines omit corrections. No dictionary. */

/** Edit distance; bail early when > max. */
export function editDistance(a: string, b: string, max = 2): number {
	const x = a.toLowerCase();
	const y = b.toLowerCase();
	if (x === y) return 0;
	if (Math.abs(x.length - y.length) > max) return max + 1;
	const prev = Array.from({ length: y.length + 1 }, (_, i) => i);
	for (let i = 1; i <= x.length; i++) {
		let diag = prev[0];
		prev[0] = i;
		let rowMin = i;
		for (let j = 1; j <= y.length; j++) {
			const next =
				x[i - 1] === y[j - 1] ? diag : 1 + Math.min(diag, prev[j], prev[j - 1]);
			diag = prev[j];
			prev[j] = next;
			rowMin = Math.min(rowMin, next);
		}
		if (rowMin > max) return max + 1;
	}
	return prev[y.length];
}

/**
 * Pick a close autocomplete alternative. Limitation: only works when
 * autocomplete is on and returns a near neighbor — no offline dictionary.
 */
export function pickDidYouMean(
	query: string,
	suggestions: string[],
): string | null {
	const q = query.trim();
	if (!q || suggestions.length === 0) return null;
	const lower = q.toLowerCase();
	for (const raw of suggestions) {
		const s = raw.trim();
		if (!s || s.toLowerCase() === lower) continue;
		if (editDistance(q, s, 2) <= 2) return s;
	}
	return null;
}
