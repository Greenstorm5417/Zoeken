# Client features

Former SearXNG / Zoeken server plugins mostly run in the SPA
(`zoeken-client/src/lib/clientFeatures/`). `/config` lists feature preference
ids and defaults; the SPA gates on `/preferences` (cookie + settings
`plugins.*.active`). There is no Lua runtime.

**Placement rule:** SPA owns cosmetic/prefs UX. Anything that is a
safety/moderation filter, must apply to `/search?format=json` and non-SPA
clients, ships a huge or sensitive blacklist, or would be weaker if the client
can skip it stays on the server. Audit conclusion: only `ahmia_filter` meets
that bar.

## Result transforms

| Id | Where | Behavior | Why |
| --- | --- | --- | --- |
| `hostnames` | SPA | Replace/remove hosts and re-sort by priority | Operator UX prefs; JSON clients get raw aggregation (intentional) |
| `oa_doi_rewrite` | SPA | Rewrite DOI links via `/config` resolvers | Display convenience; no safety requirement |
| `tracker_url_remover` | SPA | Strip tracker query params (client data) | Display/privacy cleanup for the UI; ~40 KiB ClearURLs snapshot is fine in the browser |
| `ahmia_filter` | **Server** | Drop onion results against the Ahmia MD5 blacklist when Tor proxy is enabled | Safety filter + ~1.9 MiB blacklist must not leave the instance / ship to every browser |

## Local answers

| Id | Behavior |
| --- | --- |
| `calculator` | Evaluate arithmetic expressions |
| `unit_converter` | Convert units (data from `zoeken-data`) |
| `self_info` | IP / user-agent cards |
| `time_zone` | Time zone helpers |
| `statistics` / `random` / date-time / crypto | Query-shaped local answers |

Network-backed instant answers (`weather`, `currency`, `dictionary`, `translate`,
Wikipedia/Wikidata) stay as engines on the server.

`infinite_scroll` remains a UI preference only. `tor_check` is not ported.
Unknown settings keys such as legacy `lua_plugins` are ignored.

## Query reading and results

`zoeken-query` owns `site:`, `filetype:`, quotes, and excluded terms, plus
intent. The string sent to engines is not rewritten. The SPA mirrors that
parse for chips and local answers (`src/lib/queryInterpretation.ts`). Chips
remove one operator and keep the rest of the query in the URL.

SafeSearch, time range, and language stay on the query. Engines that cannot
honor them are left out; the UI counts that reduction. General is a category,
not every enabled engine. Personal hide/prefer rules live in
`localStorage` (`zoeken.source-prefs.v1`), match a domain and its subdomains,
and are not sent to providers. Instance hostname rules stay separate.

Pagination uses `continuation`. `exhausted` stops. `more` means paging engines
returned a page, including a duplicate-only page; infinite scroll gives up
only after two such pages in a row. `unknown` allows one empty or duplicate
page so a later page is not discarded, then stops. Numbered next uses the
same rule. Counts are retrieved results. Local answers render even when the
predicted intent has no matching card, and before network search finishes.
A weather classification requests wttr.in. Tomorrow shows forecast fields
when the response has them, and does not substitute current conditions.

Title/snippet reranking is off by default. The representative evaluation did
not raise MRR, so `semantic_rerank_enabled()` is false. `ZOEK_SEMANTIC_RERANK=1`
turns it on for at most the top 40 titles and snippets. It does not fetch
documents. Init and each candidate score share a 5 ms budget; a timeout or an
empty embedding leaves the aggregated order. The daily-search fixture
(`cargo test -p zoeken-search --test daily_quality`) is a regression check,
not a live-quality score. Browser journeys are `bun run test:e2e` in
`zoeken-client` (Playwright, mocked providers). `ZOEK_LIVE_URL` opts into
the live smoke.
