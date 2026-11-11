import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useEffect, useMemo, useRef, useState } from "react";
import { InstantAnswerCard } from "#/components/answers/InstantAnswerCard";
import { InfoboxCard } from "#/components/InfoboxCard";
import { SearchResultList } from "#/components/SearchResultList";
import { SearchSerpHeader } from "#/components/SearchSerpHeader";
import { presentAnswers } from "#/lib/answers";
import {
	ApiError,
	autocomplete,
	preferencesGet,
	type SearchResult,
	search,
} from "#/lib/api";
import { applyClientFeatures, pluginEnabled } from "#/lib/clientFeatures";
import { ensureTrackerRules } from "#/lib/clientFeatures/trackerUrlRemover";
import { pickDidYouMean } from "#/lib/didYouMean";
import { engineEligibility } from "#/lib/eligibility";
import { activeStrings, stringsFor } from "#/lib/i18n";
import { nextPageNumber, showNumberedNext } from "#/lib/pagination";
import { interpret, removeOperator } from "#/lib/queryInterpretation";
import {
	correctionText,
	formatEngineLabel,
	pageNumbers,
	searchLink,
	suggestionText,
	wallClockSeconds,
} from "#/lib/searchDisplay";
import { parseSearchParams } from "#/lib/searchParams";
import {
	applySourcePrefs,
	loadSourcePrefs,
	type SourcePrefs,
	saveSourcePrefs,
	toggleHidden,
	togglePreferred,
} from "#/lib/sourcePrefs";
import { useLocalAnswers } from "#/lib/useLocalAnswers";
import { useConfig } from "./__root";

export const Route = createFileRoute("/search")({
	validateSearch: parseSearchParams,
	component: SearchPage,
});

/** Owns elapsed-time state so timer updates don't re-render the full SERP.
 *
 * `searchStartedAt` must come from the parent: this component only mounts once
 * `firstPage` exists, so starting the clock here would always read ~0s.
 */
function SearchResultStats({
	resultCount,
	retrievedLabel,
	hasResults,
	searchStartedAt,
	isSuccess,
	isFetching,
	dataUpdatedAt,
}: {
	resultCount: number | undefined;
	retrievedLabel: string;
	hasResults: boolean;
	searchStartedAt: number;
	isSuccess: boolean;
	isFetching: boolean;
	dataUpdatedAt: number;
}) {
	const [elapsedSec, setElapsedSec] = useState<number | null>(null);

	// biome-ignore lint/correctness/useExhaustiveDependencies: reset clock when search identity changes
	useEffect(() => {
		setElapsedSec(null);
	}, [searchStartedAt]);

	// biome-ignore lint/correctness/useExhaustiveDependencies: dataUpdatedAt re-runs when a fresh page lands
	useEffect(() => {
		if (!isSuccess || isFetching || searchStartedAt <= 0) return;
		// Freeze first-page wall time; don't grow on infinite-scroll fetches.
		setElapsedSec((prev) =>
			prev != null
				? prev
				: wallClockSeconds(searchStartedAt, performance.now()),
		);
	}, [isSuccess, isFetching, dataUpdatedAt, searchStartedAt]);

	if ((resultCount == null && elapsedSec == null) || !hasResults) return null;

	return (
		<p className="mb-4 max-w-[40rem] text-sm text-ink-subtle">
			{resultCount != null ? (
				<>
					{resultCount.toLocaleString()} {retrievedLabel}
				</>
			) : null}
			{resultCount != null && elapsedSec != null ? " · " : null}
			{elapsedSec != null ? `${elapsedSec.toFixed(2)}s` : null}
		</p>
	);
}

function SearchPage() {
	const params = Route.useSearch();
	const {
		q,
		pageno = 1,
		categories,
		language,
		safesearch = 0,
		time_range = "",
	} = params;
	const config = useConfig();
	const prefsQuery = useQuery({
		queryKey: ["preferences"],
		queryFn: preferencesGet,
	});
	const prefs = prefsQuery.data;
	const activeCategory = categories || "general";
	useEffect(() => {
		if (!config) return;
		const original = document.title;
		if (config.ui?.query_in_title && q.trim()) {
			document.title = `${q} - ${config.instance_name}`;
		}
		return () => {
			document.title = original;
		};
	}, [config, q]);
	useEffect(() => {
		const onKeyDown = (event: globalThis.KeyboardEvent) => {
			const target = event.target as HTMLElement | null;
			const typing =
				target?.tagName === "INPUT" ||
				target?.tagName === "TEXTAREA" ||
				target?.isContentEditable;
			if (event.key === "/" && !typing) {
				event.preventDefault();
				document
					.querySelector<HTMLInputElement>("[data-search-input]")
					?.focus();
				return;
			}
			if (config?.ui?.hotkeys !== "vim" || typing) return;
			if (event.key !== "j" && event.key !== "k") return;
			const links = Array.from(
				document.querySelectorAll<HTMLAnchorElement>("[data-result-link]"),
			);
			if (!links.length) return;
			event.preventDefault();
			const current = links.indexOf(
				document.activeElement as HTMLAnchorElement,
			);
			const delta = event.key === "j" ? 1 : -1;
			links[(current + delta + links.length) % links.length]?.focus();
		};
		document.addEventListener("keydown", onKeyDown);
		return () => document.removeEventListener("keydown", onKeyDown);
	}, [config?.ui?.hotkeys]);
	// Infinite scroll is opt-in via the `infinite_scroll` feature preference.
	const infiniteScroll = pluginEnabled(config, "infinite_scroll", prefs);
	const trackerOn = pluginEnabled(config, "tracker_url_remover", prefs);
	const [trackerReady, setTrackerReady] = useState(!trackerOn);
	useEffect(() => {
		if (!trackerOn) {
			setTrackerReady(true);
			return;
		}
		let cancelled = false;
		setTrackerReady(false);
		void ensureTrackerRules().then(() => {
			if (!cancelled) setTrackerReady(true);
		});
		return () => {
			cancelled = true;
		};
	}, [trackerOn]);
	const query = useInfiniteQuery({
		queryKey: ["search", { ...params, categories: activeCategory }],
		initialPageParam: pageno,
		queryFn: ({ pageParam, signal }) =>
			search(
				{
					...params,
					pageno: pageParam,
					// Always pin a category so the backend doesn't widen the engine set.
					categories: activeCategory,
				},
				signal,
			),
		getNextPageParam: (_lastPage, allPages) => nextPageNumber(allPages, pageno),
		enabled: q.trim().length > 0,
	});

	// Flatten all loaded pages into a single view, de-duplicating by URL so a
	// result that recurs on a later page isn't shown twice.
	const firstPage = query.data?.pages[0];
	const results = useMemo(() => {
		if (!query.data) return [] as SearchResult[];
		const seen = new Set<string>();
		const merged: SearchResult[] = [];
		for (const page of query.data.pages) {
			for (const result of page.results) {
				if (seen.has(result.url)) continue;
				seen.add(result.url);
				merged.push(result);
			}
		}
		// trackerReady: module-level rule cache fills after ensureTrackerRules();
		// keep it in deps so strip re-runs once the lazy JSON chunk loads.
		void trackerReady;
		return applyClientFeatures(merged, config, prefs);
	}, [query.data, config, prefs, trackerReady]);

	const localAnswers = useLocalAnswers(
		q,
		language,
		pageno,
		config,
		prefs,
		activeCategory,
	);
	const answers = presentAnswers(
		localAnswers,
		firstPage?.answers ?? [],
		q,
		firstPage?.interpretation?.intent,
	);
	const [sourcePrefs, setSourcePrefs] = useState<SourcePrefs>(() =>
		loadSourcePrefs(),
	);
	const [undoPrefs, setUndoPrefs] = useState<SourcePrefs | null>(null);
	const sourced = useMemo(
		() => applySourcePrefs(results, sourcePrefs),
		[results, sourcePrefs],
	);
	const visibleResults = sourced.results;
	function commitPrefs(next: SourcePrefs) {
		setUndoPrefs(sourcePrefs);
		setSourcePrefs(next);
		saveSourcePrefs(next);
	}

	const [clientCorrection, setClientCorrection] = useState<string | null>(null);

	// Fallback “Did you mean?” when engines return no corrections and results look weak.
	// ponytail: autocomplete-only hint, no dictionary — skip if autocomplete is off.
	useEffect(() => {
		setClientCorrection(null);
		if (!firstPage || !q || !config?.autocomplete) return;
		if (firstPage.corrections.length > 0) return;
		const weak =
			results.length === 0 ||
			(results.length < 3 && (firstPage.number_of_results ?? 0) < 5);
		if (!weak) return;
		let cancelled = false;
		void autocomplete(q)
			.then((items) => {
				if (cancelled) return;
				setClientCorrection(
					pickDidYouMean(
						q,
						(items ?? []).map((s) => s.text),
					),
				);
			})
			.catch(() => {
				if (!cancelled) setClientCorrection(null);
			});
		return () => {
			cancelled = true;
		};
	}, [
		firstPage,
		q,
		results.length,
		config?.autocomplete,
		firstPage?.corrections.length,
		firstPage?.number_of_results,
	]);

	const imageMode =
		activeCategory === "images" ||
		Boolean(
			visibleResults.length &&
				visibleResults.every((r) => r.kind === "image") &&
				activeCategory !== "videos",
		);
	const videoMode =
		activeCategory === "videos" ||
		Boolean(
			visibleResults.length &&
				visibleResults.every(
					(r) =>
						r.kind === "main" &&
						(Boolean(r.iframe_src) || (Boolean(r.thumbnail) && !r.iframe_src)),
				) &&
				visibleResults.some((r) => r.kind === "main" && Boolean(r.iframe_src)),
		);
	const mapMode = activeCategory === "map";
	const errorStatus =
		query.error instanceof ApiError ? query.error.status : undefined;

	// Auto-load the next page when the sentinel scrolls into view.
	const sentinelRef = useRef<HTMLDivElement | null>(null);
	useEffect(() => {
		if (!infiniteScroll) return;
		const node = sentinelRef.current;
		if (!node) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (
					entries[0]?.isIntersecting &&
					query.hasNextPage &&
					!query.isFetchingNextPage
				) {
					void query.fetchNextPage();
				}
			},
			{ rootMargin: "600px" },
		);
		observer.observe(node);
		return () => observer.disconnect();
	}, [
		infiniteScroll,
		query.hasNextPage,
		query.isFetchingNextPage,
		query.fetchNextPage,
	]);

	const t = stringsFor(language);
	const resultCount =
		visibleResults.length > 0 ? visibleResults.length : undefined;
	const eligibility = engineEligibility(config?.engines ?? [], {
		category: activeCategory,
		timeRange: time_range,
		language: language ?? "",
		safesearch,
		engines: params.engines ?? "",
		pageno,
	});
	const parsedQuery = interpret(q);
	const outcome = firstPage?.outcome ?? "";
	const statsResetKey = [
		q,
		activeCategory,
		language,
		safesearch,
		time_range,
		pageno,
	].join("\0");
	// Parent stays mounted across loading → results; stats only mounts after
	// firstPage exists, so the wall clock must start here.
	const [searchStartedAt, setSearchStartedAt] = useState(() =>
		performance.now(),
	);
	const [trackedStatsKey, setTrackedStatsKey] = useState(statsResetKey);
	if (trackedStatsKey !== statsResetKey) {
		setTrackedStatsKey(statsResetKey);
		setSearchStartedAt(performance.now());
	}

	return (
		<div className="zoeken-serp min-h-dvh text-ink">
			<SearchSerpHeader
				params={params}
				config={config}
				q={q}
				activeCategory={activeCategory}
				time_range={time_range}
				language={language}
				safesearch={safesearch}
			/>

			{!q.trim() ? (
				<p className="mt-16 text-center text-ink-muted">{t.typeAQuery}</p>
			) : null}

			{q.trim() && answers.length > 0 ? (
				<div className="mx-auto max-w-6xl px-3 pt-6 sm:px-6">
					<div className="max-w-[40rem]">
						{answers.map((a) => (
							<InstantAnswerCard
								key={`${a.engine}:${a.answer}`}
								answer={a}
								query={q}
							/>
						))}
					</div>
				</div>
			) : null}

			{query.isLoading ? (
				<div
					className="mx-auto max-w-6xl px-3 pt-6 sm:px-6 sm:pt-8"
					role="status"
					aria-label={t.loadingResults}
				>
					<div className="flex max-w-[40rem] flex-col gap-8">
						{[0, 1, 2, 3].map((i) => (
							<div key={i} className="animate-pulse">
								<div className="flex items-center gap-2.5">
									<div className="size-5 rounded-[5px] bg-line/60" />
									<div className="h-3.5 w-40 rounded bg-line/60" />
								</div>
								<div className="mt-2.5 h-5 w-4/5 rounded bg-line/70" />
								<div className="mt-2 h-3.5 w-full rounded bg-line/50" />
								<div className="mt-1.5 h-3.5 w-2/3 rounded bg-line/50" />
							</div>
						))}
					</div>
				</div>
			) : null}

			{query.isError ? (
				<div className="mx-auto max-w-6xl px-3 pt-6 sm:px-6 sm:pt-8">
					<div className="max-w-[40rem] rounded-2xl border border-line bg-surface-raised px-4 py-4 sm:px-5">
						<p className="font-medium text-ink">
							{errorStatus === 429 ? t.tooManySearches : t.searchUnavailable}
						</p>
						<p className="mt-1 text-sm text-ink-muted">
							{errorStatus === 429 ? t.rateLimitedHint : t.searchErrorHint}
						</p>
						<button
							type="button"
							onClick={() => void query.refetch()}
							className="mt-3 min-h-11 rounded-xl border border-line px-4 text-sm text-ink transition-colors hover:border-accent hover:text-accent"
						>
							{t.retry}
						</button>
					</div>
				</div>
			) : null}

			{firstPage ? (
				<div className="mx-auto max-w-6xl px-3 pt-5 pb-20 sm:px-6 sm:pt-6">
					{outcome === "no_engines" ||
					(config &&
						eligibility.eligible === 0 &&
						visibleResults.length === 0) ? (
						<div
							className="mb-6 max-w-[40rem] rounded-xl border border-line bg-surface-raised px-4 py-4"
							role="status"
						>
							<p className="font-medium text-ink">{t.noEngines}</p>
							<p className="mt-1 text-sm text-ink-muted">{t.noEnginesHint}</p>
							<div className="mt-3 flex flex-wrap gap-2">
								{time_range ? (
									<Link
										to="/search"
										search={searchLink(params, {
											time_range: undefined,
											pageno: undefined,
										})}
										className="inline-flex min-h-11 items-center rounded-lg border border-line px-3 text-sm text-accent no-underline"
									>
										{t.removeTimeFilter}
									</Link>
								) : null}
								{activeCategory !== "general" ? (
									<Link
										to="/search"
										search={searchLink(params, {
											categories: "general",
											pageno: undefined,
										})}
										className="inline-flex min-h-11 items-center rounded-lg border border-line px-3 text-sm text-accent no-underline"
									>
										{t.searchGeneral}
									</Link>
								) : null}
							</div>
						</div>
					) : null}
					{outcome === "failed" || outcome === "rate_limited" ? (
						<div
							className="mb-6 max-w-[40rem] rounded-xl border border-line bg-surface-raised px-4 py-4"
							role="status"
						>
							<p className="font-medium text-ink">
								{outcome === "rate_limited"
									? t.tooManySearches
									: t.searchUnavailable}
							</p>
							<button
								type="button"
								onClick={() => void query.refetch()}
								className="mt-3 min-h-11 rounded-lg border border-line px-3 text-sm text-accent"
							>
								{t.retry}
							</button>
						</div>
					) : null}
					{outcome === "no_matches" &&
					visibleResults.length === 0 &&
					sourced.hiddenCount === 0 ? (
						<div className="mb-6 flex flex-wrap gap-2">
							{time_range ? (
								<Link
									to="/search"
									search={searchLink(params, {
										time_range: undefined,
										pageno: undefined,
									})}
									className="inline-flex min-h-11 items-center rounded-lg border border-line px-3 text-sm text-accent no-underline"
								>
									{t.removeTimeFilter}
								</Link>
							) : null}
							{activeCategory !== "general" ? (
								<Link
									to="/search"
									search={searchLink(params, {
										categories: "general",
										pageno: undefined,
									})}
									className="inline-flex min-h-11 items-center rounded-lg border border-line px-3 text-sm text-accent no-underline"
								>
									{t.searchGeneral}
								</Link>
							) : clientCorrection ? (
								<Link
									to="/search"
									search={searchLink(params, {
										q: clientCorrection,
										pageno: undefined,
									})}
									className="inline-flex min-h-11 items-center rounded-lg border border-line px-3 text-sm text-accent no-underline"
								>
									{clientCorrection}
								</Link>
							) : null}
						</div>
					) : null}
					{(eligibility.timeReduced ||
						eligibility.safeReduced ||
						eligibility.languageReduced) &&
					eligibility.eligible > 0 ? (
						<p className="mb-4 max-w-[40rem] text-sm text-ink-muted">
							{t.sourcesCanApply
								.replace("{eligible}", String(eligibility.eligible))
								.replace("{total}", String(eligibility.inCategory))}
							{eligibility.explicitSkipped.length > 0
								? ` ${t.skippedSources.replace("{names}", eligibility.explicitSkipped.join(", "))}`
								: ""}
						</p>
					) : null}
					<ConstraintChips q={q} params={params} />
					{firstPage.operator_notes?.length ? (
						<ul className="mb-4 max-w-[40rem] text-sm text-ink-muted">
							{firstPage.operator_notes.map((note) => (
								<li key={note}>{note}</li>
							))}
						</ul>
					) : null}
					{parsedQuery.operators.phrases.length > 0 &&
					!firstPage.operator_notes?.length ? (
						<p className="mb-4 max-w-[40rem] text-sm text-ink-muted">
							{t.quotedPhrases}
						</p>
					) : null}
					{firstPage.unresponsive_engines.length > 0 &&
					visibleResults.length > 0 ? (
						<details className="mb-6 max-w-[40rem] rounded-xl border border-amber-500/30 bg-amber-500/5 px-4 py-3">
							<summary className="cursor-pointer text-sm font-medium text-ink">
								{t.partialSources} ({firstPage.unresponsive_engines.length})
							</summary>
							<ul className="mt-2 divide-y divide-line/60">
								{firstPage.unresponsive_engines.map(({ engine, cause }) => (
									<li
										key={`${engine}:${cause}`}
										className="flex flex-wrap items-baseline justify-between gap-x-3 py-1.5 text-sm"
									>
										<span className="font-medium capitalize text-ink">
											{formatEngineLabel(engine)}
										</span>
										<span className="text-ink-muted">{cause}</span>
									</li>
								))}
							</ul>
						</details>
					) : null}
					{undoPrefs ? (
						<p
							className="mb-4 flex items-center gap-3 text-sm text-ink"
							role="status"
						>
							{t.siteListUpdated}
							<button
								type="button"
								className="min-h-11 font-medium text-accent"
								onClick={() => {
									setSourcePrefs(undoPrefs);
									saveSourcePrefs(undoPrefs);
									setUndoPrefs(null);
								}}
							>
								{t.undo}
							</button>
						</p>
					) : null}
					{sourced.hiddenCount > 0 && visibleResults.length === 0 ? (
						<div
							className="mb-6 max-w-[40rem] rounded-xl border border-line px-4 py-4"
							role="status"
						>
							<p className="font-medium text-ink">{t.hiddenAll}</p>
							<button
								type="button"
								className="mt-2 min-h-11 text-sm text-accent"
								onClick={() => commitPrefs({ ...sourcePrefs, hidden: [] })}
							>
								{t.showHidden}
							</button>
						</div>
					) : null}

					<SearchResultStats
						resultCount={resultCount}
						retrievedLabel={t.resultsRetrieved}
						hasResults={visibleResults.length > 0}
						searchStartedAt={searchStartedAt}
						isSuccess={query.isSuccess}
						isFetching={query.isFetching}
						dataUpdatedAt={query.dataUpdatedAt}
					/>

					{firstPage.corrections.length > 0 ? (
						<p className="mb-6 text-ink-muted">
							{t.didYouMean}{" "}
							{firstPage.corrections.map((c, i) => {
								const text = correctionText(c);
								const className =
									"font-medium text-accent italic hover:underline";
								return (
									<span key={`${text}:${c.engine}:${c.url ?? ""}`}>
										{i > 0 ? ", " : null}
										{c.url ? (
											<a href={c.url} className={className}>
												{text}
											</a>
										) : (
											<Link
												to="/search"
												search={searchLink(params, {
													q: text,
													pageno: undefined,
												})}
												className={className}
											>
												{text}
											</Link>
										)}
									</span>
								);
							})}
							?
						</p>
					) : clientCorrection ? (
						<p className="mb-6 text-ink-muted">
							{t.didYouMean}{" "}
							<Link
								to="/search"
								search={searchLink(params, {
									q: clientCorrection,
									pageno: undefined,
								})}
								className="font-medium text-accent italic hover:underline"
							>
								{clientCorrection}
							</Link>
							?
						</p>
					) : null}

					<div className="flex flex-col gap-10 lg:flex-row lg:items-start lg:gap-12">
						<div className="min-w-0 flex-1">
							<SearchResultList
								results={visibleResults}
								query={q}
								onHideSite={(domain) =>
									commitPrefs(toggleHidden(sourcePrefs, domain))
								}
								onPreferSite={(domain) =>
									commitPrefs(togglePreferred(sourcePrefs, domain))
								}
								activeCategory={activeCategory}
								videoMode={videoMode}
								imageMode={imageMode}
								mapMode={mapMode}
								newTab={config?.ui?.results_on_new_tab}
								urlFormatting={config?.ui?.url_formatting}
								cacheUrl={config?.ui?.cache_url}
								empty={
									visibleResults.length === 0 &&
									answers.length === 0 &&
									firstPage.infoboxes.length === 0 &&
									sourced.hiddenCount === 0
								}
								emptyTitle={
									outcome === "failed"
										? t.searchUnavailable
										: outcome === "rate_limited"
											? t.tooManySearches
											: `${t.noResults} · “${q}”`
								}
								emptyHint={
									outcome === "rate_limited"
										? t.rateLimitedHint
										: t.tryDifferent
								}
							/>

							{firstPage.suggestions.length > 0 ? (
								<section className="mt-14 max-w-[38rem]">
									<h2 className="mb-3 text-base font-medium text-ink">
										{t.relatedSearches}
									</h2>
									<div className="flex flex-wrap gap-2">
										{firstPage.suggestions.map((s) => {
											const text = suggestionText(s);
											return (
												<Link
													key={text}
													to="/search"
													search={searchLink(params, {
														q: text,
														pageno: undefined,
													})}
													className="rounded-xl border border-line bg-surface-raised px-3.5 py-1.5 text-sm text-ink no-underline transition-colors hover:border-accent hover:text-accent"
												>
													{text}
												</Link>
											);
										})}
									</div>
								</section>
							) : null}

							{(visibleResults.length > 0 || showNumberedNext(firstPage)) &&
							infiniteScroll ? (
								<div className="mt-12 flex flex-col items-center gap-3">
									{query.hasNextPage ? (
										<button
											type="button"
											onClick={() => void query.fetchNextPage()}
											disabled={query.isFetchingNextPage}
											className="rounded-lg border border-line px-4 py-2 text-sm text-accent transition-colors hover:bg-accent-soft disabled:opacity-60"
										>
											{query.isFetchingNextPage ? "…" : t.loadMore}
										</button>
									) : (
										<p className="text-sm text-ink-subtle">{t.endOfResults}</p>
									)}
									<div ref={sentinelRef} aria-hidden className="h-px w-full" />
								</div>
							) : visibleResults.length > 0 || showNumberedNext(firstPage) ? (
								<nav
									aria-label={t.pagination}
									className="mt-12 flex max-w-[40rem] flex-wrap items-center gap-x-1 gap-y-2 text-[0.95rem]"
								>
									{pageno > 1 ? (
										<Link
											to="/search"
											search={searchLink(params, { pageno: pageno - 1 })}
											className="mr-1 rounded-lg px-3 py-1.5 text-accent no-underline transition-colors hover:bg-accent-soft"
										>
											‹ {t.previousPage}
										</Link>
									) : null}
									{pageNumbers(pageno).map((page) =>
										page === pageno ? (
											<span
												key={page}
												aria-current="page"
												className="min-w-9 rounded-lg bg-accent-soft px-2 py-1.5 text-center font-semibold text-ink"
											>
												{page}
											</span>
										) : (
											<Link
												key={page}
												to="/search"
												search={searchLink(params, {
													pageno: page === 1 ? undefined : page,
												})}
												className="hidden min-w-9 rounded-lg px-2 py-1.5 text-center text-accent no-underline transition-colors hover:bg-accent-soft sm:block"
											>
												{page}
											</Link>
										),
									)}
									{showNumberedNext(firstPage) ? (
										<Link
											to="/search"
											search={searchLink(params, { pageno: pageno + 1 })}
											className="ml-1 inline-flex min-h-11 items-center rounded-lg px-3 text-accent no-underline transition-colors hover:bg-accent-soft"
										>
											{t.nextPage} ›
										</Link>
									) : (
										<span className="ml-1 text-sm text-ink-subtle">
											{t.endOfResults}
										</span>
									)}
								</nav>
							) : null}
						</div>

						{firstPage.infoboxes.length > 0 ? (
							<aside className="w-full shrink-0 lg:w-[19rem]">
								{firstPage.infoboxes.map((infobox, index) => (
									<InfoboxCard
										key={
											infobox.id ||
											`${infobox.engine ?? "box"}:${infobox.infobox}:${index}`
										}
										box={infobox}
									/>
								))}
							</aside>
						) : null}
					</div>
				</div>
			) : null}
		</div>
	);
}

function ConstraintChips({
	q,
	params,
}: {
	q: string;
	params: ReturnType<typeof Route.useSearch>;
}) {
	const ops = interpret(q).operators;
	const chips: Array<{ label: string; next: string }> = [];
	if (ops.site) {
		chips.push({
			label: `site:${ops.site}`,
			next: removeOperator(q, "site", ops.site),
		});
	}
	if (ops.filetype) {
		chips.push({
			label: `filetype:${ops.filetype}`,
			next: removeOperator(q, "filetype", ops.filetype),
		});
	}
	for (const phrase of ops.phrases) {
		chips.push({
			label: `"${phrase}"`,
			next: removeOperator(q, "phrase", phrase),
		});
	}
	for (const term of ops.excluded) {
		chips.push({
			label: `-${term}`,
			next: removeOperator(q, "excluded", term),
		});
	}
	if (chips.length === 0) return null;
	return (
		<ul
			className="mb-4 flex max-w-[40rem] flex-wrap gap-2"
			aria-label={activeStrings().activeConstraints}
		>
			{chips.map((chip) => (
				<li key={chip.label}>
					<Link
						to="/search"
						search={searchLink(params, { q: chip.next, pageno: undefined })}
						className="inline-flex min-h-11 items-center gap-2 rounded-full border border-line bg-surface-raised px-3 text-sm text-ink no-underline hover:border-accent"
					>
						{chip.label}
						<span aria-hidden>×</span>
						<span className="sr-only">Remove constraint</span>
					</Link>
				</li>
			))}
		</ul>
	);
}
