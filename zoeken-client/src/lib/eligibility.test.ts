import { describe, expect, it } from "vitest";
import type { EngineInfo } from "./api";
import { engineEligibility } from "./eligibility";

function engine(
	partial: Partial<EngineInfo> & Pick<EngineInfo, "name" | "categories">,
): EngineInfo {
	return {
		shortcut: "",
		enabled: true,
		paging: true,
		language_support: false,
		languages: [],
		regions: [],
		safesearch: false,
		time_range_support: false,
		timeout: 3,
		...partial,
	};
}

describe("engine eligibility", () => {
	it("counts general engines only, and explains a time filter that drops sources", () => {
		const engines = [
			engine({
				name: "web",
				categories: ["general"],
				time_range_support: true,
				safesearch: true,
			}),
			engine({ name: "wiki", categories: ["general"] }),
			engine({
				name: "pics",
				categories: ["images"],
				time_range_support: true,
			}),
		];
		const open = engineEligibility(engines, {
			category: "general",
			timeRange: "",
			language: "",
			safesearch: 0,
			engines: "",
		});
		expect(open.inCategory).toBe(2);
		const timed = engineEligibility(engines, {
			category: "general",
			timeRange: "week",
			language: "",
			safesearch: 0,
			engines: "",
		});
		expect(timed.eligible).toBe(1);
		expect(timed.timeReduced).toBe(true);
	});

	it("does not keep an explicit engine that cannot honor SafeSearch", () => {
		const engines = [
			engine({ name: "open", categories: ["general"], safesearch: false }),
			engine({ name: "strict", categories: ["general"], safesearch: true }),
		];
		const result = engineEligibility(engines, {
			category: "general",
			timeRange: "",
			language: "",
			safesearch: 2,
			engines: "open",
		});
		expect(result.eligible).toBe(0);
		expect(result.explicitSkipped).toContain("open");
	});

	it("honors an explicit disabled engine by shortcut and drops it after page 1 without paging", () => {
		const engines = [
			engine({
				name: "wiki",
				shortcut: "w",
				categories: ["general"],
				enabled: false,
				paging: false,
			}),
		];
		const selected = engineEligibility(engines, {
			category: "general",
			timeRange: "",
			language: "",
			safesearch: 0,
			engines: "w",
		});
		expect(selected.eligible).toBe(1);
		const paged = engineEligibility(engines, {
			category: "general",
			timeRange: "",
			language: "",
			safesearch: 0,
			engines: "w",
			pageno: 2,
		});
		expect(paged.eligible).toBe(0);
	});
});
