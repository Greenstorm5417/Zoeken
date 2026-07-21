import { describe, expect, it } from "vitest";
import { editDistance, pickDidYouMean } from "./didYouMean";

describe("didYouMean", () => {
	it("scores close typos", () => {
		expect(editDistance("rust", "rast")).toBe(1);
		expect(editDistance("rust", "rust")).toBe(0);
		expect(editDistance("rust", "python")).toBeGreaterThan(2);
	});

	it("picks a near autocomplete suggestion", () => {
		expect(pickDidYouMean("rustt", ["rust", "ruby"])).toBe("rust");
		expect(pickDidYouMean("rust", ["rust"])).toBeNull();
		expect(pickDidYouMean("zzzz", ["alpha", "beta"])).toBeNull();
	});
});
