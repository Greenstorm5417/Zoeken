# Changelog

All notable changes to Zoeken are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- SPA client-features and infinite scroll honor preference cookies (and
  settings `plugins.*.active` defaults) instead of ignoring toggles; plugin id
  is consistently `infinite_scroll`.

### Changed

- Engine health: storage circuit is the sole gate; `search.suspended_times` /
  ban knobs feed circuit cooldown policy only (no in-process suspend).
  DuckDuckGo captcha does not open a circuit (no durable IP ban).
- `search.engine_list_mode`: `replace` (default) vs `merge` for non-empty
  `engines:` lists — see settings examples and registry docs.
- Former Lua/server plugins → SPA client-features; JSON/CSV/RSS stay raw
  aggregation (except server-side `ahmia_filter` when Tor proxy is enabled).
- Dropped unused brand settings: `public_instances`, `wiki_url`,
  `new_issue_url` (still ignored if present in overlay YAML).

### Removed

- Lua plugin host, `zoeken-plugins` / `zoeken-answerers`, and unused workspace
  `fasteval` dependency.

## [1.2.1] - 2026-07-21

### Fixed

- DuckDuckGo: drop Firefox-like `Accept` override that conflicted with the
  Chrome client fingerprint and triggered CAPTCHA challenges
- Bing: treat missing `#b_results` shells as CAPTCHA; surface suspended
  engines in the unresponsive list so they no longer vanish silently
- SPA: remove operator chips, OpenSearch / RSS / CSV clutter from home and
  results pages

## [1.2.0] - 2026-07-21

### Added

- Interactive instant-answer cards (weather, crypto/hash, units, currency,
  calculator, translate, dictionary, Wikipedia, self-info)
- Rich Brave autocomplete (subtext/image suggestions) as the default backend
- Extra category tabs, Bing Images, map canvas, `/stats` auth gate, `/bangs`
- Precompiled `zoeken-data` assets and `tools/sync_versions.py` CI workflow

### Fixed

- Version sync now keeps Docker `ARG VERSION` / compose defaults aligned with
  Cargo.toml (avoids shipping images labeled as the previous release)

## [1.1.0] - 2026-07-20

### Added

- `avalw` generic JSON engine ported (the last unported upstream engine):
  the compatibility scorecard now reads missing=0, ported=249. Marked
  `inactive` like upstream (disabled by default there too) — the app's
  browser-profile HTTP client sees intermittent zero-result/error responses
  from it in testing even though the API is reachable with a plain client.
  Like the other ~200 catalog-only engines in `generic_catalog.json`, it
  isn't in the default registry; add an `engines:` entry named `avalw` in
  `settings.yml` to register it.
- Currency conversion instant answer (`100 usd to eur`) via the ECB daily
  reference rates, with symbol/name aliases and EUR cross-rates.
- Dictionary definitions (`define serendipity`) via Wiktionary.
- Translation (`translate hello to spanish`) via MyMemory, 28 languages.
- Date/time-math answerer: `days until christmas` / `days until 2026-12-25`
  and fixed-offset timezone conversion (`3pm est in cet`).
- News is a working tab by default (Swisscows news enabled).
- Time-range filter (Any time / day / week / month / year) in the results
  header, on desktop and mobile.
- Bang autocomplete popover: typing `!` suggests engine shortcuts from the
  instance config and completes them.
- Rich result templates: torrents (size, seeders/leechers, magnet button),
  papers (authors, journal, DOI, PDF link), code (repository + highlighted
  line snippet), key-value/table records, and map cards with coordinates and
  OpenStreetMap / Google Maps links.
- Image lightbox: image results open a full-screen viewer with
  resolution/format/size/source metadata instead of leaving the page.
- Infinite scroll (opt-in via the `infinite_scroll` plugin preference) with a
  "Load more" fallback; classic numbered pagination otherwise.
- Manual theme toggle (System / Light / Dark), persisted per-browser.
- Copyable settings link (`#prefs=…`) to sync preferences across
  browsers/devices without an account.
- UI translations for the search chrome (English, Spanish, French, German,
  Dutch), driven by the language selector.
- Search-operators help panel in preferences.
- Calculator instant answerer (Lua plugin, on by default): evaluates
  arithmetic expressions (`2+3*4`, `sqrt(9)`, `2^10`, `6 × 7`, functions,
  constants) locally.
- Unit-converter instant answerer (Lua plugin, on by default): `10 km to
  miles`, `72 f in c`, `1 gib to mib`, and natural phrasing like `how many
  cups in a gallon` (implicit "1", reversed unit order), across length,
  mass, temperature, volume, speed, data, time, and area. Tolerates trailing
  punctuation/filler ("10 km to miles?", "... please").
- Weather engine backed by wttr.in, gated to `weather <place>` /
  `<place> weather` queries (no request otherwise).
- Styled instant-answer cards in the SPA (calculator, converter, weather,
  statistics) with icons and emphasized results.

### Fixed

- Wikidata infoboxes are reliable again. The public WDQS SPARQL endpoint
  rate-limits with HTTP 429, and because engine requests raise on HTTP error by
  default, a single 429 was classified as `TooManyRequests` and **suspended
  Wikidata for a full hour** — so it silently vanished from results. The engine
  now handles WDQS status codes itself (429/5xx → a short transient back-off,
  not an hour), and the executor caches successful responses from idempotent
  instant-answer/infobox engines (Wikidata, currency, dictionary) for 5 minutes,
  so repeated popular queries no longer re-hit — and re-trip — the rate limit.
- General searches now aggregate across more engines: Mojeek and Dogpile are
  enabled by default, and explicitly requesting a disabled engine
  (`engines=`/`!bang`) now works (SearXNG semantics). Unresponsive-engine
  reasons are honest ("blocked by CAPTCHA", "rate limited") instead of
  "unexpected crash".
- The misleading always-on "Cached" link is now a small inline "archive" link
  next to the engine names (it was always a Wayback Machine link, not a local
  cache).
- Rate limiter no longer 429s normal browsing: only `/search` and
  `/autocompleter` are charged against the token bucket (a results page fires
  dozens of favicon/image subresource requests that previously drained it),
  and the header heuristics now accept the SPA's own JSON fetches
  (`Accept: application/json`, `Sec-Fetch-Mode: cors`/`same-origin`).
  Block/pass IP lists still apply to every route.
- Mobile layout: language and safe-search filters are now reachable on small
  screens, the fixed nav no longer overlaps the search bar, and layout
  alignment hacks were removed.

- Reloading `/preferences` in the browser now serves the SPA document instead of
  raw JSON (content negotiation on the `Accept` header; API clients still get JSON).
- `/image_proxy` now follows redirects safely (each hop re-validated against the
  SSRF policy), sends a browser-like `Accept` header, and reuses the pooled
  browser-emulating `image_proxy` network client instead of building a fresh
  TLS client per request.
- Result aggregation merges the same page across engines regardless of
  `http`/`https` scheme and ignores tracking query parameters (`utm_*`,
  `gclid`, `fbclid`, …).
- Bing now detects its CAPTCHA/Turnstile challenge page (same treatment as
  DuckDuckGo already had) and reports "blocked by CAPTCHA" instead of
  silently returning zero results with no explanation.
- When the same page is merged from multiple engines, a low-quality
  description (empty, or a stale "Redirecting to ..." stub from an engine's
  own index) no longer wins the canonical title/content over a real
  description from another contributing engine.
- The SPA's own `/search` fetch is a `POST` (matching `server.method`'s
  documented default) instead of always `GET`; `/search` still accepts both
  methods for API clients and shareable/bookmarkable search URLs are
  unaffected since those are owned by the client-side router, not the fetch.
- The Wayback Machine "archive" link next to result engine names is off by
  default (`ui.cache_url` now defaults to `""`); set it in `settings.yml` to
  bring it back.
- DuckDuckGo no longer suspends for 24h on its own CAPTCHA. DDG's html
  endpoint has no client session — its CAPTCHA is a per-request heuristic,
  not a durable IP ban (upstream SearXNG suspends it for 0s for the same
  reason). The generic 24h captcha suspension was hiding the engine for a
  full day after a single transient challenge; it now retries on the next
  query, same as upstream.
- Calculator and unit conversion had two implementations each (a native Rust
  answerer plus a dormant, functionally-identical Lua plugin default-off
  "for compatibility") — the Lua ones were visible as togglable plugins in
  Preferences but toggling them did nothing, since the always-on native
  answerer wasn't gated by that preference. The native Rust answerers are
  removed; the Lua plugins are the sole implementation now, on by default,
  so the visible toggle is the real one.

### Changed

- The GET/POST "search method" option was removed from the preferences UI (the
  API still accepts `method` for SearXNG compatibility); the SPA always talks
  to the backend the same way, and engines choose their own upstream HTTP
  method internally.
- `search.favicon_resolver` now defaults to `duckduckgo`, so result favicons are
  emitted (and proxied through `/favicon_proxy`) out of the box.
- `server.image_proxy` now defaults to `true` for privacy-by-default image loading.
- Proxied images are served with `Cache-Control: public, max-age=86400`.
- Favicon resolution reuses one pooled HTTP client per resolver.
- Outgoing requests keep a stable browser identity (TLS + header profile) per
  upstream host instead of rotating profiles per request; source addresses and
  proxies still rotate. `Accept-Language` is sent as a browser-style q-graded
  list (e.g. `de-DE,de;q=0.9,en;q=0.8`) instead of a bare locale tag.

## [1.0.0] - 2026-07-16

First stable release: SearXNG-compatible metasearch (Rust backend + React SPA),
with Debian packages, systemd unit, and multi-arch Docker images on GHCR.

### Added

- `zoeken-server` HTTP API compatible with SearXNG search/config/stats/metrics routes
- React SPA (`zoeken-client`) served from on-disk assets
- ~248 ported engines; intentional skips documented in `docs/compatibility/`
- Lua plugin host and bundled plugins
- Rate limiting, secret-key gating, image/favicon proxy SSRF controls
- Debian packaging (`amd64` / `arm64`) with `zoeken.service`
- Multi-arch container image: `ghcr.io/greenstorm5417/zoeken`
- Deployment, security audit, and plugin docs

### Fixed

- `deployment.trusted_proxies` is unioned into the limiter trusted list (no longer
  ignored when bundled `limiter.toml` already lists loopback)
- Debian package ships `/etc/zoeken/limiter.toml` and systemd
  `ReadWritePaths=/var/lib/zoeken`
- Example compose secret rejected as weak; SPA Vite devtools plugin is
  development-only; CI builds/lints/tests the client on every PR

### Changed

- Ship a full commented YAML settings reference (`/etc/zoeken/settings.yml`,
  `docs/settings.yml.example`) covering every typed option; Debian/Docker also
  install Lua plugins under `/usr/share/zoeken/plugins`

### Compatibility notes

- Full SearXNG HTML/Jinja theme parity is **not** a goal; use the SPA + JSON APIs
- Command engines and several API-key / bespoke engines remain intentionally unsupported
- See `docs/compatibility/intentional-differences.md` and `docs/security/audit.md`

[1.2.1]: https://github.com/Greenstorm5417/zoeken/releases/tag/v1.2.1
[1.2.0]: https://github.com/Greenstorm5417/zoeken/releases/tag/v1.2.0
[1.1.0]: https://github.com/Greenstorm5417/zoeken/releases/tag/v1.1.0
[1.0.0]: https://github.com/Greenstorm5417/zoeken/releases/tag/v1.0.0
