# Plan: Architecture cleanup (before native search API)

Status: **complete** (exit checklist 100%; verified green after final gap-close)  
Execute **before** [`native-search-api.md`](native-search-api.md).

**Verification:** `cargo test --workspace --locked` + `zoeken-client` `bun run test` green after final gap-close. Server `ahmia_filter` wired in `run_search`; SPA has no `ahmiaFilter` (tracker remover stays client-side).

## Why this exists

Native typed search / msgpack assumes a thin, honest backend. A full audit found debt beyond “kill Lua”:

- Dual engine-health systems (in-process suspend vs storage circuit) that disagree
- Split error-category labels (metrics vs circuit vs serialize string-matching)
- Captcha/challenge detection still layered after recent improvements
- God-module `NetworkExecutor` (cache, circuit, vendor hacks)
- Duplicate query DTOs and result containers
- Lua plugin VM + server-side plugin hooks for features the SPA can own
- Engine factory in `zoeken-server`
- Dead AppState/config knobs, duplicated proxy/cache stacks
- Settings `ExtraMap` re-parsed at runtime; catalog registration footguns

**Direction:** thin backend (engines + aggregate + durable health), SPA owns former plugin UX, one vocabulary for errors/challenges/health, delete Lua entirely.

## Goal

1. One engine-health authority; one error-category enum; one challenge classifier.
2. `NetworkExecutor` carved into testable pieces; no vendor `if engine_name ==` in the composition root.
3. **Lua deleted**; **no server plugin pipeline**; former plugins → SPA client-features.
4. Engine registry lives with engines, not the HTTP crate.
5. Single query/result model where duplicates exist only as thin views.
6. `ResolvedSettings`; clear catalog merge policy; dead config removed.
7. Shared outbound/proxy and cache helpers; boot fail-fast; TLS honesty.
8. Ready for [`native-search-api.md`](native-search-api.md) without re-modeling plugin hooks.

**Non-goals:** native `/api/v1` / msgpack / ts-rs (next plan); rich-result UX redesign; splitting `zoeken-engines` into many crates; crawling; keeping SearXNG server-side plugin semantics for JSON API clients; rewriting Python tools into Bun.

## Hard decisions (locked)

| Decision | Choice |
| --- | --- |
| Lua | **Delete entirely** — no feature flag |
| Backend plugin hooks | **Delete** — no search-path plugin host |
| Former plugin behavior | **SPA client-features** |
| Prefs IDs (`calculator`, `hostnames`, …) | Client feature flags only; server does not execute |
| `/search?format=json` | **Mostly raw** aggregation; SPA transforms after fetch. **Exception:** `ahmia_filter` drops blacklisted onions server-side when Tor is enabled |
| Network instant answers | Stay **engines** |
| Pure-local answers | **SPA only** |
| Engine health | **Storage circuit is source of truth**; in-process suspension becomes a view of that or is removed |
| Error labels | **One `ErrorCategory`** for metrics, circuit, serialize, tests |
| Challenge detection | **One shared classifier**; engines only add vendor-specific extras |
| DDG captcha special-case | Encoded in **one** health policy place, not suspend=0 + circuit=900s |

### Compatibility implications (accept explicitly)

- External JSON/CSV/RSS clients do **not** get SearXNG-style server plugin URL rewrites.
- `search.suspended_times` semantics may change when health is unified — document in CHANGELOG; map old knobs onto circuit cooldowns where possible.
- Dead SearXNG UI paths (`static_path`, theme templates, etc.) stop being validated as if they worked.

---

## Success criteria (exit checklist)

### Health / captcha / errors
- [x] Single health gate on the execute path (no double-suspend) — storage circuit is SoT
- [x] `ErrorCategory` (or equivalent) shared by metrics, storage health, unresponsive serialize
- [x] No `translated_cause` substring matching on error messages — uses `ErrorCategory::user_label()`
- [x] One challenge classifier (`ChallengeKind` / `classify_challenge`); captcha variants still emitted
- [x] DDG (and similar) special-cases live in one policy table (`cooldown_for` / suspended_times)

### Executor / network
- [x] `executor.rs` no longer owns SoundCloud client-id scrape — engine `ensure_guest_client_id` + thin transport callback; `needs_client_id` prepare flag
- [x] Response cache + singleflight extracted (`FlightCache` / `outbound_cache`; autocomplete shares helper)
- [x] Favicon + image proxy share one safe outbound GET helper (`zoeken-favicons::safe_outbound` / `SafeOutboundTransport`)

### Plugins / Lua / SPA
- [x] Zero `mlua`; all `.lua` runtime files gone
- [x] `zoeken-plugins` removed from workspace; `zoeken-search` has no plugin hooks
- [x] SPA implements client-features table below (`ahmia_filter` stays server-side)
- [x] `/config` lists client features (prefs cookie shape OK) with no server execution
- [x] Local `zoeken-answerers` that duplicate SPA are removed

### Composition / settings / types
- [x] No `engine_from_settings` / `default_engines` in `zoeken-server` — live in `zoeken-engines::registry`
- [x] `Settings` → `ResolvedSettings` at boot; hot path uses resolved structs
- [x] Engine list merge/replace policy documented + tested (overlay/merge props)
- [x] `SafeSearch` / `TimeRange` owned by `zoeken-query` (re-exported); `SearchQueryView` is the engine view; `ResultContainer` is the aggregate thin wrapper over `EngineResults` fields + `unresponsive_engines` / `number_of_results` (`From<EngineResults>`). Full type flatten deferred to native API DTOs (serialize surface)
- [x] Dead `AppState.prefs` / `with_prefs` removed
- [x] Dead UI/brand knobs dropped (`static_path` / `templates_path` / `center_alignment` already gone; unused `brand.public_instances` / `wiki_url` / `new_issue_url` removed — unknown YAML keys still ignored)
- [x] Boot fails if `index.html` missing unless `APP_DISABLE_UI=1`
- [x] `RequestParams.verify` / outgoing verify: false rejected at network build (TLS always on)

### Quality bar
- [x] `cargo test --workspace` + client tests green
- [x] Compat JSON goldens = raw results (update if they encoded plugin rewrites)
- [x] `docs/compatibility/intentional-differences.md` + CHANGELOG updated

---

## Phase overview

```text
Phase 0  Finish captcha work + collapse challenge classifier
Phase 1  Unify engine health + ErrorCategory
Phase 2  Carve NetworkExecutor; engine-owned quirks; shared cache/proxy helpers
Phase 3  Engine registry extraction
Phase 4  SPA client-features (former plugins) — can overlap late Phase 3
Phase 5  Delete Lua + tear out server plugin pipeline
Phase 6  Drop duplicate answerers; consolidate query/result types
Phase 7  ResolvedSettings + catalog merge + dead config/AppState cleanup
Phase 8  Crate hygiene (botdetect/Axum), boot fail-fast, TLS honesty, processor stubs
Phase 9  Conformance/catalog bookkeeping + light SPA search.tsx split
         ── then ──►  native-search-api.md
```

Land as focused PRs. Prefer: SPA features (4) ready before Lua/plugins deletion (5) so the UI never goes dark.

---

## Former plugins → SPA map

| ID | SPA responsibility | Server data needed |
| --- | --- | --- |
| `calculator` | Eval + answer card | None |
| `unit_converter` | Convert + card | Units: generate into client from `zoeken-data` at build |
| `self_info` | IP / UA cards | UA = browser; IP = tiny `client_ip` on `/config` or search response |
| `time_zone` | TZ UI | None |
| `tor_check` | Optional SPA fetch or **drop** | Do not keep server plugin |
| `tracker_url_remover` | Strip trackers on URLs (SPA OK — display cleanup) | Tracker patterns → client generated data |
| `hostnames` | replace/remove + re-sort priority | Rules already on `/config` |
| `oa_doi_rewrite` | DOI link rewrite | `doi_resolvers` on `/config` |
| `ahmia_filter` | **Exception — server-side** (not SPA): drop blacklisted onions when Tor is enabled | Bundled `AhmiaBlacklist` in `zoeken-data`; applied in `zoeken-server` search path |
| `infinite_scroll` | UI prefs only | Flag only |

Only `ahmia_filter` is “must be server” (safety + large blacklist + JSON/API
consumers). Other former plugins stay SPA; see [`docs/client-features.md`](../client-features.md).

**Engines that stay backend:** `weather`, `currency`, `dictionary`, `translate`, Wikipedia/Wikidata infoboxes — they need HTTP.

---

## Explicitly deferred (honest, out of this plan’s exit bar)

| Item | Why deferred |
| --- | --- |
| Embed/`serde(flatten)` merge of `EngineResults` into `ResultContainer` | Would churn serialize + every struct literal; `From` + shared field bag is enough until native API DTOs |
| Full `search.tsx` decomposition (filters/header hooks) | Light `SearchResultList` (+ existing `VideoCard`) done; deeper split not needed before native API |
| Autogenerating `engine_entries()` from fixture dirs | `every_fixture_directory_with_json_is_registered` already guards drift; full codegen optional |
| Native search API / msgpack / ts-rs | Next plan: [`native-search-api.md`](native-search-api.md) |

---

## Phase 0 — Finish captcha + one challenge classifier

**Context:** Captcha improvements are almost finished. Close the loop so later health unification is not fighting three detectors.

### Work

1. Inventory all detection sites:
   - `zoeken-network` `map_response` (CF / recaptcha)
   - `zoeken-engines/.../util.rs` `looks_like_bot_wall`
   - Per-engine detectors (DDG, Bing, Google, Brave, Qwant, …)
2. Introduce a shared `ChallengeKind` (or reuse refined `EngineError` captcha variants) with one function:
   `classify_challenge(status, headers, body_prefix) -> Option<ChallengeKind>`.
3. Network owns **transport-generic** challenges (CF interstitial, generic recaptcha iframe, 429/403 patterns that are not engine-specific).
4. Engines only add **vendor** signals (e.g. DDG challenge form, Bing `div.captcha` with header text) that call into or extend the shared classifier — not a third independent string soup.
5. Resolve dead `NetworkError::Captcha`:
   - either construct it from the shared classifier, or
   - delete the variant and update executor/`status_error_mapping_prop` tests.
6. Property/unit tests: same body → same category through network map and engine parse paths where both run.

### Done when

- One obvious module owns challenge taxonomy.
- No dead captcha error variants.
- Existing engine captcha fixtures still pass.

---

## Phase 1 — Unify engine health + ErrorCategory

### Problem

- `SuspensionPolicy` / `search.suspended_times` suspends in-process.
- Storage circuit in `executor.rs` (`circuit_is_open`, `cooldown_for`, `record_health`) also gates execution.
- DDG captcha → suspend **0s** but circuit **hundreds of seconds**.
- Metrics categories (`too_many_requests`, `parse`, …) ≠ storage categories (`throttle`, `malformed`, …).
- `serialize.rs` `translated_cause` uses `"captcha" in message` string matching.

### Work

1. Define `ErrorCategory` in `zoeken-engine-core` (or tiny shared module).
2. `impl From<&EngineError> for ErrorCategory` (and from network errors via existing map).
3. **Health authority = storage circuit** (durable, multi-replica ready).
4. Carry typed category on `UnresponsiveCause`. Serialize user-facing labels from the enum.
5. Update `/metrics`, `/stats`, storage tests, selection tests, executor health tests.

### Done when

- One gate decides “skip this engine.”
- One label appears in metrics and `engine_health`.
- No substring matching in `translated_cause`.
- CHANGELOG notes behavior change for operators.

---

## Phase 2 — Carve NetworkExecutor; shared helpers

### Problem

`zoeken-server/src/executor.rs` ~1k LOC: response cache, singleflight, circuit health, SoundCloud client-id scrape, Startpage redirect hack, request build, metrics.

### Work

1. Extract modules (same crate is fine): response cache, engine health, request build.
2. **Engine-owned quirks:** SoundCloud bootstrap → engine; Startpage redirects → request params.
3. Shared `FlightCache<K,V>` used by executor cache **and** `zoeken-autocomplete`.
4. Shared `SafeOutboundGet` for favicons + image proxy; HMAC stays where it is.
5. `From<NetworkError> for EngineError` in one place.

### Done when

- `rg 'engine_name ==' executor.rs` clean (or only tests).
- Autocomplete and executor share cache helper.
- Favicon/image proxy share outbound GET helper.

---

## Phase 3 — Extract engine registry from server

Move `registry_from_settings` to `zoeken-engines`. Adding an engine must not edit `zoeken-server` factory match.

---

## Phase 4 — SPA client-features (former plugins)

`zoeken-client/src/lib/clientFeatures/` — tracker, hostnames, doi, calculator, units, crypto, timeZone, selfInfo. (`ahmia_filter` stays server-side.)

---

## Phase 5 — Delete Lua + server plugin pipeline

Delete Lua entirely. No optional Lua feature. No Rust plugin host.

---

## Phase 6 — Answerers + consolidate query/result types

Remove duplicate SPA answerers; one query model; `ResultContainer` as thin aggregate wrapper over `EngineResults`.

---

## Phase 7 — ResolvedSettings, catalog policy, dead config

Boot uses `ResolvedSettings`; dead knobs gone; merge/replace policy tested.

---

## Phase 8 — Crate hygiene, boot fail-fast, TLS, processors

Botdetect framework-free; UI fail-fast; TLS honest; unused processor stubs deleted.

---

## Phase 9 — Bookkeeping + light SPA structure

**Landed:** `every_fixture_directory_with_json_is_registered` drift guard; `SearchResultList` + `VideoCard` extracted from `search.tsx`.

---

## Testing plan (all phases)

| Area | Tests |
| --- | --- |
| Challenge classifier | Shared fixtures: CF / recaptcha / DDG / Bing / 429 |
| Health | Circuit-only gate; DDG policy; no double-suspend |
| ErrorCategory | Metrics label == storage category == serialize cause |
| Executor | Cache/singleflight unit tests; no name-string vendor branches |
| Registry | Moved `build_registry_*`; merge/replace |
| Client features | Per-transform Vitest; prefs gating |
| Search | No plugin hooks; raw aggregation snapshots |
| Settings | ResolvedSettings from example YAML |
| Boot | Missing `index.html` → error |
| CI | No `mlua`; workspace + client green |

---

## Docs / config touch list

```text
docs/plans/architecture-cleanup.md          # this file
docs/plugins.md → docs/client-features.md
docs/compatibility/intentional-differences.md
docs/settings.yml.example
default.config.yml
packaging/debian/zoeken.settings.yml
docs/deployment.md
CHANGELOG.md
README.md
docs/compatibility/engines.md               # regenerate via inventory tooling
```

---

## Suggested PR sequence

| PR | Phase | Notes |
| --- | --- | --- |
| 1–10 | 0–9 | Landed as focused PRs through this cleanup |
| — | — | Start [`native-search-api.md`](native-search-api.md) |

---

## Risk and rollback

| Risk | Mitigation |
| --- | --- |
| Health unify changes captcha cooldown behavior | Changelog; map old `suspended_times` into circuit policy; integration test DDG |
| API clients expected server URL rewriting | Document SPA-only; intentional |
| Hostname priority only affects SPA order | Accept |
| Ahmia filter server-side (exception to thin JSON) | Privacy: blacklist applied before results leave the instance |
| Custom `.lua` operators | Unsupported; no migration — loud CHANGELOG |
| ErrorCategory rename breaks dashboards | Provide mapping table in CHANGELOG; update Grafana examples if any |

---

## Explicitly out of scope / fine as-is

| Leave alone | Why |
| --- | --- |
| `zoeken-query` / `zoeken-prefs` layering | Clear; good prop tests |
| Metrics facade post-cache-removal | Clean adapter |
| Conformance fixture **format** | Solid; only registry duplication hurts |
| Flat `i18n.ts` | Appropriate for chrome strings |
| Generic + hand-written engines | Justified coexistence |
| Python `tools/` + Bun client | Documented split; optional `make check-compat` wrapper only |
| Rich result UX / sitelinks | Product work, not this cleanup |
| `zoeken-engines` multi-crate split | Later, if ever |
| Storage/metrics WIP entanglement | Finish separately first |
| Proxy DNS rebinding hardening | Security follow-up (track separately) |

---

## Dependency on native API plan

```text
architecture-cleanup.md (this audit)  ──completes──►  native-search-api.md
         │
         ├── one health + ErrorCategory + challenge story
         ├── thin executor / registry / settings
         ├── no Lua, no server plugins
         ├── SPA owns transforms + local answers
         └── consolidated query/result types
                  │
                  ▼
         native DTOs / codegen / msgpack
         model results only — not plugin hook output
```

Do **not** start native API while dual health, Lua, or server plugins still exist.
