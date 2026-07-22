import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { SiteNav } from "#/components/SiteNav";
import { preferencesGet, stats, statsErrors } from "#/lib/api";
import { stringsFor } from "#/lib/i18n";

export const Route = createFileRoute("/stats")({
	component: StatsPage,
});

function StatsPage() {
	const prefs = useQuery({
		queryKey: ["preferences"],
		queryFn: preferencesGet,
	});
	const t = stringsFor(prefs.data?.locale);
	const timing = useQuery({ queryKey: ["stats"], queryFn: stats });
	const errors = useQuery({ queryKey: ["stats-errors"], queryFn: statsErrors });

	return (
		<main className="relative mx-auto min-h-dvh w-full max-w-4xl px-6 py-10 pt-20">
			<SiteNav />
			<Link
				to="/"
				className="mb-8 inline-flex items-center gap-2 text-sm text-ink-muted no-underline hover:text-accent"
			>
				<img src="/zoeken-logo.svg" alt="" width={20} height={20} />
				Zoeken
			</Link>
			<h1 className="text-3xl font-bold tracking-tight">{t.statsTitle}</h1>
			<p className="mt-2 text-ink-muted">{t.statsBlurb}</p>

			<section className="mt-8">
				<h2 className="text-lg font-medium text-ink">{t.statsTiming}</h2>
				{timing.isLoading ? (
					<p className="mt-3 text-sm text-ink-muted">{t.statsLoading}</p>
				) : timing.isError ? (
					<p className="mt-3 text-sm text-red-700">
						{t.statsCouldntLoadTiming}
					</p>
				) : (timing.data?.engines.length ?? 0) === 0 ? (
					<p className="mt-3 text-sm text-ink-muted">{t.statsNoSamples}</p>
				) : (
					<div className="mt-3 overflow-x-auto rounded-xl border border-line">
						<table className="w-full min-w-[36rem] text-left text-sm">
							<thead className="border-b border-line bg-surface-raised text-ink-muted">
								<tr>
									<th className="px-3 py-2 font-medium">{t.statsEngine}</th>
									<th className="px-3 py-2 font-medium">{t.statsRequests}</th>
									<th className="px-3 py-2 font-medium">{t.statsAvgMs}</th>
									<th className="px-3 py-2 font-medium">{t.statsHttpAvgMs}</th>
								</tr>
							</thead>
							<tbody>
								{timing.data?.engines.map((row) => (
									<tr
										key={row.engine}
										className="border-b border-line last:border-0"
									>
										<td className="px-3 py-2 font-medium text-ink">
											{row.engine}
										</td>
										<td className="px-3 py-2 text-ink-muted">
											{row.total_count}
										</td>
										<td className="px-3 py-2 text-ink-muted">
											{(row.total_avg_seconds * 1000).toFixed(0)}
										</td>
										<td className="px-3 py-2 text-ink-muted">
											{(row.http_avg_seconds * 1000).toFixed(0)}
										</td>
									</tr>
								))}
							</tbody>
						</table>
					</div>
				)}
			</section>

			<section className="mt-8">
				<h2 className="text-lg font-medium text-ink">{t.statsErrors}</h2>
				{errors.isLoading ? (
					<p className="mt-3 text-sm text-ink-muted">{t.statsLoading}</p>
				) : errors.isError ? (
					<p className="mt-3 text-sm text-red-700">
						{t.statsCouldntLoadErrors}
					</p>
				) : (errors.data?.engines.length ?? 0) === 0 ? (
					<p className="mt-3 text-sm text-ink-muted">{t.statsNoErrors}</p>
				) : (
					<div className="mt-3 grid gap-3">
						{errors.data?.engines.map((row) => (
							<details
								key={row.engine}
								className="rounded-xl border border-line bg-surface-raised px-4 py-3"
							>
								<summary className="cursor-pointer text-sm font-medium text-ink">
									{row.engine}
									<span className="ml-2 font-normal text-ink-muted">
										{row.total} {t.statsTotal}
									</span>
								</summary>
								<ul className="mt-2 space-y-1 text-sm text-ink-muted">
									{Object.entries(row.errors).map(([name, count]) => (
										<li key={name} className="flex justify-between gap-4">
											<span className="font-mono">{name}</span>
											<span>{count}</span>
										</li>
									))}
								</ul>
							</details>
						))}
					</div>
				)}
			</section>
		</main>
	);
}
