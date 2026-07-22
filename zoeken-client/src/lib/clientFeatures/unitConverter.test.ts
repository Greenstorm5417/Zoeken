import { describe, expect, it } from "vitest";
import { computeUnitConverterAnswer } from "./unitConverter";

describe("computeUnitConverterAnswer", () => {
	it("converts a simple forward phrase", () => {
		const answer = computeUnitConverterAnswer("2 km in m", 1);
		expect(answer?.answer).toBe("2 km = 2000 m");
	});

	it('understands "how many X in Y" phrasing', () => {
		const answer = computeUnitConverterAnswer("how many cups in a gallon", 1);
		expect(answer?.answer).toBe("1 gal = 16 cup");
	});

	it("treats bare oz as fluid ounce when the target is a volume", () => {
		const answer = computeUnitConverterAnswer("how many oz in a gal", 1);
		expect(answer?.answer).toBe("1 gal = 128 floz");
		expect(answer?.interactive).toEqual({
			type: "unit",
			amount: 1,
			from: "gal",
			to: "floz",
			result: 128,
			dimension: "volume",
		});
	});

	it("handles the joined number+unit form", () => {
		const answer = computeUnitConverterAnswer("10km to miles", 1);
		expect(answer?.answer).toContain("10 km =");
	});

	it("handles temperature conversion", () => {
		const answer = computeUnitConverterAnswer("100 c to f", 1);
		expect(answer?.answer).toBe("100 °C = 212 °F");
	});

	it("strips trailing politeness filler", () => {
		const answer = computeUnitConverterAnswer("2 km to m please", 1);
		expect(answer?.answer).toBe("2 km = 2000 m");
	});

	it("returns null for unrelated queries", () => {
		expect(computeUnitConverterAnswer("rust lang", 1)).toBeNull();
	});

	it("returns null past the first page", () => {
		expect(computeUnitConverterAnswer("2 km in m", 2)).toBeNull();
	});

	it("returns null when the two units have different dimensions", () => {
		expect(computeUnitConverterAnswer("2 km to kg", 1)).toBeNull();
	});
});
