import { describe, expect, it } from "vitest";
import {
	interpret,
	parseOperators,
	removeOperator,
} from "./queryInterpretation";

describe("query interpretation mirror", () => {
	it("extracts operators without rewriting the original", () => {
		const parsed = interpret(
			'site:docs.rs "async trait" filetype:html -sponsor rust',
		);
		expect(parsed.original).toContain("site:docs.rs");
		expect(parsed.operators.site).toBe("docs.rs");
		expect(parsed.operators.filetype).toBe("html");
		expect(parsed.operators.phrases).toEqual(["async trait"]);
		expect(parsed.operators.excluded).toEqual(["sponsor"]);
		expect(parsed.residual).toBe("async trait rust");
	});

	it("protects compiler flags and false-positive arithmetic", () => {
		expect(
			parseOperators("gcc -std=c++17 main.cpp").operators.excluded,
		).toEqual([]);
		expect(interpret("2 fast 2 furious").intent).toBe("general");
		expect(interpret("C++").intent).not.toBe("arithmetic");
		expect(interpret("12 * 8").intent).toBe("arithmetic");
	});

	it("uses the model only for the sky paraphrase", () => {
		const sky = interpret("how's the sky in paris");
		expect(sky.intent).toBe("weather");
		expect(sky.source).toBe("model");
		expect(interpret("meeting in paris tomorrow").intent).toBe("general");
		expect(interpret("weather tomorrow in Paris, Texas").intent).toBe(
			"weather",
		);
	});

	it("removes a chip without dropping the rest of the query", () => {
		expect(removeOperator("site:example.com rust", "site", "example.com")).toBe(
			"rust",
		);
		expect(removeOperator('rust "async trait"', "phrase", "async trait")).toBe(
			"rust",
		);
		expect(removeOperator("rust -sponsor", "excluded", "sponsor")).toBe("rust");
		expect(removeOperator("filetype:.PDF rust", "filetype", "pdf")).toBe(
			"rust",
		);
		expect(removeOperator('site:"docs.rs" rust', "site", "docs.rs")).toBe(
			"rust",
		);
		expect(removeOperator("site:a.com site:b.com rust", "site", "a.com")).toBe(
			"site:b.com rust",
		);
	});
});
