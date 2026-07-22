/** "sum 1 2 3" / "avg 1 2" style statistics answerer. */
import type { SearchAnswer } from "../api";

const OPS = ["min", "max", "avg", "sum", "prod", "range", "median"] as const;
type StatOp = (typeof OPS)[number];

function compute(op: StatOp, nums: number[]): number {
	switch (op) {
		case "min":
			return Math.min(...nums);
		case "max":
			return Math.max(...nums);
		case "avg":
			return nums.reduce((a, b) => a + b, 0) / nums.length;
		case "sum":
			return nums.reduce((a, b) => a + b, 0);
		case "prod":
			return nums.reduce((a, b) => a * b, 1);
		case "range":
			return Math.max(...nums) - Math.min(...nums);
		case "median": {
			const sorted = [...nums].sort((a, b) => a - b);
			const n = sorted.length;
			return n % 2 === 1
				? sorted[(n - 1) / 2]
				: (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
		}
	}
}

function formatNumber(value: number): string {
	if (Number.isInteger(value) && Math.abs(value) < 1e15) {
		return String(value);
	}
	return String(value);
}

export function computeStatisticsAnswer(query: string): SearchAnswer | null {
	const tokens = query.trim().split(/\s+/).filter(Boolean);
	if (tokens.length === 0) return null;
	const op = OPS.find((candidate) => candidate === tokens[0]);
	if (!op) return null;

	const argTokens = tokens.slice(1);
	if (argTokens.length === 0) return null;
	const nums: number[] = [];
	for (const token of argTokens) {
		const value = Number(token);
		if (!Number.isFinite(value) || token.trim() === "") return null;
		nums.push(value);
	}

	const result = compute(op, nums);
	const args = nums.map(formatNumber).join(", ");
	return {
		answer: `${op}(${args}) = ${formatNumber(result)}`,
		engine: "statistics",
	};
}
