import { useInfiniteQuery } from "@tanstack/react-query";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import {
	ArrowLeftRight,
	Calculator,
	CloudSun,
	Settings2,
	Sigma,
	Sparkles,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { ImageLightbox } from "#/components/ImageLightbox";
import { MapResult, specializedTemplate } from "#/components/ResultTemplates";
import { SearchForm } from "#/components/SearchForm";
import { SelectMenu } from "#/components/SelectMenu";
import {
	ApiError,
	type Infobox,
	type SearchAnswer,
	type SearchResult,
	search,
} from "#/lib/api";
import { stringsFor } from "#/lib/i18n";
import {
	parseSearchParams,
	type SearchRouteParams,
	serializeSearchParams,
} from "#/lib/searchParams";
import { useConfig } from "./__root";

export const Route = createFileRoute("/search")({
	validateSearch: parseSearchParams,
	component: SearchPage,
});

/** Keep the tab strip short — only surface the common buckets. */
const DEFAULT_CATEGORIES = [
	"general",
	"images",
	"videos",
	"news",
	"map",
] as const;

function suggestionText(s: string | { suggestion: string }) {
	return typeof s === "string" ? s : s.suggestion;
}

function correctionText(c: string | { correction: string }) {
	return typeof c === "string" ? c : c.correction;
}

function hostnameOf(url: string) {
	try {
		return new URL(url).hostname.replace(/^www\./, "");
	} catch {
		return url;
	}
}

function pathOf(url: string) {
	try {
		const u = new URL(url);
		const path = u.pathname.replace(/\/$/, "") || "";
		return path === "/" ? "" : path;
	} catch {
		return "";
	}
}

function engineNames(result: SearchResult): string[] {
	const names =
		result.engines && result.engines.length > 0
			? result.engines
			: result.engine
				? [result.engine]
				: [];
	return [...new Set(names.filter(Boolean))];
}

function formatEngineLabel(name: string) {
	return name.replace(/[_-]+/g, " ");
}

function wikidataId(id: string | null | undefined): string | null {
	if (!id) return null;
	const match = id.match(/\/(Q\d+)\s*$/i) || id.match(/^(Q\d+)$/i);
	return match ? match[1].toUpperCase() : null;
}

function searchLink(
	search: SearchRouteParams,
	updates: Partial<SearchRouteParams>,
) {
	return serializeSearchParams({ ...search, ...updates });
}

/** Sliding window of page numbers (SearXNG-style): 1–10, then centered on current. */
function pageNumbers(pageno: number): number[] {
	const start = pageno > 5 ? pageno - 4 : 1;
	return Array.from({ length: 10 }, (_, i) => start + i);
}

function ResultItem({
	result,
	newTab = false,
	urlFormatting = "pretty",
	cacheUrl = "",
}: {
	result: SearchResult;
	newTab?: boolean;
	urlFormatting?: string;
	cacheUrl?: string;
}) {
	const host = hostnameOf(result.url);
	const crumbs = pathOf(result.url)
		.split("/")
		.filter(Boolean)
		.slice(0, 3)
		.join(" > ");
	const engines = engineNames(result);
	const displayUrl =
		urlFormatting === "full"
			? result.url
			: urlFormatting === "host"
				? host
				: `${host}${crumbs ? ` > ${crumbs}` : ""}`;

	return (
		<article className="max-w-[40rem]">
			<a
				data-result-link
				href={result.url}
				target={newTab ? "_blank" : undefined}
				rel={newTab ? "noopener noreferrer" : undefined}
				className="group block no-underline"
			>
				<div className="flex items-center gap-2.5">
					{result.favicon ? (
						<img
							src={result.favicon}
							alt=""
							width={20}
							height={20}
							className="size-5 rounded-[5px] bg-surface-raised ring-1 ring-line/80"
							loading="lazy"
							onError={(event) => {
								event.currentTarget.hidden = true;
							}}
						/>
					) : null}
					<div className="min-w-0">
						<p className="truncate text-[0.875rem] leading-tight text-ink">
							{host}
						</p>
						<p className="truncate text-[0.75rem] leading-tight text-ink-subtle">
							{displayUrl}
						</p>
					</div>
				</div>
				<h2 className="mt-1.5 text-[1.25rem] leading-snug font-medium tracking-tight text-accent transition-colors group-hover:underline">
					{result.title}
				</h2>
			</a>
			{result.content ? (
				<p className="mt-1.5 line-clamp-2 text-[0.9rem] leading-relaxed text-ink-muted">
					{result.content}
				</p>
			) : null}
			{engines.length > 0 || cacheUrl ? (
				<p className="mt-1.5 text-[0.75rem] text-ink-subtle">
					{engines.map(formatEngineLabel).join(" · ")}
					{cacheUrl ? (
						<>
							{engines.length > 0 ? <span aria-hidden> · </span> : null}
							<a
								href={cacheUrl + encodeURIComponent(result.url)}
								target="_blank"
								rel="noopener noreferrer"
								className="text-ink-subtle underline decoration-line underline-offset-2 hover:text-accent"
							>
								archive
							</a>
						</>
					) : null}
				</p>
			) : null}
		</article>
	);
}

/** Icon + label for a local instant answer, keyed on the answering engine. */
function answerKind(engine: string | undefined): {
	Icon: typeof Sparkles;
	label: string;
} {
	const name = (engine ?? "").toLowerCase();
	if (name === "calculator") return { Icon: Calculator, label: "Calculator" };
	if (name === "unit converter")
		return { Icon: ArrowLeftRight, label: "Unit converter" };
	if (name === "weather") return { Icon: CloudSun, label: "Weather" };
	if (name.startsWith("answerer:"))
		return { Icon: Sigma, label: formatEngineLabel(name.slice(9).trim()) };
	return { Icon: Sparkles, label: formatEngineLabel(name || "Answer") };
}

/** `10 km = 6.21 mi` → emphasize the result side of the equation. */
function splitEquation(text: string): [string, string] | null {
	const index = text.lastIndexOf(" = ");
	if (index <= 0) return null;
	return [text.slice(0, index), text.slice(index + 3)];
}

function InstantAnswerCard({ answer }: { answer: SearchAnswer }) {
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

function InfoboxCard({ box }: { box: Infobox }) {
	const title = box.infobox || "Info";
	const qid = wikidataId(box.id);
	const source = box.engine ? formatEngineLabel(box.engine) : null;
	const primaryUrl = box.urls?.[0]?.url ?? box.id ?? undefined;

	return (
		<article className="mb-4 overflow-hidden rounded-2xl border border-line bg-surface-raised">
			{box.img_src ? (
				<img
					src={box.img_src}
					alt=""
					className="max-h-44 w-full object-cover"
				/>
			) : null}
			<div className="p-4">
				{source ? (
					<p className="mb-1.5 text-[0.7rem] font-medium tracking-wide text-ink-subtle uppercase">
						{source}
					</p>
				) : null}
				<h3 className="text-base font-medium text-ink">
					{primaryUrl ? (
						<a
							href={primaryUrl}
							target="_blank"
							rel="noopener noreferrer"
							className="text-ink no-underline hover:text-accent hover:underline"
						>
							{title}
						</a>
					) : (
						title
					)}
				</h3>
				{qid ? (
					<p className="mt-1 font-mono text-xs text-ink-subtle">{qid}</p>
				) : null}
				{box.content ? (
					<p className="mt-2 text-sm leading-relaxed text-ink-muted">
						{box.content}
					</p>
				) : null}
				{box.urls && box.urls.length > 0 ? (
					<ul className="mt-3 flex flex-col gap-1">
						{box.urls.map((link) => (
							<li key={link.url}>
								<a
									href={link.url}
									target="_blank"
									rel="noopener noreferrer"
									className="text-sm text-accent hover:underline"
								>
									{link.title || "Source"}
								</a>
							</li>
						))}
					</ul>
				) : null}
			</div>
		</article>
	);
}

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
	// Infinite scroll is opt-in via the `infinite_scroll` plugin preference.
	const infiniteScroll = Boolean(
		config?.plugins?.find((p) => p.id === "infinite_scroll")?.enabled,
	);
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
		return merged;
	})();

	const [lightbox, setLightbox] = useState<SearchResult | null>(null);

	const imageMode =
		activeCategory === "images" ||
		Boolean(
			results.length &&
				results.every((r) => r.img_src) &&
				activeCategory !== "videos",
		);
	const videoMode =
		activeCategory === "videos" ||
		Boolean(
			results.length &&
				results.every(
					(r) =>
						r.template === "videos.html" ||
						Boolean(r.iframe_src) ||
						(Boolean(r.thumbnail) && !r.img_src),
				),
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

	const languageOptions = [
		{ value: "", label: "Any language" },
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
								<SelectMenu
									label="Time range"
									value={time_range}
									options={timeRangeOptions}
									onChange={onTimeRangeChange}
								/>
								<SelectMenu
									label="Language"
									value={language ?? ""}
									options={languageOptions}
									onChange={onLanguageChange}
								/>
								<SelectMenu
									label="Safe search"
									value={String(safesearch)}
									options={safesearchOptions}
									onChange={onSafesearchChange}
								/>
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

				{q.trim() ? (
					<div className="mx-auto flex max-w-6xl items-center gap-2 overflow-x-auto border-t border-line/60 px-3 py-2 sm:px-6 lg:hidden">
						<SelectMenu
							label="Time range"
							value={time_range}
							options={timeRangeOptions}
							onChange={onTimeRangeChange}
						/>
						<SelectMenu
							label="Language"
							value={language ?? ""}
							options={languageOptions}
							onChange={onLanguageChange}
						/>
						<SelectMenu
							label="Safe search"
							value={String(safesearch)}
							options={safesearchOptions}
							onChange={onSafesearchChange}
						/>
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
				<div
					className={[
						"mx-auto max-w-6xl px-4 pt-6 pb-20 sm:px-6",
						config?.ui?.center_alignment ? "" : "",
					].join(" ")}
				>
					{firstPage.unresponsive_engines.length > 0 ? (
						<aside className="mb-6 max-w-[40rem] rounded-xl border border-line bg-surface-raised px-4 py-3">
							<p className="text-sm font-medium text-ink">
								{firstPage.unresponsive_engines.length} engine
								{firstPage.unresponsive_engines.length === 1 ? "" : "s"} didn’t
								respond
							</p>
							<ul className="mt-2 flex flex-col gap-1">
								{firstPage.unresponsive_engines.map(([engine, reason]) => (
									<li
										key={`${engine}:${reason}`}
										className="flex flex-wrap items-baseline gap-x-2 text-sm text-ink-muted"
									>
										<span className="font-medium text-ink">
											{formatEngineLabel(engine)}
										</span>
										<span className="text-ink-subtle">{reason}</span>
									</li>
								))}
							</ul>
						</aside>
					) : null}

					{firstPage.corrections.length > 0 ? (
						<p className="mb-6 text-ink-muted">
							Did you mean{" "}
							{firstPage.corrections.map((c, i) => {
								const text = correctionText(c);
								return (
									<span key={text}>
										{i > 0 ? ", " : null}
										<Link
											to="/search"
											search={searchLink(params, {
												q: text,
												pageno: undefined,
											})}
											className="font-medium text-accent italic hover:underline"
										>
											{text}
										</Link>
									</span>
								);
							})}
							?
						</p>
					) : null}

					<div className="flex flex-col gap-10 lg:flex-row lg:items-start lg:gap-12">
						<div className="min-w-0 flex-1">
							{firstPage.answers.map((a) => (
								<InstantAnswerCard key={a.answer} answer={a} />
							))}

							{results.length === 0 &&
							firstPage.answers.length === 0 &&
							firstPage.infoboxes.length === 0 ? (
								<div className="max-w-[40rem] rounded-2xl border border-line bg-surface-raised px-5 py-4">
									<p className="font-medium text-ink">
										{t.noResults} · “{q}”
									</p>
									<p className="mt-1 text-sm text-ink-muted">
										{t.tryDifferent}
									</p>
								</div>
							) : results.length === 0 ? null : videoMode ? (
								<ul className="grid max-w-5xl grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
									{results.map((result) => {
										const thumb = result.thumbnail || result.img_src;
										return (
											<li key={result.url}>
												<a
													data-result-link
													href={result.url}
													target={
														config?.ui?.results_on_new_tab
															? "_blank"
															: undefined
													}
													rel={
														config?.ui?.results_on_new_tab
															? "noopener noreferrer"
															: undefined
													}
													className="group block overflow-hidden rounded-xl border border-line bg-surface-raised no-underline"
												>
													<div className="aspect-video bg-ink/5">
														{thumb ? (
															<img
																src={thumb}
																alt=""
																className="size-full object-cover transition-transform duration-150 group-hover:scale-[1.01]"
																loading="lazy"
															/>
														) : (
															<div className="flex size-full items-center justify-center text-sm text-ink-subtle">
																Video
															</div>
														)}
													</div>
													<div className="p-3">
														<p className="line-clamp-2 text-sm font-medium text-ink group-hover:text-accent">
															{result.title}
														</p>
														{result.content ? (
															<p className="mt-1 line-clamp-2 text-xs text-ink-muted">
																{result.content}
															</p>
														) : null}
														{engineNames(result).length ? (
															<p className="mt-1.5 truncate text-[0.65rem] text-ink-subtle">
																{engineNames(result)
																	.map(formatEngineLabel)
																	.join(" · ")}
															</p>
														) : null}
													</div>
												</a>
											</li>
										);
									})}
								</ul>
							) : imageMode ? (
								<ul className="grid max-w-5xl grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
									{results.map((result) => (
										<li key={result.url}>
											<button
												type="button"
												onClick={() => setLightbox(result)}
												className="group block w-full overflow-hidden rounded-xl text-left"
											>
												{result.thumbnail || result.img_src ? (
													<img
														src={result.thumbnail ?? result.img_src}
														alt=""
														className="aspect-square w-full bg-surface-raised object-cover transition-transform duration-150 group-hover:scale-[1.01]"
														loading="lazy"
													/>
												) : null}
												<p className="mt-1.5 truncate text-xs text-ink-muted group-hover:text-accent">
													{result.title}
												</p>
												{result.resolution ? (
													<p className="truncate text-[0.65rem] text-ink-subtle">
														{result.resolution}
													</p>
												) : null}
											</button>
										</li>
									))}
								</ul>
							) : mapMode ? (
								<ul className="flex flex-col gap-4">
									{results.map((result) => (
										<li key={result.url}>
											<MapResult
												result={result}
												newTab={config?.ui?.results_on_new_tab}
											/>
										</li>
									))}
								</ul>
							) : (
								<ul className="flex flex-col gap-8">
									{results.map((result) => {
										const Template = specializedTemplate(result);
										return (
											<li
												key={`${result.url}:${engineNames(result).join(",")}`}
											>
												{Template ? (
													<Template
														result={result}
														newTab={config?.ui?.results_on_new_tab}
													/>
												) : (
													<ResultItem
														result={result}
														newTab={config?.ui?.results_on_new_tab}
														urlFormatting={config?.ui?.url_formatting}
														cacheUrl={config?.ui?.cache_url}
													/>
												)}
											</li>
										);
									})}
								</ul>
							)}

							{firstPage.suggestions.length > 0 ? (
								<section className="mt-14 max-w-[38rem]">
									<h2 className="mb-3 text-base font-medium text-ink">
										Related searches
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
