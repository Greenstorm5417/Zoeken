/** Local "time"/"clock"/"now" keyword answerer — current UTC time, no network. */
import type { SearchAnswer } from "../api";

const KEYWORDS = new Set(["time", "timezone", "now", "clock", "timezones"]);

function nowUtc(): string {
	const iso = new Date().toISOString();
	// "2026-07-21T20:15:00.000Z" -> "2026-07-21 20:15:00 UTC"
	return `${iso.slice(0, 10)} ${iso.slice(11, 19)} UTC`;
}

/** Whole query must be time-related keywords only (matches the reference plugin). */
export function computeTimeZoneAnswer(
	query: string,
	pageno: number,
): SearchAnswer | null {
	if (pageno > 1) return null;
	const parts = query.trim().split(/\s+/).filter(Boolean);
	if (parts.length === 0) return null;
	if (!parts.every((part) => KEYWORDS.has(part.toLowerCase()))) return null;
	return {
		answer: nowUtc(),
		engine: "time_zone",
		url: null,
		interactive: null,
	};
}
