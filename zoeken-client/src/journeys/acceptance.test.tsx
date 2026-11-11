// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WeatherAnswer } from "#/components/answers/WeatherAnswer";
import { ImageLightbox } from "#/components/ImageLightbox";
import { ResultItem } from "#/components/ResultTemplates";
import { presentAnswers, weatherPresentation } from "#/lib/answers";
import type { SearchAnswer, SearchResult } from "#/lib/api";
import { search } from "#/lib/api";
import { coordsFromResult } from "#/lib/coords";
import { engineEligibility } from "#/lib/eligibility";
import { highlightPieces } from "#/lib/highlight";
import { nextPageNumber, showNumberedNext } from "#/lib/pagination";
import { interpret, removeOperator } from "#/lib/queryInterpretation";
import {
	applySourcePrefs,
	toggleHidden,
	togglePreferred,
} from "#/lib/sourcePrefs";

afterEach(() => {
	cleanup();
});

function main(url: string, title: string, content = ""): SearchResult {
	return {
		kind: "main",
		url,
		title,
		content,
		engine: "demo",
		engines: ["demo"],
		category: "general",
		score: 1,
		positions: [1],
		priority: "",
		thumbnail: "",
		iframe_src: "",
		favicon: "",
		pretty_url: "",
		published_date: null,
		length: "",
		author: "",
	};
}

function image(url: string, title: string): SearchResult {
	return {
		kind: "image",
		url,
		title,
		content: "",
		engine: "demo",
		engines: ["demo"],
		score: 1,
		positions: [1],
		priority: "",
		img_src: "",
		thumbnail_src: "",
		resolution: "800x600",
		img_format: "jpeg",
		filesize: "",
		source: "example.test",
	};
}

describe("search acceptance journeys", () => {
	it("keeps operator meaning when a chip is removed and shares the edited query", () => {
		const original = 'site:docs.rs "async trait" filetype:html -sponsor rust';
		const withoutSite = removeOperator(original, "site", "docs.rs");
		expect(withoutSite).not.toContain("site:docs.rs");
		expect(withoutSite).toContain('"async trait"');
		expect(withoutSite).toContain("filetype:html");
		expect(withoutSite).toContain("-sponsor");
		expect(interpret(withoutSite).operators.filetype).toBe("html");
	});

	it("stops numbered and infinite pagination on exhaustion and duplicate pages", () => {
		const page = {
			continuation: "exhausted",
			results: [{ url: "https://a.test/" }],
		};
		expect(showNumberedNext(page)).toBe(false);
		expect(nextPageNumber([page], 1)).toBeUndefined();
		const unknown = {
			continuation: "unknown",
			results: [{ url: "https://a.test/" }],
		};
		const repeat = {
			continuation: "unknown",
			results: [{ url: "https://a.test/" }],
		};
		expect(nextPageNumber([unknown, repeat], 1)).toBe(3);
		expect(nextPageNumber([unknown, repeat, repeat], 1)).toBeUndefined();
		expect(showNumberedNext(unknown)).toBe(true);
		expect(showNumberedNext(repeat, [unknown, repeat])).toBe(false);
	});

	it("shows a local answer when the network result is missing", () => {
		const local: SearchAnswer = {
			answer: "4",
			url: null,
			engine: "calculator",
			interactive: { type: "calculator", expression: "2 + 2", result: 4 },
		};
		expect(presentAnswers([local], [], "2 + 2")).toEqual([local]);
		expect(weatherPresentation("weather tomorrow in paris", "", "")).toBe(
			"unavailable",
		);
	});

	it("hides and prefers a domain without sending it anywhere", () => {
		const results = [
			main("https://docs.example.com/a", "Docs"),
			main("https://other.test/b", "Other"),
		];
		const hidden = applySourcePrefs(
			results,
			toggleHidden({ hidden: [], preferred: [] }, "example.com"),
		);
		expect(hidden.results.map((result) => result.url)).toEqual([
			"https://other.test/b",
		]);
		expect(hidden.hiddenCount).toBe(1);
		const preferred = applySourcePrefs(
			results,
			togglePreferred({ hidden: [], preferred: [] }, "other.test"),
		);
		expect(preferred.results[0]?.url).toBe("https://other.test/b");
	});

	it("does not treat a blank coordinate as zero", () => {
		expect(
			coordsFromResult(main("https://osm.example/?mlat=&mlon=", "Missing")),
		).toBeNull();
		expect(
			coordsFromResult(main("https://osm.example/?mlat=0&mlon=0", "Gulf")),
		).toMatchObject({
			lat: 0,
			lon: 0,
		});
	});

	it("highlights query terms without matching inside technical tokens", () => {
		const pieces = highlightPieces("Email about C++", "ai");
		expect(pieces.some((piece) => piece.mark)).toBe(false);
		const hit = highlightPieces("async trait rust", "trait");
		expect(
			hit.some((piece) => piece.mark && piece.text.toLowerCase() === "trait"),
		).toBe(true);
	});

	it("treats general as its own category when filters remove engines", () => {
		const engines = [
			{
				name: "wiki",
				enabled: true,
				categories: ["general"],
				safesearch: true,
				time_range_support: false,
				language_support: true,
			},
			{
				name: "images",
				enabled: true,
				categories: ["images"],
				safesearch: true,
				time_range_support: true,
				language_support: false,
			},
		];
		const narrowed = engineEligibility(engines as never, {
			category: "general",
			timeRange: "week",
			language: "",
			safesearch: 0,
			engines: "",
		});
		expect(narrowed.inCategory).toBe(1);
		expect(narrowed.eligible).toBe(0);
		expect(narrowed.timeReduced).toBe(true);
	});

	it("passes abort signals through search so a superseded request can stop", async () => {
		const fetch = vi
			.fn()
			.mockImplementation((_url: string, init?: RequestInit) => {
				if (init?.signal?.aborted) {
					return Promise.reject(new DOMException("Aborted", "AbortError"));
				}
				return Promise.resolve(new Response(new Uint8Array(), { status: 204 }));
			});
		const previous = globalThis.fetch;
		globalThis.fetch = fetch as unknown as typeof globalThis.fetch;
		const controller = new AbortController();
		controller.abort();
		await expect(search({ q: "rust" }, controller.signal)).rejects.toBeTruthy();
		expect(fetch).toHaveBeenCalled();
		const init = fetch.mock.calls[0]?.[1] as RequestInit;
		expect(init.signal).toBe(controller.signal);
		globalThis.fetch = previous;
	});

	it("renders a result row with one breadcrumb, highlight, and hide", () => {
		const onHide = vi.fn();
		render(
			<ResultItem
				result={main(
					"https://docs.example.com/guide/intro",
					"Async trait guide",
					"A long explanation of async trait that should stay plain text and not become html <script>.",
				)}
				query="trait"
			/>,
		);
		expect(screen.getByRole("heading").textContent).toBe("Async trait guide");
		expect(screen.getByText("docs.example.com › guide › intro")).toBeTruthy();
		expect(document.querySelectorAll("mark").length).toBeGreaterThan(0);
		expect(document.body.innerHTML).not.toContain("<script>");
		cleanup();
		render(
			<ResultItem
				result={main("https://docs.example.com/a", "Docs", "short")}
				query="docs"
				onHideSite={onHide}
			/>,
		);
		fireEvent.click(
			screen.getByRole("button", { name: /hide docs\.example\.com/i }),
		);
		expect(onHide).toHaveBeenCalledWith("docs.example.com");
	});

	it("moves the image lightbox with the keyboard and closes on escape", () => {
		const onIndex = vi.fn();
		const onClose = vi.fn();
		render(
			<ImageLightbox
				results={[
					image("https://example.test/a", "One"),
					image("https://example.test/b", "Two"),
				]}
				index={0}
				onIndex={onIndex}
				onClose={onClose}
			/>,
		);
		expect(screen.getByRole("dialog")).toBeTruthy();
		expect(screen.getByText("1 / 2")).toBeTruthy();
		fireEvent.keyDown(document, { key: "ArrowRight" });
		expect(onIndex).toHaveBeenCalledWith(1);
		fireEvent.keyDown(document, { key: "Escape" });
		expect(onClose).toHaveBeenCalled();
	});

	it("does not present current weather as tomorrow", () => {
		const answer: SearchAnswer = {
			answer: "current",
			url: "https://wttr.in/Paris",
			engine: "wttr",
			interactive: {
				type: "weather",
				place: "Paris, France",
				description: "Sunny",
				temp_c: "18",
				temp_f: "64",
				feels_c: "18",
				wind_kmph: "5",
				wind_dir: "N",
				humidity: "40",
				observation_time: "",
				forecast_date: "",
				forecast_summary: "",
				forecast_temp_c: "",
				forecast_temp_f: "",
			},
		};
		render(
			<WeatherAnswer
				answer={answer}
				initial={
					answer.interactive as Extract<
						NonNullable<SearchAnswer["interactive"]>,
						{ type: "weather" }
					>
				}
				query="weather tomorrow in paris"
			/>,
		);
		expect(
			screen.getByText(/tomorrow’s forecast isn’t in this response/i),
		).toBeTruthy();
		expect(screen.queryByText("18°C (64°F)")).toBeNull();
	});
});
