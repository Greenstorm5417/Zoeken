import { describe, expect, it } from "vitest";
import { featureCatalog } from "./catalog";

describe("featureCatalog", () => {
	it("returns empty when /config plugins are missing", () => {
		expect(featureCatalog([])).toEqual([]);
		expect(featureCatalog(undefined)).toEqual([]);
	});

	it("maps the /config plugins list", () => {
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
});
