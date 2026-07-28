# Intentional Differences & Unsupported SearXNG Features

Deliberate compatibility gaps between Zoeken and SearXNG.

## TLS certificate verification

- **Behavior**: TLS verification is always enabled for outbound engine traffic.
  `outgoing.verify` / named-network `verify` may only be `true` (or omitted).
  `verify: false` and custom CA path strings are rejected at network build with a
  clear error. There is no per-request verify flag on the engine request model.
- **Why**: Custom trust stores and per-request disable are not implemented;
  silent ignore would be unsafe.
- **Security posture**: Fails **closed** — verification cannot be turned off.
- **Impact**: Engines that need a self-signed host or private CA cannot be
  configured; use a terminating proxy with a public CA if required.
- **Revisit when**: an engine must target an internal instance needing a custom CA.

## No Lua / server plugins

- **Behavior**: Lua plugins and the generic server plugin pipeline are removed.
  Most former plugin UX lives in the SPA
  ([`docs/client-features.md`](../client-features.md)). External JSON/CSV/RSS
  clients get raw aggregation for SPA-owned transforms (hostnames, DOI rewrite,
  tracker stripping). **Exception:** `ahmia_filter` still runs on the server
  when `outgoing.using_tor_proxy` is true and the preference is enabled, so
  blacklisted onion URLs never leave the instance. No other former plugin was
  judged “must be server” (tracker strip stays SPA display cleanup).
- **Impact**: Custom `.lua` plugins are unsupported. Preference ids still appear
  on `/config` (SPA gating for client features; server gating for ahmia).

## Engine health (storage circuit only)

- **Behavior**: Engine cooldowns are owned by the durable storage circuit.
  `search.suspended_times` / `ban_time_on_fail` map onto circuit policy
  (`SuspensionPolicy`); there is no second in-process suspend gate.
  DuckDuckGo captcha opens **no** circuit (SearXNG-compatible: no IP ban).
- **Impact**: Operators tuning `suspended_times` change circuit cooldowns, not a
  separate process-local suspend table. Multi-replica instances share health via
  storage.
- **Revisit when**: half-open probe policy needs richer knobs than durations.

## Engine list merge policy

- **Behavior**: Empty `engines:` → built-in catalog. Non-empty +
  `search.engine_list_mode: replace` (default) → listed engines only.
  `engine_list_mode: merge` overlays listed entries onto the built-in catalog.
- **Impact**: Historical Zoeken configs keep replace semantics; merge is opt-in.
- **Revisit when**: merge becomes the safer default for partial overlays.

## Command engines

- **Behavior**: SearXNG's `engine: command` type is **not supported**. It is
  deliberately absent from the safe `Processor` enum (`online` / `offline` only).
- **Why**: Command engines spawn OS processes with user-influenced input.
- **Security posture**: Hardening choice — no configuration causes zoeken to shell out.
- **Impact**: `settings.yml` entries using `engine: command` are unsupported.
- **Revisit when**: a sandboxed execution model is designed and explicitly approved.

## OnlineCurrency / OnlineDictionary / OnlineUrlSearch processors

- **Behavior**: These SearXNG processor specializations are **not** in Zoeken's
  `Processor` enum. Instant answers that need HTTP are normal online engines
  (`currency`, `dictionary`, …). Engines that required the old specializations
  (e.g. SearXNG `currency_convert`) stay intentionally skipped.
- **Revisit when**: porting an engine that cannot be expressed as a normal online engine.

## Remaining bespoke engines

- **Behavior**: Engines that need API keys, live preflight, or large bespoke scrapers
  are `intentionally-skipped` in `docs/compatibility/engines.json`. Reasons live in
  `tools/compat_inventory.py` `INTENTIONALLY_SKIPPED`.
- **Revisit when**: credentials/preflight support lands, or a generic pattern covers them.

## SQLite fixtures

- **Behavior**: SQLite is settings-driven and covered by unit tests. There is no
  checked-in conformance fixture (needs a local DB path).
- **Revisit when**: the conformance harness gains temp-DB fixture support.

## Tracker patterns refresh

- **Behavior**: `tracker_patterns.json` is a ClearURLs provider snapshot used by the
  SPA `tracker_url_remover` client feature (copied into
  `zoeken-client/src/lib/generated/` via `bun run sync-data`). It is **not**
  embedded in the server binary. Runtime does not fetch ClearURLs on boot.
- **Refresh**:
  ```sh
  uv run --no-project --python 3.13 tools/fetch_tracker_patterns.py
  cd zoeken-client && bun run sync-data
  ```
  Writes `zoeken/zoeken-data/data/tracker_patterns.json`, then syncs the SPA copy.
- **Revisit when**: operators want live rule updates without rebuild.

## Frontend / static routes

- **Behavior**: `/about` is the SPA route. `/info/{locale}/{page}` redirects to
  `/about`. `/logo/{resolution}` serves `zoeken-logo.svg` from the assets directory.
  `/rss.xsl` is a static file in `assets/`. `/client{token}.css` remains a link-token
  ping with empty CSS.
- **Assets**: Not rust-embedded — loaded from `./assets` (or `APP_ASSETS_DIR`).
- **Data**: Defaults are precompiled into `zoeken-data`. Setting `APP_DATA_DIR`
  loads a full JSON bundle from that directory (it does not merge overrides onto
  the embedded defaults).

## Autocomplete backends

- **Behavior**: All 18 SearXNG `autocomplete.backends` names are registered.
  An unknown `settings.search.autocomplete` name disables autocomplete. DBpedia uses
  a light `<Label>` string extract instead of a full XML DOM.
- **Rich suggestions**: The SPA (`X-Requested-With: XMLHttpRequest`) receives
  objects `{ text, subtext?, image? }`. OpenSearch / non-XHR still gets
  `[query, [string, ...]]`. Brave uses `?rich=true` and may populate `subtext` /
  `image`; other backends fill `text` only. Suggestion thumbnails go through
  `/image_proxy` when the image proxy is enabled.

## DOI resolver preference

- **Behavior**: `/config` exposes `doi_resolvers` / `default_doi_resolver`. The
  SPA applies the instance default resolver when `oa_doi_rewrite` is enabled.
  There is no per-user DOI preference cookie field (unlike SearXNG).
- **Revisit when**: `oa_doi_rewrite` needs per-request resolver overrides.

## UI theme (SPA)

- **Behavior**: Theme is SPA-only via `localStorage` (`zoeken-client` theme helper:
  light/dark/system). `/config` does not expose `themes` / `default_theme`, and
  settings no longer ship SearXNG `ui.default_theme` / `theme_args`. The prefs
  cookie still accepts and round-trips a `theme` field for SearXNG client
  compatibility; Zoeken does not use it for UI.

## Zoeken-only engines

- **Behavior**: `wikibooks` is a MediaWiki books engine shipped in Zoeken with no
  distinct SearXNG engine module. It is tracked as `zoeken_only` in
  `engines.json` / `engines.md`, not as an accidental orphan.
- **Revisit when**: upstream adds a matching module or the engine is retired.

## Stats / metrics Basic auth

- **Behavior**: `general.open_metrics` is the HTTP Basic password for `/metrics`,
  `/stats`, and `/stats/errors`. Empty hides `/metrics` (404) and **denies**
  `/stats` JSON (401). The SPA `/stats` shell stays public and shows a
  configure-auth message on 401.
- **Why**: safer default for public instances — empty password must not expose
  engine timing/error aggregates.
- **Impact**: operators must set `open_metrics` to use stats/metrics; browsers
  without Basic credentials see the SPA message instead of live stats JSON.

## No CORS middleware

- **Behavior**: CORS is not enabled. The SPA is same-origin with the API.
- **Security posture**: Avoids accidental open CORS.

## Native search API (Zoeken-only)

- **Behavior**: The SPA consumes `POST /api/v1/search` (MessagePack by default)
  with a typed, tagged result schema. **Public stability:**
  `NATIVE_SCHEMA_VERSION` / response field `schema_version` is currently **`2`**.
  Breaking wire changes bump that constant; additive fields may appear within
  the same major schema version when documented.
- **Why**: Preserve a frozen-ish compat layer while giving the SPA full field
  parity (paper citations, torrent `time`, `hl_lines`, corrections/suggestions
  with engines, etc.) without polluting the legacy bag-of-fields JSON.
- **Impact**: Third-party JSON clients must continue using `/search?format=json`.
  Native API consumers should reject unknown `schema_version` values they do
  not implement.
- **Revisit when**: `schema_version` needs a breaking bump (document in CHANGELOG).

## Image / favicon proxy redirects

- **Behavior**: Both fetchers use `redirect::Policy::none()`. Bodies are size-capped.
  Each hop also runs `validate_resolved_url` (DNS resolve + private/link-local IP
  block) before connect.
- **Residual**: A TOCTOU race between lookup and connect remains; see
  `docs/security/audit.md`.

## No first-party YouTube / Google Images engines (1.4.0)

- **Behavior**: Zoeken does **not** ship dedicated YouTube or Google Images
  engines. Images use Bing Images (and other image engines). Videos use
  Invidious / Piped / PeerTube / etc.
- **Why**: Avoid brittle Google/YouTube scraping and API-key surface for 1.4.0.
- **Impact**: Prefer `bing images`, `invidious`, `piped` in settings; see
  `docs/compatibility/engines.md` for skipped upstream modules.

## Engine trait inventory vs Rust ports (majors)

- **Behavior**: For google / startpage / wikipedia, Rust traits match the
  compatibility inventory. Bing web paging is **enabled** in Rust (`first=`
  offset) even when older inventory rows said `paging: false`. DuckDuckGo and
  Brave Rust ports advertise paging / safesearch / time / lang where the HTML
  request path implements them; inventory rows that still say `false` are
  stale upstream-export noise — treat the Rust `EngineMeta` as source of truth
  until the next inventory refresh.
- **Why**: Inventory flags are scraped from SearXNG modules and can lag ports.
- **Impact**: Prefer live engine metadata from `/config` over `engines.json`
  for operator UX.
