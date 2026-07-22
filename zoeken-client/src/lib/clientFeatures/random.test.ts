import { describe, expect, it } from "vitest";
import { computeRandomAnswer } from "./random";

function matchesFormat(kind: string, value: string): boolean {
	switch (kind) {
		case "string":
			return /^[a-zA-Z0-9]{8,32}$/.test(value);
		case "int":
			return Number.isInteger(Number(value)) && /^-?\d+$/.test(value);
		case "float":
			return Number.isFinite(Number(value));
		case "sha256":
			return /^[0-9a-f]{64}$/.test(value);
		case "uuid":
			return /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
				value,
			);
		case "color":
			return /^#[0-9A-F]{6}$/.test(value);
		default:
			return false;
	}
}

describe("computeRandomAnswer", () => {
	it("requires exactly two tokens", () => {
		expect(computeRandomAnswer("random")).toBeNull();
		expect(computeRandomAnswer("random uuid extra")).toBeNull();
		expect(computeRandomAnswer("random bogus")).toBeNull();
	});

	it("produces values conforming to the requested kind", () => {
		for (const kind of ["string", "int", "float", "sha256", "uuid", "color"]) {
			for (let i = 0; i < 25; i++) {
				const answer = computeRandomAnswer(`random ${kind}`);
				expect(answer).not.toBeNull();
				expect(matchesFormat(kind, answer?.answer ?? "")).toBe(true);
			}
		}
	});
});
