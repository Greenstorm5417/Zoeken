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
import { CalculatorAnswer } from "./CalculatorAnswer";
import { CryptoAnswer } from "./CryptoAnswer";
import { CurrencyAnswer } from "./CurrencyAnswer";
import { DictionaryAnswer } from "./DictionaryAnswer";
import { SelfInfoAnswer } from "./SelfInfoAnswer";
import { TranslateAnswer } from "./TranslateAnswer";
import { UnitAnswer } from "./UnitAnswer";
import { WeatherAnswer } from "./WeatherAnswer";
import { WikipediaAnswer } from "./WikipediaAnswer";

function formatEngineLabel(name: string): string {
	return name
		.split(/[_:\s-]+/)
		.filter(Boolean)
		.map((part) => part.charAt(0).toUpperCase() + part.slice(1))
		.join(" ");
}

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
	if (interactive?.type === "unit") {
		return <UnitAnswer answer={answer} initial={interactive} />;
	}
	if (interactive?.type === "currency") {
		return <CurrencyAnswer answer={answer} initial={interactive} />;
	}
	if (interactive?.type === "calculator") {
		return <CalculatorAnswer answer={answer} initial={interactive} />;
	}
	if (interactive?.type === "weather") {
		return <WeatherAnswer answer={answer} initial={interactive} />;
	}
	if (interactive?.type === "self_info") {
		return <SelfInfoAnswer answer={answer} initial={interactive} />;
	}
	if (interactive?.type === "crypto") {
		return <CryptoAnswer answer={answer} initial={interactive} />;
	}
	if (interactive?.type === "translate") {
		return <TranslateAnswer answer={answer} initial={interactive} />;
	}
	if (interactive?.type === "dictionary") {
		return <DictionaryAnswer answer={answer} initial={interactive} />;
	}
	if (interactive?.type === "wikipedia") {
		return <WikipediaAnswer answer={answer} initial={interactive} />;
	}

	const { Icon, label } = answerKind(answer.engine);
	const equation = splitEquation(answer.answer);
	return (
		<section className="mb-6 max-w-[40rem] rounded-2xl border border-line bg-surface-raised px-5 py-4">
			<p className="mb-2 flex items-center gap-2 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
				<Icon className="size-4 text-accent" aria-hidden />
				{label}
			</p>
			{equation ? (
				<p className="text-[1.6rem] leading-snug tracking-tight break-words">
					<span className="text-ink-muted">{equation[0]}</span>
					<span className="text-ink-muted"> = </span>
					<span className="font-semibold text-ink">{equation[1]}</span>
				</p>
			) : (
				<p className="text-[1.35rem] leading-snug tracking-tight break-words text-ink">
					{answer.answer}
				</p>
			)}
			{answer.url ? (
				<a
					href={answer.url}
					target="_blank"
					rel="noopener noreferrer"
					className="mt-2 inline-block text-sm text-accent hover:underline"
				>
					{hostnameOf(answer.url)}
				</a>
			) : null}
		</section>
	);
}
