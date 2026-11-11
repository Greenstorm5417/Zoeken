import { encode } from "@msgpack/msgpack";
import type { Page, Route } from "@playwright/test";

type Hit = { url: string; title: string; content: string };

export function installMocks(page: Page) {
	const searches: Array<{ q: string; pageno: number }> = [];
	return {
		searches,
		ready: Promise.all([
			page.route("**/config", (route) => json(route, e2eConfig())),
			page.route("**/preferences", (route) => {
				if (route.request().resourceType() === "document") return route.fallback();
				return json(route, {
					locale: "en",
					language: "en",
					categories: ["general"],
					engines: [],
					safesearch: "Off",
					autocomplete: "local",
					image_proxy: false,
					method: "POST",
					plugins: {},
				});
			}),
			page.route("**/autocompleter**", (route) =>
				json(route, [{ text: "rust language", subtext: "" }]),
			),
			page.route("**/api/v1/search", async (route) => {
				const body = route.request().postDataJSON() as { q?: string; pageno?: number };
				const q = body.q ?? "";
				const pageno = body.pageno ?? 1;
				searches.push({ q, pageno });
				if (q === "slow") {
					await new Promise((resolve) => setTimeout(resolve, 800));
				}
				const payload = responseFor(q, pageno);
				await route.fulfill({
					status: 200,
					headers: { "content-type": "application/msgpack" },
					body: Buffer.from(encode(payload)),
				});
			}),
		]),
	};
}

function json(route: Route, body: unknown) {
	return route.fulfill({
		status: 200,
		contentType: "application/json",
		body: JSON.stringify(body),
	});
}

export function e2eConfig(infinite = true) {
	const engine = (
		name: string,
		categories: string[],
		extra: Record<string, unknown> = {},
	) => ({
		name,
		categories,
		shortcut: name.slice(0, 1),
		enabled: true,
		paging: true,
		language_support: true,
		languages: ["en"],
		regions: [],
		safesearch: true,
		time_range_support: true,
		timeout: 3,
		...extra,
	});
	return {
		instance_name: "Zoeken",
		version: "test",
		public_instance: false,
		engines: [
			engine("web", ["general"]),
			engine("archives", ["general"], { time_range_support: false, paging: false }),
			engine("images", ["images"]),
			engine("map", ["map"]),
			engine("papers", ["science"]),
		],
		plugins: [
			{ id: "calculator", name: "Calculator", description: "", enabled: true, default_enabled: true, kind: "answer", keywords: [], preference_section: "" },
			{ id: "date_time", name: "Date", description: "", enabled: true, default_enabled: true, kind: "answer", keywords: [], preference_section: "" },
			...(infinite
				? [{ id: "infinite_scroll", name: "Infinite", description: "", enabled: true, default_enabled: true, kind: "ui", keywords: [], preference_section: "" }]
				: []),
		],
		categories: ["general", "images", "map", "news", "videos", "science"],
		categories_as_tabs: ["general", "images", "map", "news", "videos", "science"],
		default_locale: "en",
		locales: { en: "English" },
		safe_search: 0,
		autocomplete: "local",
		autocomplete_min: 1,
		brand: { PRIVACYPOLICY_URL: null, CONTACT_URL: null, GIT_URL: "", DOCS_URL: "" },
		limiter: {
			enabled: false,
			"botdetection.ip_limit.link_token": false,
			"botdetection.ip_lists.pass_reserved_nets": true,
		},
		doi_resolvers: [],
		doi_resolver_urls: {},
		default_doi_resolver: "",
		ui: {
			results_on_new_tab: false,
			search_on_category_select: true,
			url_formatting: "pretty",
			cache_url: "",
			hotkeys: "default",
		},
	};
}

function main(hit: Hit) {
	return {
		kind: "main",
		url: hit.url,
		title: hit.title,
		content: hit.content,
		engine: "web",
		engines: ["web"],
		category: "general",
		score: 1,
		positions: [1],
		priority: "",
		thumbnail: "",
		iframe_src: "",
		favicon: "",
		pretty_url: hit.url,
		published_date: null,
		length: "",
		author: "",
	};
}

function responseFor(q: string, pageno: number) {
	if (q === "fail") return page(q, [], "failed", "exhausted");
	if (q === "limited") return page(q, [], "rate_limited", "exhausted");
	if (q === "nothing") return page(q, [], "no_matches", "exhausted");
	if (q === "long") {
		if (pageno > 1) {
			return page(q, [main({ url: "https://later.example/gamma", title: "Gamma", content: "loaded later" })], "ok", "exhausted");
		}
		const hits = Array.from({ length: 12 }, (_, index) =>
			main({
				url: `https://long.example/${index}`,
				title: `Long result ${index + 1}`,
				content: "A tall snippet so the result list can scroll. ".repeat(8),
			}),
		);
		return page(q, hits, "ok", "more");
	}
	if (q === "pages") {
		if (pageno === 1) return page(q, [main({ url: "https://a.test/", title: "Alpha", content: "first" })], "ok", "more");
		if (pageno === 2) return page(q, [main({ url: "https://a.test/", title: "Alpha", content: "repeat" })], "ok", "more");
		return page(q, [main({ url: "https://b.test/", title: "Beta", content: "later" })], "ok", "exhausted");
	}
	if (q === "images") {
		return page(
			q,
			[
				{
					kind: "image",
					url: "https://img.example/1",
					title: "Red panda",
					content: "",
					engine: "images",
					engines: ["images"],
					score: 1,
					positions: [1],
					priority: "",
					img_src: "data:image/gif;base64,R0lGODlhAQABAAAAACwAAAAAAQABAAA=",
					thumbnail_src: "data:image/gif;base64,R0lGODlhAQABAAAAACwAAAAAAQABAAA=",
					resolution: "1x1",
					img_format: "gif",
					source: "example",
					filesize: "",
				},
				{
					kind: "image",
					url: "https://img.example/2",
					title: "Second",
					content: "",
					engine: "images",
					engines: ["images"],
					score: 1,
					positions: [2],
					priority: "",
					img_src: "data:image/gif;base64,R0lGODlhAQABAAAAACwAAAAAAQABAAA=",
					thumbnail_src: "data:image/gif;base64,R0lGODlhAQABAAAAACwAAAAAAQABAAA=",
					resolution: "1x1",
					img_format: "gif",
					source: "example",
					filesize: "",
				},
			],
			"ok",
			"exhausted",
		);
	}
	if (q === "placed") {
		return page(
			q,
			[
				{
					kind: "key_value",
					url: "https://maps.example/library",
					title: "Library",
					content: "A mapped place",
					engine: "map",
					engines: ["map"],
					score: 1,
					positions: [1],
					priority: "",
					caption: "",
					key_title: "",
					value_title: "",
					kvmap: [
						["latitude", "48.8"],
						["longitude", "2.3"],
					],
				},
			],
			"ok",
			"exhausted",
		);
	}
	if (q === "paper") {
		return page(
			q,
			[
				{
					kind: "paper",
					url: "https://papers.example/rust",
					title: "A paper",
					content: `${"Long abstract ".repeat(20)}café`,
					engine: "papers",
					engines: ["papers"],
					score: 1,
					positions: [1],
					priority: "",
					authors: ["Ada Lovelace"],
					doi: "",
					journal: "Notes",
					published_date: "2020-01-02",
					publisher: "",
					editor: "",
					volume: "",
					pages: "",
					number: "",
					type: "article",
					tags: [],
					issn: [],
					isbn: [],
					pdf_url: "",
					html_url: "",
					comments: "",
				},
			],
			"ok",
			"exhausted",
		);
	}
	if (q === "map") {
		return page(
			q,
			[
				{
					kind: "key_value",
					url: "https://maps.example/place",
					title: "No coordinates",
					content: "A place",
					engine: "map",
					engines: ["map"],
					score: 1,
					positions: [1],
					priority: "",
					caption: "",
					key_title: "",
					value_title: "",
					kvmap: [["note", "missing"]],
				},
			],
			"ok",
			"exhausted",
		);
	}
	return page(
		q,
		[main({ url: "https://docs.example.com/rust", title: "Rust docs", content: q })],
		"partial",
		"exhausted",
		[{ engine: "other", cause: "timeout" }],
	);
}

function page(
	q: string,
	results: unknown[],
	outcome: string,
	continuation: string,
	unresponsive: unknown[] = [],
) {
	return {
		schema_version: 3,
		query: q,
		number_of_results: results.length,
		results,
		answers: [],
		corrections: [],
		suggestions: [],
		infoboxes: [],
		unresponsive_engines: unresponsive,
		engine_data: {},
		continuation,
		outcome,
		operator_notes: [],
		elapsed_ms: 12,
		interpretation: {
			intent: "general",
			confidence: 0.4,
			source: "rules",
			site: null,
			filetype: null,
			phrases: [],
			excluded: [],
			residual: q,
			entities: [],
		},
	};
}
