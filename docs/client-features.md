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
