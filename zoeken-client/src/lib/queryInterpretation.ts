/**
 * Display mirror of `zoeken-query` interpretation.
 * The server owns the contract (`interpretation` on `/api/v1/search`). This
 * copy exists so chips and local answers can render before that response.
 * It does not rewrite the query. Low confidence stays `general`.
 */

export type Intent =
	| "navigational"
	| "technical"
	| "factual"
	| "weather"
	| "definition"
	| "translation"
	| "arithmetic"
	| "conversion"
	| "fresh"
	| "general";

export type Operators = {
	site: string | null;
	filetype: string | null;
	phrases: string[];
	excluded: string[];
};

export type Interpretation = {
	original: string;
	residual: string;
	operators: Operators;
	intent: Intent;
	confidence: number;
	source: "rules" | "model";
};

const MODEL_ENABLED = true;

export function interpret(original: string): Interpretation {
	const { operators, residual } = parseOperators(original);
	let intent = rulesIntent(original);
	let source: Interpretation["source"] = "rules";
	let confidence = intent.confidence;
	if (MODEL_ENABLED && intent.intent === "general") {
		const model = modelIntent(original);
		if (model.intent !== "general" && model.confidence >= 0.75) {
			intent = model;
			source = "model";
			confidence = model.confidence;
		}
	}
	if (confidence < 0.45) {
		intent = { intent: "general", confidence };
	}
	return {
		original,
		residual,
		operators,
		intent: intent.intent,
		confidence,
		source,
	};
}

export function parseOperators(original: string): {
	operators: Operators;
	residual: string;
} {
	const operators: Operators = {
		site: null,
		filetype: null,
		phrases: [],
		excluded: [],
	};
	const residual: string[] = [];
	let rest = original;
	while (rest.trim()) {
		rest = rest.trimStart();
		if (rest.startsWith('"')) {
			const end = rest.indexOf('"', 1);
			if (end > 1) {
				const phrase = rest.slice(1, end).trim();
				if (phrase) {
					operators.phrases.push(phrase);
					residual.push(phrase);
				}
				rest = rest.slice(end + 1);
				continue;
			}
		}
		const split = rest.search(/\s/);
		const token = split === -1 ? rest : rest.slice(0, split);
		rest = split === -1 ? "" : rest.slice(split);
		if (token.startsWith("site:") && token.length > 5 && !operators.site) {
			operators.site = token.slice(5).replaceAll('"', "");
			continue;
		}
		if (
			token.startsWith("filetype:") &&
			token.length > 9 &&
			!operators.filetype
		) {
			operators.filetype = token.slice(9).replace(/^\./, "").toLowerCase();
			continue;
		}
		const excluded = excludedTerm(token);
		if (excluded) {
			operators.excluded.push(excluded);
			continue;
		}
		residual.push(token);
	}
	return { operators, residual: residual.join(" ") };
}

function excludedTerm(token: string): string | null {
	if (!token.startsWith("-") || token.startsWith("--")) return null;
	const raw = token.slice(1);
	if (!raw || raw.includes("=") || raw.includes("+") || /^\d/.test(raw))
		return null;
	if (/^[\p{L}\p{N}_']{2,}$/u.test(raw)) return raw;
	return null;
}

/** Remove one operator occurrence, matching the source token rather than the normalized value. */
export function removeOperator(
	query: string,
	kind: "site" | "filetype" | "phrase" | "excluded",
	value: string,
): string {
	const tokens = query.match(/"[^"]*"|\S+/g) ?? [];
	let removed = false;
	const kept = tokens.filter((token) => {
		if (removed || !operatorTokenMatches(token, kind, value)) return true;
		removed = true;
		return false;
	});
	return kept.join(" ").replace(/\s+/g, " ").trim();
}

function operatorTokenMatches(
	token: string,
	kind: "site" | "filetype" | "phrase" | "excluded",
	value: string,
): boolean {
	if (kind === "phrase") {
		return token === `"${value}"`;
	}
	if (kind === "excluded") {
		return token.toLowerCase() === `-${value.toLowerCase()}`;
	}
	const prefix = kind === "site" ? "site:" : "filetype:";
	if (!token.toLowerCase().startsWith(prefix)) return false;
	const raw = token.slice(prefix.length).replaceAll('"', "").replace(/^\./, "");
	return raw.toLowerCase() === value.toLowerCase();
}

function rulesIntent(original: string): { intent: Intent; confidence: number } {
	const lower = original.trim().toLowerCase();
	if (!lower) return { intent: "general", confidence: 0 };
	if (isArithmetic(lower)) return { intent: "arithmetic", confidence: 0.95 };
	if (isConversion(lower)) return { intent: "conversion", confidence: 0.9 };
	if (/\b(weather|forecast|wetter|météo|meteo)\b/.test(lower)) {
		return { intent: "weather", confidence: 0.9 };
	}
	if (
		lower.startsWith("define ") ||
		lower.includes("meaning of ") ||
		lower.includes("definition of ") ||
		lower.includes("what does ")
	) {
		return { intent: "definition", confidence: 0.88 };
	}
	if (
		lower.startsWith("translate ") ||
		lower.includes("how do you say") ||
		lower.includes("how to say")
	) {
		return { intent: "translation", confidence: 0.86 };
	}
	if (isNavigational(lower))
		return { intent: "navigational", confidence: 0.84 };
	if (
		/error|exception|traceback|undefined reference|segfault|stack overflow|syntaxerror|typeerror|panic|npm err|cargo|compiler|::|undefined|errno/.test(
			lower,
		)
	) {
		return { intent: "technical", confidence: 0.8 };
	}
	if (
		/\b(latest|breaking|news|today|this week|this month)\b/.test(lower) &&
		!/\b(weather|forecast)\b/.test(lower)
	) {
		return { intent: "fresh", confidence: 0.7 };
	}
	const words = lower.split(/\s+/).length;
	const factualStart = [
		"who ",
		"what ",
		"when ",
		"where ",
		"why ",
		"how ",
		"which ",
		"qué ",
		"qui ",
		"was ist ",
	].some((start) => lower.startsWith(start));
	if (words >= 2 && (factualStart || lower.endsWith("?"))) {
		return { intent: "factual", confidence: 0.62 };
	}
	return { intent: "general", confidence: 0.35 };
}

function modelIntent(original: string): { intent: Intent; confidence: number } {
	const lower = original.trim().toLowerCase();
	const sky =
		(lower.includes("sky in ") || lower.includes("raining in ")) &&
		!lower.includes("news");
	if (sky) return { intent: "weather", confidence: 0.78 };
	return { intent: "general", confidence: 0.2 };
}

function isArithmetic(lower: string): boolean {
	lower = lower.replace(/\d{4}-\d{2}-\d{2}/g, " ");
	if (!/\d/.test(lower)) return false;
	const compact = lower.replace(/\s+/g, "");
	if (compact.includes("c++") || compact.includes("c#")) {
		const without = compact.replaceAll("c++", "").replaceAll("c#", "");
		if (!hasNumberOperator(without) && !hasOperatorWord(lower)) return false;
	}
	return hasNumberOperator(lower) || hasOperatorWord(lower);
}

function hasNumberOperator(lower: string): boolean {
	return /\d\s*[+\-*/×÷%]\s*\d/.test(lower);
}

function hasOperatorWord(lower: string): boolean {
	return (
		/\d/.test(lower) &&
		/(?:^|\s)(plus|minus|times|divided|modulo|over)(?:\s|$)/.test(lower)
	);
}

function isConversion(lower: string): boolean {
	if (!/\d/.test(lower)) return false;
	const units = [
		"km",
		"mile",
		"meter",
		"feet",
		"ft",
		"inch",
		"inches",
		"cm",
		"mm",
		"kg",
		"pound",
		"lb",
		"oz",
		"celsius",
		"fahrenheit",
		"mph",
		"liter",
		"litre",
		"gallon",
		"acre",
	];
	const hits = units.filter((unit) =>
		new RegExp(`(?:^|\\s)${unit}s?(?:\\s|$)`).test(lower),
	).length;
	if (hits >= 2) return true;
	return hits >= 1 && /\b(to|in|into|how many)\b/.test(lower);
}

function isNavigational(lower: string): boolean {
	const trimmed = lower.trim().replace(/\/$/, "");
	if (trimmed.includes(" ")) {
		return (
			trimmed.endsWith(" official site") ||
			trimmed.endsWith(" official website") ||
			trimmed.endsWith(" homepage") ||
			trimmed.startsWith("login ")
		);
	}
	const host = trimmed.replace(/^https?:\/\//, "");
	return host.includes(".") && /[a-z]/i.test(host);
}

export function wantsTomorrow(query: string): boolean {
	return /\b(tomorrow|morgen|demain)\b/i.test(query);
}
