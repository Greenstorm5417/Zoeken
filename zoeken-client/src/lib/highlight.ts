/** Safe query-term highlighting. Returns text pieces, never HTML. */

export type HighlightPiece = { text: string; mark: boolean };

const STOP = new Set([
	"a",
	"an",
	"the",
	"of",
	"and",
	"or",
	"in",
	"on",
	"to",
	"for",
	"is",
]);

export function highlightPieces(text: string, query: string): HighlightPiece[] {
	const norm = text.normalize("NFC");
	const terms = queryTerms(query);
	if (!norm || terms.length === 0) return [{ text, mark: false }];
	const pattern = terms
		.slice()
		.sort((a, b) => b.length - a.length)
		.map(escapeRegExp)
		.join("|");
	const re = new RegExp(
		`(?<![\\p{L}\\p{N}_+#])(?:${pattern})(?![\\p{L}\\p{N}_+#])`,
		"giu",
	);
	const pieces: HighlightPiece[] = [];
	let cursor = 0;
	for (const match of norm.matchAll(re)) {
		const index = match.index ?? 0;
		if (index > cursor)
			pieces.push({ text: norm.slice(cursor, index), mark: false });
		pieces.push({ text: match[0], mark: true });
		cursor = index + match[0].length;
	}
	if (cursor < norm.length)
		pieces.push({ text: norm.slice(cursor), mark: false });
	return pieces.length ? pieces : [{ text, mark: false }];
}

function queryTerms(query: string): string[] {
	const stripped = query
		.normalize("NFC")
		.replace(/"[^"]*"/g, " ")
		.replace(/(^|\s)(?:site|filetype):[^\s]+/gi, " ")
		.replace(/(^|\s)-[\p{L}\p{N}_']+/gu, " ");
	const tokens = stripped.match(/[\p{L}\p{N}_#+.]+/gu) ?? [];
	return [
		...new Set(
			tokens
				.map((token) => token.normalize("NFC").toLowerCase())
				.filter(
					(token) =>
						token.length >= 2 || token.includes("+") || token.includes("#"),
				)
				.filter((token) => !STOP.has(token)),
		),
	];
}

function escapeRegExp(value: string) {
	return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
