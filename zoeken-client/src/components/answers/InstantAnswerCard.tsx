import {
	ArrowLeftRight,
	BookOpen,
	CloudSun,
	Languages,
	Library,
	Sigma,
	Sparkles,
} from "lucide-react";
import { lazy, Suspense } from "react";
import type { SearchAnswer } from "#/lib/api";

const UnitAnswer = lazy(() =>
	import("./UnitAnswer").then((m) => ({ default: m.UnitAnswer })),
);
const CryptoAnswer = lazy(() =>
	import("./CryptoAnswer").then((m) => ({ default: m.CryptoAnswer })),
);
const CurrencyAnswer = lazy(() =>
	import("./CurrencyAnswer").then((m) => ({ default: m.CurrencyAnswer })),
);
const CalculatorAnswer = lazy(() =>
	import("./CalculatorAnswer").then((m) => ({ default: m.CalculatorAnswer })),
);
const WeatherAnswer = lazy(() =>
	import("./WeatherAnswer").then((m) => ({ default: m.WeatherAnswer })),
);
const SelfInfoAnswer = lazy(() =>
	import("./SelfInfoAnswer").then((m) => ({ default: m.SelfInfoAnswer })),
);
const TranslateAnswer = lazy(() =>
	import("./TranslateAnswer").then((m) => ({ default: m.TranslateAnswer })),
);
const DictionaryAnswer = lazy(() =>
	import("./DictionaryAnswer").then((m) => ({ default: m.DictionaryAnswer })),
);
const WikipediaAnswer = lazy(() =>
	import("./WikipediaAnswer").then((m) => ({ default: m.WikipediaAnswer })),
);

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

function InteractiveFallback() {
	return (
		<section
			className="zoeken-answer mb-6 max-w-[40rem] animate-pulse rounded-2xl border border-line bg-surface-raised px-4 py-4 sm:px-5"
			aria-hidden
		>
			<p className="mb-3 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
				…
			</p>
			<div className="h-8 w-3/5 rounded bg-line/60" />
		</section>
	);
}

/** Instant answer card — interactive widgets when `interactive` is present. */
export function InstantAnswerCard({ answer }: { answer: SearchAnswer }) {
	const interactive = answer.interactive;
	if (interactive?.type === "unit") {
		return (
			<Suspense fallback={<InteractiveFallback />}>
				<UnitAnswer answer={answer} initial={interactive} />
			</Suspense>
		);
	}
	if (interactive?.type === "currency") {
		return (
			<Suspense fallback={<InteractiveFallback />}>
				<CurrencyAnswer answer={answer} initial={interactive} />
			</Suspense>
		);
	}
	if (interactive?.type === "calculator") {
		return (
			<Suspense fallback={<InteractiveFallback />}>
				<CalculatorAnswer answer={answer} initial={interactive} />
			</Suspense>
		);
	}
	if (interactive?.type === "weather") {
		return (
			<Suspense fallback={<InteractiveFallback />}>
				<WeatherAnswer answer={answer} initial={interactive} />
			</Suspense>
		);
	}
	if (interactive?.type === "self_info") {
		return (
			<Suspense fallback={<InteractiveFallback />}>
				<SelfInfoAnswer answer={answer} initial={interactive} />
			</Suspense>
		);
	}
	if (interactive?.type === "crypto") {
		return (
			<Suspense fallback={<InteractiveFallback />}>
				<CryptoAnswer answer={answer} initial={interactive} />
			</Suspense>
		);
	}
	if (interactive?.type === "translate") {
		return (
			<Suspense fallback={<InteractiveFallback />}>
				<TranslateAnswer answer={answer} initial={interactive} />
			</Suspense>
		);
	}
	if (interactive?.type === "dictionary") {
		return (
			<Suspense fallback={<InteractiveFallback />}>
				<DictionaryAnswer answer={answer} initial={interactive} />
			</Suspense>
		);
	}
	if (interactive?.type === "wikipedia") {
		return (
			<Suspense fallback={<InteractiveFallback />}>
				<WikipediaAnswer answer={answer} initial={interactive} />
			</Suspense>
		);
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
