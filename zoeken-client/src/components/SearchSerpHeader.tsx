import { Link, useNavigate } from "@tanstack/react-router";
import { Settings2 } from "lucide-react";
import { useEffect, useState } from "react";
import {
	hasSearchFilters,
	SearchFilterMenus,
} from "#/components/SearchFilterMenus";
import { SearchForm } from "#/components/SearchForm";
import type { Config } from "#/lib/api";
import { DEFAULT_CATEGORIES, searchLink } from "#/lib/searchDisplay";
import type { SearchRouteParams } from "#/lib/searchParams";

type Props = {
	params: SearchRouteParams;
	config: Config | undefined;
	q: string;
	activeCategory: string;
	time_range: string;
	language: string | undefined;
	safesearch: 0 | 1 | 2;
};

/** Sticky SERP chrome: logo, form, filters, category tabs, nav. */
export function SearchSerpHeader({
	params,
	config,
	q,
	activeCategory,
	time_range,
	language,
	safesearch,
}: Props) {
	const navigate = useNavigate();
	// Category-tab preview lives here so tab clicks don't re-render the full SERP.
	const [pendingCategory, setPendingCategory] = useState(activeCategory);
	useEffect(() => setPendingCategory(activeCategory), [activeCategory]);
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
	const filtersVisible = hasSearchFilters(config, activeCategory);
	const filterProps = {
		params,
		config,
		activeCategory,
		time_range,
		language,
		safesearch,
	};

	return (
		<header className="sticky top-0 z-20 border-b border-line bg-surface">
			<div className="mx-auto flex max-w-6xl items-center gap-2 px-2.5 pt-2.5 pb-2 sm:gap-4 sm:px-6 sm:pt-3 sm:pb-2.5">
				<Link
					to="/"
					className="inline-flex size-11 shrink-0 items-center justify-center no-underline sm:size-auto"
					aria-label="Zoeken home"
				>
					<img
						src="/zoeken-logo.svg"
						alt=""
						width={32}
						height={32}
						className="size-7 sm:size-8"
						decoding="async"
						fetchPriority="high"
					/>
				</Link>
				<div className="w-full min-w-0 max-w-[40rem] flex-1">
					<SearchForm initialQuery={q} compact baseSearch={params} />
				</div>
				<div className="ml-auto flex shrink-0 items-center gap-0.5 sm:gap-1">
					{q.trim() ? (
						<div className="hidden items-center gap-2 lg:flex">
							<SearchFilterMenus {...filterProps} />
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
							className="inline-flex size-11 items-center justify-center rounded-lg text-ink-muted transition-colors hover:bg-accent-soft hover:text-ink md:hidden"
						>
							<Settings2 className="size-5" aria-hidden />
						</Link>
					</nav>
				</div>
			</div>

			{q.trim() ? (
				<div className="-mx-0 flex max-w-6xl items-stretch gap-0.5 overflow-x-auto overscroll-x-contain px-2.5 sm:mx-auto sm:gap-1 sm:px-6 [-ms-overflow-style:none] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
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
									"inline-flex min-h-11 shrink-0 items-center border-b-2 px-3 text-sm capitalize no-underline transition-colors duration-100",
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
							className="mb-1 ml-1 inline-flex min-h-11 shrink-0 items-center px-2 text-sm font-medium text-accent"
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

			{q.trim() && filtersVisible ? (
				<div className="mx-auto flex max-w-6xl items-center gap-2 overflow-x-auto overscroll-x-contain border-t border-line/60 px-2.5 py-2 sm:px-6 lg:hidden">
					<SearchFilterMenus {...filterProps} />
				</div>
			) : null}
		</header>
	);
}
