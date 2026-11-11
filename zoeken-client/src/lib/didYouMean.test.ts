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

	it("prefers the closer candidate and leaves technical queries alone", () => {
		expect(pickDidYouMean("pythn", ["pylon", "python"])).toBe("python");
		expect(
			pickDidYouMean('site:docs.rs "trait"', ["site:docs.rs trait"]),
		).toBeNull();
		expect(pickDidYouMean("C++", ["cpp"])).toBeNull();
		expect(
			pickDidYouMean("std::vector pythn", ["std::vector python"]),
		).toBeNull();
		expect(pickDidYouMean("meet Ada", ["meet Ida"])).toBeNull();
		expect(pickDidYouMean("Linus Torvalds", ["linux torvalds"])).toBeNull();
		expect(
			pickDidYouMean("cannot borrow as mutable", ["cannot borrow a mutable"]),
		).toBe("cannot borrow a mutable");
		expect(pickDidYouMean("E0308 expected", ["E0308 expects"])).toBeNull();
		expect(pickDidYouMean("pythn", ["python"])).toBe("python");
	});
});
