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

/** Quoted phrases, operators, and technical tokens are not auto-corrected. */
export function correctionProtected(query: string): boolean {
	const q = query.trim();
	if (!q) return true;
	if (
		q.includes('"') ||
		/\b(?:site|filetype):/i.test(q) ||
		/(^|\s)[!][A-Za-z0-9]/.test(q)
	) {
		return true;
	}
	const tokens = q.split(/\s+/);
	if (tokens.some((token) => /[+#_:.]|\d/.test(token))) return true;
	return tokens.slice(1).some((token) => /^[A-Z][\p{L}'-]{2,}$/u.test(token));
}

/**
 * Closest autocomplete alternative, not merely the first one within distance 2.
 * Returns null when the query should be left exactly as typed.
 */
export function pickDidYouMean(
	query: string,
	suggestions: string[],
): string | null {
	const q = query.trim();
	if (!q || suggestions.length === 0 || correctionProtected(q)) return null;
	const lower = q.toLowerCase();
	let bestText: string | null = null;
	let bestDistance = 3;
	let bestIndex = Number.POSITIVE_INFINITY;
	for (let index = 0; index < suggestions.length; index += 1) {
		const text = suggestions[index]?.trim() ?? "";
		if (!text || text.toLowerCase() === lower) continue;
		const distance = editDistance(q, text, 2);
		if (distance > 2) continue;
		if (
			distance < bestDistance ||
			(distance === bestDistance && index < bestIndex)
		) {
			bestText = text;
			bestDistance = distance;
			bestIndex = index;
		}
	}
	return bestText;
}
