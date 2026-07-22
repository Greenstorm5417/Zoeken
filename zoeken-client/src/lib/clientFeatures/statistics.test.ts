import { describe, expect, it } from "vitest";
import { computeStatisticsAnswer } from "./statistics";

describe("computeStatisticsAnswer", () => {
	it("computes sum", () => {
		expect(computeStatisticsAnswer("sum 1 2 3")?.answer).toBe(
			"sum(1, 2, 3) = 6",
		);
	});

	it("computes avg with decimals", () => {
		expect(computeStatisticsAnswer("avg 1 2")?.answer).toBe("avg(1, 2) = 1.5");
	});

	it("computes min/max/prod/range/median", () => {
		expect(computeStatisticsAnswer("min 1 2 3 4")?.answer).toBe(
			"min(1, 2, 3, 4) = 1",
		);
		expect(computeStatisticsAnswer("max 1 2 3 4")?.answer).toBe(
			"max(1, 2, 3, 4) = 4",
		);
		expect(computeStatisticsAnswer("prod 1 2 3 4")?.answer).toBe(
			"prod(1, 2, 3, 4) = 24",
		);
		expect(computeStatisticsAnswer("range 1 2 3 4")?.answer).toBe(
			"range(1, 2, 3, 4) = 3",
		);
		expect(computeStatisticsAnswer("median 3 1 2")?.answer).toBe(
			"median(3, 1, 2) = 2",
		);
		expect(computeStatisticsAnswer("median 1 2 3 4")?.answer).toBe(
			"median(1, 2, 3, 4) = 2.5",
		);
	});

	it("returns null for non-numeric arguments", () => {
		expect(computeStatisticsAnswer("sum 1 two 3")).toBeNull();
	});

	it("returns null for a keyword without numbers", () => {
		expect(computeStatisticsAnswer("sum")).toBeNull();
	});

	it("returns null for an unrelated keyword", () => {
		expect(computeStatisticsAnswer("hello 1 2 3")).toBeNull();
	});
});
