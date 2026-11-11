import { describe, expect, it } from "vitest";
import { highlightPieces } from "./highlight";

describe("highlight", () => {
	it("marks whole technical tokens and never returns markup", () => {
		const pieces = highlightPieces("Use C++ and C#", "C++ C#");
		expect(pieces.filter((p) => p.mark).map((p) => p.text)).toEqual([
			"C++",
			"C#",
		]);
		expect(pieces.every((p) => !p.text.includes("<"))).toBe(true);
	});

	it("does not mark a term inside a longer word", () => {
		const pieces = highlightPieces("Email settings", "ai");
		expect(pieces.some((p) => p.mark)).toBe(false);
	});

	it("marks composed and decomposed cafe and skips operator keys", () => {
		const composed = highlightPieces("café", "café");
		expect(
			composed.some(
				(piece) => piece.mark && piece.text.normalize("NFC") === "café",
			),
		).toBe(true);
		const decomposed = highlightPieces("cafe\u0301", "café");
		expect(decomposed.some((piece) => piece.mark)).toBe(true);
		const operators = highlightPieces("site docs", "site:docs.rs rust");
		expect(
			operators.filter((piece) => piece.mark).map((piece) => piece.text),
		).toEqual([]);
		expect(
			highlightPieces("<b>rust</b>", "rust").every(
				(piece) => !piece.text.includes("<mark"),
			),
		).toBe(true);
		expect(
			highlightPieces("Москва", "Москва").some((piece) => piece.mark),
		).toBe(true);
		expect(
			highlightPieces("Straße", "straße").some((piece) => piece.mark),
		).toBe(true);
		expect(highlightPieces("東京", "東京").some((piece) => piece.mark)).toBe(
			true,
		);
		for (const word of ["Αθήνα", "القاهرة", "ירושלים", "서울"]) {
			expect(highlightPieces(word, word).some((piece) => piece.mark)).toBe(
				true,
			);
			expect(
				highlightPieces(word, word).every((piece) => !piece.text.includes("<")),
			).toBe(true);
		}
	});
});
