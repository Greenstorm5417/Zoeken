import { CloudSun } from "lucide-react";
import type { InteractiveAnswer, SearchAnswer } from "#/lib/api";

export function WeatherAnswer({
	answer,
	initial,
}: {
	answer: SearchAnswer;
	initial: Extract<InteractiveAnswer, { type: "weather" }>;
}) {
	const place = initial.place || "Unknown place";
	const temp =
		initial.temp_c || initial.temp_f
			? `${initial.temp_c || "—"}°C (${initial.temp_f || "—"}°F)`
			: null;
	const feels = initial.feels_c ? `Feels like ${initial.feels_c}°C` : null;
	const wind = initial.wind_kmph
		? `Wind ${initial.wind_kmph} km/h${initial.wind_dir ? ` ${initial.wind_dir}` : ""}`
		: null;
	const humidity = initial.humidity ? `Humidity ${initial.humidity}%` : null;

	return (
		<section className="mb-6 max-w-[40rem] rounded-2xl border border-line bg-surface-raised px-5 py-4">
			<p className="mb-2 flex items-center gap-2 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
				<CloudSun className="size-4 text-accent" aria-hidden />
				Weather
			</p>
			<p className="text-[1.35rem] leading-snug tracking-tight text-ink">
				{place}
			</p>
			{initial.description ? (
				<p className="mt-1 text-base text-ink-muted">{initial.description}</p>
			) : null}
			{temp ? (
				<p className="mt-3 text-[1.6rem] font-semibold tabular-nums tracking-tight text-ink">
					{temp}
				</p>
			) : null}
			<div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-sm text-ink-muted">
				{feels ? <span>{feels}</span> : null}
				{wind ? <span>{wind}</span> : null}
				{humidity ? <span>{humidity}</span> : null}
			</div>
			{answer.url ? (
				<a
					href={answer.url}
					target="_blank"
					rel="noopener noreferrer"
					className="mt-3 inline-block text-sm text-accent hover:underline"
				>
					wttr.in
				</a>
			) : null}
		</section>
	);
}
