import type { SearchAnswer } from "./api";
import { interpret, wantsTomorrow } from "./queryInterpretation";

const KIND_FOR_INTENT: Record<string, string> = {
	arithmetic: "calculator",
	conversion: "unit",
	weather: "weather",
	definition: "dictionary",
	translation: "translate",
};

function kindOf(answer: SearchAnswer): string {
	return answer.interactive?.type ?? answer.engine ?? "text";
}

/**
 * One primary answer. Same-kind duplicates collapse. A specific intent keeps
 * the matching answer (local first) and at most one extra reference card.
 */
export function presentAnswers(
	local: SearchAnswer[],
	remote: SearchAnswer[],
	query: string,
	serverIntent?: string,
): SearchAnswer[] {
	const intent = serverIntent || interpret(query).intent;
	const wanted = KIND_FOR_INTENT[intent];
	const ordered = [...local, ...remote];
	const seen = new Set<string>();
	const unique: SearchAnswer[] = [];
	for (const answer of ordered) {
		const kind = kindOf(answer);
		if (seen.has(kind)) continue;
		seen.add(kind);
		unique.push(answer);
	}
	if (!wanted) return unique.slice(0, 2);
	const primary = unique.find((answer) => kindOf(answer) === wanted);
	if (!primary) return unique.slice(0, 2);
	const extra = unique.find(
		(answer) =>
			answer !== primary &&
			(kindOf(answer) === "wikipedia" || kindOf(answer) === "infobox"),
	);
	return [primary, extra].filter((answer): answer is SearchAnswer =>
		Boolean(answer),
	);
}

export function localAnswersOnThisPage(
	category: string,
	pageno: number,
): boolean {
	if (pageno > 1) return false;
	return !category || category === "general";
}

export function weatherPresentation(
	query: string,
	forecastTemp: string,
	forecastSummary: string,
): "current" | "forecast" | "unavailable" {
	if (!wantsTomorrow(query)) return "current";
	if (forecastTemp.trim() || forecastSummary.trim()) return "forecast";
	return "unavailable";
}
