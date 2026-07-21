# Intentional Differences & Unsupported SearXNG Features

Deliberate compatibility gaps between Zoeken and SearXNG.

## Per-request TLS certificate verification

- **Behavior**: `RequestParams.verify` (`TlsVerify::Default | Disabled | CaFile`) is
  accepted on the engine request, but TLS verification is configured only at the
  network-client level (`build_client` → `.cert_verification(config.verify)` in
  `zoeken-network/src/lib.rs`). The per-request `NetworkRequest.verify` flag is
  stored but not consumed at send time.
  - `TlsVerify::Disabled` does **not** disable verification — the request fails with a
    TLS error instead of connecting insecurely.
  - `TlsVerify::CaFile(path)` maps to `verify(true)` but the custom CA is **not**
    loaded into the trust store, so hosts using a private CA fail against the system
    roots.
- **Why**: `wreq` exposes certificate verification at client-build granularity, not
  per request.
- **Security posture**: Fails **closed**. Verification is always on by default and the
  ignored flag cannot silently disable it.
- **Impact**: Only affects engines configured against a host with a
  self-signed/expired cert (`verify: false`) or a private/internal CA.
- **Revisit when**: an engine must target an internal instance needing a custom CA or
  disabled verification.

## Command engines

- **Behavior**: SearXNG's `engine: command` type is **not supported**. It is
  deliberately absent from the safe `Processor` enum.
- **Why**: Command engines spawn OS processes with user-influenced input.
- **Security posture**: Hardening choice — no configuration causes zoeken to shell out.
- **Impact**: `settings.yml` entries using `engine: command` are unsupported.
- **Revisit when**: a sandboxed execution model is designed and explicitly approved.

## OnlineCurrency / OnlineDictionary / OnlineUrlSearch processors

- **Behavior**: The `Processor` enum includes these variants, but no specialized
  selection/scoring path exists yet. Engines that need them (e.g. `currency_convert`)
  are intentionally skipped.
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

- **Behavior**: `tracker_patterns.json` is a bundled ClearURLs provider snapshot
  (regenerate with `tools/fetch_tracker_patterns.py`). Runtime does not fetch ClearURLs
  on boot.
- **Refresh**:
  ```sh
  uv run --no-project --python 3.13 tools/fetch_tracker_patterns.py
  ```
  Writes `zoeken/zoeken-data/data/tracker_patterns.json`. Rebuild so the binary
  picks up the new snapshot (`zoeken-data` embed).
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
  resolver URL is applied instance-wide via plugin data at boot. There is no
  per-user DOI preference cookie field (unlike SearXNG).
- **Revisit when**: `oa_doi_rewrite` needs per-request resolver overrides.

## UI theme (SPA)

- **Behavior**: `/config` still exposes `themes` / `default_theme`, and the prefs
  cookie still stores `theme` for SearXNG cookie compatibility. The SPA has its
  own light/dark/system picker (`zoeken-client` theme helper) stored in
  `localStorage`, independent of the SearXNG theme cookie.

## Zoeken-only engines

- **Behavior**: `wikibooks` is a MediaWiki books engine shipped in Zoeken with no
  distinct SearXNG engine module. It is tracked as `zoeken_only` in
  `engines.json` / `engines.md`, not as an accidental orphan.
- **Revisit when**: upstream adds a matching module or the engine is retired.

## Stats / metrics Basic auth

- **Behavior**: `general.open_metrics` is the HTTP Basic password for `/metrics`,
  `/stats`, and `/stats/errors`. Empty hides `/metrics` (404) and leaves `/stats`
  open. The SPA `/stats` shell stays public and shows a configure-auth message on 401.
- **Why**: one existing knob gates both operator endpoints without a second secret.
- **Impact**: public instances should set `open_metrics`; browsers without Basic
  credentials see the SPA message instead of live stats JSON.

## No CORS middleware

- **Behavior**: CORS is not enabled. The SPA is same-origin with the API.
- **Security posture**: Avoids accidental open CORS.

## Image / favicon proxy redirects

- **Behavior**: Both fetchers use `redirect::Policy::none()`. Bodies are size-capped.
- **Residual**: DNS rebinding remains documented in `docs/security/audit.md`.
