import { useNavigate } from "@tanstack/react-router";
import { SelectMenu } from "#/components/SelectMenu";
import type { Config } from "#/lib/api";
import { stringsFor } from "#/lib/i18n";
import { searchLink } from "#/lib/searchDisplay";
import type { SearchRouteParams } from "#/lib/searchParams";

type Props = {
	params: SearchRouteParams;
	config: Config | undefined;
	activeCategory: string;
	time_range: string;
	language: string | undefined;
	safesearch: 0 | 1 | 2;
};

/** Time / language / safesearch menus; only shows filters engines support. */
export function SearchFilterMenus({
	params,
	config,
	activeCategory,
	time_range,
	language,
	safesearch,
}: Props) {
	const navigate = useNavigate();
	const categoryEngines = (config?.engines ?? []).filter(
		(engine) =>
			engine.enabled &&
			(activeCategory === "general" ||
				engine.categories.map((c) => c.toLowerCase()).includes(activeCategory)),
	);
	const showTimeRange = categoryEngines.some((e) => e.time_range_support);
	const showSafesearch = categoryEngines.some((e) => e.safesearch);
	const showLanguage = categoryEngines.some((e) => e.language_support);

	const languageOptions = [
		{ value: "", label: "Any language / region" },
		...Object.entries(config?.locales ?? {}).map(([code, name]) => ({
			value: code,
			label: name,
		})),
	];
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

	if (!showTimeRange && !showLanguage && !showSafesearch) return null;

	return (
		<>
			{showTimeRange ? (
				<SelectMenu
					label="Time range"
					value={time_range}
					options={timeRangeOptions}
					onChange={(next) =>
						void navigate({
							to: "/search",
							search: searchLink(params, {
								time_range: next || undefined,
								pageno: undefined,
							}),
						})
					}
				/>
			) : null}
			{showLanguage ? (
				<SelectMenu
					label="Language / region"
					value={language ?? ""}
					options={languageOptions}
					onChange={(next) =>
						void navigate({
							to: "/search",
							search: searchLink(params, {
								language: next || undefined,
								pageno: undefined,
							}),
						})
					}
				/>
			) : null}
			{showSafesearch ? (
				<SelectMenu
					label="Safe search"
					value={String(safesearch)}
					options={safesearchOptions}
					onChange={(next) =>
						void navigate({
							to: "/search",
							search: searchLink(params, {
								safesearch: Number(next) as 0 | 1 | 2,
								pageno: undefined,
							}),
						})
					}
				/>
			) : null}
		</>
	);
}

/** Whether any filter menu would render for this category. */
export function hasSearchFilters(
	config: Config | undefined,
	activeCategory: string,
): boolean {
	const categoryEngines = (config?.engines ?? []).filter(
		(engine) =>
			engine.enabled &&
			(activeCategory === "general" ||
				engine.categories.map((c) => c.toLowerCase()).includes(activeCategory)),
	);
	return (
		categoryEngines.some((e) => e.time_range_support) ||
		categoryEngines.some((e) => e.language_support) ||
		categoryEngines.some((e) => e.safesearch)
	);
}
