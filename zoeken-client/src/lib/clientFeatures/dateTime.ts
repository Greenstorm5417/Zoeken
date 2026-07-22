/** Local date/time math answerer: "days until christmas", "3pm est in cet". */
import type { SearchAnswer } from "../api";

export type YMD = { y: number; m: number; d: number };

const NAMED_DAYS: Record<string, [number, number]> = {
	christmas: [12, 25],
	"christmas eve": [12, 24],
	"new year": [1, 1],
	"new years": [1, 1],
	"new year's": [1, 1],
	halloween: [10, 31],
	"valentine's day": [2, 14],
	"valentines day": [2, 14],
	valentines: [2, 14],
};

const ZONES: Record<string, number> = {
	utc: 0,
	gmt: 0,
	est: -5 * 60,
	edt: -4 * 60,
	cst: -6 * 60,
	cdt: -5 * 60,
	mst: -7 * 60,
	mdt: -6 * 60,
	pst: -8 * 60,
	pdt: -7 * 60,
	cet: 60,
	cest: 2 * 60,
	eet: 2 * 60,
	eest: 3 * 60,
	bst: 60,
	ist: 5 * 60 + 30,
	jst: 9 * 60,
	kst: 9 * 60,
	hkt: 8 * 60,
	sgt: 8 * 60,
	aest: 10 * 60,
	aedt: 11 * 60,
	nzst: 12 * 60,
	nzdt: 13 * 60,
};

function zoneOffset(raw: string): number | undefined {
	return ZONES[raw.trim().toLowerCase()];
}

function epochDay({ y, m, d }: YMD): number {
	return Math.floor(Date.UTC(y, m - 1, d) / 86_400_000);
}

function isoDate({ y, m, d }: YMD): string {
	return `${String(y).padStart(4, "0")}-${String(m).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
}

function parseIsoDate(text: string): YMD | null {
	const match = text.match(/^(\d{4})-(\d{2})-(\d{2})$/);
	if (!match) return null;
	const y = Number(match[1]);
	const m = Number(match[2]);
	const d = Number(match[3]);
	// Round-trip through epoch day + back to reject invalid calendar dates
	// (e.g. 2026-02-30), matching chrono's NaiveDate::parse_from_str strictness.
	const roundTrip = new Date(Date.UTC(y, m - 1, d));
	if (
		roundTrip.getUTCFullYear() !== y ||
		roundTrip.getUTCMonth() !== m - 1 ||
		roundTrip.getUTCDate() !== d
	) {
		return null;
	}
	return { y, m, d };
}

function parseTargetDate(text: string, today: YMD): YMD | null {
	const iso = parseIsoDate(text);
	if (iso) return epochDay(iso) >= epochDay(today) ? iso : null;
	const named = NAMED_DAYS[text];
	if (!named) return null;
	const [month, day] = named;
	const thisYear = { y: today.y, m: month, d: day };
	return epochDay(thisYear) >= epochDay(today)
		? thisYear
		: { y: today.y + 1, m: month, d: day };
}

/** Exported for tests: same logic as computeDateTimeAnswer's date branch, with an injectable `today`. */
export function daysUntil(query: string, today: YMD): string | null {
	const lower = query.toLowerCase();
	const prefixes = [
		"days until ",
		"days till ",
		"how many days until ",
		"how many days till ",
	];
	const prefix = prefixes.find((p) => lower.startsWith(p));
	if (!prefix) return null;
	const targetText = lower
		.slice(prefix.length)
		.trim()
		.replace(/\?+$/, "")
		.trim();

	const target = parseTargetDate(targetText, today);
	if (!target) return null;
	const days = epochDay(target) - epochDay(today);
	const dateStr = isoDate(target);
	if (days === 0) return `${targetText} is today (${dateStr})`;
	if (days === 1) return `1 day until ${targetText} (${dateStr})`;
	return `${days} days until ${targetText} (${dateStr})`;
}

export function parseClock(raw: string): number | null {
	let body = raw;
	let pmOffset: number | null = null;
	if (raw.endsWith("pm")) {
		body = raw.slice(0, -2);
		pmOffset = 12 * 60;
	} else if (raw.endsWith("am")) {
		body = raw.slice(0, -2);
		pmOffset = 0;
	}
	let hours: number;
	let minutes: number;
	if (body.includes(":")) {
		const [h, m] = body.split(":");
		hours = Number(h);
		minutes = Number(m);
		if (!Number.isInteger(hours) || !Number.isInteger(minutes)) return null;
	} else {
		hours = Number(body);
		minutes = 0;
		if (!Number.isInteger(hours)) return null;
	}
	if (minutes < 0 || minutes >= 60) return null;
	if (pmOffset !== null) {
		if (hours < 1 || hours > 12) return null;
		const base = hours === 12 ? 0 : hours * 60;
		return base + minutes + pmOffset;
	}
	return hours >= 0 && hours < 24 ? hours * 60 + minutes : null;
}

function formatClock(minutes: number): string {
	const hours = Math.floor(minutes / 60);
	const mins = minutes % 60;
	let display: number;
	let suffix: string;
	if (hours === 0) {
		display = 12;
		suffix = "AM";
	} else if (hours <= 11) {
		display = hours;
		suffix = "AM";
	} else if (hours === 12) {
		display = 12;
		suffix = "PM";
	} else {
		display = hours - 12;
		suffix = "PM";
	}
	return `${display}:${String(mins).padStart(2, "0")} ${suffix}`;
}

function euclidMod(value: number, modulus: number): number {
	return ((value % modulus) + modulus) % modulus;
}

function euclidDiv(value: number, modulus: number): number {
	return Math.floor(value / modulus);
}

export function zoneConvert(query: string): string | null {
	const lower = query.toLowerCase();
	const tokens = lower.split(/\s+/).filter(Boolean);
	if (tokens.length !== 4) return null;
	const [timeRaw, fromRaw, sep, toRaw] = tokens;
	if (sep !== "in" && sep !== "to") return null;
	const from = zoneOffset(fromRaw);
	const to = zoneOffset(toRaw);
	if (from === undefined || to === undefined) return null;
	const minutes = parseClock(timeRaw);
	if (minutes === null) return null;

	const delta = minutes - from + to;
	const converted = euclidMod(delta, 24 * 60);
	const dayShift = euclidDiv(delta, 24 * 60);
	const shiftNote =
		dayShift === 1 ? " (next day)" : dayShift === -1 ? " (previous day)" : "";
	return `${formatClock(minutes)} ${fromRaw.toUpperCase()} = ${formatClock(converted)} ${toRaw.toUpperCase()}${shiftNote}`;
}

export function computeDateTimeAnswer(query: string): SearchAnswer | null {
	const text = query.trim();
	const now = new Date();
	const today: YMD = {
		y: now.getUTCFullYear(),
		m: now.getUTCMonth() + 1,
		d: now.getUTCDate(),
	};

	const days = daysUntil(text, today);
	if (days)
		return { answer: days, engine: "date math", url: null, interactive: null };

	const zone = zoneConvert(text);
	if (zone)
		return { answer: zone, engine: "time zones", url: null, interactive: null };

	return null;
}
