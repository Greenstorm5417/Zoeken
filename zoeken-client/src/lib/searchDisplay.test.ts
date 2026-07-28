import { describe, expect, it } from "vitest";
import { pageNumbers, wallClockSeconds } from "./searchDisplay";

describe("wallClockSeconds", () => {
	it("converts performance.now marks to seconds", () => {
		expect(wallClockSeconds(1000, 4123.4)).toBeCloseTo(3.1234, 4);
	});

	it("never goes negative when end precedes start", () => {
		expect(wallClockSeconds(5000, 4999)).toBe(0);
	});
});

describe("pageNumbers", () => {
	it("starts at 1 for early pages", () => {
		expect(pageNumbers(1)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
		expect(pageNumbers(5)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
	});

	it("centers once past page 5", () => {
		expect(pageNumbers(6)).toEqual([2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
	});
});
