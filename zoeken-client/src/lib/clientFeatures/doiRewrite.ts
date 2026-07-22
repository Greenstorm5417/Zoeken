/** Rewrite paywalled publisher links to an open-access DOI resolver. */
import type { SearchResult } from "../api";

const DOI_RE = /10\.\d{4,9}\/\S+/;
const DOI_SUFFIXES = ["/", ".pdf", ".xml", "/full", "/meta", "/abstract"];

function stripDoiSuffixes(raw: string): string {
	let doi = raw;
	for (const suffix of DOI_SUFFIXES) {
		if (doi.endsWith(suffix)) doi = doi.slice(0, -suffix.length);
	}
	return doi;
}

/** Extract a bare DOI from a result URL's path, falling back to its query values. */
export function extractDoi(raw: string): string | null {
	let parsed: URL;
	try {
		parsed = new URL(raw);
	} catch {
		return null;
	}
	const inPath = DOI_RE.exec(parsed.pathname);
	if (inPath) return stripDoiSuffixes(inPath[0]);
	for (const value of parsed.searchParams.values()) {
		const found = DOI_RE.exec(value);
		if (found) return stripDoiSuffixes(found[0]);
	}
	return null;
}

/** Rewrite `result.url` to `resolver + doi` when a short DOI is present. */
export function applyDoiRewrite(
	result: SearchResult,
	resolver: string,
): SearchResult {
	if (!result.url) return result;
	const doi = extractDoi(result.url);
	if (!doi || doi.length >= 50) return result;
	const next = { ...result, url: resolver + doi };
	if (next.template === "paper.html" && !next.doi) next.doi = doi;
	return next;
}
