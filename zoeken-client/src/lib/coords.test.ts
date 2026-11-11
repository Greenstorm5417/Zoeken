import { describe, expect, it } from "vitest";
import type { SearchResult } from "./api";
import { coordsFromResult, parseCoord, validLatLon } from "./coords";

const main = (url: string): SearchResult =>
	({
		kind: "main",
		url,
		title: "Place",
		content: "",
		engine: "osm",
		engines: ["osm"],
		category: "map",
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
	}) as SearchResult;

describe("coordinates", () => {
	it("does not turn a missing value into zero", () => {
		expect(parseCoord("")).toBeNull();
		expect(parseCoord(null)).toBeNull();
		expect(coordsFromResult(main("https://osm.example/node"))).toBeNull();
	});

	it("keeps a real zero and rejects out-of-range numbers", () => {
		expect(parseCoord("0")).toBe(0);
		expect(validLatLon(0, 0)).toBe(true);
		expect(validLatLon(91, 0)).toBe(false);
		const gulf = coordsFromResult(main("https://osm.example/?mlat=0&mlon=0"));
		expect(gulf).toMatchObject({ lat: 0, lon: 0 });
	});

	it("prefers structured key-value coordinates over the URL", () => {
		const result = {
			kind: "key_value",
			url: "https://osm.example/?mlat=&mlon=",
			title: "Null island",
			content: "",
			engine: "osm",
			engines: ["osm"],
			score: 1,
			positions: [1],
			priority: "",
			caption: "",
			key_title: "",
			value_title: "",
			kvmap: [
				["latitude", "48.8566"],
				["longitude", "2.3522"],
			],
		} as SearchResult;
		expect(coordsFromResult(result)).toMatchObject({
			lat: 48.8566,
			lon: 2.3522,
		});
	});
});
