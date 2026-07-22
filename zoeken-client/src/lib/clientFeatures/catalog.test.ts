import { describe, expect, it } from "vitest";
import { CLIENT_FEATURE_CATALOG, featureCatalog } from "./catalog";

describe("featureCatalog", () => {
	it("falls back to the local catalog when /config plugins are empty", () => {
		expect(featureCatalog([])).toEqual([...CLIENT_FEATURE_CATALOG]);
		expect(featureCatalog(undefined)).toEqual([...CLIENT_FEATURE_CATALOG]);
	});

	it("prefers the /config plugins list when present", () => {
		const fromConfig = [
			{
				id: "calculator",
				name: "Calc",
				description: "from config",
				default_enabled: false,
			},
		];
		expect(featureCatalog(fromConfig)).toEqual([
			{
				id: "calculator",
				name: "Calc",
				description: "from config",
				default_enabled: false,
			},
		]);
	});

	it("includes the preference ids the SPA and server gate on", () => {
		const ids = CLIENT_FEATURE_CATALOG.map((f) => f.id);
		expect(ids).toEqual([
			"calculator",
			"time_zone",
			"self_info",
			"hostnames",
			"oa_doi_rewrite",
			"tracker_url_remover",
			"ahmia_filter",
			"unit_converter",
			"infinite_scroll",
		]);
	});
});
