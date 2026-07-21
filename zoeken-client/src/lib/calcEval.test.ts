import { describe, expect, it } from "vitest";
import { calcEval, formatCalcNumber } from "./calcEval";

describe("calcEval", () => {
	it("adds and subtracts", () => {
		expect(calcEval("2 + 2")).toEqual({ ok: true, value: 4 });
		expect(calcEval("10 − 3")).toEqual({ ok: true, value: 7 });
		expect(calcEval("1+2+3")).toEqual({ ok: true, value: 6 });
	});

	it("multiplies, divides, and mods", () => {
		expect(calcEval("3 × 4")).toEqual({ ok: true, value: 12 });
		expect(calcEval("8 ÷ 2")).toEqual({ ok: true, value: 4 });
		expect(calcEval("10 % 3")).toEqual({ ok: true, value: 1 });
	});

	it("handles power and parentheses", () => {
		expect(calcEval("2^3")).toEqual({ ok: true, value: 8 });
		expect(calcEval("2**3")).toEqual({ ok: true, value: 8 });
		expect(calcEval("(1+2)*3")).toEqual({ ok: true, value: 9 });
		expect(calcEval("2^(1+2)")).toEqual({ ok: true, value: 8 });
	});

	it("respects precedence and right-assoc power", () => {
		expect(calcEval("2+3*4")).toEqual({ ok: true, value: 14 });
		expect(calcEval("2^3^2")).toEqual({ ok: true, value: 512 });
	});

	it("supports unary minus", () => {
		expect(calcEval("-5")).toEqual({ ok: true, value: -5 });
		expect(calcEval("-(2+3)")).toEqual({ ok: true, value: -5 });
		expect(calcEval("3*-2")).toEqual({ ok: true, value: -6 });
	});

	it("rejects bad input", () => {
		expect(calcEval("").ok).toBe(false);
		expect(calcEval("2 +").ok).toBe(false);
		expect(calcEval("abc").ok).toBe(false);
		expect(calcEval("1/0").ok).toBe(false);
	});
});

describe("formatCalcNumber", () => {
	it("formats integers and decimals", () => {
		expect(formatCalcNumber(4)).toBe("4");
		expect(formatCalcNumber(0.5)).toBe("0.5");
	});
});
