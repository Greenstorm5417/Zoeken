import { Link, useNavigate } from "@tanstack/react-router";
import { Settings2 } from "lucide-react";
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
	pendingCategory: string;
	setPendingCategory: (category: string) => void;
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
	pendingCategory,
	setPendingCategory,
	time_range,
	language,
	safesearch,
}: Props) {
	const navigate = useNavigate();
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
			<div className="mx-auto flex max-w-6xl items-center gap-2 px-3 pt-3 pb-2.5 sm:gap-4 sm:px-6">
				<Link to="/" className="shrink-0 no-underline" aria-label="Zoeken home">
					<img src="/zoeken-logo.svg" alt="" width={32} height={32} />
				</Link>
				<div className="w-full min-w-0 max-w-[40rem] flex-1">
					<SearchForm key={q} initialQuery={q} compact baseSearch={params} />
				</div>
				<div className="ml-auto flex shrink-0 items-center gap-1">
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

			{q.trim() && filtersVisible ? (
				<div className="mx-auto flex max-w-6xl items-center gap-2 overflow-x-auto border-t border-line/60 px-3 py-2 sm:px-6 lg:hidden">
					<SearchFilterMenus {...filterProps} />
				</div>
			) : null}
		</header>
	);
}
