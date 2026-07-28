import {
	ArrowLeftRight,
	BookOpen,
	CloudSun,
	Languages,
	Library,
	Sigma,
	Sparkles,
} from "lucide-react";
import type { SearchAnswer } from "#/lib/api";
import { formatEngineLabel } from "#/lib/searchDisplay";
import { CalculatorAnswer } from "./CalculatorAnswer";
import { CryptoAnswer } from "./CryptoAnswer";
import { CurrencyAnswer } from "./CurrencyAnswer";
import { DictionaryAnswer } from "./DictionaryAnswer";
import { SelfInfoAnswer } from "./SelfInfoAnswer";
import { TranslateAnswer } from "./TranslateAnswer";
import { UnitAnswer } from "./UnitAnswer";
import { WeatherAnswer } from "./WeatherAnswer";
import { WikipediaAnswer } from "./WikipediaAnswer";

function hostnameOf(url: string): string {
	try {
		return new URL(url).hostname.replace(/^www\./, "");
	} catch {
		return url;
	}
}

function answerKind(engine: string | undefined): {
	Icon: typeof Sparkles;
	label: string;
} {
	const name = (engine ?? "").toLowerCase();
	if (name === "calculator") return { Icon: Sigma, label: "Calculator" };
	if (name === "unit converter" || name === "units")
		return { Icon: ArrowLeftRight, label: "Unit converter" };
	if (name === "currency") return { Icon: ArrowLeftRight, label: "Currency" };
	if (name === "weather") return { Icon: CloudSun, label: "Weather" };
	if (name === "translate") return { Icon: Languages, label: "Translate" };
	if (name === "dictionary") return { Icon: BookOpen, label: "Dictionary" };
	if (name === "wikipedia") return { Icon: Library, label: "Wikipedia" };
	if (name.startsWith("answerer:"))
		return { Icon: Sigma, label: formatEngineLabel(name.slice(9).trim()) };
	return { Icon: Sparkles, label: formatEngineLabel(name || "Answer") };
}

function splitEquation(text: string): [string, string] | null {
	const index = text.lastIndexOf(" = ");
	if (index <= 0) return null;
	return [text.slice(0, index), text.slice(index + 3)];
}

/** Instant answer card — interactive widgets when `interactive` is present. */
export function InstantAnswerCard({ answer }: { answer: SearchAnswer }) {
	const interactive = answer.interactive;
	switch (interactive?.type) {
		case "unit":
			return <UnitAnswer answer={answer} initial={interactive} />;
		case "currency":
			return <CurrencyAnswer answer={answer} initial={interactive} />;
		case "calculator":
			return <CalculatorAnswer answer={answer} initial={interactive} />;
		case "weather":
			return <WeatherAnswer answer={answer} initial={interactive} />;
		case "self_info":
			return <SelfInfoAnswer answer={answer} initial={interactive} />;
		case "crypto":
			return <CryptoAnswer answer={answer} initial={interactive} />;
		case "translate":
			return <TranslateAnswer answer={answer} initial={interactive} />;
		case "dictionary":
			return <DictionaryAnswer answer={answer} initial={interactive} />;
		case "wikipedia":
			return <WikipediaAnswer answer={answer} initial={interactive} />;
		default:
			break;
	}

	const { Icon, label } = answerKind(answer.engine);
	const equation = splitEquation(answer.answer);
	return (
		<section className="zoeken-answer mb-6 max-w-[40rem] rounded-2xl border border-line bg-surface-raised px-4 py-4 sm:px-5">
			<p className="mb-3 flex items-center gap-2 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
				<Icon className="size-4 text-accent" aria-hidden />
				{label}
			</p>
			{equation ? (
				<p className="text-[1.5rem] leading-snug tracking-tight break-words sm:text-[1.6rem]">
					<span className="text-ink-muted">{equation[0]}</span>
					<span className="text-ink-muted"> = </span>
					<span className="font-semibold text-ink">{equation[1]}</span>
				</p>
			) : (
				<p className="text-[1.25rem] leading-snug tracking-tight break-words text-ink sm:text-[1.35rem]">
					{answer.answer}
				</p>
			)}
			{answer.url ? (
				<a
					href={answer.url}
					target="_blank"
					rel="noopener noreferrer"
					className="mt-3 inline-block text-sm text-accent hover:underline"
				>
					{hostnameOf(answer.url)}
				</a>
			) : null}
		</section>
	);
}
