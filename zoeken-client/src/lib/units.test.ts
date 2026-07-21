import { describe, expect, it } from "vitest";
import { convertUnits, formatUnitNumber, unitById } from "./units";

describe("units", () => {
	it("converts gal to floz as 128", () => {
		expect(convertUnits(1, "gal", "floz")).toBeCloseTo(128, 10);
	});

	it("converts km to m", () => {
		expect(convertUnits(2, "km", "m")).toBe(2000);
	});

	it("converts cups in a gallon", () => {
		expect(convertUnits(1, "gal", "cup")).toBeCloseTo(16, 10);
	});

	it("handles temperature", () => {
		expect(convertUnits(32, "°F", "°C")).toBeCloseTo(0, 10);
		expect(convertUnits(0, "°C", "K")).toBeCloseTo(273.15, 10);
	});

	it("looks up units by id", () => {
		expect(unitById("floz")?.dimension).toBe("volume");
		expect(unitById("oz")?.dimension).toBe("mass");
	});

	it("formats integers cleanly", () => {
		expect(formatUnitNumber(128)).toBe("128");
	});
});
