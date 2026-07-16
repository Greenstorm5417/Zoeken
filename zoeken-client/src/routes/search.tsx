import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { SearchForm } from "#/components/SearchForm";
import { SelectMenu } from "#/components/SelectMenu";
import { SiteNav } from "#/components/SiteNav";
import { ApiError, type Infobox, type SearchResult, search } from "#/lib/api";
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
			{engines.length > 0 ? (
				<p className="mt-1.5 text-[0.75rem] text-ink-subtle">
					{engines.map(formatEngineLabel).join(" · ")}
				</p>
			) : null}
			{cacheUrl ? (
				<a
					href={cacheUrl + encodeURIComponent(result.url)}
					className="mt-1 inline-block text-[0.75rem] text-ink-subtle hover:text-accent"
				>
					Cached
				</a>
			) : null}
		</article>
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
	const { q, pageno = 1, categories, language, safesearch = 0 } = params;
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
	const query = useQuery({
		queryKey: ["search", { ...params, categories: activeCategory }],
		queryFn: () =>
			search({
				...params,
				// Always pin a category so the backend doesn't widen the engine set.
				categories: activeCategory,
			}),
		enabled: q.trim().length > 0,
	});
	const imageMode =
		activeCategory === "images" ||
		Boolean(
			query.data?.results.length &&
				query.data.results.every((r) => r.img_src) &&
				activeCategory !== "videos",
		);
	const videoMode =
		activeCategory === "videos" ||
		Boolean(
			query.data?.results.length &&
				query.data.results.every(
					(r) =>
						r.template === "videos.html" ||
						Boolean(r.iframe_src) ||
						(Boolean(r.thumbnail) && !r.img_src),
				),
		);
	const errorStatus =
		query.error instanceof ApiError ? query.error.status : undefined;

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

	return (
		<div className="zoeken-serp min-h-dvh text-ink">
			<SiteNav />
			<header className="sticky top-0 z-20 border-b border-line/70 bg-surface/90 backdrop-blur-md">
				<div className="mx-auto flex max-w-6xl items-center gap-3 px-4 pt-4 pr-36 pb-3 sm:gap-4 sm:px-6 sm:pr-48">
					<Link
						to="/"
						className="shrink-0 no-underline"
						aria-label="Zoeken home"
					>
						<img src="/zoeken-logo.svg" alt="" width={32} height={32} />
					</Link>
					<div className="min-w-0 w-full max-w-[36rem] sm:max-w-[40rem]">
						<SearchForm key={q} initialQuery={q} compact baseSearch={params} />
					</div>
					{q.trim() ? (
						<div className="ml-auto hidden shrink-0 items-center gap-2 lg:flex">
							<SelectMenu
								label="Language"
								value={language ?? ""}
								options={[
									{ value: "", label: "Any language" },
									...Object.entries(config?.locales ?? {}).map(
										([code, name]) => ({
											value: code,
											label: name,
										}),
									),
								]}
								onChange={(next) =>
									void Route.navigate({
										search: searchLink(params, {
											language: next || undefined,
											pageno: undefined,
										}),
									})
								}
							/>
							<SelectMenu
								label="Safe search"
								value={String(safesearch)}
								options={[
									{ value: "0", label: "SafeSearch off" },
									{ value: "1", label: "Moderate" },
									{ value: "2", label: "Strict" },
								]}
								onChange={(next) =>
									void Route.navigate({
										search: searchLink(params, {
											safesearch: Number(next) as 0 | 1 | 2,
											pageno: undefined,
										}),
									})
								}
							/>
						</div>
					) : null}
				</div>

				{q.trim() ? (
					<div className="mx-auto flex max-w-6xl items-end gap-1 overflow-x-auto px-4 sm:px-6 sm:pl-[4.25rem]">
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
										"shrink-0 border-b-2 px-3 pb-2.5 text-sm capitalize no-underline transition-colors",
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
									void Route.navigate({
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
			</header>

			{!q.trim() ? (
				<p className="mt-16 text-center text-ink-muted">
					Type a query to search.
				</p>
			) : null}

			{query.isLoading ? (
				<p className="mx-auto mt-10 max-w-6xl px-4 text-sm text-ink-subtle sm:px-6 sm:pl-[4.25rem]">
					Searching…
				</p>
			) : null}

			{query.isError ? (
				<p className="mx-auto mt-10 max-w-6xl px-4 text-sm text-ink-muted sm:px-6 sm:pl-[4.25rem]">
					{errorStatus === 429
						? "Too many searches. Please wait a moment and try again."
						: "Search service unavailable."}
				</p>
			) : null}

			{query.data ? (
				<div
					className={[
						"animate-fade mx-auto max-w-6xl px-4 pt-6 pb-20 sm:px-6",
						config?.ui?.center_alignment ? "" : "sm:pl-[4.25rem]",
					].join(" ")}
				>
					{query.data.unresponsive_engines.length > 0 ? (
						<aside className="mb-6 max-w-[40rem] rounded-xl border border-line bg-surface-raised px-4 py-3">
							<p className="text-sm font-medium text-ink">
								{query.data.unresponsive_engines.length} engine
								{query.data.unresponsive_engines.length === 1 ? "" : "s"} didn’t
								respond
							</p>
							<ul className="mt-2 flex flex-col gap-1">
								{query.data.unresponsive_engines.map(([engine, reason]) => (
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

					{query.data.corrections.length > 0 ? (
						<p className="mb-6 text-ink-muted">
							Did you mean{" "}
							{query.data.corrections.map((c, i) => {
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
							{query.data.answers.length > 0 ? (
								<section className="mb-8 max-w-[38rem] border-b border-line pb-6">
									{query.data.answers.map((a) => (
										<div key={a.answer} className="mb-4 last:mb-0">
											{a.engine ? (
												<p className="mb-1 text-[0.7rem] font-medium tracking-wide text-ink-subtle uppercase">
													{formatEngineLabel(a.engine)}
												</p>
											) : null}
											<p className="text-[1.75rem] leading-snug tracking-tight text-ink">
												{a.answer}
											</p>
										</div>
									))}
								</section>
							) : null}

							{query.data.results.length === 0 ? (
								<p className="text-ink-muted">
									No results for <span className="text-ink">“{q}”</span>.
								</p>
							) : videoMode ? (
								<ul className="grid max-w-5xl grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
									{query.data.results.map((result) => {
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
																className="size-full object-cover transition-transform duration-200 group-hover:scale-[1.02]"
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
									{query.data.results.map((result) => (
										<li key={result.url}>
											<a
												data-result-link
												href={result.url}
												target={
													config?.ui?.results_on_new_tab ? "_blank" : undefined
												}
												rel={
													config?.ui?.results_on_new_tab
														? "noopener noreferrer"
														: undefined
												}
												className="group block overflow-hidden rounded-xl no-underline"
											>
												{result.img_src || result.thumbnail ? (
													<img
														src={result.img_src ?? result.thumbnail}
														alt=""
														className="aspect-square w-full object-cover transition-transform duration-200 group-hover:scale-[1.02]"
														loading="lazy"
													/>
												) : null}
												<p className="mt-1.5 truncate text-xs text-ink-muted group-hover:text-accent">
													{result.title}
												</p>
												{engineNames(result).length ? (
													<p className="truncate text-[0.65rem] text-ink-subtle">
														{engineNames(result)
															.map(formatEngineLabel)
															.join(" · ")}
													</p>
												) : null}
											</a>
										</li>
									))}
								</ul>
							) : (
								<ul className="flex flex-col gap-8">
									{query.data.results.map((result) => (
										<li key={`${result.url}:${engineNames(result).join(",")}`}>
											<ResultItem
												result={result}
												newTab={config?.ui?.results_on_new_tab}
												urlFormatting={config?.ui?.url_formatting}
												cacheUrl={config?.ui?.cache_url}
											/>
										</li>
									))}
								</ul>
							)}

							{query.data.suggestions.length > 0 ? (
								<section className="mt-14 max-w-[38rem]">
									<h2 className="mb-3 text-base font-medium text-ink">
										Related searches
									</h2>
									<div className="flex flex-wrap gap-2">
										{query.data.suggestions.map((s) => {
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

							{query.data.results.length > 0 ? (
								<nav
									aria-label="Pagination"
									className="mt-14 flex max-w-[38rem] flex-wrap items-center gap-x-1 gap-y-2 text-[0.95rem]"
								>
									{pageno > 1 ? (
										<Link
											to="/search"
											search={searchLink(params, { pageno: pageno - 1 })}
											className="mr-2 text-accent no-underline hover:underline"
										>
											‹ Previous
										</Link>
									) : null}
									{pageNumbers(pageno).map((page) =>
										page === pageno ? (
											<span
												key={page}
												aria-current="page"
												className="min-w-8 px-2 text-center font-semibold text-ink"
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
												className="min-w-8 px-2 text-center text-accent no-underline hover:underline"
											>
												{page}
											</Link>
										),
									)}
									<Link
										to="/search"
										search={searchLink(params, { pageno: pageno + 1 })}
										className="ml-2 text-accent no-underline hover:underline"
									>
										Next ›
									</Link>
								</nav>
							) : null}
						</div>

						{query.data.infoboxes.length > 0 ? (
							<aside className="w-full shrink-0 lg:w-[19rem]">
								{query.data.infoboxes.map((infobox, index) => (
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
