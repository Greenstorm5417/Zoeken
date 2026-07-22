# Compatibility Targets

| Level | Scope | Pass criteria |
| --- | --- | --- |
| API compatibility | `/search`, `/config`, `/stats`, `/stats/errors`, `/metrics`, OpenSearch, image proxy, autocomplete, and JSON/RSS/CSV response shapes | Same route, method, status, content type, core schema fields, and error shape for supported formats. HTML may differ. |
| Admin/config compatibility | SearXNG `settings.yml`, engine/plugin settings, outgoing network settings, data assets, and metrics labels | Supported settings parse without loss, unknown settings are preserved or warned, runtime uses configured engines/plugins/networks, and metrics/config endpoints reflect settings. |
| Engine behavior compatibility | Engine selection, request construction, result parsing, paging, language, safe search, time range, suspension, and scoring | For every ported engine, golden fixtures match request parameters and normalized results; unsupported engine features are listed in `engines.json`. |
| Full HTML compatibility | SearXNG web routes, templates, static assets, preferences UI, cookies, redirects, and localized pages | Same user-visible route behavior — **not** a Zoeken goal (SPA instead). |

Supported target: API compatibility plus admin/config compatibility.

## Frontend

Custom SPA (`zoeken-client`) against Zoeken native search plus SearXNG-compatible prefs/config.

- Build output: `zoeken/zoeken-server/assets` (not rust-embedded).
- `/` and `/search?format=html` serve the SPA; results come from **`POST /api/v1/search`** (SPA uses MessagePack via `Accept: application/msgpack`; JSON still available for curl/compat).
- External clients keep SearXNG-compatible `/search?format=json|csv|rss`.
- No Jinja/SearXNG theme parity.

## Data packaging

- Defaults (bangs/currencies/units/…) are precompiled into `zoeken-data`.
- `APP_DATA_DIR` loads JSON overrides from disk when needed.
