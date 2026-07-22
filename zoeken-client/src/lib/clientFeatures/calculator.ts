/** Local "looks like a calculation" detector and locale-aware normalizer. */

import type { InteractiveAnswer, SearchAnswer } from "../api";
import { calcEval, formatCalcNumber } from "../calcEval";

// Locales that write decimals with a comma and thousands with a dot
// (mirrors the reference SearXNG calculator plugin's language list).
const COMMA_DECIMAL_LANGS = new Set([
	"de",
	"fr",
	"es",
	"it",
	"nl",
	"pt",
	"ru",
	"pl",
	"sv",
	"da",
	"fi",
	"nb",
	"nn",
	"no",
	"cs",
	"sk",
	"sl",
	"hr",
	"sr",
	"uk",
	"bg",
	"ro",
	"hu",
	"tr",
	"el",
	"lt",
	"lv",
	"et",
	"is",
	"ca",
	"gl",
	"eu",
	"af",
	"id",
]);

function looksLikeExpression(text: string): boolean {
	if (text === "") return false;
	let hasDigit = false;
	let hasOperator = false;
	for (const ch of text) {
		if (/\d/.test(ch)) {
			hasDigit = true;
		} else if (/[+\-*/%^(). ,]/.test(ch)) {
			if (/[+\-*/%^]/.test(ch)) hasOperator = true;
		} else {
			return false;
		}
	}
	return hasDigit && hasOperator;
}

function normalizeExpr(expr: string, locale: string): string {
	const lang = (locale.split(/[-_]/)[0] ?? "").toLowerCase();
	if (COMMA_DECIMAL_LANGS.has(lang)) {
		return expr.replaceAll(".", "").replaceAll(",", ".");
	}
	return expr.replaceAll(",", "");
}

/** Compute a calculator answer for a raw query, or `null` if it isn't one. */
export function computeCalculatorAnswer(
	query: string,
	locale: string,
	pageno: number,
): SearchAnswer | null {
	if (pageno > 1) return null;
	const expr = query.trim();
	if (!looksLikeExpression(expr)) return null;
	const normalized = normalizeExpr(expr, locale);
	const result = calcEval(normalized);
	if (!result.ok) return null;
	const interactive: InteractiveAnswer = {
		type: "calculator",
		expression: normalized,
		result: result.value,
	};
	return {
		answer: formatCalcNumber(result.value),
		engine: "calculator",
		interactive,
	};
}
