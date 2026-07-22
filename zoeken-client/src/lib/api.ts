/** Thin fetch helpers for zoeken-server (native search + SearXNG-compat prefs/config). */

export type {
	NativeAnswer as SearchAnswer,
	NativeCorrection,
	NativeInfobox as Infobox,
	NativeInfoboxUrl as InfoboxUrl,
	NativeInteractiveAnswer as InteractiveAnswer,
	NativeResult as SearchResult,
	NativeSearchRequest,
	NativeSearchResponse as SearchResponse,
	NativeSuggestion,
	NativeUnresponsiveEngine,
} from "./generated/native";

import { decode } from "@msgpack/msgpack";
import type {
	NativeResult,
	NativeSearchRequest,
	NativeSearchResponse,
} from "./generated/native";

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

async function getMsgpack<T>(path: string, init?: RequestInit): Promise<T> {
	const res = await fetch(path, {
		...init,
		headers: {
			Accept: "application/msgpack",
			...init?.headers,
		},
	});
	if (!res.ok) {
		throw new ApiError(res.status, await res.text());
	}
	return decode(new Uint8Array(await res.arrayBuffer())) as T;
}

export type SearchParams = {
	q: string;
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
	doi_resolver_urls: Record<string, string>;
	default_doi_resolver: string;
	using_tor_proxy?: boolean;
	categories_as_tabs?: string[];
	ui?: {
		results_on_new_tab: boolean;
		query_in_title: boolean;
		cache_url: string;
		search_on_category_select: boolean;
		hotkeys: string;
		url_formatting: string;
	};
	hostnames?: {
		replace: Record<string, string>;
		remove: string[];
		high_priority: string[];
		low_priority: string[];
	};
	/** Requester's IP as seen by the instance, for the self_info client feature. */
	client_ip: string | null;
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

/** Native typed search (`POST /api/v1/search`, MessagePack response). */
export function search(params: SearchParams) {
	const body: NativeSearchRequest = {
		q: params.q,
		pageno: params.pageno ?? 1,
		language: params.language ?? null,
		safesearch: params.safesearch ?? null,
		categories: params.categories ?? null,
		time_range: params.time_range ?? null,
		engines: params.engines ?? null,
	};
	return getMsgpack<NativeSearchResponse>("/api/v1/search", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(body),
	});
}

export type Suggestion = {
	text: string;
	subtext?: string;
	image?: string;
};

/** Query autocomplete (`GET /autocompleter`). XHR returns rich objects. */
export function autocomplete(q: string) {
	const qs = new URLSearchParams({ q });
	return getJson<Suggestion[]>(`/autocompleter?${qs}`, {
		headers: { "X-Requested-With": "XMLHttpRequest" },
	});
}

export type BangInfo = { shortcut: string; url: string };

/** Searchable external bangs (`GET /bangs?q=`). Empty `q` returns []. */
export function bangs(q: string, limit = 40) {
	const qs = new URLSearchParams({ q, limit: String(limit) });
	return getJson<BangInfo[]>(`/bangs?${qs}`);
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

export type EngineTiming = {
	engine: string;
	total_count: number;
	total_sum_seconds: number;
	total_avg_seconds: number;
	http_count: number;
	http_sum_seconds: number;
	http_avg_seconds: number;
};

export type StatsResponse = {
	engines: EngineTiming[];
};

export type EngineErrors = {
	engine: string;
	errors: Record<string, number>;
	total: number;
};

export type ErrorStatsResponse = {
	engines: EngineErrors[];
};

export function stats() {
	return getJson<StatsResponse>("/stats");
}

export function statsErrors() {
	return getJson<ErrorStatsResponse>("/stats/errors");
}

/** Thumbnail / preview URL across result kinds. */
export function resultThumbnail(result: NativeResult): string {
	if (result.kind === "main") return result.thumbnail;
	if (result.kind === "image") return result.thumbnail_src || result.img_src;
	return "";
}

export function resultImgSrc(result: NativeResult): string {
	if (result.kind === "image") return result.img_src;
	return "";
}

export function resultIframeSrc(result: NativeResult): string {
	return result.kind === "main" ? result.iframe_src : "";
}

export function resultFavicon(result: NativeResult): string {
	return result.kind === "main" ? result.favicon : "";
}
