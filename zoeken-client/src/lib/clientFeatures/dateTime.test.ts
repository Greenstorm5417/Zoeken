import { describe, expect, it } from "vitest";
import {
	computeDateTimeAnswer,
	daysUntil,
	parseClock,
	type YMD,
	zoneConvert,
} from "./dateTime";

function day(y: number, m: number, d: number): YMD {
	return { y, m, d };
}

describe("daysUntil", () => {
	it("handles an ISO date", () => {
		const today = day(2026, 7, 20);
		expect(daysUntil("days until 2026-12-25", today)).toBe(
			"158 days until 2026-12-25 (2026-12-25)",
		);
		expect(daysUntil("days until 2026-07-21", today)).toBe(
			"1 day until 2026-07-21 (2026-07-21)",
		);
		expect(daysUntil("days until 2026-07-20", today)).toBe(
			"2026-07-20 is today (2026-07-20)",
		);
		expect(daysUntil("days until 2020-01-01", today)).toBeNull();
	});

	it("rolls named days to next year when already past", () => {
		const today = day(2026, 12, 26);
		expect(daysUntil("days until christmas", today)).toBe(
			"364 days until christmas (2027-12-25)",
		);
		const before = day(2026, 12, 20);
		expect(daysUntil("how many days until christmas?", before)).toBe(
			"5 days until christmas (2026-12-25)",
		);
	});
});

describe("zoneConvert", () => {
	it("converts basic zones", () => {
		expect(zoneConvert("3pm est in cet")).toBe("3:00 PM EST = 9:00 PM CET");
		expect(zoneConvert("15:30 utc to ist")).toBe("3:30 PM UTC = 9:00 PM IST");
	});

	it("flags midnight crossing", () => {
		expect(zoneConvert("11pm est in cet")).toBe(
			"11:00 PM EST = 5:00 AM CET (next day)",
		);
		expect(zoneConvert("1am cet in pst")).toBe(
			"1:00 AM CET = 4:00 PM PST (previous day)",
		);
	});
});

describe("parseClock", () => {
	it("handles twelve-hour edges", () => {
		expect(parseClock("12am")).toBe(0);
		expect(parseClock("12pm")).toBe(12 * 60);
		expect(parseClock("12:30am")).toBe(30);
		expect(parseClock("13pm")).toBeNull();
		expect(parseClock("25:00")).toBeNull();
	});
});

describe("computeDateTimeAnswer", () => {
	it("returns null for unrelated queries", () => {
		expect(computeDateTimeAnswer("rust programming")).toBeNull();
		expect(computeDateTimeAnswer("days until")).toBeNull();
		expect(computeDateTimeAnswer("3pm xyz in cet")).toBeNull();
		expect(computeDateTimeAnswer("")).toBeNull();
	});

	it("tags the engine for a zone conversion", () => {
		const answer = computeDateTimeAnswer("3pm est in cet");
		expect(answer?.engine).toBe("time zones");
	});
});
