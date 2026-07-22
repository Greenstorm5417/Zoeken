# Plan: Native typed search API (SearXNG as compat layer)

Status: **done** (2026-07-22)  
**Prerequisite:** finish [`architecture-cleanup.md`](architecture-cleanup.md) first (captcha/health/ErrorCategory unify, executor carve-up, registry extraction, **delete Lua + server plugins**, SPA client-features, ResolvedSettings, type consolidation).  
Related: `docs/compatibility/targets.md`, `zoeken/zoeken-server/src/serialize.rs`, `zoeken-client/src/lib/api.ts`

## Checklist

- [x] Phase 0 — Wire DTOs + mapper + golden/unit fixtures
- [x] Phase 1 — `POST /api/v1/search` JSON + TypeScript codegen + SPA cutover
- [x] Phase 2 — MessagePack Accept / `?format=msgpack` (SPA remains JSON by default)
- [x] Phase 3 — CI drift check, docs, CHANGELOG

**Deferred / optional (not blocking “done”):** SPA defaulting to MessagePack in production (needs benchmarks); `@msgpack/msgpack` client dep unused while SPA stays JSON; dedicated `kind=video|product|news`; native `/api/v1/config`.
## Goal

Keep SearXNG `format=json|csv|rss` as a frozen-ish **compat layer**. Give the SPA a first-class **Zoeken native** search response that:

1. Uses tagged, typed result variants (no flat bag-of-optional-fields).
2. Preserves every field the SPA can already use, plus fields we currently drop or under-emit.
3. Is generated into TypeScript so frontend/backend stay in sync.
4. Can be transported as MessagePack after the schema stabilizes (JSON native first).

## Non-goals

- Replacing or breaking `/search?format=json|csv|rss` for external clients.
- Msgpack for `/config`, `/preferences`, `/autocompleter`, `/bangs` in v1 (stay JSON).
- Jinja / SearXNG HTML theme parity.
- Changing engine internals or crawling architecture.
- Making the native schema SearXNG-compatible (it must not be).

## Architecture

```text
                    ResultContainer (domain)
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
     serialize.rs (compat)              native wire (v1)
     format=json|csv|rss                POST /api/v1/search
     bag-of-fields JSON                 typed DTO → JSON | msgpack
              │                               │
              ▼                               ▼
     external clients                    zoeken-client (generated TS)
```

**Why a new path (`/api/v1/search`) instead of `format=zoeken` on `/search`:**

- Leaves SearXNG `/search` semantics alone (method, form params, format enum, OpenSearch).
- Clear versioning (`/api/v1/…`).
- Avoids accidental third-party clients latching onto an unstable native shape via the legacy route.

Legacy `/search` keeps accepting the same form params. Native route reuses the same search executor/pipeline; only the response edge differs.

---

## Phases

### Phase 0 — Wire DTO + sample fixtures (no SPA cutover)

1. Add crate or module for wire types (see [Schema](#sample-schema-v1)).
2. Implement `NativeSearchResponse::from_container(...)` with proxy URL rewriting (same HMAC rules as `format_json_for_query_with_proxies`).
3. Golden fixtures: domain container → native JSON (and later msgpack bytes).
4. Field-parity checklist vs current SPA + currently dropped fields (see [Parity matrix](#parity-matrix-do-not-lose-functionality)).

### Phase 1 — Native JSON + TypeScript codegen; SPA switches

1. Expose `POST /api/v1/search` (and GET if we want parity with legacy; POST-only is fine for SPA).
2. Content type: `application/json`.
3. Generate `zoeken-client/src/lib/generated/native.ts` from wire DTOs.
4. Point SPA `search()` at the native endpoint; delete reliance on flat `SearchResult` for rendering.
5. Keep hand-written helpers for prefs/config/autocomplete.

### Phase 2 — MessagePack transport

1. Accept `Accept: application/msgpack` or `?format=msgpack` on `/api/v1/search`.
2. Rust: `rmp-serde`. Client: `@msgpack/msgpack`.
3. Keep `Accept: application/json` (or `?format=json`) for DevTools / debugging forever.
4. Default SPA to msgpack in production builds only if benchmarks justify it; JSON remains supported.

### Phase 3 — Harden

1. CI check: generated TS is committed and up to date.
2. Compat tests still cover SearXNG JSON unchanged.
3. Optional: native `/api/v1/config` later — **not** required for this plan.

---

## Sample schema (v1)

Naming: snake_case on the wire (serde default). Tagged enums use `#[serde(tag = "kind")]` for results and existing `#[serde(tag = "type")]` for interactive answers.

### Top-level response

```json
{
  "schema_version": 1,
  "query": "rust lang",
  "number_of_results": 42,
  "results": [ /* NativeResult */ ],
  "answers": [ /* NativeAnswer */ ],
  "corrections": [ /* NativeCorrection */ ],
  "suggestions": [ /* NativeSuggestion */ ],
  "infoboxes": [ /* NativeInfobox */ ],
  "unresponsive_engines": [
    { "engine": "google", "cause": "timeout" }
  ],
  "engine_data": {}
}
```

Notes vs SearXNG JSON today:

| Field | Compat JSON today | Native v1 |
| --- | --- | --- |
| `results` | `object[]` flat bag | tagged union |
| `answers` | objects (sometimes via bag) | structured `NativeAnswer` |
| `corrections` | `string[]` (loses `url`) | full objects |
| `suggestions` | `string[]` (loses `engine`) | full objects |
| `unresponsive_engines` | `[name, cause][]` | `{ engine, cause }[]` |
| `number_of_results` | optional / inconsistent | always present (`0` if unknown) |
| `schema_version` | n/a | required |

### `NativeResult` (tagged)

```json
{
  "kind": "main",
  "url": "https://www.rust-lang.org/",
  "title": "Rust Programming Language",
  "content": "A language empowering everyone…",
  "engine": "duckduckgo",
  "engines": ["duckduckgo", "brave"],
  "category": "general",
  "score": 1.2,
  "positions": [1, 3],
  "priority": "",
  "thumbnail": "/image_proxy?url=…&h=…",
  "iframe_src": "",
  "favicon": "/favicon_proxy?authority=www.rust-lang.org&h=…",
  "pretty_url": "www.rust-lang.org",
  "published_date": null
}
```

```json
{
  "kind": "image",
  "url": "https://example.test/photo",
  "title": "Crab",
  "content": "",
  "engine": "bing_images",
  "engines": ["bing_images"],
  "score": 0.9,
  "positions": [1],
  "priority": "",
  "img_src": "/image_proxy?url=…&h=…",
  "thumbnail_src": "/image_proxy?url=…&h=…",
  "resolution": "1920x1080",
  "img_format": "jpeg",
  "source": "example.test",
  "filesize": "240 KB"
}
```

```json
{
  "kind": "paper",
  "url": "https://arxiv.org/abs/1234.5678",
  "title": "Attention Is All You Need",
  "content": "Abstract…",
  "engine": "arxiv",
  "engines": ["arxiv"],
  "score": 1.0,
  "positions": [1],
  "priority": "",
  "authors": ["Ashish Vaswani", "Noam Shazeer"],
  "doi": "10.48550/arXiv.1234.5678",
  "journal": "",
  "published_date": "2017-06-12",
  "publisher": "",
  "editor": "",
  "volume": "",
  "pages": "",
  "number": "",
  "type": "preprint",
  "tags": ["transformers"],
  "issn": [],
  "isbn": [],
  "pdf_url": "https://arxiv.org/pdf/1234.5678",
  "html_url": "",
  "comments": "15 pages, 5 figures"
}
```

```json
{
  "kind": "code",
  "url": "https://github.com/rust-lang/rust",
  "title": "fn main()",
  "content": "",
  "engine": "github_code",
  "engines": ["github_code"],
  "score": 0.8,
  "positions": [1],
  "priority": "",
  "repository": "rust-lang/rust",
  "filename": "main.rs",
  "code_language": "rust",
  "codelines": [[1, "fn main() {"], [2, "    println!(\"hi\");"], [3, "}"]],
  "hl_lines": [1]
}
```

```json
{
  "kind": "file",
  "url": "https://thepiratebay.org/…",
  "title": "Some.Torrent",
  "content": "",
  "engine": "piratebay",
  "engines": ["piratebay"],
  "score": 0.7,
  "positions": [1],
  "priority": "",
  "filename": "Some.Torrent",
  "size": "1.2 GiB",
  "time": "2024-01-02",
  "mimetype": "application/x-bittorrent",
  "abstract": "",
  "author": "uploader",
  "embedded": "",
  "mtype": "",
  "subtype": "",
  "filesize": "1.2 GiB",
  "seed": 120,
  "leech": 4,
  "magnetlink": "magnet:?xt=urn:btih:…"
}
```

```json
{
  "kind": "key_value",
  "url": "",
  "title": "Package info",
  "content": "",
  "engine": "crates",
  "engines": ["crates"],
  "score": 0.5,
  "positions": [1],
  "priority": "",
  "caption": "Metadata",
  "key_title": "Field",
  "value_title": "Value",
  "kvmap": [
    ["license", "MIT OR Apache-2.0"],
    ["downloads", "1_000_000"]
  ]
}
```

`kvmap` is an **array of pairs** (matches Rust `Vec<(String, String)>`), not a JSON object — preserves duplicate keys and order. SPA adapts `KeyValueResult` accordingly.

Video hits stay `kind: "main"` with `iframe_src` + `thumbnail` set (same as today). Optional later: `kind: "video"` without breaking v1 clients if we bump `schema_version`.

### Answers

```json
{
  "answer": "42",
  "url": null,
  "engine": "calculator",
  "interactive": {
    "type": "calculator",
    "expression": "6*7",
    "result": 42.0
  }
}
```

`interactive` reuses the existing tagged enum variants already mirrored in `api.ts`:

- `unit`, `currency`, `calculator`, `weather`, `self_info`, `crypto`, `translate`, `dictionary`, `wikipedia`

No behavioral change — only a stable generated TS union.

### Infobox / correction / suggestion

```json
{
  "infobox": "Rust",
  "id": "Q575650",
  "content": "general-purpose programming language",
  "img_src": "/image_proxy?url=…&h=…",
  "urls": [{ "title": "Wikipedia", "url": "https://en.wikipedia.org/wiki/Rust_(programming_language)" }],
  "attributes": [
    { "label": "Paradigm", "value": "multi-paradigm", "image": null }
  ],
  "related_topics": ["Cargo (package manager)", "LLVM"],
  "engine": "wikidata"
}
```

```json
{ "correction": "rust lang", "url": null, "engine": "duckduckgo" }
```

```json
{ "suggestion": "rust programming language", "engine": "brave" }
```

### Request (native)

Same search knobs as today’s SPA POST body, as JSON (not form-urlencoded) for the native route:

```json
{
  "q": "rust lang",
  "pageno": 1,
  "language": "en-US",
  "safesearch": 0,
  "categories": "general",
  "time_range": null,
  "engines": null
}
```

Auth/cookies/prefs resolution stays identical to `/search` (read prefs cookie, locks, etc.).

---

## Parity matrix (do not lose functionality)

Every row must be covered by native fixtures + SPA rendering before cutover.

| Capability | Source today | Native field(s) | SPA consumer |
| --- | --- | --- | --- |
| Default web hit | `Main` + flat JSON | `kind=main` | `ResultItem` |
| Favicon | serialize proxy inject | `favicon` | `ResultItem` |
| Thumbnail (general/video) | `Main.thumbnail` | `thumbnail` | **enable** on `ResultItem` / `VideoCard` |
| Video iframe | `iframe_src` | `iframe_src` | `VideoCard` |
| Image grid + lightbox | `Image` | `kind=image` | images layout + `ImageLightbox` |
| Map markers | URL lat/lon on results | `kind=main` urls unchanged | `MapCanvas` / `MapResult` |
| Torrents | `File` (partial JSON) | `kind=file` **incl. `time`** | `TorrentResult` (extend) |
| Papers | `Paper` (partial JSON) | `kind=paper` **full citation fields** | `PaperResult` (extend) |
| Code + highlights | `Code` (`hl_lines` unused in UI) | `kind=code` + `hl_lines` | `CodeResult` (highlight) |
| Key-value | `KeyValue` (captions dropped in UI) | `kind=key_value` + captions | `KeyValueResult` |
| Shopping thin card | category heuristic | `kind=main` + `category=shopping` (v1) | `ProductResult` |
| Instant answers | `Answer.interactive` | same | `InstantAnswerCard` |
| Infobox sidebar | `Infobox` | same (+ attributes/topics) | `InfoboxCard` |
| Did-you-mean | `corrections` strings | full `NativeCorrection` | search page |
| Related searches | `suggestions` strings | full `NativeSuggestion` | chips |
| Unresponsive engines | tuples | objects | panel |
| Image/favicon proxy | HMAC rewrite | same rewrite on native serializer | unchanged routes |

Explicit v1 **non**-features (document, don’t pretend):

- Sitelinks / rich snippets / recipes / sports — no schema yet; add in v2+ as new `kind`s.
- Dedicated `kind=product` / `kind=video` / `kind=news` — deferred unless data is already available without new engines.

---

## Rust changes

| Area | Change |
| --- | --- |
| New module/crate | Prefer `zoeken/zoeken-server/src/native/` first; extract `zoeken-api` crate only if it gets large |
| Wire types | `NativeSearchResponse`, `NativeResult` enum, thin newtypes where domain ≠ wire |
| Mapping | `From<&ResultContainer>` + proxy pass (secret, image_proxy, favicon_proxy) |
| Route | `POST /api/v1/search` in `zoeken-server` router |
| Compat | `serialize.rs` untouched except shared helpers for proxy URL signing (dedupe `signed_proxy_url`) |
| Deps | Phase 1: none new. Phase 2: `rmp-serde`. Codegen: `ts-rs` **or** `typeshare` on wire types only |
| Tests | Golden JSON per result kind; proxy rewrite tests; schema_version bump test |

### Suggested wire enum (Rust sketch)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeResult {
    Main { /* … */ },
    Image { /* … */ },
    Paper { /* … */ },
    Code { /* … */ },
    File { /* … */ },
    KeyValue { /* … */ },
}
```

Do **not** `#[derive(TS)]` on internal `zoeken-results::Result_` — generate only wire DTOs.

`Template` HTML names (`paper.html`) are **omitted** from native results; `kind` replaces them. Compat JSON keeps `template`.

---

## TypeScript / SPA changes

| Area | Change |
| --- | --- |
| Generated file | `zoeken-client/src/lib/generated/native.ts` (committed) |
| API client | `searchNative()` → `POST /api/v1/search`, decode JSON (later msgpack) |
| Rendering | `search.tsx` / `ResultTemplates.tsx` switch on `result.kind` instead of `template` string / category heuristics |
| Remove / shrink | Flat `SearchResult` type after cutover; keep thin wrappers if useful |
| Tests | Vitest fixtures from golden JSON; one test per `kind` |
| Deps (phase 2) | `@msgpack/msgpack` |

Migration strategy inside the SPA:

1. Add adapters `nativeToViewModel()` so UI can migrate component-by-component.
2. Flip `search()` default to native when all templates pass.
3. Delete adapters once components take native types directly.

---

## Build system changes

### Codegen pipeline

Add a small Rust binary or `cargo test`/`xtask` that writes TS:

```text
cargo run -p zoeken-server --bin export-native-ts
  → zoeken-client/src/lib/generated/native.ts
```

Or `ts-rs` export directory pointed at that path.

**Committed artifacts:** yes (CI fails if diff). Avoids requiring Rust in the client-only CI job for a pure `bun run build` — but see CI below.

### `Makefile`

```make
# new targets
native-types:     ## regenerate SPA types from Rust wire DTOs
	cargo run --locked -p zoeken-server --bin export-native-ts

check-native-types: native-types
	git diff --exit-code -- zoeken-client/src/lib/generated/native.ts

client:           ## unchanged outputs; may depend on check in CI only
	cd zoeken-client && bun install && bun run build
	# …
```

`make build` does **not** need to regenerate types every time if committed; CI enforces freshness.

### `zoeken-client/package.json`

```json
{
  "scripts": {
    "build": "vite build",
    "test": "vitest run",
    "check:types": "tsc --noEmit",
    "check:generated": "node ./scripts/assert-generated-native.mjs"
  }
}
```

Optional `assert-generated-native.mjs`: verifies file exists + header stamp (`// @generated by export-native-ts — do not edit`).

Phase 2:

```bash
bun add @msgpack/msgpack
```

### CI (`.github/workflows/ci.yml`)

**Backend job** (has Rust):

1. After tests: `cargo run -p zoeken-server --bin export-native-ts`
2. `git diff --exit-code zoeken-client/src/lib/generated/native.ts`

**Client job** (Bun only):

1. Unchanged lint/test/build
2. Add `bun run check:types` once generated types are part of `tsc` project
3. No Rust install required if generated file is committed

**Release / Docker / deb:** no packaging layout change — native API is in the binary; SPA assets still land in `zoeken/zoeken-server/assets`.

### Vite / proxy

`zoeken-client/vite.config.ts` already proxies `/search`. Add:

```ts
"/api": "http://127.0.0.1:8888"
```

### Docs / compatibility targets

Update `docs/compatibility/targets.md`:

- SPA consumes **native** `/api/v1/search`.
- SearXNG JSON remains the **external** API compatibility target.
- Add `docs/compatibility/intentional-differences.md` note: native schema is Zoeken-only.

Add route row in `docs/compatibility/routes.md` as `rust-only` (`/api/v1/search`).

---

## Testing plan

| Layer | What |
| --- | --- |
| Rust unit | Each `Result_` variant → native JSON golden |
| Rust unit | Proxy signing on `img_src` / `thumbnail` / `favicon` / infobox images |
| Rust unit | Compat `format=json` golden **unchanged** (regression lock) |
| HTTP | `POST /api/v1/search` status, content-type, schema_version |
| HTTP | Unknown/invalid body → 4xx stable shape |
| Client | Decode fixture per kind; render smoke tests for templates |
| CI | Generated TS drift check |
| Manual | Images, videos, maps, torrents, papers, answers, infobox, DYM |

---

## Rollout / risk

1. Ship `/api/v1/search` behind the SPA only (no docs push as stable until phase 1 done).
2. Feature-flag optional: `APP_NATIVE_SEARCH=1` or SPA env `VITE_USE_NATIVE_SEARCH` for one release.
3. Rollback = SPA points back at `/search?format=json` (keep old client types until flag removed).
4. Msgpack only after JSON native is the default for a release.

Risks:

| Risk | Mitigation |
| --- | --- |
| Dual serializers drift | Shared mapping from `Result_` → intermediate view, or native→compat only for tests — prefer dual `from_container` with shared field helpers |
| ts-rs output ugly/unusable | Hand-tune wire types; avoid generating domain junk |
| Msgpack harder to debug | Always support JSON on same route |
| Scope creep into rich-result redesign | v1 = parity + restore dropped fields only |

---

## File touch list (expected)

```text
docs/plans/native-search-api.md          # this plan
docs/compatibility/targets.md
docs/compatibility/routes.md
docs/compatibility/intentional-differences.md
CHANGELOG.md                             # when implementing

zoeken/zoeken-server/src/native/mod.rs   # wire types + mapper + export bin
zoeken/zoeken-server/src/native/schema.rs
zoeken/zoeken-server/src/native/serialize.rs
zoeken/zoeken-server/src/lib.rs          # router mount
zoeken/zoeken-server/Cargo.toml          # ts-rs / rmp-serde features
zoeken/zoeken-server/tests/native_*.rs   # or inline tests

zoeken-client/src/lib/generated/native.ts
zoeken-client/src/lib/api.ts             # searchNative + cutover
zoeken-client/src/routes/search.tsx
zoeken-client/src/components/ResultTemplates.tsx
zoeken-client/vite.config.ts
zoeken-client/package.json
zoeken-client/scripts/assert-generated-native.mjs

Makefile
.github/workflows/ci.yml
```

---

## Decision summary

| Question | Decision |
| --- | --- |
| Keep SearXNG JSON? | Yes, compat layer |
| SPA transport v1 | Native **JSON** on `/api/v1/search` |
| Msgpack | Phase 2, optional default |
| Type sync | Generate TS from **wire DTOs** only |
| Form vs JSON request | JSON body for native route |
| Versioning | `schema_version: 1` + URL `/api/v1/` |
| Dropped fields restored? | Yes (`time`, paper citations, captions, `hl_lines`, full corrections/suggestions) |

## Implementation order (when executing)

1. Wire DTOs + mapper + goldens (parity matrix green).
2. HTTP route + proxy rewrite.
3. `export-native-ts` + Makefile/CI drift check.
4. SPA decode + render by `kind`.
5. Cut over SPA; keep rollback path one release.
6. Msgpack Accept negotiation.
7. Docs + CHANGELOG.
