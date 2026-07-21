/** Thin fetch helpers for the SearXNG-compatible zoeken-server API. */

export class ApiError extends Error {
	status: number;

	constructor(status: number, message: string) {
		super(message);
		this.name = "ApiError";
		this.status = status;
	}
}

async function getJson<T>(path: string, init?: RequestInit): Promise<T> {
	const res = await fetch(path, {
		...init,
		headers: {
			Accept: "application/json",
			...init?.headers,
		},
	});
	if (!res.ok) {
		throw new ApiError(res.status, await res.text());
	}
	return res.json() as Promise<T>;
}

export type SearchParams = {
	q: string;
	format?: "json" | "csv" | "rss";
	pageno?: number;
	language?: string;
	safesearch?: 0 | 1 | 2;
	categories?: string;
	time_range?: string;
	engines?: string;
};

export type EngineInfo = {
	name: string;
	categories: string[];
	shortcut: string;
	enabled: boolean;
	paging: boolean;
	language_support: boolean;
	languages: string[];
	regions: string[];
	safesearch: boolean;
	time_range_support: boolean;
	timeout: number;
};

export type Config = {
	instance_name: string;
	version: string;
	public_instance: boolean;
	engines: EngineInfo[];
	plugins: Array<{
		id: string;
		name: string;
		description: string;
		enabled: boolean;
		default_enabled: boolean;
		kind: string;
		keywords: string[];
		preference_section: string;
		version: string;
		api_version: number;
		after: string[];
		before: string[];
		capabilities: string[];
	}>;
	categories: string[];
	default_locale: string;
	locales: Record<string, string>;
	safe_search: number;
	autocomplete: string;
	autocomplete_min?: number;
	autocomplete_backends?: string[];
	brand: {
		PRIVACYPOLICY_URL: string | null;
		CONTACT_URL: string | null;
		GIT_URL: string;
		GIT_BRANCH: string;
		DOCS_URL: string;
	};
	limiter: {
		enabled: boolean;
		"botdetection.ip_limit.link_token": boolean;
		"botdetection.ip_lists.pass_reserved_nets": boolean;
	};
	doi_resolvers: string[];
	default_doi_resolver: string;
	categories_as_tabs?: string[];
	ui?: {
		center_alignment: boolean;
		results_on_new_tab: boolean;
		query_in_title: boolean;
		cache_url: string;
		search_on_category_select: boolean;
		hotkeys: string;
		url_formatting: string;
	};
};

export type Preferences = {
	locale: string;
	language: string;
	categories: string[];
	engines: string[];
	safesearch: "Off" | "Moderate" | "Strict";
	autocomplete: string;
	image_proxy: boolean;
	method: "GET" | "POST";
	plugins: Record<string, boolean>;
};

export type SearchResult = {
	url: string;
	title: string;
	content?: string;
	engine?: string;
	engines?: string[];
	category?: string;
	pretty_url?: string;
	thumbnail?: string;
	favicon?: string;
	img_src?: string;
	iframe_src?: string;
	template?: string;
	publishedDate?: string;
	// Torrent / file results (files.html)
	magnetlink?: string;
	seed?: number;
	leech?: number;
	filesize?: string;
	filename?: string;
	// Paper results (paper.html)
	authors?: string[];
	journal?: string;
	doi?: string;
	publisher?: string;
	pdf_url?: string;
	html_url?: string;
	tags?: string[];
	// Code results (code.html)
	repository?: string;
	codelines?: Array<[number, string]>;
	code_language?: string;
	// Key-value results (keyvalue.html)
	kvmap?: Record<string, string>;
	// Image results (images.html)
	resolution?: string;
	img_format?: string;
	source?: string;
};

export type SearchAnswer = {
	answer: string;
	url?: string;
	engine?: string;
};

export type InfoboxUrl = {
	title: string;
	url: string;
};

export type Infobox = {
	infobox: string;
	id?: string | null;
	content?: string;
	img_src?: string | null;
	urls?: InfoboxUrl[];
	attributes?: Array<{ label: string; value?: string }>;
	related_topics?: string[];
	engine?: string;
};

export type SearchResponse = {
	query: string;
	number_of_results?: number;
	results: SearchResult[];
	answers: SearchAnswer[];
	corrections: Array<string | { correction: string; url?: string }>;
	infoboxes: Infobox[];
	suggestions: Array<string | { suggestion: string }>;
	unresponsive_engines: Array<[string, string]>;
};

export function search(params: SearchParams) {
	const body = new URLSearchParams();
	body.set("q", params.q);
	body.set("format", params.format ?? "json");
	if (params.pageno != null) body.set("pageno", String(params.pageno));
	if (params.language) body.set("language", params.language);
	if (params.safesearch != null)
		body.set("safesearch", String(params.safesearch));
	if (params.categories) body.set("categories", params.categories);
	if (params.time_range) body.set("time_range", params.time_range);
	if (params.engines) body.set("engines", params.engines);
	return getJson<SearchResponse>("/search", {
		method: "POST",
		headers: { "Content-Type": "application/x-www-form-urlencoded" },
		body,
	});
}

export function autocomplete(q: string) {
	const qs = new URLSearchParams({ q });
	return getJson<[string, string[]]>(`/autocompleter?${qs}`);
}

export function config() {
	return getJson<Config>("/config");
}

export function preferencesGet() {
	return getJson<Preferences>("/preferences", { credentials: "same-origin" });
}

export function preferencesPost(preferences: Preferences) {
	const body = new URLSearchParams({
		locale: preferences.locale,
		language: preferences.language,
		categories: preferences.categories.join(","),
		engines: preferences.engines.join(","),
		safesearch: String(
			{ Off: 0, Moderate: 1, Strict: 2 }[preferences.safesearch],
		),
		autocomplete: preferences.autocomplete,
		image_proxy: preferences.image_proxy ? "1" : "0",
		method: preferences.method,
	});
	for (const [id, enabled] of Object.entries(preferences.plugins ?? {})) {
		body.set(`plugin_${id}`, enabled ? "1" : "0");
	}
	return getJson<Preferences>("/preferences", {
		method: "POST",
		credentials: "same-origin",
		headers: { "Content-Type": "application/x-www-form-urlencoded" },
		body,
	});
}

export async function clearCookies() {
	const response = await fetch("/clear_cookies", {
		method: "GET",
		credentials: "same-origin",
	});
	if (!response.ok) throw new ApiError(response.status, await response.text());
}
