import { CloudSun } from "lucide-react";

function hostnameOf(url: string): string {
	try {
		return new URL(url).hostname.replace(/^www\./, "");
	} catch {
		return url;
	}
}

import { AnswerShell } from "#/components/answers/AnswerShell";
import { weatherPresentation } from "#/lib/answers";
import type { InteractiveAnswer, SearchAnswer } from "#/lib/api";
import { activeStrings } from "#/lib/i18n";

export function WeatherAnswer({
	answer,
	initial,
	query = "",
}: {
	answer: SearchAnswer;
	initial: Extract<InteractiveAnswer, { type: "weather" }>;
	query?: string;
}) {
	const t = activeStrings();
	const mode = weatherPresentation(
		query,
		initial.forecast_temp_c,
		initial.forecast_summary,
	);
	if (mode === "unavailable") {
		return (
			<AnswerShell title={t.forecast} icon={CloudSun}>
				<p className="text-base text-ink">
					{t.tomorrowForecastMissing}
					{initial.place ? ` — ${initial.place}` : ""}.{" "}
					{t.currentNotSubstituted}
				</p>
			</AnswerShell>
		);
	}
	const place = initial.place || t.unknownPlace;
	const showingForecast = mode === "forecast";
	const description = showingForecast
		? initial.forecast_summary
		: initial.description;
	const tempC = showingForecast ? initial.forecast_temp_c : initial.temp_c;
	const tempF = showingForecast ? initial.forecast_temp_f : initial.temp_f;
	const temp = tempC || tempF ? `${tempC || "—"}°C (${tempF || "—"}°F)` : null;
	const feels =
		!showingForecast && initial.feels_c
			? `${t.feelsLike} ${initial.feels_c}°C`
			: null;
	const wind =
		!showingForecast && initial.wind_kmph
			? `${t.wind} ${initial.wind_kmph} km/h${initial.wind_dir ? ` ${initial.wind_dir}` : ""}`
			: null;
	const humidity =
		!showingForecast && initial.humidity
			? `${t.humidity} ${initial.humidity}%`
			: null;
	const source = answer.url ? hostnameOf(answer.url) : answer.engine;

	return (
		<AnswerShell
			title={t.forecast}
			icon={CloudSun}
			footer={
				answer.url ? (
					<a
						href={answer.url}
						target="_blank"
						rel="noopener noreferrer"
						className="inline-block text-sm text-accent hover:underline"
					>
						{source}
					</a>
				) : source ? (
					<span className="text-sm text-ink-subtle">{source}</span>
				) : null
			}
		>
			<p className="text-xs font-medium tracking-wide text-ink-subtle uppercase">
				{showingForecast
					? initial.forecast_date
						? `${t.forecast} · ${initial.forecast_date}`
						: t.forecast
					: initial.observation_time
						? `${t.currentConditions} · ${t.observed} ${initial.observation_time}`
						: t.currentConditions}
			</p>
			<p className="mt-1 text-[1.35rem] leading-snug tracking-tight text-ink">
				{place}
			</p>
			{description ? (
				<p className="mt-1 text-base text-ink-muted">{description}</p>
			) : null}
			{temp ? (
				<p className="mt-3 text-[1.75rem] font-semibold tabular-nums tracking-tight text-ink sm:text-[1.85rem]">
					{temp}
				</p>
			) : null}
			{(feels || wind || humidity) && (
				<div className="mt-3 flex flex-wrap gap-x-4 gap-y-1.5 text-sm text-ink-muted">
					{feels ? <span>{feels}</span> : null}
					{wind ? <span>{wind}</span> : null}
					{humidity ? <span>{humidity}</span> : null}
				</div>
			)}
		</AnswerShell>
	);
}
