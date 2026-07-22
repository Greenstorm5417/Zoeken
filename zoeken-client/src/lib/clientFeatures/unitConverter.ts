/** "10 km to miles" / "how many cups in a gallon" phrase parser and answerer. */
import type { SearchAnswer } from "../api";
import { convertUnits, formatUnitNumber, UNITS, type UnitDef } from "../units";

type PhraseAlias = { phrase: string; unit: UnitDef };

const unitsByAlias = new Map<string, UnitDef[]>();
const phraseAliases: PhraseAlias[] = [];

function indexAlias(alias: string, unit: UnitDef): void {
	const key = alias.toLowerCase();
	if (/\s/.test(key)) {
		phraseAliases.push({ phrase: key, unit });
	}
	const bucket = unitsByAlias.get(key);
	if (!bucket) {
		unitsByAlias.set(key, [unit]);
	} else if (!bucket.some((existing) => existing.id === unit.id)) {
		bucket.push(unit);
	}
}

for (const unit of UNITS) {
	indexAlias(unit.id, unit);
	for (const alias of unit.abbreviations) indexAlias(alias, unit);
}
phraseAliases.sort((a, b) => b.phrase.length - a.phrase.length);

const TRAILING_FILLER = new Set(["please", "pls", "thanks", "thank", "now"]);

function stripTrailingNoise(input: string): string {
	let text = input.replace(/[?!.,]+\s*$/, "");
	for (;;) {
		const match = text.match(/^(.*)\s+(\S+)$/);
		if (!match) break;
		const [, head, last] = match;
		const bare = last.replace(/[?!.,]+$/, "");
		if (TRAILING_FILLER.has(bare.toLowerCase())) {
			text = head;
		} else if (bare !== last) {
			text = `${head} ${bare}`;
			break;
		} else {
			break;
		}
	}
	return text;
}

/** Collapse multi-word unit phrases ("fl oz", "fluid ounce") to unit ids. */
function normalizePhrases(input: string): string {
	let text = input;
	for (const entry of phraseAliases) {
		let searchFrom = 0;
		for (;;) {
			const lower = text.toLowerCase();
			const i = lower.indexOf(entry.phrase, searchFrom);
			if (i === -1) break;
			const before = text.slice(0, i);
			const after = text.slice(i + entry.phrase.length);
			text = before + entry.unit.id + after;
			searchFrom = i + entry.unit.id.length;
		}
	}
	return text;
}

function lookupAll(raw: string): UnitDef[] | undefined {
	return unitsByAlias.get((raw ?? "").toLowerCase());
}

/** Prefer a candidate matching `preferredDimension` when the alias is ambiguous (oz). */
function lookup(
	raw: string,
	preferredDimension: string | null,
): UnitDef | null {
	const candidates = lookupAll(raw);
	if (!candidates || candidates.length === 0) return null;
	if (candidates.length === 1) return candidates[0];
	if (preferredDimension) {
		const match = candidates.find(
			(unit) => unit.dimension === preferredDimension,
		);
		if (match) return match;
	}
	return candidates[0];
}

function parseNumber(raw: string): number | null {
	const cleaned = (raw ?? "").replaceAll(",", "").trim();
	if (cleaned === "") return null;
	const value = Number(cleaned);
	return Number.isFinite(value) ? value : null;
}

/** Joined form: 10km / 72f. Degree/superscript chars allowed in the unit tail. */
function splitNumberUnit(
	raw: string,
	preferredDimension: string | null,
): [number, UnitDef] | null {
	const match = raw.match(/^([+-]?[\d.,]+)([a-zA-Z°/²]+)$/);
	if (!match) return null;
	const value = parseNumber(match[1]);
	if (value === null) return null;
	const unit = lookup(match[2], preferredDimension);
	if (!unit) return null;
	return [value, unit];
}

function parseMeasure(
	input: string,
	preferredDimension: string | null,
): [number, UnitDef] | null {
	const text = input.trim();
	const match = text.match(/^([+-]?[\d.,]+)\s+(\S+)$/);
	if (match) {
		const value = parseNumber(match[1]);
		const unit = lookup(match[2], preferredDimension);
		if (value !== null && unit) return [value, unit];
		return null;
	}
	return splitNumberUnit(text, preferredDimension);
}

function parseForward(text: string): [number, UnitDef, UnitDef] | null {
	// Lazy `.+?` mirrors Lua's non-greedy `.-`: leftmost/shortest split.
	const chosen =
		text.match(/^(.+?)\s+to\s+(\S+)$/i) ??
		text.match(/^(.+?)\s+in\s+(\S+)$/i) ??
		text.match(/^(.+?)\s+as\s+(\S+)$/i);
	if (!chosen) return null;
	const [, left, toSymbol] = chosen;

	const first = parseMeasure(left, null);
	if (!first) return null;
	let [value, fromUnit] = first;
	if (!lookupAll(toSymbol)) return null;
	let toUnit = lookup(toSymbol, fromUnit.dimension);
	if (!toUnit) return null;
	// Re-parse from with to's dimension (10 oz to ml -> floz).
	const reparsed = parseMeasure(left, toUnit.dimension);
	if (reparsed) [value, fromUnit] = reparsed;
	toUnit = lookup(toSymbol, fromUnit.dimension);
	if (!toUnit) return null;
	return [value, fromUnit, toUnit];
}

function parseReversed(text: string): [number, UnitDef, UnitDef] | null {
	const rest =
		text.match(/^how\s+many\s+(.+)$/i)?.[1] ??
		text.match(/^what\s+is\s+(.+)$/i)?.[1] ??
		text.match(/^what'?s\s+(.+)$/i)?.[1];
	if (!rest) return null;

	const head = rest.match(/^(\S+)\s+in\s+(.+)$/i);
	if (!head) return null;
	const toSymbol = head[1];
	const tail = head[2].trim();

	const bareUnit = tail.match(/^an?\s+(\S+)$/i)?.[1];
	let value: number | null = null;
	let fromUnit: UnitDef | null = null;
	if (bareUnit) {
		fromUnit = lookup(bareUnit, null);
		if (fromUnit) value = 1;
	} else {
		const parsed = parseMeasure(tail, null);
		if (parsed) [value, fromUnit] = parsed;
	}
	if (value === null || !fromUnit) return null;

	let toUnit = lookup(toSymbol, fromUnit.dimension);
	if (!toUnit) return null;
	// Re-resolve from with to's dimension (how many oz in a gal -> floz).
	if (bareUnit) {
		fromUnit = lookup(bareUnit, toUnit.dimension) ?? fromUnit;
	} else {
		const reparsed = parseMeasure(tail, toUnit.dimension);
		if (reparsed) [value, fromUnit] = reparsed;
	}
	toUnit = lookup(toSymbol, fromUnit.dimension);
	if (!toUnit) return null;
	return [value, fromUnit, toUnit];
}

export function computeUnitConverterAnswer(
	query: string,
	pageno: number,
): SearchAnswer | null {
	if (pageno > 1) return null;
	let text = stripTrailingNoise(query.trim());
	if (text === "") return null;
	text = normalizePhrases(text);

	const parsed = parseForward(text) ?? parseReversed(text);
	if (!parsed) return null;
	const [value, fromUnit, toUnit] = parsed;

	const result = convertUnits(value, fromUnit.id, toUnit.id);
	if (result === null) return null;

	return {
		answer: `${formatUnitNumber(value)} ${fromUnit.id} = ${formatUnitNumber(result)} ${toUnit.id}`,
		engine: "unit converter",
		interactive: {
			type: "unit",
			amount: value,
			from: fromUnit.id,
			to: toUnit.id,
			result,
			dimension: fromUnit.dimension,
		},
	};
}
