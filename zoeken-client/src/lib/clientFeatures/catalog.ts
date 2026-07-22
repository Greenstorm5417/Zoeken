/** Preference-toggleable client features (mirrors `/config.plugins`). */
export type ClientFeatureInfo = {
	id: string;
	name: string;
	description: string;
	default_enabled: boolean;
};

/**
 * Local catalog used when `/config` is missing or returns an empty plugins list.
 * Ids/defaults must stay aligned with `zoeken-server` `client_feature_plugin_infos`.
 */
export const CLIENT_FEATURE_CATALOG: readonly ClientFeatureInfo[] = [
	{
		id: "calculator",
		name: "Calculator",
		description: "Parses and solves mathematical expressions.",
		default_enabled: true,
	},
	{
		id: "time_zone",
		name: "Time zones",
		description: "Display the current time on different time zones.",
		default_enabled: true,
	},
	{
		id: "self_info",
		name: "Self Information",
		description:
			'Displays your IP or user agent for queries like "whats my ip" / "user-agent".',
		default_enabled: true,
	},
	{
		id: "hostnames",
		name: "Hostnames",
		description:
			"Rewrite hostnames and remove or prioritize results based on the hostname",
		default_enabled: true,
	},
	{
		id: "oa_doi_rewrite",
		name: "Open Access DOI rewrite",
		description:
			"Avoid paywalls by redirecting to open-access versions of publications when available",
		default_enabled: false,
	},
	{
		id: "tracker_url_remover",
		name: "Tracker URL remover",
		description: "Remove trackers arguments from the returned URL",
		default_enabled: true,
	},
	{
		id: "ahmia_filter",
		name: "Ahmia blacklist",
		description:
			"Filter out onion results that appear in Ahmia's blacklist.",
		default_enabled: true,
	},
	{
		id: "unit_converter",
		name: "Unit converter",
		description:
			'Convert between units ("10 km to miles", "how many cups in a gallon").',
		default_enabled: true,
	},
	{
		id: "infinite_scroll",
		name: "Infinite scroll",
		description:
			"Automatically loads the next page when scrolling to bottom of the current page",
		default_enabled: false,
	},
];

export function featureCatalog(
	configPlugins:
		| Array<{
				id: string;
				name: string;
				description: string;
				default_enabled: boolean;
				enabled?: boolean;
		  }>
		| undefined
		| null,
): ClientFeatureInfo[] {
	if (configPlugins && configPlugins.length > 0) {
		return configPlugins.map((p) => ({
			id: p.id,
			name: p.name,
			description: p.description,
			default_enabled: p.default_enabled ?? Boolean(p.enabled),
		}));
	}
	return [...CLIENT_FEATURE_CATALOG];
}
