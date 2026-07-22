/** Post-search result pipeline and preference-gated client features. */
import type { Config, Preferences, SearchResult } from "../api";
import { CLIENT_FEATURE_CATALOG } from "./catalog";
import { applyDoiRewrite } from "./doiRewrite";
import { applyHostnames, sortByPriority } from "./hostnames";
import { applyTrackerUrlRemover } from "./trackerUrlRemover";

export {
	CLIENT_FEATURE_CATALOG,
	featureCatalog,
	type ClientFeatureInfo,
} from "./catalog";

/** Prefer cookie/settings prefs; fall back to `/config` then local catalog defaults. */
export function pluginEnabled(
	config: Config | undefined,
	id: string,
	prefs?: Preferences | null,
): boolean {
	const fromPrefs = prefs?.plugins?.[id];
	if (typeof fromPrefs === "boolean") {
		return fromPrefs;
	}
	const plugin = config?.plugins?.find((p) => p.id === id);
	if (plugin) {
		return Boolean(plugin.default_enabled ?? plugin.enabled);
	}
	const fallback = CLIENT_FEATURE_CATALOG.find((f) => f.id === id);
	return Boolean(fallback?.default_enabled);
}

/** Filter/map/re-sort a page of results per the user's enabled client features. */
export function applyClientFeatures(
	results: SearchResult[],
	config: Config | undefined,
	prefs?: Preferences | null,
): SearchResult[] {
	let working = results;
	if (pluginEnabled(config, "tracker_url_remover", prefs)) {
		working = applyTrackerUrlRemover(working);
	}

	const hostnamesOn = pluginEnabled(config, "hostnames", prefs);
	const doiOn = pluginEnabled(config, "oa_doi_rewrite", prefs);

	const prioritized = hostnamesOn
		? applyHostnames(working, config?.hostnames)
		: working.map((result) => ({ result, priority: "normal" as const }));
	let sorted = sortByPriority(prioritized);

	const resolverUrl = config?.default_doi_resolver
		? config?.doi_resolver_urls?.[config.default_doi_resolver]
		: undefined;
	if (doiOn && resolverUrl) {
		sorted = sorted.map((result) => applyDoiRewrite(result, resolverUrl));
	}

	return sorted;
}
