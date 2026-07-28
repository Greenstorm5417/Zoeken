/** Preference-toggleable client features (from `/config.plugins`). */
export type ClientFeatureInfo = {
	id: string;
	name: string;
	description: string;
	default_enabled: boolean;
};

/** Map `/config.plugins` into the prefs UI catalog. Empty when config is missing. */
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
	if (!configPlugins || configPlugins.length === 0) {
		return [];
	}
	return configPlugins.map((p) => ({
		id: p.id,
		name: p.name,
		description: p.description,
		default_enabled: p.default_enabled ?? Boolean(p.enabled),
	}));
}
