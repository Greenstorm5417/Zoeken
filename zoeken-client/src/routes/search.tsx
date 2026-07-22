import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { Settings2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { InstantAnswerCard } from "#/components/answers/InstantAnswerCard";
import { ImageLightbox } from "#/components/ImageLightbox";
import { InfoboxCard } from "#/components/InfoboxCard";
import { SearchForm } from "#/components/SearchForm";
import { SearchResultList } from "#/components/SearchResultList";
import { SelectMenu } from "#/components/SelectMenu";
import {
	ApiError,
	autocomplete,
	preferencesGet,
	type SearchResult,
	search,
} from "#/lib/api";
import { applyClientFeatures, pluginEnabled } from "#/lib/clientFeatures";
import { pickDidYouMean } from "#/lib/didYouMean";
import { stringsFor } from "#/lib/i18n";
import {
	correctionText,
	DEFAULT_CATEGORIES,
	formatEngineLabel,
	pageNumbers,
	searchLink,
	suggestionText,
} from "#/lib/searchDisplay";
import { parseSearchParams } from "#/lib/searchParams";
import { useLocalAnswers } from "#/lib/useLocalAnswers";
import { useConfig } from "./__root";

export const Route = createFileRoute("/search")({
	validateSearch: parseSearchParams,
	component: SearchPage,
});

function SearchPage() {
	const params = Route.useSearch();
	const navigate = useNavigate();
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
	const [pendingCategory, setPendingCategory] = useState(activeCategory);
	useEffect(() => setPendingCategory(activeCategory), [activeCategory]);
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
	const query = useInfiniteQuery({
		queryKey: ["search", { ...params, categories: activeCategory }],
		initialPageParam: pageno,
		queryFn: ({ pageParam }) =>
			search({
				...params,
				pageno: pageParam,
				// Always pin a category so the backend doesn't widen the engine set.
				categories: activeCategory,
			}),
		getNextPageParam: (lastPage, allPages) =>
			lastPage.results.length > 0 ? pageno + allPages.length : undefined,
		enabled: q.trim().length > 0,
	});

	// Flatten all loaded pages into a single view, de-duplicating by URL so a
	// result that recurs on a later page isn't shown twice.
	const firstPage = query.data?.pages[0];
	const results = (() => {
		if (!query.data) return [];
		const seen = new Set<string>();
		const merged: SearchResult[] = [];
		for (const page of query.data.pages) {
			for (const result of page.results) {
				if (seen.has(result.url)) continue;
				seen.add(result.url);
				merged.push(result);
			}
		}
		return applyClientFeatures(merged, config, prefs);
	})();

	const localAnswers = useLocalAnswers(q, language, pageno, config, prefs);
	const answers = [...localAnswers, ...(firstPage?.answers ?? [])];

	const [lightbox, setLightbox] = useState<SearchResult | null>(null);
	const [elapsedSec, setElapsedSec] = useState<number | null>(null);
	const [clientCorrection, setClientCorrection] = useState<string | null>(null);
	const fetchStarted = useRef(0);
	// biome-ignore lint/correctness/useExhaustiveDependencies: reset timer when search params change; deps are intentional triggers
	useEffect(() => {
		fetchStarted.current = performance.now();
		setElapsedSec(null);
	}, [q, activeCategory, language, safesearch, time_range, pageno]);
	// biome-ignore lint/correctness/useExhaustiveDependencies: dataUpdatedAt re-runs when a fresh page lands
	useEffect(() => {
		if (query.isSuccess && !query.isFetching && fetchStarted.current) {
			setElapsedSec((performance.now() - fetchStarted.current) / 1000);
		}
	}, [query.isSuccess, query.isFetching, query.dataUpdatedAt]);

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
			results.length &&
				results.every((r) => r.kind === "image") &&
				activeCategory !== "videos",
		);
	const videoMode =
		activeCategory === "videos" ||
		Boolean(
			results.length &&
				results.every(
					(r) =>
						r.kind === "main" &&
						(Boolean(r.iframe_src) || (Boolean(r.thumbnail) && !r.iframe_src)),
				) &&
				results.some((r) => r.kind === "main" && Boolean(r.iframe_src)),
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

	const available = new Set(
		(config?.engines ?? [])
			.filter((engine) => engine.enabled)
			.flatMap((engine) => engine.categories.map((c) => c.toLowerCase())),
	);
	available.add("general");
	const configuredCategories = config?.categories_as_tabs?.length
		? config.categories_as_tabs
		: DEFAULT_CATEGORIES;
	const categoriesList = configuredCategories.filter((category) =>
		available.has(category),
	);

	// Hide filters the active category's engines don't support.
	const categoryEngines = (config?.engines ?? []).filter(
		(engine) =>
			engine.enabled &&
			(activeCategory === "general" ||
				engine.categories.map((c) => c.toLowerCase()).includes(activeCategory)),
	);
	const showTimeRange = categoryEngines.some((e) => e.time_range_support);
	const showSafesearch = categoryEngines.some((e) => e.safesearch);
	// Locale codes (en-US, nl-BE, …) are the region control — no separate region param.
	const showLanguage = categoryEngines.some((e) => e.language_support);

	const languageOptions = [
		{ value: "", label: "Any language / region" },
		...Object.entries(config?.locales ?? {}).map(([code, name]) => ({
			value: code,
			label: name,
		})),
	];
	const onLanguageChange = (next: string) =>
		void navigate({
			to: "/search",
			search: searchLink(params, {
				language: next || undefined,
				pageno: undefined,
			}),
		});
	const onSafesearchChange = (next: string) =>
		void navigate({
			to: "/search",
			search: searchLink(params, {
				safesearch: Number(next) as 0 | 1 | 2,
				pageno: undefined,
			}),
		});
	const onTimeRangeChange = (next: string) =>
		void navigate({
			to: "/search",
			search: searchLink(params, {
				time_range: next || undefined,
				pageno: undefined,
			}),
		});

	const t = stringsFor(language);
	const timeRangeOptions = [
		{ value: "", label: t.anyTime },
		{ value: "day", label: t.pastDay },
		{ value: "week", label: t.pastWeek },
		{ value: "month", label: t.pastMonth },
		{ value: "year", label: t.pastYear },
	];
	const safesearchOptions = [
		{ value: "0", label: t.safeSearchOff },
		{ value: "1", label: t.moderate },
		{ value: "2", label: t.strict },
	];

	const filterMenus = (
		<>
			{showTimeRange ? (
				<SelectMenu
					label="Time range"
					value={time_range}
					options={timeRangeOptions}
					onChange={onTimeRangeChange}
				/>
			) : null}
			{showLanguage ? (
				<SelectMenu
					label="Language / region"
					value={language ?? ""}
					options={languageOptions}
					onChange={onLanguageChange}
				/>
			) : null}
			{showSafesearch ? (
				<SelectMenu
					label="Safe search"
					value={String(safesearch)}
					options={safesearchOptions}
					onChange={onSafesearchChange}
				/>
			) : null}
		</>
	);

	const resultCount =
		firstPage?.number_of_results ??
		(results.length > 0 ? results.length : undefined);

	return (
		<div className="zoeken-serp min-h-dvh text-ink">
			<header className="sticky top-0 z-20 border-b border-line bg-surface">
				<div className="mx-auto flex max-w-6xl items-center gap-2 px-3 pt-3 pb-2.5 sm:gap-4 sm:px-6">
					<Link
						to="/"
						className="shrink-0 no-underline"
						aria-label="Zoeken home"
					>
						<img src="/zoeken-logo.svg" alt="" width={32} height={32} />
					</Link>
					<div className="w-full min-w-0 max-w-[40rem] flex-1">
						<SearchForm key={q} initialQuery={q} compact baseSearch={params} />
					</div>
					<div className="ml-auto flex shrink-0 items-center gap-1">
						{q.trim() ? (
							<div className="hidden items-center gap-2 lg:flex">
								{filterMenus}
							</div>
						) : null}
						<nav className="flex items-center text-sm">
							<Link
								to="/preferences"
								className="hidden rounded-lg px-3 py-1.5 text-ink-muted no-underline transition-colors hover:bg-accent-soft hover:text-ink md:block"
							>
								Preferences
							</Link>
							<Link
								to="/about"
								className="hidden rounded-lg px-3 py-1.5 text-ink-muted no-underline transition-colors hover:bg-accent-soft hover:text-ink md:block"
							>
								About
							</Link>
							<Link
								to="/preferences"
								aria-label="Preferences"
								className="rounded-lg p-2 text-ink-muted transition-colors hover:bg-accent-soft hover:text-ink md:hidden"
							>
								<Settings2 className="size-5" aria-hidden />
							</Link>
						</nav>
					</div>
				</div>

				{q.trim() ? (
					<div className="mx-auto flex max-w-6xl items-end gap-1 overflow-x-auto px-3 sm:px-6">
						{categoriesList.map((category) => {
							const active =
								(config?.ui?.search_on_category_select === false
									? pendingCategory
									: activeCategory) === category;
							return (
								<Link
									key={category}
									to="/search"
									search={searchLink(params, {
										categories: category === "general" ? undefined : category,
										pageno: undefined,
									})}
									onClick={(event) => {
										if (config?.ui?.search_on_category_select === false) {
											event.preventDefault();
											setPendingCategory(category);
										}
									}}
									className={[
										"shrink-0 border-b-2 px-3 pb-2.5 text-sm capitalize no-underline transition-colors duration-100",
										active
											? "border-accent font-medium text-accent"
											: "border-transparent text-ink-muted hover:text-ink",
									].join(" ")}
								>
									{category === "general" ? "All" : category}
								</Link>
							);
						})}
						{config?.ui?.search_on_category_select === false &&
						pendingCategory !== activeCategory ? (
							<button
								type="button"
								className="mb-2 ml-2 shrink-0 text-sm font-medium text-accent"
								onClick={() =>
									void navigate({
										to: "/search",
										search: searchLink(params, {
											categories:
												pendingCategory === "general"
													? undefined
													: pendingCategory,
											pageno: undefined,
										}),
									})
								}
							>
								Search
							</button>
						) : null}
					</div>
				) : null}

				{q.trim() && (showTimeRange || showLanguage || showSafesearch) ? (
					<div className="mx-auto flex max-w-6xl items-center gap-2 overflow-x-auto border-t border-line/60 px-3 py-2 sm:px-6 lg:hidden">
						{filterMenus}
					</div>
				) : null}
			</header>

			{!q.trim() ? (
				<p className="mt-16 text-center text-ink-muted">
					Type a query to search.
				</p>
			) : null}

			{query.isLoading ? (
				<div
					className="mx-auto max-w-6xl px-4 pt-8 sm:px-6"
					role="status"
					aria-label="Loading results"
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
				<div className="mx-auto max-w-6xl px-4 pt-8 sm:px-6">
					<div className="max-w-[40rem] rounded-2xl border border-line bg-surface-raised px-5 py-4">
						<p className="font-medium text-ink">
							{errorStatus === 429 ? t.tooManySearches : t.searchUnavailable}
						</p>
						<p className="mt-1 text-sm text-ink-muted">
							{errorStatus === 429
								? "Please wait a moment and try again."
								: "Something went wrong reaching the search backend. Try again in a moment."}
						</p>
						<button
							type="button"
							onClick={() => void query.refetch()}
							className="mt-3 rounded-lg border border-line px-3 py-1.5 text-sm text-ink transition-colors hover:border-accent hover:text-accent"
						>
							Retry
						</button>
					</div>
				</div>
			) : null}

			{firstPage ? (
				<div className="mx-auto max-w-6xl px-4 pt-6 pb-20 sm:px-6">
					{firstPage.unresponsive_engines.length > 0 ? (
						<aside className="mb-6 max-w-[40rem] rounded-xl border border-amber-500/30 bg-amber-500/5 px-4 py-3">
							<p className="text-sm font-medium text-ink">
								{firstPage.unresponsive_engines.length} engine
								{firstPage.unresponsive_engines.length === 1 ? "" : "s"}{" "}
								{t.enginesDidntRespond}
							</p>
							<ul className="mt-2 divide-y divide-line/60">
								{firstPage.unresponsive_engines.map(({ engine, cause }) => (
									<li
										key={`${engine}:${cause}`}
										className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5 py-1.5 text-sm"
									>
										<span className="font-medium capitalize text-ink">
											{formatEngineLabel(engine)}
										</span>
										<span className="text-ink-muted">{cause}</span>
									</li>
								))}
							</ul>
						</aside>
					) : null}

					{(resultCount != null || elapsedSec != null) && results.length > 0 ? (
						<p className="mb-4 max-w-[40rem] text-sm text-ink-subtle">
							{resultCount != null ? (
								<>
									About {resultCount.toLocaleString()} result
									{resultCount === 1 ? "" : "s"}
								</>
							) : null}
							{resultCount != null && elapsedSec != null ? " · " : null}
							{elapsedSec != null ? `${elapsedSec.toFixed(2)}s` : null}
						</p>
					) : null}

					{firstPage.corrections.length > 0 ? (
						<p className="mb-6 text-ink-muted">
							Did you mean{" "}
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
							Did you mean{" "}
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
							{answers.map((a) => (
								<InstantAnswerCard key={a.answer} answer={a} />
							))}

							<SearchResultList
								results={results}
								activeCategory={activeCategory}
								videoMode={videoMode}
								imageMode={imageMode}
								mapMode={mapMode}
								newTab={config?.ui?.results_on_new_tab}
								urlFormatting={config?.ui?.url_formatting}
								cacheUrl={config?.ui?.cache_url}
								onOpenImage={setLightbox}
								empty={
									results.length === 0 &&
									answers.length === 0 &&
									firstPage.infoboxes.length === 0
								}
								emptyTitle={`${t.noResults} · “${q}”`}
								emptyHint={t.tryDifferent}
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

							{results.length > 0 && infiniteScroll ? (
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
							) : results.length > 0 ? (
								<nav
									aria-label="Pagination"
									className="mt-12 flex max-w-[40rem] flex-wrap items-center gap-x-1 gap-y-2 text-[0.95rem]"
								>
									{pageno > 1 ? (
										<Link
											to="/search"
											search={searchLink(params, { pageno: pageno - 1 })}
											className="mr-1 rounded-lg px-3 py-1.5 text-accent no-underline transition-colors hover:bg-accent-soft"
										>
											‹ Previous
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
									<Link
										to="/search"
										search={searchLink(params, { pageno: pageno + 1 })}
										className="ml-1 rounded-lg px-3 py-1.5 text-accent no-underline transition-colors hover:bg-accent-soft"
									>
										Next ›
									</Link>
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

			{lightbox ? (
				<ImageLightbox result={lightbox} onClose={() => setLightbox(null)} />
			) : null}
		</div>
	);
}
