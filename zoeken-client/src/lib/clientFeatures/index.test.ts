import { describe, expect, it } from "vitest";
import type { Config, SearchResult } from "../api";
import { applyClientFeatures } from "./index";

function result(url: string, overrides: Partial<SearchResult> = {}): SearchResult {
	return { url, title: "", ...overrides };
}

function config(overrides: Partial<Config> = {}): Config {
	return {
		instance_name: "test",
		version: "0",
		public_instance: false,
		engines: [],
		plugins: [],
		categories: [],
		default_locale: "en",
		locales: {},
		safe_search: 0,
		autocomplete: "",
		brand: {
			PRIVACYPOLICY_URL: null,
			CONTACT_URL: null,
			GIT_URL: "",
			GIT_BRANCH: "",
			DOCS_URL: "",
		},
		limiter: {
			enabled: false,
			"botdetection.ip_limit.link_token": false,
			"botdetection.ip_lists.pass_reserved_nets": false,
		},
		doi_resolvers: [],
		doi_resolver_urls: {},
		default_doi_resolver: "",
		client_ip: null,
		...overrides,
	};
}

function plugin(id: string, enabled: boolean) {
	return {
		id,
		name: id,
		description: "",
		enabled,
		default_enabled: enabled,
		kind: "result_plugin",
		keywords: [],
		preference_section: "",
		version: "1",
		api_version: 1,
		after: [],
		before: [],
		capabilities: [],
	};
}

describe("applyClientFeatures", () => {
	it("passes results through unchanged with no config", () => {
		const results = [result("https://example.com/a")];
		expect(applyClientFeatures(results, undefined)).toEqual(results);
	});

	it("skips hostname rules when the plugin is disabled", () => {
		const results = [result("https://spam.example.com/a")];
		const cfg = config({
			plugins: [plugin("hostnames", false)],
			hostnames: { replace: {}, remove: ["^spam\\."], high_priority: [], low_priority: [] },
		});
		expect(applyClientFeatures(results, cfg)).toEqual(results);
	});

	it("applies hostname removal when the plugin is enabled", () => {
		const results = [result("https://spam.example.com/a"), result("https://good.test/b")];
		const cfg = config({
			plugins: [plugin("hostnames", true)],
			hostnames: { replace: {}, remove: ["^spam\\."], high_priority: [], low_priority: [] },
		});
		const out = applyClientFeatures(results, cfg);
		expect(out).toHaveLength(1);
		expect(out[0].url).toBe("https://good.test/b");
	});

	it("rewrites DOIs when the plugin is enabled and a resolver URL is known", () => {
		const results = [result("https://publisher.test/10.1234/abc")];
		const cfg = config({
			plugins: [plugin("oa_doi_rewrite", true)],
			default_doi_resolver: "oadoi",
			doi_resolver_urls: { oadoi: "https://oadoi.org/" },
		});
		const out = applyClientFeatures(results, cfg);
		expect(out[0].url).toBe("https://oadoi.org/10.1234/abc");
	});

	it("does not rewrite DOIs when the resolver URL is missing", () => {
		const results = [result("https://publisher.test/10.1234/abc")];
		const cfg = config({
			plugins: [plugin("oa_doi_rewrite", true)],
			default_doi_resolver: "oadoi",
			doi_resolver_urls: {},
		});
		expect(applyClientFeatures(results, cfg)).toEqual(results);
	});

	it("strips trackers when tracker_url_remover is enabled", () => {
		const results = [result("https://example.com/?utm_source=x&keep=1")];
		const cfg = config({ plugins: [plugin("tracker_url_remover", true)] });
		const out = applyClientFeatures(results, cfg);
		expect(out[0].url).not.toContain("utm_source");
		expect(out[0].url).toContain("keep=1");
	});

	it("skips tracker stripping when the plugin is disabled", () => {
		const results = [result("https://example.com/?utm_source=x")];
		const cfg = config({ plugins: [plugin("tracker_url_remover", false)] });
		expect(applyClientFeatures(results, cfg)).toEqual(results);
	});

	it("lets preferences override /config defaults", () => {
		const results = [result("https://example.com/?utm_source=x")];
		const cfg = config({ plugins: [plugin("tracker_url_remover", true)] });
		expect(
			applyClientFeatures(results, cfg, {
				locale: "en",
				language: "en",
				categories: [],
				engines: [],
				safesearch: "Off",
				autocomplete: "",
				image_proxy: false,
				method: "POST",
				plugins: { tracker_url_remover: false },
			}),
		).toEqual(results);
	});
});
